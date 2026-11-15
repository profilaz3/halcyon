//! Account setup is separate from widget appearance and saved profiles.
use super::*;
use crate::calendar as account;

#[derive(Clone, Debug)]
pub enum Message {
    Connect,
    Begun(u64, Result<account::Authorization, account::Problem>),
    Finished(u64, Result<account::Connection, account::Problem>),
    Browser(Result<(), String>),
    Cancel,
    OpenBrowser,
    CopyLink,
    Check,
    Loaded(
        String,
        Result<Vec<account::CalendarChoice>, account::Problem>,
    ),
    Toggle(usize, bool),
    SubscriptionForm(bool),
    SubscriptionHelp(bool),
    SubscriptionProvider(SubscriptionProvider),
    SubscriptionName(String),
    SubscriptionUrl(account::SecretText),
    /// "This link needs a password".
    SubscriptionLogin(bool),
    SubscriptionUsername(account::SecretText),
    SubscriptionPassword(account::SecretText),
    AddSubscription,
    CalDavForm(bool),
    CalDavServer(account::SecretText),
    CalDavUsername(account::SecretText),
    CalDavPassword(account::SecretText),
    FindCalendars,
    Found(u64, Result<Vec<account::CalDavCalendar>, account::Problem>),
    PickCalendar(usize, bool),
    AddCalDav,
    ChooseFile,
    FileChosen(Result<Option<PathBuf>, String>),
    FileName(String),
    AddFile,
    CancelFile,
    /// Asks before removing a subscription: its private link isn't kept.
    ConfirmRemove(Option<String>),
    RemoveSubscription(String),
    ConfirmDisconnect(bool),
    Disconnect,
    Disconnected(Result<account::Connection, account::Problem>),
    ImportClient,
    Picked(Result<Option<PathBuf>, String>),
    PublisherHelp(bool),
    OpenGuide,
}
/// Shown under an address that will be read over plain http.
pub const PLAIN_HTTP: &str = "This server is on your network and the connection is not encrypted.";
/// Read-only subscription instructions. No account state or credentials are
/// involved in choosing a provider here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SubscriptionProvider {
    #[default]
    Google,
    Outlook,
    ICloud,
    Other,
}
impl SubscriptionProvider {
    pub const ALL: [Self; 4] = [Self::Google, Self::Outlook, Self::ICloud, Self::Other];
    pub const LABELS: [&str; 4] = ["Google Calendar", "Outlook", "iCloud", "Other provider"];

    // Keep these steps aligned with docs/CALENDAR.md. They are available
    // offline inside Settings; the developer OAuth guide remains separate.
    pub fn steps(self) -> &'static [&'static str] {
        match self {
            Self::Google => &[
                "1. Open Google Calendar in a desktop browser, then Settings.",
                "2. Under Settings for my calendars, select your calendar.",
                "3. Open Integrate calendar. Copy Secret address in iCal format.",
            ],
            Self::Outlook => &[
                "1. Open Outlook on the web, then Settings → Calendar → Shared calendars.",
                "2. Under Publish a calendar, choose the calendar and details to share.",
                "3. Select Publish, then copy the ICS link, not the HTML link.",
            ],
            Self::ICloud => &[
                "1. Open Calendar on iCloud.com.",
                "2. Open the information button beside your calendar.",
                "3. Enable Public Calendar, then copy its link.",
            ],
            Self::Other => &[
                "1. Find Subscribe, Export or Share in your calendar provider.",
                "2. Copy its iCal or ICS subscription link, then paste it below.",
                "Use an HTTPS or webcal link. Calendar webpages and links needing a browser login will not work. For a local file, use Add calendar file.",
            ],
        }
    }
    pub fn privacy(self) -> &'static str {
        match self {
            Self::Google => {
                "Keep this secret link private. Your calendar does not need to be public. If the address is missing on a work or school account, ask your administrator."
            }
            Self::Outlook => {
                "Publishing lets anyone with the link read the details you chose to share."
            }
            Self::ICloud => {
                "Public Calendar lets anyone with the link read this calendar. Private invitation-only sharing will not work here."
            }
            Self::Other => {
                "Anyone with a private subscription link may be able to read its events. Halcyon does not edit your calendar."
            }
        }
    }
}

