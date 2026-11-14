use super::*;
#[cfg(test)]
#[path = "layout_foundation_tests.rs"]
mod foundation_tests;
use cosmic::cctk::sctk::reexports::protocols::xdg::shell::client::xdg_positioner::{
    Anchor as PopupAnchor, Gravity,
};
use cosmic::iced::platform_specific::runtime::wayland::popup::{SctkPopupSettings, SctkPositioner};
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    destroy_layer_surface, set_layer,
};
use cosmic::iced::platform_specific::shell::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::widget::{opaque, pin, stack};
use cosmic::iced::{Point, keyboard, mouse};
use std::collections::BTreeMap;

/// A choice in a widget's right-click menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuPick {
    Settings,
    Arrange,
    Hide,
}

pub(super) const MENU_WIDTH: u32 = 220;
const MENU_ROW: u32 = 36;
const MENU_PADDING: u32 = 6;

/// The Arrange toolbar's width, and the narrowest that keeps it on one line.
const TOOLBAR_WIDTH: f32 = 960.;
const TOOLBAR_ONE_LINE: f32 = 900.;
/// The round buttons on a card's top edge while arranging.
const CARD_CONTROL: f32 = 22.;
const CARD_CONTROL_GAP: f32 = 6.;
/// The name tab on an arranged card's top edge, and the resize handle on its
/// lower-right corner.
const CHROME_TAB: f32 = 22.;
const CHROME_GRIP: f32 = 22.;
/// How far the editing chrome reaches past a card's edge: half the gap kept
/// between widgets, so two neighbors' chrome never meets. The rest of it
/// lies on the card's padding.
const CHROME_OUTSET: f32 = crate::layout::WIDGET_GAP / 2.;

/// The name tab and the resize handle: solid, so they read on any card.
fn chrome_style(p: crate::config::Palette) -> cosmic::iced::widget::container::Style {
    cosmic::iced::widget::container::Style {
        background: Some(Background::Color(Color {
            a: 1.,
            ..p.background
        })),
        border: Border {
            color: Color { a: 0.6, ..p.accent },
            width: 1.,
            radius: (CHROME_TAB / 2.).into(),
        },
        ..Default::default()
    }
}

/// A round button in an arranged card's corner, in the overlay's colors.
fn card_control<'a>(
    p: crate::config::Palette,
    icon: &'static str,
    id: String,
    message: Message,
) -> Element<'a, Message> {
    let style = move |level: usize| cosmic::widget::button::Style {
        icon_color: Some(p.accent),
        text_color: Some(p.accent),
        background: Some(
            Background::Color(Color {
                a: 1.,
                ..[p.background, p.accent, p.accent][level]
            })
            .scale_alpha([1., 0.25, 0.4][level]),
        ),
        border_radius: (CARD_CONTROL / 2.).into(),
        border_width: 1.,
        border_color: Color { a: 0.6, ..p.accent },
        ..Default::default()
    };
    cosmic::widget::button::custom(
        container(cosmic::widget::icon::from_name(icon).size(12).icon())
            .center_x(CARD_CONTROL)
            .center_y(CARD_CONTROL),
    )
    .id(cosmic::widget::Id::new(id))
    .padding(0)
    .width(CARD_CONTROL)
    .height(CARD_CONTROL)
    .class(cosmic::theme::Button::Custom {
        active: Box::new(move |_, _| style(0)),
        hovered: Box::new(move |_, _| style(1)),
        pressed: Box::new(move |_, _| style(2)),
        disabled: Box::new(move |_| style(0)),
    })
    .on_press(message)
    .into()
}

/// Where a widget waits when the desktop has no room for it.
const OFF_DESKTOP: [f32; 2] = [-10000., 0.];

/// Who keeps their spot when two widgets would overlap: earlier ones stay,
/// later ones move clear.
const PLACEMENT_ORDER: [Widget; 10] = [
    Widget::Glance,
    Widget::Clock,
    Widget::Weather,
    Widget::Calendar,
    Widget::Devbox,
    Widget::Projects,
    Widget::Focus,
    Widget::Devices,
    Widget::Notes,
    Widget::Music,
];

/// The card surfaces as built (`Widget::BASE` order). `None`: that card's
/// surface is closed, because it would draw nothing.
#[derive(PartialEq, Eq)]
pub(super) struct SurfaceGeometry {
    custom: bool,
    sizes: [Option<[u32; 2]>; 6],
}

impl SurfaceGeometry {
    /// Whether going from `self` to `next` changes a surface that stays
    /// open, or the layout mode. Opening or closing a card alone does not
    /// rebuild the others.
    fn needs_rebuild(&self, next: &Self) -> bool {
        self.custom != next.custom
            || self
                .sizes
                .iter()
                .zip(&next.sizes)
                .any(|(a, b)| matches!((a, b), (Some(a), Some(b)) if a != b))
    }
}

#[derive(Clone, PartialEq)]
struct LayoutState {
    positions: BTreeMap<String, [f32; 2]>,
    sizes: BTreeMap<String, [f32; 2]>,
    reset: bool,
    shown: BTreeMap<String, bool>,
}
struct LayoutDrag {
    widget: Widget,
    offset: [f32; 2],
    start: [f32; 2],
    size: [f32; 2],
    resize: bool,
    before: LayoutState,
}
#[derive(Default)]
pub(super) struct LayoutEditor {
    pub active: bool,
    pub grid: crate::config::PlacementPreferences,
    saved_grid: crate::config::PlacementPreferences,
    baseline: BTreeMap<String, [f32; 2]>,
    implicit_positions: BTreeMap<String, [f32; 2]>,
    draft: BTreeMap<String, [f32; 2]>,
    sizes: BTreeMap<String, [f32; 2]>,
    baseline_sizes: BTreeMap<String, [f32; 2]>,
    reset: bool,
    // Which widgets are on the desktop. Added and removed widgets are part
    // of the draft, so Undo brings them back and Cancel rolls them back.
    shown: BTreeMap<String, bool>,
    baseline_shown: BTreeMap<String, bool>,
    // The Add widget list is open in the toolbar.
    pub(super) adding: bool,
    // A widget's Settings is open: the overlay waits below windows, without
    // the keyboard, until it is clicked.
    pub(super) aside: bool,
    pointer: Point,
    drag: Option<LayoutDrag>,
    history: Vec<LayoutState>,
    session_actions: Vec<crate::desktop::LatestChange>,
    guard: Option<serde_json::Value>,
    contexts: crate::config::monitors::Contexts,
    // Context strings may gain physical identities later, but only the same
    // live output handle proves that such a change is an alias, not migration.
    output_handles: BTreeMap<String, crate::displays::WlOutput>,
    editing_output: Option<crate::displays::Output>,
    editing_display: Option<String>,
    // Recovery keeps its protection until this exact output has been measured.
    // An ordinary pending Arrange request must never consume the retained draft.
    pub(super) pending_resume: Option<crate::displays::Output>,
    resize_mode: bool,
    pub(super) selected: Option<Widget>,
    error: Option<String>,
    pub(super) exit_failed: bool,
}

impl LayoutEditor {
    fn has_unsaved_changes(&self) -> bool {
        self.reset
            || self.draft != self.baseline
            || self.sizes != self.baseline_sizes
            || self.grid != self.saved_grid
            || self.shown != self.baseline_shown
    }

    pub(super) fn pause_for_output_change(&mut self) {
        self.active = false;
        if self.error.is_some() {
            self.exit_failed = true;
        }
    }

    fn state(&self) -> LayoutState {
        LayoutState {
            positions: self.draft.clone(),
            sizes: self.sizes.clone(),
            reset: self.reset,
            shown: self.shown.clone(),
        }
    }
    fn restore(&mut self, state: LayoutState) {
        self.draft = state.positions;
        self.sizes = state.sizes;
        self.reset = state.reset;
        self.shown = state.shown;
    }
    /// The widgets this draft shows or hides that the saved settings do
    /// not yet, Daily overview first, as it decides about the others.
    fn shown_changes(&self) -> Vec<(Widget, bool)> {
        std::iter::once(Widget::Glance)
            .chain(Widget::ALL.into_iter().filter(|w| *w != Widget::Glance))
            .filter_map(|widget| {
                let shown = *self.shown.get(widget.key())?;
                (self.baseline_shown.get(widget.key()) != Some(&shown)).then_some((widget, shown))
            })
            .collect()
    }
    fn remember(&mut self, before: LayoutState) {
        if self.state() != before {
            if self.history.len() == 50 {
                self.history.remove(0);
            }
            self.history.push(before);
        }
    }

    pub(super) fn positions(&self) -> &BTreeMap<String, [f32; 2]> {
        &self.draft
    }
}

/// How tall the arrange surface is outside Arrange, in pixels: a strip
/// along the top of the display that only measures its usable width.
pub(super) const PROBE_THICKNESS: u32 = 1;

