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
        "Downloads a 99 MB model from Hugging Face once. Your pictures are analyzed only on this computer, and nothing is uploaded."
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
    // The switch's own line says how far along it is.
    assert_eq!(
        reason(&app).as_deref(),
        Some("Getting ready… 42 of 99 MB")
    );
    assert!(has(&app, "Cancel"));
    shots(&mut app, "downloading", "clock-scene-model");
    let depth = |app: &Settings| app.config.clock_layer(app.clock_moment_target()).depth;
    assert!(depth(&app));
    send_settings(&mut app, Message::SceneModel(Scene::Cancel));
    assert_eq!(
        app.scenes.model,
        ModelState::Failed(model::Error::Cancelled)
    );
    assert!(!depth(&app), "no model, so the switch goes off");
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
        "Download cancelled. It’s 99 MB from Hugging Face, and your pictures are analyzed only on this computer."
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
    assert!(!depth(&app));
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
    assert!(depth(&app), "finishing the download turns the switch back on");
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
        Some("The scene covers the clock a little lower down. The outline shows where.")
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
        Some(
            "This picture has no clear foreground. Pictures with a person, hill or building \
             in the lower half work best."
        )
    );
    // Another picture first; Ridge says it replaces the wallpaper.
    assert!(has(&app, "Try a different picture…"));
    assert!(has(&app, "Use Halcyon’s Ridge wallpaper instead"));
    assert!(present(&app, "clock-depth-alternative"));
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
    // Another picture is chosen on Wallpaper, where the change and its Undo live.
    drop(app.update(Message::ChooseScenePicture));
    assert_eq!(app.page, Page::DesktopWallpaper);
}

