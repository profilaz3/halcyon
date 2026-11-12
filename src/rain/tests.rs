use super::*;
use cosmic::iced::{Event, Length, Rectangle, Size, mouse, window};
#[test]
fn rain_gpu_modes_render_distinct_native_pixels() {
    if std::env::var_os("HALCYON_RAIN_GPU_TEST").is_none() {
        return;
    }
    let pixels = image::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/looks/evening-wallpaper.jpg"
    ))
    .unwrap()
    .resize_exact(1280, 720, image::imageops::FilterType::Triangle)
    .to_rgba8();
    let scene = gpu::Scene::new(crate::desktop::wallpaper::media::Pixels {
        width: 1280,
        height: 720,
        rgba: pixels.into_raw(),
    });
    let mut native = crate::native_test::Native::with_backend(Size::new(1280., 720.), "wgpu");
    println!("Rain renderer: {}", native.renderer_name());
    let theme = cosmic::Theme::dark();
    let mut images = Vec::new();
    let dry = || -> cosmic::Element<'_, ()> {
        cosmic::iced::widget::Shader::new(gpu::Program {
            scene: scene.clone(),
            preferences: Preferences {
                enabled: true,
                intensity: 0,
                mist: 0,
                ..Default::default()
            },
            running: false,
            fixed_time: Some(0.),
            fade: Default::default(),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    };
    let expected = &scene.pixels.rgba;
    let actual = native.render(dry(), &theme);
    let mean = expected
        .iter()
        .zip(&actual)
        .map(|(a, b)| f64::from(a.abs_diff(*b)))
        .sum::<f64>()
        / actual.len() as f64;
    assert!(
        mean < 0.6,
        "dry rain must preserve native wallpaper brightness: mean channel error {mean}"
    );
    for mode in [Mode::Procedural, Mode::Hybrid] {
        for time in [0., 6.] {
            let p = Preferences {
                enabled: true,
                mode,
                intensity: 70,
                ..Default::default()
            };
            let view = || -> cosmic::Element<'_, ()> {
                cosmic::iced::widget::Shader::new(gpu::Program {
                    scene: scene.clone(),
                    preferences: p.clone(),
                    running: false,
                    fixed_time: Some(time),
                    fade: Default::default(),
                })
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            };
            let bytes = native.render(view(), &theme);
            assert!(
                bytes.chunks_exact(4).any(|p| p[0] > 30 && p[1] > 30),
                "rain must render its real wallpaper"
            );
            native.capture(view(), &theme, &format!("rain-{mode:?}-{time}"));
            images.push(bytes);
        }
    }
    assert_ne!(images[0], images[1], "procedural drops move");
    assert_ne!(images[2], images[3], "hybrid drops move and merge");
    assert_ne!(images[0], images[2], "two actual rendering paths");
    for mode in [Mode::Procedural, Mode::Hybrid] {
        let mut samples = Vec::new();
        for (name, intensity, trails) in [("sparse", 15, 0), ("dense", 85, 0), ("trails", 85, 100)]
        {
            let view = || -> cosmic::Element<'_, ()> {
                cosmic::iced::widget::Shader::new(gpu::Program {
                    scene: scene.clone(),
                    preferences: Preferences {
                        enabled: true,
                        mode,
                        intensity,
                        trails,
                        ..Default::default()
                    },
                    running: false,
                    fixed_time: Some(6.),
                    fade: Default::default(),
                })
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            };
            samples.push(native.render(view(), &theme));
            native.capture(view(), &theme, &format!("rain-{mode:?}-{name}"));
        }
        assert_ne!(
            samples[0], samples[1],
            "{mode:?}: density changes the actual GPU image"
        );
        assert_ne!(
            samples[1], samples[2],
            "{mode:?}: trails are independently adjustable"
        );
    }
}

