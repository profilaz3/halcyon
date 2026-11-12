use super::*;
use crate::native_test::Native;
use cosmic::iced::{Event, Point, mouse};

#[test]
fn ux_native_effect_preview_survives_navigation_and_restart_without_applying() {
    if child("ux_native_effect_preview_survives_navigation_and_restart_without_applying") {
        return;
    }
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::WeatherEffects);
    let mut effect = app.config.rain.clone();
    effect.enabled = true;
    let _ = app.update(Message::Rain(rain::Edit::Set(effect.clone())));
    let _ = app.update(Message::Page(Page::General));
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
    let _ = app.update(Message::Page(Page::DesktopWallpaper));
    assert_eq!(app.wallpaper.effect(), Some(&effect));
    drop(app);
    let (mut app, _) = Settings::init(Core::default(), Page::WeatherEffects);
    assert_eq!(app.wallpaper.effect(), Some(&effect));
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
    let _ = app.update(Message::Wallpaper(wallpaper::Message::Discard));
    assert!(app.wallpaper.effect().is_none());
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}

#[test]
fn ux_native_hidden_focus_has_live_controls() {
    if child("ux_native_hidden_focus_has_live_controls") {
        return;
    }
    let (mut app, _) = Settings::init(Core::default(), Page::Focus);
    assert!(!app.config.widget_visible(Widget::Focus));
    let size = Size::new(820., 600.);
    let _ = app.update(Message::Resized(size));
    let mut native = Native::new(size);
    let target = native.text_bounds(app.settings_view(), "Start").center();
    let mut messages = Vec::new();
    for event in [
        mouse::Event::CursorMoved { position: target },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        messages.extend(
            native
                .event(app.settings_view(), Event::Mouse(event), target)
                .0,
        );
    }
    assert!(messages.iter().any(|m| matches!(
        m,
        Message::Tools(tools::Message::Card(
            crate::tools::cards::Action::FocusToggle
        ))
    )));
    native.capture(
        app.settings_view(),
        &cosmic::theme::active(),
        "hidden-focus-controls",
    );
}

#[test]
fn ux_native_notes_undo_is_document_local_and_controls_fit() {
    if child("ux_native_notes_undo_is_document_local_and_controls_fit") {
        return;
    }
    use cosmic::iced::keyboard;
    use cosmic::widget::text_editor;
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::Notes);
    let _ = app.update(Message::Tools(tools::Message::Note(
        text_editor::Action::Edit(text_editor::Edit::Paste(std::sync::Arc::new(
            "A local note 🌙".into(),
        ))),
    )));
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        let _ = app.update(Message::Resized(size));
        let mut native = Native::new(size);
        reveal(&app, &mut native, "note-editor");
        let editor = native.bounds(app.settings_view(), "note-editor");
        click_at(&mut app, &mut native, editor.center());
        let event = Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Character("z".into()),
            modified_key: keyboard::Key::Character("z".into()),
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::CTRL,
            text: None,
            repeat: false,
        });
        let (messages, status) = native.event(app.settings_view(), event, editor.center());
        assert_eq!(status, cosmic::iced::event::Status::Captured);
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::Tools(tools::Message::NoteUndo)))
        );
        for message in messages {
            let _ = app.update(message);
        }
        assert!(app.tools.content.text().trim().is_empty());
        let _ = app.update(Message::Tools(tools::Message::NoteRedo));
        assert_eq!(app.tools.content.text().trim(), "A local note 🌙");
        reveal(&app, &mut native, "note-editor");
        native.capture(
            app.settings_view(),
            &cosmic::theme::active(),
            &format!("notes-undo-{}", size.width),
        );
        // The note has no buttons of its own now: it saves as you type and
        // Undo is the app's own (Ctrl+Z above, global Undo below).
        let b = native.bounds(app.settings_view(), "note-editor");
        assert!(b.x >= 0. && b.x + b.width <= size.width, "note-editor: {b:?}");
    }
    // Global Undo after the editor loses focus still belongs to this note.
    let _ = app.update(Message::UndoDesktop);
    let _ = app.update(Message::UndoDesktop);
    assert!(app.tools.content.text().trim().is_empty());
    assert_eq!(before, std::fs::read(Config::path().unwrap()).unwrap());
}

#[test]
fn ux_native_shared_finish_sets_scope_and_busy_status_is_visible() {
    if child("ux_native_shared_finish_sets_scope_and_busy_status_is_visible") {
        return;
    }
    let (mut app, _) = Settings::init(Core::default(), Page::Clock);
    app.material_target = crate::materials::Target::Panel;
    let _ = app.update(Message::OpenWidgetFinish);
    assert_eq!(app.page, Page::Appearance);
    assert_eq!(app.material_target, crate::materials::Target::Widgets);
    app.native_busy = true;
    let mut native = Native::new(Size::new(820., 600.));
    assert!(
        native
            .try_bounds(app.settings_view(), "settings-saving")
            .is_some()
    );
}

