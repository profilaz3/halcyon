use super::*;
impl Settings {
    /// The desktop as it was before Halcyon's first change, with the
    /// confirmation folded into the same rows.
    pub(super) fn desktop_recovery_rows(&self) -> Vec<Element<'_, Message>> {
        let recovery = &self.desktop_recovery;
        let mut rows = vec![group_row(
            "Desktop from before Halcyon",
            recovery.summary(),
            button::standard("Restore…")
                .id("restore-desktop".into())
                .on_press_maybe(
                    (recovery.info.is_some() && !recovery.busy && recovery.plan.is_none())
                        .then_some(Message::DesktopRestorePreview),
                )
                .into(),
        )];
        if let Some((crate::desktop::Owner::Recovery, _, id)) = &self.last_desktop_change {
            rows.push(group_row(
                "Desktop restored",
                "",
                button::text("Undo")
                    .id("undo-desktop-restore".into())
                    .on_press_maybe(
                        (!recovery.busy && !self.native_busy)
                            .then(|| Message::UndoDesktopRestore(id.clone())),
                    )
                    .into(),
            ));
        }
        if let Some(problem) = &recovery.problem {
            rows.push(label(problem, 12., MUTED));
        }
        if let Some(plan) = &recovery.plan {
            let mut notes = vec![format!(
                "{} {} go back. Undo stays available, even after a restart.",
                plan.changes(),
                if plan.changes() == 1 {
                    "setting will"
                } else {
                    "settings will"
                },
            )];
            if !plan.skipped.is_empty() {
                notes.push(format!(
                    "{} can't be restored here and stay as they are.",
                    plan.skipped.len()
                ));
            }
            if !plan.missing_assets.is_empty() {
                notes.push(format!(
                    "Missing wallpaper: {}",
                    plan.missing_assets.join(", ")
                ));
            }
            rows.push(detail("Restore the earlier desktop?", notes.join(" ")));
            rows.push(
                row::with_children(vec![
                    button::text("Save a copy first…")
                        .id("export-recovery".into())
                        .on_press_maybe(
                            ((recovery.info.is_some() || recovery.problem.is_some())
                                && !recovery.busy)
                                .then_some(Message::ExportRecovery),
                        )
                        .into(),
                    cosmic::widget::space().width(Length::Fill).into(),
                    button::text("Cancel")
                        .id("restore-desktop-cancel".into())
                        .on_press(Message::DesktopRestoreConfirm(false))
                        .into(),
                    button::suggested("Restore desktop")
                        .id("restore-desktop-confirm".into())
                        .on_press(Message::DesktopRestoreConfirm(true))
                        .into(),
                ])
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        rows
    }
}
