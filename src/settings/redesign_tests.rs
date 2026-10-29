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
        let bounds = native_reveal(&app, &mut native, "penumbra-gradient");
        assert!(bounds.x >= 0. && bounds.x + bounds.width <= size.width);
        assert!(bounds.y >= 0. && bounds.y + bounds.height <= size.height);
        app.desktop.penumbra_supported = true;
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

fn native_send(app: &mut Settings, native: &mut crate::native_test::Native, message: Message) {
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
    assert!(app.profiles.profile().is_some());
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
    send_settings(&mut app, Message::StyleBrowser(true));
    assert!(app.style_browser);
    send_settings(&mut app, Message::Back);
    assert!(!app.style_browser);
    assert_eq!(app.page, Page::Themes);
    assert_eq!(
        app.profiles.selection(),
        &profiles::Selection::Preset("theme-soft")
    );
    assert!(app.profiles.profile().is_some());
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
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
        .find(|h| h.label == "Reorder readings")
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
    send_settings(&mut app, Message::ToggleScene);
    assert!(!app.scene_open());
    send_settings(&mut app, Message::ContentScrolled(175.));
    send_settings(&mut app, Message::Page(Page::DesktopIcons));
    assert!(app.scene_open(), "collapse belongs to its editor");
    send_settings(&mut app, Message::Page(Page::Clock));
    let desktop = app
        .nav_model
        .iter()
        .find(|&id| app.nav_model.data::<Page>(id) == Some(&Page::DesktopDock))
        .unwrap();
    send_settings(&mut app, Message::NavSelected(desktop));
    assert_eq!(app.page, Page::DesktopIcons, "root restores last editor");
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    assert!(!app.scene_open());
    assert_eq!(app.page_scroll.get(Page::DesktopDock.key()), Some(&175.));
    send_settings(&mut app, Message::Tick);
    assert!(!app.scene_open(), "refresh must preserve scene state");
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
        ("restore previous desktop", AppSettingsPage::Restore),
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
    assert!(!app.scene_open());
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
        (Page::Notes, vec!["note-editor", "note-save", "note-revert"]),
        (Page::Calendar, vec!["calendar-subscription-open"]),
        (Page::Weather, vec!["weather-city"]),
        (Page::Projects, vec!["project-folder-input"]),
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
        for id in ids {
            let bounds = native.bounds(app.settings_view(), id);
            assert!(
                bounds.x >= 184.
                    && bounds.y >= 100.
                    && bounds.x + bounds.width <= size.width
                    && bounds.y + bounds.height <= size.height - 8.,
                "{page:?}/{id}: {bounds:?}"
            );
        }
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
        "Reports app version, first start, country and daily activity. Never widget content.",
        "Turning this off stops activity reporting and queues a final preference update.",
    ] {
        let bounds = native.text_bounds(app.settings_view(), label);
        assert!(
            bounds.y > 0.
                && bounds.y + bounds.height < size.height
                && bounds.x + bounds.width <= size.width,
            "{bounds:?}"
        );
    }
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
    let search = native.bounds(app.settings_view(), "settings-search");
    assert!(search.x + search.width <= 184. && search.y > 0.);
    for page in Page::SIDEBAR[0].1 {
        let bounds = native.bounds(app.settings_view(), &format!("nav-{}", page.key()));
        assert!(
            bounds.height >= 40. && bounds.y + bounds.height <= size.height,
            "comfortable visible navigation: {bounds:?}"
        );
    }
    assert!(!app.scene_objects_open);
    assert!(
        native
            .try_bounds(app.settings_view(), "scene-open-window")
            .is_none()
    );
    native_click(&mut app, &mut native, "scene-objects-toggle");
    assert!(app.scene_objects_open);
    native_click(&mut app, &mut native, "scene-open-window");
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
        Message::ColorWell(color_well::Message::Open(color_well::Target::Accent, None)),
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
        if size.width == 820. {
            assert!(
                picker.y > scene.y + scene.height,
                "compact picker stacks: {scene:?}; {picker:?}"
            );
        } else {
            assert!(
                picker.x > scene.x + scene.width,
                "scene={scene:?}; picker={picker:?}"
            );
        }
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
            first.width >= 240. && first.y + first.height <= size.height,
            "first choices fit without scrolling: {first:?}"
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
                scene.y + scene.height <= apply.y - 8.,
                "whole desktop must fit above the fixed footer: {scene:?}, {apply:?}"
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
            send_settings(&mut app, Message::Back);
            assert_eq!(app.page, Page::DesktopProfiles);
            assert_eq!(app.profiles.selection(), &profiles::Selection::Current);
            native.scroll(app.settings_view(), "settings-content", 0.);
        }
        native_click(&mut app, &mut native, "setups-saved");
        assert!(app.profiles.saved_collection);
        assert!(
            native
                .try_bounds(app.settings_view(), "preset-tidal")
                .is_none()
        );
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("setups-saved-{}", size.width),
        );
        native_click(&mut app, &mut native, "setups-explore");
    }
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
    native.bounds(app.settings_view(), "starter-islands-panel");
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
