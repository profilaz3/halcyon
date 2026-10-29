use super::*;
use crate::displays::test_support::isolated;
use crate::settings::tests::send_settings;
use serde_json::json;
fn fixture() -> Settings {
    std::fs::write(Config::path().unwrap(), br#"{"onboarded":true,"setup_dismissed":true,"theme":"teal","clock":{"face":"digital","format_24h":false},"location":{"name":"PRIVATE","latitude":1.0,"longitude":2.0}}"#.as_slice()).unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::General);
    app.app_settings_page = AppSettingsPage::Restore;
    app
}
fn raw() -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap()
}
#[test]
fn fb16_native_migration_review_keyboard_apply_and_recovery_fit() {
    if isolated(
        "settings::design::tests::fb16_native_migration_review_keyboard_apply_and_recovery_fit",
    ) {
        return;
    }
    use cosmic::iced::{Event, Point, keyboard};
    let mut app = fixture();
    let before = raw();
    send_settings(&mut app, super::super::Message::Design(Message::Review));
    for (width, height) in [(820., 600.), (1280., 800.)] {
        send_settings(
            &mut app,
            super::super::Message::Resized(Size::new(width, height)),
        );
        let mut native = crate::native_test::Native::new(Size::new(width, height));
        crate::settings::tests::native_reveal(&app, &mut native, "design-migrate");
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("fb16-migration-{width}"),
        );
        assert_eq!(raw(), before);
    }
    let mut native = crate::native_test::Native::new(Size::new(820., 600.));
    send_settings(
        &mut app,
        super::super::Message::Resized(Size::new(820., 600.)),
    );
    crate::settings::tests::native_reveal(&app, &mut native, "design-migrate");
    native.operate(
        app.settings_view(),
        Box::new(cosmic::iced::advanced::widget::operation::focusable::focus(
            cosmic::widget::Id::new("design-migrate"),
        )),
    );
    let key = keyboard::Key::Named(keyboard::key::Named::Enter);
    let (messages, _) = native.event(
        app.settings_view(),
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        }),
        Point::new(1., 1.),
    );
    assert_eq!(messages.len(), 1);
    for message in messages {
        send_settings(&mut app, message);
    }
    assert!(app.design.managed);
    assert!(app.config.design_problem.is_none());
    let frozen = raw();
    send_settings(&mut app, super::super::Message::ClockFormat(true));
    assert!(app.config.clock.format_24h);
    assert_eq!(raw(), frozen);
    send_settings(&mut app, super::super::Message::UndoDesktop);
    assert!(!app.config.clock.format_24h);
    let path = crate::design::storage::path(&Config::path().unwrap());
    std::fs::write(&path, b"broken original").unwrap();
    send_settings(&mut app, super::super::Message::Tick);
    assert!(app.config.design_problem.is_some());
    send_settings(&mut app, super::super::Message::ClockFormat(true));
    assert_eq!(std::fs::read(&path).unwrap(), b"broken original");
    assert_eq!(raw(), frozen);
    for (width, height) in [(820., 600.), (1280., 800.)] {
        send_settings(
            &mut app,
            super::super::Message::Resized(Size::new(width, height)),
        );
        let mut native = crate::native_test::Native::new(Size::new(width, height));
        for id in ["design-recovery-export", "design-recheck"] {
            let b = native.bounds(app.settings_view(), id);
            assert!(
                b.y >= 0. && b.y + b.height <= height && b.x >= 0. && b.x + b.width <= width,
                "{id}: {b:?}"
            );
        }
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("fb16-recovery-{width}"),
        );
    }
}
#[test]
fn fb16_invalid_design_startup_has_no_default_desktop_and_exports_exact_bytes() {
    if isolated(
        "settings::design::tests::fb16_invalid_design_startup_has_no_default_desktop_and_exports_exact_bytes",
    ) {
        return;
    }
    let _ = fixture();
    let path = Config::path().unwrap();
    crate::desktop::actions::history_store()
        .unwrap()
        .migrate_design()
        .unwrap();
    let document = crate::design::storage::path(&path);
    let damaged = [0xff, 0, 23, 0xfe];
    std::fs::write(&document, damaged).unwrap();
    let config = Config::load();
    assert!(config.design_problem.is_some());
    assert!(Widget::ALL.into_iter().all(|w| !config.widget_visible(w)));
    let export = path.with_file_name("original-damaged-design.json");
    crate::design::commands::export(&path, &export, false).unwrap();
    assert_eq!(std::fs::read(&export).unwrap(), damaged);
    assert!(crate::design::commands::export(&path, &document, false).is_err());
    assert!(crate::design::commands::export(&path, &path, true).is_err());
    let (app, _) = Settings::init(Core::default(), Page::Clock);
    assert!(app.wizard.is_none());
    assert!(app.config.design_problem.is_some());
}
#[test]
fn fb16_local_and_moment_writers_use_the_design() {
    if isolated("settings::design::tests::fb16_local_and_moment_writers_use_the_design") {
        return;
    }
    let _ = fixture();
    let store = crate::desktop::actions::history_store().unwrap();
    store.migrate_design().unwrap();
    let frozen = raw();
    Config::save_moment("/moments/active", json!("day")).unwrap();
    Config::save_changes(json!({"location":{"name":"Updated","latitude":3.0,"longitude":4.0}}))
        .unwrap();
    assert_eq!(raw()["theme"], frozen["theme"]);
    assert!(raw().get("moments").is_none());
    assert_eq!(
        Config::load().moments.active,
        Some(crate::moments::Phase::Day)
    );
}

