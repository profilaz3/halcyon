use super::*;
use crate::displays::test_support::isolated;
use crate::settings::Page;
use crate::settings::tests::send_settings;
use crate::widget_style::resolver::{ClockFace, ClockSurface};
use cosmic::{Application, app::Core};

fn fixture() -> Settings {
    std::fs::write(Config::path().unwrap(), r#"{"setup_dismissed":true,"enabled_widgets":["clock","weather","notes"],"clock":{"face":"digital","format_24h":false},"notes":{"future_content_marker":"private"},"future_preference":37}"#).unwrap();
    let store = crate::desktop::DesktopStore::from_env().unwrap();
    crate::test_env::native_worker(store.fixture_root().unwrap(), true);
    let (mut app, initial) = Settings::init(Core::default(), Page::Overview);
    crate::settings::tests::settle_settings(&mut app, initial);
    assert!(app.native_worker_needed());
    app
}
fn send(app: &mut Settings, edit: Message) {
    send_settings(app, AppMessage::Canvas(edit));
}
fn place(app: &mut Settings, edit: placement::Message) {
    send_settings(app, AppMessage::Placement(edit));
}
fn raw() -> Value {
    serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap()
}
fn apply(app: &mut Settings) {
    let id = app.canvas.as_ref().unwrap().id;
    send(app, Message::Apply(id));
}
fn stage(app: &mut Settings) {
    send(app, Message::Start);
    place(app, placement::Message::Select("clock".into()));
    place(app, placement::Message::Nudge([1, 0], true));
    place(app, placement::Message::Resize([0, 1], true));
    send(app, Message::Style("/clock/face", json!("analog")));
}

#[test]
fn fb11_combined_apply_has_one_undo_and_preserves_content() {
    if isolated("settings::canvas::tests::fb11_combined_apply_has_one_undo_and_preserves_content") {
        return;
    }
    let mut app = fixture();
    let before = raw();
    stage(&mut app);
    assert_eq!(raw(), before, "completed gestures and styles remain local");
    assert_eq!(app.config.clock.face, Some(ClockFace::Digital));
    assert_eq!(app.canvas_config().clock.face, Some(ClockFace::Analog));
    apply(&mut app);
    assert!(!app.error, "{}", app.status);
    assert!(app.canvas.is_none());
    assert_eq!(Config::load().clock.face, Some(ClockFace::Analog));
    assert_ne!(raw().get("widget_sizes"), before.get("widget_sizes"));
    assert_eq!(raw()["notes"], before["notes"]);
    assert_eq!(raw()["future_preference"], 37);
    send_settings(&mut app, AppMessage::UndoDesktop);
    assert!(!app.error, "{}", app.status);
    assert_eq!(raw(), before, "one Undo owns the entire canvas");
}

#[test]
fn fb11_local_undo_cancel_and_navigation_never_apply() {
    if isolated("settings::canvas::tests::fb11_local_undo_cancel_and_navigation_never_apply") {
        return;
    }
    let mut app = fixture();
    let before = raw();
    stage(&mut app);
    let geometry = app.placement.state();
    send_settings(&mut app, AppMessage::UndoDesktop);
    assert_eq!(app.canvas_config().clock.face, Some(ClockFace::Digital));
    assert_eq!(app.placement.state(), geometry);
    send(&mut app, Message::Undo);
    assert_ne!(app.placement.state(), geometry);
    send_settings(&mut app, AppMessage::Page(Page::Notes));
    send_settings(&mut app, AppMessage::FlushEdits);
    send_settings(&mut app, AppMessage::Page(Page::Overview));
    assert!(app.canvas_dirty());
    assert_eq!(raw(), before);
    send(&mut app, Message::Cancel);
    assert!(app.canvas.is_none());
    assert_eq!(raw(), before);
}

#[test]
fn fb11_outside_style_conflict_keeps_geometry_and_draft() {
    if isolated("settings::canvas::tests::fb11_outside_style_conflict_keeps_geometry_and_draft") {
        return;
    }
    let mut app = fixture();
    stage(&mut app);
    let mut outside = raw();
    outside["clock"]["face"] = Value::Null;
    std::fs::write(
        Config::path().unwrap(),
        serde_json::to_vec(&outside).unwrap(),
    )
    .unwrap();
    apply(&mut app);
    assert!(app.error);
    assert!(app.canvas_dirty());
    assert_eq!(raw(), outside);
    assert!(app.status.contains("changed elsewhere"), "{}", app.status);
}

#[test]
fn fb11_outside_content_is_merged_and_old_apply_cannot_target_new_draft() {
    if isolated(
        "settings::canvas::tests::fb11_outside_content_is_merged_and_old_apply_cannot_target_new_draft",
    ) {
        return;
    }
    let mut app = fixture();
    stage(&mut app);
    let old_id = app.canvas.as_ref().unwrap().id;
    let mut outside = raw();
    outside["notes"]["future_content_marker"] = json!("newer content");
    std::fs::write(
        Config::path().unwrap(),
        serde_json::to_vec(&outside).unwrap(),
    )
    .unwrap();
    apply(&mut app);
    assert!(!app.error, "{}", app.status);
    assert_eq!(raw()["notes"], outside["notes"]);
    send(&mut app, Message::Start);
    send(&mut app, Message::Style("/clock/surface", json!("none")));
    let before = raw();
    send(&mut app, Message::Apply(old_id));
    assert!(app.canvas_dirty());
    assert_eq!(raw(), before);
}

#[test]
fn fb11_recovery_is_one_record_and_reset_keeps_inheritance() {
    if isolated("settings::canvas::tests::fb11_recovery_is_one_record_and_reset_keeps_inheritance")
    {
        return;
    }
    let mut app = fixture();
    let before = raw();
    stage(&mut app);
    send(&mut app, Message::Style("/clock/surface", json!("none")));
    assert_eq!(app.canvas_config().clock.surface, Some(ClockSurface::None));
    send(&mut app, Message::Reset("/clock/surface"));
    assert_eq!(app.canvas_config().clock.surface, None);
    place(&mut app, placement::Message::Reset);
    app.checkpoint_session_drafts();
    drop(app);
    let (mut recovered, _) = Settings::init(Core::default(), Page::Overview);
    assert!(recovered.canvas_dirty(), "{}", recovered.status);
    assert_eq!(raw(), before);
    assert_eq!(
        recovered.canvas_config().clock.face,
        Some(ClockFace::Analog)
    );
    assert_eq!(recovered.canvas_config().clock.surface, None);
    apply(&mut recovered);
    assert!(!recovered.error, "{}", recovered.status);
    let (reopened, _) = Settings::init(Core::default(), Page::Overview);
    assert!(reopened.canvas.is_none());
}

#[test]
fn fb11_display_epoch_blocks_apply_without_losing_previous_work() {
    if isolated(
        "settings::canvas::tests::fb11_display_epoch_blocks_apply_without_losing_previous_work",
    ) {
        return;
    }
    let mut app = fixture();
    stage(&mut app);
    let before = raw();
    let geometry = app.placement.state();
    app.canvas.as_mut().unwrap().frame.screen[0] += 100.;
    apply(&mut app);
    assert!(app.error);
    assert_eq!(raw(), before);
    assert_eq!(app.placement.state(), geometry);
    assert!(app.canvas_dirty());
}

#[test]
fn fb11_recovery_rejects_unrelated_fields_without_replacing_draft() {
    if isolated(
        "settings::canvas::tests::fb11_recovery_rejects_unrelated_fields_without_replacing_draft",
    ) {
        return;
    }
    let mut app = fixture();
    stage(&mut app);
    let before = app.placement.state();
    let mut saved = app.canvas_saved().unwrap();
    saved.styles = r#"{"/notes/text":"overwrite"}"#.into();
    assert!(app.restore_canvas(saved).is_err());
    assert_eq!(app.placement.state(), before);
    assert!(app.canvas_dirty());
}

#[test]
fn fb11_connection_notifications_with_same_geometry_keep_apply_available() {
    if isolated(
        "settings::canvas::tests::fb11_connection_notifications_with_same_geometry_keep_apply_available",
    ) {
        return;
    }
    let mut app = fixture();
    stage(&mut app);
    app.displays.generation += 3;
    apply(&mut app);
    assert!(!app.error, "{}", app.status);
    assert!(app.canvas.is_none());
}

#[test]
fn fb11_tick_keeps_styled_geometry_and_closing_keeps_draft() {
    if isolated("settings::canvas::tests::fb11_tick_keeps_styled_geometry_and_closing_keeps_draft")
    {
        return;
    }
    let mut app = fixture();
    let before = raw();
    stage(&mut app);
    let geometry = app.placement.state();
    send_settings(&mut app, AppMessage::Tick);
    assert_eq!(app.canvas_config().clock.face, Some(ClockFace::Analog));
    assert_eq!(app.placement.state(), geometry);
    let _ = app.update(AppMessage::Window(super::super::WindowControl::Close));
    assert_eq!(raw(), before);
    let (reopened, _) = Settings::init(Core::default(), Page::Overview);
    assert!(reopened.canvas_dirty());
}

#[test]
fn fb11_shared_fonts_and_readability_keep_saved_surface_intent() {
    if isolated(
        "settings::canvas::tests::fb11_shared_fonts_and_readability_keep_saved_surface_intent",
    ) {
        return;
    }
    let mut app = fixture();
    Config::save_changes(json!({"appearance":{"contrast_assist":true}})).unwrap();
    let before = raw();
    send(&mut app, Message::Start);
    send(&mut app, Message::Style("/clock/surface", json!("none")));
    send_settings(
        &mut app,
        AppMessage::WidgetFont(
            super::super::FontTarget::Clock,
            Some("No Such Installed Font 9845".into()),
        ),
    );
    let projected = app.canvas_config();
    let style = {
        let _scope = crate::widget_style::scope(projected);
        crate::widget_style::presentation(projected, projected.wallpaper_palette(), true)
    };
    assert_eq!(projected.clock.surface, Some(ClockSurface::None));
    assert_eq!(
        style.surface.source,
        crate::widget_style::resolver::Source::Accessibility
    );
    assert!(style.missing_font.is_some());
    assert_eq!(raw(), before);
    apply(&mut app);
    assert!(!app.error, "{}", app.status);
    assert_eq!(Config::load().clock.surface, Some(ClockSurface::None));
    send_settings(&mut app, AppMessage::UndoDesktop);
    assert_eq!(raw(), before);
}

#[test]
fn fb11_canvas_input_cannot_queue_into_immediate_edits_after_apply() {
    if isolated(
        "settings::canvas::tests::fb11_canvas_input_cannot_queue_into_immediate_edits_after_apply",
    ) {
        return;
    }
    let mut app = fixture();
    stage(&mut app);
    let geometry = app.placement.state();
    let before = raw();
    let id = app.canvas.as_ref().unwrap().id;
    app.native_busy = true;
    for _ in 0..1000 {
        let _ = app.update(AppMessage::Placement(placement::Message::Pointer(
            cosmic::iced::Point::new(100., 100.),
            1.,
        )));
    }
    for event in [
        AppMessage::Placement(placement::Message::Nudge([1, 0], false)),
        AppMessage::Canvas(Message::Style("/clock/face", json!("digital"))),
        AppMessage::Canvas(Message::Cancel),
        AppMessage::Canvas(Message::Apply(id)),
        AppMessage::WidgetFont(super::super::FontTarget::Text, Some("Noto Sans".into())),
        AppMessage::UndoDesktop,
    ] {
        let _ = app.update(event);
    }
    assert!(app.native_deferred.is_empty());
    assert_eq!(app.placement.state(), geometry);
    assert_eq!(app.canvas_config().clock.face, Some(ClockFace::Analog));
    assert_eq!(raw(), before);
    app.native_busy = false;
    apply(&mut app);
    assert!(!app.error, "{}", app.status);
    send_settings(&mut app, AppMessage::UndoDesktop);
    assert_eq!(raw(), before);
}

#[test]
fn fb15_canvas_recovery_keeps_original_moment_after_schedule_switch() {
    if isolated(
        "settings::canvas::tests::fb15_canvas_recovery_keeps_original_moment_after_schedule_switch",
    ) {
        return;
    }
    let mut app = fixture();
    let mut initial = raw();
    initial["moments"] = json!({"active":"day"});
    std::fs::write(
        Config::path().unwrap(),
        serde_json::to_vec(&initial).unwrap(),
    )
    .unwrap();
    send(&mut app, Message::Start);
    send(&mut app, Message::Style("/clock/face", json!("analog")));
    assert_eq!(
        app.canvas_config().moment_clock().face,
        Some(ClockFace::Analog)
    );
    assert_eq!(app.canvas_config().clock.face, Some(ClockFace::Digital));
    let saved = app.canvas_saved().unwrap();
    let mut outside = raw();
    outside["moments"]["active"] = json!("night");
    outside["moments"]["night"] = json!({"clock":{"weight":"bold"}});
    std::fs::write(
        Config::path().unwrap(),
        serde_json::to_vec(&outside).unwrap(),
    )
    .unwrap();
    send(&mut app, Message::Cancel);
    app.config = Config::load();
    app.restore_canvas(saved).unwrap();
    assert_eq!(
        app.canvas_config().moments.active,
        Some(crate::moments::Phase::Day)
    );
    assert_eq!(
        app.canvas_config().moment_clock().face,
        Some(ClockFace::Analog)
    );
    apply(&mut app);
    assert!(!app.error, "{}", app.status);
    assert_eq!(raw()["moments"]["active"], "night");
    assert_eq!(raw()["moments"]["day"]["clock"]["face"], "analog");
    assert_eq!(raw()["moments"]["night"], outside["moments"]["night"]);
    assert_eq!(raw()["clock"], initial["clock"]);
    send_settings(&mut app, AppMessage::UndoDesktop);
    assert!(!app.error, "{}", app.status);
    assert_eq!(raw(), outside);
}

#[test]
fn fb15_canvas_recovery_rejects_other_moment_or_unscoped_clock_pointer() {
    if isolated(
        "settings::canvas::tests::fb15_canvas_recovery_rejects_other_moment_or_unscoped_clock_pointer",
    ) {
        return;
    }
    let mut app = fixture();
    let mut initial = raw();
    initial["moments"] = json!({"active":"day"});
    std::fs::write(
        Config::path().unwrap(),
        serde_json::to_vec(&initial).unwrap(),
    )
    .unwrap();
    send(&mut app, Message::Start);
    send(&mut app, Message::Style("/clock/face", json!("analog")));
    for pointer in [
        "/clock/face",
        "/moments/night/clock/face",
        "/moments/active",
    ] {
        let mut saved = app.canvas_saved().unwrap();
        saved.styles = serde_json::to_string(&json!({pointer:"analog"})).unwrap();
        assert!(app.restore_canvas(saved).is_err());
        assert_eq!(
            app.canvas_config().moment_clock().face,
            Some(ClockFace::Analog)
        );
    }
    assert_eq!(raw(), initial);
}