#[test]
fn rain_widget_does_not_advance_or_request_frames_when_paused() {
    use cosmic::iced::widget::shader::Program as _;
    let scene = gpu::Scene::new(crate::desktop::wallpaper::media::Pixels {
        width: 1,
        height: 1,
        rgba: vec![0, 0, 0, 255],
    });
    let mut program = gpu::Program {
        scene,
        preferences: Preferences::default(),
        running: false,
        fixed_time: None,
        fade: Default::default(),
    };
    let mut state = gpu::State::default();
    let event = cosmic::iced::Event::Window(cosmic::iced::window::Event::RedrawRequested(
        std::time::Instant::now(),
    ));
    let bounds = cosmic::iced::Rectangle {
        width: 1920.,
        height: 1080.,
        ..Default::default()
    };
    let stopped: Option<cosmic::iced::widget::shader::Action<()>> = program.update(
        &mut state,
        &event,
        bounds,
        cosmic::iced::mouse::Cursor::Unavailable,
    );
    assert!(stopped.is_none());
    program.running = true;
    let running: Option<cosmic::iced::widget::shader::Action<()>> = program.update(
        &mut state,
        &event,
        bounds,
        cosmic::iced::mouse::Cursor::Unavailable,
    );
    assert!(running.is_some());
}

#[test]
fn rain_density_changes_update_existing_live_widget_without_waiting_for_expiry() {
    use cosmic::iced::widget::shader::Program as _;
    let mut program = gpu::Program {
        scene: gpu::Scene::new(crate::desktop::wallpaper::media::Pixels {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 255],
        }),
        preferences: Preferences {
            mode: Mode::Hybrid,
            intensity: 100,
            ..Default::default()
        },
        running: true,
        fixed_time: None,
        fade: Default::default(),
    };
    let mut state = gpu::State::default();
    let event = cosmic::iced::Event::Window(cosmic::iced::window::Event::RedrawRequested(
        std::time::Instant::now(),
    ));
    let bounds = cosmic::iced::Rectangle {
        width: 1920.,
        height: 1080.,
        ..Default::default()
    };
    let _: Option<cosmic::iced::widget::shader::Action<()>> = program.update(
        &mut state,
        &event,
        bounds,
        cosmic::iced::mouse::Cursor::Unavailable,
    );
    let dense = <gpu::Program as cosmic::iced::widget::shader::Program<()>>::draw(
        &program,
        &state,
        cosmic::iced::mouse::Cursor::Unavailable,
        bounds,
    )
    .drops
    .len();
    program.preferences.intensity = 10;
    let _: Option<cosmic::iced::widget::shader::Action<()>> = program.update(
        &mut state,
        &event,
        bounds,
        cosmic::iced::mouse::Cursor::Unavailable,
    );
    let sparse = <gpu::Program as cosmic::iced::widget::shader::Program<()>>::draw(
        &program,
        &state,
        cosmic::iced::mouse::Cursor::Unavailable,
        bounds,
    )
    .drops
    .len();
    assert!(dense > sparse * 2);
    program.preferences.intensity = 0;
    assert!(
        <gpu::Program as cosmic::iced::widget::shader::Program<()>>::draw(
            &program,
            &state,
            cosmic::iced::mouse::Cursor::Unavailable,
            bounds
        )
        .drops
        .is_empty()
    );
}

#[test]
fn rain_save_fences_concurrent_edits_and_preserves_other_preferences() {
    if crate::displays::test_support::isolated(
        "rain::tests::rain_save_fences_concurrent_edits_and_preserves_other_preferences",
    ) {
        return;
    }
    use crate::config::Config;
    let old = Config::load().rain;
    let mut a = old.clone();
    a.enabled = true;
    Config::save_changes(serde_json::json!({"future_field":42})).unwrap();
    Config::save_rain(&old, &a).unwrap();
    let mut b = old.clone();
    b.intensity = 99;
    assert!(Config::save_rain(&old, &b).is_err());
    assert_eq!(Config::load().rain, a);
    let raw: serde_json::Value =
        serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap();
    assert_eq!(raw["future_field"], 42);
}
#[test]
fn rain_design_and_setup_round_trip_require_new_schema_only_when_present() {
    let rain = serde_json::to_value(Preferences {
        enabled: true,
        mode: Mode::Hybrid,
        ..Default::default()
    })
    .unwrap();
    let raw = serde_json::json!({"rain":rain});
    let doc = crate::design::Document::from_legacy(&raw).unwrap();
    assert_eq!(doc.version, 5);
    assert!(doc.validate().is_ok());
    let mut old = doc.clone();
    old.version = 4;
    assert!(old.validate().is_err());
    use crate::desktop::profiles::{Profile, export, import};
    let mut profile = Profile::new("Rain fixture").unwrap();
    profile.widget_style = Some(raw);
    let bytes = export(&profile).unwrap().bytes;
    let imported = import(&bytes).unwrap();
    assert_eq!(imported.version, 19);
    assert_eq!(imported.widget_style, profile.widget_style);
    let mut old: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    old["version"] = 18.into();
    assert!(import(&serde_json::to_vec(&old).unwrap()).is_err());
    profile.widget_style = None;
    assert!(profile.written_version() < 19);
}

