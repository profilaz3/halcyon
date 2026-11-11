//! The app owns its Calendar connection. Tokens live only in Secret Service;
//! configuration and profiles contain no credentials. A legacy connection is
//! recognized only for an existing installation with its own calendar cache.
mod auth;
mod google;
mod keyring;
mod store;
mod subscription;
pub(crate) use subscription::parser_main;
pub use subscription::{SecretText, add_subscription, remove_subscription};
#[cfg(test)]
mod tests;
pub use auth::{Authorization, begin_sign_in, finish_sign_in};
pub use google::CalendarChoice;
pub use store::cached as store_cached;
pub use store::{
    account_path, client_path, import_client, load_client, load_connection, refresh_metadata,
    save_selections,
};

use serde::{Deserialize, Serialize};
use std::time::Duration;

// A recurring description must not expand a small feed into an enormous agenda.
const EVENT_TEXT_BUDGET: usize = 8 * 1024 * 1024;
fn event_text_bytes(event: &crate::data::CalendarEvent) -> usize {
    [
        &event.title,
        &event.start,
        &event.end,
        &event.url,
        &event.description,
        &event.location,
        &event.calendar_name,
        &event.calendar_color,
    ]
    .into_iter()
    .map(|s| s.len())
    .sum::<usize>()
        + event.meeting_url.as_ref().map_or(0, String::len)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    #[default]
    Disconnected,
    Google,
    Subscription,
    Legacy,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Connection {
    pub backend: Backend,
    pub account: String,
    pub calendar_id: String,
    pub calendar_name: String,
    /// Changes on every account/calendar transition. Also identifies this
    /// app's keyring item and fences responses from previous connections.
    pub revision: String,
    pub credential: String,
    /// Empty means the original single-calendar selection (upgrade compatibility).
    pub selections: Vec<CalendarChoice>,
    /// Non-secret subscription labels and opaque IDs; URLs live in the keyring.
    pub subscriptions: Vec<CalendarChoice>,
}
impl Connection {
    pub fn connected(&self) -> bool {
        self.backend != Backend::Disconnected && !self.account.is_empty()
    }
    pub fn cleanup_pending(&self) -> bool {
        self.backend == Backend::Disconnected && !self.credential.is_empty()
    }
    pub fn selected_calendars(&self) -> Vec<CalendarChoice> {
        if self.selections.is_empty() {
            vec![CalendarChoice {
                id: self.calendar_id.clone(),
                name: self.calendar_name.clone(),
                primary: true,
                color: String::new(),
            }]
        } else {
            self.selections.clone()
        }
    }
    pub fn web_url(&self) -> String {
        if self.backend == Backend::Subscription {
            return String::new();
        }
        let mut url = reqwest::Url::parse("https://calendar.google.com/calendar/u/0/r").unwrap();
        if self.connected() {
            url.query_pairs_mut().append_pair("authuser", &self.account);
        }
        url.into()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Problem {
    Disconnected,
    MissingClient,
    InvalidClient,
    Keyring,
    Locked,
    SignIn,
    Denied,
    Network,
    Service,
    InvalidResponse,
    Changed,
    Cancelled,
    TimedOut,
    Storage,
    Cleanup,
    Preview,
    InvalidFeed,
    FeedUrl,
    FeedTimezone,
    FeedLimit,
}
impl Problem {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Disconnected => "Connect a calendar to see your events.",
            Self::MissingClient => "Google sign-in has not been configured for this build yet.",
            Self::InvalidClient => "Choose a Google OAuth JSON file for a Desktop app.",
            Self::Keyring => {
                "The system keyring is unavailable. Start a Secret Service keyring, such as GNOME Keyring or KWallet, then try again."
            }
            Self::Locked => "Unlock your system keyring, then try again.",
            Self::SignIn => {
                "Your Google connection expired or was removed. Sign in again to reconnect."
            }
            Self::Denied => {
                "Google Calendar access was not approved. Try again, or ask your Workspace administrator if access is blocked."
            }
            Self::Network => {
                "Could not reach your calendar provider. Check your internet connection, then try again."
            }
            Self::Service => "Google Calendar isn’t responding right now. Try again later.",
            Self::InvalidResponse => {
                "Google returned an incomplete response. Try connecting again."
            }
            Self::Changed => "The calendar connection changed. Please try again.",
            Self::Cancelled => "Sign-in cancelled. Your previous connection was kept.",
            Self::TimedOut => "The calendar request timed out. Try again.",
            Self::Storage => "Could not save the calendar connection on this computer.",
            Self::Cleanup => {
                "Disconnected. Unlock or start your system keyring, then choose Finish disconnecting to remove the saved sign-in."
            }
            Self::InvalidFeed => {
                "This link did not return a supported calendar. Check the subscription link and try again."
            }
            Self::FeedUrl => "Use an HTTPS or webcal calendar subscription link.",
            Self::FeedTimezone => {
                "This calendar uses an unsupported time zone. Choose a feed using a standard city time zone, such as Europe/Zagreb."
            }
            Self::FeedLimit => {
                "This calendar is too large or has an unsupported recurrence. Your previous events were kept."
            }
            Self::Preview => "Account changes are unavailable in preview mode.",
        }
    }
}
impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}
impl std::error::Error for Problem {}

/// OAuth desktop client values are publisher configuration, not user tokens.
/// They are kept out of Debug, profiles, and logs nevertheless.
#[derive(Clone, Serialize, Deserialize)]
pub struct GoogleClient {
    pub client_id: String,
    pub client_secret: String,
}
impl std::fmt::Debug for GoogleClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GoogleClient([redacted])")
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Credential {
    pub refresh_token: String,
    pub client: GoogleClient,
}

