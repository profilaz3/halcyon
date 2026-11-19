use super::*;
use crate::native_test::Native;
use chrono::TimeZone;
use cosmic::iced::{Event, Point, Size, mouse};

fn now() -> chrono::DateTime<Local> {
    Local.with_ymd_and_hms(2026, 10, 26, 23, 59, 0).unwrap()
}
fn configured() -> Config {
    let mut config = Config::default();
    config.location = Some(crate::config::Location {
        name: "Example city".into(),
        latitude: 45.,
        longitude: 16.,
        timezone: None,
    });
    config
}

#[test]
fn fb06_weather_live_and_preview_pixels_match_for_all_content_states() {
    for family in [
        crate::desktop_theme::Family::Soft,
        crate::desktop_theme::Family::Paper,
    ] {
        let mut config = configured();
        config.appearance.family = Some(family);
        let weather = super::super::settings_preview::weather_sample(now().date_naive());
        let fresh = now().to_rfc3339();
        let old = (now() - Days::days(3)).to_rfc3339();
        for (reading, updated_at, failed) in [
            (Some(&weather), Some(fresh.as_str()), false),
            (Some(&weather), Some(old.as_str()), true),
            (None, None, false),
            (None, None, true),
        ] {
            let presentation = Presentation::new(
                &config,
                Snapshot {
                    weather: reading,
                    updated_at,
                    failed,
                },
                now(),
            );
            for size in [[300., 180.], [360., 180.], [360., 440.], [620., 240.]] {
                let mut live = Native::new(Size::new(size[0], size[1]));
                let mut preview = Native::new(live.size);
                assert_eq!(
                    live.render(
                        presentation.view(Context::Live, size),
                        &cosmic::Theme::dark()
                    ),
                    preview.render(
                        presentation.view(Context::LocalPreview, size),
                        &cosmic::Theme::dark()
                    )
                );
            }
        }
    }
}

#[test]
fn fb06_weather_preview_cannot_emit_the_live_location_action() {
    let config = Config::default();
    let p = Presentation::new(
        &config,
        Snapshot {
            weather: None,
            updated_at: None,
            failed: false,
        },
        now(),
    );
    let size = p.metrics().natural;
    for context in [Context::Live, Context::LocalPreview] {
        let mut native = Native::new(Size::new(size[0], size[1]));
        let point = native
            .bounds(p.view(context, size), "weather-location-action")
            .center();
        let mut messages = Vec::new();
        for event in [
            mouse::Event::CursorMoved { position: point },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            messages.extend(
                native
                    .event(p.view(context, size), Event::Mouse(event), point)
                    .0,
            );
        }
        if context == Context::Live {
            assert!(matches!(messages.as_slice(), [Message::WeatherSettings]));
        } else {
            assert!(messages.is_empty(), "preview emitted a desktop action");
        }
    }
}

#[test]
fn fb06_weather_freezes_date_status_and_style_without_retaining_provider_data() {
    let mut config = configured();
    config.appearance.family = Some(crate::desktop_theme::Family::Paper);
    config.weather.show_dew_point = true;
    let mut weather = super::super::settings_preview::weather_sample(now().date_naive());
    weather
        .daily
        .time
        .extend((3..100).map(|n| (now().date_naive() + Days::days(n)).to_string()));
    let old = (now() - Days::days(3)).to_rfc3339();
    let p = Presentation::new(
        &config,
        Snapshot {
            weather: Some(&weather),
            updated_at: Some(&old),
            failed: true,
        },
        now(),
    );
    let Content::Reading {
        status,
        forecast,
        details,
        ..
    } = &p.content
    else {
        panic!("reading")
    };
    assert_eq!(status.as_deref(), Some("Saved · 3d ago\nRefresh failed"));
    assert_eq!(forecast.len(), 4);
    assert_eq!(forecast[0].title, "Today");
    assert!(details.contains("Dew point"));
    drop(weather);
    let size = p.metrics().natural;
    let mut native = Native::new(Size::new(size[0], size[1]));
    let before = native.render(p.view(Context::LocalPreview, size), &cosmic::Theme::dark());
    config.appearance.family = Some(crate::desktop_theme::Family::Neon);
    config.widget_font = Some("DejaVu Sans Mono".into());
    let _other = crate::widget_style::scope(&config);
    assert_eq!(
        before,
        native.render(p.view(Context::LocalPreview, size), &cosmic::Theme::dark())
    );
}

