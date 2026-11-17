//! Calendar › What to show and How it looks: what the agenda lists, where
//! its days come from, and how the card draws them at its real sizes.
use super::*;
use crate::native_test::Native;
use chrono::TimeZone;
use cosmic::iced::Size;

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

fn at(day: NaiveDate, hour: u32) -> chrono::DateTime<Local> {
    Local
        .from_local_datetime(&day.and_hms_opt(hour, 0, 0).unwrap())
        .earliest()
        .unwrap()
}

fn timed(title: &str, day: NaiveDate, hour: u32, location: &str) -> CalendarEvent {
    CalendarEvent {
        title: title.into(),
        start: at(day, hour).to_rfc3339(),
        end: at(day, hour + 1).to_rfc3339(),
        location: location.into(),
        calendar_color: "#3a9b5c".into(),
        ..Default::default()
    }
}

fn all_day(title: &str, day: NaiveDate) -> CalendarEvent {
    CalendarEvent {
        title: title.into(),
        start: day.to_string(),
        end: (day + Days::days(1)).to_string(),
        all_day: true,
        ..Default::default()
    }
}

fn titles(agenda: &data::CalendarAgenda<'_>) -> Vec<String> {
    agenda.events.iter().map(|e| e.title.clone()).collect()
}

fn cards() -> Cards {
    let (mut cards, _) = Cards::init(
        cosmic::app::Core::default(),
        Config {
            enabled_widgets: vec!["calendar".into()],
            ..Config::default()
        },
    );
    // Only the fixture's events, never this machine's saved ones.
    cards.calendar = None;
    cards.calendar_today = None;
    cards.calendar_ahead = None;
    cards
}

#[test]
fn all_day_switch_leaves_out_all_day_events_in_widget_and_glance() {
    let now = Local::now();
    let today = now.date_naive();
    let mut cards = cards();
    let data = CalendarData {
        month: today.format("%Y-%m").to_string(),
        events: vec![all_day("Holiday", today), timed("Late call", today, 22, "")],
        ..Default::default()
    };
    cards.calendar = Some(data.clone());
    cards.calendar_today = Some(data.clone());
    let filter = |all_day| data::AgendaFilter {
        show_earlier: true,
        all_day,
        hide_finished: false,
    };
    assert_eq!(
        titles(&data.agenda(today, now, filter(true))),
        ["Holiday", "Late call"]
    );
    assert_eq!(
        titles(&data.agenda(today, now, filter(false))),
        ["Late call"]
    );
    cards.show_earlier = true;
    assert!(cards.config.calendar_all_day, "on by default");
    assert_eq!(cards.agenda_events(today).len(), 2);
    assert_eq!(cards.glance_events(today).len(), 2);
    cards.config.calendar_all_day = false;
    assert_eq!(cards.agenda_events(today)[0].title, "Late call");
    assert_eq!(cards.agenda_events(today).len(), 1);
    assert_eq!(cards.glance_events(today).len(), 1);
}

#[test]
fn hide_finished_drops_todays_ended_events_at_their_end_and_keeps_other_days() {
    let today = date(2026, 10, 19);
    let yesterday = today - Days::days(1);
    let data = CalendarData {
        events: vec![
            timed("Standup", today, 9, ""),
            timed("Review", today, 11, ""),
            all_day("Release day", today),
            timed("Retro", yesterday, 9, ""),
        ],
        ..Default::default()
    };
    let keep = data::AgendaFilter {
        show_earlier: false,
        all_day: true,
        hide_finished: false,
    };
    let hide = data::AgendaFilter {
        hide_finished: true,
        ..keep
    };
    // Before Review ends it is listed either way; the minute tick that
    // already drives meeting highlights redraws the card when it ends.
    let before_end = at(today, 11) + Days::minutes(59);
    let after_end = at(today, 12);
    assert_eq!(
        titles(&data.agenda(today, before_end, hide)),
        ["Review", "Release day"]
    );
    let folded = data.agenda(today, after_end, keep);
    assert_eq!(titles(&folded), ["Release day"]);
    assert_eq!((folded.earlier, folded.finished), (2, 2));
    let hidden = data.agenda(today, after_end, hide);
    assert_eq!(titles(&hidden), ["Release day"]);
    assert_eq!(
        (hidden.earlier, hidden.finished),
        (0, 2),
        "no Earlier to open, but the day still reads as done"
    );
    let opened = data.agenda(
        today,
        after_end,
        data::AgendaFilter {
            show_earlier: true,
            ..hide
        },
    );
    assert_eq!(
        titles(&opened),
        ["Release day"],
        "Earlier can't bring them back"
    );
    // Past days stay browsable.
    assert_eq!(titles(&data.agenda(yesterday, after_end, hide)), ["Retro"]);
}

