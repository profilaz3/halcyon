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
    let faces: Vec<String> = std::iter::once("match".to_string())
        .chain(ClockFace::ALL.iter().map(|face| face.key().to_string()))
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
    native_click(&mut app, &mut native, "clock-face-match");
    assert_eq!(saved().clock.face, None);

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
    assert!(app.config.glance.enabled && app.config.glance.show_clock, "{}", app.status);
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
        assert!(native.try_bounds(app.settings_view(), OVERVIEW_ID).is_none());
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
        assert!(drawn > 60, "{face:?}: the overview's clock is drawn ({drawn})");
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
