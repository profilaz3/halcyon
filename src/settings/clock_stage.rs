//! The Clock page's stage: the screen the clock is on, scaled down, with the
//! clock where it really sits. With the scene on, the clock is the desktop's
//! own masked and lit render for that place, and the stage offers the spot
//! where the picture's foreground would cover it. Dragging it moves the
//! clock and saves through the arrangement's save, one Undo step per move.
use super::*;
use crate::desktop::wallpaper::BgEntry;
use crate::desktop::wallpaper::media::{Pixels, SCENE};
use cosmic::iced::Point;
use cosmic::widget::image::Handle;

/// The widest the stage gets; a wider window only adds margin around it.
pub(super) const MAX_WIDTH: f32 = 600.;
/// The longer side of the wallpaper picture behind the stage.
const BACKDROP: f32 = 1280.;
/// How long the pointer rests mid-drag before the lit clock is drawn again.
const SETTLE: Duration = Duration::from_millis(160);

#[derive(Debug, Clone)]
pub enum Edit {
    /// The pointer over the stage, in stage pixels, and the stage's scale.
    Pointer(Point, f32),
    Press,
    Release,
    Cancel,
    /// An arrow key; it moves the clock only while the stage clock has focus.
    Arrow([i8; 2], bool),
    Nudge([i8; 2], bool),
    Reset,
    /// Moves the clock to where the scene covers it.
    Suggested,
    /// A mid-drag tick: once the pointer rests, the lit clock is drawn again.
    Settle,
    Backdrop(u64, Option<Pixels>),
    Rendered(u64, Option<crate::depth::Rendered>),
}

/// One cancelable background picture: the newest request wins and the
/// shown picture stays until its replacement is ready.
struct Job<T> {
    key: Option<Vec<u8>>,
    id: u64,
    cancel: Option<tokio::sync::watch::Sender<bool>>,
    pending: Option<T>,
    shown: Option<(T, Option<Handle>)>,
}
impl<T> Default for Job<T> {
    fn default() -> Self {
        Self {
            key: None,
            id: 0,
            cancel: None,
            pending: None,
            shown: None,
        }
    }
}
impl<T: Clone> Job<T> {
    /// Starts work for a new key, cancelling the one in flight. `None`
    /// forgets everything.
    fn want(
        &mut self,
        key: Option<Vec<u8>>,
        meta: Option<T>,
    ) -> Option<(u64, tokio::sync::watch::Receiver<bool>)> {
        if self.key == key {
            return None;
        }
        if let Some(cancel) = self.cancel.take() {
            cancel.send_replace(true);
        }
        self.id = self.id.wrapping_add(1);
        self.key = key;
        self.pending = meta;
        if self.key.is_none() || self.pending.is_none() {
            self.key = None;
            self.pending = None;
            self.shown = None;
            return None;
        }
        let (send, cancel) = tokio::sync::watch::channel(false);
        self.cancel = Some(send);
        Some((self.id, cancel))
    }
    /// Shows the newest result; an older one is dropped and says so.
    fn ready(&mut self, id: u64, pixels: Option<Pixels>) -> bool {
        if id != self.id {
            return false;
        }
        self.cancel = None;
        let Some(meta) = self.pending.take() else {
            return false;
        };
        let image = pixels.map(|p| Handle::from_rgba(p.width, p.height, p.rgba));
        self.shown = Some((meta, image));
        true
    }
}

