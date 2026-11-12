// The existing private desktop fixture drives actual native widget trees.
// An unfinished widget layout found on reopening is kept at once, with no
// review page, and stays one Undo away; a damaged one is dropped.
#[test]
fn polish050_recovered_layout_is_kept_without_asking() {
    let name = "polish050_recovered_layout_is_kept_without_asking";
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
    let record = crate::desktop::DesktopStore::from_env()
        .unwrap()
        .state_file("settings-form-layout.json")
        .unwrap();
    for damaged in [false, true] {
        let mut app = open_isolated_editor(&root);
        send_settings(&mut app, Message::Page(Page::Overview));
        let before = std::fs::read(Config::path().unwrap()).unwrap();
        let _ = app.update_placement(placement::Message::Select("clock".into()));
        let _ = app.update_placement(placement::Message::Nudge([1, 0], true));
        assert!(app.placement.dirty());
        app.checkpoint_session_drafts();
        assert!(record.exists(), "the unfinished layout was not recorded");
        if damaged {
            std::fs::write(&record, b"{ not a layout").unwrap();
        }
        drop(app);
        let (mut app, initial) = Settings::init(Core::default(), Page::Overview);
        settle_settings(&mut app, initial);
        assert!(!app.error, "{}", app.status);
        assert!(!app.placement.dirty());
        assert!(!record.exists(), "the record would come back next time");
        let after = std::fs::read(Config::path().unwrap()).unwrap();
        let mut native = crate::native_test::Native::new(Size::new(820., 600.));
        for id in ["layout-recovery-save", "layout-recovery-cancel"] {
            assert!(
                native.try_bounds(app.settings_view(), id).is_none(),
                "the review page is gone: {id}"
            );
        }
        let words = native.all_text(app.settings_view());
        assert!(!words.iter().any(|t| t == "Unfinished layout"), "{words:?}");
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            if damaged {
                "polish050-dropped-layout"
            } else {
                "polish050-kept-layout"
            },
        );
        if damaged {
            assert_eq!(app.status, placement::DROPPED_LAYOUT);
            assert_eq!(after, before);
            continue;
        }
        assert_ne!(after, before, "the unfinished layout was not saved");
        assert_eq!(app.status, placement::KEPT_LAYOUT);
        assert!(app.layout_kept.is_some());
        // A short note beside Undo, not a page to review.
        assert!(
            words.iter().any(|t| t == placement::KEPT_LAYOUT),
            "{words:?}"
        );
        assert!(
            native
                .try_bounds(app.settings_view(), "settings-undo")
                .is_some()
        );
        send_settings(&mut app, Message::UndoDesktop);
        assert!(!app.error, "{}", app.status);
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
    }
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
                // only Apply design, and Save sits under More in the page.
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
    // Calendar didn't make it, so neither its footer nor Ctrl+Z undoes it
    // here. It stays available on Clock and in Changes.
    assert!(app.page_undo_button().is_none());
    assert!(matches!(
        app.undo_target(),
        crate::settings::navigation::UndoTarget::Desktop(..)
    ));
    send_settings(&mut app, Message::KeyboardUndo(false));
    assert_eq!(app.config.clock.format_24h, !before);
    send_settings(&mut app, Message::Page(Page::Clock));
    assert!(app.page_undo_button().is_some());
    send_settings(&mut app, Message::KeyboardUndo(false));
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
        native.text_bounds(app.settings_view(), "Between items: 5 → 9 px");
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
        assert!(
            entry.y + entry.height <= size.height,
            "pinned inside the window: {entry:?}"
        );
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
        native_reveal(&app, &mut native, "dock-background");
        capture(&app, &mut native, "polish-e-dock");
    }
    // Following the look's style is Background's Match style now; no
    // separate switch says it on the dock or on Appearance.
    let mut native = crate::native_test::Native::new(Size::new(1280., 800.));
    assert!(native.try_bounds(app.settings_view(), "dock-style-bars").is_none());
    assert!(native.text_all(app.settings_view(), "Match the desktop style").is_empty());
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
        // The latest keeps Undo; older ones offer their own Undo.
        assert!(native.try_bounds(app.settings_view(), "history-review-undo").is_some());
        assert!(native.try_bounds(app.settings_view(), &format!("change-undo-{}", glance.id)).is_none());
        native_reveal(&app, &mut native, &format!("change-undo-{}", date.id));
        assert!(!native.text_all(app.settings_view(), "Changed Daily overview").is_empty());
        assert!(!native.text_all(app.settings_view(), "Undoes only this change. Later changes stay.").is_empty());
        for (theme, mode) in [(cosmic::Theme::light(), "light"), (cosmic::Theme::dark(), "dark")] {
            native.capture(app.settings_view(), &theme, &format!("history-f-changes-{}-{mode}", size.width));
        }
    }
    let mut native = crate::native_test::Native::new(Size::new(1280., 800.));
    native_click(&mut app, &mut native, &format!("change-undo-{}", date.id));
    assert_eq!(Config::load().clock.show_date, date_before, "the clock date is back");
    assert_eq!(
        Config::load().glance.density,
        crate::config::GlanceDensity::Comfortable,
        "newer changes stay"
    );
    send_settings(&mut app, Message::Support(support::Message::Changed));
    assert!(native.try_bounds(app.settings_view(), &format!("change-undo-{}", date.id)).is_none());
    assert!(native.try_bounds(app.settings_view(), "history-review-undo").is_some());

    // The dock spacing changed again since: its Undo opens the review.
    spacing(&store, 9);
    send_settings(&mut app, Message::Support(support::Message::Changed));
    native_reveal(&app, &mut native, &format!("change-undo-{dock}"));
    native_click(&mut app, &mut native, &format!("change-undo-{dock}"));
    let review = app.undo_review.clone().expect("the review opens");
    assert!(review.puts_back());
    assert!(!native.text_all(app.settings_view(), "Undo the rest anyway").is_empty());
    // Each conflict in the page's words, never its config key.
    assert_eq!(review.conflicts, ["Between items: now 9 px, was Default"]);
    native.text_bounds(app.settings_view(), "Between items: now 9 px, was Default");
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
        "Weather",
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
    for (theme, mode) in [(cosmic::Theme::light(), "light"), (cosmic::Theme::dark(), "dark")] {
        native.capture(app.settings_view(), &theme, &format!("rail-820-{mode}"));
    }
    // The count is a badge beside the icon, above the one-line label, so
    // it never reads as part of the name.
    assert!(label.width <= 80., "Changes is too wide: {label:?}");
    let count = lines(&mut native, "1");
    assert_eq!(count.len(), 1, "one count on Changes");
    assert!(count[0].y + count[0].height <= label.y, "{count:?} {label:?}");
    assert!(count[0].center_x() > changes.center_x(), "beside the icon: {count:?} {changes:?}");
    assert!(count[0].x + count[0].width <= changes.x + changes.width);
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

