//! The Halcyon widget configuration as a transaction participant.
//!
//! Only a fixed allowlist of JSON members may be written, each addressed by
//! a JSON pointer from the table below and checked by its own validator. The
//! rest of `config.json` (location, project roots, accounts, intervals,
//! palette links and any member this version does not know) is read and
//! written back untouched. Nothing here names a file: the configuration path
//! always comes from the store's trusted context.
use crate::config::{MAX_CONFIG_BYTES, Palette};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Which profile domain a member belongs to. Every member has exactly one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WidgetGroup {
    /// Palette, card look and per-widget display options.
    Style,
    /// Visibility, order, positions and snap.
    Layout,
}

/// One writable configuration member.
#[derive(Debug)]
pub struct WidgetField {
    pub pointer: &'static str,
    pub label: &'static str,
    pub group: WidgetGroup,
    check: fn(&Value) -> bool,
}

impl WidgetField {
    /// Whether `value` is an acceptable new value for this member.
    pub fn accepts(&self, value: &Value) -> bool {
        (self.check)(value)
    }
}

const WIDGETS: [&str; 6] = [
    "calendar", "projects", "devbox", "weather", "clock", "glance",
];
const DEVBOX_SECTIONS: [&str; 7] = [
    "cpu", "memory", "disk", "network", "gpu", "uptime", "services",
];
/// Bound on any single stored member value, serialized.
const MAX_VALUE_BYTES: usize = 16 * 1024;

/// A font family name, or `null` for the widget style's own.
fn font_family(value: &Value) -> bool {
    value.is_null()
        || value.as_str().is_some_and(|name| {
            !name.trim().is_empty() && name.len() <= 128 && !name.chars().any(char::is_control)
        })
}
fn monitor_layout(value: &Value) -> bool {
    let Ok(layouts) = serde_json::from_value::<
        BTreeMap<String, crate::config::monitors::MonitorLayout>,
    >(value.clone()) else {
        return false;
    };
    layouts.len() <= 32
        && layouts.iter().all(|(d, l)| {
            crate::displays::valid_name(d)
                && l.position
                    .is_none_or(|p| p.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)))
                && l.size.is_none_or(crate::layout::valid_size)
        })
}
fn boolean(value: &Value) -> bool {
    value.is_boolean()
}
fn int_in(value: &Value, min: u64, max: u64) -> bool {
    value.as_u64().is_some_and(|n| (min..=max).contains(&n))
}
fn card_opacity(value: &Value) -> bool {
    int_in(value, 70, 100)
}
fn card_corners(value: &Value) -> bool {
    int_in(value, 0, 32)
}
fn card_outline(value: &Value) -> bool {
    int_in(value, 0, 50)
}
fn percent(value: &Value) -> bool {
    int_in(value, 0, 100)
}
fn palette(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|name| Palette::CHOICES.iter().any(|(key, _)| *key == name))
}
fn accent(value: &Value) -> bool {
    value.is_null() || serde_json::from_value::<[u8; 3]>(value.clone()).is_ok()
}
fn color_source(value: &Value) -> bool {
    matches!(value.as_str(), Some("custom" | "desktop"))
}
fn glance_style(value: &Value) -> bool {
    matches!(value.as_str(), Some("transparent" | "panel"))
}
fn notes_size(value: &Value) -> bool {
    matches!(value.as_str(), Some("small" | "medium" | "large"))
}
fn glance_density(value: &Value) -> bool {
    matches!(value.as_str(), Some("compact" | "comfortable"))
}
fn clock_alignment(value: &Value) -> bool {
    matches!(value.as_str(), Some("left" | "center" | "right"))
}
fn clock_scale(value: &Value) -> bool {
    int_in(value, 50, 200)
}
fn unique_keys(value: &Value, allowed: &[&str]) -> bool {
    let Some(items) = value.as_array() else {
        return false;
    };
    let mut seen: Vec<&str> = Vec::new();
    items.len() <= allowed.len()
        && items.iter().all(|item| match item.as_str() {
            Some(key) if allowed.contains(&key) && !seen.contains(&key) => {
                seen.push(key);
                true
            }
            _ => false,
        })
}
fn widget_list(value: &Value) -> bool {
    unique_keys(value, &WIDGETS)
}
fn devbox_order(value: &Value) -> bool {
    unique_keys(value, &DEVBOX_SECTIONS)
}
fn position(value: &Value) -> bool {
    value.as_array().is_some_and(|items| {
        items.len() == 2
            && items.iter().all(|item| {
                item.as_f64()
                    .is_some_and(|n| n.is_finite() && (0.0..=1.0).contains(&n))
            })
    })
}

