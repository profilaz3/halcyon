//! One bounded, cancelable Clock specimen for the visible setup comparison.
use super::*;
use crate::depth::model;
use cosmic::widget::image::Handle;
use tokio::sync::watch;

#[derive(Default)]
pub(super) struct Cache {
    key: Option<Vec<u8>>,
    current: bool,
    id: u64,
    cancel: Option<tokio::sync::watch::Sender<bool>>,
    image: Option<Handle>,
}
impl Cache {
    fn clear(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.send_replace(true);
        }
        self.id = self.id.wrapping_add(1);
        self.key = None;
        self.image = None;
    }
    pub fn image(&self, current: bool) -> Option<&Handle> {
        (self.current == current)
            .then_some(self.image.as_ref())
            .flatten()
    }
    pub fn ready(&mut self, id: u64, pixels: Option<crate::desktop::wallpaper::media::Pixels>) {
        if id == self.id {
            self.cancel = None;
            self.image = pixels.map(|p| Handle::from_rgba(p.width, p.height, p.rgba));
        }
    }
}
impl Settings {
    fn depth_preview_request(&self) -> Option<(bool, String, crate::depth::Request)> {
        if self.page != Page::DesktopProfiles || !self.scene_open() {
            return None;
        }
        let current = self.profiles.compare_current;
        let candidate = self.profiles.preview()?;
        let preview = if current {
            view::profiles::comparison_current(candidate, self.profiles.plan())
        } else {
            candidate.clone()
        };
        if preview.bars.iter().any(|bar| {
            bar.listed()
                && bar.autohide().known() != Some(&crate::desktop::panels::AutoHide::Always)
        }) {
            return None;
        }
        let mut config = preview.config.unwrap_or_else(|| self.config.clone());
        if !current
            && self
                .profiles
                .included(crate::desktop::profiles::Domain::Wallpaper)
            && matches!(self.profiles.profile()?.wallpaper.as_ref()?.background.as_ref()?.source.as_ref(),Some(crate::desktop::profiles::SourceLook::Bundled(id)) if id=="halcyon-ridge")
        {
            config.appearance.wallpaper_light = false;
        }
        if !crate::depth::eligible(&config) {
            return None;
        }
        let (stamp, background) = self.profiles.depth_background(current)?;
        // The editor's existing miniature uses a cover image. Fit is only
        // qualified on the actual output until that scene supports letterboxes.
        if background.scaling_mode != crate::desktop::wallpaper::ScalingMode::Zoom {
            return None;
        }
        let layout =
            view::profiles::scene_geometry(self.placement.preview_snapshot().clone(), &config);
        let clock = layout.widgets.iter().find(|w| w.key == "clock")?;
        let frame = crate::depth::Frame {
            screen: layout.screen,
            origin: clock.position.map(f32::round),
            size: clock.size,
            scale: 1.,
        };
        let request = crate::depth::Request::new(
            &config,
            frame,
            background,
            chrono::Local::now().naive_local(),
        )?;
        Some((current, stamp, request))
    }
    pub(super) fn depth_preview_sync(&mut self) -> Task<Message> {
        let Some((current, stamp, request)) = self.depth_preview_request() else {
            self.depth_preview.clear();
            return Task::none();
        };
        let Ok(key) = serde_json::to_vec(&(current, stamp, &request)) else {
            return Task::none();
        };
        if self.depth_preview.key.as_ref() == Some(&key) {
            return Task::none();
        }
        self.depth_preview.clear();
        self.depth_preview.key = Some(key);
        self.depth_preview.current = current;
        let id = self.depth_preview.id;
        let Ok(bytes) = serde_json::to_vec(&request) else {
            return Task::none();
        };
        let (send, cancel) = tokio::sync::watch::channel(false);
        self.depth_preview.cancel = Some(send);
        cosmic::task::future(async move {
            Message::DepthPreviewReady(
                id,
                crate::desktop::wallpaper::media::worker::render_depth(bytes, cancel).await,
            )
        })
    }
    /// The hills only exist in the Ridge wallpaper; elsewhere the switch means nothing.
    pub(super) fn ridge_wallpaper(&self) -> bool {
        matches!(
            self.wallpaper.entry(),
            Some(crate::desktop::panels::Setting::Known(entry)) if matches!(&entry.source,
                crate::desktop::wallpaper::BgSource::Path(path)
                    if path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.contains("ridge")))
        )
    }
    /// Why the scene can’t cover the clock right now (see
    /// [`crate::depth::eligible`]), with the one change that fixes it. The
    /// clock’s own settings come first, then the picture and its analysis.
    pub(super) fn depth_block(&self) -> Option<Block> {
        use crate::widget_style::resolver::{ClockFace, ClockSurface, Surface};
        let config = &self.config;
        if config.recipes.clock.is_some() {
            return Some(Block::fix(
                "Your own design replaces this clock",
                "Open design",
                Message::OpenRecipe(crate::recipes::Kind::Clock),
            ));
        }
        if config.glance.enabled {
            return Some(Block::fix(
                "Daily overview shows the time instead",
                "Turn off Daily overview",
                Message::Widget(Widget::Glance, false),
            ));
        }
        // Easier to read puts a card behind the clock, so it goes before the card.
        if config.appearance.contrast_assist {
            return Some(Block::fix(
                "Off while Easier to read is on",
                "Turn off Easier to read",
                Message::ContrastAssist(false),
            ));
        }
        let style = {
            let _scope = crate::widget_style::scope(config);
            crate::widget_style::presentation(config, config.wallpaper_palette(), true)
        };
        if style.clock_face.value != ClockFace::Digital {
            return Some(Block::fix(
                "Needs the digital type",
                "Use digital",
                self.clock_moment_edit(Message::ClockFace(Some(ClockFace::Digital))),
            ));
        }
        if style.surface.value != Surface::None {
            return Some(Block::fix(
                "Needs no background card",
                "Remove the card",
                self.clock_moment_edit(Message::ClockSurface(Some(ClockSurface::None))),
            ));
        }
        self.scene_block()
    }
    /// The picture’s part of [`Self::depth_block`].
    fn scene_block(&self) -> Option<Block> {
        use crate::depth::{Found, scene::Verdict};
        if !self.picture_wallpaper() {
            return Some(Block::fix(
                "Needs a picture wallpaper",
                "Use Ridge",
                Message::UseRidgeWallpaper,
            ));
        }
        let found = self.clock_stage.found();
        if self.ridge_wallpaper() || matches!(found, Some(Found::Ridge)) {
            return None;
        }
        if let Some(Found::Judged { judgement, .. }) = found {
            return match judgement.verdict {
                Verdict::Works => None,
                Verdict::NeedsNudge => {
                    let now = self.clock_stage_place().map(|p| p.0[1]);
                    let lower = judgement
                        .suggested
                        .zip(now)
                        .is_none_or(|(spot, now)| spot[1] >= now);
                    Some(Block::reason(if lower {
                        "The scene covers the clock a little lower down"
                    } else {
                        "The scene covers the clock a little higher up"
                    }))
                }
                Verdict::NotUseful => Some(Block::fix(
                    judgement
                        .reason
                        .unwrap_or(crate::depth::scene::Reason::NoForeground)
                        .sentence(),
                    "Use Ridge",
                    Message::UseRidgeWallpaper,
                )),
            };
        }
        match self.scenes.model {
            // A failed download says why on the Scene model row, next to
            // its Try again, so it isn't repeated here.
            ModelState::Missing | ModelState::Failed(_) => {
                return Some(Block::reason(
                    "Download the scene model to use this on your own pictures",
                ));
            }
            ModelState::Downloading(_) => {
                return Some(Block::reason("Downloading the scene model…"));
            }
            ModelState::Ready => {}
        }
        let Some(Found::Unknown { key }) = found else {
            return self
                .scenes
                .analysing
                .is_some()
                .then(|| Block::reason("Finding the foreground…"));
        };
        match self.scenes.failed.get(key) {
            Some(failure @ crate::depth::analyse::Failure::Model) => Some(Block::fix(
                failure.sentence(),
                "Download again",
                Message::SceneModel(Scene::Download),
            )),
            Some(failure) => Some(Block::fix(
                failure.sentence(),
                "Try again",
                Message::SceneModel(Scene::Retry),
            )),
            None => Some(Block::reason("Finding the foreground…")),
        }
    }
    /// A picture file, not a color or gradient.
    pub(super) fn picture_wallpaper(&self) -> bool {
        matches!(
            self.wallpaper.entry(),
            Some(crate::desktop::panels::Setting::Known(entry))
                if matches!(entry.source, crate::desktop::wallpaper::BgSource::Path(_))
        )
    }

    /// The scene model’s download, removal and the analyses the Clock stage
    /// asks for.
    pub(super) fn update_scene(&mut self, event: Scene) -> Task<Message> {
        let scenes = &mut self.scenes;
        match event {
            Scene::Download => {
                if matches!(scenes.model, ModelState::Downloading(_)) {
                    return Task::none();
                }
                let Some(path) = model::path() else {
                    scenes.model = ModelState::Failed(model::Error::Disk);
                    return Task::none();
                };
                scenes.generation = scenes.generation.wrapping_add(1);
                let generation = scenes.generation;
                let (send, cancel) = watch::channel(false);
                scenes.download = Some(send);
                scenes.model = ModelState::Downloading(0);
                scenes.failed.clear();
                let source = model::pinned();
                return cosmic::iced::Task::run(download(source, path, cancel), move |event| {
                    cosmic::Action::App(Message::SceneModel(match event {
                        Ok(bytes) => Scene::Progress(generation, bytes),
                        Err(result) => Scene::Downloaded(generation, result),
                    }))
                });
            }
            Scene::Cancel => {
                if let Some(cancel) = scenes.download.take() {
                    cancel.send_replace(true);
                    scenes.generation = scenes.generation.wrapping_add(1);
                    scenes.model = ModelState::Failed(model::Error::Cancelled);
                }
            }
            Scene::Progress(generation, bytes) => {
                if generation == scenes.generation
                    && matches!(scenes.model, ModelState::Downloading(_))
                {
                    scenes.model = ModelState::Downloading(bytes);
                }
            }
            Scene::Downloaded(generation, result) => {
                if generation != scenes.generation {
                    return Task::none();
                }
                scenes.download = None;
                scenes.model = match result {
                    Ok(()) => ModelState::Ready,
                    Err(error) => ModelState::Failed(error),
                };
                return self.scene_analyse();
            }
            Scene::Remove => {
                if let Some(cancel) = scenes.download.take() {
                    cancel.send_replace(true);
                }
                if let Some((_, cancel)) = scenes.analysing.take() {
                    cancel.send_replace(true);
                }
                scenes.generation = scenes.generation.wrapping_add(1);
                scenes.failed.clear();
                if let Some(path) = model::path() {
                    model::remove(&path, crate::depth::cache::root().as_deref());
                }
                scenes.model = ModelState::read();
                // Drawn again: the picture is no longer analysed.
                self.clock_stage.redraw();
            }
            Scene::Retry => {
                if let Some(crate::depth::Found::Unknown { key }) = self.clock_stage.found() {
                    let key = key.clone();
                    self.scenes.failed.remove(&key);
                }
                return self.scene_analyse();
            }
            Scene::Analysed(key, result) => {
                if scenes.analysing.as_ref().is_none_or(|(k, _)| *k != key) {
                    return Task::none();
                }
                scenes.analysing = None;
                match result {
                    Ok(_) => self.clock_stage.redraw(),
                    Err(failure) => {
                        if scenes.failed.len() >= 64 {
                            scenes.failed.clear();
                        }
                        scenes.failed.insert(key, failure);
                    }
                }
            }
        }
        Task::none()
    }
    /// Analyses the stage’s picture when it has not been, the model is
    /// here, and it has not already failed. One analysis at a time.
    pub(super) fn scene_analyse(&mut self) -> Task<Message> {
        use crate::depth::{Found, analyse, cache};
        let Some(Found::Unknown { key }) = self.clock_stage.found() else {
            return Task::none();
        };
        let key = key.clone();
        let source = model::pinned();
        let picture = match self.wallpaper.entry() {
            Some(crate::desktop::panels::Setting::Known(entry)) => match &entry.source {
                crate::desktop::wallpaper::BgSource::Path(path) => Some(path.clone()),
                _ => None,
            },
            _ => None,
        };
        let scenes = &mut self.scenes;
        let (Some(path), Some(root), Some(picture)) = (model::path(), cache::root(), picture)
        else {
            return Task::none();
        };
        if scenes.model != ModelState::Ready
            || scenes.failed.contains_key(&key)
            || scenes.analysing.as_ref().is_some_and(|(k, _)| *k == key)
        {
            return Task::none();
        }
        if !model::installed(&path, &source) {
            scenes.model = ModelState::read();
            return Task::none();
        }
        if let Some((_, old)) = scenes.analysing.take() {
            old.send_replace(true);
        }
        let (send, cancel) = watch::channel(false);
        scenes.analysing = Some((key.clone(), send));
        #[cfg(test)]
        if scenes.hold {
            return Task::none();
        }
        cosmic::task::future(async move {
            let result = analyse::analyse_once(&key, &picture, &path, &source, root, cancel).await;
            Message::SceneModel(Scene::Analysed(key, result))
        })
    }
}

