//! Shortcuts: COSMIC's default and custom key bindings, and custom commands
//! that can be paused without losing them.
//!
//! Source (pinned): cosmic-settings-daemon e37160f
//! `config/src/shortcuts/mod.rs` (`com.system76.CosmicSettings.Shortcuts/v1`
//! `defaults` and `custom`, custom overriding defaults per binding) and
//! cosmic-comp 4d14528 `src/input/mod.rs` (modifiers + keysym matching;
//! `Disable` entries are skipped, keycodes are never consulted).
//!
//! Only `custom` and this app's own `disabled_commands` are ever written;
//! `defaults` and `system_actions` are read-only. A paused command is a
//! native `Disable` entry plus its kept command in `disabled_commands`;
//! both change in the same owned transaction, so Undo and recovery restore
//! them together. Collections are atomic: an outside change to any of them
//! replaces the draft rather than being merged into it.
pub mod format;

use super::appearance::{Blocked, Plan, decode_exact};
use super::store::{
    ConfigKey, DesktopError, DesktopPatch, DesktopStore, KeyRule, Namespace, RawValue, Snapshot,
};
pub use format::{Action, Binding, Entry, Ident, action_label};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const NAMESPACE: &str = "com.system76.CosmicSettings.Shortcuts";
/// Halcyon's own local state; not a COSMIC namespace.
pub const PAUSED_NAMESPACE: &str = "io.github.workstation.DesktopCards.Shortcuts";
const MAX_NAME: usize = 120;

pub fn defaults_key() -> ConfigKey {
    ConfigKey::new(NAMESPACE, 1, "defaults").expect("shortcut key is valid")
}
pub fn custom_key() -> ConfigKey {
    ConfigKey::new(NAMESPACE, 1, "custom").expect("shortcut key is valid")
}
pub fn paused_key() -> ConfigKey {
    ConfigKey::new(PAUSED_NAMESPACE, 1, "disabled_commands").expect("paused key is valid")
}

/// Short user-facing names of the shortcut keys, for notices.
pub fn key_label(key: &ConfigKey) -> Option<String> {
    if *key == custom_key() {
        Some("custom shortcuts".into())
    } else if *key == defaults_key() {
        Some("default shortcuts".into())
    } else if *key == paused_key() {
        Some("paused shortcut commands".into())
    } else {
        None
    }
}

/// A custom command that is switched off, kept so it can be switched on.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PausedCommand {
    pub binding: Binding,
    pub command: String,
}

fn same_paused(a: &[PausedCommand], b: &[PausedCommand]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| format::same_fields(&a.binding, &b.binding) && a.command == b.command)
}

fn same_entries(a: &[Entry], b: &[Entry]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| a.binding_raw == b.binding_raw && a.action_raw == b.action_raw)
}

/// A collection key, read from the layer COSMIC uses. Absent is the pinned
/// empty default; unreadable is an error, never empty.
fn read_map(raw: Option<&RawValue>) -> Result<Vec<Entry>, String> {
    match raw {
        None => Ok(Vec::new()),
        Some(raw) => format::parse_map(raw.as_str()),
    }
}

fn read_paused(raw: Option<&RawValue>) -> Result<Vec<PausedCommand>, String> {
    match raw {
        None => Ok(Vec::new()),
        Some(raw) => decode_exact::<Vec<PausedCommand>>(raw),
    }
}

/// What a draft operation needs before it can go ahead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// `combo` already does `current`. Replace it, or cancel.
    Replace { combo: String, current: String },
    /// Freeing `combo` would turn `default` back on. Turn it back on, or
    /// keep the combination off.
    Resume { combo: String, default: String },
    /// Not possible; the reason says why.
    Refused(String),
}

/// Answers given so far to an operation's questions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Choice {
    pub replace: bool,
    /// `Some(true)` turns the default back on, `Some(false)` keeps it off.
    pub resume: Option<bool>,
}

/// What a row operation acts on. A paused command whose combination
/// something else now uses shares its keys with that occupant, so the keys
/// alone don't say which of the two is meant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Target {
    /// The custom entry at these keys (or the default under them).
    Entry(Ident),
    /// The paused command kept for these keys, never what uses them now.
    Paused(Ident),
}

