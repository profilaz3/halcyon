//! Included Islands payload. The guarded helper activates it for the next login.
use super::{Failure, Identity, Settings, SettingsMessage, Task, identity, packagekit};
use crate::desktop::islands::{CompatibilityStatus, Installation, Provenance};
use std::{process::Stdio, time::Duration};
use tokio::sync::watch;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    Enabling,
    Failed(Failure),
}
#[derive(Default)]
pub struct State {
    pub phase: Phase,
    generation: u64,
    cancellation: Option<watch::Sender<bool>>,
}
impl State {
    pub fn busy(&self) -> bool {
        matches!(self.phase, Phase::Enabling)
    }
    fn reset(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(cancel) = self.cancellation.take() {
            let _ = cancel.send(true);
        }
        self.phase = Phase::Idle;
    }
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancellation.take() {
            let _ = cancel.send(true);
        }
    }
}
#[derive(Clone, Debug)]
pub enum Message {
    Enable,
    Disable,
    Enabled(u64, Activation),
    Recheck,
}
fn message(value: Message) -> SettingsMessage {
    SettingsMessage::Updates(super::Message::Islands(value))
}
pub(super) fn update(settings: &mut Settings, value: Message) -> Task<SettingsMessage> {
    let state = &mut settings.updates.islands;
    match value {
        Message::Enable
            if !state.busy()
                && can_enable(&settings.desktop.islands.installation)
                && settings
                    .updates
                    .identity
                    .as_ref()
                    .is_some_and(Identity::can_enable_islands) =>
        {
            state.reset();
            state.phase = Phase::Enabling;
            let (sender, cancel) = watch::channel(false);
            state.cancellation = Some(sender);
            let token = state.generation;
            return cosmic::task::future(async move {
                message(Message::Enabled(token, enable(cancel).await))
            });
        }
        Message::Disable
            if !state.busy()
                && can_disable(&settings.desktop.islands.installation)
                && settings
                    .updates
                    .identity
                    .as_ref()
                    .is_some_and(Identity::can_enable_islands) =>
        {
            state.reset();
            state.phase = Phase::Enabling;
            let (sender, cancel) = watch::channel(false);
            state.cancellation = Some(sender);
            let token = state.generation;
            return cosmic::task::future(async move {
                message(Message::Enabled(token, disable(cancel).await))
            });
        }
        Message::Enabled(token, result)
            if token == state.generation && state.phase == Phase::Enabling =>
        {
            state.cancellation = None;
            if let Some(failure) = &result.failure {
                failure.count("islands");
            }
            state.phase = result.failure.map(Phase::Failed).unwrap_or_default();
            if let Some(installation) = result.observed {
                settings.desktop.islands.installation = installation;
            }
            return refresh_installation(settings);
        }
        Message::Recheck if !state.busy() => {
            state.reset();
            return refresh_installation(settings);
        }
        _ => {}
    }
    Task::none()
}
fn refresh_installation(settings: &mut Settings) -> Task<SettingsMessage> {
    Task::batch([
        settings.handle(SettingsMessage::RecheckInstallation),
        settings.update_updates(super::Message::Inspect),
    ])
}
pub fn can_enable(installation: &Installation) -> bool {
    let Installation::Installed {
        provenance: Provenance::Package,
        enabled: false,
        compatible,
        status,
        ..
    } = installation
    else {
        return false;
    };
    // Disabled packages intentionally skip native measurement. Offer the explicit
    // request, then let the guarded helper qualify/stage it under its own lock.
    // Failure/ownership warnings must not become a generic retry button.
    matches!(
        (compatible, status),
        (Some(true), None | Some(CompatibilityStatus::Disabled))
            | (None, Some(CompatibilityStatus::Disabled))
    )
}
pub fn can_disable(installation: &Installation) -> bool {
    matches!(
        installation,
        Installation::Installed {
            provenance: Provenance::Package,
            enabled: true,
            ..
        }
    )
}
async fn disable(cancel: watch::Receiver<bool>) -> Activation {
    if identity::isolated()
        || !identity::inspect().await.can_enable_islands()
        || !identity::islands_cli_verified()
    {
        return Failure::InstallationChanged.into();
    }
    if !discover().await.as_ref().is_ok_and(can_disable) {
        return Failure::InstallationChanged.into();
    }
    let mut command = tokio::process::Command::new("/usr/bin/halcyon-islands");
    command.arg("disable");
    let operation = run_activation(command, cancel, Duration::from_secs(60)).await;
    match discover().await {
        Ok(observed) => {
            let off = matches!(observed, Installation::Installed { enabled: false, .. });
            Activation {
                observed: Some(observed),
                failure: if off {
                    None
                } else {
                    Some(operation.err().unwrap_or(Failure::Activation))
                },
            }
        }
        Err(error) => operation.err().unwrap_or(error).into(),
    }
}
async fn discover() -> Result<Installation, Failure> {
    tokio::task::spawn_blocking(|| {
        crate::desktop::DesktopStore::from_env()
            .map(|store| Installation::discover(&store))
            .map_err(|_| Failure::Failed)
    })
    .await
    .map_err(|_| Failure::Failed)?
}
#[derive(Clone, Debug)]
pub struct Activation {
    observed: Option<Installation>,
    failure: Option<Failure>,
}
impl From<Failure> for Activation {
    fn from(failure: Failure) -> Self {
        Self {
            observed: None,
            failure: Some(failure),
        }
    }
}
fn activation_result(
    operation: Result<(), Failure>,
    observed: Result<Installation, Failure>,
) -> Activation {
    match observed {
        Ok(installation) => {
            // A helper can finish staging just before its deadline. Conversely,
            // an interrupted journal is not proof that nothing changed.
            let staged = matches!(
                installation,
                Installation::Installed {
                    provenance: Provenance::Package,
                    enabled: true,
                    status: Some(CompatibilityStatus::Ready | CompatibilityStatus::LogoutRequired),
                    ..
                }
            );
            Activation {
                observed: Some(installation),
                failure: if staged {
                    None
                } else {
                    Some(operation.err().unwrap_or(Failure::Activation))
                },
            }
        }
        Err(error) => operation.err().unwrap_or(error).into(),
    }
}
async fn run_activation(
    mut command: tokio::process::Command,
    mut cancel: watch::Receiver<bool>,
    limit: Duration,
) -> Result<(), Failure> {
    // This owner outlives a dropped UI future. State::drop closes/signals the
    // watch, so it still kills and reaps the helper before exiting. kill_on_drop
    // is the final guard for runtime teardown, not the timeout implementation.
    tokio::spawn(async move {
        if *cancel.borrow() || cancel.has_changed().is_err() {
            return Err(Failure::Cancelled);
        }
        let mut child = command
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| Failure::Failed)?;
        let stdout = tokio::spawn(bounded_output(child.stdout.take()));
        let stderr = tokio::spawn(bounded_output(child.stderr.take()));
        let status = tokio::select! {
            biased;
            _ = packagekit::cancelled(&mut cancel) => Err(Failure::Cancelled),
            _ = tokio::time::sleep(limit) => Err(Failure::Timeout),
            result = child.wait() => result.map_err(|_| Failure::Failed),
        };
        match status {
            Ok(status) if status.success() => Ok(()),
            Ok(_) => {
                let stdout = stdout.await.unwrap_or_default();
                let stderr = stderr.await.unwrap_or_default();
                Err(refusal(&stdout, &stderr).map_or(Failure::Activation, Failure::IslandsRefused))
            }
            Err(error) => {
                child.kill().await.map_err(|_| Failure::Activation)?;
                Err(error)
            }
        }
    })
    .await
    .map_err(|_| Failure::Activation)?
}
/// Reads at most 64 KiB, and gives up if a stray descendant keeps the pipe open.
async fn bounded_output<R: tokio::io::AsyncRead + Unpin>(pipe: Option<R>) -> String {
    use tokio::io::AsyncReadExt;
    let Some(pipe) = pipe else {
        return String::new();
    };
    let mut bytes = Vec::new();
    let mut pipe = pipe.take(64 * 1024);
    let _ = tokio::time::timeout(Duration::from_secs(1), pipe.read_to_end(&mut bytes)).await;
    String::from_utf8_lossy(&bytes).into_owned()
}
/// Turns the helper's machine-readable refusal into something a person can act on.
/// Unknown reasons keep the generic message rather than guessing.
fn refusal(stdout: &str, stderr: &str) -> Option<&'static str> {
    if stderr.contains("PATH must put ~/.local/bin before /usr/bin") {
        return Some(
            "Islands needs ~/.local/bin ahead of /usr/bin in your session PATH. Fix your shell profile, sign out and back in, then try again.",
        );
    }
    if stderr.contains("previous Islands failure") {
        return Some(
            "Islands failed at an earlier sign-in, so the standard panel stays in use. Check Islands diagnostics before retrying.",
        );
    }
    let reason = stdout
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())?
        .get("reason")?
        .as_str()?
        .to_owned();
    Some(match reason.as_str() {
        "unowned_selector" => {
            "Another program put its own cosmic-panel in ~/.local/bin. Move it away, then try again."
        }
        "owned_files_changed" | "generation_missing" => {
            "Islands files in your home folder were changed outside Halcyon. Check Islands diagnostics before retrying."
        }
        "busy" => "Islands is already being changed. Try again in a moment.",
        "session_unknown" | "not_cosmic_session" => {
            "Couldn’t identify your COSMIC session. Sign out and back in, then try again."
        }
        "unsupported_data_location" => {
            "Islands needs the default data folder. Unset XDG_DATA_HOME for your session, then try again."
        }
        "unsupported_cosmic_family" | "no_qualified_cohort" | "installed_runtime_unqualified" => {
            "This Islands release doesn’t support your COSMIC version yet. The standard panel stays in use."
        }
        "package_absent"
        | "package_version_mismatch"
        | "package_changed_during_staging"
        | "not_installed_package"
        | "non_monotonic_version" => Failure::IslandsMismatch.label(),
        _ => return None,
    })
}
async fn enable(cancel: watch::Receiver<bool>) -> Activation {
    enable_checked(cancel)
        .await
        .unwrap_or_else(Activation::from)
}
async fn enable_checked(cancel: watch::Receiver<bool>) -> Result<Activation, Failure> {
    if identity::isolated() {
        return Err(Failure::InstallationChanged);
    }
    if !identity::inspect().await.can_enable_islands() || !identity::islands_cli_verified() {
        return Err(Failure::IslandsMismatch);
    }
    if !can_enable(&discover().await?) {
        return Err(Failure::InstallationChanged);
    }
    // No sudo, panel restart, latch removal, or alternate activation path.
    // The helper owns locking, source/ABI admission, ownership and rollback.
    let mut command = tokio::process::Command::new("/usr/bin/halcyon-islands");
    command.arg("enable");
    let operation = run_activation(command, cancel, Duration::from_secs(60)).await;
    // Inspect only after the owned helper has exited, including interruption.
    Ok(activation_result(operation, discover().await))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    struct Fixture(std::path::PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[tokio::test]
    async fn activation_helper_is_reaped_on_timeout_cancel_and_ui_drop() {
        for mode in ["timeout", "cancel", "drop"] {
            let root = Fixture(std::env::temp_dir().join(format!(
                    "halcyon-activation-{}-{mode}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                )));
            std::fs::create_dir(&root.0).unwrap();
            let ready = root.0.join("ready");
            let changed = root.0.join("changed");
            let mut command = tokio::process::Command::new("/usr/bin/python3");
            command.arg("-c").arg(
                "import os,sys,time; from pathlib import Path; Path(sys.argv[1]).write_text(str(os.getpid())); time.sleep(30); Path(sys.argv[2]).write_text('late change')"
            ).arg(&ready).arg(&changed);
            let (sender, cancel) = watch::channel(false);
            let limit = Duration::from_secs(if mode == "timeout" { 1 } else { 30 });
            let task = tokio::spawn(run_activation(command, cancel, limit));
            let pid = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if let Ok(value) = std::fs::read_to_string(&ready) {
                        if let Ok(pid) = value.parse::<i32>() {
                            break pid;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("fixture never started");
            if mode == "drop" {
                task.abort();
                drop(sender);
            } else {
                if mode == "cancel" {
                    sender.send(true).unwrap();
                }
                assert_eq!(
                    task.await.unwrap(),
                    Err(if mode == "timeout" {
                        Failure::Timeout
                    } else {
                        Failure::Cancelled
                    })
                );
            }
            tokio::time::timeout(Duration::from_secs(3), async {
                while Path::new(&format!("/proc/{pid}")).exists() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("activation helper survived or was not reaped");
            assert!(!changed.exists());
        }
    }
    #[tokio::test]
    async fn activation_helper_exit_and_prelaunch_cancellation() {
        let (sender, cancel) = watch::channel(false);
        assert_eq!(
            run_activation(
                tokio::process::Command::new("/bin/true"),
                cancel.clone(),
                Duration::from_secs(2)
            )
            .await,
            Ok(())
        );
        assert_eq!(
            run_activation(
                tokio::process::Command::new("/bin/false"),
                cancel.clone(),
                Duration::from_secs(2)
            )
            .await,
            Err(Failure::Activation)
        );
        drop(sender);
        assert_eq!(
            run_activation(
                tokio::process::Command::new("/does/not/exist"),
                cancel,
                Duration::from_secs(2)
            )
            .await,
            Err(Failure::Cancelled)
        );
    }
    #[test]
    fn helper_refusals_become_actionable_messages() {
        let refused = |stdout: &str, stderr: &str| refusal(stdout, stderr);
        assert!(
            refused(
                r#"{"state": "external_change", "reason": "unowned_selector", "generation": null}"#,
                ""
            )
            .unwrap()
            .contains("~/.local/bin")
        );
        assert!(
            refused(
                "",
                "Islands: The session PATH must put ~/.local/bin before /usr/bin"
            )
            .unwrap()
            .contains("PATH")
        );
        assert!(refused(
            "",
            "Islands: A previous Islands failure selects the stock panel. Activation was not changed."
        )
        .unwrap()
        .contains("earlier sign-in"));
        assert_eq!(
            refused(r#"{"reason": "package_absent"}"#, ""),
            Some(Failure::IslandsMismatch.label())
        );
        assert_eq!(refused(r#"{"reason": "something_new"}"#, ""), None);
        assert_eq!(refused("not json", "Traceback ..."), None);
    }
    #[tokio::test]
    async fn activation_failure_reports_the_helper_reason() {
        let (_sender, cancel) = watch::channel(false);
        let mut command = tokio::process::Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(r#"echo '{"state": "blocked", "reason": "busy"}'; exit 1"#);
        assert_eq!(
            run_activation(command, cancel, Duration::from_secs(5)).await,
            Err(Failure::IslandsRefused(
                "Islands is already being changed. Try again in a moment."
            ))
        );
    }
    #[test]
    fn activation_completion_uses_observed_state_after_interruption() {
        for status in [
            CompatibilityStatus::LogoutRequired,
            CompatibilityStatus::UpdateNeeded,
            CompatibilityStatus::Disabled,
        ] {
            let installation = Installation::Installed {
                provenance: Provenance::Package,
                enabled: status != CompatibilityStatus::Disabled,
                compatible: None,
                status: Some(status),
                diagnostics: "fixture".into(),
            };
            let result = activation_result(Err(Failure::Timeout), Ok(installation.clone()));
            assert_eq!(result.observed, Some(installation));
            assert_eq!(
                result.failure,
                if status == CompatibilityStatus::LogoutRequired {
                    None
                } else {
                    Some(Failure::Timeout)
                }
            );
        }
        let result = activation_result(Ok(()), Err(Failure::Failed));
        assert_eq!(result.failure, Some(Failure::Failed));
        assert!(result.observed.is_none());
    }
    #[tokio::test]
    async fn isolated_flows_cannot_activate_or_disable() {
        let (_sender, cancel) = watch::channel(false);
        assert_eq!(
            disable(cancel).await.failure,
            Some(Failure::InstallationChanged)
        );
        let (_sender, cancel) = watch::channel(false);
        assert_eq!(
            enable(cancel).await.failure,
            Some(Failure::InstallationChanged)
        );
    }
    #[test]
    fn activation_requires_disabled_package_and_defers_unknown_measurement_to_helper() {
        for provenance in [Provenance::Package, Provenance::Manual] {
            for enabled in [false, true] {
                for compatible in [None, Some(false), Some(true)] {
                    for status in [
                        None,
                        Some(CompatibilityStatus::Disabled),
                        Some(CompatibilityStatus::UpdateNeeded),
                    ] {
                        let state = Installation::Installed {
                            provenance,
                            enabled,
                            compatible,
                            status,
                            diagnostics: String::new(),
                        };
                        assert_eq!(
                            can_enable(&state),
                            provenance == Provenance::Package
                                && !enabled
                                && (compatible == Some(true) && status.is_none()
                                    || compatible != Some(false)
                                        && status == Some(CompatibilityStatus::Disabled))
                        );
                    }
                }
            }
        }
    }
    #[tokio::test]
    async fn cancellation_invalidates_in_flight_results() {
        let mut state = State::default();
        let token = state.generation;
        let (sender, mut receiver) = watch::channel(false);
        state.cancellation = Some(sender);
        state.phase = Phase::Enabling;
        state.reset();
        receiver.changed().await.unwrap();
        assert!(*receiver.borrow());
        assert_ne!(state.generation, token);
        assert_eq!(state.phase, Phase::Idle);
    }
}
