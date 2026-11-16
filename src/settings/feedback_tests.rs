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

/// The Dock, Panel and Activity island pages save through one bar editor,
/// so each shows that editor's failures and warnings in its footer.
#[test]
fn fb03_every_bar_page_shows_its_save_notice() {
    if child("fb03_every_bar_page_shows_its_save_notice") {
        return;
    }
    let size = Size::new(820., 600.);
    for page in [Page::DesktopDock, Page::DesktopPanel, Page::ActivityIsland] {
        let (mut app, _) = Settings::init(Core::default(), page);
        let _ = app.update(Message::Resized(size));
        let mut native = Native::new(size);
        // An Islands gap over 64 is refused before anything is written.
        app.desktop.islands.gap_text = Some("80".into());
        let _ = app.update(Message::DesktopEditor(desktop::Message::Apply));
        assert_eq!(
            native
                .text_all(app.settings_view(), "Gap must be between 0 and 64 pixels")
                .len(),
            1,
            "{page:?}"
        );
        // Saved, but the panel did not take the new layout.
        app.desktop.notice = Some(desktop::Notice::Warning(desktop::ISLANDS_PENDING.into()));
        assert_eq!(
            native
                .text_all(app.settings_view(), desktop::ISLANDS_PENDING)
                .len(),
            1,
            "{page:?}"
        );
    }
}

/// Widget pages, setup, starting the widgets and file pickers say their
/// failures as plain sentences, keep the technical text under Details and
/// offer Try again only where it can work (M32).
#[test]
fn m32_every_footer_failure_reads_plainly_with_details() {
    if child("m32_every_footer_failure_reads_plainly_with_details") {
        return;
    }
    let (mut app, _) = Settings::init(Core::default(), Page::Clock);
    let size = Size::new(820., 600.);
    let _ = app.update(Message::Resized(size));
    let mut native = Native::new(size);
    let technical = "Could not write /home/someone/.config/halcyon/config.json: \
                     No space left on device (os error 28)";
    app.failed(technical.into());
    let plain = "Your disk is full. Free some space, then try again.";
    assert_eq!(native.text_all(app.settings_view(), plain).len(), 1);
    assert!(native.text_all(app.settings_view(), technical).is_empty());
    click(&mut app, &mut native, "settings-error-details");
    assert_eq!(native.text_all(app.settings_view(), technical).len(), 1);
    assert!(
        native
            .try_bounds(app.settings_view(), "settings-error-retry")
            .is_none(),
        "doing the change again is the retry"
    );
    // A sentence that already reads plainly is kept, with nothing more.
    app.failed("Finish the current edit before undoing a change.".into());
    assert_eq!(
        native
            .text_all(
                app.settings_view(),
                "Finish the current edit before undoing a change."
            )
            .len(),
        1
    );
    assert!(
        native
            .try_bounds(app.settings_view(), "settings-error-details")
            .is_none()
    );
    // Starting the widgets: what didn't happen, with Try again; systemctl's
    // words wait under Details.
    let systemctl = "Job for halcyon.service failed because the control process exited with error code.\n\
                     See \"journalctl --user -xeu halcyon.service\" for details.";
    let _ = app.update(Message::CommandDone(
        WIDGETS_STARTED.into(),
        Err(systemctl.into()),
    ));
    let started = "The widgets couldn’t start. Try again, or see Help for what went wrong.";
    assert_eq!(native.text_all(app.settings_view(), started).len(), 1);
    native.bounds(app.settings_view(), "settings-error-retry");
    assert!(matches!(
        app.footer_failure().and_then(|f| f.retry.as_deref()),
        Some(Message::Background(true))
    ));
    assert_eq!(
        app.footer_failure().and_then(|f| f.details.as_deref()),
        Some(systemctl)
    );
    // Links name what didn't open.
    let _ = app.update(Message::CommandDone(
        "Weather source opened".into(),
        Err("xdg-open: no method available for opening 'https://open-meteo.com/'".into()),
    ));
    assert_eq!(
        app.status,
        "Couldn’t open the weather source. Check that you have a default web browser, then try again."
    );
    // A later status replaces the failure, and its Details with it.
    app.saved();
    assert!(app.footer_failure().is_none());
    assert!(app.page_error_details().is_none());
    // Setup's own card says it plainly too, with Details.
    app.wizard = Some(open_wizard());
    app.wizard
        .as_mut()
        .unwrap()
        .fail("Could not save setup progress: Is a directory (os error 21)".into());
    let wizard = app.wizard.as_ref().unwrap();
    assert_eq!(
        wizard.error.as_deref(),
        Some("Could not save setup progress.")
    );
    assert_eq!(
        wizard.error_details(),
        Some("Could not save setup progress: Is a directory (os error 21)")
    );
    let _ = app.update(Message::ToggleErrorDetails);
    assert_eq!(
        app.error_details.as_deref(),
        Some("Could not save setup progress: Is a directory (os error 21)")
    );
}

