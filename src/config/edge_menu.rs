//! The edge menu: whether its strip is on the screen edge at all, which
//! edge, where along it, how it opens, and what the menu lists.
//!
//! The list is `None` until someone customizes it: the menu then shows the
//! widget switches it always had. A customized list is a plain sequence of
//! items. Items from a newer Halcyon, and fields this version doesn't know,
//! are kept exactly as saved and shown as unavailable, never dropped.
use super::*;
use crate::automation::ProfileRef;
use serde_json::{Map, Value};

/// Hover delay before the menu opens, as it always was.
pub const DEFAULT_DELAY_MS: u32 = 1000;
pub const MAX_DELAY_MS: u32 = 3000;
pub const MIN_LENGTH: u8 = 10;
/// More than fits on any screen; a list this long is a mistake.
pub const MAX_ITEMS: usize = 64;
/// Minutes a new Timer item counts down.
pub const DEFAULT_TIMER_MINUTES: u32 = 5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    #[default]
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenOn {
    /// Resting the pointer on the strip for `delay_ms`.
    #[default]
    Hover,
    /// Clicking the strip; resting on it does nothing.
    Click,
}

/// How the menu draws its items.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    /// Icon and name on each row, as it always was.
    #[default]
    List,
    /// A slim column of icons; names show as tooltips.
    Icons,
    /// Icons in two to four columns, with short names under them.
    Grid,
}
impl Style {
    pub const ALL: [Self; 3] = [Self::List, Self::Icons, Self::Grid];
    pub fn title(self) -> &'static str {
        match self {
            Self::List => "List",
            Self::Icons => "Icons",
            Self::Grid => "Grid",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconSize {
    Small,
    #[default]
    Medium,
    Large,
}
impl IconSize {
    pub const ALL: [Self; 3] = [Self::Small, Self::Medium, Self::Large];
    pub fn title(self) -> &'static str {
        match self {
            Self::Small => "Small",
            Self::Medium => "Medium",
            Self::Large => "Large",
        }
    }
}

/// What the menu's panel looks like.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frame {
    /// A plain panel a shade apart from the widgets, as it always was.
    #[default]
    Plain,
    /// The widgets' own colors, material, border and shadow.
    Widget,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandleColor {
    #[default]
    Accent,
    Neutral,
}

/// When using an item closes the menu.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Close {
    /// Nothing closes it but the pointer leaving, Escape, or something
    /// that takes over the screen (Arrange, Settings).
    Never,
    /// Actions, Looks, apps and folders close it; switches keep it open.
    #[default]
    Actions,
    /// Every item closes it, switches too.
    Always,
}

/// How fast the menu slides in and out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Speed {
    Slow,
    #[default]
    Normal,
    Fast,
}
impl Speed {
    pub const ALL: [Self; 3] = [Self::Slow, Self::Normal, Self::Fast];
    pub fn title(self) -> &'static str {
        match self {
            Self::Slow => "Slow",
            Self::Normal => "Normal",
            Self::Fast => "Fast",
        }
    }
    /// The slide's easing rate: how much of the way is covered per second.
    pub fn rate(self) -> f32 {
        match self {
            Self::Slow => 11.,
            Self::Normal => 22.,
            Self::Fast => 40.,
        }
    }
}

/// Which displays have a strip that opens the menu.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Displays {
    /// The display the widgets are on, as it always was.
    #[default]
    Main,
    All,
    /// The displays in `outputs`.
    Chosen,
}

