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
/// Where a press in a picture would take the user, if anywhere.
fn picture_destination(message: &Message) -> Option<Page> {
    match message {
        Message::Page(page) | Message::HomePart(page) => Some(*page),
        Message::OpenBar(kind) | Message::DesktopEditor(desktop::Message::Select(kind)) => {
            Some(Page::of_bar(*kind))
        }
        _ => None,
    }
}

/// One press and release at each point of a grid over the picture `id`
/// (the part inside the window), with what it sent and the pointer's shape
/// there. Nothing is applied.
fn sweep_picture(
    app: &Settings,
    native: &mut crate::native_test::Native,
    id: &str,
    step: f32,
) -> Vec<(
    cosmic::iced::Point,
    Vec<Message>,
    cosmic::iced::mouse::Interaction,
)> {
    use cosmic::iced::{Event, Point, mouse};
    let area = native.bounds(app.settings_view(), id);
    let top = area.y.max(0.);
    let bottom = (area.y + area.height).min(native.size.height);
    assert!(bottom - top > 40., "{id} is on screen: {area:?}");
    let mut found = Vec::new();
    let mut y = top + step / 2.;
    while y < bottom {
        let mut x = area.x + step / 2.;
        while x < area.x + area.width {
            let at = Point::new(x, y);
            let pointer = native.pointer(app.settings_view(), at);
            let mut sent = Vec::new();
            for event in [
                mouse::Event::ButtonPressed(mouse::Button::Left),
                mouse::Event::ButtonReleased(mouse::Button::Left),
            ] {
                sent.extend(native.event(app.settings_view(), Event::Mouse(event), at).0);
            }
            found.push((at, sent, pointer));
            x += step;
        }
        y += step;
    }
    found
}

/// Keyboard focus stops that sit inside the picture `id`.
fn picture_stops(
    app: &Settings,
    native: &mut crate::native_test::Native,
    id: &str,
) -> Vec<cosmic::iced::Rectangle> {
    let area = native.bounds(app.settings_view(), id);
    native
        .focus_stops(app.settings_view())
        .into_iter()
        .filter(|bounds| bounds.width > 0. && area.contains(bounds.center()))
        .collect()
}

/// On `page`, no press anywhere in its picture leads to another menu, and
/// the pointer only turns into a hand over parts that answer a press.
fn assert_picture_stays(
    app: &mut Settings,
    native: &mut crate::native_test::Native,
    page: Page,
    id: &str,
) -> Vec<Page> {
    use cosmic::iced::mouse::Interaction;
    send_settings(app, Message::Page(page));
    assert_eq!(app.page, page);
    let mut own = Vec::new();
    for (at, sent, pointer) in sweep_picture(app, native, id, 14.) {
        for message in &sent {
            if let Some(to) = picture_destination(message) {
                assert!(
                    page.owns(to),
                    "{page:?}: a press at {at:?} in its preview opens {to:?}: {message:?}"
                );
                own.push(to);
            }
        }
        if pointer == Interaction::Pointer {
            assert!(
                !sent.is_empty(),
                "{page:?}: the pointer is a hand at {at:?} over a part that does nothing"
            );
        }
    }
    // A press anywhere in it leaves the page where it was.
    assert_eq!(app.page, page);
    own
}