/// The stage clock's corner handle resizes its box, shape kept: the drag
/// draws nothing new until it rests or ends, the release saves size and
/// place as one Undo step and lights the clock at its new size, and `+`/`-`
/// do the same in tenths of the automatic size.
#[test]
fn clock_stage_corner_resizes_the_clock_as_one_undo_step() {
    if desktop_test_child(
        "clock_stage_corner_resizes_the_clock_as_one_undo_step",
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
    use cosmic::iced::{Event, Point, keyboard, mouse};
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
    send_settings(
        &mut app,
        Message::ClockLighting(crate::depth::Lighting::Soft),
    );
    assert!(app.ridge_wallpaper() && app.clock_stage_open());
    let screen = stage::known(app.placement.preview_snapshot().screen);
    assert_eq!(screen, [1920., 1080.]);
    let text_size = app.config.clock.scale;

    // Somewhere in the sky with room to grow.
    let clock = Widget::Clock.key();
    assert!(app.placement.place_widget(clock, [700., 90.]));
    send_settings(&mut app, Message::Placement(placement::Message::Save));
    let (start, natural) = app.clock_stage_place().unwrap();
    assert_eq!(natural, [490., 200.]);
    let saved = || {
        let (mut fresh, initial) = Settings::init(Core::default(), Page::Clock);
        settle_settings(&mut fresh, initial);
        (
            fresh.clock_stage_place().unwrap(),
            fresh.config.widget_sizes.get(clock).copied(),
        )
    };
    let near = |a: [f32; 2], b: [f32; 2]| (0..2).all(|i| (a[i] - b[i]).abs() < 1.5);
    let lit_at = |app: &Settings, (position, size): ([f32; 2], [f32; 2])| {
        let frame = stage::frame(position, size, screen);
        app.clock_stage.requested().map(|r| &r.frame) == Some(&frame)
            && app.clock_stage.lit(&frame).is_some()
    };
    assert!(lit_at(&app, (start, natural)));
    assert_eq!(saved(), ((start, natural), None));

    // The handle is always there, faint, so the corner reads as something
    // to grab; it and the outline come up with the pointer over the clock
    // (or focus on it).
    let mut native = Native::new(size);
    let body = native_reveal(&app, &mut native, stage::CLOCK_ID);
    let scale = body.width / natural[0];
    assert!(!app.clock_stage.handle());
    let handle = native
        .try_bounds(app.settings_view(), "clock-stage-handle")
        .expect("the resize handle shows before the pointer reaches it");
    assert!((handle.x + handle.width - (body.x + body.width)).abs() < 1.);
    assert!((handle.y + handle.height - (body.y + body.height)).abs() < 1.);
    native.capture(
        app.settings_view(),
        &cosmic::Theme::light(),
        "clock-stage-faint-handle-light-1280",
    );
    // Enter on the selected clock only selects it: no dead message.
    native.operate(
        app.settings_view(),
        Box::new(cosmic::iced::advanced::widget::operation::focusable::focus(
            cosmic::widget::Id::new(stage::CLOCK_ID),
        )),
    );
    let enter = keyboard::Key::Named(keyboard::key::Named::Enter);
    let (messages, _) = native.event(
        app.settings_view(),
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: enter.clone(),
            modified_key: enter,
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Enter),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        }),
        Point::ORIGIN,
    );
    assert!(
        messages
            .iter()
            .all(|m| matches!(m, Message::ClockStage(Edit::Focus(true)))),
        "{messages:?}"
    );
    for message in messages {
        send_settings(&mut app, message);
    }
    send_settings(&mut app, Message::ClockStage(Edit::Focus(false)));
    assert!(!app.clock_stage.handle());
    let send = |app: &mut Settings, native: &mut Native, event: mouse::Event, at: Point| {
        for message in native.event(app.settings_view(), Event::Mouse(event), at).0 {
            send_settings(app, message);
        }
    };
    let grip = Point::new(body.x + body.width - 4., body.y + body.height - 4.);
    send(
        &mut app,
        &mut native,
        mouse::Event::CursorMoved { position: grip },
        grip,
    );
    assert!(app.clock_stage.handle());
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
            assert_eq!(
                native
                    .text_all(
                        app.settings_view(),
                        "Drag the clock to move it, or drag its corner to resize. \
                         With the keyboard, Tab selects the clock, arrow keys move it, \
                         + and − resize it, and Esc cancels a drag."
                    )
                    .len(),
                1
            );
            native.capture(
                app.settings_view(),
                &theme,
                &format!("clock-stage-handle-{name}-{}", size.width),
            );
        }
    }
    send_settings(&mut app, Message::Resized(size));
    let mut native = Native::new(size);
    let body = native_reveal(&app, &mut native, stage::CLOCK_ID);
    let grip = Point::new(body.x + body.width - 4., body.y + body.height - 4.);
    send(
        &mut app,
        &mut native,
        mouse::Event::CursorMoved { position: grip },
        grip,
    );
    send(
        &mut app,
        &mut native,
        mouse::Event::ButtonPressed(mouse::Button::Left),
        grip,
    );
    assert!(app.clock_stage.dragging() && app.placement.resizing());

    // A tenth larger along the diagonal: the top-left stays, the shape is
    // kept, nothing is saved and the old render stays (stretched) meanwhile.
    let to = Point::new(grip.x + 49. * scale, grip.y + 20. * scale);
    send(
        &mut app,
        &mut native,
        mouse::Event::CursorMoved { position: to },
        to,
    );
    let (at, grown) = app.clock_stage_place().unwrap();
    assert!(near(at, start), "{at:?} vs {start:?}");
    assert!(near(grown, [539., 220.]), "{grown:?}");
    assert_eq!(saved(), ((start, natural), None));
    assert_eq!(
        app.clock_stage.requested().map(|r| r.frame.size),
        Some(natural)
    );
    assert!(app.clock_stage.last_lit().is_some());
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
    assert!(!app.clock_stage.dragging() && !app.error, "{}", app.status);
    // Saved: the box and the place the desktop decodes, and the lit clock
    // drawn again at the new size.
    let ((saved_at, saved_size), stored) = saved();
    assert!(near(saved_at, start) && near(saved_size, grown));
    assert_eq!(stored, Some(grown));
    assert!(
        lit_at(&app, (start, grown)),
        "{:?}",
        app.clock_stage.requested()
    );
    // The box is the clock's size: the digits grew with it, and the
    // Text size slider, which fits them inside it, is untouched.
    assert_eq!(app.config.clock.scale, text_size);

    // One Undo restores both size and place.
    native_click(&mut app, &mut native, "settings-undo");
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.clock_stage_place().unwrap(), (start, natural));
    assert_eq!(saved(), ((start, natural), None));
    assert!(lit_at(&app, (start, natural)));

    // Keys: `+` and `-`, or Ctrl with an arrow, only when nobody else used
    // them; each step is one Undo step.
    let key = |key: keyboard::Key, modifiers: keyboard::Modifiers| {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    };
    let ignored = cosmic::iced::event::Status::Ignored;
    let plus = key(
        keyboard::Key::Character("+".into()),
        keyboard::Modifiers::SHIFT,
    );
    assert!(matches!(
        stage::arrow_input(plus.clone(), ignored),
        Some(Edit::Grow(1))
    ));
    assert!(stage::arrow_input(plus, cosmic::iced::event::Status::Captured).is_none());
    assert!(matches!(
        stage::arrow_input(
            key(
                keyboard::Key::Character("-".into()),
                keyboard::Modifiers::empty()
            ),
            ignored
        ),
        Some(Edit::Grow(-1))
    ));
    assert!(matches!(
        stage::arrow_input(
            key(
                keyboard::Key::Named(keyboard::key::Named::ArrowDown),
                keyboard::Modifiers::CTRL
            ),
            ignored
        ),
        Some(Edit::Grow(-1))
    ));
    assert!(matches!(
        stage::arrow_input(
            key(
                keyboard::Key::Named(keyboard::key::Named::ArrowDown),
                keyboard::Modifiers::empty()
            ),
            ignored
        ),
        Some(Edit::Arrow([0, 1], false))
    ));
    send_settings(&mut app, Message::ClockStage(Edit::Step(1)));
    send_settings(&mut app, Message::ClockStage(Edit::Step(1)));
    assert_eq!(saved().1, Some([588., 240.]));
    send_settings(&mut app, Message::UndoDesktop);
    assert_eq!(saved().1, Some([539., 220.]));
    send_settings(&mut app, Message::UndoDesktop);
    assert_eq!(saved(), ((start, natural), None));
    // At the smallest box another step changes nothing and adds no step.
    send_settings(&mut app, Message::ClockStage(Edit::Step(-9)));
    assert_eq!(saved().1, Some([245., 100.]));
    send_settings(&mut app, Message::ClockStage(Edit::Step(-1)));
    assert_eq!(saved().1, Some([245., 100.]));
    send_settings(&mut app, Message::UndoDesktop);
    assert_eq!(saved(), ((start, natural), None));

    // Keyboard focus shows the handle too.
    send_settings(&mut app, Message::ClockStage(Edit::Hover(false)));
    assert!(!app.clock_stage.handle());
    send_settings(&mut app, Message::ClockStage(Edit::Focus(true)));
    assert!(app.clock_stage.handle());
    send_settings(&mut app, Message::ClockStage(Edit::Focus(false)));

    // The analog face grows with the same box.
    use crate::widget_style::resolver::ClockFace;
    send_settings(&mut app, Message::ClockFace(Some(ClockFace::Analog)));
    assert_eq!(app.config.clock.face, Some(ClockFace::Analog));
    send_settings(&mut app, Message::ClockStage(Edit::Step(4)));
    assert_eq!(saved().1, Some([686., 280.]));
    send_settings(&mut app, Message::ClockStage(Edit::Hover(true)));
    for (theme, name) in [
        (cosmic::Theme::light(), "light"),
        (cosmic::Theme::dark(), "dark"),
    ] {
        let mut native = Native::new(size);
        native_reveal(&app, &mut native, stage::CLOCK_ID);
        native.capture(
            app.settings_view(),
            &theme,
            &format!("clock-stage-analog-140-{name}"),
        );
    }
    send_settings(&mut app, Message::UndoDesktop);
    assert_eq!(saved(), ((start, natural), None));
    send_settings(&mut app, Message::ClockFace(Some(ClockFace::Digital)));

    // Renders over Ridge with Soft lighting at 70%, 100% and 160%: the stage,
    // and the same request drawn at the screen's full size.
    if let Some(dir) = std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE") {
        let dir = std::path::PathBuf::from(dir);
        let picture = image::open(&ridge).unwrap().to_rgba8();
        let mut log = String::new();
        for (percent, steps) in [(70, -3), (100, 0), (160, 6)] {
            if steps != 0 {
                send_settings(&mut app, Message::ClockStage(Edit::Step(steps)));
            }
            let place = app.clock_stage_place().unwrap();
            assert!(near(place.1, natural.map(|v| v * percent as f32 / 100.)));
            assert!(lit_at(&app, place), "{percent}");
            let request = app.clock_stage.requested().cloned().unwrap();
            let frame = request.frame.clone();
            let lit = crate::depth::render(&request).unwrap();
            log += &format!("{percent}%: {:?}\n", frame);
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
                dir.join(format!("clock-stage-render-soft-{percent}.png")),
                &pixels,
                screen[0] as u32,
                screen[1] as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            let mut native = Native::new(size);
            native_reveal(&app, &mut native, stage::CLOCK_ID);
            native.capture(
                app.settings_view(),
                &cosmic::Theme::dark(),
                &format!("clock-stage-soft-{percent}-dark"),
            );
            if steps != 0 {
                send_settings(&mut app, Message::UndoDesktop);
            }
        }
        std::fs::write(dir.join("clock-stage-render-soft-sizes.txt"), log).unwrap();
    }
}

