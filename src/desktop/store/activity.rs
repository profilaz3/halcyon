//! Bounded, non-content history committed with the existing Undo transaction.
use super::{DesktopError, DesktopStore, Owner, Source, undo::UndoFile};
use serde::{Deserialize, Serialize};

pub const LIMIT: usize = 50;
const BYTES: usize = 64 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    You,
    Schedule,
    DisplayRule,
    WallpaperColors,
}
impl Cause {
    pub fn title(self) -> &'static str {
        match self {
            Self::You => "You",
            Self::Schedule => "Schedule",
            Self::DisplayRule => "Display rule",
            Self::WallpaperColors => "Wallpaper colors",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Changed,
    Undone,
    Kept,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Detail {
    pub setting: String,
    pub before: String,
    pub after: String,
}
impl Detail {
    /// "Between items: 5 → 9 px", with the name the control shows.
    pub fn line(&self) -> String {
        let unit = match self.setting.as_str() {
            "Bar spacing" | "Bar padding" | "Bar margin" | "Widget corners" => " px",
            "Clock scale" | "Widget opacity" => "%",
            _ => "",
        };
        format!(
            "{}: {} → {}{unit}",
            setting_title(&self.setting),
            self.before,
            self.after
        )
    }
}
/// The on-screen name of a recorded setting. Records keep their stored
/// names, so older and newer versions still read each other's history.
fn setting_title(setting: &str) -> &str {
    match setting {
        "Bar spacing" => "Between items",
        "Bar padding" => "Inner padding",
        "Bar margin" => "Edge gap",
        other => other,
    }
}
/// Which kind of saved choice a profile change applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Applied {
    Look,
    Style,
}
impl Applied {
    /// From the record's Undo label (see `ProfilePlan::label`).
    pub fn of_label(label: &str) -> Option<Self> {
        if label.starts_with("Profile “") {
            Some(Self::Look)
        } else if label.starts_with("Style") {
            Some(Self::Style)
        } else {
            None
        }
    }
}
/// What an item edit did on the Dock or Panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemAction {
    Added,
    Removed,
    /// To another region of the same bar.
    Moved,
    /// Within its region; the whole bar's order is named.
    Reordered,
}
/// Which bar an item edit changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemBar {
    Dock,
    Panel,
}
impl ItemBar {
    pub fn title(self) -> &'static str {
        match self {
            Self::Dock => "Dock",
            Self::Panel => "Panel",
        }
    }
}
/// The regions a moved item can go to, as their headers read.
const ITEM_REGIONS: &[&str] = &["Left", "Top", "Middle", "Right", "Bottom"];
/// The longest item name a record keeps.
const ITEM_NAME: usize = 64;
/// An item added, removed or moved on the Dock or Panel, named as the item
/// editor shows it. Only these bounded names are kept: an applet's name
/// and a region header, never the applet's id or other content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemChange {
    pub action: ItemAction,
    pub bar: ItemBar,
    /// The item's name; Reordered names none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The region a Moved item went to.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub to: String,
}
impl ItemChange {
    /// A record with `name` cut to the length a record keeps.
    pub fn new(action: ItemAction, bar: ItemBar, name: &str, to: Option<&str>) -> Option<Self> {
        let name: String = if action == ItemAction::Reordered {
            String::new()
        } else if name.chars().count() > ITEM_NAME {
            name.chars().take(ITEM_NAME - 1).chain(['…']).collect()
        } else {
            name.into()
        };
        let change = Self {
            action,
            bar,
            name,
            to: to
                .filter(|_| action == ItemAction::Moved)
                .unwrap_or_default()
                .into(),
        };
        change.valid().then_some(change)
    }
    /// "Added Sound", "Moved Sound to Right", "Reordered Dock items": the
    /// Undo label and the Changes title alike.
    pub fn title(&self) -> String {
        match self.action {
            ItemAction::Added => format!("Added {}", self.name),
            ItemAction::Removed => format!("Removed {}", self.name),
            ItemAction::Moved => format!("Moved {} to {}", self.name, self.to),
            ItemAction::Reordered => format!("Reordered {} items", self.bar.title()),
        }
    }
    pub fn valid(&self) -> bool {
        let named = !self.name.trim().is_empty()
            && self.name.chars().count() <= ITEM_NAME
            && !self.name.chars().any(char::is_control);
        match self.action {
            ItemAction::Added | ItemAction::Removed => named && self.to.is_empty(),
            ItemAction::Moved => named && ITEM_REGIONS.contains(&self.to.as_str()),
            ItemAction::Reordered => self.name.is_empty() && self.to.is_empty(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub id: String,
    pub at: String,
    pub owner: String,
    pub cause: Cause,
    pub action: Action,
    pub areas: Vec<String>,
    #[serde(default)]
    pub details: Vec<Detail>,
    /// Which widget settings a change touched, by fixed group name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub widgets: Vec<String>,
    /// A look or a style, for a profile change; older records don't say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied: Option<Applied>,
    /// Some changed values have no detail, so the details don't name the
    /// whole change.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub partial: bool,
    /// The Dock or Panel item a change added, removed or moved; older
    /// records and other changes have none.
    #[serde(
        default,
        deserialize_with = "deserialize_item",
        skip_serializing_if = "Option::is_none"
    )]
    pub item: Option<ItemChange>,
}
/// The widget setting groups a change can name, in display order.
pub const WIDGET_GROUPS: &[&str] = &[
    "Clock",
    "Daily overview",
    "Weather",
    "Calendar",
    "System monitor",
    "Music",
    "Notes",
    "Focus",
    "Projects",
    "Recipes",
    "Activity",
    "Time of day",
    "Widget style",
    "Wallpaper effects",
    "Widget arrangement",
    "Dock and panel style",
    "Settings app",
];
/// The group a widget setting belongs to, from its first path part.
pub fn widget_group(pointer: &str) -> Option<&'static str> {
    let mut parts = pointer.trim_start_matches('/').split('/');
    let first = parts.next()?;
    if pointer == "/appearance/style_bars" {
        return Some("Dock and panel style");
    }
    // An update interval belongs to the widget it updates.
    if first == "refresh_intervals" {
        return Some(match parts.next() {
            Some("clock") => "Clock",
            Some("weather") => "Weather",
            Some("calendar") => "Calendar",
            Some("projects") => "Projects",
            Some("devbox") => "System monitor",
            Some("glance") => "Daily overview",
            _ => "Widget arrangement",
        });
    }
    Some(match first {
        "clock" | "clock_font" => "Clock",
        "glance" => "Daily overview",
        "weather" | "location" => "Weather",
        "calendar_private" | "calendar_show_today" | "calendar_show_tomorrow"
        | "meeting_notice_minutes" => "Calendar",
        "devbox" => "System monitor",
        "music" => "Music",
        "notes" => "Notes",
        "focus" => "Focus",
        "project_roots" => "Projects",
        "recipes" => "Recipes",
        "activity_island" => "Activity",
        "moments" => "Time of day",
        "appearance" | "widget_font" | "theme" | "materials" | "material_motion" => "Widget style",
        "rain" => "Wallpaper effects",
        "enabled_widgets" | "widgets_visible" | "widget_order" | "widget_display"
        | "widget_positions" | "widget_sizes" | "widget_monitors" | "monitor_layouts"
        | "placement" => "Widget arrangement",
        "sidebar_slide" | "show_explanations" | "settings_animations" | "onboarded"
        | "setup_dismissed" => "Settings app",
        key if key.starts_with("setup_followup_") => "Settings app",
        _ => return None,
    })
}
const AREAS: &[&str] = &[
    "Appearance",
    "Wallpaper",
    "Dock and panel",
    "Widgets",
    "Windows",
    "Workspaces",
    "Shortcuts",
    "Desktop icons",
    "Desktop settings",
];
const SETTINGS: &[&str] = &[
    "Bar spacing",
    "Bar padding",
    "Bar margin",
    "Bar opacity",
    "Dark appearance",
    "Widget opacity",
    "Widget corners",
    "Widget outline",
    "Clock scale",
    "Clock date",
    "Clock seconds",
    "Reduce motion",
    "Widget snap",
];
impl Summary {
    pub fn title(&self) -> String {
        if self.action == Action::Undone {
            if let ([one], Some(Owner::Preferences)) =
                (self.widgets.as_slice(), Owner::parse(&self.owner))
            {
                return format!("Undid a change to {one}");
            }
            return "Undid a desktop change".into();
        }
        if self.action == Action::Kept {
            return "Kept current settings".into();
        }
        if self.cause == Cause::WallpaperColors {
            return "Updated colors from the wallpaper".into();
        }
        let owner = Owner::parse(&self.owner);
        if let Some(item) = self.item_title() {
            return item;
        }
        if let Some(line) = self.value_line() {
            return line;
        }
        if let Some(widgets) = self.widget_names() {
            return format!("Changed {widgets}");
        }
        let title = match owner {
            Some(Owner::Profile) => match self.applied {
                Some(Applied::Look) => "Applied a look",
                Some(Applied::Style) => "Applied a style",
                None => "Applied a look or style",
            },
            Some(Owner::Recovery) => "Restored desktop settings",
            Some(Owner::DockPanel) => "Changed Dock and panel",
            Some(Owner::Appearance) => "Changed appearance",
            Some(Owner::Wallpaper) => "Changed wallpaper",
            Some(Owner::Windows) => "Changed window settings",
            Some(Owner::Workspaces) => "Changed workspaces",
            Some(Owner::WindowRules) => "Changed floating app rules",
            Some(Owner::Shortcuts) => "Changed shortcuts",
            Some(Owner::DesktopIcons) => "Changed desktop icons",
            _ => "Changed widget settings",
        };
        if matches!(owner, Some(Owner::Profile | Owner::Recovery)) {
            let areas = self
                .areas
                .iter()
                .filter(|a| AREAS.contains(&a.as_str()))
                .map(String::as_str)
                .collect::<Vec<_>>();
            if !areas.is_empty() {
                return format!("{title} · {}", areas.join(", "));
            }
        }
        title.into()
    }
    /// A change of one setting, in the title's value form: "Between items:
    /// 5 → 9 px". Shows only the same bounded, non-content values the
    /// journal accepts. Old records and composite changes have none.
    pub fn value_line(&self) -> Option<String> {
        if self.action != Action::Changed || self.cause == Cause::WallpaperColors || self.partial {
            return None;
        }
        if matches!(
            Owner::parse(&self.owner),
            Some(Owner::Profile | Owner::Recovery)
        ) || self.details.len() != 1
            || self.areas.len() != 1
        {
            return None;
        }
        let detail = &self.details[0];
        (SETTINGS.contains(&detail.setting.as_str())
            && safe_scalar(&detail.before)
            && safe_scalar(&detail.after))
        .then(|| detail.line())
    }
    /// The Dock or Panel item change, as the item editor named it.
    pub fn item_title(&self) -> Option<String> {
        if self.action != Action::Changed
            || self.cause != Cause::You
            || Owner::parse(&self.owner) != Some(Owner::DockPanel)
        {
            return None;
        }
        self.item
            .as_ref()
            .filter(|item| item.valid())
            .map(ItemChange::title)
    }
    /// The one or two widget setting groups a widget change touched.
    pub fn widget_names(&self) -> Option<String> {
        if self.action != Action::Changed
            || self.cause == Cause::WallpaperColors
            || Owner::parse(&self.owner) != Some(Owner::Preferences)
        {
            return None;
        }
        match self.widgets.as_slice() {
            [one] => Some(one.clone()),
            [one, two] => Some(format!("{one} and {two}")),
            _ => None,
        }
    }
    fn valid(&self) -> bool {
        super::valid_name(&self.id, 96)
            && self.at.len() <= 64
            && chrono::DateTime::parse_from_rfc3339(&self.at).is_ok()
            && Owner::parse(&self.owner).is_some()
            && self.areas.len() <= AREAS.len()
            && self.areas.iter().all(|a| AREAS.contains(&a.as_str()))
            && self.details.len() <= 12
            && self.details.iter().all(|d| {
                SETTINGS.contains(&d.setting.as_str())
                    && safe_scalar(&d.before)
                    && safe_scalar(&d.after)
            })
    }
}
fn safe_scalar(s: &str) -> bool {
    matches!(s, "Default" | "On" | "Off")
        || (s.len() <= 24
            && s.parse::<f64>()
                .is_ok_and(|v| v.is_finite() && v.abs() <= 100000.))
}
fn scalar(value: Option<&serde_json::Value>) -> Option<String> {
    match value {
        None | Some(serde_json::Value::Null) => Some("Default".into()),
        Some(serde_json::Value::Bool(b)) => Some(if *b { "On" } else { "Off" }.into()),
        Some(serde_json::Value::Number(n)) => {
            let s = n.to_string();
            safe_scalar(&s).then_some(s)
        }
        _ => None,
    }
}
fn native_scalar(value: Option<&String>) -> Option<String> {
    match value {
        None => Some("Default".into()),
        Some(v) => {
            if v == "true" {
                Some("On".into())
            } else if v == "false" {
                Some("Off".into())
            } else {
                safe_scalar(v).then(|| v.clone())
            }
        }
    }
}
pub(super) fn has_changes(file: &UndoFile) -> bool {
    !file.entries.is_empty() || file.widget.as_ref().is_some_and(|w| !w.fields.is_empty())
}

