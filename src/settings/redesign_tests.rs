// Included inside settings::tests to share the isolated desktop/native harness.
#[test]
fn penumbra_control_is_separate_reachable_and_uses_the_normal_action_path() {
    let name = "penumbra_control_is_separate_reachable_and_uses_the_normal_action_path";
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
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Select(
            crate::desktop::panels::PanelKind::Panel,
        )),
    );
    let before = app.desktop.current().unwrap().islands();
    for size in [Size::new(1280., 900.), Size::new(820., 600.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        app.desktop.penumbra_supported = false;
        assert!(
            native
                .try_bounds(app.settings_view(), "penumbra-fade")
                .is_none()
        );
        assert!(
            native
                .try_bounds(app.settings_view(), "penumbra-maximized-opacity")
                .is_none()
        );
        let _ = app.update(Message::ObjectTab(
            crate::settings::navigation::ObjectTab::Design,
        ));
        // Without host support and while off, the row explains what it needs
        // instead of offering a switch that would do nothing.
        assert!(
            native
                .try_bounds(app.settings_view(), "penumbra-gradient")
                .is_none()
        );
        let bounds = native_reveal(&app, &mut native, "penumbra-controls");
        assert!(bounds.x >= 0. && bounds.x + bounds.width <= size.width);
        assert!(bounds.y >= 0. && bounds.y + bounds.height <= size.height);
        app.desktop.penumbra_supported = true;
        let bounds = native_reveal(&app, &mut native, "penumbra-gradient");
        assert!(bounds.x >= 0. && bounds.x + bounds.width <= size.width);
        assert!(bounds.y >= 0. && bounds.y + bounds.height <= size.height);
        native_send(
            &mut app,
            &mut native,
            Message::DesktopEditor(desktop::Message::Penumbra(true)),
        );
        assert!(!app.desktop.dirty());
        assert!(
            app.desktop
                .current()
                .unwrap()
                .penumbra()
                .known()
                .unwrap()
                .unwrap()
                .enabled
        );
        assert_eq!(app.desktop.current().unwrap().islands(), before);
        // The fixture supplies the optional host capability for visual evidence.
        app.desktop.penumbra_supported = true;
        native_reveal(&app, &mut native, "penumbra-controls");
        native_reveal(&app, &mut native, "penumbra-fade");
        native_reveal(&app, &mut native, "penumbra-maximized-opacity");
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("penumbra-{}", size.width),
        );
        native_send(
            &mut app,
            &mut native,
            Message::DesktopEditor(desktop::Message::Penumbra(false)),
        );
        assert!(
            !app.desktop
                .current()
                .unwrap()
                .penumbra()
                .known()
                .unwrap()
                .unwrap()
                .enabled
        );
        assert_eq!(app.desktop.current().unwrap().islands(), before);
    }
}

pub(super) fn native_send(
    app: &mut Settings,
    native: &mut crate::native_test::Native,
    message: Message,
) {
    use cosmic::iced::futures::StreamExt;
    use cosmic::iced::runtime::Action;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut tasks = std::collections::VecDeque::from([app.update(message)]);
        while let Some(task) = tasks.pop_front() {
            if let Some(mut stream) = cosmic::iced::runtime::task::into_stream(task) {
                while let Some(action) = stream.next().await {
                    match action {
                        Action::Output(cosmic::Action::App(message)) => {
                            tasks.push_back(app.update(message))
                        }
                        Action::Widget(operation) => native.operate(app.settings_view(), operation),
                        _ => {}
                    }
                }
            }
        }
    });
}

#[test]
fn auditfix_embedded_editor_errors_stay_visible_in_normal_and_candidate_footers() {
    let name = "auditfix_embedded_editor_errors_stay_visible_in_normal_and_candidate_footers";
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
    let mut native = crate::native_test::Native::new(Size::new(820., 600.));
    send_settings(&mut app, Message::Page(Page::Themes));
    app.profiles.notice = Some(desktop::Notice::Done("Previous theme applied".into()));
    app.system_appearance.notice = Some(desktop::Notice::Error("Mode could not be saved".into()));
    native.text_bounds(app.settings_view(), "Mode could not be saved");
    send_settings(
        &mut app,
        Message::Profiles(profiles::Message::Select(profiles::Selection::Preset(
            "theme-soft",
        ))),
    );
    // Appearance applies a chosen style at once, so there is no separate
    // candidate footer any more; the error must still show after the choice.
    app.system_appearance.notice = Some(desktop::Notice::Error("Mode could not be saved".into()));
    native.text_bounds(app.settings_view(), "Mode could not be saved");
    send_settings(&mut app, Message::Page(Page::DesktopWindows));
    app.windows.core.notice = Some(desktop::Notice::Done("Previous window change saved".into()));
    app.workspaces.core.notice = Some(desktop::Notice::Error(
        "Workspaces could not be saved".into(),
    ));
    native.text_bounds(app.settings_view(), "Workspaces could not be saved");
}

#[test]
fn auditfix_readability_search_reveals_active_widget_finish_from_any_surface() {
    let name = "auditfix_readability_search_reveals_active_widget_finish_from_any_surface";
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
    use crate::materials::{Finish, Kind, Target};
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    app.config.materials.shared = Some(Finish::new(Kind::Frosted));
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let mut native = crate::native_test::Native::new(Size::new(820., 600.));
    for target in [Target::Shared, Target::Widgets, Target::Dock, Target::Panel] {
        app.material_target = target;
        app.material_adjust = false;
        native_send(
            &mut app,
            &mut native,
            Message::SearchResult(navigation::WIDGET_READABILITY),
        );
        native_send(
            &mut app,
            &mut native,
            Message::RevealSearch(navigation::WIDGET_READABILITY),
        );
        assert_eq!(app.material_target, Target::Widgets);
        assert!(app.material_adjust);
        let bounds = native.bounds(app.settings_view(), "material-readability");
        assert!(
            bounds.y >= 0. && bounds.y + bounds.height <= 600.,
            "{bounds:?}"
        );
    }
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}

#[test]
fn auditfix_keyboard_back_closes_browser_and_preserves_theme_candidate() {
    let name = "auditfix_keyboard_back_closes_browser_and_preserves_theme_candidate";
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
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    send_settings(&mut app, Message::Page(Page::Themes));
    send_settings(
        &mut app,
        Message::Profiles(profiles::Message::Select(profiles::Selection::Preset(
            "theme-soft",
        ))),
    );
    // Choosing a style on Appearance applies it at once.
    let chosen = std::fs::read(Config::path().unwrap()).unwrap();
    assert_ne!(chosen, before);
    assert_eq!(
        Config::load().appearance.family,
        Some(crate::desktop_theme::Family::Soft)
    );
    send_settings(&mut app, Message::ToggleSection("appearance-all-styles"));
    assert_eq!(app.page, Page::Themes);
    // Showing every style keeps the chosen style and writes nothing more.
    assert_eq!(
        Config::load().appearance.family,
        Some(crate::desktop_theme::Family::Soft)
    );
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), chosen);
}