#[test]
fn fb06_weather_preview_bounds_preserve_scroll_access_and_no_actions() {
    let config = configured();
    let weather = super::super::settings_preview::weather_sample(now().date_naive());
    let p = Presentation::new(
        &config,
        Snapshot {
            weather: Some(&weather),
            updated_at: None,
            failed: false,
        },
        now(),
    );
    assert_eq!(
        p.metrics().minimum,
        crate::layout::minimum_size(Widget::Weather)
    );
    let mut native = Native::new(Size::new(300., 180.));
    let bounds = native.bounds(
        p.view(Context::LocalPreview, [300., 180.]),
        "weather-details",
    );
    assert!(bounds.x >= 0. && bounds.x + bounds.width <= 300.);
    let point = Point::new(150., 90.);
    assert!(
        native
            .event(
                p.view(Context::LocalPreview, [300., 180.]),
                Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Lines { x: 0., y: -3. }
                }),
                point
            )
            .0
            .is_empty()
    );
}

#[test]
fn fb06_weather_live_preview_parity_at_scaled_outputs() {
    let config = configured();
    let weather = super::super::settings_preview::weather_sample(now().date_naive());
    let p = Presentation::new(
        &config,
        Snapshot {
            weather: Some(&weather),
            updated_at: None,
            failed: false,
        },
        now(),
    );
    let size = p.metrics().minimum;
    for scale in [1., 1.5, 2.] {
        let mut live = Native::new(Size::new(size[0], size[1]));
        let mut preview = Native::new(live.size);
        let a = live.render_scaled(p.view(Context::Live, size), &cosmic::Theme::dark(), scale);
        let b = preview.render_scaled(
            p.view(Context::LocalPreview, size),
            &cosmic::Theme::dark(),
            scale,
        );
        assert_eq!(
            a.len(),
            (f64::from(size[0]) * f64::from(size[1]) * scale * scale * 4.) as usize
        );
        assert_eq!(a, b);
    }
}

#[test]
fn fb06_weather_preserves_native_card_state_through_reordering() {
    let config = configured();
    let weather = super::super::settings_preview::weather_sample(now().date_naive());
    let p = Presentation::new(
        &config,
        Snapshot {
            weather: Some(&weather),
            updated_at: None,
            failed: false,
        },
        now(),
    );
    let mut native = Native::new(Size::new(360., 500.));
    for widgets in [
        [Widget::Weather, Widget::Devbox],
        [Widget::Devbox, Widget::Weather],
        [Widget::Weather, Widget::Devbox],
    ] {
        let tree = cosmic::iced::widget::keyed_column(widgets.into_iter().map(|widget| {
            (
                widget,
                if widget == Widget::Weather {
                    p.view(Context::Live, [360., 180.])
                } else {
                    super::super::settings_devbox_preview(&config)
                },
            )
        }))
        .spacing(crate::drawer::CARD_GAP);
        let pixels = native.render(tree.into(), &cosmic::Theme::dark());
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] > 0));
    }
}

#[test]
fn outlook_resizes_uses_actual_units_and_handles_missing_or_stale_readings() {
    let mut config = configured();
    config.weather.layout = crate::config::WeatherLayout::Outlook;
    config.weather.fahrenheit = true;
    let today = now().date_naive();
    let mut weather = super::super::settings_preview::weather_sample(today);
    weather.current.temperature_2m = 12.;
    weather.current.apparent_temperature = 8.;
    weather.current.weather_code = 61;
    weather.daily.temperature_2m_min[0] = 8.;
    weather.daily.temperature_2m_max[0] = 15.;
    for (width, height, forecast) in [(300., 180., false), (360., 320., true), (560., 180., true)] {
        let p = Presentation::new(
            &config,
            Snapshot {
                weather: Some(&weather),
                updated_at: None,
                failed: false,
            },
            now(),
        );
        let mut native = Native::new(Size::new(width, height));
        let view = || p.view(Context::LocalPreview, [width, height]);
        let text = native.all_text(view());
        assert!(text.iter().any(|t| t == "54°"));
        assert!(text.iter().any(|t| t == "Feels like 46°"));
        assert!(text.iter().any(|t| t == "Today"));
        assert_eq!(text.iter().any(|t| t == "Forecast"), forecast);
        assert!(native.scrollables(view()).is_empty());
        for (s, r) in native.placed_text(view()) {
            assert!(
                r.x >= 0.
                    && r.y >= 0.
                    && r.x + r.width <= width + 1.
                    && r.y + r.height <= height + 1.,
                "{s}: {r:?}"
            );
        }
        native.capture(
            view(),
            &cosmic::Theme::dark(),
            &format!("weather-outlook-{width}x{height}"),
        );
    }
    let mut original = config.clone();
    original.weather.layout = crate::config::WeatherLayout::Saved;
    let old_card = Presentation::new(
        &original,
        Snapshot {
            weather: Some(&weather),
            updated_at: None,
            failed: false,
        },
        now(),
    );
    let mut native = Native::new(Size::new(560., 180.));
    let classic = || old_card.view(Context::LocalPreview, [560., 180.]);
    assert!(
        native
            .all_text(classic())
            .iter()
            .any(|s| s.contains("Humidity"))
    );
    native.capture(classic(), &cosmic::Theme::dark(), "weather-classic-560x180");
    weather.daily.temperature_2m_min.clear();
    let p = Presentation::new(
        &config,
        Snapshot {
            weather: Some(&weather),
            updated_at: None,
            failed: true,
        },
        now(),
    );
    let mut native = Native::new(Size::new(300., 180.));
    let text = native.all_text(p.view(Context::Live, [300., 180.]));
    assert!(!text.iter().any(|s| s == "Today"));
    assert!(text.iter().any(|s| s.contains("Refresh failed")));
}

