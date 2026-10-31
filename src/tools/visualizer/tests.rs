use super::*;
#[test]
fn visualizer_defaults_are_opt_in_and_legacy_serialization_stays_unchanged() {
    let config: crate::config::MusicPreferences = serde_json::from_str("{}").unwrap();
    assert!(!config.visualizer.active());
    assert!(
        serde_json::to_value(config)
            .unwrap()
            .get("visualizer")
            .is_none()
    );
    let mut config = crate::config::Config::default();
    config.music.visualizer.circle = Circle::Bars;
    assert!(!config.music_artwork());
    let mut p = Preferences::default();
    p.spectrum = true;
    p.opacity = 0;
    assert!(!p.active());
    p.circle = Circle::Bars;
    assert!(p.active());
    p.width = 0;
    assert!(p.validate().is_err());
    assert!(serde_json::from_str::<Preferences>(r#"{"circle":"wave"}"#).is_err());
}
#[test]
fn visualizer_geometry_reaches_edges_and_stays_inside_portrait_and_scaled_outputs() {
    for size in [[1920., 1080.], [853.3, 533.3], [1080., 1920.]] {
        let p = Preferences::default();
        let r = p.rect(size);
        assert_eq!(r.width, size[0]);
        assert!((r.y + r.height - size[1]).abs() < 0.01);
        for x in [0, 50, 100] {
            for y in [0, 50, 100] {
                let mut p = p.clone();
                p.width = 35;
                p.height = 22;
                p.x = x;
                p.y = y;
                let r = p.rect(size);
                assert!(
                    r.x >= 0.
                        && r.y >= 0.
                        && r.x + r.width <= size[0] + 0.01
                        && r.y + r.height <= size[1] + 0.01
                );
            }
        }
    }
}
#[test]
fn visualizer_fft_finds_tone_rejects_silence_and_decays_without_nan() {
    let mut bytes = [0; N * 2];
    for i in 0..N {
        let n = ((std::f32::consts::TAU * 1000. * i as f32 / RATE).sin() * 15000.) as i16;
        bytes[i * 2..i * 2 + 2].copy_from_slice(&n.to_le_bytes());
    }
    let mut prev = [0.; BINS];
    let bins = analyze(&bytes, &mut prev);
    let peak = bins
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .unwrap()
        .0;
    let frequency = 40. * 250f32.powf((peak as f32 + 0.5) / BINS as f32);
    assert!((850.0..1150.).contains(&frequency), "{frequency}");
    for _ in 0..40 {
        analyze(&[0; N * 2], &mut prev);
    }
    assert_eq!(prev, [0.; BINS]);
    assert!(ARGS.contains(&"--device=@DEFAULT_MONITOR@"));
}

#[test]
fn visualizer_native_opacity_and_styles_render_and_controls_fit() {
    use crate::tools::{cards, music};
    use cosmic::iced::{Color, Length, Size};
    let now = std::time::Instant::now();
    let props = std::collections::HashMap::new();
    let mut player = music::parse_player("org.mpris.MediaPlayer2.fixture", &props, &props, now);
    player.title = "Golden Hour".into();
    player.artists = vec!["Aura · Solstice".into()];
    player.status = music::Playback::Playing;
    player.can_play = true;
    player.can_pause = true;
    player.can_next = true;
    player.can_previous = true;
    let feed = music::Feed::Players(vec![player.clone()]);
    let bins = std::array::from_fn(|i| 0.2 + ((i as f32 * 0.37).sin().abs() * 0.65));
    for circle in [Circle::Bars, Circle::Dots, Circle::Rings] {
        for size in [[300., 300.], [520., 220.], [240., 118.]] {
            let mut config = crate::config::Config::default();
            config.music.visualizer.circle = circle;
            let mut native = crate::native_test::Native::new(Size::new(size[0], size[1]));
            let view = || {
                cards::music_visualized(
                    cards::MusicView {
                        feed: Some(&feed),
                        player: Some(&player),
                        art: None,
                        now,
                        manually_selected: false,
                        problem: false,
                    },
                    &config,
                    Some(|a| a),
                    Some(size),
                    bins,
                )
            };
            let r = native.bounds(view(), "music-play-pause");
            assert!(
                r.x >= 0.
                    && r.y >= 0.
                    && r.x + r.width <= size[0] + 0.1
                    && r.y + r.height <= size[1] + 0.1,
                "{circle:?} {size:?} {r:?}"
            );
            let (mut messages, _) = native.event(
                view(),
                cosmic::iced::Event::Mouse(cosmic::iced::mouse::Event::ButtonPressed(
                    cosmic::iced::mouse::Button::Left,
                )),
                r.center(),
            );
            messages.extend(
                native
                    .event(
                        view(),
                        cosmic::iced::Event::Mouse(cosmic::iced::mouse::Event::ButtonReleased(
                            cosmic::iced::mouse::Button::Left,
                        )),
                        r.center(),
                    )
                    .0,
            );
            assert!(
                messages.iter().any(
                    |m| matches!(m,cards::Action::Music(r) if r.command==music::Command::Pause)
                ),
                "{circle:?} {size:?} {messages:?}"
            );
            native.capture(
                view(),
                &cosmic::Theme::dark(),
                &format!("visualizer-{circle:?}-{}x{}", size[0], size[1]),
            );
        }
    }
    let mut sums = Vec::new();
    for opacity in [0, 30, 80, 100] {
        let p = Preferences {
            spectrum: true,
            opacity,
            ..Default::default()
        };
        let mut native = crate::native_test::Native::new(Size::new(800., 450.));
        let view = || -> cosmic::Element<'static, ()> {
            cosmic::widget::canvas(drawing::Visual {
                bins,
                circle: Circle::Off,
                color: Color::from_rgba(0.7, 0.6, 1., opacity as f32 / 100.),
                region: Some(p.clone()),
                backdrop: None,
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        };
        let pixels = native.render(view(), &cosmic::Theme::dark());
        sums.push(pixels.chunks_exact(4).map(|p| u64::from(p[3])).sum::<u64>());
        native.capture(
            view(),
            &cosmic::Theme::dark(),
            &format!("spectrum-opacity-{opacity}"),
        );
    }
    assert_eq!(sums[0], 0);
    assert!(sums.windows(2).all(|p| p[1] > p[0]), "{sums:?}");
}

#[tokio::test]
#[ignore = "requires an explicitly isolated PipeWire/Pulse test server"]
async fn visualizer_private_audio_monitor_and_cancellation() {
    let server = std::env::var("HALCYON_AUDIO_TEST_SERVER").expect("private server required");
    assert!(server.contains("halcyon-audio-test-"));
    assert_eq!(std::env::var("PULSE_SERVER").unwrap(), server);
    let mut child = command().spawn().unwrap();
    let pid = child.id().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut bytes = [0; N * 2];
    let mut previous = [0.; BINS];
    let mut peak = 0.;
    for _ in 0..20 {
        tokio::time::timeout(Duration::from_secs(5), stdout.read_exact(&mut bytes))
            .await
            .unwrap()
            .unwrap();
        peak = analyze(&bytes, &mut previous)
            .into_iter()
            .fold(peak, f32::max);
    }
    assert!(
        peak > 0.5,
        "private output tone must reach analyzer: {peak}"
    );
    drop(stdout);
    drop(child);
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if unsafe { libc::kill(pid as i32, 0) } != 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("capture child reaped on cancellation");
}
