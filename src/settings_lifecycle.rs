//! Settings ownership belongs to the binary, shared by every launch path.
//! Locks are scoped to the same configuration directory as the command mailbox.
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::{Command, ExitStatus};
use std::time::{Duration, Instant};

const HANDOFF_TIMEOUT: Duration = Duration::from_secs(15);

fn in_widget_service(cgroup: &str) -> bool {
    cgroup.lines().any(|line| {
        line.splitn(3, ':')
            .nth(2)
            .is_some_and(|path| path.split('/').any(|part| part == "halcyon.service"))
    })
}

fn scope_command(executable: &Path, args: impl Iterator<Item = std::ffi::OsString>) -> Command {
    let mut command = Command::new("/usr/bin/systemd-run");
    // A scope moves the new process out of halcyon.service's cgroup while
    // retaining the caller's environment (Wayland, config overrides, etc.).
    // Unlike setsid, this survives the widget service's control-group stop.
    command.args([
        "--user",
        "--scope",
        "--collect",
        "--quiet",
        "--slice=app.slice",
    ]);
    command.arg(format!(
        "--unit=halcyon-settings-{}.scope",
        std::process::id()
    ));
    command.arg("--").arg(executable).args(args);
    command
}

/// Only service descendants need moving. Direct desktop/terminal launches
/// already have an independent lifetime. The scope child sees its new cgroup
/// and continues through normal ownership, without a bypass flag or env marker.
pub fn independent_scope() -> io::Result<Option<ExitStatus>> {
    if !in_widget_service(&fs::read_to_string("/proc/self/cgroup")?) {
        return Ok(None);
    }
    scope_command(&std::env::current_exe()?, std::env::args_os().skip(1))
        .status()
        .map(Some)
}

/// The installed program behind this one. An update replaces the file, and
/// Linux then names the running copy “<path> (deleted)”.
fn installed(executable: &Path) -> std::path::PathBuf {
    let text = executable.to_string_lossy();
    let path = Path::new(text.strip_suffix(" (deleted)").unwrap_or(&text));
    if path.is_file() {
        path.to_path_buf()
    } else {
        "halcyon".into()
    }
}

/// Replaces this process with Settings from the installed program, opened on
/// `page`. It only returns when that fails.
pub fn restart(page: &str) -> io::Error {
    use std::os::unix::process::CommandExt;
    let executable = match std::env::current_exe() {
        Ok(executable) => installed(&executable),
        Err(_) => "halcyon".into(),
    };
    Command::new(executable).args(["--settings", page]).exec()
}

fn lock_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
}

fn try_lock(file: &File) -> io::Result<bool> {
    match file.try_lock() {
        Ok(()) => Ok(true),
        Err(TryLockError::WouldBlock) => Ok(false),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

fn wait(deadline: Instant) -> io::Result<()> {
    if Instant::now() >= deadline {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Settings did not accept the page request. Try again when the window responds.",
        ));
    }
    std::thread::sleep(Duration::from_millis(20));
    Ok(())
}

/// Returns the lifetime lock for the sole window owner, or None after the
/// existing owner consumes this request. Hold the returned file until UI exit.
pub fn prepare(config: &Path, request: &[u8]) -> io::Result<Option<File>> {
    prepare_until(config, request, Instant::now() + HANDOFF_TIMEOUT)
}

