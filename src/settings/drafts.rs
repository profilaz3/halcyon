//! Guarded native builders shared by completed settings actions and legacy
//! recovery. Temporary values preserve validation and outside-writer baselines.
use crate::desktop::appearance::{Blocked, Plan};
use crate::desktop::store::{
    ConfigKey, DesktopError, DesktopPatch, DesktopStore, Journal, LatestChange, Owner, Snapshot,
    WidgetPatch,
};
use crate::settings::desktop::Notice;
use crate::settings::system_appearance::describe;
use std::path::PathBuf;

/// Shown once a conflict is settled in favour of the draft's edits.
pub const KEEPING: &str = "Keeping your changes. Apply writes them over the outside change";

/// A page draft over a fixed set of COSMIC keys.
pub trait PageDraft: Sized {
    fn keys() -> Vec<ConfigKey>;
    fn load(store: &DesktopStore) -> Result<Self, DesktopError>;
    fn dirty(&self) -> bool;
    fn discard(&mut self);
    fn refresh(&mut self, snapshot: Snapshot);
    fn into_snapshot(self) -> Snapshot;
    fn resolve_conflicts(&mut self, snapshot: Snapshot, keys: &[ConfigKey]);
    /// Keeps the edits over outside changes; Apply then writes them.
    fn keep_edits(&mut self, snapshot: Snapshot);
    fn conflicts(&self) -> Vec<ConfigKey>;
    fn plan(&self) -> Plan;
    /// Inputs whose effective value in `fresh` differs from the draft's
    /// base although the store's user-layer check could pass (a changed
    /// system default). Checked just before Apply; such keys are handled
    /// like a store conflict.
    fn stale(&self, _fresh: &Snapshot) -> Vec<ConfigKey> {
        Vec::new()
    }
    /// Whether Apply rereads the keys to run `stale` first.
    fn rechecks() -> bool {
        false
    }
    /// The keys to watch for outside edits; drafts whose keys follow
    /// discovery (per-display entries) list the ones they read.
    fn watch_keys(&self) -> Vec<ConfigKey> {
        Self::keys()
    }
    /// Read-only checks of what Apply would make visible (such as chosen
    /// pictures), run before anything else; an error stops Apply and keeps
    /// the draft and the earlier Undo.
    fn preflight(&self) -> Result<(), String> {
        Ok(())
    }
    /// Takes fresh observations (never values, baselines or edits) from a
    /// freshly loaded draft before its snapshot refreshes this one.
    fn sync_metadata(&mut self, _fresh: &Self) {}
}

impl PageDraft for crate::desktop::windows::WindowsDraft {
    fn keys() -> Vec<ConfigKey> {
        Self::keys()
    }
    fn load(store: &DesktopStore) -> Result<Self, DesktopError> {
        Self::load(store)
    }
    fn dirty(&self) -> bool {
        self.dirty()
    }
    fn discard(&mut self) {
        self.discard();
    }
    fn refresh(&mut self, snapshot: Snapshot) {
        self.refresh(snapshot);
    }
    fn into_snapshot(self) -> Snapshot {
        self.into_snapshot()
    }
    fn resolve_conflicts(&mut self, snapshot: Snapshot, keys: &[ConfigKey]) {
        self.resolve_conflicts(snapshot, keys);
    }
    fn keep_edits(&mut self, snapshot: Snapshot) {
        self.keep_edits(snapshot);
    }
    fn conflicts(&self) -> Vec<ConfigKey> {
        self.conflicts()
    }
    fn plan(&self) -> Plan {
        self.plan()
    }
}

impl PageDraft for crate::desktop::workspaces::WorkspacesDraft {
    fn keys() -> Vec<ConfigKey> {
        Self::keys()
    }
    fn load(store: &DesktopStore) -> Result<Self, DesktopError> {
        Self::load(store)
    }
    fn dirty(&self) -> bool {
        self.dirty()
    }
    fn discard(&mut self) {
        self.discard();
    }
    fn refresh(&mut self, snapshot: Snapshot) {
        self.refresh(snapshot);
    }
    fn into_snapshot(self) -> Snapshot {
        self.into_snapshot()
    }
    fn resolve_conflicts(&mut self, snapshot: Snapshot, keys: &[ConfigKey]) {
        self.resolve_conflicts(snapshot, keys);
    }
    fn keep_edits(&mut self, snapshot: Snapshot) {
        self.keep_edits(snapshot);
    }
    fn conflicts(&self) -> Vec<ConfigKey> {
        self.conflicts().to_vec()
    }
    fn plan(&self) -> Plan {
        self.plan()
    }
    fn stale(&self, fresh: &Snapshot) -> Vec<ConfigKey> {
        self.stale(fresh)
    }
    fn rechecks() -> bool {
        true
    }
}

impl PageDraft for crate::desktop::window_rules::WindowRulesDraft {
    fn keys() -> Vec<ConfigKey> {
        Self::keys()
    }
    fn load(store: &DesktopStore) -> Result<Self, DesktopError> {
        Self::load(store)
    }
    fn dirty(&self) -> bool {
        self.dirty()
    }
    fn discard(&mut self) {
        self.discard();
    }
    fn refresh(&mut self, snapshot: Snapshot) {
        self.refresh(snapshot);
    }
    fn into_snapshot(self) -> Snapshot {
        self.into_snapshot()
    }
    fn resolve_conflicts(&mut self, snapshot: Snapshot, keys: &[ConfigKey]) {
        self.resolve_conflicts(snapshot, keys);
    }
    fn keep_edits(&mut self, snapshot: Snapshot) {
        self.keep_edits(snapshot);
    }
    fn conflicts(&self) -> Vec<ConfigKey> {
        self.conflicts().to_vec()
    }
    fn plan(&self) -> Plan {
        self.plan()
    }
    fn stale(&self, fresh: &Snapshot) -> Vec<ConfigKey> {
        self.stale(fresh)
    }
    fn rechecks() -> bool {
        true
    }
}

