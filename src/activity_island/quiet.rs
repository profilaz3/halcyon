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
    let mut files: DndWatch = dirs::config_dir()
        .map(|dir| dir.join("cosmic"))
        .and_then(|cosmic| Some((watch_dnd(&cosmic)?, cosmic)));
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
                        woke=dnd_wait(&mut files), if files.is_some()=>{
                            if woke.is_err() { files = None; }
                            tokio::time::sleep(Duration::from_millis(25)).await;
                            // A folder on the way to the key may be new.
                            if let Some((watch, cosmic)) = &mut files {
                                rewatch_dnd(watch, cosmic);
                            }
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
}
