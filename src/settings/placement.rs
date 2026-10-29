use super::Settings;
use crate::config::{Config, PlacementPreferences};
use crate::drawer::Widget;
use crate::layout_state::{Snapshot, WidgetGeometry};
use cosmic::Element;
use cosmic::app::Task;
use cosmic::iced::widget::{pin, stack};
use cosmic::iced::{Alignment, Background, Border, Color, Length, Point, mouse};
use cosmic::widget::{button, column, container, mouse_area, responsive, row, text, toggler};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub enum Message {
    Pointer(Point, f32),
    Press(String),
    Select(String),
    ResizePress(String),
    Resize([i8; 2], bool),
    ResizeMode(bool),
    Automatic,
    Center(usize),
    Undo,
    Release,
    Leave,
    ClearSelection,
    CancelDrag,
    Escape,
    Nudge([i8; 2], bool),
    Snap(bool),
    Columns(u8),
    Guides(bool),
    Save,
    Cancel,
    Reset,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct EditState {
    positions: BTreeMap<String, [f32; 2]>,
    sizes: BTreeMap<String, [f32; 2]>,
    materialized: BTreeMap<String, [f32; 2]>,
    reset: bool,
    snap: bool,
    columns: u8,
}

/// Shared by the native subscription and the widget-tree interaction audit.
pub(super) fn input(
    event: cosmic::iced::Event,
    status: cosmic::iced::event::Status,
) -> Option<Message> {
    use cosmic::iced::{Event, keyboard};
    use keyboard::key::Named;
    Some(match event {
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => Message::Release,
        Event::Mouse(mouse::Event::CursorLeft)
        | Event::Window(cosmic::iced::window::Event::Unfocused) => Message::CancelDrag,
        Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
            if status == cosmic::iced::event::Status::Ignored
                && !modifiers.alt()
                && !modifiers.control()
                && !modifiers.logo() =>
        {
            match key {
                keyboard::Key::Named(Named::Escape) => Message::Escape,
                keyboard::Key::Named(Named::ArrowLeft) => {
                    Message::Nudge([-1, 0], modifiers.shift())
                }
                keyboard::Key::Named(Named::ArrowRight) => {
                    Message::Nudge([1, 0], modifiers.shift())
                }
                keyboard::Key::Named(Named::ArrowUp) => Message::Nudge([0, -1], modifiers.shift()),
                keyboard::Key::Named(Named::ArrowDown) => Message::Nudge([0, 1], modifiers.shift()),
                _ => return None,
            }
        }
        _ => return None,
    })
}

struct Drag {
    key: String,
    offset: [f32; 2],
    before: EditState,
    resize: bool,
    origin: [f32; 2],
    size: [f32; 2],
}

/// Runtime geometry and visibility read from the config and published state.
struct Geometry {
    visible: BTreeSet<String>,
    snapshot: Snapshot,
    defaults: Snapshot,
}

impl Geometry {
    fn read(config: &Config) -> Self {
        Self::on_display(config, None)
    }
    fn on_display(config: &Config, display: Option<&str>) -> Self {
        Self::on_live_display(config, display, &crate::displays::Registry::default())
    }
    fn on_live_display(
        config: &Config,
        display: Option<&str>,
        displays: &crate::displays::Registry,
    ) -> Self {
        let snapshot = crate::layout_state::for_display(config, display, false, displays);
        let output = snapshot.display.as_ref().and_then(|d| d.output.as_ref());
        Self {
            visible: Widget::ALL
                .into_iter()
                .filter(|widget| {
                    config.widget_visible(*widget)
                        && (snapshot.widget_displays.is_empty()
                            || snapshot.widget_displays.get(widget.key()) == output)
                })
                .map(|widget| widget.key().into())
                .collect(),
            defaults: crate::layout_state::for_display(config, display, true, displays),
            snapshot,
        }
    }
}

pub(super) struct PlacementEditor {
    displays: crate::displays::Registry,
    pub(super) display: Option<String>,
    contexts: crate::config::monitors::Contexts,
    snapshot: Snapshot,
    defaults: Snapshot,
    visible: BTreeSet<String>,
    saved_sizes: BTreeMap<String, [f32; 2]>,
    sizes: BTreeMap<String, [f32; 2]>,
    saved_positions: BTreeMap<String, [f32; 2]>,
    positions: BTreeMap<String, [f32; 2]>,
    /// Implicit positions as they were when the draft first pinned them; the
    /// merge baseline stays put even if card sizes change before Save.
    materialized: BTreeMap<String, [f32; 2]>,
    /// Geometry published during a drag, adopted as soon as the drag ends.
    pending: Option<Geometry>,
    saved_snap: bool,
    saved_columns: u8,
    snap: bool,
    columns: u8,
    guides: bool,
    reset: bool,
    selected: Option<String>,
    pointer: Option<[f32; 2]>,
    inside: bool,
    drag: Option<Drag>,
    history: Vec<EditState>,
    session_actions: Vec<crate::desktop::LatestChange>,
    resize_mode: bool,
}

/// Only intentional layout work and its merge baselines survive closing.
/// Runtime geometry, pointer state and a drag in flight are read afresh.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct SavedPlacement {
    #[serde(default)]
    pub(super) contexts: crate::config::monitors::Contexts,
    #[serde(default)]
    pub(super) saved_sizes: BTreeMap<String, [f32; 2]>,
    #[serde(default)]
    pub(super) sizes: BTreeMap<String, [f32; 2]>,
    pub(super) saved_positions: BTreeMap<String, [f32; 2]>,
    pub(super) positions: BTreeMap<String, [f32; 2]>,
    pub(super) materialized: BTreeMap<String, [f32; 2]>,
    pub(super) saved_snap: bool,
    pub(super) saved_columns: u8,
    pub(super) snap: bool,
    pub(super) columns: u8,
    pub(super) reset: bool,
}

impl PlacementEditor {
    pub(super) fn session_draft(&self) -> Option<SavedPlacement> {
        self.dirty().then(|| SavedPlacement {
            contexts: self.contexts.clone(),
            saved_sizes: self.saved_sizes.clone(),
            sizes: self.sizes.clone(),
            saved_positions: self.saved_positions.clone(),
            positions: self.positions.clone(),
            materialized: self.materialized.clone(),
            saved_snap: self.saved_snap,
            saved_columns: self.saved_columns,
            snap: self.snap,
            columns: self.columns,
            reset: self.reset,
        })
    }
    pub(super) fn restore_session(&mut self, saved: SavedPlacement) -> Result<(), String> {
        for positions in [
            &saved.saved_positions,
            &saved.positions,
            &saved.materialized,
        ] {
            if positions.len() > Widget::ALL.len()
                || positions.iter().any(|(key, position)| {
                    Widget::parse(key).is_none()
                        || position
                            .iter()
                            .any(|value| !value.is_finite() || value.abs() > 100_000.)
                })
            {
                return Err("The saved widget layout contains invalid positions".into());
            }
        }
        if ![12, 16, 24].contains(&saved.columns) || ![12, 16, 24].contains(&saved.saved_columns) {
            return Err("The saved widget layout has an unsupported grid".into());
        }
        for sizes in [&saved.saved_sizes, &saved.sizes] {
            if sizes.len() > Widget::ALL.len()
                || sizes.iter().any(|(key, size)| {
                    Widget::parse(key).is_none() || !crate::layout::valid_size(*size)
                })
            {
                return Err("The saved widget layout contains invalid sizes".into());
            }
        }
        if saved.contexts != self.contexts {
            return Err(
                "Reconnect the displays used by this arrangement before restoring it.".into(),
            );
        }
        self.saved_sizes = saved.saved_sizes;
        self.sizes = saved.sizes;
        self.saved_positions = saved.saved_positions;
        self.positions = saved.positions;
        self.materialized = saved.materialized;
        self.saved_snap = saved.saved_snap;
        self.saved_columns = saved.saved_columns;
        self.snap = saved.snap;
        self.columns = saved.columns;
        self.reset = saved.reset;
        self.drag = None;
        self.pending = None;
        Ok(())
    }

    pub(super) fn new(config: &Config) -> Self {
        Self::from_geometry(config, Geometry::read(config))
    }

    fn from_geometry(config: &Config, geometry: Geometry) -> Self {
        let config = config.on_displays(&geometry.snapshot.widget_displays);
        Self {
            displays: Default::default(),
            display: geometry
                .snapshot
                .display
                .as_ref()
                .and_then(|d| d.output.clone()),
            contexts: if config.multi_monitor() || geometry.snapshot.monitors.len() > 1 {
                geometry.snapshot.widget_displays.clone()
            } else {
                Default::default()
            },
            snapshot: geometry.snapshot,
            defaults: geometry.defaults,
            visible: geometry.visible,
            saved_sizes: config.widget_sizes.clone(),
            sizes: config.widget_sizes.clone(),
            saved_positions: config.widget_positions.clone(),
            positions: config.widget_positions.clone(),
            materialized: BTreeMap::new(),
            pending: None,
            saved_snap: config.placement.snap,
            saved_columns: config.placement.columns,
            snap: config.placement.snap,
            columns: config.placement.columns,
            guides: false,
            reset: false,
            selected: None,
            pointer: None,
            inside: false,
            drag: None,
            history: Vec::new(),
            session_actions: Vec::new(),
            resize_mode: false,
        }
    }

    /// Logical size of the active output, as published by the desktop.
    pub(super) fn screen(&self) -> [f32; 2] {
        self.snapshot.screen
    }
    pub(super) fn dirty(&self) -> bool {
        self.sizes != self.saved_sizes
            || self.positions != self.saved_positions
            || self.snap != self.saved_snap
            || self.columns != self.saved_columns
            || self.reset
    }

    pub(super) fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub(super) fn update_displays(
        &mut self,
        config: &Config,
        displays: &crate::displays::Registry,
    ) {
        let before = self
            .displays
            .selected(self.display.as_deref().or(config.widget_display.as_deref()));
        let after = displays.selected(self.display.as_deref().or(config.widget_display.as_deref()));
        if !matches!((before, after), (Some(a), Some(b)) if a.same_geometry(b))
            && (before.is_some() || after.is_some())
        {
            self.cancel_drag();
            self.pointer = None;
            self.inside = false;
        }
        self.displays = displays.clone();
        self.refresh(config);
    }
    pub(super) fn refresh(&mut self, config: &Config) {
        let next = Geometry::on_live_display(config, self.display.as_deref(), &self.displays);
        let next_contexts = if config.multi_monitor() || next.snapshot.monitors.len() > 1 {
            next.snapshot.widget_displays.clone()
        } else {
            Default::default()
        };
        if next_contexts != self.contexts {
            self.cancel_drag();
        }
        if self.dirty() || self.dragging() {
            self.reconcile(next);
            return;
        }
        let selected = self.selected.clone();
        let guides = self.guides;
        let resize_mode = self.resize_mode;
        let pointer = self.pointer;
        let inside = self.inside;
        let screen = self.snapshot.screen;
        let actions = std::mem::take(&mut self.session_actions);
        let displays = std::mem::take(&mut self.displays);
        let effective = if displays.outputs().is_empty() {
            config.clone()
        } else {
            displays.effective_config(config)
        };
        *self = Self::from_geometry(&effective, next);
        self.displays = displays;
        self.session_actions = actions;
        self.selected =
            selected.filter(|key| Widget::parse(key).is_some_and(|w| config.widget_visible(w)));
        self.guides = guides;
        self.resize_mode = resize_mode;
        if self.snapshot.screen == screen {
            self.pointer = pointer;
            self.inside = inside;
        }
    }

    /// Selects a visible card without starting a drag (keyboard placement).
    pub(super) fn select(&mut self, key: &str) {
        if !self.dragging() && Widget::parse(key).is_some() {
            self.selected = Some(key.into());
        }
    }

    /// Keeps the draft but abandons a drag when the editor leaves the screen.
    pub(super) fn leave(&mut self) {
        self.cancel_drag();
        self.inside = false;
        self.pointer = None;
    }

    /// A draft keeps its positions, grid and merge baselines but follows the
    /// cards that exist and their actual sizes. A drag in progress keeps the
    /// geometry it grabbed unless its own card disappears.
    fn reconcile(&mut self, next: Geometry) {
        if let Some(drag) = &self.drag {
            let grabbed_card_remains = next.visible.contains(&drag.key);
            let same_frame = next.snapshot.screen == self.snapshot.screen
                && next
                    .snapshot
                    .display
                    .as_ref()
                    .map(|d| (&d.output, d.ready, d.epoch))
                    == self
                        .snapshot
                        .display
                        .as_ref()
                        .map(|d| (&d.output, d.ready, d.epoch));
            self.pending = Some(next);
            if !grabbed_card_remains || !same_frame {
                // Cancelling the drag adopts the pending geometry.
                self.cancel_drag();
                self.pointer = None;
                self.inside = false;
            }
            return;
        }
        if next.snapshot.screen != self.snapshot.screen {
            self.pointer = None;
            self.inside = false;
        }
        self.visible = next.visible;
        self.snapshot = next.snapshot;
        self.defaults = next.defaults;
        self.selected = self
            .selected
            .take()
            .filter(|key| self.visible.contains(key));
    }

    fn adopt_pending(&mut self) {
        if let Some(next) = self.pending.take() {
            self.reconcile(next);
        }
    }

