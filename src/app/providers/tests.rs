use super::*;
use crate::displays::{
    OutputEvent,
    test_support::{Wire, info, isolated},
};

fn location(name: &str) -> Location {
    Location {
        name: name.into(),
        latitude: 45.,
        longitude: 16.,
        timezone: None,
    }
}
fn config() -> Config {
    Config {
        enabled_widgets: vec!["clock".into(), "weather".into()],
        location: Some(location("FB07 fixture A")),
        ..Default::default()
    }
}
fn token(cards: &Cards) -> crate::providers::weather::Token {
    cards
        .weather_hub
        .token_for_test(&Key::new(cards.config.location.as_ref().unwrap()).unwrap())
        .unwrap()
}
fn snapshot(location: Location) -> WeatherSnapshot {
    WeatherSnapshot {
        location,
        updated_at: Local::now().to_rfc3339(),
        weather: crate::app::settings_preview::weather_sample(Local::now().date_naive()),
    }
}

#[test]
fn fb07_runtime_city_switch_rejects_old_delivery_without_unblocking_new_work() {
    if isolated(
        "app::providers::tests::fb07_runtime_city_switch_rejects_old_delivery_without_unblocking_new_work",
    ) {
        return;
    }
    let (mut cards, _) = Cards::init(Core::default(), config());
    let old = token(&cards);
    let mut changed = cards.config.clone();
    changed.location = Some(location("FB07 fixture B"));
    let _ = cards.apply_config(changed);
    assert_eq!(cards.refresh_weather(Instant::now()).len(), 1);
    let current = token(&cards);
    let _ = cards.update(Message::Weather(Completion {
        token: old,
        result: Some(Ok(snapshot(location("FB07 fixture A")))),
    }));
    assert!(cards.weather.is_none());
    assert!(!cards.weather_error);
    assert!(cards.weather_fetching());
    assert_eq!(token(&cards), current);
    let _ = cards.update(Message::Weather(Completion {
        token: current,
        result: Some(Ok(snapshot(location("FB07 fixture B")))),
    }));
    assert!(cards.weather.is_some());
    assert!(!cards.weather_fetching());
    assert!(cards.refresh_weather(Instant::now()).is_empty());
}

#[test]
fn fb07_hidden_runtime_ignores_results_and_overview_handover_shares_request() {
    if isolated(
        "app::providers::tests::fb07_hidden_runtime_ignores_results_and_overview_handover_shares_request",
    ) {
        return;
    }
    let (mut cards, _) = Cards::init(Core::default(), config());
    let old = token(&cards);
    let mut grouped = cards.config.clone();
    grouped.glance.enabled = true;
    grouped.glance.initialized = true;
    grouped.glance.show_weather = true;
    grouped.glance.show_calendar = false;
    let _ = cards.apply_config(grouped);
    assert_eq!(token(&cards), old);
    assert!(cards.refresh_weather(Instant::now()).is_empty());
    let mut hidden = cards.config.clone();
    hidden.widgets_visible = false;
    let _ = cards.apply_config(hidden);
    assert_eq!(cards.next_wake(), None);
    assert!(!cards.weather_fetching());
    let _ = cards.update(Message::Weather(Completion {
        token: old,
        result: Some(Ok(snapshot(location("FB07 fixture A")))),
    }));
    assert!(cards.weather.is_none());
    let mut shown = cards.config.clone();
    shown.widgets_visible = true;
    let _ = cards.apply_config(shown);
    assert_eq!(cards.refresh_weather(Instant::now()).len(), 1);
}

#[test]
fn fb07_missing_output_releases_clock_and_weather_demand() {
    if isolated("app::providers::tests::fb07_missing_output_releases_clock_and_weather_demand") {
        return;
    }
    let wire = Wire::new();
    let a = wire.output(1);
    let b = wire.output(2);
    let mut cfg = config();
    cfg.widget_display = Some("DP-2".into());
    for widget in [Widget::Clock, Widget::Weather] {
        cfg.widget_monitors.insert(
            widget.key().into(),
            crate::config::monitors::WidgetMonitor {
                display: Some("DP-2".into()),
                hide_disconnected: true,
            },
        );
    }
    let (mut cards, _) = Cards::init(Core::default(), cfg);
    let old = token(&cards);
    cards.displays.update(
        OutputEvent::Created(Some(info(Some("DP-1"), (1920, 1080)))),
        a,
    );
    let _ = cards.reconcile_output();
    assert!(!cards.provider_visible(Widget::Clock));
    assert!(!cards.weather_fetching());
    assert_eq!(cards.next_wake(), None);
    let _ = cards.update(Message::Weather(Completion {
        token: old,
        result: None,
    }));
    cards.displays.update(
        OutputEvent::Created(Some(info(Some("DP-2"), (1920, 1080)))),
        b,
    );
    let _ = cards.reconcile_output();
    assert!(cards.provider_visible(Widget::Clock));
    assert_eq!(cards.refresh_weather(Instant::now()).len(), 1);
}

