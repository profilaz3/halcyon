//! The phone's latest text messages, from KDE Connect on the session bus and
//! woken by its signals. Read only: nothing is sent, paired or started here.
//! Messages stay in memory; names come from the contacts KDE Connect keeps.
use cosmic::iced::{
    Subscription,
    futures::{FutureExt, SinkExt, Stream, StreamExt},
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use zbus::{
    Connection, MatchRule, MessageStream,
    message::Type,
    zvariant::{OwnedValue, Value},
};

use crate::devices::{call, owner, props};

const SERVICE: &str = "org.kde.kdeconnect";
const DAEMON: &str = "/modules/kdeconnect";
const DEVICE: &str = "org.kde.kdeconnect.device";
const CONVERSATIONS: &str = "org.kde.kdeconnect.device.conversations";
/// The SMS plugin's id, as `loadedPlugins` names it.
const SMS_PLUGIN: &str = "kdeconnect_sms";
/// At most this many conversations are kept, newest first.
const MOST: usize = 24;
/// A message's text is cut to this many characters; a card shows one line.
const MOST_TEXT: usize = 280;
/// The phone is asked again this often while either surface needs it, for a phone
/// that doesn't send new messages by itself.
const ASK_AGAIN: Duration = Duration::from_secs(600);
/// How long the phone may take to answer before an empty list is believed.
const ANSWER: Duration = Duration::from_secs(12);
/// Android's message types.
const INBOX: i32 = 1;
/// A message with text, rather than only a picture.
const TEXT_EVENT: i32 = 0x1;

/// One conversation's newest message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Thread {
    pub id: i64,
    /// The contact's name, else the number; a group lists its people.
    pub who: String,
    /// The message's text, on one line. Empty for a picture alone.
    pub body: String,
    /// Sent with a picture, video or other file.
    pub attachment: bool,
    /// When, in milliseconds since 1970.
    pub at_ms: i64,
    /// Sent from the phone, rather than received.
    pub mine: bool,
    /// Received and not read on the phone yet.
    pub unread: bool,
}

/// What the card can show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum State {
    /// Not looked yet.
    #[default]
    Starting,
    /// KDE Connect isn't running, or isn't installed.
    NoService,
    /// KDE Connect is installed but not running yet.
    Stopped,
    /// KDE Connect runs, with no phone paired.
    NoPhone,
    /// The paired phone isn't connected now. Messages read earlier stay.
    Away {
        phone: String,
        threads: Vec<Thread>,
    },
    /// Connected, without the SMS plugin on both sides.
    NoSms {
        phone: String,
    },
    /// Asked the phone, which hasn't answered yet.
    Asking {
        phone: String,
    },
    Ready {
        phone: String,
        threads: Vec<Thread>,
    },
}

impl State {
    pub fn phone(&self) -> Option<&str> {
        match self {
            Self::Away { phone, .. }
            | Self::NoSms { phone }
            | Self::Asking { phone }
            | Self::Ready { phone, .. } => Some(phone),
            _ => None,
        }
    }
    pub fn threads(&self) -> &[Thread] {
        match self {
            Self::Away { threads, .. } | Self::Ready { threads, .. } => threads,
            _ => &[],
        }
    }
    pub fn unread(&self) -> usize {
        self.threads().iter().filter(|thread| thread.unread).count()
    }
}

/// A paired phone, as the daemon describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Phone {
    id: String,
    name: String,
    reachable: bool,
    sms: bool,
}

/// The phone to follow: connected with SMS first, then connected, then any
/// paired one, each in the daemon's order.
fn choose(phones: &[Phone]) -> Option<&Phone> {
    phones
        .iter()
        .find(|p| p.reachable && p.sms)
        .or_else(|| phones.iter().find(|p| p.reachable))
        .or_else(|| phones.first())
}

/// Reads `field` of a D-Bus structure, looking through variants.
fn field<'a>(fields: &'a [Value<'a>], index: usize) -> Option<&'a Value<'a>> {
    let mut value = fields.get(index)?;
    while let Value::Value(inner) = value {
        value = inner;
    }
    Some(value)
}
fn int(value: Option<&Value<'_>>) -> Option<i64> {
    match value? {
        Value::I32(n) => Some(i64::from(*n)),
        Value::I64(n) => Some(*n),
        Value::U32(n) => Some(i64::from(*n)),
        Value::I16(n) => Some(i64::from(*n)),
        Value::U8(n) => Some(i64::from(*n)),
        _ => None,
    }
}
fn string(value: Option<&Value<'_>>) -> Option<String> {
    match value? {
        Value::Str(s) => Some(s.to_string()),
        _ => None,
    }
}

