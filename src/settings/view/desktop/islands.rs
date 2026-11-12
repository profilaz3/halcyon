//! Concise native controls for the existing Dock and panel page.
use super::*;
use crate::settings::desktop::islands::Edit as IslandEdit;
use dock_islands_core::{Placement, Section};
fn change(e: IslandEdit) -> Message {
    edit(Edit::Islands(e))
}
impl Settings {
    /// Shape: a plain bar or islands. Picking Islands shows the one step
    /// Halcyon's panel still needs; layout controls appear once it runs.
    pub(super) fn islands_section(&self, draft: &PanelDraft) -> Element<'_, Message> {
        // One Shape for both bars: compact, edge to edge, or islands.
        let mut items = vec![self.panel_style(draft)];
        let settings = match draft.islands() {
            Setting::Known(s) => s,
            Setting::Unsupported(_) => {
                items.push(field("Islands", kept("Unsupported island settings")));
                return section(items);
            }
        };
        if !settings.enabled {
            return section(items);
        }
        if let Some(step) = self.host_step("Islands use") {
            items.push(step);
            return section(items);
        }
        let state = &self.desktop.islands;
        for message in state.installation.applet_messages() {
            items.push(label(message, 12., WARN));
        }
        let pending_native = draft.edited().any(|key| key != "islands");
        if pending_native {
            items.push(label(
                "Finish the current adjustment before arranging on screen.",
                11.,
                MUTED,
            ));
        }
        items.push(split(
            detail("Keep islands together", "Off spreads them along the edge"),
            toggler(settings.placement == Placement::Together)
                .on_toggle(|together| {
                    change(IslandEdit::Placement(if together {
                        Placement::Together
                    } else {
                        Placement::Separate
                    }))
                })
                .into(),
        ));
        let text = state
            .gap_text
            .clone()
            .unwrap_or_else(|| settings.gap.to_string());
        items.push(field(
            if settings.placement == Placement::Together {
                "Space between"
            } else {
                "Minimum space"
            },
            row::with_children(vec![
                super::super::focus::slider(0..=64, settings.gap, |n| change(IslandEdit::Gap(n)))
                    .on_release(edit(Edit::Apply))
                    .width(Length::Fill)
                    .apply(|control| completed_slider(control.into(), edit(Edit::Apply))),
                text_input("14", text)
                    .id(cosmic::widget::Id::new("islands-gap"))
                    .on_input(|s| change(IslandEdit::GapText(s)))
                    .on_submit(|_| change(IslandEdit::GapFinished))
                    .on_unfocus(change(IslandEdit::GapFinished))
                    .width(64)
                    .into(),
                label("px", 12., MUTED),
            ])
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        ));
        // Moved islands rename the Items above; one step puts them back.
        let screen = self.desktop.screen_order(draft.kind());
        if screen.rearranged() {
            items.push(split(
                detail_state(
                    "Island order",
                    "Items follow the islands as they sit on screen",
                ),
                button::standard("Reset order")
                    .id("islands-reset-order".into())
                    .on_press(change(IslandEdit::ResetOrder))
                    .into(),
            ));
        }
        if !self.desktop.arranging() {
            items.push(
                button::standard("Arrange on screen…")
                    .id("islands-arrange".into())
                    .on_press_maybe(
                        (state.compatible && draft.listed() && !pending_native)
                            .then_some(change(IslandEdit::Arrange)),
                    )
                    .into(),
            );
            return section(items);
        }
        items.push(label(
            "Drag the handles on the panel. Tab selects; arrows move; Esc cancels.",
            12.,
            MUTED,
        ));
        // Keyboard and pointer alternatives to dragging, only while arranging.
        let groups = draft.applets().known().cloned().unwrap_or_default();
        let mut selected = Vec::new();
        for region in screen.regions() {
            if groups.list(region).is_empty() {
                continue;
            }
            let section = match region {
                Region::Start => Section::Start,
                Region::Center => Section::Center,
                Region::End => Section::End,
            };
            selected.push(
                button::text(screen.name(region))
                    .on_press(change(IslandEdit::Select(section)))
                    .into(),
            );
        }
        if settings.placement == Placement::Together {
            selected.push(
                button::text("All together")
                    .on_press(change(IslandEdit::Group(true)))
                    .into(),
            );
        }
        if state.selected.is_some() || state.group {
            selected.extend([
                button::text("Start")
                    .on_press(change(IslandEdit::End(false)))
                    .into(),
                button::text("Previous")
                    .on_press(change(IslandEdit::Move(-1)))
                    .into(),
                button::text("Next")
                    .on_press(change(IslandEdit::Move(1)))
                    .into(),
                button::text("End")
                    .on_press(change(IslandEdit::End(true)))
                    .into(),
            ]);
        }
        items.push(cosmic::widget::flex_row(selected).spacing(6).into());
        items.push(
            row::with_children(vec![
                button::text("Cancel")
                    .on_press(change(IslandEdit::Cancel))
                    .into(),
                button::suggested("Done")
                    .on_press(change(IslandEdit::Done))
                    .into(),
            ])
            .spacing(8)
            .into(),
        );
        section(items)
    }
}