// A page's preview is for the page's own thing. Parts another sidebar page
// edits (the wallpaper, bars, the window, the island, widgets) are drawn but
// take no press, hover, focus or button role; Home's picture still leads
// everywhere.
#[test]
fn previews_only_act_on_their_own_page() {
    use crate::desktop::panels::{Anchor, PanelKind};
    use cosmic::iced::{Event, Point, mouse};
    let name = "previews_only_act_on_their_own_page";
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
    app.config.rain.enabled = true;
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    let theme = cosmic::Theme::dark();

    // Windows and Activity island: the small Home picture lights the page's
    // part and nothing in it can be pressed, hovered or focused.
    for page in [Page::DesktopWindows, Page::ActivityIsland] {
        let id = format!("home-glance-{}", page.key());
        let own = assert_picture_stays(&mut app, &mut native, page, &id);
        assert!(
            own.is_empty(),
            "{page:?}: the glance leads nowhere: {own:?}"
        );
        assert!(
            picture_stops(&app, &mut native, &id).is_empty(),
            "{page:?}: nothing in the glance takes focus"
        );
        for part in super::view::home_picture::PARTS {
            assert!(
                native
                    .try_bounds(
                        app.settings_view(),
                        &super::view::home_picture::part_id(part)
                    )
                    .is_none(),
                "{page:?}: {part:?} is not a focus stop"
            );
        }
        // Hovering the wallpaper shows nothing.
        let glance = native.bounds(app.settings_view(), &id);
        let at = Point::new(glance.x + glance.width * 0.12, glance.center_y());
        assert_eq!(
            native.pointer(app.settings_view(), at),
            mouse::Interaction::None
        );
        native.capture_hover(
            app.settings_view(),
            &theme,
            at,
            &format!("preview-scope-{}-hover-wallpaper", page.key()),
        );
    }

    // Appearance: the sample window opens its own subpage; the wallpaper,
    // bars and widgets are drawn only.
    let own = assert_picture_stays(&mut app, &mut native, Page::Themes, "desktop-scene");
    assert!(
        own.contains(&Page::DesktopAppearance),
        "the sample window still opens Fonts and icons: {own:?}"
    );
    let stops = picture_stops(&app, &mut native, "desktop-scene");
    let window = native.bounds(app.settings_view(), "scene-part-window");
    assert_eq!(stops, vec![window], "only the window takes focus");
    let scene = native.bounds(app.settings_view(), "desktop-scene");
    let corner = Point::new(scene.x + scene.width * 0.08, scene.y + scene.height * 0.55);
    assert_eq!(
        native.pointer(app.settings_view(), corner),
        mouse::Interaction::None
    );
    native.capture_hover(
        app.settings_view(),
        &theme,
        corner,
        "preview-scope-themes-hover-wallpaper",
    );
    native_click(&mut app, &mut native, "scene-part-window");
    assert_eq!(app.page, Page::DesktopAppearance);

    // Widgets: each widget opens its page; the wallpaper and bars don't.
    let own = assert_picture_stays(&mut app, &mut native, Page::Overview, "desktop-scene");
    assert!(
        own.iter().any(|page| page.sidebar() == Page::Overview),
        "widgets still open their pages: {own:?}"
    );
    let scene = native.bounds(app.settings_view(), "desktop-scene");
    let corner = Point::new(scene.x + scene.width * 0.5, scene.y + scene.height * 0.55);
    native.capture_hover(
        app.settings_view(),
        &theme,
        corner,
        "preview-scope-overview-hover-wallpaper",
    );
    native_click(&mut app, &mut native, "scene-widget-clock");
    assert_eq!(app.page, Page::Clock);

    // Panel: with the dock on the same edge the close-up shows it faintly;
    // it is the Dock page's, so here it is drawn only.
    for kind in [PanelKind::Dock, PanelKind::Panel] {
        send_settings(&mut app, Message::OpenBar(kind));
        send_settings(
            &mut app,
            Message::DesktopEditor(desktop::Message::Anchor(Anchor::Top)),
        );
    }
    assert_picture_stays(&mut app, &mut native, Page::DesktopPanel, "desktop-scene");
    assert_eq!(app.desktop.selected, PanelKind::Panel);
    assert_picture_stays(&mut app, &mut native, Page::DesktopDock, "desktop-scene");
    assert_eq!(app.desktop.selected, PanelKind::Dock);

    // Home is the map: its picture still picks and opens every part.
    send_settings(&mut app, Message::Page(Page::Home));
    let sweep = sweep_picture(&app, &mut native, "home-picture", 14.);
    let picks = |part: Page| {
        sweep
            .iter()
            .filter(|(_, sent, _)| {
                sent.iter()
                    .any(|message| matches!(message, Message::HomePart(to) if *to == part))
            })
            .map(|(at, _, _)| *at)
            .collect::<Vec<_>>()
    };
    for part in [
        Page::DesktopWallpaper,
        Page::DesktopPanel,
        Page::DesktopWindows,
        Page::ActivityIsland,
    ] {
        assert!(!picks(part).is_empty(), "Home picks {part:?}");
    }
    // A press on the wallpaper picks it; a second one opens its page.
    let wallpaper = picks(Page::DesktopWallpaper)[0];
    for _ in 0..2 {
        for event in [
            mouse::Event::CursorMoved {
                position: wallpaper,
            },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            for message in native
                .event(app.settings_view(), Event::Mouse(event), wallpaper)
                .0
            {
                send_settings(&mut app, message);
            }
        }
    }
    assert_eq!(app.page, Page::DesktopWallpaper);
}
