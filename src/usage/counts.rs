//! Counts since the last successful report: Settings pages opened, problems by
//! fixed kind, setup steps, and minutes the desktop ran. Any Halcyon process may
//! add to them; the reporter subtracts what it sent once the collector accepted
//! it. Nothing is recorded while sharing is off, and turning it off deletes them.
//! Only keys from the fixed lists below are kept: never text, names or paths.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Settings pages, by their stable route key.
pub const PAGES: &[&str] = &[
    "overview",
    "layout",
    "pending",
    "appearance",
    "widgets",
    "glance",
    "calendar",
    "weather",
    "devbox",
    "clock",
    "projects",
    "music",
    "notes",
    "focus",
    "devices",
    "general",
    "desktop",
    "desktop-dock",
    "desktop-appearance",
    "desktop-wallpaper",
    "desktop-icons",
    "desktop-profiles",
    "themes",
    "desktop-windows",
    "desktop-workspaces",
    "desktop-motion",
    "desktop-window-rules",
    "desktop-shortcuts",
    "desktop-automation",
    "moments",
    "adaptive",
    "builder",
    "changes",
];
/// Update and Islands failures share one list of kinds.
pub const FAILURES: &[&str] = &[
    "missing_packagekit",
    "offline",
    "timeout",
    "authorization",
    "untrusted",
    "invalid_response",
    "failed",
    "installation_changed",
    "activation",
    "islands_mismatch",
    "islands_refused",
];
/// Setup outcomes; steps are counted as `reached.<step>` and `skipped.<step>`.
const SETUP_OUTCOMES: &[&str] = &[
    "opened",
    "left",
    "kept_for_later",
    "cancelled",
    "finished",
    "undone",
];
pub const SETUP_STEPS: &[&str] = &[
    "look", "bars", "widgets", "arrange", "weather", "ready", "done",
];
const SKIPPABLE: &[&str] = &["bars", "widgets", "arrange"];
/// The collector refuses larger counts; a count stops growing here.
const MAX_COUNT: u32 = 100_000;
const MAX_MINUTES: u32 = 600_000;
const MAX_FILE: usize = 8192;

pub fn known_problem(key: &str) -> bool {
    matches!(key, "panic" | "render_timeout")
        || [("update.", FAILURES), ("islands.", FAILURES)]
            .iter()
            .any(|(prefix, list)| key.strip_prefix(prefix).is_some_and(|k| list.contains(&k)))
}
pub fn known_setup(key: &str) -> bool {
    SETUP_OUTCOMES.contains(&key)
        || key
            .strip_prefix("reached.")
            .is_some_and(|k| SETUP_STEPS.contains(&k))
        || key
            .strip_prefix("skipped.")
            .is_some_and(|k| SKIPPABLE.contains(&k))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Page,
    Problem,
    Setup,
}
impl Kind {
    fn known(self, key: &str) -> bool {
        match self {
            Self::Page => PAGES.contains(&key),
            Self::Problem => known_problem(key),
            Self::Setup => known_setup(key),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    #[serde(default)]
    pub pages: BTreeMap<String, u32>,
    #[serde(default)]
    pub problems: BTreeMap<String, u32>,
    #[serde(default)]
    pub setup: BTreeMap<String, u32>,
    #[serde(default)]
    pub runtime_minutes: u32,
}
impl Counts {
    fn map(&mut self, kind: Kind) -> &mut BTreeMap<String, u32> {
        match kind {
            Kind::Page => &mut self.pages,
            Kind::Problem => &mut self.problems,
            Kind::Setup => &mut self.setup,
        }
    }
    fn add(&mut self, kind: Kind, key: &str) {
        if kind.known(key) {
            let n = self.map(kind).entry(key.into()).or_default();
            *n = n.saturating_add(1).min(MAX_COUNT);
        }
    }
    /// Unknown keys and zero counts never leave the file, even a hand-edited one.
    fn clean(mut self) -> Self {
        for kind in [Kind::Page, Kind::Problem, Kind::Setup] {
            self.map(kind).retain(|key, n| *n > 0 && kind.known(key));
            for n in self.map(kind).values_mut() {
                *n = (*n).min(MAX_COUNT);
            }
        }
        self.runtime_minutes = self.runtime_minutes.min(MAX_MINUTES);
        self
    }
    /// What remains after a report of `sent` was accepted.
    fn subtract(&mut self, sent: &Counts) {
        for kind in [Kind::Page, Kind::Problem, Kind::Setup] {
            let sent = match kind {
                Kind::Page => &sent.pages,
                Kind::Problem => &sent.problems,
                Kind::Setup => &sent.setup,
            };
            let map = self.map(kind);
            for (key, n) in sent {
                if let Some(have) = map.get_mut(key) {
                    *have = have.saturating_sub(*n);
                }
            }
            map.retain(|_, n| *n > 0);
        }
        self.runtime_minutes = self.runtime_minutes.saturating_sub(sent.runtime_minutes);
    }
    /// Whole hours, rounded; none when the desktop did not run long enough to tell.
    pub fn runtime_hours(&self) -> u32 {
        (self.runtime_minutes + 30) / 60
    }
}

pub fn path() -> Option<PathBuf> {
    crate::paths::state_dir().map(|p| p.join("usage-counts.json"))
}
/// Count only in production processes, and only while sharing is on.
fn recording() -> Option<PathBuf> {
    (super::available() && super::preference() == Ok(true))
        .then(path)
        .flatten()
}
pub fn page(key: &str) {
    record(Kind::Page, key);
}
pub fn problem(key: &str) {
    record(Kind::Problem, key);
}
pub fn setup(key: &str) {
    record(Kind::Setup, key);
}
fn record(kind: Kind, key: &str) {
    if let Some(path) = recording() {
        let _ = add(&path, kind, key);
    }
}
pub fn add_runtime(minutes: u32) {
    if let Some(path) = recording() {
        let _ = update(&path, |c| {
            c.runtime_minutes = c.runtime_minutes.saturating_add(minutes).min(MAX_MINUTES)
        });
    }
}
/// Count crashes: a panic anywhere in this process adds one, then the usual
/// panic output follows.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        problem("panic");
        previous(info);
    }));
}