#[derive(Default)]
pub(super) struct Stage {
    dragging: bool,
    /// The last pointer move of a drag; the lit clock waits for it to rest.
    moved: Option<Instant>,
    backdrop: Job<[u32; 2]>,
    lit: Job<crate::depth::Request>,
    /// What the worker found for the shown lit clock's picture.
    found: Option<crate::depth::Found>,
}
impl Stage {
    pub(super) fn dragging(&self) -> bool {
        self.dragging
    }
    /// The wallpaper fitted to the screen's shape, when it is ready.
    pub(super) fn backdrop(&self) -> Option<&Handle> {
        self.backdrop.shown.as_ref()?.1.as_ref()
    }
    /// The lit clock, only when it was drawn for exactly this frame.
    pub(super) fn lit(&self, frame: &crate::depth::Frame) -> Option<&Handle> {
        let (request, image) = self.lit.shown.as_ref()?;
        (request.frame == *frame)
            .then_some(image.as_ref())
            .flatten()
    }
    /// What the newest finished render found: Ridge, a picture nobody has
    /// analysed, or the verdict for the clock where it was drawn.
    pub(super) fn found(&self) -> Option<&crate::depth::Found> {
        self.lit.shown.as_ref().and(self.found.as_ref())
    }
    /// The spot the scene suggests, only for the clock where it is now.
    pub(super) fn suggested(&self, frame: &crate::depth::Frame) -> Option<[f32; 2]> {
        let (request, _) = self.lit.shown.as_ref()?;
        match self.found.as_ref()? {
            crate::depth::Found::Judged { judgement, .. }
                if request.frame == *frame
                    && judgement.verdict == crate::depth::scene::Verdict::NeedsNudge =>
            {
                judgement.suggested
            }
            _ => None,
        }
    }
    /// Draws the lit clock again for the same request, after its picture
    /// was analysed or forgotten. The shown one stays until then.
    pub(super) fn redraw(&mut self) {
        self.lit.key = None;
    }
    /// The newest lit clock asked of the worker.
    #[cfg(test)]
    pub(super) fn requested(&self) -> Option<&crate::depth::Request> {
        self.lit
            .pending
            .as_ref()
            .or(self.lit.shown.as_ref().map(|(request, _)| request))
    }
    #[cfg(test)]
    pub(super) fn show_lit(&mut self, request: crate::depth::Request, pixels: Option<Pixels>) {
        self.lit.key = serde_json::to_vec(&request).ok();
        self.lit.pending = Some(request);
        if self.lit.ready(self.lit.id, pixels) {
            self.found = Some(crate::depth::Found::Ridge);
        }
    }
}

/// Width over height of a screen, 16:9 when it is not known.
pub(super) fn aspect(screen: [f32; 2]) -> f32 {
    if screen.iter().all(|v| v.is_finite() && *v >= 1.) {
        screen[0] / screen[1]
    } else {
        16. / 9.
    }
}
/// The screen, or a 1920×1080 one when it is not known.
pub(super) fn known(screen: [f32; 2]) -> [f32; 2] {
    if screen.iter().all(|v| v.is_finite() && *v >= 1.) {
        screen
    } else {
        [1920., 1080.]
    }
}
/// The stage's size in the space it is given: the screen's shape, no wider
/// than [`MAX_WIDTH`] and no taller than `height`.
pub(super) fn stage_size(available: [f32; 2], screen: [f32; 2]) -> [f32; 2] {
    let aspect = aspect(screen);
    let width = available[0]
        .min(MAX_WIDTH)
        .min(available[1] * aspect)
        .max(1.);
    [width, width / aspect]
}
/// Stage pixels per screen pixel.
pub(super) fn scale(stage: [f32; 2], screen: [f32; 2]) -> f32 {
    stage[0] / screen[0].max(1.)
}
pub(super) fn to_stage(point: [f32; 2], scale: f32) -> [f32; 2] {
    point.map(|v| v * scale)
}
#[cfg(test)]
pub(super) fn to_screen(point: [f32; 2], scale: f32) -> [f32; 2] {
    point.map(|v| v / scale)
}
/// The wallpaper picture's size: the screen's shape, at most [`BACKDROP`].
pub(super) fn backdrop_size(screen: [f32; 2]) -> [u32; 2] {
    let aspect = aspect(screen);
    let size = if aspect >= 1. {
        [BACKDROP, BACKDROP / aspect]
    } else {
        [BACKDROP * aspect, BACKDROP]
    };
    size.map(|v| (v.round() as u32).max(1))
}
/// The desktop's depth frame for a clock at `position`: whole pixels, kept
/// on the screen.
pub(super) fn frame(position: [f32; 2], size: [f32; 2], screen: [f32; 2]) -> crate::depth::Frame {
    crate::depth::Frame {
        screen,
        origin: [0, 1].map(|i| {
            position[i]
                .round()
                .min((screen[i] - size[i]).floor())
                .max(0.)
        }),
        size,
        scale: 1.,
    }
}

