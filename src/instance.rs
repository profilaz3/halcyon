//! The command line in front of the widgets: help, one widgets process per
//! session, and the commands a running one takes from a shortcut.
//!
//! The lock is an advisory `flock` on a file in the runtime folder. The
//! kernel drops it when the process ends, however it ends, so a stale file
//! never blocks the next start. Workers, Settings and the other internal
//! modes never take it.

use crate::drawer::Widget;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub const ALREADY_RUNNING: &str =
    "Halcyon is already running. To change it, run: halcyon --settings";
pub const NOT_RUNNING: &str = "Halcyon isn’t running. Start it with: halcyon-ctl start";

pub fn usage() -> String {
    let widgets = Widget::ALL
        .iter()
        .map(|widget| format!("    {:<10}{}", widget.key(), widget.title()))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Halcyon {version}: desktop widgets and themes for COSMIC

Usage:
  halcyon                    Show the desktop widgets. Only one copy runs at a time.
  halcyon --settings [PAGE]  Open Settings, on PAGE if given (for example clock)
  halcyon --toggle TARGET    Open or close the edge menu, or show or hide a widget
  halcyon --help             Show this help
  halcyon --version          Show the version

TARGET is one of:
    drawer    The edge menu (use Tab, Enter, Alt+Up or Alt+Down, and Esc)
{widgets}

More commands, such as start, stop and focus: halcyon-ctl --help",
        version = env!("CARGO_PKG_VERSION"),
    )
}

/// What `halcyon --toggle TARGET` asks of the running widgets.
#[derive(Debug, PartialEq)]
pub enum Toggle {
    Drawer,
    Widget(Widget),
}
impl Toggle {
    pub fn parse(target: &str) -> Option<Self> {
        match target {
            "drawer" | "menu" => Some(Self::Drawer),
            "system-monitor" => Some(Self::Widget(Widget::Devbox)),
            "daily-overview" => Some(Self::Widget(Widget::Glance)),
            other => Widget::parse(other).map(Self::Widget),
        }
    }
    fn payload(&self) -> serde_json::Value {
        match self {
            Self::Drawer => serde_json::json!({"action": "drawer"}),
            Self::Widget(widget) => serde_json::json!({"action": "toggle", "widget": widget.key()}),
        }
    }
}

/// An isolated preview (and every test) keeps its lock and mailbox beside
/// its own settings, so it never meets the session's widgets.
fn isolated_folder() -> Option<PathBuf> {
    let config = std::env::var_os("DESKTOP_CARDS_CONFIG")?;
    Path::new(&config).parent().map(Path::to_path_buf)
}
pub fn lock_path() -> Option<PathBuf> {
    match std::env::var_os("DESKTOP_CARDS_CONFIG") {
        Some(_) => isolated_folder().map(|dir| dir.join("widgets.lock")),
        None => crate::paths::runtime_dir().map(|dir| dir.join("widgets.lock")),
    }
}
/// The file the running widgets watch for commands.
pub fn command_path() -> Option<PathBuf> {
    match std::env::var_os("DESKTOP_CARDS_CONFIG") {
        Some(_) => isolated_folder().map(|dir| dir.join("desktop-command.json")),
        None => crate::paths::runtime_dir().map(|dir| dir.join("command.json")),
    }
}

pub enum Lock {
    /// This process is the session's widgets; keep the file open.
    Held(#[allow(dead_code)] File),
    /// Another process holds it.
    Busy,
    /// No runtime folder or lock support: start anyway rather than never.
    Unavailable(String),
}

pub fn acquire_at(path: &Path) -> Lock {
    if let Some(parent) = path.parent() {
        if let Err(error) = std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
        {
            return Lock::Unavailable(error.to_string());
        }
    }
    // Rust opens files close-on-exec, so no child ever inherits the lock.
    let file = match std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path)
    {
        Ok(file) => file,
        Err(error) => return Lock::Unavailable(error.to_string()),
    };
    // SAFETY: flock on a descriptor this function owns.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Lock::Held(file);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
        Lock::Busy
    } else {
        Lock::Unavailable(error.to_string())
    }
}
pub fn acquire() -> Lock {
    match lock_path() {
        Some(path) => acquire_at(&path),
        None => Lock::Unavailable("No runtime folder".into()),
    }
}

/// Hands a command to the running widgets.
pub fn send(toggle: &Toggle) -> Result<(), String> {
    if !matches!(acquire(), Lock::Busy) {
        return Err(NOT_RUNNING.into());
    }
    let path = command_path().ok_or_else(|| "No runtime folder".to_string())?;
    write_atomic(&path, toggle.payload().to_string().as_bytes()).map_err(|e| e.to_string())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let temporary = parent.join(format!(".command-{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// Handles the commands that must never open a window. `Some(code)` means
/// the process is done and exits with it.
pub fn early_command(command: Option<&str>, argument: Option<&str>) -> Option<i32> {
    match command? {
        "-h" | "--help" | "help" => {
            println!("{}", usage());
            Some(0)
        }
        "-V" | "--version" => {
            println!("halcyon {}", env!("CARGO_PKG_VERSION"));
            Some(0)
        }
        "--toggle" => {
            let Some(toggle) = argument.and_then(Toggle::parse) else {
                eprintln!(
                    "Choose what to toggle: drawer, or a widget such as clock or notes.\nSee: halcyon --help"
                );
                return Some(2);
            };
            match send(&toggle) {
                Ok(()) => Some(0),
                Err(error) => {
                    eprintln!("{error}");
                    Some(1)
                }
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("halcyon-instance-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn dt01_second_lock_is_busy_and_a_stale_file_never_blocks() {
        let dir = folder("lock");
        let path = dir.join("halcyon/widgets.lock");
        let first = acquire_at(&path);
        assert!(matches!(first, Lock::Held(_)));
        assert!(matches!(acquire_at(&path), Lock::Busy));
        // The process ends: the file stays, the lock goes.
        drop(first);
        assert!(path.exists());
        assert!(matches!(acquire_at(&path), Lock::Held(_)));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn dt01_lock_folder_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = folder("private");
        let path = dir.join("halcyon/widgets.lock");
        let _lock = acquire_at(&path);
        let mode = std::fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn dt01_help_names_every_command_and_widget() {
        let text = usage();
        for needle in [
            "--settings",
            "--toggle",
            "--help",
            "--version",
            "drawer",
            "halcyon-ctl",
        ] {
            assert!(text.contains(needle), "{needle}");
        }
        for widget in Widget::ALL {
            assert!(text.contains(widget.key()) && text.contains(widget.title()));
        }
    }

    #[test]
    fn ax04_toggle_targets_parse_to_the_mailbox_actions() {
        assert_eq!(Toggle::parse("drawer"), Some(Toggle::Drawer));
        assert_eq!(Toggle::parse("notes"), Some(Toggle::Widget(Widget::Notes)));
        assert_eq!(
            Toggle::parse("system-monitor"),
            Some(Toggle::Widget(Widget::Devbox))
        );
        assert_eq!(Toggle::parse("nothing"), None);
        assert_eq!(Toggle::Drawer.payload()["action"], "drawer");
        assert_eq!(
            Toggle::Widget(Widget::Music).payload(),
            serde_json::json!({"action": "toggle", "widget": "music"})
        );
    }

    #[test]
    fn ax04_atomic_command_replaces_the_mailbox() {
        let dir = folder("mailbox");
        let path = dir.join("halcyon/command.json");
        write_atomic(&path, b"{\"action\":\"drawer\"}").unwrap();
        write_atomic(&path, b"{\"action\":\"toggle\",\"widget\":\"notes\"}").unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["widget"], "notes");
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
