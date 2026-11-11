//! Each picture's analysis, kept by the picture's content under the cache
//! folder (`~/.cache/halcyon/depth`). Only the main process writes here; the
//! workers read. Bounded in count and size, oldest use first out.
//!
//! An entry is a folder named by the picture's SHA-256 and [`VERSION`]:
//! `scene.json` (the numbers), `mask.png`, `edges.png` and `picture.png` (the
//! picture at the mask's size, for lighting). Nothing here is ever exported.
use super::scene::Stats;
use image::{GrayImage, RgbImage};
use std::path::{Path, PathBuf};

/// Bumped when the analysis changes, so old entries are not trusted.
pub const VERSION: u32 = 1;
pub const MAX_ENTRIES: usize = 32;
pub const MAX_BYTES: u64 = 256 << 20;
/// No single file in an entry is larger than this.
pub const MAX_FILE: u64 = 48 << 20;

pub fn root() -> Option<PathBuf> {
    crate::paths::cache_dir().map(|dir| dir.join("depth"))
}

/// The picture's content key.
pub fn key(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn valid_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

pub fn dir(root: &Path, key: &str) -> Option<PathBuf> {
    valid_key(key).then(|| root.join(format!("{key}-v{VERSION}")))
}

/// What the scene worker hands back for the main process to keep.
pub struct Files {
    pub stats: Stats,
    pub mask: Vec<u8>,
    pub edges: Vec<u8>,
    pub picture: Vec<u8>,
}

pub struct Entry {
    pub stats: Stats,
    pub mask: GrayImage,
    pub edges: GrayImage,
    pub picture: RgbImage,
}

fn read(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= MAX_FILE).then_some(bytes)
}

fn png(bytes: &[u8]) -> Option<image::DynamicImage> {
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 << 20);
    reader.limits(limits);
    reader.decode().ok()
}

/// Reads an entry, checking that its parts belong together. Never writes.
pub fn load(root: &Path, key: &str) -> Option<Entry> {
    let dir = dir(root, key)?;
    let stats: Stats = serde_json::from_slice(&read(&dir.join("scene.json"))?).ok()?;
    let mask = png(&read(&dir.join("mask.png"))?)?;
    let edges = png(&read(&dir.join("edges.png"))?)?;
    let picture = png(&read(&dir.join("picture.png"))?)?;
    let (mask, edges, picture) = (mask.to_luma8(), edges.to_luma8(), picture.to_rgb8());
    let size = mask.dimensions();
    if edges.dimensions() != size
        || picture.dimensions() != size
        || super::scene::work_size(stats.source) != [size.0, size.1]
    {
        return None;
    }
    Some(Entry {
        stats,
        mask,
        edges,
        picture,
    })
}

pub fn exists(root: &Path, key: &str) -> bool {
    dir(root, key).is_some_and(|dir| dir.join("scene.json").is_file())
}

/// Marks an entry as just used, so pruning keeps it.
pub fn touch(root: &Path, key: &str) {
    if let Some(dir) = dir(root, key)
        && let Ok(file) = std::fs::File::open(dir.join("scene.json"))
    {
        let _ = file.set_modified(std::time::SystemTime::now());
    }
}

/// Stores an entry whole or not at all, then prunes.
pub fn store(root: &Path, key: &str, files: &Files) -> std::io::Result<()> {
    let invalid = || std::io::Error::new(std::io::ErrorKind::InvalidInput, "scene entry");
    let target = dir(root, key).ok_or_else(invalid)?;
    if [&files.mask, &files.edges, &files.picture]
        .iter()
        .any(|bytes| bytes.len() as u64 > MAX_FILE)
    {
        return Err(invalid());
    }
    std::fs::create_dir_all(root)?;
    let temp = root.join(format!(".{key}-{}-{}", std::process::id(), nanos()));
    std::fs::create_dir(&temp)?;
    let written = (|| {
        std::fs::write(temp.join("mask.png"), &files.mask)?;
        std::fs::write(temp.join("edges.png"), &files.edges)?;
        std::fs::write(temp.join("picture.png"), &files.picture)?;
        std::fs::write(
            temp.join("scene.json"),
            serde_json::to_vec(&files.stats).map_err(std::io::Error::other)?,
        )?;
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::rename(&temp, &target)
    })();
    if written.is_err() {
        let _ = std::fs::remove_dir_all(&temp);
    }
    written?;
    prune(root, key, MAX_ENTRIES, MAX_BYTES);
    Ok(())
}

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Removes the least recently used entries, other entry versions and
/// leftovers until at most `count` entries and `bytes` remain. `keep` stays.
pub fn prune(root: &Path, keep: &str, count: usize, bytes: u64) {
    let Ok(read) = std::fs::read_dir(root) else {
        return;
    };
    let mut entries = Vec::new();
    for item in read.flatten() {
        let path = item.path();
        let name = item.file_name().to_string_lossy().into_owned();
        let current = name
            .strip_suffix(&format!("-v{VERSION}"))
            .is_some_and(valid_key);
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !current || !meta.is_dir() {
            // Old versions and unfinished stores from a crash; a store or
            // claim in progress is younger than [`STALE`].
            let fresh = name.starts_with('.')
                && meta
                    .modified()
                    .ok()
                    .and_then(|m| m.elapsed().ok())
                    .is_some_and(|age| age < STALE);
            if !fresh {
                if meta.is_dir() {
                    let _ = std::fs::remove_dir_all(&path);
                } else {
                    let _ = std::fs::remove_file(&path);
                }
            }
            continue;
        }
        let used = std::fs::metadata(path.join("scene.json"))
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        let size: u64 = std::fs::read_dir(&path)
            .map(|files| {
                files
                    .flatten()
                    .filter_map(|f| f.metadata().ok())
                    .map(|m| m.len())
                    .sum()
            })
            .unwrap_or(0);
        entries.push((name.starts_with(keep), used, size, path));
    }
    // Newest first; the entry just stored always stays.
    entries.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let mut total = 0u64;
    for (i, (kept, _, size, path)) in entries.into_iter().enumerate() {
        total += size;
        if !kept && (i >= count || total > bytes) {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

/// Longer than any analysis may run; a claim this old was left behind.
pub const STALE: std::time::Duration = std::time::Duration::from_secs(90);

/// One process analyses a picture at a time, so the desktop and Settings
/// never run the model twice for it. Given up when dropped.
pub struct Claim(Option<PathBuf>);
impl Drop for Claim {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Claims `key` for analysis, or `None` while another process holds it.
/// When the folder can't hold a claim, the analysis goes ahead unclaimed.
pub fn claim(root: &Path, key: &str) -> Option<Claim> {
    dir(root, key)?;
    if std::fs::create_dir_all(root).is_err() {
        return Some(Claim(None));
    }
    let path = root.join(format!(".{key}.claim"));
    for _ in 0..2 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => return Some(Claim(Some(path))),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let stale = std::fs::symlink_metadata(&path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|m| m.elapsed().ok())
                    .is_none_or(|age| age >= STALE);
                if !stale {
                    return None;
                }
                let _ = std::fs::remove_file(&path);
            }
            Err(_) => return Some(Claim(None)),
        }
    }
    None
}

/// Removes every entry, for Remove in Settings.
pub fn clear(root: &Path) {
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(test)]
mod tests;
