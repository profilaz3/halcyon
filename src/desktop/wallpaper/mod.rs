//! Wallpaper: the shared background, per-display backgrounds and whether
//! every display shows the same one.
//!
//! Source (pinned): cosmic-bg 91d525b — `config/src/lib.rs` (`Entry`,
//! `Source`, `Color`, `Gradient`, the `all` / `same-on-all` / `backgrounds`
//! / `output.<name>` keys, `Entry::fallback`), `src/main.rs` (per-key watch
//! handling: a display uses its own `output.<name>` entry only while it is
//! listed in `backgrounds` and `same-on-all` is off, otherwise `all`),
//! `src/wallpaper.rs` (recursive, symlink-following folder walk),
//! `src/scaler.rs` (Fit / Stretch / Zoom) and `src/colored.rs` (gradients
//! are blended in linear RGB).
//!
//! Keys are read only through `DesktopStore::load`; the upstream readers are
//! never used (`same_on_all()` writes when it reads). Every edit rewrites a
//! whole entry from its decoded current value, so members the draft does
//! not change (`filter_by_theme`, `filter_method`, …) keep their values. A
//! value with members, variants or bounds this version does not support is
//! kept and never rewritten; other entries stay editable.
use super::appearance::{Blocked, Plan, decode_exact};
use super::panels::{self, Setting};
use super::store::{
    ConfigKey, DesktopError, DesktopPatch, DesktopStore, KeyEntry, KeyRule, Namespace, RawValue,
    Snapshot,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub mod automatic;
pub mod collection;
pub mod media;
pub mod modes;
pub mod palette;

pub const NAMESPACE: &str = "com.system76.CosmicBackground";
pub const SHARED: &str = "all";
pub const SAME_ON_ALL: &str = "same-on-all";
pub const BACKGROUNDS: &str = "backgrounds";
pub const OUTPUT_PREFIX: &str = "output.";
/// Application-supported bounds, not upstream schema limits: values beyond
/// them are kept as they are and only block an edit of that entry.
pub const MAX_OUTPUTS: usize = 16;

/// The built-in wallpapers (file stem, title): the Halcyon scenes the
/// starter looks use, then one per widget palette.
pub const BUNDLED: [(&str, &str); 17] = [
    ("halcyon-ridge", "Ridge"),
    ("halcyon-night", "Halcyon night"),
    ("halcyon-calm", "Calm"),
    ("halcyon-evening", "Evening"),
    ("halcyon-focus", "Focus"),
    ("halcyon-workbench", "Workbench"),
    ("halcyon-paper", "Paper"),
    ("halcyon-rose", "Blush"),
    ("halcyon-rain", "Rainy evening"),
    ("halcyon-snow", "First snow"),
    ("halcyon-fog", "Morning fog"),
    ("lavender", "Lavender"),
    ("amber", "Amber"),
    ("teal", "Teal"),
    ("ocean", "Ocean blue"),
    ("rose", "Rose"),
    ("graphite", "Graphite"),
];

/// The built-in wallpaper called `name`, as its static stem.
pub fn bundled_name(name: &str) -> Option<&'static str> {
    BUNDLED
        .iter()
        .map(|(known, _)| *known)
        .find(|known| *known == name)
}
pub const MAX_ROTATION: u64 = 604_800;
/// The slideshow intervals COSMIC Settings offers, in seconds.
pub const ROTATIONS: [u64; 6] = [300, 600, 900, 1800, 3600, 7200];
const MAX_NAME: usize = 48;
const MAX_PATH_BYTES: usize = 4096;
const MAX_STOPS: usize = 8;
/// The image types cosmic-bg decodes.
pub const IMAGE_EXTENSIONS: [&str; 7] = ["png", "jpg", "jpeg", "webp", "jxl", "avif", "hdr"];
/// Folder walk bounds (cosmic-bg itself is unbounded).
pub const WALK_DEPTH: usize = 8;
pub const WALK_ENTRIES: usize = 10_000;
pub const WALK_DIRS: usize = 256;

pub fn shared_key() -> ConfigKey {
    ConfigKey::new(NAMESPACE, 1, SHARED).expect("background key is valid")
}
pub fn same_key() -> ConfigKey {
    ConfigKey::new(NAMESPACE, 1, SAME_ON_ALL).expect("background key is valid")
}
pub fn list_key() -> ConfigKey {
    ConfigKey::new(NAMESPACE, 1, BACKGROUNDS).expect("background key is valid")
}
/// `output.<name>` for a display name `valid_output_name` accepts.
pub fn output_key(name: &str) -> Result<ConfigKey, DesktopError> {
    if !valid_output_name(name) {
        return Err(DesktopError::InvalidKey(format!(
            "Invalid display name “{name}”"
        )));
    }
    ConfigKey::new(NAMESPACE, 1, format!("{OUTPUT_PREFIX}{name}"))
}
/// 1–48 characters of `[A-Za-z0-9._-]`, not starting or ending with `.`,
/// without `..`.
pub fn valid_output_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !name.contains("..")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}
fn output_name(key: &ConfigKey) -> Option<&str> {
    (modes::is_background(key.namespace()) && key.version() == 1)
        .then(|| key.key().strip_prefix(OUTPUT_PREFIX))
        .flatten()
}

/// Upstream `Entry`, member for member.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BgEntry {
    pub output: String,
    pub source: BgSource,
    pub filter_by_theme: bool,
    pub rotation_frequency: u64,
    /// The fade between pictures in milliseconds (COSMIC 1.11 and later).
    /// Kept as it was stored: absent stays absent, so older cosmic-bg
    /// versions never see a member they would refuse.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "plain_duration"
    )]
    pub transition_duration: Option<u64>,
    #[serde(default)]
    pub filter_method: FilterMethod,
    #[serde(default)]
    pub scaling_mode: ScalingMode,
    #[serde(default)]
    pub sampling_method: SamplingMethod,
}

/// `transition_duration` is a plain number upstream, not `Some(..)`.
mod plain_duration {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(value) => serializer.serialize_u64(*value),
            None => serializer.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<u64>, D::Error> {
        u64::deserialize(deserializer).map(Some)
    }
}

/// Upstream `Source`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BgSource {
    Path(PathBuf),
    Color(BgColor),
}

/// Upstream `Color`: solid colours are sRGB, gradient stops linear RGB.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BgColor {
    Single([f32; 3]),
    Gradient(BgGradient),
}

/// Upstream `Gradient`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BgGradient {
    pub colors: Vec<[f32; 3]>,
    pub radius: f32,
}

/// Upstream `FilterMethod`; stored but unused (always Lanczos3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterMethod {
    Nearest,
    Linear,
    #[default]
    Lanczos,
}

/// Upstream `ScalingMode`; `Fit` carries the sRGB letterbox colour.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum ScalingMode {
    Fit([f32; 3]),
    Stretch,
    #[default]
    Zoom,
}

/// Upstream `SamplingMethod`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SamplingMethod {
    #[default]
    Alphanumeric,
    Random,
}

