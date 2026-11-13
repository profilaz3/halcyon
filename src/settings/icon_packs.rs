//! Fonts and icons: installed icon themes as cards with real sample icons,
//! the "Get more icons" browser, the app icon shape and accent folders.
//!
//! The desktop's `icon_theme` is written only through the appearance editor
//! (`system_appearance::Message::IconTheme`), so Undo, history and conflicts
//! work as for the old chooser. A shape or folder tint makes Halcyon's
//! overlay theme the desktop's `icon_theme`; the theme it is built on (the
//! base) is kept in Halcyon's own config, so the chooser still shows the
//! base as the one in use.
use crate::desktop::icon_packs::install::{self, Manifest, Outcome, Progress};
use crate::desktop::icon_packs::lookup::Resolver;
use crate::desktop::icon_packs::ocs::{CatalogPage, Pack, Sort};
use crate::desktop::icon_packs::overlay::{self, Shape, Style};
use crate::desktop::icon_packs::source::Source;
use crate::desktop::icon_packs::{Paths, archive};
use cosmic::widget::image::Handle;
use std::collections::HashMap;
use std::path::PathBuf;

/// The sample icons on a theme card, each with fallbacks: the first name
/// the theme has is drawn.
pub const SAMPLES: [&[&str]; 8] = [
    &["folder", "inode-directory"],
    &["user-home", "folder-home"],
    &[
        "utilities-terminal",
        "org.gnome.Terminal",
        "com.system76.CosmicTerm",
        "terminal",
    ],
    &[
        "web-browser",
        "firefox",
        "org.mozilla.firefox",
        "internet-web-browser",
    ],
    &[
        "system-file-manager",
        "org.gnome.Nautilus",
        "com.system76.CosmicFiles",
        "file-manager",
    ],
    &[
        "preferences-system",
        "org.gnome.Settings",
        "com.system76.CosmicSettings",
        "preferences-desktop",
    ],
    &[
        "accessories-text-editor",
        "text-editor",
        "org.gnome.TextEditor",
        "com.system76.CosmicEdit",
    ],
    &["image-viewer", "org.gnome.Loupe", "eog", "image-x-generic"],
];

/// App icons for the shape preview when the dock has few pins.
const PREVIEW_APPS: [&str; 12] = [
    "firefox",
    "org.mozilla.firefox",
    "com.system76.CosmicFiles",
    "org.gnome.Nautilus",
    "com.system76.CosmicTerm",
    "org.gnome.Terminal",
    "com.system76.CosmicSettings",
    "org.gnome.Settings",
    "com.system76.CosmicEdit",
    "com.system76.CosmicStore",
    "thunderbird",
    "org.gnome.Calendar",
];

/// Generic names to fall back on for the preview row.
const PREVIEW_FALLBACK: [&str; 6] = [
    "web-browser",
    "system-file-manager",
    "utilities-terminal",
    "preferences-system",
    "accessories-text-editor",
    "image-viewer",
];

/// How many app icons the shape preview shows.
pub const PREVIEW_COUNT: usize = 7;
/// Rendered size of the preview icons (drawn at half for sharp edges).
const PREVIEW_SIZE: u32 = 112;

/// What a scan of the installed themes found.
#[derive(Clone, Debug, Default)]
pub struct Scan {
    /// Sample icon files per theme id, in [`SAMPLES`] order.
    pub samples: HashMap<String, Vec<Option<PathBuf>>>,
    /// App icon sources for the shape preview, from the base theme.
    pub apps: Vec<PathBuf>,
    /// The base theme's folder icon, for the tint preview.
    pub folder: Option<PathBuf>,
    /// App icon names for the overlay: dock pins first, then installed apps.
    pub names: Vec<String>,
}

/// Pictures for the shape picker and the folder toggle.
#[derive(Clone, Debug, Default)]
pub struct Previews {
    /// One row of app icons per shape.
    pub shapes: Vec<(Shape, Vec<Handle>)>,
    /// The folder as it is, and in the accent.
    pub folder: Option<(Handle, Handle)>,
}

#[derive(Clone, Debug)]
pub enum Message {
    Scanned(u64, Box<Scan>),
    Rendered(u64, Box<Previews>),
    /// Make this installed theme the one in use (as the base when shaped).
    Use(String),
    /// Ask before removing this Halcyon-installed theme; `None` cancels.
    ConfirmRemove(Option<String>),
    Remove(String),
    /// Open or close the pack browser.
    Browse(bool),
    Query(String),
    Search,
    Sort(Sort),
    More,
    Retry,
    Page(u64, Result<CatalogPage, String>),
    Curated(u64, Result<Vec<Pack>, String>),
    Thumb(String, Result<PathBuf, String>),
    Install(u64),
    Progress(u64, Progress),
    Installed(u64, Result<Outcome, String>),
    /// Use the first theme a just-installed pack brought.
    Apply(u64),
    Shape(Shape),
    Tint(bool),
    Building(u64, usize, usize),
    Built(u64, Result<String, String>),
    Dismiss,
}