#[test]
fn auditfix_reorder_search_scrolls_the_button_into_view() {
    let name = "auditfix_reorder_search_scrolls_the_button_into_view";
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
    let mut native = crate::native_test::Native::new(Size::new(820., 600.));
    native_send(&mut app, &mut native, Message::Page(Page::Devbox));
    native.scroll(app.settings_view(), "settings-content", 10000.);
    let hit = navigation::search("reorder readings")
        .into_iter()
        .find(|h| h.label == "Reorder")
        .unwrap();
    native_send(&mut app, &mut native, Message::SearchResult(hit));
    native_send(&mut app, &mut native, Message::RevealSearch(hit));
    let bounds = native.bounds(app.settings_view(), "devbox-reorder");
    assert!(
        bounds.y >= 0. && bounds.y + bounds.height <= 600.,
        "{bounds:?}"
    );
    native_click(&mut app, &mut native, "devbox-reorder");
    assert!(app.devbox_reordering);
}

#[test]
fn redesign_navigation_scene_and_scroll_are_local_and_remembered() {
    let name = "redesign_navigation_scene_and_scroll_are_local_and_remembered";
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
    // These are native layout tests; never query the real session's media bus.
    unsafe {
        std::env::set_var(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}/absent-bus", root.display()),
        );
    }
    let mut app = open_isolated_editor(&root);
    let before = fixture_files(&root.join("cosmic-root"));
    let preferences = std::fs::read(Config::path().unwrap()).unwrap();
    // The preview is always shown; the old collapse message changes nothing.
    send_settings(&mut app, Message::ToggleScene);
    assert!(app.scene_open());
    send_settings(&mut app, Message::ContentScrolled(175.));
    send_settings(&mut app, Message::Page(Page::DesktopIcons));
    assert!(app.scene_open());
    send_settings(&mut app, Message::Page(Page::Clock));
    let desktop = app
        .nav_model
        .iter()
        .find(|&id| app.nav_model.data::<Page>(id) == Some(&Page::Overview))
        .unwrap();
    send_settings(&mut app, Message::NavSelected(desktop));
    assert_eq!(app.page, Page::Overview, "a root opens its stable landing");
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    assert!(app.scene_open());
    assert_eq!(app.page_scroll.get(Page::DesktopDock.key()), Some(&175.));
    send_settings(&mut app, Message::Tick);
    assert!(app.scene_open(), "refresh must preserve scene state");
    assert_eq!(app.page_scroll.get(Page::DesktopDock.key()), Some(&175.));
    assert_eq!(fixture_files(&root.join("cosmic-root")), before);
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), preferences);
    assert!(!checkpoint_message(&Message::ToggleScene));
    assert!(!checkpoint_message(&Message::ToggleHelp("structural")));
    assert!(!checkpoint_message(&Message::ToggleSection("structural")));
    assert!(!checkpoint_message(&Message::ContentScrolled(20.)));
}

#[test]
fn redesign_routes_render_native_compact_and_wide_without_losing_roots() {
    let name = "redesign_routes_render_native_compact_and_wide_without_losing_roots";
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
    // These are native layout tests; never query the real session's media bus.
    unsafe {
        std::env::set_var(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}/absent-bus", root.display()),
        );
    }
    let mut app = open_isolated_editor(&root);
    let wallpaper =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wallpapers/halcyon-calm.jpg");
    let background = crate::desktop::wallpaper::BgEntry::new(
        "all",
        crate::desktop::wallpaper::BgSource::Path(wallpaper),
    );
    seed(
        &root.join("cosmic-root/config/cosmic"),
        &[(
            "com.system76.CosmicBackground",
            "all",
            &ron::to_string(&background).unwrap(),
        )],
    );
    app.config.settings_animations = false;
    let routes: Vec<_> = Page::ALL
        .into_iter()
        .filter(|p| p.canonical() == *p)
        .collect();
    for size in [Size::new(820., 600.), Size::new(1280., 900.)] {
        let mut native = crate::native_test::Native::new(size);
        send_settings(&mut app, Message::Resized(size));
        for page in &routes {
            send_settings(&mut app, Message::Page(*page));
            assert_eq!(app.page, *page);
            assert_eq!(
                app.nav_model
                    .iter()
                    .filter(|&id| app.nav_model.is_active(id))
                    .count(),
                1
            );
            for root_page in Page::SIDEBAR[0].1 {
                let bounds =
                    native.bounds(app.settings_view(), &format!("nav-{}", root_page.key()));
                assert!(
                    bounds.x >= 0.
                        && bounds.y >= 0.
                        && bounds.x + bounds.width <= size.width
                        && bounds.y + bounds.height <= size.height,
                    "root remains visible: {root_page:?} {bounds:?} at {size:?}"
                );
            }
            let start = native.bounds(app.settings_view(), "page-start");
            assert!(
                start.width > 350. && start.x + start.width <= size.width,
                "{page:?}: {start:?}"
            );
            for (name, theme) in [
                ("dark", cosmic::Theme::dark()),
                ("light", cosmic::Theme::light()),
            ] {
                native.capture_hover(
                    app.settings_view(),
                    &theme,
                    cosmic::iced::Point::new(-1., -1.),
                    &format!(
                        "redesign-{}-{}x{}-{name}",
                        page.key(),
                        size.width as u32,
                        size.height as u32
                    ),
                );
            }
        }
        for subpage in AppSettingsPage::ALL {
            send_settings(&mut app, Message::AppSettingsPage(subpage));
            native.capture_hover(
                app.settings_view(),
                &cosmic::Theme::dark(),
                cosmic::iced::Point::new(-1., -1.),
                &format!(
                    "redesign-app-{}-{}x{}",
                    subpage.key(),
                    size.width as u32,
                    size.height as u32
                ),
            );
        }
    }
}

#[test]
fn redesign_search_reveals_maintenance_pages_and_scene_bypasses_native_queue() {
    let name = "redesign_search_reveals_maintenance_pages_and_scene_bypasses_native_queue";
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
    for (query, expected) in [
        ("settings animations", AppSettingsPage::Interface),
        ("privacy usage", AppSettingsPage::Privacy),
        ("updates installation", AppSettingsPage::Updates),
        ("startup", AppSettingsPage::Operation),
    ] {
        let hit = navigation::search(query)[0];
        send_settings(&mut app, Message::SearchResult(hit));
        assert_eq!(app.page, Page::General, "{query}");
        assert_eq!(app.app_settings_page, expected, "{query}");
        assert_eq!(navigation::app_destination(), expected);
    }
    app.native_busy = true;
    send_settings(&mut app, Message::ToggleScene);
    send_settings(&mut app, Message::ContentScrolled(80.));
    send_settings(&mut app, Message::ToggleHelp("structural"));
    send_settings(&mut app, Message::ToggleSection("structural"));
    assert_eq!(app.help_open, Some("structural"));
    assert!(app.open_sections.contains("structural"));
    // The preview can no longer be hidden; the message is still view-only.
    assert!(app.scene_open());
    assert!(
        app.native_deferred.is_empty(),
        "view changes never accumulate in desktop write queue"
    );
    assert!(
        app.native_busy,
        "view changes must not release a running native operation"
    );
}

