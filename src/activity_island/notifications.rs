//! Desktop notifications from the panel's notification-applet connection.
//!
//! COSMIC's panel hands applets marked `X-NotificationsApplet` one end of a
//! private D-Bus connection to the notification daemon (fd number in
//! `COSMIC_NOTIFICATIONS`). The daemon sends every lasting notification there,
//! even while its own popups are off. Text stays in this process: it is never
//! sent to the Halcyon desktop, written to disk or logged.
use cosmic::iced::{
    Subscription,
    futures::{SinkExt, StreamExt},
};
use std::collections::HashMap;
use std::time::Duration;
use zbus::zvariant::{OwnedValue, Value};

/// Newest first; older notifications fall off the end.
pub const LIMIT: usize = 20;
const PATH: &str = "/com/system76/NotificationsApplet";
const INTERFACE: &str = "com.system76.NotificationsApplet";
const FREEDESKTOP: &str = "org.freedesktop.Notifications";

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub id: u32,
    pub app: String,
    /// An icon name or an absolute image path; empty when the app sent none.
    pub icon: String,
    pub title: String,
    pub body: String,
    /// The app's own buttons as (key, label), without "default".
    pub actions: Vec<(String, String)>,
    /// Clicking the notification itself does something.
    pub default: bool,
    pub urgent: bool,
    /// Low urgency: listed, never revealed.
    pub low: bool,
    /// The app keeps the notification after an action.
    pub resident: bool,
    /// When it arrived here, in Unix milliseconds; 0 until it does.
    pub at_ms: i64,
    /// The app's desktop entry, when it names a plausible one.
    pub entry: String,
    /// How long the app wants it shown, in milliseconds; 0 for as long as
    /// the server keeps it.
    pub timeout_ms: u32,
    /// A copy or download's percentage, from the `value` hint.
    pub progress: Option<u8>,
}
impl Note {
    /// The app this notification groups and is ruled under.
    pub fn app_key(&self) -> &str {
        if self.entry.is_empty() {
            &self.app
        } else {
            &self.entry
        }
    }
}
/// A notification that left the list this session. Kept in memory only.
#[derive(Clone, Debug, PartialEq)]
pub struct Recent {
    pub note: Note,
    /// Still held by the server, so opening it reaches the app.
    pub alive: bool,
    /// The app's desktop file, to open the app once the notification is gone.
    pub app_path: Option<std::path::PathBuf>,
}
#[derive(Clone, Debug)]
pub enum Event {
    /// Whether the panel connection is up.
    Ready(bool),
    Notify(Note),
    /// Dismissed, answered or withdrawn by its app.
    Closed(u32),
    /// Timed out on the server; it may still be there to open.
    Expired(u32),
}

/// The last connections that worked. A failed attempt is never kept, so
/// the next use tries again.
static PANEL: std::sync::Mutex<Option<zbus::Connection>> = std::sync::Mutex::new(None);
static SESSION: std::sync::Mutex<Option<zbus::Connection>> = std::sync::Mutex::new(None);

fn kept(slot: &std::sync::Mutex<Option<zbus::Connection>>) -> Option<zbus::Connection> {
    slot.lock().unwrap_or_else(|e| e.into_inner()).clone()
}
fn keep(slot: &std::sync::Mutex<Option<zbus::Connection>>, connection: Option<zbus::Connection>) {
    *slot.lock().unwrap_or_else(|e| e.into_inner()) = connection;
}

/// Why the panel connection could not be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unreachable {
    /// The panel handed this process no connection: nothing to retry.
    NotHanded,
    /// The connection is there but did not answer: worth trying again.
    Failed,
}

