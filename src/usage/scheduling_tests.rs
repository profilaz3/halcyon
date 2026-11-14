use super::super::{Pending, RETRY_SECONDS, WARMUP};
use super::*;

fn now() -> i64 {
    1792843200
} // 2026-10-24 12:00 UTC
fn state() -> State {
    State::default()
}
fn pending(at: i64) -> State {
    State {
        pending: Some(Pending {
            created_at: now() - 10,
            retry_after: at,
            attempts: 1,
        }),
        ..state()
    }
}
fn fixture() -> crate::test_env::TempPath {
    crate::test_env::TempPath::new("scheduling").at("usage.json")
}

#[test]
fn schedules_only_real_work_not_an_idle_tick() {
    let now = now();
    assert_eq!(deadline(&state(), false, Duration::ZERO, now), None);
    assert_eq!(
        deadline(
            &State {
                enabled: false,
                ..state()
            },
            true,
            Duration::ZERO,
            now
        ),
        None
    );
    assert_eq!(deadline(&state(), true, WARMUP, now), Some(now + 300));
    assert_eq!(
        deadline(&state(), true, Duration::from_millis(1500), now),
        Some(now + 2)
    );
    assert_eq!(deadline(&state(), true, Duration::ZERO, now), Some(now));
    let completed = State {
        last_success: Some("2026-10-24".into()),
        ..state()
    };
    assert_eq!(
        deadline(&completed, true, Duration::ZERO, now),
        Some(1792886400)
    );
    // A resumed process recomputes from today's date, never backfills lost days.
    assert_eq!(
        deadline(&completed, true, Duration::ZERO, now + 2 * 86400),
        Some(now + 2 * 86400)
    );
    let retry = State {
        retry_after: now + RETRY_SECONDS,
        ..state()
    };
    assert_eq!(
        deadline(&retry, true, Duration::ZERO, now),
        Some(now + RETRY_SECONDS)
    );
}

#[test]
fn status_due_and_expiry_take_priority_even_when_disabled() {
    let now = now();
    let mut s = pending(now + RETRY_SECONDS);
    s.enabled = false;
    assert_eq!(
        deadline(&s, false, Duration::ZERO, now),
        Some(now + RETRY_SECONDS)
    );
    s.pending.as_mut().unwrap().retry_after = now + 2 * STATUS_LIFETIME;
    assert_eq!(
        deadline(&s, true, Duration::ZERO, now),
        Some(now - 10 + STATUS_LIFETIME)
    );
    s.pending.as_mut().unwrap().attempts = MAX_ATTEMPTS;
    assert_eq!(deadline(&s, false, Duration::ZERO, now), Some(now));
    s.pending.as_mut().unwrap().attempts = 1;
    s.pending.as_mut().unwrap().created_at = now + 100;
    assert_eq!(deadline(&s, false, Duration::ZERO, now), Some(now));
}

