//! Behavioral regressions and native layout evidence for the widget utility pass.
use super::*;
use crate::native_test::Native;
use chrono::TimeZone;
use cosmic::Application;
use cosmic::iced::{Event, Point, Size, mouse};

fn click(native: &mut Native, cards: &Cards, widget: Widget, point: Point) -> Vec<Message> {
    let mut messages = Vec::new();
    for input in [
        mouse::Event::CursorMoved { position: point },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        let view = match widget {
            Widget::Calendar => cards.calendar_card(),
            Widget::Projects => cards.projects_card(),
            _ => unreachable!(),
        };
        messages.extend(native.event(view, Event::Mouse(input), point).0);
    }
    messages
}

fn meeting(
    title: &str,
    start: chrono::DateTime<Local>,
    end: chrono::DateTime<Local>,
) -> CalendarEvent {
    CalendarEvent {
        title: title.into(),
        start: start.to_rfc3339(),
        end: end.to_rfc3339(),
        all_day: false,
        url: "https://calendar.google.com/event?sample".into(),
        meeting_url: None,
        ..Default::default()
    }
}

#[test]
fn todays_agenda_stays_finished_and_history_remains_accessible() {
    let now = Local.with_ymd_and_hms(2026, 10, 19, 12, 0, 0).unwrap();
    let today = now.date_naive();
    let mut calendar = CalendarData {
        events: vec![
            meeting("Earlier", now - Days::hours(2), now - Days::hours(1)),
            meeting("Just ended", now - Days::minutes(30), now),
            meeting("Next", now + Days::hours(1), now + Days::hours(2)),
            CalendarEvent {
                title: "All day".into(),
                start: today.to_string(),
                end: (today + Days::days(1)).to_string(),
                all_day: true,
                url: String::new(),
                meeting_url: None,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let agenda = calendar.agenda_at(today, now, false);
    assert_eq!(agenda.earlier, 2);
    assert_eq!(
        agenda
            .events
            .iter()
            .map(|event| event.title.as_str())
            .collect::<Vec<_>>(),
        ["Next", "All day"]
    );
    assert_eq!(calendar.agenda_at(today, now, true).events.len(), 4);
    assert_eq!(
        calendar
            .agenda_at(today, now + Days::days(1), false)
            .events
            .len(),
        4
    );
    calendar.events.truncate(2);
    assert!(calendar.agenda_at(today, now, false).events.is_empty());
    assert_eq!(calendar.agenda_at(today, now, true).events.len(), 2);
    // A meeting crossing midnight is retained while live, then hidden on its ending day.
    let midnight = Local
        .from_local_datetime(&today.and_hms_opt(0, 0, 0).unwrap())
        .unwrap();
    calendar.events = vec![meeting(
        "Overnight",
        midnight - Days::hours(1),
        midnight + Days::hours(1),
    )];
    assert_eq!(calendar.agenda_at(today, midnight, false).events.len(), 1);
    assert!(calendar.agenda_at(today, now, false).events.is_empty());
}

#[test]
fn cached_data_labels_include_the_day_and_missing_forecasts_are_not_zero() {
    let now = Local.with_ymd_and_hms(2026, 10, 19, 12, 0, 0).unwrap();
    assert_eq!(timestamp_text(Some(&now.to_rfc3339()), false, now), "12:00");
    assert_eq!(
        timestamp_text(Some(&(now - Days::days(1)).to_rfc3339()), true, now),
        "Saved · Yesterday · 12:00"
    );
    assert_eq!(
        timestamp_text(Some(&(now - Days::days(3)).to_rfc3339()), false, now),
        "16 Oct · 12:00"
    );
    assert_eq!(timestamp_text(Some("invalid"), false, now), "—");
    assert_eq!(forecast_temperature(None), "—");
    assert_eq!(forecast_temperature(Some(&f64::NAN)), "—");
    assert_eq!(forecast_temperature(Some(&0.)), "0°");
}

#[test]
fn earlier_control_is_reversible_and_shared_with_daily_overview() {
    let (mut cards, _) = Cards::init(
        Core::default(),
        Config {
            enabled_widgets: vec![],
            ..Default::default()
        },
    );
    let now = Local::now();
    let today = now.date_naive();
    let midnight = Local
        .from_local_datetime(&today.and_hms_opt(0, 0, 0).unwrap())
        .unwrap();
    cards.calendar = Some(CalendarData {
        account: "alex@example.com".into(),
        month: today.format("%Y-%m").to_string(),
        updated_at: now.to_rfc3339(),
        events: vec![meeting("Morning review", midnight, midnight)],
        ..Default::default()
    });
    cards.calendar_today = cards.calendar.clone();
    cards.config.calendar_show_today = true;
    cards.config.calendar_show_tomorrow = false;
    cards.config.glance.show_clock = false;
    cards.config.glance.show_weather = false;
    cards.config.glance.show_calendar = true;
    cards.config.glance.show_tomorrow = false;
    cards
        .config
        .widget_sizes
        .insert("calendar".into(), [300., 300.]);
    let mut native = Native::new(Size::new(300., 300.));
    let theme = cosmic::Theme::dark();
    assert!(cards.agenda_events(today).is_empty());
    assert!(cards.glance_events(today).is_empty());
    native.capture(
        cards.calendar_card(),
        &theme,
        "utility-calendar-finished-300",
    );
    let bounds = native.bounds(cards.calendar_card(), "calendar-earlier");
    let messages = click(&mut native, &cards, Widget::Calendar, bounds.center());
    assert!(
        matches!(messages.as_slice(), [Message::ToggleEarlier]),
        "{messages:?}"
    );
    let _ = cards.update(messages[0].clone());
    assert_eq!(cards.agenda_events(today).len(), 1);
    assert_eq!(cards.glance_events(today).len(), 1);
    let _ = cards.update(Message::ToggleEarlier);
    assert!(cards.agenda_events(today).is_empty());
    cards.calendar.as_mut().unwrap().events.extend([
        meeting(
            "Design review at the studio",
            now + Days::minutes(5),
            now + Days::minutes(35),
        ),
        CalendarEvent {
            meeting_url: Some("https://meet.google.com/sample".into()),
            ..meeting(
                "Planning the next release together",
                now + Days::hours(1),
                now + Days::hours(2),
            )
        },
    ]);
    cards.calendar_today = cards.calendar.clone();
    for width in [300., 360.] {
        cards
            .config
            .widget_sizes
            .insert("calendar".into(), [width, 420.]);
        let height = cards.calendar_natural_height().max(240.);
        cards
            .config
            .widget_sizes
            .insert("calendar".into(), [width, height]);
        let mut native = Native::new(Size::new(width, height));
        native.capture(
            cards.calendar_card(),
            &theme,
            &format!("utility-calendar-{width}"),
        );
    }
    cards
        .config
        .widget_sizes
        .insert("glance".into(), [300., 260.]);
    let mut native = Native::new(Size::new(300., 260.));
    native.capture(cards.glance(), &theme, "utility-overview-300");
    cards.config.enabled_widgets.push("calendar".into());
    assert!(
        cards.meeting_alert(),
        "in-person events also highlight the Calendar control"
    );
    cards.calendar_today.as_mut().unwrap().events.truncate(1);
    assert!(
        !cards.meeting_alert(),
        "finished events never highlight the control"
    );
    cards.show_earlier = true;
    cards.last_today = today - Days::days(1);
    let _ = cards.update(Message::Tick);
    assert!(
        !cards.show_earlier,
        "the next day starts with a fresh agenda"
    );
}

fn checkout(path: &str, branch: &str, changed: usize) -> data::Project {
    serde_json::from_value(serde_json::json!({
        "path":path,"name":"Atlas","title":"Atlas","repository_id":"atlas",
        "group":"projects","branch":branch,"changed":changed,"untracked":0,
        "conflicts":0,"ahead":2,"behind":0,"upstream":true,"error":false,
        "servers":[],"recent":0.0
    }))
    .unwrap()
}

#[test]
fn expanding_projects_does_not_open_folders_and_each_branch_targets_its_checkout() {
    let (mut cards, _) = Cards::init(
        Core::default(),
        Config {
            enabled_widgets: vec![],
            ..Default::default()
        },
    );
    cards.projects = Some(ProjectsData {
        updated_at: Local::now().to_rfc3339(),
        projects: vec![
            checkout("/projects/atlas", "main", 0),
            checkout("/worktrees/atlas-layout", "feature/calendar-layout", 3),
        ],
    });
    let theme = cosmic::Theme::dark();
    for width in [300., 360.] {
        cards.expanded_projects.clear();
        cards
            .config
            .widget_sizes
            .insert("projects".into(), [width, 400.]);
        let height = cards.projects_natural_height();
        let mut native = Native::new(Size::new(width, 400.));
        native.capture(
            cards.projects_card(),
            &theme,
            &format!("utility-projects-collapsed-{width}"),
        );
        let bounds = native.bounds(cards.projects_card(), "project-branches:repository:atlas");
        let messages = click(&mut native, &cards, Widget::Projects, bounds.center());
        assert!(
            matches!(messages.as_slice(), [Message::ToggleProject(_)]),
            "{messages:?}"
        );
        let _ = cards.update(messages[0].clone());
        assert!(cards.projects_natural_height() > height);
        let expanded_height = cards.projects_natural_height();
        cards
            .config
            .widget_sizes
            .insert("projects".into(), [width, expanded_height]);
        native.size = Size::new(width, expanded_height);
        native.capture(
            cards.projects_card(),
            &theme,
            &format!("utility-projects-expanded-{width}"),
        );
        for path in ["/projects/atlas", "/worktrees/atlas-layout"] {
            let bounds = native.bounds(cards.projects_card(), &format!("project-checkout:{path}"));
            let messages = click(&mut native, &cards, Widget::Projects, bounds.center());
            assert!(
                matches!(messages.as_slice(), [Message::Open(target)] if target == path),
                "{messages:?}"
            );
        }
        let _ = cards.update(Message::Projects(
            cards.config.project_roots.clone(),
            Ok(cards.projects.clone().unwrap()),
        ));
        assert!(cards.expanded_projects.contains("repository:atlas"));
    }
    let _ = cards.update(Message::Projects(
        cards.config.project_roots.clone(),
        Ok(ProjectsData::default()),
    ));
    assert!(cards.expanded_projects.is_empty());
}

#[test]
fn ram_categories_and_optional_swap_fit_the_smallest_card() {
    let (mut cards, _) = Cards::init(
        Core::default(),
        Config {
            enabled_widgets: vec![],
            ..Default::default()
        },
    );
    cards.config.devbox.swap = true;
    cards.config.devbox.gpu = false;
    cards.config.devbox.network = false;
    cards.config.devbox.uptime = false;
    cards.config.devbox.services = false;
    cards.metrics.cpu = 18.;
    cards.metrics.cpu_temperature = Some(46.);
    cards.metrics.disk = Some((112.4, 256.));
    cards.metrics.swap = Some((0.2, 8.));
    let height = cards.devbox_natural_height().max(180.);
    for width in [300., 360.] {
        cards
            .config
            .widget_sizes
            .insert("devbox".into(), [width, height]);
        let mut native = Native::new(Size::new(width, height));
        for (used, cache, free, swap) in [
            (7.9, 20., 2.7, (0.2, 8.)),
            (512.1, 256.7, 255.2, (128.0, 512.0)),
        ] {
            cards.metrics.memory_total = used + cache + free;
            cards.metrics.memory_used = used;
            cards.metrics.memory_cache = cache;
            cards.metrics.memory_free = free;
            cards.metrics.swap = Some(swap);
            for show_cache in [true, false] {
                cards.config.devbox.memory_cache = show_cache;
                let ram = native.bounds(cards.devbox(), "ram-summary");
                let swap = native.bounds(cards.devbox(), "swap-summary");
                assert!(
                    ram.height <= 16. && swap.height < ram.height,
                    "Swap must stay smaller: {ram:?} {swap:?}"
                );
                assert!(
                    (ram.center_y() - swap.center_y()).abs() <= 1.,
                    "Swap belongs on the RAM row"
                );
                assert!(swap.x >= 58. && swap.x + swap.width + 4. <= ram.x);
                assert!(
                    ram.x + ram.width <= width - 18.,
                    "Inline readings must fit the card"
                );
                native.capture(
                    cards.devbox(),
                    &cosmic::Theme::dark(),
                    &format!("compact-system-{width}-{}-{show_cache}", used as u32),
                );
            }
        }
    }
}

#[test]
fn dew_point_weather_details_fit_and_missing_readings_stay_hidden() {
    let mut config = Config::default();
    let mut weather: Weather = serde_json::from_value(serde_json::json!({
        "current":{"temperature_2m":-10.0,"apparent_temperature":-15.0,"weather_code":2,
            "wind_speed_10m":100.0,"relative_humidity_2m":100.0,"dew_point_2m":-25.4,"is_day":1},
        "daily":{"time":[],"temperature_2m_max":[],"temperature_2m_min":[],
            "weather_code":[],"precipitation_probability_max":[]}
    }))
    .unwrap();
    assert!(weather_dew_point(&config, &weather.current).is_none());
    config.weather.show_dew_point = true;
    assert_eq!(
        weather_dew_point(&config, &weather.current).as_deref(),
        Some("Dew point -25°C")
    );
    for width in [300., 360.] {
        let mut native = Native::new(Size::new(width, 180.));
        let details = native.bounds(
            weather_view(&config, Some(&weather), None, false, 180.),
            "weather-details",
        );
        assert!(details.x >= 0. && details.x + details.width <= width);
        assert!(
            details.height <= 27.,
            "Details fit at most two lines: {details:?}"
        );
        native.capture(
            weather_view(&config, Some(&weather), None, false, 180.),
            &cosmic::Theme::dark(),
            &format!("dew-point-weather-{width}"),
        );
    }
    weather.current.dew_point_2m = Some(-0.1);
    assert_eq!(
        weather_dew_point(&config, &weather.current).as_deref(),
        Some("Dew point 0°C")
    );
    for reading in [None, Some(f64::NAN), Some(f64::INFINITY)] {
        weather.current.dew_point_2m = reading;
        assert!(weather_dew_point(&config, &weather.current).is_none());
    }
}

#[test]
fn old_weather_and_partial_forecasts_stay_readable_at_minimum_width() {
    let now = Local::now();
    let (mut cards, _) = Cards::init(
        Core::default(),
        Config {
            enabled_widgets: vec![],
            ..Default::default()
        },
    );
    cards.config.location = Some(Location {
        name: "Lisbon".into(),
        latitude: 38.72,
        longitude: -9.14,
        timezone: None,
    });
    cards.weather_at = Some((now - Days::days(2)).to_rfc3339());
    cards.weather_error = true;
    cards.weather = Some(serde_json::from_value(serde_json::json!({
        "current":{"temperature_2m":18.0,"apparent_temperature":17.0,"weather_code":2,
            "wind_speed_10m":9.0,"relative_humidity_2m":62.0,"is_day":1},
        "daily":{"time":(0..3).map(|offset| (now.date_naive()-Days::days(2)+Days::days(offset)).to_string()).collect::<Vec<_>>(),
            "temperature_2m_max":[21.0,20.0],"temperature_2m_min":[14.0,13.0],
            "weather_code":[2,3,3],"precipitation_probability_max":[10.0,20.0,null]}
    })).unwrap());
    cards
        .config
        .widget_sizes
        .insert("weather".into(), [300., 180.]);
    let mut native = Native::new(Size::new(300., 180.));
    native.capture(
        cards.weather_card(),
        &cosmic::Theme::dark(),
        "utility-weather-saved-300",
    );
}

/// Render the actual widgets together: transparent text and translucent cards
/// need to be judged on the wallpaper, not against an empty test surface.
#[test]
fn widgets_remain_readable_on_a_vivid_wallpaper() {
    use cosmic::iced::widget::{pin, stack};
    let (mut cards, _) = Cards::init(
        Core::default(),
        Config {
            enabled_widgets: vec![],
            ..Default::default()
        },
    );
    let now = Local::now();
    cards.config.theme = "teal".into();
    cards.config.glance.enabled = true;
    cards.config.glance.show_tomorrow = false;
    cards.config.glance.density = crate::config::GlanceDensity::Compact;
    cards.config.devbox.swap = true;
    cards.config.devbox.uptime = false;
    cards.config.devbox.services = false;
    cards.config.location = Some(Location {
        name: "Lisbon".into(),
        latitude: 38.72,
        longitude: -9.14,
        timezone: None,
    });
    cards.weather = Some(serde_json::from_value(serde_json::json!({
        "current":{"temperature_2m":20.0,"apparent_temperature":20.0,"weather_code":2,
            "wind_speed_10m":9.0,"relative_humidity_2m":62.0,"is_day":1},
        "daily":{"time":[now.date_naive().to_string()], "temperature_2m_max":[22.0],
            "temperature_2m_min":[14.0],"weather_code":[2],"precipitation_probability_max":[10.0]}
    })).unwrap());
    cards.calendar = Some(CalendarData {
        month: now.format("%Y-%m").to_string(),
        updated_at: now.to_rfc3339(),
        events: vec![
            CalendarEvent {
                meeting_url: Some("https://meet.google.com/example".into()),
                ..meeting(
                    "Design review",
                    now + Days::minutes(9),
                    now + Days::minutes(39),
                )
            },
            meeting(
                "Project planning",
                now + Days::hours(1),
                now + Days::minutes(90),
            ),
            meeting(
                "Release checklist",
                now + Days::hours(2),
                now + Days::minutes(150),
            ),
        ],
        ..Default::default()
    });
    cards.calendar_today = cards.calendar.clone();
    cards.projects = Some(ProjectsData {
        updated_at: now.to_rfc3339(),
        projects: vec![
            checkout("/projects/atlas", "main", 3),
            checkout("/projects/atlas-design", "design", 0),
        ],
    });
    cards.metrics.cpu = 6.;
    cards.metrics.cpu_temperature = Some(48.);
    cards.metrics.memory_total = 30.7;
    cards.metrics.memory_used = 16.5;
    cards.metrics.memory_cache = 11.5;
    cards.metrics.memory_free = 2.7;
    cards.metrics.swap = Some((0., 20.));
    cards.metrics.disk = Some((214.4, 219.));
    cards.metrics.network = Some(data::NetworkMetrics {
        download: Some(146432.),
        upload: Some(19456.),
        interfaces: 1,
    });
    cards.metrics.gpus = vec![data::GpuMetrics {
        name: "Cezanne".into(),
        utilization: Some(7.),
        temperature: Some(35.),
        memory: Some((472446402, 536870912)),
        ..Default::default()
    }];
    let devbox_height = cards.devbox_natural_height();
    let projects_height = cards.projects_natural_height();
    cards
        .config
        .widget_sizes
        .insert("devbox".into(), [360., devbox_height]);
    cards
        .config
        .widget_sizes
        .insert("projects".into(), [360., projects_height]);
    let wallpaper =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wallpapers/teal.jpg");
    let scene = stack(vec![
        cosmic::widget::image(cosmic::widget::image::Handle::from_path(wallpaper))
            .width(Length::Fill)
            .height(Length::Fill)
            .content_fit(cosmic::iced::ContentFit::Cover)
            .into(),
        pin(cards.glance()).position(Point::new(24., 46.)).into(),
        pin(cards.devbox()).position(Point::new(896., 48.)).into(),
        pin(cards.projects_card())
            .position(Point::new(896., 56. + devbox_height))
            .into(),
    ])
    .width(Length::Fill)
    .height(Length::Fill);
    let mut native = Native::new(Size::new(1280., 720.));
    native.capture(
        scene.into(),
        &cosmic::Theme::dark(),
        "widgets-on-teal-wallpaper",
    );
}

#[test]
fn calendar_details_and_private_mode_work_in_both_agendas() {
    let (mut cards, _) = Cards::init(
        Core::default(),
        Config {
            enabled_widgets: vec![],
            ..Default::default()
        },
    );
    let today = Local::now().date_naive();
    let event = CalendarEvent {
        title: "Release planning".into(),
        start: today.to_string(),
        end: (today + Days::days(1)).to_string(),
        all_day: true,
        calendar_name: "Work".into(),
        description: "Review the next milestone and the open questions.".into(),
        location: "Meeting room".into(),
        meeting_url: Some("https://meet.google.com/fixture".into()),
        ..Default::default()
    };
    cards.calendar = Some(CalendarData {
        month: today.format("%Y-%m").to_string(),
        events: vec![event.clone()],
        ..Default::default()
    });
    cards.calendar_today = cards.calendar.clone();
    cards.config.calendar_show_tomorrow = true;
    let key = event.details_key();
    let height = cards.calendar_natural_height();
    let _ = cards.update(Message::Open(key.clone()));
    assert!(cards.event_details_open(&event));
    assert!(cards.calendar_natural_height() >= height + 184.);
    let mut native = Native::new(Size::new(360., 600.));
    native.capture(
        cards.calendar_card(),
        &cosmic::Theme::dark(),
        "calendar-event-details",
    );
    for density in [
        crate::config::GlanceDensity::Compact,
        crate::config::GlanceDensity::Comfortable,
    ] {
        cards.config.glance.density = density;
        native.capture(
            cards.glance(),
            &cosmic::Theme::dark(),
            &format!("calendar-glance-{density:?}"),
        );
    }
    let _ = cards.update(Message::Open(key.clone()));
    assert!(!cards.event_details_open(&event));
    cards.config.calendar_private = true;
    let _ = cards.update(Message::Open(key));
    assert!(!cards.event_details_open(&event));
    let spans = glance::agenda_spans_private(&event, Color::WHITE, Color::WHITE, true);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].text, "Busy");
    assert!(spans[0].link.is_none());
    native.capture(
        cards.calendar_card(),
        &cosmic::Theme::dark(),
        "calendar-busy",
    );
}
