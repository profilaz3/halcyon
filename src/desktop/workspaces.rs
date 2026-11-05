//! Workspaces: orientation, one set per display or one shared set,
//! wrapping, and what typing in the overview does.
//!
//! Source (pinned): cosmic-comp 4d14528 `cosmic-comp-config/src/workspace.rs`
//! `WorkspaceConfig`: ONE `com.system76.CosmicComp/v1/workspaces` struct
//! with four members. An edit always rewrites the whole struct from the
//! decoded current value, so the members it does not change keep their
//! values; a value with members, variants or syntax this version does not
//! recognize is never rewritten. Pinned workspaces, display assignments and
//! the current workspace are other keys and are never touched.
use super::appearance::{Blocked, COMPOSITOR, Plan, decode_exact};
use super::panels::Setting;
use super::store::{
    ConfigKey, DesktopError, DesktopPatch, DesktopStore, KeyRule, Namespace, RawValue, Snapshot,
};
use serde::{Deserialize, Serialize};

/// How workspaces span displays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceMode {
    /// Separate workspaces on each display.
    #[default]
    OutputBound,
    /// One set of workspaces across all displays.
    Global,
}

/// The direction workspaces are arranged and navigated in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceLayout {
    #[default]
    Vertical,
    Horizontal,
}

/// What typing in the workspaces overview does (upstream `Action`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypingAction {
    #[default]
    None,
    OpenLauncher,
    OpenApplications,
}

fn default_wraparound() -> bool {
    true
}

/// `WorkspaceConfig` at 4d14528, member for member, with its serde
/// defaults: only `workspace_mode` is required.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub workspace_mode: WorkspaceMode,
    #[serde(default)]
    pub workspace_layout: WorkspaceLayout,
    #[serde(default)]
    pub action_on_typing: TypingAction,
    #[serde(default = "default_wraparound")]
    pub workspace_wraparound: bool,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            workspace_mode: WorkspaceMode::default(),
            workspace_layout: WorkspaceLayout::default(),
            action_on_typing: TypingAction::default(),
            workspace_wraparound: default_wraparound(),
        }
    }
}

/// Member edits; `None` keeps the member's current value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceEdits {
    pub workspace_mode: Option<WorkspaceMode>,
    pub workspace_layout: Option<WorkspaceLayout>,
    pub action_on_typing: Option<TypingAction>,
    pub workspace_wraparound: Option<bool>,
}

impl WorkspaceEdits {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
    fn apply(&self, config: &mut WorkspaceConfig) {
        if let Some(mode) = self.workspace_mode {
            config.workspace_mode = mode;
        }
        if let Some(layout) = self.workspace_layout {
            config.workspace_layout = layout;
        }
        if let Some(action) = self.action_on_typing {
            config.action_on_typing = action;
        }
        if let Some(wrap) = self.workspace_wraparound {
            config.workspace_wraparound = wrap;
        }
    }
    /// Forgets members that already hold the edited value.
    fn trim(&mut self, base: &WorkspaceConfig) {
        if self.workspace_mode == Some(base.workspace_mode) {
            self.workspace_mode = None;
        }
        if self.workspace_layout == Some(base.workspace_layout) {
            self.workspace_layout = None;
        }
        if self.action_on_typing == Some(base.action_on_typing) {
            self.action_on_typing = None;
        }
        if self.workspace_wraparound == Some(base.workspace_wraparound) {
            self.workspace_wraparound = None;
        }
    }
}

pub fn key() -> ConfigKey {
    ConfigKey::new(COMPOSITOR, 1, "workspaces").expect("workspace key is valid")
}

/// Short user-facing name of the workspaces key, for notices.
pub fn key_label(key: &ConfigKey) -> Option<String> {
    (*key == self::key()).then(|| "workspace settings".to_string())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkspacesDraft {
    base: Snapshot,
    edits: WorkspaceEdits,
    conflicts: Vec<ConfigKey>,
}

impl WorkspacesDraft {
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
            edits: WorkspaceEdits::default(),
            conflicts: Vec::new(),
        }
    }
    #[allow(dead_code)] // Draft interface shared with Profiles and later pages.
    pub fn snapshot(&self) -> &Snapshot {
        &self.base
    }
    pub fn into_snapshot(self) -> Snapshot {
        self.base
    }
    pub fn dirty(&self) -> bool {
        !self.edits.is_empty()
    }
    pub fn edits(&self) -> WorkspaceEdits {
        self.edits
    }
    pub fn conflicts(&self) -> &[ConfigKey] {
        &self.conflicts
    }
    pub fn discard(&mut self) {
        self.edits = WorkspaceEdits::default();
        self.conflicts.clear();
    }
    pub fn rebase(&mut self, snapshot: Snapshot) {
        *self = Self::from_snapshot(snapshot);
    }

    /// The struct before the draft's edits: the stored value decoded
    /// exactly (members it lacks take their serde defaults), or the pinned
    /// default only when no layer defines the key. A value that does not
    /// decode exactly is shown as it is and never rewritten.
    pub fn base_config(&self) -> Setting<WorkspaceConfig> {
        match self.base.effective(&key()) {
            Some(raw) => match decode_exact::<WorkspaceConfig>(raw) {
                Ok(config) => Setting::Known(config),
                Err(_) => Setting::Unsupported(raw.as_str().trim().to_string()),
            },
            None => Setting::Known(WorkspaceConfig::default()),
        }
    }
    /// The struct after the draft's edits.
    pub fn config(&self) -> Setting<WorkspaceConfig> {
        match self.base_config() {
            Setting::Known(mut config) => {
                self.edits.apply(&mut config);
                Setting::Known(config)
            }
            unsupported => unsupported,
        }
    }
    fn edit(&mut self, change: impl FnOnce(&mut WorkspaceEdits)) {
        change(&mut self.edits);
        if let Setting::Known(base) = self.base_config() {
            self.edits.trim(&base);
        }
    }
    pub fn set_mode(&mut self, mode: WorkspaceMode) {
        self.edit(|edits| edits.workspace_mode = Some(mode));
    }
    pub fn set_layout(&mut self, layout: WorkspaceLayout) {
        self.edit(|edits| edits.workspace_layout = Some(layout));
    }
    pub fn set_typing(&mut self, action: TypingAction) {
        self.edit(|edits| edits.action_on_typing = Some(action));
    }
    pub fn set_wraparound(&mut self, wrap: bool) {
        self.edit(|edits| edits.workspace_wraparound = Some(wrap));
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

    /// Every edit depends on the entire effective struct, including defaults.
    pub fn stale(&self, fresh: &Snapshot) -> Vec<ConfigKey> {
        let key = key();
        if self.dirty()
            && (self.base.user(&key) != fresh.user(&key)
                || self.base.effective(&key) != fresh.effective(&key))
        {
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
        let changed =
            snapshot.user(&key) != old.user.as_ref() || snapshot.effective(&key) != old.effective();
        self.conflicts.clear();
        if changed {
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
            self.rebase(snapshot);
        } else {
            self.refresh(snapshot);
        }
    }
}

fn check_workspaces(value: &RawValue) -> Result<(), String> {
    decode_exact::<WorkspaceConfig>(value).map(drop)
}

/// The workspaces key; no other CosmicComp key is touched.
pub static RULES: [KeyRule; 1] = [KeyRule {
    namespace: Namespace::Exact(COMPOSITOR),
    version: 1,
    key: "workspaces",
    validate: check_workspaces,
}];

#[cfg(test)]
mod tests;
