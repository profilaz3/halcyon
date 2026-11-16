// Included inside settings::tests to share the isolated desktop/native harness.
// Navigation and names: Back goes where Alt+← goes, links open the setting
// they name, explanations keep live state and page intros in view, each
// feature has one name, and the first controls fit an 820 × 600 window.

/// The header's Back link title, if it has one.
fn back_link(app: &Settings) -> Option<String> {
    app.up_link().map(|(title, _)| title)
}

/// Back names the page Alt+← returns to and goes there; with nothing
/// before, it names where the page sits. A sidebar page opened from the
/// sidebar has none, but reached from a link it leads back. Accessibility's
/// hide timing link opens the dock's fold with the setting ringed.
#[test]
fn back_goes_where_alt_left_goes_and_links_open_their_setting() {
    let name = "back_goes_where_alt_left_goes_and_links_open_their_setting";
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
    let _root = isolated_root();
    // Opened directly, a subpage leads up to where it sits.
    let (mut app, task) = Settings::init(Core::default(), Page::Clock);
    settle_settings(&mut app, task);
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    assert!(app.history.is_empty());
    assert_eq!(back_link(&app).as_deref(), Some("Widgets"));
    assert!(matches!(app.up_link(), Some((_, Message::Back))));
    send_settings(&mut app, Message::Back);
    assert_eq!(app.page, Page::Overview);

    // From the sidebar: no Back link on the sidebar page itself.
    send_settings(&mut app, Message::OpenRoot(Page::DesktopMotion));
    assert_eq!(back_link(&app), None);
    // A subpage opened from there names Accessibility, not its parent,
    // and Back and Alt+← both return there.
    send_settings(&mut app, Message::Page(Page::Appearance));
    assert_eq!(Page::Appearance.parent(), Some(Page::Themes));
    assert_eq!(back_link(&app).as_deref(), Some("Accessibility"));
    assert_eq!(app.back_title(), back_link(&app));
    let mut native = crate::native_test::Native::new(size);
    native_click(&mut app, &mut native, "settings-back");
    assert_eq!(app.page, Page::DesktopMotion);

    // The link names the setting and opens it: the dock's folded section,
    // with Hide timing ringed.
    native_reveal(&app, &mut native, "accessibility-dock");
    native.text_bounds(app.settings_view(), "Dock and panel hide timing");
    native_click(&mut app, &mut native, "accessibility-dock");
    assert_eq!(app.page, Page::DesktopDock);
    assert_eq!(
        app.desktop.selected,
        crate::desktop::panels::PanelKind::Dock
    );
    assert!(app.desktop.advanced, "the folded section is open");
    assert_eq!(
        app.search_selected.map(|hit| hit.anchor()),
        Some("dock-timing")
    );
    native_reveal(&app, &mut native, "dock-timing");
    native.text_bounds(app.settings_view(), "Hide timing");
    // A sidebar page reached by a link leads back to the link.
    assert_eq!(back_link(&app).as_deref(), Some("Accessibility"));
    native_click(&mut app, &mut native, "settings-back");
    assert_eq!(app.page, Page::DesktopMotion);
    // Alt+← does the same.
    native_click(&mut app, &mut native, "accessibility-dock");
    assert_eq!(app.page, Page::DesktopDock);
    send_settings(&mut app, Message::Back);
    assert_eq!(app.page, Page::DesktopMotion);
    // What's new, opened on its own, goes up to Updates.
    let (mut app, task) = Settings::init(Core::default(), Page::General);
    settle_settings(&mut app, task);
    app.app_settings_page = crate::settings::view::AppSettingsPage::WhatsNew;
    assert_eq!(back_link(&app).as_deref(), Some("Updates"));
    send_settings(&mut app, Message::Back);
    assert_eq!(
        app.app_settings_page,
        crate::settings::view::AppSettingsPage::Updates
    );
}

