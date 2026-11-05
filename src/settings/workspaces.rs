//! Workspaces editor state: one draft of the compositor's workspace struct,
//! written only by Apply as one change owned by this page.
use crate::desktop::store::{DesktopStore, Journal, Owner};
use crate::desktop::workspaces::{TypingAction, WorkspaceLayout, WorkspaceMode, WorkspacesDraft};
use crate::settings::desktop::Notice;
use crate::settings::drafts::DraftEditor;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Mode(WorkspaceMode),
    Layout(WorkspaceLayout),
    Typing(TypingAction),
    Wraparound(bool),
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

pub struct WorkspacesEditor {
    pub(super) core: DraftEditor<WorkspacesDraft>,
}

impl Default for WorkspacesEditor {
    fn default() -> Self {
        Self {
            core: DraftEditor::new(Owner::Workspaces, "Workspaces"),
        }
    }
}

impl WorkspacesEditor {
    pub fn open(&mut self) {
        self.core.open();
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
    pub fn draft(&self) -> Option<&WorkspacesDraft> {
        self.core.draft()
    }
    pub fn notice(&self) -> Option<&Notice> {
        self.core.notice.as_ref()
    }
    pub fn dirty(&self) -> bool {
        self.core.dirty()
    }
    pub fn recovery(&self) -> Option<&Result<Journal, String>> {
        self.core.recovery()
    }
    pub fn can_undo(&self) -> bool {
        self.core.can_undo()
    }
    pub fn can_apply(&self) -> bool {
        self.core.can_apply()
    }
    pub fn watch_paths(&self) -> Vec<PathBuf> {
        self.core.watch_paths()
    }
    pub fn state_key(&self) -> String {
        format!(
            "{}:{:?}",
            self.core.state_key(),
            self.draft().map(WorkspacesDraft::edits)
        )
    }
    pub fn refresh(&mut self) {
        self.core.refresh();
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Mode(mode) => self.core.edit(|draft| draft.set_mode(mode)),
            Message::Layout(layout) => self.core.edit(|draft| draft.set_layout(layout)),
            Message::Typing(action) => self.core.edit(|draft| draft.set_typing(action)),
            Message::Wraparound(on) => self.core.edit(|draft| draft.set_wraparound(on)),
            Message::Apply => {
                if self.can_apply() {
                    self.core.apply();
                }
            }
            Message::Discard => self.core.discard(),
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
}