fn size(value: &Value) -> bool {
    serde_json::from_value::<[f32; 2]>(value.clone()).is_ok_and(crate::layout::valid_size)
}

macro_rules! fields {
    ($($pointer:literal, $label:literal, $group:ident, $check:expr;)+) => {
        [$(WidgetField {
            pointer: $pointer,
            label: $label,
            group: WidgetGroup::$group,
            check: $check,
        }),+]
    };
}

/// Every member a profile, its Undo or its recovery may write.
pub static FIELDS: [WidgetField; 104] = fields![
    "/recipes/devbox", "System recipe", Style, |v| crate::recipes::accepts(v, crate::recipes::Kind::System);
    "/recipes/calendar", "Calendar recipe", Style, |v| crate::recipes::accepts(v, crate::recipes::Kind::Calendar);
    "/recipes/clock", "Clock recipe", Style, |v| crate::recipes::accepts(v, crate::recipes::Kind::Clock);
    "/recipes/weather", "Weather recipe", Style, |v| crate::recipes::accepts(v, crate::recipes::Kind::Weather);
    "/moments/active", "Active moment", Style, |v| v.is_null() || matches!(v.as_str(), Some("day" | "night"));
    "/moments/day/family", "Day widget style", Style, |v| serde_json::from_value::<Option<crate::desktop_theme::Family>>(v.clone()).is_ok();
    "/moments/day/clock/face", "Day clock face", Style, |v| v.is_null() || matches!(v.as_str(), Some("digital" | "analog"));
    "/moments/day/clock/weight", "Day clock weight", Style, |v| v.is_null() || matches!(v.as_str(), Some("light" | "normal" | "bold"));
    "/moments/day/clock/italic", "Day clock italic", Style, |v| v.is_null() || v.is_boolean();
    "/moments/day/clock/surface", "Day clock surface", Style, |v| v.is_null() || matches!(v.as_str(), Some("none" | "panel"));
    "/moments/night/family", "Night widget style", Style, |v| serde_json::from_value::<Option<crate::desktop_theme::Family>>(v.clone()).is_ok();
    "/moments/night/clock/face", "Night clock face", Style, |v| v.is_null() || matches!(v.as_str(), Some("digital" | "analog"));
    "/moments/night/clock/weight", "Night clock weight", Style, |v| v.is_null() || matches!(v.as_str(), Some("light" | "normal" | "bold"));
    "/moments/night/clock/italic", "Night clock italic", Style, |v| v.is_null() || v.is_boolean();
    "/moments/night/clock/surface", "Night clock surface", Style, |v| v.is_null() || matches!(v.as_str(), Some("none" | "panel"));
    "/material_motion", "Liquid widget motion", Style, boolean;
    "/materials/shared", "Shared material", Style, material;
    "/materials/widgets", "Widget material", Style, material;
    "/materials/dock", "Dock material", Style, material;
    "/materials/panel", "Panel material", Style, material;
    "/appearance/color_policy", "Color source", Style, |v| v.is_null() || matches!(v.as_str(), Some("wallpaper" | "theme" | "custom"));
    "/appearance/overrides", "Custom properties", Style, |v| v.as_u64().is_some();

    "/theme", "Card palette", Style, palette;
    "/appearance/native_mode_palettes", "Native mode palettes", Style, |v| matches!(v.as_u64(), Some(0 | 1));
    "/appearance/family", "Theme family", Style, |v| serde_json::from_value::<Option<crate::desktop_theme::Family>>(v.clone()).is_ok();
    "/appearance/surface", "Widget surface", Style, |v| serde_json::from_value::<Option<crate::desktop_theme::Surface>>(v.clone()).is_ok();
    "/appearance/wallpaper_light", "Wallpaper text contrast", Style, boolean;
    "/appearance/color_source", "Widget colors", Style, color_source;
    "/appearance/accent", "Chosen color", Style, accent;
    "/appearance/sync_colors", "Color apply scope", Style, boolean;
    "/appearance/wallpaper_colors", "Follow wallpaper colors", Style, boolean;
    "/appearance/opacity", "Card opacity", Style, card_opacity;
    "/appearance/corner_radius", "Card corners", Style, card_corners;
    "/appearance/border_opacity", "Card outlines", Style, card_outline;
    "/appearance/contrast_assist", "Widget readability", Style, boolean;
    "/clock/format_24h", "Clock 24-hour time", Style, boolean;
    "/clock/show_date", "Clock date", Style, boolean;
    "/clock/alignment", "Clock alignment", Style, clock_alignment;
    "/clock/scale", "Clock text size", Style, clock_scale;
    "/clock/depth", "Clock depth", Style, boolean;
    "/clock/face", "Clock face", Style, |v| v.is_null() || matches!(v.as_str(), Some("digital" | "analog"));
    "/clock/weight", "Clock weight", Style, |v| v.is_null() || matches!(v.as_str(), Some("light" | "normal" | "bold"));
    "/clock/italic", "Clock italic", Style, |v| v.is_null() || v.is_boolean();
    "/clock/surface", "Clock surface", Style, |v| v.is_null() || matches!(v.as_str(), Some("none" | "panel"));
    "/widget_font", "Widget font", Style, font_family;
    "/clock_font", "Clock font", Style, font_family;
    "/clock/center_vertically", "Clock vertical alignment", Style, boolean;
    "/glance/style", "Glance style", Style, glance_style;
    "/glance/density", "Glance density", Style, glance_density;
    "/glance/show_tomorrow", "Glance tomorrow", Style, boolean;
    "/glance/opacity", "Glance opacity", Style, percent;
    "/glance/own_background", "Glance own background", Style, boolean;
    "/appearance/style_bars", "Style the dock and panel", Style, boolean;
    "/glance/font", "Glance font", Style, font_family;
    "/glance/clock_font", "Glance clock font", Style, font_family;
    "/devbox/cpu", "Devbox CPU", Style, boolean;
    "/devbox/memory", "Devbox memory", Style, boolean;
    "/devbox/memory_cache", "Devbox memory cache", Style, boolean;
    "/devbox/swap", "Devbox swap", Style, boolean;
    "/devbox/disk", "Devbox disk", Style, boolean;
    "/devbox/network", "Devbox network", Style, boolean;
    "/devbox/gpu", "Devbox GPU", Style, boolean;
    "/devbox/uptime", "Devbox uptime", Style, boolean;
    "/devbox/services", "Devbox services", Style, boolean;
    "/devbox/order", "Devbox section order", Style, devbox_order;
    "/devbox/history/cpu", "Devbox CPU history", Style, boolean;
    "/devbox/history/memory", "Devbox memory history", Style, boolean;
    "/devbox/history/disk", "Devbox disk history", Style, boolean;
    "/devbox/history/network", "Devbox network history", Style, boolean;
    "/devbox/history/gpu", "Devbox GPU history", Style, boolean;
    "/calendar_show_today", "Calendar today", Style, boolean;
    "/calendar_show_tomorrow", "Calendar tomorrow", Style, boolean;
    "/music/show_artwork", "Music artwork", Style, boolean;
    "/music/show_progress", "Music progress", Style, boolean;
    "/notes/size", "Notes size", Style, notes_size;
    "/enabled_widgets", "Visible widgets", Layout, widget_list;
    "/widget_order", "Widget order", Layout, widget_list;
    "/widget_positions/calendar", "Calendar position", Layout, position;
    "/widget_positions/projects", "Projects position", Layout, position;
    "/widget_positions/devbox", "Devbox position", Layout, position;
    "/widget_positions/weather", "Weather position", Layout, position;
    "/widget_positions/clock", "Clock position", Layout, position;
    "/widget_positions/glance", "Glance position", Layout, position;
    "/placement/snap", "Snap to grid", Layout, boolean;
    "/glance/enabled", "Glance", Layout, boolean;
    "/glance/initialized", "Glance sections set up", Layout, boolean;
    "/glance/show_clock", "Glance clock", Layout, boolean;
    "/glance/show_weather", "Glance weather", Layout, boolean;
    "/glance/show_calendar", "Glance calendar", Layout, boolean;
    "/music/enabled", "Music", Layout, boolean;
    "/notes/enabled", "Notes", Layout, boolean;
    "/focus/enabled", "Focus timer", Layout, boolean;
    "/widget_positions/music", "Music position", Layout, position;
    "/widget_positions/notes", "Notes position", Layout, position;
    "/widget_positions/focus", "Focus position", Layout, position;
    "/widget_sizes/clock", "Clock size", Layout, size;
    "/widget_sizes/weather", "Weather size", Layout, size;
    "/widget_sizes/calendar", "Calendar size", Layout, size;
    "/widget_sizes/devbox", "Devbox size", Layout, size;
    "/widget_sizes/projects", "Projects size", Layout, size;
    "/widget_sizes/glance", "Glance size", Layout, size;
    "/widget_sizes/music", "Music size", Layout, size;
    "/widget_sizes/notes", "Notes size", Layout, size;
    "/widget_sizes/focus", "Focus size", Layout, size;
];

