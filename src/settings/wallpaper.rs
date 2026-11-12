//! Wallpaper editor state: one draft of the background keys (shared entry,
//! per-display entries, the display list and mode), written only by Apply
//! as one change owned by this page, plus the page's media.
//!
//! The editor owns its background work: thumbnails (at most two decodes at
//! a time, a 48-entry in-memory cache keyed by path, modification time and
//! length), folder scans, colour analysis and file-chooser requests. The
//! gallery shows one page of tiles at a time and only that page (plus the
//! shown, selected and edited sources) gets thumbnails and scans, so the
//! wanted set always fits what is kept. Each request carries an ID; a
//! result whose ID is no longer pending (another selection, display, tab
//! or page) is dropped. Nothing here writes desktop
//! settings except Apply / Undo / recovery, and nothing writes files except
//! an explicit Add or Remove saving the app's own collection.
use crate::desktop::DesktopStore;
use crate::desktop::appearance::{AppearanceDraft, ColorRole, Control, ThemeVariant};
use crate::desktop::panels::Setting;
use crate::desktop::store::{DesktopPatch, Journal, LatestChange, Owner, WidgetPatch, widget};
use crate::desktop::wallpaper::automatic;
use crate::desktop::wallpaper::collection::{self, Collection, Item, Kind};
use crate::desktop::wallpaper::media::{self, FolderScan, PREVIEWED, Pixels, Preview};
use crate::desktop::wallpaper::palette::{self, Suggestion};
use crate::desktop::wallpaper::{
    self, BgColor, BgEntry, BgGradient, BgSource, Class, DisplayView, Displays, ROTATIONS,
    SamplingMethod, ScalingMode, Target, WallpaperDraft,
};
use crate::settings::colors::Colors;
use crate::settings::desktop::Notice;
use crate::settings::drafts::DraftEditor;
use cosmic::widget::image::Handle;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
mod effect;

/// Thumbnails kept in memory.
const CACHE: usize = 48;
/// Decodes running at once.
const DECODES: usize = 2;
/// Gallery tiles per page (4 or 5 per row). The page's pictures plus the
/// shown one always fit in `CACHE`.
pub const PAGE: usize = 20;
/// Folder scans kept: a page of folder tiles, the shown one and every
/// display's edited folder (at most 16 displays and the shared entry).
const SCANS: usize = 48;
/// Header preview resolution (shown at 224×126).
pub const PREVIEW: (u32, u32) = (320, 180);
pub const ADDED_IN_PREVIEW: &str = "Not saved in preview mode";
const TYPED: &str = "Finish the color code (#RRGGBB) or pick a color";

/// COSMIC's colour backgrounds (cosmic-settings 58aeb65 `DEFAULT_COLORS`),
/// stored exactly as COSMIC Settings writes them.
pub fn default_colors() -> Vec<BgColor> {
    let single = |rgb: [f32; 3]| BgColor::Single(rgb);
    let gradient = |a: [f32; 3], b: [f32; 3]| {
        BgColor::Gradient(BgGradient {
            colors: vec![a, b],
            radius: 180.0,
        })
    };
    vec![
        single([0.580, 0.922, 0.922]),
        single([0.000, 0.286, 0.427]),
        single([1.000, 0.678, 0.000]),
        single([0.282, 0.725, 0.78]),
        single([0.333, 0.278, 0.259]),
        single([0.969, 0.878, 0.384]),
        single([0.063, 0.165, 0.298]),
        single([1.000, 0.843, 0.631]),
        single([0.976, 0.227, 0.514]),
        single([1.000, 0.612, 0.867]),
        single([0.812, 0.490, 1.000]),
        single([0.835, 0.549, 1.000]),
        single([0.243, 0.533, 1.000]),
        single([0.584, 0.769, 0.988]),
        gradient([1.000, 0.678, 0.000], [0.282, 0.725, 0.78]),
        gradient([1.000, 0.843, 0.631], [0.58, 0.922, 0.922]),
        gradient([1.000, 0.612, 0.867], [0.976, 0.29, 0.514]),
        gradient([0.584, 0.769, 0.988], [0.063, 0.165, 0.298]),
        gradient([0.969, 0.878, 0.384], [0.333, 0.278, 0.259]),
    ]
}

/// `#RRGGBB` (or without `#`) as sRGB 0–1.
pub fn parse_rgb(text: &str) -> Option<[f32; 3]> {
    crate::desktop::appearance::parse_hex(text, false)
        .map(|color| [color.red, color.green, color.blue])
}
/// sRGB 0–1 as `#RRGGBB`.
pub fn hex(rgb: [f32; 3]) -> String {
    let [r, g, b] = rgb.map(|value| (value.clamp(0., 1.) * 255.).round() as u8);
    format!("#{r:02X}{g:02X}{b:02X}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tab {
    /// Halcyon's bundled wallpapers.
    Bundled,
    Cosmic,
    Yours,
    Colors,
}

/// What a tile sets as the source.
#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    /// A bundled palette wallpaper, at its installed location.
    Bundled(String),
    Image(PathBuf),
    Folder(PathBuf),
    Color(BgColor),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FitChoice {
    /// Zoom: covers the display and crops.
    Fill,
    /// Letterboxed with a colour.
    Fit,
    Stretch,
}

#[derive(Clone, Debug)]
pub enum Message {
    Tab(Tab),
    /// Gallery page (clamped to the open tab's pages).
    Page(usize),
    Select(Pick),
    Fit(FitChoice),
    /// Letterbox colour text (`#RRGGBB`).
    FitColor(String),
    Same(bool),
    Display(Target),
    Every(u64),
    Order(SamplingMethod),
    UseShared,
    AddPicture,
    AddFolder,
    Picked(u64, Result<Option<PathBuf>, String>),
    Remove(PathBuf),
    Thumb(u64, PathBuf, Preview),
    /// Whether a shown picture is animated (`Job::Motion`).
    Animated(PathBuf, bool),
    Bundled(Vec<(&'static str, Option<Pixels>)>),
    Scanned(u64, PathBuf, Result<FolderScan, String>),
    Analysed(u64, Suggestion),
    AutoAnalysed(u64, u64, Result<Suggestion, String>),
    /// Custom colour: stop text (`#RRGGBB`).
    Stop(usize, String),
    /// Custom colour: 1 (solid), 2 or 3 stops.
    Stops(usize),
    /// Custom gradient direction (degrees: 0 ↑, 90 →, 180 ↓, 270 ←).
    Direction(u16),
    /// An extracted swatch; staged with the background until Apply.
    Accent([u8; 3]),
    ClearAccent,
    Palette(String),
    SyncColors(bool),
    AutomaticColors(bool),
    ThemeColor,
    /// `true`: the active window outline follows the accent again.
    LinkOutline(bool),
    AccentText(String),
    AccentSubmit,
    PaletteChoices(bool),
    ApplyColors,
    ResolveThemeAccent,
    KeepColors,
    ResolveEffect(bool),
    SuggestAccent,
    // Legacy preference route retained for compatibility regression coverage.
    #[cfg_attr(not(test), allow(dead_code))]
    StopMatching,
    Reinstall,
    Apply,
    Discard,
    Undo,
    Refresh,
    UseCurrent,
    /// Keeps the edits over a value changed outside this editor.
    KeepMine,
    Recover,
    KeepRecovery,
}

impl Message {
    /// Messages that write desktop settings, the change journal, the app's
    /// collection file or (through the controller) the bundled pictures.
    pub fn writes(&self) -> bool {
        matches!(
            self,
            Self::Apply
                | Self::ApplyColors
                | Self::Undo
                | Self::Recover
                | Self::KeepRecovery
                | Self::AddPicture
                | Self::AddFolder
                | Self::Picked(..)
                | Self::Remove(_)
                | Self::Reinstall
        )
    }
}

/// A file-chooser request; its result carries the ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub id: u64,
    pub folder: bool,
}

/// What the page asks of the app after a message.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    None,
    Dialog(Request),
    /// Controller arguments (refused in preview).
    Controller(Vec<String>),
}

/// Blocking background work; `run` gives the result message.
#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    Thumb {
        id: u64,
        path: PathBuf,
    },
    /// Whether the shown picture is animated.
    Motion {
        path: PathBuf,
    },
    Bundled,
    Scan {
        id: u64,
        path: PathBuf,
    },
    Analyse {
        id: u64,
        input: Input,
    },
    Automatic {
        id: u64,
        source: automatic::Source,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Pixels(Pixels),
    Color(BgColor),
}

impl Job {
    pub fn run(self) -> Message {
        match self {
            Self::Thumb { id, path } => {
                let preview = media::thumbnail(&path);
                Message::Thumb(id, path, preview)
            }
            Self::Motion { path } => {
                let animated = media::animated(&path);
                Message::Animated(path, animated)
            }
            Self::Bundled => Message::Bundled(
                wallpaper::BUNDLED
                    .iter()
                    .map(|(name, _)| (*name, media::bundled_thumbnail(name)))
                    .collect(),
            ),
            Self::Scan { id, path } => {
                let scan = media::scan_folder(&path);
                Message::Scanned(id, path, scan)
            }
            Self::Analyse { id, input } => Message::Analysed(
                id,
                match input {
                    Input::Pixels(pixels) => palette::from_pixels(&pixels),
                    Input::Color(color) => palette::from_color(&color),
                },
            ),
            Self::Automatic { id, source } => {
                Message::AutoAnalysed(id, source.stamp, source.analyse())
            }
        }
    }
    /// The result when the job could not run at all.
    fn failed(&self) -> Message {
        match self {
            Self::Thumb { id, path } => {
                Message::Thumb(*id, path.clone(), Preview::None(media::NO_PREVIEW))
            }
            Self::Motion { path } => Message::Animated(path.clone(), false),
            Self::Bundled => Message::Bundled(Vec::new()),
            Self::Scan { id, path } => {
                Message::Scanned(*id, path.clone(), Err("The folder couldn’t be read".into()))
            }
            Self::Analyse { id, .. } => Message::Analysed(*id, Suggestion::default()),
            Self::Automatic { id, source } => Message::AutoAnalysed(
                *id,
                source.stamp,
                Err("Color analysis stopped. Your last colors are kept".into()),
            ),
        }
    }
}

/// Runs `job` on the blocking pool; `wrap` makes the app's message.
pub fn task<X: Into<Y> + 'static, Y: 'static>(job: Job, wrap: fn(Message) -> X) -> cosmic::Task<Y> {
    cosmic::task::future(async move {
        let failed = job.failed();
        wrap(
            tokio::task::spawn_blocking(move || job.run())
                .await
                .unwrap_or(failed),
        )
    })
}

/// The portal file chooser for Add picture / Add folder: local files of
/// the types cosmic-bg shows. Cancelling gives `Ok(None)`.
pub fn dialog<X: Into<Y> + 'static, Y: 'static>(
    request: Request,
    wrap: fn(Message) -> X,
) -> cosmic::Task<Y> {
    use cosmic::dialog::file_chooser::{self, FileFilter, open::Dialog};
    cosmic::task::future(async move {
        let dialog = Dialog::new().title(if request.folder {
            "Add a slideshow folder"
        } else {
            "Add a picture"
        });
        let response = if request.folder {
            dialog.open_folder().await
        } else {
            let mut filter = FileFilter::new("Pictures");
            for extension in media::ACCEPTED {
                filter = filter.glob(&format!("*.{extension}"));
            }
            dialog.filter(filter).open_file().await
        };
        let result = match response {
            Ok(response) => response
                .url()
                .to_file_path()
                .map(Some)
                .map_err(|()| "Choose a local file or folder.".to_string()),
            Err(file_chooser::Error::Cancelled) => Ok(None),
            Err(error) => Err(format!("Could not open the file chooser: {error}")),
        };
        wrap(Message::Picked(request.id, result))
    })
}

