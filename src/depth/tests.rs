use super::*;

#[test]
#[ignore = "Owned private depth worker subprocess"]
fn worker_entry() {
    super::main();
}

fn fixture() -> (std::path::PathBuf, Config, Request) {
    use crate::widget_style::resolver::{ClockFace, ClockSurface, ClockWeight};
    let root = std::env::temp_dir().join(format!(
        "halcyon-depth-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("ridge.png");
    std::fs::write(&path, PICTURE).unwrap();
    let mut config = Config::default();
    config.appearance.family = Some(crate::desktop_theme::Family::Soft);
    config.appearance.wallpaper_light = false;
    config.clock.depth = true;
    // Today's unlit look; scene lighting has its own tests below.
    config.clock.lighting = Lighting::Off;
    config.clock.face = Some(ClockFace::Digital);
    config.clock.surface = Some(ClockSurface::None);
    config.clock.weight = Some(ClockWeight::Bold);
    config.clock.show_date = false;
    config.clock.scale = 200;
    let time = chrono::NaiveDate::from_ymd_opt(2026, 10, 19)
        .unwrap()
        .and_hms_opt(9, 41, 0)
        .unwrap();
    let request = Request::new(
        &config,
        Frame {
            screen: [1920., 1080.],
            origin: [560., 260.],
            size: [800., 330.],
            scale: 1.,
        },
        BgEntry::new("all", BgSource::Path(path)),
        time,
    )
    .unwrap();
    (root, config, request)
}

#[test]
fn fb14_prepared_identity_rejects_replacement_devices_and_folders() {
    let (root, _, mut request) = fixture();
    assert!(matches_picture(&request.background));
    let BgSource::Path(path) = &request.background.source else {
        unreachable!()
    };
    let mut bytes = PICTURE.to_vec();
    bytes[400] ^= 1;
    std::fs::write(path, bytes).unwrap();
    assert!(!matches_picture(&request.background));
    for path in [
        std::path::PathBuf::from("/dev/zero"),
        root.clone(),
        root.join("missing"),
    ] {
        request.background.source = BgSource::Path(path);
        assert!(!matches_picture(&request.background));
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fb14_mask_alignment_is_shared_across_scales_and_letterboxing() {
    let mask = image::GrayImage::from_fn(100, 100, |_, y| {
        image::Luma([if y >= 50 { 255 } else { 0 }])
    });
    for scale in [1., 1.5, 2.] {
        for (mode, screen, origin) in [
            (ScalingMode::Zoom, [200., 100.], [40., 30.]),
            (ScalingMode::Fit([0.; 3]), [200., 100.], [60., 30.]),
        ] {
            let frame = Frame {
                screen,
                origin,
                size: [40., 40.],
                scale,
            };
            let [width, height] = frame.pixels().unwrap();
            let mut p = Pixels {
                width,
                height,
                rgba: [39, 80, 140, 255].repeat((width * height) as usize),
            };
            apply_mask(&mut p, &frame, &mode, &mask, None).unwrap();
            for (i, pixel) in p.rgba.chunks_exact(4).enumerate() {
                assert_eq!(&pixel[..3], &[39, 80, 140]);
                let y = (i as u32 / width) as f32 / scale + origin[1];
                if y < 48. {
                    assert_eq!(pixel[3], 255);
                }
                if y > 52. {
                    assert_eq!(pixel[3], 0);
                }
            }
        }
    }
    let frame = Frame {
        screen: [200., 100.],
        origin: [0., 0.],
        size: [40., 40.],
        scale: 1.,
    };
    let mut p = Pixels {
        width: 40,
        height: 40,
        rgba: [255; 4].repeat(1600),
    };
    apply_mask(&mut p, &frame, &ScalingMode::Fit([0.; 3]), &mask, None).unwrap();
    assert!(
        p.rgba.chunks_exact(4).all(|p| p[3] == 255),
        "letterbox never occludes"
    );
}

#[test]
fn fb14_native_clock_alpha_only_at_three_scales_and_movement() {
    let (root, config, mut request) = fixture();
    let mut cut = 0;
    for scale in [1., 1.5, 2.] {
        request.frame.scale = scale;
        let masked = render(&request).unwrap();
        let mut native = crate::native_test::Native::new(cosmic::iced::Size::new(800., 330.));
        let time =
            chrono::NaiveDateTime::parse_from_str(&request.time, "%Y-%m-%dT%H:%M:%S").unwrap();
        let plain = native.render_scaled(
            crate::app::settings_clock_preview_at(&config, request.frame.size, time),
            &cosmic::Theme::dark(),
            f64::from(scale),
        );
        assert_eq!(plain.len(), masked.rgba.len());
        for (a, b) in plain.chunks_exact(4).zip(masked.rgba.chunks_exact(4)) {
            let rgb = [a[0], a[1], a[2]].map(|v| {
                if a[3] == 0 {
                    0
                } else {
                    ((u32::from(v) * 255 + u32::from(a[3]) / 2) / u32::from(a[3])).min(255) as u8
                }
            });
            assert_eq!(&rgb, &b[..3], "depth keeps straight glyph colors");
            assert!(b[3] <= a[3]);
            cut += usize::from(b[3] < a[3]);
        }
        if let Ok(dir) = std::env::var("HALCYON_DEPTH_EVIDENCE") {
            std::fs::create_dir_all(&dir).unwrap();
            let dir = std::path::Path::new(&dir);
            image::save_buffer(
                dir.join(format!("clock-{scale}.png")),
                &masked.rgba,
                masked.width,
                masked.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
            if scale == 1. {
                use cosmic::iced::widget::{image as native_image, pin, stack};
                let picture = image::load_from_memory(PICTURE).unwrap().to_rgba8();
                let bg = native_image(native_image::Handle::from_rgba(
                    picture.width(),
                    picture.height(),
                    picture.into_raw(),
                ))
                .width(1920.)
                .height(1080.)
                .content_fit(cosmic::iced::ContentFit::Cover);
                let clock = native_image(native_image::Handle::from_rgba(
                    masked.width,
                    masked.height,
                    masked.rgba.clone(),
                ))
                .width(800.)
                .height(330.);
                let view: cosmic::Element<'_, crate::app::Message> = stack([
                    bg.into(),
                    pin(clock)
                        .position(cosmic::iced::Point::new(560., 260.))
                        .into(),
                ])
                .width(1920.)
                .height(1080.)
                .into();
                let mut native =
                    crate::native_test::Native::new(cosmic::iced::Size::new(1920., 1080.));
                let pixels = native.render(view, &cosmic::Theme::dark());
                image::save_buffer(
                    dir.join("ridge-scene.png"),
                    &pixels,
                    1920,
                    1080,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
    }
    assert!(cut > 1000, "the foreground actually occludes clock ink");
    request.frame.origin = [560., 0.];
    request.frame.scale = 1.;
    let clear = render(&request).unwrap();
    request.frame.origin = [560., 650.];
    let hidden = render(&request);
    assert!(clear.rgba.chunks_exact(4).any(|p| p[3] > 0));
    assert!(
        hidden.is_none(),
        "fully covered time must use the plain fallback"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fb14_fallbacks_reject_unsupported_frames_faces_and_surfaces() {
    let (root, mut config, mut request) = fixture();
    request.background.scaling_mode = ScalingMode::Stretch;
    assert!(render(&request).is_none());
    for bad in [[f32::NAN, 10.], [4096., 2048.], [0., 10.]] {
        request.frame.size = bad;
        assert!(request.frame.pixels().is_none());
    }
    config.clock.face = Some(crate::widget_style::resolver::ClockFace::Analog);
    assert!(!eligible(&config));
    config.clock.face = Some(crate::widget_style::resolver::ClockFace::Digital);
    config.clock.surface = Some(crate::widget_style::resolver::ClockSurface::Panel);
    assert!(!eligible(&config));
    config.clock.surface = Some(crate::widget_style::resolver::ClockSurface::None);
    config.appearance.contrast_assist = true;
    assert!(!eligible(&config));
    config.appearance.contrast_assist = false;
    config.glance.enabled = true;
    assert!(!eligible(&config));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fb14_worker_renders_and_cancellation_retires_owned_work() {
    let (root, _, request) = fixture();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let (sender, cancel) = tokio::sync::watch::channel(false);
    let bytes = serde_json::to_vec(&request).unwrap();
    let pixels = runtime
        .block_on(worker::render_depth(bytes.clone(), cancel.clone()))
        .unwrap();
    assert_eq!((pixels.width, pixels.height), (800, 330));
    sender.send_replace(true);
    assert!(
        runtime
            .block_on(worker::render_depth(bytes, cancel))
            .is_none()
    );
    assert!(
        runtime
            .block_on(worker::render_depth(
                vec![0; MAX_INPUT + 1],
                sender.subscribe()
            ))
            .is_none()
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fb14_profile_depth_has_its_own_version_boundary() {
    use crate::desktop::profiles;
    let mut profile = profiles::preset("tidal").unwrap().profile;
    assert_eq!(profile.written_version(), 9);
    profile.widget_style.as_mut().unwrap()["clock"]["depth"] = true.into();
    profile.version = 10;
    assert!(profile.validate().is_err());
    profile.version = 11;
    profile.validate().unwrap();
    assert_eq!(profile.written_version(), 11);
    let exported = profiles::export(&profile).unwrap();
    assert!(
        profiles::import(&exported.bytes)
            .unwrap()
            .widget_style
            .unwrap()["clock"]["depth"]
            == true
    );
}

#[test]
fn scene_lighting_profile_has_its_own_version_boundary() {
    use crate::desktop::profiles;
    let mut profile = profiles::preset("tidal").unwrap().profile;
    profile.widget_style.as_mut().unwrap()["clock"]["depth"] = true.into();
    assert_eq!(profile.written_version(), 11);
    profile.widget_style.as_mut().unwrap()["clock"]["lighting"] = "rich".into();
    profile.version = 19;
    assert!(profile.validate().is_err());
    profile.version = 20;
    profile.validate().unwrap();
    assert_eq!(profile.written_version(), 20);
    let exported = profiles::export(&profile).unwrap();
    let imported = profiles::import(&exported.bytes).unwrap();
    assert_eq!(imported.version, 20);
    assert!(imported.widget_style.unwrap()["clock"]["lighting"] == "rich");
    // A choice this version doesn't know is refused rather than guessed.
    profile.widget_style.as_mut().unwrap()["clock"]["lighting"] = "dramatic".into();
    assert!(profiles::export(&profile).is_err() || profile.validate().is_err());
}

#[test]
fn fb14_date_stays_visible_and_public_render_needs_no_local_picture() {
    let (root, mut config, mut request) = fixture();
    config.clock.show_date = true;
    config.clock.center_vertically = true;
    let now = chrono::NaiveDateTime::parse_from_str(&request.time, "%Y-%m-%dT%H:%M:%S").unwrap();
    request.frame.origin = [560., 220.];
    let public = public_clock(&config, request.frame.clone(), ScalingMode::Zoom, now).unwrap();
    let mut native = crate::native_test::Native::new(cosmic::iced::Size::new(800., 330.));
    let plain = native.render(
        crate::app::settings_clock_preview_at(&config, request.frame.size, now),
        &cosmic::Theme::dark(),
    );
    let rows = crate::app::settings_clock_face_rows(&config, request.frame.size, now);
    let mut visible = 0;
    for (i, (a, b)) in plain
        .chunks_exact(4)
        .zip(public.rgba.chunks_exact(4))
        .enumerate()
    {
        if (i as u32 / public.width) as f32 > rows[1] + 2. {
            assert_eq!(a[3], b[3], "date is never occluded");
            visible += usize::from(b[3] > 0);
        }
    }
    assert!(visible > 50);
    std::fs::remove_dir_all(root).unwrap();
    assert!(public_clock(&config, request.frame.clone(), ScalingMode::Zoom, now).is_some());
    request.style["clock"]["scale"] = 65535.into();
    assert!(
        request.config().is_none(),
        "child validation cannot bypass saved bounds"
    );
}

#[test]
fn fb14_observation_uses_assigned_output_and_shared_fallback() {
    use crate::desktop::store::tests::Fixture;
    let fixture = Fixture::new("fb14-output-source");
    let shared = BgEntry::new("all", BgSource::Path(fixture.0.join("shared.png")));
    let selected = BgEntry::new("DP-2", BgSource::Path(fixture.0.join("selected.png")));
    fixture.user(
        wallpaper::NAMESPACE,
        "all",
        &ron::to_string(&shared).unwrap(),
    );
    fixture.user(wallpaper::NAMESPACE, "same-on-all", "false");
    fixture.user(
        wallpaper::NAMESPACE,
        wallpaper::output_key("DP-2").unwrap().key(),
        &ron::to_string(&selected).unwrap(),
    );
    // Native background selection ignores unlisted retained output files.
    assert_eq!(observe_from(&fixture.store(), "DP-2"), Some(shared.clone()));
    fixture.user(wallpaper::NAMESPACE, "backgrounds", "[\"DP-2\"]");
    assert_eq!(observe_from(&fixture.store(), "DP-2"), Some(selected));
    assert_eq!(observe_from(&fixture.store(), "DP-1"), Some(shared.clone()));
    fixture.user(wallpaper::NAMESPACE, "same-on-all", "true");
    assert_eq!(observe_from(&fixture.store(), "DP-2"), Some(shared));
}

/// Ridge's own setup: 800x330 at (0.5, 0.25) of a 1920x1080 screen, bold,
/// centred, 200%, with the date.
fn ridge_setup(time: &str, format_24h: bool, lighting: Lighting) -> (Config, Request) {
    use crate::widget_style::resolver::{ClockFace, ClockSurface, ClockWeight};
    let mut config = Config::default();
    config.appearance.family = Some(crate::desktop_theme::Family::Soft);
    config.appearance.wallpaper_light = false;
    config.clock.depth = true;
    config.clock.lighting = lighting;
    config.clock.face = Some(ClockFace::Digital);
    config.clock.surface = Some(ClockSurface::None);
    config.clock.weight = Some(ClockWeight::Bold);
    config.clock.italic = Some(false);
    config.clock.alignment = crate::config::ClockAlignment::Center;
    config.clock.format_24h = format_24h;
    config.clock.scale = 200;
    let time = chrono::NaiveDateTime::parse_from_str(
        &format!("2026-10-19T{time}:00"),
        "%Y-%m-%dT%H:%M:%S",
    )
    .unwrap();
    let mut entry = BgEntry::new("all", BgSource::Color(wallpaper::BgColor::Single([0.; 3])));
    entry.scaling_mode = ScalingMode::Zoom;
    let request = Request::new(
        &config,
        Frame {
            screen: [1920., 1080.],
            origin: [560., 188.],
            size: [800., 330.],
            scale: 1.,
        },
        entry,
        time,
    )
    .unwrap();
    (config, request)
}

fn compose_scene(clock: &Pixels, origin: [f32; 2], size: [f32; 2]) -> Vec<u8> {
    use cosmic::iced::widget::{image as native_image, pin, stack};
    let picture = image::load_from_memory(PICTURE).unwrap().to_rgba8();
    let bg = native_image(native_image::Handle::from_rgba(
        picture.width(),
        picture.height(),
        picture.into_raw(),
    ))
    .width(1920.)
    .height(1080.)
    .content_fit(cosmic::iced::ContentFit::Cover);
    let clock = native_image(native_image::Handle::from_rgba(
        clock.width,
        clock.height,
        clock.rgba.clone(),
    ))
    .width(size[0])
    .height(size[1]);
    let view: cosmic::Element<'_, crate::app::Message> = stack([
        bg.into(),
        pin(clock)
            .position(cosmic::iced::Point::new(origin[0], origin[1]))
            .into(),
    ])
    .width(1920.)
    .height(1080.)
    .into();
    crate::native_test::Native::new(cosmic::iced::Size::new(1920., 1080.))
        .render(view, &cosmic::Theme::dark())
}

/// `HALCYON_DEPTH_EVIDENCE=<dir>` writes Ridge scenes for each time and
/// lighting choice; `HALCYON_LIGHTING_TUNE=name=value,...` overrides the recipe.
#[test]
#[ignore = "Writes evidence images"]
fn lighting_evidence() {
    let dir = std::path::PathBuf::from(std::env::var("HALCYON_DEPTH_EVIDENCE").unwrap());
    std::fs::create_dir_all(&dir).unwrap();
    let tune = std::env::var("HALCYON_LIGHTING_TUNE").unwrap_or_default();
    let only = std::env::var("HALCYON_LIGHTING_ONLY").unwrap_or_default();
    // Worker inputs for timing and memory runs of the real subprocess.
    let (_root, _, mut request) = fixture();
    for lighting in Lighting::ALL {
        for scale in [1., 2.] {
            request.style["clock"]["lighting"] = serde_json::to_value(lighting).unwrap();
            request.frame.scale = scale;
            let name = format!("request-{}-{scale}.json", lighting.label().to_lowercase());
            std::fs::write(dir.join(name), serde_json::to_vec(&request).unwrap()).unwrap();
        }
    }
    for (name, time, h24) in [
        ("1042", "10:42", true),
        ("2359", "23:59", true),
        ("0805", "08:05", false),
    ] {
        for lighting in Lighting::ALL {
            if !only.is_empty()
                && !only
                    .split(',')
                    .any(|o| o == lighting.label().to_lowercase() || o == name)
            {
                continue;
            }
            lighting::TUNE.with(|t| {
                t.set(
                    lighting
                        .recipe()
                        .filter(|_| !tune.is_empty())
                        .map(|r| r.tuned(&tune)),
                )
            });
            let (_, request) = ridge_setup(time, h24, lighting);
            let started = std::time::Instant::now();
            let clock = render_clock(&request).expect("Ridge setup keeps depth");
            let took = started.elapsed();
            lighting::TUNE.with(|t| t.set(None));
            let scene = compose_scene(&clock, request.frame.origin, request.frame.size);
            let file = dir.join(format!(
                "ridge-{name}-{}.png",
                lighting.label().to_lowercase()
            ));
            image::save_buffer(&file, &scene, 1920, 1080, image::ColorType::Rgba8).unwrap();
            eprintln!("{} {:?}", file.display(), took);
        }
    }
}

#[test]
fn lighting_off_matches_plain_and_lit_is_deterministic_rgb_only() {
    let (_, off) = ridge_setup("10:42", true, Lighting::Off);
    let plain = render_clock(&off).unwrap();
    // Off never decodes the picture or touches a pixel: the fixture test above
    // pins those pixels to the native clock, so Off is today's look.
    assert_eq!(Lighting::Off.recipe(), None);
    assert_eq!(Config::default().clock.lighting, Lighting::Soft);
    let rows = {
        let (config, request) = ridge_setup("10:42", true, Lighting::Off);
        let time =
            chrono::NaiveDateTime::parse_from_str(&request.time, "%Y-%m-%dT%H:%M:%S").unwrap();
        crate::app::settings_clock_face_rows(&config, request.frame.size, time)
    };
    for lighting in [Lighting::Soft, Lighting::Rich] {
        let (_, request) = ridge_setup("10:42", true, lighting);
        let lit = render_clock(&request).unwrap();
        assert_eq!(
            lit,
            render_clock(&request).unwrap(),
            "{lighting:?} is deterministic"
        );
        assert_eq!((lit.width, lit.height), (plain.width, plain.height));
        let mut changed = 0;
        for (i, (a, b)) in plain
            .rgba
            .chunks_exact(4)
            .zip(lit.rgba.chunks_exact(4))
            .enumerate()
        {
            assert_eq!(a[3], b[3], "lighting never changes coverage");
            let row = (i as u32 / lit.width) as f32;
            if row > rows[1] + 1. {
                assert_eq!(a, b, "the date stays as it was");
            }
            changed += usize::from(a[..3] != b[..3]);
        }
        assert!(
            changed > 5000,
            "{lighting:?} visibly relights the face ({changed})"
        );
    }
}

#[test]
fn lighting_keeps_the_readability_floor_and_the_fallback() {
    let floor = |lit: [f32; 3], base: [f32; 3], bg: f32| {
        let out = lighting::readable(lit, base, bg, &lighting::RICH);
        lighting::luma(out)
    };
    // Light ink sunk toward a bright sky comes back to 1.5x + 0.04 of it.
    let out = floor([0.3; 3], [0.9; 3], 0.4);
    assert!((out - 0.64).abs() < 0.01, "{out}");
    // It never needs to be brighter than the ink was.
    assert!((floor([0.2; 3], [0.5; 3], 0.4) - 0.5).abs() < 0.01);
    // Lit ink that already contrasts stays untouched.
    assert_eq!(
        floor([0.8, 0.7, 0.75], [0.9; 3], 0.2),
        lighting::luma([0.8, 0.7, 0.75])
    );
    // Dark ink is held below the mirrored ceiling.
    let out = floor([0.5; 3], [0.02; 3], 0.4);
    assert!(out <= (0.4 - 0.04) / 1.5 + 0.01, "{out}");

    // An overdone recipe still leaves every solid digit pixel clearly
    // brighter than the picture right behind it.
    let (config, request) = ridge_setup("10:42", true, Lighting::Rich);
    let harsh =
        lighting::RICH.tuned("ambient=1,exposure=1,contact=1,haze_far=0.15,haze_near=0.15,wrap=0");
    lighting::TUNE.with(|t| t.set(Some(harsh)));
    let lit = render_clock(&request);
    lighting::TUNE.with(|t| t.set(None));
    let lit = lit.unwrap();
    let time = chrono::NaiveDateTime::parse_from_str(&request.time, "%Y-%m-%dT%H:%M:%S").unwrap();
    let rows = crate::app::settings_clock_face_rows(&config, request.frame.size, time);
    let picture = image::load_from_memory(PICTURE).unwrap().to_rgb8();
    let (scale, offset) = transform(
        &request.frame,
        &ScalingMode::Zoom,
        [picture.width(), picture.height()],
    )
    .unwrap();
    let (mut solid, mut weak) = (0, 0);
    for y in rows[0].ceil() as u32..rows[1].floor() as u32 {
        for x in 0..lit.width {
            let p = &lit.rgba[((y * lit.width + x) * 4) as usize..][..4];
            if p[3] < 255 {
                continue;
            }
            let at = [x, y].map(|v| v as f64 + 0.5);
            let src = [0, 1].map(|i| {
                ((f64::from(request.frame.origin[i]) + at[i] - offset[i]) / scale[i] - 0.5).round()
                    as u32
            });
            let bg = picture.get_pixel(
                src[0].min(picture.width() - 1),
                src[1].min(picture.height() - 1),
            );
            let l = |c: [u8; 3]| lighting::luma(c.map(lighting::to_linear));
            solid += 1;
            weak += usize::from(l([p[0], p[1], p[2]]) < 1.25 * l(bg.0));
        }
    }
    assert!(solid > 20_000, "{solid}");
    assert!(
        weak * 1000 < solid,
        "{weak} of {solid} solid pixels lost contrast"
    );

    // Lighting runs after the occlusion check, so a time mostly behind the
    // ridge still falls back to the plain clock.
    let (_, mut request) = ridge_setup("10:42", true, Lighting::Rich);
    request.frame.origin = [560., 650.];
    assert!(render_clock(&request).is_none());
}

#[test]
fn lighting_renders_inside_the_worker_limits() {
    let (root, _, mut request) = fixture();
    request.style["clock"]["lighting"] = "rich".into();
    request.frame.scale = 2.;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let (_sender, cancel) = tokio::sync::watch::channel(false);
    let bytes = serde_json::to_vec(&request).unwrap();
    let pixels = runtime
        .block_on(worker::render_depth(bytes, cancel))
        .unwrap();
    assert!(
        pixels == render(&request).unwrap(),
        "the worker draws the lit clock"
    );
    request.style["clock"]["lighting"] = "off".into();
    assert!(pixels != render(&request).unwrap());
    std::fs::remove_dir_all(root).unwrap();
}
