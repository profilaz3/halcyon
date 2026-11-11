//! How failures, progress and warnings reach people: plain sentences first,
//! technical text only on request.
use super::*;
use crate::native_test::Native;
use cosmic::iced::{Event, mouse};

/// Runs `name` again in a child process with its own fixture desktop and
/// app config, so nothing touches the live session.
fn child(name: &str) -> bool {
    if std::env::var("DESKTOP_CARDS_FEEDBACK_TEST").as_deref() == Ok(name) {
        return false;
    }
    let root = std::env::temp_dir().join(format!("cards-feedback-{}-{name}", std::process::id()));
    for dir in ["config", "data", "cache", "state", "desktop"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join("dconf-profile"), "").unwrap();
    std::fs::write(
        root.join("config.json"),
        r#"{"setup_dismissed":true,"enabled_widgets":["clock","calendar","weather"]}"#,
    )
    .unwrap();
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", &format!("settings::feedback_tests::{name}")])
        .env("DESKTOP_CARDS_FEEDBACK_TEST", name)
        .env("DESKTOP_CARDS_CONFIG", root.join("config.json"))
        .env("DESKTOP_CARDS_DESKTOP_ROOT", root.join("desktop"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("DCONF_PROFILE", root.join("dconf-profile"));
    let result = command.output().unwrap();
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
    let bounds = native.bounds(app.settings_view(), id);
    for input in [
        mouse::Event::CursorMoved {
            position: bounds.center(),
        },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        for message in native
            .event(app.settings_view(), Event::Mouse(input), bounds.center())
            .0
        {
            let _ = app.update(message);
        }
    }
}

/// Captures for review, at the sizes and themes the audit names.
fn capture(app: &mut Settings, name: &str) {
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        let _ = app.update(Message::Resized(size));
        let mut native = Native::new(size);
        for (theme, shade) in [
            (cosmic::Theme::light(), "light"),
            (cosmic::Theme::dark(), "dark"),
        ] {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("{name}-{}-{shade}", size.width),
            );
        }
    }
}

#[test]
fn fb03_a_failed_save_says_what_to_do_and_keeps_details_on_request() {
    if child("fb03_a_failed_save_says_what_to_do_and_keeps_details_on_request") {
        return;
    }
    let (mut app, _) = Settings::init(Core::default(), Page::DesktopDock);
    let size = Size::new(820., 600.);
    let _ = app.update(Message::Resized(size));
    let technical = "Could not write /home/someone/.config/cosmic/com.system76.CosmicPanel.Dock/v1/size: \
                     No space left on device (os error 28)";
    let error = crate::desktop::DesktopError::RolledBack(technical.into());
    app.desktop.notice = Some(desktop::Notice::failed(&error));
    let mut native = Native::new(size);
    let plain = "Couldn’t save this change, so your previous settings were put back. \
                 Your disk is full. Free some space, then try again.";
    assert_eq!(native.text_all(app.settings_view(), plain).len(), 1);
    let shown = |native: &mut Native, app: &Settings| {
        native
            .text_all(app.settings_view(), &error.to_string())
            .len()
    };
    assert_eq!(shown(&mut native, &app), 0, "the technical text waits");
    capture(&mut app, "fb03-error");
    let _ = app.update(Message::Resized(size));
    click(&mut app, &mut native, "settings-error-details");
    assert_eq!(shown(&mut native, &app), 1, "Details shows what failed");
    capture(&mut app, "fb03-error-details");
    let _ = app.update(Message::Resized(size));
    click(&mut app, &mut native, "settings-error-details");
    assert_eq!(shown(&mut native, &app), 0);
    // A new failure starts with its details hidden again.
    click(&mut app, &mut native, "settings-error-details");
    app.desktop.notice = Some(desktop::Notice::failed(
        &crate::desktop::DesktopError::Io("Could not lock desktop settings: (os error 11)".into()),
    ));
    assert!(
        native
            .text_all(app.settings_view(), "Your settings are busy. Wait a moment, then try again.")
            .len()
            == 1
    );
    assert!(
        native
            .text_all(app.settings_view(), "Could not lock desktop settings: (os error 11)")
            .is_empty()
    );
}

