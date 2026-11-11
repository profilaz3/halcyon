//! Window behavior: COSMIC's automatic tiling and its scope, the active
//! window outline, focus and pointer behavior, edge snapping, the window
//! title-bar buttons, and window gaps.
//!
//! Sources (pinned): cosmic-comp 4d14528 `cosmic-comp-config`
//! `CosmicCompConfig` (`com.system76.CosmicComp/v1`, one key per member) and
//! the compositor's per-key reader `config_changed`; libcosmic dc11d6a
//! `CosmicTk` v1 `show_minimize`/`show_maximize`; cosmic-settings 58aeb65
//! `window_management.rs` (edge snapping on = 10).
//!
//! Window gaps live in each theme builder, so a `WindowsDraft` keeps its own
//! private `AppearanceDraft` with only gap edits and reuses that adapter's
//! builder and derived-theme patch (and its outline coupling). It never
//! shares or applies another page's draft. Nothing here writes files.
use super::appearance::{
    AppearanceDraft, Blocked, COMPOSITOR, Control, MAX_GAP, Plan, TOOLKIT, ThemeVariant,
    Value as ThemeValue, decode_exact,
};
use super::panels::Setting;
use super::store::{
    ConfigKey, DesktopError, DesktopPatch, DesktopStore, KeyRule, Namespace, RawValue, Snapshot,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The largest focus delay the editor and profiles write, in milliseconds.
pub const MAX_FOCUS_DELAY: u64 = 10_000;
/// The largest edge-snapping distance the editor and profiles write, in
/// pixels.
pub const MAX_EDGE_SNAP: u32 = 200;
/// cosmic-settings' “edge gravity” switch writes this distance.
pub const EDGE_SNAP_ON: u32 = 10;

/// `cosmic-comp-config` TileBehavior at 4d14528. Global: automatic tiling
/// applies to every window on every workspace, and changing it rearranges
/// existing windows. PerWorkspace: it is the default for new windows and
/// new workspaces; each workspace keeps its own tiling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TileBehavior {
    #[default]
    Global,
    PerWorkspace,
}

/// One window behavior setting, each its own COSMIC key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Behavior {
    Autotile,
    Scope,
    ActiveHint,
    FocusFollowsCursor,
    CursorFollowsFocus,
    FocusDelay,
    EdgeSnap,
    ShowMinimize,
    ShowMaximize,
}

/// A behavior value, typed like its key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Choice {
    Flag(bool),
    Scope(TileBehavior),
    /// Milliseconds (`u64`).
    Millis(u64),
    /// Pixels (`u32`).
    Pixels(u32),
}

impl Choice {
    fn encode(self) -> Option<RawValue> {
        match self {
            Self::Flag(on) => RawValue::encode(&on),
            Self::Scope(scope) => RawValue::encode(&scope),
            Self::Millis(value) => RawValue::encode(&value),
            Self::Pixels(value) => RawValue::encode(&value),
        }
        .ok()
    }
}

