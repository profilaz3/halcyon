//! Desktop tools: music controls, notes and a focus timer.
//!
//! The tools' preferences (visibility, card options, durations) live in
//! `config.json` like every widget's. Their private data does not: the note
//! is user content in the data folder and the focus session is runtime state
//! in the state folder, each in a small owner-only file of its own, so
//! neither can reach a profile, an export or the shared configuration.
pub mod cards;
pub mod focus;
pub mod music;
pub mod notes;
mod seek_bar;
pub mod visualizer;

use crate::config::Config;
use std::path::{Path, PathBuf};

/// Where a tool keeps user data (`~/.local/share/halcyon`).
/// With an alternate configuration (previews, captures, tests) the file sits
/// beside that configuration instead, so a preview never touches the real one.
pub fn data_path(file: &str) -> Option<PathBuf> {
    isolated(file).or_else(|| crate::paths::data_dir().map(|dir| dir.join(file)))
}

/// Where a tool keeps runtime state (`~/.local/state/halcyon`).
pub fn state_path(file: &str) -> Option<PathBuf> {
    isolated(file).or_else(|| crate::paths::state_dir().map(|dir| dir.join(file)))
}

fn isolated(file: &str) -> Option<PathBuf> {
    std::env::var_os("DESKTOP_CARDS_CONFIG")?;
    Config::path().map(|path| path.with_file_name(file))
}

/// Whether this process runs against an alternate configuration. Previews
/// never send desktop notifications.
pub fn preview() -> bool {
    std::env::var_os("DESKTOP_CARDS_CONFIG").is_some()
}

/// Replaces `path` atomically with an owner-only file in an owner-only
/// folder. A symlink in its place is refused rather than followed, and the
/// temporary file is always a new one (never an existing file or link).
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or("No folder for this file")?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|error| error.to_string())?;
    if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err("Refusing to replace a link".into());
    }
    let name = path
        .file_name()
        .ok_or("No file name")?
        .to_string_lossy()
        .into_owned();
    let temporary = parent.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, path).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// Reads at most `limit` bytes of the regular file `path`; `Ok(None)` when
/// it is absent. A pipe or device is refused rather than read (it could
/// block or never end), and reading stops past the limit.
pub fn read_bounded(path: &Path, limit: usize) -> Result<Option<Vec<u8>>, String> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let meta = file.metadata().map_err(|error| error.to_string())?;
    if !meta.is_file() {
        return Err("Not a regular file".into());
    }
    if meta.len() > limit as u64 {
        return Err("The file is too large".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > limit {
        return Err("The file is too large".into());
    }
    Ok(Some(bytes))
}

/// A desktop notification through the freedesktop notification service.
/// Best effort: a missing service is not an error the user needs to see.
pub async fn notify(summary: String, body: String) -> Result<(), String> {
    let connection = music::connection().await?;
    let hints: std::collections::HashMap<&str, zbus::zvariant::Value<'_>> =
        std::collections::HashMap::from([("category", zbus::zvariant::Value::from("presence"))]);
    let message = (
        "Halcyon",
        0u32,
        "alarm-symbolic",
        summary.as_str(),
        body.as_str(),
        Vec::<&str>::new(),
        hints,
        -1i32,
    );
    let call = connection.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &message,
    );
    tokio::time::timeout(std::time::Duration::from_secs(3), call)
        .await
        .map_err(|_| "The notification service did not answer".to_string())?
        .map(|_| ())
        .map_err(|error| error.to_string())
}
