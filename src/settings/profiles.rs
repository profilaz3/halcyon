//! Local theme/setup chooser, library and read-only previews. Use commits
//! the selected scope through the shared desktop action/history path. Media
//! results are versioned and never apply a preview on their own.
use crate::config::{self, Config};
use crate::desktop::appearance::AppearanceDraft;
use crate::desktop::panels::{PanelDraft, PanelKind};
use crate::desktop::profiles::themes::LookColor;
use crate::desktop::profiles::{
    self, Domain, Installed, MAX_PROFILE_BYTES, Preset, Profile, ProfileEntry, ProfileId,
    ProfileLibrary, ProfilePlan,
};
use crate::desktop::store::{Journal, KeyEntry, Snapshot, widget};
use crate::desktop::wallpaper::media::{self, Pixels};
use crate::desktop::wallpaper::{self, BgEntry, BgSource};
use crate::desktop::{DesktopError, DesktopPatch, DesktopStore, LatestChange, Owner};
use crate::settings::desktop::Notice;
use cosmic::widget::image::Handle;
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub mod discovery;
pub mod mix;
pub mod packages;
mod sharing;

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);
/// Backdrops fill a Settings scene. Four stable handles cap retained CPU pixels
/// at about 14 MiB; grid thumbnails keep their separate 320×180 bound.
const BACKDROPS: usize = 4;

/// What the preview shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Selection {
    /// Nothing chosen: the current desktop, read only.
    #[default]
    Current,
    Preset(&'static str),
    Saved(ProfileId),
}

/// What a submitted name is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameAction {
    /// Save the applied desktop as a new local profile.
    Capture,
    CaptureTheme,
    Rename(ProfileId),
    /// A new local copy of a preset or saved profile.
    Duplicate,
    SaveMix,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameEdit {
    pub action: NameAction,
    pub text: String,
}

#[derive(Clone, Debug)]
pub enum Message {
    DiscoveryQuery(String),
    RemixSelected,
    Package(packages::Edit),
    MixStart,
    MixPart(profiles::mix::Component),
    MixClosePicker,
    MixPick(profiles::mix::Component, Selection),
    MixQuery(String),
    MixPage(usize),
    Select(Selection),
    /// Local browser/view state; never included in a desktop transaction.
    Collection(bool),
    Compare(bool),
    /// `true` follows the wallpaper; `false` uses the style's own color.
    ThemeColors(bool),
    ThemeScope(bool),
    /// A fixed color: one of the wallpaper's hues or a custom one.
    ThemeAccent([u8; 3]),
    /// Opens (`Some`) or closes the custom color field, with its text.
    ThemeCustom(Option<String>),
    ThemeWallpaper(bool),
    ResetSurface(Domain),
    RepairThemeMatching,
    ThemeReady(
        u64,
        Result<crate::desktop::wallpaper::palette::Suggestion, String>,
    ),
    Include(Domain, bool),
    Details(bool),
    /// Opens (`Some`) or closes the name field.
    Name(Option<NameAction>),
    NameInput(String),
    NameSubmit,
    /// `false` asks for confirmation (or withdraws it); `true` deletes.
    Delete(bool),
    Import,
    ImportTheme,
    Export,
    ShareImage,
    CancelShare,
    ShareReady(u64, Result<Vec<u8>, String>),
    /// A file chooser finished: request ID and the file's bytes, `None`
    /// when cancelled.
    Imported(u64, Result<Option<Vec<u8>>, String>),
    /// A save chooser finished: request ID and the written file.
    Exported(u64, Result<Option<PathBuf>, String>),
    Apply,
    Discard,
    Undo,
    /// Plans again after the desktop changed under the preview.
    Replan,
    Recover,
    KeepRecovery,
    /// A backdrop picture finished: request ID and the picture, `None`
    /// when the background can't be previewed.
    Backdrop(u64, Option<Pixels>),
}

impl Message {
    pub fn blocking(&self) -> bool {
        self.writes()
            || matches!(
                self,
                Self::MixStart
                    | Self::RemixSelected
                    | Self::MixPick(..)
                    | Self::Package(packages::Edit::Open)
            )
    }
    /// Whether the message may write the desktop, the library or a file.
    pub fn writes(&self) -> bool {
        matches!(
            self,
            Self::NameSubmit
                | Self::Package(packages::Edit::Export)
                | Self::Delete(true)
                | Self::Import
                | Self::Export
                | Self::Imported(..)
                | Self::Exported(..)
                | Self::Apply
                | Self::Undo
                | Self::Recover
                | Self::KeepRecovery
        )
    }
}

/// A native dialog the settings window must run for the editor. Each
/// carries the ID its result must come back with.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    ExportImage {
        id: u64,
        file_name: String,
        bytes: Vec<u8>,
        confine: Option<PathBuf>,
    },
    Import {
        id: u64,
    },
    Export {
        id: u64,
        file_name: String,
        bytes: Vec<u8>,
        /// A fixture root the chosen file must be inside.
        confine: Option<PathBuf>,
    },
}

/// What the settings window should do after an update.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    None,
    /// The desktop or widget settings changed: reload them elsewhere.
    Changed,
    /// Focus the name field.
    FocusName,
    Dialog(Request),
}

#[derive(Clone, Debug, PartialEq)]
enum Pending {
    Import(bool),
    /// The profile name the bytes were made from and what a shared profile
    /// left on this computer, for the notice.
    Export(String, Vec<&'static str>),
    Image(String, bool),
}

#[derive(Clone, Debug)]
pub struct ThemeJob {
    id: u64,
    source: Option<wallpaper::automatic::Source>,
    cancel: tokio::sync::watch::Receiver<bool>,
}
impl ThemeJob {
    #[cfg(test)]
    pub fn run(self) -> Message {
        let result = self
            .source
            .ok_or("Wallpaper is unavailable".to_string())
            .and_then(|s| s.analyse());
        Message::ThemeReady(self.id, result)
    }
}
/// Wallpaper colors are offered from the applied wallpaper before any
/// selection, and from the planned one once a look is selected.
#[derive(Clone, Debug, PartialEq)]
struct Analysis {
    stamp: u64,
    suggestion: wallpaper::palette::Suggestion,
}
pub fn theme_task<X: Into<Y> + 'static, Y: 'static>(
    job: ThemeJob,
    wrap: fn(Message) -> X,
) -> cosmic::Task<Y> {
    cosmic::task::future(async move {
        let result = match job.source {
            Some(source) => source.analyse_preview(job.cancel).await,
            None => Err("Wallpaper is unavailable".into()),
        };
        wrap(Message::ThemeReady(job.id, result))
    })
}

/// What the miniature's backdrop shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Backdrop {
    /// No included wallpaper that can be applied.
    #[default]
    Neutral,
    /// The planned shared background (drawn once its picture is ready; a
    /// background that can't be previewed stays neutral).
    Planned(BgEntry),
}

/// A theme picture belongs to the source/version used for its color analysis,
/// not just the path in its wallpaper entry. Home and broad presets have no
/// theme analysis and cannot satisfy a versioned theme picture request.
#[derive(Clone, Debug, PartialEq)]
struct BackdropKey {
    entry: BgEntry,
    source: Option<wallpaper::automatic::Source>,
    version: Option<PictureVersion>,
}

/// Metadata catches replacement (including equal-size files with preserved
/// mtime), in-place edits and transitions between missing and readable files.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PictureVersion {
    device: u64,
    inode: u64,
    bytes: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

impl BackdropKey {
    fn unanalysed(entry: BgEntry) -> Self {
        let version = Self::version(&entry);
        Self {
            entry,
            source: None,
            version,
        }
    }
    fn version(entry: &BgEntry) -> Option<PictureVersion> {
        let BgSource::Path(path) = &entry.source else {
            return None;
        };
        let metadata = fs::metadata(path).ok()?;
        Some(PictureVersion {
            device: metadata.dev(),
            inode: metadata.ino(),
            bytes: metadata.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        })
    }
    fn current(&self) -> bool {
        self.version == Self::version(&self.entry)
    }
}

/// One cancellable request for the current home or candidate backdrop.
#[derive(Clone, Debug)]
pub struct BackdropJob {
    pub id: u64,
    key: BackdropKey,
    cancel: tokio::sync::watch::Receiver<bool>,
}

impl BackdropJob {
    #[cfg(test)]
    pub fn run(self) -> Message {
        let pixels = (!*self.cancel.borrow() && self.cancel.has_changed().is_ok())
            .then(|| backdrop_entry(&self.key))
            .flatten()
            .and_then(|entry| media::worker::pixels(&entry));
        Message::Backdrop(self.id, pixels)
    }
}

/// Decode in a cancellable child; selection changes never wait for its pixels.
pub fn backdrop_task<X: Into<Y> + 'static, Y: 'static>(
    job: BackdropJob,
    wrap: fn(Message) -> X,
) -> cosmic::Task<Y> {
    cosmic::task::future(async move {
        let id = job.id;
        let pixels = if let Some(entry) = backdrop_entry(&job.key) {
            media::worker::render(entry, job.cancel).await
        } else {
            None
        };
        wrap(Message::Backdrop(id, pixels))
    })
}