#[test]
fn atmosphere_preferences_retain_each_effect_and_share_pause_policy() {
    let old: Preferences =
        serde_json::from_str(r#"{"enabled":true,"intensity":17,"size":60}"#).unwrap();
    assert_eq!(old.effect, Effect::Rain);
    assert_eq!(old.size, 60);
    assert!(
        !serde_json::to_value(&old)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("snow")
    );
    let mut p = old;
    p.snow.size = 24;
    p.snow.density = 80;
    p.fog.height = 31;
    p.clouds.coverage = 64;
    for effect in [Effect::Rain, Effect::Snow, Effect::Fog, Effect::Clouds] {
        p.effect = effect;
        let copy: Preferences = serde_json::from_value(serde_json::to_value(&p).unwrap()).unwrap();
        assert_eq!(copy, p);
        let ready = lifecycle::State {
            ready: true,
            ..Default::default()
        };
        assert!(p.active(true, ready));
        assert!(!p.active(false, ready));
        assert!(!p.active(
            true,
            lifecycle::State {
                locked: true,
                ..ready
            }
        ));
        assert!(!p.active(
            true,
            lifecycle::State {
                sleeping: true,
                ..ready
            }
        ));
        assert!(!p.active(
            true,
            lifecycle::State {
                on_battery: true,
                ..ready
            }
        ));
        assert!(!p.active(true, lifecycle::State::default()));
    }
    p.effect = Effect::Snow;
    p.intensity = 0;
    assert!(
        p.has_visible_effect(),
        "a rain control must not suppress snow"
    );
    p.snow.density = 0;
    assert!(!p.has_visible_effect());
    p.effect = Effect::Fog;
    p.fog.opacity = 0;
    assert!(!p.has_visible_effect());
    p.effect = Effect::Clouds;
    p.clouds.strength = 0;
    assert!(!p.has_visible_effect());
    p.snow.wind = 101;
    assert!(p.validate().is_err());
    p.snow.wind = 0;
    p.fog.height = 101;
    assert!(p.validate().is_err());
    p.fog.height = 50;
    p.clouds.softness = 101;
    assert!(p.validate().is_err());
}

#[test]
fn atmosphere_gpu_render_controls_motion_and_zero_effect() {
    if std::env::var_os("HALCYON_RAIN_GPU_TEST").is_none() {
        return;
    }
    let pixels = image::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/looks/evening-wallpaper.jpg"
    ))
    .unwrap()
    .resize_exact(1280, 720, image::imageops::FilterType::Triangle)
    .to_rgba8();
    let scene = gpu::Scene::new(crate::desktop::wallpaper::media::Pixels {
        width: 1280,
        height: 720,
        rgba: pixels.into_raw(),
    });
    let mut native = crate::native_test::Native::with_backend(Size::new(1280., 720.), "wgpu");
    let theme = cosmic::Theme::dark();
    for effect in [Effect::Snow, Effect::Fog, Effect::Clouds] {
        let mut p = Preferences {
            enabled: true,
            effect,
            ..Default::default()
        };
        let view = |p: Preferences, time| -> cosmic::Element<'_, ()> {
            cosmic::iced::widget::Shader::new(gpu::Program {
                scene: scene.clone(),
                preferences: p,
                running: false,
                fixed_time: Some(time),
                fade: Default::default(),
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        };
        let initial = native.render(view(p.clone(), 0.), &theme);
        let later = native.render(view(p.clone(), 6.), &theme);
        assert_ne!(initial, later, "{effect:?} must animate");
        native.capture(view(p.clone(), 6.), &theme, &format!("effect-{effect:?}"));
        let mut still = p.clone();
        match effect {
            Effect::Snow => still.snow.speed = 0,
            Effect::Fog => still.fog.speed = 0,
            Effect::Clouds => still.clouds.speed = 0,
            _ => unreachable!(),
        }
        assert_eq!(
            native.render(view(still.clone(), 0.), &theme),
            native.render(view(still, 60.), &theme),
            "{effect:?}: zero speed must be still"
        );
        let mut variants = Vec::new();
        match effect {
            Effect::Snow => {
                let mut v = p.clone();
                v.snow.size = 90;
                variants.push(v);
                let mut v = p.clone();
                v.snow.wind = -90;
                variants.push(v);
                let mut v = p.clone();
                v.snow.opacity = 20;
                variants.push(v);
            }
            Effect::Fog => {
                let mut v = p.clone();
                v.fog.height = 90;
                variants.push(v);
                let mut v = p.clone();
                v.fog.color = FogColor::Warm;
                variants.push(v);
                let mut v = p.clone();
                v.fog.wind = -90;
                variants.push(v);
            }
            Effect::Clouds => {
                let mut v = p.clone();
                v.clouds.softness = 0;
                variants.push(v);
                let mut v = p.clone();
                v.clouds.strength = 90;
                variants.push(v);
                let mut v = p.clone();
                v.clouds.wind = -90;
                variants.push(v);
            }
            _ => unreachable!(),
        }
        for (i, v) in variants.into_iter().enumerate() {
            assert_ne!(
                later,
                native.render(view(v.clone(), 6.), &theme),
                "{effect:?} control {i} must affect rendering"
            );
            native.capture(
                view(v, 6.),
                &theme,
                &format!("effect-{effect:?}-variation-{i}"),
            );
        }
        match effect {
            Effect::Snow => p.snow.density = 0,
            Effect::Fog => p.fog.opacity = 0,
            Effect::Clouds => p.clouds.coverage = 0,
            _ => unreachable!(),
        }
        let off = native.render(view(p.clone(), 6.), &theme);
        let error = scene
            .pixels
            .rgba
            .iter()
            .zip(&off)
            .map(|(a, b)| f64::from(a.abs_diff(*b)))
            .sum::<f64>()
            / off.len() as f64;
        assert!(
            error < 0.6,
            "{effect:?} at zero must preserve wallpaper: {error}"
        );
        assert_ne!(later, off);
        p.quality = Quality::Eco;
        native.render(view(p, 6.), &theme);
    }
}