#[test]
fn footer_undo_stays_on_the_page_that_made_the_change() {
    let name = "footer_undo_stays_on_the_page_that_made_the_change";
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
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    let capture = |app: &Settings, native: &mut crate::native_test::Native, name: &str| {
        for (theme, mode) in [(cosmic::Theme::light(), "light"), (cosmic::Theme::dark(), "dark")] {
            native.capture(app.settings_view(), &theme, &format!("{name}-{mode}"));
        }
    };
    // A dock setting changed on its own page.
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Select(PanelKind::Dock)),
    );
    let spacing = app.desktop.current().unwrap().spacing().known().copied().unwrap_or(0);
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Spacing(spacing + 4)),
    );
    send_settings(&mut app, Message::DesktopEditor(desktop::Message::Apply));
    assert!(!app.error, "{}", app.status);
    capture(&app, &mut native, "footer-owning-page");
    // The footer uses the Changes page's own words for it.
    let title = app.last_desktop_summary.as_ref().unwrap().title();
    // An unset spacing reads as Default, as on the Changes page.
    assert!(
        title.starts_with("Between items: ") && title.ends_with(&format!("→ {} px", spacing + 4)),
        "{title}"
    );
    native.text_bounds(app.settings_view(), &format!("Undo {title}"));
    native.text_bounds(app.settings_view(), &format!("Saved · {title}"));

    // Another page has no footer Undo for it.
    send_settings(&mut app, Message::Page(Page::Overview));
    capture(&app, &mut native, "footer-other-page");
    assert!(native.try_bounds(app.settings_view(), "settings-undo").is_none());
    assert!(native.text_all(app.settings_view(), &format!("Undo {title}")).is_empty());
    // Its own page offers it again, though it was made on an earlier visit.
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    native.text_bounds(app.settings_view(), &format!("Undo {title}"));
    // Ctrl+Z on another page does nothing, like the footer: the change
    // stays, and its own page still offers to undo it.
    send_settings(&mut app, Message::Page(Page::Overview));
    let id = app.last_desktop_change.as_ref().unwrap().2.clone();
    let spacing_now = app.desktop.current().unwrap().spacing().known().copied();
    send_settings(&mut app, Message::KeyboardUndo(false));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.last_desktop_change.as_ref().map(|c| &c.2), Some(&id));
    assert_eq!(
        app.desktop.current().unwrap().spacing().known().copied(),
        spacing_now
    );
    assert!(native.try_bounds(app.settings_view(), "settings-undo").is_none());
    capture(&app, &mut native, "footer-other-page-ctrl-z");
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    native.text_bounds(app.settings_view(), &format!("Undo {title}"));
    // On its own page, Ctrl+Z undoes it, as the button would.
    send_settings(&mut app, Message::KeyboardUndo(false));
    assert!(!app.error, "{}", app.status);
    assert_ne!(app.last_desktop_change.as_ref().map(|c| &c.2), Some(&id));

    // A switch that writes two keys is named for the switch.
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    let floating = app.desktop.current().unwrap().floating().known().copied().unwrap_or(false);
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Floating(!floating)),
    );
    assert!(!app.error, "{}", app.status);
    capture(&app, &mut native, "footer-floating");
    assert_eq!(
        app.last_desktop_change.as_ref().map(|c| c.1.as_str()),
        Some("Floating"),
        "{:?}",
        app.last_desktop_summary.as_ref().map(|s| s.title())
    );
    native.text_bounds(app.settings_view(), "Undo Floating");
    native.text_bounds(app.settings_view(), "Saved · Floating");

    // A style chosen on Appearance is a style, not a look.
    send_settings(&mut app, Message::Page(Page::Themes));
    let _ = app
        .profiles
        .update(profiles::Message::Select(profiles::Selection::Preset(
            "theme-paper",
        )));
    for job in app.profiles.take_theme_jobs() {
        send_settings(&mut app, Message::Profiles(job.run()));
    }
    send_settings(&mut app, Message::Profiles(profiles::Message::Apply));
    assert!(!app.error, "{}", app.status);
    capture(&app, &mut native, "footer-style");
    native.text_bounds(app.settings_view(), "Undo style “Paper”");
    native.text_bounds(app.settings_view(), "Saved · Style “Paper”");
    assert!(native.text_all(app.settings_view(), "Undo look “Paper”").is_empty());
    let summary = app.last_desktop_summary.clone().unwrap();
    assert!(summary.title().starts_with("Applied a style"), "{}", summary.title());
    assert_eq!(support::change_page(&summary), Page::Themes);
    // Looks didn't make it, so it has no footer Undo for it.
    send_settings(&mut app, Message::Page(Page::DesktopProfiles));
    assert!(native.try_bounds(app.settings_view(), "settings-undo").is_none());
}

