//! Update discovery and included Islands activation. The OS installs updates.
mod identity;
pub(super) mod islands;
mod packagekit;
mod summary;
pub(super) use summary::Summary;
#[cfg(test)]
mod tests;

use super::{Message as SettingsMessage, Settings, Task};
pub(super) use identity::{Enrollment, Identity, UpdateChannel};
use std::time::Duration;
use tokio::sync::watch;

const RELEASES: &str = "https://github.com/profilaz3/halcyon/releases";
const INFORMATION: &str = "https://github.com/profilaz3/halcyon/blob/main/docs/DISTRIBUTION.md";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    MissingPackageKit,
    Offline,
    Timeout,
    Cancelled,
    Authorization,
    Untrusted,
    InvalidResponse,
    Failed,
    InstallationChanged,
    Activation,
    IslandsMismatch,
}
impl Failure {
    pub fn label(&self) -> &'static str {
        match self {
            Self::IslandsMismatch => {
                "The Islands package changed or does not match this Halcyon. Update or reinstall Halcyon in your package manager, then check again."
            }
            Self::Activation => {
                "Islands could not be changed. Check Islands diagnostics and the installation guide before retrying."
            }
            Self::MissingPackageKit => "System update service unavailable.",
            Self::Offline => "Couldn’t reach the package sources. Try again when online.",
            Self::Timeout => "The system did not finish in time. Recheck its status.",
            Self::Cancelled => "Operation cancelled.",
            Self::Authorization => "System authorization was not granted.",
            Self::Untrusted => "Package sources could not be verified.",
            Self::InvalidResponse => "The system returned an incomplete update result.",
            Self::Failed => "The system could not complete this operation.",
            Self::InstallationChanged => "Installation changed. Reopen Settings to check it again.",
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Check {
    #[default]
    NotChecked,
    Checking,
    Current,
    Available(String),
    Failed(Failure),
}
#[derive(Clone, Debug)]
pub enum Message {
    Islands(islands::Message),
    Inspect,
    Inspected(u64, Identity),
    Check,
    Cancel,
    Checked(u64, Result<Option<String>, Failure>),
    OpenStore,
    StoreOpened(u64, Result<(), String>),
    Releases,
    Information,
    Notes,
}
#[derive(Default)]
pub struct Updates {
    pub islands: islands::State,
    pub identity: Option<Identity>,
    pub check: Check,
    pub store_error: Option<String>,
    pub store_busy: bool,
    pub last_checked: Option<chrono::DateTime<chrono::Local>>,
    generation: u64,
    cancellation: Option<watch::Sender<bool>>,
}
impl Drop for Updates {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancellation.take() {
            let _ = cancel.send(true);
        }
    }
}
impl Updates {
    pub fn cancel(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(cancel) = self.cancellation.take() {
            let _ = cancel.send(true);
        }
        if self.check == Check::Checking {
            self.check = Check::Failed(Failure::Cancelled);
        }
        self.store_busy = false;
    }
    fn begin(&mut self) -> (u64, watch::Receiver<bool>) {
        self.cancel();
        let (sender, receiver) = watch::channel(false);
        self.cancellation = Some(sender);
        self.check = Check::Checking;
        (self.generation, receiver)
    }
    fn finish(&mut self, token: u64, result: Result<Option<String>, Failure>) {
        if token != self.generation || self.check != Check::Checking {
            return;
        }
        self.cancellation = None;
        if result.is_ok() {
            self.last_checked = Some(chrono::Local::now());
        }
        self.check = match result {
            Ok(Some(version)) => Check::Available(version),
            Ok(None) => Check::Current,
            Err(error) => Check::Failed(error),
        };
    }
}
impl Settings {
    pub(super) fn update_updates(&mut self, message: Message) -> Task<SettingsMessage> {
        match message {
            Message::Islands(message) => return islands::update(self, message),
            Message::Inspect => {
                // Focus changes during PolicyKit authentication must not cancel
                // the check or start another local measurement.
                if self.updates.check == Check::Checking || self.updates.store_busy {
                    return Task::none();
                }
                self.updates.cancel();
                self.updates.identity = None;
                self.updates.check = Check::NotChecked;
                self.updates.store_error = None;
                let token = self.updates.generation;
                return cosmic::task::future(async move {
                    SettingsMessage::Updates(Message::Inspected(token, identity::inspect().await))
                });
            }
            Message::Inspected(token, value) if token == self.updates.generation => {
                self.updates.identity = Some(value);
            }
            Message::Check => {
                let Some(identity) = self.updates.identity.clone().filter(|i| i.can_check()) else {
                    return Task::none();
                };
                if self.updates.check == Check::Checking {
                    return Task::none();
                }
                let (token, cancel) = self.updates.begin();
                return cosmic::task::future(async move {
                    let result = check_installation(identity, cancel).await;
                    SettingsMessage::Updates(Message::Checked(token, result))
                });
            }
            Message::Cancel => self.updates.cancel(),
            Message::Checked(token, result) => self.updates.finish(token, result),
            Message::OpenStore => {
                if self.updates.store_busy
                    || !self
                        .updates
                        .identity
                        .as_ref()
                        .is_some_and(|i| i.can_check() && i.store.is_some())
                {
                    return Task::none();
                }
                self.updates.store_busy = true;
                self.updates.store_error = None;
                let token = self.updates.generation;
                return cosmic::task::future(async move {
                    SettingsMessage::Updates(Message::StoreOpened(
                        token,
                        identity::open_store().await,
                    ))
                });
            }
            Message::StoreOpened(token, result) if token == self.updates.generation => {
                self.updates.store_busy = false;
                self.updates.store_error = result.err();
            }
            Message::Releases => {
                return super::run_command(
                    "/usr/bin/xdg-open".into(),
                    vec![RELEASES.into()],
                    "Releases opened",
                );
            }
            Message::Notes => {
                if let Some(url) = self.updates.notes_url() {
                    return super::run_command(
                        "/usr/bin/xdg-open".into(),
                        vec![url],
                        "Release notes opened",
                    );
                }
            }
            Message::Information => {
                return super::run_command(
                    "/usr/bin/xdg-open".into(),
                    vec![INFORMATION.into()],
                    "Update information opened",
                );
            }
            _ => {}
        }
        Task::none()
    }
}
async fn check_installation(
    identity: Identity,
    mut cancel: watch::Receiver<bool>,
) -> Result<Option<String>, Failure> {
    // Revalidate before contacting the bus, and before accepting its result.
    // No fixture/capture can ever fall through to the host system bus.
    if identity::isolated() {
        return Err(Failure::InstallationChanged);
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    verify_identity(&identity, &mut cancel, deadline).await?;
    let result = packagekit::check_system(
        cancel.clone(),
        deadline.saturating_duration_since(tokio::time::Instant::now()),
    )
    .await?;
    verify_identity(&identity, &mut cancel, deadline).await?;
    tokio::select! {
        biased;
        _ = packagekit::cancelled(&mut cancel) => Err(Failure::Cancelled),
        result = tokio::time::timeout_at(deadline,packagekit::candidate(&identity,&result)) => result.unwrap_or(Err(Failure::Timeout)),
    }
}
async fn verify_identity(
    identity: &Identity,
    cancel: &mut watch::Receiver<bool>,
    deadline: tokio::time::Instant,
) -> Result<(), Failure> {
    let current = tokio::select! {
        biased;
        _ = packagekit::cancelled(cancel) => return Err(Failure::Cancelled),
        current = tokio::time::timeout_at(deadline,identity::inspect()) => current.map_err(|_|Failure::Timeout)?,
    };
    if identity.same_installation(&current) {
        Ok(())
    } else {
        Err(Failure::InstallationChanged)
    }
}

/// The existing helper's selected generation must match the installed payload.
/// A newer archive candidate or an unstaged installed generation isn't ready.
pub(super) fn pending_islands(installation: &crate::desktop::islands::Installation) -> bool {
    use crate::desktop::islands::{CompatibilityStatus, Installation};
    let Installation::Installed {
        enabled: true,
        compatible: Some(true),
        status: Some(CompatibilityStatus::LogoutRequired),
        diagnostics,
        ..
    } = installation
    else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(diagnostics) else {
        return false;
    };
    let Some(installed) = value["installed_generation"].as_str() else {
        return false;
    };
    installed.len() == 64
        && installed.bytes().all(|c| c.is_ascii_hexdigit())
        && value["active_generation"].as_str() == Some(installed)
}

impl Updates {
    pub fn notes_url(&self) -> Option<String> {
        let identity = self
            .identity
            .as_ref()
            .filter(|i| i.can_check() || i.can_manage_islands())?;
        let version = match &self.check {
            Check::Available(v) => v,
            _ => identity.installed_version.as_ref()?,
        };
        // Only stable upstream x.y.z tags; do not invent tags for epochs or prereleases.
        let upstream = version.split_once('-').map_or(version.as_str(), |(v, _)| v);
        let parts: Vec<_> = upstream.split('.').collect();
        (parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.len() <= 5 && p.bytes().all(|c| c.is_ascii_digit())))
        .then(|| format!("{RELEASES}/tag/v{upstream}"))
    }
}

impl Updates {
    /// Isolated native captures only. Checks/installations still reject fixtures.
    pub(super) fn capture_fixture(&mut self, state: &str) {
        if !super::capture_commands() || !identity::isolated() {
            return;
        }
        let mut identity = Identity {
            running_version: "0.2.5".into(),
            installed_version: Some("0.2.5".into()),
            architecture: Some("amd64".into()),
            ..Default::default()
        };
        match state {
            "manual" => {
                identity.channel = UpdateChannel::ManualDeb;
                identity.enrollment = Enrollment::Available;
                identity.reason = "System updates are not enabled.";
                identity.evidence = Some("capture fixture".into());
            }
            "enrolled" | "available" => {
                identity.channel = UpdateChannel::Apt;
                identity.enrollment = Enrollment::Enrolled;
                identity.reason = "System updates enabled.";
                identity.evidence = Some("capture fixture".into());
                identity.store = Some("/fixture/store.desktop".into());
            }
            "disabled" => {
                identity.channel = UpdateChannel::ManualDeb;
                identity.enrollment = Enrollment::Disabled;
                identity.reason = "The Halcyon package source is disabled.";
            }
            _ => return,
        }
        self.cancel();
        self.identity = Some(identity);
        self.check = if state == "available" {
            Check::Available("0.3.0".into())
        } else {
            Check::NotChecked
        };
    }
}