fn child(name: &str) -> bool {
    if std::env::var("DESKTOP_CARDS_UX_TEST").as_deref() == Ok(name) {
        return false;
    }
    let root = std::env::temp_dir().join(format!("cards-ux-{}-{name}", std::process::id()));
    for dir in ["config", "data", "cache", "state", "desktop"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join("dconf-profile"), "").unwrap();
    std::fs::write(root.join("config.json"), r#"{"setup_dismissed":true,"enabled_widgets":["clock","calendar","weather","devbox"],"future_preference":37}"#).unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &format!("settings::ux_tests::{name}")])
        .env("DESKTOP_CARDS_UX_TEST", name)
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

fn click_at(app: &mut Settings, native: &mut Native, point: Point) {
    for event in [
        mouse::Event::CursorMoved { position: point },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        for message in native
            .event(app.settings_view(), Event::Mouse(event), point)
            .0
        {
            let _ = app.update(message);
        }
    }
}
fn click(app: &mut Settings, native: &mut Native, id: &str) {
    let bounds = crate::settings::tests::native_reveal(app, native, id);
    assert!(
        bounds.x >= 0.
            && bounds.y >= 0.
            && bounds.x + bounds.width <= native.size.width
            && bounds.y + bounds.height <= native.size.height,
        "{id}: {bounds:?}"
    );
    click_at(app, native, bounds.center());
}

#[test]
fn ux_native_incomplete_color_has_discard_and_never_applies() {
    if child("ux_native_incomplete_color_has_discard_and_never_applies") {
        return;
    }
    use crate::settings::tests::{native_click, send_settings, settle_settings};
    let (mut app, initial) = Settings::init(Core::default(), Page::Themes);
    settle_settings(&mut app, initial);
    send_settings(&mut app, Message::Resized(Size::new(820., 600.)));
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    send_settings(
        &mut app,
        Message::Profiles(profiles::Message::ThemeCustom(Some("#12".into()))),
    );
    assert!(app.profiles.profile().is_none());
    assert!(!app.profiles.can_apply());
    let mut native = Native::new(Size::new(820., 552.));
    native_click(&mut app, &mut native, "profiles-confirm");
    assert_eq!(app.profiles.custom.as_deref(), Some("#12"));
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
    native.capture(
        app.settings_view(),
        &cosmic::Theme::dark(),
        "ux-color-incomplete-820",
    );
    native_click(&mut app, &mut native, "profiles-discard");
    assert!(!app.profiles.review_open());
    assert!(app.kept_previews().is_empty());
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}

/// Reveal a control through the page's real scrollable before checking its
/// visible hit target. Reset first so the target starts in content coordinates.
fn reveal(app: &Settings, native: &mut Native, id: &str) {
    native.operate(
        app.settings_view(),
        Box::new(super::navigation::Reveal::target(id)),
    );
}

fn reveal_click(app: &mut Settings, native: &mut Native, id: &str) {
    reveal(app, native, id);
    click(app, native, id);
}

#[test]
fn review_r2_native_theme_preview_never_falls_back_to_obsolete_home_pixels() {
    if child("review_r2_native_theme_preview_never_falls_back_to_obsolete_home_pixels") {
        return;
    }
    use crate::desktop::{DesktopStore, profiles as records, wallpaper as backgrounds};
    let root = std::path::PathBuf::from(std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap());
    let set = |key: &str, value: &str| {
        let directory = root
            .join("config/cosmic")
            .join(backgrounds::NAMESPACE)
            .join("v1");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(key), value).unwrap();
    };
    std::fs::write(
        Config::path().unwrap(),
        r#"{"setup_dismissed":true,"enabled_widgets":[]}"#,
    )
    .unwrap();
    let bright = [245u8, 243, 207];
    let dark = [10u8, 10, 10];
    let size = Size::new(1024., 720.);
    let count =
        |pixels: &[u8], rgb: [u8; 3]| pixels.chunks_exact(4).filter(|p| p[..3] == rgb).count();
    let capture = |pixels: &[u8], retained: bool, stage: &str| {
        if let Some(directory) = std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            image::save_buffer(
                directory.join(format!("r2-native-preview-{retained}-{stage}.png")),
                pixels,
                size.width as u32,
                size.height as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    };
    for retained in [false, true] {
        let image = root.join(format!("preview-{retained}.png"));
        image::RgbImage::from_pixel(32, 32, image::Rgb(bright))
            .save(&image)
            .unwrap();
        set(
            "all",
            &ron::to_string(&backgrounds::BgEntry::new(
                "all",
                backgrounds::BgSource::Path(image.clone()),
            ))
            .unwrap(),
        );
        set("same-on-all", if retained { "false" } else { "true" });
        set("backgrounds", if retained { "[\"eDP-1\"]" } else { "[]" });
        std::fs::write(root.join("outputs"), if retained { "eDP-1\n" } else { "" }).unwrap();
        if retained {
            set(
                "output.eDP-1",
                &ron::to_string(&backgrounds::BgEntry::new(
                    "eDP-1",
                    backgrounds::BgSource::Path(image.clone()),
                ))
                .unwrap(),
            );
        }
        let (mut app, _) = Settings::init(Core::default(), Page::Themes);
        app.window = size;
        // Drive jobs explicitly; init tasks are never launched in this fixture.
        let mut editor = profiles::ProfilesEditor::default();
        editor.open();
        editor.open_home();
        for job in editor.take_jobs() {
            editor.update(job.run());
        }
        assert!(
            editor.home_image().is_some(),
            "prime the applied wallpaper cache"
        );
        if retained {
            editor.update(profiles::Message::Select(profiles::Selection::Preset(
                "theme-bare",
            )));
            editor.update(profiles::Message::ThemeColors(true));
            editor.update(profiles::Message::ThemeWallpaper(true));
        } else {
            let mut p = records::themes::record(crate::desktop_theme::Family::Bare);
            p.wallpaper = Some(records::WallpaperLook {
                background: Some(records::BackgroundLook {
                    source: Some(records::SourceLook::Image(image.clone())),
                    ..Default::default()
                }),
                ..Default::default()
            });
            p.widget_style.as_mut().unwrap()["appearance"]["wallpaper_colors"] = true.into();
            let store = DesktopStore::from_env().unwrap();
            let id = records::ProfileLibrary::new(&store).create(&p).unwrap();
            editor.update(profiles::Message::Select(profiles::Selection::Saved(id)));
        }
        for job in editor.take_theme_jobs() {
            editor.update(job.run());
        }
        for job in editor.take_jobs() {
            editor.update(job.run());
        }
        app.profiles = editor;
        let mut native = Native::new(size);
        let pixels = native.render(app.settings_view(), &cosmic::Theme::dark());
        capture(&pixels, retained, "bright");
        assert!(
            count(&pixels, bright) > 1_000,
            "actual preview image must be visible"
        );
        image::RgbImage::from_pixel(33, 32, image::Rgb(dark))
            .save(&image)
            .unwrap();
        assert_eq!(
            app.profiles.update(profiles::Message::Apply),
            profiles::Outcome::None
        );
        assert!(app.profiles.stale());
        app.profiles.update(profiles::Message::Replan);
        for job in app.profiles.take_theme_jobs() {
            app.profiles.update(job.run());
        }
        assert!(app.profiles.backdrop_image().is_none());
        assert!(
            app.profiles.home_image().is_some(),
            "the old Home cache still exists"
        );
        let pixels = native.render(app.settings_view(), &cosmic::Theme::dark());
        capture(&pixels, retained, "loading");
        assert_eq!(
            count(&pixels, bright),
            0,
            "never draw old Home pixels while the new version loads"
        );
        for job in app.profiles.take_jobs() {
            app.profiles.update(job.run());
        }
        let pixels = native.render(app.settings_view(), &cosmic::Theme::dark());
        capture(&pixels, retained, "dark");
        assert_eq!(
            count(&pixels, bright),
            0,
            "old renderer textures must not survive"
        );
        assert!(
            count(&pixels, dark) > 1_000,
            "render the replacement image's actual pixels"
        );
        assert!(app.profiles.can_apply());
    }
}

#[test]
fn dew_point_switch_saves_and_undoes_without_touching_other_preferences() {
    if child("dew_point_switch_saves_and_undoes_without_touching_other_preferences") {
        return;
    }
    let original = std::fs::read(Config::path().unwrap()).unwrap();
    for size in [Size::new(820., 600.), Size::new(1100., 760.)] {
        std::fs::write(Config::path().unwrap(), &original).unwrap();
        let (mut app, _) = Settings::init(Core::default(), Page::Weather);
        let _ = app.update(Message::Resized(size));
        assert!(!app.config.weather.show_dew_point, "Existing users opt in");
        Config::save_weather(serde_json::json!({"future_reading":37})).unwrap();
        let mut native = Native::new(size);
        // Dew point sits in the folded section that names it.
        let _ = app.update(Message::ToggleAdvanced(Page::Weather));
        reveal_click(&mut app, &mut native, "weather-dew-point-toggle");
        assert!(app.config.weather.show_dew_point);
        assert!(Config::try_load().unwrap().weather.show_dew_point);
        let stored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap();
        assert_eq!(stored["weather"]["future_reading"], 37);
        assert_eq!(stored["future_preference"], 37);
        for (name, theme) in [
            ("dark", cosmic::Theme::dark()),
            ("light", cosmic::Theme::light()),
        ] {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("dew-point-settings-{name}-{}", size.width),
            );
        }
        let _ = app.update(Message::UndoPreferences);
        assert!(!Config::try_load().unwrap().weather.show_dew_point);
        let restored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap();
        assert_eq!(restored["weather"]["future_reading"], 37);
        let hit = navigation::search("dewpoint")[0];
        assert_eq!(hit.page, Page::Weather);
        assert_eq!(hit.anchor(), "weather-dew-point");
        assert!(hit.advanced, "search opens the folded section");
    }
}

#[test]
fn native_task_navigation_and_widget_switches_keep_layout_drafts() {
    if child("native_task_navigation_and_widget_switches_keep_layout_drafts") {
        return;
    }
    let original = std::fs::read(Config::path().unwrap()).unwrap();
    for size in [Size::new(820., 600.), Size::new(1024., 720.)] {
        let (mut app, _) = Settings::init(Core::default(), Page::Overview);
        let _ = app.update(Message::Canvas(canvas::Message::Start));
        let _ = app.update(Message::Resized(size));
        let mut native = Native::new(size);
        let _ = app.update(Message::Placement(placement::Message::Select(
            "clock".into(),
        )));
        let _ = app.update(Message::Placement(placement::Message::Nudge([1, 0], false)));
        assert!(app.placement.dirty());
        let draft = serde_json::to_value(app.placement.session_draft()).unwrap();
        let widgets = app
            .nav_model
            .iter()
            .find(|&id| app.nav_model.data::<Page>(id) == Some(&Page::Overview))
            .unwrap();
        let _ = app.update(Message::NavSelected(widgets));
        assert_eq!(
            app.page,
            Page::Overview,
            "the root opens its stable landing"
        );
        assert_eq!(app.nav_model.active_data::<Page>(), Some(&Page::Overview));
        let before = app.config.data_visible(Widget::Clock);
        // The switch has its own hit target, outside the button that opens settings.
        let toggle =
            crate::settings::tests::native_reveal(&app, &mut native, "widget-toggle-clock")
                .center();
        for event in [
            mouse::Event::CursorMoved { position: toggle },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            let (messages, _) = native.event(app.settings_view(), Event::Mouse(event), toggle);
            for message in messages {
                let _ = app.update(message);
            }
        }
        assert_eq!(app.page, Page::Overview);
        assert_eq!(app.config.data_visible(Widget::Clock), !before);
        let _ = app.update(Message::UndoPreferences);
        click(&mut app, &mut native, "widget-open-weather");
        assert_eq!(app.page, Page::Weather);
        click(&mut app, &mut native, "settings-back");
        assert_eq!(app.page, Page::Overview);
        // Windows, Dock and Looks are sidebar roots of their own now.
        click(&mut app, &mut native, "nav-desktop-windows");
        assert_eq!(app.page, Page::DesktopWindows);
        assert_eq!(
            app.nav_model.active_data::<Page>(),
            Some(&Page::DesktopWindows)
        );
        let _ = app.update(Message::Page(Page::DesktopWorkspaces));
        assert_eq!(
            app.page,
            Page::DesktopWindows,
            "Workspaces is part of Windows"
        );
        assert_eq!(
            app.nav_model.active_data::<Page>(),
            Some(&Page::DesktopWindows)
        );
        click(&mut app, &mut native, "nav-overview");
        assert_eq!(
            app.page,
            Page::Overview,
            "the root returns to the stable Widgets landing"
        );
        click(&mut app, &mut native, "nav-desktop-dock");
        assert_eq!(app.page, Page::DesktopDock);
        click(&mut app, &mut native, "nav-desktop-profiles");
        click(&mut app, &mut native, "presets-schedules");
        assert_eq!(app.page, Page::DesktopAutomation);
        assert_eq!(
            app.nav_model.active_data::<Page>(),
            Some(&Page::DesktopProfiles)
        );
        assert_eq!(app.back_title().as_deref(), Some("Looks"));
        assert!(
            native
                .try_bounds(app.settings_view(), "settings-back")
                .is_some(),
            "Back names the landing that opened this editor"
        );
        click(&mut app, &mut native, "settings-back");
        assert_eq!(app.page, Page::DesktopProfiles);
        let _ = app.update(Message::Resized(size));
        let _ = app.update(Message::Page(Page::Overview));
        assert_eq!(
            serde_json::to_value(app.placement.session_draft()).unwrap(),
            draft
        );
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap();
        assert_eq!(saved["future_preference"], 37);
        assert!(
            saved.get("widget_positions").is_none(),
            "navigation never applies the layout"
        );
        // Restore this fixture between sizes; no live preferences are accessible.
        std::fs::write(Config::path().unwrap(), &original).unwrap();
    }
}

#[test]
fn split_sidebar_keyboard_navigation_keeps_focus_and_drafts() {
    if child("split_sidebar_keyboard_navigation_keeps_focus_and_drafts") {
        return;
    }
    use cosmic::iced::keyboard::{self, key::Named};
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::Overview);
    let _ = app.update(Message::Canvas(canvas::Message::Start));
    let _ = app.update(Message::Resized(Size::new(820., 600.)));
    let _ = app.update(Message::Placement(placement::Message::Select(
        "clock".into(),
    )));
    let _ = app.update(Message::Placement(placement::Message::Nudge([1, 0], false)));
    let draft = serde_json::to_value(app.placement.session_draft()).unwrap();
    let mut native = Native::new(Size::new(820., 600.));
    click(&mut app, &mut native, "nav-overview");
    let key = |name| {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(name),
            modified_key: keyboard::Key::Named(name),
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        })
    };
    // Arrows walk the root destinations, then the pinned Changes, and
    // retain focus across changes.
    for (name, page) in [
        (Named::ArrowDown, Page::DesktopDock),
        (Named::ArrowDown, Page::DesktopWindows),
        (Named::ArrowDown, Page::DesktopMotion),
        (Named::ArrowDown, Page::General),
        (Named::ArrowDown, Page::Changes),
        (Named::ArrowUp, Page::General),
        (Named::ArrowUp, Page::DesktopMotion),
        (Named::ArrowUp, Page::DesktopWindows),
        (Named::Home, Page::DesktopProfiles),
        (Named::ArrowUp, Page::DesktopProfiles),
        (Named::ArrowDown, Page::Themes),
        (Named::End, Page::Changes),
        (Named::ArrowDown, Page::Changes),
    ] {
        let (messages, status) = native.event(app.settings_view(), key(name), Point::ORIGIN);
        assert_eq!(status, cosmic::iced::event::Status::Captured);
        assert!(!messages.is_empty(), "the sidebar keeps keyboard focus");
        for message in messages {
            let _ = app.update(message);
        }
        assert_eq!(app.page, page);
    }
    assert_eq!(
        serde_json::to_value(app.placement.session_draft()).unwrap(),
        draft
    );
    // An arrow in Search must edit the field, never select a sidebar destination.
    click(&mut app, &mut native, "rail-search");
    click(&mut app, &mut native, "settings-search");
    let (messages, _) = native.event(app.settings_view(), key(Named::ArrowDown), Point::ORIGIN);
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, Message::NavSelected(_) | Message::OpenRoot(_)))
    );
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}

#[test]
fn calendar_account_options_keep_disconnect_deliberate() {
    if child("calendar_account_options_keep_disconnect_deliberate") {
        return;
    }
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    for size in [Size::new(820., 600.), Size::new(1024., 720.)] {
        let (mut app, _) = Settings::init(Core::default(), Page::Calendar);
        let _ = app.update(Message::Resized(size));
        app.calendar.connection.backend = crate::calendar::Backend::Google;
        app.calendar.connection.account = "demo@example.com".into();
        app.calendar.connection.calendar_name = "Work".into();
        app.calendar.loading = false;
        let mut native = Native::new(size);
        assert!(app.help_open.is_none());
        // The preview is always shown; account options are reached by scrolling.
        click(&mut app, &mut native, "calendar-account-options");
        assert_eq!(app.help_open, Some("calendar-account"));
        click(&mut app, &mut native, "calendar-disconnect");
        assert!(app.calendar.confirm_disconnect);
        assert!(app.calendar.connection.connected());
        click(&mut app, &mut native, "calendar-keep-connected");
        assert!(!app.calendar.confirm_disconnect);
        assert!(app.calendar.connection.connected());
        let _ = app.update(Message::Page(Page::Widgets));
        assert!(app.help_open.is_none());
    }
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}