#[test]
fn dock_headings_corners_and_panel_regions_read_plainly() {
    let name = "dock_headings_corners_and_panel_regions_read_plainly";
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
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    let capture = |app: &Settings, native: &mut crate::native_test::Native, name: &str| {
        for (theme, mode) in [(cosmic::Theme::light(), "light"), (cosmic::Theme::dark(), "dark")] {
            native.capture(app.settings_view(), &theme, &format!("{name}-{mode}"));
        }
    };
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Select(PanelKind::Dock)),
    );
    send_settings(&mut app, Message::DesktopEditor(desktop::Message::Advanced(true)));
    native_reveal(&app, &mut native, "dock-advanced");
    capture(&app, &mut native, "dock-advanced");
    // Sentence case, like every other heading.
    native.text_bounds(app.settings_view(), "Hide timing");
    native.text_bounds(app.settings_view(), "Spacing");
    assert!(native.text_all(app.settings_view(), "SPACING").is_empty());
    // Each bar's own corners are named for the bar, in its folded section.
    native.text_bounds(app.settings_view(), "Dock corners");
    native_reveal(&app, &mut native, "dock-background");
    capture(&app, &mut native, "dock-tab-style");

    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Select(PanelKind::Panel)),
    );
    native_reveal(&app, &mut native, "dock-corners");
    native.text_bounds(app.settings_view(), "Panel corners");
    assert!(native.text_all(app.settings_view(), "Dock corners").is_empty());
    native_reveal(&app, &mut native, "dock-background");
    capture(&app, &mut native, "panel-tab-style");
    native_reveal(&app, &mut native, "panel-items");
    capture(&app, &mut native, "panel-tab-contents");
    // A bar's middle region has one name, across or down the screen.
    assert_eq!(Region::Center.title(true), "Middle");
    assert_eq!(Region::Center.title(false), "Middle");
}


/// Captures `name` in light and dark at the window size `native` was made for.
fn capture_both(app: &Settings, native: &mut crate::native_test::Native, name: &str) {
    for (theme, mode) in [
        (cosmic::Theme::light(), "light"),
        (cosmic::Theme::dark(), "dark"),
    ] {
        native.capture(app.settings_view(), &theme, &format!("{name}-{mode}"));
    }
}

#[test]
fn dock_and_panel_edit_items_with_one_editor() {
    let name = "dock_and_panel_edit_items_with_one_editor";
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
    let root = isolated_root();
    let fixture = root.join("cosmic-root");
    let apps = fixture.join("applications");
    std::fs::create_dir_all(&apps).unwrap();
    for (id, label) in [
        ("clock", "Time"),
        ("sound", "Sound"),
        ("network", "Network"),
    ] {
        std::fs::write(
            apps.join(format!("{id}.desktop")),
            format!("[Desktop Entry]\nName={label}\nComment=Fixture item\nX-CosmicApplet=true\n"),
        )
        .unwrap();
    }
    for kind in [PanelKind::Panel, PanelKind::Dock] {
        let dir = fixture
            .join("config/cosmic")
            .join(kind.namespace())
            .join("v1");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("plugins_wings"), "Some(([], []))").unwrap();
        std::fs::write(dir.join("plugins_center"), "Some([])").unwrap();
    }
    let empty = "Empty. Drag an item here or use Add";
    let mut app = open_isolated_editor(&root);
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    for kind in [PanelKind::Dock, PanelKind::Panel] {
        let bar = kind.title().to_lowercase();
        let anchor = match kind {
            PanelKind::Dock => "dock-items",
            PanelKind::Panel => "panel-items",
        };
        send_settings(&mut app, Message::OpenBar(kind));
        for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
            send_settings(&mut app, Message::Resized(size));
            let mut native = crate::native_test::Native::new(size);
            native_reveal(&app, &mut native, "applets-add-start");
            // The same heading, regions, Add buttons and empty text on both bars.
            native.bounds(app.settings_view(), anchor);
            native.text_bounds(app.settings_view(), "Items");
            for gone in [
                "Contents",
                "Add items",
                "Empty · drop here",
                "Centre · 0",
                "Middle · 0",
            ] {
                assert!(
                    native.text_all(app.settings_view(), gone).is_empty(),
                    "{bar}: {gone}"
                );
            }
            for region in ["Left", "Middle", "Right"] {
                assert!(
                    !native.text_all(app.settings_view(), region).is_empty(),
                    "{bar}: {region}"
                );
            }
            for region in Region::ALL {
                native.bounds(
                    app.settings_view(),
                    &format!("applets-add-{}", region.key()),
                );
            }
            assert_eq!(
                native.text_all(app.settings_view(), "Add").len(),
                3,
                "{bar}"
            );
            assert_eq!(
                native.text_all(app.settings_view(), empty).len(),
                3,
                "{bar}"
            );
            native.text_bounds(
                app.settings_view(),
                "Nothing here yet. Use Add to put items on this bar.",
            );
            // Only the panel offers a starter layout.
            assert_eq!(
                native
                    .text_all(app.settings_view(), "Starter layout…")
                    .len(),
                usize::from(kind == PanelKind::Panel),
                "{bar}"
            );
            // The editor sits near the foot of the page.
            native.scroll(app.settings_view(), "settings-content", 100_000.);
            capture_both(
                &app,
                &mut native,
                &format!("items-{bar}-empty-{}", size.width),
            );
        }
        // The same rows once there are items.
        for (region, id) in [(Region::Start, "clock"), (Region::Center, "sound")] {
            send_settings(
                &mut app,
                Message::DesktopEditor(desktop::Message::Add(region, id.into())),
            );
        }
        assert_eq!(app.desktop.picker, None, "{bar}: Add closes the picker");
        send_settings(
            &mut app,
            Message::DesktopEditor(desktop::Message::SelectApplet(None)),
        );
        for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
            send_settings(&mut app, Message::Resized(size));
            let mut native = crate::native_test::Native::new(size);
            native_reveal(&app, &mut native, "applets-add-start");
            for control in ["drag", "select", "up", "down", "remove"] {
                for slot in ["start-0", "center-0"] {
                    native.bounds(app.settings_view(), &format!("applet-{slot}-{control}"));
                }
            }
            native.text_bounds(app.settings_view(), "Time");
            native.text_bounds(app.settings_view(), "Sound");
            assert_eq!(
                native.text_all(app.settings_view(), empty).len(),
                1,
                "{bar}"
            );
            native.text_bounds(
                app.settings_view(),
                "Drag to reorder. Press a name to move it to another region.",
            );
            assert!(native.duplicate_ids(app.settings_view()).is_empty());
            // The editor sits near the foot of the page.
            native.scroll(app.settings_view(), "settings-content", 100_000.);
            capture_both(
                &app,
                &mut native,
                &format!("items-{bar}-items-{}", size.width),
            );
        }
        // Pressing a name shows where else it can go, on either bar.
        send_settings(&mut app, Message::Resized(Size::new(820., 600.)));
        let mut native = crate::native_test::Native::new(Size::new(820., 600.));
        applet_press(&mut app, &mut native, "applet-start-0-select");
        assert_eq!(
            app.desktop.focus,
            Some(crate::desktop::panels::Slot {
                region: Region::Start,
                index: 0
            })
        );
        native.text_bounds(app.settings_view(), "Move to");
        // The region's picker opens in place and offers every installed item.
        send_settings(
            &mut app,
            Message::DesktopEditor(desktop::Message::Picker(Some(Region::End))),
        );
        native_reveal(&app, &mut native, "applet-picker-add-network");
        native.text_bounds(app.settings_view(), "Add to the Right");
        native.bounds(app.settings_view(), "applet-picker-add-clock");
        capture_both(&app, &mut native, &format!("items-{bar}-picker-820"));
        send_settings(
            &mut app,
            Message::DesktopEditor(desktop::Message::Picker(None)),
        );
    }
}

