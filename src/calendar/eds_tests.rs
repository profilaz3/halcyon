//! A fake Evolution Data Server on a private dbus-daemon. Nothing here
//! touches the session bus of the person running the tests.
use super::*;
use crate::activity_island::test_bus::Bus as Daemon;
use std::sync::{Arc, Mutex};

struct Source {
    uid: String,
    data: String,
}
#[zbus::interface(name = "org.gnome.evolution.dataserver.Source")]
impl Source {
    #[zbus(property, name = "UID")]
    fn uid(&self) -> String {
        self.uid.clone()
    }
    #[zbus(property)]
    fn data(&self) -> String {
        self.data.clone()
    }
}

struct Factory {
    calendars: Vec<String>,
}
#[zbus::interface(name = "org.gnome.evolution.dataserver.CalendarFactory")]
impl Factory {
    fn open_calendar(&self, uid: &str) -> zbus::fdo::Result<(String, String)> {
        let index = self
            .calendars
            .iter()
            .position(|known| known == uid)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("No such source for UID {uid}")))?;
        Ok((calendar_path(index), CALENDARS.into()))
    }
}
fn calendar_path(index: usize) -> String {
    format!("/org/gnome/evolution/dataserver/Subprocess/1/{index}")
}

#[derive(Clone, Default)]
struct Seen {
    queries: Arc<Mutex<Vec<String>>>,
    zones: Arc<Mutex<Vec<String>>>,
    opened: Arc<Mutex<usize>>,
}
struct Calendar {
    objects: Vec<String>,
    delay: Duration,
    seen: Seen,
}
#[zbus::interface(name = "org.gnome.evolution.dataserver.Calendar")]
impl Calendar {
    fn open(&self) -> Vec<String> {
        *self.seen.opened.lock().unwrap() += 1;
        vec![]
    }
    async fn get_object_list(&self, query: String) -> Vec<String> {
        self.seen.queries.lock().unwrap().push(query);
        tokio::time::sleep(self.delay).await;
        self.objects.clone()
    }
    fn get_timezone(&self, tzid: &str) -> zbus::fdo::Result<String> {
        self.seen.zones.lock().unwrap().push(tzid.into());
        let location = tzid
            .strip_prefix("/example.org/zones/")
            .ok_or_else(|| zbus::fdo::Error::Failed("unknown zone".into()))?;
        Ok(format!(
            "BEGIN:VTIMEZONE\r\nTZID:{tzid}\r\nX-LIC-LOCATION:{location}\r\nBEGIN:STANDARD\r\nTZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\nDTSTART:19701025T030000\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n"
        ))
    }
}

