//! Notices when a package upgrade replaces the running program.
//!
//! dpkg and rpm install a new `/usr/bin/halcyon` by renaming it over the old
//! one. A process that is already running keeps the old file open, and
//! Linux then names it “/usr/bin/halcyon (deleted)” in `/proc/self/exe`.
//! The Activity applet would keep running the old program until the next
//! sign-in. (The desktop widgets have the same problem; they don't use this
//! yet, see the note on `restart`.)
//!
//! `watch` follows the program's folder with one inotify watch (no polling).
//! Once the folder has been quiet for a moment, the file at the path differs
//! from the running one, and it answers `--version` like Halcyon, it reports
//! the new program once. The process then waits until nobody is using it and
//! exits with `RESTART_STATUS`. Its owner starts it again from the same path:
//! COSMIC's panel restarts an applet that exits with a failure status, and
//! hands a notification applet a fresh notification socket as it does.
//!
//! A broken update cannot loop. A program that does not answer `--version`
//! is never restarted into. Every restart is written to a small ledger in the
//! session's runtime folder; each further upgrade within `WINDOW` waits
//! longer, and after `MAX_RESTARTS` the process stays on what it runs.
use cosmic::iced::{Subscription, futures::SinkExt, stream};
use notify::{RecursiveMode, Watcher};
use std::fs;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The exit status that asks the owner to start the updated program
/// (`EX_TEMPFAIL`). Any failure status is restarted; this one is greppable.
pub const RESTART_STATUS: i32 = 75;
/// Quiet time after the last change to the program before it is checked,
/// so a package manager has finished unpacking.
const SETTLE: Duration = Duration::from_secs(3);
/// How long the new program may take to answer `--version`.
const PROBE: Duration = Duration::from_secs(5);
/// Restarts for different programs within this window count together.
const WINDOW: Duration = Duration::from_secs(15 * 60);
/// After this many restarts within `WINDOW`, stay on the running program.
const MAX_RESTARTS: usize = 3;
/// The wait added before the second restart within `WINDOW`; it doubles for
/// each one after.
const BACKOFF: Duration = Duration::from_secs(30);
const LEDGER_LIMIT: usize = 64 * 1024;

/// One program file, by path and by the file it is.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Program {
    pub path: PathBuf,
    dev: u64,
    ino: u64,
}

impl Program {
    /// The program this process runs, and the path it was started from.
    pub fn running() -> Option<Self> {
        Self::from_exe(Path::new("/proc/self/exe"))
    }
    /// `exe` is a `/proc/<pid>/exe` link: its target names the path, and
    /// following it reaches the open file even after it was replaced.
    fn from_exe(exe: &Path) -> Option<Self> {
        let target = fs::read_link(exe).ok()?;
        let text = target.to_str()?;
        let path = PathBuf::from(text.strip_suffix(" (deleted)").unwrap_or(text));
        let meta = fs::metadata(exe).ok()?;
        Some(Self {
            path,
            dev: meta.dev(),
            ino: meta.ino(),
        })
    }
    /// The file now at `path`.
    pub fn at(path: &Path) -> Option<Self> {
        let meta = fs::metadata(path).ok()?;
        Some(Self {
            path: path.to_path_buf(),
            dev: meta.dev(),
            ino: meta.ino(),
        })
    }
    /// The complete executable now at this path, when it is a different file.
    pub fn replacement(&self) -> Option<Self> {
        let meta = fs::metadata(&self.path).ok()?;
        let complete = meta.is_file() && meta.len() > 0 && meta.mode() & 0o111 != 0;
        let other = (meta.dev(), meta.ino()) != (self.dev, self.ino);
        (complete && other).then(|| Self {
            path: self.path.clone(),
            dev: meta.dev(),
            ino: meta.ino(),
        })
    }
    fn key(&self) -> String {
        format!("{}:{}", self.dev, self.ino)
    }
}

/// Reports the updated program once, when the running one was replaced.
pub fn watch() -> Subscription<Program> {
    Subscription::run(replacements)
}

fn replacements() -> impl cosmic::iced::futures::Stream<Item = Program> {
    stream::channel(1, async move |mut output| {
        if let Some(running) = Program::running() {
            if let Some(new) = replaced(&running, SETTLE).await {
                let _ = output.send(new).await;
            }
        }
        std::future::pending::<()>().await;
    })
}

