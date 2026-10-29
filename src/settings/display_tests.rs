use super::*;
use crate::displays::{
    OutputEvent,
    test_support::{Wire, info, isolated},
};
use cosmic::Application;

#[test]
fn monitor_pinning_settings_save_undo_and_failure() {
    if isolated("settings::display_tests::monitor_pinning_settings_save_undo_and_failure") {
        return;
    }
    let wire = Wire::new();
    let a = wire.output(1);
    let b = wire.output(2);
    let (mut app, _) = Settings::init(Core::default(), Page::WidgetLayout);
    let _ = app.update(Message::DisplayOutput(
        OutputEvent::Created(Some(info(Some("DP-1"), (1920, 1080)))),
        a,
    ));
    let _ = app.update(Message::DisplayOutput(
        OutputEvent::Created(Some(info(Some("DP-2"), (1920, 1080)))),
        b.clone(),
    ));
    let _ = app.update(Message::WidgetDisplay(Some("DP-2".into())));
    assert!(!app.error, "{}", app.status);
    assert_eq!(
        Config::try_load().unwrap().widget_display.as_deref(),
        Some("DP-2")
    );
    assert!(app.last_desktop_change.is_some(), "durable Undo must exist");
    // Unrelated concurrent edits must survive the actual transaction Undo.
    Config::save_changes(serde_json::json!({"clock":{"show_date":false},"future_preference":7}))
        .unwrap();
    let _ = app.update(Message::UndoPreferences);
    let saved = Config::try_load().unwrap();
    assert_eq!(saved.widget_display, None);
    assert!(!saved.clock.show_date);
    let _ = app.update(Message::WidgetDisplay(Some("DP-2".into())));
    let _ = app.update(Message::DisplayOutput(OutputEvent::Removed, b));
    assert!(
        app.displays
            .choices(app.config.widget_display.as_deref())
            .iter()
            .any(|(label, value)| label.contains("not connected")
                && value.as_deref() == Some("DP-2"))
    );
    assert!(app.displays.status(Some("DP-2")).contains("DP-1"));
    // Disconnected choice is retained; an obsolete menu action cannot pick it anew.
    let _ = app.update(Message::WidgetDisplay(Some("missing".into())));
    assert!(app.error);
    assert_eq!(
        Config::try_load().unwrap().widget_display.as_deref(),
        Some("DP-2")
    );
    let path = Config::path().unwrap();
    let before = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    let _ = app.update(Message::WidgetDisplay(None));
    assert!(app.error);
    assert_eq!(app.config.widget_display.as_deref(), Some("DP-2"));
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, before).unwrap();
}

#[test]
fn monitor_pinning_native_selector_and_ambiguous_names() {
    if isolated("settings::display_tests::monitor_pinning_native_selector_and_ambiguous_names") {
        return;
    }
    let wire = Wire::new();
    let a = wire.output(1);
    let b = wire.output(2);
    let (mut app, _) = Settings::init(Core::default(), Page::WidgetLayout);
    let _ = app.update(Message::DisplayOutput(
        OutputEvent::Created(Some(info(Some("DP-1"), (1920, 1080)))),
        a.clone(),
    ));
    let _ = app.update(Message::DisplayOutput(
        OutputEvent::Created(Some(info(Some("DP-2"), (1920, 1080)))),
        b.clone(),
    ));
    let mut native = crate::native_test::Native::new(cosmic::iced::Size::new(640., 240.));
    let bounds = native.bounds(app.display_selector(), "widget-display-DP-2");

    assert!(bounds.width > 40. && bounds.x + bounds.width <= 640.);
    let point = bounds.center();
    // Exercise the real selector: a button click must emit and save the choice.
    let mut emitted = false;
    for event in [
        cosmic::iced::mouse::Event::CursorMoved { position: point },
        cosmic::iced::mouse::Event::ButtonPressed(cosmic::iced::mouse::Button::Left),
        cosmic::iced::mouse::Event::ButtonReleased(cosmic::iced::mouse::Button::Left),
    ] {
        let (messages, _) = native.event(
            app.display_selector(),
            cosmic::iced::Event::Mouse(event),
            point,
        );
        for message in messages {
            emitted |= matches!(&message,Message::WidgetDisplay(Some(name)) if name=="DP-2");
            let _ = app.update(message);
        }
    }
    assert!(emitted);
    assert_eq!(
        Config::try_load().unwrap().widget_display.as_deref(),
        Some("DP-2")
    );
    let theme = cosmic::Theme::dark();
    native.capture_hover(
        app.display_selector(),
        &theme,
        point,
        "monitor-selector-connected",
    );
    for (width, height) in [(1024., 760.), (820., 600.)] {
        app.config.setup_dismissed = true;
        let _ = app.update(Message::Resized(cosmic::iced::Size::new(width, height)));
        let mut full = crate::native_test::Native::new(cosmic::iced::Size::new(width, height));
        let button = full.bounds(app.settings_view(), "widget-display-DP-2");
        assert!(
            button.x >= 0.
                && button.y >= 0.
                && button.x + button.width <= width
                && button.y + button.height <= height
        );
        full.capture(
            app.settings_view(),
            &theme,
            &format!("monitor-settings-{}", width as u32),
        );
    }
    let _ = app.update(Message::WidgetDisplay(Some("DP-2".into())));
    let _ = app.update(Message::DisplayOutput(OutputEvent::Removed, b.clone()));
    native.capture(
        app.display_selector(),
        &theme,
        "monitor-selector-disconnected",
    );
    let _ = app.update(Message::DisplayOutput(
        OutputEvent::Created(Some(info(Some("DP-1"), (1920, 1080)))),
        b,
    ));
    assert!(app.displays.unique("DP-1").is_none());
    assert!(
        !app.displays
            .choices(None)
            .iter()
            .any(|(_, v)| v.as_deref() == Some("DP-1"))
    );
    let unnamed = wire.output(3);
    let _ = app.update(Message::DisplayOutput(
        OutputEvent::Created(Some(info(None, (1920, 1080)))),
        unnamed,
    ));
    assert!(app.displays.status(None).contains("cannot be pinned"));
    native.capture(app.display_selector(), &theme, "monitor-selector-ambiguous");
    let _ = app.update(Message::WidgetDisplay(Some("DP-1".into())));
    assert!(app.error);
}

