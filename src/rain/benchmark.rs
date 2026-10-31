//! Explicit device measurements. GPU timestamps exclude screenshot readback and
//! texture preparation; wall time includes submission/wait. Neither is watts.
use super::*;
use cosmic::iced::widget::shader::{Pipeline as _, Primitive as _, Viewport};
use cosmic::iced::{Rectangle, wgpu};
use std::time::{Duration, Instant};

#[test]
#[ignore = "explicit hardware GPU benchmark; writes only the evidence directory"]
fn rain_gpu_benchmark() {
    let root = std::path::PathBuf::from(
        std::env::var_os("HALCYON_RAIN_EVIDENCE").expect("private evidence directory"),
    );
    std::fs::create_dir_all(&root).unwrap();
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    });
    let adapter = cosmic::iced::futures::executor::block_on(instance.request_adapter(
        &wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: None,
        },
    ))
    .unwrap();
    let features =
        wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
    let timestamps = adapter.features().contains(features);
    let (device, queue) = cosmic::iced::futures::executor::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("Rain benchmark"),
            required_features: if timestamps {
                features
            } else {
                wgpu::Features::empty()
            },
            ..Default::default()
        },
    ))
    .unwrap();
    let info = adapter.get_info();
    println!("Rain GPU adapter: {info:?}");
    assert_ne!(
        info.device_type,
        wgpu::DeviceType::Cpu,
        "benchmark must identify an actual GPU"
    );
    let format = wgpu::TextureFormat::Bgra8Unorm;
    let mut pipeline = gpu::Pipeline::new(&device, &queue, format);
    let query = timestamps.then(|| {
        device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("Rain GPU time"),
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        })
    });
    let resolve = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let source = image::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/looks/evening-wallpaper.jpg"
    ))
    .unwrap();
    let mut results = Vec::new();
    let stress = std::env::var_os("HALCYON_EFFECT_STRESS").is_some();
    let cases = if stress {
        vec![
            (3840, 2160, Quality::Eco, 1),
            (3840, 2160, Quality::Balanced, 1),
            (3840, 2160, Quality::Smooth, 1),
            (2160, 3840, Quality::Balanced, 1),
            (2560, 1440, Quality::Balanced, 2),
        ]
    } else {
        vec![
            (1920, 1080, Quality::Balanced, 1),
            (2560, 1440, Quality::Balanced, 1),
            (3840, 2160, Quality::Balanced, 1),
        ]
    };
    for (width, height, quality, displays) in cases {
        let pixels = source
            .resize_exact(width, height, image::imageops::FilterType::Triangle)
            .to_rgba8();
        let scene = gpu::Scene::new(crate::desktop::wallpaper::media::Pixels {
            width,
            height,
            rgba: pixels.into_raw(),
        });
        let outputs: Vec<_> = (0..displays)
            .map(|_| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: None,
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
            })
            .collect();
        let views: Vec<_> = outputs
            .iter()
            .map(|o| o.create_view(&Default::default()))
            .collect();
        let mut scenes = vec![scene.clone()];
        for _ in 1..displays {
            scenes.push(gpu::Scene::new(crate::desktop::wallpaper::media::Pixels {
                width,
                height,
                rgba: scene.pixels.rgba.clone(),
            }));
        }
        let bounds = Rectangle {
            width: width as f32,
            height: height as f32,
            ..Default::default()
        };
        let viewport = Viewport::with_physical_size(cosmic::iced::Size::new(width, height), 1.);
        let clip = Rectangle {
            x: 0,
            y: 0,
            width,
            height,
        };
        for (effect, mode) in [
            (Effect::Rain, Mode::Procedural),
            (Effect::Rain, Mode::Hybrid),
            (Effect::Snow, Mode::Procedural),
            (Effect::Fog, Mode::Procedural),
            (Effect::Clouds, Mode::Procedural),
        ] {
            let mut preferences = Preferences {
                enabled: true,
                mode,
                effect,
                intensity: 70,
                quality,
                ..Default::default()
            };
            if stress {
                preferences.intensity = 100;
                preferences.size = 100;
                preferences.speed = 100;
                preferences.trails = 100;
                preferences.snow.density = 100;
                preferences.snow.size = 100;
                preferences.snow.opacity = 100;
                preferences.snow.speed = 100;
                preferences.fog.opacity = 100;
                preferences.fog.height = 100;
                preferences.fog.speed = 100;
                preferences.clouds.coverage = 100;
                preferences.clouds.strength = 100;
                preferences.clouds.speed = 100;
            }
            let fps = f64::from(quality.fps());
            let mut sim = simulation::Simulation::default();
            sim.seed(width as f32 / height as f32, &preferences);
            if preferences.hybrid() {
                for _ in 0..900 {
                    sim.step(1. / 30., width as f32 / height as f32, &preferences);
                }
            }
            let mut gpu_ms = Vec::new();
            let mut prepare_us = Vec::new();
            let mut wall_ms = Vec::new();
            let mut sim_us = Vec::new();
            for frame in 0..60 {
                let tick = Instant::now();
                if preferences.hybrid() {
                    sim.step(1. / fps as f32, width as f32 / height as f32, &preferences);
                }
                let simulation_us = tick.elapsed().as_secs_f64() * 1e6;
                let primitives: Vec<_> = scenes
                    .iter()
                    .map(|scene| gpu::Primitive {
                        scene: scene.clone(),
                        preferences: preferences.clone(),
                        time: frame as f32 / fps as f32,
                        drops: if preferences.hybrid() {
                            sim.instances(preferences.trails)
                        } else {
                            Vec::new()
                        },
                    })
                    .collect();
                let prep_started = Instant::now();
                for primitive in &primitives {
                    primitive.prepare(&mut pipeline, &device, &queue, &bounds, &viewport);
                }
                if frame >= 15 {
                    prepare_us.push(prep_started.elapsed().as_secs_f64() * 1e6);
                }
                let mut encoder = device.create_command_encoder(&Default::default());
                if let Some(q) = &query {
                    encoder.write_timestamp(q, 0);
                }
                for (primitive, view) in primitives.iter().zip(&views) {
                    primitive.render(&pipeline, &mut encoder, view, &clip);
                }
                if let Some(q) = &query {
                    encoder.write_timestamp(q, 1);
                    encoder.resolve_query_set(q, 0..2, &resolve, 0);
                    encoder.copy_buffer_to_buffer(&resolve, 0, &read, 0, 16);
                }
                let started = Instant::now();
                queue.submit([encoder.finish()]);
                device
                    .poll(wgpu::PollType::Wait {
                        submission_index: None,
                        timeout: Some(Duration::from_secs(10)),
                    })
                    .unwrap();
                let elapsed = started.elapsed().as_secs_f64() * 1000.;
                if timestamps {
                    let (send, receive) = std::sync::mpsc::channel();
                    read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                        let _ = send.send(r);
                    });
                    device
                        .poll(wgpu::PollType::Wait {
                            submission_index: None,
                            timeout: Some(Duration::from_secs(10)),
                        })
                        .unwrap();
                    receive.recv().unwrap().unwrap();
                    let bytes = read.slice(..).get_mapped_range();
                    let a = u64::from_le_bytes(bytes[..8].try_into().unwrap());
                    let b = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
                    if frame >= 15 {
                        gpu_ms.push((b - a) as f64 * f64::from(queue.get_timestamp_period()) / 1e6);
                    }
                    drop(bytes);
                    read.unmap();
                }
                if frame >= 15 {
                    wall_ms.push(elapsed);
                    sim_us.push(simulation_us);
                }
                // Wallpaper cadence; avoid measuring only a saturated turbo GPU.
                std::thread::sleep(Duration::from_secs_f64(1. / fps));
            }
            fn stats(mut v: Vec<f64>) -> serde_json::Value {
                if v.is_empty() {
                    return serde_json::Value::Null;
                }
                v.sort_by(f64::total_cmp);
                serde_json::json!({"median":v[v.len()/2],"p95":v[v.len()*95/100]})
            }
            results.push(serde_json::json!({"size":[width,height],"displays":displays,"owned_texture_bytes":scenes.iter().map(|s|s.texture_bytes()).sum::<u64>(),"effect":effect,"mode":mode,"preferences":preferences,"quality":quality,"fps":fps,"stress":stress,"drops":sim.drops.len(),"prepare_us":stats(prepare_us),"gpu_ms":stats(gpu_ms),"submit_wait_ms":stats(wall_ms),"simulation_us":stats(sim_us)}));
        }
    }
    let report = serde_json::json!({"adapter":format!("{info:?}"),"timestamps":timestamps,"scope":"Isolated native wgpu effect passes, cached backgrounds. Sleeps for the selected 30/60 fps interval after each submission. Excludes desktop compositor and other apps. Power consumption not measured.","results":results});
    std::fs::write(
        root.join(if stress {
            "gpu-stress.json"
        } else {
            "gpu-benchmark.json"
        }),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{report}");
}