pub struct Editor {
    pub connection: account::Connection,
    pub client_ready: bool,
    pub choices: Vec<account::CalendarChoice>,
    pub pending: Option<account::Authorization>,
    pub busy: bool,
    pub connecting: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub confirm_disconnect: bool,
    /// The subscription whose removal is waiting for confirmation.
    pub confirm_remove: Option<String>,
    pub publisher_help: bool,
    pub subscription_form: bool,
    pub subscription_help: bool,
    pub subscription_provider: SubscriptionProvider,
    pub subscription_name: String,
    pub subscription_url: account::SecretText,
    pub subscription_login: bool,
    pub subscription_username: account::SecretText,
    pub subscription_password: account::SecretText,
    pub caldav_form: bool,
    pub caldav_server: account::SecretText,
    pub caldav_username: account::SecretText,
    pub caldav_password: account::SecretText,
    /// Calendars found on the account, and which of them to add.
    pub caldav_found: Vec<account::CalDavCalendar>,
    pub caldav_picked: Vec<bool>,
    pub caldav_searching: bool,
    /// A chosen calendar file waiting for Add calendar.
    pub file_pending: Option<PathBuf>,
    pub file_name: String,
    attempt: u64,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            connection: account::load_connection(),
            client_ready: account::load_client().is_ok(),
            choices: vec![],
            pending: None,
            busy: false,
            connecting: false,
            loading: false,
            error: None,
            confirm_disconnect: false,
            confirm_remove: None,
            publisher_help: false,
            subscription_form: false,
            subscription_help: false,
            subscription_provider: SubscriptionProvider::default(),
            subscription_name: String::new(),
            subscription_url: account::SecretText::default(),
            subscription_login: false,
            subscription_username: account::SecretText::default(),
            subscription_password: account::SecretText::default(),
            caldav_form: false,
            caldav_server: account::SecretText::default(),
            caldav_username: account::SecretText::default(),
            caldav_password: account::SecretText::default(),
            caldav_found: vec![],
            caldav_picked: vec![],
            caldav_searching: false,
            file_pending: None,
            file_name: String::new(),
            attempt: 0,
        }
    }
}
impl Drop for Editor {
    fn drop(&mut self) {
        if let Some(pending) = &self.pending {
            pending.cancel();
        }
    }
}
impl Editor {
    /// Room left in the eight-calendar list.
    pub fn free_slots(&self) -> usize {
        let used = if self.connection.backend == account::Backend::Subscription {
            self.connection.subscriptions.len()
        } else {
            0
        };
        8usize.saturating_sub(used)
    }
    /// Whether any way of adding a calendar is open.
    pub fn adding(&self) -> bool {
        self.subscription_form || self.caldav_form || self.file_pending.is_some()
    }
    /// Closes every add form and forgets what was typed into it.
    fn close_forms(&mut self) {
        self.subscription_form = false;
        self.subscription_help = false;
        self.subscription_name.clear();
        self.subscription_url.0.clear();
        self.subscription_login = false;
        self.subscription_username.0.clear();
        self.subscription_password.0.clear();
        self.caldav_form = false;
        self.caldav_server.0.clear();
        self.caldav_username.0.clear();
        self.caldav_password.0.clear();
        self.caldav_found.clear();
        self.caldav_picked.clear();
        self.caldav_searching = false;
        self.file_pending = None;
        self.file_name.clear();
    }
    fn caldav_login(&self) -> account::Login {
        account::Login {
            username: account::SecretText(self.caldav_username.0.trim().to_string()),
            password: self.caldav_password.clone(),
        }
    }
    pub fn refresh(&mut self) {
        let connection = account::load_connection();
        if self.connection != connection {
            self.connection = connection;
            self.choices.clear();
            self.loading = false;
            self.confirm_disconnect = false;
            self.confirm_remove = None;
        }
        self.client_ready = account::load_client().is_ok();
    }
    pub fn check(&mut self, unlock: bool) -> Task<super::Message> {
        if !self.connection.connected() || self.loading {
            return Task::none();
        }
        self.loading = true;
        self.error = None;
        let connection = self.connection.clone();
        cosmic::task::future(async move {
            let revision = connection.revision.clone();
            super::Message::Calendar(Message::Loaded(
                revision,
                account::calendars(connection, unlock).await,
            ))
        })
    }
}
/// One change to the calendar list, taken from the open form.
enum Change {
    Remove(String),
    Link(String, account::SecretText, Option<account::Login>),
    CalDav(account::Login, Vec<account::CalDavCalendar>),
    File(String, PathBuf),
}
fn wrap(message: Message) -> super::Message {
    super::Message::Calendar(message)
}
async fn open_url(url: String) -> Result<(), String> {
    use std::process::Stdio;
    let mut child = tokio::process::Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "Could not open your browser. Try Open sign-in page again.".to_string())?;
    let status = tokio::time::timeout(Duration::from_secs(8), child.wait())
        .await
        .map_err(|_| "Your browser did not respond. Try Open sign-in page again.".to_string())?
        .map_err(|_| "Could not open your browser.".to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("Could not open your browser. Try Open sign-in page again.".into())
    }
}
impl Settings {
    pub(super) fn update_calendar(&mut self, message: Message) -> Task<super::Message> {
        match message {
            Message::Connect => {
                if self.calendar.busy || self.calendar.connection.cleanup_pending() {
                    return Task::none();
                }
                self.calendar.error = None;
                self.calendar.busy = true;
                self.calendar.connecting = true;
                self.calendar.attempt = self.calendar.attempt.wrapping_add(1);
                let attempt = self.calendar.attempt;
                return cosmic::task::future(async move {
                    wrap(Message::Begun(attempt, account::begin_sign_in().await))
                });
            }
            Message::Begun(attempt, result) => {
                if attempt != self.calendar.attempt {
                    if let Ok(pending) = result {
                        pending.cancel();
                    }
                    return Task::none();
                }
                match result {
                    Ok(pending) => {
                        self.calendar.pending = Some(pending.clone());
                        let url = pending.url().to_string();
                        return Task::batch([
                            cosmic::task::future(async move {
                                wrap(Message::Browser(open_url(url).await))
                            }),
                            cosmic::task::future(async move {
                                wrap(Message::Finished(
                                    attempt,
                                    account::finish_sign_in(pending).await,
                                ))
                            }),
                        ]);
                    }
                    Err(problem) => {
                        self.calendar.busy = false;
                        self.calendar.connecting = false;
                        self.calendar.error = Some(problem.to_string());
                    }
                }
            }
            Message::Finished(attempt, result) => {
                if attempt != self.calendar.attempt {
                    return Task::none();
                }
                self.calendar.pending = None;
                self.calendar.busy = false;
                self.calendar.connecting = false;
                self.calendar.refresh();
                match result {
                    Ok(connection) => {
                        self.calendar.close_forms();
                        self.calendar.connection = connection;
                        self.calendar.error = None;
                        return self.calendar.check(false);
                    }
                    Err(problem) => self.calendar.error = Some(problem.to_string()),
                }
            }
            Message::Browser(result) => {
                if self.calendar.pending.is_some() {
                    if let Err(error) = result {
                        self.calendar.error = Some(error);
                    }
                }
            }
            Message::Cancel => {
                if !self.calendar.connecting {
                    return Task::none();
                }
                if let Some(pending) = self.calendar.pending.take() {
                    pending.cancel();
                }
                self.calendar.attempt = self.calendar.attempt.wrapping_add(1);
                self.calendar.busy = false;
                self.calendar.connecting = false;
                self.calendar.error = None;
            }
            Message::CopyLink => {
                if let Some(pending) = &self.calendar.pending {
                    return cosmic::iced::clipboard::write(pending.url().to_string());
                }
            }
            Message::OpenBrowser => {
                if let Some(pending) = &self.calendar.pending {
                    let url = pending.url().to_string();
                    return cosmic::task::future(async move {
                        wrap(Message::Browser(open_url(url).await))
                    });
                }
            }
            Message::Check => return self.calendar.check(true),
            Message::Loaded(revision, result) => {
                if revision != self.calendar.connection.revision {
                    return Task::none();
                }
                self.calendar.loading = false;
                match result {
                    Ok(choices) => {
                        match account::refresh_metadata(&self.calendar.connection, &choices) {
                            Ok(connection) => self.calendar.connection = connection,
                            Err(problem) => {
                                self.calendar.error = Some(problem.to_string());
                                return Task::none();
                            }
                        }
                        self.calendar.choices = choices;
                        self.calendar.error = self.calendar.connection.selected_calendars().iter().any(|selected| !self.calendar.choices.iter().any(|c| c.id == selected.id)).then(||
                            "A selected calendar is no longer available. Choose calendars again.".into());
                    }
                    Err(problem) => self.calendar.error = Some(problem.to_string()),
                }
            }
            Message::Toggle(index, enabled) => {
                if self.calendar.busy {
                    return Task::none();
                }
                if let Some(choice) = self.calendar.choices.get(index).cloned() {
                    let mut selected = self.calendar.connection.selected_calendars();
                    selected.retain(|c| {
                        self.calendar
                            .choices
                            .iter()
                            .any(|available| available.id == c.id)
                    });
                    selected.retain(|c| c.id != choice.id);
                    if enabled {
                        selected.push(choice);
                    }
                    if selected.is_empty() {
                        self.calendar.error = Some("Keep at least one calendar selected.".into());
                        return Task::none();
                    }
                    match account::save_selections(&self.calendar.connection, selected) {
                        Ok(connection) => {
                            self.calendar.connection = connection;
                            self.calendar.loading = false;
                            self.calendar.error = None;
                        }
                        Err(problem) => self.calendar.error = Some(problem.to_string()),
                    }
                }
            }
            Message::SubscriptionForm(show) => {
                if self.calendar.busy {
                    return Task::none();
                }
                // One add form at a time; closing one forgets its secrets.
                let name = std::mem::take(&mut self.calendar.subscription_name);
                self.calendar.close_forms();
                self.calendar.subscription_form = show;
                if show {
                    self.calendar.subscription_name = name;
                }
            }
            Message::SubscriptionLogin(on) => {
                self.calendar.subscription_login = on;
                if !on {
                    self.calendar.subscription_username.0.clear();
                    self.calendar.subscription_password.0.clear();
                }
            }
            Message::SubscriptionUsername(value) => {
                self.calendar.subscription_username =
                    account::SecretText(value.0.chars().take(256).collect())
            }
            Message::SubscriptionPassword(value) => {
                self.calendar.subscription_password =
                    account::SecretText(value.0.chars().take(1024).collect())
            }
            Message::CalDavForm(show) => {
                if self.calendar.busy {
                    return Task::none();
                }
                self.calendar.close_forms();
                self.calendar.caldav_form = show;
                self.calendar.error = None;
            }
            Message::CalDavServer(value) => {
                self.calendar.caldav_server =
                    account::SecretText(value.0.chars().take(2048).collect());
                // A new address or login needs a new search.
                self.calendar.caldav_found.clear();
                self.calendar.caldav_picked.clear();
            }
            Message::CalDavUsername(value) => {
                self.calendar.caldav_username =
                    account::SecretText(value.0.chars().take(256).collect());
                self.calendar.caldav_found.clear();
                self.calendar.caldav_picked.clear();
            }
            Message::CalDavPassword(value) => {
                self.calendar.caldav_password =
                    account::SecretText(value.0.chars().take(1024).collect());
                self.calendar.caldav_found.clear();
                self.calendar.caldav_picked.clear();
            }
            Message::FindCalendars => {
                let editor = &mut self.calendar;
                if editor.busy
                    || editor.caldav_searching
                    || editor.caldav_server.0.trim().is_empty()
                    || editor.caldav_username.0.trim().is_empty()
                    || editor.caldav_password.0.is_empty()
                {
                    return Task::none();
                }
                editor.caldav_searching = true;
                editor.error = None;
                editor.attempt = editor.attempt.wrapping_add(1);
                let attempt = editor.attempt;
                let server = editor.caldav_server.clone();
                let login = editor.caldav_login();
                return cosmic::task::future(async move {
                    let result = tokio::time::timeout(
                        Duration::from_secs(60),
                        account::discover_caldav(&server, &login),
                    )
                    .await
                    .unwrap_or(Err(account::Problem::TimedOut));
                    wrap(Message::Found(attempt, result))
                });
            }
            Message::Found(attempt, result) => {
                if attempt != self.calendar.attempt || !self.calendar.caldav_form {
                    return Task::none();
                }
                self.calendar.caldav_searching = false;
                match result {
                    Ok(found) => {
                        let free = self.calendar.free_slots();
                        self.calendar.caldav_picked =
                            (0..found.len()).map(|index| index < free).collect();
                        self.calendar.caldav_found = found;
                    }
                    Err(problem) => self.calendar.error = Some(problem.to_string()),
                }
            }
            Message::PickCalendar(index, on) => {
                if let Some(picked) = self.calendar.caldav_picked.get_mut(index) {
                    *picked = on;
                }
            }
            Message::ChooseFile => {
                if self.calendar.busy {
                    return Task::none();
                }
                return cosmic::task::future(async {
                    use cosmic::dialog::file_chooser::{self, FileFilter, open::Dialog};
                    let result =
                        match Dialog::new()
                            .title("Calendar file")
                            .filter(FileFilter::new("iCalendar").glob("*.ics"))
                            .open_file()
                            .await
                        {
                            Ok(response) => response.url().to_file_path().map(Some).map_err(|_| {
                                "Choose a calendar file on this computer.".to_string()
                            }),
                            Err(file_chooser::Error::Cancelled) => Ok(None),
                            Err(_) => Err("Could not open the file chooser.".into()),
                        };
                    wrap(Message::FileChosen(result))
                });
            }
            Message::FileChosen(result) => match result {
                Ok(Some(path)) if !self.calendar.busy => {
                    self.calendar.close_forms();
                    self.calendar.error = None;
                    self.calendar.file_name = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().chars().take(80).collect())
                        .unwrap_or_default();
                    self.calendar.file_pending = Some(path);
                }
                Ok(_) => {}
                Err(error) => self.calendar.error = Some(error),
            },
            Message::FileName(name) => self.calendar.file_name = name.chars().take(80).collect(),
            Message::CancelFile => {
                self.calendar.file_pending = None;
                self.calendar.file_name.clear();
            }
            Message::SubscriptionHelp(show) => {
                self.calendar.subscription_help = show;
            }
            Message::SubscriptionProvider(provider) => {
                self.calendar.subscription_provider = provider;
            }
            Message::SubscriptionName(name) => {
                self.calendar.subscription_name = name.chars().take(80).collect()
            }
            Message::SubscriptionUrl(secret) => {
                self.calendar.subscription_url =
                    account::SecretText(secret.0.chars().take(8192).collect())
            }
            Message::ConfirmRemove(id) => self.calendar.confirm_remove = id,
            Message::AddSubscription
            | Message::RemoveSubscription(_)
            | Message::AddCalDav
            | Message::AddFile => {
                if self.calendar.busy || self.calendar.connection.cleanup_pending() {
                    return Task::none();
                }
                // Removing only follows its confirmation.
                if let Message::RemoveSubscription(id) = &message
                    && self.calendar.confirm_remove.take().as_ref() != Some(id)
                {
                    return Task::none();
                }
                self.calendar.busy = true;
                self.calendar.error = None;
                self.calendar.attempt = self.calendar.attempt.wrapping_add(1);
                let attempt = self.calendar.attempt;
                let expected = self.calendar.connection.clone();
                let editor = &self.calendar;
                let change = match message {
                    Message::RemoveSubscription(id) => Change::Remove(id),
                    Message::AddCalDav => {
                        let picked: Vec<_> = editor
                            .caldav_found
                            .iter()
                            .zip(&editor.caldav_picked)
                            .filter(|(_, on)| **on)
                            .map(|(calendar, _)| calendar.clone())
                            .collect();
                        if picked.is_empty() || picked.len() > editor.free_slots() {
                            self.calendar.busy = false;
                            return Task::none();
                        }
                        Change::CalDav(editor.caldav_login(), picked)
                    }
                    Message::AddFile => match editor.file_pending.clone() {
                        Some(path) => Change::File(editor.file_name.clone(), path),
                        None => {
                            self.calendar.busy = false;
                            return Task::none();
                        }
                    },
                    _ => Change::Link(
                        editor.subscription_name.clone(),
                        editor.subscription_url.clone(),
                        editor.subscription_login.then(|| account::Login {
                            username: account::SecretText(
                                editor.subscription_username.0.trim().to_string(),
                            ),
                            password: editor.subscription_password.clone(),
                        }),
                    ),
                };
                return cosmic::task::future(async move {
                    let result = tokio::time::timeout(Duration::from_secs(90), async move {
                        match change {
                            Change::Remove(id) => account::remove_subscription(expected, id).await,
                            Change::Link(name, secret, login) => {
                                account::add_subscription(expected, name, secret, login).await
                            }
                            Change::CalDav(login, picked) => {
                                account::add_caldav(expected, login, picked).await
                            }
                            Change::File(name, path) => {
                                account::add_file(expected, name, path).await
                            }
                        }
                    })
                    .await
                    .unwrap_or(Err(account::Problem::TimedOut));
                    wrap(Message::Finished(attempt, result))
                });
            }
            Message::ConfirmDisconnect(on) => self.calendar.confirm_disconnect = on,
            Message::Disconnect => {
                if (!self.calendar.confirm_disconnect
                    && !self.calendar.connection.cleanup_pending())
                    || self.calendar.busy
                {
                    return Task::none();
                }
                self.calendar.busy = true;
                self.calendar.error = None;
                let connection = self.calendar.connection.clone();
                return cosmic::task::future(async move {
                    wrap(Message::Disconnected(account::disconnect(connection).await))
                });
            }
            Message::Disconnected(result) => {
                self.calendar.busy = false;
                self.calendar.confirm_disconnect = false;
                self.calendar.refresh();
                if let Err(problem) = result {
                    self.calendar.error = Some(problem.to_string());
                }
            }
            Message::ImportClient => {
                if self.calendar.busy {
                    return Task::none();
                }
                self.calendar.busy = true;
                return cosmic::task::future(async {
                    use cosmic::dialog::file_chooser::{self, FileFilter, open::Dialog};
                    let result = match Dialog::new()
                        .title("Google Desktop app credentials")
                        .filter(FileFilter::new("Google OAuth JSON").glob("*.json"))
                        .open_file()
                        .await
                    {
                        Ok(response) => response
                            .url()
                            .to_file_path()
                            .map(Some)
                            .map_err(|_| "Choose a local JSON file.".to_string()),
                        Err(file_chooser::Error::Cancelled) => Ok(None),
                        Err(_) => Err("Could not open the file chooser.".into()),
                    };
                    wrap(Message::Picked(result))
                });
            }
            Message::Picked(result) => {
                self.calendar.busy = false;
                match result {
                    Ok(Some(path)) => match account::import_client(&path) {
                        Ok(()) => {
                            self.calendar.client_ready = true;
                            self.calendar.error = None;
                            self.calendar.publisher_help = false;
                        }
                        Err(problem) => self.calendar.error = Some(problem.to_string()),
                    },
                    Ok(None) => {}
                    Err(error) => self.calendar.error = Some(error),
                }
            }
            Message::PublisherHelp(on) => self.calendar.publisher_help = on,
            Message::OpenGuide => {
                let path = crate::paths::shared("google-calendar-setup.html").unwrap_or_default();
                return run_command(
                    "/usr/bin/xdg-open".into(),
                    vec![path.to_string_lossy().into_owned()],
                    "Calendar setup guide opened",
                );
            }
        }
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selecting_a_calendar_during_a_check_allows_another_check() {
        const CHILD: &str = "DESKTOP_CARDS_CALENDAR_SELECTION_TEST";
        if std::env::var_os(CHILD).is_none() {
            let root = std::env::temp_dir()
                .join(format!("cards-calendar-selection-{}", std::process::id()));
            for dir in ["config", "data", "cache", "state", "desktop"] {
                std::fs::create_dir_all(root.join(dir)).unwrap();
            }
            std::fs::write(root.join("dconf-profile"), "").unwrap();
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "settings::calendar::tests::selecting_a_calendar_during_a_check_allows_another_check",
                ])
                .env(CHILD, "1")
                .env("DESKTOP_CARDS_CONFIG", root.join("config.json"))
                .env("DESKTOP_CARDS_DESKTOP_ROOT", root.join("desktop"))
                .env("XDG_CONFIG_HOME", root.join("config"))
                .env("XDG_DATA_HOME", root.join("data"))
                .env("XDG_CACHE_HOME", root.join("cache"))
                .env("XDG_STATE_HOME", root.join("state"))
                .env("DCONF_PROFILE", root.join("dconf-profile"))
                .output()
                .unwrap();
            std::fs::remove_dir_all(root).unwrap();
            assert!(
                result.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        let connection = account::Connection {
            backend: account::Backend::Google,
            account: "alex@example.com".into(),
            calendar_id: "primary".into(),
            calendar_name: "Personal".into(),
            revision: "before-selection".into(),
            credential: "fixture".into(),
            ..Default::default()
        };
        std::fs::write(
            account::account_path().unwrap(),
            serde_json::to_vec(&connection).unwrap(),
        )
        .unwrap();
        let (mut settings, _) = Settings::init(Core::default(), Page::Overview);
        let choices = vec![
            account::CalendarChoice {
                id: "primary".into(),
                name: "Personal".into(),
                primary: true,
                color: String::new(),
            },
            account::CalendarChoice {
                id: "team".into(),
                name: "Team".into(),
                primary: false,
                color: String::new(),
            },
        ];
        let _ = settings.update_calendar(Message::Loaded(
            connection.revision.clone(),
            Ok(choices.clone()),
        ));
        // Dropping the returned task leaves the request pending without using
        // the network or a keyring. Its response is delivered explicitly below.
        let _ = settings.update_calendar(Message::Check);
        assert!(settings.calendar.loading);
        let _ = settings.update_calendar(Message::Toggle(1, true));
        assert!(
            account::load_connection()
                .selected_calendars()
                .iter()
                .any(|c| c.id == "team")
        );
        assert!(
            !settings.calendar.loading,
            "the superseded check must not block retry"
        );
        let _ = settings.update_calendar(Message::Check);
        assert!(settings.calendar.loading);
        let _ = settings.update_calendar(Message::Loaded(
            connection.revision,
            Err(account::Problem::SignIn),
        ));
        assert!(
            settings.calendar.loading,
            "a stale result must not finish the new check"
        );
        assert!(settings.calendar.error.is_none());
        let _ = settings.update_calendar(Message::Loaded(
            settings.calendar.connection.revision.clone(),
            Ok(choices),
        ));
        assert!(!settings.calendar.loading);
        assert!(
            settings
                .calendar
                .connection
                .selected_calendars()
                .iter()
                .any(|c| c.id == "team")
        );
    }

