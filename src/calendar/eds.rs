//! Calendars from system accounts (GNOME Online Accounts and local Evolution
//! calendars), read through Evolution Data Server on the session bus.
//!
//! Halcyon has no sign-in code for these: EDS owns the accounts and their
//! credentials. Only the source UID, its name and its color are kept in
//! Halcyon's connection file; event text goes through the same isolated
//! parser and limits as subscription feeds. Every call here only reads:
//! listing sources, opening a calendar and asking for its objects.
use super::*;
use crate::data::CalendarEvent;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use zbus::zvariant::OwnedValue;

const SOURCES: &str = "org.gnome.evolution.dataserver.Sources5";
const SOURCE_MANAGER: &str = "/org/gnome/evolution/dataserver/SourceManager";
const SOURCE: &str = "org.gnome.evolution.dataserver.Source";
const CALENDARS: &str = "org.gnome.evolution.dataserver.Calendar8";
const FACTORY_PATH: &str = "/org/gnome/evolution/dataserver/CalendarFactory";
const FACTORY: &str = "org.gnome.evolution.dataserver.CalendarFactory";
const CALENDAR: &str = "org.gnome.evolution.dataserver.Calendar";
/// Calendar IDs in the shared calendar list. Subscription IDs are base64url
/// nonces and never contain a colon.
const PREFIX: &str = "eds:";
/// Every D-Bus call gets this long. Opening a calendar can start its backend.
const CALL: Duration = Duration::from_secs(10);
const MAX_SOURCES: usize = 512;
const MAX_SOURCE_DATA: usize = 64 * 1024;
const MAX_OBJECTS: usize = 5000;
const MAX_ZONES: usize = 32;
/// Shared with link calendars: up to eight calendars in all.
pub const MAX_CALENDARS: usize = 8;

/// A calendar EDS offers. Nothing here is secret; `account` is the name of
/// the account it belongs to, for display only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemCalendar {
    pub uid: String,
    pub name: String,
    pub color: String,
    pub account: String,
}
impl SystemCalendar {
    pub fn id(&self) -> String {
        format!("{PREFIX}{}", self.uid)
    }
    pub fn choice(&self) -> CalendarChoice {
        CalendarChoice {
            id: self.id(),
            name: self.name.clone(),
            primary: false,
            color: self.color.clone(),
        }
    }
}

/// The EDS source UID behind a calendar list ID, when it is a system calendar.
pub fn uid(id: &str) -> Option<&str> {
    id.strip_prefix(PREFIX).filter(|uid| !uid.is_empty())
}
pub fn is_system(id: &str) -> bool {
    uid(id).is_some()
}

/// A session-bus connection with the deadline used for each call.
pub(super) struct Bus {
    connection: zbus::Connection,
    timeout: Duration,
}
impl Bus {
    async fn session() -> Result<Self, Problem> {
        let connection = tokio::time::timeout(CALL, zbus::Connection::session())
            .await
            .map_err(|_| Problem::TimedOut)?
            .map_err(|_| Problem::SystemUnavailable)?;
        Ok(Self {
            connection,
            timeout: CALL,
        })
    }
    async fn call<B, R>(
        &self,
        destination: &str,
        path: &str,
        interface: &str,
        method: &str,
        body: &B,
    ) -> Result<R, Problem>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
        R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
    {
        let reply = tokio::time::timeout(
            self.timeout,
            self.connection
                .call_method(Some(destination), path, Some(interface), method, body),
        )
        .await
        .map_err(|_| Problem::TimedOut)?
        .map_err(|_| Problem::SystemUnavailable)?;
        reply
            .body()
            .deserialize::<R>()
            .map_err(|_| Problem::SystemUnavailable)
    }
}

/// Calendars from every enabled account EDS knows, sorted by account and name.
pub async fn system_calendars() -> Result<Vec<SystemCalendar>, Problem> {
    sources(&Bus::session().await?).await
}

type Managed =
    HashMap<zbus::zvariant::OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;
