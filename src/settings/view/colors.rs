//! Wallpaper-page color conflicts.
use super::desktop::conflict_banner;
use super::*;
use crate::settings::wallpaper::Message as Edit;

fn edit(message: Edit) -> Message {
    Message::Wallpaper(message)
}

impl Settings {
    pub(super) fn colors_conflicts(&self) -> Vec<Element<'_, Message>> {
        let mut items = Vec::new();
        if self.wallpaper.colors_conflict() {
            items.push(conflict_banner(
                "App colors".into(),
                edit(Edit::UseCurrent),
                edit(Edit::KeepColors),
            ));
        }
        if !self.wallpaper.accent_conflicts().is_empty() {
            items.push(conflict_banner(
                crate::settings::system_appearance::describe(self.wallpaper.accent_conflicts()),
                edit(Edit::UseCurrent),
                edit(Edit::KeepColors),
            ));
        }
        items
    }

}