#[test]
fn polish_primary_content_actions_fit_without_collapsing_or_scrolling() {
    let name = "polish_primary_content_actions_fit_without_collapsing_or_scrolling";
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
    unsafe {
        std::env::set_var(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}/absent-bus", root.display()),
        );
    }
    let mut app = open_isolated_editor(&root);
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    // Reserve the actual native titlebar's 48 px inside an 820×600 window.
    let size = Size::new(820., 552.);
    send_settings(&mut app, Message::Resized(size));
    for (page, ids) in [
        // Notes save as you type; Save and Discard only appear for
        // edits restored from last time.
        (Page::Notes, vec!["note-editor"]),
        (Page::Calendar, vec!["calendar-subscription-open"]),
        (Page::Weather, vec!["weather-city"]),
        // The folder chooser is the primary way in; typing a path is only
        // offered when the chooser cannot open.
        (Page::Projects, vec!["project-choose-folders"]),
        (
            Page::Appearance,
            vec![
                "material-frosted",
                "material-clear",
                "material-smoked",
                "material-liquid",
            ],
        ),
    ] {
        send_settings(&mut app, Message::Page(page));
        assert!(
            app.scene_open(),
            "primary actions do not require hiding the preview"
        );
        let mut native = crate::native_test::Native::new(size);
        // The always-shown preview sits above the controls, so a primary
        // control may sit below the first screen; it is reached by
        // scrolling the page and then fits whole beside the sidebar.
        for id in ids {
            // native_reveal also checks the control is whole inside the
            // scrolling page, which may end at the window's bottom edge.
            let bounds = native_reveal(&app, &mut native, id);
            assert!(
                bounds.x >= app.sidebar_width()
                    && bounds.y >= 80.
                    && bounds.x + bounds.width <= size.width
                    && bounds.y + bounds.height <= size.height,
                "{page:?}/{id}: {bounds:?}"
            );
        }
        native.scroll(app.settings_view(), "settings-content", 0.);
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("polish-primary-{}", page.key()),
        );
    }
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}

#[test]
fn polish_setup_candidate_reveals_review_and_refresh_keeps_scroll() {
    let name = "polish_setup_candidate_reveals_review_and_refresh_keeps_scroll";
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
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    send_settings(&mut app, Message::Page(Page::DesktopProfiles));
    send_settings(&mut app, Message::ContentScrolled(400.));
    send_settings(
        &mut app,
        Message::Profiles(profiles::Message::Select(profiles::Selection::Preset(
            "calm",
        ))),
    );
    assert!(app.profiles.profile().is_some());
    assert_eq!(app.page_scroll.get(Page::DesktopProfiles.key()), Some(&0.));
    send_settings(&mut app, Message::ContentScrolled(180.));
    send_settings(&mut app, Message::Tick);
    assert_eq!(
        app.page_scroll.get(Page::DesktopProfiles.key()),
        Some(&180.)
    );
    send_settings(&mut app, Message::Profiles(profiles::Message::Discard));
    assert_eq!(app.page_scroll.get(Page::DesktopProfiles.key()), Some(&0.));
    assert_eq!(
        std::fs::read(Config::path().unwrap()).unwrap(),
        before,
        "choosing and cancelling never applies the setup"
    );
}

#[test]
fn polish_privacy_discloses_activity_and_opt_out_status_in_view() {
    let name = "polish_privacy_discloses_activity_and_opt_out_status_in_view";
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
    let size = Size::new(820., 552.);
    send_settings(&mut app, Message::Resized(size));
    send_settings(&mut app, Message::AppSettingsPage(AppSettingsPage::Privacy));
    let mut native = crate::native_test::Native::new(size);
    for label in [
        crate::settings::view::USAGE_SHARED,
        "Never sent anywhere, even with usage data on",
    ] {
        let bounds = native.text_bounds(app.settings_view(), label);
        assert!(
            bounds.y > 0.
                && bounds.y + bounds.height < size.height
                && bounds.x + bounds.width <= size.width,
            "{bounds:?}"
        );
    }
    // The opt-out switch sits beside what it shares, in the same view.
    let toggle = native.bounds(app.settings_view(), "share-usage-data");
    assert!(
        toggle.y > 0. && toggle.y + toggle.height < size.height && toggle.x + toggle.width <= size.width,
        "{toggle:?}"
    );
}

#[test]
fn polish_accessibility_readability_opens_the_actual_hidden_control() {
    let name = "polish_accessibility_readability_opens_the_actual_hidden_control";
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
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    send_settings(&mut app, Message::Page(Page::DesktopMotion));
    let mut native = crate::native_test::Native::new(Size::new(820., 600.));
    native_click(&mut app, &mut native, "accessibility-readability");
    assert_eq!(app.page, Page::Appearance);
    assert!(app.advanced_open(Page::Appearance));
    assert_eq!(
        app.search_selected.map(|h| h.anchor()),
        Some("contrast-assist")
    );
    assert!(
        native
            .try_bounds(app.settings_view(), "contrast-assist")
            .is_some()
    );
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}

#[test]
fn comfort_navigation_and_scene_shortcuts_are_reachable_without_writes() {
    let name = "comfort_navigation_and_scene_shortcuts_are_reachable_without_writes";
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
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let size = Size::new(820., 552.);
    send_settings(&mut app, Message::Resized(size));
    send_settings(&mut app, Message::Page(Page::Themes));
    let mut native = crate::native_test::Native::new(size);
    // The rail opens the search screen; its field gets focus there.
    let search = native.bounds(app.settings_view(), "rail-search");
    assert!(search.x + search.width <= app.sidebar_width() && search.y > 0.);
    for page in Page::SIDEBAR[0].1 {
        let bounds = native.bounds(app.settings_view(), &format!("nav-{}", page.key()));
        assert!(
            bounds.height >= 40. && bounds.y + bounds.height <= size.height,
            "comfortable visible navigation: {bounds:?}"
        );
    }
    // Appearance has no separate list of shortcuts: parts of the preview
    // itself open their settings. The sample window sits in the middle.
    assert!(
        native
            .try_bounds(app.settings_view(), "scene-objects-toggle")
            .is_none()
    );
    let scene = native.bounds(app.settings_view(), "desktop-scene");
    let point = scene.center();
    assert!(
        point.x > app.sidebar_width() && point.y > 0. && point.y < size.height,
        "the preview's middle is on screen: {scene:?}"
    );
    {
        use cosmic::iced::{Event, mouse};
        for event in [
            mouse::Event::CursorMoved { position: point },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            for message in native
                .event(app.settings_view(), Event::Mouse(event), point)
                .0
            {
                send_settings(&mut app, message);
            }
        }
    }
    assert_eq!(app.page, Page::DesktopAppearance);
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
    assert!(!checkpoint_message(&Message::ToggleSceneObjects));
}