/// One EDS: the source registry and a calendar factory with these calendars.
struct Fake {
    _daemon: Daemon,
    _server: zbus::Connection,
    address: String,
    seen: Seen,
}
async fn fake(
    sources: &[(&str, &str)],
    calendars: Vec<(&str, Vec<String>)>,
    delay: Duration,
) -> Fake {
    let daemon = Daemon::new();
    let server = zbus::connection::Builder::address(daemon.address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let objects = server.object_server();
    objects
        .at(SOURCE_MANAGER, zbus::fdo::ObjectManager)
        .await
        .unwrap();
    for (index, (uid, data)) in sources.iter().enumerate() {
        objects
            .at(
                format!("{SOURCE_MANAGER}/Source_{index}"),
                Source {
                    uid: (*uid).into(),
                    data: (*data).into(),
                },
            )
            .await
            .unwrap();
    }
    let seen = Seen::default();
    objects
        .at(
            FACTORY_PATH,
            Factory {
                calendars: calendars.iter().map(|(uid, _)| (*uid).to_owned()).collect(),
            },
        )
        .await
        .unwrap();
    for (index, (_, objects_list)) in calendars.into_iter().enumerate() {
        objects
            .at(
                calendar_path(index),
                Calendar {
                    objects: objects_list,
                    delay,
                    seen: seen.clone(),
                },
            )
            .await
            .unwrap();
    }
    server.request_name(SOURCES).await.unwrap();
    server.request_name(CALENDARS).await.unwrap();
    Fake {
        address: daemon.address.clone(),
        _daemon: daemon,
        _server: server,
        seen,
    }
}
async fn client(address: &str, timeout: Duration) -> Bus {
    Bus {
        connection: zbus::connection::Builder::address(address)
            .unwrap()
            .build()
            .await
            .unwrap(),
        timeout,
    }
}

const ACCOUNT: &str = "[Data Source]\nDisplayName=alex@example.com\nEnabled=true\nParent=\n\n[Collection]\nBackendName=google\nCalendarEnabled=true\n";
const ACCOUNT_OFF: &str = "[Data Source]\nDisplayName=old@example.com\nEnabled=true\n\n[Collection]\nCalendarEnabled=false\n";
const CONTACTS: &str = "[Data Source]\nDisplayName=Contacts\nEnabled=true\n";
fn calendar_source(name: &str, parent: &str, color: &str, enabled: bool) -> String {
    format!(
        "[Data Source]\nDisplayName={name}\nDisplayName[hr]=Prijevod\nEnabled={enabled}\nParent={parent}\n\n[Authentication]\nUser=alex@example.com\n\n[Calendar]\nBackendName=caldav\nColor={color}\nSelected=true\n"
    )
}
fn sources() -> Vec<(&'static str, String)> {
    vec![
        ("acct", ACCOUNT.into()),
        ("acct-off", ACCOUNT_OFF.into()),
        ("contacts-stub", CONTACTS.into()),
        ("work", calendar_source("Work\\splans", "acct", "#3465A4", true)),
        ("home", calendar_source("Home", "acct", "#fc0", true)),
        ("disabled", calendar_source("Switched off", "acct", "#000000", false)),
        ("hidden", calendar_source("Old account", "acct-off", "#111111", true)),
        (
            "birthdays",
            "[Data Source]\nDisplayName=Birthdays & Anniversaries\nEnabled=true\nParent=contacts-stub\n\n[Calendar]\nBackendName=contacts\nColor=#ffbe6f\n".into(),
        ),
        (
            "addressbook",
            "[Data Source]\nDisplayName=Address book\nEnabled=true\nParent=acct\n\n[Address Book]\nBackendName=carddav\n".into(),
        ),
        (
            "tasks",
            "[Data Source]\nDisplayName=My Tasks\nEnabled=true\nParent=acct\n\n[Task List]\nColor=#62a0ea\n".into(),
        ),
    ]
}

#[tokio::test]
async fn sources_list_calendars_with_colors_and_accounts_and_hide_disabled_ones() {
    let owned = sources();
    let list: Vec<(&str, &str)> = owned.iter().map(|(u, d)| (*u, d.as_str())).collect();
    let fake = fake(&list, vec![], Duration::ZERO).await;
    let bus = client(&fake.address, Duration::from_secs(5)).await;
    let found = super::sources(&bus).await.unwrap();
    assert_eq!(
        found,
        [
            SystemCalendar {
                uid: "birthdays".into(),
                name: "Birthdays & Anniversaries".into(),
                color: "#ffbe6f".into(),
                account: "Contacts".into(),
            },
            SystemCalendar {
                uid: "home".into(),
                name: "Home".into(),
                color: "#ffcc00".into(),
                account: "alex@example.com".into(),
            },
            SystemCalendar {
                uid: "work".into(),
                name: "Work plans".into(),
                color: "#3465a4".into(),
                account: "alex@example.com".into(),
            },
        ]
    );
    assert_eq!(found[2].choice().id, "eds:work");
    assert_eq!(uid(&found[2].choice().id), Some("work"));
}

#[tokio::test]
async fn events_ask_for_the_month_range_and_recurrences_go_through_the_isolated_parser() {
    // A weekly series in a zone EDS names its own way, one moved occurrence,
    // and an object that comes wrapped in its own VCALENDAR.
    let series = "BEGIN:VEVENT\r\nUID:standup\r\nDTSTART;TZID=/example.org/zones/Europe/Zagreb:20261005T090000\r\nDTEND;TZID=/example.org/zones/Europe/Zagreb:20261005T093000\r\nRRULE:FREQ=WEEKLY;COUNT=4\r\nSUMMARY:Stand-up with a long title that EDS folds across\r\n  two lines\r\nBEGIN:VALARM\r\nACTION:DISPLAY\r\nTRIGGER:-PT5M\r\nEND:VALARM\r\nEND:VEVENT\r\n".to_string();
    let moved = "BEGIN:VEVENT\r\nUID:standup\r\nRECURRENCE-ID;TZID=/example.org/zones/Europe/Zagreb:20261012T090000\r\nDTSTART;TZID=Europe/Zagreb:20261013T100000\r\nDTEND;TZID=Europe/Zagreb:20261013T103000\r\nSUMMARY:Moved stand-up\r\nEND:VEVENT\r\n".to_string();
    let wrapped = "BEGIN:VCALENDAR\nVERSION:2.0\nPRODID:-//Other//EN\nBEGIN:VEVENT\nUID:review\nDTSTART:20261020T120000Z\nDURATION:PT1H\nSUMMARY:Review\nEND:VEVENT\nEND:VCALENDAR\n".to_string();
    let fake = fake(
        &[("work", &calendar_source("Work", "", "#3465a4", true))],
        vec![("work", vec![series, moved, wrapped])],
        Duration::ZERO,
    )
    .await;
    let bus = client(&fake.address, Duration::from_secs(5)).await;
    let choice = SystemCalendar {
        uid: "work".into(),
        name: "Work".into(),
        color: "#3465a4".into(),
        account: String::new(),
    }
    .choice();
    let (events, problem) = events_on(&bus, &[choice], "2026-10").await;
    assert_eq!(problem, None);
    assert_eq!(*fake.seen.opened.lock().unwrap(), 1);
    // The month view's six weeks, as UTC instants of the local window.
    let (from, to) = google::window("2026-10").unwrap();
    let utc = |v: &str| {
        chrono::DateTime::parse_from_rfc3339(v)
            .unwrap()
            .with_timezone(&chrono::Utc)
            .format("%Y%m%dT%H%M%SZ")
            .to_string()
    };
    assert_eq!(
        *fake.seen.queries.lock().unwrap(),
        [format!(
            "(occur-in-time-range? (make-time \"{}\") (make-time \"{}\") \"UTC\")",
            utc(&from),
            utc(&to)
        )]
    );
    assert!(utc(&from).starts_with("202609") && utc(&to).starts_with("202611"));
    // Only the unknown zone name was looked up.
    assert_eq!(
        *fake.seen.zones.lock().unwrap(),
        ["/example.org/zones/Europe/Zagreb"]
    );
    let got: Vec<_> = events
        .iter()
        .map(|e| (e.start.as_str(), e.title.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            (
                "2026-10-05T09:00:00+02:00",
                "Stand-up with a long title that EDS folds across two lines"
            ),
            ("2026-10-13T10:00:00+02:00", "Moved stand-up"),
            (
                "2026-10-19T09:00:00+02:00",
                "Stand-up with a long title that EDS folds across two lines"
            ),
            ("2026-10-20T12:00:00+00:00", "Review"),
            (
                "2026-10-26T09:00:00+01:00",
                "Stand-up with a long title that EDS folds across two lines"
            ),
        ]
    );
    assert!(
        events
            .iter()
            .all(|e| e.calendar_name == "Work" && e.calendar_color == "#3465a4")
    );
}

#[tokio::test]
async fn a_missing_eds_reports_system_calendars_unavailable() {
    let daemon = Daemon::new();
    let bus = client(&daemon.address, Duration::from_secs(5)).await;
    let started = std::time::Instant::now();
    assert_eq!(super::sources(&bus).await, Err(Problem::SystemUnavailable));
    let choice = CalendarChoice {
        id: "eds:work".into(),
        name: "Work".into(),
        primary: false,
        color: String::new(),
    };
    let (events, problem) = events_on(&bus, &[choice.clone(), choice], "2026-10").await;
    assert!(events.is_empty());
    assert_eq!(problem, Some(Problem::SystemUnavailable));
    assert!(started.elapsed() < Duration::from_secs(3), "no call waited");
    assert_eq!(
        Problem::SystemUnavailable.to_string(),
        "System calendars are not available."
    );
}

#[tokio::test]
async fn a_removed_calendar_leaves_the_others() {
    let event =
        "BEGIN:VEVENT\r\nUID:a\r\nDTSTART:20261020T100000Z\r\nSUMMARY:Kept\r\nEND:VEVENT\r\n";
    let fake = fake(&[], vec![("home", vec![event.into()])], Duration::ZERO).await;
    let bus = client(&fake.address, Duration::from_secs(5)).await;
    let choice = |uid: &str| CalendarChoice {
        id: format!("eds:{uid}"),
        name: uid.into(),
        primary: false,
        color: String::new(),
    };
    let (events, problem) = events_on(&bus, &[choice("gone"), choice("home")], "2026-10").await;
    assert_eq!(problem, Some(Problem::SystemUnavailable));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].title, "Kept");
}

