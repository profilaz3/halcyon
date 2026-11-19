use super::*;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

/// A message as KDE Connect 23.08 and later marshals it.
type Wire = (
    i32,
    String,
    Vec<(String,)>,
    i64,
    i32,
    i32,
    i64,
    i32,
    i64,
    Vec<(i64, String, String, String)>,
);

fn wire(thread: i64, from: &[&str], body: &str, date: i64, kind: i32, read: bool) -> Wire {
    (
        TEXT_EVENT,
        body.into(),
        from.iter().map(|a| (a.to_string(),)).collect(),
        date,
        kind,
        i32::from(read),
        thread,
        thread as i32,
        -1,
        Vec::new(),
    )
}

fn owned(message: Wire) -> OwnedValue {
    OwnedValue::try_from(Value::from(message)).unwrap()
}

#[test]
fn reads_messages_as_kde_connect_marshals_them() {
    let received = owned(wire(
        7,
        &["+44 7700 900123"],
        "See you  at\nnine",
        1_700,
        1,
        false,
    ));
    let m = message(&received).unwrap();
    assert_eq!(m.thread, 7);
    assert_eq!(m.addresses, ["+44 7700 900123"]);
    assert_eq!((m.date, m.kind, m.read), (1_700, 1, false));
    assert_eq!(one_line(&m.body), "See you at nine");

    // A picture with no text, and an older daemon without subID and
    // attachments, both read.
    let mut picture = wire(8, &["555 0100"], "", 1_800, 1, true);
    picture.0 = 0;
    picture.9 = vec![(1, "image/jpeg".into(), String::new(), "a.jpg".into())];
    let picture = message(&owned(picture)).unwrap();
    assert_eq!(picture.attachments, 1);
    let old = OwnedValue::try_from(Value::from((
        1i32,
        "Hi".to_string(),
        vec![("555 0100".to_string(),)],
        1_900i64,
        2i32,
        1i32,
        9i64,
        3i32,
    )))
    .unwrap();
    let old = message(&old).unwrap();
    assert_eq!((old.thread, old.kind, old.attachments), (9, 2, 0));

    // Anything else is skipped, not misread.
    assert_eq!(message(&OwnedValue::from(5i32)), None);
    let short = OwnedValue::try_from(Value::from((1i32, "Hi".to_string()))).unwrap();
    assert_eq!(message(&short), None);
}

#[test]
fn long_text_is_cut_on_one_line() {
    let long = "word ".repeat(200);
    let cut = one_line(&long);
    assert_eq!(cut.chars().count(), MOST_TEXT + 1);
    assert!(cut.ends_with('…'));
    assert!(!cut.contains('\n'));
}

#[test]
fn contacts_name_numbers_however_they_are_written() {
    let mut contacts = Contacts::default();
    contacts.add_vcard(
        "BEGIN:VCARD\r\nVERSION:2.1\r\nFN:Sam Rivera\r\nTEL;CELL:+44 7700 900123\r\n\
         TEL;HOME:020 7946\r\n 0018\r\nPHOTO;ENCODING=BASE64:AAAA\r\nEND:VCARD\r\n",
    );
    contacts.add_vcard("BEGIN:VCARD\nitem1.TEL:555-0100\nFN:O\\, Brien\nEND:VCARD\n");
    contacts.add_vcard("BEGIN:VCARD\nTEL:555-0199\nEND:VCARD\n");
    assert_eq!(contacts.name("07700 900123"), "Sam Rivera");
    assert_eq!(contacts.name("+447700900123"), "Sam Rivera");
    // A folded line joins before it is read.
    assert_eq!(contacts.name("02079460018"), "Sam Rivera");
    assert_eq!(contacts.name("555 0100"), "O, Brien");
    // No name, or no contact, is the number as the phone gave it.
    assert_eq!(contacts.name(" 555-0199 "), "555-0199");
    assert_eq!(contacts.name("Bank"), "Bank");
}