#[test]
fn comfort_theme_inspector_keeps_color_picker_inside_its_column() {
    let name = "comfort_theme_inspector_keeps_color_picker_inside_its_column";
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
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    send_settings(&mut app, Message::Page(Page::Themes));
    send_settings(
        &mut app,
        Message::Profiles(profiles::Message::ThemeCustom(Some("#12".into()))),
    );
    send_settings(
        &mut app,
        Message::ColorWell(color_well::Message::Open(
            color_well::Target::ThemeCustom,
            None,
        )),
    );
    for size in [
        Size::new(820., 600.),
        Size::new(1024., 760.),
        Size::new(1120., 720.),
        Size::new(1280., 900.),
    ] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        let scene = native.bounds(app.settings_view(), "desktop-scene");
        let picker = native.bounds(app.settings_view(), "color-panel");
        assert!(picker.x + picker.width <= size.width - 20., "{picker:?}");
        // One column at every width: the picker opens under the color row,
        // below the preview and inside the content column.
        assert!(
            picker.y > scene.y + scene.height,
            "picker stacks under the preview: {scene:?}; {picker:?}"
        );
        assert!(
            picker.x >= scene.x && picker.x + picker.width <= scene.x + scene.width + 1.,
            "picker stays inside the content column: {scene:?}; {picker:?}"
        );
        assert!(
            picker.width >= 328.,
            "300 px native picker plus its padding must fit: {picker:?}"
        );
        // The full picker can be taller than a compact viewport. Its
        // individual controls must remain reachable in the nested scroll.
        native_reveal(&app, &mut native, "color-quick-0");
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("comfort-theme-picker-{}", size.width),
        );
    }
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}

#[test]
fn setups_collection_comparison_and_keyboard_back_are_local_and_reachable() {
    let name = "setups_collection_comparison_and_keyboard_back_are_local_and_reachable";
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
    unsafe {
        std::env::set_var(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}/absent-bus", root.display()),
        );
    }
    let mut app = open_isolated_editor(&root);
    send_settings(&mut app, Message::Page(Page::DesktopProfiles));
    let bundled = crate::desktop::wallpaper::bundled_dir(app.profiles.store().unwrap());
    std::fs::create_dir_all(&bundled).unwrap();
    for (name, _) in crate::desktop::wallpaper::BUNDLED {
        std::fs::copy(
            crate::desktop::wallpaper::bundled_path(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wallpapers"),
                name,
            ),
            crate::desktop::wallpaper::bundled_path(&bundled, name),
        )
        .unwrap();
    }
    app.profiles.open();
    let preferences = std::fs::read(Config::path().unwrap()).unwrap();
    let desktop = fixture_files(&root.join("cosmic-root"));
    app.config.settings_animations = false;
    for size in [Size::new(820., 552.), Size::new(1280., 852.)] {
        send_settings(
            &mut app,
            Message::Resized(Size::new(size.width, size.height + 48.)),
        );
        let mut native = crate::native_test::Native::new(size);
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("setups-explore-{}", size.width),
        );
        let first = native.bounds(app.settings_view(), "preset-tidal");
        let second = native.bounds(app.settings_view(), "preset-paper-studio");
        assert!(
            second.x > first.x + first.width && first.y == second.y,
            "two real cards fit: {first:?}/{second:?}"
        );
        assert!(
            first.width >= 240. && first.y >= 0. && first.y + first.height <= size.height,
            "the whole first choice fits without scrolling: {first:?}"
        );
        for id in crate::desktop::profiles::discovery::CURATED
            .into_iter()
            .chain(["halcyon", "focus", "paper", "rose", "calm", "evening"])
        {
            native_click(&mut app, &mut native, &format!("preset-{id}"));
            assert_eq!(app.profiles.selection(), &profiles::Selection::Preset(id));
            assert!(
                native
                    .try_bounds(app.settings_view(), "preset-tidal")
                    .is_none(),
                "review hides the competing gallery"
            );
            native.scroll(app.settings_view(), "settings-content", 0.);
            native.capture(
                app.settings_view(),
                &cosmic::Theme::dark(),
                &format!("setups-review-{id}-{}", size.width),
            );
            let apply = native.bounds(app.settings_view(), "profiles-confirm");
            assert!(
                apply.y + apply.height <= size.height,
                "Apply remains in the footer"
            );
            let scene = native.bounds(app.settings_view(), "profile-scene-candidate");
            assert!(
                scene.x >= app.sidebar_width()
                    && scene.x + scene.width <= size.width
                    && scene.y >= 0.
                    && scene.y + scene.height <= apply.y - 8.,
                "the whole desktop picture fits above the fixed footer: {scene:?}, {apply:?}"
            );
            native_click(&mut app, &mut native, "setup-show-current");
            assert!(app.profiles.compare_current);
            assert!(
                native
                    .try_bounds(app.settings_view(), "profile-scene-current")
                    .is_some()
            );
            assert!(
                native
                    .try_bounds(app.settings_view(), "profile-scene-candidate")
                    .is_none()
            );
            native_click(&mut app, &mut native, "setup-show-candidate");
            assert!(!app.profiles.compare_current);
            // The footer's Cancel returns to the gallery. Alt+Left follows
            // history instead, returning to the editor that opened Looks.
            native_click(&mut app, &mut native, "profiles-discard");
            assert_eq!(app.page, Page::DesktopProfiles);
            assert_eq!(app.profiles.selection(), &profiles::Selection::Current);
            native.scroll(app.settings_view(), "settings-content", 0.);
            assert!(
                native
                    .try_bounds(app.settings_view(), "preset-tidal")
                    .is_some(),
                "leaving the review shows the gallery again"
            );
        }
        // Saved looks are listed under the gallery; there are no separate
        // Browse and Saved tabs any more.
        assert!(
            native
                .try_bounds(app.settings_view(), "setups-saved")
                .is_none()
        );
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("setups-saved-{}", size.width),
        );
    }
    assert_eq!(app.back_title().as_deref(), Some("Dock"));
    send_settings(&mut app, Message::Back);
    assert_eq!(app.page, Page::DesktopDock);
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), preferences);
    assert_eq!(fixture_files(&root.join("cosmic-root")), desktop);
    for message in [
        profiles::Message::Compare(true),
        profiles::Message::Collection(true),
    ] {
        assert!(!checkpoint_message(&Message::Profiles(message)));
    }
}

