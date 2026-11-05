//! Desktop icons: size, spacing and what the desktop shows.
//!
//! Source (pinned): cosmic-files 0f15f44 `src/config.rs` `DesktopConfig`:
//! ONE `com.system76.CosmicFiles/v1/desktop` struct with struct-level
//! `#[serde(default)]`. An edit rewrites the whole struct from the decoded
//! current value, so members it does not change keep their values; a value
//! with members this version does not recognize, or sizes outside 1–500
//! (cosmic-files clamps at 5×), is kept and never rewritten. cosmic-files
//! stores an outside change at once but redraws the desktop with it only at
//! its next desktop refresh.
use super::appearance::{Blocked, Plan, decode_exact};
use super::panels::Setting;
use super::store::{
    ConfigKey, DesktopError, DesktopPatch, DesktopStore, KeyRule, Namespace, RawValue, Snapshot,
};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU16;

pub const NAMESPACE: &str = "com.system76.CosmicFiles";
/// The slider range and step (percent), as in cosmic-files' own options.
pub const SCALE_MIN: u16 = 50;
pub const SCALE_MAX: u16 = 500;
pub const SCALE_STEP: u16 = 25;
/// cosmic-files redraws the desktop with an outside change only at its
/// next desktop refresh (spec §4).
pub const REFRESH_NOTE: &str =
    "Icons update the next time the desktop refreshes, for example after you sign in again.";

/// `DesktopConfig` at 0f15f44, member for member, with its defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopIcons {
    pub grid_spacing: NonZeroU16,
    pub icon_size: NonZeroU16,
    pub show_content: bool,
    pub show_mounted_drives: bool,
    pub show_trash: bool,
}

impl Default for DesktopIcons {
    fn default() -> Self {
        Self {
            grid_spacing: NonZeroU16::new(100).expect("non-zero"),
            icon_size: NonZeroU16::new(100).expect("non-zero"),
            show_content: true,
            show_mounted_drives: false,
            show_trash: false,
        }
    }
}

/// What the desktop shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    /// The Desktop folder's contents.
    Folder,
    Drives,
    Trash,
}

/// Member edits; `None` keeps the member's current value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IconEdits {
    pub icon_size: Option<NonZeroU16>,
    pub grid_spacing: Option<NonZeroU16>,
    pub show_content: Option<bool>,
    pub show_mounted_drives: Option<bool>,
    pub show_trash: Option<bool>,
}

impl IconEdits {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
    /// Sets the edited members of `config`.
    pub fn apply_to(&self, config: &mut DesktopIcons) {
        if let Some(size) = self.icon_size {
            config.icon_size = size;
        }
        if let Some(spacing) = self.grid_spacing {
            config.grid_spacing = spacing;
        }
        if let Some(show) = self.show_content {
            config.show_content = show;
        }
        if let Some(show) = self.show_mounted_drives {
            config.show_mounted_drives = show;
        }
        if let Some(show) = self.show_trash {
            config.show_trash = show;
        }
    }
    fn trim(&mut self, base: &DesktopIcons) {
        if self.icon_size == Some(base.icon_size) {
            self.icon_size = None;
        }
        if self.grid_spacing == Some(base.grid_spacing) {
            self.grid_spacing = None;
        }
        if self.show_content == Some(base.show_content) {
            self.show_content = None;
        }
        if self.show_mounted_drives == Some(base.show_mounted_drives) {
            self.show_mounted_drives = None;
        }
        if self.show_trash == Some(base.show_trash) {
            self.show_trash = None;
        }
    }
}

