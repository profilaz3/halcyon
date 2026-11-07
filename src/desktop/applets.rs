//! Installed COSMIC applets, discovered like cosmic-settings does: desktop
//! entries carrying `X-CosmicApplet`. Directories are searched in XDG order
//! (`$XDG_DATA_HOME`, then `$XDG_DATA_DIRS`); the first entry for an ID wins,
//! so a user entry overrides or hides (`Hidden=true`) a system one.
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

pub const FALLBACK_ICON: &str = "application-x-addon-symbolic";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppletInfo {
    pub id: String,
    pub name: String,
    pub comment: String,
    pub icon: String,
}

/// Discovery filters describe installed applets; they never advertise a
/// synthetic applet or infer that a desktop card can run in the panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AppletGroup {
    #[default]
    All,
    Essentials,
    Media,
    Readings,
    Controls,
}

impl AppletGroup {
    pub const ALL: [Self; 5] = [
        Self::All,
        Self::Essentials,
        Self::Media,
        Self::Readings,
        Self::Controls,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Essentials => "Essentials",
            Self::Media => "Media",
            Self::Readings => "System readings",
            Self::Controls => "Quick controls",
        }
    }
    pub fn includes(self, info: &AppletInfo) -> bool {
        let words = info.search_text();
        let any = |terms: &[&str]| terms.iter().any(|term| words.contains(term));
        match self {
            Self::All => true,
            Self::Essentials => STARTER.iter().any(|(_, id)| *id == info.id),
            Self::Media => any(&["mpris", "music", "media", "now playing", "player"]),
            Self::Readings => any(&[
                "system monitor",
                "system readings",
                "cpu",
                "memory",
                "minimon",
                "system-monitor",
            ]),
            Self::Controls => any(&[
                "control",
                "manage",
                "switch",
                "toggle",
                "brightness",
                "night light",
                "accessibility",
            ]),
        }
    }
}

impl AppletInfo {
    pub fn search_text(&self) -> String {
        format!(
            "{} {} {} {}",
            self.name,
            self.comment,
            self.id,
            match self.id.as_str() {
                "com.system76.CosmicAppletAudio" => "now playing music media volume quick controls",
                "com.system76.CosmicAppletTime" => "clock date calendar",
                "com.system76.CosmicAppletPower" => "power shutdown logout",
                "com.system76.CosmicAppletStatusArea" => "system tray indicators",
                _ => "",
            }
        )
        .to_lowercase()
    }
    pub fn description(&self) -> &str {
        match self.id.as_str() {
            "com.system76.CosmicAppletAudio" => "Volume, audio devices and now-playing controls",
            "com.system76.CosmicAppletTime" => "Time and date · opens the native calendar",
            _ => &self.comment,
        }
    }
}