/// Page intros and live state are on screen without "?"; the "?" is easy
/// to point at, and Show explanations is on Accessibility too.
#[test]
fn explanations_keep_intros_and_state_in_view() {
    let name = "explanations_keep_intros_and_state_in_view";
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
    let mut app = open_isolated_editor(&root);
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    assert!(!app.config.show_explanations, "the default stays off");
    let mut native = crate::native_test::Native::new(size);
    send_settings(&mut app, Message::OpenRoot(Page::DesktopMotion));
    native.text_bounds(app.settings_view(), "Calmer motion and easier reading.");
    // The "?" is at least 24 px square.
    let key = view::explain::key("Easier-to-read widgets");
    let mark = native.bounds(app.settings_view(), &format!("explain-{key:016x}"));
    assert!(
        mark.width >= view::explain::MARK && mark.height >= view::explain::MARK,
        "{mark:?}"
    );
    // Show explanations from Accessibility › Reading.
    native_click(&mut app, &mut native, "accessibility-explanations-toggle");
    assert!(app.config.show_explanations);
    assert!(Config::load().show_explanations);
    native.text_bounds(
        app.settings_view(),
        "Stronger text contrast on widget cards",
    );
    send_settings(&mut app, Message::ShowExplanations(false));
    assert!(!Config::load().show_explanations);
    native_reveal(&app, &mut native, "accessibility-open-shortcuts");
    native.capture(
        app.settings_view(),
        &cosmic::Theme::dark(),
        "accessibility-reading-keyboard",
    );
    // Keyboard: the shortcuts page, then the keys of this window.
    native.text_bounds(app.settings_view(), "Keys in this window");
    native_click(&mut app, &mut native, "accessibility-open-shortcuts");
    assert_eq!(app.page, Page::DesktopShortcuts);
    assert_eq!(back_link(&app).as_deref(), Some("Accessibility"));

    // The wallpaper kept from before Halcyon says so, like the desktop row.
    send_settings(&mut app, Message::OpenRoot(Page::Changes));
    app.desktop_recovery.wallpaper_backup = true;
    native_reveal(&app, &mut native, "restore-wallpaper");
    native.text_bounds(
        app.settings_view(),
        "The picture you had when Halcyon was installed",
    );

    // The Edge menu says why Open now is the way in while the strip is off.
    send_settings(&mut app, Message::Page(Page::EdgeMenu));
    send_settings(
        &mut app,
        Message::EdgeMenu(crate::settings::edge_menu::Edit::Enabled(false)),
    );
    native.text_bounds(
        app.settings_view(),
        "The edge strip is off. Use Open now, or run halcyon --toggle drawer.",
    );
}

/// The edge menu has one name and one page: Widgets has a single row that
/// opens it, Open now is on that page, and its animation is set only there.
#[test]
fn edge_menu_has_one_name_and_one_page() {
    let name = "edge_menu_has_one_name_and_one_page";
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
    let mut app = open_isolated_editor(&root);
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    send_settings(&mut app, Message::OpenRoot(Page::Overview));
    native_reveal(&app, &mut native, "widgets-edge-menu");
    for gone in ["Widget controls", "Slide in", "Open now"] {
        assert!(
            native.text_all(app.settings_view(), gone).is_empty(),
            "{gone}"
        );
    }
    native_click(&mut app, &mut native, "widgets-edge-menu");
    assert_eq!(app.page, Page::EdgeMenu);
    assert_eq!(back_link(&app).as_deref(), Some("Widgets"));
    native.text_bounds(app.settings_view(), "Open now");
    native_reveal(&app, &mut native, "edge-menu-behavior");
    native.text_bounds(app.settings_view(), "Animation");
    assert!(native.text_all(app.settings_view(), "Show menu").is_empty());
}

