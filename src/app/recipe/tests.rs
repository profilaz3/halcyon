use super::*;
#[test]
fn fb18_recipe_clock_does_not_inherit_daily_overview_fonts() {
    let mut config = Config::default();
    config.clock.surface = Some(crate::widget_style::resolver::ClockSurface::Panel);
    config.recipes.clock = Some(Recipe::starter(Kind::Clock));
    let now = chrono::NaiveDate::from_ymd_opt(2026, 7, 21)
        .unwrap()
        .and_hms_opt(9, 41, 0)
        .unwrap();
    let size = [320., 220.];
    let mut native = crate::native_test::Native::new(cosmic::iced::Size::new(size[0], size[1]));
    let original = native.render(clock::view(&config, now, size), &cosmic::Theme::dark());
    config.glance.clock_font = Some("DejaVu Sans Mono".into());
    config.glance.font = Some("DejaVu Serif".into());
    assert_eq!(
        native.render(clock::view(&config, now, size), &cosmic::Theme::dark()),
        original
    );
}
#[test]
fn fb17_recipe_action_uses_widget_ink_and_keeps_native_keyboard_focus() {
    use cosmic::iced::{Event, Point, Size, keyboard};
    let now = chrono::NaiveDate::from_ymd_opt(2026, 7, 21)
        .unwrap()
        .and_hms_opt(9, 41, 0)
        .unwrap();
    for family in [
        crate::desktop_theme::Family::Paper,
        crate::desktop_theme::Family::Midnight,
    ] {
        let mut config = Config::default();
        config.appearance.family = Some(family);
        config.clock.surface = Some(crate::widget_style::resolver::ClockSurface::Panel);
        let mut recipe = Recipe::starter(Kind::Clock);
        recipe.parts.push(Part::Settings);
        config.recipes.clock = Some(recipe);
        let size = [520., 220.];
        let ink = crate::widget_style::presentation(&config, config.palette(), true)
            .palette()
            .foreground;
        for theme in [cosmic::Theme::light(), cosmic::Theme::dark()] {
            let mut native = crate::native_test::Native::new(Size::new(size[0], size[1]));
            let bounds = native.bounds(clock::view(&config, now, size), "recipe-settings");
            let pixels = native.render(clock::view(&config, now, size), &theme);
            let matching = (bounds.y.ceil() as usize..(bounds.y + bounds.height).floor() as usize)
                .flat_map(|y| {
                    (bounds.x.ceil() as usize..(bounds.x + bounds.width).floor() as usize)
                        .map(move |x| (y * 520 + x) * 4)
                })
                .filter(|i| {
                    [ink.r, ink.g, ink.b]
                        .into_iter()
                        .enumerate()
                        .all(|(c, expected)| (pixels[*i + c] as f32 - expected * 255.).abs() < 10.)
                })
                .count();
            assert!(
                matching >= 40,
                "{family:?}: expected visible widget ink, found {matching} pixels"
            );
            let face_ink = (8..210)
                .flat_map(|y| (8..270).map(move |x| (y * 520 + x) * 4))
                .filter(|i| {
                    [ink.r, ink.g, ink.b]
                        .into_iter()
                        .enumerate()
                        .all(|(c, expected)| (pixels[*i + c] as f32 - expected * 255.).abs() < 10.)
                })
                .count();
            assert!(
                face_ink >= 40,
                "{family:?}: Clock face must use the same surface ink ({face_ink} pixels)"
            );
            native.operate(
                clock::view(&config, now, size),
                Box::new(cosmic::iced::advanced::widget::operation::focusable::focus(
                    cosmic::widget::Id::new("recipe-settings"),
                )),
            );
            let focused = native.render(clock::view(&config, now, size), &theme);
            assert!(
                pixels
                    .chunks_exact(4)
                    .zip(focused.chunks_exact(4))
                    .filter(|(a, b)| a != b)
                    .count()
                    > 100
            );
            native.capture(
                clock::view(&config, now, size),
                &theme,
                &format!("fb17-action-{family:?}-{}", theme.theme_type.is_dark()),
            );
            let key = keyboard::Key::Named(keyboard::key::Named::Enter);
            let (messages, _) = native.event(
                clock::view(&config, now, size),
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
            assert!(matches!(messages.as_slice(), [Message::ClockSettings]));
        }
    }
}
#[test]
fn fb17_small_recipe_keeps_primary_and_status_budget() {
    for kind in Kind::ALL {
        let mut recipe = Recipe::starter(kind);
        recipe.parts = kind.parts().to_vec();
        for size in [[160., 100.], [320., 220.], [520., 220.]] {
            let frame = plan(&recipe, size, true);
            assert!(frame.parts.contains(&kind.primary()));
            assert!(frame.height <= size[1] - 52.);
            assert!(frame.heights.iter().all(|h| h.is_finite() && *h > 0.));
            if size[0] == 160. {
                assert!(frame.compacted);
                assert_eq!(frame.parts, vec![kind.primary()]);
            }
        }
    }
}
#[test]
fn fb17_recipe_native_missing_data_and_preview_actions_are_safe() {
    use crate::native_test::Native;
    let config = Config::default();
    let mut recipe = Recipe::starter(Kind::Weather);
    recipe.parts.push(Part::Settings);
    let mut native = Native::new(cosmic::iced::Size::new(320., 220.));
    let view = crate::app::settings_recipe_preview(&config, &recipe, true, [320., 220.]);
    native.text_bounds(view, "Choose a city");
}
#[test]
fn fb17_clock_action_input_matches_native_button_and_compact_fallback_releases_it() {
    use crate::native_test::Native;
    let mut config = Config::default();
    let mut recipe = Recipe::starter(Kind::Clock);
    recipe.parts.push(Part::Settings);
    config.recipes.clock = Some(recipe.clone());
    let time = chrono::NaiveDate::from_ymd_opt(2026, 7, 21)
        .unwrap()
        .and_hms_opt(9, 41, 0)
        .unwrap();
    for size in [[320., 220.], [520., 220.]] {
        let mut native = Native::new(cosmic::iced::Size::new(size[0], size[1]));
        let rect = action_rect(&recipe, size).unwrap();
        let bounds = native.bounds(clock::view(&config, time, size), "recipe-settings");
        assert!((bounds.x - rect[0] as f32).abs() <= 1.);
        assert!(
            (bounds.y - rect[1] as f32).abs() <= 1.,
            "{size:?} {bounds:?} {rect:?}"
        );
        assert!((bounds.height - rect[3] as f32).abs() <= 1.);
        let _ = native.event(
            clock::view(&config, time, size),
            cosmic::iced::Event::Mouse(cosmic::iced::mouse::Event::ButtonPressed(
                cosmic::iced::mouse::Button::Left,
            )),
            bounds.center(),
        );
        let (messages, _) = native.event(
            clock::view(&config, time, size),
            cosmic::iced::Event::Mouse(cosmic::iced::mouse::Event::ButtonReleased(
                cosmic::iced::mouse::Button::Left,
            )),
            bounds.center(),
        );
        assert!(matches!(messages.as_slice(), [Message::ClockSettings]));
    }
    recipe.parts = recipe.kind.parts().to_vec();
    assert!(action_rect(&recipe, [160., 100.]).is_none());
}
#[test]
fn fb17_stale_recipe_weather_keeps_age_and_failure_visible_at_compact_sizes() {
    use crate::native_test::Native;
    let mut config = Config::default();
    config.location = Some(crate::config::Location {
        name: "Private city".into(),
        latitude: 0.,
        longitude: 0.,
        timezone: None,
    });
    let mut recipe = Recipe::starter(Kind::Weather);
    recipe.parts = vec![Part::Reading];
    recipe.seal();
    config.recipes.weather = Some(recipe);
    let now = Local::now();
    let weather = crate::app::settings_preview::weather_sample(now.date_naive());
    let old = (now - chrono::Duration::hours(2)).to_rfc3339();
    for size in [[160., 100.], [320., 180.], [520., 220.]] {
        let p = weather::Presentation::new(
            &config,
            weather::Snapshot {
                weather: Some(&weather),
                updated_at: Some(&old),
                failed: true,
            },
            now,
        );
        let mut native = Native::new(cosmic::iced::Size::new(size[0], size[1]));
        let bounds = native.bounds(p.view(Context::Live, size), "recipe-weather-status");
        assert!(bounds.y + bounds.height <= size[1]);
        native.text_bounds(
            p.view(Context::Live, size),
            "Saved · 2h ago\nRefresh failed",
        );
        native.capture(
            p.view(Context::Live, size),
            &cosmic::Theme::dark(),
            &format!("fb17-weather-stale-{}", size[0]),
        );
    }
}
