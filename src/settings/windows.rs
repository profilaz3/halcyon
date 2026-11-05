//! Windows editor state: tiling, window gaps, window controls and focus.
//! The draft survives navigation and is written only by Apply, as one
//! change owned by this page (gaps included), through the shared store.
use crate::desktop::appearance::{Blocked, ThemeVariant};
#[cfg(test)]
use crate::desktop::store::LatestChange;
use crate::desktop::store::{DesktopStore, Journal, Owner};
use crate::desktop::windows::{
    Behavior, Choice, EDGE_SNAP_ON, Gap, MAX_EDGE_SNAP, MAX_FOCUS_DELAY, TileBehavior,
    WindowsDraft, is_transition,
};
use crate::settings::desktop::Notice;
use crate::settings::drafts::DraftEditor;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Autotile(bool),
    Scope(TileBehavior),
    /// Which theme's gaps the controls show. Never changes the mode.
    Variant(ThemeVariant),
    Gap(Gap, u32),
    Flag(Behavior, bool),
    DelayInput(String),
    SubmitNumber(bool),
    EdgeSnap(bool),
    SnapInput(String),
    Advanced(bool),
    Apply,
    /// Applies only the tiling key that goes first when tiling and its
    /// scope both change, keeping the rest of the draft.
    ApplyFirst,
    Discard,
    Undo,
    Refresh,
    UseCurrent,
    /// Keeps the edits over a value changed outside this editor.
    KeepMine,
    Recover,
    KeepRecovery,
}

impl Message {
    /// Messages that write desktop settings or the change journal.
    pub fn writes(&self) -> bool {
        matches!(
            self,
            Self::Apply | Self::ApplyFirst | Self::Undo | Self::Recover | Self::KeepRecovery
        )
    }
}

/// Typed numeric text that is not (yet) a value the draft holds.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Typed {
    pub delay: Option<String>,
    pub snap: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct SavedWindows {
    pub(super) draft: WindowsDraft,
    pub(super) variant: ThemeVariant,
    pub(super) typed: Typed,
}

pub struct WindowsEditor {
    pub(super) core: DraftEditor<WindowsDraft>,
    /// The theme whose gaps are shown; starts at the one in use.
    pub variant: ThemeVariant,
    pub advanced: bool,
    typed: Typed,
}

impl Default for WindowsEditor {
    fn default() -> Self {
        Self {
            core: DraftEditor::new(Owner::Windows, "Windows"),
            variant: ThemeVariant::default(),
            advanced: false,
            typed: Typed::default(),
        }
    }
}

impl WindowsEditor {
    pub fn open(&mut self) {
        self.core.open();
        // Gaps are edited for the mode in use; there is no mode picker.
        if let Some(draft) = self.core.draft() {
            self.variant = draft.active_variant();
        }
    }
    #[cfg(test)]
    pub fn with_store(store: DesktopStore) -> Self {
        let core = DraftEditor::with_store(Owner::Windows, "Windows", store);
        let variant = core
            .draft()
            .map(WindowsDraft::active_variant)
            .unwrap_or_default();
        Self {
            core,
            variant,
            advanced: false,
            typed: Typed::default(),
        }
    }
    pub(super) fn session_draft(&self) -> Option<SavedWindows> {
        self.dirty()
            .then(|| {
                self.draft().cloned().map(|draft| SavedWindows {
                    draft,
                    variant: self.variant,
                    typed: self.typed.clone(),
                })
            })
            .flatten()
    }
    pub(super) fn restore_session(&mut self, saved: SavedWindows) -> Result<(), String> {
        if saved
            .typed
            .delay
            .as_ref()
            .is_some_and(|text| text.len() > 8)
            || saved.typed.snap.as_ref().is_some_and(|text| text.len() > 8)
        {
            return Err("The saved window draft has invalid input text".into());
        }
        self.core.restore_session(saved.draft)?;
        self.variant = saved.variant;
        self.typed = saved.typed;
        Ok(())
    }

