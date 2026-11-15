//! How bright the wallpaper is under a clock drawn straight on it. The
//! picture is read and measured off the interface thread, once per
//! wallpaper, output and place; the clock then picks ink that reads there,
//! so light ink does not vanish on a white picture.
use super::*;
use crate::desktop::wallpaper::media::worker;
use tokio::sync::watch;

/// The width the wallpaper is read at to measure it: plenty for a middle.
const READ_WIDTH: f32 = 256.;

#[derive(Default)]
pub(super) struct State {
    /// What was measured last, or is being measured: the display
    /// generation, output and the clock's place on it.
    asked: Option<(u64, String, [i32; 4])>,
    id: u64,
    cancel: Option<watch::Sender<bool>>,
    /// The picture's middle brightness under the clock, once measured.
    pub luminance: Option<f32>,
}
impl State {
    /// Forget the measure, for a new wallpaper or a clock not on one.
    pub fn invalidate(&mut self) {
        if let Some(sender) = self.cancel.take() {
            sender.send_replace(true);
        }
        self.id = self.id.wrapping_add(1);
        self.asked = None;
        self.luminance = None;
    }
    /// A measure arrived. True when it is the one asked for last.
    pub fn complete(&mut self, id: u64, luminance: Option<f32>) -> bool {
        if id != self.id {
            return false;
        }
        self.cancel = None;
        self.luminance = luminance;
        true
    }
}