impl Target {
    pub fn ident(self) -> Ident {
        match self {
            Self::Entry(ident) | Self::Paused(ident) => ident,
        }
    }
}

/// Shown on rows whose binding carries a hardware keycode. The pinned
/// compositor ignores the keycode itself and matches the modifiers and key
/// (or, without a key, the modifiers alone), so the shown keys still work.
pub const KEYCODE_NOTE: &str = "hardware key code ignored · the keys shown still run it";

/// What a row is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A custom command: running, or paused with its command kept.
    /// `displaced` names what now uses the combination when another tool
    /// replaced a paused command's binding.
    Command {
        command: String,
        running: bool,
        displaced: Option<String>,
    },
    /// A built-in default, on, or turned off by a custom `Disable`.
    Builtin { label: String, on: bool },
    /// A custom entry with a built-in action, kept as it is.
    Override { label: String },
    /// A custom `Disable` with nothing under it.
    Off,
    /// An action this version doesn't recognize; COSMIC treats it as off.
    Unknown { raw: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub binding: Binding,
    pub name: String,
    pub kind: Kind,
    /// Whether the row comes from (or is overridden by) the custom list.
    pub custom: bool,
    /// The custom entry's position, for removing one copy of a duplicate.
    pub index: Option<usize>,
    /// The built-in action that would run if the custom entry went away.
    pub default: Option<String>,
    /// The custom list holds this combination more than once.
    pub duplicate: bool,
    /// A repeated default COSMIC doesn't use (the last copy wins).
    pub unused: bool,
}

impl Row {
    pub fn ident(&self) -> Ident {
        self.binding.ident()
    }
    /// What this row's Edit, Remove and detail act on: a paused command's
    /// kept record, or the custom entry at the keys.
    pub fn target(&self) -> Target {
        match self.kind {
            Kind::Command { running: false, .. } => Target::Paused(self.ident()),
            _ => Target::Entry(self.ident()),
        }
    }
    /// A hardware keycode binding: kept and shown, not editable here.
    pub fn keycode(&self) -> bool {
        self.binding.keycode.is_some()
    }
    /// One of COSMIC's own shortcuts, on or turned off.
    pub fn builtin(&self) -> bool {
        matches!(self.kind, Kind::Builtin { .. }) || !self.custom
    }
}

/// The lists one operation works on, from the current draft.
struct Work {
    defaults: Vec<Entry>,
    custom: Vec<Entry>,
    paused: Result<Vec<PausedCommand>, String>,
}

const PAUSED_UNREADABLE: &str =
    "Paused commands can’t be read, so this shortcut can’t be changed safely.";