type ThumbKey = (PathBuf, Option<SystemTime>, u64);

/// The actual pixels being shown. A path alone is not a version: a file
/// can be replaced, and a slideshow keeps the same folder between frames.
#[derive(Clone, Debug, PartialEq)]
enum AnalysisKey {
    Picture(ThumbKey),
    Color(BgColor),
}

fn thumb_key(path: &Path) -> Option<ThumbKey> {
    let metadata = std::fs::metadata(path).ok()?;
    metadata
        .is_file()
        .then(|| (path.to_path_buf(), metadata.modified().ok(), metadata.len()))
}
fn previewed(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            PREVIEWED
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

/// A decoded thumbnail and its image handle.
#[derive(Clone, Debug)]
pub struct Thumb {
    pub preview: Preview,
    pub handle: Option<Handle>,
}

impl Thumb {
    fn new(preview: Preview) -> Self {
        let handle = match &preview {
            Preview::Ready(pixels) => Some(Handle::from_rgba(
                pixels.width,
                pixels.height,
                pixels.rgba.clone(),
            )),
            Preview::None(_) => None,
        };
        Self { preview, handle }
    }
    fn pixels(&self) -> Option<&Pixels> {
        match &self.preview {
            Preview::Ready(pixels) => Some(pixels),
            Preview::None(_) => None,
        }
    }
}

/// Least-recently-used thumbnails.
#[derive(Default)]
struct Lru {
    order: VecDeque<ThumbKey>,
    map: BTreeMap<ThumbKey, Thumb>,
}

impl Lru {
    fn get(&self, key: &ThumbKey) -> Option<&Thumb> {
        self.map.get(key)
    }
    fn touch(&mut self, key: &ThumbKey) {
        if let Some(index) = self.order.iter().position(|known| known == key) {
            let key = self.order.remove(index).expect("present");
            self.order.push_back(key);
        }
    }
    fn insert(&mut self, key: ThumbKey, thumb: Thumb) {
        if self.map.insert(key.clone(), thumb).is_some() {
            self.order.retain(|known| *known != key);
        }
        self.order.push_back(key);
        while self.order.len() > CACHE {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            }
        }
    }
    fn len(&self) -> usize {
        self.map.len()
    }
}

/// Custom colour as typed: exact values (sRGB for a solid colour, linear
/// for gradient stops) and the text of a stop that is not a colour yet.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Custom {
    pub values: Vec<[f32; 3]>,
    pub radius: f32,
    pub texts: BTreeMap<usize, String>,
    /// The drafted colour these values last matched (`None`: the display
    /// showed no colour). A value that differs from it is a pending edit
    /// still waiting for unfinished text.
    origin: Option<BgColor>,
}

impl Custom {
    /// The fields of `color`, nothing typed.
    pub fn of(color: &BgColor) -> Self {
        let (values, radius) = match color {
            BgColor::Single(rgb) => (vec![*rgb], 180.),
            BgColor::Gradient(gradient) => (gradient.colors.clone(), gradient.radius),
        };
        Self {
            values,
            radius,
            texts: BTreeMap::new(),
            origin: Some(color.clone()),
        }
    }
    fn gradient(&self) -> bool {
        self.values.len() > 1
    }
    /// The stop's colour as displayed (sRGB).
    pub fn shown(&self, index: usize) -> Option<[f32; 3]> {
        let value = *self.values.get(index)?;
        Some(if self.gradient() {
            value.map(wallpaper::linear_to_srgb)
        } else {
            value
        })
    }
    pub fn text(&self, index: usize) -> String {
        self.texts
            .get(&index)
            .cloned()
            .or_else(|| self.shown(index).map(hex))
            .unwrap_or_default()
    }
    pub fn invalid(&self) -> Option<usize> {
        self.texts
            .iter()
            .find(|(_, text)| parse_rgb(text).is_none())
            .map(|(index, _)| *index)
    }
    fn color(&self) -> BgColor {
        if self.gradient() {
            BgColor::Gradient(BgGradient {
                colors: self.values.clone(),
                radius: self.radius,
            })
        } else {
            BgColor::Single(self.values[0])
        }
    }
    /// 1–3 stops, converting between sRGB (solid) and linear (stops).
    fn resize(&mut self, count: usize) {
        let was = self.gradient();
        let shown: Vec<[f32; 3]> = (0..self.values.len())
            .filter_map(|index| self.shown(index))
            .collect();
        let mut shown: Vec<[f32; 3]> = shown.into_iter().take(count).collect();
        while shown.len() < count {
            let last = *shown.last().unwrap_or(&[0., 0., 0.]);
            shown.push(last);
        }
        let gradient = count > 1;
        self.values = if gradient {
            shown
                .iter()
                .enumerate()
                .map(|(index, rgb)| {
                    // Unchanged stops keep their exact linear value.
                    if was && index < self.values.len() {
                        self.values[index]
                    } else {
                        rgb.map(wallpaper::srgb_to_linear)
                    }
                })
                .collect()
        } else {
            shown
        };
        self.texts.retain(|index, _| *index < count);
    }
    /// Moves the fields onto `now`, the display's drafted colour after the
    /// draft changed underneath. Untouched stops and the angle take the new
    /// values; unfinished text stays where its stop still exists, and with
    /// `accept` false (Refresh) so do pending stop, count and angle edits.
    /// With `accept` (Use current, Undo, recovery) the current values are
    /// taken as they are. `None`: no field of it is left to edit here.
    fn rebase(self, now: Option<&BgColor>, accept: bool) -> Option<Self> {
        if self.origin.as_ref() == now {
            return Some(self);
        }
        let Some(now) = now else {
            // A picture replaced the colour: its fields are gone.
            return None;
        };
        // A colour typed over a picture is all pending.
        if self.origin.is_none() && !accept {
            return Some(self);
        }
        let origin = self.origin.as_ref().map_or_else(|| Self::of(now), Self::of);
        let pending: Vec<bool> = (0..self.values.len())
            .map(|index| !accept && self.shown(index).map(hex) != origin.shown(index).map(hex))
            .collect();
        let pending = |index: usize| pending.get(index).copied().unwrap_or(false);
        let count = if !accept && self.values.len() != origin.values.len() {
            self.values.len()
        } else {
            Self::of(now).values.len()
        };
        if count > 3 {
            // Kept as stored (4–8 stops are never truncated here).
            return None;
        }
        let mut next = Self::of(now);
        next.resize(count);
        for index in 0..count {
            if index < self.values.len() && pending(index) {
                next.values[index] = match (next.gradient(), self.gradient()) {
                    (true, true) | (false, false) => self.values[index],
                    (true, false) => self.values[index].map(wallpaper::srgb_to_linear),
                    (false, true) => self.values[index].map(wallpaper::linear_to_srgb),
                };
            }
        }
        if !accept && self.radius != origin.radius {
            next.radius = self.radius;
        }
        next.texts = self
            .texts
            .into_iter()
            .filter(|(index, text)| {
                *index < count && (parse_rgb(text).is_none() || pending(*index))
            })
            .collect();
        Some(next)
    }
}

/// The colour a display's drafted entry shows, if it is one.
fn color_of(entry: Setting<BgEntry>) -> Option<BgColor> {
    match entry {
        Setting::Known(BgEntry {
            source: BgSource::Color(color),
            ..
        }) => Some(color),
        _ => None,
    }
}

/// "Built-in display" for laptop panels, else the connector.
pub fn display_name(name: &str) -> String {
    crate::displays::display_name(name, None)
}

/// One gallery tile.
#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    pub pick: Pick,
    pub label: String,
    /// The installed or listed file is gone.
    pub missing: bool,
    /// From COSMIC Settings' own list (read only).
    pub external: bool,
}

impl Tile {
    pub fn source(&self, bundled_dir: &Path) -> BgSource {
        match &self.pick {
            Pick::Bundled(name) => BgSource::Path(wallpaper::bundled_path(bundled_dir, name)),
            Pick::Image(path) | Pick::Folder(path) => BgSource::Path(path.clone()),
            Pick::Color(color) => BgSource::Color(color.clone()),
        }
    }

    /// What a screen reader calls the tile: "Wallpaper: aurora.jpg", or
    /// the tile's own words when it has no file.
    pub fn name(&self) -> String {
        let file = match &self.pick {
            Pick::Bundled(name) => Some(name.clone()),
            Pick::Image(path) | Pick::Folder(path) => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            Pick::Color(_) => None,
        };
        let what = file
            .filter(|file| !file.is_empty())
            .unwrap_or_else(|| self.label.clone());
        let kind = if matches!(self.pick, Pick::Folder(_)) {
            "Wallpaper folder"
        } else {
            "Wallpaper"
        };
        if what.is_empty() {
            kind.to_owned()
        } else {
            format!("{kind}: {what}")
        }
    }
}

