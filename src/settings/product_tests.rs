// Uses the existing isolated native Settings harness.
#[test]
fn product_applet_remove_is_visible_and_persists_for_both_bars() {
    let name = "product_applet_remove_is_visible_and_persists_for_both_bars";
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
    use crate::desktop::panels::{PanelDraft, PanelKind, Region, Slot};
    use crate::native_test::Native;
    let root = isolated_root();
    let fixture = root.join("cosmic-root");
    let applications = fixture.join("applications");
    std::fs::create_dir_all(&applications).unwrap();
    for (id, name, icon) in [
        ("a", "Workspaces", "view-grid-symbolic"),
        ("b", "Date and time", "preferences-system-time-symbolic"),
    ] {
        std::fs::write(
            applications.join(format!("{id}.desktop")),
            format!("[Desktop Entry]\nName={name}\nIcon={icon}\nX-CosmicApplet=true\n"),
        )
        .unwrap();
    }
    for kind in PanelKind::ALL {
        let dir = fixture
            .join("config/cosmic")
            .join(kind.namespace())
            .join("v1");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plugins_wings"),
            r#"Some((["a", "a"], ["missing"]))"#,
        )
        .unwrap();
        std::fs::write(dir.join("plugins_center"), r#"Some(["b"])"#).unwrap();
    }
    for worker in [false, true] {
        crate::test_env::native_worker(&fixture, worker);
        let mut app = open_isolated_editor(&root);
        for kind in [PanelKind::Panel, PanelKind::Dock] {
            for size in [
                Size::new(820., 600.),
                Size::new(1024., 760.),
                Size::new(656., 480.),
            ] {
                send_settings(&mut app, Message::OpenBar(kind));
                send_settings(&mut app, Message::Resized(size));
                let store = app.desktop.store().unwrap().clone();
                let before = PanelDraft::load(&store, kind).unwrap().applets();
                let other = if kind == PanelKind::Panel {
                    PanelKind::Dock
                } else {
                    PanelKind::Panel
                };
                let other_before = PanelDraft::load(&store, other).unwrap().applets();
                let mut native = Native::new(Size::new(size.width, size.height - 48.));
                assert_eq!(app.desktop.focus, None);
                assert!(
                    native
                        .try_bounds(app.settings_view(), "applet-start-1-remove")
                        .is_some(),
                    "{kind:?} must expose Remove without a separate inspector"
                );
                native_reveal(&app, &mut native, "applet-start-1-remove");
                native.capture(
                    app.settings_view(),
                    &cosmic::Theme::dark(),
                    &format!("applets-{:?}-{}-worker-{}", kind, size.width, worker),
                );
                native_click(&mut app, &mut native, "applet-start-1-remove");
                let after = PanelDraft::load(&store, kind)
                    .unwrap()
                    .applets()
                    .known()
                    .unwrap()
                    .clone();
                assert_eq!(after.start, ["a"], "remove exactly one duplicate");
                assert_eq!(after.center, ["b"]);
                assert_eq!(after.end, ["missing"]);
                assert_eq!(
                    PanelDraft::load(&store, other).unwrap().applets(),
                    other_before
                );
                native_send(&mut app, &mut native, Message::UndoDesktop);
                assert_eq!(PanelDraft::load(&store, kind).unwrap().applets(), before);
                native_reveal(&app, &mut native, "applet-end-0-remove");
                native.capture(
                    app.settings_view(),
                    &cosmic::Theme::dark(),
                    &format!(
                        "applets-missing-{:?}-{}-worker-{}",
                        kind, size.width, worker
                    ),
                );
                native_click(&mut app, &mut native, "applet-end-0-remove");
                assert!(
                    PanelDraft::load(&store, kind)
                        .unwrap()
                        .applets()
                        .known()
                        .unwrap()
                        .end
                        .is_empty()
                );
                assert!(native.duplicate_ids(app.settings_view()).is_empty());
                native_send(&mut app, &mut native, Message::UndoDesktop);
                assert_eq!(PanelDraft::load(&store, kind).unwrap().applets(), before);
                if worker {
                    // The first save owns the model; another click on the old
                    // row must not queue removal of its successor.
                    let pending =
                        app.update(Message::DesktopEditor(desktop::Message::Remove(Slot {
                            region: Region::Start,
                            index: 0,
                        })));
                    assert!(app.native_busy);
                    native_click(&mut app, &mut native, "applet-start-0-remove");
                    assert!(
                        !app.native_deferred.iter().any(|m| matches!(
                            m,
                            Message::DesktopEditor(desktop::Message::Remove(_))
                        ))
                    );
                    settle_settings(&mut app, pending);
                    assert_eq!(
                        PanelDraft::load(&store, kind)
                            .unwrap()
                            .applets()
                            .known()
                            .unwrap()
                            .start,
                        ["a"]
                    );
                    native_send(&mut app, &mut native, Message::UndoDesktop);
                    assert_eq!(PanelDraft::load(&store, kind).unwrap().applets(), before);
                }
            }
        }
    }
}