/// A conversation message as KDE Connect marshals it:
/// `(event, body, addresses a(s), date, type, read, thread, uID, subID,
/// attachments a(xsss))`. Older versions stop after `uID`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Message {
    event: i32,
    body: String,
    addresses: Vec<String>,
    date: i64,
    kind: i32,
    read: bool,
    thread: i64,
    attachments: usize,
}

fn message(value: &Value<'_>) -> Option<Message> {
    let mut value = value;
    while let Value::Value(inner) = value {
        value = inner;
    }
    let Value::Structure(structure) = value else {
        return None;
    };
    let fields = structure.fields();
    let addresses = match field(fields, 2)? {
        Value::Array(list) => list
            .iter()
            .filter_map(|address| match address {
                Value::Structure(s) => string(field(s.fields(), 0)),
                Value::Str(s) => Some(s.to_string()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let attachments = match field(fields, 9) {
        Some(Value::Array(list)) => list.len(),
        _ => 0,
    };
    Some(Message {
        event: int(field(fields, 0)).unwrap_or(TEXT_EVENT.into()) as i32,
        body: string(field(fields, 1)).unwrap_or_default(),
        addresses,
        date: int(field(fields, 3))?,
        kind: int(field(fields, 4)).unwrap_or(0) as i32,
        read: int(field(fields, 5)).unwrap_or(1) != 0,
        thread: int(field(fields, 6))?,
        attachments,
    })
}

/// The text on one line, cut short.
fn one_line(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match joined.char_indices().nth(MOST_TEXT) {
        Some((at, _)) => format!("{}…", &joined[..at]),
        None => joined,
    }
}

/// A number's digits without the trunk zeros, so `+44 7700 900123` and
/// `07700 900123` end alike. None for text, such as a bank's name.
fn number_key(number: &str) -> Option<String> {
    let digits: String = number.chars().filter(char::is_ascii_digit).collect();
    let digits = digits.trim_start_matches('0');
    (digits.len() >= 3).then(|| digits.to_string())
}

/// Numbers this long are matched by their ending, so a country code may be
/// there or not; shorter ones, such as service codes, only exactly.
const SUFFIX_MATCH: usize = 7;

/// Names by number, from the vCards KDE Connect syncs for a phone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Contacts {
    numbers: Vec<(String, String)>,
}

impl Contacts {
    /// The contact's name for `address`, else the address as given.
    fn name(&self, address: &str) -> String {
        let found = number_key(address).and_then(|key| {
            self.numbers
                .iter()
                .find(|(number, _)| *number == key)
                .or_else(|| {
                    (key.len() >= SUFFIX_MATCH).then_some(())?;
                    // On a tie the first contact wins, as for exact numbers.
                    self.numbers
                        .iter()
                        .rev()
                        .filter(|(number, _)| {
                            number.len() >= SUFFIX_MATCH
                                && (number.ends_with(&key) || key.ends_with(number.as_str()))
                        })
                        .max_by_key(|(number, _)| number.len().min(key.len()))
                })
        });
        match found {
            Some((_, name)) => name.clone(),
            None => address.trim().to_string(),
        }
    }

    /// Reads one vCard's name and numbers. Folded lines are joined; only
    /// `FN` and `TEL` are read.
    fn add_vcard(&mut self, text: &str) {
        let mut lines: Vec<String> = Vec::new();
        for raw in text.lines() {
            let raw = raw.trim_end_matches('\r');
            match (raw.strip_prefix([' ', '\t']), lines.last_mut()) {
                (Some(rest), Some(last)) => last.push_str(rest),
                _ => lines.push(raw.to_string()),
            }
        }
        let mut name = None;
        let mut numbers = Vec::new();
        for line in &lines {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let key = key.split(';').next().unwrap_or(key).to_ascii_uppercase();
            let key = key.rsplit('.').next().unwrap_or(&key);
            match key {
                "FN" => {
                    let value = value.replace("\\,", ",").replace("\\;", ";");
                    let value = value.trim();
                    if !value.is_empty() {
                        name = Some(value.to_string());
                    }
                }
                "TEL" => numbers.extend(number_key(value.trim_start_matches("tel:"))),
                _ => {}
            }
        }
        if let Some(name) = name {
            for number in numbers {
                // The first contact with a number keeps it.
                if self.numbers.len() < 50_000 {
                    self.numbers.push((number, name.clone()));
                }
            }
        }
    }

    /// The contacts synced from phone `id` under `data`. Nothing there is
    /// no names.
    fn read(data: &Path, id: &str) -> Self {
        let mut contacts = Self::default();
        let Some(dir) = contacts_dir(data, id) else {
            return contacts;
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return contacts;
        };
        for entry in entries.flatten().take(10_000) {
            let path = entry.path();
            if !path
                .extension()
                .is_some_and(|ext| ext == "vcf" || ext == "vcard")
            {
                continue;
            }
            // A contact's photo can make a card large; names come first.
            if let Ok(Some(bytes)) = crate::tools::read_bounded(&path, 512 * 1024) {
                contacts.add_vcard(&String::from_utf8_lossy(&bytes));
            }
        }
        contacts
    }
}

fn data_home() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".local/share")))
}