#[test]
fn setups_islands_scenes_follow_plan_and_do_not_edit_applet_selection() {
    let name = "setups_islands_scenes_follow_plan_and_do_not_edit_applet_selection";
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
    unsafe {
        std::env::set_var(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}/absent-bus", root.display()),
        );
    }
    let mut app = open_isolated_editor(&root);
    let store = app.desktop.store().unwrap().clone();
    crate::desktop::islands::fixture_installed(store.fixture_root().unwrap());
    let key = crate::desktop::islands::key(crate::desktop::panels::PanelKind::Panel);
    let on = dock_islands_core::Settings {
        enabled: true,
        ..Default::default()
    };
    seed(
        &root.join("cosmic-root/config/cosmic"),
        &[
            (key.namespace(), key.key(), &ron::to_string(&on).unwrap()),
            (
                "com.system76.CosmicPanel.Panel",
                "plugins_wings",
                "Some(([\"com.system76.CosmicAppletWorkspaces\"],[\"com.system76.CosmicAppletAudio\"]))",
            ),
            (
                "com.system76.CosmicPanel.Panel",
                "plugins_center",
                "Some([\"com.system76.CosmicAppletTime\"])",
            ),
        ],
    );
    // A direct Setups launch has never opened the Dock editor.
    app.desktop = Default::default();
    app.desktop.islands.installation = crate::desktop::islands::Installation::discover(&store);
    assert!(app.desktop.islands.installation.installed());
    send_settings(&mut app, Message::Page(Page::DesktopProfiles));
    let preferences = std::fs::read(Config::path().unwrap()).unwrap();
    let files = fixture_files(&root.join("cosmic-root"));
    send_settings(&mut app, Message::Resized(Size::new(820., 600.)));
    let mut native = crate::native_test::Native::new(Size::new(820., 552.));
    assert!(!app.desktop.opened());
    let ids: Vec<&str> = app.profiles.presets().iter().map(|p| p.id).collect();
    assert!(ids.iter().any(|id| {
        native
            .try_bounds(app.settings_view(), &format!("starter-islands-panel-{id}"))
            .is_some()
    }));
    for id in ["halcyon", "focus", "paper", "rose", "calm", "evening"] {
        send_settings(
            &mut app,
            Message::Profiles(profiles::Message::Select(profiles::Selection::Preset(id))),
        );
        let panel = app
            .profiles
            .preview()
            .unwrap()
            .bars
            .iter()
            .find(|p| p.kind() == crate::desktop::panels::PanelKind::Panel)
            .unwrap();
        assert!(panel.islands().known().unwrap().enabled);
        assert_eq!(
            panel.islands().known().unwrap().default_layout,
            crate::desktop::profiles::islands_composition(id)
                .unwrap()
                .default_layout
        );
        native.scroll(app.settings_view(), "settings-content", 0.);
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("islands-setup-{id}"),
        );
        let selected = app.desktop.islands.selected;
        // The real island body is a read-only scene, not the Dock editor's selection tool.
        let scene = native.bounds(app.settings_view(), "profile-scene-candidate");
        for x in [0.05, 0.25, 0.5, 0.75, 0.95] {
            let point = cosmic::iced::Point::new(scene.x + scene.width * x, scene.y + 12.);
            for event in [
                cosmic::iced::mouse::Event::CursorMoved { position: point },
                cosmic::iced::mouse::Event::ButtonPressed(cosmic::iced::mouse::Button::Left),
                cosmic::iced::mouse::Event::ButtonReleased(cosmic::iced::mouse::Button::Left),
            ] {
                let messages = native
                    .event(
                        app.settings_view(),
                        cosmic::iced::Event::Mouse(event),
                        point,
                    )
                    .0;
                assert!(
                    !messages
                        .iter()
                        .any(|m| matches!(m, Message::DesktopEditor(_))),
                    "preview must not send editor actions"
                );
                for message in messages {
                    send_settings(&mut app, message);
                }
            }
        }
        assert_eq!(app.desktop.islands.selected, selected);
    }
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), preferences);
    assert_eq!(fixture_files(&root.join("cosmic-root")), files);
}

#[test]
fn structural_setup_inspector_keeps_scene_and_apply_visible() {
    let name = "structural_setup_inspector_keeps_scene_and_apply_visible";
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
    send_settings(&mut app, Message::Page(Page::DesktopProfiles));
    send_settings(
        &mut app,
        Message::Profiles(profiles::Message::Select(profiles::Selection::Preset(
            "calm",
        ))),
    );
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(Size::new(size.width, size.height - 48.));
        let stage = native.bounds(app.settings_view(), "setup-stage");
        let inspector = native.bounds(app.settings_view(), "setup-inspector");
        assert!(
            inspector.x >= stage.x + stage.width + 15.,
            "{stage:?} {inspector:?}"
        );
        assert!(inspector.x + inspector.width <= size.width - 19.);
        native.scroll(app.settings_view(), "setup-scroll", 500.);
        assert_eq!(native.bounds(app.settings_view(), "setup-stage"), stage);
        native.scroll(app.settings_view(), "setup-scroll", 0.);
        for (theme, suffix) in [
            (cosmic::Theme::dark(), "dark"),
            (cosmic::Theme::light(), "light"),
        ] {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("structural-setup-{}-{suffix}", size.width),
            );
        }
    }
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}