#[test]
fn next_seven_days_cross_a_month_end_with_the_following_month_fetched() {
    let mut cards = cards();
    let today = date(2026, 12, 28);
    cards.month = date(2026, 12, 1);
    cards.calendar_home = true;
    cards.config.calendar_show_today = false;
    cards.config.calendar_show_tomorrow = false;
    cards.config.calendar_show_week = true;
    let week = cards.calendar_days(today);
    assert_eq!(week.len(), 7);
    assert_eq!(week[0], today, "today first, even with Today off");
    assert_eq!(week[6], date(2027, 1, 3));
    assert_eq!(day_heading(week[0], today), "Today");
    assert_eq!(day_heading(week[1], today), "Tomorrow");
    assert_eq!(day_heading(date(2026, 12, 30), today), "Wednesday 30");
    assert_eq!(day_heading(date(2027, 1, 1), today), "Friday 1");
    // Inside a month nothing more is fetched.
    assert_eq!(cards.calendar_ahead_month_at(date(2026, 12, 10)), None);
    assert_eq!(
        cards.calendar_ahead_month_at(today).as_deref(),
        Some("2027-01")
    );
    cards.calendar_connection = crate::calendar::Connection {
        backend: crate::calendar::Backend::Google,
        account: "fixture@example.invalid".into(),
        revision: "fixture-revision".into(),
        ..Default::default()
    };
    cards.next_ahead_calendar = Instant::now();
    assert!(cards.ahead_calendar_task(today, Instant::now()).is_some());
    assert_eq!(cards.ahead_fetching.as_deref(), Some("2027-01"));
    assert!(
        cards.ahead_calendar_task(today, Instant::now()).is_none(),
        "one request per month"
    );
    assert!(cards.calendar_month_pending("2027-01"));
    // An event over the month end is in both months' results; each day
    // reads one month, so it is listed once a day.
    let new_year_eve = CalendarEvent {
        title: "New Year party".into(),
        start: at(date(2026, 12, 31), 20).to_rfc3339(),
        end: at(date(2027, 1, 1), 2).to_rfc3339(),
        ..Default::default()
    };
    cards.calendar = Some(CalendarData {
        month: "2026-12".into(),
        events: vec![
            timed("Year review", date(2026, 12, 29), 10, ""),
            new_year_eve.clone(),
        ],
        ..Default::default()
    });
    let _ = cards.update(Message::Calendar(
        "fixture-revision".into(),
        "2027-01".into(),
        Ok(CalendarData {
            month: "2027-01".into(),
            events: vec![new_year_eve, timed("Planning", date(2027, 1, 2), 10, "")],
            ..Default::default()
        }),
    ));
    assert!(cards.ahead_fetching.is_none());
    assert_eq!(cards.calendar_ahead.as_ref().unwrap().month, "2027-01");
    assert_eq!(cards.calendar.as_ref().unwrap().month, "2026-12");
    let listed = |cards: &Cards, day| -> Vec<String> {
        cards
            .agenda_events(day)
            .into_iter()
            .map(|e| e.title.clone())
            .collect()
    };
    assert_eq!(listed(&cards, date(2026, 12, 29)), ["Year review"]);
    assert_eq!(listed(&cards, date(2026, 12, 31)), ["New Year party"]);
    assert_eq!(listed(&cards, date(2027, 1, 1)), ["New Year party"]);
    assert_eq!(listed(&cards, date(2027, 1, 2)), ["Planning"]);
    // Tomorrow on the last day of a month reads the next month as well.
    cards.config.calendar_show_week = false;
    cards.config.calendar_show_tomorrow = true;
    assert_eq!(
        cards.calendar_ahead_month_at(date(2026, 12, 31)).as_deref(),
        Some("2027-01")
    );
    cards.config.calendar_show_tomorrow = false;
    cards.config.calendar_show_today = true;
    assert_eq!(cards.calendar_ahead_month_at(date(2026, 12, 31)), None);
}

#[test]
fn week_agenda_leaves_out_empty_later_days_but_keeps_today() {
    let mut cards = cards();
    let today = Local::now().date_naive();
    cards.config.calendar_show_week = true;
    cards.calendar = Some(CalendarData {
        month: today.format("%Y-%m").to_string(),
        events: vec![timed("Dentist", today + Days::days(3), 10, "")],
        ..Default::default()
    });
    assert_eq!(cards.calendar_days(today).len(), 7);
    assert_eq!(
        cards.calendar_sections(today),
        [today, today + Days::days(3)]
    );
    cards.calendar = None;
    assert_eq!(
        cards.calendar_sections(today).len(),
        7,
        "before events arrive every day waits for them"
    );
}

