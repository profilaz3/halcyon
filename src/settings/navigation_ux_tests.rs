use super::*;
use crate::config::Config;
use crate::desktop::profiles::Domain;
use crate::displays::test_support::{Wire, info, isolated};
use crate::drawer::Widget;
use crate::native_test::Native;
use crate::settings::tests::{send_settings, settle_settings};
use crate::settings::{canvas, placement, profiles};
use cosmic::iced::{Event, Size, mouse};
use cosmic::{Application, app::Core};

fn app(page: Page) -> Settings {
    std::fs::write(
        Config::path().unwrap(),
        r#"{"setup_dismissed":true,"glance":{"enabled":false},"enabled_widgets":["clock","weather","notes"],"future_preference":37}"#,
    ).unwrap();
    let (mut app, initial) = Settings::init(Core::default(), page);
    settle_settings(&mut app, initial);
    app
}

fn click(app: &mut Settings, native: &mut Native, id: &str) {
    let point = crate::settings::tests::native_reveal(app, native, id).center();
    for event in [
        mouse::Event::CursorMoved { position: point },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        for message in native
            .event(app.settings_view(), Event::Mouse(event), point)
            .0
        {
            send_settings(app, message);
        }
    }
}

#[test]
fn setup_navigation_keeps_reviews_and_root_returns_to_browse() {
    if isolated(
        "settings::navigation::ux_regressions::setup_navigation_keeps_reviews_and_root_returns_to_browse",
    ) {
        return;
    }
    let mut app = app(Page::DesktopProfiles);
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    send_settings(
        &mut app,
        Message::Profiles(profiles::Message::Select(profiles::Selection::Preset(
            "paper",
        ))),
    );
    send_settings(
        &mut app,
        Message::Profiles(profiles::Message::Include(Domain::Wallpaper, false)),
    );
    assert!(app.profiles.touched());
    let selection = app.profiles.selection().clone();
    let mut native = Native::new(Size::new(1280., 900.));
    // The header's Back link ("Looks") leaves the review and keeps it.
    click(&mut app, &mut native, "settings-back");
    assert!(!app.profiles.review_open());
    assert!(
        app.profiles
            .parked()
            .iter()
            .any(|p| p.selection() == &selection)
    );
    send_settings(
        &mut app,
        Message::Profiles(profiles::Message::Select(profiles::Selection::Preset(
            "calm",
        ))),
    );
    send_settings(
        &mut app,
        Message::Profiles(profiles::Message::Include(Domain::Wallpaper, false)),
    );
    send_settings(
        &mut app,
        Message::ResumeKept(KeptTarget::Setup(selection.clone())),
    );
    assert!(
        app.profiles
            .parked()
            .iter()
            .any(|p| p.selection() == &profiles::Selection::Preset("calm")),
        "resuming another review preserves the current one"
    );
    let task = app.open_root(Page::DesktopProfiles);
    settle_settings(&mut app, task);
    send_settings(
        &mut app,
        Message::ResumeKept(KeptTarget::Setup(selection.clone())),
    );
    assert_eq!(app.profiles.selection(), &selection);
    let task = app.open_root(Page::DesktopProfiles);
    settle_settings(&mut app, task);
    assert!(!app.profiles.review_open());
    assert!(!app.profiles.saved_collection);
    assert!(
        app.profiles
            .parked()
            .iter()
            .any(|p| p.selection() == &selection)
    );
    send_settings(&mut app, Message::Page(Page::Clock));
    let task = app.open_root(Page::DesktopProfiles);
    settle_settings(&mut app, task);
    assert!(
        !app.profiles.review_open(),
        "a root click never resumes the parked review"
    );
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}