impl Settings {
    /// The stage shows the Clock page's own screen, not the arrangement
    /// editor, and only while no canvas draft owns the layout.
    pub(super) fn clock_stage_open(&self) -> bool {
        self.page == Page::Clock && self.canvas.is_none() && !self.canvas_visible()
    }
    /// The clock's place on the stage's screen: position and size.
    pub(super) fn clock_stage_place(&self) -> Option<([f32; 2], [f32; 2])> {
        Some((
            self.placement.widget_position(Widget::Clock.key())?,
            self.placement.display_size(Widget::Clock.key())?,
        ))
    }
    /// The stage edits the display the clock is on.
    fn clock_stage_display(&mut self) {
        if self.placement.dragging() || self.placement.dirty() {
            return;
        }
        if let Some(display) = self
            .placement
            .preview_snapshot()
            .widget_displays
            .get(Widget::Clock.key())
            .cloned()
            && self.placement.display.as_ref() != Some(&display)
        {
            self.select_monitor_preview(display);
        }
    }
    fn clock_stage_entry(&self) -> Option<BgEntry> {
        match self.wallpaper.entry()? {
            crate::desktop::panels::Setting::Known(entry) => Some(entry),
            crate::desktop::panels::Setting::Unsupported(_) => None,
        }
    }
    /// The lit clock the desktop would show at the clock's current place,
    /// allowed to use this computer's analysed pictures.
    fn clock_stage_request(&self) -> Option<crate::depth::Request> {
        if !crate::depth::eligible(&self.config) || !self.picture_wallpaper() {
            return None;
        }
        let background = self.clock_stage_entry()?;
        if background.scaling_mode == crate::desktop::wallpaper::ScalingMode::Stretch {
            return None;
        }
        let (position, size) = self.clock_stage_place()?;
        let screen = self.placement.preview_snapshot().screen;
        crate::depth::Request::new(
            &self.config,
            frame(position, size, screen),
            background,
            chrono::Local::now().naive_local(),
        )
        .map(|request| request.with_scenes(crate::depth::cache::root()))
    }
    /// Keeps the stage's pictures in step: the wallpaper at the screen's
    /// shape, and the lit clock once a drag rests or ends.
    pub(super) fn clock_stage_sync(&mut self) -> Task<Message> {
        if !self.clock_stage_open() {
            self.clock_stage.lit.want(None, None);
            self.clock_stage.found = None;
            return Task::none();
        }
        self.clock_stage_display();
        let screen = self.placement.preview_snapshot().screen;
        // A 16:9 screen showing the Home picture already has it, fitted the
        // same way, so only other shapes and pictures are drawn here.
        let backdrop = self
            .clock_stage_entry()
            .map(|entry| (entry, backdrop_size(screen)))
            .filter(|(entry, size)| {
                *size != [SCENE.0, SCENE.1] || self.profiles.home_entry() != Some(entry)
            });
        let key = backdrop
            .as_ref()
            .and_then(|backdrop| serde_json::to_vec(backdrop).ok());
        let backdrop = match (
            self.clock_stage
                .backdrop
                .want(key, backdrop.as_ref().map(|b| b.1)),
            backdrop,
        ) {
            (Some((id, cancel)), Some((entry, size))) => cosmic::task::future(async move {
                Message::ClockStage(Edit::Backdrop(
                    id,
                    crate::desktop::wallpaper::media::worker::wallpaper(entry, size, cancel).await,
                ))
            }),
            _ => Task::none(),
        };
        let stage = &self.clock_stage;
        if stage.dragging && stage.moved.is_some_and(|moved| moved.elapsed() < SETTLE) {
            return backdrop;
        }
        let request = self.clock_stage_request();
        let key = request
            .as_ref()
            .and_then(|request| serde_json::to_vec(request).ok());
        if key.is_none() {
            self.clock_stage.found = None;
        }
        let lit = match self.clock_stage.lit.want(key, request.clone()) {
            Some((id, cancel)) => match request.and_then(|r| serde_json::to_vec(&r).ok()) {
                Some(bytes) => cosmic::task::future(async move {
                    Message::ClockStage(Edit::Rendered(
                        id,
                        crate::desktop::wallpaper::media::worker::render_scene(bytes, cancel).await,
                    ))
                }),
                None => Task::none(),
            },
            None => Task::none(),
        };
        Task::batch([backdrop, lit])
    }

