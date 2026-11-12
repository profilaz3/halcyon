//! The Activity island's own page: the pill on the panel and what it shows.
use super::home_picture::has_island;
use super::*;
use crate::desktop::panels::PanelKind;

impl Settings {
    pub(super) fn activity_island_page(&self) -> Element<'_, Message> {
        let mut parts = vec![
            heading(
                "Activity island",
                "Music, meetings and alerts in a pill on the panel.",
            ),
            self.home_glance(Page::ActivityIsland),
        ];
        let panel = self.desktop.draft(PanelKind::Panel);
        if panel.is_some_and(|draft| draft.listed() && has_island(draft)) {
            parts.push(self.activity_preferences());
        } else {
            // Added from here when the panel can take it; otherwise the
            // panel's own page says where it can go.
            let add = panel.and_then(|draft| {
                draft
                    .applets()
                    .known()
                    .and_then(|regions| self.add_activity_island(draft, regions))
            });
            parts.push(group(
                None,
                vec![add.unwrap_or_else(|| {
                    self.subpage_row(
                        "Not on your panel",
                        "The island sits in the middle of a top panel",
                        Page::DesktopPanel,
                        "island-open-panel",
                    )
                })],
            ));
        }
        container(column::with_children(parts).spacing(20))
            .id(cosmic::widget::Id::new("activity-island"))
            .width(Length::Fill)
            .into()
    }
}