pub const MIN_OPACITY: u8 = 40;
pub const MAX_ROUNDNESS: u8 = 32;
pub const DEFAULT_ROUNDNESS: u8 = 20;
pub const MIN_COLUMNS: u8 = 2;
pub const MAX_COLUMNS: u8 = 4;
pub const MIN_HANDLE: u8 = 2;
pub const MAX_HANDLE: u8 = 12;
pub const DEFAULT_HANDLE: u8 = 4;
pub const MIN_HANDLE_OPACITY: u8 = 10;
/// The strip's thickness while no handle is drawn: hard to hit by accident.
pub const HIDDEN_STRIP: u32 = 2;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EdgeMenu {
    /// Off, there is no strip on the screen edge; the menu still opens with
    /// `halcyon --toggle drawer` and from Settings.
    pub enabled: bool,
    pub side: Side,
    /// Where the middle of the strip sits along the edge: 0 at the top, 50
    /// in the middle, 100 at the bottom.
    pub position: u8,
    /// How much of the edge the strip covers, in percent.
    pub length: u8,
    pub open: OpenOn,
    pub delay_ms: u32,
    pub style: Style,
    pub icon_size: IconSize,
    /// Columns of the grid.
    pub columns: u8,
    /// Short names under the grid's icons.
    pub labels: bool,
    pub section_titles: bool,
    /// The panel's opacity, in percent.
    pub opacity: u8,
    /// The panel's corner radius on the side facing the screen, in pixels.
    pub roundness: u8,
    pub frame: Frame,
    /// A visible handle on the strip, One UI style; off, the strip is an
    /// invisible hover area.
    pub handle: bool,
    /// The handle's thickness in pixels; the strip is as thick.
    pub handle_size: u8,
    pub handle_opacity: u8,
    pub handle_color: HandleColor,
    pub close: Close,
    pub speed: Speed,
    pub displays: Displays,
    /// The displays chosen with `Displays::Chosen`, by their saved names.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<String>,
    /// What the menu lists, top down. `None` is the built-in menu.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<Item>>,
    /// Settings from a newer Halcyon, kept as they were.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}
impl Default for EdgeMenu {
    fn default() -> Self {
        Self {
            enabled: true,
            side: Side::Right,
            position: 50,
            length: 100,
            open: OpenOn::Hover,
            delay_ms: DEFAULT_DELAY_MS,
            style: Style::List,
            icon_size: IconSize::Medium,
            columns: MIN_COLUMNS,
            labels: true,
            section_titles: true,
            opacity: 100,
            roundness: DEFAULT_ROUNDNESS,
            frame: Frame::Plain,
            handle: false,
            handle_size: DEFAULT_HANDLE,
            handle_opacity: 60,
            handle_color: HandleColor::Accent,
            close: Close::Actions,
            speed: Speed::Normal,
            displays: Displays::Main,
            outputs: Vec::new(),
            items: None,
            other: Map::new(),
        }
    }
}
impl EdgeMenu {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.position > 100 {
            return Err("The edge menu position must be between 0 and 100 percent".into());
        }
        if !(MIN_LENGTH..=100).contains(&self.length) {
            return Err(format!(
                "The edge menu strip must cover between {MIN_LENGTH} and 100 percent of the edge"
            ));
        }
        if self.delay_ms > MAX_DELAY_MS {
            return Err("The edge menu delay must be at most three seconds".into());
        }
        if !(MIN_COLUMNS..=MAX_COLUMNS).contains(&self.columns) {
            return Err(format!(
                "The edge menu grid has {MIN_COLUMNS} to {MAX_COLUMNS} columns"
            ));
        }
        if !(MIN_OPACITY..=100).contains(&self.opacity) {
            return Err(format!(
                "The edge menu opacity must be between {MIN_OPACITY} and 100 percent"
            ));
        }
        if self.roundness > MAX_ROUNDNESS {
            return Err(format!(
                "The edge menu corners are at most {MAX_ROUNDNESS} pixels round"
            ));
        }
        if !(MIN_HANDLE..=MAX_HANDLE).contains(&self.handle_size) {
            return Err(format!(
                "The edge handle must be {MIN_HANDLE} to {MAX_HANDLE} pixels thick"
            ));
        }
        if !(MIN_HANDLE_OPACITY..=100).contains(&self.handle_opacity) {
            return Err(format!(
                "The edge handle opacity must be between {MIN_HANDLE_OPACITY} and 100 percent"
            ));
        }
        if self
            .items
            .as_ref()
            .is_some_and(|items| items.len() > MAX_ITEMS)
        {
            return Err(format!("The edge menu holds at most {MAX_ITEMS} items"));
        }
        Ok(())
    }
    /// How long the pointer rests on the strip before the menu opens;
    /// `None` when only a click opens it.
    pub fn dwell(&self) -> Option<std::time::Duration> {
        (self.open == OpenOn::Hover)
            .then(|| std::time::Duration::from_millis(self.delay_ms.min(MAX_DELAY_MS).into()))
    }
    pub fn customized(&self) -> bool {
        self.items.is_some()
    }
    /// Whether the menu is the built-in one, drawn as it always was: no
    /// list of its own and nothing about its rows changed.
    pub fn classic(&self) -> bool {
        self.items.is_none()
            && self.style == Style::List
            && self.icon_size == IconSize::Medium
            && self.section_titles
    }
    /// The strip's thickness: the handle's, or a hard to hit 2 pixels.
    pub fn strip_thickness(&self) -> u32 {
        if self.handle {
            u32::from(self.handle_size.clamp(MIN_HANDLE, MAX_HANDLE))
        } else {
            HIDDEN_STRIP
        }
    }
    /// The grid's columns, kept in range.
    pub fn grid_columns(&self) -> usize {
        self.columns.clamp(MIN_COLUMNS, MAX_COLUMNS).into()
    }
    /// The icon size of a row in the List style.
    pub fn list_icon(&self) -> u16 {
        match self.icon_size {
            IconSize::Small => 14,
            IconSize::Medium => 16,
            IconSize::Large => 20,
        }
    }
    /// The icon size in the Icons and Grid styles.
    pub fn compact_icon(&self) -> u16 {
        match self.icon_size {
            IconSize::Small => 16,
            IconSize::Medium => 20,
            IconSize::Large => 24,
        }
    }
    /// The panel's opacity, 0 to 1.
    pub fn alpha(&self) -> f32 {
        f32::from(self.opacity.clamp(MIN_OPACITY, 100)) / 100.
    }
    pub fn radius(&self) -> f32 {
        f32::from(self.roundness.min(MAX_ROUNDNESS))
    }
    /// Whether the strip belongs on a display: `main` is the display the
    /// widgets are on, `key` the display's saved name.
    pub fn on_display(&self, main: bool, key: Option<&str>) -> bool {
        match self.displays {
            Displays::Main => main,
            Displays::All => true,
            Displays::Chosen => key.is_some_and(|key| self.outputs.iter().any(|o| o == key)),
        }
    }
    /// The strip's margins from the top and bottom of an edge `height`
    /// pixels long, so that it covers `length` percent with its middle at
    /// `position`, kept on screen.
    pub fn strip_margins(&self, height: f32) -> [i32; 2] {
        if !height.is_finite() || height <= 0. {
            return [0, 0];
        }
        let length = f32::from(self.length.clamp(MIN_LENGTH, 100)) / 100.;
        let span = (height * length).round();
        let free = (height - span).max(0.);
        let centre = height * f32::from(self.position.min(100)) / 100.;
        let top = (centre - span / 2.).clamp(0., free).round();
        [top as i32, (free - top).max(0.).round() as i32]
    }
}

