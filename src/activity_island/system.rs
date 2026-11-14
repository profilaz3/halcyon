//! Read-only, demand-driven system observations shared by the desktop owner.
use cosmic::iced::{Subscription, futures::SinkExt};
use std::time::Duration;
mod audio;
mod bluetooth;
mod bus;
mod camera;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Power,
    Audio,
    Microphone,
    Camera,
    Network,
    Bluetooth,
    Updates,
}
impl Source {
    pub const ALL: [Self; 7] = [
        Self::Power,
        Self::Audio,
        Self::Microphone,
        Self::Camera,
        Self::Network,
        Self::Bluetooth,
        Self::Updates,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Power => "Power",
            Self::Audio => "Audio",
            Self::Microphone => "Microphone",
            Self::Camera => "Camera",
            Self::Network => "Network",
            Self::Bluetooth => "Bluetooth",
            Self::Updates => "Updates",
        }
    }
    /// Shown as a dot beside the title while it lasts: a lens or a
    /// microphone is on.
    pub fn privacy(self) -> bool {
        matches!(self, Self::Microphone | Self::Camera)
    }
    pub fn icon(self) -> &'static str {
        match self {
            Self::Power => "battery-symbolic",
            Self::Audio => "audio-headphones-symbolic",
            Self::Microphone => "audio-input-microphone-symbolic",
            Self::Camera => "camera-web-symbolic",
            Self::Network => "network-wireless-symbolic",
            Self::Bluetooth => "bluetooth-active-symbolic",
            Self::Updates => "software-update-available-symbolic",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Activity {
    pub source: Source,
    pub title: String,
    pub detail: String,
    pub cue: String,
    pub progress: Option<u8>,
    pub urgent: bool,
    #[serde(default)]
    pub announce: bool,
    pub expires_ms: Option<i64>,
}
impl Activity {
    fn new(
        source: Source,
        title: &str,
        detail: &str,
        cue: String,
        progress: Option<u8>,
        urgent: bool,
        expires_ms: Option<i64>,
    ) -> Self {
        Self {
            source,
            title: clean(title),
            detail: clean(detail),
            cue: clean(&cue),
            progress: progress.filter(|n| *n <= 100),
            urgent,
            announce: true,
            expires_ms,
        }
    }
}
fn clean(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(180).collect()
}
/// The camera and Bluetooth observers run only while their switches are on:
/// they do real work (a process scan, a system bus watch) for nothing else.
pub fn subscription(camera: bool, bluetooth: bool) -> Subscription<Vec<Activity>> {
    if crate::tools::preview() {
        return Subscription::none();
    }
    Subscription::run_with((camera, bluetooth), |&(camera, bluetooth)| {
        cosmic::iced::stream::channel(2, async move |mut out| {
            let (tx, mut rx) = tokio::sync::mpsc::channel(8);
            let mut tasks = tokio::task::JoinSet::new();
            for source in [Source::Power, Source::Network, Source::Updates] {
                tasks.spawn(bus::observe(source, tx.clone()));
            }
            if camera {
                tasks.spawn(camera::observe(tx.clone()));
            }
            if bluetooth {
                tasks.spawn(bluetooth::observe(tx.clone()));
            }
            tasks.spawn(audio::observe(tx));
            let mut state: Vec<Activity> = Vec::new();
            while let Some((source, activity)) = rx.recv().await {
                let before = state.clone();
                state.retain(|a| a.source != source);
                if let Some(a) = activity {
                    state.push(a);
                }
                state.sort_by_key(|a| Source::ALL.iter().position(|s| *s == a.source));
                if before != state && out.send(state.clone()).await.is_err() {
                    break;
                }
            }
        })
    })
}
pub async fn open_settings(source: Source) -> Result<(), String> {
    let (program, page) = match source {
        Source::Updates => ("cosmic-store", None),
        Source::Power => ("cosmic-settings", Some("power")),
        Source::Audio | Source::Microphone => ("cosmic-settings", Some("sound")),
        Source::Network => ("cosmic-settings", Some("network")),
        Source::Bluetooth => ("cosmic-settings", Some("bluetooth")),
        Source::Camera => ("cosmic-settings", None),
    };
    super::renderer::command(program)
        .args(page)
        .spawn()
        .map(|_| ())
        .map_err(|_| "Could not open system settings".into())
}
