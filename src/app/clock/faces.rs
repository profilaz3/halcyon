//! The typeset faces beyond Digital: Stacked (hours over minutes), Words
//! ("half past nine") and Split (the date beside the time). Every line is
//! native text in the clock's own font and ink, with the same soft edge as
//! the digital time, so search and screen readers find the time.
use super::*;
use crate::widget_style::resolver::{HOUR_WORDS, PHRASES};

/// A Stacked line, as a share of the face size: the digits sit close.
const STACK_LINE: f32 = 0.92;
/// A Words line, as a share of the face size.
const WORD_LINE: f32 = 1.12;
/// Split's date lines, as a share of the face size.
const SPLIT_DATE: f32 = 0.24;
/// The space either side of Split's rule, as a share of the face size.
const SPLIT_GAP: f32 = 0.22;

/// How a face's height, date and starting size follow its size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Metrics {
    /// The face's height per unit of size.
    pub em: f32,
    /// The date's size below (or beside) the face per unit of size.
    pub date: f32,
    /// The size the face starts from, as a share of the digital time's.
    pub requested: f32,
}
pub(super) fn metrics(face: ClockFace) -> Metrics {
    match face {
        ClockFace::Stacked => Metrics {
            em: 2. * STACK_LINE,
            date: 20. / 104.,
            requested: 1.,
        },
        ClockFace::Words => Metrics {
            em: 2. * WORD_LINE,
            date: 0.4,
            requested: 0.5,
        },
        ClockFace::Split => Metrics {
            em: 1.05,
            date: SPLIT_DATE,
            requested: 1.,
        },
        _ => Metrics {
            em: 1.05,
            date: 20. / 104.,
            requested: 1.,
        },
    }
}

/// The widest thing a Words face can say, at 100 pixels in `font`. Words
/// are fitted to this, not to the current time, so the size holds still as
/// the minutes pass. Measured once per font.
fn widest_words(font: Font) -> f32 {
    thread_local! {
        static WIDEST: std::cell::RefCell<Vec<(Font, f32)>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    WIDEST.with_borrow_mut(|known| {
        if let Some((_, width)) = known.iter().find(|(f, _)| *f == font) {
            return *width;
        }
        let width = PHRASES
            .iter()
            .chain(HOUR_WORDS.iter())
            .chain(["o’clock"].iter())
            .map(|line| measure(line, 100., font))
            .fold(0., f32::max);
        if known.len() >= 16 {
            known.clear();
        }
        known.push((font, width));
        width
    })
}

/// The widest weekday and month-day pair Split can show, at `size`.
fn widest_date(size: f32, font: Font) -> f32 {
    ["Wednesday", "September 30"]
        .iter()
        .map(|line| measure(line, size, font))
        .fold(0., f32::max)
}

/// The size `face` fits `width` at, from `requested`, before height.
pub(super) fn fit(p: &Presentation, face: ClockFace, width: f32, requested: f32) -> f32 {
    let display = p.style.display.value;
    let body = p.style.body.value;
    let width = (width - 2.).max(1.);
    let period = |size: f32| {
        if p.preferences.format_24h {
            0.
        } else {
            6. + measure("PM", period_size(size), body)
        }
    };
    let needed = match face {
        ClockFace::Stacked => measure("00", requested, display) + period(requested),
        ClockFace::Words => widest_words(display) * requested / 100.,
        ClockFace::Split => {
            let time = measure("00:00", requested, display) + period(requested);
            if p.preferences.show_date {
                time + requested * SPLIT_GAP * 2. + 1. + widest_date(requested * SPLIT_DATE, body)
            } else {
                time
            }
        }
        _ => measure("00:00", requested, display),
    };
    (requested * (width / needed.max(1.)).min(1.)).max(1.)
}

/// One line of the face in the display font, with the soft edge.
fn line(
    p: &Presentation,
    value: String,
    size: f32,
    height: f32,
    color: Color,
) -> Element<'static, Message> {
    let font = p.style.display.value;
    haloed(
        move |color| {
            text::body(value.clone())
                .size(size)
                .line_height(height)
                .font(font)
                .wrapping(advanced_text::Wrapping::None)
                .class(cosmic::theme::Text::Color(color))
                .into()
        },
        color,
        p.halo,
        size,
    )
}