impl Behavior {
    pub const ALL: [Self; 9] = [
        Self::Autotile,
        Self::Scope,
        Self::ActiveHint,
        Self::FocusFollowsCursor,
        Self::CursorFollowsFocus,
        Self::FocusDelay,
        Self::EdgeSnap,
        Self::ShowMinimize,
        Self::ShowMaximize,
    ];
    /// The native key name, also the profile member name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Autotile => "autotile",
            Self::Scope => "autotile_behavior",
            Self::ActiveHint => "active_hint",
            Self::FocusFollowsCursor => "focus_follows_cursor",
            Self::CursorFollowsFocus => "cursor_follows_focus",
            Self::FocusDelay => "focus_follows_cursor_delay",
            Self::EdgeSnap => "edge_snap_threshold",
            Self::ShowMinimize => "show_minimize",
            Self::ShowMaximize => "show_maximize",
        }
    }
    fn namespace(self) -> &'static str {
        match self {
            Self::ShowMinimize | Self::ShowMaximize => TOOLKIT,
            _ => COMPOSITOR,
        }
    }
    pub fn key(self) -> ConfigKey {
        ConfigKey::new(self.namespace(), 1, self.name()).expect("window keys are valid")
    }
    pub fn of(key: &ConfigKey) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|behavior| behavior.key() == *key)
    }
    /// Short user-facing name.
    pub fn title(self) -> &'static str {
        match self {
            Self::Autotile => "automatic tiling",
            Self::Scope => "tiling scope",
            Self::ActiveHint => "active window outline",
            Self::FocusFollowsCursor => "focus follows pointer",
            Self::CursorFollowsFocus => "pointer follows focus",
            Self::FocusDelay => "focus delay",
            Self::EdgeSnap => "edge snapping",
            Self::ShowMinimize => "minimize button",
            Self::ShowMaximize => "maximize button",
        }
    }
    /// The pinned default COSMIC uses when neither layer has the key.
    pub fn default(self) -> Choice {
        let toolkit = cosmic::config::CosmicTk::default();
        match self {
            Self::Autotile => Choice::Flag(false),
            Self::Scope => Choice::Scope(TileBehavior::Global),
            Self::ActiveHint => Choice::Flag(true),
            Self::FocusFollowsCursor | Self::CursorFollowsFocus => Choice::Flag(false),
            Self::FocusDelay => Choice::Millis(250),
            Self::EdgeSnap => Choice::Pixels(0),
            Self::ShowMinimize => Choice::Flag(toolkit.show_minimize),
            Self::ShowMaximize => Choice::Flag(toolkit.show_maximize),
        }
    }
    /// Decodes a stored value exactly; anything else is not this key's type.
    fn decode(self, raw: &RawValue) -> Result<Choice, String> {
        Ok(match self {
            Self::Scope => Choice::Scope(decode_exact(raw)?),
            Self::FocusDelay => Choice::Millis(decode_exact(raw)?),
            Self::EdgeSnap => Choice::Pixels(decode_exact(raw)?),
            _ => Choice::Flag(decode_exact(raw)?),
        })
    }
    /// Whether `choice` has this key's type.
    fn accepts(self, choice: Choice) -> bool {
        matches!(
            (self, choice),
            (Self::Scope, Choice::Scope(_))
                | (Self::FocusDelay, Choice::Millis(_))
                | (Self::EdgeSnap, Choice::Pixels(_))
        ) || (!matches!(self, Self::Scope | Self::FocusDelay | Self::EdgeSnap)
            && matches!(choice, Choice::Flag(_)))
    }
}

/// Which window gap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gap {
    /// Between windows and the screen edges.
    Outer,
    /// Between tiled windows.
    Inner,
}

/// Short user-facing name of a window behavior key, for notices.
pub fn key_label(key: &ConfigKey) -> Option<String> {
    Behavior::of(key).map(|behavior| behavior.title().to_string())
}

/// A transition the compositor cannot take in one step. cosmic-comp reads
/// `autotile` and `autotile_behavior` as separate keys (`config_changed`),
/// and both `update_autotile` and `update_autotile_behavior` end in
/// `apply_tile_change`, which retiles every existing workspace whenever the
/// scope it holds at that moment is Global (cosmic-comp 4d14528
/// `src/shell/mod.rs`). Changing both at once could therefore briefly
/// apply one under the other's old value, so such a change is refused and
/// applied as two steps; see `first_tiling_step`.
pub fn tiling_transition(patch: &DesktopPatch) -> Option<Blocked> {
    let scope = Behavior::Scope.key();
    patch.get(&Behavior::Autotile.key())?;
    let target = patch
        .get(&scope)?
        .desired
        .as_ref()
        .and_then(|raw| raw.decode::<TileBehavior>().ok())
        .unwrap_or(TileBehavior::PerWorkspace);
    let first = match first_tiling_step(target) {
        Behavior::Scope => "the Workspaces choice first, then tiling",
        _ => "tiling first, then the Workspaces choice",
    };
    Some(Blocked {
        key: scope,
        reason: format!("{TRANSITION} Apply {first}."),
    })
}

