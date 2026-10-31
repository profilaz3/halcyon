//! Music controls through MPRIS, the desktop's media player interface.
//!
//! Players announce themselves on the session bus as
//! `org.mpris.MediaPlayer2.<name>`. While the Music card is shown, a watcher
//! follows their names and property signals and sends a fresh list after
//! each change (and a quiet resync every 15 seconds while something plays,
//! because positions are not signalled). Nothing plays, pauses or opens
//! unless the user presses a control; a missing bus or player is shown as
//! such rather than as an error.
use cosmic::iced::Subscription;
use cosmic::iced::futures::{FutureExt, SinkExt, StreamExt};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use zbus::zvariant::{OwnedValue, Value};

#[cfg(test)]
pub(crate) mod fixture;

pub const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT: &str = "org.mpris.MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const CALL_TIMEOUT: Duration = Duration::from_secs(2);
/// Artwork larger than this is not shown.
const MAX_ART_BYTES: usize = 8 * 1024 * 1024;
/// Artwork is kept as a small square thumbnail.
pub const ART_SIZE: u32 = 144;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Playback {
    Playing,
    Paused,
    #[default]
    Stopped,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    /// The full bus name, `org.mpris.MediaPlayer2.spotify`.
    pub bus: String,
    /// What follows the prefix, `spotify` or `firefox.instance_1_84`; the
    /// preference stores its `app_key`.
    pub key: String,
    /// Unique DBus owner; timing never survives a player restart.
    pub owner: Option<String>,
    /// This sample came from an explicit seek notification, including zero.
    pub seeked: bool,
    /// The player's own name, `Spotify`.
    pub identity: String,
    pub status: Playback,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub art_url: Option<String>,
    pub media_url: Option<String>,
    pub track_id: Option<String>,
    pub can_seek: bool,
    pub length_us: Option<i64>,
    pub position_us: Option<i64>,
    pub rate: f64,
    /// When `position_us` was read, for a smooth progress bar in between.
    pub sampled: Instant,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_control: bool,
    pub can_raise: bool,
}

impl Player {
    /// The playback position now, estimated from the last reading.
    pub fn position_at(&self, now: Instant) -> Option<i64> {
        let position = self.position_us?;
        let moved = if self.status == Playback::Playing {
            (now.saturating_duration_since(self.sampled).as_micros() as f64 * self.rate.max(0.))
                as i64
        } else {
            0
        };
        let at = position.saturating_add(moved).max(0);
        Some(
            self.length_us
                .filter(|length| *length > 0)
                .map_or(at, |length| at.min(length)),
        )
    }
    pub fn has_track(&self) -> bool {
        !self.title.is_empty()
    }
    /// A site label only when the player supplies its media URL.
    pub fn site(&self) -> Option<&'static str> {
        let url = reqwest::Url::parse(self.media_url.as_deref()?).ok()?;
        let host = url.host_str()?;
        let belongs = |domain: &str| {
            host == domain
                || host
                    .strip_suffix(domain)
                    .is_some_and(|prefix| prefix.ends_with('.'))
        };
        if host == "music.youtube.com" {
            Some("YouTube Music")
        } else if belongs("youtube.com") || host == "youtu.be" {
            Some("YouTube")
        } else if belongs("soundcloud.com") {
            Some("SoundCloud")
        } else if host == "open.spotify.com" {
            Some("Spotify")
        } else {
            None
        }
    }
    pub fn seek_request(&self, fraction: f32) -> Option<SeekRequest> {
        if !self.can_seek || !self.can_control || !fraction.is_finite() {
            return None;
        }
        let length = self.length_us.filter(|n| *n > 0)?;
        Some(SeekRequest {
            bus: self.bus.clone(),
            owner: self.owner.clone(),
            length_us: length,
            track_id: self.track_id.clone()?,
            title: self.title.clone(),
            media_url: self.media_url.clone(),
            position_us: ((fraction.clamp(0., 1.) as f64 * length as f64) as i64)
                .min(length.saturating_sub(1)),
        })
    }
    /// Advance known elapsed time, including while a browser has no duration.
    pub fn progress_delay(&self, now: Instant) -> Option<Duration> {
        if self.status != Playback::Playing {
            return None;
        }
        let position = self.position_at(now)?;
        if self.length_us.is_some_and(|length| position >= length) {
            return None;
        }
        let until = (1_000_000 - position % 1_000_000) as f64 / self.rate.max(0.1);
        Some(Duration::from_micros(until as u64).max(Duration::from_millis(50)))
    }
    /// The app this player belongs to, the same across its restarts.
    pub fn app(&self) -> &str {
        app_key(&self.key)
    }
    /// A short name for the player: its identity, else its bus key.
    pub fn name(&self) -> &str {
        if self.identity.is_empty() {
            &self.key
        } else {
            &self.identity
        }
    }
}

/// What the watcher reports.
#[derive(Clone, Debug, PartialEq)]
pub enum Feed {
    Players(Vec<Player>),
    /// The session bus can't be reached.
    Unavailable(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Play,
    Pause,
    Next,
    Previous,
    /// Bring the player's window forward.
    Raise,
}

/// The destination and intent the user saw, independent of later auto-follow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportRequest {
    pub bus: String,
    pub command: Command,
}