impl Cards {
    /// The arrange surface. While arranging it covers the display and takes
    /// the keyboard. Otherwise it draws nothing and stays open only to
    /// measure the display's usable width (the edge strip measures the
    /// height), so it is a strip a pixel tall.
    pub(super) fn layout_probe(&self, id: window::Id, editing: bool) -> Task<Message> {
        let front = editing && !self.layout_editor.aside;
        get_layer_surface(SctkLayerSurfaceSettings {
            id,
            output: self.layer_output(),
            layer: if front { Layer::Overlay } else { Layer::Bottom },
            keyboard_interactivity: if front {
                KeyboardInteractivity::Exclusive
            } else {
                KeyboardInteractivity::None
            },
            anchor: if editing {
                Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT
            } else {
                Anchor::TOP | Anchor::LEFT | Anchor::RIGHT
            },
            size: Some((Some(0), Some(if editing { 0 } else { PROBE_THICKNESS }))),
            exclusive_zone: 0,
            input_zone: if editing { None } else { Some(Vec::new()) },
            namespace: "halcyon-arrange".into(),
            ..Default::default()
        })
    }
    pub(super) fn custom_layout(&self) -> bool {
        self.config.has_custom_positions() || self.config.multi_monitor()
            // The legacy columns have fixed widths. Small logical outputs use
            // the same bounded individual geometry as arranged widgets.
            || self.screen_size[0] < 800.
            || self.screen_size[1] < 500.
    }
    pub(super) fn widget_id(&self, widget: Widget) -> window::Id {
        self.instance_window(&crate::design::InstanceId::legacy(widget))
            .expect("the singleton adapter has a reserved identity for every widget")
    }
    /// Runtime surfaces resolve saved identities explicitly. Unsupported
    /// duplicate IDs can never accidentally address a same-kind singleton.
    pub(super) fn instance_window(
        &self,
        instance: &crate::design::InstanceId,
    ) -> Option<window::Id> {
        Some(
            self.ids[match instance.legacy_kind()? {
                Widget::Clock => 0,
                Widget::Weather => 1,
                Widget::Calendar => 4,
                Widget::Devbox => 6,
                Widget::Projects => 7,
                Widget::Glance => 8,
                Widget::Music => 9,
                Widget::Notes => 10,
                Widget::Focus => 11,
                Widget::Devices => 13,
            }],
        )
    }
    pub(super) fn explicit_size(&self, widget: Widget) -> Option<[f32; 2]> {
        let sizes = if self.layout_editor.active {
            &self.layout_editor.sizes
        } else {
            &self.effective_sizes()
        };
        sizes
            .get(widget.key())
            .copied()
            .map(|size| crate::layout::sized(widget, size, self.widget_screen(widget)))
    }
    pub(super) fn widget_size(&self, widget: Widget) -> [f32; 2] {
        if let Some(size) = self.explicit_size(widget) {
            return crate::layout_state::effective_explicit_size(
                widget,
                size,
                self.content_height(widget),
                self.widget_screen(widget),
            );
        }
        if widget == Widget::Glance {
            self.glance_size()
        } else {
            self.automatic_size(widget)
        }
    }
    /// The height a card's content needs, for cards that list readings or
    /// items; the others lay out within any size.
    fn content_height(&self, widget: Widget) -> Option<f32> {
        match widget {
            Widget::Devbox => Some(self.devbox_natural_height()),
            Widget::Projects => Some(self.projects_natural_height()),
            Widget::Calendar => Some(self.calendar_natural_height()),
            _ => None,
        }
    }
    pub(super) fn automatic_size(&self, widget: Widget) -> [f32; 2] {
        if widget == Widget::Glance {
            return [
                self.glance_auto_width(),
                self.glance_automatic_height()
                    .min((self.widget_screen(widget)[1] - 96.).max(1.)),
            ];
        }
        if widget.is_tool() {
            let [width, height] = crate::tools::cards::size(widget, &self.config);
            return [
                width.min((self.widget_screen(widget)[0] - 48.).max(1.)),
                height.min((self.widget_screen(widget)[1] - 96.).max(1.)),
            ];
        }
        let width: f32 = if widget == Widget::Clock {
            crate::widget_presentation::CLOCK_SIZE[0]
        } else {
            crate::widget_presentation::WEATHER_SIZE[0]
        };
        let mut height = match widget {
            Widget::Clock => crate::widget_presentation::CLOCK_SIZE[1],
            Widget::Weather => crate::widget_presentation::WEATHER_SIZE[1],
            Widget::Calendar => self.calendar_natural_height(),
            Widget::Projects => self.projects_natural_height(),
            Widget::Devbox => self.devbox_natural_height(),
            _ => unreachable!(),
        };
        if matches!(widget, Widget::Weather | Widget::Devbox | Widget::Projects) {
            let widgets: Vec<_> = self
                .config
                .ordered_right_cards()
                .into_iter()
                .filter(|w| self.config.widget_visible(*w) && self.same_widget_display(*w, widget))
                .collect();
            if let Some((_, scaled)) = drawer::card_layout_with_devbox(
                &widgets,
                (self.widget_screen(widget)[1] - 96.).max(1.),
                self.devbox_natural_height(),
            )
            .into_iter()
            .find(|(w, _)| *w == widget)
            {
                height = height.min(scaled);
            }
        }
        [
            width.min((self.widget_screen(widget)[0] - 48.).max(1.)),
            height.min((self.widget_screen(widget)[1] - 96.).max(1.)),
        ]
    }
    pub(super) fn default_position(&self, widget: Widget) -> [f32; 2] {
        let size = self.widget_size(widget);
        if widget.is_tool() {
            return crate::layout::tool_position(
                widget,
                size,
                self.widget_size(Widget::Focus)[1],
                self.widget_screen(widget),
            );
        }
        let position = match widget {
            Widget::Clock | Widget::Glance => [82., 88.],
            Widget::Calendar => {
                // Under the clock when the centred spot would overlap it.
                let below_clock = if self.config.widget_visible(Widget::Clock)
                    && self.same_widget_display(Widget::Clock, widget)
                {
                    88. + self.widget_size(Widget::Clock)[1] + drawer::CARD_GAP
                } else {
                    0.
                };
                [42., ((self.widget_screen(widget)[1] - size[1]) / 2.).max(below_clock)]
            }
            _ => {
                let widgets: Vec<_> = self
                    .config
                    .ordered_right_cards()
                    .into_iter()
                    .filter(|w| {
                        self.config.widget_visible(*w) && self.same_widget_display(*w, widget)
                    })
                    .collect();
                let total: f32 = widgets.iter().map(|w| self.widget_size(*w)[1]).sum::<f32>()
                    + widgets.len().saturating_sub(1) as f32 * drawer::CARD_GAP;
                let before: f32 = widgets
                    .iter()
                    .take_while(|w| **w != widget)
                    .map(|w| self.widget_size(*w)[1] + drawer::CARD_GAP)
                    .sum();
                [
                    self.widget_screen(widget)[0] - size[0] - 42.,
                    (self.widget_screen(widget)[1] - total) / 2. + before,
                ]
            }
        };
        crate::layout::clamp(position, size, self.widget_screen(widget))
    }
    /// Where `widget` sits on the desktop: its saved or automatic place,
    /// moved to the nearest free spot when it would cover a widget placed
    /// before it (`PLACEMENT_ORDER`). Cards held by the default columns stay
    /// where the column puts them; everything else can move.
    pub(super) fn placed_position(&self, widget: Widget) -> [f32; 2] {
        let mut placed: Vec<[f32; 4]> = Vec::new();
        if !self.widget_available(widget) {
            return OFF_DESKTOP;
        }
        for candidate in PLACEMENT_ORDER {
            if !self.same_widget_display(candidate, widget) || !self.widget_available(candidate) {
                continue;
            }
            let shown = if candidate.is_tool() {
                self.tool_shown(candidate)
            } else {
                self.config.widget_visible(candidate)
            };
            if !shown && candidate != widget {
                continue;
            }
            let size = self.widget_size(candidate);
            let mut at = self.position_from(candidate, &self.effective_positions());
            let movable =
                self.custom_layout() || candidate.is_tool() || candidate == Widget::Glance;
            if shown && movable {
                match crate::layout::free_spot(at, size, &placed, self.widget_screen(widget)) {
                    Some(free) => at = free,
                    // No room left: off the desktop rather than on top of
                    // another widget, until there is space again.
                    None if candidate == widget => return OFF_DESKTOP,
                    None => continue,
                }
            }
            if candidate == widget {
                return at;
            }
            placed.push([at[0], at[1], size[0], size[1]]);
        }
        self.position_from(widget, &self.effective_positions())
    }
    /// Where Arrange draws a card placed at `position`: where its desktop
    /// surface puts it. Placed surfaces land on whole pixels; the default
    /// columns center their cards without rounding.
    pub(super) fn arrange_origin(&self, widget: Widget, position: [f32; 2]) -> [f32; 2] {
        if self.custom_layout() || widget == Widget::Glance || widget.is_tool() {
            position.map(f32::round)
        } else {
            position
        }
    }
    pub(super) fn position_from(
        &self,
        widget: Widget,
        positions: &BTreeMap<String, [f32; 2]>,
    ) -> [f32; 2] {
        if widget == Widget::Glance {
            return self.glance_position(positions);
        }
        positions
            .get(widget.key())
            .map(|p| {
                crate::layout::decode(*p, self.widget_size(widget), self.widget_screen(widget))
            })
            .unwrap_or_else(|| self.default_position(widget))
    }
    /// Decorative materials never activate native backdrop blur. Clear any
    /// pending effect without changing card geometry, shadows or input regions.
    pub(super) fn blur_surfaces(&self) -> Task<Message> {
        use cosmic::iced::platform_specific::shell::commands::blur::blur;
        Task::batch(Widget::ALL.map(|widget| blur(self.widget_id(widget), None).discard()))
    }
    /// The style's shadow margin around every widget surface, in pixels:
    /// (each side, both sides).
    /// Where a card surface takes clicks, in surface coordinates: its cards
    /// only. The transparent shadow margin and a column's empty space pass
    /// clicks through, so they never cover a tool or another card.
    pub(super) fn arrange_hides_surface(&self, widget: Widget) -> bool {
        self.layout_editor.active
            && (self.arranging_widget(widget)
                || (widget == Widget::Weather
                    && !self.custom_layout()
                    && self
                        .visible_cards()
                        .into_iter()
                        .any(|w| self.arranging_widget(w))))
    }
    pub(super) fn card_input_rects(&self, widget: Widget) -> Vec<[u32; 4]> {
        if self.arrange_hides_surface(widget) {
            return Vec::new();
        }
        let m = crate::widget_style::shadow_margin(&self.config);
        let (mi, mu) = self.shadow_px();
        let mi = mi.max(0) as u32;
        match widget {
            Widget::Clock => {
                if !self.config.widget_visible(widget) || !self.widget_available(widget) {
                    return Vec::new();
                }
                self.config
                    .recipes
                    .clock
                    .as_ref()
                    .and_then(|r| {
                        let r = super::recipe::shown(r, &self.config);
                        super::recipe::action_rect(&r, self.widget_size(widget))
                    })
                    .map(|r| [r[0] + m.round() as u32, r[1] + m.round() as u32, r[2], r[3]])
                    .into_iter()
                    .collect()
            }
            Widget::Glance => self
                .glance_input_rects()
                .into_iter()
                .map(|r| [r[0] + m.round() as u32, r[1] + m.round() as u32, r[2], r[3]])
                .collect(),
            _ if self.custom_layout() => {
                if self.config.widget_visible(widget) && self.widget_available(widget) {
                    let size = self.widget_size(widget);
                    crate::widget_presentation::input_rect(
                        [mi as f32; 2],
                        size,
                        size.map(|v| v.ceil() + mu as f32),
                    )
                    .into_iter()
                    .collect()
                } else {
                    Vec::new()
                }
            }
            Widget::Weather => {
                let cards = self.visible_cards();
                let total = cards.iter().map(|w| self.card_height(*w)).sum::<f32>()
                    + cards.len().saturating_sub(1) as f32 * drawer::CARD_GAP;
                let height = (self.screen_size[1] - 96. + mu as f32).max(1.);
                let mut top = ((height - total) / 2.).max(m);
                cards
                    .into_iter()
                    .filter_map(|card| {
                        let size = [360., self.card_height(card)];
                        let rect = crate::widget_presentation::input_rect(
                            [m, top],
                            size,
                            [360. + mu as f32, height],
                        );
                        top += size[1] + drawer::CARD_GAP;
                        rect
                    })
                    .collect()
            }
            Widget::Calendar => {
                if !self.config.widget_visible(Widget::Calendar) {
                    return Vec::new();
                }
                // The card is as tall as its agenda, up to the room the
                // column gives it; a little slack keeps a card that is a few
                // pixels taller than its estimate clickable.
                let room = self.card_height(Widget::Calendar);
                let height = (self.screen_size[1] - 96. + mu as f32).max(1.);
                let card_top = self
                    .calendar_column_top()
                    .map_or(m, |top| (top - 48.).max(0.) + m);
                let slack = 24.;
                let top = (card_top - slack).max(0.);
                let bottom =
                    (card_top + self.calendar_natural_height().min(room) + slack).min(height);
                vec![[
                    0,
                    top.floor() as u32,
                    360 + mu,
                    (bottom - top).max(0.).ceil() as u32,
                ]]
            }
            _ => Vec::new(),
        }
    }
    pub(super) fn shadow_px(&self) -> (i32, u32) {
        let m = crate::widget_style::shadow_margin(&self.config);
        (m.round() as i32, (2. * m).round() as u32)
    }
    pub(super) fn surface_geometry(&self) -> SurfaceGeometry {
        // The margin is part of each size, so a style with another shadow
        // recreates the surfaces.
        let (_, mu) = self.shadow_px();
        SurfaceGeometry {
            custom: self.custom_layout(),
            sizes: Widget::BASE.map(|widget| {
                if !self.card_surface_needed(widget) {
                    None
                } else if self.custom_layout() || widget == Widget::Glance {
                    Some(
                        self.widget_size(widget)
                            .map(|value| value.ceil() as u32 + mu),
                    )
                } else {
                    Some(match widget {
                        Widget::Clock => [490 + mu, 250 + mu],
                        _ => [360 + mu, 0],
                    })
                }
            }),
        }
    }
    /// Whether a card's surface draws anything. One that would draw nothing
    /// is closed rather than parked off screen: every open window holds its
    /// own swapchain buffers and renderer. Arranging doesn't count, so
    /// entering and leaving Arrange never reopens a card.
    pub(super) fn card_surface_needed(&self, widget: Widget) -> bool {
        if self.custom_layout() || widget == Widget::Glance {
            return self.config.widget_visible(widget) && self.widget_available(widget);
        }
        // The fixed columns: Clock and Calendar on the left, the other cards
        // stacked in the right-hand column (the Weather surface).
        match widget {
            Widget::Clock => self.config.widget_visible(widget) && self.widget_available(widget),
            Widget::Calendar => self.config.widget_visible(widget),
            Widget::Weather => !self.visible_cards().is_empty(),
            _ => false,
        }
    }
    /// The card surfaces that are open, as last built.
    pub(super) fn card_surface_open(&self, widget: Widget) -> bool {
        let index = Widget::BASE.iter().position(|w| *w == widget);
        self.surface_geometry
            .as_ref()
            .zip(index)
            .is_some_and(|(geometry, index)| geometry.sizes[index].is_some())
    }
    /// Whether the card surfaces must all be built again: none are built
    /// yet, or one that stays open changed size, or the layout mode changed.
    pub(super) fn cards_need_rebuild(&self) -> bool {
        self.surface_geometry
            .as_ref()
            .is_none_or(|known| known.needs_rebuild(&self.surface_geometry()))
    }
    /// Opens the card surfaces that draw something again and closes the
    /// ones that stopped, leaving the others alone. Runs at the end of each
    /// update, once, for its final state: an open and a close of the same
    /// surface queued within one update could reach the compositor in
    /// either order.
    pub(super) fn sync_open_cards(&mut self) -> Task<Message> {
        if !self.output_known || self.selected_output.is_none() {
            return Task::none();
        }
        let Some(open) = self
            .surface_geometry
            .as_ref()
            .map(|known| known.sizes.map(|size| size.is_some()))
        else {
            return Task::none();
        };
        let needed = Widget::BASE.map(|widget| self.card_surface_needed(widget));
        if needed == open {
            return Task::none();
        }
        if self.cards_need_rebuild() {
            return self.rebuild_cards();
        }
        let mut tasks = Vec::new();
        for (index, widget) in Widget::BASE.into_iter().enumerate() {
            if needed[index] == open[index] {
                continue;
            }
            let id = self.widget_id(widget);
            tasks.push(if needed[index] {
                destroy_layer_surface(id).chain(get_layer_surface(self.card_surface(widget)))
            } else {
                destroy_layer_surface(id)
            });
        }
        self.surface_geometry = Some(self.surface_geometry());
        self.surface_inputs = Some(Widget::BASE.map(|widget| self.card_input_rects(widget)));
        Task::batch(tasks)
    }
    /// The geometry Settings previews: each output's area widgets use, and
    /// every widget's size and place in it.
    pub(crate) fn layout_snapshot(&self) -> crate::layout_state::Snapshot {
        crate::layout_state::Snapshot {
            monitors: self
                .displays
                .outputs()
                .iter()
                .filter_map(|o| {
                    let key = self.displays.key(o)?;
                    let size = if self
                        .selected_output
                        .as_ref()
                        .is_some_and(|s| s.handle == o.handle)
                    {
                        self.screen_size
                    } else {
                        self.extra_probes
                            .iter()
                            .find(|p| p.output.handle == o.handle)
                            .and_then(monitors::Probe::size)
                            .or_else(|| {
                                o.info
                                    .logical_size
                                    .filter(|(w, h)| *w > 0 && *h > 0)
                                    .map(|(w, h)| [w as f32, h as f32])
                            })?
                    };
                    Some((key, size))
                })
                .collect(),
            widget_displays: self.displays.placements(&self.config),
            display: Some(crate::layout_state::DisplayState {
                output: self
                    .selected_output
                    .as_ref()
                    .and_then(|output| self.displays.key(output)),
                ready: true,
                epoch: self.display_generation,
            }),
            screen: self.screen_size,
            widgets: Widget::ALL
                .into_iter()
                .map(|widget| crate::layout_state::WidgetGeometry {
                    natural_size: Some(self.automatic_size(widget)),
                    key: widget.key().into(),
                    size: self.widget_size(widget),
                    natural_height: if widget == Widget::Glance {
                        Some(self.glance_automatic_height())
                    } else {
                        self.content_height(widget)
                    },
                    position: self.placed_position(widget),
                })
                .collect(),
            output: None,
        }
    }
    pub(super) fn sync_widget_surfaces(&mut self) -> Task<Message> {
        if !self.output_known || self.selected_output.is_none() {
            return Task::none();
        }
        if let Some(config) = self.seed_glance_position() {
            return self.apply_config(config);
        }
        let snapshot = self.layout_snapshot();
        let generation = crate::layout_state::next_generation();
        let publish = cosmic::iced::Task::future(async move {
            crate::layout_state::publish(&snapshot, generation);
        })
        .discard();
        // The pinned layer-shell Size action corrupts the renderer's height.
        // Keep stable surfaces for ordinary edits; recreate only on real size
        // or layout-mode changes, using the correct initial protocol geometry.
        if self.cards_need_rebuild() {
            if self.updating {
                // Once, at the end of this update, for its final geometry
                // (see `update`): two rebuilds at once race each other.
                self.rebuild_pending = true;
                return publish;
            }
            return publish.chain(self.rebuild_cards());
        }
        let tools = Task::batch(self.sync_tool_surfaces());
        let mut tasks = vec![tools];
        // Inside an update, cards open and close once at its end (see
        // `update`).
        if !self.updating {
            tasks.push(self.sync_open_cards());
        }
        // Cards that grew or moved inside their surface take clicks where
        // they are now.
        let inputs = Widget::BASE.map(|widget| self.card_input_rects(widget));
        if self.surface_inputs.as_ref() != Some(&inputs) {
            for (widget, rects) in Widget::BASE.into_iter().zip(inputs.iter()) {
                if self.surface_inputs.as_ref().is_none_or(|known| {
                    known[Widget::BASE.iter().position(|w| *w == widget).unwrap()] != *rects
                }) {
                    tasks.push(set_input_zone(self.widget_id(widget), rects));
                }
            }
            self.surface_inputs = Some(inputs);
        }
        let (mi, _) = self.shadow_px();
        if self.custom_layout() {
            for widget in Widget::BASE {
                let id = self.widget_id(widget);
                let point = if !self.config.widget_visible(widget) {
                    [-10000., 0.]
                } else {
                    self.placed_position(widget)
                };
                tasks.push(set_margin(
                    id,
                    point[1].round() as i32 - mi,
                    0,
                    0,
                    point[0].round() as i32 - mi,
                ));
            }
        } else {
            tasks.extend([
                set_margin(self.ids[0], 88 - mi, 0, 0, self.config.clock_margin() - mi),
                set_margin(self.ids[1], 48 - mi, 42 - mi, 48 - mi, 0),
                set_margin(
                    self.ids[4],
                    48 - mi,
                    0,
                    48 - mi,
                    self.config.calendar_margin() - mi,
                ),
            ]);
            let point = if self.config.widget_visible(Widget::Glance) {
                self.placed_position(Widget::Glance)
            } else {
                [-10000., 0.]
            };
            tasks.push(set_margin(
                self.ids[8],
                point[1].round() as i32 - mi,
                0,
                0,
                point[0].round() as i32 - mi,
            ));
            for id in [self.ids[6], self.ids[7]] {
                tasks.push(set_margin(id, 0, 0, 0, -10000));
            }
        }
        // The overlay owns only its editing display. Other displays still need
        // normal geometry/input reconciliation as their probes arrive.
        let hide = if self.layout_editor.active {
            let custom = self.custom_layout();
            Task::batch(
                Widget::ALL
                    .into_iter()
                    .filter(|w| self.arranging_widget(*w))
                    .map(|widget| match (custom, widget) {
                        (false, Widget::Weather) => {
                            set_margin(self.widget_id(widget), 48, -10000, 48, 0)
                        }
                        (false, Widget::Calendar) => {
                            set_margin(self.widget_id(widget), 48, 0, 48, -10000)
                        }
                        _ => set_margin(self.widget_id(widget), 0, 0, 0, -10000),
                    }),
            )
        } else {
            Task::none()
        };
        tasks.push(publish);
        tasks.push(self.blur_surfaces());
        Task::batch(tasks).chain(hide)
    }
    pub(super) fn arrange(&mut self) -> Task<Message> {
        crate::config::with_nonblocking_locks(|| self.arrange_now())
    }
    fn arrange_now(&mut self) -> Task<Message> {
        if !self.output_known {
            return Task::none();
        }
        if self
            .paused_layout
            .as_ref()
            .is_some_and(|draft| draft.exit_failed || draft.pending_resume.is_some())
        {
            self.sidebar.show(Instant::now());
            return Task::none();
        }
        if self.paused_layout.as_ref().is_some_and(|d| {
            !d.exit_failed && !d.contexts.is_empty() && d.contexts != self.layout_contexts()
        }) {
            // The drag was cancelled before migration and committed changes are
            // already durable. Start from this display's own layout on resume.
            self.paused_layout = None;
        }
        if let Some(draft) = self.paused_layout.take() {
            return self.activate_paused_arrange(draft);
        }
        if self.layout_editor.active {
            return Task::none();
        }
        // An open note is saved first; arranging needs the keyboard, and a
        // draft that couldn't be saved stays paused until Resume.
        // An unsaved note is kept, and saved only when that can't replace a
        // newer one; a conflict waits for an explicit Save.
        let note = self.save_note_quietly();
        self.pause_note();
        self.sidebar.hide();
        let mut draft = self.effective_positions().clone();
        for widget in Widget::ALL
            .into_iter()
            .filter(|w| self.arranging_widget(*w))
        {
            draft.entry(widget.key().into()).or_insert_with(|| {
                crate::layout_state::encode_position(
                    widget,
                    self.default_position(widget),
                    self.widget_size(widget),
                    self.screen_size,
                )
            });
        }
        // Widgets that ended up on top of each other are shown apart, as the
        // desktop shows them.
        self.keep_apart(&mut draft, None, None);
        self.layout_editor = LayoutEditor {
            active: true,
            contexts: self.layout_contexts(),
            output_handles: self.layout_output_handles(),
            editing_output: self.selected_output.clone(),
            editing_display: self
                .selected_output
                .as_ref()
                .and_then(|output| self.displays.key(output)),
            guard: Config::path()
                .and_then(|p| crate::config::read_config_text(&p).ok())
                .and_then(|t| crate::config::parse_config_text(t.as_deref()).ok()),
            baseline_sizes: self.effective_sizes().clone(),
            sizes: self.effective_sizes().clone(),
            implicit_positions: draft
                .iter()
                .filter(|(key, _)| {
                    !self.config.widget_positions.contains_key(*key)
                        && Widget::parse(key).is_some_and(|widget| self.arranging_widget(widget))
                })
                .map(|(key, point)| (key.clone(), *point))
                .collect(),
            baseline: draft.clone(),
            draft,
            shown: self.shown_now(),
            baseline_shown: self.shown_now(),
            grid: crate::config::PlacementPreferences {
                snap: self.config.placement.snap,
                columns: crate::config::PlacementPreferences::FINE_COLUMNS,
            },
            saved_grid: self.config.placement,
            ..Default::default()
        };
        Task::batch([
            // A draft that couldn't be saved waits, without the keyboard.
            note.chain(self.note_surface_mode()),
            // ArrangeSurface reconciles widgets after checking the entry epoch.
            // Capturing a rebuild here lets a delayed entry recreate old widget
            // surfaces after Escape or a newer Arrange session.
            self.replace_arrange_surface(),
        ])
    }
    fn activate_paused_arrange(&mut self, mut draft: LayoutEditor) -> Task<Message> {
        draft.active = true;
        draft.exit_failed = false;
        draft.pending_resume = None;
        draft.aside = false;
        draft.adding = false;
        draft.output_handles = self.layout_output_handles();
        draft.editing_output = self.selected_output.clone();
        draft.editing_display = self
            .selected_output
            .as_ref()
            .and_then(|output| self.displays.key(output));
        self.layout_editor = draft;
        self.pause_note();
        self.replace_arrange_surface()
    }

