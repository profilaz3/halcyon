// The existing private desktop fixture drives actual native widget trees.
#[test]
fn polish050_recovered_legacy_layout_can_be_saved_or_cancelled() {
    let name = "polish050_recovered_legacy_layout_can_be_saved_or_cancelled";
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
    for save in [false, true] {
        let mut app = open_isolated_editor(&root);
        send_settings(&mut app, Message::Page(Page::Overview));
        let before = std::fs::read(Config::path().unwrap()).unwrap();
        let _ = app.update_placement(placement::Message::Select("clock".into()));
        let _ = app.update_placement(placement::Message::Nudge([1, 0], true));
        assert!(app.placement.dirty());
        app.checkpoint_session_drafts();
        drop(app);
        let (mut app, initial) = Settings::init(Core::default(), Page::Overview);
        settle_settings(&mut app, initial);
        assert!(app.legacy_layout_recovery());
        assert!(!app.placement_active());
        let mut native = crate::native_test::Native::new(Size::new(820., 600.));
        for id in ["layout-recovery-save", "layout-recovery-cancel"] {
            native_reveal(&app, &mut native, id);
        }
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            "polish050-recovered-layout",
        );
        send_settings(
            &mut app,
            Message::Placement(if save {
                placement::Message::Save
            } else {
                placement::Message::Cancel
            }),
        );
        assert!(!app.error, "{}", app.status);
        assert!(!app.legacy_layout_recovery());
        if save {
            send_settings(&mut app, Message::UndoDesktop);
        }
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
    }
}

#[test]
fn polish050_dismissed_reminder_tracks_exact_undo_and_external_refresh() {
    let name = "polish050_dismissed_reminder_tracks_exact_undo_and_external_refresh";
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
    Config::save_changes(
        json!({"widgets_visible":true,"enabled_widgets":["weather"],"location":null}),
    )
    .unwrap();
    let mut app = open_isolated_editor(&root);
    let task = wizard::follow_up::Task::Weather;
    assert!(
        app.setup_followups
            .pending(&app.config, false)
            .contains(&task)
    );
    send_settings(&mut app, Message::DismissSetupTask(task));
    assert!(!app.error, "{}", app.status);
    assert!(
        !app.setup_followups
            .pending(&app.config, false)
            .contains(&task)
    );
    send_settings(&mut app, Message::UndoDesktop);
    assert!(
        app.setup_followups
            .pending(&app.config, false)
            .contains(&task)
    );
    Config::save_changes(json!({"setup_followup_weather_dismissed":true})).unwrap();
    send_settings(&mut app, Message::Tick);
    assert!(
        !app.setup_followups
            .pending(&app.config, false)
            .contains(&task)
    );
}

#[test]
fn polish050_native_page_patterns_and_one_builder_footer() {
    let name = "polish050_native_page_patterns_and_one_builder_footer";
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
    crate::test_env::native_worker(&root.join("cosmic-root"), true);
    let config = root.join("cosmic-root/widget-config.json");
    std::fs::copy(Config::path().unwrap(), &config).unwrap();
    unsafe {
        std::env::set_var("DESKTOP_CARDS_CONFIG", &config);
    }
    let mut app = open_isolated_editor(&root);
    send_settings(&mut app, Message::Design(design::Message::Review));
    send_settings(&mut app, Message::Design(design::Message::Migrate));
    assert!(app.design.managed, "{:?}", app.design.notice);
    let before = Config::try_load().unwrap();
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        for page in [
            Page::Themes,
            Page::Clock,
            Page::Calendar,
            Page::DesktopAppearance,
            Page::Builder,
            Page::Overview,
            Page::Notes,
            Page::Music,
            Page::Focus,
            Page::Projects,
            Page::Widgets,
            Page::DesktopProfiles,
            Page::DesktopAutomation,
        ] {
            native_send(&mut app, &mut native, Message::Page(page));
            native.scroll(app.settings_view(), "settings-content", 0.);
            native.capture(
                app.settings_view(),
                &cosmic::Theme::dark(),
                &format!("polish050-{}-{}", page.key(), size.width),
            );
            if page == Page::Builder {
                let controls = native.bounds(app.settings_view(), "recipe-inspector");
                let apply = native.bounds(app.settings_view(), "recipe-apply");
                let save = native.bounds(app.settings_view(), "recipe-save");
                assert!(
                    (save.center_y() - apply.center_y()).abs() < 1.,
                    "Builder actions share one row: {save:?}, {apply:?}"
                );
                assert!(controls.width >= 350., "{controls:?}");
                assert!(
                    apply.y >= 0.
                        && apply.y + apply.height <= size.height
                        && apply.x + apply.width <= size.width,
                    "{apply:?}"
                );
                assert!(
                    native
                        .try_bounds(app.settings_view(), "recipe-scroll")
                        .is_none(),
                    "one main settings scroll"
                );
            }
        }
        native_send(&mut app, &mut native, Message::Page(Page::Builder));
        native.operate(
            app.settings_view(),
            Box::new(cosmic::iced::advanced::widget::operation::focusable::focus(
                cosmic::widget::Id::new("recipe-name"),
            )),
        );
        send_settings(&mut app, Message::Resized(Size::new(760., 600.)));
        native.size = Size::new(760., 600.);
        let (messages, _) = native.event(
            app.settings_view(),
            cosmic::iced::Event::Keyboard(cosmic::iced::keyboard::Event::KeyPressed {
                key: cosmic::iced::keyboard::Key::Character("x".into()),
                modified_key: cosmic::iced::keyboard::Key::Character("x".into()),
                physical_key: cosmic::iced::keyboard::key::Physical::Code(
                    cosmic::iced::keyboard::key::Code::KeyX,
                ),
                location: cosmic::iced::keyboard::Location::Standard,
                modifiers: cosmic::iced::keyboard::Modifiers::empty(),
                text: Some("x".into()),
                repeat: false,
            }),
            cosmic::iced::Point::ORIGIN,
        );
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::Recipe(recipes::Message::Name(_)))),
            "focused draft survives reflow"
        );
        for message in messages {
            send_settings(&mut app, message);
        }
        assert!(app.recipe_editor.draft.name.contains('x'));
    }
    assert_eq!(
        serde_json::to_value(Config::try_load().unwrap()).unwrap(),
        serde_json::to_value(before).unwrap(),
        "views and local draft do not write desktop preferences"
    );
}

