//! Read-only calendar feeds. Subscription URLs are credentials, never config or logs.
use super::http::{self, Login};
use super::*;
use crate::data::{CalendarData, CalendarEvent};
use chrono::{DateTime, NaiveDate, NaiveDateTime, Offset, TimeZone, Utc};
use ical::{
    parser::ical::component::{IcalCalendar, IcalEvent},
    property::Property,
};
use rrule::{RRuleSet, Tz};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
    path::{Path, PathBuf},
};

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretText(pub String);
impl std::fmt::Debug for SecretText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Feed {
    pub choice: CalendarChoice,
    /// A link, or for CalDAV the calendar's own address.
    pub url: SecretText,
    #[serde(default)]
    pub kind: SourceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<Login>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Feeds {
    pub feeds: Vec<Feed>,
}
pub(super) const MAX_BYTES: usize = http::MAX_BYTES;
const MAX_EVENTS: usize = 10000;
/// Calendars of every kind together.

fn url(value: &str) -> Result<reqwest::Url, Problem> {
    http::parse(value, false)
}
async fn download(value: &SecretText, login: Option<&Login>) -> Result<Vec<u8>, Problem> {
    let reply = http::send(http::Request {
        method: reqwest::Method::GET,
        url: url(&value.0)?,
        login,
        headers: &[("Accept", "text/calendar")],
        body: None,
        limit: http::MAX_BYTES,
    })
    .await?;
    match reply.status.as_u16() {
        200..=299 => Ok(reply.bytes),
        401 => Err(Problem::Credentials),
        403 if reply.sent_login => Err(Problem::Credentials),
        _ => Err(Problem::InvalidFeed),
    }
}
/// Where a calendar's events come from: its keyring entry, or a path on
/// this computer, or a system calendar's Evolution Data Server UID.
pub(super) enum Origin<'a> {
    Feed(&'a Feed),
    Path(&'a Path),
    Uid(&'a str),
}
/// Where a listed calendar's details are, by its kind.
fn origin<'a>(
    connection: &'a Connection,
    feeds: &'a Feeds,
    kind: &SourceKind,
    id: &'a str,
) -> Result<Origin<'a>, Problem> {
    match kind {
        SourceKind::System => eds::uid(id).map(Origin::Uid),
        kind if kind.uses_keyring() => feeds
            .feeds
            .iter()
            .find(|f| f.choice.id == id)
            .map(Origin::Feed),
        _ => connection
            .sources
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.path.as_deref())
            .map(Origin::Path),
    }
    .ok_or(Problem::Changed)
}
/// The one place each kind of calendar is read: refreshes, Check connection
/// and the check before adding all come here. A new kind adds an arm here,
/// and says in `SourceKind::uses_keyring` where its details are kept.
pub(super) async fn read_source(
    kind: &SourceKind,
    origin: Origin<'_>,
    choice: &CalendarChoice,
    month: &str,
) -> Result<Vec<CalendarEvent>, Problem> {
    let bytes = match (kind, origin) {
        (SourceKind::Link | SourceKind::Holidays, Origin::Feed(feed)) => {
            download(&feed.url, feed.login.as_ref()).await?
        }
        (SourceKind::CalDav, Origin::Feed(feed)) => {
            caldav::month(
                &feed.url,
                feed.login.as_ref().ok_or(Problem::Changed)?,
                month,
            )
            .await?
        }
        (SourceKind::File, Origin::Path(path)) => local(path.to_path_buf(), read_file).await?,
        (SourceKind::Folder, Origin::Path(path)) => local(path.to_path_buf(), folder::read).await?,
        // Evolution Data Server hands back calendar objects that need their
        // time zones mapped first; eds.rs does that and parses them the same
        // isolated way.
        (SourceKind::System, Origin::Uid(uid)) => return eds::read(uid, choice, month).await,
        _ => return Err(Problem::Changed),
    };
    let on_disk = !kind.uses_keyring();
    parse_isolated(bytes, month.to_string(), choice.clone())
        .await
        .map_err(|problem| match problem {
            Problem::InvalidFeed if on_disk => Problem::CalendarFile,
            other => other,
        })
}
/// Reads a file or folder off the UI thread.
async fn local(
    path: PathBuf,
    read: fn(&Path) -> Result<Vec<u8>, Problem>,
) -> Result<Vec<u8>, Problem> {
    tokio::task::spawn_blocking(move || read(&path))
        .await
        .map_err(|_| Problem::CalendarFile)?
}
/// A calendar file is read again on every refresh, with the same limits.
fn read_file(path: &Path) -> Result<Vec<u8>, Problem> {
    if !path.is_absolute() {
        return Err(Problem::CalendarFile);
    }
    let meta = std::fs::metadata(path).map_err(|_| Problem::CalendarFile)?;
    if !meta.is_file() {
        return Err(Problem::CalendarFile);
    }
    if meta.len() > MAX_BYTES as u64 {
        return Err(Problem::FeedLimit);
    }
    match crate::tools::read_bounded(path, MAX_BYTES) {
        Ok(Some(bytes)) => Ok(bytes),
        _ => Err(Problem::CalendarFile),
    }
}
#[cfg(test)]
pub(super) async fn parsed(feed: &Feed, month: &str) -> Result<Vec<CalendarEvent>, Problem> {
    read_source(&feed.kind, Origin::Feed(feed), &feed.choice, month).await
}
#[cfg(test)]
pub(super) async fn parsed_file(
    path: &Path,
    month: &str,
    choice: &CalendarChoice,
) -> Result<Vec<CalendarEvent>, Problem> {
    read_source(&SourceKind::File, Origin::Path(path), choice, month).await
}

// A dropped refresh must stop the parser, including work inside rrule. One
// worker at a time, with OS limits before reading untrusted input; no secrets
// are passed in argv, written to disk, or emitted on stderr.
pub(super) async fn parse_isolated(
    bytes: Vec<u8>,
    month: String,
    choice: CalendarChoice,
) -> Result<Vec<CalendarEvent>, Problem> {
    parse_isolated_with(bytes, month, choice, |_| {}).await
}
async fn parse_isolated_with(
    bytes: Vec<u8>,
    month: String,
    choice: CalendarChoice,
    spawned: impl FnOnce(u32),
) -> Result<Vec<CalendarEvent>, Problem> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    static SLOT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
    let _slot = SLOT.acquire().await.map_err(|_| Problem::FeedLimit)?;
    let input = serde_json::to_vec(&(bytes, month, choice)).map_err(|_| Problem::InvalidFeed)?;
    let mut command =
        tokio::process::Command::new(std::env::current_exe().map_err(|_| Problem::Storage)?);
    #[cfg(not(test))]
    command.arg("--calendar-parser");
    #[cfg(test)]
    command.args([
        "--exact",
        "calendar::subscription::tests::isolated_parser_entry",
        "--ignored",
        "--nocapture",
    ]);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| Problem::Storage)?;
    spawned(child.id().ok_or(Problem::Storage)?);
    let result = tokio::time::timeout(Duration::from_secs(4), async move {
        let mut stdin = child.stdin.take().ok_or(Problem::Storage)?;
        stdin
            .write_all(&input)
            .await
            .map_err(|_| Problem::FeedLimit)?;
        drop(stdin);
        let output = child
            .wait_with_output()
            .await
            .map_err(|_| Problem::FeedLimit)?;
        if !output.status.success() {
            return Err(Problem::FeedLimit);
        }
        let bytes = output.stdout.as_slice();
        // libtest prints its preamble before entering the worker test.
        #[cfg(test)]
        let bytes = &bytes[bytes
            .iter()
            .position(|b| *b == b'{')
            .ok_or(Problem::InvalidFeed)?..];
        serde_json::from_slice(bytes).map_err(|_| Problem::InvalidFeed)?
    })
    .await;
    result.unwrap_or(Err(Problem::FeedLimit))
}