/// The start of every tiling-transition reason; see `is_transition`.
pub const TRANSITION: &str = "Automatic tiling and the Workspaces choice can’t change in one step.";

/// Whether `blocked` is the tiling transition rather than a value problem.
pub fn is_transition(blocked: &Blocked) -> bool {
    blocked.reason.starts_with(TRANSITION)
}

/// Which coupled key goes first when tiling and its scope both change: the
/// one that keeps the scope in between PerWorkspace, where a tiling change
/// only sets the default for new windows and workspaces. Toward
/// PerWorkspace the scope goes first; toward Global, tiling goes first
/// under the current PerWorkspace scope.
pub fn first_tiling_step(target: TileBehavior) -> Behavior {
    match target {
        TileBehavior::PerWorkspace => Behavior::Scope,
        TileBehavior::Global => Behavior::Autotile,
    }
}

/// The Windows page's draft: behavior edits plus window gaps for each theme
/// variant, through a private gap-only `AppearanceDraft`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowsDraft {
    base: Snapshot,
    edits: BTreeMap<Behavior, Choice>,
    gaps: AppearanceDraft,
    conflicts: Vec<ConfigKey>,
}

impl WindowsDraft {
    /// Every key the draft reads: the behavior keys, and the theme keys the
    /// gap derivation reads and writes.
    pub fn keys() -> Vec<ConfigKey> {
        let mut keys: Vec<ConfigKey> = Behavior::ALL.iter().map(|b| b.key()).collect();
        keys.extend(AppearanceDraft::keys());
        keys
    }
    /// Reads only; creates no files or directories.
    pub fn load(store: &DesktopStore) -> Result<Self, DesktopError> {
        Ok(Self::from_snapshot(store.load(&Self::keys())?))
    }
    pub fn from_snapshot(base: Snapshot) -> Self {
        Self {
            gaps: AppearanceDraft::from_snapshot(base.clone()),
            base,
            edits: BTreeMap::new(),
            conflicts: Vec::new(),
        }
    }
    /// The values the draft started from, without its edits.
    #[allow(dead_code)] // Draft interface shared with Profiles and later pages.
    pub fn snapshot(&self) -> &Snapshot {
        &self.base
    }
    pub fn into_snapshot(self) -> Snapshot {
        self.base
    }
    pub fn dirty(&self) -> bool {
        self.behavior_dirty() || self.gaps_dirty()
    }
    /// Edits to behavior keys (the portable Windows profile domain).
    pub fn behavior_dirty(&self) -> bool {
        !self.edits.is_empty()
    }
    /// Gap edits (theme values: the Appearance profile domain).
    pub fn gaps_dirty(&self) -> bool {
        self.gaps.dirty()
    }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn edited(&self, behavior: Behavior) -> bool {
        self.edits.contains_key(&behavior)
    }
    pub fn edits(&self) -> impl Iterator<Item = (Behavior, Choice)> + '_ {
        self.edits
            .iter()
            .map(|(behavior, choice)| (*behavior, *choice))
    }
    /// Gap edits as `(variant, gap, value)`, for re-applying to a new base.
    pub fn gap_edits(&self) -> Vec<(ThemeVariant, Gap, u32)> {
        self.gaps
            .edits()
            .filter_map(|(control, value)| match (control, value) {
                (Control::OuterGap(variant), ThemeValue::Number(gap)) => {
                    Some((*variant, Gap::Outer, *gap))
                }
                (Control::InnerGap(variant), ThemeValue::Number(gap)) => {
                    Some((*variant, Gap::Inner, *gap))
                }
                _ => None,
            })
            .collect()
    }
    pub fn gap_edited(&self, variant: ThemeVariant) -> bool {
        self.gaps.edited(Control::OuterGap(variant)) || self.gaps.edited(Control::InnerGap(variant))
    }
    /// Keys changed outside the draft that an edit still depends on.
    pub fn conflicts(&self) -> Vec<ConfigKey> {
        let mut keys = self.conflicts.clone();
        keys.extend(self.gaps.conflicts().iter().cloned());
        keys.sort();
        keys.dedup();
        keys
    }
    pub fn discard(&mut self) {
        self.edits.clear();
        self.conflicts.clear();
        self.gaps.discard();
    }
    #[allow(dead_code)] // Draft interface shared with Profiles and later pages.
    pub fn rebase(&mut self, snapshot: Snapshot) {
        *self = Self::from_snapshot(snapshot);
    }

    fn read(&self, behavior: Behavior) -> Setting<Choice> {
        match self.base.effective(&behavior.key()) {
            Some(raw) => match behavior.decode(raw) {
                Ok(choice) => Setting::Known(choice),
                Err(_) => Setting::Unsupported(raw.as_str().trim().to_string()),
            },
            None => Setting::Known(behavior.default()),
        }
    }
    /// The value before the draft's edit.
    pub fn base_value(&self, behavior: Behavior) -> Setting<Choice> {
        self.read(behavior)
    }
    /// The value after the draft's edit.
    pub fn value(&self, behavior: Behavior) -> Setting<Choice> {
        match self.edits.get(&behavior) {
            Some(choice) => Setting::Known(*choice),
            None => self.read(behavior),
        }
    }
    pub fn flag(&self, behavior: Behavior) -> Setting<bool> {
        match self.value(behavior) {
            Setting::Known(Choice::Flag(on)) => Setting::Known(on),
            Setting::Known(_) => Setting::Unsupported(String::new()),
            Setting::Unsupported(raw) => Setting::Unsupported(raw),
        }
    }
    pub fn scope(&self) -> Setting<TileBehavior> {
        match self.value(Behavior::Scope) {
            Setting::Known(Choice::Scope(scope)) => Setting::Known(scope),
            Setting::Known(_) => Setting::Unsupported(String::new()),
            Setting::Unsupported(raw) => Setting::Unsupported(raw),
        }
    }
    pub fn focus_delay(&self) -> Setting<u64> {
        match self.value(Behavior::FocusDelay) {
            Setting::Known(Choice::Millis(value)) => Setting::Known(value),
            Setting::Known(_) => Setting::Unsupported(String::new()),
            Setting::Unsupported(raw) => Setting::Unsupported(raw),
        }
    }
    pub fn edge_snap(&self) -> Setting<u32> {
        match self.value(Behavior::EdgeSnap) {
            Setting::Known(Choice::Pixels(value)) => Setting::Known(value),
            Setting::Known(_) => Setting::Unsupported(String::new()),
            Setting::Unsupported(raw) => Setting::Unsupported(raw),
        }
    }
    /// Records an edit, or forgets it when it matches the starting value.
    /// A choice of the wrong type is ignored.
    pub fn set(&mut self, behavior: Behavior, choice: Choice) {
        if !behavior.accepts(choice) {
            return;
        }
        if self.read(behavior).known() == Some(&choice) {
            self.edits.remove(&behavior);
        } else {
            self.edits.insert(behavior, choice);
        }
    }

    /// The variant COSMIC shows now (choosing the other never changes it).
    pub fn active_variant(&self) -> ThemeVariant {
        self.gaps.active_variant()
    }
    /// Both gaps after the draft's edits, inner already at least the
    /// outline width when it was edited (an untouched inner gap below the
    /// outline stays as it is).
    pub fn gaps(&self, variant: ThemeVariant) -> Setting<(u32, u32)> {
        self.gaps.gaps(variant)
    }
    /// Both gaps before the draft's edits.
    pub fn base_gaps(&self, variant: ThemeVariant) -> Setting<(u32, u32)> {
        match self.gaps.base_builder(variant) {
            Ok(builder) => Setting::Known(builder.gaps),
            Err(blocked) => Setting::Unsupported(
                self.base
                    .effective(&blocked.key)
                    .map(|raw| raw.as_str().trim().to_string())
                    .unwrap_or_default(),
            ),
        }
    }
    /// The active outline width of `variant`; the inner gap is never below it.
    pub fn outline_width(&self, variant: ThemeVariant) -> Setting<u32> {
        self.gaps.outline_width(variant)
    }
    /// Sets one gap of one variant; the other keeps its value exactly. An
    /// inner gap is raised to the outline before it is compared, so one
    /// that lands on the current value is no edit.
    pub fn set_gap(&mut self, variant: ThemeVariant, gap: Gap, value: u32) {
        let control = match gap {
            Gap::Outer => Control::OuterGap(variant),
            Gap::Inner => Control::InnerGap(variant),
        };
        self.gaps
            .set(control, ThemeValue::Number(value.min(MAX_GAP)));
    }

    /// The behavior part of the patch.
    pub fn behavior_plan(&self) -> Plan {
        let mut patch = DesktopPatch::new();
        let mut blocked = Vec::new();
        for (behavior, choice) in &self.edits {
            let key = behavior.key();
            let original = self.base.user(&key).cloned();
            match self.read(*behavior) {
                Setting::Known(_) => match choice.encode() {
                    Some(value) => patch.set(key, original, Some(value)),
                    None => blocked.push(Blocked {
                        key,
                        reason: "could not be encoded".into(),
                    }),
                },
                Setting::Unsupported(_) => blocked.push(Blocked {
                    key,
                    reason: "has a value this version doesn’t recognize".into(),
                }),
            }
        }
        if let Some(transition) = tiling_transition(&patch) {
            blocked.push(transition);
        }
        Plan { patch, blocked }
    }
    /// The gap part of the patch: builder gaps and the derived theme gaps,
    /// with every input the derivation read as a guard.
    pub fn gap_plan(&self) -> Plan {
        if !self.gaps.dirty() {
            return Plan::default();
        }
        self.gaps.plan()
    }
    /// The patch Apply writes as one Windows change, and edits that cannot
    /// be applied faithfully.
    pub fn plan(&self) -> Plan {
        let mut plan = self.behavior_plan();
        let gaps = self.gap_plan();
        plan.blocked.extend(gaps.blocked);
        if let Err(DesktopError::Overlap(keys)) = plan.patch.merge(gaps.patch) {
            plan.blocked.extend(keys.into_iter().map(|key| Blocked {
                key,
                reason: "overlaps another part of this change".into(),
            }));
        }
        plan
    }
    pub fn blocked(&self) -> Vec<Blocked> {
        if !self.dirty() {
            return Vec::new();
        }
        self.plan().blocked
    }

    /// Adopts outside changes to keys no edit depends on. A key an edit
    /// writes keeps its base so Apply detects the change; one that changed
    /// is listed as a conflict until resolved.
    pub fn refresh(&mut self, snapshot: Snapshot) {
        self.gaps.refresh(snapshot.clone());
        let mut base = snapshot;
        for behavior in self.edits.keys() {
            let key = behavior.key();
            let old = self.base.entry(&key).cloned().unwrap_or_default();
            let changed = base.user(&key) != old.user.as_ref();
            if changed && !self.conflicts.contains(&key) {
                self.conflicts.push(key.clone());
            } else if !changed {
                self.conflicts.retain(|conflict| *conflict != key);
            }
            base.insert(key, old);
        }
        self.conflicts
            .retain(|key| Behavior::of(key).is_some_and(|b| self.edits.contains_key(&b)));
        self.conflicts.sort();
        // Theme keys as the gap draft holds them.
        for key in AppearanceDraft::keys() {
            if let Some(entry) = self.gaps.snapshot().entry(&key) {
                base.insert(key, entry.clone());
            }
        }
        self.base = base;
    }
    /// Keeps every edit over the outside changes: the current values of
    /// the conflicting keys become the baseline, so Apply writes the edits
    /// over them and still refuses anything changed after this.
    pub fn keep_edits(&mut self, snapshot: Snapshot) {
        self.gaps.keep_edits(snapshot.clone());
        let keys = std::mem::take(&mut self.conflicts);
        self.base.adopt(&snapshot, &keys);
        self.refresh(snapshot);
    }
    /// Drops every edit that depends on a conflicting key, adopts the
    /// current values and keeps the other edits.
    pub fn resolve_conflicts(&mut self, snapshot: Snapshot, keys: &[ConfigKey]) {
        let conflicted: Vec<ConfigKey> =
            keys.iter().chain(self.conflicts.iter()).cloned().collect();
        self.edits
            .retain(|behavior, _| !conflicted.contains(&behavior.key()));
        self.conflicts.clear();
        for key in &conflicted {
            if let Some(entry) = snapshot.entry(key) {
                self.base.insert(key.clone(), entry.clone());
            }
        }
        self.gaps.resolve_conflicts(snapshot.clone(), keys);
        self.refresh(snapshot);
    }
    /// When the draft changes both tiling and its scope: the key that goes
    /// first (`first_tiling_step`) and a plan writing only that key. The
    /// other coupled key is guarded at its original user value, so the step
    /// never runs under a scope or tiling state changed meanwhile.
    pub fn first_step(&self) -> Option<(Behavior, Plan)> {
        let Some(Choice::Scope(target)) = self.edits.get(&Behavior::Scope) else {
            return None;
        };
        self.edits.get(&Behavior::Autotile)?;
        let first = first_tiling_step(*target);
        let other = match first {
            Behavior::Scope => Behavior::Autotile,
            _ => Behavior::Scope,
        };
        let mut single = Self::from_snapshot(self.base.clone());
        single.edits.insert(first, self.edits[&first]);
        let mut plan = single.behavior_plan();
        let key = other.key();
        plan.patch.guard(key.clone(), self.base.user(&key).cloned());
        Some((first, plan))
    }
    /// This draft without its edit to `behavior`, for after that edit was
    /// applied on its own. Every other edit keeps its desired value and
    /// original baseline, so a `refresh` still finds outside changes to
    /// them as conflicts.
    pub fn without(&self, behavior: Behavior) -> Self {
        let mut rest = self.clone();
        rest.edits.remove(&behavior);
        rest.conflicts.retain(|key| *key != behavior.key());
        rest
    }
}