    pub(super) fn update_clock_stage(&mut self, event: Edit) -> Task<Message> {
        let clock = Widget::Clock.key();
        match event {
            Edit::Backdrop(id, pixels) => {
                self.clock_stage.backdrop.ready(id, pixels);
            }
            Edit::Rendered(id, rendered) => {
                let (found, pixels) = match rendered {
                    Some(rendered) => (Some(rendered.found), rendered.pixels),
                    None => (None, None),
                };
                if self.clock_stage.lit.ready(id, pixels) {
                    if let Some(crate::depth::Found::Judged { key, .. }) = &found
                        && let Some(root) = crate::depth::cache::root()
                    {
                        crate::depth::cache::touch(&root, key);
                    }
                    self.clock_stage.found = found;
                    return self.scene_analyse();
                }
            }
            Edit::Settle => {}
            _ if !self.clock_stage_open() || self.busy => {
                if self.clock_stage.dragging {
                    self.clock_stage.dragging = false;
                    return self.update_placement(placement::Message::CancelDrag);
                }
            }
            Edit::Pointer(point, scale) => {
                let task = self.update_placement(placement::Message::Pointer(point, scale));
                if self.clock_stage.dragging {
                    self.clock_stage.moved = Some(Instant::now());
                }
                return task;
            }
            Edit::Press => {
                if self.clock_stage.dragging {
                    return Task::none();
                }
                self.clock_stage_display();
                let task = self.update_placement(placement::Message::Press(clock.into()));
                self.clock_stage.dragging = self.placement.dragging();
                self.clock_stage.moved = None;
                return task;
            }
            Edit::Release => {
                if !std::mem::take(&mut self.clock_stage.dragging) {
                    return Task::none();
                }
                self.clock_stage.moved = None;
                // Save ends the drag first, as the arrangement editor does.
                return self.update_placement(placement::Message::Save);
            }
            Edit::Cancel => {
                if std::mem::take(&mut self.clock_stage.dragging) {
                    self.clock_stage.moved = None;
                    return self.update_placement(placement::Message::CancelDrag);
                }
            }
            Edit::Arrow(direction, large) => {
                if self.clock_stage.dragging {
                    return Task::none();
                }
                return cosmic::iced::runtime::task::widget(Focused::default()).map(move |_| {
                    cosmic::Action::App(Message::ClockStage(Edit::Nudge(direction, large)))
                });
            }
            Edit::Nudge(direction, large) => {
                if self.clock_stage.dragging {
                    return Task::none();
                }
                self.clock_stage_display();
                self.placement.nudge_widget(clock, direction, large);
                return self.update_placement(placement::Message::Save);
            }
            Edit::Reset => {
                if self.clock_stage.dragging {
                    return Task::none();
                }
                self.clock_stage_display();
                if self.placement.reset_widget(clock) {
                    return self.update_placement(placement::Message::Save);
                }
            }
            Edit::Suggested => {
                if self.clock_stage.dragging {
                    return Task::none();
                }
                self.clock_stage_display();
                let screen = self.placement.preview_snapshot().screen;
                let spot = self
                    .clock_stage_place()
                    .and_then(|(at, size)| self.clock_stage.suggested(&frame(at, size, screen)));
                if let Some(spot) = spot
                    && self.placement.place_widget(clock, spot)
                {
                    return self.update_placement(placement::Message::Save);
                }
            }
        }
        Task::none()
    }
    /// Whether the clock is somewhere other than its automatic place.
    pub(super) fn clock_stage_moved(&self) -> bool {
        let clock = Widget::Clock.key();
        match (
            self.placement.widget_position(clock),
            self.placement.default_position(clock),
        ) {
            (Some(now), Some(default)) => (0..2).any(|i| (now[i] - default[i]).abs() >= 0.5),
            _ => false,
        }
    }
}