fn applet_pointer(
    app: &mut Settings,
    native: &mut crate::native_test::Native,
    event: cosmic::iced::mouse::Event,
    at: cosmic::iced::Point,
) {
    for message in native
        .event(app.settings_view(), cosmic::iced::Event::Mouse(event), at)
        .0
    {
        native_send(app, native, message);
    }
}

fn applet_click(app: &mut Settings, native: &mut crate::native_test::Native, id: &str) {
    use cosmic::iced::mouse;
    let at = native_reveal(app, native, id).center();
    for event in [
        mouse::Event::CursorMoved { position: at },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        applet_pointer(app, native, event, at);
    }
}

#[test]
fn product_applet_add_move_drag_cancel_and_empty_regions() {
    let name = "product_applet_add_move_drag_cancel_and_empty_regions";
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
    use crate::desktop::panels::{PanelDraft, PanelKind, Region, Regions, Slot};
    use crate::native_test::Native;
    use cosmic::iced::{Event, Point, keyboard, mouse};
    let root = isolated_root();
    let fixture = root.join("cosmic-root");
    let apps = fixture.join("applications");
    std::fs::create_dir_all(&apps).unwrap();
    for i in 0..10 {
        std::fs::write(apps.join(format!("sample{i}.desktop")),
            format!("[Desktop Entry]\nName=Sample applet {i}\nComment=Fixture applet\nX-CosmicApplet=true\n")).unwrap();
    }
    for worker in [false, true] {
        crate::test_env::native_worker(&fixture, worker);
        for kind in [PanelKind::Panel, PanelKind::Dock] {
            let dir = fixture
                .join("config/cosmic")
                .join(kind.namespace())
                .join("v1");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("plugins_wings"), "Some(([], []))").unwrap();
            std::fs::write(dir.join("plugins_center"), "Some([])").unwrap();
            let mut app = open_isolated_editor(&root);
            send_settings(&mut app, Message::OpenBar(kind));
            send_settings(&mut app, Message::Resized(Size::new(1024., 900.)));
            app.collapsed_scenes.push(Page::DesktopDock);
            let mut native = Native::new(Size::new(1024., 852.));
            let store = app.desktop.store().unwrap().clone();
            applet_click(&mut app, &mut native, "applets-add-start");
            native_send(
                &mut app,
                &mut native,
                Message::DesktopEditor(desktop::Message::Search("applet 3".into())),
            );
            assert!(
                native
                    .try_bounds(app.settings_view(), "applet-picker-add-sample2")
                    .is_none()
            );
            applet_click(&mut app, &mut native, "applet-picker-add-sample3");
            assert_eq!(
                PanelDraft::load(&store, kind)
                    .unwrap()
                    .applets()
                    .known()
                    .unwrap()
                    .start,
                ["sample3"]
            );
            if kind == PanelKind::Panel {
                applet_click(&mut app, &mut native, "applet-start-0-select");
            }
            applet_click(&mut app, &mut native, "applet-start-0-down");
            assert_eq!(
                app.desktop.focus,
                Some(Slot {
                    region: Region::Center,
                    index: 0
                })
            );
            assert!(
                native
                    .try_bounds(app.settings_view(), "applet-center-0-down")
                    .is_some()
            );
            // Focus returned by the first move supports the next keyboard move.
            let enter = Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Enter),
                modified_key: keyboard::Key::Named(keyboard::key::Named::Enter),
                physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Enter),
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::empty(),
                text: None,
                repeat: false,
            });
            let messages = native
                .event(app.settings_view(), enter, Point::new(2., 2.))
                .0;
            assert!(
                messages
                    .iter()
                    .any(|m| matches!(m, Message::DesktopEditor(desktop::Message::Step(_, 1))))
            );
            for message in messages {
                native_send(&mut app, &mut native, message);
            }
            assert_eq!(
                PanelDraft::load(&store, kind)
                    .unwrap()
                    .applets()
                    .known()
                    .unwrap()
                    .end,
                ["sample3"]
            );
            let before_drag = PanelDraft::load(&store, kind).unwrap().applets();
            let from = native_reveal(&app, &mut native, "applet-end-0-drag").center();
            applet_pointer(
                &mut app,
                &mut native,
                mouse::Event::CursorMoved { position: from },
                from,
            );
            applet_pointer(
                &mut app,
                &mut native,
                mouse::Event::ButtonPressed(mouse::Button::Left),
                from,
            );
            assert!(app.desktop.drag.active());
            native_send(
                &mut app,
                &mut native,
                Message::DesktopEditor(desktop::Message::DragCancel),
            );
            assert!(!app.desktop.drag.active());
            assert_eq!(
                PanelDraft::load(&store, kind).unwrap().applets(),
                before_drag
            );
            let from = native_reveal(&app, &mut native, "applet-end-0-drag").center();
            applet_pointer(
                &mut app,
                &mut native,
                mouse::Event::CursorMoved { position: from },
                from,
            );
            applet_pointer(
                &mut app,
                &mut native,
                mouse::Event::ButtonPressed(mouse::Button::Left),
                from,
            );
            let to = native_reveal(&app, &mut native, "applets-add-start").center();
            applet_pointer(
                &mut app,
                &mut native,
                mouse::Event::CursorMoved { position: to },
                to,
            );
            applet_pointer(
                &mut app,
                &mut native,
                mouse::Event::ButtonReleased(mouse::Button::Left),
                to,
            );
            assert!(!app.desktop.drag.active());
            assert_eq!(
                PanelDraft::load(&store, kind)
                    .unwrap()
                    .applets()
                    .known()
                    .unwrap()
                    .start,
                ["sample3"]
            );
            applet_click(&mut app, &mut native, "applet-start-0-remove");
            assert_eq!(
                PanelDraft::load(&store, kind).unwrap().applets().known(),
                Some(&Regions::default())
            );
            assert!(native.duplicate_ids(app.settings_view()).is_empty());
            send_settings(&mut app, Message::DesktopEditor(desktop::Message::Refresh));
            assert_eq!(
                app.desktop.current().unwrap().applets().known(),
                Some(&Regions::default())
            );
        }
    }
}

