// The Clock page's stage, driven through the real native widget tree.

/// Dragging the clock on the stage moves the desktop clock as one Undo step,
/// keeps it on the screen, and draws the lit clock where it lands.
#[test]
fn clock_stage_drag_saves_one_undo_step_and_relights() {
    if desktop_test_child(
        "clock_stage_drag_saves_one_undo_step_and_relights",
        DesktopEnv {
            app_config: true,
            fixture: true,
            icons: Icons::Shown,
            ..Default::default()
        },
    ) {
        return;
    }
    use crate::desktop::wallpaper::{BgEntry, BgSource, bundled_dir, bundled_path};
    use crate::native_test::Native;
    use crate::settings::clock_stage::{self as stage, Edit};
    use cosmic::iced::{Event, Point, mouse};
    let root = isolated_root();
    let fixture = root.join("cosmic-root");
    let (probe, _) = Settings::init(Core::default(), Page::DesktopWallpaper);
    let bundled = bundled_dir(probe.wallpaper.store().unwrap());
    drop(probe);
    let ridge = bundled_path(&bundled, "halcyon-ridge");
    std::fs::create_dir_all(&bundled).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets/wallpapers/halcyon-ridge.png"),
        &ridge,
    )
    .unwrap();
    seed(
        &fixture.join("config/cosmic"),
        &[(
            "com.system76.CosmicBackground",
            "all",
            &ron::to_string(&BgEntry::new("all", BgSource::Path(ridge.clone()))).unwrap(),
        )],
    );
    Config::save_clock(json!({"depth": true, "face": "digital", "surface": "none"})).unwrap();
    let (mut app, initial) = Settings::init(Core::default(), Page::Clock);
    settle_settings(&mut app, initial);
    drive_wallpaper(&mut app);
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    assert!(
        app.ridge_wallpaper(),
        "{:?} {:?} {}",
        app.wallpaper.entry(),
        ridge,
        app.status
    );
    assert!(app.depth_block().is_none());
    assert!(app.clock_stage_open());

    let screen = stage::known(app.placement.preview_snapshot().screen);
    let (start, clock) = app.clock_stage_place().expect("the clock is on the stage");
    let saved = || {
        let (mut fresh, initial) = Settings::init(Core::default(), Page::Clock);
        settle_settings(&mut fresh, initial);
        fresh.clock_stage_place().unwrap().0
    };
    let near = |a: [f32; 2], b: [f32; 2]| (0..2).all(|i| (a[i] - b[i]).abs() < 1.5);
    let lit_at = |app: &Settings, position: [f32; 2]| {
        let frame = stage::frame(position, clock, screen);
        app.clock_stage.requested().map(|r| &r.frame) == Some(&frame)
            && app.clock_stage.lit(&frame).is_some()
    };
    // Opening the page already lights the clock where it is.
    assert!(lit_at(&app, start), "{:?}", app.clock_stage.requested());
    // A 16:9 screen reuses the Home picture instead of drawing its own.
    assert_eq!(screen, [1920., 1080.]);
    assert!(app.clock_stage.backdrop().is_none());
    assert!(app.profiles.home_image().is_some());

    let mut native = Native::new(size);
    let body = native_reveal(&app, &mut native, stage::CLOCK_ID);
    let scale = body.width / clock[0];
    let send = |app: &mut Settings, native: &mut Native, event: mouse::Event, at: Point| {
        for message in native.event(app.settings_view(), Event::Mouse(event), at).0 {
            send_settings(app, message);
        }
    };
    let grab = body.center();
    send(
        &mut app,
        &mut native,
        mouse::Event::CursorMoved { position: grab },
        grab,
    );
    send(
        &mut app,
        &mut native,
        mouse::Event::ButtonPressed(mouse::Button::Left),
        grab,
    );
    assert!(app.clock_stage.dragging());

    // Past the stage's far corner, the clock stops at the screen's edge.
    let stage_box = native.bounds(app.settings_view(), "clock-stage");
    let corner = Point::new(
        stage_box.center_x() + screen[0] * scale / 2. - 1.,
        body.y + (screen[1] - start[1]) * scale - 1.,
    );
    send(
        &mut app,
        &mut native,
        mouse::Event::CursorMoved { position: corner },
        corner,
    );
    let (edge, _) = app.clock_stage_place().unwrap();
    assert!(edge[0] + clock[0] <= screen[0] + 0.5 && edge[1] + clock[1] <= screen[1] + 0.5);
    assert!(edge[0] > start[0], "{edge:?} from {start:?}");

    // High in the sky. While the pointer moves, nothing is saved and the
    // old lit picture is not swapped for a new render.
    let target = [(screen[0] - clock[0]) / 2., screen[1] * 0.08];
    let to = Point::new(
        grab.x + (target[0] - start[0]) * scale,
        grab.y + (target[1] - start[1]) * scale,
    );
    send(
        &mut app,
        &mut native,
        mouse::Event::CursorMoved { position: to },
        to,
    );
    let (moved, _) = app.clock_stage_place().unwrap();
    // Within one snap step of where the pointer put it.
    assert!(
        (0..2).all(|i| (moved[i] - target[i]).abs() <= 12.),
        "{moved:?} vs {target:?}"
    );
    assert!(near(saved(), start));
    assert!(!lit_at(&app, moved));
    // The pointer comes up wherever it is; the stage hears it through its
    // drag subscription.
    send(
        &mut app,
        &mut native,
        mouse::Event::ButtonReleased(mouse::Button::Left),
        to,
    );
    let release = stage::drag_input(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        cosmic::iced::event::Status::Ignored,
    )
    .unwrap();
    send_settings(&mut app, Message::ClockStage(release));
    assert!(!app.clock_stage.dragging());
    assert!(!app.error, "{}", app.status);
    assert!(near(saved(), moved), "{:?} vs {moved:?}", saved());
    assert!(lit_at(&app, moved), "{:?}", app.clock_stage.requested());
    assert!(app.clock_stage_moved());

    for (theme, name) in [
        (cosmic::Theme::light(), "light"),
        (cosmic::Theme::dark(), "dark"),
    ] {
        for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
            send_settings(&mut app, Message::Resized(size));
            let mut native = Native::new(size);
            native_reveal(&app, &mut native, "clock-stage-reset");
            assert!(native.overflowing_text(app.settings_view()).is_empty());
            assert!(native.duplicate_ids(app.settings_view()).is_empty());
            let stage_box = native.bounds(app.settings_view(), "clock-stage");
            let picture = stage::stage_size([stage_box.width, f32::INFINITY], screen);
            assert!(picture[0] <= stage::MAX_WIDTH);
            assert!((picture[0] / picture[1] - screen[0] / screen[1]).abs() < 0.01);
            native.capture(
                app.settings_view(),
                &theme,
                &format!("clock-stage-{name}-{}", size.width),
            );
        }
    }
    send_settings(&mut app, Message::Resized(size));
    let mut native = Native::new(size);

    // One Undo puts it back, on the desktop and on the stage.
    assert!(app.page_undo_button().is_some());
    native_click(&mut app, &mut native, "settings-undo");
    assert!(!app.error, "{}", app.status);
    assert!(near(app.clock_stage_place().unwrap().0, start));
    assert!(near(saved(), start));
    assert!(lit_at(&app, start), "{:?}", app.clock_stage.requested());

    // Arrow keys move it only when the stage clock has focus; each nudge is
    // saved and undone on its own.
    let right = Event::Keyboard(cosmic::iced::keyboard::Event::KeyPressed {
        key: cosmic::iced::keyboard::Key::Named(cosmic::iced::keyboard::key::Named::ArrowRight),
        modified_key: cosmic::iced::keyboard::Key::Named(
            cosmic::iced::keyboard::key::Named::ArrowRight,
        ),
        physical_key: cosmic::iced::keyboard::key::Physical::Unidentified(
            cosmic::iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: cosmic::iced::keyboard::Location::Standard,
        modifiers: cosmic::iced::keyboard::Modifiers::SHIFT,
        text: None,
        repeat: false,
    });
    assert!(matches!(
        stage::arrow_input(right.clone(), cosmic::iced::event::Status::Ignored),
        Some(Edit::Arrow([1, 0], true))
    ));
    assert!(stage::arrow_input(right, cosmic::iced::event::Status::Captured).is_none());
    send_settings(&mut app, Message::ClockStage(Edit::Nudge([1, 0], true)));
    let nudged = app.clock_stage_place().unwrap().0;
    assert!(nudged[0] > start[0] + 1.);
    assert!(near(saved(), nudged));
    send_settings(&mut app, Message::UndoDesktop);
    assert!(near(saved(), start));

    // Reset position goes back to the automatic place.
    send_settings(&mut app, Message::ClockStage(Edit::Nudge([0, 1], true)));
    assert!(app.clock_stage_moved());
    native_reveal(&app, &mut native, "clock-stage-reset");
    native_click(&mut app, &mut native, "clock-stage-reset");
    assert!(!app.clock_stage_moved());
    let home = app.placement.default_position(Widget::Clock.key()).unwrap();
    assert!(near(saved(), home));

    // Scene lighting changes redraw the stage clock.
    let before = app.clock_stage.requested().cloned();
    send_settings(
        &mut app,
        Message::ClockLighting(crate::depth::Lighting::Rich),
    );
    assert_ne!(app.clock_stage.requested().cloned(), before);
    assert!(lit_at(&app, home));
    // With the hills off the stage shows the plain clock.
    send_settings(&mut app, Message::ClockDepth(false));
    assert!(app.clock_stage.requested().is_none());
    send_settings(&mut app, Message::ClockDepth(true));
    send_settings(
        &mut app,
        Message::ClockLighting(crate::depth::Lighting::Soft),
    );

    // Comparison renders over Ridge with Soft lighting: the stage, and the
    // same request drawn at the screen's full size.
    if let Some(dir) = std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE") {
        let dir = std::path::PathBuf::from(dir);
        let picture = image::open(&ridge).unwrap().to_rgba8();
        let mut log = String::new();
        for (name, at) in [
            ("sky", [(screen[0] - clock[0]) * 0.5, screen[1] * 0.08]),
            ("ridge", [(screen[0] - clock[0]) * 0.55, screen[1] * 0.33]),
            ("left", [0., screen[1] * 0.3]),
        ] {
            let body = native_reveal(&app, &mut native, stage::CLOCK_ID);
            let (from, _) = app.clock_stage_place().unwrap();
            let grab = body.center();
            let to = Point::new(
                grab.x + (at[0] - from[0]) * scale,
                grab.y + (at[1] - from[1]) * scale,
            );
            send(
                &mut app,
                &mut native,
                mouse::Event::CursorMoved { position: grab },
                grab,
            );
            send(
                &mut app,
                &mut native,
                mouse::Event::ButtonPressed(mouse::Button::Left),
                grab,
            );
            send(
                &mut app,
                &mut native,
                mouse::Event::CursorMoved { position: to },
                to,
            );
            send_settings(&mut app, Message::ClockStage(Edit::Release));
            let (placed, _) = app.clock_stage_place().unwrap();
            let frame = stage::frame(placed, clock, screen);
            assert!(app.clock_stage.lit(&frame).is_some(), "{name} {placed:?}");
            let request = app.clock_stage.requested().cloned().unwrap();
            let lit = crate::depth::render(&request).unwrap();
            log += &format!("{name}: {:?}\n", request.frame);
            use cosmic::iced::widget::{image as native_image, pin, stack};
            let view: cosmic::Element<'_, Message> = stack([
                native_image(native_image::Handle::from_rgba(
                    picture.width(),
                    picture.height(),
                    picture.as_raw().clone(),
                ))
                .width(screen[0])
                .height(screen[1])
                .content_fit(cosmic::iced::ContentFit::Cover)
                .into(),
                pin(native_image(native_image::Handle::from_rgba(
                    lit.width, lit.height, lit.rgba,
                ))
                .width(frame.size[0])
                .height(frame.size[1]))
                .position(Point::new(frame.origin[0], frame.origin[1]))
                .into(),
            ])
            .width(screen[0])
            .height(screen[1])
            .into();
            let pixels =
                Native::new(Size::new(screen[0], screen[1])).render(view, &cosmic::Theme::dark());
            image::save_buffer(
                dir.join(format!("clock-stage-render-soft-{name}.png")),
                &pixels,
                screen[0] as u32,
                screen[1] as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            for (theme, mode) in [
                (cosmic::Theme::light(), "light"),
                (cosmic::Theme::dark(), "dark"),
            ] {
                native.capture(
                    app.settings_view(),
                    &theme,
                    &format!("clock-stage-soft-{name}-{mode}"),
                );
            }
        }
        std::fs::write(dir.join("clock-stage-render-soft.txt"), log).unwrap();
    }
}