// Local preferences participate in transactions and Undo but are never
// captured or exported by the profile library.
pub static LOCAL_FIELDS: &[WidgetField] = &fields![
    "/calendar_private", "Calendar privacy", Style, boolean;
    "/widget_monitors/clock/display", "Widget display", Layout, |v| v.is_null() || v.as_str().is_some_and(crate::displays::valid_name);
    "/widget_monitors/clock/hide_disconnected", "Disconnected widget", Layout, boolean;
    "/monitor_layouts/clock", "Display layout", Layout, monitor_layout;
    "/widget_monitors/calendar/display", "Widget display", Layout, |v| v.is_null() || v.as_str().is_some_and(crate::displays::valid_name);
    "/widget_monitors/calendar/hide_disconnected", "Disconnected widget", Layout, boolean;
    "/monitor_layouts/calendar", "Display layout", Layout, monitor_layout;
    "/widget_monitors/weather/display", "Widget display", Layout, |v| v.is_null() || v.as_str().is_some_and(crate::displays::valid_name);
    "/widget_monitors/weather/hide_disconnected", "Disconnected widget", Layout, boolean;
    "/monitor_layouts/weather", "Display layout", Layout, monitor_layout;
    "/widget_monitors/devbox/display", "Widget display", Layout, |v| v.is_null() || v.as_str().is_some_and(crate::displays::valid_name);
    "/widget_monitors/devbox/hide_disconnected", "Disconnected widget", Layout, boolean;
    "/monitor_layouts/devbox", "Display layout", Layout, monitor_layout;
    "/widget_monitors/projects/display", "Widget display", Layout, |v| v.is_null() || v.as_str().is_some_and(crate::displays::valid_name);
    "/widget_monitors/projects/hide_disconnected", "Disconnected widget", Layout, boolean;
    "/monitor_layouts/projects", "Display layout", Layout, monitor_layout;
    "/widget_monitors/glance/display", "Widget display", Layout, |v| v.is_null() || v.as_str().is_some_and(crate::displays::valid_name);
    "/widget_monitors/glance/hide_disconnected", "Disconnected widget", Layout, boolean;
    "/monitor_layouts/glance", "Display layout", Layout, monitor_layout;
    "/widget_monitors/music/display", "Widget display", Layout, |v| v.is_null() || v.as_str().is_some_and(crate::displays::valid_name);
    "/widget_monitors/music/hide_disconnected", "Disconnected widget", Layout, boolean;
    "/monitor_layouts/music", "Display layout", Layout, monitor_layout;
    "/widget_monitors/notes/display", "Widget display", Layout, |v| v.is_null() || v.as_str().is_some_and(crate::displays::valid_name);
    "/widget_monitors/notes/hide_disconnected", "Disconnected widget", Layout, boolean;
    "/monitor_layouts/notes", "Display layout", Layout, monitor_layout;
    "/widget_monitors/focus/display", "Widget display", Layout, |v| v.is_null() || v.as_str().is_some_and(crate::displays::valid_name);
    "/widget_monitors/focus/hide_disconnected", "Disconnected widget", Layout, boolean;
    "/monitor_layouts/focus", "Display layout", Layout, monitor_layout;
    "/widget_display", "Widget display", Layout, |v| v.is_null() || v.as_str().is_some_and(crate::displays::valid_name);
    "/appearance/match_wallpaper", "Matching wallpaper", Style, boolean;
    "/appearance/match_cosmic_colors", "Follow COSMIC colors", Style, boolean;
    "/weather/show_dew_point", "Weather dew point", Style, boolean;
    "/placement/columns", "Arrangement grid", Layout, |v| int_in(v,12,24);
    "/widgets_visible", "Show widgets", Layout, boolean;
    "/location", "Weather city", Style, |v| v.is_null() || serde_json::from_value::<crate::config::Location>(v.clone()).is_ok();
    "/project_roots", "Project folders", Style, |v| v.as_array().is_some_and(|a| a.len() <= 128 && a.iter().all(Value::is_string));
    "/refresh_intervals/devbox", "Monitor refresh", Style, |v| int_in(v,1,60);
    "/refresh_intervals/clock", "Clock refresh", Style, |v| int_in(v,1,60);
    "/refresh_intervals/weather", "Weather refresh", Style, |v| int_in(v,300,10800);
    "/refresh_intervals/calendar", "Calendar refresh", Style, |v| int_in(v,60,3600);
    "/refresh_intervals/projects", "Projects refresh", Style, |v| int_in(v,10,600);
    "/meeting_notice_minutes", "Meeting notice", Style, |v| int_in(v,1,60);
    "/sidebar_slide", "Sidebar animation", Style, boolean;
    "/settings_animations", "Settings animation", Style, boolean;
    "/setup_dismissed", "Setup guide", Style, boolean;
    "/setup_followup_weather_dismissed", "Weather setup reminder", Style, boolean;
    "/setup_followup_calendar_dismissed", "Calendar setup reminder", Style, boolean;
    "/setup_followup_projects_dismissed", "Projects setup reminder", Style, boolean;
    "/onboarded", "First-run setup", Style, boolean;
    "/music/player", "Music player", Style, |v| v.is_null() || v.as_str().is_some_and(|s| s.len() <= 256);
    "/focus/focus_minutes", "Focus duration", Style, |v| int_in(v,1,180);
    "/focus/break_minutes", "Break duration", Style, |v| int_in(v,1,60);
    "/focus/long_break_minutes", "Long break", Style, |v| int_in(v,1,90);
    "/focus/long_break_every", "Long break frequency", Style, |v| int_in(v,0,12);
    "/focus/auto_start_breaks", "Automatic breaks", Style, boolean;
    "/focus/notify", "Focus notifications", Style, boolean;
];