impl Message {
    /// Whether the message can change the desktop or Halcyon's icon files.
    pub fn writes(&self) -> bool {
        matches!(
            self,
            Self::Use(_)
                | Self::Remove(_)
                | Self::Install(_)
                | Self::Apply(_)
                | Self::Shape(_)
                | Self::Tint(_)
                | Self::Built(..)
                | Self::Installed(..)
        )
    }
}

/// A pack's install, by pack id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Install {
    /// Asking the catalog for a fresh download link.
    Preparing,
    Downloading(Progress),
    /// Unpacking and checking the archive.
    Unpacking,
    /// Done: the themes it installed.
    Done(Vec<String>),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Thumb {
    Loading,
    Ready(PathBuf),
    Failed,
}

/// A list the catalog is filling.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Load {
    #[default]
    Loading,
    Ready,
    Failed(String),
}

#[derive(Clone, Debug, Default)]
pub struct Browser {
    /// What is typed in the search field.
    pub query: String,
    /// The search the list shows.
    pub search: String,
    pub sort: Sort,
    pub packs: Vec<Pack>,
    pub total: u32,
    pub page: u32,
    pub state: Load,
    /// Loading more is under way.
    pub more: bool,
    pub curated: Vec<Pack>,
    pub curated_state: Load,
    request: u64,
}

impl Browser {
    pub fn has_more(&self) -> bool {
        self.state == Load::Ready
            && (self.page + 1) * crate::desktop::icon_packs::ocs::PAGE_SIZE < self.total
    }
}

/// The overlay build in progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Build {
    pub done: usize,
    pub total: usize,
}

#[derive(Clone, Debug)]
pub enum Notice {
    Done(String),
    Error(String),
}

#[derive(Debug, Default)]
pub struct IconPacks {
    pub paths: Option<Paths>,
    pub manifest: Manifest,
    /// Halcyon's saved record: base, shape and tint.
    pub style: Style,
    pub scan: Scan,
    /// The scan's base theme, so a new base scans again.
    scanned_base: Option<String>,
    pub previews: Previews,
    /// What the previews were drawn for: base, shade and accent.
    rendered: Option<(String, bool, [u8; 3])>,
    pub browser: Option<Browser>,
    pub thumbs: HashMap<String, Thumb>,
    pub installs: HashMap<u64, Install>,
    pub build: Option<Build>,
    pub confirm: Option<String>,
    /// A built overlay to apply when the page opens again.
    pending: Option<String>,
    pub notice: Option<Notice>,
    /// The accent and shade the overlay was last checked against.
    seen: Option<([u8; 3], bool)>,
    source: Option<Source>,
    /// Counters that tell a stale scan, drawing or build from the latest.
    scans: u64,
    renders: u64,
    builds: u64,
}

/// The desktop facts the icon page works from.
#[derive(Clone, Debug)]
pub struct Context {
    /// The desktop's `icon_theme`.
    pub icon_theme: Option<String>,
    pub accent: [u8; 3],
    pub dark: bool,
    /// The desktop store's fixture root, if any.
    pub fixture: Option<PathBuf>,
    /// The desktop store, for the dock's pins.
    pub store: Option<crate::desktop::store::DesktopStore>,
    /// Ids of the installed themes, from the appearance catalog.
    pub themes: Vec<String>,
}

/// What [`IconPacks::update`] asks of the settings app.
#[derive(Debug, Default)]
pub struct Outcome2 {
    pub task: Option<cosmic::iced::Task<Message>>,
    /// Set the desktop's `icon_theme` to this theme.
    pub icon_theme: Option<String>,
    /// The installed themes changed: list them again.
    pub rescan: bool,
}

impl Outcome2 {
    fn task(task: cosmic::iced::Task<Message>) -> Self {
        Self {
            task: Some(task),
            ..Default::default()
        }
    }
}

fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(T) -> Message + Send + 'static,
) -> cosmic::iced::Task<Message> {
    cosmic::iced::Task::perform(
        async move { tokio::task::spawn_blocking(work).await.ok() },
        move |value| match value {
            Some(value) => done(value),
            None => Message::Dismiss,
        },
    )
}

fn handle(picture: image::RgbaImage) -> Handle {
    let (width, height) = picture.dimensions();
    Handle::from_rgba(width, height, picture.into_raw())
}

impl IconPacks {
    fn source(&self) -> Source {
        self.source.clone().unwrap_or_else(Source::from_env)
    }

