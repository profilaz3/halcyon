//! "Add calendar…", then Online Accounts: calendars EDS offers, picked with
//! switches.
//! Picked calendars join the calendar list beside links; nothing here signs in.
use super::{Editor, Message as Account, wrap};
use super::{Settings, Task, account};
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum Message {
    Open(bool),
    Listed(u64, Result<Vec<account::SystemCalendar>, account::Problem>),
    Toggle(String, bool),
    OnlineAccounts,
}

#[derive(Default)]
pub struct Picker {
    pub open: bool,
    /// None while the list is being read.
    pub listed: Option<Result<Vec<account::SystemCalendar>, account::Problem>>,
    /// Whether the Online Accounts app is installed.
    pub online_accounts: bool,
    request: u64,
}
impl Picker {
    /// Whether `calendar` is in the calendar list now.
    pub fn added(editor: &Editor, calendar: &account::SystemCalendar) -> bool {
        editor.connection.backend == account::Backend::Subscription
            && editor
                .connection
                .subscriptions
                .iter()
                .any(|c| c.id == calendar.id())
    }
    /// No room for another calendar in the list.
    pub fn full(editor: &Editor) -> bool {
        editor.connection.backend == account::Backend::Subscription
            && editor.connection.subscriptions.len() >= account::MAX_CALENDARS
    }
}

impl Settings {
    pub(super) fn update_calendar_system(
        &mut self,
        message: Message,
    ) -> Task<super::super::Message> {
        let editor = &mut self.calendar;
        match message {
            Message::Open(open) => {
                editor.system.open = open;
                if !open {
                    return Task::none();
                }
                editor.system.listed = None;
                editor.system.online_accounts = account::online_accounts_app().is_some();
                editor.system.request = editor.system.request.wrapping_add(1);
                let request = editor.system.request;
                return cosmic::task::future(async move {
                    let listed =
                        tokio::time::timeout(Duration::from_secs(15), account::system_calendars())
                            .await
                            .unwrap_or(Err(account::Problem::TimedOut));
                    wrap(Account::System(Message::Listed(request, listed)))
                });
            }
            Message::Listed(request, listed) => {
                if request == editor.system.request && editor.system.open {
                    editor.system.listed = Some(listed);
                }
            }
            Message::Toggle(uid, on) => {
                if editor.busy || editor.connection.cleanup_pending() {
                    return Task::none();
                }
                let Some(Ok(listed)) = &editor.system.listed else {
                    return Task::none();
                };
                let Some(calendar) = listed.iter().find(|c| c.uid == uid).cloned() else {
                    return Task::none();
                };
                if on == Picker::added(editor, &calendar) {
                    return Task::none();
                }
                if on && Picker::full(editor) {
                    editor.error = Some(account::Problem::CalendarLimit.to_string());
                    return Task::none();
                }
                editor.busy = true;
                editor.error = None;
                editor.attempt = editor.attempt.wrapping_add(1);
                let attempt = editor.attempt;
                let expected = editor.connection.clone();
                return cosmic::task::future(async move {
                    let result = tokio::time::timeout(
                        Duration::from_secs(60),
                        account::set_system_calendar(expected, calendar, on),
                    )
                    .await
                    .unwrap_or(Err(account::Problem::TimedOut));
                    wrap(Account::Finished(attempt, result))
                });
            }
            Message::OnlineAccounts => {
                if let Err(error) = account::open_online_accounts() {
                    editor.error = Some(error);
                }
            }
        }
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Core, Page};
    use cosmic::Application;

