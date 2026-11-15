// The Clock page's face gallery and typeface tiles, through the native tree.

/// Every face shows as a picture and every shipped typeface as the time in
/// that typeface; a tile click saves exactly its choice, weight stays on
/// offer, and More fonts opens the searchable picker.
#[test]
fn clock_page_face_gallery_and_typefaces_choose_and_draw() {
    if desktop_test_child(
        "clock_page_face_gallery_and_typefaces_choose_and_draw",
        DesktopEnv {
            app_config: true,
            fixture: true,
            icons: Icons::Shown,
            ..Default::default()
        },
    ) {
        return;
    }
    use crate::native_test::Native;
    use crate::settings::navigation::ObjectTab;
    use crate::widget_style::resolver::{ClockFace, ClockWeight};
    let _root = isolated_root();
    let (mut app, initial) = Settings::init(Core::default(), Page::Clock);
    settle_settings(&mut app, initial);
    send_settings(&mut app, Message::ObjectTab(ObjectTab::Design));
    let faces: Vec<String> = ClockFace::ALL
        .iter()
        .map(|face| face.key().to_string())
        .collect();
    let fonts = [
        "clock-font-widgets",
        "clock-font-outfit",
        "clock-font-antonio",
        "clock-font-red-hat-mono",
        "clock-font-bodoni-moda",
        "clock-font-more",
    ];
    for width in [1280., 820.] {
        let size = Size::new(width, 800.);
        send_settings(&mut app, Message::Resized(size));
        let mut native = Native::new(size);
        for key in &faces {
            let tile = native_reveal(&app, &mut native, &format!("clock-face-{key}"));
            assert!(
                tile.width >= 120. && tile.height >= 90.,
                "{key} at {width}: {tile:?}"
            );
        }
        for id in fonts {
            let tile = native_reveal(&app, &mut native, id);
            assert!(
                tile.width >= 120. && tile.height >= 100.,
                "{id} at {width}: {tile:?}"
            );
        }
        assert!(
            native.overflowing_text(app.settings_view()).is_empty(),
            "{:?}",
            native.overflowing_text(app.settings_view())
        );
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        // Each picture stays inside its tile: the line just above the tile
        // and the line between picture and name show one plain color, not
        // the wallpaper. The names sit on the tile under the picture, and a
        // dial's picture says nothing but the dial.
        let tiles = faces
            .iter()
            .map(|key| format!("clock-face-{key}"))
            .chain(fonts.iter().map(|id| id.to_string()));
        for id in tiles {
            native_reveal(&app, &mut native, &id);
            let tile = native.bounds(app.settings_view(), &id);
            let picture = native.bounds(app.settings_view(), &format!("{id}-picture"));
            assert!(
                picture.x >= tile.x
                    && picture.y >= tile.y
                    && picture.x + picture.width <= tile.x + tile.width + 0.5
                    && picture.y + picture.height <= tile.y + tile.height + 0.5,
                "{id} at {width}: {picture:?} in {tile:?}"
            );
            let pixels = native.render(app.settings_view(), &cosmic::Theme::dark());
            let shades = |y: f32| {
                let y = y.round() as usize;
                let mut seen: Vec<&[u8]> = Vec::new();
                for x in picture.x.ceil() as usize..(picture.x + picture.width) as usize {
                    let at = (y * width as usize + x) * 4;
                    let pixel = &pixels[at..at + 4];
                    if !seen.contains(&pixel) {
                        seen.push(pixel);
                    }
                }
                seen.len()
            };
            for (y, place) in [
                (tile.y - 2., "above the tile"),
                (picture.y + picture.height + 2., "under the picture"),
            ] {
                assert!(
                    shades(y) <= 3,
                    "{id} at {width}: the picture spills {place} ({} shades)",
                    shades(y)
                );
            }
            let texts: Vec<_> = native
                .placed_text(app.settings_view())
                .into_iter()
                .filter(|(_, bounds)| tile.contains(bounds.center()))
                .collect();
            let dial = ["analog", "minimal", "numerals"]
                .iter()
                .any(|key| id == format!("clock-face-{key}"));
            for (text, bounds) in &texts {
                let named = bounds.y >= picture.y + picture.height;
                assert!(
                    named || (!dial && picture.contains(bounds.center())),
                    "{id} at {width}: {text:?} at {bounds:?}, picture {picture:?}"
                );
            }
            assert!(
                texts
                    .iter()
                    .any(|(_, bounds)| bounds.y >= picture.y + picture.height),
                "{id} is named under its picture: {texts:?}"
            );
        }
        for (theme, name) in [
            (cosmic::Theme::light(), "light"),
            (cosmic::Theme::dark(), "dark"),
        ] {
            native_reveal(&app, &mut native, "clock-face-gallery");
            native.capture(
                app.settings_view(),
                &theme,
                &format!("clock-page-faces-{width}-{name}"),
            );
            native_reveal(&app, &mut native, "clock-font-gallery");
            native.capture(
                app.settings_view(),
                &theme,
                &format!("clock-page-fonts-{width}-{name}"),
            );
        }
    }

    let mut native = Native::new(Size::new(1280., 800.));
    let saved = || Config::try_load().unwrap();
    for face in ClockFace::ALL {
        native_click(&mut app, &mut native, &format!("clock-face-{}", face.key()));
        assert_eq!(saved().clock.face, Some(face), "{}", app.status);
    }
    // One control matches the look: the switch, which a face of the
    // clock's own turns off, and which brings back the look's whole clock
    // style, face included. The gallery has no second match tile.
    assert!(
        native
            .try_bounds(app.settings_view(), "clock-face-match")
            .is_none()
    );
    native_reveal(&app, &mut native, "clock-follow-style");
    assert!(!native.text_all(app.settings_view(), "Weight").is_empty());
    send_settings(&mut app, Message::ClockFollowStyle(true));
    assert_eq!(
        crate::settings::profiles::LookStyle::of(&saved()).clock,
        app.profiles.applied_style().clock,
        "{}",
        app.status
    );
    assert!(native.text_all(app.settings_view(), "Weight").is_empty());
    native_click(&mut app, &mut native, "clock-face-digital");
    assert!(!native.text_all(app.settings_view(), "Weight").is_empty());
    // A time-of-day layer has no switch, so its tile for the usual face
    // stays.
    let (active, base) = (app.config.moments.active, app.moment_base_clock);
    app.config.moments.active = Some(crate::moments::Phase::Night);
    app.moment_base_clock = false;
    assert!(
        native
            .try_bounds(app.settings_view(), "clock-follow-style")
            .is_none()
    );
    native_reveal(&app, &mut native, "clock-face-match");
    (app.config.moments.active, app.moment_base_clock) = (active, base);

    native_click(&mut app, &mut native, "clock-font-antonio");
    assert_eq!(saved().clock_font.as_deref(), Some("Antonio"));
    // A typeface of its own shows the weights, and the shipped axis covers
    // Light to Bold.
    assert!(!native.text_all(app.settings_view(), "Weight").is_empty());
    for weight in [ClockWeight::Light, ClockWeight::Normal, ClockWeight::Bold] {
        assert!(
            crate::desktop_theme::font_style_available(cosmic::iced::Font {
                family: cosmic::iced::font::Family::Name("Antonio"),
                weight: weight.native(),
                ..cosmic::iced::Font::DEFAULT
            }),
            "{weight:?}"
        );
    }
    native_click(&mut app, &mut native, "clock-font-widgets");
    assert_eq!(saved().clock_font, None);

    native_click(&mut app, &mut native, "clock-font-more");
    assert_eq!(app.font_menu, Some(FontTarget::Clock));
    native_reveal(&app, &mut native, "font-search-clock");
    for (theme, name) in [
        (cosmic::Theme::light(), "light"),
        (cosmic::Theme::dark(), "dark"),
    ] {
        native.capture(
            app.settings_view(),
            &theme,
            &format!("clock-page-font-picker-{name}"),
        );
    }
    assert!(native.duplicate_ids(app.settings_view()).is_empty());
    native_click(&mut app, &mut native, "clock-font-more");
    assert_eq!(app.font_menu, None);
}

