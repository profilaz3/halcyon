//! Bounded, window-local text history. It never writes Notes or desktop preferences.
use cosmic::widget::text_editor::{self, Action, Content, Edit};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const MAX_ENTRIES: usize = 64;
const MAX_BYTES: usize = 2 * 1024 * 1024;

pub(super) struct Snapshot {
    text: String,
    cursor: text_editor::Cursor,
}
impl Snapshot {
    pub fn capture(content: &Content) -> Self {
        Self {
            text: super::editor_text(content),
            cursor: content.cursor(),
        }
    }
    fn restore(self, content: &mut Content) {
        *content = Content::with_text(&self.text);
        content.move_to(self.cursor);
    }
}

#[derive(Default)]
pub(super) struct History {
    undo: VecDeque<Snapshot>,
    redo: VecDeque<Snapshot>,
    group: Option<(u8, Instant)>,
}
impl History {
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn boundary(&mut self) {
        self.group = None;
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn edit(&mut self, content: &mut Content, action: Action) {
        if !action.is_edit() {
            self.boundary();
            content.perform(action);
            return;
        }
        let before = Snapshot::capture(content);
        let kind = match &action {
            Action::Edit(Edit::Insert(c)) if !c.is_whitespace() => Some(1),
            Action::Edit(Edit::Backspace) => Some(2),
            Action::Edit(Edit::Delete) => Some(3),
            _ => None,
        };
        content.perform(action);
        if before.text == super::editor_text(content) {
            return;
        }
        let now = Instant::now();
        let merge = before.cursor.selection.is_none()
            && kind.is_some()
            && self.group.is_some_and(|(last, at)| {
                Some(last) == kind && now.duration_since(at) < Duration::from_millis(750)
            });
        self.redo.clear();
        if before.text.len() > crate::tools::notes::MAX_TEXT_BYTES {
            self.clear();
        } else {
            if !merge {
                Self::push(&mut self.undo, before);
            }
            self.group = kind.map(|k| (k, now));
        }
    }
    fn push(stack: &mut VecDeque<Snapshot>, value: Snapshot) {
        if value.text.len() > crate::tools::notes::MAX_TEXT_BYTES {
            return;
        }
        stack.push_back(value);
        while stack.len() > MAX_ENTRIES
            || stack.iter().map(|s| s.text.len()).sum::<usize>() > MAX_BYTES
        {
            stack.pop_front();
        }
    }
    pub fn undo(&mut self, content: &mut Content) {
        self.boundary();
        if let Some(before) = self.undo.pop_back() {
            Self::push(&mut self.redo, Snapshot::capture(content));
            before.restore(content);
        }
    }
    pub fn redo(&mut self, content: &mut Content) {
        self.boundary();
        if let Some(after) = self.redo.pop_back() {
            Self::push(&mut self.undo, Snapshot::capture(content));
            after.restore(content);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ux_native_text_undo_restores_unicode_selection_and_redo() {
        let mut content = Content::with_text("Hello 🌙");
        content.perform(Action::SelectAll);
        let selected = content.cursor();
        let mut history = History::default();
        history.edit(
            &mut content,
            Action::Edit(Edit::Paste(std::sync::Arc::new("Živjo".into()))),
        );
        history.undo(&mut content);
        assert_eq!(super::super::editor_text(&content), "Hello 🌙");
        assert_eq!(content.cursor(), selected);
        history.redo(&mut content);
        assert_eq!(super::super::editor_text(&content), "Živjo");
        history.edit(&mut content, Action::Edit(Edit::Insert('!')));
        assert!(!history.can_redo());
    }
    #[test]
    fn ux_native_text_history_groups_typing_and_bounds_pastes() {
        let mut content = Content::new();
        let mut history = History::default();
        for c in "abc".chars() {
            history.edit(&mut content, Action::Edit(Edit::Insert(c)));
        }
        history.undo(&mut content);
        assert_eq!(super::super::editor_text(&content), "");
        for _ in 0..100 {
            content.perform(Action::SelectAll);
            history.edit(
                &mut content,
                Action::Edit(Edit::Paste(std::sync::Arc::new("x".repeat(32 * 1024)))),
            );
            history.edit(&mut content, Action::Edit(Edit::Insert('y')));
        }
        assert!(history.undo.len() <= MAX_ENTRIES);
        assert!(history.undo.iter().map(|s| s.text.len()).sum::<usize>() <= MAX_BYTES);
    }
}
