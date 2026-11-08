// Appearance's one Corners choice, and Match style as the way the dock and
// panel follow the look's style again.

/// The corners each surface has now: widget cards, the dock, the panel and
/// windows.
fn surface_corners(
    store: &crate::desktop::DesktopStore,
) -> (
    u16,
    crate::desktop::panels::Setting<u32>,
    crate::desktop::panels::Setting<u32>,
    Option<cosmic::cosmic_theme::Roundness>,
) {
    use crate::desktop::panels::{PanelDraft, PanelKind};
    let windows = crate::desktop::appearance::AppearanceDraft::load(store).unwrap();
    (
        Config::load().appearance.corner_radius,
        PanelDraft::load(store, PanelKind::Dock).unwrap().radius(),
        PanelDraft::load(store, PanelKind::Panel).unwrap().radius(),
        windows.roundness(windows.active_variant()).known().copied(),
    )
}

#[test]
fn corners_is_one_change_for_every_surface_and_one_undo() {
    let name = "corners_is_one_change_for_every_surface_and_one_undo";
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
    use crate::desktop::panels::{MAX_RADIUS, Setting};
    use cosmic::cosmic_theme::Roundness;
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    let store = crate::desktop::actions::history_store().unwrap();
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    let before = surface_corners(&store);
    let config = std::fs::read(Config::path().unwrap()).unwrap();
    let changes = store.recent_changes().unwrap().entries.len();

    // Opening Appearance after an upgrade changes nothing: saved corners
    // only read as a step, and surfaces that differ read as Mixed.
    send_settings(&mut app, Message::Page(Page::Themes));
    native_reveal(&app, &mut native, "appearance-corners");
    assert_eq!(app.corner_step, None, "{before:?}");
    native.text_bounds(app.settings_view(), "Mixed");
    assert_eq!(surface_corners(&store), before);
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), config);
    assert_eq!(store.recent_changes().unwrap().entries.len(), changes);

    // Pill: one change sets every surface. Windows can't be pill-shaped, so
    // they turn very round.
    native_click(&mut app, &mut native, "corners-pill");
    assert!(!app.error, "{:?}", app.notice);
    assert_eq!(
        surface_corners(&store),
        (
            32,
            Setting::Known(MAX_RADIUS),
            Setting::Known(MAX_RADIUS),
            Some(Roundness::Round)
        )
    );
    assert_eq!(store.recent_changes().unwrap().entries.len(), changes + 1);
    assert_eq!(store.latest_change().unwrap().unwrap().label, "Corners");
    assert_eq!(app.corner_step, Some(crate::settings::corners::Step::Pill));
    assert!(native.text_all(app.settings_view(), "Mixed").is_empty());
    // Each surface's own setting shows the same step.
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Advanced(true)),
    );
    native_reveal(&app, &mut native, "dock-corners");
    assert!(
        native
            .text_all(app.settings_view(), &format!("Custom · {MAX_RADIUS} px"))
            .is_empty()
    );

    // One Undo, on the page that made the change, puts every surface back.
    send_settings(&mut app, Message::Page(Page::Themes));
    native_click(&mut app, &mut native, "settings-undo");
    assert!(!app.error, "{:?}", app.notice);
    assert_eq!(surface_corners(&store), before);
    assert_eq!(app.corner_step, None);

    // Square reaches windows too.
    native_click(&mut app, &mut native, "corners-square");
    assert_eq!(
        surface_corners(&store),
        (
            0,
            Setting::Known(0),
            Setting::Known(0),
            Some(Roundness::Square)
        )
    );
    // Choosing the step already shown records nothing.
    let changes = store.recent_changes().unwrap().entries.len();
    native_click(&mut app, &mut native, "corners-square");
    assert_eq!(store.recent_changes().unwrap().entries.len(), changes);
}

