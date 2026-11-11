//! First-use and Themes journeys rendered by the production Settings view in
//! the headless native renderer. Every run uses a private fixture: the user's
//! preferences, desktop and accounts are never read or written.
use super::*;
use crate::native_test::Native;

/// Runs `name` again in a child process with private XDG, config and desktop
/// roots, like the other native Settings regressions.
fn child(name: &str, config: &str) -> bool {
    if std::env::var("DESKTOP_CARDS_UX_TEST").as_deref() == Ok(name) {
        return false;
    }
    let root = std::env::temp_dir().join(format!("cards-journey-{}-{name}", std::process::id()));
    for dir in ["config", "data", "cache", "state", "desktop"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join("dconf-profile"), "").unwrap();
    std::fs::write(root.join("config.json"), config).unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &format!("settings::theme_journey_tests::{name}")])
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

fn desktop_root() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap())
}

/// Writes one COSMIC user value inside the private fixture root.
fn cosmic(namespace: &str, version: u64, key: &str, value: &str) {
    let dir = desktop_root()
        .join("config/cosmic")
        .join(namespace)
        .join(format!("v{version}"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(key), value).unwrap();
}

fn wallpaper(path: &std::path::Path) {
    use crate::desktop::wallpaper as backgrounds;
    cosmic(
        backgrounds::NAMESPACE,
        1,
        "all",
        &ron::to_string(&backgrounds::BgEntry::new(
            "all",
            backgrounds::BgSource::Path(path.to_path_buf()),
        ))
        .unwrap(),
    );
    cosmic(backgrounds::NAMESPACE, 1, "same-on-all", "true");
}

/// The state the user reported: a dark accent from an exact wallpaper swatch
/// on both COSMIC variants, and an older custom active-window outline.
fn reported_cosmic_state(dark: bool) {
    for variant in ["Dark", "Light"] {
        cosmic(
            &format!("com.system76.CosmicTheme.{variant}.Builder"),
            2,
            "accent",
            "Some(\"#40352CFF\")",
        );
    }
    cosmic(
        "com.system76.CosmicTheme.Dark.Builder",
        2,
        "window_hint",
        "Some(\"#63D0DFFF\")",
    );
    cosmic(
        "com.system76.CosmicTheme.Mode",
        1,
        "is_dark",
        &dark.to_string(),
    );
}

/// Background work runs synchronously here, as the app's task runner would.
fn drive(app: &mut Settings) {
    for _ in 0..32 {
        let wallpaper = app.wallpaper.take_jobs();
        let backdrops = app.profiles.take_jobs();
        let themes = app.profiles.take_theme_jobs();
        if wallpaper.is_empty() && backdrops.is_empty() && themes.is_empty() {
            return;
        }
        for job in wallpaper {
            app.wallpaper.update(job.run());
        }
        for job in themes {
            app.profiles.update(job.run());
        }
        for job in backdrops {
            app.profiles.update(job.run());
        }
    }
}

/// Navigates like the sidebar, then reopens the editors whose background
/// work `update` turned into tasks this fixture never launches.
fn goto(app: &mut Settings, page: Page) {
    let _ = app.update(Message::Page(page));
    app.profiles = profiles::ProfilesEditor::default();
    app.profiles.open();
    app.profiles.open_home();
    if matches!(page, Page::DesktopWallpaper | Page::Appearance) {
        app.wallpaper = wallpaper::WallpaperEditor::default();
        app.wallpaper.open();
    }
    drive(app);
}

fn themes(app: &mut Settings, message: profiles::Message) {
    let _ = app.profiles.update(message);
    drive(app);
}

fn click(app: &mut Settings, native: &mut Native, id: &str) {
    use cosmic::iced::{Event, mouse};
    let bounds = super::tests::native_reveal(app, native, id);
    let mut emitted = false;
    for event in [
        mouse::Event::CursorMoved {
            position: bounds.center(),
        },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        for message in native
            .event(app.settings_view(), Event::Mouse(event), bounds.center())
            .0
        {
            emitted = true;
            let _ = app.update(message);
        }
    }
    assert!(emitted, "{id} emits a native input message");
    drive(app);
}

fn prefix() -> String {
    std::env::var("DESKTOP_CARDS_JOURNEY_PREFIX").unwrap_or_else(|_| "journey".into())
}

/// The page top, then each further screenful the viewport can show.
fn capture_page(app: &Settings, size: Size, theme: &cosmic::Theme, name: &str, screens: usize) {
    let mut native = Native::new(size);
    let _ = native.render(app.settings_view(), theme);
    for screen in 0..screens {
        native.scroll(
            app.settings_view(),
            "settings-content",
            screen as f32 * (size.height - 180.),
        );
        native.capture(
            app.settings_view(),
            theme,
            &format!("{}-{name}-{}-{screen}", prefix(), size.width),
        );
    }
}

const FIRST_RUN: &str = r#"{"enabled_widgets":["clock","calendar","weather","devbox","projects"],"location":{"name":"Sample city","latitude":45.0,"longitude":16.0}}"#;
const REPORTED: &str = r#"{"setup_dismissed":true,"theme":"amber","enabled_widgets":["clock","calendar","weather","devbox","projects"],"location":{"name":"Sample city","latitude":45.0,"longitude":16.0},"appearance":{"opacity":92,"corner_radius":24,"border_opacity":20,"color_source":"custom","accent":[64,53,44],"sync_colors":true,"wallpaper_colors":true}}"#;

fn amber() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wallpapers/amber.jpg")
}

