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
                // One column: the sample, then the controls; the footer has
                // only Use this design, and Save sits under More in the page.
                let controls = native.bounds(app.settings_view(), "recipe-stage");
                let apply = native.bounds(app.settings_view(), "recipe-apply");
                assert!(
                    controls.width >= 320.,
                    "readable builder controls: {controls:?}"
                );
                native.operate(
                    app.settings_view(),
                    Box::new(navigation::Reveal::target("recipe-save")),
                );
                let viewport = native.bounds(app.settings_view(), "settings-viewport");
                let save = native.bounds(app.settings_view(), "recipe-save");
                assert!(
                    save.y >= viewport.y && save.y + save.height <= viewport.y + viewport.height,
                    "Save is reachable in the page: {save:?} in {viewport:?}"
                );
                native.scroll(app.settings_view(), "settings-content", 0.);
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
    // The bar names the real target everywhere, so Ctrl+Z never surprises.
    assert!(app.page_undo_button().is_some());
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
    send_settings(&mut app, Message::Page(Page::Themes));
    // Day and night style is now part of Time of day.
    send_settings(&mut app, Message::Page(Page::Moments));
    assert_eq!(app.page, Page::DesktopAutomation);
    let target = crate::automation::ProfileRef::Moment(crate::moments::Phase::Night);
    send_settings(
        &mut app,
        Message::Automation(automation::Message::Schedule(target.clone())),
    );
    assert_eq!(app.page, Page::DesktopAutomation);
    assert_eq!(app.automation.draft.as_ref().unwrap().profile, Some(target.clone()));
    // Adaptive looks are chosen on the same time of day page, so opening
    // them keeps the schedule draft and its target.
    send_settings(&mut app, Message::Page(Page::AdaptiveLooks));
    assert_eq!(app.page, Page::DesktopAutomation);
    assert_eq!(app.automation.draft.as_ref().unwrap().profile, Some(target));
    // Back returns to where the person came from.
    send_settings(&mut app, Message::Back);
    assert_eq!(app.page, Page::Themes);
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
        // Saving sits at the top of Looks; importing waits behind ⋯.
        let mut native = crate::native_test::Native::new(size);
        if app.help_open != Some("looks-more") {
            send_settings(&mut app, Message::ToggleHelp("looks-more"));
        }
        for id in ["setup-save-current", "looks-more", "discovery-import"] {
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
        native.text_bounds(app.settings_view(), "Look file (.json)…");
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
    send_settings(&mut app, Message::Page(Page::Changes));
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

/// Changes sits pinned at the bottom of the sidebar with today's count.
/// The page filters by who made a change, offers Undo only on the latest
/// manual change and opens any change where it was made.
#[test]
fn changes_pinned_in_sidebar_filters_and_opens_where_made() {
    let name = "changes_pinned_in_sidebar_filters_and_opens_where_made";
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
    use crate::settings::support::Filter;
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
                .apply_action(&patch, None, Owner::DockPanel, "Dock spacing", Source::Manual)
                .unwrap()
                .id,
        );
    }
    send_settings(&mut app, Message::Page(Page::Overview));
    send_settings(&mut app, Message::Support(support::Message::Changed));
    assert_eq!(app.changes_today(), 2);
    assert!(app.latest_change_line().is_some_and(|line| line.starts_with("Latest: ")));

    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        send_settings(&mut app, Message::Page(Page::Overview));
        let mut native = crate::native_test::Native::new(size);
        let entry = native.bounds(app.settings_view(), "nav-changes");
        assert!(entry.y + entry.height <= size.height, "pinned inside the window");
        native.capture_hover(
            app.settings_view(),
            &cosmic::Theme::dark(),
            entry.center(),
            &format!("changes-sidebar-hover-{}", size.width),
        );
        native_click(&mut app, &mut native, "nav-changes");
        assert_eq!(app.page, Page::Changes);
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        native_reveal(&app, &mut native, "history-review-undo");
        for (theme, name) in [(cosmic::Theme::light(), "light"), (cosmic::Theme::dark(), "dark")] {
            native.capture(app.settings_view(), &theme, &format!("changes-page-{}-{name}", size.width));
        }
    }

    let mut native = crate::native_test::Native::new(Size::new(1280., 800.));
    // Only the latest manual change can be undone; the earlier one opens.
    assert!(native.try_bounds(app.settings_view(), &format!("change-open-{}", ids[0])).is_some());
    send_settings(&mut app, Message::Support(support::Message::Filter(Filter::Automatic)));
    assert!(native.try_bounds(app.settings_view(), "history-review-undo").is_none());
    native.text_bounds(app.settings_view(), "No automatic changes yet");
    native.capture(app.settings_view(), &cosmic::Theme::dark(), "changes-filter-automatic");
    send_settings(&mut app, Message::Support(support::Message::Filter(Filter::You)));
    assert!(native.try_bounds(app.settings_view(), "history-review-undo").is_some());

    native_click(&mut app, &mut native, &format!("change-open-{}", ids[0]));
    assert_eq!(app.page.sidebar(), Page::DesktopDock, "opens where it was made");
    send_settings(&mut app, Message::Back);
    assert_eq!(app.page, Page::Changes);

    // The footer's last save leads to the same list.
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    app.last_desktop_change = Some((Owner::DockPanel, "Dock spacing".into(), ids[1].clone()));
    app.undo_baseline = None;
    // The footer sits below the scrolling page, so it is clicked in place.
    let point = native.bounds(app.settings_view(), "settings-saved-changes").center();
    for event in [
        cosmic::iced::mouse::Event::CursorMoved { position: point },
        cosmic::iced::mouse::Event::ButtonPressed(cosmic::iced::mouse::Button::Left),
        cosmic::iced::mouse::Event::ButtonReleased(cosmic::iced::mouse::Button::Left),
    ] {
        let (messages, _) =
            native.event(app.settings_view(), cosmic::iced::Event::Mouse(event), point);
        for message in messages {
            send_settings(&mut app, message);
        }
    }
    assert_eq!(app.page, Page::Changes);
}