#[test]
fn atmosphere_zero_speed_does_not_request_animation_frames() {
    use cosmic::iced::widget::shader::Program as _;
    for effect in [Effect::Snow, Effect::Fog, Effect::Clouds] {
        let mut preferences = Preferences {
            effect,
            ..Default::default()
        };
        preferences.snow.speed = 0;
        preferences.fog.speed = 0;
        preferences.clouds.speed = 0;
        let p = gpu::Program {
            scene: gpu::Scene::new(crate::desktop::wallpaper::media::Pixels {
                width: 1,
                height: 1,
                rgba: vec![0, 0, 0, 255],
            }),
            preferences,
            running: true,
            fixed_time: None,
            fade: Default::default(),
        };
        let mut state = gpu::State::default();
        let event = cosmic::iced::Event::Window(cosmic::iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        ));
        let action: Option<cosmic::iced::widget::shader::Action<()>> = p.update(
            &mut state,
            &event,
            cosmic::iced::Rectangle {
                width: 1280.,
                height: 720.,
                ..Default::default()
            },
            cosmic::iced::mouse::Cursor::Unavailable,
        );
        assert!(action.is_none(), "{effect:?} is static");
    }
}

#[test]
fn atmosphere_setups_preserve_all_effects_and_independent_controls() {
    use crate::desktop::profiles::{Profile, export, import};
    for effect in [Effect::Rain, Effect::Snow, Effect::Fog, Effect::Clouds] {
        let mut p = Preferences {
            enabled: true,
            effect,
            ..Default::default()
        };
        p.snow.size = 27;
        p.fog.color = FogColor::Warm;
        p.clouds.strength = 36;
        let raw = serde_json::json!({"rain":p});
        assert!(
            crate::design::Document::from_legacy(&raw)
                .unwrap()
                .validate()
                .is_ok()
        );
        let mut profile = Profile::new("Atmosphere fixture").unwrap();
        profile.widget_style = Some(raw);
        let imported = import(&export(&profile).unwrap().bytes).unwrap();
        assert_eq!(imported.widget_style, profile.widget_style);
        assert_eq!(imported.version, 19);
    }
}