/// Captured with the visible track, including metadata for browsers that reuse IDs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeekRequest {
    pub bus: String,
    owner: Option<String>,
    length_us: i64,
    pub track_id: String,
    pub position_us: i64,
    title: String,
    media_url: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Control {
    Transport(TransportRequest),
    Seek(SeekRequest),
}
/// Query after a control as well as listening for signals: browsers need not
/// emit Position changes, and some players don't emit transport changes promptly.
pub async fn perform(request: Control) -> Result<Vec<Player>, String> {
    let connection = connection().await?;
    let (bus, done) = match request {
        Control::Transport(request) => (
            request.bus.clone(),
            control(&connection, &request.bus, request.command).await,
        ),
        Control::Seek(request) => (request.bus.clone(), seek_to(&connection, &request).await),
    };
    if let Err(error) = done {
        // The player may have quit or restarted under a new name: the card
        // takes the live players now, so the next click reaches one,
        // instead of waiting for the next periodic resync.
        return match snapshot(&connection).await {
            Ok(players) if !players.iter().any(|player| player.bus == bus) => Ok(players),
            _ => Err(error),
        };
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    snapshot(&connection).await
}

/// The part of a player's bus key that names its app. MPRIS lets a player
/// add a per-process instance (`firefox.instance_1_84`,
/// `chromium.instance4021`, `vlc.instance7389`), which changes every time
/// the app starts; Follow stores and matches what comes before it.
pub fn app_key(key: &str) -> &str {
    for (at, _) in key.match_indices(".instance") {
        let rest = &key[at + ".instance".len()..];
        if rest.is_empty() || rest.starts_with(|c: char| c.is_ascii_digit() || c == '_') {
            return &key[..at];
        }
    }
    key
}

/// The player the card follows: one the user picked on the card, else the
/// preferred app when it is running, else the one most recently playing,
/// else the most recently active, else the first.
pub fn choose<'a>(
    players: &'a [Player],
    pinned: Option<&str>,
    preferred: Option<&str>,
    recent: &[String],
) -> Option<&'a Player> {
    if let Some(player) = pinned.and_then(|bus| players.iter().find(|p| p.bus == bus)) {
        return Some(player);
    }
    let rank = |player: &Player| {
        recent
            .iter()
            .position(|bus| *bus == player.bus)
            .unwrap_or(usize::MAX)
    };
    let best = |players: &[&'a Player]| -> Option<&'a Player> {
        let by = |status: Playback| {
            players
                .iter()
                .copied()
                .filter(|player| player.status == status)
                .min_by_key(|player| rank(player))
        };
        by(Playback::Playing)
            .or_else(|| by(Playback::Paused))
            .or_else(|| {
                players
                    .iter()
                    .copied()
                    .filter(|p| p.has_track())
                    .min_by_key(|p| rank(p))
            })
            .or_else(|| players.iter().copied().min_by_key(|p| rank(p)))
    };
    // A saved per-process key (older preferences) still names its app.
    if let Some(app) = preferred.map(app_key) {
        let followed: Vec<_> = players.iter().filter(|p| p.app() == app).collect();
        if let Some(player) = best(&followed) {
            return Some(player);
        }
    }
    best(&players.iter().collect::<Vec<_>>())
}

/// Moves players that just started playing to the front of `recent`, and
/// forgets players that are gone.
pub fn remember(recent: &mut Vec<String>, before: &[Player], now: &[Player]) {
    for player in now {
        let was_playing = before
            .iter()
            .any(|old| old.bus == player.bus && old.status == Playback::Playing);
        if player.status == Playback::Playing && (!was_playing || !recent.contains(&player.bus)) {
            recent.retain(|bus| *bus != player.bus);
            recent.insert(0, player.bus.clone());
        } else if !recent.contains(&player.bus) {
            recent.push(player.bus.clone());
        }
    }
    recent.retain(|bus| now.iter().any(|player| player.bus == *bus));
}

/// `m:ss` (or `h:mm:ss`) for microseconds.
pub fn duration(us: i64) -> String {
    let seconds = us.max(0) / 1_000_000;
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn unwrap<'a>(value: &'a Value<'a>) -> &'a Value<'a> {
    match value {
        Value::Value(inner) => unwrap(inner),
        other => other,
    }
}
fn text(value: &Value<'_>) -> Option<String> {
    match unwrap(value) {
        Value::Str(text) => Some(text.as_str().to_string()),
        Value::ObjectPath(path) => Some(path.as_str().to_string()),
        _ => None,
    }
}
fn texts(value: &Value<'_>) -> Vec<String> {
    match unwrap(value) {
        Value::Array(items) => items.inner().iter().filter_map(text).collect(),
        other => text(other).into_iter().collect(),
    }
}
fn integer(value: &Value<'_>) -> Option<i64> {
    match unwrap(value) {
        Value::I64(n) => Some(*n),
        Value::U64(n) => i64::try_from(*n).ok(),
        Value::I32(n) => Some(i64::from(*n)),
        Value::U32(n) => Some(i64::from(*n)),
        Value::F64(n) if n.is_finite() => Some(*n as i64),
        _ => None,
    }
}
fn float(value: &Value<'_>) -> Option<f64> {
    match unwrap(value) {
        Value::F64(n) if n.is_finite() => Some(*n),
        other => integer(other).map(|n| n as f64),
    }
}
fn flag(value: &Value<'_>) -> Option<bool> {
    match unwrap(value) {
        Value::Bool(value) => Some(*value),
        _ => None,
    }
}

/// A player from its bus name and the two interfaces' properties. Missing
/// or odd values fall back quietly, as players vary a lot.
pub fn parse_player(
    bus: &str,
    root: &HashMap<String, OwnedValue>,
    props: &HashMap<String, OwnedValue>,
    sampled: Instant,
) -> Player {
    let get = |key: &str| props.get(key).map(|value| &**value);
    let status = match get("PlaybackStatus").and_then(text).as_deref() {
        Some("Playing") => Playback::Playing,
        Some("Paused") => Playback::Paused,
        _ => Playback::Stopped,
    };
    let mut title = String::new();
    let mut artists = Vec::new();
    let mut album = String::new();
    let mut art_url = None;
    let mut media_url = None;
    let mut track_id = None;
    let mut length_us = None;
    if let Some(Value::Dict(metadata)) = get("Metadata").map(unwrap) {
        for (key, value) in metadata.iter() {
            match text(key).as_deref() {
                Some("xesam:title") => title = text(value).unwrap_or_default(),
                Some("xesam:artist") => artists = texts(value),
                Some("xesam:album") => album = text(value).unwrap_or_default(),
                Some("mpris:artUrl") => art_url = text(value).filter(|url| !url.is_empty()),
                Some("xesam:url") => {
                    media_url = text(value).filter(|url| {
                        reqwest::Url::parse(url)
                            .is_ok_and(|u| matches!(u.scheme(), "https" | "http"))
                    })
                }
                Some("mpris:trackid") => track_id = text(value).filter(|id| valid_track_id(id)),
                Some("mpris:length") => length_us = integer(value).filter(|n| *n > 0),
                _ => {}
            }
        }
    }
    let can_control = get("CanControl").and_then(flag).unwrap_or(true);
    let key = bus.strip_prefix(PREFIX).unwrap_or(bus).to_string();
    Player {
        bus: bus.into(),
        identity: root
            .get("Identity")
            .and_then(|value| text(value))
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| fallback_name(&key)),
        key,
        owner: None,
        seeked: false,
        status,
        title: title.trim().to_string(),
        artists: artists
            .into_iter()
            .map(|artist| artist.trim().to_string())
            .filter(|artist| !artist.is_empty())
            .collect(),
        album: album.trim().to_string(),
        art_url,
        media_url,
        track_id,
        can_seek: can_control && get("CanSeek").and_then(flag).unwrap_or(false),
        length_us,
        position_us: get("Position").and_then(integer).filter(|n| *n >= 0),
        rate: get("Rate")
            .and_then(float)
            .filter(|rate| *rate > 0.)
            .unwrap_or(1.),
        sampled,
        can_play: can_control && get("CanPlay").and_then(flag).unwrap_or(false),
        can_pause: can_control && get("CanPause").and_then(flag).unwrap_or(false),
        can_next: can_control && get("CanGoNext").and_then(flag).unwrap_or(false),
        can_previous: can_control && get("CanGoPrevious").and_then(flag).unwrap_or(false),
        can_control,
        can_raise: root
            .get("CanRaise")
            .and_then(|value| flag(value))
            .unwrap_or(false),
    }
}

