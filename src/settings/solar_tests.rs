// Included in settings::tests; native controls and the isolated store are real.
#[test]
fn solar_city_picker_keeps_weather_and_stale_searches_separate() {
    let name = "solar_city_picker_keeps_weather_and_stale_searches_separate";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            fixture: true,
            ..Default::default()
        },
    ) {
        return;
    }
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    send_settings(&mut app, Message::Page(Page::DesktopAutomation));
    let weather = app.config.location.clone();
    let city = City {
        name: "Sydney".into(),
        latitude: -33.87,
        longitude: 151.21,
        timezone: Some("Australia/Sydney".into()),
        admin1: "NSW".into(),
        country: "Australia".into(),
    };
    send_settings(&mut app, Message::ScheduleCityInput("Sydney".into()));
    let _ = app
        .schedule_city_search
        .begin(app.schedule_city_search.generation);
    let stale = app.schedule_city_search.generation;
    send_settings(&mut app, Message::ScheduleCityInput("Paris".into()));
    send_settings(
        &mut app,
        Message::ScheduleCities(stale, Ok(vec![city.clone()])),
    );
    assert!(app.schedule_cities.is_empty());
    let _ = app
        .schedule_city_search
        .begin(app.schedule_city_search.generation);
    let current = app.schedule_city_search.generation;
    send_settings(
        &mut app,
        Message::ScheduleCities(current, Ok(vec![city.clone()])),
    );
    send_settings(&mut app, Message::ChooseScheduleCity(0));
    // Weather adopts the schedule city only when it has none of its own;
    // an existing weather city is never replaced.
    let weather = weather.or_else(|| Some(city.location()));
    assert_eq!(app.config.location, weather);
    assert_eq!(
        app.automation.settings().unwrap().manual_city,
        Some(city.location())
    );
    let stored = Config::load();
    assert_eq!(stored.location, weather);
    assert!(app.schedule_cities.is_empty());
    assert!(
        !app.automation.settings().unwrap().enabled,
        "saving a city never enables automation"
    );
}

#[test]
fn solar_native_rules_save_and_render_compact_wide_and_missing_city() {
    let name = "solar_native_rules_save_and_render_compact_wide_and_missing_city";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            fixture: true,
            ..Default::default()
        },
    ) {
        return;
    }
    use crate::automation::{
        ProfileRef,
        solar::{Event, Source},
    };
    use automation::{DraftEdit, Kind, Message as Edit};
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    send_settings(&mut app, Message::Page(Page::DesktopAutomation));
    let weather_before = app.config.location.clone();
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        native_send(
            &mut app,
            &mut native,
            Message::Automation(Edit::New(Kind::Solar)),
        );
        native_send(
            &mut app,
            &mut native,
            Message::Automation(Edit::Draft(DraftEdit::Source(Source::Manual))),
        );
        native_send(
            &mut app,
            &mut native,
            Message::Automation(Edit::Draft(DraftEdit::Event(Event::GoldenHour))),
        );
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("solar-editor-empty-{}", size.width),
        );
        native_send(
            &mut app,
            &mut native,
            Message::Automation(Edit::ManualCity(Location {
                name: "Zagreb".into(),
                latitude: 45.815,
                longitude: 15.982,
                timezone: Some("Europe/Zagreb".into()),
            })),
        );
        native_send(
            &mut app,
            &mut native,
            Message::Automation(Edit::Draft(DraftEdit::Profile(ProfileRef::Moment(
                crate::moments::Phase::Night,
            )))),
        );
        for (theme_name, theme) in [
            ("dark", cosmic::Theme::dark()),
            ("light", cosmic::Theme::light()),
        ] {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("solar-editor-{}-{theme_name}", size.width),
            );
        }
        let bounds = native.bounds(app.settings_view(), "automation-save");
        assert!(
            bounds.y >= 0. && bounds.y + bounds.height <= size.height,
            "Save must be visible without scrolling: {bounds:?}"
        );
        use cosmic::iced::{Event as UiEvent, mouse};
        for event in [
            mouse::Event::CursorMoved {
                position: bounds.center(),
            },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            for message in native
                .event(app.settings_view(), UiEvent::Mouse(event), bounds.center())
                .0
            {
                native_send(&mut app, &mut native, message);
            }
        }
        assert!(
            app.automation.draft.is_none(),
            "Save must work through the native button"
        );
        assert!(app.automation.settings().unwrap().enabled);
        assert!(matches!(
            app.automation
                .settings()
                .unwrap()
                .rules
                .last()
                .unwrap()
                .when,
            crate::automation::Trigger::Solar {
                event: Event::GoldenHour,
                source: Source::Manual,
                ..
            }
        ));
        native.scroll(app.settings_view(), "settings-scroll", 0.);
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("solar-rules-{}", size.width),
        );
        assert_eq!(app.config.location, weather_before);
    }
}