    /// Reads the record and manifest; returns the scan of installed themes.
    pub fn open(&mut self, context: &Context) -> cosmic::iced::Task<Message> {
        self.paths = Paths::from_env(context.fixture.as_deref());
        if self.source.is_none() {
            self.source = Some(Source::from_env());
        }
        let Some(paths) = &self.paths else {
            return cosmic::iced::Task::none();
        };
        self.manifest = Manifest::load(paths);
        self.style = Style::load(paths);
        self.seen = Some((context.accent, context.dark));
        self.scan(context)
    }

    /// The style the desktop has now. An overlay's own record wins over the
    /// saved style: a look's Apply or Undo may have changed the overlay
    /// since the style was saved.
    pub fn effective(&self, context: &Context) -> Style {
        match &context.icon_theme {
            Some(icon_theme) => {
                overlay::desktop_style(self.paths.as_ref(), icon_theme, &self.style)
            }
            None => self.style.clone(),
        }
    }

    /// The theme the chooser shows as the one in use: an overlay shows its
    /// base.
    pub fn shown(&self, context: &Context) -> Option<String> {
        context
            .icon_theme
            .as_ref()
            .map(|_| self.effective(context).base.unwrap_or_default())
            .filter(|base| !base.is_empty())
    }

    pub fn base(&self, context: &Context) -> String {
        self.effective(context)
            .base
            .unwrap_or_else(|| "Cosmic".to_string())
    }

    /// Looks up every theme's sample icons and the preview sources, off the
    /// UI thread.
    pub fn scan(&mut self, context: &Context) -> cosmic::iced::Task<Message> {
        let Some(paths) = self.paths.clone() else {
            return cosmic::iced::Task::none();
        };
        self.scans += 1;
        let generation = self.scans;
        let base = self.base(context);
        self.scanned_base = Some(base.clone());
        let themes = context.themes.clone();
        let fixture = context.fixture.clone();
        let store = context.store.clone();
        blocking(
            move || scan(&paths, &themes, &base, fixture.as_deref(), store.as_ref()),
            move |scan| Message::Scanned(generation, Box::new(scan)),
        )
    }

    /// Draws the shape and folder previews for the current base, shade and
    /// accent, unless they are drawn already.
    fn render(&mut self, context: &Context) -> cosmic::iced::Task<Message> {
        let key = (self.base(context), context.dark, context.accent);
        if self.rendered.as_ref() == Some(&key) {
            return cosmic::iced::Task::none();
        }
        self.rendered = Some(key.clone());
        self.renders += 1;
        let generation = self.renders;
        let apps = self.scan.apps.clone();
        let folder = self.scan.folder.clone();
        let (dark, accent) = (context.dark, context.accent);
        blocking(
            move || render(&apps, folder.as_deref(), dark, accent),
            move |previews| Message::Rendered(generation, Box::new(previews)),
        )
    }

    /// Checks the overlay against the desktop's accent and shade, and
    /// builds it again when they changed.
    pub fn sync(&mut self, context: &Context) -> Outcome2 {
        // The theme changed elsewhere: its icons are the previews now.
        if self.paths.is_some() && self.scanned_base.as_deref() != Some(&self.base(context)) {
            return Outcome2::task(self.scan(context));
        }
        let now = (context.accent, context.dark);
        if self.seen == Some(now) || self.paths.is_none() {
            return Outcome2::default();
        }
        self.seen = Some(now);
        let mut outcome = Outcome2::default();
        let mut tasks = vec![self.render(context)];
        if let (Some(paths), Some(icon_theme)) = (&self.paths, &context.icon_theme)
            && self.build.is_none()
            && let Some(recorded) = overlay::recorded(paths, icon_theme)
        {
            let tint = recorded.tint.is_some().then_some(context.accent);
            let stale = recorded.tint != tint
                || (recorded.shape != Shape::Original && recorded.dark != context.dark);
            if stale {
                let style = self.effective(context);
                tasks.push(self.start_build(&style, context));
            }
        }
        outcome.task = Some(cosmic::iced::Task::batch(tasks));
        outcome
    }

    fn start_build(&mut self, style: &Style, context: &Context) -> cosmic::iced::Task<Message> {
        let Some(paths) = self.paths.clone() else {
            return cosmic::iced::Task::none();
        };
        self.builds += 1;
        let generation = self.builds;
        self.build = Some(Build { done: 0, total: 0 });
        let request = overlay::Request {
            base: style.base.clone().unwrap_or_else(|| "Cosmic".into()),
            shape: style.shape,
            tint: style.tint.then_some(context.accent),
            dark: context.dark,
            apps: self.scan.names.clone(),
        };
        let current = context.icon_theme.clone();
        cosmic::iced::Task::run(build(paths, request, current), move |event| match event {
            Ok((done, total)) => Message::Building(generation, done, total),
            Err(result) => Message::Built(generation, result),
        })
    }