#[tokio::test]
async fn a_hung_eds_times_out_every_call_and_stops_at_the_first() {
    let fake = fake(
        &[],
        vec![("a", vec![]), ("b", vec![])],
        Duration::from_secs(30),
    )
    .await;
    let bus = client(&fake.address, Duration::from_millis(300)).await;
    let choice = |uid: &str| CalendarChoice {
        id: format!("eds:{uid}"),
        name: uid.into(),
        primary: false,
        color: String::new(),
    };
    let started = std::time::Instant::now();
    let (events, problem) = events_on(&bus, &[choice("a"), choice("b")], "2026-10").await;
    assert!(events.is_empty());
    assert_eq!(problem, Some(Problem::TimedOut));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(
        fake.seen.queries.lock().unwrap().len(),
        1,
        "b was not tried"
    );
}

/// The refresh path of a list with only system calendars, against a session
/// bus with no EDS on it: the card reports the problem instead of an empty month.
#[test]
fn a_system_only_calendar_list_without_eds_fails_its_refresh_plainly() {
    const CHILD: &str = "DESKTOP_CARDS_EDS_MISSING_TEST";
    if std::env::var_os(CHILD).is_none() {
        let daemon = Daemon::new();
        let root = std::env::temp_dir().join(format!("cards-eds-{}", store::nonce().unwrap()));
        std::fs::create_dir_all(&root).unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "calendar::eds::tests::a_system_only_calendar_list_without_eds_fails_its_refresh_plainly",
            ])
            .env(CHILD, "1")
            .env("DESKTOP_CARDS_CONFIG", root.join("config.json"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("DBUS_SESSION_BUS_ADDRESS", &daemon.address)
            .output()
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        return;
    }
    assert!(
        !std::env::var("DBUS_SESSION_BUS_ADDRESS")
            .unwrap()
            .contains("/run/user/")
    );
    let calendar = SystemCalendar {
        uid: "work".into(),
        name: "Work".into(),
        color: "#3465a4".into(),
        account: "alex@example.com".into(),
    };
    let connection = with_system(&Connection::default(), &calendar).unwrap();
    store::replace(&Connection::default(), &connection).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = runtime.block_on(super::super::fetch(connection.clone(), "2026-10".into()));
    assert_eq!(result.unwrap_err(), "System calendars are not available.");
    assert!(store::cached(&connection, "2026-10").is_none());
}

