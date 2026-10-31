use super::*;
#[test]
fn tool_recipe_missing_art_has_no_slot_and_small_cards_keep_controls() {
    for template in 0..3 {
        let recipe = Recipe::template(Kind::Music, template);
        for height in [112., 192., 372.] {
            let plan = parts(&recipe, height, false);
            assert!(!plan.contains(&Part::Artwork));
            assert!(plan.contains(&Part::Track));
            assert!(plan.contains(&Part::Playback));
        }
    }
    let mut config = Config::default();
    config.music.show_artwork = false;
    config.recipes.music = Some(Recipe::template(Kind::Music, 0));
    assert!(config.music_artwork());
    config.recipes.music = Some(Recipe::template(Kind::Music, 2));
    assert!(!config.music_artwork());
    config.recipes.music = None;
    assert!(!config.music_artwork());
}
#[test]
fn tool_recipe_native_samples_all_templates_states_and_finishes() {
    let config = Config::default();
    for kind in [Kind::Music, Kind::Focus] {
        for template in 0..3 {
            let mut recipe = Recipe::template(kind, template);
            recipe.finish = Some(if template == 0 {
                Finish::Tidal
            } else if template == 1 {
                Finish::Paper
            } else {
                Finish::Carbon
            });
            for (wide, size) in [(false, recipe.tool_size()), (true, [520., 220.])] {
                for state in PreviewState::choices(kind) {
                    let mut native =
                        crate::native_test::Native::new(cosmic::iced::Size::new(size[0], size[1]));
                    let view = || sample(&config, &recipe, *state, size);
                    let id = if kind == Kind::Music {
                        "music-play-pause"
                    } else {
                        "focus-toggle"
                    };
                    if !matches!(state, PreviewState::NoPlayer | PreviewState::Unavailable) {
                        let bounds = native.bounds(view(), id);
                        assert!(
                            bounds.x >= 0.
                                && bounds.y >= 0.
                                && bounds.x + bounds.width <= size[0] + 1.
                                && bounds.y + bounds.height <= size[1] + 1.,
                            "{kind:?} {template} {wide} {state:?}: {bounds:?}"
                        );
                    }
                    let pixels = native.render(view(), &cosmic::Theme::dark());
                    assert_eq!(pixels.len(), size[0] as usize * size[1] as usize * 4);
                    {
                        native.capture(
                            view(),
                            &cosmic::Theme::dark(),
                            &format!("tool-{kind:?}-{template}-{wide}-{state:?}"),
                        );
                    }
                }
            }
        }
    }
}