#[test]
fn reactive_activity_preferences_are_reachable_saved_and_undoable() {
    let name = "reactive_activity_preferences_are_reachable_saved_and_undoable";
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
    seed(
        &root.join("cosmic-root/config/cosmic"),
        &[(
            "com.system76.CosmicPanel.Panel",
            "plugins_center",
            "Some([\"io.github.profilaz3.Halcyon.Activity\"])",
        )],
    );
    send_settings(&mut app, Message::DesktopEditor(desktop::Message::Refresh));
    send_settings(
        &mut app,
        Message::OpenBar(crate::desktop::panels::PanelKind::Panel),
    );
    send_settings(
        &mut app,
        Message::ObjectTab(crate::settings::navigation::ObjectTab::Content),
    );
    for size in [Size::new(820., 600.), Size::new(1280., 900.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        let b = native_reveal(&app, &mut native, "activity-preference-media");
        assert!(
            b.x >= 0. && b.x + b.width <= size.width && b.y >= 0. && b.y + b.height <= size.height
        );
        native_click(&mut app, &mut native, "activity-preference-media");
        assert!(
            !Config::load().activity_island.media,
            "click must save: {:?} / {} / {:?}",
            b,
            app.status,
            Config::load().activity_island
        );
        assert!(Config::load().activity_island.system);
        assert!(
            crate::desktop::store::widget::FIELDS
                .iter()
                .all(|f| !f.pointer.starts_with("/activity_island/")),
            "local activity choices never enter shared setups"
        );
        send_settings(&mut app, Message::UndoDesktop);
        assert!(Config::load().activity_island.media);
        send_settings(&mut app, Message::ActivityPreference("system", false));
        send_settings(&mut app, Message::ActivityLead(2));
        assert!(
            !Config::load().activity_island.system,
            "editing lead time preserves other choices"
        );
        assert_eq!(Config::load().activity_island.lead_minutes, 15);
        send_settings(&mut app, Message::ActivityPreference("system", true));
        // Per-source and notification switches sit under their parent and
        // only while it is on.
        for key in ["battery", "camera", "bluetooth", "updates", "replace_popups", "notification_text"] {
            assert!(native.try_bounds(app.settings_view(), &format!("activity-preference-{key}")).is_some(), "{key}");
        }
        send_settings(&mut app, Message::ActivityPreference("network", false));
        assert!(!Config::load().activity_island.network && Config::load().activity_island.battery);
        send_settings(&mut app, Message::UndoDesktop);
        assert!(Config::load().activity_island.network, "per-source choices undo");
        send_settings(&mut app, Message::ActivityPreference("notifications", false));
        assert!(
            native
                .try_bounds(app.settings_view(), "activity-preference-replace_popups")
                .is_none()
        );
        send_settings(&mut app, Message::ActivityPreference("notifications", true));
        // Taller than a small window: each switch scrolls into view.
        for key in ["camera", "bluetooth", "reveals"] {
            native_reveal(&app, &mut native, &format!("activity-preference-{key}"));
        }
        send_settings(&mut app, Message::ActivityPreference("bluetooth", false));
        assert!(!Config::load().activity_island.bluetooth && Config::load().activity_island.camera);
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("activity-preferences-{}", size.width),
        );
    }
}

#[test]
fn activity_island_style_is_chosen_saved_and_undone() {
    use crate::activity_island::preferences::IslandStyle;
    let name = "activity_island_style_is_chosen_saved_and_undone";
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
    let mut app = open_isolated_editor(&root);
    seed(
        &root.join("cosmic-root/config/cosmic"),
        &[(
            "com.system76.CosmicPanel.Panel",
            "plugins_center",
            "Some([\"io.github.profilaz3.Halcyon.Activity\"])",
        )],
    );
    send_settings(&mut app, Message::DesktopEditor(desktop::Message::Refresh));
    send_settings(
        &mut app,
        Message::OpenBar(crate::desktop::panels::PanelKind::Panel),
    );
    send_settings(
        &mut app,
        Message::ObjectTab(crate::settings::navigation::ObjectTab::Content),
    );
    let size = Size::new(1280., 900.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    assert_eq!(Config::load().activity_island.style, IslandStyle::Theme);
    let b = native_reveal(&app, &mut native, "activity-preference-style");
    assert!(b.x >= 0. && b.x + b.width <= size.width && b.y >= 0. && b.y + b.height <= size.height);
    send_settings(&mut app, Message::ActivityStyle(1));
    assert_eq!(Config::load().activity_island.style, IslandStyle::Dark);
    assert!(Config::load().activity_island.media, "other choices are kept");
    let b = native_reveal(&app, &mut native, "activity-preference-style");
    assert!(b.width > 0. && b.y + b.height <= size.height);
    native.capture(
        app.settings_view(),
        &cosmic::Theme::dark(),
        "activity-island-style",
    );
    send_settings(&mut app, Message::UndoDesktop);
    assert_eq!(Config::load().activity_island.style, IslandStyle::Theme, "the style undoes");
}

#[test]
fn activity_app_rules_list_changes_resets_and_undoes() {
    use crate::activity_island::preferences::AppRule;
    let name = "activity_app_rules_list_changes_resets_and_undoes";
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
    // The Activity island on the panel, so its settings show.
    seed(
        &root.join("cosmic-root/config/cosmic"),
        &[(
            "com.system76.CosmicPanel.Panel",
            "plugins_center",
            "Some([\"io.github.profilaz3.Halcyon.Activity\"])",
        )],
    );
    let rules = |config: &Config| {
        config
            .activity_island
            .app_rules
            .iter()
            .map(|r| (r.app.clone(), r.rule))
            .collect::<Vec<_>>()
    };
    // Empty, the list says how a rule is made.
    {
        let mut app = open_isolated_editor(&root);
        send_settings(&mut app, Message::DesktopEditor(desktop::Message::Refresh));
        send_settings(&mut app, Message::OpenBar(crate::desktop::panels::PanelKind::Panel));
        send_settings(
            &mut app,
            Message::ObjectTab(crate::settings::navigation::ObjectTab::Content),
        );
        let size = Size::new(820., 600.);
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        native_reveal(&app, &mut native, "activity-app-rules");
        assert!(
            native
                .all_text(app.settings_view())
                .iter()
                .any(|t| t.starts_with("No app rules yet. Right-click a notification"))
        );
    }
    Config::save_activity_preferences(json!({"app_rules": [
        {"app": "Slack", "entry": "com.slack.Slack", "rule": "mute"},
        {"app": "Mail", "rule": "list_only"},
    ]}))
    .unwrap();
    let mut app = open_isolated_editor(&root);
    send_settings(&mut app, Message::DesktopEditor(desktop::Message::Refresh));
    send_settings(&mut app, Message::OpenBar(crate::desktop::panels::PanelKind::Panel));
    send_settings(
        &mut app,
        Message::ObjectTab(crate::settings::navigation::ObjectTab::Content),
    );
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        for id in ["activity-app-rule-0", "activity-app-rule-reset-0", "activity-app-rule-1"] {
            let b = native_reveal(&app, &mut native, id);
            assert!(b.x >= 0. && b.x + b.width <= size.width, "{id} fits: {b:?}");
        }
        let text = native.all_text(app.settings_view());
        // The dropdowns draw their own labels; the captures show them.
        for shown in ["App rules", "Slack", "com.slack.Slack", "Mail", "Reset"] {
            assert!(text.iter().any(|t| t == shown), "{shown} in {text:?}");
        }
        for (theme, suffix) in [
            (cosmic::Theme::dark(), "dark"),
            (cosmic::Theme::light(), "light"),
        ] {
            native_reveal(&app, &mut native, "activity-app-rules");
            native.capture(
                app.settings_view(),
                &theme,
                &format!("activity-app-rules-{}-{suffix}", size.width),
            );
        }
    }
    // A muted app is unmuted here, and the change undoes.
    send_settings(&mut app, Message::ActivityAppRule(0, Some(AppRule::Always)));
    assert_eq!(
        rules(&Config::load()),
        [("Slack".into(), AppRule::Always), ("Mail".into(), AppRule::ListOnly)]
    );
    assert_eq!(Config::load().activity_island.app_rules[0].entry, "com.slack.Slack");
    send_settings(&mut app, Message::UndoDesktop);
    assert_eq!(rules(&Config::load())[0], ("Slack".into(), AppRule::Mute));
    // Reset removes the rule; the others and the other choices stay.
    let mut native = crate::native_test::Native::new(Size::new(1280., 800.));
    native_reveal(&app, &mut native, "activity-app-rule-reset-0");
    native_click(&mut app, &mut native, "activity-app-rule-reset-0");
    assert_eq!(rules(&Config::load()), [("Mail".into(), AppRule::ListOnly)]);
    assert!(Config::load().activity_island.notifications);
    send_settings(&mut app, Message::UndoDesktop);
    assert_eq!(rules(&Config::load()).len(), 2, "reset undoes");
    // Out of range does nothing.
    send_settings(&mut app, Message::ActivityAppRule(9, None));
    assert_eq!(rules(&Config::load()).len(), 2);
    // Rules never enter shared setups, and the store checks their shape.
    assert!(
        crate::desktop::store::widget::FIELDS
            .iter()
            .all(|f| !f.pointer.starts_with("/activity_island/"))
    );
    let field = crate::desktop::store::widget::LOCAL_FIELDS
        .iter()
        .find(|f| f.pointer == "/activity_island/app_rules")
        .expect("app rules are a local field");
    assert!(field.accepts(&json!([{"app": "Slack", "rule": "always"}])));
    assert!(!field.accepts(&json!([{"app": "", "rule": "always"}])));
    assert!(!field.accepts(&json!([{"app": "Slack", "entry": "../x", "rule": "mute"}])));
    assert!(!field.accepts(&json!([{"app": "Slack", "rule": "loud"}])));
}

