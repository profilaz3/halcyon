//! The Clock page's stage: the screen the clock is on, scaled down, with the
//! clock where it really sits. With the scene on, the clock is the desktop's
//! own masked and lit render for that place, and the stage offers the spot
//! where the picture's foreground would cover it. Dragging it moves the
//! clock and its corner resizes it, shape kept; both save through the
//! arrangement's save, one Undo step each.
use super::*;
use crate::desktop::wallpaper::BgEntry;
use crate::desktop::wallpaper::media::{Pixels, SCENE};
#[cfg(test)]
use cosmic::iced::Color;
use cosmic::iced::Point;
use cosmic::widget::image::Handle;

/// The map's tint and band on the stage, and its word on where the clock is.
pub(super) const TINT_ID: &str = "clock-stage-tint";
pub(super) const BAND_ID: &str = "clock-stage-band";
pub(super) const FIT_ID: &str = "clock-stage-fit";
/// What the stage says about where the clock is on the map.
pub(crate) const FIT_ON: &str = "Behind the scene";
pub(crate) const FIT_HIGH: &str = "Lower it onto the green band";
pub(crate) const FIT_LOW: &str = "Too low: the scene covers the top of the time";
pub(crate) const FIT_ELSEWHERE: &str = "Doesn’t tuck in here. The green band shows where it does.";
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
    /// A press on the corner handle: the drag resizes the clock's box.
    ResizePress,
    Release,
    Cancel,
    /// An arrow key; it moves the clock only while the stage clock has focus.
    Arrow([i8; 2], bool),
    Nudge([i8; 2], bool),
    /// `+` or `-`; it resizes the clock only while the stage clock has focus.
    Grow(i8),
    /// Tenths of the clock's automatic size, larger or smaller.
    Step(i8),
    /// The pointer left the stage (or, in tests, came over the clock).
    Hover(bool),
    /// Keyboard focus may have moved; asks whether the stage clock has it.
    CheckFocus,
    Focus(bool),
    Reset,
    /// Moves the clock to where the scene covers it.
    Suggested,
    /// A mid-drag tick: once the pointer rests, the lit clock is drawn again.
    Settle,
    Backdrop(u64, Option<Pixels>),
    Rendered(u64, Option<crate::depth::Rendered>),
    /// Where the clock tucks behind the picture, for its size anywhere on
    /// the stage. `Some(None)` is the worker saying there is no map.
    Mapped(u64, Option<Option<crate::depth::Placement>>),
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

/// The worker's map for the clock's size: where it tucks behind the
/// picture, and the tint and band drawn over the whole screen.
pub(super) struct Shown {
    pub places: crate::depth::scene::Places,
    pub tint: Handle,
    pub band: Handle,
    /// The clock size and whole screen it was drawn for.
    size: [f32; 2],
    screen: [f32; 2],
}

/// The map in flight and the one shown. A new picture, style or clock size
/// asks again and forgets the old map, which would be wrong for it.
#[derive(Default)]
struct Map {
    key: Option<Vec<u8>>,
    id: u64,
    cancel: Option<tokio::sync::watch::Sender<bool>>,
    pending: Option<[[f32; 2]; 2]>,
    shown: Option<Shown>,
}
impl Map {
    fn want(
        &mut self,
        key: Option<Vec<u8>>,
        frame: Option<[[f32; 2]; 2]>,
    ) -> Option<(u64, tokio::sync::watch::Receiver<bool>)> {
        if self.key == key {
            return None;
        }
        if let Some(cancel) = self.cancel.take() {
            cancel.send_replace(true);
        }
        self.id = self.id.wrapping_add(1);
        self.shown = None;
        self.key = key.filter(|_| frame.is_some());
        self.pending = frame.filter(|_| self.key.is_some());
        self.key.as_ref()?;
        let (send, cancel) = tokio::sync::watch::channel(false);
        self.cancel = Some(send);
        Some((self.id, cancel))
    }
    fn ready(&mut self, id: u64, placement: Option<Option<crate::depth::Placement>>) {
        if id != self.id {
            return;
        }
        self.cancel = None;
        let Some([size, screen]) = self.pending.take() else {
            return;
        };
        let handle = |p: Pixels| Handle::from_rgba(p.width, p.height, p.rgba);
        self.shown = placement.flatten().map(|placement| Shown {
            places: placement.places,
            tint: handle(placement.tint),
            band: handle(placement.band),
            size,
            screen,
        });
    }
}

