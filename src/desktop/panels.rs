//! Dock and Panel settings, mirroring `cosmic-panel-config` at cosmic-panel
//! commit 3ce6ce1 (`cosmic-panel-config/src/panel_config.rs`).
//!
//! Each panel is a namespace `com.system76.CosmicPanel.<Name>` (v1), with
//! one key per field. A `PanelDraft` records only the keys a control touched
//! and builds a `DesktopPatch`; values it cannot decode stay `Unsupported`
//! and are never rewritten unless the user picks a new value.
use super::store::{
    ConfigKey, DesktopError, DesktopPatch, DesktopStore, KeyRule, Namespace, RawValue, Snapshot,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const NAMESPACE: &str = "com.system76.CosmicPanel";
pub const VERSION: u64 = 1;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum PanelKind {
    #[default]
    Dock,
    Panel,
}

impl PanelKind {
    pub const ALL: [Self; 2] = [Self::Dock, Self::Panel];
    /// Entry name in `com.system76.CosmicPanel/v1/entries`.
    pub fn entry(self) -> &'static str {
        match self {
            Self::Dock => "Dock",
            Self::Panel => "Panel",
        }
    }
    pub fn title(self) -> &'static str {
        self.entry()
    }
    pub fn namespace(self) -> String {
        format!("{NAMESPACE}.{}", self.entry())
    }
    /// The matching native settings page.
    pub fn system_page(self) -> &'static str {
        match self {
            Self::Dock => "dock",
            Self::Panel => "panel",
        }
    }
}

// Upstream types. Serialized names and shapes must stay identical.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Anchor {
    Left,
    Right,
    Top,
    Bottom,
}

