//! COSMIC's shortcut file format, as the pinned compositor reads it.
//!
//! Source (pinned): cosmic-settings-daemon e37160f `config/src/shortcuts`
//! (`binding.rs`, `modifier.rs`, `sym.rs`, `action.rs`), the
//! cosmic-settings-config revision cosmic-comp 4d14528 is built with.
//!
//! A shortcut collection is one RON map from `Binding` to `Action`. Two
//! bindings are the same shortcut when their modifiers and keysym match;
//! the description (a custom shortcut's name) and a hardware keycode are
//! carried along but never part of the identity, exactly like upstream's
//! `PartialEq`/`Hash`. Each entry keeps the exact text of its binding and
//! action, so a rewrite reproduces every entry it does not change, including
//! actions this version does not recognize (upstream's reader would turn
//! those into `Disable`).
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use xkbcommon::xkb::{self, Keysym};

/// `x11rb`'s unused keysym value, which xkbcommon returns for unknown names.
const NO_SYMBOL: u32 = 0;

/// Pressed modifiers (upstream `Modifiers`).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
}

impl Modifiers {
    pub fn any(self) -> bool {
        self.ctrl || self.alt || self.shift || self.logo
    }
}

/// Upstream `Modifier`, the serialized form of one pressed modifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Super,
}

/// What makes two bindings the same shortcut: modifiers and keysym.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Ident {
    pub modifiers: Modifiers,
    pub key: Option<u32>,
}

/// A key combination (upstream `Binding`).
#[derive(Clone, Debug, Default)]
pub struct Binding {
    pub modifiers: Modifiers,
    pub key: Option<Keysym>,
    /// A hardware keycode. Kept, but the pinned compositor never matches it.
    pub keycode: Option<u32>,
    /// A custom shortcut's name.
    pub description: Option<String>,
}

impl PartialEq for Binding {
    fn eq(&self, other: &Self) -> bool {
        self.ident() == other.ident()
    }
}
impl Eq for Binding {}

/// Field-for-field equality, for checking that an entry is reproduced.
pub fn same_fields(a: &Binding, b: &Binding) -> bool {
    a.ident() == b.ident() && a.keycode == b.keycode && a.description == b.description
}

impl Binding {
    pub fn new(modifiers: Modifiers, key: Option<Keysym>) -> Self {
        Self {
            modifiers,
            key,
            keycode: None,
            description: None,
        }
    }
    pub fn ident(&self) -> Ident {
        Ident {
            modifiers: self.modifiers,
            key: self.key.map(Keysym::raw),
        }
    }
    /// The same combination without a name or keycode.
    pub fn plain(&self) -> Self {
        Self::new(self.modifiers, self.key)
    }