#[test]
fn monitor_pinning_config_validation_and_portable_boundary() {
    if isolated("settings::display_tests::monitor_pinning_config_validation_and_portable_boundary")
    {
        return;
    }
    assert!(
        crate::config::check_config_value(&serde_json::json!({}))
            .unwrap()
            .widget_display
            .is_none()
    );
    for value in [
        serde_json::json!(""),
        serde_json::json!("DP\n1"),
        serde_json::json!("x".repeat(129)),
        serde_json::json!(4),
    ] {
        assert!(
            crate::config::check_config_value(&serde_json::json!({"widget_display":value}))
                .is_err()
        );
    }
    Config::save_changes(serde_json::json!({"widget_display":"DP-2","future_key":{"value":42},"widget_positions":{"weather":[0.7,0.4]}})).unwrap();
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    let mut captured = crate::desktop::profiles::capture(&store, "Display regression").unwrap();
    let encoded = serde_json::to_string(&captured.profile).unwrap();
    assert!(!encoded.contains("widget_display"));
    assert!(!encoded.contains("DP-2"));
    let exported = crate::desktop::profiles::export(&captured.profile).unwrap();
    let mut imported = crate::desktop::profiles::import(&exported.bytes).unwrap();
    imported.widget_layout = Some(serde_json::json!({"widget_positions":{"weather":[0.2,0.2]}}));
    let plan = crate::desktop::profiles::plan(
        &store,
        &crate::desktop::profiles::Installed::default(),
        &imported,
        &[crate::desktop::profiles::Domain::WidgetLayout]
            .into_iter()
            .collect(),
    );
    crate::desktop::profiles::apply(&store, &plan).unwrap();
    assert_eq!(
        Config::try_load().unwrap().widget_display.as_deref(),
        Some("DP-2")
    );
    assert_eq!(
        Config::try_load().unwrap().widget_positions["weather"],
        [0.2, 0.2]
    );
    captured.profile.widget_layout = Some(serde_json::json!({"widget_display":"DP-1"}));
    assert!(captured.profile.validate().is_err());
    assert_eq!(
        Config::try_load().unwrap().widget_display.as_deref(),
        Some("DP-2")
    );
}