impl PageDraft for crate::desktop::shortcuts::ShortcutsDraft {
    fn keys() -> Vec<ConfigKey> {
        Self::keys()
    }
    fn load(store: &DesktopStore) -> Result<Self, DesktopError> {
        Self::load(store)
    }
    fn dirty(&self) -> bool {
        self.dirty()
    }
    fn discard(&mut self) {
        self.discard();
    }
    fn refresh(&mut self, snapshot: Snapshot) {
        self.refresh(snapshot);
    }
    fn into_snapshot(self) -> Snapshot {
        self.into_snapshot()
    }
    fn resolve_conflicts(&mut self, snapshot: Snapshot, keys: &[ConfigKey]) {
        self.resolve_conflicts(snapshot, keys);
    }
    fn keep_edits(&mut self, snapshot: Snapshot) {
        self.keep_edits(snapshot);
    }
    fn conflicts(&self) -> Vec<ConfigKey> {
        self.conflicts().to_vec()
    }
    fn plan(&self) -> Plan {
        self.plan()
    }
    fn stale(&self, fresh: &Snapshot) -> Vec<ConfigKey> {
        self.stale(fresh)
    }
    fn rechecks() -> bool {
        true
    }
}

impl PageDraft for crate::desktop::wallpaper::WallpaperDraft {
    fn keys() -> Vec<ConfigKey> {
        Self::keys()
    }
    fn load(store: &DesktopStore) -> Result<Self, DesktopError> {
        Self::load(store)
    }
    fn dirty(&self) -> bool {
        self.dirty()
    }
    fn discard(&mut self) {
        self.discard();
    }
    fn refresh(&mut self, snapshot: Snapshot) {
        self.refresh(snapshot);
    }
    fn into_snapshot(self) -> Snapshot {
        self.into_snapshot()
    }
    fn resolve_conflicts(&mut self, snapshot: Snapshot, keys: &[ConfigKey]) {
        self.resolve_conflicts(snapshot, keys);
    }
    fn keep_edits(&mut self, snapshot: Snapshot) {
        self.keep_edits(snapshot);
    }
    fn conflicts(&self) -> Vec<ConfigKey> {
        self.conflicts().to_vec()
    }
    fn plan(&self) -> Plan {
        self.plan()
    }
    fn stale(&self, fresh: &Snapshot) -> Vec<ConfigKey> {
        self.stale(fresh)
    }
    fn rechecks() -> bool {
        true
    }
    fn watch_keys(&self) -> Vec<ConfigKey> {
        self.watch_keys()
    }
    fn preflight(&self) -> Result<(), String> {
        self.check_sources()
    }
    fn sync_metadata(&mut self, fresh: &Self) {
        self.sync_metadata(fresh);
    }
}

impl PageDraft for crate::desktop::desktop_icons::DesktopIconsDraft {
    fn keys() -> Vec<ConfigKey> {
        Self::keys()
    }
    fn load(store: &DesktopStore) -> Result<Self, DesktopError> {
        Self::load(store)
    }
    fn dirty(&self) -> bool {
        self.dirty()
    }
    fn discard(&mut self) {
        self.discard();
    }
    fn refresh(&mut self, snapshot: Snapshot) {
        self.refresh(snapshot);
    }
    fn into_snapshot(self) -> Snapshot {
        self.into_snapshot()
    }
    fn resolve_conflicts(&mut self, snapshot: Snapshot, keys: &[ConfigKey]) {
        self.resolve_conflicts(snapshot, keys);
    }
    fn keep_edits(&mut self, snapshot: Snapshot) {
        self.keep_edits(snapshot);
    }
    fn conflicts(&self) -> Vec<ConfigKey> {
        self.conflicts().to_vec()
    }
    fn plan(&self) -> Plan {
        self.plan()
    }
    fn stale(&self, fresh: &Snapshot) -> Vec<ConfigKey> {
        self.stale(fresh)
    }
    fn rechecks() -> bool {
        true
    }
}

pub struct DraftEditor<D> {
    store: Option<Result<DesktopStore, String>>,
    draft: Option<Result<D, String>>,
    owner: Owner,
    /// The Undo label of this page's changes.
    label: &'static str,
    /// The persistent latest change, when this page made it.
    undo: Option<LatestChange>,
    /// The owner and label of a latest change another page made.
    elsewhere: Option<(Owner, String)>,
    pub notice: Option<Notice>,
    recovery: Option<Result<Journal, String>>,
    session_form_baseline: bool,
}