#[test]
fn fb16_native_old_client_conflict_requires_review_before_keep() {
    if isolated(
        "settings::design::tests::fb16_native_old_client_conflict_requires_review_before_keep",
    ) {
        return;
    }
    let mut app = fixture();
    let store = crate::desktop::actions::history_store().unwrap();
    store.migrate_design().unwrap();
    let mut changed = raw();
    changed["theme"] = json!("amber");
    std::fs::write(Config::path().unwrap(), changed.to_string()).unwrap();
    send_settings(&mut app, super::super::Message::Tick);
    assert!(app.config.design_problem.is_some());
    assert!(app.design.source_conflict);
    send_settings(&mut app, super::super::Message::Design(Message::KeepDesign));
    assert_eq!(raw(), changed);
    send_settings(
        &mut app,
        super::super::Message::Design(Message::ReviewSource),
    );
    for (width, height) in [(820., 600.), (1280., 800.)] {
        send_settings(
            &mut app,
            super::super::Message::Resized(Size::new(width, height)),
        );
        let mut native = crate::native_test::Native::new(Size::new(width, height));
        let b = native.bounds(app.settings_view(), "design-keep-source");
        assert!(b.y >= 0. && b.y + b.height <= height);
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("fb16-source-review-{width}"),
        );
    }
    send_settings(&mut app, super::super::Message::Design(Message::KeepDesign));
    assert!(app.config.design_problem.is_none());
    assert_eq!(app.config.theme, "teal");
    assert!(!app.design.source_conflict);
    assert!(
        app.design
            .notice
            .as_deref()
            .unwrap()
            .contains("Older changes saved")
    );
}

#[test]
fn polish050_fresh_settings_uses_current_design_before_first_view() {
    if isolated(
        "settings::design::tests::polish050_fresh_settings_uses_current_design_before_first_view",
    ) {
        return;
    }
    let original = Config::path().unwrap();
    let path = original.parent().unwrap().join("fresh/config.json");
    // This test already runs alone in the isolated child process.
    unsafe { std::env::set_var("DESKTOP_CARDS_CONFIG", &path) };
    let (config, state) = State::initialize();
    assert!(state.managed);
    assert!(state.notice.is_none());
    assert!(config.design_problem.is_none());
    assert!(!config.onboarded);
    let before = std::fs::read(&path).unwrap();
    let (_, again) = State::initialize();
    assert!(again.managed);
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn polish050_builder_migration_review_stays_with_unsaved_recipe() {
    if isolated(
        "settings::design::tests::polish050_builder_migration_review_stays_with_unsaved_recipe",
    ) {
        return;
    }
    let mut app = fixture();
    send_settings(&mut app, super::super::Message::Page(Page::Builder));
    send_settings(
        &mut app,
        super::super::Message::Recipe(crate::settings::recipes::Message::Name(
            "My clock draft".into(),
        )),
    );
    let before = raw();
    let draft = app.recipe_editor.draft.clone();
    send_settings(&mut app, super::super::Message::Design(Message::Migrate));
    assert!(!app.design.managed, "migration requires explicit review");
    send_settings(&mut app, super::super::Message::Design(Message::Review));
    assert_eq!(app.page, Page::Builder);
    assert_eq!(raw(), before);
    for (width, height) in [(820., 600.), (1280., 800.)] {
        send_settings(
            &mut app,
            super::super::Message::Resized(Size::new(width, height)),
        );
        let mut native = crate::native_test::Native::new(Size::new(width, height));
        crate::settings::tests::native_reveal(&app, &mut native, "design-migrate");
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("polish050-builder-migration-{width}"),
        );
    }
    send_settings(&mut app, super::super::Message::Design(Message::Cancel));
    assert_eq!(app.page, Page::Builder);
    assert!(!app.design.managed);
    assert_eq!(raw(), before);
    assert_eq!(app.recipe_editor.draft, draft);
    send_settings(&mut app, super::super::Message::Design(Message::Review));
    send_settings(&mut app, super::super::Message::Design(Message::Migrate));
    assert_eq!(app.page, Page::Builder);
    assert!(app.design.managed);
    assert_eq!(app.recipe_editor.draft, draft);
    assert_eq!(app.config.theme, "teal");
    assert_eq!(app.config.location.as_ref().unwrap().name, "PRIVATE");
}