#[test]
fn contacts_come_from_the_phone_folder_only() {
    let data = crate::test_env::TempPath::new("messages");
    let dir = data.root().join("kpeoplevcard/kdeconnect-abc_123");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("1.vcf"),
        "BEGIN:VCARD\nFN:Mum\nTEL:+1 202 555 0143\nEND:VCARD\n",
    )
    .unwrap();
    std::fs::write(dir.join("notes.txt"), "FN:Not a contact\nTEL:999\n").unwrap();
    let contacts = Contacts::read(data.root(), "abc_123");
    assert_eq!(contacts.name("2025550143"), "Mum");
    assert_eq!(contacts.name("999"), "999");
    // An id that isn't a plain name is never a path.
    assert_eq!(contacts_dir(data.root(), "../../etc"), None);
    assert_eq!(Contacts::read(data.root(), "missing"), Contacts::default());
}

#[test]
fn conversations_are_newest_first_with_names() {
    let mut contacts = Contacts::default();
    contacts.add_vcard("BEGIN:VCARD\nFN:Sam\nTEL:555 0100\nEND:VCARD\n");
    contacts.add_vcard("BEGIN:VCARD\nFN:Alex\nTEL:555 0101\nEND:VCARD\n");
    let messages = [
        wire(1, &["5550100"], "older", 100, 1, true),
        wire(1, &["5550100"], "newest from Sam", 300, 1, false),
        wire(2, &["5550101"], "sent", 200, 2, true),
        wire(
            3,
            &["5550100", "5550101", "5550102", "5550103"],
            "group",
            50,
            1,
            true,
        ),
    ]
    .into_iter()
    .map(|w| message(&owned(w)).unwrap())
    .collect();
    let list = threads(messages, &contacts);
    let shown: Vec<_> = list
        .iter()
        .map(|t| (t.who.as_str(), t.body.as_str(), t.mine, t.unread))
        .collect();
    assert_eq!(
        shown,
        [
            ("Sam", "newest from Sam", false, true),
            ("Alex", "sent", true, false),
            ("Sam, Alex and 2 more", "group", false, false),
        ]
    );
}

#[test]
fn the_connected_phone_with_messages_is_followed() {
    let phone = |id: &str, reachable, sms| Phone {
        id: id.into(),
        name: id.into(),
        reachable,
        sms,
    };
    let list = [
        phone("away", false, false),
        phone("tablet", true, false),
        phone("phone", true, true),
    ];
    assert_eq!(choose(&list).unwrap().id, "phone");
    assert_eq!(choose(&list[..2]).unwrap().id, "tablet");
    assert_eq!(choose(&list[..1]).unwrap().id, "away");
    assert_eq!(choose(&[]), None);
}

/// A fake KDE Connect: the daemon, one phone and its conversations.
#[derive(Default)]
struct Fake {
    reachable: AtomicBool,
    sms: AtomicBool,
    asked: AtomicUsize,
    messages: Mutex<Vec<Wire>>,
    read_failed: AtomicBool,
}

struct Daemon(Arc<Fake>);
#[zbus::interface(name = "org.kde.kdeconnect.daemon")]
impl Daemon {
    #[zbus(name = "devices")]
    fn devices(&self, only_reachable: bool, only_paired: bool) -> Vec<String> {
        assert!(only_paired);
        if only_reachable && !self.0.reachable.load(Ordering::SeqCst) {
            return Vec::new();
        }
        vec!["abc_123".into()]
    }
}

struct Device(Arc<Fake>);
#[zbus::interface(name = "org.kde.kdeconnect.device")]
impl Device {
    #[zbus(property, name = "name")]
    fn name(&self) -> String {
        "Pixel 8".into()
    }
    #[zbus(property, name = "isReachable")]
    fn is_reachable(&self) -> bool {
        self.0.reachable.load(Ordering::SeqCst)
    }
    #[zbus(name = "loadedPlugins")]
    fn loaded_plugins(&self) -> Vec<String> {
        let mut plugins = vec!["kdeconnect_battery".to_string()];
        if self.0.sms.load(Ordering::SeqCst) {
            plugins.push(SMS_PLUGIN.into());
        }
        plugins
    }
}