#[test]
fn system_calendars_join_the_list_with_only_uid_name_and_color() {
    let calendar = |uid: &str| SystemCalendar {
        uid: uid.into(),
        name: format!("Calendar {uid}"),
        color: "#3465a4".into(),
        account: "private-account@example.com".into(),
    };
    // From nothing: a list of one, with no keyring item.
    let first = with_system(&Connection::default(), &calendar("a")).unwrap();
    assert_eq!(first.backend, Backend::Subscription);
    assert!(first.connected() && first.credential.is_empty());
    assert_eq!(first.selected_calendars(), [calendar("a").choice()]);
    let saved = serde_json::to_string(&first).unwrap();
    assert!(!saved.contains("private-account"), "{saved}");
    // Adding the same one again changes nothing.
    assert_eq!(with_system(&first, &calendar("a")).unwrap(), first);
    // Beside a link calendar, the link keeps its keyring item.
    let link = CalendarChoice {
        id: "link-nonce".into(),
        name: "Team".into(),
        primary: false,
        color: String::new(),
    };
    let mixed = Connection {
        credential: "keyring-id".into(),
        subscriptions: vec![link.clone()],
        selections: vec![link.clone()],
        ..first.clone()
    };
    let both = with_system(&mixed, &calendar("b")).unwrap();
    assert_eq!(both.credential, "keyring-id");
    assert_eq!(
        both.subscriptions
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        ["link-nonce", "eds:b"]
    );
    assert_eq!(both.selections.len(), 2);
    assert_ne!(both.revision, mixed.revision);
    // Eight in all, links and system calendars together.
    let mut full = both.clone();
    for n in 0..6 {
        full = with_system(&full, &calendar(&n.to_string())).unwrap();
    }
    assert_eq!(full.subscriptions.len(), MAX_CALENDARS);
    assert_eq!(
        with_system(&full, &calendar("ninth")),
        Err(Problem::CalendarLimit)
    );
    // A Google connection is replaced, as a link replaces it.
    let google = Connection {
        backend: Backend::Google,
        account: "alex@example.com".into(),
        calendar_id: "primary".into(),
        credential: "google-keyring-id".into(),
        revision: "r".into(),
        ..Default::default()
    };
    let replaced = with_system(&google, &calendar("a")).unwrap();
    assert_eq!(replaced.backend, Backend::Subscription);
    assert!(replaced.credential.is_empty());
    // A disconnect still cleaning up comes first.
    let pending = Connection {
        credential: "left".into(),
        ..Default::default()
    };
    assert_eq!(with_system(&pending, &calendar("a")), Err(Problem::Cleanup));
}

