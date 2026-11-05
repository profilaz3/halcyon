use super::*;
use tokio::net::UnixListener;

fn reply(enabled: bool, output: &str) -> Vec<u8> {
    let value = serde_json::json!({"Draft": {
        "token":0,"revision":0,"settings":dock_islands_core::Settings { enabled, ..Default::default() },
        "layout": {"spans":[null,null,null],"gap":14.,"bounds":{"start":0.,"length":1000.},
        "group":{"start":0.,"length":0.},"budgets":[0.,0.,0.],"natural":[0.,0.,0.]},
        "view":{"output":output,"selected":null,"visible":true,"popups":0,"overflow":[0,0,0],
        "horizontal":true,"origin":[0.,0.],"scale":1.,"handles":[]}
    }});
    let mut data = serde_json::to_vec(&value).unwrap();
    data.push(b'\n');
    data
}
async fn probe(data: Vec<u8>, wrong_host: bool) -> bool {
    let dir = Temp::new();
    let path = dir.path().join("host.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut line = String::new();
        let mut reader = BufReader::new(&mut stream);
        if reader.read_line(&mut line).await.unwrap_or(0) > 0 {
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&line).unwrap(),
                serde_json::json!({"Inspect":{"panel":"Panel","output":"screen-A"}})
            );
            let _ = stream.write_all(&data).await;
        }
    });
    let enabled = Host {
        height: 56,
        socket: Some(path),
        config: None,
        panel: true,
        output: "screen-A".into(),
        pid: if wrong_host {
            u32::MAX
        } else {
            std::process::id()
        },
    }
    .enabled()
    .await;
    server.await.unwrap();
    enabled
}
#[tokio::test]
async fn follows_own_panels_observed_mode_and_output() {
    assert!(probe(reply(true, "screen-A"), false).await);
    assert!(!probe(reply(false, "screen-A"), false).await);
    assert!(!probe(reply(true, "screen-B"), false).await);
    assert!(!probe(reply(true, "screen-A"), true).await);
}
#[tokio::test]
async fn missing_malformed_oversized_or_partial_host_replies_stay_hidden() {
    assert!(!probe(b"{\"Error\":\"not active\"}\n".to_vec(), false).await);
    assert!(!probe(vec![b'x'; MAX_WIRE_BYTES + 1], false).await);
    let mut partial = reply(true, "screen-A");
    partial.pop();
    assert!(!probe(partial, false).await);
    let h = Host {
        socket: Some(PathBuf::from("/no-such-activity-host/socket")),
        config: None,
        panel: true,
        output: "screen-A".into(),
        pid: std::process::id(),
        height: 56,
    };
    assert!(!h.enabled().await);
}
#[tokio::test]
async fn host_observation_has_one_bounded_deadline() {
    let dir = Temp::new();
    let path = dir.path().join("host.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        loop {
            if stream.write_all(b" ").await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
    let h = Host {
        socket: Some(path),
        config: None,
        panel: true,
        output: "screen-A".into(),
        pid: std::process::id(),
        height: 56,
    };
    let started = std::time::Instant::now();
    assert!(!h.enabled().await);
    assert!(started.elapsed() < Duration::from_secs(1));
    server.abort();
    let _ = server.await;
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "activity-host-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
