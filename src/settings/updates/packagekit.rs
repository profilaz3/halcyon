//! Noble PackageKit 1.2.8 protocol. Only hash-pinned release files can be installed.
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

/// Package installation outlives a Settings window. Files are retained when a
/// lost connection/timeout leaves the daemon's outcome unknown.
#[cfg(test)]
pub(super) async fn install_on(
    connection: &Connection,
    prepared: &super::enrollment::Prepared,
) -> Result<(), Failure> {
    files_transaction(connection, prepared, false)
        .await
        .map(|_| ())
}

/// A supported COSMIC desktop already has the companion's system dependencies.
/// Keep a larger repair/upgrade transaction in the OS package manager instead of
/// hiding it behind an optional-component button.
fn islands_plan(packages: &[(u32, String)], version: &str) -> Result<(), Failure> {
    let mut installs = 0;
    for (info, id) in packages {
        if *info == 1 {
            continue;
        } // Already installed, no change.
        let fields: Vec<_> = id.split(';').collect();
        // apt's simulation also reports the pinned local file as untrusted.
        // Accept that annotation only for this exact companion; never accept an
        // unsigned repository dependency or weaken a rejected trust check.
        if !matches!(*info, 12 | 23)
            || fields.len() != 4
            || fields[0] != "cosmic-ext-halcyon-islands"
            || fields[1] != version
            || fields[2] != "amd64"
        {
            return Err(Failure::DependencyChanges);
        }
        if *info == 12 {
            installs += 1;
        }
    }
    if installs != 1 {
        return Err(Failure::DependencyChanges);
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn islands_local_trust_annotation_is_scoped_to_the_pinned_file() {
    let id = "cosmic-ext-halcyon-islands;0.3.0;amd64;local".to_owned();
    assert_eq!(
        islands_plan(&[(23, id.clone()), (12, id.clone())], "0.3.0"),
        Ok(())
    );
    assert_eq!(
        islands_plan(
            &[(23, "dependency;1;amd64;repo".into()), (12, id.clone())],
            "0.3.0"
        ),
        Err(Failure::DependencyChanges)
    );
    assert_eq!(
        islands_plan(&[(23, id)], "0.3.0"),
        Err(Failure::DependencyChanges)
    );
}

pub(super) async fn prepare_islands(prepared: &super::enrollment::Prepared) -> Result<(), Failure> {
    if super::identity::isolated() {
        return Err(Failure::InstallationChanged);
    }
    let connection = timeout(STEP_TIMEOUT, Connection::system())
        .await
        .map_err(|_| Failure::Timeout)?
        .map_err(|_| Failure::MissingPackageKit)?;
    islands_preflight(&connection, prepared).await
}

pub(super) async fn islands_preflight(
    connection: &Connection,
    prepared: &super::enrollment::Prepared,
) -> Result<(), Failure> {
    let root = zbus::Proxy::new(connection, SERVICE, ROOT, SERVICE)
        .await
        .map_err(bus_error)?;
    let active: Vec<OwnedObjectPath> = timeout(STEP_TIMEOUT, root.call("GetTransactionList", &()))
        .await
        .map_err(|_| Failure::Timeout)?
        .map_err(bus_error)?;
    if !active.is_empty() {
        return Err(Failure::Busy);
    }
    let changes = files_transaction(connection, prepared, true).await?;
    islands_plan(&changes, &super::enrollment::islands_version()?)
}

pub(super) async fn install_islands(
    prepared: std::sync::Arc<super::enrollment::Prepared>,
) -> Result<(), Failure> {
    if super::identity::isolated() {
        return Err(Failure::InstallationChanged);
    }
    let connection = timeout(STEP_TIMEOUT, Connection::system())
        .await
        .map_err(|_| Failure::Timeout)?
        .map_err(|_| Failure::MissingPackageKit)?;
    // Repeat the simulation immediately before authorizing; never turn a
    // previously displayed plan into an unrelated package upgrade/repair.
    islands_preflight(&connection, &prepared).await?;
    files_transaction(&connection, &prepared, false)
        .await
        .map(|_| ())
}

async fn files_transaction(
    connection: &Connection,
    prepared: &super::enrollment::Prepared,
    simulate: bool,
) -> Result<Vec<(u32, String)>, Failure> {
    prepared.verify()?;
    let work = async {
        let root = zbus::Proxy::new(connection, SERVICE, ROOT, SERVICE)
            .await
            .map_err(bus_error)?;
        // pk_role_enum_from_string("install-files") on the supported Noble library.
        let roles: u64 = root.get_property("Roles").await.map_err(bus_error)?;
        if roles & (1 << 10) == 0 {
            return Err(Failure::MissingPackageKit);
        }
        let reply = connection
            .call_method(Some(SERVICE), ROOT, Some(SERVICE), "CreateTransaction", &())
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
        if path.as_str() == "/" || path.as_str() == ROOT {
            return Err(Failure::InvalidResponse);
        }
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(owner.as_str())
            .map_err(|_| Failure::InvalidResponse)?
            .path(path.as_str())
            .map_err(|_| Failure::InvalidResponse)?
            .interface(TRANSACTION)
            .map_err(|_| Failure::InvalidResponse)?
            .build();
        let mut signals = MessageStream::for_match_rule(rule, connection, Some(128))
            .await
            .map_err(bus_error)?;
        let proxy = zbus::Proxy::new(connection, owner.as_str(), path.as_str(), TRANSACTION)
            .await
            .map_err(bus_error)?;
        proxy
            .call::<_, _, ()>("SetHints", &vec!["interactive=true", "background=false"])
            .await
            .map_err(bus_error)?;
        prepared.verify()?;
        if !simulate {
            prepared
                .handed_off
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let paths: Vec<_> = prepared
            .paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        // These exact hash-pinned local .debs are not repository-signed packages.
        // This is the OS's explicit localinstall authorization, never a retry
        // that weakens a rejected repository trust check.
        proxy
            .call::<_, _, ()>(
                "InstallFiles",
                &(if simulate { 1u64 << 2 } else { 0u64 }, paths),
            )
            .await
            .map_err(bus_error)?;
        let mut failure = None;
        let mut packages = Vec::new();
        while let Some(message) = signals.next().await {
            let message = message.map_err(bus_error)?;
            match message.header().member().map(|v| v.as_str()) {
                Some("Package") => {
                    let (info, id, _): (u32, String, String) = message
                        .body()
                        .deserialize()
                        .map_err(|_| Failure::InvalidResponse)?;
                    if id.len() > 1024 || packages.len() >= 128 {
                        return Err(Failure::InvalidResponse);
                    }
                    packages.push((info, id));
                }
                Some("ErrorCode") => {
                    let (code, _): (u32, String) = message
                        .body()
                        .deserialize()
                        .map_err(|_| Failure::InvalidResponse)?;
                    failure = Some(error_code(code));
                }
                Some("Finished") => {
                    let (exit, _): (u32, u32) = message
                        .body()
                        .deserialize()
                        .map_err(|_| Failure::InvalidResponse)?;
                    if !simulate {
                        prepared
                            .handed_off
                            .store(false, std::sync::atomic::Ordering::Relaxed);
                    }
                    failure.map_or_else(|| exit_code(exit), Err)?;
                    return Ok(packages);
                }
                _ => {}
            }
        }
        Err(Failure::Failed)
    };
    timeout(
        Duration::from_secs(if simulate { 60 } else { 15 * 60 }),
        work,
    )
    .await
    .unwrap_or(Err(Failure::Timeout))
}

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
        .installed_version
        .as_ref()
        .ok_or(Failure::InstallationChanged)?;
    let arch = identity
        .architecture
        .as_ref()
        .ok_or(Failure::InstallationChanged)?;
    let mut main = None;
    let mut islands = None;
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
            "cosmic-ext-halcyon-islands" if identity.islands_version.is_some() => {
                (1, identity.islands_version.as_ref().unwrap(), &mut islands)
            }
            _ => continue,
        };
        if seen[slot] || !super::identity::valid_version(parts[1]) {
            return Err(Failure::InvalidResponse);
        }
        seen[slot] = true;
        // dpkg owns Debian version ordering; candidates are never installed here.
        if newer(parts[1], baseline).await? {
            *selected = Some(parts[1].to_owned());
        }
    }
    if let Some(installed_islands) = &identity.islands_version {
        let resulting_main = main.as_ref().unwrap_or(installed);
        let resulting_islands = islands.as_ref().unwrap_or(installed_islands);
        if resulting_main != resulting_islands {
            return Err(Failure::IslandsMismatch);
        }
    }
    let candidate = main.or(islands);
    Ok(candidate)
}
async fn newer(candidate: &str, installed: &str) -> Result<bool, Failure> {
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