    /// Upstream `from_str_partial`: `+`-separated tokens, case-insensitive
    /// modifier names, at most one key. A single character is that
    /// character's (lowercase) keysym; longer names are xkbcommon keysym
    /// names, case-sensitive first. The key may be missing.
    pub fn parse_partial(value: &str) -> Result<Self, String> {
        let mut binding = Self::default();
        for token in value.split('+') {
            let token = token.trim();
            match token.to_ascii_lowercase().as_str() {
                "super" => binding.modifiers.logo = true,
                "ctrl" => binding.modifiers.ctrl = true,
                "alt" => binding.modifiers.alt = true,
                "shift" => binding.modifiers.shift = true,
                lowercased => {
                    if binding.key.is_some() {
                        return Err("Use only one key besides the modifiers.".into());
                    }
                    if token.chars().count() == 1 {
                        let character = lowercased.chars().next().unwrap_or_default();
                        binding.key = Some(Keysym::from_char(character));
                        continue;
                    }
                    binding.key = Some(keysym(token).ok_or_else(|| {
                        if token.is_empty() {
                            "Type a key after the last +.".to_string()
                        } else {
                            format!("“{token}” isn’t a key name.")
                        }
                    })?);
                }
            }
        }
        if !binding.modifiers.any() && binding.key.is_none() {
            return Err("Type a key combination, for example Super+Shift+K.".into());
        }
        Ok(binding)
    }
    /// Upstream `FromStr`: a key, or Super by itself.
    pub fn parse(value: &str) -> Result<Self, String> {
        let binding = Self::parse_partial(value)?;
        if binding.key.is_none() && !binding.modifiers.logo {
            return Err("Add a key to the modifiers, for example Ctrl+Alt+T.".into());
        }
        Ok(binding)
    }
    /// A combination COSMIC accepts for a new shortcut (upstream `is_set`):
    /// modifiers with a key, Super by itself, or a supported special/media key.
    pub fn parse_new(value: &str) -> Result<Self, String> {
        let binding = Self::parse(value)?;
        if binding.is_set() {
            Ok(binding)
        } else {
            Err("Add a modifier to that key, for example Ctrl, Alt or Super.".into())
        }
    }
    pub fn is_set(&self) -> bool {
        (self.modifiers.any() && self.key.is_some())
            || self.is_super()
            || self.key.is_some_and(|key| {
                key.is_misc_function_key() || matches!(key.raw(), 0x10080001..=0x1008FFFF)
            })
    }
    pub fn is_super(&self) -> bool {
        self.key.is_none()
            && self.modifiers.logo
            && !self.modifiers.shift
            && !self.modifiers.alt
            && !self.modifiers.ctrl
    }
    /// Keycaps in upstream's order (Super, Ctrl, Alt, Shift, key). Single
    /// letters are shown uppercase, as printed on keys.
    pub fn keycaps(&self) -> Vec<String> {
        let mut caps = Vec::new();
        for (on, name) in [
            (self.modifiers.logo, "Super"),
            (self.modifiers.ctrl, "Ctrl"),
            (self.modifiers.alt, "Alt"),
            (self.modifiers.shift, "Shift"),
        ] {
            if on {
                caps.push(name.to_string());
            }
        }
        if let Some(key) = self.key {
            let name = xkb::keysym_get_name(key);
            caps.push(if name.chars().count() == 1 {
                name.to_uppercase()
            } else {
                name
            });
        }
        caps
    }
}

impl Binding {
    /// The keycaps joined for text, e.g. `Super+Shift+K`.
    pub fn label(&self) -> String {
        self.keycaps().join("+")
    }
}

/// Upstream `to_string`: `Super+Ctrl+Alt+Shift+key` with the keysym name.
impl fmt::Display for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        for (on, name) in [
            (self.modifiers.logo, "Super"),
            (self.modifiers.ctrl, "Ctrl"),
            (self.modifiers.alt, "Alt"),
            (self.modifiers.shift, "Shift"),
        ] {
            if on {
                parts.push(name.into());
            }
        }
        if let Some(key) = self.key {
            parts.push(xkb::keysym_get_name(key));
        }
        write!(f, "{}", parts.join("+"))
    }
}

/// Upstream `sym::deserialize`: a keysym name, case-sensitive first.
fn keysym(name: &str) -> Option<Keysym> {
    match xkb::keysym_from_name(name, xkb::KEYSYM_NO_FLAGS) {
        found if found.raw() != NO_SYMBOL => Some(found),
        _ => Some(xkb::keysym_from_name(name, xkb::KEYSYM_CASE_INSENSITIVE))
            .filter(|found| found.raw() != NO_SYMBOL),
    }
}

/// The serialized binding: `(modifiers: [Super], key: "t")`, with
/// upstream's `deny_unknown_fields`, required modifiers and optional rest.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingDef {
    modifiers: Vec<Modifier>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_key",
        serialize_with = "serialize_key"
    )]
    key: Option<Keysym>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    keycode: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

fn deserialize_key<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Keysym>, D::Error> {
    let name = String::deserialize(deserializer)?;
    keysym(&name).map(Some).ok_or_else(|| {
        de::Error::invalid_value(de::Unexpected::Str(&name), &"an xkbcommon keysym name")
    })
}

fn serialize_key<S: Serializer>(key: &Option<Keysym>, serializer: S) -> Result<S::Ok, S::Error> {
    match key {
        Some(key) => serializer.serialize_str(&xkb::keysym_get_name(*key)),
        None => serializer.serialize_str("None"),
    }
}

