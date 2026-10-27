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
        let mut rows = vec![detail(
            "Islands setup",
            self.desktop.islands.installation.status_label(),
        )];
        match &state.phase {
            Phase::Preparing => {
                rows.push(label("Downloading and verifying Islands…", 12., MUTED));
                rows.push(button::text("Cancel").on_press(action(AddonMessage::Cancel)).into());
            }
            Phase::Ready => {
                rows.push(label("Install the verified Islands package? Your system may ask for an administrator password. Activation is a separate choice after installation.", 12., MUTED));
                rows.push(row::with_children(vec![
                    button::suggested("Install Islands").id("install-islands-confirm".into()).on_press(action(AddonMessage::Install)).into(),
                    button::text("Cancel").id("install-islands-cancel".into()).on_press(action(AddonMessage::Cancel)).into(),
                ]).spacing(8).wrap().into());
            }
            Phase::Installing => rows.push(label("Installing Islands… Complete the system authorization dialog. The system installer continues if this window closes.", 12., MUTED)),
            Phase::Enabling => rows.push(label("Preparing Islands for the next login…", 12., MUTED)),
            Phase::Failed(error) => {
                rows.push(label(error.label(), 12., MUTED));
                rows.push(button::text("Check again").on_press(action(AddonMessage::Recheck)).into());
            }
            Phase::Idle => match &self.desktop.islands.installation {
                Installation::Absent => {
                    rows.push(label("Add floating dock and panel islands. Halcyon works without this add-on.", 12., MUTED));
                    if self.updates.identity.as_ref().is_some_and(|id| id.can_install_islands()) {
                        rows.push(button::suggested("Get Islands…")
                            .id("get-islands".into()).on_press(action(AddonMessage::Prepare)).into());
                    }
                }
                Installation::Installed { .. } => {
                    if islands::can_enable(&self.desktop.islands.installation)
                        && self.updates.identity.as_ref().is_some_and(|id| id.can_enable_islands()) {
                        rows.push(label("Enable Islands for your next login. Your current panel keeps running.", 12., MUTED));
                        rows.push(button::suggested("Enable for next login")
                            .id("enable-islands".into()).on_press(action(AddonMessage::Enable)).into());
                    } else if islands::can_enable(&self.desktop.islands.installation) {
                        if let Some(notice) = self.updates.identity.as_ref().and_then(|id| id.islands_activation_notice()) {
                            rows.push(label(notice, 12., MUTED));
                        }
                    }
                    rows.push(row::with_children(vec![
                        button::text("Check again").on_press_maybe(
                            (!self.installation_busy).then_some(action(AddonMessage::Recheck))).into(),
                        button::text("Copy diagnostics").on_press(Message::CopyIslandsDiagnostics).into(),
                        button::text("Islands").id("manage-islands".into())
                            .on_press(Message::Page(Page::DesktopDock)).into(),
                    ]).spacing(8).wrap().into());
                    for value in self.desktop.islands.installation.applet_messages() {
                        rows.push(label(value, 12., MUTED));
                    }
                }
                Installation::Pending => {}
            },
        }
        rows.push(
            button::text("Installation guide ↗")
                .on_press(Message::OpenIslandsGuide)
                .into(),
        );
        pages::section(rows)
    }
    pub(super) fn desktop_recovery_section(&self) -> Element<'_, Message> {
        let recovery = &self.desktop_recovery;
        let mut rows = vec![split(
            detail("Restore desktop", recovery.summary()),
            row::with_children(vec![
                button::text("Export…")
                    .id("export-recovery".into())
                    .on_press_maybe(
                        ((recovery.info.is_some() || recovery.problem.is_some()) && !recovery.busy)
                            .then_some(Message::ExportRecovery),
                    )
                    .into(),
                button::standard("Restore desktop…")
                    .id("restore-desktop".into())
                    .on_press_maybe(
                        (recovery.info.is_some() && !recovery.busy)
                            .then_some(Message::DesktopRestorePreview),
                    )
                    .into(),
            ])
            .spacing(8)
            .into(),
        )];
        if let Some((crate::desktop::Owner::Recovery, _, id)) = &self.last_desktop_change {
            rows.push(
                button::text("Undo Restore")
                    .id("undo-desktop-restore".into())
                    .on_press_maybe(
                        (!recovery.busy && !self.native_busy)
                            .then(|| Message::UndoDesktopRestore(id.clone())),
                    )
                    .into(),
            );
        }
        if let Some(problem) = &recovery.problem {
            rows.push(label(problem, 12., MUTED));
        }
        if let Some(plan) = &recovery.plan {
            rows.push(label(
                format!(
                    "Restore {} {} in {} {}?",
                    plan.changes(),
                    if plan.changes() == 1 {
                        "setting"
                    } else {
                        "settings"
                    },
                    plan.domains.len(),
                    if plan.domains.len() == 1 {
                        "scope"
                    } else {
                        "scopes"
                    }
                ),
                14.,
                FG,
            ));
            rows.push(label("A recovery point is saved first. Undo stays available after restart. Schedules stay on; the next future event can apply. Automatic colors resume with the next wallpaper.", 12., MUTED));
            let later = plan
                .snapshot
                .domains
                .iter()
                .filter(|d| d.captured_at != plan.snapshot.captured_at)
                .map(|d| {
                    format!(
                        "{:?} · {}",
                        d.domain,
                        d.captured_at.split('T').next().unwrap_or(&d.captured_at)
                    )
                })
                .collect::<Vec<_>>();
            if !later.is_empty() {
                rows.push(label(
                    format!("Captured later: {}", later.join(", ")),
                    12.,
                    MUTED,
                ));
            }
            if !plan.skipped.is_empty() {
                rows.push(label(
                    format!(
                        "{} unsupported or unavailable settings kept; included in export.",
                        plan.skipped.len()
                    ),
                    12.,
                    MUTED,
                ));
            }
            if !plan.missing_assets.is_empty() {
                rows.push(label(
                    format!("Missing wallpaper: {}", plan.missing_assets.join(", ")),
                    12.,
                    MUTED,
                ));
            }
            if plan.snapshot.domains.iter().any(|d| d.motion.is_some()) {
                rows.push(label("App animations use a separate GSettings preference. Its saved raw override is exportable; Restore and Undo leave it unchanged.", 12., MUTED));
            }
            rows.push(
                row::with_children(vec![
                    button::standard("Cancel")
                        .id("restore-desktop-cancel".into())
                        .on_press(Message::DesktopRestoreConfirm(false))
                        .into(),
                    button::suggested("Restore desktop")
                        .id("restore-desktop-confirm".into())
                        .on_press(Message::DesktopRestoreConfirm(true))
                        .into(),
                ])
                .spacing(8)
                .into(),
            );
        }
        pages::section(rows)
    }
}
