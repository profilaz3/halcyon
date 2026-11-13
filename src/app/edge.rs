//! The edge menu's strip and its customized list. The strip is the hover
//! area on the screen edge; turned off there is none, and the menu opens
//! only from `halcyon --toggle drawer` and Settings. A customized menu is a
//! list of items, each running an action the desktop already has.
use super::*;
use crate::config::edge_menu::{Action, Close, EdgeMenu, Item, Kind, Side, Style};
use crate::desktop::apps::AppCatalog;
use cosmic::iced::mouse;
use drawer::DrawerItem;

/// What the surface `ids[3]` is now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EdgeKind {
    /// The hover strip along the whole edge, this thick. It measures the
    /// usable height of the display as well.
    Strip(Side, u32),
    /// Only measures the usable height: no input, nothing drawn. Used while
    /// the strip is off, or covers only part of the edge on its own surface.
    Probe,
}

/// A strip covering part of the edge, on its own surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PartStrip {
    pub id: window::Id,
    pub side: Side,
    pub thickness: u32,
    /// From the top and bottom of the usable height.
    pub margins: [i32; 2],
}

/// A strip on another display than the widgets'.
#[derive(Clone, Debug)]
pub(super) struct OtherStrip {
    pub id: window::Id,
    pub output: crate::displays::Output,
    pub side: Side,
    pub thickness: u32,
    pub margins: [i32; 2],
}

