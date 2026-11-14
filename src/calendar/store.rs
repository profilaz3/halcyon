use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use std::os::{
    fd::AsRawFd,
    unix::fs::{OpenOptionsExt, PermissionsExt},
};
use std::path::{Path, PathBuf};

pub(super) fn preview() -> bool {
    std::env::var_os("DESKTOP_CARDS_CONFIG").is_some()
}
fn directory() -> Option<PathBuf> {
    crate::config::Config::path()?
        .parent()
        .map(Path::to_path_buf)
}
/// Keyring IDs of credentials that should be gone but couldn't be removed
/// (the keyring was busy or locked). Only IDs, never secrets.
fn stale_path() -> Option<PathBuf> {
    Some(directory()?.join("calendar-stale-credentials.json"))
}
pub(super) fn stale_credentials() -> Result<Vec<String>, Problem> {
    let path = stale_path().ok_or(Problem::Storage)?;
    match crate::tools::read_bounded(&path, 64 * 1024).map_err(|_| Problem::Storage)? {
        Some(bytes) => serde_json::from_slice(&bytes).map_err(|_| Problem::Storage),
        None => Ok(vec![]),
    }
}
fn update_stale_locked(add: &[String], remove: &[String]) -> Result<(), Problem> {
    let mut ids = stale_credentials()?;
    ids.retain(|id| !remove.contains(id));
    for id in add {
        if !id.is_empty() && !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    let bytes = serde_json::to_vec(&ids).map_err(|_| Problem::Storage)?;
    if bytes.len() > 64 * 1024 {
        return Err(Problem::Storage);
    }
    atomic(&stale_path().ok_or(Problem::Storage)?, &bytes)
}
pub(super) fn update_stale(add: &[String], remove: &[String]) -> Result<(), Problem> {
    let _lock = lock()?;
    update_stale_locked(add, remove)
}
// Keep a preparing credential out of cleanup in every process. The ID is
// journaled before its first keyring call; after a crash the lock is released.
pub(super) struct CredentialLease {
    _lock: Lock,
}
pub(super) fn keyring_lease() -> Result<CredentialLease, Problem> {
    let path = directory()
        .ok_or(Problem::Storage)?
        .join("calendar-keyring.lock");
    Ok(CredentialLease {
        _lock: lock_path(&path)?,
    })
}
pub(super) fn active_credential() -> Result<Option<String>, Problem> {
    let bytes = crate::tools::read_bounded(&account_path().ok_or(Problem::Storage)?, 64 * 1024)
        .map_err(|_| Problem::Storage)?;
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    let connection: Connection = serde_json::from_slice(&bytes).map_err(|_| Problem::Storage)?;
    Ok(connection.connected().then_some(connection.credential))
}
pub(super) async fn wait_keyring_lease() -> Result<CredentialLease, Problem> {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            match keyring_lease() {
                Err(Problem::Changed) => tokio::time::sleep(Duration::from_millis(20)).await,
                result => return result,
            }
        }
    })
    .await
    .map_err(|_| Problem::TimedOut)?
}
pub(super) async fn prepare_credential(id: &str) -> Result<CredentialLease, Problem> {
    let lease = wait_keyring_lease().await?;
    update_stale(&[id.to_string()], &[])?;
    Ok(lease)
}
pub fn account_path() -> Option<PathBuf> {
    Some(directory()?.join("calendar-account.json"))
}
pub fn client_path() -> Option<PathBuf> {
    Some(directory()?.join("google-calendar-client.json"))
}
fn cache_directory() -> Option<PathBuf> {
    if preview() {
        Some(directory()?.join("calendar-cache"))
    } else {
        crate::paths::cache_dir()
    }
}
pub fn cache_path(month: &str) -> Option<PathBuf> {
    valid_month(month).then_some(())?;
    Some(cache_directory()?.join(format!("calendar-{month}.json")))
}
pub(super) fn valid_month(month: &str) -> bool {
    month.len() == 7
        && chrono::NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d").is_ok()
}
pub(super) fn nonce() -> Result<String, Problem> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| Problem::Storage)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn private_parent(path: &Path) -> Result<(), Problem> {
    let parent = path.parent().ok_or(Problem::Storage)?;
    if !parent.exists() {
        std::fs::create_dir_all(parent).map_err(|_| Problem::Storage)?;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| Problem::Storage)?;
    }
    Ok(())
}
#[cfg(test)]
thread_local! {
    /// Files this thread wrote through `atomic`.
    pub(super) static WRITES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
pub(super) fn atomic(path: &Path, bytes: &[u8]) -> Result<(), Problem> {
    use std::io::Write;
    #[cfg(test)]
    WRITES.with(|count| count.set(count.get() + 1));
    private_parent(path)?;
    let temp = path.with_file_name(format!(".calendar-{}.tmp", nonce()?));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
            .map_err(|_| Problem::Storage)?;
        file.write_all(bytes).map_err(|_| Problem::Storage)?;
        file.sync_all().map_err(|_| Problem::Storage)?;
        std::fs::rename(&temp, path).map_err(|_| Problem::Storage)
    })();
    let _ = std::fs::remove_file(&temp);
    result
}
/// Writes `bytes` unless `path` already holds exactly them.
pub(super) fn atomic_if_changed(path: &Path, bytes: &[u8]) -> Result<(), Problem> {
    let same = std::fs::metadata(path).is_ok_and(|meta| meta.len() == bytes.len() as u64)
        && std::fs::read(path).is_ok_and(|current| current == bytes);
    if same { Ok(()) } else { atomic(path, bytes) }
}
struct Lock(std::fs::File);
impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
fn lock() -> Result<Lock, Problem> {
    let path = account_path()
        .ok_or(Problem::Storage)?
        .with_extension("lock");
    lock_path(&path)
}
fn lock_path(path: &Path) -> Result<Lock, Problem> {
    private_parent(&path)?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| Problem::Storage)?;
    // Only short, local atomic writes hold this lock. Never block the UI on a
    // second process: ask it to retry instead.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(Problem::Changed);
    }
    Ok(Lock(file))
}
pub fn load_connection() -> Connection {
    let Some(path) = account_path() else {
        return Connection::default();
    };
    if path.exists() {
        return std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
    }
    // Upgrade only: both the old helper and this app's prior cache must exist.
    // A new installation never silently consumes another app's credentials.
    if !preview() {
        if let Some(home) = dirs::home_dir() {
            if home
                .join(".local/share/workspace-mcp/venv/bin/python")
                .is_file()
            {
                if let Some(dir) = cache_directory() {
                    let mut paths = std::fs::read_dir(dir)
                        .into_iter()
                        .flatten()
                        .filter_map(Result::ok)
                        .map(|entry| entry.path())
                        .filter(|path| {
                            path.file_name()
                                .and_then(|v| v.to_str())
                                .is_some_and(|v| v.starts_with("calendar-") && v.ends_with(".json"))
                        })
                        .collect::<Vec<_>>();
                    paths.sort();
                    for path in paths.into_iter().rev().take(24) {
                        let Some(data) = std::fs::read(path).ok().and_then(|bytes| {
                            serde_json::from_slice::<crate::data::CalendarData>(&bytes).ok()
                        }) else {
                            continue;
                        };
                        if data.connection_revision.is_empty() && !data.account.is_empty() {
                            return Connection {
                                backend: Backend::Legacy,
                                account: data.account.clone(),
                                calendar_id: data.account.clone(),
                                calendar_name: "Primary calendar".into(),
                                revision: format!("legacy:{}", data.account),
                                credential: String::new(),
                                ..Default::default()
                            };
                        }
                    }
                }
            }
        }
    }
    Connection::default()
}
fn persist(connection: &Connection) -> Result<(), Problem> {
    atomic(
        &account_path().ok_or(Problem::Storage)?,
        &serde_json::to_vec(connection).map_err(|_| Problem::Storage)?,
    )
}
pub(super) fn replace(expected: &Connection, connection: &Connection) -> Result<(), Problem> {
    let _lock = lock()?;
    if load_connection() != *expected {
        return Err(Problem::Changed);
    }
    if expected.credential != connection.credential
        && !expected.credential.is_empty()
        && (expected.connected() || !connection.credential.is_empty())
    {
        // If persistence fails, cleanup still checks the active connection and
        // must retain its credential. No asynchronous gap exists here.
        update_stale_locked(&[expected.credential.clone()], &[])?;
    }
    persist(connection)?;
    clear_cache();
    Ok(())
}
pub(super) fn disconnect(expected: &Connection) -> Result<Connection, Problem> {
    let disconnected = Connection {
        revision: nonce()?,
        credential: if matches!(expected.backend, Backend::Google | Backend::Subscription)
            || expected.cleanup_pending()
        {
            expected.credential.clone()
        } else {
            String::new()
        },
        ..Default::default()
    };
    replace(expected, &disconnected)?;
    Ok(disconnected)
}
pub fn save_selections(
    expected: &Connection,
    choices: Vec<super::CalendarChoice>,
) -> Result<Connection, Problem> {
    if choices.is_empty()
        || choices.len() > 32
        || choices.iter().any(|c| c.id.is_empty())
        || choices
            .iter()
            .map(|c| &c.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != choices.len()
    {
        return Err(Problem::InvalidResponse);
    }
    let connection = Connection {
        calendar_id: choices[0].id.clone(),
        calendar_name: choices[0].name.clone(),
        selections: choices,
        revision: nonce()?,
        ..expected.clone()
    };
    replace(expected, &connection)?;
    Ok(connection)
}
/// Refresh display metadata without changing selected IDs or their order.
/// replace() fences provider responses against concurrent selection/account edits.
pub fn refresh_metadata(
    expected: &Connection,
    available: &[CalendarChoice],
) -> Result<Connection, Problem> {
    if expected.backend != Backend::Google {
        return Ok(expected.clone());
    }
    let selected = expected.selected_calendars();
    let refreshed: Vec<_> = selected
        .iter()
        .map(|old| {
            available
                .iter()
                .find(|new| new.id == old.id)
                .unwrap_or(old)
                .clone()
        })
        .collect();
    if refreshed == selected {
        return Ok(expected.clone());
    }
    save_selections(expected, refreshed)
}
fn clear_cache() {
    if let Some(dir) = cache_directory() {
        for path in std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
        {
            if path.file_name().and_then(|v| v.to_str()).is_some_and(|v| {
                v.strip_prefix("calendar-")
                    .and_then(|v| v.strip_suffix(".json"))
                    .is_some_and(valid_month)
            }) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}
pub(super) fn save_cache(
    connection: &Connection,
    month: &str,
    data: &crate::data::CalendarData,
) -> Result<(), Problem> {
    let _lock = lock()?;
    if load_connection() != *connection {
        return Err(Problem::Changed);
    }
    // Persist legacy metadata once, before the upgraded cache replaces the
    // old record used to recognize this existing installation. Later saves
    // find it already written.
    if connection.backend == Backend::Legacy {
        atomic_if_changed(
            &account_path().ok_or(Problem::Storage)?,
            &serde_json::to_vec(connection).map_err(|_| Problem::Storage)?,
        )?;
    }
    let bytes = serde_json::to_vec(data).map_err(|_| Problem::Storage)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(Problem::FeedLimit);
    }
    let path = cache_path(month).ok_or(Problem::InvalidResponse)?;
    if cache_current(&path, data) {
        return Ok(());
    }
    atomic(&path, &bytes)
}
/// How old the saved fetch time may grow while refreshes keep finding the
/// same events. The card shows it ("Saved · 14:05") only after a failed
/// refresh, a restart or a change of month.
const CACHE_STAMP: chrono::TimeDelta = chrono::TimeDelta::hours(1);
/// Whether `path` already holds `data` but for a fetch time less than
/// `CACHE_STAMP` older. Every fetch stamps the current time, so comparing
/// bytes alone would rewrite and sync the cache on every refresh.
fn cache_current(path: &Path, data: &crate::data::CalendarData) -> bool {
    let Ok(Some(bytes)) = crate::tools::read_bounded(path, 16 * 1024 * 1024) else {
        return false;
    };
    let Ok(mut stored) = serde_json::from_slice::<crate::data::CalendarData>(&bytes) else {
        return false;
    };
    let time = |value: &str| chrono::DateTime::parse_from_rfc3339(value).ok();
    let fresh = time(&stored.updated_at)
        .zip(time(&data.updated_at))
        .is_some_and(|(stored, now)| stored <= now && now - stored < CACHE_STAMP);
    stored.updated_at.clone_from(&data.updated_at);
    fresh && serde_json::to_vec(&stored).ok() == serde_json::to_vec(data).ok()
}
pub fn cached(connection: &Connection, month: &str) -> Option<crate::data::CalendarData> {
    if !connection.connected() {
        return None;
    }
    let data: crate::data::CalendarData = serde_json::from_slice(
        &crate::tools::read_bounded(&cache_path(month)?, 16 * 1024 * 1024).ok()??,
    )
    .ok()?;
    let identity = data.connection_revision == connection.revision
        && data.calendar_id == connection.calendar_id;
    let legacy = connection.backend == Backend::Legacy && data.connection_revision.is_empty();
    (data.month == month && data.account == connection.account && (identity || legacy))
        .then_some(data)
}
pub(super) fn parse_client(bytes: &[u8]) -> Result<GoogleClient, Problem> {
    if bytes.len() > 64 * 1024 {
        return Err(Problem::InvalidClient);
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| Problem::InvalidClient)?;
    let installed = value.get("installed").ok_or(Problem::InvalidClient)?;
    let client: GoogleClient =
        serde_json::from_value(installed.clone()).map_err(|_| Problem::InvalidClient)?;
    if !client.client_id.ends_with(".apps.googleusercontent.com")
        || client.client_id.len() > 512
        || client.client_id.chars().any(char::is_whitespace)
        || client.client_secret.is_empty()
        || client.client_secret.len() > 512
    {
        return Err(Problem::InvalidClient);
    }
    Ok(client)
}
pub fn load_client() -> Result<GoogleClient, Problem> {
    let own = client_path().ok_or(Problem::MissingClient)?;
    if own.exists() {
        return parse_client(&std::fs::read(own).map_err(|_| Problem::InvalidClient)?);
    }
    // The publisher's sign-in client, installed with Halcyon.
    let bundled =
        crate::paths::shared("google-calendar-client.json").ok_or(Problem::MissingClient)?;
    if bundled.exists() && !preview() {
        return parse_client(&std::fs::read(bundled).map_err(|_| Problem::InvalidClient)?);
    }
    Err(Problem::MissingClient)
}
pub fn import_client(path: &Path) -> Result<(), Problem> {
    if preview() {
        return Err(Problem::Preview);
    }
    let metadata = std::fs::metadata(path).map_err(|_| Problem::InvalidClient)?;
    if !metadata.is_file() || metadata.len() > 64 * 1024 {
        return Err(Problem::InvalidClient);
    }
    let bytes = std::fs::read(path).map_err(|_| Problem::InvalidClient)?;
    let client = parse_client(&bytes)?;
    atomic(
        &client_path().ok_or(Problem::Storage)?,
        &serde_json::to_vec(&serde_json::json!({"installed":client}))
            .map_err(|_| Problem::Storage)?,
    )
}