/// Whether `cosmic-files-applet` (which draws the desktop) is an executable
/// file in one of the `PATH` directories.
pub fn files_installed(path: Option<&std::ffi::OsStr>) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.into_iter()
        .flat_map(std::env::split_paths)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join("cosmic-files-applet"))
        .any(|file| {
            std::fs::metadata(file)
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
}

pub fn key() -> ConfigKey {
    ConfigKey::new(NAMESPACE, 1, "desktop").expect("desktop icons key is valid")
}

/// Short user-facing name of the desktop icons key, for notices.
pub fn key_label(key: &ConfigKey) -> Option<String> {
    (*key == self::key()).then(|| "desktop icon settings".to_string())
}

fn read(raw: &RawValue) -> Result<DesktopIcons, String> {
    let config = decode_exact::<DesktopIcons>(raw)?;
    let max = NonZeroU16::new(SCALE_MAX).expect("non-zero");
    if config.icon_size > max || config.grid_spacing > max {
        return Err("sizes above 500 % are not supported".into());
    }
    Ok(config)
}

fn check_icons(value: &RawValue) -> Result<(), String> {
    read(value).map(drop)
}

/// The desktop key; no other cosmic-files key is touched.
pub static RULES: [KeyRule; 1] = [KeyRule {
    namespace: Namespace::Exact(NAMESPACE),
    version: 1,
    key: "desktop",
    validate: check_icons,
}];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesktopIconsDraft {
    base: Snapshot,
    edits: IconEdits,
    conflicts: Vec<ConfigKey>,
}

impl DesktopIconsDraft {
    pub fn keys() -> Vec<ConfigKey> {
        vec![key()]
    }
    /// Reads only; creates no files or directories.
    pub fn load(store: &DesktopStore) -> Result<Self, DesktopError> {
        Ok(Self::from_snapshot(store.load(&Self::keys())?))
    }
    pub fn from_snapshot(base: Snapshot) -> Self {
        Self {
            base,
            edits: IconEdits::default(),
            conflicts: Vec::new(),
        }
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
    pub fn edits(&self) -> IconEdits {
        self.edits
    }
    pub fn conflicts(&self) -> &[ConfigKey] {
        &self.conflicts
    }
    pub fn discard(&mut self) {
        self.edits = IconEdits::default();
        self.conflicts.clear();
    }

    /// The stored value decoded exactly (absent members take upstream's
    /// defaults), or the defaults when no layer defines the key.
    pub fn base_config(&self) -> Setting<DesktopIcons> {
        match self.base.effective(&key()) {
            Some(raw) => match read(raw) {
                Ok(config) => Setting::Known(config),
                Err(_) => Setting::Unsupported(raw.as_str().trim().to_string()),
            },
            None => Setting::Known(DesktopIcons::default()),
        }
    }
    /// The struct after the draft's edits.
    pub fn config(&self) -> Setting<DesktopIcons> {
        match self.base_config() {
            Setting::Known(mut config) => {
                self.edits.apply_to(&mut config);
                Setting::Known(config)
            }
            unsupported => unsupported,
        }
    }
    fn edit(&mut self, change: impl FnOnce(&mut IconEdits)) {
        change(&mut self.edits);
        if let Setting::Known(base) = self.base_config() {
            self.edits.trim(&base);
        }
    }
    fn scale(percent: u16) -> Option<NonZeroU16> {
        (SCALE_MIN..=SCALE_MAX)
            .contains(&percent)
            .then(|| NonZeroU16::new(percent))
            .flatten()
    }
    /// Percent in `SCALE_MIN..=SCALE_MAX`; other values are ignored.
    pub fn set_icon_size(&mut self, percent: u16) {
        if let Some(size) = Self::scale(percent) {
            self.edit(|edits| edits.icon_size = Some(size));
        }
    }
    /// Percent in `SCALE_MIN..=SCALE_MAX`; other values are ignored.
    pub fn set_spacing(&mut self, percent: u16) {
        if let Some(spacing) = Self::scale(percent) {
            self.edit(|edits| edits.grid_spacing = Some(spacing));
        }
    }
    /// Any valid native values (sizes 1–500 %, also between the slider's
    /// steps), as a profile sets them; an out-of-range size is ignored.
    pub fn set_members(&mut self, edits: IconEdits) {
        let max = NonZeroU16::new(SCALE_MAX).expect("non-zero");
        let valid = |size: Option<NonZeroU16>| size.filter(|size| *size <= max);
        self.edit(|current| {
            current.icon_size = valid(edits.icon_size).or(current.icon_size);
            current.grid_spacing = valid(edits.grid_spacing).or(current.grid_spacing);
            current.show_content = edits.show_content.or(current.show_content);
            current.show_mounted_drives = edits.show_mounted_drives.or(current.show_mounted_drives);
            current.show_trash = edits.show_trash.or(current.show_trash);
        });
    }
    pub fn set_show(&mut self, what: Show, show: bool) {
        self.edit(|edits| match what {
            Show::Folder => edits.show_content = Some(show),
            Show::Drives => edits.show_mounted_drives = Some(show),
            Show::Trash => edits.show_trash = Some(show),
        });
    }

    /// The whole struct with the edited members, or a reason it cannot be
    /// rewritten faithfully.
    pub fn plan(&self) -> Plan {
        let mut patch = DesktopPatch::new();
        let mut blocked = Vec::new();
        if self.dirty() {
            let key = key();
            match self.config() {
                Setting::Known(config) => match RawValue::encode(&config) {
                    Ok(value) => patch.set(key.clone(), self.base.user(&key).cloned(), Some(value)),
                    Err(error) => blocked.push(Blocked {
                        key,
                        reason: error.to_string(),
                    }),
                },
                Setting::Unsupported(_) => blocked.push(Blocked {
                    key,
                    reason: "contains settings this version does not recognize".into(),
                }),
            }
        }
        Plan { patch, blocked }
    }

    /// The edits read the whole struct, also from the system default.
    pub fn stale(&self, fresh: &Snapshot) -> Vec<ConfigKey> {
        let key = key();
        let changed = self.base.user(&key) != fresh.user(&key)
            || self.base.effective(&key) != fresh.effective(&key);
        if self.dirty() && changed {
            vec![key]
        } else {
            Vec::new()
        }
    }

    /// Adopts outside changes while nothing is edited; with edits the key
    /// keeps its base so Apply detects the change, listed as a conflict.
    pub fn refresh(&mut self, snapshot: Snapshot) {
        let key = key();
        if !self.dirty() {
            self.base = snapshot;
            self.conflicts.clear();
            return;
        }
        let old = self.base.entry(&key).cloned().unwrap_or_default();
        let fresh = snapshot.entry(&key).cloned().unwrap_or_default();
        self.conflicts.clear();
        if fresh.user != old.user || fresh.effective() != old.effective() {
            self.conflicts.push(key.clone());
        }
        let mut base = snapshot;
        base.insert(key, old);
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
    /// Drops the edits (they all depend on the one struct) and adopts the
    /// current value.
    pub fn resolve_conflicts(&mut self, snapshot: Snapshot, keys: &[ConfigKey]) {
        if keys.contains(&key()) || !self.conflicts.is_empty() {
            *self = Self::from_snapshot(snapshot);
        } else {
            self.refresh(snapshot);
        }
    }
}

#[cfg(test)]
mod tests;