impl BgEntry {
    /// Upstream `Entry::new`.
    pub fn new(output: impl Into<String>, source: BgSource) -> Self {
        Self {
            output: output.into(),
            source,
            filter_by_theme: false,
            rotation_frequency: 900,
            transition_duration: None,
            filter_method: FilterMethod::default(),
            scaling_mode: ScalingMode::default(),
            sampling_method: SamplingMethod::default(),
        }
    }
    /// Upstream `Entry::fallback`, what cosmic-bg shows when `all` can't be
    /// read from any layer.
    pub fn fallback() -> Self {
        Self {
            output: SHARED.into(),
            source: BgSource::Path(PathBuf::from(
                "/usr/share/backgrounds/cosmic/orion_nebula_nasa_heic0601a.jpg",
            )),
            filter_by_theme: true,
            rotation_frequency: 3600,
            transition_duration: None,
            filter_method: FilterMethod::default(),
            scaling_mode: ScalingMode::default(),
            sampling_method: SamplingMethod::default(),
        }
    }
}

/// The sRGB transfer to linear light.
pub fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
/// Linear light to the sRGB transfer.
pub fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

fn check_rgb(rgb: &[f32; 3]) -> Result<(), String> {
    if rgb
        .iter()
        .all(|component| component.is_finite() && (0.0..=1.0).contains(component))
    {
        Ok(())
    } else {
        Err("colour components must be between 0 and 1".into())
    }
}

/// Supported bounds of a decoded entry (see `check_entry`).
pub fn entry_problem(entry: &BgEntry) -> Option<String> {
    let result = (|| {
        if entry.output.is_empty() || entry.output.chars().count() > MAX_NAME {
            return Err("the display name is empty or too long".to_string());
        }
        match &entry.source {
            BgSource::Path(path) => {
                let text = path.to_str().ok_or("the picture path is not text")?;
                if !path.is_absolute()
                    || text.len() > MAX_PATH_BYTES
                    || text.chars().any(char::is_control)
                {
                    return Err("the picture path is not a plain absolute path".into());
                }
            }
            BgSource::Color(BgColor::Single(rgb)) => check_rgb(rgb)?,
            BgSource::Color(BgColor::Gradient(gradient)) => {
                if !(2..=MAX_STOPS).contains(&gradient.colors.len()) {
                    return Err("a gradient needs 2 to 8 colours".into());
                }
                for stop in &gradient.colors {
                    check_rgb(stop)?;
                }
                if !gradient.radius.is_finite() || !(0.0..=360.0).contains(&gradient.radius) {
                    return Err("the gradient angle must be between 0 and 360".into());
                }
            }
        }
        if let ScalingMode::Fit(rgb) = &entry.scaling_mode {
            check_rgb(rgb)?;
        }
        if entry.rotation_frequency > MAX_ROTATION {
            return Err("the slideshow interval is longer than a week".into());
        }
        Ok(())
    })();
    result.err()
}

fn read_entry(raw: &RawValue) -> Result<BgEntry, String> {
    let entry = decode_exact::<BgEntry>(raw)?;
    match entry_problem(&entry) {
        Some(problem) => Err(problem),
        None => Ok(entry),
    }
}

/// `all` and `output.*`: an exact entry within the supported bounds.
pub fn check_entry(value: &RawValue) -> Result<(), String> {
    read_entry(value).map(drop)
}

fn read_list(raw: &RawValue) -> Result<Vec<String>, String> {
    let list = decode_exact::<Vec<String>>(raw)?;
    if list.len() > MAX_OUTPUTS {
        return Err("lists more than 16 displays".into());
    }
    let mut seen = BTreeSet::new();
    for name in &list {
        if !valid_output_name(name) || !seen.insert(name) {
            return Err(format!(
                "lists a display this version can’t address: {name}"
            ));
        }
    }
    Ok(list)
}

/// `backgrounds`: at most 16 unique, valid display names.
pub fn check_list(value: &RawValue) -> Result<(), String> {
    read_list(value).map(drop)
}

/// `same-on-all`: a bool.
pub fn check_same(value: &RawValue) -> Result<(), String> {
    decode_exact::<bool>(value).map(drop)
}

/// The four keys this app writes, in the shared namespace and in both
/// per-mode namespaces (`modes`); `output.*` matches only valid display
/// names in these namespaces (see `KeyRule::matches`).
pub static RULES: [KeyRule; 12] = [
    rule(NAMESPACE, SHARED, check_entry),
    rule(NAMESPACE, SAME_ON_ALL, check_same),
    rule(NAMESPACE, BACKGROUNDS, check_list),
    rule(NAMESPACE, "output.*", check_entry),
    rule(modes::DARK, SHARED, check_entry),
    rule(modes::DARK, SAME_ON_ALL, check_same),
    rule(modes::DARK, BACKGROUNDS, check_list),
    rule(modes::DARK, "output.*", check_entry),
    rule(modes::LIGHT, SHARED, check_entry),
    rule(modes::LIGHT, SAME_ON_ALL, check_same),
    rule(modes::LIGHT, BACKGROUNDS, check_list),
    rule(modes::LIGHT, "output.*", check_entry),
];

const fn rule(
    namespace: &'static str,
    key: &'static str,
    validate: fn(&RawValue) -> Result<(), String>,
) -> KeyRule {
    KeyRule {
        namespace: Namespace::Exact(namespace),
        version: 1,
        key,
        validate,
    }
}

/// Write order within one change: entries before the list that refers to
/// them, the mode last (cosmic-bg reloads per key event). Every other key
/// ranks 0, so other domains keep their key order.
pub fn write_rank(key: &ConfigKey) -> u8 {
    if !modes::is_background(key.namespace()) || key.version() != 1 {
        return 0;
    }
    match key.key() {
        SHARED => 1,
        BACKGROUNDS => 2,
        SAME_ON_ALL => 3,
        _ => 0,
    }
}

/// Short user-facing name of a background key, for notices.
pub fn key_label(key: &ConfigKey) -> Option<String> {
    if !modes::is_background(key.namespace()) {
        return None;
    }
    let mode = match key.namespace() {
        modes::DARK => " in dark mode",
        modes::LIGHT => " in light mode",
        _ => "",
    };
    if let Some(name) = output_name(key) {
        return Some(format!("background for {name}{mode}"));
    }
    let label = match key.key() {
        SHARED => "shared background",
        BACKGROUNDS => "display list",
        SAME_ON_ALL => "same background on all displays",
        _ => return None,
    };
    Some(format!("{label}{mode}"))
}

/// Connected display connectors: `<fixture>/outputs` (one per line; none
/// when absent) or DRM sysfs. Reads only.
pub fn discover_outputs(store: &DesktopStore) -> Vec<String> {
    let mut names: Vec<String> = match store.fixture_root() {
        Some(root) => std::fs::read_to_string(root.join("outputs"))
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|name| valid_output_name(name))
            .map(String::from)
            .collect(),
        None => panels::connected_outputs(Path::new("/sys/class/drm"))
            .into_iter()
            .filter(|name| valid_output_name(name))
            .collect(),
    };
    names.sort();
    names.dedup();
    names
}

/// The outcome of a bounded folder walk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Walk {
    /// A bound was reached, so some files were not visited.
    pub truncated: bool,
}