#[test]
fn all_settings_render_in_cosmic_light_dark_and_high_contrast_without_saving() {
    if child("all_settings_render_in_cosmic_light_dark_and_high_contrast_without_saving") {
        return;
    }
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    for size in [Size::new(820., 600.), Size::new(1024., 720.)] {
        for (name, theme) in [
            ("dark", cosmic::Theme::dark()),
            ("light", cosmic::Theme::light()),
            ("contrast", cosmic::Theme::dark_hc()),
        ] {
            let mut native = Native::new(size);
            for page in Page::ALL
                .into_iter()
                .filter(|page| *page != Page::WidgetLayout)
            {
                let (mut app, _) = Settings::init(Core::default(), page);
                let _ = app.update(Message::Resized(size));
                let pixels = native.render(app.settings_view(), &theme);
                if let Some(directory) = std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE") {
                    let path = std::path::PathBuf::from(directory).join(format!(
                        "cosmic-{name}-{}-{}.png",
                        page.key(),
                        size.width
                    ));
                    image::save_buffer(
                        path,
                        &pixels,
                        size.width as u32,
                        size.height as u32,
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
                // The main background follows the rendering theme, even in a headless test.
                let expected: cosmic::iced::Color = theme.cosmic().bg_color().into();
                let margin = native.bounds(app.settings_view(), "settings-viewport").x as usize + 2;
                let index = ((size.height as usize - 2) * size.width as usize + margin) * 4;
                for (got, want) in pixels[index..index + 3]
                    .iter()
                    .zip([expected.r, expected.g, expected.b])
                {
                    assert!(
                        (*got as f32 - want * 255.).abs() < 3.,
                        "{name} {page:?}: background follows COSMIC"
                    );
                }
            }
        }
    }
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}

#[test]
fn settings_follow_the_system_theme_independently_of_widget_colors() {
    if child("settings_follow_the_system_theme_independently_of_widget_colors") {
        return;
    }
    // libcosmic reads the light and dark themes from COSMIC's installed
    // defaults; without COSMIC (a build server) there is nothing to follow.
    if !std::path::Path::new("/usr/share/cosmic/com.system76.CosmicTheme.Light").exists() {
        eprintln!("COSMIC's theme defaults aren't installed; skipped");
        return;
    }
    let mode = std::path::PathBuf::from(std::env::var_os("XDG_CONFIG_HOME").unwrap())
        .join("cosmic/com.system76.CosmicTheme.Mode/v1/is_dark");
    std::fs::create_dir_all(mode.parent().unwrap()).unwrap();
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let mut config = Config::try_load().unwrap();
    for is_dark in [false, true] {
        std::fs::write(&mode, is_dark.to_string()).unwrap();
        let expected = controls_theme(&config);
        assert_eq!(expected.theme_type.is_dark(), is_dark);
        assert!(matches!(
            expected.theme_type,
            cosmic::theme::ThemeType::System { .. }
        ));
        for palette in ["lavender", "amber", "teal"] {
            config.theme = palette.into();
            assert_eq!(controls_theme(&config), expected);
        }
    }
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}

#[test]
fn visual_preview_selection_reorder_and_readability_have_separate_effects() {
    if child("visual_preview_selection_reorder_and_readability_have_separate_effects") {
        return;
    }
    Config::save_changes(json!({"appearance":{"opacity":70}, "location":{"name":"Zagreb","latitude":45.8,"longitude":15.9}})).unwrap();
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    let background = store.user_path(&crate::desktop::wallpaper::shared_key());
    assert!(background.starts_with(std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap()));
    std::fs::create_dir_all(background.parent().unwrap()).unwrap();
    let picture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wallpapers/teal.jpg");
    std::fs::write(background,format!("(output: \"all\", source: Path({}), filter_by_theme: false, rotation_frequency: 3600, filter_method: Lanczos, scaling_mode: Zoom, sampling_method: Alphanumeric)",serde_json::to_string(&picture).unwrap())).unwrap();
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    for size in [
        Size::new(820., 600.),
        Size::new(1024., 720.),
        Size::new(1280., 900.),
    ] {
        let (mut app, _) = Settings::init(Core::default(), Page::Appearance);
        // Complete the normal background jobs synchronously in this isolated
        // renderer; the init Task is deliberately never launched by tests.
        app.profiles = profiles::ProfilesEditor::default();
        app.profiles.open_home();
        for job in app.profiles.take_jobs() {
            app.profiles.update(job.run());
        }
        assert!(app.profiles.home_image().is_some());
        let _ = app.update(Message::Resized(size));
        let mut native = Native::new(size);
        for widget in [Widget::Weather, Widget::Devbox, Widget::Clock] {
            let _ = app.update(Message::AppearancePreview(widget));
            assert_eq!(app.appearance_preview, widget);
            reveal(&app, &mut native, "material-frosted");
            native.capture(
                app.settings_view(),
                &cosmic::Theme::dark(),
                &format!("polish-style-{}-{}", widget.key(), size.width),
            );
            if size.width >= 990. {
                let stage = native.bounds(app.settings_view(), "material-frosted");
                assert!(
                    stage.x >= 5. && stage.center_y() >= 0. && stage.center_y() < size.height,
                    "Visible wallpaper clipping probe: {stage:?}"
                );
                let pixels = native.render(app.settings_view(), &cosmic::Theme::dark());
                let index = ((stage.center_y() as usize) * size.width as usize
                    + (stage.x as usize - 5))
                    * 4;
                // The scene sits directly on the window surface. Wallpaper
                // must remain clipped to its stage, outside the content gutter.
                let theme = cosmic::Theme::dark();
                let background: cosmic::iced::Color =
                    theme.cosmic().background(theme.transparent).base.into();
                for (got, want) in
                    pixels[index..index + 3]
                        .iter()
                        .zip([background.r, background.g, background.b])
                {
                    assert!(
                        (*got as f32 - want * 255.).abs() < 3.,
                        "Wallpaper stays inside the preview stage: stage={stage:?}, index={index}, got={got}, want={want}, background={background:?}"
                    );
                }
            }
        }
        assert_eq!(
            std::fs::read(Config::path().unwrap()).unwrap(),
            before,
            "Inspecting samples saves nothing"
        );
        // Reach Advanced through the same disclosure and scrolling as a user.
        app.system_animations = false;
        reveal_click(&mut app, &mut native, "settings-advanced");
        reveal_click(&mut app, &mut native, "contrast-assist");
        assert!(Config::load().appearance.contrast_assist);
        assert_eq!(Config::load().appearance.opacity, 70);
        click(&mut app, &mut native, "settings-undo");
        assert!(!Config::load().appearance.contrast_assist);
        let _ = app.update(Message::Resized(size));
        native = Native::new(size);
        let _ = app.update(Message::Page(Page::Devbox));
        let previous = app.config.devbox.clone();
        reveal_click(&mut app, &mut native, "devbox-details-memory");
        assert!(app.devbox_details.contains(&DevboxSection::Memory));
        assert_eq!(
            app.config.devbox, previous,
            "Details must not toggle the reading"
        );
        reveal_click(&mut app, &mut native, "devbox-reorder");
        assert!(app.devbox_reordering);
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("polish-reorder-{}", size.width),
        );
        let original = app.config.devbox.ordered_sections();
        reveal_click(&mut app, &mut native, "devbox-move-cpu-1");
        assert_eq!(app.config.devbox.ordered_sections()[1], DevboxSection::Cpu);
        click(&mut app, &mut native, "settings-undo");
        assert_eq!(app.config.devbox.ordered_sections(), original);
        let _ = app.update(Message::Page(Page::Clock));
        assert!(!app.devbox_reordering);
        for page in [
            Page::Clock,
            Page::Projects,
            Page::Music,
            Page::Notes,
            Page::Focus,
            Page::DesktopProfiles,
        ] {
            let _ = app.update(Message::Page(page));
            native.capture(
                app.settings_view(),
                &cosmic::Theme::dark(),
                &format!("polish-{}-{}", page.key(), size.width),
            );
        }
        // The fixture has no accounts or project folders. Both illustrative
        // renderers still work, including the combined calendar's Tomorrow.
        for combined in [false, true] {
            let _ = app.update(Message::Page(Page::Calendar));
            app.config.glance.enabled = combined;
            app.config.glance.show_tomorrow = true;
            app.config.calendar_show_tomorrow = true;
            app.samples.update(&app.config);
            native.capture(
                app.settings_view(),
                &cosmic::Theme::dark(),
                &format!("polish-calendar-{combined}-{}", size.width),
            );
        }
        std::fs::write(Config::path().unwrap(), &before).unwrap();
    }
}

#[test]
fn motion_preference_ignores_stale_reads_and_can_be_undone() {
    if child("motion_preference_ignores_stale_reads_and_can_be_undone") {
        return;
    }
    let (mut app, _) = Settings::init(Core::default(), Page::General);
    let generation = app.motion_generation;
    let _ = app.update(Message::SystemMotion(generation, Some(true)));
    assert!(app.system_animations);
    let _ = app.update(Message::ReadSystemMotion);
    let _ = app.update(Message::SystemMotion(generation, Some(true)));
    let _ = app.update(Message::SystemMotion(app.motion_generation, Some(false)));
    assert!(!app.system_animations);
    let _ = app.update(Message::SystemMotion(generation, Some(true)));
    assert!(!app.system_animations);
    let _ = app.update(Message::SettingsAnimations(false));
    assert!(!Config::load().settings_animations);
    let _ = app.update(Message::UndoPreferences);
    assert!(Config::load().settings_animations);
    assert!(
        !app.system_animations,
        "The app toggle never overrides Reduce motion"
    );
}

