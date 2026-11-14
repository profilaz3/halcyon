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
use tokio::sync::OnceCell;
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

static PANEL: OnceCell<Option<zbus::Connection>> = OnceCell::const_new();
static SESSION: OnceCell<Option<zbus::Connection>> = OnceCell::const_new();

/// The panel connection, set up once per process. The descriptor is
/// duplicated, so a wrong number can never close something else's file.
async fn panel() -> Option<zbus::Connection> {
    PANEL
        .get_or_init(|| async {
            let raw: i32 = std::env::var("COSMIC_NOTIFICATIONS").ok()?.parse().ok()?;
            if raw <= 2 {
                return None;
            }
            // SAFETY: the panel opened this descriptor for us and it stays
            // open for the life of the process; we only borrow it to dup it.
            let fd = unsafe { std::os::fd::BorrowedFd::borrow_raw(raw) }
                .try_clone_to_owned()
                .ok()?;
            let stream = std::os::unix::net::UnixStream::from(fd);
            // Not a socket: refuse instead of speaking D-Bus to a file.
            stream.peer_addr().ok()?;
            stream.set_nonblocking(true).ok()?;
            let stream = tokio::net::UnixStream::from_std(stream).ok()?;
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                zbus::connection::Builder::unix_stream(stream).p2p().build(),
            )
            .await
            .ok()?
            .ok()
        })
        .await
        .clone()
}
async fn session() -> Option<zbus::Connection> {
    SESSION
        .get_or_init(|| async { zbus::Connection::session().await.ok() })
        .await
        .clone()
}

pub fn subscription() -> Subscription<Event> {
    if crate::tools::preview() {
        return Subscription::none();
    }
    Subscription::run_with((), |_| {
        cosmic::iced::stream::channel(32, async |mut out| {
            let Some(panel) = panel().await else {
                let _ = out.send(Event::Ready(false)).await;
                return std::future::pending().await;
            };
            let mut notify = zbus::MessageStream::from(&panel);
            let mut closed = match session().await {
                Some(session) => closed_stream(&session).await,
                None => None,
            };
            if out.send(Event::Ready(true)).await.is_err() {
                return;
            }
            loop {
                let event = tokio::select! {
                    message = notify.next() => match message {
                        Some(Ok(message)) => parse(&message).map(Event::Notify),
                        Some(Err(_)) => None,
                        None => {
                            let _ = out.send(Event::Ready(false)).await;
                            break;
                        }
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
                            None
                        }
                    },
                };
                if let Some(event) = event
                    && out.send(event).await.is_err()
                {
                    break;
                }
            }
            std::future::pending().await
        })
    })
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
        out = out.chars().take(limit - 1).collect::<String>().trim_end().to_owned() + "…";
    }
    out
}
/// Runs one of the notification's own actions ("default" for the
/// notification itself). The daemon passes the app an activation token.
pub async fn invoke(id: u32, action: String) -> Result<(), String> {
    let panel = panel().await.ok_or("Notifications are unavailable")?;
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        panel.call_method(None::<&str>, PATH, Some(INTERFACE), "InvokeAction", &(id, action.as_str())),
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
    #[test]
    fn markup_is_flattened_and_bounded() {
        assert_eq!(
            clean("<b>Build</b> &amp; deploy\n  finished <a href=\"x\">log</a>", 80),
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
        let actions = ["default", "Open", "reply", "Reply", "mark", "Mark as read", "x", "Four"]
            .map(String::from);
        let n = note(7, "Calendar", "", "", "Stand-up <i>now</i>", &actions, &hints);
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
        let n = note(8, "Files", "file:///tmp/a.png", "Copied", "", &[], &HashMap::new());
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
