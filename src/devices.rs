//! Connected devices and their battery: headphones, mice, keyboards,
//! controllers and phones. Read only, from UPower and BlueZ on the system bus
//! and KDE Connect or GSConnect on the session bus, woken by their signals.
//! Nothing is paired, installed or started here, and a missing source is
//! simply left out. The laptop's own battery belongs to the Power card.
use cosmic::iced::{
    Subscription,
    futures::{FutureExt, SinkExt, Stream, StreamExt},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use zbus::{
    Connection, MatchRule, MessageStream, Proxy,
    message::Type,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

const UPOWER: &str = "org.freedesktop.UPower";
const UPOWER_DEVICE: &str = "org.freedesktop.UPower.Device";
const BLUEZ: &str = "org.bluez";
const BLUEZ_DEVICE: &str = "org.bluez.Device1";
const BLUEZ_BATTERY: &str = "org.bluez.Battery1";
const KDECONNECT: &str = "org.kde.kdeconnect";
const GSCONNECT: &str = "org.gnome.Shell.Extensions.GSConnect";
const GSCONNECT_PATH: &str = "/org/gnome/Shell/Extensions/GSConnect";
const GSCONNECT_DEVICE: &str = "org.gnome.Shell.Extensions.GSConnect.Device";
/// Low battery alerts, highest first: each fires once per discharge.
pub const LOW: [u8; 2] = [20, 10];
/// At most this many devices are listed.
const MOST: usize = 16;
type Props = HashMap<String, OwnedValue>;
type Objects = HashMap<OwnedObjectPath, HashMap<String, Props>>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Headphones,
    Headset,
    Speaker,
    Mouse,
    Keyboard,
    Gamepad,
    Phone,
    Tablet,
    #[default]
    #[serde(other)]
    Other,
}
impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Headphones => "Headphones",
            Self::Headset => "Headset",
            Self::Speaker => "Speaker",
            Self::Mouse => "Mouse",
            Self::Keyboard => "Keyboard",
            Self::Gamepad => "Controller",
            Self::Phone => "Phone",
            Self::Tablet => "Tablet",
            Self::Other => "Device",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Self::Headphones => "audio-headphones-symbolic",
            Self::Headset => "audio-headset-symbolic",
            Self::Speaker => "audio-speakers-symbolic",
            Self::Mouse => "input-mouse-symbolic",
            Self::Keyboard => "input-keyboard-symbolic",
            Self::Gamepad => "input-gaming-symbolic",
            Self::Phone => "phone-symbolic",
            Self::Tablet => "tablet-symbolic",
            Self::Other => "bluetooth-symbolic",
        }
    }
    /// UPower's device type; `None` for what is not a device to list, such
    /// as the laptop's battery, its charger or a UPS.
    pub fn from_upower(kind: u32) -> Option<Self> {
        Some(match kind {
            0..=4 => return None,
            5 | 14 => Self::Mouse,
            6 => Self::Keyboard,
            8 => Self::Phone,
            10 => Self::Tablet,
            12 => Self::Gamepad,
            17 => Self::Headset,
            18 => Self::Speaker,
            19 => Self::Headphones,
            _ => Self::Other,
        })
    }
    /// BlueZ's icon name for a device.
    pub fn from_icon(icon: &str) -> Self {
        match icon.trim_end_matches("-symbolic") {
            "audio-headphones" => Self::Headphones,
            "audio-headset" => Self::Headset,
            "audio-card" | "audio-speakers" => Self::Speaker,
            "input-mouse" => Self::Mouse,
            "input-keyboard" => Self::Keyboard,
            "input-gaming" => Self::Gamepad,
            "phone" => Self::Phone,
            "input-tablet" => Self::Tablet,
            _ => Self::Other,
        }
    }
    /// KDE Connect's and GSConnect's device type.
    pub fn from_phone_type(kind: &str) -> Self {
        match kind {
            "smartphone" | "phone" => Self::Phone,
            "tablet" => Self::Tablet,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    /// `bt:<address>` for Bluetooth devices, whichever source saw them.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battery: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charging: Option<bool>,
    /// When it connected; `None` for what was already there at start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected_ms: Option<i64>,
    /// The last low battery level crossed (one of `LOW`), and when.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert_ms: Option<i64>,
}
impl Device {
    pub fn charging(&self) -> bool {
        self.charging == Some(true)
    }
    /// The low level it sits at or under while discharging, if any.
    pub fn low(&self) -> Option<u8> {
        let battery = self.battery?;
        (!self.charging())
            .then(|| LOW.iter().rev().copied().find(|t| battery <= *t))
            .flatten()
    }
    pub fn percent(&self) -> Option<String> {
        self.battery.map(|b| format!("{b}%"))
    }
    /// “Headphones” or “Headphones · Charging”.
    pub fn detail(&self) -> String {
        if self.charging() {
            format!("{} · Charging", self.kind.label())
        } else {
            self.kind.label().into()
        }
    }
}

fn clean(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect::<String>()
        .trim()
        .to_owned()
}
fn text(p: &Props, key: &str) -> String {
    p.get(key)
        .and_then(|v| <&str>::try_from(v).ok())
        .map(clean)
        .unwrap_or_default()
}
fn flag(p: &Props, key: &str) -> Option<bool> {
    p.get(key).and_then(|v| bool::try_from(v).ok())
}
fn whole(v: &Value) -> Option<i64> {
    match v {
        Value::U8(n) => Some(i64::from(*n)),
        Value::I16(n) => Some(i64::from(*n)),
        Value::U16(n) => Some(i64::from(*n)),
        Value::I32(n) => Some(i64::from(*n)),
        Value::U32(n) => Some(i64::from(*n)),
        Value::I64(n) => Some(*n),
        Value::U64(n) => i64::try_from(*n).ok(),
        Value::F64(n) if n.is_finite() => Some(n.round() as i64),
        Value::Value(inner) => whole(inner),
        _ => None,
    }
}
fn percent(n: i64) -> Option<u8> {
    u8::try_from(n).ok().filter(|n| *n <= 100)
}
/// A Bluetooth address in any of the forms the sources use
/// (`/org/bluez/hci0/dev_AA_BB_…`, `hid-aa:bb:…-battery`, `AA:BB:…`), upper case.
pub fn address(s: &str) -> Option<String> {
    let chars: Vec<char> = s.chars().collect();
    chars.windows(17).find_map(|w| {
        let ok = w.iter().enumerate().all(|(i, c)| {
            if i % 3 == 2 {
                matches!(c, ':' | '_')
            } else {
                c.is_ascii_hexdigit()
            }
        }) && w[2] == w[5]
            && w[2] == w[14];
        ok.then(|| {
            w.iter()
                .map(|c| {
                    if *c == '_' {
                        ':'
                    } else {
                        c.to_ascii_uppercase()
                    }
                })
                .collect()
        })
    })
}

/// One UPower device, unless it powers the computer or is not there.
pub fn upower(path: &str, p: &Props) -> Option<Device> {
    if path.ends_with("/DisplayDevice")
        || flag(p, "PowerSupply") == Some(true)
        || flag(p, "IsPresent") == Some(false)
    {
        return None;
    }
    let kind = Kind::from_upower(p.get("Type").and_then(|v| u32::try_from(v).ok())?)?;
    let native = text(p, "NativePath");
    let state = p
        .get("State")
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(0);
    let battery = p
        .get("Percentage")
        .and_then(|v| f64::try_from(v).ok())
        .filter(|v| v.is_finite() && (*v > 0. || state != 0))
        .and_then(|v| percent(v.round() as i64));
    let name = [text(p, "Model"), text(p, "Vendor")]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or_else(|| kind.label().into());
    let id = match address(&native).or_else(|| address(&text(p, "Serial"))) {
        Some(mac) => format!("bt:{mac}"),
        None if native.is_empty() => format!("upower:{path}"),
        None => format!("upower:{native}"),
    };
    Some(Device {
        id,
        name,
        kind,
        battery,
        charging: match state {
            1 => Some(true),
            2..=6 => Some(false),
            _ => None,
        },
        ..Default::default()
    })
}
/// Connected BlueZ devices, with the battery BlueZ knows.
pub fn bluez(objects: &Objects) -> Vec<Device> {
    objects
        .iter()
        .take(256)
        .filter_map(|(path, interfaces)| {
            let device = interfaces.get(BLUEZ_DEVICE)?;
            if flag(device, "Connected") != Some(true) {
                return None;
            }
            let mac = address(&text(device, "Address")).or_else(|| address(path.as_str()))?;
            let kind = Kind::from_icon(&text(device, "Icon"));
            let name = [text(device, "Alias"), text(device, "Name")]
                .into_iter()
                .find(|s| !s.is_empty())
                .unwrap_or_else(|| kind.label().into());
            Some(Device {
                id: format!("bt:{mac}"),
                name,
                kind,
                battery: interfaces
                    .get(BLUEZ_BATTERY)
                    .and_then(|b| b.get("Percentage"))
                    .and_then(|v| whole(v))
                    .and_then(percent),
                ..Default::default()
            })
        })
        .collect()
}
/// A reachable KDE Connect device, with its battery plugin's reading.
pub fn kdeconnect(id: &str, device: &Props, battery: Option<&Props>) -> Option<Device> {
    if flag(device, "isReachable") == Some(false) {
        return None;
    }
    let kind = Kind::from_phone_type(&text(device, "type"));
    let name = Some(text(device, "name"))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| kind.label().into());
    Some(Device {
        id: format!("kdeconnect:{}", clean(id)),
        name,
        kind,
        battery: battery
            .and_then(|b| b.get("charge"))
            .and_then(|v| whole(v))
            .and_then(percent),
        charging: battery.and_then(|b| flag(b, "isCharging")),
        ..Default::default()
    })
}
/// The battery action's state, read tolerantly: older GSConnect gives a
/// tuple (charging, icon, level, time), others a dictionary.
pub fn gsconnect_battery(state: &Value) -> (Option<u8>, Option<bool>) {
    fn walk(v: &Value, level: &mut Option<u8>, charging: &mut Option<bool>) {
        match v {
            Value::Bool(b) => *charging = charging.or(Some(*b)),
            Value::Value(inner) => walk(inner, level, charging),
            Value::Structure(s) => s.fields().iter().for_each(|f| walk(f, level, charging)),
            Value::Array(a) => a.iter().for_each(|f| walk(f, level, charging)),
            Value::Dict(d) => {
                for (k, v) in d.iter() {
                    match <&str>::try_from(k).unwrap_or_default() {
                        "level" | "charge" | "percentage" => {
                            *level = level.or(whole(v).and_then(percent))
                        }
                        "charging" | "is-charging" => {
                            *charging = charging.or(bool::try_from(v).ok().or_else(|| match v {
                                Value::Value(b) => bool::try_from(&**b).ok(),
                                _ => None,
                            }))
                        }
                        _ => {}
                    }
                }
            }
            other => {
                if level.is_none() {
                    *level = whole(other).and_then(percent);
                }
            }
        }
    }
    let (mut level, mut charging) = (None, None);
    walk(state, &mut level, &mut charging);
    (level, charging)
}
/// Connected GSConnect devices; `batteries` holds each device path's
/// battery action state.
pub fn gsconnect(objects: &Objects, batteries: &HashMap<String, OwnedValue>) -> Vec<Device> {
    objects
        .iter()
        .take(64)
        .filter_map(|(path, interfaces)| {
            let device = interfaces.get(GSCONNECT_DEVICE)?;
            if flag(device, "Connected") != Some(true) {
                return None;
            }
            let kind = Kind::from_phone_type(&text(device, "Type"));
            let name = Some(text(device, "Name"))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| kind.label().into());
            let (battery, charging) = batteries
                .get(path.as_str())
                .map(|s| gsconnect_battery(s))
                .unwrap_or_default();
            Some(Device {
                id: format!(
                    "gsconnect:{}",
                    clean(path.as_str().rsplit('/').next().unwrap_or_default())
                ),
                name,
                kind,
                battery,
                charging,
                ..Default::default()
            })
        })
        .collect()
}
/// One list from all sources. A device UPower and BlueZ both see is listed
/// once, by its address; a phone KDE Connect or GSConnect knows replaces a
/// Bluetooth entry of the same name that has no battery.
pub fn merge(upower: Vec<Device>, bluez: Vec<Device>, phones: Vec<Device>) -> Vec<Device> {
    let mut list = bluez;
    for d in upower {
        match list.iter_mut().find(|b| b.id == d.id) {
            Some(b) => {
                b.battery = b.battery.or(d.battery);
                b.charging = b.charging.or(d.charging);
                if b.kind == Kind::Other {
                    b.kind = d.kind;
                }
            }
            None => list.push(d),
        }
    }
    for phone in phones {
        list.retain(|d| {
            !(d.battery.is_none()
                && matches!(d.kind, Kind::Phone | Kind::Tablet | Kind::Other)
                && d.name.eq_ignore_ascii_case(&phone.name))
        });
        list.push(phone);
    }
    list.sort_by(|a, b| (a.name.to_lowercase(), &a.id).cmp(&(b.name.to_lowercase(), &b.id)));
    list.dedup_by(|a, b| a.id == b.id);
    list.truncate(MOST);
    list
}