/// Where KDE Connect writes phone `id`'s contacts, under the data folder.
fn contacts_dir(data: &Path, id: &str) -> Option<PathBuf> {
    // A device id is letters, digits and underscores; anything else is not
    // a folder name to follow.
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    Some(data.join("kpeoplevcard").join(format!("kdeconnect-{id}")))
}

/// Conversations newest first, named from `contacts`.
fn threads(messages: Vec<Message>, contacts: &Contacts) -> Vec<Thread> {
    let mut newest: HashMap<i64, Message> = HashMap::new();
    for message in messages {
        match newest.get(&message.thread) {
            Some(kept) if kept.date >= message.date => {}
            _ => {
                newest.insert(message.thread, message);
            }
        }
    }
    let mut list: Vec<Thread> = newest
        .into_values()
        .map(|m| {
            let mut names: Vec<String> = Vec::new();
            for address in &m.addresses {
                let name = contacts.name(address);
                if !name.is_empty() && !names.contains(&name) {
                    names.push(name);
                }
            }
            let who = match names.len() {
                0 => "Unknown".to_string(),
                1..=3 => names.join(", "),
                n => format!("{}, {} and {} more", names[0], names[1], n - 2),
            };
            Thread {
                id: m.thread,
                who,
                body: if m.event & TEXT_EVENT != 0 || m.attachments == 0 {
                    one_line(&m.body)
                } else {
                    String::new()
                },
                attachment: m.attachments > 0,
                at_ms: m.date,
                mine: m.kind != INBOX,
                unread: m.kind == INBOX && !m.read,
            }
        })
        .collect();
    list.sort_by(|a, b| b.at_ms.cmp(&a.at_ms).then(a.id.cmp(&b.id)));
    list.truncate(MOST);
    list
}

/// Whether the bus could start KDE Connect: it is installed, though not
/// running. It is not started here.
async fn installed(c: &Connection) -> bool {
    let names: Option<Vec<String>> = call(
        c,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ListActivatableNames",
        &(),
    )
    .await;
    names.is_some_and(|names| names.iter().any(|name| name == SERVICE))
}

async fn read_phones(c: &Connection) -> Option<Vec<Phone>> {
    if !owner(c, SERVICE).await {
        return None;
    }
    // Paired phones, connected or not.
    let ids: Vec<String> = call(
        c,
        SERVICE,
        DAEMON,
        "org.kde.kdeconnect.daemon",
        "devices",
        &(false, true),
    )
    .await
    .unwrap_or_default();
    let mut phones = Vec::new();
    for id in ids.iter().take(16) {
        let path = format!("{DAEMON}/devices/{id}");
        if zbus::zvariant::ObjectPath::try_from(path.as_str()).is_err() {
            continue;
        }
        let Some(device) = props(c, SERVICE, &path, DEVICE).await else {
            continue;
        };
        let text = |key: &str| {
            device
                .get(key)
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_default()
        };
        let reachable = device
            .get("isReachable")
            .and_then(|v| bool::try_from(v).ok())
            .unwrap_or(false);
        let plugins: Option<Vec<String>> = if reachable {
            call(c, SERVICE, &path, DEVICE, "loadedPlugins", &()).await
        } else {
            None
        };
        let sms = plugins.is_some_and(|plugins| plugins.iter().any(|p| p == SMS_PLUGIN));
        let name = text("name");
        phones.push(Phone {
            id: id.clone(),
            name: if name.trim().is_empty() {
                "Your phone".into()
            } else {
                name
            },
            reachable,
            sms,
        });
    }
    Some(phones)
}

