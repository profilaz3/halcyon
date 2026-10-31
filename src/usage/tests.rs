use super::*;
use std::os::unix::fs::PermissionsExt;
fn fixture() -> PathBuf {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).unwrap();
    let p = std::env::temp_dir().join(format!("halcyon-usage-{:x}", u64::from_le_bytes(bytes)));
    std::fs::create_dir_all(&p).unwrap();
    p.join("usage.json")
}
#[test]
fn first_report_persists_a_random_private_identity_and_reserves_retry() {
    let path = fixture();
    let token = claim(&path, "2026-10-23", 1000)
        .unwrap()
        .expect("first daily report")
        .0;
    assert_eq!(token.len(), 64);
    assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(claim(&path, "2026-10-23", 1001).unwrap().is_none());
    assert_eq!(claim(&path, "2026-10-23", 22600).unwrap(), Some((token, 0)));
}
#[test]
fn success_survives_restart_and_next_day_keeps_the_same_identity() {
    let path = fixture();
    let token = claim(&path, "2026-10-23", 1000).unwrap().unwrap().0;
    complete(&path, &token, 0, "2026-10-23").unwrap();
    assert!(claim(&path, "2026-10-23", 24000).unwrap().is_none());
    assert_eq!(claim(&path, "2026-10-24", 90000).unwrap(), Some((token, 0)));
}
#[test]
fn concurrent_claims_only_schedule_one_request() {
    let path = fixture();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let path = path.clone();
            let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                claim(&path, "2026-10-23", 1000).ok().flatten()
            })
        })
        .collect();
    assert_eq!(
        threads
            .into_iter()
            .filter_map(|t| t.join().unwrap())
            .count(),
        1
    );
}
#[test]
fn corrupted_state_does_not_silently_register_another_installation() {
    let path = fixture();
    std::fs::write(&path, "broken").unwrap();
    assert!(claim(&path, "2026-10-23", 1000).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "broken");
}
#[test]
fn late_response_cannot_replace_a_new_identity() {
    let path = fixture();
    let token = claim(&path, "2026-10-23", 1000).unwrap().unwrap().0;
    complete(&path, &"a".repeat(64), 0, "2026-10-23").unwrap();
    let state: State = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(state.installation, token);
    assert!(state.last_success.is_none());
}
#[test]
fn endpoint_is_limited_to_a_production_google_deployment() {
    assert!(endpoint("https://script.google.com/macros/s/ABC_123/exec").is_some());
    for value in [
        "",
        "https://script.google.com/macros/s//exec",
        "https://script.google.com/macros/s/%20/exec",
        "http://script.google.com/macros/s/ABC/exec",
        "https://evil.test/macros/s/ABC/exec",
        "https://user:secret@script.google.com/macros/s/ABC/exec",
        "https://script.google.com/macros/s/ABC/dev",
        "https://script.google.com/macros/s/ABC/exec?account=private",
        "https://script.google.com:123/macros/s/ABC/exec",
    ] {
        assert!(endpoint(value).is_none(), "{value}");
    }
}
#[test]
fn report_contains_only_the_documented_fields() {
    let report = serde_json::to_value(Report {
        schema: 5,
        kind: "activity",
        revision: 0,
        installation: &"a".repeat(64),
        version: "0.2.0",
        country: Some("HR"),
        first_started_at: Some(1000),
        installation_info: crate::installation::Observation::default(),
    })
    .unwrap();
    assert_eq!(report.as_object().unwrap().len(), 10);
    assert_eq!(report["schema"], 5);
    assert_eq!(report["version"], "0.2.0");
    assert_eq!(report["country"], "HR");
    assert!(report.get("installation").is_some());
    assert!(
        !available(),
        "test builds must never start reporting automatically"
    );
}
async fn reply(response: &'static str) -> Result<String, String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = [0u8; 4096];
        let _ = stream.read(&mut request);
        stream.write_all(response.as_bytes()).unwrap();
    });
    let result = send(
        &client().unwrap(),
        reqwest::Url::parse(&format!("http://{address}/")).unwrap(),
        &"a".repeat(64),
        0,
        None,
        Some(1000),
    )
    .await;
    server.join().unwrap();
    result
}
#[tokio::test]
async fn only_an_explicit_success_with_a_valid_day_completes_a_report() {
    assert_eq!(reply("HTTP/1.1 200 OK\r\nContent-Length: 30\r\nConnection: close\r\n\r\n{\"ok\":true,\"day\":\"2026-10-23\"}").await.unwrap(),"2026-10-23");
    assert!(
        reply("HTTP/1.1 200 OK\r\nContent-Length: 12\r\nConnection: close\r\n\r\n{\"ok\":false}")
            .await
            .is_err()
    );
    assert!(
        reply("HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\n<html")
            .await
            .is_err()
    );
    assert!(reply("HTTP/1.1 302 Found\r\nLocation: https://example.com/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.is_err());
}

#[test]
fn country_state_round_trips_without_replacing_the_existing_identity() {
    let path = fixture();
    let id = claim(&path, "2026-10-23", 1000).unwrap().unwrap().0;
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    json["country"] = serde_json::json!("HR");
    std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    let state = load(&path).expect("country-compatible state");
    assert_eq!(state.installation, id);
    assert_eq!(serde_json::to_value(state).unwrap()["country"], "HR");
}

#[test]
fn legacy_state_migrates_and_a_late_lookup_cannot_change_identity_or_first_country() {
    let path = fixture();
    let id = "b".repeat(64);
    std::fs::write(
        &path,
        format!(r#"{{"installation":"{id}","last_success":null,"retry_after":0}}"#),
    )
    .unwrap();
    assert_eq!(
        claim(&path, "2026-10-23", 1000)
            .unwrap()
            .map(|t| t.0)
            .as_deref(),
        Some(id.as_str())
    );
    assert_eq!(
        remember_country(&path, &id, 0, "HR").unwrap().as_deref(),
        Some("HR")
    );
    assert_eq!(
        remember_country(&path, &id, 0, "US").unwrap().as_deref(),
        Some("HR")
    );
    assert_eq!(
        remember_country(&path, &"c".repeat(64), 0, "DE").unwrap(),
        None
    );
    assert_eq!(load(&path).unwrap().country.as_deref(), Some("HR"));
}

// A local HTTP boundary exercises the real client, without sending activity or IPs online.
fn local_server(status: &str, body: &str) -> (String, std::thread::JoinHandle<String>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let task = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let (mut stream, _) = loop {
            if let Ok(stream) = listener.accept() {
                break stream;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "local client never connected"
            );
            std::thread::sleep(Duration::from_millis(5));
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = Vec::new();
        loop {
            let mut buffer = [0; 4096];
            let n = stream.read(&mut buffer).unwrap();
            assert!(n > 0, "incomplete request");
            request.extend_from_slice(&buffer[..n]);
            let text = String::from_utf8_lossy(&request);
            if let Some(end) = text.find("\r\n\r\n") {
                let length: usize = text[..end]
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                if request.len() >= end + 4 + length {
                    break;
                }
            }
        }
        stream.write_all(response.as_bytes()).unwrap();
        String::from_utf8(request).unwrap()
    });
    (url, task)
}

#[tokio::test]
async fn country_lookup_discards_ip_caches_first_country_and_never_sends_identity() {
    let path = fixture();
    let id = claim(&path, "2026-10-23", 1000).unwrap().unwrap().0;
    let (url, request) = local_server("200 OK", r#"{"ip":"192.0.2.1","country":"HR"}"#);
    assert_eq!(
        country_for(&path, &id, 0, &url).await.as_deref(),
        Some("HR")
    );
    let request = request.join().unwrap();
    assert!(request.starts_with("GET / HTTP/1.1"));
    assert!(!request.contains(&id));
    assert!(
        !std::fs::read_to_string(&path)
            .unwrap()
            .contains("192.0.2.1")
    );
    // The server has gone away; this must use the persisted cache after a restart.
    assert_eq!(
        country_for(&path, &id, 0, &url).await.as_deref(),
        Some("HR")
    );
}

#[tokio::test]
async fn failed_country_lookup_still_sends_activity_with_unknown_country() {
    let path = fixture();
    record_start(&path, 900).unwrap();
    let id = claim(&path, "2026-10-23", 1000).unwrap().unwrap().0;
    let (country_url, country_request) = local_server("503 Unavailable", "unavailable");
    let (report_url, report_request) = local_server("200 OK", r#"{"ok":true,"day":"2026-10-23"}"#);
    assert_eq!(
        report_once(
            &path,
            &client().unwrap(),
            reqwest::Url::parse(&report_url).unwrap(),
            &id,
            0,
            &country_url
        )
        .await
        .unwrap(),
        "2026-10-23"
    );
    country_request.join().unwrap();
    let request = report_request.join().unwrap();
    let report: serde_json::Value =
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(report.as_object().unwrap().len(), 10);
    assert_eq!(report["schema"], 5);
    assert_eq!(report["installation"], id);
    assert_eq!(report["first_started_at"], 900);
    assert!(report["country"].is_null());
    assert!(load(&path).unwrap().country.is_none());
    // Failure was not cached: a later report can acquire a country.
    let (url, request) = local_server("200 OK", r#"{"country":"DE"}"#);
    assert_eq!(
        country_for(&path, &id, 0, &url).await.as_deref(),
        Some("DE")
    );
    request.join().unwrap();
}

#[tokio::test]
async fn invalid_or_oversized_country_responses_never_enter_state() {
    for body in [
        r#"{"country":"hr"}"#.to_string(),
        r#"{"country":null}"#.to_string(),
        "<html>".into(),
        format!(r#"{{"country":"US","extra":"{}"}}"#, "x".repeat(1024)),
    ] {
        let path = fixture();
        let id = claim(&path, "2026-10-23", 1000).unwrap().unwrap().0;
        let (url, request) = local_server("200 OK", &body);
        assert!(country_for(&path, &id, 0, &url).await.is_none());
        request.join().unwrap();
        assert!(load(&path).unwrap().country.is_none());
    }
}

#[test]
fn choice_is_durable_minimal_and_latest_only_with_monotonic_revision() {
    let path = fixture();
    assert!(load(&path).unwrap().enabled);
    change(&path, false, 1000).unwrap();
    let first = load(&path).unwrap();
    assert!(!first.enabled);
    assert!(valid_token(&first.installation));
    assert!(first.country.is_none());
    assert!(first.last_success.is_none());
    assert_eq!(first.revision, 1);
    assert!(claim(&path, "2026-10-23", 1001).unwrap().is_none());
    change(&path, false, 1001).unwrap();
    assert_eq!(load(&path).unwrap().revision, 1);
    change(&path, true, 900).unwrap(); // clock skew cannot reorder preference
    let enabled = load(&path).unwrap();
    assert_eq!(enabled.revision, 2);
    assert_eq!(enabled.opted_out_at, Some(1000));
    assert_eq!(enabled.installation, first.installation);
    assert_eq!(enabled.pending.unwrap().attempts, 0);
    change(&path, false, 1002).unwrap();
    assert_eq!(load(&path).unwrap().revision, 3);
    let state = claim_status(&path, 1002).unwrap().unwrap();
    let body = serde_json::to_value(StatusReport {
        schema: 3,
        kind: "status",
        installation: &state.installation,
        revision: state.revision,
        enabled: state.enabled,
        changed_at: state.changed_at.unwrap(),
    })
    .unwrap();
    assert_eq!(body.as_object().unwrap().len(), 6);
    for forbidden in [
        "country",
        "version",
        "ip",
        "last_success",
        "opted_out_at",
        "first_started_at",
        "package_format",
        "update_channel",
        "installed_app_version",
    ] {
        assert!(body.get(forbidden).is_none());
    }
}

#[test]
fn queue_bounds_survive_restart_expiry_and_clock_rollback_without_reenabling() {
    let path = fixture();
    change(&path, false, 1000).unwrap();
    for attempt in 0..MAX_ATTEMPTS {
        let now = 1000 + i64::from(attempt) * RETRY_SECONDS;
        assert!(claim_status(&path, now).unwrap().is_some());
        assert!(claim_status(&path, now + 1).unwrap().is_none());
    }
    assert!(
        claim_status(&path, 1000 + STATUS_LIFETIME)
            .unwrap()
            .is_none()
    );
    let state = load(&path).unwrap();
    assert!(!state.enabled);
    assert!(state.pending.is_none());
    assert!(
        claim(&path, "2026-10-24", 1000 + STATUS_LIFETIME)
            .unwrap()
            .is_none()
    );
    change(&path, true, 1000 + STATUS_LIFETIME).unwrap();
    change(&path, false, 1001 + STATUS_LIFETIME).unwrap();
    assert!(claim_status(&path, 900).unwrap().is_none());
    assert!(!load(&path).unwrap().enabled);
}

#[test]
fn damaged_new_preferences_fail_closed_and_legacy_identity_migrates() {
    let path = fixture();
    let id = "c".repeat(64);
    std::fs::write(
        &path,
        format!(r#"{{"installation":"{id}","last_success":null,"retry_after":0}}"#),
    )
    .unwrap();
    change(&path, false, 1000).unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert_eq!(load(&path).unwrap().installation, id);
    for key in [
        "state_version",
        "enabled",
        "revision",
        "changed_at",
        "opted_out_at",
        "pending",
    ] {
        let mut value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        value.as_object_mut().unwrap().remove(key);
        let bytes = serde_json::to_vec(&value).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        assert!(load(&path).is_err(), "{key}");
        assert!(change(&path, true, 1001).is_err(), "{key}");
        assert!(claim(&path, "2026-10-23", 1001).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[tokio::test]
async fn disabled_before_warmup_and_first_country_only_sends_status() {
    let path = fixture();
    change(&path, false, 1000).unwrap();
    let country = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    country.set_nonblocking(true).unwrap();
    let (url, request) = local_server("200 OK", r#"{"ok":true,"revision":1,"enabled":false}"#);
    let url = reqwest::Url::parse(&url).unwrap();
    let country_url = format!("http://{}/", country.local_addr().unwrap());
    cycle(
        &path,
        &client().unwrap(),
        url.clone(),
        &country_url,
        false,
        1000,
    )
    .await;
    let sent = request.join().unwrap();
    assert!(sent.contains("\"kind\":\"status\""));
    assert!(!sent.contains("country"));
    assert!(!sent.contains("version"));
    assert!(load(&path).unwrap().pending.is_none());
    for now in [1001, 99999, 999999] {
        cycle(
            &path,
            &client().unwrap(),
            url.clone(),
            &country_url,
            true,
            now,
        )
        .await;
    }
    assert!(country.accept().is_err());
    assert!(load(&path).unwrap().last_success.is_none());
}

#[tokio::test]
async fn offline_status_retries_exact_identity_and_requires_matching_ack() {
    let path = fixture();
    change(&path, false, 1000).unwrap();
    let original = load(&path).unwrap().installation;
    for (attempt, body) in [
        r#"{"ok":false}"#,
        r#"{"ok":true,"day":"2026-10-23"}"#,
        r#"{"ok":true,"revision":2,"enabled":false}"#,
        r#"{"ok":true,"revision":1,"enabled":false}"#,
    ]
    .iter()
    .enumerate()
    {
        let now = 1000 + attempt as i64 * RETRY_SECONDS;
        let state = claim_status(&path, now).unwrap().unwrap();
        let (url, request) = local_server("200 OK", body);
        let result = status_once(
            &path,
            &client().unwrap(),
            reqwest::Url::parse(&url).unwrap(),
            &state,
        )
        .await;
        assert_eq!(result.is_ok(), attempt == 3);
        assert!(request.join().unwrap().contains(&original));
        assert_eq!(load(&path).unwrap().pending.is_some(), attempt != 3);
        assert!(!load(&path).unwrap().enabled);
    }
}

// The server changes the preference after the request has reached it. The client
// must drop the pending response without storing a country or starting activity.
#[tokio::test]
async fn optout_during_country_cancels_socket_and_never_posts_activity() {
    let path = fixture();
    let (id, revision) = claim(&path, "2026-10-23", 1000).unwrap().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let country_url = format!("http://{}/", listener.local_addr().unwrap());
    let server_path = path.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut buf = [0; 4096];
        assert!(stream.read(&mut buf).unwrap() > 0);
        change(&server_path, false, 1001).unwrap(); // proves no network-held file lock
        assert_eq!(
            stream.read(&mut buf).unwrap(),
            0,
            "cancelled connection must close"
        );
    });
    let reports = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    reports.set_nonblocking(true).unwrap();
    let now = std::time::Instant::now();
    assert!(
        report_once(
            &path,
            &client().unwrap(),
            reqwest::Url::parse(&format!("http://{}/", reports.local_addr().unwrap())).unwrap(),
            &id,
            revision,
            &country_url
        )
        .await
        .is_err()
    );
    assert!(now.elapsed() < Duration::from_secs(1));
    tokio::task::spawn_blocking(move || server.join().unwrap())
        .await
        .unwrap();
    assert!(reports.accept().is_err());
    assert!(load(&path).unwrap().country.is_none());
    complete(&path, &id, revision, "2026-10-23").unwrap();
    assert!(load(&path).unwrap().last_success.is_none());
}

#[tokio::test]
async fn old_activity_and_status_completions_cannot_overwrite_a_newer_choice() {
    let path = fixture();
    let (id, rev) = claim(&path, "2026-10-23", 1000).unwrap().unwrap();
    change(&path, false, 1001).unwrap();
    let old_status = claim_status(&path, 1001).unwrap().unwrap();
    change(&path, true, 1002).unwrap();
    assert_eq!(remember_country(&path, &id, rev, "US").unwrap(), None);
    complete(&path, &id, rev, "2026-10-23").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    assert!(
        status_once(&path, &client().unwrap(), url.clone(), &old_status)
            .await
            .is_err()
    );
    assert!(
        report_once(
            &path,
            &client().unwrap(),
            url,
            &id,
            rev,
            "http://127.0.0.1:1/"
        )
        .await
        .is_err()
    );
    assert!(listener.accept().is_err());
    let state = load(&path).unwrap();
    assert!(state.enabled);
    assert_eq!(state.revision, 2);
    assert!(state.pending.is_some());
    assert!(state.last_success.is_none());
}

#[test]
fn multiprocess_preference_child() {
    if let Some(path) = std::env::var_os("HALCYON_USAGE_CHILD") {
        change(Path::new(&path), false, 1001).unwrap();
    }
}
#[test]
fn separate_process_optout_invalidates_an_already_claimed_report() {
    let path = fixture();
    let (id, rev) = claim(&path, "2026-10-23", 1000).unwrap().unwrap();
    assert!(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "usage::tests::multiprocess_preference_child"])
            .env("HALCYON_USAGE_CHILD", &path)
            .status()
            .unwrap()
            .success()
    );
    assert!(!current(&path, &id, rev, true, true));
    assert!(claim(&path, "2026-10-24", 100000).unwrap().is_none());
    assert_eq!(load(&path).unwrap().installation, id);
}

#[tokio::test]
async fn optout_during_activity_cancels_without_marking_success_then_enable_resumes() {
    let path = fixture();
    let (id, rev) = claim(&path, "2026-10-23", 1000).unwrap().unwrap();
    remember_country(&path, &id, rev, "HR").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let server_path = path.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut buffer = [0; 4096];
        assert!(stream.read(&mut buffer).unwrap() > 0);
        change(&server_path, false, 1001).unwrap();
        // Bytes already received cannot be unsent. Only pending response work is cancelled.
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) => panic!("cancel failed: {e}"),
            }
        }
    });
    let started = std::time::Instant::now();
    assert!(
        report_once(
            &path,
            &client().unwrap(),
            url,
            &id,
            rev,
            "http://127.0.0.1:1/"
        )
        .await
        .is_err()
    );
    assert!(started.elapsed() < Duration::from_secs(1));
    tokio::task::spawn_blocking(move || server.join().unwrap())
        .await
        .unwrap();
    complete(&path, &id, rev, "2026-10-23").unwrap();
    assert!(load(&path).unwrap().last_success.is_none());
    change(&path, true, 1002).unwrap();
    let state = claim_status(&path, 1002).unwrap().unwrap();
    let (url, request) = local_server("200 OK", r#"{"ok":true,"revision":2,"enabled":true}"#);
    status_once(
        &path,
        &client().unwrap(),
        reqwest::Url::parse(&url).unwrap(),
        &state,
    )
    .await
    .unwrap();
    request.join().unwrap();
    let (same_id, new_rev) = claim(&path, "2026-10-23", 1003).unwrap().unwrap();
    assert_eq!(same_id, id);
    assert_eq!(new_rev, 2);
    let (url, request) = local_server("200 OK", r#"{"ok":true,"day":"2026-10-23"}"#);
    report_once(
        &path,
        &client().unwrap(),
        reqwest::Url::parse(&url).unwrap(),
        &id,
        new_rev,
        "http://127.0.0.1:1/",
    )
    .await
    .unwrap();
    let request = request.join().unwrap();
    assert!(request.contains("\"revision\":2"));
    assert!(request.contains("\"country\":\"HR\""));
    complete(&path, &id, new_rev, "2026-10-23").unwrap();
    assert_eq!(
        load(&path).unwrap().last_success.as_deref(),
        Some("2026-10-23")
    );
}

#[tokio::test]
async fn status_response_after_a_new_choice_cannot_clear_its_queue() {
    let path = fixture();
    change(&path, false, 1000).unwrap();
    let state = claim_status(&path, 1000).unwrap().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let server_path = path.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut buf = [0; 4096];
        assert!(stream.read(&mut buf).unwrap() > 0);
        change(&server_path, true, 1001).unwrap();
        let body = r#"{"ok":true,"revision":1,"enabled":false}"#;
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    });
    assert!(
        status_once(&path, &client().unwrap(), url, &state)
            .await
            .is_err()
    );
    server.join().unwrap();
    let latest = load(&path).unwrap();
    assert_eq!(latest.revision, 2);
    assert!(latest.enabled);
    assert!(latest.pending.is_some());
}

#[test]
fn first_start_is_local_stable_and_never_inferred_for_existing_installations() {
    let path = fixture();
    record_start(&path, 1000).unwrap();
    let first = load(&path).unwrap();
    assert_eq!(first.first_started_at, Some(1000));
    assert!(first.last_success.is_none());
    record_start(&path, 2000).unwrap();
    assert_eq!(load(&path).unwrap().first_started_at, Some(1000));
    assert_eq!(load(&path).unwrap().installation, first.installation);
    let legacy = fixture();
    claim(&legacy, "2026-10-23", 1000).unwrap();
    let mut old: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&legacy).unwrap()).unwrap();
    old.as_object_mut().unwrap().remove("first_started_at");
    std::fs::write(&legacy, serde_json::to_vec(&old).unwrap()).unwrap();
    record_start(&legacy, 2000).unwrap();
    assert_eq!(load(&legacy).unwrap().first_started_at, None);
}
#[test]
fn disabled_and_corrupt_start_state_never_acquires_a_timestamp() {
    let path = fixture();
    change(&path, false, 1000).unwrap();
    let before = std::fs::read(&path).unwrap();
    record_start(&path, 2000).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let mut value: serde_json::Value = serde_json::from_slice(&before).unwrap();
    value["first_started_at"] = serde_json::json!(-1);
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(load(&path).is_err());
    assert!(record_start(&path, 2000).is_err());
}

