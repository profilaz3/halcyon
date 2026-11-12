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
fn fresh_user_home_offers_setup_once_and_says_when_widgets_are_stopped() {
    if child("fresh_user_home_offers_setup_once_and_says_when_widgets_are_stopped") {
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
        // One card, no checklist, and no banner while the widgets run.
        assert!(
            native
                .try_bounds(app.settings_view(), "setup-open")
                .is_some()
        );
        assert!(
            native
                .try_bounds(app.settings_view(), "updated-whats-new")
                .is_none()
        );
        assert!(
            native
                .try_bounds(app.settings_view(), "setup-step-1")
                .is_none()
        );
        assert!(
            native
                .try_bounds(app.settings_view(), "widgets-start")
                .is_none()
        );
        let canvas = native.bounds(app.settings_view(), "desktop-scene");
        assert!(canvas.y >= 0. && canvas.y + canvas.height <= size.height);
        assert!(!app.placement_active());

        // Stopped: Home says so. Set up… is the card right above it.
        let _ = app.update(Message::Service(Ok(ServiceState {
            running: false,
            startup: false,
        })));
        native.capture(
            app.settings_view(),
            &controls_theme(&app.config),
            &format!("new-user-stopped-{}", size.width),
        );
        assert!(
            native
                .try_bounds(app.settings_view(), "widgets-start")
                .is_some()
        );
        assert!(
            native
                .try_bounds(app.settings_view(), "widgets-setup")
                .is_none()
        );
        let before = std::fs::read(Config::path().unwrap()).unwrap();
        // Tests never start the real service: the preview seam turns it away.
        click(&mut app, &mut native, "widgets-start");
        assert!(!app.busy, "{}", app.status);
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);

        // Looks says the same and offers setup.
        let _ = app.update(Message::Page(Page::DesktopProfiles));
        assert!(
            native
                .try_bounds(app.settings_view(), "widgets-start")
                .is_some()
        );
        click(&mut app, &mut native, "widgets-setup");
        assert!(app.wizard.is_some());
        // Skipping leaves setup unfinished, so Home still offers it.
        click(&mut app, &mut native, "wizard-leave");
        assert!(app.wizard.is_none());
        assert_eq!(app.page, Page::Overview);
        assert!(!Config::load().onboarded);
        assert!(app.needs_setup());
        let _ = app.update(Message::Service(Ok(ServiceState {
            running: true,
            startup: false,
        })));

        let _ = app.update(Message::Page(Page::Calendar));
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
        // The preview is always shown; the connect button is reached by scrolling.
        click(&mut app, &mut native, "calendar-connect");
        assert!(app.calendar.busy && app.calendar.connecting);
        let _ = app.update(Message::Calendar(calendar::Message::Cancel));
        assert!(!app.calendar.busy);
        let _ = app.update(Message::Page(Page::Overview));
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
        click(&mut app, &mut native, "setup-dismiss");
        assert!(!app.needs_setup());
        assert!(Config::load().setup_dismissed);
        assert!(
            native
                .try_bounds(app.settings_view(), "setup-open")
                .is_none()
        );
        // A first run has nothing new to tell: setting setup aside records
        // this version, so no update card follows it.
        assert_eq!(
            crate::settings::release_notes::seen().as_deref(),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert!(
            native
                .try_bounds(app.settings_view(), "updated-whats-new")
                .is_none()
        );
        let _ = app.update(Message::SetupDismissed(false));
        assert!(app.needs_setup());
        click(&mut app, &mut native, "setup-open");
        assert!(app.wizard.is_some(), "Start opens the setup wizard");
        let _ = app.update(Message::Wizard(wizard::Message::Leave));
    }
}