/// Waits until the program at `running.path` is replaced by one that
/// answers `--version`. `None` when its folder cannot be watched.
async fn replaced(running: &Program, settle: Duration) -> Option<Program> {
    let folder = running.path.parent()?.to_path_buf();
    let (sender, mut changes) = tokio::sync::mpsc::channel(1);
    let path = running.path.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        // A lost event (queue overflow) may have been the program.
        if event.map_or(true, |event| event.paths.iter().any(|p| *p == path)) {
            let _ = sender.try_send(());
        }
    })
    .ok()?;
    watcher.watch(&folder, RecursiveMode::NonRecursive).ok()?;
    loop {
        changes.recv().await?;
        // Let the package manager finish: wait for a quiet `settle`.
        while let Ok(change) = tokio::time::timeout(settle, changes.recv()).await {
            change?;
        }
        let Some(new) = running.replacement() else {
            continue;
        };
        if answers(&new.path).await {
            return Some(new);
        }
        eprintln!(
            "Halcyon was updated, but {} does not start; staying on the running version",
            new.path.display()
        );
    }
}

/// The program at `path` starts and says it is Halcyon.
async fn answers(path: &Path) -> bool {
    let run = tokio::process::Command::new(path)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(PROBE, run).await {
        Ok(Ok(out)) => out.status.success() && out.stdout.starts_with(b"halcyon "),
        _ => false,
    }
}

/// What to do about an updated program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// Restart once idle, but not sooner than `after`.
    Restart { after: Duration },
    /// Too many restarts lately: keep running.
    Stay,
}

/// Plans the restart into `new`, given the restarts recorded this session
/// (seconds since the epoch, program key). Another process that already
/// restarted into the same program (a second panel's applet) does not count.
fn plan(ledger: &[(u64, String)], new: &Program, now: u64) -> Plan {
    let key = new.key();
    let mut recent: Vec<&str> = ledger
        .iter()
        .filter(|(at, other)| *other != key && now.abs_diff(*at) < WINDOW.as_secs())
        .map(|(_, other)| other.as_str())
        .collect();
    recent.sort_unstable();
    recent.dedup();
    match recent.len() {
        n if n >= MAX_RESTARTS => Plan::Stay,
        0 => Plan::Restart {
            after: Duration::ZERO,
        },
        n => Plan::Restart {
            after: BACKOFF * 2u32.pow(n as u32 - 1),
        },
    }
}