// Validators: exact pinned types within the ranges the editor offers.
fn check_bool(value: &RawValue) -> Result<(), String> {
    decode_exact::<bool>(value).map(drop)
}
fn check_scope(value: &RawValue) -> Result<(), String> {
    decode_exact::<TileBehavior>(value).map(drop)
}
fn check_delay(value: &RawValue) -> Result<(), String> {
    (decode_exact::<u64>(value)? <= MAX_FOCUS_DELAY)
        .then_some(())
        .ok_or_else(|| format!("Focus delay must be at most {MAX_FOCUS_DELAY} ms"))
}
fn check_snap(value: &RawValue) -> Result<(), String> {
    (decode_exact::<u32>(value)? <= MAX_EDGE_SNAP)
        .then_some(())
        .ok_or_else(|| format!("Edge snapping must be at most {MAX_EDGE_SNAP} px"))
}

macro_rules! rule {
    ($namespace:expr, $key:literal => $validate:expr) => {
        KeyRule {
            namespace: Namespace::Exact($namespace),
            version: 1,
            key: $key,
            validate: $validate,
        }
    };
}

/// Every window behavior key the adapter may write. Gap keys are the
/// Appearance adapter's.
pub static RULES: [KeyRule; 9] = [
    rule!(COMPOSITOR, "autotile" => check_bool),
    rule!(COMPOSITOR, "autotile_behavior" => check_scope),
    rule!(COMPOSITOR, "active_hint" => check_bool),
    rule!(COMPOSITOR, "focus_follows_cursor" => check_bool),
    rule!(COMPOSITOR, "cursor_follows_focus" => check_bool),
    rule!(COMPOSITOR, "focus_follows_cursor_delay" => check_delay),
    rule!(COMPOSITOR, "edge_snap_threshold" => check_snap),
    rule!(TOOLKIT, "show_minimize" => check_bool),
    rule!(TOOLKIT, "show_maximize" => check_bool),
];

#[cfg(test)]
mod tests;