/// Bounds of one folder walk.
#[derive(Clone, Copy, Debug)]
struct Limits {
    depth: usize,
    entries: usize,
    dirs: usize,
}
const LIMITS: Limits = Limits {
    depth: WALK_DEPTH,
    entries: WALK_ENTRIES,
    dirs: WALK_DIRS,
};

/// Visits every file below `path` like cosmic-bg (recursive, following
/// symbolic links, unreadable entries skipped), in path order, but bounded:
/// depth 8, 10,000 entries and 256 distinct directories, which also stops
/// symlink loops. A listing is read only up to the remaining entry budget,
/// so a folder cut at the bound is sorted only within what was read.
/// `visit` returns `false` to stop early. Reads only.
pub fn walk_folder(path: &Path, visit: impl FnMut(&Path) -> bool) -> Result<Walk, String> {
    walk_with(path, LIMITS, visit).map(|(walk, _)| walk)
}
/// `walk_folder` with explicit bounds; also returns how many directory
/// entries were taken from listings.
fn walk_with(
    path: &Path,
    limits: Limits,
    visit: impl FnMut(&Path) -> bool,
) -> Result<(Walk, usize), String> {
    walk_listing(path, limits, |_| {}, visit)
}
/// `walk_with` that also names every directory it lists, just before the
/// listing is read (`listed`), so a later change to it is noticed.
fn walk_listing(
    path: &Path,
    limits: Limits,
    mut listed: impl FnMut(&Path),
    mut visit: impl FnMut(&Path) -> bool,
) -> Result<(Walk, usize), String> {
    let root =
        std::fs::canonicalize(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if !root.is_dir() {
        return Err(format!("{} is not a folder", path.display()));
    }
    let mut walk = Walk::default();
    let mut seen = BTreeSet::from([root]);
    let mut entries = 0usize;
    let mut taken = 0usize;
    // Depth-first in sorted order, like the alphanumeric slideshow.
    let mut stack: Vec<(PathBuf, usize)> = vec![(path.to_path_buf(), 0)];
    let mut first = true;
    while let Some((dir, depth)) = stack.pop() {
        listed(&dir);
        let listing = match std::fs::read_dir(&dir) {
            Ok(listing) => listing,
            Err(error) if first => return Err(format!("{}: {error}", path.display())),
            Err(_) => continue,
        };
        first = false;
        // At most the remaining budget plus one entry, which shows the
        // bound was reached; entries that fail to read count too.
        let budget = limits.entries.saturating_sub(entries) + 1;
        let mut read = 0usize;
        let mut children: Vec<PathBuf> = listing
            .take(budget)
            .inspect(|_| read += 1)
            .flatten()
            .map(|entry| entry.path())
            .collect();
        taken += read;
        entries += read - children.len();
        if entries > limits.entries {
            walk.truncated = true;
            return Ok((walk, taken));
        }
        children.sort();
        let mut folders = Vec::new();
        for child in children {
            entries += 1;
            if entries > limits.entries {
                walk.truncated = true;
                return Ok((walk, taken));
            }
            let Ok(metadata) = std::fs::metadata(&child) else {
                continue;
            };
            if metadata.is_dir() {
                if depth + 1 > limits.depth {
                    walk.truncated = true;
                    continue;
                }
                let Ok(canonical) = std::fs::canonicalize(&child) else {
                    continue;
                };
                if seen.contains(&canonical) {
                    continue;
                }
                if seen.len() >= limits.dirs {
                    walk.truncated = true;
                    continue;
                }
                seen.insert(canonical);
                folders.push(child);
            } else if metadata.is_file() && !visit(&child) {
                return Ok((walk, taken));
            }
        }
        stack.extend(folders.into_iter().rev().map(|folder| (folder, depth + 1)));
    }
    Ok((walk, taken))
}

/// Whether `path` has a picture cosmic-bg decodes (by extension).
pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

/// Whether a bounded walk of `path` finds at least one picture, also in
/// subfolders. A walk that reaches a bound before finding one is an error,
/// not "no pictures".
pub fn folder_has_image(path: &Path) -> Result<bool, String> {
    folder_has_image_with(path, LIMITS)
}
fn folder_has_image_with(path: &Path, limits: Limits) -> Result<bool, String> {
    let mut found = false;
    let (walk, _) = walk_with(path, limits, |file| {
        found = is_image(file);
        !found
    })?;
    if !found && walk.truncated {
        return Err(format!(
            "This folder is too large to check: {}",
            path.display()
        ));
    }
    Ok(found)
}

/// A chosen source path that would be shown is usable: UTF-8, an existing
/// file, or a folder with at least one picture. Reads only.
pub fn check_path(path: &Path) -> Result<(), String> {
    if path.to_str().is_none() {
        return Err(format!(
            "{}: this picture’s location can’t be used",
            path.display()
        ));
    }
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(metadata) if metadata.is_dir() => match folder_has_image(path)? {
            true => Ok(()),
            false => Err(format!("No pictures in this folder: {}", path.display())),
        },
        Ok(_) => Err(format!("Not a picture or folder: {}", path.display())),
        Err(_) => Err(format!(
            "This picture was moved or deleted: {}",
            path.display()
        )),
    }
}