fn prepare_until(config: &Path, request: &[u8], deadline: Instant) -> io::Result<Option<File>> {
    let directory = config
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(directory)?;
    // Serialize writers until consumption, so simultaneous launchers cannot
    // overwrite a page that the owner has not read yet. This is a request lock,
    // not a second ownership mechanism. Stable inodes are never removed.
    let requests = lock_file(&directory.join("settings-requests.lock"))?;
    while !try_lock(&requests)? {
        wait(deadline)?;
    }
    let owner = lock_file(&directory.join("settings.lock"))?;
    let mailbox = directory.join("settings-command.json");
    let mut sent = false;
    loop {
        if sent && !mailbox.try_exists()? {
            return Ok(None);
        }
        if try_lock(&owner)? {
            match fs::remove_file(&mailbox) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            return Ok(Some(owner));
        }
        if !sent && !mailbox.try_exists()? {
            let temporary = directory.join(format!(".settings-command-{}.tmp", std::process::id()));
            let result = (|| {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&temporary)?;
                file.write_all(request)?;
                file.sync_all()?;
                fs::rename(&temporary, &mailbox)
            })();
            let _ = fs::remove_file(temporary);
            result?;
            sent = true;
        }
        wait(deadline)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "halcyon-lifecycle-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn config(&self) -> std::path::PathBuf {
            self.0.join("config.json")
        }
        fn consume(&self) -> Vec<u8> {
            let path = self.0.join("settings-command.json");
            let deadline = Instant::now() + Duration::from_secs(5);
            while !path.exists() {
                wait(deadline).unwrap();
            }
            let bytes = fs::read(&path).unwrap();
            fs::remove_file(path).unwrap();
            bytes
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn restart_runs_the_installed_program() {
        let fixture = Fixture::new();
        let program = fixture.0.join("halcyon");
        fs::write(&program, "").unwrap();
        // An update replaced the file under the running copy.
        let replaced = fixture.0.join("halcyon (deleted)");
        assert_eq!(installed(&replaced), program);
        assert_eq!(installed(&program), program);
        // Gone altogether: the one on PATH.
        assert_eq!(
            installed(&fixture.0.join("missing (deleted)")),
            Path::new("halcyon")
        );
    }

    #[test]
    fn widget_descendants_use_an_independent_application_scope() {
        for group in [
            "0::/user.slice/halcyon.service",
            "1:name=systemd:/user/halcyon.service/child",
        ] {
            assert!(in_widget_service(group));
        }
        for group in [
            "0::/user.slice/app.slice/halcyon-settings-123.scope",
            "0::/user.slice/app-terminal.scope",
            "0::/halcyon.service-other",
        ] {
            assert!(!in_widget_service(group));
        }
        let command = scope_command(
            Path::new("/private/bin/halcyon"),
            ["--settings", "calendar"].into_iter().map(Into::into),
        );
        let args: Vec<_> = command.get_args().map(|s| s.to_str().unwrap()).collect();
        assert!(args.contains(&"--scope"));
        assert!(args.contains(&"--slice=app.slice"));
        assert!(args.ends_with(&["--", "/private/bin/halcyon", "--settings", "calendar"]));
        assert_eq!(
            command.get_envs().count(),
            0,
            "inherit the session/config environment"
        );
    }

    #[test]
    fn repeated_launches_keep_one_owner_and_deliver_every_page() {
        let fixture = Fixture::new();
        let owner = prepare(&fixture.config(), br#"{"page":"overview"}"#)
            .unwrap()
            .unwrap();
        let launches: Vec<_> = ["calendar", "weather", "projects"]
            .into_iter()
            .map(|page| {
                let config = fixture.config();
                std::thread::spawn(move || {
                    let request = format!("{{\"page\":\"{page}\"}}");
                    assert!(prepare(&config, request.as_bytes()).unwrap().is_none());
                })
            })
            .collect();
        let mut delivered: Vec<_> = (0..3)
            .map(|_| String::from_utf8(fixture.consume()).unwrap())
            .collect();
        delivered.sort();
        assert_eq!(
            delivered,
            [
                r#"{"page":"calendar"}"#,
                r#"{"page":"projects"}"#,
                r#"{"page":"weather"}"#
            ]
        );
        for launch in launches {
            launch.join().unwrap();
        }
        drop(owner);
        assert!(prepare(&fixture.config(), b"{}").unwrap().is_some());
    }

    #[test]
    fn owner_exit_during_handoff_promotes_waiting_launch() {
        let fixture = Fixture::new();
        let owner = prepare(&fixture.config(), b"{}").unwrap().unwrap();
        let config = fixture.config();
        let launch =
            std::thread::spawn(move || prepare(&config, br#"{"page":"calendar"}"#).unwrap());
        let deadline = Instant::now() + Duration::from_secs(5);
        while !fixture.0.join("settings-command.json").exists() {
            wait(deadline).unwrap();
        }
        drop(owner);
        let replacement = launch.join().unwrap().unwrap();
        assert!(!fixture.0.join("settings-command.json").exists());
        drop(replacement);
    }

    #[test]
    fn unresponsive_owner_times_out_without_overwriting_pending_request() {
        let fixture = Fixture::new();
        let _owner = prepare(&fixture.config(), b"{}").unwrap().unwrap();
        let mailbox = fixture.0.join("settings-command.json");
        fs::write(&mailbox, b"earlier request").unwrap();
        let result = prepare_until(
            &fixture.config(),
            b"new request",
            Instant::now() + Duration::from_millis(40),
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert_eq!(fs::read(mailbox).unwrap(), b"earlier request");
    }
}