/// The desktop tool a member belongs to (`music`, `notes`, `focus`), if
/// any. Tool members are version 4 profile members, and are captured only
/// once that tool has been set up.
pub fn tool_of(pointer: &str) -> Option<&'static str> {
    let rest = pointer
        .strip_prefix("/widget_positions/")
        .or_else(|| pointer.strip_prefix("/widget_sizes/"))
        .or_else(|| pointer.strip_prefix('/'))?;
    let name = rest.split('/').next()?;
    ["music", "notes", "focus"].into_iter().find(|tool| {
        *tool == name
            && (rest.contains('/')
                || pointer.starts_with("/widget_positions/")
                || pointer.starts_with("/widget_sizes/"))
    })
}

/// The allowlisted member at `pointer`.
pub fn field(pointer: &str) -> Option<&'static WidgetField> {
    // The source stamp belongs to local transaction/Undo state. Profiles
    // may carry the mode, but never a machine-specific wallpaper version.
    static STAMP: WidgetField = WidgetField {
        pointer: "/appearance/wallpaper_color_stamp",
        label: "Wallpaper color source",
        group: WidgetGroup::Style,
        check: |value| value.as_u64().is_some(),
    };
    FIELDS
        .iter()
        .chain(LOCAL_FIELDS)
        .chain(std::iter::once(&STAMP))
        .find(|field| field.pointer == pointer)
}

