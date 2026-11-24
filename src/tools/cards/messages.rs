//! The Messages card: the phone's newest conversations through KDE Connect,
//! one row each, and a plain next step while the phone isn't ready.
use super::{Action, card_title, frame, label, line, mix};
use crate::config::{Config, Palette};
use crate::drawer::Widget;
use crate::messages::{State, Thread};
use cosmic::Element;
use cosmic::iced::{Alignment, Border, Color, Length};
use cosmic::widget::{column, container, icon, row};
use unicode_segmentation::UnicodeSegmentation;

const GAP: f32 = 8.;
const LINE: f32 = 1.25;
const TITLE_HEIGHT: f32 = 24.;
/// A name a little larger than its message.
const NAME: f32 = crate::widget_style::SECONDARY + 2.;
const AVATAR: f32 = 34.;
const PRIVATE_ROW: f32 = 22.;
const ROW_GAP: f32 = 9.;
const DOT: f32 = 7.;
/// The room a row takes: its name and its message.
const ROW: f32 = (NAME + crate::widget_style::SECONDARY) * LINE + 2.;

/// How many rows fit a card this tall.
pub(crate) fn rows_fitting(height: f32, show_text: bool) -> usize {
    let pad = 2. * f32::from(super::PADDING);
    let room = height - pad - TITLE_HEIGHT - GAP;
    let row = if show_text {
        ROW.max(AVATAR)
    } else {
        PRIVATE_ROW
    };
    ((room + ROW_GAP) / (row + ROW_GAP)).floor().max(0.) as usize
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

/// A visible preview, including the sender or attachment hint when appropriate.
pub(crate) fn preview(thread: &Thread) -> String {
    let what = if thread.body.is_empty() {
        if thread.attachment {
            "Picture or file".into()
        } else {
            "Message".into()
        }
    } else {
        thread.body.clone()
    };
    if thread.mine {
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
        State::Stale { threads, .. } if threads.is_empty() => (
            "Couldn’t refresh messages".into(),
            "Halcyon will try KDE Connect again",
        ),
        State::Ready { threads, .. } if threads.is_empty() => ("No messages yet".into(), ""),
        State::Away { .. } | State::Stale { .. } | State::Ready { .. } => return None,
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

fn medallion<'a, M: 'a>(content: Element<'a, M>, ink: Color, size: f32) -> Element<'a, M> {
    container(content)
        .width(size)
        .height(size)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Color { a: 0.14, ..ink }.into()),
                icon_color: Some(ink),
                border: Border {
                    radius: (size / 2.).into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

fn contact_mark<'a, M: 'a>(thread: &Thread, p: Palette) -> Element<'a, M> {
    let who = thread.who.as_str();
    let ink = crate::widget_style::semantic(if thread.unread { p.accent } else { p.muted });
    let mut words = who.split_whitespace();
    let first = words.next().unwrap_or_default();
    let content = if who.contains(", ") {
        icon::from_name("system-users-symbolic")
            .size(16)
            .icon()
            .into()
    } else if who == "Unknown" || !first.starts_with(char::is_alphabetic) {
        icon::from_name("avatar-default-symbolic")
            .size(16)
            .icon()
            .into()
    } else {
        let initials = [Some(first), words.next_back()]
            .into_iter()
            .flatten()
            .filter_map(|word| word.graphemes(true).next())
            .collect::<String>();
        line(initials, NAME, ink)
    };
    let mark = medallion(content, ink, AVATAR);
    if !thread.unread {
        return mark;
    }
    let unread = container(dot(Some(ink)))
        .width(AVATAR)
        .height(AVATAR)
        .align_x(Alignment::End)
        .align_y(Alignment::End);
    cosmic::iced::widget::Stack::new()
        .push(mark)
        .push(unread)
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
        name.into(),
        label(
            when(thread.at_ms, now, format_24h),
            crate::widget_style::META + 1.,
            if thread.unread { p.accent } else { p.muted },
        ),
    ])
    .spacing(6)
    .align_y(Alignment::Center)
    .width(Length::Fill);
    if !show_text {
        return row::with_children(vec![
            dot(thread
                .unread
                .then_some(crate::widget_style::semantic(p.accent))),
            top.into(),
        ])
        .spacing(GAP)
        .align_y(Alignment::Center)
        .height(PRIVATE_ROW)
        .into();
    }
    let details = column::with_children(vec![
        top.into(),
        line(
            preview(thread),
            crate::widget_style::SECONDARY,
            if thread.unread { p.foreground } else { p.muted },
        ),
    ])
    .spacing(2)
    .width(Length::Fill);
    row::with_children(vec![contact_mark(thread, p), details.into()])
        .spacing(GAP)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into()
}

/// The newest conversations, with a count of those that did not fit.
/// A click opens the conversations, or KDE Connect during setup.
pub fn messages<'a, M: Clone + 'a>(
    state: &State,
    config: &Config,
    on_action: Option<fn(Action) -> M>,
    dimensions: Option<[f32; 2]>,
) -> Element<'a, M> {
    let _style = crate::widget_style::scope(config);
    let p = config.palette();
    let [width, height] = dimensions.unwrap_or_else(|| super::size(Widget::Messages, config));
    let show_text = config.messages.show_text;
    let mut header: Vec<Element<'a, M>> = vec![
        container(card_title("Messages", p))
            .width(Length::Fill)
            .into(),
    ];
    match (state, state.phone()) {
        (State::Stale { threads, .. }, _) if !threads.is_empty() => header.push(label(
            "Last known messages",
            crate::widget_style::META,
            p.muted,
        )),
        (State::Away { threads, .. }, Some(phone)) if !threads.is_empty() => header.push(
            container(line(
                format!("Away · {phone}"),
                crate::widget_style::META,
                p.muted,
            ))
            .max_width((width - 2. * f32::from(super::PADDING)) * 0.55)
            .into(),
        ),
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
            let glyph = if matches!(state, State::Ready { .. }) {
                "chat-symbolic"
            } else {
                "smartphone-symbolic"
            };
            let mut lines = vec![
                medallion(
                    icon::from_name(glyph).size(18).icon().into(),
                    p.accent,
                    AVATAR,
                ),
                cosmic::widget::text::body(title)
                    .size(NAME)
                    .font(super::font())
                    .align_x(cosmic::iced::alignment::Horizontal::Center)
                    .class(cosmic::theme::Text::Color(crate::widget_style::semantic(
                        p.foreground,
                    )))
                    .into(),
            ];
            if !hint.is_empty() {
                lines.push(
                    cosmic::widget::text::body(hint)
                        .size(crate::widget_style::SECONDARY)
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
            let away = matches!(state, State::Away { .. } | State::Stale { .. });
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
            let overflow = total > rows_fitting(height, show_text);
            let visible = rows_fitting(
                height
                    - if overflow {
                        ROW_GAP + crate::widget_style::META * LINE
                    } else {
                        0.
                    },
                show_text,
            );
            let rows = state
                .threads()
                .iter()
                .take(visible)
                .map(|thread| thread_row(thread, p, show_text, now, format_24h))
                .collect::<Vec<_>>();
            let mut list = column::with_children(rows).spacing(ROW_GAP);
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
        Some(on) => cosmic::widget::button::custom(content)
            .name("Open KDE Connect messages")
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(0)
            .class(cosmic::theme::Button::Text)
            .on_press(on(Action::MessagesOpen))
            .into(),
        None => content,
    };
    frame(content, config, height)
}

/// Example conversations, for previews.
pub fn sample() -> State {
    crate::messages::sample()
}