#[test]
fn keyfiles_colors_and_ids_read_the_way_eds_writes_them() {
    let file = keyfile(
        "# comment\n[Data Source]\nDisplayName=A\\sb\\\\c\nDisplayName[de]=X\n\n[Calendar]\nColor = #ABCDEF\n",
    );
    assert_eq!(get(&file, "Data Source", "DisplayName"), Some("A b\\c"));
    assert_eq!(get(&file, "Calendar", "Color"), Some("#ABCDEF"));
    assert_eq!(color(Some("#ABCDEF")), "#abcdef");
    assert_eq!(color(Some("#abc")), "#aabbcc");
    assert_eq!(color(Some("#1111aaaaffff")), "#11aaff");
    for bad in ["red", "#12345", "#ggg", ""] {
        assert_eq!(color(Some(bad)), "");
    }
    assert_eq!(uid("eds:work"), Some("work"));
    assert_eq!(uid("eds:"), None);
    assert!(!is_system("plain-nonce_-"));
    assert_eq!(
        strip_zone_prefix("/freeassociation.sourceforge.net/Europe/Zagreb").as_deref(),
        Some("Europe/Zagreb")
    );
    assert_eq!(
        strip_zone_prefix("/freeassociation.sourceforge.net/Tzfile/America/New_York").as_deref(),
        Some("America/New_York")
    );
    assert_eq!(strip_zone_prefix("Europe/Zagreb"), None);
    assert_eq!(
        rename_zone(
            "DTSTART;VALUE=DATE-TIME;TZID=\"/x.org/Europe/Zagreb\";X=1:20261005T090000",
            &BTreeMap::from([(
                "/x.org/Europe/Zagreb".to_string(),
                (Some("Europe/Zagreb".to_string()), None)
            )])
        ),
        "DTSTART;VALUE=DATE-TIME;TZID=Europe/Zagreb;X=1:20261005T090000"
    );
    // A colon inside a quoted parameter is not the value separator.
    assert_eq!(
        head("ATTENDEE;CN=\"A: B\":mailto:a@example.com"),
        "ATTENDEE;CN=\"A: B\""
    );
}
