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
    crate::desktop::store::DesktopStore::from_env().is_ok_and(|s| dnd_in(&s))
}
fn dnd_in(store: &crate::desktop::store::DesktopStore) -> bool {
    use crate::desktop::store::ConfigKey;
    let Ok(key) = ConfigKey::new("com.system76.CosmicNotifications", 1, "do_not_disturb") else {
        return false;
    };
    store
        .load(std::slice::from_ref(&key))
        .ok()
        .and_then(|s| s.effective(&key).and_then(|v| v.decode::<bool>().ok()))
        .unwrap_or(false)
}
/// Do Not Disturb's key under COSMIC's config folder `cosmic`.
fn dnd_key(cosmic: &std::path::Path) -> std::path::PathBuf {
    cosmic.join("com.system76.CosmicNotifications/v1/do_not_disturb")
}
/// Whether a change to the entry `path` can have changed Do Not Disturb:
/// the key itself, or a folder on the way to it being made, renamed or
/// removed. Other settings in the same folders are not.
fn dnd_changed(path: &std::path::Path, key: &std::path::Path) -> bool {
    key.starts_with(path)
}
/// Watches the folders that lead to Do Not Disturb's key, and only those:
/// COSMIC's config folder (for the namespace appearing), the namespace and
/// its version folder. Not recursive, so other apps' settings never wake
/// it, and never for reads: `dnd` opens the key and each folder above it,
/// and a watch that counted opens woke itself on every read. Folders made
/// later are watched by [`rewatch_dnd`] after the change that made them.
fn watch_dnd(cosmic: &std::path::Path) -> Option<crate::watch::dir::DirWatch> {
    let mut watch = crate::watch::dir::DirWatch::new().ok()?;
    rewatch_dnd(&mut watch, cosmic);
    Some(watch)
}
fn rewatch_dnd(watch: &mut crate::watch::dir::DirWatch, cosmic: &std::path::Path) {
    let key = dnd_key(cosmic);
    for dir in key.ancestors().skip(1) {
        let _ = watch.watch(dir);
        if dir == cosmic {
            break;
        }
    }
}
/// The Do Not Disturb watch and the COSMIC config folder it watches.
type DndWatch = Option<(crate::watch::dir::DirWatch, std::path::PathBuf)>;
/// Waits for a change that can affect Do Not Disturb; never with no watch.
async fn dnd_wait(watch: &mut DndWatch) -> std::io::Result<()> {
    match watch {
        Some((watch, cosmic)) => {
            let key = dnd_key(cosmic);
            watch.changed(|path| dnd_changed(path, &key)).await
        }
        None => std::future::pending().await,
    }
}
const LOGIN1: &str = "org.freedesktop.login1";
/// This process's logind session. The island runs in the user's app slice,
/// which need not belong to a session, so the user's graphical Display
/// session stands in when the PID has none.
async fn session(c: &zbus::Connection) -> Option<zbus::Proxy<'static>> {
    let manager = zbus::Proxy::new(
        c,
        LOGIN1,
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .await
    .ok()?;
    let path = match manager
        .call::<_, _, zbus::zvariant::OwnedObjectPath>("GetSessionByPID", &(std::process::id(),))
        .await
    {
        Ok(path) => path,
        Err(_) => {
            // SAFETY: geteuid has no preconditions.
            let uid = unsafe { libc::geteuid() };
            let user: zbus::zvariant::OwnedObjectPath =
                manager.call("GetUser", &(uid,)).await.ok()?;
            let user = zbus::Proxy::new(c, LOGIN1, user, "org.freedesktop.login1.User")
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
    zbus::Proxy::new(c, LOGIN1, path, "org.freedesktop.login1.Session")
        .await
        .ok()
}
/// Reads Active and LockedHint from logind itself, past the proxy's cache.
async fn read_state(session: &zbus::Proxy<'_>) -> Option<(bool, bool)> {
    let properties = zbus::fdo::PropertiesProxy::builder(session.connection())
        .destination(session.destination().to_owned())
        .ok()?
        .path(session.path().to_owned())
        .ok()?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .ok()?;
    let interface = session.interface().to_owned();
    let all = properties.get_all(interface).await.ok()?;
    let flag = |name: &str| all.get(name).and_then(|v| bool::try_from(v).ok());
    Some((flag("Active")?, flag("LockedHint")?))
}
/// How following the session ended.
enum Ended {
    /// The island went away.
    Closed,
    /// The bus or logind did; reconnect.
    Lost,
}
/// Follows the session on `c`: resolved once, read once, then kept current
/// from its own LockedHint and Active changes and its Lock and Unlock
/// signals. No polling: nothing is read again until logind says something
/// changed. Sends Do Not Disturb (`dnd`) with each change, and again when
/// its file changes.
async fn follow(
    c: &zbus::Connection,
    tx: &tokio::sync::mpsc::Sender<Part>,
    files: &mut DndWatch,
    dnd: &impl Fn() -> bool,
) -> Ended {
    let followed = async {
        let session = tokio::time::timeout(Duration::from_secs(2), session(c))
            .await
            .ok()??;
        let owner = session.receive_owner_changed().await.ok()?;
        let hint = session.receive_property_changed::<bool>("LockedHint").await;
        let active = session.receive_property_changed::<bool>("Active").await;
        let lock = session.receive_signal("Lock").await.ok()?;
        let unlock = session.receive_signal("Unlock").await.ok()?;
        let state = tokio::time::timeout(Duration::from_secs(2), read_state(&session))
            .await
            .ok()??;
        Some((session, owner, hint, active, lock, unlock, state))
    };
    let Some((session, mut owner, mut hint, mut active, mut lock, mut unlock, mut state)) =
        followed.await
    else {
        return Ended::Lost;
    };
    // Unknown after a failed read: the gate closes, as on any loss.
    let reread = async |session: &zbus::Proxy<'_>| {
        tokio::time::timeout(Duration::from_secs(2), read_state(session))
            .await
            .ok()
            .flatten()
            .unwrap_or((false, true))
    };
    let mut sent = None;
    loop {
        // The property streams start with the value already known, and a
        // change can be announced twice; only news is sent.
        let now = (dnd(), !state.0 || state.1);
        if sent != Some(now) {
            sent = Some(now);
            if tx.send(Part::Policy(now.0, now.1)).await.is_err() {
                return Ended::Closed;
            }
        }
        tokio::select! {
            _ = tx.closed() => return Ended::Closed,
            _ = owner.next() => return Ended::Lost,
            // PropertiesChanged carries the new value: no round trip.
            Some(v) = hint.next() => state.1 = v.get().await.unwrap_or(true),
            Some(v) = active.next() => state.0 = v.get().await.unwrap_or(false),
            Some(_) = lock.next() => state = reread(&session).await,
            Some(_) = unlock.next() => state = reread(&session).await,
            // A Do Not Disturb change needs no logind round trip.
            // Coalesce the write/rename pair, as `watch::files` does.
            woke = dnd_wait(files), if files.is_some() => {
                if woke.is_err() { *files = None; }
                tokio::time::sleep(Duration::from_millis(25)).await;
                // A folder on the way to the key may be new.
                if let Some((watch, cosmic)) = files {
                    rewatch_dnd(watch, cosmic);
                }
            }
            else => return Ended::Lost,
        }
    }
}
async fn policy(tx: tokio::sync::mpsc::Sender<Part>) {
    let mut files: DndWatch = dirs::config_dir()
        .map(|dir| dir.join("cosmic"))
        .and_then(|cosmic| Some((watch_dnd(&cosmic)?, cosmic)));
    loop {
        let c = tokio::time::timeout(Duration::from_secs(3), zbus::Connection::system())
            .await
            .ok()
            .and_then(Result::ok);
        if let Some(c) = c
            && let Ended::Closed = follow(&c, &tx, &mut files, &dnd).await
        {
            return;
        }
        if tx.send(Part::Policy(dnd(), true)).await.is_err() {
            return;
        }
        tokio::select! {_=tx.closed()=>return,_=tokio::time::sleep(Duration::from_secs(10))=>{}}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::store::DesktopStore;
    use std::path::{Path, PathBuf};

    fn root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "halcyon-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("config/cosmic/com.system76.CosmicNotifications/v1"))
            .unwrap();
        std::fs::create_dir_all(root.join("system/cosmic")).unwrap();
        root
    }
    fn key(cosmic: &Path) -> PathBuf {
        dnd_key(cosmic)
    }
    /// What `policy` does with the watch: every wake reads the key again.
    /// Returns the wakes before the watch stays quiet for 300 ms, at most 50.
    async fn wakes_until_quiet(watch: &mut DndWatch, store: &DesktopStore) -> usize {
        let mut wakes = 0;
        while wakes < 50
            && tokio::time::timeout(Duration::from_millis(300), dnd_wait(watch))
                .await
                .is_ok_and(|w| w.is_ok())
        {
            wakes += 1;
            dnd_in(store);
        }
        wakes
    }

    #[test]
    fn only_changes_to_the_key_or_a_folder_above_it_count() {
        let cosmic = Path::new("/home/a/.config/cosmic");
        let key = key(cosmic);
        assert!(dnd_changed(&key, &key));
        assert!(dnd_changed(key.parent().unwrap(), &key));
        assert!(dnd_changed(
            &cosmic.join("com.system76.CosmicNotifications"),
            &key
        ));
        // Other keys and namespaces are not.
        assert!(!dnd_changed(&key.with_file_name("max_notifications"), &key));
        assert!(!dnd_changed(&cosmic.join("com.system76.CosmicPanel"), &key));
    }

    /// The idle CPU bug in 0.6.1: reading Do Not Disturb woke its own watch,
    /// so the policy loop never slept (~1,500 wakes a second, each with a
    /// logind round trip, ~85% CPU with an island connected).
    #[tokio::test]
    async fn reading_do_not_disturb_does_not_wake_its_own_watch() {
        let root = root("dnd-watch");
        let store = DesktopStore::fixture(&root).unwrap();
        let cosmic = store.user_root().to_path_buf();
        std::fs::write(key(&cosmic), "false").unwrap();
        let mut watch: DndWatch =
            Some((watch_dnd(&cosmic).expect("inotify watch"), cosmic.clone()));

        // The loop's first pass reads the key; nothing changed, so it sleeps.
        assert!(!dnd_in(&store));
        assert_eq!(wakes_until_quiet(&mut watch, &store).await, 0);
        assert_eq!(watch.as_ref().unwrap().0.wakes, 0, "reads woke the task");

        // A real change wakes it, and the read that follows does not.
        std::fs::write(key(&cosmic), "true").unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(5), dnd_wait(&mut watch))
                .await
                .is_ok_and(|w| w.is_ok()),
            "a write to the key wakes the watch"
        );
        assert!(dnd_in(&store));
        assert!(wakes_until_quiet(&mut watch, &store).await <= 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Other apps' settings under COSMIC's config folder never wake the
    /// task: the watch was recursive over all of it.
    #[tokio::test]
    async fn other_settings_do_not_wake_the_watch_and_a_new_namespace_does() {
        let root = root("dnd-narrow");
        let cosmic = root.join("config/cosmic");
        std::fs::remove_dir_all(cosmic.join("com.system76.CosmicNotifications")).unwrap();
        let other = cosmic.join("com.system76.CosmicPanel.Panel/v1");
        std::fs::create_dir_all(&other).unwrap();
        let mut watch: DndWatch = Some((watch_dnd(&cosmic).unwrap(), cosmic.clone()));
        for i in 0..50 {
            std::fs::write(other.join("size"), format!("{i}")).unwrap();
            std::fs::write(other.join(".tmp"), "x").unwrap();
            std::fs::rename(other.join(".tmp"), other.join("spacing")).unwrap();
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(200), dnd_wait(&mut watch))
                .await
                .is_err()
        );
        assert_eq!(watch.as_ref().unwrap().0.wakes, 0, "another app's settings");

        // The namespace appearing wakes it; the watch then follows the new
        // folders down to the key.
        let v1 = cosmic.join("com.system76.CosmicNotifications/v1");
        std::fs::create_dir_all(&v1).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), dnd_wait(&mut watch))
                .await
                .is_ok()
        );
        let (dir_watch, _) = watch.as_mut().unwrap();
        rewatch_dnd(dir_watch, &cosmic);
        std::fs::write(key(&cosmic), "true").unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), dnd_wait(&mut watch))
                .await
                .is_ok()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    mod logind {
        use super::super::*;
        use crate::activity_island::test_bus::Bus;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
        use zbus::object_server::SignalEmitter;

        const SESSION: &str = "/org/freedesktop/login1/session/_31";
        struct Manager(Arc<AtomicUsize>);
        #[zbus::interface(name = "org.freedesktop.login1.Manager")]
        impl Manager {
            #[zbus(name = "GetSessionByPID")]
            fn get_session_by_pid(&self, _pid: u32) -> zbus::zvariant::OwnedObjectPath {
                self.0.fetch_add(1, Relaxed);
                zbus::zvariant::OwnedObjectPath::try_from(SESSION).unwrap()
            }
        }
        struct Session {
            active: bool,
            locked: bool,
            reads: Arc<AtomicUsize>,
        }
        #[zbus::interface(name = "org.freedesktop.login1.Session")]
        impl Session {
            #[zbus(property)]
            fn active(&self) -> bool {
                self.reads.fetch_add(1, Relaxed);
                self.active
            }
            #[zbus(property)]
            fn locked_hint(&self) -> bool {
                self.reads.fetch_add(1, Relaxed);
                self.locked
            }
            #[zbus(signal)]
            async fn lock(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
            #[zbus(signal)]
            async fn unlock(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
        }
        async fn next(rx: &mut tokio::sync::mpsc::Receiver<Part>) -> Option<bool> {
            match tokio::time::timeout(Duration::from_secs(3), rx.recv()).await {
                Ok(Some(Part::Policy(_, locked))) => Some(locked),
                _ => None,
            }
        }

        /// The session is looked up once and read only when logind says it
        /// changed. Before, every logind signal on the bus (any session,
        /// any user) looked the session up again and read it, and a 30 s
        /// timer read it again regardless.
        #[tokio::test]
        async fn the_session_is_resolved_once_and_read_only_on_its_own_changes() {
            let bus = Bus::new();
            let lookups = Arc::new(AtomicUsize::new(0));
            let reads = Arc::new(AtomicUsize::new(0));
            let service = zbus::connection::Builder::address(bus.address.as_str())
                .unwrap()
                .name(LOGIN1)
                .unwrap()
                .serve_at("/org/freedesktop/login1", Manager(lookups.clone()))
                .unwrap()
                .serve_at(
                    SESSION,
                    Session {
                        active: true,
                        locked: false,
                        reads: reads.clone(),
                    },
                )
                .unwrap()
                .build()
                .await
                .unwrap();
            let client = zbus::connection::Builder::address(bus.address.as_str())
                .unwrap()
                .build()
                .await
                .unwrap();
            let (tx, mut rx) = tokio::sync::mpsc::channel(4);
            let task = tokio::spawn(async move {
                let mut files = None;
                matches!(
                    follow(&client, &tx, &mut files, &|| false).await,
                    Ended::Lost
                )
            });
            assert_eq!(next(&mut rx).await, Some(false), "active and unlocked");

            // Idle: nothing is read.
            let idle = reads.load(Relaxed);
            assert!(
                tokio::time::timeout(Duration::from_millis(500), rx.recv())
                    .await
                    .is_err()
            );
            assert_eq!(reads.load(Relaxed), idle, "read while nothing changed");

            // Another session's signals are not this session's changes.
            service
                .emit_signal(
                    None::<()>,
                    "/org/freedesktop/login1/session/_99",
                    "org.freedesktop.login1.Session",
                    "Lock",
                    &(),
                )
                .await
                .unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(300), rx.recv())
                    .await
                    .is_err()
            );

            let session = service
                .object_server()
                .interface::<_, Session>(SESSION)
                .await
                .unwrap();
            let emitter = session.signal_emitter().clone();
            session.get_mut().await.locked = true;
            session
                .get()
                .await
                .locked_hint_changed(&emitter)
                .await
                .unwrap();
            assert_eq!(next(&mut rx).await, Some(true), "LockedHint");
            session.get_mut().await.locked = false;
            Session::unlock(&emitter).await.unwrap();
            assert_eq!(next(&mut rx).await, Some(false), "Unlock");
            session.get_mut().await.active = false;
            session.get().await.active_changed(&emitter).await.unwrap();
            assert_eq!(next(&mut rx).await, Some(true), "inactive");
            assert_eq!(lookups.load(Relaxed), 1, "the session is looked up once");

            // logind going away ends the follow, to reconnect.
            service.release_name(LOGIN1).await.unwrap();
            assert!(
                tokio::time::timeout(Duration::from_secs(3), task)
                    .await
                    .unwrap()
                    .unwrap()
            );
        }
    }
}