impl From<BindingDef> for Binding {
    fn from(def: BindingDef) -> Self {
        let mut modifiers = Modifiers::default();
        for modifier in def.modifiers {
            match modifier {
                Modifier::Ctrl => modifiers.ctrl = true,
                Modifier::Alt => modifiers.alt = true,
                Modifier::Shift => modifiers.shift = true,
                Modifier::Super => modifiers.logo = true,
            }
        }
        Self {
            modifiers,
            key: def.key,
            keycode: def.keycode,
            description: def.description,
        }
    }
}

impl From<&Binding> for BindingDef {
    fn from(binding: &Binding) -> Self {
        let mut modifiers = Vec::new();
        for (on, modifier) in [
            (binding.modifiers.logo, Modifier::Super),
            (binding.modifiers.ctrl, Modifier::Ctrl),
            (binding.modifiers.alt, Modifier::Alt),
            (binding.modifiers.shift, Modifier::Shift),
        ] {
            if on {
                modifiers.push(modifier);
            }
        }
        Self {
            modifiers,
            key: binding.key,
            keycode: binding.keycode,
            description: binding.description.clone(),
        }
    }
}

impl Serialize for Binding {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BindingDef::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Binding {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        BindingDef::deserialize(deserializer).map(Self::from)
    }
}