#[test]
fn journey_first_run_setup_and_themes() {
    if child("journey_first_run_setup_and_themes", FIRST_RUN) {
        return;
    }
    wallpaper(&amber());
    reported_cosmic_state(true);
    for size in [Size::new(1024., 720.), Size::new(820., 600.)] {
        let (mut app, _) = Settings::init(Core::default(), Page::Overview);
        let _ = app.update(Message::Resized(size));
        goto(&mut app, Page::Overview);
        drive(&mut app);
        capture_page(&app, size, &cosmic::Theme::dark(), "setup", 2);
        goto(&mut app, Page::Themes);
        capture_page(&app, size, &cosmic::Theme::dark(), "themes-first", 3);
    }
}

#[test]
fn journey_reported_wallpaper_colors_and_outline() {
    if child("journey_reported_wallpaper_colors_and_outline", REPORTED) {
        return;
    }
    wallpaper(&amber());
    reported_cosmic_state(true);
    for size in [Size::new(1024., 720.), Size::new(820., 600.)] {
        let (mut app, _) = Settings::init(Core::default(), Page::Themes);
        let _ = app.update(Message::Resized(size));
        goto(&mut app, Page::Themes);
        capture_page(&app, size, &cosmic::Theme::dark(), "themes-reported", 3);
        themes(
            &mut app,
            profiles::Message::Select(profiles::Selection::Preset("theme-soft")),
        );
        capture_page(&app, size, &cosmic::Theme::dark(), "themes-soft", 3);
        // The preview is always shown; each choice remains reachable by
        // scrolling, and the styles themselves are one click away.
        // libcosmic reserves a 48 px native window header outside settings_view.
        let mut native = Native::new(Size::new(size.width, size.height - 48.));
        for id in [
            "appearance-all-styles",
            "preset-theme-soft",
            "theme-color-wallpaper",
            "theme-color-custom",
        ] {
            let bounds = super::tests::native_reveal(&app, &mut native, id);
            assert!(
                bounds.y >= 0. && bounds.y + bounds.height <= native.size.height,
                "{id} is visible at {size:?}: {bounds:?}"
            );
        }
        // The styles are tiles on the page itself (the separate style browser
        // and its matching-wallpaper option are gone); Show all reveals every
        // style and keeps the previewed look selected.
        click(&mut app, &mut native, "appearance-all-styles");
        assert!(app.open_sections.contains("appearance-all-styles"));
        for id in ["preset-theme-soft", "preset-theme-outline"] {
            let bounds = super::tests::native_reveal(&app, &mut native, id);
            assert!(
                bounds.y >= 0. && bounds.y + bounds.height <= native.size.height,
                "{id} is visible with every style shown at {size:?}: {bounds:?}"
            );
        }
        capture_page(&app, size, &cosmic::Theme::dark(), "themes-styles", 2);
        click(&mut app, &mut native, "appearance-all-styles");
        assert!(!app.open_sections.contains("appearance-all-styles"));
        let style = |pointer: &str| {
            app.profiles
                .profile()
                .and_then(|p| p.widget_style.as_ref())
                .and_then(|s| s.pointer(pointer))
                .cloned()
        };
        assert_eq!(
            style("/appearance/wallpaper_colors"),
            Some(true.into()),
            "a wallpaper-colors user keeps that choice"
        );
        let accent: [u8; 3] = serde_json::from_value(style("/appearance/accent").unwrap()).unwrap();
        let tone = crate::desktop_theme::tone::lch(accent);
        assert!(
            tone.l > 0.7 && tone.c > 0.08,
            "not the dark swatch: {accent:?}"
        );
        themes(
            &mut app,
            profiles::Message::Select(profiles::Selection::Preset("theme-paper")),
        );
        capture_page(&app, size, &cosmic::Theme::dark(), "themes-paper", 2);
        goto(&mut app, Page::DesktopWallpaper);
        capture_page(&app, size, &cosmic::Theme::dark(), "wallpaper-colors", 2);
        goto(&mut app, Page::Appearance);
        capture_page(&app, size, &cosmic::Theme::dark(), "widget-style", 1);
    }
}