/// Google sign-in can become available while the Calendar page is open. The
/// Connect button then appears in front of the link button in the same row,
/// and the next frame must lay out both instead of panicking.
#[test]
fn calendar_page_survives_google_sign_in_becoming_available() {
    if child("calendar_page_survives_google_sign_in_becoming_available") {
        return;
    }
    for size in [Size::new(820., 600.), Size::new(1024., 720.)] {
        let (mut app, _) = Settings::init(Core::default(), Page::Calendar);
        app.calendar.connection = crate::calendar::Connection::default();
        app.calendar.client_ready = false;
        let _ = app.update(Message::Resized(size));
        let mut native = Native::new(size);
        assert!(
            native
                .try_bounds(app.settings_view(), "calendar-connect")
                .is_none()
        );
        let link = native.bounds(app.settings_view(), "calendar-subscription-open");
        app.calendar.client_ready = true;
        let connect = native.bounds(app.settings_view(), "calendar-connect");
        let moved = native.bounds(app.settings_view(), "calendar-subscription-open");
        assert!(
            connect.width > 0. && moved.width >= link.width,
            "{connect:?} {moved:?}"
        );
        click(&mut app, &mut native, "calendar-connect");
        assert!(app.calendar.busy && app.calendar.connecting);
        let _ = app.update(Message::Calendar(calendar::Message::Cancel));
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
        // The preview is always shown; account options are reached by scrolling.
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
fn upgrade_home_names_whats_new_since_the_last_version_seen() {
    if child("upgrade_home_names_whats_new_since_the_last_version_seen") {
        return;
    }
    let running = env!("CARGO_PKG_VERSION");
    let open = |seen: Option<&str>, size: Size| {
        Config::save_changes(json!({"onboarded": true, "setup_dismissed": true})).unwrap();
        let store = crate::desktop::DesktopStore::from_env().unwrap();
        std::fs::create_dir_all(store.state_dir()).unwrap();
        let file = store.state_dir().join("release-notes-seen");
        match seen {
            Some(version) => std::fs::write(&file, version).unwrap(),
            None => drop(std::fs::remove_file(&file)),
        }
        let (mut app, _) = Settings::init(Core::default(), Page::Overview);
        app.calendar.connection = crate::calendar::Connection::default();
        let _ = app.update(Message::Service(Ok(ServiceState {
            running: true,
            startup: false,
        })));
        let _ = app.update(Message::Resized(size));
        app
    };
    // Upgraded from a version that never recorded its notes: this one's.
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        let mut app = open(None, size);
        let mut native = Native::new(size);
        let card = native.bounds(app.settings_view(), "updated-whats-new");
        assert!(
            card.y >= 0. && card.y + card.height <= size.height,
            "{card:?}"
        );
        native.text_bounds(
            app.settings_view(),
            &format!("Halcyon was updated to {running}"),
        );
        let notes = crate::settings::release_notes::current().unwrap().notes;
        let summary = match notes.len() - 1 {
            0 => notes[0].title.to_string(),
            more => format!("{}, and {more} more", notes[0].title),
        };
        native.text_bounds(app.settings_view(), &summary);
        for (theme, name) in [
            (cosmic::Theme::light(), "light"),
            (cosmic::Theme::dark(), "dark"),
        ] {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("on05-updated-card-{}-{name}", size.width),
            );
        }
        click(&mut app, &mut native, "updated-whats-new");
        assert_eq!(app.app_settings_page, AppSettingsPage::WhatsNew);
        assert_eq!(
            crate::settings::release_notes::seen().as_deref(),
            Some(running)
        );
        native.text_bounds(app.settings_view(), &format!("Halcyon {running}"));
        let _ = app.update(Message::Page(Page::Overview));
        assert!(
            native
                .try_bounds(app.settings_view(), "updated-whats-new")
                .is_none()
        );
    }
    // Several releases behind: every one in between, newest first.
    let size = Size::new(1280., 800.);
    let mut app = open(Some("0.5.0"), size);
    let mut native = Native::new(size);
    let all = crate::settings::release_notes::since(Some("0.5.0"));
    assert!(all.len() >= 2);
    let count: usize = all.iter().map(|release| release.notes.len()).sum();
    native.text_bounds(
        app.settings_view(),
        &format!("{}, and {} more", all[0].notes[0].title, count - 1),
    );
    let _ = app.update(Message::AppSettingsPage(AppSettingsPage::WhatsNew));
    let mut tops = Vec::new();
    for release in &all {
        let heading = format!("Halcyon {}", release.version);
        tops.push(native.text_bounds(app.settings_view(), &heading).y);
    }
    assert!(tops.is_sorted(), "{tops:?}");
    for (theme, name) in [
        (cosmic::Theme::light(), "light"),
        (cosmic::Theme::dark(), "dark"),
    ] {
        native.capture(
            app.settings_view(),
            &theme,
            &format!("on05-whats-new-since-{name}"),
        );
    }
    // The statistics address opens in the browser.
    let linked = all
        .iter()
        .flat_map(|release| release.notes)
        .position(|note| note.link.is_some())
        .unwrap();
    assert!(
        native
            .try_bounds(app.settings_view(), &format!("whats-new-link-{linked}"))
            .is_some()
    );
    // Dismissing records the version without opening the notes.
    let mut app = open(Some("0.5.0"), Size::new(820., 600.));
    let mut native = Native::new(Size::new(820., 600.));
    click(&mut app, &mut native, "updated-dismiss");
    assert_eq!(app.page, Page::Overview);
    assert_eq!(
        crate::settings::release_notes::seen().as_deref(),
        Some(running)
    );
    assert!(
        native
            .try_bounds(app.settings_view(), "updated-whats-new")
            .is_none()
    );
    // Already seen: nothing on Home.
    let app = open(Some(running), size);
    let mut native = Native::new(size);
    assert!(
        native
            .try_bounds(app.settings_view(), "updated-whats-new")
            .is_none()
    );
}

