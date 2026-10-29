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
}
const AREAS: &[&str] = &[
    "Appearance",
    "Wallpaper",
    "Dock & panel",
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
            return "Undid a desktop change".into();
        }
        if self.action == Action::Kept {
            return "Kept current settings".into();
        }
        if self.cause == Cause::WallpaperColors {
            return "Updated colors from the wallpaper".into();
        }
        let owner = Owner::parse(&self.owner);
        // Display only the same bounded, non-content values accepted by the
        // journal. Old records and composite changes retain useful fallbacks.
        if !matches!(owner, Some(Owner::Profile | Owner::Recovery))
            && self.details.len() == 1
            && self.areas.len() == 1
        {
            let detail = &self.details[0];
            if SETTINGS.contains(&detail.setting.as_str())
                && safe_scalar(&detail.before)
                && safe_scalar(&detail.after)
            {
                let unit = match detail.setting.as_str() {
                    "Bar spacing" | "Bar padding" | "Bar margin" | "Widget corners" => " px",
                    "Clock scale" | "Widget opacity" => "%",
                    _ => "",
                };
                return format!(
                    "{}: {} → {}{}",
                    detail.setting, detail.before, detail.after, unit
                );
            }
        }
        let title = match owner {
            Some(Owner::Profile) => "Applied a theme or setup",
            Some(Owner::Recovery) => "Restored desktop settings",
            Some(Owner::DockPanel) => "Changed Dock & panel",
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
    for e in &file.entries {
        let area =
            if e.namespace.contains("CosmicPanel") || e.namespace == dock_islands_core::NAMESPACE {
                "Dock & panel"
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
        }
    }
    if let Some(widget) = &file.widget {
        areas.push("Widgets".into());
        for field in &widget.fields {
            let name = match field.pointer.as_str() {
                "/appearance/opacity" => "Widget opacity",
                "/appearance/corner_radius" => "Widget corners",
                "/appearance/border_opacity" => "Widget outline",
                "/clock/scale" => "Clock scale",
                "/clock/show_date" => "Clock date",
                "/clock/show_seconds" => "Clock seconds",
                "/appearance/reduce_motion" => "Reduce motion",
                "/placement/snap" => "Widget snap",
                _ => continue,
            };
            if let (Some(before), Some(after)) =
                (scalar(field.before.as_ref()), scalar(field.after.as_ref()))
            {
                details.push(Detail {
                    setting: name.into(),
                    before,
                    after,
                });
            }
        }
    }
    details.truncate(12);
    if areas.is_empty() {
        areas.push("Desktop settings".into());
    }
    Summary {
        id: file.id.clone(),
        at: file.created.clone(),
        owner: file.owner.clone(),
        cause,
        action: Action::Changed,
        areas,
        details,
    }
}
pub(super) fn append(recent: &mut Vec<Summary>, summary: Summary) {
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
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::store::{ConfigKey, DesktopPatch, Fault, RawValue, tests::Fixture};
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
    #[test]
    fn polish050_history_titles_keep_safe_detail_and_legacy_fallback() {
        let f = Fixture::new("polish050-history-title");
        apply(&f.store(), 7, Source::Manual, false);
        let mut entry = f.store().recent_changes().unwrap().entries.pop().unwrap();
        assert_eq!(entry.title(), "Bar spacing: Default → 7 px");
        entry.details.clear();
        assert_eq!(entry.title(), "Changed Dock & panel");
        entry.details.push(Detail {
            setting: "Note text".into(),
            before: "PRIVATE_SENTINEL".into(),
            after: "SECRET".into(),
        });
        assert_eq!(entry.title(), "Changed Dock & panel");
        entry.details[0].setting = "Bar spacing".into();
        assert_eq!(entry.title(), "Changed Dock & panel");
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

pub(super) fn deserialize_partial<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<bool, D::Error> {
    Ok(serde_json::Value::deserialize(deserializer)?
        .as_bool()
        .unwrap_or(true))
}