/// A new panel connection. The descriptor is duplicated, so a wrong number
/// can never close something else's file.
async fn connect_panel() -> Result<zbus::Connection, Unreachable> {
    let raw: i32 = std::env::var("COSMIC_NOTIFICATIONS")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .filter(|raw| *raw > 2)
        .ok_or(Unreachable::NotHanded)?;
    // SAFETY: the panel opened this descriptor for us and it stays open for
    // the life of the process; we only borrow it to dup it.
    let fd = unsafe { std::os::fd::BorrowedFd::borrow_raw(raw) }
        .try_clone_to_owned()
        .map_err(|_| Unreachable::NotHanded)?;
    let stream = std::os::unix::net::UnixStream::from(fd);
    // Not a socket: refuse instead of speaking D-Bus to a file.
    stream.peer_addr().map_err(|_| Unreachable::NotHanded)?;
    stream
        .set_nonblocking(true)
        .map_err(|_| Unreachable::Failed)?;
    let stream = tokio::net::UnixStream::from_std(stream).map_err(|_| Unreachable::Failed)?;
    let connection = tokio::time::timeout(
        Duration::from_secs(5),
        zbus::connection::Builder::unix_stream(stream).p2p().build(),
    )
    .await
    .map_err(|_| Unreachable::Failed)?
    .map_err(|_| Unreachable::Failed)?;
    keep(&PANEL, Some(connection.clone()));
    Ok(connection)
}
async fn connect_session() -> Option<zbus::Connection> {
    let connection = zbus::Connection::session().await.ok()?;
    keep(&SESSION, Some(connection.clone()));
    Some(connection)
}
async fn panel() -> Option<zbus::Connection> {
    match kept(&PANEL) {
        Some(connection) => Some(connection),
        None => connect_panel().await.ok(),
    }
}
async fn session() -> Option<zbus::Connection> {
    match kept(&SESSION) {
        Some(connection) => Some(connection),
        None => connect_session().await,
    }
}

/// The waits between tries to reach a connection again: doubling from
/// `first` up to a minute, and short again after a connection that held.
#[derive(Debug, Clone, Copy)]
struct Backoff {
    first: Duration,
    next: Duration,
}
impl Backoff {
    const LONGEST: Duration = Duration::from_secs(60);
    /// A connection up this long counts as having held.
    const HELD: Duration = Duration::from_secs(30);
    fn new(first: Duration) -> Self {
        Self { first, next: first }
    }
    fn wait(&mut self) -> Duration {
        let wait = self.next;
        self.next = (wait * 2).min(Self::LONGEST.max(self.first));
        wait
    }
    fn reset(&mut self) {
        self.next = self.first;
    }
}

pub fn subscription() -> Subscription<Event> {
    if crate::tools::preview() {
        return Subscription::none();
    }
    Subscription::run_with((), |_| {
        cosmic::iced::stream::channel(32, async |mut out| {
            run(
                &mut out,
                connect_panel,
                || async {
                    keep(&SESSION, None);
                    connect_session().await
                },
                Backoff::new(Duration::from_secs(1)),
            )
            .await;
            std::future::pending().await
        })
    })
}