/// White digits straight on a white picture: the stage says so in words,
/// and its button gives the clock a card, which settles it.
#[test]
fn clock_stage_warns_when_the_clock_is_hard_to_read_on_the_picture() {
    if desktop_test_child(
        "clock_stage_warns_when_the_clock_is_hard_to_read_on_the_picture",
        DesktopEnv {
            app_config: true,
            fixture: true,
            icons: Icons::Shown,
            ..Default::default()
        },
    ) {
        return;
    }
    use crate::desktop::wallpaper::{BgEntry, BgSource};
    use crate::native_test::Native;
    const LOW_CONTRAST_WARNING: &str =
        "Hard to read on this picture. Set Background to Card, or move the clock.";
    use crate::widget_style::resolver::ClockSurface;
    use cosmic::iced::{Event, mouse};
    let root = isolated_root();
    let fixture = root.join("cosmic-root");
    let picture = root.join("white.png");
    image::RgbImage::from_pixel(64, 36, image::Rgb([250, 250, 250]))
        .save(&picture)
        .unwrap();
    seed(
        &fixture.join("config/cosmic"),
        &[(
            "com.system76.CosmicBackground",
            "all",
            &ron::to_string(&BgEntry::new("all", BgSource::Path(picture))).unwrap(),
        )],
    );
    Config::save_clock(json!({"face": "digital", "surface": "none"})).unwrap();
    let (mut app, initial) = Settings::init(Core::default(), Page::Clock);
    settle_settings(&mut app, initial);
    drive_wallpaper(&mut app);
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    assert!(app.clock_stage_open());
    assert!(
        app.clock_stage
            .backdrop()
            .or(app.profiles.home_image())
            .is_some(),
        "the picture is on the stage"
    );
    let ratio = app.clock_stage_contrast().expect("a clock on the picture");
    assert!(ratio < 1.5, "{ratio}");

    let mut native = Native::new(size);
    native_reveal(&app, &mut native, "clock-stage-contrast");
    assert!(
        !native
            .text_all(app.settings_view(), LOW_CONTRAST_WARNING)
            .is_empty(),
        "the warning is said in words"
    );
    native.capture(
        app.settings_view(),
        &cosmic::Theme::dark(),
        "clock-stage-low-contrast-dark-1280",
    );
    native.capture(
        app.settings_view(),
        &cosmic::Theme::light(),
        "clock-stage-low-contrast-light-1280",
    );
    let use_card = native.bounds(app.settings_view(), "clock-stage-use-card");
    let mut messages = Vec::new();
    for event in [
        mouse::Event::CursorMoved {
            position: use_card.center(),
        },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        messages.extend(
            native
                .event(app.settings_view(), Event::Mouse(event), use_card.center())
                .0,
        );
    }
    // The same edit as Background › Card, for the clock's look.
    assert_eq!(messages.len(), 1, "{messages:?}");
    for message in messages {
        send_settings(&mut app, message);
    }
    assert_eq!(app.config.moment_clock().surface, Some(ClockSurface::Panel));
    assert_eq!(app.clock_stage_contrast(), None);
    assert!(
        native
            .try_bounds(app.settings_view(), "clock-stage-contrast")
            .is_none()
    );
}