#[test]
fn fb07_clock_views_share_one_observation_and_hidden_ticks_do_not_advance_it() {
    if isolated(
        "app::providers::tests::fb07_clock_views_share_one_observation_and_hidden_ticks_do_not_advance_it",
    ) {
        return;
    }
    use chrono::TimeZone;
    let mut cfg = config();
    cfg.enabled_widgets = vec!["clock".into()];
    cfg.location = None;
    let (mut cards, _) = Cards::init(Core::default(), cfg);
    cards.clock_at = Local.with_ymd_and_hms(2026, 10, 26, 1, 2, 0).unwrap();
    let mut native = crate::native_test::Native::new(cosmic::iced::Size::new(490., 200.));
    native.text_bounds(cards.clock(), "01:02");
    // Both adapters retain that exact observation regardless of wall-clock now.
    cards.config.glance.enabled = true;
    cards.config.glance.show_clock = true;
    cards.config.glance.show_weather = false;
    cards.config.glance.show_calendar = false;
    let mut native = crate::native_test::Native::new(cosmic::iced::Size::new(640., 400.));
    native.text_bounds(cards.glance(), "01:02");
    cards.config.widgets_visible = false;
    let before = cards.clock_at;
    cards.next_clock = Instant::now();
    let _ = cards.refresh_visible_data();
    assert_eq!(cards.clock_at, before);
    assert_eq!(cards.next_wake(), None);
    cards.config.widgets_visible = true;
    let _ = cards.refresh_visible_data();
    assert_ne!(cards.clock_at, before);
    assert!(cards.next_clock > Instant::now());
}
#[test]
fn fb17_recipe_parts_and_previews_share_the_existing_weather_request() {
    if isolated(
        "app::providers::tests::fb17_recipe_parts_and_previews_share_the_existing_weather_request",
    ) {
        return;
    }
    let (mut cards, _) = Cards::init(Core::default(), config());
    let before = token(&cards);
    let mut changed = cards.config.clone();
    let mut recipe = crate::recipes::Recipe::starter(crate::recipes::Kind::Weather);
    recipe.parts = recipe.kind.parts().to_vec();
    recipe.seal();
    changed.recipes.weather = Some(recipe.clone());
    let _ = cards.apply_config(changed);
    assert_eq!(token(&cards), before);
    assert!(cards.refresh_weather(Instant::now()).is_empty());
    for _ in 0..12 {
        let _ = crate::app::settings_recipe_preview(&cards.config, &recipe, false, [520., 220.]);
    }
    assert_eq!(token(&cards), before);
    assert!(cards.refresh_weather(Instant::now()).is_empty());
}

#[test]
fn architecture_output_demand_preserves_background_consumers() {
    if isolated("app::providers::tests::architecture_output_demand_preserves_background_consumers")
    {
        return;
    }
    let mut config = Config::default();
    config.glance.enabled = true;
    config.glance.initialized = true;
    config.glance.show_clock = false;
    config.glance.show_weather = false;
    config.glance.show_calendar = false;
    config.project_roots = vec!["/fixture/projects".into()];
    config.music.enabled = true;
    config.focus.enabled = true;
    config.devbox.services = true;
    for widget in [
        Widget::Projects,
        Widget::Devbox,
        Widget::Music,
        Widget::Focus,
        Widget::Calendar,
    ] {
        config.widget_monitors.insert(
            widget.key().into(),
            crate::config::monitors::WidgetMonitor {
                display: Some("DP-2".into()),
                hide_disconnected: true,
            },
        );
    }
    let (mut cards, _) = Cards::init(Core::default(), config);
    assert!(
        cards.projects_fetching && cards.metrics_fetching,
        "startup prefetch retained"
    );
    cards.projects_fetching = false;
    cards.metrics_fetching = false;
    cards.service_fetching = false;
    let due = Instant::now() - Duration::from_secs(1);
    cards.next_projects = due;
    cards.next_metrics = due;
    cards.next_service = due;
    let wire = Wire::new();
    cards.displays.update(
        OutputEvent::Created(Some(info(Some("DP-1"), (1920, 1080)))),
        wire.output(1),
    );
    assert!(cards.widget_output(Widget::Glance).is_some());
    assert!(!cards.provider_visible(Widget::Projects));
    assert!(!cards.provider_visible(Widget::Devbox));
    assert!(!cards.music_demand());
    assert!(cards.next_wake().is_none());
    drop(cards.refresh_visible_data());
    assert!(!cards.projects_fetching && !cards.metrics_fetching && !cards.service_fetching);

    // The rail remains a Calendar consumer, independently of its hidden output.
    cards.config.glance.enabled = false;
    assert!(!cards.provider_visible(Widget::Calendar));
    assert!(cards.calendar_demand());
    cards.config.glance.enabled = true;
    let wall = crate::tools::focus::now_ms();
    cards
        .tools
        .focus
        .toggle(wall, &cards.config.focus, &crate::tools::focus::today());
    let wake = cards
        .tools_wake()
        .expect("hidden running round must still finish");
    assert!(
        wake > Instant::now() + Duration::from_secs(30),
        "no hidden per-second redraw"
    );

    cards.displays.update(
        OutputEvent::Created(Some(info(Some("DP-2"), (1920, 1080)))),
        wire.output(2),
    );
    assert!(
        cards.music_demand(),
        "discover a player even when Music is empty"
    );
    assert!(cards.next_wake().is_some());
    drop(cards.refresh_visible_data());
    assert!(cards.projects_fetching && cards.metrics_fetching && cards.service_fetching);
    cards.config.widgets_visible = false;
    assert!(!cards.music_demand() && !cards.calendar_demand());
    assert!(cards.tools_wake().is_some());
}
