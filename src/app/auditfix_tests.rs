use super::*;
use crate::native_test::Native;
use chrono::TimeZone;
use cosmic::Application;
use cosmic::iced::{Rectangle, Size};

fn isolated(name: &str) -> bool {
    if std::env::var("HALCYON_RUNTIME_AUDITFIX_TEST").as_deref() == Ok(name) {
        return false;
    }
    let root = std::env::temp_dir().join(format!(
        "halcyon-runtime-auditfix-{}-{name}",
        std::process::id()
    ));
    for dir in ["config", "data", "cache", "state"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join("config.json"), "{}").unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("app::auditfix_tests::{name}"),
            "--nocapture",
        ])
        .env("HALCYON_RUNTIME_AUDITFIX_TEST", name)
        .env("DESKTOP_CARDS_CONFIG", root.join("config.json"))
        .env("DESKTOP_CARDS_DESKTOP_ROOT", &root)
        .env("DESKTOP_CARDS_CACHE_DIR", root.join("cache"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .status()
        .unwrap();
    assert!(status.success(), "fixture {}", root.display());
    std::fs::remove_dir_all(root).unwrap();
    true
}

fn location() -> Location {
    Location {
        name: "Fixture city".into(),
        latitude: 45.,
        longitude: 15.,
        timezone: None,
    }
}

#[test]
fn auditfix_ob03_planned_viewports_match_runtime_and_collision_positions() {
    if isolated("auditfix_ob03_planned_viewports_match_runtime_and_collision_positions") {
        return;
    }
    let raw = serde_json::json!({
        "enabled_widgets": ["devbox"], "notes": {"enabled": true},
        "widget_sizes": {"devbox": [360,180], "notes": [360,180]}
    });
    let config: Config = serde_json::from_value(raw.clone()).unwrap();
    let snapshot = crate::layout_state::defaults(&config);
    let mut draft = crate::settings::TestWidgetDraft::new(&config, &raw, &snapshot);
    draft.setup_order = vec!["devbox".into(), "notes".into()];
    draft.reset_order(&draft.active());
    let plan = draft.plan(
        &config,
        snapshot,
        &crate::displays::Registry::default(),
        [24.; 4],
    );
    assert!(plan.error.is_none(), "{:?}", plan.error);
    let mut committed = raw.clone();
    for (pointer, entry) in draft
        .patch(&plan, &crate::displays::Registry::default())
        .unwrap()
        .changes()
    {
        crate::desktop::store::widget::put(&mut committed, pointer, entry.desired.clone()).unwrap();
    }
    assert_eq!(committed["widget_sizes"], raw["widget_sizes"]);
    let (mut cards, _) = Cards::init(Core::default(), serde_json::from_value(committed).unwrap());
    let _output = cards.test_output_ready();
    cards.screen_size = plan.snapshot.screen;
    // Both zero detected GPUs and one placeholder use one GPU row.
    assert_eq!(super::devbox::natural_height(&cards.config.devbox, 0), 226.);
    assert_eq!(super::devbox::natural_height(&cards.config.devbox, 1), 226.);
    for widget in [Widget::Devbox, Widget::Notes] {
        let geometry = plan
            .snapshot
            .widgets
            .iter()
            .find(|g| g.key == widget.key())
            .unwrap();
        assert_eq!(cards.widget_size(widget), geometry.size);
        let actual = cards.placed_position(widget);
        for axis in 0..2 {
            assert!(
                (actual[axis] - geometry.position[axis]).abs() < 0.01,
                "{widget:?}: {actual:?} != {:?}",
                geometry.position
            );
        }
    }
    let saved = cards.config.widget_sizes.clone();
    cards.screen_size = [420., 300.];
    assert_eq!(cards.widget_size(Widget::Devbox), [360., 204.]);
    let small = crate::layout_state::project(
        crate::layout_state::Snapshot {
            screen: cards.screen_size,
            ..plan.snapshot
        },
        &cards.config,
    );
    assert_eq!(
        small
            .widgets
            .iter()
            .find(|g| g.key == "devbox")
            .unwrap()
            .size,
        cards.widget_size(Widget::Devbox)
    );
    assert_eq!(cards.config.widget_sizes, saved);
}

#[test]
fn auditfix_w02_hidden_glance_stops_wakes_and_refreshes_but_running_focus_survives() {
    if isolated("auditfix_w02_hidden_glance_stops_wakes_and_refreshes_but_running_focus_survives") {
        return;
    }
    let mut config = Config::default();
    config.enabled_widgets.clear();
    config.glance.enabled = true;
    config.glance.initialized = true;
    config.glance.show_calendar = false;
    config.location = Some(location());
    let (mut cards, _) = Cards::init(Core::default(), config);
    let mut hidden = cards.config.clone();
    hidden.widgets_visible = false;
    let _ = cards.apply_config(hidden);
    cards.weather_hub =
        crate::providers::weather::Hub::new(crate::widget_presentation::Context::Live);
    cards.weather_hub.due_now(Instant::now());
    cards.next_clock = Instant::now();
    let before = cards.next_clock;
    assert!(cards.next_wake().is_none());
    assert!(cards.refresh_visible_data().is_empty());
    assert!(!cards.weather_fetching());
    assert_eq!(cards.next_clock, before);
    let now = crate::tools::focus::now_ms();
    cards
        .tools
        .focus
        .toggle(now, &cards.config.focus, &crate::tools::focus::today());
    let session = cards.tools.focus.clone();
    assert!(cards.tools_wake().is_some());
    assert!(cards.wake_deadline().is_some());
    assert!(cards.refresh_visible_data().is_empty());
    assert_eq!(cards.tools.focus, session);
    let mut shown = cards.config.clone();
    shown.widgets_visible = true;
    let _ = cards.apply_config(shown);
    assert!(cards.next_wake().is_some());
    // These tasks are only constructed; the fixture makes no provider requests.
    assert_eq!(cards.refresh_visible_data().len(), 1);
    assert!(cards.weather_fetching());
}

#[test]
fn auditfix_w03_age_failure_and_expired_forecast_dates_are_distinct() {
    let now = Local.with_ymd_and_hms(2026, 10, 26, 12, 0, 0).unwrap();
    let config = Config::default();
    let fresh = (now - Days::minutes(2)).to_rfc3339();
    assert_eq!(weather_status(&config, Some(&fresh), false, now), None);
    assert_eq!(
        weather_status(&config, Some(&fresh), true, now).as_deref(),
        Some("Saved · 2m ago\nRefresh failed")
    );
    assert_eq!(
        weather_status(
            &config,
            Some(&(now - Days::minutes(15)).to_rfc3339()),
            false,
            now
        )
        .as_deref(),
        Some("Saved · 15m ago")
    );
    assert_eq!(
        weather_status(
            &config,
            Some(&(now - Days::days(3)).to_rfc3339()),
            false,
            now
        )
        .as_deref(),
        Some("Saved · 3d ago")
    );
    assert_eq!(
        weather_status(&config, Some("invalid"), false, now).as_deref(),
        Some("Saved · time unknown")
    );
    let daily = data::Daily {
        time: vec![
            "2026-10-23".into(),
            "invalid".into(),
            "2026-10-26".into(),
            "2026-10-27".into(),
        ],
        temperature_2m_max: vec![],
        temperature_2m_min: vec![],
        weather_code: vec![],
        precipitation_probability_max: vec![],
    };
    assert_eq!(
        weather_forecast_days(&daily, now.date_naive())
            .map(|(index, _)| index)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
}

fn visible(bounds: Rectangle, size: Size) {
    assert!(bounds.width > 0. && bounds.height > 0.);
    assert!(
        bounds.x >= 0.
            && bounds.y >= 0.
            && bounds.x + bounds.width <= size.width + 1.
            && bounds.y + bounds.height <= size.height + 1.,
        "status must fit: {bounds:?} in {size:?}"
    );
}

fn assert_weather_status_views(cards: &mut Cards, expected: &str) {
    let size = Size::new(300., 180.);
    let mut native = Native::new(size);
    visible(native.text_bounds(cards.weather_card(), expected), size);
    visible(native.bounds(cards.weather_card(), "weather-status"), size);
    for style in [
        crate::config::GlanceStyle::Transparent,
        crate::config::GlanceStyle::Panel,
    ] {
        for density in [
            crate::config::GlanceDensity::Compact,
            crate::config::GlanceDensity::Comfortable,
        ] {
            for clock in [false, true] {
                cards.config.glance.style = style;
                cards.config.glance.density = density;
                cards.config.glance.show_clock = clock;
                let [width, height] = cards.glance_size();
                let size = Size::new(width, height);
                let mut native = Native::new(size);
                visible(native.text_bounds(cards.glance(), expected), size);
                visible(native.bounds(cards.glance(), "glance-weather-status"), size);
            }
        }
    }
}

#[test]
fn auditfix_w03_offline_cache_and_refresh_failure_are_visible_in_both_views() {
    if isolated("auditfix_w03_offline_cache_and_refresh_failure_are_visible_in_both_views") {
        return;
    }
    let now = Local::now();
    let weather: Weather = serde_json::from_value(serde_json::json!({
        "current":{"temperature_2m":18.,"apparent_temperature":17.,"weather_code":2,
            "wind_speed_10m":9.,"relative_humidity_2m":62.,"is_day":1},
        "daily":{"time":[(now.date_naive()-Days::days(3)).to_string()],
            "temperature_2m_max":[21.],"temperature_2m_min":[14.],
            "weather_code":[2],"precipitation_probability_max":[10.]}
    }))
    .unwrap();
    let cached = WeatherSnapshot {
        location: location(),
        updated_at: (now - Days::days(3)).to_rfc3339(),
        weather,
    };
    let cache_dir = std::path::PathBuf::from(std::env::var_os("DESKTOP_CARDS_CACHE_DIR").unwrap());
    std::fs::write(
        cache_dir.join("weather.json"),
        serde_json::to_vec(&cached).unwrap(),
    )
    .unwrap();
    let mut config = Config::default();
    config.enabled_widgets.clear();
    config.location = Some(location());
    config.glance.enabled = true;
    config.glance.show_weather = true;
    config.glance.show_calendar = false;
    config.widget_sizes.insert("weather".into(), [300., 180.]);
    let (mut cards, _) = Cards::init(Core::default(), config);
    assert!(!cards.weather_error);
    assert_weather_status_views(&mut cards, "Saved · 3d ago");
    let mut native = Native::new(Size::new(300., 180.));
    native.text_bounds(cards.weather_card(), "Forecast unavailable");
    let _ = cards.update(weather_delivery(&cards, Err("fixture offline".into())));
    assert_weather_status_views(&mut cards, "Saved · 3d ago\nRefresh failed");
    assert_eq!(
        serde_json::to_value(cards.weather.as_ref().unwrap()).unwrap(),
        serde_json::to_value(&cached.weather).unwrap()
    );
    // A successful retry removes the stale indicator in both real widget trees.
    cards.weather_hub.due_now(Instant::now());
    let _ = cards.refresh_weather(Instant::now());
    let _ = cards.update(weather_delivery(
        &cards,
        Ok(WeatherSnapshot {
            updated_at: Local::now().to_rfc3339(),
            ..cached
        }),
    ));
    assert!(
        native
            .try_bounds(cards.weather_card(), "weather-status")
            .is_none()
    );
    let [width, height] = cards.glance_size();
    let mut native = Native::new(Size::new(width, height));
    assert!(
        native
            .try_bounds(cards.glance(), "glance-weather-status")
            .is_none()
    );
    cards.weather_hub.due_now(Instant::now());
    let _ = cards.refresh_weather(Instant::now());
    let _ = cards.update(weather_delivery(&cards, Err("fixture retry failed".into())));
    assert_weather_status_views(&mut cards, "Saved · just now\nRefresh failed");
}

fn weather_delivery(cards: &Cards, result: Result<WeatherSnapshot, String>) -> Message {
    let key = crate::providers::weather::Key::new(cards.config.location.as_ref().unwrap()).unwrap();
    Message::Weather(crate::providers::weather::Completion {
        token: cards.weather_hub.token_for_test(&key).unwrap(),
        result: Some(result),
    })
}
