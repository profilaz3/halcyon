use super::*;
use crate::settings::design::Message as Edit;
impl Settings {
    /// Widget storage, in plain words: the one-time update before your own
    /// designs can be used, or a private copy afterwards.
    pub(super) fn design_section(&self, title: Option<&'static str>) -> Element<'_, Message> {
        let action = |edit| Message::Design(edit);
        let mut rows = Vec::new();
        if self.design.review {
            rows.push(detail_state(
                "Update your widgets?",
                "Every widget comes along, hidden ones too. Notes, accounts and your weather city stay as they are, and Undo keeps working. Older Halcyon versions can't edit widgets afterwards, so save a copy first if you might go back.",
            ));
            rows.push(
                row::with_children(vec![
                    button::text("Save a copy first…")
                        .on_press(action(Edit::Export(true)))
                        .into(),
                    cosmic::widget::space().width(Length::Fill).into(),
                    button::text("Cancel")
                        .id("design-cancel".into())
                        .on_press(action(Edit::Cancel))
                        .into(),
                    button::suggested("Update widgets")
                        .id("design-migrate".into())
                        .on_press_maybe((!self.native_busy).then_some(action(Edit::Migrate)))
                        .into(),
                ])
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            );
        } else if !self.design.managed {
            rows.push(group_row(
                "Get ready for your own designs",
                "A one-time update. Your widgets look and sit exactly the same.",
                button::standard("Get ready…")
                    .id("design-review".into())
                    .on_press(action(Edit::Review))
                    .into(),
            ));
        } else {
            rows.push(group_row(
                "Save a copy of your widgets",
                "A private file you can keep as a backup",
                row::with_children(vec![
                    button::text("For older versions…")
                        .on_press(action(Edit::Export(true)))
                        .into(),
                    button::standard("Save…")
                        .on_press(action(Edit::Export(false)))
                        .into(),
                ])
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            ));
        }
        if let Some(notice) = &self.design.notice {
            rows.push(label(notice, 13., MUTED));
        }
        if self.native_busy {
            rows.push(label("Updating your widgets…", 13., MUTED));
        }
        group(title, rows)
    }
    pub(super) fn design_recovery_view(&self) -> Element<'_, Message> {
        let mut parts = vec![
            heading(
                "Your widget design needs attention",
                "The saved data was kept. Widget editing is paused.",
            ),
            label(
                self.config
                    .design_problem
                    .as_deref()
                    .unwrap_or("Could not open the widget design"),
                15.,
                FG,
            ),
            label(
                "Restore the saved design file, or open it with a compatible Halcyon version. An older app's changes to the frozen widget settings require review.",
                13.,
                MUTED,
            ),
            row::with_children(vec![
                button::standard("Export original…")
                    .id("design-recovery-export".into())
                    .on_press(Message::Design(Edit::Export(false)))
                    .into(),
                button::text("Check again")
                    .id("design-recheck".into())
                    .on_press(Message::Design(Edit::Recheck))
                    .into(),
            ])
            .spacing(8)
            .wrap()
            .into(),
        ];
        if self.design.source_conflict {
            if self.design.source_review {
                parts.push(pages::section(vec![
                    detail_state("Keep the current widget design?", "Changes from the older app and its saved history are backed up first. The current design and unrelated app preferences are retained."),
                    row::with_children(vec![
                        button::text("Cancel").on_press(Message::Design(Edit::Cancel)).into(),
                        button::suggested("Keep current design").id("design-keep-source".into()).on_press_maybe((!self.native_busy).then_some(Message::Design(Edit::KeepDesign))).into(),
                    ]).spacing(8).into(),
                ]));
            } else {
                parts.push(
                    button::standard("Review older-app changes…")
                        .id("design-review-source".into())
                        .on_press(Message::Design(Edit::ReviewSource))
                        .into(),
                );
            }
        }
        if self.design.pending {
            parts.push(
                button::suggested("Recover interrupted change")
                    .id("design-recover".into())
                    .on_press_maybe((!self.native_busy).then_some(Message::Design(Edit::Recover)))
                    .into(),
            );
        }
        if let Some(notice) = &self.design.notice {
            parts.push(label(notice, 13., MUTED));
        }
        if self.native_busy {
            parts.push(label("Recovering…", 13., MUTED));
        }
        container(
            scrollable(column::with_children(parts).spacing(18).max_width(680))
                .height(Length::Fill),
        )
        .padding(28)
        .class(cosmic::theme::Container::Primary)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}