    /// Saves `style` and makes the desktop follow it: the base alone, or a
    /// new overlay on it.
    fn restyle(&mut self, style: Style, context: &Context) -> Outcome2 {
        let Some(paths) = self.paths.clone() else {
            return Outcome2::default();
        };
        if let Err(error) = style.save(&paths) {
            self.notice = Some(Notice::Error(format!("Can’t save the icon style: {error}")));
            return Outcome2::default();
        }
        self.style = style.clone();
        if style.active() {
            return Outcome2::task(self.start_build(&style, context));
        }
        // Nothing to adjust: the base itself, and no overlay left behind.
        self.builds += 1;
        self.build = None;
        let base = style.base.unwrap_or_else(|| "Cosmic".into());
        Outcome2 {
            icon_theme: (context.icon_theme.as_deref() != Some(base.as_str())).then_some(base),
            ..Default::default()
        }
    }

    /// Called after the desktop's `icon_theme` was written: an overlay slot
    /// no longer in use is removed.
    pub fn applied(&mut self, icon_theme: Option<&str>) {
        if let Some(paths) = &self.paths {
            match icon_theme {
                Some(theme) if overlay::is_overlay(theme) => {
                    overlay::remove_all(paths, Some(theme));
                }
                Some(_) => overlay::remove_all(paths, None),
                None => {}
            }
        }
    }