    pub(super) fn cancel(&mut self) {
        self.positions = self.saved_positions.clone();
        self.sizes = self.saved_sizes.clone();
        self.history.clear();
        self.resize_mode = false;
        self.snap = self.saved_snap;
        self.columns = self.saved_columns;
        self.reset = false;
        self.materialized.clear();
        self.drag = None;
        self.adopt_pending();
    }

    pub(super) fn preview_snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    fn geometry(&self, key: &str) -> Option<&WidgetGeometry> {
        if !self.visible.contains(key) {
            return None;
        }
        self.snapshot
            .widgets
            .iter()
            .find(|widget| widget.key == key)
    }

    fn position(&self, key: &str) -> Option<[f32; 2]> {
        self.geometry(key)?;
        if let Some(position) = self.positions.get(key) {
            let size = self.size(key)?;
            let point = crate::layout_state::decode_position(
                Widget::parse(key)?,
                *position,
                size,
                self.snapshot.screen,
            );
            return Some(if self.sizes.contains_key(key) {
                crate::layout::clamp(point, size, self.screen())
            } else {
                point
            });
        }
        let source = if self.reset {
            &self.defaults
        } else {
            &self.snapshot
        };
        source
            .widgets
            .iter()
            .find(|widget| widget.key == key)
            .map(|widget| widget.position)
    }

    fn pointer(&mut self, point: Point, scale: f32) {
        if !scale.is_finite() || scale <= 0. || !point.x.is_finite() || !point.y.is_finite() {
            return;
        }
        let point = [point.x / scale, point.y / scale];
        self.inside =
            (0..2).all(|axis| point[axis] >= 0. && point[axis] <= self.snapshot.screen[axis]);
        self.pointer = self.inside.then_some(point);
        if self.inside {
            if let Some(drag) = &self.drag {
                let key = drag.key.clone();
                if drag.resize {
                    let size = [0, 1].map(|axis| drag.size[axis] + point[axis] - drag.origin[axis]);
                    self.put_size(&key, size);
                } else {
                    let next = [point[0] - drag.offset[0], point[1] - drag.offset[1]];
                    self.put_point(&key, next);
                }
            }
        }
    }

    pub(super) fn display_size(&self, key: &str) -> Option<[f32; 2]> {
        let geometry = self.geometry(key)?;
        let mut size = self.size(key)?;
        if key == Widget::Glance.key() && !self.sizes.contains_key(key) {
            let position = self.position(key)?;
            size[1] = geometry
                .natural_height
                .unwrap_or(size[1])
                .min((self.snapshot.screen[1] - 48. - position[1]).max(1.));
        }
        Some(size)
    }

    fn press(&mut self, key: &str) {
        let Some(size) = self.display_size(key) else {
            return;
        };
        self.selected = Some(key.into());
        let (Some(pointer), Some(position)) = (self.pointer, self.position(key)) else {
            return;
        };
        if !self.inside
            || !(0..2).all(|axis| {
                pointer[axis] >= position[axis] && pointer[axis] <= position[axis] + size[axis]
            })
        {
            return;
        }
        self.drag = Some(Drag {
            key: key.into(),
            offset: [pointer[0] - position[0], pointer[1] - position[1]],
            before: self.state(),
            resize: false,
            origin: pointer,
            size,
        });
    }

    fn put_point(&mut self, key: &str, point: [f32; 2]) {
        let Some(widget) = Widget::parse(key) else {
            return;
        };
        let Some(size) = self.size(key) else {
            return;
        };
        let placement_size = if self.sizes.contains_key(key) {
            size
        } else {
            crate::layout_state::placement_size(widget, size)
        };
        let point = if self.snap {
            crate::layout::snap(point, placement_size, self.snapshot.screen, self.columns)
        } else {
            crate::layout::clamp(point, placement_size, self.snapshot.screen)
        };
        self.materialize();
        self.positions.insert(
            key.into(),
            crate::layout_state::encode_position(widget, point, size, self.snapshot.screen),
        );
    }

    fn center(&mut self, axis: usize) {
        if axis > 1 || self.dragging() {
            return;
        }
        let Some(key) = self.selected.clone() else {
            return;
        };
        let Some(mut point) = self.position(&key) else {
            return;
        };
        let Some(mut size) = self.display_size(&key) else {
            return;
        };
        let widget = Widget::parse(&key).unwrap();
        if widget == Widget::Glance && !self.sizes.contains_key(&key) && axis == 1 {
            // A growing agenda can expand when moved up. Center the height it
            // will actually have at its new position, including screen limits.
            size[1] = self
                .geometry(&key)
                .and_then(|g| g.natural_height)
                .unwrap_or(size[1])
                .min((self.screen()[1] - 96.).max(1.));
        }
        point[axis] = (self.screen()[axis] - size[axis]) / 2.;
        self.materialize();
        // Deliberately independent of Snap: center is a precise alignment.
        self.positions.insert(
            key,
            crate::layout_state::encode_position(widget, point, size, self.screen()),
        );
    }

    fn materialize(&mut self) {
        // Preserve every other visible card exactly where the miniature shows it
        // when the first move switches the desktop into a custom layout.
        let current: Vec<_> = self
            .visible
            .iter()
            .filter_map(|key| {
                Some((
                    key.clone(),
                    crate::layout_state::encode_position(
                        Widget::parse(key)?,
                        self.position(key)?,
                        self.size(key)?,
                        self.snapshot.screen,
                    ),
                ))
            })
            .collect();
        for (key, position) in current {
            if !self.positions.contains_key(&key) {
                self.materialized.insert(key.clone(), position);
                self.positions.insert(key, position);
            }
        }
    }
    fn size(&self, key: &str) -> Option<[f32; 2]> {
        let geometry = self.geometry(key)?;
        let automatic = self
            .defaults
            .widgets
            .iter()
            .find(|g| g.key == key)
            .unwrap_or(geometry);
        Some(
            self.sizes
                .get(key)
                .map(|size| crate::layout::sized(Widget::parse(key).unwrap(), *size, self.screen()))
                .unwrap_or(automatic.size),
        )
    }
    pub(super) fn selected(&self) -> Option<Widget> {
        self.selected.as_deref().and_then(Widget::parse)
    }
    pub(super) fn state(&self) -> EditState {
        EditState {
            positions: self.positions.clone(),
            sizes: self.sizes.clone(),
            materialized: self.materialized.clone(),
            reset: self.reset,
            snap: self.snap,
            columns: self.columns,
        }
    }
    pub(super) fn restore(&mut self, state: EditState) {
        self.positions = state.positions;
        self.sizes = state.sizes;
        self.materialized = state.materialized;
        self.reset = state.reset;
        self.snap = state.snap;
        self.columns = state.columns;
    }
    fn remember(&mut self, before: EditState) {
        if before != self.state() {
            if self.history.len() == 50 {
                self.history.remove(0);
            }
            self.history.push(before);
        }
    }
    fn resize_press(&mut self, key: &str) {
        self.press(key);
        if let Some(drag) = &mut self.drag {
            drag.resize = true;
        }
    }
    fn put_size(&mut self, key: &str, requested: [f32; 2]) {
        let Some(widget) = Widget::parse(key) else {
            return;
        };
        let Some(position) = self.position(key) else {
            return;
        };
        let size = crate::layout::resize(widget, position, requested, self.screen(), self.snap);
        self.materialize();
        self.sizes.insert(key.into(), size);
        self.positions.insert(
            key.into(),
            crate::layout_state::encode_position(widget, position, size, self.screen()),
        );
    }
    fn resize_nudge(&mut self, direction: [i8; 2], large: bool) {
        if self.dragging() {
            return;
        }
        let Some(key) = self.selected.clone() else {
            return;
        };
        let Some(size) = self.display_size(&key) else {
            return;
        };
        let step = (if self.snap { 12. } else { 8. }) * if large { 4. } else { 1. };
        self.put_size(
            &key,
            [0, 1].map(|axis| size[axis] + f32::from(direction[axis]) * step),
        );
    }
    fn automatic(&mut self) {
        self.cancel_drag();
        let Some(key) = self.selected.clone() else {
            return;
        };
        let Some(point) = self.position(&key) else {
            return;
        };
        if !self.sizes.contains_key(&key) {
            return;
        }
        // Returning to natural size can change automatic stack positions too.
        // Keep the neighbors shown in the draft, just as a manual resize does.
        self.materialize();
        self.sizes.remove(&key);
        let widget = Widget::parse(&key).unwrap();
        let size = self.size(&key).unwrap();
        self.positions.insert(
            key,
            crate::layout_state::encode_position(widget, point, size, self.screen()),
        );
    }
    pub(super) fn changed_fields(&self) -> Vec<String> {
        let baseline = self.save_baseline();
        let size_baseline = self.size_baseline();
        let mut fields = Vec::new();
        for (name, draft, before) in [
            ("widget_positions", &self.positions, &baseline),
            ("widget_sizes", &self.sizes, &size_baseline),
        ] {
            for key in draft.keys().chain(before.keys()) {
                if draft.get(key) != before.get(key) {
                    fields.push(if self.contexts.contains_key(key) {
                        format!("/monitor_layouts/{key}")
                    } else {
                        format!("/{name}/{key}")
                    });
                }
            }
        }
        fields.extend(
            self.materialized
                .keys()
                .filter(|key| self.positions.contains_key(*key))
                .map(|key| {
                    if self.contexts.contains_key(key) {
                        format!("/monitor_layouts/{key}")
                    } else {
                        format!("/widget_positions/{key}")
                    }
                }),
        );
        if self.snap != self.saved_snap {
            fields.push("/placement/snap".into());
        }
        if self.saved_columns != PlacementPreferences::FINE_COLUMNS {
            fields.push("/placement/columns".into());
        }
        fields
    }
    fn size_baseline(&self) -> BTreeMap<String, [f32; 2]> {
        let mut baseline = self.saved_sizes.clone();
        if self.reset {
            for key in &self.visible {
                if self.sizes.contains_key(key) {
                    baseline.remove(key);
                } else {
                    baseline.entry(key.clone()).or_insert([1., 1.]);
                }
            }
        }
        baseline
    }

    fn release(&mut self) {
        if !self.inside {
            self.cancel_drag();
        } else {
            if let Some(drag) = self.drag.take() {
                self.remember(drag.before);
            }
            self.adopt_pending();
        }
    }

    fn cancel_drag(&mut self) {
        if let Some(drag) = self.drag.take() {
            self.restore(drag.before);
        }
        self.adopt_pending();
    }

    fn nudge(&mut self, direction: [i8; 2], large: bool) {
        if self.dragging() {
            return;
        }
        let Some(key) = self.selected.clone() else {
            return;
        };
        let Some(mut point) = self.position(&key) else {
            return;
        };
        let step = if self.snap {
            crate::layout::grid_step(self.snapshot.screen, self.columns)
        } else {
            [8., 8.]
        };
        for axis in 0..2 {
            point[axis] += f32::from(direction[axis]) * step[axis] * if large { 4. } else { 1. };
        }
        self.put_point(&key, point);
    }

    fn reset(&mut self) {
        self.cancel_drag();
        self.positions.retain(|key, _| !self.visible.contains(key));
        self.sizes.retain(|key, _| !self.visible.contains(key));
        if self.visible.contains(Widget::Glance.key()) {
            if let Some(geometry) = self
                .defaults
                .widgets
                .iter()
                .find(|widget| widget.key == Widget::Glance.key())
            {
                // An absent Glance position means first-use seeding from Clock.
                // Reset explicitly chooses its default while keeping the hidden
                // standalone Clock's saved position unchanged.
                self.positions.insert(
                    Widget::Glance.key().into(),
                    crate::layout_state::encode_position(
                        Widget::Glance,
                        geometry.position,
                        geometry.size,
                        self.snapshot.screen,
                    ),
                );
            }
        }
        self.reset = true;
    }

    fn save_baseline(&self) -> BTreeMap<String, [f32; 2]> {
        let mut baseline = self.saved_positions.clone();
        for geometry in &self.snapshot.widgets {
            let key = &geometry.key;
            let has_position = self.positions.contains_key(key);
            let reset_visible = self.reset && self.visible.contains(key);
            if reset_visible && has_position {
                // Reset followed by a move materializes the visible defaults.
                // Every visible card is intentional, including unchanged defaults.
                baseline.remove(key);
            } else if reset_visible || has_position {
                // An absent Reset key must remove even a concurrently added value.
                // During ordinary moves, implicit defaults are the baseline so
                // untouched cards retain concurrent changes from another editor.
                baseline.entry(key.clone()).or_insert_with(|| {
                    self.materialized.get(key).copied().unwrap_or_else(|| {
                        crate::layout_state::encode_position(
                            Widget::parse(key).expect("Snapshot widget keys are validated"),
                            geometry.position,
                            geometry.size,
                            self.snapshot.screen,
                        )
                    })
                });
            }
        }
        baseline
    }
}

const FOREGROUND: Color = Color::from_rgb(0.94, 0.93, 0.97);
const MUTED: Color = Color::from_rgb(0.65, 0.64, 0.71);

fn message(message: Message) -> super::Message {
    super::Message::Placement(message)
}

fn label<'a>(value: impl Into<String>, size: f32, color: Color) -> Element<'a, super::Message> {
    text::body(value.into())
        .size(size)
        .line_height(1.25)
        .class(super::view::text_style(color))
        .into()
}