#[test]
fn review_r1_gallery_keyboard_actions_are_reachable() {
    if child("review_r1_gallery_keyboard_actions_are_reachable") {
        return;
    }
    use cosmic::iced::advanced::{
        clipboard,
        renderer::Headless,
        widget::{
            Operation,
            operation::{Focusable, Outcome as OpOutcome, focusable},
        },
    };
    use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
    use cosmic::iced::{Rectangle, Vector, keyboard};
    #[derive(Default)]
    struct Focused {
        bounds: Option<Rectangle>,
        viewport: Option<Rectangle>,
        offset: Vector,
        clip: Option<Rectangle>,
        pending_scroll: Option<(Rectangle, Vector)>,
    }
    impl Operation for Focused {
        fn traverse(&mut self, f: &mut dyn FnMut(&mut dyn Operation)) {
            let previous = (self.offset, self.clip);
            if let Some((mut viewport, translation)) = self.pending_scroll.take() {
                viewport.x -= self.offset.x;
                viewport.y -= self.offset.y;
                self.clip = Some(viewport);
                self.offset += translation;
            }
            f(self);
            (self.offset, self.clip) = previous;
        }
        fn scrollable(
            &mut self,
            _: Option<&cosmic::widget::Id>,
            bounds: Rectangle,
            _: Rectangle,
            translation: Vector,
            _: &mut dyn cosmic::iced::advanced::widget::operation::Scrollable,
        ) {
            self.pending_scroll = Some((bounds, translation));
        }
        fn focusable(
            &mut self,
            _: Option<&cosmic::widget::Id>,
            mut bounds: Rectangle,
            s: &mut dyn Focusable,
        ) {
            if s.is_focused() {
                bounds.x -= self.offset.x;
                bounds.y -= self.offset.y;
                self.bounds = Some(bounds);
                self.viewport = self.clip;
            }
        }
    }
    let mut renderer =
        cosmic::iced::futures::executor::block_on(<cosmic::Renderer as Headless>::new(
            cosmic::font::default(),
            cosmic::iced::Pixels(14.),
            Some("tiny-skia"),
        ))
        .unwrap();
    let mut missing = Vec::new();
    for size in [Size::new(820., 600.), Size::new(1024., 720.)] {
        for matching in [false, true] {
            let (mut app, _) = Settings::init(Core::default(), Page::Themes);
            // Each size/option is an independent keyboard case. The preceding
            // case deliberately edits its preview, so a new Settings session
            // correctly restores that work awaiting Review. Start this case
            // with an explicit discard before selecting its fresh preview.
            let _ = app.update(Message::Profiles(profiles::Message::Discard));
            let _ = app.update(Message::Resized(size));
            app.profiles
                .update(profiles::Message::Select(profiles::Selection::Preset(
                    "theme-soft",
                )));
            app.profiles
                .update(profiles::Message::ThemeWallpaper(matching));
            for job in app.profiles.take_theme_jobs() {
                app.profiles.update(job.run());
            }
            assert!(
                app.profiles.can_apply(),
                "fixture must have a ready preview: {:?}",
                app.profiles.plan()
            );
            let mut ui =
                UserInterface::build(app.settings_view(), size, Cache::default(), &mut renderer);
            let mut reached = std::collections::BTreeSet::new();
            for _ in 0..100 {
                let mut operation: Box<dyn Operation> = Box::new(focusable::focus_next());
                loop {
                    ui.operate(&renderer, operation.as_mut());
                    match operation.finish() {
                        OpOutcome::Chain(next) => operation = next,
                        _ => break,
                    }
                }
                let mut bounds = Focused::default();
                ui.operate(&renderer, &mut bounds);
                // Exercise the same ordered focus reveal used by Tab in the app.
                // No pointer movement or wheel scrolling is permitted here.
                let mut operation: Box<dyn Operation> =
                    Box::new(navigation::FocusReveal::default());
                loop {
                    ui.operate(&renderer, operation.as_mut());
                    match operation.finish() {
                        OpOutcome::Chain(next) => operation = next,
                        _ => break,
                    }
                }
                let mut bounds = Focused::default();
                ui.operate(&renderer, &mut bounds);
                if let (Some(target), Some(viewport)) = (bounds.bounds, bounds.viewport) {
                    assert!(
                        target.y >= viewport.y - 0.5
                            && target.y + target.height <= viewport.y + viewport.height + 0.5,
                        "Keyboard focus must be visible without scrolling: {target:?} in {viewport:?}"
                    );
                }
                for key in [
                    keyboard::Key::Named(keyboard::key::Named::Enter),
                    keyboard::Key::Character(" ".into()),
                ] {
                    let event = Event::Keyboard(keyboard::Event::KeyPressed {
                        key: key.clone(),
                        modified_key: key,
                        physical_key: keyboard::key::Physical::Unidentified(
                            keyboard::key::NativeCode::Unidentified,
                        ),
                        location: keyboard::Location::Standard,
                        modifiers: keyboard::Modifiers::empty(),
                        text: None,
                        repeat: false,
                    });
                    let mut messages = Vec::new();
                    ui.update(
                        &[event],
                        mouse::Cursor::Available(Point::new(2., 2.)),
                        &mut renderer,
                        &mut clipboard::Null,
                        &mut messages,
                    );
                    for message in messages {
                        let label = format!("{message:?}");
                        if matches!(
                            message,
                            Message::Profiles(profiles::Message::ThemeColors(true))
                        ) && !reached.contains(&label)
                        {
                            let target = bounds
                                .bounds
                                .expect("From wallpaper has keyboard focus");
                            assert!(
                                target.x >= 0.
                                    && target.y >= 0.
                                    && target.x + target.width <= size.width
                                    && target.y + target.height <= size.height,
                                "primary option must be visible: {target:?}"
                            );
                            if let Some(viewport) = bounds.viewport {
                                assert!(
                                    target.y >= viewport.y
                                        && target.y + target.height <= viewport.y + viewport.height,
                                    "primary option must be inside its scroll viewport: {target:?} in {viewport:?}"
                                );
                            }
                            ui.draw(
                                &mut renderer,
                                &cosmic::Theme::dark(),
                                &cosmic::iced::advanced::renderer::Style::default(),
                                mouse::Cursor::Unavailable,
                            );
                            let pixels = renderer.screenshot(
                                Size::new(size.width as u32, size.height as u32),
                                1.,
                                cosmic::iced::Color::TRANSPARENT,
                            );
                            if let Some(dir) = std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE") {
                                let dir = std::path::PathBuf::from(dir);
                                std::fs::create_dir_all(&dir).unwrap();
                                image::save_buffer(
                                    dir.join(format!(
                                        "theme-wallpaper-keyboard-{matching}-{}.png",
                                        size.width
                                    )),
                                    &pixels,
                                    size.width as u32,
                                    size.height as u32,
                                    image::ColorType::Rgba8,
                                )
                                .unwrap();
                            }
                        }
                        if reached.insert(label.clone()) {
                            println!("{size:?}: {label}, focused bounds {:?}", bounds.bounds);
                        }
                    }
                }
            }
            // Styles are tiles on the page itself, and every choice applies on
            // its own, so there is no browser or Apply. Saving, sharing and the
            // matching wallpaper moved to Looks.
            for expected in [
                r#"Profiles(Select(Preset("theme-outline")))"#,
                r#"ToggleSection("appearance-all-styles")"#,
                "Profiles(ThemeColors(true))",
                "Profiles(ThemeColors(false))",
                "Profiles(Color(Mode(Dark)))",
                "Corners(VeryRound)",
            ] {
                if !reached.contains(expected) {
                    missing.push(format!("{size:?}: keyboard cannot activate {expected}"));
                }
            }
            drop(ui);
            let _ = app.update(Message::Profiles(profiles::Message::ThemeWallpaper(
                !matching,
            )));
            assert_eq!(
                app.profiles.profile().unwrap().wallpaper.is_some(),
                !matching
            );
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn windows_page_ends_with_workspaces_and_the_menu_lists_every_page() {
    if child("windows_page_ends_with_workspaces_and_the_menu_lists_every_page") {
        return;
    }
    let size = Size::new(1020., 1500.);
    let (mut app, _) = Settings::init(Core::default(), Page::DesktopWorkspaces);
    let _ = app.update(Message::Resized(size));
    assert_eq!(app.page, Page::DesktopWindows, "Workspaces opens Windows");
    let mut native = Native::new(size);
    native.capture(
        app.view(),
        &cosmic::Theme::dark(),
        "windows-with-workspaces",
    );
    assert!(
        native
            .try_bounds(app.view(), "windows-workspaces")
            .is_some()
    );
    let mut last = 0.;
    for (_, pages) in Page::SIDEBAR {
        for page in pages {
            let entry = native.bounds(app.view(), &format!("nav-{}", page.key()));
            assert!(entry.y > last, "{page:?} follows the previous entry");
            last = entry.y;
        }
    }
}

#[test]
fn color_control_offers_suggestions_and_the_picker_and_applies_a_choice() {
    if child("color_control_offers_suggestions_and_the_picker_and_applies_a_choice") {
        return;
    }
    let size = Size::new(1020., 1300.);
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::Themes);
    let _ = app.update(Message::Resized(size));
    let mut native = Native::new(size);
    assert!(
        native
            .try_bounds(app.settings_view(), "color-panel")
            .is_none(),
        "closed until asked"
    );
    click(&mut app, &mut native, "theme-color-custom");
    click(&mut app, &mut native, "theme-color-hex");
    assert!(
        native
            .try_bounds(app.settings_view(), "color-panel")
            .is_some()
    );
    native.capture(
        app.settings_view(),
        &cosmic::Theme::dark(),
        "color-control-open",
    );
    click(&mut app, &mut native, "color-quick-2");
    let chosen = crate::settings::color_well::QUICK[2];
    assert_eq!(
        app.profiles.color_preferences(&app.config).accent,
        Some(chosen)
    );
    assert_eq!(
        std::fs::read(Config::path().unwrap()).unwrap(),
        before,
        "choosing a suggestion updates only the preview"
    );
    assert!(
        native
            .try_bounds(app.settings_view(), "color-panel")
            .is_none(),
        "a choice closes the panel"
    );
    native.capture(
        app.settings_view(),
        &cosmic::Theme::dark(),
        "color-control-chosen",
    );
    // Appearance has no Apply button: "Done" closes the custom color and the
    // choice applies on its own, with one Undo in the footer.
    let done = native.text_bounds(app.settings_view(), "Done").center();
    for event in [
        mouse::Event::CursorMoved { position: done },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        for message in native.event(app.settings_view(), Event::Mouse(event), done).0 {
            crate::settings::tests::send_settings(&mut app, message);
        }
    }
    assert_eq!(Config::load().appearance.accent, Some(chosen));
    crate::settings::tests::native_click(&mut app, &mut native, "settings-undo");
    assert_eq!(
        std::fs::read(Config::path().unwrap()).unwrap(),
        before,
        "one Undo reverses the applied choice"
    );
}

#[test]
fn fb10_type_wall_pages_are_bounded_and_browsing_keeps_preferences() {
    if child("fb10_type_wall_pages_are_bounded_and_browsing_keeps_preferences") {
        return;
    }
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::Clock);
    let _ = app.update(Message::ToggleScene);
    let _ = app.update(Message::FontMenu(Some(FontTarget::Clock)));
    assert!(app.font_loading);
    let manifest = (0..31)
        .map(|i| format!("interface\tFamily {i:02}\t400\n"))
        .collect::<String>();
    let _ = app.update(Message::FontCatalogReady(
        crate::desktop::appearance::catalog::FontCatalog::from_manifest(&manifest),
    ));
    let _ = app.update(Message::ObjectTab(navigation::ObjectTab::Design));
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        let _ = app.update(Message::Resized(size));
        let _ = app.update(Message::FontPage(0));
        let mut native = Native::new(size);
        for i in 0..31 {
            assert_eq!(
                native
                    .try_bounds(
                        app.settings_view(),
                        &format!("font-choice-clock-Family {i:02}")
                    )
                    .is_some(),
                i < 12
            );
        }
        reveal_click(&mut app, &mut native, "font-next-clock");
        assert_eq!(app.font_page, 1);
        for i in 0..31 {
            assert_eq!(
                native
                    .try_bounds(
                        app.settings_view(),
                        &format!("font-choice-clock-Family {i:02}")
                    )
                    .is_some(),
                (12..24).contains(&i)
            );
        }
        let _ = app.update(Message::FontPage(usize::MAX));
        assert_eq!(app.font_page, 2);
        let _ = app.update(Message::FontSearch("Family 30".into()));
        assert_eq!(app.font_page, 0);
        assert_eq!(app.widget_font_families(), vec!["Family 30"]);
        reveal(&app, &mut native, "font-list-clock");
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("fb10-type-search-{}", size.width),
        );
        let _ = app.update(Message::FontSearch(String::new()));
    }
    let _ = app.update(Message::FontMenu(None));
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}

#[test]
fn fb10_type_choice_click_and_undo_preserve_face_and_other_fonts() {
    if child("fb10_type_choice_click_and_undo_preserve_face_and_other_fonts") {
        return;
    }
    let (mut app, _) = Settings::init(Core::default(), Page::Clock);
    let before = Config::load();
    let _ = app.update(Message::ToggleScene);
    let _ = app.update(Message::FontMenu(Some(FontTarget::Clock)));
    let _ = app.update(Message::FontCatalogReady(
        crate::desktop::appearance::catalog::FontCatalog::from_manifest(
            "interface\tNoto Serif\t400\n",
        ),
    ));
    let size = Size::new(820., 600.);
    let _ = app.update(Message::Resized(size));
    let _ = app.update(Message::ObjectTab(navigation::ObjectTab::Design));
    let mut native = Native::new(size);
    reveal_click(&mut app, &mut native, "font-choice-clock-Noto Serif");
    let chosen = Config::load();
    assert_eq!(chosen.clock_font.as_deref(), Some("Noto Serif"));
    assert_eq!(chosen.clock, before.clock);
    assert_eq!(chosen.widget_font, before.widget_font);
    assert!(app.font_menu.is_none());
    let _ = app.update(Message::UndoDesktop);
    assert_eq!(Config::load().clock_font, before.clock_font);
    assert_eq!(Config::load().clock, before.clock);
}

#[test]
fn widget_fonts_list_installed_families_and_save_a_choice() {
    if child("widget_fonts_list_installed_families_and_save_a_choice") {
        return;
    }
    let size = Size::new(1020., 720.);
    // The widget font lives with the other fonts, on Fonts and icons.
    let (mut app, initial) = Settings::init(Core::default(), Page::DesktopAppearance);
    crate::settings::tests::settle_settings(&mut app, initial);
    let _ = app.update(Message::Resized(size));
    let mut native = Native::new(size);
    assert!(
        native
            .try_bounds(app.settings_view(), "font-list-text")
            .is_none()
    );
    app.system_animations = false;
    reveal_click(&mut app, &mut native, "font-text");
    assert!(
        native
            .try_bounds(app.settings_view(), "font-list-text")
            .is_some()
    );
    native.capture(
        app.settings_view(),
        &cosmic::Theme::dark(),
        "widget-fonts-open",
    );
    let _ = app.update(Message::FontSearch("zzz-no-such-font".into()));
    assert!(app.widget_font_families().is_empty());
    let _ = app.update(Message::FontSearch(String::new()));
    let _ = app.update(Message::WidgetFont(
        FontTarget::Text,
        Some("Noto Sans Mono".into()),
    ));
    assert_eq!(
        Config::load().widget_font.as_deref(),
        Some("Noto Sans Mono")
    );
    assert!(app.font_menu.is_none(), "a choice closes the list");
    let _ = app.update(Message::WidgetFont(
        FontTarget::Clock,
        Some("Noto Serif".into()),
    ));
    assert_eq!(Config::load().clock_font.as_deref(), Some("Noto Serif"));
    native.capture(
        app.settings_view(),
        &cosmic::Theme::dark(),
        "widget-fonts-chosen",
    );
    let _ = app.update(Message::WidgetFont(FontTarget::Text, None));
    assert_eq!(Config::load().widget_font, None, "back to the style's font");
}

#[test]
fn appearance_shows_one_row_of_styles_and_every_style_on_request() {
    let size = Size::new(1024., 720.);
    let (mut app, _) = Settings::init(Core::default(), Page::Themes);
    let _ = app.update(Message::Resized(size));
    let shown = |app: &Settings| {
        let mut native = crate::native_test::Native::new(size);
        crate::desktop_theme::Family::ALL
            .into_iter()
            .filter(|family| {
                native
                    .try_bounds(app.settings_view(), &format!("preset-{}", family.preset()))
                    .is_some()
            })
            .count()
    };
    let row = shown(&app);
    assert!(row >= 2 && row < crate::desktop_theme::Family::ALL.len());
    let _ = app.update(Message::ToggleSection("appearance-all-styles"));
    assert_eq!(shown(&app), crate::desktop_theme::Family::ALL.len());
    // Choosing a tile previews that style in place.
    let _ = app.update(Message::Profiles(profiles::Message::Select(
        profiles::Selection::Preset("theme-neon"),
    )));
    assert_eq!(
        *app.profiles.selection(),
        profiles::Selection::Preset("theme-neon")
    );
    // Whether looks color the dock and panel lives with the dock now. The
    // Dock page needs a desktop to draw its switch, so the isolated
    // polish_e_dock_shape_follow_style_and_looks_header checks it there.
    let mut native = crate::native_test::Native::new(size);
    assert!(native.text_all(app.settings_view(), "Style the dock and panel").is_empty());
}