fn system_wallpaper(file: &str) -> Option<std::path::PathBuf> {
    let path = std::path::Path::new("/usr/share/backgrounds/cosmic").join(file);
    path.is_file().then_some(path)
}

#[test]
fn journey_apply_customize_and_follow_other_wallpapers() {
    if child(
        "journey_apply_customize_and_follow_other_wallpapers",
        REPORTED,
    ) {
        return;
    }
    wallpaper(&amber());
    reported_cosmic_state(true);
    let size = Size::new(1024., 720.);
    let (mut app, _) = Settings::init(Core::default(), Page::Themes);
    let _ = app.update(Message::Resized(size));
    goto(&mut app, Page::Themes);
    // The reported desktop has no theme yet: Wallpaper & colors offers the
    // explicit outline link next to its colors.
    goto(&mut app, Page::DesktopWallpaper);
    capture_page(&app, size, &cosmic::Theme::dark(), "wallpaper-outline", 1);
    goto(&mut app, Page::Themes);
    themes(
        &mut app,
        profiles::Message::Select(profiles::Selection::Preset("theme-soft")),
    );
    let _ = app.update(Message::Profiles(profiles::Message::Apply));
    assert_eq!(
        app.config.appearance.family,
        Some(crate::desktop_theme::Family::Soft)
    );
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    let native = crate::desktop::appearance::AppearanceDraft::load(&store).unwrap();
    for variant in crate::desktop::appearance::ThemeVariant::ALL {
        assert_eq!(
            native
                .color(variant, crate::desktop::appearance::ColorRole::Outline)
                .known(),
            Some(&None),
            "the older outline color now follows the accent"
        );
    }
    goto(&mut app, Page::Themes);
    capture_page(&app, size, &cosmic::Theme::dark(), "themes-applied", 3);
    themes(
        &mut app,
        profiles::Message::ThemeCustom(Some("#5E81AC".into())),
    );
    capture_page(&app, size, &cosmic::Theme::dark(), "themes-custom", 1);
    themes(&mut app, profiles::Message::Discard);
    goto(&mut app, Page::DesktopWallpaper);
    capture_page(&app, size, &cosmic::Theme::dark(), "wallpaper-themed", 1);
    if let Some(orion) = system_wallpaper("orion_nebula_nasa_heic0601a.jpg") {
        wallpaper(&orion);
        goto(&mut app, Page::Themes);
        themes(
            &mut app,
            profiles::Message::Select(profiles::Selection::Preset("theme-outline")),
        );
        capture_page(&app, size, &cosmic::Theme::dark(), "themes-orion", 1);
    }
    if let Some(light) = system_wallpaper("COSMIC-Pattern-Light.png") {
        wallpaper(&light);
        cosmic("com.system76.CosmicTheme.Mode", 1, "is_dark", "false");
        goto(&mut app, Page::Themes);
        themes(
            &mut app,
            profiles::Message::Select(profiles::Selection::Preset("theme-paper")),
        );
        capture_page(&app, size, &cosmic::Theme::light(), "themes-light-mode", 1);
        let small = Size::new(820., 600.);
        let _ = app.update(Message::Resized(small));
        capture_page(&app, small, &cosmic::Theme::light(), "themes-light-mode", 1);
    }
}

