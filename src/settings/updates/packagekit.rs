//! Read-only PackageKit update checks. Installation belongs to the system store.
//! Enum numbers verified against installed libpackagekit-glib2.so.18 using its
//! pk_exit_enum_from_string / pk_error_enum_from_string (no service calls).
use super::{Failure, Identity};
use cosmic::iced::futures::StreamExt;
use std::{process::Stdio, time::Duration};
use tokio::{
    sync::watch,
    time::{Instant, timeout, timeout_at},
};
use zbus::{Connection, MessageStream, zvariant::OwnedObjectPath};
const SERVICE: &str = "org.freedesktop.PackageKit";
const ROOT: &str = "/org/freedesktop/PackageKit";
const TRANSACTION: &str = "org.freedesktop.PackageKit.Transaction";
const EXIT_SUCCESS: u32 = 1;
const FILTER_NONE: u64 = 1 << 1;
const STEP_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) async fn cancelled(cancel: &mut watch::Receiver<bool>) {
    loop {
        if *cancel.borrow_and_update() {
            return;
        }
        if cancel.changed().await.is_err() {
            return;
        }
    }
}
fn bus_error(error: zbus::Error) -> Failure {
    use zbus::DBusError;
    let name = match &error {
        zbus::Error::MethodError(name, _, _) => name.to_string(),
        zbus::Error::FDO(error) => error.name().to_string(),
        _ => return Failure::Failed,
    };
    match name.as_str() {
        "org.freedesktop.DBus.Error.ServiceUnknown"
        | "org.freedesktop.DBus.Error.NameHasNoOwner"
        | "org.freedesktop.DBus.Error.Spawn.ServiceNotFound"
        | "org.freedesktop.DBus.Error.UnknownMethod"
        | "org.freedesktop.DBus.Error.UnknownInterface"
        | "org.freedesktop.DBus.Error.UnknownObject" => Failure::MissingPackageKit,
        "org.freedesktop.DBus.Error.AccessDenied"
        | "org.freedesktop.PackageKit.Transaction.RefusedByPolicy"
        | "org.freedesktop.PackageKit.Denied" => Failure::Authorization,
        "org.freedesktop.DBus.Error.NoReply" | "org.freedesktop.DBus.Error.Timeout" => {
            Failure::Timeout
        }
        _ => Failure::Failed,
    }
}
fn error_code(code: u32) -> Failure {
    match code {
        2 | 37 | 64 => Failure::Offline,
        48 => Failure::Authorization,
        17 | 65 => Failure::Cancelled,
        5 | 30 | 31 => Failure::Untrusted,
        _ => Failure::Failed,
    }
}
fn exit_code(code: u32) -> Result<(), Failure> {
    match code {
        EXIT_SUCCESS => Ok(()),
        3 | 9 => Err(Failure::Cancelled),
        4 | 8 => Err(Failure::Untrusted),
        _ => Err(Failure::Failed),
    }
}
pub(super) async fn check_system(
    cancel: watch::Receiver<bool>,
    limit: Duration,
) -> Result<Vec<String>, Failure> {
    let mut connection_cancel = cancel.clone();
    let deadline = Instant::now() + limit;
    let connection = tokio::select! {
        biased;
        _ = cancelled(&mut connection_cancel) => return Err(Failure::Cancelled),
        connection = timeout_at(deadline.min(Instant::now()+STEP_TIMEOUT),Connection::system()) => connection.map_err(|_|Failure::Timeout)?.map_err(|_|Failure::MissingPackageKit)?,
    };
    check_on(&connection, cancel, deadline).await
}
pub(super) async fn check_on(
    connection: &Connection,
    mut cancel: watch::Receiver<bool>,
    deadline: Instant,
) -> Result<Vec<String>, Failure> {
    let (owner, _) = transaction(connection, SERVICE, true, &mut cancel, deadline).await?;
    // Always a separate, fresh query after Finished(success) for the refresh.
    let (_, packages) = transaction(connection, &owner, false, &mut cancel, deadline).await?;
    Ok(packages)
}
async fn transaction(
    connection: &Connection,
    destination: &str,
    refresh: bool,
    cancel: &mut watch::Receiver<bool>,
    deadline: Instant,
) -> Result<(String, Vec<String>), Failure> {
    let create = async {
        let reply = connection
            .call_method(
                Some(destination),
                ROOT,
                Some(SERVICE),
                "CreateTransaction",
                &(),
            )
            .await
            .map_err(bus_error)?;
        let owner = reply
            .header()
            .sender()
            .ok_or(Failure::InvalidResponse)?
            .to_string();
        let path: OwnedObjectPath = reply
            .body()
            .deserialize()
            .map_err(|_| Failure::InvalidResponse)?;
        // Installed noble XML documents /45_dafeca, outside the root object's
        // subtree. OwnedObjectPath validates syntax; the unique sender and exact
        // returned path scope the transaction, not an invented path prefix.
        if path.as_str() == ROOT || path.as_str() == "/" {
            return Err(Failure::InvalidResponse);
        }
        Ok((owner, path))
    };
    let (owner, path) = tokio::select! {
        biased;
        _ = cancelled(cancel) => return Err(Failure::Cancelled),
        value = timeout_at(deadline.min(Instant::now()+STEP_TIMEOUT),create) => value.map_err(|_|Failure::Timeout)??,
    };
    let work = async {
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(owner.as_str())
            .map_err(|_| Failure::InvalidResponse)?
            .path(path.as_str())
            .map_err(|_| Failure::InvalidResponse)?
            .interface(TRANSACTION)
            .map_err(|_| Failure::InvalidResponse)?
            .build();
        // Install the match and receiver BEFORE SetHints / the operation call.
        let mut signals = MessageStream::for_match_rule(rule, connection, Some(4096))
            .await
            .map_err(bus_error)?;
        timeout(
            STEP_TIMEOUT,
            connection.call_method(
                Some(owner.as_str()),
                path.as_str(),
                Some(TRANSACTION),
                "SetHints",
                &vec!["interactive=true", "background=false"],
            ),
        )
        .await
        .map_err(|_| Failure::Timeout)?
        .map_err(bus_error)?;
        if refresh {
            connection
                .call_method(
                    Some(owner.as_str()),
                    path.as_str(),
                    Some(TRANSACTION),
                    "RefreshCache",
                    &false,
                )
                .await
                .map_err(bus_error)?;
        } else {
            connection
                .call_method(
                    Some(owner.as_str()),
                    path.as_str(),
                    Some(TRANSACTION),
                    "GetUpdates",
                    &FILTER_NONE,
                )
                .await
                .map_err(bus_error)?;
        }
        let mut packages = Vec::new();
        while let Some(message) = signals.next().await {
            let message = message.map_err(bus_error)?;
            let header = message.header();
            match header.member().map(|v| v.as_str()) {
                Some("Package") => {
                    let (_info, id, _summary): (u32, String, String) = message
                        .body()
                        .deserialize()
                        .map_err(|_| Failure::InvalidResponse)?;
                    if id.len() > 1024 || packages.len() >= 4096 {
                        return Err(Failure::InvalidResponse);
                    }
                    if !refresh {
                        packages.push(id);
                    }
                }
                Some("ErrorCode") => {
                    let (code, _details): (u32, String) = message
                        .body()
                        .deserialize()
                        .map_err(|_| Failure::InvalidResponse)?;
                    return Err(error_code(code));
                }
                Some("Finished") => {
                    let (exit, _runtime): (u32, u32) = message
                        .body()
                        .deserialize()
                        .map_err(|_| Failure::InvalidResponse)?;
                    exit_code(exit)?;
                    return Ok(packages);
                }
                _ => {}
            }
        }
        Err(Failure::InvalidResponse)
    };
    let result = tokio::select! {
        biased;
        _ = cancelled(cancel) => Err(Failure::Cancelled),
        result = timeout_at(deadline,work) => result.unwrap_or(Err(Failure::Timeout)),
    };
    if result.is_err() {
        // Best effort only; a non-cancellable daemon never holds the UI hostage.
        let _ = timeout(Duration::from_secs(1), async {
            let proxy =
                zbus::Proxy::new(connection, owner.as_str(), path.as_str(), TRANSACTION).await?;
            if proxy.get_property::<bool>("AllowCancel").await? {
                proxy.call::<_, _, ()>("Cancel", &()).await?;
            }
            Ok::<(), zbus::Error>(())
        })
        .await;
    }
    result.map(|packages| (owner, packages))
}
pub(super) async fn candidate(
    identity: &Identity,
    packages: &[String],
) -> Result<Option<String>, Failure> {
    let installed = identity
        .package_version
        .as_ref()
        .or(identity.installed_version.as_ref())
        .ok_or(Failure::InstallationChanged)?;
    let arch = identity
        .architecture
        .as_ref()
        .ok_or(Failure::InstallationChanged)?;
    let mut main = None;
    let mut seen = [false; 2];
    for id in packages {
        let parts: Vec<_> = id.split(';').collect();
        if parts.len() != 4 || parts.iter().take(3).any(|s| s.is_empty()) {
            return Err(Failure::InvalidResponse);
        }
        if parts[2] != arch {
            continue;
        }
        let (slot, baseline, selected) = match parts[0] {
            "cosmic-ext-halcyon" => (0, installed, &mut main),
            _ => continue,
        };
        if seen[slot] || !valid_native_version(parts[1], identity.channel) {
            return Err(Failure::InvalidResponse);
        }
        seen[slot] = true;
        // The native package manager owns version ordering.
        if newer(parts[1], baseline, identity.channel).await? {
            *selected = Some(parts[1].to_owned());
        }
    }
    let candidate = main;
    Ok(candidate)
}
fn valid_native_version(value: &str, channel: super::UpdateChannel) -> bool {
    if channel == super::UpdateChannel::Dnf {
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b".+:~^_-".contains(&c))
    } else {
        super::identity::valid_version(value)
    }
}

