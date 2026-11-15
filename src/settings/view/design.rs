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
    /// Whether `page` stays usable while the widget design can't be read:
    /// its edits change only COSMIC's own settings, never Halcyon's.
    pub(super) fn design_safe(page: Page) -> bool {
        matches!(
            page,
            Page::DesktopWindows
                | Page::DesktopWorkspaces
                | Page::DesktopWindowRules
                | Page::DesktopShortcuts
        )
    }
    /// A line above a page that stays usable, while widget editing waits.
    pub(super) fn design_banner(&self) -> Element<'_, Message> {
        super::desktop::banner(
            "Your widget design needs attention. Widget editing is paused until it is restored.",
            super::desktop::WARN,
            vec![
                button::standard("Restore…")
                    .id("design-banner-restore".into())
                    .on_press(Message::Page(Page::Home))
                    .into(),
            ],
        )
    }
    /// The guided restore, shown in place of a page whose edits wait.
    pub(super) fn design_recovery_view(&self) -> Element<'_, Message> {
        let design_file = crate::config::Config::path()
            .map(|config| crate::design::storage::path(&config).display().to_string())
            .unwrap_or_else(|| "design-v1.json".into());
        let mut parts = vec![
            heading(
                "Your widget design needs attention",
                "Your widgets are kept as they were. Widget editing is paused; the rest of Settings still works.",
            ),
            label(
                self.config
                    .design_problem
                    .as_deref()
                    .unwrap_or("Could not open the widget design"),
                15.,
                FG,
            ),
            group(
                Some("Restore it in three steps"),
                vec![
                    group_row(
                        "1. Save the original",
                        "A copy of the file as it is now, in case you need it later",
                        button::standard("Export original…")
                            .id("design-recovery-export".into())
                            .on_press(Message::Design(Edit::Export(false)))
                            .into(),
                    ),
                    detail_state(
                        "2. Put back a saved copy",
                        format!(
                            "Copy a widget design you saved earlier, or one from a backup, to {design_file}. A compatible Halcyon version can also open it."
                        ),
                    ),
                    group_row(
                        "3. Check again",
                        "Halcyon opens the restored design and widget editing comes back",
                        button::suggested("Check again")
                            .id("design-recheck".into())
                            .on_press(Message::Design(Edit::Recheck))
                            .into(),
                    ),
                ],
            ),
        ];
        if self.design.source_conflict {
            parts.push(label(
                "An older Halcyon also changed your widget settings. Review those changes below first.",
                13.,
                MUTED,
            ));
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
        column::with_children(parts)
            .spacing(18)
            .max_width(680)
            .into()
    }
}