struct Conversations(Arc<Fake>);
#[zbus::interface(name = "org.kde.kdeconnect.device.conversations")]
impl Conversations {
    #[zbus(name = "activeConversations")]
    fn active_conversations(&self) -> zbus::fdo::Result<Vec<OwnedValue>> {
        if self.0.read_failed.load(Ordering::SeqCst) {
            return Err(zbus::fdo::Error::Failed("Simulated read failure".into()));
        }
        Ok(self
            .0
            .messages
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .map(owned)
            .collect())
    }
    #[zbus(name = "requestAllConversationThreads")]
    fn request_all_conversation_threads(&self) {
        self.0.asked.fetch_add(1, Ordering::SeqCst);
    }
}

async fn serve(address: &str, fake: Arc<Fake>) -> zbus::Connection {
    zbus::connection::Builder::address(address)
        .unwrap()
        .name(SERVICE)
        .unwrap()
        .serve_at(DAEMON, Daemon(fake.clone()))
        .unwrap()
        .serve_at("/modules/kdeconnect/devices/abc_123", Device(fake.clone()))
        .unwrap()
        .serve_at("/modules/kdeconnect/devices/abc_123", Conversations(fake))
        .unwrap()
        .build()
        .await
        .unwrap()
}

#[test]
fn follows_a_phone_through_kde_connect() {
    let bus = crate::test_env::PrivateBus::new();
    let data = crate::test_env::TempPath::new("messages");
    let contacts = data.root().join("kpeoplevcard/kdeconnect-abc_123");
    std::fs::create_dir_all(&contacts).unwrap();
    std::fs::write(
        contacts.join("1.vcf"),
        "BEGIN:VCARD\nFN:Sam\nTEL:555 0100\nEND:VCARD\n",
    )
    .unwrap();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let client = zbus::connection::Builder::address(bus.address.as_str())
                .unwrap()
                .build()
                .await
                .unwrap();
            let mut follower = Follower {
                data: Some(data.root().to_path_buf()),
                ..Default::default()
            };
            // Nothing running yet.
            assert_eq!(follower.read(&client).await, State::NoService);

            let fake = Arc::new(Fake::default());
            let _service = serve(&bus.address, fake.clone()).await;
            assert_eq!(
                follower.read(&client).await,
                State::Away {
                    phone: "Pixel 8".into(),
                    threads: Vec::new()
                }
            );
            fake.reachable.store(true, Ordering::SeqCst);
            assert_eq!(
                follower.read(&client).await,
                State::NoSms {
                    phone: "Pixel 8".into()
                }
            );
            assert_eq!(fake.asked.load(Ordering::SeqCst), 0, "no SMS, no asking");

            // Connected with SMS: the phone is asked once, then answers.
            fake.sms.store(true, Ordering::SeqCst);
            assert_eq!(
                follower.read(&client).await,
                State::Asking {
                    phone: "Pixel 8".into()
                }
            );
            assert_eq!(fake.asked.load(Ordering::SeqCst), 1);
            *fake.messages.lock().unwrap() = vec![
                wire(1, &["+1 555 0100"], "Late, save me a seat", 2_000, 1, false),
                wire(2, &["555 0199"], "Thanks!", 1_000, 2, true),
            ];
            let ready = follower.read(&client).await;
            assert_eq!(fake.asked.load(Ordering::SeqCst), 1, "asked once");
            assert_eq!(ready.phone(), Some("Pixel 8"));
            assert_eq!(ready.unread(), 1);
            let shown: Vec<_> = ready
                .threads()
                .iter()
                .map(|t| (t.who.as_str(), t.body.as_str(), t.mine))
                .collect();
            assert_eq!(
                shown,
                [
                    ("Sam", "Late, save me a seat", false),
                    ("555 0199", "Thanks!", true)
                ]
            );

            // Away, the conversations read stay; back, it is asked again.
            fake.reachable.store(false, Ordering::SeqCst);
            fake.messages.lock().unwrap().clear();
            let away = follower.read(&client).await;
            assert!(matches!(&away, State::Away { threads, .. } if threads.len() == 2));
            fake.reachable.store(true, Ordering::SeqCst);
            let back = follower.read(&client).await;
            assert_eq!(fake.asked.load(Ordering::SeqCst), 2);
            assert!(
                matches!(&back, State::Ready { threads, .. } if threads.len() == 2),
                "kept while the phone answers: {back:?}"
            );
            follower.asked.as_mut().unwrap().1 -= ANSWER + Duration::from_secs(1);
            fake.read_failed.store(true, Ordering::SeqCst);
            let stale = follower.read(&client).await;
            assert!(matches!(stale, State::Stale { .. }));
            assert_eq!(stale.threads(), ready.threads());
            fake.read_failed.store(false, Ordering::SeqCst);
            let empty = follower.read(&client).await;
            assert!(matches!(empty, State::Ready { threads, .. } if threads.is_empty()));
            fake.reachable.store(false, Ordering::SeqCst);
            let away = follower.read(&client).await;
            assert!(matches!(away, State::Away { threads, .. } if threads.is_empty()));
        });
}