#[derive(Clone, Debug, Default)]
struct Known {
    connected_ms: Option<i64>,
    /// Per `LOW` level: `None` until a first reading, then whether it may fire.
    armed: [Option<bool>; 2],
    alert: Option<(u8, i64)>,
}
/// Stamps connects and low battery crossings onto the list. The first read
/// is silent, a level already under a threshold when first read never
/// fires, and a threshold re-arms only once the battery is back above it.
#[derive(Default)]
pub struct Tracker {
    known: Option<HashMap<String, Known>>,
}
impl Tracker {
    pub fn update(&mut self, mut list: Vec<Device>, now: i64) -> Vec<Device> {
        let first = self.known.is_none();
        let before = self.known.take().unwrap_or_default();
        let mut next = HashMap::new();
        for d in &mut list {
            let mut k = before.get(&d.id).cloned().unwrap_or_else(|| Known {
                connected_ms: (!first).then_some(now),
                ..Default::default()
            });
            if let Some(b) = d.battery {
                let draining = !d.charging();
                for (armed, t) in k.armed.iter_mut().zip(LOW) {
                    match *armed {
                        None => *armed = Some(b > t),
                        Some(true) if b <= t && draining => {
                            *armed = Some(false);
                            k.alert = Some((t, now));
                        }
                        Some(false) if b > t => *armed = Some(true),
                        _ => {}
                    }
                }
            }
            d.connected_ms = k.connected_ms;
            d.alert = k.alert.map(|a| a.0);
            d.alert_ms = k.alert.map(|a| a.1);
            next.insert(d.id.clone(), k);
        }
        self.known = Some(next);
        list
    }
}