impl<D: PageDraft> DraftEditor<D> {
    pub fn new(owner: Owner, label: &'static str) -> Self {
        Self {
            store: None,
            draft: None,
            owner,
            label,
            undo: None,
            elsewhere: None,
            notice: None,
            recovery: None,
            session_form_baseline: false,
        }
    }
    /// First visit: choose the store and read values. Later visits refresh
    /// untouched values and keep the draft. Reads only.
    pub fn open(&mut self) {
        if self.store.is_some() {
            self.refresh();
            return;
        }
        self.store = Some(DesktopStore::from_env().map_err(|error| error.to_string()));
        self.reload(true);
    }
    #[cfg(test)]
    pub fn with_store(owner: Owner, label: &'static str, store: DesktopStore) -> Self {
        let mut editor = Self::new(owner, label);
        editor.store = Some(Ok(store));
        editor.reload(true);
        editor
    }
    pub fn opened(&self) -> bool {
        self.store.is_some()
    }
    pub fn store(&self) -> Option<&DesktopStore> {
        self.store.as_ref().and_then(|store| store.as_ref().ok())
    }
    pub fn unavailable(&self) -> Option<&str> {
        match (&self.store, &self.draft) {
            (Some(Err(error)), _) | (_, Some(Err(error))) => Some(error),
            _ => None,
        }
    }
    pub fn writable(&self) -> bool {
        self.store().is_some_and(DesktopStore::writable)
    }
    pub fn draft(&self) -> Option<&D> {
        self.draft.as_ref().and_then(|draft| draft.as_ref().ok())
    }
    /// Edits clear the last notice.
    pub fn edit(&mut self, change: impl FnOnce(&mut D)) {
        if let Some(Ok(draft)) = &mut self.draft {
            change(draft);
            self.notice = None;
        }
    }
    pub fn dirty(&self) -> bool {
        self.draft().is_some_and(PageDraft::dirty)
    }
    pub fn blocked(&self) -> Vec<Blocked> {
        match self.draft() {
            Some(draft) if draft.dirty() => draft.plan().blocked,
            _ => Vec::new(),
        }
    }
    pub fn recovery(&self) -> Option<&Result<Journal, String>> {
        self.recovery.as_ref()
    }
    /// This page's latest change, if it is the latest.
    pub fn latest(&self) -> Option<&LatestChange> {
        self.undo.as_ref()
    }
    /// A latest change another page made, while this page has none to undo.
    #[cfg(test)]
    pub fn elsewhere(&self) -> Option<&(Owner, String)> {
        self.elsewhere.as_ref().filter(|_| self.undo.is_none())
    }
    /// Undo is offered for this page's own latest change only, never with
    /// unapplied edits or while an interrupted change waits.
    pub fn can_undo(&self) -> bool {
        self.undo.is_some() && self.recovery.is_none() && !self.dirty() && self.writable()
    }
    pub fn can_apply(&self) -> bool {
        self.dirty() && self.writable() && self.recovery.is_none() && self.blocked().is_empty()
    }
    /// Existing user directories to watch for outside edits.
    pub fn watch_paths(&self) -> Vec<PathBuf> {
        let Some(store) = self.store() else {
            return Vec::new();
        };
        let keys = match self.draft() {
            Some(draft) => draft.watch_keys(),
            None => D::keys(),
        };
        let mut paths: Vec<PathBuf> = Vec::new();
        for key in keys {
            let path = store.user_path(&key);
            if path.parent().is_some_and(std::path::Path::is_dir) && !paths.contains(&path) {
                paths.push(path);
            }
        }
        paths
    }

    fn load(&self) -> Result<D, DesktopError> {
        let store = self
            .store()
            .ok_or_else(|| DesktopError::Io("Desktop settings unavailable".into()))?;
        D::load(store)
    }
    fn refresh_recovery(&mut self) {
        if let Some(store) = self.store() {
            let recovery = store
                .pending_recovery()
                .map_err(|error| error.to_string())
                .transpose();
            let latest = store.latest_change().ok().flatten();
            let owner = self.owner;
            self.elsewhere = latest
                .as_ref()
                .filter(|latest| latest.owner != owner)
                .map(|latest| (latest.owner, latest.label.clone()));
            self.undo = latest.filter(|latest| latest.owner == owner);
            self.recovery = recovery;
        }
    }
    pub fn reload(&mut self, rebase: bool) {
        if self.store().is_none() {
            return;
        }
        self.refresh_recovery();
        match self.load() {
            Ok(fresh) => match &mut self.draft {
                Some(Ok(draft)) if !rebase => {
                    draft.sync_metadata(&fresh);
                    if !self.session_form_baseline || draft.dirty() {
                        draft.refresh(fresh.into_snapshot());
                    }
                }
                _ => self.draft = Some(Ok(fresh)),
            },
            Err(error) => {
                if self.draft().is_some() {
                    self.notice = Some(Notice::failed(&error));
                } else {
                    self.draft = Some(Err(error.to_string()));
                }
            }
        }
    }
    /// Reopens the exact original draft, then observes current values.
    /// Refresh preserves edited keys' baselines, so outside changes remain
    /// conflicts rather than silently becoming a new starting point.
    pub(super) fn restore_session(&mut self, mut saved: D) -> Result<(), String> {
        let fresh = self.load().map_err(|error| error.to_string())?;
        saved.sync_metadata(&fresh);
        if !self.session_form_baseline || saved.dirty() {
            saved.refresh(fresh.into_snapshot());
        }
        self.draft = Some(Ok(saved));
        self.notice = Some(Notice::Done(super::session_drafts::RECOVERED.into()));
        Ok(())
    }

    /// An unsubmitted recovered form must still derive its first edit from
    /// the original snapshot. Release once its text is saved or cancelled.
    pub(super) fn keep_session_form_baseline(&mut self, keep: bool) {
        self.session_form_baseline = keep;
    }