fn line<'a>(width: f32, height: f32, color: Color) -> Element<'a, super::Message> {
    container(text::body(""))
        .width(width.max(0.5))
        .height(height.max(0.5))
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(color)),
                border: Border {
                    radius: 2.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

impl Settings {
    pub(super) fn legacy_layout_recovery(&self) -> bool {
        self.canvas.is_none()
            && (self.placement.dirty() || !self.placement.session_actions.is_empty())
    }
    pub(super) fn finish_layout_recovery(&mut self) {
        self.placement.session_actions.clear();
    }
    fn placement_card<'a>(
        &'a self,
        geometry: &WidgetGeometry,
        size: [f32; 2],
        scale: f32,
        selected: bool,
        dragging: bool,
        interactive: bool,
    ) -> Element<'a, super::Message> {
        let widget = Widget::parse(&geometry.key).unwrap();
        let accent = self.settings_accent();
        let content = super::view::profiles::card(widget, size, self.canvas_config(), scale);
        if !interactive {
            return content;
        }
        // The native button supplies Tab focus and keyboard activation. Its
        // child owns pointer press, so a drag never opens a settings page.
        let body = button::custom(
            mouse_area(content).on_press(message(Message::Press(geometry.key.clone()))),
        )
        .class(super::view::home::hotspot(accent, 5.))
        .on_press(message(Message::Select(geometry.key.clone())))
        .id(cosmic::widget::Id::new(format!(
            "placement-card-{}",
            widget.key()
        )))
        .padding(0)
        .width(size[0])
        .height(size[1]);
        let mut layers: Vec<Element<'a, super::Message>> = vec![body.into()];
        if selected {
            // A noninteractive outline leaves the native focus ring visible.
            let outline = container(text::body(""))
                .width(size[0])
                .height(size[1])
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        border: Border {
                            color: accent,
                            width: 2.,
                            radius: 5.into(),
                        },
                        ..Default::default()
                    }
                }));
            layers.push(outline.into());
            let grip = mouse_area(
                container(label("◢", 16., accent))
                    .width(19)
                    .height(19)
                    .align_x(Alignment::End)
                    .align_y(Alignment::End),
            )
            .on_press(message(Message::ResizePress(geometry.key.clone())))
            .interaction(mouse::Interaction::ResizingDiagonallyDown);
            layers.push(
                pin(grip)
                    .position(Point::new((size[0] - 19.).max(0.), (size[1] - 19.).max(0.)))
                    .into(),
            );
        }
        mouse_area(stack(layers).width(size[0]).height(size[1]))
            .interaction(if dragging {
                mouse::Interaction::Grabbing
            } else {
                mouse::Interaction::Grab
            })
            .into()
    }

    pub(super) fn placement_inspector(&self) -> Element<'_, super::Message> {
        let editor = &self.placement;
        let accent = self.settings_accent();
        let Some(widget) = editor.selected() else {
            // An ordinary button for every visible card also makes selection
            // easy without a pointer, including small or overlapping cards.
            return row::with_children(
                Widget::ALL
                    .into_iter()
                    .filter(|w| self.config.widget_visible(*w))
                    .map(|w| {
                        button::text(w.title())
                            .on_press(message(Message::Select(w.key().into())))
                            .into()
                    })
                    .collect::<Vec<_>>(),
            )
            .spacing(4)
            .wrap()
            .into();
        };
        let Some(size) = editor.display_size(widget.key()) else {
            return column::with_children(vec![
                text::body(widget.title()).into(),
                if self.canvas.is_none() {
                    self.widget_display_controls(widget)
                } else {
                    text::caption("This widget is not on the selected display.").into()
                },
                button::text("Done")
                    .on_press(message(Message::ClearSelection))
                    .into(),
            ])
            .spacing(6)
            .into();
        };
        let dimensions = |axis: usize, title: &'static str| {
            let mut minus = [0, 0];
            minus[axis] = -1;
            let mut plus = [0, 0];
            plus[axis] = 1;
            row::with_children(vec![
                label(
                    format!("{title} {}", size[axis].round() as u32),
                    12.,
                    FOREGROUND,
                ),
                button::text("−")
                    .on_press(message(Message::Resize(minus, false)))
                    .into(),
                button::text("+")
                    .on_press(message(Message::Resize(plus, false)))
                    .into(),
            ])
            .spacing(3)
            .align_y(Alignment::Center)
            .into()
        };
        column::with_children(vec![
            if self.canvas.is_none() {
                self.widget_display_controls(widget)
            } else {
                text::caption("Local draft · arrows move the selection").into()
            },
            row::with_children(vec![
                label(widget.title(), 13., accent),
                button::text(if self.canvas.is_some() {
                    "Content settings"
                } else {
                    "Customize"
                })
                .on_press(super::Message::Page(super::view::home::widget_page(widget)))
                .into(),
                button::text(if editor.resize_mode {
                    "Move with arrows"
                } else {
                    "Resize with arrows"
                })
                .id(cosmic::widget::Id::new("layout-resize-mode"))
                .on_press(message(Message::ResizeMode(!editor.resize_mode)))
                .into(),
                button::text("Done")
                    .id(cosmic::widget::Id::new("layout-done"))
                    .on_press(message(Message::ClearSelection))
                    .into(),
            ])
            .spacing(8)
            .align_y(Alignment::Center)
            .wrap()
            .into(),
            row::with_children(vec![
                dimensions(0, "W"),
                dimensions(1, "H"),
                button::text("Fit content")
                    .id(cosmic::widget::Id::new("layout-automatic"))
                    .on_press_maybe(
                        editor
                            .sizes
                            .contains_key(widget.key())
                            .then_some(message(Message::Automatic)),
                    )
                    .into(),
                button::text("Center ↔")
                    .id(cosmic::widget::Id::new("layout-center-x"))
                    .on_press(message(Message::Center(0)))
                    .into(),
                button::text("Center ↕")
                    .id(cosmic::widget::Id::new("layout-center-y"))
                    .on_press(message(Message::Center(1)))
                    .into(),
            ])
            .spacing(8)
            .align_y(Alignment::Center)
            .wrap()
            .into(),
        ])
        .spacing(2)
        .into()
    }

    pub(super) fn update_placement(&mut self, event: Message) -> Task<super::Message> {
        if self.canvas.is_some() {
            match event {
                Message::Save => return Task::none(),
                Message::Undo => return self.update_canvas(super::canvas::Message::Undo),
                Message::Cancel => return self.update_canvas(super::canvas::Message::Cancel),
                Message::Escape if !self.placement.dragging() => {
                    self.placement.selected = None;
                    return Task::none();
                }
                _ => {}
            }
        }
        let canvas_before = if matches!(event, Message::Release) {
            self.placement.drag.as_ref().map(|drag| drag.before.clone())
        } else if !matches!(
            event,
            Message::Pointer(..) | Message::CancelDrag | Message::Leave | Message::Escape
        ) {
            Some(self.placement.state())
        } else {
            None
        };
        let before = self.placement.state();
        let record = !matches!(
            event,
            Message::Pointer(..)
                | Message::Release
                | Message::Cancel
                | Message::Escape
                | Message::CancelDrag
                | Message::Undo
                | Message::Save
        );
        let previous_positions = self.placement.positions.clone();
        let previous_preferences = (
            self.placement.snap,
            self.placement.columns,
            self.placement.reset,
        );
        match event {
            Message::Pointer(point, scale) => self.placement.pointer(point, scale),
            Message::Press(key) => self.placement.press(&key),
            Message::Select(key) => {
                if let Some(display) = self.placement.snapshot.widget_displays.get(&key).cloned() {
                    self.select_monitor_preview(display.clone());
                    if self.canvas.is_some() && self.placement.display.as_ref() != Some(&display) {
                        return Task::none();
                    }
                }
                self.placement.select(&key);
            }
            Message::ResizePress(key) => self.placement.resize_press(&key),
            Message::Resize(direction, large) => self.placement.resize_nudge(direction, large),
            Message::ResizeMode(value) => self.placement.resize_mode = value,
            Message::Automatic => self.placement.automatic(),
            Message::Center(axis) => self.placement.center(axis),
            Message::Undo => {
                self.placement.cancel_drag();
                return self.undo_desktop_change();
            }
            Message::Release => self.placement.release(),
            Message::Leave => self.placement.inside = false,
            Message::ClearSelection => {
                self.placement.selected = None;
                self.placement.session_actions.clear();
            }
            Message::CancelDrag => {
                self.placement.cancel_drag();
                self.placement.inside = false;
                self.placement.pointer = None;
            }
            Message::Escape if self.placement.dragging() => self.placement.cancel_drag(),
            // Esc on a clean editor must not report a cancel (it also ends a
            // column reorder, which shares the key).
            Message::Escape if !self.placement.dirty() => self.placement.selected = None,
            Message::Escape | Message::Cancel => {
                if let Err(error) =
                    crate::desktop::actions::cancel_arrangement(&self.placement.session_actions)
                {
                    self.failed(error);
                    return Task::none();
                }
                self.placement.session_actions.clear();
                if let Ok(config) = Config::try_load() {
                    self.config = config;
                }
                self.placement.cancel();
                self.placement.refresh(&self.config);
                self.status = "Layout changes canceled".into();
                self.error = false;
                return Task::none();
            }
            Message::Nudge(direction, large) => {
                if self.placement.resize_mode {
                    self.placement.resize_nudge(direction, large);
                } else {
                    self.placement.nudge(direction, large);
                }
            }
            Message::Snap(value) => self.placement.snap = value,
            // Older command senders can still request a density; every
            // supported legacy choice now selects the same fine grid.
            Message::Columns(value) if matches!(value, 12 | 16 | 24) => {
                self.placement.columns = PlacementPreferences::FINE_COLUMNS
            }
            Message::Columns(_) => return Task::none(),
            Message::Guides(value) => self.placement.guides = value,
            Message::Reset => self.placement.reset(),
            Message::Save => {
                self.placement.release();
                if self.busy || !self.placement.dirty() {
                    return Task::none();
                }
                if !self.displays.outputs().is_empty()
                    && self.displays.contexts(&self.config) != self.placement.contexts
                {
                    self.failed("Displays changed. Cancel this move and try again.".into());
                    return Task::none();
                }
                let baseline = self.placement.save_baseline();
                let (result, receipts) = crate::desktop::actions::with_receipts(|| {
                    Config::save_layout_on_displays(
                        &self.placement.contexts,
                        &self.placement.positions,
                        &baseline,
                        &self.placement.sizes,
                        &self.placement.size_baseline(),
                        &self.placement.materialized,
                        PlacementPreferences {
                            snap: self.placement.snap,
                            // Opening a legacy layout leaves it clean. Upgrade its
                            // stored preference only with an explicit draft save.
                            columns: PlacementPreferences::FINE_COLUMNS,
                        },
                        PlacementPreferences {
                            snap: self.placement.saved_snap,
                            columns: self.placement.saved_columns,
                        },
                    )
                });
                self.placement.session_actions.extend(receipts);
                match result {
                    Ok(config) => {
                        self.config = config;
                        let resize_mode = self.placement.resize_mode;
                        self.placement.cancel();
                        self.placement.refresh(&self.config);
                        self.placement.resize_mode = resize_mode;
                        self.status = "Layout saved".into();
                        self.error = false;
                    }
                    Err(error) => self.failed(error),
                }
                return Task::none();
            }
        }
        if let Some(before) = canvas_before
            && !self.placement.dragging()
            && before != self.placement.state()
            && let Some(canvas) = self.canvas.as_mut()
        {
            canvas.remember(before);
        }
        if record && !self.placement.dragging() {
            self.placement.remember(before.clone());
        }
        if before.sizes != self.placement.sizes
            || previous_positions != self.placement.positions
            || previous_preferences
                != (
                    self.placement.snap,
                    self.placement.columns,
                    self.placement.reset,
                )
        {
            self.status = if self.placement.dirty() {
                if self.canvas.is_some() {
                    "Previewing · Apply when ready"
                } else {
                    "Finish this gesture to save"
                }
            } else {
                "Layout matches your desktop"
            }
            .into();
            self.error = false;
        }
        Task::none()
    }

    pub(super) fn select_monitor_preview(&mut self, display: String) {
        if self.placement.display.as_ref() == Some(&display) {
            return;
        }
        if self.canvas.is_some() && self.canvas_dirty() {
            self.failed("Apply or cancel this canvas before editing another display.".into());
            return;
        }
        self.placement.cancel_drag();
        if self.placement.dirty() {
            let _ = self.update_placement(Message::Save);
        }
        if !self.placement.dirty() {
            self.placement.display = Some(display);
            self.placement.refresh(&self.config);
            self.canvas_retarget();
            self.refresh_canvas();
        }
    }
    /// Local connection choice; ordinary buttons remain keyboard accessible.
    pub(super) fn display_selector(&self) -> Element<'_, super::Message> {
        let options: Vec<Element<'_, super::Message>> = self
            .displays
            .choices(self.config.widget_display.as_deref())
            .into_iter()
            .map(|(title, value)| {
                let selected = self
                    .displays
                    .same_choice(value.as_deref(), self.config.widget_display.as_deref());
                let enabled = value
                    .as_deref()
                    .is_none_or(|name| self.displays.unique(name).is_some());
                let id = format!("widget-display-{}", value.as_deref().unwrap_or("auto"));
                let button = if selected {
                    button::suggested(title)
                } else {
                    button::standard(title)
                };
                button
                    .id(cosmic::widget::Id::new(id))
                    .on_press_maybe(enabled.then_some(super::Message::WidgetDisplay(value)))
                    .into()
            })
            .collect();
        column::with_children(vec![
            label("Default widget display", 14., FOREGROUND).into(),
            row::with_children(options).spacing(6).wrap().into(),
            label(
                self.displays.status(self.config.widget_display.as_deref()),
                12.,
                MUTED,
            )
            .into(),
        ])
        .spacing(6)
        .into()
    }
    /// The widget placement canvas on Layout. `height` follows the window.
    pub(super) fn placement_picture(
        &self,
        height: f32,
        interactive: bool,
    ) -> Element<'_, super::Message> {
        let editor = &self.placement;
        let palette = self.config.palette();
        responsive(move |available| {
            let scale = (available.width.max(1.) / editor.snapshot.screen[0])
                .min(available.height.max(1.) / editor.snapshot.screen[1]);
            let screen = [
                editor.snapshot.screen[0] * scale,
                editor.snapshot.screen[1] * scale,
            ];
            let background = container(text::body(""))
                .width(screen[0])
                .height(screen[1])
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        background: Some(Background::Color(Color::from_rgb(0.055, 0.055, 0.075))),
                        border: Border {
                            color: Color {
                                a: 0.35,
                                ..palette.accent
                            },
                            width: 1.,
                            radius: 9.into(),
                        },
                        ..Default::default()
                    }
                }));
            let mut layers: Vec<Element<'_, super::Message>> = if self.page == super::Page::Overview
            {
                self.home_backdrop(screen)
            } else {
                vec![background.into()]
            };
            // Guides share the desktop grid's 24px horizontal and 48px vertical
            // safe margins, so a snapped miniature lands on the same line.
            if editor.guides {
                let [vertical, horizontal] = crate::layout::grid_lines(editor.snapshot.screen);
                let color = Color {
                    a: 0.10,
                    ..palette.accent
                };
                for x in vertical {
                    layers.push(
                        pin(line(
                            0.5,
                            (editor.snapshot.screen[1] - 96.).max(1.) * scale,
                            color,
                        ))
                        .position(Point::new(x * scale, 48. * scale))
                        .into(),
                    );
                }
                for y in horizontal {
                    layers.push(
                        pin(line(
                            (editor.snapshot.screen[0] - 48.).max(1.) * scale,
                            0.5,
                            color,
                        ))
                        .position(Point::new(24. * scale, y * scale))
                        .into(),
                    );
                }
            }
            let mut widgets: Vec<_> = editor
                .snapshot
                .widgets
                .iter()
                .filter(|geometry| editor.visible.contains(&geometry.key))
                .collect();
            // The selected card stays on top while it crosses another card.
            widgets
                .sort_by_key(|geometry| editor.selected.as_deref() == Some(geometry.key.as_str()));
            for geometry in widgets {
                let Some(position) = editor.position(&geometry.key) else {
                    continue;
                };
                let selected = editor.selected.as_deref() == Some(geometry.key.as_str());
                let dragging = editor
                    .drag
                    .as_ref()
                    .is_some_and(|drag| drag.key == geometry.key);
                let size = editor.display_size(&geometry.key).unwrap_or(geometry.size);
                layers.push(
                    pin(self.placement_card(
                        geometry,
                        size.map(|axis| axis * scale),
                        scale,
                        selected,
                        dragging,
                        interactive,
                    ))
                    .position(Point::new(position[0] * scale, position[1] * scale))
                    .into(),
                );
            }
            if editor.visible.is_empty() {
                layers.push(
                    container(label("Turn on a widget to place it", 12., MUTED))
                        .width(screen[0])
                        .height(screen[1])
                        .align_x(Alignment::Center)
                        .align_y(Alignment::Center)
                        .into(),
                );
            }
            let canvas = mouse_area(stack(layers).width(screen[0]).height(screen[1]).clip(true));
            let canvas = if interactive {
                canvas
                    .on_move(move |point| message(Message::Pointer(point, scale)))
                    .on_exit(message(Message::Leave))
                    .on_press(message(Message::ClearSelection))
            } else {
                canvas
            };
            container(
                container(canvas)
                    .id(cosmic::widget::Id::new("placement-canvas"))
                    .width(screen[0])
                    .height(screen[1]),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
        })
        .width(Length::Fill)
        .height(height)
        .into()
    }

    pub(super) fn placement_editor(&self, height: f32) -> Element<'_, super::Message> {
        let editor = &self.placement;
        let dirty = editor.dirty();
        let switch = |title: &'static str, value: bool, event: fn(bool) -> Message| {
            row::with_children(vec![
                label(title, 12., if value { FOREGROUND } else { MUTED }),
                toggler(value)
                    .on_toggle(move |value| message(event(value)))
                    .into(),
            ])
            .spacing(8)
            .align_y(Alignment::Center)
        };
        let preview = self.placement_picture(height, true);
        let hint = editor
            .selected
            .as_deref()
            .and_then(Widget::parse)
            .map(|widget| {
                format!(
                    "{} · arrows {} · Shift for more",
                    widget.title(),
                    if editor.resize_mode { "resize" } else { "move" }
                )
            })
            .unwrap_or_else(|| "Drag to move · select a widget to resize or customize".into());
        let caption = row::with_children(vec![
            container(label(hint, 11., MUTED))
                .width(Length::Fill)
                .into(),
            if dirty {
                label(
                    if self.canvas.is_some() && !editor.dragging() {
                        "Previewing"
                    } else {
                        "Moving…"
                    },
                    11.,
                    self.settings_accent(),
                )
            } else {
                label(
                    format!(
                        "{} × {}",
                        editor.snapshot.screen[0] as u32, editor.snapshot.screen[1] as u32
                    ),
                    11.,
                    MUTED,
                )
            },
        ])
        .spacing(10)
        .height(24)
        .align_y(Alignment::Center);
        let mut action_buttons = vec![
            switch("Snap", editor.snap, Message::Snap).into(),
            switch("Grid", editor.guides, Message::Guides).into(),
            container(text::body("")).width(Length::Fill).into(),
        ];
        if self.canvas.is_some() {
            action_buttons.push(
                button::text("Reset layout")
                    .on_press(message(Message::Reset))
                    .into(),
            );
        } else if !editor.session_actions.is_empty() || editor.dragging() || dirty {
            action_buttons.push(
                button::text("Cancel arrangement")
                    .on_press(message(Message::Cancel))
                    .into(),
            );
            if dirty && !editor.dragging() && self.error {
                action_buttons.push(
                    button::suggested("Retry move")
                        .on_press(message(Message::Save))
                        .into(),
                );
            }
        } else {
            action_buttons.push(
                button::text("Arrange on desktop")
                    .id(cosmic::widget::Id::new("layout-arrange"))
                    .on_press_maybe((!self.busy).then_some(super::Message::ArrangeDesktop))
                    .into(),
            );
            action_buttons.push(
                button::text("Reset layout")
                    .id(cosmic::widget::Id::new("layout-reset"))
                    .on_press_maybe((!self.busy).then_some(message(Message::Reset)))
                    .into(),
            );
        }
        // Reserve the same toolbar height: selecting or dragging must not
        // move the miniature under the pointer when draft actions appear.
        let actions = row::with_children(action_buttons)
            .spacing(8)
            .height(40)
            .align_y(Alignment::Center);
        let mut parts = vec![actions.into(), preview.into(), caption.into()];
        if matches!(self.page, super::Page::Overview | super::Page::WidgetLayout) {
            if self.canvas.is_some() {
                let choices = self
                    .displays
                    .outputs()
                    .iter()
                    .filter_map(|output| {
                        let key = self.displays.key(output)?;
                        Some(
                            button::text(output.name().unwrap_or("Display"))
                                .on_press(super::Message::MonitorPreview(key))
                                .into(),
                        )
                    })
                    .collect::<Vec<Element<'_, super::Message>>>();
                if choices.len() > 1 {
                    parts.insert(0, row::with_children(choices).spacing(6).wrap().into());
                }
            } else {
                parts.insert(0, self.monitor_overview());
            }
        }
        if self.page != super::Page::Overview {
            parts.push(self.placement_inspector());
        }
        column::with_children(parts)
            .spacing(12)
            .width(Length::Fill)
            .into()
    }

    pub(super) fn placement_dirty(&self) -> bool {
        self.placement.dirty()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cancel_arrangement_keeps_a_second_writers_interleaved_commit() {
        use cosmic::Application;
        if crate::displays::test_support::isolated(
            "settings::placement::tests::cancel_arrangement_keeps_a_second_writers_interleaved_commit",
        ) {
            return;
        }
        Config::save_changes(
            serde_json::json!({"enabled_widgets":["weather"], "placement":{"snap":true}}),
        )
        .unwrap();
        let (mut app, _) = Settings::init(
            cosmic::app::Core::default(),
            super::super::Page::WidgetLayout,
        );
        let _ = app.update_placement(Message::Snap(false));
        crate::test_env::AFTER_PREFERENCE_COMMIT.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(|| {
                std::thread::spawn(|| {
                    crate::desktop::actions::in_action("Other writer", || {
                        Config::save_changes(serde_json::json!({"calendar_private":true})).unwrap();
                    });
                })
                .join()
                .unwrap();
            }))
        });
        crate::desktop::actions::in_action("Widget arrangement", || {
            let _ = app.update_placement(Message::Save);
        });
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.placement.session_actions.len(), 1);
        let store = crate::desktop::actions::history_store().unwrap();
        let other = store.latest_change().unwrap().unwrap();
        assert_ne!(app.placement.session_actions[0].id, other.id);
        assert!(!Config::load().placement.snap);
        let _ = app.update_placement(Message::Cancel);
        assert!(!app.error, "{}", app.status);
        assert!(Config::load().placement.snap);
        assert!(Config::load().calendar_private);
        store.undo_change(&other).unwrap();
        assert!(!Config::load().calendar_private);
        assert!(Config::load().placement.snap);
    }

    #[test]
    fn fb08_geometry_epoch_cancels_only_inflight_gesture_and_keeps_prior_draft() {
        if run_isolated(
            "fb08_geometry_epoch_cancels_only_inflight_gesture_and_keeps_prior_draft",
            r#"{"enabled_widgets":["clock","weather"],"placement":{"snap":false}}"#,
        ) {
            return;
        }
        crate::desktop::actions::in_action("Existing resize", || {
            Config::save_changes(serde_json::json!({"widget_sizes":{"clock":[320,140]}}))
        })
        .unwrap();
        let receipt = crate::desktop::actions::history_store()
            .unwrap()
            .latest_change()
            .unwrap()
            .unwrap();
        let config = Config::load();
        let bytes = std::fs::read(Config::path().unwrap()).unwrap();
        for resized in [false, true] {
            let mut geometry = Geometry::read(&config);
            geometry.snapshot.display = Some(crate::layout_state::DisplayState {
                output: Some("DP-1".into()),
                ready: true,
                epoch: 10,
            });
            let mut editor = PlacementEditor::from_geometry(&config, geometry);
            editor.select("clock");
            editor.nudge([1, 0], false);
            editor.session_actions.push(receipt.clone());
            let before = editor.state();
            let at = editor.position("weather").unwrap();
            editor.pointer(Point::new(at[0] + 20., at[1] + 20.), 1.);
            editor.press("weather");
            editor.pointer(Point::new(at[0] - 40., at[1] + 60.), 1.);
            assert!(editor.dragging());
            assert_ne!(editor.state(), before);
            let mut next = Geometry {
                visible: editor.visible.clone(),
                snapshot: editor.snapshot.clone(),
                defaults: editor.defaults.clone(),
            };
            // Data-only updates wait until release and never dirty preferences.
            next.snapshot
                .widgets
                .iter_mut()
                .find(|g| g.key == "clock")
                .unwrap()
                .natural_height = Some(210.);
            editor.reconcile(next);
            assert!(editor.dragging());
            let mut changed = editor.pending.take().unwrap();
            if resized {
                changed.snapshot.screen = [1280., 720.];
            } else {
                changed.snapshot.display.as_mut().unwrap().epoch += 1;
            }
            editor.reconcile(changed);
            assert!(!editor.dragging());
            assert_eq!(editor.state(), before);
            assert_eq!(editor.session_actions.len(), 1);
            assert_eq!(editor.session_actions[0], receipt);
            assert!(!editor.inside && editor.pointer.is_none());
            editor.release();
            assert_eq!(
                editor.state(),
                before,
                "queued release cannot commit an obsolete gesture"
            );
            assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), bytes);
        }
    }
    #[test]
    fn multi_monitor_first_edit_uses_display_context_without_an_assignment() {
        let config = crate::config::Config::default();
        let mut geometry = super::Geometry::read(&config);
        geometry.snapshot.monitors = [
            ("eDP-1".into(), [1280., 720.]),
            ("DP-2".into(), [1920., 1080.]),
        ]
        .into();
        geometry.snapshot.widget_displays = [("weather".into(), "eDP-1".into())].into();
        let editor = super::PlacementEditor::from_geometry(&config, geometry);
        assert_eq!(
            editor.contexts.get("weather").map(String::as_str),
            Some("eDP-1")
        );
    }
    use super::*;

    fn start_canvas(app: &mut Settings) {
        use cosmic::Application;
        let _ = app.update(super::super::Message::Canvas(
            super::super::canvas::Message::Start,
        ));
        assert!(app.canvas.is_some(), "{}", app.status);
        assert!(app.placement_active());
    }

    fn apply_canvas(app: &mut Settings) {
        use cosmic::Application;
        let id = app.canvas.as_ref().expect("an active canvas").id;
        let _ = app.update(super::super::Message::Canvas(
            super::super::canvas::Message::Apply(id),
        ));
        assert!(!app.error, "{}", app.status);
        assert!(app.canvas.is_none(), "Apply must finish the canvas");
        assert!(!app.placement.dirty());
    }

    #[test]
    fn center_actions_preserve_neighbors_and_roundtrip_through_undo_and_save() {
        use super::super::{Message as App, Page};
        use cosmic::Application;
        if run_isolated(
            "center_actions_preserve_neighbors_and_roundtrip_through_undo_and_save",
            r#"{"enabled_widgets":["clock","weather"],"placement":{"snap":true},"widget_sizes":{"clock":[320,140]},"future_option":42}"#,
        ) {
            return;
        }
        let (mut app, _) = Settings::init(cosmic::app::Core::default(), Page::Clock);
        let neighbor = app.placement.position("weather").unwrap();
        let original = app.placement.position("clock").unwrap();
        let _ = app.update(App::EditWidgetLayout(Widget::Clock));
        assert_eq!(app.page, Page::Overview);
        assert_eq!(app.placement.selected(), Some(Widget::Clock));
        let _ = app.update(App::Placement(Message::Center(0)));
        let _ = app.update(App::Placement(Message::Center(1)));
        let p = app.placement.position("clock").unwrap();
        let size = app.placement.display_size("clock").unwrap();
        for axis in 0..2 {
            assert!((p[axis] + size[axis] / 2. - app.placement.screen()[axis] / 2.).abs() < 0.01);
        }
        assert_eq!(app.placement.position("weather"), Some(neighbor));
        assert!(app.canvas_dirty());
        assert!(
            Config::load().widget_positions.is_empty(),
            "Alignment remains local until Apply"
        );
        let _ = app.update(App::Placement(Message::Undo));
        assert!((app.placement.position("clock").unwrap()[1] - original[1]).abs() < 0.01);
        let _ = app.update(App::Placement(Message::Center(1)));
        apply_canvas(&mut app);
        let (reopened, _) = Settings::init(cosmic::app::Core::default(), Page::Overview);
        assert_eq!(reopened.placement.position("clock"), Some(p));
        assert_eq!(reopened.placement.position("weather"), Some(neighbor));
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap();
        assert_eq!(raw["future_option"], 42);
        let _ = app.update(App::Widget(Widget::Glance, true));
        let _ = app.update(App::EditWidgetLayout(Widget::Clock));
        assert_eq!(app.placement.selected(), Some(Widget::Glance));
        // Growing agendas encode their top independently of their height.
        for snapshot in [&mut app.placement.snapshot, &mut app.placement.defaults] {
            let geometry = snapshot
                .widgets
                .iter_mut()
                .find(|g| g.key == "glance")
                .unwrap();
            geometry.natural_height = Some(1600.);
        }
        std::fs::write(
            crate::layout_state::path().unwrap(),
            serde_json::to_vec(&app.placement.snapshot).unwrap(),
        )
        .unwrap();
        let _ = app.update(App::Placement(Message::Center(1)));
        assert!((app.placement.position("glance").unwrap()[1] - 48.).abs() < 0.01);
        assert!(
            (app.placement.display_size("glance").unwrap()[1] - (app.placement.screen()[1] - 96.))
                .abs()
                < 0.01
        );
    }

    #[test]
    fn home_resize_automatic_preserves_later_enabled_neighbor() {
        use super::super::{Message as App, Page};
        use cosmic::Application;
        if run_isolated(
            "home_resize_automatic_preserves_later_enabled_neighbor",
            r#"{"enabled_widgets":[],"focus":{"enabled":true},"placement":{"snap":false}}"#,
        ) {
            return;
        }
        let (mut app, _) = Settings::init(cosmic::app::Core::default(), Page::Overview);
        start_canvas(&mut app);
        app.placement.select("focus");
        let _ = app.update(App::Placement(Message::Automatic));
        assert!(
            !app.placement.dirty(),
            "Automatic is a no-op without an override"
        );
        assert!(app.placement.positions.is_empty());
        for _ in 0..5 {
            let _ = app.update(App::Placement(Message::Resize([0, 1], true)));
        }
        assert!(app.canvas_dirty());
        apply_canvas(&mut app);
        assert_eq!(Config::load().widget_sizes["focus"], [280., 320.]);
        assert!(!Config::load().widget_positions.contains_key("notes"));
        let _ = app.update(App::Widget(Widget::Notes, true));
        let _ = app.update(App::Tick);
        let before = app.placement.position("notes").unwrap();
        assert_eq!(before, [800., 422.]);
        let saved_before_automatic = Config::load();
        start_canvas(&mut app);
        app.placement.select("focus");
        let _ = app.update(App::Placement(Message::Automatic));
        assert_eq!(app.placement.position("notes"), Some(before));
        let _ = app.update(App::Placement(Message::Undo));
        assert_eq!(app.placement.display_size("focus"), Some([280., 320.]));
        assert_eq!(app.placement.position("notes"), Some(before));
        let _ = app.update(App::Placement(Message::Automatic));
        assert!(app.canvas_dirty());
        apply_canvas(&mut app);
        let after = app.placement.position("notes").unwrap();
        assert!(
            (0..2).all(|axis| (after[axis] - before[axis]).abs() < 0.01),
            "Automatic must preserve the neighboring card shown in the draft: {before:?} -> {after:?}"
        );
        let (reopened, _) = Settings::init(cosmic::app::Core::default(), Page::Overview);
        let reopened_point = reopened.placement.position("notes").unwrap();
        assert!((0..2).all(|axis| (reopened_point[axis] - before[axis]).abs() < 0.01));
        assert!(!Config::load().widget_sizes.contains_key("focus"));
        let _ = app.update(App::UndoPreferences);
        assert!(!app.error, "{}", app.status);
        let undone = Config::load();
        assert_eq!(undone.widget_sizes, saved_before_automatic.widget_sizes);
        assert_eq!(
            undone.widget_positions,
            saved_before_automatic.widget_positions
        );
        assert_eq!(app.placement.position("notes"), Some(before));
    }

    #[test]
    fn home_resize_clean_refresh_keeps_keyboard_resize_mode() {
        use super::super::{Message as App, Page};
        use cosmic::Application;
        if run_isolated(
            "home_resize_clean_refresh_keeps_keyboard_resize_mode",
            r#"{"enabled_widgets":[],"focus":{"enabled":true},"notes":{"enabled":true},"placement":{"snap":false}}"#,
        ) {
            return;
        }
        let (mut app, _) = Settings::init(cosmic::app::Core::default(), Page::Overview);
        start_canvas(&mut app);
        app.placement.select("focus");
        let old_point = app.placement.position("focus").unwrap();
        let _ = app.update(App::Placement(Message::ResizeMode(true)));
        assert!(!app.placement.dirty());
        // Tick follows watched config, layout, notes and focus file updates.
        let _ = app.update(App::Tick);
        let _ = app.update(App::Placement(Message::Nudge([0, 1], false)));
        assert_eq!(
            app.placement.display_size("focus"),
            Some([280., 168.]),
            "A live refresh must not switch a chosen resize arrow into a move"
        );
        let point = app.placement.position("focus").unwrap();
        assert!((0..2).all(|axis| (point[axis] - old_point[axis]).abs() < 0.01));
        let _ = app.update(App::Canvas(super::super::canvas::Message::Cancel));
        assert!(!app.placement_active());
        app.placement.select("focus");
        let _ = app.update(App::Placement(Message::Nudge([0, 1], false)));
        assert_eq!(
            app.placement.position("focus"),
            Some(old_point),
            "late actions after Cancel are ignored"
        );
        assert_eq!(app.placement.display_size("focus"), Some([280., 160.]));
        start_canvas(&mut app);
        app.placement.select("focus");
        let _ = app.update(App::Placement(Message::Nudge([0, 1], false)));
        assert_eq!(app.placement.display_size("focus"), Some([280., 160.]));
        let moved = app.placement.position("focus").unwrap();
        assert!((moved[1] - old_point[1] - 8.).abs() < 0.01);
    }

    #[test]
    fn resizing_is_a_draft_preserves_top_left_and_escape_restores_the_gesture() {
        let mut editor = editor(false);
        editor.pointer(Point::new(455., 275.), 1.);
        editor.resize_press("weather");
        editor.pointer(Point::new(575., 415.), 1.);
        assert_eq!(editor.display_size("weather"), Some([480., 320.]));
        let point = editor.position("weather").unwrap();
        assert!((point[0] - 100.).abs() < 0.001 && (point[1] - 100.).abs() < 0.001);
        assert!(editor.dirty());
        editor.cancel_drag();
        assert!(!editor.dirty());
        assert!(editor.materialized.is_empty());
        assert!(editor.sizes.is_empty());
        editor.pointer(Point::new(455., 275.), 1.);
        editor.resize_press("weather");
        editor.pointer(Point::new(575., 415.), 1.);
        editor.release();
        let saved = serde_json::to_vec(&editor.session_draft().unwrap()).unwrap();
        let mut reopened = super::tests::editor(false);
        reopened
            .restore_session(serde_json::from_slice(&saved).unwrap())
            .unwrap();
        assert_eq!(reopened.display_size("weather"), Some([480., 320.]));
        let undo = editor.history.pop().unwrap();
        editor.restore(undo);
        assert!(!editor.dirty());
        reopened.selected = Some("weather".into());
        reopened.automatic();
        assert_eq!(reopened.display_size("weather"), Some([360., 180.]));
        reopened.cancel();
        assert!(!reopened.dirty());
    }

    #[test]
    fn refreshed_automatic_geometry_does_not_reuse_the_previous_fixed_size() {
        let mut config = Config::default();
        config.enabled_widgets = vec!["weather".into()];
        config.widget_sizes.insert("weather".into(), [480., 320.]);
        let mut geometry = weather_geometry();
        geometry.snapshot.widgets[0].size = [480., 320.];
        geometry.snapshot.widgets[0].natural_size = Some([360., 180.]);
        let mut editor = PlacementEditor::from_geometry(&config, geometry);
        editor.select("weather");
        editor.automatic();
        assert_eq!(editor.display_size("weather"), Some([360., 180.]));
        let point = editor.position("weather").unwrap();
        assert!((point[0] - 100.).abs() < 0.001 && (point[1] - 100.).abs() < 0.001);
    }

    #[test]
    fn idle_geometry_refresh_keeps_the_pointer_ready_for_a_native_press() {
        let mut config = Config::default();
        config.enabled_widgets = vec!["weather".into()];
        let mut editor = PlacementEditor::new(&config);
        let point = editor.position("weather").unwrap();
        editor.pointer(
            Point::new((point[0] + 30.) * 0.25, (point[1] + 20.) * 0.25),
            0.25,
        );
        editor.refresh(&config);
        editor.press("weather");
        assert!(
            editor.dragging(),
            "An idle refresh must not discard the last cursor position before pressing"
        );
    }

    fn weather_geometry() -> Geometry {
        let snapshot = Snapshot {
            monitors: Default::default(),
            widget_displays: Default::default(),
            display: None,
            screen: [1920., 1080.],
            widgets: vec![WidgetGeometry {
                natural_size: None,
                natural_height: None,
                key: "weather".into(),
                size: [360., 180.],
                position: [100., 100.],
            }],
        };
        let mut defaults = snapshot.clone();
        defaults.widgets[0].position = [1518., 450.];
        Geometry {
            visible: BTreeSet::from(["weather".into()]),
            snapshot,
            defaults,
        }
    }

    fn editor(snap: bool) -> PlacementEditor {
        let mut config = Config::default();
        config.enabled_widgets = vec!["weather".into()];
        config.placement.snap = snap;
        PlacementEditor::from_geometry(&config, weather_geometry())
    }

    #[test]
    fn resetting_glance_keeps_an_explicit_default_and_preserves_hidden_clock() {
        let mut config = Config::default();
        config.glance.enabled = true;
        config.widget_positions.insert("clock".into(), [0.8, 0.6]);
        config.widget_positions.insert("glance".into(), [0.4, 0.3]);
        let mut geometry = weather_geometry();
        geometry.visible = BTreeSet::from(["glance".into()]);
        geometry.snapshot.widgets[0].key = "glance".into();
        geometry.defaults.widgets[0].key = "glance".into();
        geometry.defaults.widgets[0].position = [82., 88.];
        let mut editor = PlacementEditor::from_geometry(&config, geometry);
        editor.reset();
        assert_eq!(editor.positions["clock"], [0.8, 0.6]);
        let position = editor.position("glance").unwrap();
        assert!((position[0] - 82.).abs() < 0.001 && (position[1] - 88.).abs() < 0.001);
        assert!(editor.positions.contains_key("glance"));
        assert!(!editor.save_baseline().contains_key("glance"));
    }

    #[test]
    fn expanded_glance_miniature_can_be_grabbed_across_its_entire_visible_area() {
        let mut config = Config::default();
        config.glance.enabled = true;
        config.placement.snap = false;
        let mut geometry = weather_geometry();
        geometry.visible = BTreeSet::from(["glance".into()]);
        geometry.snapshot.widgets[0] = WidgetGeometry {
            natural_size: None,
            key: "glance".into(),
            size: [360., 252.],
            natural_height: Some(700.),
            position: [100., 780.],
        };
        geometry.defaults = geometry.snapshot.clone();
        let mut editor = PlacementEditor::from_geometry(&config, geometry);
        editor.put_point("glance", [100., 100.]);
        assert_eq!(editor.display_size("glance"), Some([360., 700.]));
        editor.pointer(Point::new(130., 600.), 1.);
        editor.press("glance");
        assert!(editor.dragging());
        editor.release();
        editor.put_point("glance", [100., 780.]);
        assert_eq!(editor.display_size("glance"), Some([360., 252.]));
        editor.pointer(Point::new(130., 1045.), 1.);
        editor.press("glance");
        assert!(!editor.dragging());
    }

    #[test]
    fn growing_glance_keeps_its_top_and_can_be_placed_near_the_bottom() {
        let mut config = Config::default();
        config.glance.enabled = true;
        config.placement.snap = false;
        let mut geometry = weather_geometry();
        geometry.visible = BTreeSet::from(["glance".into()]);
        geometry.snapshot.widgets[0] = WidgetGeometry {
            natural_size: None,
            natural_height: None,
            key: "glance".into(),
            size: [360., 700.],
            position: [100., 100.],
        };
        geometry.defaults = geometry.snapshot.clone();
        let mut editor = PlacementEditor::from_geometry(&config, geometry);
        editor.put_point("glance", [1480., 780.]);
        assert_eq!(editor.position("glance"), Some([1480., 780.]));
        editor.snapshot.widgets[0].size[1] = 240.;
        assert_eq!(editor.position("glance"), Some([1480., 780.]));
        editor.snapshot.widgets[0].size[1] = 900.;
        assert_eq!(editor.position("glance"), Some([1480., 780.]));
    }

    #[test]
    fn scaled_pointer_preserves_the_grab_offset_and_updates_only_a_draft() {
        let mut editor = editor(false);
        editor.pointer(Point::new(30., 30.), 0.25);
        editor.press("weather");
        editor.pointer(Point::new(130., 180.), 0.25);
        editor.release();
        let position = editor.position("weather").unwrap();
        assert!((position[0] - 500.).abs() < 0.001);
        assert!((position[1] - 700.).abs() < 0.001);
        assert!(editor.dirty());
        assert!(editor.saved_positions.is_empty());
        assert!(!editor.dragging());
    }

    #[test]
    fn miniature_drag_snaps_to_the_shared_desktop_grid() {
        let mut editor = editor(true);
        editor.pointer(Point::new(30., 30.), 0.25);
        editor.press("weather");
        editor.pointer(Point::new(70., 72.5), 0.25);
        editor.release();
        let position = editor.position("weather").unwrap();
        assert!((position[0] - 264.).abs() < 0.001);
        assert!((position[1] - 276.).abs() < 0.001);
    }

    #[test]
    fn outside_drop_and_escape_restore_the_drag_start_without_losing_earlier_edits() {
        let mut editor = editor(false);
        editor.selected = Some("weather".into());
        editor.nudge([1, 0], false);
        let earlier = editor.positions.clone();
        editor.pointer(Point::new(32., 30.), 0.25);
        editor.press("weather");
        editor.pointer(Point::new(100., 100.), 0.25);
        editor.inside = false;
        editor.release();
        assert_eq!(editor.positions, earlier);
        assert!(editor.dirty());
        editor.pointer(Point::new(32., 30.), 0.25);
        editor.press("weather");
        editor.pointer(Point::new(110., 90.), 0.25);
        editor.cancel_drag();
        assert_eq!(editor.positions, earlier);
        editor.cancel();
        assert!(!editor.dirty());
        assert_eq!(editor.position("weather"), Some([100., 100.]));
    }

    #[test]
    fn reset_uses_default_geometry_and_keeps_hidden_widget_positions() {
        let mut editor = editor(false);
        editor.positions.insert("calendar".into(), [0.3, 0.7]);
        editor.saved_positions = editor.positions.clone();
        editor.selected = Some("weather".into());
        editor.nudge([1, 0], false);
        editor.reset();
        assert_eq!(editor.position("weather"), Some([1518., 450.]));
        assert_eq!(editor.positions.get("calendar"), Some(&[0.3, 0.7]));
        editor.press("calendar");
        assert!(!editor.dragging());
        editor.cancel();
        assert_eq!(editor.position("weather"), Some([100., 100.]));
    }

    #[test]
    fn reset_without_explicit_positions_remains_a_draft_until_save_or_cancel() {
        let mut editor = editor(false);
        editor.reset();
        assert!(editor.dirty());
        editor.reconcile(weather_geometry());
        assert_eq!(editor.position("weather"), Some([1518., 450.]));
        assert!(editor.dirty());
        editor.cancel();
        assert!(!editor.dirty());
        assert_eq!(editor.position("weather"), Some([100., 100.]));
    }

    #[test]
    fn refresh_preserves_unsaved_grid_preferences_and_reload_resumes_after_cancel() {
        let mut editor = editor(false);
        editor.snap = true;
        let mut config = Config::default();
        config.enabled_widgets = vec!["weather".into()];
        config.placement.snap = false;
        config.placement.columns = 12;
        editor.refresh(&config);
        assert!(editor.snap);
        assert_eq!(editor.columns, 24);
        editor.cancel();
        editor.refresh(&config);
        assert!(!editor.snap);
        assert_eq!(editor.columns, 12);
        assert!(!editor.dirty());
    }

    #[test]
    fn hiding_the_grabbed_card_cancels_the_drag_and_adopts_new_geometry() {
        let mut editor = editor(false);
        editor.selected = Some("weather".into());
        editor.nudge([1, 0], false);
        let earlier = editor.positions.clone();
        editor.pointer(Point::new(32., 30.), 0.25);
        editor.press("weather");
        editor.pointer(Point::new(100., 100.), 0.25);
        let mut hidden = weather_geometry();
        hidden.visible.clear();
        editor.reconcile(hidden);
        assert!(!editor.dragging());
        assert!(editor.pending.is_none());
        assert_eq!(editor.positions, earlier);
        assert!(editor.selected.is_none());
        assert!(editor.position("weather").is_none());
    }

    #[test]
    fn invalid_pointer_scales_cannot_corrupt_positions() {
        let mut editor = editor(false);
        editor.pointer(Point::new(30., 30.), 0.25);
        editor.press("weather");
        for scale in [0., -1., f32::INFINITY, f32::NAN] {
            editor.pointer(Point::new(500., 500.), scale);
        }
        assert!(!editor.dirty());
        assert_eq!(editor.position("weather"), Some([100., 100.]));
    }

    fn run_isolated(name: &str, initial_config: &str) -> bool {
        if std::env::var("DESKTOP_CARDS_PLACEMENT_CASE")
            .ok()
            .as_deref()
            == Some(name)
        {
            return false;
        }
        let directory =
            std::env::temp_dir().join(format!("cards-placement-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let desktop = directory.join("desktop");
        std::fs::create_dir_all(desktop.join("config/cosmic")).unwrap();
        let path = directory.join("config.json");
        std::fs::write(&path, initial_config).unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg(format!("settings::placement::tests::{name}"))
            .env("DESKTOP_CARDS_PLACEMENT_CASE", name)
            .env("DESKTOP_CARDS_CONFIG", &path)
            .env("DESKTOP_CARDS_DESKTOP_ROOT", &desktop)
            .env_remove("DESKTOP_CARDS_SETTINGS_CAPTURE_DIR")
            .status()
            .unwrap();
        let _ = std::fs::remove_dir_all(directory);
        assert!(result.success(), "isolated placement case {name} failed");
        true
    }

    #[test]
    fn legacy_grid_is_clean_on_open_and_upgrades_only_when_a_layout_edit_is_saved() {
        use cosmic::Application;
        if run_isolated(
            "legacy_grid_is_clean_on_open_and_upgrades_only_when_a_layout_edit_is_saved",
            r#"{"enabled_widgets":["weather"],"placement":{"columns":12,"future_option":17},"widget_positions":{"weather":[0.4,0.4]}}"#,
        ) {
            return;
        }
        let path = Config::path().unwrap();
        let original = std::fs::read(&path).unwrap();
        let (mut settings, _) = Settings::init(
            cosmic::app::Core::default(),
            super::super::Page::WidgetLayout,
        );
        assert!(!settings.placement.dirty());
        assert_eq!(settings.placement.positions["weather"], [0.4, 0.4]);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let original_point = settings.placement.position("weather").unwrap();
        settings.placement.selected = Some("weather".into());
        let _ = settings.update_placement(Message::Nudge([-1, 0], false));
        let moved = settings.placement.position("weather").unwrap();
        assert!((6.0..=18.).contains(&(original_point[0] - moved[0])));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let _ = settings.update_placement(Message::Cancel);
        assert!(!settings.placement.dirty());
        assert_eq!(settings.placement.positions["weather"], [0.4, 0.4]);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let _ = settings.update_placement(Message::Nudge([-1, 0], false));
        let draft = settings.placement.positions.clone();
        let _ = settings.update_placement(Message::Save);
        assert!(!settings.error, "{}", settings.status);
        assert!(!settings.placement.dirty());
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(saved["widget_positions"], serde_json::json!(draft));
        assert_eq!(saved["placement"]["columns"], 24);
        assert_eq!(saved["placement"]["future_option"], 17);
    }

    #[test]
    fn reset_clears_concurrent_visible_positions_and_preserves_hidden_positions() {
        use cosmic::Application;
        if run_isolated(
            "reset_clears_concurrent_visible_positions_and_preserves_hidden_positions",
            r#"{"enabled_widgets":["weather","projects"],"widget_positions":{"weather":[0.2,0.3],"calendar":[0.2,0.4]}}"#,
        ) {
            return;
        }
        let path = Config::path().unwrap();
        let before = std::fs::read(&path).unwrap();
        let (mut settings, _) = Settings::init(
            cosmic::app::Core::default(),
            super::super::Page::WidgetLayout,
        );
        let _ = settings.update_placement(Message::Reset);
        let default_weather = settings.placement.position("weather");
        let default_projects = settings.placement.position("projects");
        assert!(settings.placement.dirty());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        Config::save_changes(serde_json::json!({
            "theme":"amber",
            "widget_positions":{
                "weather":[0.7,0.8], "projects":[0.4,0.5],
                "calendar":[0.6,0.7], "clock":[0.8,0.9]
            }
        }))
        .unwrap();
        let _ = settings.update_placement(Message::Save);
        assert!(!settings.error, "{}", settings.status);
        let saved = Config::try_load().unwrap();
        assert!(!saved.widget_positions.contains_key("weather"));
        assert!(!saved.widget_positions.contains_key("projects"));
        assert_eq!(saved.widget_positions["calendar"], [0.6, 0.7]);
        assert_eq!(saved.widget_positions["clock"], [0.8, 0.9]);
        assert_eq!(saved.theme, "amber");
        assert_eq!(settings.placement.position("weather"), default_weather);
        assert_eq!(settings.placement.position("projects"), default_projects);
        assert!(!settings.placement.dirty());
    }

    #[test]
    fn reset_then_move_saves_every_visible_draft_position_and_keeps_hidden_changes() {
        use cosmic::Application;
        if run_isolated(
            "reset_then_move_saves_every_visible_draft_position_and_keeps_hidden_changes",
            r#"{"enabled_widgets":["weather","projects"],"placement":{"snap":false},"widget_positions":{"weather":[0.2,0.3],"calendar":[0.2,0.4]}}"#,
        ) {
            return;
        }
        let (mut settings, _) = Settings::init(
            cosmic::app::Core::default(),
            super::super::Page::WidgetLayout,
        );
        let _ = settings.update_placement(Message::Reset);
        let before_move = settings.placement.position("weather").unwrap();
        settings.placement.selected = Some("weather".into());
        let _ = settings.update_placement(Message::Nudge([-1, 0], false));
        let moved = settings.placement.position("weather").unwrap();
        assert!((moved[0] - (before_move[0] - 8.)).abs() < 0.001);
        let desired = settings.placement.positions.clone();
        Config::save_changes(serde_json::json!({"widget_positions":{
            "weather":[0.7,0.8], "projects":[0.4,0.5],
            "calendar":[0.6,0.7], "clock":[0.8,0.9]
        }}))
        .unwrap();
        let _ = settings.update_placement(Message::Save);
        assert!(!settings.error, "{}", settings.status);
        let saved = Config::try_load().unwrap();
        assert_eq!(saved.widget_positions["weather"], desired["weather"]);
        assert_eq!(saved.widget_positions["projects"], desired["projects"]);
        assert_eq!(saved.widget_positions["calendar"], [0.6, 0.7]);
        assert_eq!(saved.widget_positions["clock"], [0.8, 0.9]);
        assert!(!settings.placement.dirty());
    }

    #[test]
    fn resizing_pins_automatic_neighbors_without_overwriting_external_moves() {
        use cosmic::Application;
        if run_isolated(
            "resizing_pins_automatic_neighbors_without_overwriting_external_moves",
            r#"{"enabled_widgets":["clock","weather","devbox","projects"],"focus":{"enabled":true},"notes":{"enabled":true},"placement":{"snap":false}}"#,
        ) {
            return;
        }
        let (mut settings, _) =
            Settings::init(cosmic::app::Core::default(), super::super::Page::Overview);
        let initial: BTreeMap<_, _> = ["clock", "weather", "projects", "focus", "notes"]
            .into_iter()
            .map(|key| (key, settings.placement.position(key).unwrap()))
            .collect();
        for key in ["weather", "focus"] {
            start_canvas(&mut settings);
            let saved_before = Config::load().widget_sizes;
            settings.placement.select(key);
            let _ = settings.update(super::super::Message::Placement(Message::Resize(
                [0, 1],
                true,
            )));
            assert!(settings.canvas_dirty());
            assert_eq!(
                Config::load().widget_sizes,
                saved_before,
                "Resize stays local until Apply"
            );
            apply_canvas(&mut settings);
        }
        let mut positions = Config::load().widget_positions;
        positions.insert("devbox".into(), [0.3, 0.2]);
        Config::save_changes(serde_json::json!({"widget_positions":positions,"theme":"amber"}))
            .unwrap();
        let saved = Config::try_load().unwrap();
        let reopened = PlacementEditor::new(&saved);
        for (key, point) in initial {
            let actual = reopened.position(key).unwrap();
            assert!(
                (0..2).all(|axis| (actual[axis] - point[axis]).abs() < 0.01),
                "{key} moved from {point:?} to {actual:?} after saving a neighbor's size"
            );
        }
        assert_eq!(saved.widget_positions["devbox"], [0.3, 0.2]);
        assert_eq!(saved.theme, "amber");
        let _ = settings.update(super::super::Message::UndoPreferences);
        assert!(
            Config::load().widget_sizes.contains_key("weather"),
            "each explicitly applied canvas has its own Undo"
        );
        let _ = settings.update(super::super::Message::UndoDesktop);
        assert!(
            settings.undo_review.is_some(),
            "the externally moved pinned neighbor is reviewed locally"
        );
        let review = settings.undo_review.clone().unwrap();
        let _ = settings.update(super::super::Message::ResolveUndoShown(review, true));
        let undone = Config::try_load().unwrap();
        assert!(undone.widget_sizes.is_empty());
        assert_eq!(
            undone.widget_positions,
            BTreeMap::from([("devbox".into(), [0.3, 0.2])])
        );
        assert_eq!(undone.theme, "amber");
    }

    #[test]
    fn ordinary_move_preserves_concurrent_changes_to_untouched_implicit_positions() {
        use cosmic::Application;
        if run_isolated(
            "ordinary_move_preserves_concurrent_changes_to_untouched_implicit_positions",
            r#"{"enabled_widgets":["weather","projects"],"placement":{"snap":false}}"#,
        ) {
            return;
        }
        let (mut settings, _) = Settings::init(
            cosmic::app::Core::default(),
            super::super::Page::WidgetLayout,
        );
        settings.placement.selected = Some("weather".into());
        let _ = settings.update_placement(Message::Nudge([-1, 0], false));
        let desired_weather = settings.placement.positions["weather"];
        Config::save_changes(serde_json::json!({"widget_positions":{
            "projects":[0.4,0.5], "clock":[0.8,0.9]
        }}))
        .unwrap();
        let _ = settings.update_placement(Message::Save);
        assert!(!settings.error, "{}", settings.status);
        let saved = Config::try_load().unwrap();
        assert_eq!(saved.widget_positions["weather"], desired_weather);
        assert_eq!(saved.widget_positions["projects"], [0.4, 0.5]);
        assert_eq!(saved.widget_positions["clock"], [0.8, 0.9]);
        assert!(!settings.placement.dirty());
    }

    #[test]
    fn failed_save_survives_idle_events_until_an_edit_retry_or_cancel() {
        use cosmic::Application;
        if run_isolated(
            "failed_save_survives_idle_events_until_an_edit_retry_or_cancel",
            r#"{"enabled_widgets":["weather"]}"#,
        ) {
            return;
        }
        let path = Config::path().unwrap();
        let original = std::fs::read(&path).unwrap();
        let (mut settings, _) = Settings::init(
            cosmic::app::Core::default(),
            super::super::Page::WidgetLayout,
        );
        let _ = settings.update_placement(Message::Snap(false));
        let _ = settings.update_placement(Message::Columns(24));
        std::fs::write(&path, "{").unwrap();
        let _ = settings.update_placement(Message::Save);
        assert!(settings.error);
        let error = settings.status.clone();
        for event in [
            Message::Pointer(Point::new(1., 1.), 0.25),
            Message::Release,
            Message::Press("weather".into()),
            Message::Guides(false),
            Message::Leave,
            Message::ClearSelection,
            Message::CancelDrag,
            Message::Columns(24),
            Message::Snap(false),
            Message::Nudge([1, 0], false),
        ] {
            let name = format!("{event:?}");
            let _ = settings.update_placement(event);
            assert!(settings.error, "{name} must preserve the save error");
            assert_eq!(
                settings.status, error,
                "{name} must preserve the save error"
            );
        }
        let _ = settings.update(super::super::Message::Tick);
        assert!(settings.error);
        assert_eq!(settings.status, error);
        settings.placement.selected = Some("weather".into());
        let _ = settings.update_placement(Message::Nudge([-1, 0], false));
        assert!(!settings.error);
        assert_eq!(settings.status, "Finish this gesture to save");
        let _ = settings.update_placement(Message::Save);
        assert!(settings.error);
        std::fs::write(&path, original).unwrap();
        let _ = settings.update_placement(Message::Save);
        assert!(!settings.error);
        assert_eq!(settings.status, "Layout saved");
        assert!(!settings.placement.dirty());
        let _ = settings.update_placement(Message::Snap(true));
        std::fs::write(&path, "{").unwrap();
        let _ = settings.update_placement(Message::Save);
        assert!(settings.error);
        let _ = settings.update_placement(Message::Cancel);
        assert!(!settings.error);
        assert_eq!(settings.status, "Layout changes canceled");
        assert!(!settings.placement.dirty());
    }

    #[test]
    fn saving_layout_is_explicit_and_preserves_other_settings() {
        use cosmic::Application;
        if std::env::var_os("DESKTOP_CARDS_PLACEMENT_TEST").is_none() {
            let directory =
                std::env::temp_dir().join(format!("cards-placement-test-{}", std::process::id()));
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("config.json");
            std::fs::write(
                &path,
                r#"{"theme":"teal","enabled_widgets":["weather"],"placement":{"columns":16,"future_option":17},"future_option":42}"#,
            )
            .unwrap();
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "settings::placement::tests::saving_layout_is_explicit_and_preserves_other_settings"])
                .env("DESKTOP_CARDS_PLACEMENT_TEST", "1").env("DESKTOP_CARDS_CONFIG", &path).status().unwrap();
            let _ = std::fs::remove_dir_all(directory);
            assert!(result.success());
            return;
        }
        let path = Config::path().unwrap();
        let before = std::fs::read(&path).unwrap();
        let (mut settings, _) = Settings::init(
            cosmic::app::Core::default(),
            super::super::Page::WidgetLayout,
        );
        let _ = settings.update_placement(Message::Snap(false));
        let _ = settings.update_placement(Message::Columns(24));
        settings.placement.selected = Some("weather".into());
        let _ = settings.update_placement(Message::Nudge([-1, 0], false));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let _ = settings.update_placement(Message::Cancel);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(!settings.placement.dirty());
        let _ = settings.update_placement(Message::Snap(false));
        let _ = settings.update_placement(Message::Columns(24));
        settings.placement.selected = Some("weather".into());
        let _ = settings.update_placement(Message::Nudge([-1, 0], false));
        Config::save_changes(serde_json::json!({"theme":"amber", "future_option":43})).unwrap();
        let _ = settings.update_placement(Message::Save);
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(saved["theme"], "amber");
        assert_eq!(saved["future_option"], 43);
        assert_eq!(saved["placement"]["snap"], false);
        assert_eq!(saved["placement"]["columns"], 24);
        assert_eq!(saved["placement"]["future_option"], 17);
        assert_eq!(saved["enabled_widgets"], serde_json::json!(["weather"]));
        assert!(saved["widget_positions"]["weather"].is_array());
        assert!(!settings.placement.dirty());
    }

    #[test]
    fn layout_links_open_the_arrangement_editor() {
        use super::super::{Message as App, Page};
        use cosmic::Application;
        if run_isolated(
            "layout_links_open_the_arrangement_editor",
            r#"{"enabled_widgets":["weather"]}"#,
        ) {
            return;
        }
        assert_eq!(Page::parse("layout"), Page::Overview);
        assert!(Page::ALL.contains(&Page::WidgetLayout));
        assert_eq!(Page::WidgetLayout.nav(), Page::Overview);
        let (mut settings, _) = <Settings as cosmic::Application>::init(
            cosmic::app::Core::default(),
            Page::parse("layout"),
        );
        assert!(
            !settings.placement_active(),
            "Layout opens its read-only landing"
        );
        start_canvas(&mut settings);
        let _ = settings.update(App::Page(Page::Calendar));
        assert!(!settings.placement_active());
        assert!(
            settings.canvas.is_some(),
            "Navigation keeps the local canvas"
        );
        let _ = settings.update(App::Page(Page::Overview));
        assert!(settings.placement_active());
        let _ = settings.update(App::Canvas(super::super::canvas::Message::Cancel));
        assert!(!settings.placement_active());
    }

    #[test]
    fn layout_draft_survives_settings_visits_and_follows_visibility() {
        use super::super::{Message as App, Page};
        use cosmic::Application;
        if run_isolated(
            "layout_draft_survives_settings_visits_and_follows_visibility",
            r#"{"enabled_widgets":["weather","projects"],"placement":{"snap":false}}"#,
        ) {
            return;
        }
        let (mut settings, _) = Settings::init(cosmic::app::Core::default(), Page::WidgetLayout);
        start_canvas(&mut settings);
        let before = Config::load().widget_positions;
        settings.placement.selected = Some("weather".into());
        let _ = settings.update(App::Placement(Message::Nudge([-1, 0], false)));
        assert!(settings.placement.dirty());
        let draft = settings.placement.positions.clone();
        // Visiting a widget's page keeps the draft and silences the editor.
        let _ = settings.update(App::Page(Page::Weather));
        assert!(settings.placement.dirty());
        let _ = settings.update(App::Placement(Message::Nudge([-1, 0], true)));
        let _ = settings.update(App::Placement(Message::Cancel));
        assert_eq!(settings.placement.positions, draft);
        // Showing a card adds it to the miniature without discarding the draft.
        let _ = settings.update(App::Widget(Widget::Devbox, true));
        assert!(settings.placement.visible.contains("devbox"));
        assert!(settings.placement.position("devbox").is_some());
        assert_eq!(settings.placement.positions, draft);
        // Hiding the selected card clears the selection so arrows cannot move it.
        let _ = settings.update(App::Widget(Widget::Weather, false));
        assert!(settings.placement.selected.is_none());
        assert!(settings.placement.position("weather").is_none());
        let _ = settings.update(App::Widget(Widget::Weather, true));
        let _ = settings.update(App::Tick);
        assert_eq!(settings.placement.positions, draft);
        let _ = settings.update(App::Page(Page::WidgetLayout));
        assert!(settings.placement_active());
        assert!(settings.canvas_dirty());
        assert_eq!(Config::load().widget_positions, before);
        apply_canvas(&mut settings);
        let saved = Config::try_load().unwrap();
        assert_eq!(saved.widget_positions["weather"], draft["weather"]);
        assert!(saved.widget_visible(Widget::Devbox));
        assert!(!settings.placement.dirty());
    }

    #[test]
    fn column_reorder_and_miniature_drag_never_overlap() {
        use super::super::{Message as App, Page};
        use cosmic::Application;
        if run_isolated(
            "column_reorder_and_miniature_drag_never_overlap",
            r#"{"enabled_widgets":["weather","devbox","projects"],"widget_order":["weather","devbox","projects"],"placement":{"snap":false}}"#,
        ) {
            return;
        }
        let order = vec![Widget::Weather, Widget::Devbox, Widget::Projects];
        let (mut settings, _) = Settings::init(cosmic::app::Core::default(), Page::WidgetLayout);
        start_canvas(&mut settings);
        let before = Config::load().widget_positions;
        let at = settings.placement.position("weather").unwrap();
        let point = Point::new((at[0] + 20.) * 0.25, (at[1] + 20.) * 0.25);
        // A column drag owns the pointer: the miniature ignores presses and Esc.
        let _ = settings.update(App::DragStart(Widget::Weather));
        assert_eq!(settings.reorder.source, Some(Widget::Weather));
        let _ = settings.update(App::Placement(Message::Pointer(point, 0.25)));
        let _ = settings.update(App::Placement(Message::Press("weather".into())));
        assert!(!settings.placement.dragging());
        let _ = settings.update(App::DragCancel);
        let _ = settings.update(App::Placement(Message::Escape));
        assert_ne!(settings.status, "Layout changes canceled");
        // A miniature drag blocks starting a column drag.
        let _ = settings.update(App::Placement(Message::Pointer(point, 0.25)));
        let _ = settings.update(App::Placement(Message::Press("weather".into())));
        assert!(settings.placement.dragging());
        let _ = settings.update(App::DragStart(Widget::Devbox));
        assert!(settings.reorder.source.is_none());
        let _ = settings.update(App::Placement(Message::Pointer(
            Point::new(point.x + 30., point.y + 10.),
            0.25,
        )));
        let _ = settings.update(App::Placement(Message::Release));
        assert!(settings.placement.dirty());
        assert_eq!(Config::load().widget_positions, before);
        // A local canvas also blocks automatic column reordering until cancelled.
        let _ = settings.update(App::Move(Widget::Weather, 1));
        let _ = settings.update(App::DragStart(Widget::Weather));
        assert!(settings.reorder.source.is_none());
        assert_eq!(Config::load().ordered_right_cards(), order);
        let _ = settings.update(App::Canvas(super::super::canvas::Message::Cancel));
        assert!(!settings.placement_active());
        assert_eq!(Config::load().widget_positions, before);
        let _ = settings.update(App::Move(Widget::Weather, 1));
        assert_eq!(
            Config::load().ordered_right_cards(),
            vec![Widget::Devbox, Widget::Weather, Widget::Projects]
        );
    }

    fn publish_devbox_height(height: f32) {
        let mut state = crate::layout_state::defaults(&Config::default());
        for widget in &mut state.widgets {
            if widget.key == "devbox" {
                widget.size[1] = height;
            }
        }
        let path = crate::layout_state::path().unwrap();
        std::fs::write(path, serde_json::to_vec(&state).unwrap()).unwrap();
    }

    #[test]
    fn unsaved_draft_adopts_fresh_runtime_geometry_after_a_settings_visit() {
        use super::super::{Message as App, Page};
        use cosmic::Application;
        if run_isolated(
            "unsaved_draft_adopts_fresh_runtime_geometry_after_a_settings_visit",
            r#"{"enabled_widgets":["weather","devbox","projects"],"placement":{"snap":false}}"#,
        ) {
            return;
        }
        publish_devbox_height(280.);
        let (mut settings, _) = Settings::init(cosmic::app::Core::default(), Page::WidgetLayout);
        start_canvas(&mut settings);
        let size = |settings: &Settings| settings.placement.geometry("devbox").unwrap().size;
        assert_eq!(size(&settings), [360., 280.]);
        settings.placement.selected = Some("weather".into());
        let _ = settings.update(App::Placement(Message::Nudge([-1, 0], false)));
        let _ = settings.update(App::Placement(Message::Columns(24)));
        let draft = settings.placement.positions.clone();
        let baseline = settings.placement.save_baseline();
        let before = std::fs::read(Config::path().unwrap()).unwrap();
        // Devbox metrics change its height while the draft waits on another page.
        let _ = settings.update(App::Page(Page::Devbox));
        publish_devbox_height(220.);
        let _ = settings.update(App::Tick);
        let _ = settings.update(App::Page(Page::WidgetLayout));
        assert_eq!(size(&settings), [360., 220.]);
        assert_eq!(settings.placement.positions, draft);
        assert_eq!(settings.placement.columns, 24);
        assert_eq!(settings.placement.save_baseline(), baseline);
        assert!(settings.placement.dirty());
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
        // Geometry published mid-drag waits so the grab offset stays exact,
        // then lands as soon as the drag ends without another file event.
        let at = settings.placement.position("weather").unwrap();
        let grab = Point::new((at[0] + 20.) * 0.25, (at[1] + 20.) * 0.25);
        let _ = settings.update(App::Placement(Message::Pointer(grab, 0.25)));
        let _ = settings.update(App::Placement(Message::Press("weather".into())));
        assert!(settings.placement.dragging());
        publish_devbox_height(300.);
        let _ = settings.update(App::Tick);
        assert_eq!(size(&settings), [360., 220.]);
        let _ = settings.update(App::Placement(Message::Pointer(
            Point::new(grab.x - 10., grab.y + 10.),
            0.25,
        )));
        let moved = settings.placement.position("weather").unwrap();
        assert!((moved[0] - (at[0] - 40.)).abs() < 0.01);
        assert!((moved[1] - (at[1] + 40.)).abs() < 0.01);
        let _ = settings.update(App::Placement(Message::Release));
        assert_eq!(size(&settings), [360., 300.]);
        assert!(settings.placement.dirty());
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
        // Apply still writes only what the draft changed.
        apply_canvas(&mut settings);
        let saved = Config::try_load().unwrap();
        assert_eq!(
            saved.widget_positions.keys().collect::<Vec<_>>(),
            vec!["weather"]
        );
        assert_eq!(saved.placement.columns, 24);
        assert_eq!(size(&settings), [360., 300.]);
    }
    #[test]
    fn recovered_layout_keeps_intent_and_original_merge_baselines() {
        let mut config = Config::default();
        config.widget_positions.insert("weather".into(), [0.7, 0.2]);
        let geometry = || Geometry {
            visible: Widget::ALL
                .into_iter()
                .map(|widget| widget.key().into())
                .collect(),
            snapshot: crate::layout_state::defaults(&config),
            defaults: crate::layout_state::defaults(&config),
        };
        let mut editor = PlacementEditor::from_geometry(&config, geometry());
        editor.positions.insert("weather".into(), [0.5, 0.3]);
        editor.materialized.insert("calendar".into(), [0.1, 0.4]);
        editor.snap = !editor.saved_snap;
        let saved = editor.session_draft().unwrap();
        let encoded = ron::to_string(&saved).unwrap();
        let original_baseline = editor.save_baseline();
        // Fresh config can move another card while Settings is closed.
        let mut fresh_config = config.clone();
        fresh_config
            .widget_positions
            .insert("calendar".into(), [0.2, 0.3]);
        let mut reopened = PlacementEditor::from_geometry(&fresh_config, geometry());
        reopened
            .restore_session(ron::from_str(&encoded).unwrap())
            .unwrap();
        assert!(reopened.dirty());
        assert_eq!(reopened.positions, editor.positions);
        assert_eq!(reopened.save_baseline(), original_baseline);
        assert_eq!(reopened.saved_positions, config.widget_positions);
        assert!(!reopened.dragging());
        reopened.cancel();
        assert!(!reopened.dirty());
    }
}

