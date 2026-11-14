//! Per-output rain surfaces, below desktop widgets and with empty input regions.
use super::*;
use crate::desktop::wallpaper::{BgEntry, BgSource, media};
use crate::rain::{Preferences, gpu, lifecycle, weather};
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    destroy_layer_surface, set_size,
};
use std::sync::Arc;
#[derive(Default)]
pub(super) struct State {
    surfaces: Vec<Surface>,
    next: u64,
    native_state: Option<(std::path::PathBuf, Option<crate::watch::FileStamp>)>,
    /// What cosmic-bg's state file said, and which cosmic-bg wrote it.
    native_seen: Option<Seen>,
    pub lifecycle: lifecycle::State,
    /// The scene the surfaces show. When it goes quiet (calm weather, or the
    /// effect turned off) it fades out before the surfaces are removed; a
    /// pause removes them at once.
    shown: Option<(Preferences, Option<weather::Level>)>,
    retiring: Option<u64>,
    /// The clock behind the glass and the output it stands on.
    behind: Option<(crate::displays::WlOutput, Arc<gpu::Behind>)>,
    /// What hides the desktop: covering windows, an idle session, an
    /// output turned off. A hidden output's effect holds still.
    pub desk: crate::rain::desk::Desk,
    /// The surfaces the frame watchdog looks after, shared with its stream.
    watchers: Watchers,
}
/// cosmic-bg's state file and the cosmic-bg process running when it was read.
pub(super) type Seen = (Option<Vec<u8>>, Option<(u32, u64)>);
/// What a rewrite of cosmic-bg's state file means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Receipt {
    /// The same cosmic-bg wrote the same pictures again: its rotation timer
    /// rewrites the file every few minutes even for a single picture.
    Same,
    /// A new cosmic-bg wrote the same pictures: its new Background surfaces
    /// now lie over the effect's, so the effect's are made again on top.
    Restarted,
    /// The pictures changed.
    Changed,
}
pub(super) fn receipt(before: Option<&Seen>, now: &Seen) -> Receipt {
    match before {
        Some((content, _)) if *content != now.0 => Receipt::Changed,
        Some((_, Some(process))) if now.1 == Some(*process) => Receipt::Same,
        Some(_) => Receipt::Restarted,
        None => Receipt::Changed,
    }
}
struct Surface {
    output: crate::displays::Output,
    id: window::Id,
    generation: u64,
    image: Option<Arc<gpu::Scene>>,
    picture: Option<std::path::PathBuf>,
    stamp: Option<crate::watch::FileStamp>,
    cancel: tokio::sync::watch::Sender<bool>,
}
/// A layer surface draws its next frame only when the compositor's frame
/// callback for the last one arrives. A frame that fails to present (a
/// timed out, outdated or lost swapchain image) is never committed, so its
/// callback never comes and the animation stops on that frame until
/// something else reconfigures the surface. The watchdog notices a surface
/// whose animation should be running but has not drawn for `STALL`, and
/// re-arms it with a configure that keeps its size.
///
/// The compositor may also stop the callbacks on purpose, for an output
/// that is off. A configure draws one frame without a callback, so only
/// frames that come at an animation's pace again count as recovery, and
/// after `TRIES` nudges the watchdog waits for the output or the scene to
/// change instead of forcing a frame every few seconds.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct Watchdog {
    /// When the scene was attached, so a surface that never drew counts too.
    since: Option<Instant>,
    nudged: Option<Instant>,
    tries: u32,
    backoff: Duration,
    gave_up: bool,
}
/// What the watchdog decided on one look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verdict {
    Quiet,
    /// Re-arm the surface. `first` is the first nudge of this stall.
    Nudge {
        first: bool,
    },
    /// Nudges did not bring the frames back: stop until something changes.
    GiveUp,
}
/// How long an animating surface may go without drawing.
pub(super) const STALL: Duration = Duration::from_secs(3);
/// How often the watchdog looks while rain animates.
pub(super) const WATCH: Duration = Duration::from_secs(2);
const LONGEST: Duration = Duration::from_secs(60);
/// Nudges for one stall before the watchdog waits for a change.
pub(super) const TRIES: u32 = 3;
impl Watchdog {
    fn attached(&mut self, now: Instant) {
        *self = Self {
            since: Some(now),
            ..Default::default()
        };
    }
    /// What to do now, given how the surface's frames have been coming.
    pub(super) fn check(&mut self, flow: gpu::Flow, now: Instant) -> Verdict {
        let Some(since) = self.since else {
            return Verdict::Quiet;
        };
        if self.nudged.is_some_and(|nudged| flow.flowing_since(nudged)) {
            *self = Self {
                since: Some(since),
                ..Default::default()
            };
        }
        if self.gave_up {
            return Verdict::Quiet;
        }
        let last = flow.last.map_or(since, |drawn| drawn.max(since));
        if now.saturating_duration_since(last) < STALL
            || self
                .nudged
                .is_some_and(|nudged| now.saturating_duration_since(nudged) < self.backoff)
        {
            return Verdict::Quiet;
        }
        if self.tries >= TRIES {
            self.gave_up = true;
            return Verdict::GiveUp;
        }
        let first = self.tries == 0;
        self.tries += 1;
        self.nudged = Some(now);
        self.backoff = (self.backoff * 2).clamp(STALL, LONGEST);
        Verdict::Nudge { first }
    }
}
/// The watched surfaces, shared between the app and the watchdog's stream.
/// The stream looks on its own clock and sends a message only when a
/// surface needs a nudge, so a healthy effect costs the app nothing.
#[derive(Clone, Default)]
pub(super) struct Watchers(Arc<std::sync::Mutex<Watched>>);
#[derive(Default)]
struct Watched {
    /// The display generation the entries were made for: an output change
    /// starts every watchdog over.
    generation: u64,
    entries: Vec<(window::Id, Arc<gpu::Scene>, Watchdog)>,
}
impl std::hash::Hash for Watchers {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (Arc::as_ptr(&self.0) as usize).hash(state);
    }
}
impl Watchers {
    fn lock(&self) -> std::sync::MutexGuard<'_, Watched> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    /// Watch exactly `wanted`. A surface new to the list, or with a new
    /// scene, starts a fresh watchdog.
    fn set(&self, wanted: Vec<(window::Id, Arc<gpu::Scene>)>, generation: u64, now: Instant) {
        let mut watched = self.lock();
        if watched.generation != generation {
            watched.generation = generation;
            watched.entries.clear();
        }
        watched.entries.retain(|(id, scene, _)| {
            wanted
                .iter()
                .any(|(want, fresh)| want == id && Arc::ptr_eq(scene, fresh))
        });
        for (id, scene) in wanted {
            if !watched.entries.iter().any(|(watched, ..)| *watched == id) {
                let mut dog = Watchdog::default();
                dog.attached(now);
                watched.entries.push((id, scene, dog));
            }
        }
    }
    fn is_empty(&self) -> bool {
        self.lock().entries.is_empty()
    }
    /// One look: the surfaces to re-arm now. Logs once per stall.
    pub(super) fn look(&self, now: Instant) -> Vec<window::Id> {
        let mut watched = self.lock();
        let mut nudge = Vec::new();
        for (id, scene, dog) in &mut watched.entries {
            match dog.check(scene.flow(), now) {
                Verdict::Quiet => {}
                Verdict::Nudge { first } => {
                    if first {
                        eprintln!("Rain surface {id:?} stopped drawing; re-arming its frames");
                    }
                    nudge.push(*id);
                }
                Verdict::GiveUp => eprintln!(
                    "Rain surface {id:?} still not drawing after {TRIES} tries; \
                     waiting for its output or scene to change"
                ),
            }
        }
        nudge
    }
}
/// Looks every `WATCH` and yields only the surfaces that need a nudge.
fn watch_stream(
    watchers: &Watchers,
) -> impl cosmic::iced::futures::Stream<Item = Vec<window::Id>> + use<> {
    watch_every(watchers.clone(), WATCH)
}
fn watch_every(
    watchers: Watchers,
    period: Duration,
) -> impl cosmic::iced::futures::Stream<Item = Vec<window::Id>> {
    cosmic::iced::stream::channel(1, async move |mut out| {
        use cosmic::iced::futures::SinkExt;
        loop {
            tokio::time::sleep(period).await;
            let nudge = watchers.look(Instant::now());
            if !nudge.is_empty() && out.send(nudge).await.is_err() {
                return;
            }
        }
    })
}
impl Drop for Surface {
    fn drop(&mut self) {
        self.cancel.send_replace(true);
    }
}
impl State {
    pub fn owns(&self, id: window::Id) -> bool {
        self.surfaces.iter().any(|s| s.id == id)
    }
    pub fn invalidate(&mut self) -> Task<Message> {
        self.shown = None;
        self.retiring = None;
        if let Some((path, stamp)) = &mut self.native_state {
            *stamp = crate::watch::file_stamp(path);
        }
        Task::batch(self.surfaces.drain(..).map(|s| destroy_layer_surface(s.id)))
    }
    /// The fade-out that `generation` waited for has finished.
    pub fn retired(&mut self, generation: u64) -> Task<Message> {
        if self.retiring != Some(generation) {
            return Task::none();
        }
        self.invalidate()
    }
    /// Give every surface a small scene, as if its wallpaper had arrived.
    #[cfg(test)]
    pub fn ready_for_test(&mut self) {
        let surfaces: Vec<_> = self.surfaces.iter().map(|s| (s.id, s.generation)).collect();
        for (id, generation) in surfaces {
            self.ready(
                id,
                generation,
                None,
                Some(media::Pixels {
                    width: 1,
                    height: 1,
                    rgba: vec![0, 0, 0, 255],
                }),
                None,
            );
        }
    }
    /// Whether a scene is on screen and staying there.
    pub fn showing(&self) -> bool {
        self.shown.is_some() && self.retiring.is_none()
    }
    /// Whether the rain surface on `output` has its scene.
    pub fn drawn_on(&self, output: &crate::displays::WlOutput) -> bool {
        self.surfaces
            .iter()
            .any(|s| s.output.handle == *output && s.image.is_some())
    }
    /// Put a picture behind the glass on its output, and nothing behind it
    /// elsewhere.
    pub fn set_behind(&mut self, behind: Option<(crate::displays::WlOutput, Arc<gpu::Behind>)>) {
        for surface in &self.surfaces {
            if let Some(scene) = &surface.image {
                scene.set_behind(
                    behind
                        .as_ref()
                        .filter(|(output, _)| surface.output.handle == *output)
                        .map(|(_, b)| b.clone()),
                );
            }
        }
        self.behind = behind;
    }
    /// Whether the rain surface on `output` shows a picture behind its glass.
    pub fn shows_behind(&self, output: &crate::displays::WlOutput) -> bool {
        self.showing()
            && self.surfaces.iter().any(|s| {
                s.output.handle == *output
                    && s.image
                        .as_ref()
                        .is_some_and(|scene| scene.behind().is_some())
            })
    }
    /// Whether the shown scene should be animating, so its surfaces must
    /// keep drawing.
    pub fn animating(&self) -> bool {
        self.retiring.is_none()
            && self.shown.as_ref().is_some_and(|(p, _)| p.animates())
            && self.surfaces.iter().any(|s| s.image.is_some())
    }
    /// Whether the effect on `surface`'s output holds still because
    /// nothing of the desktop there can be seen.
    fn hidden(&self, surface: &Surface) -> bool {
        surface
            .output
            .name()
            .is_some_and(|name| self.desk.hides(name))
    }
    /// Whether the surface `id` should animate.
    pub fn running(&self, id: window::Id) -> bool {
        self.surfaces
            .iter()
            .find(|s| s.id == id)
            .is_some_and(|s| !self.hidden(s))
    }
    /// Point the watchdog at the surfaces that should be drawing now.
    pub fn watch_sync(&mut self, generation: u64, now: Instant) {
        let wanted = if self.animating() {
            self.surfaces
                .iter()
                .filter(|s| !self.hidden(s))
                .filter_map(|s| Some((s.id, s.image.clone()?)))
                .collect()
        } else {
            Vec::new()
        };
        self.watchers.set(wanted, generation, now);
    }
    /// The watchdog's stream, while it has surfaces to look after.
    pub fn watch(&self) -> Option<Subscription<Vec<window::Id>>> {
        (!self.watchers.is_empty())
            .then(|| Subscription::run_with(self.watchers.clone(), watch_stream))
    }
    /// Re-arm the surfaces the watchdog found stalled.
    pub fn nudge(&self, ids: Vec<window::Id>) -> Task<Message> {
        let tasks: Vec<_> = ids
            .into_iter()
            .filter(|id| self.owns(*id))
            .map(|id| set_size(id, None, None))
            .collect();
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }
    pub fn paths(&self) -> Vec<(std::path::PathBuf, Option<crate::watch::FileStamp>)> {
        self.surfaces
            .iter()
            .filter_map(|s| s.picture.clone().map(|p| (p, s.stamp)))
            .collect()
    }
    /// cosmic-bg's state file, watched apart from the pictures: it is
    /// rewritten without any picture changing.
    pub fn receipt_path(&self) -> Option<(std::path::PathBuf, Option<crate::watch::FileStamp>)> {
        self.native_state.clone()
    }
    pub fn ready(
        &mut self,
        id: window::Id,
        generation: u64,
        entry: Option<BgEntry>,
        pixels: Option<media::Pixels>,
        stamp: Option<crate::watch::FileStamp>,
    ) {
        if let Some(surface) = self
            .surfaces
            .iter_mut()
            .find(|s| s.id == id && s.generation == generation)
        {
            surface.picture = entry.and_then(|e| {
                if let BgSource::Path(p) = e.source {
                    Some(p)
                } else {
                    None
                }
            });
            surface.image = pixels.map(gpu::Scene::new);
            if let (Some(scene), Some((output, behind))) = (&surface.image, &self.behind)
                && surface.output.handle == *output
            {
                scene.set_behind(Some(behind.clone()));
            }
            surface.stamp = stamp;
        }
    }
}
fn target_size(output: &crate::displays::Output) -> [u32; 2] {
    let (w, h) = output.info.logical_size.unwrap_or((1920, 1080));
    let scale = output.info.scale_factor.max(1) as f32;
    let w = (w.max(1) as f32 * scale).min(8192.);
    let h = (h.max(1) as f32 * scale).min(8192.);
    let factor = ((3840. * 2160.) / (w * h)).sqrt().min(1.);
    [
        (w * factor).floor().max(1.) as u32,
        (h * factor).floor().max(1.) as u32,
    ]
}
async fn background(
    output: String,
    size: [u32; 2],
    cancel: tokio::sync::watch::Receiver<bool>,
) -> (
    Option<BgEntry>,
    Option<media::Pixels>,
    Option<crate::watch::FileStamp>,
) {
    let (send, receive) = tokio::sync::oneshot::channel();
    media::worker::prepare_static(move || {
        let entry = crate::depth::observe(&output).and_then(|mut entry| {
            if let BgSource::Path(path) = &entry.source
                && path.is_dir()
            {
                let store = crate::desktop::DesktopStore::from_env().ok()?;
                entry.source = BgSource::Path(media::current_slideshow_image(&store, &output)?);
            }
            Some(entry)
        });
        let _ = send.send(entry);
    })
    .await;
    let entry = receive.await.ok().flatten();
    let stamp = entry.as_ref().and_then(|e| match &e.source {
        BgSource::Path(p) => crate::watch::file_stamp(p),
        _ => None,
    });
    let pixels = if let Some(entry) = entry.clone() {
        media::worker::wallpaper(entry, size, cancel).await
    } else {
        None
    };
    (entry, pixels, stamp)
}
impl Cards {
    /// The effect to show now: the person's choice, or the weather's while
    /// Match the weather is on and the Weather widget has a current reading
    /// for its city. Weather scenes read only that widget's data: without a
    /// visible Weather widget nothing is fetched and the choice plays.
    pub(super) fn effect_scene(&self) -> weather::Scene {
        let rain = &self.config.rain;
        let snapshot = rain
            .weather
            .then(|| self.config.location.as_ref())
            .flatten()
            .filter(|_| self.provider_visible(Widget::Weather))
            .and_then(crate::providers::weather::Key::new)
            .and_then(|key| self.weather_hub.snapshot(&key, Local::now()).data);
        let reading = snapshot
            .as_deref()
            .and_then(weather::Observation::from_snapshot);
        weather::resolve(
            rain,
            reading.as_ref(),
            Local::now(),
            weather::cutoff(self.config.refresh_intervals.duration(Widget::Weather)),
        )
    }
    /// When the weather's reading stops being current, so the person's choice
    /// takes over even if nothing else wakes the app. Anchored to the last
    /// wake, so the deadline stays the same between wakes.
    pub(super) fn effect_wake(&self) -> Option<Instant> {
        let until = self.effect_scene().until?;
        let (then, then_wall) = self.woke_at;
        let ahead = until
            .signed_duration_since(chrono::DateTime::<Local>::from(then_wall))
            .to_std()
            .unwrap_or_default();
        Some(then + ahead + Duration::from_secs(1))
    }
    pub(super) fn rain_view(&self, id: window::Id) -> Option<Element<'_, Message>> {
        let surface = self.rain.surfaces.iter().find(|s| s.id == id)?;
        Some(
            if let Some(scene) = &surface.image
                && let Some((preferences, level)) = &self.rain.shown
            {
                crate::rain::visibility::visible(
                    cosmic::iced::widget::Shader::new(gpu::Program {
                        scene: scene.clone(),
                        preferences: preferences.clone(),
                        running: self.rain.running(id),
                        fixed_time: None,
                        fade: gpu::Fade {
                            enabled: true,
                            hidden: self.rain.retiring.is_some(),
                            level: *level,
                        },
                    })
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into(),
                )
            } else {
                empty_surface()
            },
        )
    }
    /// cosmic-bg rewrote its state file. Only a change of picture reloads
    /// the wallpaper (and the clock cut from it); a new cosmic-bg only makes
    /// the effect's surfaces again; a timed rewrite changes nothing.
    pub(super) fn rain_receipt(&mut self) -> Task<Message> {
        let Some((path, stamp)) = self.rain.native_state.as_mut() else {
            return Task::none();
        };
        *stamp = crate::watch::file_stamp(path);
        let now = (media::read_state(path), media::cosmic_bg_process());
        let before = self.rain.native_seen.replace(now.clone());
        match receipt(before.as_ref(), &now) {
            Receipt::Same => Task::none(),
            Receipt::Restarted => self.rain.invalidate(),
            Receipt::Changed => {
                self.depth.invalidate();
                let rain = self.rain.invalidate();
                Task::batch([rain, self.wallpaper_colors_changed()])
            }
        }
    }
    pub(super) fn rain_sync(&mut self) -> Task<Message> {
        if !self.config.rain.wanted() {
            self.rain.lifecycle = Default::default();
        }
        let scene = self.effect_scene();
        let level = scene.level();
        let p = scene.preferences;
        let paused =
            self.layout_editor.active || p.paused(self.config.material_motion, self.rain.lifecycle);
        if paused || !p.enabled || !p.has_visible_effect() {
            let drawn =
                self.rain.shown.is_some() && self.rain.surfaces.iter().any(|s| s.image.is_some());
            if paused || !drawn {
                return self.rain.invalidate();
            }
            if self.rain.retiring.is_some() {
                return Task::none();
            }
            self.rain.next = self.rain.next.wrapping_add(1);
            let generation = self.rain.next;
            self.rain.retiring = Some(generation);
            return cosmic::task::future(async move {
                tokio::time::sleep(gpu::FADE + Duration::from_millis(250)).await;
                Message::RainRetired(generation)
            });
        }
        self.rain.retiring = None;
        self.rain.shown = Some((p, level));
        if self.rain.native_state.is_none()
            && let Ok(store) = crate::desktop::DesktopStore::from_env()
            && let Some(path) = media::state_path(&store)
        {
            // cosmic-bg rewrites this receipt after recreating its surfaces,
            // including restarts with the same image. Remap above its new
            // Background layer without changing its config or owning its PID.
            self.rain.native_state = Some((path.clone(), crate::watch::file_stamp(&path)));
            self.rain.native_seen = Some((media::read_state(&path), media::cosmic_bg_process()));
        }
        let mut tasks = Vec::new();
        let mut kept = Vec::new();
        for surface in self.rain.surfaces.drain(..) {
            if self
                .displays
                .outputs()
                .iter()
                .any(|o| o.same_geometry(&surface.output))
            {
                kept.push(surface);
            } else {
                tasks.push(destroy_layer_surface(surface.id));
            }
        }
        self.rain.surfaces = kept;
        for output in self.displays.outputs().iter().take(8) {
            if self
                .rain
                .surfaces
                .iter()
                .any(|s| s.output.handle == output.handle)
            {
                continue;
            }
            let Some(name) = output.name().map(str::to_owned) else {
                continue;
            };
            let size = target_size(output);
            self.rain.next = self.rain.next.wrapping_add(1);
            let generation = self.rain.next;
            let id = window::Id::unique();
            let (cancel, receiver) = tokio::sync::watch::channel(false);
            self.rain.surfaces.push(Surface {
                output: output.clone(),
                id,
                generation,
                image: None,
                picture: None,
                stamp: None,
                cancel,
            });
            tasks.push(get_layer_surface(SctkLayerSurfaceSettings {
                id,
                output: IcedOutput::Output(output.handle.clone()),
                layer: Layer::Background,
                keyboard_interactivity: KeyboardInteractivity::None,
                anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                size: Some((None, None)),
                exclusive_zone: -1,
                input_zone: Some(vec![]),
                namespace: "halcyon-rain".into(),
                ..Default::default()
            }));
            tasks.push(cosmic::task::future(async move {
                let (entry, pixels, stamp) = background(name, size, receiver).await;
                Message::RainReady(id, generation, entry, pixels, stamp)
            }));
        }
        Task::batch(tasks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rain_resize_keeps_owned_surfaces_and_rejects_retired_generations() {
        if crate::displays::test_support::isolated(
            "app::rain::tests::rain_resize_keeps_owned_surfaces_and_rejects_retired_generations",
        ) {
            return;
        }
        let (mut app, _) = Cards::init(Core::default(), Config::default());
        let wire = crate::displays::test_support::Wire::new();
        let output = crate::displays::Output {
            handle: wire.output(7),
            info: crate::displays::test_support::info(Some("Rain-test"), (1920, 1080)),
        };
        let (cancel, _) = tokio::sync::watch::channel(false);
        let id = window::Id::unique();
        app.rain.surfaces.push(Surface {
            output,
            id,
            generation: 12,
            image: None,
            picture: None,
            stamp: None,
            cancel,
        });
        let task = app.handle_update(Message::SurfaceResized(
            id,
            cosmic::iced::Size::new(1920., 1080.),
        ));
        assert!(
            cosmic::iced::runtime::task::into_stream(task).is_none(),
            "live rain is not a stale Arrange surface"
        );
        let _ = app.rain.invalidate();
        let task = app.handle_update(Message::SurfaceResized(
            id,
            cosmic::iced::Size::new(1920., 1080.),
        ));
        assert!(
            cosmic::iced::runtime::task::into_stream(task).is_some(),
            "late retired surfaces are still destroyed"
        );
    }
    /// A scene whose frames the test draws by hand.
    fn scene() -> Arc<gpu::Scene> {
        gpu::Scene::new(media::Pixels {
            width: 1,
            height: 1,
            rgba: vec![1, 2, 3, 255],
        })
    }
    /// Frames at 30 a second from `from` for `secs`.
    fn paced(scene: &gpu::Scene, from: Instant, secs: f32) -> Instant {
        let mut at = from;
        while at < from + Duration::from_secs_f32(secs) {
            scene.mark_drawn_at(at);
            at += Duration::from_millis(33);
        }
        at
    }
    #[test]
    fn rain_watchdog_counts_only_paced_frames_as_recovery_and_gives_up() {
        let start = Instant::now();
        let scene = scene();
        let mut dog = Watchdog::default();
        assert_eq!(
            dog.check(scene.flow(), start),
            Verdict::Quiet,
            "not attached"
        );
        dog.attached(start);
        let stopped = paced(&scene, start, 5.);
        assert_eq!(dog.check(scene.flow(), stopped), Verdict::Quiet);
        // The frames stop. Each nudge's configure draws one frame without a
        // frame callback, and then nothing again, as on an output the
        // compositor turned off.
        let mut verdicts = Vec::new();
        let mut now = stopped;
        while now < stopped + Duration::from_secs(3600) {
            now += WATCH;
            let verdict = dog.check(scene.flow(), now);
            if let Verdict::Nudge { .. } = verdict {
                scene.mark_drawn_at(now + Duration::from_millis(20));
            }
            if verdict != Verdict::Quiet {
                verdicts.push((now, verdict, dog.backoff));
            }
        }
        let kinds: Vec<_> = verdicts.iter().map(|(_, v, _)| *v).collect();
        assert_eq!(
            kinds,
            [
                Verdict::Nudge { first: true },
                Verdict::Nudge { first: false },
                Verdict::Nudge { first: false },
                Verdict::GiveUp
            ],
            "{verdicts:?}"
        );
        // The forced frames did not count as recovery: the waits grew.
        let backoffs: Vec<_> = verdicts.iter().take(3).map(|(.., b)| *b).collect();
        assert_eq!(backoffs, [STALL, STALL * 2, STALL * 4]);
        let gaps: Vec<_> = verdicts.windows(2).map(|w| w[1].0 - w[0].0).collect();
        assert!(gaps.windows(2).all(|w| w[1] >= w[0]), "{gaps:?}");
        // Frame callbacks come back by themselves: it is healthy again, and
        // a later stall starts over with the shortest wait.
        let back = paced(&scene, now, 1.);
        assert_eq!(dog.check(scene.flow(), back), Verdict::Quiet);
        assert_eq!((dog.tries, dog.gave_up, dog.nudged), (0, false, None));
        assert_eq!(
            dog.check(scene.flow(), back + STALL + WATCH),
            Verdict::Nudge { first: true }
        );
        assert_eq!(dog.backoff, STALL);
        // After giving up, a new scene or output starts a fresh watchdog.
        let mut tired = Watchdog {
            gave_up: true,
            ..dog
        };
        assert_eq!(
            tired.check(scene.flow(), back + LONGEST * 10),
            Verdict::Quiet
        );
        tired.attached(back + LONGEST * 10);
        assert_eq!(
            tired.check(scene.flow(), back + LONGEST * 10 + STALL),
            Verdict::Nudge { first: true }
        );
    }
    #[test]
    fn rain_flow_needs_frames_in_a_row_at_animation_pace() {
        let start = Instant::now();
        let scene = scene();
        assert!(!scene.flow().flowing_since(start));
        // Lone frames, as configures or app messages force them.
        for second in 1..=20 {
            scene.mark_drawn_at(start + Duration::from_secs(second));
        }
        assert_eq!(scene.flow().streak, 1);
        assert!(!scene.flow().flowing_since(start));
        // The compositor's slow pace for a covered surface is not recovery.
        for n in 0..20 {
            scene.mark_drawn_at(start + Duration::from_secs(30) + Duration::from_millis(995) * n);
        }
        assert!(!scene.flow().flowing_since(start));
        paced(&scene, start + Duration::from_secs(60), 0.3);
        assert!(scene.flow().flowing_since(start + Duration::from_secs(59)));
        assert!(!scene.flow().flowing_since(start + Duration::from_secs(61)));
    }
    /// A rain state with one animating surface on "Rain-test".
    fn watched() -> (State, window::Id, Arc<gpu::Scene>) {
        let wire = crate::displays::test_support::Wire::new();
        let output = crate::displays::Output {
            handle: wire.output(7),
            info: crate::displays::test_support::info(Some("Rain-test"), (1920, 1080)),
        };
        let (cancel, _) = tokio::sync::watch::channel(false);
        let id = window::Id::unique();
        let mut state = State::default();
        state.surfaces.push(Surface {
            output,
            id,
            generation: 3,
            image: None,
            picture: None,
            stamp: None,
            cancel,
        });
        state.ready(
            id,
            3,
            None,
            Some(media::Pixels {
                width: 1,
                height: 1,
                rgba: vec![1, 2, 3, 255],
            }),
            None,
        );
        state.shown = Some((Preferences::default(), None));
        let scene = state.surfaces[0].image.clone().unwrap();
        (state, id, scene)
    }
    #[test]
    fn rain_watch_looks_only_after_shown_surfaces_that_should_draw() {
        let now = Instant::now();
        let (mut state, id, scene) = watched();
        let quiet = |task: Task<Message>| cosmic::iced::runtime::task::into_stream(task).is_none();
        state.watch_sync(1, now);
        assert!(state.watch().is_some() && state.running(id));
        assert_eq!(
            state.watchers.look(now + STALL),
            [id],
            "never drew: re-armed"
        );
        assert!(!quiet(state.nudge(vec![id])));
        assert!(quiet(state.nudge(vec![window::Id::unique()])), "not ours");
        // Windows cover its output: it holds still and nobody watches it.
        state.desk.covered.insert("Rain-test".into());
        state.watch_sync(1, now);
        assert!(!state.running(id) && state.watch().is_none());
        // Uncovered: a fresh watchdog, so the time it held still is no stall.
        state.desk = Default::default();
        let later = now + LONGEST * 5;
        state.watch_sync(1, later);
        assert!(state.running(id));
        paced(&scene, later, 1.);
        assert!(
            state
                .watchers
                .look(later + Duration::from_secs(1))
                .is_empty()
        );
        // Idle, or the output turned off, hold it still too.
        for desk in [
            crate::rain::desk::Desk {
                idle: true,
                ..Default::default()
            },
            crate::rain::desk::Desk {
                off: ["Rain-test".to_owned()].into(),
                ..Default::default()
            },
        ] {
            state.desk = desk;
            state.watch_sync(1, later);
            assert!(!state.running(id) && state.watch().is_none());
        }
        state.desk = Default::default();
        // A display change starts every watchdog over.
        state.watch_sync(1, later);
        let give_up = later + Duration::from_secs(3600);
        for n in 0..4 {
            let _ = state.watchers.look(give_up + LONGEST * n);
        }
        assert!(
            state.watchers.look(give_up + LONGEST * 9).is_empty(),
            "gave up"
        );
        state.watch_sync(2, give_up + LONGEST * 10);
        assert_eq!(state.watchers.look(give_up + LONGEST * 10 + STALL), [id]);
        // A fading scene is allowed to stop.
        state.retiring = Some(9);
        state.watch_sync(2, give_up);
        assert!(state.watch().is_none());
    }
    #[test]
    fn rain_desk_messages_hold_the_effect_still_and_let_it_go() {
        if crate::displays::test_support::isolated(
            "app::rain::tests::rain_desk_messages_hold_the_effect_still_and_let_it_go",
        ) {
            return;
        }
        let (mut app, _) = Cards::init(Core::default(), Config::default());
        let (state, id, _) = watched();
        app.rain = state;
        app.rain.watch_sync(app.display_generation, Instant::now());
        assert!(app.rain.running(id) && app.rain.watch().is_some());
        let covered = crate::rain::desk::Desk {
            covered: ["Rain-test".to_owned()].into(),
            ..Default::default()
        };
        // As `update` does after every message, without the rest of the
        // desktop this test has no outputs for.
        let send = |app: &mut Cards, desk| {
            let _ = app.handle_update(Message::RainDesk(desk));
            app.rain.watch_sync(app.display_generation, Instant::now());
        };
        send(&mut app, covered);
        assert!(!app.rain.running(id), "covered: held still");
        assert!(app.rain.watch().is_none(), "and not watched");
        assert!(app.rain.owns(id), "the surface and its scene stay");
        send(&mut app, Default::default());
        assert!(app.rain.running(id) && app.rain.watch().is_some());
    }
    #[test]
    fn rain_watch_sends_no_messages_while_frames_flow() {
        // The watchdog's own stream, looking every 5 ms instead of every
        // `WATCH`, for 120 looks.
        let (mut state, id, scene) = watched();
        state.watch_sync(1, Instant::now());
        let watchers = state.watchers.clone();
        let drawing = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let painter = {
            let (scene, drawing) = (scene.clone(), drawing.clone());
            std::thread::spawn(move || {
                while drawing.load(std::sync::atomic::Ordering::Relaxed) {
                    scene.mark_drawn_at(Instant::now());
                    std::thread::sleep(Duration::from_millis(16));
                }
            })
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let count = |period: Duration, looks: u32| {
            runtime.block_on(async {
                use cosmic::iced::futures::StreamExt;
                let stream = watch_every(watchers.clone(), period);
                let deadline = tokio::time::sleep(period * looks);
                tokio::pin!(deadline, stream);
                let mut messages = Vec::new();
                loop {
                    tokio::select! {
                        _ = &mut deadline => return messages,
                        Some(m) = stream.next() => messages.push(m),
                    }
                }
            })
        };
        assert!(count(Duration::from_millis(5), 120).is_empty());
        drawing.store(false, std::sync::atomic::Ordering::Relaxed);
        painter.join().unwrap();
        // Stopped for good: messages only for the nudges, then none.
        let stalled = Watchers::default();
        let attached = Instant::now().checked_sub(STALL * 2).unwrap();
        stalled.set(vec![(id, self::scene())], 1, attached);
        drop(scene);
        let messages = runtime.block_on(async {
            use cosmic::iced::futures::StreamExt;
            let stream = watch_every(stalled.clone(), Duration::from_millis(5));
            tokio::pin!(stream);
            let mut messages = Vec::new();
            // Each look advances the watchdog's clock by `WATCH`.
            while let Ok(Some(m)) =
                tokio::time::timeout(Duration::from_millis(200), stream.next()).await
            {
                messages.push(m);
            }
            messages
        });
        assert_eq!(messages.len(), 1, "{messages:?}");
    }
    #[test]
    fn rain_surface_completion_is_fenced_and_removal_cancels_owned_work() {
        let wire = crate::displays::test_support::Wire::new();
        let output = crate::displays::Output {
            handle: wire.output(7),
            info: crate::displays::test_support::info(Some("Rain-test"), (1920, 1080)),
        };
        let (cancel, receiver) = tokio::sync::watch::channel(false);
        let id = window::Id::unique();
        let mut state = State::default();
        state.surfaces.push(Surface {
            output,
            id,
            generation: 12,
            image: None,
            picture: None,
            stamp: None,
            cancel,
        });
        let pixels = || {
            Some(media::Pixels {
                width: 1,
                height: 1,
                rgba: vec![1, 2, 3, 255],
            })
        };
        state.ready(id, 11, None, pixels(), None);
        assert!(state.surfaces[0].image.is_none());
        state.ready(id, 12, None, pixels(), None);
        assert!(state.surfaces[0].image.is_some());
        let _ = state.invalidate();
        assert!(*receiver.borrow());
        assert!(state.surfaces.is_empty());
        state.ready(id, 12, None, pixels(), None);
        assert!(state.surfaces.is_empty());
    }

    use crate::displays::{OutputEvent, test_support::info};
    use crate::providers::weather::{Completion, Key};
    use crate::rain::{Effect, weather::Source};

    /// The Weather widget for one city, with Match the weather on, shown on
    /// one output in an unlocked session.
    fn weather_desktop(choice: Option<Effect>) -> (Cards, crate::displays::test_support::Wire) {
        let mut config = Config {
            enabled_widgets: vec!["weather".into()],
            location: Some(crate::config::Location {
                name: "Weather scene fixture".into(),
                latitude: 45.,
                longitude: 16.,
                timezone: None,
            }),
            ..Default::default()
        };
        config.rain.weather = true;
        if let Some(effect) = choice {
            config.rain.enabled = true;
            config.rain.effect = effect;
        }
        let (mut cards, _) = Cards::init(Core::default(), config);
        let wire = crate::displays::test_support::Wire::new();
        cards.displays.update(
            OutputEvent::Created(Some(info(Some("DP-1"), (1920, 1080)))),
            wire.output(1),
        );
        let _ = cards.reconcile_output();
        let _ = cards.update(Message::RainLifecycle(lifecycle::State {
            ready: true,
            ..Default::default()
        }));
        (cards, wire)
    }
    /// The Weather widget's own request finishing with `code`, observed `at`.
    /// No network: the request is answered here.
    fn deliver(cards: &mut Cards, code: u32, at: chrono::DateTime<Local>) {
        let now = Instant::now();
        cards.weather_hub.due_now(now);
        let _ = cards.refresh_weather(now);
        let location = cards.config.location.clone().unwrap();
        let token = cards
            .weather_hub
            .token_for_test(&Key::new(&location).unwrap())
            .expect("the Weather widget's request");
        let mut weather = crate::app::settings_preview::weather_sample(Local::now().date_naive());
        weather.current.weather_code = code;
        let _ = cards.update(Message::Weather(Completion {
            token,
            result: Some(Ok(crate::data::WeatherSnapshot {
                location,
                updated_at: at.to_rfc3339(),
                weather,
            })),
        }));
    }
    /// Gives each surface its wallpaper, as the background worker would.
    fn draw(cards: &mut Cards) {
        for (id, generation) in cards
            .rain
            .surfaces
            .iter()
            .map(|s| (s.id, s.generation))
            .collect::<Vec<_>>()
        {
            cards.rain.ready(
                id,
                generation,
                None,
                Some(media::Pixels {
                    width: 1,
                    height: 1,
                    rgba: vec![1, 2, 3, 255],
                }),
                None,
            );
        }
    }

    #[test]
    fn weather_scenes_follow_the_weather_widget_and_fall_back_to_the_choice() {
        if crate::displays::test_support::isolated(
            "app::rain::tests::weather_scenes_follow_the_weather_widget_and_fall_back_to_the_choice",
        ) {
            return;
        }
        let (mut cards, _wire) = weather_desktop(Some(Effect::Fog));
        // No reading yet: Moving effect plays.
        assert_eq!(cards.effect_scene().source, Source::NoReading);
        assert_eq!(cards.rain.shown.as_ref().unwrap().0.effect, Effect::Fog);
        assert_eq!(cards.effect_wake(), None);
        deliver(&mut cards, 73, Local::now());
        let scene = cards.effect_scene();
        assert_eq!(
            scene.source,
            Source::Weather {
                code: 73,
                play: Some((Effect::Snow, crate::rain::weather::Level::Moderate))
            }
        );
        let (shown, level) = cards.rain.shown.clone().unwrap();
        assert_eq!(shown.effect, Effect::Snow);
        assert_eq!(shown.snow.density, 39, "70% of the person's 55%");
        assert_eq!(level, Some(crate::rain::weather::Level::Moderate));
        assert_eq!(cards.rain.surfaces.len(), 1, "the same surface fades over");
        // A wake when the reading stops being current, two refreshes on, the
        // same between wakes.
        let wake = cards.effect_wake().unwrap();
        assert_eq!(cards.effect_wake(), Some(wake));
        let cutoff =
            crate::rain::weather::cutoff(cards.config.refresh_intervals.duration(Widget::Weather))
                .to_std()
                .unwrap();
        let ahead = wake.saturating_duration_since(Instant::now());
        assert!(
            ahead > cutoff - Duration::from_secs(60) && ahead <= cutoff + Duration::from_secs(2),
            "{ahead:?}"
        );
        assert!(cards.wake_deadline().is_some_and(|d| d <= wake));
        // Another city: the old city's reading never describes its sky.
        let home = cards.config.clone();
        let mut moved = home.clone();
        moved.location.as_mut().unwrap().name = "Another fixture city".into();
        moved.location.as_mut().unwrap().latitude = -33.;
        let _ = cards.apply_config(moved);
        let _ = cards.update(Message::Tick);
        assert_eq!(cards.effect_scene().source, Source::NoReading);
        assert_eq!(cards.rain.shown.as_ref().unwrap().0.effect, Effect::Fog);
        let _ = cards.apply_config(home);
        // An old reading hands back to Moving effect.
        deliver(&mut cards, 73, Local::now() - chrono::Duration::hours(4));
        assert_eq!(cards.effect_scene().source, Source::Stale);
        assert_eq!(cards.rain.shown.as_ref().unwrap().0.effect, Effect::Fog);
        assert_eq!(cards.effect_wake(), None);
        // Off: Moving effect, whatever the weather.
        deliver(&mut cards, 65, Local::now());
        assert_eq!(cards.rain.shown.as_ref().unwrap().0.effect, Effect::Rain);
        let mut off = cards.config.clone();
        off.rain.weather = false;
        let _ = cards.apply_config(off);
        let _ = cards.update(Message::Tick);
        assert_eq!(cards.effect_scene().source, Source::Manual);
        assert_eq!(cards.rain.shown.as_ref().unwrap().0.effect, Effect::Fog);
        // Without a visible Weather widget there is no reading to match, and
        // nothing is fetched for the effect.
        let mut hidden = cards.config.clone();
        hidden.rain.weather = true;
        hidden.widgets_visible = false;
        let _ = cards.apply_config(hidden);
        let _ = cards.update(Message::Tick);
        assert_eq!(cards.effect_scene().source, Source::NoReading);
        assert!(cards.refresh_weather(Instant::now()).is_empty());
        assert_eq!(cards.rain.shown.as_ref().unwrap().0.effect, Effect::Fog);
    }

    #[test]
    fn calm_weather_fades_out_before_surfaces_go_and_pauses_stop_at_once() {
        if crate::displays::test_support::isolated(
            "app::rain::tests::calm_weather_fades_out_before_surfaces_go_and_pauses_stop_at_once",
        ) {
            return;
        }
        // No effect chosen: only the weather plays one.
        let (mut cards, _wire) = weather_desktop(None);
        assert!(cards.rain.surfaces.is_empty());
        deliver(&mut cards, 63, Local::now());
        assert_eq!(cards.rain.surfaces.len(), 1);
        draw(&mut cards);
        let id = cards.rain.surfaces[0].id;
        // Clear skies: the rain fades out on the same surface, then it goes.
        deliver(&mut cards, 0, Local::now());
        let generation = cards.rain.retiring.expect("a fade-out");
        assert_eq!(cards.rain.surfaces.len(), 1);
        assert!(cards.rain_view(id).is_some());
        let _ = cards.update(Message::Tick);
        assert_eq!(cards.rain.retiring, Some(generation), "one fade-out");
        let _ = cards.update(Message::RainRetired(generation));
        assert!(cards.rain.surfaces.is_empty() && cards.rain.shown.is_none());
        // Rain again before the fade-out ends keeps the surface, and the old
        // fade-out's end changes nothing.
        deliver(&mut cards, 63, Local::now());
        draw(&mut cards);
        let id = cards.rain.surfaces[0].id;
        deliver(&mut cards, 0, Local::now());
        let generation = cards.rain.retiring.unwrap();
        deliver(&mut cards, 61, Local::now());
        assert_eq!(cards.rain.retiring, None);
        let _ = cards.update(Message::RainRetired(generation));
        assert_eq!(cards.rain.surfaces.len(), 1);
        assert_eq!(cards.rain.surfaces[0].id, id);
        assert_eq!(
            cards.rain.shown.as_ref().unwrap().1,
            Some(crate::rain::weather::Level::Light)
        );
        // Lock, sleep and Reduce motion stop a weather scene at once.
        let _ = cards.update(Message::RainLifecycle(lifecycle::State {
            ready: true,
            locked: true,
            ..Default::default()
        }));
        assert!(cards.rain.surfaces.is_empty() && cards.rain.retiring.is_none());
        let _ = cards.update(Message::RainLifecycle(lifecycle::State {
            ready: true,
            ..Default::default()
        }));
        assert_eq!(cards.rain.surfaces.len(), 1);
        draw(&mut cards);
        let mut still = cards.config.clone();
        still.material_motion = false;
        let _ = cards.apply_config(still);
        let _ = cards.update(Message::Tick);
        assert!(cards.rain.surfaces.is_empty() && cards.rain.retiring.is_none());
    }
}
