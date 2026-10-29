use super::*;
use crate::native_test::Native;
use cosmic::Application;
use cosmic::iced::{Event, mouse};

fn child(name: &str) -> bool {
    if std::env::var("DESKTOP_CARDS_ONBOARDING_TEST").as_deref() == Ok(name) {
        return false;
    }
    let root = std::env::temp_dir().join(format!("cards-onboarding-{}-{name}", std::process::id()));
    for dir in ["config", "data", "cache", "state", "desktop"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join("dconf-profile"), "").unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &format!("settings::onboarding_tests::{name}")])
        .env("DESKTOP_CARDS_ONBOARDING_TEST", name)
        .env("DESKTOP_CARDS_CONFIG", root.join("config.json"))
        .env("DESKTOP_CARDS_DESKTOP_ROOT", root.join("desktop"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("DCONF_PROFILE", root.join("dconf-profile"))
        .output()
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    true
}

fn click(app: &mut Settings, native: &mut Native, id: &str) {
    // Checklist and connection details can extend beyond the short viewport.
    // Reveal them through the actual page scrollable before asserting fit.
    native.scroll(app.settings_view(), "settings-content", 0.);
    let target = native.bounds(app.settings_view(), id);
    if target.y + target.height > native.size.height - 64. {
        native.scroll(
            app.settings_view(),
            "settings-content",
            (target.y - 240.).max(0.),
        );
    }
    let bounds = native.bounds(app.settings_view(), id);
    assert!(
        bounds.x >= 0.
            && bounds.x + bounds.width <= native.size.width
            && bounds.y >= 0.
            && bounds.y + bounds.height <= native.size.height,
        "{id}: {bounds:?}"
    );
    let point = bounds.center();
    for input in [
        mouse::Event::CursorMoved { position: point },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        for message in native
            .event(app.settings_view(), Event::Mouse(input), point)
            .0
        {
            let _ = app.update(message);
        }
    }
}

#[test]
fn fresh_user_setup_routes_to_weather_calendar_and_local_widgets() {
    if child("fresh_user_setup_routes_to_weather_calendar_and_local_widgets") {
        return;
    }
    for size in [Size::new(820., 600.), Size::new(1024., 720.)] {
        Config::save_changes(json!({"setup_dismissed":false,"location":null,"project_roots":[]}))
            .unwrap();
        let (mut app, _) = Settings::init(Core::default(), Page::Overview);
        app.calendar.connection = crate::calendar::Connection::default();
        app.calendar.client_ready = false;
        let _ = app.update(Message::Service(Ok(ServiceState {
            running: true,
            startup: false,
        })));
        let _ = app.update(Message::Resized(size));
        assert!(app.needs_setup());
        let mut native = Native::new(size);
        native.capture(
            app.settings_view(),
            &controls_theme(&app.config),
            &format!("new-user-home-{}", size.width),
        );
        let canvas = native.bounds(app.settings_view(), "placement-canvas");
        assert!(canvas.y >= 0. && canvas.y + canvas.height <= size.height);
        click(&mut app, &mut native, "setup-open");
        assert!(!app.placement_active());
        native.capture(
            app.settings_view(),
            &controls_theme(&app.config),
            &format!("new-user-setup-{}", size.width),
        );
        click(&mut app, &mut native, "setup-step-2");
        assert_eq!(app.page, Page::Weather);
        let _ = app.update(Message::Page(Page::Overview));
        click(&mut app, &mut native, "setup-open");
        click(&mut app, &mut native, "setup-step-3");
        assert_eq!(
            app.page,
            Page::Themes,
            "setup opens Appearance's theme choices"
        );
        let _ = app.update(Message::Page(Page::Overview));
        click(&mut app, &mut native, "setup-open");
        click(&mut app, &mut native, "setup-step-4");
        assert_eq!(app.page, Page::Calendar);
        native.capture(
            app.settings_view(),
            &controls_theme(&app.config),
            &format!("new-user-unconfigured-{}", size.width),
        );
        std::fs::write(
            crate::calendar::account_path().unwrap(),
            serde_json::to_vec(&app.calendar.connection).unwrap(),
        )
        .unwrap();
        app.calendar.client_ready = true;
        native.capture(
            app.settings_view(),
            &controls_theme(&app.config),
            &format!("new-user-connect-{}", size.width),
        );
        if app.scene_open() {
            click(&mut app, &mut native, "scene-toggle");
        }
        click(&mut app, &mut native, "calendar-connect");
        assert!(app.calendar.busy && app.calendar.connecting);
        let _ = app.update(Message::Calendar(calendar::Message::Cancel));
        assert!(!app.calendar.busy);
        let _ = app.update(Message::Page(Page::Overview));
        click(&mut app, &mut native, "setup-open");
        click(&mut app, &mut native, "setup-back");
        assert_eq!(app.page, Page::Overview);
        assert!(
            !app.placement_active(),
            "Returning to the overview must not start an edit"
        );
        let before_layout = wizard::widgets::raw().unwrap();
        click(&mut app, &mut native, "canvas-start");
        assert!(
            app.placement_active() && app.canvas.is_some(),
            "Edit layout must open the draft editor"
        );
        click(&mut app, &mut native, "canvas-cancel");
        assert!(!app.placement_active() && app.canvas.is_none());
        assert_eq!(
            wizard::widgets::raw().unwrap(),
            before_layout,
            "Opening and cancelling the layout editor must preserve saved preferences"
        );
        assert!(!Config::load().setup_dismissed);
        click(&mut app, &mut native, "setup-open");
        click(&mut app, &mut native, "setup-skip");
        assert!(!app.needs_setup());
        assert!(Config::load().setup_dismissed);
        let _ = app.update(Message::SetupDismissed(false));
        assert!(app.needs_setup());
    }
}

#[test]
fn connected_account_selector_and_errors_fit_and_reject_old_responses() {
    if child("connected_account_selector_and_errors_fit_and_reject_old_responses") {
        return;
    }
    for size in [Size::new(820., 600.), Size::new(1024., 720.)] {
        let (mut app, _) = Settings::init(Core::default(), Page::Calendar);
        app.calendar.connection = crate::calendar::Connection {
            backend: crate::calendar::Backend::Google,
            account: "alex@example.com".into(),
            calendar_id: "primary".into(),
            calendar_name: "Personal".into(),
            revision: "new".into(),
            credential: "fixture".into(),
            ..Default::default()
        };
        std::fs::write(
            crate::calendar::account_path().unwrap(),
            serde_json::to_vec(&app.calendar.connection).unwrap(),
        )
        .unwrap();
        app.calendar.client_ready = true;
        let choices = vec![
            crate::calendar::CalendarChoice {
                id: "primary".into(),
                name: "Personal".into(),
                primary: true,
                color: String::new(),
            },
            crate::calendar::CalendarChoice {
                id: "team".into(),
                name: "Team calendar".into(),
                primary: false,
                color: String::new(),
            },
        ];
        let _ = app.update(Message::Calendar(calendar::Message::Loaded(
            "old".into(),
            Ok(choices.clone()),
        )));
        assert!(app.calendar.choices.is_empty());
        let _ = app.update(Message::Calendar(calendar::Message::Loaded(
            "new".into(),
            Ok(choices),
        )));
        assert_eq!(
            app.calendar.connection.selected_calendars()[0].id,
            "primary"
        );
        let _ = app.update(Message::Resized(size));
        let mut native = Native::new(size);
        native.capture(
            app.settings_view(),
            &controls_theme(&app.config),
            &format!("new-user-connected-{}", size.width),
        );
        click(&mut app, &mut native, "scene-toggle");
        assert!(!app.scene_open());
        click(&mut app, &mut native, "calendar-account-options");
        click(&mut app, &mut native, "calendar-disconnect");
        assert!(app.calendar.confirm_disconnect);
        assert!(
            app.calendar.connection.connected(),
            "confirmation alone does not disconnect"
        );
        let _ = app.update(Message::Calendar(calendar::Message::ConfirmDisconnect(
            false,
        )));
        let _ = app.update(Message::Calendar(calendar::Message::Loaded(
            "new".into(),
            Err(crate::calendar::Problem::SignIn),
        )));
        native.capture(
            app.settings_view(),
            &controls_theme(&app.config),
            &format!("new-user-reconnect-{}", size.width),
        );
        assert!(
            app.calendar.connection.connected(),
            "failure preserves the previous account"
        );
    }
}

#[test]
fn polish050_saved_selection_and_optional_followups_survive_hiding_and_restart() {
    if child("polish050_saved_selection_and_optional_followups_survive_hiding_and_restart") {
        return;
    }
    Config::save_changes(json!({
        "onboarded": true, "setup_dismissed": true, "widgets_visible": true,
        "enabled_widgets": ["weather", "calendar", "projects"], "glance": {"enabled": false},
        "location": null, "project_roots": [], "future_preference": {"keep": 42},
        "notes": {"enabled": false, "text": "keep my note"}
    }))
    .unwrap();
    let size = Size::new(820., 600.);
    let (mut app, _) = Settings::init(Core::default(), Page::Widgets);
    app.calendar.connection = crate::calendar::Connection::default();
    let _ = app.update(Message::Resized(size));
    let mut native = Native::new(size);
    assert!(
        app.needs_setup(),
        "Finish must not erase selected missing-content tasks"
    );
    assert_eq!(app.setup_followups.pending(&app.config, false).len(), 3);
    native.capture(
        app.settings_view(),
        &controls_theme(&app.config),
        "polish050-widget-followups-820",
    );
    click(&mut app, &mut native, "setup-task-dismiss-weather");
    assert!(
        !app.error,
        "Dismissal must save through the production dispatcher: {}",
        app.status
    );
    assert!(
        !app.setup_followups
            .pending(&app.config, false)
            .contains(&wizard::follow_up::Task::Weather)
    );
    let raw = wizard::widgets::raw().unwrap();
    assert_eq!(raw["future_preference"]["keep"], 42);
    assert_eq!(raw["notes"]["text"], "keep my note");
    assert_eq!(raw["setup_followup_weather_dismissed"], true);
    let (mut app, _) = Settings::init(Core::default(), Page::Widgets);
    app.calendar.connection = crate::calendar::Connection::default();
    let _ = app.update(Message::Resized(size));
    assert_eq!(
        app.setup_followups.pending(&app.config, false),
        vec![
            wizard::follow_up::Task::Calendar,
            wizard::follow_up::Task::Projects
        ]
    );
    click(&mut app, &mut native, "widgets-show-all");
    assert!(app.config.widget_selected(Widget::Weather));
    assert!(!app.config.data_visible(Widget::Weather));
    assert!(app.setup_followups.pending(&app.config, false).is_empty());
    native.capture(
        app.settings_view(),
        &controls_theme(&app.config),
        "polish050-widgets-hidden-820",
    );
    let _ = app.update(Message::Widget(Widget::Calendar, false));
    assert!(!app.config.widget_selected(Widget::Calendar));
    assert!(
        !app.config.widgets_visible,
        "An individual choice cannot undo Hide all"
    );
    click(&mut app, &mut native, "widgets-show-all");
    assert_eq!(
        app.setup_followups.pending(&app.config, false),
        vec![wizard::follow_up::Task::Projects]
    );
    click(&mut app, &mut native, "setup-task-projects");
    assert_eq!(app.page, Page::Projects);
    assert!(
        native
            .try_bounds(app.settings_view(), "project-choose-folders")
            .is_some()
    );
}
