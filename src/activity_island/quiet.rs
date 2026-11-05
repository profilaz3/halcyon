//! Conservative automatic-reveal gates. Observation loss always closes the gate.
use cosmic::iced::{
    Subscription,
    futures::{SinkExt, StreamExt},
};
use std::time::Duration;
mod fullscreen;
#[derive(Clone, Copy)]
enum Part {
    Fullscreen(bool),
    /// Do Not Disturb, then locked or inactive.
    Policy(bool, bool),
}
/// Do Not Disturb is kept apart: critical notifications still reveal there,
/// while fullscreen and the lock screen hold everything back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quiet {
    pub quiet: bool,
    pub dnd: bool,
}
pub fn subscription() -> Subscription<Quiet> {
    if crate::tools::preview() {
        return Subscription::none();
    }
    Subscription::run_with((), |_| {
        cosmic::iced::stream::channel(1, async |mut out| {
            let (tx, mut rx) = tokio::sync::mpsc::channel(4);
            let mut tasks = tokio::task::JoinSet::new();
            tasks.spawn(fullscreen::observe(tx.clone()));
            tasks.spawn(policy(tx));
            // Quiet until known; Do Not Disturb is shown, so off until read.
            let (mut fullscreen, mut dnd, mut locked) = (true, false, true);
            let mut last = None;
            while let Some(part) = rx.recv().await {
                match part {
                    Part::Fullscreen(v) => fullscreen = v,
                    Part::Policy(d, l) => (dnd, locked) = (d, l),
                }
                let quiet = Quiet {
                    quiet: fullscreen || locked,
                    dnd,
                };
                if last != Some(quiet) {
                    last = Some(quiet);
                    if out.send(quiet).await.is_err() {
                        break;
                    }
                }
            }
        })
    })
}
/// COSMIC's Do Not Disturb. Unreadable reads as off, as COSMIC's own
/// default: the island shows this switch, and the lock and fullscreen gates
/// stay conservative on their own.
fn dnd() -> bool {
    use crate::desktop::store::{ConfigKey, DesktopStore};
    let Ok(key) = ConfigKey::new("com.system76.CosmicNotifications", 1, "do_not_disturb") else {
        return false;
    };
    DesktopStore::from_env()
        .ok()
        .and_then(|s| s.load(std::slice::from_ref(&key)).ok())
        .and_then(|s| s.effective(&key).and_then(|v| v.decode::<bool>().ok()))
        .unwrap_or(false)
}
async fn locked(c: &zbus::Connection) -> bool {
    tokio::time::timeout(Duration::from_secs(2), async {
        let root = zbus::Proxy::new(
            c,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .await
        .ok()?;
        let pid = std::process::id();
        let path = match root
            .call::<_, _, zbus::zvariant::OwnedObjectPath>("GetSessionByPID", &(pid,))
            .await
        {
            Ok(path) => path,
            Err(_) => {
                let user: zbus::zvariant::OwnedObjectPath = root
                    .call("GetUser", &(unsafe { libc::geteuid() },))
                    .await
                    .ok()?;
                let user = zbus::Proxy::new(
                    c,
                    "org.freedesktop.login1",
                    user,
                    "org.freedesktop.login1.User",
                )
                .await
                .ok()?;
                let (id, path): (String, zbus::zvariant::OwnedObjectPath) =
                    user.get_property("Display").await.ok()?;
                if id.is_empty() {
                    return None;
                }
                path
            }
        };
        let session = zbus::Proxy::new(
            c,
            "org.freedesktop.login1",
            path,
            "org.freedesktop.login1.Session",
        )
        .await
        .ok()?;
        let active: bool = session.get_property("Active").await.ok()?;
        let locked: bool = session.get_property("LockedHint").await.ok()?;
        Some(!active || locked)
    })
    .await
    .ok()
    .flatten()
    .unwrap_or(true)
}
async fn policy(tx: tokio::sync::mpsc::Sender<Part>) {
    use notify::Watcher;
    let (changed, mut files) = tokio::sync::mpsc::channel(1);
    let watched = dirs::config_dir()
        .map(|d| d.join("cosmic/com.system76.CosmicNotifications/v1/do_not_disturb"));
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.as_ref().is_err()
            || event.as_ref().is_ok_and(|e| {
                e.paths
                    .iter()
                    .any(|p| watched.as_ref().is_some_and(|w| w.starts_with(p)))
            })
        {
            let _ = changed.try_send(());
        }
    })
    .ok();
    if let (Some(w), Some(dir)) = (&mut watcher, dirs::config_dir()) {
        let _ = w.watch(&dir.join("cosmic"), notify::RecursiveMode::Recursive);
    }
    loop {
        let c = tokio::time::timeout(Duration::from_secs(3), zbus::Connection::system())
            .await
            .ok()
            .and_then(Result::ok);
        if let Some(c) = c {
            let rule = zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender("org.freedesktop.login1")
                .expect("constant")
                .build();
            if let Ok(mut changes) = zbus::MessageStream::for_match_rule(rule, &c, Some(8)).await {
                loop {
                    if tx
                        .send(Part::Policy(dnd(), locked(&c).await))
                        .await
                        .is_err()
                    {
                        return;
                    }
                    tokio::select! {
                        _=tx.closed()=>return,
                        m=changes.next()=>{if !matches!(m,Some(Ok(_))){break}},
                        Some(_)=files.recv()=>{},
                        _=tokio::time::sleep(Duration::from_secs(30))=>{},
                    }
                }
            }
        }
        if tx.send(Part::Policy(dnd(), true)).await.is_err() {
            return;
        }
        tokio::select! {_=tx.closed()=>return,_=tokio::time::sleep(Duration::from_secs(10))=>{}}
    }
}