use super::panels::{Region, Regions};
const STARTER: [(Region, &str); 10] = [
    (Region::Start, "com.system76.CosmicPanelWorkspacesButton"),
    (Region::Start, "com.system76.CosmicPanelLauncherButton"),
    (Region::Center, "com.system76.CosmicAppletTime"),
    (Region::End, "com.system76.CosmicAppletNotifications"),
    (Region::End, "com.system76.CosmicAppletAudio"),
    (Region::End, "com.system76.CosmicAppletNetwork"),
    (Region::End, "com.system76.CosmicAppletBluetooth"),
    (Region::End, "com.system76.CosmicAppletStatusArea"),
    (Region::End, "com.system76.CosmicAppletBattery"),
    (Region::End, "com.system76.CosmicAppletPower"),
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppletCatalog {
    applets: BTreeMap<String, AppletInfo>,
    /// Icons of the dock's pinned apps, drawn in previews in place of the
    /// app list applet's own icon.
    pinned: Vec<String>,
}

impl AppletCatalog {
    /// Public scene illustrations only; never use this catalog for admission.
    pub(crate) fn public_sample() -> Self {
        let applets = [
            (
                "com.system76.CosmicPanelWorkspacesButton",
                "view-grid-symbolic",
            ),
            (
                "com.system76.CosmicAppletTime",
                "preferences-system-time-symbolic",
            ),
            (
                "com.system76.CosmicAppletAudio",
                "audio-volume-high-symbolic",
            ),
            ("com.system76.CosmicAppletPower", "system-shutdown-symbolic"),
        ]
        .into_iter()
        .map(|(id, icon)| {
            (
                id.into(),
                AppletInfo {
                    id: id.into(),
                    name: String::new(),
                    comment: String::new(),
                    icon: icon.into(),
                },
            )
        })
        .collect();
        Self { applets, pinned: Vec::new() }
    }

    /// An explicit starting point, using only installed native applets.
    pub fn starter(&self, battery: bool) -> Regions {
        let mut regions = Regions::default();
        for (region, id) in STARTER {
            if self.known(id) && (battery || id != "com.system76.CosmicAppletBattery") {
                regions.add(region, id);
            }
        }
        regions
    }
    /// Short labels in the visual editor; discovery retains the native names.
    pub fn panel_label(&self, id: &str) -> String {
        match id {
            "com.system76.CosmicPanelWorkspacesButton" => "Workspaces".into(),
            "com.system76.CosmicPanelLauncherButton" => "Launcher".into(),
            "com.system76.CosmicPanelAppButton" => "Applications".into(),
            "com.system76.CosmicAppletTime" => "Time & date".into(),
            "com.system76.CosmicAppletNotifications" => "Notifications".into(),
            "com.system76.CosmicAppletStatusArea" => "System tray".into(),
            "com.system76.CosmicAppletPower" => "Power".into(),
            "com.system76.CosmicAppletBattery" => "Battery".into(),
            _ => self.label(id),
        }
    }
    pub fn installed() -> Self {
        Self::discover(&application_dirs(
            dirs::data_dir(),
            std::env::var("XDG_DATA_DIRS").ok(),
        ))
    }
    /// `dirs` are `applications` directories, highest precedence first.
    pub fn discover(dirs: &[PathBuf]) -> Self {
        let mut seen = HashSet::new();
        let mut applets = BTreeMap::new();
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
                    applets.insert(id, info);
                }
            }
        }
        Self { applets, pinned: Vec::new() }
    }
    pub fn get(&self, id: &str) -> Option<&AppletInfo> {
        self.applets.get(id)
    }
    pub fn known(&self, id: &str) -> bool {
        self.applets.contains_key(id)
    }
    pub fn label(&self, id: &str) -> String {
        self.get(id)
            .map(|info| info.name.clone())
            .unwrap_or_else(|| fallback_label(id))
    }
    /// Draws the app list as the apps pinned to it.
    pub fn with_pinned(mut self, pinned: Vec<String>) -> Self {
        self.pinned = pinned;
        self
    }
    /// The icons a preview draws for one applet: the pinned apps for the
    /// app list when there are any, otherwise the applet's own icon.
    pub fn icons(&self, id: &str) -> Vec<String> {
        if id == "com.system76.CosmicAppList" && !self.pinned.is_empty() {
            return self.pinned.clone();
        }
        vec![self.icon(id)]
    }
    pub fn icon(&self, id: &str) -> String {
        self.get(id)
            .map(|info| info.icon.clone())
            .filter(|icon| !icon.is_empty())
            .unwrap_or_else(|| FALLBACK_ICON.into())
    }
    /// Installed applets sorted by name.
    pub fn sorted(&self) -> Vec<&AppletInfo> {
        let mut applets: Vec<_> = self.applets.values().collect();
        applets.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        applets
    }
}

/// `applications` directories in XDG precedence order.
pub fn application_dirs(data_home: Option<PathBuf>, data_dirs: Option<String>) -> Vec<PathBuf> {
    let dirs = data_dirs
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let mut result = Vec::new();
    for dir in data_home
        .into_iter()
        .chain(dirs.split(':').map(PathBuf::from))
    {
        let dir = dir.join("applications");
        if dir.is_absolute() && !result.contains(&dir) {
            result.push(dir);
        }
    }
    result
}

/// Desktop entry IDs use the path below `applications`, with `/` as `-`.
pub(crate) fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, files);
        } else if path.extension().is_some_and(|ext| ext == "desktop")
            && let Ok(relative) = path.strip_prefix(root)
        {
            let id = relative
                .with_extension("")
                .to_string_lossy()
                .replace('/', "-");
            files.push((id, path));
        }
    }
}

fn parse(id: &str, text: &str) -> Option<AppletInfo> {
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
    if values.get("Hidden").is_some_and(|value| value == "true") {
        return None;
    }
    values.get("X-CosmicApplet")?;
    let name = values
        .get("Name")
        .filter(|name| !name.is_empty())
        .cloned()
        .unwrap_or_else(|| fallback_label(id));
    Some(AppletInfo {
        id: id.into(),
        name,
        comment: values.get("Comment").cloned().unwrap_or_default(),
        icon: values.get("Icon").cloned().unwrap_or_default(),
    })
}