pub(crate) fn parser_main() -> ! {
    use std::io::Read;
    // This entry point runs before migrations, toolkit initialization or threads.
    for (resource, value) in [
        (libc::RLIMIT_AS, 512 * 1024 * 1024),
        (libc::RLIMIT_CPU, 2),
        (libc::RLIMIT_CORE, 0),
    ] {
        let limit = libc::rlimit {
            rlim_cur: value,
            rlim_max: value,
        };
        if unsafe { libc::setrlimit(resource, &limit) } != 0 {
            std::process::exit(2);
        }
    }
    let mut input = Vec::new();
    let result = (|| {
        std::io::stdin()
            .take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut input)
            .map_err(|_| Problem::InvalidFeed)?;
        if input.len() > 32 * 1024 * 1024 {
            return Err(Problem::FeedLimit);
        }
        let (bytes, month, choice): (Vec<u8>, String, CalendarChoice) =
            serde_json::from_slice(&input).map_err(|_| Problem::InvalidFeed)?;
        parse(&bytes, &month, &choice)
    })();
    if serde_json::to_writer(std::io::stdout(), &result).is_err() {
        std::process::exit(2);
    }
    std::process::exit(0);
}

/// One calendar to add: a feed kept in the keyring (link, holidays,
/// CalDAV), or a file or folder on this computer.
pub(super) enum Added {
    Feed(Feed),
    Path {
        kind: SourceKind,
        path: PathBuf,
        /// The name typed or found for it; empty uses the file name.
        name: String,
        color: String,
    },
}
fn choice(name: &str, color: &str) -> Result<CalendarChoice, Problem> {
    let name: String = name.trim().chars().take(80).collect();
    Ok(CalendarChoice {
        id: store::nonce()?,
        name: if name.is_empty() {
            "Calendar".into()
        } else {
            name
        },
        primary: false,
        color: color.into(),
    })
}

pub async fn add_subscription(
    expected: Connection,
    name: String,
    secret: SecretText,
    login: Option<Login>,
) -> Result<Connection, Problem> {
    if store::preview() {
        return Err(Problem::Preview);
    }
    let feed = Feed {
        choice: choice(&name, "")?,
        url: SecretText(url(&secret.0)?.into()),
        kind: SourceKind::Link,
        login: login.filter(|l| !l.is_empty()),
    };
    add(expected, vec![Added::Feed(feed)]).await
}
/// Adds the chosen calendars of one CalDAV account. Each keeps its name and
/// color from the server.
pub async fn add_caldav(
    expected: Connection,
    login: Login,
    calendars: Vec<caldav::CalDavCalendar>,
) -> Result<Connection, Problem> {
    if store::preview() {
        return Err(Problem::Preview);
    }
    if calendars.is_empty() {
        return Err(Problem::NoCalendars);
    }
    let mut added = vec![];
    for calendar in calendars {
        added.push(Added::Feed(Feed {
            choice: choice(&calendar.name, &calendar.color)?,
            url: SecretText(url(&calendar.url.0)?.into()),
            kind: SourceKind::CalDav,
            login: Some(login.clone()),
        }));
    }
    add(expected, added).await
}
/// Adds a local .ics file. Its path is not a secret and needs no keyring.
pub async fn add_file(
    expected: Connection,
    name: String,
    path: PathBuf,
) -> Result<Connection, Problem> {
    if store::preview() {
        return Err(Problem::Preview);
    }
    if !path.is_absolute() {
        return Err(Problem::CalendarFile);
    }
    add(
        expected,
        vec![Added::Path {
            kind: SourceKind::File,
            path,
            name,
            color: String::new(),
        }],
    )
    .await
}
/// Adds public holidays for a country from the table in `holidays`. It is
/// an ordinary public link, kept like any other, in a muted color.
pub async fn add_holidays(expected: Connection, country: usize) -> Result<Connection, Problem> {
    if store::preview() {
        return Err(Problem::Preview);
    }
    add(expected, vec![Added::Feed(holiday_feed(country)?)]).await
}
/// The feed for a country's holidays: a public link with no login.
pub(super) fn holiday_feed(country: usize) -> Result<Feed, Problem> {
    let address = holidays::url(country).ok_or(Problem::FeedUrl)?;
    let name = holidays::name(country).ok_or(Problem::FeedUrl)?;
    Ok(Feed {
        choice: choice(&name, holidays::COLOR)?,
        url: SecretText(url(&address)?.into()),
        kind: SourceKind::Holidays,
        login: None,
    })
}
/// Adds calendars found in a folder of .ics files. Paths need no keyring.
pub async fn add_folders(
    expected: Connection,
    calendars: Vec<folder::FolderCalendar>,
) -> Result<Connection, Problem> {
    if store::preview() {
        return Err(Problem::Preview);
    }
    if calendars.is_empty() {
        return Err(Problem::EmptyFolder);
    }
    let added = calendars
        .into_iter()
        .map(|c| Added::Path {
            kind: SourceKind::Folder,
            path: c.path,
            name: c.name,
            color: c.color,
        })
        .collect();
    add(expected, added).await
}
pub(super) async fn add(expected: Connection, added: Vec<Added>) -> Result<Connection, Problem> {
    // A Google sign-in is replaced, as before; other sources join the list.
    let subscribed = expected.backend == Backend::Subscription;
    let current = if subscribed {
        expected.subscriptions.clone()
    } else {
        vec![]
    };
    if current.len() + added.len() > MAX_CALENDARS {
        return Err(Problem::CalendarLimit);
    }
    let new_feeds = added.iter().any(|a| matches!(a, Added::Feed(_)));
    let mut feeds = if new_feeds && subscribed && !expected.credential.is_empty() {
        keyring::read::<Feeds>(&expected.credential, true).await?
    } else {
        Feeds::default()
    };
    let mut sources = if subscribed {
        expected.sources.clone()
    } else {
        vec![]
    };
    let month = chrono::Local::now().format("%Y-%m").to_string();
    let mut choices = vec![];
    for item in added {
        match item {
            Added::Feed(feed) => {
                if feeds
                    .feeds
                    .iter()
                    .any(|f| f.url.0 == feed.url.0 && f.kind == feed.kind)
                {
                    return Err(Problem::Duplicate);
                }
                read_source(&feed.kind, Origin::Feed(&feed), &feed.choice, &month).await?;
                sources.push(Source {
                    id: feed.choice.id.clone(),
                    kind: feed.kind.clone(),
                    path: None,
                });
                choices.push(feed.choice.clone());
                feeds.feeds.push(feed);
            }
            Added::Path {
                kind,
                path,
                name,
                color,
            } => {
                if !path.is_absolute() {
                    return Err(Problem::CalendarFile);
                }
                if sources.iter().any(|s| s.path.as_ref() == Some(&path)) {
                    return Err(Problem::Duplicate);
                }
                let name = if name.trim().is_empty() {
                    path.file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default()
                } else {
                    name
                };
                let choice = choice(&name, &color)?;
                read_source(&kind, Origin::Path(&path), &choice, &month).await?;
                sources.push(Source {
                    id: choice.id.clone(),
                    kind,
                    path: Some(path),
                });
                choices.push(choice);
            }
        }
    }
    if new_feeds {
        keyring::ready().await?;
    }
    let mut selections = if subscribed {
        expected.selected_calendars()
    } else {
        vec![]
    };
    selections.extend(choices.iter().cloned());
    let mut subscriptions = current;
    subscriptions.extend(choices.iter().cloned());
    let connection = Connection {
        backend: Backend::Subscription,
        account: "Calendar subscriptions".into(),
        calendar_id: selections[0].id.clone(),
        calendar_name: selections[0].name.clone(),
        revision: store::nonce()?,
        // Files and folders alone need no keyring item; an unchanged one is kept.
        credential: if new_feeds {
            store::nonce()?
        } else if subscribed {
            expected.credential.clone()
        } else {
            String::new()
        },
        selections,
        subscriptions,
        sources,
    };
    commit(&expected, connection, new_feeds.then_some(&feeds)).await
}
async fn commit(
    expected: &Connection,
    connection: Connection,
    feeds: Option<&Feeds>,
) -> Result<Connection, Problem> {
    // Without feeds to save there is no keyring item for this connection.
    let _lease = match feeds {
        Some(feeds) if !feeds.feeds.is_empty() => {
            Some(keyring::save_prepared(connection.credential.clone(), feeds.clone()).await?)
        }
        _ => None,
    };
    if let Err(problem) = store::replace(expected, &connection) {
        // The prepared credential remains journaled for retry.
        return Err(problem);
    }
    drop(_lease);
    super::schedule_cleanup();
    Ok(connection)
}
pub async fn remove_subscription(expected: Connection, id: String) -> Result<Connection, Problem> {
    if store::preview() {
        return Err(Problem::Preview);
    }
    if expected.backend != Backend::Subscription {
        return Err(Problem::Changed);
    }
    if !expected.subscriptions.iter().any(|c| c.id == id) {
        return Err(Problem::Changed);
    }
    if expected.subscriptions.len() == 1 {
        return super::disconnect(expected).await;
    }
    let mut connection = expected.clone();
    connection.subscriptions.retain(|c| c.id != id);
    connection.sources.retain(|s| s.id != id);
    connection.selections = expected
        .selected_calendars()
        .into_iter()
        .filter(|c| c.id != id)
        .collect();
    if connection.selections.is_empty() {
        connection
            .selections
            .push(connection.subscriptions[0].clone());
    }
    connection.calendar_id = connection.selections[0].id.clone();
    connection.calendar_name = connection.selections[0].name.clone();
    connection.revision = store::nonce()?;
    // A file or folder leaves the keyring item as it is.
    if !expected.source_kind(&id).uses_keyring() {
        return commit(&expected, connection, None).await;
    }
    let mut feeds: Feeds = keyring::read(&expected.credential, true).await?;
    let old = feeds.feeds.len();
    feeds.feeds.retain(|f| f.choice.id != id);
    if feeds.feeds.len() == old {
        return Err(Problem::Changed);
    }
    connection.credential = if feeds.feeds.is_empty() {
        String::new()
    } else {
        store::nonce()?
    };
    commit(&expected, connection, Some(&feeds)).await
}
pub(super) async fn events(
    connection: &Connection,
    month: &str,
    unlock: bool,
) -> Result<CalendarData, Problem> {
    let selected = connection.selected_calendars();
    let feeds: Feeds = if selected
        .iter()
        .any(|c| connection.source_kind(&c.id).uses_keyring())
    {
        keyring::read(&connection.credential, unlock).await?
    } else {
        Feeds::default()
    };
    if feeds.feeds.len() > MAX_CALENDARS {
        return Err(Problem::FeedLimit);
    }
    let mut events = vec![];
    // The first problem with a system calendar. A refresh still shows the
    // other calendars; Check connection, or a list of only system
    // calendars, reports it.
    let mut system_problem = None;
    let only_system = selected
        .iter()
        .all(|c| connection.source_kind(&c.id) == SourceKind::System);
    for choice in &selected {
        let kind = connection.source_kind(&choice.id);
        // When Evolution Data Server is missing or hung, every other system
        // calendar would wait out the same 10 s deadline: skip them.
        if kind == SourceKind::System
            && matches!(
                system_problem,
                Some(Problem::SystemUnavailable | Problem::TimedOut)
            )
        {
            continue;
        }
        let origin = origin(connection, &feeds, &kind, &choice.id)?;
        // The name and color shown are the ones in Settings.
        match read_source(&kind, origin, choice, month).await {
            Ok(found) => events.extend(found),
            Err(problem) if kind == SourceKind::System => {
                if unlock || only_system {
                    return Err(problem);
                }
                system_problem.get_or_insert(problem);
            }
            Err(problem) => return Err(problem),
        }
        if events.len() > MAX_EVENTS
            || events.iter().map(event_text_bytes).sum::<usize>() > EVENT_TEXT_BUDGET
        {
            return Err(Problem::FeedLimit);
        }
    }
    events.sort_by(CalendarEvent::chronological_order);
    Ok(CalendarData {
        account: connection.account.clone(),
        month: month.into(),
        updated_at: Utc::now().to_rfc3339(),
        events,
        ..Default::default()
    })
}
fn value<'a>(props: &'a [Property], name: &str) -> Option<&'a str> {
    props
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))?
        .value
        .as_deref()
}
fn prop<'a>(props: &'a [Property], name: &str) -> Option<&'a Property> {
    props.iter().find(|p| p.name.eq_ignore_ascii_case(name))
}
fn param<'a>(p: &'a Property, key: &str) -> Option<&'a str> {
    p.params
        .as_ref()?
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))?
        .1
        .first()
        .map(String::as_str)
}
fn unescape(text: &str, limit: usize) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    for _ in 0..limit {
        match chars.next() {
            Some('\\') => match chars.next() {
                Some('n' | 'N') => out.push('\n'),
                Some(c) => out.push(c),
                None => break,
            },
            Some(c) => out.push(c),
            None => break,
        }
    }
    out
}
#[derive(Default)]
struct FeedTimeZones {
    default: Option<String>,
    // An alias is usable only if its declarations agree on the IANA location.
    locations: BTreeMap<String, Option<chrono_tz::Tz>>,
}