#[test]
fn time_of_day_one_page_switches_whole_look_or_widget_style() {
    let name = "time_of_day_one_page_switches_whole_look_or_widget_style";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            fixture: true,
            ..Default::default()
        },
    ) {
        return;
    }
    use crate::automation::{ProfileRef, Trigger, solar::Event};
    use crate::moments::Phase;
    use automation::Message as Edit;
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    // The old Day and night style page opens Time of day, ready for widgets.
    send_settings(&mut app, Message::Page(Page::Moments));
    assert_eq!(app.page, Page::DesktopAutomation);
    assert_eq!(Page::Moments.sidebar(), Page::DesktopProfiles);
    assert_eq!(Page::DesktopAutomation.nav_title(), "Time of day");
    let city = City {
        name: "Sydney".into(),
        latitude: -33.87,
        longitude: 151.21,
        timezone: Some("Australia/Sydney".into()),
        admin1: "NSW".into(),
        country: "Australia".into(),
    };
    send_settings(&mut app, Message::Automation(Edit::ManualCity(city.location())));
    // Widget style only was chosen first, so adding a city starts the day
    // and night styles; choosing it again changes nothing.
    assert_eq!(app.automation.settings().unwrap().rules.len(), 2);
    send_settings(&mut app, Message::Automation(Edit::WidgetStyle(true)));
    let settings = app.automation.settings().unwrap().clone();
    let daily = |event| {
        settings
            .rules
            .iter()
            .find(|rule| matches!(rule.when, Trigger::Solar { event: e, .. } if e == event))
            .map(|rule| rule.profile.clone())
    };
    assert_eq!(daily(Event::Sunrise), Some(ProfileRef::Moment(Phase::Day)));
    assert_eq!(daily(Event::GoldenHour), None);
    assert_eq!(daily(Event::Sunset), Some(ProfileRef::Moment(Phase::Night)));
    assert!(settings.enabled);
    assert!(app.time_widget_mode(&settings));
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        for id in ["automation-enable", "time-of-day-mode", "moment-activate", "moment-preview"] {
            assert!(native.try_bounds(app.settings_view(), id).is_some(), "{id}");
        }
        assert!(native.try_bounds(app.settings_view(), "automation-shades").is_none());
        for (theme, name) in [(cosmic::Theme::light(), "light"), (cosmic::Theme::dark(), "dark")] {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("time-of-day-widgets-{}-{name}", size.width),
            );
        }
    }
    // Turning it off keeps the choices for later.
    send_settings(&mut app, Message::Automation(Edit::Enable(false)));
    let settings = app.automation.settings().unwrap().clone();
    assert!(!settings.enabled);
    assert_eq!(settings.rules.len(), 2);
    send_settings(&mut app, Message::Automation(Edit::Enable(true)));
    // Whole look: the widget styles leave the daily slots for looks.
    send_settings(&mut app, Message::Automation(Edit::WidgetStyle(false)));
    let settings = app.automation.settings().unwrap().clone();
    assert!(
        !settings.rules.iter().any(|rule| matches!(rule.profile, ProfileRef::Moment(_))),
        "{:?}",
        settings.rules
    );
    assert!(!app.time_widget_mode(&settings));
    let look = app.automation.look_choices.first().unwrap().0.clone();
    send_settings(&mut app, Message::Automation(Edit::Moment(Event::Sunrise, Some(look.clone()))));
    let settings = app.automation.settings().unwrap().clone();
    assert!(!app.time_widget_mode(&settings));
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        assert!(native.try_bounds(app.settings_view(), "automation-shades").is_some());
        assert!(native.try_bounds(app.settings_view(), "moment-activate").is_none());
        for (theme, name) in [(cosmic::Theme::light(), "light"), (cosmic::Theme::dark(), "dark")] {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("time-of-day-looks-{}-{name}", size.width),
            );
        }
    }
    // Switching back and forth puts each mode's choices back, as they were.
    let id = app.automation.settings().unwrap().rules[0].id.clone();
    send_settings(&mut app, Message::Automation(Edit::Rule(id, false)));
    send_settings(&mut app, Message::Automation(Edit::WidgetStyle(true)));
    let settings = app.automation.settings().unwrap().clone();
    assert!(app.time_widget_mode(&settings));
    assert!(!settings.rules.iter().any(|rule| rule.profile == look), "{:?}", settings.rules);
    assert_eq!(settings.rules.len(), 2);
    send_settings(&mut app, Message::Automation(Edit::WidgetStyle(false)));
    let settings = app.automation.settings().unwrap().clone();
    assert!(!app.time_widget_mode(&settings));
    assert_eq!(
        settings.rules.iter().map(|rule| rule.profile.clone()).collect::<Vec<_>>(),
        vec![look.clone()],
        "the look for the day came back"
    );
    assert!(!settings.rules[0].enabled, "a slot turned off stays off");
    send_settings(&mut app, Message::Automation(Edit::Rule(settings.rules[0].id.clone(), true)));
    // Emptying the only slot keeps the page in the mode being edited.
    send_settings(&mut app, Message::Automation(Edit::Moment(Event::Sunrise, None)));
    let settings = app.automation.settings().unwrap().clone();
    assert!(settings.rules.is_empty());
    assert!(!app.time_widget_mode(&settings));
    // A widget style rule on another trigger doesn't stop the daily looks.
    send_settings(&mut app, Message::Automation(Edit::Moment(Event::Sunrise, Some(look.clone()))));
    app.automation.widget_mode = None;
    let mut mixed = app.automation.settings().unwrap().clone();
    mixed.rules.push(crate::automation::Rule {
        id: mixed.next_id(),
        enabled: true,
        since: 0,
        profile: ProfileRef::Moment(Phase::Night),
        name: "Night style".into(),
        when: Trigger::Solar {
            event: Event::Sunset,
            days: crate::automation::Days::WEEKDAYS,
            source: crate::automation::solar::Source::Manual,
        },
    });
    assert!(!app.automation.widget_style(&mixed), "daily looks decide the mode");
    // Looks opens Time of day from a card at the top.
    send_settings(&mut app, Message::Page(Page::DesktopProfiles));
    let size = Size::new(1280., 800.);
    let mut native = crate::native_test::Native::new(size);
    let card = native.bounds(app.settings_view(), "presets-schedules");
    assert!(card.y < 300., "the card sits at the top: {card:?}");
    native.capture(app.settings_view(), &cosmic::Theme::light(), "time-of-day-looks-card");
}