fn applet_press(app: &mut Settings, native: &mut crate::native_test::Native, id: &str) {
    use cosmic::iced::{Event, mouse};
    let at = native_reveal(app, native, id).center();
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

#[test]
fn time_of_day_without_a_city_asks_for_it_once() {
    let name = "time_of_day_without_a_city_asks_for_it_once";
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
    send_settings(&mut app, Message::Page(Page::DesktopAutomation));
    let line =
        "Choose a city so Halcyon knows when sunrise and sunset are. Until then, nothing switches.";
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        // One first step: the city search and one line.
        native.bounds(app.settings_view(), "time-of-day-first-step");
        native.bounds(app.settings_view(), "solar-city-search");
        native.text_bounds(app.settings_view(), line);
        for gone in [
            "Add where you are to begin",
            "Add your city to pick a look for each part of the day",
            "Ready once your city is added",
            "Day",
            "Evening",
            "Night",
        ] {
            assert!(
                native.text_all(app.settings_view(), gone).is_empty(),
                "{gone}"
            );
        }
        for id in ["automation-enable", "automation-shades"] {
            assert!(native.try_bounds(app.settings_view(), id).is_none(), "{id}");
        }
        // Widget styles work without a city, so the choice that leads to
        // them stays.
        native.bounds(app.settings_view(), "time-of-day-mode");
        // Set times and screens work without a city, so they stay.
        assert!(
            !native
                .text_all(app.settings_view(), "Set times and screens")
                .is_empty()
        );
        capture_both(
            &app,
            &mut native,
            &format!("time-of-day-no-city-{}", size.width),
        );
    }
    // Set times and screens still switch without a city, so the line says
    // that instead when one of them is on.
    {
        use crate::automation::{Days, ProfileRef, Rule, Time, Trigger};
        let settings = app.automation.settings_mut().expect("automation read");
        settings.enabled = true;
        settings.rules.push(Rule {
            id: "on16-evening".into(),
            enabled: true,
            since: 0,
            profile: ProfileRef::Preset("calm".into()),
            name: "Slow morning".into(),
            when: Trigger::Schedule {
                time: Time::new(19, 0).unwrap(),
                days: Days::EVERY,
            },
        });
    }
    let qualified = "Choose a city so Halcyon knows when sunrise and sunset are. Until then, only set times and screens switch.";
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        native.text_bounds(app.settings_view(), qualified);
        assert!(native.text_all(app.settings_view(), line).is_empty());
        capture_both(
            &app,
            &mut native,
            &format!("on16-time-of-day-set-times-{}", size.width),
        );
    }
    {
        let settings = app.automation.settings_mut().expect("automation read");
        settings.rules.retain(|rule| rule.id != "on16-evening");
    }
    let city = City {
        name: "Sydney".into(),
        latitude: -33.87,
        longitude: 151.21,
        timezone: Some("Australia/Sydney".into()),
        admin1: "NSW".into(),
        country: "Australia".into(),
    };
    send_settings(
        &mut app,
        Message::Automation(automation::Message::ManualCity(city.location())),
    );
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        assert!(
            native
                .try_bounds(app.settings_view(), "time-of-day-first-step")
                .is_none()
        );
        for id in ["time-of-day-mode", "automation-shades"] {
            native.bounds(app.settings_view(), id);
        }
        native.text_bounds(app.settings_view(), "Evening");
        assert!(native.text_all(app.settings_view(), line).is_empty());
        capture_both(
            &app,
            &mut native,
            &format!("time-of-day-city-{}", size.width),
        );
    }
}

