//! Floating apps: COSMIC's tiling exceptions, windows that open floating
//! while automatic tiling is on.
//!
//! Source (pinned): cosmic-settings-daemon e37160f
//! `config/src/window_rules/mod.rs` (the cosmic-settings-config cosmic-comp
//! 4d14528 is built with) and cosmic-comp 4d14528 `src/shell/layout/mod.rs`.
//!
//! `com.system76.CosmicSettings.WindowRules/v1` holds read-only
//! `tiling_exception_defaults` (an app-id pattern with title patterns) and
//! the user's `tiling_exception_custom` (one app-id/title pair with an
//! on/off switch each). Defaults are expanded in order and a custom entry
//! with the exact same pair strings overrides that default's switch;
//! anything else is appended. The compositor matches both strings as
//! unanchored Rust regexes and a rule applies only when both match. The
//! matcher is rebuilt when the keys change and only consulted when a window
//! is placed, so a change affects windows opened afterwards.
//!
//! Only the custom key is written. Its list is rewritten whole, in order,
//! from its exact decoding. Lists are read like COSMIC reads them (serde
//! skips members it doesn't know), so a list with such members is shown as
//! COSMIC applies it but is never rewritten, which would drop them; a list
//! COSMIC can't read at all is ignored by COSMIC and never rewritten.
use super::appearance::{Blocked, Plan, decode_exact};
use super::store::{
    ConfigKey, DesktopError, DesktopPatch, DesktopStore, KeyRule, Namespace, RawValue, Snapshot,
};
use regex::{Regex, RegexSet};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub const NAMESPACE: &str = "com.system76.CosmicSettings.WindowRules";

/// Upstream `DefaultApplicationException`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefaultRule {
    pub appid: String,
    pub titles: Vec<String>,
}

/// Upstream `PreciseApplicationException`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomRule {
    pub appid: String,
    pub title: String,
    pub enabled: bool,
}

pub fn defaults_key() -> ConfigKey {
    ConfigKey::new(NAMESPACE, 1, "tiling_exception_defaults").expect("rule key is valid")
}
pub fn custom_key() -> ConfigKey {
    ConfigKey::new(NAMESPACE, 1, "tiling_exception_custom").expect("rule key is valid")
}

/// Short user-facing names of the rule keys, for notices.
pub fn key_label(key: &ConfigKey) -> Option<String> {
    if *key == custom_key() {
        Some("floating-app rules".into())
    } else if *key == defaults_key() {
        Some("default floating-app rules".into())
    } else {
        None
    }
}

/// How one pattern reads to a person. Nothing is ever rewritten from this.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Match {
    /// Empty or `.*`: matches everything.
    Any,
    /// `^literal$`: exactly this text.
    Exact(String),
    /// A plain literal: anywhere in the text.
    Contains(String),
    /// Anything else, shown as it is.
    Pattern(String),
}

pub fn classify(pattern: &str) -> Match {
    if pattern.is_empty() || pattern == ".*" {
        return Match::Any;
    }
    if let Some(inner) = pattern
        .strip_prefix('^')
        .and_then(|rest| rest.strip_suffix('$'))
        && let Some(literal) = unescape(inner)
        && !literal.is_empty()
    {
        return Match::Exact(literal);
    }
    match unescape(pattern) {
        Some(literal) => Match::Contains(literal),
        None => Match::Pattern(pattern.to_string()),
    }
}

/// Characters `regex::escape` escapes; outside a character class only
/// some of them are special, but each may be written escaped.
const META: &str = "\\.+*?()|[]{}^$#&-~";

/// The text `pattern` matches literally: plain characters, and escaped
/// metacharacters, but nothing special (so `wl-clipboard` and
/// `wl\\-clipboard` both read as text, `a.b` does not).
fn unescape(pattern: &str) -> Option<String> {
    let mut literal = String::new();
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let next = chars.next().filter(|next| META.contains(*next))?;
                literal.push(next);
            }
            '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' => {
                return None;
            }
            c => literal.push(c),
        }
    }
    Some(literal)
}

/// The app-id pattern for exactly `id`.
pub fn exact_app(id: &str) -> String {
    format!("^{}$", regex::escape(id))
}
/// The title pattern for a title containing `text`; empty means any title.
pub fn title_contains(text: &str) -> String {
    regex::escape(text)
}