    pub fn update(&mut self, message: Message, context: &Context) -> Outcome2 {
        match message {
            Message::Scanned(generation, scan) => {
                if generation != self.scans {
                    return Outcome2::default();
                }
                self.scan = *scan;
                self.rendered = None;
                return Outcome2::task(self.render(context));
            }
            Message::Rendered(generation, previews) => {
                if generation == self.renders {
                    self.previews = *previews;
                }
            }
            Message::Use(id) => {
                let mut style = self.effective(context);
                style.base = Some(id.clone());
                self.confirm = None;
                // A choice was made; an earlier warning no longer applies.
                if matches!(self.notice, Some(Notice::Error(_))) {
                    self.notice = None;
                }
                let mut outcome = self.restyle(style, context);
                // The previews follow the new theme, which the desktop only
                // names once the outcome is applied.
                let next = Context {
                    icon_theme: Some(id),
                    ..context.clone()
                };
                outcome.task = Some(cosmic::iced::Task::batch(
                    outcome.task.into_iter().chain([self.scan(&next)]),
                ));
                return outcome;
            }
            Message::Shape(shape) => {
                let mut style = self.effective(context);
                if style.shape == shape && self.build.is_none() {
                    return Outcome2::default();
                }
                style.shape = shape;
                return self.restyle(style, context);
            }
            Message::Tint(on) => {
                let mut style = self.effective(context);
                if style.tint == on && self.build.is_none() {
                    return Outcome2::default();
                }
                style.tint = on;
                return self.restyle(style, context);
            }
            Message::Building(generation, done, total) => {
                if generation == self.builds && self.build.is_some() {
                    self.build = Some(Build { done, total });
                }
            }
            Message::Built(generation, result) => {
                if generation != self.builds {
                    return Outcome2::default();
                }
                self.build = None;
                match result {
                    Ok(slot) => {
                        return Outcome2 {
                            icon_theme: Some(slot),
                            ..Default::default()
                        };
                    }
                    Err(error) => {
                        self.notice =
                            Some(Notice::Error(format!("Can’t adjust the icons: {error}")));
                    }
                }
            }
            Message::ConfirmRemove(id) => {
                if id.is_some() {
                    self.notice = None;
                }
                self.confirm = id;
            }
            Message::Remove(id) => {
                self.confirm = None;
                let Some(paths) = &self.paths else {
                    return Outcome2::default();
                };
                if self.shown(context).as_deref() == Some(id.as_str()) {
                    self.notice = Some(Notice::Error(
                        "Choose another theme before removing the one in use.".into(),
                    ));
                    return Outcome2::default();
                }
                match install::remove(paths, &id) {
                    Ok(()) => {
                        self.manifest = Manifest::load(paths);
                        self.installs.retain(|_, install| {
                            !matches!(install, Install::Done(themes) if themes.contains(&id))
                        });
                        self.notice = Some(Notice::Done(format!("Removed {id}")));
                        return Outcome2 {
                            rescan: true,
                            ..Default::default()
                        };
                    }
                    Err(error) => {
                        self.notice = Some(Notice::Error(format!("Can’t remove {id}: {error}")));
                    }
                }
            }
            Message::Browse(open) => {
                if !open {
                    self.browser = None;
                    return Outcome2::default();
                }
                if self.browser.is_none() {
                    self.browser = Some(Browser::default());
                    return Outcome2::task(cosmic::iced::Task::batch([
                        self.fetch(false),
                        self.fetch_curated(),
                    ]));
                }
            }
            Message::Query(text) => {
                if let Some(browser) = &mut self.browser {
                    browser.query = text.chars().filter(|c| !c.is_control()).take(80).collect();
                    // Clearing the field shows everything again.
                    if browser.query.trim().is_empty() && !browser.search.is_empty() {
                        browser.search.clear();
                        return Outcome2::task(self.fetch(false));
                    }
                }
            }
            Message::Search => {
                if let Some(browser) = &mut self.browser {
                    browser.search = browser.query.trim().to_string();
                    return Outcome2::task(self.fetch(false));
                }
            }
            Message::Sort(sort) => {
                if let Some(browser) = &mut self.browser
                    && browser.sort != sort
                {
                    browser.sort = sort;
                    return Outcome2::task(self.fetch(false));
                }
            }
            Message::More => {
                if self
                    .browser
                    .as_ref()
                    .is_some_and(|b| b.has_more() && !b.more)
                {
                    return Outcome2::task(self.fetch(true));
                }
            }
            Message::Retry => {
                if let Some(browser) = &self.browser {
                    let curated = browser.curated_state != Load::Ready;
                    let mut tasks = vec![self.fetch(false)];
                    if curated {
                        tasks.push(self.fetch_curated());
                    }
                    return Outcome2::task(cosmic::iced::Task::batch(tasks));
                }
            }
            Message::Page(request, result) => {
                let Some(browser) = &mut self.browser else {
                    return Outcome2::default();
                };
                if request != browser.request {
                    return Outcome2::default();
                }
                let append = browser.more;
                browser.more = false;
                match result {
                    Ok(page) => {
                        browser.total = page.total;
                        browser.page = page.page;
                        if append {
                            for pack in page.packs {
                                if !browser.packs.iter().any(|known| known.id == pack.id) {
                                    browser.packs.push(pack);
                                }
                            }
                        } else {
                            browser.packs = page.packs;
                        }
                        browser.state = Load::Ready;
                    }
                    Err(error) if append => {
                        self.notice = Some(Notice::Error(error));
                    }
                    Err(error) => browser.state = Load::Failed(error),
                }
                return Outcome2::task(self.fetch_thumbs());
            }
            Message::Curated(request, result) => {
                let Some(browser) = &mut self.browser else {
                    return Outcome2::default();
                };
                if request != browser.request && browser.curated_state == Load::Ready {
                    return Outcome2::default();
                }
                match result {
                    Ok(packs) => {
                        browser.curated = packs;
                        browser.curated_state = Load::Ready;
                    }
                    Err(error) => browser.curated_state = Load::Failed(error),
                }
                return Outcome2::task(self.fetch_thumbs());
            }
            Message::Thumb(url, result) => {
                self.thumbs.insert(
                    url,
                    match result {
                        Ok(path) => Thumb::Ready(path),
                        Err(_) => Thumb::Failed,
                    },
                );
            }
            Message::Install(id) => return Outcome2::task(self.install(id)),
            Message::Progress(id, progress) => {
                if matches!(
                    self.installs.get(&id),
                    Some(Install::Preparing | Install::Downloading(_))
                ) {
                    let next = if progress.total.is_some_and(|total| progress.done >= total) {
                        Install::Unpacking
                    } else {
                        Install::Downloading(progress)
                    };
                    self.installs.insert(id, next);
                }
            }
            Message::Installed(id, result) => match result {
                Ok(outcome) => {
                    if let Some(paths) = &self.paths {
                        self.manifest = Manifest::load(paths);
                    }
                    self.notice = Some(if outcome.installed.is_empty() {
                        Notice::Error(outcome.message())
                    } else {
                        Notice::Done(outcome.message())
                    });
                    self.installs.insert(
                        id,
                        if outcome.installed.is_empty() {
                            Install::Failed("Nothing new was installed".into())
                        } else {
                            Install::Done(outcome.installed)
                        },
                    );
                    return Outcome2 {
                        rescan: true,
                        ..Default::default()
                    };
                }
                Err(error) => {
                    self.installs.insert(id, Install::Failed(error));
                }
            },
            Message::Apply(id) => {
                if let Some(theme) = self.pack_themes(id).into_iter().next() {
                    return self.update(Message::Use(theme), context);
                }
            }
            Message::Dismiss => self.notice = None,
        }
        Outcome2::default()
    }

    /// Asks for the first page again, or the next one with `more`.
    fn fetch(&mut self, more: bool) -> cosmic::iced::Task<Message> {
        let source = self.source();
        let Some(browser) = &mut self.browser else {
            return cosmic::iced::Task::none();
        };
        browser.request += 1;
        let request = browser.request;
        let page = if more {
            browser.more = true;
            browser.page + 1
        } else {
            browser.state = Load::Loading;
            browser.more = false;
            0
        };
        let (sort, search) = (browser.sort, browser.search.clone());
        cosmic::iced::Task::perform(
            async move { source.page(sort, search, page).await },
            move |result| Message::Page(request, result),
        )
    }