/// The background as cosmic-bg shows it on a 16:9 display: a colour or
/// gradient rendered, a picture scaled with its fit. Themes use their analyzed
/// picture; other previews use a folder's first image. Bundled pictures use the
/// installed source too, preserving detail at the larger scene size.
fn backdrop_entry(key: &BackdropKey) -> Option<BgEntry> {
    let mut entry = key.entry.clone();
    if let Some(source) = &key.source {
        // Use the exact analyzed slideshow frame or image. A bundled
        // thumbnail must not stand in for a file that has been replaced.
        if matches!(entry.source, BgSource::Path(_)) {
            entry.source = BgSource::Path(source.picture()?.to_path_buf());
        }
    }
    Some(entry)
}

/// The selected look drawn over the current desktop: both bars, the theme
/// and the widget preferences, each after the plan's actual changes.
#[derive(Clone, Debug)]
pub struct Preview {
    pub bars: Vec<PanelDraft>,
    pub appearance: Option<AppearanceDraft>,
    pub config: Option<Config>,
}

/// A preset choice is an intention, not an applied transaction. On reopen
/// its plan is rebuilt read-only and always waits for an explicit Review.
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct SavedSelection {
    pub(super) preset: bool,
    pub(super) id: String,
    pub(super) excluded: Vec<String>,
    #[serde(default)]
    pub(super) theme: Option<String>,
}

#[derive(Clone, Default)]
pub struct ProfilesEditor {
    pub package: Option<packages::Draft>,
    pub mix: Option<mix::Draft>,
    share_pending: Option<(u64, String, bool)>,
    share_source: Option<super::sharing::Scene>,
    share_cancel: Option<tokio::sync::watch::Sender<bool>>,
    share_jobs: Vec<super::sharing::Job>,
    store: Option<Result<DesktopStore, String>>,
    /// The Islands panel component answered when the looks were fitted.
    islands: bool,
    /// A widget style leaves the dock and panel as they are
    /// (Colors & theme › "Style the dock and panel too" off).
    keep_bars: bool,
    theme_override: Option<Profile>,
    theme_jobs: Vec<ThemeJob>,
    theme_pending: Option<u64>,
    theme_source: Option<wallpaper::automatic::Source>,
    theme_error: Option<String>,
    /// The latest analysis for the colors the page offers.
    analysis: Option<Analysis>,
    /// Analysis of the applied wallpaper, before a look is selected.
    current_pending: Option<(u64, u64)>,
    /// Why the applied wallpaper offers no colors, when it can't.
    current_error: Option<String>,
    /// The custom color field's text while it is open.
    pub custom: Option<String>,
    pub custom_surfaces: BTreeSet<Domain>,
    pub installed: Installed,
    presets: Vec<Preset>,
    /// `None` before the first read.
    library: Option<Result<Vec<ProfileEntry>, String>>,
    selection: Selection,
    /// The selected saved profile, or why it can't be read.
    loaded: Option<Result<Profile, String>>,
    /// Parts the user turned off for this selection.
    excluded: BTreeSet<Domain>,
    plan: Option<ProfilePlan>,
    preview: Option<Preview>,
    /// The desktop changed since the plan was made; Apply waits for Replan.
    stale: bool,
    session_review: bool,
    pub notice: Option<Notice>,
    /// Lines from the last capture or recovery, shown with the notice.
    pub notes: Vec<String>,
    pub details: bool,
    pub saved_collection: bool,
    pub discovery_query: String,
    pub compare_current: bool,
    pub name: Option<NameEdit>,
    pub confirm_delete: bool,
    /// Automation rules that apply the profile being deleted.
    pub rules_using: usize,
    pending: Option<(u64, Pending)>,
    next_request: u64,
    undo: Option<LatestChange>,
    elsewhere: Option<(Owner, String)>,
    recovery: Option<Result<Journal, String>>,
    backdrop: Backdrop,
    backdrop_key: Option<BackdropKey>,
    /// Finished backdrop pictures, newest last; `None`: no preview.
    backdrops: Vec<(BackdropKey, Option<Handle>)>,
    backdrop_pending: Option<(u64, BackdropKey)>,
    jobs: Vec<BackdropJob>,
    /// The applied shared background used by Home's placement canvas.
    home_background: Option<BackdropKey>,
    home_pending: Option<(u64, BackdropKey)>,
    backdrop_cancel: Option<tokio::sync::watch::Sender<bool>>,
    home_cancel: Option<tokio::sync::watch::Sender<bool>>,
    theme_cancel: Option<tokio::sync::watch::Sender<bool>>,
    current_cancel: Option<tokio::sync::watch::Sender<bool>>,
}

impl ProfilesEditor {
    /// First visit: choose the store and discover installed components.
    /// Every visit re-reads the library, the latest change and the plan.
    /// Reads only.
    pub fn open(&mut self) {
        if self.store.is_none() {
            let store = DesktopStore::from_env().map_err(|error| error.to_string());
            if let Ok(store) = &store {
                self.installed = Installed::discover(store);
            }
            self.store = Some(store);
        }
        // Fitted to the current preferences on every visit, so adding
        // project folders brings Projects back into Workbench.
        let config = crate::config::Config::load();
        self.keep_bars = !config.appearance.style_bars;
        self.fit_presets(&config);
        self.refresh();
    }
    /// Colors from the applied wallpaper, for the color row and the style
    /// specimens before a look is selected. Reads only.
    fn queue_current_colors(&mut self) {
        let Some(store) = self.store() else {
            return;
        };
        let Ok(draft) = wallpaper::WallpaperDraft::load(store) else {
            return;
        };
        let source = wallpaper::automatic::source(store, &draft);
        let known = self
            .analysis
            .as_ref()
            .is_some_and(|analysis| analysis.stamp == source.stamp);
        let making = self
            .current_pending
            .is_some_and(|(_, stamp)| stamp == source.stamp);
        if known || making {
            return;
        }
        self.next_request = self.next_request.wrapping_add(1);
        let id = self.next_request;
        self.current_pending = Some((id, source.stamp));
        let (cancel, receiver) = tokio::sync::watch::channel(false);
        if let Some(old) = self.current_cancel.replace(cancel) {
            let _ = old.send(true);
        }
        self.theme_jobs.push(ThemeJob {
            id,
            source: Some(source),
            cancel: receiver,
        });
        self.retire_theme_jobs();
    }
    /// Wallpaper colors for what the page shows: the planned wallpaper of
    /// the selected look, or the applied one.
    pub fn suggestion(&self) -> Option<&wallpaper::palette::Suggestion> {
        self.analysis.as_ref().map(|analysis| &analysis.suggestion)
    }
    /// Why the applied wallpaper offers no colors.
    pub fn colors_unavailable(&self) -> Option<&str> {
        self.current_error.as_deref()
    }
    /// The built-in looks fitted to `config` and the installed wallpapers,
    /// then the styles Colors & theme offers.
    fn fit_presets(&mut self, config: &crate::config::Config) {
        let bundled = self.store().map(wallpaper::bundled_dir).unwrap_or_default();
        self.islands = self.store().is_some_and(profiles::islands_available);
        let islands = self.islands;
        self.presets = profiles::studio_presets()
            .into_iter()
            .map(profiles::discovery::credited)
            .map(|preset| profiles::fitted(preset, config, &bundled, islands))
            .chain(profiles::themes::presets())
            .chain(profiles::adaptive::presets())
            .collect();
    }
    /// Whether choosing a widget style also restyles the dock and panel.
    pub fn set_style_bars(&mut self, on: bool) {
        if self.keep_bars == on {
            self.keep_bars = !on;
            self.replan();
        }
    }
    /// Whether the Islands panel component answered when the looks were fitted.
    pub fn islands(&self) -> bool {
        self.islands
    }
    /// The bundled wallpapers' folder for this store.
    pub fn bundled_dir(&self) -> std::path::PathBuf {
        self.store().map(wallpaper::bundled_dir).unwrap_or_default()
    }
    #[cfg(test)]
    fn with_store(store: DesktopStore) -> Self {
        let mut editor = Self {
            installed: Installed::discover(&store),
            store: Some(Ok(store)),
            ..Default::default()
        };
        editor.fit_presets(&crate::config::Config::default());
        editor.refresh();
        editor
    }
    #[cfg(test)]
    pub(super) fn session_draft(&self) -> Option<SavedSelection> {
        let (preset, id) = match &self.selection {
            Selection::Current => return None,
            Selection::Preset(id) => (true, id.to_string()),
            Selection::Saved(id) => (false, id.as_str().to_string()),
        };
        let pending = self.session_review
            || self
                .plan
                .as_ref()
                .is_some_and(|plan| !plan.is_empty() || !plan.blocking_reasons().is_empty());
        pending.then(|| SavedSelection {
            theme: self
                .theme_override
                .as_ref()
                .and_then(|p| serde_json::to_string(p).ok()),
            preset,
            id,
            excluded: self
                .excluded
                .iter()
                .map(|domain| domain.key().into())
                .collect(),
        })
    }
    pub(super) fn restore_session(&mut self, saved: SavedSelection) -> Result<(), String> {
        let selection = if saved.preset {
            let preset = self
                .presets
                .iter()
                .find(|preset| preset.id == saved.id)
                .ok_or("The saved preset is no longer available")?;
            Selection::Preset(preset.id)
        } else {
            let id = ProfileId::parse(&saved.id).map_err(|error| error.to_string())?;
            if !self
                .library()
                .is_ok_and(|entries| entries.iter().any(|entry| entry.id == id))
            {
                return Err("The saved preset is no longer in your library".into());
            }
            Selection::Saved(id)
        };
        let excluded: Result<BTreeSet<_>, _> = saved
            .excluded
            .iter()
            .map(|name| {
                Domain::parse(name)
                    .ok_or_else(|| "The saved preset refers to an unsupported section".to_string())
            })
            .collect();
        self.select(selection);
        if let Some(text) = saved.theme {
            let p = profiles::import_local(text.as_bytes()).map_err(|e| e.to_string())?;
            if !p.appearance_only {
                return Err("Saved theme selection is not a theme".into());
            }
            // A built-in style is planned again from today's roles; only the
            // draft's own choices carry over, since an earlier release may
            // have saved its colors (and no window outline) for that style.
            // A Reset of one surface stays as it was saved: its scope and the
            // color fields it leaves out are its intent.
            match self.theme_override.as_mut() {
                Some(fresh)
                    if matches!(self.selection, Selection::Preset(_))
                        && profiles::themes::selects_style(&p)
                        && profiles::themes::family(fresh) == profiles::themes::family(&p) =>
                {
                    carry_choices(&p, fresh);
                }
                _ => self.theme_override = Some(p),
            }
        }
        self.excluded = excluded?;
        self.queue_theme_colors();
        self.session_review = true;
        self.replan();
        self.notice = Some(Notice::Done("Recovered your preset selection. Review its current changes before applying; nothing has been applied.".into()));
        Ok(())
    }

