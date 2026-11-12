//! Private, local recovery of unapplied Settings work. These records never
//! apply anything: the original draft (including its read baselines) is
//! reopened and refreshed through the same conflict checks as navigation.
//! Each page owns one bounded versioned record. Read-only previews never
//! read or write live records; fixtures use only their own state directory.
use super::{Page, Settings};
use crate::desktop::DesktopStore;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) const RECOVERED: &str =
    "Your changes that weren’t applied are back. Apply or discard them; nothing has changed yet.";
const FORMAT: &str = "desktop-cards-settings-draft";
const VERSION: u32 = 1;
const MAX_BYTES: usize = 4 * 1024 * 1024;
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Kind {
    Dock,
    Appearance,
    Windows,
    Workspaces,
    Wallpaper,
    DesktopIcons,
    WindowRules,
    Shortcuts,
    Layout,
    Canvas,
    Automation,
    Preset,
    Previews,
    Recipe,
}
impl Kind {
    pub(super) fn key(self) -> &'static str {
        match self {
            Self::Dock => "dock-panel",
            Self::Appearance => "appearance",
            Self::Windows => "windows",
            Self::Workspaces => "workspaces",
            Self::Wallpaper => "wallpaper",
            Self::DesktopIcons => "desktop-icons",
            Self::WindowRules => "window-rules",
            Self::Shortcuts => "shortcuts",
            Self::Layout => "layout",
            Self::Canvas => "canvas",
            Self::Automation => "automation",
            Self::Preset => "preset",
            Self::Previews => "previews",
            Self::Recipe => "recipe",
        }
    }
    pub(super) fn title(self) -> &'static str {
        match self {
            // Named the way the sidebar and page headings are.
            Self::Dock => Page::DesktopDock.title(),
            Self::Appearance => Page::DesktopAppearance.title(),
            Self::Windows => Page::DesktopWindows.title(),
            Self::Workspaces => Page::DesktopWorkspaces.title(),
            Self::Wallpaper => Page::DesktopWallpaper.title(),
            Self::DesktopIcons => Page::DesktopIcons.title(),
            Self::WindowRules => Page::DesktopWindowRules.title(),
            Self::Shortcuts => "Keyboard shortcuts",
            Self::Layout => Page::WidgetLayout.title(),
            Self::Canvas => "Widget positions",
            Self::Automation => Page::DesktopAutomation.title(),
            Self::Preset => Page::DesktopProfiles.title(),
            Self::Previews => "Appearance and look previews",
            Self::Recipe => "Widget design preview",
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: String,
    version: u32,
    kind: String,
    /// RON supports typed enum/map keys while preserving opaque original
    /// COSMIC values verbatim, including unsupported or malformed values.
    draft: String,
}