impl Work {
    fn custom_at(&self, ident: Ident) -> Option<usize> {
        self.custom.iter().position(|entry| entry.ident() == ident)
    }
    /// The default COSMIC uses for `ident` (the last copy).
    fn default_at(&self, ident: Ident) -> Option<&Entry> {
        self.defaults
            .iter()
            .rev()
            .find(|entry| entry.ident() == ident)
    }
    fn default_active(&self, ident: Ident) -> Option<String> {
        self.default_at(ident)
            .filter(|entry| entry.active())
            .map(Entry::label)
    }
    fn paused(&self) -> Result<&Vec<PausedCommand>, Ask> {
        self.paused
            .as_ref()
            .map_err(|_| Ask::Refused(PAUSED_UNREADABLE.into()))
    }
    fn paused_mut(&mut self) -> Result<&mut Vec<PausedCommand>, Ask> {
        self.paused
            .as_mut()
            .map_err(|_| Ask::Refused(PAUSED_UNREADABLE.into()))
    }
    fn paused_at(&self, ident: Ident) -> Result<Option<usize>, Ask> {
        Ok(self
            .paused()?
            .iter()
            .position(|paused| paused.binding.ident() == ident))
    }
    /// A paused command at `ident`, when paused commands can be read.
    fn paused_known(&self, ident: Ident) -> Option<usize> {
        self.paused.as_ref().ok().and_then(|paused| {
            paused
                .iter()
                .position(|paused| paused.binding.ident() == ident)
        })
    }
    /// What uses `ident` now, if anything would be lost by taking it. A
    /// turned-off combination is free unless it holds a paused command;
    /// when paused commands can't be read, that can't be known.
    fn occupant(&self, ident: Ident) -> Result<Option<String>, Ask> {
        let paused_name_at = |at: usize| {
            self.paused
                .as_ref()
                .map(|paused| format!("the paused command “{}”", paused_name(&paused[at])))
                .unwrap_or_default()
        };
        if let Some(index) = self.custom_at(ident) {
            let entry = &self.custom[index];
            if entry.is_disable() {
                return Ok(self.paused_at(ident)?.map(paused_name_at));
            }
            return Ok(Some(match entry.command() {
                Some(command) => format!("the command “{}”", entry_name(entry, command)),
                None if entry.action.is_none() => "an unrecognized action".into(),
                None => entry.label(),
            }));
        }
        if let Some(at) = self.paused_known(ident) {
            return Ok(Some(paused_name_at(at)));
        }
        Ok(self.default_active(ident))
    }
    /// Takes `ident` for a new entry: drops its custom entry and any paused
    /// command there. Returns where the custom entry was.
    fn clear(&mut self, ident: Ident) -> Option<usize> {
        if let Some(at) = self.paused_known(ident)
            && let Ok(paused) = self.paused.as_mut()
        {
            paused.remove(at);
        }
        self.custom_at(ident).inspect(|&at| {
            self.custom.remove(at);
        })
    }
    /// After the custom entry at `ident` went away (it was at `at`): a
    /// built-in default would run again. Ask, or keep it off with a plain
    /// `Disable`.
    fn vacate(&mut self, binding: &Binding, at: usize, resume: Option<bool>) -> Result<(), Ask> {
        let Some(default) = self.default_active(binding.ident()) else {
            return Ok(());
        };
        match resume {
            None => Err(Ask::Resume {
                combo: binding.label(),
                default,
            }),
            Some(true) => Ok(()),
            Some(false) => {
                let off = Entry::new(binding.plain(), Action::Disable).map_err(Ask::Refused)?;
                self.custom.insert(at.min(self.custom.len()), off);
                Ok(())
            }
        }
    }
}

fn entry_name(entry: &Entry, fallback: &str) -> String {
    entry
        .binding
        .description
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn paused_name(paused: &PausedCommand) -> String {
    paused
        .binding
        .description
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| paused.command.clone())
}