async fn sources(bus: &Bus) -> Result<Vec<SystemCalendar>, Problem> {
    let managed: Managed = bus
        .call(
            SOURCES,
            SOURCE_MANAGER,
            "org.freedesktop.DBus.ObjectManager",
            "GetManagedObjects",
            &(),
        )
        .await?;
    let mut all = BTreeMap::new();
    for interfaces in managed.values().take(MAX_SOURCES * 4) {
        let Some(source) = interfaces.get(SOURCE) else {
            continue;
        };
        let text = |key: &str| {
            source
                .get(key)
                .and_then(|value| <&str>::try_from(value).ok())
                .map(str::to_owned)
        };
        let (Some(uid), Some(data)) = (text("UID"), text("Data")) else {
            continue;
        };
        if uid.is_empty() || uid.len() > 256 || data.len() > MAX_SOURCE_DATA {
            continue;
        }
        all.insert(uid, keyfile(&data));
        if all.len() >= MAX_SOURCES {
            break;
        }
    }
    Ok(calendars_in(&all))
}

type KeyFile = BTreeMap<String, BTreeMap<String, String>>;
/// The GKeyFile subset EDS writes: `[Group]`, `Key=value`, `#` comments.
/// Localized keys (`Key[hr]`) are skipped.
fn keyfile(data: &str) -> KeyFile {
    let mut groups = KeyFile::new();
    let mut group = None;
    for line in data.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            group = Some(name.to_owned());
            groups.entry(name.to_owned()).or_default();
            continue;
        }
        let (Some(group), Some((key, value))) = (&group, line.split_once('=')) else {
            continue;
        };
        let key = key.trim();
        if key.contains('[') {
            continue;
        }
        groups
            .entry(group.clone())
            .or_default()
            .insert(key.to_owned(), unescape(value.trim_start()));
    }
    groups
}
fn unescape(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}
fn get<'a>(file: &'a KeyFile, group: &str, key: &str) -> Option<&'a str> {
    file.get(group)?.get(key).map(String::as_str)
}
fn off(file: &KeyFile, group: &str, key: &str) -> bool {
    get(file, group, key).is_some_and(|v| v.eq_ignore_ascii_case("false"))
}
fn color(value: Option<&str>) -> String {
    let Some(raw) = value.map(str::trim).and_then(|v| v.strip_prefix('#')) else {
        return String::new();
    };
    let hex = |s: &str| s.chars().all(|c| c.is_ascii_hexdigit());
    match raw.len() {
        6 if hex(raw) => format!("#{}", raw.to_ascii_lowercase()),
        3 if hex(raw) => format!(
            "#{}",
            raw.chars()
                .flat_map(|c| [c, c])
                .collect::<String>()
                .to_ascii_lowercase()
        ),
        // GdkRGBA strings such as #rrrrggggbbbb keep their high bytes.
        12 if hex(raw) => format!(
            "#{}{}{}",
            &raw[0..2].to_ascii_lowercase(),
            &raw[4..6].to_ascii_lowercase(),
            &raw[8..10].to_ascii_lowercase()
        ),
        _ => String::new(),
    }
}
fn clip(text: &str, limit: usize) -> String {
    text.trim().chars().take(limit).collect()
}
fn calendars_in(all: &BTreeMap<String, KeyFile>) -> Vec<SystemCalendar> {
    let mut out: Vec<_> = all
        .iter()
        .filter(|(_, file)| file.contains_key("Calendar"))
        .filter(|(_, file)| {
            !off(file, "Data Source", "Enabled") && !off(file, "Calendar", "Enabled")
        })
        .filter_map(|(uid, file)| {
            let parent = get(file, "Data Source", "Parent").and_then(|p| all.get(p));
            // An account switched off, or with Calendar switched off in
            // Online Accounts, hides its calendars.
            if parent.is_some_and(|p| {
                off(p, "Data Source", "Enabled") || off(p, "Collection", "CalendarEnabled")
            }) {
                return None;
            }
            let name = clip(
                get(file, "Data Source", "DisplayName").unwrap_or_default(),
                80,
            );
            Some(SystemCalendar {
                uid: uid.clone(),
                name: if name.is_empty() {
                    "Calendar".into()
                } else {
                    name
                },
                color: color(get(file, "Calendar", "Color")),
                account: parent
                    .and_then(|p| get(p, "Data Source", "DisplayName"))
                    .map(|n| clip(n, 120))
                    .unwrap_or_default(),
            })
        })
        .collect();
    out.sort_by(|a, b| (&a.account, &a.name, &a.uid).cmp(&(&b.account, &b.name, &b.uid)));
    out
}