/// What the header preview shows for the selected display.
#[derive(Clone, Debug, Default)]
pub struct Shown {
    pub folder: bool,
    pub missing: bool,
    /// The picture the preview shows (a slideshow's current or first).
    pub image: Option<PathBuf>,
    pub handle: Option<Handle>,
    /// Why there is no picture ("No preview", "Loading…", "Missing").
    pub note: Option<&'static str>,
    /// One of the bundled wallpapers (Reinstall can restore it).
    pub bundled: bool,
    /// The display's unchanged background is missing: a warning only, as
    /// Apply checks newly shown pictures alone.
    pub warning: bool,
    /// The single picture shown is animated; COSMIC shows it still.
    pub animated: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct SavedWallpaper {
    #[serde(default)]
    effect: Option<effect::EffectDraft>,
    pub(super) draft: WallpaperDraft,
    pub(super) target: Target,
    pub(super) custom: BTreeMap<Target, Custom>,
    pub(super) fit_text: BTreeMap<Target, String>,
    #[serde(default)]
    pub(super) accent: Option<AppearanceDraft>,
    #[serde(default)]
    pub(super) colors: Option<Colors>,
    #[serde(default)]
    pub(super) accent_text: Option<String>,
}

pub struct WallpaperEditor {
    /// Earlier states of this preview, oldest first. Ctrl+Z stays inside it.
    steps: Vec<Option<String>>,
    /// An exhausted preview history stays local until Apply or Discard.
    preview_open: bool,
    /// The state before a slider gesture, recorded when it finishes.
    gesture: Option<Option<String>>,
    effect: Option<effect::EffectDraft>,
    effect_conflict: bool,
    pub(super) core: DraftEditor<WallpaperDraft>,
    pub tab: Tab,
    pub palette_choices: bool,
    pending_colors_commit: bool,
    /// The gallery page of the open tab.
    page: usize,
    /// A tile to show the page of once the tiles are rebuilt (Add).
    reveal: Option<PathBuf>,
    target: Target,
    bundled_dir: PathBuf,
    system_dirs: Vec<PathBuf>,
    system: Vec<PathBuf>,
    cosmic_images: Vec<PathBuf>,
    collection: Result<Collection, String>,
    /// Session-only items (preview mode).
    session: bool,
    bundled: BTreeMap<&'static str, Thumb>,
    bundled_pending: bool,
    thumbs: Lru,
    /// The key each wanted path had when last checked.
    keys: BTreeMap<PathBuf, ThumbKey>,
    queue: VecDeque<PathBuf>,
    decoding: BTreeMap<u64, ThumbKey>,
    scans: BTreeMap<PathBuf, Result<FolderScan, String>>,
    scan_order: VecDeque<PathBuf>,
    scan_queue: VecDeque<PathBuf>,
    scanning: Option<(u64, PathBuf)>,
    analysis: Option<(u64, AnalysisKey)>,
    suggestion: Option<(AnalysisKey, Suggestion)>,
    /// Only the two accent controls; committed with the wallpaper as one
    /// Wallpaper-owned transaction, so one Undo restores both.
    accent: Option<AppearanceDraft>,
    colors: Option<Colors>,
    colors_conflict: bool,
    /// The active window outline has its own color (see `outline_custom`).
    outline_custom: bool,
    automatic_active: bool,
    automatic_source: Option<automatic::Source>,
    automatic_analysis: Option<(u64, u64)>,
    automatic_suggestion: Option<Result<Suggestion, String>>,
    accent_text: Option<String>,
    custom: BTreeMap<Target, Custom>,
    fit_text: BTreeMap<Target, String>,
    dialog: Option<u64>,
    next_id: u64,
    jobs: Vec<Job>,
    tiles: Vec<Tile>,
    /// Rendered colour tiles (as cosmic-bg draws them).
    color_tiles: Vec<(BgColor, Handle)>,
    shown: Shown,
    /// The last picture checked for animation, and the one being checked.
    motion: Option<(ThumbKey, bool)>,
    motion_checking: Option<ThumbKey>,
    preview_key: Option<String>,
    problem: Option<String>,
    current: Option<BgSource>,
}

impl Default for WallpaperEditor {
    fn default() -> Self {
        Self {
            steps: Vec::new(),
            preview_open: false,
            gesture: None,
            effect: None,
            effect_conflict: false,
            core: DraftEditor::new(Owner::Wallpaper, "Wallpaper"),
            tab: Tab::Bundled,
            palette_choices: false,
            pending_colors_commit: false,
            page: 0,
            reveal: None,
            target: Target::Shared,
            bundled_dir: PathBuf::new(),
            system_dirs: Vec::new(),
            system: Vec::new(),
            cosmic_images: Vec::new(),
            collection: Ok(Collection::default()),
            session: false,
            bundled: BTreeMap::new(),
            bundled_pending: false,
            thumbs: Lru::default(),
            keys: BTreeMap::new(),
            queue: VecDeque::new(),
            decoding: BTreeMap::new(),
            scans: BTreeMap::new(),
            scan_order: VecDeque::new(),
            scan_queue: VecDeque::new(),
            scanning: None,
            analysis: None,
            suggestion: None,
            accent: None,
            colors: None,
            colors_conflict: false,
            outline_custom: false,
            automatic_active: false,
            automatic_source: None,
            automatic_analysis: None,
            automatic_suggestion: None,
            accent_text: None,
            custom: BTreeMap::new(),
            fit_text: BTreeMap::new(),
            dialog: None,
            next_id: 0,
            jobs: Vec::new(),
            tiles: Vec::new(),
            color_tiles: Vec::new(),
            shown: Shown::default(),
            motion: None,
            motion_checking: None,
            preview_key: None,
            problem: None,
            current: None,
        }
    }
}

impl WallpaperEditor {
    /// First visit: read values, the collections and discovery. Later
    /// visits refresh untouched values and keep the draft. Reads only.
    pub fn open(&mut self) {
        let first = !self.core.opened();
        self.core.open();
        if first {
            self.discover();
        } else {
            self.reread();
            self.settle_typed(false);
            self.refresh_accent(false);
        }
        self.refresh_outline();
        self.refresh_effect();
        self.sync();
    }
    #[cfg(test)]
    pub fn with_store(store: DesktopStore) -> Self {
        let store = if store.app_config().is_none() {
            match store.fixture_root() {
                Some(root) => {
                    let path = root.join("config.json");
                    store.with_app_config(path)
                }
                None => store,
            }
        } else {
            store
        };
        let mut editor = Self {
            core: DraftEditor::with_store(Owner::Wallpaper, "Wallpaper", store),
            ..Default::default()
        };
        editor.discover();
        editor.sync();
        editor
    }
    fn discover(&mut self) {
        if let Some(store) = self.core.store() {
            self.bundled_dir = wallpaper::bundled_dir(store);
            self.system_dirs = wallpaper::background_dirs(store);
        }
        self.bundled_pending = true;
        self.jobs.push(Job::Bundled);
        self.reread();
    }
    /// Rereads the collections and the folders' pictures (reads only).
    fn reread(&mut self) {
        let Some(store) = self.core.store() else {
            return;
        };
        self.system = media::system_wallpapers(&self.system_dirs);
        self.cosmic_images = collection::cosmic_settings_images(store);
        if !self.session {
            self.collection = Collection::load(store);
        }
        self.tiles.clear();
    }

    pub(super) fn session_draft(&self) -> Option<SavedWallpaper> {
        self.dirty()
            .then(|| {
                self.draft().cloned().map(|draft| SavedWallpaper {
                    effect: self.effect.clone(),
                    draft,
                    target: self.target.clone(),
                    custom: self.custom.clone(),
                    fit_text: self.fit_text.clone(),
                    accent: self.accent.clone(),
                    colors: self.colors.clone(),
                    accent_text: self.accent_text.clone(),
                })
            })
            .flatten()
    }
    pub(super) fn restore_session(&mut self, saved: SavedWallpaper) -> Result<(), String> {
        if let Some(effect) = &saved.effect {
            effect.validate()?;
        }
        if saved
            .accent_text
            .as_ref()
            .is_some_and(|text| text.len() > 9)
            || saved.custom.len() > 128
            || saved.fit_text.len() > 128
            || saved.fit_text.values().any(|text| text.len() > 64)
            || saved.custom.values().any(|custom| {
                custom.values.len() > 256
                    || !custom.radius.is_finite()
                    || custom
                        .values
                        .iter()
                        .flatten()
                        .any(|value| !value.is_finite())
                    || custom.texts.len() > 256
                    || custom.texts.values().any(|text| text.len() > 64)
            })
        {
            return Err("The saved background draft contains invalid color fields".into());
        }
        if let Some(colors) = &saved.colors {
            colors.validate()?;
        }
        let outline = saved.colors.as_ref().is_some_and(|colors| colors.outline);
        if saved.accent.as_ref().is_some_and(|draft| {
            draft.edits().any(|(control, value)| match control {
                Control::Color(_, ColorRole::Accent) => false,
                Control::Color(_, ColorRole::Outline) => {
                    !(outline && *value == crate::desktop::appearance::Value::Color(None))
                }
                Control::Color(variant, role @ (ColorRole::Background | ColorRole::Primary)) => {
                    !saved.colors.as_ref().is_some_and(|colors| {
                        colors.sync
                            && crate::desktop::profiles::themes::linked_surface(
                                draft,
                                colors.baseline_appearance(),
                                *variant,
                                *role,
                            )
                    })
                }
                _ => true,
            })
        }) {
            return Err("The saved wallpaper accent contains unrelated changes".into());
        }
        self.core
            .keep_session_form_baseline(!saved.custom.is_empty() || !saved.fit_text.is_empty());
        self.core.restore_session(saved.draft)?;
        self.effect = saved.effect;
        self.target = saved.target;
        self.custom = saved.custom;
        self.fit_text = saved.fit_text;
        self.accent = saved.accent;
        self.colors = saved.colors;
        // Reanalyse generated recommendations on recovery (the algorithm may
        // have improved). A manually adjusted swatch or hex remains exact.
        if let Some(colors) = &mut self.colors
            && colors.automatic
            && !colors.adjusted
        {
            colors.stamp = 0;
        }
        self.accent_text = saved.accent_text;
        // A pending accent saved by the previous version becomes an app
        // choice on its next explicit Apply, without an upgrade-time write.
        if self.colors.is_none()
            && let Some(rgb) = self.accent_color()
        {
            self.ensure_colors()?;
            self.colors.as_mut().expect("loaded").accent = Some(rgb);
            self.colors.as_mut().expect("loaded").policy = Some(crate::config::ColorPolicy::Custom);
            self.colors.as_mut().expect("loaded").automatic = false;
        }
        if self.colors.is_some() {
            self.stage_colors();
        }
        self.refresh_accent(false);
        self.sync();
        Ok(())
    }

    pub fn opened(&self) -> bool {
        self.core.opened()
    }
    pub fn store(&self) -> Option<&DesktopStore> {
        self.core.store()
    }
    pub fn unavailable(&self) -> Option<&str> {
        self.core.unavailable()
    }
    pub fn writable(&self) -> bool {
        self.core.writable()
    }
    pub fn draft(&self) -> Option<&WallpaperDraft> {
        self.core.draft()
    }
    pub fn notice(&self) -> Option<&Notice> {
        self.core.notice.as_ref()
    }
    /// Edits or typed colour text not applied yet.
    pub fn dirty(&self) -> bool {
        self.effect.is_some()
            || self.core.dirty()
            || self.colors_dirty()
            || self.accent_dirty()
            || self.typed_error().is_some()
    }
    pub fn accent_input_invalid(&self) -> bool {
        self.accent_text
            .as_ref()
            .is_some_and(|text| parse_rgb(text).is_none())
    }
    pub fn valid_accent_input(&self) -> bool {
        self.accent_text
            .as_ref()
            .is_some_and(|text| parse_rgb(text).is_some())
    }
    pub fn background_dirty(&self) -> bool {
        self.core.dirty() || self.background_typed_error().is_some()
    }
    pub fn recovery(&self) -> Option<&Result<Journal, String>> {
        self.core.recovery()
    }
    #[allow(dead_code)] // Profile overlap checks (Task 3) and tests.
    pub fn latest(&self) -> Option<&LatestChange> {
        self.core.latest()
    }
    #[cfg(test)]
    pub fn elsewhere(&self) -> Option<&(Owner, String)> {
        self.core.elsewhere()
    }
    pub fn can_undo(&self) -> bool {
        self.core.can_undo() && !self.dirty()
    }
    /// Apply needs the shared lifecycle's conditions, finished colour text
    /// and the cached checks of every newly shown picture or folder.
    pub fn can_apply(&self) -> bool {
        self.dirty()
            && !self.effect_conflict
            && self.draft().is_some()
            && self.writable()
            && self.recovery().is_none()
            && self.blocked().is_empty()
            && self.accent_conflicts().is_empty()
            && !self.colors_conflict
            && self.typed_error().is_none()
            && self.problem.is_none()
            && self.colors_ready()
    }