#[test]
fn effect_quality_caps_simulation_cadence_during_unrelated_redraws() {
    use cosmic::iced::widget::shader::Program as _;
    let p = gpu::Program {
        scene: gpu::Scene::new(crate::desktop::wallpaper::media::Pixels {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 255],
        }),
        preferences: Preferences::default(),
        running: true,
        fixed_time: None,
        fade: Default::default(),
    };
    let mut state = gpu::State::default();
    let start = std::time::Instant::now();
    let bounds = Rectangle {
        x: 0.,
        y: 0.,
        width: 640.,
        height: 360.,
    };
    let mut times = Vec::new();
    for ms in [0, 5, 10, 20, 34, 40] {
        let event = Event::Window(window::Event::RedrawRequested(
            start + std::time::Duration::from_millis(ms),
        ));
        let _: Option<cosmic::iced::widget::shader::Action<()>> =
            p.update(&mut state, &event, bounds, mouse::Cursor::Unavailable);
        times.push(
            <gpu::Program as cosmic::iced::widget::shader::Program<()>>::draw(
                &p,
                &state,
                mouse::Cursor::Unavailable,
                bounds,
            )
            .time,
        );
    }
    assert_eq!(&times[..4], &[0.; 4]);
    assert!(times[4] > 0.03);
    assert_eq!(times[4], times[5]);
}

/// A desktop surface's program, stepped frame by frame.
struct Fading {
    program: gpu::Program,
    state: gpu::State,
    now: std::time::Instant,
}
impl Fading {
    fn new(preferences: Preferences) -> Self {
        Self {
            program: gpu::Program {
                scene: gpu::Scene::new(crate::desktop::wallpaper::media::Pixels {
                    width: 1,
                    height: 1,
                    rgba: vec![0, 0, 0, 255],
                }),
                preferences,
                running: true,
                fixed_time: None,
                fade: gpu::Fade {
                    enabled: true,
                    ..Default::default()
                },
            },
            state: gpu::State::default(),
            now: std::time::Instant::now(),
        }
    }
    const BOUNDS: Rectangle = Rectangle {
        x: 0.,
        y: 0.,
        width: 640.,
        height: 360.,
    };
    /// One frame 34 ms after the last; true if another frame was asked for.
    fn frame(&mut self) -> bool {
        use cosmic::iced::widget::shader::Program as _;
        self.now += std::time::Duration::from_millis(34);
        let event = Event::Window(window::Event::RedrawRequested(self.now));
        let action: Option<cosmic::iced::widget::shader::Action<()>> = self.program.update(
            &mut self.state,
            &event,
            Self::BOUNDS,
            mouse::Cursor::Unavailable,
        );
        action.is_some()
    }
    fn shown(&self) -> gpu::Primitive {
        <gpu::Program as cosmic::iced::widget::shader::Program<()>>::draw(
            &self.program,
            &self.state,
            mouse::Cursor::Unavailable,
            Self::BOUNDS,
        )
    }
    /// Frames until the fade settles, checking that no frame jumps and that
    /// a new scene always starts from nothing.
    fn settle(&mut self) -> Vec<(Effect, f32)> {
        let mut seen = vec![(self.shown().preferences.effect, self.shown().fade)];
        for _ in 0..200 {
            let more = self.frame();
            let now = (self.shown().preferences.effect, self.shown().fade);
            let (effect, fade) = *seen.last().unwrap();
            assert!((now.1 - fade).abs() <= 0.03, "{seen:?} then {now:?}");
            assert!(effect == now.0 || now.1 == 0., "{seen:?} then {now:?}");
            seen.push(now);
            if !more || (now.1 == 1. && self.program.preferences.animates()) {
                return seen;
            }
        }
        panic!("the fade never settled: {seen:?}");
    }
}

