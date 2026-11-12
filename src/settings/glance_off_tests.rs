// Turning Daily overview off never leaves the desktop without the time, and
// the Clock page can always bring the clock back.

/// Daily overview on with no separate widgets underneath (what an older look
/// left behind): turning it off brings the clock and weather back where they
/// were saved, as one Undo step with its own name.
#[test]
fn glance_off_with_nothing_underneath_brings_back_the_clock_as_one_step() {
    if desktop_test_child(
        "glance_off_with_nothing_underneath_brings_back_the_clock_as_one_step",
        DesktopEnv {
            app_config: true,
            fixture: true,
            ..Default::default()
        },
    ) {
        return;
    }
    let _root = isolated_root();
    Config::save_changes(json!({
        "widgets_visible": true,
        "enabled_widgets": [],
        "widget_positions": {"glance": [0.04, 0.09], "clock": [0.3, 0.2]},
        "widget_sizes": {"clock": [420, 200]},
        "glance": {"enabled": true, "initialized": true, "show_clock": true, "show_weather": true}
    }))
    .unwrap();
    let (mut app, task) = Settings::init(Core::default(), Page::Glance);
    settle_settings(&mut app, task);
    let store = crate::desktop::actions::history_store().unwrap();
    let changes = store.recent_changes().unwrap().entries.len();

    send_settings(&mut app, Message::Widget(Widget::Glance, false));
    assert!(!app.error, "{:?}", app.notice);
    let saved = Config::try_load().unwrap();
    assert!(!saved.glance.enabled);
    assert!(saved.widget_visible(Widget::Clock));
    assert!(saved.widget_visible(Widget::Weather));
    assert!(!saved.widget_visible(Widget::Calendar));
    let raw = raw_preferences();
    assert_eq!(raw["widget_positions"]["clock"], json!([0.3, 0.2]));
    assert_eq!(raw["widget_sizes"]["clock"], json!([420, 200]));
    assert_eq!(store.recent_changes().unwrap().entries.len(), changes + 1);
    assert_eq!(
        store.latest_change().unwrap().unwrap().label,
        "Daily overview off, clock and weather shown"
    );

    // One Undo puts Daily overview back, with nothing separate underneath.
    send_settings(&mut app, Message::UndoDesktop);
    assert!(!app.error, "{:?}", app.notice);
    let saved = Config::try_load().unwrap();
    assert!(saved.glance.enabled);
    assert!(
        saved.enabled_widgets.is_empty(),
        "{:?}",
        saved.enabled_widgets
    );
}

/// With separate widgets still selected, turning Daily overview off changes
/// only that one setting and shows what was underneath.
#[test]
fn glance_off_with_widgets_underneath_changes_nothing_else() {
    if desktop_test_child(
        "glance_off_with_widgets_underneath_changes_nothing_else",
        DesktopEnv {
            app_config: true,
            fixture: true,
            ..Default::default()
        },
    ) {
        return;
    }
    let _root = isolated_root();
    Config::save_changes(json!({
        "enabled_widgets": ["calendar", "devbox"],
        "glance": {"enabled": true, "initialized": true}
    }))
    .unwrap();
    let (mut app, task) = Settings::init(Core::default(), Page::Glance);
    settle_settings(&mut app, task);
    send_settings(&mut app, Message::Widget(Widget::Glance, false));
    assert!(!app.error, "{:?}", app.notice);
    let saved = Config::try_load().unwrap();
    assert_eq!(saved.enabled_widgets, ["calendar", "devbox"]);
    assert!(saved.widget_visible(Widget::Calendar));
    assert!(!saved.widget_visible(Widget::Clock));
    let store = crate::desktop::actions::history_store().unwrap();
    assert_ne!(
        store.latest_change().unwrap().unwrap().label,
        "Daily overview off, clock and weather shown"
    );
}

/// The state the bug left: Daily overview off and no widgets selected. The
/// Clock page, with its stage, still says “Show on desktop” and both the
/// heading switch and the row button bring the clock back.
#[test]
fn clock_page_shows_the_clock_again_when_nothing_is_selected() {
    if desktop_test_child(
        "clock_page_shows_the_clock_again_when_nothing_is_selected",
        DesktopEnv {
            app_config: true,
            fixture: true,
            icons: Icons::Shown,
            ..Default::default()
        },
    ) {
        return;
    }
    let _root = isolated_root();
    let lost = json!({
        "widgets_visible": true,
        "enabled_widgets": [],
        "glance": {"enabled": false, "initialized": true}
    });
    Config::save_changes(lost.clone()).unwrap();
    let (mut app, task) = Settings::init(Core::default(), Page::Clock);
    settle_settings(&mut app, task);
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        Config::save_changes(lost.clone()).unwrap();
        send_settings(&mut app, Message::Page(Page::Clock));
        app.config = Config::try_load().unwrap();
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        assert!(app.clock_stage_open());
        assert!(!app.config.widget_visible(Widget::Clock));
        // The heading's switch and the row that offers the one step.
        assert_eq!(
            native
                .text_all(app.settings_view(), "Show on desktop")
                .len(),
            2
        );
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        for (theme, mode) in [
            (cosmic::Theme::light(), "light"),
            (cosmic::Theme::dark(), "dark"),
        ] {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("glance-off-clock-page-{}-{mode}", size.width),
            );
        }
        native_click(&mut app, &mut native, "clock-show");
        assert!(!app.error, "{:?}", app.notice);
        assert!(Config::try_load().unwrap().widget_visible(Widget::Clock));
        assert!(app.config.widget_visible(Widget::Clock));
        // The clock is on the stage again, and the heading says so.
        assert!(app.clock_stage_place().is_some());
        assert_eq!(
            native
                .text_all(app.settings_view(), "Show on desktop")
                .len(),
            1
        );
    }
    // The heading switch sends the same change.
    Config::save_changes(lost).unwrap();
    app.config = Config::try_load().unwrap();
    send_settings(&mut app, Message::Widget(Widget::Clock, true));
    assert!(Config::try_load().unwrap().widget_visible(Widget::Clock));
}