/// Members of `group`, in table order.
pub fn fields(group: WidgetGroup) -> impl Iterator<Item = &'static WidgetField> {
    FIELDS.iter().filter(move |field| field.group == group)
}

fn segments(pointer: &str) -> impl Iterator<Item = &str> {
    pointer.split('/').skip(1)
}

/// The member at an allowlisted `pointer` (`None` when absent).
pub fn get(value: &Value, pointer: &str) -> Option<Value> {
    value.pointer(pointer).cloned()
}

/// Sets (or, with `None`, removes) the member at `pointer`, creating
/// missing parent objects. A parent that is not an object is an error, so
/// an unexpected shape is never replaced.
pub fn put(value: &mut Value, pointer: &str, member: Option<Value>) -> Result<(), String> {
    let parts: Vec<&str> = segments(pointer).collect();
    let (last, parents) = parts.split_last().ok_or("Empty setting name")?;
    let mut current = value;
    for part in parents {
        let object = current
            .as_object_mut()
            .ok_or_else(|| format!("{pointer}: a parent is not an object"))?;
        if !object.contains_key(*part) {
            if member.is_none() {
                return Ok(());
            }
            object.insert((*part).to_string(), Value::Object(Default::default()));
        }
        current = object.get_mut(*part).expect("inserted above");
    }
    let object = current
        .as_object_mut()
        .ok_or_else(|| format!("{pointer}: a parent is not an object"))?;
    match member {
        Some(member) => {
            object.insert((*last).to_string(), member);
        }
        None => {
            object.remove(*last);
        }
    }
    Ok(())
}