#[test]
fn desktop_effects_fade_in_change_and_fade_out_without_popping() {
    let snow = Preferences {
        enabled: true,
        effect: Effect::Snow,
        ..Default::default()
    };
    let mut f = Fading::new(snow.clone());
    // A new surface starts as the plain wallpaper and fades in over FADE.
    assert_eq!(f.shown().fade, 0.);
    let seen = f.settle();
    assert_eq!(seen.last().unwrap(), &(Effect::Snow, 1.));
    let frames = seen.len() as f32 * 0.034;
    assert!((1.4..1.8).contains(&frames), "{frames} s");
    // A different effect: snow fades out, then rain fades in.
    f.program.preferences = Preferences {
        effect: Effect::Rain,
        ..snow.clone()
    };
    assert_eq!(f.shown().preferences.effect, Effect::Snow);
    assert_eq!(f.shown().fade, 1.);
    let seen = f.settle();
    let switch = seen.iter().position(|(e, _)| *e == Effect::Rain).unwrap();
    assert!(
        seen[switch - 1].1 <= 0.03 && seen[switch].1 == 0.,
        "{seen:?}"
    );
    assert!(
        seen[..switch].windows(2).all(|w| w[1].1 < w[0].1),
        "{seen:?}"
    );
    assert_eq!(seen.last().unwrap(), &(Effect::Rain, 1.));
    // A heavier sky with the same effect fades too.
    f.program.fade.level = Some(weather::Level::Heavy);
    let seen = f.settle();
    assert!(seen.iter().any(|(_, fade)| *fade == 0.), "{seen:?}");
    assert_eq!(seen.last().unwrap(), &(Effect::Rain, 1.));
    // Other edits within the same scene show at once.
    f.program.preferences.intensity = 90;
    assert_eq!(f.shown().preferences.intensity, 90);
    assert_eq!(f.shown().fade, 1.);
    // A retiring surface fades out and then stops asking for frames.
    f.program.fade.hidden = true;
    let seen = f.settle();
    assert_eq!(seen.last().unwrap().1, 0.);
    assert!(!f.frame(), "nothing on screen asks for no frames");
}

#[test]
fn frozen_effects_finish_their_fade_then_stop_drawing() {
    let mut frozen = Preferences {
        enabled: true,
        effect: Effect::Fog,
        ..Default::default()
    };
    frozen.fog.speed = 0;
    let mut f = Fading::new(frozen);
    let seen = f.settle();
    assert_eq!(seen.last().unwrap(), &(Effect::Fog, 1.));
    assert!(!f.frame(), "a finished fade of a frozen effect stops");
    let time = f.shown().time;
    f.frame();
    assert_eq!(f.shown().time, time, "a frozen field never advances");
}

#[test]
fn settings_previews_captures_and_paused_surfaces_never_fade() {
    let rain = Preferences {
        enabled: true,
        ..Default::default()
    };
    // The Settings preview: edits show at once.
    let mut f = Fading::new(rain.clone());
    f.program.fade.enabled = false;
    assert_eq!(f.shown().fade, 1.);
    f.program.preferences.effect = Effect::Clouds;
    assert_eq!(f.shown().preferences.effect, Effect::Clouds);
    assert_eq!(f.shown().fade, 1.);
    // Fixed-time captures and a stopped program show the scene as it is.
    let mut f = Fading::new(rain.clone());
    f.program.fixed_time = Some(2.);
    f.frame();
    assert_eq!(f.shown().fade, 1.);
    let mut f = Fading::new(rain);
    f.program.running = false;
    assert!(!f.frame());
    assert_eq!(f.shown().fade, 1.);
}

