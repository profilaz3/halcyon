//! The edge menu's strip and its customized list. The strip is the hover
//! area on the screen edge; turned off there is none, and the menu opens
//! only from `halcyon --toggle drawer` and Settings. A customized menu is a
//! list of items, each running an action the desktop already has.
use super::*;
use crate::config::edge_menu::{Action, EdgeMenu, Item, Kind, Side};
use crate::desktop::apps::AppCatalog;
use cosmic::iced::mouse;
use drawer::DrawerItem;

/// What the surface `ids[3]` is now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EdgeKind {
    /// The hover strip along the whole edge. It measures the usable height
    /// of the display as well.
    Strip(Side),
    /// Only measures the usable height: no input, nothing drawn. Used while
    /// the strip is off, or covers only part of the edge on its own surface.
    Probe,
}

/// A strip covering part of the edge, on its own surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PartStrip {
    pub id: window::Id,
    pub side: Side,
    /// From the top and bottom of the usable height.
    pub margins: [i32; 2],
}

#[derive(Default)]
pub(super) struct EdgeState {
    /// What `ids[3]` was created as; `None` before it exists.
    pub kind: Option<EdgeKind>,
    pub part: Option<PartStrip>,
    /// Whether each item of the customized list can run now: its app is
    /// installed, its folder exists, its type is known.
    pub available: Vec<bool>,
}

const SEPARATOR: f32 = 13.;
const TITLE_GAP: f32 = 10.;
const FOOTER_GAP: f32 = 8.;
const FOOTER: f32 = 30.;
const EMPTY: f32 = 150.;

/// The customized list as the menu draws it: titles, separators and runs
/// of rows that share one rounded group.
#[derive(Debug, PartialEq)]
pub(super) enum Block {
    Title(usize),
    Separator,
    Rows(Vec<usize>),
}

pub(super) fn blocks(items: &[Item]) -> Vec<Block> {
    let mut blocks = Vec::new();
    for (index, item) in items.iter().enumerate() {
        match &item.kind {
            Kind::Title { .. } => blocks.push(Block::Title(index)),
            Kind::Separator => blocks.push(Block::Separator),
            _ => match blocks.last_mut() {
                Some(Block::Rows(rows)) => rows.push(index),
                _ => blocks.push(Block::Rows(vec![index])),
            },
        }
    }
    blocks
}

/// The height the menu needs for `items`, chrome included.
pub(super) fn natural_height(items: &[Item]) -> f32 {
    let body = if items.is_empty() {
        EMPTY
    } else {
        blocks(items)
            .iter()
            .enumerate()
            .map(|(index, block)| match block {
                Block::Title(_) => {
                    (if index == 0 { 0. } else { TITLE_GAP })
                        + drawer::SECTION_LABEL
                        + drawer::LABEL_GAP
                }
                Block::Separator => SEPARATOR,
                Block::Rows(rows) => {
                    rows.len() as f32 * drawer::ROW_STEP - (drawer::ROW_STEP - drawer::ROW_HEIGHT)
                }
            })
            .sum()
    };
    drawer::RAIL_PADDING * 2. + drawer::HEADER + drawer::HEADER_GAP + body + FOOTER_GAP + FOOTER
}

fn side_anchor(side: Side) -> Anchor {
    match side {
        Side::Left => Anchor::LEFT,
        Side::Right => Anchor::RIGHT,
    }
}

fn destroy(id: window::Id) -> Task<Message> {
    cosmic::iced::platform_specific::shell::commands::layer_surface::destroy_layer_surface(id)
}