    fn fetch_curated(&mut self) -> cosmic::iced::Task<Message> {
        let source = self.source();
        let Some(browser) = &mut self.browser else {
            return cosmic::iced::Task::none();
        };
        browser.curated_state = Load::Loading;
        let request = browser.request;
        cosmic::iced::Task::perform(async move { source.curated().await }, move |result| {
            Message::Curated(request, result)
        })
    }

    /// Fetches the previews the browser shows and doesn't have yet, four
    /// at a time.
    fn fetch_thumbs(&mut self) -> cosmic::iced::Task<Message> {
        use cosmic::iced::futures::StreamExt;
        let (Some(browser), Some(paths)) = (&self.browser, &self.paths) else {
            return cosmic::iced::Task::none();
        };
        let urls: Vec<String> = browser
            .curated
            .iter()
            .chain(&browser.packs)
            .filter_map(|pack| pack.preview.clone())
            .filter(|url| !self.thumbs.contains_key(url))
            .fold(Vec::new(), |mut urls, url| {
                if !urls.contains(&url) {
                    urls.push(url);
                }
                urls
            });
        if urls.is_empty() {
            return cosmic::iced::Task::none();
        }
        for url in &urls {
            self.thumbs.insert(url.clone(), Thumb::Loading);
        }
        let source = self.source();
        let cache = paths.cache.clone();
        let stream = cosmic::iced::futures::stream::iter(urls)
            .map(move |url| {
                let (source, cache) = (source.clone(), cache.clone());
                async move {
                    let result = source.thumbnail(cache, url.clone()).await;
                    (url, result)
                }
            })
            .buffer_unordered(4);
        cosmic::iced::Task::run(stream, |(url, result)| Message::Thumb(url, result))
    }

    fn pack(&self, id: u64) -> Option<&Pack> {
        let browser = self.browser.as_ref()?;
        browser
            .packs
            .iter()
            .chain(&browser.curated)
            .find(|pack| pack.id == id)
    }

    fn install(&mut self, id: u64) -> cosmic::iced::Task<Message> {
        if matches!(
            self.installs.get(&id),
            Some(Install::Preparing | Install::Downloading(_) | Install::Unpacking)
        ) {
            return cosmic::iced::Task::none();
        }
        let (Some(paths), Some(pack)) = (self.paths.clone(), self.pack(id).cloned()) else {
            return cosmic::iced::Task::none();
        };
        self.installs.insert(id, Install::Preparing);
        let source = self.source();
        cosmic::iced::Task::run(
            install_stream(source, paths, pack),
            move |event| match event {
                Ok(progress) => Message::Progress(id, progress),
                Err(result) => Message::Installed(id, result),
            },
        )
    }

    /// The themes this pack installed that are still there: this run's
    /// install first, then the manifest's record.
    pub fn pack_themes(&self, pack: u64) -> Vec<String> {
        let recorded = self.manifest.of_pack(pack);
        match self.installs.get(&pack) {
            Some(Install::Done(themes)) => themes
                .iter()
                .filter(|theme| recorded.contains(theme))
                .cloned()
                .collect(),
            _ => recorded,
        }
    }

    /// The previews as they would render in the other shade, so screenshots
    /// of a light theme show light tiles.
    #[cfg(test)]
    pub(super) fn previews_in(&self, dark: bool) -> Previews {
        let accent = self
            .rendered
            .as_ref()
            .map_or([0x63, 0xd0, 0xdf], |key| key.2);
        render(&self.scan.apps, self.scan.folder.as_deref(), dark, accent)
    }
}

/// The scan: sample icons for every theme, the preview sources and the
/// app icon names for the overlay.
fn scan(
    paths: &Paths,
    themes: &[String],
    base: &str,
    fixture: Option<&std::path::Path>,
    store: Option<&crate::desktop::store::DesktopStore>,
) -> Scan {
    let mut resolver = Resolver::new(paths.search.clone());
    let mut out = Scan::default();
    for theme in themes {
        if overlay::is_overlay(theme) {
            continue;
        }
        let chain = resolver.chain(theme);
        let samples = SAMPLES
            .iter()
            .map(|names| resolver.find_any(&chain, names, 48))
            .collect();
        out.samples.insert(theme.clone(), samples);
    }
    let (names, pinned) = crate::desktop::icon_packs::app_names(fixture, store);
    let chain = overlay::chain(&mut resolver, base);
    // The preview: pinned apps first, then well-known apps, then generic
    // names, each drawn once.
    let mut preview: Vec<PathBuf> = Vec::new();
    let candidates = names[..pinned]
        .iter()
        .map(String::as_str)
        .chain(PREVIEW_APPS)
        .chain(PREVIEW_FALLBACK)
        .chain(names[pinned..].iter().map(String::as_str));
    for name in candidates {
        if preview.len() >= PREVIEW_COUNT {
            break;
        }
        if let Some((_, path)) = resolver.find(&chain, name, 128)
            && !preview.contains(&path)
        {
            preview.push(path);
        }
    }
    out.apps = preview;
    out.folder = resolver.find_any(&chain, &["folder", "inode-directory"], 128);
    out.names = names;
    out
}