async fn owner(c: &Connection, service: &str) -> bool {
    tokio::time::timeout(Duration::from_secs(2), async {
        let p = Proxy::new(
            c,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
        )
        .await
        .ok()?;
        p.call::<_, _, bool>("NameHasOwner", &(service,)).await.ok()
    })
    .await
    .ok()
    .flatten()
    .unwrap_or(false)
}
async fn call<R>(
    c: &Connection,
    service: &str,
    path: &str,
    interface: &str,
    method: &str,
    args: &(impl Serialize + zbus::zvariant::DynamicType),
) -> Option<R>
where
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    tokio::time::timeout(Duration::from_secs(2), async {
        Proxy::new(c, service.to_owned(), path.to_owned(), interface.to_owned())
            .await
            .ok()?
            .call_with_flags(method, zbus::proxy::MethodFlags::NoAutoStart.into(), args)
            .await
            .ok()
            .flatten()
    })
    .await
    .ok()
    .flatten()
}
async fn props(c: &Connection, service: &str, path: &str, interface: &str) -> Option<Props> {
    call(
        c,
        service,
        path,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        &(interface,),
    )
    .await
}
async fn objects(c: &Connection, service: &str, path: &str) -> Option<Objects> {
    call(
        c,
        service,
        path,
        "org.freedesktop.DBus.ObjectManager",
        "GetManagedObjects",
        &(),
    )
    .await
}
async fn read_upower(c: &Connection) -> Vec<Device> {
    if !owner(c, UPOWER).await {
        return Vec::new();
    }
    let paths: Vec<OwnedObjectPath> = call(
        c,
        UPOWER,
        "/org/freedesktop/UPower",
        UPOWER,
        "EnumerateDevices",
        &(),
    )
    .await
    .unwrap_or_default();
    let mut found = Vec::new();
    for path in paths.iter().take(32) {
        if let Some(p) = props(c, UPOWER, path.as_str(), UPOWER_DEVICE).await {
            found.extend(upower(path.as_str(), &p));
        }
    }
    found
}
async fn read_bluez(c: &Connection) -> Vec<Device> {
    if !owner(c, BLUEZ).await {
        return Vec::new();
    }
    objects(c, BLUEZ, "/")
        .await
        .map(|o| bluez(&o))
        .unwrap_or_default()
}
async fn read_kdeconnect(c: &Connection) -> Vec<Device> {
    if !owner(c, KDECONNECT).await {
        return Vec::new();
    }
    let ids: Vec<String> = call(
        c,
        KDECONNECT,
        "/modules/kdeconnect",
        "org.kde.kdeconnect.daemon",
        "devices",
        &(true, true),
    )
    .await
    .unwrap_or_default();
    let mut found = Vec::new();
    for id in ids.iter().take(MOST) {
        let path = format!("/modules/kdeconnect/devices/{id}");
        if zbus::zvariant::ObjectPath::try_from(path.as_str()).is_err() {
            continue;
        }
        let Some(device) = props(c, KDECONNECT, &path, "org.kde.kdeconnect.device").await else {
            continue;
        };
        let battery_path = format!("{path}/battery");
        let interface = "org.kde.kdeconnect.device.battery";
        let mut battery = props(c, KDECONNECT, &battery_path, interface)
            .await
            .filter(|b| !b.is_empty());
        if battery.is_none() {
            // Older KDE Connect has methods rather than properties.
            let charge: Option<i32> =
                call(c, KDECONNECT, &battery_path, interface, "charge", &()).await;
            let charging: Option<bool> =
                call(c, KDECONNECT, &battery_path, interface, "isCharging", &()).await;
            let mut b = Props::new();
            b.extend(charge.map(|v| ("charge".to_string(), OwnedValue::from(v))));
            b.extend(charging.map(|v| ("isCharging".to_string(), OwnedValue::from(v))));
            battery = Some(b);
        }
        found.extend(kdeconnect(id, &device, battery.as_ref()));
    }
    found
}
async fn read_gsconnect(c: &Connection) -> Vec<Device> {
    if !owner(c, GSCONNECT).await {
        return Vec::new();
    }
    let Some(objects) = objects(c, GSCONNECT, GSCONNECT_PATH).await else {
        return Vec::new();
    };
    let mut batteries = HashMap::new();
    for (path, interfaces) in objects.iter().take(MOST) {
        if interfaces
            .get(GSCONNECT_DEVICE)
            .and_then(|d| flag(d, "Connected"))
            != Some(true)
        {
            continue;
        }
        let described: Option<(bool, zbus::zvariant::Signature, Vec<OwnedValue>)> = call(
            c,
            GSCONNECT,
            path.as_str(),
            "org.gtk.Actions",
            "Describe",
            &("battery",),
        )
        .await;
        if let Some(state) = described.and_then(|(_, _, mut state)| state.pop()) {
            batteries.insert(path.to_string(), state);
        }
    }
    gsconnect(&objects, &batteries)
}