/// Mouse release, focus loss and Escape end a stage drag; a resting pointer
/// redraws the lit clock.
pub(super) fn drag_input(
    event: cosmic::iced::Event,
    _status: cosmic::iced::event::Status,
) -> Option<Edit> {
    use cosmic::iced::{Event, keyboard, mouse};
    match event {
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => Some(Edit::Release),
        Event::Mouse(mouse::Event::CursorLeft)
        | Event::Window(cosmic::iced::window::Event::Unfocused)
        | Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            ..
        }) => Some(Edit::Cancel),
        _ => None,
    }
}
/// Arrow keys nobody else used; they move the clock only when the stage
/// clock has keyboard focus.
pub(super) fn arrow_input(
    event: cosmic::iced::Event,
    status: cosmic::iced::event::Status,
) -> Option<Edit> {
    use cosmic::iced::{Event, keyboard};
    use keyboard::key::Named;
    let Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(key),
        modifiers,
        ..
    }) = event
    else {
        return None;
    };
    if status != cosmic::iced::event::Status::Ignored
        || modifiers.alt()
        || modifiers.control()
        || modifiers.logo()
    {
        return None;
    }
    let direction = match key {
        Named::ArrowLeft => [-1, 0],
        Named::ArrowRight => [1, 0],
        Named::ArrowUp => [0, -1],
        Named::ArrowDown => [0, 1],
        _ => return None,
    };
    Some(Edit::Arrow(direction, modifiers.shift()))
}

pub(super) const CLOCK_ID: &str = "clock-stage-clock";