#[cfg(test)]
mod review030_tests {
    use super::super::{Message as SettingsMessage, Page};
    use super::*;
    use crate::displays::{
        OutputEvent,
        test_support::{Wire, info, isolated},
    };
    use cosmic::{Application, app::Core};

    fn outputs(app: &mut Settings) -> Wire {
        let wire = Wire::new();
        for (id, name, size) in [(1, "DP-1", (1920, 1080)), (2, "DP-2", (1280, 720))] {
            let _ = app.update(SettingsMessage::DisplayOutput(
                OutputEvent::Created(Some(info(Some(name), size))),
                wire.output(id),
            ));
        }
        wire
    }

    #[test]
    fn review030_live_layout_without_snapshot_can_select_move_save_and_guard_hotplug() {
        if isolated(
            "settings::placement::review030_tests::review030_live_layout_without_snapshot_can_select_move_save_and_guard_hotplug",
        ) {
            return;
        }
        let _ = std::fs::remove_file(crate::layout_state::path().unwrap());
        Config::save_changes(serde_json::json!({"enabled_widgets":["weather"],"widget_monitors":{"weather":{"display":"DP-2"}}})).unwrap();
        let (mut app, _) = Settings::init(Core::default(), Page::WidgetLayout);
        let _wire = outputs(&mut app);
        let _ = app.update_placement(Message::Select("weather".into()));
        assert_eq!(app.placement.display.as_deref(), Some("DP-2"));
        assert_eq!(app.placement.screen(), [1280., 720.]);
        assert!(app.placement.geometry("weather").is_some());
        assert_eq!(app.placement.contexts, app.displays.contexts(&app.config));
        let _ = app.update_placement(Message::Nudge([-1, 0], false));
        assert!(app.placement.dirty());
        let moved = app.placement.positions["weather"];
        let _ = app.update_placement(Message::Save);
        assert!(!app.error, "{}", app.status);
        assert!(!app.placement.dirty());
        assert_eq!(
            Config::load().monitor_layouts["weather"]["DP-2"].position,
            Some(moved)
        );
        let _ = app.update_placement(Message::Nudge([-1, 0], false));
        let output = app.displays.unique("DP-2").unwrap().handle.clone();
        let _ = app.update(SettingsMessage::DisplayOutput(OutputEvent::Removed, output));
        let _ = app.update_placement(Message::Save);
        assert!(app.error && app.placement.dirty());
        assert_eq!(
            Config::load().monitor_layouts["weather"]["DP-2"].position,
            Some(moved)
        );
    }

