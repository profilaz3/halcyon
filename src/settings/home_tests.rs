// Included inside settings::tests to share the isolated desktop/native harness.

/// The Activity island on the fixture's panel, so the picture shows it.
fn seed_island(root: &std::path::Path) {
    seed(
        &root.join("cosmic-root/config/cosmic"),
        &[(
            "com.system76.CosmicPanel.Panel",
            "plugins_center",
            "Some([\"io.github.profilaz3.Halcyon.Activity\"])",
        )],
    );
}

const HOME_SIZES: [Size; 2] = [Size::new(820., 600.), Size::new(1280., 800.)];

fn home_themes() -> [(&'static str, cosmic::Theme); 2] {
    [
        ("light", cosmic::Theme::light()),
        ("dark", cosmic::Theme::dark()),
    ]
}

fn size_name(size: Size) -> String {
    format!("{}x{}", size.width as u32, size.height as u32)
}

/// Clicks the window point `at`, as a pointer would.
fn home_click_at(app: &mut Settings, native: &mut crate::native_test::Native, at: cosmic::iced::Point) {
    use cosmic::iced::{Event, mouse};
    for event in [
        mouse::Event::CursorMoved { position: at },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        for message in native.event(app.settings_view(), Event::Mouse(event), at).0 {
            send_settings(app, message);
        }
    }
}

// Home: the picture shows every part where it is, a click picks one and a
// second click opens its page. Captured in light and dark at both sizes.
#[test]
fn home_picture_picks_parts_and_opens_their_pages() {
    use crate::desktop::panels::PanelKind;
    use crate::settings::view::home_picture::PARTS;
    let name = "home_picture_picks_parts_and_opens_their_pages";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            fixture: true,
            icons: Icons::Shown,
            ..Default::default()
        },
    ) {
        return;
    }
    let root = isolated_root();
    seed_island(&root);
    let mut app = open_isolated_editor(&root);
    send_settings(&mut app, Message::DesktopEditor(desktop::Message::Refresh));
    // Effects on, so the picture shows them too.
    app.config.rain.enabled = true;
    send_settings(&mut app, Message::Page(Page::Home));
    assert_eq!(app.page, Page::Home);
    for size in HOME_SIZES {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        let tag = size_name(size);
        // Every part in the picture is on screen.
        for page in [
            Page::DesktopWallpaper,
            Page::DesktopPanel,
            Page::DesktopDock,
            Page::ActivityIsland,
            Page::DesktopWindows,
            Page::WeatherEffects,
        ] {
            let id = format!("home-part-{}", page.key());
            let b = native.bounds(app.settings_view(), &id);
            assert!(
                b.width > 4. && b.height > 4. && b.y + b.height <= size.height,
                "{tag} {id}: {b:?}"
            );
        }
        // So is one tile per sidebar entry, inside the window's width.
        for page in PARTS {
            let id = format!("home-tile-{}", page.key());
            let b = native.bounds(app.settings_view(), &id);
            assert!(b.width > 40. && b.x + b.width <= size.width, "{tag} {id}: {b:?}");
        }
        let text = native.all_text(app.settings_view());
        assert!(text.iter().any(|t| t == "Pick a part of the picture, or a tile below."));
        for (theme_name, theme) in home_themes() {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("home-{tag}-{theme_name}-none"),
            );
        }
        for part in [
            Page::DesktopPanel,
            Page::DesktopDock,
            Page::Overview,
            Page::WeatherEffects,
            Page::Themes,
        ] {
            send_settings(&mut app, Message::HomePart(part));
            assert_eq!(app.page, Page::Home, "picking a part stays on Home");
            assert_eq!(app.home_part, Some(part));
            // The row says what it is and how it is set now.
            let text = native.all_text(app.settings_view());
            assert!(text.iter().any(|t| t == part.nav_title()), "{part:?}: {text:?}");
            assert!(text.iter().any(|t| *t == app.part_state(part)), "{part:?}: {text:?}");
            for (theme_name, theme) in home_themes() {
                native.capture(
                    app.settings_view(),
                    &theme,
                    &format!("home-{tag}-{theme_name}-{}", part.key()),
                );
            }
            app.home_part = None;
        }
    }

    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    // A click picks the panel; a second click opens its page. Its middle
    // holds the island, so the click lands near its start.
    let panel = native_reveal(&app, &mut native, "home-part-desktop-panel");
    let start = cosmic::iced::Point::new(panel.x + panel.width * 0.1, panel.center_y());
    home_click_at(&mut app, &mut native, start);
    assert_eq!((app.page, app.home_part), (Page::Home, Some(Page::DesktopPanel)));
    home_click_at(&mut app, &mut native, start);
    assert_eq!(app.page, Page::DesktopPanel);
    assert_eq!(app.desktop.selected, PanelKind::Panel);
    // Coming back starts with nothing picked.
    send_settings(&mut app, Message::Page(Page::Home));
    assert_eq!(app.home_part, None);
    // Empty wallpaper picks Wallpaper.
    let picture = native.bounds(app.settings_view(), "home-part-desktop-wallpaper");
    home_click_at(
        &mut app,
        &mut native,
        cosmic::iced::Point::new(
            picture.x + picture.width * 0.3,
            picture.y + picture.height * 0.8,
        ),
    );
    assert_eq!(app.home_part, Some(Page::DesktopWallpaper));
    // A tile picks its part, even one the picture has no place for, and
    // Open goes there.
    native_click(&mut app, &mut native, "home-tile-desktop-motion");
    assert_eq!((app.page, app.home_part), (Page::Home, Some(Page::DesktopMotion)));
    native_click(&mut app, &mut native, "home-open");
    assert_eq!(app.page, Page::DesktopMotion);
    // The dock opens on its own bar.
    send_settings(&mut app, Message::Page(Page::Home));
    native_click(&mut app, &mut native, "home-tile-desktop-dock");
    native_click(&mut app, &mut native, "home-tile-desktop-dock");
    assert_eq!(app.page, Page::DesktopDock);
    assert_eq!(app.desktop.selected, PanelKind::Dock);
}