#[test]
fn windows_rows_match_the_other_pages() {
    let name = "windows_rows_match_the_other_pages";
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
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    // A row title elsewhere: the dock's Floating.
    send_settings(&mut app, Message::OpenBar(PanelKind::Dock));
    native_reveal(&app, &mut native, "dock-background");
    let floating = native.text_bounds(app.settings_view(), "Floating").height;
    send_settings(&mut app, Message::Page(Page::DesktopWindows));
    let titles = [
        "Tile windows automatically",
        "Snap to screen edges",
        "Outline the active window",
        "Minimize button",
        "Maximize button",
        "Select a window by pointing at it",
        "Move the pointer to the window you switch to",
    ];
    let notes = [
        "Arrange windows side by side for you",
        "Dragged windows stick to the display edges",
        "A thin line in your accent color",
        "Handy when switching with the keyboard",
    ];
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        for title in titles {
            let found = native.text_bounds(app.settings_view(), title);
            assert_eq!(found.height, floating, "{title} uses the same title size");
        }
        // Notes sit behind "?", like every other page; the shortest is cut.
        for note in notes.into_iter().chain(["No click needed"]) {
            assert!(
                native.text_all(app.settings_view(), note).is_empty(),
                "{note}"
            );
        }
        capture_both(&app, &mut native, &format!("windows-rows-{}", size.width));
        native.scroll(app.settings_view(), "settings-content", 100_000.);
        capture_both(
            &app,
            &mut native,
            &format!("windows-rows-{}-lower", size.width),
        );
    }
    // Rows that appear with tiling, snapping and pointer focus match too.
    use crate::desktop::windows::Behavior;
    for message in [
        windows::Message::Autotile(true),
        windows::Message::EdgeSnap(true),
        windows::Message::Flag(Behavior::FocusFollowsCursor, true),
    ] {
        send_settings(&mut app, Message::Windows(message));
    }
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        for title in ["Workspaces", "Snap distance", "Delay"] {
            // "Workspaces" is also a section title further down; the row is first.
            let found = native.text_all(app.settings_view(), title)[0];
            assert_eq!(found.height, floating, "{title} uses the same title size");
        }
        assert!(
            native
                .text_all(
                    app.settings_view(),
                    "How close a window gets before it snaps"
                )
                .is_empty()
        );
        native.scroll(app.settings_view(), "settings-content", 260.);
        capture_both(
            &app,
            &mut native,
            &format!("windows-rows-{}-on", size.width),
        );
        native.scroll(app.settings_view(), "settings-content", 100_000.);
        capture_both(
            &app,
            &mut native,
            &format!("windows-rows-{}-on-lower", size.width),
        );
    }
    send_settings(
        &mut app,
        Message::Explain(view::explain::key("Tile windows automatically")),
    );
    let mut native = crate::native_test::Native::new(size);
    native.text_bounds(app.settings_view(), "Arrange windows side by side for you");

    // The folded corners, outline color and shadows section uses the same
    // rows, and its Effects disclosure the same title as the dock's.
    send_settings(
        &mut app,
        Message::Explain(view::explain::key("Tile windows automatically")),
    );
    send_settings(
        &mut app,
        Message::ToggleSection("appearance-window-details"),
    );
    let advanced = [
        "Corners, outline color and shadows",
        "Active window outline",
        "Outline width",
        "Window corners",
        "Round floating window corners",
        "Round tiled window corners",
        "Shadows on tiled windows",
        "Effects",
    ];
    let effects = [
        "Panel and dock",
        "Applet popups",
        "System interface",
        "App windows",
        "Blur strength",
        "Glass opacity",
    ];
    for open in [false, true] {
        app.system_appearance.effects = open;
        for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
            send_settings(&mut app, Message::Resized(size));
            let mut native = crate::native_test::Native::new(size);
            native_reveal(&app, &mut native, "window-corners");
            let name = if open { "-effects" } else { "" };
            capture_both(
                &app,
                &mut native,
                &format!("windows-advanced-{}{name}", size.width),
            );
            // Only what is in view is drawn: scroll the page through.
            let mut heights = std::collections::HashMap::new();
            // Where the page puts the Effects row at the top of the view.
            let mut effects_at = None;
            for step in 0..60 {
                let offset = step as f32 * 150.;
                native.scroll(app.settings_view(), "settings-content", offset);
                for title in advanced
                    .iter()
                    .chain(&effects)
                    .chain(["Frosted surfaces and blur"].iter())
                {
                    let found = native.text_all(app.settings_view(), title);
                    assert!(found.len() <= 1, "{title} is drawn once");
                    if let Some(found) = found.first() {
                        heights.insert(*title, found.height);
                        if *title == "Effects" && effects_at.is_none() {
                            effects_at = Some(offset + found.y - 120.);
                        }
                    }
                }
            }
            for title in advanced.iter().chain(effects.iter().filter(|_| open)) {
                assert_eq!(
                    heights.get(title),
                    Some(&floating),
                    "{title} uses the same title size"
                );
            }
            for title in effects.iter().filter(|_| !open) {
                assert!(!heights.contains_key(title), "{title} stays folded");
            }
            // Search names the Effects section by this line, so it stays shown.
            assert!(heights.contains_key("Frosted surfaces and blur"));
            if open {
                native.scroll(app.settings_view(), "settings-content", effects_at.unwrap());
                capture_both(
                    &app,
                    &mut native,
                    &format!("windows-advanced-{}{name}-lower", size.width),
                );
            }
        }
    }
    // Search still opens the folded rows by their ids.
    for (query, id) in [
        ("window corners", "window-corners"),
        ("blur", "appearance-effects"),
    ] {
        let hit = crate::settings::navigation::search(query)
            .into_iter()
            .find(|hit| hit.page == Page::DesktopWindows)
            .unwrap_or_else(|| panic!("{query}"));
        assert_eq!(hit.anchor(), id, "{query}");
    }
}

