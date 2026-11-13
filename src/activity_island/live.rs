//! The island's live activities: the quick timer, copies and downloads, and
//! the Music card's scrubber and output picker. Every surface, text and line
//! colour comes from the island's `Palette`; only the accents (timer orange,
//! job blue, done green, the artwork's tint) are the activity's own, made
//! readable on whichever surface the island has.
use super::*;
use cosmic::iced::advanced::{Layout, Renderer as _, Widget, layout, renderer, widget::Tree};
use cosmic::iced::{Border, Rectangle, Size, mouse};

use accent::{DONE, TIMER};
/// The picker's presets and stepper need a little more than a card.
const PICKER_EXTRA: f32 = 14.;
const OUTPUT_ROW: f32 = 32.;
const OUTPUT_GAP: f32 = 2.;
/// A released scrubber keeps its place this long while the player catches up.
const SEEK_SETTLE: Duration = Duration::from_secs(2);
/// Left and right arrows move the track this far.
pub(super) const SEEK_STEP: i64 = 5_000;
/// The room the chooser layout gives below the title line.
const ROOM: f32 = HEIGHT - 50.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum View {
    #[default]
    Card,
    /// Presets and a custom length for a new timer.
    Picker,
    /// The Music card's list of outputs.
    Outputs,
}
/// Where the scrubber was put, until the player reports it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Hold {
    pub key: String,
    pub position_ms: i64,
    pub at: Instant,
    pub dragging: bool,
}
pub(super) struct Ui {
    pub view: View,
    /// The picker's custom length, in minutes.
    pub minutes: i64,
    pub sinks: Vec<outputs::Sink>,
    pub sinks_loading: bool,
    pub seek: Option<Hold>,
    pub jobs: transfers::Tracker,
}
impl Default for Ui {
    fn default() -> Self {
        Self {
            view: View::Card,
            minutes: 15,
            sinks: Vec::new(),
            sinks_loading: false,
            seek: None,
            jobs: transfers::Tracker::default(),
        }
    }
}
#[derive(Debug, Clone)]
pub enum Msg {
    TimerPicker,
    /// Start a timer of this many minutes.
    TimerStart(i64),
    /// The custom length one step up or down.
    TimerStep(i64),
    TimerAdd,
    TimerPause,
    TimerResume,
    TimerCancel,
    Outputs,
    OutputsLoaded(Result<Vec<outputs::Sink>, String>),
    OutputPick(outputs::Sink),
    Seek(f64),
    SeekRelease,
    SeekBy(i64),
    Transfer(transfers::Update),
    TransferOpen,
    /// A finished download, selected in the file manager.
    TransferOpenFolder,
    TransferDismiss,
    /// From the picker or the outputs back to the card.
    Back,
}

/// `m:ss`, or `h:mm:ss` from an hour.
pub(super) fn timer_clock(ms: i64) -> String {
    let s = (ms.max(0) + 999) / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}
/// The custom length one step on: minutes to 10, fives to an hour, then
/// quarter hours.
pub(super) fn step_minutes(current: i64, direction: i64) -> i64 {
    let step = |m: i64| match m {
        ..10 => 1,
        10..60 => 5,
        _ => 15,
    };
    let next = if direction > 0 {
        let s = step(current);
        (current / s + 1) * s
    } else {
        let s = step(current - 1);
        (current - 1) / s * s
    };
    next.clamp(1, timer::MAX_MS / 60_000)
}
/// What the scrubber shows: the dragged place, then the requested one until
/// the player reports near it, otherwise the player's own interpolated place.
pub(super) fn shown_position(
    hold: Option<&Hold>,
    key: &str,
    reported: (i64, i64),
    playing: bool,
    now: Instant,
) -> (i64, i64) {
    let (at, total) = reported;
    let Some(h) = hold.filter(|h| h.key == key) else {
        return reported;
    };
    if h.dragging {
        return (h.position_ms.clamp(0, total), total);
    }
    let since = now.saturating_duration_since(h.at);
    if since >= SEEK_SETTLE {
        return reported;
    }
    let expected = h.position_ms + if playing { since.as_millis() as i64 } else { 0 };
    if (at - expected).abs() <= 1_500 {
        return reported;
    }
    (expected.clamp(0, total), total)
}
/// The unlit part of a ring or scrubber.
fn track(palette: Palette) -> Color {
    palette.track
}