/// Events from the selected system calendars for `month`. When EDS can't be
/// reached the other calendars still load: the problem is returned beside
/// whatever was read.
pub(super) async fn events(
    choices: &[CalendarChoice],
    month: &str,
) -> (Vec<CalendarEvent>, Option<Problem>) {
    if choices.is_empty() {
        return (vec![], None);
    }
    match Bus::session().await {
        Ok(bus) => events_on(&bus, choices, month).await,
        Err(problem) => (vec![], Some(problem)),
    }
}
async fn events_on(
    bus: &Bus,
    choices: &[CalendarChoice],
    month: &str,
) -> (Vec<CalendarEvent>, Option<Problem>) {
    let mut events = vec![];
    let mut first = None;
    for choice in choices {
        match calendar_events(bus, choice, month).await {
            Ok(found) => events.extend(found),
            // A hung EDS would hang every calendar after it the same way.
            Err(Problem::TimedOut) => return (events, Some(Problem::TimedOut)),
            // A removed calendar or an oversized one leaves the others.
            Err(problem) => {
                first.get_or_insert(problem);
            }
        }
    }
    (events, first)
}

/// The query EDS evaluates for one month view, in UTC.
fn range_query(month: &str) -> Result<String, Problem> {
    let (from, to) = google::window(month)?;
    let utc = |value: &str| {
        chrono::DateTime::parse_from_rfc3339(value)
            .map(|time| {
                time.with_timezone(&chrono::Utc)
                    .format("%Y%m%dT%H%M%SZ")
                    .to_string()
            })
            .map_err(|_| Problem::InvalidResponse)
    };
    Ok(format!(
        "(occur-in-time-range? (make-time \"{}\") (make-time \"{}\") \"UTC\")",
        utc(&from)?,
        utc(&to)?
    ))
}

async fn calendar_events(
    bus: &Bus,
    choice: &CalendarChoice,
    month: &str,
) -> Result<Vec<CalendarEvent>, Problem> {
    let uid = uid(&choice.id).ok_or(Problem::Changed)?;
    let query = range_query(month)?;
    let (path, name): (String, String) = bus
        .call(CALENDARS, FACTORY_PATH, FACTORY, "OpenCalendar", &(uid,))
        .await?;
    if zbus::zvariant::ObjectPath::try_from(path.as_str()).is_err()
        || zbus::names::BusName::try_from(name.as_str()).is_err()
    {
        return Err(Problem::SystemUnavailable);
    }
    // Some EDS versions need Open before reads; others have it as a no-op.
    // Its own failure is left to the read below, a hang is not.
    if let Err(Problem::TimedOut) = bus
        .call::<_, Vec<String>>(&name, &path, CALENDAR, "Open", &())
        .await
    {
        return Err(Problem::TimedOut);
    }
    let objects: Vec<String> = bus
        .call(&name, &path, CALENDAR, "GetObjectList", &(query.as_str(),))
        .await?;
    if objects.len() > MAX_OBJECTS
        || objects.iter().map(String::len).sum::<usize>() > subscription::MAX_BYTES
    {
        return Err(Problem::FeedLimit);
    }
    let objects: Vec<String> = objects.iter().map(|o| unfold(o)).collect();
    let mut zones = BTreeMap::new();
    for tzid in tzids(&objects) {
        if zones.len() >= MAX_ZONES {
            return Err(Problem::FeedTimezone);
        }
        if known_zone(&tzid) {
            continue;
        }
        // Not an IANA name: ask EDS for its definition, which names the
        // location it stands for in X-LIC-LOCATION or a known prefix.
        let definition: Option<String> = bus
            .call(&name, &path, CALENDAR, "GetTimezone", &(tzid.as_str(),))
            .await
            .ok()
            .filter(|d: &String| d.len() <= 64 * 1024);
        let location = definition
            .as_deref()
            .and_then(zone_location)
            .or_else(|| strip_zone_prefix(&tzid))
            .filter(|zone| known_zone(zone));
        zones.insert(tzid, (location, definition));
    }
    let bytes = calendar(&objects, &zones);
    subscription::parse_isolated(bytes, month.to_string(), choice.clone()).await
}