/// Before/after evidence: rain at a few moments and consecutive snow frames
/// with a temporal flicker measure. Writes only `HALCYON_RAIN_EVIDENCE`.
#[test]
#[ignore = "explicit native GPU captures; writes only the evidence directory"]
fn rain_and_snow_visual_evidence() {
    let root = std::path::PathBuf::from(
        std::env::var_os("HALCYON_RAIN_EVIDENCE").expect("private evidence directory"),
    );
    std::fs::create_dir_all(&root).unwrap();
    let (width, height) = (1920u32, 1080u32);
    let pixels = image::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/looks/evening-wallpaper.jpg"
    ))
    .unwrap()
    .resize_exact(width, height, image::imageops::FilterType::Triangle)
    .to_rgba8();
    let scene = gpu::Scene::new(crate::desktop::wallpaper::media::Pixels {
        width,
        height,
        rgba: pixels.into_raw(),
    });
    let mut native =
        crate::native_test::Native::with_backend(Size::new(width as f32, height as f32), "wgpu");
    let theme = cosmic::Theme::dark();
    let save = |name: &str, bytes: &[u8]| {
        image::save_buffer(
            root.join(format!("{name}.png")),
            bytes,
            width,
            height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    };
    let view = |p: Preferences, time: f32| -> cosmic::Element<'_, ()> {
        cosmic::iced::widget::Shader::new(gpu::Program {
            scene: scene.clone(),
            preferences: p,
            running: false,
            fixed_time: Some(time),
            fade: Default::default(),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    };
    for mode in [Mode::Procedural, Mode::Hybrid] {
        for time in [3., 9., 20.] {
            let p = Preferences {
                enabled: true,
                mode,
                ..Default::default()
            };
            let bytes = native.render(view(p, time), &theme);
            save(&format!("rain-{mode:?}-{time}s"), &bytes);
        }
    }
    let mut report = Vec::new();
    for (name, size, quality) in [
        ("default", 30u8, Quality::Balanced),
        ("small", 0, Quality::Balanced),
        ("small-eco", 0, Quality::Eco),
    ] {
        let mut p = Preferences {
            enabled: true,
            effect: Effect::Snow,
            quality,
            ..Default::default()
        };
        p.snow.size = size;
        let frames: Vec<Vec<u8>> = (0..12)
            .map(|i| native.render(view(p.clone(), 4. + i as f32 / 30.), &theme))
            .collect();
        for (i, frame) in frames.iter().enumerate().take(4) {
            save(&format!("snow-{name}-frame{i}"), frame);
        }
        let luma = |f: &[u8], i: usize| {
            0.2126 * f64::from(f[i * 4])
                + 0.7152 * f64::from(f[i * 4 + 1])
                + 0.0722 * f64::from(f[i * 4 + 2])
        };
        let count = (width * height) as usize;
        let base = &scene.pixels.rgba;
        // Snow energy per frame: total brightening over the wallpaper. Smooth
        // motion keeps it steady; aliasing makes flakes pop in and out.
        let energy: Vec<f64> = frames
            .iter()
            .map(|f| {
                (0..count)
                    .map(|i| (luma(f, i) - luma(base, i)).max(0.))
                    .sum()
            })
            .collect();
        let mean = energy.iter().sum::<f64>() / energy.len() as f64;
        let deviation =
            (energy.iter().map(|e| (e - mean).powi(2)).sum::<f64>() / energy.len() as f64).sqrt();
        // Per-pixel temporal second difference: zero for linear motion of a
        // well filtered image, large where a pixel blinks.
        let mut second = 0.;
        let mut blinks = 0u64;
        for w in frames.windows(3) {
            for i in 0..count {
                let d = (luma(&w[2], i) - 2. * luma(&w[1], i) + luma(&w[0], i)).abs();
                second += d;
                if d > 48. {
                    blinks += 1;
                }
            }
        }
        let windows = (frames.len() - 2) as f64;
        // Tile flicker: mean change of 32 x 32 pixel tile brightness between
        // frames. A moving flake keeps its tile steady; a popping one does not.
        let tiles = |f: &[u8]| {
            let (columns, rows) = (width as usize / 32, height as usize / 32);
            let mut sums = vec![0.; columns * rows];
            for y in 0..rows * 32 {
                for x in 0..columns * 32 {
                    sums[(y / 32) * columns + x / 32] += luma(f, y * width as usize + x) / 1024.;
                }
            }
            sums
        };
        let tiled: Vec<Vec<f64>> = frames.iter().map(|f| tiles(f)).collect();
        let tile_change = tiled
            .windows(2)
            .map(|w| {
                w[0].iter()
                    .zip(&w[1])
                    .map(|(a, b)| (a - b).abs())
                    .sum::<f64>()
                    / w[0].len() as f64
            })
            .sum::<f64>()
            / (tiled.len() - 1) as f64;
        let mut difference = vec![0u8; count * 4];
        for i in 0..count {
            let d = ((luma(&frames[1], i) - luma(&frames[0], i)).abs() * 4.).min(255.) as u8;
            difference[i * 4..i * 4 + 4].copy_from_slice(&[d, d, d, 255]);
        }
        save(&format!("snow-{name}-difference-x4"), &difference);
        report.push(serde_json::json!({
            "case": name,
            "energy_mean": mean,
            "energy_cv": deviation / mean.max(1.),
            "second_difference_per_pixel": second / windows / count as f64,
            "blinking_pixels_per_frame": blinks as f64 / windows,
            "tile_change_luma": tile_change,
        }));
    }
    let report = serde_json::Value::Array(report);
    std::fs::write(
        root.join("snow-flicker.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{report}");
}