/// With the clock part of Daily overview, the Clock page's stage shows
/// Daily overview's clock and date where the overview sits, for every face,
/// rather than an empty picture.
#[test]
fn clock_page_previews_the_daily_overview_clock() {
    if desktop_test_child(
        "clock_page_previews_the_daily_overview_clock",
        DesktopEnv {
            app_config: true,
            fixture: true,
            icons: Icons::Shown,
            ..Default::default()
        },
    ) {
        return;
    }
    use crate::native_test::Native;
    use crate::settings::clock_stage::OVERVIEW_ID;
    use crate::widget_style::resolver::ClockFace;
    let _root = isolated_root();
    Config::save_clock(json!({"show_date": true})).unwrap();
    let (mut app, initial) = Settings::init(Core::default(), Page::Clock);
    settle_settings(&mut app, initial);
    send_settings(&mut app, Message::Widget(Widget::Glance, true));
    assert!(
        app.config.glance.enabled && app.config.glance.show_clock,
        "{}",
        app.status
    );
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = Native::new(size);
    for face in ClockFace::ALL {
        send_settings(&mut app, Message::ClockFace(Some(face)));
        let card = native_reveal(&app, &mut native, OVERVIEW_ID);
        assert!(card.width > 60. && card.height > 20., "{face:?}: {card:?}");
        let with = native.render(app.settings_view(), &cosmic::Theme::dark());
        // The same stage with the overview's clock turned off, in memory only.
        app.config.glance.show_clock = false;
        assert!(
            native
                .try_bounds(app.settings_view(), OVERVIEW_ID)
                .is_none()
        );
        let without = native.render(app.settings_view(), &cosmic::Theme::dark());
        app.config.glance.show_clock = true;
        let width = size.width as usize;
        let drawn = with
            .chunks_exact(4)
            .zip(without.chunks_exact(4))
            .enumerate()
            .filter(|(i, (a, b))| {
                let (x, y) = ((i % width) as f32, (i / width) as f32);
                card.contains(cosmic::iced::Point::new(x, y))
                    && a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 24)
            })
            .count();
        // A thin dial at the stage's scale changes fewer pixels.
        assert!(
            drawn > 60,
            "{face:?}: the overview's clock is drawn ({drawn})"
        );
        assert!(native.overflowing_text(app.settings_view()).is_empty());
        if face == ClockFace::Digital {
            native_reveal(&app, &mut native, "clock-stage");
            for (theme, name) in [
                (cosmic::Theme::light(), "light"),
                (cosmic::Theme::dark(), "dark"),
            ] {
                native.capture(
                    app.settings_view(),
                    &theme,
                    &format!("clock-content-1280-{name}"),
                );
            }
        }
    }
}