/// Checks a pattern with the compositor's regex engine and version.
pub fn validate(pattern: &str) -> Result<(), String> {
    Regex::new(pattern).map(drop).map_err(|error| {
        let detail = error.to_string();
        let last = detail.lines().last().unwrap_or_default().trim();
        if last.starts_with("error:") {
            format!(
                "Not a valid pattern: {}",
                last.trim_start_matches("error:").trim()
            )
        } else {
            "Not a valid pattern.".into()
        }
    })
}

/// One effective rule: the expanded defaults with their overrides, then the
/// other custom rules, exactly as COSMIC merges them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleRow {
    pub appid: String,
    pub title: String,
    pub enabled: bool,
    /// The pair comes from COSMIC's defaults.
    pub default: bool,
    /// A custom entry overrides this default's switch.
    pub overridden: bool,
    /// How many custom entries hold this pair (more than one is a
    /// duplicate; edits apply to every copy).
    pub copies: usize,
    /// How many default entries hold this pair; an override only reaches
    /// the first, so a repeated default can't be switched off here.
    pub default_copies: usize,
}

/// A list as COSMIC's reader sees it: absent is empty; members this
/// version doesn't know are skipped, as serde does upstream.
fn read<T: DeserializeOwned>(raw: Option<&RawValue>) -> Result<Vec<T>, String> {
    match raw {
        None => Ok(Vec::new()),
        Some(raw) => raw.decode::<Vec<T>>(),
    }
}

/// How a rule list reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListState {
    /// Read exactly; the custom list can be rewritten.
    Exact,
    /// COSMIC reads and applies it, but it has members this version
    /// doesn't know. Shown as COSMIC applies it; never rewritten.
    Unknown,
    /// COSMIC can't read it, so it uses none of it. Never rewritten.
    Unreadable(String),
}

fn state<T: DeserializeOwned + Serialize>(raw: Option<&RawValue>) -> ListState {
    match (read::<T>(raw), raw.map(decode_exact::<Vec<T>>)) {
        (Err(error), _) => ListState::Unreadable(error),
        (Ok(_), Some(Err(_))) => ListState::Unknown,
        (Ok(_), _) => ListState::Exact,
    }
}

