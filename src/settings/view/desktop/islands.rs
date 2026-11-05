//! Concise native controls for the existing Dock and panel page.
use super::*;
use crate::settings::desktop::islands::Edit as IslandEdit;
use dock_islands_core::{Placement, Section};
fn change(e: IslandEdit) -> Message {
    edit(Edit::Islands(e))
}
impl Settings {
    pub(super) fn islands_section(&self, draft: &PanelDraft) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let settings = match draft.islands() {
            Setting::Known(s) => s,
            Setting::Unsupported(_) => {
                return section(vec![field("Style", kept("Unsupported island settings"))]);
            }
        };
        let mut items = if draft.kind() == PanelKind::Panel {
            vec![label("Island placement", 16., FG)]
        } else {
            vec![
                label("Island placement", 16., FG),
                field(
                    "Dock shape",
                    choice(
                        &Setting::Known(settings.enabled),
                        vec![("Standard".into(), false), ("Islands".into(), true)],
                        |v| Edit::Islands(IslandEdit::Enabled(v)),
                        accent,
                    ),
                ),
            ]
        };
        if !settings.enabled {
            return section(items);
        }
        let state = &self.desktop.islands;
        for message in state.installation.applet_messages() {
            items.push(label(message, 12., WARN));
        }
        if let Some(status) = state.availability_message() {
            items.push(banner(
                status,
                WARN,
                vec![
                    button::text("Setup and updates")
                        .on_press(Message::OpenIslandsGuide)
                        .into(),
                    button::text("Check again")
                        .on_press(change(IslandEdit::Check))
                        .into(),
                ],
            ));
        }
        let pending_native = draft.edited().any(|key| key != "islands");
        if pending_native {
            items.push(label(
                "Finish the current adjustment before arranging on screen.",
                11.,
                MUTED,
            ));
        }
        items.push(field(
            "Placement",
            row::with_children(vec![
                choice(
                    &Setting::Known(settings.placement),
                    vec![
                        ("Together".into(), Placement::Together),
                        ("Separate".into(), Placement::Separate),
                    ],
                    |v| Edit::Islands(IslandEdit::Placement(v)),
                    accent,
                ),
                if self.desktop.arranging() {
                    row::with_children(vec![
                        button::text("Cancel")
                            .on_press(change(IslandEdit::Cancel))
                            .into(),
                        button::suggested("Done")
                            .on_press(change(IslandEdit::Done))
                            .into(),
                    ])
                    .spacing(8)
                    .into()
                } else {
                    button::standard("Arrange")
                        .on_press_maybe(
                            (state.compatible && draft.listed() && !pending_native)
                                .then_some(change(IslandEdit::Arrange)),
                        )
                        .into()
                },
            ])
            .spacing(12)
            .align_y(Alignment::Center)
            .wrap()
            .into(),
        ));
        let text = state
            .gap_text
            .clone()
            .unwrap_or_else(|| settings.gap.to_string());
        items.push(field(
            if settings.placement == Placement::Together {
                "Gap"
            } else {
                "Minimum gap"
            },
            row::with_children(vec![
                cosmic::widget::slider(0..=64, settings.gap, |n| change(IslandEdit::Gap(n)))
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
        if self.desktop.arranging() {
            items.push(label(
                "Drag the handles on the selected bar. Tab selects; arrows move; Esc cancels.",
                12.,
                MUTED,
            ));
        }
        let groups = draft.applets().known().cloned().unwrap_or_default();
        let horizontal = draft.anchor().known().is_none_or(|a| a.horizontal());
        let mut selected = Vec::new();
        for (i, region) in Region::ALL.into_iter().enumerate() {
            if groups.list(region).is_empty() {
                continue;
            }
            let id = Section::ALL[i];
            selected.push(
                button::text(format!("{}: {}", i + 1, region.title(horizontal)))
                    .on_press(change(IslandEdit::Select(id)))
                    .into(),
            );
        }
        if settings.placement == Placement::Together {
            selected.push(
                button::text("Group")
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
        section(items)
    }
}