    #[test]
    fn polish050_calendar_link_guide_is_local_and_keeps_the_form() {
        if crate::displays::test_support::isolated(
            "settings::calendar::tests::polish050_calendar_link_guide_is_local_and_keeps_the_form",
        ) {
            return;
        }
        let (mut app, _) = Settings::init(Core::default(), Page::Calendar);
        let before = std::fs::read(Config::path().unwrap()).unwrap();
        let account_path = account::account_path().unwrap();
        let connection_before = std::fs::read(&account_path).ok();
        let _ = app.update_calendar(Message::SubscriptionForm(true));
        let _ = app.update_calendar(Message::SubscriptionName("My calendar".into()));
        let _ = app.update_calendar(Message::SubscriptionUrl(account::SecretText(
            "https://example.invalid/private-calendar.ics".into(),
        )));
        let _ = app.update_calendar(Message::SubscriptionHelp(true));
        for provider in SubscriptionProvider::ALL {
            let _ = app.update_calendar(Message::SubscriptionProvider(provider));
            assert_eq!(app.calendar.subscription_provider, provider);
            assert!(app.calendar.subscription_help);
            assert!(!app.calendar.busy);
            assert!(!app.calendar.connecting);
            assert_eq!(app.calendar.subscription_name, "My calendar");
            assert_eq!(
                app.calendar.subscription_url.0,
                "https://example.invalid/private-calendar.ics"
            );
        }
        let _ = app.update_calendar(Message::SubscriptionHelp(false));
        assert!(!app.calendar.subscription_help);
        assert!(!app.calendar.subscription_url.0.is_empty());
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
        assert_eq!(std::fs::read(account_path).ok(), connection_before);
        let _ = app.update_calendar(Message::SubscriptionForm(false));
        assert!(!app.calendar.subscription_help);
        assert!(app.calendar.subscription_url.0.is_empty());
    }

