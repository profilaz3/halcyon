//! Lends COSMIC's popup limit to the island and gives it back.
//!
//! While the island shows notifications in place of popups, COSMIC's
//! `max_notifications` is 0: the daemon still hands every notification to
//! notification applets, but opens no popup of its own. The value it had is
//! kept in a small lease file and put back when the island stops (preference
//! off, Halcyon gone, applet gone).
//!
//! The lease names its holders: each island that took the popups, by
//! process and start time. An island gives back only its own hold, and the
//! limit returns when the last holder leaves. The desktop only clears holds
//! whose island process is gone, so it can never give popups back under a
//! live island. Every change runs under a lock beside the lease, and an
//! island's own requests carry a generation, so a request overtaken by a
//! newer one from the same island does nothing.
//!
//! This is a temporary lease, not a design change, so it is written with
//! COSMIC's own config API rather than through the desktop store's journal,
//! snapshots and Undo.
use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const NAMESPACE: &str = "com.system76.CosmicNotifications";
const KEY: &str = "max_notifications";
const DND: &str = "do_not_disturb";
const LEASE: &str = "notification-popups";
/// COSMIC's default, used when nothing can be read.
const DEFAULT: u32 = 3;
const LIMIT: u32 = 50;

struct Target {
    config: Config,
    lease: PathBuf,
}
impl Target {
    fn live() -> Result<Self, String> {
        Ok(Self {
            config: Config::new(NAMESPACE, 1)
                .map_err(|_| "COSMIC notification settings are unavailable")?,
            lease: crate::paths::state_dir()
                .ok_or("No state folder for the popup setting")?
                .join(LEASE),
        })
    }
    fn current(&self) -> u32 {
        self.config.get::<u32>(KEY).unwrap_or(DEFAULT)
    }
    fn set(&self, value: u32) -> Result<(), String> {
        self.config
            .set(KEY, value)
            .map_err(|_| "Could not change COSMIC's notification popups".into())
    }
    fn read_lease(&self) -> Result<Option<Lease>, String> {
        match crate::tools::read_bounded(&self.lease, 64 * 1024)? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes).unwrap_or(Lease {
                previous: DEFAULT,
                holders: Vec::new(),
            }))),
            None => Ok(None),
        }
    }
    fn write_lease(&self, lease: &Lease) -> Result<(), String> {
        crate::tools::write_private(&self.lease, &serde_json::to_vec(lease).unwrap_or_default())
    }
    /// Runs `change` holding the lease's lock, which every island and the
    /// desktop take, so their reads and writes never interleave.
    fn locked<T>(&self, change: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        use std::os::fd::AsRawFd;
        let mut name = self.lease.as_os_str().to_os_string();
        name.push(".lock");
        let lock = crate::desktop::recovery::open_lock_file(std::path::Path::new(&name))
            .map_err(|_| "Could not lock the saved popup setting".to_string())?;
        loop {
            // SAFETY: lock owns a valid descriptor for this whole call.
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } == 0 {
                break;
            }
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                return Err("Could not lock the saved popup setting".into());
            }
        }
        let result = change();
        drop(lock);
        result
    }
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Lease {
    /// The popup limit before the island took it.
    previous: u32,
    /// The islands holding it. A lease from before holders were named has
    /// none, and the next holder or the desktop settles it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    holders: Vec<HolderId>,
}
/// An island holding the lease: its process, by start time so a reused
/// process number is not taken for it, and which island in that process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HolderId {
    pid: u32,
    start: u64,
    island: u64,
}
impl HolderId {
    fn alive(&self) -> bool {
        process_start(self.pid) == Some(self.start)
    }
}
/// A process's start time, the 22nd field of its stat file.
fn process_start(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The name before it may hold spaces; count from its closing parenthesis.
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(19)?.parse().ok()
}
/// One island's hold on COSMIC's popup limit, and the order of its own
/// requests.
#[derive(Debug, Clone)]
pub struct Holder {
    id: HolderId,
    generation: Arc<AtomicU64>,
}
impl Default for Holder {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let pid = std::process::id();
        Self {
            id: HolderId {
                pid,
                start: process_start(pid).unwrap_or_default(),
                island: NEXT.fetch_add(1, Ordering::Relaxed),
            },
            generation: Arc::default(),
        }
    }
}
/// Turns COSMIC's popups off for `holder`, first saving the current limit.
/// Returns whether the limit was written.
fn take(target: &Target, holder: &HolderId) -> Result<bool, String> {
    let current = target.current();
    let mut lease = target.read_lease()?.unwrap_or(Lease {
        previous: current.min(LIMIT),
        holders: Vec::new(),
    });
    lease.holders.retain(|h| h != holder && h.alive());
    lease.holders.push(holder.clone());
    target.write_lease(&lease)?;
    if current == 0 {
        return Ok(false);
    }
    target.set(0)?;
    Ok(true)
}
/// Gives back `holder`'s hold. The limit returns only when no other live
/// island holds it. Returns whether the limit was put back.
fn release(target: &Target, holder: &HolderId) -> Result<bool, String> {
    let Some(mut lease) = target.read_lease()? else {
        return Ok(false);
    };
    lease.holders.retain(|h| h != holder && h.alive());
    if !lease.holders.is_empty() {
        target.write_lease(&lease)?;
        return Ok(false);
    }
    restore(target, &lease)
}
/// For the desktop: clears holds whose island is gone, and puts the limit
/// back when none is left. Returns whether the limit was put back.
fn release_departed(target: &Target) -> Result<bool, String> {
    let Some(mut lease) = target.read_lease()? else {
        return Ok(false);
    };
    let before = lease.holders.len();
    lease.holders.retain(HolderId::alive);
    if !lease.holders.is_empty() {
        if lease.holders.len() != before {
            target.write_lease(&lease)?;
        }
        return Ok(false);
    }
    restore(target, &lease)
}
/// Puts the saved limit back and forgets the lease. A limit someone changed
/// while the island held it is left alone.
fn restore(target: &Target, lease: &Lease) -> Result<bool, String> {
    if target.current() == 0 {
        target.set(lease.previous.min(LIMIT))?;
    }
    match std::fs::remove_file(&target.lease) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(_) => Err("Could not clear the saved popup setting".into()),
    }
}
/// `holder`'s request number `generation`, unless a newer one from the same
/// holder was made since: that one runs after it and decides.
fn apply(
    target: &Target,
    holder: &Holder,
    generation: u64,
    take_popups: bool,
) -> Result<bool, String> {
    target.locked(|| {
        if holder.generation.load(Ordering::SeqCst) != generation {
            return Ok(false);
        }
        if take_popups {
            take(target, &holder.id)
        } else {
            release(target, &holder.id)
        }
    })
}
fn live_enabled() -> bool {
    // Tests and previews never touch the session's real configuration.
    !cfg!(test) && !crate::tools::preview()
}
#[cfg(test)]
thread_local! {
    /// Calls to `restore_soon` on this thread.
    pub static RESTORES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
/// From a thread of its own, for owners without a task runtime at hand.
pub fn restore_soon() {
    #[cfg(test)]
    RESTORES.with(|count| count.set(count.get() + 1));
    if !live_enabled() {
        return;
    }
    std::thread::spawn(|| {
        if let Ok(target) = Target::live() {
            let _ = target.locked(|| release_departed(&target));
        }
    });
}
/// The same switch as COSMIC's notification applet: the daemon, its applet
/// and the island all follow this key.
fn write_dnd(config: &Config, on: bool) -> Result<(), String> {
    config
        .set(DND, on)
        .map_err(|_| "Could not change Do Not Disturb".into())
}
/// Off the UI thread, like `set`.
pub async fn set_dnd(on: bool) -> Result<(), String> {
    if !live_enabled() {
        return Ok(());
    }
    tokio::task::spawn_blocking(move || {
        let config = Config::new(NAMESPACE, 1)
            .map_err(|_| "COSMIC notification settings are unavailable".to_string())?;
        write_dnd(&config, on)
    })
    .await
    .map_err(|_| "Could not change Do Not Disturb".to_string())?
}
/// Off the UI thread. Errors are reported, never retried in a loop. The
/// request is numbered now, so a later one from the same island wins.
pub fn set(
    holder: &Holder,
    take_popups: bool,
) -> impl std::future::Future<Output = Result<bool, String>> + use<> {
    let generation = holder.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let holder = holder.clone();
    async move {
        if !live_enabled() {
            return Ok(false);
        }
        tokio::task::spawn_blocking(move || {
            let target = Target::live()?;
            apply(&target, &holder, generation, take_popups)
        })
        .await
        .map_err(|_| "Could not change notification popups".to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Root(PathBuf);
    impl Drop for Root {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn target() -> (Root, Target) {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "popup-lease-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let target = Target {
            config: Config::with_custom_path(NAMESPACE, 1, root.clone()).unwrap(),
            lease: root.join("state").join(LEASE),
        };
        (Root(root), target)
    }
    fn island() -> HolderId {
        Holder::default().id
    }
    /// A holder whose process is gone.
    fn departed() -> HolderId {
        HolderId {
            start: island().start + 1,
            ..island()
        }
    }
    #[test]
    fn lease_saves_the_limit_and_gives_it_back_once() {
        let (_root, t) = target();
        let me = island();
        t.set(5).unwrap();
        assert!(take(&t, &me).unwrap());
        assert_eq!(t.current(), 0);
        // Taking again keeps the first saved value.
        assert!(!take(&t, &me).unwrap());
        assert!(release(&t, &me).unwrap());
        assert_eq!(t.current(), 5);
        assert!(!release(&t, &me).unwrap());
        assert_eq!(t.current(), 5);
    }
    #[test]
    fn a_limit_changed_while_held_is_kept_and_unreadable_leases_fall_back() {
        let (_root, t) = target();
        let me = island();
        t.set(4).unwrap();
        take(&t, &me).unwrap();
        t.set(2).unwrap();
        assert!(release(&t, &me).unwrap());
        assert_eq!(t.current(), 2);
        take(&t, &me).unwrap();
        std::fs::write(&t.lease, b"not json").unwrap();
        assert!(release_departed(&t).unwrap());
        assert_eq!(t.current(), DEFAULT);
    }
    #[test]
    fn d6_each_island_holds_its_own_share_and_the_last_one_gives_it_back() {
        let (_root, t) = target();
        let (a, b) = (island(), island());
        t.set(5).unwrap();
        take(&t, &a).unwrap();
        take(&t, &b).unwrap();
        // One island leaving keeps popups off for the other.
        assert!(!release(&t, &a).unwrap());
        assert_eq!(t.current(), 0);
        assert!(release(&t, &b).unwrap());
        assert_eq!(t.current(), 5);
    }
    #[test]
    fn d6_the_desktop_never_restores_under_a_live_island() {
        let (_root, t) = target();
        let me = island();
        t.set(3).unwrap();
        take(&t, &me).unwrap();
        // The desktop thinks every island left (a panel restart), but this
        // one took the popups again in the meantime.
        assert!(!release_departed(&t).unwrap());
        assert_eq!(t.current(), 0);
        assert!(t.read_lease().unwrap().is_some());
        // A hold left by a process that is gone is cleared, and with no
        // live holder the limit comes back.
        let (_root, t) = target();
        t.set(3).unwrap();
        take(&t, &departed()).unwrap();
        assert!(release_departed(&t).unwrap());
        assert_eq!(t.current(), 3);
        // A live island taking over prunes the departed hold too.
        take(&t, &departed()).unwrap();
        take(&t, &me).unwrap();
        assert_eq!(t.read_lease().unwrap().unwrap().holders, [me.clone()]);
        // A lease from before holders were named still settles.
        std::fs::write(&t.lease, br#"{"previous":4}"#).unwrap();
        assert!(release_departed(&t).unwrap());
        assert_eq!(t.current(), 4);
    }
    #[test]
    fn d6_an_overtaken_request_from_the_same_island_does_nothing() {
        let (_root, t) = target();
        let holder = Holder::default();
        t.set(5).unwrap();
        // Take, then give back, asked in that order but run the other way.
        let first = holder.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let second = holder.generation.fetch_add(1, Ordering::SeqCst) + 1;
        assert!(apply(&t, &holder, second, false).is_ok());
        assert!(!apply(&t, &holder, first, true).unwrap());
        assert_eq!(t.current(), 5, "the stale take must not turn popups off");
        assert!(t.read_lease().unwrap().is_none());
    }
    #[test]
    fn d6_islands_and_the_desktop_racing_keep_the_lease_whole() {
        let (_root, t) = target();
        t.set(5).unwrap();
        let t = Arc::new(t);
        let holder = Holder::default();
        let threads: Vec<_> = (0..8)
            .map(|n| {
                let t = t.clone();
                let holder = holder.clone();
                std::thread::spawn(move || {
                    for _ in 0..25 {
                        if n % 2 == 0 {
                            let generation = holder.generation.load(Ordering::SeqCst);
                            let _ = apply(&t, &holder, generation, true);
                        } else {
                            let _ = t.locked(|| release_departed(&t));
                        }
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        // The island is alive and wants the popups: they are off, and the
        // lease still remembers the limit to give back.
        assert_eq!(t.current(), 0);
        let lease = t.read_lease().unwrap().unwrap();
        assert_eq!(lease.previous, 5);
        assert!(release(&t, &holder.id).unwrap());
        assert_eq!(t.current(), 5);
    }
    #[test]
    fn do_not_disturb_uses_cosmics_own_key() {
        let (_root, t) = target();
        write_dnd(&t.config, true).unwrap();
        assert!(t.config.get::<bool>("do_not_disturb").unwrap());
        write_dnd(&t.config, false).unwrap();
        assert!(!t.config.get::<bool>("do_not_disturb").unwrap());
    }
}
