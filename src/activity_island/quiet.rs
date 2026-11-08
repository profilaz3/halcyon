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
/// Whether a file event under COSMIC's config folder can have changed Do Not
/// Disturb: a creation, write, rename or removal of the key or a folder above
/// it. Opening or reading is not a change. `dnd` opens the key and each
/// folder above it, so a watch that counted opens woke itself on every read
/// and queried logind each time, about 1,500 times a second.
fn dnd_changed(event: &notify::Result<notify::Event>, key: &std::path::Path) -> bool {
    use notify::EventKind;
    match event {
        Ok(e) => {
            matches!(
                e.kind,
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
            ) && e.paths.iter().any(|p| key.starts_with(p))
        }
        Err(_) => true,
    }
}
/// Watches `cosmic` (COSMIC's config folder) for Do Not Disturb changes. The
/// receiver gets one wake per burst of changes.
fn watch_dnd(
    cosmic: &std::path::Path,
) -> (
    Option<notify::RecommendedWatcher>,
    tokio::sync::mpsc::Receiver<()>,
) {
    use notify::Watcher;
    let (changed, files) = tokio::sync::mpsc::channel(1);
    let key = cosmic.join("com.system76.CosmicNotifications/v1/do_not_disturb");
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if dnd_changed(&event, &key) {
            let _ = changed.try_send(());
        }
    })
    .ok();
    if let Some(w) = &mut watcher {
        let _ = w.watch(cosmic, notify::RecursiveMode::Recursive);
    }
    (watcher, files)
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
    let (_watcher, mut files) = match dirs::config_dir() {
        Some(dir) => watch_dnd(&dir.join("cosmic")),
        None => (None, tokio::sync::mpsc::channel(1).1),
    };
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
                let mut is_locked = locked(&c).await;
                loop {
                    if tx.send(Part::Policy(dnd(), is_locked)).await.is_err() {
                        return;
                    }
                    tokio::select! {
                        _=tx.closed()=>return,
                        m=changes.next()=>{
                            if !matches!(m,Some(Ok(_))){break}
                            is_locked = locked(&c).await;
                        },
                        // A Do Not Disturb change needs no logind round trip.
                        // Coalesce the write/rename pair, as `watch::files` does.
                        Some(_)=files.recv()=>{
                            tokio::time::sleep(Duration::from_millis(25)).await;
                            while files.try_recv().is_ok() {}
                        },
                        _=tokio::time::sleep(Duration::from_secs(30))=>is_locked = locked(&c).await,
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
#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::store::DesktopStore;
    use notify::EventKind;
    use notify::event::{AccessKind, AccessMode, CreateKind, DataChange, ModifyKind, RemoveKind};
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
        cosmic.join("com.system76.CosmicNotifications/v1/do_not_disturb")
    }
    /// What `policy` does with the watch: every wake reads the key again.
    /// Returns the wakes before the watch stays quiet for 300 ms, at most 50.
    async fn wakes_until_quiet(
        files: &mut tokio::sync::mpsc::Receiver<()>,
        store: &DesktopStore,
    ) -> usize {
        let mut wakes = 0;
        while wakes < 50
            && tokio::time::timeout(Duration::from_millis(300), files.recv())
                .await
                .is_ok_and(|w| w.is_some())
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
        let event = |kind, path: &Path| -> notify::Result<notify::Event> {
            Ok(notify::Event::new(kind).add_path(path.into()))
        };
        let open = EventKind::Access(AccessKind::Open(AccessMode::Any));
        let close = EventKind::Access(AccessKind::Close(AccessMode::Read));
        let write = EventKind::Modify(ModifyKind::Data(DataChange::Any));
        let create = EventKind::Create(CreateKind::Folder);
        let remove = EventKind::Remove(RemoveKind::Any);
        // Reads, including `dnd`'s own, are not changes.
        assert!(!dnd_changed(&event(open, &key), &key));
        assert!(!dnd_changed(&event(close, &key), &key));
        assert!(!dnd_changed(&event(open, cosmic), &key));
        assert!(!dnd_changed(&event(open, key.parent().unwrap()), &key));
        // Writes to the key, or a folder above it appearing, are.
        assert!(dnd_changed(&event(write, &key), &key));
        assert!(dnd_changed(&event(create, key.parent().unwrap()), &key));
        assert!(dnd_changed(&event(remove, &key), &key));
        // Other keys are not; a lost watch is.
        assert!(!dnd_changed(
            &event(write, &key.with_file_name("max_notifications")),
            &key
        ));
        assert!(dnd_changed(&Err(notify::Error::generic("lost")), &key));
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
        let (watcher, mut files) = watch_dnd(&cosmic);
        assert!(watcher.is_some(), "inotify watch");

        // The loop's first pass reads the key; nothing changed, so it sleeps.
        assert!(!dnd_in(&store));
        assert_eq!(wakes_until_quiet(&mut files, &store).await, 0);

        // A real change wakes it, and the read that follows does not.
        std::fs::write(key(&cosmic), "true").unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(5), files.recv())
                .await
                .is_ok_and(|w| w.is_some()),
            "a write to the key wakes the watch"
        );
        assert!(dnd_in(&store));
        assert!(wakes_until_quiet(&mut files, &store).await <= 2);
        drop(watcher);
        std::fs::remove_dir_all(root).unwrap();
    }
}