    #[test]
    fn polish050_native_calendar_link_guide_fits_compact_and_wide() {
        if crate::displays::test_support::isolated(
            "settings::calendar::tests::polish050_native_calendar_link_guide_fits_compact_and_wide",
        ) {
            return;
        }
        use crate::settings::tests::{native_reveal, send_settings};
        use cosmic::iced::{Event, mouse};
        fn click(app: &mut Settings, native: &mut crate::native_test::Native, id: &str) {
            let bounds = native_reveal(app, native, id);
            for event in [
                mouse::Event::CursorMoved {
                    position: bounds.center(),
                },
                mouse::Event::ButtonPressed(mouse::Button::Left),
                mouse::Event::ButtonReleased(mouse::Button::Left),
            ] {
                for message in native
                    .event(app.settings_view(), Event::Mouse(event), bounds.center())
                    .0
                {
                    send_settings(app, message);
                }
            }
        }
        for (width, height) in [(820., 600.), (1280., 800.)] {
            let (mut app, _) = Settings::init(Core::default(), Page::Calendar);
            app.calendar.client_ready = false;
            send_settings(
                &mut app,
                super::super::Message::Resized(Size::new(width, height)),
            );
            let mut native = crate::native_test::Native::new(Size::new(width, height));
            click(&mut app, &mut native, "calendar-subscription-open");
            click(&mut app, &mut native, "calendar-link-help");
            assert!(app.calendar.subscription_help);
            for (index, provider) in SubscriptionProvider::ALL.into_iter().enumerate() {
                send_settings(&mut app, wrap(Message::SubscriptionProvider(provider)));
                native_reveal(&app, &mut native, "calendar-link-guide");
                let privacy = native.text_bounds(app.settings_view(), provider.privacy());
                assert!(privacy.width > 0. && privacy.height > 0.);
                assert!(privacy.x >= 0. && privacy.x + privacy.width <= width);
                let add = native_reveal(&app, &mut native, "calendar-subscription-add");
                assert!(
                    add.y >= 0. && add.y + add.height <= height,
                    "{provider:?}: {add:?}"
                );
                native.capture(
                    app.settings_view(),
                    &cosmic::Theme::dark(),
                    &format!("polish050-calendar-link-{index}-{width}"),
                );
            }
            click(&mut app, &mut native, "calendar-link-help");
            assert!(!app.calendar.subscription_help);
            assert!(
                native
                    .try_bounds(app.settings_view(), "calendar-link-guide")
                    .is_none()
            );
            native_reveal(&app, &mut native, "calendar-subscription-add");
        }
    }