#[test]
fn polish050_layout_landing_is_read_only_and_canvas_cancel_preserves_desktop() {
    let name = "polish050_layout_landing_is_read_only_and_canvas_cancel_preserves_desktop";
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
    send_settings(&mut app, Message::Page(Page::Overview));
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let mut native = crate::native_test::Native::new(Size::new(820., 600.));
    assert!(
        native
            .try_bounds(app.settings_view(), "placement-card-clock")
            .is_none()
    );
    native_reveal(&app, &mut native, "canvas-start");
    let point = native.bounds(app.settings_view(), "canvas-start").center();
    for event in [
        cosmic::iced::mouse::Event::CursorMoved { position: point },
        cosmic::iced::mouse::Event::ButtonPressed(cosmic::iced::mouse::Button::Left),
        cosmic::iced::mouse::Event::ButtonReleased(cosmic::iced::mouse::Button::Left),
    ] {
        for message in native
            .event(
                app.settings_view(),
                cosmic::iced::Event::Mouse(event),
                point,
            )
            .0
        {
            native_send(&mut app, &mut native, message);
        }
    }
    assert!(app.canvas.is_some());
    send_settings(
        &mut app,
        Message::Canvas(canvas::Message::Style("/clock/face", json!("analog"))),
    );
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
    send_settings(&mut app, Message::Canvas(canvas::Message::Cancel));
    assert!(app.canvas.is_none());
    assert!(!app.placement_active());
    send_settings(
        &mut app,
        Message::Placement(placement::Message::Select("clock".into())),
    );
    send_settings(
        &mut app,
        Message::Placement(placement::Message::Nudge([1, 0], true)),
    );
    send_settings(&mut app, Message::Placement(placement::Message::Save));
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}

#[test]
fn polish050_prior_page_undo_does_not_label_calendar_but_remains_available() {
    let name = "polish050_prior_page_undo_does_not_label_calendar_but_remains_available";
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
    send_settings(&mut app, Message::Page(Page::Clock));
    let before = app.config.clock.format_24h;
    send_settings(&mut app, Message::ClockFormat(!before));
    assert!(app.page_undo_button().is_some());
    send_settings(&mut app, Message::Page(Page::Calendar));
    assert!(app.page_undo_button().is_none());
    send_settings(&mut app, Message::UndoDesktop);
    assert_eq!(app.config.clock.format_24h, before);
}