    pub fn accent_dirty(&self) -> bool {
        self.accent.as_ref().is_some_and(AppearanceDraft::dirty)
    }
    /// Whether the active window outline has its own color in either theme,
    /// so a shared accent would not reach it. Read when the page opens.
    pub fn outline_custom(&self) -> bool {
        self.outline_custom
    }
    /// The outline will follow the accent once colors apply.
    pub fn outline_linked(&self) -> bool {
        self.colors.as_ref().is_some_and(|colors| colors.outline)
    }
    fn refresh_outline(&mut self) {
        self.outline_custom = self
            .store()
            .and_then(|store| AppearanceDraft::load(store).ok())
            .is_some_and(|draft| {
                [ThemeVariant::Light, ThemeVariant::Dark]
                    .into_iter()
                    .any(|variant| {
                        draft
                            .color(variant, ColorRole::Outline)
                            .known()
                            .is_some_and(Option::is_some)
                            || draft.derived_outline(variant).is_some()
                    })
            });
    }
    pub fn accent_patch(&self) -> Option<DesktopPatch> {
        self.accent
            .as_ref()
            .filter(|draft| draft.dirty())
            .map(AppearanceDraft::patch)
    }
    pub fn colors_dirty(&self) -> bool {
        self.colors.is_some()
    }
    pub fn colors_conflict(&self) -> bool {
        self.colors_conflict
    }
    pub fn sync_colors(&self, config: &crate::config::Config) -> bool {
        self.colors
            .as_ref()
            .map_or(config.appearance.sync_colors, |colors| colors.sync)
    }
    pub fn automatic_colors(&self, config: &crate::config::Config) -> bool {
        self.colors
            .as_ref()
            .map_or(config.appearance.wallpaper_colors, |colors| {
                colors.automatic
            })
    }
    pub fn color_text(&self, config: &crate::config::Config) -> String {
        self.accent_text.clone().unwrap_or_else(|| {
            let rgb = self
                .accent_color()
                .or(config.appearance.accent)
                .unwrap_or_else(|| {
                    let color = crate::config::Palette::named(&config.theme).accent;
                    [color.r, color.g, color.b].map(|v| (v * 255.).round() as u8)
                });
            hex(rgb.map(|v| v as f32 / 255.))
        })
    }
    pub fn automatic_status(&self) -> String {
        match &self.automatic_suggestion {
            Some(Err(reason)) => reason.clone(),
            None => "Finding the best color…".into(),
            Some(Ok(_)) => "Updates with each wallpaper. Adjust this one below.".into(),
        }
    }
    /// A suggestion was found, so the status line has nothing new to say.
    pub fn automatic_ready(&self) -> bool {
        matches!(self.automatic_suggestion, Some(Ok(_)))
    }
    pub fn color_source_label(&self) -> Option<&str> {
        self.automatic_source
            .as_ref()
            .map(|source| source.label.as_str())
    }
    pub fn selected_palette<'a>(&'a self, config: &'a crate::config::Config) -> Option<&'a str> {
        match &self.colors {
            Some(colors) => (colors.family.is_none() && colors.accent.is_none())
                .then_some(colors.theme.as_str()),
            None if config.appearance.family.is_some() => None,
            None => config
                .appearance
                .accent
                .is_none()
                .then_some(config.theme.as_str()),
        }
    }
    pub fn preview_palette(&self, config: &crate::config::Config) -> crate::config::Palette {
        self.colors
            .as_ref()
            .map_or_else(|| config.palette(), Colors::palette)
    }
    pub fn accent_color(&self) -> Option<[u8; 3]> {
        if let Some(colors) = &self.colors {
            return Some(colors.rgb());
        }
        let Some(draft) = self.accent.as_ref() else {
            return self
                .store()
                .and_then(|store| automatic::read_config(store).ok())
                .and_then(|(_, config)| config.appearance.accent);
        };
        let Setting::Known(Some(color)) = draft.color(ThemeVariant::Dark, ColorRole::Accent) else {
            return None;
        };
        Some([color.red, color.green, color.blue].map(|value| (value * 255.).round() as u8))
    }
    pub fn can_apply_colors(&self) -> bool {
        self.colors_dirty()
            && self.draft().is_some()
            && self.writable()
            && self.recovery().is_none()
            && !self.colors_conflict
            && self.accent_conflicts().is_empty()
            && self
                .accent
                .as_ref()
                .is_none_or(|draft| draft.blocked().is_empty())
            && self.colors_ready()
    }
    pub fn colors_ready(&self) -> bool {
        self.automatic_analysis.is_none()
            && self
                .colors
                .as_ref()
                .is_none_or(|colors| !colors.sync || self.accent.is_some())
    }
    fn clear_colors(&mut self) {
        self.pending_colors_commit = false;
        self.colors = None;
        self.accent = None;
        self.colors_conflict = false;
    }
    fn ensure_colors(&mut self) -> Result<(), String> {
        if self.colors.is_none() {
            self.colors = Some(Colors::load(
                self.store().ok_or("Color settings unavailable")?,
            )?);
        }
        Ok(())
    }
    fn select_palette(&mut self, theme: String) {
        if !crate::config::Palette::CHOICES
            .iter()
            .any(|(key, _)| *key == theme)
        {
            return;
        }
        if let Err(error) = self.ensure_colors() {
            self.set_notice(Notice::Error(error));
            return;
        }
        let colors = self.colors.as_mut().expect("loaded");
        colors.theme = theme;
        colors.accent = colors.family.map(|_| {
            let c = crate::config::Palette::named(&colors.theme).accent;
            crate::desktop_theme::bytes(c)
        });
        colors.automatic = false;
        colors.policy = Some(crate::config::ColorPolicy::Custom);
        self.stage_colors();
    }
    fn stage_colors(&mut self) {
        let Some(colors) = &self.colors else {
            return;
        };
        if !colors.sync {
            self.accent = None;
            self.core.notice = None;
            return;
        }
        let rgb = colors.rgb();
        if self.accent.is_none() {
            let Some(store) = self.store() else {
                return;
            };
            match AppearanceDraft::load(store) {
                Ok(draft) => self.accent = Some(draft),
                Err(error) => {
                    self.set_notice(Notice::failed(&error));
                    return;
                }
            }
        }
        let outline = colors.outline;
        let draft = self.accent.as_mut().expect("loaded");
        crate::desktop::profiles::themes::linked_colors(draft, colors.baseline_appearance(), rgb);
        crate::desktop::profiles::themes::linked_accents(draft, colors.baseline_appearance(), rgb);
        for variant in [ThemeVariant::Light, ThemeVariant::Dark] {
            let control = Control::Color(variant, ColorRole::Outline);
            if outline {
                draft.set(control, crate::desktop::appearance::Value::Color(None));
            } else {
                draft.forget(control);
            }
        }
        self.core.notice = None;
    }
    pub fn accent_conflicts(&self) -> &[crate::desktop::ConfigKey] {
        self.accent.as_ref().map_or(&[], AppearanceDraft::conflicts)
    }
    pub fn set_notice(&mut self, notice: Notice) {
        self.core.notice = Some(notice);
    }
    fn refresh_accent(&mut self, accept: bool) -> bool {
        if accept && (!self.accent_conflicts().is_empty() || self.colors_conflict) {
            self.clear_colors();
            return true;
        }
        if let Some(colors) = &self.colors
            && let Some(store) = self.store()
        {
            match colors.conflicts(store) {
                Ok(conflict) => self.colors_conflict = conflict,
                Err(error) => {
                    self.colors_conflict = true;
                    self.set_notice(Notice::Error(error));
                    return false;
                }
            }
        }
        let Some(store) = self.store() else {
            return false;
        };
        if self.accent.is_none() {
            return true;
        }
        match AppearanceDraft::load(store) {
            Ok(fresh) => {
                let accent = self.accent.as_mut().expect("checked");
                if accept {
                    accent.resolve_conflicts(fresh.into_snapshot(), &[]);
                } else {
                    accent.refresh(fresh.into_snapshot());
                }
                true
            }
            Err(error) => {
                self.core.notice = Some(Notice::failed(&error));
                false
            }
        }
    }
    /// The applied theme's style, if a theme is applied.
    fn family(&self) -> Option<crate::desktop_theme::Family> {
        match &self.colors {
            Some(colors) => colors.family,
            None => self
                .store()
                .and_then(|store| automatic::read_config(store).ok())
                .and_then(|(_, config)| config.appearance.family),
        }
    }
    /// The wallpaper's recommended color for the applied look. On a grey
    /// picture a theme keeps its own color, as on Themes and at runtime.
    fn recommended(&self, suggestion: &Suggestion) -> Option<[u8; 3]> {
        automatic::recommended_for(suggestion, self.family())
    }
    /// A theme draws wallpaper text for the wallpaper's polarity, which
    /// Themes and the runtime write with its colors. An Apply that shows a
    /// wallpaper settles it too, since the runtime then skips that wallpaper
    /// (`keep_manual_colors`). `None` when nothing needs writing; an error
    /// while the analysis is of another wallpaper.
    fn polarity(&self) -> Result<Option<WidgetPatch>, String> {
        let (Some(store), Some(draft)) = (self.store(), self.draft()) else {
            return Ok(None);
        };
        let Some(Ok(suggestion)) = &self.automatic_suggestion else {
            return Ok(None);
        };
        if self.family().is_none() {
            return Ok(None);
        }
        let current = automatic::source(store, draft).stamp;
        if self.automatic_source.as_ref().map(|source| source.stamp) != Some(current) {
            return Err(FINDING_COLORS.into());
        }
        let (raw, config) = automatic::read_config(store)?;
        let light = automatic::light(suggestion);
        if light == config.appearance.wallpaper_light {
            return Ok(None);
        }
        let pointer = "/appearance/wallpaper_light";
        let mut patch = WidgetPatch::new();
        patch.set(
            pointer,
            widget::get(&raw, pointer),
            Some(serde_json::json!(light)),
        )?;
        Ok(Some(patch))
    }
    /// Applies the background draft; a theme's wallpaper polarity goes with
    /// it in the same change and Undo.
    fn apply_background(&mut self) {
        let mut polarity = match self.polarity() {
            Ok(polarity) => polarity,
            Err(reason) => {
                self.set_notice(polarity_notice(reason));
                return;
            }
        };
        if let Err(error) = self.merge_effect(&mut polarity) {
            self.set_notice(Notice::Error(error));
            return;
        }
        let applied = match polarity {
            None => self.core.apply(),
            Some(polarity) => {
                let draft = self.draft().expect("open");
                let mut patch = draft.plan().patch;
                draft.guard(&mut patch);
                let mut rest = draft.clone();
                rest.discard();
                self.core
                    .apply_with_widgets(&patch, Some(&polarity), "Wallpaper", rest, true)
            }
        };
        if applied {
            self.effect = None;
            self.keep_applied_colors();
        }
    }
    fn select_accent(&mut self, rgb: [u8; 3]) {
        if !self.suggestion().is_some_and(|suggestion| {
            suggestion.accents.contains(&rgb) || self.recommended(suggestion) == Some(rgb)
        }) {
            return;
        }
        if let Err(error) = self.ensure_colors() {
            self.set_notice(Notice::Error(error));
            return;
        }
        let colors = self.colors.as_mut().expect("loaded");
        colors.accent = Some(rgb);
        colors.adjusted = true;
        colors.automatic = false;
        colors.policy = Some(crate::config::ColorPolicy::Custom);
        if self.automatic_active
            && let Some(source) = &self.automatic_source
        {
            self.colors.as_mut().expect("loaded").stamp = source.stamp;
        }
        self.accent_text = None;
        self.stage_colors();
    }
    fn apply_colors(&mut self, include_background: bool) {
        self.refresh_accent(false);
        if !(if include_background {
            self.can_apply()
        } else {
            self.can_apply_colors()
        }) {
            return;
        }
        let background_changed = include_background && self.core.dirty();
        let mut patch = if include_background {
            self.draft().expect("open").plan().patch
        } else {
            DesktopPatch::new()
        };
        if let Some(colors) = self.colors.as_ref().filter(|colors| colors.automatic) {
            let draft = self.draft().expect("open");
            let current = automatic::source(self.store().expect("open"), draft);
            if current.stamp != colors.stamp {
                self.automatic_source = None;
                self.set_notice(Notice::Busy(FINDING_COLORS.into()));
                return;
            }
            draft.guard(&mut patch);
        }
        if let Some(accent) = self.accent_patch()
            && let Err(error) = patch.merge(accent)
        {
            self.set_notice(Notice::failed(&error));
            return;
        }
        let mut widgets = match self.colors.as_ref().map(Colors::patch).transpose() {
            Ok(widgets) => widgets,
            Err(error) => {
                self.set_notice(Notice::Error(error));
                return;
            }
        };
        if include_background && let Err(error) = self.merge_effect(&mut widgets) {
            self.set_notice(Notice::Error(error));
            return;
        }
        // A theme's wallpaper polarity, for the wallpaper this Apply shows.
        if include_background || !self.core.dirty() {
            match self.polarity() {
                Ok(Some(polarity)) => {
                    let draft = self.draft().expect("open");
                    draft.guard(&mut patch);
                    let merged = match widgets.as_mut() {
                        Some(widgets) => widgets.merge(polarity).map_err(|overlaps| {
                            format!(
                                "Overlaps another part of this change: {}",
                                overlaps.join(", ")
                            )
                        }),
                        None => {
                            widgets = Some(polarity);
                            Ok(())
                        }
                    };
                    if let Err(error) = merged {
                        self.set_notice(Notice::Error(error));
                        return;
                    }
                }
                Ok(None) => {}
                Err(reason) => {
                    self.set_notice(polarity_notice(reason));
                    return;
                }
            }
        }
        if patch.is_empty() && widgets.as_ref().is_none_or(|patch| patch.is_empty()) {
            // No transaction will run, so verify both participants before
            // reporting success. A watcher may not have delivered its event.
            if !self.refresh_accent(false) {
                return;
            }
            if self.colors_conflict || !self.accent_conflicts().is_empty() {
                self.set_notice(Notice::Error(
                    "Colors changed elsewhere. Nothing was applied.".into(),
                ));
                return;
            }
            self.clear_colors();
            self.set_notice(Notice::Done("Colors already applied".into()));
            return;
        }
        let sync = self.colors.as_ref().is_some_and(|colors| colors.sync);
        let mut rest = self.draft().expect("open").clone();
        if include_background {
            rest.discard();
        }
        let label = if background_changed {
            "Wallpaper and colors"
        } else {
            "Colors"
        };
        if self
            .core
            .apply_with_widgets(&patch, widgets.as_ref(), label, rest, include_background)
        {
            self.keep_applied_colors();
            self.clear_colors();
            if include_background {
                self.effect = None;
            }
            let message = if sync {
                "Colors applied to widgets and Pop!_OS"
            } else {
                "Colors applied to widgets"
            };
            self.set_notice(Notice::Done(if background_changed {
                format!("Wallpaper applied · {message}")
            } else {
                message.into()
            }));
        } else {
            self.refresh_accent(false);
        }
    }
    fn keep_applied_colors(&self) {
        if let Some(store) = self.store()
            && let Err(error) = automatic::keep_manual_colors(store)
        {
            eprintln!("Keeping applied wallpaper colors: {error}");
        }
    }
    /// Why a newly shown source can't be applied (cached; Apply rechecks).
    pub fn source_problem(&self) -> Option<&str> {
        self.problem.as_deref()
    }
    /// Unfinished colour text that holds back Apply; another display's
    /// is named so it can be found.
    pub fn typed_error(&self) -> Option<String> {
        if self
            .accent_text
            .as_ref()
            .is_some_and(|text| parse_rgb(text).is_none())
        {
            return Some(TYPED.into());
        }
        self.background_typed_error()
    }
    fn background_typed_error(&self) -> Option<String> {
        let invalid = |target: &Target| {
            self.custom
                .get(target)
                .is_some_and(|custom| custom.invalid().is_some())
                || self
                    .fit_text
                    .get(target)
                    .is_some_and(|text| parse_rgb(text).is_none())
        };
        if invalid(&self.target) {
            return Some(TYPED.to_string());
        }
        let other = self
            .custom
            .keys()
            .chain(self.fit_text.keys())
            .find(|target| invalid(target))?;
        Some(match other {
            Target::Shared => {
                "Finish the shared background’s color code (#RRGGBB) or pick a color".to_string()
            }
            Target::Output(name) => format!(
                "Finish the color code (#RRGGBB) for {} or pick a color",
                display_name(name)
            ),
        })
    }
    pub fn blocked(&self) -> Vec<crate::desktop::appearance::Blocked> {
        let mut blocked = self.core.blocked();
        if let Some(accent) = &self.accent {
            blocked.extend(accent.blocked());
        }
        blocked
    }
    pub fn watch_paths(&self) -> Vec<PathBuf> {
        let mut paths = self.core.watch_paths();
        if let Some(store) = self.store() {
            if self.colors.is_some()
                && let Some(path) = store.app_config()
            {
                paths.push(path.to_path_buf());
            }
            if self.accent.is_some() {
                paths.extend(
                    AppearanceDraft::keys()
                        .iter()
                        .map(|key| store.user_path(key)),
                );
            }
            if let Some(path) = media::state_path(store) {
                paths.push(path);
            }
        }
        paths.extend(self.shown.image.iter().cloned());
        paths.extend(
            self.automatic_source
                .as_ref()
                .and_then(automatic::Source::picture)
                .map(Path::to_path_buf),
        );
        paths
    }
    pub fn target(&self) -> &Target {
        &self.target
    }
    pub fn bundled_dir(&self) -> &Path {
        &self.bundled_dir
    }
    /// Every tile of the open tab.
    pub fn tiles(&self) -> &[Tile] {
        &self.tiles
    }
    /// The tiles of the gallery page that shows.
    pub fn page_tiles(&self) -> &[Tile] {
        let start = (self.page * PAGE).min(self.tiles.len());
        &self.tiles[start..(start + PAGE).min(self.tiles.len())]
    }
    pub fn page(&self) -> usize {
        self.page
    }
    /// Gallery pages of the open tab (at least one).
    pub fn pages(&self) -> usize {
        self.tiles.len().div_ceil(PAGE).max(1)
    }
    /// Whether `path` is an item of the app's collection (also a session
    /// one); other pictures on *Yours* can't be removed from it.
    pub fn in_collection(&self, path: &Path) -> bool {
        self.collection
            .as_ref()
            .is_ok_and(|collection| collection.contains(path))
    }
    pub fn collection_error(&self) -> Option<&str> {
        self.collection.as_ref().err().map(String::as_str)
    }
    pub fn session(&self) -> bool {
        self.session
    }
    pub fn shown(&self) -> &Shown {
        &self.shown
    }
    pub fn suggestion(&self) -> Option<&Suggestion> {
        if self.automatic_active {
            self.automatic_suggestion
                .as_ref()
                .and_then(|result| result.as_ref().ok())
        } else {
            self.suggestion.as_ref().map(|(_, suggestion)| suggestion)
        }
    }
    /// The stored (applied) source of the selected display.
    pub fn current(&self) -> Option<&BgSource> {
        self.current.as_ref()
    }
    pub fn custom(&self) -> Option<&Custom> {
        self.custom.get(&self.target)
    }
    pub fn fit_text(&self) -> Option<&str> {
        self.fit_text.get(&self.target).map(String::as_str)
    }
    /// A tile's thumbnail handle, if decoded.
    pub fn thumb(&self, tile: &Tile) -> Option<&Handle> {
        match &tile.pick {
            Pick::Bundled(name) => self
                .bundled
                .get(name.as_str())
                .and_then(|thumb| thumb.handle.as_ref()),
            Pick::Image(path) => self
                .keys
                .get(path)
                .and_then(|key| self.thumbs.get(key))
                .and_then(|thumb| thumb.handle.as_ref()),
            Pick::Color(color) => self
                .color_tiles
                .iter()
                .find(|(known, _)| known == color)
                .map(|(_, handle)| handle),
            Pick::Folder(_) => None,
        }
    }
    /// A tile picture's state when it has no thumbnail.
    pub fn thumb_note(&self, tile: &Tile) -> Option<&'static str> {
        match &tile.pick {
            Pick::Image(path) if !previewed(path) => Some(media::NO_PREVIEW),
            Pick::Image(path) => match self.keys.get(path).and_then(|key| self.thumbs.get(key)) {
                Some(Thumb {
                    preview: Preview::None(reason),
                    ..
                }) => Some(reason),
                Some(_) => None,
                None => None,
            },
            _ => None,
        }
    }
    pub fn scan(&self, path: &Path) -> Option<&Result<FolderScan, String>> {
        self.scans.get(path)
    }
    /// Displays the page lists (connected, listed, drafted).
    pub fn displays(&self) -> Vec<DisplayView> {
        self.draft()
            .map(WallpaperDraft::display_views)
            .unwrap_or_default()
    }
    /// The drafted entry of the selected display (`None`: it uses the
    /// shared background).
    pub fn entry(&self) -> Option<Setting<BgEntry>> {
        self.draft()?.entry(&self.target)
    }
    /// What the selected display shows after the draft.
    pub fn effective(&self) -> Option<Setting<BgEntry>> {
        Some(self.draft()?.effective(&self.target))
    }
    /// Why the selected target can't be edited, if it can't.
    pub fn target_blocked(&self) -> Option<String> {
        self.blocked_for(&self.target)
    }
    fn blocked_for(&self, target: &Target) -> Option<String> {
        let draft = self.draft()?;
        match target {
            Target::Shared => match draft.entry(&Target::Shared)? {
                Setting::Unsupported(raw) => Some(raw),
                Setting::Known(_) => None,
            },
            Target::Output(name) => {
                if draft.displays() != Setting::Known(Displays::Separate) {
                    return None;
                }
                match self
                    .displays()
                    .into_iter()
                    .find(|view| view.name == *name)
                    .and_then(|view| view.own)
                {
                    Some(Setting::Unsupported(raw)) => Some(raw),
                    _ => match draft.entry(target) {
                        Some(Setting::Unsupported(raw)) => Some(raw),
                        _ => None,
                    },
                }
            }
        }
    }
    /// Picture tiles' thumbnails decoded so far (for capture state).
    pub fn state_key(&self) -> String {
        format!(
            "{}:{:?}:{:?}:{:?}:{}:{}:{}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}",
            self.core.state_key(),
            self.draft().map(|draft| draft.displays()),
            self.draft().map(|draft| draft.effective(&self.target)),
            self.tab,
            self.page,
            self.thumbs.len(),
            self.bundled.len(),
            self.target,
            self.preview_key,
            self.suggestion.as_ref().map(|(source, _)| source),
            self.custom,
            self.fit_text,
            (&self.accent, &self.colors, self.colors_conflict),
            (
                &self.automatic_source,
                &self.automatic_suggestion,
                &self.accent_text,
                self.palette_choices
            ),
        )
    }
    /// An outside change (a controller command, a profile, Undo on
    /// another page): the same reread and reconciliation as Refresh. The
    /// app runs the work it starts (`take_jobs`).
    pub fn refresh(&mut self) {
        self.update(Message::Refresh);
    }
    /// Thumbnail decodes, folder scan and analysis in flight (tests).
    #[cfg(test)]
    pub fn in_flight(&self) -> usize {
        self.decoding.len()
            + usize::from(self.scanning.is_some())
            + usize::from(self.analysis.is_some())
            + usize::from(self.automatic_analysis.is_some())
    }
    /// Background work the last messages started (drained by the app).
    pub fn take_jobs(&mut self) -> Vec<Job> {
        std::mem::take(&mut self.jobs)
    }
    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn update(&mut self, message: Message) -> Outcome {
        let outcome = self.handle(message);
        self.sync();
        if self.pending_colors_commit && !self.background_dirty() && self.can_apply_colors() {
            self.apply_colors(false);
        }
        if self.typed_error().is_none() {
            self.core.keep_session_form_baseline(false);
        }
        outcome
    }