#[test]
fn calendar_colors_off_draws_markers_in_the_accent() {
    let mut cards = cards();
    let accent = Color::from_rgb8(10, 20, 30);
    let colored = timed("Gym", date(2026, 10, 19), 7, "");
    let plain = CalendarEvent {
        calendar_color: String::new(),
        ..colored.clone()
    };
    assert!(cards.config.calendar_colors, "on by default");
    assert_eq!(
        cards.event_marker_color(&colored, accent),
        Color::from_rgb8(0x3a, 0x9b, 0x5c)
    );
    assert_eq!(cards.event_marker_color(&plain, accent), accent);
    cards.config.calendar_colors = false;
    assert_eq!(cards.event_marker_color(&colored, accent), accent);
}

#[test]
fn location_shows_under_the_title_only_when_details_are_shown_and_there_is_room() {
    let mut cards = cards();
    let day = date(2026, 10, 19);
    let event = timed("Design review", day, 10, "Studio 2, second floor");
    assert_eq!(event.place(), Some("Studio 2, second floor"));
    let call = timed("Call", day, 10, "https://meet.example/abc");
    assert_eq!(call.place(), None, "a call link is Join, not a place");
    let mixed = timed("Mixed", day, 10, "https://meet.example/abc\n  Room 4  \n");
    assert_eq!(mixed.place(), Some("Room 4"));
    assert!(!cards.config.calendar_show_location, "off by default");
    assert_eq!(cards.calendar_place(&event), None);
    let without = cards.calendar_event_height(&event, 300.);
    cards.config.calendar_show_location = true;
    assert_eq!(cards.calendar_place(&event), Some("Studio 2, second floor"));
    // One more line in the title column; rows keep their 30px minimum.
    let with = cards.calendar_event_height(&event, 300.);
    assert!(
        with > without && with <= without + PLACE_HEIGHT + 3.,
        "{without} {with}"
    );
    // Hide event details wins: no place, as no title.
    cards.config.calendar_private = true;
    assert_eq!(cards.calendar_place(&event), None);
    assert_eq!(cards.calendar_event_height(&event, 300.), without);
    cards.config.calendar_private = false;
    // A card too short for its controls and an event (its compact layout,
    // on a short screen) keeps rows plain.
    cards.config.widget_sizes.insert(
        "calendar".into(),
        crate::layout::minimum_size(Widget::Calendar),
    );
    assert!(
        cards.calendar_place(&event).is_some(),
        "the smallest size has room"
    );
    let screen = cards.screen_size;
    cards.screen_size = [1920., 300.];
    assert!(cards.explicit_size(Widget::Calendar).unwrap()[1] < CALENDAR_CHROME_HEIGHT + 44.);
    assert_eq!(cards.calendar_place(&event), None);
    cards.screen_size = screen;
    cards.config.widget_sizes.clear();
    // Drawn under the title, and not drawn when details are hidden.
    let today = Local::now().date_naive();
    let tomorrow = today + Days::days(1);
    cards.calendar = Some(CalendarData {
        events: vec![timed(
            "Design review",
            tomorrow,
            10,
            "Studio 2, second floor",
        )],
        ..Default::default()
    });
    cards.config.calendar_show_tomorrow = true;
    let mut native = Native::new(Size::new(360., 600.));
    // The title is rich text; the place sits in its column, right of the
    // time, below the day's heading.
    let starts =
        cards.calendar.as_ref().unwrap().events[0].time_label(cards.config.clock.format_24h);
    let time = native.text_bounds(cards.calendar_card(), &starts);
    let heading = native.text_bounds(cards.calendar_card(), "Tomorrow");
    let place = native.text_bounds(cards.calendar_card(), "Studio 2, second floor");
    assert!(place.x >= time.x + 43., "{time:?} {place:?}");
    assert!(
        place.y > heading.y + heading.height,
        "{heading:?} {place:?}"
    );
    assert!(place.x + place.width <= 360., "{place:?}");
    cards.config.calendar_private = true;
    assert!(
        native
            .text_all(cards.calendar_card(), "Studio 2, second floor")
            .is_empty()
    );
}

