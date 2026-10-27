use super::*;
use crate::settings::updates::{Check, Message as Update, UpdateChannel};

impl Settings {
    pub(in crate::settings) fn updates_section(&self) -> Element<'_, Message> {
        let mut content = Vec::new();
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
                content.push(
                    column::with_children(vec![
                        label(summary.title.clone(), 20., FG),
                        label(summary.detail.clone(), 13., MUTED),
                    ])
                    .spacing(6)
                    .into(),
                );
                let main = identity
                    .installed_version
                    .as_deref()
                    .unwrap_or(&identity.running_version);
                content.push(split(label("Halcyon", 14., FG), label(main, 14., MUTED)));
                let addon = identity.islands_version.as_ref().map_or_else(
                    || self.desktop.islands.installation.status_label().to_string(),
                    |v| format!("{v} · {}", self.desktop.islands.installation.status_label()),
                );
                content.push(split(label("Islands", 14., FG), label(addon, 12., MUTED)));
                if let Check::Failed(error) = &self.updates.check
                    && summary.title != "Update check failed"
                {
                    content.push(label(error.label(), 12., MUTED));
                }
                match identity.channel {
                    UpdateChannel::Apt if identity.can_check() => {
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
                                    button::suggested("Update in COSMIC Store")
                                } else {
                                    button::standard("Open COSMIC Store")
                                })
                                .id(cosmic::widget::Id::new("updates-store"))
                                .on_press_maybe(
                                    (!self.updates.store_busy)
                                        .then_some(Message::Updates(Update::OpenStore)),
                                )
                                .into(),
                            );
                            content.push(self.help("updates-system", "How updates work", "Open Updates in COSMIC Store and install the offered packages. The signed Halcyon source supplies matching app and Islands releases. Islands is not added unless you choose to install it. A new Islands host starts at your next login; no logout happens automatically."));
                        } else {
                            content.push(label(
                                "COSMIC Store is unavailable. Use your system package manager.",
                                12.,
                                MUTED,
                            ));
                        }
                    }
                    UpdateChannel::ManualDeb => {
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
                        "Installation information",
                        "updates-information",
                        Update::Information,
                    )),
                }
            }
        }
        if let Some(at) = self.updates.last_checked {
            content.push(label(
                format!("Last successful check: {}", at.format("%e %b, %H:%M")),
                12.,
                MUTED,
            ));
        }
        if self.updates.notes_url().is_some() {
            content.push(
                button::text("Release notes ↗")
                    .on_press(Message::Updates(Update::Notes))
                    .into(),
            );
        }
        if let Some(error) = &self.updates.store_error {
            content.push(label(error, 12., MUTED));
        }
        if !actions.is_empty() {
            content.push(row::with_children(actions).spacing(8).wrap().into());
        }
        pages::section(content)
    }
}
