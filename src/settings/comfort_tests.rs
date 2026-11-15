// Included inside settings::tests. Native captures of the comfort fixes:
// Look Studio captions in light mode, the Looks sample windows beside the
// clock, widget design recovery, narrow windows and the Wallpaper preview's
// display shape. Captures are written only while
// `DESKTOP_CARDS_LAYOUT_EVIDENCE` names a folder.

fn comfort_tones(native: &mut crate::native_test::Native, app: &Settings, name: &str) {
    for (theme, tone) in [
        (cosmic::Theme::dark(), "dark"),
        (cosmic::Theme::light(), "light"),
    ] {
        native.capture(app.settings_view(), &theme, &format!("{name}-{tone}"));
    }
}

/// A 5:4 display beside the default layout, as the only connected output.
fn comfort_square_display(app: &mut Settings) {
    use crate::displays::{
        OutputEvent,
        test_support::{Wire, info},
    };
    let wire = Wire::new();
    let _ = app.update(Message::DisplayOutput(
        OutputEvent::Created(Some(info(Some("DP-1"), (1280, 1024)))),
        wire.output(1),
    ));
    let mut snapshot = crate::layout_state::read(&app.config);
    snapshot.monitors = [("DP-1".into(), [1280., 1024.])].into();
    snapshot.display = Some(crate::layout_state::DisplayState {
        output: Some("DP-1".into()),
        ready: true,
        epoch: 1,
    });
    crate::layout_state::publish(&snapshot, crate::layout_state::next_generation());
    let _ = app.update(Message::MonitorPreview("DP-1".into()));
}

#[test]
fn comfort_captures_looks_studio_and_wallpaper() {
    use profiles::studio::{Edit as Studio, Start};
    let name = "comfort_captures_looks_studio_and_wallpaper";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            fixture: true,
            ..Default::default()
        },
    ) {
        return;
    }
    if std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE").is_none() {
        return;
    }
    let root = isolated_root();
    let mut app = looks_desktop(&root);
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    for look in ["ridge", "rainy-lake"] {
        looks_select(&mut app, look);
        native.scroll(app.settings_view(), "settings-content", 0.);
        comfort_tones(&mut native, &app, &format!("s8-{look}"));
    }
    let size = Size::new(820., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    looks_send(
        &mut app,
        profiles::Message::Select(profiles::Selection::Current),
    );
    studio_send(&mut app, Studio::Start(Start::Desktop));
    native.scroll(app.settings_view(), "settings-content", 380.);
    comfort_tones(&mut native, &app, "s7-studio-820");
    studio_send(&mut app, Studio::Close);

    comfort_square_display(&mut app);
    let size = Size::new(1024., 760.);
    send_settings(&mut app, Message::Resized(size));
    send_settings(&mut app, Message::Page(Page::DesktopWallpaper));
    let mut native = crate::native_test::Native::new(size);
    native.scroll(app.settings_view(), "settings-content", 0.);
    comfort_tones(&mut native, &app, "s11-wallpaper-5x4");
}

#[test]
fn comfort_captures_recovery_and_narrow_pages() {
    let name = "comfort_captures_recovery_and_narrow_pages";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            ..Default::default()
        },
    ) {
        return;
    }
    if std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE").is_none() {
        return;
    }
    let size = Size::new(1024., 720.);
    let (mut app, task) = Settings::init(Core::default(), Page::Overview);
    settle_settings(&mut app, task);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    app.config.design_problem = Some("The widget design file could not be read".into());
    comfort_tones(&mut native, &app, "s9-design-problem");
    app.config.design_problem = None;

    let size = Size::new(640., 600.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    for page in [
        Page::Overview,
        Page::Appearance,
        Page::Weather,
        Page::Calendar,
        Page::DesktopProfiles,
        Page::DesktopWallpaper,
        Page::DesktopAppearance,
        Page::DesktopDock,
        Page::DesktopPanel,
    ] {
        send_settings(&mut app, Message::Page(page));
        native.scroll(app.settings_view(), "settings-content", 0.);
        native.capture(
            app.settings_view(),
            &cosmic::Theme::light(),
            &format!("s10-640-{}", page.key()),
        );
    }
}