    pub fn opened(&self) -> bool {
        self.core.opened()
    }
    pub fn store(&self) -> Option<&DesktopStore> {
        self.core.store()
    }
    pub fn unavailable(&self) -> Option<&str> {
        self.core.unavailable()
    }
    pub fn writable(&self) -> bool {
        self.core.writable()
    }
    pub fn draft(&self) -> Option<&WindowsDraft> {
        self.core.draft()
    }
    pub fn notice(&self) -> Option<&Notice> {
        self.core.notice.as_ref()
    }
    /// Edits or typed text not applied yet.
    pub fn dirty(&self) -> bool {
        self.core.dirty() || self.typed_error().is_some()
    }
    pub fn blocked(&self) -> Vec<Blocked> {
        self.core.blocked()
    }
    pub fn recovery(&self) -> Option<&Result<Journal, String>> {
        self.core.recovery()
    }
    #[cfg(test)]
    pub fn latest(&self) -> Option<&LatestChange> {
        self.core.latest()
    }
    pub fn can_undo(&self) -> bool {
        self.core.can_undo() && self.typed_error().is_none()
    }
    /// Empty or out-of-range typed numbers cannot be applied.
    pub fn can_apply(&self) -> bool {
        self.core.can_apply()
    }
    /// The tiling key that goes first when the draft changes both tiling
    /// and its scope.
    pub fn first_step(&self) -> Option<Behavior> {
        self.draft()?.first_step().map(|(first, _)| first)
    }
    /// The draft is held back only by the tiling transition, and its first
    /// step can go now. A conflict Refresh listed waits for Use current or
    /// Discard first.
    pub fn can_apply_first(&self) -> bool {
        self.core.writable()
            && self.core.recovery().is_none()
            && self.typed_error().is_none()
            && self.draft().is_some_and(|draft| {
                let blocked = draft.blocked();
                !blocked.is_empty()
                    && blocked.iter().all(is_transition)
                    && draft.conflicts().is_empty()
                    && draft
                        .first_step()
                        .is_some_and(|(_, plan)| plan.blocked.is_empty())
            })
    }
    pub fn delay_text(&self) -> Option<&str> {
        self.typed.delay.as_deref()
    }
    pub fn snap_text(&self) -> Option<&str> {
        self.typed.snap.as_deref()
    }
    /// The first typed number that cannot be applied, with how to fix it.
    pub fn typed_error(&self) -> Option<String> {
        if self.typed.delay.is_some() {
            return Some(format!("Focus delay: use 0–{MAX_FOCUS_DELAY} ms"));
        }
        if self.typed.snap.is_some() {
            return Some(format!("Snap distance: use 1–{MAX_EDGE_SNAP} px"));
        }
        None
    }
    pub fn watch_paths(&self) -> Vec<PathBuf> {
        self.core.watch_paths()
    }
    pub fn state_key(&self) -> String {
        format!(
            "{}:{:?}:{:?}:{:?}:{:?}:{}",
            self.core.state_key(),
            self.draft()
                .map(|draft| (draft.edits().collect::<Vec<_>>(), draft.gap_edits())),
            self.variant,
            self.typed,
            self.advanced,
            self.dirty(),
        )
    }
    pub fn refresh(&mut self) {
        self.core.refresh();
    }