#[test]
fn polish050_schedule_from_moment_routes_without_losing_target() {
    let name = "polish050_schedule_from_moment_routes_without_losing_target";
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
    send_settings(&mut app, Message::Page(Page::Moments));
    let target = crate::automation::ProfileRef::Moment(crate::moments::Phase::Night);
    send_settings(
        &mut app,
        Message::Automation(automation::Message::Schedule(target.clone())),
    );
    assert_eq!(app.page, Page::DesktopAutomation);
    assert_eq!(app.automation.draft.as_ref().unwrap().profile, Some(target));
    send_settings(&mut app, Message::Page(Page::AdaptiveLooks));
    send_settings(&mut app, Message::Back);
    assert_eq!(app.page, Page::DesktopAutomation);
}
// Include inside settings::tests; uses the existing isolated desktop harness.
#[test]
fn polish050_setup_scope_exports_and_empty_library_fit_native_windows() {
    let name = "polish050_setup_scope_exports_and_empty_library_fit_native_windows";
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
    use crate::desktop::profiles::Domain;
    use profiles::{Message as Edit, Selection};
    let root = isolated_root();
    let pictures = root.join("cosmic-root/pictures/Desktop Cards");
    std::fs::create_dir_all(&pictures).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wallpapers");
    std::fs::copy(
        crate::desktop::wallpaper::bundled_path(&source, "halcyon-calm"),
        crate::desktop::wallpaper::bundled_path(&pictures, "halcyon-calm"),
    )
    .unwrap();
    let mut app = open_isolated_editor(&root);
    app.config.settings_animations = false;
    let preferences = std::fs::read(Config::path().unwrap()).unwrap();
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        send_settings(&mut app, Message::Page(Page::DesktopProfiles));
        send_settings(
            &mut app,
            Message::Profiles(Edit::Select(Selection::Current)),
        );
        send_settings(&mut app, Message::Profiles(Edit::Collection(true)));
        let mut native = crate::native_test::Native::new(size);
        for id in ["setup-save-current", "setup-import"] {
            native_reveal(&app, &mut native, id);
        }
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("polish050-empty-setups-{}", size.width),
        );
        send_settings(
            &mut app,
            Message::Profiles(Edit::Select(Selection::Preset("tidal"))),
        );
        assert_eq!(app.profiles.profile().unwrap().domains().len(), 6);
        app.open_sections.remove("setup-library-actions");
        app.config.settings_animations = false;
        let mut native = crate::native_test::Native::new(size);
        let viewport = native.bounds(app.settings_view(), "settings-viewport");
        for domain in [
            Domain::Dock,
            Domain::Panel,
            Domain::Appearance,
            Domain::Wallpaper,
            Domain::WidgetStyle,
            Domain::WidgetLayout,
        ] {
            let id = format!("setup-include-{}", format!("{domain:?}").to_lowercase());
            let bounds = native.bounds(app.settings_view(), &id);
            assert!(
                bounds.width > 0.
                    && bounds.height > 0.
                    && bounds.x >= 0.
                    && bounds.y >= 0.
                    && bounds.x + bounds.width <= size.width
                    && bounds.y + bounds.height <= viewport.y + viewport.height,
                "all six scopes visible initially: {id}: {bounds:?}"
            );
        }
        let apply = native.bounds(app.settings_view(), "profiles-confirm");
        assert!(apply.y + apply.height <= size.height);
        assert!(
            native
                .try_bounds(app.settings_view(), "setup-export-file")
                .is_none()
        );
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("polish050-setup-review-{}", size.width),
        );
        send_settings(
            &mut app,
            Message::Profiles(Edit::Include(Domain::Wallpaper, false)),
        );
        assert!(!app.profiles.included(Domain::Wallpaper));
        native_click(&mut app, &mut native, "setup-library-actions");
        assert!(app.open_sections.contains("setup-library-actions"));
        for id in ["profiles-share-image", "setup-export-file", "package-open"] {
            native_reveal(&app, &mut native, id);
        }
        native.text_bounds(app.settings_view(), "Save image…");
        native.text_bounds(app.settings_view(), "Setup file (.json)…");
        native.text_bounds(app.settings_view(), "Portable package (.halcyon)…");
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("polish050-setup-exports-{}", size.width),
        );
        assert!(
            !app.profiles.included(Domain::Wallpaper),
            "opening save/export must preserve Apply scope"
        );
        send_settings(&mut app, Message::Profiles(Edit::Discard));
    }
    assert_eq!(
        std::fs::read(Config::path().unwrap()).unwrap(),
        preferences,
        "preview and export disclosure do not apply a setup"
    );
}

#[test]
fn polish050_history_exposes_only_exact_eligible_undo_without_expansion() {
    let name = "polish050_history_exposes_only_exact_eligible_undo_without_expansion";
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
    use crate::desktop::store::{RawValue, Source};
    use crate::desktop::{DesktopPatch, Owner};
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    let store = app.desktop.store().unwrap().clone();
    let key = dock_key("spacing");
    let mut ids = Vec::new();
    for value in [5, 9] {
        let mut patch = DesktopPatch::new();
        patch.set(
            key.clone(),
            store.read_user(&key).unwrap(),
            Some(RawValue::parse(value.to_string()).unwrap()),
        );
        ids.push(
            store
                .apply_action(
                    &patch,
                    None,
                    Owner::DockPanel,
                    "PRIVATE_SENTINEL",
                    Source::Manual,
                )
                .unwrap()
                .id,
        );
    }
    send_settings(&mut app, Message::Support(support::Message::Recent(true)));
    assert_eq!(
        app.support.local.recent.undo_id.as_deref(),
        Some(ids[1].as_str())
    );
    assert!(app.support.expanded.is_none());
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        native_reveal(&app, &mut native, "history-review-undo");
        native.text_bounds(app.settings_view(), "Bar spacing: 5 → 9 px");
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("polish050-history-{}", size.width),
        );
        let current = app.support.local.recent.undo_id.take();
        assert!(
            native
                .try_bounds(app.settings_view(), "history-review-undo")
                .is_none(),
            "no ineligible historical Undo"
        );
        app.support.local.recent.undo_id = current;
    }
    let mut native = crate::native_test::Native::new(Size::new(1280., 800.));
    native_click(&mut app, &mut native, "history-review-undo");
    assert_eq!(
        store.read_user(&key).unwrap().as_ref().map(|v| v.as_str()),
        Some("5")
    );
    assert_eq!(
        store.recent_changes().unwrap().undo_id.as_deref(),
        Some(ids[0].as_str())
    );
}