/// Unfolded content lines with CRLF endings.
fn unfold(object: &str) -> String {
    let text = object.replace("\r\n", "\n");
    let mut out = String::with_capacity(text.len() + 64);
    for line in text.split('\n') {
        if let Some(rest) = line.strip_prefix([' ', '\t']) {
            out.push_str(rest);
            continue;
        }
        if !out.is_empty() {
            out.push_str("\r\n");
        }
        out.push_str(line);
    }
    out.push_str("\r\n");
    out
}
/// The property name and parameters of a content line, before its value.
fn head(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ':' if !quoted => return &line[..i],
            _ => {}
        }
    }
    line
}
fn tzid_param(head: &str) -> Option<(usize, usize, &str)> {
    let upper = head.to_ascii_uppercase();
    let start = upper.find(";TZID=")? + ";TZID=".len();
    let rest = &head[start..];
    let (value, len) = if let Some(quoted) = rest.strip_prefix('"') {
        let end = quoted.find('"')?;
        (&quoted[..end], end + 2)
    } else {
        let end = rest.find(';').unwrap_or(rest.len());
        (&rest[..end], end)
    };
    Some((start, start + len, value))
}
fn tzids(objects: &[String]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for object in objects {
        for line in object.lines() {
            if let Some((_, _, value)) = tzid_param(head(line)) {
                if !value.is_empty() && value.len() <= 256 {
                    out.insert(value.to_owned());
                }
            }
        }
    }
    out
}
fn known_zone(name: &str) -> bool {
    name.parse::<chrono_tz::Tz>().is_ok()
}
/// libical names its built-in zones like `/freeassociation.sourceforge.net/Europe/Zagreb`.
fn strip_zone_prefix(tzid: &str) -> Option<String> {
    let trimmed = tzid.trim_start_matches('/');
    let mut parts = trimmed.splitn(2, '/');
    let first = parts.next()?;
    let rest = parts.next()?;
    (first.contains('.') || first.eq_ignore_ascii_case("citadel")).then(|| {
        // Some generations add another segment, such as `Tzfile/`.
        rest.strip_prefix("Tzfile/").unwrap_or(rest).to_owned()
    })
}
fn zone_location(definition: &str) -> Option<String> {
    unfold(definition).lines().find_map(|line| {
        let name = head(line);
        name.eq_ignore_ascii_case("X-LIC-LOCATION")
            .then(|| line[name.len() + 1..].trim().to_owned())
    })
}
/// One VCALENDAR from EDS's objects: TZIDs EDS knows by another name are
/// rewritten to their IANA location, and the VTIMEZONEs it returned ride along.
fn calendar(
    objects: &[String],
    zones: &BTreeMap<String, (Option<String>, Option<String>)>,
) -> Vec<u8> {
    let mut out = String::from(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Halcyon//System calendars//EN\r\n",
    );
    for (_, definition) in zones.values() {
        if let Some(definition) = definition {
            for component in components(&unfold(definition)) {
                if component
                    .get(..15)
                    .is_some_and(|b| b.eq_ignore_ascii_case("BEGIN:VTIMEZONE"))
                {
                    out.push_str(&component);
                }
            }
        }
    }
    for object in objects {
        for component in components(object) {
            for line in component.lines() {
                out.push_str(&rename_zone(line, zones));
                out.push_str("\r\n");
            }
        }
    }
    out.push_str("END:VCALENDAR\r\n");
    out.into_bytes()
}
/// Top-level components of an object, which may itself be a VCALENDAR.
fn components(object: &str) -> Vec<String> {
    let mut out = vec![];
    let mut depth = 0usize;
    let mut current = String::new();
    for line in object.lines() {
        let name = head(line);
        let value = line.get(name.len() + 1..).unwrap_or_default();
        let begin = name.eq_ignore_ascii_case("BEGIN");
        let end = name.eq_ignore_ascii_case("END");
        if begin && value.eq_ignore_ascii_case("VCALENDAR") && depth == 0 {
            continue;
        }
        if end && value.eq_ignore_ascii_case("VCALENDAR") && depth == 0 {
            continue;
        }
        if begin {
            depth += 1;
        }
        if depth > 0 {
            current.push_str(line);
            current.push_str("\r\n");
        }
        if end && depth > 0 {
            depth -= 1;
            if depth == 0 {
                out.push(std::mem::take(&mut current));
            }
        }
    }
    out
}
fn rename_zone(line: &str, zones: &BTreeMap<String, (Option<String>, Option<String>)>) -> String {
    let name = head(line);
    if let Some((start, end, value)) = tzid_param(name)
        && let Some((Some(location), _)) = zones.get(value)
    {
        return format!("{}{}{}", &line[..start], location, &line[end..]);
    }
    line.to_owned()
}