#[test]
fn polish_e_dock_shape_follow_style_and_looks_header() {
    let name = "polish_e_dock_shape_follow_style_and_looks_header";
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
    use crate::desktop::panels::{PanelKind, Setting};
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    let capture = |app: &Settings, native: &mut crate::native_test::Native, name: &str| {
        for (theme, mode) in [(cosmic::Theme::light(), "light"), (cosmic::Theme::dark(), "dark")] {
            native.capture(
                app.settings_view(),
                &theme,
                &format!("{name}-{}-{mode}", native.size.width),
            );
        }
    };
    // Dock: one Shape (Compact / Full width) replaces the Full width
    // switch; Floating stays its own choice.
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Select(PanelKind::Dock)),
    );
    let floating = app.desktop.current().unwrap().floating();
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        native_click(&mut app, &mut native, "native-shape-full");
        assert_eq!(app.desktop.current().unwrap().extend(), Setting::Known(true));
        native_click(&mut app, &mut native, "native-shape-compact");
        assert_eq!(app.desktop.current().unwrap().extend(), Setting::Known(false));
        assert_eq!(app.desktop.current().unwrap().floating(), floating);
        assert!(native.text_all(app.settings_view(), "Full width").len() == 1, "only the Shape option");
        native_reveal(&app, &mut native, "dock-style-bars");
        capture(&app, &mut native, "polish-e-dock");
    }
    // Whether looks color the bars is set here now, not on Appearance.
    let before = app.config.appearance.style_bars;
    let mut native = crate::native_test::Native::new(Size::new(1280., 800.));
    // The switch changes only itself: the rest of the widget appearance stays.
    Config::save_appearance(json!({"opacity": 61, "corner_radius": 9})).unwrap();
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    native_click(&mut app, &mut native, "dock-style-bars");
    let saved = Config::load().appearance;
    assert_eq!(saved.style_bars, !before);
    assert_eq!((saved.opacity, saved.corner_radius), (61, 9));
    send_settings(&mut app, Message::Page(Page::Themes));
    assert!(native.text_all(app.settings_view(), "Style the dock and panel").is_empty());
    // Clock: following the look hides its own style choices.
    send_settings(&mut app, Message::Page(Page::Clock));
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        native_reveal(&app, &mut native, "clock-follow-style");
        assert!(native.try_bounds(app.settings_view(), "font-clock").is_none());
        capture(&app, &mut native, "polish-e-clock-follow");
    }
    let mut native = crate::native_test::Native::new(Size::new(1280., 800.));
    native_click(&mut app, &mut native, "clock-follow-style");
    assert!(native.try_bounds(app.settings_view(), "font-clock").is_some());
    send_settings(
        &mut app,
        Message::ClockWeight(Some(crate::widget_style::resolver::ClockWeight::Bold)),
    );
    native_reveal(&app, &mut native, "clock-follow-style");
    capture(&app, &mut native, "polish-e-clock-own");
    native_click(&mut app, &mut native, "clock-follow-style");
    assert_eq!(Config::load().clock.weight, None);
    assert!(native.try_bounds(app.settings_view(), "font-clock").is_none());
    // Looks: saving is the main action; Mix and Import wait behind ⋯.
    send_settings(&mut app, Message::Page(Page::DesktopProfiles));
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        assert!(native.try_bounds(app.settings_view(), "setup-save-current").is_some());
        assert!(native.try_bounds(app.settings_view(), "mix-start").is_none());
        native_click(&mut app, &mut native, "looks-more");
        assert!(native.try_bounds(app.settings_view(), "mix-start").is_some());
        assert!(native.try_bounds(app.settings_view(), "discovery-import").is_some());
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        capture(&app, &mut native, "polish-e-looks-more");
        native_click(&mut app, &mut native, "looks-more");
        assert!(native.try_bounds(app.settings_view(), "mix-start").is_none());
    }
    // The look applied last says so on its tile.
    let state = app.profiles.store().unwrap().state_dir().join("looks-last-applied");
    std::fs::create_dir_all(state.parent().unwrap()).unwrap();
    let tidal = json!({"face": "digital", "weight": "light", "italic": false, "surface": "none"});
    std::fs::write(
        &state,
        serde_json::to_vec(&json!({"look": "preset:tidal", "change": "tidal-change", "style": {
            "clock": tidal, "clock_font": null,
            "glance": {"own_background": true, "font": null, "clock_font": null}}}))
        .unwrap(),
    )
    .unwrap();
    send_settings(&mut app, Message::Page(Page::Overview));
    send_settings(&mut app, Message::Page(Page::DesktopProfiles));
    assert_eq!(
        app.profiles.last_applied(),
        Some(profiles::Selection::Preset("tidal"))
    );
    let mut native = crate::native_test::Native::new(Size::new(1280., 800.));
    native_reveal(&app, &mut native, "preset-tidal");
    assert_eq!(native.text_all(app.settings_view(), "Last applied").len(), 1);
    capture(&app, &mut native, "polish-e-looks-last-applied");
    // Following the look puts back the look's own clock, not a blank one.
    send_settings(&mut app, Message::Page(Page::Clock));
    assert!(native.try_bounds(app.settings_view(), "font-clock").is_some());
    native_reveal(&app, &mut native, "clock-follow-style");
    native_click(&mut app, &mut native, "clock-follow-style");
    assert_eq!(profiles::LookStyle::of(&Config::load()).clock, tidal);
    assert!(native.try_bounds(app.settings_view(), "font-clock").is_none());
    // Undoing that look forgets it; its clock then counts as the user's own.
    app.profiles.forget_applied("another-change");
    assert!(app.profiles.last_applied().is_some());
    app.profiles.forget_applied("tidal-change");
    assert_eq!(app.profiles.last_applied(), None);
    assert!(!state.exists());
    assert!(native.try_bounds(app.settings_view(), "font-clock").is_some());
}