    /// Home only needs the applied wallpaper. Avoid loading the theme, bars
    /// and a second widget config for this canvas. The preset plan is untouched.
    pub fn open_home(&mut self) {
        if self.store.is_none() {
            self.open();
        }
        let Some(store) = self.store() else {
            self.home_background = None;
            return;
        };
        let background = wallpaper::WallpaperDraft::load(store)
            .ok()
            .and_then(|draft| match draft.effective(&wallpaper::Target::Shared) {
                crate::desktop::panels::Setting::Known(entry) => {
                    Some(BackdropKey::unanalysed(entry))
                }
                crate::desktop::panels::Setting::Unsupported(_) => None,
            });
        self.home_background = background.clone();
        let Some(entry) = background else {
            return;
        };
        let kept = self.backdrops.iter().any(|(known, _)| *known == entry);
        let making = [&self.backdrop_pending, &self.home_pending]
            .into_iter()
            .flatten()
            .any(|(_, known)| *known == entry);
        if kept || making {
            return;
        }
        self.next_request = self.next_request.wrapping_add(1);
        let id = self.next_request;
        self.home_pending = Some((id, entry.clone()));
        let (cancel, receiver) = tokio::sync::watch::channel(false);
        if let Some(old) = self.home_cancel.replace(cancel) {
            let _ = old.send(true);
        }
        self.jobs.push(BackdropJob {
            id,
            key: entry,
            cancel: receiver,
        });
        self.retire_backdrop_jobs();
    }
    /// The current shared background's picture, when it has one.
    pub fn home_image(&self) -> Option<&Handle> {
        let entry = self.home_background.as_ref()?;
        self.backdrops
            .iter()
            .find(|(known, _)| known == entry)
            .and_then(|(_, handle)| handle.as_ref())
    }