    fn listed() -> Vec<account::SystemCalendar> {
        [
            ("work", "Work", "#3465a4", "alex@example.com"),
            ("team", "Team rota", "#2ec27e", "alex@example.com"),
            ("outlook", "Calendar", "#e66100", "alex@contoso.example"),
            (
                "birthdays",
                "Birthdays & Anniversaries",
                "#ffbe6f",
                "Contacts",
            ),
        ]
        .into_iter()
        .map(|(uid, name, color, account)| account::SystemCalendar {
            uid: uid.into(),
            name: name.into(),
            color: color.into(),
            account: account.into(),
        })
        .collect()
    }
    fn send(app: &mut Settings, message: Message) {
        let _ = app.update_calendar(Account::System(message));
    }
    /// The picker lists, adds through the shared calendar result and keeps
    /// to eight calendars, without touching the account file itself.
    #[test]
    fn system_picker_lists_adds_and_stops_at_eight() {
        if crate::displays::test_support::isolated(
            "settings::calendar::system::tests::system_picker_lists_adds_and_stops_at_eight",
        ) {
            return;
        }
        let (mut app, _) = Settings::init(Core::default(), Page::Calendar);
        let account_file = std::fs::read(account::account_path().unwrap()).ok();
        send(&mut app, Message::Open(true));
        assert!(app.calendar.system.open);
        assert!(app.calendar.system.listed.is_none());
        let request = app.calendar.system.request;
        // A reply to an earlier opening is dropped.
        send(
            &mut app,
            Message::Listed(request.wrapping_sub(1), Ok(vec![])),
        );
        assert!(app.calendar.system.listed.is_none());
        send(&mut app, Message::Listed(request, Ok(listed())));
        assert_eq!(app.calendar.system.listed, Some(Ok(listed())));

        send(&mut app, Message::Toggle("work".into(), true));
        assert!(app.calendar.busy, "adding runs in the background");
        // Another switch waits for it.
        let attempt = app.calendar.attempt;
        send(&mut app, Message::Toggle("team".into(), true));
        assert_eq!(app.calendar.attempt, attempt);
        // Its result arrives like a link's.
        let added = account::Connection {
            backend: account::Backend::Subscription,
            account: "Calendar subscriptions".into(),
            calendar_id: "eds:work".into(),
            calendar_name: "Work".into(),
            revision: "r1".into(),
            selections: vec![listed()[0].choice()],
            subscriptions: vec![listed()[0].choice()],
            ..Default::default()
        };
        let _ = app.update_calendar(Account::Finished(attempt, Ok(added.clone())));
        assert!(!app.calendar.busy);
        assert!(Picker::added(&app.calendar, &listed()[0]));
        assert!(!Picker::added(&app.calendar, &listed()[1]));
        assert!(app.calendar.system.open, "the picker stays open");

        // Eight in the list: another can't be switched on.
        let mut full = added.clone();
        for n in 0..7 {
            full.subscriptions.push(account::CalendarChoice {
                id: format!("link-{n}"),
                name: format!("Link {n}"),
                primary: false,
                color: String::new(),
            });
        }
        app.calendar.connection = full;
        assert!(Picker::full(&app.calendar));
        send(&mut app, Message::Toggle("team".into(), true));
        assert!(!app.calendar.busy);
        assert_eq!(
            app.calendar.error.as_deref(),
            Some(account::Problem::CalendarLimit.message())
        );
        // Switching one off is still allowed.
        send(&mut app, Message::Toggle("work".into(), false));
        assert!(app.calendar.busy);

        send(&mut app, Message::Open(false));
        assert!(!app.calendar.system.open);
        assert_eq!(
            std::fs::read(account::account_path().unwrap()).ok(),
            account_file
        );
    }

    #[test]
    fn native_system_picker_fits_compact_and_wide() {
        if crate::displays::test_support::isolated(
            "settings::calendar::system::tests::native_system_picker_fits_compact_and_wide",
        ) {
            return;
        }
        use crate::settings::tests::{native_reveal, send_settings};
        const HELP: &str =
            "Calendars from accounts in Online Accounts, such as Google and Nextcloud.";
        for (width, height) in [(820., 600.), (1280., 800.)] {
            let (mut app, _) = Settings::init(Core::default(), Page::Calendar);
            send_settings(
                &mut app,
                crate::settings::Message::Resized(cosmic::iced::Size::new(width, height)),
            );
            let mut native =
                crate::native_test::Native::new(cosmic::iced::Size::new(width, height));
            native_reveal(&app, &mut native, "calendar-add-open");
            send(&mut app, Message::Open(true));
            app.calendar.system.online_accounts = true;
            let request = app.calendar.system.request;
            send(&mut app, Message::Listed(request, Ok(listed())));
            // One already in the list, beside a link calendar.
            let mut work = listed()[0].choice();
            work.name = "Work".into();
            let link = account::CalendarChoice {
                id: "link".into(),
                name: "Team link".into(),
                primary: false,
                color: String::new(),
            };
            app.calendar.connection = account::Connection {
                backend: account::Backend::Subscription,
                account: "Calendar subscriptions".into(),
                calendar_id: "link".into(),
                calendar_name: "Team link".into(),
                revision: "r".into(),
                selections: vec![link.clone(), work.clone()],
                subscriptions: vec![link, work],
                ..Default::default()
            };
            app.calendar.choices = app.calendar.connection.subscriptions.clone();
            let picker = native_reveal(&app, &mut native, "calendar-system-picker");
            assert!(picker.width > 0. && picker.x + picker.width <= width);
            let help = native.text_bounds(app.settings_view(), HELP);
            assert!(help.width > 0. && help.x >= 0. && help.x + help.width <= width);
            for id in [
                "calendar-system-3",
                "calendar-online-accounts",
                "calendar-system-done",
            ] {
                let bounds = native_reveal(&app, &mut native, id);
                assert!(
                    bounds.x >= 0. && bounds.x + bounds.width <= width,
                    "{id} at {width}: {bounds:?}"
                );
            }
            native.capture(
                app.settings_view(),
                &cosmic::Theme::dark(),
                &format!("calendar-system-picker-{width}"),
            );
            // EDS missing: the plain message, nothing else breaks.
            send(&mut app, Message::Open(true));
            let request = app.calendar.system.request;
            send(
                &mut app,
                Message::Listed(request, Err(account::Problem::SystemUnavailable)),
            );
            let missing =
                native.text_bounds(app.settings_view(), "System calendars are not available.");
            assert!(missing.width > 0.);
            native_reveal(&app, &mut native, "calendar-system-done");
            native.capture(
                app.settings_view(),
                &cosmic::Theme::dark(),
                &format!("calendar-system-unavailable-{width}"),
            );
        }
    }
}