/// Phase F: widget setting changes name their widget in Changes, and any
/// kept change of yours can be put back. A setting changed again since stops
/// it and opens the review, which keeps the newer value.
#[test]
fn history_f_changes_names_widgets_and_puts_back_older_changes() {
    let name = "history_f_changes_names_widgets_and_puts_back_older_changes";
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
    let latest = |store: &crate::desktop::DesktopStore| {
        store.recent_changes().unwrap().entries.last().unwrap().clone()
    };
    let spacing = |store: &crate::desktop::DesktopStore, value: u32| {
        let key = dock_key("spacing");
        let mut patch = DesktopPatch::new();
        patch.set(
            key.clone(),
            store.read_user(&key).unwrap(),
            Some(RawValue::parse(value.to_string()).unwrap()),
        );
        store
            .apply_action(&patch, None, Owner::DockPanel, "Dock spacing", Source::Manual)
            .unwrap()
            .id
    };
    // A widget change made in Settings, a dock change, then another widget one.
    let date_before = app.config.clock.show_date;
    send_settings(&mut app, Message::ClockDate(!date_before));
    let date = latest(&store);
    assert_eq!(date.widgets, ["Clock"]);
    let dock = spacing(&store, 5);
    send_settings(
        &mut app,
        Message::GlanceDensity(crate::config::GlanceDensity::Comfortable),
    );
    let glance = latest(&store);
    assert_eq!(glance.title(), "Changed Daily overview");
    assert_eq!(crate::settings::support::change_page(&glance), Page::Glance);

    send_settings(&mut app, Message::Page(Page::Changes));
    send_settings(&mut app, Message::Support(support::Message::Changed));
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        assert!(native.duplicate_ids(app.settings_view()).is_empty());
        // The latest keeps Undo; older ones offer Put back.
        assert!(native.try_bounds(app.settings_view(), "history-review-undo").is_some());
        assert!(native.try_bounds(app.settings_view(), &format!("change-put-back-{}", glance.id)).is_none());
        native_reveal(&app, &mut native, &format!("change-put-back-{}", date.id));
        assert!(!native.text_all(app.settings_view(), "Changed Daily overview").is_empty());
        for (theme, mode) in [(cosmic::Theme::light(), "light"), (cosmic::Theme::dark(), "dark")] {
            native.capture(app.settings_view(), &theme, &format!("history-f-changes-{}-{mode}", size.width));
        }
    }
    let mut native = crate::native_test::Native::new(Size::new(1280., 800.));
    native_click(&mut app, &mut native, &format!("change-put-back-{}", date.id));
    assert_eq!(Config::load().clock.show_date, date_before, "the clock date is back");
    assert_eq!(
        Config::load().glance.density,
        crate::config::GlanceDensity::Comfortable,
        "newer changes stay"
    );
    send_settings(&mut app, Message::Support(support::Message::Changed));
    assert!(native.try_bounds(app.settings_view(), &format!("change-put-back-{}", date.id)).is_none());
    assert!(native.try_bounds(app.settings_view(), "history-review-undo").is_some());

    // The dock spacing changed again since: Put back opens the review.
    spacing(&store, 9);
    send_settings(&mut app, Message::Support(support::Message::Changed));
    native_reveal(&app, &mut native, &format!("change-put-back-{dock}"));
    native_click(&mut app, &mut native, &format!("change-put-back-{dock}"));
    let review = app.undo_review.clone().expect("the review opens");
    assert!(review.puts_back());
    assert!(!native.text_all(app.settings_view(), "Put back the rest").is_empty());
    for (theme, mode) in [(cosmic::Theme::light(), "light"), (cosmic::Theme::dark(), "dark")] {
        native.capture(app.settings_view(), &theme, &format!("history-f-review-1280-{mode}"));
    }
    // The review sits in the footer, below the scrolling page.
    let point = native.bounds(app.settings_view(), "undo-keep-current").center();
    for event in [
        cosmic::iced::mouse::Event::CursorMoved { position: point },
        cosmic::iced::mouse::Event::ButtonPressed(cosmic::iced::mouse::Button::Left),
        cosmic::iced::mouse::Event::ButtonReleased(cosmic::iced::mouse::Button::Left),
    ] {
        let (messages, _) =
            native.event(app.settings_view(), cosmic::iced::Event::Mouse(event), point);
        for message in messages {
            send_settings(&mut app, message);
        }
    }
    assert!(app.undo_review.is_none());
    assert_eq!(
        store.read_user(&dock_key("spacing")).unwrap().map(|v| v.as_str().trim().to_string()),
        Some("9".into()),
        "the newer value is kept"
    );
    let recent = store.recent_changes().unwrap();
    assert!(!recent.restorable.contains(&dock));
    assert_eq!(
        recent.entries.last().unwrap().title(),
        "Kept current settings",
        "keeping writes nothing back"
    );
}