async fn read_messages(c: &Connection, id: &str) -> Vec<Message> {
    let path = format!("{DAEMON}/devices/{id}");
    let list: Vec<OwnedValue> = call(c, SERVICE, &path, CONVERSATIONS, "activeConversations", &())
        .await
        .unwrap_or_default();
    list.iter().filter_map(|value| message(value)).collect()
}

async fn ask(c: &Connection, id: &str) {
    let path = format!("{DAEMON}/devices/{id}");
    let _: Option<()> = call(
        c,
        SERVICE,
        &path,
        CONVERSATIONS,
        "requestAllConversationThreads",
        &(),
    )
    .await;
}

/// The signals that may change what the card shows; battery and other
/// plugins' signals are not.
fn relevant(message: &zbus::Message) -> bool {
    let header = message.header();
    let interface = header.interface().map(|i| i.as_str()).unwrap_or_default();
    let member = header.member().map(|m| m.as_str()).unwrap_or_default();
    match (interface, member) {
        ("org.freedesktop.DBus", "NameOwnerChanged") => true,
        (CONVERSATIONS, _) => true,
        ("org.freedesktop.DBus.Properties", "PropertiesChanged") => message
            .body()
            .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
            .is_ok_and(|(interface, ..)| interface == DEVICE),
        (i, m) if i.starts_with(SERVICE) => matches!(
            m,
            "deviceAdded"
                | "deviceRemoved"
                | "deviceVisibilityChanged"
                | "deviceListChanged"
                | "reachableChanged"
                | "pairStateChanged"
                | "pluginsChanged"
                | "nameChanged"
        ),
        _ => false,
    }
}

async fn watch(c: &Connection) -> Option<cosmic::iced::futures::stream::SelectAll<MessageStream>> {
    let from = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(SERVICE)
        .ok()?
        .build();
    let owner = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender("org.freedesktop.DBus")
        .ok()?
        .interface("org.freedesktop.DBus")
        .ok()?
        .member("NameOwnerChanged")
        .ok()?
        .arg(0, SERVICE)
        .ok()?
        .build();
    let mut streams = Vec::new();
    for rule in [from, owner] {
        streams.push(
            MessageStream::for_match_rule(rule, c, Some(64))
                .await
                .ok()?,
        );
    }
    Some(cosmic::iced::futures::stream::select_all(streams))
}

/// Follows the chosen phone, remembering what it was asked and what it said.
#[derive(Default)]
struct Follower {
    /// The data folder KDE Connect keeps contacts in.
    data: Option<PathBuf>,
    /// The phone asked for its conversations, and when.
    asked: Option<(String, tokio::time::Instant)>,
    /// The last conversations read per phone, kept while it is away.
    kept: HashMap<String, Vec<Thread>>,
    /// Contacts per phone, with the folder's time they were read at.
    contacts: HashMap<String, (Option<std::time::SystemTime>, Contacts)>,
}

impl Follower {
    fn contacts(&mut self, id: &str) -> &Contacts {
        let data = self.data.clone().unwrap_or_default();
        let stamp = contacts_dir(&data, id)
            .and_then(|dir| std::fs::metadata(dir).ok())
            .and_then(|meta| meta.modified().ok());
        let fresh = self
            .contacts
            .get(id)
            .is_some_and(|(read, _)| read.is_some() && *read == stamp);
        if !fresh {
            let contacts = match &self.data {
                Some(data) => Contacts::read(data, id),
                None => Contacts::default(),
            };
            self.contacts.insert(id.to_string(), (stamp, contacts));
        }
        &self.contacts[id].1
    }