    fn edit_target(&mut self, change: impl FnOnce(&mut WallpaperDraft, &Target)) {
        if self.target_blocked().is_some() {
            return;
        }
        self.pending_colors_commit = false;
        let target = self.target.clone();
        if let Some(colors) = &mut self.colors {
            colors.adjusted = false;
        }
        self.core.edit(|draft| change(draft, &target));
    }

    fn handle(&mut self, message: Message) -> Outcome {
        match message {
            Message::Tab(tab) => {
                self.tab = tab;
                self.page = 0;
                self.tiles.clear();
            }
            Message::Page(page) => self.page = page,
            Message::Select(pick) => {
                let source = match &pick {
                    Pick::Bundled(name) => {
                        if wallpaper::bundled_name(name).is_none() {
                            return Outcome::None;
                        }
                        BgSource::Path(wallpaper::bundled_path(&self.bundled_dir, name))
                    }
                    Pick::Image(path) | Pick::Folder(path) => BgSource::Path(path.clone()),
                    Pick::Color(color) => BgSource::Color(color.clone()),
                };
                if self.target_blocked().is_none() {
                    self.custom.remove(&self.target);
                }
                self.edit_target(|draft, target| draft.set_source(target, source));
            }
            Message::Fit(choice) => {
                let scaling = match choice {
                    FitChoice::Fill => ScalingMode::Zoom,
                    FitChoice::Stretch => ScalingMode::Stretch,
                    FitChoice::Fit => match self.entry_scaling() {
                        Some(ScalingMode::Fit(color)) => ScalingMode::Fit(color),
                        _ => ScalingMode::Fit([0., 0., 0.]),
                    },
                };
                if choice != FitChoice::Fit {
                    self.fit_text.remove(&self.target);
                }
                self.edit_target(|draft, target| draft.set_scaling(target, scaling));
            }
            Message::FitColor(text) => {
                if self.target_blocked().is_some() {
                    return Outcome::None;
                }
                let text: String = text.chars().filter(|c| !c.is_control()).take(9).collect();
                if let Some(rgb) = parse_rgb(&text) {
                    self.edit_target(|draft, target| {
                        draft.set_scaling(target, ScalingMode::Fit(rgb))
                    });
                }
                self.fit_text.insert(self.target.clone(), text);
            }
            Message::Same(same) => {
                if let Some(colors) = &mut self.colors {
                    colors.adjusted = false;
                }
                self.core.edit(|draft| {
                    draft.set_displays(if same {
                        Displays::Same
                    } else {
                        Displays::Separate
                    })
                });
                if same {
                    // Output fields aren't shown any more (as Use shared).
                    self.target = Target::Shared;
                    self.custom.retain(|target, _| *target == Target::Shared);
                    self.fit_text.retain(|target, _| *target == Target::Shared);
                }
            }
            Message::Display(target) => self.target = target,
            Message::Every(seconds) => {
                if ROTATIONS.contains(&seconds) {
                    self.edit_target(|draft, target| draft.set_rotation(target, seconds));
                }
            }
            Message::Order(order) => {
                self.edit_target(|draft, target| draft.set_sampling(target, order));
            }
            Message::UseShared => {
                if let Some(colors) = &mut self.colors {
                    colors.adjusted = false;
                }
                if let Target::Output(name) = self.target.clone() {
                    self.custom.remove(&self.target);
                    self.fit_text.remove(&self.target);
                    self.core.edit(|draft| draft.use_shared(&name));
                }
            }
            Message::AddPicture | Message::AddFolder => {
                let id = self.id();
                self.dialog = Some(id);
                return Outcome::Dialog(Request {
                    id,
                    folder: matches!(message, Message::AddFolder),
                });
            }
            Message::Picked(id, result) => {
                if self.dialog != Some(id) {
                    return Outcome::None;
                }
                self.dialog = None;
                match result {
                    Ok(Some(path)) => self.add(path),
                    Ok(None) => {}
                    Err(error) => self.core.notice = Some(Notice::Error(error)),
                }
            }
            Message::Remove(path) => self.remove(&path),
            Message::Thumb(id, path, preview) => {
                if let Some(key) = self.decoding.remove(&id)
                    && key.0 == path
                    // The file changed while it was decoded: decode again.
                    && thumb_key(&path).as_ref() == Some(&key)
                {
                    self.thumbs.insert(key, Thumb::new(preview));
                }
            }
            Message::Animated(path, animated) => {
                if let Some(key) = self.motion_checking.take_if(|key| key.0 == path) {
                    self.motion = Some((key, animated));
                }
            }
            Message::Bundled(thumbs) => {
                self.bundled_pending = false;
                for (name, pixels) in thumbs {
                    if let Some(pixels) = pixels {
                        self.bundled
                            .insert(name, Thumb::new(Preview::Ready(pixels)));
                    }
                }
            }
            Message::Scanned(id, path, scan) => {
                if self.scanning.as_ref() == Some(&(id, path.clone())) {
                    self.scanning = None;
                    self.remember_scan(path, scan);
                }
            }
            Message::Analysed(id, suggestion) => {
                if let Some((pending, source)) = self.analysis.take() {
                    if pending == id {
                        self.suggestion = Some((source, suggestion));
                    } else {
                        self.analysis = Some((pending, source));
                    }
                }
            }
            Message::AutoAnalysed(id, stamp, suggestion) => {
                if self.automatic_analysis == Some((id, stamp)) {
                    self.automatic_analysis = None;
                    self.automatic_suggestion = Some(suggestion);
                    // The colors Apply waited for are ready.
                    if matches!(self.core.notice, Some(Notice::Busy(_))) {
                        self.core.notice = None;
                    }
                }
            }
            Message::Stop(index, text) => {
                let text: String = text.chars().filter(|c| !c.is_control()).take(9).collect();
                let mut custom = self.custom_or_current();
                if index >= custom.values.len() || custom.values.len() > 3 {
                    return Outcome::None;
                }
                if let Some(rgb) = parse_rgb(&text) {
                    custom.values[index] = if custom.gradient() {
                        rgb.map(wallpaper::srgb_to_linear)
                    } else {
                        rgb
                    };
                }
                custom.texts.insert(index, text);
                self.set_custom(custom);
            }
            Message::Stops(count) => {
                if (1..=3).contains(&count) {
                    let mut custom = self.custom_or_current();
                    custom.resize(count);
                    self.set_custom(custom);
                }
            }
            Message::Direction(degrees) => {
                let mut custom = self.custom_or_current();
                if custom.gradient() && [0, 90, 180, 270].contains(&degrees) {
                    custom.radius = f32::from(degrees);
                    self.set_custom(custom);
                }
            }
            Message::Accent(rgb) => self.select_accent(rgb),
            Message::Palette(theme) => self.select_palette(theme),
            Message::SyncColors(sync) => match self.ensure_colors() {
                Ok(()) => {
                    self.colors.as_mut().expect("loaded").sync = sync;
                    self.stage_colors();
                }
                Err(error) => self.set_notice(Notice::Error(error)),
            },
            Message::LinkOutline(on) => match self.ensure_colors() {
                Ok(()) => {
                    let colors = self.colors.as_mut().expect("loaded");
                    colors.outline = on;
                    if on {
                        colors.overrides &= !((1u64 << 28) | (1u64 << 36));
                    }
                    self.stage_colors();
                }
                Err(error) => self.set_notice(Notice::Error(error)),
            },
            Message::AutomaticColors(enabled) => match self.ensure_colors() {
                Ok(()) => {
                    let colors = self.colors.as_mut().expect("loaded");
                    colors.automatic = enabled;
                    colors.policy = Some(if enabled {
                        crate::config::ColorPolicy::Wallpaper
                    } else {
                        crate::config::ColorPolicy::Custom
                    });
                    if enabled {
                        colors.stamp = 0;
                        colors.adjusted = false;
                    }
                    self.accent_text = None;
                    self.stage_colors();
                }
                Err(error) => self.set_notice(Notice::Error(error)),
            },
            Message::ThemeColor => {
                if let Err(error) = self.ensure_colors() {
                    self.set_notice(Notice::Error(error));
                    return Outcome::None;
                }
                let colors = self.colors.as_mut().expect("loaded");
                colors.automatic = false;
                colors.accent = None;
                colors.policy = Some(crate::config::ColorPolicy::Theme);
                self.accent_text = None;
                self.stage_colors();
            }
            Message::AccentText(text) => {
                let text: String = text.chars().filter(|c| !c.is_control()).take(9).collect();
                if let Err(error) = self.ensure_colors() {
                    self.set_notice(Notice::Error(error));
                    return Outcome::None;
                }
                self.accent_text = Some(text);
            }
            Message::AccentSubmit => {
                if let Some(text) = self.accent_text.clone() {
                    if let Some(rgb) = parse_rgb(&text) {
                        // Text survives independent completed controls, which
                        // may release their color model. Submission starts a
                        // fresh guarded action if that model is no longer here.
                        if let Err(error) = self.ensure_colors() {
                            self.set_notice(Notice::Error(error));
                            return Outcome::None;
                        }
                        let colors = self.colors.as_mut().expect("loaded");
                        colors.adjusted = true;
                        colors.policy = Some(crate::config::ColorPolicy::Custom);
                        colors.automatic = false;
                        colors.accent = Some(rgb.map(|v| (v * 255.).round() as u8));
                        if self.automatic_active
                            && let Some(source) = &self.automatic_source
                        {
                            colors.stamp = source.stamp;
                        }
                        self.accent_text = None;
                        self.stage_colors();
                    }
                }
            }
            Message::PaletteChoices(open) => self.palette_choices = open,
            Message::ApplyColors => {
                if !self.background_dirty() {
                    self.pending_colors_commit = true;
                    self.apply_colors(false);
                }
                self.refresh_outline();
            }
            Message::ResolveThemeAccent => {} // Settings owns the other page.
            Message::KeepColors => {
                // Explicit conflict resolution: keep the chosen accent, using
                // today's theme and preferences as the transaction baseline.
                if let Some(store) = self.store() {
                    match (Colors::load(store), AppearanceDraft::load(store)) {
                        (Ok(fresh), Ok(accent)) => {
                            if let Some(colors) = &mut self.colors {
                                colors.accept_baseline(fresh);
                            }
                            self.accent = Some(accent);
                            self.colors_conflict = false;
                            self.stage_colors();
                        }
                        (Err(error), _) => self.set_notice(Notice::Error(error)),
                        (_, Err(error)) => self.set_notice(Notice::failed(&error)),
                    }
                }
            }
            Message::ResolveEffect(keep) => self.resolve_effect(keep),
            Message::ClearAccent => {
                self.accent_text = None;
                self.clear_colors();
                self.core.notice = None;
            }
            Message::SuggestAccent => {
                if let Some(rgb) = self.suggestion().and_then(|s| self.recommended(s)) {
                    self.select_accent(rgb);
                    if let Some(colors) = &mut self.colors {
                        colors.adjusted = false;
                    }
                }
            }
            // Routed to the app's own preference before it gets here.
            Message::StopMatching => {}
            Message::Reinstall => {
                if self.writable() {
                    return Outcome::Controller(vec!["wallpapers-install".into()]);
                }
                self.core.notice =
                    Some(Notice::Error("Reinstalling is off in preview mode".into()));
            }
            Message::Apply => {
                self.refresh_effect();
                if self.can_apply() {
                    if self.colors_dirty() || self.accent_dirty() {
                        self.apply_colors(true);
                    } else {
                        self.apply_background();
                    }
                    self.settle_typed(true);
                    self.refresh_outline();
                    if !self.dirty() {
                        self.preview_open = false;
                        self.steps.clear();
                    }
                }
            }
            Message::Discard => {
                self.preview_open = false;
                self.steps.clear();
                self.gesture = None;
                self.effect = None;
                self.effect_conflict = false;
                self.core.discard();
                self.custom.clear();
                self.fit_text.clear();
                self.clear_colors();
            }
            Message::Undo => {
                if self.can_undo() {
                    self.core.undo();
                    if self.core.latest().is_none() {
                        self.keep_applied_colors();
                    }
                    self.clear_colors();
                    self.settle_typed(true);
                    self.refresh_outline();
                }
            }
            Message::Refresh => {
                self.refresh_effect();
                self.core.refresh();
                self.recheck();
                self.settle_typed(false);
                self.refresh_accent(false);
                self.refresh_outline();
            }
            Message::UseCurrent => {
                self.core.use_current(&[]);
                self.refresh_accent(true);
                self.settle_typed(true);
            }
            Message::KeepMine => {
                self.core.keep_edits();
                self.settle_typed(true);
            }
            Message::Recover => {
                self.core.recover(true);
                self.refresh_accent(false);
                self.settle_typed(true);
            }
            Message::KeepRecovery => {
                self.core.recover(false);
                self.refresh_accent(false);
                self.settle_typed(true);
            }
        }
        Outcome::None
    }