impl Anchor {
    pub const ALL: [Self; 4] = [Self::Top, Self::Bottom, Self::Left, Self::Right];
    pub fn title(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Top => "Top",
            Self::Bottom => "Bottom",
        }
    }
    pub fn horizontal(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanelSize {
    XS,
    S,
    M,
    L,
    XL,
    Custom(u32),
}

impl PanelSize {
    pub const CHOICES: [Self; 5] = [Self::XS, Self::S, Self::M, Self::L, Self::XL];
    pub fn title(&self) -> String {
        match self {
            Self::XS => "XS".into(),
            Self::S => "S".into(),
            Self::M => "M".into(),
            Self::L => "L".into(),
            Self::XL => "XL".into(),
            Self::Custom(size) => format!("Custom {size}"),
        }
    }
    /// Non-symbolic applet icon size plus padding, from upstream
    /// `get_applet_icon_size_with_padding(false)`.
    pub fn thickness(&self) -> u32 {
        match self {
            Self::XS => 24 + 8,
            Self::S => 32 + 8,
            Self::M => 40 + 16,
            Self::L => 48 + 16,
            Self::XL => 56 + 24,
            Self::Custom(size) => (*size).max(16) / 4 * 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoHide {
    Never,
    OnOverlap,
    Always,
}

impl AutoHide {
    pub const ALL: [Self; 3] = [Self::Never, Self::OnOverlap, Self::Always];
    pub fn title(self) -> &'static str {
        match self {
            Self::Never => "Always visible",
            Self::OnOverlap => "Hide behind windows",
            Self::Always => "Auto-hide",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoHideBehavior {
    pub wait_time: u32,
    pub transition_time: u32,
    pub handle_size: u32,
    pub unhide_delay: u32,
}

impl Default for AutoHideBehavior {
    fn default() -> Self {
        Self {
            wait_time: 1000,
            transition_time: 200,
            handle_size: 4,
            unhide_delay: 200,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timing {
    Wait,
    Transition,
    Handle,
    Unhide,
}

impl Timing {
    pub const ALL: [Self; 4] = [Self::Wait, Self::Unhide, Self::Transition, Self::Handle];
    pub fn title(self) -> &'static str {
        match self {
            Self::Wait => "Hide after",
            Self::Transition => "Slide duration",
            Self::Handle => "Reveal edge",
            Self::Unhide => "Show after",
        }
    }
    pub fn range(self) -> std::ops::RangeInclusive<u32> {
        match self {
            Self::Wait => 0..=5000,
            Self::Transition => 0..=1000,
            Self::Handle => 1..=16,
            Self::Unhide => 0..=2000,
        }
    }
    pub fn step(self) -> u32 {
        match self {
            Self::Handle => 1,
            _ => 50,
        }
    }
    pub fn unit(self) -> &'static str {
        match self {
            Self::Handle => "px",
            _ => "ms",
        }
    }
    pub fn get(self, behavior: &AutoHideBehavior) -> u32 {
        match self {
            Self::Wait => behavior.wait_time,
            Self::Transition => behavior.transition_time,
            Self::Handle => behavior.handle_size,
            Self::Unhide => behavior.unhide_delay,
        }
    }
    fn set(self, behavior: &mut AutoHideBehavior, value: u32) {
        match self {
            Self::Wait => behavior.wait_time = value,
            Self::Transition => behavior.transition_time = value,
            Self::Handle => behavior.handle_size = value,
            Self::Unhide => behavior.unhide_delay = value,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Background {
    ThemeDefault,
    Dark,
    Light,
    Color([f32; 3]),
}

/// Penumbra's existing per-bar record, independent of the Islands sidecar.
/// Keep the legacy key and fields compatible with motixo/penumbra-panel.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PenumbraGradient {
    pub enabled: bool,
    pub height: u32,
    pub opacity: f32,
    pub maximized_color: [u8; 3],
    pub maximized_opacity: f32,
}

impl Default for PenumbraGradient {
    fn default() -> Self {
        Self {
            enabled: true,
            height: 96,
            opacity: 0.4,
            maximized_color: [0x24, 0x26, 0x2a],
            maximized_opacity: 1.,
        }
    }
}

fn check_penumbra(value: &RawValue) -> Result<(), String> {
    if let Some(effect) = value.decode::<Option<PenumbraGradient>>()?
        && (!(16..=512).contains(&effect.height)
            || !effect.opacity.is_finite()
            || !(0.0..=1.0).contains(&effect.opacity)
            || !effect.maximized_opacity.is_finite()
            || !(0.0..=1.0).contains(&effect.maximized_opacity))
    {
        return Err("Unsupported Penumbra gradient settings".into());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Output {
    All,
    Active,
    Name(String),
}

/// A decoded effective value, or the raw text of one we do not understand.
#[derive(Clone, Debug, PartialEq)]
pub enum Setting<T> {
    Known(T),
    Unsupported(String),
}

impl<T> Setting<T> {
    pub fn known(&self) -> Option<&T> {
        match self {
            Self::Known(value) => Some(value),
            Self::Unsupported(_) => None,
        }
    }
}

// Upper bounds keep values inside what cosmic-panel renders safely; for
// example `get_dimensions` asserts `2 * padding + gap` fits the bar.
pub const MAX_PADDING: u32 = 16;
pub const MAX_SPACING: u32 = 24;
pub const MAX_MARGIN: u16 = 48;
pub const MAX_RADIUS: u32 = 160;

fn typed<T: DeserializeOwned>(value: &RawValue) -> Result<T, String> {
    value.decode::<T>()
}
fn check_anchor(value: &RawValue) -> Result<(), String> {
    typed::<Anchor>(value).map(drop)
}
fn check_bool(value: &RawValue) -> Result<(), String> {
    typed::<bool>(value).map(drop)
}
fn check_size_value(size: &PanelSize) -> Result<(), String> {
    match size {
        PanelSize::Custom(size) if !(16..=256).contains(size) => {
            Err("Custom size must be 16–256".into())
        }
        _ => Ok(()),
    }
}
fn check_size(value: &RawValue) -> Result<(), String> {
    check_size_value(&typed::<PanelSize>(value)?)
}
fn check_size_wings(value: &RawValue) -> Result<(), String> {
    if let Some((start, end)) = typed::<Option<(Option<PanelSize>, Option<PanelSize>)>>(value)? {
        for size in start.iter().chain(end.iter()) {
            check_size_value(size)?;
        }
    }
    Ok(())
}
fn check_size_center(value: &RawValue) -> Result<(), String> {
    typed::<Option<PanelSize>>(value)?
        .as_ref()
        .map_or(Ok(()), check_size_value)
}
fn check_autohide(value: &RawValue) -> Result<(), String> {
    typed::<AutoHide>(value).map(drop)
}
fn check_behavior(value: &RawValue) -> Result<(), String> {
    let behavior = typed::<AutoHideBehavior>(value)?;
    for timing in Timing::ALL {
        if !timing.range().contains(&timing.get(&behavior)) {
            return Err(format!("{} is out of range", timing.title()));
        }
    }
    Ok(())
}
fn check_margin(value: &RawValue) -> Result<(), String> {
    (typed::<u16>(value)? <= MAX_MARGIN)
        .then_some(())
        .ok_or_else(|| format!("Gap must be at most {MAX_MARGIN}"))
}
fn check_opacity(value: &RawValue) -> Result<(), String> {
    let opacity = typed::<f32>(value)?;
    (opacity.is_finite() && (0.0..=1.0).contains(&opacity))
        .then_some(())
        .ok_or_else(|| "Opacity must be between 0 and 1".into())
}
fn bounded(value: &RawValue, max: u32) -> Result<(), String> {
    (typed::<u32>(value)? <= max)
        .then_some(())
        .ok_or_else(|| format!("Must be at most {max}"))
}
fn check_radius(value: &RawValue) -> Result<(), String> {
    bounded(value, MAX_RADIUS)
}
fn check_spacing(value: &RawValue) -> Result<(), String> {
    bounded(value, MAX_SPACING)
}
fn check_padding(value: &RawValue) -> Result<(), String> {
    bounded(value, MAX_PADDING)
}
fn check_output(value: &RawValue) -> Result<(), String> {
    match typed::<Output>(value)? {
        Output::Name(name) if !valid_label(&name, 64) => Err("Invalid display name".into()),
        _ => Ok(()),
    }
}
fn check_background(value: &RawValue) -> Result<(), String> {
    match typed::<Background>(value)? {
        Background::Color(color)
            if !color
                .iter()
                .all(|c| c.is_finite() && (0.0..=1.0).contains(c)) =>
        {
            Err("Invalid color".into())
        }
        _ => Ok(()),
    }
}
fn valid_label(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn check_applet_ids(ids: &[String]) -> Result<(), String> {
    if ids.len() > 64 || !ids.iter().all(|id| valid_label(id, 255)) {
        return Err("Invalid applet list".into());
    }
    Ok(())
}
fn check_wings(value: &RawValue) -> Result<(), String> {
    if let Some((start, end)) = typed::<Option<(Vec<String>, Vec<String>)>>(value)? {
        check_applet_ids(&start)?;
        check_applet_ids(&end)?;
    }
    Ok(())
}
fn check_center(value: &RawValue) -> Result<(), String> {
    typed::<Option<Vec<String>>>(value)?
        .as_deref()
        .map_or(Ok(()), check_applet_ids)
}

macro_rules! rules {
    ($($key:literal => $validate:expr),+ $(,)?) => {
        [$(KeyRule {
            namespace: Namespace::Entry(NAMESPACE),
            version: VERSION,
            key: $key,
            validate: $validate,
        }),+]
    };
}

/// The keys this editor may write, for any panel entry.
pub static RULES: [KeyRule; 20] = rules![
    "anchor" => check_anchor,
    "anchor_gap" => check_bool,
    "margin" => check_margin,
    "size" => check_size,
    "size_wings" => check_size_wings,
    "size_center" => check_size_center,
    "expand_to_edges" => check_bool,
    "autohide" => check_autohide,
    "exclusive_zone" => check_bool,
    "autohide_behavior" => check_behavior,
    "opacity" => check_opacity,
    "top_edge_gradient" => check_penumbra,
    "border_radius" => check_radius,
    "spacing" => check_spacing,
    "padding" => check_padding,
    "output" => check_output,
    "background" => check_background,
    "keep_style_on_maximize" => check_bool,
    "plugins_wings" => check_wings,
    "plugins_center" => check_center,
];

/// Keys that one control writes together. When an external change makes one
/// of them conflict, the whole group's edits are dropped.
const GROUPS: [&[&str]; 4] = [
    &["autohide", "exclusive_zone"],
    &["anchor_gap", "margin", "top_edge_gradient"],
    &["size", "size_wings", "size_center"],
    &["plugins_wings", "plugins_center"],
];

/// Short user-facing name of a panel key, for conflict messages.
pub fn key_label(name: &str) -> &'static str {
    match name {
        "islands" => "islands",
        "anchor" => "position",
        "anchor_gap" => "floating",
        "margin" => "edge gap",
        "size" | "size_wings" | "size_center" => "size",
        "expand_to_edges" => "extend to edges",
        "autohide" | "exclusive_zone" => "hiding",
        "autohide_behavior" => "hide timing",
        "opacity" => "opacity",
        "top_edge_gradient" => "Penumbra gradient",
        "border_radius" => "corners",
        "spacing" => "spacing",
        "padding" => "padding",
        "output" => "display",
        "background" => "style",
        "keep_style_on_maximize" => "maximized style",
        "plugins_wings" | "plugins_center" => "applets",
        _ => "setting",
    }
}

fn key(kind: PanelKind, name: &str) -> ConfigKey {
    if name == "islands" {
        return super::islands::key(kind);
    }
    ConfigKey::new(kind.namespace(), VERSION, name).expect("panel keys are valid")
}
fn entries_key() -> ConfigKey {
    ConfigKey::new(NAMESPACE, VERSION, "entries").expect("container key is valid")
}

/// Ordered applet regions: wings start, center, wings end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Region {
    Start,
    Center,
    End,
}

impl Region {
    pub const ALL: [Self; 3] = [Self::Start, Self::Center, Self::End];
    pub fn title(self, horizontal: bool) -> &'static str {
        match (self, horizontal) {
            (Self::Start, true) => "Left",
            (Self::Start, false) => "Top",
            (Self::Center, true) => "Center",
            (Self::Center, false) => "Middle",
            (Self::End, true) => "Right",
            (Self::End, false) => "Bottom",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Center => "center",
            Self::End => "end",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|region| region.key() == value)
    }
    fn previous(self) -> Option<Self> {
        match self {
            Self::Start => None,
            Self::Center => Some(Self::Start),
            Self::End => Some(Self::Center),
        }
    }
    fn next(self) -> Option<Self> {
        match self {
            Self::Start => Some(Self::Center),
            Self::Center => Some(Self::End),
            Self::End => None,
        }
    }
}

/// A position: an occurrence, not an applet ID, so duplicates stay distinct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub region: Region,
    pub index: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Regions {
    pub start: Vec<String>,
    pub center: Vec<String>,
    pub end: Vec<String>,
}

impl Regions {
    pub fn list(&self, region: Region) -> &[String] {
        match region {
            Region::Start => &self.start,
            Region::Center => &self.center,
            Region::End => &self.end,
        }
    }
    fn list_mut(&mut self, region: Region) -> &mut Vec<String> {
        match region {
            Region::Start => &mut self.start,
            Region::Center => &mut self.center,
            Region::End => &mut self.end,
        }
    }
    pub fn contains(&self, id: &str) -> bool {
        Region::ALL
            .into_iter()
            .any(|region| self.list(region).iter().any(|item| item == id))
    }
    /// Drag and drop: within a region the item takes the target's place
    /// (like the existing reorder lists); across regions it is inserted
    /// before the target, or appended when `to.index` is the region length.
    pub fn move_item(&mut self, from: Slot, to: Slot) -> Option<Slot> {
        if from.index >= self.list(from.region).len() {
            return None;
        }
        if from.region == to.region {
            let last = self.list(to.region).len() - 1;
            let index = to.index.min(last);
            if index == from.index {
                return None;
            }
            let list = self.list_mut(from.region);
            let item = list.remove(from.index);
            list.insert(index, item);
            return Some(Slot {
                region: to.region,
                index,
            });
        }
        let index = to.index.min(self.list(to.region).len());
        let item = self.list_mut(from.region).remove(from.index);
        self.list_mut(to.region).insert(index, item);
        Some(Slot {
            region: to.region,
            index,
        })
    }
    /// Keyboard movement by one step. Past a region's edge the item moves to
    /// the nearest end of the adjacent region.
    pub fn step(&mut self, from: Slot, delta: i32) -> Option<Slot> {
        let len = self.list(from.region).len();
        if from.index >= len || delta == 0 {
            return None;
        }
        if delta < 0 {
            if from.index > 0 {
                self.list_mut(from.region).swap(from.index, from.index - 1);
                return Some(Slot {
                    index: from.index - 1,
                    ..from
                });
            }
            let region = from.region.previous()?;
            let item = self.list_mut(from.region).remove(from.index);
            let list = self.list_mut(region);
            list.push(item);
            return Some(Slot {
                region,
                index: list.len() - 1,
            });
        }
        if from.index + 1 < len {
            self.list_mut(from.region).swap(from.index, from.index + 1);
            return Some(Slot {
                index: from.index + 1,
                ..from
            });
        }
        let region = from.region.next()?;
        let item = self.list_mut(from.region).remove(from.index);
        self.list_mut(region).insert(0, item);
        Some(Slot { region, index: 0 })
    }
    pub fn remove(&mut self, slot: Slot) -> Option<String> {
        let list = self.list_mut(slot.region);
        (slot.index < list.len()).then(|| list.remove(slot.index))
    }
    pub fn add(&mut self, region: Region, id: impl Into<String>) -> Slot {
        let list = self.list_mut(region);
        list.push(id.into());
        Slot {
            region,
            index: list.len() - 1,
        }
    }
}

/// Pointer drag of one applet occurrence. Cancel or a drop outside every
/// target leaves the regions untouched.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppletDrag {
    pub source: Option<Slot>,
    pub target: Option<Slot>,
}

impl AppletDrag {
    pub fn start(&mut self, slot: Slot) {
        self.source = Some(slot);
        self.target = Some(slot);
    }
    pub fn over(&mut self, slot: Slot) {
        if self.source.is_some() {
            self.target = Some(slot);
        }
    }
    pub fn leave(&mut self, slot: Slot) {
        if self.target == Some(slot) {
            self.target = None;
        }
    }
    pub fn cancel(&mut self) {
        self.source = None;
        self.target = None;
    }
    pub fn active(&self) -> bool {
        self.source.is_some()
    }
    pub fn finish(&mut self, regions: &mut Regions) -> Option<Slot> {
        let (source, target) = (self.source.take(), self.target.take());
        regions.move_item(source?, target?)
    }
}

/// Upstream `CosmicPanelConfig::default()` values, used when neither the
/// user nor the system defines a key (cosmic-panel does the same).
fn upstream_default(name: &str) -> Option<RawValue> {
    if name == "islands" {
        return RawValue::encode(&dock_islands_core::Settings::default()).ok();
    }
    let value = match name {
        "anchor" => RawValue::encode(&Anchor::Top),
        "anchor_gap" => RawValue::encode(&false),
        "size" => RawValue::encode(&PanelSize::M),
        "output" => RawValue::encode(&Output::All),
        "background" => RawValue::encode(&Background::ThemeDefault),
        "plugins_wings" => RawValue::encode(&None::<(Vec<String>, Vec<String>)>),
        "plugins_center" => RawValue::encode(&None::<Vec<String>>),
        "size_wings" => RawValue::encode(&None::<(Option<PanelSize>, Option<PanelSize>)>),
        "size_center" => RawValue::encode(&None::<PanelSize>),
        "expand_to_edges" => RawValue::encode(&true),
        "padding" => RawValue::encode(&4u32),
        "spacing" => RawValue::encode(&0u32),
        "exclusive_zone" => RawValue::encode(&true),
        "autohide" => RawValue::encode(&AutoHide::Never),
        "autohide_behavior" => RawValue::encode(&AutoHideBehavior::default()),
        "border_radius" => RawValue::encode(&8u32),
        "margin" => RawValue::encode(&4u16),
        "opacity" => RawValue::encode(&0.8f32),
        "top_edge_gradient" => RawValue::encode(&None::<PenumbraGradient>),
        "keep_style_on_maximize" => RawValue::encode(&false),
        _ => return None,
    };
    value.ok()
}

/// One panel's draft: the snapshot it started from plus touched keys.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelDraft {
    kind: PanelKind,
    base: Snapshot,
    edits: BTreeMap<&'static str, Option<RawValue>>,
    conflicts: Vec<&'static str>,
}

// Lifetimes in the in-memory key table stay static. The recovery record
// carries names and validates them back through the allowlist on read.
#[derive(Serialize, Deserialize)]
struct PanelDraftRecord {
    kind: PanelKind,
    base: Snapshot,
    edits: BTreeMap<String, Option<RawValue>>,
}
impl Serialize for PanelDraft {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        PanelDraftRecord {
            kind: self.kind,
            base: self.base.clone(),
            edits: self
                .edits
                .iter()
                .map(|(key, value)| ((*key).into(), value.clone()))
                .collect(),
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for PanelDraft {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let record = PanelDraftRecord::deserialize(deserializer)?;
        let mut draft = Self::from_snapshot(record.kind, record.base);
        for (name, value) in record.edits {
            let key = if name == "islands" {
                "islands"
            } else {
                RULES
                    .iter()
                    .find(|rule| rule.key == name)
                    .map(|rule| rule.key)
                    .ok_or_else(|| {
                        serde::de::Error::custom("Unknown panel setting in saved draft")
                    })?
            };
            draft.edits.insert(key, value);
        }
        Ok(draft)
    }
}

impl PanelDraft {
    /// Keys read for a panel: every writable key, its name and the
    /// container's entry list.
    pub fn keys(kind: PanelKind) -> Vec<ConfigKey> {
        RULES
            .iter()
            .map(|rule| key(kind, rule.key))
            .chain([key(kind, "name"), entries_key(), key(kind, "islands")])
            .collect()
    }
    pub fn load(store: &DesktopStore, kind: PanelKind) -> Result<Self, DesktopError> {
        Ok(Self::from_snapshot(kind, store.load(&Self::keys(kind))?))
    }
    pub fn from_snapshot(kind: PanelKind, base: Snapshot) -> Self {
        Self {
            kind,
            base,
            edits: BTreeMap::new(),
            conflicts: Vec::new(),
        }
    }
    pub fn kind(&self) -> PanelKind {
        self.kind
    }
    /// The values the draft started from (its edits are not included).
    pub fn into_snapshot(self) -> Snapshot {
        self.base
    }
    /// Whether cosmic-panel lists this panel. An unlisted panel is off.
    pub fn listed(&self) -> bool {
        self.base
            .effective(&entries_key())
            .and_then(|value| value.decode::<Vec<String>>().ok())
            .is_some_and(|entries| entries.iter().any(|entry| entry == self.kind.entry()))
    }
    /// Whether any user or system value exists for this panel at all.
    pub fn installed(&self) -> bool {
        self.base
            .keys()
            .filter(|key| key.namespace() == self.kind.namespace())
            .any(|key| self.base.effective(key).is_some())
    }
    pub fn dirty(&self) -> bool {
        !self.edits.is_empty()
    }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn edited(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.edits.keys().copied()
    }
    /// Edited keys that changed outside the draft since it began.
    pub fn conflicts(&self) -> &[&'static str] {
        &self.conflicts
    }
    pub fn discard(&mut self) {
        self.edits.clear();
        self.conflicts.clear();
    }
    /// Edited keys plus the other members of their coupled groups. A
    /// control derives each group from all of its members, so all of them
    /// keep the draft's base and are checked for external changes.
    fn pinned(&self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = Vec::new();
        for name in self.edits.keys() {
            let group = GROUPS
                .iter()
                .find(|group| group.contains(name))
                .map_or(&[][..], |group| &group[..]);
            for member in std::iter::once(*name).chain(group.iter().copied()) {
                if !names.contains(&member) {
                    names.push(member);
                }
            }
        }
        names
    }
    pub fn patch(&self) -> DesktopPatch {
        let mut patch = DesktopPatch::new();
        for name in self.pinned() {
            let key = key(self.kind, name);
            let original = self.base.user(&key).cloned();
            match self.edits.get(name) {
                Some(desired) => patch.set(key, original, desired.clone()),
                None => patch.guard(key, original),
            }
        }
        patch
    }
    /// Starts over from `snapshot` (after Apply, Undo or Discard + reload).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn rebase(&mut self, snapshot: Snapshot) {
        self.base = snapshot;
        self.discard();
    }
    /// Takes external changes to untouched keys. Edited keys and their
    /// coupled peers keep their original base so Apply can still detect the
    /// conflict; one that is back at its base (say, after a restore) no
    /// longer conflicts.
    pub fn refresh(&mut self, snapshot: Snapshot) {
        let mut base = snapshot;
        for name in self.pinned() {
            let key = key(self.kind, name);
            let old = self.base.entry(&key).cloned().unwrap_or_default();
            let changed = base.user(&key) != old.user.as_ref();
            if changed && !self.conflicts.contains(&name) {
                self.conflicts.push(name);
            } else if !changed {
                self.conflicts.retain(|conflict| *conflict != name);
            }
            base.insert(key, old);
        }
        self.base = base;
    }
    /// Drops edits of conflicting key groups and adopts their current values,
    /// keeping every other edit.
    pub fn resolve_conflicts(&mut self, snapshot: &Snapshot, keys: &[ConfigKey]) {
        let mut names: Vec<&'static str> = keys
            .iter()
            .filter_map(|item| {
                if *item == key(self.kind, "islands") {
                    Some("islands")
                } else if item.namespace() == self.kind.namespace() {
                    RULES
                        .iter()
                        .find(|rule| rule.key == item.key())
                        .map(|r| r.key)
                } else {
                    None
                }
            })
            .chain(self.conflicts.iter().copied())
            .collect();
        for group in GROUPS {
            if names.iter().any(|name| group.contains(name)) {
                names.extend(group.iter().copied());
            }
        }
        for name in names {
            self.edits.remove(name);
            let key = key(self.kind, name);
            if let Some(entry) = snapshot.entry(&key) {
                self.base.insert(key, entry.clone());
            }
        }
        self.conflicts.clear();
    }

    fn raw(&self, name: &str) -> Option<RawValue> {
        let key = key(self.kind, name);
        match self.edits.get(name) {
            Some(Some(value)) => Some(value.clone()),
            Some(None) => self.base.entry(&key).and_then(|entry| entry.system.clone()),
            None => self.base.effective(&key).cloned(),
        }
        .or_else(|| upstream_default(name))
    }
    fn get<T: DeserializeOwned>(&self, name: &str) -> Setting<T> {
        match self.raw(name) {
            Some(value) => value
                .decode()
                .map(Setting::Known)
                .unwrap_or_else(|_| Setting::Unsupported(value.as_str().trim().to_string())),
            None => Setting::Unsupported(String::new()),
        }
    }
    fn base_value<T: DeserializeOwned>(&self, name: &str) -> Option<T> {
        let key = key(self.kind, name);
        self.base
            .effective(&key)
            .cloned()
            .or_else(|| upstream_default(name))?
            .decode()
            .ok()
    }
    /// Records `value` for `name`, or forgets the edit when it matches the
    /// value the draft started from.
    fn set<T: Serialize + DeserializeOwned + PartialEq>(&mut self, name: &'static str, value: &T) {
        if self.base_value::<T>(name).as_ref() == Some(value) {
            self.edits.remove(name);
        } else if let Ok(encoded) = RawValue::encode(value) {
            self.edits.insert(name, Some(encoded));
        }
    }

    /// The effective value of writable key `name` (for profiles), or
    /// `Unsupported` for an unknown key or a value this version can't decode.
    pub fn value<T: DeserializeOwned>(&self, name: &str) -> Setting<T> {
        match RULES.iter().find(|rule| rule.key == name) {
            Some(rule) => self.get(rule.key),
            None => Setting::Unsupported(String::new()),
        }
    }
    /// Records `value` for writable key `name` (the coupled peers of a
    /// group stay guarded by `patch`). Returns false for any other name.
    pub fn set_value<T: Serialize + DeserializeOwned + PartialEq>(
        &mut self,
        name: &str,
        value: &T,
    ) -> bool {
        match RULES.iter().find(|rule| rule.key == name) {
            Some(rule) => {
                self.set(rule.key, value);
                true
            }
            None => false,
        }
    }

    pub fn islands(&self) -> Setting<dock_islands_core::Settings> {
        match self.get::<dock_islands_core::Settings>("islands") {
            Setting::Known(s) if s.validate().is_ok() => Setting::Known(s),
            _ => Setting::Unsupported("Unsupported island settings are kept".into()),
        }
    }
    pub fn set_islands(&mut self, value: dock_islands_core::Settings) {
        if value.validate().is_ok() {
            self.set("islands", &value);
        }
    }
    pub fn islands_base(&self) -> Option<dock_islands_core::Settings> {
        self.base_value("islands")
    }

    pub fn anchor(&self) -> Setting<Anchor> {
        self.get("anchor")
    }
    pub fn set_anchor(&mut self, anchor: Anchor) {
        self.set("anchor", &anchor);
    }
    pub fn size(&self) -> Setting<PanelSize> {
        self.get("size")
    }
    /// Like cosmic-settings, a new size also clears per-region overrides.
    pub fn set_size(&mut self, size: PanelSize) {
        self.set("size", &size);
        if !matches!(
            self.get::<Option<(Option<PanelSize>, Option<PanelSize>)>>("size_wings"),
            Setting::Known(None)
        ) {
            self.set(
                "size_wings",
                &None::<(Option<PanelSize>, Option<PanelSize>)>,
            );
        }
        if !matches!(
            self.get::<Option<PanelSize>>("size_center"),
            Setting::Known(None)
        ) {
            self.set("size_center", &None::<PanelSize>);
        }
    }
    pub fn autohide(&self) -> Setting<AutoHide> {
        self.get("autohide")
    }
    /// Hidden panels give up their reserved space, as in cosmic-settings.
    pub fn set_autohide(&mut self, mode: AutoHide) {
        self.set("autohide", &mode);
        self.set("exclusive_zone", &(mode == AutoHide::Never));
    }
    pub fn behavior(&self) -> Setting<AutoHideBehavior> {
        self.get("autohide_behavior")
    }
    pub fn set_timing(&mut self, timing: Timing, value: u32) {
        if let Setting::Known(mut behavior) = self.behavior() {
            let range = timing.range();
            timing.set(&mut behavior, value.clamp(*range.start(), *range.end()));
            self.set("autohide_behavior", &behavior);
        }
    }
    pub fn floating(&self) -> Setting<bool> {
        self.get("anchor_gap")
    }
    /// Floating uses a gap from the screen edge (cosmic-settings uses 4 px).
    pub fn set_floating(&mut self, floating: bool) {
        if floating {
            self.set_penumbra(false);
        }
        self.set("anchor_gap", &floating);
        let margin = self.margin().known().copied().unwrap_or(0);
        if floating && margin == 0 {
            self.set("margin", &4u16);
        } else if !floating {
            self.set("margin", &0u16);
        }
    }
    pub fn margin(&self) -> Setting<u16> {
        self.get("margin")
    }
    pub fn set_margin(&mut self, margin: u16) {
        self.set("margin", &margin.min(MAX_MARGIN));
    }
    pub fn extend(&self) -> Setting<bool> {
        self.get("expand_to_edges")
    }
    pub fn set_extend(&mut self, extend: bool) {
        self.set("expand_to_edges", &extend);
    }
    pub fn opacity(&self) -> Setting<f32> {
        self.get("opacity")
    }
    pub fn penumbra(&self) -> Setting<Option<PenumbraGradient>> {
        if let Some(raw) = self.raw("top_edge_gradient")
            && check_penumbra(&raw).is_err()
        {
            return Setting::Unsupported(raw.as_str().trim().into());
        }
        self.get("top_edge_gradient")
    }
    pub fn set_penumbra(&mut self, enabled: bool) {
        if let Setting::Known(current) = self.penumbra() {
            if current.is_none() && !enabled {
                return;
            }
            let mut effect = current.unwrap_or_default();
            effect.enabled = enabled;
            self.set("top_edge_gradient", &Some(effect));
            if enabled {
                self.set_floating(false);
            }
        }
    }
    pub fn set_penumbra_height(&mut self, height: u32) {
        if let Setting::Known(Some(mut effect)) = self.penumbra()
            && effect.enabled
        {
            effect.height = height.clamp(16, 512);
            self.set("top_edge_gradient", &Some(effect));
        }
    }
    pub fn set_penumbra_maximized_opacity(&mut self, percent: u8) {
        if let Setting::Known(Some(mut effect)) = self.penumbra()
            && effect.enabled
        {
            effect.maximized_opacity = f32::from(percent.min(100)) / 100.;
            self.set("top_edge_gradient", &Some(effect));
        }
    }
    pub fn set_opacity_percent(&mut self, percent: u8) {
        let target = f32::from(percent.min(100)) / 100.;
        // Keep the stored value when it already rounds to this percentage.
        if let Setting::Known(current) = self.opacity()
            && (current * 100.).round() as u8 == percent.min(100)
        {
            return;
        }
        self.set("opacity", &target);
    }
    pub fn radius(&self) -> Setting<u32> {
        self.get("border_radius")
    }
    pub fn set_radius(&mut self, radius: u32) {
        self.set("border_radius", &radius.min(MAX_RADIUS));
    }
    pub fn spacing(&self) -> Setting<u32> {
        self.get("spacing")
    }
    pub fn set_spacing(&mut self, spacing: u32) {
        self.set("spacing", &spacing.min(MAX_SPACING));
    }
    pub fn padding(&self) -> Setting<u32> {
        self.get("padding")
    }
    pub fn set_padding(&mut self, padding: u32) {
        self.set("padding", &padding.min(MAX_PADDING));
    }
    pub fn output(&self) -> Setting<Output> {
        self.get("output")
    }
    pub fn set_output(&mut self, output: Output) {
        self.set("output", &output);
    }
    pub fn background(&self) -> Setting<Background> {
        self.get("background")
    }
    pub fn set_background(&mut self, background: Background) {
        self.set("background", &background);
    }
    pub fn keep_style(&self) -> Setting<bool> {
        self.get("keep_style_on_maximize")
    }
    pub fn set_keep_style(&mut self, keep: bool) {
        self.set("keep_style_on_maximize", &keep);
    }

    pub fn applets(&self) -> Setting<Regions> {
        let wings = self.get::<Option<(Vec<String>, Vec<String>)>>("plugins_wings");
        let center = self.get::<Option<Vec<String>>>("plugins_center");
        match (wings, center) {
            (Setting::Known(wings), Setting::Known(center)) => {
                let (start, end) = wings.unwrap_or_default();
                Setting::Known(Regions {
                    start,
                    center: center.unwrap_or_default(),
                    end,
                })
            }
            (Setting::Unsupported(raw), _) | (_, Setting::Unsupported(raw)) => {
                Setting::Unsupported(raw)
            }
        }
    }
    /// Writes only the region keys whose lists changed.
    pub fn set_applets(&mut self, regions: &Regions) {
        let Setting::Known(current) = self.applets() else {
            return;
        };
        if current.start != regions.start || current.end != regions.end {
            let wings = Some((regions.start.clone(), regions.end.clone()));
            let unchanged = self
                .base_value::<Option<(Vec<String>, Vec<String>)>>("plugins_wings")
                .map(|base| base.unwrap_or_default())
                == wings.clone();
            if unchanged {
                self.edits.remove("plugins_wings");
            } else {
                self.set("plugins_wings", &wings);
            }
        }
        if current.center != regions.center {
            let unchanged = self
                .base_value::<Option<Vec<String>>>("plugins_center")
                .map(|base| base.unwrap_or_default())
                == Some(regions.center.clone());
            if unchanged {
                self.edits.remove("plugins_center");
            } else {
                self.set("plugins_center", &Some(regions.center.clone()));
            }
        }
    }
}

/// Connected display connectors from DRM sysfs (`card1-DP-1` → `DP-1`),
/// the names cosmic-panel matches in `Output::Name`.
pub fn connected_outputs(drm: &Path) -> Vec<String> {
    let mut outputs: Vec<String> = std::fs::read_dir(drm)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let (card, connector) = name.split_once('-')?;
            if !card.starts_with("card") || connector.is_empty() {
                return None;
            }
            let status = std::fs::read_to_string(entry.path().join("status")).ok()?;
            (status.trim() == "connected").then(|| connector.to_string())
        })
        .collect();
    outputs.sort();
    outputs.dedup();
    outputs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::store::tests::Fixture;

    const DOCK: &str = "com.system76.CosmicPanel.Dock";

    #[test]
    fn penumbra_keeps_legacy_preferences_and_islands_independent() {
        let fixture = live_like("penumbra-independent");
        let store = fixture.store();
        let sidecar = super::super::islands::key(PanelKind::Dock);
        let islands = dock_islands_core::Settings {
            enabled: true,
            ..Default::default()
        };
        fixture.user(
            sidecar.namespace(),
            sidecar.key(),
            &ron::to_string(&islands).unwrap(),
        );
        let original = "Some((enabled:false,height:144,opacity:0.65,maximized_color:(10,20,30),maximized_opacity:0.8))";
        fixture.user(DOCK, "top_edge_gradient", original);
        let mut draft = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        let before = draft.penumbra().known().unwrap().unwrap();
        draft.set_penumbra(true);
        let mut expected = before;
        expected.enabled = true;
        assert_eq!(draft.penumbra(), Setting::Known(Some(expected)));
        assert_eq!(
            draft.edited().collect::<Vec<_>>(),
            ["anchor_gap", "margin", "top_edge_gradient"]
        );
        let change = store.apply(&draft.patch()).unwrap();
        assert_eq!(
            PanelDraft::load(&store, PanelKind::Dock).unwrap().islands(),
            Setting::Known(islands)
        );
        assert_eq!(fixture.read(DOCK, "opacity").as_deref(), Some("0.25"));
        assert_eq!(fixture.read(DOCK, "anchor_gap").as_deref(), Some("false"));
        assert_eq!(fixture.read(DOCK, "margin").as_deref(), Some("0"));
        assert_eq!(
            fixture.read("com.system76.CosmicPanel.Panel", "top_edge_gradient"),
            None
        );
        store.undo(&change).unwrap();
        assert_eq!(fixture.read(DOCK, "anchor_gap"), None);
        assert_eq!(fixture.read(DOCK, "margin"), None);
        assert_eq!(
            fixture.read(DOCK, "top_edge_gradient").as_deref(),
            Some(original)
        );
    }

    #[test]
    fn penumbra_absent_and_future_records_are_preserved() {
        let fixture = live_like("penumbra-future");
        let store = fixture.store();
        let mut draft = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        assert_eq!(draft.penumbra(), Setting::Known(None));
        draft.set_penumbra(false);
        assert!(!draft.dirty());
        draft.set_penumbra(true);
        let change = store.apply(&draft.patch()).unwrap();
        store.undo(&change).unwrap();
        assert_eq!(fixture.read(DOCK, "top_edge_gradient"), None);
        for original in [
            "Some((future: true))",
            "Some((height: 0))",
            "Some((opacity: NaN))",
            "Some((maximized_opacity: 2.0))",
        ] {
            fixture.user(DOCK, "top_edge_gradient", original);
            let mut draft = PanelDraft::load(&store, PanelKind::Dock).unwrap();
            assert!(
                matches!(draft.penumbra(), Setting::Unsupported(_)),
                "{original}"
            );
            draft.set_penumbra(true);
            draft.set_penumbra(false);
            assert!(!draft.dirty());
            draft.set_opacity_percent(50);
            store.apply(&draft.patch()).unwrap();
            assert_eq!(
                fixture.read(DOCK, "top_edge_gradient").as_deref(),
                Some(original)
            );
        }
    }

    #[test]
    fn penumbra_rejects_concurrent_preference_changes() {
        let fixture = live_like("penumbra-conflict");
        let store = fixture.store();
        let mut draft = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        draft.set_penumbra(true);
        fixture.user(
            DOCK,
            "top_edge_gradient",
            "Some((enabled:false,height:192))",
        );
        assert!(matches!(
            store.apply(&draft.patch()),
            Err(DesktopError::Conflict(_))
        ));
        assert_eq!(
            fixture.read(DOCK, "top_edge_gradient").as_deref(),
            Some("Some((enabled:false,height:192))")
        );
    }

    #[test]
    fn penumbra_and_floating_are_one_guarded_undoable_choice() {
        let fixture = live_like("penumbra-floating");
        let store = fixture.store();
        let mut draft = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        draft.set_penumbra(true);
        assert_eq!(draft.floating(), Setting::Known(false));
        let enabled = store.apply(&draft.patch()).unwrap();
        let mut draft = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        draft.set_floating(true);
        assert!(!draft.penumbra().known().unwrap().unwrap().enabled);
        assert_eq!(draft.floating(), Setting::Known(true));
        let floating = store.apply(&draft.patch()).unwrap();
        store.undo(&floating).unwrap();
        let draft = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        assert!(draft.penumbra().known().unwrap().unwrap().enabled);
        assert_eq!(draft.floating(), Setting::Known(false));
        store.undo(&enabled).unwrap();
        let mut draft = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        draft.set_penumbra(true);
        fixture.user(DOCK, "anchor_gap", "true");
        assert!(matches!(
            store.apply(&draft.patch()),
            Err(DesktopError::Conflict(_))
        ));
        assert_eq!(fixture.read(DOCK, "top_edge_gradient"), None);
    }

    #[test]
    fn penumbra_settings_survive_disabling_and_ignore_stale_slider_events() {
        let fixture = live_like("penumbra-settings");
        let store = fixture.store();
        let mut draft = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        draft.set_penumbra_height(192);
        assert!(!draft.dirty());
        draft.set_penumbra(true);
        draft.set_penumbra_height(192);
        draft.set_penumbra_maximized_opacity(65);
        store.apply(&draft.patch()).unwrap();
        let mut draft = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        draft.set_penumbra(false);
        store.apply(&draft.patch()).unwrap();
        let mut draft = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        draft.set_penumbra_height(256);
        draft.set_penumbra_maximized_opacity(100);
        assert!(!draft.dirty());
        let saved = draft.penumbra().known().unwrap().unwrap();
        assert_eq!(saved.height, 192);
        assert_eq!(saved.maximized_opacity, 0.65);
        draft.set_penumbra(true);
        assert_eq!(draft.penumbra().known().unwrap().unwrap().height, 192);
        draft.set_penumbra_height(0);
        draft.set_penumbra_maximized_opacity(255);
        let normalized = draft.penumbra().known().unwrap().unwrap();
        assert_eq!(normalized.height, 16);
        assert_eq!(normalized.maximized_opacity, 1.);
        store.apply(&draft.patch()).unwrap();
    }

    /// The installed system defaults and the live user values observed on
    /// this machine (read-only), reproduced as fixture text.
    fn live_like(name: &str) -> Fixture {
        let fixture = Fixture::new(name);
        fixture.system(NAMESPACE, "entries", "[\"Panel\",\"Dock\"]");
        for (key, value) in [
            ("anchor_gap", "true"),
            ("background", "ThemeDefault"),
            ("exclusive_zone", "true"),
            ("opacity", "1.0"),
            ("expand_to_edges", "false"),
            ("border_radius", "160"),
            ("size_center", "None\n"),
            ("autohide", "Never\n"),
            ("output", "All"),
            ("size_wings", "None\n"),
            ("padding", "4"),
            ("margin", "4"),
            ("name", "\"Dock\""),
            ("size", "L"),
            ("anchor", "Bottom"),
            (
                "autohide_behavior",
                "(\n    wait_time: 1000,\n    transition_time: 200,\n    handle_size: 4,\n    unhide_delay: 200,\n)\n",
            ),
            (
                "plugins_center",
                "Some([\"com.system76.CosmicPanelLauncherButton\", \"com.system76.CosmicAppList\"])",
            ),
            ("spacing", "0"),
            ("plugins_wings", "None"),
        ] {
            fixture.system(DOCK, key, value);
        }
        fixture.user(NAMESPACE, "entries", "[\n    \"Panel\",\n    \"Dock\",\n]");
        for (key, value) in [
            ("anchor", "Bottom"),
            ("autohide", "OnOverlap"),
            ("background", "Dark"),
            ("exclusive_zone", "false"),
            ("opacity", "0.25"),
            ("size", "S"),
            (
                "plugins_center",
                "Some([\n    \"io.github.workstation.PersonalQuotaMonitor\",\n])",
            ),
            (
                "plugins_wings",
                "Some(([\n    \"io.github.cosmic_utils.minimon-applet\",\n    \"com.system76.CosmicAppletNetwork\",\n    \"com.system76.CosmicAppletNetwork\",\n], [\n    \"com.system76.CosmicAppList\",\n]))",
            ),
            ("plugins_wings.before-agent-hub-v2", "Some(([], []))"),
        ] {
            fixture.user(DOCK, key, value);
        }
        fixture
    }

    #[test]
    fn upstream_shapes_round_trip_in_cosmic_formatting() {
        assert_eq!(
            RawValue::encode(&Anchor::Bottom).unwrap().as_str(),
            "Bottom"
        );
        assert_eq!(
            RawValue::encode(&PanelSize::Custom(40)).unwrap().as_str(),
            "Custom(40)"
        );
        assert_eq!(
            RawValue::encode(&AutoHide::OnOverlap).unwrap().as_str(),
            "OnOverlap"
        );
        assert_eq!(
            RawValue::encode(&Output::Name("DP-1".into()))
                .unwrap()
                .as_str(),
            "Name(\"DP-1\")"
        );
        assert_eq!(RawValue::encode(&0.25f32).unwrap().as_str(), "0.25");
        assert_eq!(
            RawValue::encode(&AutoHideBehavior::default()).unwrap(),
            RawValue::parse(
                "(\n    wait_time: 1000,\n    transition_time: 200,\n    handle_size: 4,\n    unhide_delay: 200,\n)"
            )
            .unwrap()
        );
        let wings: Option<(Vec<String>, Vec<String>)> =
            RawValue::parse("Some(([\"a\", \"b\"],[\"c\"]))")
                .unwrap()
                .decode()
                .unwrap();
        assert_eq!(
            wings,
            Some((vec!["a".into(), "b".into()], vec!["c".into()]))
        );
        assert_eq!(
            RawValue::parse("Color((0.1, 0.2, 0.3))")
                .unwrap()
                .decode::<Background>(),
            Ok(Background::Color([0.1, 0.2, 0.3]))
        );
        // Every rule accepts its own upstream default.
        for rule in &RULES {
            let value = upstream_default(rule.key).unwrap();
            assert_eq!((rule.validate)(&value), Ok(()), "{}", rule.key);
        }
    }

    #[test]
    fn effective_values_combine_user_system_and_upstream_defaults() {
        let fixture = live_like("effective");
        let draft = PanelDraft::load(&fixture.store(), PanelKind::Dock).unwrap();
        assert!(draft.listed());
        assert!(draft.installed());
        assert_eq!(draft.anchor(), Setting::Known(Anchor::Bottom));
        assert_eq!(draft.size(), Setting::Known(PanelSize::S));
        assert_eq!(draft.autohide(), Setting::Known(AutoHide::OnOverlap));
        assert_eq!(draft.floating(), Setting::Known(true));
        assert_eq!(draft.extend(), Setting::Known(false));
        assert_eq!(draft.radius(), Setting::Known(160));
        assert_eq!(draft.opacity(), Setting::Known(0.25));
        assert_eq!(draft.background(), Setting::Known(Background::Dark));
        assert_eq!(draft.output(), Setting::Known(Output::All));
        assert_eq!(
            draft.behavior(),
            Setting::Known(AutoHideBehavior::default())
        );
        // Not defined anywhere: cosmic-panel uses its struct default.
        assert_eq!(draft.keep_style(), Setting::Known(false));
        let Setting::Known(regions) = draft.applets() else {
            panic!("applets should decode");
        };
        assert_eq!(
            regions.center,
            ["io.github.workstation.PersonalQuotaMonitor"]
        );
        assert_eq!(regions.start.len(), 3);
        assert_eq!(regions.end, ["com.system76.CosmicAppList"]);
        assert!(!draft.dirty());
        assert!(draft.patch().is_empty());

        let panel = PanelDraft::load(&fixture.store(), PanelKind::Panel).unwrap();
        assert!(panel.listed());
        assert!(!panel.installed());
        fixture.user(NAMESPACE, "entries", "[\"Panel\"]");
        let dock = PanelDraft::load(&fixture.store(), PanelKind::Dock).unwrap();
        assert!(!dock.listed());
    }

    #[test]
    fn controls_touch_only_their_keys_and_reverting_clears_the_edit() {
        let fixture = live_like("controls");
        let mut draft = PanelDraft::load(&fixture.store(), PanelKind::Dock).unwrap();
        draft.set_anchor(Anchor::Left);
        draft.set_opacity_percent(60);
        let names: Vec<_> = draft.edited().collect();
        assert_eq!(names, ["anchor", "opacity"]);
        draft.set_anchor(Anchor::Bottom);
        draft.set_opacity_percent(25);
        assert!(!draft.dirty());
        // Coupled keys follow cosmic-settings.
        draft.set_autohide(AutoHide::Never);
        assert_eq!(
            draft.edited().collect::<Vec<_>>(),
            ["autohide", "exclusive_zone"]
        );
        draft.set_floating(false);
        assert_eq!(draft.margin(), Setting::Known(0));
        draft.set_floating(true);
        assert_eq!(draft.margin(), Setting::Known(4));
        assert!(
            !draft
                .edited()
                .any(|name| name == "anchor_gap" || name == "margin")
        );
        draft.set_timing(Timing::Wait, 99_999);
        assert_eq!(draft.behavior().known().unwrap().wait_time, 5000);
        draft.set_size(PanelSize::XL);
        // Region overrides are already None: size alone is written.
        assert!(
            draft
                .edited()
                .all(|name| name != "size_wings" && name != "size_center")
        );
        let patch = draft.patch();
        let keys: Vec<_> = patch.keys().map(|key| key.key().to_string()).collect();
        assert_eq!(
            keys,
            ["autohide", "autohide_behavior", "exclusive_zone", "size"]
        );
        // Originals carry user presence: absent system-only keys stay absent.
        let behavior = patch
            .get(&ConfigKey::new(DOCK, 1, "autohide_behavior").unwrap())
            .unwrap();
        assert_eq!(behavior.original, None);
        let store = fixture.store();
        store.apply(&patch).unwrap();
        assert_eq!(fixture.read(DOCK, "size").as_deref(), Some("XL"));
        assert_eq!(
            fixture.read(DOCK, "exclusive_zone").as_deref(),
            Some("true")
        );
        assert_eq!(fixture.read(DOCK, "opacity").as_deref(), Some("0.25"));
        assert_eq!(
            fixture
                .read(DOCK, "plugins_wings.before-agent-hub-v2")
                .as_deref(),
            Some("Some(([], []))")
        );
    }

    #[test]
    fn unknown_values_are_preserved_until_explicitly_changed() {
        let fixture = live_like("unknown");
        fixture.user(DOCK, "anchor", "Diagonal");
        fixture.user(DOCK, "size", "Custom(44)");
        fixture.user(
            DOCK,
            "autohide",
            "Some((wait_time: 1, transition_time: 1, handle_size: 1, unhide_delay: 1))",
        );
        fixture.user(DOCK, "output", "Active");
        fixture.user(DOCK, "background", "Color((0.1, 0.2, 0.3))");
        let mut draft = PanelDraft::load(&fixture.store(), PanelKind::Dock).unwrap();
        assert_eq!(draft.anchor(), Setting::Unsupported("Diagonal".into()));
        assert_eq!(draft.size(), Setting::Known(PanelSize::Custom(44)));
        assert!(matches!(draft.autohide(), Setting::Unsupported(_)));
        assert_eq!(draft.output(), Setting::Known(Output::Active));
        assert_eq!(
            draft.background(),
            Setting::Known(Background::Color([0.1, 0.2, 0.3]))
        );
        draft.set_opacity_percent(50);
        let store = fixture.store();
        store.apply(&draft.patch()).unwrap();
        for (key, value) in [
            ("anchor", "Diagonal"),
            ("size", "Custom(44)"),
            ("output", "Active"),
            ("background", "Color((0.1, 0.2, 0.3))"),
        ] {
            assert_eq!(fixture.read(DOCK, key).as_deref(), Some(value));
        }
        draft.rebase(store.load(&PanelDraft::keys(PanelKind::Dock)).unwrap());
        draft.set_anchor(Anchor::Top);
        assert_eq!(draft.anchor(), Setting::Known(Anchor::Top));
        assert_eq!(draft.edited().collect::<Vec<_>>(), ["anchor"]);
    }

    #[test]
    fn applet_regions_keep_duplicates_and_write_only_changed_regions() {
        let fixture = live_like("applets");
        let store = fixture.store();
        let mut draft = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        let Setting::Known(mut regions) = draft.applets() else {
            panic!()
        };
        // Reordering within Start rewrites only plugins_wings and keeps the
        // duplicate occurrence that was not moved.
        let moved = regions
            .move_item(
                Slot {
                    region: Region::Start,
                    index: 0,
                },
                Slot {
                    region: Region::Start,
                    index: 2,
                },
            )
            .unwrap();
        assert_eq!(
            moved,
            Slot {
                region: Region::Start,
                index: 2
            }
        );
        assert_eq!(
            regions.start,
            [
                "com.system76.CosmicAppletNetwork",
                "com.system76.CosmicAppletNetwork",
                "io.github.cosmic_utils.minimon-applet"
            ]
        );
        draft.set_applets(&regions);
        assert_eq!(draft.edited().collect::<Vec<_>>(), ["plugins_wings"]);
        // Moving it back clears the edit, despite different formatting on disk.
        regions.move_item(
            moved,
            Slot {
                region: Region::Start,
                index: 0,
            },
        );
        draft.set_applets(&regions);
        assert!(!draft.dirty());
        // A cross-region move writes both region keys.
        regions.move_item(
            Slot {
                region: Region::End,
                index: 0,
            },
            Slot {
                region: Region::Center,
                index: 0,
            },
        );
        draft.set_applets(&regions);
        assert_eq!(
            draft.edited().collect::<Vec<_>>(),
            ["plugins_center", "plugins_wings"]
        );
        store.apply(&draft.patch()).unwrap();
        let center: Option<Vec<String>> =
            RawValue::parse(fixture.read(DOCK, "plugins_center").unwrap())
                .unwrap()
                .decode()
                .unwrap();
        assert_eq!(
            center.unwrap(),
            [
                "com.system76.CosmicAppList",
                "io.github.workstation.PersonalQuotaMonitor"
            ]
        );
    }

    #[test]
    fn region_moves_preserve_order_and_cancelled_drags_are_no_ops() {
        let ids = |items: &[&str]| {
            items
                .iter()
                .map(|item| item.to_string())
                .collect::<Vec<_>>()
        };
        let original = Regions {
            start: ids(&["a", "b", "b"]),
            center: ids(&["c"]),
            end: ids(&["d", "e"]),
        };
        let mut regions = original.clone();
        let mut drag = AppletDrag::default();
        drag.start(Slot {
            region: Region::Start,
            index: 1,
        });
        drag.over(Slot {
            region: Region::End,
            index: 1,
        });
        drag.cancel();
        assert_eq!(drag.finish(&mut regions), None);
        drag.start(Slot {
            region: Region::Start,
            index: 1,
        });
        drag.over(Slot {
            region: Region::End,
            index: 1,
        });
        drag.leave(Slot {
            region: Region::End,
            index: 1,
        });
        assert_eq!(drag.finish(&mut regions), None);
        drag.start(Slot {
            region: Region::Start,
            index: 0,
        });
        assert_eq!(drag.finish(&mut regions), None);
        assert_eq!(regions, original);
        // Drop onto an empty-region target appends.
        drag.start(Slot {
            region: Region::Start,
            index: 2,
        });
        drag.over(Slot {
            region: Region::End,
            index: 2,
        });
        assert_eq!(
            drag.finish(&mut regions),
            Some(Slot {
                region: Region::End,
                index: 2
            })
        );
        assert_eq!(regions.start, ids(&["a", "b"]));
        assert_eq!(regions.end, ids(&["d", "e", "b"]));
        // Keyboard: stepping crosses regions and keeps everyone else in order.
        let mut regions = original.clone();
        let slot = regions
            .step(
                Slot {
                    region: Region::Center,
                    index: 0,
                },
                -1,
            )
            .unwrap();
        assert_eq!(
            slot,
            Slot {
                region: Region::Start,
                index: 3
            }
        );
        assert_eq!(regions.start, ids(&["a", "b", "b", "c"]));
        assert!(regions.center.is_empty());
        let slot = regions.step(slot, 1).unwrap();
        assert_eq!(
            slot,
            Slot {
                region: Region::Center,
                index: 0
            }
        );
        let slot = regions.step(slot, 1).unwrap();
        assert_eq!(
            slot,
            Slot {
                region: Region::End,
                index: 0
            }
        );
        assert_eq!(regions.end, ids(&["c", "d", "e"]));
        assert_eq!(
            regions.step(
                Slot {
                    region: Region::Start,
                    index: 0
                },
                -1
            ),
            None
        );
        assert_eq!(
            regions.step(
                Slot {
                    region: Region::End,
                    index: 2
                },
                1
            ),
            None
        );
        assert_eq!(
            regions.step(
                Slot {
                    region: Region::End,
                    index: 9
                },
                1
            ),
            None
        );
        assert_eq!(
            regions
                .remove(Slot {
                    region: Region::End,
                    index: 0
                })
                .as_deref(),
            Some("c")
        );
        assert_eq!(
            regions.add(Region::Center, "x"),
            Slot {
                region: Region::Center,
                index: 0
            }
        );
    }

    #[test]
    fn refresh_keeps_conflict_detection_and_resolving_keeps_other_edits() {
        let fixture = live_like("refresh");
        let store = fixture.store();
        let mut draft = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        draft.set_size(PanelSize::M);
        draft.set_anchor(Anchor::Top);
        let Setting::Known(mut regions) = draft.applets() else {
            panic!()
        };
        regions.move_item(
            Slot {
                region: Region::End,
                index: 0,
            },
            Slot {
                region: Region::Center,
                index: 1,
            },
        );
        draft.set_applets(&regions);
        // Native settings change one edited key and one untouched key.
        fixture.user(DOCK, "size", "XL");
        fixture.user(DOCK, "opacity", "0.5");
        fixture.user(DOCK, "plugins_center", "Some([])");
        draft.refresh(store.load(&PanelDraft::keys(PanelKind::Dock)).unwrap());
        assert_eq!(draft.opacity(), Setting::Known(0.5));
        assert_eq!(draft.size(), Setting::Known(PanelSize::M));
        assert_eq!(draft.conflicts(), ["plugins_center", "size"]);
        let error = store.apply(&draft.patch()).unwrap_err();
        let DesktopError::Conflict(keys) = error else {
            panic!("{error:?}")
        };
        assert_eq!(keys.len(), 2);
        let snapshot = store.load(&PanelDraft::keys(PanelKind::Dock)).unwrap();
        draft.resolve_conflicts(&snapshot, &keys);
        // The applet move is dropped as a group; the anchor edit survives.
        assert_eq!(draft.edited().collect::<Vec<_>>(), ["anchor"]);
        assert_eq!(draft.size(), Setting::Known(PanelSize::XL));
        store.apply(&draft.patch()).unwrap();
        assert_eq!(fixture.read(DOCK, "anchor").as_deref(), Some("Top"));
        assert_eq!(
            fixture.read(DOCK, "plugins_center").as_deref(),
            Some("Some([])")
        );
    }

    /// R1 M-3: hiding is written as a coupled pair. When the reserved-space
    /// key was already false, choosing another hidden mode writes only
    /// `autohide`; an outside change to `exclusive_zone` must not be adopted
    /// into a hidden panel that still reserves space.
    #[test]
    fn coupled_peer_changed_outside_is_a_conflict_and_other_groups_survive() {
        let fixture = live_like("coupled");
        fixture.user(DOCK, "autohide", "Always");
        let store = fixture.store();
        let mut draft = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        draft.set_autohide(AutoHide::OnOverlap);
        draft.set_opacity_percent(50);
        assert_eq!(draft.edited().collect::<Vec<_>>(), ["autohide", "opacity"]);
        // The peer is not written but is checked at Apply.
        let patch = draft.patch();
        assert_eq!(patch.len(), 2);
        assert!(
            patch
                .guards()
                .any(|(key, value)| key.key() == "exclusive_zone"
                    && value.as_ref().map(RawValue::as_str) == Some("false"))
        );
        // Outside: the peer changes, and so does an unrelated group.
        fixture.user(DOCK, "exclusive_zone", "true");
        fixture.user(DOCK, "margin", "9");
        draft.refresh(store.load(&PanelDraft::keys(PanelKind::Dock)).unwrap());
        assert_eq!(draft.conflicts(), ["exclusive_zone"]);
        assert_eq!(
            draft.margin(),
            Setting::Known(9),
            "unrelated change adopted"
        );
        assert_eq!(
            draft.get::<bool>("exclusive_zone"),
            Setting::Known(false),
            "the preview keeps a coherent pair"
        );
        let before = fixture.tree();
        let error = store.apply(&draft.patch()).unwrap_err();
        let DesktopError::Conflict(keys) = error else {
            panic!("{error:?}")
        };
        assert_eq!(
            keys.iter().map(ConfigKey::key).collect::<Vec<_>>(),
            ["exclusive_zone"]
        );
        let unchanged = |tree: &std::collections::BTreeMap<std::path::PathBuf, Option<Vec<u8>>>| {
            tree.iter()
                .filter(|(path, _)| !path.starts_with(fixture.0.join("state")))
                .map(|(path, value)| (path.clone(), value.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(unchanged(&fixture.tree()), unchanged(&before));
        // Using the current value drops the hiding pair only.
        let snapshot = store.load(&PanelDraft::keys(PanelKind::Dock)).unwrap();
        draft.resolve_conflicts(&snapshot, &[]);
        assert_eq!(draft.edited().collect::<Vec<_>>(), ["opacity"]);
        assert_eq!(draft.autohide(), Setting::Known(AutoHide::Always));
        store.apply(&draft.patch()).unwrap();
        assert_eq!(fixture.read(DOCK, "opacity").as_deref(), Some("0.5"));
        assert_eq!(fixture.read(DOCK, "autohide").as_deref(), Some("Always"));
        assert_eq!(
            fixture.read(DOCK, "exclusive_zone").as_deref(),
            Some("true")
        );
        assert_eq!(fixture.read(DOCK, "margin").as_deref(), Some("9"));
    }

    #[test]
    fn connected_outputs_come_from_drm_status() {
        let fixture = Fixture::new("drm");
        for (name, status) in [
            ("card1-DP-1", "connected\n"),
            ("card1-HDMI-A-1", "disconnected\n"),
            ("card0-eDP-1", "connected\n"),
            ("renderD128", "connected\n"),
        ] {
            std::fs::create_dir_all(fixture.0.join(name)).unwrap();
            std::fs::write(fixture.0.join(name).join("status"), status).unwrap();
        }
        assert_eq!(connected_outputs(&fixture.0), ["DP-1", "eDP-1"]);
        assert!(connected_outputs(Path::new("/nonexistent")).is_empty());
    }
}
