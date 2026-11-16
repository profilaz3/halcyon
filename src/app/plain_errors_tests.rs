//! Every failure on the desktop says what happened and what to do next:
//! the calendar (M31), the edge menu's Looks (M35), anything a click opens
//! (L22) and the System monitor before its first reading (L7).
use super::*;
use crate::calendar::Problem;
use crate::native_test::Native;
use cosmic::Application;
use cosmic::iced::Size;

fn cards() -> Cards {
    let (mut cards, _) = Cards::init(
        Core::default(),
        Config {
            enabled_widgets: vec!["calendar".into()],
            ..Config::default()
        },
    );
    // Only this test's calendar, never this machine's.
    cards.calendar = None;
    cards.calendar_today = None;
    cards.calendar_ahead = None;
    cards.calendar_connection = crate::calendar::Connection {
        backend: crate::calendar::Backend::Google,
        account: "sample@example.com".into(),
        revision: "fixture".into(),
        ..Default::default()
    };
    cards
}

/// Both views of this month finish with `result`.
fn finish(cards: &mut Cards, result: Result<CalendarData, Problem>) {
    let month = Local::now().format("%Y-%m").to_string();
    cards.today_fetching = Some(month.clone());
    cards.calendar_fetching = Some(month.clone());
    let _ = cards.update(Message::Calendar("fixture".into(), month, result));
}

#[test]
fn a_calendar_only_the_person_can_fix_says_why_and_stops_retrying() {
    let mut cards = cards();
    finish(&mut cards, Err(Problem::Locked));
    assert!(cards.calendar_error && cards.calendar_today_error);
    assert_eq!(
        cards.calendar_needs_you(),
        Some(("Your keyring is locked", "Unlock your keyring"))
    );
    // No retry every two minutes: it can't fix itself.
    for next in [cards.next_calendar, cards.next_today_calendar] {
        assert!(next > Instant::now() + Duration::from_secs(3600));
    }
    // The card says why, with the button that opens Settings › Calendar.
    let mut native = Native::new(Size::new(360., 600.));
    assert!(
        !native
            .text_all(cards.calendar_card(), "Your keyring is locked")
            .is_empty()
    );
    assert!(
        !native
            .text_all(cards.calendar_card(), "Unlock your keyring →")
            .is_empty()
    );
    assert!(
        native
            .text_all(cards.calendar_card(), "Check calendar connection →")
            .is_empty()
    );
    // So does the island, with its reason.
    let snapshot = cards.activity_snapshot();
    assert!(snapshot.calendar_problem);
    assert_eq!(
        snapshot.calendar_reason.as_deref(),
        Some("Your keyring is locked")
    );
    // Try again, here or in the island, tries at once.
    let _ = cards.update(Message::RefreshCalendar);
    assert!(cards.next_calendar <= Instant::now());
    cards.next_calendar = Instant::now() + CALENDAR_BLOCKED;
    let _ = cards.activity_message(crate::activity_island::bridge::Event::Command(
        crate::activity_island::bridge::Command::RefreshCalendar,
    ));
    assert!(cards.next_calendar <= Instant::now());
    // Settings › Calendar reached the calendar again (after an unlock):
    // its command asks for the same.
    assert!(matches!(
        Cards::command(&serde_json::json!({"action": "refresh-calendar"})),
        Some(Message::RefreshCalendar)
    ));
    // An expired sign-in asks for a new one.
    finish(&mut cards, Err(Problem::SignIn));
    assert_eq!(
        cards.calendar_needs_you(),
        Some(("Your calendar sign-in expired", "Sign in again"))
    );
    // Events come back: nothing is wrong any more.
    finish(&mut cards, Ok(CalendarData::default()));
    assert_eq!(cards.calendar_needs_you(), None);
    assert!(cards.calendar_problem.is_none());
    assert_eq!(cards.activity_snapshot().calendar_reason, None);
}

#[test]
fn a_calendar_that_cant_be_reached_keeps_trying() {
    let mut cards = cards();
    finish(&mut cards, Err(Problem::Network));
    assert!(cards.calendar_error);
    assert_eq!(cards.calendar_needs_you(), None);
    assert!(cards.next_calendar <= Instant::now() + Duration::from_secs(121));
    assert_eq!(cards.activity_snapshot().calendar_reason, None);
    let mut native = Native::new(Size::new(360., 600.));
    assert!(
        !native
            .text_all(cards.calendar_card(), "Check calendar connection →")
            .is_empty()
    );
}

