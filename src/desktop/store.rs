//! Per-key COSMIC configuration adapter.
//!
//! COSMIC stores every setting as one RON file:
//! `<root>/cosmic/<namespace>/v<version>/<key>`. User files override the
//! system defaults found under `$XDG_DATA_HOME` and `$XDG_DATA_DIRS`.
//! The store reads both layers without writing anything, and applies a
//! `DesktopPatch` of touched keys only:
//!
//! 1. reject read-only stores, non-whitelisted keys and invalid values;
//! 2. lock against our own writers and refuse while a journal is pending;
//! 3. re-read every touched key and reject the whole patch on any external
//!    change since the draft began;
//! 4. persist a journal of before/after values, write each key atomically,
//!    and roll back touched keys on failure (in reverse write order);
//! 5. remove the journal and return an `AppliedChange` for Undo.
//!
//! Untouched keys, unknown files and other namespaces are never rewritten.
//!
//! Keys are journaled and written in `desktop::write_rank` order, then key
//! order. Only cosmic-bg's keys rank (`output.*`, then `all`, then
//! `backgrounds`, then `same-on-all`, so referenced entries exist before the
//! list and mode events); every other key ranks 0 and keeps key order. Undo
//! and recovery replay the journal's order. Rules name exact keys; the one
//! wildcard is cosmic-bg's `output.*`, limited to valid display names.
//!
//! An owned transaction (`apply_owned`) may also carry allowlisted members
//! of the Halcyon widget configuration (`widget`). It takes the shared
//! `config.json.lock` before `desktop.lock`, journals both parts before the
//! first write, reverses both conditionally on failure, and records the
//! persistent latest change (`undo`) before its journal is removed.
//!
//! That record is a participant too: an owned transaction's journal
//! (version 3) keeps the record's exact text as the transaction found it,
//! so recovery can put a replaced or cleared record back. Plain `apply` and
//! `undo` never touch the record and still write version 1.
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub mod activity;
mod design;
mod history;
pub use design::DesignWrite;
pub mod message;
pub use message::UserMessage;
mod ownership;
pub use history::UndoReview;
mod undo;
pub mod widget;

use crate::config;
pub use undo::{LatestChange, Owner, Source};
pub use widget::{WidgetChange, WidgetFieldChange, WidgetPatch};

/// Explicit isolated root for previews and tests. When set it always wins and
/// the store never falls back to the real desktop configuration.
pub const FIXTURE_ENV: &str = "DESKTOP_CARDS_DESKTOP_ROOT";
const JOURNAL: &str = "desktop-journal.json";
const LOCK: &str = "desktop.lock";
const JOURNAL_FORMAT: &str = "desktop-cards-journal";
/// The persistent latest change, for Undo after a restart.
const UNDO: &str = "desktop-undo.json";
/// Reserved for the profile library directory.
pub const PROFILES_DIR: &str = "profiles";
const MAX_VALUE_BYTES: usize = 256 * 1024;
const MAX_RECORD_BYTES: u64 = 8 * 1024 * 1024;
const PREVIEW_REASON: &str = "Preview mode · desktop changes are disabled";
static ACTION_REVISION: AtomicU64 = AtomicU64::new(0);
/// In-process observers refresh once after any successful transaction, including
/// an asynchronous completion that has no separate page-level Apply message.
pub fn revision() -> u64 {
    ACTION_REVISION.load(Ordering::Relaxed)
}
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

/// A COSMIC setting address: namespace, schema version and key.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "ConfigKeyRecord")]
pub struct ConfigKey {
    namespace: String,
    version: u64,
    key: String,
}

// Recovery records must pass the same path-component validation as live keys.
#[derive(Deserialize)]
struct ConfigKeyRecord {
    namespace: String,
    version: u64,
    key: String,
}
impl TryFrom<ConfigKeyRecord> for ConfigKey {
    type Error = DesktopError;
    fn try_from(value: ConfigKeyRecord) -> Result<Self, Self::Error> {
        Self::new(value.namespace, value.version, value.key)
    }
}

fn valid_name(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && !value.starts_with('.')
        && !value.ends_with('.')
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

impl ConfigKey {
    /// Validates each part as a single plain path component, so no key can
    /// address a file outside its `<namespace>/v<version>` directory.
    pub fn new(
        namespace: impl Into<String>,
        version: u64,
        key: impl Into<String>,
    ) -> Result<Self, DesktopError> {
        let namespace = namespace.into();
        let key = key.into();
        if !valid_name(&namespace, 128) {
            return Err(DesktopError::InvalidKey(format!(
                "Invalid settings namespace “{namespace}”"
            )));
        }
        if !valid_name(&key, 64) || key.starts_with("atomicwrite") {
            return Err(DesktopError::InvalidKey(format!(
                "Invalid setting name “{key}”"
            )));
        }
        if version == 0 || version > u32::MAX as u64 {
            return Err(DesktopError::InvalidKey(format!(
                "Invalid settings version {version}"
            )));
        }
        Ok(Self {
            namespace,
            version,
            key,
        })
    }
    pub fn namespace(&self) -> &str {
        &self.namespace
    }
    #[allow(dead_code)] // Adapter surface for later stages.
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    fn relative(&self) -> PathBuf {
        Path::new(&self.namespace)
            .join(format!("v{}", self.version))
            .join(&self.key)
    }
}

impl fmt::Display for ConfigKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/v{}/{}", self.namespace, self.version, self.key)
    }
}

/// One serialized RON value, exactly as COSMIC stores it in a key file.
/// Values compare by text, ignoring surrounding whitespace.
#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct RawValue(String);

impl PartialEq for RawValue {
    fn eq(&self, other: &Self) -> bool {
        self.0.trim() == other.0.trim()
    }
}