#[test]
fn material_native_chooser_adjust_reset_and_undo() {
    if child("material_native_chooser_adjust_reset_and_undo", REPORTED) {
        return;
    }
    use crate::materials::{Kind, Target};
    use cosmic::iced::{Event, mouse};
    wallpaper(&amber());
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    crate::desktop::islands::fixture_installed(store.fixture_root().unwrap());
    let click = |native: &mut Native, app: &mut Settings, id: &str| {
        let b = super::tests::native_reveal(app, native, id);
        assert!(
            b.y >= 0. && b.y + b.height <= native.size.height,
            "{id} {b:?}"
        );
        let point = b.center();
        let _ = native.event(
            app.settings_view(),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            point,
        );
        let (messages, _) = native.event(
            app.settings_view(),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            point,
        );
        assert!(!messages.is_empty(), "{id} emits an input message");
        for message in messages {
            let _ = app.update(message);
        }
        assert!(!app.error, "{}", app.status);
    };
    let original = Config::load();
    for size in [Size::new(820., 600.), Size::new(1280., 900.)] {
        let (mut app, _) = Settings::init(Core::default(), Page::Appearance);
        let _ = app.update(Message::InstallationReady(
            app.installation_generation,
            crate::desktop::islands::Installation::discover(&store),
        ));
        let _ = app.update(Message::Resized(size));
        goto(&mut app, Page::Appearance);
        // libcosmic reserves a 48 px native window header outside settings_view.
        let mut native = Native::new(Size::new(size.width, size.height - 48.));
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("materials-before-{}", size.width),
        );
        // Compare the four finish specimens directly, without a second scene.
        assert!(
            native
                .try_bounds(app.settings_view(), "scene-toggle")
                .is_none()
        );
        for kind in Kind::ALL {
            click(&mut native, &mut app, &format!("material-{}", kind.key()));
            assert_eq!(Config::load().materials.shared.unwrap().kind, kind);
            native.capture(
                app.settings_view(),
                &cosmic::Theme::dark(),
                &format!("materials-{}-{}", kind.key(), size.width),
            );
        }
        assert_eq!(Config::load().appearance, original.appearance);
        assert_eq!(Config::load().glance, original.glance);
        // Fine-tune shows under the chooser once a finish is chosen. Reveal the
        // real row through both scroll areas and compare against the viewport.
        let viewport = native.bounds(app.settings_view(), "settings-viewport");
        let motion = super::tests::native_reveal(&app, &mut native, "material-motion");
        assert!(
            motion.y >= viewport.y && motion.y + motion.height <= viewport.y + viewport.height,
            "Liquid widget motion fits above the footer: {motion:?}"
        );
        let readability = super::tests::native_reveal(&app, &mut native, "material-readability");
        assert!(
            readability.y + readability.height <= viewport.y + viewport.height,
            "Adjust fits above the footer: {readability:?}"
        );
        let b = super::tests::native_reveal(&app, &mut native, "material-transparency");
        let p = cosmic::iced::Point::new(b.x + b.width * 0.4, b.center_y());
        let (messages, _) = native.event(
            app.settings_view(),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            p,
        );
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::MaterialValue(true, _)))
        );
        for m in messages {
            let _ = app.update(m);
        }
        let (messages, _) = native.event(
            app.settings_view(),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            p,
        );
        assert!(messages.iter().any(|m| matches!(m, Message::MaterialSave)));
        for m in messages {
            let _ = app.update(m);
        }
        assert!(Config::load().materials.shared.unwrap().opacity.is_some());
        let before_key = Config::load().materials.shared.unwrap();
        use cosmic::iced::keyboard::{self, key};
        let event = |pressed| {
            Event::Keyboard(if pressed {
                keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(key::Named::ArrowUp),
                    modified_key: keyboard::Key::Named(key::Named::ArrowUp),
                    physical_key: key::Physical::Code(key::Code::ArrowUp),
                    location: keyboard::Location::Standard,
                    modifiers: keyboard::Modifiers::empty(),
                    text: None,
                    repeat: false,
                }
            } else {
                keyboard::Event::KeyReleased {
                    modified_key: keyboard::Key::Named(key::Named::ArrowUp),
                    key: keyboard::Key::Named(key::Named::ArrowUp),
                    physical_key: key::Physical::Code(key::Code::ArrowUp),
                    location: keyboard::Location::Standard,
                    modifiers: keyboard::Modifiers::empty(),
                }
            })
        };
        for m in native.event(app.settings_view(), event(true), p).0 {
            let _ = app.update(m);
        }
        assert_eq!(
            Config::load().materials.shared.unwrap(),
            before_key,
            "held key does not commit"
        );
        for m in native.event(app.settings_view(), event(false), p).0 {
            let _ = app.update(m);
        }
        assert_ne!(
            Config::load().materials.shared.unwrap(),
            before_key,
            "keyboard release commits"
        );
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("materials-adjust-{}", size.width),
        );
        click(&mut native, &mut app, "material-reset");
        assert_eq!(Config::load().materials.shared, None);
        let _ = app.update(Message::UndoPreferences);
        assert!(Config::load().materials.shared.unwrap().opacity.is_some());
        // Real scope tabs, real chooser; a new shared choice keeps the Dock override.
        super::tests::native_reveal(&app, &mut native, "material-scope");
        let bounds = native.text_bounds(app.settings_view(), "Dock only");
        let p = bounds.center();
        let _ = native.event(
            app.settings_view(),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            p,
        );
        let (messages, _) = native.event(
            app.settings_view(),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            p,
        );
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::MaterialTarget(Target::Dock)))
        );
        for m in messages {
            let _ = app.update(m);
        }
        click(&mut native, &mut app, "material-smoked");
        assert_eq!(Config::load().materials.dock.unwrap().kind, Kind::Smoked);
        click(&mut native, &mut app, "material-reset");
        assert!(Config::load().materials.dock.is_none());
    }
}