/// `chromium.instance1234` → `Chromium`.
pub fn fallback_name(key: &str) -> String {
    let first = key.split('.').next().unwrap_or(key);
    let mut chars = first.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => key.to_string(),
    }
}

static CONNECTION: tokio::sync::OnceCell<zbus::Connection> = tokio::sync::OnceCell::const_new();

/// The session bus connection, made once.
pub async fn connection() -> Result<zbus::Connection, String> {
    CONNECTION
        .get_or_try_init(|| async {
            tokio::time::timeout(CALL_TIMEOUT, zbus::Connection::session())
                .await
                .map_err(|_| "The session bus did not answer".to_string())?
                .map_err(|error| error.to_string())
        })
        .await
        .cloned()
}

async fn call<B>(
    connection: &zbus::Connection,
    destination: &str,
    interface: &str,
    method: &str,
    body: &B,
) -> Result<zbus::Message, String>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    let path = if destination == "org.freedesktop.DBus" {
        "/org/freedesktop/DBus"
    } else {
        PATH
    };
    tokio::time::timeout(
        CALL_TIMEOUT,
        connection.call_method(Some(destination), path, Some(interface), method, body),
    )
    .await
    .map_err(|_| format!("{destination} did not answer"))?
    .map_err(|error| error.to_string())
}

async fn properties(
    connection: &zbus::Connection,
    bus: &str,
    interface: &str,
) -> Result<HashMap<String, OwnedValue>, String> {
    call(
        connection,
        bus,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        &(interface,),
    )
    .await?
    .body()
    .deserialize()
    .map_err(|error| error.to_string())
}

async fn player_owner(connection: &zbus::Connection, bus: &str) -> Result<String, String> {
    call(
        connection,
        "org.freedesktop.DBus",
        "org.freedesktop.DBus",
        "GetNameOwner",
        &(bus,),
    )
    .await?
    .body()
    .deserialize()
    .map_err(|error| error.to_string())
}

/// A browser can reuse its track ID, so require the visible media identity too.
fn same_media(before: &Player, now: &Player) -> bool {
    before.bus == now.bus
        && before.owner == now.owner
        && before.track_id.is_some()
        && before.track_id == now.track_id
        && !before.title.is_empty()
        && before.title == now.title
        && before.media_url == now.media_url
        && before.artists == now.artists
        && before.album == now.album
        && now.status != Playback::Stopped
}

/// Keep timing for an identified track when Firefox withdraws it after a seek.
/// A real Seeked(0), paused reading, track change or player restart wins.
pub fn reconcile_timing(before: &[Player], now: &mut [Player]) {
    for player in now {
        let Some(old) = before.iter().find(|old| same_media(old, player)) else {
            continue;
        };
        let missing_length = player.length_us.is_none();
        if missing_length {
            player.length_us = old.length_us;
        }
        let broken_zero = !player.seeked
            && player.status == Playback::Playing
            && player.position_us == Some(0)
            && (missing_length || player.app() == "firefox")
            && old.position_at(player.sampled).is_some_and(|at| at > 0);
        if player.position_us.is_none() || broken_zero {
            player.position_us = old.position_at(player.sampled);
        }
    }
}

struct SeekUpdate {
    track: Player,
    position: i64,
    sampled: Instant,
}

fn remember_seek(message: &zbus::Message, players: &[Player], seeks: &mut Vec<SeekUpdate>) {
    let header = message.header();
    if header.interface().map(|i| i.as_str()) != Some(PLAYER)
        || header.member().map(|m| m.as_str()) != Some("Seeked")
    {
        return;
    }
    let Some(sender) = header.sender() else {
        return;
    };
    let Ok((position,)) = message.body().deserialize::<(i64,)>() else {
        return;
    };
    if position < 0 {
        return;
    }
    if let Some(track) = players
        .iter()
        .find(|p| p.owner.as_deref() == Some(sender.as_str()))
    {
        seeks.push(SeekUpdate {
            track: track.clone(),
            position,
            sampled: Instant::now(),
        });
    }
}

/// Every player on `connection`, in bus-name order. A player that does not
/// answer is left out.
pub async fn snapshot(connection: &zbus::Connection) -> Result<Vec<Player>, String> {
    let names: Vec<String> = call(
        connection,
        "org.freedesktop.DBus",
        "org.freedesktop.DBus",
        "ListNames",
        &(),
    )
    .await?
    .body()
    .deserialize()
    .map_err(|error| error.to_string())?;
    let buses = names.into_iter().filter(|name| {
        name.strip_prefix(PREFIX)
            .is_some_and(|key| crate::config::valid_player_key(key) && app_key(key) != "playerctld")
    });
    let mut players: Vec<Player> =
        cosmic::iced::futures::stream::iter(buses.map(|bus| async move {
            // Address the unique owner so a process replacement cannot mix
            // properties from two different players in the same snapshot.
            let owner = player_owner(connection, &bus).await.ok()?;
            let (props, root) = tokio::join!(
                properties(connection, &owner, PLAYER),
                properties(connection, &owner, ROOT)
            );
            props.ok().map(|props| {
                let mut player =
                    parse_player(&bus, &root.unwrap_or_default(), &props, Instant::now());
                player.owner = Some(owner);
                player
            })
        }))
        .buffer_unordered(4)
        .filter_map(|player| async move { player })
        .collect()
        .await;
    players.sort_by(|a, b| a.bus.cmp(&b.bus));
    Ok(players)
}

/// Sends `command` to the player at `bus`.
pub async fn control(
    connection: &zbus::Connection,
    bus: &str,
    command: Command,
) -> Result<(), String> {
    if !bus
        .strip_prefix(PREFIX)
        .is_some_and(crate::config::valid_player_key)
    {
        return Err("Not a media player".into());
    }
    let (interface, method) = match command {
        Command::Play => (PLAYER, "Play"),
        Command::Pause => (PLAYER, "Pause"),
        Command::Next => (PLAYER, "Next"),
        Command::Previous => (PLAYER, "Previous"),
        Command::Raise => (ROOT, "Raise"),
    };
    call(connection, bus, interface, method, &())
        .await
        .map(|_| ())
}

fn valid_track_id(id: &str) -> bool {
    id != "/org/mpris/MediaPlayer2/TrackList/NoTrack"
        && zbus::zvariant::ObjectPath::try_from(id).is_ok()
}