impl Cards {
    /// Whether the clock is drawn by itself straight on the wallpaper.
    fn clock_on_wallpaper(&self) -> bool {
        if !self.output_known
            || self.layout_editor.active
            || !self.config.widget_visible(Widget::Clock)
            || self.config.recipes.clock.is_some()
        {
            return false;
        }
        let _scope = crate::widget_style::scope(&self.config);
        crate::widget_style::presentation(&self.config, self.config.wallpaper_palette(), true)
            .surface
            .value
            == crate::widget_style::resolver::Surface::None
    }
    /// Measures the wallpaper under the clock when the clock, its place or
    /// the display changed since the last measure.
    pub(super) fn clock_backdrop_sync(&mut self) -> Task<Message> {
        if !self.clock_on_wallpaper() {
            if self.clock_backdrop.asked.is_some() || self.clock_backdrop.luminance.is_some() {
                self.clock_backdrop.invalidate();
            }
            return Task::none();
        }
        let Some((output, full, inset)) = self.clock_spot() else {
            return Task::none();
        };
        let Some(name) = output.name().map(str::to_owned) else {
            return Task::none();
        };
        let placed = self.placed_position(Widget::Clock);
        let size = self.widget_size(Widget::Clock);
        let area = [
            (inset[0] + placed[0]) / full[0],
            (inset[1] + placed[1]) / full[1],
            (inset[0] + placed[0] + size[0]) / full[0],
            (inset[1] + placed[1] + size[1]) / full[1],
        ];
        let asked = (
            self.display_generation,
            name.clone(),
            area.map(|v| (v * 1000.).round() as i32),
        );
        if self.clock_backdrop.asked.as_ref() == Some(&asked) {
            return Task::none();
        }
        if let Some(sender) = self.clock_backdrop.cancel.take() {
            sender.send_replace(true);
        }
        self.clock_backdrop.id = self.clock_backdrop.id.wrapping_add(1);
        self.clock_backdrop.asked = Some(asked);
        let id = self.clock_backdrop.id;
        let (send, cancel) = watch::channel(false);
        self.clock_backdrop.cancel = Some(send);
        let read = [
            READ_WIDTH as u32,
            ((READ_WIDTH * full[1] / full[0].max(1.)).round() as u32).clamp(1, 4096),
        ];
        cosmic::task::future(async move {
            let (send, receive) = tokio::sync::oneshot::channel();
            worker::prepare_static(move || {
                let _ = send.send(crate::depth::observe(&name));
            })
            .await;
            let luminance = match receive.await.ok().flatten() {
                Some(entry) => worker::wallpaper(entry, read, cancel)
                    .await
                    .and_then(|pixels| {
                        clock::backdrop_brightness(
                            &pixels.rgba,
                            [pixels.width, pixels.height],
                            area,
                        )
                    })
                    .map(|[_, middle, _]| middle),
                None => None,
            };
            Message::ClockBackdrop(id, luminance)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A desktop showing `shade` as its whole wallpaper and a bare clock in
    /// the light ink a wallpaper clock has by default.
    fn desktop(shade: u8) -> Cards {
        use crate::desktop::wallpaper::{BgEntry, BgSource};
        use crate::displays::{OutputEvent, test_support::info};
        use crate::widget_style::resolver::{ClockFace, ClockSurface};
        let root =
            std::path::PathBuf::from(std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap());
        let picture = root.join("plain.png");
        image::RgbImage::from_pixel(64, 36, image::Rgb([shade; 3]))
            .save(&picture)
            .unwrap();
        let file = root.join("config/cosmic/com.system76.CosmicBackground/v1/all");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            file,
            ron::to_string(&BgEntry::new("all", BgSource::Path(picture))).unwrap(),
        )
        .unwrap();
        let mut config = Config::default();
        config.clock.face = Some(ClockFace::Digital);
        config.clock.surface = Some(ClockSurface::None);
        config.clock.show_date = true;
        config.clock.scale = 150;
        config.enabled_widgets = vec!["clock".into()];
        config.widget_positions.insert("clock".into(), [0.5, 0.3]);
        config.widget_sizes.insert("clock".into(), [240., 150.]);
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
        cards.screen_size = [1920., 1080.];
        cards.output_known = true;
        std::mem::forget(wire);
        cards
    }

    /// Runs the measure as the desktop does, worker process included.
    fn measure(cards: &mut Cards) {
        use cosmic::iced::futures::StreamExt;
        cards.clock_backdrop.invalidate();
        let task = cards.clock_backdrop_sync();
        assert!(
            cards.clock_backdrop.asked.is_some(),
            "the clock is measured"
        );
        let stream = cosmic::iced::runtime::task::into_stream(task).expect("a measure");
        let messages: Vec<_> = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(stream.collect());
        for action in messages {
            if let cosmic::iced::runtime::Action::Output(cosmic::Action::App(message)) = action {
                let _ = cards.update(message);
            }
        }
    }

    /// The clock drawn on the wallpaper's shade: its strongest ink's
    /// brightness, and the pixels.
    fn drawn(cards: &Cards, shade: u8) -> (f32, Vec<u8>) {
        let size = cards.widget_size(Widget::Clock);
        let mut native = crate::native_test::Native::new(cosmic::iced::Size::new(size[0], size[1]));
        let clock = cards.clock();
        let bare = native.render(clock, &cosmic::Theme::dark());
        // Ink is where the clock covers most, away from its soft edge.
        let ink: Vec<f32> = bare
            .chunks_exact(4)
            .filter(|p| p[3] > 230)
            .map(|p| crate::widget_style::luminance(Color::from_rgb8(p[0], p[1], p[2])))
            .collect();
        assert!(ink.len() > 200, "the clock is drawn: {}", ink.len());
        let back = Color::from_rgb8(shade, shade, shade);
        let on = cosmic::widget::container(cards.clock())
            .width(size[0])
            .height(size[1])
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    background: Some(back.into()),
                    ..Default::default()
                }
            }));
        let shown = native.render(on.into(), &cosmic::Theme::dark());
        (ink.iter().sum::<f32>() / ink.len() as f32, shown)
    }

    /// Light ink on a white wallpaper turns dark once the wallpaper under
    /// the clock is measured, off the interface thread; on a dark wallpaper
    /// the light ink stays, and a clock on a card is left alone.
    #[test]
    fn a_bare_clock_picks_ink_that_reads_on_its_wallpaper() {
        if crate::displays::test_support::isolated(
            "app::clock_backdrop::tests::a_bare_clock_picks_ink_that_reads_on_its_wallpaper",
        ) {
            return;
        }
        let ratio = |a: f32, b: f32| (a.max(b) + 0.05) / (a.min(b) + 0.05);
        let white = crate::widget_style::luminance(Color::from_rgb8(250, 250, 250));
        let mut cards = desktop(250);
        let (before, _) = drawn(&cards, 250);
        assert!(ratio(before, white) < 1.5, "light ink at first: {before}");
        measure(&mut cards);
        let backdrop = cards
            .clock_backdrop
            .luminance
            .expect("the wallpaper is measured");
        assert!((backdrop - white).abs() < 0.02, "{backdrop}");
        let (after, shown) = drawn(&cards, 250);
        assert!(
            ratio(after, white) >= crate::app::clock::READABLE,
            "the clock reads on white: {after} against {white}"
        );
        let size = cards.widget_size(Widget::Clock);
        if let Some(dir) = std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE") {
            let path = std::path::Path::new(&dir).join("minimum-clock-light-150.png");
            image::RgbaImage::from_raw(size[0] as u32, size[1] as u32, shown)
                .unwrap()
                .save(path)
                .unwrap();
        }

        // A dark wallpaper keeps the light ink.
        let mut cards = desktop(12);
        measure(&mut cards);
        assert!(cards.clock_backdrop.luminance.is_some_and(|l| l < 0.02));
        let (dark, _) = drawn(&cards, 12);
        assert!((dark - before).abs() < 0.02, "{dark} against {before}");

        // A clock on a card is not measured, so nothing is asked of the
        // worker.
        let mut cards = desktop(250);
        cards.config.clock.surface = Some(crate::widget_style::resolver::ClockSurface::Panel);
        let _ = cards.clock_backdrop_sync();
        assert!(cards.clock_backdrop.asked.is_none());
    }
}
