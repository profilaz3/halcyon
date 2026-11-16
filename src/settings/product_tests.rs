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
                // Held for a moment, then applied once.
                assert_eq!(PanelDraft::load(&store, kind).unwrap().applets(), before);
                pause_items(&mut app);
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
                pause_items(&mut app);
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
                    pause_items(&mut app);
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
            pause_items(&mut app);
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
            pause_items(&mut app);
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
            pause_items(&mut app);
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
            pause_items(&mut app);
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
    pause_items(&mut app);
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
        for (title, _, _) in support::GUIDES {
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
        // FIRST-08: the links that go to GitHub's issues say they need an account.
        native.text_bounds(app.settings_view(), support::NEEDS_ACCOUNT);
        send_settings(&mut app, Message::Support(support::Message::Preview(true)));
        native_reveal(&app, &mut native, "help-copy-report");
        native_reveal(&app, &mut native, "help-open-issue");
        native.text_bounds(app.settings_view(), support::NEEDS_ACCOUNT);
        // Opening the issue page copies the report the person just read and
        // says so. The task isn't run, so no browser opens.
        let _ = app.update(Message::Support(support::Message::ReportProblem));
        assert!(
            app.support
                .notice
                .is_some_and(|notice| notice.starts_with("Report copied")),
            "{:?}",
            app.support.notice
        );
        send_settings(&mut app, Message::Support(support::Message::Preview(false)));
    }
}

/// FIRST-08: the issue page opens with the previewed report in its text
/// when the address stays short, and plain when it wouldn't.
#[test]
fn product_issue_page_carries_the_report_only_when_it_fits() {
    let report = "Halcyon support report · version 1\nDisplay 1: 1920 × 1080, scale 1\n";
    let url = reqwest::Url::parse(&support::issue_url(report)).unwrap();
    assert_eq!(url.path(), "/profilaz3/halcyon/issues/new");
    let body = url
        .query_pairs()
        .find(|(key, _)| key == "body")
        .map(|(_, value)| value.into_owned());
    assert_eq!(body.as_deref(), Some(report));
    let long = "x".repeat(10_000);
    assert_eq!(
        support::issue_url(&long),
        "https://github.com/profilaz3/halcyon/issues/new"
    );
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

/// FIRST-09: the Before Halcyon rows say whether anything can be put back,
/// and the wallpaper row is only there with a saved picture, asking first
/// like the desktop row.
#[test]
fn before_halcyon_rows_say_what_can_be_restored_and_ask_first() {
    let name = "before_halcyon_rows_say_what_can_be_restored_and_ask_first";
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
    let size = Size::new(1280., 900.);
    send_settings(&mut app, Message::Resized(size));
    send_settings(&mut app, Message::Page(Page::Changes));
    let mut native = crate::native_test::Native::new(size);
    assert!(app.desktop_recovery.info.is_none(), "nothing saved yet");
    native_reveal(&app, &mut native, "restore-desktop");
    native.text_bounds(
        app.settings_view(),
        "Nothing to restore yet. Halcyon saves your desktop before its first change.",
    );
    assert!(
        native
            .try_bounds(app.settings_view(), "restore-wallpaper")
            .is_none(),
        "no saved wallpaper, no row"
    );
    let backup = super::recovery::wallpaper_backup().unwrap();
    assert!(backup.starts_with(&root));
    std::fs::create_dir_all(backup.parent().unwrap()).unwrap();
    std::fs::write(&backup, "saved").unwrap();
    send_settings(&mut app, Message::Page(Page::Changes));
    native_click(&mut app, &mut native, "restore-wallpaper");
    assert!(app.restore_confirm, "Restore… asks first");
    native_reveal(&app, &mut native, "restore-wallpaper-confirm");
    native.text_bounds(app.settings_view(), "Restore the earlier wallpaper?");
    native_click(&mut app, &mut native, "restore-wallpaper-cancel");
    assert!(!app.restore_confirm);
    assert!(
        native
            .try_bounds(app.settings_view(), "restore-wallpaper-confirm")
            .is_none()
    );
}

/// FIRST-13: a waiting change has one decision, use it or keep what's on
/// screen, with no page-wide switch on top, and the empty page doesn't
/// talk about a previous version that may never have existed.
#[test]
fn pending_changes_offer_one_decision_per_row() {
    let name = "pending_changes_offer_one_decision_per_row";
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
    use crate::desktop::panels::Anchor;
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    let size = Size::new(1280., 900.);
    send_settings(&mut app, Message::Resized(size));
    send_settings(&mut app, Message::Page(Page::PendingChanges));
    let mut native = crate::native_test::Native::new(size);
    native.text_bounds(
        app.settings_view(),
        "This appears only if an update or change was interrupted.",
    );
    // A dock change an older version saved but never applied.
    let store = app.desktop.store().unwrap().clone();
    let _ = app.desktop.update(desktop::Message::Anchor(Anchor::Left));
    let draft = app.desktop.session_draft().unwrap();
    let record = json!({
        "format": "desktop-cards-settings-draft", "version": 1,
        "kind": session_drafts::Kind::Dock.key(),
        "draft": ron::to_string(&draft).unwrap(),
    });
    let path = store
        .state_file(&format!(
            "settings-draft-{}.json",
            session_drafts::Kind::Dock.key()
        ))
        .unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, record.to_string()).unwrap();
    let _ = app.desktop.update(desktop::Message::Discard);
    app.recovery_sheet = legacy::Recovery::load();
    assert_eq!(app.recovery_sheet.rows.len(), 1);
    send_settings(&mut app, Message::Page(Page::PendingChanges));
    native.bounds(app.settings_view(), "pending-keep-0");
    native.bounds(app.settings_view(), "pending-use-0");
    assert!(
        native
            .text_all(app.settings_view(), "Use selected choices")
            .is_empty(),
        "the rows cover it"
    );
    assert!(
        native
            .try_bounds(app.settings_view(), "pending-edit-0")
            .is_none(),
        "nothing to edit here"
    );
    native_click(&mut app, &mut native, "pending-use-0");
    assert!(app.recovery_sheet.rows.is_empty(), "applied");
    native.text_bounds(app.settings_view(), "Nothing waiting");
}