/// Readable name for an applet without installed metadata:
/// `com.system76.CosmicAppletAudio` → `Audio`, `io.x.minimon-applet` → `Minimon`.
pub fn fallback_label(id: &str) -> String {
    let last = id.rsplit('.').next().unwrap_or(id);
    let trimmed = ["CosmicApplet", "CosmicPanel", "CosmicExtApplet", "Cosmic"]
        .into_iter()
        .find_map(|prefix| last.strip_prefix(prefix).filter(|rest| !rest.is_empty()))
        .unwrap_or(last);
    let trimmed = trimmed
        .trim_end_matches("-applet")
        .trim_end_matches("_applet");
    let mut words = String::new();
    let mut previous_lower = false;
    for character in trimmed.chars() {
        if matches!(character, '-' | '_') {
            words.push(' ');
            previous_lower = false;
            continue;
        }
        if character.is_uppercase() && previous_lower {
            words.push(' ');
        }
        previous_lower = character.is_lowercase() || character.is_ascii_digit();
        words.push(character);
    }
    let mut label = words.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some(first) = label.get(..1) {
        label = first.to_uppercase() + &label[1..];
    }
    if label.is_empty() {
        id.to_string()
    } else {
        label
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::store::tests::Fixture;

    fn entry(dir: &Path, name: &str, body: &str) {
        std::fs::create_dir_all(dir.join(name).parent().unwrap()).unwrap();
        std::fs::write(dir.join(name), format!("[Desktop Entry]\n{body}")).unwrap();
    }

    #[test]
    fn media_search_finds_native_sound_without_inventing_optional_applets() {
        let sound = AppletInfo {
            id: "com.system76.CosmicAppletAudio".into(),
            name: "Sound".into(),
            comment: "Audio devices".into(),
            icon: String::new(),
        };
        assert!(sound.search_text().contains("now playing"));
        assert!(AppletGroup::Media.includes(&sound));
        assert!(AppletGroup::Essentials.includes(&sound));
        assert!(!AppletGroup::Readings.includes(&sound));
        let nightlight = AppletInfo {
            id: "io.github.cosmic_nightlight".into(),
            name: "Night Light".into(),
            comment: "Warm the screen color temperature".into(),
            icon: String::new(),
        };
        assert!(AppletGroup::Controls.includes(&nightlight));
        assert!(!AppletGroup::Readings.includes(&nightlight));
        assert_eq!(AppletCatalog::default().starter(true), Regions::default());
    }

    #[test]
    fn user_entries_take_precedence_and_can_hide_system_applets() {
        let fixture = Fixture::new("applets");
        let user = fixture.0.join("home/applications");
        let flatpak = fixture.0.join("flatpak/applications");
        let system = fixture.0.join("usr/applications");
        entry(
            &system,
            "com.system76.CosmicAppletAudio.desktop",
            "Name=Sound\nIcon=audio-symbolic\nX-CosmicApplet=true\nNoDisplay=true\n",
        );
        entry(
            &user,
            "com.system76.CosmicAppletAudio.desktop",
            "Name=My Sound\nX-CosmicApplet=true\n[Desktop Action x]\nName=Ignored\n",
        );
        entry(
            &system,
            "com.system76.CosmicAppletPower.desktop",
            "Name=Power\nX-CosmicApplet=true\n",
        );
        entry(
            &user,
            "com.system76.CosmicAppletPower.desktop",
            "Name=Power\nHidden=true\n",
        );
        entry(
            &flatpak,
            "io.github.example.Clock.desktop",
            "Name=Clock\nX-CosmicApplet=true\n",
        );
        entry(
            &system,
            "io.github.example.Clock.desktop",
            "Name=Old clock\nX-CosmicApplet=true\n",
        );
        entry(&system, "org.example.Editor.desktop", "Name=Editor\n");
        entry(
            &system,
            "vendor/Nested.desktop",
            "Name=Nested\nX-CosmicApplet=true\n",
        );
        let catalog = AppletCatalog::discover(&[user, flatpak, system]);
        assert_eq!(catalog.label("com.system76.CosmicAppletAudio"), "My Sound");
        assert_eq!(
            catalog.icon("com.system76.CosmicAppletAudio"),
            FALLBACK_ICON
        );
        assert!(!catalog.known("com.system76.CosmicAppletPower"));
        assert_eq!(catalog.label("io.github.example.Clock"), "Clock");
        assert!(!catalog.known("org.example.Editor"));
        assert!(catalog.known("vendor-Nested"));
        assert_eq!(
            catalog
                .sorted()
                .iter()
                .map(|info| info.name.as_str())
                .collect::<Vec<_>>(),
            ["Clock", "My Sound", "Nested"]
        );
    }

    #[test]
    fn unknown_ids_get_readable_fallback_labels() {
        assert_eq!(fallback_label("com.system76.CosmicAppletAudio"), "Audio");
        assert_eq!(
            fallback_label("com.system76.CosmicPanelWorkspacesButton"),
            "Workspaces Button"
        );
        assert_eq!(
            fallback_label("io.github.cosmic_utils.minimon-applet"),
            "Minimon"
        );
        assert_eq!(
            fallback_label("io.github.workstation.PersonalQuotaMonitor"),
            "Personal Quota Monitor"
        );
        assert_eq!(fallback_label("plain"), "Plain");
        assert_eq!(fallback_label(""), "");
    }

    #[test]
    fn xdg_application_dirs_keep_order_without_duplicates() {
        assert_eq!(
            application_dirs(
                Some(PathBuf::from("/home/u/.local/share")),
                Some("/var/lib/flatpak/exports/share:/usr/share/:/usr/share".into())
            ),
            [
                PathBuf::from("/home/u/.local/share/applications"),
                PathBuf::from("/var/lib/flatpak/exports/share/applications"),
                PathBuf::from("/usr/share/applications"),
            ]
        );
    }
}