    pub fn refresh(&mut self) {
        self.reload(false);
    }
    pub fn discard(&mut self) {
        if let Some(Ok(draft)) = &mut self.draft {
            draft.discard();
        }
        self.notice = None;
        self.reload(true);
    }
    /// Adopts current values for conflicting keys, dropping only the edits
    /// that depend on them.
    pub fn use_current(&mut self, keys: &[ConfigKey]) {
        match self.load() {
            Ok(fresh) => {
                if let Some(Ok(draft)) = &mut self.draft {
                    draft.sync_metadata(&fresh);
                    draft.resolve_conflicts(fresh.into_snapshot(), keys);
                }
                if keys.is_empty() {
                    self.notice = Some(Notice::Done("Using the current values".into()));
                }
            }
            Err(error) => self.notice = Some(Notice::failed(&error)),
        }
    }
    /// Keeps every edit over the outside changes; Apply then writes them.
    pub fn keep_edits(&mut self) {
        match self.load() {
            Ok(fresh) => {
                if let Some(Ok(draft)) = &mut self.draft {
                    draft.sync_metadata(&fresh);
                    draft.keep_edits(fresh.into_snapshot());
                }
                self.notice = Some(Notice::Done(KEEPING.into()));
            }
            Err(error) => self.notice = Some(Notice::failed(&error)),
        }
    }
    /// Applies the draft as one change owned by this page.
    pub fn apply(&mut self) -> bool {
        let Some(draft) = self.draft() else {
            return false;
        };
        let plan = draft.plan();
        if !plan.blocked.is_empty() || plan.patch.is_empty() {
            return false;
        }
        let label = self.label;
        self.write(&plan.patch, None, label, None, true)
    }
    /// Applies `patch`, one part of the draft, as a change owned by this
    /// page. On success `rest` (the draft without that part, still on its
    /// original baselines) becomes the draft and is refreshed, so outside
    /// changes to its keys stay conflicts instead of becoming its new base.
    pub fn apply_part(&mut self, patch: &DesktopPatch, label: &str, rest: D) -> bool {
        self.write(patch, None, label, Some(rest), true)
    }
    /// The same journal and Undo can include app preferences. A color-only
    /// operation retains the background draft and does not preflight it.
    pub fn apply_with_widgets(
        &mut self,
        patch: &DesktopPatch,
        widget: Option<&WidgetPatch>,
        label: &str,
        rest: D,
        include_draft: bool,
    ) -> bool {
        self.write(patch, widget, label, Some(rest), include_draft)
    }
    fn write(
        &mut self,
        patch: &DesktopPatch,
        widget: Option<&WidgetPatch>,
        label: &str,
        rest: Option<D>,
        include_draft: bool,
    ) -> bool {
        let Some(store) = self.store() else {
            return false;
        };
        // Read-only checks of what the change would show (chosen pictures).
        // A failure writes nothing and keeps the draft and earlier Undo.
        if include_draft && let Some(Err(reason)) = self.draft().map(PageDraft::preflight) {
            self.notice = Some(Notice::Error(reason));
            return false;
        }
        // The store checks user-layer bytes; a draft that also read a
        // system default rechecks it here. A failed reread is no pass.
        let stale = match self.draft() {
            Some(draft) if include_draft && D::rechecks() => {
                D::load(store).map(|fresh| draft.stale(&fresh.into_snapshot()))
            }
            _ => Ok(Vec::new()),
        };
        let result = match stale {
            Ok(stale) if stale.is_empty() => store.apply_owned(patch, widget, self.owner, label),
            Ok(stale) => Err(DesktopError::Conflict(stale)),
            Err(error) => Err(error),
        };
        match result {
            Ok(change) => {
                self.undo = Some(change);
                self.elsewhere = None;
                match rest {
                    Some(rest) => {
                        self.draft = Some(Ok(rest));
                        self.reload(false);
                    }
                    None => self.reload(true),
                }
                self.notice = Some(Notice::Done("Applied to your desktop".into()));
                true
            }
            Err(DesktopError::Conflict(keys)) => {
                self.use_current(&keys);
                let changed = capitalize(&describe(&keys));
                self.notice = Some(Notice::Error(if self.dirty() {
                    format!("{changed} changed outside this editor. Your other edits are kept.")
                } else {
                    format!("{changed} changed outside this editor. Nothing was applied.")
                }));
                false
            }
            Err(error) => {
                if error == DesktopError::RecoveryPending {
                    self.reload(false);
                } else {
                    self.refresh_recovery();
                }
                self.notice = Some(Notice::failed(&error));
                false
            }
        }
    }
    pub fn undo(&mut self) {
        let (Some(store), Some(change)) = (self.store(), self.undo.as_ref()) else {
            return;
        };
        if self.dirty() {
            return;
        }
        match store.undo_latest(&change.id) {
            Ok(_) => {
                self.undo = None;
                self.reload(true);
                self.notice = Some(Notice::Done("Previous settings restored".into()));
            }
            Err(DesktopError::Conflict(keys)) => {
                self.notice = Some(Notice::Error(format!(
                    "Can’t undo: {} changed since. Nothing was changed.",
                    describe(&keys)
                )));
            }
            Err(error) => {
                if error == DesktopError::RecoveryPending {
                    self.reload(false);
                } else {
                    self.refresh_recovery();
                }
                self.notice = Some(Notice::failed(&error));
            }
        }
    }
    pub fn recover(&mut self, restore: bool) {
        let Some(store) = self.store() else {
            return;
        };
        let result = if restore {
            store
                .recover()
                .map(|report| super::recovery::summary(&report))
        } else {
            store
                .discard_recovery()
                .map(|()| "Kept what’s on screen".to_string())
        };
        match result {
            Ok(message) => {
                self.undo = None;
                self.reload(false);
                self.notice = Some(Notice::Done(message));
            }
            Err(error) => self.notice = Some(Notice::failed(&error)),
        }
    }
    pub fn state_key(&self) -> String {
        format!(
            "{:?}:{:?}:{}:{}",
            self.notice,
            self.draft().map(PageDraft::conflicts),
            self.recovery.is_some(),
            self.undo
                .as_ref()
                .map(|undo| undo.id.as_str())
                .unwrap_or(""),
        )
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::store::tests::Fixture;
    use crate::desktop::window_rules::{CustomRule, WindowRulesDraft};
    use crate::desktop::windows::{Behavior, Choice, WindowsDraft};
    use crate::desktop::workspaces::{TypingAction, WorkspaceLayout, WorkspacesDraft};

    const COMP: &str = "com.system76.CosmicComp";

    #[test]
    fn auditfix_workspace_apply_and_resume_refuse_changed_system_defaults() {
        for resume in [false, true] {
            for changed in [
                "(workspace_mode:Global,workspace_layout:Horizontal,workspace_wraparound:true)",
                "(workspace_mode:OutputBound,future_member:42)",
            ] {
                let fixture = Fixture::new("auditfix-workspaces-inherited");
                fixture.system(COMP, "workspaces", "(workspace_mode:OutputBound)");
                let mut editor = DraftEditor::<WorkspacesDraft>::with_store(
                    Owner::Workspaces,
                    "Workspaces",
                    fixture.store(),
                );
                editor.edit(|draft| draft.set_wraparound(false));
                let saved = editor.draft().unwrap().clone();
                fixture.system(COMP, "workspaces", changed);
                if resume {
                    editor.restore_session(saved).unwrap();
                    assert_eq!(
                        editor.draft().unwrap().conflicts(),
                        &[crate::desktop::workspaces::key()]
                    );
                }
                let before = fixture.tree();
                assert!(!editor.apply());
                assert_eq!(fixture.tree(), before);
                assert!(fixture.read(COMP, "workspaces").is_none());
                assert!(matches!(editor.notice, Some(Notice::Error(_))));
            }
        }
    }

    /// An Apply-time conflict writes nothing, keeps the earlier Undo and
    /// says whether any edits remain. Workspaces edits one grouped key, so
    /// a conflict there leaves none.
    #[test]
    fn apply_conflict_notice_says_whether_edits_remain() {
        let fixture = Fixture::new("drafts-conflict-notice");
        let store = fixture.store();
        let mut spaces =
            DraftEditor::<WorkspacesDraft>::with_store(Owner::Workspaces, "Workspaces", store);
        spaces.edit(|draft| draft.set_wraparound(false));
        assert!(spaces.apply());
        let earlier = spaces.latest().cloned();
        spaces.edit(|draft| draft.set_layout(WorkspaceLayout::Horizontal));
        spaces.edit(|draft| draft.set_typing(TypingAction::OpenLauncher));
        fixture.user(
            COMP,
            "workspaces",
            "(workspace_mode:Global,workspace_layout:Vertical,action_on_typing:r#None,workspace_wraparound:false)",
        );
        let files = fixture.tree();
        assert!(!spaces.apply());
        assert_eq!(fixture.tree(), files, "nothing written");
        assert!(!spaces.dirty());
        assert_eq!(spaces.latest().cloned(), earlier);
        let Some(Notice::Error(text)) = &spaces.notice else {
            panic!("{:?}", spaces.notice);
        };
        assert_eq!(
            text,
            "Workspace settings changed outside this editor. Nothing was applied."
        );

        let mut windows =
            DraftEditor::<WindowsDraft>::with_store(Owner::Windows, "Windows", fixture.store());
        windows.edit(|draft| draft.set(Behavior::ActiveHint, Choice::Flag(false)));
        windows.edit(|draft| draft.set(Behavior::ShowMaximize, Choice::Flag(false)));
        fixture.user(COMP, "active_hint", "true");
        assert!(!windows.apply());
        assert!(windows.dirty());
        assert!(
            matches!(&windows.notice, Some(Notice::Error(text)) if text.contains("other edits are kept")),
            "{:?}",
            windows.notice
        );
    }

    thread_local! {
        static UNREADABLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// Floating-app rules whose pre-Apply reread fails on demand.
    #[derive(Clone)]
    struct Flaky(WindowRulesDraft);

    impl PageDraft for Flaky {
        fn keys() -> Vec<ConfigKey> {
            WindowRulesDraft::keys()
        }
        fn load(store: &DesktopStore) -> Result<Self, DesktopError> {
            if UNREADABLE.get() {
                return Err(DesktopError::Io("Sentinel reread failure".into()));
            }
            WindowRulesDraft::load(store).map(Self)
        }
        fn dirty(&self) -> bool {
            self.0.dirty()
        }
        fn discard(&mut self) {
            self.0.discard();
        }
        fn refresh(&mut self, snapshot: Snapshot) {
            self.0.refresh(snapshot);
        }
        fn into_snapshot(self) -> Snapshot {
            self.0.into_snapshot()
        }
        fn resolve_conflicts(&mut self, snapshot: Snapshot, keys: &[ConfigKey]) {
            self.0.resolve_conflicts(snapshot, keys);
        }
        fn keep_edits(&mut self, snapshot: Snapshot) {
            self.0.keep_edits(snapshot);
        }
        fn conflicts(&self) -> Vec<ConfigKey> {
            self.0.conflicts().to_vec()
        }
        fn plan(&self) -> Plan {
            self.0.plan()
        }
        fn stale(&self, fresh: &Snapshot) -> Vec<ConfigKey> {
            self.0.stale(fresh)
        }
        fn rechecks() -> bool {
            true
        }
    }

    mod wallpaper {
        use super::*;
        use crate::desktop::panels::Setting;
        use crate::desktop::wallpaper::{
            BgColor, BgSource, Displays, NAMESPACE, ScalingMode, Target, WallpaperDraft, list_key,
            output_key, same_key, shared_key,
        };
        use std::fs;
        use std::path::Path;

        fn entry(output: &str, source: &Path) -> String {
            format!(
                "(output: \"{output}\", source: Path(\"{}\"), filter_by_theme: false, rotation_frequency: 900)",
                source.display()
            )
        }
        fn seeded(name: &str, outputs: &str) -> Fixture {
            let fixture = Fixture::new(name);
            fs::write(fixture.0.join("outputs"), outputs).unwrap();
            fixture
        }
        fn editor(fixture: &Fixture) -> DraftEditor<WallpaperDraft> {
            DraftEditor::with_store(Owner::Workspaces, "Wallpaper", fixture.store())
        }
        fn source(draft: &WallpaperDraft, target: &Target) -> BgSource {
            draft.entry(target).unwrap().known().unwrap().source.clone()
        }

        /// A chosen picture that is gone, or a folder without pictures,
        /// stops Apply before anything is written; the draft and the
        /// earlier Undo stay. Pictures only in a subfolder count.
        #[test]
        fn preflight_failure_keeps_draft_and_undo() {
            let fixture = seeded("drafts-preflight", "DP-1\n");
            let pictures = fixture.0.join("pictures");
            fs::create_dir_all(pictures.join("empty")).unwrap();
            fs::create_dir_all(pictures.join("nested/sub")).unwrap();
            fs::write(pictures.join("nested/notes.txt"), "text").unwrap();
            fs::write(pictures.join("nested/sub/b.JPG"), "jpeg").unwrap();
            fs::write(pictures.join("a.png"), "png").unwrap();
            fixture.user(NAMESPACE, "all", &entry("all", &pictures.join("a.png")));
            let mut editor = editor(&fixture);
            editor.edit(|draft| draft.set_scaling(&Target::Shared, ScalingMode::Stretch));
            assert!(editor.apply());
            let earlier = editor.latest().cloned();
            assert!(earlier.is_some());
            for (chosen, reason) in [
                (pictures.join("gone.png"), "moved or deleted"),
                (pictures.join("empty"), "No pictures"),
            ] {
                editor.edit(|draft| draft.set_source(&Target::Shared, BgSource::Path(chosen)));
                let files = fixture.tree();
                assert!(!editor.apply(), "{reason}");
                assert_eq!(fixture.tree(), files, "nothing written");
                assert!(editor.dirty(), "the draft is kept");
                assert_eq!(editor.latest().cloned(), earlier, "earlier Undo kept");
                assert!(
                    matches!(&editor.notice, Some(Notice::Error(text)) if text.contains(reason)),
                    "{:?}",
                    editor.notice
                );
            }
            editor.edit(|draft| {
                draft.set_source(&Target::Shared, BgSource::Path(pictures.join("nested")))
            });
            assert!(editor.apply());
        }

        #[test]
        fn watch_paths_include_dynamic_output_keys() {
            let fixture = seeded("drafts-watch", "DP-1\nHDMI-1\n");
            fixture.user(NAMESPACE, "same-on-all", "false");
            let store = fixture.store();
            let editor = editor(&fixture);
            let paths = editor.watch_paths();
            for key in [
                shared_key(),
                output_key("DP-1").unwrap(),
                output_key("HDMI-1").unwrap(),
            ] {
                assert!(paths.contains(&store.user_path(&key)), "{key}");
            }
        }

        /// Refresh and Use current take fresh display discovery but never
        /// rebase an edit that is still pending on its original values.
        #[test]
        fn refresh_discovery_keeps_edits_and_original_baselines() {
            let fixture = seeded("drafts-discovery", "DP-1\n");
            let shared = entry("all", Path::new("/shared-a.png"));
            let own = entry("DP-1", Path::new("/own.png"));
            fixture.user(NAMESPACE, "all", &shared);
            fixture.user(NAMESPACE, "same-on-all", "false");
            fixture.user(NAMESPACE, "backgrounds", "[\"DP-1\"]");
            fixture.user(NAMESPACE, "output.DP-1", &own);
            let red = BgSource::Color(BgColor::Single([0.8, 0.1, 0.1]));
            let dp1 = Target::Output("DP-1".into());
            let mut editor = editor(&fixture);
            editor.edit(|draft| draft.set_source(&Target::Shared, red.clone()));
            editor.edit(|draft| draft.set_scaling(&dp1, ScalingMode::Stretch));
            // Outside: COSMIC Settings changes the shared background and a
            // display is connected.
            fixture.user(NAMESPACE, "all", &entry("all", Path::new("/shared-b.png")));
            fs::write(fixture.0.join("outputs"), "DP-1\nHDMI-1\n").unwrap();
            editor.refresh();
            let draft = editor.draft().unwrap();
            let hdmi = draft
                .display_views()
                .into_iter()
                .find(|view| view.name == "HDMI-1");
            assert!(hdmi.is_some_and(|view| view.connected));
            assert!(draft.watch_keys().contains(&output_key("HDMI-1").unwrap()));
            assert_eq!(source(draft, &Target::Shared), red, "the edit is kept");
            assert_eq!(
                draft.snapshot().user(&shared_key()).map(|raw| raw.as_str()),
                Some(shared.as_str()),
                "original baseline kept"
            );
            assert_eq!(draft.conflicts(), &[shared_key()]);

            // Use current for the shared background: its edit goes, the
            // independent DP-1 edit keeps its original baseline (an outside
            // change to it stays a conflict) and discovery is fresh.
            fixture.user(
                NAMESPACE,
                "output.DP-1",
                &entry("DP-1", Path::new("/other.png")),
            );
            fs::write(fixture.0.join("outputs"), "DP-1\nHDMI-1\nHDMI-2\n").unwrap();
            editor.use_current(&[shared_key()]);
            let draft = editor.draft().unwrap();
            assert_eq!(
                source(draft, &Target::Shared),
                BgSource::Path("/shared-b.png".into())
            );
            assert!(matches!(
                draft.entry(&dp1),
                Some(Setting::Known(entry)) if entry.scaling_mode == ScalingMode::Stretch
                    && entry.source == BgSource::Path("/own.png".into())
            ));
            assert_eq!(
                draft
                    .snapshot()
                    .user(&output_key("DP-1").unwrap())
                    .map(|raw| raw.as_str()),
                Some(own.as_str())
            );
            assert_eq!(draft.conflicts(), &[output_key("DP-1").unwrap()]);
            assert!(
                draft
                    .display_views()
                    .iter()
                    .any(|view| view.name == "HDMI-2" && view.connected)
            );
        }
        /// A listed display whose entry can't be read (here a folder where
        /// the file belongs).
        fn unreadable_fixture(name: &str) -> (Fixture, std::path::PathBuf) {
            let fixture = seeded(name, "DP-1\n");
            fixture.user(NAMESPACE, "all", &entry("all", Path::new("/shared.png")));
            fixture.user(NAMESPACE, "same-on-all", "false");
            fixture.user(NAMESPACE, "backgrounds", "[\"DP-1\"]");
            let file = fixture.store().user_path(&output_key("DP-1").unwrap());
            fs::create_dir_all(&file).unwrap();
            (fixture, file)
        }
        fn make_readable(file: &Path, text: &str) {
            if file.is_dir() {
                fs::remove_dir(file).unwrap();
            }
            fs::write(file, text).unwrap();
        }

        #[test]
        fn unreadable_display_entry_is_watched() {
            let (fixture, file) = unreadable_fixture("drafts-unreadable-watch");
            let editor = editor(&fixture);
            assert!(editor.draft().is_some());
            assert!(editor.watch_paths().contains(&file));
        }

        /// An edit of a display whose entry can't be read is not recorded,
        /// so it can't later adopt the value it never saw; independent edits
        /// are kept.
        #[test]
        fn unreadable_display_edit_never_adopts_a_later_baseline() {
            let (fixture, file) = unreadable_fixture("drafts-unreadable-edit");
            let red = BgSource::Color(BgColor::Single([0.8, 0.1, 0.1]));
            let dp1 = Target::Output("DP-1".into());
            let mut editor = editor(&fixture);
            editor.edit(|draft| draft.set_scaling(&dp1, ScalingMode::Stretch));
            editor.edit(|draft| draft.set_source(&Target::Shared, red.clone()));
            make_readable(&file, &entry("DP-1", Path::new("/own.png")));
            editor.refresh();
            let draft = editor.draft().unwrap();
            assert_eq!(source(draft, &Target::Shared), red, "independent edit kept");
            let plan = draft.plan();
            assert!(plan.blocked.is_empty(), "{:?}", plan.blocked);
            assert_eq!(
                plan.patch.keys().cloned().collect::<Vec<_>>(),
                vec![shared_key()]
            );
            assert!(matches!(
                draft.entry(&dp1),
                Some(Setting::Known(entry)) if entry.scaling_mode == ScalingMode::Zoom
            ));
        }

        /// An edit made on a readable entry keeps that baseline while the
        /// file is briefly unreadable and after it is readable again; a new
        /// member edit while it can't be read is not recorded.
        #[test]
        fn display_edit_keeps_its_baseline_through_an_unreadable_file() {
            let fixture = seeded("drafts-unreadable-baseline", "DP-1\n");
            let own = entry("DP-1", Path::new("/own.png"));
            fixture.user(NAMESPACE, "all", &entry("all", Path::new("/shared.png")));
            fixture.user(NAMESPACE, "same-on-all", "false");
            fixture.user(NAMESPACE, "backgrounds", "[\"DP-1\"]");
            fixture.user(NAMESPACE, "output.DP-1", &own);
            let key = output_key("DP-1").unwrap();
            let store = fixture.store();
            let file = store.user_path(&key);
            let dp1 = Target::Output("DP-1".into());
            let mut editor = editor(&fixture);
            editor.edit(|draft| draft.set_scaling(&dp1, ScalingMode::Stretch));
            fs::remove_file(&file).unwrap();
            fs::create_dir_all(&file).unwrap();
            editor.refresh();
            let draft = editor.draft().unwrap();
            assert!(draft.dirty());
            assert_eq!(draft.conflicts(), &[key.clone()]);
            assert_eq!(
                draft.snapshot().user(&key).map(|raw| raw.as_str()),
                Some(own.as_str())
            );
            assert!(
                draft
                    .plan()
                    .blocked
                    .iter()
                    .any(|blocked| blocked.key == key)
            );
            editor.edit(|draft| draft.set_rotation(&dp1, 600));
            make_readable(&file, &entry("DP-1", Path::new("/other.png")));
            editor.refresh();
            let draft = editor.draft().unwrap();
            assert_eq!(draft.conflicts(), &[key.clone()]);
            assert_eq!(
                draft.snapshot().user(&key).map(|raw| raw.as_str()),
                Some(own.as_str()),
                "original baseline kept"
            );
            assert!(matches!(
                draft.entry(&dp1),
                Some(Setting::Known(entry)) if entry.scaling_mode == ScalingMode::Stretch
                    && entry.rotation_frequency == 900
            ));
            let config = || crate::desktop::store::tests::tree(&fixture.0.join("config"));
            let files = config();
            assert!(matches!(
                store.apply(&draft.plan().patch),
                Err(DesktopError::Conflict(keys)) if keys == vec![key.clone()]
            ));
            assert_eq!(config(), files, "nothing written");
        }

        /// Separate would make a stale-listed entry that can't be read
        /// effective unchecked, so it is blocked, also after the file is
        /// repaired but before a refresh; Use shared for it clears the block.
        #[test]
        fn separate_blocks_an_unreadable_listed_entry() {
            let (fixture, file) = unreadable_fixture("drafts-unreadable-separate");
            fixture.user(NAMESPACE, "same-on-all", "true");
            let key = output_key("DP-1").unwrap();
            let mut page = editor(&fixture);
            page.edit(|draft| draft.set_displays(Displays::Separate));
            let blocked = page.draft().unwrap().plan().blocked;
            assert_eq!(
                blocked.iter().map(|b| b.key.clone()).collect::<Vec<_>>(),
                vec![key.clone()]
            );
            let files = fixture.tree();
            assert!(!page.apply());
            assert_eq!(fixture.tree(), files, "nothing written");
            assert!(file.is_dir(), "the unreadable file is kept");
            // Repaired outside, before the editor refreshes: still blocked.
            let picture = fixture.0.join("own.png");
            fs::write(&picture, "png").unwrap();
            make_readable(&file, &entry("DP-1", &picture));
            let files = fixture.tree();
            assert!(!page.apply());
            assert_eq!(fixture.tree(), files, "nothing written");
            // After a refresh it is a conflict with the absent baseline.
            page.refresh();
            assert_eq!(page.draft().unwrap().conflicts(), &[key.clone()]);
            assert!(!page.apply());
            assert_eq!(fixture.tree(), files, "nothing written");

            let (fixture, file) = unreadable_fixture("drafts-unreadable-separate-shared");
            fixture.user(NAMESPACE, "same-on-all", "true");
            let red = BgSource::Color(BgColor::Single([0.8, 0.1, 0.1]));
            let mut page = editor(&fixture);
            page.edit(|draft| draft.set_source(&Target::Shared, red.clone()));
            page.edit(|draft| draft.set_displays(Displays::Separate));
            assert!(!page.draft().unwrap().plan().blocked.is_empty());
            page.edit(|draft| draft.use_shared("DP-1"));
            let plan = page.draft().unwrap().plan();
            assert!(plan.blocked.is_empty(), "{:?}", plan.blocked);
            assert_eq!(
                plan.patch.keys().cloned().collect::<Vec<_>>(),
                vec![shared_key(), list_key(), same_key()]
            );
            // Use shared clears the per-key planner block, but a first
            // immutable snapshot cannot preserve a directory as raw key bytes.
            let native_before = crate::desktop::store::tests::tree(&fixture.0.join("config"));
            assert!(!page.apply());
            assert!(
                matches!(&page.notice, Some(Notice::Error(text)) if text.contains("preserve the desktop"))
            );
            assert_eq!(
                crate::desktop::store::tests::tree(&fixture.0.join("config")),
                native_before
            );
            assert!(
                file.is_dir(),
                "failed capture leaves the unreadable entry intact"
            );
            // An outside repair makes capture possible. This unlisted key is
            // still untouched by Apply; only the selected shared keys change.
            let repaired = entry("DP-1", Path::new("/repaired-outside.png"));
            make_readable(&file, &repaired);
            assert!(page.apply());
            let store = fixture.store();
            let value = |key: &ConfigKey| {
                fs::read_to_string(store.user_path(key))
                    .unwrap()
                    .split_whitespace()
                    .collect::<String>()
            };
            assert_eq!(value(&list_key()), "[]");
            assert_eq!(value(&same_key()), "false");
            assert_eq!(source(page.draft().unwrap(), &Target::Shared), red);
            assert_eq!(
                fs::read_to_string(&file).unwrap(),
                repaired,
                "the externally repaired key is kept"
            );
        }
    }

    /// When the reread a recheck needs fails, Apply writes nothing, keeps
    /// the earlier Undo and the draft, and says why.
    #[test]
    fn failed_recheck_applies_nothing() {
        let fixture = Fixture::new("drafts-recheck-error");
        let mut editor =
            DraftEditor::<Flaky>::with_store(Owner::WindowRules, "Floating apps", fixture.store());
        let rule = |appid: &str| CustomRule {
            appid: appid.into(),
            title: String::new(),
            enabled: true,
        };
        editor.edit(|draft| draft.0.save(None, rule("^a$")).unwrap());
        assert!(editor.apply());
        let earlier = editor.latest().cloned();
        editor.edit(|draft| draft.0.save(None, rule("^b$")).unwrap());
        let files = fixture.tree();
        UNREADABLE.set(true);
        let applied = editor.apply();
        UNREADABLE.set(false);
        assert!(!applied, "a failed recheck is not a pass");
        assert_eq!(fixture.tree(), files, "nothing written");
        assert_eq!(editor.latest().cloned(), earlier, "earlier Undo kept");
        assert!(editor.dirty(), "the draft is kept");
        assert!(
            matches!(&editor.notice, Some(Notice::Error(text)) if text.contains("Sentinel reread failure")),
            "{:?}",
            editor.notice
        );
    }
}