impl RawValue {
    /// Serializes with the same RON settings as `cosmic_config::ConfigSet`.
    pub fn encode<T: Serialize>(value: &T) -> Result<Self, DesktopError> {
        ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::new())
            .map(Self)
            .map_err(|error| DesktopError::InvalidValue {
                key: None,
                message: error.to_string(),
            })
    }
    /// Accepts only syntactically valid RON within the size limit.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn parse(text: impl Into<String>) -> Result<Self, DesktopError> {
        let value = Self(text.into());
        value
            .check()
            .map_err(|message| DesktopError::InvalidValue { key: None, message })?;
        Ok(value)
    }
    pub fn decode<T: DeserializeOwned>(&self) -> Result<T, String> {
        ron::from_str(&self.0).map_err(|error| error.to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub(crate) fn from_disk(text: String) -> Self {
        Self(text)
    }
    fn check(&self) -> Result<(), String> {
        if self.0.len() > MAX_VALUE_BYTES {
            return Err("Value is too large".into());
        }
        ron::from_str::<ron::Value>(&self.0)
            .map(|_| ())
            .map_err(|error| format!("Malformed value: {error}"))
    }
}

/// The namespace a rule covers: one exact name, or `<prefix>.<entry>` where
/// entry is a single identifier such as a panel name.
#[derive(Clone, Copy, Debug)]
pub enum Namespace {
    #[allow(dead_code)] // For single-namespace domains such as appearance.
    Exact(&'static str),
    Entry(&'static str),
}

/// A whitelisted key and the validator its desired values must pass.
#[derive(Debug)]
pub struct KeyRule {
    pub namespace: Namespace,
    pub version: u64,
    pub key: &'static str,
    pub validate: fn(&RawValue) -> Result<(), String>,
}

impl KeyRule {
    pub fn matches(&self, key: &ConfigKey) -> bool {
        let namespace = match self.namespace {
            Namespace::Exact(name) => key.namespace == name,
            Namespace::Entry(prefix) => key
                .namespace
                .strip_prefix(prefix)
                .and_then(|rest| rest.strip_prefix('.'))
                .is_some_and(|entry| {
                    !entry.is_empty()
                        && entry
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
                }),
        };
        let name = match self.key.strip_suffix('*') {
            // The only wildcard: cosmic-bg's per-display `output.<name>`
            // entries, for display names it can address.
            Some(prefix) => {
                matches!(
                    self.namespace,
                    Namespace::Exact(super::wallpaper::NAMESPACE)
                ) && self.version == 1
                    && prefix == super::wallpaper::OUTPUT_PREFIX
                    && key
                        .key
                        .strip_prefix(prefix)
                        .is_some_and(super::wallpaper::valid_output_name)
            }
            None => key.key == self.key,
        };
        namespace && key.version == self.version && name
    }
}

pub fn rule_for(key: &ConfigKey) -> Option<&'static KeyRule> {
    super::rules().find(|rule| rule.matches(key))
}

#[derive(Clone, Debug, PartialEq)]
pub enum DesktopError {
    /// Writes are disabled for this store.
    ReadOnly(String),
    InvalidKey(String),
    NotAllowed(ConfigKey),
    InvalidValue {
        key: Option<ConfigKey>,
        message: String,
    },
    /// These touched keys changed outside the draft; nothing was written.
    Conflict(Vec<ConfigKey>),
    /// Touched widget settings (and possibly keys) changed outside the
    /// draft; nothing was written.
    FieldConflict {
        keys: Vec<ConfigKey>,
        fields: Vec<String>,
    },
    /// Two parts of one change disagree about these keys; nothing was
    /// combined.
    Overlap(Vec<ConfigKey>),
    /// There is no matching latest change to undo (or its record is not
    /// usable); nothing was changed.
    UndoUnavailable(String),
    /// An interrupted change must be restored or kept first.
    RecoveryPending,
    Io(String),
    /// A write failed and every touched key was restored.
    RolledBack(String),
    /// A write failed and these keys (and widget settings) could not be
    /// restored; the journal stays.
    RollbackIncomplete {
        error: String,
        keys: Vec<ConfigKey>,
        fields: Vec<String>,
    },
}

impl fmt::Display for DesktopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |keys: &[ConfigKey]| {
            keys.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        };
        match self {
            Self::ReadOnly(reason) => write!(f, "{reason}"),
            Self::InvalidKey(message) => write!(f, "{message}"),
            Self::NotAllowed(key) => write!(f, "Halcyon does not edit {key}"),
            Self::InvalidValue {
                key: Some(key),
                message,
            } => write!(f, "Invalid value for {key}: {message}"),
            Self::InvalidValue { key: None, message } => write!(f, "Invalid value: {message}"),
            Self::Conflict(keys) => write!(
                f,
                "Changed elsewhere since you started editing: {}. Nothing was changed.",
                list(keys)
            ),
            Self::FieldConflict { keys, fields } => write!(
                f,
                "Changed elsewhere since you started editing: {}. Nothing was changed.",
                keys.iter()
                    .map(ToString::to_string)
                    .chain(fields.iter().cloned())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Overlap(keys) => write!(
                f,
                "Parts of this change disagree about {}. Nothing was changed.",
                list(keys)
            ),
            Self::UndoUnavailable(reason) => write!(f, "{reason}"),
            Self::RecoveryPending => write!(
                f,
                "An earlier change was interrupted. Restore or keep it before applying."
            ),
            Self::Io(message) => write!(f, "{message}"),
            Self::RolledBack(error) => {
                write!(
                    f,
                    "Could not apply changes; previous settings restored. {error}"
                )
            }
            Self::RollbackIncomplete {
                error,
                keys,
                fields,
            } => write!(
                f,
                "Could not apply changes ({error}). Not yet restored: {}. Use Restore to retry.",
                keys.iter()
                    .map(ToString::to_string)
                    .chain(fields.iter().cloned())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

impl std::error::Error for DesktopError {}

fn io_error(context: impl fmt::Display, error: io::Error) -> DesktopError {
    DesktopError::Io(format!("{context}: {error}"))
}

/// User override and system default for one key.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KeyEntry {
    pub user: Option<RawValue>,
    pub system: Option<RawValue>,
}

impl KeyEntry {
    /// The value COSMIC uses: the user file, or else the system default.
    pub fn effective(&self) -> Option<&RawValue> {
        self.user.as_ref().or(self.system.as_ref())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    entries: BTreeMap<ConfigKey, KeyEntry>,
}

impl Snapshot {
    pub fn entry(&self, key: &ConfigKey) -> Option<&KeyEntry> {
        self.entries.get(key)
    }
    pub fn user(&self, key: &ConfigKey) -> Option<&RawValue> {
        self.entries.get(key).and_then(|entry| entry.user.as_ref())
    }
    pub fn effective(&self, key: &ConfigKey) -> Option<&RawValue> {
        self.entries.get(key).and_then(KeyEntry::effective)
    }
    pub fn keys(&self) -> impl Iterator<Item = &ConfigKey> {
        self.entries.keys()
    }
    pub fn insert(&mut self, key: ConfigKey, entry: KeyEntry) {
        self.entries.insert(key, entry);
    }
    /// Takes `from`'s entries for `keys`; one absent there becomes absent.
    /// Drafts use it to keep their edits over an outside change: the
    /// current value becomes the baseline Apply checks and writes over.
    pub fn adopt(&mut self, from: &Snapshot, keys: &[ConfigKey]) {
        for key in keys {
            match from.entries.get(key) {
                Some(entry) => self.entries.insert(key.clone(), entry.clone()),
                None => self.entries.remove(key),
            };
        }
    }
}

/// The user-layer state a draft started from and the state it wants.
/// `None` means the user file is absent (COSMIC uses the system default).
#[derive(Clone, Debug, PartialEq)]
pub struct PatchEntry {
    pub original: Option<RawValue>,
    pub desired: Option<RawValue>,
}

/// Only the keys a draft touched. No-op entries are dropped.
///
/// Guards are keys a draft derived its values from without writing them,
/// such as the other member of a coupled pair. Apply checks them for
/// external changes like touched keys, but never writes or journals them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DesktopPatch {
    entries: BTreeMap<ConfigKey, PatchEntry>,
    guards: BTreeMap<ConfigKey, Option<RawValue>>,
    pub manual_guard: Option<Option<String>>,
    pub(crate) exact: bool,
    pub(crate) epoch_guard: Option<u64>,
    pub(crate) palette_ack: Option<u64>,
    pub(crate) activity_display: bool,
    /// The Dock or Panel item this patch adds, removes or moves, as the
    /// item editor names it, for the change's label and history.
    pub(crate) activity_item: Option<activity::ItemChange>,
    // Verified reversals, including semantic restorations that become no-ops.
    // Kept-current conflicts are deliberately absent from this attribution.
    reversed_native: std::collections::BTreeSet<ConfigKey>,
    reversed_widget: std::collections::BTreeSet<String>,
}

impl DesktopPatch {
    pub fn new() -> Self {
        Self::default()
    }
    pub(crate) fn exact() -> Self {
        Self {
            exact: true,
            ..Self::default()
        }
    }
    fn equal(&self, a: &Option<RawValue>, b: &Option<RawValue>) -> bool {
        if self.exact {
            exact_equal(a, b)
        } else {
            a == b
        }
    }
    pub fn set(&mut self, key: ConfigKey, original: Option<RawValue>, desired: Option<RawValue>) {
        if self.equal(&original, &desired) {
            self.entries.remove(&key);
            self.guards.insert(key, original);
        } else {
            self.guards.remove(&key);
            self.entries.insert(key, PatchEntry { original, desired });
        }
    }
    /// Requires `key` to still hold `original` (its user-layer value) when
    /// the patch is applied. A key the patch writes is already checked.
    pub fn guard(&mut self, key: ConfigKey, original: Option<RawValue>) {
        if !self.entries.contains_key(&key) {
            self.guards.insert(key, original);
        }
    }
    /// Later entries win; guards never replace a written entry. Only for
    /// pieces of one draft's own derivation; independent parts use `merge`.
    pub fn extend(&mut self, other: DesktopPatch) {
        for key in other.entries.keys() {
            self.guards.remove(key);
        }
        self.entries.extend(other.entries);
        for (key, original) in other.guards {
            self.guard(key, original);
        }
    }
    /// Combines independent parts of one change. Identical writes and
    /// guards are kept once. A key both parts write differently, a guard
    /// whose original disagrees, or a guard on a key the other part writes
    /// (so its derivation would be stale) is an overlap: `self` is left
    /// unchanged and the keys are returned.
    pub fn merge(&mut self, other: DesktopPatch) -> Result<(), DesktopError> {
        let mut overlaps = Vec::new();
        for (key, entry) in &other.entries {
            let clash = match (self.entries.get(key), self.guards.get(key)) {
                (Some(mine), _) => mine != entry,
                (None, Some(_)) => true,
                (None, None) => false,
            };
            if clash {
                overlaps.push(key.clone());
            }
        }
        for (key, original) in &other.guards {
            let clash = match (self.entries.get(key), self.guards.get(key)) {
                (Some(_), _) => true,
                (None, Some(mine)) => mine != original,
                (None, None) => false,
            };
            if clash {
                overlaps.push(key.clone());
            }
        }
        if !overlaps.is_empty() {
            overlaps.sort();
            overlaps.dedup();
            return Err(DesktopError::Overlap(overlaps));
        }
        self.entries.extend(other.entries);
        self.guards.extend(other.guards);
        Ok(())
    }
    pub fn get(&self, key: &ConfigKey) -> Option<&PatchEntry> {
        self.entries.get(key)
    }
    pub fn guards(&self) -> impl Iterator<Item = (&ConfigKey, &Option<RawValue>)> {
        self.guards.iter()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&ConfigKey, &PatchEntry)> {
        self.entries.iter()
    }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn keys(&self) -> impl Iterator<Item = &ConfigKey> {
        self.entries.keys()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChangeEntry {
    pub key: ConfigKey,
    pub before: Option<RawValue>,
    pub after: Option<RawValue>,
}

/// What a successful apply changed, for conflict-aware Undo.
#[derive(Clone, Debug, PartialEq)]
pub struct AppliedChange {
    id: String,
    entries: Vec<ChangeEntry>,
}

#[allow(dead_code)] // Read by profiles and history in later stages.
impl AppliedChange {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn entries(&self) -> &[ChangeEntry] {
        &self.entries
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn touches(&self, key: &ConfigKey) -> bool {
        self.entries.iter().any(|entry| &entry.key == key)
    }
}

/// Persistent record of an in-flight transaction (`desktop-journal.json`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Journal {
    pub format: String,
    /// 1: COSMIC keys only (readable by older releases); 2: with `widget`
    /// (no longer written); 3: an owned transaction, with `undo` and an
    /// optional `widget`.
    pub version: u32,
    pub id: String,
    pub operation: String,
    pub created: String,
    pub entries: Vec<JournalEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<WidgetChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undo: Option<UndoBefore>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design: Option<DesignWrite>,
}

/// The persistent latest change as an owned transaction found it, kept in
/// its journal until the transaction has fully succeeded. The text is kept
/// as it was, even when it is damaged or from another version; it is only
/// ever written back to the store's own record file, never interpreted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UndoBefore {
    /// The exact text of `desktop-undo.json`; `None` when there was none.
    pub record: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub namespace: String,
    pub version: u64,
    pub key: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

impl From<&ChangeEntry> for JournalEntry {
    fn from(entry: &ChangeEntry) -> Self {
        Self {
            namespace: entry.key.namespace.clone(),
            version: entry.key.version,
            key: entry.key.key.clone(),
            before: entry.before.as_ref().map(|value| value.0.clone()),
            after: entry.after.as_ref().map(|value| value.0.clone()),
        }
    }
}

/// Validates a recorded entry: a whitelisted key and bounded values.
impl TryFrom<&JournalEntry> for ChangeEntry {
    type Error = DesktopError;
    fn try_from(entry: &JournalEntry) -> Result<Self, DesktopError> {
        let key = ConfigKey::new(entry.namespace.clone(), entry.version, entry.key.clone())?;
        if rule_for(&key).is_none() {
            return Err(DesktopError::NotAllowed(key));
        }
        let before = entry.before.clone().map(RawValue::from_disk);
        let after = entry.after.clone().map(RawValue::from_disk);
        if before
            .iter()
            .chain(after.iter())
            .any(|value| value.0.len() > MAX_VALUE_BYTES)
        {
            return Err(DesktopError::InvalidValue {
                key: Some(key),
                message: "Journal value is too large".into(),
            });
        }
        Ok(Self { key, before, after })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecoveryReport {
    /// Keys returned to their value from before the interrupted change.
    pub restored: Vec<ConfigKey>,
    /// Keys the interrupted change had not reached.
    pub unchanged: Vec<ConfigKey>,
    /// Keys changed by someone else afterwards; left as they are.
    pub kept_external: Vec<ConfigKey>,
    /// Widget settings returned to their previous values.
    pub restored_fields: Vec<String>,
    /// Widget settings changed by someone else afterwards; left as they are.
    pub kept_fields: Vec<String>,
    /// The saved Undo entry was changed by someone else afterwards and was
    /// left as it is (it could not be returned to its earlier state).
    pub kept_undo: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Access {
    Live,
    Fixture(PathBuf),
    ReadOnly(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootChoice {
    Live,
    Fixture(PathBuf),
    /// An alternate Halcyon config without a fixture: read live
    /// values for display, never write them.
    ReadOnlyLive,
}

/// The preview isolation contract in one place.
pub fn choose_root(app_config_override: bool, fixture: Option<PathBuf>) -> RootChoice {
    match fixture {
        Some(root) => RootChoice::Fixture(root),
        None if app_config_override => RootChoice::ReadOnlyLive,
        None => RootChoice::Live,
    }
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) enum Fault {
    FailWrite(ConfigKey),
    InterruptAfter(usize),
    InterruptBeforeEpoch,
    PauseAfterEpoch(
        std::sync::Arc<std::sync::Barrier>,
        std::sync::Arc<std::sync::Barrier>,
    ),
    /// Another program writes this raw value to the key at this index just
    /// before the transaction gets to it.
    ChangedElsewhereBefore(usize, &'static str),
    /// Writing `write` fails, then restoring `rollback` fails too.
    FailWriteAndRollback {
        write: ConfigKey,
        rollback: ConfigKey,
    },
    /// The process stops after every key is written, before the widget
    /// configuration is.
    InterruptBeforeWidget,
    /// The process stops after both parts are written, before the Undo
    /// record and journal removal.
    InterruptBeforeRecord,
    /// The process stops after the Undo record is written or cleared,
    /// before the journal is removed.
    InterruptAfterRecord,
    FailWidget,
    InterruptBetweenDesignFiles,
    FailRecord,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation {
    Apply,
    Undo,
}

/// What a transaction does with the persistent latest change.
#[derive(Clone, Copy)]
enum Record<'a> {
    /// Plain `apply`/`undo`: leave it as it is.
    Keep,
    /// Replace it with this transaction.
    Set(Owner, &'a str, Source),
    /// A successful Undo of the record with this id clears it.
    Clear(&'a str),
    /// Cancel an exact receipt without discarding newer manual actions.
    Retire(&'a LatestChange),
}

/// The widget configuration a transaction will write, as read under lock.
struct WidgetWrite {
    path: PathBuf,
    change: WidgetChange,
    design: Option<DesignWrite>,
}

#[derive(Clone, Debug)]
pub struct DesktopStore {
    user_root: PathBuf,
    system_roots: Vec<PathBuf>,
    state_dir: PathBuf,
    access: Access,
    /// The Halcyon configuration file, from trusted context only.
    app_config: Option<PathBuf>,
    #[cfg(test)]
    fault: std::cell::RefCell<Option<Fault>>,
}

fn system_roots_from(data_home: Option<PathBuf>, data_dirs: Option<String>) -> Vec<PathBuf> {
    let dirs = data_dirs
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let mut roots = Vec::new();
    for dir in data_home
        .into_iter()
        .chain(dirs.split(':').map(PathBuf::from))
    {
        if dir.is_absolute() {
            let root = dir.join("cosmic");
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
    }
    roots
}

fn live_system_roots() -> Vec<PathBuf> {
    system_roots_from(dirs::data_dir(), std::env::var("XDG_DATA_DIRS").ok())
}

impl DesktopStore {
    /// Live settings, a read-only live view for alternate app configs, or an
    /// explicit fixture. A configured fixture that is unusable is an error.
    ///
    /// The widget configuration participant follows the same contract: the
    /// live (or alternate preview) `config.json`, but for a fixture only an
    /// explicitly configured alternate file, never the real one.
    pub fn from_env() -> Result<Self, DesktopError> {
        let fixture = std::env::var_os(FIXTURE_ENV).map(PathBuf::from);
        let alternate = std::env::var_os("DESKTOP_CARDS_CONFIG").map(PathBuf::from);
        match choose_root(alternate.is_some(), fixture) {
            RootChoice::Fixture(root) => {
                let store = Self::fixture(&root)?;
                Ok(match alternate {
                    Some(path) => store.with_app_config(path),
                    None => store,
                })
            }
            RootChoice::ReadOnlyLive => Ok(Self::live()?
                .read_only(PREVIEW_REASON)
                .with_app_config_opt(crate::config::Config::path())),
            RootChoice::Live => {
                Ok(Self::live()?.with_app_config_opt(crate::config::Config::path()))
            }
        }
    }
    pub fn live() -> Result<Self, DesktopError> {
        let config = dirs::config_dir()
            .ok_or_else(|| DesktopError::Io("Configuration directory unavailable".into()))?;
        let data = crate::paths::data_dir()
            .ok_or_else(|| DesktopError::Io("Data directory unavailable".into()))?;
        Ok(Self::with_roots(
            config.join("cosmic"),
            live_system_roots(),
            data,
            Access::Live,
        )
        .with_app_config_opt(crate::config::Config::path()))
    }
    /// Layout: `<root>/config/cosmic` (user), `<root>/system/cosmic` (system
    /// defaults, else the installed read-only defaults), `<root>/state`.
    pub fn fixture(root: &Path) -> Result<Self, DesktopError> {
        if !root.is_absolute() || !root.is_dir() {
            return Err(DesktopError::Io(format!(
                "{FIXTURE_ENV} must name an existing absolute directory: {}",
                root.display()
            )));
        }
        let system = root.join("system/cosmic");
        let system_roots = if system.is_dir() {
            vec![system]
        } else {
            live_system_roots()
        };
        Ok(Self::with_roots(
            root.join("config/cosmic"),
            system_roots,
            root.join("state"),
            Access::Fixture(root.into()),
        ))
    }
    pub fn with_roots(
        user_root: PathBuf,
        system_roots: Vec<PathBuf>,
        state_dir: PathBuf,
        access: Access,
    ) -> Self {
        Self {
            user_root,
            system_roots,
            state_dir,
            access,
            app_config: None,
            #[cfg(test)]
            fault: std::cell::RefCell::new(None),
        }
    }
    pub fn read_only(mut self, reason: impl Into<String>) -> Self {
        self.access = Access::ReadOnly(reason.into());
        self
    }
    /// The widget configuration owned transactions may also change. Must
    /// come from trusted context (the environment or a test), never from a
    /// profile, journal or Undo record.
    pub fn with_app_config(mut self, path: PathBuf) -> Self {
        self.app_config = path.is_absolute().then_some(path);
        self
    }
    fn with_app_config_opt(self, path: Option<PathBuf>) -> Self {
        match path {
            Some(path) => self.with_app_config(path),
            None => self,
        }
    }
    pub fn app_config(&self) -> Option<&Path> {
        self.app_config.as_deref()
    }
    #[allow(dead_code)] // Adapter surface for later stages.
    pub fn access(&self) -> &Access {
        &self.access
    }
    pub fn writable(&self) -> bool {
        !matches!(self.access, Access::ReadOnly(_))
    }
    pub fn fixture_root(&self) -> Option<&Path> {
        match &self.access {
            Access::Fixture(root) => Some(root),
            _ => None,
        }
    }
    /// `<config>/cosmic`: the user layer every key lives under.
    pub fn user_root(&self) -> &Path {
        &self.user_root
    }
    /// Where the lock and journal live (also legacy palette backups).
    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }
    pub fn user_path(&self, key: &ConfigKey) -> PathBuf {
        self.user_root.join(key.relative())
    }
    fn journal_path(&self) -> PathBuf {
        self.state_dir.join(JOURNAL)
    }
    fn undo_path(&self) -> PathBuf {
        self.state_dir.join(UNDO)
    }

    /// Reads user overrides and system defaults. Performs no writes and
    /// creates no directories.
    pub fn load(&self, keys: &[ConfigKey]) -> Result<Snapshot, DesktopError> {
        let mut snapshot = Snapshot::default();
        for key in keys {
            let entry = KeyEntry {
                user: self.read_user(key)?,
                system: self.read_system(key),
            };
            snapshot.insert(key.clone(), entry);
        }
        Ok(snapshot)
    }
    pub fn read_user(&self, key: &ConfigKey) -> Result<Option<RawValue>, DesktopError> {
        self.confine(&self.user_path(key))?;
        super::recovery::read_native(self, key)
    }
    fn read_system(&self, key: &ConfigKey) -> Option<RawValue> {
        self.system_roots
            .iter()
            .map(|root| root.join(key.relative()))
            .find(|path| path.is_file())
            .and_then(|path| read_value(&path).ok().flatten())
    }

    pub fn apply(&self, patch: &DesktopPatch) -> Result<AppliedChange, DesktopError> {
        self.transact(patch, None, Operation::Apply, Record::Keep)
            .map(|committed| committed.change)
    }
    /// Restores the values from before `change`, unless a touched key has
    /// changed since; then nothing is written. (In-memory Undo; the pages
    /// use the persistent `undo_latest`.)
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn undo(&self, change: &AppliedChange) -> Result<AppliedChange, DesktopError> {
        let mut patch = DesktopPatch::exact();
        for entry in &change.entries {
            patch.set(entry.key.clone(), entry.after.clone(), entry.before.clone());
        }
        self.transact(&patch, None, Operation::Undo, Record::Keep)
            .map(|committed| committed.change)
    }
    /// Applies `patch` and the optional widget part as one recoverable
    /// change, and on full success records it as `owner`'s latest change
    /// (replacing the previous record) for Undo after a restart. On any
    /// failure the previous record stays.
    pub fn apply_owned(
        &self,
        patch: &DesktopPatch,
        widget: Option<&WidgetPatch>,
        owner: Owner,
        label: &str,
    ) -> Result<LatestChange, DesktopError> {
        super::actions::commit(self, patch, widget, owner, label, Source::Manual)
    }

    pub fn apply_action(
        &self,
        patch: &DesktopPatch,
        widget: Option<&WidgetPatch>,
        owner: Owner,
        label: &str,
        source: Source,
    ) -> Result<LatestChange, DesktopError> {
        if !undo::valid_label(label) {
            return Err(DesktopError::InvalidValue {
                key: None,
                message: "Invalid change label".into(),
            });
        }
        // A desktop write can depend on unchanged app colors. Keep those
        // guards under the app config lock even when no app member writes.
        let widget = widget.filter(|widget| widget.iter().next().is_some());
        if patch.is_empty() && widget.is_none_or(WidgetPatch::is_empty) {
            return Err(DesktopError::InvalidValue {
                key: None,
                message: "Nothing to change".into(),
            });
        }
        let committed = self.transact(
            patch,
            widget,
            Operation::Apply,
            Record::Set(owner, label, source),
        )?;
        if committed.change.is_empty() && committed.widget.is_none() {
            // Everything already matched under lock; no record was written.
            return Err(DesktopError::InvalidValue {
                key: None,
                message: "Nothing to change".into(),
            });
        }
        Ok(LatestChange {
            id: committed.change.id.clone(),
            owner,
            label: label.to_string(),
            created: committed.created,
            change: committed.change,
            widget: committed.widget,
            // As recorded: only the Dock and panel's own item edits are named.
            item: patch
                .activity_item
                .clone()
                .filter(|_| owner == Owner::DockPanel && source == Source::Manual),
        })
    }
    /// The persistent latest change, validated. `Ok(None)` when there is
    /// none; an error explains a record that cannot be used (damaged, or
    /// from another version). Reads only.
    pub fn latest_change(&self) -> Result<Option<LatestChange>, DesktopError> {
        self.history_file()?
            .as_ref()
            .and_then(undo::UndoFile::manual)
            .cloned()
            .map(LatestChange::from_file)
            .transpose()
    }
    fn history_file(&self) -> Result<Option<undo::UndoFile>, DesktopError> {
        let path = self.undo_path();
        let bytes = match read_record(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error("Could not read the saved history", error)),
        };
        let file: undo::UndoFile = serde_json::from_slice(&bytes).map_err(|_| {
            DesktopError::UndoUnavailable("The saved Undo record is damaged.".into())
        })?;
        file.validate_history()?;
        Ok(Some(file))
    }
    pub fn manual_at(&self) -> Option<String> {
        let history = self.history_file().ok().flatten().and_then(|file| {
            file.manual_at
                .clone()
                .or_else(|| file.manual().map(|f| f.created.clone()))
        });
        history
            .into_iter()
            .chain(self.epoch().ok().and_then(|e| e.manual_at))
            .max_by_key(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
    }
    /// Undoes the latest change, which must still be the one with `id` (the
    /// one the caller showed), so a newer operation is never undone by
    /// mistake. Refuses without writing when any touched key or widget
    /// setting changed since. Restores the widget file's exact bytes when it
    /// is still exactly as the change left it; otherwise only touched
    /// settings, keeping newer unrelated ones. Clears the record on success.
    pub fn undo_latest(&self, id: &str) -> Result<AppliedChange, DesktopError> {
        let latest = self
            .latest_change()?
            .filter(|latest| latest.id == id)
            .ok_or_else(|| {
                DesktopError::UndoUnavailable(
                    "This change can no longer be undone; a newer change replaced it.".into(),
                )
            })?;
        let (patch, widget) = self.reversal(&latest)?;
        self.transact(
            &patch,
            widget.as_ref(),
            Operation::Undo,
            Record::Clear(&latest.id),
        )
        .map(|committed| committed.change)
    }

    /// Reverse the caller's exact receipt and retire it in the same journaled
    /// transaction, preserving every newer unrelated Undo action.
    pub fn undo_change(&self, change: &LatestChange) -> Result<AppliedChange, DesktopError> {
        let (patch, widget) = self.reversal(change)?;
        let record = if change.change.entries.is_empty()
            && change.widget.as_ref().is_none_or(|w| w.fields.is_empty())
        {
            Record::Keep
        } else {
            Record::Retire(change)
        };
        self.transact(&patch, widget.as_ref(), Operation::Undo, record)
            .map(|committed| committed.change)
    }

    pub(super) fn restore_desktop(
        &self,
        plan: &super::recovery::RestorePlan,
    ) -> Result<LatestChange, DesktopError> {
        self.check(&plan.patch, Operation::Apply)?;
        let _config = self
            .app_config
            .as_deref()
            .map(config::lock_config)
            .transpose()
            .map_err(DesktopError::Io)?;
        let _desktop = self.lock()?;
        super::recovery::verify_plan(self, plan)?;
        let id = format!(
            "restore-{}",
            getrandom::u64().map_err(|e| DesktopError::Io(e.to_string()))?
        );
        let committed = self.commit_locked(
            &plan.patch,
            self.app_config
                .as_deref()
                .filter(|_| plan.widgets.iter().next().is_some())
                .map(|p| (&plan.widgets, p)),
            Operation::Apply,
            Record::Set(Owner::Recovery, "Restore desktop", Source::Manual),
            id,
        )?;
        Ok(LatestChange {
            id: committed.change.id.clone(),
            owner: Owner::Recovery,
            label: "Restore desktop".into(),
            created: committed.created,
            change: committed.change,
            widget: committed.widget,
            item: None,
        })
    }

    pub(super) fn capture_baseline(
        &self,
        kind: super::recovery::BaselineKind,
    ) -> Result<super::recovery::BaselineInfo, DesktopError> {
        self.preflight(&DesktopPatch::new())?;
        let _config = self
            .app_config
            .as_deref()
            .map(config::lock_config)
            .transpose()
            .map_err(DesktopError::Io)?;
        let _desktop = self.lock()?;
        super::recovery::ensure_locked(self, kind)
    }

    /// Direct app preference writers already hold config.json.lock. This
    /// sibling boundary takes only the desktop lock, so it cannot recurse.
    pub(crate) fn capture_config_locked(&self) -> Result<(), DesktopError> {
        self.preflight(&DesktopPatch::new())?;
        let _desktop = self.lock()?;
        super::recovery::ensure_locked(self, super::recovery::BaselineKind::OriginalDesktop)?;
        Ok(())
    }

    /// The GSettings writer shares capture and writer serialization, while
    /// retaining its own bounded backend operation and read-back verification.
    pub(crate) fn mutate_motion<T>(
        &self,
        backend: &str,
        capture: impl FnOnce() -> Result<Option<Vec<u8>>, String>,
        write: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        self.preflight(&DesktopPatch::new())
            .map_err(|e| e.to_string())?;
        let _config = self
            .app_config
            .as_deref()
            .map(config::lock_config)
            .transpose()?;
        let _desktop = self.lock().map_err(|e| e.to_string())?;
        super::recovery::ensure_motion_locked(self, backend, capture).map_err(|e| e.to_string())?;
        write()
    }

    fn ensure_writable(&self) -> Result<(), DesktopError> {
        match &self.access {
            Access::ReadOnly(reason) => Err(DesktopError::ReadOnly(reason.clone())),
            _ => Ok(()),
        }
    }

    /// Everything `apply(patch)` refuses before it takes its lock, checked
    /// without touching the filesystem: write access, whitelisted keys and
    /// valid values, and, for a fixture, that every key, the lock and the
    /// journal stay inside the root. A caller with side effects of its own
    /// runs this first, so a refused change leaves nothing behind.
    pub fn preflight(&self, patch: &DesktopPatch) -> Result<(), DesktopError> {
        self.check(patch, Operation::Apply)?;
        self.confine(&self.state_dir.join(LOCK))?;
        self.confine(&self.journal_path())
    }

    /// The path of caller-owned state file `name` (a plain file name, not the
    /// store's own lock or journal) after the same write-access and
    /// confinement checks as the store's lock. Creates nothing.
    pub fn state_file(&self, name: &str) -> Result<PathBuf, DesktopError> {
        self.ensure_writable()?;
        if !valid_name(name, 64)
            || [LOCK, JOURNAL, UNDO, PROFILES_DIR, "desktop-recovery"].contains(&name)
        {
            return Err(DesktopError::InvalidKey(format!(
                "Invalid state file name: {name}"
            )));
        }
        let path = self.state_dir.join(name);
        self.confine(&path)?;
        Ok(path)
    }

    /// `<state>/<dir>/<name>` for app-owned state below a reserved
    /// directory (the profile library), with the same write-access and
    /// confinement checks. Both parts must be plain names, and an existing
    /// directory or leaf may not be a symlink. Creates nothing.
    pub fn state_subpath(&self, dir: &str, name: Option<&str>) -> Result<PathBuf, DesktopError> {
        self.ensure_writable()?;
        if ![PROFILES_DIR, crate::desktop::profiles::packages::ASSET_DIR].contains(&dir)
            || name.is_some_and(|name| !valid_name(name, 96))
        {
            return Err(DesktopError::InvalidKey("Invalid state path".into()));
        }
        let folder = self.state_dir.join(dir);
        let path = match name {
            Some(name) => folder.join(name),
            None => folder.clone(),
        };
        for candidate in [&folder, &path] {
            if fs::symlink_metadata(candidate).is_ok_and(|meta| meta.file_type().is_symlink()) {
                return Err(DesktopError::Io(format!(
                    "{} is a symbolic link; nothing was changed",
                    candidate.display()
                )));
            }
        }
        self.confine(&path)?;
        Ok(path)
    }

    fn check(&self, patch: &DesktopPatch, operation: Operation) -> Result<(), DesktopError> {
        self.ensure_writable()?;
        for (key, entry) in patch.iter() {
            let rule = rule_for(key).ok_or_else(|| DesktopError::NotAllowed(key.clone()))?;
            self.confine(&self.user_path(key))?;
            // Undo restores the exact bytes that were read before Apply;
            // new values must be well-formed and valid for their key.
            if operation == Operation::Apply
                && let Some(value) = &entry.desired
            {
                value
                    .check()
                    .and_then(|()| (rule.validate)(value))
                    .map_err(|message| DesktopError::InvalidValue {
                        key: Some(key.clone()),
                        message,
                    })?;
            }
        }
        Ok(())
    }

    /// Checks, locks and commits. Lock order, shared with the Python
    /// controller (`config_lock`) and the palette helper: `config.json.lock`
    /// (also for a native-only change), then `desktop.lock`. Legacy palette
    /// helpers take their compatibility lock before entering this boundary. The configuration lock is never taken while
    /// `desktop.lock` is held.
    fn transact(
        &self,
        patch: &DesktopPatch,
        widget: Option<&WidgetPatch>,
        operation: Operation,
        record: Record,
    ) -> Result<Committed, DesktopError> {
        self.check(patch, operation)?;
        let widget = widget.filter(|widget| widget.iter().next().is_some());
        if widget.is_some() && self.app_config.is_none() {
            return Err(DesktopError::ReadOnly(
                "Widget settings can’t be changed here".into(),
            ));
        }
        // Capture app appearance even for a native-only mutation. Every writer
        // takes these locks in the same order; CLI parents hold neither.
        let config_path = self.app_config.clone();
        self.confine(&self.state_dir.join(LOCK))?;
        self.confine(&self.journal_path())?;
        if !matches!(record, Record::Keep) {
            self.confine(&self.undo_path())?;
        }
        let id = format!(
            "{}-{}-{}",
            chrono::Local::now().format("%Y%m%dT%H%M%S%.3f"),
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        );
        if patch.is_empty()
            && widget.is_none()
            && !matches!(
                record,
                Record::Clear(_) | Record::Retire(_) | Record::Set(Owner::Recovery, _, _)
            )
            && patch.palette_ack.is_none()
        {
            return Ok(Committed::empty(id));
        }
        let _config_lock = config_path
            .as_deref()
            .map(config::lock_config)
            .transpose()
            .map_err(|error| {
                DesktopError::Io(format!("Could not lock widget settings: {error}"))
            })?;
        let _lock = self.lock()?;
        self.commit_locked(
            patch,
            widget.zip(config_path.as_deref()),
            operation,
            record,
            id,
        )
    }

    fn commit_locked(
        &self,
        patch: &DesktopPatch,
        widget: Option<(&WidgetPatch, &Path)>,
        operation: Operation,
        record: Record,
        id: String,
    ) -> Result<Committed, DesktopError> {
        if patch.keys().any(|key| {
            key.namespace() == dock_islands_core::NAMESPACE
                || key.namespace() == dock_islands_core::material::NAMESPACE
        }) && !super::islands::Installation::for_write(self).installed()
        {
            return Err(DesktopError::Io(
                "Islands is not installed. Saved add-on preferences were kept.".into(),
            ));
        }
        if let Some(expected) = patch.epoch_guard {
            if self.restore_epoch()? != expected {
                return Err(DesktopError::Io(
                    "A desktop restore superseded this automatic work".into(),
                ));
            }
        }
        if let Some(expected) = &patch.manual_guard {
            if self.manual_at() != *expected {
                return Err(DesktopError::Io(
                    "A newer manual choice superseded this automatic switch".into(),
                ));
            }
        }
        if self.journal_path().exists() {
            return Err(DesktopError::RecoveryPending);
        }
        if let Some(path) = &self.app_config {
            crate::design::storage::check(path).map_err(DesktopError::Io)?;
        }
        // Preparation may observe the new epoch before Restore publishes its
        // acknowledgement. Make the final ownership decision under these locks.
        if let (Some(epoch), Some(stamp)) = (patch.epoch_guard, patch.palette_ack) {
            if self.palette_seen(epoch, stamp)? {
                return Ok(Committed::empty(id));
            }
        }
        if let Record::Clear(expected) = record
            && self.latest_change()?.map(|latest| latest.id).as_deref() != Some(expected)
        {
            return Err(DesktopError::UndoUnavailable(
                "This change can no longer be undone; a newer change replaced it.".into(),
            ));
        }
        let mut conflicts = Vec::new();
        for (key, original) in patch
            .iter()
            .map(|(key, entry)| (key, &entry.original))
            .chain(patch.guards())
        {
            if !patch.equal(&self.read_user(key)?, original) {
                conflicts.push(key.clone());
            }
        }
        conflicts.sort();
        let mut fields = Vec::new();
        let write = match widget {
            Some((widget, path)) => self.prepare_widget(widget, path, &mut fields)?,
            None => None,
        };
        if !fields.is_empty() {
            return Err(DesktopError::FieldConflict {
                keys: conflicts,
                fields,
            });
        }
        if !conflicts.is_empty() {
            return Err(DesktopError::Conflict(conflicts));
        }
        if patch.is_empty()
            && write.is_none()
            && !matches!(
                record,
                Record::Clear(_) | Record::Retire(_) | Record::Set(Owner::Recovery, _, _)
            )
            && patch.palette_ack.is_none()
        {
            return Ok(Committed::empty(id));
        }
        // The record as found, kept until this transaction fully succeeds.
        let undo = match record {
            Record::Keep => None,
            Record::Set(..) | Record::Clear(_) | Record::Retire(_) => Some(UndoBefore {
                record: self.snapshot_record()?,
            }),
        };
        let created = chrono::Local::now().to_rfc3339();
        // The journal, the Undo record and the writes below all follow the
        // same order: `write_rank`, then key order (stable; every key
        // outside the wallpaper namespace ranks 0).
        let mut order: Vec<(&ConfigKey, &PatchEntry)> = patch.iter().collect();
        order.sort_by_key(|(key, _)| super::write_rank(key));
        let entries: Vec<ChangeEntry> = order
            .iter()
            .map(|(key, entry)| ChangeEntry {
                key: (*key).clone(),
                before: entry.original.clone(),
                after: entry.desired.clone(),
            })
            .collect();
        let restore_boundary = matches!(record, Record::Set(Owner::Recovery, _, _))
            || (operation == Operation::Undo
                && match record {
                    Record::Retire(receipt) => receipt.owner == Owner::Recovery,
                    _ => self
                        .latest_change()?
                        .is_some_and(|c| c.owner == Owner::Recovery),
                });
        let journal = Journal {
            format: JOURNAL_FORMAT.into(),
            version: match (&undo, &write) {
                (_, Some(write)) if write.design.is_some() => 4,
                (Some(_), _) => 3,
                (None, Some(_)) => 2,
                (None, None) => 1,
            },
            id: id.clone(),
            operation: match (operation, restore_boundary) {
                (Operation::Apply, true) => "restore",
                (Operation::Undo, true) => "undo_restore",
                (Operation::Apply, false) => "apply",
                (Operation::Undo, false) => "undo",
            }
            .into(),
            created: created.clone(),
            entries: entries.iter().map(JournalEntry::from).collect(),
            widget: write.as_ref().map(|write| write.change.clone()),
            design: write.as_ref().and_then(|write| write.design.clone()),
            undo,
        };
        let bytes = serde_json::to_vec_pretty(&journal)
            .map_err(|error| DesktopError::Io(error.to_string()))?;
        let change = AppliedChange { id, entries };
        let widget_change = write.as_ref().map(|write| write.change.clone());
        let mut new_record = match record {
            Record::Set(owner, label, source) => {
                let mut next = LatestChange {
                    id: change.id.clone(),
                    owner,
                    label: label.to_string(),
                    created: created.clone(),
                    change: change.clone(),
                    widget: widget_change.clone(),
                    // Only the Dock and panel's own item edits are named.
                    item: patch
                        .activity_item
                        .clone()
                        .filter(|_| owner == Owner::DockPanel && source == Source::Manual),
                }
                .to_file();
                next.source = source;
                let summary = activity::has_changes(&next)
                    .then(|| activity::summarize(&next, patch.activity_display));
                // Invalid legacy records are still preserved in the journal;
                // they cannot be interpreted as useful history.
                if let Ok(Some(mut previous)) = self.history_file() {
                    next.palette_ack = previous.palette_ack;
                    if source == Source::Derived {
                        // Every relevant manual action retains its own continuous
                        // derivation, including actions below unrelated controls.
                        previous.attribute_derived(&next.entries, next.widget.as_ref());
                    }
                    if source != Source::Manual {
                        next.manual_at = previous.manual_at.clone();
                    }
                    next = previous.followed_by(next);
                } else if source != Source::Manual {
                    next.manual_at = None;
                }
                if let Some(stamp) = patch.palette_ack {
                    next.palette_ack = Some((self.restore_epoch()?, stamp));
                }
                if let Some(summary) = summary {
                    activity::append(&mut next.recent, summary);
                }
                next.transaction_id = Some(change.id.clone());
                Some(
                    serde_json::to_vec_pretty(&next)
                        .map_err(|error| DesktopError::Io(error.to_string()))?,
                )
            }
            Record::Clear(_) | Record::Retire(_) => {
                let expected = match record {
                    Record::Clear(id) => id,
                    Record::Retire(receipt) => &receipt.id,
                    _ => unreachable!(),
                };
                let mut file = self
                    .history_file()?
                    .or_else(|| match record {
                        Record::Retire(receipt) => Some(receipt.to_file()),
                        _ => None,
                    })
                    .ok_or_else(|| DesktopError::UndoUnavailable("No history".into()))?;
                let palette_ack = file.palette_ack;
                let recent_partial = file.recent_partial || file.recent.is_empty();
                let mut recent = std::mem::take(&mut file.recent);
                let mut history = std::mem::take(&mut file.history);
                if file.source == Source::Manual {
                    history.push(file);
                }
                let index = history.iter().rposition(|file| file.id == expected);
                let removed = if let Some(index) = index {
                    let removed = history.remove(index);
                    if matches!(record, Record::Clear(_)) {
                        history.truncate(index);
                    }
                    removed
                } else if let Record::Retire(receipt) = record {
                    // Bounded global history may have evicted a still-owned
                    // wizard receipt. Reversal guards still protect its fields.
                    receipt.to_file()
                } else {
                    return Err(DesktopError::UndoUnavailable("History changed".into()));
                };
                // A semantic Undo can restore an older value using today's
                // inputs. Bridge that verified restoration into older actions;
                // only verified reversals, including no-op restorations, qualify.
                // Retained conflicts and Keep current never create attribution.
                let restored: Vec<_> = patch
                    .reversed_native
                    .iter()
                    .map(|key| {
                        let after = patch
                            .entries
                            .get(key)
                            .map(|entry| entry.desired.clone())
                            .unwrap_or_else(|| patch.guards.get(key).cloned().flatten());
                        ChangeEntry {
                            key: key.clone(),
                            before: after.clone(),
                            after,
                        }
                    })
                    .collect();
                let restored_widget = widget_change.as_ref().map(|change| {
                    let mut restored = change.clone();
                    restored
                        .fields
                        .retain(|field| patch.reversed_widget.contains(&field.pointer));
                    restored
                });
                for older in history.iter_mut().take(index.unwrap_or(0)) {
                    older.attribute_restoration(&removed, &restored, restored_widget.as_ref());
                }
                let mut previous = history.pop().unwrap_or_else(|| {
                    let mut empty = LatestChange {
                        id: change.id.clone(),
                        owner: Owner::Preferences,
                        label: "Undo".into(),
                        created: created.clone(),
                        change: AppliedChange {
                            id: change.id.clone(),
                            entries: Vec::new(),
                        },
                        widget: None,
                        item: None,
                    }
                    .to_file();
                    empty.source = Source::Derived;
                    empty
                });
                let mut summary = activity::summarize(&removed, false);
                summary.id = change.id.clone();
                summary.at = created.clone();
                summary.cause = activity::Cause::You;
                // Keep current writes nothing back. A retired receipt counts
                // as kept only when it reversed nothing at all, so a wizard
                // Cancel of no-op restorations still reads as undone.
                let wrote_nothing = change.entries.is_empty()
                    && widget_change.as_ref().is_none_or(|w| w.fields.is_empty());
                let kept = match record {
                    Record::Retire(_) => {
                        wrote_nothing
                            && patch.reversed_native.is_empty()
                            && patch.reversed_widget.is_empty()
                    }
                    _ => wrote_nothing,
                };
                summary.action = if kept {
                    activity::Action::Kept
                } else {
                    activity::Action::Undone
                };
                summary.details.clear();
                activity::append(&mut recent, summary);
                previous.recent = recent;
                previous.recent_partial = recent_partial;
                previous.history = history;
                previous.palette_ack = palette_ack;
                previous.transaction_id = Some(change.id.clone());
                previous.manual_at = Some(created.clone());
                Some(
                    serde_json::to_vec_pretty(&previous)
                        .map_err(|error| DesktopError::Io(error.to_string()))?,
                )
            }
            Record::Keep => None,
        };
        // Nested records and configuration text grow when escaped; nothing
        // is written that `read_record` would later refuse.
        if self
            .app_config
            .as_deref()
            .map(crate::design::storage::read)
            .transpose()
            .map_err(DesktopError::Io)?
            .flatten()
            .is_some()
        {
            if let Some(bytes) = &mut new_record {
                let mut file: undo::UndoFile =
                    serde_json::from_slice(bytes).map_err(|e| DesktopError::Io(e.to_string()))?;
                file.version = 2;
                *bytes = serde_json::to_vec_pretty(&file)
                    .map_err(|e| DesktopError::Io(e.to_string()))?;
            }
        }
        if bytes.len() as u64 > MAX_RECORD_BYTES
            || new_record
                .as_ref()
                .is_some_and(|bytes| bytes.len() as u64 > MAX_RECORD_BYTES)
        {
            return Err(DesktopError::InvalidValue {
                key: None,
                message: "This change is too large to record safely; nothing was changed.".into(),
            });
        }
        self.confine(&self.journal_path())?;
        if matches!(record, Record::Set(Owner::Recovery, _, _)) {
            super::recovery::save_pre_restore(self)?;
        }
        // Capture only after every read-only transaction validation succeeds.
        super::recovery::ensure_locked(self, super::recovery::BaselineKind::OriginalDesktop)?;
        atomic_write(&self.journal_path(), &bytes, Some(0o600))
            .map_err(|error| io_error("Could not record the change journal", error))?;
        #[cfg(test)]
        if matches!(*self.fault.borrow(), Some(Fault::InterruptBeforeEpoch)) {
            return Err(DesktopError::Io(
                "Simulated journal-before-epoch interruption".into(),
            ));
        }
        if restore_boundary {
            self.advance_restore_epoch()?;
        }
        #[cfg(test)]
        if let Some(Fault::PauseAfterEpoch(published, resume)) = self.fault.borrow().clone() {
            published.wait();
            resume.wait();
        }
        let mut written: Vec<&ConfigKey> = Vec::new();
        for (index, (key, entry)) in order.into_iter().enumerate() {
            #[cfg(test)]
            if matches!(*self.fault.borrow(), Some(Fault::InterruptAfter(limit)) if limit == index)
            {
                return Err(DesktopError::Io("Simulated interruption".into()));
            }
            #[cfg(test)]
            if let Some(Fault::ChangedElsewhereBefore(at, value)) = self.fault.borrow().clone()
                && at == index
            {
                let _ = fs::write(self.user_path(key), value);
            }
            let _ = index;
            // COSMIC Settings or the compositor may have changed this key
            // since the check above. Never overwrite that: put back what this
            // change wrote so far and report the conflict.
            match self.read_user(key) {
                Ok(current) if patch.equal(&current, &entry.original) => {}
                Ok(_) => {
                    return Err(match self.fail(patch, &written, None, String::new()) {
                        DesktopError::RolledBack(_) => DesktopError::Conflict(vec![key.clone()]),
                        other => other,
                    });
                }
                Err(error) => return Err(self.fail(patch, &written, None, error.to_string())),
            }
            if let Err(error) = self.write_key(key, entry.desired.as_ref()) {
                written.push(key);
                return Err(self.fail(patch, &written, None, error.to_string()));
            }
            written.push(key);
            if entry.original.is_none() && entry.desired.is_some() {
                if let Err(error) = super::recovery::record_created(self, key) {
                    return Err(self.fail(patch, &written, None, error.to_string()));
                }
            }
        }
        #[cfg(test)]
        if matches!(*self.fault.borrow(), Some(Fault::InterruptBeforeWidget)) {
            return Err(DesktopError::Io("Simulated interruption".into()));
        }
        if let Some(write) = &write
            && let Err(error) = self.write_widget(write)
        {
            #[cfg(test)]
            if matches!(
                *self.fault.borrow(),
                Some(Fault::InterruptBetweenDesignFiles)
            ) {
                return Err(DesktopError::Io(error));
            }
            return Err(self.fail(patch, &written, Some(write), error));
        }
        #[cfg(test)]
        if matches!(*self.fault.borrow(), Some(Fault::InterruptBeforeRecord)) {
            return Err(DesktopError::Io("Simulated interruption".into()));
        }
        if restore_boundary {
            let ack = Some(
                self.restore_palette_ack()
                    .map_err(|e| self.fail(patch, &written, write.as_ref(), e.to_string()))?,
            );
            if let Some(bytes) = &mut new_record {
                let mut next: undo::UndoFile = serde_json::from_slice(bytes)
                    .map_err(|e| self.fail(patch, &written, write.as_ref(), e.to_string()))?;
                next.palette_ack = ack;
                *bytes = serde_json::to_vec_pretty(&next)
                    .map_err(|e| self.fail(patch, &written, write.as_ref(), e.to_string()))?;
            }
        }
        if new_record
            .as_ref()
            .is_some_and(|bytes| bytes.len() as u64 > MAX_RECORD_BYTES)
        {
            return Err(self.fail(
                patch,
                &written,
                write.as_ref(),
                "Updated recovery history is too large".into(),
            ));
        }
        if let Err(error) = self.update_record(record, new_record.as_deref()) {
            return Err(self.fail(patch, &written, write.as_ref(), error));
        }
        #[cfg(test)]
        if matches!(*self.fault.borrow(), Some(Fault::InterruptAfterRecord)) {
            return Err(DesktopError::Io("Simulated interruption".into()));
        }
        // Every part is written. A journal that cannot be removed shows up as
        // a pending recovery rather than turning this into a false failure.
        let _ = fs::remove_file(self.journal_path());
        let _ = sync_dir(&self.state_dir);
        ACTION_REVISION.fetch_add(1, Ordering::Relaxed);
        Ok(Committed {
            change,
            widget: widget_change,
            created,
        })
    }

    /// Reads the widget configuration under its lock, checks every touched
    /// member against the draft's original and builds the new file on top
    /// of the current one (unknown and excluded members stay). Conflicting
    /// member labels are pushed to `conflicts`. `None` when nothing changes.
    fn prepare_widget(
        &self,
        patch: &WidgetPatch,
        path: &Path,
        conflicts: &mut Vec<String>,
    ) -> Result<Option<WidgetWrite>, DesktopError> {
        let invalid = |message: String| DesktopError::InvalidValue {
            key: None,
            message: format!("Widget settings: {message}"),
        };
        if let Some(text) = crate::design::storage::read(path).map_err(invalid)? {
            return self.prepare_design(patch, path, conflicts, text);
        }
        if patch.iter().any(|(pointer, entry)| {
            pointer.starts_with("/recipes/") && entry.desired.as_ref().is_some_and(|v| !v.is_null())
        }) {
            return Err(invalid(
                "Get your widgets ready first: “Get ready…” in Halcyon › Updates.".into(),
            ));
        }
        let before = config::read_raw_config_text(path).map_err(invalid)?;
        let changes = || {
            patch
                .changes()
                .map(|(pointer, entry)| WidgetFieldChange {
                    pointer: pointer.to_string(),
                    before: entry.original.clone(),
                    after: entry.desired.clone(),
                })
                .collect::<Vec<_>>()
        };
        let (after, fields) = match patch.exact() {
            // Undo of a file still exactly as the change left it: its
            // original bytes come back.
            Some((expected, restore)) if *expected == before => {
                if let Some(text) = restore {
                    config::check_config_value(
                        &config::parse_config_text(Some(text)).map_err(invalid)?,
                    )
                    .map_err(invalid)?;
                }
                (restore.clone(), changes())
            }
            _ => {
                let mut value = config::parse_config_text(before.as_deref()).map_err(invalid)?;
                for (pointer, entry) in patch.iter() {
                    if widget::get(&value, pointer) != entry.original {
                        conflicts.push(widget::label(pointer));
                    }
                }
                if !conflicts.is_empty() {
                    return Ok(None);
                }
                for (pointer, entry) in patch.changes() {
                    widget::put(&mut value, pointer, entry.desired.clone()).map_err(invalid)?;
                }
                config::check_config_value(&value).map_err(invalid)?;
                (
                    Some(config::config_text(&value).map_err(invalid)?),
                    changes(),
                )
            }
        };
        if after == before || fields.is_empty() {
            return Ok(None);
        }
        Ok(Some(WidgetWrite {
            path: path.to_path_buf(),
            design: None,
            change: WidgetChange {
                design_v1: false,
                before,
                after,
                fields,
            },
        }))
    }

    fn write_widget(&self, write: &WidgetWrite) -> Result<(), String> {
        #[cfg(test)]
        if matches!(*self.fault.borrow(), Some(Fault::FailWidget)) {
            return Err("injected widget failure".into());
        }
        if let Some(design) = &write.design {
            return self.write_design(&write.path, design);
        }
        config::write_config_text(&write.path, write.change.after.as_deref())
    }

    /// The record's exact text, read under the transaction's locks for its
    /// journal. A record that cannot be kept as text (unreadable, oversized
    /// or not UTF-8) refuses the change before anything is written, rather
    /// than being lost.
    fn snapshot_record(&self) -> Result<Option<String>, DesktopError> {
        let path = self.undo_path();
        self.confine(&path)?;
        let unusable = |reason: String| {
            DesktopError::UndoUnavailable(format!(
                "The saved Undo record {} can’t be kept ({reason}); nothing was changed. Remove it to continue.",
                path.display()
            ))
        };
        match read_record(&path) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| unusable("it is not text".into())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(unusable(error.to_string())),
        }
    }

    /// Replaces (with `new_record`) or clears the persistent latest change.
    fn update_record(&self, record: Record, new_record: Option<&[u8]>) -> Result<(), String> {
        #[cfg(test)]
        if matches!(*self.fault.borrow(), Some(Fault::FailRecord))
            && !matches!(record, Record::Keep)
        {
            return Err("injected Undo record failure".into());
        }
        let path = self.undo_path();
        match record {
            Record::Keep => Ok(()),
            Record::Set(..) | Record::Clear(_) | Record::Retire(_) if new_record.is_some() => {
                let bytes = new_record.ok_or("No Undo record to write")?;
                match atomic_write(&path, bytes, Some(0o600)) {
                    Ok(()) => Ok(()),
                    // Renamed into place but not synced: the record is ours.
                    Err(_) if fs::read(&path).is_ok_and(|current| current == bytes) => Ok(()),
                    Err(error) => Err(format!("Could not record Undo: {error}")),
                }
            }
            Record::Set(..) | Record::Clear(_) | Record::Retire(_) => {
                match fs::remove_file(&path) {
                    Ok(()) => {
                        let _ = sync_dir(&self.state_dir);
                        Ok(())
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                    Err(error) => Err(format!("Could not clear the Undo record: {error}")),
                }
            }
        }
    }

    /// Reverses whatever of a failed transaction is still ours and returns
    /// the error to report: `RolledBack` (journal removed) when everything
    /// was restored, otherwise `RollbackIncomplete` (journal kept).
    fn fail(
        &self,
        patch: &DesktopPatch,
        written: &[&ConfigKey],
        widget: Option<&WidgetWrite>,
        error: String,
    ) -> DesktopError {
        let fields = widget
            .map(|write| self.rollback_widget(write))
            .unwrap_or_default();
        let keys = self.rollback(patch, written);
        if keys.is_empty() && fields.is_empty() {
            let _ = fs::remove_file(self.journal_path());
            DesktopError::RolledBack(error)
        } else {
            DesktopError::RollbackIncomplete {
                error,
                keys,
                fields,
            }
        }
    }

    /// Restores the widget file's original bytes if it still holds ours;
    /// returns the settings that could not be restored.
    fn rollback_widget(&self, write: &WidgetWrite) -> Vec<String> {
        if let Some(design) = &write.design {
            let mut report = RecoveryReport::default();
            return match self.recover_design(&write.path, design, Some(&write.change), &mut report)
            {
                Ok(()) => report.kept_fields,
                Err(_) => write.change.labels(),
            };
        }
        match config::read_raw_config_text(&write.path) {
            Ok(current) if current == write.change.before => Vec::new(),
            Ok(current) if current == write.change.after => {
                match config::write_config_text(&write.path, write.change.before.as_deref()) {
                    Ok(()) => Vec::new(),
                    Err(_) => write.change.labels(),
                }
            }
            _ => write.change.labels(),
        }
    }

    /// Restores touched keys still holding our value; returns the keys that
    /// could not be restored.
    fn rollback(&self, patch: &DesktopPatch, written: &[&ConfigKey]) -> Vec<ConfigKey> {
        let mut unrestored = Vec::new();
        for key in written.iter().rev() {
            let Some(entry) = patch.get(key) else {
                continue;
            };
            match self.read_user(key) {
                Ok(current) if patch.equal(&current, &entry.original) => {}
                Ok(current) if patch.equal(&current, &entry.desired) => {
                    #[cfg(test)]
                    if matches!(&*self.fault.borrow(), Some(Fault::FailWriteAndRollback { rollback, .. }) if rollback == *key)
                    {
                        unrestored.push((*key).clone());
                        continue;
                    }
                    if self.write_key(key, entry.original.as_ref()).is_err() {
                        unrestored.push((*key).clone());
                    }
                }
                _ => unrestored.push((*key).clone()),
            }
        }
        unrestored.reverse();
        unrestored
    }

    /// The journal of an interrupted change, if any. Reads only. Version 1
    /// (keys only, also written by older releases), version 2 (with a
    /// widget part) and version 3 (an owned transaction, with the Undo
    /// record it found) are understood; anything else is explained and kept.
    pub fn pending_recovery(&self) -> Result<Option<Journal>, DesktopError> {
        let path = self.journal_path();
        let bytes = match read_record(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error("Could not read the change journal", error)),
        };
        let journal: Journal = serde_json::from_slice(&bytes).map_err(|error| {
            DesktopError::Io(format!(
                "The change journal {} is unreadable ({error}). Keep current settings to clear it.",
                path.display()
            ))
        })?;
        let widget_ok = journal
            .widget
            .as_ref()
            .is_none_or(|widget| widget.validate().is_ok());
        let design_ok = match &journal.design {
            Some(design) => journal.version == 4 && design.validate().is_ok(),
            None => journal.version != 4,
        };
        let supported = design_ok
            && journal.format == JOURNAL_FORMAT
            && widget_ok
            && match (journal.version, &journal.widget, &journal.undo) {
                (1, None, None) | (2, Some(_), None) | (4, Some(_), None) => true,
                // An Undo always started from a record; an Apply may not.
                (3 | 4, _, Some(undo)) => {
                    undo.record
                        .as_ref()
                        .is_none_or(|text| text.len() as u64 <= MAX_RECORD_BYTES)
                        && match journal.operation.as_str() {
                            "apply" | "restore" => true,
                            "migrate" | "repair_design" => {
                                journal.version == 4
                                    && journal.widget.is_none()
                                    && journal.entries.is_empty()
                            }
                            "undo" | "undo_restore" => undo.record.is_some(),
                            _ => false,
                        }
                }
                _ => false,
            };
        if !supported {
            return Err(DesktopError::Io(format!(
                "Unsupported change journal {}. Keep current settings to clear it.",
                path.display()
            )));
        }
        Ok(Some(journal))
    }

    /// Returns keys and widget settings from an interrupted change to their
    /// previous values, leaving those someone else changed afterwards
    /// untouched. Works after a restart at every point of the transaction.
    pub fn recover(&self) -> Result<RecoveryReport, DesktopError> {
        self.ensure_writable()?;
        // Same order as a transaction: configuration lock first.
        let _config_lock = self
            .app_config
            .as_deref()
            .map(config::lock_config)
            .transpose()
            .map_err(|error| {
                DesktopError::Io(format!("Could not lock widget settings: {error}"))
            })?;
        let _lock = self.lock()?;
        let Some(journal) = self.pending_recovery()? else {
            return Ok(RecoveryReport::default());
        };
        if matches!(journal.operation.as_str(), "restore" | "undo_restore") {
            // The journal can be durable before the original epoch write.
            // Every recovery attempt is a new conservative manual boundary.
            self.advance_restore_epoch()?;
        }
        super::recovery::ensure_locked(self, super::recovery::BaselineKind::CurrentDesktop)?;
        let entries = journal
            .entries
            .iter()
            .map(ChangeEntry::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let widget = match &journal.widget {
            Some(change) => Some((
                change,
                self.app_config.as_deref().ok_or_else(|| {
                    DesktopError::Io(
                        "This interrupted change also touched widget settings. Open Halcyon settings normally to restore it.".into(),
                    )
                })?,
            )),
            None => None,
        };
        if journal.undo.is_some() {
            self.confine(&self.undo_path())?;
        }
        let mut report = RecoveryReport::default();
        for ChangeEntry { key, before, after } in entries {
            let current = self.read_user(&key)?;
            if exact_equal(&current, &before) {
                report.unchanged.push(key);
            } else if exact_equal(&current, &after) {
                self.write_key(&key, before.as_ref())
                    .map_err(|error| io_error(format!("Could not restore {key}"), error))?;
                report.restored.push(key);
            } else {
                report.kept_external.push(key);
            }
        }
        if let Some(design) = &journal.design {
            let path = self.app_config.as_deref().ok_or_else(|| {
                DesktopError::Io(
                    "Open the original configuration to recover its widget design".into(),
                )
            })?;
            self.recover_design(path, design, journal.widget.as_ref(), &mut report)?;
        } else if let Some((change, path)) = widget {
            // An interrupted old-client journal must not write frozen bytes.
            if crate::design::storage::read(path)
                .map_err(DesktopError::Io)?
                .is_some()
            {
                return Err(DesktopError::Io(
                    "An older transaction needs review before the migrated design can be recovered"
                        .into(),
                ));
            }
            recover_widget(change, path, &mut report)?;
        }
        match &journal.undo {
            Some(found) => report.kept_undo = !self.recover_record(&journal, found)?,
            // An older journal does not know the record it replaced. The
            // change was reverted, so its own record (written just before an
            // interruption) no longer describes the desktop.
            None => {
                if self
                    .latest_change()
                    .ok()
                    .flatten()
                    .is_some_and(|latest| latest.id == journal.id)
                {
                    fs::remove_file(self.undo_path())
                        .map_err(|error| io_error("Could not clear the Undo record", error))?;
                }
            }
        }
        if matches!(journal.operation.as_str(), "restore" | "undo_restore") {
            self.acknowledge_recovered_restore(&journal.id)?;
        }
        fs::remove_file(self.journal_path())
            .map_err(|error| io_error("Could not clear the change journal", error))?;
        Ok(report)
    }

    /// Returns the Undo record to the state an interrupted owned transaction
    /// found it in, when it still holds what that transaction left: its own
    /// record after an Apply, none after an Undo. A record already as it was
    /// needs nothing. Anything else is newer and is kept (`false`).
    fn recover_record(&self, journal: &Journal, found: &UndoBefore) -> Result<bool, DesktopError> {
        let path = self.undo_path();
        let current = match read_record(&path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(_) => return Ok(false),
        };
        if current.as_deref() == found.record.as_deref().map(str::as_bytes) {
            return Ok(true);
        }
        let ours = match (journal.operation.as_str(), &current) {
            (
                "apply" | "undo" | "restore" | "undo_restore" | "migrate" | "repair_design",
                Some(bytes),
            ) => serde_json::from_slice::<undo::UndoFile>(bytes).is_ok_and(|file| {
                file.format == undo::FORMAT
                    && (file.transaction_id.as_deref() == Some(&journal.id)
                        || file.id == journal.id)
            }),
            ("undo" | "undo_restore", None) => true,
            _ => false,
        };
        if !ours {
            return Ok(false);
        }
        match &found.record {
            Some(text) => atomic_write(&path, text.as_bytes(), Some(0o600)),
            None => fs::remove_file(&path).and_then(|()| sync_dir(&self.state_dir)),
        }
        .map_err(|error| io_error("Could not restore the Undo record", error))?;
        Ok(true)
    }

    /// Keeps the current values (and the Undo record as it now is) and
    /// clears the journal.
    pub fn discard_recovery(&self) -> Result<(), DesktopError> {
        self.ensure_writable()?;
        let _config_lock = self
            .app_config
            .as_deref()
            .map(config::lock_config)
            .transpose()
            .map_err(DesktopError::Io)?;
        let _lock = self.lock()?;
        let design_pending = match self.pending_recovery() {
            Ok(journal) => journal.is_some_and(|j| j.version == 4),
            Err(_) => self
                .app_config
                .as_deref()
                .is_some_and(crate::design::storage::managed),
        };
        if design_pending {
            return Err(DesktopError::Io(
                "Recover the interrupted widget design change before continuing".into(),
            ));
        }
        match fs::remove_file(self.journal_path()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error("Could not clear the change journal", error)),
        }
    }

    /// A fixture never writes outside its root. The deepest existing
    /// ancestor of `path` must resolve inside the resolved root, so a
    /// symlinked `config`, namespace or state directory pointing elsewhere is
    /// refused before anything is created; an existing symlink leaf is
    /// refused too. Live stores write the real configuration by design.
    fn confine(&self, path: &Path) -> Result<(), DesktopError> {
        let Access::Fixture(root) = &self.access else {
            return Ok(());
        };
        let escape = || {
            DesktopError::Io(format!(
                "{} is outside the fixture root {}; nothing was changed",
                path.display(),
                root.display()
            ))
        };
        let resolved_root = fs::canonicalize(root)
            .map_err(|error| io_error(format!("Fixture root {}", root.display()), error))?;
        if !path.starts_with(root) {
            return Err(escape());
        }
        let mut existing = path;
        loop {
            match fs::symlink_metadata(existing) {
                Ok(metadata) => {
                    if existing == path && metadata.file_type().is_symlink() {
                        return Err(escape());
                    }
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    existing = existing.parent().ok_or_else(escape)?;
                }
                Err(error) => {
                    return Err(io_error(
                        format!("Could not check {}", path.display()),
                        error,
                    ));
                }
            }
        }
        let resolved = fs::canonicalize(existing)
            .map_err(|error| io_error(format!("Could not check {}", path.display()), error))?;
        if resolved.starts_with(&resolved_root) {
            Ok(())
        } else {
            Err(escape())
        }
    }

    fn lock(&self) -> Result<fs::File, DesktopError> {
        self.confine(&self.state_dir.join(LOCK))?;
        let lock = super::recovery::open_lock_file(&self.state_dir.join(LOCK))
            .map_err(|error| io_error("Could not lock desktop settings", error))?;
        loop {
            // SAFETY: `lock` owns a valid descriptor for the guard's lifetime.
            if unsafe { libc::flock(lock.as_raw_fd(), config::settings_lock_operation()) } == 0 {
                return Ok(lock);
            }
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::WouldBlock {
                return Err(DesktopError::Io(
                    "Desktop settings are busy in another process. Try again when it finishes."
                        .into(),
                ));
            }
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(io_error("Could not lock desktop settings", error));
            }
        }
    }

    fn write_key(&self, key: &ConfigKey, value: Option<&RawValue>) -> io::Result<()> {
        #[cfg(test)]
        if matches!(&*self.fault.borrow(), Some(Fault::FailWrite(failing) | Fault::FailWriteAndRollback { write: failing, .. }) if failing == key)
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected failure",
            ));
        }
        let path = self.user_path(key);
        // Defense in depth on top of ConfigKey validation.
        if !path.starts_with(&self.user_root) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path escapes the config root",
            ));
        }
        self.confine(&path)
            .map_err(|error| io::Error::new(io::ErrorKind::PermissionDenied, error.to_string()))?;
        super::recovery::write_native(&path, value.map(|v| v.as_str().as_bytes()))
            .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))
    }

    #[cfg(test)]
    pub(crate) fn inject(&self, fault: Option<Fault>) {
        *self.fault.borrow_mut() = fault;
    }
}

fn exact_equal(a: &Option<RawValue>, b: &Option<RawValue>) -> bool {
    a.as_ref().map(RawValue::as_str) == b.as_ref().map(RawValue::as_str)
}

fn read_value(path: &Path) -> Result<Option<RawValue>, DesktopError> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {
            if metadata.len() > MAX_VALUE_BYTES as u64 {
                return Err(DesktopError::Io(format!("{} is too large", path.display())));
            }
            fs::read_to_string(path)
                .map(|text| Some(RawValue::from_disk(text)))
                .map_err(|error| io_error(format!("Could not read {}", path.display()), error))
        }
        Ok(_) => Err(DesktopError::Io(format!(
            "{} is not a regular file",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) if error.kind() == io::ErrorKind::NotADirectory => Ok(None),
        Err(error) => Err(io_error(
            format!("Could not read {}", path.display()),
            error,
        )),
    }
}

/// A transaction's outcome before it becomes an `AppliedChange` or a
/// `LatestChange`.
struct Committed {
    change: AppliedChange,
    widget: Option<WidgetChange>,
    created: String,
}

impl Committed {
    fn empty(id: String) -> Self {
        Self {
            change: AppliedChange {
                id,
                entries: Vec::new(),
            },
            widget: None,
            created: chrono::Local::now().to_rfc3339(),
        }
    }
}

/// Reads a journal or Undo record, refusing oversized files.
fn read_record(path: &Path) -> io::Result<Vec<u8>> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > MAX_RECORD_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is not a usable record", path.display()),
        ));
    }
    fs::read(path)
}

/// The widget part of `recover`: exact original bytes when the file is
/// still exactly ours, otherwise member by member.
fn recover_widget(
    change: &WidgetChange,
    path: &Path,
    report: &mut RecoveryReport,
) -> Result<(), DesktopError> {
    let current = config::read_raw_config_text(path).map_err(DesktopError::Io)?;
    let written = || {
        change
            .fields
            .iter()
            .filter(|field| field.before != field.after)
            .map(|field| widget::label(&field.pointer))
    };
    if current == change.after {
        config::write_config_text(path, change.before.as_deref()).map_err(|error| {
            DesktopError::Io(format!("Could not restore widget settings: {error}"))
        })?;
        report.restored_fields.extend(written());
        return Ok(());
    }
    if current == change.before {
        return Ok(());
    }
    let mut value = config::parse_config_text(current.as_deref()).map_err(|error| {
        DesktopError::Io(format!(
            "Widget settings are unreadable ({error}); nothing was restored."
        ))
    })?;
    let mut restored = false;
    for field in &change.fields {
        let now = widget::get(&value, &field.pointer);
        if field.before == field.after || now == field.before {
            continue;
        }
        if now == field.after {
            widget::put(&mut value, &field.pointer, field.before.clone())
                .map_err(DesktopError::Io)?;
            report.restored_fields.push(widget::label(&field.pointer));
            restored = true;
        } else {
            report.kept_fields.push(widget::label(&field.pointer));
        }
    }
    if restored {
        config::check_config_value(&value).map_err(|message| DesktopError::InvalidValue {
            key: None,
            message: format!("Widget settings: {message}"),
        })?;
        let text = config::config_text(&value).map_err(DesktopError::Io)?;
        config::write_config_text(path, Some(&text)).map_err(|error| {
            DesktopError::Io(format!("Could not restore widget settings: {error}"))
        })?;
    }
    Ok(())
}

fn sync_dir(path: &Path) -> io::Result<()> {
    fs::File::open(path)?.sync_all()
}

/// Same-directory temp file + fsync + rename. The `.atomicwrite` prefix is
/// the one cosmic-config watchers already ignore.
fn atomic_write(path: &Path, bytes: &[u8], mode: Option<u32>) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no parent directory"))?;
    fs::create_dir_all(parent)?;
    let mode = mode.unwrap_or_else(|| {
        fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o777)
            .unwrap_or(0o644)
    });
    let temporary = parent.join(format!(
        ".atomicwrite.desktop-cards.{}.{}",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&temporary)?;
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        sync_dir(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod transaction_tests;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A temporary fixture root, removed on drop.
    pub(crate) struct Fixture(pub PathBuf);
    impl Fixture {
        pub(crate) fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "desktop-cards-store-{name}-{}-{}",
                std::process::id(),
                NEXT_FILE.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        pub(crate) fn store(&self) -> DesktopStore {
            DesktopStore::with_roots(
                self.0.join("config/cosmic"),
                vec![self.0.join("system/cosmic")],
                self.0.join("state"),
                Access::Fixture(self.0.clone()),
            )
        }
        pub(crate) fn user(&self, namespace: &str, key: &str, value: &str) {
            let path = self
                .0
                .join("config/cosmic")
                .join(namespace)
                .join("v1")
                .join(key);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, value).unwrap();
        }
        pub(crate) fn system(&self, namespace: &str, key: &str, value: &str) {
            let path = self
                .0
                .join("system/cosmic")
                .join(namespace)
                .join("v1")
                .join(key);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, value).unwrap();
        }
        pub(crate) fn read(&self, namespace: &str, key: &str) -> Option<String> {
            fs::read_to_string(
                self.0
                    .join("config/cosmic")
                    .join(namespace)
                    .join("v1")
                    .join(key),
            )
            .ok()
        }
        /// Every path below the root, with file contents; directories map to None.
        pub(crate) fn tree(&self) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
            tree(&self.0)
        }
    }

    /// Every path below `root`, with file contents; directories map to None.
    pub(crate) fn tree(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
        fn walk(path: &Path, result: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    result.insert(path.clone(), None);
                    walk(&path, result);
                } else {
                    result.insert(path.clone(), Some(fs::read(&path).unwrap()));
                }
            }
        }
        let mut result = BTreeMap::new();
        walk(root, &mut result);
        result
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const DOCK: &str = "com.system76.CosmicPanel.Dock";
    const PANEL: &str = "com.system76.CosmicPanel.Panel";

    fn key(namespace: &str, name: &str) -> ConfigKey {
        ConfigKey::new(namespace, 1, name).unwrap()
    }
    fn raw(text: &str) -> RawValue {
        RawValue::parse(text).unwrap()
    }
    fn seeded(name: &str) -> Fixture {
        let fixture = Fixture::new(name);
        fixture.system(DOCK, "anchor", "Bottom\n");
        fixture.system(DOCK, "size", "L\n");
        fixture.system(DOCK, "opacity", "1.0\n");
        fixture.user(DOCK, "anchor", "Bottom");
        fixture.user(DOCK, "size", "S");
        fixture.user(DOCK, "opacity", "0.25");
        fixture.user(DOCK, "plugins_wings.before-agent-hub-v2", "Some(([], []))");
        fixture.user(PANEL, "anchor", "Left");
        fixture.user("com.system76.CosmicComp", "cursor_shake_to_find", "true");
        fixture
    }

    #[test]
    fn loading_reads_both_layers_and_writes_nothing() {
        let fixture = seeded("load");
        let store = fixture.store();
        let before = fixture.tree();
        let keys = [
            key(DOCK, "anchor"),
            key(DOCK, "size"),
            key(DOCK, "padding"),
            key("com.system76.CosmicPanel.Missing", "anchor"),
        ];
        let snapshot = store.load(&keys).unwrap();
        assert_eq!(snapshot.user(&keys[1]), Some(&raw("S")));
        assert_eq!(snapshot.entry(&keys[1]).unwrap().system, Some(raw("L")));
        assert_eq!(snapshot.effective(&keys[1]), Some(&raw("S")));
        assert_eq!(snapshot.effective(&keys[2]), None);
        assert_eq!(snapshot.effective(&keys[3]), None);
        assert_eq!(store.pending_recovery().unwrap(), None);
        // No state directory, namespace directory or lock file appears.
        assert_eq!(fixture.tree(), before);
        assert!(!fixture.0.join("state").exists());
    }

    #[test]
    fn two_key_apply_preserves_third_key_unknown_files_and_other_namespaces() {
        let fixture = seeded("apply");
        let store = fixture.store();
        let keys = [key(DOCK, "anchor"), key(DOCK, "size"), key(DOCK, "opacity")];
        let snapshot = store.load(&keys).unwrap();
        let mut patch = DesktopPatch::new();
        patch.set(
            keys[0].clone(),
            snapshot.user(&keys[0]).cloned(),
            Some(raw("Top")),
        );
        patch.set(
            keys[1].clone(),
            snapshot.user(&keys[1]).cloned(),
            Some(raw("M")),
        );
        // A no-op entry is dropped rather than rewritten.
        patch.set(
            keys[2].clone(),
            snapshot.user(&keys[2]).cloned(),
            Some(raw("0.25")),
        );
        assert_eq!(patch.len(), 2);
        let mut before = fixture.tree();
        let change = store.apply(&patch).unwrap();
        assert_eq!(fixture.read(DOCK, "anchor").as_deref(), Some("Top"));
        assert_eq!(fixture.read(DOCK, "size").as_deref(), Some("M"));
        assert_eq!(change.entries().len(), 2);
        assert_eq!(change.entries()[0].before, Some(raw("Bottom")));
        let after = fixture.tree();
        for path in [
            fixture.0.join("config/cosmic").join(DOCK).join("v1/anchor"),
            fixture.0.join("config/cosmic").join(DOCK).join("v1/size"),
        ] {
            before.remove(&path);
        }
        for (path, contents) in &before {
            // Third key, backup-like unknown file, other panel and other
            // namespaces are byte-identical; no temp or journal remains.
            if !path.starts_with(fixture.0.join("state")) {
                assert_eq!(after.get(path), Some(contents), "{}", path.display());
            }
        }
        assert!(
            after
                .keys()
                .all(|path| !path.to_string_lossy().contains("atomicwrite"))
        );
        assert!(!fixture.0.join("state").join(JOURNAL).exists());
    }

    #[test]
    fn external_edit_to_touched_key_rejects_whole_patch_untouched_edit_survives() {
        let fixture = seeded("conflict");
        let store = fixture.store();
        let keys = [key(DOCK, "anchor"), key(DOCK, "size"), key(DOCK, "opacity")];
        let snapshot = store.load(&keys).unwrap();
        let mut patch = DesktopPatch::new();
        patch.set(
            keys[0].clone(),
            snapshot.user(&keys[0]).cloned(),
            Some(raw("Top")),
        );
        patch.set(
            keys[1].clone(),
            snapshot.user(&keys[1]).cloned(),
            Some(raw("XL")),
        );
        // Native settings change an untouched key: it must survive Apply.
        fixture.user(DOCK, "opacity", "0.6");
        store.apply(&patch).unwrap();
        assert_eq!(fixture.read(DOCK, "opacity").as_deref(), Some("0.6"));

        let snapshot = store.load(&keys).unwrap();
        let mut patch = DesktopPatch::new();
        patch.set(
            keys[0].clone(),
            snapshot.user(&keys[0]).cloned(),
            Some(raw("Left")),
        );
        patch.set(
            keys[1].clone(),
            snapshot.user(&keys[1]).cloned(),
            Some(raw("XS")),
        );
        // Native settings change a touched key: nothing at all is written.
        fixture.user(DOCK, "size", "L");
        let before = fixture.tree();
        assert_eq!(
            store.apply(&patch),
            Err(DesktopError::Conflict(vec![keys[1].clone()]))
        );
        let after = fixture.tree();
        assert_eq!(
            before
                .iter()
                .filter(|(path, _)| !path.starts_with(fixture.0.join("state")))
                .collect::<Vec<_>>(),
            after
                .iter()
                .filter(|(path, _)| !path.starts_with(fixture.0.join("state")))
                .collect::<Vec<_>>()
        );
        assert_eq!(fixture.read(DOCK, "anchor").as_deref(), Some("Top"));
    }

    #[test]
    fn absent_keys_are_distinct_and_undo_restores_absence() {
        let fixture = seeded("absent");
        let store = fixture.store();
        let padding = key(DOCK, "padding");
        let anchor = key(DOCK, "anchor");
        let snapshot = store.load(&[padding.clone(), anchor.clone()]).unwrap();
        assert_eq!(snapshot.user(&padding), None);
        let mut patch = DesktopPatch::new();
        patch.set(padding.clone(), None, Some(raw("8")));
        patch.set(
            anchor.clone(),
            snapshot.user(&anchor).cloned(),
            Some(raw("Right")),
        );
        let change = store.apply(&patch).unwrap();
        assert_eq!(fixture.read(DOCK, "padding").as_deref(), Some("8"));
        assert_eq!(
            change
                .entries()
                .iter()
                .find(|e| e.key == padding)
                .unwrap()
                .before,
            None
        );
        store.undo(&change).unwrap();
        assert!(!store.user_path(&padding).exists());
        assert_eq!(fixture.read(DOCK, "anchor").as_deref(), Some("Bottom"));
        // An explicit patch can also return a key to the system default.
        let mut patch = DesktopPatch::new();
        patch.set(anchor.clone(), Some(raw("Bottom")), None);
        store.apply(&patch).unwrap();
        assert!(!store.user_path(&anchor).exists());
        assert_eq!(
            store.load(&[anchor.clone()]).unwrap().effective(&anchor),
            Some(&raw("Bottom"))
        );
    }

    #[test]
    fn partial_write_failure_restores_touched_originals_and_clears_journal() {
        let fixture = seeded("partial");
        let store = fixture.store();
        let keys = [key(DOCK, "anchor"), key(DOCK, "padding"), key(DOCK, "size")];
        let snapshot = store.load(&keys).unwrap();
        let mut patch = DesktopPatch::new();
        for (key, value) in keys.iter().zip(["Top", "6", "XL"]) {
            patch.set(key.clone(), snapshot.user(key).cloned(), Some(raw(value)));
        }
        let before = fixture.tree();
        // BTreeMap order: anchor, padding, size. The last write fails after
        // the first two keys (one of them previously absent) were written.
        store.inject(Some(Fault::FailWrite(keys[2].clone())));
        let error = store.apply(&patch).unwrap_err();
        assert!(matches!(error, DesktopError::RolledBack(_)), "{error:?}");
        assert!(error.to_string().contains("previous settings restored"));
        let after = fixture.tree();
        let config = |tree: &BTreeMap<PathBuf, Option<Vec<u8>>>| {
            tree.iter()
                .filter(|(path, contents)| {
                    contents.is_some() && path.starts_with(fixture.0.join("config"))
                })
                .map(|(path, contents)| (path.clone(), contents.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(config(&before), config(&after));
        assert!(!store.user_path(&keys[1]).exists());
        assert_eq!(store.pending_recovery().unwrap(), None);
    }

    #[test]
    fn failed_rollback_keeps_the_journal_and_recovery_keeps_newer_edits() {
        let fixture = seeded("rollback-incomplete");
        let store = fixture.store();
        let keys = [key(DOCK, "anchor"), key(DOCK, "size")];
        let snapshot = store.load(&keys).unwrap();
        let mut patch = DesktopPatch::new();
        for (key, value) in keys.iter().zip(["Top", "XL"]) {
            patch.set(key.clone(), snapshot.user(key).cloned(), Some(raw(value)));
        }
        // anchor is written, size fails, and restoring anchor fails too.
        store.inject(Some(Fault::FailWriteAndRollback {
            write: keys[1].clone(),
            rollback: keys[0].clone(),
        }));
        let error = store.apply(&patch).unwrap_err();
        assert!(
            matches!(&error, DesktopError::RollbackIncomplete { keys: stuck, .. } if stuck == &keys[..1]),
            "{error:?}"
        );
        assert!(error.to_string().contains("Use Restore to retry"));
        assert_eq!(fixture.read(DOCK, "anchor").as_deref(), Some("Top"));
        let journal = store.pending_recovery().unwrap().expect("journal kept");
        assert_eq!(journal.entries.len(), 2);
        assert_eq!(store.apply(&patch), Err(DesktopError::RecoveryPending));
        // A newer external edit to the key that never got our value survives.
        fixture.user(DOCK, "size", "M");
        store.inject(None);
        let report = store.recover().unwrap();
        assert_eq!(report.restored, keys[..1]);
        assert_eq!(report.kept_external, keys[1..]);
        assert_eq!(fixture.read(DOCK, "anchor").as_deref(), Some("Bottom"));
        assert_eq!(fixture.read(DOCK, "size").as_deref(), Some("M"));
        assert_eq!(store.pending_recovery().unwrap(), None);
    }

    #[test]
    fn real_permission_failure_rolls_back_the_other_namespace() {
        let fixture = seeded("permission");
        let store = fixture.store();
        let dock = key(DOCK, "anchor");
        let panel = key(PANEL, "anchor");
        let snapshot = store.load(&[dock.clone(), panel.clone()]).unwrap();
        let mut patch = DesktopPatch::new();
        patch.set(
            dock.clone(),
            snapshot.user(&dock).cloned(),
            Some(raw("Top")),
        );
        patch.set(
            panel.clone(),
            snapshot.user(&panel).cloned(),
            Some(raw("Right")),
        );
        let locked = store.user_path(&panel).parent().unwrap().to_path_buf();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
        let probe = locked.join("probe");
        if fs::write(&probe, "").is_ok() {
            // Running with elevated rights; the injected-failure test covers this.
            let _ = fs::remove_file(probe);
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
            return;
        }
        let result = store.apply(&patch);
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            matches!(result, Err(DesktopError::RolledBack(_))),
            "{result:?}"
        );
        assert_eq!(fixture.read(DOCK, "anchor").as_deref(), Some("Bottom"));
        assert_eq!(fixture.read(PANEL, "anchor").as_deref(), Some("Left"));
    }

    #[test]
    fn undo_refuses_to_overwrite_a_later_external_change() {
        let fixture = seeded("undo");
        let store = fixture.store();
        let keys = [key(DOCK, "anchor"), key(DOCK, "size")];
        let snapshot = store.load(&keys).unwrap();
        let mut patch = DesktopPatch::new();
        patch.set(
            keys[0].clone(),
            snapshot.user(&keys[0]).cloned(),
            Some(raw("Top")),
        );
        patch.set(
            keys[1].clone(),
            snapshot.user(&keys[1]).cloned(),
            Some(raw("XL")),
        );
        let change = store.apply(&patch).unwrap();
        fixture.user(DOCK, "size", "M");
        assert_eq!(
            store.undo(&change),
            Err(DesktopError::Conflict(vec![keys[1].clone()]))
        );
        assert_eq!(fixture.read(DOCK, "anchor").as_deref(), Some("Top"));
        assert_eq!(fixture.read(DOCK, "size").as_deref(), Some("M"));
    }

    #[test]
    fn alternate_config_without_fixture_is_read_only_and_writes_nothing() {
        assert_eq!(choose_root(true, None), RootChoice::ReadOnlyLive);
        assert_eq!(choose_root(false, None), RootChoice::Live);
        let root = PathBuf::from("/tmp/fixture");
        assert_eq!(
            choose_root(true, Some(root.clone())),
            RootChoice::Fixture(root)
        );
        assert!(DesktopStore::fixture(Path::new("relative/root")).is_err());
        assert!(DesktopStore::fixture(Path::new("/nonexistent/desktop-cards-root")).is_err());

        let fixture = seeded("readonly");
        let store = fixture.store().read_only(PREVIEW_REASON);
        let anchor = key(DOCK, "anchor");
        let snapshot = store.load(&[anchor.clone()]).unwrap();
        let mut patch = DesktopPatch::new();
        patch.set(
            anchor.clone(),
            snapshot.user(&anchor).cloned(),
            Some(raw("Top")),
        );
        let before = fixture.tree();
        assert!(matches!(
            store.apply(&patch),
            Err(DesktopError::ReadOnly(_))
        ));
        assert!(matches!(store.recover(), Err(DesktopError::ReadOnly(_))));
        assert!(matches!(
            store.discard_recovery(),
            Err(DesktopError::ReadOnly(_))
        ));
        assert_eq!(fixture.tree(), before);
    }

    #[test]
    fn malformed_values_and_path_escapes_never_reach_writes() {
        for (namespace, name) in [
            ("../etc", "passwd"),
            ("com.system76.CosmicPanel.Dock", "../../x"),
            ("com.system76.CosmicPanel..Dock", "anchor"),
            ("/etc", "anchor"),
            ("com.system76.CosmicPanel.Dock", ".hidden"),
            ("com.system76.CosmicPanel.Dock", "a/b"),
            ("com.system76.CosmicPanel.Dock", ""),
            ("com.system76.CosmicPanel.Dock", ".atomicwrite.x"),
        ] {
            assert!(
                ConfigKey::new(namespace, 1, name).is_err(),
                "{namespace} {name}"
            );
        }
        assert!(ConfigKey::new(DOCK, 0, "anchor").is_err());
        assert!(RawValue::parse("Some((").is_err());

        let fixture = seeded("malformed");
        let store = fixture.store();
        let before = fixture.tree();
        let anchor = key(DOCK, "anchor");
        let attempts = [
            // Syntactically broken text that bypassed RawValue::parse.
            (anchor.clone(), RawValue::from_disk("Top(".into())),
            // Valid RON with the wrong type or an unknown variant.
            (anchor.clone(), raw("\"Top\"")),
            (anchor.clone(), raw("Sideways")),
            (key(DOCK, "opacity"), raw("7.5")),
            // A well-formed key outside the whitelist.
            (
                key("com.system76.CosmicComp", "cursor_shake_to_find"),
                raw("false"),
            ),
            (key("com.system76.CosmicPanel", "entries"), raw("[]")),
            (
                key("com.system76.CosmicPanel.Dock-x.y", "anchor"),
                raw("Top"),
            ),
        ];
        for (key, value) in attempts {
            let mut patch = DesktopPatch::new();
            patch.set(key.clone(), store.read_user(&key).unwrap(), Some(value));
            let error = store.apply(&patch).unwrap_err();
            assert!(
                matches!(
                    error,
                    DesktopError::InvalidValue { .. } | DesktopError::NotAllowed(_)
                ),
                "{key}: {error:?}"
            );
        }
        assert_eq!(fixture.tree(), before);
    }

    #[test]
    fn a_key_changed_elsewhere_mid_transaction_is_never_overwritten() {
        let fixture = seeded("changed-elsewhere");
        let store = fixture.store();
        let keys = [key(DOCK, "anchor"), key(DOCK, "padding"), key(DOCK, "size")];
        let snapshot = store.load(&keys).unwrap();
        let mut patch = DesktopPatch::new();
        for (key, value) in keys.iter().zip(["Top", "6", "XL"]) {
            patch.set(key.clone(), snapshot.user(key).cloned(), Some(raw(value)));
        }
        let anchor = fixture.read(DOCK, "anchor");
        // COSMIC Settings saves the padding after the check, before its write.
        store.inject(Some(Fault::ChangedElsewhereBefore(1, "12")));
        assert_eq!(
            store.apply(&patch),
            Err(DesktopError::Conflict(vec![keys[1].clone()]))
        );
        store.inject(None);
        assert_eq!(
            fixture.read(DOCK, "padding").as_deref(),
            Some("12"),
            "theirs stays"
        );
        assert_eq!(fixture.read(DOCK, "anchor"), anchor, "ours is put back");
        assert_eq!(fixture.read(DOCK, "size").as_deref(), Some("S"));
        assert_eq!(store.pending_recovery().unwrap(), None);
    }

    #[test]
    fn interrupted_transaction_is_recovered_from_its_journal() {
        let fixture = seeded("recover");
        let store = fixture.store();
        let keys = [key(DOCK, "anchor"), key(DOCK, "padding"), key(DOCK, "size")];
        let snapshot = store.load(&keys).unwrap();
        let mut patch = DesktopPatch::new();
        for (key, value) in keys.iter().zip(["Top", "6", "XL"]) {
            patch.set(key.clone(), snapshot.user(key).cloned(), Some(raw(value)));
        }
        // Stop after two writes, as if the process died: no rollback ran.
        store.inject(Some(Fault::InterruptAfter(2)));
        assert!(store.apply(&patch).is_err());
        store.inject(None);
        assert_eq!(fixture.read(DOCK, "anchor").as_deref(), Some("Top"));
        assert_eq!(fixture.read(DOCK, "padding").as_deref(), Some("6"));
        assert_eq!(fixture.read(DOCK, "size").as_deref(), Some("S"));
        let journal = store.pending_recovery().unwrap().unwrap();
        assert_eq!(journal.entries.len(), 3);
        assert_eq!(journal.entries[1].before, None);
        // Further changes wait for the user's decision.
        let mut other = DesktopPatch::new();
        other.set(key(DOCK, "opacity"), Some(raw("0.25")), Some(raw("0.5")));
        assert_eq!(store.apply(&other), Err(DesktopError::RecoveryPending));
        // Someone changes the first key after the crash: recovery keeps it.
        fixture.user(DOCK, "anchor", "Left");
        let report = store.recover().unwrap();
        assert_eq!(report.restored, vec![keys[1].clone()]);
        assert_eq!(report.unchanged, vec![keys[2].clone()]);
        assert_eq!(report.kept_external, vec![keys[0].clone()]);
        assert!(!store.user_path(&keys[1]).exists());
        assert_eq!(fixture.read(DOCK, "anchor").as_deref(), Some("Left"));
        assert_eq!(store.pending_recovery().unwrap(), None);
        store.apply(&other).unwrap();
    }

    #[test]
    fn keeping_current_values_clears_an_unreadable_journal() {
        let fixture = seeded("discard");
        let store = fixture.store();
        fs::create_dir_all(fixture.0.join("state")).unwrap();
        fs::write(fixture.0.join("state").join(JOURNAL), "{not json").unwrap();
        assert!(store.pending_recovery().is_err());
        store.discard_recovery().unwrap();
        assert_eq!(store.pending_recovery().unwrap(), None);
        // A forged journal naming a non-whitelisted key is refused.
        let journal = Journal {
            format: JOURNAL_FORMAT.into(),
            version: 1,
            id: "x".into(),
            operation: "apply".into(),
            created: String::new(),
            entries: vec![JournalEntry {
                namespace: "com.system76.CosmicComp".into(),
                version: 1,
                key: "cursor_shake_to_find".into(),
                before: Some("true".into()),
                after: Some("false".into()),
            }],
            widget: None,
            design: None,
            undo: None,
        };
        fs::write(
            fixture.0.join("state").join(JOURNAL),
            serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();
        assert!(matches!(store.recover(), Err(DesktopError::NotAllowed(_))));
        assert_eq!(
            fixture
                .read("com.system76.CosmicComp", "cursor_shake_to_find")
                .as_deref(),
            Some("true")
        );
    }

    #[test]
    fn guards_reject_external_changes_to_unwritten_inputs_without_writing_them() {
        let fixture = seeded("guards");
        let store = fixture.store();
        let keys = [key(DOCK, "anchor"), key(DOCK, "size"), key(DOCK, "padding")];
        let snapshot = store.load(&keys).unwrap();
        let mut patch = DesktopPatch::new();
        patch.set(
            keys[0].clone(),
            snapshot.user(&keys[0]).cloned(),
            Some(raw("Top")),
        );
        patch.guard(keys[1].clone(), snapshot.user(&keys[1]).cloned());
        // An absent input is guarded as absent; a guard never replaces an entry.
        patch.guard(keys[2].clone(), None);
        patch.guard(keys[0].clone(), Some(raw("Left")));
        assert_eq!(patch.len(), 1);
        assert_eq!(patch.guards().count(), 2);
        fixture.user(DOCK, "padding", "6");
        let config = |fixture: &Fixture| {
            let mut tree = fixture.tree();
            tree.retain(|path, _| !path.starts_with(fixture.0.join("state")));
            tree
        };
        let before = config(&fixture);
        assert_eq!(
            store.apply(&patch),
            Err(DesktopError::Conflict(vec![keys[2].clone()]))
        );
        assert_eq!(config(&fixture), before);
        // Back at its guarded value, the patch applies; guards are not written
        // and not part of the change Undo restores.
        fs::remove_file(store.user_path(&keys[2])).unwrap();
        let change = store.apply(&patch).unwrap();
        assert_eq!(change.entries().len(), 1);
        assert_eq!(fixture.read(DOCK, "size").as_deref(), Some("S"));
        assert!(!store.user_path(&keys[2]).exists());
        // Extending keeps written entries ahead of the other patch's guards.
        let mut other = DesktopPatch::new();
        other.guard(keys[0].clone(), None);
        other.set(keys[1].clone(), Some(raw("S")), Some(raw("M")));
        let mut merged = patch.clone();
        merged.extend(other);
        assert_eq!(merged.len(), 2);
        assert!(merged.guards().all(|(key, _)| *key == keys[2]));
    }

    /// Two separate temporary roots: the fixture and a fake live tree that a
    /// symlink inside the fixture tries to reach. The fake tree must stay
    /// byte-identical and nothing may be created in either root.
    #[test]
    fn fixture_symlinks_that_escape_the_root_are_refused_before_any_write() {
        use std::os::unix::fs::symlink;
        let anchor = key(DOCK, "anchor");
        let mut patch = DesktopPatch::new();
        patch.set(anchor.clone(), Some(raw("Bottom")), Some(raw("Top")));
        let live = seeded("escape-live");
        let cases: [(&str, fn(&Fixture, &Fixture)); 4] = [
            ("config", |fixture, live| {
                symlink(live.0.join("config"), fixture.0.join("config")).unwrap();
            }),
            ("namespace", |fixture, live| {
                let parent = fixture.0.join("config/cosmic");
                fs::create_dir_all(&parent).unwrap();
                symlink(live.0.join("config/cosmic").join(DOCK), parent.join(DOCK)).unwrap();
            }),
            ("leaf", |fixture, live| {
                let parent = fixture.0.join("config/cosmic").join(DOCK).join("v1");
                fs::create_dir_all(&parent).unwrap();
                symlink(
                    live.0.join("config/cosmic").join(DOCK).join("v1/anchor"),
                    parent.join("anchor"),
                )
                .unwrap();
            }),
            ("state", |fixture, live| {
                fs::create_dir_all(live.0.join("state")).unwrap();
                fixture.user(DOCK, "anchor", "Bottom");
                symlink(live.0.join("state"), fixture.0.join("state")).unwrap();
            }),
        ];
        for (name, link) in cases {
            let fixture = Fixture::new(&format!("escape-{name}"));
            link(&fixture, &live);
            let live_before = live.tree();
            let fixture_before = fixture.tree();
            let store = fixture.store();
            let error = store.apply(&patch).unwrap_err();
            assert!(
                error.to_string().contains("outside the fixture root"),
                "{name}: {error:?}"
            );
            if name == "state" {
                // Recovery locks first, so it is refused the same way.
                assert!(matches!(store.recover(), Err(DesktopError::Io(_))));
                assert!(matches!(store.discard_recovery(), Err(DesktopError::Io(_))));
            }
            assert_eq!(live.tree(), live_before, "{name}: fake live tree changed");
            assert_eq!(fixture.tree(), fixture_before, "{name}: fixture changed");
        }
        // A fixture whose directories do not exist yet still works and stays
        // inside its own root.
        let fresh = Fixture::new("escape-fresh");
        let store = fresh.store();
        let mut create = DesktopPatch::new();
        create.set(anchor.clone(), None, Some(raw("Top")));
        let before = live.tree();
        store.apply(&create).unwrap();
        assert_eq!(fresh.read(DOCK, "anchor").as_deref(), Some("Top"));
        assert_eq!(live.tree(), before);
    }

    #[test]
    fn preflight_and_state_files_refuse_without_creating_anything() {
        let fixture = Fixture::new("preflight");
        let store = fixture.store();
        for name in ["", "../escape", "a/b", ".hidden", LOCK, JOURNAL] {
            assert!(store.state_file(name).is_err(), "{name:?}");
        }
        assert_eq!(
            store.state_file("legacy.lock").unwrap(),
            fixture.0.join("state/legacy.lock")
        );
        let mut patch = DesktopPatch::new();
        patch.set(key(DOCK, "anchor"), None, Some(raw("Top")));
        store.preflight(&patch).unwrap();
        let mut foreign = DesktopPatch::new();
        foreign.set(key("org.example.Other", "anchor"), None, Some(raw("Top")));
        assert!(matches!(
            store.preflight(&foreign),
            Err(DesktopError::NotAllowed(_))
        ));
        let preview = fixture.store().read_only("preview");
        assert!(matches!(
            preview.preflight(&patch),
            Err(DesktopError::ReadOnly(_))
        ));
        assert!(matches!(
            preview.state_file("legacy.lock"),
            Err(DesktopError::ReadOnly(_))
        ));
        assert!(fixture.tree().is_empty());
    }

    #[test]
    fn system_defaults_follow_xdg_precedence() {
        let roots = system_roots_from(
            Some(PathBuf::from("/home/u/.local/share")),
            Some("/usr/local/share/:relative:/usr/share/:/usr/share".into()),
        );
        assert_eq!(
            roots,
            [
                PathBuf::from("/home/u/.local/share/cosmic"),
                PathBuf::from("/usr/local/share/cosmic"),
                PathBuf::from("/usr/share/cosmic"),
            ]
        );
        assert_eq!(
            system_roots_from(None, None),
            [
                PathBuf::from("/usr/local/share/cosmic"),
                PathBuf::from("/usr/share/cosmic")
            ]
        );
    }
}