/// Separate light and dark wallpapers: the page offers this mode or both,
/// this mode by default, and Apply keeps the other mode's wallpaper.
#[test]
fn wallpaper_offers_this_mode_or_both_with_separate_light_and_dark() {
    use crate::desktop::wallpaper::modes::{DARK, LIGHT};
    let name = "wallpaper_offers_this_mode_or_both_with_separate_light_and_dark";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            fixture: true,
            ..Default::default()
        },
    ) {
        return;
    }
    let root = isolated_root();
    let entry = |picture: &str| {
        format!(
            r#"(output: "all", source: Path("{picture}"), filter_by_theme: false, rotation_frequency: 300, filter_method: Lanczos, scaling_mode: Zoom, sampling_method: Alphanumeric)"#
        )
    };
    let pictures = root.join("pictures");
    std::fs::create_dir_all(&pictures).unwrap();
    let picture = |name: &str| {
        let path = pictures.join(name);
        image::RgbImage::from_pixel(32, 18, image::Rgb([40, 60, 90]))
            .save(&path)
            .unwrap();
        path.display().to_string()
    };
    let (dark, light, new) = (
        picture("dark.png"),
        picture("light.png"),
        picture("new.png"),
    );
    let mut app = open_isolated_editor(&root);
    seed(
        &root.join("cosmic-root/config/cosmic"),
        &[(DARK, "all", &entry(&dark)), (LIGHT, "all", &entry(&light))],
    );
    send_settings(&mut app, Message::Page(Page::DesktopWallpaper));
    let draft = app.wallpaper.draft().unwrap();
    assert!(draft.separate_modes());
    assert_eq!(draft.mode_only(), Some(true));
    let size = Size::new(1024., 760.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    assert!(
        native
            .text_bounds(app.settings_view(), "Dark mode only")
            .width
            > 0.
    );
    assert!(native.text_bounds(app.settings_view(), "Both modes").width > 0.);
    if std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE").is_some() {
        native_reveal(&app, &mut native, "wallpaper-preview");
        comfort_tones(&mut native, &app, "s6-wallpaper-modes");
    }
    let store = app.wallpaper.store().unwrap().clone();
    let stored = |namespace: &str| {
        let key = crate::desktop::wallpaper::modes::rehome(
            &crate::desktop::wallpaper::shared_key(),
            namespace,
        );
        store.read_user(&key).unwrap().unwrap().as_str().to_string()
    };
    app.wallpaper.core.edit(|draft| {
        draft.set_source(
            &crate::desktop::wallpaper::Target::Shared,
            crate::desktop::wallpaper::BgSource::Path(new.clone().into()),
        )
    });
    send_settings(&mut app, Message::Wallpaper(wallpaper::Message::Apply));
    assert!(stored(DARK).contains(&new), "{}", stored(DARK));
    assert!(stored(LIGHT).contains(&light), "the light wallpaper stays");
    send_settings(&mut app, Message::Wallpaper(wallpaper::Message::Undo));
    assert!(stored(DARK).contains(&dark));
    // Both modes, when chosen.
    send_settings(
        &mut app,
        Message::Wallpaper(wallpaper::Message::BothModes(true)),
    );
    assert_eq!(app.wallpaper.draft().unwrap().mode_only(), None);
    app.wallpaper.core.edit(|draft| {
        draft.set_source(
            &crate::desktop::wallpaper::Target::Shared,
            crate::desktop::wallpaper::BgSource::Path(new.clone().into()),
        )
    });
    send_settings(&mut app, Message::Wallpaper(wallpaper::Message::Apply));
    assert!(stored(DARK).contains(&new));
    assert!(stored(LIGHT).contains(&new));
}