pub async fn seek_to(connection: &zbus::Connection, request: &SeekRequest) -> Result<(), String> {
    if !request
        .bus
        .strip_prefix(PREFIX)
        .is_some_and(crate::config::valid_player_key)
        || !valid_track_id(&request.track_id)
        || request.position_us < 0
    {
        return Err("Invalid media seek request".into());
    }
    // SetPosition normally rejects stale IDs. Firefox-based browsers reuse an
    // ID, so compare the available media identity as well. Missing optional
    // metadata alone does not imply a different track.
    let owner = player_owner(connection, &request.bus).await?;
    if request
        .owner
        .as_ref()
        .is_some_and(|previous| previous != &owner)
    {
        return Ok(());
    }
    let props = properties(connection, &owner, PLAYER).await?;
    let current = parse_player(&request.bus, &HashMap::new(), &props, Instant::now());
    let title_changed =
        !request.title.is_empty() && !current.title.is_empty() && request.title != current.title;
    let url_changed = request
        .media_url
        .as_ref()
        .zip(current.media_url.as_ref())
        .is_some_and(|(before, now)| before != now);
    // Firefox can lose duration after a seek. Use the captured duration only
    // with a positively matched media URL, title and process, never ID alone.
    let length = current.length_us.or_else(|| {
        (request.owner.as_ref() == Some(&owner)
            && !request.title.is_empty()
            && request.title == current.title
            && request.media_url.is_some()
            && request.media_url == current.media_url)
            .then_some(request.length_us)
    });
    if current.track_id.as_deref() != Some(request.track_id.as_str())
        || title_changed
        || url_changed
        || !current.can_seek
        || length.is_none_or(|length| request.position_us >= length)
    {
        // Match MPRIS's stale-request behavior. perform() still refreshes the
        // card, so it shows the current track/timing after an ignored click.
        return Ok(());
    }
    let id = zbus::zvariant::ObjectPath::try_from(request.track_id.as_str())
        .map_err(|e| e.to_string())?;
    call(
        connection,
        &owner,
        PLAYER,
        "SetPosition",
        &(id, request.position_us),
    )
    .await
    .map(|_| ())
}

/// Follows the session bus while subscribed.
pub fn watch() -> Subscription<Feed> {
    Subscription::run_with("halcyon-mpris", |_| feed())
}