    #[test]
    fn review030_clock_shortcut_selects_target_monitor_and_moves_clock_or_glance() {
        if isolated(
            "settings::placement::review030_tests::review030_clock_shortcut_selects_target_monitor_and_moves_clock_or_glance",
        ) {
            return;
        }
        for glance in [false, true] {
            Config::save_changes(serde_json::json!({
                "enabled_widgets":["clock"], "glance":{"enabled":glance},
                "widget_positions":{}, "monitor_layouts":{},
                "widget_monitors":{"clock":{"display":"DP-2"},"glance":{"display":"DP-2"}}
            }))
            .unwrap();
            let (mut app, _) = Settings::init(Core::default(), Page::Clock);
            let _wire = outputs(&mut app);
            app.select_monitor_preview("DP-1".into());
            let _ = app.update(SettingsMessage::EditWidgetLayout(Widget::Clock));
            let key = if glance { "glance" } else { "clock" };
            assert_eq!(app.placement.display.as_deref(), Some("DP-2"));
            assert_eq!(app.placement.selected.as_deref(), Some(key));
            assert!(app.placement.geometry(key).is_some());
            let before = app.placement.position(key).unwrap();
            let _ = app.update_placement(Message::Nudge([1, 0], false));
            assert!(app.placement.dirty());
            assert_ne!(app.placement.position(key).unwrap(), before);
        }
    }