/// A progress ring, lit clockwise from the top. Drawn as round dots so it
/// needs no path support and stays a handful of quads.
pub(super) struct Ring {
    pub size: f32,
    pub stroke: f32,
    pub fraction: f32,
    pub color: Color,
    pub track: Color,
}
impl Widget<Message, cosmic::Theme, cosmic::Renderer> for Ring {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.size), Length::Fixed(self.size))
    }
    fn layout(&mut self, _: &mut Tree, _: &cosmic::Renderer, _: &layout::Limits) -> layout::Node {
        layout::Node::new(Size::new(self.size, self.size))
    }
    fn draw(
        &self,
        _: &Tree,
        r: &mut cosmic::Renderer,
        _: &cosmic::Theme,
        _: &renderer::Style,
        l: Layout<'_>,
        _: mouse::Cursor,
        _: &Rectangle,
    ) {
        let b = l.bounds();
        let centre = b.center();
        let radius = (self.size - self.stroke) / 2.;
        let dots = ((std::f32::consts::TAU * radius) / (self.stroke * 0.45))
            .ceil()
            .clamp(24., 180.) as usize;
        let lit = self.fraction.clamp(0., 1.);
        for i in 0..dots {
            let t = i as f32 / dots as f32;
            let angle = -std::f32::consts::FRAC_PI_2 + t * std::f32::consts::TAU;
            r.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: centre.x + radius * angle.cos() - self.stroke / 2.,
                        y: centre.y + radius * angle.sin() - self.stroke / 2.,
                        width: self.stroke,
                        height: self.stroke,
                    },
                    border: Border {
                        radius: (self.stroke / 2.).into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                if lit > 0. && t <= lit {
                    self.color
                } else {
                    self.track
                },
            );
        }
    }
}
impl<'a> From<Ring> for Element<'a, Message> {
    fn from(value: Ring) -> Self {
        Element::new(value)
    }
}
/// Music playing: four still bars in the artwork's colour.
pub(super) struct Waves {
    pub size: f32,
    pub color: Color,
}
impl Widget<Message, cosmic::Theme, cosmic::Renderer> for Waves {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.size), Length::Fixed(self.size))
    }
    fn layout(&mut self, _: &mut Tree, _: &cosmic::Renderer, _: &layout::Limits) -> layout::Node {
        layout::Node::new(Size::new(self.size, self.size))
    }
    fn draw(
        &self,
        _: &Tree,
        r: &mut cosmic::Renderer,
        _: &cosmic::Theme,
        _: &renderer::Style,
        l: Layout<'_>,
        _: mouse::Cursor,
        _: &Rectangle,
    ) {
        let b = l.bounds();
        let width = (self.size / 6.4).max(1.5);
        let gap = (self.size - 4. * width) / 3.;
        for (i, h) in [0.45, 0.9, 0.62, 0.78].into_iter().enumerate() {
            let height = (self.size * h).round();
            r.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: b.x + i as f32 * (width + gap),
                        y: b.y + (self.size - height) / 2.,
                        width,
                        height,
                    },
                    border: Border {
                        radius: (width / 2.).into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                self.color,
            );
        }
    }
}
impl<'a> From<Waves> for Element<'a, Message> {
    fn from(value: Waves) -> Self {
        Element::new(value)
    }
}
/// Figures in equal cells so a counting clock never shifts sideways.
pub(super) fn tabular<'a>(
    value: &str,
    size: f32,
    color: Color,
    bold: bool,
) -> Element<'a, Message> {
    let digit = (size * 0.62).ceil();
    let narrow = (size * 0.34).ceil();
    widget::row(value.chars().map(|c| {
        let mut glyph = widget::text(c.to_string())
            .size(size)
            .wrapping(cosmic::iced::advanced::text::Wrapping::None)
            .class(cosmic::theme::Text::Color(color));
        if bold {
            glyph = glyph.font(cosmic::font::semibold());
        }
        widget::container(glyph)
            .width(if c.is_ascii_digit() { digit } else { narrow })
            .align_x(cosmic::iced::Alignment::Center)
            .into()
    }))
    .align_y(cosmic::iced::Alignment::Center)
    .into()
}
/// Text whose figures sit in equal cells while words keep their own width,
/// so times such as "in 3 min · 10:00–10:30" never shift as they change.
pub(super) fn tabular_text<'a>(value: &str, size: f32, color: Color) -> Element<'a, Message> {
    let mut parts: Vec<Element<'a, Message>> = Vec::new();
    let mut run = String::new();
    let flush = |run: &mut String, parts: &mut Vec<Element<'a, Message>>| {
        if !run.is_empty() {
            parts.push(
                widget::text(std::mem::take(run))
                    .size(size)
                    .wrapping(cosmic::iced::advanced::text::Wrapping::None)
                    .class(cosmic::theme::Text::Color(color))
                    .into(),
            );
        }
    };
    for c in value.chars() {
        if c.is_ascii_digit() {
            flush(&mut run, &mut parts);
            parts.push(
                widget::container(
                    widget::text(c.to_string())
                        .size(size)
                        .wrapping(cosmic::iced::advanced::text::Wrapping::None)
                        .class(cosmic::theme::Text::Color(color)),
                )
                .width((size * 0.62).ceil())
                .align_x(cosmic::iced::Alignment::Center)
                .into(),
            );
        } else {
            run.push(c);
        }
    }
    flush(&mut run, &mut parts);
    widget::row(parts)
        .align_y(cosmic::iced::Alignment::Center)
        .into()
}
/// A large round control. The primary one is a deep disc of the accent with
/// the glyph in the accent; the others are the island's ink on a quiet disc.
fn round_style(
    palette: Palette,
    accent: Color,
    reveal: f32,
    primary: bool,
) -> cosmic::theme::Button {
    let style = move |focused: bool, level: usize, disabled: bool| {
        let ink = if disabled { 0.32 * reveal } else { reveal };
        let (fill, glyph) = if primary {
            (
                alpha(accent, [0.2, 0.28, 0.36][level] * reveal),
                alpha(accent, ink),
            )
        } else {
            (
                alpha(palette.ink, [0.08, 0.14, 0.2][level] * reveal),
                alpha(palette.ink, ink),
            )
        };
        cosmic::widget::button::Style {
            icon_color: Some(glyph),
            text_color: Some(glyph),
            background: Some(fill.into()),
            border_radius: 28.0.into(),
            outline_width: if focused { 1.5 } else { 0. },
            outline_color: accent,
            ..Default::default()
        }
    };
    cosmic::theme::Button::Custom {
        active: Box::new(move |f, _| style(f, 0, false)),
        hovered: Box::new(move |f, _| style(f, 1, false)),
        pressed: Box::new(move |f, _| style(f, 2, false)),
        disabled: Box::new(move |_| style(false, 0, true)),
    }
}
/// A 56px ring with the time left inside in tabular figures.
fn ring_clock<'a>(
    palette: Palette,
    accent: Color,
    reveal: f32,
    fraction: f32,
    left_ms: i64,
) -> Element<'a, Message> {
    cosmic::iced::widget::stack![
        Ring {
            size: 56.,
            stroke: 4.,
            fraction,
            color: alpha(accent, reveal),
            track: alpha(track(palette), reveal),
        },
        widget::container(tabular(
            &timer_clock(left_ms),
            if left_ms >= 3_600_000 { 10. } else { 13. },
            alpha(palette.ink, reveal),
            true,
        ))
        .width(56)
        .height(56)
        .align_x(cosmic::iced::Alignment::Center)
        .align_y(cosmic::iced::Alignment::Center),
    ]
    .into()
}