#[cfg(test)]
mod rpm_tests {
    use super::*;
    #[test]
    fn native_version_input_cannot_contain_macro_or_lua_syntax() {
        for value in ["0:0.5.0-0.1.next.fc43", "1.0~rc1-1.fc43", "1.0^git2-1.fc43"] {
            assert!(valid_native_version(
                value,
                super::super::UpdateChannel::Dnf
            ));
        }
        for value in ["", "%{lua:print(1)}", "1');os.execute('id')", "1\n2", "1;2"] {
            assert!(!valid_native_version(
                value,
                super::super::UpdateChannel::Dnf
            ));
        }
    }
    #[tokio::test]
    #[ignore = "Requires Fedora's rpm implementation in the isolated acceptance container"]
    async fn fedora_native_ordering_handles_epoch_release_and_prerelease() {
        for (candidate, installed, expected) in [
            ("0:0.5.0-2.fc43", "0:0.5.0-1.fc43", true),
            ("1:0.4.0-1.fc43", "0:0.5.0-2.fc43", true),
            ("0:0.6.0~rc1-1.fc43", "0:0.6.0-1.fc43", false),
            ("0:0.5.0-1.fc43", "0:0.5.0-1.fc43", false),
        ] {
            assert_eq!(
                newer(candidate, installed, super::super::UpdateChannel::Dnf)
                    .await
                    .unwrap(),
                expected
            );
        }
    }
}
async fn newer(
    candidate: &str,
    installed: &str,
    channel: super::UpdateChannel,
) -> Result<bool, Failure> {
    if !valid_native_version(candidate, channel) || !valid_native_version(installed, channel) {
        return Err(Failure::InvalidResponse);
    }
    if channel == super::UpdateChannel::Dnf {
        // Values have a restricted native-version alphabet: no Lua/macro syntax.
        let expression = format!("%{{lua:print(rpm.vercmp('{candidate}', '{installed}'))}}");
        let output = timeout(
            Duration::from_secs(2),
            tokio::process::Command::new("/usr/bin/rpm")
                .args(["--eval", &expression])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| Failure::Timeout)?
        .map_err(|_| Failure::Failed)?;
        if !output.status.success() {
            return Err(Failure::Failed);
        }
        return match output.stdout.as_slice() {
            b"1\n" => Ok(true),
            b"0\n" | b"-1\n" => Ok(false),
            _ => Err(Failure::InvalidResponse),
        };
    }
    let status = timeout(
        Duration::from_secs(2),
        tokio::process::Command::new("/usr/bin/dpkg")
            .args(["--compare-versions", candidate, "gt", installed])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status(),
    )
    .await
    .map_err(|_| Failure::Timeout)?
    .map_err(|_| Failure::Failed)?;
    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(Failure::InvalidResponse),
    }
}