#[test]
fn calendar_subscription_form_and_selection_use_real_controls() {
    if child("calendar_subscription_form_and_selection_use_real_controls") {
        return;
    }
    let (mut app, _) = Settings::init(Core::default(), Page::Calendar);
    app.calendar.client_ready = false;
    for size in [Size::new(820., 600.), Size::new(1024., 720.)] {
        let _ = app.update(Message::Resized(size));
        let mut native = Native::new(size);
        app.calendar.subscription_form = false;
        // The preview is always shown; the link button is reached by scrolling.
        native.scroll(app.settings_view(), "settings-content", 0.);
        click(&mut app, &mut native, "calendar-subscription-open");
        assert!(app.calendar.subscription_form);
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("calendar-link-{}", size.width),
        );
        let secret = "https://example.invalid/private-credential.ics";
        let message =
            calendar::Message::SubscriptionUrl(crate::calendar::SecretText(secret.into()));
        assert!(!format!("{message:?}").contains(secret));
        let _ = app.update(Message::Calendar(message));
        let _ = app.update(Message::Calendar(calendar::Message::SubscriptionForm(
            false,
        )));
        assert!(app.calendar.subscription_url.0.is_empty());
    }
    let choices: Vec<_> = ["Work", "Personal"]
        .into_iter()
        .map(|name| crate::calendar::CalendarChoice {
            id: name.into(),
            name: name.into(),
            primary: false,
            color: String::new(),
        })
        .collect();
    let connection = crate::calendar::Connection {
        backend: crate::calendar::Backend::Subscription,
        account: "Calendar subscriptions".into(),
        calendar_id: choices[0].id.clone(),
        calendar_name: choices[0].name.clone(),
        revision: "fixture".into(),
        credential: "opaque-keyring-id".into(),
        subscriptions: choices.clone(),
        selections: vec![choices[0].clone()],
    };
    std::fs::write(
        crate::calendar::account_path().unwrap(),
        serde_json::to_vec(&connection).unwrap(),
    )
    .unwrap();
    app.calendar.connection = connection;
    app.calendar.choices = choices;
    let mut native = Native::new(Size::new(1024., 720.));
    click(&mut app, &mut native, "calendar-choice-1");
    assert_eq!(crate::calendar::load_connection().selections.len(), 2);
    click(&mut app, &mut native, "calendar-choice-0");
    assert_eq!(
        crate::calendar::load_connection().selections[0].name,
        "Personal"
    );
    click(&mut app, &mut native, "calendar-choice-1");
    assert_eq!(crate::calendar::load_connection().selections.len(), 1);
    assert!(app.calendar.error.is_some());
    app.calendar.error = None;
    native.capture(
        app.settings_view(),
        &cosmic::Theme::dark(),
        "calendar-subscriptions",
    );
    // Removing a subscription asks first: its private link isn’t kept.
    click(&mut app, &mut native, "calendar-remove-1");
    assert_eq!(app.calendar.confirm_remove.as_deref(), Some("Personal"));
    native.text_bounds(app.settings_view(), "Remove “Personal”?");
    native.text_bounds(app.settings_view(), "You’ll need its private link to add it again.");
    native.capture(app.settings_view(), &cosmic::Theme::dark(), "calendar-remove-confirm");
    native.capture(app.settings_view(), &cosmic::Theme::light(), "calendar-remove-confirm-light");
    click(&mut app, &mut native, "calendar-remove-cancel");
    assert!(app.calendar.confirm_remove.is_none());
    // Without its confirmation, a removal does nothing.
    let _ = app.update(Message::Calendar(calendar::Message::RemoveSubscription(
        "Personal".into(),
    )));
    assert!(!app.calendar.busy);
    click(&mut app, &mut native, "calendar-remove-1");
    click(&mut app, &mut native, "calendar-remove-confirm");
    assert!(app.calendar.busy, "the confirmed removal starts");
    assert!(app.calendar.confirm_remove.is_none());
    app.calendar.busy = false;
    let _ = app.update(Message::CalendarPrivacy(true));
    assert!(Config::load().calendar_private);
    native.capture(
        app.settings_view(),
        &cosmic::Theme::dark(),
        "calendar-private-settings",
    );
    click(&mut app, &mut native, "settings-undo");
    assert!(!Config::load().calendar_private);
}

/// Actual native controls, including compact scrolling and sticky Apply.
#[test]
fn fb11_native_canvas_selection_style_and_apply_at_both_sizes() {
    if child("fb11_native_canvas_selection_style_and_apply_at_both_sizes") {
        return;
    }
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        let before = std::fs::read(Config::path().unwrap()).unwrap();
        let (mut app, _) = Settings::init(Core::default(), Page::Overview);
        let _ = app.update(Message::Resized(size));
        let mut native = Native::new(size);
        reveal_click(&mut app, &mut native, "canvas-start");
        assert!(app.canvas.is_some());
        reveal_click(&mut app, &mut native, "canvas-object-clock");
        assert_eq!(app.placement.selected(), Some(Widget::Clock));
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("fb11-canvas-{}-initial", size.width),
        );
        // The existing ignored-keyboard subscription maps arrows to these
        // geometry messages. No completion may leak through to the live file.
        let event = Event::Keyboard(cosmic::iced::keyboard::Event::KeyPressed {
            key: cosmic::iced::keyboard::Key::Named(cosmic::iced::keyboard::key::Named::ArrowRight),
            modified_key: cosmic::iced::keyboard::Key::Named(
                cosmic::iced::keyboard::key::Named::ArrowRight,
            ),
            physical_key: cosmic::iced::keyboard::key::Physical::Code(
                cosmic::iced::keyboard::key::Code::ArrowRight,
            ),
            location: cosmic::iced::keyboard::Location::Standard,
            modifiers: cosmic::iced::keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        });
        if let Some(edit) = placement::input(event, cosmic::iced::event::Status::Ignored) {
            let _ = app.update(Message::Placement(edit));
        } else {
            panic!("arrow did not route");
        }
        // Clock's look moved to its Design tab; a draft keeps this path for saved work.
        let _ = app.update(Message::Canvas(canvas::Message::Style(
            "/clock/face",
            serde_json::json!("analog"),
        )));
        assert_eq!(
            app.canvas_config().clock.face,
            Some(crate::widget_style::resolver::ClockFace::Analog)
        );
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("fb11-canvas-{}-inspector", size.width),
        );
        native.capture(
            app.settings_view(),
            &cosmic::Theme::light(),
            &format!("fb11-canvas-{}-light", size.width),
        );
        click(&mut app, &mut native, "canvas-apply");
        assert!(app.canvas.is_none(), "{}", app.status);
        assert!(!app.error, "{}", app.status);
        let _ = app.update(Message::UndoDesktop);
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
    }
}

#[test]
fn fb11_native_canvas_drag_resize_cancel_and_inheritance() {
    if child("fb11_native_canvas_drag_resize_cancel_and_inheritance") {
        return;
    }
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::Overview);
    let size = Size::new(820., 600.);
    let _ = app.update(Message::Resized(size));
    let mut native = Native::new(size);
    reveal_click(&mut app, &mut native, "canvas-start");
    reveal_click(&mut app, &mut native, "canvas-object-clock");
    reveal(&app, &mut native, "placement-card-clock");
    let card = native.bounds(app.settings_view(), "placement-card-clock");
    let original = app.placement.state();
    let start = card.center();
    let end = Point::new(start.x + 48., start.y + 20.);
    for (event, point) in [
        (mouse::Event::CursorMoved { position: start }, start),
        (mouse::Event::ButtonPressed(mouse::Button::Left), start),
        (mouse::Event::CursorMoved { position: end }, end),
        (mouse::Event::ButtonReleased(mouse::Button::Left), end),
    ] {
        for message in native
            .event(app.settings_view(), Event::Mouse(event.clone()), point)
            .0
        {
            let _ = app.update(message);
        }
        if let Some(edit) =
            placement::input(Event::Mouse(event), cosmic::iced::event::Status::Ignored)
        {
            let _ = app.update(Message::Placement(edit));
        }
    }
    assert_ne!(
        app.placement.state(),
        original,
        "native mouse drag must move the widget"
    );
    reveal_click(&mut app, &mut native, "layout-resize-mode");
    let _ = app.update(Message::Placement(placement::Message::Nudge([0, 1], false)));
    let _ = app.update(Message::Canvas(canvas::Message::Style(
        "/clock/surface",
        serde_json::json!("none"),
    )));
    assert_eq!(
        app.canvas_config().clock.surface,
        Some(crate::widget_style::resolver::ClockSurface::None)
    );
    let _ = app.update(Message::Canvas(canvas::Message::Reset("/clock/surface")));
    assert_eq!(app.canvas_config().clock.surface, None);
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
    click(&mut app, &mut native, "canvas-cancel");
    assert!(app.canvas.is_none());
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}

fn mix_native_click(app: &mut Settings, native: &mut Native, id: &str) {
    use cosmic::iced::futures::StreamExt;
    use cosmic::iced::runtime::Action;
    reveal(app, native, id);
    let point = native.bounds(app.settings_view(), id).center();
    assert!(point.y < native.size.height && point.y >= 0.);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        for event in [
            mouse::Event::CursorMoved { position: point },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            let messages = native
                .event(app.settings_view(), Event::Mouse(event), point)
                .0;
            let mut tasks = std::collections::VecDeque::new();
            for message in messages {
                tasks.push_back(app.update(message));
            }
            while let Some(task) = tasks.pop_front() {
                if let Some(mut stream) = cosmic::iced::runtime::task::into_stream(task) {
                    while let Some(action) = stream.next().await {
                        match action {
                            Action::Output(cosmic::Action::App(message)) => {
                                tasks.push_back(app.update(message))
                            }
                            Action::Widget(operation) => {
                                native.operate(app.settings_view(), operation)
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    });
}

#[test]
fn fb12_native_mix_choices_preview_cancel_and_apply_fit_both_sizes() {
    if child("fb12_native_mix_choices_preview_cancel_and_apply_fit_both_sizes") {
        return;
    }
    use crate::desktop::{DesktopStore, profiles as records, wallpaper as backgrounds};
    let root = std::path::PathBuf::from(std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap());
    // Supply the desktop being tested instead of inheriting this machine's
    // installed COSMIC defaults (absent on plain Ubuntu CI runners).
    for (namespace, key, value) in [
        (
            "com.system76.CosmicPanel",
            "entries",
            "[\"Panel\",\"Dock\"]",
        ),
        ("com.system76.CosmicPanel.Dock", "anchor", "Bottom"),
        (
            "com.system76.CosmicPanel.Dock",
            "plugins_center",
            "Some([\"com.system76.CosmicAppList\"])",
        ),
        ("com.system76.CosmicPanel.Panel", "anchor", "Top"),
        (
            "com.system76.CosmicPanel.Panel",
            "plugins_wings",
            "Some(([], [\"com.system76.CosmicAppletTime\"]))",
        ),
    ] {
        let file = root
            .join("system/cosmic")
            .join(namespace)
            .join("v1")
            .join(key);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, value).unwrap();
    }
    let directory = root
        .join("config/cosmic")
        .join(backgrounds::NAMESPACE)
        .join("v1");
    std::fs::create_dir_all(&directory).unwrap();
    let entry = backgrounds::BgEntry::new(
        backgrounds::SHARED,
        backgrounds::BgSource::Color(backgrounds::BgColor::Single([0.1, 0.2, 0.3])),
    );
    std::fs::write(directory.join("all"), ron::to_string(&entry).unwrap()).unwrap();
    for size in [Size::new(820., 552.), Size::new(1280., 752.)] {
        let original = std::fs::read(Config::path().unwrap()).unwrap();
        let (mut app, _) = Settings::init(Core::default(), Page::DesktopProfiles);
        let _ = app.update(Message::Resized(size));
        let mut native = Native::new(size);
        mix_native_click(&mut app, &mut native, "looks-more");
        mix_native_click(&mut app, &mut native, "mix-start");
        assert!(app.profiles.mix.is_some());
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("fb12-mix-{}-sources", size.width),
        );
        for id in [
            "mix-source-desktop",
            "mix-source-paper-studio",
            "mix-source-tidal",
        ] {
            mix_native_click(&mut app, &mut native, id);
        }
        for job in app.profiles.take_theme_jobs() {
            let _ = app.update(Message::Profiles(job.run()));
        }
        assert!(app.profiles.profile().unwrap().composition.is_some());
        assert!(!app.profiles.mix.as_ref().unwrap().choosing);
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), original);
        native.scroll(app.settings_view(), "settings-content", 0.);
        let scene = native.bounds(app.settings_view(), "desktop-scene");
        assert!(
            scene.y + scene.height <= size.height - 45.,
            "scene {scene:?}; content {size:?}"
        );
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("fb12-mix-{}-preview", size.width),
        );
        native.capture(
            app.settings_view(),
            &cosmic::Theme::light(),
            &format!("fb12-mix-{}-light", size.width),
        );
        assert!(
            app.profiles.can_apply(),
            "{:?} {:?}",
            app.profiles.plan(),
            app.profiles.notice
        );
        mix_native_click(&mut app, &mut native, "mix-save");
        assert!(app.profiles.name.is_some());
        assert!(native.bounds(app.settings_view(), "profile-name").y < size.height - 60.);
        let _ = app.update(Message::Profiles(profiles::Message::NameInput(format!(
            "Native mix {}",
            size.width
        ))));
        mix_native_click(&mut app, &mut native, "mix-save");
        assert!(matches!(
            app.profiles.selection(),
            profiles::Selection::Saved(_)
        ));
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), original);
        assert!(app.profiles.profile().unwrap().composition.is_some());
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("fb12-mix-{}-saved", size.width),
        );
        mix_native_click(&mut app, &mut native, "profiles-confirm");
        assert!(app.profiles.mix.is_none());
        let store = DesktopStore::from_env().unwrap();
        assert!(store.latest_change().unwrap().is_some());
        let _ = app.update(Message::UndoDesktop);
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), original);
        // Source metadata exists only in saved setup records, not preferences.
        assert!(
            records::capture(&store, "Applied")
                .unwrap()
                .profile
                .composition
                .is_none()
        );
    }
}