/// What an item is called in the menu and to a screen reader.
pub(crate) fn item_title(item: &Item) -> String {
    let named = |name: &str, fallback: &str| {
        if name.trim().is_empty() {
            fallback.to_string()
        } else {
            name.to_string()
        }
    };
    match &item.kind {
        Kind::Widget { widget } => {
            Widget::parse(widget).map_or_else(|| named(widget, "Widget"), |w| w.title().into())
        }
        Kind::Action { action, .. } => {
            Action::parse(action).map_or_else(|| named(action, "Action"), |a| a.title().into())
        }
        Kind::Look { name, .. } => named(name, "Look"),
        Kind::App { app, name, .. } => named(name, app),
        Kind::Folder { path, name } => named(
            name,
            std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path),
        ),
        Kind::Separator => "Separator".into(),
        Kind::Title { text } => named(text, "Section"),
        Kind::Unknown(_) => "Unknown".into(),
    }
}

/// The item's icon name: symbolic for Halcyon's own, the app's own icon
/// for an app.
pub(crate) fn item_icon(item: &Item) -> (String, bool) {
    let symbolic = |name: &str| (name.to_string(), true);
    match &item.kind {
        Kind::Widget { widget } => {
            symbolic(Widget::parse(widget).map_or("dialog-question-symbolic", Widget::icon))
        }
        Kind::Action { action, .. } => {
            symbolic(Action::parse(action).map_or("dialog-question-symbolic", Action::icon))
        }
        Kind::Look { .. } => symbolic("applications-graphics-symbolic"),
        Kind::App { icon, .. } if !icon.trim().is_empty() => (icon.clone(), false),
        Kind::App { .. } => symbolic("application-x-executable-symbolic"),
        Kind::Folder { .. } => symbolic("folder-symbolic"),
        Kind::Separator => symbolic("view-more-horizontal-symbolic"),
        Kind::Title { .. } => symbolic("format-text-bold-symbolic"),
        Kind::Unknown(_) => symbolic("dialog-question-symbolic"),
    }
}

/// Whether each item can run here: known type, installed app, existing
/// folder. Reads the app list only when the list has apps.
pub(crate) fn availability(items: &[Item], apps: impl FnOnce() -> AppCatalog) -> Vec<bool> {
    let catalog = items
        .iter()
        .any(|item| matches!(item.kind, Kind::App { .. }))
        .then(apps);
    items
        .iter()
        .map(|item| match &item.kind {
            Kind::Widget { .. } => item.as_widget().is_some(),
            Kind::Action { .. } => item.as_action().is_some(),
            Kind::App { app, .. } => catalog.as_ref().is_some_and(|c| c.get(app).is_some()),
            Kind::Folder { path, .. } => std::path::Path::new(path).is_dir(),
            Kind::Look { .. } | Kind::Separator | Kind::Title { .. } => true,
            Kind::Unknown(_) => false,
        })
        .collect()
}