/// Why the effect is off, and the one button that changes it, if the fix
/// is not elsewhere on the page.
pub(super) struct Block {
    pub reason: std::borrow::Cow<'static, str>,
    pub fix: Option<(&'static str, Message)>,
}
impl Block {
    fn reason(reason: impl Into<std::borrow::Cow<'static, str>>) -> Self {
        Self {
            reason: reason.into(),
            fix: None,
        }
    }
    fn fix(reason: &'static str, label: &'static str, message: Message) -> Self {
        Self {
            reason: reason.into(),
            fix: Some((label, message)),
        }
    }
}

/// The scene model on this computer, as Settings shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModelState {
    Missing,
    /// Bytes received so far.
    Downloading(u64),
    Failed(model::Error),
    Ready,
}
impl ModelState {
    fn read() -> Self {
        match model::path() {
            Some(path) if model::installed(&path, &model::pinned()) => Self::Ready,
            _ => Self::Missing,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Scene {
    Download,
    Cancel,
    Remove,
    /// A download's generation and the bytes it has received.
    Progress(u64, u64),
    Downloaded(u64, Result<(), model::Error>),
    /// Analyse the stage’s picture again after it failed.
    Retry,
    Analysed(String, Result<String, crate::depth::analyse::Failure>),
}

pub(super) struct Scenes {
    pub(super) model: ModelState,
    generation: u64,
    download: Option<watch::Sender<bool>>,
    analysing: Option<(String, watch::Sender<bool>)>,
    failed: std::collections::HashMap<String, crate::depth::analyse::Failure>,
    /// Tests only: record an analysis without starting it.
    #[cfg(test)]
    pub(super) hold: bool,
}
impl Default for Scenes {
    fn default() -> Self {
        Self {
            model: ModelState::read(),
            generation: 0,
            download: None,
            analysing: None,
            failed: Default::default(),
            #[cfg(test)]
            hold: false,
        }
    }
}
impl Scenes {
    #[cfg(test)]
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }
    /// Reads the model again when the Clock page opens, unless a download
    /// is under way or its error is still worth showing.
    pub(super) fn refresh(&mut self) {
        match (self.model, ModelState::read()) {
            (ModelState::Downloading(_), _) => {}
            (ModelState::Failed(_), ModelState::Missing) => {}
            (_, now) => self.model = now,
        }
    }
}

/// The download as a stream: progress in bytes, at most every 1%, then the
/// result.
fn download(
    source: model::Source,
    path: std::path::PathBuf,
    cancel: watch::Receiver<bool>,
) -> impl cosmic::iced::futures::Stream<Item = Result<u64, Result<(), model::Error>>> {
    use cosmic::iced::futures::SinkExt;
    cosmic::iced::stream::channel(4, async move |mut out| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let step = (source.size / 100).max(1);
        let mut shown = 0u64;
        let run = model::download(&source, &path, cancel, move |bytes| {
            if bytes >= shown + step {
                shown = bytes;
                let _ = tx.send(bytes);
            }
        });
        tokio::pin!(run);
        let result = loop {
            tokio::select! {
                result = &mut run => break result,
                Some(bytes) = rx.recv() => {
                    let _ = out.send(Ok(bytes)).await;
                }
            }
        };
        let _ = out.send(Err(result)).await;
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fb14_preview_cache_rejects_old_completion_after_navigation() {
        let mut cache = Cache::default();
        let id = cache.id;
        let (sender, receiver) = tokio::sync::watch::channel(false);
        cache.cancel = Some(sender);
        cache.clear();
        assert!(*receiver.borrow());
        cache.ready(
            id,
            Some(crate::desktop::wallpaper::media::Pixels {
                width: 1,
                height: 1,
                rgba: vec![255; 4],
            }),
        );
        assert!(cache.image(false).is_none());
        cache.current = true;
        cache.ready(
            cache.id,
            Some(crate::desktop::wallpaper::media::Pixels {
                width: 1,
                height: 1,
                rgba: vec![255; 4],
            }),
        );
        assert!(cache.image(true).is_some() && cache.image(false).is_none());
    }
}