/// The signals that may change the list; property updates such as signal
/// strength or the laptop's energy rate are not.
fn relevant(message: &zbus::Message) -> bool {
    let header = message.header();
    let interface = header.interface().map(|i| i.as_str()).unwrap_or_default();
    let member = header.member().map(|m| m.as_str()).unwrap_or_default();
    let path = header.path().map(|p| p.as_str()).unwrap_or_default();
    match (interface, member) {
        ("org.freedesktop.DBus", "NameOwnerChanged") => true,
        (UPOWER, "DeviceAdded" | "DeviceRemoved") => true,
        (_, "InterfacesAdded" | "InterfacesRemoved") => true,
        ("org.freedesktop.DBus.Properties", "PropertiesChanged") => message
            .body()
            .deserialize::<(String, Props, Vec<String>)>()
            .is_ok_and(|(interface, changed, _)| {
                let any = |keys: &[&str]| keys.iter().any(|k| changed.contains_key(*k));
                match interface.as_str() {
                    UPOWER_DEVICE => {
                        !(path.ends_with("/DisplayDevice")
                            || path.contains("/battery_BAT")
                            || path.contains("/line_power_"))
                            && any(&["Percentage", "State", "Model", "IsPresent", "Type"])
                    }
                    BLUEZ_DEVICE => any(&["Connected", "Alias", "Icon"]),
                    BLUEZ_BATTERY => any(&["Percentage"]),
                    GSCONNECT_DEVICE => any(&["Connected", "Name", "Type"]),
                    i => i.starts_with("org.kde.kdeconnect.device"),
                }
            }),
        ("org.gtk.Actions", "Changed") => message
            .body()
            .deserialize::<(
                Vec<String>,
                HashMap<String, bool>,
                HashMap<String, OwnedValue>,
                HashMap<String, (bool, zbus::zvariant::Signature, Vec<OwnedValue>)>,
            )>()
            .map_or(true, |(removed, enabled, state, added)| {
                removed.iter().any(|a| a == "battery")
                    || enabled.contains_key("battery")
                    || state.contains_key("battery")
                    || added.contains_key("battery")
            }),
        (i, m) if i.starts_with(KDECONNECT) => matches!(
            m,
            "deviceAdded"
                | "deviceRemoved"
                | "deviceVisibilityChanged"
                | "deviceListChanged"
                | "reachableChanged"
                | "pairStateChanged"
                | "trustedChanged"
                | "nameChanged"
                | "typeChanged"
                | "refreshed"
                | "chargeChanged"
                | "stateChanged"
        ),
        _ => false,
    }
}
async fn watch(
    c: &Connection,
    services: &[&str],
) -> Option<cosmic::iced::futures::stream::SelectAll<MessageStream>> {
    let mut streams = Vec::new();
    for service in services {
        let from = rules(service).ok()?;
        // Notice the service starting or stopping.
        let owner = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender("org.freedesktop.DBus")
            .ok()?
            .interface("org.freedesktop.DBus")
            .ok()?
            .member("NameOwnerChanged")
            .ok()?
            .arg(0, *service)
            .ok()?
            .build();
        for rule in from.into_iter().chain([owner]) {
            streams.push(
                MessageStream::for_match_rule(rule, c, Some(32))
                    .await
                    .ok()?,
            );
        }
    }
    Some(cosmic::iced::futures::stream::select_all(streams))
}
/// The signals asked of each service: property changes on the interfaces
/// read, and device lists. Not everything it sends: UPower reports the
/// laptop's energy rate and BlueZ each scan result's signal strength, and
/// the bus woke this task for each. KDE Connect spreads its signals over
/// several interfaces a rule cannot name by prefix; it is quiet and on the
/// session bus, so all of its signals still count.
fn rules(service: &str) -> zbus::Result<Vec<MatchRule<'_>>> {
    let changed = |interface: &'static str| {
        Ok::<_, zbus::Error>(
            MatchRule::builder()
                .msg_type(Type::Signal)
                .sender(service)?
                .interface("org.freedesktop.DBus.Properties")?
                .member("PropertiesChanged")?
                .arg(0, interface)?
                .build(),
        )
    };
    let signal = |interface: &'static str, member: &'static str| {
        Ok::<_, zbus::Error>(
            MatchRule::builder()
                .msg_type(Type::Signal)
                .sender(service)?
                .interface(interface)?
                .member(member)?
                .build(),
        )
    };
    const OBJECTS: &str = "org.freedesktop.DBus.ObjectManager";
    Ok(match service {
        UPOWER => vec![
            changed(UPOWER_DEVICE)?,
            signal(UPOWER, "DeviceAdded")?,
            signal(UPOWER, "DeviceRemoved")?,
        ],
        BLUEZ => vec![
            changed(BLUEZ_DEVICE)?,
            changed(BLUEZ_BATTERY)?,
            signal(OBJECTS, "InterfacesAdded")?,
            signal(OBJECTS, "InterfacesRemoved")?,
        ],
        GSCONNECT => vec![
            changed(GSCONNECT_DEVICE)?,
            signal(OBJECTS, "InterfacesAdded")?,
            signal(OBJECTS, "InterfacesRemoved")?,
            signal("org.gtk.Actions", "Changed")?,
        ],
        _ => vec![
            MatchRule::builder()
                .msg_type(Type::Signal)
                .sender(service)?
                .build(),
        ],
    })
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Bus {
    System,
    Session,
}
async fn observe(bus: Bus, tx: tokio::sync::mpsc::Sender<(Bus, Vec<Device>)>) {
    loop {
        let _: Option<()> = async {
            let c = tokio::time::timeout(
                Duration::from_secs(3),
                match bus {
                    Bus::System => Connection::system().boxed(),
                    Bus::Session => Connection::session().boxed(),
                },
            )
            .await
            .ok()?
            .ok()?;
            let services: &[&str] = match bus {
                Bus::System => &[UPOWER, BLUEZ],
                Bus::Session => &[KDECONNECT, GSCONNECT],
            };
            let mut changes = watch(&c, services).await?;
            loop {
                let list = match bus {
                    Bus::System => merge(read_upower(&c).await, read_bluez(&c).await, Vec::new()),
                    Bus::Session => {
                        let mut phones = read_kdeconnect(&c).await;
                        phones.extend(read_gsconnect(&c).await);
                        phones
                    }
                };
                tx.send((bus, list)).await.ok()?;
                // Signals wake the next read. UPower and BlueZ say when
                // anything changes; the phone services get a long fallback
                // for a battery that changed without saying so.
                let deadline = tokio::time::sleep(Duration::from_secs(600));
                tokio::pin!(deadline);
                let mut wanted = false;
                while !wanted {
                    tokio::select! {
                        _ = tx.closed() => return None,
                        msg = changes.next() => wanted = relevant(&msg?.ok()?),
                        _ = &mut deadline, if bus == Bus::Session => wanted = true,
                    }
                }
                // Connecting raises a burst of signals; read once after it.
                tokio::time::sleep(Duration::from_millis(300)).await;
                for _ in 0..64 {
                    match changes.next().now_or_never() {
                        Some(Some(Ok(_))) => {}
                        Some(Some(Err(_))) | Some(None) => return None,
                        None => break,
                    }
                }
            }
        }
        .await;
        if tx.send((bus, Vec::new())).await.is_err() {
            return;
        }
        tokio::select! {
            _ = tx.closed() => return,
            _ = tokio::time::sleep(Duration::from_secs(30)) => {}
        }
    }
}
fn feed() -> impl Stream<Item = Vec<Device>> {
    cosmic::iced::stream::channel(2, async move |mut out| {
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let mut tasks = tokio::task::JoinSet::new();
        tasks.spawn(observe(Bus::System, tx.clone()));
        tasks.spawn(observe(Bus::Session, tx));
        let (mut system, mut session) = (Vec::new(), Vec::new());
        let mut tracker = Tracker::default();
        let mut sent: Option<Vec<Device>> = None;
        while let Some((bus, list)) = rx.recv().await {
            match bus {
                Bus::System => system = list,
                Bus::Session => session = list,
            }
            let list = tracker.update(
                merge(Vec::new(), system.clone(), session.clone()),
                chrono::Utc::now().timestamp_millis(),
            );
            if sent.as_ref() != Some(&list) {
                sent = Some(list.clone());
                if out.send(list).await.is_err() {
                    break;
                }
            }
        }
    })
}
/// Connected devices, sent whenever the list or a battery changes.
pub fn subscription() -> Subscription<Vec<Device>> {
    if crate::tools::preview() {
        return Subscription::none();
    }
    Subscription::run(feed)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(v: &str) -> OwnedValue {
        OwnedValue::try_from(Value::from(v)).unwrap()
    }
    fn props(list: Vec<(&str, OwnedValue)>) -> Props {
        list.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }
    fn device(id: &str, name: &str, kind: Kind, battery: Option<u8>) -> Device {
        Device {
            id: id.into(),
            name: name.into(),
            kind,
            battery,
            ..Default::default()
        }
    }
    #[test]
    fn kinds_map_from_each_source() {
        assert_eq!(Kind::from_upower(1), None, "line power");
        assert_eq!(Kind::from_upower(2), None, "the laptop battery");
        assert_eq!(Kind::from_upower(5), Some(Kind::Mouse));
        assert_eq!(Kind::from_upower(6), Some(Kind::Keyboard));
        assert_eq!(Kind::from_upower(8), Some(Kind::Phone));
        assert_eq!(Kind::from_upower(12), Some(Kind::Gamepad));
        assert_eq!(Kind::from_upower(17), Some(Kind::Headset));
        assert_eq!(Kind::from_upower(19), Some(Kind::Headphones));
        assert_eq!(Kind::from_upower(28), Some(Kind::Other));
        assert_eq!(Kind::from_icon("audio-headset"), Kind::Headset);
        assert_eq!(Kind::from_icon("audio-headphones"), Kind::Headphones);
        assert_eq!(Kind::from_icon("audio-card"), Kind::Speaker);
        assert_eq!(Kind::from_icon("input-gaming"), Kind::Gamepad);
        assert_eq!(Kind::from_icon("input-mouse-symbolic"), Kind::Mouse);
        assert_eq!(Kind::from_icon("phone"), Kind::Phone);
        assert_eq!(Kind::from_icon(""), Kind::Other);
        assert_eq!(Kind::from_phone_type("smartphone"), Kind::Phone);
        assert_eq!(Kind::from_phone_type("phone"), Kind::Phone);
        assert_eq!(Kind::from_phone_type("tablet"), Kind::Tablet);
        assert_eq!(Kind::from_phone_type("laptop"), Kind::Other);
        // A kind a newer owner sends reads as Other rather than failing.
        let d: Device = serde_json::from_str(r#"{"id":"x","name":"Pen","kind":"stylus"}"#).unwrap();
        assert_eq!(d.kind, Kind::Other);
        assert_eq!(Kind::Gamepad.label(), "Controller");
    }
    #[test]
    fn addresses_are_found_in_every_form() {
        let mac = Some("AA:BB:CC:0D:EE:FF".to_string());
        assert_eq!(address("/org/bluez/hci0/dev_AA_BB_CC_0D_EE_FF"), mac);
        assert_eq!(address("hid-aa:bb:cc:0d:ee:ff-battery"), mac);
        assert_eq!(address("AA:BB:CC:0D:EE:FF"), mac);
        assert_eq!(address("battery_hidpp_battery_0"), None);
        assert_eq!(address("AA:BB_CC:0D:EE:FF"), None, "mixed separators");
    }
    #[test]
    fn upower_lists_peripherals_but_not_the_laptop_battery() {
        let laptop = props(vec![
            ("Type", OwnedValue::from(2u32)),
            ("PowerSupply", OwnedValue::from(true)),
            ("Percentage", OwnedValue::from(64f64)),
        ]);
        assert_eq!(
            upower("/org/freedesktop/UPower/devices/battery_BAT0", &laptop),
            None
        );
        let display = props(vec![("Type", OwnedValue::from(2u32))]);
        assert_eq!(
            upower("/org/freedesktop/UPower/devices/DisplayDevice", &display),
            None
        );
        let mouse = props(vec![
            ("Type", OwnedValue::from(5u32)),
            ("PowerSupply", OwnedValue::from(false)),
            ("NativePath", s("hid-aa:bb:cc:dd:ee:01-battery")),
            ("Model", s("MX Anywhere 3")),
            ("Percentage", OwnedValue::from(15f64)),
            ("State", OwnedValue::from(2u32)),
        ]);
        let d = upower("/org/freedesktop/UPower/devices/mouse_hid", &mouse).unwrap();
        assert_eq!(d.id, "bt:AA:BB:CC:DD:EE:01");
        assert_eq!(
            (d.name.as_str(), d.kind, d.battery, d.charging),
            ("MX Anywhere 3", Kind::Mouse, Some(15), Some(false))
        );
        // A receiver device with no address keeps its own id, and an unknown
        // empty reading is no reading.
        let keyboard = props(vec![
            ("Type", OwnedValue::from(6u32)),
            ("NativePath", s("hidpp_battery_1")),
            ("Vendor", s("Logitech")),
            ("Percentage", OwnedValue::from(0f64)),
            ("State", OwnedValue::from(0u32)),
        ]);
        let k = upower(
            "/org/freedesktop/UPower/devices/battery_hidpp_battery_1",
            &keyboard,
        )
        .unwrap();
        assert_eq!(k.id, "upower:hidpp_battery_1");
        assert_eq!(
            (k.name.as_str(), k.battery, k.charging),
            ("Logitech", None, None)
        );
        let charging = props(vec![
            ("Type", OwnedValue::from(19u32)),
            ("NativePath", s("/org/bluez/hci0/dev_11_22_33_44_55_66")),
            ("Percentage", OwnedValue::from(79.6f64)),
            ("State", OwnedValue::from(1u32)),
        ]);
        let h = upower("/p", &charging).unwrap();
        assert_eq!(
            (h.name.as_str(), h.battery, h.charging),
            ("Headphones", Some(80), Some(true))
        );
    }
    fn bluez_object(
        objects: &mut Objects,
        mac: &str,
        alias: &str,
        icon: &str,
        connected: bool,
        battery: Option<u8>,
    ) {
        let mut interfaces = HashMap::new();
        interfaces.insert(
            BLUEZ_DEVICE.to_string(),
            props(vec![
                ("Address", s(mac)),
                ("Alias", s(alias)),
                ("Icon", s(icon)),
                ("Connected", OwnedValue::from(connected)),
            ]),
        );
        if let Some(b) = battery {
            interfaces.insert(
                BLUEZ_BATTERY.to_string(),
                props(vec![("Percentage", OwnedValue::from(b))]),
            );
        }
        let path = format!("/org/bluez/hci0/dev_{}", mac.replace(':', "_"));
        objects.insert(OwnedObjectPath::try_from(path).unwrap(), interfaces);
    }
    #[test]
    fn bluez_lists_connected_devices_only() {
        let mut objects = Objects::new();
        bluez_object(
            &mut objects,
            "11:22:33:44:55:66",
            "Sony WH-1000XM5\n",
            "audio-headphones",
            true,
            Some(80),
        );
        bluez_object(
            &mut objects,
            "11:22:33:44:55:77",
            "Xbox Wireless Controller",
            "input-gaming",
            false,
            None,
        );
        let mut adapter = HashMap::new();
        adapter.insert("org.bluez.Adapter1".to_string(), Props::new());
        objects.insert(
            OwnedObjectPath::try_from("/org/bluez/hci0").unwrap(),
            adapter,
        );
        let found = bluez(&objects);
        assert_eq!(
            found,
            vec![Device {
                id: "bt:11:22:33:44:55:66".into(),
                name: "Sony WH-1000XM5".into(),
                kind: Kind::Headphones,
                battery: Some(80),
                ..Default::default()
            }]
        );
    }
    #[test]
    fn phones_come_from_kde_connect_and_gsconnect() {
        let device = props(vec![
            ("name", s("Pixel 8")),
            ("type", s("smartphone")),
            ("isReachable", OwnedValue::from(true)),
        ]);
        let battery = props(vec![
            ("charge", OwnedValue::from(62i32)),
            ("isCharging", OwnedValue::from(true)),
        ]);
        let d = kdeconnect("abc_123", &device, Some(&battery)).unwrap();
        assert_eq!(
            (
                d.id.as_str(),
                d.name.as_str(),
                d.kind,
                d.battery,
                d.charging
            ),
            (
                "kdeconnect:abc_123",
                "Pixel 8",
                Kind::Phone,
                Some(62),
                Some(true)
            )
        );
        // No battery plugin, or an unknown charge of -1, is no reading.
        let unknown = props(vec![("charge", OwnedValue::from(-1i32))]);
        assert_eq!(
            kdeconnect("a", &device, Some(&unknown)).unwrap().battery,
            None
        );
        assert_eq!(kdeconnect("a", &device, None).unwrap().battery, None);
        let away = props(vec![
            ("name", s("Old")),
            ("isReachable", OwnedValue::from(false)),
        ]);
        assert_eq!(kdeconnect("b", &away, None), None);

        // GSConnect's tuple state and a dictionary state both read.
        let tuple = Value::from((false, "battery-good-symbolic", 47i32, 3600i32));
        assert_eq!(gsconnect_battery(&tuple), (Some(47), Some(false)));
        let mut dict = HashMap::new();
        dict.insert("level", Value::from(30u32));
        dict.insert("charging", Value::from(true));
        assert_eq!(
            gsconnect_battery(&Value::from(dict)),
            (Some(30), Some(true))
        );
        let mut objects = Objects::new();
        let mut interfaces = HashMap::new();
        interfaces.insert(
            GSCONNECT_DEVICE.to_string(),
            props(vec![
                ("Name", s("Galaxy Tab")),
                ("Type", s("tablet")),
                ("Connected", OwnedValue::from(true)),
            ]),
        );
        let path = "/org/gnome/Shell/Extensions/GSConnect/Device/f00d";
        objects.insert(OwnedObjectPath::try_from(path).unwrap(), interfaces);
        let mut batteries = HashMap::new();
        batteries.insert(path.to_string(), OwnedValue::try_from(tuple).unwrap());
        let found = gsconnect(&objects, &batteries);
        assert_eq!(found.len(), 1);
        assert_eq!(
            (found[0].id.as_str(), found[0].kind, found[0].battery),
            ("gsconnect:f00d", Kind::Tablet, Some(47))
        );
    }
    #[test]
    fn merge_lists_each_device_once() {
        let bt = device("bt:11", "Sony WH-1000XM5", Kind::Headphones, None);
        let up = Device {
            charging: Some(true),
            ..device("bt:11", "WH-1000XM5", Kind::Headphones, Some(80))
        };
        let receiver = device("upower:hidpp", "MX Keys", Kind::Keyboard, Some(90));
        let phone_bt = device("bt:22", "pixel 8", Kind::Phone, None);
        let phone = device("kdeconnect:p", "Pixel 8", Kind::Phone, Some(62));
        let list = merge(vec![up, receiver], vec![bt, phone_bt], vec![phone]);
        let ids: Vec<&str> = list.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, ["upower:hidpp", "kdeconnect:p", "bt:11"]);
        // BlueZ's name stays; UPower fills in the battery and charging.
        assert_eq!(
            (list[2].name.as_str(), list[2].battery, list[2].charging),
            ("Sony WH-1000XM5", Some(80), Some(true))
        );
        assert_eq!(list[1].detail(), "Phone");
    }
    #[test]
    fn connects_are_stamped_after_the_first_read() {
        let mut t = Tracker::default();
        let mouse = device("bt:1", "Mouse", Kind::Mouse, Some(70));
        let first = t.update(vec![mouse.clone()], 1000);
        assert_eq!(first[0].connected_ms, None, "already there at start");
        let buds = device("bt:2", "Buds", Kind::Headphones, None);
        let next = t.update(vec![mouse.clone(), buds.clone()], 5000);
        assert_eq!(next[1].connected_ms, Some(5000));
        // The stamp holds while connected, and a reconnect is new.
        let later = t.update(
            vec![
                mouse.clone(),
                Device {
                    battery: Some(80),
                    ..buds.clone()
                },
            ],
            6000,
        );
        assert_eq!(later[1].connected_ms, Some(5000));
        t.update(vec![mouse.clone()], 7000);
        assert_eq!(
            t.update(vec![mouse, buds], 9000)[1].connected_ms,
            Some(9000)
        );
    }
    #[test]
    fn low_battery_fires_once_per_discharge_at_each_level() {
        let mut t = Tracker::default();
        let at = |b: u8, charging: bool| Device {
            charging: Some(charging),
            ..device("bt:1", "Mouse", Kind::Mouse, Some(b))
        };
        let alert = |d: &[Device]| (d[0].alert, d[0].alert_ms);
        assert_eq!(alert(&t.update(vec![at(40, false)], 0)), (None, None));
        assert_eq!(alert(&t.update(vec![at(21, false)], 1)), (None, None));
        assert_eq!(
            alert(&t.update(vec![at(20, false)], 2)),
            (Some(20), Some(2))
        );
        // Further drops under 20 are the same alert.
        assert_eq!(
            alert(&t.update(vec![at(15, false)], 3)),
            (Some(20), Some(2))
        );
        assert_eq!(
            alert(&t.update(vec![at(10, false)], 4)),
            (Some(10), Some(4))
        );
        assert_eq!(alert(&t.update(vec![at(5, false)], 5)), (Some(10), Some(4)));
        // Charging does not fire, and re-arms each level once above it.
        assert_eq!(alert(&t.update(vec![at(9, true)], 6)), (Some(10), Some(4)));
        assert_eq!(alert(&t.update(vec![at(50, true)], 7)), (Some(10), Some(4)));
        assert_eq!(
            alert(&t.update(vec![at(20, false)], 8)),
            (Some(20), Some(8))
        );
        // Crossing both at once gives the lower one.
        let mut t = Tracker::default();
        t.update(vec![at(30, false)], 0);
        assert_eq!(alert(&t.update(vec![at(8, false)], 1)), (Some(10), Some(1)));
        // A level already under a threshold when first read never fires.
        let mut t = Tracker::default();
        t.update(vec![device("bt:1", "Mouse", Kind::Mouse, None)], 0);
        assert_eq!(alert(&t.update(vec![at(15, false)], 1)), (None, None));
        assert_eq!(alert(&t.update(vec![at(9, false)], 2)), (Some(10), Some(2)));
        assert_eq!(at(15, false).low(), Some(20));
        assert_eq!(at(9, false).low(), Some(10));
        assert_eq!(at(9, true).low(), None);
    }
    #[test]
    fn only_list_changing_signals_cause_a_read() {
        let changed = |path: &str, interface: &str, key: &str| {
            let mut c = Props::new();
            c.insert(key.to_string(), OwnedValue::from(1u32));
            zbus::Message::signal(path, "org.freedesktop.DBus.Properties", "PropertiesChanged")
                .unwrap()
                .build(&(interface, c, Vec::<String>::new()))
                .unwrap()
        };
        let mouse = "/org/freedesktop/UPower/devices/mouse_dev_AA";
        assert!(relevant(&changed(mouse, UPOWER_DEVICE, "Percentage")));
        assert!(!relevant(&changed(mouse, UPOWER_DEVICE, "UpdateTime")));
        assert!(!relevant(&changed(
            "/org/freedesktop/UPower/devices/battery_BAT0",
            UPOWER_DEVICE,
            "Percentage"
        )));
        assert!(relevant(&changed(
            "/org/bluez/hci0/dev_AA",
            BLUEZ_DEVICE,
            "Connected"
        )));
        assert!(!relevant(&changed(
            "/org/bluez/hci0/dev_AA",
            BLUEZ_DEVICE,
            "RSSI"
        )));
        assert!(relevant(&changed(
            "/org/bluez/hci0/dev_AA",
            BLUEZ_BATTERY,
            "Percentage"
        )));
        let added = zbus::Message::signal("/org/freedesktop/UPower", UPOWER, "DeviceAdded")
            .unwrap()
            .build(&(OwnedObjectPath::try_from("/x").unwrap(),))
            .unwrap();
        assert!(relevant(&added));
        let refreshed = zbus::Message::signal(
            "/modules/kdeconnect/devices/a/battery",
            "org.kde.kdeconnect.device.battery",
            "refreshed",
        )
        .unwrap()
        .build(&(true, 50i32))
        .unwrap();
        assert!(relevant(&refreshed));
        let media = zbus::Message::signal(
            "/modules/kdeconnect/devices/a/mprisremote",
            "org.kde.kdeconnect.device.mprisremote",
            "propertiesChanged",
        )
        .unwrap()
        .build(&())
        .unwrap();
        assert!(!relevant(&media));
    }

    /// The bus delivers only the signals a read could follow: not UPower's
    /// own properties (such as OnBattery) or other signals it sends.
    #[tokio::test]
    async fn only_the_signals_read_reach_the_watch() {
        use cosmic::iced::futures::StreamExt;
        let bus = crate::activity_island::test_bus::Bus::new();
        let service = zbus::connection::Builder::address(bus.address.as_str())
            .unwrap()
            .name(UPOWER)
            .unwrap()
            .build()
            .await
            .unwrap();
        let client = zbus::connection::Builder::address(bus.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        let mut changes = watch(&client, &[UPOWER]).await.unwrap();
        let path = "/org/freedesktop/UPower";
        let props = HashMap::from([("OnBattery", zbus::zvariant::Value::from(true))]);
        for _ in 0..20 {
            service
                .emit_signal(
                    None::<()>,
                    path,
                    "org.freedesktop.DBus.Properties",
                    "PropertiesChanged",
                    &(UPOWER, &props, Vec::<&str>::new()),
                )
                .await
                .unwrap();
            service
                .emit_signal(None::<()>, path, UPOWER, "Changed", &())
                .await
                .unwrap();
        }
        let device =
            zbus::zvariant::ObjectPath::try_from("/org/freedesktop/UPower/devices/mouse_0")
                .unwrap();
        service
            .emit_signal(None::<()>, path, UPOWER, "DeviceAdded", &(device,))
            .await
            .unwrap();
        let first = tokio::time::timeout(Duration::from_secs(3), changes.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(first.header().member().unwrap().as_str(), "DeviceAdded");
        assert!(
            tokio::time::timeout(Duration::from_millis(200), changes.next())
                .await
                .is_err()
        );
    }
}