impl Cards {
    fn edge_strip_measures(&self) -> bool {
        let menu = &self.config.edge_menu;
        menu.enabled && menu.length >= 100
    }
    pub(super) fn wanted_edge_kind(&self) -> EdgeKind {
        if self.edge_strip_measures() {
            EdgeKind::Strip(self.config.edge_menu.side)
        } else {
            EdgeKind::Probe
        }
    }
    /// `ids[3]`: the strip that opens the edge menu, or, while there is no
    /// full-length strip, an invisible probe in its place. Either measures
    /// the usable height of the display (see `Message::SurfaceResized`).
    pub(super) fn edge_surface(&mut self, id: window::Id) -> Task<Message> {
        let kind = self.wanted_edge_kind();
        self.edge.kind = Some(kind);
        get_layer_surface(match kind {
            EdgeKind::Strip(side) => SctkLayerSurfaceSettings {
                id,
                output: self.layer_output(),
                layer: Layer::Overlay,
                keyboard_interactivity: KeyboardInteractivity::None,
                anchor: side_anchor(side) | Anchor::TOP | Anchor::BOTTOM,
                size: Some((Some(2), Some(0))),
                exclusive_zone: 0,
                namespace: "halcyon-edge".into(),
                ..Default::default()
            },
            EdgeKind::Probe => SctkLayerSurfaceSettings {
                id,
                output: self.layer_output(),
                layer: Layer::Bottom,
                keyboard_interactivity: KeyboardInteractivity::None,
                anchor: Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM,
                size: Some((Some(layout::PROBE_THICKNESS), Some(0))),
                exclusive_zone: 0,
                input_zone: Some(Vec::new()),
                namespace: "halcyon-height-probe".into(),
                ..Default::default()
            },
        })
    }
    /// Whether `id` is a strip that opens the menu.
    pub(super) fn is_edge_strip(&self, id: window::Id) -> bool {
        (id == self.ids[3] && matches!(self.edge.kind, Some(EdgeKind::Strip(_))))
            || self.edge.part.is_some_and(|part| part.id == id)
    }
    /// Opens, moves or closes the strip that covers part of the edge. It
    /// waits for the display's height, which places it.
    pub(super) fn sync_edge_strip(&mut self) -> Task<Message> {
        let menu = &self.config.edge_menu;
        let wanted = self.output_known
            && self.selected_output.is_some()
            && menu.enabled
            && menu.length < 100;
        let side = menu.side;
        let margins = menu.strip_margins(self.screen_size[1]);
        match (self.edge.part, wanted) {
            (None, false) => Task::none(),
            (Some(part), false) => {
                self.edge.part = None;
                self.sidebar.hover(true, false, Instant::now());
                destroy(part.id)
            }
            (Some(part), true) if part.side == side => {
                if part.margins == margins {
                    return Task::none();
                }
                self.edge.part = Some(PartStrip { margins, ..part });
                set_margin(part.id, margins[0], 0, margins[1], 0)
            }
            (old, true) => {
                let id = window::Id::unique();
                self.edge.part = Some(PartStrip { id, side, margins });
                let create = get_layer_surface(SctkLayerSurfaceSettings {
                    id,
                    output: self.layer_output(),
                    layer: Layer::Overlay,
                    keyboard_interactivity: KeyboardInteractivity::None,
                    anchor: side_anchor(side) | Anchor::TOP | Anchor::BOTTOM,
                    margin: IcedMargin {
                        top: margins[0],
                        bottom: margins[1],
                        ..Default::default()
                    },
                    size: Some((Some(2), Some(0))),
                    exclusive_zone: 0,
                    namespace: "halcyon-edge".into(),
                    ..Default::default()
                });
                match old {
                    Some(old) => destroy(old.id).chain(create),
                    None => create,
                }
            }
        }
    }
    /// The edge menu settings changed: every change shows at once.
    pub(super) fn edge_menu_changed(&mut self, old: &EdgeMenu) -> Task<Message> {
        let menu = self.config.edge_menu.clone();
        if &menu == old {
            return Task::none();
        }
        self.sidebar.dwell = menu.dwell();
        let mut tasks = Vec::new();
        if self
            .edge
            .kind
            .is_some_and(|kind| kind != self.wanted_edge_kind())
            && self.selected_output.is_some()
        {
            self.sidebar.hover(true, false, Instant::now());
            let id = self.ids[3];
            tasks.push(destroy(id).chain(self.edge_surface(id)));
        }
        tasks.push(self.sync_edge_strip());
        if old.items != menu.items {
            self.refresh_edge_available();
            if let Some(focus) = self.sidebar.focus {
                self.sidebar.focus = Some(focus.min(self.drawer_items().len() - 1));
            }
        }
        let placed = |m: &EdgeMenu| (m.side, m.position, m.length, m.enabled, m.items.clone());
        if self.controls_open && placed(old) != placed(&menu) {
            let id = self.ids[2];
            tasks.push(destroy(id).chain(self.controls_surface(id, self.sidebar.margin())));
        }
        Task::batch(tasks)
    }
    pub(super) fn refresh_edge_available(&mut self) {
        self.edge.available = match &self.config.edge_menu.items {
            Some(items) => availability(items, AppCatalog::installed),
            None => Vec::new(),
        };
    }
    pub(super) fn edge_available(&self, index: usize) -> bool {
        let Some(item) = self
            .config
            .edge_menu
            .items
            .as_ref()
            .and_then(|i| i.get(index))
        else {
            return true;
        };
        !matches!(item.kind, Kind::Unknown(_)) && self.edge.available.get(index) != Some(&false)
    }
    /// The menu's height: the built-in menu's, or what the customized list
    /// needs, kept on screen.
    pub(super) fn controls_height(&self) -> u32 {
        let natural = match &self.config.edge_menu.items {
            Some(items) => natural_height(items).ceil() as u32,
            None => drawer::SIDEBAR_HEIGHT,
        };
        let screen = self.screen_size[1];
        if self.output_known && screen.is_finite() && screen > 0. {
            natural.min((screen as u32).saturating_sub(16).max(240))
        } else {
            natural
        }
    }
    /// Where the menu opens: on the strip's side, centred on the strip. In
    /// the middle the compositor centres it; elsewhere it hangs from the top.
    pub(super) fn controls_anchor(&self) -> (Anchor, i32) {
        let menu = &self.config.edge_menu;
        let side = side_anchor(menu.side);
        let height = self.screen_size[1];
        if menu.position == 50 || !height.is_finite() || height <= 0. {
            return (side, 0);
        }
        let [top, bottom] = menu.strip_margins(height);
        let centre = top as f32 + (height - (top + bottom) as f32) / 2.;
        let own = self.controls_height() as f32;
        let y = (centre - own / 2.)
            .clamp(0., (height - own).max(0.))
            .round();
        (side | Anchor::TOP, y as i32)
    }
    /// The menu's margins with its slide `margin` on the strip's side.
    pub(super) fn controls_margin(&self, margin: i32) -> IcedMargin {
        let (_, top) = self.controls_anchor();
        let side = self.config.edge_menu.side;
        IcedMargin {
            top,
            right: if side == Side::Right { margin } else { 0 },
            left: if side == Side::Left { margin } else { 0 },
            bottom: 0,
        }
    }
    pub(super) fn slide_controls(&self) -> Task<Message> {
        let m = self.controls_margin(self.sidebar.margin());
        set_margin(self.ids[2], m.top, m.right, m.bottom, m.left)
    }
    /// The message an item of the customized list sends; `None` for
    /// headings, separators and items that can't run here.
    pub(super) fn edge_item_message(&self, index: usize) -> Option<Message> {
        let item = self.config.edge_menu.items.as_ref()?.get(index)?;
        if !self.edge_available(index) {
            return None;
        }
        use crate::activity_island::bridge::{Command, Event};
        Some(match &item.kind {
            Kind::Widget { .. } => Message::ToggleWidget(item.as_widget()?),
            Kind::Action { .. } => match item.as_action()? {
                Action::ShowWidgets => Message::SetWidgetsVisible(!self.config.widgets_visible),
                Action::Arrange => Message::Arrange,
                Action::Settings => Message::Settings,
                Action::DoNotDisturb => Message::SetDnd(!self.activity.dnd),
                Action::Timer => Message::Activity(Event::Command(match &self.activity.timer {
                    Some(timer) if timer.running() => Command::TimerPause,
                    Some(timer) if timer.paused() => Command::TimerResume,
                    _ => Command::TimerStart {
                        ms: i64::from(item.minutes()) * 60_000,
                    },
                })),
                Action::Focus => Message::FocusCommand("toggle".into()),
                Action::WeatherEffect => Message::SetWeatherEffect(!self.config.rain.enabled),
            },
            Kind::Look { look, name } => Message::ApplyLook(look.clone(), name.clone()),
            Kind::App { app, .. } => Message::LaunchApp(app.clone()),
            Kind::Folder { path, .. } => Message::Open(path.clone()),
            Kind::Separator | Kind::Title { .. } | Kind::Unknown(_) => return None,
        })
    }
    /// Runs item `index`. Opening something elsewhere closes the menu.
    pub(super) fn edge_item(&mut self, index: usize) -> Task<Message> {
        let Some(message) = self.edge_item_message(index) else {
            return Task::none();
        };
        if matches!(
            message,
            Message::Arrange | Message::Settings | Message::LaunchApp(_) | Message::Open(_)
        ) {
            self.sidebar.hide();
        }
        self.update(message)
    }
    /// Keyboard Move up or Move down of item `index` in the customized list.
    pub(super) fn move_edge_item(&mut self, index: usize, up: bool) -> Task<Message> {
        let Some(mut items) = self.config.edge_menu.items.clone() else {
            return Task::none();
        };
        let to = if up {
            index.checked_sub(1)
        } else {
            (index + 1 < items.len()).then_some(index + 1)
        };
        let Some(to) = to else {
            return Task::none();
        };
        items.swap(index, to);
        let task = self.save_edge_items(items);
        // The keyboard stays on the item it moved.
        if let Some(at) = self
            .drawer_items()
            .iter()
            .position(|item| *item == DrawerItem::Entry(to))
        {
            self.sidebar.focus = Some(at);
        }
        task
    }
    pub(super) fn save_edge_items(&mut self, items: Vec<Item>) -> Task<Message> {
        let saved = crate::desktop::actions::in_action("Edge menu", || {
            Config::save_edge_menu(serde_json::json!({ "items": items }))
        });
        match saved {
            Ok(config) => self.apply_config(config),
            Err(error) => {
                eprintln!("Could not save the edge menu: {error}");
                self.raise_notice(
                    "Couldn’t save the edge menu",
                    "The menu is as it was.",
                    error,
                    None,
                );
                Task::none()
            }
        }
    }
    pub(super) fn set_widgets_visible(&mut self, visible: bool) -> Task<Message> {
        let saved = crate::desktop::actions::in_action("Show widgets", || {
            Config::save_changes(serde_json::json!({ "widgets_visible": visible }))
        });
        match saved {
            Ok(config) => self.apply_config(config),
            Err(error) => {
                self.raise_notice(
                    "Couldn’t show or hide the widgets",
                    "Your change wasn’t kept.",
                    error,
                    Some(Message::SetWidgetsVisible(visible)),
                );
                Task::none()
            }
        }
    }
    pub(super) fn set_weather_effect(&mut self, on: bool) -> Task<Message> {
        let old = self.config.rain.clone();
        let mut next = old.clone();
        next.enabled = on;
        let saved =
            crate::desktop::actions::in_action("Weather effect", || Config::save_rain(&old, &next));
        match saved {
            Ok(config) => self.apply_config(config),
            Err(error) => {
                self.raise_notice(
                    "Couldn’t change the weather effect",
                    "Your change wasn’t kept.",
                    error,
                    Some(Message::SetWeatherEffect(on)),
                );
                Task::none()
            }
        }
    }
    /// The same switch as the island's and COSMIC's notification applet.
    pub(super) fn set_dnd(&mut self, on: bool) -> Task<Message> {
        self.activity.dnd = on;
        cosmic::task::future(async move {
            Message::ToolDone(crate::activity_island::popups::set_dnd(on).await)
        })
    }
    /// The Look through the profile engine, off the UI thread, with Undo.
    pub(super) fn apply_look(
        &mut self,
        look: crate::automation::ProfileRef,
        name: String,
    ) -> Task<Message> {
        cosmic::task::future(async move {
            let result = tokio::task::spawn_blocking(move || {
                let store = crate::desktop::DesktopStore::from_env().map_err(|e| e.to_string())?;
                let installed = crate::desktop::profiles::Installed::discover(&store);
                let held = crate::automation::read_hold(&store);
                match crate::automation::run::run_chosen(&store, &installed, &look, &name, &held) {
                    crate::automation::run::Ran::Applied { .. }
                    | crate::automation::run::Ran::AlreadySet => Ok(()),
                    crate::automation::run::Ran::Waiting(parts) => Err(format!(
                        "Settings has changes to {} that aren’t applied yet.",
                        parts
                            .iter()
                            .map(|part| part.title())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                    crate::automation::run::Ran::Failed(reason) => Err(reason),
                    crate::automation::run::Ran::KeptManual(_) => Ok(()),
                }
            })
            .await
            .unwrap_or_else(|_| Err("The switch stopped unexpectedly.".into()));
            Message::LookApplied(result)
        })
    }
    /// The menu's own page in Settings.
    pub(super) fn customize_edge_menu(&mut self) -> Task<Message> {
        self.sidebar.hide();
        self.launch(
            crate::paths::CONTROLLER,
            vec!["settings".into(), "--page".into(), "edge-menu".into()],
        )
    }

    /// "Customize this menu", under every menu.
    pub(super) fn customize_link(&self) -> Element<'_, Message> {
        let p = self.config.surface_palette();
        let focused = self.drawer_focus() == Some(DrawerItem::Customize);
        cosmic::widget::button::custom(
            row::with_children(vec![
                cosmic::widget::icon::from_name("document-edit-symbolic")
                    .size(14)
                    .icon()
                    .into(),
                label("Customize this menu", 12., p.muted),
            ])
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .name("Customize this menu")
        .id(cosmic::widget::Id::new("edge-customize"))
        .padding([0, 10])
        .height(Length::Fixed(FOOTER))
        .width(Length::Fill)
        .class(row_style(p, p.accent, focused, [12.; 4], false))
        .on_press(Message::EdgeCustomize)
        .into()
    }

    /// The customized menu.
    pub(super) fn custom_rail(&self, items: &[Item]) -> Element<'_, Message> {
        let p = self.config.surface_palette();
        let focus = self.drawer_focus();
        let header = container(
            column::with_children(vec![
                label("Edge menu", 15., p.foreground),
                label("Your shortcuts and switches", 12., p.muted),
            ])
            .spacing(2),
        )
        .padding([0, 4])
        .height(Length::Fixed(drawer::HEADER));
        let mut body: Vec<Element<'_, Message>> = Vec::new();
        if items.is_empty() {
            body.push(empty_state(p));
        }
        for (position, block) in blocks(items).into_iter().enumerate() {
            match block {
                Block::Title(index) => {
                    if position > 0 {
                        body.push(gap(TITLE_GAP));
                    }
                    body.push(heading(item_title(&items[index]), p));
                    body.push(gap(drawer::LABEL_GAP));
                }
                Block::Separator => body.push(
                    container(
                        container(cosmic::iced::widget::space())
                            .width(Length::Fill)
                            .height(1.)
                            .class(cosmic::theme::Container::custom(move |_| {
                                cosmic::iced::widget::container::Style {
                                    background: Some(Background::Color(mix(
                                        p.foreground,
                                        p.background,
                                        0.18,
                                    ))),
                                    ..Default::default()
                                }
                            })),
                    )
                    .padding([6, 8])
                    .height(SEPARATOR)
                    .into(),
                ),
                Block::Rows(rows) => {
                    let count = rows.len();
                    body.push(
                        column::with_children(
                            rows.into_iter()
                                .enumerate()
                                .map(|(at, index)| {
                                    let top = if at == 0 { 12. } else { 0. };
                                    let bottom = if at + 1 == count { 12. } else { 0. };
                                    self.custom_row(
                                        index,
                                        &items[index],
                                        focus == Some(DrawerItem::Entry(index)),
                                        [top, top, bottom, bottom],
                                    )
                                })
                                .collect::<Vec<_>>(),
                        )
                        .spacing(drawer::ROW_STEP - drawer::ROW_HEIGHT)
                        .into(),
                    );
                }
            }
        }
        let body = scrollable(column::with_children(body))
            .id(cosmic::widget::Id::new("edge-menu-items"))
            .height(Length::Fill);
        self.rail_frame(
            column::with_children(vec![
                header.into(),
                gap(drawer::HEADER_GAP),
                body.into(),
                gap(FOOTER_GAP),
                self.customize_link(),
            ])
            .into(),
        )
    }
    /// One row of the customized menu: icon, name and its state.
    fn custom_row(
        &self,
        index: usize,
        item: &Item,
        focused: bool,
        corners: [f32; 4],
    ) -> Element<'_, Message> {
        let p = self.config.surface_palette();
        let available = self.edge_available(index);
        let title = item_title(item);
        let state = available.then(|| self.item_state(item)).flatten();
        let caption = if available {
            self.item_caption(item)
        } else {
            Some("Unavailable".to_string())
        };
        let ink = if available { p.foreground } else { p.muted };
        let (icon_name, symbolic) = item_icon(item);
        let mut icon = cosmic::widget::icon::from_name(icon_name.as_str()).size(16);
        if symbolic {
            icon = icon.symbolic(true);
        }
        let icon: Element<'_, Message> = if symbolic {
            icon.icon()
                .class(cosmic::theme::Svg::custom(move |_| {
                    cosmic::iced::widget::svg::Style { color: Some(ink) }
                }))
                .into()
        } else {
            icon.icon().into()
        };
        let mut parts = vec![
            container(icon).width(16.).center_x(16.).into(),
            // One line, so every row keeps the same height.
            container(
                text::body(title.clone())
                    .size(13.)
                    .line_height(1.25)
                    .font(crate::widget_style::font())
                    .wrapping(cosmic::iced::advanced::text::Wrapping::None)
                    .class(cosmic::theme::Text::Color(crate::widget_style::semantic(
                        ink,
                    ))),
            )
            .width(Length::Fill)
            .clip(true)
            .into(),
        ];
        if let Some(caption) = &caption {
            parts.push(label(caption.clone(), 11., p.muted));
        }
        if let Some(on) = state {
            parts.push(super::panel::switch(on, p.accent, p));
        }
        let spoken = match (state, &caption) {
            (Some(on), _) => format!("{title}, {}", if on { "on" } else { "off" }),
            (None, Some(caption)) => format!("{title}, {caption}"),
            (None, None) => title.clone(),
        };
        let mut button = cosmic::widget::button::custom(
            row::with_children(parts)
                .spacing(8)
                .height(Length::Fill)
                .align_y(Alignment::Center),
        )
        .name(spoken)
        .id(cosmic::widget::Id::new(format!("edge-item-{index}")))
        .padding([0, 10])
        .width(Length::Fill)
        .height(Length::Fixed(drawer::ROW_HEIGHT))
        .class(row_style(p, p.accent, focused, corners, true));
        if available {
            button = button.on_press(Message::EdgeItem(index));
        }
        mouse_area(button)
            .interaction(if available {
                mouse::Interaction::Pointer
            } else {
                mouse::Interaction::NotAllowed
            })
            .into()
    }
    /// On or off, for an item whose state is known.
    fn item_state(&self, item: &Item) -> Option<bool> {
        if let Some(widget) = item.as_widget() {
            return Some(if widget.is_tool() {
                self.config.widget_visible(widget)
            } else {
                self.config.data_visible(widget)
            });
        }
        match item.as_action()? {
            Action::ShowWidgets => Some(self.config.widgets_visible),
            Action::WeatherEffect => Some(self.config.rain.enabled),
            Action::DoNotDisturb => Some(self.activity.dnd),
            _ => None,
        }
    }
    /// A short note after the name: the timer's state or length, Focus's.
    fn item_caption(&self, item: &Item) -> Option<String> {
        match item.as_action()? {
            Action::Timer => Some(match &self.activity.timer {
                Some(timer) if timer.running() => "Running".into(),
                Some(timer) if timer.paused() => "Paused".into(),
                _ => format!("{} min", item.minutes()),
            }),
            Action::Focus => self.tools.focus.running().then(|| "Running".into()),
            _ => None,
        }
    }
    /// The menu's panel: rounded on the side facing the screen.
    pub(super) fn rail_frame<'a>(&self, content: Element<'a, Message>) -> Element<'a, Message> {
        let p = self.config.surface_palette();
        let radius = match self.config.edge_menu.side {
            Side::Right => [20., 0., 0., 20.],
            Side::Left => [0., 20., 20., 0.],
        };
        mouse_area(
            container(content)
                .padding(drawer::RAIL_PADDING)
                .width(Length::Fill)
                .height(Length::Fill)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        // A shade apart from the cards, so the open rail reads
                        // as its own layer where it covers them.
                        background: Some(Background::Color(Color {
                            a: 0.99,
                            ..mix(p.foreground, p.background, 0.05)
                        })),
                        icon_color: Some(p.muted),
                        border: Border {
                            color: mix(p.foreground, p.background, 0.22),
                            width: 1.,
                            radius: radius.into(),
                        },
                        ..Default::default()
                    }
                })),
        )
        .on_enter(Message::Hover(false, true))
        .on_exit(Message::Hover(false, false))
        .into()
    }
}