#[test]
fn only_conversation_and_phone_signals_wake_the_card() {
    let signal = |path: &str, interface: &str, member: &str| {
        zbus::Message::signal(path, interface, member)
            .unwrap()
            .build(&())
            .unwrap()
    };
    let phone = "/modules/kdeconnect/devices/abc_123";
    assert!(relevant(&signal(
        phone,
        CONVERSATIONS,
        "conversationUpdated"
    )));
    assert!(relevant(&signal(phone, DEVICE, "reachableChanged")));
    assert!(relevant(&signal(phone, DEVICE, "pluginsChanged")));
    assert!(relevant(&signal(
        DAEMON,
        "org.kde.kdeconnect.daemon",
        "deviceAdded"
    )));
    assert!(!relevant(&signal(
        &format!("{phone}/battery"),
        "org.kde.kdeconnect.device.battery",
        "refreshed"
    )));
}

#[test]
fn the_sample_shows_every_kind_of_row() {
    let sample = sample();
    let threads = sample.threads();
    assert!(threads.iter().any(|t| t.unread));
    assert!(threads.iter().any(|t| t.mine));
    assert!(threads.iter().any(|t| t.body.is_empty() && t.attachment));
    assert!(threads.windows(2).all(|w| w[0].at_ms >= w[1].at_ms));
}

#[test]
fn short_codes_match_only_exactly() {
    let mut contacts = Contacts::default();
    contacts.add_vcard("BEGIN:VCARD\nFN:Parking\nTEL:7275\nEND:VCARD\n");
    contacts.add_vcard("BEGIN:VCARD\nFN:Long\nTEL:+1 555 867 7275\nEND:VCARD\n");
    assert_eq!(contacts.name("7275"), "Parking");
    assert_eq!(contacts.name("17275"), "17275");
    assert_eq!(contacts.name("(555) 867-7275"), "Long");
}

/// KDE Connect installed but not running says so, and isn't started: the
/// bus lists it as one it could start, from a service file that would fail.
#[test]
fn installed_but_not_running_is_told_apart_and_left_alone() {
    use std::io::BufRead;
    let folder = crate::test_env::TempPath::new("messages-bus");
    let services = folder.root().join("services");
    std::fs::create_dir_all(&services).unwrap();
    let started = folder.root().join("started");
    std::fs::write(
        services.join("org.kde.kdeconnect.service"),
        format!(
            "[D-BUS Service]\nName=org.kde.kdeconnect\nExec=/bin/sh -c 'touch {}'\n",
            started.display()
        ),
    )
    .unwrap();
    let config = folder.root().join("bus.conf");
    std::fs::write(
        &config,
        format!(
            r#"<busconfig><type>session</type><listen>unix:path={}</listen><auth>EXTERNAL</auth><servicedir>{}</servicedir><policy context="default"><allow user="*"/><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>"#,
            folder.root().join("socket").display(),
            services.display()
        ),
    )
    .unwrap();
    let mut daemon = std::process::Command::new("dbus-daemon")
        .arg("--config-file")
        .arg(&config)
        .args(["--nofork", "--print-address"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut address = String::new();
    std::io::BufReader::new(daemon.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let state = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let client = zbus::connection::Builder::address(address.trim())
                .unwrap()
                .build()
                .await
                .unwrap();
            Follower::default().read(&client).await
        });
    let _ = daemon.kill();
    let _ = daemon.wait();
    assert_eq!(state, State::Stopped);
    assert!(!started.exists(), "KDE Connect was started");
}
