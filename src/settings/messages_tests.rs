// Included in settings::tests to use the isolated native app harness.

/// The Messages widget is off until chosen, has its own page and search
/// words, saves its switches like the other tools, and its card says what
/// to do in every state. With `DESKTOP_CARDS_LAYOUT_EVIDENCE` set, the page
/// and the card are captured in both shades.
#[test]
fn messages_widget_is_off_until_chosen_and_says_what_to_do() {
    let name = "messages_widget_is_off_until_chosen_and_says_what_to_do";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            fixture: true,
            icons: Icons::Shown,
            ..Default::default()
        },
    ) {
        return;
    }
    use crate::drawer::Widget;
    use crate::messages::State;
    let _root = isolated_root();
    assert!(!Config::default().widget_enabled(Widget::Messages));
    assert!(Config::default().messages.show_text);
    assert!(Widget::TOOLS.contains(&Widget::Messages));
    assert_eq!(navigation::widget_page(Widget::Messages), Page::Messages);
    assert_eq!(
        navigation::widget_of(Page::Messages),
        Some(Widget::Messages)
    );
    assert_eq!(
        crate::desktop::store::widget::tool_of("/messages/enabled"),
        Some("messages")
    );
    for query in ["messages", "sms", "kde connect", "hide message text"] {
        assert!(
            navigation::search(query)
                .iter()
                .any(|hit| hit.page == Page::Messages),
            "{query}"
        );
    }
    let size = crate::tools::cards::size(Widget::Messages, &Config::default());
    assert!(size[0] >= 260. && size[1] >= 200., "{size:?}");

    let (mut app, task) = Settings::init(Core::default(), Page::Messages);
    settle_settings(&mut app, task);
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        native.text_bounds(
            app.settings_view(),
            "Sample · your phone’s newest conversations show here, through KDE Connect.",
        );
        native.text_bounds(app.settings_view(), "Show message text");
        native.text_bounds(app.settings_view(), "Sam Rivera");
        for (theme, shade) in [
            (cosmic::Theme::light(), "light"),
            (cosmic::Theme::dark(), "dark"),
        ] {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("messages-page-{shade}-{}", size.width),
            );
        }
    }
    // The switches save like every other tool's.
    send_settings(&mut app, Message::Widget(Widget::Messages, true));
    assert!(Config::load().widget_enabled(Widget::Messages));
    send_settings(
        &mut app,
        Message::Tools(crate::settings::tools::Message::Option(
            Widget::Messages,
            "show_text",
            false.into(),
        )),
    );
    assert!(!Config::load().messages.show_text);
    // With the text hidden, the preview names who wrote but not what.
    let hidden =
        crate::native_test::Native::new(Size::new(1280., 800.)).all_text(app.settings_view());
    assert!(hidden.iter().any(|t| t == "Sam Rivera"), "{hidden:?}");
    assert!(
        !hidden.iter().any(|t| t.contains("save me a seat")),
        "{hidden:?}"
    );
    assert!(hidden.iter().any(|t| t == "New message"), "{hidden:?}");
    send_settings(&mut app, Message::Widget(Widget::Messages, false));
    assert!(!Config::load().widget_enabled(Widget::Messages));

    // The card as the desktop draws it, in every state, at its default size
    // and a larger one.
    let config = Config::default();
    let sample = crate::tools::cards::sample_messages();
    let away = State::Away {
        phone: "Pixel 8".into(),
        threads: sample.threads().to_vec(),
    };
    let states = [
        ("sample", sample.clone()),
        ("away", away),
        ("no-service", State::NoService),
        ("stopped", State::Stopped),
        ("no-phone", State::NoPhone),
        (
            "no-sms",
            State::NoSms {
                phone: "Pixel 8".into(),
            },
        ),
        (
            "asking",
            State::Asking {
                phone: "Pixel 8".into(),
            },
        ),
        (
            "empty",
            State::Ready {
                phone: "Pixel 8".into(),
                threads: Vec::new(),
            },
        ),
    ];
    for (dims, label) in [(size, "default"), ([420., 340.], "large")] {
        let mut native = crate::native_test::Native::new(Size::new(dims[0], dims[1]));
        let card = |state: &State| {
            crate::tools::cards::messages::<Message>(state, &config, None, Some(dims))
        };
        // The newest conversations fill the card, inside it.
        let rows = crate::tools::cards::messages_rows(dims[1]);
        assert!(rows >= 3, "{label} {rows}");
        let drawn = native.all_text(card(&sample));
        for thread in sample.threads().iter().take(rows) {
            assert!(
                drawn.contains(&thread.who),
                "{label} {} in {drawn:?}",
                thread.who
            );
            let text = native.text_bounds(card(&sample), &thread.who);
            assert!(
                text.x >= 0. && text.x + text.width <= dims[0] && text.y + text.height <= dims[1],
                "{label} {} {text:?}",
                thread.who
            );
        }
        assert!(drawn.contains(&"2 unread".to_string()), "{drawn:?}");
        assert!(drawn.contains(&"You: Sounds good, see you Friday".to_string()));
        // Every state that isn't messages says what to do, inside the card.
        for (name, state) in &states {
            if let Some((title, hint)) = crate::tools::cards::messages_status(state) {
                for line in [title.as_str(), hint].into_iter().filter(|l| !l.is_empty()) {
                    let text = native.text_bounds(card(state), line);
                    assert!(
                        text.x >= 0.
                            && text.x + text.width <= dims[0]
                            && text.y + text.height <= dims[1],
                        "{label} {name} {line:?} {text:?}"
                    );
                }
            }
        }
        for (theme, shade) in [
            (cosmic::Theme::light(), "light"),
            (cosmic::Theme::dark(), "dark"),
        ] {
            for (name, state) in &states {
                native.capture(
                    card(state),
                    &theme,
                    &format!("messages-card-{label}-{name}-{shade}"),
                );
            }
        }
    }
}

/// When a message came reads like a phone's list: the time today, then
/// Yesterday, a weekday, a date.
#[test]
fn message_times_read_like_a_phone() {
    use chrono::TimeZone;
    let now = chrono::Local
        .with_ymd_and_hms(2026, 11, 16, 15, 0, 0)
        .unwrap();
    let at = |days: i64, hour: u32| {
        (now.date_naive() - chrono::Days::new(days as u64))
            .and_hms_opt(hour, 5, 0)
            .unwrap()
            .and_local_timezone(chrono::Local)
            .unwrap()
            .timestamp_millis()
    };
    let when = |ms| crate::tools::cards::messages_when(ms, now, true);
    assert_eq!(when(at(0, 9)), "09:05");
    assert_eq!(
        crate::tools::cards::messages_when(at(0, 14), now, false),
        "2:05 PM"
    );
    assert_eq!(when(at(1, 9)), "Yesterday");
    assert_eq!(when(at(3, 9)), "Fri");
    assert_eq!(when(at(40, 9)), "Oct 7");
    assert_eq!(when(at(400, 9)), "Oct 12, 2025");
}
