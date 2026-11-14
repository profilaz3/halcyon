//! Own pictures: the render worker's answers, the scene worker's limits,
//! and the real model (ignored; it needs the downloaded file).
use super::*;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::watch;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}

fn png(image: image::DynamicImage) -> Vec<u8> {
    let mut out = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

/// A 960×540 sky over a hill whose top is picture row `horizon`, saved
/// under `root`, and the analysis the scene worker would keep for it.
pub(crate) fn hill(root: &Path, horizon: u32) -> (PathBuf, String, cache::Files) {
    let picture = image::RgbImage::from_fn(960, 540, |_, y| {
        image::Rgb(if y < horizon {
            [150, 190, 235]
        } else {
            [40, 70, 45]
        })
    });
    let depth = scene::Depth {
        width: 96,
        height: 54,
        values: (0..54u32)
            .flat_map(|y| (0..96).map(move |_| if y * 10 < horizon { 1. } else { 6. }))
            .collect(),
    };
    analysed(root, "hill.png", picture, &depth)
}

/// An even grey-blue picture with nothing in front of anything, saved as
/// `flat.png`, and its analysis.
pub(crate) fn flat(root: &Path) -> (PathBuf, String, cache::Files) {
    let picture = image::RgbImage::from_pixel(960, 540, image::Rgb([90, 120, 160]));
    let depth = scene::Depth {
        width: 64,
        height: 36,
        values: vec![3.; 64 * 36],
    };
    analysed(root, "flat.png", picture, &depth)
}

fn analysed(
    root: &Path,
    name: &str,
    picture: image::RgbImage,
    depth: &scene::Depth,
) -> (PathBuf, String, cache::Files) {
    let found = scene::derive(depth, &picture).unwrap();
    let path = root.join(name);
    picture.save(&path).unwrap();
    let key = cache::key(&std::fs::read(&path).unwrap());
    let files = cache::Files {
        stats: found.stats,
        mask: png(found.mask.into()),
        edges: png(found.edges.into()),
        picture: png(found.picture.into()),
    };
    (path, key, files)
}

fn judged(rendered: &Rendered) -> &scene::Judgement {
    match &rendered.found {
        Found::Judged { judgement, .. } => judgement,
        other => panic!("not judged: {other:?}"),
    }
}

#[test]
fn own_wallpaper_render_is_unknown_then_nudged_then_cut() {
    let (root, _, request) = fixture();
    let scenes = root.join("scenes");
    let (path, key, files) = hill(&root, 300);
    let mut request = Request {
        background: BgEntry::new("all", BgSource::Path(path)),
        ..request
    }
    .with_scenes(Some(scenes.clone()));
    // Public previews never look in the cache.
    assert!(
        render_scene(&Request {
            scenes: None,
            ..request.clone()
        })
        .is_none()
    );
    assert_eq!(
        render_scene(&request).unwrap(),
        Rendered {
            found: Found::Unknown { key: key.clone() },
            pixels: None
        }
    );
    cache::store(&scenes, &key, &files).unwrap();

    // High in the sky the clock stays plain, with a spot below.
    request.frame.origin = [560., 0.];
    let high = render_scene(&request).unwrap();
    let judgement = judged(&high);
    assert_eq!(
        judgement.verdict,
        scene::Verdict::NeedsNudge,
        "{judgement:?}"
    );
    assert!(high.pixels.is_none());
    let spot = judgement.suggested.unwrap();
    assert_eq!(spot[0], 560.);

    // There, the hill covers the bottom of the time and nothing else.
    request.frame.origin = spot;
    let started = Instant::now();
    let there = render_scene(&request).unwrap();
    eprintln!("judge and cut: {:?}", started.elapsed());
    assert_eq!(judged(&there).verdict, scene::Verdict::Works);
    let cut = there.pixels.clone().unwrap();
    let time = chrono::NaiveDateTime::parse_from_str(&request.time, "%Y-%m-%dT%H:%M:%S").unwrap();
    let (plain, face) = draw(&request.config().unwrap(), &request.frame, time).unwrap();
    let line = 600. - spot[1];
    let (mut hidden, mut kept) = (0, 0);
    for (i, (a, b)) in plain
        .rgba
        .chunks_exact(4)
        .zip(cut.rgba.chunks_exact(4))
        .enumerate()
    {
        let y = (i as u32 / plain.width) as f32;
        if a[3] == 0 || y < face[0] {
            continue;
        }
        if y < line - 4. {
            assert_eq!(a, b, "above the hill nothing changes");
            kept += 1;
        } else if y > line + 4. && b[3] == 0 {
            hidden += 1;
        }
    }
    assert!(kept > 1000 && hidden > 1000, "{kept} {hidden}");

    // The render worker answers the same.
    let (_owner, cancel) = watch::channel(false);
    let via = runtime()
        .block_on(worker::render_scene(
            serde_json::to_vec(&request).unwrap(),
            cancel,
        ))
        .unwrap();
    assert_eq!(via, there);

    // An entry that no longer reads is the same as none.
    std::fs::write(cache::dir(&scenes, &key).unwrap().join("mask.png"), b"x").unwrap();
    assert_eq!(
        render_scene(&request).unwrap().found,
        Found::Unknown { key }
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// Only the choices are settings: what a picture holds stays in this
/// computer's cache. Profiles, setups and shares carry `depth` and
/// `lighting` and no new version, never a mask, key or path, and a depth-on
/// setup arriving where nothing was found shows the plain clock.
#[test]
fn own_wallpaper_masks_and_paths_never_travel() {
    use crate::desktop::profiles;
    use crate::desktop::store::widget::{self, WidgetGroup};
    let clock: Vec<&str> = widget::fields(WidgetGroup::Style)
        .chain(widget::fields(WidgetGroup::Layout))
        .map(|field| field.pointer)
        .filter(|pointer| pointer.starts_with("/clock/"))
        .collect();
    assert!(clock.contains(&"/clock/depth") && clock.contains(&"/clock/lighting"));
    for pointer in &clock {
        for local in ["mask", "scene", "key", "path", "cache", "spot", "model"] {
            assert!(!pointer.contains(local), "{pointer}");
        }
    }

    let (root, _, request) = fixture();
    let scenes = root.join("scenes");
    let (path, key, files) = hill(&root, 300);
    cache::store(&scenes, &key, &files).unwrap();
    let mut profile = profiles::preset("tidal").unwrap().profile;
    // Only the clock's version boundary: Tidal's icon shape needs a later one.
    let icons = profile.appearance.as_mut().unwrap();
    (icons.icon_shape, icons.folder_tint) = (None, None);
    profile.widget_style.as_mut().unwrap()["clock"]["depth"] = true.into();
    profile.widget_style.as_mut().unwrap()["clock"]["lighting"] = "rich".into();
    profile.version = profile.written_version();
    assert_eq!(
        profile.version,
        profiles::VERSION_20,
        "lighting needs only the scene lighting version"
    );
    let shared = profiles::export(&profile).unwrap().bytes;
    for bytes in [shared.clone(), profiles::export_local(&profile).unwrap()] {
        let text = String::from_utf8(bytes).unwrap();
        for local in [
            key.as_str(),
            "scenes",
            "mask",
            "models",
            &root.to_string_lossy(),
        ] {
            assert!(!text.contains(local), "{local} in {text}");
        }
    }

    // Received on a computer that never looked at its picture: plain.
    let imported = profiles::import(&shared).unwrap();
    assert!(imported.widget_style.unwrap()["clock"]["depth"] == true);
    let elsewhere = root.join("elsewhere");
    let request = Request {
        background: BgEntry::new("all", BgSource::Path(path)),
        ..request
    };
    for scenes in [None, Some(elsewhere)] {
        let rendered = render_scene(&request.clone().with_scenes(scenes));
        assert!(
            rendered.as_ref().is_none_or(|r| r.pixels.is_none()),
            "{rendered:?}"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn own_wallpaper_ridge_keeps_its_drawn_matte() {
    let (root, _, request) = fixture();
    let request = request.with_scenes(Some(root.join("scenes")));
    let rendered = render_scene(&request).unwrap();
    assert_eq!(rendered.found, Found::Ridge);
    assert!(rendered.pixels.unwrap() == render(&request).unwrap());
    assert!(!root.join("scenes").exists(), "Ridge never needs the cache");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn own_wallpaper_render_reply_is_bounded() {
    let found = serde_json::to_vec(&Found::Unknown {
        key: "a".repeat(64),
    })
    .unwrap();
    let reply = |found: &[u8], w: u32, h: u32, rgba: usize| {
        let mut bytes = (found.len() as u32).to_le_bytes().to_vec();
        bytes.extend(found);
        bytes.extend(w.to_le_bytes());
        bytes.extend(h.to_le_bytes());
        bytes.extend(vec![7; rgba]);
        bytes
    };
    assert_eq!(read_rendered(&reply(&found, 0, 0, 0)).unwrap().pixels, None);
    assert_eq!(
        read_rendered(&reply(&found, 2, 1, 8))
            .unwrap()
            .pixels
            .unwrap()
            .width,
        2
    );
    assert!(read_rendered(&reply(&found, 2, 1, 7)).is_none());
    assert!(read_rendered(&reply(&found, 0, 0, 4)).is_none());
    let wide = MAX_RASTER[0] + 1;
    assert!(read_rendered(&reply(&found, wide, 1, wide as usize * 4)).is_none());
    assert!(read_rendered(&reply(b"{\"Other\":1}", 0, 0, 0)).is_none());
    let mut long = reply(&found, 0, 0, 0);
    long[..4].copy_from_slice(&(64 * 1024 + 1u32).to_le_bytes());
    assert!(read_rendered(&long).is_none());
}

/// A value from `/proc/<pid>/limits`, such as "Max address space".
fn proc_limit(pid: u32, name: &str) -> Option<String> {
    let limits = std::fs::read_to_string(format!("/proc/{pid}/limits")).ok()?;
    let line = limits.lines().find(|l| l.starts_with(name))?;
    line[name.len()..]
        .split_whitespace()
        .next()
        .map(str::to_owned)
}

#[test]
fn own_wallpaper_scene_worker_is_limited_cancelled_and_reaped() {
    let job = analyse::Job {
        picture: "/nonexistent/picture.png".into(),
        model: "/nonexistent/model.onnx".into(),
        size: 1,
        sha256: "0".repeat(64),
        threads: 1,
    };
    runtime().block_on(async {
        let _guard = worker::tests::TEST_LOCK.lock().await;
        // Its own limits, read while it waits.
        let (owner, cancel) = watch::channel(false);
        let (started, ready) = tokio::sync::oneshot::channel();
        let stalled = job.clone();
        let task = tokio::spawn(async move {
            analyse::run_job(&stalled, cancel, true, move |pid| {
                let _ = started.send(pid);
            })
            .await
        });
        let pid = tokio::time::timeout(Duration::from_secs(4), ready)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            proc_limit(pid, "Max address space").unwrap(),
            analyse::MEMORY.to_string()
        );
        assert_eq!(
            proc_limit(pid, "Max cpu time").unwrap(),
            analyse::CPU_SECONDS.to_string()
        );
        assert_eq!(proc_limit(pid, "Max core file size").unwrap(), "0");
        let nice_of = |stat: &str| -> i32 {
            stat.rsplit(')')
                .next()
                .unwrap()
                .split_whitespace()
                .nth(16)
                .unwrap()
                .parse()
                .unwrap()
        };
        // Nice is per thread on Linux. The thread that runs the job lowers
        // its own, and the inference threads it starts later inherit it. In
        // the app that is the main thread; under libtest it is the test's.
        let nices: Vec<i32> = std::fs::read_dir(format!("/proc/{pid}/task"))
            .unwrap()
            .map(|task| {
                nice_of(&std::fs::read_to_string(task.unwrap().path().join("stat")).unwrap())
            })
            .collect();
        // Below the desktop's priority; a parent already lower keeps its own.
        let parent = nice_of(&std::fs::read_to_string("/proc/self/stat").unwrap());
        assert!(nices.contains(&parent.max(10)), "{nices:?}");
        // Cancel stops and reaps that exact child.
        owner.send(true).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(200), task)
                .await
                .unwrap()
                .unwrap()
                .is_none()
        );
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );

        // A child past its deadline is stopped the same way.
        let (_owner, cancel) = watch::channel(false);
        let started = Instant::now();
        let (seen, pid) = std::sync::mpsc::channel();
        let reply = worker::run_child(
            worker::Child {
                flag: "--depth-scene-worker",
                test: "depth::tests::scene_worker_entry",
                slots: 2,
                deadline: Duration::from_millis(1500),
                max: 1024,
                stall: true,
            },
            serde_json::to_vec(&job).unwrap(),
            cancel,
            move |p| {
                let _ = seen.send(p);
            },
        )
        .await;
        assert!(reply.is_none());
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(unsafe { libc::kill(pid.recv().unwrap() as i32, 0) }, -1);
    });
}

#[test]
fn own_wallpaper_scene_worker_refuses_a_wrong_model_or_picture() {
    let (root, _, _) = fixture();
    let scenes = root.join("scenes");
    let picture = root.join("ridge.png");
    let model = root.join("model.onnx");
    std::fs::write(&model, vec![1u8; 4096]).unwrap();
    let source = model::Source {
        url: String::new(),
        size: 4096,
        sha256: "0".repeat(64),
    };
    let text = root.join("notes.png");
    std::fs::write(&text, b"not a picture").unwrap();
    runtime().block_on(async {
        let _guard = worker::tests::TEST_LOCK.lock().await;
        let (_owner, cancel) = watch::channel(false);
        let analyse = |picture: PathBuf, source: model::Source| {
            let (model, scenes, cancel) = (model.clone(), scenes.clone(), cancel.clone());
            async move { analyse::analyse(&picture, &model, &source, scenes, cancel).await }
        };
        // The model file is checked by hash before it runs.
        assert_eq!(
            analyse(picture.clone(), source.clone()).await,
            Err(analyse::Failure::Model)
        );
        // A different size is the wrong file too.
        assert_eq!(
            analyse(
                picture.clone(),
                model::Source {
                    size: 4097,
                    ..source.clone()
                }
            )
            .await,
            Err(analyse::Failure::Model)
        );
        assert_eq!(
            analyse(text.clone(), source.clone()).await,
            Err(analyse::Failure::Picture)
        );
    });
    assert!(!scenes.exists(), "nothing is kept for a failure");
    assert_eq!(
        analyse::Failure::Model.sentence(),
        "The scene model needs downloading again"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn own_wallpaper_scene_reply_must_be_whole() {
    let reply = |reply: &analyse::Reply, files: &[&[u8]]| {
        let json = serde_json::to_vec(reply).unwrap();
        let mut bytes = (json.len() as u32).to_le_bytes().to_vec();
        bytes.extend(json);
        for file in files {
            bytes.extend((file.len() as u32).to_le_bytes());
            bytes.extend(*file);
        }
        bytes
    };
    let done = analyse::Reply::Done {
        key: "b".repeat(64),
        stats: scene::Stats {
            source: [64, 48],
            eta: 0.9,
            coverage: 0.4,
            top_touch: 0.,
            flat: false,
        },
    };
    let (key, files) = analyse::parse(&reply(&done, &[b"m", b"e", b"p"])).unwrap();
    assert_eq!(
        (key.len(), files.mask.as_slice(), files.picture.as_slice()),
        (64, &b"m"[..], &b"p"[..])
    );
    assert_eq!(
        analyse::parse(&reply(&done, &[b"m", b"e"])).err(),
        Some(analyse::Failure::Stopped)
    );
    let mut cut = reply(&done, &[b"m", b"e", b"pp"]);
    cut.pop();
    assert_eq!(analyse::parse(&cut).err(), Some(analyse::Failure::Stopped));
    assert_eq!(
        analyse::parse(&reply(
            &analyse::Reply::Failed(analyse::Failure::Picture),
            &[]
        ))
        .err(),
        Some(analyse::Failure::Picture)
    );
}

/// The real model on one picture through the scene worker:
/// `HALCYON_DEPTH_MODEL=<the pinned model.onnx>` and optionally
/// `HALCYON_DEPTH_PICTURE=<picture>` (Ridge otherwise).
#[test]
#[ignore = "Needs the scene model: HALCYON_DEPTH_MODEL=<path>"]
fn own_wallpaper_real_model_analyses_a_picture() {
    let model =
        PathBuf::from(std::env::var_os("HALCYON_DEPTH_MODEL").expect("HALCYON_DEPTH_MODEL"));
    let (root, _, request) = fixture();
    let picture = std::env::var_os("HALCYON_DEPTH_PICTURE")
        .map_or_else(|| root.join("ridge.png"), PathBuf::from);
    let scenes = root.join("scenes");
    let started = Instant::now();
    let (_owner, cancel) = watch::channel(false);
    let key = runtime()
        .block_on(analyse::analyse(
            &picture,
            &model,
            &model::pinned(),
            scenes.clone(),
            cancel,
        ))
        .unwrap();
    let took = started.elapsed();
    let entry = cache::load(&scenes, &key).unwrap();
    assert!(entry.stats.eta.is_finite() && (0. ..=1.).contains(&entry.stats.coverage));
    let request = Request {
        background: BgEntry::new("all", BgSource::Path(picture)),
        ..request
    }
    .with_scenes(Some(scenes));
    let rendered = render_scene(&request).unwrap();
    eprintln!(
        "analysed in {took:?}: {:?}\n{:?}",
        entry.stats, rendered.found
    );
    assert!(matches!(
        rendered.found,
        Found::Judged { .. } | Found::Ridge
    ));
    std::fs::remove_dir_all(root).unwrap();
}

/// Highest resident and virtual size of `pid` until it exits.
fn watch_memory(pid: u32) -> std::thread::JoinHandle<(u64, u64)> {
    std::thread::spawn(move || {
        let (mut rss, mut peak) = (0u64, 0u64);
        while let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) {
            let kb = |name: &str| {
                status
                    .lines()
                    .find(|l| l.starts_with(name))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|v| v.parse::<u64>().ok())
            };
            match (kb("VmHWM:"), kb("VmPeak:")) {
                (Some(r), Some(p)) => {
                    rss = rss.max(r);
                    peak = peak.max(p);
                }
                _ => break,
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        (rss, peak)
    })
}

/// Shows `clock` on `picture` the way the desktop zooms it, at `origin` on
/// a 1920×1080 screen.
fn on_picture(
    picture: &image::RgbImage,
    clock: Option<&Pixels>,
    origin: [f32; 2],
) -> image::RgbImage {
    let k = (1920. / picture.width() as f32).max(1080. / picture.height() as f32);
    let (w, h) = (
        (picture.width() as f32 * k).round() as u32,
        (picture.height() as f32 * k).round() as u32,
    );
    let mut screen = image::imageops::crop_imm(
        &image::imageops::resize(picture, w, h, image::imageops::FilterType::Triangle),
        (w - 1920) / 2,
        (h - 1080) / 2,
        1920,
        1080,
    )
    .to_image();
    if let Some(clock) = clock {
        for (i, p) in clock.rgba.chunks_exact(4).enumerate() {
            let (x, y) = (
                origin[0] as u32 + i as u32 % clock.width,
                origin[1] as u32 + i as u32 / clock.width,
            );
            if x < 1920 && y < 1080 && p[3] > 0 {
                let a = f32::from(p[3]) / 255.;
                let under = screen.get_pixel_mut(x, y);
                for c in 0..3 {
                    under[c] = (f32::from(p[c]) * a + f32::from(under[c]) * (1. - a)).round() as u8;
                }
            }
        }
    }
    image::imageops::resize(&screen, 640, 360, image::imageops::FilterType::Triangle)
}

/// The local evaluation: `HALCYON_DEPTH_EVAL=<folder of <id>/scene.png>`,
/// `HALCYON_DEPTH_EVAL_OUT=<folder>` and `HALCYON_DEPTH_MODEL`. For each
/// picture it runs the scene worker, records its time and memory, and writes
/// the plain and the cut, lit clock side by side, plus `results.jsonl`.
#[test]
#[ignore = "Local evaluation over private pictures"]
fn own_wallpaper_evaluation() {
    use std::io::Write;
    let input = PathBuf::from(std::env::var_os("HALCYON_DEPTH_EVAL").expect("HALCYON_DEPTH_EVAL"));
    let out =
        PathBuf::from(std::env::var_os("HALCYON_DEPTH_EVAL_OUT").expect("HALCYON_DEPTH_EVAL_OUT"));
    let model =
        PathBuf::from(std::env::var_os("HALCYON_DEPTH_MODEL").expect("HALCYON_DEPTH_MODEL"));
    std::fs::create_dir_all(&out).unwrap();
    let (root, _, request) = fixture();
    let scenes = root.join("scenes");
    let mut ids: Vec<String> = std::fs::read_dir(&input)
        .unwrap()
        .flatten()
        .filter(|e| e.path().join("scene.png").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    ids.sort();
    let only: Vec<String> = std::env::var("HALCYON_DEPTH_EVAL_ONLY")
        .map(|v| v.split(',').map(str::to_owned).collect())
        .unwrap_or_default();
    let mut results = std::fs::File::create(out.join("results.jsonl")).unwrap();
    let rt = runtime();
    for id in ids.iter().filter(|id| only.is_empty() || only.contains(id)) {
        let picture = input.join(id).join("scene.png");
        let job = analyse::Job {
            picture: picture.clone(),
            model: model.clone(),
            size: model::SIZE,
            sha256: model::SHA256.into(),
            threads: analyse::THREADS,
        };
        let (_owner, cancel) = watch::channel(false);
        let memory = std::sync::Arc::new(std::sync::Mutex::new(None));
        let started = Instant::now();
        let seen = memory.clone();
        let reply = rt
            .block_on(analyse::run_job(&job, cancel, false, move |pid| {
                *seen.lock().unwrap() = Some(watch_memory(pid));
            }))
            .unwrap();
        let took = started.elapsed().as_secs_f32();
        let (rss, peak) = memory.lock().unwrap().take().unwrap().join().unwrap();
        let (key, files) = analyse::parse(&reply).unwrap();
        cache::store(&scenes, &key, &files).unwrap();
        let entry = cache::load(&scenes, &key).unwrap();

        let mut request = Request {
            background: BgEntry::new("all", BgSource::Path(picture.clone())),
            ..request.clone()
        }
        .with_scenes(Some(scenes.clone()));
        request.style["clock"]["lighting"] = "rich".into();
        let start = request.frame.origin;
        let judge_started = Instant::now();
        let here = render_scene(&request).unwrap();
        let judge_took = judge_started.elapsed().as_secs_f32();
        let judgement = judged(&here).clone();
        let mut spot = start;
        let mut cut = here.pixels.clone();
        if let Some(suggested) = judgement.suggested {
            spot = suggested;
            request.frame.origin = suggested;
            cut = render_scene(&request).unwrap().pixels;
        }
        let time =
            chrono::NaiveDateTime::parse_from_str(&request.time, "%Y-%m-%dT%H:%M:%S").unwrap();
        let mut plain_config = request.config().unwrap();
        plain_config.clock.lighting = Lighting::Off;
        let plain = draw(&plain_config, &request.frame, time).unwrap().0;
        let source = image::open(&picture).unwrap().to_rgb8();
        let mut row = image::RgbImage::new(1290, 360);
        image::imageops::replace(&mut row, &on_picture(&source, Some(&plain), start), 0, 0);
        image::imageops::replace(
            &mut row,
            &on_picture(&source, cut.as_ref().or(Some(&plain)), spot),
            650,
            0,
        );
        row.save(out.join(format!("{id}.png"))).unwrap();
        image::GrayImage::from_raw(
            entry.mask.width(),
            entry.mask.height(),
            entry.mask.into_raw(),
        )
        .unwrap()
        .save(out.join(format!("{id}-mask.png")))
        .unwrap();
        let line = serde_json::json!({
            "id": id,
            "seconds": took,
            "judge_seconds": judge_took,
            "rss_mb": rss as f64 / 1024.,
            "vm_peak_mb": peak as f64 / 1024.,
            "stats": entry.stats,
            "judgement": judgement,
            "spot": spot,
            "cut": cut.is_some(),
        });
        writeln!(results, "{line}").unwrap();
        eprintln!("{line}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

/// CPU time this thread has used.
fn thread_cpu() -> Duration {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut now) };
    Duration::new(now.tv_sec as u64, now.tv_nsec as u32)
}

/// The per-minute clock: once a picture is known, the next minute's worker
/// neither reads and hashes the wallpaper again nor judges the day again,
/// and draws the same clock. A changed file is read again.
#[test]
fn own_wallpaper_next_minute_reuses_what_the_last_answer_found() {
    let (root, _, request) = fixture();
    let scenes = root.join("scenes");
    // A wallpaper-sized photo: a hill under a grainy sky, about 10 MB.
    let (width, height, horizon) = (2560u32, 1440u32, 800u32);
    let mut seed = 0x2545_f491_u32;
    let picture = image::RgbImage::from_fn(width, height, |_, y| {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let grain = (seed >> 24) as u8 / 8;
        let [r, g, b] = if y < horizon {
            [150, 190, 235]
        } else {
            [40, 70, 45]
        };
        image::Rgb([r - grain, g - grain, b - grain])
    });
    let depth = scene::Depth {
        width: 96,
        height: 54,
        values: (0..54u32)
            .flat_map(|y| (0..96).map(move |_| if y * 10 < 300 { 1. } else { 6. }))
            .collect(),
    };
    let (path, key, files) = analysed(&root, "photo.png", picture, &depth);
    cache::store(&scenes, &key, &files).unwrap();
    let mut request = Request {
        background: BgEntry::new("all", BgSource::Path(path.clone())),
        ..request
    }
    .with_scenes(Some(scenes));
    request.frame.origin = [560., 0.];
    let spot = judged(&render_scene(&request).unwrap()).suggested.unwrap();
    request.frame.origin = spot;
    let first = render_scene(&request).unwrap();
    assert_eq!(judged(&first).verdict, scene::Verdict::Works);

    let next = Request {
        time: "2026-10-19T09:42:00".into(),
        ..request.clone()
    };
    let stamp = Stamp::at(&path).unwrap();
    let known = Request {
        seen: Some(Seen {
            stamp,
            found: Some(first.found.clone()),
        }),
        ..next.clone()
    };
    let (mut before, mut after) = (Duration::ZERO, Duration::ZERO);
    let runs = 3;
    for _ in 0..runs {
        let started = thread_cpu();
        let full = render_scene(&next).unwrap();
        before += thread_cpu() - started;
        let started = thread_cpu();
        let reused = render_scene(&known).unwrap();
        after += thread_cpu() - started;
        assert_eq!(reused, full, "the same clock either way");
        assert!(reused.pixels.is_some());
    }
    let (before, after) = (before / runs, after / runs);
    eprintln!(
        "per-minute depth render CPU ({} MB picture): {before:?} before, {after:?} after",
        std::fs::metadata(&path).unwrap().len() >> 20
    );
    assert!(after < before, "{after:?} < {before:?}");

    // A file changed since: its own content is read, whatever was seen.
    let stale = Request {
        seen: Some(Seen {
            stamp: Stamp {
                inode: stamp.inode.wrapping_add(1),
                ..stamp
            },
            found: Some(Found::Unknown {
                key: "0".repeat(64),
            }),
        }),
        ..next.clone()
    };
    assert_eq!(render_scene(&stale).unwrap(), render_scene(&next).unwrap());
    std::fs::remove_dir_all(root).unwrap();
}