impl FeedTimeZones {
    fn new(calendar: &IcalCalendar) -> Self {
        let mut locations = BTreeMap::new();
        for definition in &calendar.timezones {
            let properties = &definition.properties;
            let Some(id) = value(properties, "TZID") else {
                continue;
            };
            if let Some(location) =
                value(properties, "X-LIC-LOCATION").and_then(|s| s.trim().parse().ok())
            {
                // A TZID property is iCalendar TEXT. A quoted TZID parameter
                // carries commas literally, so decode the declaration first.
                let mut key = unescape(id, id.len());
                key.make_ascii_lowercase();
                let entry = locations.entry(key).or_insert(Some(location));
                if *entry != Some(location) {
                    *entry = None;
                }
            }
        }
        Self {
            default: value(&calendar.properties, "X-WR-TIMEZONE").map(|id| unescape(id, id.len())),
            locations,
        }
    }

    fn named(&self, id: &str) -> Result<Tz, Problem> {
        // A calendar can supply a more specific location than the worldwide
        // Windows default. Preserve native IANA identifiers as written.
        if let Ok(iana) = id.parse() {
            return Ok(Tz::Tz(iana));
        }
        // MS-OXCICAL §2.1.3.1.1.19.1 matches TZID references case-insensitively.
        let key = id.to_ascii_lowercase();
        self.locations
            .get(&key)
            .copied()
            .unwrap_or_else(|| windows_zones::lookup_folded(&key))
            .map(Tz::Tz)
            .ok_or(Problem::FeedTimezone)
    }
}

fn datetime(p: &Property, raw: &str, zones: &FeedTimeZones) -> Result<DateTime<Tz>, Problem> {
    let normalized = raw.to_ascii_uppercase();
    let raw = normalized.as_str();
    let zone = param(p, "TZID")
        .or(zones.default.as_deref())
        .filter(|_| !raw.ends_with('Z') && raw.len() != 8);
    // Resolve the zone independently of the explicit local date: RFC 5545
    // requires the earlier/pre-gap offset for explicit DATE-TIME values.
    let tz = if let Some(z) = zone {
        zones.named(z)?
    } else if raw.ends_with('Z') {
        Tz::UTC
    } else {
        Tz::LOCAL
    };
    let local = if raw.len() == 8 {
        NaiveDate::parse_from_str(raw, "%Y%m%d")
            .map_err(|_| Problem::InvalidFeed)?
            .and_hms_opt(0, 0, 0)
            .unwrap()
    } else {
        NaiveDateTime::parse_from_str(raw.trim_end_matches('Z'), "%Y%m%dT%H%M%S")
            .map_err(|_| Problem::InvalidFeed)?
    };
    resolve_local_datetime(tz, local)
}

