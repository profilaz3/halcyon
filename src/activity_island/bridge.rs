//! Bounded, owner-only session transport. The desktop remains the provider owner.
use cosmic::iced::{Subscription, futures::SinkExt};
use serde::{Deserialize, Serialize};
use std::{
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::{mpsc, watch},
    task::JoinSet,
};

const LIMIT: u64 = 16_384;
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    /// Fullscreen, locked or not yet known: no automatic reveals at all.
    pub quiet: Option<bool>,
    /// COSMIC's Do Not Disturb. Only critical notifications still reveal.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dnd: bool,
    pub calendar_minute: i64,
    pub preferences: super::preferences::Preferences,
    pub music_source: Option<String>,
    pub timing: Option<MusicTiming>,
    pub system: Vec<super::system::Activity>,
    pub music: Option<Music>,
    pub focus: Focus,
    pub calendar: Option<Calendar>,
    pub calendar_problem: bool,
    pub music_problem: bool,
    pub reduced_motion: bool,
    /// The quick timer. The desktop owns it so `halcyon-ctl` reaches it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timer: Option<super::timer::Timer>,
    /// The player takes a new position (MPRIS CanSeek) and knows its length.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub music_seek: bool,
    /// The artwork's dominant colour, worked out once per track.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub music_tint: Option<[u8; 3]>,
    /// A small square of the artwork, `tint::THUMB` pixels a side, RGB in base64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub music_art: Option<String>,
    /// Connected devices with their battery, at most `DEVICES`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<crate::devices::Device>,
}
/// The most devices a snapshot carries.
pub const DEVICES: usize = 8;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MusicTiming {
    pub key: String,
    pub position_ms: i64,
    pub length_ms: i64,
    pub rate: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Music {
    pub key: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub play: bool,
    pub pause: bool,
    pub next: bool,
    pub previous: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Focus {
    pub key: String,
    pub label: String,
    pub ends_at_ms: Option<i64>,
    pub remaining_ms: i64,
    pub running: bool,
    pub done: bool,
    pub ready: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Calendar {
    pub key: String,
    pub title: String,
    pub starts_at_ms: i64,
    pub ends_at_ms: i64,
    pub all_day: bool,
    /// A checked https meeting link, never sent for private calendars.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join: Option<String>,
    /// The event's calendar colour, the card's accent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "action", rename_all = "snake_case")]
pub enum Command {
    Play { key: String },
    Pause { key: String },
    Next { key: String },
    Previous { key: String },
    /// Bring the player's window forward.
    Raise { key: String },
    Focus { key: String },
    /// Set, or with no rule reset, what the island does with an app's
    /// notifications. Only the app's name and desktop entry cross.
    NoteRule {
        app: String,
        #[serde(default)]
        entry: String,
        #[serde(default)]
        rule: Option<super::preferences::AppRule>,
    },
    /// Move the current track to a position.
    Seek { key: String, position_ms: i64 },
    TimerStart { ms: i64 },
    /// +1 min, or start again once done.
    TimerAdd { ms: i64 },
    TimerPause,
    TimerResume,
    /// Cancel, and Dismiss once done.
    TimerCancel,
}
#[derive(Clone, Debug)]
pub enum Event {
    RefreshCalendar,
    Clients(usize),
    System(Vec<super::system::Activity>),
    Quiet(super::quiet::Quiet),
    Command(Command),
    /// The running timer reached its end.
    TimerDue,
    /// Some time after startup: an island that has not connected by now is
    /// not coming, so nothing should still hold COSMIC's popups.
    Settled,
    /// The grace after the last island left ran out. Carries the departure
    /// it belongs to, so an island that came back in time cancels it.
    Released(u64),
}
#[derive(Clone, Debug)]
pub enum ClientEvent {
    Connected(mpsc::Sender<Command>),
    Snapshot(Snapshot),
    Offline,
}

struct ServerData {
    path: Option<PathBuf>,
    sender: watch::Sender<Snapshot>,
    identity: u64,
}
impl std::hash::Hash for ServerData {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.identity.hash(state);
        self.path.hash(state)
    }
}
pub struct Owner {
    pub preferences: super::preferences::Preferences,
    pub quiet: bool,
    pub dnd: bool,
    pub system: Vec<super::system::Activity>,
    pub clients: usize,
    /// Counts departures of the last island; see `Event::Released`.
    pub departures: u64,
    pub timer: Option<super::timer::Timer>,
    /// Artwork address, its dominant colour and its island thumbnail, as
    /// many as the art cache.
    pub tints: Vec<(String, Option<[u8; 3]>, Option<String>)>,
    sender: watch::Sender<Snapshot>,
    identity: u64,
}
impl Default for Owner {
    fn default() -> Self {
        {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
            Self {
                preferences: Default::default(),
                quiet: true,
                dnd: false,
                system: Vec::new(),
                clients: 0,
                departures: 0,
                timer: None,
                tints: Vec::new(),
                sender: watch::channel(Snapshot::default()).0,
                identity: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            }
        }
    }
}
impl Owner {
    pub fn publish(&self, next: Snapshot) {
        self.sender.send_if_modified(|old| {
            if *old == next {
                false
            } else {
                *old = next;
                true
            }
        });
    }
    pub fn subscription(&self, prefs: super::preferences::Preferences) -> Subscription<Event> {
        let server = Subscription::run_with(
            ServerData {
                sender: self.sender.clone(),
                path: socket_path(),
                identity: self.identity,
            },
            |data| {
                let sender = data.sender.clone();
                let path = data.path.clone();
                cosmic::iced::stream::channel(32, async move |mut out| {
                    if let Some(path) = path {
                        match bind(&path).await {
                            Ok((listener, _guard)) => {
                                let (events, mut incoming) = mpsc::channel(16);
                                let mut clients = JoinSet::new();
                                loop {
                                    tokio::select! {
                                        accepted=listener.accept()=>if let Ok((stream,_))=accepted {
                                            if clients.len()<8 && same_user(&stream) {
                                                clients.spawn(serve(stream,sender.subscribe(),events.clone()));
                                                if out.send(Event::Clients(clients.len())).await.is_err(){break}
                                            }
                                        },
                                        _=clients.join_next(),if !clients.is_empty()=>{if out.send(Event::Clients(clients.len())).await.is_err(){break}},
                                        command=incoming.recv()=>if let Some(command)=command {if out.send(Event::Command(command)).await.is_err(){break}},
                                    }
                                }
                            }
                            Err(error) => {
                                eprintln!("Activity island transport unavailable: {error}")
                            }
                        }
                    }
                    std::future::pending::<()>().await;
                })
            },
        );
        let mut all = vec![server];
        // Runs with or without an island: the chime still matters.
        // Keyed by the wall-clock end, so a redraw never re-arms it; naps
        // are short so a suspend cannot carry it past the end.
        if let Some(ends) = self.timer.and_then(|t| t.ends_at_ms) {
            all.push(Subscription::run_with(ends, |&ends| {
                cosmic::iced::futures::stream::once(async move {
                    loop {
                        let left = ends - chrono::Utc::now().timestamp_millis();
                        if left <= 0 {
                            break Event::TimerDue;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(
                            left.min(20_000) as u64
                        ))
                        .await;
                    }
                })
            }));
        }
        if self.clients > 0 {
            // The shown event changes only when it is two minutes late or
            // ends; the island counts down to it on its own clock. Waking
            // every minute to republish cost a snapshot and a redraw.
            let shown = self
                .sender
                .borrow()
                .calendar
                .as_ref()
                .map(|c| (c.starts_at_ms, c.ends_at_ms));
            if prefs.calendar
                && let Some(shown) = shown
            {
                all.push(Subscription::run_with(shown, |&(starts, ends)| {
                    cosmic::iced::stream::channel(1, async move |mut out| {
                        loop {
                            let now = chrono::Utc::now().timestamp_millis();
                            let Some(next) = calendar_change(starts, ends, now) else {
                                break;
                            };
                            // Naps bound how late a suspend can make it.
                            tokio::time::sleep(std::time::Duration::from_millis(
                                (next - now).min(CALENDAR_NAP_MS) as u64,
                            ))
                            .await;
                            if chrono::Utc::now().timestamp_millis() >= next
                                && out.send(Event::RefreshCalendar).await.is_err()
                            {
                                break;
                            }
                        }
                        std::future::pending::<()>().await;
                    })
                }));
            }
            if prefs.system {
                all.push(
                    super::system::subscription(prefs.camera, prefs.bluetooth)
                        .map(Event::System),
                );
            }
            // Always while an island listens: its moon and Do Not Disturb
            // switch follow this even with reveals off.
            all.push(super::quiet::subscription().map(Event::Quiet));
        }
        Subscription::batch(all)
    }
}
/// The longest wait between checks of the wall clock for a calendar change.
const CALENDAR_NAP_MS: i64 = 300_000;
/// When the desktop next picks a different event to show, given the shown
/// one: two minutes after it starts the next event may take over, and when
/// it ends it goes. Nothing else in the choice depends on the time.
pub(crate) fn calendar_change(starts_at_ms: i64, ends_at_ms: i64, now: i64) -> Option<i64> {
    [starts_at_ms.saturating_add(120_000), ends_at_ms]
        .into_iter()
        .filter(|at| *at > now)
        .min()
}
pub fn socket_path() -> Option<PathBuf> {
    let directory = crate::paths::runtime_dir()?.join("activity-island");
    let name = if std::env::var_os("DESKTOP_CARDS_CONFIG").is_some() {
        use sha2::{Digest, Sha256};
        let path = crate::config::Config::path()?;
        let digest = Sha256::digest(path.as_os_str().as_encoded_bytes());
        let key: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
        format!("preview-{key}-v5.sock")
    } else {
        // v5: per-app notification rules, the quick timer, seeking and
        // the artwork tint. A mismatched applet and desktop wait for each
        // other, never misread.
        "session-v5.sock".into()
    };
    Some(directory.join(name))
}

fn same_user(stream: &UnixStream) -> bool {
    stream
        .peer_cred()
        .is_ok_and(|c| c.uid() == unsafe { libc::geteuid() })
}
struct SocketGuard {
    path: PathBuf,
    inode: u64,
    _lock: std::fs::File,
}
impl Drop for SocketGuard {
    fn drop(&mut self) {
        if std::fs::symlink_metadata(&self.path).is_ok_and(|m| m.ino() == self.inode) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
async fn bind(path: &std::path::Path) -> std::io::Result<(UnixListener, SocketGuard)> {
    use std::os::unix::fs::{DirBuilderExt, FileTypeExt};
    let parent = path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let meta = std::fs::symlink_metadata(parent)?;
    if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o022 != 0 {
        return Err(std::io::ErrorKind::PermissionDenied.into());
    }
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path.with_extension("lock"))?;
    let metadata = lock.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(std::io::ErrorKind::PermissionDenied.into());
    }
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(std::io::ErrorKind::AddrInUse.into());
    }
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if !meta.file_type().is_socket() || meta.uid() != unsafe { libc::geteuid() } {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        // Never unlink another active desktop owner's endpoint.
        match UnixStream::connect(path).await {
            Ok(_) => return Err(std::io::ErrorKind::AddrInUse.into()),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
                ) =>
            {
                std::fs::remove_file(path)?
            }
            Err(e) => return Err(e),
        }
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok((
        listener,
        SocketGuard {
            path: path.into(),
            inode: std::fs::symlink_metadata(path)?.ino(),
            _lock: lock,
        },
    ))
}
async fn line<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    bytes: &mut Vec<u8>,
) -> std::io::Result<()> {
    if bytes.len() > LIMIT as usize {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let count = reader
        .take(LIMIT + 1 - bytes.len() as u64)
        .read_until(b'\n', bytes)
        .await?;
    if count == 0 || bytes.len() > LIMIT as usize || bytes.last() != Some(&b'\n') {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    Ok(())
}
async fn write<W: tokio::io::AsyncWrite + Unpin, T: Serialize>(
    writer: &mut W,
    value: &T,
) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    if bytes.len() >= LIMIT as usize {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    bytes.push(b'\n');
    tokio::time::timeout(std::time::Duration::from_secs(2), writer.write_all(&bytes)).await??;
    Ok(())
}
async fn serve(
    stream: UnixStream,
    mut snapshots: watch::Receiver<Snapshot>,
    events: mpsc::Sender<Command>,
) -> std::io::Result<()> {
    let (read, mut writer) = stream.into_split();
    let mut reader = BufReader::new(read);
    let mut buffer = Vec::new();
    let first = snapshots.borrow_and_update().clone();
    write(&mut writer, &first).await?;
    loop {
        tokio::select! {
            changed=snapshots.changed()=>{if changed.is_err(){return Ok(())}let snapshot=snapshots.borrow_and_update().clone();write(&mut writer,&snapshot).await?},
            complete=line(&mut reader,&mut buffer)=>{
                complete?;let command=serde_json::from_slice::<Command>(&std::mem::take(&mut buffer))?;
                events.send(command).await.map_err(|_|std::io::ErrorKind::BrokenPipe)?;
                // Bound command traffic even from a same-user client.
                tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            },
        }
    }
}
pub fn client() -> Subscription<ClientEvent> {
    Subscription::run_with(socket_path(), |path| {
        let path = path.clone();
        cosmic::iced::stream::channel(16, async move |mut out| {
            if let Some(path) = path {
                loop {
                    if let Ok(stream) = UnixStream::connect(&path).await
                        && same_user(&stream)
                    {
                        let (read, mut writer) = stream.into_split();
                        let mut reader = BufReader::new(read);
                        let mut buffer = Vec::new();
                        let (tx, mut commands) = mpsc::channel(4);
                        if out.send(ClientEvent::Connected(tx)).await.is_err() {
                            break;
                        }
                        loop {
                            tokio::select! {
                                complete=line(&mut reader,&mut buffer)=>match complete.ok().and_then(|_|serde_json::from_slice::<Snapshot>(&std::mem::take(&mut buffer)).ok()) {
                                    Some(snapshot)=>{if out.send(ClientEvent::Snapshot(snapshot)).await.is_err(){break}},None=>break,
                                },
                                command=commands.recv()=>match command {Some(c)=>{if write(&mut writer,&c).await.is_err(){break}},None=>break},
                            }
                        }
                    }
                    if out.send(ClientEvent::Offline).await.is_err() {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
            }
            std::future::pending::<()>().await;
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    /// An event from 10:00 to 11:00, watched from 09:00: the desktop tells
    /// the island twice (10:02, 11:00), where the minute ticker told it 120
    /// times, and checks the clock every five minutes in between.
    #[test]
    fn the_calendar_wakes_only_when_the_shown_event_can_change() {
        const MINUTE: i64 = 60_000;
        let (starts, ends) = (600 * MINUTE, 660 * MINUTE);
        let mut now = 540 * MINUTE;
        let (mut naps, mut sends) = (0, vec![]);
        while let Some(next) = calendar_change(starts, ends, now) {
            now += (next - now).min(CALENDAR_NAP_MS);
            naps += 1;
            if now >= next {
                sends.push(now / MINUTE);
            }
        }
        assert_eq!(sends, [602, 660]);
        assert_eq!(naps, 25, "62 then 58 minutes in five-minute naps");
        assert_eq!(calendar_change(starts, ends, ends), None);
        // An event already late waits only for its end.
        assert_eq!(calendar_change(starts, ends, 630 * MINUTE), Some(ends));
    }
    #[tokio::test]
    async fn partial_frame_survives_subscription_cancellation() {
        let (mut tx, rx) = tokio::io::duplex(512);
        let mut reader = BufReader::new(rx);
        let mut buffer = Vec::new();
        tx.write_all(b"{\"action\":\"focus\",").await.unwrap();
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(10),
                line(&mut reader, &mut buffer)
            )
            .await
            .is_err()
        );
        assert!(!buffer.is_empty());
        tx.write_all(b"\"key\":\"current\"}\n").await.unwrap();
        line(&mut reader, &mut buffer).await.unwrap();
        assert!(
            matches!(serde_json::from_slice::<Command>(&buffer).unwrap(), Command::Focus{key} if key=="current")
        );
    }
    #[tokio::test]
    async fn oversized_frame_is_bounded_and_rejected() {
        let input = vec![b'a'; LIMIT as usize + 512];
        let mut reader = BufReader::new(input.as_slice());
        let mut buffer = Vec::new();
        assert!(line(&mut reader, &mut buffer).await.is_err());
        assert_eq!(buffer.len(), LIMIT as usize + 1);
    }
    #[tokio::test]
    async fn endpoint_is_exclusive_private_and_cleaned_on_drop() {
        let directory = std::env::temp_dir().join(format!(
            "halcyon-island-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o775)).unwrap();
        let path = directory.join("private/activity.sock");
        let (listener, guard) = bind(&path).await.unwrap();
        assert_eq!(
            std::fs::metadata(&directory).unwrap().mode() & 0o777,
            0o775,
            "do not change existing runtime-directory permissions"
        );
        assert_eq!(
            std::fs::metadata(path.parent().unwrap()).unwrap().mode() & 0o777,
            0o700
        );
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        assert!(bind(&path).await.is_err());
        let client = UnixStream::connect(&path).await.unwrap();
        assert!(same_user(&client));
        drop(client);
        drop(listener);
        drop(guard);
        assert!(!path.exists());
        let (listener, guard) = bind(&path).await.unwrap();
        drop(listener);
        drop(guard);
        std::os::unix::fs::symlink("missing", &path).unwrap();
        assert!(bind(&path).await.is_err());
        assert!(
            std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[tokio::test]
    async fn snapshots_do_not_wake_idle_clients_without_a_change() {
        let owner = Owner::default();
        let mut rx = owner.sender.subscribe();
        owner.publish(Snapshot::default());
        assert!(!rx.has_changed().unwrap());
        let mut changed = Snapshot::default();
        changed.focus.key = "test".into();
        owner.publish(changed.clone());
        assert!(rx.has_changed().unwrap());
        rx.borrow_and_update();
        owner.publish(changed);
        assert!(!rx.has_changed().unwrap());
    }
}