#[tokio::test]
async fn indefinite_wait_is_idle_and_notifications_coalesce_without_lost_wakeups() {
    let ready = Notify::new();
    assert!(
        tokio::time::timeout(Duration::from_millis(30), wait(&ready, None))
            .await
            .is_err()
    );
    ready.notify_one();
    ready.notify_one();
    tokio::time::timeout(Duration::from_secs(1), wait(&ready, None))
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(30), wait(&ready, None))
            .await
            .is_err()
    );
    let notify = async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        ready.notify_one();
    };
    tokio::time::timeout(Duration::from_secs(1), async {
        tokio::join!(wait(&ready, None), notify);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn kernel_realtime_timer_fires_once_and_future_deadlines_do_not_poll() {
    tokio::time::timeout(
        Duration::from_secs(1),
        wall_deadline(Utc::now().timestamp() - 1),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        tokio::time::timeout(
            Duration::from_millis(30),
            wall_deadline(Utc::now().timestamp() + 3600)
        )
        .await
        .is_err()
    );
    let ready = Notify::new();
    ready.notify_one();
    tokio::time::timeout(
        Duration::from_secs(1),
        wait(&ready, Some(Utc::now().timestamp() + 3600)),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn cross_process_atomic_replacement_wakes_a_disabled_worker() {
    let path = fixture();
    let ready = Arc::new(Notify::new());
    let mut watch = StateWatch::new(path.to_path_buf(), ready.clone());
    assert!(watch.refresh());
    // Use the real state writer in a subprocess, without touching user state.
    let result=std::process::Command::new("/usr/bin/python3").args(["-c",
        "import pathlib,sys; p=pathlib.Path(sys.argv[1]); t=p.with_suffix('.tmp'); t.write_text('{}'); t.replace(p)"])
        .arg(path.as_os_str()).status().unwrap();
    assert!(result.success());
    tokio::time::timeout(Duration::from_secs(2), wait(&ready, None))
        .await
        .unwrap();
    // Let any remaining create/rename notifications drain before the read test.
    tokio::time::sleep(Duration::from_millis(30)).await;
    while ready.notified().now_or_never().is_some() {}
    std::fs::read(&path).unwrap();
    std::fs::write(path.with_extension("unrelated"), "other").unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(80), wait(&ready, None))
            .await
            .is_err()
    );
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[tokio::test]
async fn watch_follows_missing_and_recreated_state_directories() {
    let folder = fixture();
    let root = folder.root().to_path_buf();
    let path = root.join("new-state/usage.json");
    let ready = Arc::new(Notify::new());
    let mut watch = StateWatch::new(path.clone(), ready.clone());
    assert!(watch.refresh());
    std::fs::create_dir(path.parent().unwrap()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), wait(&ready, None))
        .await
        .unwrap();
    assert!(watch.refresh());
    assert_eq!(watch.directory.as_deref(), path.parent());
    std::fs::remove_dir(path.parent().unwrap()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), wait(&ready, None))
        .await
        .unwrap();
    assert!(watch.refresh());
    assert_eq!(watch.directory.as_deref(), Some(root.as_path()));
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn watch_rebinds_after_directory_replacement_at_the_same_path() {
    let folder = fixture();
    let root = folder.root().to_path_buf();
    let path = root.join("state/usage.json");
    std::fs::create_dir(path.parent().unwrap()).unwrap();
    let ready = Arc::new(Notify::new());
    let mut watch = StateWatch::new(path.clone(), ready.clone());
    assert!(watch.refresh());
    std::fs::remove_dir(path.parent().unwrap()).unwrap();
    std::fs::create_dir(path.parent().unwrap()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), wait(&ready, None))
        .await
        .unwrap();
    assert!(watch.refresh());
    tokio::time::sleep(Duration::from_millis(30)).await;
    while ready.notified().now_or_never().is_some() {}
    std::fs::write(&path, "replacement").unwrap();
    tokio::time::timeout(Duration::from_secs(2), wait(&ready, None))
        .await
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_progress_uses_error_backoff_but_healthy_disabled_state_has_no_deadline() {
    let path = fixture();
    super::super::change(&path, false, now()).unwrap();
    let mut s = super::super::load(&path).unwrap();
    s.pending = None;
    super::super::save(&path, &s).unwrap();
    assert_eq!(next(&path, true, Duration::ZERO, true), None);
    let at = next(&path, true, Duration::ZERO, false).unwrap();
    assert!((Utc::now().timestamp() + ERROR_BACKOFF - at).abs() <= 1);
    s.enabled = true;
    s.revision = 0;
    s.changed_at = None;
    s.opted_out_at = None;
    super::super::save(&path, &s).unwrap();
    let at = next(&path, true, Duration::ZERO, true).unwrap();
    assert!((Utc::now().timestamp() + ERROR_BACKOFF - at).abs() <= 1);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

use cosmic::iced::futures::FutureExt;
