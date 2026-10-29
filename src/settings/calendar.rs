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
    AddSubscription,
    RemoveSubscription(String),
    ConfirmDisconnect(bool),
    Disconnect,
    Disconnected(Result<account::Connection, account::Problem>),
    ImportClient,
    Picked(Result<Option<PathBuf>, String>),
    PublisherHelp(bool),
    OpenGuide,
}
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
                "Use an HTTPS or webcal link. Calendar webpages, local files and links needing a browser login will not work.",
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
    pub publisher_help: bool,
    pub subscription_form: bool,
    pub subscription_help: bool,
    pub subscription_provider: SubscriptionProvider,
    pub subscription_name: String,
    pub subscription_url: account::SecretText,
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
            publisher_help: false,
            subscription_form: false,
            subscription_help: false,
            subscription_provider: SubscriptionProvider::default(),
            subscription_name: String::new(),
            subscription_url: account::SecretText::default(),
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
    pub fn refresh(&mut self) {
        let connection = account::load_connection();
        if self.connection != connection {
            self.connection = connection;
            self.choices.clear();
            self.loading = false;
            self.confirm_disconnect = false;
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
                        self.calendar.subscription_form = false;
                        self.calendar.subscription_help = false;
                        self.calendar.subscription_name.clear();
                        self.calendar.subscription_url.0.clear();
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
                self.calendar.subscription_form = show;
                if !show {
                    self.calendar.subscription_url.0.clear();
                    self.calendar.subscription_help = false;
                }
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
            Message::AddSubscription | Message::RemoveSubscription(_) => {
                if self.calendar.busy || self.calendar.connection.cleanup_pending() {
                    return Task::none();
                }
                self.calendar.busy = true;
                self.calendar.error = None;
                self.calendar.attempt = self.calendar.attempt.wrapping_add(1);
                let attempt = self.calendar.attempt;
                let expected = self.calendar.connection.clone();
                let name = self.calendar.subscription_name.clone();
                let secret = self.calendar.subscription_url.clone();
                let remove = if let Message::RemoveSubscription(id) = message {
                    Some(id)
                } else {
                    None
                };
                return cosmic::task::future(async move {
                    let result = tokio::time::timeout(Duration::from_secs(60), async move {
                        if let Some(id) = remove {
                            account::remove_subscription(expected, id).await
                        } else {
                            account::add_subscription(expected, name, secret).await
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
                let add = native.bounds(app.settings_view(), "calendar-subscription-add");
                assert!(
                    add.y >= 0. && add.y + add.height <= height - 8.,
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
}