    /// Called only when output handles and geometry are unchanged. Keep the
    /// draft and rollback receipts, but write future gestures under live keys.
    pub(super) fn rebase_arrange_identity(&mut self) {
        let contexts = self.layout_contexts();
        let handles = self.layout_output_handles();
        let canonical = self.config.on_displays(&contexts);
        for editor in std::iter::once(&mut self.layout_editor).chain(self.paused_layout.iter_mut())
        {
            if let Some(original) = &editor.editing_output
                && let Some(output) = self
                    .displays
                    .outputs()
                    .iter()
                    .find(|output| output.handle == original.handle)
            {
                editor.editing_display = self.displays.key(output);
            }
            if editor.contexts.is_empty() || editor.contexts == contexts {
                continue;
            }
            for (key, context) in &contexts {
                if editor.contexts.get(key) == Some(context)
                    || !editor.contexts.contains_key(key)
                    || !handles
                        .get(key)
                        .is_some_and(|handle| editor.output_handles.get(key) == Some(handle))
                {
                    continue;
                }
                for (baseline, current) in [
                    (&mut editor.baseline, &canonical.widget_positions),
                    (&mut editor.baseline_sizes, &canonical.widget_sizes),
                ] {
                    if let Some(value) = current.get(key) {
                        baseline.insert(key.clone(), *value);
                    } else {
                        baseline.remove(key);
                    }
                }
                editor.contexts.insert(key.clone(), context.clone());
            }
        }
    }
    pub(super) fn commit_arrange(&mut self) -> bool {
        crate::config::with_nonblocking_locks(|| self.commit_arrange_now())
    }
    fn commit_arrange_now(&mut self) -> bool {
        if !self.layout_editor.active {
            return true;
        }
        if !self.layout_editor.has_unsaved_changes() {
            return true;
        }
        let mut baseline = self.layout_editor.baseline.clone();
        if self.layout_editor.reset {
            for widget in Widget::ALL {
                if self.arranging_widget(widget)
                    && self.layout_editor.draft.contains_key(widget.key())
                {
                    // A move after Reset is intentional even if it happens
                    // to return to the position from before the reset.
                    baseline.remove(widget.key());
                }
            }
        }
        let mut size_baseline = self.layout_editor.baseline_sizes.clone();
        if self.layout_editor.reset {
            for widget in Widget::ALL
                .into_iter()
                .filter(|w| self.arranging_widget(*w))
            {
                if self.layout_editor.sizes.contains_key(widget.key()) {
                    size_baseline.remove(widget.key());
                } else {
                    size_baseline.entry(widget.key().into()).or_insert([1., 1.]);
                }
            }
        }
        let (result, receipts) = crate::desktop::actions::with_receipts(|| {
            crate::desktop::actions::in_action("Widget arrangement", || {
                crate::desktop::actions::guarded(self.layout_editor.guard.as_ref(), || {
                    Config::save_arrangement_on_displays(
                        &self.layout_editor.contexts,
                        &self.layout_editor.draft,
                        &baseline,
                        &self.layout_editor.sizes,
                        &size_baseline,
                        &self.layout_editor.implicit_positions,
                        self.layout_editor.grid,
                        self.layout_editor.saved_grid,
                        &self.layout_editor.shown_changes(),
                    )
                })
            })
        });
        self.layout_editor.session_actions.extend(receipts);
        match result {
            Ok(mut config) => {
                self.layout_editor.error = None;
                if !self.system_animations {
                    Self::still(&mut config);
                }
                self.config = config;
                self.refresh_display_layouts();
                self.layout_editor.baseline = self.layout_editor.draft.clone();
                self.layout_editor.baseline_sizes = self.layout_editor.sizes.clone();
                self.layout_editor.saved_grid = self.layout_editor.grid;
                self.layout_editor.reset = false;
                // Showing or hiding Daily overview can change other widgets.
                self.layout_editor.shown = self.shown_now();
                self.layout_editor.baseline_shown = self.layout_editor.shown.clone();
                self.layout_editor.guard = Config::path()
                    .and_then(|p| crate::config::read_config_text(&p).ok())
                    .and_then(|t| crate::config::parse_config_text(t.as_deref()).ok());
            }
            Err(error) => {
                self.layout_editor.error = Some(format!("Could not save: {error}"));
                return false;
            }
        }
        true
    }
    /// Retire the input identity on every entry/exit. Deferred creation is
    /// checked against the same epoch as hotplug, so an old task cannot reopen
    /// an exclusive layer over a newer session or a different output.
    fn replace_arrange_surface(&mut self) -> Task<Message> {
        let old = std::mem::replace(&mut self.ids[5], window::Id::unique());
        self.display_generation += 1;
        destroy_layer_surface(old).chain(Task::done(cosmic::Action::App(Message::ArrangeSurface(
            self.display_generation,
        ))))
    }

    pub(super) fn finish_arrange(&mut self, save: bool) -> Task<Message> {
        crate::config::with_nonblocking_locks(|| {
            let task = self.finish_arrange_now(save);
            if self.arrange_display.take().is_some() {
                self.pending_arrange = None;
                task.chain(self.reconcile_output())
            } else {
                task
            }
        })
    }
    fn finish_arrange_now(&mut self, save: bool) -> Task<Message> {
        if !self.layout_editor.active {
            return Task::none();
        }
        let result = if save {
            if self.commit_arrange() {
                Ok(())
            } else {
                Err(self
                    .layout_editor
                    .error
                    .clone()
                    .unwrap_or_else(|| "Could not save arrangement".into()))
            }
        } else {
            crate::desktop::actions::cancel_arrangement(&self.layout_editor.session_actions)
        };
        // Input release never depends on rollback success. Preserve the exact
        // session receipt and draft for explicit recovery outside the overlay.
        let mut editor = std::mem::take(&mut self.layout_editor);
        let reload = !editor.session_actions.is_empty() || result.is_err();
        editor.active = false;
        editor.drag = None;
        if let Err(error) = result {
            editor.error = Some(error);
            editor.exit_failed = true;
            self.paused_layout = Some(editor);
            self.sidebar.show(Instant::now());
        }
        // Retire the captured probe before applying config. A changed display
        // pin then advances the epoch again, invalidating this Arrange
        // continuation while keeping migration's fresh probe/IDs intact.
        let release = self.replace_arrange_surface();
        // A no-op exit preserves the current in-memory configuration, as
        // before; only a transaction or failure needs a storage refresh.
        if reload && let Ok(config) = Config::try_load() {
            release.chain(self.apply_config(config))
        } else {
            release
        }
    }

    pub(super) fn retry_arrange_cancel(&mut self) -> Task<Message> {
        crate::config::with_nonblocking_locks(|| {
            let Some(editor) = self.paused_layout.as_mut().filter(|e| e.exit_failed) else {
                return Task::none();
            };
            let was_resuming = editor.pending_resume.take().is_some();
            match crate::desktop::actions::cancel_arrangement(&editor.session_actions) {
                Ok(()) => self.clear_arrange_recovery(),
                Err(error) => {
                    editor.error = Some(error);
                    if was_resuming {
                        self.pending_arrange = None;
                        self.arrange_display = None;
                        self.reconcile_output()
                    } else {
                        Task::none()
                    }
                }
            }
        })
    }
    pub(super) fn resume_arrange(&mut self) -> Task<Message> {
        let contexts = self.layout_contexts();
        let Some(editor) = self.paused_layout.as_mut() else {
            // A queued recovery click after Keep/Retry cannot start a new session.
            return Task::none();
        };
        if editor.pending_resume.is_some() {
            return Task::none();
        }
        let output = editor
            .editing_display
            .as_deref()
            .and_then(|display| self.displays.unique(display))
            .or_else(|| {
                let original = editor.editing_output.as_ref()?;
                self.displays
                    .outputs()
                    .iter()
                    .find(|output| output.handle == original.handle)
            });
        if editor.contexts != contexts || output.is_none() {
            editor.error = Some("Displays changed. Reconnect the original displays to keep arranging, or choose another option.".into());
            return Task::none();
        }
        let display = output.and_then(|output| self.displays.key(output));
        if display.is_none()
            && output.map(|output| &output.handle)
                != self.selected_output.as_ref().map(|output| &output.handle)
        {
            editor.error = Some("The original editing display is unavailable.".into());
            return Task::none();
        }
        editor.pending_resume = output.cloned();
        editor.exit_failed = true;
        self.pending_arrange = None;
        self.arrange_display = display;
        let migrate = self.reconcile_output();
        if self.output_known {
            migrate.chain(self.complete_arrange_resume())
        } else {
            migrate
        }
    }

