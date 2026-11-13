//! Runtime ownership for the prepared Clock frame. Observations and raster
//! completions are fenced independently from ordinary widget/provider updates.
//! A picture nobody has analysed yet shows the plain clock; once the scene
//! model is installed, it is analysed once and the clock is drawn again.
use super::*;
use crate::desktop::wallpaper::{BgEntry, BgSource, media::worker};
use cosmic::iced::widget::image::{Handle, Image};
use tokio::sync::watch;

#[derive(Default)]
pub(super) struct State {
    observed: Option<(u64, String)>,
    pub entry: Option<BgEntry>,
    watched_picture: Option<std::path::PathBuf>,
    id: u64,
    key: Option<Vec<u8>>,
    cancel: Option<watch::Sender<bool>>,
    pub image: Option<Handle>,
    /// The same cut clock as pixels, for behind the rain glass.
    pub pixels: Option<std::sync::Arc<crate::desktop::wallpaper::media::Pixels>>,
    /// The picture being analysed, by content, and how to stop it.
    analysing: Option<(String, watch::Sender<bool>)>,
    /// Pictures whose analysis failed; not tried again until restart.
    failed: std::collections::HashSet<String>,
    /// The last request without its time, so a new minute keeps showing the
    /// cut clock until the next one is drawn.
    shape: Option<Vec<u8>>,
    /// The wallpaper is being looked up.
    observing: bool,
}
fn trace(message: impl std::fmt::Display) {
    if std::env::var_os("HALCYON_DEPTH_TRACE").is_some()
        && std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").is_some()
    {
        eprintln!("Depth probe: {message}");
    }
}
fn full_output_scale(screen: [f32; 2], logical: (i32, i32), pixels: (i32, i32)) -> Option<f32> {
    // xdg-output rounds logical dimensions to integers, while the measured
    // layer area retains fractional scale precision. `screen` is the whole
    // output (panel space added back by `clock_spot`); permit only that
    // rounding.
    if (screen[0] - logical.0 as f32).abs() >= 1. || (screen[1] - logical.1 as f32).abs() >= 1. {
        return None;
    }
    [1., 1.5, 2.].into_iter().find(|scale| {
        (pixels.0 as f32 - screen[0] * scale).abs() <= 1.
            && (pixels.1 as f32 - screen[1] * scale).abs() <= 1.
    })
}
impl State {
    fn cancel(&mut self) {
        self.restart(false);
        self.shape = None;
    }
    /// Stops the render in flight. `keep` leaves the clock shown until the
    /// next one arrives, for a request that changed only its time.
    fn restart(&mut self, keep: bool) {
        if let Some(sender) = self.cancel.take() {
            sender.send_replace(true);
        }
        self.id = self.id.wrapping_add(1);
        self.key = None;
        if !keep {
            self.image = None;
            self.pixels = None;
        }
    }
    pub fn invalidate(&mut self) {
        self.cancel();
        self.observed = None;
        self.entry = None;
        self.observing = false;
    }
    /// Whether a cut clock is on its way and none is shown yet.
    pub fn waiting(&self) -> bool {
        self.pixels.is_none() && (self.observing || self.cancel.is_some())
    }
    pub fn observe(&mut self, id: u64, entry: Option<BgEntry>) {
        trace(format!(
            "observe {id}/{} present={}",
            self.id,
            entry.is_some()
        ));
        if id == self.id {
            self.observing = false;
            self.watched_picture = entry.as_ref().and_then(|e| match &e.source {
                BgSource::Path(p) => Some(p.clone()),
                _ => None,
            });
            self.entry = entry;
        }
    }
    /// Shows a finished render. Returns a picture to analyse when it is one
    /// nobody has analysed yet.
    pub fn complete(
        &mut self,
        id: u64,
        rendered: Option<crate::depth::Rendered>,
    ) -> Option<String> {
        trace(format!(
            "ready {id}/{} pixels={}",
            self.id,
            rendered.as_ref().is_some_and(|r| r.pixels.is_some())
        ));
        if id != self.id {
            return None;
        }
        self.cancel = None;
        let rendered = rendered.unwrap_or(crate::depth::Rendered {
            found: crate::depth::Found::Ridge,
            pixels: None,
        });
        self.pixels = rendered.pixels.map(std::sync::Arc::new);
        self.image = self
            .pixels
            .as_ref()
            .map(|p| Handle::from_rgba(p.width, p.height, p.rgba.clone()));
        match rendered.found {
            crate::depth::Found::Unknown { key }
                if !self.failed.contains(&key)
                    && self.analysing.as_ref().is_none_or(|(k, _)| *k != key) =>
            {
                Some(key)
            }
            // In use, so pruning keeps it.
            crate::depth::Found::Judged { key, .. } => {
                if let Some(root) = crate::depth::cache::root() {
                    crate::depth::cache::touch(&root, &key);
                }
                None
            }
            _ => None,
        }
    }
    /// An analysis ended. Returns whether the clock should be drawn again.
    pub fn analysed(&mut self, key: &str, ok: bool) -> bool {
        if self.analysing.as_ref().is_none_or(|(k, _)| k != key) {
            return false;
        }
        self.analysing = None;
        if !ok {
            if self.failed.len() >= 64 {
                self.failed.clear();
            }
            self.failed.insert(key.to_owned());
            return false;
        }
        self.key = None;
        true
    }
    pub fn picture(&self) -> Option<std::path::PathBuf> {
        self.watched_picture.clone()
    }
    /// The picture file the clock is drawn over.
    fn scene_picture(&self) -> Option<std::path::PathBuf> {
        match &self.entry.as_ref()?.source {
            BgSource::Path(path) => Some(path.clone()),
            _ => None,
        }
    }
}
impl Cards {
    pub(super) fn depth_sync(&mut self) -> Task<Message> {
        if !self.output_known
            || self.layout_editor.active
            || !self.config.widget_visible(Widget::Clock)
            || !crate::depth::eligible(&self.config)
            || !self.widget_available(Widget::Clock)
        {
            self.depth.invalidate();
            return Task::none();
        }
        let Some(output) = self
            .widget_output(Widget::Clock)
            .and_then(|o| o.name())
            .map(str::to_owned)
        else {
            self.depth.invalidate();
            return Task::none();
        };
        let observation = (self.display_generation, output.clone());
        if self.depth.observed.as_ref() != Some(&observation) {
            self.depth.invalidate();
            self.depth.observed = Some(observation);
            self.depth.observing = true;
            let id = self.depth.id;
            return cosmic::task::future(async move {
                let (send, receive) = tokio::sync::oneshot::channel();
                worker::prepare_static(move || {
                    let _ = send.send(crate::depth::observe(&output));
                })
                .await;
                Message::DepthObserved(id, receive.await.ok().flatten())
            });
        }
        let Some(entry) = self.depth.entry.clone() else {
            return Task::none();
        };
        // The wallpaper covers the whole output, so the clock is cut where it
        // lies on the whole output: panels that keep space move the area
        // widgets use off its corner.
        let Some((output, full, inset)) = self.clock_spot() else {
            self.depth.cancel();
            return Task::none();
        };
        let Some(scale) = output.info.logical_size.and_then(|logical| {
            let mode = output.info.modes.iter().find(|m| m.current)?;
            let mut pixels = mode.dimensions;
            use cosmic::cctk::sctk::reexports::client::protocol::wl_output::Transform;
            if matches!(
                output.info.transform,
                Transform::_90 | Transform::_270 | Transform::Flipped90 | Transform::Flipped270
            ) {
                pixels = (pixels.1, pixels.0);
            }
            full_output_scale(full, logical, pixels)
        }) else {
            self.depth.cancel();
            return Task::none();
        };
        let placed = self.placed_position(Widget::Clock).map(f32::round);
        let frame = crate::depth::Frame {
            screen: full,
            origin: [inset[0] + placed[0], inset[1] + placed[1]],
            size: self.widget_size(Widget::Clock),
            scale,
        };
        let Some(request) =
            crate::depth::Request::new(&self.config, frame, entry, self.clock_at.naive_local())
                .map(|r| r.with_scenes(crate::depth::cache::root()))
        else {
            self.depth.cancel();
            return Task::none();
        };
        trace(format!("frame {:?}", request.frame));
        let Ok(bytes) = serde_json::to_vec(&request) else {
            return Task::none();
        };
        if self.depth.key.as_ref() == Some(&bytes) {
            return Task::none();
        }
        let shape = serde_json::to_vec(&crate::depth::Request {
            time: String::new(),
            ..request.clone()
        })
        .ok();
        let keep = shape.is_some() && self.depth.shape == shape;
        self.depth.restart(keep);
        self.depth.shape = shape;
        self.depth.key = Some(bytes.clone());
        let id = self.depth.id;
        let (send, cancel) = watch::channel(false);
        self.depth.cancel = Some(send);
        cosmic::task::future(async move {
            Message::DepthReady(id, worker::render_scene(bytes, cancel).await)
        })
    }
    /// Analyses the shown picture, whose content is `key`, when the scene
    /// model is installed. Settings may be analysing it too; the cache's
    /// claim makes one wait for the other.
    pub(super) fn depth_analyse(&mut self, key: String) -> Task<Message> {
        use crate::depth::{analyse, cache, model};
        let source = model::pinned();
        let (Some(path), Some(root), Some(picture)) =
            (model::path(), cache::root(), self.depth.scene_picture())
        else {
            return Task::none();
        };
        if !model::installed(&path, &source) {
            return Task::none();
        }
        if let Some((_, old)) = self.depth.analysing.take() {
            old.send_replace(true);
        }
        let (send, cancel) = watch::channel(false);
        self.depth.analysing = Some((key.clone(), send));
        trace(format!("analyse {key}"));
        cosmic::task::future(async move {
            let ok = analyse::analyse_once(&key, &picture, &path, &source, root, cancel)
                .await
                .is_ok();
            Message::DepthAnalysed(key, ok)
        })
    }
    pub(super) fn depth_view(&self) -> Option<Element<'static, Message>> {
        let handle = self.depth.image.as_ref()?;
        if self.layout_editor.active || self.config.recipes.clock.is_some() {
            return None;
        }
        let size = self.widget_size(Widget::Clock);
        Some(
            Image::new(handle.clone())
                .width(size[0])
                .height(size[1])
                .content_fit(cosmic::iced::ContentFit::Fill)
                .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lit() -> crate::depth::Rendered {
        crate::depth::Rendered {
            found: crate::depth::Found::Ridge,
            pixels: Some(crate::desktop::wallpaper::media::Pixels {
                width: 1,
                height: 1,
                rgba: vec![255; 4],
            }),
        }
    }
    fn unknown(key: &str) -> Option<crate::depth::Rendered> {
        Some(crate::depth::Rendered {
            found: crate::depth::Found::Unknown { key: key.into() },
            pixels: None,
        })
    }
    #[test]
    fn own_wallpaper_desktop_analyses_each_unknown_picture_once() {
        let mut state = State::default();
        state.observe(
            state.id,
            Some(BgEntry::new("all", BgSource::Path("own.png".into()))),
        );
        assert_eq!(state.scene_picture(), Some("own.png".into()));
        // An unknown picture shows the plain clock and asks for analysis.
        assert_eq!(state.complete(state.id, unknown("a")), Some("a".into()));
        assert!(state.image.is_none());
        let (send, cancel) = watch::channel(false);
        state.analysing = Some(("a".into(), send));
        // Not again while it runs.
        assert_eq!(state.complete(state.id, unknown("a")), None);
        // An answer for a picture no longer analysed changes nothing.
        assert!(!state.analysed("b", true));
        // Done: the clock is drawn again from the kept analysis.
        state.key = Some(vec![1]);
        assert!(state.analysed("a", true));
        assert!(state.key.is_none() && state.analysing.is_none());
        assert!(!*cancel.borrow());
        // A failed picture is not tried again.
        let (send, _cancel) = watch::channel(false);
        state.analysing = Some(("c".into(), send));
        assert!(!state.analysed("c", false));
        assert_eq!(state.complete(state.id, unknown("c")), None);
        // Ridge and judged pictures never ask.
        assert_eq!(state.complete(state.id, Some(lit())), None);
        assert!(state.image.is_some());
    }
    #[test]
    fn fb14_arrange_retires_depth_without_changing_clock_input() {
        if crate::displays::test_support::isolated(
            "app::depth::tests::fb14_arrange_retires_depth_without_changing_clock_input",
        ) {
            return;
        }
        let mut config = Config::default();
        config.clock.depth = true;
        config.clock.surface = Some(crate::widget_style::resolver::ClockSurface::None);
        let (mut cards, _) = Cards::init(Core::default(), config);
        let _wire = cards.test_output_ready();
        cards.depth.complete(cards.depth.id, Some(lit()));
        assert!(cards.depth_view().is_some());
        assert!(cards.card_input_rects(Widget::Clock).is_empty());
        cards.layout_editor.active = true;
        assert!(cards.depth_view().is_none());
        let _ = cards.depth_sync();
        assert!(cards.depth.image.is_none());
    }
    #[test]
    fn fb14_fractional_output_rounding_is_allowed_but_panel_insets_are_not() {
        assert_eq!(
            full_output_scale([853.3333, 533.3333], (853, 533), (1280, 800)),
            Some(1.5)
        );
        assert_eq!(
            full_output_scale([640., 400.], (640, 400), (1280, 800)),
            Some(2.)
        );
        assert_eq!(
            full_output_scale([853.3333, 525.3333], (853, 533), (1280, 800)),
            None
        );
        assert_eq!(
            full_output_scale([1024., 640.], (1024, 640), (1280, 800)),
            None
        );
    }
    #[test]
    fn fb14_stale_depth_observations_and_pixels_cannot_restore_cancelled_frames() {
        let mut state = State::default();
        let id = state.id;
        let entry = BgEntry::new("all", BgSource::Path("first.png".into()));
        state.observe(id, Some(entry));
        assert!(state.entry.is_some());
        let (sender, mut receiver) = watch::channel(false);
        state.cancel = Some(sender);
        state.invalidate();
        assert_eq!(
            state.picture(),
            Some("first.png".into()),
            "invalidating a frame must not restart the source subscription"
        );
        assert!(*receiver.borrow_and_update());
        state.observe(
            id,
            Some(BgEntry::new("all", BgSource::Path("stale.png".into()))),
        );
        state.complete(id, Some(lit()));
        assert!(state.entry.is_none() && state.image.is_none());
        state.complete(state.id, Some(lit()));
        assert!(state.image.is_some());
        state.invalidate();
        assert!(state.image.is_none());
    }

    /// The Ridge desktop with a top panel that keeps 40 px, the clock where
    /// the Ridge setup puts it, and `effect` playing.
    fn ridge_desktop(effect: crate::rain::Effect) -> (Cards, crate::displays::test_support::Wire) {
        use crate::displays::{OutputEvent, test_support::info};
        use crate::widget_style::resolver::{ClockFace, ClockSurface, ClockWeight};
        let root =
            std::path::PathBuf::from(std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap());
        let picture = root.join("ridge.png");
        std::fs::write(&picture, crate::depth::PICTURE).unwrap();
        let mut entry = BgEntry::new("all", BgSource::Path(picture));
        entry.scaling_mode = crate::desktop::wallpaper::ScalingMode::Zoom;
        let cosmic = root.join("config/cosmic");
        let write = |namespace: &str, key: &str, value: &str| {
            let file = cosmic.join(namespace).join("v1").join(key);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, value).unwrap();
        };
        write(
            "com.system76.CosmicBackground",
            "all",
            &ron::to_string(&entry).unwrap(),
        );
        write("com.system76.CosmicPanel", "entries", "[\"Panel\"]");
        for (key, value) in [
            ("anchor", "Top"),
            ("exclusive_zone", "true"),
            ("autohide", "Never"),
            ("size", "XS"),
            ("margin", "6"),
            ("output", "All"),
        ] {
            write("com.system76.CosmicPanel.Panel", key, value);
        }
        let mut config = Config::default();
        config.clock.depth = true;
        config.clock.lighting = crate::depth::Lighting::Off;
        config.clock.face = Some(ClockFace::Digital);
        config.clock.surface = Some(ClockSurface::None);
        config.clock.weight = Some(ClockWeight::Bold);
        config.clock.show_date = false;
        config.clock.scale = 200;
        config.enabled_widgets = vec!["clock".into()];
        config.widget_positions.insert("clock".into(), [0.5, 0.2]);
        config.widget_sizes.insert("clock".into(), [800., 330.]);
        config.rain.enabled = true;
        config.rain.effect = effect;
        let (mut cards, _) = Cards::init(Core::default(), config);
        let wire = crate::displays::test_support::Wire::new();
        let mut output = info(Some("DP-1"), (1920, 1080));
        output.modes.push(cosmic::cctk::sctk::output::Mode {
            dimensions: (1920, 1080),
            refresh_rate: 60_000,
            current: true,
            preferred: true,
        });
        cards
            .displays
            .update(OutputEvent::Created(Some(output)), wire.output(1));
        let _ = cards.reconcile_output();
        // The panel keeps the top 40 px, so the area widgets use is shorter.
        cards.screen_size = [1920., 1040.];
        cards.output_known = true;
        let _ = cards.update(Message::RainLifecycle(crate::rain::lifecycle::State {
            ready: true,
            ..Default::default()
        }));
        cards.rain.ready_for_test();
        let _ = cards.update(Message::Preview);
        (cards, wire)
    }
    /// Runs the depth worker's steps in this process, as their messages.
    fn settle(cards: &mut Cards) -> crate::depth::Request {
        if cards.depth.observing {
            let id = cards.depth.id;
            let _ = cards.update(Message::DepthObserved(id, crate::depth::observe("DP-1")));
        }
        let request: crate::depth::Request =
            serde_json::from_slice(cards.depth.key.as_ref().expect("a cut clock is asked for"))
                .unwrap();
        let id = cards.depth.id;
        let rendered = crate::depth::render_scene(&request);
        let _ = cards.update(Message::DepthReady(id, rendered));
        request
    }
    /// Ink of the uncut clock that the cut one hides, and ink both keep.
    fn hidden_and_kept(
        request: &crate::depth::Request,
        cut: &crate::desktop::wallpaper::media::Pixels,
    ) -> (usize, usize) {
        let config = request.style_config().unwrap();
        let time =
            chrono::NaiveDateTime::parse_from_str(&request.time, "%Y-%m-%dT%H:%M:%S").unwrap();
        let plain = crate::depth::draw_plain(&config, &request.frame, time).unwrap();
        assert_eq!((plain.width, plain.height), (cut.width, cut.height));
        let mut hidden = 0;
        let mut kept = 0;
        for (p, c) in plain.rgba.chunks_exact(4).zip(cut.rgba.chunks_exact(4)) {
            if p[3] > 200 {
                if c[3] < 10 {
                    hidden += 1;
                } else if c[3] > 200 {
                    kept += 1;
                }
            }
        }
        (hidden, kept)
    }
    #[test]
    fn depth_clock_stays_behind_the_scene_with_every_effect_and_after_arrange() {
        if crate::displays::test_support::isolated(
            "app::depth::tests::depth_clock_stays_behind_the_scene_with_every_effect_and_after_arrange",
        ) {
            return;
        }
        use crate::rain::Effect;
        for effect in [Effect::Rain, Effect::Snow, Effect::Fog, Effect::Clouds] {
            let (mut cards, _wire) = ridge_desktop(effect);
            assert!(cards.rain.showing(), "{effect:?} plays");
            // The cut clock is on its way: the clock waits on its own
            // surface instead of standing uncut behind the glass.
            assert!(cards.depth.waiting(), "{effect:?}");
            assert!(!cards.clock_behind_glass(), "{effect:?}");
            let request = settle(&mut cards);
            // Cut where it lies on the whole output, below the panel.
            let placed = cards.placed_position(Widget::Clock).map(f32::round);
            assert_eq!(request.frame.screen, [1920., 1080.], "{effect:?}");
            assert_eq!(
                request.frame.origin,
                [placed[0], placed[1] + 40.],
                "{effect:?}"
            );
            let cut = cards.depth.pixels.clone().expect("Ridge keeps depth here");
            // The ridge hides part of the clock and leaves the rest.
            let (hidden, kept) = hidden_and_kept(&request, &cut);
            assert!(
                hidden > 2_000 && kept > 2_000,
                "{effect:?}: {hidden} hidden, {kept} kept"
            );
            // The cut clock, not a plain one, stands behind the glass.
            assert!(cards.clock_behind_glass(), "{effect:?}");
            let (_, behind) = cards.glass.picture.clone().unwrap();
            assert_eq!(behind.pixels, *cut, "{effect:?}");
            assert!(
                (behind.rect[1] - request.frame.origin[1] / 1080.).abs() < 1e-5,
                "{effect:?}: {:?}",
                behind.rect
            );
            // A new minute keeps the cut clock up while the next is drawn.
            cards.clock_at += chrono::Duration::minutes(1);
            let _ = cards.update(Message::Preview);
            assert!(
                cards.depth.pixels.is_some() && cards.clock_behind_glass(),
                "{effect:?}"
            );
            settle(&mut cards);
            assert!(cards.clock_behind_glass(), "{effect:?}");
            // Arrange shows the plain clock on its own surface to drag.
            cards.layout_editor.active = true;
            let _ = cards.update(Message::Preview);
            assert!(!cards.clock_behind_glass(), "{effect:?}");
            assert!(cards.depth_view().is_none() && cards.depth.image.is_none());
            assert!(cards.glass.picture.is_none(), "{effect:?}");
            // Moved and done: depth and glass come back at the new place.
            cards
                .config
                .widget_positions
                .insert("clock".into(), [0.5, 0.3]);
            cards.layout_editor.active = false;
            let _ = cards.update(Message::Preview);
            cards.rain.ready_for_test();
            let _ = cards.update(Message::Preview);
            assert!(
                !cards.clock_behind_glass(),
                "{effect:?}: no uncut clock first"
            );
            let moved = settle(&mut cards);
            assert!(
                moved.frame.origin[1] > request.frame.origin[1],
                "{effect:?}"
            );
            assert!(cards.clock_behind_glass(), "{effect:?}");
            let (_, behind) = cards.glass.picture.clone().unwrap();
            assert_eq!(behind.pixels, **cards.depth.pixels.as_ref().unwrap());
            assert!((behind.rect[1] - moved.frame.origin[1] / 1080.).abs() < 1e-5);
        }
    }
    #[test]
    fn depth_ignores_cosmic_bg_rewriting_the_same_pictures() {
        use super::super::rain::{Receipt, receipt};
        let seen = |text: &str, process| (Some(text.as_bytes().to_vec()), process);
        let before = seen("[(\"DP-1\", Path(\"/a.png\"))]", Some((10, 500)));
        // The rotation timer rewrites the same pictures from the same process.
        assert_eq!(receipt(Some(&before), &before.clone()), Receipt::Same);
        // A restarted cosmic-bg covers the effect: only the effect moves up.
        assert_eq!(
            receipt(
                Some(&before),
                &seen("[(\"DP-1\", Path(\"/a.png\"))]", Some((10, 900)))
            ),
            Receipt::Restarted
        );
        assert_eq!(
            receipt(Some(&before), &seen("[(\"DP-1\", Path(\"/a.png\"))]", None)),
            Receipt::Restarted
        );
        // Another picture is a wallpaper change.
        assert_eq!(
            receipt(
                Some(&before),
                &seen("[(\"DP-1\", Path(\"/b.png\"))]", Some((10, 500)))
            ),
            Receipt::Changed
        );
        assert_eq!(receipt(None, &before), Receipt::Changed);
    }
}