#[test]
fn a_size_set_elsewhere_shows_as_custom_in_its_own_setting() {
    let name = "a_size_set_elsewhere_shows_as_custom_in_its_own_setting";
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
    use crate::desktop::panels::PanelKind;
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    // The fixture's bars are 8 px and widget cards 24 px: neither is a step.
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Select(PanelKind::Dock)),
    );
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Advanced(true)),
    );
    native_reveal(&app, &mut native, "dock-corners");
    native.text_bounds(app.settings_view(), "Dock corners");
    native.text_bounds(app.settings_view(), "Custom · 8 px");
    send_settings(&mut app, Message::Page(Page::Appearance));
    send_settings(&mut app, Message::ToggleAdvanced(Page::Appearance));
    native_reveal(&app, &mut native, "widget-corners");
    native.text_bounds(app.settings_view(), "Custom · 24 px");
    // The dock has no corner slider and no apologetic notes any more.
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    assert!(
        native
            .text_all(app.settings_view(), "Corner radius")
            .is_empty()
    );
    assert!(
        native
            .text_all(
                app.settings_view(),
                "Only the dock. Window corners are in Windows"
            )
            .is_empty()
    );
}

#[test]
fn match_style_follows_the_look_and_keeps_its_values() {
    let name = "match_style_follows_the_look_and_keeps_its_values";
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
    use crate::desktop::panels::{Background, PanelDraft, PanelKind, Setting};
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    let store = crate::desktop::actions::history_store().unwrap();
    let size = Size::new(1280., 800.);
    send_settings(&mut app, Message::Resized(size));
    let mut native = crate::native_test::Native::new(size);
    // A built-in look styles the bars.
    send_settings(&mut app, Message::Page(Page::Themes));
    let _ = app
        .profiles
        .update(profiles::Message::Select(profiles::Selection::Preset(
            "theme-outline",
        )));
    for job in app.profiles.take_theme_jobs() {
        send_settings(&mut app, Message::Profiles(job.run()));
    }
    assert!(app.profiles.can_apply(), "{:?}", app.profiles.notice);
    send_settings(&mut app, Message::Profiles(profiles::Message::Apply));
    let bar = |kind| {
        let draft = PanelDraft::load(&store, kind).unwrap();
        (
            draft.radius(),
            draft.opacity(),
            draft.padding(),
            draft.spacing(),
        )
    };
    let (_, radius, _, _) = crate::desktop_theme::Family::Outline.bar();
    assert_eq!(bar(PanelKind::Dock).0, Setting::Known(radius));
    let look = (bar(PanelKind::Dock), bar(PanelKind::Panel));

    // Someone picked Dark, and an earlier version's switch stopped styles
    // restyling the bars. Other widget appearance settings are their own.
    send_settings(&mut app, Message::Page(Page::DesktopDock));
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Select(PanelKind::Dock)),
    );
    send_settings(
        &mut app,
        Message::DesktopEditor(desktop::Message::Background(Background::Dark)),
    );
    app.config =
        Config::save_appearance(json!({"style_bars": false, "opacity": 61, "border_opacity": 33}))
            .unwrap();
    app.profiles.set_style_bars(false);
    let widgets = Config::load().appearance;
    let changes = store.recent_changes().unwrap().entries.len();
    native_reveal(&app, &mut native, "dock-background");
    native.text_bounds(
        app.settings_view(),
        "Choosing a style leaves the dock and panel as they are",
    );

    // Match style: one change brings back styles restyling the bars and
    // keeps everything the look set.
    native_click(&mut app, &mut native, "native-background-match");
    assert!(!app.error, "{:?}", app.notice);
    let saved = Config::load().appearance;
    assert!(saved.style_bars);
    assert_eq!(
        (
            saved.opacity,
            saved.border_opacity,
            saved.corner_radius,
            saved.family
        ),
        (
            widgets.opacity,
            widgets.border_opacity,
            widgets.corner_radius,
            widgets.family
        )
    );
    let dock = PanelDraft::load(&store, PanelKind::Dock).unwrap();
    assert_eq!(dock.background(), Setting::Known(Background::ThemeDefault));
    assert_eq!((bar(PanelKind::Dock), bar(PanelKind::Panel)), look);
    // Match style is not a choice a style keeps: the bars follow it again.
    assert_eq!(saved.overrides & (1 << 12), 0);
    assert_eq!(store.recent_changes().unwrap().entries.len(), changes + 1);
    native.text_bounds(app.settings_view(), "Match style");
    assert!(
        native
            .text_all(
                app.settings_view(),
                "Choosing a style leaves the dock and panel as they are"
            )
            .is_empty()
    );

    // One Undo puts back Dark and the earlier preference together.
    native_click(&mut app, &mut native, "settings-undo");
    assert!(!app.error, "{:?}", app.notice);
    let dock = PanelDraft::load(&store, PanelKind::Dock).unwrap();
    assert_eq!(dock.background(), Setting::Known(Background::Dark));
    assert!(!Config::load().appearance.style_bars);
}

