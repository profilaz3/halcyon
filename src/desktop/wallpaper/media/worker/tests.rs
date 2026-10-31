use super::*;
use crate::desktop::wallpaper::BgColor;
static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[test]
#[ignore = "private child entry, started only by the bounded image worker fixture"]
fn worker_entry() {
    super::main();
}

fn entry(stalled: bool) -> BgEntry {
    BgEntry::new(
        if stalled { "fixture-stall" } else { "all" },
        BgSource::Color(BgColor::Single([0.2, 0.4, 0.8])),
    )
}

#[tokio::test]
async fn fb09_cold_worker_matches_the_existing_renderer_without_config_writes() {
    let _guard = TEST_LOCK.lock().await;
    let (_owner, cancel) = watch::channel(false);
    let root = std::env::temp_dir().join(format!("halcyon-fb09-image-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let picture = root.join("source.png");
    image::RgbaImage::from_fn(2048, 1536, |x, y| {
        image::Rgba([(x / 8) as u8, (y / 6) as u8, 128, 255])
    })
    .save(&picture)
    .unwrap();
    let config = root.join("config.json");
    std::fs::write(&config, b"private fixture; not a preview output").unwrap();
    let entry = BgEntry::new("all", BgSource::Path(picture.clone()));
    let before = [
        std::fs::read(&picture).unwrap(),
        std::fs::read(&config).unwrap(),
    ];
    let expected = pixels(&entry).unwrap();
    let started = std::time::Instant::now();
    let actual = render(entry.clone(), cancel.clone())
        .await
        .expect("worker image");
    eprintln!(
        "FB09 cold 2048x1536 PNG decode/fit/IPC: {:?}",
        started.elapsed()
    );
    assert_eq!(actual, expected);
    let thumb = thumbnail(picture.clone(), cancel.clone()).await.unwrap();
    let Preview::Ready(expected_thumb) = crate::desktop::wallpaper::media::thumbnail(&picture)
    else {
        panic!("fixture thumbnail")
    };
    assert_eq!(thumb, expected_thumb);
    let fixture = crate::desktop::store::tests::Fixture::new("fb09-color-worker");
    fixture.user(
        crate::desktop::wallpaper::NAMESPACE,
        "all",
        &ron::to_string(&entry).unwrap(),
    );
    let store = fixture.store();
    let draft = crate::desktop::wallpaper::WallpaperDraft::load(&store).unwrap();
    let source = crate::desktop::wallpaper::automatic::source(&store, &draft);
    assert_eq!(
        source.analyse_preview(cancel).await.unwrap(),
        source.analyse().unwrap()
    );
    assert_eq!(
        [
            std::fs::read(&picture).unwrap(),
            std::fs::read(&config).unwrap()
        ],
        before
    );
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn fb09_started_worker_cancellation_is_acknowledged_and_reaped_within_deadline() {
    let _guard = TEST_LOCK.lock().await;
    let (owner, cancel) = watch::channel(false);
    let (started, ready) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(run(
        entry(true),
        cancel,
        Duration::from_secs(4),
        move |pid| {
            let _ = started.send(pid);
        },
    ));
    let pid = tokio::time::timeout(Duration::from_secs(2), ready)
        .await
        .unwrap()
        .unwrap();
    owner.send(true).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), task)
            .await
            .unwrap()
            .unwrap()
            .is_none()
    );
    assert_eq!(
        unsafe { libc::kill(pid as i32, 0) },
        -1,
        "exact owned child was reaped"
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
}

#[tokio::test]
async fn fb09_worker_slots_stay_bounded_and_cancelled_waiters_never_start() {
    let _guard = TEST_LOCK.lock().await;
    let mut owners = Vec::new();
    let mut tasks = Vec::new();
    for _ in 0..2 {
        let (owner, cancel) = watch::channel(false);
        let (started, ready) = tokio::sync::oneshot::channel();
        tasks.push(tokio::spawn(run(
            entry(true),
            cancel,
            Duration::from_secs(4),
            move |pid| {
                let _ = started.send(pid);
            },
        )));
        tokio::time::timeout(Duration::from_secs(2), ready)
            .await
            .unwrap()
            .unwrap();
        owners.push(owner);
    }
    assert_eq!(SLOTS.available_permits(), 0);
    let (owner, cancel) = watch::channel(false);
    let waiting = tokio::spawn(run(entry(true), cancel, Duration::from_secs(4), |_| {
        panic!("cancelled waiter started")
    }));
    tokio::task::yield_now().await;
    owner.send(true).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), waiting)
            .await
            .unwrap()
            .unwrap()
            .is_none()
    );
    for owner in owners {
        owner.send(true).unwrap();
    }
    for task in tasks {
        assert!(
            tokio::time::timeout(Duration::from_millis(100), task)
                .await
                .unwrap()
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(SLOTS.available_permits(), 2);
}

#[tokio::test]
async fn fb09_dropped_future_keeps_its_slot_until_the_child_is_reaped() {
    let _guard = TEST_LOCK.lock().await;
    let (_owner, cancel) = watch::channel(false);
    let mut tasks = Vec::new();
    let mut pids = Vec::new();
    for _ in 0..2 {
        let (started, ready) = tokio::sync::oneshot::channel();
        tasks.push(tokio::spawn(run(
            entry(true),
            cancel.clone(),
            Duration::from_secs(4),
            move |pid| {
                let _ = started.send(pid);
            },
        )));
        pids.push(ready.await.unwrap());
    }
    assert_eq!(SLOTS.available_permits(), 0);
    let first = tasks.remove(0);
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    let previous = pids[0];
    let replacement = run(entry(false), cancel, Duration::from_secs(4), move |_| {
        assert_eq!(
            unsafe { libc::kill(previous as i32, 0) },
            -1,
            "replacement acquired a slot while its previous child still existed"
        );
    });
    assert!(
        tokio::time::timeout(Duration::from_secs(1), replacement)
            .await
            .unwrap()
            .is_some()
    );
    let second = tasks.remove(0);
    second.abort();
    assert!(second.await.unwrap_err().is_cancelled());
    tokio::time::timeout(Duration::from_millis(100), async {
        while SLOTS.available_permits() != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(unsafe { libc::kill(pids[1] as i32, 0) }, -1);
}

#[tokio::test]
async fn fb09_owner_drop_and_timeout_retire_the_exact_worker() {
    let _guard = TEST_LOCK.lock().await;
    for drop_owner in [true, false] {
        let (owner, cancel) = watch::channel(false);
        let (started, ready) = tokio::sync::oneshot::channel();
        let timeout = if drop_owner {
            Duration::from_secs(4)
        } else {
            Duration::from_millis(150)
        };
        let task = tokio::spawn(run(entry(true), cancel, timeout, move |pid| {
            let _ = started.send(pid);
        }));
        let pid = tokio::time::timeout(Duration::from_secs(2), ready)
            .await
            .unwrap()
            .unwrap();
        let owner = if drop_owner {
            drop(owner);
            None
        } else {
            Some(owner)
        };
        assert!(
            tokio::time::timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap()
                .is_none()
        );
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
        drop(owner);
    }
}

#[tokio::test]
async fn rain_wallpaper_worker_uses_exact_output_bounds_and_cancellation() {
    let _guard = TEST_LOCK.lock().await;
    let (owner, cancel) = watch::channel(false);
    for size in [[1920, 1080], [720, 1280]] {
        let result = wallpaper(entry(false), size, cancel.clone()).await.unwrap();
        assert_eq!([result.width, result.height], size);
        assert_eq!(result.rgba.len(), (size[0] * size[1] * 4) as usize);
    }
    assert!(
        wallpaper(entry(false), [8192, 8192], cancel.clone())
            .await
            .is_none()
    );
    owner.send_replace(true);
    assert!(
        wallpaper(entry(false), [1920, 1080], cancel)
            .await
            .is_none()
    );
}