fn feed() -> impl cosmic::iced::futures::Stream<Item = Feed> {
    cosmic::iced::stream::channel(4, async move |mut output| {
        loop {
            let connection = match connection().await {
                Ok(connection) => connection,
                Err(error) => {
                    let _ = output.send(Feed::Unavailable(error)).await;
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    continue;
                }
            };
            if let Err(error) = follow(&connection, &mut output).await {
                let _ = output.send(Feed::Unavailable(error)).await;
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        }
    })
}

async fn follow(
    connection: &zbus::Connection,
    output: &mut cosmic::iced::futures::channel::mpsc::Sender<Feed>,
) -> Result<(), String> {
    let error = |error: zbus::Error| error.to_string();
    let signals = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .path(PATH)
        .map_err(error)?
        .build();
    let owners = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")
        .map_err(error)?
        .interface("org.freedesktop.DBus")
        .map_err(error)?
        .member("NameOwnerChanged")
        .map_err(error)?
        .arg0ns(ROOT)
        .map_err(error)?
        .build();
    let mut signals = zbus::MessageStream::for_match_rule(signals, connection, Some(64))
        .await
        .map_err(error)?;
    let mut owners = zbus::MessageStream::for_match_rule(owners, connection, Some(16))
        .await
        .map_err(error)?;
    // Browser transitions can announce a title/status before timing settles.
    // Brief follow-up reads recover new timing; same-media reconciliation
    // below retains known timing only while the track and process match.
    let mut settling_reads = 3;
    let mut previous = Vec::new();
    let mut seeks: Vec<SeekUpdate> = Vec::new();
    loop {
        let mut players = snapshot(connection).await?;
        reconcile_timing(&previous, &mut players);
        for seek in seeks.drain(..) {
            if let Some(player) = players.iter_mut().find(|p| same_media(&seek.track, p)) {
                player.position_us = Some(seek.position);
                player.sampled = seek.sampled;
                player.seeked = true;
            }
        }
        previous = players.clone();
        let playing = players
            .iter()
            .any(|player| player.status == Playback::Playing);
        let incomplete = players.iter().any(|player| {
            player.has_track() && (player.position_us.is_none() || player.length_us.is_none())
        });
        if output.send(Feed::Players(players)).await.is_err() {
            return Ok(());
        }
        let resync = if settling_reads > 0 {
            settling_reads -= 1;
            Duration::from_secs(1)
        } else if playing || incomplete {
            Duration::from_secs(15)
        } else {
            Duration::from_secs(120)
        };
        let changed = tokio::select! {
            message = signals.next() => {
                let message = message.ok_or("Lost the session bus")?.map_err(error)?;
                remember_seek(&message, &previous, &mut seeks);
                true
            },
            message = owners.next() => {
                if message.is_none() { return Err("Lost the session bus".into()) }
                true
            },
            _ = tokio::time::sleep(resync) => false,
        };
        if changed {
            settling_reads = 3;
        }
        // A track change sends several signals at once; read them as one.
        tokio::time::sleep(Duration::from_millis(120)).await;
        while let Some(Some(message)) = signals.next().now_or_never() {
            remember_seek(&message.map_err(error)?, &previous, &mut seeks);
        }
        while let Some(Some(_)) = owners.next().now_or_never() {}
    }
}

/// A decoded, square artwork thumbnail.
#[derive(Clone, Debug, PartialEq)]
pub struct Art {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Loads `url` (a local `file://` picture, or `https://` as streaming
/// players give) into a thumbnail. PNG and JPEG only, bounded in size.
pub async fn load_art(url: String) -> Result<Art, String> {
    let bytes = if let Some(path) = url.strip_prefix("file://") {
        let path = std::path::PathBuf::from(percent_decode(path)?);
        tokio::task::spawn_blocking(move || super::read_bounded(&path, MAX_ART_BYTES))
            .await
            .map_err(|error| error.to_string())??
            .ok_or("The artwork file is gone")?
    } else if url.starts_with("https://") {
        // A player names any address it likes. Artwork comes from the public
        // internet over HTTPS only, through every redirect, and never from
        // this computer or the local network.
        if !public_https(&reqwest::Url::parse(&url).map_err(|error| error.to_string())?) {
            return Err("Unsupported artwork address".into());
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .https_only(true)
            .dns_resolver(std::sync::Arc::new(PublicOnly))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 || !public_https(attempt.url()) {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|error| error.to_string())?;
        let mut response = client
            .get(&url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
            if bytes.len() + chunk.len() > MAX_ART_BYTES {
                return Err("The artwork is too large".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        bytes
    } else {
        return Err("Unsupported artwork address".into());
    };
    tokio::task::spawn_blocking(move || thumbnail(&bytes))
        .await
        .map_err(|error| error.to_string())?
}

/// An HTTPS address whose host isn't a local or private IP address (names
/// are checked when they resolve, by `PublicOnly`).
fn public_https(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    url.scheme() == "https"
        && match host.parse::<std::net::IpAddr>() {
            Ok(ip) => public_ip(ip),
            Err(_) => {
                let host = host.to_ascii_lowercase();
                host != "localhost" && !host.ends_with(".localhost")
            }
        }
}

/// Not this computer, the local network, or a reserved range.
fn public_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => {
            !(ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_multicast()
                || ip.is_documentation()
                // Shared address space (carrier-grade NAT), 100.64.0.0/10.
                || (ip.octets()[0] == 100 && (ip.octets()[1] & 0xc0) == 64))
        }
        std::net::IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return public_ip(std::net::IpAddr::V4(v4));
            }
            let first = ip.segments()[0];
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || (first & 0xfe00) == 0xfc00 // unique local
                || (first & 0xffc0) == 0xfe80) // link local
        }
    }
}

/// Resolves artwork hosts to public addresses only, so a name that points
/// at this computer or the local network is refused too.
struct PublicOnly;

impl reqwest::dns::Resolve for PublicOnly {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addresses: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .filter(|address| public_ip(address.ip()))
                .collect();
            if addresses.is_empty() {
                return Err("no public address for the artwork host".into());
            }
            Ok(Box::new(addresses.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

pub(super) fn thumbnail(bytes: &[u8]) -> Result<Art, String> {
    let format = image::guess_format(bytes).map_err(|error| error.to_string())?;
    if !matches!(format, image::ImageFormat::Png | image::ImageFormat::Jpeg) {
        return Err("Unsupported artwork format".into());
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let picture = reader.decode().map_err(|error| error.to_string())?;
    let square = picture
        .resize_to_fill(ART_SIZE, ART_SIZE, image::imageops::FilterType::Triangle)
        .to_rgba8();
    Ok(Art {
        width: square.width(),
        height: square.height(),
        rgba: square.into_raw(),
    })
}

fn percent_decode(text: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text
                .get(index + 1..index + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                .ok_or("Invalid artwork address")?;
            out.push(hex);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    let path = String::from_utf8(out).map_err(|_| "Invalid artwork address".to_string())?;
    if !path.starts_with('/') {
        return Err("Invalid artwork address".into());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value<'a>(v: impl Into<Value<'a>>) -> OwnedValue {
        OwnedValue::try_from(Value::Value(Box::new(v.into()))).unwrap()
    }
    fn player(bus: &str, status: Playback, title: &str) -> Player {
        Player {
            bus: format!("{PREFIX}{bus}"),
            key: bus.into(),
            owner: None,
            seeked: false,
            identity: bus.into(),
            status,
            title: title.into(),
            artists: Vec::new(),
            album: String::new(),
            art_url: None,
            media_url: None,
            track_id: None,
            can_seek: false,
            length_us: None,
            position_us: None,
            rate: 1.,
            sampled: Instant::now(),
            can_play: true,
            can_pause: true,
            can_next: true,
            can_previous: true,
            can_control: true,
            can_raise: false,
        }
    }

    #[test]
    fn metadata_and_capabilities_parse_with_quiet_fallbacks() {
        let mut metadata: HashMap<String, Value<'_>> = HashMap::new();
        metadata.insert("xesam:title".into(), Value::from(" Clair de lune "));
        metadata.insert(
            "xesam:artist".into(),
            Value::from(vec!["Claude Debussy", " ", "Pianist"]),
        );
        metadata.insert("xesam:album".into(), Value::from("Suite"));
        metadata.insert("mpris:length".into(), Value::from(300_000_000u64));
        metadata.insert("mpris:artUrl".into(), Value::from("file:///tmp/a%20b.png"));
        let props = HashMap::from([
            ("PlaybackStatus".to_string(), value("Playing")),
            (
                "Metadata".to_string(),
                OwnedValue::try_from(Value::from(metadata)).unwrap(),
            ),
            ("Position".to_string(), value(12_000_000i64)),
            ("CanPlay".to_string(), value(true)),
            ("CanPause".to_string(), value(true)),
            ("CanGoNext".to_string(), value(false)),
            ("CanGoPrevious".to_string(), value(true)),
        ]);
        let root = HashMap::from([("Identity".to_string(), value("Lollypop"))]);
        let p = parse_player(
            "org.mpris.MediaPlayer2.Lollypop",
            &root,
            &props,
            Instant::now(),
        );
        assert_eq!(p.key, "Lollypop");
        assert_eq!(p.identity, "Lollypop");
        assert_eq!(p.status, Playback::Playing);
        assert_eq!(p.title, "Clair de lune");
        assert_eq!(p.artists, ["Claude Debussy", "Pianist"]);
        assert_eq!(p.album, "Suite");
        assert_eq!(p.length_us, Some(300_000_000));
        assert_eq!(p.position_us, Some(12_000_000));
        assert_eq!(p.art_url.as_deref(), Some("file:///tmp/a%20b.png"));
        assert!(p.can_play && p.can_pause && p.can_previous && !p.can_next);
        // CanControl false disables every control; no identity → readable key.
        let props = HashMap::from([
            ("CanControl".to_string(), value(false)),
            ("CanPlay".to_string(), value(true)),
        ]);
        let p = parse_player(
            "org.mpris.MediaPlayer2.chromium.instance42",
            &HashMap::new(),
            &props,
            Instant::now(),
        );
        assert_eq!(p.identity, "Chromium");
        assert_eq!(p.status, Playback::Stopped);
        assert!(!p.can_play && !p.has_track());
        assert_eq!(percent_decode("/tmp/a%20b.png").unwrap(), "/tmp/a b.png");
        assert!(percent_decode("relative.png").is_err());
        assert!(percent_decode("/bad%2").is_err());
    }

    #[test]
    fn the_card_follows_a_pick_then_the_preference_then_what_plays() {
        let players = vec![
            player("firefox", Playback::Paused, "Video"),
            player("spotify", Playback::Playing, "Song"),
            player("vlc", Playback::Stopped, ""),
        ];
        let recent = vec![format!("{PREFIX}firefox")];
        assert_eq!(
            choose(&players, None, None, &recent).unwrap().key,
            "spotify"
        );
        assert_eq!(
            choose(&players, None, Some("vlc"), &recent).unwrap().key,
            "vlc"
        );
        // A preferred player that is not running falls back.
        assert_eq!(
            choose(&players, None, Some("rhythmbox"), &recent)
                .unwrap()
                .key,
            "spotify"
        );
        let pinned = format!("{PREFIX}firefox");
        assert_eq!(
            choose(&players, Some(&pinned), Some("vlc"), &recent)
                .unwrap()
                .key,
            "firefox"
        );
        // Following a browser outlives its per-process instance name.
        let restarted = vec![
            player("firefox.instance_1_207", Playback::Paused, "Video"),
            player("spotify", Playback::Playing, "Song"),
            player("firefoxpwa", Playback::Paused, "App"),
        ];
        for saved in ["firefox", "firefox.instance_1_84"] {
            assert_eq!(
                choose(&restarted, None, Some(saved), &[]).unwrap().key,
                "firefox.instance_1_207"
            );
        }
        // Two windows of the followed app: the one playing wins.
        let two = vec![
            player("chromium.instance11", Playback::Paused, "A"),
            player("chromium.instance12", Playback::Playing, "B"),
            player("spotify", Playback::Playing, "Song"),
        ];
        assert_eq!(
            choose(&two, None, Some("chromium"), &[]).unwrap().key,
            "chromium.instance12"
        );
        assert_eq!(app_key("firefox.instance_1_84"), "firefox");
        assert_eq!(app_key("chromium.instance4021"), "chromium");
        assert_eq!(app_key("vlc.instance"), "vlc");
        // Only an instance suffix is dropped; other names stay whole.
        assert_eq!(app_key("spotify"), "spotify");
        assert_eq!(
            app_key("kdeconnect.mpris_000001"),
            "kdeconnect.mpris_000001"
        );
        assert_eq!(app_key("tool.instances"), "tool.instances");
        // Nothing playing: the most recently active paused player.
        let quiet = vec![
            player("a", Playback::Paused, "x"),
            player("b", Playback::Paused, "y"),
        ];
        let recent = vec![format!("{PREFIX}b"), format!("{PREFIX}a")];
        assert_eq!(choose(&quiet, None, None, &recent).unwrap().key, "b");
        assert!(choose(&[], None, None, &recent).is_none());
    }

    #[test]
    fn recency_moves_players_that_start_playing_and_forgets_closed_ones() {
        let mut recent = Vec::new();
        let first = vec![
            player("a", Playback::Playing, "x"),
            player("b", Playback::Paused, "y"),
        ];
        remember(&mut recent, &[], &first);
        assert_eq!(recent, [format!("{PREFIX}a"), format!("{PREFIX}b")]);
        let second = vec![
            player("a", Playback::Paused, "x"),
            player("b", Playback::Playing, "y"),
        ];
        remember(&mut recent, &first, &second);
        assert_eq!(recent, [format!("{PREFIX}b"), format!("{PREFIX}a")]);
        remember(&mut recent, &second, &second[..1]);
        assert_eq!(recent, [format!("{PREFIX}a")]);
    }

    #[test]
    fn positions_advance_only_while_playing_and_stop_at_the_end() {
        let start = Instant::now();
        let mut p = player("a", Playback::Playing, "x");
        p.sampled = start;
        p.position_us = Some(1_000_000);
        p.length_us = Some(3_000_000);
        assert_eq!(
            p.position_at(start + Duration::from_secs(1)),
            Some(2_000_000)
        );
        assert_eq!(
            p.position_at(start + Duration::from_secs(9)),
            Some(3_000_000)
        );
        p.status = Playback::Paused;
        assert_eq!(
            p.position_at(start + Duration::from_secs(1)),
            Some(1_000_000)
        );
        assert_eq!(duration(65_000_000), "1:05");
        assert_eq!(duration(3_725_000_000), "1:02:05");
    }

    #[test]
    fn browser_sources_and_seek_requests_preserve_track_identity() {
        let mut p = player("firefox.instance_1_84", Playback::Playing, "Video");
        p.can_seek = true;
        p.length_us = Some(180_000_000);
        p.position_us = Some(30_000_000);
        p.track_id = Some("/org/mpris/MediaPlayer2/track/first".into());
        for (url, site) in [
            ("https://www.youtube.com/watch?v=fixture", Some("YouTube")),
            (
                "https://music.youtube.com/watch?v=fixture",
                Some("YouTube Music"),
            ),
            ("https://youtu.be/fixture", Some("YouTube")),
            ("https://notyoutube.com/watch?v=fixture", None),
            ("https://youtube.com.evil.test/watch", None),
            ("https://soundcloud.com/artist/song", Some("SoundCloud")),
        ] {
            p.media_url = Some(url.into());
            assert_eq!(p.site(), site);
        }
        let request = p.seek_request(0.5).unwrap();
        assert_eq!(request.bus, p.bus);
        assert_eq!(request.track_id, p.track_id.clone().unwrap());
        assert_eq!(request.position_us, 90_000_000);
        assert_eq!(p.seek_request(-0.5).unwrap().position_us, 0);
        assert_eq!(p.seek_request(1.5).unwrap().position_us, 179_999_999);
        assert!(p.seek_request(f32::NAN).is_none());
        assert!(p.progress_delay(p.sampled).is_some());
        assert!(
            p.progress_delay(p.sampled + Duration::from_secs(200))
                .is_none()
        );
        p.length_us = None;
        assert!(p.seek_request(0.5).is_none());
        assert!(p.progress_delay(p.sampled).is_some());
        p.length_us = Some(180_000_000);
        p.can_seek = false;
        assert!(p.seek_request(0.5).is_none());
        p.can_seek = true;
        p.track_id = None;
        assert!(p.seek_request(0.5).is_none());
        assert!(!valid_track_id("/org/mpris/MediaPlayer2/TrackList/NoTrack"));
        assert!(!valid_track_id("not a path"));
    }

    #[test]
    fn artwork_is_decoded_into_a_bounded_square_thumbnail() {
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(300, 150, image::Rgba([10, 20, 30, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let art = thumbnail(&png).unwrap();
        assert_eq!((art.width, art.height), (ART_SIZE, ART_SIZE));
        assert_eq!(art.rgba.len(), (ART_SIZE * ART_SIZE * 4) as usize);
        assert!(thumbnail(b"GIF89a....").is_err());
    }

    #[test]
    fn timing_recovery_respects_zero_pause_track_and_process_changes() {
        let mut old = player("firefox", Playback::Playing, "Video");
        old.owner = Some(":1.20".into());
        old.track_id = Some("/org/mpris/MediaPlayer2/firefox".into());
        old.media_url = Some("https://www.youtube.com/watch?v=1".into());
        old.length_us = Some(300_000_000);
        old.position_us = Some(150_000_000);
        let mut missing = old.clone();
        missing.sampled += Duration::from_secs(2);
        missing.position_us = Some(0);
        missing.length_us = None;
        for boundary in [
            "seek-zero",
            "paused",
            "new-track",
            "new-owner",
            "stopped",
            "missing-title",
            "missing-url",
            "normal",
        ] {
            let mut current = missing.clone();
            match boundary {
                "seek-zero" => current.seeked = true,
                "paused" => current.status = Playback::Paused,
                "new-track" => current.media_url = Some("https://www.youtube.com/watch?v=2".into()),
                "new-owner" => current.owner = Some(":1.21".into()),
                "stopped" => current.status = Playback::Stopped,
                "missing-title" => current.title.clear(),
                "missing-url" => current.media_url = None,
                _ => {}
            }
            reconcile_timing(
                std::slice::from_ref(&old),
                std::slice::from_mut(&mut current),
            );
            let recover = matches!(boundary, "normal" | "paused" | "seek-zero");
            assert_eq!(
                current.length_us,
                if recover { Some(300_000_000) } else { None },
                "{boundary}"
            );
            assert_eq!(
                current.position_us,
                Some(if boundary == "normal" { 152_000_000 } else { 0 }),
                "{boundary}"
            );
        }
    }

    #[test]
    fn seeking_again_works_when_same_browser_track_withdraws_duration() {
        use std::sync::atomic::Ordering;
        let bus = fixture::Bus::new();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let state = fixture::State::new();
                let _service = fixture::serve(
                    &bus.address,
                    "org.mpris.MediaPlayer2.firefox",
                    state.clone(),
                )
                .await;
                let client = zbus::connection::Builder::address(bus.address.as_str())
                    .unwrap()
                    .build()
                    .await
                    .unwrap();
                let before = snapshot(&client).await.unwrap().remove(0);
                let request = before.seek_request(0.5).unwrap();
                state.broken_timing.store(true, Ordering::SeqCst);
                seek_to(&client, &request).await.unwrap();
                assert_eq!(state.position.load(Ordering::SeqCst), 150_000_000);
                // The fallback must still reject a new video with Firefox's constant ID.
                state.track.store(2, Ordering::SeqCst);
                let request = before.seek_request(0.25).unwrap();
                seek_to(&client, &request).await.unwrap();
                assert_eq!(state.position.load(Ordering::SeqCst), 150_000_000);
            });
    }

    #[test]
    fn watcher_keeps_seeked_position_when_browser_loses_timing() {
        use std::sync::atomic::Ordering;
        let bus = fixture::Bus::new();
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let state = fixture::State::new();
            state.playing.store(true, Ordering::SeqCst);
            let service = fixture::serve(&bus.address, "org.mpris.MediaPlayer2.firefox", state.clone()).await;
            let client = zbus::connection::Builder::address(bus.address.as_str()).unwrap().build().await.unwrap();
            let (mut output, mut input) = cosmic::iced::futures::channel::mpsc::channel(4);
            let watcher = tokio::spawn(async move { follow(&client, &mut output).await });
            let first = tokio::time::timeout(Duration::from_secs(2), input.next()).await.unwrap().unwrap();
            assert!(matches!(first, Feed::Players(players) if players[0].length_us == Some(300_000_000)));
            // Firefox emits the correct seek position, then returns zero and
            // omits the duration in every GetAll while it keeps playing.
            state.position.store(0, Ordering::SeqCst);
            state.length.store(0, Ordering::SeqCst);
            service.emit_signal(None::<&str>, PATH, PLAYER, "Seeked", &(150_000_000i64,)).await.unwrap();
            for _ in 0..2 {
                let next = tokio::time::timeout(Duration::from_secs(3), input.next()).await.unwrap().unwrap();
                let Feed::Players(players) = next else { panic!("lost player") };
                assert_eq!(players[0].length_us, Some(300_000_000));
                assert!(players[0].position_us.unwrap() >= 150_000_000);
                assert!(players[0].position_us.unwrap() < 154_000_000);
            }
            // Seeking to the beginning is authoritative, not a broken zero poll.
            service.emit_signal(None::<&str>, PATH, PLAYER, "Seeked", &(0i64,)).await.unwrap();
            let next = tokio::time::timeout(Duration::from_secs(3), input.next()).await.unwrap().unwrap();
            let Feed::Players(players) = next else { panic!("lost player") };
            assert_eq!(players[0].position_us, Some(0));
            assert!(players[0].seeked);
            // A new video reusing Firefox's ID must not inherit the old time.
            state.track.store(2, Ordering::SeqCst);
            service.emit_signal(None::<&str>, PATH, PLAYER, "Seeked", &(0i64,)).await.unwrap();
            let next = tokio::time::timeout(Duration::from_secs(3), input.next()).await.unwrap().unwrap();
            let Feed::Players(players) = next else { panic!("lost player") };
            assert_eq!(players[0].length_us, None);
            assert_eq!(players[0].position_us, Some(0));
            watcher.abort();
        });
    }

    #[test]
    fn watcher_recovers_browser_timing_that_arrives_without_a_signal() {
        use std::sync::atomic::Ordering;
        let bus = fixture::Bus::new();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let state = fixture::State::new();
            state.length.store(0, Ordering::SeqCst);
            let _service = fixture::serve(&bus.address, "org.mpris.MediaPlayer2.browser", state.clone()).await;
            let client = zbus::connection::Builder::address(bus.address.as_str()).unwrap()
                .build().await.unwrap();
            let (mut output, mut input) = cosmic::iced::futures::channel::mpsc::channel(4);
            let watcher = tokio::spawn(async move { follow(&client, &mut output).await });
            let first = tokio::time::timeout(Duration::from_secs(2), input.next()).await.unwrap().unwrap();
            assert!(matches!(first, Feed::Players(players) if players[0].length_us.is_none()));
            // Browser position/metadata can settle after its transition signals.
            state.length.store(224_000_000, Ordering::SeqCst);
            let next = tokio::time::timeout(Duration::from_secs(3), input.next()).await;
            watcher.abort();
            assert!(matches!(next, Ok(Some(Feed::Players(players))) if players[0].length_us == Some(224_000_000)),
                "Late timing must appear promptly, without waiting for another user action");
        });
    }

    #[test]
    fn a_reused_browser_track_id_cannot_seek_a_different_video() {
        use std::sync::atomic::Ordering;
        let bus = fixture::Bus::new();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let state = fixture::State::new();
                let _service = fixture::serve(
                    &bus.address,
                    "org.mpris.MediaPlayer2.browser",
                    state.clone(),
                )
                .await;
                let client = zbus::connection::Builder::address(bus.address.as_str())
                    .unwrap()
                    .build()
                    .await
                    .unwrap();
                let before = snapshot(&client).await.unwrap().remove(0);
                let request = before.seek_request(0.5).unwrap();
                state.track.store(2, Ordering::SeqCst);
                seek_to(&client, &request).await.unwrap();
                assert_eq!(
                    state.position.load(Ordering::SeqCst),
                    60_000_000,
                    "A new video sharing Firefox's constant ID must ignore the old seek"
                );
                let current = snapshot(&client).await.unwrap().remove(0);
                let request = current.seek_request(0.5).unwrap();
                seek_to(&client, &request).await.unwrap();
                assert_eq!(state.position.load(Ordering::SeqCst), 150_000_000);
                // Missing optional metadata is not evidence that a track changed.
                let request = current.seek_request(0.6).unwrap();
                state.details.store(false, Ordering::SeqCst);
                seek_to(&client, &request).await.unwrap();
                assert!((state.position.load(Ordering::SeqCst) - 180_000_000).abs() < 100);
                // Do not trust a previous duration when the browser withdraws it.
                state.length.store(0, Ordering::SeqCst);
                state.position.store(0, Ordering::SeqCst);
                seek_to(&client, &request).await.unwrap();
                assert_eq!(state.position.load(Ordering::SeqCst), 0);
            });
    }

    /// A private bus with a pretend player: the watcher's queries and a
    /// control call behave as on a desktop, without touching the real one.
    #[test]
    fn a_private_bus_player_is_listed_and_controlled() {
        let Ok(mut daemon) = std::process::Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        else {
            eprintln!("dbus-daemon is not installed; skipping");
            return;
        };
        let mut address = String::new();
        {
            use std::io::BufRead;
            let stdout = daemon.stdout.take().unwrap();
            std::io::BufReader::new(stdout)
                .read_line(&mut address)
                .unwrap();
        }
        let address = address.trim().to_string();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = runtime.block_on(async {
            struct Root;
            #[zbus::interface(name = "org.mpris.MediaPlayer2")]
            impl Root {
                #[zbus(property)]
                fn identity(&self) -> String {
                    "Test Player".into()
                }
                #[zbus(property)]
                fn can_raise(&self) -> bool {
                    false
                }
            }
            struct Transport {
                presses: std::sync::Arc<std::sync::atomic::AtomicUsize>,
                position: std::sync::Arc<std::sync::atomic::AtomicI64>,
            }
            #[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
            impl Transport {
                fn play(&self) {
                    self.presses
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
                fn set_position(&self, track: zbus::zvariant::OwnedObjectPath, position: i64) {
                    if track.as_str() == "/test/track_1" {
                        self.position
                            .store(position, std::sync::atomic::Ordering::SeqCst);
                    }
                }
                #[zbus(property)]
                fn position(&self) -> i64 {
                    self.position.load(std::sync::atomic::Ordering::SeqCst)
                }
                #[zbus(property)]
                fn can_seek(&self) -> bool {
                    true
                }
                #[zbus(property)]
                fn playback_status(&self) -> String {
                    "Paused".into()
                }
                #[zbus(property)]
                fn metadata(&self) -> HashMap<String, OwnedValue> {
                    HashMap::from([
                        (
                            "xesam:title".to_string(),
                            OwnedValue::from(zbus::zvariant::Str::from("Song")),
                        ),
                        (
                            "xesam:url".to_string(),
                            OwnedValue::from(zbus::zvariant::Str::from(
                                "https://www.youtube.com/watch?v=fixture",
                            )),
                        ),
                        (
                            "mpris:trackid".to_string(),
                            OwnedValue::from(
                                zbus::zvariant::ObjectPath::try_from("/test/track_1").unwrap(),
                            ),
                        ),
                        ("mpris:length".to_string(), OwnedValue::from(180_000_000i64)),
                    ])
                }
                #[zbus(property)]
                fn can_play(&self) -> bool {
                    true
                }
                #[zbus(property)]
                fn can_pause(&self) -> bool {
                    true
                }
            }
            let presses = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let position = std::sync::Arc::new(std::sync::atomic::AtomicI64::new(0));
            let service = zbus::connection::Builder::address(address.as_str())?
                .name("org.mpris.MediaPlayer2.testplayer")?
                .name("org.mpris.MediaPlayer2.playerctld")?
                .serve_at(PATH, Root)?
                .serve_at(
                    PATH,
                    Transport {
                        presses: presses.clone(),
                        position: position.clone(),
                    },
                )?
                .build()
                .await?;
            let client = zbus::connection::Builder::address(address.as_str())?
                .build()
                .await?;
            let players = snapshot(&client).await.map_err(zbus::Error::Failure)?;
            assert_eq!(players.len(), 1);
            assert_eq!(players[0].identity, "Test Player");
            assert_eq!(players[0].title, "Song");
            assert_eq!(players[0].status, Playback::Paused);
            control(&client, &players[0].bus, Command::Play)
                .await
                .map_err(zbus::Error::Failure)?;
            assert_eq!(presses.load(std::sync::atomic::Ordering::SeqCst), 1);
            assert!(
                control(&client, "org.example.Other", Command::Next)
                    .await
                    .is_err()
            );
            assert_eq!(players[0].site(), Some("YouTube"));
            let mut request = players[0].seek_request(0.5).unwrap();
            seek_to(&client, &request)
                .await
                .map_err(zbus::Error::Failure)?;
            assert_eq!(
                position.load(std::sync::atomic::Ordering::SeqCst),
                90_000_000
            );
            let refreshed = snapshot(&client).await.map_err(zbus::Error::Failure)?;
            assert_eq!(refreshed[0].position_us, Some(90_000_000));
            request.track_id = "/test/old_track".into();
            request.position_us = 0;
            seek_to(&client, &request)
                .await
                .map_err(zbus::Error::Failure)?;
            assert_eq!(
                position.load(std::sync::atomic::Ordering::SeqCst),
                90_000_000,
                "Stale track request must not seek the new video"
            );
            request.bus = "org.example.Other".into();
            assert!(seek_to(&client, &request).await.is_err());
            drop(service);
            Ok::<(), zbus::Error>(())
        });
        let _ = daemon.kill();
        let _ = daemon.wait();
        result.unwrap();
    }

    #[test]
    fn artwork_comes_only_from_the_public_internet_over_https() {
        let ok = |url: &str| public_https(&reqwest::Url::parse(url).unwrap());
        assert!(ok("https://i.scdn.co/image/abc"));
        for refused in [
            "http://i.scdn.co/image/abc",
            "https://localhost/a.png",
            "https://printer.localhost/a.png",
            "https://127.0.0.1/a.png",
            "https://192.168.1.10/a.png",
            "https://10.0.0.1/a.png",
            "https://169.254.169.254/latest",
            "https://100.64.1.1/a.png",
            "https://[::1]/a.png",
            "https://[fd00::1]/a.png",
            "https://[fe80::1]/a.png",
            "https://[::ffff:127.0.0.1]/a.png",
        ] {
            assert!(!ok(refused), "{refused}");
        }
    }
}
