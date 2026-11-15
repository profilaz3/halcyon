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

/// The bundled Ridge picture as every display's wallpaper, so the preview
/// has a picture without cosmic-bg's fallback under /usr/share, which a
/// machine without COSMIC (CI) does not have.
fn comfort_ridge_wallpaper(root: &std::path::Path) {
    let store = crate::desktop::actions::history_store().unwrap();
    let ridge = crate::desktop::wallpaper::bundled_path(
        &crate::desktop::wallpaper::bundled_dir(&store),
        "halcyon-ridge",
    );
    assert!(ridge.is_file());
    seed(
        &root.join("cosmic-root/config/cosmic"),
        &[(
            "com.system76.CosmicBackground",
            "all",
            &ron::to_string(&crate::desktop::wallpaper::BgEntry::new(
                "all",
                crate::desktop::wallpaper::BgSource::Path(ridge),
            ))
            .unwrap(),
        )],
    );
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

/// WCAG contrast between the commonest color in `area` of an RGBA capture
/// (the background) and the pixel that differs from it most (the ink).
fn comfort_ink_contrast(pixels: &[u8], width: f32, area: cosmic::iced::Rectangle) -> f32 {
    fn luminance([r, g, b]: [u8; 3]) -> f32 {
        let channel = |c: u8| {
            let c = c as f32 / 255.;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }
    let mut counts = std::collections::HashMap::<[u8; 3], usize>::new();
    let mut seen = Vec::new();
    for y in area.y.max(0.) as usize..(area.y + area.height) as usize {
        for x in area.x.max(0.) as usize..(area.x + area.width) as usize {
            let i = (y * width as usize + x) * 4;
            let rgb = [pixels[i], pixels[i + 1], pixels[i + 2]];
            *counts.entry(rgb).or_default() += 1;
            seen.push(rgb);
        }
    }
    let background = counts.into_iter().max_by_key(|(_, n)| *n).unwrap().0;
    let back = luminance(background);
    seen.into_iter()
        .map(|rgb| {
            let ink = luminance(rgb);
            (back.max(ink) + 0.05) / (back.min(ink) + 0.05)
        })
        .fold(1., f32::max)
}

/// Look Studio's tile captions read in a light theme, not only a dark one.
#[test]
fn studio_tile_captions_are_readable_in_light_and_dark() {
    use profiles::studio::{Edit as Studio, Start};
    let name = "studio_tile_captions_are_readable_in_light_and_dark";
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
    let mut app = looks_desktop(&root);
    let size = Size::new(820., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    looks_send(
        &mut app,
        profiles::Message::Select(profiles::Selection::Current),
    );
    studio_send(&mut app, Studio::Start(Start::Desktop));
    for part in ["wallpaper", "panel", "windows"] {
        let tile = native_reveal(&app, &mut native, &format!("studio-tile-{part}"));
        // The caption is the tile's last line, inside its 8 px padding.
        let caption = cosmic::iced::Rectangle {
            x: tile.x + 10.,
            y: tile.y + tile.height - 8. - 16.,
            width: tile.width - 20.,
            height: 16.,
        };
        for (theme, tone) in [
            (cosmic::Theme::light(), "light"),
            (cosmic::Theme::dark(), "dark"),
        ] {
            let pixels = native.render(app.settings_view(), &theme);
            let contrast = comfort_ink_contrast(&pixels, size.width, caption);
            assert!(
                contrast >= 4.5,
                "{part} caption in {tone}: {contrast:.2}:1 at {caption:?}"
            );
        }
    }
}

/// The Looks preview's sample windows stay clear of each look's widgets,
/// the big clock of Ridge and Rainy Lake most of all.
#[test]
fn looks_sample_windows_stay_clear_of_the_widgets() {
    let name = "looks_sample_windows_stay_clear_of_the_widgets";
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
    let mut app = looks_desktop(&root);
    for size in [Size::new(1280., 800.), Size::new(820., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        for look in [
            "ridge",
            "rainy-lake",
            "halcyon",
            "tidal",
            "carbon",
            "harbor",
        ] {
            looks_select(&mut app, look);
            native.scroll(app.settings_view(), "settings-content", 0.);
            let Some(windows) = native.try_bounds(app.settings_view(), "looks-sample-windows")
            else {
                continue;
            };
            assert!(
                windows.width > 20. && windows.height > 20.,
                "{look}: {windows:?}"
            );
            let scene = native.bounds(app.settings_view(), "profile-scene-candidate");
            assert!(
                windows.x >= scene.x - 0.5
                    && windows.x + windows.width <= scene.x + scene.width + 0.5,
                "{look}: windows stay on the picture"
            );
            for widget in [
                "clock", "weather", "calendar", "devbox", "glance", "notes", "music",
            ] {
                if let Some(card) =
                    native.try_bounds(app.settings_view(), &format!("scene-card-{widget}"))
                {
                    assert!(
                        windows
                            .intersection(&card)
                            .is_none_or(|area| area.width * area.height < 1.),
                        "{look} at {}: the windows cover the {widget}: {windows:?} {card:?}",
                        size.width
                    );
                }
            }
        }
    }
}

// S10: the window can be as narrow as `MIN_WIDTH`, and every page fits it.
#[test]
fn every_page_fits_the_narrowest_window() {
    let name = "every_page_fits_the_narrowest_window";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            ..Default::default()
        },
    ) {
        return;
    }
    assert_eq!(MIN_WIDTH, 640.);
    let size = Size::new(MIN_WIDTH, 480.);
    let (mut app, task) = Settings::init(Core::default(), Page::Overview);
    settle_settings(&mut app, task);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    let mut overflow = Vec::new();
    for page in Page::ALL
        .into_iter()
        .filter(|page| *page != Page::WidgetLayout)
    {
        send_settings(&mut app, Message::Page(page));
        for (text, bounds) in native.overflowing_text(app.settings_view()) {
            overflow.push(format!("{page:?}: {text:?} at {bounds:?}"));
        }
    }
    assert!(
        overflow.is_empty(),
        "runs past a {MIN_WIDTH} px window:\n{}",
        overflow.join("\n")
    );
}

// S11: the Wallpaper preview takes the selected display's shape, and the
// clock sits where the desktop puts it, drawn by the shared card renderer.
#[test]
fn wallpaper_preview_takes_the_display_shape_with_the_clock_in_place() {
    let name = "wallpaper_preview_takes_the_display_shape_with_the_clock_in_place";
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
    let mut app = looks_desktop(&root);
    comfort_ridge_wallpaper(&root);
    comfort_square_display(&mut app);
    let size = Size::new(1024., 760.);
    send_settings(&mut app, Message::Resized(size));
    send_settings(&mut app, Message::Page(Page::DesktopWallpaper));
    assert_eq!(app.wallpaper_screen(), [1280., 1024.]);
    let mut native = crate::native_test::Native::new(size);
    native.scroll(app.settings_view(), "settings-content", 0.);
    let preview = native.bounds(app.settings_view(), "wallpaper-preview");
    let ratio = preview.height / preview.width;
    assert!(
        (ratio - 1024. / 1280.).abs() < 0.01,
        "a 5:4 display, a 5:4 preview: {preview:?}"
    );
    let clock = native.bounds(app.settings_view(), "wallpaper-preview-clock");
    assert!(
        preview.contains(clock.position())
            && clock.x + clock.width <= preview.x + preview.width + 0.5
            && clock.y + clock.height <= preview.y + preview.height + 0.5,
        "the clock stays on the picture: {clock:?} in {preview:?}"
    );
    let layout =
        view::profiles::scene_geometry(app.placement.preview_snapshot().clone(), &app.config);
    let geometry = layout
        .widgets
        .iter()
        .find(|geometry| geometry.key == "clock")
        .unwrap();
    let (full, origin) = layout
        .output
        .as_ref()
        .map_or((layout.screen, [0., 0.]), |frame| {
            (frame.size, frame.origin)
        });
    let scale = preview.width / full[0];
    let expected = [
        preview.x + (origin[0] + geometry.position[0]) * scale,
        preview.y + (origin[1] + geometry.position[1]) * scale,
    ];
    assert!(
        (clock.x - expected[0]).abs() < 1.5 && (clock.y - expected[1]).abs() < 1.5,
        "the clock at its desktop position: {clock:?}, expected {expected:?}"
    );
    assert!(
        (clock.width - geometry.size[0] * scale).abs() < 1.5,
        "the clock at its desktop size: {clock:?}"
    );
}
