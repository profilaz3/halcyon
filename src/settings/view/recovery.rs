use super::*;
impl Settings {
    pub(super) fn addons_section(&self) -> Element<'_, Message> {
        use crate::desktop::islands::Installation;
        use crate::settings::updates::{
            Message as UpdateMessage,
            islands::{self, Message as AddonMessage, Phase},
        };
        let action = |value| Message::Updates(UpdateMessage::Islands(value));
        let state = &self.updates.islands;
        if self.page == Page::DesktopDock {
            let installation = &self.desktop.islands.installation;
            let verified = self
                .updates
                .identity
                .as_ref()
                .is_some_and(|id| id.can_enable_islands());
            let (title, id, next) = if islands::can_enable(installation) && verified {
                (
                    "Enable for next login",
                    "enable-islands",
                    AddonMessage::Enable,
                )
            } else if islands::can_disable(installation) && verified {
                (
                    "Turn off for next login",
                    "disable-islands",
                    AddonMessage::Disable,
                )
            } else {
                ("Check again", "check-islands", AddonMessage::Recheck)
            };
            let mut rows = vec![split(
                detail("Islands", installation.status_label()),
                button::standard(title)
                    .id(id.into())
                    .on_press_maybe(
                        (!state.busy() && !self.installation_busy).then_some(action(next)),
                    )
                    .into(),
            )];
            match &state.phase {
                Phase::Enabling => {
                    rows.push(label("Updating Islands for the next login…", 12., MUTED))
                }
                Phase::Failed(error) => rows.push(label(error.label(), 12., MUTED)),
                Phase::Idle => {
                    if let Some(notice) = self
                        .updates
                        .identity
                        .as_ref()
                        .and_then(|id| id.islands_activation_notice())
                    {
                        rows.push(label(notice, 12., MUTED));
                    }
                }
            }
            return pages::section(rows);
        }
        let mut rows = vec![detail(
            "Islands",
            self.desktop.islands.installation.status_label(),
        )];
        match &state.phase {
            Phase::Enabling => rows.push(label("Updating Islands for the next login…", 12., MUTED)),
            Phase::Failed(error) => {
                rows.push(label(error.label(), 12., MUTED));
                rows.push(
                    button::text("Check again")
                        .on_press(action(AddonMessage::Recheck))
                        .into(),
                );
            }
            Phase::Idle => {
                match &self.desktop.islands.installation {
                    Installation::Absent => {
                        rows.push(label("Islands is included in the Halcyon package. Install or repair Halcyon to use it.", 12., MUTED));
                    }
                    Installation::Installed { .. } => {
                        if islands::can_enable(&self.desktop.islands.installation)
                            && self
                                .updates
                                .identity
                                .as_ref()
                                .is_some_and(|id| id.can_enable_islands())
                        {
                            rows.push(label("Enable Islands for your next login. Your current panel keeps running.", 12., MUTED));
                            rows.push(
                                button::suggested("Enable for next login")
                                    .id("enable-islands".into())
                                    .on_press(action(AddonMessage::Enable))
                                    .into(),
                            );
                        } else if islands::can_enable(&self.desktop.islands.installation) {
                            if let Some(notice) = self
                                .updates
                                .identity
                                .as_ref()
                                .and_then(|id| id.islands_activation_notice())
                            {
                                rows.push(label(notice, 12., MUTED));
                            }
                        }
                        if islands::can_disable(&self.desktop.islands.installation)
                            && self
                                .updates
                                .identity
                                .as_ref()
                                .is_some_and(|id| id.can_enable_islands())
                        {
                            rows.push(
                                button::standard("Turn off for next login")
                                    .id("disable-islands".into())
                                    .on_press(action(AddonMessage::Disable))
                                    .into(),
                            );
                        }
                        rows.push(
                            row::with_children(vec![
                                button::text("Check again")
                                    .on_press_maybe(
                                        (!self.installation_busy)
                                            .then_some(action(AddonMessage::Recheck)),
                                    )
                                    .into(),
                                button::text("Copy diagnostics")
                                    .on_press(Message::CopyIslandsDiagnostics)
                                    .into(),
                                button::text("Dock and panel")
                                    .id("manage-islands".into())
                                    .on_press(Message::Page(Page::DesktopDock))
                                    .into(),
                            ])
                            .spacing(8)
                            .wrap()
                            .into(),
                        );
                        for value in self.desktop.islands.installation.applet_messages() {
                            rows.push(label(value, 12., MUTED));
                        }
                    }
                    Installation::Pending => {}
                }
            }
        }
        rows.push(
            button::text("Installation guide ↗")
                .on_press(Message::OpenIslandsGuide)
                .into(),
        );
        pages::section(rows)
    }
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