#[test]
fn whats_new_opens_from_updates_and_each_note_leads_to_its_setting() {
    let name = "whats_new_opens_from_updates_and_each_note_leads_to_its_setting";
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
    use cosmic::iced::{Event, mouse};
    fn click(app: &mut Settings, native: &mut crate::native_test::Native, id: &str) {
        let point = native_reveal(app, native, id).center();
        for event in [
            mouse::Event::CursorMoved { position: point },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            for message in native
                .event(app.settings_view(), Event::Mouse(event), point)
                .0
            {
                native_send(app, native, message);
            }
        }
    }
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let mut native = crate::native_test::Native::new(Size::new(1280., 900.));
    native_send(
        &mut app,
        &mut native,
        Message::AppSettingsPage(AppSettingsPage::Updates),
    );
    let away = cosmic::iced::Point::new(-1., -1.);
    native.capture_hover(app.settings_view(), &cosmic::Theme::dark(), away, "whats-new-updates");
    click(&mut app, &mut native, "updates-whats-new");
    assert_eq!(app.app_settings_page, AppSettingsPage::WhatsNew);
    // Its way up names Updates, where it was opened.
    assert_eq!(app.up_link().map(|(title, _)| title).as_deref(), Some("Updates"));
    for (theme, name) in [(cosmic::Theme::dark(), "dark"), (cosmic::Theme::light(), "light")] {
        native.capture_hover(app.settings_view(), &theme, away, &format!("whats-new-{name}"));
    }
    let release = crate::settings::release_notes::current().unwrap();
    for (index, note) in release.notes.iter().enumerate() {
        let id = format!("whats-new-open-{index}");
        let Some(place) = note.place else {
            assert!(native.try_bounds(app.settings_view(), &id).is_none());
            continue;
        };
        click(&mut app, &mut native, &id);
        assert_eq!(app.page, place.page, "{}", note.title);
        if place.page == Page::General {
            assert_eq!(app.app_settings_page, place.app_destination());
        }
        // The note's place is on screen, not just its page.
        native_send(&mut app, &mut native, Message::RevealSearch(place));
        if place.anchor() != "page-start" {
            let bounds = native.bounds(app.settings_view(), place.anchor());
            assert!(bounds.y >= 0. && bounds.y < 900., "{}: {bounds:?}", note.title);
        }
        native_send(&mut app, &mut native, Message::Back);
        assert_eq!(
            (app.page, app.app_settings_page),
            (Page::General, AppSettingsPage::WhatsNew),
            "Back from {}",
            note.title
        );
    }
    // Reading the notes and following them changes nothing.
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
    // Seeing them is remembered apart from settings: no change to undo.
    assert_eq!(
        crate::settings::release_notes::seen().as_deref(),
        Some(env!("CARGO_PKG_VERSION"))
    );
}

/// Choosing Islands shows the one step the Islands panel still needs, right
/// under Shape; Blend into wallpaper points there instead of repeating it;
/// layout controls appear only once the panel answers.
#[test]
fn islands_shape_shows_one_step_then_layout_controls() {
    let name = "islands_shape_shows_one_step_then_layout_controls";
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
    use crate::desktop::islands::{CompatibilityStatus, Installation, Provenance};
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    let store = app.desktop.store().unwrap().fixture_root().unwrap().to_path_buf();
    crate::desktop::islands::fixture_installed(&store);
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Select(
            crate::desktop::panels::PanelKind::Panel,
        )),
    );
    let size = Size::new(1024., 900.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    let disabled = Installation::Installed {
        provenance: Provenance::Package,
        enabled: false,
        compatible: None,
        status: Some(CompatibilityStatus::Disabled),
        diagnostics: String::new(),
    };
    app.desktop.islands.installation = disabled.clone();
    app.installation_busy = false;
    app.updates.identity = Some(crate::settings::updates::Identity {
        format_version: 1,
        enrollment: crate::settings::updates::Enrollment::Enrolled,
        channel: crate::settings::updates::UpdateChannel::Apt,
        reason: "fixture",
        running_version: "0.2.0".into(),
        installed_version: Some("0.2.0".into()),
        package_version: None,
        islands_version: None,
        architecture: Some("amd64".into()),
        store: None,
        islands_match: crate::installation::IslandsMatch::Ready,
        evidence: Some("fixture verified identity".into()),
    });
    let _ = app.update(Message::ObjectTab(
        crate::settings::navigation::ObjectTab::Design,
    ));
    native_send(
        &mut app,
        &mut native,
        Message::DesktopEditor(desktop::Message::PanelStyle(desktop::PanelStyle::Islands)),
    );
    assert!(
        app.desktop
            .current()
            .unwrap()
            .islands()
            .known()
            .is_some_and(|s| s.enabled),
        "the Islands shape is saved even before the Islands panel runs"
    );
    app.desktop.islands.installation = disabled;
    app.desktop.islands.compatible = false;
    app.desktop.penumbra_supported = false;
    native_reveal(&app, &mut native, "enable-islands");
    native.text_bounds(
        app.settings_view(),
        "Islands use the Islands panel instead of the COSMIC one. It takes over the next time you sign in.",
    );
    native.text_bounds(
        app.settings_view(),
        "Comes with the Islands panel. See Shape above.",
    );
    assert!(native.duplicate_ids(app.settings_view()).is_empty());
    assert!(
        native
            .try_bounds(app.settings_view(), "islands-arrange")
            .is_none(),
        "no layout controls for a panel that isn't running"
    );
    native.capture(
        app.settings_view(),
        &cosmic::Theme::dark(),
        "panel-shape-islands-switch",
    );
    app.desktop.islands.compatible = true;
    app.desktop.penumbra_supported = true;
    native_reveal(&app, &mut native, "islands-arrange");
    assert!(native.try_bounds(app.settings_view(), "panel-host").is_none());
    native.text_bounds(app.settings_view(), "Keep islands together");
    native_reveal(&app, &mut native, "penumbra-gradient");
    native.capture(
        app.settings_view(),
        &cosmic::Theme::dark(),
        "panel-shape-islands-running",
    );
}

