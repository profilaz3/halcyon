//! Escape closes the innermost open thing in Settings, one per press: a
//! colour panel, then a font list, then an explanation, then the search.
//! Drags, the canvas and the Clock stage listen for Escape themselves while
//! they are active, so this steps aside for them.
use super::{Message, Settings, color_well};
use cosmic::app::Task;
use cosmic::widget::color_picker::ColorPickerUpdate;

/// What one Escape press closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Closes {
    ColorPanel,
    FontList,
    Explanation,
    Search,
}

impl Settings {
    /// True while a drag or an arrangement owns Escape.
    fn escape_owned_elsewhere(&self) -> bool {
        self.reorder.source.is_some()
            || self.devbox_reorder.source.is_some()
            || self.desktop.drag.active()
            || self.desktop.arranging()
            || self.clock_stage.dragging()
            || self.placement_active()
    }

    /// The innermost open thing, if Escape has anything to close here.
    pub(super) fn escape_closes(&self) -> Option<Closes> {
        if self.escape_owned_elsewhere() {
            None
        } else if self.color_well.is_some() {
            Some(Closes::ColorPanel)
        } else if self.font_menu.is_some() {
            Some(Closes::FontList)
        } else if self.explained.is_some_and(|(page, _)| page == self.page) {
            Some(Closes::Explanation)
        } else if self.search_open || !self.search_query.is_empty() {
            Some(Closes::Search)
        } else {
            None
        }
    }

    pub(super) fn escape(&mut self) -> Task<Message> {
        match self.escape_closes() {
            Some(Closes::ColorPanel) => self.handle(Message::ColorWell(
                color_well::Message::Picker(ColorPickerUpdate::Cancel),
            )),
            Some(Closes::FontList) => self.handle(Message::FontMenu(None)),
            Some(Closes::Explanation) => {
                self.explained = None;
                Task::none()
            }
            Some(Closes::Search) => self.handle(Message::SearchSettings(String::new())),
            None => Task::none(),
        }
    }
}
