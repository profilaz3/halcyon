use super::*;
use crate::native_test::Native;
use cosmic::iced::{Event, mouse};

#[test]
fn usage_checkbox_native_save_restart_setup_undo_export_and_failure() {
    if crate::displays::test_support::isolated(
        "settings::privacy_tests::usage_checkbox_native_save_restart_setup_undo_export_and_failure",
    ) {
        return;
    }
    let (mut app, _) = Settings::init(Core::default(), Page::General);
    assert!(app.usage_enabled);
    assert!(
        !crate::usage::path().unwrap().exists(),
        "rendering must not create an identity"
    );
    let mut native = Native::new(Size::new(580., 330.));
    native.capture(
        app.privacy_section(),
        &cosmic::Theme::dark(),
        "usage-privacy-default-on",
    );
    let click = |app: &mut Settings, native: &mut Native| {
        let bounds = native.bounds(app.usage_control(), "share-usage-data");
        let point = bounds.center();
        let mut emitted = false;
        for event in [
            mouse::Event::CursorMoved { position: point },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            for message in native
                .event(app.usage_control(), Event::Mouse(event), point)
                .0
            {
                emitted |= matches!(message, Message::ShareUsage(_));
                super::tests::send_settings(app, message);
            }
        }
        assert!(emitted, "real checkbox must emit a choice");
    };
    app.native_busy = true; // an unrelated desktop write cannot defer opt-out
    click(&mut app, &mut native);
    app.native_busy = false;
    assert!(!app.usage_enabled);
    assert!(app.usage_error.is_none());
    assert!(!crate::usage::preference().unwrap());
    let path = crate::usage::path().unwrap();
    let off = std::fs::read(&path).unwrap();
    native.capture(
        app.privacy_section(),
        &cosmic::Theme::dark(),
        "usage-privacy-off",
    );
    for (width, height) in [(820., 600.), (1024., 760.)] {
        app.config.setup_dismissed = true;
        let _ = app.update(Message::Resized(Size::new(width, height)));
        let mut full = Native::new(Size::new(width, height));
        full.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("usage-settings-off-{}", width as u32),
        );
    }
    // Ordinary preference changes still preserve unknown source keys and their
    // own Undo; privacy is deliberately not in that transaction's scope.
    Config::save_changes(serde_json::json!({"future_key":{"retained":37}})).unwrap();
    super::tests::send_settings(&mut app, Message::SidebarSlide(false));
    super::tests::send_settings(&mut app, Message::UndoPreferences);
    assert_eq!(std::fs::read(&path).unwrap(), off);
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap();
    assert_eq!(saved["future_key"]["retained"], 37);
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    let captured = crate::desktop::profiles::capture(&store, "Privacy isolation").unwrap();
    let exported = crate::desktop::profiles::export(&captured.profile).unwrap();
    let mut imported = crate::desktop::profiles::import(&exported.bytes).unwrap();
    imported.widget_layout = Some(serde_json::json!({"widget_positions":{"weather":[0.2,0.3]}}));
    imported.widget_style = Some(serde_json::json!({"theme":"amber"}));
    let plan = crate::desktop::profiles::plan(
        &store,
        &crate::desktop::profiles::Installed::default(),
        &imported,
        &[
            crate::desktop::profiles::Domain::WidgetLayout,
            crate::desktop::profiles::Domain::WidgetStyle,
        ]
        .into_iter()
        .collect(),
    );
    crate::desktop::profiles::apply(&store, &plan).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), off);
    assert!(
        !serde_json::to_string(&captured.profile)
            .unwrap()
            .contains("installation")
    );
    let (mut reopened, _) = Settings::init(Core::default(), Page::General);
    assert!(!reopened.usage_enabled);
    super::tests::send_settings(&mut reopened, Message::OpenWizard);
    reopened.wizard.as_mut().unwrap().step = wizard::Step::Ready;
    assert!(
        !reopened.usage_enabled,
        "running setup again cannot reset optout"
    );
    for (width, height) in [(820., 600.), (1024., 760.)] {
        let _ = reopened.update(Message::Resized(Size::new(width, height)));
        let mut full = Native::new(Size::new(width, height));
        full.capture(
            reopened.settings_view(),
            &cosmic::Theme::dark(),
            &format!("usage-onboarding-off-{}", width as u32),
        );
    }
    // The same actual checkbox works inside the first-run native tree.
    let mut full = Native::new(Size::new(1024., 760.));
    let bounds = full.bounds(reopened.settings_view(), "share-usage-data");
    assert!(
        bounds.y >= 0. && bounds.y + bounds.height <= 760.,
        "{bounds:?}"
    );
    let point = bounds.center();
    let mut toggled = false;
    for event in [
        mouse::Event::CursorMoved { position: point },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        for message in full
            .event(reopened.settings_view(), Event::Mouse(event), point)
            .0
        {
            toggled |= matches!(message, Message::ShareUsage(true));
            super::tests::send_settings(&mut reopened, message);
        }
    }
    assert!(toggled);
    assert!(crate::usage::preference().unwrap());
    full.capture(
        reopened.settings_view(),
        &cosmic::Theme::dark(),
        "usage-onboarding-enabled",
    );
    // A held state lock must fail promptly, without a misleading saved checkbox.
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path.with_extension("lock"))
        .unwrap();
    use std::os::fd::AsRawFd;
    assert_eq!(
        unsafe { libc::flock(lock_file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    let began = Instant::now();
    click(&mut reopened, &mut native);
    assert!(began.elapsed() < Duration::from_secs(1));
    assert!(reopened.usage_enabled);
    assert!(reopened.usage_error.is_some());
    native.capture(
        reopened.privacy_section(),
        &cosmic::Theme::dark(),
        "usage-privacy-save-failed",
    );
    reopened.wizard = None;
    full.capture(
        reopened.settings_view(),
        &cosmic::Theme::dark(),
        "usage-settings-save-failed",
    );
    drop(lock_file);
    click(&mut reopened, &mut native);
    assert!(!reopened.usage_enabled);
    assert!(reopened.usage_error.is_none());
    // Another Settings window sees the persisted choice via the file watcher tick.
    super::tests::send_settings(&mut app, Message::Tick);
    assert!(!app.usage_enabled);
    assert!(
        !crate::usage::available(),
        "fixture and test network guard stays closed"
    );
}

#[test]
fn every_settings_page_is_a_known_usage_count() {
    for page in Page::ALL {
        assert!(
            crate::usage::counts::PAGES.contains(&page.usage_key()),
            "{}",
            page.key()
        );
    }
}
