//! Shortcuts editor state: one draft of the custom shortcuts and paused
//! commands, a search and filter, one name/combination/command form and
//! at most one pending question (replace a combination, or whether a
//! built-in default comes back). Nothing is ever run.
//!
//! A question, and every answer already given in the same operation, holds
//! only for the draft it was derived from. When the draft changes under it
//! (Refresh, reopening, Use current, an Apply conflict), the answers are
//! dropped and the question is derived again from what is there now.
use crate::desktop::shortcuts::{Ask, Binding, Choice, Ident, Kind, ShortcutsDraft, Target};
use crate::desktop::store::{DesktopStore, Journal, Owner};
use crate::settings::desktop::Notice;
use crate::settings::drafts::DraftEditor;
use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;

mod recording;
pub use recording::guard as recording_view;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    All,
    #[default]
    Custom,
    Builtin,
}

/// Small, predictable groups for the desktop's own shortcuts. Labels come
/// from our pinned COSMIC action reader; unknown actions stay discoverable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Apps,
    Windows,
    Workspaces,
    Media,
    System,
    Other,
}

impl Category {
    pub const ALL: [Self; 6] = [
        Self::Apps,
        Self::Windows,
        Self::Workspaces,
        Self::Media,
        Self::System,
        Self::Other,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Apps => "Apps & launcher",
            Self::Windows => "Windows & tiling",
            Self::Workspaces => "Workspaces & displays",
            Self::Media => "Sound, media & brightness",
            Self::System => "Session & accessibility",
            Self::Other => "Other shortcuts",
        }
    }

    pub fn for_row(row: &crate::desktop::shortcuts::Row) -> Self {
        let label = match &row.kind {
            Kind::Builtin { label, .. } | Kind::Override { label } => label.as_str(),
            _ => row.name.as_str(),
        };
        let lower = label.to_lowercase();
        if lower.contains("workspace") || lower.contains("display") {
            Self::Workspaces
        } else if lower.contains("window")
            || lower.contains("tiling")
            || lower.starts_with("tile ")
            || lower == "full screen"
            || lower.contains("group")
        {
            Self::Windows
        } else if lower.starts_with("open ") {
            Self::Apps
        } else if [
            "volume",
            "mute",
            "microphone",
            "track",
            "play",
            "brightness",
        ]
        .iter()
        .any(|word| lower.contains(word))
        {
            Self::Media
        } else if [
            "session",
            "screen",
            "log out",
            "power",
            "suspend",
            "input source",
            "touchpad",
            "zoom",
            "debug",
        ]
        .iter()
        .any(|word| lower.contains(word))
        {
            Self::System
        } else {
            Self::Other
        }
    }
}

/// The draft a question was derived from. Its answers count only while
/// the draft is still exactly this.
#[derive(Clone, PartialEq)]
struct Basis(ShortcutsDraft);

impl fmt::Debug for Basis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Basis")
    }
}

const CHANGED: &str = "Shortcuts changed meanwhile, so nothing was changed. Check and try again.";

/// The open add/edit form.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CommandForm {
    /// The command being edited: running, or paused.
    pub editing: Option<Target>,
    /// Its combination as first shown, kept exactly while unchanged.
    original: Option<Binding>,
    pub name: String,
    pub combo: String,
    pub command: String,
    #[serde(skip)]
    pub error: Option<String>,
    /// The question Save is waiting on.
    #[serde(skip)]
    pub ask: Option<Ask>,
    #[serde(skip)]
    choice: Choice,
    #[serde(skip)]
    basis: Option<Basis>,
}

impl CommandForm {
    /// The combination typed, or why it isn't one. An unchanged
    /// combination keeps its exact original binding.
    pub fn binding(&self) -> Result<Binding, String> {
        if let Some(original) = &self.original
            && self.combo.trim() == original.to_string()
        {
            return Ok(original.clone());
        }
        Binding::parse_new(&self.combo)
    }
}

/// A row operation waiting on a question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowOp {
    Remove(Target),
    Resume(Ident),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    pub op: RowOp,
    pub ask: Ask,
    choice: Choice,
    basis: Basis,
}

/// An answer to the pending question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Replace,
    TurnBackOn,
    KeepOff,
    Cancel,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Search(String),
    Filter(Filter),
    Category(Category),
    Add,
    Edit(Target),
    Name(String),
    Combo(String),
    Record,
    CancelRecording,
    Recorded(Result<String, String>),
    Command(String),
    Save,
    Cancel,
    /// Answers the form's question, or a row's.
    Answer(Answer),
    /// Pause (`false`) or resume a custom command.
    Running(Ident, bool),
    /// Turn a built-in default off or back on.
    Default(Ident, bool),
    Remove(Target),
    RemoveCopy(usize),
    /// Remove the custom entry so the built-in default runs again.
    Restore(Ident),
    Expand(Option<Ident>),
    Apply,
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
            Self::Apply | Self::Undo | Self::Recover | Self::KeepRecovery
        )
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct SavedShortcuts {
    pub(super) draft: ShortcutsDraft,
    pub(super) form: Option<CommandForm>,
}