#[test]
fn fb13_native_share_actions_fit_compact_and_wide_setup_review() {
    if child("fb13_native_share_actions_fit_compact_and_wide_setup_review") {
        return;
    }
    for size in [Size::new(820., 552.), Size::new(1280., 752.)] {
        let (mut app, _) = Settings::init(Core::default(), Page::DesktopProfiles);
        let _ = app.update(Message::Resized(size));
        let _ = app.update(Message::Profiles(profiles::Message::Select(
            profiles::Selection::Preset("spine"),
        )));
        let mut native = Native::new(size);
        super::tests::native_reveal(&app, &mut native, "setup-library-actions");
        click(&mut app, &mut native, "setup-library-actions");
        assert!(app.open_sections.contains("setup-library-actions"));
        reveal(&app, &mut native, "profiles-share-image");
        // Do not execute the returned asynchronous dialog task in this UI test.
        let bounds = native.bounds(app.settings_view(), "profiles-share-image");
        assert!(
            bounds.x >= 0. && bounds.x + bounds.width <= size.width,
            "{bounds:?}"
        );
        assert!(
            bounds.y >= 0.
                && bounds.y + bounds.height <= {
                    let viewport = native.bounds(app.settings_view(), "settings-viewport");
                    viewport.y + viewport.height
                },
            "{bounds:?}"
        );
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("fb13-spine-{}", size.width),
        );
        let messages: Vec<_> = [
            mouse::Event::CursorMoved {
                position: bounds.center(),
            },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ]
        .into_iter()
        .flat_map(|event| {
            native
                .event(app.settings_view(), Event::Mouse(event), bounds.center())
                .0
        })
        .collect();
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::Profiles(profiles::Message::ShareImage)))
        );
        app.profiles.update(profiles::Message::ShareImage);
        assert!(app.profiles.sharing());
        let messages: Vec<_> = [
            mouse::Event::CursorMoved {
                position: bounds.center(),
            },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ]
        .into_iter()
        .flat_map(|event| {
            native
                .event(app.settings_view(), Event::Mouse(event), bounds.center())
                .0
        })
        .collect();
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::Profiles(profiles::Message::CancelShare)))
        );
        app.profiles.update(profiles::Message::CancelShare);
        assert!(!app.profiles.sharing());
    }
}

/// Renders the real views for visual review. Writes nothing outside the fixture
/// and the evidence folder, and does nothing without an evidence folder.
#[test]
fn ux_native_tour() {
    if std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE").is_none() {
        return;
    }
    if child("ux_native_tour") {
        return;
    }
    use crate::desktop::wallpaper::{BgEntry, BgSource, bundled_dir, bundled_path};
    use crate::settings::navigation::ObjectTab;
    use crate::settings::tests::{native_send, settle_settings};
    let (probe, _) = Settings::init(Core::default(), Page::DesktopWallpaper);
    let bundled = bundled_dir(probe.wallpaper.store().unwrap());
    drop(probe);
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wallpapers");
    for (name, _) in crate::desktop::wallpaper::BUNDLED {
        let target = bundled_path(&bundled, name);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(bundled_path(&assets, name), &target).unwrap();
    }
    let root = std::path::PathBuf::from(std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap());
    // A complete, isolated applet catalog keeps the scene representative of an
    // installed desktop. Missing metadata otherwise draws fallback puzzle icons.
    let applications = root.join("applications");
    std::fs::create_dir_all(&applications).unwrap();
    for (id, name, icon) in [
        (
            "CosmicPanelWorkspacesButton",
            "Workspaces",
            "view-grid-symbolic",
        ),
        (
            "CosmicPanelAppButton",
            "Applications",
            "view-app-grid-symbolic",
        ),
        (
            "CosmicPanelLauncherButton",
            "Launcher",
            "system-search-symbolic",
        ),
        (
            "CosmicAppList",
            "Open applications",
            "preferences-system-windows-symbolic",
        ),
        (
            "CosmicAppletMinimize",
            "Show desktop",
            "user-desktop-symbolic",
        ),
        (
            "CosmicAppletTime",
            "Time and date",
            "preferences-system-time-symbolic",
        ),
        ("CosmicAppletAudio", "Sound", "audio-volume-high-symbolic"),
        (
            "CosmicAppletNetwork",
            "Network",
            "network-wireless-signal-excellent-symbolic",
        ),
        (
            "CosmicAppletBattery",
            "Battery",
            "battery-level-80-symbolic",
        ),
        ("CosmicAppletPower", "Power", "system-shutdown-symbolic"),
    ] {
        std::fs::write(
            applications.join(format!("com.system76.{id}.desktop")),
            format!("[Desktop Entry]\nName={name}\nIcon={icon}\nX-CosmicApplet=true\n"),
        )
        .unwrap();
    }
    for (namespace, key, value) in [
        (
            "com.system76.CosmicPanel",
            "entries",
            r#"["Panel", "Dock"]"#,
        ),
        ("com.system76.CosmicPanel.Panel", "anchor", "Top"),
        ("com.system76.CosmicPanel.Panel", "size", "XS"),
        ("com.system76.CosmicPanel.Panel", "padding", "0"),
        (
            "com.system76.CosmicPanel.Panel",
            "plugins_center",
            r#"Some(["com.system76.CosmicAppletTime"])"#,
        ),
        (
            "com.system76.CosmicPanel.Panel",
            "plugins_wings",
            r#"Some((["com.system76.CosmicPanelWorkspacesButton", "com.system76.CosmicPanelAppButton"], ["com.system76.CosmicAppletAudio", "com.system76.CosmicAppletNetwork", "com.system76.CosmicAppletBattery", "com.system76.CosmicAppletPower"]))"#,
        ),
        ("com.system76.CosmicPanel.Dock", "anchor", "Bottom"),
        ("com.system76.CosmicPanel.Dock", "size", "S"),
        ("com.system76.CosmicPanel.Dock", "expand_to_edges", "false"),
        ("com.system76.CosmicPanel.Dock", "border_radius", "20"),
        (
            "com.system76.CosmicPanel.Dock",
            "plugins_center",
            r#"Some(["com.system76.CosmicPanelLauncherButton", "com.system76.CosmicPanelWorkspacesButton", "com.system76.CosmicPanelAppButton", "com.system76.CosmicAppList", "com.system76.CosmicAppletMinimize"])"#,
        ),
        ("com.system76.CosmicPanel.Dock", "plugins_wings", "None"),
    ] {
        let file = root
            .join("system/cosmic")
            .join(namespace)
            .join("v1")
            .join(key);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, value).unwrap();
    }
    let dir = root.join("config/cosmic/com.system76.CosmicBackground/v1");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("all"),
        ron::to_string(&BgEntry::new(
            "all",
            BgSource::Path(bundled_path(&bundled, "halcyon-calm")),
        ))
        .unwrap(),
    )
    .unwrap();
    let filter = std::env::var("UX_TOUR_ONLY").unwrap_or_default();
    let pages: Vec<(&str, Page, ObjectTab)> = vec![
        ("desktop", Page::Overview, ObjectTab::Content),
        ("arrange", Page::Overview, ObjectTab::Content),
        ("clock-content", Page::Clock, ObjectTab::Content),
        ("clock-design", Page::Clock, ObjectTab::Design),
        ("clock-position", Page::Clock, ObjectTab::Position),
        ("weather-content", Page::Weather, ObjectTab::Content),
        ("weather-design", Page::Weather, ObjectTab::Design),
        ("calendar-content", Page::Calendar, ObjectTab::Content),
        ("glance-content", Page::Glance, ObjectTab::Content),
        ("glance-design", Page::Glance, ObjectTab::Design),
        ("system-content", Page::Devbox, ObjectTab::Content),
        ("projects-content", Page::Projects, ObjectTab::Content),
        ("music-content", Page::Music, ObjectTab::Content),
        ("music-design", Page::Music, ObjectTab::Design),
        ("notes-content", Page::Notes, ObjectTab::Content),
        ("notes-design", Page::Notes, ObjectTab::Design),
        ("focus-content", Page::Focus, ObjectTab::Content),
        ("wallpaper", Page::DesktopWallpaper, ObjectTab::Content),
        ("dock", Page::DesktopDock, ObjectTab::Content),
        ("icons", Page::DesktopIcons, ObjectTab::Content),
        ("windows", Page::DesktopWindows, ObjectTab::Content),
        ("rules", Page::DesktopWindowRules, ObjectTab::Content),
        ("shortcuts", Page::DesktopShortcuts, ObjectTab::Content),
        ("builder", Page::Builder, ObjectTab::Content),
        ("colors", Page::Themes, ObjectTab::Content),
        ("surfaces", Page::Appearance, ObjectTab::Content),
        ("fonts", Page::DesktopAppearance, ObjectTab::Content),
        ("setups", Page::DesktopProfiles, ObjectTab::Content),
        ("schedules", Page::DesktopAutomation, ObjectTab::Content),
        ("moments", Page::Moments, ObjectTab::Content),
        ("adaptive", Page::AdaptiveLooks, ObjectTab::Content),
        ("settings", Page::General, ObjectTab::Content),
        ("accessibility", Page::DesktopMotion, ObjectTab::Content),
    ];
    let wanted = |name: &str| filter.is_empty() || filter.split(',').any(|f| name.contains(f));
    if wanted("wizard") {
        use crate::settings::wizard::{Message as Step, Step as Screen};
        for (size, label) in [
            (Size::new(820., 600.), "820"),
            (Size::new(1280., 800.), "1280"),
        ] {
            let (mut app, initial) = Settings::init(Core::default(), Page::Overview);
            settle_settings(&mut app, initial);
            let _ = app.update(Message::Resized(size));
            let task = app.update(Message::OpenWizard);
            settle_settings(&mut app, task);
            let task = app.update(Message::LookImagesReady);
            settle_settings(&mut app, task);
            let steps: Vec<(&str, Option<Step>)> = vec![
                ("wizard-welcome", None),
                ("wizard-start", Some(Step::Start)),
                ("wizard-look", Some(Step::Look("halcyon"))),
                ("wizard-bars-keep", Some(Step::Next)),
                ("wizard-bars-custom", Some(Step::KeepBars(false))),
                ("wizard-widgets", Some(Step::Next)),
                ("wizard-order", Some(Step::Show(Screen::Arrange))),
                ("wizard-review", Some(Step::Next)),
                ("wizard-leave", Some(Step::AskLeave(true))),
                ("wizard-apply-stay", Some(Step::AskLeave(false))),
                ("wizard-done", Some(Step::ApplyAll)),
            ];
            for (name, step) in steps {
                if let Some(step) = step {
                    let task = app.update(Message::Wizard(step));
                    settle_settings(&mut app, task);
                }
                let mut native = Native::new(size);
                assert_eq!(
                    native.duplicate_ids(app.settings_view()),
                    Vec::<String>::new(),
                    "{name} at {size:?}"
                );
                native.capture(
                    app.settings_view(),
                    &cosmic::Theme::dark(),
                    &format!("{name}-{label}-dark"),
                );
            }
        }
    }
    for (size, label) in [
        (Size::new(820., 600.), "820"),
        (Size::new(1280., 800.), "1280"),
        // The same physical window with text at 125%.
        (Size::new(656., 480.), "text125"),
    ] {
        let (mut app, initial) = Settings::init(Core::default(), Page::Overview);
        settle_settings(&mut app, initial);
        let _ = app.update(Message::Resized(size));
        // Reserve the titlebar and retain the widget/renderer cache through
        // navigation, including its scroll operations, as the native app does.
        let mut native = Native::new(Size::new(size.width, size.height - 48.));
        for (name, page, tab) in &pages {
            if !wanted(name) {
                continue;
            }
            native_send(&mut app, &mut native, Message::Page(*page));
            native_send(&mut app, &mut native, Message::ObjectTab(*tab));
            if *name == "arrange" {
                let task = app.update(Message::Canvas(canvas::Message::Start));
                settle_settings(&mut app, task);
            }
            let task = app.update(Message::LookImagesReady);
            settle_settings(&mut app, task);
            native.scroll(app.settings_view(), "settings-content", 0.);
            assert_eq!(
                native.duplicate_ids(app.settings_view()),
                Vec::<String>::new(),
                "{name} at {size:?}"
            );
            let themes: &[(&str, cosmic::Theme)] = if label == "820" {
                &[
                    ("dark", cosmic::Theme::dark()),
                    ("light", cosmic::Theme::light()),
                ]
            } else {
                &[("dark", cosmic::Theme::dark())]
            };
            for (mode, theme) in themes {
                // Keep first-frame evidence separately and capture another
                // frame before the presented image. This exposes transient
                // software renderer differences instead of overwriting them.
                native.capture(
                    app.settings_view(),
                    theme,
                    &format!("{name}-{label}-{mode}-first-frame"),
                );
                native.capture(
                    app.settings_view(),
                    theme,
                    &format!("{name}-{label}-{mode}-second-frame"),
                );
                native.capture(
                    app.settings_view(),
                    theme,
                    &format!("{name}-{label}-{mode}"),
                );
                // The header's one way up is a button named settings-back,
                // labelled with its destination after a Back arrow. The same
                // title can also be a sidebar entry, so find the label inside
                // the button.
                if let Some((title, _)) = app.up_link() {
                    let button = native.bounds(app.settings_view(), "settings-back");
                    let inside: Vec<_> = native
                        .text_all(app.settings_view(), &title)
                        .into_iter()
                        .filter(|text| {
                            text.x >= button.x
                                && text.y >= button.y
                                && text.x + text.width <= button.x + button.width + 0.5
                                && text.y + text.height <= button.y + button.height + 0.5
                        })
                        .collect();
                    assert_eq!(
                        inside.len(),
                        1,
                        "Back label {title} inside {button:?}: {name} {label} {mode}"
                    );
                    let bounds = inside[0];
                    assert!(
                        bounds.x >= 0.
                            && bounds.y >= 0.
                            && bounds.x + bounds.width <= native.size.width
                            && bounds.y + bounds.height <= native.size.height,
                        "Back label on screen: {name} {label} {mode}: {bounds:?}"
                    );
                    let path = std::path::PathBuf::from(
                        std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE").unwrap(),
                    )
                    .join(format!("{name}-{label}-{mode}.png"));
                    let pixels = image::open(path).unwrap().to_rgba8();
                    let base: cosmic::iced::Color =
                        theme.cosmic().background(theme.transparent).base.into();
                    let rgb = [base.r * 255., base.g * 255., base.b * 255.];
                    let mut ink = 0;
                    // The label's first 40 px must be drawn. A clipped start
                    // still passes bounds tests but leaves a partial word.
                    for y in bounds.y.max(0.).ceil() as u32
                        ..(bounds.y + bounds.height)
                            .min(pixels.height() as f32)
                            .floor() as u32
                    {
                        for x in bounds.x.max(0.).ceil() as u32
                            ..(bounds.x + 40.).min(pixels.width() as f32).floor() as u32
                        {
                            let pixel = pixels.get_pixel(x, y);
                            if (0..3).any(|c| (pixel[c] as f32 - rgb[c]).abs() > 45.) {
                                ink += 1;
                            }
                        }
                    }
                    assert!(
                        ink > 25,
                        "complete Back label: {name} {label} {mode}: {bounds:?}, {ink} visible pixels"
                    );
                }
            }
        }
    }
}

