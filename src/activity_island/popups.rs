//! Lends COSMIC's popup limit to the island and gives it back.
//!
//! While the island shows notifications in place of popups, COSMIC's
//! `max_notifications` is 0: the daemon still hands every notification to
//! notification applets, but opens no popup of its own. The value it had is
//! kept in a small lease file and put back when the island stops (preference
//! off, Halcyon gone, applet gone). Both the applet and the desktop may
//! restore; whichever finds the lease first does it.
//!
//! This is a temporary lease, not a design change, so it is written with
//! COSMIC's own config API rather than through the desktop store's journal,
//! snapshots and Undo.
use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
        match crate::tools::read_bounded(&self.lease, 4096)? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes).unwrap_or(Lease {
                previous: DEFAULT,
            }))),
            None => Ok(None),
        }
    }
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Lease {
    /// The popup limit before the island took it.
    previous: u32,
}
/// Turns COSMIC's popups off, first saving the current limit. Returns
/// whether anything was written.
fn take(target: &Target) -> Result<bool, String> {
    let current = target.current();
    if target.read_lease()?.is_none() {
        let lease = Lease {
            previous: current.min(LIMIT),
        };
        crate::tools::write_private(
            &target.lease,
            &serde_json::to_vec(&lease).unwrap_or_default(),
        )?;
    }
    if current == 0 {
        return Ok(false);
    }
    target.set(0)?;
    Ok(true)
}
/// Puts the saved limit back and forgets the lease. A limit someone changed
/// while the island held it is left alone. Returns whether a lease existed.
fn restore(target: &Target) -> Result<bool, String> {
    let Some(lease) = target.read_lease()? else {
        return Ok(false);
    };
    if target.current() == 0 {
        target.set(lease.previous.min(LIMIT))?;
    }
    match std::fs::remove_file(&target.lease) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(_) => Err("Could not clear the saved popup setting".into()),
    }
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
            let _ = restore(&target);
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
/// Off the UI thread. Errors are reported, never retried in a loop.
pub async fn set(take_popups: bool) -> Result<bool, String> {
    if !live_enabled() {
        return Ok(false);
    }
    tokio::task::spawn_blocking(move || {
        let target = Target::live()?;
        if take_popups {
            take(&target)
        } else {
            restore(&target)
        }
    })
    .await
    .map_err(|_| "Could not change notification popups".to_string())?
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
    #[test]
    fn lease_saves_the_limit_and_gives_it_back_once() {
        let (_root, t) = target();
        t.set(5).unwrap();
        assert!(take(&t).unwrap());
        assert_eq!(t.current(), 0);
        // Taking again keeps the first saved value.
        assert!(!take(&t).unwrap());
        assert!(restore(&t).unwrap());
        assert_eq!(t.current(), 5);
        assert!(!restore(&t).unwrap());
        assert_eq!(t.current(), 5);
    }
    #[test]
    fn a_limit_changed_while_held_is_kept_and_unreadable_leases_fall_back() {
        let (_root, t) = target();
        t.set(4).unwrap();
        take(&t).unwrap();
        t.set(2).unwrap();
        assert!(restore(&t).unwrap());
        assert_eq!(t.current(), 2);
        take(&t).unwrap();
        std::fs::write(&t.lease, b"not json").unwrap();
        assert!(restore(&t).unwrap());
        assert_eq!(t.current(), DEFAULT);
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
