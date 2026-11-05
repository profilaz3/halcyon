//! Local, scoped Activity Island choices. No provider credentials or account state.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub system: bool,
    pub media: bool,
    pub calendar: bool,
    pub reveals: bool,
    pub lead_minutes: u8,
    /// Desktop notifications the panel shares with notification applets.
    pub notifications: bool,
    /// Show a notification's message text, not only its app and title.
    pub notification_text: bool,
    /// Turn COSMIC's own popups off while the island shows notifications.
    pub replace_popups: bool,
    pub battery: bool,
    pub sound: bool,
    pub microphone: bool,
    /// A green dot while a camera is on.
    pub camera: bool,
    pub network: bool,
    /// Devices connecting and disconnecting, with their battery.
    pub bluetooth: bool,
    pub updates: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            system: true,
            media: true,
            calendar: true,
            reveals: true,
            lead_minutes: 5,
            notifications: true,
            notification_text: true,
            replace_popups: false,
            battery: true,
            sound: true,
            microphone: true,
            camera: true,
            network: true,
            bluetooth: true,
            updates: true,
        }
    }
}
impl Preferences {
    pub fn lead_ms(self) -> i64 {
        i64::from(match self.lead_minutes {
            10 => 10,
            15 => 15,
            _ => 5,
        }) * 60_000
    }
    /// The island shows this system source.
    pub fn source(self, source: super::system::Source) -> bool {
        use super::system::Source;
        self.system
            && match source {
                Source::Power => self.battery,
                Source::Audio => self.sound,
                Source::Microphone => self.microphone,
                Source::Camera => self.camera,
                Source::Network => self.network,
                Source::Bluetooth => self.bluetooth,
                Source::Updates => self.updates,
            }
    }
    /// COSMIC popups should be off: the island has taken them over.
    pub fn takes_popups(self) -> bool {
        self.notifications && self.replace_popups
    }
}
