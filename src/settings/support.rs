//! Read-only local observations. Reports are an allowlisted projection, never config dumps.
use super::{AppSettingsPage, Message as AppMessage, Page, Settings, Task};
use super::view::Host;
use crate::desktop::islands::Installation;
use crate::desktop::store::activity::RecentChanges;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    Attention,
    Unknown,
    Neutral,
    Healthy,
}
#[derive(Clone, Copy, Debug)]
pub enum Destination {
    Updates,
    Background,
    Islands,
    Calendar,
    Displays,
    Automation,
    Recovery,
}
#[derive(Clone, Debug)]
pub struct Component {
    pub name: &'static str,
    pub state: &'static str,
    pub health: Health,
    pub action: Destination,
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub captured: String,
    pub version: String,
    pub installed_version: Option<String>,
    pub platform: Option<(String, String)>,
    pub channel: &'static str,
    pub components: Vec<Component>,
    pub displays: Vec<(i32, i32, i32)>,
}
impl Snapshot {
    /// Same text is previewed, copied and exported. No caller-provided messages.
    pub fn report(&self) -> String {
        let mut s = format!(
            "Halcyon support report · version 1\nCaptured: {}\nHalcyon: {}\nUpdate channel: {}\nSettings renderer: wgpu; GPU device not recorded\n\nCached local observations (not a connectivity test)\n",
            self.captured,
            safe_version(&self.version),
            self.channel
        );
        if let Some(version) = &self.installed_version {
            s.push_str(&format!("Installed package: {}\n", safe_version(version)));
        }
        if let Some((name, version)) = &self.platform {
            s.push_str(&format!(
                "Platform: {} {}\n",
                safe_version(name),
                safe_version(version)
            ));
        }
        for c in &self.components {
            s.push_str(&format!("{}: {}\n", c.name, c.state));
        }
        s.push_str("\nDisplays (logical pixels)\n");
        if self.displays.is_empty() {
            s.push_str("Not observed\n");
        }
        for (i, (width, height, scale)) in self.displays.iter().take(32).enumerate() {
            s.push_str(&format!(
                "Display {}: {} × {}, scale {}\n",
                i + 1,
                width,
                height,
                scale
            ));
        }
        s.push_str("\nNo account details, URLs, notes, file paths, commands or change history are included.\nUsage report delivery: not checked.\n");
        s
    }
}
fn safe_version(value: &str) -> &str {
    if !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".+:~-".contains(&b))
    {
        value
    } else {
        "Unknown"
    }
}
#[derive(Clone, Debug, Default)]
pub struct Local {
    pub store: Option<crate::desktop::DesktopStore>,
    pub recent: RecentChanges,
    pub automation: crate::automation::State,
    pub automation_enabled: Option<bool>,
    pub paused: bool,
    pub skipping: bool,
    pub platform: Option<(String, String)>,
    pub problem: bool,
}
#[derive(Clone, Debug)]
pub enum Message {
    Refresh,
    Changed,
    Loaded(u64, Local),
    Filter(Filter),
    /// Opens the setting a recorded change was made in.
    OpenChange(String),
    Details(Option<String>),
    Preview(bool),
    Copy,
    Save,
    Saved(Result<bool, ()>),
    Open(Destination),
    ReportProblem,
}
/// Which changes the Changes page lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    You,
    Automatic,
}
impl Filter {
    pub fn title(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::You => "You",
            Self::Automatic => "Automatic",
        }
    }
}
#[derive(Default)]
pub struct Support {
    pub local: Local,
    pub loading: bool,
    pub observed: Option<chrono::DateTime<chrono::Local>>,
    pub filter: Filter,
    pub expanded: Option<String>,
    pub report: Option<String>,
    pub notice: Option<&'static str>,
    generation: u64,
}
/// The page a recorded change was made on; `Changes` when it has no
/// other home (a restore made from Changes itself).
pub(in crate::settings) fn change_page(entry: &crate::desktop::store::activity::Summary) -> Page {
    use crate::desktop::Owner;
    use crate::desktop::store::activity::Cause;
    match entry.cause {
        Cause::Schedule | Cause::DisplayRule => Page::DesktopAutomation,
        Cause::WallpaperColors => Page::Themes,
        Cause::You => match Owner::parse(&entry.owner) {
            Some(Owner::DockPanel) => Page::DesktopDock,
            Some(Owner::Profile) => Page::DesktopProfiles,
            Some(Owner::Wallpaper) => Page::DesktopWallpaper,
            Some(Owner::Appearance) => Page::Themes,
            Some(Owner::Windows) => Page::DesktopWindows,
            Some(Owner::Workspaces) => Page::DesktopWorkspaces,
            Some(Owner::WindowRules) => Page::DesktopWindowRules,
            Some(Owner::Shortcuts) => Page::DesktopShortcuts,
            Some(Owner::DesktopIcons) => Page::DesktopIcons,
            Some(Owner::Preferences) => Page::Appearance,
            Some(Owner::Recovery) | None => Page::Changes,
        },
    }
}
impl Settings {
    /// Opens where a change was made: the changed setting itself, picked
    /// out as a search result would be, or else its page.
    fn open_change(&mut self, id: &str) -> Task<AppMessage> {
        let Some(entry) = self.support.local.recent.entries.iter().find(|e| e.id == id) else {
            return Task::none();
        };
        let page = change_page(entry);
        if page == Page::Changes {
            return Task::none();
        }
        // Widget preferences span several pages, so their setting decides.
        let any_page = crate::desktop::Owner::parse(&entry.owner)
            == Some(crate::desktop::Owner::Preferences);
        let hit = entry.details.iter().find_map(|detail| {
            super::navigation::search(&detail.setting)
                .into_iter()
                .find(|hit| any_page || hit.page.sidebar() == page.sidebar())
        });
        match hit {
            Some(hit) => self.handle(AppMessage::SearchResult(hit)),
            None => {
                self.remember_route();
                self.handle(AppMessage::Page(page))
            }
        }
    }
    pub(super) fn help_snapshot(&self) -> Snapshot {
        use super::updates::{Check, UpdateChannel};
        use Destination::*;
        use Health::*;
        let mut components = vec![];
        let (state, health) = if !self.service_known {
            ("Not checked", Unknown)
        } else if self.service.running {
            ("On", Healthy)
        } else if self.service.startup {
            ("Off, but set to start at sign-in", Attention)
        } else {
            ("Off", Neutral)
        };
        components.push(Component {
            name: "Running in the background",
            state,
            health,
            action: Background,
        });
        // Same words as the Dock and panel page and Updates.
        let (state, health) = match self.panel_host() {
            Host::Running => ("Running", Healthy),
            Host::Checking => ("Checking…", Unknown),
            Host::Switching => ("Getting ready…", Unknown),
            Host::Switch => ("Not in use. Choose Islands under Dock and panel › Shape", Neutral),
            Host::SignIn => ("Ready. Sign out and back in to start it", Attention),
            Host::Failed(reason) => (reason, Attention),
            Host::Unavailable(reason) => (
                reason,
                match self.desktop.islands.installation {
                    Installation::Absent => Neutral,
                    _ => Attention,
                },
            ),
        };
        components.push(Component {
            name: "Halcyon’s panel",
            state,
            health,
            action: Islands,
        });
        let (state, health) =
            if self.calendar.error.is_some() || self.calendar.connection.cleanup_pending() {
                ("Needs attention", Attention)
            } else if self.calendar.connection.connected()
                || !self.calendar.connection.subscriptions.is_empty()
            {
                ("Connected", Neutral)
            } else {
                ("No calendar connected", Neutral)
            };
        components.push(Component {
            name: "Calendar",
            state,
            health,
            action: Calendar,
        });
        let (state, health) = if matches!(self.updates.check, Check::Failed(_)) {
            ("Last update check failed", Attention)
        } else if matches!(self.updates.check, Check::Available(_)) {
            ("Update available", Attention)
        } else if self
            .updates
            .identity
            .as_ref()
            .is_some_and(|i| i.can_check())
        {
            ("Your system keeps it up to date", Healthy)
        } else {
            ("Comes from your system", Neutral)
        };
        components.push(Component {
            name: "Updates",
            state,
            health,
            action: Updates,
        });
        let (state, health) =
            match self.support.local.automation_enabled {
                None => ("Not checked", Unknown),
                Some(false) => ("Off", Neutral),
                Some(true) => {
                    if self.support.local.paused {
                        ("Paused", Neutral)
                    } else if self.support.local.skipping {
                        ("Skipping the next switch", Neutral)
                    } else if self.support.local.automation.pending.is_some() {
                        ("Waiting for unsaved changes", Attention)
                    } else if self.support.local.automation.last().is_some_and(|o| {
                        matches!(o.effect, crate::automation::Effect::Failed { .. })
                    }) {
                        ("Last switch failed", Attention)
                    } else {
                        ("On", Neutral)
                    }
                }
            };
        components.push(Component {
            name: "Schedules",
            state,
            health,
            action: Automation,
        });
        let displays = self
            .displays
            .outputs()
            .iter()
            .take(32)
            .filter_map(|d| {
                let (w, h) = d.info.logical_size?;
                (w > 0 && h > 0 && w <= 100000 && h <= 100000).then_some((
                    w,
                    h,
                    d.info.scale_factor,
                ))
            })
            .collect::<Vec<_>>();
        components.push(Component {
            name: "Displays",
            state: if displays.is_empty() {
                "None found"
            } else {
                "Connected"
            },
            health: if displays.is_empty() {
                Unknown
            } else {
                Healthy
            },
            action: Displays,
        });
        if self.support.local.problem {
            components.push(Component {
                name: "Changes",
                state: "History can't be read",
                health: Attention,
                action: Recovery,
            });
        }
        components.sort_by_key(|c| match c.health {
            Attention => 0,
            Unknown => 1,
            Neutral => 2,
            Healthy => 3,
        });
        Snapshot {
            captured: chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            version: env!("CARGO_PKG_VERSION").into(),
            installed_version: self
                .updates
                .identity
                .as_ref()
                .and_then(|i| i.installed_version.clone()),
            platform: self.support.local.platform.clone(),
            channel: match self.updates.identity.as_ref().map(|i| i.channel) {
                Some(UpdateChannel::Apt | UpdateChannel::Dnf) => "System packages",
                Some(UpdateChannel::ManualDeb | UpdateChannel::ManualRpm) => "Manual package",
                Some(UpdateChannel::Source) => "Source build",
                _ => "Not verified",
            },
            components,
            displays,
        }
    }
    pub(super) fn load_support(&mut self) -> Task<AppMessage> {
        self.support.generation = self.support.generation.wrapping_add(1);
        let token = self.support.generation;
        // Only the first read shows "Reading…"; later refreshes swap the
        // list in place so nothing flickers after every save.
        self.support.loading = self.support.observed.is_none();
        let store = self.desktop.store().cloned();
        cosmic::task::future(async move {
            let worker = tokio::task::spawn_blocking(move || {
                let Some(store) = store.or_else(|| crate::desktop::DesktopStore::from_env().ok())
                else {
                    return Local {
                        problem: true,
                        ..Default::default()
                    };
                };
                let recent = store.recent_changes();
                let automation = crate::automation::load(&store).ok();
                let now = chrono::Utc::now().timestamp();
                Local {
                    store: Some(store.clone()),
                    problem: recent.is_err(),
                    recent: recent.unwrap_or_default(),
                    automation: crate::automation::load_state(&store),
                    automation_enabled: automation.as_ref().map(|a| a.enabled),
                    paused: automation
                        .as_ref()
                        .is_some_and(|a| a.paused_at(now).is_some()),
                    skipping: automation
                        .as_ref()
                        .is_some_and(|a| a.skipping_at(now).is_some()),
                    platform: read_platform(),
                }
            });
            let local = tokio::time::timeout(std::time::Duration::from_secs(6), worker)
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_else(|| Local {
                    problem: true,
                    ..Default::default()
                });
            AppMessage::Support(Message::Loaded(token, local))
        })
    }
    pub(super) fn update_support(&mut self, message: Message) -> Task<AppMessage> {
        match message {
            Message::Changed => return self.load_support(),
            Message::Refresh => {
                let load = self.load_support();
                self.support.loading = true;
                return Task::batch([
                    load,
                    super::service_task(),
                    self.handle(AppMessage::RecheckInstallation),
                ]);
            }
            Message::Loaded(token, local) if token == self.support.generation => {
                self.support.local = local;
                self.support.loading = false;
                self.support.observed = Some(chrono::Local::now());
            }
            Message::Filter(filter) => {
                self.support.filter = filter;
                self.support.expanded = None;
            }
            Message::OpenChange(id) => return self.open_change(&id),
            Message::Details(id) => self.support.expanded = id,
            Message::Preview(open) => {
                if open && self.support.report.is_none() {
                    self.remember_route();
                }
                self.support.report = open.then(|| self.help_snapshot().report());
                self.support.notice = None;
            }
            Message::Copy => {
                if let Some(report) = &self.support.report {
                    self.support.notice = Some("Report copied");
                    return cosmic::iced::clipboard::write(report.clone());
                }
            }
            Message::Save => {
                if let Some(report) = self.support.report.clone() {
                    let confine = self
                        .support
                        .local
                        .store
                        .as_ref()
                        .or_else(|| self.desktop.store())
                        .and_then(|s| s.fixture_root())
                        .map(std::path::Path::to_path_buf);
                    return cosmic::task::future(async move {
                        let result = async {
                            let response = cosmic::dialog::file_chooser::save::Dialog::new()
                                .title("Save support report".into())
                                .file_name("halcyon-support.txt".into())
                                .save_file()
                                .await;
                            let response = match response {
                                Ok(v) => v,
                                Err(cosmic::dialog::file_chooser::Error::Cancelled) => {
                                    return Ok(false);
                                }
                                Err(_) => return Err(()),
                            };
                            let Some(url) = response.url() else {
                                return Ok(false);
                            };
                            let path = url.to_file_path().map_err(|_| ())?;
                            tokio::task::spawn_blocking(move || {
                                crate::desktop::recovery::write_export(
                                    &path,
                                    report.as_bytes(),
                                    confine.as_deref(),
                                )
                            })
                            .await
                            .map_err(|_| ())?
                            .map_err(|_| ())?;
                            Ok(true)
                        }
                        .await;
                        AppMessage::Support(Message::Saved(result))
                    });
                }
            }
            Message::Saved(result) => {
                self.support.notice = match result {
                    Ok(true) => Some("Report saved"),
                    Ok(false) => None,
                    Err(()) => Some("Couldn’t save the report. Choose another location."),
                }
            }
            Message::Open(destination) => {
                return self.handle(match destination {
                    Destination::Updates | Destination::Islands => {
                        AppMessage::AppSettingsPage(AppSettingsPage::Updates)
                    }
                    Destination::Background => {
                        AppMessage::AppSettingsPage(AppSettingsPage::Operation)
                    }
                    Destination::Recovery => AppMessage::Page(Page::Changes),
                    Destination::Calendar => AppMessage::Page(super::Page::Calendar),
                    Destination::Displays => AppMessage::Page(super::Page::Overview),
                    Destination::Automation => AppMessage::Page(super::Page::DesktopAutomation),
                });
            }
            Message::ReportProblem => {
                return super::run_command(
                    "/usr/bin/xdg-open".into(),
                    vec!["https://github.com/profilaz3/halcyon/issues/new".into()],
                    "Issue page opened",
                );
            }
            _ => {}
        }
        Task::none()
    }
}

fn read_platform() -> Option<(String, String)> {
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open("/etc/os-release")
        .ok()?
        .take(8193)
        .read_to_string(&mut text)
        .ok()?;
    if text.len() > 8192 {
        return None;
    }
    let field = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key))
            .map(|v| v.trim_matches('"').to_string())
            .filter(|v| safe_version(v) != "Unknown")
    };
    Some((field("ID=")?, field("VERSION_ID=")?))
}
