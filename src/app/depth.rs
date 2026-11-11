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
    /// The picture being analysed, by content, and how to stop it.
    analysing: Option<(String, watch::Sender<bool>)>,
    /// Pictures whose analysis failed; not tried again until restart.
    failed: std::collections::HashSet<String>,
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
    // layer area retains fractional scale precision. Permit only that rounding,
    // not reserved panel space whose origin we cannot yet project.
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
        if let Some(sender) = self.cancel.take() {
            sender.send_replace(true);
        }
        self.id = self.id.wrapping_add(1);
        self.key = None;
        self.image = None;
    }
    pub fn invalidate(&mut self) {
        self.cancel();
        self.observed = None;
        self.entry = None;
    }
    pub fn observe(&mut self, id: u64, entry: Option<BgEntry>) {
        trace(format!(
            "observe {id}/{} present={}",
            self.id,
            entry.is_some()
        ));
        if id == self.id {
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
        self.image = rendered
            .pixels
            .map(|p| Handle::from_rgba(p.width, p.height, p.rgba));
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
        let Some(scale) = self.widget_output(Widget::Clock).and_then(|output| {
            let full = output.info.logical_size?;
            let screen = self.widget_screen(Widget::Clock);
            // Layer surfaces use the compositor's available area. Until a
            // verified inset origin is available, reserve-strut displays use
            // the plain clock instead of guessing its wallpaper coordinates.
            let mode = output.info.modes.iter().find(|m| m.current)?;
            let mut pixels = mode.dimensions;
            use cosmic::cctk::sctk::reexports::client::protocol::wl_output::Transform;
            if matches!(
                output.info.transform,
                Transform::_90 | Transform::_270 | Transform::Flipped90 | Transform::Flipped270
            ) {
                pixels = (pixels.1, pixels.0);
            }
            full_output_scale(screen, full, pixels)
        }) else {
            self.depth.cancel();
            return Task::none();
        };
        let frame = crate::depth::Frame {
            screen: self.widget_screen(Widget::Clock),
            origin: self.placed_position(Widget::Clock).map(f32::round),
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
        self.depth.cancel();
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
}