#[test]
fn multi_monitor_settings_assignment_preview_undo_and_layout() {
    if isolated(
        "settings::display_tests::multi_monitor_settings_assignment_preview_undo_and_layout",
    ) {
        return;
    }
    use crate::drawer::Widget;
    Config::save_changes(serde_json::json!({"setup_dismissed":true,"glance":{"enabled":false},"enabled_widgets":["weather","calendar"]})).unwrap();
    let wire = Wire::new();
    let a = wire.output(1);
    let b = wire.output(2);
    let (mut app, _) = Settings::init(Core::default(), Page::WidgetLayout);
    let mut ia = info(Some("eDP-1"), (1280, 720));
    ia.model = "Laptop".into();
    let mut ib = info(Some("DP-2"), (1920, 1080));
    ib.model = "External".into();
    ib.logical_position = Some((1280, 0));
    ib.scale_factor = 2;
    let _ = app.update(Message::DisplayOutput(OutputEvent::Created(Some(ia)), a));
    let _ = app.update(Message::DisplayOutput(
        OutputEvent::Created(Some(ib)),
        b.clone(),
    ));
    let mut native = crate::native_test::Native::new(cosmic::iced::Size::new(760., 300.));
    let bounds = native.bounds(
        app.widget_display_controls(Widget::Weather),
        "widget-monitor-weather-DP-2",
    );
    let point = bounds.center();
    let mut clicked = false;
    for event in [
        cosmic::iced::mouse::Event::CursorMoved { position: point },
        cosmic::iced::mouse::Event::ButtonPressed(cosmic::iced::mouse::Button::Left),
        cosmic::iced::mouse::Event::ButtonReleased(cosmic::iced::mouse::Button::Left),
    ] {
        let (messages, _) = native.event(
            app.widget_display_controls(Widget::Weather),
            cosmic::iced::Event::Mouse(event),
            point,
        );
        for message in messages {
            clicked |= matches!(message, Message::WidgetMonitor(Widget::Weather, _));
            let _ = app.update(message);
        }
    }
    assert!(clicked);
    assert!(!app.error, "{}", app.status);
    assert_eq!(
        Config::try_load().unwrap().widget_monitors["weather"]
            .display
            .as_deref(),
        Some("DP-2")
    );
    let _ = app.update(Message::UndoPreferences);
    assert!(
        Config::try_load()
            .unwrap()
            .widget_monitors
            .get("weather")
            .is_none_or(|w| w.display.is_none())
    );
    let _ = app.update(Message::WidgetMonitor(Widget::Weather, Some("DP-2".into())));
    let mut snapshot = crate::layout_state::read(&app.config);
    snapshot.monitors = [
        ("eDP-1".into(), [1280., 720.]),
        ("DP-2".into(), [1920., 1080.]),
    ]
    .into();
    snapshot.widget_displays = Widget::ALL
        .into_iter()
        .map(|w| {
            (
                w.key().into(),
                if w == Widget::Weather {
                    "DP-2"
                } else {
                    "eDP-1"
                }
                .into(),
            )
        })
        .collect();
    snapshot.display = Some(crate::layout_state::DisplayState {
        output: Some("eDP-1".into()),
        ready: true,
        epoch: 1,
    });
    crate::layout_state::publish(&snapshot, crate::layout_state::next_generation());
    let _ = app.update(Message::MonitorPreview("DP-2".into()));
    assert_eq!(app.placement.screen(), [1920., 1080.]);
    let before_layout = Config::try_load().unwrap().monitor_layouts;
    let _ = app.update(Message::Canvas(canvas::Message::Start));
    assert!(app.canvas.is_some(), "{}", app.status);
    let _ = app.update(Message::Placement(placement::Message::Select(
        "weather".into(),
    )));
    let _ = app.update(Message::Placement(placement::Message::Nudge(
        [-1, 0],
        false,
    )));
    assert!(app.canvas_dirty());
    assert_eq!(Config::try_load().unwrap().monitor_layouts, before_layout);
    let id = app.canvas.as_ref().unwrap().id;
    let _ = app.update(Message::Canvas(canvas::Message::Apply(id)));
    assert!(!app.error, "{}", app.status);
    assert!(app.canvas.is_none());
    let saved = Config::try_load().unwrap();
    assert!(saved.monitor_layouts["weather"].contains_key("DP-2"));
    assert!(!saved.widget_positions.contains_key("weather"));
    let _ = app.update(Message::UndoPreferences);
    let saved = Config::try_load().unwrap();
    assert!(
        saved
            .monitor_layouts
            .get("weather")
            .is_none_or(|m| !m.contains_key("DP-2"))
    );
    for (width, height) in [(820., 600.), (1280., 720.)] {
        let _ = app.update(Message::Resized(cosmic::iced::Size::new(width, height)));
        let mut capture = crate::native_test::Native::new(cosmic::iced::Size::new(width, height));
        capture.capture(
            app.settings_view(),
            &cosmic::theme::active(),
            &format!("multi-monitor-layout-{}", width as u32),
        );
    }
    // A disconnected hidden widget still has an accessible assignment control.
    let _ = app.update(Message::WidgetMonitorHide(Widget::Weather, true));
    let _ = app.update(Message::DisplayOutput(OutputEvent::Removed, b));
    // A hidden widget remains reachable from My widgets → Weather, even
    // though there is no card to click on the layout canvas.
    let _ = app.update(Message::Page(Page::Weather));
    let bounds = native.bounds(app.settings_view(), "widget-monitor-weather-default");
    assert!(bounds.width > 0. && bounds.height > 0.);
    app.placement.select("weather");
    let _ = native.render(app.placement_inspector(), &cosmic::theme::active());
}