/// Every screen, tab and old route renders twice with no repeated widget names.
/// A repeated name loses its state and panics on the next frame.
#[test]
fn ux_native_every_route_and_tab_renders_without_repeated_names() {
    if child("ux_native_every_route_and_tab_renders_without_repeated_names") {
        return;
    }
    use crate::settings::navigation::ObjectTab;
    let (mut app, _) = Settings::init(Core::default(), Page::Overview);
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        let _ = app.update(Message::Resized(size));
        // The installed app retains its widget cache across navigation, too.
        let mut native = Native::new(size);
        for page in Page::ALL {
            let _ = app.update(Message::Page(page));
            assert_eq!(app.page, page.canonical());
            for tab in ObjectTab::ALL {
                let _ = app.update(Message::ObjectTab(tab));
                assert_eq!(
                    native.duplicate_ids(app.settings_view()),
                    Vec::<String>::new(),
                    "{page:?} {tab:?} {size:?}"
                );
                let _ = native.render(app.settings_view(), &cosmic::Theme::dark());
            }
            let _ = app.update(Message::Canvas(canvas::Message::Cancel));
        }
    }
}

#[test]
fn ux_native_nine_roots_and_every_old_route_resolves() {
    assert_eq!(
        Page::SIDEBAR[0].1,
        &[
            Page::DesktopProfiles,
            Page::Themes,
            Page::DesktopWallpaper,
            Page::WeatherEffects,
            Page::Overview,
            Page::DesktopDock,
            Page::DesktopWindows,
            Page::DesktopMotion,
            Page::General
        ]
    );
    for (page, root) in [
        (Page::Overview, Page::Overview),
        (Page::WidgetLayout, Page::Overview),
        (Page::Desktop, Page::Overview),
        (Page::Widgets, Page::Overview),
        (Page::Glance, Page::Overview),
        (Page::Clock, Page::Overview),
        (Page::Weather, Page::Overview),
        (Page::Calendar, Page::Overview),
        (Page::Devbox, Page::Overview),
        (Page::Projects, Page::Overview),
        (Page::Music, Page::Overview),
        (Page::Notes, Page::Overview),
        (Page::Focus, Page::Overview),
        (Page::Devices, Page::Overview),
        (Page::Builder, Page::Overview),
        (Page::DesktopWallpaper, Page::DesktopWallpaper),
        (Page::DesktopIcons, Page::DesktopWallpaper),
        (Page::WeatherEffects, Page::WeatherEffects),
        (Page::DesktopDock, Page::DesktopDock),
        (Page::DesktopWindows, Page::DesktopWindows),
        (Page::DesktopWorkspaces, Page::DesktopWindows),
        (Page::DesktopWindowRules, Page::DesktopWindows),
        (Page::DesktopShortcuts, Page::DesktopWindows),
        (Page::Themes, Page::Themes),
        (Page::Appearance, Page::Themes),
        (Page::DesktopAppearance, Page::Themes),
        (Page::Moments, Page::DesktopProfiles),
        (Page::DesktopProfiles, Page::DesktopProfiles),
        (Page::DesktopAutomation, Page::DesktopProfiles),
        (Page::AdaptiveLooks, Page::DesktopProfiles),
        (Page::General, Page::General),
        (Page::DesktopMotion, Page::DesktopMotion),
        (Page::PendingChanges, Page::Changes),
        (Page::Changes, Page::Changes),
    ] {
        assert_eq!(page.sidebar(), root, "{page:?}");
        assert_eq!(Page::parse(page.key()).sidebar(), root, "{page:?}");
    }
    assert_eq!(Page::ALL.len(), 34, "a new route must be mapped here too");
    for (alias, page) in [
        ("home", Page::Overview),
        ("layout", Page::Overview),
        ("setups", Page::DesktopProfiles),
        ("presets", Page::DesktopProfiles),
        ("look", Page::Themes),
        ("widget-style", Page::Appearance),
        ("system-monitor", Page::Devbox),
        ("daily-overview", Page::Glance),
        ("focus-timer", Page::Focus),
        ("app-settings", Page::General),
        ("schedules", Page::DesktopAutomation),
        ("desktop", Page::Overview),
        ("widgets", Page::Overview),
        // Weather effects lived on Wallpaper; their old names reach them.
        ("weather-effects", Page::WeatherEffects),
        ("effects", Page::WeatherEffects),
        ("wallpaper-effects", Page::WeatherEffects),
        ("rain", Page::WeatherEffects),
        ("wallpaper", Page::DesktopWallpaper),
        ("weather", Page::Weather),
    ] {
        assert_eq!(Page::parse(alias), page, "{alias}");
    }
}

#[test]
fn ux_native_back_restores_tab_query_display_and_flat_tabs_add_no_history() {
    if child("ux_native_back_restores_tab_query_display_and_flat_tabs_add_no_history") {
        return;
    }
    use crate::displays::{
        OutputEvent,
        test_support::{Wire, info},
    };
    use crate::settings::navigation::ObjectTab;
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::Overview);
    let wire = Wire::new();
    for (id, name, geometry) in [(1, "eDP-1", (1280, 720)), (2, "DP-2", (1920, 1080))] {
        let _ = app.update(Message::DisplayOutput(
            OutputEvent::Created(Some(info(Some(name), geometry))),
            wire.output(id),
        ));
    }
    let mut snapshot = crate::layout_state::read(&app.config);
    snapshot.monitors = [
        ("eDP-1".into(), [1280., 720.]),
        ("DP-2".into(), [1920., 1080.]),
    ]
    .into();
    snapshot.display = Some(crate::layout_state::DisplayState {
        output: Some("eDP-1".into()),
        ready: true,
        epoch: 1,
    });
    crate::layout_state::publish(&snapshot, crate::layout_state::next_generation());
    let _ = app.update(Message::Page(Page::Clock));
    let _ = app.update(Message::MonitorPreview("eDP-1".into()));
    assert_eq!(app.placement.display.as_deref(), Some("eDP-1"));
    assert_eq!(app.placement.screen(), [1280., 720.]);
    assert_eq!(app.back_title().as_deref(), Some("Widgets"));
    let _ = app.update(Message::ObjectTab(ObjectTab::Design));
    let _ = app.update(Message::ContentScrolled(120.));
    // A flat tab adds nothing to Back.
    let depth = app.history.len();
    let _ = app.update(Message::ObjectTab(ObjectTab::Content));
    let _ = app.update(Message::ObjectTab(ObjectTab::Design));
    assert_eq!(app.history.len(), depth);
    // The breadcrumb's parent and Back's destination can differ.
    let _ = app.update(Message::OpenWidgetFinish);
    assert_eq!(app.page, Page::Appearance);
    assert_eq!(app.material_target, crate::materials::Target::Widgets);
    assert_eq!(app.back_title().as_deref(), Some("Clock"));
    let _ = app.update(Message::MonitorPreview("DP-2".into()));
    assert_eq!(app.placement.display.as_deref(), Some("DP-2"));
    assert_eq!(app.placement.screen(), [1920., 1080.]);
    let _ = app.update(Message::FlatPage(Page::Themes));
    assert_eq!(app.back_title().as_deref(), Some("Clock"), "tabs are flat");
    let _ = app.update(Message::Back);
    assert_eq!(app.page, Page::Clock);
    assert_eq!(app.object_tab, ObjectTab::Design, "the tab comes back");
    assert_eq!(app.page_scroll.get(Page::Clock.key()), Some(&120.));
    assert_eq!(app.placement.display.as_deref(), Some("eDP-1"));
    assert_eq!(
        app.placement.screen(),
        [1280., 720.],
        "Back restores the viewed display"
    );
    // Search: the result lands on its control and Back returns to the results.
    let _ = app.update(Message::SearchSettings("spectrum".into()));
    let _ = app.update(Message::SearchFirst);
    assert_eq!(app.page, Page::Music);
    assert_eq!(app.object_tab, ObjectTab::Design);
    assert_eq!(app.search_query, "");
    assert_eq!(app.back_title().as_deref(), Some("Search results"));
    let _ = app.update(Message::Back);
    assert_eq!(app.search_query, "spectrum");
    assert_eq!(app.page, Page::Clock);
    let _ = app.update(Message::Back);
    assert_eq!(app.search_query, "");
    assert_eq!(app.page, Page::Clock);
    // A root always opens its landing.
    let _ = app.update(Message::AppSettingsPage(AppSettingsPage::Updates));
    let root = app
        .nav_model
        .iter()
        .find(|&id| app.nav_model.data::<Page>(id) == Some(&Page::General))
        .unwrap();
    let _ = app.update(Message::NavSelected(root));
    assert_eq!(app.app_settings_page, AppSettingsPage::Home);
    assert!(!app.page_scroll.contains_key(Page::General.key()));
    assert_eq!(
        std::fs::read(Config::path().unwrap()).unwrap(),
        before,
        "viewing another display and navigating never changes widget assignments"
    );
}