// RFC 5545 §3.3.5: use the first instant in an overlap and the offset before
// a gap. This also resolves nominal duration endpoints (§3.3.6), but must not
// be used for generated RRULE start times: those must skip gaps (§3.3.10).
fn resolve_local_datetime(tz: Tz, local: NaiveDateTime) -> Result<DateTime<Tz>, Problem> {
    if let Some(date) = tz.from_local_datetime(&local).earliest() {
        return Ok(date);
    }
    // Offset before a gap (including historical full-day transitions). Bound
    // the search independently of the feed and preserve the explicit instant.
    for hours in 1..=48 {
        let before = local
            .checked_sub_signed(chrono::Duration::hours(hours))
            .ok_or(Problem::InvalidFeed)?;
        if let Some(date) = tz.from_local_datetime(&before).earliest() {
            return date
                .offset()
                .fix()
                .from_local_datetime(&local)
                .single()
                .map(|date| date.with_timezone(&tz))
                .ok_or(Problem::InvalidFeed);
        }
    }
    Err(Problem::FeedTimezone)
}

fn duration(text: &str) -> Result<(chrono::Duration, u64), Problem> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"^\+?P(?:(\d+)W)?(?:(\d+)D)?(?:T(?:(\d+)H)?(?:(\d+)M)?(?:(\d+)S)?)?$")
            .unwrap()
    });
    let normalized = text.to_ascii_uppercase();
    let caps = re.captures(&normalized).ok_or(Problem::InvalidFeed)?;
    let mut seconds = 0i64;
    let mut nominal_days = 0u64;
    if caps.iter().skip(1).all(|part| part.is_none()) {
        return Err(Problem::InvalidFeed);
    }
    for (i, scale) in [604800i64, 86400, 3600, 60, 1].into_iter().enumerate() {
        if let Some(m) = caps.get(i + 1) {
            let n = m.as_str().parse::<i64>().map_err(|_| Problem::FeedLimit)?;
            seconds = seconds
                .checked_add(n.checked_mul(scale).ok_or(Problem::FeedLimit)?)
                .ok_or(Problem::FeedLimit)?;
            if i < 2 {
                nominal_days = seconds as u64 / 86400;
            }
        }
    }
    if seconds > 366 * 86400 {
        return Err(Problem::FeedLimit);
    }
    Ok((chrono::Duration::seconds(seconds), nominal_days))
}
fn event(
    e: &IcalEvent,
    zones: &FeedTimeZones,
    choice: &CalendarChoice,
) -> Result<(DateTime<Tz>, chrono::Duration, u64, CalendarEvent), Problem> {
    let p = &e.properties;
    let start_prop = prop(p, "DTSTART").ok_or(Problem::InvalidFeed)?;
    let raw = start_prop.value.as_deref().ok_or(Problem::InvalidFeed)?;
    let start = datetime(start_prop, raw, zones)?;
    let all_day = raw.len() == 8;
    let (span, nominal_days) = if let Some(end) = prop(p, "DTEND") {
        let raw_end = end.value.as_deref().ok_or(Problem::InvalidFeed)?;
        if (raw_end.len() == 8) != all_day {
            return Err(Problem::InvalidFeed);
        }
        let elapsed = if all_day {
            let a = NaiveDate::parse_from_str(raw, "%Y%m%d").map_err(|_| Problem::InvalidFeed)?;
            let b =
                NaiveDate::parse_from_str(raw_end, "%Y%m%d").map_err(|_| Problem::InvalidFeed)?;
            b - a
        } else {
            datetime(end, raw_end, zones)? - start
        };
        (elapsed, 0)
    } else if let Some(d) = value(p, "DURATION") {
        duration(d)?
    } else {
        (chrono::Duration::days(i64::from(all_day)), 0)
    };
    if span < chrono::Duration::zero()
        || span > chrono::Duration::days(366)
        || (all_day && span.num_seconds() % 86400 != 0)
    {
        return Err(Problem::InvalidFeed);
    }
    let description = unescape(value(p, "DESCRIPTION").unwrap_or_default(), 8000);
    let location = unescape(value(p, "LOCATION").unwrap_or_default(), 1000);
    let meeting_url = google::meeting_link(
        &serde_json::json!({"description": description, "location": location}),
    );
    let title = unescape(
        value(p, "SUMMARY")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("Untitled event"),
        1000,
    );
    Ok((
        start,
        span,
        nominal_days,
        CalendarEvent {
            title,
            all_day,
            url: value(p, "URL")
                .and_then(google::safe_link)
                .unwrap_or_default(),
            meeting_url,
            description,
            location,
            calendar_name: choice.name.clone(),
            calendar_color: choice.color.clone(),
            ..Default::default()
        },
    ))
}
fn instance(
    mut e: CalendarEvent,
    start: DateTime<Tz>,
    span: chrono::Duration,
    nominal_days: u64,
) -> Result<CalendarEvent, Problem> {
    if e.all_day {
        let end = start
            .date_naive()
            .checked_add_signed(span)
            .ok_or(Problem::InvalidFeed)?;
        e.start = start.date_naive().to_string();
        e.end = end.to_string();
    } else {
        e.start = start.to_rfc3339();
        // Add nominal days in local calendar time, resolve that endpoint, then
        // add elapsed hours/minutes/seconds. Keep the original instant when
        // there are no nominal days, including the later half of an overlap.
        let end = if nominal_days == 0 {
            start
        } else {
            let local = start
                .naive_local()
                .checked_add_days(chrono::Days::new(nominal_days))
                .ok_or(Problem::InvalidFeed)?;
            resolve_local_datetime(start.timezone(), local)?
        };
        e.end = end
            .checked_add_signed(span - chrono::Duration::days(nominal_days as i64))
            .ok_or(Problem::InvalidFeed)?
            .to_rfc3339();
    }
    Ok(e)
}
fn overlaps(e: &CalendarEvent, first: NaiveDate, last: NaiveDate) -> bool {
    let mut day = first;
    while day < last {
        if e.on_date(day) {
            return true;
        }
        let Some(next) = day.succ_opt() else { break };
        day = next;
    }
    false
}
pub(super) fn parse(
    bytes: &[u8],
    month: &str,
    choice: &CalendarChoice,
) -> Result<Vec<CalendarEvent>, Problem> {
    if bytes.len() > MAX_BYTES {
        return Err(Problem::FeedLimit);
    }
    let (from, to) = google::window(month)?;
    let from = DateTime::parse_from_rfc3339(&from).map_err(|_| Problem::InvalidFeed)?;
    let to = DateTime::parse_from_rfc3339(&to).map_err(|_| Problem::InvalidFeed)?;
    let started = std::time::Instant::now();
    let mut normalized = Vec::with_capacity(bytes.len());
    // Unfold before normalizing: even structural tokens can span physical
    // lines. Preserve case-sensitive text, URLs, UIDs and parameter values.
    for content in ical::LineReader::new(Cursor::new(bytes)) {
        let mut line = content.as_str().as_bytes().to_vec();
        if let Some(end) = line.iter().position(|b| matches!(b, b';' | b':')) {
            line[..end].make_ascii_uppercase();
            if &line[..end] == b"BEGIN" || &line[..end] == b"END" {
                line[end + 1..].make_ascii_uppercase();
            }
        }
        normalized.extend_from_slice(&line);
        normalized.push(b'\n');
    }
    let mut parser = ical::IcalParser::new(Cursor::new(normalized));
    let calendar = parser
        .next()
        .ok_or(Problem::InvalidFeed)?
        .map_err(|_| Problem::InvalidFeed)?;
    if parser.next().is_some() || calendar.events.len() > 5000 {
        return Err(Problem::FeedLimit);
    }
    let zones = FeedTimeZones::new(&calendar);
    // One highest-sequence record per UID/recurrence identity. A cancelled
    // override suppresses its original occurrence even without DTSTART.
    let mut records: BTreeMap<(String, Option<i64>), &IcalEvent> = BTreeMap::new();
    for e in &calendar.events {
        let uid = value(&e.properties, "UID").ok_or(Problem::InvalidFeed)?;
        if uid.is_empty() || uid.len() > 1024 {
            return Err(Problem::FeedLimit);
        }
        let uid = uid.to_string();
        let recurrence = if let Some(p) = prop(&e.properties, "RECURRENCE-ID") {
            if param(p, "RANGE").is_some() {
                return Err(Problem::FeedLimit);
            }
            Some(datetime(p, p.value.as_deref().ok_or(Problem::InvalidFeed)?, &zones)?.timestamp())
        } else {
            None
        };
        let key = (uid, recurrence);
        let rank = |e: &IcalEvent| {
            (
                value(&e.properties, "SEQUENCE")
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0),
                value(&e.properties, "DTSTAMP")
                    .unwrap_or_default()
                    .to_owned(),
            )
        };
        if records.get(&key).is_none_or(|old| rank(e) >= rank(old)) {
            records.insert(key, e);
        }
    }
    let mut out = vec![];
    let mut text_bytes = 0usize;
    for ((uid, rid), e) in &records {
        if started.elapsed() > Duration::from_secs(4) {
            return Err(Problem::FeedLimit);
        }
        if value(&e.properties, "STATUS").is_some_and(|v| v.eq_ignore_ascii_case("CANCELLED")) {
            continue;
        }
        let (start, span, nominal_days, template) = event(e, &zones, choice)?;
        let mut dates = vec![start];
        if rid.is_none() {
            let mut set = RRuleSet::new(start).rdate(start);
            let mut recurring = false;
            for p in &e.properties {
                if p.name.eq_ignore_ascii_case("RRULE") {
                    let rule: rrule::RRule<rrule::Unvalidated> = p
                        .value
                        .as_deref()
                        .ok_or(Problem::InvalidFeed)?
                        .to_ascii_uppercase()
                        .parse()
                        .map_err(|_| Problem::InvalidFeed)?;
                    set = set.rrule(rule.validate(start).map_err(|_| Problem::FeedLimit)?);
                    recurring = true;
                } else if p.name.eq_ignore_ascii_case("RDATE")
                    || p.name.eq_ignore_ascii_case("EXDATE")
                {
                    for raw in p.value.as_deref().ok_or(Problem::InvalidFeed)?.split(',') {
                        let date = datetime(p, raw, &zones)?;
                        set = if p.name.eq_ignore_ascii_case("RDATE") {
                            set.rdate(date)
                        } else {
                            set.exdate(date)
                        };
                    }
                    recurring = true;
                } else if p.name.eq_ignore_ascii_case("EXRULE") {
                    return Err(Problem::FeedLimit);
                }
            }
            if recurring {
                let result = set
                    .after((from - span - chrono::Duration::days(1)).with_timezone(&Tz::UTC))
                    .before(to.with_timezone(&Tz::UTC))
                    .all(4096);
                if result.limited {
                    return Err(Problem::FeedLimit);
                }
                dates = result.dates;
            }
        }
        let mut seen = BTreeSet::new();
        for date in dates {
            if !seen.insert(date.timestamp())
                || (rid.is_none() && records.contains_key(&(uid.clone(), Some(date.timestamp()))))
            {
                continue;
            }
            let item = instance(template.clone(), date, span, nominal_days)?;
            if overlaps(&item, from.date_naive(), to.date_naive()) {
                text_bytes = text_bytes.saturating_add(event_text_bytes(&item));
                if text_bytes > EVENT_TEXT_BUDGET {
                    return Err(Problem::FeedLimit);
                }
                out.push(item);
            }
            if out.len() > MAX_EVENTS {
                return Err(Problem::FeedLimit);
            }
        }
    }
    out.sort_by(CalendarEvent::chronological_order);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "private stdin worker invoked by isolated parser tests"]
    fn isolated_parser_entry() {
        parser_main();
    }
    fn choice() -> CalendarChoice {
        CalendarChoice {
            id: "fixture".into(),
            name: "Work".into(),
            primary: false,
            color: "#123abc".into(),
        }
    }
    fn feed(body: &str) -> Vec<u8> {
        format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{body}\r\nEND:VCALENDAR\r\n").into_bytes()
    }
    fn parse_events(body: &str) -> Vec<CalendarEvent> {
        parse(&feed(body), "2026-10", &choice()).unwrap()
    }
    #[test]
    fn explicit_transition_dates_use_first_or_pre_gap_offset() {
        let p = Property {
            name: "DTSTART".into(),
            params: Some(vec![("TZID".into(), vec!["Europe/Zagreb".into()])]),
            value: None,
        };
        assert_eq!(
            datetime(&p, "20261025T023000", &FeedTimeZones::default())
                .unwrap()
                .to_rfc3339(),
            "2026-10-25T02:30:00+02:00"
        );
        assert_eq!(
            datetime(&p, "20260329T023000", &FeedTimeZones::default())
                .unwrap()
                .to_rfc3339(),
            "2026-03-29T03:30:00+02:00"
        );
        let events = parse_events(
            "BEGIN:VEVENT\r\nUID:old\r\nDTSTART;TZID=Europe/Zagreb:20250330T023000\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:current\r\nDTSTART:20261020T100000Z\r\nEND:VEVENT",
        );
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn auditfix_nominal_duration_resolves_transition_endpoints_before_elapsed_remainder() {
        for (month, start, duration, end) in [
            (
                "2026-03",
                "20260328T023000",
                "P1D",
                "2026-03-29T03:30:00+02:00",
            ),
            (
                "2026-10",
                "20261024T023000",
                "P1D",
                "2026-10-25T02:30:00+02:00",
            ),
            (
                "2026-03",
                "20260328T023000",
                "P1DT1H30M45S",
                "2026-03-29T05:00:45+02:00",
            ),
            (
                "2026-10",
                "20261024T023000",
                "P1DT1H30M45S",
                "2026-10-25T03:00:45+01:00",
            ),
            (
                "2026-03",
                "20260322T023000",
                "P1W",
                "2026-03-29T03:30:00+02:00",
            ),
            (
                "2026-10",
                "20261018T023000",
                "P1W",
                "2026-10-25T02:30:00+02:00",
            ),
        ] {
            let bytes = feed(&format!(
                "BEGIN:VEVENT\r\nUID:nominal\r\nDTSTART;TZID=Europe/Zagreb:{start}\r\nDURATION:{duration}\r\nEND:VEVENT"
            ));
            let events = parse(&bytes, month, &choice()).unwrap();
            assert_eq!(events.len(), 1, "{start} {duration}");
            assert_eq!(events[0].end, end, "{start} {duration}");
        }
    }

    #[test]
    fn auditfix_nominal_day_and_elapsed_24_hours_remain_distinct() {
        for (month, start, nominal_end, elapsed_end, nominal_hours) in [
            (
                "2026-03",
                "20260328T120000",
                "2026-03-29T12:00:00+02:00",
                "2026-03-29T13:00:00+02:00",
                23,
            ),
            (
                "2026-10",
                "20261024T120000",
                "2026-10-25T12:00:00+01:00",
                "2026-10-25T11:00:00+01:00",
                25,
            ),
        ] {
            for (duration, expected, hours) in [
                ("P1D", nominal_end, nominal_hours),
                ("PT24H", elapsed_end, 24),
            ] {
                let bytes = feed(&format!(
                    "BEGIN:VEVENT\r\nUID:duration\r\nDTSTART;TZID=Europe/Zagreb:{start}\r\nDURATION:{duration}\r\nEND:VEVENT"
                ));
                let events = parse(&bytes, month, &choice()).unwrap();
                assert_eq!(events.len(), 1);
                assert_eq!(events[0].end, expected);
                assert_eq!(
                    DateTime::parse_from_rfc3339(&events[0].end).unwrap()
                        - DateTime::parse_from_rfc3339(&events[0].start).unwrap(),
                    chrono::Duration::hours(hours),
                );
            }
        }
    }

    #[test]
    fn auditfix_elapsed_duration_preserves_later_overlap_start() {
        let start = DateTime::parse_from_rfc3339("2026-10-25T02:30:00+01:00")
            .unwrap()
            .with_timezone(&Tz::Europe__Zagreb);
        for (minutes, expected) in [
            (0, "2026-10-25T02:30:00+01:00"),
            (30, "2026-10-25T03:00:00+01:00"),
        ] {
            let event = instance(
                CalendarEvent::default(),
                start,
                chrono::Duration::minutes(minutes),
                0,
            )
            .unwrap();
            assert_eq!(event.end, expected);
        }
    }

    #[test]
    fn auditfix_historical_nominal_transition_events_do_not_reject_current_feed() {
        let events = parse_events(concat!(
            "BEGIN:VEVENT\r\nUID:old-spring\r\nDTSTART;TZID=Europe/Zagreb:20250329T023000\r\nDURATION:P1D\r\nEND:VEVENT\r\n",
            "BEGIN:VEVENT\r\nUID:old-autumn\r\nDTSTART;TZID=Europe/Zagreb:20251025T023000\r\nDURATION:P1D\r\nEND:VEVENT\r\n",
            "BEGIN:VEVENT\r\nUID:current\r\nDTSTART:20261020T100000Z\r\nSUMMARY:Current\r\nEND:VEVENT",
        ));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Current");
        assert_eq!(events[0].start, "2026-10-20T10:00:00+00:00");
    }

    #[test]
    fn auditfix_daily_gap_is_skipped_without_consuming_count() {
        // Exercise both iterator generation paths, with and without BYSETPOS.
        for rule in [
            "FREQ=DAILY;COUNT=3",
            "FREQ=DAILY;BYHOUR=2;BYSETPOS=1;COUNT=3",
        ] {
            let bytes = feed(&format!(
                "BEGIN:VEVENT\r\nUID:daily\r\nDTSTART;TZID=Europe/Zagreb:20260328T023000\r\nDURATION:PT30M\r\nRRULE:{rule}\r\nEND:VEVENT"
            ));
            let events = parse(&bytes, "2026-03", &choice()).unwrap();
            assert_eq!(
                events.iter().map(|e| e.start.as_str()).collect::<Vec<_>>(),
                [
                    "2026-03-28T02:30:00+01:00",
                    "2026-03-30T02:30:00+02:00",
                    "2026-03-31T02:30:00+02:00"
                ],
                "{rule}",
            );
        }
    }

    #[test]
    fn auditfix_multiple_hours_keep_valid_gap_day_occurrence_and_complete_count() {
        let bytes = feed(
            "BEGIN:VEVENT\r\nUID:hours\r\nDTSTART;TZID=Europe/Zagreb:20260328T023000\r\nRRULE:FREQ=DAILY;BYHOUR=2,3;COUNT=6\r\nEND:VEVENT",
        );
        let events = parse(&bytes, "2026-03", &choice()).unwrap();
        assert_eq!(
            events.iter().map(|e| e.start.as_str()).collect::<Vec<_>>(),
            [
                "2026-03-28T02:30:00+01:00",
                "2026-03-28T03:30:00+01:00",
                "2026-03-29T03:30:00+02:00",
                "2026-03-30T02:30:00+02:00",
                "2026-03-30T03:30:00+02:00",
                "2026-03-31T02:30:00+02:00",
            ],
        );
    }

    #[test]
    fn auditfix_hourly_gap_preserves_count_and_order() {
        let bytes = feed(
            "BEGIN:VEVENT\r\nUID:hourly\r\nDTSTART;TZID=Europe/Zagreb:20260329T013000\r\nRRULE:FREQ=HOURLY;COUNT=4\r\nEND:VEVENT",
        );
        let events = parse(&bytes, "2026-03", &choice()).unwrap();
        assert_eq!(
            events.iter().map(|e| e.start.as_str()).collect::<Vec<_>>(),
            [
                "2026-03-29T01:30:00+01:00",
                "2026-03-29T03:30:00+02:00",
                "2026-03-29T04:30:00+02:00",
                "2026-03-29T05:30:00+02:00"
            ],
        );
    }

    #[test]
    fn auditfix_recurrence_uses_first_autumn_occurrence_once() {
        let events = parse_events(
            "BEGIN:VEVENT\r\nUID:autumn\r\nDTSTART;TZID=Europe/Zagreb:20261024T023000\r\nDURATION:PT30M\r\nRRULE:FREQ=DAILY;COUNT=3\r\nEND:VEVENT",
        );
        assert_eq!(
            events.iter().map(|e| e.start.as_str()).collect::<Vec<_>>(),
            [
                "2026-10-24T02:30:00+02:00",
                "2026-10-25T02:30:00+02:00",
                "2026-10-26T02:30:00+01:00"
            ],
        );
        assert_eq!(events[1].end, "2026-10-25T02:00:00+01:00");
    }

    #[test]
    fn auditfix_explicit_gap_dtstart_and_rdate_keep_pre_gap_interpretation() {
        let explicit = feed(
            "BEGIN:VEVENT\r\nUID:explicit\r\nDTSTART;TZID=Europe/Zagreb:20260329T023000\r\nDURATION:PT30M\r\nEND:VEVENT",
        );
        let events = parse(&explicit, "2026-03", &choice()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].start, "2026-03-29T03:30:00+02:00");
        assert_eq!(events[0].end, "2026-03-29T04:00:00+02:00");

        let recurring = feed(
            "BEGIN:VEVENT\r\nUID:rdate\r\nDTSTART;TZID=Europe/Zagreb:20260328T023000\r\nRRULE:FREQ=DAILY;COUNT=3\r\nRDATE;TZID=Europe/Zagreb:20260329T023000\r\nEND:VEVENT",
        );
        let events = parse(&recurring, "2026-03", &choice()).unwrap();
        assert_eq!(
            events.iter().map(|e| e.start.as_str()).collect::<Vec<_>>(),
            [
                "2026-03-28T02:30:00+01:00",
                "2026-03-29T03:30:00+02:00",
                "2026-03-30T02:30:00+02:00",
                "2026-03-31T02:30:00+02:00"
            ],
        );
    }

    #[test]
    fn auditfix_recurrence_skips_half_hour_and_full_day_gaps() {
        for (month, zone, start, expected) in [
            (
                "2026-10",
                "Australia/Lord_Howe",
                "20261003T021500",
                [
                    "2026-10-03T02:15:00+10:30",
                    "2026-10-05T02:15:00+11:00",
                    "2026-10-06T02:15:00+11:00",
                ],
            ),
            (
                "2011-12",
                "Pacific/Apia",
                "20111229T120000",
                [
                    "2011-12-29T12:00:00-10:00",
                    "2011-12-31T12:00:00+14:00",
                    "2012-01-01T12:00:00+14:00",
                ],
            ),
        ] {
            let bytes = feed(&format!(
                "BEGIN:VEVENT\r\nUID:gap\r\nDTSTART;TZID={zone}:{start}\r\nRRULE:FREQ=DAILY;COUNT=3\r\nEND:VEVENT"
            ));
            let events = parse(&bytes, month, &choice()).unwrap();
            assert_eq!(
                events.iter().map(|e| e.start.as_str()).collect::<Vec<_>>(),
                expected,
                "{zone}"
            );
        }
    }

    #[tokio::test]
    async fn auditfix_isolated_parser_keeps_nominal_duration_and_completes_gap_count() {
        let bytes = feed(
            "BEGIN:VEVENT\r\nUID:isolated-dst\r\nDTSTART;TZID=Europe/Zagreb:20260328T023000\r\nDURATION:P1D\r\nRRULE:FREQ=DAILY;COUNT=3\r\nEND:VEVENT",
        );
        let events = parse_isolated(bytes, "2026-03".into(), choice())
            .await
            .unwrap();
        assert_eq!(
            events
                .iter()
                .map(|e| (e.start.as_str(), e.end.as_str()))
                .collect::<Vec<_>>(),
            [
                ("2026-03-28T02:30:00+01:00", "2026-03-29T03:30:00+02:00"),
                ("2026-03-30T02:30:00+02:00", "2026-03-31T02:30:00+02:00"),
                ("2026-03-31T02:30:00+02:00", "2026-04-01T02:30:00+02:00"),
            ],
        );
    }

    #[test]
    fn mixed_case_tokens_preserve_text_and_recurrence() {
        let body = "be\r\n gin:ve\r\n vent\r\nUid:CaseSensitive\r\ndtstart:20261020T100000Z\r\nrRuLe:freq=daily;count=3\r\nexdate:20261021T100000Z\r\nrdate:20261024T100000Z\r\nsummary:MiXeD Text\r\nURL:https://example.com/MiXeD\r\nend:vevent\r\nBEGIN:VEVENT\r\nUID:cancelled\r\nDTSTART:20261022T100000Z\r\nstatus:cancelled\r\nEND:VEVENT";
        let events = parse_events(body);
        assert_eq!(events.len(), 3);
        assert!(
            events
                .iter()
                .all(|e| e.title == "MiXeD Text" && e.url == "https://example.com/MiXeD")
        );
        assert_eq!(events[1].start, "2026-10-22T10:00:00+00:00");
    }
    #[tokio::test]
    async fn isolated_parser_bounds_pathological_rules_and_recovers() {
        let started = std::time::Instant::now();
        let seconds = (0..60).map(|n| n.to_string()).collect::<Vec<_>>().join(",");
        let minutes = seconds.clone();
        let hours = (0..24).map(|n| n.to_string()).collect::<Vec<_>>().join(",");
        for rule in [
            "FREQ=SECONDLY".to_string(),
            format!(
                "FREQ=YEARLY;BYMONTH=1,2,3,4,5,6,7,8,9,10,11,12;BYMONTHDAY=1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31;BYHOUR={hours};BYMINUTE={minutes};BYSECOND={seconds}"
            ),
        ] {
            let bytes = feed(&format!(
                "BEGIN:VEVENT\r\nUID:limit\r\nDTSTART:19700101T000000Z\r\nRRULE:{rule}\r\nEND:VEVENT"
            ));
            assert_eq!(
                parse_isolated(bytes, "2026-10".into(), choice())
                    .await
                    .unwrap_err(),
                Problem::FeedLimit
            );
        }
        assert!(started.elapsed() < Duration::from_secs(10));
        let good = feed(
            "BEGIN:VEVENT\r\nUID:good\r\nDTSTART:20261020T100000Z\r\nRRULE:FREQ=DAILY;COUNT=3\r\nEND:VEVENT",
        );
        assert_eq!(
            parse_isolated(good, "2026-10".into(), choice())
                .await
                .unwrap()
                .len(),
            3
        );
    }
    #[tokio::test]
    async fn cancelling_refreshes_terminates_the_parser_workers() {
        for _ in 0..3 {
            let bytes = feed(
                "BEGIN:VEVENT\r\nUID:slow\r\nDTSTART:19700101T000000Z\r\nRRULE:FREQ=SECONDLY\r\nEND:VEVENT",
            );
            let (started, pid) = tokio::sync::oneshot::channel();
            let task = tokio::spawn(parse_isolated_with(
                bytes,
                "2026-10".into(),
                choice(),
                move |pid| {
                    let _ = started.send(pid);
                },
            ));
            let pid = tokio::time::timeout(Duration::from_secs(10), pid)
                .await
                .unwrap()
                .unwrap();
            assert!(
                std::path::Path::new(&format!("/proc/{pid}")).exists(),
                "this request's worker has started"
            );
            task.abort();
            let _ = task.await;
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while std::path::Path::new(&format!("/proc/{pid}")).exists()
                && std::time::Instant::now() < deadline
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert!(
                !std::path::Path::new(&format!("/proc/{pid}")).exists(),
                "cancelled worker must be terminated and reaped"
            );
        }
    }
    #[test]
    fn urls_are_private_https_credentials() {
        assert_eq!(
            url("webcal://example.com/private.ics").unwrap().as_str(),
            "https://example.com/private.ics"
        );
        assert_eq!(
            url("http://example.com/a.ics").unwrap_err(),
            Problem::PublicHttp
        );
        assert_eq!(
            url("http://192.168.1.4/a.ics").unwrap().as_str(),
            "http://192.168.1.4/a.ics"
        );
        for invalid in [
            "file:///etc/passwd",
            "https://user:pass@example.com/x",
            "https://example.com/x#secret",
            "javascript:alert(1)",
        ] {
            assert_eq!(url(invalid).unwrap_err(), Problem::FeedUrl);
        }
        assert!(!format!("{:?}", SecretText("secret-feed-url".into())).contains("secret-feed-url"));
    }
    #[test]
    fn folded_text_all_day_and_meeting_links() {
        let events = parse_events(
            "BEGIN:VEVENT\r\nUID:all-day\r\nDTSTART;VALUE=DATE:20261020\r\nDTEND;VALUE=DATE:20261022\r\nSUMMARY:Trip\\, with\r\n  friends\r\nDESCRIPTION:Line one\\nLine two\r\nLOCATION:https://meet.google.com/abc-defg-hij\r\nEND:VEVENT",
        );
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.title, "Trip, with friends");
        assert_eq!(event.start, "2026-10-20");
        assert_eq!(event.end, "2026-10-22");
        assert!(event.on_date(NaiveDate::from_ymd_opt(2026, 10, 21).unwrap()));
        assert!(!event.on_date(NaiveDate::from_ymd_opt(2026, 10, 22).unwrap()));
        assert_eq!(event.description, "Line one\nLine two");
        assert!(event.join_url().is_some());
        assert_eq!(event.calendar_name, "Work");
    }
    #[test]
    fn recurrence_observes_dst_exclusions_moves_and_cancellations() {
        let events = parse_events(
            "BEGIN:VEVENT\r\nUID:weekly\r\nDTSTART;TZID=Europe/Zagreb:20261018T090000\r\nDTEND;TZID=Europe/Zagreb:20261018T100000\r\nRRULE:FREQ=WEEKLY;COUNT=4\r\nEXDATE;TZID=Europe/Zagreb:20261108T090000\r\nSUMMARY:Weekly\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:weekly\r\nRECURRENCE-ID;TZID=Europe/Zagreb:20261101T090000\r\nDTSTART;TZID=Europe/Zagreb:20261102T110000\r\nDTEND;TZID=Europe/Zagreb:20261102T120000\r\nSUMMARY:Moved\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:weekly\r\nRECURRENCE-ID;TZID=Europe/Zagreb:20261018T090000\r\nSTATUS:CANCELLED\r\nEND:VEVENT",
        );
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].start, "2026-10-25T09:00:00+01:00");
        assert_eq!(events[1].title, "Moved");
        assert_eq!(events[1].start, "2026-11-02T11:00:00+01:00");
    }

    #[test]
    fn outlook_calendar_default_only_applies_to_unqualified_local_times() {
        let body = "X-WR-TIMEZONE:Nepal Standard Time\r\n\
            BEGIN:VEVENT\r\nUID:local\r\nDTSTART:20261019T090000\r\nEND:VEVENT\r\n\
            BEGIN:VEVENT\r\nUID:london\r\nDTSTART;TZID=GMT Standard Time:20261020T090000\r\nEND:VEVENT\r\n\
            BEGIN:VEVENT\r\nUID:utc\r\nDTSTART:20261021T090000Z\r\nEND:VEVENT\r\n\
            BEGIN:VEVENT\r\nUID:date\r\nDTSTART;VALUE=DATE:20261022\r\nEND:VEVENT";
        let events = parse_events(body);
        let starts: Vec<_> = events.iter().map(|e| e.start.as_str()).collect();
        assert_eq!(
            starts,
            [
                "2026-10-19T09:00:00+05:45",
                "2026-10-20T09:00:00+01:00",
                "2026-10-21T09:00:00+00:00",
                "2026-10-22",
            ]
        );
    }

    #[test]
    fn custom_zone_requires_an_unambiguous_location_in_the_same_feed() {
        let event = "BEGIN:VEVENT\r\nUID:custom\r\nDTSTART;TZID=\"(UTC+05:30) Chennai, Kolkata\":20261020T090000\r\nEND:VEVENT";
        let zone = "BEGIN:VTIMEZONE\r\nTZID:(UTC+05:30) CHENNAI\\, Kolkata\r\nX-LIC-LOCATION:Asia/Kolkata\r\n\
            BEGIN:STANDARD\r\nDTSTART:19700101T000000\r\nTZOFFSETFROM:+0530\r\nTZOFFSETTO:+0530\r\nEND:STANDARD\r\nEND:VTIMEZONE";
        assert_eq!(
            parse_events(&format!("{zone}\r\n{event}"))[0].start,
            "2026-10-20T09:00:00+05:30"
        );
        let conflict = zone.replace("Asia/Kolkata", "Asia/Kathmandu");
        let without_location = zone.replace("X-LIC-LOCATION:Asia/Kolkata\r\n", "");
        for body in [
            event.to_string(),
            format!("{without_location}\r\n{event}"),
            format!("{zone}\r\n{conflict}\r\n{event}"),
        ] {
            assert_eq!(
                parse(&feed(&body), "2026-10", &choice()).unwrap_err(),
                Problem::FeedTimezone
            );
        }
    }
    #[test]
    fn highest_sequence_wins_and_unrelated_month_is_omitted() {
        let events = parse_events(
            "BEGIN:VEVENT\r\nUID:a\r\nSEQUENCE:2\r\nDTSTART:20261020T100000Z\r\nDURATION:PT30M\r\nSUMMARY:New\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:a\r\nSEQUENCE:1\r\nDTSTART:20261020T090000Z\r\nSUMMARY:Old\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:b\r\nDTSTART:20250101T090000Z\r\nSUMMARY:Unrelated\r\nEND:VEVENT",
        );
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "New");
        assert_eq!(events[0].end, "2026-10-20T10:30:00+00:00");
    }
    #[test]
    fn invalid_feeds_do_not_become_empty_successes() {
        assert!(parse(b"<html>Sign in</html>", "2026-10", &choice()).is_err());
        assert!(parse(&vec![b'x'; MAX_BYTES + 1], "2026-10", &choice()).is_err());
        let unknown = feed(
            "BEGIN:VEVENT\r\nUID:a\r\nDTSTART;TZID=Custom/Unknown:20261020T100000\r\nEND:VEVENT",
        );
        assert_eq!(
            parse(&unknown, "2026-10", &choice()).unwrap_err(),
            Problem::FeedTimezone
        );
        let too_many = feed(
            "BEGIN:VEVENT\r\nUID:a\r\nDTSTART:20261020T100000Z\r\nRRULE:FREQ=SECONDLY\r\nEND:VEVENT",
        );
        assert_eq!(
            parse(&too_many, "2026-10", &choice()).unwrap_err(),
            Problem::FeedLimit
        );
    }
    #[test]
    fn recurrence_cannot_multiply_large_descriptions_without_bound() {
        let body = format!(
            "BEGIN:VEVENT\r\nUID:large\r\nDTSTART:20261020T100000Z\r\nRRULE:FREQ=MINUTELY;COUNT=2000\r\nDESCRIPTION:{}\r\nEND:VEVENT",
            "x".repeat(8000)
        );
        assert_eq!(
            parse(&feed(&body), "2026-10", &choice()).unwrap_err(),
            Problem::FeedLimit
        );
    }
    #[test]
    fn nominal_days_and_elapsed_hours_differ_across_dst() {
        for (duration, expected) in [
            ("P1D", "2026-10-25T12:00:00+01:00"),
            ("PT24H", "2026-10-25T11:00:00+01:00"),
        ] {
            let body = format!(
                "BEGIN:VEVENT\r\nUID:span\r\nDTSTART;TZID=Europe/Zagreb:20261024T120000\r\nDURATION:{duration}\r\nEND:VEVENT"
            );
            assert_eq!(parse_events(&body)[0].end, expected);
        }
    }
    #[test]
    fn until_and_rdate_keep_inclusive_last_day_without_duplicates() {
        let events = parse_events(
            "BEGIN:VEVENT\r\nUID:until\r\nDTSTART;VALUE=DATE:20261024\r\nRRULE:FREQ=DAILY;UNTIL=20261026\r\nRDATE;VALUE=DATE:20261026,20261028\r\nEND:VEVENT",
        );
        assert_eq!(
            events
                .iter()
                .map(|event| event.start.as_str())
                .collect::<Vec<_>>(),
            ["2026-10-24", "2026-10-25", "2026-10-26", "2026-10-28"]
        );
    }
    #[test]
    fn all_day_recurrence_retains_dates_across_dst() {
        let events = parse_events(
            "BEGIN:VEVENT\r\nUID:a\r\nDTSTART;VALUE=DATE:20261024\r\nDURATION:P2D\r\nRRULE:FREQ=DAILY;COUNT=3\r\nEND:VEVENT",
        );
        assert_eq!(events.len(), 3);
        assert_eq!(events[1].start, "2026-10-25");
        assert_eq!(events[1].end, "2026-10-27");
    }
}