/// FIRST-05: Halcyon › Running and startup has one way out. It asks
/// first, puts back the desktop from before Halcyon when there is one,
/// stops the widgets (never from a preview), and ends with the command
/// that removes the package. Search finds it by the words people use.
#[test]
fn stop_using_halcyon_puts_the_desktop_back_and_shows_the_removal_command() {
    let name = "stop_using_halcyon_puts_the_desktop_back_and_shows_the_removal_command";
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
    for query in ["uninstall", "remove", "stop"] {
        assert!(
            navigation::search(query)
                .iter()
                .any(|hit| hit.label == "Stop using Halcyon"),
            "{query}"
        );
    }
    assert_eq!(
        navigation::search("uninstall")[0].label,
        "Stop using Halcyon"
    );
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    // Nothing saved yet: it says the desktop stays as it is.
    let (mut app, initial) = Settings::init(Core::default(), Page::General);
    settle_settings(&mut app, initial);
    let size = Size::new(1280., 900.);
    send_settings(&mut app, Message::Resized(size));
    send_settings(
        &mut app,
        Message::AppSettingsPage(AppSettingsPage::Operation),
    );
    let mut native = crate::native_test::Native::new(size);
    native_click(&mut app, &mut native, "stop-using-open");
    assert_eq!(app.stop_using, StopUsing::Confirm);
    native.text_bounds(app.settings_view(), "Stop using Halcyon?");
    assert!(app.desktop_recovery.info.is_none());
    native_click(&mut app, &mut native, "stop-using-cancel");
    assert_eq!(app.stop_using, StopUsing::Closed);
    // A desktop change saves the desktop from before it.
    let key =
        crate::desktop::ConfigKey::new("com.system76.CosmicPanel.Dock", 1, "opacity").unwrap();
    let before = store.read_user(&key).unwrap();
    let mut patch = crate::desktop::DesktopPatch::new();
    patch.set(
        key.clone(),
        before.clone(),
        Some(crate::desktop::RawValue::parse("0.71").unwrap()),
    );
    store.apply(&patch).unwrap();
    native_click(&mut app, &mut native, "stop-using-open");
    assert!(app.desktop_recovery.info.is_some());
    native_click(&mut app, &mut native, "stop-using-confirm");
    assert_eq!(app.stop_using, StopUsing::Done { restored: true });
    assert_eq!(
        store.read_user(&key).unwrap().as_ref().map(|v| v.as_str()),
        before.as_ref().map(|v| v.as_str()),
        "the desktop from before Halcyon is back"
    );
    native.text_bounds(app.settings_view(), "Halcyon is stopped");
    native_reveal(&app, &mut native, "stop-using-command");
    let command = removal_command(app.updates.identity.as_ref().map(|i| i.channel));
    assert!(command.contains("remove cosmic-ext-halcyon"), "{command}");
    native_click(&mut app, &mut native, "stop-using-copy");
    assert_eq!(app.status, "Command copied");
    native_click(&mut app, &mut native, "stop-using-done");
    assert_eq!(app.stop_using, StopUsing::Closed);
}

/// FIRST-07 / FAIL-09: a settings file that is there but can't be read
/// isn't replaced by defaults. Settings opens the same repair widget
/// designs get, setup isn't offered meanwhile, the parser's words wait
/// under Details, and Start fresh keeps a copy of the damaged file.
#[test]
fn a_damaged_settings_file_gets_a_repair_not_defaults() {
    let name = "a_damaged_settings_file_gets_a_repair_not_defaults";
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
    let path = Config::path().unwrap();
    std::fs::write(&path, "{broken").unwrap();
    let loaded = Config::load();
    assert!(loaded.settings_damaged);
    let problem = loaded.design_problem.clone().expect("the damage is named");
    assert!(problem.contains("line 1"), "{problem}");
    assert!(loaded.onboarded && !loaded.widgets_visible);
    let (mut app, initial) = Settings::init(Core::default(), Page::Home);
    settle_settings(&mut app, initial);
    let size = Size::new(1280., 900.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    assert!(app.config.settings_damaged);
    assert!(
        !app.offers_setup(),
        "setup isn't offered over a damaged file"
    );
    assert!(
        native
            .try_bounds(app.settings_view(), "setup-open")
            .is_none()
    );
    native.text_bounds(app.settings_view(), "Halcyon’s settings file is damaged");
    assert!(
        native.text_all(app.settings_view(), &problem).is_empty(),
        "the parser's words wait under Details"
    );
    native_click(&mut app, &mut native, "settings-damage-details");
    native.text_bounds(app.settings_view(), &problem);
    // Start fresh keeps the damaged file and reads a fresh one.
    native_click(&mut app, &mut native, "settings-start-fresh");
    assert!(
        app.config.design_problem.is_none(),
        "{:?}",
        app.design.notice
    );
    let kept: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("config.damaged-")
        })
        .map(|entry| std::fs::read(entry.path()).unwrap())
        .collect();
    assert_eq!(kept, [b"{broken".to_vec()]);
    assert!(Config::load().design_problem.is_none());
    assert!(
        app.design
            .notice
            .as_deref()
            .is_some_and(|notice| notice.starts_with("Started fresh")),
        "{:?}",
        app.design.notice
    );
}
