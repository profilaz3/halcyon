//! Property signals, with finite read timeouts and no package installation calls.
use super::*;
use cosmic::iced::futures::StreamExt;
use std::collections::HashMap;
use zbus::{
    Connection, MatchRule, MessageStream, Proxy,
    message::Type,
    zvariant::{OwnedObjectPath, OwnedValue},
};
type Props = HashMap<String, OwnedValue>;
fn number(p: &Props, k: &str) -> Option<u32> {
    p.get(k).and_then(|v| u32::try_from(v).ok())
}
fn word(p: &Props, k: &str) -> String {
    p.get(k)
        .and_then(|v| <&str>::try_from(v).ok())
        .map(clean)
        .unwrap_or_default()
}
async fn props(c: &Connection, service: &str, path: &str, interface: &str) -> Option<Props> {
    tokio::time::timeout(Duration::from_secs(2), async {
        Proxy::new(c, service, path, "org.freedesktop.DBus.Properties")
            .await
            .ok()?
            .call_with_flags(
                "GetAll",
                zbus::proxy::MethodFlags::NoAutoStart.into(),
                &(interface,),
            )
            .await
            .ok()
            .flatten()
    })
    .await
    .ok()
    .flatten()
}
pub(super) async fn owner(c: &Connection, service: &str) -> bool {
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
#[derive(Clone, Debug, PartialEq)]
enum Raw {
    Power {
        state: u32,
        level: u32,
        percent: Option<u8>,
    },
    Network {
        state: u32,
        vpn: String,
    },
    Updates {
        ids: Vec<String>,
        count: usize,
        phase: u32,
        progress: Option<u8>,
    },
}
#[derive(Default)]
struct Tracker {
    last: Option<Raw>,
    current: Option<Activity>,
    completion: Option<Activity>,
}
impl Tracker {
    fn signal(&mut self, message: &zbus::Message, now: i64) {
        let h = message.header();
        if h.interface().map(|s| s.as_str()) != Some("org.freedesktop.PackageKit.Transaction")
            || h.member().map(|s| s.as_str()) != Some("Finished")
        {
            return;
        }
        let Some(path) = h.path() else { return };
        if !matches!(&self.last, Some(Raw::Updates { ids, .. }) if ids.iter().any(|id|id==path.as_str()))
        {
            return;
        }
        let Ok((exit, _runtime)) = message.body().deserialize::<(u32, u32)>() else {
            return;
        };
        let title = match exit {
            1 => "Software operation complete",
            2 => "Software operation failed",
            3 => "Software operation cancelled",
            _ => "Software operation ended",
        };
        self.completion = Some(Activity::new(
            Source::Updates,
            title,
            "Open the updater for details",
            String::new(),
            None,
            false,
            Some(now + 3000),
        ));
    }
    fn update(&mut self, source: Source, raw: Option<Raw>, now: i64) -> Option<Activity> {
        let Some(raw) = raw else {
            self.last = None;
            self.current = None;
            self.completion = None;
            return None;
        };
        if self.last.as_ref() == Some(&raw) {
            return self.current.clone();
        }
        let seen = self.last.is_some();
        let activity = match &raw {
            Raw::Power {
                state,
                level,
                percent,
            } => {
                let cue = percent.map(|p| format!("{p}%")).unwrap_or_default();
                if *level >= 3 && !matches!(state, 1 | 4 | 5) {
                    Some(Activity::new(
                        source,
                        if *level >= 4 {
                            "Battery critically low"
                        } else {
                            "Battery low"
                        },
                        "Connect a power source",
                        cue,
                        None,
                        *level >= 4,
                        None,
                    ))
                } else if seen
                    && matches!(state, 1 | 4)
                    && !matches!(&self.last,Some(Raw::Power{state:old,..}) if old==state)
                {
                    Some(Activity::new(
                        source,
                        if *state == 4 {
                            "Fully charged"
                        } else {
                            "Charging"
                        },
                        "Power connected",
                        cue,
                        *percent,
                        false,
                        Some(now + 3000),
                    ))
                } else {
                    None
                }
            }
            Raw::Network { state, vpn } => {
                if matches!(state, 40) {
                    Some(Activity::new(
                        source,
                        "Connecting",
                        "Network connection in progress",
                        String::new(),
                        None,
                        false,
                        None,
                    ))
                } else if seen && self.last.as_ref() != Some(&raw) {
                    let old_vpn = match &self.last {
                        Some(Raw::Network { vpn, .. }) => vpn.as_str(),
                        _ => "",
                    };
                    let (title, detail) = if vpn != old_vpn {
                        if vpn.is_empty() {
                            ("VPN disconnected", "Open network settings".to_owned())
                        } else {
                            ("VPN connected", vpn.clone())
                        }
                    } else if *state >= 60 {
                        ("Connected", "Network available".into())
                    } else {
                        ("Disconnected", "Open network settings".into())
                    };
                    Some(Activity::new(
                        source,
                        title,
                        &detail,
                        String::new(),
                        None,
                        false,
                        Some(now + 3000),
                    ))
                } else {
                    None
                }
            }
            Raw::Updates {
                count,
                phase,
                progress,
                ..
            } => {
                if *count == 0 {
                    // A vanished transaction is unknown. Only its own Finished
                    // signal, observed while running, can supply an outcome.
                    self.completion
                        .clone()
                        .filter(|a| a.expires_ms.is_some_and(|end| end > now))
                } else {
                    let title = match phase {
                        8 | 20..=25 => "Downloading updates",
                        9 => "Installing software",
                        10 => "Updating software",
                        30 => "Waiting for package manager",
                        31 => "Authorization required",
                        _ => "Software operation",
                    };
                    Some(Activity::new(
                        source,
                        title,
                        if *count == 1 {
                            "Open the updater for details"
                        } else {
                            "Several package operations are active"
                        },
                        progress
                            .map(|p| format!("{p}%"))
                            .unwrap_or_else(|| "Working".into()),
                        *progress,
                        false,
                        None,
                    ))
                }
            }
        };
        let mut activity = activity;
        if !seen {
            if let Some(a) = &mut activity {
                a.announce = false;
            }
        }
        self.last = Some(raw);
        self.current = activity.clone();
        activity
    }
}
const POWER_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const POWER_DEVICE: &str = "org.freedesktop.UPower.Device";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const NM: &str = "org.freedesktop.NetworkManager";
const NM_ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const PK_PATH: &str = "/org/freedesktop/PackageKit";
const PK: &str = "org.freedesktop.PackageKit";
const PK_TRANSACTION: &str = "org.freedesktop.PackageKit.Transaction";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

/// What a source's service last said: its main object's properties and,
/// for networks and updates, those of each active connection or package
/// transaction, in the service's order. Kept current from the service's
/// own change signals, so a change costs no read.
#[derive(Clone, Debug, Default, PartialEq)]
struct Model {
    root: Props,
    children: Vec<(String, Props)>,
}
/// Reads a source's model in full, as at start and after the service
/// restarts or its list of connections or transactions changes.
async fn load(c: &Connection, source: Source, service: &str) -> Option<Model> {
    if !owner(c, service).await {
        return None;
    }
    let mut children = Vec::new();
    let root = match source {
        Source::Power => props(c, service, POWER_PATH, POWER_DEVICE).await?,
        Source::Network => {
            let root = props(c, service, NM_PATH, NM).await?;
            let paths = root
                .get("ActiveConnections")
                .and_then(|v| v.try_clone().ok())
                .and_then(|v| Vec::<OwnedObjectPath>::try_from(v).ok())
                .unwrap_or_default();
            for path in paths.iter().take(8) {
                if let Some(a) = props(c, service, path.as_str(), NM_ACTIVE).await {
                    children.push((path.to_string(), a));
                }
            }
            root
        }
        Source::Updates => {
            let root = Proxy::new(c, service, PK_PATH, PK).await.ok()?;
            let paths: Vec<OwnedObjectPath> = tokio::time::timeout(
                Duration::from_secs(2),
                root.call_with_flags(
                    "GetTransactionList",
                    zbus::proxy::MethodFlags::NoAutoStart.into(),
                    &(),
                ),
            )
            .await
            .ok()?
            .ok()??;
            for path in paths.iter().take(16) {
                if let Some(p) = props(c, service, path.as_str(), PK_TRANSACTION).await {
                    children.push((path.to_string(), p));
                }
            }
            Props::new()
        }
        _ => return None,
    };
    Some(Model { root, children })
}
/// What the island shows of a model.
fn raw(source: Source, model: &Model) -> Option<Raw> {
    let p = &model.root;
    match source {
        Source::Power => {
            if !p
                .get("IsPresent")
                .and_then(|v| bool::try_from(v).ok())
                .unwrap_or(false)
            {
                return None;
            }
            let pct = p
                .get("Percentage")
                .and_then(|v| f64::try_from(v).ok())
                .filter(|p| p.is_finite() && (0.0..=100.0).contains(p))
                .map(|p| p.round() as u8);
            Some(Raw::Power {
                state: number(p, "State")?,
                level: number(p, "WarningLevel").unwrap_or(0),
                percent: if number(p, "BatteryLevel").unwrap_or(0) == 0 {
                    pct
                } else {
                    None
                },
            })
        }
        Source::Network => {
            let state = number(p, "State")?;
            let vpn = model
                .children
                .iter()
                .map(|(_, a)| a)
                .find(|a| {
                    matches!(word(a, "Type").as_str(), "vpn" | "wireguard")
                        && number(a, "State") == Some(2)
                })
                .map(|a| word(a, "Id"))
                .unwrap_or_default();
            Some(Raw::Network { state, vpn })
        }
        Source::Updates => {
            let mut running = Vec::new();
            let mut ids = Vec::new();
            for (path, p) in &model.children {
                if !number(p, "Uid")
                    .is_some_and(|uid| uid == 0 || uid == unsafe { libc::geteuid() })
                {
                    continue;
                }
                if !matches!(number(p, "Role"), Some(10 | 11 | 14 | 22 | 25 | 29 | 33)) {
                    continue;
                }
                if matches!(number(p, "Status"), Some(18 | 19)) {
                    continue;
                }
                ids.push(path.clone());
                running.push((
                    number(p, "Status").unwrap_or(0),
                    number(p, "Percentage")
                        .filter(|p| *p <= 100)
                        .map(|p| p as u8),
                ));
            }
            let (phase, progress) = running.first().copied().unwrap_or((0, None));
            Some(Raw::Updates {
                ids,
                count: running.len(),
                phase,
                progress,
            })
        }
        _ => None,
    }
}
#[cfg(test)]
async fn read(c: &Connection, source: Source, service: &str) -> Option<Raw> {
    raw(source, &load(c, source, service).await?)
}
/// What a signal means for a model.
#[derive(Debug, PartialEq)]
enum Step {
    /// Nothing shown can have changed.
    Same,
    /// The model changed in place.
    Changed,
    /// The model must be read again.
    Reload,
}
/// Applies one of the source's signals to its model.
fn apply(source: Source, model: &mut Model, message: &zbus::Message) -> Step {
    let header = message.header();
    let path = header.path().map(|p| p.as_str()).unwrap_or_default();
    match header.member().map(|m| m.as_str()) {
        Some("TransactionListChanged") if source == Source::Updates => Step::Reload,
        // The completion it announces is the tracker's; show it now.
        Some("Finished") if source == Source::Updates => Step::Changed,
        Some("PropertiesChanged") => {
            let Ok((interface, changed, invalidated)) =
                message.body().deserialize::<(String, Props, Vec<String>)>()
            else {
                return Step::Same;
            };
            let target = match (source, interface.as_str()) {
                (Source::Power, POWER_DEVICE) if path == POWER_PATH => Some(&mut model.root),
                (Source::Network, NM) if path == NM_PATH => {
                    if changed.contains_key("ActiveConnections") {
                        return Step::Reload;
                    }
                    Some(&mut model.root)
                }
                (Source::Network, NM_ACTIVE) | (Source::Updates, PK_TRANSACTION) => model
                    .children
                    .iter_mut()
                    .find(|(p, _)| p == path)
                    .map(|(_, props)| props),
                _ => None,
            };
            let Some(target) = target else {
                return Step::Same;
            };
            if !invalidated.is_empty() {
                return Step::Reload;
            }
            let mut step = Step::Same;
            for (key, value) in changed {
                if target.get(&key) != Some(&value) {
                    target.insert(key, value);
                    step = Step::Changed;
                }
            }
            step
        }
        _ => Step::Same,
    }
}
/// The signals a source listens to: property changes of the objects it
/// shows, on their own interfaces, and PackageKit's list and completion
/// signals. Not every signal the service sends: NetworkManager alone
/// reports signal strength and traffic many times a minute.
fn rules(source: Source, service: &'static str) -> zbus::Result<Vec<MatchRule<'static>>> {
    let changed = |path: Option<&'static str>, interface: &'static str| {
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(service)?
            .interface(PROPERTIES)?
            .member("PropertiesChanged")?;
        let rule = match path {
            Some(path) => rule.path(path)?,
            None => rule,
        };
        Ok::<_, zbus::Error>(rule.arg(0, interface)?.build())
    };
    let signal = |path: Option<&'static str>, interface: &'static str, member: &'static str| {
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(service)?
            .interface(interface)?
            .member(member)?;
        Ok::<_, zbus::Error>(
            match path {
                Some(path) => rule.path(path)?,
                None => rule,
            }
            .build(),
        )
    };
    Ok(match source {
        Source::Power => vec![changed(Some(POWER_PATH), POWER_DEVICE)?],
        Source::Network => vec![changed(Some(NM_PATH), NM)?, changed(None, NM_ACTIVE)?],
        Source::Updates => vec![
            signal(Some(PK_PATH), PK, "TransactionListChanged")?,
            changed(None, PK_TRANSACTION)?,
            signal(None, PK_TRANSACTION, "Finished")?,
        ],
        _ => Vec::new(),
    })
}
/// Follows one source on `c` until the bus fails (`Some`) or the island
/// goes away (`None`). Reads in full only at start, when the service comes
/// or goes and when its list of connections or transactions changes;
/// otherwise each change signal updates the model in place. No polling.
async fn follow(
    c: &Connection,
    source: Source,
    service: &'static str,
    tx: &tokio::sync::mpsc::Sender<(Source, Option<Activity>)>,
) -> Option<()> {
    let owners = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender("org.freedesktop.DBus")
        .ok()?
        .member("NameOwnerChanged")
        .ok()?
        .arg(0, service)
        .ok()?
        .build();
    let mut owners = MessageStream::for_match_rule(owners, c, Some(4))
        .await
        .ok()?;
    let mut streams = Vec::new();
    for rule in rules(source, service).ok()? {
        streams.push(
            MessageStream::for_match_rule(rule, c, Some(16))
                .await
                .ok()?,
        );
    }
    let mut changes = cosmic::iced::futures::stream::select_all(streams);
    let mut tracker = Tracker::default();
    let mut model = None;
    let mut reload = true;
    let now = || chrono::Utc::now().timestamp_millis();
    loop {
        if reload {
            model = tokio::time::timeout(Duration::from_secs(5), load(c, source, service))
                .await
                .ok()
                .flatten();
            reload = false;
        }
        let state = tracker.update(source, model.as_ref().and_then(|m| raw(source, m)), now());
        tx.send((source, state)).await.ok()?;
        loop {
            tokio::select! {
                _ = tx.closed() => return None,
                msg = changes.next() => {
                    if take(source, &mut tracker, &mut model, &msg?.ok()?, &mut reload) {
                        break;
                    }
                }
                msg = owners.next() => {
                    msg?.ok()?;
                    tracker = Tracker::default();
                    reload = true;
                    break;
                }
            }
        }
        // Bound redraw and read bursts: take what else arrived meanwhile.
        tokio::time::sleep(Duration::from_millis(200)).await;
        for _ in 0..16 {
            match changes.next().now_or_never() {
                Some(Some(Ok(m))) => {
                    take(source, &mut tracker, &mut model, &m, &mut reload);
                }
                Some(Some(Err(_))) | Some(None) => return None,
                None => break,
            }
        }
    }
}
/// Takes one signal into the tracker and the model; answers whether what
/// the island shows can have changed. Without a model, any signal reads.
fn take(
    source: Source,
    tracker: &mut Tracker,
    model: &mut Option<Model>,
    message: &zbus::Message,
    reload: &mut bool,
) -> bool {
    tracker.signal(message, chrono::Utc::now().timestamp_millis());
    let step = match model {
        Some(m) => apply(source, m, message),
        None => Step::Reload,
    };
    *reload |= step == Step::Reload;
    step != Step::Same
}
pub(super) async fn observe(
    source: Source,
    tx: tokio::sync::mpsc::Sender<(Source, Option<Activity>)>,
) {
    let service = match source {
        Source::Power => "org.freedesktop.UPower",
        Source::Network => "org.freedesktop.NetworkManager",
        Source::Updates => "org.freedesktop.PackageKit",
        _ => return,
    };
    loop {
        if let Ok(Ok(c)) = tokio::time::timeout(Duration::from_secs(3), Connection::system()).await
        {
            follow(&c, source, service, &tx).await;
        }
        if tx.send((source, None)).await.is_err() {
            return;
        }
        tokio::select! {_=tx.closed()=>return,_=tokio::time::sleep(Duration::from_secs(10))=>{}}
    }
}
use cosmic::iced::futures::FutureExt;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reactive_power_discovery_is_silent_and_transitions_expire() {
        let mut t = Tracker::default();
        let p = |state| {
            Some(Raw::Power {
                state,
                level: 1,
                percent: Some(62),
            })
        };
        assert!(t.update(Source::Power, p(2), 1000).is_none());
        let a = t.update(Source::Power, p(1), 2000).unwrap();
        assert_eq!(a.expires_ms, Some(5000));
        assert!(!a.urgent);
        assert!(t.update(Source::Power, None, 3000).is_none());
        assert!(t.update(Source::Power, p(1), 4000).is_none());
    }
    #[test]
    fn reactive_low_battery_remains_and_unknown_does_not_become_success() {
        let mut t = Tracker::default();
        let a = t
            .update(
                Source::Power,
                Some(Raw::Power {
                    state: 2,
                    level: 4,
                    percent: None,
                }),
                0,
            )
            .unwrap();
        assert!(a.urgent);
        assert!(a.cue.is_empty());
        assert!(t.update(Source::Power, None, 1).is_none());
        let mut t = Tracker::default();
        assert!(
            t.update(
                Source::Updates,
                Some(Raw::Updates {
                    ids: vec![],
                    count: 1,
                    phase: 10,
                    progress: None
                }),
                0
            )
            .unwrap()
            .progress
            .is_none()
        );
        assert!(
            t.update(
                Source::Updates,
                Some(Raw::Updates {
                    ids: vec![],
                    count: 0,
                    phase: 0,
                    progress: None
                }),
                1
            )
            .is_none()
        );
    }
}
#[cfg(test)]
#[path = "bus/tests.rs"]
mod connection_tests;