#[test]
fn dock_and_panel_item_changes_are_named() {
    let name = "dock_and_panel_item_changes_are_named";
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
    use crate::desktop::panels::{PanelKind, Region, Slot};
    use desktop::Message as Edit;
    let root = isolated_root();
    let fixture = root.join("cosmic-root");
    let apps = fixture.join("applications");
    std::fs::create_dir_all(&apps).unwrap();
    for (id, label) in [("clock", "Time"), ("sound", "Sound")] {
        std::fs::write(
            apps.join(format!("{id}.desktop")),
            format!("[Desktop Entry]\nName={label}\nComment=Fixture item\nX-CosmicApplet=true\n"),
        )
        .unwrap();
    }
    for kind in [PanelKind::Panel, PanelKind::Dock] {
        let dir = fixture
            .join("config/cosmic")
            .join(kind.namespace())
            .join("v1");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("plugins_wings"), "Some(([], []))").unwrap();
        std::fs::write(dir.join("plugins_center"), "Some([])").unwrap();
    }
    let mut app = open_isolated_editor(&root);
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    let items = |app: &Settings| {
        app.desktop
            .current()
            .unwrap()
            .applets()
            .known()
            .cloned()
            .unwrap()
    };
    let at = |region, index| Slot { region, index };
    let sizes = [Size::new(820., 600.), Size::new(1280., 800.)];
    for kind in [PanelKind::Dock, PanelKind::Panel] {
        let bar = kind.title();
        send_settings(&mut app, Message::OpenBar(kind));
        let steps = [
            (
                "add",
                Edit::Add(Region::Start, "sound".into()),
                "Added Sound".to_string(),
            ),
            (
                "add-time",
                Edit::Add(Region::Start, "clock".into()),
                "Added Time".into(),
            ),
            (
                "reorder",
                Edit::Step(at(Region::Start, 0), 1),
                format!("Reordered {bar} items"),
            ),
            // Past the region's end, the arrow moves it to the next one.
            (
                "step-across",
                Edit::Step(at(Region::Start, 1), 1),
                "Moved Sound to Middle".into(),
            ),
            (
                "move",
                Edit::MoveTo(at(Region::Start, 0), Region::End),
                "Moved Time to Right".into(),
            ),
            (
                "remove",
                Edit::Remove(at(Region::End, 0)),
                "Removed Time".into(),
            ),
        ];
        for (step, edit, title) in steps {
            let before = items(&app);
            send_settings(&mut app, Message::DesktopEditor(edit.clone()));
            assert!(!app.error, "{bar} {step}: {}", app.status);
            let after = items(&app);
            assert_ne!(after, before, "{bar} {step}");
            // The footer, its Undo and Changes all use the same words.
            let summary = app.last_desktop_summary.clone().unwrap();
            assert_eq!(summary.title(), title, "{bar} {step}");
            assert_eq!(
                app.last_desktop_change.as_ref().map(|c| c.1.as_str()),
                Some(title.as_str()),
                "{bar} {step}"
            );
            send_settings(&mut app, Message::Support(support::Message::Changed));
            assert_eq!(
                app.support.local.recent.entries.last().map(|e| e.title()),
                Some(title.clone()),
                "{bar} {step}"
            );
            for size in sizes {
                send_settings(&mut app, Message::Resized(size));
                let mut native = crate::native_test::Native::new(size);
                native.text_bounds(app.settings_view(), &format!("Saved · {title}"));
                native.text_bounds(app.settings_view(), &format!("Undo {title}"));
                assert!(
                    native
                        .text_all(app.settings_view(), "Dock and panel change")
                        .is_empty(),
                    "{bar} {step}"
                );
                // The item editor sits near the foot of the page.
                native.scroll(app.settings_view(), "settings-content", 100_000.);
                capture_both(
                    &app,
                    &mut native,
                    &format!("item-footer-{}-{step}-{}", bar.to_lowercase(), size.width),
                );
            }
            // Its Undo puts back exactly that change, then it is made again.
            let mut native = crate::native_test::Native::new(sizes[1]);
            native_click(&mut app, &mut native, "settings-undo");
            assert!(!app.error, "{bar} {step}: {}", app.status);
            assert_eq!(items(&app), before, "{bar} {step}: undo");
            send_settings(&mut app, Message::DesktopEditor(edit));
            assert_eq!(items(&app), after, "{bar} {step}: again");
            assert_eq!(app.last_desktop_summary.as_ref().unwrap().title(), title);
        }
    }
    // Other edits keep their own names.
    let spacing = app
        .desktop
        .current()
        .unwrap()
        .spacing()
        .known()
        .copied()
        .unwrap_or(0);
    send_settings(&mut app, Message::DesktopEditor(Edit::Spacing(spacing + 4)));
    send_settings(&mut app, Message::DesktopEditor(Edit::Apply));
    assert!(
        app.last_desktop_summary
            .as_ref()
            .unwrap()
            .title()
            .starts_with("Between items: ")
    );
    assert!(app.last_desktop_summary.as_ref().unwrap().item.is_none());

    // Changes lists them by the same names.
    send_settings(&mut app, Message::Page(Page::Changes));
    send_settings(&mut app, Message::Support(support::Message::Changed));
    for size in sizes {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        for title in [
            "Removed Time",
            "Moved Time to Right",
            "Moved Sound to Middle",
        ] {
            assert!(
                !native.text_all(app.settings_view(), title).is_empty(),
                "{title}"
            );
        }
        capture_both(&app, &mut native, &format!("item-changes-{}", size.width));
    }
}

#[test]
fn windows_page_saves_its_corner_and_effect_controls() {
    let name = "windows_page_saves_its_corner_and_effect_controls";
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
    use cosmic::cosmic_theme::Roundness;
    use system_appearance::Message as Look;
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    send_settings(&mut app, Message::Page(Page::DesktopWindows));
    send_settings(
        &mut app,
        Message::ToggleSection("appearance-window-details"),
    );
    // Effects opens in place.
    send_settings(&mut app, Message::SystemAppearance(Look::Effects(true)));
    assert!(app.system_appearance.effects);
    // Window corners saves from here, as on Appearance.
    let roundness = |app: &Settings| {
        let variant = app.system_appearance.variant;
        app.system_appearance
            .draft()
            .unwrap()
            .roundness(variant)
            .known()
            .copied()
    };
    let other = if roundness(&app) == Some(Roundness::Square) {
        Roundness::Round
    } else {
        Roundness::Square
    };
    send_settings(&mut app, Message::SystemAppearance(Look::Roundness(other)));
    assert!(!app.error, "{}", app.status);
    assert_eq!(roundness(&app), Some(other));
    assert_eq!(
        app.last_desktop_change.as_ref().map(|c| c.1.as_str()),
        Some("Window corners")
    );
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    native.text_bounds(app.settings_view(), "Undo Window corners");
    // A fresh read sees it saved.
    let mut reopened = open_isolated_editor(&root);
    send_settings(&mut reopened, Message::Page(Page::DesktopWindows));
    assert_eq!(roundness(&reopened), Some(other));
}