#[test]
fn product_applet_step_keeps_panel_controls_after_saving() {
    let name = "product_applet_step_keeps_panel_controls_after_saving";
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
    use crate::desktop::panels::{PanelKind, Region, Slot};
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    send_settings(&mut app, Message::OpenBar(PanelKind::Panel));
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Add(Region::Start, "a".into())),
    );
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::SelectApplet(Some(Slot {
            region: Region::Start,
            index: 0,
        }))),
    );
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Step(
            Slot {
                region: Region::Start,
                index: 0,
            },
            1,
        )),
    );
    assert_eq!(
        app.desktop.focus,
        Some(Slot {
            region: Region::Center,
            index: 0
        }),
        "auto-save must not close the selected applet's controls after one move"
    );
}

#[test]
fn product_help_report_and_history_are_read_only_and_keep_private_data_out() {
    let name = "product_help_report_and_history_are_read_only_and_keep_private_data_out";
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
    let before = fixture_files(&root.join("cosmic-root"));
    let config = std::fs::read(Config::path().unwrap()).unwrap();
    let sentinel = "PRIVATE_SENTINEL@example.invalid/private/calendar.ics?token=secret";
    app.calendar.connection.account = sentinel.into();
    app.calendar.connection.calendar_id = sentinel.into();
    app.calendar.connection.credential = sentinel.into();
    app.calendar.connection.calendar_name = sentinel.into();
    app.calendar.error = Some(sentinel.into());
    app.updates.store_error = Some(sentinel.into());
    send_settings(&mut app, Message::AppSettingsPage(AppSettingsPage::Help));
    send_settings(&mut app, Message::Support(support::Message::Preview(true)));
    let report = app.support.report.clone().unwrap();
    assert!(!report.contains("PRIVATE_SENTINEL"));
    assert!(!report.contains("secret"));
    assert!(!report.contains(&root.display().to_string()));
    assert!(report.contains("Calendar: Needs attention"));
    app.calendar.error = None;
    assert_eq!(
        app.support.report.as_deref(),
        Some(report.as_str()),
        "preview is the immutable copy/export payload"
    );
    send_settings(&mut app, Message::Back);
    assert!(app.support.report.is_none());
    send_settings(&mut app, Message::Page(Page::Changes));
    assert_eq!(app.page, Page::Changes);
    send_settings(&mut app, Message::Back);
    assert_ne!(app.page, Page::Changes);
    for query in ["diagnostics", "why did my desktop change"] {
        send_settings(&mut app, Message::SearchSettings(query.into()));
        send_settings(&mut app, Message::SearchFirst);
        let expected = if query.starts_with("why") { Page::Changes } else { Page::General };
        assert_eq!(app.page, expected, "{query}");
    }
    assert_eq!(fixture_files(&root.join("cosmic-root")), before);
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), config);
}
#[test]
fn product_help_tabs_and_primary_actions_fit_compact_and_wide_windows() {
    let name = "product_help_tabs_and_primary_actions_fit_compact_and_wide_windows";
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
    for size in [Size::new(820., 600.), Size::new(1280., 900.)] {
        send_settings(&mut app, Message::Resized(size));
        send_settings(&mut app, Message::Back);
        send_settings(&mut app, Message::Page(Page::General));
        send_settings(&mut app, Message::AppSettingsPage(AppSettingsPage::Home));
        let mut native = crate::native_test::Native::new(size);
        for page in AppSettingsPage::ALL {
            let bounds = crate::settings::tests::native_reveal(
                &app,
                &mut native,
                &format!("app-settings-{}", page.key()),
            );
            assert!(
                bounds.x >= 0.
                    && bounds.x + bounds.width <= size.width
                    && bounds.y + bounds.height <= size.height,
                "{page:?}: {bounds:?}"
            );
        }
        send_settings(&mut app, Message::AppSettingsPage(AppSettingsPage::Help));
        let primary = native.bounds(app.settings_view(), "help-preview-report");
        assert!(
            primary.y + primary.height < size.height,
            "primary action visible on entry"
        );
        // DT-11: the guides sit right under Report a problem.
        for (title, _) in support::GUIDES {
            let link = native.text_bounds(app.settings_view(), title);
            assert!(link.x + link.width <= size.width && link.y + link.height < size.height);
        }
        for (name, theme) in [
            ("dark", cosmic::Theme::dark()),
            ("light", cosmic::Theme::light()),
        ] {
            native.capture_hover(
                app.settings_view(),
                &theme,
                cosmic::iced::Point::new(-1., -1.),
                &format!("product-help-{}-{name}", size.width),
            );
        }
        send_settings(&mut app, Message::Support(support::Message::Preview(true)));
        native_reveal(&app, &mut native, "help-copy-report");
    }
}

#[test]
fn product_direct_help_loads_history_without_opening_a_desktop_editor() {
    let name = "product_direct_help_loads_history_without_opening_a_desktop_editor";
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
    let (mut app, _) = Settings::init(Core::default(), Page::General);
    send_settings(&mut app, Message::AppSettingsPage(AppSettingsPage::Help));
    assert!(
        !app.support.local.problem,
        "A missing history file is a healthy empty state"
    );
    assert!(app.support.local.store.is_some());
    assert!(app.support.local.recent.entries.is_empty());
}
