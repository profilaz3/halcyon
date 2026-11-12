//! Native Activity island: a fixed panel slot and an overlapping popup.
//! The desktop supplies a bounded projection of its existing providers.
pub(crate) mod bridge;
mod downloads;
mod host;
mod inbox;
mod live;
mod motion;
mod notifications;
mod outputs;
mod palette;
pub(crate) mod popups;
pub mod preferences;
mod quiet;
mod reactive;
mod surface;
pub(crate) mod system;
#[cfg(test)]
mod tests;
pub(crate) mod timer;
pub(crate) mod tint;
mod transfers;
use bridge::{ClientEvent, Command, Snapshot};
#[cfg(test)]
use palette::contrast;
use palette::{Palette, accent};
use reactive::{NOTE_REVEAL, PENDING_PAUSE};
use cosmic::app::{Core, Task};
use cosmic::iced::{Color, Length, Subscription, keyboard, window};
use cosmic::{Application, Element, widget};
use std::time::{Duration, Instant};
const WIDTH: f32 = 296.;
const HEIGHT: f32 = 128.;
const COMPACT_WIDTH: f32 = 220.;
const COMPACT_HEIGHT: f32 = 32.;
/// Other activities shown as icons beside the title, at most.
const OVERVIEW: usize = 3;
/// Do Not Disturb, as COSMIC's own applets draw it.
const MOON: &str = "weather-clear-night-symbolic";
/// Privacy dots: green while a camera is on, orange for a microphone.
const DOT: f32 = 6.;
/// Room around a privacy dot that still opens its card.
const DOT_REACH: f32 = 4.;
const ICON_BADGE: f32 = 22.;
const BADGE_WIDTH: f32 = 30.;
const OVERVIEW_GAP: f32 = 2.;
/// The width of `icons` app icons and `badges` count badges side by side.
fn overview_width(icons: usize, badges: usize) -> f32 {
    let items = icons + badges;
    if items == 0 {
        return 0.;
    }
    icons as f32 * ICON_BADGE + badges as f32 * BADGE_WIDTH + (items - 1) as f32 * OVERVIEW_GAP
}
/// Notifications listed in the open island, at most; the rest are counted.
const LIST_ROWS: usize = 5;
const ROW_HEIGHT: f32 = 44.;
const ROW_GAP: f32 = 4.;
/// Clear all beneath the rows, with a little air above it.
const LIST_FOOTER: f32 = 40.;
/// The room a card has below its title line, as the chooser uses it.
const LIST_ROOM: f32 = HEIGHT - 50.;
/// The height of a list of `rows` notifications and its footer.
#[cfg(test)]
fn list_height(rows: usize) -> f32 {
    rows as f32 * (ROW_HEIGHT + ROW_GAP) - ROW_GAP + LIST_FOOTER
}
/// How long ago a notification arrived, as a list row says it.
fn note_age(at_ms: i64, now_ms: i64, format_24h: bool) -> String {
    let minutes = (now_ms - at_ms).max(0) / 60_000;
    match minutes {
        0 => "now".into(),
        1..60 => format!("{minutes} min"),
        _ => clock_time(at_ms, format_24h),
    }
}
/// The pointer rests this long before the capsule opens, so crossing it on the
/// way to something else does nothing. Motion can only postpone it up to the cap.
const HOVER_DELAY: Duration = Duration::from_millis(150);
const HOVER_MAX_WAIT: Duration = Duration::from_millis(600);
const LEAVE_GRACE: Duration = Duration::from_millis(300);
/// The panel proxy withholds the popup's pointer enter until the next motion,
/// and sends nothing when the pointer had already gone while the popup mapped.
/// This bounds that otherwise stuck-open view and leaves time to read it.
const UNENTERED_GRACE: Duration = Duration::from_secs(5);
/// How long a Do Not Disturb switch made here waits for the desktop.
const DND_SETTLE: Duration = Duration::from_secs(3);
/// Escape with the pointer still on the capsule must not reopen it on the next
/// small motion. Leaving the capsule ends this sooner.
const ESCAPE_HOVER_BLOCK: Duration = Duration::from_secs(10);
#[derive(Clone, Copy, PartialEq)]
enum Role {
    /// The title: always visible, never a filled control.
    Identity,
    Transport,
    /// The one main action for the current activity.
    Primary,
    /// A secondary labeled action.
    Quiet,
    /// A secondary icon action, resting without a fill.
    Ghost,
    /// A small count that must read as a control on any panel.
    Badge,
}
impl Palette {
    fn accent(self, kind: Kind) -> Color {
        self.readable(kind.accent())
    }
}
fn alpha(color: Color, a: f32) -> Color {
    Color { a, ..color }
}
fn capsule_button(palette: Palette, tint: Color, reveal: f32, role: Role) -> cosmic::theme::Button {
    let style = move |focused: bool, level: usize, disabled: bool| {
        let ink = match role {
            Role::Identity => 1.,
            _ if disabled => 0.32 * reveal,
            _ => reveal,
        };
        let fill = match role {
            Role::Identity => Color::TRANSPARENT,
            Role::Primary if disabled => alpha(palette.ink, 0.05 * reveal),
            // The one filled action, in the activity's own color.
            Role::Primary => {
                let t = [0., 0.14, 0.26][level];
                alpha(
                    Color::from_rgb(
                        tint.r + (palette.ink.r - tint.r) * t,
                        tint.g + (palette.ink.g - tint.g) * t,
                        tint.b + (palette.ink.b - tint.b) * t,
                    ),
                    reveal,
                )
            }
            // The primary round control: a deep disc of the accent.
            Role::Transport => alpha(
                tint,
                if disabled { 0.08 } else { [0.2, 0.28, 0.36][level] } * reveal,
            ),
            Role::Quiet if disabled => alpha(palette.ink, 0.03 * reveal),
            Role::Quiet => alpha(palette.ink, [0.07, 0.13, 0.18][level] * reveal),
            Role::Ghost if disabled => Color::TRANSPARENT,
            Role::Ghost => alpha(palette.ink, [0., 0.1, 0.16][level] * reveal),
            Role::Badge => alpha(palette.ink, [0.16, 0.24, 0.3][level]),
        };
        cosmic::widget::button::Style {
            icon_color: Some(if role == Role::Transport {
                alpha(tint, ink)
            } else if role == Role::Identity {
                tint
            } else {
                alpha(palette.ink, ink)
            }),
            text_color: Some(alpha(palette.ink, ink)),
            background: Some(fill.into()),
            border_radius: 24.0.into(),
            outline_width: if focused { 1.5 } else { 0. },
            outline_color: if role == Role::Primary { palette.ink } else { tint },
            ..Default::default()
        }
    };
    cosmic::theme::Button::Custom {
        active: Box::new(move |f, _| style(f, 0, false)),
        hovered: Box::new(move |f, _| style(f, 1, false)),
        pressed: Box::new(move |f, _| style(f, 2, false)),
        disabled: Box::new(move |_| style(false, 0, true)),
    }
}
/// What the capsule says about the selected activity. One title and one short
/// cue travel with the capsule; details appear only once it is open.
#[derive(Debug, Default, PartialEq)]
struct Content {
    title: String,
    cue: String,
    meta: String,
    /// Elapsed share of a running Focus session or a started Calendar event.
    progress: Option<f32>,
    /// Where the line starts: a download with no known total moves a
    /// segment along instead of filling.
    bar_start: f32,
}
fn clock_time(ms: i64, format_24h: bool) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format(if format_24h { "%H:%M" } else { "%-I:%M %p" })
                .to_string()
        })
        .unwrap_or_default()
}
fn clock_range(start: i64, end: i64, format_24h: bool) -> String {
    let (first, last) = (clock_time(start, format_24h), clock_time(end, format_24h));
    let period = |text: &str| text.rsplit(' ').next().map(str::to_owned);
    match first.rsplit_once(' ') {
        Some((time, _)) if period(&first) == period(&last) => format!("{time}–{last}"),
        _ => format!("{first}–{last}"),
    }
}
fn calendar_cue(starts_at_ms: i64, now: i64) -> String {
    if starts_at_ms <= now {
        return "Now".into();
    }
    let minutes = (starts_at_ms - now + 59_999) / 60_000;
    if minutes < 60 {
        format!("In {minutes} min")
    } else {
        format!("In {} h", (minutes + 30) / 60)
    }
}
fn read_24h() -> bool {
    use cosmic::cosmic_config::{Config, ConfigGet};
    Config::new("com.system76.CosmicAppletTime", 1)
        .ok()
        .and_then(|config| config.get::<bool>("military_time").ok())
        .unwrap_or(false)
}
pub(crate) const APPLET_ID: &str = "io.github.profilaz3.Halcyon.Activity";
/// The session ledger of restarts into updates (see `crate::upgrade`).
const RESTARTS: &str = "activity-island";
/// How long the island must be left alone before restarting into an update.
const RESTART_QUIET: Duration = Duration::from_secs(2);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Idle,
    System(system::Source),
    Music,
    Focus,
    Calendar,
    /// A desktop notification, by its daemon id.
    Notification(u32),
    /// The quick timer, counting, paused or done.
    Timer,
    /// Copies and downloads in progress.
    Transfer,
}
impl Kind {
    fn label(self) -> &'static str {
        match self {
            Self::Idle => "Activities",
            Self::Notification(_) => "Notification",
            Self::System(source) => source.label(),
            Self::Music => "Music",
            Self::Focus => "Focus",
            Self::Calendar => "Calendar",
            Self::Timer => "Timer",
            Self::Transfer => "Transfers",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Self::Idle => "view-grid-symbolic",
            Self::Notification(_) => "preferences-system-notifications-symbolic",
            Self::System(source) => source.icon(),
            Self::Music => "audio-x-generic-symbolic",
            Self::Focus => "alarm-symbolic",
            Self::Calendar => "x-office-calendar-symbolic",
            Self::Timer => "alarm-symbolic",
            Self::Transfer => "folder-download-symbolic",
        }
    }
    fn accent(self) -> Color {
        match self {
            Self::Idle => accent::IDLE,
            Self::System(_) => accent::SYSTEM,
            Self::Music => accent::MUSIC,
            Self::Focus => accent::FOCUS,
            Self::Calendar => accent::CALENDAR,
            Self::Notification(_) => accent::NOTIFICATION,
            Self::Timer => accent::TIMER,
            Self::Transfer => accent::JOB,
        }
    }
    /// A stable widget id fragment.
    fn id(self) -> String {
        match self {
            Self::Idle => "idle".into(),
            Self::Music => "music".into(),
            Self::Focus => "focus".into(),
            Self::Calendar => "calendar".into(),
            Self::System(source) => source.label().to_lowercase(),
            Self::Notification(id) => format!("notification-{id}"),
            Self::Timer => "timer".into(),
            Self::Transfer => "transfer".into(),
        }
    }
}
pub struct Applet {
    core: Core,
    popup: Option<window::Id>,
    motion: motion::Motion,
    snapshot: Snapshot,
    /// The artwork tile, decoded once per picture from the snapshot.
    art: Option<(String, widget::image::Handle)>,
    commands: Option<tokio::sync::mpsc::Sender<Command>>,
    kind: Kind,
    held: bool,
    hover_deadline: Option<Instant>,
    hover_cap: Instant,
    leave_deadline: Option<Instant>,
    hovered: bool,
    next_clock: Instant,
    priority_deadline: Option<Instant>,
    now_ms: i64,
    problem: Option<String>,
    supported: bool,
    /// The owning Panel can show the island: Islands or a Standard bar.
    active: bool,
    panel_frame: host::Frame,
    starting: bool,
    popup_entered: bool,
    hover_block: Option<Instant>,
    format_24h: bool,
    chooser: bool,
    host_reveals: bool,
    reveal_deadline: Option<Instant>,
    last_reveal: Option<Instant>,
    observed: bool,
    seen_meeting: Option<String>,
    paused_key: Option<String>,
    paused_until: Option<Instant>,
    music_sampled: Instant,
    dismissed_calendar: Option<String>,
    selected_source: Option<String>,
    system_deadlines: Vec<(system::Source, i64, Instant)>,
    promote_popup: Option<window::Id>,
    /// Desktop notifications, newest first. Never leave this process.
    notes: Vec<notifications::Note>,
    /// The panel's notification connection is up.
    notes_ready: bool,
    /// What was last asked of COSMIC's popups: `Some(true)` while taken.
    popups: Option<bool>,
    /// Notifications that arrived while a reveal could not show, oldest
    /// first. They show once the way is clear: fullscreen ended, the screen
    /// unlocked or the island closed.
    pending: Vec<u32>,
    /// When to try showing the pending notifications.
    pending_at: Option<Instant>,
    /// The notifications the showing reveal stands for. Beyond
    /// `SUMMARY_AFTER` it reads as one "N new notifications".
    batch: Vec<u32>,
    /// Do Not Disturb as just switched here, until the desktop reports it.
    dnd_wish: Option<(bool, Instant)>,
    /// The open island came by itself, as a reveal, and has not been taken
    /// in hand. Pointing at it keeps it open but leaves this set, so a
    /// right-click or swipe can still send it away.
    revealed: bool,
    /// The open island lists every notification instead of one.
    list: bool,
    /// Height the open popup has beyond its usual size, for the list.
    popup_extra: f32,
    /// Notifications that left the list this session, newest first. Kept
    /// in memory only, like `notes`, and bounded.
    recent: Vec<notifications::Recent>,
    /// The Recent section is open.
    recent_open: bool,
    /// Arrivals from muted apps this session.
    muted: usize,
    /// The app group open in the list, by app key; one at a time.
    open_group: Option<String>,
    /// The app whose rules menu is open.
    menu: Option<inbox::MenuTarget>,
    /// App rules as just chosen here, until the desktop reports them.
    rules_wish: Option<(Vec<preferences::AppRuleEntry>, Instant)>,
    /// When the next notification's own timeout ends.
    expiry_at: Option<Instant>,
    /// The timer, transfers, scrubber and outputs.
    ui: live::Ui,
    /// A working update replaced the program: the new one, and the earliest
    /// moment to restart into it (see `crate::upgrade`).
    updated: Option<(crate::upgrade::Program, Instant)>,
    /// When to restart into the update. Armed only while nothing is in use.
    restart_at: Option<Instant>,
}
#[derive(Debug, Clone)]
pub enum Message {
    PanelMode(bool),
    PanelObserved(host::Observation),
    Enter(bool),
    Leave(bool),
    /// The pointer moved while waiting to open.
    Moved,
    Hold,
    Open,
    Close,
    PopupGone(window::Id),
    FocusLost(window::Id),
    Frame(Instant),
    Wake(Duration),
    Surface(cosmic::surface::Action),
    Backend(ClientEvent),
    Select(Kind),
    Choose,
    Dismiss,
    Key(keyboard::Key),
    Control(Command),
    Settings(&'static str),
    SystemSettings(system::Source),
    Start,
    Started(Result<(), String>),
    SettingsOpened(Result<(), String>),
    /// Open the current meeting's checked https link.
    Join(String),
    Notes(notifications::Event),
    /// Run a notification's action; "default" is the notification itself.
    NoteAction(u32, String),
    NoteDismiss(u32),
    NotesClear,
    /// Open the island on the list of notifications.
    ShowNotes,
    /// Switch COSMIC's Do Not Disturb.
    Dnd(bool),
    /// Middle button: play or pause.
    Middle,
    /// Right button: send a reveal away, otherwise bring the player forward.
    Secondary,
    /// A touchpad swipe over the island.
    Swipe,
    PopupsSet(Result<bool, String>),
    /// Open or close an app's group in the list.
    GroupToggle(String),
    /// Dismiss every notification in an app's group.
    GroupDismiss(String),
    RecentToggle,
    RecentClear,
    /// Open a recent notification, or its app.
    RecentOpen(u32),
    /// Open the island on the Recent list.
    ShowRecent,
    /// Open the rules menu for the app of a notification, live or recent.
    NoteMenu(u32),
    MenuClose,
    /// Set, or with `None` reset, the rule for the app in the open menu.
    NoteRule(Option<preferences::AppRule>),
    /// The timer, transfers, scrubber and outputs.
    Live(live::Msg),
    /// A working update replaced the program.
    Updated(crate::upgrade::Program),
    /// The quiet moment before restarting into the update came.
    RestartDue(Duration),
    Noop,
}
impl Application for Applet {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APPLET_ID;
    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }
    fn init(core: Core, _: ()) -> (Self, Task<Message>) {
        let now = Instant::now();
        let supported = core.applet.anchor == cosmic::applet::cosmic_panel_config::PanelAnchor::Top;
        (
            Self {
                core,
                popup: None,
                motion: motion::Motion::new(now),
                snapshot: Snapshot::default(),
                art: None,
                commands: None,
                kind: Kind::Idle,
                held: false,
                hover_deadline: None,
                hover_cap: now,
                leave_deadline: None,
                hovered: false,
                next_clock: now,
                priority_deadline: None,
                now_ms: chrono::Utc::now().timestamp_millis(),
                problem: None,
                supported,
                active: false,
                panel_frame: host::Frame::default(),
                starting: false,
                popup_entered: false,
                hover_block: None,
                format_24h: read_24h(),
                chooser: false,
                host_reveals: false,
                reveal_deadline: None,
                last_reveal: None,
                observed: false,
                seen_meeting: None,
                paused_key: None,
                paused_until: None,
                music_sampled: now,
                dismissed_calendar: None,
                selected_source: None,
                system_deadlines: Vec::new(),
                promote_popup: None,
                notes: Vec::new(),
                notes_ready: false,
                popups: None,
                pending: Vec::new(),
                pending_at: None,
                batch: Vec::new(),
                dnd_wish: None,
                revealed: false,
                list: false,
                popup_extra: 0.,
                recent: Vec::new(),
                recent_open: false,
                muted: 0,
                open_group: None,
                menu: None,
                rules_wish: None,
                expiry_at: None,
                ui: live::Ui::default(),
                updated: None,
                restart_at: None,
            },
            Task::none(),
        )
    }
    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
    fn subscription(&self) -> Subscription<Message> {
        let mut all = vec![
            host::subscription(&self.core.applet).map(Message::PanelObserved),
            crate::upgrade::watch().map(Message::Updated),
        ];
        if let Some(at) = self.restart_at {
            all.push(crate::schedule::wake_at(at).map(Message::RestartDue));
        }
        if !self.active {
            return Subscription::batch(all);
        }
        all.push(bridge::client().map(Message::Backend));
        all.push(notifications::subscription().map(Message::Notes));
        all.push(transfers::subscription().map(|u| Message::Live(live::Msg::Transfer(u))));
        all.push(downloads::subscription().map(|u| Message::Live(live::Msg::Transfer(u))));
        let clock = (self.popup.is_some()
            && self.kind == Kind::Music
            && self.music_progress().is_some()
            && self.snapshot.music.as_ref().is_some_and(|m| m.playing))
            || (self.kind == Kind::Focus
                && self.snapshot.focus.running
                && self
                    .snapshot
                    .focus
                    .ends_at_ms
                    .is_some_and(|end| end > self.now_ms))
            || (self.kind == Kind::Calendar
                && self
                    .snapshot
                    .calendar
                    .as_ref()
                    .is_some_and(|c| c.ends_at_ms > self.now_ms))
            // A running timer counts once a second, open or closed.
            || (self.kind == Kind::Timer && self.timer().is_some_and(|t| t.running()));
        if let Some(at) = self
            .hover_deadline
            .into_iter()
            .chain(self.leave_deadline)
            .chain(self.priority_deadline)
            .chain(self.paused_until)
            .chain(self.reveal_deadline)
            .chain(self.pending_at)
            .chain(self.expiry_at)
            .chain(self.ui.jobs.deadline())
            .chain(clock.then_some(self.next_clock))
            .min()
        {
            all.push(crate::schedule::wake_at(at).map(Message::Wake));
        }
        all.push(cosmic::iced::event::listen_with(
            |event, _, _| match event {
                cosmic::iced::Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                    Some(Message::Key(key))
                }
                cosmic::iced::Event::PlatformSpecific(
                    cosmic::iced::event::PlatformSpecific::Wayland(
                        cosmic::iced::event::wayland::Event::Popup(
                            cosmic::iced::event::wayland::PopupEvent::Unfocused,
                            _,
                            id,
                        ),
                    ),
                ) => Some(Message::FocusLost(id)),
                _ => None,
            },
        ));
        Subscription::batch(all)
    }
    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::PopupGone(id))
    }
    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        self.arm_restart(Instant::now());
        task
    }
    fn view(&self) -> Element<'_, Message> {
        if !self.active {
            return self
                .core
                .applet
                .autosize_window(widget::Space::new().width(1.).height(1.))
                .into();
        }
        if !self.supported {
            return self
                .core
                .applet
                .icon_button("view-more-symbolic")
                .on_press(Message::Hold)
                .into();
        }
        self.core.applet.autosize_window(self.capsule(false)).into()
    }
    fn view_window(&self, _: window::Id) -> Element<'_, Message> {
        self.capsule(true)
    }
}
impl Applet {
    /// Decodes a new artwork thumbnail; the same picture is never decoded twice.
    fn keep_art(&mut self) {
        match &self.snapshot.music_art {
            None => self.art = None,
            Some(encoded) if self.art.as_ref().is_some_and(|(k, _)| k == encoded) => {}
            Some(encoded) => {
                let side = tint::DISC as u32;
                self.art = tint::thumbnail_pixels(encoded)
                    .and_then(|rgba| tint::disc(&rgba))
                    .map(|rgba| {
                        (
                            encoded.clone(),
                            widget::image::Handle::from_rgba(side, side, rgba),
                        )
                    });
            }
        }
    }
    /// Every surface, text and line colour the island draws, for its style.
    fn palette(&self) -> Palette {
        Palette::new(&self.core.applet, self.snapshot.preferences.style)
    }
    /// Every message, before the restart for an update is re-armed.
    fn handle(&mut self, message: Message) -> Task<Message> {
        let now = Instant::now();
        if !self.active
            && matches!(
                &message,
                Message::Enter(_)
                    | Message::Leave(_)
                    | Message::Moved
                    | Message::Hold
                    | Message::Open
                    | Message::Select(_)
                    | Message::Choose
                    | Message::Dismiss
                    | Message::Control(_)
                    | Message::Start
                    | Message::Settings(_)
                    | Message::SystemSettings(_)
                    | Message::Join(_)
                    | Message::NoteAction(..)
                    | Message::NoteDismiss(_)
                    | Message::NotesClear
                    | Message::ShowNotes
                    | Message::GroupToggle(_)
                    | Message::GroupDismiss(_)
                    | Message::RecentToggle
                    | Message::RecentClear
                    | Message::RecentOpen(_)
                    | Message::ShowRecent
                    | Message::NoteMenu(_)
                    | Message::MenuClose
                    | Message::NoteRule(_)
                    | Message::Dnd(_)
                    | Message::Middle
                    | Message::Secondary
                    | Message::Swipe
                    | Message::Key(_)
                    | Message::Backend(_)
                    | Message::Frame(_)
                    | Message::Wake(_)
                    | Message::Live(_)
            )
        {
            return Task::none();
        }
        if crate::tools::preview() && std::env::var_os("HALCYON_ACTIVITY_TRACE").is_some() {
            let tag = match &message {
                Message::Enter(true) => "enter_popup",
                Message::Enter(false) => "enter_panel",
                Message::Leave(true) => "leave_popup",
                Message::Leave(false) => "leave_panel",
                Message::Open => "open",
                Message::Close => "close",
                Message::Control(_) => "control",
                Message::Hold => "hold",
                Message::Choose => "choose",
                Message::Select(Kind::Focus) => "select_focus",
                Message::Key(keyboard::Key::Named(keyboard::key::Named::Escape)) => "escape",
                Message::Key(keyboard::Key::Named(keyboard::key::Named::Tab)) => "tab",
                Message::Frame(_) => "frame",
                Message::Wake(_) => "wake",
                Message::PopupGone(_) => "gone",
                Message::FocusLost(_) => "focus_lost",
                _ => "other",
            };
            if tag != "other" {
                eprintln!(
                    "activity {tag} p={:.4} target={} popup={}",
                    self.motion.value,
                    self.motion.target(),
                    self.popup.is_some()
                );
            }
        }
        match message {
            Message::PanelObserved(observed) => {
                let changed = self.panel_frame != observed.frame;
                self.host_reveals = observed.reveals_allowed;
                let stop_reveal = !self.host_reveals && self.reveal_deadline.is_some();
                self.panel_frame = observed.frame;
                let mode = self.update(Message::PanelMode(observed.enabled));
                if changed && self.popup.is_some() {
                    self.motion.retarget(0., false, now);
                    return Task::batch([mode, self.destroy_popup()]);
                }
                return if stop_reveal {
                    Task::batch([mode, self.update(Message::Close)])
                } else {
                    mode
                };
            }
            Message::PanelMode(active) => {
                if self.active != active {
                    self.active = active;
                    if !active {
                        self.observed = false;
                        self.reveal_deadline = None;
                        self.pending.clear();
                        self.pending_at = None;
                        self.paused_key = None;
                        self.paused_until = None;
                        self.commands = None;
                        self.snapshot = Snapshot::default();
                        self.priority_deadline = None;
                        self.motion.retarget(0., false, now);
                        return Task::batch([self.destroy_popup(), self.sync_popups()]);
                    }
                }
            }
            Message::Backend(event) => match event {
                ClientEvent::Connected(tx) => {
                    self.commands = Some(tx);
                    self.problem = None;
                }
                ClientEvent::Offline => {
                    if self.commands.is_some() {
                        self.problem = None;
                    }
                    self.priority_deadline = None;
                    self.commands = None;
                    self.snapshot = Snapshot::default();
                    if self.popup.is_none() {
                        self.kind = Kind::Idle;
                    }
                    return self.sync_popups();
                }
                ClientEvent::Snapshot(mut snapshot) => {
                    // The switch just flipped here wins until the desktop
                    // has seen the change, so it never flickers back.
                    if let Some((wish, until)) = self.dnd_wish {
                        if snapshot.dnd == wish || now >= until {
                            self.dnd_wish = None;
                        } else {
                            snapshot.dnd = wish;
                        }
                    }
                    // So does a rule just chosen in the island.
                    if let Some((wish, until)) = &self.rules_wish {
                        if snapshot.preferences.app_rules == *wish || now >= *until {
                            self.rules_wish = None;
                        } else {
                            snapshot.preferences.app_rules = wish.clone();
                        }
                    }
                    self.now_ms = chrono::Utc::now().timestamp_millis();
                    let system_reveal = self.new_system_reveal(&snapshot);
                    // A timer just finished asks for the island first.
                    let timer_due = snapshot.timer.is_some_and(|t| t.done)
                        && !self.snapshot.timer.is_some_and(|t| t.done);
                    self.observe_playback(&snapshot, now);
                    self.music_sampled = now;
                    self.observe_system_deadlines(&snapshot, now);
                    self.snapshot = snapshot;
                    self.keep_art();
                    if !self.snapshot.preferences.notifications {
                        self.notes.clear();
                        self.pending.clear();
                        self.recent.clear();
                        self.muted = 0;
                    }
                    self.drop_muted();
                    let popups = self.sync_popups();
                    self.refresh_deadline(now);
                    self.problem = None;
                    if self.popup.is_none() {
                        self.kind = self.automatic_kind();
                    }
                    if self.snapshot.reduced_motion {
                        self.motion.retarget(self.motion.target(), false, now);
                    }
                    let critical = self.showing_critical();
                    if (self.snapshot.quiet != Some(false)
                        || (!self.snapshot.preferences.reveals && !self.showing_always())
                        || (self.snapshot.dnd && critical.is_none()))
                        && self.reveal_deadline.is_some()
                    {
                        // A critical notice cut short by the lock screen or a
                        // fullscreen app comes back when the person does.
                        if let Some(id) = critical {
                            self.queue_note(id);
                        }
                        self.reveal_deadline = None;
                        return Task::batch([popups, self.update(Message::Close)]);
                    }
                    if self.snapshot.quiet == Some(false) {
                        self.arm_pending(now);
                    }
                    let reveal = timer_due.then_some(Kind::Timer).or(system_reveal);
                    return Task::batch([popups, self.consider_reveal(reveal, now)]);
                }
            },
            Message::Enter(popup) => {
                self.hovered = true;
                self.leave_deadline = None;
                self.popup_entered |= popup;
                if popup {
                    self.reveal_deadline = None;
                }
                if popup && self.popup.is_some() {
                    if self.motion.target() == 0. && self.hover_block.is_none_or(|end| now >= end) {
                        self.motion.retarget(1., self.animates(), now);
                    }
                } else if self.popup.is_none() && self.hover_block.is_none_or(|end| now >= end) {
                    self.hover_block = None;
                    self.hover_cap = now + HOVER_MAX_WAIT;
                    self.hover_deadline = Some(now + HOVER_DELAY);
                }
            }
            Message::Moved => {
                if self.hover_deadline.is_some() {
                    self.hover_deadline = Some((now + HOVER_DELAY).min(self.hover_cap));
                }
            }
            Message::Leave(false) if self.popup.is_some() => {
                // Mapping the overlapping popup removes pointer focus from the
                // applet, so this is normally a handoff rather than a departure.
                // Only a popup that has not seen the pointer needs the bound.
                if !self.popup_entered && !self.held {
                    self.hovered = false;
                    self.leave_deadline.get_or_insert(now + UNENTERED_GRACE);
                }
            }
            Message::Leave(popup) => {
                self.hovered = false;
                self.hover_deadline = None;
                if popup {
                    self.popup_entered = false;
                } else {
                    self.hover_block = None;
                }
                if !self.held && self.popup.is_some() {
                    self.leave_deadline = Some(now + LEAVE_GRACE);
                } else if self.popup.is_none() {
                    self.arm_pending(now + LEAVE_GRACE);
                }
            }
            Message::Open => {
                self.hover_deadline = None;
                self.leave_deadline = None;
                self.now_ms = chrono::Utc::now().timestamp_millis();
                self.paused_until = None;
                if self.popup.is_none() {
                    self.seen_pending();
                    return self.open_popup();
                }
                self.motion.retarget(1., self.animates(), now);
            }
            Message::Hold => {
                self.reveal_deadline = None;
                self.revealed = false;
                self.hover_deadline = None;
                self.leave_deadline = None;
                self.hover_block = None;
                self.now_ms = chrono::Utc::now().timestamp_millis();
                if self.popup.is_none() {
                    self.held = true;
                    self.paused_until = None;
                    self.seen_pending();
                    return self.open_popup();
                }
                if self.motion.target() == 0. {
                    // A press on the closing capsule reopens and keeps it.
                    self.held = true;
                    self.motion.retarget(1., self.animates(), now);
                    return Task::none();
                }
                if !self.held {
                    return self.hold_popup();
                }
                return self.update(Message::Close);
            }
            Message::Close => {
                self.promote_popup = None;
                self.reveal_deadline = None;
                self.held = false;
                self.hover_deadline = None;
                self.leave_deadline = None;
                self.motion.retarget(0., self.animates(), now);
                if !self.motion.active() {
                    return self.destroy_popup();
                }
            }
            Message::FocusLost(id) => {
                if self.promote_popup == Some(id) {
                    return Task::none();
                }
                // The panel can briefly transfer keyboard focus while a
                // pointer click is delivered inside its embedded popup.
                if self.popup == Some(id) && !self.hovered {
                    return self.update(Message::Close);
                }
            }
            Message::PopupGone(id) => {
                if self.promote_popup == Some(id) && self.active {
                    self.promote_popup = None;
                    self.popup = None;
                    let source = self.selected_source.clone();
                    // Reopened in hand, it keeps what it was showing.
                    let open = self.open_popup_sized();
                    self.selected_source = source;
                    return open;
                }
                if self.popup == Some(id) {
                    self.popup = None;
                    self.held = false;
                    self.hovered = false;
                    self.popup_entered = false;
                    self.hover_deadline = None;
                    self.leave_deadline = None;
                    self.now_ms = chrono::Utc::now().timestamp_millis();
                    self.arm_pause_grace();
                    self.kind = self.automatic_kind();
                    self.chooser = false;
                    self.list = false;
                    self.reset_inbox();
                    self.ui.view = live::View::Card;
                    self.batch.clear();
                    self.revealed = false;
                    self.motion.retarget(0., false, now);
                    self.arm_pending(now + PENDING_PAUSE);
                }
            }
            Message::Frame(at) => {
                if self.popup.is_some() {
                    self.motion.advance(at);
                    if !self.motion.active() && self.motion.target() == 0. {
                        return self.destroy_popup();
                    }
                }
            }
            Message::Wake(early) => {
                for at in [
                    &mut self.hover_deadline,
                    &mut self.leave_deadline,
                    &mut self.priority_deadline,
                    &mut self.paused_until,
                    &mut self.reveal_deadline,
                    &mut self.pending_at,
                    &mut self.expiry_at,
                ]
                .into_iter()
                .flatten()
                {
                    *at = at.checked_sub(early).unwrap_or(now);
                }
                self.now_ms = chrono::Utc::now().timestamp_millis();
                if self.ui.jobs.tick(now + early) && self.popup.is_none() {
                    self.kind = self.automatic_kind();
                }
                if self.paused_until.is_some_and(|at| at <= now) {
                    self.paused_until = None;
                    self.paused_key = None;
                    if self.popup.is_none() {
                        self.kind = self.automatic_kind();
                    }
                }
                if self.priority_deadline.is_some_and(|at| at <= now) {
                    if self.popup.is_none() {
                        self.kind = self.automatic_kind();
                    }
                }
                if self.reveal_deadline.is_some_and(|at| at <= now) {
                    // A finished timer stays like a critical notice.
                    let critical = self.showing_critical().is_some()
                        || (self.kind == Kind::Timer && self.timer_done() && !self.chooser);
                    self.reveal_deadline = None;
                    if !self.held && !self.hovered {
                        if critical {
                            // Stays until someone points at it or dismisses it.
                            self.reveal_deadline = Some(now + NOTE_REVEAL);
                            return Task::none();
                        }
                        if let Some(next) = self.next_pending(now) {
                            return next;
                        }
                        return self.update(Message::Close);
                    }
                }
                if self.expiry_at.is_some_and(|at| at <= now) {
                    self.expire_notes();
                }
                if self.pending_at.is_some_and(|at| at <= now) {
                    self.pending_at = None;
                    let shown = self.flush_pending(now);
                    if self.reveal_deadline.is_some() {
                        return shown;
                    }
                }
                self.refresh_deadline(now);
                let reveal = self.consider_reveal(None, now);
                if self.reveal_deadline.is_some() {
                    return reveal;
                }
                let interval = if self.kind == Kind::Calendar { 60 } else { 1 };
                self.next_clock =
                    crate::schedule::next_clock_tick(now, Duration::from_secs(interval));
                if self.hover_deadline.is_some_and(|at| at <= now) {
                    self.hover_deadline = None;
                    return self.update(Message::Open);
                }
                if self.leave_deadline.is_some_and(|at| at <= now) {
                    self.leave_deadline = None;
                    if !self.held && !self.hovered {
                        return self.update(Message::Close);
                    }
                }
            }
            Message::Choose => {
                self.reveal_deadline = None;
                self.chooser = !self.chooser;
                self.ui.view = live::View::Card;
                return self.hold_popup();
            }
            Message::GroupToggle(_)
            | Message::GroupDismiss(_)
            | Message::RecentToggle
            | Message::RecentClear
            | Message::RecentOpen(_)
            | Message::ShowRecent
            | Message::NoteMenu(_)
            | Message::MenuClose
            | Message::NoteRule(_) => return self.inbox_update(message, now),
            Message::Notes(event) => {
                // A notification with a percentage is a job as well.
                let job = self.note_transfer(&event, now);
                return Task::batch([job, self.note_event(event, now)]);
            }
            Message::Live(msg) => return self.live_update(msg, now),
            Message::NoteAction(id, action) => {
                let Some(note) = self.note(id) else {
                    return Task::none();
                };
                if !note.resident {
                    self.forget_note(id);
                }
                let close = self.update(Message::Close);
                let run = cosmic::task::future(async move {
                    Message::SettingsOpened(notifications::invoke(id, action).await)
                });
                return Task::batch([close, run]);
            }
            Message::NoteDismiss(id) => {
                if let Some(note) = self.note(id).cloned() {
                    self.remember(note, false);
                }
                self.forget_note(id);
                let next = self.eligible_kinds().into_iter().find(|k| matches!(k, Kind::Notification(_)));
                let follow = match next {
                    Some(kind) if self.popup.is_some() => {
                        self.kind = kind;
                        Task::none()
                    }
                    _ => self.update(Message::Close),
                };
                let dismiss = cosmic::task::future(async move {
                    Message::SettingsOpened(notifications::dismiss(vec![id]).await)
                });
                return Task::batch([follow, dismiss]);
            }
            Message::NotesClear => {
                let notes: Vec<notifications::Note> = self.notes.drain(..).collect();
                let ids: Vec<u32> = notes.iter().map(|n| n.id).collect();
                // Oldest first, so Recent keeps them newest first.
                for note in notes.into_iter().rev() {
                    self.remember(note, false);
                }
                self.open_group = None;
                // The closing card keeps its heading, not Recent's.
                self.list = false;
                self.pending.clear();
                self.batch.clear();
                let close = self.update(Message::Close);
                let dismiss = cosmic::task::future(async move {
                    Message::SettingsOpened(notifications::dismiss(ids).await)
                });
                return Task::batch([close, dismiss]);
            }
            Message::ShowNotes => {
                if !self.notes_shown() || (self.notes.is_empty() && !self.has_recent()) {
                    return Task::none();
                }
                self.chooser = false;
                self.batch.clear();
                self.reveal_deadline = None;
                self.revealed = false;
                self.hover_deadline = None;
                self.leave_deadline = None;
                self.hover_block = None;
                // The list leads with what the closed island showed: an
                // urgent notification, otherwise the newest.
                if !matches!(self.kind, Kind::Notification(id) if self.note(id).is_some())
                    && let Some(first) = self.notes.iter().find(|n| n.urgent).or(self.notes.first())
                {
                    self.kind = Kind::Notification(first.id);
                }
                self.capture_source();
                self.list = true;
                self.problem = None;
                self.now_ms = chrono::Utc::now().timestamp_millis();
                let Some(id) = self.popup else {
                    self.held = true;
                    self.paused_until = None;
                    self.seen_pending();
                    return self.open_popup_sized();
                };
                if !self.held || self.popup_extra < self.list_extra() {
                    // A popup's size is fixed when it opens: reopen it in
                    // hand and tall enough for the rows.
                    self.held = true;
                    self.promote_popup = Some(id);
                    return self
                        .update(Message::Surface(cosmic::surface::action::destroy_popup(id)));
                }
                if self.motion.target() == 0. {
                    self.motion.retarget(1., self.animates(), now);
                }
            }
            Message::Middle => {
                let Some(m) = self.snapshot.music.as_ref() else {
                    return Task::none();
                };
                if !self.live() || self.snapshot.music_problem {
                    return Task::none();
                }
                let key = m.key.clone();
                let command = if m.playing {
                    m.pause.then_some(Command::Pause { key })
                } else {
                    m.play.then_some(Command::Play { key })
                };
                if let Some(command) = command {
                    return self.update(Message::Control(command));
                }
            }
            Message::Secondary | Message::Swipe if self.revealed && self.popup.is_some() => {
                // Like Escape: the pointer resting here must not reopen it.
                if self.hovered {
                    self.hover_block = Some(now + ESCAPE_HOVER_BLOCK);
                }
                return self.update(Message::Close);
            }
            // Held open on one notification, the right button offers its
            // app's rules, as a list row does.
            Message::Secondary
                if self.popup.is_some()
                    && !self.chooser
                    && !self.listing()
                    && matches!(self.kind, Kind::Notification(id) if self.note(id).is_some()) =>
            {
                if let Kind::Notification(id) = self.kind {
                    return self.update(Message::NoteMenu(id));
                }
            }
            // A row's own right-click opened its rules, or the list is
            // open: the player is not raised from under it.
            Message::Secondary if self.listing() => {}
            Message::Secondary => {
                if let Some(m) = self.snapshot.music.as_ref().filter(|_| self.live()) {
                    let key = m.key.clone();
                    return self.update(Message::Control(Command::Raise { key }));
                }
            }
            Message::Swipe => {}
            Message::Dnd(on) => {
                self.snapshot.dnd = on;
                self.dnd_wish = Some((on, now + DND_SETTLE));
                return cosmic::task::future(async move {
                    Message::SettingsOpened(popups::set_dnd(on).await)
                });
            }
            Message::PopupsSet(result) => {
                if result.is_err() {
                    // Asked again on the next change rather than in a loop.
                    self.popups = None;
                }
            }
            Message::Dismiss => {
                if self.kind == Kind::Calendar && self.selected_source_matches() {
                    self.dismissed_calendar =
                        self.snapshot.calendar.as_ref().map(|c| c.key.clone());
                }
                return self.update(Message::Close);
            }
            Message::Select(kind) => {
                if !self.live() || !self.eligible_kinds().contains(&kind) {
                    return Task::none();
                }
                self.chooser = false;
                self.list = false;
                self.ui.view = live::View::Card;
                self.batch.clear();
                self.kind = kind;
                self.capture_source();
                self.problem = None;
                self.now_ms = chrono::Utc::now().timestamp_millis();
            }
            Message::Control(command) => {
                if let Some(tx) = &self.commands {
                    if tx.try_send(command).is_err() {
                        self.problem = Some("Please try again".into());
                    } else {
                        self.problem = None;
                    }
                } else {
                    self.problem = Some("Halcyon is reconnecting".into());
                }
            }
            Message::Key(key) => match key.as_ref() {
                // Escape leaves a rules menu before the island.
                keyboard::Key::Named(keyboard::key::Named::Escape) if self.menu.is_some() => {
                    self.menu = None;
                }
                keyboard::Key::Named(keyboard::key::Named::Escape)
                    if self.popup.is_some() && self.ui.view != live::View::Card =>
                {
                    return self.live_update(live::Msg::Back, now);
                }
                keyboard::Key::Named(
                    named @ (keyboard::key::Named::ArrowLeft | keyboard::key::Named::ArrowRight),
                ) if self.popup.is_some() && self.held && self.scrubbing() => {
                    let step = if named == keyboard::key::Named::ArrowLeft {
                        -live::SEEK_STEP
                    } else {
                        live::SEEK_STEP
                    };
                    return self.live_update(live::Msg::SeekBy(step), now);
                }
                keyboard::Key::Named(keyboard::key::Named::Escape) => {
                    if self.popup.is_some() && self.hovered {
                        self.hover_block = Some(now + ESCAPE_HOVER_BLOCK);
                    }
                    return self.update(Message::Close);
                }
                keyboard::Key::Named(keyboard::key::Named::Tab) if self.popup.is_some() => {
                    return self.hold_popup();
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowDown) if self.popup.is_none() => {
                    self.held = true;
                    return self.update(Message::Open);
                }
                _ => {}
            },
            Message::Join(url) => {
                let close = self.update(Message::Close);
                let launch = cosmic::task::future(async move {
                    Message::SettingsOpened(
                        tokio::process::Command::new("xdg-open")
                            .arg(url)
                            .spawn()
                            .map(|_| ())
                            .map_err(|_| "Could not open the meeting link".into()),
                    )
                });
                return Task::batch([close, launch]);
            }
            Message::SystemSettings(source) => {
                let close = self.update(Message::Close);
                let launch = cosmic::task::future(async move {
                    Message::SettingsOpened(system::open_settings(source).await)
                });
                return Task::batch([close, launch]);
            }
            Message::Settings(page) => {
                // Like a native menu item: the popup has done its job.
                let launch = cosmic::task::future(async move {
                    let opened = match std::env::current_exe() {
                        Ok(exe) => tokio::process::Command::new(exe)
                            .args(["--settings", page])
                            .status()
                            .await
                            .is_ok_and(|status| status.success()),
                        Err(_) => false,
                    };
                    Message::SettingsOpened(if opened {
                        Ok(())
                    } else {
                        Err("Could not open Settings".into())
                    })
                });
                return Task::batch([self.update(Message::Close), launch]);
            }
            Message::Start => {
                if self.starting || self.commands.is_some() {
                    return Task::none();
                }
                self.starting = true;
                self.problem = None;
                return cosmic::task::future(async {
                    if crate::tools::preview() {
                        return Message::Started(Err("Start the preview desktop first".into()));
                    }
                    let result = tokio::time::timeout(
                        Duration::from_secs(8),
                        tokio::process::Command::new("systemctl")
                            .args(["--user", "start", crate::paths::SERVICE])
                            .kill_on_drop(true)
                            .output(),
                    )
                    .await;
                    Message::Started(match result {
                        Ok(Ok(out)) if out.status.success() => Ok(()),
                        _ => Err("Could not start Halcyon".into()),
                    })
                });
            }
            Message::Started(result) => {
                self.starting = false;
                self.problem = result.err();
            }
            Message::SettingsOpened(result) => {
                // Settings finishing says nothing about an earlier failure.
                if let Err(error) = result {
                    self.problem = Some(error);
                }
            }
            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
            Message::Updated(new) => match crate::upgrade::Ledger::session(RESTARTS).plan(&new) {
                crate::upgrade::Plan::Restart { after } => self.updated = Some((new, now + after)),
                crate::upgrade::Plan::Stay => eprintln!(
                    "Halcyon was updated, but the Activity pill restarted often lately; it keeps the running version until the next sign-in"
                ),
            },
            Message::RestartDue(early) => {
                if self.restart_due(now + early)
                    && let Some((new, _)) = self.updated.take()
                {
                    // Returns only when the restart cannot be recorded.
                    crate::upgrade::restart(
                        &crate::upgrade::Ledger::session(RESTARTS),
                        &new,
                        "the Activity pill",
                    );
                    self.restart_at = None;
                }
            }
            Message::Noop => {}
        }
        Task::none()
    }
    /// Nothing about the island is in use: no island open or on its way, no
    /// reveal showing, no pointer on it, nothing starting.
    fn idle(&self) -> bool {
        self.popup.is_none()
            && self.promote_popup.is_none()
            && !self.held
            && !self.hovered
            && self.hover_deadline.is_none()
            && self.leave_deadline.is_none()
            && self.reveal_deadline.is_none()
            && !self.motion.active()
            && !self.starting
    }
    /// Arms the restart into an update once the island has been left alone
    /// for `RESTART_QUIET`, and disarms it as soon as it is in use again.
    fn arm_restart(&mut self, now: Instant) {
        let Some((_, earliest)) = &self.updated else {
            return;
        };
        if self.idle() {
            let earliest = *earliest;
            self.restart_at
                .get_or_insert_with(|| (now + RESTART_QUIET).max(earliest));
        } else {
            self.restart_at = None;
        }
    }
    fn restart_due(&self, now: Instant) -> bool {
        self.updated.is_some() && self.idle() && self.restart_at.is_some_and(|at| now >= at)
    }
    /// Only the top edge has a slot to grow from. Other edges open as ordinary
    /// anchored popups, already complete.
    fn animates(&self) -> bool {
        self.supported && !self.snapshot.reduced_motion
    }
    fn live(&self) -> bool {
        self.commands.is_some() && !self.snapshot.focus.key.is_empty()
    }
    fn refresh_deadline(&mut self, now: Instant) {
        self.priority_deadline = self
            .snapshot
            .calendar
            .as_ref()
            .into_iter()
            .flat_map(|c| {
                [
                    c.starts_at_ms
                        .saturating_sub(self.snapshot.preferences.lead_ms()),
                    c.starts_at_ms,
                    c.starts_at_ms.saturating_add(120_000),
                    c.ends_at_ms,
                ]
            })
            .chain(self.snapshot.focus.ends_at_ms)
            .chain(self.snapshot.system.iter().filter_map(|a| a.expires_ms))
            .filter_map(|at| at.checked_sub(self.now_ms).filter(|delta| *delta > 0))
            .min()
            .and_then(|delta| now.checked_add(Duration::from_millis(delta as u64)));
        self.priority_deadline = self
            .priority_deadline
            .into_iter()
            .chain(
                self.system_deadlines
                    .iter()
                    .map(|(_, _, at)| *at)
                    .filter(|at| *at > now),
            )
            .min();
    }
    fn hold_popup(&mut self) -> Task<Message> {
        self.reveal_deadline = None;
        self.revealed = false;
        self.leave_deadline = None;
        if self.held {
            return Task::none();
        }
        self.held = true;
        // Retire the hover popup before requesting a new native grab. Sending
        // both lifecycle requests together can let the old popup's teardown
        // remove the replacement's keyboard focus in the panel proxy.
        if let Some(id) = self.popup {
            self.promote_popup = Some(id);
            self.update(Message::Surface(cosmic::surface::action::destroy_popup(id)))
        } else {
            self.open_popup()
        }
    }
    /// Several notifications open as a list, unless one is being revealed.
    fn wants_list(&self) -> bool {
        self.notes_shown()
            && self.notes.len() > 1
            && !self.chooser
            && !self.revealed
            && self.summary().is_none()
            && matches!(self.kind, Kind::Notification(_))
    }
    /// The open island is listing notifications, Recent or an app's rules.
    fn listing(&self) -> bool {
        self.popup.is_some()
            && !self.chooser
            && self.notes_shown()
            && (self.menu.is_some()
                || (self.list && (!self.notes.is_empty() || self.has_recent())))
            && (self.notes.is_empty() || matches!(self.kind, Kind::Notification(_)))
    }
    /// Closed, the island forgets which group, section or menu was open.
    fn reset_inbox(&mut self) {
        self.open_group = None;
        self.recent_open = false;
        self.menu = None;
        self.arm_expiry();
    }
    fn popup_height(&self) -> f32 {
        self.frame().popup_height() + self.popup_extra
    }
    fn open_popup(&mut self) -> Task<Message> {
        self.list = self.wants_list();
        self.open_popup_sized()
    }
    /// Opens the popup for what is set to show, tall enough for a list.
    fn open_popup_sized(&mut self) -> Task<Message> {
        self.popup_extra = if self.list || self.menu.is_some() {
            self.list_extra()
        } else {
            self.live_extra()
        };
        let id = window::Id::unique();
        self.capture_source();
        self.popup = Some(id);
        self.popup_entered = false;
        self.format_24h = read_24h();
        self.motion.retarget(1., self.animates(), Instant::now());
        // The host reserves this fixed slot. The popup never asks the panel to relayout.
        self.update(Message::Surface(
            cosmic::surface::action::app_popup::<Self>(
                |_| cosmic::surface::action::LiveSettings {
                    blur: Some(false),
                    ..Default::default()
                },
                move |s| {
                    let mut p = s.core.applet.get_popup_settings(
                        s.core.main_window_id().unwrap(),
                        id,
                        Some((s.frame().popup_width() as u32, s.popup_height() as u32)),
                        None,
                        None,
                    );
                    p.grab = s.held;
                    if s.supported {
                        p.positioner.anchor_rect = cosmic::iced::Rectangle {
                            x: 0,
                            y: 0,
                            width: s.slot_width() as i32,
                            height: 1,
                        };
                    }
                    p.positioner.offset = if s.supported {
                        (0, -1 - s.frame().padding as i32)
                    } else {
                        p.positioner.offset
                    };
                    p.positioner.size_limits = cosmic::iced::Limits::NONE
                        .width(s.frame().popup_width())
                        .height(s.popup_height());
                    p.positioner.constraint_adjustment = 3;
                    // COSMIC's popup proxy drops offsets from multi-rectangle
                    // input regions. Keep one bounded native popup input area;
                    // the animated visual remains clipped inside it.
                    p.input_zone = None;
                    p
                },
                Some(Box::new(|s| s.capsule(true).map(cosmic::Action::App))),
            ),
        ))
    }
    fn destroy_popup(&mut self) -> Task<Message> {
        self.promote_popup = None;
        self.held = false;
        self.chooser = false;
        self.list = false;
        self.reset_inbox();
        self.ui.view = live::View::Card;
        self.arm_pause_grace();
        self.hover_deadline = None;
        self.leave_deadline = None;
        self.hovered = false;
        self.popup_entered = false;
        self.batch.clear();
        self.revealed = false;
        self.now_ms = chrono::Utc::now().timestamp_millis();
        self.kind = self.automatic_kind();
        if self.popup.is_some() {
            self.arm_pending(Instant::now() + PENDING_PAUSE);
        }
        if let Some(id) = self.popup.take() {
            return self.update(Message::Surface(cosmic::surface::action::destroy_popup(id)));
        }
        Task::none()
    }
    fn text<'a>(&self, value: impl Into<String>, size: f32, color: Color) -> Element<'a, Message> {
        widget::text(value.into())
            .size(size)
            .wrapping(cosmic::iced::advanced::text::Wrapping::None)
            .ellipsize(cosmic::iced::advanced::text::Ellipsize::End(
                cosmic::iced::advanced::text::EllipsizeHeightLimit::Lines(1),
            ))
            .class(cosmic::theme::Text::Color(color))
            .into()
    }
    /// Tooltips follow the panel palette. The delay keeps them from flashing
    /// over neighbors as the pointer crosses a control.
    fn tip<'a>(
        &self,
        palette: Palette,
        content: impl Into<Element<'a, Message>>,
        label: &str,
        position: widget::tooltip::Position,
    ) -> Element<'a, Message> {
        widget::tooltip(content, self.text(label, 12., palette.ink), position)
            .padding(6)
            .gap(4)
            .delay(Duration::from_millis(500))
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::widget::container::Style {
                    background: Some(palette.surface.into()),
                    border: cosmic::iced::Border {
                        radius: 8.0.into(),
                        width: 1.,
                        color: alpha(palette.ink, 0.12),
                    },
                    ..Default::default()
                }
            }))
            .into()
    }
    fn content(&self) -> Content {
        let now = self.now_ms;
        if !self.live() {
            let (title, meta) = if let Some(problem) = &self.problem {
                (problem.clone(), "Try again, or open Settings for help")
            } else if self.commands.is_some() {
                ("Connecting to Halcyon…".into(), "")
            } else if self.starting {
                ("Starting Halcyon…".into(), "This takes a few seconds")
            } else {
                (
                    "Halcyon isn't running".into(),
                    "Widgets and activities are paused",
                )
            };
            return Content {
                title,
                meta: meta.into(),
                ..Default::default()
            };
        }
        if !self.selected_source_matches() {
            return Content {
                title: self.kind.label().into(),
                meta: "This activity has ended".into(),
                ..Default::default()
            };
        }
        let mut content = match self.kind {
            Kind::Idle => Content {
                title: "Activities".into(),
                meta: {
                    let waiting: Vec<Kind> = self
                        .eligible_kinds()
                        .into_iter()
                        .filter(|kind| *kind != Kind::Idle)
                        .collect();
                    match waiting.as_slice() {
                        [] => "Nothing going on".into(),
                        [one] => format!("{} is waiting", one.label()),
                        [first, rest @ ..] => {
                            format!("{} and {} more waiting", first.label(), rest.len())
                        }
                    }
                },
                ..Default::default()
            },
            Kind::System(source) => self.system_content(source),
            Kind::Notification(_) if self.summary().is_some() => {
                let count = self.batch.len();
                let mut apps: Vec<&str> = Vec::new();
                for note in self.notes.iter().filter(|n| self.batch.contains(&n.id)) {
                    if !apps.contains(&note.app.as_str()) {
                        apps.push(&note.app);
                    }
                }
                let meta = match apps.as_slice() {
                    [] => String::new(),
                    [one] => (*one).to_owned(),
                    [a, b] => format!("{a} and {b}"),
                    [a, b, rest @ ..] => format!("{a}, {b} and {} more", rest.len()),
                };
                Content {
                    title: format!("{count} new notifications"),
                    meta,
                    ..Default::default()
                }
            }
            Kind::Notification(id) => match self.note(id) {
                Some(n) => Content {
                    title: n.title.clone(),
                    meta: if self.snapshot.preferences.notification_text && !n.body.is_empty() {
                        n.body.clone()
                    } else {
                        n.app.clone()
                    },
                    ..Default::default()
                },
                None => Content {
                    title: "Notification".into(),
                    meta: "This notification was closed".into(),
                    ..Default::default()
                },
            },
            Kind::Music => self.music_content(),
            Kind::Focus => self.focus_content(now),
            Kind::Calendar => self.calendar_content(now),
            Kind::Timer => self.timer_content(),
            Kind::Transfer => self.transfer_content(),
        };
        if let Some(problem) = &self.problem {
            content.meta.clone_from(problem);
        }
        content
    }
    fn music_content(&self) -> Content {
        let problem = self.snapshot.music_problem;
        match &self.snapshot.music {
            Some(m) => Content {
                title: if m.title.is_empty() {
                    "Music".into()
                } else {
                    m.title.clone()
                },
                // Sound is its own signal, so only the exception needs a word.
                cue: if !problem && !m.playing {
                    "Paused".into()
                } else {
                    String::new()
                },
                meta: if problem {
                    "Player unavailable".into()
                } else {
                    m.artist.clone()
                },
                progress: self.music_progress().map(|(p, n)| p as f32 / n as f32),
                bar_start: 0.,
            },
            None if problem => Content {
                title: "Music unavailable".into(),
                meta: "Player not responding".into(),
                ..Default::default()
            },
            None => Content {
                title: "Nothing playing".into(),
                meta: "Start a music app".into(),
                ..Default::default()
            },
        }
    }
    fn focus_content(&self, now: i64) -> Content {
        let f = &self.snapshot.focus;
        let ms = f
            .ends_at_ms
            .map_or(f.remaining_ms, |end| (end - now).max(0));
        let seconds = (ms + 999) / 1000;
        let clock = format!("{}:{:02}", seconds / 60, seconds % 60);
        let session = if f.label == "Focus" {
            "Focus session".to_string()
        } else {
            f.label.clone()
        };
        if f.done || f.ends_at_ms.is_some_and(|end| end <= now) {
            return Content {
                title: format!("{} complete", f.label),
                meta: if f.label == "Focus" {
                    "Ready for a break"
                } else {
                    "Ready to focus"
                }
                .into(),
                ..Default::default()
            };
        }
        match f.ends_at_ms.filter(|_| f.running) {
            Some(end) => Content {
                title: clock,
                cue: f.label.clone(),
                meta: format!("Ends {}", clock_time(end, self.format_24h)),
                progress: (f.remaining_ms > 0)
                    .then(|| 1. - (end - now) as f32 / f.remaining_ms as f32),
                bar_start: 0.,
            },
            None => Content {
                title: clock,
                cue: if f.ready { "Ready" } else { "Paused" }.into(),
                meta: session,
                progress: None,
                bar_start: 0.,
            },
        }
    }
    fn calendar_content(&self, now: i64) -> Content {
        let problem = self.snapshot.calendar_problem;
        match self
            .snapshot
            .calendar
            .as_ref()
            .filter(|c| c.ends_at_ms > now)
        {
            Some(c) => Content {
                title: c.title.clone(),
                cue: calendar_cue(c.starts_at_ms, now),
                meta: if problem {
                    "May be out of date".into()
                } else {
                    clock_range(c.starts_at_ms, c.ends_at_ms, self.format_24h)
                },
                progress: (c.starts_at_ms <= now && c.ends_at_ms > c.starts_at_ms).then(|| {
                    (now - c.starts_at_ms) as f32 / (c.ends_at_ms - c.starts_at_ms) as f32
                }),
                bar_start: 0.,
            },
            None if problem => Content {
                title: "Calendar unavailable".into(),
                meta: "Could not refresh your calendar".into(),
                ..Default::default()
            },
            None => Content {
                title: "No upcoming meetings".into(),
                meta: "Timed events appear here".into(),
                ..Default::default()
            },
        }
    }
    fn label_button<'a>(
        &self,
        palette: Palette,
        id: &'static str,
        label: &str,
        tint: Color,
        reveal: f32,
        role: Role,
        press: Option<Message>,
    ) -> Element<'a, Message> {
        let ink = if press.is_some() {
            reveal
        } else {
            0.32 * reveal
        };
        widget::button::custom(
            widget::container(self.text(
                label,
                13.,
                alpha(
                    if role == Role::Primary && press.is_some() {
                        palette.on_accent(tint)
                    } else {
                        palette.ink
                    },
                    ink,
                ),
            ))
                .height(Length::Fill)
                .align_y(cosmic::iced::Alignment::Center),
        )
        .id(id.into())
        .height(Length::Fixed(32.))
        .padding([0, 14])
        .class(capsule_button(palette, tint, reveal, role))
        .on_press_maybe(press)
        .into()
    }
    /// Recent notifications, with how many.
    fn recent_button<'a>(&self, palette: Palette, tint: Color, reveal: f32) -> Element<'a, Message> {
        self.tip(
            palette,
            widget::button::custom(
                widget::row([
                    widget::icon::from_name("document-open-recent-symbolic")
                        .symbolic(true)
                        .size(16)
                        .into(),
                    self.text(self.recent.len().to_string(), 12., alpha(palette.ink, reveal)),
                ])
                .spacing(6)
                .align_y(cosmic::iced::Alignment::Center),
            )
            .id("activity-recent-show".into())
            .height(32)
            .padding([0, 12])
            .class(capsule_button(palette, tint, reveal, Role::Quiet))
            .on_press(Message::ShowRecent),
            "Recent notifications",
            widget::tooltip::Position::Top,
        )
    }
    /// COSMIC's Do Not Disturb, lit while on.
    fn dnd_button<'a>(&self, palette: Palette, tint: Color, reveal: f32) -> Element<'a, Message> {
        let on = self.snapshot.dnd;
        self.tip(
            palette,
            widget::button::icon(
                widget::icon::from_name(MOON).symbolic(true),
            )
            .icon_size(16)
            .padding([8, 14])
            .width(44)
            .height(32)
            .id("activity-dnd".into())
            .class(capsule_button(
                palette,
                tint,
                reveal,
                if on { Role::Primary } else { Role::Quiet },
            ))
            .on_press(Message::Dnd(!on)),
            if on {
                "Turn off Do Not Disturb"
            } else {
                "Do Not Disturb"
            },
            widget::tooltip::Position::Top,
        )
    }
    /// The track's artwork as a disc, or the music glyph on a disc of its
    /// colour. It brings the player forward.
    fn artwork<'a>(
        &self,
        palette: Palette,
        tint: Color,
        reveal: f32,
        key: &str,
    ) -> Element<'a, Message> {
        const SIZE: f32 = surface::ART;
        // The disc carries its own soft edge, so it is drawn unclipped.
        let content: Element<'a, Message> = match &self.art {
            Some((_, handle)) => widget::image(handle.clone())
                .width(SIZE)
                .height(SIZE)
                .content_fit(cosmic::iced::ContentFit::Fill)
                .opacity(reveal)
                .into(),
            None => {
                let fill = alpha(palette.deep(tint), reveal);
                widget::container(
                    widget::icon::from_name(Kind::Music.icon())
                        .symbolic(true)
                        .size(18)
                        .icon()
                        .class(cosmic::theme::Svg::custom(move |_| {
                            cosmic::iced::widget::svg::Style {
                                color: Some(alpha(tint, reveal)),
                            }
                        })),
                )
                .width(SIZE)
                .height(SIZE)
                .align_x(cosmic::iced::Alignment::Center)
                .align_y(cosmic::iced::Alignment::Center)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::widget::container::Style {
                        background: Some(fill.into()),
                        border: cosmic::iced::Border {
                            radius: (SIZE / 2.).into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                }))
                .into()
            }
        };
        self.tip(
            palette,
            widget::button::custom(content)
                .id("activity-artwork".into())
                .width(SIZE)
                .height(SIZE)
                .padding(0)
                .class(capsule_button(palette, tint, reveal, Role::Identity))
                .on_press(Message::Control(Command::Raise { key: key.to_owned() })),
            "Show player",
            widget::tooltip::Position::Top,
        )
    }
    fn actions(&self, palette: Palette, tint: Color, reveal: f32) -> Element<'_, Message> {
        let now = self.now_ms;
        let mut row = widget::row([])
            .spacing(4)
            .align_y(cosmic::iced::Alignment::Center);
        if !self.live() {
            row = row.push(self.label_button(
                palette,
                "activity-settings",
                "Settings",
                tint,
                reveal,
                Role::Quiet,
                Some(Message::Settings("general")),
            ));
            if self.commands.is_none() && !self.starting {
                row = row.push(self.label_button(
                    palette,
                    "activity-start",
                    if self.problem.is_some() {
                        "Try again"
                    } else {
                        "Start Halcyon"
                    },
                    tint,
                    reveal,
                    Role::Primary,
                    Some(Message::Start),
                ));
            }
            return row.into();
        }
        if !self.selected_source_matches() {
            return self.label_button(
                palette,
                "activity-ended",
                "Other activities",
                tint,
                reveal,
                Role::Quiet,
                Some(Message::Choose),
            );
        }
        match self.kind {
            Kind::Idle => {
                if self.notes_shown() && self.has_recent() {
                    row = row.push(self.recent_button(palette, tint, reveal));
                }
                return row
                    .push(self.dnd_button(palette, tint, reveal))
                    .push(self.timer_button(palette, tint, reveal))
                    .push(self.label_button(
                        palette,
                        "activity-preferences",
                        // A Standard bar's narrower card keeps the row on one line.
                        if self.frame().attached {
                            "Settings"
                        } else {
                            "Activity settings"
                        },
                        tint,
                        reveal,
                        Role::Quiet,
                        Some(Message::Settings("panel")),
                    ))
                    .into();
            }
            Kind::System(source) => {
                return self.label_button(
                    palette,
                    "activity-system-settings",
                    match source {
                        system::Source::Updates => "Open updater",
                        system::Source::Power => "Power settings",
                        system::Source::Network => "Network settings",
                        system::Source::Bluetooth => "Bluetooth settings",
                        system::Source::Audio | system::Source::Microphone => "Sound settings",
                        // No camera page in COSMIC: the island's own switch.
                        system::Source::Camera => "Activity settings",
                    },
                    tint,
                    reveal,
                    // A settings link, not an action on the activity itself.
                    Role::Quiet,
                    Some(if source == system::Source::Camera {
                        Message::Settings("panel")
                    } else {
                        Message::SystemSettings(source)
                    }),
                );
            }
            Kind::Notification(_) if self.summary().is_some() => {
                row = row.push(self.label_button(
                    palette,
                    "activity-note-all",
                    "Show all",
                    tint,
                    reveal,
                    Role::Primary,
                    Some(Message::ShowNotes),
                ));
                row = row.push(self.label_button(
                    palette,
                    "activity-note-clear",
                    "Clear all",
                    tint,
                    reveal,
                    Role::Quiet,
                    Some(Message::NotesClear),
                ));
            }
            Kind::Notification(id) => {
                let Some(note) = self.note(id) else {
                    return self.label_button(
                        palette,
                        "activity-ended",
                        "Other activities",
                        tint,
                        reveal,
                        Role::Quiet,
                        Some(Message::Choose),
                    );
                };
                // The card's width decides how many of the app's own buttons
                // fit beside Dismiss; Clear all comes last.
                let mut room = if self.frame().attached {
                    surface::ATTACHED_WIDTH - 24.
                } else {
                    WIDTH - 32.
                };
                let width = |label: &str| label.chars().count() as f32 * 7.5 + 32.;
                room -= width("Dismiss");
                row = row.push(self.label_button(
                    palette,
                    "activity-note-dismiss",
                    "Dismiss",
                    tint,
                    reveal,
                    Role::Quiet,
                    Some(Message::NoteDismiss(id)),
                ));
                for (index, (key, label)) in note.actions.iter().take(2).enumerate() {
                    if width(label) > room {
                        break;
                    }
                    room -= width(label);
                    row = row.push(self.label_button(
                        palette,
                        ["activity-note-action-0", "activity-note-action-1"][index],
                        label,
                        tint,
                        reveal,
                        if index == 0 { Role::Primary } else { Role::Quiet },
                        Some(Message::NoteAction(id, key.clone())),
                    ));
                }
                // The app's rules stay in reach from the keyboard.
                room -= 36.;
                if self.notes.len() > 1 && width("Clear all") <= room {
                    row = row.push(self.label_button(
                        palette,
                        "activity-note-clear",
                        "Clear all",
                        tint,
                        reveal,
                        Role::Quiet,
                        Some(Message::NotesClear),
                    ));
                }
                row = row.push(self.tip(
                    palette,
                    widget::button::icon(widget::icon::from_name("view-more-symbolic").symbolic(true))
                        .id("activity-note-more".into())
                        .icon_size(16)
                        .padding(8)
                        .width(32)
                        .height(32)
                        .class(capsule_button(palette, tint, reveal, Role::Ghost))
                        .on_press(Message::NoteMenu(id)),
                    "More",
                    widget::tooltip::Position::Top,
                ));
            }
            Kind::Music => match &self.snapshot.music {
                Some(m) => {
                    let transport = |name: &'static str,
                                     label: &'static str,
                                     allowed: bool,
                                     role: Role,
                                     command: Command| {
                        let mut button =
                            widget::button::icon(widget::icon::from_name(name).symbolic(true))
                                // Native icon buttons align their content to the left.
                                // Equal insets center the 16px glyph in its 44px target.
                                .icon_size(16)
                                .padding(14)
                                .width(44)
                                .height(44)
                                .id(cosmic::widget::Id::new(format!(
                                    "activity-{}",
                                    label.to_lowercase()
                                )))
                                .class(capsule_button(palette, tint, reveal, role));
                        if allowed && !self.snapshot.music_problem {
                            button = button.on_press(Message::Control(command));
                        }
                        self.tip(palette, button, label, widget::tooltip::Position::Top)
                    };
                    let key = || m.key.clone();
                    // The transport sits centred, the output button at the
                    // end, balanced by an empty cell at the start.
                    row = row
                        .width(Length::Fill)
                        .spacing(0)
                        .push(widget::Space::new().width(40))
                        .push(widget::Space::new().width(Length::Fill))
                        .push(transport(
                            "media-skip-backward-symbolic",
                            "Previous",
                            m.previous,
                            Role::Ghost,
                            Command::Previous { key: key() },
                        ))
                        .push(if m.playing {
                            transport(
                                "media-playback-pause-symbolic",
                                "Pause",
                                m.pause,
                                Role::Transport,
                                Command::Pause { key: key() },
                            )
                        } else {
                            transport(
                                "media-playback-start-symbolic",
                                "Play",
                                m.play,
                                Role::Transport,
                                Command::Play { key: key() },
                            )
                        })
                        .push(transport(
                            "media-skip-forward-symbolic",
                            "Next",
                            m.next,
                            Role::Ghost,
                            Command::Next { key: key() },
                        ))
                        .push(widget::Space::new().width(Length::Fill))
                        .push(self.outputs_button(palette, tint, reveal));
                }
                None => {
                    row = row.push(self.label_button(
                        palette,
                        "activity-music-settings",
                        "Music settings",
                        tint,
                        reveal,
                        Role::Quiet,
                        Some(Message::Settings("music")),
                    ));
                }
            },
            Kind::Focus => {
                let f = &self.snapshot.focus;
                let elapsed = f.running && f.ends_at_ms.is_some_and(|end| end <= now);
                let label = if f.done || elapsed {
                    if f.label == "Focus" {
                        "Start break"
                    } else {
                        "Start focus"
                    }
                } else if f.running {
                    "Pause"
                } else if f.ready {
                    "Start"
                } else {
                    "Resume"
                };
                row = row.push(self.label_button(
                    palette,
                    "activity-focus-action",
                    label,
                    tint,
                    reveal,
                    Role::Primary,
                    (!elapsed).then(|| Message::Control(Command::Focus { key: f.key.clone() })),
                ));
            }
            Kind::Timer => return self.timer_actions(palette, tint, reveal),
            Kind::Transfer => return self.transfer_actions(palette, tint, reveal),
            Kind::Calendar => {
                // Only an upcoming event can be dismissed or joined.
                let event = self
                    .snapshot
                    .calendar
                    .as_ref()
                    .filter(|c| c.ends_at_ms > now);
                let join = event.and_then(|c| c.join.clone());
                let event = event.is_some();
                if event {
                    // Dismiss is the stronger choice unless there is a meeting to join.
                    row = row.push(self.label_button(
                        palette,
                        "activity-dismiss-calendar",
                        "Dismiss",
                        tint,
                        reveal,
                        Role::Quiet,
                        Some(Message::Dismiss),
                    ));
                }
                row = row.push(match join {
                    Some(url) => self.label_button(
                        palette,
                        "activity-open-calendar",
                        "Join",
                        tint,
                        reveal,
                        Role::Primary,
                        Some(Message::Join(url)),
                    ),
                    None => self.label_button(
                        palette,
                        "activity-open-calendar",
                        "Calendar settings",
                        tint,
                        reveal,
                        Role::Quiet,
                        Some(Message::Settings("calendar")),
                    ),
                });
            }
        }
        row.into()
    }
    fn frame(&self) -> host::Frame {
        if self.supported {
            self.panel_frame
        } else {
            host::Frame::default()
        }
    }
    fn capsule(&self, popup: bool) -> Element<'_, Message> {
        let palette = self.palette();
        let p = if popup { self.motion.value } else { 0. };
        let live = self.live();
        let recent_only = live && self.listing() && self.notes.is_empty();
        let (icon, accent) = if recent_only {
            (
                Kind::Notification(0).icon(),
                palette.accent(Kind::Notification(0)),
            )
        } else if live {
            (
                if self.chooser {
                    "view-list-symbolic"
                } else if self.urgent() == Some(system::Source::Power) {
                    "battery-level-10-symbolic"
                } else {
                    self.kind.icon()
                },
                if self.urgent().is_some()
                    || matches!(self.kind, Kind::Notification(id) if self.note(id).is_some_and(|n| n.urgent))
                {
                    palette.warning()
                } else {
                    self.live_accent(palette)
                        .unwrap_or_else(|| palette.accent(self.kind))
                },
            )
        } else {
            ("system-shutdown-symbolic", palette.quiet)
        };
        // Everything beyond the title fades in within the growing outline.
        let reveal = ((p - 0.5) / 0.4).clamp(0., 1.);
        let meta_reveal = ((p - 0.45) / 0.4).clamp(0., 1.);
        // The bottom row waits until the outline has nearly reached it.

        let Content {
            mut title,
            mut cue,
            mut meta,
            progress,
            bar_start,
        } = self.content();
        // Open, a meeting name gets the whole first line; when moves under it.
        if self.kind == Kind::Calendar && p >= 0.5 && !cue.is_empty() && !self.chooser {
            meta = format!("{cue} · {meta}");
            cue.clear();
        }
        let summary = self.summary().is_some();
        let listing = live && self.listing();
        // The timer, the picker or the outputs fill the card below the title.
        let panel = popup && live && self.live_panel();
        if let Some(heading) = self.live_heading(p) {
            title = heading;
            cue.clear();
        }
        // Open on the list, the title line names it.
        let heading = listing && p >= 0.5;
        if heading {
            title = match (&self.menu, self.notes.len()) {
                (Some(target), _) => target.app.clone(),
                (None, 0) => "Recent".into(),
                (None, 1) => "1 notification".into(),
                (None, count) => format!("{count} notifications"),
            };
        }
        // Resting, two or more from one app read as that app and a count.
        if live && p < 0.5 && !self.chooser && self.summary().is_none()
            && let Kind::Notification(id) = self.kind
            && let Some(n) = self.note(id).filter(|n| !n.urgent)
        {
            let count = self.group_ids(n.app_key()).len();
            if count > 1 {
                title = format!("{} · {count}", n.app);
            }
        }
        // A notification in front stands for the others as a count; behind
        // another activity, the newest one stands for them all as its icon.
        let leads = !self.chooser && matches!(self.kind, Kind::Notification(_));
        let mut more_notes = 0;
        let mut others: Vec<Kind> = Vec::new();
        for kind in self.eligible_kinds() {
            if kind == self.kind
                // A camera or microphone on is its dot beside the title, not an icon.
                || (live && matches!(kind, Kind::System(source) if source.privacy()))
            {
                continue;
            }
            if matches!(kind, Kind::Notification(_)) {
                if leads {
                    // A summary or the list already stands for them.
                    if !summary && !heading {
                        more_notes += 1;
                    }
                    continue;
                }
                if others.iter().any(|k| matches!(k, Kind::Notification(_))) {
                    continue;
                }
            }
            others.push(kind);
        }
        let count_badge = usize::from(!self.chooser && more_notes > 0);
        // Up to three places in all; beyond that the last place says how
        // many more and opens the chooser.
        let places = OVERVIEW - count_badge;
        let (shown, more) = if others.len() > places {
            (&others[..places - 1], others.len() - (places - 1))
        } else {
            (&others[..], 0)
        };
        let others_width = if self.chooser && !others.is_empty() {
            BADGE_WIDTH
        } else {
            overview_width(shown.len(), count_badge + usize::from(more > 0))
        };
        // A single badge keeps the resting title centred; a row of icons
        // takes the title's balancing space instead.
        let balance = if others_width <= BADGE_WIDTH { others_width } else { 0. };
        let others = others.len() + count_badge;
        // The cue says when or how far along, so it keeps its full width and
        // the title gives way. This mirrors the title slot in `surface`.
        let compact = COMPACT_WIDTH.min(self.slot_width());
        // A Standard bar keeps the title line at its resting width.
        let attached = self.frame().attached;
        let slot = if attached {
            compact - 24.
        } else {
            compact + (WIDTH - compact) * p - 2. * (12. + 2. * p)
        } - 32. * p
            - balance * (1. - p)
            - others_width
            - if others > 0 { 8. } else { 0. };
        // A lens or microphone on shows as a dot, unless it is the card.
        let privacy: Vec<&system::Activity> = [system::Source::Camera, system::Source::Microphone]
            .into_iter()
            .filter(|source| live && self.kind != Kind::System(*source))
            .filter_map(|source| self.system_activity(source))
            .collect();
        let cue_width = if self.chooser || cue.is_empty() {
            0.
        } else {
            cue.chars().count() as f32 * 7. + 12.
        };
        let dnd = live && self.snapshot.dnd;
        let room = slot
            - 24.
            - privacy.len() as f32 * (DOT + 2. * DOT_REACH)
            - if privacy.is_empty() { 0. } else { 8. }
            - if dnd { 22. } else { 0. };
        // A resting capsule keeps a readable title; the cue waits for the popup.
        // On a Standard bar the title line never widens, so an open card
        // carries the cue in its details instead.
        let tight = room - cue_width < 72.;
        if attached && tight && p >= 0.5 && !cue.is_empty() && !self.chooser {
            meta = format!("{cue} · {meta}");
        }
        let cue = if tight && (p < 0.5 || attached) {
            String::new()
        } else {
            cue
        };
        // Open on a track, the artwork disc leads the card: floating, it
        // takes the glyph's place beside the title; attached, the card
        // repeats the title beside it under the bar.
        let art = popup
            && live
            && !self.chooser
            && !listing
            && !panel
            && self.kind == Kind::Music
            && self.snapshot.music.is_some();
        let art_glyph = art && !attached && p >= 0.5;
        let title_width = (room
            - if cue.is_empty() { 0. } else { cue_width }
            - if art_glyph { (surface::ART - 14.) * reveal } else { 0. })
        .max(40.);
        let live_glyph = (live && !self.chooser && !heading)
            .then(|| self.live_glyph(palette, accent))
            .flatten();
        let glyph = if art_glyph {
            widget::Space::new()
                .width(16. + (surface::ART - 14.) * reveal)
                .into()
        } else if let Some(glyph) = live_glyph {
            glyph
        } else if live
            && !self.chooser
            && !heading
            && self.summary().is_none()
            && matches!(self.kind, Kind::Notification(_))
        {
            self.kind_icon(self.kind, 16)
        } else {
            widget::icon::from_name(icon).symbolic(true).size(16).into()
        };
        let header = (art && attached).then(|| title.clone());
        // A counting timer keeps its figures still.
        let title_text: Element<'_, Message> =
            match live.then(|| self.live_title(palette, p)).flatten() {
                Some(clock) => clock,
                None => widget::text(if self.chooser {
                    "Activities".to_owned()
                } else {
                    title
                })
                .size(14.)
                .font(cosmic::font::semibold())
                .wrapping(cosmic::iced::advanced::text::Wrapping::None)
                .ellipsize(cosmic::iced::advanced::text::Ellipsize::End(
                    cosmic::iced::advanced::text::EllipsizeHeightLimit::Lines(1),
                ))
                .class(cosmic::theme::Text::Color(palette.ink))
                .into(),
            };
        let mut identity = widget::row([])
            .push(glyph)
            .push(
                widget::container(title_text)
                    .width(Length::Shrink)
                    .max_width(title_width),
            )
            .spacing(8)
            .align_y(cosmic::iced::Alignment::Center);
        // The dots sit together, closer than the title's spacing.
        let mut dots = widget::row([]).align_y(cosmic::iced::Alignment::Center);
        let any_dot = !privacy.is_empty();
        for activity in privacy {
            let camera = activity.source == system::Source::Camera;
            let muted = activity.title.ends_with("muted");
            let color = alpha(
                palette.readable(if camera { accent::CAMERA } else { accent::MICROPHONE }),
                if muted { 0.45 } else { 1. },
            );
            let label = if activity.detail.is_empty() {
                activity.title.clone()
            } else {
                format!("{} · {}", activity.title, activity.detail)
            };
            let dot = widget::container(widget::Space::new().width(DOT).height(DOT))
                    .id(cosmic::widget::Id::new(if camera {
                        "activity-privacy-camera"
                    } else {
                        "activity-privacy-microphone"
                    }))
                    .class(cosmic::theme::Container::custom(move |_| {
                        cosmic::widget::container::Style {
                            background: Some(color.into()),
                            border: cosmic::iced::Border {
                                radius: (DOT / 2.).into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    }));
            // The dot is also the way to that activity's card.
            let source = activity.source;
            dots = dots.push(self.tip(
                palette,
                widget::button::custom(dot)
                    .id(cosmic::widget::Id::new(if camera {
                        "activity-privacy-camera-open"
                    } else {
                        "activity-privacy-microphone-open"
                    }))
                    .padding([7., DOT_REACH])
                    .class(capsule_button(palette, palette.ink, 1., Role::Ghost))
                    .on_press(Message::Select(Kind::System(source))),
                &label,
                widget::tooltip::Position::Bottom,
            ));
        }
        if any_dot {
            identity = identity.push(dots);
        }
        if dnd {
            identity = identity.push(self.tip(
                palette,
                widget::container(widget::icon::from_name(MOON).symbolic(true).size(14))
                    .id(cosmic::widget::Id::new("activity-dnd-on")),
                "Do Not Disturb is on",
                widget::tooltip::Position::Bottom,
            ));
        }
        if !self.chooser && !cue.is_empty() {
            identity = identity.push(
                widget::text(cue.clone())
                    .size(12.)
                    .wrapping(cosmic::iced::advanced::text::Wrapping::None)
                    .class(cosmic::theme::Text::Color(accent)),
            );
        }
        if let Some(ring) = live.then(|| self.live_trailing(palette, accent)).flatten() {
            identity = identity.push(ring);
        }
        // Keep the full hit target while moving the measured identity smoothly
        // from the capsule centre to the expanded reading edge.
        let leading = ((1. - p) * 1000.).round() as u16;
        let trailing = ((1. + p) * 1000.).round() as u16;
        let title = widget::button::custom(
            widget::row([
                widget::Space::new()
                    .width(if leading == 0 {
                        Length::Fixed(0.)
                    } else {
                        Length::FillPortion(leading)
                    })
                    .into(),
                identity.into(),
                widget::Space::new()
                    .width(Length::FillPortion(trailing))
                    .into(),
            ])
            .width(Length::Fill)
            .align_y(cosmic::iced::Alignment::Center),
        )
        .id("activity-title".into())
        .padding([4, 0])
        .width(Length::Fill)
        .height(28)
        .class(capsule_button(palette, accent, 1., Role::Identity))
        .on_press(match self.kind {
            // Open, the notification itself is the target, as in a popup.
            Kind::Notification(_) if live && p > 0.9 && self.summary().is_some() => {
                Message::ShowNotes
            }
            Kind::Notification(id)
                if live
                    && !self.chooser
                    && !listing
                    && p > 0.9
                    && self.note(id).is_some_and(|n| n.default) =>
            {
                Message::NoteAction(id, "default".into())
            }
            // Open on the picker or the outputs, the title goes back.
            _ if live && p > 0.9 && self.ui.view != live::View::Card => {
                Message::Live(live::Msg::Back)
            }
            _ => Message::Hold,
        });
        let pin = self.tip(
            palette,
            widget::button::icon(widget::icon::from_name("go-up-symbolic").symbolic(true))
                .id("activity-collapse".into())
                .icon_size(16)
                .padding(6)
                .width(28)
                .height(28)
                .class(capsule_button(palette, accent, reveal, Role::Ghost))
                .on_press(Message::Close),
            "Collapse (Esc)",
            widget::tooltip::Position::Bottom,
        );
        let note = live && !self.chooser && !listing && matches!(self.kind, Kind::Notification(_));
        let meta = if note {
            // A notification's message may take a second line. Clipped, not
            // ellipsized: wrapped ellipsis keeps a space at the line start.
            widget::container(
                widget::text(meta)
                    .size(12.)
                    .line_height(cosmic::iced::widget::text::LineHeight::Absolute(16.into()))
                    .wrapping(cosmic::iced::advanced::text::Wrapping::Word)
                    .class(cosmic::theme::Text::Color(alpha(palette.quiet, meta_reveal))),
            )
            .height(32)
            .clip(true)
            .into()
        } else if live && !self.chooser && matches!(self.kind, Kind::Calendar | Kind::Focus) {
            // Times in equal cells, so a counting cue never shifts.
            live::tabular_text(&meta, 12., alpha(palette.quiet, meta_reveal))
        } else {
            self.text(meta, 12., alpha(palette.quiet, meta_reveal))
        };
        let meta = match header {
            Some(header) => widget::column([
                widget::text(header)
                    .size(14.)
                    .font(cosmic::font::semibold())
                    .wrapping(cosmic::iced::advanced::text::Wrapping::None)
                    .ellipsize(cosmic::iced::advanced::text::Ellipsize::End(
                        cosmic::iced::advanced::text::EllipsizeHeightLimit::Lines(1),
                    ))
                    .class(cosmic::theme::Text::Color(alpha(palette.ink, meta_reveal)))
                    .into(),
                meta,
            ])
            .spacing(1)
            .into(),
            None => meta,
        };
        let badge = |content: Element<'static, Message>, id: &str, width: f32, press: Message| {
            widget::button::custom(
                widget::container(content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(cosmic::iced::Alignment::Center)
                    .align_y(cosmic::iced::Alignment::Center),
            )
            .id(cosmic::widget::Id::new(id.to_owned()))
            .width(width)
            .height(20)
            .padding(0)
            .class(capsule_button(palette, accent, 1., Role::Badge))
            .on_press(press)
        };
        let activities: Element<'_, Message> = if others == 0 {
            widget::Space::new().into()
        } else if self.chooser {
            self.tip(
                palette,
                badge(
                    widget::icon::from_name("go-previous-symbolic")
                        .symbolic(true)
                        .size(14)
                        .into(),
                    "activity-others",
                    BADGE_WIDTH,
                    Message::Choose,
                ),
                "Back to activity",
                widget::tooltip::Position::Bottom,
            )
        } else {
            let mut row = widget::row([]).spacing(OVERVIEW_GAP);
            if count_badge > 0 {
                let label = match more_notes {
                    1 => "1 more notification".to_owned(),
                    count => format!("{count} more notifications"),
                };
                row = row.push(self.tip(
                    palette,
                    badge(
                        self.text(
                            format!("+{more_notes}"),
                            11.,
                            palette.accent(Kind::Notification(0)),
                        ),
                        "activity-note-count",
                        BADGE_WIDTH,
                        Message::ShowNotes,
                    ),
                    &label,
                    widget::tooltip::Position::Bottom,
                ));
            }
            for kind in shown {
                let label = match kind {
                    Kind::Notification(id) => self
                        .note(*id)
                        .map(|n| n.app.clone())
                        .filter(|app| !app.is_empty())
                        .unwrap_or_else(|| "Notification".into()),
                    kind => kind.label().into(),
                };
                // Behind another activity, the newest notification opens the
                // list when there are more.
                let press = if matches!(kind, Kind::Notification(_)) && self.notes.len() > 1 {
                    Message::ShowNotes
                } else {
                    Message::Select(*kind)
                };
                row = row.push(self.tip(
                    palette,
                    badge(
                        self.badge_icon(*kind, palette, 14),
                        &format!("activity-other-{}", kind.id()),
                        ICON_BADGE,
                        press,
                    ),
                    &label,
                    widget::tooltip::Position::Bottom,
                ));
            }
            if more > 0 {
                row = row.push(self.tip(
                    palette,
                    badge(
                        self.text(format!("+{more}"), 11., palette.ink),
                        "activity-others",
                        BADGE_WIDTH,
                        Message::Choose,
                    ),
                    "All activities",
                    widget::tooltip::Position::Bottom,
                ));
            }
            row.into()
        };
        let position = (self.kind == Kind::Music)
            .then(|| self.music_progress())
            .flatten();
        let scrubber = live
            .then(|| self.scrubber(palette, accent, reveal))
            .flatten();
        let tight = position.is_none() && progress.is_none() && !note;
        let seeking = scrubber.is_some();
        let times = if let Some(scrubber) = scrubber {
            scrubber
        } else {
            position
                .map(|(at, total)| {
                    let clock = |ms: i64| format!("{}:{:02}", ms / 60_000, ms / 1000 % 60);
                    widget::row([
                        self.text(clock(at), 10., alpha(palette.quiet, reveal)),
                        widget::Space::new().width(Length::Fill).into(),
                        self.text(clock(total), 10., alpha(palette.quiet, reveal)),
                    ])
                    .into()
                })
                .unwrap_or_else(|| widget::Space::new().into())
        };
        let extra = if popup { self.popup_extra } else { 0. };
        surface::Surface {
            palette,
            frame: self.frame(),
            slot_width: self.slot_width(),
            // The list takes the chooser's place below the title line.
            chooser: self.chooser || listing || panel,
            extra,
            card_extra: if listing {
                self.list_card_extra(extra)
            } else if panel {
                self.live_extra().clamp(0., extra)
            } else {
                0.
            },
            others_width: if others > 0 { others_width } else { 0. },
            balance: if others > 0 { balance } else { 0. },
            wrap_meta: note,
            joined: palette.joins(
                Palette::new(&self.core.applet, preferences::IslandStyle::Theme).surface,
            ),
            art,
            // Hand the title to the popup after its first frame. Restore it at
            // the exact resting frame before destroying the closing popup.
            show_content: popup || self.popup.is_none() || self.motion.value == 0.,
            children: vec![
                title.into(),
                pin,
                meta,
                if self.chooser {
                    self.chooser_view(palette, accent, reveal)
                } else if listing {
                    self.list_view(palette, accent, reveal)
                } else if panel {
                    self.live_view(palette, accent, reveal)
                } else {
                    self.actions(
                        palette,
                        if live {
                            accent
                        } else {
                            palette.soft_ink()
                        },
                        reveal,
                    )
                },
                activities,
                times,
                match self.snapshot.music.as_ref().filter(|_| art) {
                    Some(m) => self.artwork(palette, accent, reveal, &m.key),
                    None => widget::Space::new().into(),
                },
            ],
            progress: p,
            popup,
            animate: popup && self.motion.active(),
            waiting: !popup && self.hover_deadline.is_some(),
            // The scrubber is the music's progress line.
            bar: progress
                .filter(|_| !seeking)
                .map(|fraction| (bar_start, fraction, accent)),
            tight,
        }
        .into()
    }
}
