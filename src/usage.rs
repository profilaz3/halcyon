//! Pseudonymous daily activity and explicit reporting choices. No content or hardware identity.
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
mod scheduling;

const RETRY_SECONDS: i64 = 6 * 60 * 60;
const STATUS_LIFETIME: i64 = 7 * 86400;
const MAX_ATTEMPTS: u8 = 28;
const MAX_REVISION: u64 = 9_007_199_254_740_991;
const WARMUP: Duration = Duration::from_secs(300);
static WORK_READY: std::sync::OnceLock<std::sync::Arc<tokio::sync::Notify>> =
    std::sync::OnceLock::new();
const COUNTRY_ENDPOINT: &str = "https://api.country.is/";
const ENDPOINT: &str = include_str!("../assets/usage-endpoint.txt");
// A click cancels this process's activity before its asynchronous disk write.
static PAUSED: AtomicBool = AtomicBool::new(false);
fn yes() -> bool {
    true
}
fn state_version() -> u8 {
    3
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    created_at: i64,
    retry_after: i64,
    attempts: u8,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    installation: String,
    last_success: Option<String>,
    retry_after: i64,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    first_started_at: Option<i64>,
    #[serde(default = "state_version")]
    state_version: u8,
    #[serde(default = "yes")]
    enabled: bool,
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    changed_at: Option<i64>,
    #[serde(default)]
    opted_out_at: Option<i64>,
    #[serde(default)]
    pending: Option<Pending>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            installation: String::new(),
            last_success: None,
            retry_after: 0,
            country: None,
            first_started_at: None,
            state_version: 3,
            enabled: true,
            revision: 0,
            changed_at: None,
            opted_out_at: None,
            pending: None,
        }
    }
}
#[derive(Serialize)]
struct Report<'a> {
    schema: u8,
    kind: &'a str,
    installation: &'a str,
    revision: u64,
    version: &'a str,
    country: Option<&'a str>,
    first_started_at: Option<i64>,
    #[serde(flatten)]
    installation_info: crate::installation::Observation,
}
#[derive(Serialize)]
struct StatusReport<'a> {
    schema: u8,
    kind: &'a str,
    installation: &'a str,
    revision: u64,
    enabled: bool,
    changed_at: i64,
}
#[derive(Deserialize)]
struct Acknowledgement {
    ok: bool,
    day: Option<String>,
    revision: Option<u64>,
    enabled: Option<bool>,
}
fn valid_time(t: i64) -> bool {
    (0..=253402300799).contains(&t)
}
// Record only a new identity's first production desktop start. Existing state
// cannot reveal the original install/start time; never infer it from file mtime.
// No network or additional activity is caused by this local record.
fn record_start(path: &Path, now: i64) -> Result<(), String> {
    let _lock = lock(path)?;
    let mut state = load(path)?;
    if !state.enabled || !state.installation.is_empty() || PAUSED.load(Ordering::SeqCst) {
        return Ok(());
    }
    if !valid_time(now) {
        return Err("Reporting clock is invalid".into());
    }
    state.installation = token()?;
    state.first_started_at = Some(now);
    save(path, &state)
}
pub fn path() -> Option<PathBuf> {
    crate::paths::state_dir().map(|p| p.join("usage.json"))
}
pub fn preference() -> Result<bool, String> {
    load(&path().ok_or("Reporting state directory unavailable")?).map(|s| s.enabled)
}
pub fn pause() {
    PAUSED.store(true, Ordering::SeqCst);
}
/// Only an explicit checkbox action changes this file. Config imports and Undo
/// cannot restore a prior consent value. The nonblocking lock serializes processes.
pub fn set_preference(enabled: bool) -> Result<bool, String> {
    let result = change(
        &path().ok_or("Reporting state directory unavailable")?,
        enabled,
        Utc::now().timestamp(),
    );
    if result.is_ok() {
        PAUSED.store(false, Ordering::SeqCst);
        work_ready().notify_one();
    }
    result.map(|_| enabled)
}
fn change(path: &Path, enabled: bool, now: i64) -> Result<(), String> {
    let _lock = lock(path)?;
    let mut state = load(path)?;
    if state.enabled == enabled {
        return Ok(());
    }
    if !valid_time(now) || state.revision >= MAX_REVISION {
        return Err("Reporting clock or revision is invalid".into());
    }
    if state.installation.is_empty() {
        state.installation = token()?;
    }
    state.enabled = enabled;
    state.revision += 1;
    state.changed_at = Some(now);
    if !enabled {
        state.opted_out_at = Some(now);
    }
    state.pending = Some(Pending {
        created_at: now,
        retry_after: 0,
        attempts: 0,
    });
    // Never backfill. A newer explicit enable may report today's activity after
    // its status is acknowledged, subject to the normal startup warmup.
    state.retry_after = 0;
    save(path, &state)
}
fn token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| "Could not create reporting identity")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn valid_token(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
struct Lock(std::fs::File);
impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
fn lock(path: &Path) -> Result<Lock, String> {
    let parent = path.parent().ok_or("Missing state directory")?;
    if !parent.exists() {
        std::fs::create_dir_all(parent).map_err(|_| "Could not create state directory")?;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "Could not protect state directory")?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path.with_extension("lock"))
        .map_err(|_| "Could not lock activity state")?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("Activity state is busy".into());
    }
    Ok(Lock(file))
}
fn load(path: &Path) -> Result<State, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(State::default()),
        Err(_) => return Err("Could not read activity state".into()),
    };
    let mut bytes = Vec::new();
    file.take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read activity state")?;
    if bytes.len() > 4096 {
        return Err("Invalid activity state".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid activity state")?;
    // Only the complete old format may acquire defaults. A partial new preference
    // must never fall back to enabled (including after a damaged write/upgrade).
    let modern = value.get("state_version").is_some();
    if modern {
        for key in [
            "enabled",
            "revision",
            "changed_at",
            "opted_out_at",
            "pending",
        ] {
            if value.get(key).is_none() {
                return Err("Incomplete reporting preference".into());
            }
        }
    } else if [
        "enabled",
        "revision",
        "changed_at",
        "opted_out_at",
        "pending",
    ]
    .iter()
    .any(|k| value.get(k).is_some())
    {
        return Err("Incomplete reporting preference".into());
    }
    let state: State = serde_json::from_value(value).map_err(|_| "Invalid activity state")?;
    if state.state_version != 3
        || state.revision > MAX_REVISION
        || (state.revision == 0
            && (!state.enabled || state.changed_at.is_some() || state.pending.is_some()))
        || (state.revision > 0 && state.changed_at.is_none())
        || (!state.enabled && state.opted_out_at != state.changed_at)
        || state.changed_at.is_some_and(|t| !valid_time(t))
        || state.opted_out_at.is_some_and(|t| !valid_time(t))
        || state.first_started_at.is_some_and(|t| !valid_time(t))
        || state
            .pending
            .as_ref()
            .is_some_and(|p| p.attempts > MAX_ATTEMPTS || !valid_time(p.created_at))
    {
        return Err("Invalid reporting preference".into());
    }
    if !valid_token(&state.installation)
        || state.country.as_deref().is_some_and(|c| !valid_country(c))
    {
        return Err("Invalid activity identity".into());
    }
    Ok(state)
}
fn save(path: &Path, state: &State) -> Result<(), String> {
    let tmp = path.with_extension(format!("{}.tmp", token()?));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|_| "Could not save activity state")?;
        file.write_all(&serde_json::to_vec(state).map_err(|_| "Invalid activity state")?)
            .map_err(|_| "Could not write activity state")?;
        file.sync_all()
            .map_err(|_| "Could not save activity state")?;
        std::fs::rename(&tmp, path).map_err(|_| "Could not replace activity state")?;
        std::fs::File::open(path.parent().ok_or("Missing state directory")?)
            .and_then(|dir| dir.sync_all())
            .map_err(|_| "Could not sync activity state")
    })();
    let _ = std::fs::remove_file(tmp);
    result.map_err(String::from)
}
fn claim(path: &Path, day: &str, now: i64) -> Result<Option<(String, u64)>, String> {
    let _lock = lock(path)?;
    let mut state = load(path)?;
    if PAUSED.load(Ordering::SeqCst)
        || !state.enabled
        || state.pending.is_some()
        || state.last_success.as_deref() == Some(day)
        || now < state.retry_after
    {
        return Ok(None);
    }
    if state.installation.is_empty() {
        state.installation = token()?;
    }
    state.retry_after = now.saturating_add(RETRY_SECONDS);
    save(path, &state)?;
    Ok(Some((state.installation, state.revision)))
}
fn complete(path: &Path, installation: &str, revision: u64, day: &str) -> Result<(), String> {
    let _lock = lock(path)?;
    let mut state = load(path)?;
    if !matches_choice(&state, installation, revision, true) || PAUSED.load(Ordering::SeqCst) {
        return Ok(());
    }
    state.last_success = Some(day.into());
    state.retry_after = 0;
    save(path, &state)
}
fn matches_choice(s: &State, id: &str, revision: u64, enabled: bool) -> bool {
    s.installation == id && s.revision == revision && s.enabled == enabled
}
fn current(path: &Path, id: &str, revision: u64, enabled: bool, activity: bool) -> bool {
    (!activity || !PAUSED.load(Ordering::SeqCst))
        && load(path).is_ok_and(|s| matches_choice(&s, id, revision, enabled))
}
// Dropping reqwest's future cancels outstanding socket/body work where possible.
// Poll disk as well as the process flag: a separate Settings process can opt out.
// No file lock spans an await. Already transmitted requests cannot be unsent.
async fn guarded<T>(
    path: &Path,
    id: &str,
    revision: u64,
    enabled: bool,
    activity: bool,
    work: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    if !current(path, id, revision, enabled, activity) {
        return Err("Reporting choice changed".into());
    }
    tokio::pin!(work);
    loop {
        tokio::select! {
            biased;
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                if !current(path, id, revision, enabled, activity) { return Err("Reporting choice changed".into()); }
            }
            result = &mut work => {
                if !current(path, id, revision, enabled, activity) { return Err("Reporting choice changed".into()); }
                return result;
            }
        }
    }
}
fn valid_country(value: &str) -> bool {
    value.len() == 2 && value.bytes().all(|b| b.is_ascii_uppercase())
}
fn remember_country(
    path: &Path,
    installation: &str,
    revision: u64,
    country: &str,
) -> Result<Option<String>, String> {
    if !valid_country(country) {
        return Ok(None);
    }
    let _lock = lock(path)?;
    let mut state = load(path)?;
    if !matches_choice(&state, installation, revision, true) || PAUSED.load(Ordering::SeqCst) {
        return Ok(None);
    }
    if state.country.is_none() {
        state.country = Some(country.into());
        save(path, &state)?;
    }
    Ok(state.country)
}
async fn country_for(path: &Path, installation: &str, revision: u64, url: &str) -> Option<String> {
    if !current(path, installation, revision, true, true) {
        return None;
    }
    if let Some(country) = load(path).ok()?.country {
        return Some(country);
    }
    let country = guarded(path, installation, revision, true, true, async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "Country unavailable")?;
        let mut response = client
            .get(url)
            .send()
            .await
            .map_err(|_| "Country unavailable")?;
        if response.status() != reqwest::StatusCode::OK {
            return Err("Country unavailable".into());
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| "Country unavailable")? {
            if body.len() + chunk.len() > 1024 {
                return Err("Invalid country".into());
            }
            body.extend_from_slice(&chunk);
        }
        #[derive(Deserialize)]
        struct Country {
            country: String,
        }
        let country: Country = serde_json::from_slice(&body).map_err(|_| "Invalid country")?;
        Ok(country.country)
    })
    .await
    .ok()?;
    remember_country(path, installation, revision, &country)
        .ok()
        .flatten()
}
async fn report_once(
    path: &Path,
    client: &reqwest::Client,
    url: reqwest::Url,
    installation: &str,
    revision: u64,
    country_url: &str,
) -> Result<String, String> {
    let country = country_for(path, installation, revision, country_url).await;
    let first_started_at = load(path)?.first_started_at;
    guarded(
        path,
        installation,
        revision,
        true,
        true,
        send(
            client,
            url,
            installation,
            revision,
            country.as_deref(),
            first_started_at,
        ),
    )
    .await
}
fn endpoint(value: &str) -> Option<reqwest::Url> {
    let url = reqwest::Url::parse(value.trim()).ok()?;
    (url.scheme() == "https"
        && url.host_str() == Some("script.google.com")
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path().starts_with("/macros/s/")
        && url.path().ends_with("/exec")
        && url.path().split('/').count() == 5
        && url.path().split('/').nth(3).is_some_and(|id| {
            !id.is_empty()
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        }))
    .then_some(url)
}
fn client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            // Apps Script serves the response from a temporary Google URL.
            if attempt.previous().len() < 4
                && attempt.url().scheme() == "https"
                && matches!(
                    attempt.url().host_str(),
                    Some("script.google.com" | "script.googleusercontent.com")
                )
            {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
}
async fn post(
    client: &reqwest::Client,
    url: reqwest::Url,
    report: &impl Serialize,
) -> Result<Acknowledgement, String> {
    let mut response = client
        .post(url)
        .json(report)
        .send()
        .await
        .map_err(|_| "Report unavailable")?;
    if response.status() != reqwest::StatusCode::OK {
        return Err("Report not accepted".into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Incomplete acknowledgement")?
    {
        if body.len() + chunk.len() > 1024 {
            return Err("Invalid acknowledgement".into());
        }
        body.extend_from_slice(&chunk);
    }
    let ack: Acknowledgement =
        serde_json::from_slice(&body).map_err(|_| "Invalid acknowledgement")?;
    if !ack.ok {
        return Err("Report not accepted".into());
    }
    Ok(ack)
}
async fn send(
    client: &reqwest::Client,
    url: reqwest::Url,
    installation: &str,
    revision: u64,
    country: Option<&str>,
    first_started_at: Option<i64>,
) -> Result<String, String> {
    let ack = post(
        client,
        url,
        &Report {
            schema: 5,
            kind: "activity",
            installation,
            revision,
            version: env!("CARGO_PKG_VERSION"),
            country,
            first_started_at,
            installation_info: crate::installation::reporting_observation().await,
        },
    )
    .await?;
    ack.day
        .filter(|d| d.len() == 10 && chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok())
        .ok_or("Invalid acknowledgement".into())
}
fn claim_status(path: &Path, now: i64) -> Result<Option<State>, String> {
    let _lock = lock(path)?;
    let mut state = load(path)?;
    let Some(p) = state.pending.as_mut() else {
        return Ok(None);
    };
    // Also abandon delivery if the wall clock moves backwards: do not extend a
    // queue indefinitely. Attempts provide a second bound for repeated restarts.
    if now < p.created_at || now - p.created_at >= STATUS_LIFETIME || p.attempts >= MAX_ATTEMPTS {
        state.pending = None;
        save(path, &state)?;
        return Ok(None);
    }
    if now < p.retry_after {
        return Ok(None);
    }
    p.retry_after = now.saturating_add(RETRY_SECONDS);
    p.attempts += 1;
    save(path, &state)?;
    Ok(Some(state))
}
async fn status_once(
    path: &Path,
    client: &reqwest::Client,
    url: reqwest::Url,
    state: &State,
) -> Result<(), String> {
    let ack = guarded(
        path,
        &state.installation,
        state.revision,
        state.enabled,
        false,
        post(
            client,
            url,
            &StatusReport {
                schema: 3,
                kind: "status",
                installation: &state.installation,
                revision: state.revision,
                enabled: state.enabled,
                changed_at: state.changed_at.ok_or("Missing choice time")?,
            },
        ),
    )
    .await?;
    if ack.revision != Some(state.revision) || ack.enabled != Some(state.enabled) {
        return Err("Invalid status acknowledgement".into());
    }
    let _lock = lock(path)?;
    let mut latest = load(path)?;
    if matches_choice(&latest, &state.installation, state.revision, state.enabled) {
        latest.pending = None;
        save(path, &latest)?;
    }
    Ok(())
}
/// Only production desktop processes report; previews and test builds never do.
pub fn available() -> bool {
    !cfg!(debug_assertions)
        && !cfg!(test)
        && endpoint(ENDPOINT).is_some()
        && ![
            "DESKTOP_CARDS_CONFIG",
            "DESKTOP_CARDS_DESKTOP_ROOT",
            "DESKTOP_CARDS_CAPTURE_DIR",
            "DESKTOP_CARDS_SETTINGS_CAPTURE_DIR",
        ]
        .iter()
        .any(|name| std::env::var_os(name).is_some())
}
pub fn start(path: Option<PathBuf>) {
    start_worker(path, true);
}
/// Settings may deliver a choice while the desktop is stopped; never activity.
pub fn start_status() {
    start_worker(path(), false);
}
fn start_worker(path: Option<PathBuf>, activity: bool) {
    if !available() {
        return;
    }
    if let Some(path) = path {
        let _ = std::thread::Builder::new()
            .name("halcyon-activity".into())
            .spawn(move || {
                if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    runtime.block_on(run(path, activity));
                }
            });
    }
}
async fn cycle(
    path: &Path,
    client: &reqwest::Client,
    url: reqwest::Url,
    country_url: &str,
    activity: bool,
    now: i64,
) {
    if let Ok(Some(state)) = claim_status(path, now) {
        let _ = status_once(path, client, url.clone(), &state).await;
    }
    if activity {
        let Some(date) = chrono::DateTime::from_timestamp(now, 0) else {
            return;
        };
        let day = date.format("%Y-%m-%d").to_string();
        if let Ok(Some((id, revision))) = claim(path, &day, now) {
            if report_once(path, client, url, &id, revision, country_url)
                .await
                .is_ok()
            {
                let _ = complete(path, &id, revision, &day);
            }
        }
    }
}
async fn run(path: PathBuf, activity: bool) {
    if !available() {
        return;
    }
    let Some(url) = endpoint(ENDPOINT) else {
        return;
    };
    let Ok(client) = client() else {
        return;
    };
    let started = std::time::Instant::now();
    if activity {
        let _ = record_start(&path, Utc::now().timestamp());
    }
    let mut watch = scheduling::StateWatch::new(path.clone(), work_ready().clone());
    loop {
        // Install/rebind before reading, so changes between reading and waiting
        // leave a stored notification rather than disappearing into a race.
        let watch_ok = watch.refresh();
        cycle(
            &path,
            &client,
            url.clone(),
            COUNTRY_ENDPOINT,
            activity && started.elapsed() >= WARMUP,
            Utc::now().timestamp(),
        )
        .await;
        let deadline = scheduling::next(
            &path,
            activity,
            WARMUP.saturating_sub(started.elapsed()),
            watch_ok,
        );
        scheduling::wait(work_ready(), deadline).await;
    }
}
fn work_ready() -> &'static std::sync::Arc<tokio::sync::Notify> {
    WORK_READY.get_or_init(|| std::sync::Arc::new(tokio::sync::Notify::new()))
}
#[cfg(test)]
#[path = "usage/tests.rs"]
mod tests;
