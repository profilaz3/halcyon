//! Folders of .ics files, as vdirsyncer and khal keep them: one event per
//! file or whole calendars, in the folder itself or in one level of
//! subfolders, each subfolder its own calendar with optional `displayname`
//! and `color` files. Everything goes through the isolated parser and the
//! same limits as a link, with one size cap for the whole folder.
use super::*;
use std::path::{Path, PathBuf};

/// More files than this in one calendar folder are refused.
const MAX_FILES: usize = 10_000;
/// Subfolders looked at when finding calendars.
const MAX_SUBFOLDERS: usize = 64;

/// A calendar found in a chosen folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderCalendar {
    pub path: PathBuf,
    pub name: String,
    /// `#rrggbb`, or empty.
    pub color: String,
}

fn is_ics(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("ics"))
}
fn hidden(path: &Path) -> bool {
    path.file_name()
        .is_none_or(|n| n.to_string_lossy().starts_with('.'))
}
/// The .ics files directly inside `dir`, sorted, without following links
/// out of it into devices or pipes (read_bounded refuses those).
fn ics_files(dir: &Path) -> Result<Vec<PathBuf>, Problem> {
    let mut files = vec![];
    for entry in std::fs::read_dir(dir).map_err(|_| Problem::CalendarFile)? {
        let path = entry.map_err(|_| Problem::CalendarFile)?.path();
        if hidden(&path) || !is_ics(&path) || !path.is_file() {
            continue;
        }
        files.push(path);
        if files.len() > MAX_FILES {
            return Err(Problem::FeedLimit);
        }
    }
    files.sort();
    Ok(files)
}
/// A small metadata file vdirsyncer writes beside the events.
fn meta(dir: &Path, name: &str) -> Option<String> {
    let bytes = crate::tools::read_bounded(&dir.join(name), 4096).ok()??;
    let text = String::from_utf8_lossy(&bytes);
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.chars().take(80).collect())
}
fn calendar(dir: &Path) -> FolderCalendar {
    let fallback = dir
        .file_name()
        .map(|n| n.to_string_lossy().chars().take(80).collect::<String>())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Calendar".into());
    FolderCalendar {
        path: dir.to_path_buf(),
        name: meta(dir, "displayname").unwrap_or(fallback),
        color: meta(dir, "color")
            .and_then(|c| caldav::color(&c))
            .unwrap_or_default(),
    }
}

/// Finds the calendars in a chosen folder: the folder itself when it holds
/// .ics files, and each subfolder that does.
pub fn discover(dir: &Path) -> Result<Vec<FolderCalendar>, Problem> {
    if !dir.is_absolute() || !dir.is_dir() {
        return Err(Problem::CalendarFile);
    }
    let mut found = vec![];
    if !ics_files(dir)?.is_empty() {
        found.push(calendar(dir));
    }
    let mut subfolders: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|_| Problem::CalendarFile)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| !hidden(p) && p.is_dir())
        .take(MAX_SUBFOLDERS)
        .collect();
    subfolders.sort();
    for sub in subfolders {
        if ics_files(&sub).is_ok_and(|files| !files.is_empty()) {
            found.push(calendar(&sub));
        }
    }
    if found.is_empty() {
        Err(Problem::EmptyFolder)
    } else {
        Ok(found)
    }
}

/// Reads one calendar folder as a single iCalendar text, within the byte
/// limit for the folder as a whole.
pub(super) fn read(dir: &Path) -> Result<Vec<u8>, Problem> {
    if !dir.is_absolute() || !dir.is_dir() {
        return Err(Problem::CalendarFile);
    }
    let mut total = 0usize;
    let mut objects = vec![];
    for file in ics_files(dir)? {
        if std::fs::metadata(&file).is_ok_and(|m| m.len() > http::MAX_BYTES as u64) {
            return Err(Problem::FeedLimit);
        }
        let bytes = match crate::tools::read_bounded(&file, http::MAX_BYTES) {
            Ok(Some(bytes)) => bytes,
            // Removed between listing and reading: a sync is under way.
            Ok(None) => continue,
            Err(_) => return Err(Problem::CalendarFile),
        };
        total = total.saturating_add(bytes.len());
        if total > http::MAX_BYTES {
            return Err(Problem::FeedLimit);
        }
        objects.push(String::from_utf8_lossy(&bytes).into_owned());
    }
    let merged = caldav::merge(&objects);
    if merged.len() > http::MAX_BYTES {
        return Err(Problem::FeedLimit);
    }
    Ok(merged)
}