pub struct ShortcutsEditor {
    pub(super) core: DraftEditor<ShortcutsDraft>,
    pub search: String,
    pub filter: Filter,
    pub form: Option<CommandForm>,
    pub pending: Option<Pending>,
    pub expanded: Option<Ident>,
    pub expanded_categories: BTreeSet<Category>,
    recording: bool,
    pub recording_error: Option<String>,
}

impl Default for ShortcutsEditor {
    fn default() -> Self {
        Self {
            core: DraftEditor::new(Owner::Shortcuts, "Shortcuts"),
            search: String::new(),
            filter: Filter::default(),
            form: None,
            pending: None,
            expanded: None,
            expanded_categories: BTreeSet::new(),
            recording: false,
            recording_error: None,
        }
    }
}

impl ShortcutsEditor {
    #[cfg(test)]
    pub fn with_store(store: DesktopStore) -> Self {
        Self {
            core: DraftEditor::with_store(Owner::Shortcuts, "Shortcuts", store),
            ..Self::default()
        }
    }
    pub fn open(&mut self) {
        self.cancel_recording();
        self.core.open();
        self.revalidate();
    }
    pub(super) fn session_draft(&self) -> Option<SavedShortcuts> {
        (self.dirty() || self.form.is_some())
            .then(|| {
                self.draft().cloned().map(|draft| SavedShortcuts {
                    draft,
                    form: self.form.clone(),
                })
            })
            .flatten()
    }
    pub(super) fn restore_session(&mut self, saved: SavedShortcuts) -> Result<(), String> {
        self.cancel_recording();
        if saved.form.as_ref().is_some_and(|form| {
            [&form.name, &form.combo, &form.command]
                .iter()
                .any(|text| text.len() > 256 * 1024)
        }) {
            return Err("The saved shortcut form is too large".into());
        }
        self.core.keep_session_form_baseline(saved.form.is_some());
        self.core.restore_session(saved.draft)?;
        self.form = saved.form;
        self.pending = None;
        self.revalidate();
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
    pub fn draft(&self) -> Option<&ShortcutsDraft> {
        self.core.draft()
    }
    pub fn notice(&self) -> Option<&Notice> {
        self.core.notice.as_ref()
    }
    pub fn dirty(&self) -> bool {
        self.core.dirty()
    }
    /// A form or a question is open; Apply and Undo wait for it.
    pub fn busy(&self) -> bool {
        self.form.is_some() || self.pending.is_some()
    }
    pub fn recording(&self) -> bool {
        self.recording && self.form.is_some()
    }
    pub fn cancel_recording(&mut self) {
        self.recording = false;
        self.recording_error = None;
    }
    pub fn recovery(&self) -> Option<&Result<Journal, String>> {
        self.core.recovery()
    }
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn latest(&self) -> Option<&crate::desktop::LatestChange> {
        self.core.latest()
    }
    pub fn can_undo(&self) -> bool {
        self.core.can_undo() && !self.busy()
    }
    pub fn can_apply(&self) -> bool {
        self.core.can_apply() && !self.busy()
    }
    pub fn watch_paths(&self) -> Vec<PathBuf> {
        self.core.watch_paths()
    }
    pub fn state_key(&self) -> String {
        format!(
            "{}:{:?}:{:?}:{:?}:{}:{:?}:{:?}:{:?}:{}:{:?}",
            self.core.state_key(),
            self.draft().map(|draft| draft.rows()),
            self.form,
            self.pending,
            self.search,
            self.filter,
            self.expanded,
            self.expanded_categories,
            self.recording,
            self.recording_error,
        )
    }
    pub fn refresh(&mut self) {
        self.core.refresh();
        self.revalidate();
    }
    fn form_mut(&mut self, change: impl FnOnce(&mut CommandForm)) {
        if let Some(form) = &mut self.form {
            change(form);
            form.error = None;
            form.ask = None;
            form.choice = Choice::default();
            form.basis = None;
        }
    }

    pub fn update(&mut self, message: Message) {
        // While listening, keys can only set the combination or cancel.
        // Never let a queued field/Save message mutate an active recording.
        if self.recording()
            && matches!(
                message,
                Message::Name(_) | Message::Combo(_) | Message::Command(_) | Message::Save
            )
        {
            return;
        }
        self.handle(message);
        self.revalidate();
        if self.form.is_none() {
            self.cancel_recording();
            self.core.keep_session_form_baseline(false);
        }
    }

    fn handle(&mut self, message: Message) {
        match message {
            Message::Search(text) => self.search = text,
            Message::Filter(filter) => self.filter = filter,
            Message::Category(category) => {
                if !self.expanded_categories.remove(&category) {
                    self.expanded_categories.insert(category);
                }
            }
            Message::Add => {
                self.cancel_recording();
                self.pending = None;
                self.form = Some(CommandForm::default());
            }
            Message::Edit(target) => {
                self.cancel_recording();
                let row = self.draft().and_then(|draft| {
                    draft.rows().into_iter().find(|row| {
                        row.target() == target && matches!(row.kind, Kind::Command { .. })
                    })
                });
                if let Some(row) = row
                    && let Kind::Command { command, .. } = &row.kind
                {
                    self.pending = None;
                    self.form = Some(CommandForm {
                        editing: Some(target),
                        original: Some(row.binding.clone()),
                        name: row.binding.description.clone().unwrap_or_default(),
                        combo: row.binding.to_string(),
                        command: command.clone(),
                        ..CommandForm::default()
                    });
                }
            }
            Message::Name(text) => self.form_mut(|form| form.name = text),
            Message::Combo(text) => self.form_mut(|form| form.combo = text),
            Message::Record => {
                if self.form.as_ref().is_some_and(|form| form.ask.is_none()) {
                    self.recording = true;
                    self.recording_error = None;
                }
            }
            Message::CancelRecording => self.cancel_recording(),
            Message::Recorded(result) => {
                if self.recording() {
                    match result {
                        Ok(combo) => {
                            self.cancel_recording();
                            self.form_mut(|form| form.combo = combo);
                        }
                        Err(error) => self.recording_error = Some(error),
                    }
                }
            }
            Message::Command(text) => self.form_mut(|form| form.command = text),
            Message::Save => self.save(),
            Message::Cancel => {
                self.form = None;
                self.pending = None;
            }
            Message::Answer(answer) => self.answer(answer),
            Message::Running(ident, on) => {
                if on {
                    self.row_op(RowOp::Resume(ident), Choice::default());
                } else {
                    self.edit(|draft| draft.set_running(ident, false, Choice::default()));
                }
            }
            Message::Default(ident, on) => self.edit(|draft| draft.set_default(ident, on)),
            Message::Remove(target) => self.row_op(RowOp::Remove(target), Choice::default()),
            Message::RemoveCopy(index) => self.edit(|draft| draft.remove_copy(index)),
            Message::Restore(ident) => self.edit(|draft| {
                draft.remove(
                    Target::Entry(ident),
                    Choice {
                        resume: Some(true),
                        ..Choice::default()
                    },
                )
            }),
            Message::Expand(ident) => self.expanded = ident,
            Message::Apply => {
                if self.can_apply() {
                    self.core.apply();
                }
            }
            Message::Discard => {
                self.form = None;
                self.pending = None;
                self.core.discard();
            }
            Message::Undo => {
                if self.can_undo() {
                    self.core.undo();
                }
            }
            Message::Refresh => self.core.refresh(),
            Message::UseCurrent => {
                self.pending = None;
                self.core.use_current(&[]);
            }
            Message::KeepMine => {
                self.pending = None;
                self.core.keep_edits();
            }
            Message::Recover => self.core.recover(true),
            Message::KeepRecovery => self.core.recover(false),
        }
    }

    /// Runs `change` on a copy of the draft, which stays as it was.
    fn probe(
        &self,
        change: impl FnOnce(&mut ShortcutsDraft) -> Result<(), Ask>,
    ) -> Result<ShortcutsDraft, Ask> {
        let mut next = self
            .draft()
            .cloned()
            .ok_or_else(|| Ask::Refused("Shortcuts aren’t available.".into()))?;
        change(&mut next)?;
        Ok(next)
    }
    /// Runs `change` on a copy of the draft; the draft takes it only if it
    /// went through. A question or refusal leaves the draft as it was.
    fn try_edit(
        &mut self,
        change: impl FnOnce(&mut ShortcutsDraft) -> Result<(), Ask>,
    ) -> Result<(), Ask> {
        let next = self.probe(change)?;
        self.core.edit(|draft| *draft = next);
        Ok(())
    }
    fn edit(&mut self, change: impl FnOnce(&mut ShortcutsDraft) -> Result<(), Ask>) {
        self.pending = None;
        if let Err(ask) = self.try_edit(change) {
            self.core.notice = Some(Notice::Error(match ask {
                Ask::Refused(reason) => reason,
                _ => "This shortcut changed meanwhile; nothing was changed.".into(),
            }));
        }
    }
    fn basis(&self) -> Option<Basis> {
        self.draft().cloned().map(Basis)
    }
    fn run_row(draft: &mut ShortcutsDraft, op: &RowOp, choice: Choice) -> Result<(), Ask> {
        match *op {
            RowOp::Remove(target) => draft.remove(target, choice),
            RowOp::Resume(ident) => draft.set_running(ident, true, choice),
        }
    }
    fn run_save(draft: &mut ShortcutsDraft, form: &CommandForm, choice: Choice) -> Result<(), Ask> {
        let binding = form.binding().map_err(Ask::Refused)?;
        draft.save_command(form.editing, &form.name, &binding, &form.command, choice)
    }
    fn row_op(&mut self, op: RowOp, choice: Choice) {
        let result = self.try_edit(|draft| Self::run_row(draft, &op, choice));
        self.pending = None;
        match result {
            Ok(()) => {}
            Err(Ask::Refused(reason)) => self.core.notice = Some(Notice::Error(reason)),
            Err(ask) => {
                if let Some(basis) = self.basis() {
                    self.pending = Some(Pending {
                        op,
                        ask,
                        choice,
                        basis,
                    });
                }
            }
        }
    }
    fn answer(&mut self, answer: Answer) {
        let apply = |choice: &mut Choice| match answer {
            Answer::Replace => choice.replace = true,
            Answer::TurnBackOn => choice.resume = Some(true),
            Answer::KeepOff => choice.resume = Some(false),
            Answer::Cancel => {}
        };
        let basis = self.basis();
        if let Some(form) = &mut self.form
            && form.ask.is_some()
        {
            if answer == Answer::Cancel {
                form.ask = None;
                form.choice = Choice::default();
                form.basis = None;
                return;
            }
            if form.basis != basis {
                // Not the draft this question was asked about.
                self.revalidate();
                return;
            }
            apply(&mut form.choice);
            self.save();
            return;
        }
        if let Some(mut pending) = self.pending.take() {
            if answer == Answer::Cancel {
                return;
            }
            if Some(&pending.basis) != basis.as_ref() {
                self.pending = Some(pending);
                self.revalidate();
                return;
            }
            apply(&mut pending.choice);
            self.row_op(pending.op, pending.choice);
        }
    }
    fn save(&mut self) {
        let Some(form) = self.form.clone() else {
            return;
        };
        let result = self.try_edit(|draft| Self::run_save(draft, &form, form.choice));
        let basis = self.basis();
        let Some(form) = &mut self.form else {
            return;
        };
        match result {
            Ok(()) => self.form = None,
            Err(Ask::Refused(reason)) => {
                form.error = Some(reason);
                form.ask = None;
                form.choice = Choice::default();
                form.basis = None;
            }
            Err(ask) => {
                form.ask = Some(ask);
                form.basis = basis;
            }
        }
    }
    /// An open question whose draft changed is derived again with no
    /// answers: the new question is shown, or, when none is needed any
    /// more, nothing happens and the form or row says so. Typed form text
    /// stays.
    fn revalidate(&mut self) {
        let basis = self.basis();
        if let Some(form) = self.form.clone()
            && form.ask.is_some()
            && form.basis != basis
        {
            let result = self.probe(|draft| Self::run_save(draft, &form, Choice::default()));
            if let Some(form) = &mut self.form {
                form.choice = Choice::default();
                match result {
                    Err(Ask::Refused(reason)) => {
                        form.ask = None;
                        form.basis = None;
                        form.error = Some(reason);
                    }
                    Err(ask) => {
                        form.ask = Some(ask);
                        form.basis = basis.clone();
                    }
                    Ok(_) => {
                        form.ask = None;
                        form.basis = None;
                        form.error = Some(CHANGED.into());
                    }
                }
            }
        }
        if let Some(pending) = self.pending.clone()
            && Some(&pending.basis) != basis.as_ref()
        {
            let result = self.probe(|draft| Self::run_row(draft, &pending.op, Choice::default()));
            self.pending = None;
            match (result, basis) {
                (Err(Ask::Refused(reason)), _) => self.core.notice = Some(Notice::Error(reason)),
                (Err(ask), Some(basis)) => {
                    self.pending = Some(Pending {
                        op: pending.op,
                        ask,
                        choice: Choice::default(),
                        basis,
                    });
                }
                _ => self.core.notice = Some(Notice::Error(CHANGED.into())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::shortcuts::{NAMESPACE, PAUSED_NAMESPACE};
    use crate::desktop::store::tests::Fixture;

    #[test]
    fn custom_shortcuts_open_first_and_builtins_group_by_action() {
        let fixture = Fixture::new("shortcuts-groups");
        fixture.system(
            NAMESPACE,
            "defaults",
            r#"{
            (modifiers: [Super], key: "t"): System(Terminal),
            (modifiers: [Super], key: "q"): Close,
            (modifiers: [Super], key: "2"): Workspace(2),
            (modifiers: [], key: "XF86AudioMute"): System(Mute),
            (modifiers: [Super], key: "Escape"): System(LockScreen)
        }"#,
        );
        let editor = ShortcutsEditor::with_store(fixture.store());
        assert_eq!(editor.filter, Filter::Custom);
        assert!(editor.expanded_categories.is_empty());
        let rows = editor.draft().unwrap().rows();
        for (name, category) in [
            ("Open terminal", Category::Apps),
            ("Close window", Category::Windows),
            ("Go to workspace 2", Category::Workspaces),
            ("Mute sound", Category::Media),
            ("Lock screen", Category::System),
        ] {
            let row = rows.iter().find(|row| row.name == name).unwrap();
            assert_eq!(Category::for_row(row), category, "{name}");
        }
    }

    #[test]
    fn recording_only_changes_form_and_keeps_replace_confirmation() {
        let fixture = Fixture::new("shortcuts-recorder-conflict");
        fixture.system(
            NAMESPACE,
            "defaults",
            r#"{(modifiers: [Super], key: "t"): System(Terminal)}"#,
        );
        let mut editor = ShortcutsEditor::with_store(fixture.store());
        editor.update(Message::Add);
        editor.update(Message::Name("Notes".into()));
        editor.update(Message::Command("sentinel-notes".into()));
        editor.update(Message::Combo("Ctrl+Alt+n".into()));
        editor.update(Message::Record);
        editor.update(Message::Name("must not reach the field".into()));
        editor.update(Message::Save);
        assert!(editor.recording());
        assert_eq!(editor.form.as_ref().unwrap().name, "Notes");
        assert!(!editor.dirty());
        editor.update(Message::Recorded(Err("Try a modifier".into())));
        assert!(editor.recording());
        assert_eq!(editor.form.as_ref().unwrap().combo, "Ctrl+Alt+n");
        editor.update(Message::CancelRecording);
        assert!(!editor.recording());
        editor.update(Message::Recorded(Ok("Ctrl+Alt+x".into())));
        assert_eq!(
            editor.form.as_ref().unwrap().combo,
            "Ctrl+Alt+n",
            "a late capture cannot overwrite a cancelled form"
        );
        editor.update(Message::Record);
        editor.update(Message::Recorded(Ok("Super+t".into())));
        assert!(!editor.recording());
        assert!(!editor.dirty(), "recording does not add to the draft");
        editor.update(Message::Save);
        assert!(matches!(
            editor.form.as_ref().unwrap().ask,
            Some(Ask::Replace { .. })
        ));
        assert!(!editor.can_apply() && !editor.dirty());
        editor.update(Message::Record);
        assert!(
            !editor.recording(),
            "answer the conflict before changing keys"
        );
        editor.update(Message::Answer(Answer::Cancel));
        editor.update(Message::Record);
        editor.update(Message::Recorded(Ok("Ctrl+Alt+n".into())));
        editor.update(Message::Save);
        assert!(editor.form.is_none() && editor.can_apply());
        assert!(
            fixture.read(NAMESPACE, "custom").is_none(),
            "recording and Add to draft never apply desktop shortcuts"
        );
        editor.update(Message::Discard);
        assert!(!editor.dirty() && !editor.recording());
    }

    /// Replace and Cancel through the form, the question when a default
    /// would come back, and a pause that survives reopening with its
    /// command and an Undo that restores both keys.
    #[test]
    fn collision_replace_cancel_and_pause_across_restart() {
        let fixture = Fixture::new("shortcuts-editor");
        fixture.system(
            NAMESPACE,
            "defaults",
            r#"{(modifiers: [Super], key: "t"): System(Terminal)}"#,
        );
        let mut editor = ShortcutsEditor::with_store(fixture.store());
        editor.update(Message::Add);
        editor.update(Message::Name("Shell".into()));
        editor.update(Message::Combo("super+T".into()));
        editor.update(Message::Command("sentinel-shell --x".into()));
        editor.update(Message::Save);
        assert!(matches!(
            editor.form.as_ref().unwrap().ask,
            Some(Ask::Replace { ref current, .. }) if current == "Open terminal"
        ));
        assert!(!editor.dirty() && !editor.can_apply());
        editor.update(Message::Answer(Answer::Cancel));
        assert!(!editor.dirty(), "Cancel writes nothing into the draft");
        assert!(
            editor.form.is_some(),
            "the form stays for another combination"
        );
        editor.update(Message::Save);
        editor.update(Message::Answer(Answer::Replace));
        assert!(editor.form.is_none() && editor.dirty());
        editor.update(Message::Apply);
        assert!(editor.latest().is_some(), "{:?}", editor.notice());

        // Removing it asks whether Terminal comes back.
        let ident = Binding::parse("Super+T").unwrap().ident();
        editor.update(Message::Remove(Target::Entry(ident)));
        assert!(matches!(
            editor.pending.as_ref().unwrap().ask,
            Ask::Resume { ref default, .. } if default == "Open terminal"
        ));
        assert!(!editor.can_apply() && !editor.can_undo());
        editor.update(Message::Answer(Answer::Cancel));
        assert!(editor.pending.is_none() && !editor.dirty());

        // Pause, apply, and reopen as if after a restart.
        editor.update(Message::Running(ident, false));
        editor.update(Message::Apply);
        let reopened = ShortcutsEditor::with_store(fixture.store());
        assert!(reopened.can_undo(), "Undo after reopening");
        let rows = reopened.draft().unwrap().rows();
        let row = rows.iter().find(|row| row.ident() == ident).unwrap();
        assert_eq!(
            row.kind,
            Kind::Command {
                command: "sentinel-shell --x".into(),
                running: false,
                displaced: None
            }
        );
        let mut reopened = reopened;
        reopened.update(Message::Undo);
        assert!(
            !fixture
                .read(PAUSED_NAMESPACE, "disabled_commands")
                .unwrap_or_default()
                .contains("sentinel"),
            "both keys restored together"
        );
        assert!(
            fixture
                .read(NAMESPACE, "custom")
                .unwrap()
                .contains("Spawn(\"sentinel-shell --x\")")
        );
    }

    const PAUSED_J: &str = r#"[(binding: (modifiers: [Super, Ctrl], key: "j", description: Some("Mail app")), command: "sentinel-mail"), (binding: (modifiers: [Super], key: "t", description: Some("Shell")), command: "sentinel-shell"), (binding: (modifiers: [Super, Ctrl], key: "k", description: Some("Mine")), command: "sentinel-mine")]"#;
    const OCCUPIED: &str = r#"{(modifiers: [Super, Ctrl], key: "j"): Minimize, (modifiers: [Super, Ctrl], key: "k", description: Some("Other")): Spawn("sentinel-other")}"#;

    /// The paused row whose combination something else now uses.
    fn paused_row(editor: &ShortcutsEditor, combo: &str) -> crate::desktop::shortcuts::Row {
        let ident = Binding::parse(combo).unwrap().ident();
        editor
            .draft()
            .unwrap()
            .rows()
            .into_iter()
            .find(|row| {
                row.ident() == ident && matches!(row.kind, Kind::Command { running: false, .. })
            })
            .unwrap_or_else(|| panic!("no paused row at {combo}"))
    }

    /// Edit and Remove on a paused row whose keys another custom entry or a
    /// built-in default now uses act on the paused command only: Remove
    /// keeps the occupant byte for byte, Edit opens the paused command, and
    /// saving it back over the occupant asks Replace even without a new
    /// combination. The occupant's own Edit/Remove keep the paused record.
    #[test]
    fn displaced_paused_rows_edit_and_remove_only_their_command() {
        let fixture = Fixture::new("shortcuts-displaced");
        fixture.system(
            NAMESPACE,
            "defaults",
            r#"{(modifiers: [Super], key: "t"): System(Terminal)}"#,
        );
        fixture.user(NAMESPACE, "custom", OCCUPIED);
        fixture.user(PAUSED_NAMESPACE, "disabled_commands", PAUSED_J);
        let mut editor = ShortcutsEditor::with_store(fixture.store());
        let base = editor.draft().unwrap().base_custom().unwrap();
        let same_custom = |editor: &ShortcutsEditor| {
            let now = editor.draft().unwrap().custom().unwrap();
            now.len() == base.len()
                && now
                    .iter()
                    .zip(&base)
                    .all(|(a, b)| a.binding_raw == b.binding_raw && a.action_raw == b.action_raw)
        };

        // Remove the paused Mail app (Super+Ctrl+J now minimizes).
        let row = paused_row(&editor, "Super+Ctrl+J");
        editor.update(Message::Remove(row.target()));
        assert!(editor.pending.is_none(), "{:?}", editor.pending);
        assert!(same_custom(&editor), "the occupant is kept byte for byte");
        let paused = editor.draft().unwrap().paused().unwrap();
        assert!(
            paused.iter().all(|p| p.command != "sentinel-mail") && paused.len() == 2,
            "only the paused record goes: {paused:?}"
        );
        editor.update(Message::Discard);

        // Rename it: the form shows the paused command, and saving it
        // would take Super+Ctrl+J from Minimize, so it asks.
        let row = paused_row(&editor, "Super+Ctrl+J");
        editor.update(Message::Edit(row.target()));
        assert_eq!(editor.form.as_ref().unwrap().command, "sentinel-mail");
        editor.update(Message::Name("Mail".into()));
        editor.update(Message::Save);
        assert!(
            matches!(
                editor.form.as_ref().unwrap().ask,
                Some(Ask::Replace { ref current, .. }) if current == "Minimize window"
            ),
            "{:?}",
            editor.form
        );
        assert!(!editor.dirty() && same_custom(&editor));
        editor.update(Message::Answer(Answer::Cancel));
        assert!(!editor.dirty(), "Cancel changes nothing");
        editor.update(Message::Cancel);

        // The same for a paused command a built-in default displaced.
        let row = paused_row(&editor, "Super+T");
        editor.update(Message::Edit(row.target()));
        assert_eq!(editor.form.as_ref().unwrap().command, "sentinel-shell");
        editor.update(Message::Command("sentinel-shell --new".into()));
        editor.update(Message::Save);
        assert!(
            matches!(
                editor.form.as_ref().unwrap().ask,
                Some(Ask::Replace { ref current, .. }) if current == "Open terminal"
            ),
            "{:?}",
            editor.form
        );
        assert!(!editor.dirty(), "the default isn't turned off silently");
        editor.update(Message::Answer(Answer::Replace));
        assert!(editor.form.is_none() && editor.dirty());
        let custom = editor.draft().unwrap().custom().unwrap();
        let t = Binding::parse("Super+T").unwrap().ident();
        assert!(custom.iter().any(|e| e.ident() == t && e.is_disable()));
        assert!(
            editor
                .draft()
                .unwrap()
                .paused()
                .unwrap()
                .iter()
                .any(|p| p.command == "sentinel-shell --new")
        );
        editor.update(Message::Discard);

        // Super+Ctrl+K runs another command; its paused namesake opens as
        // itself, and the running one keeps the paused record when edited
        // or removed.
        let row = paused_row(&editor, "Super+Ctrl+K");
        editor.update(Message::Edit(row.target()));
        assert_eq!(editor.form.as_ref().unwrap().command, "sentinel-mine");
        assert_eq!(editor.form.as_ref().unwrap().name, "Mine");
        editor.update(Message::Cancel);
        let k = Binding::parse("Super+Ctrl+K").unwrap().ident();
        let running = editor
            .draft()
            .unwrap()
            .rows()
            .into_iter()
            .find(|row| row.ident() == k && matches!(row.kind, Kind::Command { running: true, .. }))
            .unwrap();
        editor.update(Message::Edit(running.target()));
        assert_eq!(editor.form.as_ref().unwrap().command, "sentinel-other");
        editor.update(Message::Name("Other app".into()));
        editor.update(Message::Save);
        assert!(editor.form.is_none());
        editor.update(Message::Remove(running.target()));
        assert!(editor.pending.is_none());
        let draft = editor.draft().unwrap();
        assert!(draft.custom().unwrap().iter().all(|e| e.ident() != k));
        assert!(
            draft
                .paused()
                .unwrap()
                .iter()
                .any(|p| p.command == "sentinel-mine"),
            "the paused record is not consumed"
        );
    }

    /// An answer approves only the question it was given for, against the
    /// lists that question was derived from. After an outside change
    /// (Refresh or Use current), an open question — and any answer already
    /// given earlier in the same operation — is not reused: the question is
    /// derived again and names what is there now. The typed form stays.
    #[test]
    fn stale_answers_never_approve_changed_inputs() {
        let fixture = Fixture::new("shortcuts-stale-answer");
        fixture.system(
            NAMESPACE,
            "defaults",
            r#"{(modifiers: [Super], key: "t"): System(Terminal), (modifiers: [Super], key: "m"): Maximize, (modifiers: [Super], key: "q"): Close}"#,
        );
        let backup_t = r#"{(modifiers: [Super], key: "t", description: Some("Backup")): Spawn("backup-tool")}"#;
        let mut editor = ShortcutsEditor::with_store(fixture.store());
        let replaces = |editor: &ShortcutsEditor, name: &str| {
            matches!(
                editor.form.as_ref().and_then(|form| form.ask.as_ref()),
                Some(Ask::Replace { current, .. }) if current == name
            )
        };

        // 1. Refresh while a Replace question is open.
        editor.update(Message::Add);
        editor.update(Message::Name("Shell".into()));
        editor.update(Message::Combo("Super+T".into()));
        editor.update(Message::Command("sentinel-shell".into()));
        editor.update(Message::Save);
        assert!(replaces(&editor, "Open terminal"));
        fixture.user(NAMESPACE, "custom", backup_t);
        editor.update(Message::Refresh);
        assert!(
            !editor.dirty() && replaces(&editor, "the command “Backup”"),
            "asked again about the new occupant: {:?}",
            editor.form
        );
        assert_eq!(editor.form.as_ref().unwrap().command, "sentinel-shell");
        editor.update(Message::Answer(Answer::Replace));
        assert!(editor.form.is_none() && editor.dirty());
        let t = Binding::parse("Super+T").unwrap().ident();
        let custom = editor.draft().unwrap().custom().unwrap();
        assert_eq!(
            custom.iter().find(|e| e.ident() == t).unwrap().command(),
            Some("sentinel-shell"),
            "replaced only after the question named it"
        );
        editor.update(Message::Discard);

        // 2. Replace, then Resume: the accepted Replace was for Open
        // terminal, so it doesn't carry over to a new occupant.
        fixture.user(
            NAMESPACE,
            "custom",
            r#"{(modifiers: [Super], key: "m", description: Some("Mine")): Spawn("sentinel-m")}"#,
        );
        editor.update(Message::Refresh);
        let m = Binding::parse("Super+M").unwrap().ident();
        let mine = editor
            .draft()
            .unwrap()
            .rows()
            .into_iter()
            .find(|row| row.ident() == m)
            .unwrap();
        editor.update(Message::Edit(mine.target()));
        editor.update(Message::Combo("Super+T".into()));
        editor.update(Message::Save);
        assert!(replaces(&editor, "Open terminal"));
        editor.update(Message::Answer(Answer::Replace));
        assert!(matches!(
            editor.form.as_ref().unwrap().ask,
            Some(Ask::Resume { ref default, .. }) if default == "Maximize window"
        ));
        fixture.user(
            NAMESPACE,
            "custom",
            r#"{(modifiers: [Super], key: "m", description: Some("Mine")): Spawn("sentinel-m"), (modifiers: [Super], key: "t", description: Some("Backup")): Spawn("backup-tool")}"#,
        );
        editor.update(Message::Refresh);
        assert!(
            !editor.dirty() && replaces(&editor, "the command “Backup”"),
            "{:?}",
            editor.form
        );
        assert_eq!(
            editor.form.as_ref().unwrap().choice,
            Choice::default(),
            "the earlier Replace doesn't carry over"
        );
        editor.update(Message::Answer(Answer::TurnBackOn));
        assert!(!editor.dirty(), "a Resume answer doesn't approve a Replace");
        editor.update(Message::Cancel);

        // 3. Use current after an outside change to an edited draft.
        fixture.user(NAMESPACE, "custom", "{}");
        editor.update(Message::Refresh);
        let q = Binding::parse("Super+Q").unwrap().ident();
        editor.update(Message::Default(q, false));
        assert!(editor.dirty());
        editor.update(Message::Add);
        editor.update(Message::Name("Shell".into()));
        editor.update(Message::Combo("Super+T".into()));
        editor.update(Message::Command("sentinel-shell".into()));
        editor.update(Message::Save);
        assert!(replaces(&editor, "Open terminal"));
        fixture.user(NAMESPACE, "custom", backup_t);
        editor.update(Message::Refresh);
        assert!(editor.dirty() && !editor.draft().unwrap().conflicts().is_empty());
        editor.update(Message::UseCurrent);
        assert!(
            !editor.dirty() && replaces(&editor, "the command “Backup”"),
            "{:?}",
            editor.form
        );
        editor.update(Message::Cancel);

        // 4. A row's question: resuming a paused command.
        fixture.user(
            NAMESPACE,
            "custom",
            r#"{(modifiers: [Super, Ctrl], key: "j"): Minimize}"#,
        );
        fixture.user(PAUSED_NAMESPACE, "disabled_commands", PAUSED_J);
        editor.update(Message::Refresh);
        let j = Binding::parse("Super+Ctrl+J").unwrap().ident();
        editor.update(Message::Running(j, true));
        assert!(matches!(
            editor.pending.as_ref().unwrap().ask,
            Ask::Replace { ref current, .. } if current == "Minimize window"
        ));
        fixture.user(
            NAMESPACE,
            "custom",
            r#"{(modifiers: [Super, Ctrl], key: "j", description: Some("Backup")): Spawn("backup-tool")}"#,
        );
        editor.update(Message::Refresh);
        assert!(
            !editor.dirty(),
            "the backup command isn't replaced silently"
        );
        assert!(matches!(
            editor.pending.as_ref().map(|pending| &pending.ask),
            Some(Ask::Replace { current, .. }) if current == "the command “Backup”"
        ));

        // 5. Nothing changed: Refresh keeps the question and its answers.
        let before = editor.pending.clone();
        editor.update(Message::Refresh);
        assert_eq!(editor.pending, before);
        editor.update(Message::Answer(Answer::Replace));
        assert!(editor.pending.is_none() && editor.dirty());
    }
}