// Panel, Dock and Activity island: three sidebar entries, each opening on its
// own part. Captured in light and dark at both sizes, with the narrow rail.
#[test]
fn panel_dock_and_island_pages_open_from_the_sidebar() {
    use crate::desktop::panels::PanelKind;
    let name = "panel_dock_and_island_pages_open_from_the_sidebar";
    if desktop_test_child(
        name,
        DesktopEnv {
            app_config: true,
            fixture: true,
            icons: Icons::Shown,
            ..Default::default()
        },
    ) {
        return;
    }
    let root = isolated_root();
    seed_island(&root);
    let mut app = open_isolated_editor(&root);
    send_settings(&mut app, Message::DesktopEditor(desktop::Message::Refresh));
    for size in HOME_SIZES {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        let tag = size_name(size);
        for (page, kind) in [
            (Page::DesktopPanel, PanelKind::Panel),
            (Page::DesktopDock, PanelKind::Dock),
            (Page::ActivityIsland, PanelKind::Panel),
        ] {
            send_settings(&mut app, Message::Page(page));
            assert_eq!(app.page, page);
            assert_eq!(app.desktop.selected, kind, "{page:?}");
            if page == Page::ActivityIsland {
                // Its options moved here from the panel's contents.
                native_reveal(&app, &mut native, "activity-preference-style");
                native.bounds(app.settings_view(), "home-glance-activity-island");
            } else {
                // One bar per page: no switcher between them.
                let text = native.all_text(app.settings_view());
                assert!(
                    !text.iter().any(|t| t == "Dock and panel"),
                    "{page:?}: {text:?}"
                );
            }
            for (theme_name, theme) in home_themes() {
                native.capture(
                    app.settings_view(),
                    &theme,
                    &format!("page-{}-{tag}-{theme_name}", page.key()),
                );
            }
        }
    }
    // The narrow rail names every entry in short.
    send_settings(&mut app, Message::Resized(Size::new(820., 600.)));
    send_settings(&mut app, Message::Page(Page::Home));
    assert!(app.rail());
    let mut native = crate::native_test::Native::new(Size::new(820., 600.));
    let text = native.all_text(app.settings_view());
    for title in ["Home", "Panel", "Dock", "Island", "Effects"] {
        assert!(text.iter().any(|t| t == title), "{title}: {text:?}");
    }
    for (theme_name, theme) in home_themes() {
        native.capture(
            app.settings_view(),
            &theme,
            &format!("rail-820x600-{theme_name}"),
        );
    }
    // The panel's contents keep a row that leads to the island's page.
    send_settings(&mut app, Message::Page(Page::DesktopPanel));
    send_settings(
        &mut app,
        Message::ObjectTab(crate::settings::navigation::ObjectTab::Content),
    );
    native_click(&mut app, &mut native, "panel-open-island");
    assert_eq!(app.page, Page::ActivityIsland);
}
