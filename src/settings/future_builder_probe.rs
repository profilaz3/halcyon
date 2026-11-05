//! Explicit, isolated measurement of the current native Settings tree.
//! Includes GPU screenshot readback; this is not compositor input-to-paint latency.
use super::*;
use crate::native_test::Native;
use cosmic::iced::{Color, Event, Point, mouse};
use serde_json::json;
use std::{path::PathBuf, time::Instant};

fn rss_kib() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find(|line| line.starts_with("VmRSS:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

#[test]
#[ignore = "explicit preset/type capture; requires a private fixture"]
fn measure_curated_presets() {
    let root = PathBuf::from(std::env::var_os("HALCYON_BUILDER_PROBE_ROOT").expect("private root"));
    assert!(root.is_absolute());
    assert_eq!(Config::path().unwrap(), root.join("config.json"));
    assert_eq!(
        std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap(),
        root.join("desktop").as_os_str()
    );
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::DesktopProfiles);
    app.wizard = None;
    let mut rows = Vec::new();
    for name in ["tidal", "paper-studio", "carbon", "rain"] {
        app.profiles
            .update(profiles::Message::Select(profiles::Selection::Preset(name)));
        for _ in 0..4 {
            let themes = app.profiles.take_theme_jobs();
            let backdrops = app.profiles.take_jobs();
            if themes.is_empty() && backdrops.is_empty() {
                break;
            }
            for job in themes {
                app.profiles.update(job.run());
            }
            for job in backdrops {
                app.profiles.update(job.run());
            }
        }
        assert!(
            app.profiles.backdrop_image().is_some(),
            "{name}: {:?} {:?}",
            app.profiles.backdrop(),
            app.profiles.theme_problem()
        );
        assert!(
            app.profiles.can_apply(),
            "{name}: {:?}",
            app.profiles.plan()
        );
        let selected = app
            .profiles
            .presets()
            .iter()
            .find(|p| p.id == name)
            .unwrap();
        let mut native = Native::with_backend(Size::new(1280., 720.), "wgpu");
        let pixels = native.render(app.preset_thumbnail(selected, 720.), &cosmic::Theme::dark());
        image::save_buffer(
            root.join("output").join(format!("{name}-scene.png")),
            &pixels,
            1280,
            720,
            image::ColorType::Rgba8,
        )
        .unwrap();
        if name == "paper-studio" {
            for backend in ["wgpu", "tiny-skia"] {
                let mut native = Native::with_backend(Size::new(480., 270.), backend);
                let handle = app.profiles.backdrop_image().unwrap().clone();
                let pixels = native.render::<Message>(
                    cosmic::widget::image(handle).width(480).height(270).into(),
                    &cosmic::Theme::dark(),
                );
                image::save_buffer(
                    root.join("output")
                        .join(format!("paper-backdrop-{backend}.png")),
                    &pixels,
                    480,
                    270,
                    image::ColorType::Rgba8,
                )
                .unwrap();
                let _ = app.update(Message::Resized(Size::new(820., 600.)));
                let mut native = Native::with_backend(Size::new(820., 600.), backend);
                let pixels = native.render(app.settings_view(), &cosmic::Theme::dark());
                image::save_buffer(
                    root.join("output")
                        .join(format!("paper-review-{backend}.png")),
                    &pixels,
                    820,
                    600,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
        for (width, height) in [(820, 600), (1280, 800)] {
            let _ = app.update(Message::Resized(Size::new(width as f32, height as f32)));
            let mut native = Native::with_backend(Size::new(width as f32, height as f32), "wgpu");
            let pixels = native.render(app.settings_view(), &cosmic::Theme::dark());
            image::save_buffer(
                root.join("output")
                    .join(format!("{name}-review-{width}.png")),
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )
            .unwrap();
            let mut times = Vec::new();
            for n in 0..23 {
                let start = Instant::now();
                drop(native.render(app.settings_view(), &cosmic::Theme::dark()));
                if n >= 3 {
                    times.push(start.elapsed().as_secs_f64() * 1000.);
                }
            }
            let pixels = native.render(app.settings_view(), &cosmic::Theme::dark());
            image::save_buffer(
                root.join("output").join(format!("{name}-warm-{width}.png")),
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )
            .unwrap();
            rows.push(json!({"preset":name,"width":width,"height":height,"draw_readback_ms":times,"rss_kib":rss_kib()}));
        }
    }
    let _ = app.update(Message::Page(Page::Clock));
    if app.scene_open() {
        let _ = app.update(Message::ToggleScene);
    }
    let _ = app.update(Message::FontMenu(Some(FontTarget::Clock)));
    let _ = app.update(Message::FontCatalogReady(
        crate::desktop::appearance::catalog::FontCatalog::installed(),
    ));
    assert!(!app.widget_font_families().is_empty());
    for (width, height) in [(820, 600), (1280, 800)] {
        let _ = app.update(Message::Resized(Size::new(width as f32, height as f32)));
        for (mode, theme) in [
            ("dark", cosmic::Theme::dark()),
            ("light", cosmic::Theme::light()),
        ] {
            let mut native = Native::with_backend(Size::new(width as f32, height as f32), "wgpu");
            let bounds = native.bounds(app.settings_view(), "font-clock");
            native.scroll(
                app.settings_view(),
                "settings-content",
                (bounds.y - 90.).max(0.),
            );
            let pixels = native.render(app.settings_view(), &theme);
            image::save_buffer(
                root.join("output").join(format!("type-{mode}-{width}.png")),
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
    assert_eq!(before, std::fs::read(Config::path().unwrap()).unwrap());
    std::fs::write(root.join("output/presets.json"),serde_json::to_vec_pretty(&json!({"configuration_unchanged":true,"scope":"Actual native preset components and candidate review; explicit sample content; stock panel capability; no live providers or physical compositor acceptance","rows":rows})).unwrap()).unwrap();
}

#[test]
#[ignore = "explicit Clock foundation capture; requires a private fixture"]
fn measure_clock_foundation() {
    use crate::widget_style::resolver::{ClockFace, ClockSurface, ClockWeight};
    let root = PathBuf::from(std::env::var_os("HALCYON_BUILDER_PROBE_ROOT").expect("private root"));
    assert!(root.is_absolute());
    assert_eq!(Config::path().unwrap(), root.join("config.json"));
    assert_eq!(
        std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap(),
        root.join("desktop").as_os_str()
    );
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::Clock);
    app.wizard = None;
    let mut rows = Vec::new();
    for (name, family, face) in [
        (
            "tidal",
            crate::desktop_theme::Family::Soft,
            ClockFace::Digital,
        ),
        (
            "paper",
            crate::desktop_theme::Family::Paper,
            ClockFace::Analog,
        ),
    ] {
        app.config.appearance.family = Some(family);
        let (opacity, radius, border) = family.geometry();
        app.config.appearance.opacity = opacity;
        app.config.appearance.corner_radius = radius;
        app.config.appearance.border_opacity = border;
        app.config.appearance.wallpaper_light = name == "paper";
        app.config.appearance.accent = Some(if name == "tidal" {
            [135, 185, 219]
        } else {
            family.accent()
        });
        app.config.clock.face = Some(face);
        app.config.clock.alignment = crate::config::ClockAlignment::Center;
        app.config.clock.center_vertically = true;
        app.config.clock.weight = Some(if name == "paper" {
            ClockWeight::Normal
        } else {
            ClockWeight::Light
        });
        app.config.clock.surface = Some(if name == "paper" {
            ClockSurface::Panel
        } else {
            ClockSurface::None
        });
        for (width, height) in [(820u32, 600u32), (1280, 800)] {
            let size = Size::new(width as f32, height as f32);
            let _ = app.update(Message::Resized(size));
            let mut native = Native::with_backend(size, "wgpu");
            let theme = controls_theme(&app.config);
            let pixels = native.render(app.settings_view(), &theme);
            image::save_buffer(
                root.join("output")
                    .join(format!("{name}-settings-{width}.png")),
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )
            .unwrap();
            let mut times = Vec::new();
            for n in 0..43 {
                let start = Instant::now();
                let _ = native.event(
                    app.settings_view(),
                    Event::Mouse(mouse::Event::WheelScrolled {
                        delta: mouse::ScrollDelta::Lines {
                            x: 0.,
                            y: if n < 22 { -1. } else { 1. },
                        },
                    }),
                    Point::new(width as f32 * 0.7, height as f32 * 0.6),
                );
                drop(native.render(app.settings_view(), &theme));
                if n >= 3 {
                    times.push(start.elapsed().as_secs_f64() * 1000.);
                }
            }
            rows.push(json!({"style":name,"width":width,"height":height,"draw_readback_ms":times,"rss_kib":rss_kib()}));
        }
        let mut native = Native::with_backend(Size::new(520., 300.), "wgpu");
        let bg = if name == "paper" {
            Color::from_rgb8(243, 237, 225)
        } else {
            Color::from_rgb8(27, 51, 75)
        };
        let view = cosmic::widget::container(
            crate::app::settings_clock_preview(&app.config, [480., 260.]).map(|_| Message::Preview),
        )
        .padding(20)
        .width(520)
        .height(300)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(bg.into()),
                ..Default::default()
            }
        }));
        let pixels = native.render(view.into(), &controls_theme(&app.config));
        image::save_buffer(
            root.join("output").join(format!("{name}-clock.png")),
            &pixels,
            520,
            300,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    assert_eq!(before, std::fs::read(Config::path().unwrap()).unwrap());
    std::fs::write(root.join("output/samples.json"),serde_json::to_vec_pretty(&json!({"configuration_unchanged":true,"scope":"Real native Clock and Settings; isolated presentation fixtures, no provider execution; synchronous readback, not input-to-present latency","rows":rows})).unwrap()).unwrap();
}

#[test]
#[ignore = "explicit baseline probe; requires a private fixture and GPU"]
fn measure_current_native_tree() {
    let root = PathBuf::from(std::env::var_os("HALCYON_BUILDER_PROBE_ROOT").expect("private root"));
    assert!(root.is_absolute());
    assert_eq!(Config::path().unwrap(), root.join("config.json"));
    assert_eq!(
        std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap(),
        root.join("desktop").as_os_str()
    );
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    assert!(root.join("output").is_dir());
    let (mut app, _) = Settings::init(Core::default(), Page::Clock);
    app.wizard = None;
    let theme = controls_theme(&app.config);
    let mut rows = Vec::new();
    for (width, height, backend, count) in [
        (820u32, 600u32, "wgpu", 40usize),
        (1280, 800, "wgpu", 40),
        (1280, 800, "tiny-skia", 12),
    ] {
        let size = Size::new(width as f32, height as f32);
        let _ = app.update(Message::Resized(size));
        let started = Instant::now();
        let mut native = Native::with_backend(size, backend);
        let init_ms = started.elapsed().as_secs_f64() * 1000.;
        for page in [Page::Clock, Page::Weather, Page::Overview, Page::Themes] {
            let _ = app.update(Message::Page(page));
            let start = Instant::now();
            let pixels = native.render(app.settings_view(), &theme);
            let cold_ms = start.elapsed().as_secs_f64() * 1000.;
            assert_eq!(pixels.len(), (width * height * 4) as usize);
            if backend == "wgpu" {
                image::save_buffer(
                    root.join("output").join(format!("{page:?}-{width}.png")),
                    &pixels,
                    width,
                    height,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            for _ in 0..3 {
                drop(native.render(app.settings_view(), &theme));
            }
            let mut frames = Vec::new();
            let mut events = Vec::new();
            for n in 0..count {
                let start = Instant::now();
                let _ = native.event(
                    app.settings_view(),
                    Event::Mouse(mouse::Event::WheelScrolled {
                        delta: mouse::ScrollDelta::Lines {
                            x: 0.,
                            y: if n < count / 2 { -1. } else { 1. },
                        },
                    }),
                    Point::new(width as f32 * 0.7, height as f32 * 0.55),
                );
                events.push(start.elapsed().as_secs_f64() * 1000.);
                let pixels = native.render(app.settings_view(), &theme);
                frames.push(start.elapsed().as_secs_f64() * 1000.);
                assert_eq!(pixels.len(), (width * height * 4) as usize);
            }
            let row = json!({
                "page":format!("{page:?}"),"backend":backend,"renderer":native.renderer_name(),
                "width":width,"height":height,"scale":1,"samples":count,
                "renderer_init_ms":init_ms,"first_page_render_ms":cold_ms,
                "event_and_draw_readback_ms":frames,"event_dispatch_ms":events,
                "process_rss_kib":rss_kib()
            });
            println!("BUILDER_PROBE {backend} {width}x{height} {page:?}: {count} samples");
            rows.push(row);
        }
    }
    assert_eq!(before, std::fs::read(Config::path().unwrap()).unwrap());
    std::fs::write(
        root.join("output/samples.json"),
        serde_json::to_vec_pretty(&json!({
            "scope":"Current Settings widget tree, isolated preferences, synchronous screenshot readback. No compositor presentation or provider execution.",
            "configuration_unchanged":true,"rows":rows
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "explicit component capture; requires a private fixture"]
fn measure_component_foundation() {
    use crate::widget_style::resolver::{ClockFace, ClockSurface};
    let root = PathBuf::from(std::env::var_os("HALCYON_BUILDER_PROBE_ROOT").expect("private root"));
    assert!(root.is_absolute());
    assert_eq!(Config::path().unwrap(), root.join("config.json"));
    assert_eq!(
        std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap(),
        root.join("desktop").as_os_str()
    );
    let before = std::fs::read(Config::path().unwrap()).unwrap();
    let (mut app, _) = Settings::init(Core::default(), Page::Clock);
    app.wizard = None;
    let mut captures = Vec::new();
    for (name, family) in [
        ("tidal", crate::desktop_theme::Family::Soft),
        ("paper", crate::desktop_theme::Family::Paper),
    ] {
        app.config.appearance.family = Some(family);
        let (opacity, radius, border) = family.geometry();
        app.config.appearance.opacity = opacity;
        app.config.appearance.corner_radius = radius;
        app.config.appearance.border_opacity = border;
        app.config.appearance.wallpaper_light = name == "paper";
        app.config.clock.surface = Some(if name == "paper" {
            ClockSurface::Panel
        } else {
            ClockSurface::None
        });
        app.config.clock.alignment = crate::config::ClockAlignment::Center;
        app.config.clock.center_vertically = true;
        for (face_name, face) in [
            ("digital", ClockFace::Digital),
            ("analog", ClockFace::Analog),
        ] {
            app.config.clock.face = Some(face);
            for scale in [1., 1.5, 2.] {
                let mut native = Native::with_backend(Size::new(490., 200.), "wgpu");
                let pixels = native.render_scaled(
                    crate::app::settings_clock_preview(&app.config, [490., 200.]),
                    &controls_theme(&app.config),
                    scale,
                );
                let filename = format!("{name}-{face_name}-{}.png", (scale * 100.) as u32);
                image::save_buffer(
                    root.join("output").join(&filename),
                    &pixels,
                    (490. * scale) as u32,
                    (200. * scale) as u32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
                captures.push(filename);
            }
        }
        let mut native = Native::with_backend(Size::new(360., 180.), "wgpu");
        let pixels = native.render(
            crate::app::settings_weather_preview(&app.config),
            &controls_theme(&app.config),
        );
        image::save_buffer(
            root.join("output").join(format!("{name}-weather.png")),
            &pixels,
            360,
            180,
            image::ColorType::Rgba8,
        )
        .unwrap();
        for (width, height) in [(820, 600), (1280, 800)] {
            let _ = app.update(Message::Resized(Size::new(width as f32, height as f32)));
            for page in [Page::Clock, Page::Weather, Page::Overview, Page::Themes] {
                let _ = app.update(Message::Page(page));
                let mut native =
                    Native::with_backend(Size::new(width as f32, height as f32), "wgpu");
                let pixels = native.render(app.settings_view(), &controls_theme(&app.config));
                image::save_buffer(
                    root.join("output")
                        .join(format!("{name}-{page:?}-{width}.png")),
                    &pixels,
                    width,
                    height,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
    }
    assert_eq!(before, std::fs::read(Config::path().unwrap()).unwrap());
    std::fs::write(root.join("output/components.json"),serde_json::to_vec_pretty(&json!({"configuration_unchanged":true,"scope":"Native Clock/Weather and Settings/scene capture; synthetic weather; no providers; physical renderer scales are not compositor/hotplug acceptance","clock_captures":captures})).unwrap()).unwrap();
}