fn bounded(value: &Value) -> bool {
    serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= MAX_VALUE_BYTES)
}

/// The original and desired state of one member.
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetEntry {
    pub original: Option<Value>,
    pub desired: Option<Value>,
}

/// Touched widget members, compared and written like a `DesktopPatch`.
/// Entries whose desired value equals the original are guards: checked for
/// outside changes, never written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WidgetPatch {
    entries: BTreeMap<&'static str, WidgetEntry>,
    /// Undo only: when the file still holds exactly the first text, write
    /// the second (its bytes from before the change; `None` = no file).
    exact: Option<(Option<String>, Option<String>)>,
}

impl WidgetPatch {
    pub fn new() -> Self {
        Self::default()
    }
    /// Records `pointer`, which must be allowlisted. A new desired value
    /// must pass the member's validator; `None` restores absence.
    pub fn set(
        &mut self,
        pointer: &str,
        original: Option<Value>,
        desired: Option<Value>,
    ) -> Result<(), String> {
        let field = field(pointer).ok_or_else(|| format!("Halcyon does not edit {pointer}"))?;
        if let Some(value) = &desired
            && original.as_ref() != Some(value)
            && !field.accepts(value)
        {
            return Err(format!("Invalid value for {}", field.label));
        }
        for value in original.iter().chain(desired.iter()) {
            if !bounded(value) {
                return Err(format!("{} is too large", field.label));
            }
        }
        self.entries
            .insert(field.pointer, WidgetEntry { original, desired });
        Ok(())
    }
    pub(crate) fn with_exact(mut self, expected: Option<String>, restore: Option<String>) -> Self {
        self.exact = Some((expected, restore));
        self
    }
    pub(crate) fn forget_exact(&mut self) {
        self.exact = None;
    }
    pub(crate) fn exact(&self) -> Option<&(Option<String>, Option<String>)> {
        self.exact.as_ref()
    }
    /// Every entry, guards included.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &WidgetEntry)> {
        self.entries
            .iter()
            .map(|(pointer, entry)| (*pointer, entry))
    }
    /// Entries that write.
    pub fn changes(&self) -> impl Iterator<Item = (&'static str, &WidgetEntry)> {
        self.iter()
            .filter(|(_, entry)| entry.original != entry.desired)
    }
    /// Whether nothing would be written.
    pub fn is_empty(&self) -> bool {
        self.changes().next().is_none()
    }
    /// Combines patches from independent domains. Identical entries are
    /// kept once; anything else touching the same member is an overlap and
    /// leaves `self` unchanged.
    pub fn merge(&mut self, other: WidgetPatch) -> Result<(), Vec<String>> {
        let overlaps: Vec<String> = other
            .entries
            .iter()
            .filter(|(pointer, entry)| {
                self.entries
                    .get(*pointer)
                    .is_some_and(|mine| mine != *entry)
            })
            .map(|(pointer, _)| label(pointer))
            .collect();
        if !overlaps.is_empty() || (self.exact.is_some() && other.exact.is_some()) {
            return Err(overlaps);
        }
        self.entries.extend(other.entries);
        self.exact = self.exact.take().or(other.exact);
        Ok(())
    }
}