/// The preview pictures for every shape, and the folder.
fn render(
    apps: &[PathBuf],
    folder: Option<&std::path::Path>,
    dark: bool,
    accent: [u8; 3],
) -> Previews {
    use rayon::prelude::*;
    let shapes = Shape::ALL
        .par_iter()
        .map(|shape| {
            let row = apps
                .iter()
                .filter_map(|path| overlay::preview(path, *shape, dark, PREVIEW_SIZE))
                .map(handle)
                .collect();
            (*shape, row)
        })
        .collect();
    let folder = folder.and_then(|path| {
        Some((
            handle(overlay::preview_folder(path, None, PREVIEW_SIZE)?),
            handle(overlay::preview_folder(path, Some(accent), PREVIEW_SIZE)?),
        ))
    });
    Previews { shapes, folder }
}

/// The overlay build as a stream: (done, total) as icons finish, then the
/// result.
fn build(
    paths: Paths,
    request: overlay::Request,
    current: Option<String>,
) -> impl cosmic::iced::futures::Stream<Item = Result<(usize, usize), Result<String, String>>> {
    use cosmic::iced::futures::SinkExt;
    cosmic::iced::stream::channel(8, async move |mut out| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let work = tokio::task::spawn_blocking(move || {
            let step = std::sync::atomic::AtomicUsize::new(0);
            overlay::generate(&paths, &request, current.as_deref(), &|done, total| {
                // About fifty updates, whatever the count.
                let every = (total / 50).max(1);
                if done == total || done >= step.load(std::sync::atomic::Ordering::Relaxed) {
                    step.store(done + every, std::sync::atomic::Ordering::Relaxed);
                    let _ = tx.send((done, total));
                }
            })
        });
        tokio::pin!(work);
        let result = loop {
            tokio::select! {
                result = &mut work => break result.unwrap_or_else(|error| Err(error.to_string())),
                Some(progress) = rx.recv() => {
                    let _ = out.send(Ok(progress)).await;
                }
            }
        };
        let _ = out.send(Err(result)).await;
    })
}

/// A pack install as a stream: download progress, then the outcome.
fn install_stream(
    source: Source,
    paths: Paths,
    pack: Pack,
) -> impl cosmic::iced::futures::Stream<Item = Result<Progress, Result<Outcome, String>>> {
    use cosmic::iced::futures::SinkExt;
    cosmic::iced::stream::channel(8, async move |mut out| {
        let result = async {
            // Download links are signed and expire: ask for fresh ones.
            let fresh = source.pack(pack.id).await.unwrap_or(pack);
            let file = fresh
                .files
                .first()
                .cloned()
                .ok_or("This pack has no download Halcyon can open")?;
            let downloads = paths.cache.join("downloads");
            let archive = downloads.join(format!(
                "{}-{}.download",
                fresh.id,
                crate::desktop::icon_packs::unique()
            ));
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut shown = 0u64;
            let download =
                source.download(&file.link, &archive, archive::DOWNLOAD_LIMIT, move |p| {
                    // At most every 1% or 256 KB.
                    let step = p.total.map_or(256 * 1024, |total| (total / 100).max(1));
                    if p.done == 0 || p.done >= shown + step || p.total == Some(p.done) {
                        shown = p.done;
                        let _ = tx.send(p);
                    }
                });
            tokio::pin!(download);
            let downloaded = loop {
                tokio::select! {
                    result = &mut download => break result,
                    Some(progress) = rx.recv() => {
                        let _ = out.send(Ok(progress)).await;
                    }
                }
            };
            while let Ok(progress) = rx.try_recv() {
                let _ = out.send(Ok(progress)).await;
            }
            if let Err(error) = downloaded {
                let _ = std::fs::remove_file(&archive);
                return Err(error);
            }
            let size = std::fs::metadata(&archive)
                .map(|meta| meta.len())
                .unwrap_or(0);
            let _ = out
                .send(Ok(Progress {
                    done: size,
                    total: Some(size),
                }))
                .await;
            let (id, name) = (fresh.id, fresh.name.clone());
            let target = archive.clone();
            let result = tokio::task::spawn_blocking(move || {
                install::install_archive(&paths, &target, id, &name)
            })
            .await
            .unwrap_or_else(|error| Err(error.to_string()));
            let _ = std::fs::remove_file(&archive);
            result
        }
        .await;
        let _ = out.send(Err(result)).await;
    })
}