fn expand(defaults: &[DefaultRule]) -> Vec<(String, String)> {
    defaults
        .iter()
        .flat_map(|rule| {
            rule.titles
                .iter()
                .map(|title| (rule.appid.clone(), title.clone()))
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowRulesDraft {
    base: Snapshot,
    /// The edited custom list; `None` while unchanged.
    custom: Option<Vec<CustomRule>>,
    conflicts: Vec<ConfigKey>,
}

impl WindowRulesDraft {
    pub fn keys() -> Vec<ConfigKey> {
        vec![defaults_key(), custom_key()]
    }
    /// Reads only; creates no files or directories.
    pub fn load(store: &DesktopStore) -> Result<Self, DesktopError> {
        Ok(Self::from_snapshot(store.load(&Self::keys())?))
    }
    pub fn from_snapshot(base: Snapshot) -> Self {
        Self {
            base,
            custom: None,
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
        self.custom.is_some()
    }
    pub fn conflicts(&self) -> &[ConfigKey] {
        &self.conflicts
    }
    pub fn discard(&mut self) {
        self.custom = None;
        self.conflicts.clear();
    }
    pub fn rebase(&mut self, snapshot: Snapshot) {
        *self = Self::from_snapshot(snapshot);
    }

    /// COSMIC's defaults as COSMIC reads them; absent means none.
    /// Unreadable defaults are an error here, and COSMIC uses none of them.
    pub fn defaults(&self) -> Result<Vec<DefaultRule>, String> {
        read(self.base.effective(&defaults_key()))
    }
    pub fn base_custom(&self) -> Result<Vec<CustomRule>, String> {
        read(self.base.effective(&custom_key()))
    }
    pub fn defaults_state(&self) -> ListState {
        state::<DefaultRule>(self.base.effective(&defaults_key()))
    }
    pub fn custom_state(&self) -> ListState {
        state::<CustomRule>(self.base.effective(&custom_key()))
    }
    /// Whether the custom list can be changed here.
    pub fn editable(&self) -> bool {
        self.custom_state() == ListState::Exact
    }
    /// The custom list after the draft's edits.
    pub fn custom(&self) -> Result<Vec<CustomRule>, String> {
        match &self.custom {
            Some(custom) => Ok(custom.clone()),
            None => self.base_custom(),
        }
    }

    /// The effective rules, merged like upstream `tiling_exceptions`. A
    /// list that can't be read contributes nothing, as in COSMIC; members
    /// this version doesn't know are skipped, as in COSMIC.
    pub fn rows(&self) -> Vec<RuleRow> {
        let defaults = expand(&self.defaults().unwrap_or_default());
        let custom = self.custom().unwrap_or_default();
        let mut rows: Vec<RuleRow> = defaults
            .iter()
            .map(|(appid, title)| RuleRow {
                appid: appid.clone(),
                title: title.clone(),
                enabled: true,
                default: true,
                overridden: false,
                copies: 0,
                default_copies: defaults
                    .iter()
                    .filter(|(a, t)| a == appid && t == title)
                    .count(),
            })
            .collect();
        for rule in &custom {
            let copies = custom
                .iter()
                .filter(|other| other.appid == rule.appid && other.title == rule.title)
                .count();
            match rows
                .iter_mut()
                .find(|row| row.appid == rule.appid && row.title == rule.title)
            {
                Some(row) => {
                    row.enabled = rule.enabled;
                    row.overridden |= row.default;
                    row.copies = copies;
                }
                None => rows.push(RuleRow {
                    appid: rule.appid.clone(),
                    title: rule.title.clone(),
                    enabled: rule.enabled,
                    default: false,
                    overridden: false,
                    copies,
                    default_copies: 0,
                }),
            }
        }
        rows
    }

    fn work(&self) -> Result<(Vec<(String, String)>, Vec<CustomRule>), String> {
        match self.custom_state() {
            ListState::Exact => {}
            ListState::Unknown => {
                return Err(
                    "Your floating-app rules include settings this version doesn’t know, so they can’t be changed here."
                        .into(),
                );
            }
            ListState::Unreadable(error) => {
                return Err(format!(
                    "Your floating-app rules can’t be read ({error}). They are kept as they are."
                ));
            }
        }
        let custom = self.custom()?;
        Ok((expand(&self.defaults().unwrap_or_default()), custom))
    }
    fn commit(&mut self, custom: Vec<CustomRule>) {
        let base = self.base_custom().unwrap_or_default();
        self.custom = (custom != base).then_some(custom);
    }

    /// Adds a custom rule, or replaces the custom rule `editing` (every copy
    /// of that pair) with it. Patterns are checked with the compositor's
    /// regex engine; a pair already listed is refused.
    pub fn save(&mut self, editing: Option<(&str, &str)>, rule: CustomRule) -> Result<(), String> {
        validate(&rule.appid)?;
        validate(&rule.title)?;
        if classify(&rule.appid) == Match::Any && classify(&rule.title) == Match::Any {
            return Err("This would float every window. Add an app ID or a title.".into());
        }
        let (defaults, mut custom) = self.work()?;
        let same = |appid: &str, title: &str| appid == rule.appid && title == rule.title;
        let editing_same = editing.is_some_and(|(a, t)| same(a, t));
        if !editing_same
            && (custom.iter().any(|c| same(&c.appid, &c.title))
                || defaults.iter().any(|(a, t)| same(a, t)))
        {
            return Err("This app and title are already listed.".into());
        }
        match editing {
            None => custom.push(rule),
            Some((appid, title)) => {
                if defaults.iter().any(|(a, t)| a == appid && t == title) {
                    return Err("Default rules can be switched off, not edited.".into());
                }
                let at = custom
                    .iter()
                    .position(|c| c.appid == appid && c.title == title)
                    .ok_or("This rule changed meanwhile. Cancel and open it again.")?;
                custom.retain(|c| !(c.appid == appid && c.title == title));
                custom.insert(at.min(custom.len()), rule);
            }
        }
        self.commit(custom);
        Ok(())
    }

    /// Switches a rule on or off. A default is switched off with a custom
    /// override and back on by removing the override.
    pub fn set_enabled(&mut self, appid: &str, title: &str, enabled: bool) -> Result<(), String> {
        let (defaults, mut custom) = self.work()?;
        let pair = |c: &CustomRule| c.appid == appid && c.title == title;
        let default_copies = defaults
            .iter()
            .filter(|(a, t)| a == appid && t == title)
            .count();
        if default_copies > 1 && !enabled {
            return Err(
                "COSMIC lists this default more than once, so it can’t be switched off here."
                    .into(),
            );
        }
        if default_copies > 0 && enabled {
            custom.retain(|c| !pair(c));
        } else if custom.iter().any(pair) {
            for rule in custom.iter_mut().filter(|c| pair(c)) {
                rule.enabled = enabled;
            }
        } else if default_copies > 0 {
            custom.push(CustomRule {
                appid: appid.into(),
                title: title.into(),
                enabled,
            });
        } else {
            return Err("This rule changed meanwhile.".into());
        }
        self.commit(custom);
        Ok(())
    }

    /// Removes a custom rule (every copy of its pair). For a default this
    /// removes its override, restoring the default.
    pub fn remove(&mut self, appid: &str, title: &str) -> Result<(), String> {
        let (_, mut custom) = self.work()?;
        let before = custom.len();
        custom.retain(|c| !(c.appid == appid && c.title == title));
        if custom.len() == before {
            return Err("This rule changed meanwhile.".into());
        }
        self.commit(custom);
        Ok(())
    }

    /// The whole custom list, or why it can't be written. The guard on the
    /// defaults makes an outside change to the user layer a conflict; the
    /// editor also rechecks the effective defaults (`stale`).
    pub fn plan(&self) -> Plan {
        let mut patch = DesktopPatch::new();
        let mut blocked = Vec::new();
        let Some(custom) = &self.custom else {
            return Plan { patch, blocked };
        };
        let key = custom_key();
        // The compositor builds one regex set from every enabled rule and
        // would panic if it can't; never hand it one.
        let rows = self.rows();
        let enabled = rows.iter().filter(|row| row.enabled);
        let usable = |pattern: &String| Regex::new(pattern).is_ok();
        let apps = enabled
            .clone()
            .filter(|row| usable(&row.appid) && usable(&row.title));
        if RegexSet::new(apps.clone().map(|row| &row.appid)).is_err()
            || RegexSet::new(apps.map(|row| &row.title)).is_err()
        {
            blocked.push(Blocked {
                key,
                reason: "are too large for COSMIC to combine".into(),
            });
            return Plan { patch, blocked };
        }
        match RawValue::encode(custom) {
            Ok(value) => patch.set(key.clone(), self.base.user(&key).cloned(), Some(value)),
            Err(error) => blocked.push(Blocked {
                key,
                reason: error.to_string(),
            }),
        }
        for key in Self::keys() {
            patch.guard(key.clone(), self.base.user(&key).cloned());
        }
        Plan { patch, blocked }
    }

    /// Adopts outside changes while nothing is edited; with edits, both
    /// lists keep their base and a changed one (user layer or, for the
    /// defaults, the system layer) is a conflict.
    pub fn refresh(&mut self, snapshot: Snapshot) {
        if !self.dirty() {
            self.base = snapshot;
            self.conflicts.clear();
            return;
        }
        self.conflicts = super::shortcuts::changed(&self.base, &snapshot);
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
    /// The edited list depends on both inputs: adopt the current values.
    pub fn resolve_conflicts(&mut self, snapshot: Snapshot, _keys: &[ConfigKey]) {
        self.rebase(snapshot);
    }
    /// Inputs whose effective value differs in `fresh`.
    pub fn stale(&self, fresh: &Snapshot) -> Vec<ConfigKey> {
        if self.dirty() {
            super::shortcuts::changed(&self.base, fresh)
        } else {
            Vec::new()
        }
    }
}

fn check_custom(value: &RawValue) -> Result<(), String> {
    decode_exact::<Vec<CustomRule>>(value).map(drop)
}

/// The user's custom rules; COSMIC's defaults are never written.
pub static RULES: [KeyRule; 1] = [KeyRule {
    namespace: Namespace::Exact(NAMESPACE),
    version: 1,
    key: "tiling_exception_custom",
    validate: check_custom,
}];

#[cfg(test)]
mod tests;