    /// Seeds the bundled Ridge picture as the desktop wallpaper of the
    /// isolated fixture, so captures never show another default.
    fn seed_ridge() {
        use crate::desktop::wallpaper::{BgEntry, BgSource, bundled_dir, bundled_path};
        let root = PathBuf::from(std::env::var_os("DESKTOP_CARDS_DESKTOP_ROOT").unwrap());
        assert!(root.starts_with(std::env::temp_dir()));
        let (probe, _) = Settings::init(Core::default(), Page::DesktopWallpaper);
        let bundled = bundled_dir(probe.wallpaper.store().unwrap());
        drop(probe);
        std::fs::create_dir_all(&bundled).unwrap();
        let ridge = bundled_path(&bundled, "halcyon-ridge");
        std::fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("assets/wallpapers/halcyon-ridge.png"),
            &ridge,
        )
        .unwrap();
        let folder = root.join("config/cosmic/com.system76.CosmicBackground/v1");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("all"),
            ron::to_string(&BgEntry::new("all", BgSource::Path(ridge))).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn caldav_account_password_link_and_file_flows() {
        if crate::displays::test_support::isolated(
            "settings::calendar::tests::caldav_account_password_link_and_file_flows",
        ) {
            return;
        }
        use crate::settings::tests::{native_reveal, send_settings};
        use cosmic::iced::{Event, mouse};
        fn click(app: &mut Settings, native: &mut crate::native_test::Native, id: &str) {
            let bounds = native_reveal(app, native, id);
            for event in [
                mouse::Event::CursorMoved {
                    position: bounds.center(),
                },
                mouse::Event::ButtonPressed(mouse::Button::Left),
                mouse::Event::ButtonReleased(mouse::Button::Left),
            ] {
                for message in native
                    .event(app.settings_view(), Event::Mouse(event), bounds.center())
                    .0
                {
                    send_settings(app, message);
                }
            }
        }
        fn visible(native: &mut crate::native_test::Native, app: &Settings, id: &str, size: Size) {
            let bounds = native_reveal(app, native, id);
            assert!(
                bounds.x >= 0.
                    && bounds.x + bounds.width <= size.width
                    && bounds.y >= 0.
                    && bounds.y + bounds.height <= size.height,
                "{id}: {bounds:?}"
            );
        }
        seed_ridge();
        let config_before = std::fs::read(Config::path().unwrap()).unwrap();
        let account_path = account::account_path().unwrap();
        let account_before = std::fs::read(&account_path).ok();
        let password = "app-password-1234";
        for (width, height) in [(820., 600.), (1280., 800.)] {
            let size = Size::new(width, height);
            let (mut app, _) = Settings::init(Core::default(), Page::Calendar);
            app.calendar.client_ready = false;
            send_settings(&mut app, super::super::Message::Resized(size));
            let mut native = crate::native_test::Native::new(size);
            let theme = cosmic::Theme::dark();

            // CalDAV: address, login, then the calendars found there.
            click(&mut app, &mut native, "calendar-caldav-open");
            assert!(app.calendar.caldav_form && !app.calendar.subscription_form);
            native_reveal(&app, &mut native, "calendar-caldav-server");
            let _ = app.update_calendar(Message::CalDavServer(account::SecretText(
                "http://192.168.1.5:5232".into(),
            )));
            assert!(
                native.text_all(app.settings_view(), PLAIN_HTTP).len() == 1,
                "plain http on the local network is pointed out"
            );
            let _ = app.update_calendar(Message::CalDavServer(account::SecretText(
                "caldav.fastmail.com".into(),
            )));
            assert!(native.text_all(app.settings_view(), PLAIN_HTTP).is_empty());
            let _ = app.update_calendar(Message::CalDavUsername(account::SecretText(
                "alex@fastmail.com".into(),
            )));
            let _ = app.update_calendar(Message::CalDavPassword(account::SecretText(
                password.into(),
            )));
            native.capture(
                app.settings_view(),
                &theme,
                &format!("caldav-account-{width}"),
            );
            // The search runs in a task that this test drops: no network.
            let _ = app.update_calendar(Message::FindCalendars);
            assert!(app.calendar.caldav_searching);
            let attempt = app.calendar.attempt;
            let _ =
                app.update_calendar(Message::Found(attempt, Err(account::Problem::Credentials)));
            assert!(!app.calendar.caldav_searching);
            assert_eq!(
                native
                    .text_all(
                        app.settings_view(),
                        "The server did not accept the username or password."
                    )
                    .len(),
                1
            );
            let _ = app.update_calendar(Message::FindCalendars);
            let found = vec![
                account::CalDavCalendar {
                    url: account::SecretText(
                        "https://caldav.fastmail.com/dav/calendars/user/alex/work/".into(),
                    ),
                    name: "Work".into(),
                    color: "#ff2968".into(),
                },
                account::CalDavCalendar {
                    url: account::SecretText(
                        "https://caldav.fastmail.com/dav/calendars/user/alex/home/".into(),
                    ),
                    name: "Home".into(),
                    color: String::new(),
                },
            ];
            // A stale answer is ignored.
            let _ = app.update_calendar(Message::Found(attempt, Ok(found.clone())));
            assert!(app.calendar.caldav_found.is_empty());
            let _ = app.update_calendar(Message::Found(app.calendar.attempt, Ok(found)));
            assert_eq!(app.calendar.caldav_picked, [true, true]);
            visible(&mut native, &app, "calendar-caldav-add", size);
            native.capture(
                app.settings_view(),
                &theme,
                &format!("caldav-calendars-{width}"),
            );
            click(&mut app, &mut native, "calendar-caldav-pick-1");
            assert_eq!(app.calendar.caldav_picked, [true, false]);
            let _ = app.update_calendar(Message::AddCalDav);
            assert!(app.calendar.busy);
            let added = account::CalendarChoice {
                id: "work-id".into(),
                name: "Work".into(),
                primary: false,
                color: "#ff2968".into(),
            };
            let connection = account::Connection {
                backend: account::Backend::Subscription,
                account: "Calendar subscriptions".into(),
                calendar_id: added.id.clone(),
                calendar_name: added.name.clone(),
                revision: "after-caldav".into(),
                credential: "opaque".into(),
                selections: vec![added.clone()],
                subscriptions: vec![added.clone()],
                sources: vec![account::Source {
                    id: added.id.clone(),
                    kind: account::SourceKind::CalDav,
                    path: None,
                }],
            };
            let _ = app.update_calendar(Message::Finished(
                app.calendar.attempt,
                Ok(connection.clone()),
            ));
            assert!(!app.calendar.busy && !app.calendar.caldav_form);
            assert!(app.calendar.caldav_password.0.is_empty());
            assert!(app.calendar.caldav_username.0.is_empty());
            assert!(app.calendar.caldav_found.is_empty());
            let _ = app.update_calendar(Message::Loaded(
                connection.revision.clone(),
                Ok(vec![added.clone()]),
            ));
            click(&mut app, &mut native, "calendar-remove-0");
            assert_eq!(
                native
                    .text_all(
                        app.settings_view(),
                        "You can add it again from your CalDAV account at any time."
                    )
                    .len(),
                1
            );
            let _ = app.update_calendar(Message::ConfirmRemove(None));

            // A link that needs a password.
            click(&mut app, &mut native, "calendar-subscription-open");
            assert!(app.calendar.subscription_form && !app.calendar.caldav_form);
            assert!(
                native
                    .try_bounds(app.settings_view(), "calendar-link-username")
                    .is_none()
            );
            click(&mut app, &mut native, "calendar-link-login");
            assert!(app.calendar.subscription_login);
            native_reveal(&app, &mut native, "calendar-link-username");
            let _ = app.update_calendar(Message::SubscriptionUrl(account::SecretText(
                "https://cal.example.com/private.ics".into(),
            )));
            let _ = app.update_calendar(Message::SubscriptionUsername(account::SecretText(
                "alex".into(),
            )));
            let _ = app.update_calendar(Message::SubscriptionPassword(account::SecretText(
                password.into(),
            )));
            visible(&mut native, &app, "calendar-subscription-add", size);
            native.capture(
                app.settings_view(),
                &theme,
                &format!("calendar-password-link-{width}"),
            );
            let _ = app.update_calendar(Message::SubscriptionForm(false));
            assert!(app.calendar.subscription_password.0.is_empty());
            assert!(!app.calendar.subscription_login);

            // A calendar file, after the file chooser.
            let file = std::env::temp_dir().join("Family.ics");
            let _ = app.update_calendar(Message::FileChosen(Ok(Some(file.clone()))));
            assert_eq!(app.calendar.file_pending.as_ref(), Some(&file));
            assert_eq!(app.calendar.file_name, "Family");
            visible(&mut native, &app, "calendar-file-add", size);
            native.capture(
                app.settings_view(),
                &theme,
                &format!("calendar-file-{width}"),
            );
            let _ = app.update_calendar(Message::CancelFile);
            assert!(app.calendar.file_pending.is_none());

            // Nothing typed was saved to settings or shows up in a message.
            assert!(
                !format!(
                    "{:?}",
                    Message::CalDavPassword(account::SecretText(password.into()))
                )
                .contains(password)
            );
        }
        assert_eq!(
            std::fs::read(Config::path().unwrap()).unwrap(),
            config_before
        );
        assert_eq!(std::fs::read(&account_path).ok(), account_before);
    }
}