    pub fn store(&self) -> Option<&DesktopStore> {
        self.store.as_ref().and_then(|store| store.as_ref().ok())
    }
    pub fn unavailable(&self) -> Option<&str> {
        self.store
            .as_ref()
            .and_then(|store| store.as_ref().err())
            .map(String::as_str)
    }
    pub fn writable(&self) -> bool {
        self.store().is_some_and(DesktopStore::writable)
    }
    pub fn presets(&self) -> &[Preset] {
        &self.presets
    }
    pub fn library(&self) -> Result<&[ProfileEntry], &str> {
        match &self.library {
            Some(Ok(entries)) => Ok(entries),
            Some(Err(error)) => Err(error),
            None => Ok(&[]),
        }
    }
    pub fn selection(&self) -> &Selection {
        &self.selection
    }
    /// The selected profile, if it can be read.
    pub fn profile(&self) -> Option<&Profile> {
        if let Some(profile) = &self.theme_override {
            return Some(profile);
        }
        match &self.selection {
            Selection::Current => None,
            Selection::Preset(id) => self
                .presets
                .iter()
                .find(|preset| preset.id == *id)
                .map(|preset| &preset.profile),
            Selection::Saved(_) => self.loaded.as_ref().and_then(|loaded| loaded.as_ref().ok()),
        }
    }
    /// Why the selected saved profile can't be read.
    pub fn load_error(&self) -> Option<&str> {
        match (&self.selection, &self.loaded) {
            (Selection::Saved(_), Some(Err(error))) => Some(error),
            _ => None,
        }
    }
    pub fn description(&self) -> Option<&'static str> {
        match &self.selection {
            Selection::Preset(id) => self
                .presets
                .iter()
                .find(|preset| preset.id == *id)
                .map(|preset| preset.description),
            _ => None,
        }
    }
    pub fn included(&self, domain: Domain) -> bool {
        !self.excluded.contains(&domain)
    }
    /// The parts the plan covers: the profile's sections the user kept on.
    pub fn selected_domains(&self) -> BTreeSet<Domain> {
        self.profile()
            .map(|profile| {
                profile
                    .domains()
                    .into_iter()
                    .filter(|domain| !self.excluded.contains(domain))
                    .filter(|domain| {
                        !(self.keep_bars
                            && profile.appearance_only
                            && matches!(domain, Domain::Dock | Domain::Panel))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn plan(&self) -> Option<&ProfilePlan> {
        self.plan.as_ref()
    }
    pub fn preview(&self) -> Option<&Preview> {
        self.preview.as_ref()
    }
    pub fn stale(&self) -> bool {
        self.stale
    }
    pub fn recovery(&self) -> Option<&Result<Journal, String>> {
        self.recovery.as_ref()
    }
    /// What the miniature's backdrop represents.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn backdrop(&self) -> &Backdrop {
        &self.backdrop
    }
    /// The backdrop's finished picture, if it has one.
    pub(super) fn depth_background(&self, current: bool) -> Option<(String, BgEntry)> {
        let key = if current {
            self.home_background.as_ref()
        } else {
            self.backdrop_key.as_ref().or(self.home_background.as_ref())
        }?;
        self.backdrops
            .iter()
            .find(|(known, _)| known == key)?
            .1
            .as_ref()?;
        Some((format!("{key:?}"), key.entry.clone()))
    }
    pub fn backdrop_image(&self) -> Option<&Handle> {
        let key = self.backdrop_key.as_ref()?;
        self.backdrops
            .iter()
            .find(|(known, _)| known == key)
            .and_then(|(_, handle)| handle.as_ref())
    }
    /// Background work the last messages started (drained by the app).
    pub fn take_jobs(&mut self) -> Vec<BackdropJob> {
        self.retire_backdrop_jobs();
        std::mem::take(&mut self.jobs)
    }
    fn retire_backdrop_jobs(&mut self) {
        for (pending, wanted, cancel) in [
            (
                &mut self.home_pending,
                &self.home_background,
                &mut self.home_cancel,
            ),
            (
                &mut self.backdrop_pending,
                &self.backdrop_key,
                &mut self.backdrop_cancel,
            ),
        ] {
            if pending
                .as_ref()
                .is_none_or(|(_, key)| wanted.as_ref() != Some(key))
            {
                if let Some(old) = cancel.take() {
                    let _ = old.send(true);
                }
                *pending = None;
            }
        }
        self.jobs
            .retain(|job| !*job.cancel.borrow() && job.cancel.has_changed().is_ok());
    }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn latest(&self) -> Option<&LatestChange> {
        self.undo.as_ref()
    }
    /// A latest change another page made; its Undo lives there.
    #[cfg(test)]
    pub fn elsewhere(&self) -> Option<&(Owner, String)> {
        self.elsewhere.as_ref().filter(|_| self.undo.is_none())
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some() || self.share_pending.is_some()
    }
    /// Parts Apply would actually change, for the overlap check.
    #[cfg(test)]
    pub fn apply_domains(&self) -> BTreeSet<Domain> {
        self.plan
            .iter()
            .flat_map(|plan| &plan.domains)
            .filter(|part| !part.changes.is_empty())
            .map(|part| part.domain)
            .collect()
    }
    /// Parts this page's Undo would change, for the overlap check.
    #[cfg(test)]
    pub fn undo_domains(&self) -> BTreeSet<Domain> {
        self.undo.as_ref().map(change_domains).unwrap_or_default()
    }
    pub fn can_apply(&self) -> bool {
        self.writable()
            && self.recovery.is_none()
            && !self.stale
            && self.pending.is_none()
            && self.theme_pending.is_none()
            && self.theme_error.is_none()
            && self.plan.as_ref().is_some_and(ProfilePlan::can_apply)
    }
    pub fn can_undo(&self) -> bool {
        self.undo.is_some() && self.recovery.is_none() && self.writable() && self.pending.is_none()
    }
    /// Changes whenever anything visible changes; used by capture mode.
    pub fn state_key(&self) -> String {
        format!(
            "{:?}:{:?}:{:?}:{:?}:{:?}:{}:{:?}:{}:{}:{:?}:{:?}:{}:{:?}:{}:{}:{}:{:?}:{}",
            self.selection,
            self.excluded,
            self.plan,
            self.notice,
            self.notes,
            self.details,
            self.name,
            self.confirm_delete,
            self.stale,
            self.pending,
            self.undo.as_ref().map(|undo| &undo.id),
            self.recovery.is_some(),
            self.backdrop_key,
            self.backdrop_image().is_some(),
            self.saved_collection,
            self.compare_current,
            self.mix.as_ref().map(|m| (
                m.picking,
                m.choosing,
                &m.query,
                m.page,
                m.sources.each_ref().map(|s| s.as_ref().map(|s| &s.origin))
            )),
            self.discovery_query,
        )
    }

    /// Re-reads the library, the latest change and the selected profile,
    /// then plans again. Reads only.
    pub fn refresh(&mut self) {
        let Some(store) = self.store() else {
            return;
        };
        let library = ProfileLibrary::new(store)
            .list()
            .map_err(|error| error.to_string());
        self.library = Some(library);
        self.refresh_recovery();
        if let Selection::Saved(id) = &self.selection {
            let exists = self
                .library()
                .is_ok_and(|entries| entries.iter().any(|entry| entry.id == *id));
            if exists {
                let id = id.clone();
                self.load(&id);
            } else {
                // Deleted elsewhere: back to the current desktop.
                self.selection = Selection::Current;
                self.loaded = None;
            }
        }
        self.replan();
        self.refresh_surface_status();
        if self.theme_source.is_none() {
            self.queue_current_colors();
        }
    }
    fn refresh_surface_status(&mut self) {
        let Some(store) = self.store() else {
            return;
        };
        let Ok((_, config)) = wallpaper::automatic::read_config(store) else {
            return;
        };
        let Some(f) = config.appearance.family else {
            self.custom_surfaces.clear();
            return;
        };
        let record = profiles::themes::record(f);
        let plan = profiles::plan(
            store,
            &self.installed,
            &record,
            &[Domain::Dock, Domain::Panel].into_iter().collect(),
        );
        // Bars paint the owned variant's surfaces; windows show the accent
        // and the active outline. Each is Custom on its own.
        let native = AppearanceDraft::load(store)
            .map(|draft| profiles::themes::native_custom(&draft, config.appearance))
            .unwrap_or_default();
        self.custom_surfaces = plan
            .domains
            .iter()
            .filter(|p| {
                matches!(p.domain, Domain::Dock | Domain::Panel)
                    && (native.surfaces
                        || !p.changes.is_empty()
                        || config.appearance.overrides
                            & (0x1f << if p.domain == Domain::Dock { 8 } else { 16 })
                            != 0)
            })
            .map(|p| p.domain)
            .collect();
        if native.windows {
            self.custom_surfaces.insert(Domain::Appearance);
        }
    }
    fn refresh_recovery(&mut self) {
        let Some(store) = self.store() else {
            return;
        };
        let recovery = store
            .pending_recovery()
            .map_err(|error| error.to_string())
            .transpose();
        let latest = store.latest_change().ok().flatten();
        self.elsewhere = latest
            .as_ref()
            .filter(|latest| latest.owner != Owner::Profile)
            .map(|latest| (latest.owner, latest.label.clone()));
        self.undo = latest.filter(|latest| latest.owner == Owner::Profile);
        self.recovery = recovery;
    }
    fn load(&mut self, id: &ProfileId) {
        let loaded = match self.store() {
            Some(store) => ProfileLibrary::new(store)
                .load(id)
                .map_err(|error| error.to_string()),
            None => Err("Desktop settings unavailable".into()),
        };
        self.loaded = Some(loaded);
    }
    /// Plans the selection against the current desktop and builds the
    /// preview from the same plan. Reads only.
    fn replan(&mut self) {
        self.stale = self.session_review;
        let Some(store) = self.store() else {
            self.plan = None;
            self.preview = None;
            return;
        };
        let plan = self.profile().map(|profile| {
            profiles::plan(store, &self.installed, profile, &self.selected_domains())
        });
        let preview = preview(store, plan.as_ref());
        self.plan = plan;
        self.preview = Some(preview);
        self.want_backdrop();
    }
    /// The plan's background as the backdrop; its picture is requested
    /// unless it is kept or already being made.
    fn want_backdrop(&mut self) {
        self.backdrop_key = self
            .theme_source
            .as_ref()
            .and_then(|source| {
                source.background.clone().map(|entry| BackdropKey {
                    version: BackdropKey::version(&entry),
                    entry,
                    source: Some(source.clone()),
                })
            })
            .or_else(|| {
                self.plan
                    .as_ref()
                    .and_then(ProfilePlan::background)
                    .cloned()
                    .map(BackdropKey::unanalysed)
            })
            .or_else(|| {
                self.home_background
                    .as_ref()
                    .map(|key| BackdropKey::unanalysed(key.entry.clone()))
            });
        self.backdrop = self
            .backdrop_key
            .as_ref()
            .map(|key| Backdrop::Planned(key.entry.clone()))
            .unwrap_or(Backdrop::Neutral);
        if self
            .backdrop_pending
            .as_ref()
            .is_some_and(|(_, pending)| Some(pending) != self.backdrop_key.as_ref())
        {
            self.backdrop_pending = None;
        }
        let Some(key) = &self.backdrop_key else {
            return;
        };
        let kept = self.backdrops.iter().any(|(known, _)| known == key);
        let making = self
            .backdrop_pending
            .as_ref()
            .is_some_and(|(_, known)| known == key);
        if kept || making {
            return;
        }
        self.next_request = self.next_request.wrapping_add(1);
        let id = self.next_request;
        self.backdrop_pending = Some((id, key.clone()));
        let (cancel, receiver) = tokio::sync::watch::channel(false);
        if let Some(old) = self.backdrop_cancel.replace(cancel) {
            let _ = old.send(true);
        }
        self.jobs.push(BackdropJob {
            id,
            key: key.clone(),
            cancel: receiver,
        });
        self.retire_backdrop_jobs();
    }
    fn keep_backdrop(&mut self, key: BackdropKey, pixels: Option<Pixels>) {
        let handle =
            pixels.map(|pixels| Handle::from_rgba(pixels.width, pixels.height, pixels.rgba));
        self.backdrops.retain(|(known, _)| *known != key);
        self.backdrops.push((key, handle));
        if self.backdrops.len() > BACKDROPS {
            self.backdrops.remove(0);
        }
    }
    fn backdrop_ready(&mut self, id: u64, pixels: Option<Pixels>) {
        if self
            .home_pending
            .as_ref()
            .is_some_and(|(pending, _)| *pending == id)
        {
            if let Some((_, entry)) = self.home_pending.take() {
                self.home_cancel = None;
                if entry.current() {
                    self.keep_backdrop(entry, pixels);
                } else {
                    self.open_home();
                }
            }
            return;
        }
        let Some((pending, entry)) = self.backdrop_pending.take() else {
            return;
        };
        if pending != id {
            self.backdrop_pending = Some((pending, entry));
            return;
        }
        self.backdrop_cancel = None;
        if entry.source.is_some() && !self.check_theme_source() {
            return;
        }
        if !entry.current() {
            self.want_backdrop();
            return;
        }
        self.keep_backdrop(entry, pixels);
        // The selection may have moved on while it was made.
        self.want_backdrop();
    }

    fn select(&mut self, selection: Selection) {
        self.package = None;
        self.cancel_share();
        // Style and color are separate choices: a pending look's color and
        // matching-wallpaper choice carry over to another built-in style,
        // otherwise the applied look's color does (see `preserve_custom`).
        let pending = self
            .profile()
            .filter(|p| p.appearance_only && profiles::themes::family(p).is_some())
            .cloned();
        self.mix = None;
        self.theme_override = None;
        self.theme_pending = None;
        self.theme_source = None;
        self.theme_error = None;
        self.custom = None;
        self.session_review = false;
        self.excluded.clear();
        self.details = false;
        self.compare_current = false;
        self.confirm_delete = false;
        self.name = None;
        self.notes.clear();
        self.notice = None;
        self.loaded = None;
        if let Selection::Saved(id) = &selection {
            let id = id.clone();
            self.selection = selection;
            self.load(&id);
        } else {
            self.selection = selection;
        }
        // Choosing a built-in family moves linked styling only. Restoring a
        // saved variation deliberately uses its complete recorded appearance,
        // and a starter look (not appearance-only) applies its own colors.
        if matches!(self.selection, Selection::Preset(_))
            && let Some(mut p) = self
                .profile()
                .filter(|p| p.appearance_only && profiles::themes::family(p).is_some())
                .cloned()
            && let Some(store) = self.store()
        {
            match profiles::themes::preserve_custom(store, &self.installed, &mut p) {
                Ok(()) => {
                    self.first_theme_colors(&mut p);
                    if let Some(pending) = &pending {
                        carry_choices(pending, &mut p);
                    }
                    self.theme_override = Some(p);
                }
                Err(error) => {
                    self.loaded = Some(Err(error.to_string()));
                    self.notice = Some(Notice::Error(error.to_string()));
                    // Never fall back to an unmodified record on a failed read.
                    self.selection = Selection::Current;
                    self.replan();
                    return;
                }
            }
        }
        self.queue_theme_colors();
        self.replan();
    }
    /// The first theme keeps the color people already chose: following the
    /// wallpaper, or their own fixed accent. Someone who never chose gets
    /// colors from a wallpaper that has them. Later family changes keep the
    /// applied choice (see `preserve_custom`).
    fn first_theme_colors(&self, p: &mut Profile) {
        let Some(store) = self.store() else {
            return;
        };
        let Ok((_, config)) = wallpaper::automatic::read_config(store) else {
            return;
        };
        if config.appearance.family.is_some() {
            return;
        }
        if let Some(policy) = config.appearance.color_policy {
            let choice = match policy {
                crate::config::ColorPolicy::Wallpaper => LookColor::Wallpaper,
                crate::config::ColorPolicy::Theme => LookColor::Style,
                crate::config::ColorPolicy::Custom => {
                    LookColor::Fixed(config.appearance.accent.unwrap_or_else(|| {
                        crate::desktop_theme::bytes(
                            crate::config::Palette::named(&config.theme).accent,
                        )
                    }))
                }
            };
            choice.set(p);
            return;
        }
        let follow = config.appearance.wallpaper_colors
            || (config.appearance.accent.is_none()
                && self
                    .suggestion()
                    .is_some_and(|suggestion| !suggestion.swatches.is_empty()));
        if follow {
            LookColor::Wallpaper.set(p);
        } else if let Some(rgb) = config.appearance.accent {
            LookColor::Fixed(rgb).set(p);
        }
    }
    /// A pending Reset of one surface to the applied style that makes no
    /// color choice of its own (see `reset_surface`): the page shows the
    /// applied look's colors, and it takes no wallpaper colors.
    pub fn resetting(&self) -> bool {
        matches!(self.selection, Selection::Preset(_))
            && self.theme_override.as_ref().is_some_and(|p| {
                p.appearance_only
                    && !profiles::themes::selects_style(p)
                    && LookColor::of(p).is_none()
            })
    }
    /// Color and wallpaper choices edit the applied look when nothing else
    /// is selected.
    fn select_applied(&mut self) {
        if self.profile().is_some() {
            return;
        }
        let Some(store) = self.store() else {
            return;
        };
        let Ok((_, config)) = wallpaper::automatic::read_config(store) else {
            return;
        };
        if let Some(family) = config.appearance.family {
            self.select(Selection::Preset(family.preset()));
        }
    }
    pub fn take_theme_jobs(&mut self) -> Vec<ThemeJob> {
        self.retire_theme_jobs();
        std::mem::take(&mut self.theme_jobs)
    }
    fn retire_theme_jobs(&mut self) {
        for (pending, cancel) in [
            (self.theme_pending, &mut self.theme_cancel),
            (
                self.current_pending.map(|(id, _)| id),
                &mut self.current_cancel,
            ),
        ] {
            if pending.is_none() {
                if let Some(old) = cancel.take() {
                    let _ = old.send(true);
                }
            }
        }
        self.theme_jobs
            .retain(|job| !*job.cancel.borrow() && job.cancel.has_changed().is_ok());
    }
    pub fn theme_loading(&self) -> bool {
        self.theme_pending.is_some()
    }
    pub fn theme_problem(&self) -> Option<&str> {
        self.theme_error.as_deref()
    }
    fn planned_theme_source(&self) -> Option<wallpaper::automatic::Source> {
        let store = self.store()?;
        let p = self.profile()?;
        profiles::planned_wallpaper_source(
            store,
            &self.installed,
            p.wallpaper
                .as_ref()
                .filter(|_| !self.excluded.contains(&Domain::Wallpaper)),
        )
    }
    /// Check before accepting either asynchronous result and again at Apply.
    /// A file can change during decoding even without a new selection/request.
    fn check_theme_source(&mut self) -> bool {
        if self.theme_source.as_ref().is_none_or(|source| {
            self.planned_theme_source()
                .is_some_and(|current| current.stamp == source.stamp)
        }) {
            return true;
        }
        self.theme_pending = None;
        self.backdrop_pending = None;
        self.backdrop_key = None;
        self.backdrop = Backdrop::Neutral;
        self.stale = true;
        self.notice = Some(Notice::Error(
            "The wallpaper changed. Choose the theme again to preview its current colors.".into(),
        ));
        false
    }
    fn queue_theme_colors(&mut self) {
        self.theme_error = None;
        self.theme_pending = None;
        self.theme_source = None;
        if self.resetting()
            || !self.profile().is_some_and(|p| {
                p.composition.is_some()
                    || (p.appearance_only && profiles::themes::family(p).is_some())
            })
        {
            // Nothing selected or a surface Reset: offer the applied
            // wallpaper's colors.
            self.queue_current_colors();
            return;
        }
        let source = self.planned_theme_source();
        self.theme_source = source.clone();
        self.next_request = self.next_request.wrapping_add(1);
        let id = self.next_request;
        self.theme_pending = Some(id);
        let (cancel, receiver) = tokio::sync::watch::channel(false);
        if let Some(old) = self.theme_cancel.replace(cancel) {
            let _ = old.send(true);
        }
        self.theme_jobs.push(ThemeJob {
            id,
            source,
            cancel: receiver,
        });
        self.retire_theme_jobs();
    }
    fn theme_ready(&mut self, id: u64, result: Result<wallpaper::palette::Suggestion, String>) {
        if let Some((pending, stamp)) = self.current_pending
            && pending == id
        {
            self.current_pending = None;
            self.current_cancel = None;
            // A selection's own analysis takes precedence.
            match result {
                Ok(suggestion) if self.theme_source.is_none() => {
                    self.current_error = None;
                    self.analysis = Some(Analysis { stamp, suggestion });
                }
                Ok(_) => {}
                Err(reason) => self.current_error = Some(reason),
            }
            return;
        }
        if self.theme_pending != Some(id) {
            return;
        }
        if !self.check_theme_source() {
            return;
        }
        self.theme_pending = None;
        self.theme_cancel = None;
        let Some(mut p) = self.profile().cloned() else {
            return;
        };
        let auto = p
            .widget_style
            .as_ref()
            .and_then(|v| v.pointer("/appearance/wallpaper_colors"))
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        match result {
            Ok(suggestion) => {
                if let Some(source) = &self.theme_source {
                    self.analysis = Some(Analysis {
                        stamp: source.stamp,
                        suggestion: suggestion.clone(),
                    });
                }
                let style = p.widget_style.get_or_insert(serde_json::json!({}));
                let light = wallpaper::automatic::light(&suggestion);
                let _ = widget::put(style, "/appearance/wallpaper_light", Some(light.into()));
                if auto
                    && let Some(rgb) = wallpaper::automatic::recommended_for(
                        &suggestion,
                        profiles::themes::family(&p),
                    )
                {
                    profiles::themes::recolor(&mut p, Some(rgb));
                }
                self.theme_override = Some(p);
            }
            Err(reason) => {
                self.theme_source = None;
                if auto {
                    self.theme_error = Some(reason.clone());
                }
                self.notes = vec![reason];
            }
        }
        self.replan();
    }
    fn reset_surface(&mut self, domain: Domain) {
        let Some(store) = self.store() else {
            return;
        };
        let Ok((_, config)) = wallpaper::automatic::read_config(store) else {
            return;
        };
        let Some(f) = config.appearance.family else {
            return;
        };
        let mut p = profiles::themes::record(f);
        p.name = format!("Reset {} to {}", domain.title(), f.title());
        p.appearance_only = true;
        let mut look = profiles::themes::appearance(f, config.appearance.accent);
        for variant in [&mut look.light, &mut look.dark].into_iter().flatten() {
            match domain {
                // Bars paint the surfaces; the window outline is not theirs.
                Domain::Dock | Domain::Panel => variant.outline = None,
                // Windows: the accent and active outline, not the surfaces.
                _ => {
                    variant.background = None;
                    variant.primary = None;
                }
            }
        }
        p.appearance =
            matches!(domain, Domain::Dock | Domain::Panel | Domain::Appearance).then_some(look);
        match domain {
            Domain::Dock => {
                p.panel = None;
                p.widget_style = None;
            }
            Domain::Panel => {
                p.dock = None;
                p.widget_style = None;
            }
            Domain::Appearance => {
                p.name = format!("Reset windows to {}", f.title());
                p.dock = None;
                p.panel = None;
                p.widget_style = None;
            }
            Domain::WidgetStyle => {
                p.dock = None;
                p.panel = None;
                if let Some(style) = p
                    .widget_style
                    .as_mut()
                    .and_then(|v| v.get_mut("appearance"))
                    .and_then(serde_json::Value::as_object_mut)
                {
                    for key in [
                        "accent",
                        "color_source",
                        "sync_colors",
                        "wallpaper_colors",
                        "color_policy",
                    ] {
                        style.remove(key);
                    }
                }
            }
            _ => return,
        }
        // Reset owns only this surface's customization bits. Color policy and
        // coordination scope remain the applied choice.
        let mask = match domain {
            Domain::Dock => 0x1f << 8,
            Domain::Panel => 0x1f << 16,
            Domain::WidgetStyle => 0x1f,
            Domain::Appearance => (0x1f << 24) | (0x1f << 32),
            _ => 0,
        };
        let style = p.widget_style.get_or_insert(serde_json::json!({}));
        let _ = widget::put(
            style,
            "/appearance/overrides",
            Some(serde_json::json!(config.appearance.overrides & !mask)),
        );
        let _ = widget::put(
            style,
            "/appearance/sync_colors",
            Some(config.appearance.sync_colors.into()),
        );
        self.select(Selection::Preset(f.preset()));
        self.theme_override = Some(p);
        self.queue_theme_colors();
        self.replan();
    }

    fn reload_library(&mut self) {
        if let Some(store) = self.store() {
            self.library = Some(
                ProfileLibrary::new(store)
                    .list()
                    .map_err(|error| error.to_string()),
            );
        }
    }
    fn selected_name(&self) -> Option<String> {
        if let Some(profile) = self.profile() {
            return Some(profile.name.clone());
        }
        None
    }

    pub fn update(&mut self, message: Message) -> Outcome {
        // A changed candidate must never open a chooser for an earlier render.
        if !matches!(
            message,
            Message::ShareImage
                | Message::ShareReady(..)
                | Message::Backdrop(..)
                | Message::ThemeReady(..)
                | Message::Exported(..)
                | Message::Imported(..)
        ) {
            self.cancel_share();
        }
        match message {
            Message::ShareImage => self.share_image(),
            Message::CancelShare => self.cancel_share(),
            Message::ShareReady(id, result) => return self.share_ready(id, result),
            Message::Select(selection) => self.select(selection),
            Message::DiscoveryQuery(query) => {
                self.discovery_query = query.chars().take(80).collect()
            }
            Message::RemixSelected => self.start_remix(),
            Message::Collection(saved) => self.saved_collection = saved,
            Message::Compare(current) => self.compare_current = current,
            Message::ThemeColors(on) => {
                self.select_applied();
                if let Some(mut p) = self.profile().filter(|p| p.appearance_only).cloned() {
                    let style = p.widget_style.get_or_insert(serde_json::json!({}));
                    let _ = widget::put(style, "/appearance/wallpaper_colors", Some(on.into()));
                    let _ = widget::put(
                        style,
                        "/appearance/color_policy",
                        Some(serde_json::json!(if on { "wallpaper" } else { "theme" })),
                    );
                    if !on {
                        profiles::themes::recolor(&mut p, None);
                    }
                    self.custom = None;
                    self.theme_override = Some(p);
                    self.queue_theme_colors();
                    self.replan();
                }
            }
            Message::ThemeAccent(rgb) => {
                self.select_applied();
                if let Some(mut p) = self.profile().filter(|p| p.appearance_only).cloned()
                    && profiles::themes::family(&p).is_some()
                {
                    let style = p.widget_style.get_or_insert(serde_json::json!({}));
                    let _ = widget::put(style, "/appearance/wallpaper_colors", Some(false.into()));
                    let _ = widget::put(
                        style,
                        "/appearance/color_policy",
                        Some(serde_json::json!("custom")),
                    );
                    profiles::themes::recolor(&mut p, Some(rgb));
                    // A fixed color needs no new analysis, and a wallpaper
                    // that could not be read no longer holds Apply back.
                    self.theme_error = None;
                    self.theme_override = Some(p);
                    self.replan();
                }
            }
            Message::ThemeCustom(text) => match text {
                None => self.custom = None,
                Some(text) => {
                    let text: String = text.chars().filter(|c| !c.is_control()).take(9).collect();
                    if let Some(rgb) = crate::settings::wallpaper::parse_rgb(&text) {
                        let _ = self
                            .update(Message::ThemeAccent(rgb.map(|v| (v * 255.).round() as u8)));
                    }
                    self.custom = Some(text);
                }
            },
            Message::ThemeWallpaper(on) => {
                self.select_applied();
                if let Some(mut p) = self.profile().filter(|p| p.appearance_only).cloned() {
                    p.wallpaper = if on {
                        profiles::themes::family(&p).map(profiles::themes::matching)
                    } else {
                        None
                    };
                    self.excluded.remove(&Domain::Wallpaper);
                    self.theme_override = Some(p);
                    self.queue_theme_colors();
                    self.replan();
                }
            }
            Message::ThemeScope(sync) => {
                if let Some(mut p) = self.profile().filter(|p| p.appearance_only).cloned() {
                    let style = p.widget_style.get_or_insert(serde_json::json!({}));
                    let _ = widget::put(style, "/appearance/sync_colors", Some(sync.into()));
                    self.theme_override = Some(p);
                    self.replan();
                }
            }
            Message::ResetSurface(domain) => self.reset_surface(domain),
            Message::RepairThemeMatching => {
                if let Some(store) = self.store() {
                    match profiles::themes::repair_matching(store) {
                        Ok(profile) => {
                            self.select(Selection::Current);
                            self.theme_override = Some(profile);
                            self.session_review = false;
                            self.replan();
                        }
                        Err(error) => self.notice = Some(Notice::Error(error.to_string())),
                    }
                }
            }
            Message::ThemeReady(id, result) => self.theme_ready(id, result),
            Message::Include(domain, on) => {
                if on {
                    self.excluded.remove(&domain);
                } else {
                    self.excluded.insert(domain);
                }
                self.notice = None;
                if domain == Domain::Wallpaper {
                    self.queue_theme_colors();
                }
                self.replan();
            }
            Message::Details(open) => self.details = open,
            Message::MixStart => self.start_mix(),
            Message::MixClosePicker => {
                if let Some(draft) = &mut self.mix {
                    if draft.sources.iter().all(Option::is_some) {
                        draft.choosing = false;
                    }
                }
            }
            Message::MixPart(part) => {
                if let Some(draft) = &mut self.mix {
                    draft.picking = part;
                    draft.choosing = true;
                    draft.query.clear();
                    draft.page = 0;
                }
            }
            Message::Package(edit) => return self.package_edit(edit),
            Message::MixPick(part, selection) => self.pick_mix(part, selection),
            Message::MixQuery(query) => {
                if let Some(draft) = &mut self.mix {
                    draft.query = query.chars().take(80).collect();
                    draft.page = 0;
                }
            }
            Message::MixPage(page) => {
                let max = self.mix_choices().len().saturating_sub(1) / mix::PAGE_SIZE;
                if let Some(draft) = &mut self.mix {
                    draft.page = page.min(max);
                }
            }
            Message::Name(action) => {
                self.confirm_delete = false;
                self.name = action.map(|action| {
                    let text = match &action {
                        NameAction::Capture => "My desktop".to_string(),
                        NameAction::SaveMix => "My mix".to_string(),
                        NameAction::CaptureTheme => "My theme".to_string(),
                        NameAction::Rename(_) => self.selected_name().unwrap_or_default(),
                        NameAction::Duplicate => {
                            let name = self.selected_name().unwrap_or_default();
                            let copy = format!("{name} copy");
                            if copy.chars().count() <= 80 {
                                copy
                            } else {
                                name
                            }
                        }
                    };
                    NameEdit { action, text }
                });
                if self.name.is_some() {
                    return Outcome::FocusName;
                }
            }
            Message::NameInput(text) => {
                if let Some(edit) = &mut self.name {
                    edit.text = text;
                }
            }
            Message::NameSubmit => self.submit_name(),
            Message::Delete(false) => {
                self.name = None;
                self.confirm_delete =
                    !self.confirm_delete && matches!(self.selection, Selection::Saved(_));
                self.rules_using = self.rules_using();
            }
            Message::Delete(true) => self.delete(),
            Message::Import | Message::ImportTheme => {
                if self.pending.is_some() {
                    return Outcome::None;
                }
                let id = self.request(Pending::Import(matches!(message, Message::ImportTheme)));
                return Outcome::Dialog(Request::Import { id });
            }
            Message::Export => return self.export(),
            Message::Imported(id, result) => return self.imported(id, result),
            Message::Exported(id, result) => self.exported(id, result),
            Message::Apply => return self.apply(),
            Message::Discard => self.select(Selection::Current),
            Message::Undo => return self.undo(),
            Message::Replan => {
                self.queue_theme_colors();
                self.session_review = false;
                self.notice = None;
                self.refresh();
            }
            Message::Recover => return self.recover(true),
            Message::KeepRecovery => return self.recover(false),
            Message::Backdrop(id, pixels) => self.backdrop_ready(id, pixels),
        }
        Outcome::None
    }

    fn request(&mut self, kind: Pending) -> u64 {
        self.next_request = self.next_request.wrapping_add(1);
        self.pending = Some((self.next_request, kind));
        self.next_request
    }
    /// The pending request `id`, taken; `None` for a stale or unknown result.
    fn finish(&mut self, id: u64) -> Option<Pending> {
        match &self.pending {
            Some((pending, _)) if *pending == id => self.pending.take().map(|(_, kind)| kind),
            _ => None,
        }
    }

    fn submit_name(&mut self) {
        let Some(edit) = self.name.clone() else {
            return;
        };
        if edit.action == NameAction::SaveMix
            && (self.mix.is_none()
                || self.profile().is_none()
                || self.theme_loading()
                || self.theme_problem().is_some())
        {
            return;
        }
        let name = edit.text.trim().to_string();
        if Profile::new(&name).is_none() {
            self.notice = Some(Notice::Error(
                "Use 1–80 characters, without line breaks.".into(),
            ));
            return;
        }
        let Some(store) = self.store() else {
            return;
        };
        let library = ProfileLibrary::new(store);
        let result: Result<(Option<ProfileId>, Vec<String>), profiles::ProfileError> =
            match &edit.action {
                NameAction::CaptureTheme => {
                    profiles::themes::capture(store, &name).and_then(|capture| {
                        library
                            .create(&capture.profile)
                            .map(|id| (Some(id), capture.warnings))
                    })
                }
                NameAction::Capture => profiles::capture(store, &name).and_then(|capture| {
                    library
                        .create(&capture.profile)
                        .map(|id| (Some(id), capture.warnings))
                }),
                NameAction::Rename(id) => library.rename(id, &name).map(|_| (None, Vec::new())),
                NameAction::Duplicate | NameAction::SaveMix => {
                    let Some(mut copy) = self.profile().cloned() else {
                        return;
                    };
                    copy.name = name.clone();
                    if edit.action == NameAction::SaveMix {
                        for domain in &self.excluded {
                            match domain {
                                Domain::Dock => copy.dock = None,
                                Domain::Panel => copy.panel = None,
                                Domain::Appearance => copy.appearance = None,
                                Domain::WidgetStyle => copy.widget_style = None,
                                Domain::WidgetLayout => copy.widget_layout = None,
                                Domain::Wallpaper => copy.wallpaper = None,
                                Domain::Windows => copy.windows = None,
                                Domain::Workspaces => copy.workspaces = None,
                                Domain::DesktopIcons => copy.desktop_icons = None,
                            }
                        }
                        if copy.domains().is_empty() {
                            self.notice =
                                Some(Notice::Error("Choose at least one part to save".into()));
                            return;
                        }
                    }
                    library.create(&copy).map(|id| (Some(id), Vec::new()))
                }
            };
        match result {
            Ok((created, warnings)) => {
                self.reload_library();
                match created {
                    Some(id) => self.select(Selection::Saved(id)),
                    None => {
                        if let Some(profile) = &mut self.theme_override {
                            profile.name = name.clone();
                        }
                        if let Selection::Saved(id) = &self.selection {
                            let id = id.clone();
                            self.load(&id);
                            self.replan();
                        }
                    }
                }
                self.name = None;
                self.notes = warnings;
                self.notice = Some(Notice::Done(match edit.action {
                    NameAction::Capture | NameAction::CaptureTheme => {
                        format!("Saved “{name}” from your desktop")
                    }
                    NameAction::Rename(_) => format!("Renamed to “{name}”"),
                    NameAction::Duplicate | NameAction::SaveMix => format!("Saved “{name}”"),
                }));
            }
            // The name field stays open with the text for another try.
            Err(error) => self.notice = Some(Notice::Error(error.to_string())),
        }
    }

    /// How many automation rules apply the selected saved profile.
    fn rules_using(&self) -> usize {
        let (Some(store), Selection::Saved(id)) = (self.store(), &self.selection) else {
            return 0;
        };
        let target = crate::automation::ProfileRef::Saved(id.as_str().into());
        crate::automation::load(store).map_or(0, |settings| {
            settings
                .rules
                .iter()
                .filter(|rule| rule.profile == target)
                .count()
        })
    }

    fn delete(&mut self) {
        let Selection::Saved(id) = self.selection.clone() else {
            self.confirm_delete = false;
            return;
        };
        if !self.confirm_delete {
            return;
        }
        self.confirm_delete = false;
        let Some(store) = self.store() else {
            return;
        };
        let name = self.selected_name();
        match ProfileLibrary::new(store).delete(&id) {
            Ok(()) => {
                self.reload_library();
                self.select(Selection::Current);
                self.notice = Some(Notice::Done(match name {
                    Some(name) => format!("Deleted “{name}”"),
                    None => "Deleted the unreadable profile".into(),
                }));
            }
            Err(error) => self.notice = Some(Notice::Error(error.to_string())),
        }
    }

    fn export(&mut self) -> Outcome {
        if self.pending.is_some() {
            return Outcome::None;
        }
        let Some(store) = self.store() else {
            return Outcome::None;
        };
        if !store.writable() {
            self.notice = Some(Notice::Error("Export is off in preview mode".into()));
            return Outcome::None;
        }
        let confine = store.fixture_root().map(Path::to_path_buf);
        let Some(profile) = self.profile().cloned() else {
            return Outcome::None;
        };
        // The bytes are fixed now: a later selection change can't alter
        // what the chosen file receives. Shared files never carry this
        // computer's own pictures or displays.
        match profiles::export(&profile) {
            Ok(exported) => {
                let id = self.request(Pending::Export(profile.name.clone(), exported.omitted));
                Outcome::Dialog(Request::Export {
                    id,
                    file_name: file_name(&profile.name),
                    bytes: exported.bytes,
                    confine,
                })
            }
            Err(error) => {
                self.notice = Some(Notice::Error(error.to_string()));
                Outcome::None
            }
        }
    }

    fn imported(&mut self, id: u64, result: Result<Option<Vec<u8>>, String>) -> Outcome {
        let Some(Pending::Import(themes_only)) = self.finish(id) else {
            return Outcome::None;
        };
        let bytes = match result {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return Outcome::None,
            Err(error) => {
                self.notice = Some(Notice::Error(error));
                return Outcome::None;
            }
        };
        let Some(store) = self.store() else {
            return Outcome::None;
        };
        // Validated before anything is stored; never applied.
        let imported = if profiles::packages::recognizes(&bytes) {
            profiles::packages::Package::parse(&bytes)
                .map_err(profiles::ProfileError::Invalid)
                .and_then(|package| {
                    if themes_only {
                        return Err(profiles::ProfileError::Invalid(
                            "Import setup packages in Setups.".into(),
                        ));
                    }
                    let name = package.profile.name.clone();
                    package.import(store).map(|id| (id, name))
                })
        } else {
            profiles::import(&bytes).and_then(|profile| {
                if themes_only && !profile.appearance_only {
                    return Err(profiles::ProfileError::Invalid(
                        "This file is a layout preset. Import it in Presets & schedules.".into(),
                    ));
                }
                ProfileLibrary::new(store)
                    .create(&profile)
                    .map(|id| (id, profile.name))
            })
        };
        match imported {
            Ok((id, name)) => {
                self.reload_library();
                self.select(Selection::Saved(id));
                self.notice = Some(Notice::Done(format!(
                    "Imported “{name}”. Review it, then apply."
                )));
            }
            Err(error) => {
                self.notice = Some(Notice::Error(format!("Not imported. {error}")));
            }
        }
        Outcome::None
    }

    fn exported(&mut self, id: u64, result: Result<Option<PathBuf>, String>) {
        let (name, omitted) = match self.finish(id) {
            Some(Pending::Export(name, omitted)) => (name, omitted),
            Some(Pending::Image(name, omitted)) => (
                name,
                if omitted {
                    vec!["personal or unlisted wallpaper"]
                } else {
                    vec![]
                },
            ),
            _ => return,
        };
        match result {
            Ok(Some(path)) => {
                let file = path
                    .file_name()
                    .map(|file| file.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.notice = Some(Notice::Done(format!("Exported “{name}” to {file}")));
                self.notes = kept_note(&omitted).into_iter().collect();
            }
            Ok(None) => {}
            Err(error) => self.notice = Some(Notice::Error(error)),
        }
    }

    /// The footer's line for a preview that can't apply as it is: a plan
    /// with a blocked part is never shown as matching the desktop.
    #[cfg(test)]
    pub fn blocked_status(&self) -> Option<&'static str> {
        let plan = self.plan().filter(|_| !self.stale())?;
        if plan.blocking_reasons().is_empty() {
            return None;
        }
        Some(if plan.domains.iter().any(|part| part.blocked.is_empty()) {
            "Leave out the blocked part to apply the rest"
        } else {
            "Can’t apply this profile yet"
        })
    }

    /// Applies the displayed plan exactly; a stale plan conflicts instead
    /// of being replaced.
    fn apply(&mut self) -> Outcome {
        if !self.can_apply() || !self.check_theme_source() {
            return Outcome::None;
        }
        let (Some(store), Some(plan)) = (self.store(), self.plan.as_ref()) else {
            return Outcome::None;
        };
        // Wallpaper pictures can go missing after the preview: checked
        // again before anything is written; the part then shows why.
        if let Some(reason) = plan.recheck().into_iter().next() {
            self.replan();
            self.notice = Some(Notice::Error(format!("{reason} Nothing was changed.")));
            return Outcome::None;
        }
        match profiles::apply(store, plan) {
            Ok(latest) => {
                let name = plan.name.clone();
                // COSMIC Files shows new icon settings only after a refresh.
                let icons = plan.domains.iter().any(|part| {
                    part.domain == crate::desktop::profiles::Domain::DesktopIcons
                        && !part.changes.is_empty()
                });
                // Only acknowledge color/polarity choices included in this
                // transaction. A Wallpaper-only setup must leave the new
                // source unseen so the runtime can derive its colors.
                let includes = |pointer| plan.widget_patch().iter().any(|(p, _)| p == pointer);
                let defines_colors =
                    wallpaper::automatic::read_config(store).is_ok_and(|(_, config)| {
                        (!config.appearance.wallpaper_colors || includes("/appearance/accent"))
                            && (config.appearance.family.is_none()
                                || includes("/appearance/wallpaper_light"))
                    });
                if defines_colors {
                    if let Some(source) = &self.theme_source {
                        let _ = wallpaper::automatic::acknowledge_analyzed(store, source);
                    } else if !self.profile().is_some_and(|p| p.appearance_only) {
                        let _ = wallpaper::automatic::keep_manual_colors(store);
                    }
                }
                self.undo = Some(latest);
                self.elsewhere = None;
                self.replan();
                self.notes.clear();
                if icons {
                    self.notes
                        .push(crate::desktop::desktop_icons::REFRESH_NOTE.to_string());
                }
                self.refresh_surface_status();
                self.reload_library();
                self.select(Selection::Current);
                if icons {
                    self.notes
                        .push(crate::desktop::desktop_icons::REFRESH_NOTE.to_string());
                }
                self.notice = None;
                let _ = name;
                Outcome::Changed
            }
            Err(error) => {
                self.write_failed(error);
                Outcome::None
            }
        }
    }

    fn undo(&mut self) -> Outcome {
        if !self.can_undo() {
            return Outcome::None;
        }
        let (Some(store), Some(change)) = (self.store(), self.undo.as_ref()) else {
            return Outcome::None;
        };
        match store.undo_latest(&change.id) {
            Ok(_) => {
                let _ = wallpaper::automatic::keep_manual_colors(store);
                self.undo = None;
                self.refresh();
                self.notice = Some(Notice::Done("Previous settings restored".into()));
                Outcome::Changed
            }
            Err(DesktopError::Conflict(keys)) => {
                self.notice = Some(Notice::Error(format!(
                    "Can’t undo: {} changed since. Nothing was changed.",
                    changed(&keys, Vec::new())
                )));
                Outcome::None
            }
            Err(DesktopError::FieldConflict { keys, fields }) => {
                self.notice = Some(Notice::Error(format!(
                    "Can’t undo: {} changed since. Nothing was changed.",
                    changed(&keys, fields)
                )));
                Outcome::None
            }
            Err(error) => {
                self.refresh_recovery();
                self.notice = Some(Notice::Error(error.to_string()));
                Outcome::None
            }
        }
    }

    /// The selection and included parts are kept on every failure.
    fn write_failed(&mut self, error: DesktopError) {
        let stale = match error {
            DesktopError::Conflict(keys) => changed(&keys, Vec::new()),
            DesktopError::FieldConflict { keys, fields } => changed(&keys, fields),
            error => {
                self.refresh_recovery();
                self.notice = Some(Notice::Error(error.to_string()));
                return;
            }
        };
        self.stale = true;
        self.notice = Some(Notice::Error(format!(
            "{stale} changed since the preview. Nothing was changed."
        )));
    }

    fn recover(&mut self, restore: bool) -> Outcome {
        let Some(store) = self.store() else {
            return Outcome::None;
        };
        let result = if restore {
            store.recover().map(|report| {
                (
                    super::recovery::summary(&report),
                    super::recovery::details(&report),
                )
            })
        } else {
            store
                .discard_recovery()
                .map(|()| ("Current settings kept".to_string(), Vec::new()))
        };
        match result {
            Ok((summary, details)) => {
                self.refresh();
                self.notice = Some(Notice::Done(summary));
                self.notes = details;
                Outcome::Changed
            }
            Err(error) => {
                self.notice = Some(Notice::Error(error.to_string()));
                Outcome::None
            }
        }
    }
}

/// Copies a pending look's color and matching-wallpaper choice to another
/// built-in style. A style's own color stays that style's; a record without
/// a color choice (a surface Reset) leaves the one `to` has.
fn carry_choices(from: &Profile, to: &mut Profile) {
    if let Some(color) = LookColor::of(from) {
        color.set(to);
    }
    if from.wallpaper.is_some() {
        to.wallpaper = profiles::themes::family(to).map(profiles::themes::matching);
    }
}

/// The name field, focused when it opens.
pub fn name_id() -> cosmic::widget::Id {
    cosmic::widget::Id::new("profile-name")
}

/// “Dock size, Card palette” for conflicting keys and widget settings.
fn changed(keys: &[crate::desktop::ConfigKey], fields: Vec<String>) -> String {
    let mut names = Vec::new();
    if !keys.is_empty() {
        names.push(crate::settings::system_appearance::describe(keys));
    }
    names.extend(fields);
    if names.is_empty() {
        "A setting".into()
    } else {
        names.join(", ")
    }
}

/// Which parts of the desktop a recorded change touched.
#[cfg(test)]
pub fn change_domains(change: &LatestChange) -> BTreeSet<Domain> {
    let mut domains = BTreeSet::new();
    for entry in change.change.entries() {
        let namespace = entry.key.namespace();
        domains.insert(if namespace == PanelKind::Dock.namespace() {
            Domain::Dock
        } else if namespace == PanelKind::Panel.namespace() {
            Domain::Panel
        } else if namespace == wallpaper::NAMESPACE {
            Domain::Wallpaper
        } else if namespace == crate::desktop::desktop_icons::NAMESPACE {
            Domain::DesktopIcons
        } else if crate::desktop::windows::Behavior::of(&entry.key).is_some() {
            Domain::Windows
        } else if entry.key == crate::desktop::workspaces::key() {
            Domain::Workspaces
        } else {
            // Theme (window gaps included), toolkit and compositor appearance.
            Domain::Appearance
        });
    }
    for field in change.widget.iter().flat_map(|widget| &widget.fields) {
        if let Some(field) = widget::field(&field.pointer) {
            domains.insert(match field.group {
                widget::WidgetGroup::Style => Domain::WidgetStyle,
                widget::WidgetGroup::Layout => Domain::WidgetLayout,
            });
        }
    }
    domains
}

/// The current desktop with the plan's actual changes laid over it.
fn preview(store: &DesktopStore, plan: Option<&ProfilePlan>) -> Preview {
    let empty = DesktopPatch::new();
    let patch = plan.map_or(&empty, ProfilePlan::patch);
    let bars = PanelKind::ALL
        .into_iter()
        .filter_map(|kind| {
            store
                .load(&PanelDraft::keys(kind))
                .ok()
                .map(|snapshot| PanelDraft::from_snapshot(kind, overlay(snapshot, patch)))
        })
        .collect();
    let appearance = store
        .load(&AppearanceDraft::keys())
        .ok()
        .map(|snapshot| AppearanceDraft::from_snapshot(overlay(snapshot, patch)));
    let config = store.app_config().and_then(|path| {
        let mut value = config::read_config_text(path)
            .and_then(|text| config::parse_config_text(text.as_deref()))
            .ok()?;
        if let Some(plan) = plan {
            for (pointer, entry) in plan.widget_patch().changes() {
                widget::put(&mut value, pointer, entry.desired.clone()).ok()?;
            }
        }
        config::check_config_value(&value).ok()
    });
    Preview {
        bars,
        appearance,
        config,
    }
}

fn overlay(mut snapshot: Snapshot, patch: &DesktopPatch) -> Snapshot {
    for (key, entry) in patch.iter() {
        let system = snapshot.entry(key).and_then(|entry| entry.system.clone());
        snapshot.insert(
            key.clone(),
            KeyEntry {
                user: entry.desired.clone(),
                system,
            },
        );
    }
    snapshot
}

/// “Kept on this computer: your own wallpaper, …” for what a shared
/// profile left out.
pub fn kept_note(omitted: &[&str]) -> Option<String> {
    (!omitted.is_empty()).then(|| format!("Kept on this computer: {}", omitted.join(", ")))
}

/// A suggested export file name: the profile name, made file-safe.
fn file_name(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let stem = stem.trim().trim_matches('-');
    format!("{}.json", if stem.is_empty() { "profile" } else { stem })
}

/// Reads a chosen profile file, refusing anything but a regular file and
/// never reading more than the profile limit (plus one byte to tell).
pub fn read_import(path: &Path) -> Result<Vec<u8>, String> {
    let unreadable = |error: io::Error| format!("Could not read the file: {error}");
    let meta = fs::metadata(path).map_err(unreadable)?;
    if !meta.is_file() {
        return Err("Choose a profile file.".into());
    }
    if meta.len() > profiles::packages::MAX_BYTES as u64 {
        return Err("This profile is too large.".into());
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(unreadable)?;
    if !file.metadata().map_err(unreadable)?.is_file() {
        return Err("Choose a regular setup file.".into());
    }
    let mut bytes = Vec::new();
    file.take(profiles::packages::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(unreadable)?;
    if bytes.len() > profiles::packages::MAX_BYTES
        || (bytes.len() > MAX_PROFILE_BYTES && !profiles::packages::recognizes(&bytes))
    {
        return Err("This profile is too large.".into());
    }
    Ok(bytes)
}

/// Writes `bytes` to the chosen `path` atomically: a new temporary file
/// beside it, flushed, then renamed over it (the chooser has already asked
/// about replacing). With `confine`, the folder must be inside that root.
pub fn write_export(path: &Path, bytes: &[u8], confine: Option<&Path>) -> Result<(), String> {
    let failed = |error: io::Error| format!("Could not export: {error}");
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err("Choose a file to export to.".into());
    };
    if !path.is_absolute() {
        return Err("Choose a local file.".into());
    }
    if let Some(root) = confine {
        let root = root.canonicalize().map_err(failed)?;
        let folder = parent.canonicalize().map_err(failed)?;
        if !folder.starts_with(&root) {
            return Err("In a fixture, export inside the fixture folder.".into());
        }
    }
    if fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir()) {
        return Err("Choose a file, not a folder.".into());
    }
    let temp = parent.join(format!(
        ".{}.{}-{}.tmp",
        name.to_string_lossy(),
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        fs::File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(failed)
}

#[cfg(test)]
mod tests;
