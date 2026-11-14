//! A real private DBus service for transport and watcher regressions.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering},
};

/// The shared private bus: nothing on it can start a desktop service.
pub(crate) use crate::test_env::PrivateBus as Bus;

pub(crate) fn isolated(test: &str, body: impl FnOnce()) {
    if std::env::var("DESKTOP_CARDS_MPRIS_TEST").as_deref() == Ok(test) {
        body();
        return;
    }
    let bus = Bus::new();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env("DESKTOP_CARDS_MPRIS_TEST", test)
        .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
        .status()
        .unwrap();
    assert!(status.success(), "private media regression failed");
}

pub(crate) struct State {
    pub playing: AtomicBool,
    pub length: AtomicI64,
    pub position: AtomicI64,
    pub track: AtomicUsize,
    pub can_skip: AtomicBool,
    pub details: AtomicBool,
    pub broken_timing: AtomicBool,
}

impl State {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            playing: AtomicBool::new(false),
            length: AtomicI64::new(300_000_000),
            position: AtomicI64::new(60_000_000),
            track: AtomicUsize::new(1),
            can_skip: AtomicBool::new(true),
            details: AtomicBool::new(true),
            broken_timing: AtomicBool::new(false),
        })
    }
}

struct Root;
#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    #[zbus(property)]
    fn identity(&self) -> &str {
        "Browser Fixture"
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }
}

struct Transport(Arc<State>);
#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Transport {
    fn play_pause(&self) {
        self.0.playing.fetch_xor(true, Ordering::SeqCst);
    }
    fn play(&self) {
        self.0.playing.store(true, Ordering::SeqCst);
    }
    fn pause(&self) {
        self.0.playing.store(false, Ordering::SeqCst);
    }
    fn next(&self) {
        if self.0.can_skip.load(Ordering::SeqCst) {
            self.0.track.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn previous(&self) {
        if self.0.can_skip.load(Ordering::SeqCst) {
            self.0.track.fetch_sub(1, Ordering::SeqCst);
        }
    }
    fn set_position(&self, track: zbus::zvariant::OwnedObjectPath, position: i64) {
        if track.as_str() == "/org/mpris/MediaPlayer2/firefox"
            && position < self.0.length.load(Ordering::SeqCst)
        {
            self.0.position.store(position, Ordering::SeqCst);
        }
    }
    #[zbus(property)]
    fn playback_status(&self) -> &str {
        if self.0.playing.load(Ordering::SeqCst) {
            "Playing"
        } else {
            "Paused"
        }
    }
    #[zbus(property)]
    fn position(&self) -> i64 {
        if self.0.broken_timing.load(Ordering::SeqCst) {
            0
        } else {
            self.0.position.load(Ordering::SeqCst)
        }
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.0.can_skip.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.0.can_skip.load(Ordering::SeqCst)
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let track = self.0.track.load(Ordering::SeqCst);
        let mut metadata = HashMap::from([
            (
                "xesam:title".into(),
                OwnedValue::from(zbus::zvariant::Str::from(format!("Video {track}"))),
            ),
            (
                "xesam:url".into(),
                OwnedValue::from(zbus::zvariant::Str::from(format!(
                    "https://www.youtube.com/watch?v={track}"
                ))),
            ),
            (
                "mpris:trackid".into(),
                OwnedValue::from(
                    zbus::zvariant::ObjectPath::try_from("/org/mpris/MediaPlayer2/firefox")
                        .unwrap(),
                ),
            ),
        ]);
        let length = self.0.length.load(Ordering::SeqCst);
        if !self.0.details.load(Ordering::SeqCst) {
            metadata.remove("xesam:title");
            metadata.remove("xesam:url");
        }
        if length > 0 && !self.0.broken_timing.load(Ordering::SeqCst) {
            metadata.insert("mpris:length".into(), OwnedValue::from(length));
        }
        metadata
    }
}

pub(crate) async fn serve(address: &str, name: &str, state: Arc<State>) -> zbus::Connection {
    zbus::connection::Builder::address(address)
        .unwrap()
        .name(name)
        .unwrap()
        .serve_at(PATH, Root)
        .unwrap()
        .serve_at(PATH, Transport(state))
        .unwrap()
        .build()
        .await
        .unwrap()
}
