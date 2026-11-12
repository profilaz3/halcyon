// Each page holds what it is about: widget behaviour on Widgets, every
// surface's transparency reachable from Glass and transparency, and the
// pictures that aren't installed behind one button on Wallpaper.

/// Captures at both checked sizes, light and dark.
fn structure_captures(app: &Settings, native: &mut crate::native_test::Native, name: &str) {
    for (theme, shade) in [
        (cosmic::Theme::light(), "light"),
        (cosmic::Theme::dark(), "dark"),
    ] {
        native.capture(app.settings_view(), &theme, &format!("{name}-{shade}"));
    }
}

const STRUCTURE_SIZES: [(f32, f32); 2] = [(820., 600.), (1280., 800.)];

/// Widget controls and Slide in live under “How widgets behave” on Widgets;
/// Halcyon › Interface is about this window only, and search follows.
#[test]
fn ia14_widget_behaviour_lives_on_widgets() {
    if desktop_test_child(
        "ia14_widget_behaviour_lives_on_widgets",
        DesktopEnv {
            app_config: true,
            fixture: true,
            ..Default::default()
        },
    ) {
        return;
    }
    let (mut app, initial) = Settings::init(Core::default(), Page::Widgets);
    settle_settings(&mut app, initial);
    for (width, height) in STRUCTURE_SIZES {
        let size = Size::new(width, height);
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        send_settings(&mut app, Message::Page(Page::Widgets));
        native_reveal(&app, &mut native, "widgets-behave");
        for text in ["How widgets behave", "Widget controls", "Slide in"] {
            assert_eq!(
                native.text_all(app.settings_view(), text).len(),
                1,
                "{text}"
            );
        }
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        structure_captures(&app, &mut native, &format!("ia14-widgets-{width}"));

        send_settings(&mut app, Message::Page(Page::General));
        send_settings(
            &mut app,
            Message::AppSettingsPage(crate::settings::view::AppSettingsPage::Interface),
        );
        for text in ["Widget controls", "Slide in", "On the desktop"] {
            assert!(
                native.text_all(app.settings_view(), text).is_empty(),
                "{text}"
            );
        }
        assert_eq!(
            native
                .text_all(app.settings_view(), "Animate settings")
                .len(),
            1
        );
        structure_captures(&app, &mut native, &format!("ia14-interface-{width}"));
    }
    for query in ["widget controls", "edge menu"] {
        let hit = navigation::search(query)
            .into_iter()
            .find(|hit| hit.label == "Widget controls")
            .expect(query);
        assert_eq!((hit.page, hit.anchor()), (Page::Widgets, "widgets-behave"));
    }
}

/// Glass and transparency names the dock’s and the windows’ own controls,
/// and each Open lands on that control’s page.
#[test]
fn ia06_glass_lists_every_surface_and_opens_its_control() {
    if desktop_test_child(
        "ia06_glass_lists_every_surface_and_opens_its_control",
        DesktopEnv {
            app_config: true,
            fixture: true,
            ..Default::default()
        },
    ) {
        return;
    }
    let (mut app, initial) = Settings::init(Core::default(), Page::Appearance);
    settle_settings(&mut app, initial);
    for (width, height) in STRUCTURE_SIZES {
        let size = Size::new(width, height);
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        send_settings(&mut app, Message::Page(Page::Appearance));
        native_reveal(&app, &mut native, "material-open-windows");
        for text in [
            "Other surfaces",
            "Dock › See-through",
            "Windows › Frosted surfaces and blur",
        ] {
            assert_eq!(
                native.text_all(app.settings_view(), text).len(),
                1,
                "{text}"
            );
        }
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        structure_captures(&app, &mut native, &format!("ia06-glass-{width}"));

        native_click(&mut app, &mut native, "material-open-dock");
        assert_eq!(app.page, Page::DesktopDock);
        assert_eq!(app.search_selected, Some(navigation::DOCK_SEE_THROUGH));

        send_settings(&mut app, Message::Page(Page::Appearance));
        native_click(&mut app, &mut native, "material-open-windows");
        assert_eq!(app.page, Page::DesktopWindows);
        assert_eq!(app.search_selected, Some(navigation::WINDOW_BLUR));
        assert!(
            app.system_appearance.effects,
            "the blur control is unfolded"
        );
    }
    for query in ["dock transparency", "window blur"] {
        let hit = navigation::search(query)[0];
        assert!(
            [navigation::DOCK_SEE_THROUGH, navigation::WINDOW_BLUR].contains(&hit),
            "{query}: {}",
            hit.label
        );
    }
}

/// Featured pictures that aren’t installed wait behind “Get more pictures
/// (n)” instead of filling the grid with dimmed tiles, and Icons sits below
/// the grid under a Desktop heading.
#[test]
fn ia13_pictures_not_installed_wait_behind_one_button() {
    if desktop_test_child(
        "ia13_pictures_not_installed_wait_behind_one_button",
        DesktopEnv {
            app_config: true,
            fixture: true,
            ..Default::default()
        },
    ) {
        return;
    }
    let root = isolated_root();
    std::fs::write(root.join("cosmic-root").join("outputs"), "DP-1\n").unwrap();
    let (mut app, initial) = Settings::init(Core::default(), Page::DesktopWallpaper);
    settle_settings(&mut app, initial);
    let drive = |app: &mut Settings| {
        for _ in 0..32 {
            let jobs = app.wallpaper.take_jobs();
            if jobs.is_empty() {
                return;
            }
            for job in jobs {
                app.wallpaper.update(job.run());
            }
        }
    };
    drive(&mut app);
    send_settings(
        &mut app,
        Message::Wallpaper(wallpaper::Message::Tab(wallpaper::Tab::Bundled)),
    );
    drive(&mut app);
    let missing = app
        .wallpaper
        .tiles()
        .iter()
        .filter(|tile| tile.missing)
        .count();
    assert!(
        missing > 0,
        "the fixture has no Featured pictures installed"
    );
    for (width, height) in STRUCTURE_SIZES {
        let size = Size::new(width, height);
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        native_reveal(&app, &mut native, "wallpaper-get-more");
        let texts = native.all_text(app.settings_view());
        let button = texts
            .iter()
            .find(|text| text.starts_with("Get more pictures ("))
            .expect("Get more pictures");
        let hidden: usize = button["Get more pictures (".len()..button.len() - 1]
            .parse()
            .unwrap();
        // Only a picture in use may still show as Not installed.
        let dimmed = native.text_all(app.settings_view(), "Not installed").len();
        assert!(dimmed <= 1, "{dimmed}");
        assert_eq!(hidden + dimmed, missing);
        let icons = native_reveal(&app, &mut native, "desktop-open-desktop-icons");
        let more = native.bounds(app.settings_view(), "wallpaper-get-more");
        assert!(more.y < icons.y, "Icons sits below the pictures");
        assert!(
            native
                .text_all(app.settings_view(), "Desktop")
                .iter()
                .any(|heading| more.y < heading.y && heading.y < icons.y),
            "a Desktop heading between the pictures and Icons"
        );
        native_reveal(&app, &mut native, "wallpaper-get-more");
        structure_captures(&app, &mut native, &format!("ia13-wallpaper-{width}"));
    }
}