/// Halcyon actions an item can run. Each is an action the desktop already
/// offers elsewhere: Settings, Arrange, the island's Do Not Disturb switch
/// and timer, the Focus timer, the weather effect switch, Show widgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Show widgets on or off (Widgets › Show widgets).
    ShowWidgets,
    Arrange,
    Settings,
    DoNotDisturb,
    /// Starts the island's timer, then pauses and resumes it.
    Timer,
    /// Starts or pauses the Focus timer.
    Focus,
    /// The wallpaper effect on or off (Effects).
    WeatherEffect,
}
impl Action {
    pub const ALL: [Self; 7] = [
        Self::ShowWidgets,
        Self::Arrange,
        Self::Settings,
        Self::DoNotDisturb,
        Self::Timer,
        Self::Focus,
        Self::WeatherEffect,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::ShowWidgets => "show_widgets",
            Self::Arrange => "arrange",
            Self::Settings => "settings",
            Self::DoNotDisturb => "do_not_disturb",
            Self::Timer => "timer",
            Self::Focus => "focus",
            Self::WeatherEffect => "weather_effect",
        }
    }
    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.key() == key)
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::ShowWidgets => "Show widgets",
            Self::Arrange => "Arrange",
            Self::Settings => "Settings",
            Self::DoNotDisturb => "Do not disturb",
            Self::Timer => "Timer",
            Self::Focus => "Focus timer",
            Self::WeatherEffect => "Weather effect",
        }
    }
    /// What it does, for the Add picker.
    pub fn description(self) -> &'static str {
        match self {
            Self::ShowWidgets => "Show or hide all widgets at once",
            Self::Arrange => "Move and resize widgets",
            Self::Settings => "Open Halcyon Settings",
            Self::DoNotDisturb => "Silence notifications",
            Self::Timer => "Start, pause or resume a countdown",
            Self::Focus => "Start or pause a focus session",
            Self::WeatherEffect => "Turn the wallpaper effect on or off",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Self::ShowWidgets => "preferences-desktop-wallpaper-symbolic",
            Self::Arrange => "view-grid-symbolic",
            Self::Settings => "preferences-system-symbolic",
            Self::DoNotDisturb => "notification-disabled-symbolic",
            Self::Timer => "alarm-symbolic",
            Self::Focus => "task-due-symbolic",
            Self::WeatherEffect => "weather-showers-symbolic",
        }
    }
    /// Shown with an on or off switch: the state it changes is known.
    pub fn switch(self) -> bool {
        matches!(
            self,
            Self::ShowWidgets | Self::DoNotDisturb | Self::WeatherEffect
        )
    }
}