    pub fn update(&mut self, message: Message) {
        let variant = self.variant;
        match message {
            Message::Autotile(on) => self
                .core
                .edit(|draft| draft.set(Behavior::Autotile, Choice::Flag(on))),
            Message::Scope(scope) => self
                .core
                .edit(|draft| draft.set(Behavior::Scope, Choice::Scope(scope))),
            Message::Variant(variant) => self.variant = variant,
            Message::Gap(gap, value) => self.core.edit(|draft| draft.set_gap(variant, gap, value)),
            Message::Flag(behavior, on) => self
                .core
                .edit(|draft| draft.set(behavior, Choice::Flag(on))),
            Message::DelayInput(text) => {
                self.typed.delay =
                    Some(text.chars().filter(char::is_ascii_digit).take(6).collect());
            }
            Message::SubmitNumber(delay) => {
                if delay {
                    if let Some(value) = self
                        .typed
                        .delay
                        .as_deref()
                        .and_then(|s| s.parse::<u64>().ok())
                        .filter(|v| *v <= MAX_FOCUS_DELAY)
                    {
                        self.typed.delay = None;
                        self.core
                            .edit(|draft| draft.set(Behavior::FocusDelay, Choice::Millis(value)));
                        self.core.apply();
                    }
                } else if let Some(value) = self
                    .typed
                    .snap
                    .as_deref()
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|v| (1..=MAX_EDGE_SNAP).contains(v))
                {
                    self.typed.snap = None;
                    self.core
                        .edit(|draft| draft.set(Behavior::EdgeSnap, Choice::Pixels(value)));
                    self.core.apply();
                }
            }
            Message::EdgeSnap(on) => {
                self.typed.snap = None;
                let restore = self
                    .draft()
                    .and_then(|draft| draft.base_value(Behavior::EdgeSnap).known().copied())
                    .filter(|choice| *choice != Choice::Pixels(0));
                let choice = match (on, restore) {
                    (false, _) => Choice::Pixels(0),
                    (true, Some(previous)) => previous,
                    (true, None) => Choice::Pixels(EDGE_SNAP_ON),
                };
                self.core
                    .edit(|draft| draft.set(Behavior::EdgeSnap, choice));
            }
            Message::SnapInput(text) => {
                self.typed.snap = Some(text.chars().filter(char::is_ascii_digit).take(4).collect());
            }
            Message::Advanced(open) => self.advanced = open,
            Message::Apply => {
                if self.can_apply() {
                    self.core.apply();
                }
            }
            Message::ApplyFirst => self.apply_first(),
            Message::Discard => {
                self.typed = Typed::default();
                self.core.discard();
            }
            Message::Undo => {
                if self.can_undo() {
                    self.core.undo();
                }
            }
            Message::Refresh => self.core.refresh(),
            Message::UseCurrent => self.core.use_current(&[]),
            Message::KeepMine => self.core.keep_edits(),
            Message::Recover => self.core.recover(true),
            Message::KeepRecovery => self.core.recover(false),
        }
    }

    /// Applies the first tiling step on its own. The other edits stay in
    /// the draft on their original baselines, for the next Apply.
    fn apply_first(&mut self) {
        if !self.can_apply_first() {
            return;
        }
        let Some((first, plan, rest)) = self.draft().and_then(|draft| {
            let (first, plan) = draft.first_step()?;
            Some((first, plan, draft.without(first)))
        }) else {
            return;
        };
        let (label, done) = match first {
            Behavior::Scope => ("Windows tiling scope", "Tiling scope"),
            _ => ("Windows automatic tiling", "Automatic tiling"),
        };
        if self.core.apply_part(&plan.patch, label, rest) {
            self.core.notice = Some(Notice::Done(format!(
                "{done} applied. Apply again for the rest"
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::store::tests::Fixture;

    const COMP: &str = "com.system76.CosmicComp";

    #[test]
    fn typed_numbers_that_are_not_values_block_apply_and_survive_refresh() {
        let fixture = Fixture::new("windows-editor-typed");
        let mut editor = WindowsEditor::with_store(fixture.store());
        editor.update(Message::Flag(Behavior::FocusFollowsCursor, true));
        editor.update(Message::DelayInput("99999".into()));
        editor.update(Message::SubmitNumber(true));
        assert_eq!(fixture.read(COMP, "focus_follows_cursor_delay"), None);
        editor.update(Message::Refresh);
        assert_eq!(editor.delay_text(), Some("99999"));
        editor.update(Message::Apply);
        assert_eq!(
            fixture.read(COMP, "focus_follows_cursor").as_deref(),
            Some("true")
        );
        assert_eq!(
            editor.delay_text(),
            Some("99999"),
            "unrelated valid action preserves the form"
        );
        editor.update(Message::DelayInput("400".into()));
        assert_eq!(
            fixture.read(COMP, "focus_follows_cursor_delay"),
            None,
            "valid prefixes are still local"
        );
        editor.update(Message::SubmitNumber(true));
        assert_eq!(
            fixture.read(COMP, "focus_follows_cursor_delay").as_deref(),
            Some("400")
        );
        editor.update(Message::Undo);
        assert_eq!(fixture.read(COMP, "focus_follows_cursor_delay"), None);
        assert_eq!(
            fixture.read(COMP, "focus_follows_cursor").as_deref(),
            Some("true")
        );
    }

    fn dark_gaps(fixture: &Fixture) -> std::path::PathBuf {
        fixture
            .0
            .join("config/cosmic/com.system76.CosmicTheme.Dark.Builder/v2/gaps")
    }

    /// A draft that changes tiling and its scope applies one of them
    /// first so the scope in between is PerWorkspace: the scope toward
    /// PerWorkspace, tiling toward Global. Only that key is written, and
    /// the other must still hold its original value: an outside change to
    /// it writes nothing and keeps the earlier Undo.
    #[test]
    fn first_tiling_step_keeps_the_scope_per_workspace_in_between() {
        for (tiled, scope, first) in [
            (false, TileBehavior::Global, Behavior::Scope),
            (true, TileBehavior::PerWorkspace, Behavior::Autotile),
        ] {
            let fixture = Fixture::new("windows-editor-first-step");
            fixture.user(COMP, "autotile", &tiled.to_string());
            fixture.user(COMP, "autotile_behavior", &format!("{scope:?}"));
            let target = match scope {
                TileBehavior::Global => TileBehavior::PerWorkspace,
                TileBehavior::PerWorkspace => TileBehavior::Global,
            };
            let mut editor = WindowsEditor::with_store(fixture.store());
            editor.update(Message::Scope(target));
            editor.update(Message::Autotile(!tiled));
            assert!(!editor.can_apply(), "both at once are refused");
            editor.update(Message::Apply);
            assert_eq!(fixture.read(COMP, "autotile"), Some(tiled.to_string()));
            assert!(editor.can_apply_first());
            assert_eq!(editor.first_step(), Some(first));
            editor.update(Message::ApplyFirst);
            let (autotile, behavior) = match first {
                Behavior::Scope => (tiled, target),
                _ => (!tiled, scope),
            };
            assert_eq!(
                fixture.read(COMP, "autotile"),
                Some(autotile.to_string()),
                "{first:?} first"
            );
            assert_eq!(
                fixture.read(COMP, "autotile_behavior"),
                Some(format!("{behavior:?}"))
            );
            assert_eq!(behavior, TileBehavior::PerWorkspace, "in between");
            let draft = editor.draft().unwrap();
            assert!(!draft.edited(first) && draft.dirty());
            assert!(editor.can_apply());
            editor.update(Message::Apply);
            assert_eq!(fixture.read(COMP, "autotile"), Some((!tiled).to_string()));
            assert_eq!(
                fixture.read(COMP, "autotile_behavior"),
                Some(format!("{target:?}"))
            );
        }

        // Toward Global tiling goes first, guarded by the PerWorkspace scope.
        let fixture = Fixture::new("windows-editor-first-step-guard");
        fixture.user(COMP, "autotile", "true");
        fixture.user(COMP, "autotile_behavior", "PerWorkspace");
        let mut editor = WindowsEditor::with_store(fixture.store());
        editor.update(Message::Flag(Behavior::ShowMinimize, false));
        editor.update(Message::Apply);
        let earlier = editor.latest().cloned().unwrap();
        editor.update(Message::Scope(TileBehavior::Global));
        editor.update(Message::Autotile(false));
        // Someone sets Global outside; this page has not seen it yet.
        fixture.user(COMP, "autotile_behavior", "Global");
        let files = fixture.tree();
        assert!(editor.can_apply_first());
        editor.update(Message::ApplyFirst);
        assert_eq!(fixture.tree(), files, "no step under an unexpected scope");
        assert!(
            matches!(editor.notice(), Some(Notice::Error(text)) if text.contains("Tiling scope changed")),
            "{:?}",
            editor.notice()
        );
        assert_eq!(editor.latest(), Some(&earlier), "the earlier Undo is kept");
    }

    /// Keep my change writes the edits over the outside values, the gap
    /// included, and writes nothing until Apply.
    #[test]
    fn keep_my_change_writes_edits_over_outside_values() {
        let fixture = Fixture::new("windows-editor-keep-mine");
        let mut editor = WindowsEditor::with_store(fixture.store());
        editor.update(Message::Flag(Behavior::FocusFollowsCursor, true));
        editor.update(Message::Gap(Gap::Outer, 12));
        fixture.user(COMP, "focus_follows_cursor", "false");
        std::fs::create_dir_all(dark_gaps(&fixture).parent().unwrap()).unwrap();
        std::fs::write(dark_gaps(&fixture), "(5, 9)").unwrap();
        editor.update(Message::Refresh);
        let conflicts = editor.draft().unwrap().conflicts();
        assert!(
            conflicts.contains(&Behavior::FocusFollowsCursor.key()) && conflicts.len() >= 2,
            "{conflicts:?}"
        );
        let files = fixture.tree();
        editor.update(Message::KeepMine);
        let draft = editor.draft().unwrap();
        assert!(draft.conflicts().is_empty());
        assert!(draft.edited(Behavior::FocusFollowsCursor));
        assert!(draft.gap_edited(ThemeVariant::Dark));
        assert_eq!(fixture.tree(), files, "keeping writes nothing");
        editor.update(Message::Apply);
        assert_eq!(
            fixture.read(COMP, "focus_follows_cursor").as_deref(),
            Some("true")
        );
        let gaps = std::fs::read_to_string(dark_gaps(&fixture)).unwrap();
        assert!(gaps.contains("12"), "the kept gap is written: {gaps}");
    }

    /// The first step keeps every other edit with its original baseline,
    /// so outside changes to them stay conflicts after it, whether Refresh
    /// saw them (the step waits) or not (the next Apply writes nothing).
    #[test]
    fn first_tiling_step_keeps_remaining_baselines() {
        let fixture = Fixture::new("windows-editor-first-step-rest");
        fixture.user(COMP, "autotile", "false");
        fixture.user(COMP, "autotile_behavior", "Global");
        let mut editor = WindowsEditor::with_store(fixture.store());
        editor.update(Message::Scope(TileBehavior::PerWorkspace));
        editor.update(Message::Autotile(true));
        editor.update(Message::Flag(Behavior::ActiveHint, false));
        editor.update(Message::Flag(Behavior::FocusFollowsCursor, true));
        editor.update(Message::Gap(Gap::Outer, 12));

        // A conflict Refresh already listed holds the first step back.
        fixture.user(COMP, "focus_follows_cursor", "false");
        editor.update(Message::Refresh);
        assert_eq!(
            editor.draft().unwrap().conflicts(),
            [Behavior::FocusFollowsCursor.key()]
        );
        assert!(!editor.can_apply_first());
        let files = fixture.tree();
        editor.update(Message::ApplyFirst);
        assert_eq!(fixture.tree(), files, "the message path holds it back too");
        editor.update(Message::UseCurrent);
        assert!(!editor.draft().unwrap().edited(Behavior::FocusFollowsCursor));

        // Changes nobody refreshed: the outline flag and the Dark gaps.
        fixture.user(COMP, "active_hint", "true");
        std::fs::create_dir_all(dark_gaps(&fixture).parent().unwrap()).unwrap();
        std::fs::write(dark_gaps(&fixture), "(5, 9)").unwrap();
        editor.update(Message::ApplyFirst);
        assert_eq!(
            fixture.read(COMP, "autotile_behavior").as_deref(),
            Some("PerWorkspace")
        );
        let draft = editor.draft().unwrap();
        assert!(draft.edited(Behavior::Autotile) && draft.edited(Behavior::ActiveHint));
        assert!(draft.gap_edited(ThemeVariant::Dark));
        let conflicts = draft.conflicts();
        assert!(
            conflicts.contains(&Behavior::ActiveHint.key()) && conflicts.len() == 2,
            "outside values are not a new base: {conflicts:?}"
        );
        let files = fixture.tree();
        editor.update(Message::Apply);
        assert_eq!(fixture.tree(), files, "the outside values stay");
        assert_eq!(
            std::fs::read_to_string(dark_gaps(&fixture)).unwrap(),
            "(5, 9)"
        );
        // As on every page, the conflicting edits give way; tiling remains.
        let draft = editor.draft().unwrap();
        assert!(draft.edited(Behavior::Autotile) && !draft.edited(Behavior::ActiveHint));
        assert!(!draft.gap_edited(ThemeVariant::Dark));
        editor.update(Message::Apply);
        assert_eq!(fixture.read(COMP, "autotile").as_deref(), Some("true"));
        assert_eq!(fixture.read(COMP, "active_hint").as_deref(), Some("true"));
    }
}