/// The tasks the Done step lists for widgets that still need content.
fn pending(app: &Settings) -> Vec<wizard::follow_up::Task> {
    wizard::follow_up::Task::ALL
        .into_iter()
        .filter(|task| task.needed(&app.config, false))
        .collect()
}

#[test]
fn polish050_saved_selection_survives_hiding_and_restart() {
    if child("polish050_saved_selection_survives_hiding_and_restart") {
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
    // Finished setup offers nothing more on Home.
    assert!(!app.needs_setup());
    assert_eq!(pending(&app).len(), 3);
    assert!(native.duplicate_ids(app.settings_view()).is_empty());
    native.capture(
        app.settings_view(),
        &controls_theme(&app.config),
        "polish050-widget-followups-820",
    );
    let (mut app, _) = Settings::init(Core::default(), Page::Widgets);
    app.calendar.connection = crate::calendar::Connection::default();
    let _ = app.update(Message::Resized(size));
    click(&mut app, &mut native, "widgets-show-all");
    assert!(app.config.widget_selected(Widget::Weather));
    assert!(!app.config.data_visible(Widget::Weather));
    assert!(pending(&app).is_empty());
    let raw = wizard::widgets::raw().unwrap();
    assert_eq!(raw["future_preference"]["keep"], 42);
    assert_eq!(raw["notes"]["text"], "keep my note");
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
        pending(&app),
        vec![
            wizard::follow_up::Task::Weather,
            wizard::follow_up::Task::Projects
        ]
    );
    let _ = app.update(Message::Page(wizard::follow_up::Task::Projects.page()));
    assert_eq!(app.page, Page::Projects);
    assert!(
        native
            .try_bounds(app.settings_view(), "project-choose-folders")
            .is_some()
    );
}

#[test]
fn arrange_widgets_opens_the_desktop_editor_and_falls_back_to_settings() {
    if child("arrange_widgets_opens_the_desktop_editor_and_falls_back_to_settings") {
        return;
    }
    // Settings' editor drafts from the saved settings.
    std::fs::write(Config::path().unwrap(), "{}").unwrap();
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        let (mut app, _) = Settings::init(Core::default(), Page::Overview);
        let _ = app.update(Message::Resized(size));
        let _ = app.update(Message::Service(Ok(ServiceState {
            running: true,
            startup: false,
        })));
        let mut native = Native::new(size);
        assert!(desktop_arrange_available(&app.config));
        // Arrange widgets asks the desktop for its editor; Settings opens
        // none of its own.
        click(&mut app, &mut native, "arrange-widgets");
        assert!(app.busy && app.canvas.is_none(), "{}", app.status);
        // The desktop couldn't open it: Settings' editor opens instead.
        let _ = app.update(Message::ArrangeHere("no desktop".into()));
        assert!(
            !app.busy && app.canvas.is_some() && !app.error,
            "{}",
            app.status
        );
        click(&mut app, &mut native, "canvas-cancel");
        assert!(app.canvas.is_none());
        // Search reaches the desktop editor the same way.
        let hit = navigation::search("arrange widgets")[0];
        let _ = app.update(Message::SearchResult(hit));
        assert!(app.busy && app.canvas.is_none(), "{}", app.status);
        let _ = app.update(Message::CommandDone("done".into(), Ok(())));
        // The keyboard link opens the list in Settings directly.
        let _ = app.update(Message::Page(Page::Overview));
        click(&mut app, &mut native, "canvas-start");
        assert!(app.canvas.is_some() && !app.busy);
        click(&mut app, &mut native, "canvas-cancel");
        // Nothing on the desktop to arrange there: Settings' editor.
        app.config.enabled_widgets.clear();
        assert!(!desktop_arrange_available(&app.config));
        let _ = app.update(Message::ArrangeWidgets);
        assert!(app.canvas.is_some() && !app.busy);
        let _ = app.update(Message::Canvas(canvas::Message::Cancel));
    }
}