/// One entry of a customized menu, in the form it is saved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    /// A widget's show or hide switch.
    Widget {
        widget: String,
    },
    /// A Halcyon action (`Action::key`); a key this version doesn't know is
    /// kept and shown as unavailable.
    Action {
        action: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minutes: Option<u32>,
    },
    /// Switch to a Look.
    Look {
        look: ProfileRef,
        #[serde(default)]
        name: String,
    },
    /// Open an installed application by its desktop entry ID. The name and
    /// icon are what it had when added, so a removed app still has a name.
    App {
        app: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        icon: String,
    },
    /// Open a folder in the file manager.
    Folder {
        path: String,
        #[serde(default)]
        name: String,
    },
    Separator,
    /// A small heading over the items after it.
    Title {
        #[serde(default)]
        text: String,
    },
    /// From a newer Halcyon, or unreadable: the whole entry as saved.
    #[serde(skip)]
    Unknown(Value),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub kind: Kind,
    /// Fields of a known entry that this version doesn't know.
    extra: Map<String, Value>,
}
impl Item {
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
            extra: Map::new(),
        }
    }
    pub fn widget(widget: Widget) -> Self {
        Self::new(Kind::Widget {
            widget: widget.key().into(),
        })
    }
    pub fn action(action: Action) -> Self {
        Self::new(Kind::Action {
            action: action.key().into(),
            minutes: (action == Action::Timer).then_some(DEFAULT_TIMER_MINUTES),
        })
    }
    pub fn title(text: &str) -> Self {
        Self::new(Kind::Title { text: text.into() })
    }
    /// A row the keyboard stops on: everything but separators and titles.
    pub fn focusable(&self) -> bool {
        !matches!(self.kind, Kind::Separator | Kind::Title { .. })
    }
    pub fn as_widget(&self) -> Option<Widget> {
        match &self.kind {
            Kind::Widget { widget } => Widget::parse(widget),
            _ => None,
        }
    }
    pub fn as_action(&self) -> Option<Action> {
        match &self.kind {
            Kind::Action { action, .. } => Action::parse(action),
            _ => None,
        }
    }
    /// The timer's length, for a Timer item.
    pub fn minutes(&self) -> u32 {
        match &self.kind {
            Kind::Action {
                minutes: Some(minutes),
                ..
            } => (*minutes).clamp(1, 1440),
            _ => DEFAULT_TIMER_MINUTES,
        }
    }
}
impl Serialize for Item {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Kind::Unknown(value) = &self.kind {
            return value.serialize(serializer);
        }
        let mut value = serde_json::to_value(&self.kind).map_err(serde::ser::Error::custom)?;
        if let Some(object) = value.as_object_mut() {
            for (key, extra) in &self.extra {
                object.entry(key.clone()).or_insert_with(|| extra.clone());
            }
        }
        value.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Item {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let Ok(kind) = serde_json::from_value::<Kind>(value.clone()) else {
            return Ok(Self::new(Kind::Unknown(value)));
        };
        // What this version writes back for the entry; anything else in it
        // came from a newer version and goes back out unchanged.
        let known = serde_json::to_value(&kind).unwrap_or(Value::Null);
        let extra = match (value, known.as_object()) {
            (Value::Object(saved), Some(known)) => saved
                .into_iter()
                .filter(|(key, _)| !known.contains_key(key))
                .collect(),
            _ => Map::new(),
        };
        Ok(Self { kind, extra })
    }
}

