use super::*;
use crate::settings::updates::{Check, Message as Update, UpdateChannel};

impl Settings {
    /// One line for the Settings landing; never claims more than a check found.
    pub(super) fn updates_summary_line(&self) -> String {
        self.updates.summary_line()
    }
    pub(in crate::settings) fn updates_section(&self) -> Element<'_, Message> {
        let mut content = Vec::new();
        let mut addons = Vec::new();
        let mut actions = Vec::new();
        let action =
            |title: &'static str, id: &'static str, message: Update| -> Element<'_, Message> {
                button::standard(title)
                    .id(cosmic::widget::Id::new(id))
                    .on_press(Message::Updates(message))
                    .into()
            };
        match &self.updates.identity {
            None => content.push(label("Checking this installation…", 14., MUTED)),
            Some(identity) => {
                let summary = crate::settings::updates::Summary::new(
                    identity,
                    &self.updates.check,
                    &self.desktop.islands.installation,
                );
                let main = identity
                    .installed_version
                    .as_deref()
                    .unwrap_or(&identity.running_version);
                content.push(group_row("Version", "", label(main, 14., MUTED)));
                content.push(detail(summary.title.clone(), summary.detail.clone()));
                let status = match self.desktop.islands.installation.status_label() {
                    "Islands not installed" => "Not installed",
                    other => other,
                };
                let addon = if identity.islands_version.is_some() {
                    format!("Floating panel style · included · {status}")
                } else {
                    format!("Floating panel style · {status}")
                };
                // One update card. Islands turns on in Dock and panel; only
                // a real exception adds anything here.
                addons.push(self.link_row(
                    "Panel islands",
                    addon,
                    Message::OpenBar(crate::desktop::panels::PanelKind::Panel),
                    "updates-open-panel".into(),
                ));
                if let crate::settings::updates::islands::Phase::Failed(error) =
                    &self.updates.islands.phase
                {
                    addons.push(label(error.label(), 12., MUTED));
                }
                if let Some(notice) = identity.islands_activation_notice() {
                    addons.push(label(notice, 12., MUTED));
                }
                if let Check::Failed(error) = &self.updates.check
                    && summary.title != "Update check failed"
                {
                    content.push(label(error.label(), 12., MUTED));
                }
                match identity.channel {
                    UpdateChannel::Apt | UpdateChannel::Dnf if identity.can_check() => {
                        if self.updates.check == Check::Checking {
                            actions.push(action("Cancel check", "updates-cancel", Update::Cancel));
                        } else {
                            let check = if summary.system_action {
                                button::standard("Check for updates")
                            } else {
                                button::suggested("Check for updates")
                            };
                            actions.push(
                                check
                                    .id(cosmic::widget::Id::new("updates-check"))
                                    .on_press(Message::Updates(Update::Check))
                                    .into(),
                            );
                        }
                        if identity.store.is_some() {
                            actions.push(
                                (if summary.system_action {
                                    button::suggested("Open system updates")
                                } else {
                                    button::standard("Open system updates")
                                })
                                .id(cosmic::widget::Id::new("updates-store"))
                                .on_press_maybe(
                                    (!self.updates.store_busy)
                                        .then_some(Message::Updates(Update::OpenStore)),
                                )
                                .into(),
                            );
                        } else {
                            content.push(label(
                                "COSMIC Store is unavailable. Use your system package manager.",
                                12.,
                                MUTED,
                            ));
                            if let Some(command) = identity.update_command() {
                                content.push(label(command, 12., MUTED));
                                actions.push(action(
                                    "Copy update command",
                                    "updates-copy-command",
                                    Update::CopyCommand,
                                ));
                            }
                        }
                    }
                    UpdateChannel::ManualDeb | UpdateChannel::ManualRpm => {
                        actions.push(action(
                            "Download a release",
                            "updates-releases",
                            Update::Releases,
                        ));
                        actions.push(action(
                            "Installation guide",
                            "updates-information",
                            Update::Information,
                        ));
                    }
                    UpdateChannel::Source => {
                        actions.push(action(
                            "Release information",
                            "updates-releases",
                            Update::Releases,
                        ));
                    }
                    _ => actions.push(action(
                        "Details",
                        "updates-information",
                        Update::Information,
                    )),
                }
            }
        }
        if let Some(at) = self.updates.last_checked {
            content.push(group_row(
                "Last checked",
                "",
                label(at.format("%b %-d, %H:%M").to_string(), 14., MUTED),
            ));
        }
        // This version's notes ship with it; an available update's are online.
        if matches!(self.updates.check, Check::Available(_)) && self.updates.notes_url().is_some() {
            actions.push(
                button::text("What’s in the update ↗")
                    .on_press(Message::Updates(Update::Notes))
                    .into(),
            );
        }
        if let Some(error) = &self.updates.store_error {
            content.push(label(error, 12., MUTED));
        }
        if !actions.is_empty() {
            content.push(group_row(
                "More about this version",
                "",
                row::with_children(actions)
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
            ));
        }
        if let Some(release) = crate::settings::release_notes::current() {
            content.push(self.link_row(
                "What’s new",
                format!("New in {}", release.version),
                Message::AppSettingsPage(AppSettingsPage::WhatsNew),
                "updates-whats-new".into(),
            ));
        }
        let mut groups = vec![group(Some("This version"), content)];
        if !addons.is_empty() {
            groups.push(group(Some("Add-ons"), addons));
        }
        if !self.design.managed {
            groups.push(self.design_section(Some("Widgets")));
        }
        column::with_children(groups).spacing(16).into()
    }

    /// This version's release notes. A note with a place to set it up opens
    /// that place, and Back returns here.
    pub(in crate::settings) fn whats_new_page(&self) -> Element<'_, Message> {
        let Some(release) = crate::settings::release_notes::current() else {
            return label("There are no notes for this version.", 14., MUTED);
        };
        let rows = release
            .notes
            .iter()
            .enumerate()
            .map(|(index, note)| {
                let text = column::with_children(vec![
                    label(note.title, 15., FG),
                    label(note.body, 13., MUTED),
                ])
                .spacing(4)
                .width(Length::Fill);
                let mut line = row::with_children(vec![text.into()])
                    .spacing(16)
                    .align_y(Alignment::Center);
                if let Some(place) = note.place {
                    line = line.push(
                        button::standard(format!(
                            "Open {}",
                            if place.page == Page::General {
                                place.app_destination().title()
                            } else {
                                place.page.title()
                            }
                        ))
                            .id(cosmic::widget::Id::new(format!("whats-new-open-{index}")))
                            .on_press(Message::SearchResult(place)),
                    );
                }
                container(line).padding([6, 0]).into()
            })
            .collect();
        let mut groups = vec![
            text::heading(format!("Halcyon {}", release.version)).into(),
            group(None, rows),
        ];
        if self.updates.notes_url().is_some() {
            groups.push(
                button::text("Release notes online ↗")
                    .on_press(Message::Updates(Update::Notes))
                    .into(),
            );
        }
        column::with_children(groups).spacing(16).into()
    }
}
