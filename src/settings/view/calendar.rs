use super::*;
use crate::calendar::{Backend, SourceKind};
use crate::settings::calendar::{Message as Account, SubscriptionProvider};
use cosmic::Apply;

use crate::settings::calendar::PLAIN_HTTP;
const REPLACES_GOOGLE: &str = "This replaces the current Google connection on this computer.";

impl Settings {
    pub(super) fn calendar_connection(&self) -> Element<'_, Message> {
        let editor = &self.calendar;
        let message = |value| Message::Calendar(value);
        // One header row: service, signed-in account and its state.
        let connected = editor.connection.connected() && !editor.busy && !editor.confirm_disconnect;
        let mut header = vec![
            icon::from_name("x-office-calendar-symbolic")
                .size(28)
                .icon()
                .into(),
            detail_state(
                match editor.connection.backend {
                    Backend::Subscription => "Your calendars",
                    Backend::Disconnected => "Connect a calendar",
                    _ => "Google Calendar",
                },
                if connected && editor.connection.backend != Backend::Subscription {
                    editor.connection.account.clone()
                } else {
                    String::new()
                },
            ),
        ];
        if connected {
            header.push(if editor.error.is_some() {
                pill("Needs attention", Color::from_rgb8(245, 190, 107))
            } else {
                pill(
                    if editor.connection.backend == Backend::Legacy {
                        "Workspace"
                    } else {
                        "Read-only"
                    },
                    self.settings_accent(),
                )
            });
            header.push(
                button::icon(icon::from_name("view-more-symbolic").size(18))
                    .id(cosmic::widget::Id::new("calendar-account-options"))
                    .on_press(Message::ToggleHelp("calendar-account"))
                    .apply(|control| super::tip::tip(control, "Account options")),
            );
        }
        // Not connected and nothing in progress: the single "Connect a
        // calendar" row below says it all, so the header is left out.
        let invite = !editor.connection.connected()
            && !editor.busy
            && !editor.confirm_disconnect
            && !editor.adding()
            && !editor.connection.cleanup_pending();
        let mut rows = Vec::new();
        if !invite {
            rows.push(
                row::with_children(header)
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .into(),
            );
        }
        if let Some(error) = &editor.error {
            rows.push(label(error, 12., Color::from_rgb8(245, 190, 107)));
        }
        if editor.busy {
            rows.push(detail_state(
                if editor.pending.is_some() {
                    "Finish signing in in your browser"
                } else if editor.connecting {
                    "Preparing Google sign-in…"
                } else {
                    "Updating Calendar settings…"
                },
                if editor.pending.is_some() { "Complete sign-in in a normal browser, such as Firefox or Chrome. You can paste the sign-in link into another browser." } else { "" },
            ));
            let mut actions = vec![];
            if editor.pending.is_some() {
                actions.push(
                    button::standard("Open sign-in page")
                        .on_press(message(Account::OpenBrowser))
                        .into(),
                );
            }
            if editor.pending.is_some() {
                actions.push(
                    button::text("Copy sign-in link")
                        .on_press(message(Account::CopyLink))
                        .into(),
                );
            }
            if editor.connecting {
                actions.push(
                    button::text("Cancel")
                        .on_press(message(Account::Cancel))
                        .into(),
                );
            }
            rows.push(row::with_children(actions).spacing(8).wrap().into());
        } else if editor.confirm_disconnect {
            rows.push(detail_state("Disconnect on this computer?", "This removes the app’s cached events. Your original calendars and events stay unchanged."));
            rows.push(
                row::with_children(vec![
                    button::standard("Keep connected")
                        .id(cosmic::widget::Id::new("calendar-keep-connected"))
                        .on_press(message(Account::ConfirmDisconnect(false)))
                        .into(),
                    button::text("Disconnect")
                        .on_press(message(Account::Disconnect))
                        .into(),
                ])
                .spacing(8)
                .wrap()
                .into(),
            );
        } else if editor.connection.connected() {
            if editor.error.is_some() {
                rows.push(label("Check connection to reconnect", 12., MUTED));
            }
            let selected = editor.connection.selected_calendars();
            if editor.choices.is_empty() {
                rows.push(label(editor.connection.calendar_name.clone(), 14., MUTED));
            }
            for (index, choice) in editor.choices.iter().enumerate() {
                if editor.confirm_remove.as_ref() == Some(&choice.id) {
                    rows.push(detail_state(
                        format!("Remove “{}”?", choice.name),
                        match editor.connection.source_kind(&choice.id) {
                            SourceKind::Link => "You’ll need its private link to add it again.",
                            SourceKind::CalDav => {
                                "You can add it again from your CalDAV account at any time."
                            }
                            SourceKind::Holidays => "You can add it again from Add holidays.",
                            SourceKind::File => "The file stays on your computer.",
                            SourceKind::Folder => "The folder stays on your computer.",
                        },
                    ));
                    rows.push(
                        row::with_children(vec![
                            button::standard("Cancel")
                                .id(cosmic::widget::Id::new("calendar-remove-cancel"))
                                .on_press(message(Account::ConfirmRemove(None)))
                                .into(),
                            button::destructive("Remove")
                                .id(cosmic::widget::Id::new("calendar-remove-confirm"))
                                .on_press_maybe((!editor.busy).then(|| {
                                    message(Account::RemoveSubscription(choice.id.clone()))
                                }))
                                .into(),
                        ])
                        .spacing(8)
                        .wrap()
                        .into(),
                    );
                    continue;
                }
                let enabled = selected.iter().any(|c| c.id == choice.id);
                let mut controls: Vec<Element<'_, Message>> = vec![
                    container(
                        toggler(enabled).on_toggle(move |on| message(Account::Toggle(index, on))),
                    )
                    .id(cosmic::widget::Id::new(format!("calendar-choice-{index}")))
                    .into(),
                ];
                if editor.connection.backend == Backend::Subscription {
                    controls.push(
                        button::text("Remove")
                            .id(cosmic::widget::Id::new(format!("calendar-remove-{index}")))
                            .on_press(message(Account::ConfirmRemove(Some(choice.id.clone()))))
                            .into(),
                    );
                }
                rows.push(split(
                    label(choice.name.clone(), 14., FG),
                    row::with_children(controls).spacing(8).into(),
                ));
            }
            let mut actions = vec![
                button::text(if editor.loading {
                    "Checking…"
                } else {
                    "Check connection"
                })
                .on_press_maybe((!editor.loading).then_some(message(Account::Check)))
                .into(),
                // "Open calendar" comes and goes in front of it with the backend.
                slot(
                    button::text("Disconnect")
                        .id(cosmic::widget::Id::new("calendar-disconnect"))
                        .on_press(message(Account::ConfirmDisconnect(true)))
                        .into(),
                ),
            ];
            if editor.connection.backend != Backend::Subscription {
                actions.insert(
                    0,
                    button::text("Open calendar ↗")
                        .on_press(Message::OpenCalendar)
                        .into(),
                );
            }
            if editor.client_ready {
                actions.push(
                    button::text(if editor.error.is_some() {
                        "Reconnect"
                    } else {
                        "Switch account"
                    })
                    .on_press(message(Account::Connect))
                    .into(),
                );
            }
            if self.help_open == Some("calendar-account") || editor.error.is_some() {
                // Keep the disclosed account actions beside their trigger,
                // even when the account has a long list of calendars.
                rows.insert(
                    if editor.error.is_some() { 2 } else { 1 },
                    row::with_children(actions).spacing(6).wrap().into(),
                );
            }
        } else if editor.connection.cleanup_pending() {
            rows.push(detail_state(
                "Disconnected on this computer",
                "Your cached events were removed. Finish removing the saved sign-in from the system keyring before connecting again.",
            ));
            rows.push(
                button::suggested("Finish disconnecting")
                    .id(cosmic::widget::Id::new("calendar-finish-disconnect"))
                    .on_press(message(Account::Disconnect))
                    .into(),
            );
        } else if editor.caldav_form
            || editor.file_pending.is_some()
            || !editor.folder_found.is_empty()
            || editor.holidays_form
        {
            rows.push(label("Halcyon only reads your events.", 12., MUTED));
        } else if editor.subscription_form {
            rows.push(label(
                if editor.client_ready {
                    "Halcyon only reads your events. Sign in to Google, or paste a link from any calendar app."
                } else {
                    "Halcyon only reads your events. Paste the private link from your calendar app."
                },
                12.,
                MUTED,
            ));
        }
        if !editor.busy && !editor.confirm_disconnect && !editor.connection.cleanup_pending() {
            // Adding replaces a Google sign-in, as a calendar link always has.
            let replaces =
                editor.connection.connected() && editor.connection.backend != Backend::Subscription;
            if editor.subscription_form {
                rows.push(
                    button::text(if editor.subscription_help {
                        "Hide calendar link guide"
                    } else {
                        "Get a calendar link"
                    })
                    .id("calendar-link-help".into())
                    .on_press(message(Account::SubscriptionHelp(
                        !editor.subscription_help,
                    )))
                    .into(),
                );
                if editor.subscription_help {
                    rows.push(self.calendar_subscription_guide());
                }
                // Enter adds the calendar, like Add calendar, once there's a link.
                let add = (!editor.subscription_url.0.trim().is_empty()
                    && (!editor.subscription_login
                        || !editor.subscription_username.0.trim().is_empty()))
                .then_some(message(Account::AddSubscription));
                rows.push(
                    text_input("Calendar name", &editor.subscription_name)
                        .on_input(move |name| message(Account::SubscriptionName(name)))
                        .on_submit_maybe(add.clone().map(|add| move |_| add.clone()))
                        .into(),
                );
                rows.push(
                    cosmic::widget::secure_input(
                        "Calendar subscription link",
                        &editor.subscription_url.0,
                        None,
                        true,
                    )
                    .on_input(move |url| {
                        message(Account::SubscriptionUrl(crate::calendar::SecretText(url)))
                    })
                    .on_submit_maybe(add.clone().map(|add| move |_| add.clone()))
                    .into(),
                );
                if crate::calendar::plain_local(&editor.subscription_url.0, false) {
                    rows.push(label(PLAIN_HTTP, 12., MUTED));
                }
                rows.push(split(
                    label("This link needs a password", 14., FG),
                    container(
                        toggler(editor.subscription_login)
                            .on_toggle(move |on| message(Account::SubscriptionLogin(on))),
                    )
                    .id("calendar-link-login")
                    .into(),
                ));
                if editor.subscription_login {
                    rows.push(
                        text_input("Username", &editor.subscription_username.0)
                            .id("calendar-link-username".into())
                            .on_input(move |value| {
                                message(Account::SubscriptionUsername(crate::calendar::SecretText(
                                    value,
                                )))
                            })
                            .on_submit_maybe(add.clone().map(|add| move |_| add.clone()))
                            .into(),
                    );
                    rows.push(
                        cosmic::widget::secure_input(
                            "Password",
                            &editor.subscription_password.0,
                            None,
                            true,
                        )
                        .on_input(move |value| {
                            message(Account::SubscriptionPassword(crate::calendar::SecretText(
                                value,
                            )))
                        })
                        .on_submit_maybe(add.clone().map(|add| move |_| add.clone()))
                        .into(),
                    );
                }
                if replaces {
                    rows.push(label(REPLACES_GOOGLE, 12., MUTED));
                }
                rows.push(
                    row::with_children(vec![
                        button::standard("Add calendar")
                            .id(cosmic::widget::Id::new("calendar-subscription-add"))
                            .on_press_maybe(add)
                            .into(),
                        button::text("Cancel")
                            .on_press(message(Account::SubscriptionForm(false)))
                            .into(),
                        self.help("calendar-subscription-privacy", "Private link", "Saved in your system keyring. Halcyon reads events directly from the provider. Anyone with a private link may be able to read that calendar."),
                    ])
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .wrap()
                    .into(),
                );
            } else if editor.caldav_form {
                rows.extend(self.calendar_caldav_form(replaces));
            } else if let Some(path) = &editor.file_pending {
                let add = Some(message(Account::AddFile));
                rows.push(detail_state(
                    "Calendar file",
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                ));
                rows.push(
                    text_input("Calendar name", &editor.file_name)
                        .on_input(move |name| message(Account::FileName(name)))
                        .on_submit_maybe(add.clone().map(|add| move |_| add.clone()))
                        .into(),
                );
                rows.push(label(
                    "Halcyon reads the file again whenever it changes.",
                    12.,
                    MUTED,
                ));
                if replaces {
                    rows.push(label(REPLACES_GOOGLE, 12., MUTED));
                }
                rows.push(
                    row::with_children(vec![
                        button::standard("Add calendar")
                            .id(cosmic::widget::Id::new("calendar-file-add"))
                            .on_press_maybe(add)
                            .into(),
                        button::text("Cancel")
                            .on_press(message(Account::CancelFile))
                            .into(),
                    ])
                    .spacing(8)
                    .wrap()
                    .into(),
                );
            } else if !editor.folder_found.is_empty() {
                rows.extend(self.calendar_folder_form(replaces));
            } else if editor.holidays_form {
                rows.extend(self.calendar_holidays_form(replaces));
            } else {
                // Every way to connect sits together in one row.
                // Google sign-in can become available while this page is open,
                // which puts a named Connect button in front of the named link
                // button. Unnamed slots keep both on the right widget state.
                let mut actions: Vec<Element<'_, Message>> = Vec::new();
                if editor.client_ready && !editor.connection.connected() {
                    actions.push(slot(
                        button::suggested("Connect Google Calendar")
                            .id(cosmic::widget::Id::new("calendar-connect"))
                            .on_press(message(Account::Connect))
                            .into(),
                    ));
                }
                let full = editor.free_slots() == 0;
                actions.push(slot(
                    button::standard(if editor.connection.connected() {
                        "Add calendar link"
                    } else {
                        "Paste link…"
                    })
                    .id(cosmic::widget::Id::new("calendar-subscription-open"))
                    .on_press_maybe((!full).then(|| message(Account::SubscriptionForm(true))))
                    .into(),
                ));
                actions.push(slot(
                    button::standard("Add CalDAV account")
                        .id(cosmic::widget::Id::new("calendar-caldav-open"))
                        .on_press_maybe((!full).then(|| message(Account::CalDavForm(true))))
                        .into(),
                ));
                actions.push(slot(
                    button::text("Add calendar file…")
                        .id(cosmic::widget::Id::new("calendar-file-open"))
                        .on_press_maybe((!full).then(|| message(Account::ChooseFile)))
                        .into(),
                ));
                actions.push(slot(
                    button::text("Add calendar folder…")
                        .id(cosmic::widget::Id::new("calendar-folder-open"))
                        .on_press_maybe((!full).then(|| message(Account::ChooseFolder)))
                        .into(),
                ));
                actions.push(slot(
                    button::text("Add holidays…")
                        .id(cosmic::widget::Id::new("calendar-holidays-open"))
                        .on_press_maybe((!full).then(|| message(Account::HolidaysForm(true))))
                        .into(),
                ));
                let actions = row::with_children(actions).spacing(8).wrap();
                if editor.connection.connected() {
                    rows.push(actions.into());
                    if full {
                        rows.push(label(
                            "You have eight calendars, the most Halcyon shows. Remove one to add another.",
                            12.,
                            MUTED,
                        ));
                    }
                } else {
                    // Not connected yet: one row that says what to do, with
                    // the ways to do it on the right.
                    rows.push(split(
                        detail_state(
                            "Add a calendar",
                            if editor.client_ready {
                                "Sign in to Google, add a CalDAV account, or paste a calendar link from Outlook or Apple Calendar"
                            } else {
                                "Add a CalDAV account, or paste the sharing link from Google, Outlook or Apple Calendar"
                            },
                        ),
                        actions.into(),
                    ));
                }
            }
        }
        group(Some("Calendars"), rows)
    }
    /// Server, username and password, then the calendars found there.
    fn calendar_caldav_form(&self, replaces: bool) -> Vec<Element<'_, Message>> {
        let editor = &self.calendar;
        let message = |value| Message::Calendar(value);
        let ready = !editor.caldav_server.0.trim().is_empty()
            && !editor.caldav_username.0.trim().is_empty()
            && !editor.caldav_password.0.is_empty()
            && !editor.caldav_searching;
        let find = ready.then_some(message(Account::FindCalendars));
        let mut rows: Vec<Element<'_, Message>> = vec![
            detail_state(
                "CalDAV account",
                "Works with iCloud, Fastmail, Nextcloud, mailbox.org and other CalDAV servers.",
            ),
            text_input("Server address", &editor.caldav_server.0)
                .id("calendar-caldav-server".into())
                .on_input(move |value| {
                    message(Account::CalDavServer(crate::calendar::SecretText(value)))
                })
                .on_submit_maybe(find.clone().map(|find| move |_| find.clone()))
                .into(),
            label(
                "For example caldav.icloud.com, caldav.fastmail.com or https://cloud.example.com/remote.php/dav",
                12.,
                MUTED,
            ),
        ];
        if crate::calendar::plain_local(&editor.caldav_server.0, true) {
            rows.push(label(PLAIN_HTTP, 12., MUTED));
        }
        rows.push(
            text_input("Username", &editor.caldav_username.0)
                .id("calendar-caldav-username".into())
                .on_input(move |value| {
                    message(Account::CalDavUsername(crate::calendar::SecretText(value)))
                })
                .on_submit_maybe(find.clone().map(|find| move |_| find.clone()))
                .into(),
        );
        rows.push(
            cosmic::widget::secure_input("Password", &editor.caldav_password.0, None, true)
                .on_input(move |value| {
                    message(Account::CalDavPassword(crate::calendar::SecretText(value)))
                })
                .on_submit_maybe(find.clone().map(|find| move |_| find.clone()))
                .into(),
        );
        rows.push(label(
            "Use an app password if your provider offers one. Google Calendar needs its sign in or a calendar link instead.",
            12.,
            MUTED,
        ));
        if !editor.caldav_found.is_empty() {
            rows.push(label("Choose calendars to add", 14., FG));
            for (index, calendar) in editor.caldav_found.iter().enumerate() {
                let on = editor.caldav_picked.get(index).copied().unwrap_or(false);
                rows.push(split(
                    label(calendar.name.clone(), 14., FG),
                    container(
                        toggler(on).on_toggle(move |on| message(Account::PickCalendar(index, on))),
                    )
                    .id(cosmic::widget::Id::new(format!(
                        "calendar-caldav-pick-{index}"
                    )))
                    .into(),
                ));
            }
            let picked = editor.caldav_picked.iter().filter(|on| **on).count();
            let free = editor.free_slots();
            if picked > free {
                rows.push(label(
                    format!("You can add {free} more. Halcyon shows up to eight calendars."),
                    12.,
                    Color::from_rgb8(245, 190, 107),
                ));
            }
            if replaces {
                rows.push(label(REPLACES_GOOGLE, 12., MUTED));
            }
            rows.push(
                row::with_children(vec![
                    button::standard("Add calendars")
                        .id(cosmic::widget::Id::new("calendar-caldav-add"))
                        .on_press_maybe(
                            (picked > 0 && picked <= free).then_some(message(Account::AddCalDav)),
                        )
                        .into(),
                    button::text("Cancel")
                        .on_press(message(Account::CalDavForm(false)))
                        .into(),
                ])
                .spacing(8)
                .wrap()
                .into(),
            );
        } else {
            rows.push(
                row::with_children(vec![
                    button::standard(if editor.caldav_searching {
                        "Finding calendars…"
                    } else {
                        "Find calendars"
                    })
                    .id(cosmic::widget::Id::new("calendar-caldav-find"))
                    .on_press_maybe(find)
                    .into(),
                    button::text("Cancel")
                        .on_press(message(Account::CalDavForm(false)))
                        .into(),
                    self.help(
                        "calendar-caldav-privacy",
                        "Your password",
                        "The server, username and password are saved in your system keyring and sent only to that server.",
                    ),
                ])
                .spacing(8)
                .align_y(Alignment::Center)
                .wrap()
                .into(),
            );
        }
        rows
    }
    /// The calendars found in a chosen folder, each with a switch.
    fn calendar_folder_form(&self, replaces: bool) -> Vec<Element<'_, Message>> {
        let editor = &self.calendar;
        let message = |value| Message::Calendar(value);
        let found = editor.folder_found.len();
        let mut rows: Vec<Element<'_, Message>> = vec![detail_state(
            "Calendar folder",
            if found == 1 {
                "1 calendar found".to_string()
            } else {
                format!("{found} calendars found")
            },
        )];
        for (index, calendar) in editor.folder_found.iter().enumerate() {
            let on = editor.folder_picked.get(index).copied().unwrap_or(false);
            rows.push(split(
                label(calendar.name.clone(), 14., FG),
                container(toggler(on).on_toggle(move |on| message(Account::PickFolder(index, on))))
                    .id(cosmic::widget::Id::new(format!(
                        "calendar-folder-pick-{index}"
                    )))
                    .into(),
            ));
        }
        rows.push(label(
            "Halcyon reads the folder again whenever its files change.",
            12.,
            MUTED,
        ));
        let picked = editor.folder_picked.iter().filter(|on| **on).count();
        let free = editor.free_slots();
        if picked > free {
            rows.push(label(
                format!("You can add {free} more. Halcyon shows up to eight calendars."),
                12.,
                Color::from_rgb8(245, 190, 107),
            ));
        }
        if replaces {
            rows.push(label(REPLACES_GOOGLE, 12., MUTED));
        }
        rows.push(
            row::with_children(vec![
                button::standard(if picked > 1 {
                    "Add calendars"
                } else {
                    "Add calendar"
                })
                .id(cosmic::widget::Id::new("calendar-folder-add"))
                .on_press_maybe(
                    (picked > 0 && picked <= free).then_some(message(Account::AddFolders)),
                )
                .into(),
                button::text("Cancel")
                    .on_press(message(Account::CancelFolder))
                    .into(),
            ])
            .spacing(8)
            .wrap()
            .into(),
        );
        rows
    }
    /// A country picker for a public holiday calendar.
    fn calendar_holidays_form(&self, replaces: bool) -> Vec<Element<'_, Message>> {
        let editor = &self.calendar;
        let message = |value| Message::Calendar(value);
        let mut rows: Vec<Element<'_, Message>> = vec![
            detail_state(
                "Public holidays",
                "Shown in a muted color behind your own events.",
            ),
            super::focus::control(
                Some("Country".to_owned()),
                None,
                container(cosmic::widget::dropdown(
                    crate::calendar::holidays::LABELS.as_slice(),
                    editor.holidays_country,
                    |index| Message::Calendar(Account::HolidaysCountry(index)),
                ))
                .id("calendar-holidays-country")
                .into(),
            ),
        ];
        if replaces {
            rows.push(label(REPLACES_GOOGLE, 12., MUTED));
        }
        rows.push(
            row::with_children(vec![
                button::standard("Add calendar")
                    .id(cosmic::widget::Id::new("calendar-holidays-add"))
                    .on_press_maybe(
                        editor
                            .holidays_country
                            .map(|_| message(Account::AddHolidays)),
                    )
                    .into(),
                button::text("Cancel")
                    .on_press(message(Account::HolidaysForm(false)))
                    .into(),
                self.help(
                    "calendar-holidays-privacy",
                    "Where holidays come from",
                    "Holiday calendars are public files kept by Thunderbird. Halcyon downloads them from thunderbird.net like any calendar link.",
                ),
            ])
            .spacing(8)
            .align_y(Alignment::Center)
            .wrap()
            .into(),
        );
        rows
    }
    fn calendar_subscription_guide(&self) -> Element<'_, Message> {
        let provider = self.calendar.subscription_provider;
        let mut rows = vec![super::focus::control(
            Some("Calendar provider".to_owned()),
            None,
            cosmic::widget::dropdown(
                &SubscriptionProvider::LABELS,
                SubscriptionProvider::ALL
                    .iter()
                    .position(|value| *value == provider),
                |index| {
                    Message::Calendar(Account::SubscriptionProvider(
                        SubscriptionProvider::ALL
                            .get(index)
                            .copied()
                            .unwrap_or_default(),
                    ))
                },
            )
            .into(),
        )];
        rows.extend(provider.steps().iter().map(|step| label(*step, 13., FG)));
        rows.push(label(provider.privacy(), 12., MUTED));
        container(column::with_children(rows).spacing(8))
            .id("calendar-link-guide")
            .width(Length::Fill)
            .into()
    }
    pub(super) fn calendar_publisher_setup(&self) -> Element<'_, Message> {
        let mut rows = vec![
            button::text(if self.calendar.publisher_help {
                "Hide Google sign-in setup"
            } else {
                "Google sign-in setup…"
            })
            .on_press(Message::Calendar(Account::PublisherHelp(
                !self.calendar.publisher_help,
            )))
            .into(),
        ];
        if self.calendar.publisher_help {
            if !self.calendar.client_ready {
                rows.push(label("Google sign-in is not configured in this build. You can use a calendar link without setting it up.", 12., MUTED));
            }
            rows.push(detail("For the app publisher or a personal build", "Register a Google Desktop app with the Calendar API enabled. Users of a configured release only need the Connect button."));
            rows.push(
                row::with_children(vec![
                    button::standard("Import Google app JSON…")
                        .on_press_maybe(
                            (!self.calendar.busy)
                                .then_some(Message::Calendar(Account::ImportClient)),
                        )
                        .into(),
                    button::text("Setup guide ↗")
                        .on_press(Message::Calendar(Account::OpenGuide))
                        .into(),
                ])
                .spacing(8)
                .wrap()
                .into(),
            );
        }
        column::with_children(rows).spacing(10).into()
    }
}