impl Applet {
    pub(super) fn timer(&self) -> Option<timer::Timer> {
        self.snapshot.timer
    }
    /// A timer is counting or paused, not finished.
    pub(super) fn timer_live(&self) -> bool {
        self.timer().is_some_and(|t| !t.done)
    }
    pub(super) fn timer_done(&self) -> bool {
        self.timer().is_some_and(|t| t.done)
    }
    /// The artwork's colour, when the player gave one.
    pub(super) fn music_tint(&self) -> Option<Color> {
        self.snapshot
            .music_tint
            .filter(|_| self.snapshot.music.is_some())
            .map(|[r, g, b]| Color::from_rgb8(r, g, b))
    }
    /// The open island shows a live view in place of the card's details.
    pub(super) fn live_panel(&self) -> bool {
        self.live()
            && !self.chooser
            && (self.ui.view != View::Card
                || (self.kind == Kind::Devices && self.devices.card)
                || (self.kind == Kind::Timer && self.timer_live())
                || (self.kind == Kind::Focus && self.focus_live()))
    }
    /// The open title line names the live view.
    pub(super) fn live_heading(&self, p: f32) -> Option<String> {
        if p < 0.5 || self.chooser || !self.live() {
            return None;
        }
        match self.ui.view {
            View::Picker => Some("New timer".into()),
            View::Outputs => Some("Output".into()),
            View::Card if self.kind == Kind::Devices && self.devices.card => Some("Devices".into()),
            // The ring holds the time, so the title says what it is.
            View::Card if self.kind == Kind::Timer && self.timer_live() => Some("Timer".into()),
            View::Card if self.kind == Kind::Focus && self.focus_live() => {
                Some(self.snapshot.focus.label.clone())
            }
            View::Card => None,
        }
    }
    /// The Music card shows a scrubber the keyboard can move.
    pub(super) fn scrubbing(&self) -> bool {
        self.kind == Kind::Music
            && self.ui.view == View::Card
            && !self.chooser
            && self.snapshot.music_seek
            && !self.snapshot.music_problem
            && self.music_progress().is_some()
    }
    /// The chooser's way to a new timer.
    pub(super) fn timer_choice<'a>(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
    ) -> Element<'a, Message> {
        widget::button::custom(
            widget::row([
                Self::icon("alarm-symbolic", 16),
                self.text("New timer", 13., alpha(palette.ink, reveal)),
            ])
            .spacing(10)
            .align_y(cosmic::iced::Alignment::Center),
        )
        .id("activity-choice-timer".into())
        .width(Length::Fill)
        .height(32)
        .padding([6, 10])
        .class(capsule_button(palette, tint, reveal, Role::Ghost))
        .on_press(Message::Live(Msg::TimerPicker))
        .into()
    }
    /// Height the live view needs beyond the card.
    pub(super) fn live_extra(&self) -> f32 {
        match self.ui.view {
            View::Card if self.kind == Kind::Devices && self.devices.card => self.devices_extra(),
            View::Card => 0.,
            View::Picker => PICKER_EXTRA,
            View::Outputs => {
                // Room for a few while the list loads, so it seldom regrows.
                let rows = if self.ui.sinks.is_empty() {
                    3
                } else {
                    self.ui.sinks.len()
                } as f32;
                (rows * (OUTPUT_ROW + OUTPUT_GAP) - OUTPUT_GAP - ROOM).max(0.)
            }
        }
    }
    /// The accent the capsule takes for the live kinds.
    pub(super) fn live_accent(&self, palette: Palette) -> Option<Color> {
        if self.ui.view == View::Picker && !self.chooser {
            return Some(palette.readable(TIMER));
        }
        match self.kind {
            Kind::Devices => self.devices_accent(palette),
            Kind::Music => self.music_tint().map(|t| palette.readable(t)),
            // The event's own calendar colour.
            Kind::Calendar => self
                .snapshot
                .calendar
                .as_ref()
                .and_then(|c| c.color)
                .map(|[r, g, b]| palette.readable(Color::from_rgb8(r, g, b))),
            Kind::Transfer
                if self
                    .ui
                    .jobs
                    .lead(Instant::now())
                    .is_some_and(|j| j.done_at.is_some()) =>
            {
                Some(palette.readable(DONE))
            }
            _ => None,
        }
    }
    /// The title line's own glyph for the live kinds.
    pub(super) fn live_glyph<'a>(
        &self,
        palette: Palette,
        accent: Color,
    ) -> Option<Element<'a, Message>> {
        if self.ui.view == View::Picker && !self.chooser {
            return Some(Self::tinted("alarm-symbolic", 16, accent));
        }
        match self.kind {
            Kind::Devices => self.devices_glyph(accent),
            Kind::Timer if self.timer_live() => {
                let t = self.timer()?;
                Some(
                    Ring {
                        size: 16.,
                        stroke: 2.5,
                        fraction: t.fraction(self.now_ms),
                        color: accent,
                        track: track(palette),
                    }
                    .into(),
                )
            }
            Kind::Music
                if self.snapshot.music.as_ref().is_some_and(|m| m.playing)
                    && !self.snapshot.music_problem =>
            {
                Some(
                    // The music icon's footprint, so the title does not move.
                    Waves {
                        size: 16.,
                        color: accent,
                    }
                    .into(),
                )
            }
            Kind::Transfer
                if self
                    .ui
                    .jobs
                    .lead(Instant::now())
                    .is_some_and(|j| j.done_at.is_some()) =>
            {
                Some(
                    widget::icon::from_name("object-select-symbolic")
                        .symbolic(true)
                        .size(16)
                        .into(),
                )
            }
            _ => None,
        }
    }
    /// The collapsed timer's title: its time in tabular figures.
    pub(super) fn live_title<'a>(&self, palette: Palette, p: f32) -> Option<Element<'a, Message>> {
        if p >= 0.5 || self.chooser {
            return None;
        }
        if self.kind == Kind::Focus && self.focus_live() {
            let f = &self.snapshot.focus;
            let left = f
                .ends_at_ms
                .filter(|_| f.running)
                .map_or(f.remaining_ms, |end| (end - self.now_ms).max(0));
            return Some(tabular(&timer_clock(left), 14., palette.ink, true));
        }
        let t = self.timer().filter(|t| !t.done)?;
        (self.kind == Kind::Timer).then(|| {
            tabular(
                &timer_clock(t.remaining(self.now_ms)),
                14.,
                palette.ink,
                true,
            )
        })
    }
    /// What follows the title: a running job's ring, with how many run.
    pub(super) fn live_trailing<'a>(
        &self,
        palette: Palette,
        accent: Color,
    ) -> Option<Element<'a, Message>> {
        if self.kind == Kind::Devices && !self.chooser {
            return self.devices_trailing(palette, accent);
        }
        if self.kind != Kind::Transfer || self.chooser {
            return None;
        }
        let job = self
            .ui
            .jobs
            .lead(Instant::now())
            .filter(|j| j.done_at.is_none())?;
        let running = self.ui.jobs.running();
        if job.indeterminate() {
            // No total, so no ring: the title's glyph says it all.
            return (running > 1).then(|| tabular(&running.to_string(), 11., palette.quiet, false));
        }
        let mut row = widget::row([Ring {
            size: 14.,
            stroke: 2.5,
            fraction: job.fraction as f32,
            color: accent,
            track: track(palette),
        }
        .into()])
        .spacing(4)
        .align_y(cosmic::iced::Alignment::Center);
        if running > 1 {
            row = row.push(tabular(&running.to_string(), 11., palette.quiet, false));
        }
        Some(row.into())
    }
    /// A badge for an activity behind the shown one: a job is its ring.
    pub(super) fn badge_icon<'a>(
        &self,
        kind: Kind,
        palette: Palette,
        size: u16,
    ) -> Element<'a, Message> {
        match kind {
            Kind::Transfer => match self.ui.jobs.lead(Instant::now()) {
                Some(job) if job.done_at.is_none() && job.indeterminate() => Self::tinted(
                    "folder-download-symbolic",
                    size,
                    palette.accent(Kind::Transfer),
                ),
                Some(job) if job.done_at.is_none() => Ring {
                    size: f32::from(size),
                    stroke: 2.,
                    fraction: job.fraction as f32,
                    color: palette.accent(Kind::Transfer),
                    track: track(palette),
                }
                .into(),
                _ => self.kind_icon(kind, size),
            },
            Kind::Timer if self.timer_live() => Ring {
                size: f32::from(size),
                stroke: 2.,
                fraction: self.timer().map_or(0., |t| t.fraction(self.now_ms)),
                color: palette.accent(Kind::Timer),
                track: track(palette),
            }
            .into(),
            _ => self.kind_icon(kind, size),
        }
    }
    pub(super) fn timer_content(&self) -> Content {
        match self.timer() {
            Some(t) if t.done => Content {
                title: "Timer done".into(),
                meta: "Time’s up".into(),
                ..Default::default()
            },
            Some(t) => Content {
                title: timer_clock(t.remaining(self.now_ms)),
                cue: if t.paused() {
                    "Paused".into()
                } else {
                    String::new()
                },
                meta: match t.ends_at_ms {
                    Some(end) => format!("Ends at {}", clock_time(end, self.format_24h)),
                    None => "Paused".into(),
                },
                progress: None,
                bar_start: 0.,
            },
            None => Content {
                title: "Timer".into(),
                meta: "Pick a length".into(),
                ..Default::default()
            },
        }
    }
    pub(super) fn transfer_content(&self) -> Content {
        let now = Instant::now();
        let Some(job) = self.ui.jobs.lead(now) else {
            return Content {
                title: "Transfers".into(),
                meta: "Nothing in progress".into(),
                ..Default::default()
            };
        };
        let more = self.ui.jobs.running().saturating_sub(1);
        let with_more = |text: String| {
            if more > 0 {
                format!("{text} · {more} more")
            } else {
                text
            }
        };
        if let Some(file) = &job.file {
            if job.done_at.is_some() {
                return Content {
                    title: "Done".into(),
                    meta: format!("{} · {}", file.name, downloads::bytes_text(file.bytes)),
                    progress: Some(1.),
                    ..Default::default()
                };
            }
            // No total is known, so no percentage and no estimate: the
            // size so far, the rate and a moving segment.
            let mut meta = downloads::bytes_text(file.bytes);
            if let Some(rate) = file.rate.filter(|_| !file.paused) {
                meta = format!("{meta} · {}/s", downloads::bytes_text(rate as u64));
            }
            return Content {
                title: format!("Downloading {}", file.name),
                cue: if file.paused {
                    "Paused".into()
                } else {
                    String::new()
                },
                meta: with_more(meta),
                progress: Some(transfers::SWEEP),
                bar_start: job.sweep(),
            };
        }
        if job.done_at.is_some() {
            return Content {
                title: "Done".into(),
                meta: job.app.clone(),
                progress: Some(1.),
                ..Default::default()
            };
        }
        let eta = job
            .remaining(now)
            .map(transfers::eta_text)
            .unwrap_or_else(|| "In progress".into());
        Content {
            title: job.app.clone(),
            cue: format!("{}%", (job.fraction * 100.).floor() as i64),
            meta: with_more(eta),
            progress: Some(job.fraction as f32),
            bar_start: 0.,
        }
    }
    fn transfer_openable(&self, job: &transfers::Job) -> bool {
        if job.file.is_some() {
            return job.done_at.is_some();
        }
        job.note
            .is_some_and(|id| self.note(id).is_some_and(|n| n.default))
            || job.desktop.is_some()
    }
    pub(super) fn transfer_actions<'a>(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
    ) -> Element<'a, Message> {
        let mut row = widget::row([])
            .spacing(4)
            .align_y(cosmic::iced::Alignment::Center);
        let Some(job) = self.ui.jobs.lead(Instant::now()) else {
            return row.into();
        };
        let done = job.done_at.is_some();
        if !done && job.file.is_some() {
            return row
                .push(self.label_button(
                    palette,
                    "activity-transfer-folder",
                    "Open folder",
                    tint,
                    reveal,
                    Role::Quiet,
                    Some(Message::Live(Msg::TransferOpenFolder)),
                ))
                .into();
        }
        if done && job.file.is_some() {
            // A finished download: its folder, with the file selected, or
            // the file itself.
            return row
                .push(self.label_button(
                    palette,
                    "activity-transfer-folder",
                    "Open folder",
                    tint,
                    reveal,
                    Role::Quiet,
                    Some(Message::Live(Msg::TransferOpenFolder)),
                ))
                .push(self.label_button(
                    palette,
                    "activity-transfer-open",
                    "Open",
                    tint,
                    reveal,
                    Role::Primary,
                    Some(Message::Live(Msg::TransferOpen)),
                ))
                .into();
        }
        if done {
            row = row.push(self.label_button(
                palette,
                "activity-transfer-dismiss",
                "Dismiss",
                tint,
                reveal,
                Role::Quiet,
                Some(Message::Live(Msg::TransferDismiss)),
            ));
        }
        if self.transfer_openable(job) {
            row = row.push(self.label_button(
                palette,
                "activity-transfer-open",
                "Open",
                tint,
                reveal,
                if done { Role::Primary } else { Role::Quiet },
                Some(Message::Live(Msg::TransferOpen)),
            ));
        }
        row.into()
    }
    pub(super) fn timer_actions<'a>(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
    ) -> Element<'a, Message> {
        let row = widget::row([])
            .spacing(4)
            .align_y(cosmic::iced::Alignment::Center);
        if self.timer_done() {
            return row
                .push(self.label_button(
                    palette,
                    "activity-timer-dismiss",
                    "Dismiss",
                    tint,
                    reveal,
                    Role::Quiet,
                    Some(Message::Live(Msg::TimerCancel)),
                ))
                .push(self.label_button(
                    palette,
                    "activity-timer-again",
                    "+1 min",
                    tint,
                    reveal,
                    Role::Primary,
                    Some(Message::Live(Msg::TimerAdd)),
                ))
                .into();
        }
        row.push(self.label_button(
            palette,
            "activity-timer-new",
            "New timer",
            tint,
            reveal,
            Role::Primary,
            Some(Message::Live(Msg::TimerPicker)),
        ))
        .into()
    }
    /// The Idle card's way to a timer.
    pub(super) fn timer_button<'a>(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
    ) -> Element<'a, Message> {
        self.tip(
            palette,
            widget::button::icon(widget::icon::from_name("alarm-symbolic").symbolic(true))
                .icon_size(16)
                .padding([8, 14])
                .width(44)
                .height(32)
                .id("activity-timer".into())
                .class(capsule_button(palette, tint, reveal, Role::Quiet))
                .on_press(Message::Live(Msg::TimerPicker)),
            "Timer",
            widget::tooltip::Position::Top,
        )
    }
    /// The Music card's way to its outputs.
    pub(super) fn outputs_button<'a>(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
    ) -> Element<'a, Message> {
        self.tip(
            palette,
            widget::button::icon(widget::icon::from_name("audio-speakers-symbolic").symbolic(true))
                .icon_size(16)
                .padding(12)
                .width(40)
                .height(40)
                .id("activity-outputs".into())
                .class(capsule_button(palette, tint, reveal, Role::Ghost))
                .on_press(Message::Live(Msg::Outputs)),
            "Output",
            widget::tooltip::Position::Top,
        )
    }
    fn round<'a>(
        &self,
        palette: Palette,
        accent: Color,
        reveal: f32,
        id: &'static str,
        content: Element<'a, Message>,
        size: f32,
        primary: bool,
        label: &str,
        press: Message,
    ) -> Element<'a, Message> {
        self.tip(
            palette,
            widget::button::custom(
                widget::container(content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(cosmic::iced::Alignment::Center)
                    .align_y(cosmic::iced::Alignment::Center),
            )
            .id(id.into())
            .width(size)
            .height(size)
            .padding(0)
            .class(round_style(palette, accent, reveal, primary))
            .on_press(press),
            label,
            widget::tooltip::Position::Top,
        )
    }
    /// Below the title while open: the picker, the outputs or the running
    /// timer.
    pub(super) fn live_view<'a>(
        &'a self,
        palette: Palette,
        accent: Color,
        reveal: f32,
    ) -> Element<'a, Message> {
        match self.ui.view {
            View::Picker => self.picker_view(palette, accent, reveal),
            View::Outputs => self.outputs_view(palette, accent, reveal),
            View::Card if self.kind == Kind::Devices => self.devices_view(palette, accent, reveal),
            View::Card if self.kind == Kind::Focus => self.focus_view(palette, accent, reveal),
            View::Card => self.timer_view(palette, accent, reveal),
        }
    }
    fn icon<'a>(name: &'static str, size: u16) -> Element<'a, Message> {
        widget::icon::from_name(name)
            .symbolic(true)
            .size(size)
            .into()
    }
    /// A symbolic glyph in a given colour: buttons do not tint drawn icons.
    fn tinted<'a>(name: &'static str, size: u16, color: Color) -> Element<'a, Message> {
        widget::icon::from_name(name)
            .symbolic(true)
            .size(size)
            .icon()
            .class(cosmic::theme::Svg::custom(move |_| {
                cosmic::iced::widget::svg::Style { color: Some(color) }
            }))
            .into()
    }
    fn picker_view<'a>(
        &'a self,
        palette: Palette,
        accent: Color,
        reveal: f32,
    ) -> Element<'a, Message> {
        let presets = timer::PRESETS.map(|minutes| {
            self.round(
                palette,
                accent,
                reveal,
                [
                    "activity-timer-1",
                    "activity-timer-5",
                    "activity-timer-10",
                    "activity-timer-25",
                ][timer::PRESETS
                    .iter()
                    .position(|m| *m == minutes)
                    .unwrap_or(0)],
                widget::column([
                    tabular(&minutes.to_string(), 15., alpha(palette.ink, reveal), true),
                    self.text("min", 10., alpha(palette.quiet, reveal)),
                ])
                .align_x(cosmic::iced::Alignment::Center)
                .into(),
                48.,
                false,
                &format!("{minutes} min"),
                Message::Live(Msg::TimerStart(minutes)),
            )
        });
        let minutes = self.ui.minutes;
        let step = |id: &'static str, icon: &'static str, label: &str, direction: i64| {
            self.round(
                palette,
                accent,
                reveal,
                id,
                Self::tinted(icon, 16, alpha(palette.ink, reveal)),
                32.,
                false,
                label,
                Message::Live(Msg::TimerStep(direction)),
            )
        };
        let custom = widget::row([
            step("activity-timer-less", "list-remove-symbolic", "Shorter", -1),
            widget::container(self.text(format!("{minutes} min"), 13., alpha(palette.ink, reveal)))
                .width(64)
                .align_x(cosmic::iced::Alignment::Center)
                .into(),
            step("activity-timer-more", "list-add-symbolic", "Longer", 1),
            widget::Space::new().width(8).into(),
            self.label_button(
                palette,
                "activity-timer-start",
                "Start",
                accent,
                reveal,
                Role::Primary,
                Some(Message::Live(Msg::TimerStart(minutes))),
            ),
        ])
        .spacing(4)
        .align_y(cosmic::iced::Alignment::Center);
        widget::column([widget::row(presets).spacing(10).into(), custom.into()])
            .spacing(10)
            .align_x(cosmic::iced::Alignment::Center)
            .into()
    }
    /// The running timer: its ring with the time inside, when it ends, and
    /// +1 min, cancel and pause as round controls.
    fn timer_view<'a>(
        &'a self,
        palette: Palette,
        accent: Color,
        reveal: f32,
    ) -> Element<'a, Message> {
        let Some(t) = self.timer().filter(|t| !t.done) else {
            return widget::Space::new().into();
        };
        let now = self.now_ms;
        let ring = ring_clock(palette, accent, reveal, t.fraction(now), t.remaining(now));
        let line = match t.ends_at_ms {
            Some(end) => format!("Ends at {}", clock_time(end, self.format_24h)),
            None => "Paused".into(),
        };
        let controls = widget::row([
            self.round(
                palette,
                accent,
                reveal,
                "activity-timer-add",
                self.text("+1", 13., alpha(palette.ink, reveal)),
                44.,
                false,
                "+1 min",
                Message::Live(Msg::TimerAdd),
            ),
            self.round(
                palette,
                accent,
                reveal,
                "activity-timer-cancel",
                Self::tinted("window-close-symbolic", 16, alpha(palette.ink, reveal)),
                44.,
                false,
                "Cancel",
                Message::Live(Msg::TimerCancel),
            ),
            if t.paused() {
                self.round(
                    palette,
                    accent,
                    reveal,
                    "activity-timer-resume",
                    Self::tinted("media-playback-start-symbolic", 20, alpha(accent, reveal)),
                    48.,
                    true,
                    "Resume",
                    Message::Live(Msg::TimerResume),
                )
            } else {
                self.round(
                    palette,
                    accent,
                    reveal,
                    "activity-timer-pause",
                    Self::tinted("media-playback-pause-symbolic", 20, alpha(accent, reveal)),
                    48.,
                    true,
                    "Pause",
                    Message::Live(Msg::TimerPause),
                )
            },
        ])
        .spacing(6)
        .align_y(cosmic::iced::Alignment::Center);
        self.ring_panel(palette, reveal, line, ring, controls.into())
    }
    /// The line under the title, then the ring at the reading edge and the
    /// controls at the far edge: the timer and focus share one layout.
    fn ring_panel<'a>(
        &self,
        palette: Palette,
        reveal: f32,
        line: String,
        ring: Element<'a, Message>,
        controls: Element<'a, Message>,
    ) -> Element<'a, Message> {
        // Floating, the line keeps the title's text column. Attached, it and
        // the ring start under the title's glyph, like every card's details.
        let (line_left, ring_left) = if self.frame().attached {
            (12, 12)
        } else {
            (24, 8)
        };
        widget::column([
            widget::container(self.text(line, 12., alpha(palette.quiet, reveal)))
                .padding([0, 12, 0, line_left])
                .into(),
            widget::row([
                widget::container(ring).padding([0, 0, 0, ring_left]).into(),
                widget::Space::new().width(Length::Fill).into(),
                controls,
            ])
            .align_y(cosmic::iced::Alignment::Center)
            .width(Length::Fill)
            .into(),
        ])
        .spacing(4)
        .width(Length::Fill)
        .into()
    }
    /// A focus or break session that is ready, counting or paused.
    pub(super) fn focus_live(&self) -> bool {
        let f = &self.snapshot.focus;
        !f.done && !(f.running && f.ends_at_ms.is_some_and(|end| end <= self.now_ms))
    }
    /// The focus session as the timer shows itself: its ring with the time
    /// inside, when it ends, and one accent control to start or pause.
    fn focus_view<'a>(
        &'a self,
        palette: Palette,
        accent: Color,
        reveal: f32,
    ) -> Element<'a, Message> {
        let f = &self.snapshot.focus;
        let now = self.now_ms;
        let end = f.ends_at_ms.filter(|_| f.running);
        let left = end.map_or(f.remaining_ms, |end| (end - now).max(0));
        let fraction = match end {
            Some(end) if f.remaining_ms > 0 => 1. - (end - now) as f32 / f.remaining_ms as f32,
            _ => 0.,
        };
        let line = match end {
            Some(end) => format!("Ends at {}", clock_time(end, self.format_24h)),
            None if f.ready => {
                if f.label == "Focus" {
                    "Ready to focus".into()
                } else {
                    "Ready for a break".into()
                }
            }
            None => "Paused".into(),
        };
        let (icon, label) = if f.running {
            ("media-playback-pause-symbolic", "Pause")
        } else if f.ready {
            ("media-playback-start-symbolic", "Start")
        } else {
            ("media-playback-start-symbolic", "Resume")
        };
        let action = self.round(
            palette,
            accent,
            reveal,
            "activity-focus-action",
            Self::tinted(icon, 20, alpha(accent, reveal)),
            48.,
            true,
            label,
            Message::Control(Command::Focus { key: f.key.clone() }),
        );
        let ring = ring_clock(palette, accent, reveal, fraction, left);
        self.ring_panel(palette, reveal, line, ring, action)
    }
    fn outputs_view<'a>(
        &'a self,
        palette: Palette,
        tint: Color,
        reveal: f32,
    ) -> Element<'a, Message> {
        let mut list = widget::column([]).spacing(OUTPUT_GAP);
        if self.ui.sinks.is_empty() {
            list = list.push(
                widget::container(self.text(
                    if self.ui.sinks_loading {
                        "Looking for outputs…"
                    } else {
                        "No outputs found"
                    },
                    13.,
                    alpha(palette.quiet, reveal),
                ))
                .height(OUTPUT_ROW)
                .padding([6, 10]),
            );
        }
        for (index, sink) in self.ui.sinks.iter().enumerate() {
            let mut row = widget::row([
                Self::icon(sink.icon, 16),
                widget::container(self.text(sink.label.clone(), 13., alpha(palette.ink, reveal)))
                    .width(Length::Fill)
                    .into(),
            ])
            .spacing(10)
            .align_y(cosmic::iced::Alignment::Center);
            if sink.default {
                row = row.push(
                    widget::icon::from_name("object-select-symbolic")
                        .symbolic(true)
                        .size(16)
                        .icon()
                        .class(cosmic::theme::Svg::custom(move |_| {
                            cosmic::iced::widget::svg::Style {
                                color: Some(alpha(tint, reveal)),
                            }
                        })),
                );
            }
            list = list.push(
                widget::button::custom(row)
                    .id(cosmic::widget::Id::new(format!("activity-output-{index}")))
                    .width(Length::Fill)
                    .height(OUTPUT_ROW)
                    .padding([6, 10])
                    .class(capsule_button(palette, tint, reveal, Role::Ghost))
                    .on_press_maybe(
                        (!sink.default).then(|| Message::Live(Msg::OutputPick(sink.clone()))),
                    ),
            );
        }
        list.width(Length::Fill).into()
    }
    /// The Music card's scrubber: elapsed, the line, and what is left.
    pub(super) fn scrubber<'a>(
        &self,
        palette: Palette,
        accent: Color,
        reveal: f32,
    ) -> Option<Element<'a, Message>> {
        if self.kind != Kind::Music || !self.snapshot.music_seek || self.snapshot.music_problem {
            return None;
        }
        let m = self.snapshot.music.as_ref()?;
        let reported = self.music_progress()?;
        let (at, total) = shown_position(
            self.ui.seek.as_ref(),
            &m.key,
            reported,
            m.playing,
            Instant::now(),
        );
        let clock = |ms: i64| {
            let s = ms.max(0) / 1000;
            if s >= 3600 {
                format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
            } else {
                format!("{}:{:02}", s / 60, s % 60)
            }
        };
        let rail = alpha(track(palette), reveal);
        let lit = alpha(accent, reveal);
        let look = move |radius: f32| {
            move |_: &cosmic::Theme| cosmic::iced::widget::slider::Style {
                rail: cosmic::iced::widget::slider::Rail {
                    backgrounds: (lit.into(), rail.into()),
                    width: 3.,
                    border: Border {
                        radius: 1.5.into(),
                        ..Default::default()
                    },
                },
                handle: cosmic::iced::widget::slider::Handle {
                    shape: cosmic::iced::widget::slider::HandleShape::Circle { radius },
                    background: lit.into(),
                    border_width: 0.,
                    border_color: Color::TRANSPARENT,
                },
                breakpoint: cosmic::iced::widget::slider::Breakpoint { color: rail },
            }
        };
        let slider = widget::slider(0.0..=total as f64, at as f64, |v| {
            Message::Live(Msg::Seek(v))
        })
        .on_release(Message::Live(Msg::SeekRelease))
        .step(1_000.)
        .height(15.)
        .class(cosmic::theme::iced::Slider::Custom {
            active: std::rc::Rc::new(look(4.)),
            hovered: std::rc::Rc::new(look(5.5)),
            dragging: std::rc::Rc::new(look(6.)),
        });
        let quiet = alpha(palette.quiet, reveal);
        Some(
            widget::row([
                widget::container(tabular(&clock(at), 10., quiet, false))
                    .width(38)
                    .into(),
                widget::container(slider).width(Length::Fill).into(),
                widget::container(tabular(
                    &format!("-{}", clock(total - at)),
                    10.,
                    quiet,
                    false,
                ))
                .width(42)
                .align_x(cosmic::iced::Alignment::End)
                .into(),
            ])
            .spacing(6)
            .align_y(cosmic::iced::Alignment::Center)
            .into(),
        )
    }
    /// Grows the open island to fit the live view: a popup's size is fixed
    /// when it opens, so it reopens in hand when it needs more room.
    pub(super) fn regrow(&mut self, now: Instant) -> Task<Message> {
        self.reveal_deadline = None;
        self.revealed = false;
        self.hover_deadline = None;
        self.leave_deadline = None;
        self.hover_block = None;
        self.chooser = false;
        self.list = false;
        self.problem = None;
        self.now_ms = chrono::Utc::now().timestamp_millis();
        let Some(id) = self.popup else {
            self.held = true;
            self.paused_until = None;
            return self.open_popup_sized();
        };
        if !self.held || self.popup_extra + 0.5 < self.live_extra() {
            self.held = true;
            self.promote_popup = Some(id);
            return self.update(Message::Surface(cosmic::surface::action::destroy_popup(id)));
        }
        if self.motion.target() == 0. {
            self.motion.retarget(1., self.animates(), now);
        }
        Task::none()
    }
    fn send(&mut self, command: Command) -> Task<Message> {
        self.update(Message::Control(command))
    }
    pub(super) fn live_update(&mut self, msg: Msg, now: Instant) -> Task<Message> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        match msg {
            Msg::TimerPicker => {
                self.ui.view = View::Picker;
                self.regrow(now)
            }
            Msg::TimerStart(minutes) => {
                let ms = minutes.clamp(1, timer::MAX_MS / 60_000) * 60_000;
                self.snapshot.timer = Some(timer::Timer::start(ms, now_ms));
                self.ui.view = View::Card;
                self.kind = Kind::Timer;
                self.now_ms = now_ms;
                self.send(Command::TimerStart { ms })
            }
            Msg::TimerStep(direction) => {
                self.ui.minutes = step_minutes(self.ui.minutes, direction);
                Task::none()
            }
            Msg::TimerAdd => {
                if let Some(t) = &mut self.snapshot.timer {
                    t.add(60_000, now_ms);
                }
                self.kind = Kind::Timer;
                self.reveal_deadline = None;
                self.send(Command::TimerAdd { ms: 60_000 })
            }
            Msg::TimerPause => {
                if let Some(t) = &mut self.snapshot.timer {
                    t.pause(now_ms);
                }
                self.send(Command::TimerPause)
            }
            Msg::TimerResume => {
                if let Some(t) = &mut self.snapshot.timer {
                    t.resume(now_ms);
                }
                self.send(Command::TimerResume)
            }
            Msg::TimerCancel => {
                self.snapshot.timer = None;
                let sent = self.send(Command::TimerCancel);
                Task::batch([sent, self.update(Message::Close)])
            }
            Msg::Outputs => {
                if self.ui.view == View::Outputs {
                    self.ui.view = View::Card;
                    return Task::none();
                }
                self.ui.view = View::Outputs;
                self.ui.sinks_loading = true;
                let grow = self.regrow(now);
                let load = cosmic::task::future(async {
                    Message::Live(Msg::OutputsLoaded(outputs::list().await))
                });
                Task::batch([grow, load])
            }
            Msg::OutputsLoaded(result) => {
                self.ui.sinks_loading = false;
                match result {
                    Ok(sinks) => self.ui.sinks = sinks,
                    Err(problem) => self.problem = Some(problem),
                }
                if self.ui.view == View::Outputs && self.popup.is_some() {
                    return self.regrow(now);
                }
                Task::none()
            }
            Msg::OutputPick(sink) => {
                for s in &mut self.ui.sinks {
                    s.default = s.name == sink.name;
                }
                // Back to the card: the choice is made.
                self.ui.view = View::Card;
                cosmic::task::future(async move {
                    Message::Live(Msg::OutputsLoaded(outputs::select(sink).await))
                })
            }
            Msg::Seek(value) => {
                let Some(m) = self.snapshot.music.as_ref() else {
                    return Task::none();
                };
                self.ui.seek = Some(Hold {
                    key: m.key.clone(),
                    position_ms: value as i64,
                    at: now,
                    dragging: true,
                });
                Task::none()
            }
            Msg::SeekRelease => {
                let Some(hold) = self.ui.seek.as_mut().filter(|h| h.dragging) else {
                    return Task::none();
                };
                hold.dragging = false;
                hold.at = now;
                let command = Command::Seek {
                    key: hold.key.clone(),
                    position_ms: hold.position_ms,
                };
                self.send(command)
            }
            Msg::SeekBy(delta) => {
                let Some(m) = self.snapshot.music.as_ref() else {
                    return Task::none();
                };
                let Some(reported) = self.music_progress() else {
                    return Task::none();
                };
                let key = m.key.clone();
                let (at, total) =
                    shown_position(self.ui.seek.as_ref(), &key, reported, m.playing, now);
                let position_ms = (at + delta).clamp(0, (total - 1_000).max(0));
                self.ui.seek = Some(Hold {
                    key: key.clone(),
                    position_ms,
                    at: now,
                    dragging: false,
                });
                self.send(Command::Seek { key, position_ms })
            }
            Msg::Transfer(update) => self.transfer_update(update, now),
            Msg::TransferOpenFolder => {
                // Running, it selects the temporary file.
                let Some(path) = self
                    .ui
                    .jobs
                    .lead(now)
                    .and_then(|j| j.file.as_ref())
                    .map(|f| f.path.clone())
                else {
                    return Task::none();
                };
                self.ui.jobs.dismiss_done();
                let close = self.update(Message::Close);
                let show = cosmic::task::future(async move {
                    Message::SettingsOpened(downloads::show_in_folder(path).await)
                });
                Task::batch([close, show])
            }
            Msg::TransferOpen => {
                let Some(job) = self.ui.jobs.lead(now).cloned() else {
                    return Task::none();
                };
                self.ui.jobs.dismiss_done();
                if let Some(path) = job.file.filter(|_| job.done_at.is_some()).map(|f| f.path) {
                    let close = self.update(Message::Close);
                    let open = cosmic::task::future(async move {
                        Message::SettingsOpened(downloads::open_path(path).await)
                    });
                    return Task::batch([close, open]);
                }
                if let Some(id) = job
                    .note
                    .filter(|id| self.note(*id).is_some_and(|n| n.default))
                {
                    return self.update(Message::NoteAction(id, "default".into()));
                }
                let Some(desktop) = job.desktop else {
                    return Task::none();
                };
                let close = self.update(Message::Close);
                let launch = cosmic::task::future(async move {
                    let id = desktop
                        .strip_suffix(".desktop")
                        .unwrap_or(&desktop)
                        .to_owned();
                    let launched = tokio::process::Command::new("gtk-launch")
                        .arg(id)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()
                        .is_ok();
                    Message::SettingsOpened(if launched {
                        Ok(())
                    } else {
                        Err("Could not open the app".into())
                    })
                });
                Task::batch([close, launch])
            }
            Msg::TransferDismiss => {
                self.ui.jobs.dismiss_done();
                self.update(Message::Close)
            }
            Msg::Back => {
                self.ui.view = View::Card;
                Task::none()
            }
        }
    }
    /// A job's progress from an app's launcher entry or a notification.
    pub(super) fn transfer_update(
        &mut self,
        update: transfers::Update,
        now: Instant,
    ) -> Task<Message> {
        let rule = self.owners_rule(&update.owners);
        if rule == Some(preferences::AppRule::Mute) {
            // A muted app's jobs are not shown at all.
            let key = update.key;
            if self.ui.jobs.forget(|j| j.key == key) {
                return self.after_transfer(false, None, now);
            }
            return Task::none();
        }
        let finished = self.ui.jobs.update(update, now);
        self.after_transfer(finished, rule, now)
    }
    /// The first of an app's names that has a rule decides for its jobs.
    pub(super) fn owners_rule(&self, owners: &[(String, String)]) -> Option<preferences::AppRule> {
        let prefs = &self.snapshot.preferences;
        owners
            .iter()
            .find_map(|(app, entry)| prefs.rule_for(app, entry))
    }
    pub(super) fn after_transfer(
        &mut self,
        finished: bool,
        rule: Option<preferences::AppRule>,
        now: Instant,
    ) -> Task<Message> {
        if self.popup.is_none() {
            self.kind = self.automatic_kind();
        }
        if finished {
            return self.reveal_done(rule, now);
        }
        Task::none()
    }
    /// Done is shown once, as a notification's arrival would be: never for
    /// an app set to List only, always for one set to Always show, and
    /// otherwise when reveals are on. A job's progress never reveals.
    fn reveal_done(&mut self, rule: Option<preferences::AppRule>, now: Instant) -> Task<Message> {
        let allowed = match rule {
            Some(preferences::AppRule::Always) => true,
            Some(_) => false,
            None => self.snapshot.preferences.reveals,
        };
        if !allowed
            || !self.eligible_kinds().contains(&Kind::Transfer)
            || self.popup.is_some()
            || self.hovered
            || !self.host_reveals
            || self.snapshot.quiet != Some(false)
            || self.snapshot.dnd
            || !self.live()
            || self
                .snapshot
                .system
                .iter()
                .any(|a| a.urgent && self.system_activity(a.source).is_some())
        {
            return Task::none();
        }
        self.kind = Kind::Transfer;
        self.last_reveal = Some(now);
        self.reveal_deadline = Some(now + Duration::from_secs(3));
        self.revealed = true;
        self.open_popup()
    }
    fn note_job_closed(&mut self, id: u32, now: Instant) -> Task<Message> {
        let rule = self
            .ui
            .jobs
            .for_note(id)
            .map(|j| j.owners.clone())
            .and_then(|o| self.owners_rule(&o));
        let finished = self.ui.jobs.closed(id, now);
        self.after_transfer(finished, rule, now)
    }
    /// Notifications that carry a percentage are jobs too.
    pub(super) fn note_transfer(
        &mut self,
        event: &notifications::Event,
        now: Instant,
    ) -> Task<Message> {
        match event {
            notifications::Event::Notify(note) => {
                if self.live() && !self.snapshot.preferences.notifications {
                    return Task::none();
                }
                let Some(progress) = note.progress else {
                    // The value dropped from a followed job: it ended.
                    return self.note_job_closed(note.id, now);
                };
                let update = transfers::note_update(note, progress);
                self.transfer_update(update, now)
            }
            notifications::Event::Closed(id) => self.note_job_closed(*id, now),
            _ => Task::none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_timer_clock_and_custom_steps() {
        assert_eq!(timer_clock(0), "0:00");
        assert_eq!(timer_clock(59_001), "1:00");
        assert_eq!(timer_clock(25 * 60_000), "25:00");
        assert_eq!(timer_clock(3_600_000 + 61_000), "1:01:01");
        assert_eq!(step_minutes(9, 1), 10);
        assert_eq!(step_minutes(10, 1), 15);
        assert_eq!(step_minutes(10, -1), 9);
        assert_eq!(step_minutes(15, -1), 10);
        assert_eq!(step_minutes(60, 1), 75);
        assert_eq!(step_minutes(60, -1), 55);
        assert_eq!(step_minutes(1, -1), 1);
        assert_eq!(step_minutes(1440, 1), 1440);
    }
    #[test]
    fn live_seek_holds_the_dragged_place_until_the_player_catches_up() {
        let t = Instant::now();
        let reported = (10_000, 200_000);
        assert_eq!(shown_position(None, "a", reported, true, t), reported);
        let mut hold = Hold {
            key: "a".into(),
            position_ms: 90_000,
            at: t,
            dragging: true,
        };
        assert_eq!(
            shown_position(Some(&hold), "a", reported, true, t),
            (90_000, 200_000)
        );
        assert_eq!(
            shown_position(Some(&hold), "b", reported, true, t),
            reported,
            "another track ignores the hold"
        );
        hold.dragging = false;
        let later = t + Duration::from_millis(500);
        // Released, the player still reports the old place: the new one
        // keeps moving on its own while it plays.
        assert_eq!(
            shown_position(Some(&hold), "a", reported, true, later),
            (90_500, 200_000)
        );
        assert_eq!(
            shown_position(Some(&hold), "a", reported, false, later),
            (90_000, 200_000)
        );
        // The player reports near it: its own place again.
        assert_eq!(
            shown_position(Some(&hold), "a", (91_000, 200_000), true, later),
            (91_000, 200_000)
        );
        // Never longer than the settle time.
        assert_eq!(
            shown_position(Some(&hold), "a", reported, true, t + SEEK_SETTLE),
            reported
        );
        hold.dragging = true;
        hold.position_ms = 999_999;
        assert_eq!(
            shown_position(Some(&hold), "a", reported, true, t),
            (200_000, 200_000)
        );
    }
}
