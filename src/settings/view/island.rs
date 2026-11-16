//! The Activity island's own page: the pill on the panel and what it shows.
use super::home_picture::has_island;
use super::*;
use crate::desktop::panels::{Anchor, PanelKind};
use crate::settings::desktop::Message as Edit;

impl Settings {
    pub(super) fn activity_island_page(&self) -> Element<'_, Message> {
        let mut parts = vec![
            heading(
                "Activity island",
                "Music, meetings and alerts in the middle of your top panel.",
            ),
            self.home_glance(Page::ActivityIsland),
        ];
        let panel = self.desktop.draft(PanelKind::Panel);
        if !panel.is_some_and(|draft| draft.listed() && has_island(draft)) {
            // Why it isn't there, with the step that puts it there.
            parts.push(group(None, vec![self.island_way_on()]));
            // What it offers shows before it's added; the choices are kept
            // for when it is.
            parts.push(label(
                "These choices apply once the island is on your panel.",
                12.,
                MUTED,
            ));
        }
        parts.push(self.activity_preferences());
        parts.push(self.activity_gestures());
        container(column::with_children(parts).spacing(20))
            .id(cosmic::widget::Id::new("activity-island"))
            .width(Length::Fill)
            .into()
    }

    /// The island isn't on the panel: the reason in plain words and what
    /// to do. A top panel adds it here; a panel on another edge moves to the
    /// top here, with the Panel page's own Screen edge setting.
    fn island_way_on(&self) -> Element<'_, Message> {
        let idle = !self.native_busy && !self.desktop.drag.active();
        let open_panel = |state: &'static str| {
            self.link_row_state(
                "Not on your panel",
                state,
                Message::Page(Page::DesktopPanel),
                "island-open-panel".into(),
            )
        };
        let Some(draft) = self.desktop.draft(PanelKind::Panel) else {
            return open_panel("The island sits in the middle of a top panel.");
        };
        if !draft.listed() {
            return split(
                detail_state(
                    "Your top panel is off",
                    "The Activity island needs a top panel. Turn it on in COSMIC Settings.",
                ),
                button::text("Open COSMIC Settings ↗")
                    .id(cosmic::widget::Id::new("island-panel-settings"))
                    .on_press(Message::OpenDesktop(PanelKind::Panel.system_page()))
                    .into(),
            );
        }
        if let Some(add) = draft
            .applets()
            .known()
            .and_then(|regions| self.add_activity_island(draft, regions))
        {
            return add;
        }
        match draft.anchor().known() {
            Some(&edge) if edge != Anchor::Top => split(
                detail_state(
                    format!("Your panel is on the {} edge", edge.title().to_lowercase()),
                    "The Activity island needs a top panel.",
                ),
                button::standard("Move panel to the top")
                    .id(cosmic::widget::Id::new("island-move-panel"))
                    .on_press_maybe(
                        idle.then_some(Message::DesktopEditor(Edit::Anchor(Anchor::Top))),
                    )
                    .into(),
            ),
            _ if !self
                .desktop
                .catalog
                .known(crate::activity_island::APPLET_ID) =>
            {
                open_panel("The panel can’t find the Activity island, so it can’t be added yet.")
            }
            Some(_) => {
                open_panel("The panel’s items can’t be edited here. See them on the Panel page.")
            }
            None => open_panel("Set the panel’s Screen edge to Top, then add it."),
        }
    }
}