fn parse(text: &str) -> Vec<(u64, String)> {
    text.lines()
        .filter_map(|line| {
            let (at, key) = line.split_once(' ')?;
            Some((at.parse().ok()?, key.to_string()))
        })
        .collect()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The session's restart ledger for one kind of process.
pub struct Ledger(Option<PathBuf>);

impl Ledger {
    pub fn session(role: &str) -> Self {
        Self(crate::paths::runtime_dir().map(|dir| dir.join(format!("upgrade-{role}"))))
    }
    #[cfg(test)]
    fn at(path: PathBuf) -> Self {
        Self(Some(path))
    }
    /// Without a readable ledger nothing proves a restart cannot loop, so
    /// the process stays.
    pub fn plan(&self, new: &Program) -> Plan {
        let Some(path) = &self.0 else {
            return Plan::Stay;
        };
        match crate::tools::read_bounded(path, LEDGER_LIMIT) {
            Ok(bytes) => plan(
                &parse(&String::from_utf8_lossy(&bytes.unwrap_or_default())),
                new,
                now_secs(),
            ),
            Err(_) => Plan::Stay,
        }
    }
    /// Notes a restart into `new`. One appended line, so processes that
    /// restart together do not overwrite each other.
    pub fn record(&self, new: &Program) -> Result<(), String> {
        let path = self.0.as_ref().ok_or("No runtime folder")?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        file.write_all(format!("{} {}\n", now_secs(), new.key()).as_bytes())
            .map_err(|e| e.to_string())
    }
}

/// Records the restart and exits for the owner to start `new`. Returns only
/// when the restart cannot be recorded: without a record a broken update
/// could loop, so the caller keeps running.
pub fn restart(ledger: &Ledger, new: &Program, what: &str) -> String {
    if let Err(error) = ledger.record(new) {
        eprintln!("Halcyon was updated; {what} keeps running the old version: {error}");
        return error;
    }
    eprintln!("Halcyon was updated; restarting {what}");
    std::process::exit(RESTART_STATUS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn folder(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("halcyon-upgrade-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Installs a fake program the way dpkg does: write beside, rename over.
    fn install(path: &Path, script: &str) {
        let staged = path.with_extension("dpkg-new");
        fs::write(&staged, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755)).unwrap();
        fs::rename(&staged, path).unwrap();
    }

    fn program(key: &str) -> Program {
        let (dev, ino) = key.split_once(':').unwrap();
        Program {
            path: "/usr/bin/halcyon".into(),
            dev: dev.parse().unwrap(),
            ino: ino.parse().unwrap(),
        }
    }

    #[test]
    fn running_program_is_this_test_binary() {
        let running = Program::running().unwrap();
        assert_eq!(running.path, std::env::current_exe().unwrap());
        assert_eq!(running.replacement(), None);
    }

    #[test]
    fn a_renamed_over_program_is_a_replacement_and_a_rewrite_in_place_is_not() {
        let dir = folder("replace");
        let path = dir.join("halcyon");
        install(&path, "echo 'halcyon 1'");
        let old = Program::at(&path).unwrap();
        assert_eq!(old.replacement(), None);
        install(&path, "echo 'halcyon 2'");
        let new = old.replacement().unwrap();
        assert_eq!(new, Program::at(&path).unwrap());
        // Not executable yet (mid-install): not a replacement.
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(old.replacement(), None);
        // Removed: nothing to restart into.
        fs::remove_file(&path).unwrap();
        assert_eq!(old.replacement(), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn plan_backs_off_and_then_stays() {
        let now = 10_000;
        let new = program("1:9");
        let at = |ago: u64, key: &str| (now - ago, key.to_string());
        assert_eq!(
            plan(&[], &new, now),
            Plan::Restart {
                after: Duration::ZERO
            }
        );
        // Old restarts are forgotten.
        assert_eq!(
            plan(&[at(3600, "1:1"), at(1200, "1:2")], &new, now),
            Plan::Restart {
                after: Duration::ZERO
            }
        );
        assert_eq!(
            plan(&[at(60, "1:1")], &new, now),
            Plan::Restart { after: BACKOFF }
        );
        assert_eq!(
            plan(&[at(60, "1:1"), at(30, "1:2")], &new, now),
            Plan::Restart { after: BACKOFF * 2 }
        );
        assert_eq!(
            plan(&[at(90, "1:1"), at(60, "1:2"), at(30, "1:3")], &new, now),
            Plan::Stay
        );
        // Another panel's applet restarting into the same program, or the
        // same earlier program twice, counts once.
        assert_eq!(
            plan(&[at(60, "1:1"), at(59, "1:1"), at(30, "1:9")], &new, now),
            Plan::Restart { after: BACKOFF }
        );
        // A clock that went back still counts the restart as recent.
        assert_eq!(
            plan(&[(now + 60, "1:1".into())], &new, now),
            Plan::Restart { after: BACKOFF }
        );
    }

    #[test]
    fn ledger_records_and_counts_restarts() {
        let dir = folder("ledger");
        let ledger = Ledger::at(dir.join("runtime/upgrade-test"));
        let zero = Plan::Restart {
            after: Duration::ZERO,
        };
        assert_eq!(ledger.plan(&program("1:1")), zero);
        for key in ["1:1", "1:2", "1:3"] {
            ledger.record(&program(key)).unwrap();
        }
        assert_eq!(ledger.plan(&program("1:4")), Plan::Stay);
        // A ledger that cannot be read is no proof against a loop.
        fs::write(dir.join("big"), vec![b'1'; LEDGER_LIMIT + 1]).unwrap();
        assert_eq!(
            Ledger::at(dir.join("big")).plan(&program("1:4")),
            Plan::Stay
        );
        assert_eq!(Ledger(None).plan(&program("1:4")), Plan::Stay);
        assert_eq!(parse("12 1:2\nbroken\n\nx 1:3\n"), vec![(12, "1:2".into())]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn watch_reports_a_working_replacement_and_ignores_a_broken_one() {
        let dir = folder("watch");
        let path = dir.join("halcyon");
        install(&path, "echo 'halcyon 1'");
        let running = Program::at(&path).unwrap();
        let settle = Duration::from_millis(100);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            // A neighbour changing does not wake it.
            let wait = replaced(&running, settle);
            tokio::pin!(wait);
            let quiet = tokio::time::timeout(Duration::from_millis(400), async {
                tokio::select! {
                    found = &mut wait => found,
                    _ = async {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        fs::write(dir.join("halcyon-ctl"), "x").unwrap();
                        // A broken update is not restarted into.
                        install(&path, "exit 1");
                        std::future::pending::<()>().await;
                    } => unreachable!(),
                }
            })
            .await;
            assert!(quiet.is_err(), "a broken program must not be reported");
            // Something else answering is not Halcyon either.
            install(&path, "echo 'other 1'");
            let other = tokio::time::timeout(Duration::from_millis(400), &mut wait).await;
            assert!(other.is_err());
            install(&path, "echo 'halcyon 2'");
            let found = tokio::time::timeout(Duration::from_secs(5), &mut wait)
                .await
                .expect("the working update is reported")
                .unwrap();
            assert_eq!(found, Program::at(&path).unwrap());
        });
        let _ = fs::remove_dir_all(dir);
    }
}