/// The user-facing name of an allowlisted member.
pub fn label(pointer: &str) -> String {
    field(pointer).map_or_else(|| pointer.to_string(), |field| field.label.to_string())
}

/// One member as a completed change wrote it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WidgetFieldChange {
    pub pointer: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub before: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub after: Option<Value>,
}

/// The widget part of a journal or Undo record: the whole file before and
/// after (for exact restoration) plus each member written.
fn present_value<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WidgetChange {
    /// New receipts target reserved instance identities through the adapter.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub design_v1: bool,
    pub before: Option<String>,
    pub after: Option<String>,
    pub fields: Vec<WidgetFieldChange>,
}

impl<'de> Deserialize<'de> for WidgetChange {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Record {
            #[serde(default)]
            design_v1: bool,
            before: Option<String>,
            after: Option<String>,
            fields: Vec<WidgetFieldChange>,
        }
        let mut record = Record::deserialize(deserializer)?;
        let before = crate::config::parse_config_text(record.before.as_deref())
            .map_err(serde::de::Error::custom)?;
        let after = crate::config::parse_config_text(record.after.as_deref())
            .map_err(serde::de::Error::custom)?;
        for field in &mut record.fields {
            // Version-one records encoded both absent and explicit null as
            // JSON null. Their exact file snapshots resolve that ambiguity.
            if field.before == Some(Value::Null) {
                field.before = get(&before, &field.pointer);
            }
            if field.after == Some(Value::Null) {
                field.after = get(&after, &field.pointer);
            }
        }
        Ok(Self {
            design_v1: record.design_v1,
            before: record.before,
            after: record.after,
            fields: record.fields,
        })
    }
}

impl WidgetChange {
    /// Refuses records that name members outside the allowlist or carry
    /// oversized text, so a damaged file can never address anything else.
    pub fn validate(&self) -> Result<(), String> {
        for text in self.before.iter().chain(self.after.iter()) {
            if text.len() > MAX_CONFIG_BYTES {
                return Err("Recorded widget settings are too large".into());
            }
        }
        if self.fields.len() > FIELDS.len() + LOCAL_FIELDS.len() + 1 {
            return Err("Too many recorded widget settings".into());
        }
        for change in &self.fields {
            if field(&change.pointer).is_none() {
                return Err(format!("Unsupported widget setting {}", change.pointer));
            }
            if change
                .before
                .iter()
                .chain(change.after.iter())
                .any(|value| !bounded(value))
            {
                return Err(format!("{} is too large", label(&change.pointer)));
            }
        }
        Ok(())
    }
    /// The patch that reverses this change, for Undo.
    pub(crate) fn reverse(&self) -> Result<WidgetPatch, String> {
        self.validate()?;
        let mut patch = WidgetPatch::new();
        for change in &self.fields {
            let field = field(&change.pointer).ok_or("Unsupported widget setting")?;
            // The original value is restored as it was, without the
            // new-value validator (it may predate this version's limits).
            patch.entries.insert(
                field.pointer,
                WidgetEntry {
                    original: change.after.clone(),
                    desired: change.before.clone(),
                },
            );
        }
        Ok(patch.with_exact(self.after.clone(), self.before.clone()))
    }
    pub fn labels(&self) -> Vec<String> {
        self.fields
            .iter()
            .map(|change| label(&change.pointer))
            .collect()
    }
}

fn material(value: &Value) -> bool {
    serde_json::from_value::<Option<crate::materials::Finish>>(value.clone())
        .is_ok_and(|f| f.is_none_or(|f| f.validate().is_ok()))
}

#[cfg(test)]
#[path = "widget_tests.rs"]
mod tests;
