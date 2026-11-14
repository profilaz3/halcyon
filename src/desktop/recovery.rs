//! Immutable, scoped pre-mutation evidence. Opaque bytes are deliberately
//! separate from RawValue: capture never weakens ordinary Apply validation.
//!
//! Caller order is config lock, then desktop lock. Each domain has its own
//! capture date. Adding a supported domain appends dated current-state evidence;
//! it never relabels or replaces existing evidence as Original desktop.
use super::store::{ConfigKey, DesktopError, DesktopStore, Namespace, widget};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CString, OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path};

pub const FORMAT_VERSION: u32 = 1;
pub const MAX_FILE_BYTES: usize = 256 * 1024;
pub const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_FILES: usize = 4096;
const MAX_MANIFEST_BYTES: usize = 32 * 1024 * 1024;
const FORMAT: &str = "halcyon-desktop-baseline";
const DIRECTORY: &str = "desktop-recovery";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineKind {
    OriginalDesktop,
    CurrentDesktop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryDomain {
    Appearance,
    DockPanel,
    Wallpaper,
    DesktopIcons,
    Windows,
    Workspaces,
    Shortcuts,
    WindowRules,
    Islands,
    Materials,
    WidgetAppearance,
    WidgetLayout,
    AppAnimations,
}
impl RecoveryDomain {
    pub const ALL: [Self; 12] = [
        Self::Appearance,
        Self::DockPanel,
        Self::Wallpaper,
        Self::DesktopIcons,
        Self::Windows,
        Self::Workspaces,
        Self::Shortcuts,
        Self::WindowRules,
        Self::Islands,
        Self::Materials,
        Self::WidgetAppearance,
        Self::WidgetLayout,
    ];
}

/// Base64 name and contents preserve unknown/non-UTF8 files without interpreting
/// them as writable keys. `None` bytes means the user key was absent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeRecord {
    pub namespace: String,
    pub version: u64,
    pub name: String,
    pub bytes: Option<String>,
    pub sha256: Option<String>,
    pub mode: Option<u32>,
}
impl NativeRecord {
    pub fn key_name(&self) -> Result<OsString, DesktopError> {
        let bytes = STANDARD.decode(&self.name).map_err(failed)?;
        component(&bytes).map_err(failed)?;
        Ok(OsString::from_vec(bytes))
    }
    pub fn contents(&self) -> Result<Option<Vec<u8>>, DesktopError> {
        self.bytes
            .as_ref()
            .map(|bytes| STANDARD.decode(bytes).map_err(failed))
            .transpose()
    }
    /// Only supported keys may be offered for restoration. The next stage
    /// must also validate their schema and exact current bytes under lock.
    pub fn supported_key(&self) -> Option<ConfigKey> {
        let name = self.key_name().ok()?;
        let key = ConfigKey::new(&self.namespace, self.version, name.to_str()?).ok()?;
        super::store::rule_for(&key).map(|_| key)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeInventory {
    pub namespace: String,
    pub version: u64,
    pub present: bool,
    /// All observed file names, base64 encoded. New files are not implicitly
    /// owned: restore must restrict deletions to supported Halcyon writes.
    pub names: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppLeaf {
    pub pointer: String,
    pub present: bool,
    pub value: serde_json::Value,
}
/// The one supported GSettings preference, not a dconf database dump.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotionRecord {
    pub backend: String,
    pub bytes: Option<String>,
    pub sha256: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainSnapshot {
    pub domain: RecoveryDomain,
    pub kind: BaselineKind,
    pub captured_at: String,
    pub inventory: Vec<NativeInventory>,
    pub native: Vec<NativeRecord>,
    pub app: Vec<AppLeaf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<MotionRecord>,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineInfo {
    pub format: String,
    pub version: u32,
    pub kind: BaselineKind,
    pub captured_at: String,
    pub app_config_present: bool,
    pub supported_domains: Vec<RecoveryDomain>,
    pub domains: Vec<DomainSnapshot>,
}
fn failed(error: impl std::fmt::Display) -> DesktopError {
    DesktopError::Io(format!(
        "Could not preserve the desktop before changing it: {error}. Nothing was changed; check Recovery and try again"
    ))
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn domain_digest(domain: &DomainSnapshot) -> Result<String, DesktopError> {
    let mut content = domain.clone();
    content.sha256.clear();
    Ok(digest(&serde_json::to_vec(&content).map_err(failed)?))
}

/// A directory descriptor opened one component at a time, without following
/// links. Descendant opens stay anchored even if a parent is renamed meanwhile.
struct Directory(File);

#[cfg(test)]
thread_local! {
    /// fsync calls made by the desktop store on this thread, for tests that
    /// keep a transaction's durability cost in check.
    pub(crate) static SYNCS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
/// `fsync` for the desktop store's own files and folders.
pub(crate) fn sync(file: &File) -> io::Result<()> {
    #[cfg(test)]
    SYNCS.with(|count| count.set(count.get() + 1));
    file.sync_all()
}
fn component(bytes: &[u8]) -> io::Result<CString> {
    if bytes.is_empty() || bytes == b"." || bytes == b".." || bytes.contains(&b'/') {
        return Err(io::Error::other("unsafe path component"));
    }
    CString::new(bytes).map_err(|_| io::Error::other("NUL in path"))
}
impl Directory {
    fn open(path: &Path, create: bool) -> io::Result<Option<Self>> {
        if !path.is_absolute() {
            return Err(io::Error::other("an absolute path is required"));
        }
        let mut dir = Self(File::open("/")?);
        for part in path.components() {
            match part {
                Component::RootDir => {}
                Component::Normal(name) => match dir.child(name, create)? {
                    Some(next) => dir = next,
                    None => return Ok(None),
                },
                _ => return Err(io::Error::other("unsafe path")),
            }
        }
        Ok(Some(dir))
    }
    fn child(&self, name: &OsStr, create: bool) -> io::Result<Option<Self>> {
        let name = component(name.as_bytes())?;
        if create {
            // SAFETY: directory and NUL-terminated name live across the call.
            if unsafe { libc::mkdirat(self.0.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::AlreadyExists {
                    return Err(error);
                }
            } else {
                // A promoted manifest must remain reachable after power loss,
                // including when this capture created its parent directories.
                sync(&self.0)?;
            }
        }
        // O_NOFOLLOW checks the component at open time, including live roots.
        let fd = unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::NotFound {
                Ok(None)
            } else {
                Err(error)
            };
        }
        // SAFETY: openat returned a new owned descriptor.
        Ok(Some(Self(unsafe { File::from_raw_fd(fd) })))
    }
    fn names(&self) -> io::Result<Vec<OsString>> {
        let mut names = Vec::new();
        // proc resolves our already-open descriptor, not an untrusted path.
        for entry in fs::read_dir(format!("/proc/self/fd/{}", self.0.as_raw_fd()))? {
            names.push(entry?.file_name());
            if names.len() > MAX_FILES {
                return Err(io::Error::other("too many settings files"));
            }
        }
        names.sort();
        Ok(names)
    }
    fn read(&self, name: &OsStr, max: usize) -> io::Result<Option<(Vec<u8>, u32)>> {
        let name = component(name.as_bytes())?;
        let fd = unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::NotFound {
                Ok(None)
            } else {
                Err(error)
            };
        }
        let file = unsafe { File::from_raw_fd(fd) };
        let meta = file.metadata()?;
        if !meta.is_file() || meta.len() > max as u64 {
            return Err(io::Error::other("setting is not a bounded regular file"));
        }
        let mut bytes = Vec::new();
        file.take(max as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > max {
            return Err(io::Error::other("setting exceeds capture limit"));
        }
        Ok(Some((bytes, meta.permissions().mode() & 0o777)))
    }
    fn write_new(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        self.write_new_mode(name, bytes, 0o600)
    }
    fn write_new_mode(&self, name: &str, bytes: &[u8], mode: u32) -> io::Result<()> {
        let name = component(name.as_bytes())?;
        let fd = unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut file = unsafe { File::from_raw_fd(fd) };
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.write_all(bytes)?;
        sync(&file)
    }
    fn rename(&self, from: &str, to: &str, replace: bool) -> io::Result<()> {
        let from = component(from.as_bytes())?;
        let to = component(to.as_bytes())?;
        let result = unsafe {
            libc::renameat2(
                self.0.as_raw_fd(),
                from.as_ptr(),
                self.0.as_raw_fd(),
                to.as_ptr(),
                if replace { 0 } else { libc::RENAME_NOREPLACE },
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        sync(&self.0)
    }
    /// Like `rename`, without syncing the folder: the caller syncs it once
    /// for a whole batch of renames.
    fn rename_unsynced(&self, from: &str, to: &str) -> io::Result<()> {
        let from = component(from.as_bytes())?;
        let to = component(to.as_bytes())?;
        // SAFETY: directory and NUL-terminated names live across the call.
        let result = unsafe {
            libc::renameat2(
                self.0.as_raw_fd(),
                from.as_ptr(),
                self.0.as_raw_fd(),
                to.as_ptr(),
                0,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
/// Open a stable lock inode without traversing untrusted ancestors or leaves.
/// Existing special files are inspected through O_PATH before any read/write
/// open; reopening our own descriptor cannot race a pathname replacement.
pub(crate) fn open_lock_file(path: &Path) -> io::Result<File> {
    let path = std::path::absolute(path)?;
    if path
        .components()
        .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
    {
        return Err(io::Error::other("unsafe lock path"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing lock parent"))?;
    let dir =
        Directory::open(parent, true)?.ok_or_else(|| io::Error::other("missing lock directory"))?;
    let name = component(
        path.file_name()
            .ok_or_else(|| io::Error::other("missing lock name"))?
            .as_bytes(),
    )?;
    // An absent leaf is created exclusively: even a dangling link is EEXIST.
    let fd = unsafe {
        libc::openat(
            dir.0.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd >= 0 {
        // SAFETY: openat returned a newly owned regular-file descriptor.
        return Ok(unsafe { File::from_raw_fd(fd) });
    }
    let error = io::Error::last_os_error();
    if error.kind() != io::ErrorKind::AlreadyExists {
        return Err(error);
    }
    // O_PATH neither opens a device nor waits for a FIFO peer. O_NOFOLLOW
    // returns a link's own descriptor, which the regular-file check rejects.
    let fd = unsafe {
        libc::openat(
            dir.0.as_raw_fd(),
            name.as_ptr(),
            libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let entry = unsafe { File::from_raw_fd(fd) };
    if !entry.metadata()?.is_file() {
        return Err(io::Error::other("lock is not a regular file"));
    }
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(format!("/proc/self/fd/{}", entry.as_raw_fd()))
}

pub(crate) fn read_path(path: &Path, max: usize) -> Result<Option<(Vec<u8>, u32)>, DesktopError> {
    let parent = path.parent().ok_or_else(|| failed("missing parent"))?;
    match Directory::open(parent, false).map_err(failed)? {
        Some(dir) => dir
            .read(
                path.file_name().ok_or_else(|| failed("missing filename"))?,
                max,
            )
            .map_err(failed),
        None => Ok(None),
    }
}

/// Map every supported write to a recovery scope. Dedicated appearance/bar/
/// wallpaper namespaces also retain unknown leaves as evidence, never as writes.
fn domain_of(namespace: &str, key: &str) -> Option<RecoveryDomain> {
    use RecoveryDomain::*;
    Some(match namespace {
        n if n.starts_with("com.system76.CosmicTheme.") => Appearance,
        "com.system76.CosmicTk" => {
            if ["show_minimize", "show_maximize"].contains(&key) {
                Windows
            } else {
                Appearance
            }
        }
        "com.system76.CosmicComp" => match key {
            "workspaces" => Workspaces,
            "appearance_settings" => Appearance,
            _ => Windows,
        },
        n if n == super::panels::NAMESPACE || n.starts_with("com.system76.CosmicPanel.") => {
            DockPanel
        }
        super::wallpaper::NAMESPACE
        | super::wallpaper::modes::DARK
        | super::wallpaper::modes::LIGHT
        | "com.system76.CosmicSettings.Wallpaper" => Wallpaper,
        super::desktop_icons::NAMESPACE => DesktopIcons,
        super::shortcuts::NAMESPACE | super::shortcuts::PAUSED_NAMESPACE => Shortcuts,
        super::window_rules::NAMESPACE => WindowRules,
        dock_islands_core::NAMESPACE => Islands,
        dock_islands_core::material::NAMESPACE => Materials,
        _ => return None,
    })
}
fn whole_namespace(namespace: &str) -> bool {
    namespace.starts_with("com.system76.CosmicTheme.")
        || namespace == super::panels::NAMESPACE
        || namespace.starts_with("com.system76.CosmicPanel.")
        || [
            super::wallpaper::NAMESPACE,
            super::wallpaper::modes::DARK,
            super::wallpaper::modes::LIGHT,
            super::shortcuts::NAMESPACE,
            super::shortcuts::PAUSED_NAMESPACE,
            super::window_rules::NAMESPACE,
            dock_islands_core::NAMESPACE,
            dock_islands_core::material::NAMESPACE,
        ]
        .contains(&namespace)
}
/// Local colors/geometry/layout only. FIELDS excludes account data, note text,
/// location, project roots, reporting, schedules and the complete config/journal.
pub fn app_pointers() -> impl Iterator<Item = &'static str> {
    widget::FIELDS
        .iter()
        // Local geometry belongs to recovery, while display assignments remain
        // machine policy and are intentionally excluded from this baseline.
        .chain(
            widget::LOCAL_FIELDS
                .iter()
                .filter(|field| field.pointer.starts_with("/monitor_layouts/")),
        )
        .map(|field| field.pointer)
        .chain([
            "/appearance/match_wallpaper",
            "/appearance/match_cosmic_colors",
            "/appearance/wallpaper_color_stamp",
            "/placement/columns",
            "/widgets_visible",
        ])
}
fn app_domain(pointer: &str) -> RecoveryDomain {
    match widget::field(pointer).map(|field| field.group) {
        Some(widget::WidgetGroup::Layout) => RecoveryDomain::WidgetLayout,
        _ => RecoveryDomain::WidgetAppearance,
    }
}

fn capture(store: &DesktopStore, requested: BaselineKind) -> Result<BaselineInfo, DesktopError> {
    let app_bytes = store
        .app_config()
        .map(|path| read_path(path, MAX_MANIFEST_BYTES))
        .transpose()?
        .flatten();
    let mut app: serde_json::Value = match &app_bytes {
        Some((bytes, _)) => serde_json::from_slice(bytes).map_err(failed)?,
        None => serde_json::json!({}),
    };
    if !app.is_object() {
        return Err(failed("app settings are not a JSON object"));
    }
    if let Some(path) = store.app_config()
        && let Some(text) = crate::design::storage::read(path).map_err(failed)?
    {
        app = crate::design::Document::parse(text.as_bytes())
            .and_then(|d| d.project(&app))
            .map_err(failed)?;
    }
    // Absence of the new baseline is never evidence of a fresh installation.
    // Only absent app configuration AND absent prior mutation/setup evidence
    // establish a new installation; ambiguity is dated current state.
    let legacy = [
        "desktop-undo.json",
        "desktop-journal.json",
        "cosmic-colors-original.json",
        "profiles",
    ]
    .iter()
    .any(|name| store.state_dir().join(name).symlink_metadata().is_ok());
    let legacy_config = store
        .app_config()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(|root| root.join(crate::paths::LEGACY))
        .is_some_and(|p| p.symlink_metadata().is_ok());
    let kind = if requested == BaselineKind::OriginalDesktop
        && store.app_config().is_some()
        && app_bytes.is_none()
        && !legacy
        && !legacy_config
    {
        BaselineKind::OriginalDesktop
    } else {
        BaselineKind::CurrentDesktop
    };
    let captured_at = chrono::Utc::now().to_rfc3339();
    let mut domains: BTreeMap<_, _> = RecoveryDomain::ALL
        .into_iter()
        .map(|domain| {
            (
                domain,
                DomainSnapshot {
                    domain,
                    kind,
                    captured_at: captured_at.clone(),
                    inventory: vec![],
                    native: vec![],
                    app: vec![],
                    motion: None,
                    sha256: String::new(),
                },
            )
        })
        .collect();
    for pointer in app_pointers() {
        let value = app.pointer(pointer);
        domains
            .get_mut(&app_domain(pointer))
            .unwrap()
            .app
            .push(AppLeaf {
                pointer: pointer.into(),
                present: value.is_some(),
                value: value.cloned().unwrap_or_default(),
            });
    }
    let root = Directory::open(store.user_root(), false).map_err(failed)?;
    let mut scopes: BTreeMap<(String, u64), BTreeSet<OsString>> = BTreeMap::new();
    for rule in super::rules() {
        let namespaces = match rule.namespace {
            Namespace::Exact(name) => vec![name.to_string()],
            Namespace::Entry(prefix) => {
                let mut names = vec![format!("{prefix}.Dock"), format!("{prefix}.Panel")];
                if let Some(root) = &root {
                    for name in root.names().map_err(failed)? {
                        if let Some(name) = name
                            .to_str()
                            .filter(|name| name.starts_with(&format!("{prefix}.")))
                        {
                            ConfigKey::new(name, 1, "entry").map_err(failed)?;
                            names.push(name.to_string());
                        }
                    }
                }
                names
            }
        };
        for namespace in namespaces {
            let names = scopes.entry((namespace, rule.version)).or_default();
            if !rule.key.contains('*') {
                names.insert(OsString::from(rule.key));
            }
        }
    }
    scopes
        .entry((super::panels::NAMESPACE.into(), 1))
        .or_default()
        .insert("entries".into());
    scopes
        .entry(("com.system76.CosmicSettings.Wallpaper".into(), 1))
        .or_default()
        .insert("custom-images".into());
    // Unknown schema versions in scoped namespaces remain exportable evidence.
    let namespaces: BTreeSet<_> = scopes.keys().map(|(ns, _)| ns.clone()).collect();
    for namespace in namespaces {
        if !whole_namespace(&namespace) {
            continue;
        }
        if let Some(dir) = root
            .as_ref()
            .map(|root| root.child(OsStr::new(&namespace), false))
            .transpose()
            .map_err(failed)?
            .flatten()
        {
            for name in dir.names().map_err(failed)? {
                if let Some(version) = name
                    .to_str()
                    .and_then(|n| n.strip_prefix('v'))
                    .and_then(|n| n.parse::<u64>().ok())
                    .filter(|n| *n > 0 && *n <= u32::MAX as u64)
                {
                    scopes.entry((namespace.clone(), version)).or_default();
                }
            }
        }
    }
    let mut total: usize = domains
        .values()
        .flat_map(|d| &d.app)
        .filter(|leaf| leaf.present)
        .map(|leaf| {
            serde_json::to_vec(&leaf.value)
                .map(|v| v.len())
                .map_err(failed)
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .sum();
    if total > MAX_TOTAL_BYTES {
        return Err(failed("desktop snapshot exceeds 16 MiB"));
    }
    let mut files = 0usize;
    for ((namespace, version), mut names) in scopes {
        let dir = match &root {
            Some(root) => root
                .child(OsStr::new(&namespace), false)
                .map_err(failed)?
                .map(|ns| ns.child(OsStr::new(&format!("v{version}")), false))
                .transpose()
                .map_err(failed)?
                .flatten(),
            None => None,
        };
        let inventory = if whole_namespace(&namespace) {
            dir.as_ref()
                .map(Directory::names)
                .transpose()
                .map_err(failed)?
                .unwrap_or_default()
        } else {
            // Only these keys in mixed/content-bearing namespaces are in scope.
            names
                .iter()
                .filter(|name| {
                    dir.as_ref()
                        .is_some_and(|d| d.read(name, MAX_FILE_BYTES).ok().flatten().is_some())
                })
                .cloned()
                .collect()
        };
        names.extend(inventory.iter().cloned());
        let mut inventory_domains: BTreeSet<_> = names
            .iter()
            .filter_map(|key| domain_of(&namespace, key.to_str().unwrap_or("")))
            .collect();
        if inventory_domains.is_empty() {
            inventory_domains.extend(domain_of(&namespace, ""));
        }
        for domain in inventory_domains {
            domains
                .get_mut(&domain)
                .unwrap()
                .inventory
                .push(NativeInventory {
                    namespace: namespace.clone(),
                    version,
                    present: dir.is_some(),
                    names: inventory
                        .iter()
                        .filter(|name| {
                            domain_of(&namespace, name.to_str().unwrap_or("")) == Some(domain)
                        })
                        .map(|name| STANDARD.encode(name.as_bytes()))
                        .collect(),
                });
        }
        for name in names {
            files += 1;
            if files > MAX_FILES {
                return Err(failed("too many settings files"));
            }
            let captured = dir
                .as_ref()
                .map(|dir| dir.read(&name, MAX_FILE_BYTES))
                .transpose()
                .map_err(failed)?
                .flatten();
            let domain = domain_of(&namespace, name.to_str().unwrap_or(""))
                .ok_or_else(|| failed("unknown capture domain"))?;
            let (bytes, sha256, mode) = match captured {
                Some((bytes, mode)) => {
                    total += bytes.len();
                    if total > MAX_TOTAL_BYTES {
                        return Err(failed("desktop snapshot exceeds 16 MiB"));
                    }
                    (
                        Some(STANDARD.encode(&bytes)),
                        Some(digest(&bytes)),
                        Some(mode),
                    )
                }
                None => (None, None, None),
            };
            domains.get_mut(&domain).unwrap().native.push(NativeRecord {
                namespace: namespace.clone(),
                version,
                name: STANDARD.encode(name.as_bytes()),
                bytes,
                sha256,
                mode,
            });
        }
    }
    for domain in domains.values_mut() {
        domain.sha256 = domain_digest(domain)?;
    }
    Ok(BaselineInfo {
        format: FORMAT.into(),
        version: FORMAT_VERSION,
        kind,
        captured_at,
        app_config_present: app_bytes.is_some(),
        supported_domains: domains.keys().copied().collect(),
        domains: domains.into_values().collect(),
    })
}

fn validate(info: &BaselineInfo) -> Result<(), DesktopError> {
    if info.format != FORMAT
        || info.version != FORMAT_VERSION
        || chrono::DateTime::parse_from_rfc3339(&info.captured_at).is_err()
    {
        return Err(failed(
            "the existing baseline is damaged or from another version; it was preserved",
        ));
    }
    if info.supported_domains.iter().collect::<BTreeSet<_>>().len() != info.supported_domains.len()
        || info.supported_domains.len() != info.domains.len()
        || info
            .supported_domains
            .iter()
            .any(|scope| !info.domains.iter().any(|d| d.domain == *scope))
    {
        return Err(failed(
            "baseline inventory is incomplete; existing evidence was preserved",
        ));
    }
    // The display label/date must agree with checksummed original evidence.
    // A changed header cannot relabel a dated-current snapshot as Original.
    if !info
        .domains
        .iter()
        .any(|domain| domain.kind == info.kind && domain.captured_at == info.captured_at)
        || info.domains.iter().any(|domain| {
            domain.kind == BaselineKind::OriginalDesktop
                && (info.kind != BaselineKind::OriginalDesktop
                    || domain.captured_at != info.captured_at)
        })
    {
        return Err(failed("baseline label or date disagrees with its evidence"));
    }
    let mut seen = BTreeSet::new();
    let mut total = 0;
    let mut files = 0;
    for domain in &info.domains {
        if !seen.insert(domain.domain)
            || domain.sha256 != domain_digest(domain)?
            || chrono::DateTime::parse_from_rfc3339(&domain.captured_at).is_err()
        {
            return Err(failed("baseline domain checksum or date is invalid"));
        }
        if let Some(motion) = &domain.motion {
            let bytes = motion
                .bytes
                .as_ref()
                .map(|b| STANDARD.decode(b))
                .transpose()
                .map_err(failed)?;
            if domain.domain != RecoveryDomain::AppAnimations
                || !["dconf", "fixture"].contains(&motion.backend.as_str())
                || bytes.as_ref().map(|b| digest(b)) != motion.sha256
                || bytes.as_ref().is_some_and(|b| b.len() > MAX_FILE_BYTES)
            {
                return Err(failed("invalid animation baseline"));
            }
            total += bytes.as_ref().map_or(0, Vec::len);
            if total > MAX_TOTAL_BYTES {
                return Err(failed("baseline exceeds capture limits"));
            }
        } else if domain.domain == RecoveryDomain::AppAnimations {
            return Err(failed("missing animation baseline"));
        }
        let mut inventories = BTreeSet::new();
        for inventory in &domain.inventory {
            ConfigKey::new(&inventory.namespace, inventory.version, "checked").map_err(failed)?;
            if !inventories.insert((&inventory.namespace, inventory.version))
                || (!inventory.present && !inventory.names.is_empty())
            {
                return Err(failed("invalid baseline directory inventory"));
            }
            let mut names = BTreeSet::new();
            for name in &inventory.names {
                let decoded = STANDARD.decode(name).map_err(failed)?;
                component(&decoded).map_err(failed)?;
                if !names.insert(name)
                    || !domain.native.iter().any(|record| {
                        record.namespace == inventory.namespace
                            && record.version == inventory.version
                            && record.name == *name
                            && record.bytes.is_some()
                    })
                {
                    return Err(failed("baseline inventory does not match its files"));
                }
            }
        }
        let mut paths = BTreeSet::new();
        for record in &domain.native {
            let name = record.key_name()?;
            ConfigKey::new(&record.namespace, record.version, "checked").map_err(failed)?;
            if domain_of(&record.namespace, name.to_str().unwrap_or("")) != Some(domain.domain)
                || !paths.insert((&record.namespace, record.version, record.name.clone()))
            {
                return Err(failed("baseline contains an invalid address"));
            }
            let bytes = record.contents()?;
            if bytes.as_ref().map(|b| digest(b)) != record.sha256
                || bytes.is_some() != record.mode.is_some()
                || record.mode.is_some_and(|mode| mode > 0o777)
            {
                return Err(failed("baseline file checksum is invalid"));
            }
            let size = bytes.as_ref().map_or(0, Vec::len);
            files += 1;
            total += size;
            if size > MAX_FILE_BYTES || total > MAX_TOTAL_BYTES || files > MAX_FILES {
                return Err(failed("baseline exceeds capture limits"));
            }
        }
        let allowed: BTreeSet<_> = app_pointers().collect();
        let mut pointers = BTreeSet::new();
        for leaf in &domain.app {
            if leaf.present {
                total += serde_json::to_vec(&leaf.value).map_err(failed)?.len();
            }
            if total > MAX_TOTAL_BYTES {
                return Err(failed("baseline exceeds capture limits"));
            }
            if !allowed.contains(leaf.pointer.as_str())
                || app_domain(&leaf.pointer) != domain.domain
                || !pointers.insert(&leaf.pointer)
                || (!leaf.present && !leaf.value.is_null())
            {
                return Err(failed("baseline contains an invalid app leaf"));
            }
        }
    }
    Ok(())
}

/// Read and verify without creating directories or following symlinks.
pub fn load_baseline(store: &DesktopStore) -> Result<Option<BaselineInfo>, DesktopError> {
    let path = store.state_dir().join(DIRECTORY).join("original");
    let Some(dir) = Directory::open(&path, false).map_err(failed)? else {
        return Ok(None);
    };
    let (bytes, _) = dir
        .read(OsStr::new("manifest.json"), MAX_MANIFEST_BYTES)
        .map_err(failed)?
        .ok_or_else(|| failed("the existing baseline is incomplete; it was preserved"))?;
    let info = serde_json::from_slice(&bytes).map_err(failed)?;
    validate(&info)?;
    Ok(Some(info))
}

/// Public capture boundary; the store owns the shared locks. Never call this
/// while holding either lock. A requested Original is downgraded on legacy or
/// ambiguous installs. Existing corruption is an error, never a recapture.
pub fn ensure_baseline(
    store: &DesktopStore,
    kind: BaselineKind,
) -> Result<BaselineInfo, DesktopError> {
    store.capture_baseline(kind)
}

pub(super) fn ensure_locked(
    store: &DesktopStore,
    kind: BaselineKind,
) -> Result<BaselineInfo, DesktopError> {
    let existing = load_baseline(store)?;
    if let Some(info) = &existing {
        if RecoveryDomain::ALL
            .iter()
            .all(|domain| info.domains.iter().any(|d| d.domain == *domain))
        {
            return Ok(info.clone());
        }
    }
    let mut next = capture(
        store,
        if existing.is_some() {
            BaselineKind::CurrentDesktop
        } else {
            kind
        },
    )?;
    if let Some(mut info) = existing {
        for domain in next.domains {
            if !info.domains.iter().any(|old| old.domain == domain.domain) {
                info.supported_domains.push(domain.domain);
                info.domains.push(domain);
            }
        }
        next = info;
    }
    promote(store, next)
}

pub(super) fn ensure_motion_locked(
    store: &DesktopStore,
    backend: &str,
    capture: impl FnOnce() -> Result<Option<Vec<u8>>, String>,
) -> Result<(), DesktopError> {
    let mut info = ensure_locked(store, BaselineKind::OriginalDesktop)?;
    if let Some(domain) = info
        .domains
        .iter()
        .find(|d| d.domain == RecoveryDomain::AppAnimations)
    {
        if domain
            .motion
            .as_ref()
            .is_none_or(|motion| motion.backend != backend)
        {
            return Err(failed("animation backend changed since capture"));
        }
        return Ok(());
    }
    let bytes = capture().map_err(failed)?;
    let mut domain = DomainSnapshot {
        domain: RecoveryDomain::AppAnimations,
        kind: BaselineKind::CurrentDesktop,
        captured_at: chrono::Utc::now().to_rfc3339(),
        inventory: vec![],
        native: vec![],
        app: vec![],
        motion: Some(MotionRecord {
            backend: backend.into(),
            bytes: bytes.as_ref().map(|b| STANDARD.encode(b)),
            sha256: bytes.as_ref().map(|b| digest(b)),
        }),
        sha256: String::new(),
    };
    domain.sha256 = domain_digest(&domain)?;
    info.supported_domains.push(domain.domain);
    info.domains.push(domain);
    promote(store, info)?;
    Ok(())
}

pub(crate) fn read_motion_fixture(path: &Path) -> Result<Option<Vec<u8>>, String> {
    read_path(path, MAX_FILE_BYTES)
        .map(|value| value.map(|(bytes, _)| bytes))
        .map_err(|e| e.to_string())
}
/// Fixture backend writes use the same descriptor confinement as capture.
pub(crate) fn write_motion_fixture(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("missing animation directory")?;
    let dir = Directory::open(parent, false)
        .map_err(|e| e.to_string())?
        .ok_or("missing animation directory")?;
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or("invalid animation key")?;
    let pending = format!(".motion-{}", getrandom::u64().map_err(|e| e.to_string())?);
    dir.write_new(&pending, bytes)
        .and_then(|_| dir.rename(&pending, name, true))
        .map_err(|e| e.to_string())
}

fn promote(store: &DesktopStore, next: BaselineInfo) -> Result<BaselineInfo, DesktopError> {
    validate(&next)?;
    let bytes = serde_json::to_vec(&next).map_err(failed)?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(failed("baseline manifest is too large"));
    }
    let dir = Directory::open(&store.state_dir().join(DIRECTORY), true)
        .map_err(failed)?
        .unwrap();
    if let Some(original) = dir.child(OsStr::new("original"), false).map_err(failed)? {
        // Append new domain dates only; existing domain bytes and digests stay.
        let name = format!("pending-{}.json", getrandom::u64().map_err(failed)?);
        original.write_new(&name, &bytes).map_err(failed)?;
        original
            .rename(&name, "manifest.json", true)
            .map_err(failed)?;
    } else {
        let name = format!("pending-{}", getrandom::u64().map_err(failed)?);
        let staging = dir.child(OsStr::new(&name), true).map_err(failed)?.unwrap();
        staging.write_new("manifest.json", &bytes).map_err(failed)?;
        sync(&staging.0).map_err(failed)?;
        dir.rename(&name, "original", false).map_err(failed)?;
    }
    Ok(next)
}

#[cfg(test)]
mod tests;

mod restore;
pub(crate) use restore::{
    NativeBatch, read_native, record_created_all, write_native, write_scoped,
};
pub use restore::{RestorePlan, apply_restore, export_baseline, plan_restore, write_export};
pub(super) use restore::{save_pre_restore, verify_plan};

#[cfg(test)]
mod auditfix_tests {
    use super::*;
    use crate::desktop::DesktopPatch;
    use crate::desktop::store::{Owner, WidgetPatch, tests::Fixture};
    use serde_json::json;

    #[test]
    fn auditfix_tx01_restore_and_undo_include_monitor_geometry_without_assignments() {
        let fixture = Fixture::new("auditfix-monitor-restore");
        let path = fixture.0.join("config.json");
        let geometry = json!({"DP-1":{"position":[0.25,0.35],"size":[400,180]}});
        let baseline = json!({"enabled_widgets":["clock"],"monitor_layouts":{"clock":geometry},"widget_monitors":{"clock":{"display":"DP-1"}}});
        fs::write(&path, baseline.to_string()).unwrap();
        let store = fixture.store().with_app_config(path.clone());
        ensure_baseline(&store, BaselineKind::CurrentDesktop).unwrap();
        let changed =
            json!({"DP-1":{"position":[0.75,0.65],"size":[600,240]},"DP-2":{"position":[0.5,0.5]}});
        let mut patch = WidgetPatch::new();
        patch
            .set(
                "/monitor_layouts/clock",
                Some(geometry.clone()),
                Some(changed.clone()),
            )
            .unwrap();
        patch
            .set(
                "/widget_monitors/clock/display",
                Some(json!("DP-1")),
                Some(json!("DP-2")),
            )
            .unwrap();
        store
            .apply_owned(
                &DesktopPatch::new(),
                Some(&patch),
                Owner::Preferences,
                "Arrange",
            )
            .unwrap();
        let exported: serde_json::Value =
            serde_json::from_str(&export_baseline(&store).unwrap()).unwrap();
        assert!(exported.to_string().contains("/monitor_layouts/clock"));
        assert!(
            !exported
                .to_string()
                .contains("/widget_monitors/clock/display")
        );
        let plan = plan_restore(&store, &[RecoveryDomain::WidgetLayout]).unwrap();
        let restore = apply_restore(&store, &plan).unwrap();
        let raw: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(raw["monitor_layouts"]["clock"], geometry);
        assert_eq!(raw["widget_monitors"]["clock"]["display"], "DP-2");
        let config: crate::config::Config = serde_json::from_value(raw).unwrap();
        let effective =
            config.on_displays(&[("clock".into(), "DP-1".into())].into_iter().collect());
        assert_eq!(effective.widget_positions["clock"], [0.25, 0.35]);
        assert_eq!(effective.widget_sizes["clock"], [400., 180.]);
        store.undo_latest(&restore.id).unwrap();
        let raw: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(raw["monitor_layouts"]["clock"], changed);
    }

    #[test]
    fn auditfix_tx01_legacy_baseline_does_not_invent_missing_monitor_originals() {
        let fixture = Fixture::new("auditfix-monitor-legacy");
        let path = fixture.0.join("config.json");
        let raw =
            json!({"monitor_layouts":{"clock":{"DP-1":{"position":[0.75,0.65],"size":[600,240]}}}});
        fs::write(&path, raw.to_string()).unwrap();
        let store = fixture.store().with_app_config(path.clone());
        let mut baseline = ensure_baseline(&store, BaselineKind::CurrentDesktop).unwrap();
        for domain in &mut baseline.domains {
            domain
                .app
                .retain(|leaf| !leaf.pointer.starts_with("/monitor_layouts/"));
            domain.sha256 = domain_digest(domain).unwrap();
        }
        let manifest = store
            .state_dir()
            .join("desktop-recovery/original/manifest.json");
        fs::write(&manifest, serde_json::to_vec(&baseline).unwrap()).unwrap();
        let legacy = fs::read(&manifest).unwrap();
        let plan = plan_restore(&store, &[RecoveryDomain::WidgetLayout]).unwrap();
        assert!(
            !plan
                .widgets
                .iter()
                .any(|(pointer, _)| pointer.starts_with("/monitor_layouts/"))
        );
        apply_restore(&store, &plan).unwrap();
        let after: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(after["monitor_layouts"], raw["monitor_layouts"]);
        assert_eq!(fs::read(manifest).unwrap(), legacy);
    }
}
