use super::*;
use crate::displays::test_support::isolated;
use crate::settings::tests::send_settings;
use crate::widget_style::resolver::ClockFace;
use serde_json::Value;
fn raw() -> Value {
    serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap()
}
fn fixture() -> Settings {
    std::fs::write(Config::path().unwrap(),br#"{"setup_dismissed":true,"clock":{"face":"digital"},"location":{"name":"PRIVATE","latitude":1.0,"longitude":2.0},"future_setting":37}"#).unwrap();
    Settings::init(Core::default(), Page::Moments).0
}
fn send(app: &mut Settings, message: Message) {
    send_settings(app, super::super::Message::Moment(message));
}
#[test]
fn fb15_preview_activation_scoped_edit_and_undo_preserve_base_and_other_phase() {
    if isolated(
        "settings::moments::tests::fb15_preview_activation_scoped_edit_and_undo_preserve_base_and_other_phase",
    ) {
        return;
    }
    let mut app = fixture();
    let before = raw();
    send(&mut app, Message::Preview(Phase::Night));
    assert_eq!(raw(), before);
    send(&mut app, Message::Activate(Some(Phase::Day)));
    assert!(!app.error, "{}", app.status);
    let activated = raw();
    for (k, v) in before.as_object().unwrap() {
        assert_eq!(activated.get(k), Some(v));
    }
    send(
        &mut app,
        Message::Clock(Some(Phase::Day), "face", json!("analog")),
    );
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.config.clock.face, Some(ClockFace::Digital));
    assert_eq!(app.config.moment_clock().face, Some(ClockFace::Analog));
    assert_eq!(
        app.config.moments.day.family,
        Some(crate::desktop_theme::Family::Paper)
    );
    let mut outside = raw();
    outside["moments"]["night"] = json!({"clock":{"weight":"bold"}});
    std::fs::write(
        Config::path().unwrap(),
        serde_json::to_vec(&outside).unwrap(),
    )
    .unwrap();
    send_settings(&mut app, super::super::Message::UndoDesktop);
    assert!(!app.error, "{}", app.status);
    assert_eq!(raw()["moments"]["night"], outside["moments"]["night"]);
    assert_eq!(app.config.moment_clock().face, Some(ClockFace::Digital));
}
#[test]
fn fb15_queued_clock_edit_cannot_retarget_and_explicit_inactive_edit_stays_local() {
    if isolated(
        "settings::moments::tests::fb15_queued_clock_edit_cannot_retarget_and_explicit_inactive_edit_stays_local",
    ) {
        return;
    }
    let mut app = fixture();
    send(&mut app, Message::Activate(Some(Phase::Day)));
    let rendered = app.clock_moment_edit(super::super::Message::ClockFace(Some(ClockFace::Analog)));
    Config::save_moment("/moments/active", json!("night")).unwrap();
    let before = raw();
    send_settings(&mut app, rendered);
    assert!(app.error);
    assert_eq!(raw(), before);
    send(
        &mut app,
        Message::Style(Phase::Day, "face", json!("analog")),
    );
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.config.moments.active, Some(Phase::Night));
    assert_eq!(app.config.moments.day.clock.face, Some(ClockFace::Analog));
    assert_eq!(app.config.moment_clock().face, Some(ClockFace::Digital));
    send(&mut app, Message::Style(Phase::Day, "face", Value::Null));
    assert_eq!(app.config.moments.day.clock.face, None);
    send(&mut app, Message::BaseClock(true));
    send_settings(
        &mut app,
        super::super::Message::Moment(Message::Clock(None, "face", json!("analog"))),
    );
    assert_eq!(app.config.clock.face, Some(ClockFace::Analog));
    assert_eq!(app.config.moments.night.clock.face, None);
}
/// Focuses the focusable widget under `point`, accounting for scrolling,
/// and unfocuses every other one.
struct FocusAt {
    point: cosmic::iced::Point,
    offset: cosmic::iced::Vector,
    pending: cosmic::iced::Vector,
    found: bool,
}
impl FocusAt {
    fn new(point: cosmic::iced::Point) -> Self {
        Self {
            point,
            offset: Default::default(),
            pending: Default::default(),
            found: false,
        }
    }
}
impl cosmic::iced::advanced::widget::Operation for FocusAt {
    fn traverse(
        &mut self,
        f: &mut dyn FnMut(&mut dyn cosmic::iced::advanced::widget::Operation),
    ) {
        let previous = self.offset;
        self.offset += std::mem::take(&mut self.pending);
        f(self);
        self.offset = previous;
    }
    fn scrollable(
        &mut self,
        _: Option<&cosmic::widget::Id>,
        _: cosmic::iced::Rectangle,
        _: cosmic::iced::Rectangle,
        translation: cosmic::iced::Vector,
        _: &mut dyn cosmic::iced::advanced::widget::operation::Scrollable,
    ) {
        self.pending = translation;
    }
    fn focusable(
        &mut self,
        _: Option<&cosmic::widget::Id>,
        bounds: cosmic::iced::Rectangle,
        state: &mut dyn cosmic::iced::advanced::widget::operation::Focusable,
    ) {
        if !self.found && (bounds - self.offset).contains(self.point) {
            state.focus();
            self.found = true;
        } else {
            state.unfocus();
        }
    }
}
#[test]
fn fb15_native_moments_primary_controls_fit_and_keyboard_activation_is_scoped() {
    if isolated(
        "settings::moments::tests::fb15_native_moments_primary_controls_fit_and_keyboard_activation_is_scoped",
    ) {
        return;
    }
    use cosmic::iced::{Event, Point, keyboard};
    let mut app = fixture();
    for (width, height) in [(820., 600.), (1280., 800.)] {
        send(&mut app, Message::Activate(None));
        send(&mut app, Message::Preview(Phase::Day));
        send_settings(
            &mut app,
            super::super::Message::Resized(Size::new(width, height)),
        );
        let mut native = crate::native_test::Native::new(Size::new(width, height));
        // The schedule buttons only show once switching is on; the switch is always there.
        for id in ["moment-activate", "moment-auto"] {
            let b = native.bounds(app.settings_view(), id);
            assert!(
                b.x >= 0. && b.y >= 0. && b.x + b.width <= width && b.y + b.height <= height,
                "{id}: {b:?}"
            );
        }
        let before = raw();
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("fb15-day-{width}"),
        );
        send(&mut app, Message::Preview(Phase::Night));
        native.capture(
            app.settings_view(),
            &cosmic::Theme::dark(),
            &format!("fb15-night-{width}"),
        );
        assert_eq!(raw(), before);
        // The phase-specific Schedule action is primary. The all-schedules link
        // remains reachable below the style controls through the actual scrollable.
        crate::settings::tests::native_reveal(&app, &mut native, "moments-schedules");
        crate::settings::tests::native_reveal(&app, &mut native, "moment-activate");
        // "Right now" is a segmented choice; focus its "Night style" option.
        let night = native.text_bounds(app.settings_view(), "Night style").center();
        native.operate(app.settings_view(), Box::new(FocusAt::new(night)));
        let key = keyboard::Key::Named(keyboard::key::Named::Enter);
        let (messages, _) = native.event(
            app.settings_view(),
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: keyboard::key::Physical::Unidentified(
                    keyboard::key::NativeCode::Unidentified,
                ),
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::empty(),
                text: None,
                repeat: false,
            }),
            Point::new(1., 1.),
        );
        assert_eq!(
            messages.len(),
            1,
            "keyboard must activate the focused action"
        );
        for message in messages {
            send_settings(&mut app, message);
        }
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.config.moments.active, Some(Phase::Night));
        let after = raw();
        for (k, v) in before
            .as_object()
            .unwrap()
            .iter()
            .filter(|(k, _)| *k != "moments")
        {
            assert_eq!(after.get(k), Some(v));
        }
        send_settings(&mut app, super::super::Message::UndoDesktop);
        assert!(!app.error, "{}", app.status);
        assert_eq!(raw(), before);
    }
}

#[test]
fn fb15_commit_time_phase_guard_rejects_switch_between_read_and_write() {
    if isolated(
        "settings::moments::tests::fb15_commit_time_phase_guard_rejects_switch_between_read_and_write",
    ) {
        return;
    }
    let mut app = fixture();
    send(&mut app, Message::Activate(Some(Phase::Day)));
    let path = Config::path().unwrap();
    let mut outside = raw();
    outside["moments"]["active"] = json!("night");
    let result = crate::desktop::actions::preferences_guarded(
        &path,
        "Day clock",
        Some(("/moments/active", Some(json!("day")))),
        |value| {
            // Model a different writer completing after this action reads its input.
            std::fs::write(&path, serde_json::to_vec(&outside).unwrap()).unwrap();
            crate::desktop::store::widget::put(
                value,
                "/moments/day/clock/face",
                Some(json!("analog")),
            )
        },
    );
    assert!(result.is_err());
    assert_eq!(raw(), outside);
    let sample = app.moment_sample();
    assert_eq!(sample.location.unwrap().name, "Sample city");
    assert_eq!(app.config.location.as_ref().unwrap().name, "PRIVATE");
}
