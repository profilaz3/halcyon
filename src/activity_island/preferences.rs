//! Local, scoped Activity Island choices. No provider credentials or account state.
use serde::{Deserialize, Serialize};
/// What the island does with one app's notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppRule {
    /// Never revealed nor listed; only counted under Recent.
    Mute,
    /// Revealed even where the island would only list it. Do Not Disturb
    /// still holds back all but urgent ones.
    Always,
    /// Listed, never revealed.
    ListOnly,
}
/// A rule for one app, matched by its desktop entry when both sides know
/// one, otherwise by the app name the notification gives.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AppRuleEntry {
    pub app: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub entry: String,
    pub rule: AppRule,
}
impl AppRuleEntry {
    pub fn matches(&self, app: &str, entry: &str) -> bool {
        if !self.entry.is_empty() && !entry.is_empty() {
            self.entry == entry
        } else {
            self.app == app
        }
    }
}
/// At most this many app rules are kept; the oldest give way.
pub const APP_RULES: usize = 32;
/// How the open island and its pill are painted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IslandStyle {
    /// The panel's own colours, light or dark.
    #[default]
    Theme,
    /// A near-black surface on any panel.
    Dark,
}
impl IslandStyle {
    pub const ALL: [Self; 2] = [Self::Theme, Self::Dark];
    pub fn key(self) -> &'static str {
        match self {
            Self::Theme => "theme",
            Self::Dark => "dark",
        }
    }
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}
pub const RULE_APP: usize = 48;
pub const RULE_ENTRY: usize = 96;
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub system: bool,
    pub media: bool,
    pub calendar: bool,
    pub reveals: bool,
    /// Files dropped on the island, kept on its shelf.
    pub shelf: bool,
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
    /// Follow the panel theme, or always dark.
    pub style: IslandStyle,
    /// Per-app choices from the island's menu or Settings.
    #[serde(deserialize_with = "bounded_rules")]
    pub app_rules: Vec<AppRuleEntry>,
}
/// Rules from disk are trusted only as far as the island would write them.
fn bounded_rules<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<AppRuleEntry>, D::Error> {
    let rules = Vec::<AppRuleEntry>::deserialize(d)?;
    Ok(rules
        .into_iter()
        .filter(|r| valid_rule(&r.app, &r.entry))
        .take(APP_RULES)
        .collect())
}
/// An app name and a desktop entry fit to store and to match.
pub fn valid_rule(app: &str, entry: &str) -> bool {
    !app.trim().is_empty()
        && app.chars().count() <= RULE_APP
        && !app.chars().any(char::is_control)
        && entry.len() <= RULE_ENTRY
        && entry
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            system: true,
            media: true,
            calendar: true,
            reveals: true,
            shelf: true,
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
            style: IslandStyle::Theme,
            app_rules: Vec::new(),
        }
    }
}
impl Preferences {
    pub fn lead_ms(&self) -> i64 {
        i64::from(match self.lead_minutes {
            10 => 10,
            15 => 15,
            _ => 5,
        }) * 60_000
    }
    /// The island shows this system source.
    pub fn source(&self, source: super::system::Source) -> bool {
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
    pub fn takes_popups(&self) -> bool {
        self.notifications && self.replace_popups
    }
    /// The rule for an app, if one is set.
    pub fn rule_for(&self, app: &str, entry: &str) -> Option<AppRule> {
        self.app_rules
            .iter()
            .find(|r| r.matches(app, entry))
            .map(|r| r.rule)
    }
    /// Sets or, with `None`, removes the rule for an app. A new rule goes
    /// first; beyond `APP_RULES` the oldest is dropped.
    pub fn set_rule(&mut self, app: &str, entry: &str, rule: Option<AppRule>) {
        if !valid_rule(app, entry) {
            return;
        }
        let at = self.app_rules.iter().position(|r| r.matches(app, entry));
        match (at, rule) {
            (Some(at), Some(rule)) => {
                let r = &mut self.app_rules[at];
                r.rule = rule;
                if r.entry.is_empty() {
                    r.entry = entry.to_owned();
                }
            }
            (Some(at), None) => {
                self.app_rules.remove(at);
            }
            (None, Some(rule)) => {
                self.app_rules.insert(
                    0,
                    AppRuleEntry {
                        app: app.to_owned(),
                        entry: entry.to_owned(),
                        rule,
                    },
                );
                self.app_rules.truncate(APP_RULES);
            }
            (None, None) => {}
        }
    }
}
