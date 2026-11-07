//! Installed applications for the Floating apps picker: name, icon and a
//! candidate app ID from each desktop entry. Same discovery as the applet
//! catalog: `applications` directories in XDG precedence order, the first
//! entry for an ID wins and `Hidden=true` hides it. Entries are only read;
//! nothing is launched or probed.
use super::applets::{application_dirs, collect};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppInfo {
    /// The desktop entry ID (file name without `.desktop`).
    pub id: String,
    pub name: String,
    pub icon: String,
    /// The app ID a window of this app is likely to report: the entry's
    /// `StartupWMClass` when it has one, else its desktop ID. Always shown
    /// for editing; it is a suggestion, not a guarantee.
    pub app_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppCatalog {
    apps: BTreeMap<String, AppInfo>,
}

impl AppCatalog {
    pub fn installed() -> Self {
        Self::discover(&application_dirs(
            dirs::data_dir(),
            std::env::var("XDG_DATA_DIRS").ok(),
        ))
    }
    /// `dirs` are `applications` directories, highest precedence first.
    pub fn discover(dirs: &[PathBuf]) -> Self {
        let mut seen = HashSet::new();
        let mut apps = BTreeMap::new();
        for dir in dirs {
            let mut files = Vec::new();
            collect(dir, dir, &mut files);
            files.sort();
            for (id, path) in files {
                if !seen.insert(id.clone()) {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                if let Some(info) = parse(&id, &text) {
                    apps.insert(id, info);
                }
            }
        }
        Self { apps }
    }
    /// Apps sorted by name.
    pub fn sorted(&self) -> Vec<&AppInfo> {
        let mut apps: Vec<_> = self.apps.values().collect();
        apps.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        apps
    }
    pub fn get(&self, id: &str) -> Option<&AppInfo> {
        self.apps.get(id)
    }
    /// The app whose candidate app ID is exactly `app_id`.
    pub fn for_app_id(&self, app_id: &str) -> Option<&AppInfo> {
        self.apps
            .values()
            .find(|info| info.app_id == app_id)
            .or_else(|| self.apps.get(app_id))
    }
}

/// The dock's pinned apps (COSMIC's app list favorites), as icon names in
/// their order. Reads only each pinned app's own entry from `dirs`
/// (highest precedence first); at most `limit`.
pub fn pinned_icons(
    store: &crate::desktop::store::DesktopStore,
    dirs: &[PathBuf],
    limit: usize,
) -> Vec<String> {
    let Ok(key) = crate::desktop::store::ConfigKey::new("com.system76.CosmicAppList", 1, "favorites")
    else {
        return Vec::new();
    };
    let Ok(snapshot) = store.load(std::slice::from_ref(&key)) else {
        return Vec::new();
    };
    let Some(Ok(ids)) = snapshot.effective(&key).map(|raw| raw.decode::<Vec<String>>()) else {
        return Vec::new();
    };
    ids.iter()
        .filter(|id| !id.is_empty() && !id.contains(['/', '\\']) && id.len() <= 255)
        .take(limit)
        .map(|id| {
            // The first entry found wins, as for the launcher; its icon is
            // used even when the entry is kept out of menus.
            dirs.iter()
                .find_map(|dir| std::fs::read_to_string(dir.join(format!("{id}.desktop"))).ok())
                .and_then(|text| entry_icon(&text))
                .filter(|icon| !icon.is_empty())
                // Apps usually name their icon after their ID.
                .unwrap_or_else(|| id.clone())
        })
        .collect()
}

/// The `Icon` of a desktop entry, unless the entry is `Hidden` (removed).
fn entry_icon(text: &str) -> Option<String> {
    let mut in_entry = false;
    let mut icon = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        match line.split_once('=').filter(|_| in_entry) {
            Some(("Hidden", value)) if value.trim() == "true" => return None,
            Some(("Icon", value)) => icon = Some(value.trim().to_string()),
            _ => {}
        }
    }
    icon
}

fn parse(id: &str, text: &str) -> Option<AppInfo> {
    let mut in_entry = false;
    let mut values = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            values
                .entry(key.trim().to_string())
                .or_insert_with(|| value.trim().to_string());
        }
    }
    let flag = |key: &str| values.get(key).is_some_and(|value| value == "true");
    if flag("Hidden") || flag("NoDisplay") {
        return None;
    }
    if values.get("Type").is_some_and(|kind| kind != "Application") {
        return None;
    }
    let name = values.get("Name").filter(|name| !name.is_empty())?.clone();
    let app_id = values
        .get("StartupWMClass")
        .filter(|class| !class.is_empty())
        .cloned()
        .unwrap_or_else(|| id.to_string());
    Some(AppInfo {
        id: id.into(),
        name,
        icon: values.get("Icon").cloned().unwrap_or_default(),
        app_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::store::tests::Fixture;
    use std::path::Path;

    fn entry(dir: &Path, name: &str, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), format!("[Desktop Entry]\n{body}")).unwrap();
    }

    /// The picker follows XDG precedence and hiding, offers StartupWMClass
    /// when an entry names one, and never reads an Exec line as an ID.
    #[test]
    fn installed_apps_follow_precedence_and_suggest_app_ids() {
        let fixture = Fixture::new("apps");
        let user = fixture.0.join("home/applications");
        let system = fixture.0.join("usr/applications");
        entry(
            &system,
            "org.example.Editor.desktop",
            "Type=Application\nName=Editor\nIcon=accessories-text-editor\nExec=sentinel-run\n",
        );
        entry(
            &user,
            "org.example.Editor.desktop",
            "Type=Application\nName=My Editor\nExec=sentinel-run\n",
        );
        entry(
            &system,
            "legacy.desktop",
            "Type=Application\nName=Legacy\nStartupWMClass=LegacyWindow\n",
        );
        entry(&system, "gone.desktop", "Type=Application\nName=Gone\n");
        entry(&user, "gone.desktop", "Hidden=true\n");
        entry(
            &system,
            "helper.desktop",
            "Type=Application\nName=Helper\nNoDisplay=true\n",
        );
        entry(&system, "link.desktop", "Type=Link\nName=Link\n");
        let catalog = AppCatalog::discover(&[user, system]);
        let names: Vec<_> = catalog.sorted().iter().map(|a| a.name.clone()).collect();
        assert_eq!(names, ["Legacy", "My Editor"]);
        assert_eq!(
            catalog.get("org.example.Editor").unwrap().app_id,
            "org.example.Editor"
        );
        assert_eq!(catalog.get("legacy").unwrap().app_id, "LegacyWindow");
        assert_eq!(catalog.for_app_id("LegacyWindow").unwrap().name, "Legacy");
        assert!(
            catalog
                .sorted()
                .iter()
                .all(|a| !a.app_id.contains("sentinel"))
        );
    }
}