/// Identities the list holds more than once.
fn duplicates(entries: &[Entry]) -> Vec<Ident> {
    let mut counts: BTreeMap<Ident, usize> = BTreeMap::new();
    for entry in entries {
        *counts.entry(entry.ident()).or_default() += 1;
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(ident, _)| ident)
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShortcutsDraft {
    base: Snapshot,
    /// The edited custom list; `None` while unchanged.
    custom: Option<Vec<Entry>>,
    /// The edited paused commands; `None` while unchanged.
    paused: Option<Vec<PausedCommand>>,
    conflicts: Vec<ConfigKey>,
}

impl PartialEq for ShortcutsDraft {
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.conflicts == other.conflicts
            && match (&self.custom, &other.custom) {
                (Some(a), Some(b)) => same_entries(a, b),
                (None, None) => true,
                _ => false,
            }
            && match (&self.paused, &other.paused) {
                (Some(a), Some(b)) => same_paused(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}

impl ShortcutsDraft {
    pub fn keys() -> Vec<ConfigKey> {
        vec![defaults_key(), custom_key(), paused_key()]
    }
    /// Reads only; creates no files or directories.
    pub fn load(store: &DesktopStore) -> Result<Self, DesktopError> {
        Ok(Self::from_snapshot(store.load(&Self::keys())?))
    }
    pub fn from_snapshot(base: Snapshot) -> Self {
        Self {
            base,
            custom: None,
            paused: None,
            conflicts: Vec::new(),
        }
    }
    #[allow(dead_code)] // Draft interface shared with the other pages.
    pub fn snapshot(&self) -> &Snapshot {
        &self.base
    }
    pub fn into_snapshot(self) -> Snapshot {
        self.base
    }
    pub fn dirty(&self) -> bool {
        self.custom.is_some() || self.paused.is_some()
    }
    pub fn conflicts(&self) -> &[ConfigKey] {
        &self.conflicts
    }
    pub fn discard(&mut self) {
        self.custom = None;
        self.paused = None;
        self.conflicts.clear();
    }
    pub fn rebase(&mut self, snapshot: Snapshot) {
        *self = Self::from_snapshot(snapshot);
    }

    pub fn defaults(&self) -> Result<Vec<Entry>, String> {
        read_map(self.base.effective(&defaults_key()))
    }
    pub fn base_custom(&self) -> Result<Vec<Entry>, String> {
        read_map(self.base.effective(&custom_key()))
    }
    /// The custom list after the draft's edits.
    pub fn custom(&self) -> Result<Vec<Entry>, String> {
        match &self.custom {
            Some(custom) => Ok(custom.clone()),
            None => self.base_custom(),
        }
    }
    pub fn base_paused(&self) -> Result<Vec<PausedCommand>, String> {
        read_paused(self.base.effective(&paused_key()))
    }
    pub fn paused(&self) -> Result<Vec<PausedCommand>, String> {
        match &self.paused {
            Some(paused) => Ok(paused.clone()),
            None => self.base_paused(),
        }
    }
    /// Why the custom list can't be rewritten, if it can't.
    pub fn custom_problem(&self) -> Option<String> {
        if let Err(error) = self.base_custom() {
            return Some(format!(
                "Your custom shortcuts can’t be read ({error}). They are kept as they are, and COSMIC ignores them until fixed."
            ));
        }
        if self
            .base
            .effective(&custom_key())
            .is_some_and(|raw| format::uses_extensions(raw.as_str()))
        {
            return Some(
                "Your custom shortcuts use RON extensions this editor can’t reproduce. They are kept as they are.".into(),
            );
        }
        None
    }
    /// Combinations the custom list holds more than once.
    pub fn duplicates(&self) -> Vec<Ident> {
        self.custom()
            .map(|custom| duplicates(&custom))
            .unwrap_or_default()
    }

    /// Every shortcut: the custom list first (in its order), then the
    /// defaults it doesn't override, then paused commands whose combination
    /// is now used by something else.
    pub fn rows(&self) -> Vec<Row> {
        let defaults = self.defaults().unwrap_or_default();
        let custom = self.custom().unwrap_or_default();
        let paused = self.paused().unwrap_or_default();
        let dups = duplicates(&custom);
        let used: BTreeMap<Ident, usize> = defaults
            .iter()
            .enumerate()
            .map(|(at, entry)| (entry.ident(), at))
            .collect();
        let default_label = |ident: Ident| {
            used.get(&ident)
                .map(|&at| &defaults[at])
                .filter(|entry| entry.active())
                .map(Entry::label)
        };
        let paused_for = |ident: Ident| paused.iter().find(|p| p.binding.ident() == ident);
        let mut rows = Vec::new();
        for (index, entry) in custom.iter().enumerate() {
            let ident = entry.ident();
            let default = default_label(ident);
            let (name, kind) = match &entry.action {
                Some(Action::Spawn(command)) => (
                    entry_name(entry, command),
                    Kind::Command {
                        command: command.clone(),
                        running: true,
                        displaced: None,
                    },
                ),
                Some(Action::Disable) => match (paused_for(ident), &default) {
                    (Some(paused), _) => (
                        paused_name(paused),
                        Kind::Command {
                            command: paused.command.clone(),
                            running: false,
                            displaced: None,
                        },
                    ),
                    (None, Some(label)) => (
                        label.clone(),
                        Kind::Builtin {
                            label: label.clone(),
                            on: false,
                        },
                    ),
                    (None, None) => ("Turned off".into(), Kind::Off),
                },
                Some(action) => (
                    entry_name(entry, &action_label(action)),
                    Kind::Override {
                        label: action_label(action),
                    },
                ),
                None => (
                    entry_name(entry, "Unrecognized action"),
                    Kind::Unknown {
                        raw: entry.action_raw.clone(),
                    },
                ),
            };
            rows.push(Row {
                binding: entry.binding.clone(),
                name,
                kind,
                custom: true,
                index: Some(index),
                default,
                duplicate: dups.contains(&ident),
                unused: false,
            });
        }
        for (at, entry) in defaults.iter().enumerate() {
            let ident = entry.ident();
            if custom.iter().any(|custom| custom.ident() == ident) {
                continue;
            }
            let kind = match &entry.action {
                None => Kind::Unknown {
                    raw: entry.action_raw.clone(),
                },
                Some(Action::Disable) => Kind::Off,
                Some(action) => Kind::Builtin {
                    label: action_label(action),
                    on: true,
                },
            };
            rows.push(Row {
                binding: entry.binding.clone(),
                name: entry.label(),
                kind,
                custom: false,
                index: None,
                default: None,
                duplicate: false,
                unused: used.get(&ident) != Some(&at),
            });
        }
        for paused in &paused {
            let ident = paused.binding.ident();
            let held = custom
                .iter()
                .any(|entry| entry.ident() == ident && entry.is_disable());
            if held {
                continue;
            }
            let displaced = custom
                .iter()
                .rev()
                .find(|entry| entry.ident() == ident)
                .map(|entry| match entry.command() {
                    Some(command) => format!("the command “{}”", entry_name(entry, command)),
                    None => entry.label(),
                })
                .or_else(|| default_label(ident));
            rows.push(Row {
                binding: paused.binding.clone(),
                name: paused_name(paused),
                kind: Kind::Command {
                    command: paused.command.clone(),
                    running: false,
                    displaced,
                },
                custom: true,
                index: None,
                default: default_label(ident),
                duplicate: false,
                unused: false,
            });
        }
        rows
    }

    /// The lists to edit, or why they can't be. Duplicates block every
    /// operation except removing one copy.
    fn work(&self, allow_duplicates: bool) -> Result<Work, Ask> {
        if let Some(problem) = self.custom_problem() {
            return Err(Ask::Refused(problem));
        }
        let custom = self.custom().map_err(Ask::Refused)?;
        if !allow_duplicates && let Some(ident) = duplicates(&custom).first() {
            let combo = custom
                .iter()
                .find(|entry| entry.ident() == *ident)
                .map(|entry| entry.binding.label())
                .unwrap_or_default();
            return Err(Ask::Refused(format!(
                "Custom shortcuts list {combo} more than once. Remove the extra copy first; COSMIC uses the last one."
            )));
        }
        Ok(Work {
            defaults: self.defaults().unwrap_or_default(),
            custom,
            paused: self.paused(),
        })
    }
    fn commit(&mut self, work: Work) {
        let base_custom = self.base_custom().unwrap_or_default();
        self.custom = (!same_entries(&work.custom, &base_custom)).then_some(work.custom);
        self.paused = match (work.paused, self.base_paused()) {
            (Ok(paused), Ok(base)) if !same_paused(&paused, &base) => Some(paused),
            _ => None,
        };
    }

    /// Adds a custom command (`editing: None`) or changes the one at
    /// `editing` (a running command, or a paused one): its name,
    /// combination and command. A combination already in use needs
    /// `choice.replace`, also when a paused command would take its own
    /// combination back from what uses it now; moving off a combination
    /// whose built-in default would run again needs `choice.resume`.
    pub fn save_command(
        &mut self,
        editing: Option<Target>,
        name: &str,
        binding: &Binding,
        command: &str,
        choice: Choice,
    ) -> Result<(), Ask> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > MAX_NAME || name.chars().any(char::is_control)
        {
            return Err(Ask::Refused("Give the shortcut a short name.".into()));
        }
        if command.trim().is_empty() {
            return Err(Ask::Refused("Type the command to run.".into()));
        }
        if !binding.is_set() && editing.is_none_or(|old| old.ident() != binding.ident()) {
            return Err(Ask::Refused(
                "Add Super, Ctrl or Alt to that key; only function and media keys work alone."
                    .into(),
            ));
        }
        let mut work = self.work(false)?;
        let target = binding.ident();
        let mut named = binding.plain();
        named.description = Some(name.to_string());
        let spawn = Action::Spawn(command.to_string());
        let Some(editing) = editing else {
            if let Some(current) = work.occupant(target)?
                && !choice.replace
            {
                return Err(Ask::Replace {
                    combo: binding.label(),
                    current,
                });
            }
            let at = work.clear(target);
            let entry = Entry::new(named, spawn).map_err(Ask::Refused)?;
            match at {
                Some(at) => work.custom.insert(at, entry),
                None => work.custom.push(entry),
            }
            self.commit(work);
            return Ok(());
        };
        // The command being edited: running in the custom list, or paused.
        let old = editing.ident();
        let gone =
            || Ask::Refused("This shortcut changed meanwhile. Cancel and open it again.".into());
        let (running, paused) = match editing {
            Target::Entry(old) => (
                Some(
                    work.custom_at(old)
                        .filter(|&at| work.custom[at].command().is_some())
                        .ok_or_else(gone)?,
                ),
                None,
            ),
            Target::Paused(old) => (None, Some(work.paused_at(old)?.ok_or_else(gone)?)),
        };
        let original = match (running, paused) {
            (Some(at), _) => work.custom[at].binding.clone(),
            (None, Some(at)) => work.paused()?[at].binding.clone(),
            _ => unreachable!(),
        };
        if original.keycode.is_some() {
            return Err(Ask::Refused(
                "Hardware key-code shortcuts are kept as they are and can’t be edited here.".into(),
            ));
        }
        if target != old
            && let Some(current) = work.occupant(target)?
            && !choice.replace
        {
            return Err(Ask::Replace {
                combo: binding.label(),
                current,
            });
        }
        let unchanged = target == old
            && original.description.as_deref() == Some(name)
            && match (running, paused) {
                (Some(at), _) => work.custom[at].command() == Some(command),
                (None, Some(at)) => work.paused()?[at].command == command,
                _ => false,
            };
        if unchanged {
            return Ok(());
        }
        match (running, paused) {
            (Some(at), _) if target == old => {
                work.custom[at] = Entry::new(named, spawn).map_err(Ask::Refused)?;
            }
            (Some(at), _) => {
                work.custom.remove(at);
                work.vacate(&original, at, choice.resume)?;
                let at = work.clear(target).unwrap_or(work.custom.len());
                let entry = Entry::new(named, spawn).map_err(Ask::Refused)?;
                work.custom.insert(at.min(work.custom.len()), entry);
            }
            (None, Some(at)) => {
                // A paused command stays paused under its new name or
                // combination; its `Disable` entry follows it.
                work.paused_mut()?.remove(at);
                let held = work.custom_at(old).filter(|&i| work.custom[i].is_disable());
                // Displaced: something else uses its combination now, and
                // saving it there again would take the combination over.
                if target == old
                    && held.is_none()
                    && let Some(current) = work.occupant(target)?
                    && !choice.replace
                {
                    return Err(Ask::Replace {
                        combo: binding.label(),
                        current,
                    });
                }
                if let Some(held) = held {
                    work.custom.remove(held);
                    if target != old {
                        work.vacate(&original, held, choice.resume)?;
                    }
                }
                let at = work
                    .clear(target)
                    .or(held.filter(|_| target == old))
                    .unwrap_or(work.custom.len());
                let off = Entry::new(named.clone(), Action::Disable).map_err(Ask::Refused)?;
                work.custom.insert(at.min(work.custom.len()), off);
                work.paused_mut()?.push(PausedCommand {
                    binding: named,
                    command: command.to_string(),
                });
            }
            _ => unreachable!(),
        }
        self.commit(work);
        Ok(())
    }

    /// Pauses (`on: false`) or resumes a custom command. Pausing writes a
    /// native `Disable` and keeps the command; resuming puts it back and
    /// forgets the kept copy. A combination taken over meanwhile needs
    /// `choice.replace`.
    pub fn set_running(&mut self, ident: Ident, on: bool, choice: Choice) -> Result<(), Ask> {
        let mut work = self.work(false)?;
        if !on {
            let at = work
                .custom_at(ident)
                .filter(|&at| work.custom[at].command().is_some())
                .ok_or_else(|| Ask::Refused("Only custom commands can be paused.".into()))?;
            if work.paused_at(ident)?.is_some() {
                return Err(Ask::Refused(
                    "A paused command already uses this combination. Remove it first.".into(),
                ));
            }
            let entry = work.custom[at].clone();
            let command = entry.command().unwrap_or_default().to_string();
            work.custom[at] = entry.with_action(Action::Disable).map_err(Ask::Refused)?;
            work.paused_mut()?.push(PausedCommand {
                binding: entry.binding.clone(),
                command,
            });
            self.commit(work);
            return Ok(());
        }
        let at = work
            .paused_at(ident)?
            .ok_or_else(|| Ask::Refused("This command isn’t paused any more.".into()))?;
        let paused = work.paused()?[at].clone();
        let spawn = Action::Spawn(paused.command.clone());
        match work.custom_at(ident) {
            Some(index) if work.custom[index].is_disable() => {
                work.custom[index] = work.custom[index]
                    .with_action(spawn)
                    .map_err(Ask::Refused)?;
            }
            Some(index) => {
                if !choice.replace {
                    return Err(Ask::Replace {
                        combo: paused.binding.label(),
                        current: work.occupant(ident)?.unwrap_or_default(),
                    });
                }
                work.custom[index] =
                    Entry::new(paused.binding.clone(), spawn).map_err(Ask::Refused)?;
            }
            None => {
                if let Some(current) = work.default_active(ident)
                    && !choice.replace
                {
                    return Err(Ask::Replace {
                        combo: paused.binding.label(),
                        current,
                    });
                }
                work.custom
                    .push(Entry::new(paused.binding.clone(), spawn).map_err(Ask::Refused)?);
            }
        }
        work.paused_mut()?.remove(at);
        self.commit(work);
        Ok(())
    }

    /// Removes `target`. A custom entry goes with the paused command its
    /// `Disable` holds. A paused command goes with its own `Disable`, if it
    /// still has one; whatever else uses its keys now is kept. When a
    /// built-in default would run again, `choice.resume` decides.
    pub fn remove(&mut self, target: Target, choice: Choice) -> Result<(), Ask> {
        let mut work = self.work(false)?;
        match target {
            Target::Entry(ident) => {
                let at = work.custom_at(ident).ok_or_else(|| {
                    Ask::Refused("Built-in shortcuts can be turned off, not removed.".into())
                })?;
                let entry = work.custom.remove(at);
                if entry.is_disable()
                    && let Some(paused) = work.paused_at(ident)?
                {
                    work.paused_mut()?.remove(paused);
                }
                work.vacate(&entry.binding, at, choice.resume)?;
            }
            Target::Paused(ident) => {
                let paused = work
                    .paused_at(ident)?
                    .ok_or_else(|| Ask::Refused("This command isn’t paused any more.".into()))?;
                work.paused_mut()?.remove(paused);
                let held = work
                    .custom_at(ident)
                    .filter(|&at| work.custom[at].is_disable());
                if let Some(at) = held {
                    let entry = work.custom.remove(at);
                    work.vacate(&entry.binding, at, choice.resume)?;
                }
            }
        }
        self.commit(work);
        Ok(())
    }

    /// Turns a built-in default off with a custom `Disable`, or back on by
    /// removing that `Disable`.
    pub fn set_default(&mut self, ident: Ident, on: bool) -> Result<(), Ask> {
        let mut work = self.work(false)?;
        let default = work
            .default_at(ident)
            .filter(|entry| entry.active())
            .cloned()
            .ok_or_else(|| Ask::Refused("This isn’t a built-in shortcut.".into()))?;
        match (work.custom_at(ident), on) {
            (Some(at), true) if work.custom[at].is_disable() => {
                if work.paused_at(ident)?.is_some() {
                    return Err(Ask::Refused(
                        "A paused command holds this combination. Remove it first.".into(),
                    ));
                }
                work.custom.remove(at);
            }
            (None, false) => {
                let off =
                    Entry::new(default.binding.plain(), Action::Disable).map_err(Ask::Refused)?;
                work.custom.push(off);
            }
            (None, true) => return Ok(()),
            _ => {
                return Err(Ask::Refused(
                    "A custom shortcut uses this combination; remove it to restore the default."
                        .into(),
                ));
            }
        }
        self.commit(work);
        Ok(())
    }

    /// Removes one copy of a combination the custom list repeats.
    pub fn remove_copy(&mut self, index: usize) -> Result<(), Ask> {
        let mut work = self.work(true)?;
        let repeated = work
            .custom
            .get(index)
            .is_some_and(|entry| duplicates(&work.custom).contains(&entry.ident()));
        if !repeated {
            return Err(Ask::Refused(
                "This shortcut isn’t repeated any more.".into(),
            ));
        }
        work.custom.remove(index);
        self.commit(work);
        Ok(())
    }

    /// The custom list and paused commands as written, every untouched
    /// entry's text reproduced exactly; checked by reading it back.
    pub fn plan(&self) -> Plan {
        let mut patch = DesktopPatch::new();
        let mut blocked = Vec::new();
        if !self.dirty() {
            return Plan { patch, blocked };
        }
        if let Some(custom) = &self.custom {
            let key = custom_key();
            let text = format::write_map(custom);
            let faithful = format::parse_map(&text).is_ok_and(|read| {
                read.len() == custom.len()
                    && read.iter().zip(custom).all(|(read, entry)| {
                        format::same_fields(&read.binding, &entry.binding)
                            && read.action_raw == entry.action_raw
                    })
            });
            match RawValue::parse(text) {
                Ok(value) if faithful => {
                    patch.set(key.clone(), self.base.user(&key).cloned(), Some(value));
                }
                _ => blocked.push(Blocked {
                    key,
                    reason: "can’t be rewritten exactly".into(),
                }),
            }
        }
        if let Some(paused) = &self.paused {
            let key = paused_key();
            match RawValue::encode(paused) {
                Ok(value) => patch.set(key.clone(), self.base.user(&key).cloned(), Some(value)),
                Err(error) => blocked.push(Blocked {
                    key,
                    reason: error.to_string(),
                }),
            }
        }
        for key in Self::keys() {
            patch.guard(key.clone(), self.base.user(&key).cloned());
        }
        Plan { patch, blocked }
    }

    /// Adopts outside changes while nothing is edited; with edits, the
    /// lists keep their base and every changed one is a conflict. A changed
    /// system default counts too: the store only checks the user layer.
    pub fn refresh(&mut self, snapshot: Snapshot) {
        if !self.dirty() {
            self.base = snapshot;
            self.conflicts.clear();
            return;
        }
        self.conflicts = changed(&self.base, &snapshot);
        let mut base = snapshot;
        for key in Self::keys() {
            base.insert(
                key.clone(),
                self.base.entry(&key).cloned().unwrap_or_default(),
            );
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
    /// Every edit depends on all three lists: adopt the current values.
    pub fn resolve_conflicts(&mut self, snapshot: Snapshot, _keys: &[ConfigKey]) {
        self.rebase(snapshot);
    }
    /// Inputs whose effective value differs in `fresh`.
    pub fn stale(&self, fresh: &Snapshot) -> Vec<ConfigKey> {
        if self.dirty() {
            changed(&self.base, fresh)
        } else {
            Vec::new()
        }
    }
}

/// Keys of `before` whose user layer or effective value differs in `after`.
pub(crate) fn changed(before: &Snapshot, after: &Snapshot) -> Vec<ConfigKey> {
    before
        .keys()
        .filter(|key| {
            before.user(key) != after.user(key) || before.effective(key) != after.effective(key)
        })
        .cloned()
        .collect()
}

fn check_custom(value: &RawValue) -> Result<(), String> {
    format::parse_map(value.as_str()).map(drop)
}

fn check_paused(value: &RawValue) -> Result<(), String> {
    decode_exact::<Vec<PausedCommand>>(value).map(drop)
}

/// The custom shortcuts and this app's paused commands. COSMIC's defaults
/// and system actions are never written.
pub static RULES: [KeyRule; 2] = [
    KeyRule {
        namespace: Namespace::Exact(NAMESPACE),
        version: 1,
        key: "custom",
        validate: check_custom,
    },
    KeyRule {
        namespace: Namespace::Exact(PAUSED_NAMESPACE),
        version: 1,
        key: "disabled_commands",
        validate: check_paused,
    },
];

#[cfg(test)]
mod tests;