pub(super) fn summarize(file: &UndoFile, display: bool) -> Summary {
    let cause = match file.source {
        Source::Manual => Cause::You,
        Source::Derived => Cause::WallpaperColors,
        Source::Scheduled if display => Cause::DisplayRule,
        Source::Scheduled => Cause::Schedule,
    };
    let mut areas = Vec::new();
    let mut details = Vec::new();
    // A value without a detail of its own: the details are not the whole change.
    let mut partial = false;
    for e in &file.entries {
        let area =
            if e.namespace.contains("CosmicPanel") || e.namespace == dock_islands_core::NAMESPACE {
                "Dock and panel"
            } else if e.namespace.contains("Theme") || e.namespace.contains("Tk") {
                "Appearance"
            } else if e.namespace.contains("Background") {
                "Wallpaper"
            } else if e.namespace.contains("Shortcuts") {
                "Shortcuts"
            } else if e.namespace.contains("Workspaces") {
                "Workspaces"
            } else if e.namespace.contains("Files") {
                "Desktop icons"
            } else {
                "Windows"
            };
        if !areas.contains(&area.to_string()) {
            areas.push(area.to_string());
        }
        let name = if e.namespace.starts_with("com.system76.CosmicPanel.") {
            match e.key.as_str() {
                "spacing" => Some("Bar spacing"),
                "padding" => Some("Bar padding"),
                "margin" => Some("Bar margin"),
                "opacity" => Some("Bar opacity"),
                _ => None,
            }
        } else if e.namespace == "com.system76.CosmicTheme.Mode" && e.key == "is_dark" {
            Some("Dark appearance")
        } else {
            None
        };
        if let Some(name) = name
            && let (Some(before), Some(after)) = (
                native_scalar(e.before.as_ref()),
                native_scalar(e.after.as_ref()),
            )
        {
            details.push(Detail {
                setting: name.into(),
                before,
                after,
            });
        } else {
            partial = true;
        }
    }
    let mut widgets = Vec::new();
    if let Some(widget) = &file.widget {
        // A dock or appearance choice also marks its property as chosen;
        // that mark alone is not a widget change.
        let mut fields: Vec<_> = widget
            .fields
            .iter()
            .filter(|f| f.pointer != "/appearance/overrides")
            .collect();
        if fields.is_empty() && file.entries.is_empty() {
            fields = widget.fields.iter().collect();
        }
        if !fields.is_empty() || file.entries.is_empty() {
            areas.push("Widgets".into());
        }
        for group in WIDGET_GROUPS {
            if fields.iter().any(|f| widget_group(&f.pointer) == Some(group)) {
                widgets.push(group.to_string());
            }
        }
        for field in fields {
            let name = match field.pointer.as_str() {
                "/appearance/opacity" => "Widget opacity",
                "/appearance/corner_radius" => "Widget corners",
                "/appearance/border_opacity" => "Widget outline",
                "/clock/scale" => "Clock scale",
                "/clock/show_date" => "Clock date",
                "/clock/show_seconds" => "Clock seconds",
                "/appearance/reduce_motion" => "Reduce motion",
                "/placement/snap" => "Widget snap",
                _ => {
                    partial = true;
                    continue;
                }
            };
            if let (Some(before), Some(after)) =
                (scalar(field.before.as_ref()), scalar(field.after.as_ref()))
            {
                details.push(Detail {
                    setting: name.into(),
                    before,
                    after,
                });
            } else {
                partial = true;
            }
        }
    }
    partial |= details.len() > 12;
    details.truncate(12);
    if areas.is_empty() {
        areas.push("Desktop settings".into());
    }
    let applied = (Owner::parse(&file.owner) == Some(Owner::Profile))
        .then(|| Applied::of_label(&file.label))
        .flatten();
    let item = (Owner::parse(&file.owner) == Some(Owner::DockPanel))
        .then(|| file.item.clone().filter(ItemChange::valid))
        .flatten();
    Summary {
        id: file.id.clone(),
        at: file.created.clone(),
        owner: file.owner.clone(),
        cause,
        action: Action::Changed,
        areas,
        details,
        widgets,
        applied,
        partial,
        item,
    }
}
impl super::LatestChange {
    /// This change as Changes lists it.
    pub fn summary(&self) -> Summary {
        summarize(&self.to_file(), false)
    }
}
pub(super) fn append(recent: &mut Vec<Summary>, mut summary: Summary) {
    // A group name this version doesn't know is dropped, never the entry.
    summary
        .widgets
        .retain(|w| WIDGET_GROUPS.contains(&w.as_str()));
    summary.widgets.truncate(WIDGET_GROUPS.len());
    // Likewise an item this version can't read.
    if summary.item.as_ref().is_some_and(|item| !item.valid()) {
        summary.item = None;
    }
    if !summary.valid() || recent.iter().any(|s| s.id == summary.id) {
        return;
    }
    recent.push(summary);
    while recent.len() > LIMIT || serde_json::to_vec(&recent).map_or(true, |b| b.len() > BYTES) {
        recent.remove(0);
    }
}
/// Bad optional metadata never invalidates the independently validated Undo data.
pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Summary>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    let mut result = Vec::new();
    if let Some(values) = value.as_array() {
        for v in values.iter().rev().take(LIMIT).rev() {
            if let Ok(summary) = serde_json::from_value::<Summary>(v.clone()) {
                append(&mut result, summary);
            }
        }
    }
    Ok(result)
}
#[derive(Clone, Debug, Default)]
pub struct RecentChanges {
    pub entries: Vec<Summary>,
    pub undo_id: Option<String>,
    pub earlier_partial: bool,
    /// Your changes whose full record is still kept, so they can be put back.
    pub restorable: Vec<String>,
}
impl DesktopStore {
    pub fn recent_change_path(&self) -> std::path::PathBuf {
        self.undo_path()
    }
    pub fn recent_changes(&self) -> Result<RecentChanges, DesktopError> {
        let Some(file) = self.history_file()? else {
            return Ok(RecentChanges::default());
        };
        let undo_id = file.manual().map(|f| f.id.clone());
        let restorable = file
            .history
            .iter()
            .chain(std::iter::once(&file))
            .filter(|f| f.source == Source::Manual && has_changes(f))
            .map(|f| f.id.clone())
            .collect();
        let earlier_partial = file.recent_partial || file.recent.is_empty();
        let mut entries = file.recent.clone();
        if entries.is_empty() {
            for older in file.history.iter().chain(std::iter::once(&file)) {
                // Empty retained records are not observations of a real action.
                if has_changes(older) {
                    append(&mut entries, summarize(older, false));
                }
            }
        }
        Ok(RecentChanges {
            entries,
            undo_id,
            earlier_partial,
            restorable,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::store::{
        ConfigKey, DesktopPatch, Fault, LatestChange, RawValue, tests::Fixture,
    };
    fn apply(
        store: &DesktopStore,
        value: u32,
        source: Source,
        display: bool,
    ) -> super::super::LatestChange {
        let key = ConfigKey::new("com.system76.CosmicPanel.Panel", 1, "spacing").unwrap();
        let mut patch = DesktopPatch::new();
        patch.activity_display = display;
        patch.set(
            key.clone(),
            store.read_user(&key).unwrap(),
            Some(RawValue::parse((value % 24).to_string()).unwrap()),
        );
        store
            .apply_action(&patch, None, Owner::DockPanel, "PRIVATE_SENTINEL", source)
            .unwrap()
    }
    fn apply_item(store: &DesktopStore, value: u32, item: Option<ItemChange>) -> LatestChange {
        let key = ConfigKey::new("com.system76.CosmicPanel.Dock", 1, "spacing").unwrap();
        let mut patch = DesktopPatch::new();
        patch.activity_item = item;
        patch.set(
            key.clone(),
            store.read_user(&key).unwrap(),
            Some(RawValue::parse(value.to_string()).unwrap()),
        );
        store
            .apply_action(
                &patch,
                None,
                Owner::DockPanel,
                "Dock and panel",
                Source::Manual,
            )
            .unwrap()
    }
    /// Removes every `item` field, as a record written before items were named.
    fn strip_items(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.remove("item");
                map.values_mut().for_each(strip_items);
            }
            serde_json::Value::Array(list) => list.iter_mut().for_each(strip_items),
            _ => {}
        }
    }
    #[test]
    fn item_changes_keep_their_names_and_older_records_still_load() {
        use ItemAction::*;
        for (item, title) in [
            (
                ItemChange::new(Added, ItemBar::Dock, "Sound", None),
                "Added Sound",
            ),
            (
                ItemChange::new(Removed, ItemBar::Panel, "Time", None),
                "Removed Time",
            ),
            (
                ItemChange::new(Moved, ItemBar::Dock, "Sound", Some("Right")),
                "Moved Sound to Right",
            ),
            (
                ItemChange::new(Moved, ItemBar::Panel, "Time", Some("Bottom")),
                "Moved Time to Bottom",
            ),
            (
                ItemChange::new(Reordered, ItemBar::Dock, "Sound", None),
                "Reordered Dock items",
            ),
            (
                ItemChange::new(Reordered, ItemBar::Panel, "", None),
                "Reordered Panel items",
            ),
        ] {
            assert_eq!(item.unwrap().title(), title);
        }
        // Only bounded names: a region header, a short applet name.
        assert!(ItemChange::new(Moved, ItemBar::Dock, "Sound", Some("Elsewhere")).is_none());
        assert!(ItemChange::new(Moved, ItemBar::Dock, "Sound", None).is_none());
        assert!(ItemChange::new(Added, ItemBar::Dock, " ", None).is_none());
        assert!(ItemChange::new(Added, ItemBar::Dock, "A\nB", None).is_none());
        let long = ItemChange::new(Added, ItemBar::Dock, &"x".repeat(200), None).unwrap();
        assert_eq!(long.name.chars().count(), ITEM_NAME);
        assert!(long.name.ends_with('…'));

        let f = Fixture::new("activity-items");
        let store = f.store();
        let added = ItemChange::new(Added, ItemBar::Dock, "Sound", None).unwrap();
        let latest = apply_item(&store, 3, Some(added.clone()));
        assert_eq!(latest.item.as_ref(), Some(&added));
        assert_eq!(latest.summary().title(), "Added Sound");
        // Kept across a restart, for Changes and the footer alike.
        let entry = f.store().recent_changes().unwrap().entries.pop().unwrap();
        assert_eq!(entry.title(), "Added Sound");
        assert_eq!(
            f.store().latest_change().unwrap().unwrap().item,
            Some(added.clone())
        );
        // Only your own change is named for its item.
        let mut other = entry.clone();
        other.cause = Cause::Schedule;
        assert_eq!(other.item_title(), None);
        other = entry.clone();
        other.action = Action::Undone;
        assert_eq!(other.title(), "Undid a desktop change");
        other = entry.clone();
        other.owner = Owner::Profile.key().into();
        assert_eq!(other.item_title(), None);
        // Other changes keep their value titles.
        apply_item(&store, 4, None);
        let entry = f.store().recent_changes().unwrap().entries.pop().unwrap();
        assert_eq!(entry.item, None);
        assert!(
            entry.title().starts_with("Between items: "),
            "{}",
            entry.title()
        );

        // A record written before items were named still loads and undoes.
        let path = store.recent_change_path();
        let mut file: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(serde_json::to_string(&file).unwrap().contains("\"added\""));
        let mut legacy = file.clone();
        strip_items(&mut legacy);
        std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        let recent = f.store().recent_changes().unwrap();
        assert_eq!(recent.entries.len(), 2);
        assert!(recent.entries.iter().all(|e| e.item.is_none()));
        assert!(recent.entries[0].title().starts_with("Between items: "));
        assert!(f.store().latest_change().unwrap().unwrap().item.is_none());

        // An item record this version can't read drops the item, not the change.
        let renamed = serde_json::json!({"action": "renamed", "bar": "dock", "name": "Sound"});
        let elsewhere = serde_json::json!({"action": "moved", "bar": "dock", "to": "Elsewhere"});
        file["history"][0]["item"] = renamed;
        file["recent"][0]["item"] = elsewhere;
        std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
        let recent = f.store().recent_changes().unwrap();
        assert_eq!(recent.entries.len(), 2);
        assert_eq!(recent.entries[0].item, None);
        assert!(recent.entries[0].title().starts_with("Between items: "));
        let latest = f.store().latest_change().unwrap().unwrap();
        f.store().undo_latest(&latest.id).unwrap();
        let undone = f.store().latest_change().unwrap().unwrap();
        assert_eq!(undone.item, None);
        assert_eq!(undone.summary().title(), recent.entries[0].title());
    }
    #[test]
    fn polish050_history_titles_keep_safe_detail_and_legacy_fallback() {
        let f = Fixture::new("polish050-history-title");
        apply(&f.store(), 7, Source::Manual, false);
        let mut entry = f.store().recent_changes().unwrap().entries.pop().unwrap();
        assert_eq!(entry.title(), "Between items: Default → 7 px");
        entry.details.clear();
        assert_eq!(entry.title(), "Changed Dock and panel");
        entry.details.push(Detail {
            setting: "Note text".into(),
            before: "PRIVATE_SENTINEL".into(),
            after: "SECRET".into(),
        });
        assert_eq!(entry.title(), "Changed Dock and panel");
        entry.details[0].setting = "Bar spacing".into();
        assert_eq!(entry.title(), "Changed Dock and panel");
        entry.action = Action::Undone;
        assert_eq!(entry.title(), "Undid a desktop change");
    }

    #[test]
    fn palette_acknowledgement_does_not_report_a_desktop_change() {
        let f = Fixture::new("activity-acknowledgement");
        let store = f.store();
        let first = apply(&store, 1, Source::Manual, false);
        let before = store.recent_changes().unwrap().entries;
        assert!(
            store
                .apply_palette(
                    &DesktopPatch::new(),
                    &super::super::WidgetPatch::new(),
                    store.restore_epoch().unwrap(),
                    42,
                )
                .unwrap()
                .is_none()
        );
        let after = f.store().recent_changes().unwrap();
        assert_eq!(after.entries, before);
        assert_eq!(after.undo_id, Some(first.id));
        assert!(
            store
                .palette_seen(store.restore_epoch().unwrap(), 42)
                .unwrap()
        );
    }

    #[test]
    fn bounded_history_survives_derived_switches_undo_and_restart() {
        let f = Fixture::new("activity-retention");
        let store = f.store();
        let first = apply(&store, 1, Source::Manual, false);
        for value in 2..60 {
            apply(&store, value, Source::Scheduled, value % 2 == 0);
        }
        let recent = f.store().recent_changes().unwrap();
        assert_eq!(recent.entries.len(), LIMIT);
        assert_eq!(recent.undo_id.as_deref(), Some(first.id.as_str()));
        assert_eq!(
            recent.entries[recent.entries.len() - 2].cause,
            Cause::DisplayRule
        );
        assert!(serde_json::to_string(&recent.entries).unwrap().len() <= BYTES);
        assert!(
            !serde_json::to_string(&recent.entries)
                .unwrap()
                .contains("PRIVATE_SENTINEL")
        );
        let latest = apply(&store, 60, Source::Manual, false);
        store.undo_latest(&latest.id).unwrap();
        let recent = f.store().recent_changes().unwrap();
        assert_eq!(recent.entries.last().unwrap().action, Action::Undone);
        assert_eq!(recent.undo_id, Some(first.id));
    }
    #[test]
    fn failed_commits_do_not_write_successful_history_and_optional_damage_does_not_break_undo() {
        let f = Fixture::new("activity-failure");
        let store = f.store();
        let first = apply(&store, 1, Source::Manual, false);
        let before = store.recent_changes().unwrap().entries;
        let key = ConfigKey::new("com.system76.CosmicPanel.Panel", 1, "spacing").unwrap();
        let mut patch = DesktopPatch::new();
        patch.set(
            key.clone(),
            store.read_user(&key).unwrap(),
            Some(RawValue::parse("2").unwrap()),
        );
        store.inject(Some(Fault::FailRecord));
        assert!(
            store
                .apply_action(&patch, None, Owner::DockPanel, "failed", Source::Manual)
                .is_err()
        );
        store.inject(None);
        assert_eq!(store.recent_changes().unwrap().entries, before);
        let mut file = serde_json::to_value(store.history_file().unwrap().unwrap()).unwrap();
        file["recent"] = serde_json::json!([{"private":"PRIVATE_SENTINEL"},42]);
        let file: UndoFile = serde_json::from_value(file).unwrap();
        file.validate_history().unwrap();
        assert!(file.recent.is_empty());
        store.undo_latest(&first.id).unwrap();
        let recent = f.store().recent_changes().unwrap();
        assert!(recent.undo_id.is_none());
        assert_eq!(recent.entries.len(), 2);
        assert_eq!(recent.entries.last().unwrap().action, Action::Undone);
    }
}

/// An item record this version can't read is left out, not the change
/// that carries it.
pub(super) fn deserialize_item<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ItemChange>, D::Error> {
    Ok(
        serde_json::from_value(serde_json::Value::deserialize(deserializer)?)
            .ok()
            .filter(ItemChange::valid),
    )
}

pub(super) fn deserialize_partial<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<bool, D::Error> {
    Ok(serde_json::Value::deserialize(deserializer)?
        .as_bool()
        .unwrap_or(true))
}