#[test]
fn classic_reflows_the_forecast_with_the_widget() {
    let config = configured();
    let weather = super::super::settings_preview::weather_sample(now().date_naive());
    let p = Presentation::new(
        &config,
        Snapshot {
            weather: Some(&weather),
            updated_at: None,
            failed: false,
        },
        now(),
    );
    for size in [
        [300., 180.],
        [420., 235.],
        [360., 260.],
        [360., 320.],
        [510., 180.],
        [560., 180.],
        [560., 240.],
    ] {
        let mut native = Native::new(Size::new(size[0], size[1]));
        let view = || p.view(Context::Live, size);
        let today = native.text_bounds(view(), "Today");
        let tomorrow = native.text_bounds(view(), "Tomorrow");
        let labels = native.all_text(view());
        let fourth_day = size[1] >= 320. || (size[0] >= 510. && size[1] >= 224.);
        assert_eq!(labels.iter().any(|s| s == "Thu 29"), fourth_day);
        let city = native.text_bounds(view(), "Example city");
        if size[0] >= 510. {
            let heading = native.text_bounds(view(), "Forecast");
            assert!(today.x > size[0] * 0.4 && (heading.y - city.y).abs() < 2.);
            assert!(tomorrow.y > today.y && (today.x - tomorrow.x).abs() < 1.);
        } else if size[1] >= 260. {
            assert!(tomorrow.y > today.y && (today.x - tomorrow.x).abs() < 1.);
        } else {
            assert!(tomorrow.x > today.x && (today.y - tomorrow.y).abs() < 1.);
        }
        assert!(native.scrollables(view()).is_empty());
        for (label, bounds) in native.placed_text(view()) {
            assert!(
                bounds.x >= 0.
                    && bounds.y >= 0.
                    && bounds.x + bounds.width <= size[0] + 1.
                    && bounds.y + bounds.height <= size[1] + 1.,
                "{label}: {bounds:?}"
            );
        }
        native.capture(
            view(),
            &cosmic::Theme::dark(),
            &format!("classic-reflow-{}x{}", size[0], size[1]),
        );
    }
}

#[test]
fn classic_forecast_keeps_partial_and_subzero_readings_on_resize() {
    let mut config = configured();
    config.weather.show_dew_point = true;
    config.location.as_mut().unwrap().name = "Saint-Rémy-en-Bouzemont-Saint-Genest-et-Isson".into();
    let mut weather = super::super::settings_preview::weather_sample(now().date_naive());
    weather.daily.temperature_2m_min = vec![-11., -2., f64::NAN];
    weather.daily.temperature_2m_max = vec![-3., 4., 9.];
    weather.daily.weather_code[0] = 95;
    weather.daily.precipitation_probability_max[0] = None;
    let p = Presentation::new(
        &config,
        Snapshot {
            weather: Some(&weather),
            updated_at: None,
            failed: true,
        },
        now(),
    );
    for size in [[510., 180.], [320., 320.]] {
        let mut native = Native::new(Size::new(size[0], size[1]));
        let view = || p.view(Context::Live, size);
        let labels = native.all_text(view());
        for expected in ["-11°", "-3°", "4°", "9°", "Thunderstorms"] {
            assert!(labels.iter().any(|s| s == expected), "{expected}");
        }
        assert!(labels.iter().any(|s| s.contains("Refresh failed")));
        assert!(!labels.iter().any(|s| s == "Rain 10%"));
        let details = native.bounds(view(), "weather-details");
        assert!(
            details.height >= 14. && details.y + details.height <= size[1] - 12.,
            "{size:?} {details:?}"
        );
        assert!(native.scrollables(view()).is_empty());
        native.capture(
            view(),
            &cosmic::Theme::dark(),
            &format!("classic-partial-{}x{}", size[0], size[1]),
        );
    }
}