/// The built-in wallpapers, installed with Halcyon (see `paths`).
pub fn bundled_dir(store: &DesktopStore) -> PathBuf {
    match store.fixture_root() {
        Some(root) => root.join("pictures/Desktop Cards"),
        None => crate::paths::backgrounds().unwrap_or_default(),
    }
}
/// The built-in wallpaper `name` (a 3840x2160 JPEG).
pub fn bundled_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!(
        "{name}.{}",
        if name == "halcyon-ridge" {
            "png"
        } else {
            "jpg"
        }
    ))
}
/// The built-in wallpaper a path from an install before the rename names:
/// `Desktop Cards/<name>-4k.png` or `-1080p.png` in the Pictures folder.
fn legacy_bundled(path: &Path) -> Option<&'static str> {
    if path.parent()?.file_name()? != "Desktop Cards" {
        return None;
    }
    let file = path.file_name()?.to_str()?;
    BUNDLED.iter().map(|(name, _)| *name).find(|name| {
        ["4k", "1080p"]
            .iter()
            .any(|size| file == format!("{name}-{size}.png"))
    })
}
/// Trusted system `backgrounds` directories: `$XDG_DATA_DIRS`, without any
/// entry inside the home folder or the user's data home (also through a
/// symbolic link); fixture `<root>/backgrounds`.
pub fn background_dirs(store: &DesktopStore) -> Vec<PathBuf> {
    if let Some(root) = store.fixture_root() {
        return vec![root.join("backgrounds")];
    }
    let private: Vec<PathBuf> = [dirs::home_dir(), dirs::data_dir()]
        .into_iter()
        .flatten()
        .collect();
    trusted_background_dirs(std::env::var("XDG_DATA_DIRS").ok().as_deref(), &private)
}
/// `<dir>/backgrounds` for each plain absolute `data_dirs` entry (default
/// `/usr/local/share:/usr/share`) that is not inside a `private` folder,
/// compared as written and with symbolic links resolved.
fn trusted_background_dirs(data_dirs: Option<&str>, private: &[PathBuf]) -> Vec<PathBuf> {
    let private: Vec<PathBuf> = private
        .iter()
        .filter(|root| root.is_absolute() && root.parent().is_some())
        .flat_map(|root| [root.clone(), resolved(root)])
        .collect();
    let inside = |path: &Path| private.iter().any(|root| path.starts_with(root));
    let dirs = data_dirs
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("/usr/local/share:/usr/share");
    let mut result: Vec<PathBuf> = Vec::new();
    for dir in dirs.split(':').map(PathBuf::from) {
        let dir = dir.join("backgrounds");
        let plain = dir.components().all(|component| {
            matches!(
                component,
                std::path::Component::RootDir | std::path::Component::Normal(_)
            )
        });
        if plain
            && dir.is_absolute()
            && !inside(&dir)
            && !inside(&resolved(&dir))
            && !result.contains(&dir)
        {
            result.push(dir);
        }
    }
    result
}
/// `path` with its longest existing prefix resolved (symbolic links).
fn resolved(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut current = path;
    loop {
        if let Ok(canonical) = std::fs::canonicalize(current) {
            return rest
                .iter()
                .rev()
                .fold(canonical, |result, part| result.join(part));
        }
        match (current.parent(), current.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                current = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Where a picture or folder comes from, for collections and profiles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Class {
    /// A built-in wallpaper (its `BUNDLED` stem).
    Bundled(&'static str),
    /// Inside a trusted system backgrounds directory (relative path).
    System(String),
    Personal,
}

/// Bundled when `path` is a bundled wallpaper location; System only when it
/// resolves inside one of `dirs` with plain relative components and no
/// escaping symlink; otherwise Personal.
pub fn classify(path: &Path, bundled_dir: &Path, dirs: &[PathBuf]) -> Class {
    for (name, _) in BUNDLED {
        if path == bundled_path(bundled_dir, name) {
            return Class::Bundled(name);
        }
    }
    if let Some(name) = legacy_bundled(path) {
        return Class::Bundled(name);
    }
    for dir in dirs {
        let Ok(relative) = path.strip_prefix(dir) else {
            continue;
        };
        let parts: Option<Vec<&str>> = relative
            .components()
            .map(|component| match component {
                std::path::Component::Normal(part) => part.to_str(),
                _ => None,
            })
            .collect();
        let Some(parts) = parts.filter(|parts| !parts.is_empty()) else {
            continue;
        };
        let plain = parts.iter().all(|part| {
            part != &".."
                && part.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b' ' | b'-')
                })
        });
        let contained = match (std::fs::canonicalize(dir), std::fs::canonicalize(path)) {
            (Ok(dir), Ok(path)) => path.starts_with(dir),
            _ => false,
        };
        if plain && contained {
            return Class::System(parts.join("/"));
        }
    }
    Class::Personal
}

/// What an edit addresses: the shared entry or one display's own entry.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Target {
    Shared,
    Output(String),
}

/// Whether every display shows the shared background.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Displays {
    Same,
    Separate,
}

/// One display the page lists.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayView {
    pub name: String,
    pub connected: bool,
    /// In the drafted `backgrounds` list.
    pub listed: bool,
    /// Its own entry as drafted (or stored), when it has one.
    pub own: Option<Setting<BgEntry>>,
}

/// Member edits of one entry; `None` keeps the member.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Members {
    source: Option<BgSource>,
    scaling: Option<ScalingMode>,
    rotation: Option<u64>,
    sampling: Option<SamplingMethod>,
    /// A display gets its own entry even where no member differs from
    /// where it starts (`assign_own`).
    own: bool,
}

