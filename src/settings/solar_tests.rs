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