    fn entry_scaling(&self) -> Option<ScalingMode> {
        match self.effective()? {
            Setting::Known(entry) => Some(entry.scaling_mode),
            Setting::Unsupported(_) => None,
        }
    }
    /// The custom state of the selected display, or one from its colour.
    fn custom_or_current(&self) -> Custom {
        if let Some(custom) = self.custom.get(&self.target) {
            return custom.clone();
        }
        match self.effective().and_then(color_of) {
            Some(color) => Custom::of(&color),
            None => Custom {
                origin: None,
                ..Custom::of(&BgColor::Single([0.2, 0.4, 0.6]))
            },
        }
    }
    /// Keeps the typed state; a complete one goes into the draft.
    fn set_custom(&mut self, mut custom: Custom) {
        if self.target_blocked().is_some() {
            return;
        }
        if custom.invalid().is_none() {
            let color = custom.color();
            self.edit_target(|draft, target| draft.set_source(target, BgSource::Color(color)));
            custom.origin = self.effective().and_then(color_of);
        }
        self.custom.insert(self.target.clone(), custom);
    }
    /// Typed colours after the draft changed underneath (Refresh, Undo,
    /// Use current, recovery, Apply). Complete text gives way to the
    /// draft; unfinished text stays on the rebased fields (`rebase`). With
    /// Refresh, an edit that waited only for text whose stop is gone goes
    /// into the draft rather than being lost.
    fn settle_typed(&mut self, accept: bool) {
        self.prune_typed();
        let Some(draft) = self.core.draft().cloned() else {
            return;
        };
        for (target, custom) in std::mem::take(&mut self.custom) {
            let now = color_of(draft.effective(&target));
            let Some(custom) = custom.rebase(now.as_ref(), accept) else {
                continue;
            };
            if custom.invalid().is_some() {
                self.custom.insert(target, custom);
            } else if now.as_ref() != Some(&custom.color()) && !accept {
                let color = custom.color();
                self.core
                    .edit(|draft| draft.set_source(&target, BgSource::Color(color)));
            }
        }
        self.fit_text.retain(|_, text| parse_rgb(text).is_none());
    }
    /// Drops typed state the page no longer shows: a display that isn't
    /// listed or separate any more, one that can't be edited, a letterbox
    /// colour whose background isn't a fitted picture. Other displays'
    /// reachable text stays.
    fn prune_typed(&mut self) {
        if self.custom.is_empty() && self.fit_text.is_empty() {
            return;
        }
        let Some(draft) = self.core.draft().cloned() else {
            return;
        };
        let views = draft.display_views();
        let separate = draft.displays() == Setting::Known(Displays::Separate);
        let reachable = |editor: &Self, target: &Target| {
            let listed = match target {
                Target::Shared => true,
                Target::Output(name) => separate && views.iter().any(|view| view.name == *name),
            };
            listed && editor.blocked_for(target).is_none()
        };
        let custom: Vec<Target> = self.custom.keys().cloned().collect();
        for target in custom {
            if !reachable(self, &target) {
                self.custom.remove(&target);
            }
        }
        let fit: Vec<Target> = self.fit_text.keys().cloned().collect();
        for target in fit {
            let fitted = matches!(
                draft.entry(&target),
                Some(Setting::Known(BgEntry {
                    source: BgSource::Path(path),
                    scaling_mode: ScalingMode::Fit(_),
                    ..
                })) if !path.is_dir()
            );
            if !fitted || !reachable(self, &target) {
                self.fit_text.remove(&target);
            }
        }
    }

