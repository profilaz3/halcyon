//! Profile automation: apply an existing profile at local times on chosen
//! weekdays, or when a display connects or disconnects. Off until chosen.
//!
//! - `rules`: the settings (`automation.json`), written by Settings only.
//! - `clock`: local wall-time arithmetic (DST, time-zone changes).
//! - `decide`: the pure policy (what is due, override, catch-up,
//!   priority) and the runtime state (`automation-state.json`), written by
//!   the desktop only.
//! - `run`: applying a switch through the profile engine.
//! - `displays`: connected displays and their model names.
//! - `runtime`: the desktop's loop around `decide` (settling displays,
//!   saving state, running switches as jobs).
//! - `status`: the words Settings shows.
//!
//! Every file lives in the desktop store's state folder beside the profile
//! library (a fixture's `state` folder in tests and previews), owner-only.
//! None of them is part of a profile, an export or `config.json`, so
//! importing or applying a profile never turns automation on. While Settings
//! has unapplied desktop changes it says which parts in
//! `automation-hold.json`, and switches touching those parts wait.
pub mod appearance;
pub mod clock;
pub mod decide;
pub mod displays;
pub mod rules;
pub mod run;
pub mod runtime;
pub mod solar;
pub mod status;

#[cfg_attr(not(test), allow(unused_imports))]
pub use decide::{Cause, Due, Effect, Outcome, State, decide, next_check, next_switch};
pub use rules::{Automation, Change, Days, DisplayWhen, Pause, ProfileRef, Rule, Time, Trigger};

use crate::desktop::DesktopStore;
use crate::desktop::profiles::Domain;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;

pub const FILE: &str = "automation.json";
pub const STATE_FILE: &str = "automation-state.json";
pub const HOLD_FILE: &str = "automation-hold.json";

/// Where `name` is read from (reading needs no write access).
pub fn path(store: &DesktopStore, name: &str) -> PathBuf {
    store.state_dir().join(name)
}

fn read(store: &DesktopStore, name: &str) -> Result<Option<Vec<u8>>, String> {
    let path = path(store, name);
    match std::fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => return Err(format!("{} is not a regular file", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    }
    if std::fs::metadata(&path).map_or(0, |meta| meta.len()) > rules::MAX_BYTES as u64 {
        return Err(format!("{} is too large", path.display()));
    }
    std::fs::read(&path)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn write(store: &DesktopStore, name: &str, bytes: &[u8]) -> Result<(), String> {
    let path = store.state_file(name).map_err(|error| error.to_string())?;
    crate::tools::write_private(&path, bytes)
}

/// The settings; a missing file is automation off, without rules.
pub fn load(store: &DesktopStore) -> Result<Automation, String> {
    match read(store, FILE)? {
        Some(bytes) => Automation::parse(&bytes),
        None => Ok(Automation::default()),
    }
}

/// Reads the current settings, changes them and writes them back, so two
/// edits never undo each other.
pub fn update(
    store: &DesktopStore,
    change: impl FnOnce(&mut Automation) -> Result<(), String>,
) -> Result<Automation, String> {
    let _lock = lock_rules(store)?;
    let mut automation = load(store)?;
    change(&mut automation)?;
    automation.version = automation.written_version();
    let bytes = automation.to_bytes()?;
    write(store, FILE, &bytes)?;
    Ok(automation)
}

/// Rule saves and scheduled execution share this lock. Hold it from the
/// final rule read through the profile transaction, so a saved edit cannot
/// be overtaken by work from the previous rule revision. This is a separate
/// file from the app config lock; acquire it before desktop writer locks.
pub(super) fn lock_rules(store: &DesktopStore) -> Result<crate::config::ConfigLock, String> {
    let path = store.state_file(FILE).map_err(|error| error.to_string())?;
    crate::config::lock_config(&path)
}

/// Reset also needs the execution fence, including when the old file is
/// damaged and cannot be loaded by `update`.
pub fn reset(store: &DesktopStore) -> Result<(), String> {
    let _lock = lock_rules(store)?;
    write(store, FILE, &Automation::default().to_bytes()?)
}

/// The desktop's runtime state; damaged or missing starts fresh.
pub fn load_state(store: &DesktopStore) -> State {
    read(store, STATE_FILE)
        .ok()
        .flatten()
        .map(|bytes| State::parse(&bytes))
        .unwrap_or_default()
}

pub fn save_state(store: &DesktopStore, state: &State) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(state).map_err(|error| error.to_string())?;
    write(store, STATE_FILE, &bytes)
}

/// The parts Settings has unapplied changes to.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hold {
    #[serde(default)]
    pub expires: i64,
    pub pid: u32,
    pub parts: Vec<String>,
    /// Compatibility with older Settings versions that saved layout precedence
    /// here. Current saves use the guarded store's durable manual history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied: Option<i64>,
}

/// Renew a live drag lease, preserving a legacy applied stamp across upgrades.
/// Current writers cannot create a second manual-precedence timestamp.
pub fn write_hold(store: &DesktopStore, parts: &BTreeSet<Domain>) -> Result<(), String> {
    let applied = hold_applied(store);
    if parts.is_empty() && applied.is_none() {
        return clear_hold(store);
    }
    let hold = Hold {
        expires: now() + 10,
        pid: std::process::id(),
        parts: parts.iter().map(|part| part.key().to_string()).collect(),
        applied,
    };
    let bytes = serde_json::to_vec(&hold).map_err(|error| error.to_string())?;
    write(store, HOLD_FILE, &bytes)
}

pub fn clear_hold(store: &DesktopStore) -> Result<(), String> {
    let path = store
        .state_file(HOLD_FILE)
        .map_err(|error| error.to_string())?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn hold(store: &DesktopStore) -> Option<Hold> {
    read(store, HOLD_FILE)
        .ok()
        .flatten()
        .and_then(|bytes| serde_json::from_slice::<Hold>(&bytes).ok())
}

/// The held parts, if the Settings window that wrote them still runs.
pub fn read_hold(store: &DesktopStore) -> BTreeSet<Domain> {
    let Some(hold) = hold(store) else {
        return BTreeSet::new();
    };
    if hold.expires < now() || !settings_alive(hold.pid) {
        return BTreeSet::new();
    }
    hold.parts
        .iter()
        .filter_map(|part| Domain::parse(part))
        .collect()
}

/// Compatibility-only manual precedence from an older Settings version.
pub fn hold_applied(store: &DesktopStore) -> Option<i64> {
    hold(store)?.applied
}

fn settings_alive(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    std::fs::read(format!("/proc/{pid}/cmdline")).is_ok_and(|cmdline| {
        cmdline
            .split(|byte| *byte == 0)
            .any(|arg| arg == b"--settings")
    })
}

/// Now, in Unix seconds.
pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod solar_tests;