    pub(super) fn pause_invalid_arrange_resume(
        &mut self,
        selected: Option<&crate::displays::Output>,
    ) -> bool {
        let contexts = self.layout_contexts();
        let Some(editor) = self.paused_layout.as_mut() else {
            return false;
        };
        let Some(expected) = editor.pending_resume.as_ref() else {
            return false;
        };
        if editor.contexts == contexts
            && selected.is_some_and(|output| expected.same_geometry(output))
        {
            return false;
        }
        editor.pending_resume = None;
        editor.exit_failed = true;
        editor.error = Some("Displays changed while resuming. Reconnect the original displays to keep arranging, or choose another option.".into());
        self.pending_arrange = None;
        self.arrange_display = None;
        self.sidebar.show(Instant::now());
        true
    }

    pub(super) fn complete_arrange_resume(&mut self) -> Task<Message> {
        if !self.output_known
            || !self
                .paused_layout
                .as_ref()
                .is_some_and(|editor| editor.pending_resume.is_some())
        {
            return Task::none();
        }
        let selected = self.selected_output.clone();
        if self.pause_invalid_arrange_resume(selected.as_ref()) {
            return self.reconcile_output();
        }
        let editor = self.paused_layout.take().unwrap();
        self.activate_paused_arrange(editor)
    }
    pub(super) fn keep_arrange_saved(&mut self) -> Task<Message> {
        if self.paused_layout.as_ref().is_some_and(|e| e.exit_failed) {
            return self.clear_arrange_recovery();
        }
        Task::none()
    }

    fn clear_arrange_recovery(&mut self) -> Task<Message> {
        self.paused_layout = None;
        self.pending_arrange = None;
        // Geometry can pause a failed gesture before finish_arrange has had
        // a chance to return the controls to the default display.
        let restore_output = self.arrange_display.take().is_some();
        let task = if let Ok(config) = Config::try_load() {
            self.apply_config(config)
        } else {
            self.sync_widget_surfaces()
        };
        if restore_output {
            task.chain(self.reconcile_output())
        } else {
            task
        }
    }

