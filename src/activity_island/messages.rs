//! A bounded, memory-only projection of phone conversations for the island.
use super::*;
use crate::messages::State;
use crate::tools::cards::messages::{preview, status, when};
use serde::{Deserialize, Serialize};

const ROWS: usize = 4;
const ROW: f32 = 42.;
const GAP: f32 = 4.;
const FOOTER: f32 = 60.;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conversation {
    pub who: String,
    pub preview: String,
    pub at_ms: i64,
    pub unread: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Messages {
    pub phone: String,
    pub action: String,
    pub away: bool,
    pub unread: usize,
    pub total: usize,
    pub status: Option<(String, String)>,
    pub threads: Vec<Conversation>,
}
fn bounded(text: &str, limit: usize) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(limit)
        .collect()
}
/// Redact before crossing the bridge, including outgoing/attachment metadata.
pub(crate) fn project(state: &State, show_text: bool) -> Messages {
    Messages {
        phone: bounded(state.phone().unwrap_or("Phone messages"), 60),
        action: match state {
            State::Starting | State::NoService => "Set up messages",
            State::Stopped | State::NoPhone | State::NoSms { .. } => "Open KDE Connect",
            _ => "Open messages",
        }
        .into(),
        away: matches!(state, State::Away { .. }),
        unread: state.unread(),
        total: state.threads().len(),
        status: status(state).map(|(title, hint)| (bounded(&title, 100), hint.into())),
        threads: state
            .threads()
            .iter()
            .take(ROWS)
            .map(|thread| Conversation {
                who: bounded(&thread.who, 60),
                preview: if show_text {
                    bounded(&preview(thread, true), 160)
                } else if thread.unread {
                    "New message".into()
                } else {
                    "Message".into()
                },
                at_ms: thread.at_ms,
                unread: thread.unread,
            })
            .collect(),
    }
}
impl Applet {
    pub(super) fn messages_heading(&self) -> String {
        match self.snapshot.messages.as_ref() {
            Some(m) if m.unread > 0 => format!("Messages · {} unread", m.unread),
            _ => "Messages".into(),
        }
    }
    pub(super) fn messages_content(&self) -> Content {
        let Some(m) = &self.snapshot.messages else {
            return Content {
                title: "Messages".into(),
                ..Default::default()
            };
        };
        Content {
            // The panel shows the count, leaving personal text inside the open card.
            title: self.messages_heading(),
            meta: if m.away {
                format!("{} away", m.phone)
            } else {
                m.phone.clone()
            },
            ..Default::default()
        }
    }
    pub(super) fn messages_extra(&self) -> f32 {
        // Keep room for incoming conversations while the popup is open.
        ROWS as f32 * (ROW + GAP) + FOOTER - (HEIGHT - 50.)
    }
    pub(super) fn messages_view<'a>(
        &'a self,
        palette: Palette,
        accent: Color,
        reveal: f32,
    ) -> Element<'a, Message> {
        let Some(m) = &self.snapshot.messages else {
            return self.text("Messages unavailable", 13., alpha(palette.quiet, reveal));
        };
        let ink = alpha(if m.away { palette.quiet } else { palette.ink }, reveal);
        let quiet = alpha(palette.quiet, reveal);
        let mut list = widget::column([]).spacing(GAP);
        if let Some((title, hint)) = &m.status {
            list = list.push(self.text(title, 13., ink));
            list = list.push(
                widget::text(hint.clone())
                    .size(12.)
                    .class(cosmic::theme::Text::Color(quiet)),
            );
        } else {
            for (i, thread) in m.threads.iter().take(ROWS).enumerate() {
                let name = if thread.unread {
                    format!("● {}", thread.who)
                } else {
                    thread.who.clone()
                };
                let top = widget::row([
                    widget::container(self.text(
                        name,
                        13.,
                        if thread.unread && !m.away {
                            alpha(accent, reveal)
                        } else {
                            ink
                        },
                    ))
                    .width(Length::Fill)
                    .into(),
                    self.text(
                        when(thread.at_ms, chrono::Local::now(), self.format_24h),
                        11.,
                        quiet,
                    ),
                ])
                .spacing(6);
                list = list.push(
                    widget::container(
                        widget::column([top.into(), self.text(&thread.preview, 12., quiet)])
                            .spacing(2),
                    )
                    .height(ROW)
                    .id(widget::Id::new(format!("activity-message-{i}"))),
                );
            }
            let more = m.total.saturating_sub(m.threads.len());
            list = list.push(self.text(
                {
                    let phone = if m.away {
                        format!("{} away", m.phone)
                    } else {
                        m.phone.clone()
                    };
                    if more > 0 {
                        format!("{phone} · {more} more in KDE Connect")
                    } else {
                        phone
                    }
                },
                11.,
                quiet,
            ));
        }
        list = list.push(self.label_button(
            palette,
            "activity-messages-open",
            &m.action,
            accent,
            reveal,
            Role::Primary,
            Some(Message::Control(Command::OpenMessages)),
        ));
        list.width(Length::Fill).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projection_bounds_and_redacts_before_transport() {
        let mut threads = crate::messages::sample().threads().to_vec();
        threads[0].body = "PRIVATE ".repeat(100);
        threads[0].who = "Name ".repeat(100);
        let state = State::Ready {
            phone: "Phone".into(),
            threads: threads.iter().cycle().take(24).cloned().collect(),
        };
        let shown = project(&state, true);
        assert_eq!(shown.threads.len(), ROWS);
        assert_eq!(shown.total, state.threads().len());
        assert_eq!(shown.unread, state.unread());
        assert!(
            shown
                .threads
                .iter()
                .all(|t| t.who.chars().count() <= 60 && t.preview.chars().count() <= 160)
        );
        let hidden = project(&state, false);
        let wire = serde_json::to_string(&hidden).unwrap();
        assert!(!wire.contains("PRIVATE"));
        assert!(
            hidden
                .threads
                .iter()
                .all(|t| matches!(t.preview.as_str(), "New message" | "Message"))
        );
        assert_eq!(serde_json::from_str::<Messages>(&wire).unwrap(), hidden);
    }
    #[test]
    fn island_messages_are_opt_in_and_show_offline_and_setup_states() {
        let mut app = super::super::tests::app();
        app.snapshot.messages = Some(project(&crate::messages::sample(), true));
        assert!(!app.eligible_kinds().contains(&Kind::Messages));
        app.snapshot.preferences.messages = true;
        assert!(app.eligible_kinds().contains(&Kind::Messages));
        app.kind = Kind::Messages;
        assert!(app.content().title.contains("unread"));
        let away = State::Away {
            phone: "Phone".into(),
            threads: crate::messages::sample().threads().to_vec(),
        };
        app.snapshot.messages = Some(project(&away, false));
        assert_eq!(app.content().meta, "Phone away");
        for state in [
            State::Starting,
            State::NoService,
            State::Stopped,
            State::NoPhone,
            State::NoSms {
                phone: "Phone".into(),
            },
            State::Asking {
                phone: "Phone".into(),
            },
            State::Ready {
                phone: "Phone".into(),
                threads: vec![],
            },
        ] {
            let m = project(&state, false);
            assert!(m.status.is_some());
            assert!(m.threads.is_empty());
        }
    }
    #[test]
    fn island_messages_layout_and_open_action() {
        use cosmic::iced::{Event, Size, mouse};
        let mut app = super::super::tests::app();
        app.snapshot.preferences.messages = true;
        app.kind = Kind::Messages;
        app.popup = Some(window::Id::unique());
        app.held = true;
        app.popup_extra = app.messages_extra();
        app.motion.retarget(1., false, Instant::now());
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        app.commands = Some(tx);
        let sample = crate::messages::sample();
        let states = [
            sample.clone(),
            State::NoService,
            State::Stopped,
            State::NoSms {
                phone: "Phone".into(),
            },
            State::Away {
                phone: "Phone".into(),
                threads: sample.threads().iter().cycle().take(6).cloned().collect(),
            },
        ];
        for state in states {
            for show_text in [true, false] {
                app.snapshot.messages = Some(project(&state, show_text));
                let size = Size::new(app.frame().popup_width(), app.popup_height());
                let mut ui = crate::native_test::Native::new(size);
                let view = || app.capsule(true);
                assert!(ui.duplicate_ids(view()).is_empty());
                let button = ui.bounds(view(), "activity-messages-open");
                assert!(
                    button.height > 0.
                        && button.y >= 0.
                        && button.y + button.height <= size.height + 0.5,
                    "button {button:?} outside {size:?}"
                );
                let texts = ui.all_text(view());
                if !show_text {
                    assert!(!texts.iter().any(|s| s.contains("save me a seat")));
                }
                for i in 0..state.threads().len().min(ROWS) {
                    let row = ui.bounds(view(), &format!("activity-message-{i}"));
                    assert!(row.width > 0. && row.y + row.height <= button.y + 0.5);
                }
                let _ = ui.event(
                    view(),
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    button.center(),
                );
                let clicks = ui
                    .event(
                        view(),
                        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                        button.center(),
                    )
                    .0;
                assert!(
                    clicks
                        .iter()
                        .any(|m| matches!(m, Message::Control(Command::OpenMessages)))
                );
                for click in clicks {
                    let _ = app.update(click);
                }
                assert!(matches!(rx.try_recv(), Ok(Command::OpenMessages)));
            }
        }
    }
    #[test]
    fn updates_do_not_reveal_and_disabling_removes_the_open_list() {
        let mut app = super::super::tests::app();
        let mut snapshot = app.snapshot.clone();
        snapshot.quiet = Some(false);
        snapshot.preferences.messages = true;
        snapshot.messages = Some(project(&crate::messages::sample(), true));
        let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot.clone())));
        assert_eq!(app.kind, Kind::Messages);
        assert!(app.popup.is_none(), "phone messages never auto-reveal");
        app.popup = Some(window::Id::unique());
        snapshot.preferences.messages = false;
        snapshot.messages = None;
        let _ = app.update(Message::Backend(ClientEvent::Snapshot(snapshot)));
        assert_ne!(app.kind, Kind::Messages);
        assert!(app.snapshot.messages.is_none());
    }
}
