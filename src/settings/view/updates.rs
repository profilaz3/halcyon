use super::*;
use crate::settings::updates::{Check, Message as Update, UpdateChannel};

impl Settings {
    /// One line for the Settings landing; never claims more than a check found.
    pub(in crate::settings) fn updates_summary_line(&self) -> String {
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
                content.push(detail_state(summary.title.clone(), summary.detail.clone()));
                // Islands and Blend turn on in Dock and panel. This is where
                // to look when Halcyon's panel won't start.
                addons.extend(self.panel_troubleshooting());
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
            content.push(self.link_row_state(
                "What’s new",
                format!("New in {}", release.version),
                Message::AppSettingsPage(AppSettingsPage::WhatsNew),
                "updates-whats-new".into(),
            ));
        }
        let mut groups = vec![group(Some("This version"), content)];
        if !addons.is_empty() {
            groups.push(group(Some("Halcyon’s panel"), addons));
        }
        // Before the update: get ready; afterwards: save a copy.
        groups.push(self.design_section(Some("Widgets")));
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

    /// Status in plain words, then the tools for when it won't start.
    fn panel_troubleshooting(&self) -> Vec<Element<'_, Message>> {
        use super::desktop::host::Host;
        use crate::settings::updates::islands::{self, Message as Panel};
        let installation = &self.desktop.islands.installation;
        let status = match self.panel_host() {
            Host::Running => "Running. Islands and Blend into wallpaper are available.",
            Host::Checking => "Checking…",
            Host::Switching => "Getting ready for your next sign-in…",
            Host::Switch => "Off. Choose Islands in Dock and panel to use it.",
            Host::SignIn => "Starts the next time you sign in.",
            Host::Failed(reason) | Host::Unavailable(reason) => reason,
        };
        let mut rows = vec![self.link_row_state(
            "Status",
            status,
            Message::OpenBar(crate::desktop::panels::PanelKind::Panel),
            "updates-open-panel".into(),
        )];
        let mut tools = vec![
            button::text("Check again")
                .id("check-islands".into())
                .on_press_maybe(
                    (!self.installation_busy && !self.updates.islands.busy())
                        .then_some(Message::Updates(Update::Islands(Panel::Recheck))),
                )
                .into(),
        ];
        if installation.diagnostics().is_some() {
            tools.push(
                button::text("Copy details for a bug report")
                    .on_press(Message::CopyIslandsDiagnostics)
                    .into(),
            );
        }
        tools.push(
            button::text("Setup guide ↗")
                .on_press(Message::OpenIslandsGuide)
                .into(),
        );
        rows.push(row::with_children(tools).spacing(8).wrap().into());
        if islands::can_disable(installation)
            && self
                .updates
                .identity
                .as_ref()
                .is_some_and(|id| id.can_enable_islands())
        {
            rows.push(group_row(
                "Go back to the COSMIC panel",
                "From your next sign-in. Your Islands layout is kept.",
                button::standard("Switch back")
                    .id("disable-islands".into())
                    .on_press_maybe(
                        (!self.updates.islands.busy())
                            .then_some(Message::Updates(Update::Islands(Panel::Disable))),
                    )
                    .into(),
            ));
        }
        rows
    }
}