/// Move or resize opens Arrange widgets on the desktop with the widget
/// picked; the Clock page has it too. Where the desktop can't, the page's
/// own canvas opens. Both editors use the same words.
#[test]
fn move_or_resize_opens_the_desktop_editor_with_the_widget_picked() {
    let name = "move_or_resize_opens_the_desktop_editor_with_the_widget_picked";
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
    let _root = isolated_root();
    Config::save_changes(serde_json::json!({
        "enabled_widgets": ["clock", "weather"],
        "glance": {"enabled": false},
        "music": {"enabled": false}
    }))
    .unwrap();
    let (mut app, task) = Settings::init(Core::default(), Page::Clock);
    settle_settings(&mut app, task);
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    assert!(desktop_arrange_available(&app.config));
    native_reveal(&app, &mut native, "clock-arrange");
    // Its explanation says where it opens.
    send_settings(
        &mut app,
        Message::Explain(view::explain::key("Move or resize")),
    );
    native.text_bounds(
        app.settings_view(),
        "Opens Arrange widgets on your desktop, with this widget picked",
    );
    native.bounds(app.settings_view(), "clock-stage");
    // The request names the widget; Settings opens no editor of its own.
    let task = app.update(Message::ArrangeWidget(Widget::Clock));
    assert!(app.busy && app.canvas.is_none(), "{}", app.status);
    settle_settings(&mut app, task);
    let command: serde_json::Value =
        serde_json::from_slice(&std::fs::read(desktop_command_path().unwrap()).unwrap()).unwrap();
    assert_eq!(command["action"], "arrange");
    assert_eq!(command["widget"], "clock");
    // The desktop couldn't take it: this page's canvas, with Clock picked.
    send_settings(
        &mut app,
        Message::ArrangeWidgetHere(Widget::Clock, "no desktop".into()),
    );
    assert_eq!(app.page, Page::Clock);
    assert!(app.canvas_visible());
    assert_eq!(app.placement.selected(), Some(Widget::Clock));
    native.text_bounds(app.settings_view(), "Fit content");
    native.text_bounds(app.settings_view(), "Resize with arrows");
    send_settings(&mut app, Message::Canvas(canvas::Message::Cancel));
    // The Widgets header says where each editor opens.
    send_settings(&mut app, Message::OpenRoot(Page::Overview));
    native.text_bounds(app.settings_view(), "Arrange in this window");
    assert!(
        native
            .text_all(app.settings_view(), "Arrange with keyboard")
            .is_empty()
    );
    // A widget that is off says how to show it first.
    send_settings(&mut app, Message::Page(Page::Music));
    native_reveal(&app, &mut native, "music-show");
    native.text_bounds(app.settings_view(), "Show it on your desktop first");
}

/// The applet is the Activity island everywhere, and the panel's Islands
/// shape says it is something else.
#[test]
fn activity_island_has_one_name_apart_from_the_islands_shape() {
    let name = "activity_island_has_one_name_apart_from_the_islands_shape";
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
    let packaged = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/packaging/io.github.profilaz3.Halcyon.Activity.desktop"
    ))
    .unwrap();
    assert!(
        packaged
            .lines()
            .any(|line| line == "Name=Activity island (beta)")
    );
    assert!(!packaged.contains("pill"));
    let root = isolated_root();
    // The packaged applet, installed where the fixture's panel finds applets.
    let fixture = open_isolated_editor(&root)
        .desktop
        .store()
        .unwrap()
        .fixture_root()
        .unwrap()
        .to_path_buf();
    let applications = fixture.join("applications");
    std::fs::create_dir_all(&applications).unwrap();
    std::fs::write(
        applications.join(format!("{}.desktop", crate::activity_island::APPLET_ID)),
        &packaged,
    )
    .unwrap();
    let mut app = looks_desktop(&root);
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    send_settings(&mut app, Message::Page(Page::ActivityIsland));
    native_reveal(&app, &mut native, "panel-add-activity");
    native.text_bounds(app.settings_view(), "Add Activity island");
    assert!(
        native
            .text_all(app.settings_view(), "Add Activity pill")
            .is_empty()
    );
    send_settings(
        &mut app,
        Message::OpenBar(crate::desktop::panels::PanelKind::Panel),
    );
    native_reveal(&app, &mut native, "native-islands-style");
    native.text_bounds(
        app.settings_view(),
        "Islands splits the panel into floating pieces. It is not the Activity island, which is an item you add to the panel.",
    );
}