#[test]
fn back_from_setup_review_reaches_its_named_search_destination() {
    if isolated(
        "settings::navigation::ux_regressions::back_from_setup_review_reaches_its_named_search_destination",
    ) {
        return;
    }
    let mut app = app(Page::Clock);
    send_settings(&mut app, Message::SearchSettings("setups".into()));
    send_settings(&mut app, Message::SearchFirst);
    send_settings(
        &mut app,
        Message::Profiles(profiles::Message::Select(profiles::Selection::Preset(
            "paper",
        ))),
    );
    send_settings(
        &mut app,
        Message::Profiles(profiles::Message::Include(Domain::Wallpaper, false)),
    );
    assert_eq!(app.back_title().as_deref(), Some("Search results"));
    send_settings(&mut app, Message::Back);
    assert_eq!(app.search_query, "setups");
    assert_eq!(app.page, Page::Clock);
    assert!(
        app.profiles
            .parked()
            .iter()
            .any(|p| p.selection() == &profiles::Selection::Preset("paper"))
    );
    send_settings(&mut app, Message::SearchSettings(String::new()));
    send_settings(&mut app, Message::SetupsTab(true));
    send_settings(&mut app, Message::Page(Page::Weather));
    send_settings(&mut app, Message::Back);
    assert_eq!(app.page, Page::DesktopProfiles);
    assert!(app.profiles.saved_collection, "Back restores the Saved tab");
    assert!(
        !app.profiles.review_open(),
        "Back restores the list rather than opening a retained review"
    );
    send_settings(&mut app, Message::SearchSettings("recent changes".into()));
    send_settings(&mut app, Message::SearchFirst);
    assert!(app.support.recent_open);
    assert_eq!(app.back_title().as_deref(), Some("Search results"));
    send_settings(&mut app, Message::Back);
    assert_eq!(app.search_query, "recent changes");
    assert!(!app.support.recent_open);
}

#[test]
fn theme_browser_and_bar_objects_follow_named_history() {
    if isolated(
        "settings::navigation::ux_regressions::theme_browser_and_bar_objects_follow_named_history",
    ) {
        return;
    }
    let mut app = app(Page::Clock);
    send_settings(&mut app, Message::Page(Page::Themes));
    send_settings(&mut app, Message::ContentScrolled(120.));
    // Showing every style is part of the page, not a step in its history.
    send_settings(&mut app, Message::ToggleSection("appearance-all-styles"));
    assert_eq!(app.page_scroll.get(Page::Themes.key()), Some(&120.));
    assert_eq!(app.back_title().as_deref(), Some("Clock"));
    send_settings(
        &mut app,
        Message::OpenBar(crate::desktop::panels::PanelKind::Panel),
    );
    send_settings(
        &mut app,
        Message::OpenBar(crate::desktop::panels::PanelKind::Dock),
    );
    assert_eq!(app.back_title().as_deref(), Some("Panel"));
    send_settings(&mut app, Message::Back);
    assert_eq!(
        app.desktop.selected,
        crate::desktop::panels::PanelKind::Panel
    );
    for (query, bar) in [
        ("dock", crate::desktop::panels::PanelKind::Dock),
        ("panel", crate::desktop::panels::PanelKind::Panel),
    ] {
        send_settings(&mut app, Message::SearchResult(search(query)[0]));
        assert_eq!(app.desktop.selected, bar);
    }
}

#[test]
fn moved_design_search_controls_and_position_content_link_are_reachable() {
    if isolated(
        "settings::navigation::ux_regressions::moved_design_search_controls_and_position_content_link_are_reachable",
    ) {
        return;
    }
    let mut app = app(Page::Clock);
    for query in ["analog", "clock weight", "clock surface", "glance font"] {
        let hit = search(query)[0];
        send_settings(&mut app, Message::SearchResult(hit));
        assert_eq!(app.object_tab, ObjectTab::Design, "{query}");
        let mut native = Native::new(Size::new(1280., 900.));
        assert!(
            native
                .try_bounds(app.settings_view(), hit.anchor())
                .is_some(),
            "{query}"
        );
    }
    send_settings(&mut app, Message::Page(Page::Clock));
    send_settings(&mut app, Message::ObjectTab(ObjectTab::Position));
    let mut native = Native::new(Size::new(1280., 900.));
    click(&mut app, &mut native, "layout-content-settings");
    assert_eq!(app.object_tab, ObjectTab::Content);
    assert_eq!(app.page, Page::Clock);
}