fn enter() -> cosmic::iced::Event {
    use cosmic::iced::keyboard::{self, key};
    cosmic::iced::Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(key::Named::Enter),
        modified_key: keyboard::Key::Named(key::Named::Enter),
        physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::empty(),
        text: None,
        repeat: false,
    })
}
#[test]
fn tool_recipe_live_controls_use_native_keyboard_and_player_capabilities() {
    let config = Config::default();
    let recipe = Recipe::template(Kind::Music, 2);
    let mut player = music::parse_player(
        "org.mpris.MediaPlayer2.fixture",
        &Default::default(),
        &Default::default(),
        Instant::now(),
    );
    player.title = "One track".into();
    player.status = music::Playback::Playing;
    player.can_control = true;
    player.can_pause = true;
    let feed = music::Feed::Players(vec![player.clone()]);
    for allowed in [true, false] {
        player.can_control = allowed;
        let view = || {
            music(
                MusicView {
                    feed: Some(&feed),
                    player: Some(&player),
                    art: None,
                    now: Instant::now(),
                    manually_selected: false,
                    problem: false,
                },
                &config,
                &recipe,
                Some(|a| a),
                Some([360., 180.]),
            )
        };
        let mut native = crate::native_test::Native::new(cosmic::iced::Size::new(360., 180.));
        assert!(native.try_bounds(view(), "music-artwork").is_none());
        native.operate(
            view(),
            Box::new(cosmic::iced::advanced::widget::operation::focusable::focus(
                cosmic::widget::Id::new("music-play-pause"),
            )),
        );
        let (actions, _) = native.event(view(), enter(), cosmic::iced::Point::new(1., 1.));
        if allowed {
            assert!(
                matches!(actions.as_slice(), [Action::Music(music::TransportRequest { command: music::Command::Pause, bus })] if bus == &player.bus)
            );
        } else {
            assert!(actions.is_empty());
        }
    }
    let session = focus::Session::default();
    let recipe = Recipe::template(Kind::Focus, 2);
    let view = || {
        focus(
            &session,
            &config,
            &recipe,
            0,
            "2026-07-21",
            Some(|a| a),
            None,
            false,
        )
    };
    let mut native = crate::native_test::Native::new(cosmic::iced::Size::new(320., 160.));
    native.operate(
        view(),
        Box::new(cosmic::iced::advanced::widget::operation::focusable::focus(
            cosmic::widget::Id::new("focus-toggle"),
        )),
    );
    let (actions, _) = native.event(view(), enter(), cosmic::iced::Point::new(1., 1.));
    assert!(matches!(actions.as_slice(), [Action::FocusToggle]));
    let preview = || sample(&config, &recipe, PreviewState::Active, [320., 160.]);
    native.operate(
        preview(),
        Box::new(cosmic::iced::advanced::widget::operation::focusable::focus(
            cosmic::widget::Id::new("focus-toggle"),
        )),
    );
    assert!(
        native
            .event(preview(), enter(), cosmic::iced::Point::new(1., 1.))
            .0
            .is_empty()
    );
}
#[test]
fn tool_recipe_minimum_sizes_keep_primary_controls_inside_surface() {
    let config = Config::default();
    for kind in [Kind::Music, Kind::Focus] {
        for template in 0..3 {
            let recipe = Recipe::template(kind, template);
            let size = crate::layout::minimum_size(kind.widget());
            for state in PreviewState::choices(kind)
                .iter()
                .filter(|s| !matches!(s, PreviewState::NoPlayer | PreviewState::Unavailable))
            {
                let view = || sample(&config, &recipe, *state, size);
                let mut native =
                    crate::native_test::Native::new(cosmic::iced::Size::new(size[0], size[1]));
                let b = native.bounds(
                    view(),
                    if kind == Kind::Music {
                        "music-play-pause"
                    } else {
                        "focus-toggle"
                    },
                );
                assert!(
                    b.x >= 0.
                        && b.y >= 0.
                        && b.x + b.width <= size[0] + 1.
                        && b.y + b.height <= size[1] + 1.,
                    "{kind:?} {template} {state:?}: {b:?}"
                );
                if *state == PreviewState::NoArtwork {
                    assert!(native.try_bounds(view(), "music-artwork").is_none());
                }
                if *state == PreviewState::Complete {
                    native.capture(
                        view(),
                        &cosmic::Theme::dark(),
                        &format!("tool-min-{kind:?}-{template}"),
                    );
                }
            }
        }
    }
}

#[test]
fn tool_recipe_seek_requests_keep_track_identity_and_unsupported_seeks_are_inert() {
    let config = Config::default();
    let mut recipe = Recipe::template(Kind::Music, 2);
    recipe.finish = Some(Finish::Paper);
    let mut player = music::parse_player(
        "org.mpris.MediaPlayer2.fixture",
        &Default::default(),
        &Default::default(),
        Instant::now(),
    );
    player.can_control = true;
    player.can_seek = true;
    player.can_raise = true;
    player.title = "Selected track".into();
    player.track_id = Some("/tracks/selected".into());
    player.length_us = Some(200_000_000);
    player.position_us = Some(50_000_000);
    let feed = music::Feed::Players(vec![player.clone()]);
    let now = player.sampled;
    for seekable in [true, false] {
        player.can_seek = seekable;
        let view = || {
            music(
                MusicView {
                    feed: Some(&feed),
                    player: Some(&player),
                    art: None,
                    now,
                    manually_selected: false,
                    problem: false,
                },
                &config,
                &recipe,
                Some(|a| a),
                None,
            )
        };
        let mut native = crate::native_test::Native::new(cosmic::iced::Size::new(360., 200.));
        let bounds = native.bounds(view(), "music-progress");
        let point = cosmic::iced::Point::new(bounds.x + bounds.width * 0.75, bounds.center_y());
        let (actions, _) = native.event(
            view(),
            cosmic::iced::Event::Mouse(cosmic::iced::mouse::Event::ButtonPressed(
                cosmic::iced::mouse::Button::Left,
            )),
            point,
        );
        if seekable {
            assert!(
                matches!(actions.as_slice(),[Action::MusicSeek(request)] if request.bus == player.bus && request.track_id == "/tracks/selected" && request.position_us > 100_000_000 && request.position_us < 200_000_000)
            );
        } else {
            assert!(actions.is_empty());
        }
        native.capture(
            view(),
            &cosmic::Theme::dark(),
            &format!("tool-live-music-seeking-{seekable}"),
        );
        native.operate(
            view(),
            Box::new(cosmic::iced::advanced::widget::operation::focusable::focus(
                cosmic::widget::Id::new("music-open-player"),
            )),
        );
        assert!(matches!(
            native
                .event(view(), enter(), cosmic::iced::Point::new(1., 1.))
                .0
                .as_slice(),
            [Action::Music(music::TransportRequest {
                command: music::Command::Raise,
                ..
            })]
        ));
    }
}