/// The card drawn over the bundled Ridge wallpaper, cropped where the
/// default column puts the calendar, so the evidence shows real contrast.
fn capture_over_ridge(pixels: &[u8], size: [f32; 2], name: &str) {
    let Some(dir) = std::env::var_os("DESKTOP_CARDS_LAYOUT_EVIDENCE") else {
        return;
    };
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wallpapers");
    let ridge = crate::desktop::wallpaper::bundled_path(&assets, "halcyon-ridge");
    let ridge = image::open(&ridge).unwrap().to_rgba8();
    let screen = image::imageops::resize(&ridge, 1920, 1080, image::imageops::FilterType::Triangle);
    let [width, height] = size.map(|v| v.round() as u32);
    let mut out = image::imageops::crop_imm(&screen, 48, 120, width, height).to_image();
    for (pixel, card) in out.pixels_mut().zip(pixels.chunks_exact(4)) {
        let alpha = card[3] as f32 / 255.;
        for channel in 0..3 {
            pixel[channel] =
                (card[channel] as f32 * alpha + pixel[channel] as f32 * (1. - alpha)).round() as u8;
        }
    }
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    out.save(dir.join(format!("{name}.png"))).unwrap();
}

#[test]
fn week_range_and_location_fit_the_calendar_card_at_small_and_default_sizes() {
    let mut cards = cards();
    cards.screen_size = [1920., 1080.];
    let today = Local::now().date_naive();
    cards.calendar = Some(CalendarData {
        month: today.format("%Y-%m").to_string(),
        updated_at: Local::now().to_rfc3339(),
        events: vec![
            all_day("Release day", today),
            timed(
                "Design review",
                today + Days::days(1),
                10,
                "Studio 2, second floor",
            ),
            timed(
                "Quarterly planning with the whole product group and partners",
                today + Days::days(2),
                14,
                "Main conference room, building B, north wing by the garden",
            ),
            timed("Dentist", today + Days::days(4), 9, "Clinic"),
            all_day("Weekend trip", today + Days::days(6)),
        ],
        ..Default::default()
    });
    cards.calendar_connection = crate::calendar::Connection {
        backend: crate::calendar::Backend::Google,
        account: "sample@example.com".into(),
        ..Default::default()
    };
    cards.config.calendar_show_week = true;
    cards.config.calendar_show_location = true;
    let sections = cards.calendar_sections(today);
    assert_eq!(sections.len(), 5, "empty later days are left out");
    let headings: Vec<String> = sections.iter().map(|d| day_heading(*d, today)).collect();
    let theme = cosmic::Theme::dark();
    for (name, explicit) in [
        ("small", Some(crate::layout::minimum_size(Widget::Calendar))),
        ("default", None),
    ] {
        match explicit {
            Some(size) => cards.config.widget_sizes.insert("calendar".into(), size),
            None => cards.config.widget_sizes.remove("calendar"),
        };
        let size = cards.widget_size(Widget::Calendar);
        let mut native = Native::new(Size::new(size[0], size[1]));
        let overflow = native.overflowing_text(cards.calendar_card());
        assert!(overflow.is_empty(), "{name}: {overflow:?}");
        let placed = native.placed_text(cards.calendar_card());
        let find = |text: &str| -> cosmic::iced::Rectangle {
            placed
                .iter()
                .find(|(t, _)| t == text)
                .unwrap_or_else(|| panic!("{name}: {text} is drawn"))
                .1
        };
        assert!(
            placed.iter().any(|(t, _)| t == "Overview"),
            "{name}: back to the week"
        );
        let title = find("Next 7 days");
        assert!(title.x + title.width <= size[0], "{name}: {title:?}");
        assert!(native.scrollables(cards.calendar_card()).is_empty());
        if name == "small" {
            assert!(placed.iter().any(|(value, _)| value.contains("Open agenda")));
            let pixels = native.render(cards.calendar_card(), &theme);
            capture_over_ridge(&pixels, size, "calendar-week-location-small");
            continue;
        }
        for heading in &headings {
            let bounds = find(heading);
            assert!(
                bounds.x + bounds.width <= size[0],
                "{name}: {heading} {bounds:?}"
            );
        }
        for place in ["Studio 2, second floor", "Clinic"] {
            let bounds = find(place);
            assert!(
                bounds.x + bounds.width <= size[0],
                "{name}: {place} {bounds:?}"
            );
        }
        // The long place keeps to one line, ellipsized inside the card.
        let long = find("Main conference room, building B, north wing by the garden");
        assert!(long.height <= PLACE_HEIGHT + 1., "{name}: {long:?}");
        assert!(long.x + long.width <= size[0], "{name}: {long:?}");
        let pixels = native.render(cards.calendar_card(), &theme);
        capture_over_ridge(&pixels, size, &format!("calendar-week-location-{name}"));
    }
}