#[test]
fn position_location_is_previewed_recovered_and_applied_with_one_undo() {
    if isolated(
        "settings::navigation::ux_regressions::position_location_is_previewed_recovered_and_applied_with_one_undo",
    ) {
        return;
    }
    let mut app = app(Page::Clock);
    let wire = Wire::new();
    for (id, name) in [(1, "DP-1"), (2, "DP-2")] {
        send_settings(
            &mut app,
            Message::DisplayOutput(
                crate::displays::OutputEvent::Created(Some(info(Some(name), (1920, 1080)))),
                wire.output(id),
            ),
        );
    }
    send_settings(&mut app, Message::MonitorPreview("DP-1".into()));
    send_settings(&mut app, Message::ObjectTab(ObjectTab::Position));
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    send_settings(
        &mut app,
        Message::Placement(placement::Message::Nudge([1, 0], false)),
    );
    let geometry = app.placement.state();
    send_settings(
        &mut app,
        Message::WidgetMonitor(Widget::Clock, Some("DP-2".into())),
    );
    send_settings(&mut app, Message::WidgetMonitorHide(Widget::Clock, true));
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
    assert_eq!(
        app.canvas_config().widget_monitors["clock"]
            .display
            .as_deref(),
        Some("DP-2")
    );
    assert!(app.canvas_config().widget_monitors["clock"].hide_disconnected);
    assert_eq!(
        app.placement.state(),
        geometry,
        "changing location keeps the geometry preview"
    );
    send_settings(&mut app, Message::Canvas(canvas::Message::Undo));
    assert!(!app.canvas_config().widget_monitors["clock"].hide_disconnected);
    let saved = app.canvas_saved().unwrap();
    send_settings(&mut app, Message::Canvas(canvas::Message::Cancel));
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
    app.restore_canvas(saved).unwrap();
    assert_eq!(
        app.canvas_config().widget_monitors["clock"]
            .display
            .as_deref(),
        Some("DP-2")
    );
    let id = app.canvas.as_ref().unwrap().id;
    send_settings(&mut app, Message::Canvas(canvas::Message::Apply(id)));
    assert!(!app.error, "{}", app.status);
    assert_eq!(
        Config::load().widget_monitors["clock"].display.as_deref(),
        Some("DP-2")
    );
    send_settings(&mut app, Message::UndoDesktop);
    let after: serde_json::Value =
        serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap();
    let before: serde_json::Value = serde_json::from_slice(&before).unwrap();
    assert_eq!(
        after, before,
        "one Undo restores both geometry and monitor assignment"
    );
}

#[test]
fn preview_conflict_choices_render_and_resolve_at_compact_size() {
    if isolated(
        "settings::navigation::ux_regressions::preview_conflict_choices_render_and_resolve_at_compact_size",
    ) {
        return;
    }
    use crate::desktop::profiles as records;
    let mut app = app(Page::DesktopProfiles);
    Config::save_changes(serde_json::json!({"appearance":{"opacity":92,"corner_radius":12}}))
        .unwrap();
    app.config = Config::load();
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    let profile = records::import_local(
        serde_json::json!({
            "format":"desktop-cards-profile", "version":records::VERSION, "name":"Conflict preview",
            "widget_style":{"appearance":{"opacity":70,"corner_radius":18}}
        })
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    let id = records::ProfileLibrary::new(&store)
        .create(&profile)
        .unwrap();
    send_settings(
        &mut app,
        Message::Profiles(profiles::Message::Select(profiles::Selection::Saved(id))),
    );
    send_settings(
        &mut app,
        Message::Profiles(profiles::Message::Include(Domain::WidgetStyle, true)),
    );
    send_settings(&mut app, Message::Page(Page::Weather));
    Config::save_changes(serde_json::json!({"appearance":{"opacity":80,"corner_radius":24}}))
        .unwrap();
    send_settings(&mut app, Message::Page(Page::DesktopProfiles));
    assert_eq!(app.profiles.conflict_values().len(), 2);
    assert!(!app.profiles.can_apply());
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    send_settings(&mut app, Message::Resized(Size::new(820., 600.)));
    let mut native = Native::new(Size::new(820., 600.));
    assert!(native.duplicate_ids(app.settings_view()).is_empty());
    click(&mut app, &mut native, "review-preview-conflicts");
    native.capture(
        app.settings_view(),
        &cosmic::Theme::dark(),
        "ux-conflicts-820",
    );
    let apply = native.bounds(app.settings_view(), "profiles-confirm");
    assert!(apply.y >= 0. && apply.y + apply.height <= 600.);
    click(
        &mut app,
        &mut native,
        "conflict-current-widget:/appearance/opacity",
    );
    assert_eq!(app.profiles.conflict_values().len(), 1);
    click(
        &mut app,
        &mut native,
        "conflict-preview-widget:/appearance/corner_radius",
    );
    assert!(app.profiles.conflict_values().is_empty());
    assert!(app.profiles.can_apply());
    assert!(native.duplicate_ids(app.settings_view()).is_empty());
    let _ = native.render(app.settings_view(), &cosmic::Theme::dark());
    assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
}