/// Your own picture, from no scene model to a verdict: the download's
/// progress, cancel, a failed file and its retry, finding the foreground,
/// a nudge with its one-step Undo, the cut clock, a picture with no
/// foreground, and removing the model. Nothing here reaches the network:
/// the download task is dropped unpolled and its results are sent by hand.
#[test]
fn own_wallpaper_clock_page_walks_every_scene_state() {
    if desktop_test_child(
        "own_wallpaper_clock_page_walks_every_scene_state",
        DesktopEnv {
            app_config: true,
            fixture: true,
            icons: Icons::Shown,
            ..Default::default()
        },
    ) {
        return;
    }
    use crate::depth::{Found, cache, model, scene::Verdict};
    use crate::desktop::wallpaper::{BgEntry, BgSource};
    use crate::native_test::Native;
    use crate::settings::clock_stage as stage;
    use crate::settings::depth::{ModelState, Scene};
    let root = isolated_root();
    let fixture = root.join("cosmic-root");
    let pictures = root.join("pictures");
    std::fs::create_dir_all(&pictures).unwrap();
    let (hill, key, files) = crate::depth::tests::own::hill(&pictures, 300);
    seed(
        &fixture.join("config/cosmic"),
        &[(
            "com.system76.CosmicBackground",
            "all",
            &ron::to_string(&BgEntry::new("all", BgSource::Path(hill.clone()))).unwrap(),
        )],
    );
    Config::save_clock(json!({"depth": true, "face": "digital", "surface": "none"})).unwrap();
    let (mut app, initial) = Settings::init(Core::default(), Page::Clock);
    app.scenes.hold = true;
    settle_settings(&mut app, initial);
    drive_wallpaper(&mut app);
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    assert!(app.picture_wallpaper() && !app.ridge_wallpaper());
    let screen = stage::known(app.placement.preview_snapshot().screen);
    assert_eq!(screen, [1920., 1080.]);

    // The clock high in the sky, where the hill can't cover it.
    let (_, clock) = app.clock_stage_place().unwrap();
    let high = [(screen[0] - clock[0]) / 2., 60.];
    assert!(app.placement.place_widget(Widget::Clock.key(), high));
    send_settings(&mut app, Message::Placement(placement::Message::Save));
    let saved = || {
        let (mut fresh, initial) = Settings::init(Core::default(), Page::Clock);
        fresh.scenes.hold = true;
        settle_settings(&mut fresh, initial);
        fresh.clock_stage_place().unwrap().0
    };
    let near = |a: [f32; 2], b: [f32; 2]| (0..2).all(|i| (a[i] - b[i]).abs() < 1.5);
    assert!(near(saved(), high));

    let has = |app: &Settings, text: &str| {
        Native::new(size).text_all(app.settings_view(), text).len() == 1
    };
    let reason = |app: &Settings| app.depth_block().map(|b| b.reason.into_owned());
    let present = |app: &Settings, id: &str| {
        Native::new(size)
            .try_bounds(app.settings_view(), id)
            .is_some()
    };
    let shots = |app: &mut Settings, name: &str, reveal: &str| {
        for (theme, mode) in [
            (cosmic::Theme::light(), "light"),
            (cosmic::Theme::dark(), "dark"),
        ] {
            for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
                send_settings(app, Message::Resized(size));
                let mut native = Native::new(size);
                native_reveal(app, &mut native, reveal);
                assert!(
                    native.overflowing_text(app.settings_view()).is_empty(),
                    "{name}"
                );
                assert!(
                    native.duplicate_ids(app.settings_view()).is_empty(),
                    "{name}"
                );
                native.capture(
                    app.settings_view(),
                    &theme,
                    &format!("own-depth-{name}-{mode}-{}", size.width),
                );
            }
        }
        send_settings(app, Message::Resized(size));
    };

    // No model: the reason, and the model's row explains it and offers it.
    assert_eq!(app.scenes.model, ModelState::Missing);
    assert_eq!(
        app.clock_stage.found(),
        Some(&Found::Unknown { key: key.clone() })
    );
    assert_eq!(
        reason(&app).as_deref(),
        Some("Download the scene model to use this on your own pictures")
    );
    assert!(has(&app, "Let the scene cover the clock"));
    assert!(has(
        &app,
        "Finds the foreground in your own pictures. About 99 MB, and it runs only on this computer."
    ));
    assert!(has(&app, "Download"));
    assert!(!present(&app, "clock-depth-fix") && !present(&app, "clock-lighting"));
    shots(&mut app, "no-model", "clock-scene-model");

    // Downloading, with honest progress, then cancelled.
    drop(app.update(Message::SceneModel(Scene::Download)));
    let first = app.scenes.generation();
    assert_eq!(app.scenes.model, ModelState::Downloading(0));
    send_settings(
        &mut app,
        Message::SceneModel(Scene::Progress(first, 42_500_000)),
    );
    assert!(has(&app, "Downloading… 42 of 99 MB"));
    assert_eq!(
        reason(&app).as_deref(),
        Some("Downloading the scene model…")
    );
    assert!(has(&app, "Cancel"));
    shots(&mut app, "downloading", "clock-scene-model");
    send_settings(&mut app, Message::SceneModel(Scene::Cancel));
    assert_eq!(
        app.scenes.model,
        ModelState::Failed(model::Error::Cancelled)
    );
    // The cancelled download's own ending changes nothing.
    send_settings(
        &mut app,
        Message::SceneModel(Scene::Downloaded(first, Err(model::Error::Cancelled))),
    );
    send_settings(
        &mut app,
        Message::SceneModel(Scene::Progress(first, 60_000_000)),
    );
    assert_eq!(
        app.scenes.model,
        ModelState::Failed(model::Error::Cancelled)
    );
    assert!(has(
        &app,
        "Download cancelled. About 99 MB, and it runs only on this computer."
    ));

    // A file that isn't the model: said plainly, with a retry.
    drop(app.update(Message::SceneModel(Scene::Download)));
    let second = app.scenes.generation();
    assert_ne!(second, first);
    send_settings(
        &mut app,
        Message::SceneModel(Scene::Downloaded(second, Err(model::Error::Hash))),
    );
    let wrong = "The download wasn’t the expected file, so it was removed";
    assert_eq!(
        reason(&app).as_deref(),
        Some("Download the scene model to use this on your own pictures")
    );
    assert!(has(&app, wrong), "said once, on the Scene model row");
    assert!(has(&app, "Try again"));
    shots(&mut app, "download-failed", "clock-scene-model");

    // Retried and finished: finding the foreground starts by itself.
    drop(app.update(Message::SceneModel(Scene::Download)));
    let third = app.scenes.generation();
    let path = model::path().unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::File::create(&path)
        .unwrap()
        .set_len(model::SIZE)
        .unwrap();
    send_settings(
        &mut app,
        Message::SceneModel(Scene::Downloaded(third, Ok(()))),
    );
    assert_eq!(app.scenes.model, ModelState::Ready);
    assert_eq!(reason(&app).as_deref(), Some("Finding the foreground…"));
    assert!(has(&app, "Remove"));
    shots(&mut app, "analysing", "clock-scene-model");

    // It failed: one reason, one button, and the button tries again.
    send_settings(
        &mut app,
        Message::SceneModel(Scene::Analysed(
            key.clone(),
            Err(crate::depth::analyse::Failure::Stopped),
        )),
    );
    assert_eq!(
        reason(&app).as_deref(),
        Some("Couldn’t find the foreground in this picture")
    );
    let mut native = Native::new(size);
    native_reveal(&app, &mut native, "clock-depth-fix");
    native_click(&mut app, &mut native, "clock-depth-fix");
    assert_eq!(reason(&app).as_deref(), Some("Finding the foreground…"));

    // Found: high in the sky it needs a nudge, shown on the stage.
    let scenes = cache::root().unwrap();
    cache::store(&scenes, &key, &files).unwrap();
    send_settings(
        &mut app,
        Message::SceneModel(Scene::Analysed(key.clone(), Ok(key.clone()))),
    );
    let Some(Found::Judged { judgement, .. }) = app.clock_stage.found().cloned() else {
        panic!("{:?}", app.clock_stage.found());
    };
    assert_eq!(judgement.verdict, Verdict::NeedsNudge, "{judgement:?}");
    let spot = judgement.suggested.unwrap();
    assert!(spot[1] > high[1] + 100., "{spot:?}");
    assert_eq!(
        reason(&app).as_deref(),
        Some("The scene covers the clock a little lower down")
    );
    let frame = stage::frame(high, clock, screen);
    assert!(
        app.clock_stage.lit(&frame).is_none(),
        "the plain clock until it moves"
    );
    assert!(present(&app, "clock-stage-suggested"));
    shots(&mut app, "nudge", "clock-stage-suggested");

    // One click moves it there, as one Undo step, and the scene covers it.
    let mut native = Native::new(size);
    native_reveal(&app, &mut native, "clock-stage-suggested");
    native_click(&mut app, &mut native, "clock-stage-suggested");
    assert!(!app.error, "{}", app.status);
    let (moved, _) = app.clock_stage_place().unwrap();
    assert!(near(moved, spot), "{moved:?} vs {spot:?}");
    assert!(near(saved(), spot));
    let frame = stage::frame(moved, clock, screen);
    assert!(app.clock_stage.lit(&frame).is_some());
    assert!(app.depth_block().is_none(), "{:?}", reason(&app));
    assert!(present(&app, "clock-lighting"));
    assert!(!present(&app, "clock-stage-suggested"));
    shots(&mut app, "works", "clock-scene-model");
    let mut native = Native::new(size);
    native_click(&mut app, &mut native, "settings-undo");
    assert!(near(app.clock_stage_place().unwrap().0, high));
    assert!(near(saved(), high));
    assert!(present(&app, "clock-stage-suggested"));
    let mut native = Native::new(size);
    native_click(&mut app, &mut native, "clock-stage-suggested");
    assert!(near(saved(), spot));

    // A picture with nothing in front: the plain clock and one sentence.
    let (flat, flat_key, flat_files) = crate::depth::tests::own::flat(&pictures);
    std::fs::copy(&flat, &hill).unwrap();
    cache::store(&scenes, &flat_key, &flat_files).unwrap();
    app.clock_stage.redraw();
    send_settings(&mut app, Message::Resized(size));
    assert!(matches!(
        app.clock_stage.found(),
        Some(Found::Judged { judgement, .. }) if judgement.verdict == Verdict::NotUseful
    ));
    assert_eq!(
        reason(&app).as_deref(),
        Some("This picture has no clear foreground")
    );
    assert!(has(&app, "Use Ridge"));
    assert!(!present(&app, "clock-lighting"));
    assert!(app.clock_stage.lit(&frame).is_none());
    shots(&mut app, "not-useful", "clock-depth-fix");

    // Removing the model forgets every analysed picture too.
    let mut native = Native::new(size);
    native_reveal(&app, &mut native, "clock-scene-model");
    native_click(&mut app, &mut native, "clock-scene-model");
    assert_eq!(app.scenes.model, ModelState::Missing);
    assert!(!path.exists());
    assert!(!cache::exists(&scenes, &flat_key) && !cache::exists(&scenes, &key));
    assert_eq!(
        reason(&app).as_deref(),
        Some("Download the scene model to use this on your own pictures")
    );
    assert!(!app.error, "{}", app.status);
}