pub async fn calendars(
    connection: Connection,
    unlock: bool,
) -> Result<Vec<CalendarChoice>, Problem> {
    match connection.backend {
        Backend::Disconnected => Err(Problem::Disconnected),
        Backend::Legacy => {
            if unlock {
                let month = chrono::Local::now().format("%Y-%m").to_string();
                tokio::time::timeout(Duration::from_secs(50), legacy(&connection, &month))
                    .await
                    .map_err(|_| Problem::Network)??;
            }
            Ok(vec![CalendarChoice {
                id: connection.calendar_id,
                name: "Primary calendar".into(),
                primary: true,
                color: String::new(),
            }])
        }
        Backend::Subscription => {
            if unlock {
                subscription::events(
                    &connection,
                    &chrono::Local::now().format("%Y-%m").to_string(),
                    true,
                )
                .await?;
            }
            Ok(connection.subscriptions.clone())
        }
        Backend::Google => {
            let access = google::access_token(&connection, unlock).await?;
            google::calendars(&access).await
        }
    }
}

/// Removes credentials an earlier sign-in switch couldn't, keeping the IDs
/// that still fail for the next try.
pub(crate) async fn retry_stale_credentials() -> Result<(), Problem> {
    retry_cleanup_with(|id| async move { keyring::remove(&id).await }).await
}
async fn retry_cleanup_with<F, Fut>(mut remove: F) -> Result<(), Problem>
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = Result<(), Problem>>,
{
    let stale = store::stale_credentials()?;
    let mut failed = false;
    for id in stale {
        let _lease = store::wait_keyring_lease().await?;
        if store::active_credential()?.as_deref() == Some(id.as_str()) {
            continue;
        }
        if remove(id.clone()).await.is_ok() {
            // Remove only this successful ID from the latest registry. An
            // append while the keyring was pending must survive.
            store::update_stale(&[], &[id])?;
        } else {
            failed = true;
        }
    }
    if failed {
        Err(Problem::Cleanup)
    } else {
        Ok(())
    }
}
fn schedule_cleanup() {
    tokio::spawn(async {
        // Connection commit has already succeeded; cleanup is durable and can
        // retry on the next explicit connect/disconnect if the keyring stalls.
        let _ = tokio::time::timeout(Duration::from_secs(60), retry_stale_credentials()).await;
    });
}

pub async fn disconnect(expected: Connection) -> Result<Connection, Problem> {
    if store::preview() {
        return Err(Problem::Preview);
    }
    // Commit the account/cache transition before deleting anything from the
    // keyring. A stale request or failed local write must keep the current
    // credential. The tombstone retains only its ID so cleanup can be retried.
    let disconnected = store::disconnect(&expected)?;
    retry_stale_credentials().await?;
    if disconnected.cleanup_pending() {
        keyring::remove(&disconnected.credential)
            .await
            .map_err(|_| Problem::Cleanup)?;
        let cleaned = Connection {
            credential: String::new(),
            ..disconnected.clone()
        };
        store::replace(&disconnected, &cleaned)?;
        Ok(cleaned)
    } else {
        Ok(disconnected)
    }
}

pub async fn fetch(
    connection: Connection,
    month: String,
) -> Result<crate::data::CalendarData, String> {
    let result = tokio::time::timeout(Duration::from_secs(50), async {
        if !connection.connected() {
            return Err(Problem::Disconnected);
        }
        let data = match connection.backend {
            Backend::Google => google::events(&connection, &month).await?,
            Backend::Legacy => legacy(&connection, &month).await?,
            Backend::Subscription => subscription::events(&connection, &month, false).await?,
            Backend::Disconnected => return Err(Problem::Disconnected),
        };
        cache_fetched(&connection, &month, data)
    })
    .await
    .map_err(|_| Problem::Network)
    .and_then(|result| result);
    result.map_err(|problem| problem.to_string())
}

fn cache_fetched(
    connection: &Connection,
    month: &str,
    mut data: crate::data::CalendarData,
) -> Result<crate::data::CalendarData, Problem> {
    data.connection_revision = connection.revision.clone();
    data.calendar_id = connection.calendar_id.clone();
    store::save_cache(connection, month, &data)?;
    Ok(data)
}

// Tests inject provider completions at the real cache-publication boundary.
#[cfg(test)]
pub(crate) fn fixture_completion(
    connection: &Connection,
    month: &str,
    data: crate::data::CalendarData,
) -> Result<crate::data::CalendarData, String> {
    cache_fetched(connection, month, data).map_err(|e| e.to_string())
}

async fn legacy(
    connection: &Connection,
    month: &str,
) -> Result<crate::data::CalendarData, Problem> {
    use std::process::Stdio;
    if store::preview() {
        return Err(Problem::Preview);
    }
    let home = dirs::home_dir().ok_or(Problem::Storage)?;
    let result =
        tokio::process::Command::new(home.join(".local/share/workspace-mcp/venv/bin/python"))
            .arg(crate::paths::helper("calendar-sync.py").ok_or(Problem::Storage)?)
            .args(["--month", month, "--account", &connection.account])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|_| Problem::SignIn)?;
    if !result.status.success() {
        return Err(Problem::SignIn);
    }
    let data: crate::data::CalendarData =
        serde_json::from_slice(&result.stdout).map_err(|_| Problem::InvalidResponse)?;
    if data.account != connection.account || data.month != month {
        return Err(Problem::Changed);
    }
    Ok(data)
}