/// Finds whether the stage clock holds keyboard focus.
#[derive(Default)]
pub struct Focused(bool);
impl cosmic::iced::advanced::widget::Operation<()> for Focused {
    fn traverse(
        &mut self,
        operate: &mut dyn FnMut(&mut dyn cosmic::iced::advanced::widget::Operation<()>),
    ) {
        operate(self);
    }
    fn focusable(
        &mut self,
        id: Option<&cosmic::widget::Id>,
        _: cosmic::iced::Rectangle,
        state: &mut dyn cosmic::iced::advanced::widget::operation::Focusable,
    ) {
        if state.is_focused() && id == Some(&cosmic::widget::Id::new(CLOCK_ID)) {
            self.0 = true;
        }
    }
    fn finish(&self) -> cosmic::iced::advanced::widget::operation::Outcome<()> {
        if self.0 {
            cosmic::iced::advanced::widget::operation::Outcome::Some(())
        } else {
            cosmic::iced::advanced::widget::operation::Outcome::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::wallpaper::ScalingMode;
    use crate::desktop::wallpaper::media::fit_preview;

    fn black(width: u32, height: u32) -> Pixels {
        Pixels {
            width,
            height,
            rgba: [0, 0, 0, 255].repeat((width * height) as usize),
        }
    }

    #[test]
    fn stage_takes_the_screen_shape_and_caps_its_width() {
        // 1280 window: capped, 16:9 like the screen.
        assert_eq!(stage_size([1000., 900.], [1920., 1080.]), [600., 337.5]);
        // 820 window: the content width, still the screen's shape.
        let narrow = stage_size([560., 900.], [2560., 1600.]);
        assert_eq!(narrow, [560., 350.]);
        // A portrait screen is bounded by height instead.
        let tall = stage_size([600., 400.], [1080., 1920.]);
        assert!((tall[1] - 400.).abs() < 0.01 && (tall[0] / tall[1] - 0.5625).abs() < 0.001);
        // Unknown screens fall back to 16:9.
        assert_eq!(aspect([0., 0.]), 16. / 9.);
        assert_eq!(stage_size([800., 900.], [f32::NAN, 1080.]), [600., 337.5]);
    }

    #[test]
    fn stage_and_screen_points_map_both_ways() {
        let screen = [1920., 1080.];
        let stage = stage_size([1000., 900.], screen);
        let scale = scale(stage, screen);
        assert!((scale - 0.3125).abs() < 1e-6);
        let point = [560., 188.];
        let on_stage = to_stage(point, scale);
        assert_eq!(on_stage, [175., 58.75]);
        let back = to_screen(on_stage, scale);
        assert!((0..2).all(|i| (back[i] - point[i]).abs() < 1e-3));
        // The stage's far corner is the screen's far corner.
        let corner = to_screen(stage, scale);
        assert!((0..2).all(|i| (corner[i] - screen[i]).abs() < 1e-3));
    }

    #[test]
    fn depth_frame_stays_on_the_screen_in_whole_pixels() {
        let screen = [1920., 1080.];
        let size = [800., 330.];
        let inside = frame([560.4, 187.6], size, screen);
        assert_eq!(inside.origin, [560., 188.]);
        assert!(inside.pixels().is_some());
        // A position half a pixel past the edge would make an invalid frame.
        let edge = frame([1120.5, 750.5], size, screen);
        assert_eq!(edge.origin, [1120., 750.]);
        assert!(edge.pixels().is_some());
        assert_eq!(frame([-3., -0.4], size, screen).origin, [0., 0.]);
    }

    /// The stage picture has the screen's shape, so each scaling mode puts
    /// the picture where the desktop does, relative to the screen.
    #[test]
    fn backdrop_matches_the_desktop_for_every_scaling_mode() {
        assert_eq!(backdrop_size([1920., 1080.]), [1280, 720]);
        assert_eq!(backdrop_size([2560., 1600.]), [1280, 800]);
        assert_eq!(backdrop_size([1080., 1920.]), [720, 1280]);
        assert_eq!(backdrop_size([0., 0.]), [1280, 720]);
        // A 4:3 picture with a white centre column and a black edge.
        let mut picture = black(400, 300);
        for y in 0..300 {
            for x in 190..210 {
                let i = (y * 400 + x) * 4;
                picture.rgba[i..i + 3].copy_from_slice(&[255, 255, 255]);
            }
        }
        let fill = [10, 200, 30];
        let white_columns = |p: &Pixels, row: u32| -> Vec<u32> {
            (0..p.width)
                .filter(|x| p.rgba[((row * p.width + x) * 4) as usize] > 200)
                .collect()
        };
        let green_columns = |p: &Pixels, row: u32| -> u32 {
            (0..p.width)
                .filter(|x| {
                    let i = ((row * p.width + x) * 4) as usize;
                    p.rgba[i + 1] > 150 && p.rgba[i] < 60
                })
                .count() as u32
        };
        for screen in [[1920., 1080.], [2560., 1600.]] {
            let desktop = screen.map(|v| v as u32);
            let stage = backdrop_size(screen);
            for mode in [
                ScalingMode::Zoom,
                ScalingMode::Fit([
                    fill[0] as f32 / 255.,
                    fill[1] as f32 / 255.,
                    fill[2] as f32 / 255.,
                ]),
                ScalingMode::Stretch,
            ] {
                let big = fit_preview(&picture, &mode, desktop[0], desktop[1]);
                let small = fit_preview(&picture, &mode, stage[0], stage[1]);
                let fraction = |p: &Pixels, columns: Vec<u32>| -> [f32; 2] {
                    [
                        *columns.first().unwrap() as f32 / p.width as f32,
                        *columns.last().unwrap() as f32 / p.width as f32,
                    ]
                };
                let a = fraction(&big, white_columns(&big, big.height / 2));
                let b = fraction(&small, white_columns(&small, small.height / 2));
                assert!(
                    (0..2).all(|i| (a[i] - b[i]).abs() < 0.01),
                    "{mode:?} at {screen:?}: {a:?} vs {b:?}"
                );
                let bars = |p: &Pixels| green_columns(p, p.height / 2) as f32 / p.width as f32;
                assert!(
                    (bars(&big) - bars(&small)).abs() < 0.01,
                    "{mode:?} letterbox"
                );
                assert_eq!(
                    bars(&small) > 0.,
                    matches!(mode, ScalingMode::Fit(_)),
                    "{mode:?}"
                );
            }
        }
    }

    #[test]
    fn newest_lit_clock_wins_and_old_results_are_dropped() {
        let mut job: Job<u8> = Job::default();
        let (first, cancel) = job.want(Some(vec![1]), Some(1)).unwrap();
        let (second, _) = job.want(Some(vec![2]), Some(2)).unwrap();
        assert!(*cancel.borrow(), "the older render is cancelled");
        assert!(
            job.want(Some(vec![2]), Some(2)).is_none(),
            "same key, no new work"
        );
        job.ready(first, Some(black(1, 1)));
        assert!(job.shown.is_none());
        job.ready(second, Some(black(1, 1)));
        assert_eq!(job.shown.as_ref().map(|s| s.0), Some(2));
        job.want(None, None);
        assert!(job.shown.is_none() && job.pending.is_none());
    }
}