    /// Adds a chosen picture or folder to *Yours* (saved unless preview)
    /// and selects it.
    fn add(&mut self, path: PathBuf) {
        let folder = path.is_dir();
        let item = Item {
            kind: if folder { Kind::Folder } else { Kind::Image },
            path: path.clone(),
        };
        let writable = self.writable();
        let mut collection = match (writable, self.core.store()) {
            // The file may have changed since it was read: start from it.
            (true, Some(store)) => Collection::load(store).unwrap_or_default(),
            _ => self.collection.clone().unwrap_or_default(),
        };
        if !collection.contains(&path) && !collection.add(item) {
            self.core.notice = Some(Notice::Error(
                "This picture can’t be added to your collection".into(),
            ));
            return;
        }
        self.tab = Tab::Yours;
        self.reveal = Some(path.clone());
        let pick = if folder {
            Pick::Folder(path)
        } else {
            Pick::Image(path)
        };
        // Selecting is a draft edit (it clears the notice); the save's
        // notice comes after it.
        self.handle(Message::Select(pick));
        self.save_collection(collection);
        self.tiles.clear();
    }
    fn remove(&mut self, path: &Path) {
        let mut collection = match (self.writable(), self.core.store()) {
            (true, Some(store)) => match Collection::load(store) {
                Ok(collection) => collection,
                Err(error) => {
                    self.core.notice = Some(Notice::Error(error));
                    return;
                }
            },
            _ => self.collection.clone().unwrap_or_default(),
        };
        if collection.remove(path) {
            self.save_collection(collection);
            self.tiles.clear();
        }
    }
    fn save_collection(&mut self, collection: Collection) {
        match self.core.store() {
            Some(store) if store.writable() => match collection.save(store) {
                Ok(()) => self.collection = Ok(collection),
                Err(error) => self.core.notice = Some(Notice::Error(error)),
            },
            _ => {
                self.session = true;
                self.collection = Ok(collection);
                self.core.notice = Some(Notice::Done(ADDED_IN_PREVIEW.into()));
            }
        }
    }
    /// Forgets cached checks of edited folders and rereads collections.
    fn recheck(&mut self) {
        if let Some(draft) = self.core.draft() {
            for path in draft.edited_paths() {
                self.scans.remove(&path);
            }
        }
        self.keys.clear();
        self.reread();
    }
    fn remember_scan(&mut self, path: PathBuf, scan: Result<FolderScan, String>) {
        self.scan_order.retain(|known| *known != path);
        self.scan_order.push_back(path.clone());
        self.scans.insert(path, scan);
        while self.scan_order.len() > SCANS {
            if let Some(oldest) = self.scan_order.pop_front() {
                self.scans.remove(&oldest);
            }
        }
    }