pub(super) fn add(path: &Path, kind: Kind, key: &str) -> Result<(), String> {
    if !kind.known(key) {
        return Ok(());
    }
    update(path, |c| c.add(kind, key))
}
fn update(path: &Path, change: impl FnOnce(&mut Counts)) -> Result<(), String> {
    let _lock = lock(path)?;
    // A damaged file starts over rather than blocking every later count.
    let mut counts = load(path).unwrap_or_default();
    change(&mut counts);
    save(path, &counts.clean())
}
/// The counts to send now. Unreadable counts send nothing.
pub(super) fn snapshot(path: &Path) -> Counts {
    lock(path)
        .ok()
        .and_then(|_lock| load(path).ok())
        .unwrap_or_default()
        .clean()
}
/// After an accepted report: keep only what was counted since the snapshot.
pub(super) fn sent(path: &Path, counts: &Counts) -> Result<(), String> {
    if *counts == Counts::default() {
        return Ok(());
    }
    update(path, |c| c.subtract(counts))
}
/// Turning sharing off forgets every count.
pub(super) fn forget(path: &Path) {
    if let Ok(_lock) = lock(path) {
        let _ = std::fs::remove_file(path);
    }
}

struct Lock(std::fs::File);
impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
/// Writers hold the lock for one small read and write, so wait briefly for it
/// rather than losing a count to another process.
fn lock(path: &Path) -> Result<Lock, String> {
    let parent = path.parent().ok_or("Missing state directory")?;
    if !parent.exists() {
        std::fs::create_dir_all(parent).map_err(|_| "Could not create state directory")?;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "Could not protect state directory")?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path.with_extension("lock"))
        .map_err(|_| "Could not lock usage counts")?;
    for _ in 0..40 {
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(Lock(file));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Err("Usage counts are busy".into())
}
fn load(path: &Path) -> Result<Counts, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Counts::default()),
        Err(_) => return Err("Could not read usage counts".into()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_FILE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read usage counts")?;
    if bytes.len() > MAX_FILE {
        return Err("Invalid usage counts".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid usage counts".into())
}
fn save(path: &Path, counts: &Counts) -> Result<(), String> {
    let tmp = path.with_extension(format!("{}.tmp", super::token()?));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|_| "Could not save usage counts")?;
        file.write_all(&serde_json::to_vec(counts).map_err(|_| "Invalid usage counts")?)
            .map_err(|_| "Could not write usage counts")?;
        file.sync_all().map_err(|_| "Could not save usage counts")?;
        std::fs::rename(&tmp, path).map_err(|_| "Could not replace usage counts")
    })();
    let _ = std::fs::remove_file(tmp);
    result.map_err(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> crate::test_env::TempPath {
        crate::test_env::TempPath::new("counts")
    }

    #[test]
    fn counts_only_fixed_keys_and_saturate() {
        let dir = dir();
        let path = dir.join("state/usage-counts.json");
        for _ in 0..3 {
            add(&path, Kind::Page, "appearance").unwrap();
        }
        add(&path, Kind::Page, "/home/me/.config").unwrap();
        add(&path, Kind::Problem, "update.offline").unwrap();
        add(
            &path,
            Kind::Problem,
            "update.Couldn’t reach the package sources",
        )
        .unwrap();
        add(&path, Kind::Problem, "update.cancelled").unwrap();
        add(&path, Kind::Setup, "reached.look").unwrap();
        add(&path, Kind::Setup, "skipped.look").unwrap();
        add(&path, Kind::Setup, "skipped.bars").unwrap();
        let counts = snapshot(&path);
        assert_eq!(counts.pages, BTreeMap::from([("appearance".into(), 3)]));
        assert_eq!(
            counts.problems,
            BTreeMap::from([("update.offline".into(), 1)])
        );
        assert_eq!(
            counts.setup,
            BTreeMap::from([("reached.look".into(), 1), ("skipped.bars".into(), 1)])
        );
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        let mut big = Counts::default();
        big.pages.insert("notes".into(), MAX_COUNT);
        big.runtime_minutes = u32::MAX;
        big.add(Kind::Page, "notes");
        let big = big.clean();
        assert_eq!(big.pages["notes"], MAX_COUNT);
        assert_eq!(big.runtime_minutes, MAX_MINUTES);
    }

    #[test]
    fn an_accepted_report_keeps_what_was_counted_meanwhile() {
        let dir = dir();
        let path = dir.join("usage-counts.json");
        add(&path, Kind::Page, "weather").unwrap();
        add(&path, Kind::Page, "weather").unwrap();
        add(&path, Kind::Problem, "panic").unwrap();
        update(&path, |c| c.runtime_minutes = 130).unwrap();
        let taken = snapshot(&path);
        assert_eq!(taken.runtime_hours(), 2);
        // Counted while the report was on its way.
        add(&path, Kind::Page, "weather").unwrap();
        update(&path, |c| c.runtime_minutes += 10).unwrap();
        sent(&path, &taken).unwrap();
        let left = snapshot(&path);
        assert_eq!(left.pages, BTreeMap::from([("weather".into(), 1)]));
        assert!(left.problems.is_empty());
        assert_eq!(left.runtime_minutes, 10);
        assert_eq!(left.runtime_hours(), 0);
        forget(&path);
        assert!(!path.exists());
        assert_eq!(snapshot(&path), Counts::default());
    }

    #[test]
    fn damaged_or_foreign_files_never_reach_a_report() {
        let dir = dir();
        let path = dir.join("usage-counts.json");
        std::fs::write(
            &path,
            b"{\"pages\":{\"weather\":2,\"secret\":5},\"setup\":{\"opened\":0}}",
        )
        .unwrap();
        let counts = snapshot(&path);
        assert_eq!(counts.pages, BTreeMap::from([("weather".into(), 2)]));
        assert!(counts.setup.is_empty());
        std::fs::write(&path, b"{\"pages\":{},\"locale\":\"hr_HR\"}").unwrap();
        assert_eq!(snapshot(&path), Counts::default());
        // A damaged file is replaced by the next count.
        add(&path, Kind::Page, "notes").unwrap();
        assert_eq!(snapshot(&path).pages, BTreeMap::from([("notes".into(), 1)]));
        std::fs::write(&path, vec![b' '; MAX_FILE + 1]).unwrap();
        assert_eq!(snapshot(&path), Counts::default());
    }

    #[test]
    fn a_busy_lock_waits_briefly_then_gives_up() {
        let dir = dir();
        let path = dir.join("usage-counts.json");
        let held = lock(&path).unwrap();
        let started = std::time::Instant::now();
        assert!(add(&path, Kind::Page, "notes").is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
        drop(held);
        add(&path, Kind::Page, "notes").unwrap();
    }

    #[test]
    fn vocabulary_matches_the_collector() {
        assert!(known_problem("islands.islands_refused"));
        assert!(known_problem("render_timeout"));
        assert!(!known_problem("update.cancelled"));
        assert!(!known_problem("update."));
        assert!(known_setup("reached.weather"));
        assert!(!known_setup("skipped.ready"));
        let details = include_str!("../../scripts/usage/worker/src/details.js");
        for key in PAGES
            .iter()
            .chain(FAILURES)
            .chain(SETUP_OUTCOMES)
            .chain(SETUP_STEPS)
        {
            let quoted = format!("'{key}'");
            let bare = format!("{key}:");
            assert!(
                details.contains(&quoted) || details.contains(&bare),
                "{key} is missing from the collector's vocabulary"
            );
        }
    }
}