/// The stage is the desktop scaled down: the whole output with the area
/// panels leave, the clock box where and as large as the desktop puts it,
/// and a drag or resize there lands exactly where the desktop then shows
/// the clock. Writes side by side captures to `HALCYON_STAGE_EVIDENCE`.
#[test]
fn clock_stage_is_the_desktop_scaled_down() {
    if desktop_test_child(
        "clock_stage_is_the_desktop_scaled_down",
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
    use crate::displays::{OutputEvent, test_support::info};
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
    std::fs::write(&ridge, crate::depth::PICTURE).unwrap();
    // The Ridge picture, and a top panel that keeps the top 40 px.
    seed(
        &fixture.join("config/cosmic"),
        &[
            (
                "com.system76.CosmicBackground",
                "all",
                &ron::to_string(&BgEntry::new("all", BgSource::Path(ridge.clone()))).unwrap(),
            ),
            (
                "com.system76.CosmicPanel",
                "entries",
                "[\"Panel\",\"Dock\"]",
            ),
            ("com.system76.CosmicPanel.Panel", "anchor", "Top"),
            ("com.system76.CosmicPanel.Panel", "exclusive_zone", "true"),
            ("com.system76.CosmicPanel.Panel", "autohide", "Never"),
            ("com.system76.CosmicPanel.Panel", "size", "XS"),
            ("com.system76.CosmicPanel.Panel", "margin", "6"),
            ("com.system76.CosmicPanel.Panel", "output", "All"),
            ("com.system76.CosmicPanel.Dock", "exclusive_zone", "false"),
        ],
    );
    let wire = crate::displays::test_support::Wire::new();
    let output = || {
        let mut output = info(Some("DP-1"), (1920, 1080));
        output.modes.push(cosmic::cctk::sctk::output::Mode {
            dimensions: (1920, 1080),
            refresh_rate: 60_000,
            current: true,
            preferred: true,
        });
        output
    };
    let area = [1920., 1040.];
    // The desktop for the saved config: its clock box on the whole output.
    let desktop = || {
        let (mut cards, wire) =
            crate::app::Cards::on_output_for_test(Config::load(), "DP-1", (1920, 1080), area);
        let rect = cards
            .clock_on_output()
            .expect("the desktop places the clock");
        (cards, wire, rect)
    };
    let size = Size::new(1280., 900.);
    let open = |n: u32, published: &crate::layout_state::Snapshot| {
        crate::layout_state::publish(published, crate::layout_state::next_generation());
        let (mut app, initial) = Settings::init(Core::default(), Page::Clock);
        settle_settings(&mut app, initial);
        send_settings(
            &mut app,
            Message::DisplayOutput(OutputEvent::Created(Some(output())), wire.output(n)),
        );
        drive_wallpaper(&mut app);
        send_settings(&mut app, Message::Resized(size));
        app
    };
    // The stage's clock box, measured in what it draws, on the output.
    let drawn = |app: &Settings, native: &mut Native| {
        let picture = native.bounds(app.settings_view(), stage::SCREEN_ID);
        let body = native.bounds(app.settings_view(), stage::CLOCK_ID);
        let scale = picture.width / 1920.;
        assert!((picture.height / scale - 1080.).abs() < 1., "{picture:?}");
        (
            [
                (body.x - picture.x) / scale,
                (body.y - picture.y) / scale,
                body.width / scale,
                body.height / scale,
            ],
            scale,
            picture,
        )
    };
    let near =
        |a: [f32; 4], b: [f32; 4], within: f32| (0..4).all(|i| (a[i] - b[i]).abs() <= within);
    let recipe = serde_json::json!({
        "version": 1,
        "id": "r-6e1beedf0418a1000575316a47514616",
        "revision": "e8c8e85fd20d86ed08ff1e9724cf9669dcb16087d213d14e209f680730f77620",
        "name": "My clock",
        "kind": "clock",
        "parts": ["face", "date"],
        "caption": "Make room for today",
        "compact": "stack",
        "wide": "stack"
    });
    let cases = [
        (
            "recipe",
            json!({
                "widget_positions": {"clock": [0.0, 0.0]},
                "widget_sizes": {"clock": [512.0, 547.0]},
                "clock": {"alignment": "left", "center_vertically": true, "scale": 170,
                          "show_date": false},
                "recipes": {"clock": recipe}
            }),
        ),
        (
            "middle-200",
            json!({"widget_positions": {"clock": [0.5, 0.25]}, "clock": {"scale": 200}}),
        ),
        (
            "low-200",
            json!({
                "widget_positions": {"clock": [0.3, 0.8]},
                "widget_sizes": {"clock": [800.0, 330.0]},
                "clock": {"scale": 200}
            }),
        ),
        (
            "corner-100",
            json!({"widget_positions": {"clock": [1.0, 1.0]}, "clock": {"scale": 100}}),
        ),
    ];
    let evidence = std::env::var_os("HALCYON_STAGE_EVIDENCE").map(std::path::PathBuf::from);
    for (n, (name, case)) in cases.iter().enumerate() {
        let mut value = json!({"theme": "teal", "enabled_widgets": ["clock"]});
        for (key, member) in case.as_object().unwrap() {
            value[key] = member.clone();
        }
        std::fs::write(Config::path().unwrap(), serde_json::to_vec(&value).unwrap()).unwrap();
        let (cards, _desktop_wire, rect) = desktop();
        let app = open(n as u32 + 1, &cards.layout_snapshot());
        let mut native = Native::new(size);
        let (shown, scale, picture) = drawn(&app, &mut native);
        if let Some(root) = &evidence {
            std::fs::create_dir_all(root).unwrap();
            // The desktop: the picture as cosmic-bg covers the output, the
            // panel's strip, and the clock's own surface where it sits.
            let mut screen_image = image::load_from_memory(crate::depth::PICTURE)
                .unwrap()
                .resize_to_fill(1920, 1080, image::imageops::FilterType::Triangle)
                .to_rgba8();
            for y in 0..40 {
                for x in 0..1920 {
                    screen_image.put_pixel(x, y, image::Rgba([24, 24, 28, 255]));
                }
            }
            let clock_size = Size::new(rect[2], rect[3]);
            let pixels =
                Native::new(clock_size).render(cards.clock_surface(), &cosmic::Theme::dark());
            let surface =
                image::RgbaImage::from_raw(rect[2] as u32, rect[3] as u32, pixels).unwrap();
            image::imageops::overlay(
                &mut screen_image,
                &surface,
                rect[0].round() as i64,
                rect[1].round() as i64,
            );
            let desktop_image = image::imageops::resize(
                &screen_image,
                960,
                540,
                image::imageops::FilterType::Triangle,
            );
            let settings = image::RgbaImage::from_raw(
                size.width as u32,
                size.height as u32,
                native.render(app.settings_view(), &cosmic::Theme::dark()),
            )
            .unwrap();
            let stage_image = image::imageops::crop_imm(
                &settings,
                picture.x.round() as u32,
                picture.y.round() as u32,
                picture.width.round() as u32,
                picture.height.round() as u32,
            )
            .to_image();
            let stage_image = image::imageops::resize(
                &stage_image,
                960,
                540,
                image::imageops::FilterType::Triangle,
            );
            let mut both =
                image::RgbaImage::from_pixel(960 * 2 + 16, 540, image::Rgba([0, 0, 0, 255]));
            image::imageops::overlay(&mut both, &desktop_image, 0, 0);
            image::imageops::overlay(&mut both, &stage_image, 960 + 16, 0);
            both.save(root.join(format!("stage-vs-desktop-{name}.png")))
                .unwrap();
        }
        let screen = app.clock_stage_screen();
        assert_eq!(
            (screen.whole, screen.origin, screen.area),
            ([1920., 1080.], [0., 40.], area),
            "{name}"
        );
        let (position, clock) = app.clock_stage_place().unwrap();
        let placed = screen.on_output(position);
        assert!(
            near([placed[0], placed[1], clock[0], clock[1]], rect, 0.5),
            "{name}: Settings {placed:?} {clock:?}, desktop {rect:?}"
        );
        // Within a stage pixel of the desktop's box.
        assert!(
            near(shown, rect, 1. / scale),
            "{name}: {shown:?} vs {rect:?}"
        );
        // The corner handle sits at the box's corner.
        let handle = native.bounds(app.settings_view(), "clock-stage-handle");
        let corner = [
            picture.x + (rect[0] + rect[2]) * scale,
            picture.y + (rect[1] + rect[3]) * scale,
        ];
        assert!(
            (handle.x + handle.width - corner[0]).abs() <= 1.
                && (handle.y + handle.height - corner[1]).abs() <= 1.,
            "{name}: handle {handle:?}, corner {corner:?}"
        );
    }

    // Dragged on the stage, the clock lands where the desktop then shows it.
    let (cards, _desktop_wire, _) = desktop();
    let mut app = open(9, &cards.layout_snapshot());
    let mut native = Native::new(size);
    let (before, scale, _) = drawn(&app, &mut native);
    let body = native.bounds(app.settings_view(), stage::CLOCK_ID);
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
    let to = Point::new(grab.x - 120., grab.y - 60.);
    send(
        &mut app,
        &mut native,
        mouse::Event::CursorMoved { position: to },
        to,
    );
    send(
        &mut app,
        &mut native,
        mouse::Event::ButtonReleased(mouse::Button::Left),
        to,
    );
    send_settings(&mut app, Message::ClockStage(Edit::Release));
    assert!(!app.error, "{}", app.status);
    // One step larger, saved too.
    send_settings(&mut app, Message::ClockStage(Edit::Step(2)));
    assert!(!app.error, "{}", app.status);
    let (stage_rect, _, _) = drawn(&app, &mut native);
    assert!(
        (stage_rect[0] - before[0]).abs() > 100. / scale * 0.5,
        "{stage_rect:?} from {before:?}"
    );
    let (_, _, rect) = desktop();
    assert!(
        near(stage_rect, rect, 1. / scale),
        "after editing: stage {stage_rect:?}, desktop {rect:?}"
    );
}