#[derive(Default)]
pub(super) struct EdgeState {
    /// What `ids[3]` was created as; `None` before it exists.
    pub kind: Option<EdgeKind>,
    pub part: Option<PartStrip>,
    /// Strips on the other displays the menu is shown on.
    pub others: Vec<OtherStrip>,
    /// The display the menu opens on when it isn't the widgets': the one
    /// whose strip opened it.
    pub menu_output: Option<crate::displays::Output>,
    /// The item under the pointer in the Icons and Grid styles, whose name
    /// shows beside it.
    pub tip: Option<DrawerItem>,
    /// Drawn for Settings' preview: the menu alone, no room for names.
    pub preview: bool,
    /// The size the menu's surface was opened at.
    pub surface: Option<[u32; 2]>,
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
pub(super) fn natural_height(items: &[Item], titles: bool) -> f32 {
    let body = if items.is_empty() {
        EMPTY
    } else {
        blocks(items)
            .iter()
            .enumerate()
            .map(|(index, block)| match block {
                Block::Title(_) if !titles => {
                    if index == 0 {
                        0.
                    } else {
                        TITLE_GAP
                    }
                }
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

pub(super) fn side_anchor(side: Side) -> Anchor {
    match side {
        Side::Left => Anchor::LEFT,
        Side::Right => Anchor::RIGHT,
    }
}

pub(super) fn destroy(id: window::Id) -> Task<Message> {
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

/// Whether an item is an on or off switch: a widget's, or a switch action.
pub(super) fn is_switch(item: &Item) -> bool {
    item.as_widget().is_some() || item.as_action().is_some_and(Action::switch)
}

/// Whether running an item that sends `message` closes the menu. Arrange,
/// Settings and Customize take over the screen and always close it.
pub(super) fn closes_after(close: Close, switch: bool, message: &Message) -> bool {
    if matches!(message, Message::Arrange | Message::Settings) {
        return true;
    }
    match close {
        Close::Never => false,
        Close::Actions => !switch,
        Close::Always => true,
    }
}

/// Room beside the Icons and Grid menus where an item's name shows. The
/// surface is this much wider; the room is clear and takes no input.
pub(super) const TIP_ROOM: u32 = 176;

impl Cards {
    /// The items the menu draws one by one, or `None` for the built-in
    /// menu drawn as it always was.
    pub(super) fn menu_items(&self) -> Option<Vec<Item>> {
        (!self.config.edge_menu.classic()).then(|| self.config.edge_items())
    }
    /// Whether the menu shows a notice or Arrange's recovery in place of
    /// its items.
    pub(super) fn rail_message(&self) -> bool {
        self.notice.is_some()
            || self
                .paused_layout
                .as_ref()
                .is_some_and(|editor| editor.exit_failed)
    }
    /// Whether the menu draws the Icons or Grid style now.
    pub(super) fn compact(&self) -> bool {
        self.config.edge_menu.style != Style::List && !self.rail_message()
    }
    /// The menu panel's own width.
    pub(super) fn rail_width(&self) -> u32 {
        if self.compact() {
            super::edge_compact::width(&self.config.edge_menu).ceil() as u32
        } else {
            drawer::SIDEBAR_WIDTH
        }
    }
    /// The menu surface's width: the panel, and room for names beside the
    /// Icons and Grid menus.
    pub(super) fn controls_width(&self) -> u32 {
        if self.compact() {
            self.rail_width() + TIP_ROOM
        } else {
            drawer::SIDEBAR_WIDTH
        }
    }
    /// Where the panel sits across the surface, from its left.
    pub(super) fn rail_x(&self) -> f32 {
        match self.config.edge_menu.side {
            Side::Right => (self.controls_width() - self.rail_width()) as f32,
            Side::Left => 0.,
        }
    }
    /// The height of the display the menu opens on.
    pub(super) fn edge_screen_height(&self) -> f32 {
        match &self.edge.menu_output {
            Some(output) => output
                .info
                .logical_size
                .map_or(self.screen_size[1], |(_, height)| height as f32),
            None => self.screen_size[1],
        }
    }
    /// The output the menu's surface opens on.
    pub(super) fn menu_layer_output(&self) -> IcedOutput {
        match &self.edge.menu_output {
            Some(output) => IcedOutput::Output(output.handle.clone()),
            None => self.layer_output(),
        }
    }
    /// Whether the widgets' display has a strip: the menu is on and shown
    /// there.
    fn edge_on_main(&self) -> bool {
        let menu = &self.config.edge_menu;
        let key = self
            .selected_output
            .as_ref()
            .and_then(|output| self.displays.key(output));
        menu.enabled && menu.on_display(true, key.as_deref())
    }
    fn edge_strip_measures(&self) -> bool {
        self.edge_on_main() && self.config.edge_menu.length >= 100
    }
    pub(super) fn wanted_edge_kind(&self) -> EdgeKind {
        if self.edge_strip_measures() {
            let menu = &self.config.edge_menu;
            EdgeKind::Strip(menu.side, menu.strip_thickness())
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
            EdgeKind::Strip(side, thickness) => SctkLayerSurfaceSettings {
                id,
                output: self.layer_output(),
                layer: Layer::Overlay,
                keyboard_interactivity: KeyboardInteractivity::None,
                anchor: side_anchor(side) | Anchor::TOP | Anchor::BOTTOM,
                size: Some((Some(thickness), Some(0))),
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
        (id == self.ids[3] && matches!(self.edge.kind, Some(EdgeKind::Strip(..))))
            || self.edge.part.is_some_and(|part| part.id == id)
            || self.edge.others.iter().any(|strip| strip.id == id)
    }
    /// Opens, moves or closes the strip that covers part of the edge. It
    /// waits for the display's height, which places it.
    pub(super) fn sync_edge_strip(&mut self) -> Task<Message> {
        let others = self.sync_other_strips();
        let menu = &self.config.edge_menu;
        let wanted = self.output_known
            && self.selected_output.is_some()
            && self.edge_on_main()
            && menu.length < 100;
        let side = menu.side;
        let thickness = menu.strip_thickness();
        let margins = menu.strip_margins(self.screen_size[1]);
        let part = match (self.edge.part, wanted) {
            (None, false) => Task::none(),
            (Some(part), false) => {
                self.edge.part = None;
                self.sidebar.hover(true, false, Instant::now());
                destroy(part.id)
            }
            (Some(part), true) if part.side == side && part.thickness == thickness => {
                if part.margins == margins {
                    return Task::none();
                }
                self.edge.part = Some(PartStrip { margins, ..part });
                set_margin(part.id, margins[0], 0, margins[1], 0)
            }
            (old, true) => {
                let id = window::Id::unique();
                self.edge.part = Some(PartStrip {
                    id,
                    side,
                    thickness,
                    margins,
                });
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
                    size: Some((Some(thickness), Some(0))),
                    exclusive_zone: 0,
                    namespace: "halcyon-edge".into(),
                    ..Default::default()
                });
                match old {
                    Some(old) => destroy(old.id).chain(create),
                    None => create,
                }
            }
        };
        Task::batch([part, others])
    }
    /// Opens, moves or closes the strips on the other displays the menu is
    /// shown on. Their length is placed on the display's whole height.
    fn sync_other_strips(&mut self) -> Task<Message> {
        let menu = &self.config.edge_menu;
        let main = self.selected_output.as_ref().map(|o| o.handle.clone());
        let wanted: Vec<OtherStrip> = if self.output_known && menu.enabled {
            self.displays
                .outputs()
                .iter()
                .filter(|output| Some(&output.handle) != main.as_ref())
                .filter(|output| menu.on_display(false, self.displays.key(output).as_deref()))
                .map(|output| OtherStrip {
                    id: window::Id::NONE,
                    output: output.clone(),
                    side: menu.side,
                    thickness: menu.strip_thickness(),
                    margins: menu.strip_margins(
                        output
                            .info
                            .logical_size
                            .map_or(0., |(_, height)| height as f32),
                    ),
                })
                .collect()
        } else {
            Vec::new()
        };
        let same = |a: &OtherStrip, b: &OtherStrip| {
            a.output.handle == b.output.handle
                && a.side == b.side
                && a.thickness == b.thickness
                && a.margins == b.margins
        };
        let mut tasks = Vec::new();
        let old = std::mem::take(&mut self.edge.others);
        for strip in &old {
            if !wanted.iter().any(|w| same(w, strip)) {
                tasks.push(destroy(strip.id));
            }
        }
        for mut strip in wanted {
            if let Some(kept) = old.iter().find(|o| same(o, &strip)) {
                self.edge.others.push(kept.clone());
                continue;
            }
            strip.id = window::Id::unique();
            tasks.push(get_layer_surface(SctkLayerSurfaceSettings {
                id: strip.id,
                output: IcedOutput::Output(strip.output.handle.clone()),
                layer: Layer::Overlay,
                keyboard_interactivity: KeyboardInteractivity::None,
                anchor: side_anchor(strip.side) | Anchor::TOP | Anchor::BOTTOM,
                margin: IcedMargin {
                    top: strip.margins[0],
                    bottom: strip.margins[1],
                    ..Default::default()
                },
                size: Some((Some(strip.thickness), Some(0))),
                exclusive_zone: 0,
                namespace: "halcyon-edge".into(),
                ..Default::default()
            }));
            self.edge.others.push(strip);
        }
        if self.edge.menu_output.as_ref().is_some_and(|output| {
            !self
                .edge
                .others
                .iter()
                .any(|s| s.output.handle == output.handle)
        }) {
            self.edge.menu_output = None;
        }
        Task::batch(tasks)
    }
    /// Every strip on the other displays, closed: their displays are
    /// measured again first.
    pub(super) fn forget_other_strips(&mut self) -> Vec<window::Id> {
        self.edge.menu_output = None;
        std::mem::take(&mut self.edge.others)
            .into_iter()
            .map(|strip| strip.id)
            .collect()
    }
    /// The pointer on strip `id`: the menu opens on that strip's display.
    pub(super) fn edge_strip_hover(&mut self, id: window::Id, entered: bool) -> Task<Message> {
        if entered && !self.controls_open {
            self.edge.menu_output = self
                .edge
                .others
                .iter()
                .find(|strip| strip.id == id)
                .map(|strip| strip.output.clone());
        }
        self.update(Message::Hover(true, entered))
    }
    /// The edge menu settings changed: every change shows at once.
    pub(super) fn edge_menu_changed(&mut self, old: &EdgeMenu) -> Task<Message> {
        let menu = self.config.edge_menu.clone();
        if &menu == old {
            return Task::none();
        }
        self.sidebar.dwell = menu.dwell();
        self.sidebar.rate = menu.speed.rate();
        self.sidebar.width = self.controls_width();
        self.edge.tip = None;
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
        if old.items != menu.items || old.classic() != menu.classic() {
            self.refresh_edge_available();
            if let Some(focus) = self.sidebar.focus {
                self.sidebar.focus = Some(focus.min(self.drawer_items().len() - 1));
            }
        }
        // What sizes or places the menu's surface; the rest only redraws.
        let placed = |m: &EdgeMenu| {
            (
                (m.side, m.position, m.length, m.enabled, m.items.clone()),
                (m.style, m.icon_size, m.columns, m.labels, m.section_titles),
            )
        };
        if self.controls_open && placed(old) != placed(&menu) {
            let id = self.ids[2];
            let margin = self.sidebar.margin();
            tasks.push(destroy(id).chain(self.controls_surface(id, margin)));
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
        let menu = &self.config.edge_menu;
        let natural = match self.menu_items() {
            Some(items) if self.compact() => super::edge_compact::Compact::new(menu, &items)
                .height()
                .ceil() as u32,
            Some(items) => {
                let natural = natural_height(&items, menu.section_titles).ceil() as u32;
                if self.rail_message() && menu.style != Style::List {
                    natural.max(drawer::SIDEBAR_HEIGHT)
                } else {
                    natural
                }
            }
            None => drawer::SIDEBAR_HEIGHT,
        };
        let screen = self.edge_screen_height();
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
        let height = self.edge_screen_height();
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
        let items = self.menu_items()?;
        let item = items.get(index)?;
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
    /// Runs item `index`, and closes the menu as the Close setting says.
    pub(super) fn edge_item(&mut self, index: usize) -> Task<Message> {
        let Some(message) = self.edge_item_message(index) else {
            return Task::none();
        };
        let switch = self
            .menu_items()
            .and_then(|items| items.get(index).map(is_switch))
            .unwrap_or(false);
        if closes_after(self.config.edge_menu.close, switch, &message) {
            self.sidebar.hide();
        }
        self.update(message)
    }
    /// Keyboard Move up or Move down of item `index` in the customized list.
    pub(super) fn move_edge_item(&mut self, index: usize, up: bool) -> Task<Message> {
        let Some(mut items) = self.menu_items() else {
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
                    if self.config.edge_menu.section_titles {
                        body.push(heading(item_title(&items[index]), p));
                        body.push(gap(drawer::LABEL_GAP));
                    }
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
        let size = self.config.edge_menu.list_icon();
        let mut icon = cosmic::widget::icon::from_name(icon_name.as_str()).size(size);
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
            container(icon)
                .width(f32::from(size))
                .center_x(f32::from(size))
                .into(),
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
    pub(super) fn item_state(&self, item: &Item) -> Option<bool> {
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
    pub(super) fn item_caption(&self, item: &Item) -> Option<String> {
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
        self.rail_panel(content, drawer::RAIL_PADDING)
    }
    /// The panel with `padding` inside: a plain panel a shade apart from
    /// the widgets, or the widgets' own style, at the menu's opacity and
    /// roundness.
    pub(super) fn rail_panel<'a>(
        &self,
        content: Element<'a, Message>,
        padding: f32,
    ) -> Element<'a, Message> {
        let menu = &self.config.edge_menu;
        let p = self.config.surface_palette();
        let r = menu.radius();
        let radius = match menu.side {
            Side::Right => [r, 0., 0., r],
            Side::Left => [0., r, r, 0.],
        };
        let alpha = menu.alpha();
        let (background, border, width) = match menu.frame {
            // A shade apart from the cards, so the open rail reads as its
            // own layer where it covers them.
            crate::config::edge_menu::Frame::Plain => (
                Background::Color(Color {
                    a: 0.99 * alpha,
                    ..mix(p.foreground, p.background, 0.05)
                }),
                mix(p.foreground, p.background, 0.22),
                1.,
            ),
            crate::config::edge_menu::Frame::Widget => {
                let _style = crate::widget_style::scope(&self.config);
                let card = self.config.palette();
                let appearance = &self.config.appearance;
                (
                    crate::widget_style::background(Color {
                        a: card.background.a * alpha,
                        ..card.background
                    }),
                    crate::widget_style::border(Color {
                        a: f32::from(appearance.border_opacity.min(50)) / 100.,
                        ..card.accent
                    }),
                    crate::widget_style::edge(card).0,
                )
            }
        };
        mouse_area(
            container(content)
                .padding(padding)
                .width(Length::Fill)
                .height(Length::Fill)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        background: Some(background),
                        icon_color: Some(p.muted),
                        border: Border {
                            color: border,
                            width,
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