/// The narrow rail shows each entry as an icon over one line of text, with
/// the Changes count beside its label.
#[test]
fn rail_entries_keep_one_line_labels() {
    let name = "rail_entries_keep_one_line_labels";
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
    // Something changed today, so Changes shows its count.
    let date = app.config.clock.show_date;
    send_settings(&mut app, Message::ClockDate(!date));
    send_settings(&mut app, Message::Support(support::Message::Changed));
    let size = Size::new(820., 600.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    assert!(native.duplicate_ids(app.settings_view()).is_empty());
    let changes = native.bounds(app.settings_view(), "nav-changes");
    let label = native.text_bounds(app.settings_view(), "Changes");
    let lines = |native: &mut crate::native_test::Native, text: &str| {
        native
            .text_all(app.settings_view(), text)
            .into_iter()
            .filter(|bounds| bounds.x < 84.)
            .collect::<Vec<_>>()
    };
    let one = label.height;
    for text in [
        "Looks",
        "Appearance",
        "Wallpaper",
        "Widgets",
        "Dock",
        "Windows",
        "Accessibility",
        "Halcyon",
    ] {
        let found = lines(&mut native, text);
        assert_eq!(found.len(), 1, "{text} shows once in the rail");
        assert!(found[0].height <= one + 1., "{text} wraps: {:?}", found[0]);
        assert!(found[0].width <= 80., "{text} is too wide: {:?}", found[0]);
    }
    assert!(lines(&mut native, "Dock and panel").is_empty());
    // The count sits on the label's line, inside the entry.
    let count = lines(&mut native, "1");
    assert_eq!(count.len(), 1, "one count beside Changes");
    assert!((count[0].center_y() - label.center_y()).abs() <= 2., "{count:?} {label:?}");
    assert!(count[0].x > label.x + label.width);
    assert!(count[0].x + count[0].width <= changes.x + changes.width);
    for (theme, mode) in [(cosmic::Theme::light(), "light"), (cosmic::Theme::dark(), "dark")] {
        native.capture(app.settings_view(), &theme, &format!("rail-820-{mode}"));
    }
    // The wide sidebar keeps the full name.
    send_settings(&mut app, Message::Resized(Size::new(1280., 800.)));
    let mut native = crate::native_test::Native::new(Size::new(1280., 800.));
    assert!(!native.text_all(app.settings_view(), "Dock and panel").is_empty());
}

/// Every sidebar root and the main subpages in the narrow layout, light and
/// dark, with real icons: no text runs past the window and no name repeats.
/// Captures each screenful from the top to the end of the page.
#[test]
fn narrow_pages_fit_the_window() {
    let name = "narrow_pages_fit_the_window";
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
    use crate::desktop::panels::PanelKind;
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    let size = Size::new(820., 600.);
    send_settings(&mut app, Message::Resized(size));
    assert!(app.rail());
    let stops: [(&str, fn() -> Message); 16] = [
        ("looks", || Message::OpenRoot(Page::DesktopProfiles)),
        ("appearance", || Message::OpenRoot(Page::Themes)),
        ("wallpaper", || Message::OpenRoot(Page::DesktopWallpaper)),
        ("widgets", || Message::OpenRoot(Page::Overview)),
        ("dock", || Message::OpenRoot(Page::DesktopDock)),
        ("windows", || Message::OpenRoot(Page::DesktopWindows)),
        ("accessibility", || Message::OpenRoot(Page::DesktopMotion)),
        ("halcyon", || Message::OpenRoot(Page::General)),
        ("changes", || Message::OpenRoot(Page::Changes)),
        ("clock", || Message::Page(Page::Clock)),
        ("glance", || Message::Page(Page::Glance)),
        ("weather", || Message::Page(Page::Weather)),
        ("calendar", || Message::Page(Page::Calendar)),
        ("time-of-day", || Message::Page(Page::DesktopAutomation)),
        ("dock-panel", || Message::OpenBar(PanelKind::Panel)),
        // A saved change puts its note and Undo in the footer.
        ("dock-panel-saved", || {
            Message::DesktopEditor(desktop::Message::Floating(true))
        }),
    ];
    for (stop, open) in stops {
        send_settings(&mut app, open());
        let mut native = crate::native_test::Native::new(size);
        assert_eq!(
            native.duplicate_ids(app.settings_view()),
            Vec::<String>::new(),
            "{stop}"
        );
        assert_eq!(
            native.overflowing_text(app.settings_view()),
            Vec::new(),
            "{stop} runs past the window"
        );
        for (theme, mode) in [(cosmic::Theme::light(), "light"), (cosmic::Theme::dark(), "dark")] {
            let viewport = native.bounds(app.settings_view(), "settings-viewport");
            let step = viewport.height - 80.;
            for screen in 0..8 {
                native.scroll(app.settings_view(), "settings-content", screen as f32 * step);
                native.capture(
                    app.settings_view(),
                    &theme,
                    &format!("narrow-820-{stop}-{mode}-{screen}"),
                );
                let page = native.bounds(app.settings_view(), "page-start");
                if page.y + page.height <= viewport.y + viewport.height {
                    break;
                }
            }
            native.scroll(app.settings_view(), "settings-content", 0.);
        }
    }
}
