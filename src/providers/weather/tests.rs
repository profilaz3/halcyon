use super::*;
use chrono::Local;
use std::sync::atomic::{AtomicUsize, Ordering};

fn key(name: &str) -> Key {
    Key::new(&Location {
        name: name.into(),
        latitude: 45.,
        longitude: 16.,
        timezone: None,
    })
    .unwrap()
}
fn demand(id: u64, key: &Key) -> Demand {
    Demand {
        consumer: id,
        key: key.clone(),
        interval: Duration::from_secs(900),
    }
}
fn reading(key: &Key) -> WeatherSnapshot {
    WeatherSnapshot { location: key.location(), updated_at: Local::now().to_rfc3339(), weather: serde_json::from_value(serde_json::json!({
        "current":{"temperature_2m":21.,"apparent_temperature":20.,"weather_code":2,"wind_speed_10m":8.,"relative_humidity_2m":52.,"is_day":1},
        "daily":{"time":[],"temperature_2m_max":[],"temperature_2m_min":[],"weather_code":[],"precipitation_probability_max":[]}
    })).unwrap() }
}
fn hub() -> Hub {
    Hub::new(Context::Live)
}

#[tokio::test]
async fn fb07_same_key_consumers_run_one_fetch_and_share_the_observation() {
    let mut hub = hub();
    let key = key("Zagreb");
    let now = Instant::now();
    hub.reconcile(&[demand(1, &key), demand(2, &key)], now)
        .unwrap();
    let mut requests = hub.start_due(now);
    assert_eq!(requests.len(), 1);
    assert!(hub.start_due(now).is_empty());
    let count = AtomicUsize::new(0);
    let done = requests
        .pop()
        .unwrap()
        .execute(
            async {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(reading(&key))
            },
            DEADLINE,
        )
        .await;
    let accepted = hub.complete(done, now, Local::now()).unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(accepted.state, State::Ready);
    let other = hub.snapshot(&key, Local::now());
    assert!(Arc::ptr_eq(
        accepted.data.as_ref().unwrap(),
        other.data.as_ref().unwrap()
    ));
    assert_eq!(other.revision, accepted.revision);
    assert!(!other.refreshing);
    hub.reconcile(&[demand(2, &key)], now).unwrap();
    assert_eq!(hub.entries[&key].consumers.len(), 1);
    assert!(hub.start_due(now).is_empty());
}

#[tokio::test]
async fn fb07_different_coordinates_and_labels_are_separate_bindings() {
    let mut hub = hub();
    let a = key("City");
    let mut location = a.location();
    location.longitude += 1.;
    let b = Key::new(&location).unwrap();
    let c = key("Renamed city");
    let now = Instant::now();
    hub.reconcile(&[demand(0, &a), demand(1, &b), demand(2, &c)], now)
        .unwrap();
    let jobs = hub.start_due(now);
    assert_eq!(jobs.len(), 3);
    for job in jobs {
        let k = job.token.key.clone();
        let mut data = reading(&k);
        data.weather.current.temperature_2m = if k == a { 10. } else { 20. };
        let done = job.execute(async { Ok(data) }, DEADLINE).await;
        hub.complete(done, now, Local::now()).unwrap();
    }
    assert_eq!(
        hub.snapshot(&a, Local::now())
            .data
            .unwrap()
            .weather
            .current
            .temperature_2m,
        10.
    );
    assert_eq!(
        hub.snapshot(&b, Local::now())
            .data
            .unwrap()
            .weather
            .current
            .temperature_2m,
        20.
    );
    assert_ne!(a, b);
    assert_ne!(a, c);
}

