//! Bluetooth devices connecting and disconnecting, with their battery, from
//! BlueZ on the system bus. Read only: nothing is paired or connected here.
use super::*;
use cosmic::iced::futures::{FutureExt, StreamExt};
use std::collections::HashMap;
use zbus::{
    Connection, MatchRule, MessageStream, Proxy,
    message::Type,
    zvariant::{OwnedObjectPath, OwnedValue},
};

const SERVICE: &str = "org.bluez";
const DEVICE: &str = "org.bluez.Device1";
const BATTERY: &str = "org.bluez.Battery1";
/// How long a connect or disconnect stays in the island.
const SHOWN_MS: i64 = 4000;
type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

#[derive(Clone, Debug, PartialEq)]
struct Device {
    name: String,
    connected: bool,
    battery: Option<u8>,
}
fn devices(objects: &Objects) -> HashMap<String, Device> {
    objects
        .iter()
        .take(256)
        .filter_map(|(path, interfaces)| {
            let device = interfaces.get(DEVICE)?;
            let text = |key: &str| {
                device
                    .get(key)
                    .and_then(|v| <&str>::try_from(v).ok())
                    .map(clean)
                    .filter(|s| !s.is_empty())
            };
            Some((
                path.to_string(),
                Device {
                    name: text("Alias")
                        .or_else(|| text("Name"))
                        .unwrap_or_else(|| "Bluetooth device".into()),
                    connected: device
                        .get("Connected")
                        .and_then(|v| bool::try_from(v).ok())
                        .unwrap_or(false),
                    battery: interfaces
                        .get(BATTERY)
                        .and_then(|b| b.get("Percentage"))
                        .and_then(|v| u8::try_from(v).ok())
                        .filter(|p| *p <= 100),
                },
            ))
        })
        .collect()
}
fn announce(device: &Device, now: i64) -> Activity {
    Activity::new(
        Source::Bluetooth,
        &device.name,
        if device.connected {
            "Connected"
        } else {
            "Disconnected"
        },
        match device.battery {
            Some(p) if device.connected => format!("{p}%"),
            _ => String::new(),
        },
        None,
        false,
        Some(now + SHOWN_MS),
    )
}
#[derive(Default)]
struct Tracker {
    /// Devices as last read; `None` until the first read, which is silent.
    known: Option<HashMap<String, Device>>,
    /// The device being announced, and the announcement.
    current: Option<(String, Activity)>,
}
impl Tracker {
    fn update(&mut self, next: Option<HashMap<String, Device>>, now: i64) -> Option<Activity> {
        let Some(next) = next else {
            self.known = None;
            self.current = None;
            return None;
        };
        if let Some(before) = &self.known {
            let mut paths: Vec<&String> = next.keys().chain(before.keys()).collect();
            paths.sort();
            paths.dedup();
            for path in paths {
                let was = before.get(path).is_some_and(|d| d.connected);
                let device = match next.get(path) {
                    Some(d) => d.clone(),
                    // Removed while connected: it has gone.
                    None if was => Device {
                        connected: false,
                        ..before[path].clone()
                    },
                    None => continue,
                };
                if device.connected != was {
                    self.current = Some((path.clone(), announce(&device, now)));
                }
            }
        }
        // BlueZ often reports the battery a moment after connecting.
        if let Some((path, a)) = &mut self.current
            && let Some(Device {
                connected: true,
                battery: Some(p),
                ..
            }) = next.get(path)
            && a.detail == "Connected"
        {
            a.cue = format!("{p}%");
        }
        self.known = Some(next);
        if self
            .current
            .as_ref()
            .is_some_and(|(_, a)| a.expires_ms.is_some_and(|end| end <= now))
        {
            self.current = None;
        }
        self.current.as_ref().map(|(_, a)| a.clone())
    }
}
async fn read(c: &Connection) -> Option<HashMap<String, Device>> {
    if !super::bus::owner(c, SERVICE).await {
        return None;
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        let manager = Proxy::new(c, SERVICE, "/", "org.freedesktop.DBus.ObjectManager")
            .await
            .ok()?;
        let objects: Objects = manager
            .call_with_flags(
                "GetManagedObjects",
                zbus::proxy::MethodFlags::NoAutoStart.into(),
                &(),
            )
            .await
            .ok()??;
        Some(devices(&objects))
    })
    .await
    .ok()
    .flatten()
}
/// Only connection, battery and device lists matter; signal strength
/// updates during a scan do not cause a read.
fn relevant(message: &zbus::Message) -> bool {
    let header = message.header();
    match header.member().map(|m| m.as_str()) {
        Some("InterfacesAdded" | "InterfacesRemoved") => true,
        Some("PropertiesChanged") => message
            .body()
            .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
            .is_ok_and(|(interface, changed, _)| {
                (interface == DEVICE && changed.contains_key("Connected"))
                    || (interface == BATTERY && changed.contains_key("Percentage"))
                    || (interface == DEVICE && changed.contains_key("Alias"))
            }),
        _ => false,
    }
}
pub(super) async fn observe(tx: tokio::sync::mpsc::Sender<(Source, Option<Activity>)>) {
    loop {
        let result = async {
            let c = tokio::time::timeout(Duration::from_secs(3), Connection::system())
                .await
                .ok()?
                .ok()?;
            let rule = MatchRule::builder()
                .msg_type(Type::Signal)
                .sender(SERVICE)
                .ok()?
                .build();
            let mut changes = MessageStream::for_match_rule(rule, &c, Some(32))
                .await
                .ok()?;
            let mut tracker = Tracker::default();
            loop {
                let devices = read(&c).await;
                let gone = devices.is_none();
                let state = tracker.update(devices, chrono::Utc::now().timestamp_millis());
                let shown = state.is_some();
                tx.send((Source::Bluetooth, state)).await.ok()?;
                if gone {
                    // No BlueZ: look again later rather than on every signal.
                    return Some(());
                }
                let wake = if shown {
                    Duration::from_millis(SHOWN_MS as u64)
                } else {
                    Duration::from_secs(60)
                };
                // One deadline for the whole wait: signal strength updates
                // during a scan must not keep pushing it back.
                let deadline = tokio::time::sleep(wake);
                tokio::pin!(deadline);
                let mut wanted = false;
                while !wanted {
                    tokio::select! {
                        _ = tx.closed() => return None,
                        msg = changes.next() => wanted = relevant(&msg?.ok()?),
                        _ = &mut deadline => wanted = true,
                    }
                }
                // Connecting raises a burst of signals; read once after it.
                tokio::time::sleep(Duration::from_millis(300)).await;
                for _ in 0..32 {
                    match changes.next().now_or_never() {
                        Some(Some(Ok(_))) => {}
                        Some(Some(Err(_))) | Some(None) => return None,
                        None => break,
                    }
                }
            }
        }
        .await;
        let _ = result;
        if tx.send((Source::Bluetooth, None)).await.is_err() {
            return;
        }
        tokio::select! {_ = tx.closed() => return, _ = tokio::time::sleep(Duration::from_secs(30)) => {}}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn device(name: &str, connected: bool, battery: Option<u8>) -> Device {
        Device {
            name: name.into(),
            connected,
            battery,
        }
    }
    fn set(list: &[(&str, Device)]) -> Option<HashMap<String, Device>> {
        Some(list.iter().map(|(p, d)| (p.to_string(), d.clone())).collect())
    }
    #[test]
    fn connects_and_disconnects_are_announced_briefly_with_battery() {
        let mut t = Tracker::default();
        let path = "/org/bluez/hci0/dev_AA";
        // What was already connected at start is not news.
        assert!(t.update(set(&[(path, device("Buds", true, Some(80)))]), 0).is_none());
        let a = t
            .update(set(&[(path, device("Buds", false, None))]), 1000)
            .unwrap();
        assert_eq!((a.title.as_str(), a.detail.as_str(), a.cue.as_str()), ("Buds", "Disconnected", ""));
        assert_eq!(a.expires_ms, Some(1000 + SHOWN_MS));
        assert!(!a.urgent && a.announce);
        let a = t.update(set(&[(path, device("Buds", true, None))]), 6000).unwrap();
        assert_eq!(a.detail, "Connected");
        assert!(a.cue.is_empty());
        // The battery arriving a moment later fills in the same announcement.
        let b = t
            .update(set(&[(path, device("Buds", true, Some(70)))]), 6500)
            .unwrap();
        assert_eq!(b.cue, "70%");
        assert_eq!(b.expires_ms, a.expires_ms, "not announced again");
        // It ends on time, and a removed connected device has disconnected.
        assert!(t.update(set(&[(path, device("Buds", true, Some(70)))]), 6000 + SHOWN_MS).is_none());
        let gone = t.update(set(&[]), 20_000).unwrap();
        assert_eq!(gone.detail, "Disconnected");
        // Losing BlueZ forgets everything; coming back is silent again.
        assert!(t.update(None, 21_000).is_none());
        assert!(t.update(set(&[(path, device("Buds", true, None))]), 22_000).is_none());
    }
    #[test]
    fn bluez_objects_become_named_devices() {
        let mut objects: Objects = HashMap::new();
        let mut device = HashMap::new();
        device.insert(
            "Alias".to_string(),
            OwnedValue::try_from(zbus::zvariant::Value::from("Desk speaker\n")).unwrap(),
        );
        device.insert("Connected".to_string(), OwnedValue::from(true));
        let mut battery = HashMap::new();
        battery.insert("Percentage".to_string(), OwnedValue::from(55u8));
        let mut interfaces = HashMap::new();
        interfaces.insert(DEVICE.to_string(), device);
        interfaces.insert(BATTERY.to_string(), battery);
        objects.insert(
            OwnedObjectPath::try_from("/org/bluez/hci0/dev_BB").unwrap(),
            interfaces,
        );
        // An adapter is not a device.
        let mut adapter = HashMap::new();
        adapter.insert("org.bluez.Adapter1".to_string(), HashMap::new());
        objects.insert(OwnedObjectPath::try_from("/org/bluez/hci0").unwrap(), adapter);
        let found = devices(&objects);
        assert_eq!(found.len(), 1);
        assert_eq!(found["/org/bluez/hci0/dev_BB"], device_named());
        fn device_named() -> Device {
            Device {
                name: "Desk speaker".into(),
                connected: true,
                battery: Some(55),
            }
        }
    }
}