/// Upstream `Action` at e37160f, variant for variant (deprecated ones too),
/// so a recognized action is decoded exactly and anything else is kept as
/// text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Close,
    Debug,
    Disable,
    Focus(FocusDirection),
    LastWorkspace,
    Maximize,
    Fullscreen,
    MigrateWorkspaceToNextOutput,
    MigrateWorkspaceToOutput(Direction),
    MigrateWorkspaceToPreviousOutput,
    Minimize,
    Move(Direction),
    MoveToLastWorkspace,
    MoveToNextOutput,
    MoveToNextWorkspace,
    MoveToOutput(Direction),
    MoveToPreviousOutput,
    MoveToPreviousWorkspace,
    MoveToWorkspace(u8),
    NextOutput,
    NextWorkspace,
    Orientation(Orientation),
    PreviousOutput,
    PreviousWorkspace,
    Resizing(ResizeDirection),
    SendToLastWorkspace,
    SendToNextOutput,
    SendToNextWorkspace,
    SendToOutput(Direction),
    SendToPreviousOutput,
    SendToPreviousWorkspace,
    SendToWorkspace(u8),
    SwapWindow,
    SwitchOutput(Direction),
    System(System),
    Spawn(String),
    Terminate,
    ToggleOrientation,
    ToggleStacking,
    ToggleSticky,
    ToggleTiling,
    ToggleWindowFloating,
    Workspace(u8),
    ZoomIn,
    ZoomOut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum System {
    AppLibrary,
    BrightnessDown,
    BrightnessUp,
    DisplayToggle,
    HomeFolder,
    InputSourceSwitch,
    KeyboardBrightnessDown,
    KeyboardBrightnessUp,
    Launcher,
    LockScreen,
    LogOut,
    Mute,
    MuteMic,
    PlayPause,
    PlayNext,
    PlayPrev,
    PowerOff,
    ScreenReader,
    Screenshot,
    Suspend,
    Terminal,
    TouchpadToggle,
    VolumeLower,
    VolumeRaise,
    WebBrowser,
    WindowSwitcher,
    WindowSwitcherPrevious,
    WorkspaceOverview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FocusDirection {
    Left,
    Right,
    Up,
    Down,
    In,
    Out,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResizeDirection {
    Inwards,
    Outwards,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

fn direction(direction: Direction) -> &'static str {
    match direction {
        Direction::Left => "left",
        Direction::Right => "right",
        Direction::Up => "up",
        Direction::Down => "down",
    }
}

/// A short readable name for a built-in action.
pub fn action_label(action: &Action) -> String {
    use Action as A;
    match action {
        A::Close => "Close window".into(),
        A::Debug => "Debug overlay".into(),
        A::Disable => "Off".into(),
        A::Focus(to) => match to {
            FocusDirection::In => "Focus inside group".into(),
            FocusDirection::Out => "Focus outside group".into(),
            FocusDirection::Left => "Focus window left".into(),
            FocusDirection::Right => "Focus window right".into(),
            FocusDirection::Up => "Focus window up".into(),
            FocusDirection::Down => "Focus window down".into(),
        },
        A::LastWorkspace => "Go to last workspace".into(),
        A::Maximize => "Maximize window".into(),
        A::Fullscreen => "Full screen".into(),
        A::MigrateWorkspaceToNextOutput => "Move workspace to next display".into(),
        A::MigrateWorkspaceToOutput(to) => {
            format!("Move workspace to display {}", direction(*to))
        }
        A::MigrateWorkspaceToPreviousOutput => "Move workspace to previous display".into(),
        A::Minimize => "Minimize window".into(),
        A::Move(to) => format!("Move window {}", direction(*to)),
        A::MoveToLastWorkspace => "Move window to last workspace".into(),
        A::MoveToNextOutput => "Move window to next display".into(),
        A::MoveToNextWorkspace => "Move window to next workspace".into(),
        A::MoveToOutput(to) => format!("Move window to display {}", direction(*to)),
        A::MoveToPreviousOutput => "Move window to previous display".into(),
        A::MoveToPreviousWorkspace => "Move window to previous workspace".into(),
        A::MoveToWorkspace(n) => format!("Move window to workspace {n}"),
        A::NextOutput => "Focus next display".into(),
        A::NextWorkspace => "Next workspace".into(),
        A::Orientation(Orientation::Horizontal) => "Tile horizontally".into(),
        A::Orientation(Orientation::Vertical) => "Tile vertically".into(),
        A::PreviousOutput => "Focus previous display".into(),
        A::PreviousWorkspace => "Previous workspace".into(),
        A::Resizing(ResizeDirection::Inwards) => "Resize window smaller".into(),
        A::Resizing(ResizeDirection::Outwards) => "Resize window larger".into(),
        A::SendToLastWorkspace => "Send window to last workspace".into(),
        A::SendToNextOutput => "Send window to next display".into(),
        A::SendToNextWorkspace => "Send window to next workspace".into(),
        A::SendToOutput(to) => format!("Send window to display {}", direction(*to)),
        A::SendToPreviousOutput => "Send window to previous display".into(),
        A::SendToPreviousWorkspace => "Send window to previous workspace".into(),
        A::SendToWorkspace(n) => format!("Send window to workspace {n}"),
        A::SwapWindow => "Swap windows".into(),
        A::SwitchOutput(to) => format!("Focus display {}", direction(*to)),
        A::System(system) => system_label(*system).into(),
        A::Spawn(_) => "Run a command".into(),
        A::Terminate => "Quit COSMIC session".into(),
        A::ToggleOrientation => "Toggle tiling orientation".into(),
        A::ToggleStacking => "Toggle window stacking".into(),
        A::ToggleSticky => "Toggle sticky window".into(),
        A::ToggleTiling => "Toggle tiling".into(),
        A::ToggleWindowFloating => "Toggle floating window".into(),
        A::Workspace(n) => format!("Go to workspace {n}"),
        A::ZoomIn => "Zoom in".into(),
        A::ZoomOut => "Zoom out".into(),
    }
}

fn system_label(system: System) -> &'static str {
    use System as S;
    match system {
        S::AppLibrary => "Open app library",
        S::BrightnessDown => "Screen brightness down",
        S::BrightnessUp => "Screen brightness up",
        S::DisplayToggle => "Switch display mode",
        S::HomeFolder => "Open home folder",
        S::InputSourceSwitch => "Switch input source",
        S::KeyboardBrightnessDown => "Keyboard brightness down",
        S::KeyboardBrightnessUp => "Keyboard brightness up",
        S::Launcher => "Open launcher",
        S::LockScreen => "Lock screen",
        S::LogOut => "Log out",
        S::Mute => "Mute sound",
        S::MuteMic => "Mute microphone",
        S::PlayPause => "Play or pause",
        S::PlayNext => "Next track",
        S::PlayPrev => "Previous track",
        S::PowerOff => "Power off",
        S::ScreenReader => "Screen reader",
        S::Screenshot => "Take a screenshot",
        S::Suspend => "Suspend",
        S::Terminal => "Open terminal",
        S::TouchpadToggle => "Touchpad on or off",
        S::VolumeLower => "Volume down",
        S::VolumeRaise => "Volume up",
        S::WebBrowser => "Open web browser",
        S::WindowSwitcher => "Switch windows",
        S::WindowSwitcherPrevious => "Switch windows backwards",
        S::WorkspaceOverview => "Workspace overview",
    }
}

/// One `binding: action` entry with its exact source text.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub binding: Binding,
    pub binding_raw: String,
    /// `None` for an action this version doesn't recognize.
    pub action: Option<Action>,
    pub action_raw: String,
}