/// At 820 × 600 every picture leaves the page's first controls in view:
/// Windows shows only its own picture, and no picture is taller than a
/// third of the window.
#[test]
fn previews_leave_the_first_controls_in_view() {
    let name = "previews_leave_the_first_controls_in_view";
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
    let mut app = open_isolated_editor(&root);
    let size = Size::new(820., 600.);
    send_settings(&mut app, Message::Resized(size));
    assert_eq!(app.preview_cap(), 200.);
    for (page, picture, first) in [
        (Page::Overview, "desktop-scene", "widgets-show-all"),
        (
            Page::DesktopWindows,
            "desktop-scene",
            "windows-open-shortcuts",
        ),
    ] {
        send_settings(&mut app, Message::OpenRoot(page));
        let mut native = crate::native_test::Native::new(size);
        native.scroll(app.settings_view(), "settings-content", 0.);
        let viewport = native.bounds(app.settings_view(), "settings-viewport");
        let scene = native.bounds(app.settings_view(), picture);
        assert!(scene.height <= size.height / 3. + 1., "{page:?}: {scene:?}");
        let control = native.bounds(app.settings_view(), first);
        assert!(
            control.y + control.height <= viewport.y + viewport.height,
            "{page:?}: {first} {control:?} below {viewport:?}"
        );
        for (theme, shade) in [
            (cosmic::Theme::light(), "light"),
            (cosmic::Theme::dark(), "dark"),
        ] {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("first-controls-820-{}-{shade}", page.key()),
            );
        }
    }
    // Windows: no Home picture above its own.
    let mut native = crate::native_test::Native::new(size);
    assert!(
        native
            .try_bounds(app.settings_view(), "home-glance-desktop-windows")
            .is_none()
    );
}

/// Home, setup and the bar pages use the same word for each choice.
#[test]
fn home_and_setup_use_the_pages_words() {
    let name = "home_and_setup_use_the_pages_words";
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
    use crate::desktop::panels::{AutoHide, PanelKind};
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    send_settings(&mut app, Message::OpenBar(PanelKind::Dock));
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    // Home says the shape as Shape on the bar's page does.
    for kind in [PanelKind::Dock, PanelKind::Panel] {
        let state = app.part_state(Page::of_bar(kind));
        assert!(
            ["Full width", "Full height", "Compact", "Islands"]
                .iter()
                .any(|word| state.contains(word)),
            "{kind:?}: {state}"
        );
        assert!(!state.contains("edge to edge") && !state.contains("centered"));
    }
    assert_eq!(AutoHide::OnOverlap.title(), "Hide if covered");
    let mut native = crate::native_test::Native::new(size);
    send_settings(&mut app, Message::OpenBar(PanelKind::Dock));
    native_reveal(&app, &mut native, "dock-visibility");
    native.text_bounds(app.settings_view(), "Hide if covered");
    native.capture(
        app.settings_view(),
        &cosmic::Theme::dark(),
        "dock-visibility-words",
    );
    assert!(
        !native
            .text_all(app.settings_view(), "This restarts the dock")
            .is_empty()
    );
    assert!(
        native
            .text_all(app.settings_view(), "This restarts the panel")
            .is_empty()
    );
    send_settings(&mut app, Message::OpenBar(PanelKind::Panel));
    assert!(
        !native
            .text_all(app.settings_view(), "This restarts the panel")
            .is_empty()
    );
    // Uptime is called what the card calls it.
    assert_eq!(crate::config::DevboxSection::Uptime.title(), "Uptime");
    // Setup's bars and widgets speak the same words.
    send_settings(&mut app, Message::OpenWizard);
    for step in [
        wizard::Message::Start,
        wizard::Message::Look("calm"),
        wizard::Message::Next,
    ] {
        send_settings(&mut app, Message::Wizard(step));
    }
    assert_eq!(app.wizard.as_ref().unwrap().step, wizard::Step::Bars);
    send_settings(&mut app, Message::Wizard(wizard::Message::KeepBars(false)));
    let text = native.all_text(app.settings_view());
    for word in ["Shape", "Floating", "Hide if covered"] {
        assert!(text.iter().any(|t| t == word), "{word}: {text:?}");
    }
    for old in ["Style", "Bar", "On the edge", "Always shown", "Hides"] {
        assert!(!text.iter().any(|t| t == old), "{old}: {text:?}");
    }
    let small = Size::new(820., 600.);
    send_settings(&mut app, Message::Resized(small));
    crate::native_test::Native::new(small).capture(
        app.settings_view(),
        &cosmic::Theme::dark(),
        "setup-bars-words-820",
    );
    send_settings(&mut app, Message::Resized(size));
    send_settings(&mut app, Message::Wizard(wizard::Message::Next));
    assert_eq!(app.wizard.as_ref().unwrap().step, wizard::Step::Widgets);
    let text = native.all_text(app.settings_view());
    assert!(text.iter().any(|t| t == "Daily"), "{text:?}");
    assert!(!text.iter().any(|t| t == "Everyday"), "{text:?}");
}