#[test]
fn a_failed_look_switch_says_why_and_where_to_sort_it_out() {
    use crate::desktop::profiles::Domain;
    let retry = Message::ApplyLook(
        crate::automation::ProfileRef::Preset("night".into()),
        "Night".into(),
    );
    let mut cards = cards();
    let raise = |cards: &mut Cards, failure: edge::LookFailure| {
        let _ = cards.update(Message::LookApplied(Err(failure)));
        cards.notice.take().expect("the failure is shown")
    };
    // Unapplied changes in Settings: say which, open their page, try again.
    let notice = raise(
        &mut cards,
        edge::LookFailure::waiting(&[Domain::Dock].into()).retry_with(retry.clone()),
    );
    assert_eq!(notice.title, "Couldn’t switch the Look");
    assert!(
        notice
            .body
            .contains("changes to Dock that aren’t applied yet")
    );
    assert!(matches!(
        notice.open,
        Some(Message::SettingsPage("desktop-dock"))
    ));
    assert!(notice.retry.is_some());
    // An interrupted change needs a decision on Looks; Try again can't help.
    let notice = raise(
        &mut cards,
        edge::LookFailure::failed(
            "An interrupted change needs a decision on the Looks page first.".into(),
        )
        .retry_with(retry.clone()),
    );
    assert_eq!(
        notice.body,
        "An interrupted change needs a decision on the Looks page first."
    );
    assert!(matches!(
        notice.open,
        Some(Message::SettingsPage("desktop-profiles"))
    ));
    assert!(notice.retry.is_none());
    // A rollback that couldn't finish never claims nothing changed.
    let notice = raise(
        &mut cards,
        edge::LookFailure::failed(
            "Could not apply changes (No space left on device (os error 28)). Not yet restored: com.system76.CosmicPanel.Dock/v1/size. Choose Go back to before to retry.".into(),
        )
        .retry_with(retry.clone()),
    );
    assert!(
        !notice.body.contains("Nothing was changed"),
        "{}",
        notice.body
    );
    assert!(
        notice.body.contains("couldn’t be put back"),
        "{}",
        notice.body
    );
    assert!(
        notice.error.contains("os error 28"),
        "the technical text is kept"
    );
    assert!(notice.retry.is_none());
    // A store error reads plainly; Try again can work.
    let notice = raise(
        &mut cards,
        edge::LookFailure::failed(
            "Could not write /home/a/b: No space left on device (os error 28)".into(),
        )
        .retry_with(retry),
    );
    assert_eq!(
        notice.body,
        "Your disk is full. Free some space, then try again."
    );
    assert!(notice.retry.is_some());
    // The menu shows the reason and Open Settings.
    cards.notice = Some(notice);
    let mut native = Native::new(Size::new(
        drawer::SIDEBAR_WIDTH as f32,
        drawer::SIDEBAR_HEIGHT as f32,
    ));
    native.text_bounds(
        cards.control_rail(),
        "Your disk is full. Free some space, then try again.",
    );
    native.bounds(cards.control_rail(), "notice-open-settings");
    native.bounds(cards.control_rail(), "notice-retry");
}

#[test]
fn what_a_click_could_not_open_is_named_in_the_edge_menu() {
    let named = |command: &str, arg: &str| Opened::of(command, &[arg.to_string()]).name;
    assert_eq!(
        named("gtk-launch", "org.gnome.Nautilus.desktop"),
        "org.gnome.Nautilus"
    );
    assert_eq!(named("xdg-open", "https://open-meteo.com/"), "the link");
    assert_eq!(named("xdg-open", "/home/a/Projects"), "Projects");
    assert_eq!(named(crate::paths::CONTROLLER, "settings"), "Settings");
    assert_eq!(
        named(crate::paths::CONTROLLER, "weather-picker"),
        "the city picker"
    );
    let mut cards = cards();
    let _ = cards.update(Message::ControlFinished(
        Opened::of("xdg-open", &["/home/a/Projects".into()]),
        Err("/usr/bin/xdg-open /home/a/Projects exited with exit status: 4".into()),
    ));
    let notice = cards.notice.as_ref().expect("a failed open is shown");
    assert_eq!(notice.title, "Couldn’t open Projects");
    assert_eq!(
        notice.body,
        "Check that the folder still exists, then try again."
    );
    assert!(notice.error.contains("exit status: 4"), "kept for Details");
    assert!(cards.sidebar.visible, "the edge menu opens to show it");
    // Without gtk-launch (or gio), opening apps says what is missing.
    let _ = cards.update(Message::ControlFinished(
        Opened::of("gtk-launch", &["firefox.desktop".into()]),
        Err(NO_GTK_LAUNCH.into()),
    ));
    let notice = cards.notice.as_ref().unwrap();
    assert_eq!(notice.title, "Couldn’t open firefox");
    assert_eq!(notice.body, NO_GTK_LAUNCH);
    assert!(notice.error.is_empty(), "nothing more to tell");
    // Something that opened says nothing.
    cards.notice = None;
    let _ = cards.update(Message::ControlFinished(
        Opened::of("xdg-open", &["https://open-meteo.com/".into()]),
        Ok(()),
    ));
    assert!(cards.notice.is_none());
}

#[test]
fn the_system_monitor_measures_before_it_says_anything_is_missing() {
    let config = Config::default();
    let histories = history::MetricHistories::default();
    let mut native = Native::new(Size::new(360., 400.));
    let first = Metrics::default();
    let shown = native.all_text(devbox::view(&config, &first, &histories, 400.));
    assert!(shown.iter().any(|text| text == "Measuring…"), "{shown:?}");
    for missing in ["Unavailable", "No GPU detected", "0%"] {
        assert!(
            !shown.iter().any(|text| text == missing),
            "{missing}: {shown:?}"
        );
    }
    // A reading came back without them: now they really are missing.
    let read = Metrics {
        measured: true,
        ..Metrics::default()
    };
    let shown = native.all_text(devbox::view(&config, &read, &histories, 400.));
    assert!(shown.iter().any(|text| text == "Unavailable"), "{shown:?}");
    assert!(!shown.iter().any(|text| text == "Measuring…"), "{shown:?}");
}