#[cfg(test)]
#[tokio::test]
#[ignore = "requires a disposable Secret Service on a private D-Bus session"]
async fn private_subscription_commit_remove_and_stale_replacement() {
    crate::calendar::tests::require_private_keyring();
    keyring::ready().await.unwrap();
    let feeds = Feeds {
        feeds: ["Work", "Personal"]
            .into_iter()
            .map(|name| Feed {
                choice: CalendarChoice {
                    id: name.into(),
                    name: name.into(),
                    primary: false,
                    color: String::new(),
                },
                url: SecretText(format!(
                    "https://example.invalid/{name}/private-feed-token.ics"
                )),
                kind: SourceKind::Link,
                login: None,
            })
            .collect(),
    };
    let connection = Connection {
        backend: Backend::Subscription,
        account: "Calendar subscriptions".into(),
        calendar_id: "Work".into(),
        calendar_name: "Work".into(),
        revision: store::nonce().unwrap(),
        credential: store::nonce().unwrap(),
        selections: feeds.feeds.iter().map(|f| f.choice.clone()).collect(),
        subscriptions: feeds.feeds.iter().map(|f| f.choice.clone()).collect(),
        sources: vec![],
    };
    let expected = store::load_connection();
    let connected = commit(&expected, connection, Some(&feeds)).await.unwrap();
    let secret: Feeds = keyring::read(&connected.credential, false).await.unwrap();
    assert_eq!(secret.feeds[1].url.0, feeds.feeds[1].url.0);
    let metadata = std::fs::read_to_string(store::account_path().unwrap()).unwrap();
    assert!(!metadata.contains("private-feed-token") && !metadata.contains("example.invalid"));
    let candidate = Connection {
        credential: store::nonce().unwrap(),
        revision: store::nonce().unwrap(),
        ..connected.clone()
    };
    assert_eq!(
        commit(&expected, candidate.clone(), Some(&feeds)).await,
        Err(Problem::Changed)
    );
    assert!(
        store::stale_credentials()
            .unwrap()
            .contains(&candidate.credential)
    );
    crate::calendar::tests::drain_cleanup().await;
    assert!(
        keyring::read::<Feeds>(&candidate.credential, false)
            .await
            .is_err()
    );
    assert!(
        keyring::read::<Feeds>(&connected.credential, false)
            .await
            .is_ok()
    );
    let remaining = remove_subscription(connected.clone(), "Work".into())
        .await
        .unwrap();
    assert_eq!(remaining.selected_calendars()[0].name, "Personal");
    crate::calendar::tests::drain_cleanup().await;
    assert!(
        keyring::read::<Feeds>(&connected.credential, false)
            .await
            .is_err()
    );
    let remaining_feeds: Feeds = keyring::read(&remaining.credential, false).await.unwrap();
    assert_eq!(remaining_feeds.feeds.len(), 1);
    let removed = remove_subscription(remaining.clone(), "Personal".into())
        .await
        .unwrap();
    assert!(!removed.connected() && !removed.cleanup_pending());
    assert!(
        keyring::read::<Feeds>(&remaining.credential, false)
            .await
            .is_err()
    );
}