#[test]
fn ux_native_position_tab_hosts_the_canvas_and_keeps_its_draft() {
    if child("ux_native_position_tab_hosts_the_canvas_and_keeps_its_draft") {
        return;
    }
    use crate::settings::navigation::{KeptTarget, ObjectTab};
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::Clock);
    let _ = app.update(Message::Resized(Size::new(820., 600.)));
    let _ = app.update(Message::ObjectTab(ObjectTab::Position));
    assert!(app.canvas_visible());
    assert_eq!(app.placement.selected(), Some(Widget::Clock));
    let _ = app.update(Message::Placement(placement::Message::Nudge([1, 0], false)));
    assert!(app.canvas_dirty());
    let mut native = Native::new(Size::new(820., 600.));
    let _ = native.bounds(app.settings_view(), "canvas-apply");
    // Leaving keeps it and says so; nothing is written.
    let _ = app.update(Message::ObjectTab(ObjectTab::Content));
    assert!(!app.canvas_visible());
    assert!(app.canvas_dirty());
    assert!(
        app.kept_previews()
            .iter()
            .any(|i| i.target == KeptTarget::Canvas)
    );
    let _ = app.update(Message::Page(Page::General));
    assert!(app.canvas_dirty());
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
    let _ = app.update(Message::ResumeKept(KeptTarget::Canvas));
    assert!(app.canvas_visible());
    let _ = app.update(Message::Canvas(canvas::Message::Cancel));
    assert!(app.kept_previews().is_empty());
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}

#[test]
fn ux_native_undo_target_is_one_thing_for_button_and_keyboard() {
    if child("ux_native_undo_target_is_one_thing_for_button_and_keyboard") {
        return;
    }
    use crate::settings::navigation::UndoTarget;
    let (mut app, _) = Settings::init(Core::default(), Page::Clock);
    let before = app.config.clock.format_24h;
    let _ = app.update(Message::ClockFormat(!before));
    assert!(matches!(app.undo_target(), UndoTarget::Desktop(..)));
    // The note owns Undo on its screen, even with nothing to undo.
    let _ = app.update(Message::Page(Page::Notes));
    assert_eq!(app.undo_target(), UndoTarget::Note);
    let _ = app.update(Message::KeyboardUndo(false));
    assert_eq!(Config::load().clock.format_24h, !before);
    // A preview owns Undo on its screen; an empty history stops there.
    let _ = app.update(Message::Page(Page::WeatherEffects));
    let mut effect = app.config.rain.clone();
    effect.enabled = !effect.enabled;
    let _ = app.update(Message::Rain(rain::Edit::Set(effect.clone())));
    assert_eq!(
        app.undo_target(),
        UndoTarget::Preview(Some("wallpaper edit"))
    );
    let _ = app.update(Message::KeyboardUndo(false));
    assert_eq!(app.wallpaper.effect(), None);
    assert_eq!(Config::load().clock.format_24h, !before);
    // Exhausting local history cannot undo an unrelated desktop action.
    assert_eq!(app.undo_target(), UndoTarget::Preview(None));
    let _ = app.update(Message::KeyboardUndo(false));
    assert_eq!(Config::load().clock.format_24h, !before);
    // Closing the preview makes the desktop change the target again, but
    // like the footer's Undo, Ctrl+Z only undoes it on the page that made it.
    let _ = app.update(Message::Wallpaper(wallpaper::Message::Discard));
    assert!(matches!(app.undo_target(), UndoTarget::Desktop(..)));
    let _ = app.update(Message::KeyboardUndo(false));
    assert_eq!(Config::load().clock.format_24h, !before);
    let _ = app.update(Message::Page(Page::Clock));
    let _ = app.update(Message::KeyboardUndo(false));
    assert_eq!(Config::load().clock.format_24h, before);
}

#[test]
fn ux_native_setup_and_theme_previews_are_isolated_retained_and_conflict_safe() {
    if child("ux_native_setup_and_theme_previews_are_isolated_retained_and_conflict_safe") {
        return;
    }
    use crate::desktop::profiles::Domain;
    use crate::settings::navigation::KeptTarget;
    use crate::settings::profiles::{Message as Edit, Selection};
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::DesktopProfiles);
    // Looking at a setup leaves nothing behind.
    let _ = app.update(Message::Profiles(Edit::Select(Selection::Preset("glass"))));
    let _ = app.update(Message::Page(Page::General));
    assert!(app.kept_previews().is_empty());
    assert_eq!(app.profiles.selection(), &Selection::Current);
    // Changing one waits, listed, and does not leak into Colors.
    let _ = app.update(Message::Page(Page::DesktopProfiles));
    let _ = app.update(Message::Profiles(Edit::Select(Selection::Preset("glass"))));
    let _ = app.update(Message::Profiles(Edit::Include(Domain::Dock, false)));
    let _ = app.update(Message::Page(Page::Themes));
    assert_eq!(app.profiles.selection(), &Selection::Current, "isolated");
    assert!(app.kept_note.is_some(), "leaving says what it kept");
    assert!(
        app.kept_previews()
            .iter()
            .any(|i| matches!(&i.target, KeptTarget::Setup(_)))
    );
    // A theme is its own preview, with its own local steps.
    let _ = app.update(Message::Profiles(Edit::Select(Selection::Preset(
        "theme-soft",
    ))));
    let _ = app.update(Message::Profiles(Edit::Select(Selection::Preset(
        "theme-paper",
    ))));
    let _ = app.update(Message::KeyboardUndo(false));
    assert_eq!(app.profiles.selection(), &Selection::Preset("theme-soft"));
    let _ = app.update(Message::Page(Page::General));
    assert_eq!(app.kept_previews().len(), 2);
    // The sidebar says how many wait, in words, not only with a dot.
    for size in [
        cosmic::iced::Size::new(820., 600.),
        cosmic::iced::Size::new(1280., 800.),
    ] {
        let _ = app.update(Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        let changes = native.bounds(app.settings_view(), "nav-changes");
        let words = native.text_all(app.settings_view(), "2 waiting");
        if size.width > 1000. {
            assert!(
                words.iter().any(|w| changes.contains(w.center())),
                "{words:?} {changes:?}"
            );
        } else {
            assert!(words.is_empty(), "the narrow rail keeps its dot");
        }
        for (theme, mode) in [
            (cosmic::Theme::light(), "light"),
            (cosmic::Theme::dark(), "dark"),
        ] {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("changes-waiting-{}-{mode}", size.width),
            );
        }
    }
    let _ = app.update(Message::Page(Page::Themes));
    assert_eq!(app.profiles.selection(), &Selection::Preset("theme-soft"));
    let _ = app.update(Message::Profiles(Edit::Discard));
    // The setup comes back exactly, and nothing was ever written.
    let _ = app.update(Message::ResumeKept(KeptTarget::Setup(Selection::Preset(
        "glass",
    ))));
    assert_eq!(app.page, Page::DesktopProfiles);
    assert_eq!(app.profiles.selection(), &Selection::Preset("glass"));
    assert!(!app.profiles.included(Domain::Dock));
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
    // It survives a restart as an intention that needs review.
    let _ = app.update(Message::Page(Page::General));
    drop(app);
    let (mut app, _) = Settings::init(Core::default(), Page::General);
    assert!(
        app.kept_previews()
            .iter()
            .any(|i| matches!(&i.target, KeptTarget::Setup(_))),
        "kept across a restart: {}; {:?}",
        app.status,
        app.session_drafts.error
    );
    let _ = app.update(Message::ResumeKept(KeptTarget::Setup(Selection::Preset(
        "glass",
    ))));
    assert!(!app.profiles.included(Domain::Dock));
    assert!(app.profiles.stale(), "a restart always asks for review");
    assert!(!app.profiles.can_apply());
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}

#[test]
fn ux_three_screens_keep_primary_actions_reachable_and_state_local() {
    if child("ux_three_screens_keep_primary_actions_reachable_and_state_local") {
        return;
    }
    use crate::settings::navigation::ObjectTab;
    use crate::settings::tests::{native_click, send_settings, settle_settings};
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let (mut app, initial) = Settings::init(Core::default(), Page::Overview);
    settle_settings(&mut app, initial);
    for window in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(window));
        send_settings(&mut app, Message::Page(Page::Overview));
        let size = Size::new(window.width, window.height - 48.);
        let mut native = Native::new(size);
        // The Widgets landing is the picture with the widget list under it;
        // wallpaper, panel and dock are sidebar roots of their own.
        for id in ["canvas-start", "desktop-scene"] {
            let b = native.bounds(app.settings_view(), id);
            assert!(
                b.x >= app.sidebar_width()
                    && b.x + b.width <= size.width
                    && b.y >= 0.
                    && b.y + b.height <= size.height,
                "{id}: {b:?}"
            );
        }
        let scene = native.bounds(app.settings_view(), "desktop-scene");
        assert!(
            scene.width > (size.width - app.sidebar_width()) * 0.8,
            "the desktop picture gets the space: {scene:?}"
        );
        assert!(
            native
                .try_bounds(app.settings_view(), "desktop-widgets")
                .is_some(),
            "the widget list is part of the landing"
        );
        native_click(&mut app, &mut native, "widget-open-clock");
        assert_eq!(app.page, Page::Clock);
        native_click(&mut app, &mut native, "settings-back");
        assert_eq!(app.page, Page::Overview);
        send_settings(&mut app, Message::Page(Page::Clock));
        send_settings(&mut app, Message::ObjectTab(ObjectTab::Design));
        // One column of groups under the preview; the face choices are
        // reached through the page's own scrolling.
        let face = crate::settings::tests::native_reveal(&app, &mut native, "clock-face");
        assert!(
            face.y >= 0. && face.y + face.height <= size.height,
            "face choices visible: {face:?}"
        );
        send_settings(&mut app, Message::Page(Page::Appearance));
        // Glass is a detail page of Appearance; Back names it.
        let back = native.bounds(app.settings_view(), "settings-back");
        assert!(back.width > 70. && back.x >= app.sidebar_width());
        assert_eq!(app.up_link().map(|(title, _)| title).as_deref(), Some("Appearance"));
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("transition-surfaces-{}", window.width),
        );
        for id in [
            "material-none",
            "material-frosted",
            "material-clear",
            "material-smoked",
            "material-liquid",
        ] {
            let b = crate::settings::tests::native_reveal(&app, &mut native, id);
            assert!(
                b.width >= 150. && b.height >= 120.,
                "large finish samples: {id}: {b:?}"
            );
            assert!(
                b.y >= 0. && b.y + b.height <= size.height,
                "every choice can be shown: {id}: {b:?}"
            );
        }
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        let _ = native.render(app.settings_view(), &cosmic::Theme::dark());
        let _ = native.render(app.settings_view(), &cosmic::Theme::light());
    }
    assert_eq!(
        std::fs::read(Config::path().unwrap()).unwrap(),
        before,
        "navigation and samples never write preferences"
    );
    // A real tile click still commits exactly its finish, and the visible Undo
    // retains the guarded action boundary rather than changing another setting.
    send_settings(&mut app, Message::Resized(Size::new(820., 600.)));
    let mut native = Native::new(Size::new(820., 552.));
    native_click(&mut app, &mut native, "material-clear");
    assert_eq!(
        app.config.materials.shared.unwrap().kind,
        crate::materials::Kind::Clear
    );
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap();
    assert_eq!(saved["future_preference"], 37);
    native_click(&mut app, &mut native, "settings-undo");
    assert!(app.config.materials.shared.is_none());
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap();
    assert_eq!(saved["future_preference"], 37);
}