/// A file chooser that won't open, or a picture that can't be kept, says so
/// plainly on the Wallpaper and Looks pages.
#[test]
fn m32_file_picker_failures_read_plainly() {
    let notice = desktop::Notice::plain(
        "Could not open the file chooser: org.freedesktop.DBus.Error.ServiceUnknown".into(),
    );
    assert_eq!(notice.text(), "Couldn’t open the file chooser. Try again.");
    assert_eq!(
        notice.details(),
        Some("Could not open the file chooser: org.freedesktop.DBus.Error.ServiceUnknown")
    );
    let notice = desktop::Notice::plain("Choose a local file or folder.".into());
    assert_eq!(notice.text(), "Choose a local file or folder.");
    assert_eq!(notice.details(), None);
}

/// The Dock and Panel pages state what is wrong and offer a way on, never
/// a raw error or a question (L23).
#[test]
fn l23_dock_and_panel_dead_ends_offer_a_way_on() {
    if child("l23_dock_and_panel_dead_ends_offer_a_way_on") {
        return;
    }
    let (mut app, _) = Settings::init(Core::default(), Page::DesktopDock);
    let size = Size::new(1020., 760.);
    let _ = app.update(Message::Resized(size));
    let mut native = Native::new(size);
    // The desktop settings can't be read: say so, try again or get help;
    // the raw error is only under Details.
    let raw = "Could not read /home/someone/.config/cosmic: Permission denied (os error 13)";
    app.desktop.unreadable(Some(raw));
    native.text_bounds(
        app.settings_view(),
        "Halcyon can’t read your desktop settings right now.",
    );
    native.text_bounds(app.settings_view(), "Help and diagnostics");
    native.bounds(app.settings_view(), "desktop-retry");
    assert!(
        native
            .all_text(app.settings_view())
            .iter()
            .all(|text| !text.contains("os error")),
        "the raw error waits"
    );
    click(&mut app, &mut native, "desktop-retry");
    assert!(app.desktop.unavailable().is_none(), "read again");
    assert!(app.desktop.current().is_some());
    // A bar that can't be read: the same, with COSMIC Settings.
    app.desktop.unreadable(None);
    native.text_bounds(
        app.settings_view(),
        "Halcyon couldn’t read the Dock settings.",
    );
    native.text_bounds(app.settings_view(), "Open COSMIC Settings ↗");
    click(&mut app, &mut native, "desktop-retry");
    assert!(app.desktop.current().is_some(), "read again");
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

#[test]
fn dt13_the_weather_preview_says_it_is_an_example() {
    if child("dt13_the_weather_preview_says_it_is_an_example") {
        return;
    }
    let (mut app, _) = Settings::init(Core::default(), Page::Weather);
    let size = Size::new(820., 600.);
    let _ = app.update(Message::Resized(size));
    let mut native = Native::new(size);
    assert!(app.config.location.is_none());
    let before = "Example weather. Choose your city below to see yours.";
    assert_eq!(native.text_all(app.settings_view(), before).len(), 1);
    // In view without scrolling, right under the preview.
    let note = native.bounds(app.settings_view(), "weather-preview-note");
    assert!(note.y + note.height < size.height);
    capture(&mut app, "dt13-no-city");
    app.config.location = Some(crate::config::Location {
        name: "Zagreb".into(),
        latitude: 45.8,
        longitude: 16.0,
        timezone: None,
    });
    let _ = app.update(Message::Resized(size));
    assert!(native.text_all(app.settings_view(), before).is_empty());
    assert_eq!(native.text_all(app.settings_view(), "Example weather").len(), 1);
    capture(&mut app, "dt13-city");
}