/// FB-05: after Halcyon was closed in the middle of a change, the banner
/// says what was being changed, and its choices say what they do.
#[test]
fn interrupted_change_banner_names_what_was_changing() {
    let name = "interrupted_change_banner_names_what_was_changing";
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
    use crate::desktop::store::{Journal, JournalEntry};
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    let store = app.desktop.store().unwrap().clone();
    let journal = Journal {
        format: "desktop-cards-journal".into(),
        version: 1,
        id: "interrupted".into(),
        operation: "apply".into(),
        created: "2026-11-11T12:00:00Z".into(),
        entries: vec![JournalEntry {
            namespace: "com.system76.CosmicComp".into(),
            version: 1,
            key: "autotile".into(),
            before: Some("false".into()),
            after: Some("true".into()),
        }],
        widget: None,
        undo: None,
        design: None,
    };
    std::fs::create_dir_all(store.state_dir()).unwrap();
    std::fs::write(
        store.state_dir().join("desktop-journal.json"),
        serde_json::to_vec(&journal).unwrap(),
    )
    .unwrap();
    assert!(store.pending_recovery().unwrap().is_some());
    send_settings(&mut app, Message::Page(Page::DesktopWindows));
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        native.text_bounds(
            app.settings_view(),
            "Halcyon was closed while changing Windows.",
        );
        native.text_bounds(app.settings_view(), "Keep what’s on screen");
        native.text_bounds(app.settings_view(), "Go back to before");
        capture_both(&app, &mut native, &format!("interrupted-windows-{}", size.width));
    }
    let mut native = crate::native_test::Native::new(Size::new(1280., 800.));
    native_click(&mut app, &mut native, "recovery-keep");
    assert!(store.pending_recovery().unwrap().is_none(), "keeping clears it");
    assert!(native.try_bounds(app.settings_view(), "recovery-keep").is_none());
    assert!(app.workspaces.recovery().is_none(), "Workspaces below follows");
}

/// The Dock and panel picture is a close-up of the selected bar on its own
/// edge, at a size where its applets read, for every edge and style.
#[test]
fn previews_dock_and_panel_closeups_show_the_bar_at_a_readable_size() {
    let name = "previews_dock_and_panel_closeups_show_the_bar_at_a_readable_size";
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
    use crate::desktop::panels::{Anchor, AutoHide, PanelKind};
    let root = isolated_root();
    let cosmic = root.join("cosmic-root");
    seed_real_bars(&cosmic);
    let shots = [
        (Size::new(820., 600.), "820x600"),
        (Size::new(1280., 800.), "1280x800"),
    ];
    let capture = |app: &mut Settings, case: &str| {
        for kind in [PanelKind::Dock, PanelKind::Panel] {
            send_settings(app, Message::DesktopEditor(desktop::Message::Select(kind)));
            let tab = match kind {
                PanelKind::Dock => "dock",
                PanelKind::Panel => "panel",
            };
            for (size, label) in shots {
                send_settings(app, Message::Resized(size));
                let mut native = crate::native_test::Native::new(size);
                assert!(native.duplicate_ids(app.settings_view()).is_empty());
                let scene = native.bounds(app.settings_view(), "desktop-scene");
                // A band with room for the bar, not a letterbox.
                assert!(scene.height >= 110., "{case} {tab} {label}: {scene:?}");
                assert!(scene.height <= 300., "{case} {tab} {label}: {scene:?}");
                for (theme, mode) in [
                    (cosmic::Theme::light(), "light"),
                    (cosmic::Theme::dark(), "dark"),
                ] {
                    native.capture(
                        app.settings_view(),
                        &theme,
                        &format!("previews-{case}-{tab}-{label}-{mode}"),
                    );
                }
            }
        }
    };
    let mut app = open_isolated_editor(&root);
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    let dock = app.desktop.draft(PanelKind::Dock).unwrap();
    assert_eq!(dock.anchor().known(), Some(&Anchor::Bottom));
    assert!(
        app.desktop
            .catalog
            .icons("com.system76.CosmicAppList")
            .len()
            > 3
    );
    capture(&mut app, "top-panel-bottom-dock");

    // A changed setting shows at once: the dock moves to the left edge.
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Select(PanelKind::Dock)),
    );
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Anchor(Anchor::Left)),
    );
    capture(&mut app, "left-dock");

    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Select(PanelKind::Dock)),
    );
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Anchor(Anchor::Bottom)),
    );
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::AutoHide(AutoHide::Always)),
    );
    capture(&mut app, "auto-hide-dock");
    send_settings(&mut app, Message::DesktopEditor(desktop::Message::Discard));
    drop(app);

    // The panel split into islands.
    let key = crate::desktop::islands::key(PanelKind::Panel);
    let on = dock_islands_core::Settings {
        enabled: true,
        ..Default::default()
    };
    seed(
        &cosmic.join("config/cosmic"),
        &[(key.namespace(), key.key(), &ron::to_string(&on).unwrap())],
    );
    let mut app = open_isolated_editor(&root);
    let store = app.desktop.store().unwrap().clone();
    crate::desktop::islands::fixture_installed(store.fixture_root().unwrap());
    app.desktop.islands.installation = crate::desktop::islands::Installation::discover(&store);
    app.desktop.islands.compatible = true;
    assert!(app.desktop.islands.installation.installed());
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    capture(&mut app, "segmented-panel");
}

/// A top panel and a bottom dock holding the real applets and pinned apps,
/// read from the installed entries, so close-ups draw their real icons.
fn seed_real_bars(cosmic: &std::path::Path) {
    // The real applets and pinned apps, read from the installed entries.
    let apps = cosmic.join("applications");
    std::fs::create_dir_all(&apps).unwrap();
    let pinned = [
        "firefox",
        "com.system76.CosmicFiles",
        "com.system76.CosmicTerm",
        "com.system76.CosmicEdit",
        "com.system76.CosmicStore",
        "com.system76.CosmicSettings",
    ];
    let applets = [
        "com.system76.CosmicAppList",
        "com.system76.CosmicAppletWorkspaces",
        "com.system76.CosmicAppletTime",
        "com.system76.CosmicAppletNetwork",
        "com.system76.CosmicAppletAudio",
        "com.system76.CosmicAppletBattery",
        "com.system76.CosmicAppletNotifications",
        "com.system76.CosmicAppletPower",
        "com.system76.CosmicPanelAppButton",
        "com.system76.CosmicPanelLauncherButton",
        "com.system76.CosmicPanelWorkspacesButton",
    ];
    for id in pinned.iter().chain(&applets) {
        let entry =
            std::path::Path::new("/usr/share/applications").join(format!("{id}.desktop"));
        let text = std::fs::read_to_string(&entry).unwrap_or_else(|_| {
            format!("[Desktop Entry]\nName={id}\nIcon={id}\nX-CosmicApplet=true\n")
        });
        std::fs::write(apps.join(format!("{id}.desktop")), text).unwrap();
    }
    let favorites = format!("{:?}", pinned);
    seed(
        &cosmic.join("config/cosmic"),
        &[
            ("com.system76.CosmicAppList", "favorites", &favorites),
            ("com.system76.CosmicPanel.Panel", "size", "XS"),
            (
                "com.system76.CosmicPanel.Panel",
                "plugins_wings",
                "Some(([\"com.system76.CosmicAppletWorkspaces\"],[\"com.system76.CosmicAppletNetwork\",\"com.system76.CosmicAppletAudio\",\"com.system76.CosmicAppletBattery\",\"com.system76.CosmicAppletNotifications\",\"com.system76.CosmicAppletPower\"]))",
            ),
            (
                "com.system76.CosmicPanel.Panel",
                "plugins_center",
                "Some([\"com.system76.CosmicAppletTime\"])",
            ),
            ("com.system76.CosmicPanel.Dock", "anchor", "Bottom"),
            ("com.system76.CosmicPanel.Dock", "expand_to_edges", "false"),
            ("com.system76.CosmicPanel.Dock", "anchor_gap", "true"),
            ("com.system76.CosmicPanel.Dock", "margin", "8"),
            ("com.system76.CosmicPanel.Dock", "border_radius", "16"),
            ("com.system76.CosmicPanel.Dock", "plugins_wings", "None"),
            (
                "com.system76.CosmicPanel.Dock",
                "plugins_center",
                "Some([\"com.system76.CosmicPanelLauncherButton\",\"com.system76.CosmicPanelWorkspacesButton\",\"com.system76.CosmicPanelAppButton\",\"com.system76.CosmicAppList\"])",
            ),
        ],
    );
}