/// How far, as a share of the screen's height, a dropped clock settles
/// onto the band.
pub(super) const SETTLE_REACH: f32 = 0.06;

#[derive(Default)]
pub(super) struct Stage {
    dragging: bool,
    /// The pointer is over the clock, or the clock has keyboard focus: the
    /// corner handle shows.
    hover: bool,
    focus: bool,
    /// The last pointer move of a drag; the lit clock waits for it to rest.
    moved: Option<Instant>,
    backdrop: Job<[u32; 2]>,
    lit: Job<crate::depth::Request>,
    /// What the worker found for the shown lit clock's picture.
    found: Option<crate::depth::Found>,
    map: Map,
}
impl Stage {
    pub(super) fn dragging(&self) -> bool {
        self.dragging
    }
    /// Whether the corner handle shows: pointer over the clock, keyboard
    /// focus on it, or a drag in progress.
    pub(super) fn handle(&self) -> bool {
        self.hover || self.focus || self.dragging
    }
    /// The newest lit clock, whatever frame it was drawn for; a resize
    /// shows it scaled until the new size is drawn.
    pub(super) fn last_lit(&self) -> Option<&Handle> {
        self.lit.shown.as_ref()?.1.as_ref()
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
    /// The verdict for the clock exactly where it was last drawn.
    pub(super) fn judgement(
        &self,
        frame: &crate::depth::Frame,
    ) -> Option<&crate::depth::scene::Judgement> {
        let (request, _) = self.lit.shown.as_ref()?;
        match self.found.as_ref()? {
            crate::depth::Found::Judged { judgement, .. } if request.frame == *frame => {
                Some(judgement)
            }
            _ => None,
        }
    }
    /// The map, only when it was drawn for this clock's size and screen.
    pub(super) fn map(&self, frame: &crate::depth::Frame) -> Option<&Shown> {
        self.map
            .shown
            .as_ref()
            .filter(|shown| shown.size == frame.size && shown.screen == frame.screen)
    }
    /// Draws the lit clock and the map again for the same request, after
    /// its picture was analysed or forgotten. The shown one stays until then.
    pub(super) fn redraw(&mut self) {
        self.lit.key = None;
        self.map.key = None;
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
/// Below this, the stage says the clock is hard to read where it sits.
pub(super) const LOW_CONTRAST: f32 = 3.;
/// How `ink` stands out from the picture under `area` (fractions of the
/// picture: left, top, right, bottom): its contrast ratio with the middle
/// brightness there, so a few bright or dark spots don't decide it. `None`
/// when the area misses the picture.
#[cfg(test)]
pub(super) fn picture_contrast(
    ink: Color,
    rgba: &[u8],
    size: [u32; 2],
    area: [f32; 4],
) -> Option<f32> {
    let [_, middle, _] = crate::app::backdrop_brightness(rgba, size, area)?;
    let ink = crate::widget_style::luminance(ink);
    Some((ink.max(middle) + 0.05) / (ink.min(middle) + 0.05))
}
/// What the stage shows: the whole output, and the area on it that widgets
/// use (panels that keep space make it smaller and move it off the corner).
/// Widget places are in the area; the picture covers the whole output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Screen {
    pub whole: [f32; 2],
    pub origin: [f32; 2],
    pub area: [f32; 2],
}
impl Screen {
    pub(super) fn of(snapshot: &crate::layout_state::Snapshot) -> Self {
        let area = known(snapshot.screen);
        match snapshot.output {
            Some(output)
                if output.size.iter().all(|v| v.is_finite() && *v >= 1.)
                    && (0..2).all(|i| {
                        output.origin[i] >= 0. && output.origin[i] + area[i] <= output.size[i] + 1.
                    }) =>
            {
                Self {
                    whole: output.size,
                    origin: output.origin,
                    area,
                }
            }
            _ => area.into(),
        }
    }
    /// A point in the area, on the whole output.
    pub(super) fn on_output(&self, point: [f32; 2]) -> [f32; 2] {
        [point[0] + self.origin[0], point[1] + self.origin[1]]
    }
    /// A point on the whole output, in the area.
    pub(super) fn in_area(&self, point: [f32; 2]) -> [f32; 2] {
        [point[0] - self.origin[0], point[1] - self.origin[1]]
    }
}
/// A screen whose area is the whole output.
impl From<[f32; 2]> for Screen {
    fn from(area: [f32; 2]) -> Self {
        let area = known(area);
        Self {
            whole: area,
            origin: [0., 0.],
            area,
        }
    }
}
/// The desktop's depth frame for a clock at `position` in the area: on the
/// whole output, as the desktop cuts it, in whole pixels, kept in the area.
pub(super) fn frame(
    position: [f32; 2],
    size: [f32; 2],
    screen: impl Into<Screen>,
) -> crate::depth::Frame {
    let screen = screen.into();
    crate::depth::Frame {
        screen: screen.whole,
        origin: screen.on_output([0, 1].map(|i| {
            position[i]
                .round()
                .min((screen.area[i] - size[i]).floor())
                .max(0.)
        })),
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
    /// The stage's screen: the clock's output and the area widgets use.
    pub(super) fn clock_stage_screen(&self) -> Screen {
        Screen::of(self.placement.preview_snapshot())
    }
    /// The clock's place in the area widgets use: position and size.
    /// Where Daily overview sits and its size, when it shows the time in
    /// place of the clock.
    pub(super) fn clock_stage_overview(&self) -> Option<([f32; 2], [f32; 2])> {
        let key = Widget::Glance.key();
        (self.config.glance.show_clock && self.config.widget_visible(Widget::Glance))
            .then_some(())?;
        Some((
            self.placement.widget_position(key)?,
            self.placement.display_size(key)?,
        ))
    }
    pub(super) fn clock_stage_place(&self) -> Option<([f32; 2], [f32; 2])> {
        Some((
            self.placement.widget_position(Widget::Clock.key())?,
            self.placement.display_size(Widget::Clock.key())?,
        ))
    }
    /// The clock's contrast with the picture behind it, when it is drawn
    /// straight on the picture and the picture is loaded. The clock turns
    /// its ink to suit the picture's middle, so this is how its ink stands
    /// out from the picture's darker and lighter parts: low only where the
    /// picture is busy under it.
    pub(super) fn clock_stage_contrast(&self) -> Option<f32> {
        let [dark, middle, light] = self.clock_stage_brightness()?;
        let style = {
            let _scope = crate::widget_style::scope(&self.config);
            crate::widget_style::presentation(&self.config, self.config.wallpaper_palette(), true)
        };
        let ink = crate::widget_style::luminance(crate::app::adapted_ink(style.ink.value, middle));
        let contrast = |back: f32| (ink.max(back) + 0.05) / (ink.min(back) + 0.05);
        Some(contrast(dark).min(contrast(light)))
    }
    /// The picture's middle brightness under a clock drawn straight on it,
    /// which the desktop measures too to pick the clock's ink.
    pub(super) fn clock_stage_backdrop(&self) -> Option<f32> {
        self.clock_stage_brightness().map(|[_, middle, _]| middle)
    }
    /// The brightness of the picture under the clock, darker tenth, middle
    /// and lighter tenth, when the clock is drawn straight on the picture.
    fn clock_stage_brightness(&self) -> Option<[f32; 3]> {
        if !self.config.widget_visible(Widget::Clock) || self.config.recipes.clock.is_some() {
            return None;
        }
        let style = {
            let _scope = crate::widget_style::scope(&self.config);
            crate::widget_style::presentation(&self.config, self.config.wallpaper_palette(), true)
        };
        if style.surface.value != crate::widget_style::resolver::Surface::None {
            return None;
        }
        let Handle::Rgba {
            width,
            height,
            pixels,
            ..
        } = self.clock_stage.backdrop().or(self.profiles.home_image())?
        else {
            return None;
        };
        let (position, size) = self.clock_stage_place()?;
        let screen = self.clock_stage_screen();
        let [x, y] = screen.on_output(position);
        let whole = screen.whole;
        let area = [
            x / whole[0],
            y / whole[1],
            (x + size[0]) / whole[0],
            (y + size[1]) / whole[1],
        ];
        crate::app::backdrop_brightness(pixels, [*width, *height], area)
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
        let screen = self.clock_stage_screen();
        crate::depth::Request::new(
            &self.config,
            frame(position, size, screen),
            background,
            chrono::Local::now().naive_local(),
        )
        .map(|request| crate::depth::Request {
            backdrop: self.clock_stage_backdrop(),
            ..request.with_scenes(crate::depth::cache::root())
        })
    }
    /// Where the clock tucks behind the picture, for its size anywhere in
    /// the area widgets use. It doesn't change as the clock moves, nor with
    /// the minute.
    fn clock_stage_map_request(&self) -> Option<crate::depth::Request> {
        let request = self.clock_stage_request()?;
        let screen = self.clock_stage_screen();
        let day = request.time.get(.."YYYY-MM-DD".len())?.to_owned();
        Some(crate::depth::Request {
            frame: crate::depth::Frame {
                origin: screen.origin,
                ..request.frame.clone()
            },
            time: format!("{day}T00:00:00"),
            backdrop: None,
            map: Some([
                screen.origin[0],
                screen.origin[1],
                screen.area[0],
                screen.area[1],
            ]),
            ..request
        })
    }
    /// The map for the clock as it is now, and where the clock sits on it.
    /// At rest, the verdict for exactly this place decides; while it moves,
    /// the map does.
    pub(super) fn clock_stage_fit(&self) -> Option<(&Shown, crate::depth::scene::Fit)> {
        use crate::depth::scene::{Fit, Verdict};
        let (position, size) = self.clock_stage_place()?;
        let screen = self.clock_stage_screen();
        let frame = frame(position, size, screen);
        let shown = self.clock_stage.map(&frame)?;
        if !shown.places.any() {
            return None;
        }
        let fit = shown.places.fit(frame.origin);
        let judged = (!self.clock_stage.dragging())
            .then(|| self.clock_stage.judgement(&frame))
            .flatten();
        let fit = match judged.map(|j| (j.verdict, j.suggested)) {
            Some((Verdict::Works, _)) => Fit::On,
            Some((_, Some(spot))) if fit == Fit::On => {
                if spot[1] > frame.origin[1] {
                    Fit::High
                } else {
                    Fit::Low
                }
            }
            _ => fit,
        };
        Some((shown, fit))
    }
    /// Whether the stage shows the map: while the clock is handled, or
    /// when it isn't tucked in and the band says where it would be.
    pub(super) fn clock_stage_map_shown(&self) -> Option<(&Shown, crate::depth::scene::Fit)> {
        use crate::depth::scene::Verdict;
        let (shown, fit) = self.clock_stage_fit()?;
        let (position, size) = self.clock_stage_place()?;
        let frame = frame(position, size, self.clock_stage_screen());
        let astray = self
            .clock_stage
            .judgement(&frame)
            .is_some_and(|j| j.verdict != Verdict::Works);
        (self.clock_stage.handle() || astray).then_some((shown, fit))
    }
    /// Where a clock dropped near the band settles: a little inside the
    /// band, at the same x when it can be. `None` when it is on the band
    /// already or too far from it.
    fn clock_stage_settle(&self) -> Option<[f32; 2]> {
        let (position, size) = self.clock_stage_place()?;
        let screen = self.clock_stage_screen();
        let frame = frame(position, size, screen);
        let shown = self.clock_stage.map(&frame)?;
        if shown.places.fit(frame.origin) == crate::depth::scene::Fit::On {
            return None;
        }
        let spot = shown
            .places
            .nearest(frame.origin, SETTLE_REACH * screen.whole[1])?;
        Some(screen.in_area(spot))
    }
    /// Keeps the stage's pictures in step: the wallpaper at the screen's
    /// shape, and the lit clock and the map once a drag rests or ends.
    pub(super) fn clock_stage_sync(&mut self) -> Task<Message> {
        if !self.clock_stage_open() {
            self.clock_stage.lit.want(None, None);
            self.clock_stage.map.want(None, None);
            self.clock_stage.found = None;
            return Task::none();
        }
        self.clock_stage_display();
        let screen = self.clock_stage_screen().whole;
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
        let request = self.clock_stage_map_request();
        let key = request
            .as_ref()
            .and_then(|request| serde_json::to_vec(request).ok());
        let frame = request
            .as_ref()
            .map(|request| [request.frame.size, request.frame.screen]);
        let map = match (self.clock_stage.map.want(key.clone(), frame), key) {
            (Some((id, cancel)), Some(bytes)) => cosmic::task::future(async move {
                Message::ClockStage(Edit::Mapped(
                    id,
                    crate::desktop::wallpaper::media::worker::render_map(bytes, cancel).await,
                ))
            }),
            _ => Task::none(),
        };
        Task::batch([backdrop, lit, map])
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
            Edit::Mapped(id, placement) => self.clock_stage.map.ready(id, placement),
            Edit::Settle => {}
            // Kept even while busy, so the handle never sticks.
            Edit::Hover(over) => self.clock_stage.hover = over,
            Edit::Focus(focused) => self.clock_stage.focus = focused,
            _ if !self.clock_stage_open() || self.busy => {
                if self.clock_stage.dragging {
                    self.clock_stage.dragging = false;
                    return self.update_placement(placement::Message::CancelDrag);
                }
            }
            Edit::Pointer(point, scale) => {
                let task = self.update_placement(placement::Message::Pointer(point, scale));
                self.clock_stage.hover = self.placement.pointer_over(clock);
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
            Edit::ResizePress => {
                if self.clock_stage.dragging {
                    return Task::none();
                }
                self.clock_stage_display();
                let task = self.update_placement(placement::Message::ResizePress(clock.into()));
                self.placement.keep_shape();
                self.clock_stage.dragging = self.placement.dragging();
                self.clock_stage.moved = None;
                return task;
            }
            Edit::Release => {
                if !self.clock_stage.dragging {
                    return Task::none();
                }
                // Dropped near the band, the clock settles onto it, in the
                // same Undo step as the move.
                let settle = (!self.placement.resizing())
                    .then(|| self.clock_stage_settle())
                    .flatten();
                self.clock_stage.dragging = false;
                self.clock_stage.moved = None;
                if let Some(spot) = settle {
                    self.placement.release_at(clock, spot);
                }
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
            Edit::Grow(steps) => {
                if self.clock_stage.dragging {
                    return Task::none();
                }
                return cosmic::iced::runtime::task::widget(Focused::default())
                    .map(move |_| cosmic::Action::App(Message::ClockStage(Edit::Step(steps))));
            }
            Edit::Step(steps) => {
                if self.clock_stage.dragging {
                    return Task::none();
                }
                self.clock_stage_display();
                if self.placement.step_widget_size(clock, steps) {
                    return self.update_placement(placement::Message::Save);
                }
            }
            Edit::CheckFocus => return focus_check(),
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
                let screen = self.clock_stage_screen();
                let spot = self
                    .clock_stage_place()
                    .and_then(|(at, size)| self.clock_stage.suggested(&frame(at, size, screen)));
                // The scene's spot is on the whole output; places are in
                // the area widgets use.
                if let Some(spot) = spot
                    && self.placement.place_widget(clock, screen.in_area(spot))
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
/// Keys nobody else used: arrows move the clock, `+` and `-` (or Ctrl with
/// an arrow) resize it, only when the stage clock has keyboard focus. A
/// click may move focus, so it asks again where focus is.
pub(super) fn arrow_input(
    event: cosmic::iced::Event,
    status: cosmic::iced::event::Status,
) -> Option<Edit> {
    use cosmic::iced::{Event, keyboard, mouse};
    use keyboard::key::Named;
    let (key, modifiers) = match event {
        Event::Mouse(mouse::Event::ButtonPressed(_)) => return Some(Edit::CheckFocus),
        Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => (key, modifiers),
        _ => return None,
    };
    if status != cosmic::iced::event::Status::Ignored || modifiers.alt() || modifiers.logo() {
        return None;
    }
    let grow = match &key {
        keyboard::Key::Character(c) if !modifiers.control() => match c.as_str() {
            "+" | "=" => Some(1),
            "-" | "_" => Some(-1),
            _ => None,
        },
        keyboard::Key::Named(Named::ArrowUp | Named::ArrowRight) if modifiers.control() => Some(1),
        keyboard::Key::Named(Named::ArrowDown | Named::ArrowLeft) if modifiers.control() => {
            Some(-1)
        }
        _ => None,
    };
    if let Some(steps) = grow {
        return Some(Edit::Grow(steps));
    }
    if modifiers.control() {
        return None;
    }
    let direction = match key {
        keyboard::Key::Named(Named::ArrowLeft) => [-1, 0],
        keyboard::Key::Named(Named::ArrowRight) => [1, 0],
        keyboard::Key::Named(Named::ArrowUp) => [0, -1],
        keyboard::Key::Named(Named::ArrowDown) => [0, 1],
        _ => return None,
    };
    Some(Edit::Arrow(direction, modifiers.shift()))
}

/// Asks whether the stage clock holds keyboard focus, for its handle.
pub(super) fn focus_check() -> Task<Message> {
    cosmic::iced::runtime::task::widget(Focused::default())
        .collect()
        .map(|found| cosmic::Action::App(Message::ClockStage(Edit::Focus(!found.is_empty()))))
}

pub(super) const CLOCK_ID: &str = "clock-stage-clock";
/// Daily overview's clock on the stage, when it carries the time.
pub(super) const OVERVIEW_ID: &str = "clock-stage-overview";
/// The stage's picture of the whole output.
pub(super) const SCREEN_ID: &str = "clock-stage-screen";

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

    /// The middle brightness under the clock decides: white digits on a
    /// white sky fail, on a dark one they pass, and a few bright stars or
    /// a strip beside the clock don't change that.
    #[test]
    fn picture_contrast_reads_the_picture_under_the_clock() {
        let white = Color::WHITE;
        // Left half white, right half black, with a white dot in the black.
        let (w, h) = (40u32, 20u32);
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let v = if x < w / 2 || (x == 30 && y == 10) {
                    255
                } else {
                    0
                };
                rgba.extend([v, v, v, 255]);
            }
        }
        let left = picture_contrast(white, &rgba, [w, h], [0., 0., 0.5, 1.]).unwrap();
        let right = picture_contrast(white, &rgba, [w, h], [0.5, 0., 1., 1.]).unwrap();
        assert!(left < LOW_CONTRAST, "{left}");
        assert!(right > 15., "{right}");
        // Dark digits read the other way round.
        let black = Color::BLACK;
        assert!(picture_contrast(black, &rgba, [w, h], [0., 0., 0.5, 1.]).unwrap() > 15.);
        // Mostly on the black side, the middle is black.
        assert!(picture_contrast(white, &rgba, [w, h], [0.3, 0., 1., 1.]).unwrap() > 15.);
        // Off the picture, or a picture without pixels, says nothing.
        assert_eq!(
            picture_contrast(white, &rgba, [w, h], [1.2, 0., 1.5, 1.]),
            None
        );
        assert_eq!(picture_contrast(white, &[], [w, h], [0., 0., 1., 1.]), None);
    }

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