    #[test]
    fn fb08_selected_output_scale_change_cancels_drag_but_unrelated_output_does_not() {
        if isolated(
            "settings::placement::review030_tests::fb08_selected_output_scale_change_cancels_drag_but_unrelated_output_does_not",
        ) {
            return;
        }
        Config::save_changes(serde_json::json!({"enabled_widgets":["weather"],"widget_monitors":{"weather":{"display":"DP-2"}}})).unwrap();
        let (mut app, _) = Settings::init(Core::default(), Page::WidgetLayout);
        let _wire = outputs(&mut app);
        let _ = app.update_placement(Message::Select("weather".into()));
        let before = app.placement.state();
        let bytes = std::fs::read(Config::path().unwrap()).unwrap();
        let at = app.placement.position("weather").unwrap();
        app.placement
            .pointer(Point::new(at[0] + 20., at[1] + 20.), 1.);
        app.placement.press("weather");
        app.placement
            .pointer(Point::new(at[0] - 40., at[1] + 60.), 1.);
        let other = app.displays.unique("DP-1").unwrap().handle.clone();
        let _ = app.update(SettingsMessage::DisplayOutput(
            OutputEvent::InfoUpdate(info(Some("DP-1"), (1280, 720))),
            other,
        ));
        assert!(app.placement.dragging());
        let output = app.displays.unique("DP-2").unwrap().handle.clone();
        let mut updated = info(Some("DP-2"), (1280, 720));
        updated.scale_factor = 2;
        let _ = app.update(SettingsMessage::DisplayOutput(
            OutputEvent::InfoUpdate(updated),
            output,
        ));
        assert!(!app.placement.dragging());
        assert_eq!(app.placement.state(), before);
        let _ = app.update(SettingsMessage::Placement(Message::Release));
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), bytes);
    }

    #[test]
    fn review030_monitor_only_layout_blocks_automatic_reordering() {
        if isolated(
            "settings::placement::review030_tests::review030_monitor_only_layout_blocks_automatic_reordering",
        ) {
            return;
        }
        Config::save_changes(serde_json::json!({
            "enabled_widgets":["weather","devbox"],
            "widget_positions":{}, "widget_sizes":{},
            "monitor_layouts":{
                "weather":{"DP-1":{"position":[0.2,0.2]}},
                "devbox":{"DP-2":{"size":[400.,300.]}}
            }
        }))
        .unwrap();
        let (mut app, _) = Settings::init(Core::default(), Page::Overview);
        let before = app.config.ordered_right_cards();
        assert!(app.config.has_custom_positions());
        let _ = app.update(SettingsMessage::Move(Widget::Weather, 1));
        let _ = app.update(SettingsMessage::DragStart(Widget::Weather));
        assert!(app.reorder.source.is_none());
        assert_eq!(Config::load().ordered_right_cards(), before);
        app.config = Config::save_changes(serde_json::json!({"monitor_layouts":{}})).unwrap();
        assert!(!app.config.has_custom_positions());
        let _ = app.update(SettingsMessage::Move(Widget::Weather, 1));
        assert_ne!(Config::load().ordered_right_cards(), before);
    }
}