/// Islands moved on screen rename the Items to match, so the first heading is
/// the island on the left; one Reset order puts them back as left, middle,
/// right, and Arrange on screen stays where it was.
#[test]
fn islands_order_names_items_as_the_screen_shows_them_and_resets() {
    let name = "islands_order_names_items_as_the_screen_shows_them_and_resets";
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
    use crate::desktop::panels::{PanelKind, Region};
    use dock_islands_core::Section;
    let root = isolated_root();
    let key = crate::desktop::islands::key(PanelKind::Panel);
    // The order Arrange on screen leaves after dragging the middle island first.
    let mut moved = dock_islands_core::Settings {
        enabled: true,
        gap: 40,
        ..Default::default()
    };
    moved.default_layout.together.order = [Section::Center, Section::Start, Section::End];
    seed(
        &root.join("cosmic-root/config/cosmic"),
        &[
            (key.namespace(), key.key(), &ron::to_string(&moved).unwrap()),
            (
                "com.system76.CosmicPanel.Panel",
                "plugins_wings",
                "Some(([\"com.system76.CosmicAppletWorkspaces\"],[\"com.system76.CosmicAppletAudio\"]))",
            ),
            (
                "com.system76.CosmicPanel.Panel",
                "plugins_center",
                "Some([\"com.system76.CosmicAppletTime\"])",
            ),
        ],
    );
    let mut app = open_isolated_editor(&root);
    let store = app.desktop.store().unwrap().clone();
    crate::desktop::islands::fixture_installed(store.fixture_root().unwrap());
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Select(PanelKind::Panel)),
    );
    app.desktop.islands.installation = crate::desktop::islands::Installation::discover(&store);
    app.desktop.islands.compatible = true;
    assert!(app.desktop.islands.installation.installed());
    assert_eq!(
        app.desktop.current().unwrap().islands().known().unwrap(),
        &moved
    );
    let screen = app.desktop.screen_order(PanelKind::Panel);
    assert_eq!(
        screen.regions(),
        [Region::Center, Region::Start, Region::End]
    );
    assert!(screen.rearranged());

    // Read top to bottom, the Items list runs left to right across the screen.
    let headings = |app: &Settings, native: &mut crate::native_test::Native| {
        ["Left island", "Middle island", "Right island"]
            .map(|title| native.text_bounds(app.settings_view(), title).y)
    };
    let adds = |app: &Settings, native: &mut crate::native_test::Native| {
        ["applets-add-start", "applets-add-center", "applets-add-end"]
            .map(|id| native.bounds(app.settings_view(), id).y)
    };
    let shots = [
        (Size::new(820., 600.), "820x600"),
        (Size::new(1280., 800.), "1280x800"),
    ];
    for (size, label) in shots {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        // Twice: a repeated widget ID only fails on the second render.
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        native_reveal(&app, &mut native, "islands-reset-order");
        native.text_bounds(app.settings_view(), "Island order");
        native.text_bounds(
            app.settings_view(),
            "Items follow the islands as they sit on screen",
        );
        assert!(
            native
                .try_bounds(app.settings_view(), "islands-arrange")
                .is_some()
        );
        for (theme, tone) in [
            (cosmic::Theme::dark(), "dark"),
            (cosmic::Theme::light(), "light"),
        ] {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("islands-order-moved-{label}-{tone}"),
            );
        }
        native_reveal(&app, &mut native, "applets-add-end");
        let [left, middle, right] = headings(&app, &mut native);
        assert!(left < middle && middle < right, "{left} {middle} {right}");
        // The middle part's items lead, as its island does on screen.
        let [start, center, end] = adds(&app, &mut native);
        assert!(center < start && start < end, "{center} {start} {end}");
        for (theme, tone) in [
            (cosmic::Theme::dark(), "dark"),
            (cosmic::Theme::light(), "light"),
        ] {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("islands-order-moved-items-{label}-{tone}"),
            );
        }
    }

    let size = Size::new(820., 600.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    native_click(&mut app, &mut native, "islands-reset-order");
    // Apply checks the panel again; the fixture has none to answer, a
    // running Islands panel does.
    app.desktop.islands.compatible = true;
    let reset = app
        .desktop
        .current()
        .unwrap()
        .islands()
        .known()
        .cloned()
        .unwrap();
    assert_eq!(reset.default_layout.together.order, Section::ALL);
    assert_eq!(
        reset,
        dock_islands_core::Settings {
            default_layout: Default::default(),
            ..moved.clone()
        },
        "only the order changes"
    );
    assert!(reset.outputs.is_empty(), "no display entry is made up");
    // Reset applies at once, like the island's other direct edits.
    let saved = std::fs::read_to_string(store.user_path(&key)).unwrap();
    assert_eq!(
        ron::from_str::<dock_islands_core::Settings>(&saved).unwrap(),
        reset
    );
    assert!(!app.desktop.screen_order(PanelKind::Panel).rearranged());

    for (size, label) in shots {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        native_reveal(&app, &mut native, "islands-arrange");
        assert!(
            native
                .try_bounds(app.settings_view(), "islands-reset-order")
                .is_none(),
            "nothing to reset in the natural order"
        );
        assert!(
            !native
                .all_text(app.settings_view())
                .iter()
                .any(|t| t == "Island order")
        );
        for (theme, tone) in [
            (cosmic::Theme::dark(), "dark"),
            (cosmic::Theme::light(), "light"),
        ] {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("islands-order-reset-{label}-{tone}"),
            );
        }
        native_reveal(&app, &mut native, "applets-add-end");
        let [left, middle, right] = headings(&app, &mut native);
        assert!(left < middle && middle < right);
        let [start, center, end] = adds(&app, &mut native);
        assert!(start < center && center < end, "{start} {center} {end}");
        for (theme, tone) in [
            (cosmic::Theme::dark(), "dark"),
            (cosmic::Theme::light(), "light"),
        ] {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("islands-order-reset-items-{label}-{tone}"),
            );
        }
    }
}
