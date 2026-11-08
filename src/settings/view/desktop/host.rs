//! Islands and Blend into wallpaper both need Halcyon's own panel. This is the
//! single place that says whether it is running and, if not, the one next step.
//! Diagnostics and rechecks live in App settings › Updates, not here.
use super::*;
use crate::desktop::islands::{CompatibilityStatus, Installation, Provenance};
use crate::settings::updates::Message as Update;
use crate::settings::updates::islands::{self, Message as Panel, Phase};

/// What stands between the person and Halcyon's panel, in their terms.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::settings) enum Host {
    /// It answers right now; its features work.
    Running,
    /// Installation or package checks haven't finished.
    Checking,
    /// The helper is preparing it for the next sign-in.
    Switching,
    /// One click away: switch at the next sign-in.
    Switch,
    /// Prepared; it starts after signing out and back in.
    SignIn,
    /// The last attempt failed with this reason.
    Failed(&'static str),
    /// Nothing here can fix it; Troubleshooting explains.
    Unavailable(&'static str),
}

const READY: Color = Color::from_rgb(0.45, 0.82, 0.55);

fn actions(items: Vec<Element<'_, Message>>) -> Element<'_, Message> {
    row::with_children(items).spacing(8).wrap().into()
}

fn update(message: Panel) -> Message {
    Message::Updates(Update::Islands(message))
}

impl Settings {
    fn panel_verified(&self) -> bool {
        self.updates
            .identity
            .as_ref()
            .is_some_and(|id| id.can_enable_islands())
    }

    pub(in crate::settings) fn panel_host(&self) -> Host {
        let installation = &self.desktop.islands.installation;
        let state = &self.updates.islands;
        if state.busy() {
            return Host::Switching;
        }
        if let Phase::Failed(error) = &state.phase {
            return Host::Failed(error.label());
        }
        if self.desktop.islands.compatible {
            return Host::Running;
        }
        let checking = self.installation_busy
            || self.updates.identity.is_none()
            || matches!(
                installation,
                Installation::Pending
                    | Installation::Installed {
                        status: Some(CompatibilityStatus::Checking),
                        ..
                    }
            );
        if checking {
            return Host::Checking;
        }
        if matches!(
            installation,
            Installation::Installed {
                enabled: true,
                status: Some(CompatibilityStatus::LogoutRequired),
                ..
            }
        ) {
            return Host::SignIn;
        }
        if islands::can_enable(installation) {
            return match self.panel_verified() {
                true => Host::Switch,
                false => Host::Unavailable(
                    self.updates
                        .identity
                        .as_ref()
                        .and_then(|id| id.islands_activation_notice())
                        .unwrap_or("Halcyon’s panel doesn’t match this Halcyon. Update or reinstall Halcyon."),
                ),
            };
        }
        Host::Unavailable(match installation {
            Installation::Absent => "Halcyon’s panel isn’t installed. Reinstall Halcyon to get it.",
            Installation::Installed {
                provenance: Provenance::Manual,
                ..
            } => {
                "Not available with a manually installed panel. Install the Halcyon package to use it."
            }
            Installation::Installed {
                compatible: Some(false),
                ..
            }
            | Installation::Installed {
                status: Some(CompatibilityStatus::UpdateNeeded),
                ..
            } => {
                "Halcyon’s panel doesn’t support this version of COSMIC yet. Update Halcyon, then sign in again."
            }
            Installation::Installed { enabled: true, .. } => {
                "Halcyon’s panel didn’t start, so the COSMIC panel is in use."
            }
            _ => "Halcyon’s panel isn’t available right now.",
        })
    }

    /// The one next step toward Halcyon's panel, or None once it runs.
    /// `lead` says what asked for it, e.g. "Islands use".
    pub(super) fn host_step(&self, lead: &str) -> Option<Element<'_, Message>> {
        // Updates holds the panel's checks and fixes; the link says so, and
        // lines up with the text above it.
        let help = || -> Element<'_, Message> {
            button::text("Troubleshoot in Updates")
                .id("islands-help".into())
                .padding([4, 0])
                .on_press(Message::AppSettingsPage(AppSettingsPage::Updates))
                .into()
        };
        let rows: Vec<Element<'_, Message>> = match self.panel_host() {
            Host::Running => return None,
            Host::Checking => vec![label("Checking Halcyon’s panel…", 12., MUTED)],
            Host::Switching => vec![label("Getting Halcyon’s panel ready…", 12., MUTED)],
            Host::Switch => vec![
                label(
                    format!(
                        "{lead} Halcyon’s panel instead of the COSMIC one. \
                         It takes over the next time you sign in."
                    ),
                    12.,
                    MUTED,
                ),
                actions(vec![
                    button::suggested("Switch at next sign-in")
                        .id("enable-islands".into())
                        .on_press(update(Panel::Enable))
                        .into(),
                ]),
            ],
            Host::SignIn => {
                let mut buttons = vec![
                    button::suggested("Sign out now")
                        .id("sign-out-islands".into())
                        .on_press(Message::SignOut)
                        .into(),
                ];
                if islands::can_disable(&self.desktop.islands.installation) && self.panel_verified()
                {
                    buttons.push(
                        button::text("Undo")
                            .id("disable-islands".into())
                            .on_press(update(Panel::Disable))
                            .into(),
                    );
                }
                vec![
                    row::with_children(vec![
                        dot(READY, 8.),
                        label("Ready. Sign out and back in to see it.", 12., FG),
                    ])
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
                    actions(buttons),
                ]
            }
            Host::Failed(reason) => {
                let retry = if islands::can_enable(&self.desktop.islands.installation)
                    && self.panel_verified()
                {
                    Panel::Enable
                } else {
                    Panel::Recheck
                };
                vec![banner(
                    reason,
                    WARN,
                    vec![
                        button::standard("Try again")
                            .id("retry-islands".into())
                            .on_press(update(retry))
                            .into(),
                    ],
                )]
            }
            Host::Unavailable(reason) => vec![label(reason, 12., MUTED), actions(vec![help()])],
        };
        Some(
            container(column::with_children(rows).spacing(8))
                .id(cosmic::widget::Id::new("panel-host"))
                .width(Length::Fill)
                .into(),
        )
    }
}