#[tokio::test]
async fn short_daily_sessions_deliver_after_warmup_and_survive_restarts() {
    let path = fixture();
    let boot = 1_792_843_200; // 2026-10-24 12:00 UTC
    record_start(&path, boot).unwrap();
    let mut state = load(&path).unwrap();
    state.country = Some("HR".into());
    let id = state.installation.clone();
    save(&path, &state).unwrap();
    let client = client().unwrap();
    for (offset, day) in [(0, "2026-10-24"), (86400, "2026-10-25")] {
        let started = boot + offset;
        record_start(&path, started).unwrap();
        let (url, request) = local_server("200 OK", &format!(r#"{{"ok":true,"day":"{day}"}}"#));
        let url = reqwest::Url::parse(&url).unwrap();
        // Production's warmup gate suppresses activity before five minutes.
        cycle(
            &path,
            &client,
            url.clone(),
            "http://127.0.0.1:1",
            false,
            started + 299,
        )
        .await;
        assert_ne!(load(&path).unwrap().last_success.as_deref(), Some(day));
        cycle(
            &path,
            &client,
            url.clone(),
            "http://127.0.0.1:1",
            true,
            started + 300,
        )
        .await;
        let request = request.join().unwrap();
        let sent: serde_json::Value =
            serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(sent["installation"], id);
        assert_eq!(sent["first_started_at"], boot);
        assert_eq!(load(&path).unwrap().last_success.as_deref(), Some(day));
        // One-hour shutdown, then a second boot: persisted state suppresses duplicates.
        record_start(&path, started + 7200).unwrap();
        cycle(
            &path,
            &client,
            url,
            "http://127.0.0.1:1",
            true,
            started + 7500,
        )
        .await;
        assert_eq!(load(&path).unwrap().retry_after, 0);
    }
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[tokio::test]
async fn failed_short_session_stays_pending_and_next_day_delivers_without_backfill() {
    let path = fixture();
    let boot = 1_792_843_200;
    record_start(&path, boot).unwrap();
    let mut state = load(&path).unwrap();
    state.country = Some("HR".into());
    save(&path, &state).unwrap();
    let client = client().unwrap();
    let (url, request) = local_server("503 Unavailable", "offline");
    cycle(
        &path,
        &client,
        reqwest::Url::parse(&url).unwrap(),
        "http://127.0.0.1:1",
        true,
        boot + 300,
    )
    .await;
    request.join().unwrap();
    let state = load(&path).unwrap();
    assert!(state.last_success.is_none());
    assert_eq!(state.retry_after, boot + 300 + RETRY_SECONDS);
    // Rebooting inside the retry window must not cause a request storm.
    record_start(&path, boot + 3600).unwrap();
    assert!(claim(&path, "2026-10-24", boot + 3900).unwrap().is_none());
    let (url, request) = local_server("200 OK", r#"{"ok":true,"day":"2026-10-25"}"#);
    cycle(
        &path,
        &client,
        reqwest::Url::parse(&url).unwrap(),
        "http://127.0.0.1:1",
        true,
        boot + 86400 + 300,
    )
    .await;
    request.join().unwrap();
    assert_eq!(
        load(&path).unwrap().last_success.as_deref(),
        Some("2026-10-25")
    );
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