    pub(super) fn arrange_recovery_view(&self) -> Option<Element<'_, Message>> {
        let editor = self.paused_layout.as_ref().filter(|e| e.exit_failed)?;
        let mut content: Vec<Element<'_, Message>> = vec![
            text::heading("Your arrangement wasn’t saved").into(),
            text::body("Your desktop works normally. Choose what to do with your changes.").into(),
            cosmic::widget::button::suggested("Keep arranging")
                .width(Length::Fill)
                .id("arrange-resume".into())
                .on_press(Message::LayoutResume)
                .into(),
            cosmic::widget::button::standard("Discard changes")
                .width(Length::Fill)
                .id("arrange-keep".into())
                .on_press(Message::LayoutKeepSaved)
                .into(),
            cosmic::widget::button::standard("Undo all of Arrange")
                .width(Length::Fill)
                .id("arrange-retry".into())
                .on_press(Message::LayoutRetryCancel)
                .into(),
            cosmic::widget::button::link(if self.recovery_details {
                "Hide details"
            } else {
                "Details"
            })
            .id("arrange-details".into())
            .on_press(Message::RecoveryDetails)
            .into(),
        ];
        if self.recovery_details {
            content.push(
                cosmic::widget::scrollable(text::caption(
                    editor
                        .error
                        .as_deref()
                        .unwrap_or("Could not finish arrangement"),
                ))
                .height(Length::Fill)
                .into(),
            );
        }
        let content = column::with_children(content).spacing(10);
        Some(
            container(content)
                .padding(12)
                .width(Length::Fill)
                .height(Length::Fill)
                .class(cosmic::theme::Container::Card)
                .into(),
        )
    }
    pub(super) fn reset_layout(&mut self) -> Task<Message> {
        match crate::desktop::actions::in_action("Reset widget arrangement", || {
            Config::save_changes(
                serde_json::json!({"widget_positions": {}, "widget_sizes": {}, "monitor_layouts": {}}),
            )
        }) {
            Ok(mut config) => {
                if !self.system_animations {
                    Self::still(&mut config);
                }
                self.config = config;
                self.refresh_display_layouts();
            }
            Err(error) => {
                eprintln!("Could not reset desktop layout: {error}");
                self.raise_notice(
                    "Couldn’t reset the layout",
                    "Your widgets stay where they were.",
                    error.to_string(),
                    Some(Message::ResetLayout),
                );
                return Task::none();
            }
        }
        if self.layout_editor.active {
            self.layout_editor.draft.clear();
            self.layout_editor.sizes.clear();
            self.layout_editor.drag = None;
        }
        self.sync_widget_surfaces()
    }
    pub(super) fn layout_press(&mut self, widget: Widget) {
        let point = self.position_from(widget, &self.layout_editor.draft);
        self.layout_editor.selected = Some(widget);
        self.layout_editor.drag = Some(LayoutDrag {
            widget,
            offset: [
                self.layout_editor.pointer.x - point[0],
                self.layout_editor.pointer.y - point[1],
            ],
            start: [self.layout_editor.pointer.x, self.layout_editor.pointer.y],
            size: self.widget_size(widget),
            resize: false,
            before: self.layout_editor.state(),
        });
    }
    pub(super) fn layout_resize_press(&mut self, widget: Widget) {
        self.layout_press(widget);
        if let Some(drag) = &mut self.layout_editor.drag {
            drag.resize = true;
        }
    }
    fn put_layout_size(&mut self, widget: Widget, requested: [f32; 2]) {
        let point = self.position_from(widget, &self.layout_editor.draft);
        let size = crate::layout::resize(
            widget,
            point,
            requested,
            self.screen_size,
            self.layout_editor.grid.snap,
        );
        self.layout_editor.sizes.insert(widget.key().into(), size);
        self.layout_editor.draft.insert(
            widget.key().into(),
            crate::layout_state::encode_position(widget, point, size, self.screen_size),
        );
    }
    pub(super) fn layout_resize(&mut self, direction: [i8; 2], large: bool) {
        let Some(widget) = self.layout_editor.selected else {
            return;
        };
        let before = self.layout_editor.state();
        let size = self.widget_size(widget);
        let step = (if self.layout_editor.grid.snap {
            12.
        } else {
            8.
        }) * if large { 4. } else { 1. };
        self.put_layout_size(
            widget,
            [0, 1].map(|axis| size[axis] + f32::from(direction[axis]) * step),
        );
        self.layout_editor.remember(before);
    }
    pub(super) fn layout_automatic(&mut self) {
        let Some(widget) = self.layout_editor.selected else {
            return;
        };
        let before = self.layout_editor.state();
        let point = self.position_from(widget, &self.layout_editor.draft);
        self.layout_editor.sizes.remove(widget.key());
        self.layout_editor.draft.insert(
            widget.key().into(),
            crate::layout_state::encode_position(
                widget,
                point,
                self.widget_size(widget),
                self.screen_size,
            ),
        );
        self.layout_editor.remember(before);
    }
    pub(super) fn layout_undo(&mut self) {
        self.cancel_layout_drag();
        if let Some(state) = self.layout_editor.history.pop() {
            self.layout_editor.restore(state);
        }
    }
    /// Which widgets the saved settings show now.
    fn shown_now(&self) -> BTreeMap<String, bool> {
        Widget::ALL
            .into_iter()
            .map(|widget| (widget.key().into(), self.config.widget_visible(widget)))
            .collect()
    }
    /// The hidden widgets Add widget can put on the display being arranged.
    pub(super) fn addable_widgets(&self) -> Vec<Widget> {
        if !self.config.widgets_visible {
            return Vec::new();
        }
        Widget::ALL
            .into_iter()
            .filter(|widget| {
                !self.config.widget_visible(*widget)
                    && self.on_arranged_display(*widget)
                    // Daily overview shows these itself while it is on.
                    && !(self.config.glance.enabled
                        && matches!(widget, Widget::Clock | Widget::Weather | Widget::Calendar))
            })
            .collect()
    }
    /// Saves the draft. A widget it added or removed also gets or loses its
    /// surface and data, as with the widget switches.
    pub(super) fn commit_arrange_shown(&mut self) -> Task<Message> {
        let old = self.config.clone();
        self.commit_arrange();
        if Widget::ALL
            .into_iter()
            .all(|widget| old.widget_visible(widget) == self.config.widget_visible(widget))
        {
            return Task::none();
        }
        let new = std::mem::replace(&mut self.config, old);
        self.apply_config(new)
    }
    /// Puts `widget` on the desktop being arranged, or takes it off. Like a
    /// move, Undo brings back the widgets as they were and Cancel restores
    /// the ones shown when Arrange opened.
    pub(super) fn layout_show(&mut self, widget: Widget, visible: bool) -> Task<Message> {
        if !self.layout_editor.active
            || (visible && !self.addable_widgets().contains(&widget))
            || (!visible && !self.arranging_widget(widget))
        {
            return Task::none();
        }
        self.cancel_layout_drag();
        self.layout_editor.adding = false;
        let before = self.layout_editor.state();
        let was: Vec<Widget> = Widget::ALL
            .into_iter()
            .filter(|w| self.arranging_widget(*w))
            .collect();
        self.layout_editor
            .shown
            .insert(widget.key().into(), visible);
        if self.layout_editor.selected == Some(widget) {
            self.layout_editor.selected = None;
        }
        let task = self.commit_arrange_shown();
        // The new widget, and any that came back with it (the clock, when
        // Daily overview goes), take a free spot rather than cover others.
        let mut draft = self.layout_editor.draft.clone();
        for new in PLACEMENT_ORDER {
            if was.contains(&new) || !self.arranging_widget(new) {
                continue;
            }
            let others: Vec<[f32; 4]> = PLACEMENT_ORDER
                .into_iter()
                .filter(|other| *other != new && self.arranging_widget(*other))
                .map(|other| {
                    let at = self.position_from(other, &draft);
                    let size = self.widget_size(other);
                    [at[0], at[1], size[0], size[1]]
                })
                .collect();
            let size = self.widget_size(new);
            let at = self.position_from(new, &draft);
            let free = crate::layout::free_spot(at, size, &others, self.screen_size).unwrap_or(at);
            if free != at || draft.contains_key(new.key()) {
                draft.insert(
                    new.key().into(),
                    crate::layout_state::encode_position(new, free, size, self.screen_size),
                );
            }
        }
        if draft != self.layout_editor.draft {
            self.layout_editor.draft = draft;
            self.commit_arrange();
        }
        if visible && self.arranging_widget(widget) {
            self.layout_editor.selected = Some(widget);
        }
        self.layout_editor.remember(before);
        task
    }
    /// Opens `widget`'s settings. Arrange and its draft stay as they are;
    /// the overlay steps behind windows so Settings can be seen, and comes
    /// back to the front when clicked.
    pub(super) fn layout_settings(&mut self, widget: Widget) -> Task<Message> {
        self.cancel_layout_drag();
        self.layout_editor.adding = false;
        self.layout_editor.selected = Some(widget);
        let open = self.launch(
            crate::paths::CONTROLLER,
            vec!["settings".into(), "--page".into(), widget.key().into()],
        );
        if self.layout_editor.aside {
            return open;
        }
        self.layout_editor.aside = true;
        Task::batch([
            set_layer(self.ids[5], Layer::Bottom),
            set_keyboard_interactivity(self.ids[5], KeyboardInteractivity::None),
            open,
        ])
    }
    /// Brings the overlay back in front, with the keyboard, after Settings.
    pub(super) fn layout_front(&mut self) -> Task<Message> {
        if !self.layout_editor.active || !self.layout_editor.aside {
            return Task::none();
        }
        self.layout_editor.aside = false;
        Task::batch([
            set_layer(self.ids[5], Layer::Overlay),
            set_keyboard_interactivity(self.ids[5], KeyboardInteractivity::Exclusive),
        ])
    }
    pub(super) fn layout_resize_mode(&mut self) {
        self.layout_editor.resize_mode = !self.layout_editor.resize_mode;
    }

    /// Moves widgets in `positions` clear of each other. With `moving`, only
    /// that widget moves (the one just dropped). Otherwise later widgets in
    /// `PLACEMENT_ORDER` give way to earlier ones, and to `first` (one just
    /// grown) before all of them.
    fn keep_apart(
        &self,
        positions: &mut BTreeMap<String, [f32; 2]>,
        moving: Option<Widget>,
        first: Option<Widget>,
    ) {
        let rect = |widget: Widget, positions: &BTreeMap<String, [f32; 2]>| {
            let size = self.widget_size(widget);
            let at = self.position_from(widget, positions);
            [at[0], at[1], size[0], size[1]]
        };
        let visible: Vec<Widget> = PLACEMENT_ORDER
            .into_iter()
            .filter(|widget| self.arranging_widget(*widget))
            .collect();
        let place = |widget: Widget,
                     others: &[[f32; 4]],
                     positions: &mut BTreeMap<String, [f32; 2]>| {
            let size = self.widget_size(widget);
            let at = self.position_from(widget, positions);
            // No room: it stays where it was put; the desktop keeps it off
            // screen until there is.
            let free = crate::layout::free_spot(at, size, others, self.screen_size).unwrap_or(at);
            if free != at && positions.contains_key(widget.key()) {
                positions.insert(
                    widget.key().into(),
                    crate::layout_state::encode_position(widget, free, size, self.screen_size),
                );
            }
        };
        match moving {
            Some(widget) => {
                let others: Vec<[f32; 4]> = visible
                    .iter()
                    .filter(|other| **other != widget)
                    .map(|other| rect(*other, positions))
                    .collect();
                place(widget, &others, positions);
            }
            None => {
                let mut placed = Vec::new();
                let order = first.into_iter().chain(
                    visible
                        .iter()
                        .copied()
                        .filter(|widget| Some(*widget) != first),
                );
                for widget in order {
                    if Some(widget) != first {
                        place(widget, &placed, positions);
                    }
                    placed.push(rect(widget, positions));
                }
            }
        }
    }
    fn put_layout_point(&mut self, widget: Widget, point: [f32; 2]) {
        let point = if self.layout_editor.grid.snap {
            crate::layout::snap(
                point,
                if self.explicit_size(widget).is_some() {
                    self.widget_size(widget)
                } else {
                    crate::layout_state::placement_size(widget, self.widget_size(widget))
                },
                self.screen_size,
                self.layout_editor.grid.columns,
            )
        } else {
            point
        };
        let point = crate::layout_state::encode_position(
            widget,
            point,
            self.widget_size(widget),
            self.screen_size,
        );
        self.layout_editor.draft.insert(widget.key().into(), point);
        self.layout_editor.error = None;
    }
    pub(super) fn layout_pointer(&mut self, event: mouse::Event) -> Task<Message> {
        match event {
            mouse::Event::CursorMoved { position } => {
                self.layout_editor.pointer = position;
                if let Some(drag) = &self.layout_editor.drag {
                    let widget = drag.widget;
                    if drag.resize {
                        let size = [
                            drag.size[0] + position.x - drag.start[0],
                            drag.size[1] + position.y - drag.start[1],
                        ];
                        self.put_layout_size(widget, size);
                    } else {
                        let size = self.widget_size(widget);
                        let placement = if self.explicit_size(widget).is_some() {
                            size
                        } else {
                            crate::layout_state::placement_size(widget, size)
                        };
                        let point = crate::layout::drag(
                            [position.x, position.y],
                            drag.offset,
                            placement,
                            self.screen_size,
                        );
                        self.put_layout_point(widget, point);
                    }
                }
            }
            mouse::Event::CursorLeft => self.cancel_layout_drag(),
            mouse::Event::ButtonReleased(mouse::Button::Left) => {
                if let Some(drag) = self.layout_editor.drag.take() {
                    // A widget dropped or grown onto another moves to the
                    // nearest free spot; widgets never stack.
                    // A grown one keeps its place and pushes neighbors aside.
                    let mut draft = std::mem::take(&mut self.layout_editor.draft);
                    self.keep_apart(
                        &mut draft,
                        Some(drag.widget).filter(|_| !drag.resize),
                        Some(drag.widget).filter(|_| drag.resize),
                    );
                    self.layout_editor.draft = draft;
                    self.layout_editor.remember(drag.before);
                    self.commit_arrange();
                }
            }
            _ => {}
        }
        Task::none()
    }
    pub(super) fn cancel_layout_drag(&mut self) {
        if let Some(drag) = self.layout_editor.drag.take() {
            self.layout_editor.restore(drag.before);
        }
    }
    /// Every card surface again, then every tool surface: new card surfaces
    /// stack above older ones, and the tools belong on top, clickable.
    pub(super) fn rebuild_cards(&mut self) -> Task<Message> {
        let rebuild = self.recreate_widget_surfaces();
        self.forget_tool_surfaces();
        let tools = Task::batch(self.sync_tool_surfaces());
        rebuild.chain(tools)
    }
    fn recreate_widget_surfaces(&mut self) -> Task<Message> {
        if !self.output_known || self.selected_output.is_none() {
            return Task::none();
        }
        self.surface_geometry = Some(self.surface_geometry());
        self.surface_inputs = Some(Widget::BASE.map(|widget| self.card_input_rects(widget)));
        // Clock last, so it stacks above the other cards.
        let mut create = Vec::new();
        let mut clock = Task::none();
        for widget in [
            Widget::Calendar,
            Widget::Weather,
            Widget::Devbox,
            Widget::Projects,
            Widget::Glance,
            Widget::Clock,
        ] {
            if !self.card_surface_open(widget) {
                continue;
            }
            let surface = get_layer_surface(self.card_surface(widget));
            if widget == Widget::Clock {
                clock = surface;
            } else {
                create.push(surface);
            }
        }
        let destroy =
            Task::batch([0, 1, 4, 6, 7, 8].map(|index| destroy_layer_surface(self.ids[index])));
        destroy
            .chain(Task::batch(create))
            .chain(clock)
            .chain(self.blur_surfaces())
    }
    /// A card's surface as it should be created now.
    fn card_surface(&self, widget: Widget) -> SctkLayerSurfaceSettings {
        let id = self.widget_id(widget);
        let size = self.widget_size(widget);
        // The surface reaches past the card by the style's shadow margin
        // so a shadow isn't clipped; the card keeps its size and place.
        let (mi, mu) = self.shadow_px();
        let mut surface = SctkLayerSurfaceSettings {
            id,
            output: self.widget_layer_output(widget),
            layer: Layer::Bottom,
            keyboard_interactivity: KeyboardInteractivity::None,
            anchor: Anchor::TOP | Anchor::LEFT,
            exclusive_zone: 0,
            input_zone: Some(zone(&self.card_input_rects(widget))),
            namespace: format!("halcyon-{}", widget.key()),
            size: Some((
                Some(size[0].ceil() as u32 + mu),
                Some(size[1].ceil() as u32 + mu),
            )),
            ..Default::default()
        };
        if self.custom_layout() || widget == Widget::Glance {
            let position = if self.config.widget_visible(widget) {
                self.placed_position(widget)
            } else {
                [-10000., 0.]
            };
            surface.margin = IcedMargin {
                top: position[1].round() as i32 - mi,
                left: position[0].round() as i32 - mi,
                ..Default::default()
            };
        } else {
            match widget {
                Widget::Clock => {
                    surface.size = Some((Some(490 + mu), Some(250 + mu)));
                    surface.margin = IcedMargin {
                        top: 88 - mi,
                        left: self.config.clock_margin() - mi,
                        ..Default::default()
                    };
                }
                Widget::Weather => {
                    surface.anchor = Anchor::TOP | Anchor::BOTTOM | Anchor::RIGHT;
                    surface.size = Some((Some(360 + mu), Some(0)));
                    surface.margin = IcedMargin {
                        top: 48 - mi,
                        bottom: 48 - mi,
                        right: 42 - mi,
                        ..Default::default()
                    };
                }
                Widget::Calendar => {
                    surface.anchor = Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT;
                    surface.size = Some((Some(360 + mu), Some(0)));
                    surface.margin = IcedMargin {
                        top: 48 - mi,
                        bottom: 48 - mi,
                        left: self.config.calendar_margin() - mi,
                        ..Default::default()
                    };
                }
                _ => {
                    surface.size = Some((Some(16), Some(16)));
                    surface.margin = IcedMargin {
                        left: -10000,
                        ..Default::default()
                    };
                }
            }
        }
        if self.layout_editor.active && self.arranging_widget(widget) {
            if !self.custom_layout() && widget == Widget::Weather {
                surface.margin.right = -10000;
            } else {
                surface.margin.left = -10000;
            }
        }
        surface
    }
    pub(super) fn layout_key(&mut self, key: keyboard::Key, shift: bool) -> Task<Message> {
        if !self.layout_editor.active {
            return Task::none();
        }
        use keyboard::key::Named;
        match key {
            keyboard::Key::Named(Named::Escape) if self.layout_editor.drag.is_some() => {
                self.cancel_layout_drag()
            }
            keyboard::Key::Named(Named::Escape) if self.layout_editor.adding => {
                self.layout_editor.adding = false
            }
            keyboard::Key::Named(Named::Escape) => return self.finish_arrange(false),
            keyboard::Key::Character(ref value) if value.as_str() == "r" => {
                self.layout_resize_mode()
            }
            keyboard::Key::Character(ref value) if value.as_str() == "z" => {
                self.layout_undo();
                return self.commit_arrange_shown();
            }
            keyboard::Key::Named(Named::Tab) => {
                let widgets: Vec<_> = Widget::ALL
                    .into_iter()
                    .filter(|w| self.arranging_widget(*w))
                    .collect();
                if !widgets.is_empty() {
                    let index = self
                        .layout_editor
                        .selected
                        .and_then(|w| widgets.iter().position(|x| *x == w))
                        .map_or(0, |i| (i + 1) % widgets.len());
                    self.layout_editor.selected = Some(widgets[index]);
                }
            }
            keyboard::Key::Named(Named::Enter) => return self.finish_arrange(true),
            keyboard::Key::Named(named) => {
                if let Some(widget) = self.layout_editor.selected {
                    if self.layout_editor.resize_mode {
                        let direction = match named {
                            Named::ArrowLeft => [-1, 0],
                            Named::ArrowRight => [1, 0],
                            Named::ArrowUp => [0, -1],
                            Named::ArrowDown => [0, 1],
                            _ => return Task::none(),
                        };
                        self.layout_resize(direction, shift);
                        self.commit_arrange();
                        return Task::none();
                    }
                    let before = self.layout_editor.state();
                    let mut point = self.position_from(widget, &self.layout_editor.draft);
                    let step = if self.layout_editor.grid.snap {
                        crate::layout::grid_step(self.screen_size, self.layout_editor.grid.columns)
                            .map(|value| value * if shift { 4. } else { 1. })
                    } else {
                        [if shift { 32. } else { 8. }; 2]
                    };
                    match named {
                        Named::ArrowLeft => point[0] -= step[0],
                        Named::ArrowRight => point[0] += step[0],
                        Named::ArrowUp => point[1] -= step[1],
                        Named::ArrowDown => point[1] += step[1],
                        _ => return Task::none(),
                    }
                    self.put_layout_point(widget, point);
                    self.layout_editor.remember(before);
                    self.commit_arrange();
                }
            }
            _ => {}
        }
        Task::none()
    }
    /// A card that opens its menu on a right click.
    pub(super) fn with_menu<'a>(
        &self,
        widget: Widget,
        card: Element<'a, Message>,
    ) -> Element<'a, Message> {
        if self.layout_editor.active {
            return card;
        }
        mouse_area(card)
            .on_right_press(Message::WidgetMenu(widget))
            .into()
    }
    /// The surface showing `widget` now.
    fn surface_of(&self, widget: Widget) -> window::Id {
        if self.custom_layout()
            || !matches!(widget, Widget::Weather | Widget::Devbox | Widget::Projects)
        {
            self.widget_id(widget)
        } else {
            self.ids[1]
        }
    }
    pub(super) fn open_widget_menu(&mut self, widget: Widget) -> Task<Message> {
        let close = self.close_widget_menu();
        let fallback = self.surface_of(widget);
        let (parent, at) = match POINTER.lock().ok().and_then(|pointer| *pointer) {
            Some((id, at)) if self.ids.contains(&id) => (id, at),
            _ => (fallback, Point::new(16., 16.)),
        };
        let id = window::Id::unique();
        self.menu = Some((id, widget));
        let height = MENU_ROW * 3 + MENU_PADDING * 2;
        let popup = get_popup(SctkPopupSettings {
            parent,
            id,
            positioner: SctkPositioner {
                size: Some((MENU_WIDTH, height)),
                size_limits: cosmic::iced::Limits::NONE
                    .width(MENU_WIDTH as f32)
                    .height(height as f32),
                anchor_rect: cosmic::iced::Rectangle {
                    x: at.x as i32,
                    y: at.y as i32,
                    width: 1,
                    height: 1,
                },
                anchor: PopupAnchor::BottomRight,
                gravity: Gravity::BottomRight,
                // Slide, then flip, to stay on screen near the pointer.
                constraint_adjustment: 15,
                offset: (0, 0),
                reactive: true,
            },
            parent_size: None,
            grab: true,
            close_with_children: false,
            input_zone: None,
        });
        Task::batch([close, popup])
    }
    pub(super) fn close_widget_menu(&mut self) -> Task<Message> {
        match self.menu.take() {
            Some((id, _)) => destroy_popup(id),
            None => Task::none(),
        }
    }
    pub(super) fn widget_menu_pick(&mut self, pick: MenuPick) -> Task<Message> {
        let Some((_, widget)) = self.menu else {
            return Task::none();
        };
        let close = self.close_widget_menu();
        let next = self.update(match pick {
            MenuPick::Settings => Message::WidgetSettings(widget),
            MenuPick::Arrange => Message::Arrange,
            MenuPick::Hide => Message::SetWidgetVisible(widget, false),
        });
        Task::batch([close, next])
    }
    pub(super) fn widget_menu_view(&self, widget: Widget) -> Element<'_, Message> {
        let item = |label: String, id: &'static str, pick: MenuPick| -> Element<'_, Message> {
            cosmic::widget::button::custom(
                container(text::body(label))
                    .height(Length::Fill)
                    .align_y(Alignment::Center),
            )
            .id(id.into())
            .class(cosmic::theme::Button::MenuItem)
            .padding([0, 12])
            .width(Length::Fill)
            .height(Length::Fixed(MENU_ROW as f32))
            .on_press(Message::WidgetMenuPick(pick))
            .into()
        };
        let (settings, hide) = widget_menu_labels(widget);
        container(column::with_children(vec![
            item(settings, "widget-menu-settings", MenuPick::Settings),
            item(
                "Arrange widgets".into(),
                "widget-menu-arrange",
                MenuPick::Arrange,
            ),
            item(hide, "widget-menu-hide", MenuPick::Hide),
        ]))
        .padding(MENU_PADDING as u16)
        .width(Length::Fill)
        .height(Length::Fill)
        .class(cosmic::theme::Container::Dropdown)
        .into()
    }
    pub(super) fn individual_widget(&self, widget: Widget) -> Element<'_, Message> {
        if !self.config.widget_visible(widget) || !self.widget_available(widget) {
            return empty_surface();
        }
        let card = match widget {
            Widget::Clock => self.clock(),
            Widget::Weather => self.weather_card(),
            Widget::Devbox => self.devbox(),
            Widget::Projects => self.projects_card(),
            Widget::Calendar => self.calendar_card(),
            Widget::Glance => self.glance(),
            Widget::Music | Widget::Notes | Widget::Focus | Widget::Devices => {
                self.tool_card(widget)
            }
        };
        let card = self.with_menu(widget, card);
        // Matches the surface's shadow margin (see the layer surface sizes).
        let m = crate::widget_style::shadow_margin(&self.config);
        if m > 0. {
            container(card).padding(m).into()
        } else {
            card
        }
    }
    pub(super) fn layout_view(&self) -> Element<'_, Message> {
        if !self.layout_editor.active {
            return empty_surface();
        }
        let p = self.config.surface_palette();
        let background = container(text::body(""))
            .width(Length::Fill)
            .height(Length::Fill)
            .class(cosmic::theme::Container::custom(|_| {
                cosmic::iced::widget::container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.04, 0.04, 0.06, 0.82))),
                    ..Default::default()
                }
            }));
        let mut layers: Vec<Element<'_, Message>> = vec![background.into()];
        if self.layout_editor.grid.snap {
            let guides = crate::layout::grid_lines(self.screen_size);
            for (axis, lines) in guides.iter().enumerate() {
                for &coordinate in lines {
                    let position = if axis == 0 {
                        Point::new(coordinate, 48.)
                    } else {
                        Point::new(24., coordinate)
                    };
                    let line = container(text::body(""))
                        .width(if axis == 0 {
                            1.
                        } else {
                            (self.screen_size[0] - 48.).max(1.)
                        })
                        .height(if axis == 0 {
                            (self.screen_size[1] - 96.).max(1.)
                        } else {
                            1.
                        })
                        .class(cosmic::theme::Container::custom(move |_| {
                            cosmic::iced::widget::container::Style {
                                background: Some(Background::Color(Color {
                                    a: 0.12,
                                    ..p.accent
                                })),
                                ..Default::default()
                            }
                        }));
                    layers.push(pin(line).position(position).into());
                }
            }
        }
        let (mi, mu) = self.shadow_px();
        // Every card first, then every card's chrome, so a neighbor never
        // covers a handle that reaches past its card's edge.
        let mut chrome_layers: Vec<Element<'_, Message>> = Vec::new();
        for widget in Widget::ALL {
            if !self.arranging_widget(widget) {
                continue;
            }
            let size = self.widget_size(widget);
            let at = self.arrange_origin(
                widget,
                self.position_from(widget, &self.layout_editor.draft),
            );
            let selected = self.layout_editor.selected == Some(widget);
            // Everything for one card shares a box that reaches past the
            // card on every side, so the editing chrome can sit on its edges
            // and the card keeps its desktop surface's shadow margin.
            let pad = (mi as f32).max(CHROME_OUTSET);
            let [width, height] = size;
            // The card exactly as its desktop surface draws it: the same
            // element in a box the surface's size, at the surface's place.
            // The default calendar column also leaves the agenda the room
            // below the card, as on the desktop.
            let room = match self.calendar_column_top() {
                Some(top) if widget == Widget::Calendar => {
                    (48. + self.calendar_height - top).max(height)
                }
                _ => height,
            };
            let card = container(self.individual_widget(widget))
                .width(width.ceil() + mu as f32)
                .height(room.ceil() + mu as f32);
            let mut card_layers = vec![
                pin(card)
                    .position(Point::new(pad - mi as f32, pad - mi as f32))
                    .into(),
            ];
            // An outline just outside the card. Pressing anywhere within it
            // starts a drag; it never draws over the card's own content.
            let ring = 3.;
            let radius = crate::widget_style::corner_radius(&self.config) + ring;
            let outline = opaque(
                mouse_area(
                    container(cosmic::iced::widget::space())
                        .id(cosmic::widget::Id::new(format!(
                            "arrange-card-{}",
                            widget.key()
                        )))
                        .width(width + 2. * ring)
                        .height(height + 2. * ring)
                        .class(cosmic::theme::Container::custom(move |_| {
                            cosmic::iced::widget::container::Style {
                                border: Border {
                                    color: Color {
                                        a: if selected { 0.95 } else { 0.45 },
                                        ..p.accent
                                    },
                                    width: if selected { 2. } else { 1. },
                                    radius: radius.into(),
                                },
                                ..Default::default()
                            }
                        })),
                )
                .on_press(Message::LayoutPress(widget))
                .interaction(mouse::Interaction::Grab),
            );
            card_layers.push(
                pin(outline)
                    .position(Point::new(pad - ring, pad - ring))
                    .into(),
            );
            let size_box = [width.ceil() + 2. * pad, height.ceil() + 2. * pad];
            let origin = Point::new(at[0] - pad, at[1] - pad);
            layers.push(
                pin(stack(card_layers)
                    .width(size_box[0])
                    .height(room.ceil() + 2. * pad))
                .position(origin)
                .into(),
            );
            let mut card_layers: Vec<Element<'_, Message>> = Vec::new();
            // The name sits on the top edge.
            let name = opaque(
                mouse_area(
                    container(label(format!("⠿  {}", widget.title()), 11., p.accent))
                        .id(cosmic::widget::Id::new(format!(
                            "arrange-name-{}",
                            widget.key()
                        )))
                        .height(CHROME_TAB)
                        .padding([0, 8])
                        .align_y(Alignment::Center)
                        .class(cosmic::theme::Container::custom(move |_| chrome_style(p))),
                )
                .on_press(Message::LayoutPress(widget))
                .interaction(mouse::Interaction::Grab),
            );
            card_layers.push(
                pin(name)
                    .position(Point::new(pad + 14., pad - CHROME_OUTSET))
                    .into(),
            );
            // The resize handle straddles the lower-right corner.
            let grip = opaque(
                mouse_area(
                    container(label("◢", 11., p.accent))
                        .id(cosmic::widget::Id::new(format!(
                            "arrange-grip-{}",
                            widget.key()
                        )))
                        .center_x(CHROME_GRIP)
                        .center_y(CHROME_GRIP)
                        .class(cosmic::theme::Container::custom(move |_| chrome_style(p))),
                )
                .on_press(Message::LayoutResizePress(widget))
                .interaction(mouse::Interaction::ResizingDiagonallyDown),
            );
            card_layers.push(
                pin(grip)
                    .position(Point::new(
                        pad + width + CHROME_OUTSET - CHROME_GRIP,
                        pad + height + CHROME_OUTSET - CHROME_GRIP,
                    ))
                    .into(),
            );
            // Last, so a click on them never starts a drag. They straddle
            // the top edge at the right.
            let controls = row::with_children(vec![
                card_control(
                    p,
                    "emblem-system-symbolic",
                    format!("arrange-settings-{}", widget.key()),
                    Message::WidgetSettings(widget),
                ),
                card_control(
                    p,
                    "window-close-symbolic",
                    format!("arrange-remove-{}", widget.key()),
                    Message::LayoutShow(widget, false),
                ),
            ])
            .spacing(CARD_CONTROL_GAP);
            card_layers.push(
                pin(opaque(controls))
                    .position(Point::new(
                        (pad + width - 14. - 2. * CARD_CONTROL - CARD_CONTROL_GAP).max(pad),
                        pad - CHROME_OUTSET,
                    ))
                    .into(),
            );
            chrome_layers.push(
                pin(stack(card_layers).width(size_box[0]).height(size_box[1]))
                    .position(origin)
                    .into(),
            );
        }
        layers.extend(chrome_layers);
        let title = column::with_children(vec![
            label("Arrange widgets", 17., p.foreground),
            label(
                self.layout_editor
                    .error
                    .as_deref()
                    .unwrap_or("Drag to move · corner to resize · R: resize with arrows"),
                11.,
                p.muted,
            ),
        ])
        .spacing(3);
        let addable = self.addable_widgets();
        let snap = row::with_children(vec![
            label("Snap to grid", 12., p.muted),
            cosmic::widget::toggler(self.layout_editor.grid.snap)
                .on_toggle(Message::LayoutSnap)
                .into(),
        ])
        .spacing(8)
        .align_y(Alignment::Center);
        let actions = vec![
            cosmic::widget::button::text(if addable.is_empty() {
                "All widgets added"
            } else {
                "Add widget"
            })
            .id(cosmic::widget::Id::new("arrange-add"))
            .leading_icon(cosmic::widget::icon::from_name("list-add-symbolic").size(14))
            .on_press_maybe((!addable.is_empty()).then_some(Message::LayoutAddMenu))
            .into(),
            cosmic::widget::button::text("Undo")
                .id(cosmic::widget::Id::new("arrange-undo"))
                .on_press_maybe(
                    (!self.layout_editor.history.is_empty()).then_some(Message::LayoutUndo),
                )
                .into(),
            cosmic::widget::button::text("Reset layout")
                .on_press(Message::LayoutDefaults)
                .into(),
            cosmic::widget::button::text("Cancel")
                .id(cosmic::widget::Id::new("arrange-cancel"))
                .on_press(Message::LayoutFinish(false))
                .into(),
            cosmic::widget::button::suggested("Done")
                .id(cosmic::widget::Id::new("arrange-done"))
                .on_press(Message::LayoutFinish(true))
                .into(),
        ];
        let toolbar_width = TOOLBAR_WIDTH.min(self.screen_size[0] - 16.).max(0.);
        // Narrow displays put the buttons under the title.
        let mut rows: Vec<Element<'_, Message>> = if toolbar_width >= TOOLBAR_ONE_LINE {
            let mut line = vec![container(title).width(Length::Fill).into(), snap.into()];
            line.extend(actions);
            vec![
                row::with_children(line)
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into(),
            ]
        } else {
            vec![
                row::with_children(vec![
                    container(title).width(Length::Fill).into(),
                    snap.into(),
                ])
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
                cosmic::widget::flex_row(actions)
                    .row_spacing(6)
                    .column_spacing(10)
                    .into(),
            ]
        };
        if self.layout_editor.adding && !addable.is_empty() {
            let mut chips = vec![label("Add to this desktop:", 12., p.muted)];
            chips.extend(addable.iter().map(|widget| {
                cosmic::widget::button::text(widget.title())
                    .id(cosmic::widget::Id::new(format!(
                        "arrange-add-{}",
                        widget.key()
                    )))
                    .leading_icon(cosmic::widget::icon::from_name(widget.icon()).size(14))
                    .on_press(Message::LayoutShow(*widget, true))
                    .into()
            }));
            rows.push(
                cosmic::widget::flex_row(chips)
                    .row_spacing(6)
                    .column_spacing(6)
                    .align_items(Alignment::Center)
                    .into(),
            );
        }
        let toolbar = container(column::with_children(rows).spacing(10))
            .padding([12, 18])
            .width(toolbar_width)
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    background: Some(Background::Color(Color {
                        a: 1.,
                        ..p.background
                    })),
                    border: Border {
                        radius: 16.into(),
                        color: Color { a: 0.3, ..p.accent },
                        width: 1.,
                    },
                    ..Default::default()
                }
            }));
        layers.push(
            pin(opaque(toolbar))
                .position(Point::new(
                    ((self.screen_size[0] - toolbar_width) / 2.).max(0.),
                    8.,
                ))
                .into(),
        );
        let inspector: Element<'_, Message> = if let Some(widget) = self.layout_editor.selected {
            let size = self.widget_size(widget);
            cosmic::widget::flex_row(vec![
                label(
                    format!(
                        "{} · {} × {}",
                        widget.title(),
                        size[0].round() as u32,
                        size[1].round() as u32
                    ),
                    13.,
                    p.foreground,
                ),
                cosmic::widget::button::text(if self.layout_editor.resize_mode {
                    "Arrows: resize"
                } else {
                    "Arrows: move"
                })
                .on_press(Message::LayoutResizeMode)
                .into(),
                cosmic::widget::button::text("W −")
                    .on_press(Message::LayoutResize([-1, 0]))
                    .into(),
                cosmic::widget::button::text("W +")
                    .on_press(Message::LayoutResize([1, 0]))
                    .into(),
                cosmic::widget::button::text("H −")
                    .on_press(Message::LayoutResize([0, -1]))
                    .into(),
                cosmic::widget::button::text("H +")
                    .on_press(Message::LayoutResize([0, 1]))
                    .into(),
                cosmic::widget::button::text("Automatic")
                    .on_press(Message::LayoutAutomatic)
                    .into(),
                cosmic::widget::button::text("Settings")
                    .id(cosmic::widget::Id::new("arrange-inspector-settings"))
                    .leading_icon(
                        cosmic::widget::icon::from_name("emblem-system-symbolic").size(14),
                    )
                    .on_press(Message::WidgetSettings(widget))
                    .into(),
                cosmic::widget::button::destructive("Remove")
                    .id(cosmic::widget::Id::new("arrange-inspector-remove"))
                    .on_press(Message::LayoutShow(widget, false))
                    .into(),
            ])
            .row_spacing(6)
            .column_spacing(6)
            .align_items(Alignment::Center)
            .into()
        } else {
            cosmic::widget::flex_row(
                Widget::ALL
                    .into_iter()
                    .filter(|w| self.arranging_widget(*w))
                    .map(|w| {
                        cosmic::widget::button::text(w.title())
                            .on_press(Message::LayoutSelect(w))
                            .into()
                    })
                    .collect::<Vec<_>>(),
            )
            .row_spacing(4)
            .column_spacing(4)
            .into()
        };
        // Along the bottom edge; on a narrow display it wraps upwards.
        layers.push(
            container(opaque(
                container(inspector)
                    .padding(8)
                    .max_width((self.screen_size[0] - 48.).max(0.))
                    .class(cosmic::theme::Container::custom(move |_| {
                        cosmic::iced::widget::container::Style {
                            background: Some(Background::Color(Color {
                                a: 1.,
                                ..p.background
                            })),
                            border: Border {
                                radius: 12.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    })),
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::End)
            .padding([0., 24., 2., 24.])
            .into(),
        );
        if self.layout_editor.aside {
            // Settings is open in front. The first click anywhere here brings
            // Arrange back, without also moving a widget.
            let note = container(label(
                "Widget settings are open. Click anywhere here to keep arranging.",
                13.,
                p.foreground,
            ))
            .padding([10, 16])
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    background: Some(Background::Color(Color {
                        a: 1.,
                        ..p.background
                    })),
                    border: Border {
                        radius: 12.into(),
                        color: Color { a: 0.6, ..p.accent },
                        width: 1.,
                    },
                    ..Default::default()
                }
            }));
            layers.push(
                opaque(
                    mouse_area(
                        container(note)
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .center_x(Length::Fill)
                            .center_y(Length::Fill),
                    )
                    .on_press(Message::LayoutFront),
                )
                .into(),
            );
        }
        stack(layers)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
    pub(super) fn layout_defaults(&mut self) {
        self.cancel_layout_drag();
        let before = self.layout_editor.state();
        let visible: Vec<_> = Widget::ALL
            .into_iter()
            .filter(|w| self.arranging_widget(*w))
            .collect();
        self.layout_editor
            .sizes
            .retain(|key, _| Widget::parse(key).is_none_or(|w| !visible.contains(&w)));
        self.layout_editor
            .draft
            .retain(|key, _| Widget::parse(key).is_none_or(|widget| !visible.contains(&widget)));
        if self.arranging_widget(Widget::Glance) {
            // Keep an explicit default so a later sync cannot mistake Reset
            // for Glance's first enable and copy the hidden Clock again.
            let point = crate::layout_state::encode_position(
                Widget::Glance,
                self.default_position(Widget::Glance),
                self.widget_size(Widget::Glance),
                self.screen_size,
            );
            self.layout_editor.draft.insert("glance".into(), point);
        }
        self.layout_editor.reset = true;
        self.layout_editor.drag = None;
        self.layout_editor.selected = None;
        self.layout_editor.remember(before);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::Application;
    #[test]
    fn decorative_finishes_only_clear_native_blur_without_changing_geometry() {
        use cosmic::iced::futures::StreamExt;
        use cosmic::iced::runtime::{Action, platform_specific, task};
        for finish in std::iter::once(None).chain(
            crate::materials::Kind::ALL.map(|kind| Some(crate::materials::Finish::new(kind))),
        ) {
            let mut config = Config::default();
            config.appearance.family = Some(crate::desktop_theme::Family::Frosted);
            config.materials.shared = finish;
            config.glance.enabled = true;
            config.glance.own_background = true;
            config.glance.style = crate::config::GlanceStyle::Transparent;
            let (cards, _) = Cards::init(Core::default(), config);
            let before: Vec<_> = Widget::ALL.map(|w| cards.card_input_rects(w)).into();
            let mut cleared = Vec::new();
            if let Some(mut stream) = task::into_stream(cards.blur_surfaces()) {
                cosmic::iced::futures::executor::block_on(async {
                    while let Some(action) = stream.next().await {
                        match action {
                            Action::PlatformSpecific(platform_specific::Action::Wayland(
                                platform_specific::wayland::Action::BlurSurface(id, region),
                            )) => {
                                assert!(region.is_none(), "{finish:?}");
                                cleared.push(id);
                            }
                            _ => panic!("clearing blur must not move or recreate a surface"),
                        }
                    }
                });
            }
            assert_eq!(cleared.len(), Widget::ALL.len());
            for (widget, input) in Widget::ALL.into_iter().zip(before) {
                assert!(cleared.contains(&cards.widget_id(widget)));
                assert_eq!(cards.card_input_rects(widget), input);
            }
            assert!(!cards.config.glance_panel());
            assert!(cards.core.auto_blur().is_empty());
        }
    }

    #[test]
    fn sized_native_cards_fill_their_surface_and_keep_busy_agendas_scrollable() {
        use crate::native_test::Native;
        use cosmic::iced::{Event, Size};
        let mut config = Config::default();
        config.music.enabled = true;
        config.notes.enabled = true;
        config.focus.enabled = true;
        config.glance.enabled = true;
        config.glance.style = crate::config::GlanceStyle::Panel;
        config.glance.initialized = true;
        let (mut cards, _) = Cards::init(Core::default(), config);
        let _output = cards.test_output_ready();
        cards.screen_size = [1920., 1080.];
        let today = Local::now().date_naive();
        let calendar = CalendarData {
            month: today.format("%Y-%m").to_string(),
            events: (0..24)
                .map(|n| CalendarEvent {
                    title: format!(
                        "Fixture event {n:02} · A long agenda entry wraps at the chosen width"
                    ),
                    start: today.to_string(),
                    end: (today + Days::days(1)).to_string(),
                    all_day: true,
                    url: String::new(),
                    meeting_url: None,
                    ..Default::default()
                })
                .collect(),
            ..CalendarData::default()
        };
        cards.calendar = Some(calendar.clone());
        cards.calendar_today = Some(calendar);
        cards.config.focus.long_break_every = 12;
        cards.tools.focus.phase = crate::tools::focus::Phase::LongBreak;
        cards.tools.focus.status = crate::tools::focus::Status::Paused { remaining: 600_000 };
        cards.tools.focus.rounds = 12;
        cards.tools.focus.day = today.to_string();
        let theme = cosmic::Theme::dark();
        for widget in [
            Widget::Weather,
            Widget::Devbox,
            Widget::Projects,
            Widget::Calendar,
            Widget::Music,
            Widget::Notes,
            Widget::Focus,
            Widget::Glance,
        ] {
            cards.config.glance.enabled = widget == Widget::Glance;
            for (name, size) in [
                ("minimum", crate::layout::minimum_size(widget)),
                ("large", [480., 360.]),
            ] {
                cards.config.widget_sizes.insert(widget.key().into(), size);
                let mut native = Native::new(Size::new(size[0], size[1]));
                let pixels = native.render(cards.individual_widget(widget), &theme);
                let index =
                    (((size[1] as usize - 6) * size[0] as usize) + size[0] as usize / 2) * 4;
                // Music too: with nothing playing it keeps its slot (DT-14).
                assert!(
                    pixels[index + 3] > 150,
                    "{widget:?} {name}: frame must fill its native surface"
                );
                native.capture(
                    cards.individual_widget(widget),
                    &theme,
                    &format!("card-{}-{name}", widget.key()),
                );
                if matches!(widget, Widget::Calendar | Widget::Glance) {
                    assert_eq!(cards.calendar.as_ref().unwrap().events.len(), 24);
                    let point = Point::new(size[0] / 2., size[1] - 80.);
                    let _ = native.event(
                        cards.individual_widget(widget),
                        Event::Mouse(mouse::Event::WheelScrolled {
                            delta: mouse::ScrollDelta::Pixels { x: 0., y: -10000. },
                        }),
                        point,
                    );
                    let scrolled = native.render(cards.individual_widget(widget), &theme);
                    assert!(
                        pixels != scrolled,
                        "{widget:?} {name}: agenda remains scrollable"
                    );
                    native.capture(
                        cards.individual_widget(widget),
                        &theme,
                        &format!("card-{}-{name}-last-events", widget.key()),
                    );
                }
            }
        }
        cards.config.widget_sizes.remove("glance");
        cards.config.glance.enabled = true;
        assert!(
            cards.glance_size()[1] > 360.,
            "Automatic restores downward growth"
        );
    }

    #[test]
    fn native_desktop_resize_moves_real_content_and_roundtrips_sizes() {
        use crate::native_test::Native;
        use cosmic::iced::{Event, Size};
        if std::env::var_os("DESKTOP_CARDS_NATIVE_LAYOUT_CHILD").is_none() {
            let root =
                std::env::temp_dir().join(format!("cards-native-layout-{}", std::process::id()));
            for part in ["config", "data", "cache", "state", "desktop/config/cosmic"] {
                std::fs::create_dir_all(root.join(part)).unwrap();
            }
            std::fs::write(root.join("config.json"), r#"{"enabled_widgets":["weather"],"music":{"enabled":true},"focus":{"enabled":true},"notes":{"enabled":true},"placement":{"snap":false},"widget_positions":{"weather":[0.05,0.1],"music":[0.5,0.45],"focus":[0.8,0.1],"notes":[0.1,0.85]}}"#).unwrap();
            let result = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact", "app::layout::tests::native_desktop_resize_moves_real_content_and_roundtrips_sizes", "--nocapture"])
                .env("DESKTOP_CARDS_NATIVE_LAYOUT_CHILD", "1").env("DESKTOP_CARDS_CONFIG", root.join("config.json"))
                .env("DESKTOP_CARDS_DESKTOP_ROOT", root.join("desktop"))
                .env("XDG_CONFIG_HOME", root.join("config")).env("XDG_DATA_HOME", root.join("data"))
                .env("XDG_CACHE_HOME", root.join("cache")).env("XDG_STATE_HOME", root.join("state"))
                .status().unwrap();
            let _ = std::fs::remove_dir_all(root);
            assert!(result.success());
            return;
        }
        fn pointer(cards: &mut Cards, native: &mut Native, point: Point, press: Option<bool>) {
            let event = match press {
                Some(true) => mouse::Event::ButtonPressed(mouse::Button::Left),
                Some(false) => mouse::Event::ButtonReleased(mouse::Button::Left),
                None => mouse::Event::CursorMoved { position: point },
            };
            let (messages, _) = native.event(cards.layout_view(), Event::Mouse(event), point);
            for message in messages {
                let _ = cards.update(message);
            }
            let _ = cards.layout_pointer(event);
        }
        let initial = std::fs::read(Config::path().unwrap()).unwrap();
        let (mut cards, _) = Cards::init(Core::default(), Config::load());
        let _output = cards.test_output_ready();
        cards.screen_size = [1280., 800.];
        let mut native = Native::new(Size::new(1280., 800.));
        let theme = cosmic::Theme::dark();
        let notes_before = cards.position_from(Widget::Notes, &cards.config.widget_positions);
        let _ = cards.arrange();
        for widget in [Widget::Weather, Widget::Music, Widget::Focus] {
            let position = cards.position_from(widget, &cards.layout_editor.draft);
            let size = cards.widget_size(widget);
            let grip = Point::new(position[0] + size[0] - 5., position[1] + size[1] - 5.);
            pointer(&mut cards, &mut native, grip, None);
            pointer(&mut cards, &mut native, grip, Some(true));
            assert!(
                cards.layout_editor.drag.as_ref().unwrap().resize,
                "native grip, {widget:?}"
            );
            let target = Point::new(grip.x + 24., grip.y + 80.);
            pointer(&mut cards, &mut native, target, None);
            pointer(&mut cards, &mut native, target, Some(false));
            assert_eq!(cards.widget_size(widget), [size[0] + 24., size[1] + 80.]);
            assert_eq!(
                cards.config.widget_sizes.get(widget.key()),
                Some(&[size[0] + 24., size[1] + 80.])
            );
            let top = cards.position_from(widget, &cards.layout_editor.draft);
            assert!((top[0] - position[0]).abs() < 0.001 && (top[1] - position[1]).abs() < 0.001);
        }
        native.capture(cards.layout_view(), &theme, "desktop-arranging-resized");
        assert_ne!(std::fs::read(Config::path().unwrap()).unwrap(), initial);
        let wanted = cards.layout_editor.sizes.clone();
        let _ = cards.finish_arrange(true);
        assert_eq!(Config::load().widget_sizes, wanted);
        let notes_after = cards.position_from(Widget::Notes, &cards.config.widget_positions);
        assert!((0..2).all(|axis| (notes_before[axis] - notes_after[axis]).abs() < 0.01));
        for widget in [Widget::Weather, Widget::Music, Widget::Focus] {
            let size = cards.widget_size(widget);
            let mut card = Native::new(Size::new(size[0], size[1]));
            card.capture(
                cards.individual_widget(widget),
                &theme,
                &format!("desktop-{}-resized", widget.key()),
            );
        }
        let _ = cards.arrange();
        cards.layout_editor.selected = Some(Widget::Music);
        cards.layout_resize_mode();
        let _ = cards.layout_key(keyboard::Key::Named(keyboard::key::Named::ArrowDown), false);
        assert_eq!(cards.widget_size(Widget::Music)[1], wanted["music"][1] + 8.);
        let _ = cards.layout_key(keyboard::Key::Named(keyboard::key::Named::Escape), false);
        assert_eq!(Config::load().widget_sizes, wanted);
        let _ = cards.arrange();
        cards.layout_defaults();
        assert!(cards.layout_editor.sizes.is_empty());
        let _ = cards.finish_arrange(true);
        assert!(Config::load().widget_sizes.is_empty());
    }

    #[test]
    fn widgets_on_the_desktop_never_stack() {
        // The reported layout: Focus saved almost exactly on top of Notes,
        // Projects across the system monitor.
        let mut config = Config::default();
        config.notes.enabled = true;
        config.focus.enabled = true;
        config.enabled_widgets = vec!["devbox".into(), "projects".into()];
        for (key, at) in [
            ("devbox", [1.0, 0.0]),
            ("projects", [0.984, 0.214]),
            ("notes", [1.0, 0.584]),
            ("focus", [1.0, 0.585]),
        ] {
            config.widget_positions.insert(key.into(), at);
        }
        let (mut cards, _) = Cards::init(Core::default(), config);
        let _output = cards.test_output_ready();
        cards.screen_size = [1920., 1039.];
        assert!(cards.custom_layout());
        let shown = [
            Widget::Devbox,
            Widget::Projects,
            Widget::Focus,
            Widget::Notes,
        ];
        let rects: Vec<[f32; 4]> = shown
            .iter()
            .map(|widget| {
                let at = cards.placed_position(*widget);
                let size = cards.widget_size(*widget);
                [at[0], at[1], size[0], size[1]]
            })
            .collect();
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                let apart = a[0] + a[2] <= b[0]
                    || b[0] + b[2] <= a[0]
                    || a[1] + a[3] <= b[1]
                    || b[1] + b[3] <= a[1];
                assert!(apart, "{a:?} overlaps {b:?}");
            }
            assert!(a[0] >= 0. && a[1] >= 0. && a[0] + a[2] <= 1920. && a[1] + a[3] <= 1039.);
        }
        // The first-placed keeps its spot.
        assert_eq!(
            cards.placed_position(Widget::Devbox),
            cards.position_from(Widget::Devbox, &cards.config.widget_positions)
        );
    }

    #[test]
    fn cards_take_clicks_only_where_they_are_drawn() {
        // A placed card: its own rectangle, not the shadow margin around it.
        let mut config = Config::default();
        config.widget_sizes.insert("weather".into(), [480., 320.]);
        let (mut cards, _) = Cards::init(Core::default(), config);
        let _output = cards.test_output_ready();
        cards.screen_size = [1920., 1080.];
        assert!(cards.custom_layout());
        let (mi, mu) = cards.shadow_px();
        let rects = cards.card_input_rects(Widget::Weather);
        assert_eq!(rects, vec![[mi.max(0) as u32, mi.max(0) as u32, 480, 320]]);
        assert!(rects[0][2] < 480 + mu || mu == 0);
        assert!(cards.card_input_rects(Widget::Clock).is_empty());
        // The default right column: only the band its cards fill, so the
        // rest of the full-height surface passes clicks to the tools.
        let (mut cards, _) = Cards::init(Core::default(), Config::default());
        let _output = cards.test_output_ready();
        cards.screen_size = [1920., 1080.];
        assert!(!cards.custom_layout());
        let column = cards.card_input_rects(Widget::Weather);
        assert_eq!(column.len(), cards.visible_cards().len());
        let height = 1080. - 96. + mu as f32;
        assert!((column[0][3] as f32) < height, "{column:?}");
        let visible = cards.visible_cards();
        let total: f32 = visible.iter().map(|w| cards.card_height(*w)).sum();
        assert!(
            column.iter().map(|r| r[3] as f32).sum::<f32>() >= total - 6.,
            "every card stays clickable"
        );
        // Hidden or offscreen surfaces take no clicks at all.
        assert!(cards.card_input_rects(Widget::Devbox).is_empty());
    }

    #[test]
    fn explicit_geometry_drives_cards_surfaces_and_desktop_resize_cancel() {
        let mut config = Config::default();
        config.music.enabled = true;
        config.focus.enabled = true;
        config.widget_sizes.insert("weather".into(), [480., 320.]);
        config.widget_sizes.insert("music".into(), [480., 320.]);
        config.widget_sizes.insert("focus".into(), [480., 320.]);
        let (mut cards, _) = Cards::init(Core::default(), config);
        let _output = cards.test_output_ready();
        cards.screen_size = [1920., 1080.];
        assert!(cards.custom_layout());
        for widget in [Widget::Weather, Widget::Music, Widget::Focus] {
            assert_eq!(cards.widget_size(widget), [480., 320.]);
            assert_eq!(cards.card_height(widget), 320.);
        }
        let _ = cards.arrange();
        cards.layout_editor.selected = Some(Widget::Focus);
        let point = cards.position_from(Widget::Focus, &cards.layout_editor.draft);
        cards.layout_resize([0, 1], true);
        assert_eq!(cards.widget_size(Widget::Focus), [480., 372.]);
        let after = cards.position_from(Widget::Focus, &cards.layout_editor.draft);
        assert!((point[0] - after[0]).abs() < 0.001 && (point[1] - after[1]).abs() < 0.001);
        cards.layout_undo();
        assert_eq!(cards.widget_size(Widget::Focus), [480., 320.]);
        cards.layout_automatic();
        assert_eq!(cards.widget_size(Widget::Focus), [280., 160.]);
        let _ = cards.finish_arrange(false);
        assert_eq!(cards.widget_size(Widget::Focus), [480., 320.]);
        assert_eq!(cards.config.widget_sizes["focus"], [480., 320.]);
    }

    #[test]
    fn reset_glance_keeps_an_explicit_default_without_reseeding_from_hidden_clock() {
        let mut config = Config::default();
        config.glance.enabled = true;
        config.glance.initialized = true;
        config.widget_positions.insert("clock".into(), [0.7, 0.7]);
        config.widget_positions.insert("glance".into(), [0.3, 0.4]);
        let (mut cards, _) = Cards::init(Core::default(), config);
        let _output = cards.test_output_ready();
        let _ = cards.arrange();
        cards.layout_defaults();
        assert_eq!(cards.layout_editor.draft["clock"], [0.7, 0.7]);
        assert!(cards.layout_editor.draft.contains_key("glance"));
        assert_eq!(
            cards.glance_position(&cards.layout_editor.draft),
            [82., 88.]
        );
    }
    #[test]
    fn desktop_layout_preserves_concurrent_changes_and_resets_visible_cards() {
        if std::env::var_os("DESKTOP_CARDS_LAYOUT_MERGE_TEST").is_none() {
            let dir = std::env::temp_dir()
                .join(format!("desktop-cards-layout-merge-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("config.json");
            std::fs::write(&path, r#"{"enabled_widgets":["weather","clock"],"placement":{"snap":false},"widget_positions":{"weather":[0.2,0.2],"clock":[0.3,0.3],"devbox":[0.4,0.4]}}"#).unwrap();
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "app::layout::tests::desktop_layout_preserves_concurrent_changes_and_resets_visible_cards"])
                .env("DESKTOP_CARDS_LAYOUT_MERGE_TEST", "1").env("DESKTOP_CARDS_CONFIG", &path).status().unwrap();
            let _ = std::fs::remove_dir_all(dir);
            assert!(result.success());
            return;
        }
        let (mut app, _) = Cards::init(Core::default(), Config::load());
        let _output = app.test_output_ready();
        let _ = app.arrange();
        app.put_layout_point(Widget::Weather, [600., 300.]);
        let moved = app.layout_editor.draft["weather"];
        Config::save_changes(serde_json::json!({
            "widget_positions":{"weather":[0.2,0.2],"clock":[0.8,0.8],"devbox":[0.7,0.7],"projects":[0.6,0.6]},
            "placement":{"snap":true,"columns":24,"future_option":42},"theme":"ocean"
        })).unwrap();
        let _ = app.finish_arrange(true);
        let saved = Config::load();
        assert!(
            app.paused_layout.is_none(),
            "{:?}",
            app.paused_layout.as_ref().and_then(|e| e.error.as_ref())
        );
        assert_eq!(saved.widget_positions["weather"], moved);
        assert_eq!(saved.widget_positions["clock"], [0.8, 0.8]);
        assert_eq!(saved.widget_positions["devbox"], [0.7, 0.7]);
        assert_eq!(saved.widget_positions["projects"], [0.6, 0.6]);
        assert_eq!(saved.widget_positions.len(), 4);
        assert!(saved.placement.snap);
        assert_eq!(saved.placement.columns, 24);
        assert_eq!(saved.theme, "ocean");

        let _ = app.arrange();
        app.layout_defaults();
        Config::save_changes(serde_json::json!({"widget_positions":{
            "weather":[0.9,0.9],"clock":[0.7,0.7],"devbox":[0.2,0.2],"projects":[0.4,0.4]
        }}))
        .unwrap();
        let _ = app.finish_arrange(true);
        let saved = Config::load();
        assert_eq!(saved.widget_positions["weather"], [0.9, 0.9]);
        assert_eq!(saved.widget_positions["clock"], [0.7, 0.7]);
        assert_eq!(saved.widget_positions["devbox"], [0.2, 0.2]);
        assert_eq!(saved.widget_positions["projects"], [0.4, 0.4]);
        let _ = app.arrange();
        let _ = app.finish_arrange(true);
        assert_eq!(Config::load().widget_positions, saved.widget_positions);
    }
    #[test]
    fn desktop_drag_cancel_save_and_reset_preserve_unrelated_preferences() {
        if std::env::var_os("DESKTOP_CARDS_LAYOUT_TEST").is_none() {
            let dir =
                std::env::temp_dir().join(format!("desktop-cards-layout-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("config.json");
            std::fs::write(
                &path,
                r#"{"theme":"teal","devbox":{"network":false},"placement":{"snap":false},"future_option":42}"#,
            )
            .unwrap();
            let result = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact", "app::layout::tests::desktop_drag_cancel_save_and_reset_preserve_unrelated_preferences", "--nocapture"]).env("DESKTOP_CARDS_LAYOUT_TEST", "1").env("DESKTOP_CARDS_CONFIG", &path).status().unwrap();
            let _ = std::fs::remove_dir_all(dir);
            assert!(result.success());
            return;
        }
        let path = Config::path().unwrap();
        let before = std::fs::read(&path).unwrap();
        let (mut app, _) = Cards::init(Core::default(), Config::load());
        let _output = app.test_output_ready();
        for save in [false, true] {
            let _ = app.update(Message::Arrange);
            let start = app.position_from(Widget::Weather, &app.layout_editor.draft);
            let _ = app.update(Message::Pointer(
                app.ids[5],
                mouse::Event::CursorMoved {
                    position: Point::new(start[0] + 50., start[1] + 30.),
                },
            ));
            let _ = app.update(Message::LayoutPress(Widget::Weather));
            let _ = app.update(Message::Pointer(
                app.ids[5],
                mouse::Event::CursorMoved {
                    position: Point::new(650., 330.),
                },
            ));
            let _ = app.update(Message::Pointer(
                app.ids[5],
                mouse::Event::ButtonReleased(mouse::Button::Left),
            ));
            let point = app.position_from(Widget::Weather, &app.layout_editor.draft);
            assert!((point[0] - 600.).abs() < 0.001 && (point[1] - 300.).abs() < 0.001);
            assert_ne!(
                std::fs::read(&path).unwrap(),
                before,
                "release completes the gesture"
            );
            let _ = app.update(Message::LayoutFinish(save));
            assert!(!app.layout_editor.active);
            if !save {
                let mut restored: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                if restored.get("widget_positions") == Some(&serde_json::json!({})) {
                    restored.as_object_mut().unwrap().remove("widget_positions");
                }
                assert_eq!(
                    restored,
                    serde_json::from_slice::<serde_json::Value>(&before).unwrap()
                );
            }
        }
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["theme"], "teal");
        assert_eq!(saved["devbox"]["network"], false);
        assert_eq!(saved["future_option"], 42);
        let restored = Config::load();
        assert!(!restored.widget_positions.is_empty());
        let _ = app.update(Message::ResetLayout);
        assert!(Config::load().widget_positions.is_empty());
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(raw["future_option"], 42);
    }
}

/// Rectangles for a layer surface's input region.
fn zone(rects: &[[u32; 4]]) -> Vec<cosmic::iced::Rectangle> {
    rects
        .iter()
        .map(|r| cosmic::iced::Rectangle {
            x: r[0] as f32,
            y: r[1] as f32,
            width: r[2] as f32,
            height: r[3] as f32,
        })
        .collect()
}

/// Changes where a layer surface takes clicks, without recreating it.
pub(super) fn set_input_zone<M>(id: window::Id, rects: &[[u32; 4]]) -> Task<M> {
    use cosmic::iced::runtime::platform_specific::{self, wayland};
    cosmic::iced::runtime::task::effect(cosmic::iced::runtime::Action::PlatformSpecific(
        platform_specific::Action::Wayland(wayland::Action::LayerSurface(
            wayland::layer_surface::Action::InputZone {
                id,
                zone: Some(zone(rects)),
            },
        )),
    ))
}

#[cfg(test)]
mod arrange_match_tests;
#[cfg(test)]
mod input_safety_tests;

#[cfg(test)]
mod review030_tests {
    use super::*;
    use crate::displays::{
        OutputEvent,
        test_support::{Wire, info, isolated},
    };
    use cosmic::Application;

    fn cards() -> (Cards, Wire) {
        let wire = Wire::new();
        let config = Config::save_changes(serde_json::json!({
            "enabled_widgets": ["weather"], "widget_display": "DP-2",
            "placement": {"snap": false},
            "monitor_layouts": {"weather": {"DP-2": {"position": [0.2, 0.2], "size": [400., 240.]}}}
        }))
        .unwrap();
        let (mut cards, _) = Cards::init(Core::default(), config);
        cards.displays.update(
            OutputEvent::Created(Some(info(Some("DP-2"), (1920, 1080)))),
            wire.output(1),
        );
        cards.selected_output = cards.displays.selected(Some("DP-2")).cloned();
        cards.widget_outputs = Widget::ALL
            .into_iter()
            .map(|w| cards.widget_output(w).cloned())
            .collect();
        cards.output_known = true;
        cards.refresh_display_layouts();
        (cards, wire)
    }

    #[test]
    fn review030_late_identity_preserves_connector_geometry_save_and_undo() {
        if isolated(
            "app::layout::review030_tests::review030_late_identity_preserves_connector_geometry_save_and_undo",
        ) {
            return;
        }
        let (mut cards, _wire) = cards();
        let _ = cards.arrange();
        let original = cards.layout_editor.draft["weather"];
        let generation = cards.displays.generation;
        let _ = cards.update(Message::DisplayIdentities(
            generation,
            [("DP-2".into(), "edid:test".into())].into(),
        ));
        assert!(cards.layout_editor.active);
        assert_eq!(cards.layout_editor.contexts["weather"], "edid:test");
        assert_eq!(cards.layout_editor.draft["weather"], original);
        assert_eq!(cards.effective_sizes()["weather"], [400., 240.]);
        cards.put_layout_point(Widget::Weather, [700., 350.]);
        let moved = cards.layout_editor.draft["weather"];
        let _ = cards.finish_arrange(true);
        assert!(cards.paused_layout.is_none());
        assert_eq!(cards.effective_positions()["weather"], moved);
        assert_eq!(cards.effective_sizes()["weather"], [400., 240.]);
        let saved = Config::load();
        assert_eq!(
            saved.monitor_layouts["weather"]["DP-2"].position,
            Some(original)
        );
        assert_eq!(
            saved.monitor_layouts["weather"]["edid:test"].position,
            Some(moved)
        );
        let _ = cards.arrange();
        assert_eq!(cards.layout_editor.draft["weather"], moved);
        let _ = cards.finish_arrange(true);
        let store = crate::desktop::actions::history_store().unwrap();
        store
            .undo_latest(&store.latest_change().unwrap().unwrap().id)
            .unwrap();
        let _ = cards.apply_config(Config::load());
        assert_eq!(cards.effective_positions()["weather"], original);
        assert_eq!(cards.effective_sizes()["weather"], [400., 240.]);
    }

    #[test]
    fn review030_failed_resume_keeps_draft_and_receipts_until_explicit_discard() {
        if isolated(
            "app::layout::review030_tests::review030_failed_resume_keeps_draft_and_receipts_until_explicit_discard",
        ) {
            return;
        }
        for save in [true, false] {
            let (mut cards, _wire) = cards();
            let _ = cards.arrange();
            cards.put_layout_point(Widget::Weather, [600., 300.]);
            assert!(cards.commit_arrange());
            let receipt = cards.layout_editor.session_actions.last().unwrap().clone();
            cards.put_layout_point(Widget::Weather, [850., 450.]);
            let sentinel = cards.layout_editor.draft.clone();
            // A changed touched field rejects both Save and rollback.
            Config::save_changes(serde_json::json!({"monitor_layouts":{"weather":{"DP-2":{"position":[0.95,0.95],"size":[400.,240.]}}}})).unwrap();
            let _ = cards.finish_arrange(save);
            assert!(cards.paused_layout.as_ref().unwrap().exit_failed);
            cards.displays.update(
                OutputEvent::Removed,
                cards.selected_output.as_ref().unwrap().handle.clone(),
            );
            let _ = cards.resume_arrange();
            let paused = cards.paused_layout.as_ref().unwrap();
            assert!(paused.exit_failed);
            assert_eq!(paused.draft, sentinel);
            assert!(paused.session_actions.iter().any(|a| a.id == receipt.id));
            let _ = cards.keep_arrange_saved();
            assert!(cards.paused_layout.is_none());
        }
    }

    #[test]
    fn review030_desktop_reset_clears_all_geometry_and_undo_restores_it() {
        if isolated(
            "app::layout::review030_tests::review030_desktop_reset_clears_all_geometry_and_undo_restores_it",
        ) {
            return;
        }
        let (mut cards, _wire) = cards();
        cards.config = Config::save_changes(serde_json::json!({
            "widget_positions":{"weather":[0.4,0.4]}, "widget_sizes":{"weather":[500.,280.]},
            "widget_monitors":{"weather":{"display":"DP-2","hide_disconnected":true}}
        }))
        .unwrap();
        cards.refresh_display_layouts();
        let before = cards.config.clone();
        let _ = cards.reset_layout();
        let saved = Config::load();
        assert!(
            saved.widget_positions.is_empty()
                && saved.widget_sizes.is_empty()
                && saved.monitor_layouts.is_empty()
        );
        assert!(cards.effective_positions().is_empty() && cards.effective_sizes().is_empty());
        assert_eq!(saved.widget_monitors, before.widget_monitors);
        assert_eq!(saved.widget_display, before.widget_display);
        let store = crate::desktop::actions::history_store().unwrap();
        store
            .undo_latest(&store.latest_change().unwrap().unwrap().id)
            .unwrap();
        let restored = Config::load();
        assert_eq!(restored.widget_positions, before.widget_positions);
        assert_eq!(restored.widget_sizes, before.widget_sizes);
        assert_eq!(restored.monitor_layouts, before.monitor_layouts);
    }
}

/// The first and last lines of a widget's right-click menu.
pub(super) fn widget_menu_labels(widget: Widget) -> (String, String) {
    let title = widget.title();
    let mut lower = title.to_string();
    if let Some(first) = lower.get_mut(..1) {
        first.make_ascii_lowercase();
    }
    (format!("{title} settings…"), format!("Hide {lower}"))
}