fn gap<'a>(height: f32) -> Element<'a, Message> {
    cosmic::iced::widget::space().height(height).into()
}

fn heading<'a>(title: String, p: crate::config::Palette) -> Element<'a, Message> {
    container(
        text::body(title)
            .size(12.)
            .font(cosmic::font::semibold())
            .class(cosmic::theme::Text::Color(mix(p.foreground, p.muted, 0.5))),
    )
    .padding([0, 4])
    .height(Length::Fixed(drawer::SECTION_LABEL))
    .align_y(Alignment::End)
    .into()
}

fn empty_state<'a>(p: crate::config::Palette) -> Element<'a, Message> {
    container(
        column::with_children(vec![
            cosmic::widget::icon::from_name("view-list-symbolic")
                .size(32)
                .icon()
                .class(cosmic::theme::Svg::custom(move |_| {
                    cosmic::iced::widget::svg::Style {
                        color: Some(p.muted),
                    }
                }))
                .into(),
            label("Nothing here yet", 14., p.foreground),
            text::body("Add apps, folders, switches and actions from Settings.")
                .size(12.)
                .align_x(cosmic::iced::alignment::Horizontal::Center)
                .class(cosmic::theme::Text::Color(p.muted))
                .into(),
        ])
        .spacing(8)
        .align_x(Alignment::Center),
    )
    .padding([0, 8])
    .width(Length::Fill)
    .height(EMPTY)
    .center_y(EMPTY)
    .class(cosmic::theme::Container::custom(move |_| {
        cosmic::iced::widget::container::Style {
            border: Border {
                color: mix(p.foreground, p.background, 0.18),
                width: 1.,
                radius: 12.into(),
            },
            ..Default::default()
        }
    }))
    .into()
}

/// A menu row's look: a tinted rounded group, an accent ring when the
/// keyboard is on it, a little lighter under the pointer.
fn row_style(
    p: crate::config::Palette,
    accent: Color,
    focused: bool,
    corners: [f32; 4],
    filled: bool,
) -> cosmic::theme::Button {
    let style = move |hovered: bool| {
        let base = if filled { 0.07 } else { 0. };
        cosmic::widget::button::Style {
            background: Some(Background::Color(if hovered {
                mix(p.foreground, p.background, base + 0.06)
            } else if filled {
                mix(p.foreground, p.background, base)
            } else {
                Color::TRANSPARENT
            })),
            border_radius: corners.into(),
            border_width: if focused { 2. } else { 0. },
            border_color: accent,
            icon_color: Some(p.foreground),
            text_color: Some(p.foreground),
            ..Default::default()
        }
    };
    cosmic::theme::Button::Custom {
        active: Box::new(move |_, _| style(false)),
        hovered: Box::new(move |_, _| style(true)),
        pressed: Box::new(move |_, _| style(true)),
        disabled: Box::new(move |_| style(false)),
    }
}