impl Entry {
    /// A new entry, serialized compactly like a RON value COSMIC reads.
    pub fn new(binding: Binding, action: Action) -> Result<Self, String> {
        let binding_raw = ron::to_string(&binding).map_err(|error| error.to_string())?;
        let action_raw = ron::to_string(&action).map_err(|error| error.to_string())?;
        Ok(Self {
            binding,
            binding_raw,
            action: Some(action),
            action_raw,
        })
    }
    /// The same action, its text kept exactly, on other keys.
    pub fn with_binding(&self, binding: Binding) -> Result<Self, String> {
        Ok(Self {
            binding_raw: ron::to_string(&binding).map_err(|error| error.to_string())?,
            binding,
            ..self.clone()
        })
    }
    /// The same binding text with another action.
    pub fn with_action(&self, action: Action) -> Result<Self, String> {
        Ok(Self {
            action_raw: ron::to_string(&action).map_err(|error| error.to_string())?,
            action: Some(action),
            ..self.clone()
        })
    }
    pub fn ident(&self) -> Ident {
        self.binding.ident()
    }
    pub fn is_disable(&self) -> bool {
        self.action == Some(Action::Disable)
    }
    /// What COSMIC does with this entry: unknown actions are off there.
    pub fn active(&self) -> bool {
        !matches!(self.action, Some(Action::Disable) | None)
    }
    pub fn command(&self) -> Option<&str> {
        match &self.action {
            Some(Action::Spawn(command)) => Some(command),
            _ => None,
        }
    }
    pub fn label(&self) -> String {
        match &self.action {
            Some(action) => action_label(action),
            None => "Unrecognized action".into(),
        }
    }
}

/// Every entry in source order, duplicates included. Borrowing raw values
/// keeps each binding's and action's exact text.
struct RawMap<'a>(Vec<(&'a ron::value::RawValue, &'a ron::value::RawValue)>);

impl<'de: 'a, 'a> Deserialize<'de> for RawMap<'a> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Entries;
        impl<'de> Visitor<'de> for Entries {
            type Value = RawMap<'de>;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map of shortcuts")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut access: M) -> Result<Self::Value, M::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = access.next_entry()? {
                    entries.push(entry);
                }
                Ok(RawMap(entries))
            }
        }
        deserializer.deserialize_map(Entries)
    }
}

/// Reads a shortcut collection. A binding upstream would refuse (an unknown
/// field, an unknown key name) makes the collection unreadable, as it is
/// for COSMIC; an unrecognized action does not.
pub fn parse_map(text: &str) -> Result<Vec<Entry>, String> {
    let map: RawMap = ron::from_str(text).map_err(|error| error.to_string())?;
    map.0
        .into_iter()
        .map(|(binding, action)| {
            let binding_raw = binding.trim().get_ron().to_string();
            let action_raw = action.trim().get_ron().to_string();
            let binding = ron::from_str::<Binding>(&binding_raw)
                .map_err(|error| format!("shortcut {binding_raw}: {error}"))?;
            Ok(Entry {
                binding,
                action: ron::from_str::<Action>(&action_raw).ok(),
                binding_raw,
                action_raw,
            })
        })
        .collect()
}

/// Whether the file switches on RON extensions; its entries might depend on
/// them, so it is never rewritten without them.
pub fn uses_extensions(text: &str) -> bool {
    text.trim_start().starts_with("#!")
}

/// The collection as RON, each entry's text reproduced exactly.
pub fn write_map(entries: &[Entry]) -> String {
    let mut text = String::from("{\n");
    for entry in entries {
        text.push_str("    ");
        text.push_str(&entry.binding_raw);
        text.push_str(": ");
        text.push_str(&entry.action_raw);
        text.push_str(",\n");
    }
    text.push('}');
    text
}