#[tokio::test]
async fn fb07_release_cancels_a_started_future_and_rebind_waits_for_ack() {
    let mut hub = hub();
    let key = key("A");
    let now = Instant::now();
    hub.reconcile(&[demand(1, &key)], now).unwrap();
    let request = hub.start_due(now).pop().unwrap();
    let dropped = Arc::new(AtomicUsize::new(0));
    struct Guard(Arc<AtomicUsize>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let (started, ready) = tokio::sync::oneshot::channel();
    let observed = dropped.clone();
    let job = tokio::spawn(request.execute(
        async move {
            let _guard = Guard(observed);
            started.send(()).unwrap();
            std::future::pending().await
        },
        DEADLINE,
    ));
    ready.await.unwrap(); // The exact worker has started; no fixed sleep.
    hub.reconcile(&[], now).unwrap();
    hub.reconcile(&[demand(2, &key)], now).unwrap();
    assert!(hub.start_due(now).is_empty());
    let done = tokio::time::timeout(Duration::from_millis(100), job)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert!(done.result.is_none());
    assert!(hub.complete(done, now, Local::now()).is_none());
    assert_eq!(hub.start_due(now).len(), 1);
}

#[test]
fn fb07_a_b_a_rejects_old_success_and_error_without_clearing_the_new_flight() {
    let mut hub = hub();
    let a = key("A");
    let b = key("B");
    let now = Instant::now();
    hub.reconcile(&[demand(0, &a)], now).unwrap();
    let old = hub.start_due(now).pop().unwrap();
    let token = old.token.clone();
    hub.reconcile(&[demand(0, &b)], now).unwrap();
    hub.reconcile(&[demand(0, &a)], now).unwrap();
    assert!(
        hub.complete(
            Completion {
                token: token.clone(),
                result: Some(Ok(reading(&a)))
            },
            now,
            Local::now()
        )
        .is_none()
    );
    let new = hub.start_due(now).pop().unwrap();
    assert!(
        hub.complete(
            Completion {
                token,
                result: Some(Err("late failure".into()))
            },
            now,
            Local::now()
        )
        .is_none()
    );
    assert_eq!(hub.token_for_test(&a), Some(new.token));
    assert!(!hub.snapshot(&a, Local::now()).failed);
}

#[tokio::test]
async fn fb07_timeout_releases_the_slot_and_schedules_a_bounded_retry() {
    let mut hub = hub();
    let key = key("A");
    let now = Instant::now();
    hub.reconcile(&[demand(0, &key)], now).unwrap();
    let job = hub.start_due(now).pop().unwrap();
    let done = job
        .execute(std::future::pending(), Duration::from_millis(5))
        .await;
    let state = hub.complete(done, now, Local::now()).unwrap();
    assert_eq!(state.state, State::Unavailable);
    assert_eq!(hub.deadline(), Some(now + RETRY));
    assert!(hub.start_due(now).is_empty());
    assert_eq!(hub.start_due(now + RETRY).len(), 1);
}

#[test]
fn fb07_snapshot_states_retain_age_failure_and_binding_revision() {
    let mut hub = hub();
    let key = key("A");
    let now = Instant::now();
    let wall = Local::now();
    assert_eq!(hub.snapshot(&key, wall).state, State::Disconnected);
    hub.reconcile(&[demand(0, &key)], now).unwrap();
    assert_eq!(hub.snapshot(&key, wall).state, State::Loading);
    let mut data = reading(&key);
    data.updated_at = (wall - chrono::Duration::days(2)).to_rfc3339();
    hub.seed(data);
    assert_eq!(hub.snapshot(&key, wall).state, State::Stale);
    let job = hub.start_due(now).pop().unwrap();
    let s = hub
        .complete(
            Completion {
                token: job.token,
                result: Some(Err("offline".into())),
            },
            now,
            wall,
        )
        .unwrap();
    assert!(s.failed && s.data.is_some());
    assert_eq!(s.state, State::Stale);
    assert!(s.revision > 0);
    hub.reconcile(&[], now).unwrap();
    assert_eq!(hub.snapshot(&key, wall).state, State::Disconnected);
}

#[test]
fn fb07_preview_demand_starts_no_jobs_and_keeps_no_readings() {
    let mut hub = Hub::new(Context::LocalPreview);
    let key = key("A");
    let now = Instant::now();
    hub.reconcile(&[demand(0, &key), demand(1, &key)], now)
        .unwrap();
    hub.seed(reading(&key));
    assert!(hub.start_due(now).is_empty());
    assert_eq!(hub.deadline(), None);
    assert!(hub.entries.is_empty());
}

#[test]
fn fb07_concurrency_capacity_and_earliest_due_queue_are_bounded() {
    let mut hub = hub();
    let now = Instant::now();
    let demands: Vec<_> = (0..MAX_KEYS)
        .map(|n| demand(n as u64, &key(&format!("{n:02}"))))
        .collect();
    hub.reconcile(&demands, now).unwrap();
    let jobs = hub.start_due(now);
    assert_eq!(jobs.len(), MAX_JOBS);
    assert_eq!(hub.deadline(), None);
    assert!(hub.start_due(now).is_empty());
    let mut excessive = demands.clone();
    excessive.push(demand(40, &key("extra")));
    assert!(hub.reconcile(&excessive, now).is_err());
    assert_eq!(hub.entries.len(), MAX_KEYS);
    for job in jobs {
        hub.complete(
            Completion {
                token: job.token,
                result: Some(Err("fixture".into())),
            },
            now,
            Local::now(),
        )
        .unwrap();
    }
    assert_eq!(hub.start_due(now).len(), MAX_JOBS);
    assert_eq!(hub.entries.len(), MAX_KEYS);
    hub.reconcile(&[], now).unwrap();
    assert!(hub.entries.len() <= MAX_JOBS);
}

#[test]
fn fb07_cadence_changes_share_current_work_and_resume_moves_deadlines() {
    let mut hub = hub();
    let key = key("A");
    let now = Instant::now();
    let mut fast = demand(2, &key);
    fast.interval = Duration::from_secs(60);
    hub.reconcile(&[demand(1, &key), fast.clone()], now)
        .unwrap();
    let job = hub.start_due(now).pop().unwrap();
    fast.interval = Duration::from_secs(30);
    hub.reconcile(&[demand(1, &key), fast], now).unwrap();
    assert!(hub.start_due(now).is_empty());
    hub.complete(
        Completion {
            token: job.token,
            result: Some(Ok(reading(&key))),
        },
        now,
        Local::now(),
    )
    .unwrap();
    assert_eq!(hub.deadline(), Some(now + Duration::from_secs(30)));
    hub.resumed(Duration::from_secs(3600), now);
    assert_eq!(hub.start_due(now).len(), 1);
}

#[test]
fn fb07_invalid_sources_never_become_ready() {
    let mut location = key("A").location();
    for n in [f64::NAN, f64::INFINITY, 91.] {
        location.latitude = n;
        assert!(Key::new(&location).is_none());
    }
    let mut hub = hub();
    let key = key("A");
    let now = Instant::now();
    hub.reconcile(&[demand(0, &key)], now).unwrap();
    let job = hub.start_due(now).pop().unwrap();
    let s = hub
        .complete(
            Completion {
                token: job.token,
                result: Some(Ok(reading(&super::tests::key("B")))),
            },
            now,
            Local::now(),
        )
        .unwrap();
    assert_eq!(s.state, State::Unavailable);
    assert!(s.data.is_none());
}

#[test]
fn fb07_cache_worker_order_never_replaces_a_newer_accepted_observation() {
    let root = std::env::temp_dir().join(format!("halcyon-fb07-cache-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("weather.json");
    let cache = Cache::default();
    let a = key("A");
    let b = key("B");
    let writer = cache
        .prepare_at(Arc::new(reading(&a)), Some(path.clone()))
        .unwrap();
    assert!(
        cache
            .prepare_at(Arc::new(reading(&b)), Some(path.clone()))
            .is_none()
    );
    writer.save().unwrap();
    let saved: WeatherSnapshot = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved.location, b.location());
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fb07_blocked_cache_writer_coalesces_pending_values_without_blocking_producers() {
    let cache = Cache::default();
    let a = key("A");
    let b = key("B");
    let c = key("C");
    let path = Some(std::path::PathBuf::from("unused-fixture-path"));
    let writer = cache
        .prepare_at(Arc::new(reading(&a)), path.clone())
        .unwrap();
    let (started, ready) = std::sync::mpsc::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut names = Vec::new();
        writer
            .save_with(|data, _| {
                names.push(data.location.name.clone());
                if names.len() == 1 {
                    started.send(()).unwrap();
                    wait.recv_timeout(Duration::from_secs(2)).unwrap();
                }
                Ok(())
            })
            .unwrap();
        names
    });
    ready.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(
        cache
            .prepare_at(Arc::new(reading(&b)), path.clone())
            .is_none()
    );
    assert!(
        cache
            .prepare_at(Arc::new(reading(&c)), path.clone())
            .is_none()
    );
    release.send(()).unwrap();
    assert_eq!(worker.join().unwrap(), ["A", "C"]);
    // A task dropped before it started releases its lease; later accepted data
    // can start a new worker, without retaining a stuck ownership flag.
    drop(
        cache
            .prepare_at(Arc::new(reading(&a)), path.clone())
            .unwrap(),
    );
    let next = cache.prepare_at(Arc::new(reading(&b)), path).unwrap();
    next.save_with(|data, _| {
        assert_eq!(data.location.name, "B");
        Ok(())
    })
    .unwrap();
}

#[tokio::test]
async fn fb07_real_http_transport_shares_demand_and_bounds_streamed_responses() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let key = key("HTTP fixture");
    let body = serde_json::to_vec(&reading(&key).weather).unwrap();
    for mode in 0..3 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/forecast", listener.local_addr().unwrap());
        let body = body.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 8192];
            let mut n = 0;
            while !request[..n].windows(4).any(|w| w == b"\r\n\r\n") {
                assert!(n < request.len());
                let read = stream.read(&mut request[n..]).await.unwrap();
                assert!(read > 0);
                n += read;
            }
            let text = String::from_utf8_lossy(&request[..n]);
            assert!(
                text.contains("latitude=45")
                    && text.contains("longitude=16")
                    && text.contains("forecast_days=3")
            );
            let response=match mode {
                0=>[format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).into_bytes(),body].concat(),
                1=>b"HTTP/1.1 200 OK\r\nContent-Length: 999999\r\nConnection: close\r\n\r\n".to_vec(),
                _=>[b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n40001\r\n".to_vec(),vec![b' ';256*1024+1],b"\r\n0\r\n\r\n".to_vec()].concat(),
            };
            let _ = stream.write_all(&response).await;
            // Stop after the one actual transport; extra demand cannot create a second.
        });
        let mut hub = hub();
        let now = Instant::now();
        hub.reconcile(&[demand(1, &key), demand(2, &key)], now)
            .unwrap();
        let mut jobs = hub.start_due(now);
        assert_eq!(jobs.len(), 1);
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let location = key.location();
        let done = jobs
            .pop()
            .unwrap()
            .execute(
                crate::data::test_fetch_weather(&client, &endpoint, &location),
                Duration::from_secs(2),
            )
            .await;
        if mode == 0 {
            assert!(done.result.as_ref().unwrap().is_ok());
        } else {
            assert!(
                done.result
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .unwrap_err()
                    .contains("too large")
            );
        }
        let accepted = hub.complete(done, now, Local::now()).unwrap();
        assert_eq!(
            accepted.state,
            if mode == 0 {
                State::Ready
            } else {
                State::Unavailable
            }
        );
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn fb07_dropping_provider_owner_cancels_its_started_work() {
    let mut hub = hub();
    let key = key("A");
    let now = Instant::now();
    hub.reconcile(&[demand(0, &key)], now).unwrap();
    let request = hub.start_due(now).pop().unwrap();
    let (send, ready) = tokio::sync::oneshot::channel();
    let job = tokio::spawn(request.execute(
        async move {
            send.send(()).unwrap();
            std::future::pending().await
        },
        DEADLINE,
    ));
    ready.await.unwrap();
    drop(hub);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), job)
            .await
            .unwrap()
            .unwrap()
            .result
            .is_none()
    );
}
