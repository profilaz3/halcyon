//! Icon packs: browsing Pling's catalog, installing packs safely, and
//! Halcyon's overlay icon theme (one shape for app icons, folders in the
//! accent color).
//!
//! Every function takes its directories through `Paths`, so tests and the
//! fixture desktop never touch the real home. Installs go to the user icon
//! directory only, and a manifest in Halcyon's data directory records what
//! Halcyon put there, so Remove can never touch anything else.
pub mod archive;
pub mod install;
pub mod lookup;
pub mod ocs;
pub mod overlay;
pub mod raster;
pub mod source;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

/// Where icon packs are installed, looked up, recorded and cached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    /// The user icon directory: installs and the overlay go here.
    pub icons: PathBuf,
    /// Icon directories in lookup order, `icons` first.
    pub search: Vec<PathBuf>,
    /// Halcyon's data directory: the install manifest.
    pub data: PathBuf,
    /// Halcyon's cache directory: preview thumbnails.
    pub cache: PathBuf,
    /// Halcyon's config directory: the chosen base theme, shape and tint.
    pub config: PathBuf,
}

impl Paths {
    /// The real directories, or the fixture desktop's: with a fixture root
    /// the user icon directory is `<root>/icons`, the one the fixture's icon
    /// theme list comes from, and installed themes are still looked up in
    /// the XDG directories after it.
    pub fn from_env(fixture: Option<&Path>) -> Option<Self> {
        let xdg = crate::desktop::appearance::catalog::icon_dirs(
            dirs::data_dir(),
            dirs::home_dir(),
            std::env::var("XDG_DATA_DIRS").ok(),
        );
        let icons = match fixture {
            Some(root) => root.join("icons"),
            None => dirs::data_dir()?.join("icons"),
        };
        let mut search = vec![icons.clone()];
        search.extend(xdg.into_iter().filter(|dir| *dir != icons));
        let config = crate::config::Config::path()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .or_else(crate::paths::config_dir)?;
        Some(Self {
            icons,
            search,
            data: crate::paths::data_dir()?,
            cache: crate::paths::cache_dir()?.join("icon-packs"),
            config,
        })
    }
    pub fn manifest(&self) -> PathBuf {
        self.data.join("icon-packs.json")
    }
    pub fn style(&self) -> PathBuf {
        self.config.join("icon-style.json")
    }
}

/// A theme directory name safe to store and to create: one plain path
/// component, printable, not hidden.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && !id.starts_with('.')
        && !id.contains(['/', '\\'])
        && !id.chars().any(char::is_control)
}

/// Writes `bytes` to `path` through a temporary file in the same directory.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = parent.join(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })
}

/// A short unique suffix for staging directories.
pub(crate) fn unique() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.subsec_nanos())
        .unwrap_or_default();
    format!(
        "{}-{nanos:x}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// The file at `path` if it is a regular file of at most `limit` bytes.
pub(crate) fn read(path: &Path, limit: usize) -> Option<Vec<u8>> {
    crate::tools::read_bounded(path, limit).ok().flatten()
}
