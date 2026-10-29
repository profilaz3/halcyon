use super::*;
use crate::calendar::Backend;
use crate::settings::calendar::{Message as Account, SubscriptionProvider};

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
            detail(
                match editor.connection.backend {
                    Backend::Subscription => "Calendar links",
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
                    .tooltip("Account options")
                    .on_press(Message::ToggleHelp("calendar-account"))
                    .into(),
            );
        }
        let mut rows = vec![
            row::with_children(header)
                .spacing(12)
                .align_y(Alignment::Center)
                .into(),
        ];
        if let Some(error) = &editor.error {
            rows.push(label(error, 12., Color::from_rgb8(245, 190, 107)));
        }
        if editor.busy {
            rows.push(detail(
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
            rows.push(detail("Disconnect on this computer?", "This removes the app’s cached events. Your original calendars and events stay unchanged."));
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
                            .on_press(message(Account::RemoveSubscription(choice.id.clone())))
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
                button::text("Disconnect")
                    .id(cosmic::widget::Id::new("calendar-disconnect"))
                    .on_press(message(Account::ConfirmDisconnect(true)))
                    .into(),
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
            rows.push(detail(
                "Disconnected on this computer",
                "Your cached events were removed. Finish removing the saved sign-in from the system keyring before connecting again.",
            ));
            rows.push(
                button::suggested("Finish disconnecting")
                    .id(cosmic::widget::Id::new("calendar-finish-disconnect"))
                    .on_press(message(Account::Disconnect))
                    .into(),
            );
        } else if editor.client_ready {
            rows.push(
                button::suggested("Connect Google Calendar")
                    .id(cosmic::widget::Id::new("calendar-connect"))
                    .on_press(message(Account::Connect))
                    .into(),
            );
            rows.push(label(
                "Sign in through your browser. Calendar access is read-only.",
                12.,
                MUTED,
            ));
        }
        if !editor.busy && !editor.confirm_disconnect && !editor.connection.cleanup_pending() {
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
                rows.push(
                    text_input("Calendar name", &editor.subscription_name)
                        .on_input(move |name| message(Account::SubscriptionName(name)))
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
                    .into(),
                );
                if editor.connection.connected()
                    && editor.connection.backend != Backend::Subscription
                {
                    rows.push(label(
                        "This replaces the current Google connection on this computer.",
                        12.,
                        MUTED,
                    ));
                }
                rows.push(
                    row::with_children(vec![
                        button::suggested("Add calendar")
                            .id(cosmic::widget::Id::new("calendar-subscription-add"))
                            .on_press_maybe(
                                (!editor.subscription_url.0.trim().is_empty())
                                    .then_some(message(Account::AddSubscription)),
                            )
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
            } else {
                rows.push(
                    (if editor.client_ready || connected {
                        button::standard("Add calendar link")
                    } else {
                        button::suggested("Add calendar link")
                    })
                    .id(cosmic::widget::Id::new("calendar-subscription-open"))
                    .on_press(message(Account::SubscriptionForm(true)))
                    .into(),
                );
            }
        }
        group(Some("Calendars & connection"), rows)
    }
    fn calendar_subscription_guide(&self) -> Element<'_, Message> {
        let provider = self.calendar.subscription_provider;
        let mut rows = vec![
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
        ];
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