impl Members {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }
    fn apply(&self, entry: &mut BgEntry) {
        if let Some(source) = &self.source {
            entry.source = source.clone();
        }
        if let Some(scaling) = &self.scaling {
            entry.scaling_mode = scaling.clone();
        }
        if let Some(rotation) = self.rotation {
            entry.rotation_frequency = rotation;
        }
        if let Some(sampling) = self.sampling {
            entry.sampling_method = sampling;
        }
    }
    /// Forgets members that already hold the edited value.
    fn trim(&mut self, base: &BgEntry) {
        if self.source.as_ref() == Some(&base.source) {
            self.source = None;
        }
        if self.scaling.as_ref() == Some(&base.scaling_mode) {
            self.scaling = None;
        }
        if self.rotation == Some(base.rotation_frequency) {
            self.rotation = None;
        }
        if self.sampling == Some(base.sampling_method) {
            self.sampling = None;
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Edits {
    displays: Option<Displays>,
    entries: BTreeMap<Target, Members>,
    /// Listed displays returned to the shared background.
    shared_only: BTreeSet<String>,
}

impl Edits {
    fn is_empty(&self) -> bool {
        self.displays.is_none() && self.entries.is_empty() && self.shared_only.is_empty()
    }
}

/// One edit, for dependency tracking.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Displays,
    Entry(Target),
    SharedOnly(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WallpaperDraft {
    base: Snapshot,
    /// Observations, not baselines: connected displays and display entries
    /// whose files can't be read. Refreshed by `sync_metadata`.
    connected: Vec<String>,
    unreadable: BTreeMap<String, String>,
    edits: Edits,
    conflicts: Vec<ConfigKey>,
    /// Where the values were read and where the plan writes them: the
    /// shared namespace, or the per-mode one cosmic-bg shows (`modes`).
    /// Everything else in the draft names keys by their shared names.
    #[serde(default = "shared_namespace")]
    namespace: String,
    /// Whether this desktop keeps per-mode sets, so a change of mode or a
    /// set cosmic-bg seeds can change what is shown.
    #[serde(default)]
    per_mode: bool,
}

fn shared_namespace() -> String {
    NAMESPACE.into()
}

/// `keys` (shared names) read from `namespace`, under their shared names.
pub fn load_from(
    store: &DesktopStore,
    namespace: &str,
    keys: &[ConfigKey],
) -> Result<Snapshot, DesktopError> {
    let stored: Vec<ConfigKey> = keys
        .iter()
        .map(|key| modes::rehome(key, namespace))
        .collect();
    let loaded = store.load(&stored)?;
    let mut snapshot = Snapshot::default();
    for (key, stored) in keys.iter().zip(&stored) {
        snapshot.insert(
            key.clone(),
            loaded.entry(stored).cloned().unwrap_or_default(),
        );
    }
    Ok(snapshot)
}

impl WallpaperDraft {
    /// The fixed keys; per-display keys follow discovery (`watch_keys`).
    pub fn keys() -> Vec<ConfigKey> {
        vec![shared_key(), same_key(), list_key()]
    }

    /// Reads the fixed keys and the entry of every connected or listed
    /// display, without writing. The list is reread after the entries; if
    /// it changed in between, the read is retried so no newly listed
    /// display is missed. An unreadable display entry is kept and noted.
    pub fn load(store: &DesktopStore) -> Result<Self, DesktopError> {
        let connected = discover_outputs(store);
        let namespace = modes::current(store);
        for _ in 0..3 {
            let list = load_from(store, namespace, &[list_key()])?;
            let mut names: BTreeSet<String> = connected.iter().cloned().collect();
            names.extend(listed_names(list.effective(&list_key())));
            let mut snapshot = load_from(store, namespace, &Self::keys())?;
            if snapshot.entry(&list_key()) != list.entry(&list_key()) {
                continue;
            }
            let mut unreadable = BTreeMap::new();
            for name in names {
                let key = output_key(&name)?;
                match load_from(store, namespace, std::slice::from_ref(&key)) {
                    Ok(one) => {
                        snapshot.insert(key.clone(), one.entry(&key).cloned().unwrap_or_default())
                    }
                    Err(error) => {
                        unreadable.insert(name, error.to_string());
                    }
                }
            }
            if load_from(store, namespace, &[list_key()])?.entry(&list_key())
                != list.entry(&list_key())
            {
                continue;
            }
            let mut draft = Self::from_parts(snapshot, connected);
            draft.unreadable = unreadable;
            draft.namespace = namespace.into();
            draft.per_mode = modes::per_mode(store);
            return Ok(draft);
        }
        Err(DesktopError::Io(
            "The display list kept changing while it was read. Try again.".into(),
        ))
    }
    pub fn from_parts(base: Snapshot, connected: Vec<String>) -> Self {
        Self {
            base,
            connected,
            unreadable: BTreeMap::new(),
            edits: Edits::default(),
            conflicts: Vec::new(),
            namespace: shared_namespace(),
            per_mode: false,
        }
    }
    /// The fixed keys and `extra` (shared names) from the namespace
    /// cosmic-bg shows now, without discovering displays.
    pub fn load_keys(
        store: &DesktopStore,
        extra: &[ConfigKey],
        connected: Vec<String>,
    ) -> Result<Self, DesktopError> {
        let namespace = modes::current(store);
        let mut keys = Self::keys();
        keys.extend(extra.iter().cloned());
        let mut draft = Self::from_parts(load_from(store, namespace, &keys)?, connected);
        draft.namespace = namespace.into();
        draft.per_mode = modes::per_mode(store);
        Ok(draft)
    }
    /// The namespace the draft reads and writes.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }
    /// The stored key a shared name stands for in this draft.
    fn stored(&self, key: &ConfigKey) -> ConfigKey {
        modes::rehome(key, &self.namespace)
    }
    /// Every stored key the draft read or tried to (the fixed keys and each
    /// display's entry, also one that couldn't be read). With per-mode sets,
    /// the same names in all three namespaces and the theme mode, since
    /// either picks which keys cosmic-bg shows.
    pub fn watch_keys(&self) -> Vec<ConfigKey> {
        let names = self.read_keys();
        if !self.per_mode {
            return names.iter().map(|key| self.stored(key)).collect();
        }
        let mut keys: Vec<ConfigKey> = modes::NAMESPACES
            .iter()
            .flat_map(|namespace| names.iter().map(|key| modes::rehome(key, namespace)))
            .collect();
        keys.push(modes::mode_key());
        keys
    }
    /// Requires every stored key the draft read to be unchanged when
    /// `patch` is applied.
    pub fn guard(&self, patch: &mut DesktopPatch) {
        for key in self.read_keys() {
            patch.guard(self.stored(&key), self.base.user(&key).cloned());
        }
    }
    /// The shared names of every key the draft read or tried to.
    fn read_keys(&self) -> Vec<ConfigKey> {
        let mut keys: Vec<ConfigKey> = Self::keys();
        let unreadable = self
            .unreadable
            .keys()
            .filter_map(|name| output_key(name).ok());
        for key in self.base.keys().cloned().chain(unreadable) {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        keys
    }
    /// Takes fresh observations (connected displays, unreadable entries)
    /// from `fresh`; never its values, baselines or edits.
    pub fn sync_metadata(&mut self, fresh: &Self) {
        self.connected = fresh.connected.clone();
        self.unreadable = fresh.unreadable.clone();
        self.namespace = fresh.namespace.clone();
        self.per_mode = fresh.per_mode;
    }
    pub fn snapshot(&self) -> &Snapshot {
        &self.base
    }
    pub fn into_snapshot(self) -> Snapshot {
        self.base
    }
    pub fn dirty(&self) -> bool {
        !self.edits.is_empty()
    }
    pub fn conflicts(&self) -> &[ConfigKey] {
        &self.conflicts
    }
    pub fn discard(&mut self) {
        self.edits = Edits::default();
        self.conflicts.clear();
    }

    // Stored values.

    /// The shared entry: the user file decoded exactly; else the system
    /// default, or upstream's fallback when that can't be decoded (as
    /// cosmic-bg does). A user value that can't be decoded is kept.
    fn base_shared(&self) -> Setting<BgEntry> {
        let entry = self.base.entry(&shared_key()).cloned().unwrap_or_default();
        match (&entry.user, &entry.system) {
            (Some(raw), _) => setting(raw, SHARED),
            // A default that decodes but is out of bounds or names a display
            // is kept; only one that can't be decoded falls back.
            (None, Some(raw)) => match decode_exact::<BgEntry>(raw) {
                Ok(value) if entry_problem(&value).is_none() && value.output == SHARED => {
                    Setting::Known(value)
                }
                Ok(_) => Setting::Unsupported(raw.as_str().trim().to_string()),
                Err(_) => Setting::Known(BgEntry::fallback()),
            },
            (None, None) => Setting::Known(BgEntry::fallback()),
        }
    }
    fn base_displays(&self) -> Setting<Displays> {
        match self.base.effective(&same_key()) {
            None => Setting::Known(Displays::Same),
            Some(raw) => match decode_exact::<bool>(raw) {
                Ok(true) => Setting::Known(Displays::Same),
                Ok(false) => Setting::Known(Displays::Separate),
                Err(_) => Setting::Unsupported(raw.as_str().trim().to_string()),
            },
        }
    }
    fn base_list(&self) -> Setting<Vec<String>> {
        match self.base.effective(&list_key()) {
            None => Setting::Known(Vec::new()),
            Some(raw) => match read_list(raw) {
                Ok(list) => Setting::Known(list),
                Err(_) => Setting::Unsupported(raw.as_str().trim().to_string()),
            },
        }
    }
    /// A display's stored entry, when it has a file. One naming another
    /// display is kept (cosmic-bg shows it there).
    fn stored_own(&self, name: &str) -> Option<Setting<BgEntry>> {
        if let Some(reason) = self.unreadable.get(name) {
            return Some(Setting::Unsupported(reason.clone()));
        }
        let key = output_key(name).ok()?;
        self.base.effective(&key).map(|raw| setting(raw, name))
    }

    // Drafted values for a set of edits (the stored ones for none).

    fn displays_with(&self, edits: &Edits) -> Setting<Displays> {
        match edits.displays {
            Some(displays) => Setting::Known(displays),
            None => self.base_displays(),
        }
    }
    fn separate_with(&self, edits: &Edits) -> bool {
        self.displays_with(edits) == Setting::Known(Displays::Separate)
    }
    fn shared_with(&self, edits: &Edits) -> Setting<BgEntry> {
        match (self.base_shared(), edits.entries.get(&Target::Shared)) {
            (Setting::Known(mut entry), Some(members)) => {
                members.apply(&mut entry);
                entry.output = SHARED.into();
                Setting::Known(entry)
            }
            (shared, _) => shared,
        }
    }
    /// Whether `name` uses its own stored entry as the starting point.
    fn listed_in_base(&self, name: &str) -> bool {
        self.base_list()
            .known()
            .is_some_and(|list| list.iter().any(|listed| listed == name))
    }
    /// Where an edit of `name` starts: its own entry when it is listed and
    /// readable, otherwise the (drafted) shared entry. A readable unlisted
    /// leftover is replaced, never resurrected.
    fn start_of(&self, edits: &Edits, name: &str) -> (Setting<BgEntry>, bool) {
        if self.listed_in_base(name)
            && let Some(Setting::Known(own)) = self.stored_own(name)
        {
            return (Setting::Known(own), false);
        }
        (self.shared_with(edits), true)
    }
    fn own_with(&self, edits: &Edits, name: &str) -> Option<Setting<BgEntry>> {
        let Some(members) = edits.entries.get(&Target::Output(name.to_string())) else {
            return self.stored_own(name);
        };
        if let Some(Setting::Unsupported(text)) = self.stored_own(name) {
            return Some(Setting::Unsupported(text));
        }
        Some(match self.start_of(edits, name).0 {
            Setting::Known(mut entry) => {
                members.apply(&mut entry);
                entry.output = name.to_string();
                Setting::Known(entry)
            }
            unsupported => unsupported,
        })
    }
    /// The drafted list; `None` when the stored list can't be rewritten.
    fn list_with(&self, edits: &Edits) -> Setting<Vec<String>> {
        if edits.displays == Some(Displays::Same) {
            return Setting::Known(Vec::new());
        }
        match self.base_list() {
            Setting::Known(mut list) => {
                list.retain(|name| !edits.shared_only.contains(name));
                for target in edits.entries.keys() {
                    if let Target::Output(name) = target
                        && !list.contains(name)
                    {
                        list.push(name.clone());
                    }
                }
                Setting::Known(list)
            }
            unsupported => unsupported,
        }
    }
    fn effective_with(&self, edits: &Edits, target: &Target) -> Setting<BgEntry> {
        match target {
            Target::Output(name) => self
                .shown_own(edits, name)
                .unwrap_or_else(|| self.shared_with(edits)),
            Target::Shared => self.shared_with(edits),
        }
    }
    /// What display `name` shows instead of the shared entry after `edits`;
    /// `None` when it shows the shared one. Where this version can't tell
    /// exactly what cosmic-bg shows, the kept value is `Unsupported`.
    fn shown_own(&self, edits: &Edits, name: &str) -> Option<Setting<BgEntry>> {
        if !self.separate_with(edits) {
            return None;
        }
        match self.list_with(edits) {
            Setting::Known(list) => {
                if let Some(text) = self.claimed_by_other(&list, name) {
                    return Some(Setting::Unsupported(text));
                }
                if !list.iter().any(|listed| listed == name) {
                    return None;
                }
                match self.own_with(edits, name) {
                    Some(Setting::Known(own)) => Some(Setting::Known(own)),
                    // Kept here, but cosmic-bg still reads and shows it.
                    Some(Setting::Unsupported(text)) if self.runtime_reads(name) => {
                        Some(Setting::Unsupported(text))
                    }
                    _ => None,
                }
            }
            // cosmic-bg still reads a list this version can't rewrite.
            Setting::Unsupported(text) => {
                let list = self.runtime_list();
                let unknown = list.iter().any(|listed| !valid_output_name(listed));
                let own = list.iter().any(|listed| listed == name) && self.runtime_reads(name);
                (unknown || own || self.claimed_by_other(&list, name).is_some())
                    .then_some(Setting::Unsupported(text))
            }
        }
    }
    /// The stored list as cosmic-bg reads it (no bounds; empty when it
    /// can't be read).
    fn runtime_list(&self) -> Vec<String> {
        self.base
            .effective(&list_key())
            .and_then(|raw| raw.decode::<Vec<String>>().ok())
            .unwrap_or_default()
    }
    /// The entry cosmic-bg reads for `name` (it decodes without this
    /// version's bounds; unknown members or variants make it use `all`).
    fn runtime_entry(&self, name: &str) -> Option<BgEntry> {
        if self.unreadable.contains_key(name) {
            return None;
        }
        output_key(name)
            .ok()
            .and_then(|key| self.base.effective(&key))
            .and_then(|raw| raw.decode::<BgEntry>().ok())
    }
    /// Whether cosmic-bg shows the display's own stored entry on it: it
    /// places an entry on the display its `output` member names.
    fn runtime_reads(&self, name: &str) -> bool {
        self.runtime_entry(name)
            .is_some_and(|entry| entry.output == name)
    }
    /// The stored text of another listed display's entry that names `name`.
    fn claimed_by_other(&self, list: &[String], name: &str) -> Option<String> {
        list.iter()
            .filter(|listed| *listed != name && valid_output_name(listed))
            .find(|listed| {
                self.runtime_entry(listed)
                    .is_some_and(|entry| entry.output == name)
            })
            .and_then(|listed| output_key(listed).ok())
            .and_then(|key| self.base.effective(&key))
            .map(|raw| raw.as_str().trim().to_string())
    }

    // The page's view.

    pub fn displays(&self) -> Setting<Displays> {
        self.displays_with(&self.edits)
    }
    /// Connected displays, then listed ones that are not connected (kept
    /// and shown as such), then displays only the draft added.
    pub fn display_views(&self) -> Vec<DisplayView> {
        let list = match self.list_with(&self.edits) {
            Setting::Known(list) => list,
            Setting::Unsupported(_) => Vec::new(),
        };
        let mut names: Vec<String> = self.connected.clone();
        for name in self
            .base
            .effective(&list_key())
            .map(|raw| raw.decode::<Vec<String>>().unwrap_or_default())
            .unwrap_or_default()
            .into_iter()
            .chain(list.iter().cloned())
        {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
            .into_iter()
            .map(|name| DisplayView {
                connected: self.connected.contains(&name),
                listed: list.contains(&name),
                own: valid_output_name(&name)
                    .then(|| self.own_with(&self.edits, &name))
                    .flatten(),
                name,
            })
            .collect()
    }
    /// The drafted entry of `target`; `None` when a display uses the shared
    /// background.
    pub fn entry(&self, target: &Target) -> Option<Setting<BgEntry>> {
        match target {
            Target::Shared => Some(self.shared_with(&self.edits)),
            Target::Output(name) => {
                if !self.separate_with(&self.edits) {
                    return None;
                }
                match self.list_with(&self.edits) {
                    Setting::Known(list) if list.contains(name) => self.own_with(&self.edits, name),
                    Setting::Known(_) => None,
                    // Listed in a list this version can't rewrite: kept.
                    Setting::Unsupported(text) => self
                        .runtime_list()
                        .contains(name)
                        .then_some(Setting::Unsupported(text)),
                }
            }
        }
    }
    /// What COSMIC would show for `target` after the draft (spec §3.3).
    pub fn effective(&self, target: &Target) -> Setting<BgEntry> {
        self.effective_with(&self.edits, target)
    }

    // Edits.

    pub fn set_displays(&mut self, displays: Displays) {
        self.edits.displays = Some(displays);
        if displays == Displays::Same {
            // The list is cleared; display entries are left as they are.
            self.edits
                .entries
                .retain(|target, _| *target == Target::Shared);
            self.edits.shared_only.clear();
        }
        if self.base_displays() == Setting::Known(displays) {
            self.edits.displays = None;
        }
    }
    fn edit(&mut self, target: &Target, change: impl FnOnce(&mut Members)) {
        if let Target::Output(name) = target {
            // An entry that can't be read has no baseline to edit from; an
            // older edit made on a readable one is kept.
            if !valid_output_name(name)
                || !self.separate_with(&self.edits)
                || self.unreadable.contains_key(name)
            {
                return;
            }
            self.edits.shared_only.remove(name);
        }
        let mut members = self.edits.entries.remove(target).unwrap_or_default();
        change(&mut members);
        let start = match target {
            Target::Shared => self.base_shared(),
            Target::Output(name) => self.start_of(&self.edits, name).0,
        };
        if let Setting::Known(start) = start {
            members.trim(&start);
        }
        if !members.is_empty() {
            self.edits.entries.insert(target.clone(), members);
        }
    }
    pub fn set_source(&mut self, target: &Target, source: BgSource) {
        self.edit(target, |members| members.source = Some(source));
    }
    pub fn set_scaling(&mut self, target: &Target, scaling: ScalingMode) {
        self.edit(target, |members| members.scaling = Some(scaling));
    }
    pub fn set_rotation(&mut self, target: &Target, seconds: u64) {
        self.edit(target, |members| members.rotation = Some(seconds));
    }
    pub fn set_sampling(&mut self, target: &Target, sampling: SamplingMethod) {
        self.edit(target, |members| members.sampling = Some(sampling));
    }
    /// Gives display `name` its own entry, starting from where an edit of
    /// it starts, even when no member changes (a profile's arrangement
    /// lists exactly its displays). A display already listed keeps its
    /// entry; one that can't be read is left alone.
    pub fn assign_own(&mut self, name: &str) {
        if !self.listed_in_base(name) || self.edits.shared_only.contains(name) {
            self.edit(&Target::Output(name.to_string()), |members| {
                members.own = true;
            });
        }
    }
    /// Returns a display to the shared background: it leaves the list; its
    /// own file is left untouched (as COSMIC Settings does).
    pub fn use_shared(&mut self, name: &str) {
        self.edits.entries.remove(&Target::Output(name.to_string()));
        if self.listed_in_base(name) && self.separate_with(&self.edits) {
            self.edits.shared_only.insert(name.to_string());
        }
    }

    // Applying.

    fn parts(&self) -> Vec<Part> {
        let mut parts = Vec::new();
        if self.edits.displays.is_some() {
            parts.push(Part::Displays);
        }
        parts.extend(self.edits.entries.keys().cloned().map(Part::Entry));
        parts.extend(self.edits.shared_only.iter().cloned().map(Part::SharedOnly));
        parts
    }
    /// The keys one edit reads, including the values it newly shows.
    fn depends(&self, part: &Part) -> Vec<ConfigKey> {
        match part {
            Part::Displays => {
                let mut keys = vec![same_key(), list_key()];
                let listed = listed_names(self.base.effective(&list_key()));
                let shared = match self.edits.displays {
                    // Listed displays show their own entries again, or the
                    // shared one where they have none cosmic-bg reads. An
                    // unreadable entry has no baseline to guard.
                    Some(Displays::Separate) => {
                        let mut shared = false;
                        for name in &listed {
                            if self.edits.shared_only.contains(name) {
                                continue;
                            }
                            if !self.unreadable.contains_key(name) {
                                keys.extend(output_key(name));
                            }
                            shared |= self.shown_own(&self.edits, name).is_none();
                        }
                        shared
                    }
                    // The shared entry appears where a display showed another.
                    Some(Displays::Same) => {
                        let none = Edits::default();
                        let mut names = self.connected.clone();
                        names.extend(listed);
                        names
                            .iter()
                            .any(|name| self.shown_own(&none, name).is_some())
                    }
                    None => false,
                };
                if shared {
                    keys.push(shared_key());
                }
                keys
            }
            Part::Entry(Target::Shared) => vec![shared_key()],
            Part::Entry(Target::Output(name)) => {
                let mut keys = vec![same_key(), list_key()];
                keys.extend(output_key(name));
                if self.start_of(&self.edits, name).1 {
                    keys.push(shared_key());
                }
                keys
            }
            Part::SharedOnly(_) => vec![same_key(), list_key(), shared_key()],
        }
    }
    fn depends_all(&self) -> BTreeSet<ConfigKey> {
        self.parts()
            .iter()
            .flat_map(|part| self.depends(part))
            .collect()
    }

    /// Only the entries, list and mode the edits change, each rewritten from
    /// its decoded current value; inputs it derived from are guarded.
    pub fn plan(&self) -> Plan {
        let mut patch = DesktopPatch::new();
        let mut blocked = Vec::new();
        if !self.dirty() {
            return Plan { patch, blocked };
        }
        let user = |key: &ConfigKey| self.base.user(key).cloned();
        let mut block = |key: ConfigKey, reason: &str| {
            blocked.push(Blocked {
                key,
                reason: reason.into(),
            })
        };
        let encoded = |patch: &mut DesktopPatch, key: ConfigKey, entry: &BgEntry| {
            match RawValue::encode(entry) {
                Ok(value) => match check_entry(&value) {
                    Ok(()) => patch.set(key.clone(), user(&key), Some(value)),
                    Err(reason) => return Err((key, reason)),
                },
                Err(error) => return Err((key, error.to_string())),
            }
            Ok(())
        };
        let mut failures = Vec::new();
        if self.edits.entries.contains_key(&Target::Shared) {
            match self.shared_with(&self.edits) {
                Setting::Known(entry) => {
                    if let Err(failure) = encoded(&mut patch, shared_key(), &entry) {
                        failures.push(failure);
                    }
                }
                Setting::Unsupported(_) => failures.push((
                    shared_key(),
                    "is set elsewhere with settings this version does not support; it is kept"
                        .into(),
                )),
            }
        }
        let mut list_touched = !self.edits.shared_only.is_empty();
        for target in self.edits.entries.keys() {
            let Target::Output(name) = target else {
                continue;
            };
            let Ok(key) = output_key(name) else {
                continue;
            };
            list_touched = true;
            match self.own_with(&self.edits, name) {
                Some(Setting::Known(entry)) => {
                    if let Err(failure) = encoded(&mut patch, key, &entry) {
                        failures.push(failure);
                    }
                    if self.start_of(&self.edits, name).1 {
                        patch.guard(shared_key(), user(&shared_key()));
                    }
                }
                _ if matches!(self.stored_own(name), Some(Setting::Unsupported(_))) => failures
                    .push((
                        key,
                        "has settings this version can’t read or rewrite; it is kept".into(),
                    )),
                _ => failures.push((
                    shared_key(),
                    "is set elsewhere with settings this version does not support; it is kept"
                        .into(),
                )),
            }
        }
        if let Some(displays) = self.edits.displays {
            match self.base_displays() {
                Setting::Known(_) => match RawValue::encode(&(displays == Displays::Same)) {
                    Ok(value) => patch.set(same_key(), user(&same_key()), Some(value)),
                    Err(error) => failures.push((same_key(), error.to_string())),
                },
                Setting::Unsupported(_) => failures.push((
                    same_key(),
                    "is set elsewhere to a value this version does not recognize".into(),
                )),
            }
            if displays == Displays::Same {
                // The runtime keeps per-display entries it already loaded
                // unless the list is cleared too.
                list_touched = true;
            } else {
                // Separate would show a listed entry that can't be read
                // unchecked (`depends` has no baseline for it); Use shared
                // for that display clears this.
                for name in listed_names(self.base.effective(&list_key())) {
                    if self.unreadable.contains_key(&name)
                        && !self.edits.shared_only.contains(&name)
                        && let Ok(key) = output_key(&name)
                    {
                        failures.push((
                            key,
                            "can’t be read, so it can’t be checked before it is shown; it is kept"
                                .into(),
                        ));
                    }
                }
            }
        } else if list_touched {
            patch.guard(same_key(), user(&same_key()));
        }
        for (key, reason) in failures {
            block(key, &reason);
        }
        if list_touched {
            let current = self.base_list();
            match (self.list_with(&self.edits), current) {
                (Setting::Known(list), Setting::Known(current)) => {
                    if list.len() > MAX_OUTPUTS {
                        block(list_key(), "can list at most 16 displays");
                    } else if list != current {
                        match RawValue::encode(&list) {
                            Ok(value) => patch.set(list_key(), user(&list_key()), Some(value)),
                            Err(error) => block(list_key(), &error.to_string()),
                        }
                    } else {
                        patch.guard(list_key(), user(&list_key()));
                    }
                }
                _ => block(
                    list_key(),
                    "lists displays this version can’t address; it is kept",
                ),
            }
        }
        // Every input the edits read and don't write must be unchanged when
        // the patch is applied (the same keys `stale` rechecks).
        for key in self.depends_all() {
            patch.guard(key.clone(), user(&key));
        }
        Plan {
            patch: self.stored_patch(patch),
            blocked,
        }
    }
    /// `patch` (shared names) with every key at its stored place.
    fn stored_patch(&self, patch: DesktopPatch) -> DesktopPatch {
        if self.namespace == NAMESPACE {
            return patch;
        }
        let mut stored = DesktopPatch::new();
        for (key, entry) in patch.iter() {
            stored.set(
                self.stored(key),
                entry.original.clone(),
                entry.desired.clone(),
            );
        }
        for (key, original) in patch.guards() {
            stored.guard(self.stored(key), original.clone());
        }
        stored
    }

    /// The paths this draft makes newly shown somewhere: a changed source,
    /// or a background a display starts showing. Unchanged displays are
    /// never checked, so an unrelated missing picture doesn't block Apply.
    pub fn edited_paths(&self) -> Vec<PathBuf> {
        let none = Edits::default();
        let mut targets = vec![Target::Shared];
        targets.extend(
            self.display_views()
                .into_iter()
                .filter(|view| valid_output_name(&view.name))
                .map(|view| Target::Output(view.name)),
        );
        let mut paths: Vec<PathBuf> = Vec::new();
        for target in targets {
            let before = self.effective_with(&none, &target);
            let after = self.effective_with(&self.edits, &target);
            let source = |entry: &Setting<BgEntry>| entry.known().map(|entry| entry.source.clone());
            if source(&before) == source(&after) {
                continue;
            }
            if let Some(BgSource::Path(path)) = source(&after)
                && !paths.contains(&path)
            {
                paths.push(path);
            }
        }
        paths
    }
    /// Re-checks every newly shown path (`check_path`). Reads only; a file
    /// can still disappear afterwards.
    pub fn check_sources(&self) -> Result<(), String> {
        self.edited_paths()
            .iter()
            .try_for_each(|path| check_path(path))
    }

    /// Inputs the edits read whose value differs in `fresh` (including a
    /// changed system default).
    pub fn stale(&self, fresh: &Snapshot) -> Vec<ConfigKey> {
        self.depends_all()
            .into_iter()
            .filter(|key| changed(&self.base, fresh, key))
            .collect()
    }

    /// Adopts outside changes; with edits, the keys they read keep their
    /// base so Apply detects the change, listed as conflicts.
    pub fn refresh(&mut self, snapshot: Snapshot) {
        if !self.dirty() {
            self.base = snapshot;
            self.conflicts.clear();
            return;
        }
        let depends = self.depends_all();
        self.conflicts = depends
            .iter()
            .filter(|key| changed(&self.base, &snapshot, key))
            .cloned()
            .collect();
        // An input the edits read keeps its old entry; one they planned
        // against without reading (absent from the old base) stays absent,
        // as the plan guards it, so it never adopts a fresh value.
        let mut base = snapshot;
        for key in depends {
            let entry = self.base.entry(&key).cloned().unwrap_or_default();
            base.insert(key, entry);
        }
        self.base = base;
    }
    /// Keeps every edit over the outside changes: the current values of
    /// the conflicting keys become the baseline, so Apply writes the edits
    /// over them and still refuses anything changed after this.
    pub fn keep_edits(&mut self, snapshot: Snapshot) {
        let keys = std::mem::take(&mut self.conflicts);
        self.base.adopt(&snapshot, &keys);
        self.refresh(snapshot);
    }
    /// Drops only the edits that read `keys` (or the current conflicts),
    /// adopts current values and keeps the other edits on their baselines.
    pub fn resolve_conflicts(&mut self, snapshot: Snapshot, keys: &[ConfigKey]) {
        // A refused Apply names stored keys; the edits use shared names.
        let keys: Vec<ConfigKey> = if keys.is_empty() {
            self.conflicts.clone()
        } else {
            keys.iter().map(modes::logical).collect()
        };
        for part in self.parts() {
            if self.depends(&part).iter().any(|key| keys.contains(key)) {
                match part {
                    Part::Displays => self.edits.displays = None,
                    Part::Entry(target) => {
                        self.edits.entries.remove(&target);
                    }
                    Part::SharedOnly(name) => {
                        self.edits.shared_only.remove(&name);
                    }
                }
            }
        }
        self.conflicts.clear();
        self.refresh(snapshot);
    }
}

/// An entry stored for `name` (`all` or a display): known when it is
/// within the supported bounds and its `output` member names `name`.
fn setting(raw: &RawValue, name: &str) -> Setting<BgEntry> {
    match read_entry(raw) {
        Ok(entry) if entry.output == name => Setting::Known(entry),
        _ => Setting::Unsupported(raw.as_str().trim().to_string()),
    }
}

fn listed_names(raw: Option<&RawValue>) -> Vec<String> {
    raw.and_then(|raw| raw.decode::<Vec<String>>().ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|name| valid_output_name(name))
        .collect()
}

fn changed(before: &Snapshot, after: &Snapshot, key: &ConfigKey) -> bool {
    let empty = KeyEntry::default();
    let old = before.entry(key).unwrap_or(&empty);
    let new = after.entry(key).unwrap_or(&empty);
    old.user != new.user || old.effective() != new.effective()
}

#[cfg(test)]
mod tests;