    /// The gallery of the open tab.
    fn build_tiles(&mut self) {
        let mut tiles = Vec::new();
        let file_tile = |path: &Path, external: bool| {
            let folder = path.is_dir();
            Tile {
                label: path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string()),
                missing: !path.exists(),
                external,
                pick: if folder {
                    Pick::Folder(path.to_path_buf())
                } else {
                    Pick::Image(path.to_path_buf())
                },
            }
        };
        match self.tab {
            Tab::Bundled => {
                for (name, title) in wallpaper::BUNDLED {
                    tiles.push(Tile {
                        pick: Pick::Bundled(name.to_string()),
                        label: title.to_string(),
                        missing: !wallpaper::bundled_path(&self.bundled_dir, name).is_file(),
                        external: false,
                    });
                }
            }
            Tab::Cosmic => {
                for dir in self.system_dirs.iter().filter(|dir| dir.is_dir()) {
                    let mut tile = file_tile(dir, false);
                    tile.label = "All as slideshow".into();
                    tiles.push(tile);
                }
                for path in &self.system {
                    tiles.push(file_tile(path, false));
                }
            }
            Tab::Yours => {
                let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
                if let Ok(collection) = &self.collection {
                    for item in collection.items() {
                        seen.insert(item.path.clone());
                        let mut tile = file_tile(&item.path, false);
                        tile.pick = match item.kind {
                            Kind::Folder => Pick::Folder(item.path.clone()),
                            Kind::Image => Pick::Image(item.path.clone()),
                        };
                        tiles.push(tile);
                    }
                }
                for path in &self.cosmic_images {
                    if seen.insert(path.clone()) {
                        tiles.push(file_tile(path, true));
                    }
                }
                // Personal pictures the desktop uses now.
                let dirs = self.system_dirs.clone();
                for path in self.configured_paths() {
                    if wallpaper::classify(&path, &self.bundled_dir, &dirs) == Class::Personal
                        && seen.insert(path.clone())
                    {
                        tiles.push(file_tile(&path, false));
                    }
                }
            }
            Tab::Colors => {
                if self.color_tiles.is_empty() {
                    self.color_tiles = default_colors()
                        .into_iter()
                        .map(|color| {
                            let pixels = media::render_color(&color, 204, 116);
                            let handle =
                                Handle::from_rgba(pixels.width, pixels.height, pixels.rgba);
                            (color, handle)
                        })
                        .collect();
                }
                for color in default_colors() {
                    tiles.push(Tile {
                        pick: Pick::Color(color),
                        label: String::new(),
                        missing: false,
                        external: false,
                    });
                }
            }
        }
        self.tiles = tiles;
    }
    /// Picture paths of every stored entry (shared and displays).
    fn configured_paths(&self) -> Vec<PathBuf> {
        let Some(draft) = self.core.draft() else {
            return Vec::new();
        };
        let mut base = draft.clone();
        base.discard();
        let mut targets = vec![Target::Shared];
        targets.extend(
            base.display_views()
                .into_iter()
                .map(|view| Target::Output(view.name)),
        );
        targets
            .iter()
            .filter_map(|target| match base.effective(target) {
                Setting::Known(BgEntry {
                    source: BgSource::Path(path),
                    ..
                }) => Some(path),
                _ => None,
            })
            .collect()
    }

    /// The display name a target's slideshow state is read for.
    fn state_output(&self) -> Option<String> {
        match &self.target {
            Target::Output(name) => Some(name.clone()),
            Target::Shared => self
                .displays()
                .into_iter()
                .find(|view| view.connected)
                .map(|view| view.name),
        }
    }

    /// Recomputes what the view shows and schedules background work.
    fn sync(&mut self) {
        let Some(draft) = self.core.draft().cloned() else {
            return;
        };
        // The selected display must still be listed and Separate.
        if let Target::Output(name) = &self.target
            && (draft.displays() != Setting::Known(Displays::Separate)
                || !draft.display_views().iter().any(|view| view.name == *name))
        {
            self.target = Target::Shared;
        }
        self.prune_typed();
        if self.tiles.is_empty() {
            self.build_tiles();
            if let Some(path) = self.reveal.take()
                && let Some(index) = self.tiles.iter().position(|tile| {
                    matches!(&tile.pick, Pick::Image(known) | Pick::Folder(known) if *known == path)
                })
            {
                self.page = index / PAGE;
            }
        }
        self.page = self.page.min(self.pages() - 1);
        let effective = draft.effective(&self.target);
        let mut base = draft.clone();
        base.discard();
        self.current = base
            .effective(&self.target)
            .known()
            .map(|entry| entry.source.clone());

        // Folders: the shown source's scan first, then every edited one,
        // then the page's tiles.
        let mut folders: Vec<PathBuf> = Vec::new();
        let mut edited_folders: Vec<PathBuf> = Vec::new();
        let source = effective.known().map(|entry| entry.source.clone());
        let source_path = match &source {
            Some(BgSource::Path(path)) => Some(path.clone()),
            _ => None,
        };
        let source_folder = source_path.as_ref().is_some_and(|path| path.is_dir());
        if source_folder && let Some(path) = &source_path {
            folders.push(path.clone());
        }
        let edited = draft.edited_paths();
        for path in &edited {
            if path.is_dir() {
                edited_folders.push(path.clone());
                folders.push(path.clone());
            }
        }
        folders.extend(
            self.page_tiles()
                .iter()
                .filter_map(|tile| match &tile.pick {
                    Pick::Folder(path) => Some(path.clone()),
                    _ => None,
                }),
        );

        // Source checks of newly shown paths: files are stat'ed, folders
        // use their (background) scan.
        self.problem = None;
        for path in &edited {
            let problem = if path.to_str().is_none() {
                Some(media::SourceCheck::NotText.problem())
            } else if path.is_file() {
                None
            } else if edited_folders.contains(path) {
                match self.scans.get(path) {
                    None => Some(Some("Checking the folder…".to_string())),
                    Some(Err(error)) => Some(Some(error.clone())),
                    Some(Ok(scan)) if scan.images > 0 => None,
                    Some(Ok(scan)) if scan.truncated => {
                        Some(Some("This folder is too large to check".to_string()))
                    }
                    Some(Ok(_)) => Some(media::SourceCheck::Empty.problem()),
                }
            } else if path.exists() {
                Some(Some("Not a picture or folder".to_string()))
            } else {
                Some(media::SourceCheck::Missing.problem())
            };
            if let Some(problem) = problem.flatten() {
                self.problem = Some(problem);
                break;
            }
        }

        // The picture the header shows.
        let mut shown = Shown {
            folder: source_folder,
            ..Default::default()
        };
        let mut image: Option<PathBuf> = None;
        let mut bundled: Option<&'static str> = None;
        if let Some(path) = &source_path {
            if source_folder {
                let current = self
                    .store()
                    .zip(self.state_output())
                    .and_then(|(store, output)| media::current_slideshow_image(store, &output))
                    .filter(|current| current.starts_with(path) && current.is_file());
                image = current.or_else(|| match self.scans.get(path) {
                    Some(Ok(scan)) => scan.first.first().cloned(),
                    _ => None,
                });
            } else {
                if let Class::Bundled(name) = wallpaper::classify(path, &self.bundled_dir, &[]) {
                    bundled = Some(name);
                }
                // Analyse the installed picture, including bundled ones.
                // Compiled thumbnails are only a gallery fallback.
                image = Some(path.clone());
                shown.missing = !path.is_file();
                shown.bundled = bundled.is_some();
                shown.warning = shown.missing && !edited.contains(path);
                if shown.missing {
                    // Never a healthy-looking preview of a missing file.
                    bundled = None;
                }
            }
        }
        shown.image = image.clone();
        // A single picture: is it animated? (Checked once per file version.)
        if !source_folder
            && !shown.missing
            && let Some(key) = image.as_deref().and_then(thumb_key)
        {
            match &self.motion {
                Some((known, animated)) if *known == key => shown.animated = *animated,
                _ if self.motion_checking.as_ref() == Some(&key) => {}
                _ => {
                    self.jobs.push(Job::Motion {
                        path: key.0.clone(),
                    });
                    self.motion_checking = Some(key);
                }
            }
        }

        // Thumbnails wanted: the shown picture, then the page's tiles
        // (at most `PAGE + 1`, so all of them stay in the cache).
        let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
        let wanted: Vec<PathBuf> = image
            .iter()
            .cloned()
            .chain(
                self.page_tiles()
                    .iter()
                    .filter_map(|tile| match &tile.pick {
                        Pick::Image(path) if !tile.missing => Some(path.clone()),
                        _ => None,
                    }),
            )
            .filter(|path| previewed(path) && seen.insert(path.clone()))
            .collect();
        self.keys.retain(|path, _| seen.contains(path));
        self.queue.clear();
        for path in wanted {
            let Some(key) = thumb_key(&path) else {
                continue;
            };
            self.keys.insert(path.clone(), key.clone());
            if self.thumbs.get(&key).is_some() {
                self.thumbs.touch(&key);
            } else if !self.decoding.values().any(|known| *known == key) {
                self.queue.push_back(path);
            }
        }
        while self.decoding.len() < DECODES {
            let Some(path) = self.queue.pop_front() else {
                break;
            };
            let Some(key) = self.keys.get(&path).cloned() else {
                continue;
            };
            let id = self.id();
            self.decoding.insert(id, key);
            self.jobs.push(Job::Thumb { id, path });
        }

        // Folder scans, one at a time. The wanted ones are kept (touched),
        // so a result never evicts another wanted one and the queue
        // settles; only a draft over the display limit (blocked anyway)
        // can want more than is kept.
        let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
        folders.retain(|path| seen.insert(path.clone()));
        folders.truncate(SCANS);
        for path in &folders {
            if self.scans.contains_key(path)
                && let Some(index) = self.scan_order.iter().position(|known| known == path)
                && let Some(known) = self.scan_order.remove(index)
            {
                self.scan_order.push_back(known);
            }
        }
        self.scan_queue = folders
            .into_iter()
            .filter(|path| !self.scans.contains_key(path))
            .collect();
        if self.scanning.is_none()
            && let Some(path) = self.scan_queue.pop_front()
        {
            let id = self.id();
            self.scanning = Some((id, path.clone()));
            self.jobs.push(Job::Scan { id, path });
        }

        // The preview image and the colour analysis input.
        let pixels: Option<Pixels> = match &image {
            Some(path) => self
                .keys
                .get(path)
                .and_then(|key| self.thumbs.get(key))
                .and_then(Thumb::pixels)
                .cloned(),
            _ => None,
        };
        let note = match (&effective, &source) {
            (Setting::Unsupported(_), _) => Some("Kept"),
            (_, Some(BgSource::Color(_))) => None,
            _ if shown.missing => Some(media::MISSING),
            _ if pixels.is_some() => None,
            (_, Some(BgSource::Path(_))) => match (&image, bundled) {
                (Some(path), _) if !previewed(path) => Some(media::NO_PREVIEW),
                (Some(path), _) => match self.keys.get(path).and_then(|key| self.thumbs.get(key)) {
                    Some(Thumb {
                        preview: Preview::None(reason),
                        ..
                    }) => Some(*reason),
                    _ => Some("Loading…"),
                },
                (None, None) if source_folder => {
                    match self.scans.get(source_path.as_ref().expect("path")) {
                        Some(_) => Some(media::NO_PREVIEW),
                        None => Some("Loading…"),
                    }
                }
                _ => Some("Loading…"),
            },
            _ => None,
        };
        shown.note = note;
        let key = effective.known().map(|entry| {
            format!(
                "{:?}:{:?}:{:?}:{}",
                entry.source,
                entry.scaling_mode,
                image.as_ref().and_then(|path| self.keys.get(path)),
                pixels.is_some()
            )
        });
        if key != self.preview_key {
            self.preview_key = key;
            let rendered = match (effective.known(), &pixels) {
                (
                    Some(BgEntry {
                        source: BgSource::Color(color),
                        ..
                    }),
                    _,
                ) => Some(media::render_color(color, PREVIEW.0, PREVIEW.1)),
                (Some(entry), Some(pixels)) => Some(media::fit_preview(
                    pixels,
                    &entry.scaling_mode,
                    PREVIEW.0,
                    PREVIEW.1,
                )),
                _ => None,
            };
            self.shown.handle =
                rendered.map(|pixels| Handle::from_rgba(pixels.width, pixels.height, pixels.rgba));
        }
        shown.handle = self.shown.handle.clone();
        self.shown = shown;

        // Version both analysis and preview by the actual shown picture.
        let analysis_key = match source {
            Some(BgSource::Color(color)) => Some(AnalysisKey::Color(color)),
            Some(BgSource::Path(_)) => image
                .as_ref()
                .and_then(|path| self.keys.get(path))
                .cloned()
                .map(AnalysisKey::Picture),
            None => None,
        };
        if self
            .suggestion
            .as_ref()
            .is_some_and(|(known, _)| Some(known) != analysis_key.as_ref())
        {
            self.suggestion = None;
        }
        if self
            .analysis
            .as_ref()
            .is_some_and(|(_, known)| Some(known) != analysis_key.as_ref())
        {
            self.analysis = None;
        }
        if let Some(source) = analysis_key {
            let done = self
                .suggestion
                .as_ref()
                .is_some_and(|(known, _)| *known == source);
            let running = self
                .analysis
                .as_ref()
                .is_some_and(|(_, known)| *known == source);
            let input = match &source {
                AnalysisKey::Color(color) => Some(Input::Color(color.clone())),
                AnalysisKey::Picture(_) => pixels.map(Input::Pixels),
            };
            if !done && !running {
                if self
                    .suggestion
                    .as_ref()
                    .is_some_and(|(known, _)| *known != source)
                {
                    self.suggestion = None;
                }
                if let Some(input) = input {
                    let id = self.id();
                    self.analysis = Some((id, source));
                    self.jobs.push(Job::Analyse { id, input });
                }
            }
        }
        self.sync_automatic();
    }

    fn sync_automatic(&mut self) {
        let Some(store) = self.store() else {
            return;
        };
        let (active, themed) = match &self.colors {
            Some(colors) => (colors.automatic, colors.family.is_some()),
            None => automatic::read_config(store).map_or((false, false), |(_, config)| {
                (
                    config.appearance.wallpaper_colors,
                    config.appearance.family.is_some(),
                )
            }),
        };
        self.automatic_active = active;
        // A theme also needs the wallpaper's polarity (see `polarity`).
        if !active && !themed {
            self.automatic_source = None;
            self.automatic_analysis = None;
            self.automatic_suggestion = None;
            return;
        }
        let Some(draft) = self.draft() else {
            return;
        };
        let source = automatic::source(self.store().expect("open"), draft);
        if self.automatic_source.as_ref() != Some(&source) {
            self.automatic_suggestion = None;
            let id = self.id();
            self.automatic_analysis = Some((id, source.stamp));
            self.automatic_source = Some(source.clone());
            self.jobs.push(Job::Automatic {
                id,
                source: source.clone(),
            });
        }
        // Merely visiting Settings never stages a change. A wallpaper or
        // color draft absorbs the generated accent into its existing Apply.
        if active
            && self.automatic_suggestion.is_some()
            && (self.background_dirty() || self.colors.is_some())
            && self
                .colors
                .as_ref()
                .is_none_or(|colors| colors.stamp != source.stamp)
        {
            if let Err(error) = self.ensure_colors() {
                self.set_notice(Notice::Error(error));
                return;
            }
            let rgb = self
                .automatic_suggestion
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .and_then(|s| self.recommended(s));
            let colors = self.colors.as_mut().expect("loaded");
            if !colors.adjusted
                && let Some(rgb) = rgb
            {
                colors.accent = Some(rgb);
            }
            colors.stamp = source.stamp;
            self.stage_colors();
        }
    }
}

/// Apply waits for the new wallpaper's colors; it is progress, not a failure.
pub(crate) const FINDING_COLORS: &str = "Finding colors for the new wallpaper…";

fn polarity_notice(reason: String) -> Notice {
    if reason == FINDING_COLORS {
        Notice::Busy(reason)
    } else {
        Notice::Error(reason)
    }
}

#[cfg(test)]
mod tests;
