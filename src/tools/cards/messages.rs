//! The Messages card: the phone's newest conversations through KDE Connect,
//! one row each, and a plain next step while the phone isn't ready.
use super::{Action, card_title, frame, label, line, mix};
use crate::config::{Config, Palette};
use crate::drawer::Widget;
use crate::messages::{State, Thread};
use cosmic::Element;
use cosmic::iced::{Alignment, Border, Color, Length};
use cosmic::widget::{column, container, row};

const GAP: f32 = 8.;
const LINE: f32 = 1.25;
const TITLE_HEIGHT: f32 = 24.;
/// A name a little larger than its message.
const NAME: f32 = crate::widget_style::SECONDARY + 1.;
const DOT: f32 = 7.;
/// The room a row takes: its name and its message.
const ROW: f32 = (NAME + crate::widget_style::SECONDARY) * LINE + 2.;

/// How many rows fit a card this tall.
pub(crate) fn rows_fitting(height: f32) -> usize {
    let pad = 2. * f32::from(super::PADDING);
    let room = height - pad - TITLE_HEIGHT - GAP;
    (((room + GAP) / (ROW + GAP)).floor().max(1.)) as usize
}

/// When a message came: the time today, else "Yesterday", the weekday this
/// week, or the date.
pub(crate) fn when(at_ms: i64, now: chrono::DateTime<chrono::Local>, format_24h: bool) -> String {
    use chrono::{Datelike, TimeZone};
    let Some(at) = chrono::Local.timestamp_millis_opt(at_ms).single() else {
        return String::new();
    };
    let days = (now.date_naive() - at.date_naive()).num_days();
    match days {
        ..=0 => crate::config::time_of_day(&at, format_24h),
        1 => "Yesterday".into(),
        2..=6 => at.format("%a").to_string(),
        _ if at.year() == now.year() => at.format("%b %-d").to_string(),
        _ => at.format("%b %-d, %Y").to_string(),
    }
}

/// The message line: its text, or what it was when there is none or the
/// text is hidden. "You:" marks a message sent from the phone.
pub(crate) fn preview(thread: &Thread, show_text: bool) -> String {
    let what = if !show_text {
        if thread.unread {
            "New message".to_string()
        } else {
            "Message".to_string()
        }
    } else if thread.body.is_empty() {
        if thread.attachment {
            "Picture or file".into()
        } else {
            "Message".into()
        }
    } else {
        thread.body.clone()
    };
    if thread.mine && show_text {
        format!("You: {what}")
    } else {
        what
    }
}

/// The card's next step for a phone that isn't ready, with a line saying
/// how to get there.
pub(crate) fn status(state: &State) -> Option<(String, &'static str)> {
    Some(match state {
        State::Starting => ("Looking for your phone".into(), ""),
        State::NoService => (
            "Connect your phone".into(),
            "Install KDE Connect on this computer and your phone, then pair them",
        ),
        State::Stopped => (
            "KDE Connect isn't running".into(),
            "Open KDE Connect to start it, then pair your phone",
        ),
        State::NoPhone => (
            "Pair your phone".into(),
            "Open KDE Connect on both and pair this computer with your phone",
        ),
        State::NoSms { phone } => (
            format!("Messages are off on {phone}"),
            "In KDE Connect on your phone, turn on SMS and allow it to read messages",
        ),
        State::Asking { phone } => (format!("Asking {phone} for messages"), ""),
        State::Away { phone, threads } if threads.is_empty() => (
            format!("{phone} isn't connected"),
            "Messages show when your phone and this computer are on the same network",
        ),
        State::Ready { threads, .. } if threads.is_empty() => ("No messages yet".into(), ""),
        State::Away { .. } | State::Ready { .. } => return None,
    })
}