impl PlacementEditor {
    pub(super) fn canvas_project(&mut self, config: &Config) {
        self.reconcile(Geometry::on_live_display(
            config,
            self.display.as_deref(),
            &self.displays,
        ));
    }
    pub(super) fn canvas_snapshot(&self) -> SavedPlacement {
        SavedPlacement {
            contexts: self.contexts.clone(),
            saved_sizes: self.saved_sizes.clone(),
            sizes: self.sizes.clone(),
            saved_positions: self.saved_positions.clone(),
            positions: self.positions.clone(),
            materialized: self.materialized.clone(),
            saved_snap: self.saved_snap,
            saved_columns: self.saved_columns,
            snap: self.snap,
            columns: self.columns,
            reset: self.reset,
        }
    }
    pub(super) fn canvas_apply_snapshot(&self) -> SavedPlacement {
        let mut saved = self.canvas_snapshot();
        saved.saved_positions = self.save_baseline();
        saved.saved_sizes = self.size_baseline();
        if self.dirty() {
            saved.columns = PlacementPreferences::FINE_COLUMNS;
        }
        saved
    }
    pub(super) fn canvas_end(&mut self, config: &Config) {
        self.cancel_drag();
        self.cancel();
        self.session_actions.clear();
        self.refresh(config);
    }
    pub(super) fn canvas_stop_gesture(&mut self) {
        self.cancel_drag();
    }
}