#[cfg(test)]
impl IconPacks {
    /// Uses `source` instead of the environment's.
    pub fn with_source(&mut self, source: Source) {
        self.source = Some(source);
    }
}

fn app(message: Message) -> cosmic::Action<super::Message> {
    cosmic::Action::App(super::Message::IconPacks(message))
}

impl super::Settings {
    /// The desktop facts the icon page works from.
    pub(super) fn icon_context(&self) -> Context {
        use crate::desktop::appearance::{ColorRole, ThemeVariant};
        use crate::desktop::panels::Setting;
        let editor = &self.system_appearance;
        let draft = editor.draft();
        let icon_theme = draft.and_then(|draft| match draft.icon_theme() {
            Setting::Known(theme) => Some(theme),
            Setting::Unsupported(_) => None,
        });
        let variant = draft.map(|draft| draft.active_variant());
        let theme = cosmic::theme::active();
        let dark = variant.map_or(theme.cosmic().is_dark, |variant| {
            variant == ThemeVariant::Dark
        });
        let chosen = draft.zip(variant).and_then(|(draft, variant)| {
            match draft.color(variant, ColorRole::Accent) {
                Setting::Known(Some(color)) => Some([color.red, color.green, color.blue]),
                _ => None,
            }
        });
        let accent = chosen.unwrap_or_else(|| {
            let color = theme.cosmic().accent_color();
            [color.red, color.green, color.blue]
        });
        let accent = accent.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
        let store = editor.store();
        Context {
            icon_theme,
            accent,
            dark,
            fixture: store
                .and_then(|store| store.fixture_root())
                .map(std::path::Path::to_path_buf),
            store: store.cloned(),
            themes: editor
                .icons
                .themes
                .iter()
                .map(|theme| theme.id.clone())
                .collect(),
        }
    }

    /// Opens the icon page: reads Halcyon's record and scans the themes.
    pub(super) fn open_icon_packs(&mut self) -> cosmic::app::Task<super::Message> {
        let context = self.icon_context();
        let open = self.icon_packs.open(&context).map(app);
        // An overlay that finished building after the page was left.
        match self.icon_packs.pending.take() {
            Some(theme) => cosmic::Task::batch([open, self.set_icon_theme(theme)]),
            None => open,
        }
    }

    /// Keeps the overlay's folder tint and shading in step with the desktop.
    pub(super) fn sync_icon_packs(&mut self) -> cosmic::app::Task<super::Message> {
        if !self.system_appearance.opened() || self.icon_packs.paths.is_none() {
            return cosmic::Task::none();
        }
        let context = self.icon_context();
        let outcome = self.icon_packs.sync(&context);
        self.finish_icon_packs(outcome)
    }

    pub(super) fn update_icon_packs(
        &mut self,
        message: Message,
    ) -> cosmic::app::Task<super::Message> {
        if !self.system_appearance.opened() {
            return cosmic::Task::none();
        }
        let context = self.icon_context();
        let outcome = self.icon_packs.update(message, &context);
        self.finish_icon_packs(outcome)
    }

    fn finish_icon_packs(&mut self, outcome: Outcome2) -> cosmic::app::Task<super::Message> {
        let mut tasks: Vec<cosmic::app::Task<super::Message>> = Vec::new();
        if let Some(task) = outcome.task {
            tasks.push(task.map(app));
        }
        if outcome.rescan {
            self.system_appearance.rescan_icons();
            let context = self.icon_context();
            tasks.push(self.icon_packs.scan(&context).map(app));
        }
        if let Some(theme) = outcome.icon_theme {
            tasks.push(self.set_icon_theme(theme));
        }
        cosmic::Task::batch(tasks)
    }

    /// Makes `theme` the desktop's icon theme through the appearance
    /// editor, so it is applied, recorded and undoable like any choice.
    fn set_icon_theme(&mut self, theme: String) -> cosmic::app::Task<super::Message> {
        use crate::desktop::appearance::Control;
        use crate::desktop::panels::Setting;
        if !matches!(
            self.page,
            super::Page::DesktopAppearance | super::Page::Themes | super::Page::DesktopWindows
        ) {
            // Applied when the page opens again.
            self.icon_packs.pending = Some(theme);
            return cosmic::Task::none();
        }
        use cosmic::Application;
        let task = self.update(super::Message::SystemAppearance(
            super::system_appearance::Message::IconTheme(theme.clone()),
        ));
        let applied = self.system_appearance.draft().is_some_and(|draft| {
            matches!(draft.icon_theme(), Setting::Known(current) if current == theme)
                && !draft.edited(Control::IconTheme)
        });
        if applied {
            self.icon_packs.applied(Some(&theme));
        }
        task
    }
}
