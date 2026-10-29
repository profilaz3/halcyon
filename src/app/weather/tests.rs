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
    assert_eq!(forecast.len(), 3);
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
