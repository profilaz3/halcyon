//! The Music card's output picker: the sound server's sinks by friendly
//! name, and switching the default with what is already playing moved along.
//! Read only while the picker is open; nothing polls.
use serde_json::Value;
use std::time::Duration;
use tokio::io::AsyncReadExt;

const MAX_BYTES: u64 = 1024 * 1024;
/// Outputs listed at most.
pub const LIMIT: usize = 6;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sink {
    pub index: u32,
    pub name: String,
    pub label: String,
    pub icon: &'static str,
    pub default: bool,
}

/// `pactl -f json list sinks`, with the default sink's name.
pub fn parse_sinks(json: &Value, default: &str) -> Vec<Sink> {
    let Some(list) = json.as_array() else {
        return Vec::new();
    };
    let mut sinks: Vec<(Sink, String)> = list
        .iter()
        .filter_map(|s| {
            let name = s.get("name")?.as_str()?;
            let index = u32::try_from(s.get("index")?.as_u64()?).ok()?;
            let prop = |key: &str| {
                s.get("properties")
                    .and_then(|p| p.get(key))
                    .and_then(Value::as_str)
                    .unwrap_or("")
            };
            if prop("node.virtual") == "true" || name.len() > 256 {
                return None;
            }
            let description = s.get("description").and_then(Value::as_str).unwrap_or(name);
            let active = s.get("active_port").and_then(Value::as_str).unwrap_or("");
            let port = s
                .get("ports")
                .and_then(Value::as_array)
                .and_then(|ports| {
                    ports
                        .iter()
                        .find(|p| p.get("name").and_then(Value::as_str) == Some(active))
                })
                .and_then(|p| p.get("description"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let bluetooth = prop("device.bus") == "bluetooth" || prop("device.api") == "bluez5";
            // A Bluetooth device's own name says it all; a sound card's says
            // nothing, so its plug does.
            let label = if bluetooth || port.is_empty() {
                plain(description)
            } else {
                plain(port)
            };
            let nick = [
                prop("node.nick"),
                prop("device.nick"),
                prop("device.description"),
            ]
            .into_iter()
            .map(plain)
            .find(|n| !n.is_empty() && *n != label)
            .unwrap_or_default();
            let form = prop("device.form_factor");
            let icon = match () {
                _ if form == "headset" => "audio-headset-symbolic",
                _ if form == "headphone" || form == "headphones" => "audio-headphones-symbolic",
                _ if bluetooth => "bluetooth-symbolic",
                _ if active.contains("headphone") => "audio-headphones-symbolic",
                _ if active.starts_with("hdmi") || active.contains("displayport") => {
                    "video-display-symbolic"
                }
                _ => "audio-speakers-symbolic",
            };
            Some((
                Sink {
                    index,
                    name: name.to_owned(),
                    label,
                    icon,
                    default: name == default,
                },
                nick,
            ))
        })
        .collect();
    // Two plugs with the same name are told apart by their device.
    let labels: Vec<String> = sinks.iter().map(|(s, _)| s.label.clone()).collect();
    for (sink, nick) in &mut sinks {
        if labels.iter().filter(|l| **l == sink.label).count() > 1 && !nick.is_empty() {
            sink.label = format!("{} · {nick}", sink.label);
        }
    }
    let mut sinks: Vec<Sink> = sinks.into_iter().map(|(s, _)| s).collect();
    // The current output leads; the rest keep the server's order.
    sinks.sort_by_key(|s| !s.default);
    sinks.truncate(LIMIT);
    sinks
}
fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(48)
        .collect::<String>()
        .trim()
        .to_owned()
}
/// `pactl -f json list sink-inputs`: the streams not yet on `target`.
pub fn streams_to_move(json: &Value, target: u32) -> Vec<u32> {
    json.as_array()
        .into_iter()
        .flatten()
        .filter(|s| s.get("sink").and_then(Value::as_u64) != Some(u64::from(target)))
        .filter_map(|s| u32::try_from(s.get("index")?.as_u64()?).ok())
        .take(64)
        .collect()
}

async fn pactl(args: &[&str]) -> Option<Vec<u8>> {
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
        if bytes.len() as u64 > MAX_BYTES || !child.wait().await.ok()?.success() {
            return None;
        }
        Some(bytes)
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
pub async fn list() -> Result<Vec<Sink>, String> {
    if crate::tools::preview() {
        return Ok(preview());
    }
    let default = pactl(&["get-default-sink"]).await.unwrap_or_default();
    let default = String::from_utf8_lossy(&default).trim().to_owned();
    let json = pactl(&["-f", "json", "list", "sinks"])
        .await
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or("Could not list outputs")?;
    Ok(parse_sinks(&json, &default))
}
/// Makes `sink` the default and moves what is playing onto it.
pub async fn select(sink: Sink) -> Result<Vec<Sink>, String> {
    if crate::tools::preview() {
        return Ok(preview()
            .into_iter()
            .map(|s| Sink {
                default: s.name == sink.name,
                ..s
            })
            .collect());
    }
    pactl(&["set-default-sink", &sink.name])
        .await
        .ok_or("Could not switch the output")?;
    if let Some(inputs) = pactl(&["-f", "json", "list", "sink-inputs"])
        .await
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    {
        let target = sink.index.to_string();
        for stream in streams_to_move(&inputs, sink.index) {
            let _ = pactl(&["move-sink-input", &stream.to_string(), &target]).await;
        }
    }
    list().await
}
/// Fixed outputs for the preview desktop and its captures.
fn preview() -> Vec<Sink> {
    vec![
        Sink {
            index: 1,
            name: "preview.speakers".into(),
            label: "Speakers".into(),
            icon: "audio-speakers-symbolic",
            default: true,
        },
        Sink {
            index: 2,
            name: "preview.headphones".into(),
            label: "Headphones".into(),
            icon: "audio-headphones-symbolic",
            default: false,
        },
        Sink {
            index: 3,
            name: "preview.bluetooth".into(),
            label: "Wireless earbuds".into(),
            icon: "audio-headset-symbolic",
            default: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    /// Shaped like this machine's `pactl -f json list sinks`.
    fn sinks() -> Value {
        json!([
            {"index": 5727, "name": "alsa_output.pci-0000_07_00.6.iec958-stereo",
             "description": "Family 17h/19h HD Audio Controller Digital Stereo (IEC958)",
             "active_port": "iec958-stereo-output",
             "ports": [{"name": "iec958-stereo-output", "description": "Digital Output (S/PDIF)"}],
             "properties": {"device.bus": "pci", "node.nick": "ALCS1200A Digital"}},
            {"index": 5729, "name": "alsa_output.pci-0000_07_00.1.hdmi-stereo",
             "description": "Renoir Radeon High Definition Audio Controller Digital Stereo (HDMI)",
             "active_port": "hdmi-output-0",
             "ports": [{"name": "hdmi-output-0", "description": "HDMI / DisplayPort"}],
             "properties": {"device.bus": "pci", "node.nick": "24G2W1G5"}},
            {"index": 5730, "name": "alsa_output.usb.analog-stereo",
             "description": "USB Audio Analog Stereo",
             "active_port": "analog-output-headphones",
             "ports": [{"name": "analog-output-headphones", "description": "Headphones"}],
             "properties": {"device.bus": "usb"}},
            {"index": 5747, "name": "bluez_output.00_13_D6_5B_4E_E0.1",
             "description": "REDMI Headphones Neo", "active_port": "headset-output",
             "ports": [{"name": "headset-output", "description": "Headphones"}],
             "properties": {"device.bus": "bluetooth", "device.form_factor": "headset"}},
            {"index": 9, "name": "null", "description": "Dummy",
             "properties": {"node.virtual": "true"}},
            {"name": "no index"}
        ])
    }
    #[test]
    fn outputs_use_friendly_names_and_icons_with_the_default_first() {
        let list = parse_sinks(&sinks(), "bluez_output.00_13_D6_5B_4E_E0.1");
        let shown: Vec<(&str, &str, bool)> = list
            .iter()
            .map(|s| (s.label.as_str(), s.icon, s.default))
            .collect();
        assert_eq!(
            shown,
            [
                ("REDMI Headphones Neo", "audio-headset-symbolic", true),
                ("Digital Output (S/PDIF)", "audio-speakers-symbolic", false),
                ("HDMI / DisplayPort", "video-display-symbolic", false),
                ("Headphones", "audio-headphones-symbolic", false),
            ]
        );
        assert_eq!(list[0].index, 5747);
    }
    #[test]
    fn outputs_tell_apart_two_plugs_with_one_name() {
        let two = json!([
            {"index": 1, "name": "a", "description": "Card A", "active_port": "hdmi-output-0",
             "ports": [{"name": "hdmi-output-0", "description": "HDMI / DisplayPort"}],
             "properties": {"node.nick": "Monitor A"}},
            {"index": 2, "name": "b", "description": "Card B", "active_port": "hdmi-output-1",
             "ports": [{"name": "hdmi-output-1", "description": "HDMI / DisplayPort"}],
             "properties": {"node.nick": "Monitor B"}}
        ]);
        let labels: Vec<String> = parse_sinks(&two, "").into_iter().map(|s| s.label).collect();
        assert_eq!(
            labels,
            [
                "HDMI / DisplayPort · Monitor A",
                "HDMI / DisplayPort · Monitor B"
            ]
        );
        assert!(parse_sinks(&json!({}), "").is_empty());
    }
    #[test]
    fn outputs_move_only_streams_on_other_sinks() {
        let inputs = json!([
            {"index": 299, "sink": 5747},
            {"index": 5749, "sink": 5729},
            {"index": 12, "sink": 5727},
            {"sink": 1}
        ]);
        assert_eq!(streams_to_move(&inputs, 5747), [5749, 12]);
        assert!(streams_to_move(&json!(null), 1).is_empty());
    }
}