/// AM or PM beside the last line, small, at its foot.
fn period(
    p: &Presentation,
    reading: &ClockReading,
    size: f32,
) -> Option<Element<'static, Message>> {
    let period = reading.period.clone()?;
    Some(
        container(clock_label(
            period,
            period_size(size),
            p.body_ink,
            p.style.body.value,
            p.halo,
        ))
        .padding([0., 0., size * 0.1, 0.])
        .into(),
    )
}

/// Lines on top of each other in the face's box, aligned as the clock is.
fn lines(
    p: &Presentation,
    rows: Vec<Element<'static, Message>>,
    width: f32,
    height: f32,
) -> Element<'static, Message> {
    container(
        column::with_children(rows).align_x(match p.preferences.alignment {
            ClockAlignment::Left => Alignment::Start,
            ClockAlignment::Center => Alignment::Center,
            ClockAlignment::Right => Alignment::End,
        }),
    )
    .width(width)
    .height(height)
    .align_x(p.preferences.alignment.layout())
    .into()
}

/// Hours over minutes, large: the hours in the ink, the minutes in the
/// accent, AM or PM small beside the minutes.
pub(super) fn stacked(
    p: &Presentation,
    reading: &ClockReading,
    width: f32,
    size: f32,
) -> Element<'static, Message> {
    let palette = p.style.palette();
    let mut minutes = vec![line(
        p,
        reading.minutes.clone(),
        size,
        STACK_LINE,
        palette.accent,
    )];
    minutes.extend(period(p, reading, size));
    lines(
        p,
        vec![
            line(
                p,
                reading.hours.clone(),
                size,
                STACK_LINE,
                palette.foreground,
            ),
            row::with_children(minutes)
                .spacing(6)
                .align_y(Alignment::End)
                .into(),
        ],
        width,
        size * 2. * STACK_LINE,
    )
}

/// The time in words on two lines: the hour in the ink, what goes with it
/// ("half past", "o’clock") quieter.
pub(super) fn words(
    p: &Presentation,
    reading: &ClockReading,
    width: f32,
    size: f32,
) -> Element<'static, Message> {
    let ink = p.style.palette().foreground;
    let rows = reading
        .words
        .iter()
        .map(|words| {
            let hour = HOUR_WORDS.contains(&words.as_str());
            line(
                p,
                words.clone(),
                size,
                WORD_LINE,
                if hour { ink } else { p.body_ink },
            )
        })
        .collect();
    lines(p, rows, width, size * 2. * WORD_LINE)
}

/// The time, a thin rule, and the weekday over the day of the month.
pub(super) fn split(
    p: &Presentation,
    reading: &ClockReading,
    width: f32,
    size: f32,
) -> Element<'static, Message> {
    let palette = p.style.palette();
    let mut time = vec![line(
        p,
        reading.time.clone(),
        size,
        1.05,
        palette.foreground,
    )];
    time.extend(period(p, reading, size));
    let mut parts: Vec<Element<'static, Message>> = vec![
        row::with_children(time)
            .spacing(6)
            .align_y(Alignment::End)
            .into(),
    ];
    if p.preferences.show_date
        && let Some([weekday, day]) = reading.date.clone()
    {
        let rule = Color {
            a: 0.55,
            ..p.body_ink
        };
        parts.push(
            container(cosmic::iced::widget::space())
                .width(1.)
                .height(size * 0.72)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        background: Some(rule.into()),
                        ..Default::default()
                    }
                }))
                .into(),
        );
        let date = size * SPLIT_DATE;
        parts.push(
            column::with_children(vec![
                clock_label(
                    weekday,
                    date,
                    palette.foreground,
                    p.style.body.value,
                    p.halo,
                ),
                clock_label(day, date, p.body_ink, p.style.body.value, p.halo),
            ])
            .into(),
        );
    }
    container(
        row::with_children(parts)
            .spacing(size * SPLIT_GAP)
            .align_y(Alignment::Center),
    )
    .width(width)
    .height(size * 1.05)
    .align_x(p.preferences.alignment.layout())
    .align_y(Alignment::Center)
    .into()
}