/// Follows the panel connection, and the session bus for closed
/// notifications, reaching each again with backoff when it fails or drops.
/// Returns only when nobody listens any more.
async fn run<P, PF, S, SF>(
    out: &mut cosmic::iced::futures::channel::mpsc::Sender<Event>,
    mut connect_panel: P,
    mut connect_session: S,
    backoff: Backoff,
) where
    P: FnMut() -> PF,
    PF: std::future::Future<Output = Result<zbus::Connection, Unreachable>>,
    S: FnMut() -> SF,
    SF: std::future::Future<Output = Option<zbus::Connection>>,
{
    let mut panel_backoff = backoff;
    // What the island was last told while the panel was out of reach.
    let mut ready = None;
    loop {
        let panel = match connect_panel().await {
            Ok(panel) => panel,
            Err(why) => {
                if ready != Some(false) {
                    ready = Some(false);
                    if out.send(Event::Ready(false)).await.is_err() {
                        return;
                    }
                }
                if why == Unreachable::NotHanded {
                    return std::future::pending().await;
                }
                tokio::time::sleep(panel_backoff.wait()).await;
                continue;
            }
        };
        let mut notify = zbus::MessageStream::from(&panel);
        let mut session_backoff = backoff;
        let mut closed: Option<zbus::MessageStream> = None;
        // Ask for the session bus right away, then after each wait.
        let mut session_at = Some(tokio::time::Instant::now());
        if out.send(Event::Ready(true)).await.is_err() {
            return;
        }
        let up = tokio::time::Instant::now();
        loop {
            let event = tokio::select! {
                message = notify.next() => match message {
                    Some(Ok(message)) => parse(&message).map(Event::Notify),
                    Some(Err(_)) => None,
                    None => break,
                },
                message = async {
                    match closed.as_mut() {
                        Some(stream) => stream.next().await,
                        None => std::future::pending().await,
                    }
                } => match message {
                    Some(Ok(message)) => closed_event(&message),
                    Some(Err(_)) => None,
                    None => {
                        closed = None;
                        session_at = Some(tokio::time::Instant::now() + session_backoff.wait());
                        None
                    }
                },
                () = async {
                    match session_at {
                        Some(at) => tokio::time::sleep_until(at).await,
                        None => std::future::pending().await,
                    }
                } => {
                    closed = match connect_session().await {
                        Some(session) => closed_stream(&session).await,
                        None => None,
                    };
                    session_at = closed
                        .is_none()
                        .then(|| tokio::time::Instant::now() + session_backoff.wait());
                    None
                }
            };
            if let Some(event) = event
                && out.send(event).await.is_err()
            {
                return;
            }
        }
        // The panel connection dropped: say so, and reach it again.
        keep(&PANEL, None);
        ready = Some(false);
        if out.send(Event::Ready(false)).await.is_err() {
            return;
        }
        if up.elapsed() >= Backoff::HELD {
            panel_backoff.reset();
        }
        tokio::time::sleep(panel_backoff.wait()).await;
    }
}
async fn closed_stream(session: &zbus::Connection) -> Option<zbus::MessageStream> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(FREEDESKTOP)
        .ok()?
        .member("NotificationClosed")
        .ok()?
        .path("/org/freedesktop/Notifications")
        .ok()?
        .build();
    zbus::MessageStream::for_match_rule(rule, session, Some(32))
        .await
        .ok()
}
fn closed_event(message: &zbus::Message) -> Option<Event> {
    let (id, reason): (u32, u32) = message.body().deserialize().ok()?;
    // 1 is expiry: the popup timed out, the notification may still stand.
    Some(if reason == 1 {
        Event::Expired(id)
    } else {
        Event::Closed(id)
    })
}
type Notify = (
    String,
    u32,
    String,
    String,
    String,
    Vec<String>,
    HashMap<String, OwnedValue>,
    i32,
);
fn parse(message: &zbus::Message) -> Option<Note> {
    let header = message.header();
    if message.message_type() != zbus::message::Type::Signal
        || header.interface()?.as_str() != INTERFACE
        || header.member()?.as_str() != "Notify"
    {
        return None;
    }
    let (app, id, icon, summary, body, actions, hints, timeout): Notify =
        message.body().deserialize().ok()?;
    let mut note = note(id, &app, &icon, &summary, &body, &actions, &hints);
    // At most a day: a longer wish is as good as never.
    note.timeout_ms = u32::try_from(timeout).unwrap_or(0).min(86_400_000);
    Some(note)
}
pub(super) fn note(
    id: u32,
    app: &str,
    icon: &str,
    summary: &str,
    body: &str,
    actions: &[String],
    hints: &HashMap<String, OwnedValue>,
) -> Note {
    let hint = |name: &str| hints.get(name).map(|v| &**v);
    let urgent = matches!(hint("urgency"), Some(Value::U8(2)));
    let low = matches!(hint("urgency"), Some(Value::U8(0)));
    let resident = matches!(hint("resident"), Some(Value::Bool(true)));
    let entry = match hint("desktop-entry") {
        Some(Value::Str(s)) => s.as_str(),
        _ => "",
    };
    let icon = [icon, entry]
        .into_iter()
        .map(|s| s.strip_prefix("file://").unwrap_or(s))
        .find(|s| !s.is_empty() && s.len() <= 512 && !s.chars().any(char::is_control))
        .unwrap_or_default()
        .to_owned();
    let mut default = false;
    let mut buttons = Vec::new();
    for pair in actions.chunks_exact(2) {
        if pair[0] == "default" {
            default = true;
        } else if buttons.len() < 3 && pair[0].len() <= 256 {
            let label = clean(&pair[1], 24);
            if !label.is_empty() {
                buttons.push((pair[0].clone(), label));
            }
        }
    }
    let app = clean(app, 48);
    Note {
        id,
        title: match clean(summary, 160) {
            title if title.is_empty() => app.clone(),
            title => title,
        },
        app,
        icon,
        body: clean(body, 400),
        actions: buttons,
        default,
        urgent,
        low,
        resident,
        at_ms: 0,
        entry: desktop_entry(entry),
        timeout_ms: 0,
        progress: super::transfers::note_progress(hint("value")),
    }
}
/// A desktop entry ID safe to look up as a file name, or empty.
pub(super) fn desktop_entry(entry: &str) -> String {
    let entry = entry.strip_suffix(".desktop").unwrap_or(entry);
    if entry.is_empty()
        || entry.len() > 96
        || entry.starts_with('.')
        || !entry
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return String::new();
    }
    entry.to_owned()
}
/// Plain, single-spaced text: notification bodies may carry simple markup.
pub(super) fn clean(text: &str, limit: usize) -> String {
    let mut plain = String::with_capacity(text.len().min(limit * 4));
    let mut tag = false;
    for c in text.chars() {
        match c {
            '<' => tag = true,
            '>' if tag => tag = false,
            _ if tag => {}
            c => plain.push(c),
        }
    }
    let plain = plain
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    let mut out = String::new();
    for word in plain.split(|c: char| c.is_whitespace() || c.is_control()) {
        if word.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.chars().count() > limit {
        out = out
            .chars()
            .take(limit - 1)
            .collect::<String>()
            .trim_end()
            .to_owned()
            + "…";
    }
    out
}
/// Runs one of the notification's own actions ("default" for the
/// notification itself). The daemon passes the app an activation token.
pub async fn invoke(id: u32, action: String) -> Result<(), String> {
    let panel = panel().await.ok_or("Notifications are unavailable")?;
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        panel.call_method(
            None::<&str>,
            PATH,
            Some(INTERFACE),
            "InvokeAction",
            &(id, action.as_str()),
        ),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .map(|_| ())
    .ok_or_else(|| "Could not open the notification".into())
}
/// The installed desktop file for `entry`, first in XDG precedence.
pub(super) fn entry_path(entry: &str) -> Option<std::path::PathBuf> {
    if desktop_entry(entry) != entry {
        return None;
    }
    crate::desktop::applets::application_dirs(dirs::data_dir(), std::env::var("XDG_DATA_DIRS").ok())
        .into_iter()
        .map(|dir| dir.join(format!("{entry}.desktop")))
        .find(|path| path.is_file())
}
/// Starts an app by its desktop file, the way a launcher would.
pub async fn open_app(path: std::path::PathBuf) -> Result<(), String> {
    let failed = || "Could not open the app".to_owned();
    let status = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        super::renderer::command("gio")
            .arg("launch")
            .arg(&path)
            .kill_on_drop(true)
            .status(),
    )
    .await;
    match status {
        Ok(Ok(status)) if status.success() => Ok(()),
        _ => {
            let entry = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(failed)?;
            super::renderer::command("gtk-launch")
                .arg(entry)
                .spawn()
                .map(|_| ())
                .map_err(|_| failed())
        }
    }
}
/// Closes notifications as if dismissed from a popup; their apps hear it.
pub async fn dismiss(ids: Vec<u32>) -> Result<(), String> {
    let session = session().await.ok_or("Notifications are unavailable")?;
    let mut failed = false;
    for id in ids {
        failed |= tokio::time::timeout(
            std::time::Duration::from_secs(2),
            session.call_method(
                Some(FREEDESKTOP),
                "/org/freedesktop/Notifications",
                Some(FREEDESKTOP),
                "CloseNotification",
                &(id,),
            ),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .is_none();
    }
    if failed {
        Err("Could not dismiss the notification".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// A connected pair as the panel hands over: the daemon's end and ours.
    async fn handed() -> (zbus::Connection, zbus::Connection) {
        let (daemon, ours) = tokio::net::UnixStream::pair().unwrap();
        let guid = zbus::Guid::generate();
        let (daemon, ours) = tokio::join!(
            zbus::connection::Builder::unix_stream(daemon)
                .server(guid)
                .unwrap()
                .p2p()
                .build(),
            zbus::connection::Builder::unix_stream(ours).p2p().build(),
        );
        (daemon.unwrap(), ours.unwrap())
    }
    async fn notify(daemon: &zbus::Connection, id: u32, title: &str) {
        let hints: HashMap<String, OwnedValue> = HashMap::new();
        daemon
            .emit_signal(
                None::<()>,
                PATH,
                INTERFACE,
                "Notify",
                &("app", id, "", title, "", Vec::<String>::new(), hints, 0_i32),
            )
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn d7_the_notification_connection_comes_back_after_failing_or_dropping() {
        use cosmic::iced::futures::channel::mpsc;
        let (first, ours) = handed().await;
        let (second, ours_again) = handed().await;
        let mut handouts = vec![
            Ok(ours_again),
            Ok(ours),
            Err(Unreachable::Failed),
            Err(Unreachable::Failed),
        ];
        let sessions = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let asked = sessions.clone();
        let (mut out, mut events) = mpsc::channel(32);
        let runner = tokio::spawn(async move {
            run(
                &mut out,
                move || {
                    let next = handouts.pop().unwrap_or(Err(Unreachable::Failed));
                    async move { next }
                },
                move || {
                    asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    async { None }
                },
                Backoff::new(Duration::from_millis(5)),
            )
            .await;
        });
        let next = async |events: &mut mpsc::Receiver<Event>| {
            tokio::time::timeout(Duration::from_secs(5), events.next())
                .await
                .expect("an event in time")
                .unwrap()
        };
        // Two failed tries say "not ready" once, then the connection is up.
        assert!(matches!(next(&mut events).await, Event::Ready(false)));
        assert!(matches!(next(&mut events).await, Event::Ready(true)));
        notify(&first, 1, "Build finished").await;
        assert!(matches!(next(&mut events).await, Event::Notify(n) if n.title == "Build finished"));
        // The daemon's end goes away: not ready, then a new connection.
        drop(first);
        assert!(matches!(next(&mut events).await, Event::Ready(false)));
        assert!(matches!(next(&mut events).await, Event::Ready(true)));
        notify(&second, 2, "Still here").await;
        assert!(matches!(next(&mut events).await, Event::Notify(n) if n.title == "Still here"));
        // The session bus was asked for again after failing, with waits.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let tries = sessions.load(std::sync::atomic::Ordering::SeqCst);
        assert!(tries >= 3, "session asked {tries} times");
        assert!(tries < 40, "session asked {tries} times: no backoff");
        // Nobody listens any more: it stops.
        drop(events);
        drop(second);
        tokio::time::timeout(Duration::from_secs(5), runner)
            .await
            .expect("stops when nobody listens")
            .unwrap();
    }
    #[test]
    fn d7_backoff_doubles_to_a_minute_and_starts_over() {
        let mut backoff = Backoff::new(Duration::from_secs(1));
        let waits: Vec<_> = (0..9).map(|_| backoff.wait().as_secs()).collect();
        assert_eq!(waits, [1, 2, 4, 8, 16, 32, 60, 60, 60]);
        backoff.reset();
        assert_eq!(backoff.wait(), Duration::from_secs(1));
    }
    #[test]
    fn markup_is_flattened_and_bounded() {
        assert_eq!(
            clean(
                "<b>Build</b> &amp; deploy\n  finished <a href=\"x\">log</a>",
                80
            ),
            "Build & deploy finished log"
        );
        assert_eq!(clean("abcdefghij", 5), "abcd…");
        assert_eq!(clean("\u{7}bell\tand\u{0}null", 40), "bell and null");
    }
    #[test]
    fn hints_and_actions_become_a_note() {
        let mut hints = HashMap::new();
        hints.insert("urgency".into(), OwnedValue::from(2u8));
        hints.insert(
            "desktop-entry".into(),
            OwnedValue::try_from(Value::from("org.gnome.Calendar")).unwrap(),
        );
        let actions = [
            "default",
            "Open",
            "reply",
            "Reply",
            "mark",
            "Mark as read",
            "x",
            "Four",
        ]
        .map(String::from);
        let n = note(
            7,
            "Calendar",
            "",
            "",
            "Stand-up <i>now</i>",
            &actions,
            &hints,
        );
        assert!(n.urgent && n.default && !n.resident);
        assert_eq!(n.title, "Calendar");
        assert_eq!(n.icon, "org.gnome.Calendar");
        assert_eq!(
            (n.entry.as_str(), n.app_key()),
            ("org.gnome.Calendar", "org.gnome.Calendar")
        );
        assert_eq!(n.body, "Stand-up now");
        assert_eq!(
            n.actions,
            vec![
                ("reply".into(), "Reply".into()),
                ("mark".into(), "Mark as read".into()),
                ("x".into(), "Four".into())
            ]
        );
        let n = note(
            8,
            "Files",
            "file:///tmp/a.png",
            "Copied",
            "",
            &[],
            &HashMap::new(),
        );
        assert_eq!(n.icon, "/tmp/a.png");
        assert!(!n.urgent && !n.low && !n.default);
        let mut hints = HashMap::new();
        hints.insert("urgency".into(), OwnedValue::from(0u8));
        let n = note(9, "Sync", "", "Synced", "", &[], &hints);
        assert!(n.low && !n.urgent);
        assert_eq!(n.app_key(), "Sync");
        // An entry is only ever a plain ID, never a path.
        for bad in ["../../bin/sh", "/usr/share/x", ".hidden", "a b", "x;y"] {
            assert_eq!(desktop_entry(bad), "", "{bad}");
        }
        assert_eq!(desktop_entry("org.Slack.desktop"), "org.Slack");
    }
}