fn dot<'a, M: 'a>(color: Option<Color>) -> Element<'a, M> {
    container(cosmic::iced::widget::space())
        .width(Length::Fixed(DOT))
        .height(Length::Fixed(DOT))
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: color.map(Into::into),
                border: Border {
                    radius: (DOT / 2.).into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

fn thread_row<'a, M: 'a>(
    thread: &Thread,
    p: Palette,
    show_text: bool,
    now: chrono::DateTime<chrono::Local>,
    format_24h: bool,
) -> Element<'a, M> {
    let name = cosmic::widget::text::body(thread.who.clone())
        .size(NAME)
        .line_height(LINE)
        .font(cosmic::iced::Font {
            weight: if thread.unread {
                cosmic::iced::font::Weight::Bold
            } else {
                cosmic::iced::font::Weight::Medium
            },
            ..super::font()
        })
        .wrapping(cosmic::iced::advanced::text::Wrapping::None)
        .ellipsize(cosmic::iced::advanced::text::Ellipsize::End(
            cosmic::iced::advanced::text::EllipsizeHeightLimit::Lines(1),
        ))
        .width(Length::Fill)
        .class(cosmic::theme::Text::Color(crate::widget_style::semantic(
            p.foreground,
        )));
    let top = row::with_children(vec![
        dot(thread
            .unread
            .then_some(crate::widget_style::semantic(p.accent))),
        name.into(),
        label(
            when(thread.at_ms, now, format_24h),
            crate::widget_style::META,
            if thread.unread { p.accent } else { p.muted },
        ),
    ])
    .spacing(6)
    .align_y(Alignment::Center);
    let message = row::with_children(vec![
        container(cosmic::iced::widget::space())
            .width(Length::Fixed(DOT))
            .into(),
        line(
            preview(thread, show_text),
            crate::widget_style::SECONDARY,
            if thread.unread { p.foreground } else { p.muted },
        ),
    ])
    .spacing(6);
    column::with_children(vec![top.into(), message.into()])
        .spacing(2)
        .width(Length::Fill)
        .into()
}

/// The Messages card. The newest conversations fill it; more than fit say
/// how many are unread in the header. A click opens the conversations, or
/// KDE Connect while the phone isn't ready.
pub fn messages<'a, M: Clone + 'a>(
    state: &State,
    config: &Config,
    on_action: Option<fn(Action) -> M>,
    dimensions: Option<[f32; 2]>,
) -> Element<'a, M> {
    let _style = crate::widget_style::scope(config);
    let p = config.palette();
    let [_, height] = dimensions.unwrap_or_else(|| super::size(Widget::Messages, config));
    let show_text = config.messages.show_text;
    let mut header: Vec<Element<'a, M>> = vec![
        container(card_title("Messages", p))
            .width(Length::Fill)
            .into(),
    ];
    match (state, state.phone()) {
        (State::Away { threads, .. }, Some(phone)) if !threads.is_empty() => header.push(label(
            format!("{phone} away"),
            crate::widget_style::META,
            p.muted,
        )),
        _ => {
            let unread = state.unread();
            if unread > 0 {
                header.push(label(
                    format!("{unread} unread"),
                    crate::widget_style::SECONDARY,
                    p.accent,
                ));
            }
        }
    }
    let body: Element<'a, M> = match status(state) {
        Some((title, hint)) => {
            let mut lines = vec![label(title, crate::widget_style::SECONDARY, p.muted)];
            if !hint.is_empty() {
                lines.push(
                    cosmic::widget::text::body(hint)
                        .size(crate::widget_style::META)
                        .line_height(LINE)
                        .font(super::font())
                        .align_x(cosmic::iced::alignment::Horizontal::Center)
                        .class(cosmic::theme::Text::Color(crate::widget_style::semantic(
                            p.muted,
                        )))
                        .into(),
                );
            }
            container(
                column::with_children(lines)
                    .spacing(4)
                    .align_x(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
        }
        None => {
            let now = chrono::Local::now();
            let format_24h = config.clock.format_24h;
            let away = matches!(state, State::Away { .. });
            let p = if away {
                // Read earlier: quieter, so it doesn't pass for live.
                Palette {
                    foreground: mix(p.foreground, p.background, 0.35),
                    accent: mix(p.accent, p.background, 0.35),
                    muted: mix(p.muted, p.background, 0.35),
                    ..p
                }
            } else {
                p
            };
            let total = state.threads().len();
            let overflow = total > rows_fitting(height);
            let visible = rows_fitting(height - if overflow { 22. } else { 0. });
            let rows = state
                .threads()
                .iter()
                .take(visible)
                .map(|thread| thread_row(thread, p, show_text, now, format_24h))
                .collect::<Vec<_>>();
            let mut list = column::with_children(rows).spacing(GAP);
            if total > visible {
                list = list.push(label(
                    format!("{} more in KDE Connect", total - visible),
                    crate::widget_style::META,
                    p.muted,
                ));
            }
            container(list)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    };
    let content: Element<'a, M> = column::with_children(vec![
        row::with_children(header)
            .spacing(8)
            .align_y(Alignment::Center)
            .height(Length::Fixed(TITLE_HEIGHT))
            .into(),
        body,
    ])
    .spacing(GAP)
    .height(Length::Fill)
    .into();
    let content = match on_action {
        Some(on_action) => cosmic::widget::mouse_area(content)
            .on_press(on_action(Action::MessagesOpen))
            .interaction(cosmic::iced::mouse::Interaction::Pointer)
            .into(),
        None => content,
    };
    frame(content, config, height)
}

/// Example conversations, for previews.
pub fn sample() -> State {
    crate::messages::sample()
}