#[test]
fn fb03_notices_split_the_sentence_from_the_technical_text() {
    let error = crate::desktop::DesktopError::Conflict(vec![
        crate::desktop::ConfigKey::new("com.system76.CosmicTk", 1, "accent_palette").unwrap(),
    ]);
    let notice = desktop::Notice::failed(&error);
    assert_eq!(
        notice.text(),
        "These settings changed somewhere else since you started editing. Nothing was changed."
    );
    assert_eq!(notice.details(), Some(error.to_string().as_str()));
    // A sentence that is already plain has nothing more to show.
    let plain = crate::desktop::DesktopError::RecoveryPending;
    assert_eq!(desktop::Notice::failed(&plain).details(), None);
    assert_eq!(desktop::Notice::Busy("Saving…".into()).details(), None);
}

#[test]
fn fb10_a_note_that_is_saving_is_not_reported_as_unsaved() {
    if child("fb10_a_note_that_is_saving_is_not_reported_as_unsaved") {
        return;
    }
    use cosmic::widget::text_editor::{Action, Edit};
    let (mut app, _) = Settings::init(Core::default(), Page::Notes);
    for c in "Buy bread".chars() {
        let _ = app.update(Message::Tools(tools::Message::Note(Action::Edit(
            Edit::Insert(c),
        ))));
    }
    assert!(app.tools.dirty());
    // Typing saves itself when it pauses, so leaving keeps nothing back.
    assert!(!app.tools.unsaved());
    let _ = app.update(Message::Page(Page::Overview));
    assert!(app.kept_note.is_none(), "{:?}", app.kept_note);
    assert!(app.tools.autosave_pending(), "autosave continues off Notes");
    std::thread::sleep(std::time::Duration::from_millis(1300));
    super::tests::send_settings(&mut app, Message::Tools(tools::Message::NoteAutosave));
    assert!(!app.tools.dirty());
    assert_eq!(crate::tools::notes::load().unwrap().text, "Buy bread");
    // A note the desktop changed meanwhile waits for a choice: that one is
    // unsaved, and says so.
    let _ = app.update(Message::Page(Page::Notes));
    let _ = app.update(Message::Tools(tools::Message::Note(Action::Edit(
        Edit::Insert('!'),
    ))));
    crate::tools::notes::save("Buy bread", "Buy rye bread").unwrap();
    let _ = app.update(Message::Tick);
    std::thread::sleep(std::time::Duration::from_millis(1300));
    super::tests::send_settings(&mut app, Message::Tools(tools::Message::NoteAutosave));
    assert!(app.tools.conflict.is_some());
    assert!(app.tools.unsaved());
    let _ = app.update(Message::Page(Page::Overview));
    assert!(app.kept_note.is_some());
}

#[test]
fn fb11_an_installed_update_offers_restart_settings() {
    if child("fb11_an_installed_update_offers_restart_settings") {
        return;
    }
    let (mut app, _) = Settings::init(Core::default(), Page::General);
    let _ = app.update(Message::AppSettingsPage(AppSettingsPage::Updates));
    app.updates.identity = Some(updates::Identity {
        channel: updates::UpdateChannel::Apt,
        running_version: "0.4.1".into(),
        installed_version: Some("0.5.0".into()),
        ..Default::default()
    });
    let size = Size::new(820., 600.);
    let _ = app.update(Message::Resized(size));
    let mut native = Native::new(size);
    assert_eq!(
        native
            .text_all(
                app.settings_view(),
                "Settings is still running 0.4.1. Restart Settings to use 0.5.0."
            )
            .len(),
        1
    );
    capture(&mut app, "fb11-update-installed");
    // A failure says what to do next.
    app.updates.identity.as_mut().unwrap().installed_version = Some("0.4.1".into());
    app.updates.check = updates::Check::Failed(updates::Failure::MissingPackageKit);
    let _ = app.update(Message::Resized(size));
    assert!(native.try_bounds(app.settings_view(), "updates-restart").is_none());
    assert!(updates::Failure::MissingPackageKit.label().contains("Restart your computer"));
    capture(&mut app, "fb11-service-missing");
    app.updates.check = updates::Check::Failed(updates::Failure::InstallationChanged);
    let _ = app.update(Message::Resized(size));
    assert!(!restart_requested());
    click(&mut app, &mut native, "updates-restart");
    assert!(restart_requested(), "main opens Settings again once it closes");
}