/// A drag on the clock size slider, or a run of arrow keys, previews every
/// step at once but saves once when the gesture ends, so one Undo returns
/// to the size before it. Commits that arrive while a save is still running
/// are coalesced into one more save of the latest size, and a step costs
/// the interface thread no file work.
#[test]
fn clock_size_gestures_save_once_off_the_interface_thread() {
    if desktop_test_child(
        "clock_size_gestures_save_once_off_the_interface_thread",
        DesktopEnv {
            app_config: true,
            fixture: true,
            ..Default::default()
        },
    ) {
        return;
    }
    use crate::native_test::Native;
    use cosmic::iced::keyboard::{self, Key, key};
    use cosmic::iced::{Event, Point, mouse};
    let _root = isolated_root();
    let (mut app, initial) = Settings::init(Core::default(), Page::Clock);
    settle_settings(&mut app, initial);
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let saved = || Config::try_load().unwrap().clock.scale;
    assert_eq!(saved(), 100);

    // A pointer drag across the slider.
    let mut native = Native::new(size);
    let track = native_reveal(&app, &mut native, "clock-scale");
    let at = |share: f32| Point::new(track.x + track.width * share, track.center_y());
    let mut steps = 0;
    let mut commits = 0;
    let mut send = |app: &mut Settings, native: &mut Native, event: mouse::Event, point: Point| {
        for message in native
            .event(app.settings_view(), Event::Mouse(event), point)
            .0
        {
            match message {
                Message::ClockScale(_) => steps += 1,
                Message::ClockScaleCommit => commits += 1,
                _ => {}
            }
            send_settings(app, message);
        }
    };
    send(
        &mut app,
        &mut native,
        mouse::Event::CursorMoved { position: at(0.4) },
        at(0.4),
    );
    send(
        &mut app,
        &mut native,
        mouse::Event::ButtonPressed(mouse::Button::Left),
        at(0.4),
    );
    for share in [0.5, 0.6, 0.7, 0.8] {
        send(
            &mut app,
            &mut native,
            mouse::Event::CursorMoved {
                position: at(share),
            },
            at(share),
        );
        // The preview follows the pointer; nothing is written yet.
        assert_eq!(saved(), 100, "a drag sample saved");
    }
    let dragged = app.config.clock.scale;
    assert!(dragged >= 150, "the preview follows the drag: {dragged}");
    assert_eq!(app.clock_scale.saves, 0);
    send(
        &mut app,
        &mut native,
        mouse::Event::ButtonReleased(mouse::Button::Left),
        at(0.8),
    );
    assert!(steps >= 4, "{steps} live steps");
    assert_eq!(commits, 1);
    assert_eq!(app.clock_scale.saves, 1);
    assert_eq!(saved(), dragged);
    assert!(!app.error, "{}", app.status);
    send_settings(&mut app, Message::UndoPreferences);
    assert!(!app.error, "{}", app.status);
    assert_eq!(saved(), 100, "one Undo undoes the whole drag");
    assert_eq!(app.config.clock.scale, 100);

    // Arrow keys: each press steps, the key coming up saves once.
    let reached = (0..80).any(|_| {
        native.operate(
            app.settings_view(),
            Box::new(cosmic::iced::advanced::widget::operation::focusable::focus_next::<()>()),
        );
        native.operate(
            app.settings_view(),
            Box::new(navigation::FocusReveal::default()),
        );
        let slider = native.bounds(app.settings_view(), "clock-scale");
        native
            .focused(app.settings_view())
            .is_some_and(|focus| slider.intersects(&focus))
    });
    assert!(reached, "Tab reaches the clock size slider");
    let right = |pressed: bool| {
        let key = Key::Named(key::Named::ArrowRight);
        let physical_key = key::Physical::Code(key::Code::ArrowRight);
        Event::Keyboard(if pressed {
            keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key,
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::empty(),
                text: None,
                repeat: false,
            }
        } else {
            keyboard::Event::KeyReleased {
                key: key.clone(),
                modified_key: key,
                physical_key,
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::empty(),
            }
        })
    };
    let saves = app.clock_scale.saves;
    for event in [right(true), right(true), right(true), right(false)] {
        let release = matches!(event, Event::Keyboard(keyboard::Event::KeyReleased { .. }));
        if release {
            assert_eq!(app.config.clock.scale, 130, "three steps preview at once");
            assert_eq!(saved(), 100, "a held arrow key saved");
        }
        for message in native
            .event(app.settings_view(), event, Point::new(1., 1.))
            .0
        {
            send_settings(&mut app, message);
        }
    }
    assert_eq!(app.clock_scale.saves, saves + 1);
    assert_eq!(saved(), 130);
    send_settings(&mut app, Message::UndoPreferences);
    assert_eq!(saved(), 100, "one Undo undoes the key presses");

    // Commits while a save runs: the first save is held back, the next two
    // commits wait, and only the latest of them is saved after it.
    let saves = app.clock_scale.saves;
    let _ = app.update(Message::ClockScale(120));
    let first = app.update(Message::ClockScaleCommit);
    for value in [130, 140] {
        let _ = app.update(Message::ClockScale(value));
        let waiting = app.update(Message::ClockScaleCommit);
        settle_settings(&mut app, waiting);
    }
    assert_eq!(saved(), 100);
    assert_eq!(app.config.clock.scale, 140);
    settle_settings(&mut app, first);
    assert_eq!(app.clock_scale.saves, saves + 2, "coalesced into two saves");
    assert_eq!(saved(), 140);
    assert_eq!(app.config.clock.scale, 140);
    assert!(!app.error, "{}", app.status);
    // Reset is one change too.
    send_settings(&mut app, Message::ClockScaleReset);
    assert_eq!(saved(), 100);
    assert_eq!(app.clock_scale.saves, saves + 3);

    // Interface thread time per drag sample: a step now, against a clock
    // edit that still saves and refreshes on the interface thread, as each
    // size sample used to.
    let samples = 20u32;
    let started = std::time::Instant::now();
    for i in 0..samples {
        let _ = app.update(Message::ClockScale(60 + (i as u16 % 10) * 10));
    }
    let step = started.elapsed() / samples;
    let started = std::time::Instant::now();
    for i in 0..samples {
        let alignment = if i % 2 == 0 {
            crate::config::ClockAlignment::Center
        } else {
            crate::config::ClockAlignment::Left
        };
        let _ = app.update(Message::ClockAlignment(alignment));
    }
    let synchronous = started.elapsed() / samples;
    eprintln!(
        "clock size sample on the interface thread: {step:?} now, {synchronous:?} saving in place"
    );
    assert!(step < synchronous, "{step:?} against {synchronous:?}");
}