/// Adds or removes one system calendar in the shared calendar list.
pub async fn set_system_calendar(
    expected: Connection,
    calendar: SystemCalendar,
    on: bool,
) -> Result<Connection, Problem> {
    if store::preview() {
        return Err(Problem::Preview);
    }
    if !on {
        return subscription::remove_subscription(expected, calendar.id()).await;
    }
    let connection = with_system(&expected, &calendar)?;
    if connection == expected {
        return Ok(expected);
    }
    store::replace(&expected, &connection)?;
    if !expected.credential.is_empty() && expected.credential != connection.credential {
        // A Google sign-in this replaced leaves its keyring item behind.
        super::schedule_cleanup();
    }
    Ok(connection)
}
/// The connection with `calendar` added and selected. A Google or legacy
/// connection is replaced, as adding a calendar link does.
fn with_system(expected: &Connection, calendar: &SystemCalendar) -> Result<Connection, Problem> {
    if expected.cleanup_pending() {
        return Err(Problem::Cleanup);
    }
    let choice = calendar.choice();
    if expected.backend == Backend::Subscription && expected.connected() {
        if expected.subscriptions.iter().any(|c| c.id == choice.id) {
            return Ok(expected.clone());
        }
        if expected.subscriptions.len() >= MAX_CALENDARS {
            return Err(Problem::CalendarLimit);
        }
        let mut connection = expected.clone();
        connection.subscriptions.push(choice.clone());
        connection.selections = expected.selected_calendars();
        connection.selections.push(choice);
        connection.revision = store::nonce()?;
        return Ok(connection);
    }
    Ok(Connection {
        backend: Backend::Subscription,
        account: "Calendar subscriptions".into(),
        calendar_id: choice.id.clone(),
        calendar_name: choice.name.clone(),
        revision: store::nonce()?,
        credential: String::new(),
        selections: vec![choice.clone()],
        subscriptions: vec![choice],
    })
}

/// The Online Accounts app, when it is installed.
pub fn online_accounts_app() -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("gnome-online-accounts-gtk"))
        .find(|candidate| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(candidate)
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
}
/// Starts Online Accounts on its own and returns at once. The child is reaped
/// on a thread of its own so Settings never waits for it.
pub fn open_online_accounts() -> Result<(), String> {
    use std::process::Stdio;
    let program = online_accounts_app().ok_or("Online Accounts is not installed.")?;
    let in_service = crate::settings_lifecycle::in_widget_service_now();
    let mut child = crate::settings_lifecycle::independent_command(&program, &[], in_service)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Could not open Online Accounts.".to_string())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
#[path = "eds_tests.rs"]
mod tests;