    async fn read(&mut self, c: &Connection) -> State {
        let Some(phones) = read_phones(c).await else {
            self.asked = None;
            return if installed(c).await {
                State::Stopped
            } else {
                State::NoService
            };
        };
        let Some(phone) = choose(&phones).cloned() else {
            self.asked = None;
            return State::NoPhone;
        };
        if !phone.reachable {
            if self.asked.as_ref().is_some_and(|(id, _)| *id == phone.id) {
                self.asked = None;
            }
            return State::Away {
                threads: self.kept.get(&phone.id).cloned().unwrap_or_default(),
                phone: phone.name,
            };
        }
        if !phone.sms {
            return State::NoSms { phone: phone.name };
        }
        let messages = read_messages(c, &phone.id).await;
        let now = tokio::time::Instant::now();
        let due = match &self.asked {
            Some((id, at)) => *id != phone.id || now.duration_since(*at) >= ASK_AGAIN,
            None => true,
        };
        if due {
            ask(c, &phone.id).await;
            self.asked = Some((phone.id.clone(), now));
        }
        let waiting = self
            .asked
            .as_ref()
            .is_some_and(|(_, at)| now.duration_since(*at) < ANSWER);
        if messages.is_empty() && waiting && !self.kept.contains_key(&phone.id) {
            return State::Asking { phone: phone.name };
        }
        let contacts = self.contacts(&phone.id).clone();
        let list = threads(messages, &contacts);
        if list.is_empty() {
            // Keep what was shown rather than blank the card while the
            // phone reconnects and the daemon's cache refills.
            if let Some(kept) = self.kept.get(&phone.id).filter(|_| waiting) {
                return State::Ready {
                    phone: phone.name,
                    threads: kept.clone(),
                };
            }
        } else {
            self.kept.insert(phone.id.clone(), list.clone());
        }
        State::Ready {
            phone: phone.name,
            threads: list,
        }
    }

    /// When to look again without a signal: soon while the phone is asked,
    /// else when it should be asked again.
    fn next_look(&self) -> Duration {
        match &self.asked {
            Some((_, at)) => {
                let since = at.elapsed();
                if since < ANSWER {
                    ANSWER - since + Duration::from_millis(200)
                } else {
                    ASK_AGAIN.saturating_sub(since).max(Duration::from_secs(5))
                }
            }
            None => Duration::from_secs(60),
        }
    }
}

async fn observe(out: &mut cosmic::iced::futures::channel::mpsc::Sender<State>) -> Option<()> {
    let c = tokio::time::timeout(Duration::from_secs(3), Connection::session())
        .await
        .ok()?
        .ok()?;
    let mut changes = watch(&c).await?;
    let mut follower = Follower {
        data: data_home(),
        ..Default::default()
    };
    let mut sent: Option<State> = None;
    loop {
        let state = follower.read(&c).await;
        if sent.as_ref() != Some(&state) {
            sent = Some(state.clone());
            out.send(state).await.ok()?;
        }
        let deadline = tokio::time::sleep(follower.next_look());
        tokio::pin!(deadline);
        let mut wanted = false;
        while !wanted {
            tokio::select! {
                msg = changes.next() => wanted = relevant(&msg?.ok()?),
                _ = &mut deadline => wanted = true,
            }
        }
        // A phone's answer comes as a burst of signals; read once after it.
        tokio::time::sleep(Duration::from_millis(400)).await;
        for _ in 0..256 {
            match changes.next().now_or_never() {
                Some(Some(Ok(_))) => {}
                Some(Some(Err(_))) | Some(None) => return None,
                None => break,
            }
        }
    }
}

fn feed() -> impl Stream<Item = State> {
    cosmic::iced::stream::channel(2, async move |mut out| {
        loop {
            observe(&mut out).await;
            if out.is_closed() {
                return;
            }
            // The bus went away; say what is known, then try again.
            if out.send(State::NoService).await.is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    })
}

/// The phone's conversations, sent whenever they or the phone change.
pub fn subscription() -> Subscription<State> {
    if crate::tools::preview() {
        return Subscription::none();
    }
    Subscription::run(feed)
}

/// Example conversations for previews, so the look can be judged without a
/// phone.
pub fn sample() -> State {
    let now = chrono::Utc::now().timestamp_millis();
    let minutes = |n: i64| now - n * 60_000;
    State::Ready {
        phone: "Pixel 8".into(),
        threads: vec![
            Thread {
                id: 1,
                who: "Sam Rivera".into(),
                body: "Running ten minutes late, save me a seat".into(),
                at_ms: minutes(3),
                unread: true,
                ..Default::default()
            },
            Thread {
                id: 2,
                who: "Mum".into(),
                body: "Call me when you’re home".into(),
                at_ms: minutes(48),
                unread: true,
                ..Default::default()
            },
            Thread {
                id: 3,
                who: "Alex Chen".into(),
                body: "Sounds good, see you Friday".into(),
                at_ms: minutes(190),
                mine: true,
                ..Default::default()
            },
            Thread {
                id: 4,
                who: "+44 7700 900123".into(),
                attachment: true,
                at_ms: minutes(60 * 26),
                ..Default::default()
            },
        ],
    }
}

#[cfg(test)]
#[path = "messages/tests.rs"]
mod tests;