// The pages Corners touches, and the searches that reach them, at both
// window sizes in both themes.
#[test]
fn corners_pages_and_searches_render_at_both_sizes() {
    let name = "corners_pages_and_searches_render_at_both_sizes";
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
    use crate::desktop::panels::PanelKind;
    let root = isolated_root();
    let mut app = open_isolated_editor(&root);
    for size in [Size::new(820., 600.), Size::new(1280., 800.)] {
        send_settings(&mut app, Message::Resized(size));
        let mut native = crate::native_test::Native::new(size);
        let mut capture = |app: &Settings, native: &mut crate::native_test::Native, what: &str| {
            assert!(
                native.duplicate_ids(app.settings_view()).is_empty(),
                "{what}"
            );
            assert!(
                native.overflowing_text(app.settings_view()).is_empty(),
                "{what}"
            );
            for (theme, mode) in [
                (cosmic::Theme::light(), "light"),
                (cosmic::Theme::dark(), "dark"),
            ] {
                native.capture(
                    app.settings_view(),
                    &theme,
                    &format!("g2-{what}-{}-{mode}", size.width),
                );
            }
        };
        send_settings(&mut app, Message::Page(Page::Themes));
        native_reveal(&app, &mut native, "appearance-corners");
        capture(&app, &mut native, "appearance");
        for (kind, what) in [(PanelKind::Dock, "dock"), (PanelKind::Panel, "panel")] {
            send_settings(&mut app, Message::Page(Page::DesktopDock));
            send_settings(
                &mut app,
                Message::DesktopEditor(desktop::Message::Select(kind)),
            );
            send_settings(
                &mut app,
                Message::DesktopEditor(desktop::Message::Advanced(false)),
            );
            native_reveal(&app, &mut native, "dock-background");
            capture(&app, &mut native, what);
            send_settings(
                &mut app,
                Message::DesktopEditor(desktop::Message::Advanced(true)),
            );
            native_reveal(&app, &mut native, "dock-corners");
            capture(&app, &mut native, &format!("{what}-advanced"));
        }
        send_settings(&mut app, Message::Page(Page::DesktopWindows));
        app.open_sections.insert("appearance-window-details");
        native_reveal(&app, &mut native, "window-corners");
        capture(&app, &mut native, "windows");
        for query in ["dark", "corners", "spacing"] {
            send_settings(&mut app, Message::Page(Page::Overview));
            send_settings(&mut app, Message::SearchSettings(query.into()));
            let label = navigation::search(query)[0].label;
            assert!(
                !native.text_all(app.settings_view(), label).is_empty(),
                "{query}"
            );
            capture(&app, &mut native, &format!("search-{query}"));
            send_settings(&mut app, Message::SearchSettings(String::new()));
        }
    }
    // The first result for each names the row it opens, and where it is.
    let first = |query: &str| {
        let hit = navigation::search(query)[0];
        (hit.label, hit.crumb())
    };
    assert_eq!(first("dark"), ("Mode", "Appearance".into()));
    assert_eq!(first("corners"), ("Corners", "Appearance".into()));
    assert_eq!(
        first("spacing"),
        (
            "Spacing",
            "Dock and panel › Dock › Corners, spacing and timing".into()
        )
    );
}
