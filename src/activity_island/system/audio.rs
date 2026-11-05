//! Observe PulseAudio-compatible PipeWire metadata. Never capture audio buffers.
use super::*;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
const MAX_BYTES: u64 = 1_048_576;
async fn query(args: &[&str]) -> Option<Value> {
    let mut child = tokio::process::Command::new("pactl")
        .args(args)
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let mut output = child.stdout.take()?.take(MAX_BYTES + 1);
    let result = tokio::time::timeout(Duration::from_secs(3), async {
        let mut bytes = Vec::new();
        output.read_to_end(&mut bytes).await.ok()?;
        if bytes.len() > MAX_BYTES as usize {
            return None;
        }
        if !child.wait().await.ok()?.success() {
            return None;
        }
        serde_json::from_slice(&bytes).ok()
    })
    .await
    .ok()
    .flatten();
    if result.is_none() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    result
}
fn list(v: &Value) -> &[Value] {
    v.as_array()
        .filter(|a| a.len() <= 256)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}
fn label(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .map(clean)
        .unwrap_or_default()
}
fn capture(outputs: &Value, sources: &Value) -> Option<Activity> {
    let mut names = Vec::new();
    let mut muted = true;
    for output in list(outputs) {
        if output.get("corked").and_then(Value::as_bool) != Some(false) {
            continue;
        }
        let Some(source) = list(sources).iter().find(|s| {
            s.get("index").and_then(Value::as_u64) == output.get("source").and_then(Value::as_u64)
                && s.get("index").and_then(Value::as_u64).is_some()
                && s.get("monitor_source").and_then(Value::as_str) == Some("")
                && s.get("state").and_then(Value::as_str) == Some("RUNNING")
                && s.get("flags")
                    .and_then(Value::as_array)
                    .is_some_and(|f| f.iter().any(|v| v.as_str() == Some("HARDWARE")))
        }) else {
            continue;
        };
        let name = label(&output["properties"], "application.name");
        if names.len() < 4 {
            names.push(if name.is_empty() {
                "Audio application".into()
            } else {
                name
            });
        }
        muted &= source.get("mute").and_then(Value::as_bool) == Some(true);
    }
    if names.is_empty() {
        return None;
    }
    names.sort();
    names.dedup();
    Some(Activity::new(
        Source::Microphone,
        if muted {
            "Microphone muted"
        } else {
            "Microphone in use"
        },
        &names.join(", "),
        if muted {
            "Muted".into()
        } else {
            "Active".into()
        },
        None,
        false,
        None,
    ))
}
async fn sample() -> Option<(Option<Activity>, String)> {
    let outputs = query(&["--format=json", "list", "source-outputs"]).await?;
    let sources = query(&["--format=json", "list", "sources"]).await?;
    let info = query(&["--format=json", "info"]).await?;
    let sinks = query(&["--format=json", "list", "sinks"]).await?;
    let name = info.get("default_sink_name").and_then(Value::as_str)?;
    let route = list(&sinks)
        .iter()
        .find(|s| s.get("name").and_then(Value::as_str) == Some(name))
        .map(|s| label(s, "description"))
        .unwrap_or_default();
    Some((capture(&outputs, &sources), route))
}
pub(super) async fn observe(tx: tokio::sync::mpsc::Sender<(Source, Option<Activity>)>) {
    loop {
        let mut child = match tokio::process::Command::new("pactl")
            .arg("subscribe")
            .env("LC_ALL", "C")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(_) => return,
        };
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill().await;
            return;
        };
        let mut reader = BufReader::new(stdout);
        let mut route: Option<String> = None;
        loop {
            match sample().await {
                Some((mic, next)) => {
                    if tx.send((Source::Microphone, mic)).await.is_err() {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                        return;
                    }
                    if route
                        .as_ref()
                        .is_some_and(|old| !next.is_empty() && old != &next)
                    {
                        let a = Activity::new(
                            Source::Audio,
                            "Audio output changed",
                            &next,
                            String::new(),
                            None,
                            false,
                            Some(chrono::Utc::now().timestamp_millis() + 3000),
                        );
                        if tx.send((Source::Audio, Some(a))).await.is_err() {
                            let _ = child.kill().await;
                            let _ = child.wait().await;
                            return;
                        }
                    }
                    route = Some(next);
                }
                None => {
                    route = None;
                    if tx.send((Source::Microphone, None)).await.is_err() {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                        return;
                    }
                }
            }
            // Read bounded event lines; no periodic polling while the audio server is healthy.
            let mut changed = false;
            while !changed {
                let mut line = Vec::new();
                let mut bounded = (&mut reader).take(1025);
                let read = tokio::select! {_=tx.closed()=>{let _=child.kill().await;let _=child.wait().await;return},r=bounded.read_until(b'\n',&mut line)=>r};
                if !matches!(read,Ok(n) if n>0&&n<=1024&&line.ends_with(b"\n")) {
                    break;
                }
                let line = String::from_utf8_lossy(&line);
                changed = [
                    " on source ",
                    " on source-output ",
                    " on sink ",
                    " on server ",
                ]
                .iter()
                .any(|p| line.contains(p));
            }
            if !changed {
                break;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
            // Collapse queued graph events into one authoritative snapshot.
            // Reading is cancellation-safe; never buffer an unbounded line.
            use cosmic::iced::futures::FutureExt;
            for _ in 0..8 {
                let n = match reader.fill_buf().now_or_never() {
                    Some(Ok(bytes)) => bytes.len(),
                    _ => 0,
                };
                if n == 0 {
                    break;
                }
                reader.consume(n);
            }
        }
        let _ = child.kill().await;
        let _ = child.wait().await;
        if tx.send((Source::Microphone, None)).await.is_err() {
            return;
        }
        let _ = tx.send((Source::Audio, None)).await;
        tokio::select! {_=tx.closed()=>return,_=tokio::time::sleep(Duration::from_secs(15))=>{}}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn reactive_capture_requires_running_physical_input_not_loopback() {
        let out = json!([{"source":1,"corked":false,"properties":{"application.name":"Recorder"}}]);
        let mut sources = json!([{"index":1,"monitor_source":"","state":"RUNNING","mute":false,"flags":["HARDWARE"]}]);
        assert_eq!(capture(&out, &sources).unwrap().detail, "Recorder");
        sources[0]["monitor_source"] = json!("speakers");
        assert!(capture(&out, &sources).is_none());
        sources[0]["monitor_source"] = json!("");
        sources[0]["state"] = json!("SUSPENDED");
        assert!(capture(&out, &sources).is_none());
        assert!(capture(&json!([{"corked":false}]), &json!([{}])).is_none());
    }
    #[test]
    fn reactive_paused_capture_is_absent_and_muted_capture_remains_truthful() {
        let sources = json!([{"index":1,"monitor_source":"","state":"RUNNING","mute":true,"flags":["HARDWARE"]}]);
        assert!(capture(&json!([{"source":1,"corked":true}]), &sources).is_none());
        assert_eq!(
            capture(&json!([{"source":1,"corked":false}]), &sources)
                .unwrap()
                .title,
            "Microphone muted"
        );
    }
}