/// Setup's Dock and panel step shows both bars close up, with their real
/// icons and names, as each choice on the step would leave them; the later
/// steps draw the same bars on their whole-desktop miniature.
#[test]
fn previews_wizard_bars_step_shows_each_choice_close_up() {
    let name = "previews_wizard_bars_step_shows_each_choice_close_up";
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
    use wizard::{Message as Step, Shape, Step as Screen};
    let root = isolated_root();
    let cosmic = root.join("cosmic-root");
    seed_real_bars(&cosmic);
    crate::desktop::islands::fixture_installed(&cosmic);
    OPEN_WIZARD.store(true, std::sync::atomic::Ordering::Relaxed);
    let (mut app, initial) = Settings::init(Core::default(), Page::Overview);
    settle_settings(&mut app, initial);
    // Setup reads the bars for its pictures without opening their page.
    assert!(app.desktop.opened());
    let store = app.desktop.store().unwrap().clone();
    app.desktop.islands.installation = crate::desktop::islands::Installation::discover(&store);
    assert!(app.desktop.islands.installation.installed());
    app.wizard.as_mut().unwrap().islands_available = true;
    for step in [Step::Start, Step::Look("calm"), Step::Next] {
        send_settings(&mut app, Message::Wizard(step));
    }
    assert_eq!(app.wizard.as_ref().unwrap().step, Screen::Bars);
    let shots = [
        (Size::new(820., 600.), "820x600"),
        (Size::new(1280., 800.), "1280x800"),
    ];
    let capture = |app: &mut Settings, case: &str| {
        for (size, label) in shots {
            send_settings(app, Message::Resized(size));
            let mut native = crate::native_test::Native::new(size);
            assert!(native.duplicate_ids(app.settings_view()).is_empty());
            native.scroll(app.settings_view(), "wizard-body", 0.);
            let picture = native.bounds(app.settings_view(), "wizard-bars-preview");
            let footer = native.bounds(app.settings_view(), "wizard-next");
            assert!(
                picture.x >= 0. && picture.x + picture.width <= size.width,
                "{case} {label}: {picture:?}"
            );
            assert!(picture.y >= 0. && picture.y < footer.y, "{case} {label}");
            assert!(picture.height >= 150., "{case} {label}: {picture:?}");
            // Both bars say what they are.
            let words = native.all_text(app.settings_view());
            for bar in ["Dock", "Panel"] {
                assert!(
                    words.iter().any(|t| t.starts_with(bar)),
                    "{case} {label}: {words:?}"
                );
            }
            for (theme, mode) in [
                (cosmic::Theme::light(), "light"),
                (cosmic::Theme::dark(), "dark"),
            ] {
                native.capture(
                    app.settings_view(),
                    &theme,
                    &format!("wizard-bars-{case}-{label}-{mode}"),
                );
            }
        }
    };
    capture(&mut app, "keep");
    send_settings(&mut app, Message::Wizard(Step::KeepBars(false)));
    capture(&mut app, "change");
    let choices: Vec<(String, Step)> = vec![
        ("panel-islands".into(), Step::Islands(true)),
        ("panel-bar".into(), Step::Islands(false)),
    ]
    .into_iter()
    .chain(Shape::ALL.into_iter().map(|shape| {
        (
            format!("panel-{}", shape.title().to_lowercase()),
            Step::PanelShape(shape),
        )
    }))
    .chain(Shape::ALL.into_iter().map(|shape| {
        (
            format!("dock-{}", shape.title().to_lowercase()),
            Step::DockShape(shape),
        )
    }))
    .chain([
        ("dock-attached".into(), Step::Floating(false)),
        ("dock-floating".into(), Step::Floating(true)),
        ("dock-hides".into(), Step::Hides(true)),
        ("dock-shown".into(), Step::Hides(false)),
    ])
    .collect();
    for (case, step) in choices {
        send_settings(&mut app, Message::Wizard(step));
        capture(&mut app, &case);
    }
    // The widget and review pictures draw the same bars, to scale.
    send_settings(&mut app, Message::Wizard(Step::Islands(true)));
    for (screen, case) in [(Screen::Widgets, "widgets"), (Screen::Ready, "review")] {
        send_settings(&mut app, Message::Wizard(Step::Show(screen)));
        for (size, label) in shots {
            send_settings(&mut app, Message::Resized(size));
            let mut native = crate::native_test::Native::new(size);
            for (theme, mode) in [
                (cosmic::Theme::light(), "light"),
                (cosmic::Theme::dark(), "dark"),
            ] {
                native.capture(
                    app.settings_view(),
                    &theme,
                    &format!("wizard-{case}-{label}-{mode}"),
                );
            }
        }
    }
}