/// The cache avoids filesystem writes for navigation, ticks and pointer
/// movement. An unreadable record is retained until new work replaces it;
/// merely opening the app must not erase recovery evidence.
#[derive(Default)]
pub(super) struct SessionDrafts {
    known: BTreeMap<Kind, Option<String>>,
    unreadable: BTreeSet<Kind>,
    pub recovered: usize,
    pub error: Option<String>,
}
impl SessionDrafts {
    fn load<T: DeserializeOwned>(
        &mut self,
        store: &DesktopStore,
        kind: Kind,
    ) -> Result<Option<T>, String> {
        let result = read::<T>(store, kind);
        match result {
            Ok(Some((draft, raw))) => {
                self.known.insert(kind, Some(raw));
                self.unreadable.remove(&kind);
                Ok(Some(draft))
            }
            Ok(None) => {
                self.known.insert(kind, None);
                self.unreadable.remove(&kind);
                Ok(None)
            }
            Err(error) => {
                self.unreadable.insert(kind);
                Err(format!("{} draft: {error}", kind.title()))
            }
        }
    }
    /// Removes a record for good, readable or not.
    fn forget(&mut self, store: &DesktopStore, kind: Kind) -> Result<(), String> {
        let path = path(store, kind)?;
        match fs::remove_file(&path) {
            Ok(()) => sync_parent(&path)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        self.known.insert(kind, None);
        self.unreadable.remove(&kind);
        Ok(())
    }
    fn checkpoint<T: Serialize>(
        &mut self,
        store: &DesktopStore,
        kind: Kind,
        draft: Option<&T>,
    ) -> Result<(), String> {
        if !store.writable() {
            return Ok(());
        }
        let raw = draft
            .map(ron::to_string)
            .transpose()
            .map_err(|error| error.to_string())?;
        if self.known.get(&kind) == Some(&raw) {
            return Ok(());
        }
        if raw.is_none() && (self.unreadable.contains(&kind) || !self.known.contains_key(&kind)) {
            return Ok(());
        }
        let path = path(store, kind)?;
        match &raw {
            Some(raw) => {
                let record = Record {
                    format: FORMAT.into(),
                    version: VERSION,
                    kind: kind.key().into(),
                    draft: raw.clone(),
                };
                let bytes = serde_json::to_vec(&record).map_err(|error| error.to_string())?;
                if bytes.len() > MAX_BYTES {
                    return Err("This draft is too large to keep between sessions".into());
                }
                atomic_private(&path, &bytes)?;
            }
            None => match fs::remove_file(&path) {
                Ok(()) => sync_parent(&path)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            },
        }
        self.known.insert(kind, raw);
        self.unreadable.remove(&kind);
        Ok(())
    }
}

fn path(store: &DesktopStore, kind: Kind) -> Result<PathBuf, String> {
    let path = store
        .state_file(&format!("settings-form-{}.json", kind.key()))
        .map_err(|error| error.to_string())?;
    for candidate in [store.state_dir(), path.as_path()] {
        match fs::symlink_metadata(candidate) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err("The saved draft path is a symbolic link".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(path)
}
fn read<T: DeserializeOwned>(
    store: &DesktopStore,
    kind: Kind,
) -> Result<Option<(T, String)>, String> {
    if !store.writable() {
        return Ok(None);
    }
    let path = path(store, kind)?;
    let file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
        return Err("The saved draft is not a supported size or file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("The saved draft is too large".into());
    }
    let record: Record = serde_json::from_slice(&bytes)
        .map_err(|_| "The saved draft cannot be read; the desktop is unchanged")?;
    if record.format != FORMAT || record.version != VERSION || record.kind != kind.key() {
        return Err("The saved draft belongs to a different version or page".into());
    }
    let draft = ron::from_str(&record.draft).map_err(|error| {
        format!("The saved draft cannot be decoded ({error}); the desktop is unchanged")
    })?;
    Ok(Some((draft, record.draft)))
}

fn sync_parent(path: &Path) -> Result<(), String> {
    fs::File::open(path.parent().ok_or("No draft folder")?)
        .and_then(|dir| dir.sync_all())
        .map_err(|error| error.to_string())
}
fn atomic_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("No draft folder")?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(
        ".settings-draft-{}-{}.tmp",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| error.to_string())?;
        // Do not replace a symlink introduced while writing the temporary.
        if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err("The saved draft path became a symbolic link".into());
        }
        fs::rename(&temporary, path).map_err(|error| error.to_string())?;
        sync_parent(path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

impl Settings {
    /// Called once at startup before the initial page and media tasks open.
    /// Only editors with a saved draft are initialized, avoiding discovery
    /// work for pages the user has never edited.
    pub(super) fn restore_session_drafts(&mut self) {
        let Ok(store) = DesktopStore::from_env() else {
            return;
        };
        if !store.writable() {
            return;
        }
        let mut session = std::mem::take(&mut self.session_drafts);
        let mut errors = Vec::new();
        macro_rules! restore {
            ($kind:ident, $editor:ident, $restore:expr) => {
                match session.load(&store, Kind::$kind) {
                    Ok(Some(saved)) => {
                        self.$editor.open();
                        match $restore(self, saved) {
                            Ok(()) => session.recovered += 1,
                            Err(error) => {
                                session.unreadable.insert(Kind::$kind);
                                errors.push(format!("{} draft: {error}", Kind::$kind.title()));
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(error) => errors.push(error),
                }
            };
        }
        restore!(Dock, desktop, |this: &mut Self, saved| this
            .desktop
            .restore_session(saved));
        restore!(Appearance, system_appearance, |this: &mut Self, saved| this
            .system_appearance
            .restore_session(saved));
        restore!(Windows, windows, |this: &mut Self, saved| this
            .windows
            .restore_session(saved));
        restore!(Workspaces, workspaces, |this: &mut Self, saved| this
            .workspaces
            .core
            .restore_session(saved));
        restore!(Wallpaper, wallpaper, |this: &mut Self, saved| this
            .wallpaper
            .restore_session(saved));
        restore!(DesktopIcons, desktop_icons, |this: &mut Self, saved| this
            .desktop_icons
            .core
            .restore_session(saved));
        restore!(WindowRules, window_rules, |this: &mut Self, saved| this
            .window_rules
            .restore_session(saved));
        restore!(Shortcuts, shortcuts, |this: &mut Self, saved| this
            .shortcuts
            .restore_session(saved));
        restore!(Automation, automation, |this: &mut Self, saved| this
            .automation
            .restore_session(saved));
        restore!(Preset, profiles, |this: &mut Self, saved| this
            .profiles
            .restore_session(saved));
        match session.load::<Vec<super::profiles::previews::SavedPreview>>(&store, Kind::Previews) {
            Ok(Some(saved)) => {
                self.profiles.open();
                match self.profiles.restore_previews(saved) {
                    Ok(()) => session.recovered += 1,
                    Err(error) => {
                        session.unreadable.insert(Kind::Previews);
                        errors.push(format!("{}: {error}", Kind::Previews.title()));
                    }
                }
            }
            Ok(None) => {}
            Err(error) => errors.push(error),
        }
        // An unfinished layout is kept without asking (see
        // `keep_recovered_layout`). A damaged one is dropped: the current
        // layout stays, and the record goes so it can't come back.
        let layout = match session.load(&store, Kind::Layout) {
            Ok(Some(saved)) => match self.placement.restore_session(saved) {
                Ok(()) => {
                    self.keep_recovered_layout();
                    Some(self.status.clone())
                }
                Err(_) => {
                    self.drop_recovered_layout();
                    Some(super::placement::DROPPED_LAYOUT.to_string())
                }
            },
            Ok(None) => None,
            Err(_) => Some(super::placement::DROPPED_LAYOUT.to_string()),
        };
        if layout.is_some()
            && let Err(error) = session.forget(&store, Kind::Layout)
        {
            errors.push(format!("{}: {error}", Kind::Layout.title()));
        }
        match session.load(&store, Kind::Canvas) {
            Ok(Some(saved)) => match self.restore_canvas(saved) {
                Ok(()) => session.recovered += 1,
                Err(error) => {
                    session.unreadable.insert(Kind::Canvas);
                    errors.push(format!("{}: {error}", Kind::Canvas.title()));
                }
            },
            Ok(None) => {}
            Err(error) => errors.push(error),
        }
        match session.load::<super::recipes::SavedDraft>(&store, Kind::Recipe) {
            Ok(Some(saved)) => match self.recipe_editor.restore_draft(saved) {
                Ok(()) => session.recovered += 1,
                Err(error) => {
                    session.unreadable.insert(Kind::Recipe);
                    errors.push(format!("{}: {error}", Kind::Recipe.title()));
                }
            },
            Ok(None) => {}
            Err(error) => errors.push(error),
        }
        if let Some(note) = layout {
            self.status = note;
            self.error = false;
        }
        if session.recovered > 0 {
            self.status = format!(
                "{} {} not applied yet came back · continue where you left off",
                session.recovered,
                if session.recovered == 1 {
                    "change"
                } else {
                    "changes"
                }
            );
            self.error = false;
        }
        if !errors.is_empty() {
            session.error = Some(errors.join(" · "));
            self.status = session.error.clone().unwrap();
            self.error = true;
        }
        self.session_drafts = session;
    }

    /// Called after settings messages and once at exit. Files for clean
    /// editors are removed after Apply / Save / Discard succeeds; failed
    /// actions retain the draft on its original baselines.
    pub(super) fn checkpoint_session_drafts(&mut self) {
        let Ok(store) = DesktopStore::from_env() else {
            return;
        };
        if !store.writable() {
            return;
        }
        let previous_error = self.session_drafts.error.clone();
        let mut errors = Vec::new();
        macro_rules! save {
            ($kind:ident, $opened:expr, $draft:expr) => {
                if $opened
                    && let Err(error) = self.session_drafts.checkpoint(&store, Kind::$kind, $draft)
                {
                    errors.push(format!("{}: {error}", Kind::$kind.title()));
                }
            };
        }
        save!(
            Appearance,
            self.system_appearance.opened(),
            self.system_appearance
                .session_draft()
                .as_ref()
                .filter(|saved| !saved.hex.is_empty())
        );
        save!(
            Windows,
            self.windows.opened(),
            self.windows
                .session_draft()
                .as_ref()
                .filter(|saved| saved.typed.delay.is_some() || saved.typed.snap.is_some())
        );
        save!(
            Wallpaper,
            self.wallpaper.opened(),
            self.wallpaper.session_draft().as_ref()
        );
        save!(
            WindowRules,
            self.window_rules.opened(),
            self.window_rules.session_draft().as_ref()
        );
        save!(
            Shortcuts,
            self.shortcuts.opened(),
            self.shortcuts.session_draft().as_ref()
        );
        save!(
            Layout,
            self.canvas.is_none() && !self.placement.dragging(),
            self.placement.session_draft().as_ref()
        );
        save!(
            Canvas,
            !self.placement.dragging(),
            self.canvas_saved().as_ref()
        );
        save!(Recipe, true, self.recipe_editor.saved_draft().as_ref());
        save!(
            Automation,
            self.automation.opened(),
            self.automation.draft.as_ref().filter(|draft| draft.dirty())
        );
        // The older single-selection record is replaced by this list once read.
        if self.profiles.store().is_some() {
            let previews = self.profiles.saved_previews();
            match self.session_drafts.checkpoint(
                &store,
                Kind::Previews,
                Some(&previews).filter(|list| !list.is_empty()),
            ) {
                Ok(()) => save!(Preset, true, None::<&super::profiles::SavedSelection>),
                Err(error) => errors.push(format!("{}: {error}", Kind::Previews.title())),
            }
        }
        if !errors.is_empty() {
            self.session_drafts.error = Some(format!(
                "Could not keep changes not applied yet for next time: {}",
                errors.join(" · ")
            ));
            self.status = self.session_drafts.error.clone().unwrap();
            self.error = true;
        } else if self.session_drafts.unreadable.is_empty() {
            self.session_drafts.error = None;
            if previous_error.as_deref() == Some(self.status.as_str()) {
                self.status = "Changes not applied yet are kept for next time".into();
                self.error = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::appearance::{AppearanceDraft, Control, SystemMode, Value};

    #[test]
    fn retained_preview_checkpoint_load_preserves_native_and_widget_baselines() {
        use crate::desktop::profiles::{self, Domain, ProfileLibrary};
        use crate::settings::profiles::{
            Message, ProfilesEditor, Selection, previews::PreviewKind,
        };
        let fixture = Fixture::new("ux-preview-checkpoint");
        let dock = "com.system76.CosmicPanel.Dock";
        fixture.system("com.system76.CosmicPanel", "entries", "[\"Dock\"]");
        fixture.system(dock, "anchor", "Bottom");
        fixture.user(dock, "padding", "4");
        let config = fixture.0.join("config.json");
        fs::write(&config, r#"{"appearance":{"opacity":92}}"#).unwrap();
        let store = fixture.store().with_app_config(config.clone());
        let profile = profiles::import_local(
            serde_json::json!({
                "format": "desktop-cards-profile", "version": profiles::VERSION,
                "name": "Checkpoint", "dock": {"padding": 8},
                "widget_style": {"appearance": {"opacity": 70}}
            })
            .to_string()
            .as_bytes(),
        )
        .unwrap();
        let selection = Selection::Saved(ProfileLibrary::new(&store).create(&profile).unwrap());
        let mut editor = ProfilesEditor::with_store(store.clone());
        editor.enter(PreviewKind::Setup);
        editor.update(Message::Select(selection));
        editor.update(Message::Include(Domain::Dock, true));
        let mut session = SessionDrafts::default();
        session
            .checkpoint(&store, Kind::Previews, Some(&editor.saved_previews()))
            .unwrap();
        let record: Record =
            serde_json::from_slice(&fs::read(path(&store, Kind::Previews).unwrap()).unwrap())
                .unwrap();
        assert!(
            !record.draft.is_empty(),
            "outer JSON carries RON as a string"
        );
        fixture.user(dock, "padding", "6");
        fs::write(config, r#"{"appearance":{"opacity":90}}"#).unwrap();
        let saved = SessionDrafts::default()
            .load(&store, Kind::Previews)
            .unwrap()
            .unwrap();
        let mut reopened = ProfilesEditor::with_store(store.clone());
        reopened.restore_previews(saved).unwrap();
        reopened.enter(PreviewKind::Setup);
        assert_eq!(reopened.conflict_values().len(), 2);
        assert!(!reopened.can_apply());
        assert!(store.latest_change().unwrap().is_none());
    }
    use crate::desktop::desktop_icons::{DesktopIconsDraft, Show};
    use crate::desktop::panels::{PanelDraft, PanelKind, PanelSize};
    use crate::desktop::shortcuts::{Choice as ShortcutChoice, ShortcutsDraft, format::Binding};
    use crate::desktop::store::{Owner, tests::Fixture};
    use crate::desktop::wallpaper::{BgColor, BgSource, Target, WallpaperDraft};
    use crate::desktop::window_rules::{CustomRule, WindowRulesDraft};
    use crate::desktop::windows::{Behavior, Choice, WindowsDraft};
    use crate::desktop::workspaces::{WorkspaceLayout, WorkspacesDraft};
    use crate::settings::drafts::DraftEditor;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

    fn roundtrip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(
        state: &mut SessionDrafts,
        store: &DesktopStore,
        kind: Kind,
        draft: &T,
    ) {
        state.checkpoint(store, kind, Some(draft)).unwrap();
        let mut reopened = SessionDrafts::default();
        let loaded: T = reopened.load(store, kind).unwrap().unwrap();
        assert_eq!(&loaded, draft);
        assert_eq!(
            fs::metadata(path(store, kind).unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn session_drafts_roundtrip_all_desktop_domains_without_applying() {
        let fixture = Fixture::new("session-roundtrip");
        let store = fixture.store();
        // Unsupported original values are observations, kept byte-for-byte.
        fixture.user(
            "com.system76.CosmicComp",
            "active_hint",
            "unrecognized original value",
        );
        fixture.system(
            "com.system76.CosmicPanel",
            "entries",
            "[\"Panel\",\"Dock\"]",
        );
        let original = fixture.read("com.system76.CosmicComp", "active_hint");
        let mut state = SessionDrafts::default();
        let mut windows = WindowsDraft::load(&store).unwrap();
        windows.set(Behavior::ShowMaximize, Choice::Flag(false));
        roundtrip(&mut state, &store, Kind::Windows, &windows);
        let mut appearance = AppearanceDraft::load(&store).unwrap();
        appearance.set(Control::Mode, Value::Mode(SystemMode::Light));
        roundtrip(&mut state, &store, Kind::Appearance, &appearance);
        let mut dock = PanelDraft::load(&store, PanelKind::Dock).unwrap();
        dock.set_size(PanelSize::S);
        roundtrip(&mut state, &store, Kind::Dock, &vec![dock]);
        let mut workspaces = WorkspacesDraft::load(&store).unwrap();
        workspaces.set_layout(WorkspaceLayout::Horizontal);
        roundtrip(&mut state, &store, Kind::Workspaces, &workspaces);
        let mut icons = DesktopIconsDraft::load(&store).unwrap();
        icons.set_show(Show::Trash, false);
        roundtrip(&mut state, &store, Kind::DesktopIcons, &icons);
        let mut rules = WindowRulesDraft::load(&store).unwrap();
        rules
            .save(
                None,
                CustomRule {
                    appid: "^saved-test$".into(),
                    title: ".*".into(),
                    enabled: true,
                },
            )
            .unwrap();
        roundtrip(&mut state, &store, Kind::WindowRules, &rules);
        let mut wallpaper = WallpaperDraft::load(&store).unwrap();
        wallpaper.set_source(
            &Target::Shared,
            BgSource::Color(BgColor::Single([0.1, 0.2, 0.3])),
        );
        roundtrip(&mut state, &store, Kind::Wallpaper, &wallpaper);
        let mut shortcuts = ShortcutsDraft::load(&store).unwrap();
        let binding = Binding::parse("Super+F12").unwrap();
        shortcuts
            .save_command(
                None,
                "Example",
                &binding,
                "echo fixture",
                ShortcutChoice::default(),
            )
            .unwrap();
        roundtrip(&mut state, &store, Kind::Shortcuts, &shortcuts);
        assert_eq!(
            fixture.read("com.system76.CosmicComp", "active_hint"),
            original
        );
        assert!(store.latest_change().unwrap().is_none());
        assert!(store.pending_recovery().unwrap().is_none());
        assert_eq!(fs::read_dir(store.state_dir()).unwrap().count(), 8);
    }

    #[test]
    fn session_drafts_reopen_preserves_baseline_and_clear_after_apply_or_discard() {
        let fixture = Fixture::new("session-reopen");
        let store = fixture.store();
        let mut editor =
            DraftEditor::<WindowsDraft>::with_store(Owner::Windows, "Windows", fixture.store());
        editor.edit(|draft| draft.set(Behavior::ActiveHint, Choice::Flag(false)));
        let mut state = SessionDrafts::default();
        state
            .checkpoint(&store, Kind::Windows, editor.draft())
            .unwrap();
        let record = path(&store, Kind::Windows).unwrap();
        let inode = fs::metadata(&record).unwrap().ino();
        state
            .checkpoint(&store, Kind::Windows, editor.draft())
            .unwrap();
        assert_eq!(
            fs::metadata(&record).unwrap().ino(),
            inode,
            "navigation does not rewrite an unchanged draft"
        );
        drop(editor);
        let mut reopened =
            DraftEditor::<WindowsDraft>::with_store(Owner::Windows, "Windows", fixture.store());
        let saved: WindowsDraft = state.load(&store, Kind::Windows).unwrap().unwrap();
        reopened.restore_session(saved).unwrap();
        assert!(reopened.dirty());
        assert_eq!(fixture.read("com.system76.CosmicComp", "active_hint"), None);
        assert!(reopened.apply());
        state
            .checkpoint(
                &store,
                Kind::Windows,
                reopened.draft().filter(|draft| draft.dirty()),
            )
            .unwrap();
        assert!(!record.exists());
        assert_eq!(
            fixture
                .read("com.system76.CosmicComp", "active_hint")
                .unwrap()
                .trim(),
            "false"
        );
        reopened.edit(|draft| draft.set(Behavior::ShowMaximize, Choice::Flag(false)));
        state
            .checkpoint(&store, Kind::Windows, reopened.draft())
            .unwrap();
        reopened.discard();
        state
            .checkpoint(
                &store,
                Kind::Windows,
                reopened.draft().filter(|draft| draft.dirty()),
            )
            .unwrap();
        assert!(!record.exists());
    }

    #[test]
    fn session_drafts_external_change_while_closed_is_not_silently_rebased() {
        let fixture = Fixture::new("session-conflict");
        let store = fixture.store();
        let mut editor =
            DraftEditor::<WindowsDraft>::with_store(Owner::Windows, "Windows", fixture.store());
        editor.edit(|draft| draft.set(Behavior::ActiveHint, Choice::Flag(false)));
        editor.edit(|draft| draft.set(Behavior::ShowMaximize, Choice::Flag(false)));
        let mut state = SessionDrafts::default();
        state
            .checkpoint(&store, Kind::Windows, editor.draft())
            .unwrap();
        fixture.user("com.system76.CosmicComp", "active_hint", "true");
        let saved: WindowsDraft = state.load(&store, Kind::Windows).unwrap().unwrap();
        let mut reopened =
            DraftEditor::<WindowsDraft>::with_store(Owner::Windows, "Windows", fixture.store());
        reopened.restore_session(saved).unwrap();
        assert!(
            reopened
                .draft()
                .unwrap()
                .conflicts()
                .contains(&Behavior::ActiveHint.key())
        );
        assert!(
            !reopened.apply(),
            "outside edits require conflict resolution"
        );
        assert_eq!(
            fixture
                .read("com.system76.CosmicComp", "active_hint")
                .as_deref(),
            Some("true")
        );
        assert_eq!(fixture.read("com.system76.CosmicTk", "show_maximize"), None);
        assert!(reopened.dirty(), "the independent maximize edit remains");
    }

    #[test]
    fn session_drafts_records_are_bounded_versioned_and_corruption_is_retained() {
        let fixture = Fixture::new("session-bad-record");
        let store = fixture.store();
        let mut state = SessionDrafts::default();
        let mut draft = WindowsDraft::load(&store).unwrap();
        draft.set(Behavior::ActiveHint, Choice::Flag(false));
        state
            .checkpoint(&store, Kind::Windows, Some(&draft))
            .unwrap();
        let file = path(&store, Kind::Windows).unwrap();
        let valid = fs::read(&file).unwrap();
        let mut record: Record = serde_json::from_slice(&valid).unwrap();
        record.version += 1;
        fs::write(&file, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(
            SessionDrafts::default()
                .load::<WindowsDraft>(&store, Kind::Windows)
                .is_err()
        );
        fs::write(&file, b"broken").unwrap();
        let mut reopened = SessionDrafts::default();
        assert!(
            reopened
                .load::<WindowsDraft>(&store, Kind::Windows)
                .is_err()
        );
        reopened
            .checkpoint::<WindowsDraft>(&store, Kind::Windows, None)
            .unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"broken");
        fs::write(&file, vec![b'x'; MAX_BYTES + 1]).unwrap();
        assert!(
            reopened
                .load::<WindowsDraft>(&store, Kind::Windows)
                .is_err()
        );
        fs::write(&file, valid).unwrap();
        assert!(
            reopened
                .load::<WindowsDraft>(&store, Kind::Windows)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn session_drafts_read_only_previews_and_escaped_fixture_paths_are_isolated() {
        let fixture = Fixture::new("session-read-only");
        let store = fixture.store();
        let mut draft = WindowsDraft::load(&store).unwrap();
        draft.set(Behavior::ActiveHint, Choice::Flag(false));
        let mut state = SessionDrafts::default();
        state
            .checkpoint(&store, Kind::Windows, Some(&draft))
            .unwrap();
        let before = fixture.tree();
        let preview = fixture.store().read_only("fixture preview");
        assert!(
            state
                .load::<WindowsDraft>(&preview, Kind::Windows)
                .unwrap()
                .is_none()
        );
        state
            .checkpoint(&preview, Kind::Windows, Some(&draft))
            .unwrap();
        assert_eq!(fixture.tree(), before);
        let escaped = Fixture::new("session-escape");
        symlink(store.state_dir(), escaped.0.join("state")).unwrap();
        assert!(
            state
                .checkpoint(&escaped.store(), Kind::Appearance, Some(&draft))
                .is_err()
        );
        assert!(read::<WindowsDraft>(&escaped.store(), Kind::Windows).is_err());
        assert_eq!(fixture.tree(), before);
    }
    #[test]
    fn recovered_incomplete_numeric_and_color_fields_stay_visible_and_unapplied() {
        use crate::desktop::appearance::{ColorRole, ThemeVariant};
        use crate::settings::{system_appearance, wallpaper, windows};
        let fixture = Fixture::new("session-typed-input");
        let mut windows = windows::WindowsEditor::with_store(fixture.store());
        windows.update(windows::Message::DelayInput(String::new()));
        let raw = ron::to_string(&windows.session_draft().unwrap()).unwrap();
        let mut reopened = windows::WindowsEditor::with_store(fixture.store());
        reopened
            .restore_session(ron::from_str(&raw).unwrap())
            .unwrap();
        assert!(reopened.dirty() && reopened.typed_error().is_some());
        assert!(!reopened.can_apply());
        reopened.update(windows::Message::Discard);
        assert!(reopened.session_draft().is_none());

        let mut appearance = system_appearance::AppearanceEditor::with_store(fixture.store());
        appearance.update(system_appearance::Message::Variant(ThemeVariant::Light));
        appearance.update(system_appearance::Message::HexInput(
            ColorRole::Accent,
            "#F".into(),
        ));
        let raw = ron::to_string(&appearance.session_draft().unwrap()).unwrap();
        let mut reopened = system_appearance::AppearanceEditor::with_store(fixture.store());
        reopened
            .restore_session(ron::from_str(&raw).unwrap())
            .unwrap();
        assert_eq!(reopened.variant, ThemeVariant::Light);
        assert_eq!(reopened.hex_text(ColorRole::Accent), Some("#F"));
        assert!(reopened.unfinished_variant(ThemeVariant::Light));
        assert!(reopened.dirty() && !reopened.can_apply());
        let before = fixture.tree();
        reopened.update(system_appearance::Message::Apply);
        assert_eq!(fixture.tree(), before);
        reopened.update(system_appearance::Message::Discard);
        assert!(reopened.session_draft().is_none());

        let mut wallpaper = wallpaper::WallpaperEditor::with_store(fixture.store());
        wallpaper.update(wallpaper::Message::Select(wallpaper::Pick::Color(
            BgColor::Single([0.1, 0.2, 0.3]),
        )));
        wallpaper.update(wallpaper::Message::Stop(0, "#F".into()));
        let raw = ron::to_string(&wallpaper.session_draft().unwrap()).unwrap();
        let mut reopened = wallpaper::WallpaperEditor::with_store(fixture.store());
        reopened
            .restore_session(ron::from_str(&raw).unwrap())
            .unwrap();
        assert!(reopened.typed_error().is_some());
        assert!(reopened.dirty() && !reopened.can_apply());
        reopened.update(wallpaper::Message::Discard);
        assert!(reopened.session_draft().is_none());
        assert!(fixture.store().latest_change().unwrap().is_none());
    }

    #[test]
    fn recovered_forms_keep_text_but_not_stale_confirmations_or_picker_state() {
        use crate::desktop::shortcuts::Ask;
        use crate::settings::{shortcuts, window_rules};
        let fixture = Fixture::new("session-open-forms");
        let mut rules = window_rules::WindowRulesEditor::with_store(fixture.store());
        rules.update(window_rules::Message::Add);
        rules.update(window_rules::Message::AppId("org.example.App".into()));
        rules.update(window_rules::Message::Title("unfinished title".into()));
        rules.update(window_rules::Message::Picker(true));
        let raw = ron::to_string(&rules.session_draft().unwrap()).unwrap();
        let mut reopened = window_rules::WindowRulesEditor::with_store(fixture.store());
        reopened
            .restore_session(ron::from_str(&raw).unwrap())
            .unwrap();
        let form = reopened.form.as_ref().unwrap();
        assert_eq!(form.app_id, "org.example.App");
        assert_eq!(form.title, "unfinished title");
        assert!(!form.picker);
        assert!(!reopened.can_apply());
        reopened.update(window_rules::Message::Cancel);
        assert!(reopened.session_draft().is_none());

        let mut shortcuts = shortcuts::ShortcutsEditor::with_store(fixture.store());
        shortcuts.update(shortcuts::Message::Add);
        shortcuts.update(shortcuts::Message::Name("My shortcut".into()));
        shortcuts.update(shortcuts::Message::Combo("Super+F12".into()));
        shortcuts.update(shortcuts::Message::Command("echo pending".into()));
        shortcuts.form.as_mut().unwrap().ask = Some(Ask::Replace {
            combo: "Super+F12".into(),
            current: "another action".into(),
        });
        let raw = ron::to_string(&shortcuts.session_draft().unwrap()).unwrap();
        let mut reopened = shortcuts::ShortcutsEditor::with_store(fixture.store());
        reopened
            .restore_session(ron::from_str(&raw).unwrap())
            .unwrap();
        let form = reopened.form.as_ref().unwrap();
        assert_eq!(form.name, "My shortcut");
        assert_eq!(form.command, "echo pending");
        assert!(
            form.ask.is_none(),
            "old confirmation is not authority after reopening"
        );
        assert!(!reopened.can_apply());
        assert!(!fixture.0.join("state").exists());
    }

    #[test]
    fn recovered_unsubmitted_shortcut_cannot_rebase_over_an_external_edit() {
        use crate::desktop::shortcuts::{self, Target};
        use crate::settings::shortcuts::{Message, ShortcutsEditor};
        let fixture = Fixture::new("session-form-baseline");
        let store = fixture.store();
        let binding = Binding::parse("Super+F12").unwrap();
        let target = Target::Entry(binding.ident());
        let mut seed = ShortcutsDraft::load(&store).unwrap();
        seed.save_command(
            None,
            "Original",
            &binding,
            "echo original",
            ShortcutChoice::default(),
        )
        .unwrap();
        store.apply(&seed.plan().patch).unwrap();
        let mut editor = ShortcutsEditor::with_store(fixture.store());
        editor.update(Message::Edit(target));
        editor.update(Message::Command("echo my unsaved change".into()));
        let raw = ron::to_string(&editor.session_draft().unwrap()).unwrap();
        // Change the original command while Settings is closed.
        let mut outside = ShortcutsDraft::load(&store).unwrap();
        outside
            .save_command(
                Some(target),
                "Original",
                &binding,
                "echo outside",
                ShortcutChoice::default(),
            )
            .unwrap();
        store.apply(&outside.plan().patch).unwrap();
        let before = fixture.read(shortcuts::NAMESPACE, "custom");
        let mut reopened = ShortcutsEditor::with_store(fixture.store());
        reopened
            .restore_session(ron::from_str(&raw).unwrap())
            .unwrap();
        // A navigation or watcher refresh still cannot turn the changed
        // command into this recovered form's new baseline.
        reopened.update(Message::Refresh);
        reopened.update(Message::Save);
        assert!(reopened.dirty());
        reopened.update(Message::Apply);
        assert_eq!(fixture.read(shortcuts::NAMESPACE, "custom"), before);
        assert!(
            matches!(reopened.notice(), Some(crate::settings::desktop::Notice::Error(message)) if message.contains("changed outside"))
        );
    }

    #[test]
    fn session_startup_recovers_unopened_page_and_checkpoint_clears_discard() {
        use crate::settings::{Message, Page, windows};
        use cosmic::Application;
        const CHILD: &str = "DESKTOP_CARDS_DRAFT_RECOVERY_TEST";
        if std::env::var_os(CHILD).is_none() {
            let fixture = Fixture::new("session-settings-startup");
            let config = fixture.0.join("config.json");
            fs::write(&config, b"{}").unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("--exact")
                .arg("settings::session_drafts::tests::session_startup_recovers_unopened_page_and_checkpoint_clears_discard")
                .arg("--nocapture")
                .env(CHILD, "1")
                .env(crate::desktop::store::FIXTURE_ENV, &fixture.0)
                .env("DESKTOP_CARDS_CONFIG", &config)
                .status().unwrap();
            assert!(status.success());
            return;
        }
        let store = DesktopStore::from_env().unwrap();
        assert!(store.fixture_root().is_some());
        let mut editor = windows::WindowsEditor::with_store(DesktopStore::from_env().unwrap());
        editor.update(windows::Message::Flag(Behavior::ActiveHint, false));
        let mut recovery = SessionDrafts::default();
        recovery
            .checkpoint(&store, Kind::Windows, editor.session_draft().as_ref())
            .unwrap();
        let file = path(&store, Kind::Windows).unwrap();
        let (mut settings, _) = Settings::init(cosmic::app::Core::default(), Page::Overview);
        assert_eq!(settings.page, Page::Overview);
        assert!(settings.windows.opened() && settings.windows.dirty());
        assert_eq!(settings.session_drafts.recovered, 1);
        assert!(settings.session_drafts.error.is_none());
        assert!(!store.user_path(&Behavior::ActiveHint.key()).exists());
        let _ = settings.update(Message::Page(Page::DesktopWindows));
        let _ = settings.update(Message::Windows(windows::Message::Discard));
        assert!(!settings.windows.dirty());
        assert!(!file.exists());
        assert!(!store.user_path(&Behavior::ActiveHint.key()).exists());
    }
}