/// The edge menu's second group, as it always was.
pub const ELSEWHERE: [Widget; 6] = [
    Widget::Calendar,
    Widget::Clock,
    Widget::Glance,
    Widget::Music,
    Widget::Notes,
    Widget::Focus,
];

/// The built-in menu as a list: where a customized menu starts. `right` is
/// the right column in its order. Every row of the built-in menu is here,
/// in the same order, under the same headings.
pub fn default_items(right: &[Widget]) -> Vec<Item> {
    std::iter::once(Item::title("Right column"))
        .chain(right.iter().map(|widget| Item::widget(*widget)))
        .chain(std::iter::once(Item::title("Other widgets")))
        .chain(ELSEWHERE.map(Item::widget))
        .chain([
            Item::new(Kind::Separator),
            Item::action(Action::Arrange),
            Item::action(Action::Settings),
        ])
        .collect()
}

impl Config {
    /// The edge menu's own settings, merged into what is saved.
    pub fn save_edge_menu(changes: serde_json::Value) -> Result<Self, String> {
        let path = Self::path().ok_or("Configuration directory unavailable")?;
        Self::save_section_at(&path, "edge_menu", changes)
    }
    /// The items the menu lists now: the customized list, or the built-in
    /// menu as a list.
    pub fn edge_items(&self) -> Vec<Item> {
        self.edge_menu
            .items
            .clone()
            .unwrap_or_else(|| default_items(&self.ordered_right_cards()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_are_not_written_and_old_files_read_as_the_built_in_menu() {
        let config = Config::default();
        assert!(config.edge_menu.is_default());
        let saved = serde_json::to_value(&config).unwrap();
        assert!(saved.get("edge_menu").is_none(), "{saved}");
        let old: Config = serde_json::from_value(json!({"theme": "teal"})).unwrap();
        assert!(old.edge_menu.enabled && old.edge_menu.items.is_none());
        assert_eq!(
            old.edge_menu.dwell(),
            Some(std::time::Duration::from_secs(1))
        );
    }

    #[test]
    fn unknown_item_types_and_fields_survive_a_round_trip() {
        let saved = json!({
            "enabled": false,
            "side": "left",
            "items": [
                {"type": "app", "app": "org.example.Player", "name": "Player", "icon": "player", "badge": 3},
                {"type": "hologram", "colour": "teal", "depth": [1, 2]},
                {"type": "action", "action": "teleport"},
                {"type": "widget", "widget": "aquarium"},
                "a bare string",
                {"type": "separator"},
            ],
            "panels": [{"name": "Later"}],
        });
        let menu: EdgeMenu = serde_json::from_value(saved.clone()).unwrap();
        let items = menu.items.as_ref().unwrap();
        assert!(matches!(items[0].kind, Kind::App { .. }));
        assert!(matches!(items[1].kind, Kind::Unknown(_)));
        assert_eq!(items[2].as_action(), None);
        assert_eq!(items[3].as_widget(), None);
        assert!(matches!(items[4].kind, Kind::Unknown(_)));
        assert_eq!(menu.other["panels"], saved["panels"]);
        let written = serde_json::to_value(&menu).unwrap();
        for key in ["items", "panels", "enabled", "side"] {
            assert_eq!(written[key], saved[key], "{key}");
        }
    }

    #[test]
    fn the_default_list_is_the_built_in_menu_in_order() {
        let right = [Widget::Weather, Widget::Devbox, Widget::Projects];
        let items = default_items(&right);
        let widgets: Vec<_> = items.iter().filter_map(Item::as_widget).collect();
        assert_eq!(widgets[..3], right);
        assert_eq!(widgets[3..], ELSEWHERE);
        let actions: Vec<_> = items.iter().filter_map(Item::as_action).collect();
        assert_eq!(actions, [Action::Arrange, Action::Settings]);
        assert_eq!(items.iter().filter(|item| item.focusable()).count(), 11);
    }

    #[test]
    fn strip_margins_cover_the_length_around_the_position() {
        let mut menu = EdgeMenu::default();
        assert_eq!(menu.strip_margins(1000.), [0, 0]);
        menu.length = 40;
        assert_eq!(menu.strip_margins(1000.), [300, 300]);
        menu.position = 0;
        assert_eq!(menu.strip_margins(1000.), [0, 600]);
        menu.position = 100;
        assert_eq!(menu.strip_margins(1000.), [600, 0]);
        menu.position = 25;
        assert_eq!(menu.strip_margins(1000.), [50, 550]);
        assert_eq!(menu.strip_margins(0.), [0, 0]);
    }

    #[test]
    fn style_sizes_handle_and_behavior_round_trip_and_old_files_read_as_today() {
        // A file from before these settings: today's menu in every respect.
        let old: EdgeMenu =
            serde_json::from_value(json!({"enabled": true, "side": "left", "length": 60})).unwrap();
        assert_eq!(old.style, Style::List);
        assert_eq!(old.icon_size, IconSize::Medium);
        assert_eq!(old.columns, 2);
        assert!(old.labels && old.section_titles);
        assert_eq!((old.opacity, old.roundness), (100, DEFAULT_ROUNDNESS));
        assert_eq!(old.frame, Frame::Plain);
        assert!(!old.handle);
        assert_eq!(old.strip_thickness(), HIDDEN_STRIP);
        assert_eq!(old.close, Close::Actions);
        assert_eq!(old.speed.rate(), crate::drawer::SLIDE_RATE);
        assert_eq!(old.displays, Displays::Main);
        assert!(old.classic(), "drawn as the built-in menu");
        assert!(old.validate().is_ok());
        let saved = json!({
            "style": "grid",
            "icon_size": "large",
            "columns": 3,
            "labels": false,
            "section_titles": false,
            "opacity": 80,
            "roundness": 8,
            "frame": "widget",
            "handle": true,
            "handle_size": 6,
            "handle_opacity": 70,
            "handle_color": "neutral",
            "close": "always",
            "speed": "fast",
            "displays": "chosen",
            "outputs": ["DP-2"],
        });
        let menu: EdgeMenu = serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(menu.style, Style::Grid);
        assert_eq!(menu.icon_size, IconSize::Large);
        assert_eq!(menu.grid_columns(), 3);
        assert!(!menu.labels && !menu.section_titles);
        assert_eq!(menu.alpha(), 0.8);
        assert_eq!(menu.frame, Frame::Widget);
        assert_eq!(menu.strip_thickness(), 6);
        assert_eq!(menu.handle_color, HandleColor::Neutral);
        assert_eq!(menu.close, Close::Always);
        assert_eq!(menu.speed, Speed::Fast);
        assert!(menu.on_display(false, Some("DP-2")));
        assert!(!menu.on_display(true, Some("DP-1")));
        assert!(!menu.classic());
        assert!(menu.validate().is_ok());
        let written = serde_json::to_value(&menu).unwrap();
        for (key, value) in saved.as_object().unwrap() {
            assert_eq!(&written[key], value, "{key}");
        }
        assert_eq!(serde_json::from_value::<EdgeMenu>(written).unwrap(), menu);
        // Nothing but a style changed still leaves the list unsaved.
        let icons = EdgeMenu {
            style: Style::Icons,
            ..Default::default()
        };
        assert!(!icons.classic() && !icons.customized());
        assert!(icons.on_display(true, None) && !icons.on_display(false, None));
        let all = EdgeMenu {
            displays: Displays::All,
            ..Default::default()
        };
        assert!(all.on_display(false, None));
    }

    #[test]
    fn out_of_range_settings_are_refused() {
        for bad in [
            json!({"position": 101}),
            json!({"length": 5}),
            json!({"delay_ms": 9000}),
            json!({"columns": 5}),
            json!({"columns": 1}),
            json!({"opacity": 10}),
            json!({"roundness": 40}),
            json!({"handle_size": 1}),
            json!({"handle_size": 20}),
            json!({"handle_opacity": 0}),
        ] {
            let menu: EdgeMenu = serde_json::from_value(bad.clone()).unwrap();
            assert!(menu.validate().is_err(), "{bad}");
        }
        let click: EdgeMenu = serde_json::from_value(json!({"open": "click"})).unwrap();
        assert_eq!(click.dwell(), None);
    }
}
