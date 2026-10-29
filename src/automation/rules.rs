//! The automation settings (`automation.json`): an ordered list of rules,
//! each applying one existing profile at a local time on chosen weekdays or
//! when a display connects or disconnects, plus the master switch and an
//! optional pause. The list order is the priority: when rules happen
//! together, the higher one wins.
//!
//! Timestamps are Unix seconds. Every `since` marks when a rule or the whole
//! feature last became active: scheduled times and display changes before it
//! never count, so turning something on never replays the past.
use crate::desktop::profiles::ProfileId;
use chrono::{Datelike, NaiveDate, NaiveTime};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const FORMAT: &str = "desktop-cards-automation";
pub const VERSION: u32 = 1;
pub const MOMENTS_VERSION: u32 = 2;
pub const MAX_RULES: usize = 32;
pub const MAX_BYTES: usize = 64 * 1024;
const MAX_LABEL: usize = 80;

/// Weekdays as a bit set, Monday = bit 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Days(u8);

const DAY_KEYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
const DAY_SHORT: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const DAY_LONG: [&str; 7] = [
    "Mondays",
    "Tuesdays",
    "Wednesdays",
    "Thursdays",
    "Fridays",
    "Saturdays",
    "Sundays",
];

impl Days {
    pub const EVERY: Self = Self(0x7f);
    pub const WEEKDAYS: Self = Self(0x1f);
    pub const WEEKEND: Self = Self(0x60);
    #[cfg_attr(not(test), allow(dead_code))]
    pub const NONE: Self = Self(0);

    pub fn has(self, index: usize) -> bool {
        index < 7 && self.0 & (1 << index) != 0
    }
    pub fn toggled(self, index: usize) -> Self {
        Self((self.0 ^ (1 << index.min(6))) & 0x7f)
    }
    pub fn is_empty(self) -> bool {
        self.0 & 0x7f == 0
    }
    /// "Every day", "Weekdays", "Weekends", "Mondays" or "Mon, Wed, Fri".
    pub fn label(self) -> String {
        match self.0 & 0x7f {
            0x7f => "Every day".into(),
            0x1f => "Weekdays".into(),
            0x60 => "Weekends".into(),
            0 => "No days".into(),
            bits if bits.count_ones() == 1 => DAY_LONG[bits.trailing_zeros() as usize].into(),
            _ => (0..7)
                .filter(|index| self.has(*index))
                .map(|index| DAY_SHORT[index])
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
    pub fn short(index: usize) -> &'static str {
        DAY_SHORT[index.min(6)]
    }
}

impl Serialize for Days {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let keys: Vec<&str> = (0..7)
            .filter(|index| self.has(*index))
            .map(|index| DAY_KEYS[index])
            .collect();
        keys.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Days {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let keys = Vec::<String>::deserialize(deserializer)?;
        let mut bits = 0u8;
        for key in &keys {
            let index = DAY_KEYS
                .iter()
                .position(|known| known == key)
                .ok_or_else(|| serde::de::Error::custom(format!("unknown day {key}")))?;
            bits |= 1 << index;
        }
        Ok(Self(bits))
    }
}

/// A local wall-clock time of day, to the minute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Time(u16);

impl Time {
    pub fn new(hour: u32, minute: u32) -> Option<Self> {
        (hour < 24 && minute < 60).then(|| Self((hour * 60 + minute) as u16))
    }
    pub fn hour(self) -> u32 {
        u32::from(self.0 / 60)
    }
    pub fn minute(self) -> u32 {
        u32::from(self.0 % 60)
    }
    pub fn naive(self) -> NaiveTime {
        NaiveTime::from_hms_opt(self.hour(), self.minute(), 0).expect("valid time")
    }
    /// Reads what people type: `19:00`, `7:30`, `0730`, `19`, `7pm`,
    /// `7:30 am`, `19.00`.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().to_ascii_lowercase().replace(' ', "");
        let (digits, meridiem) = if let Some(rest) = text
            .strip_suffix("am")
            .or_else(|| text.strip_suffix("a.m."))
            .or_else(|| text.strip_suffix('a'))
        {
            (rest.to_string(), Some(false))
        } else if let Some(rest) = text
            .strip_suffix("pm")
            .or_else(|| text.strip_suffix("p.m."))
            .or_else(|| text.strip_suffix('p'))
        {
            (rest.to_string(), Some(true))
        } else {
            (text, None)
        };
        let (hour, minute) = match digits.split_once([':', '.', 'h']) {
            Some((hour, minute)) => {
                if minute.len() != 2 && !(minute.is_empty() && digits.ends_with('h')) {
                    return None;
                }
                let minute = if minute.is_empty() { "0" } else { minute };
                (hour.to_string(), minute.to_string())
            }
            None if digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()) => {
                (digits[..2].to_string(), digits[2..].to_string())
            }
            None if (1..=2).contains(&digits.len()) => (digits.clone(), "0".into()),
            None => return None,
        };
        if hour.is_empty() || hour.len() > 2 || !hour.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        if !minute.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let (mut hour, minute): (u32, u32) = (hour.parse().ok()?, minute.parse().ok()?);
        if let Some(pm) = meridiem {
            if !(1..=12).contains(&hour) {
                return None;
            }
            hour = match (hour, pm) {
                (12, false) => 0,
                (12, true) => 12,
                (hour, true) => hour + 12,
                (hour, false) => hour,
            };
        }
        Self::new(hour, minute)
    }
    /// `19:00`, or `7:00 PM` for a 12-hour clock.
    pub fn format(self, h24: bool) -> String {
        if h24 {
            format!("{:02}:{:02}", self.hour(), self.minute())
        } else {
            let hour = match self.hour() % 12 {
                0 => 12,
                hour => hour,
            };
            let suffix = if self.hour() < 12 { "AM" } else { "PM" };
            format!("{hour}:{:02} {suffix}", self.minute())
        }
    }
}

impl Serialize for Time {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.format(true).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Time {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let valid = text.len() == 5 && text.as_bytes()[2] == b':';
        valid
            .then(|| Self::parse(&text))
            .flatten()
            .ok_or_else(|| serde::de::Error::custom("times are HH:MM"))
    }
}

/// The profile a rule applies: a built-in starting point or a saved one.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileRef {
    Moment(crate::moments::Phase),
    Preset(String),
    Saved(String),
}

impl ProfileRef {
    fn valid(&self) -> bool {
        match self {
            Self::Moment(_) => true,
            Self::Preset(id) => {
                !id.is_empty()
                    && id.len() <= 32
                    && id.bytes().all(|b| {
                        b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'
                    })
            }
            Self::Saved(id) => ProfileId::parse(id).is_ok(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Connect,
    Disconnect,
}

/// A display change: one named display (its DRM connector, `DP-2`), or
/// with no name, the external displays as a group (the first one connecting,
/// the last one disconnecting).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DisplayWhen {
    pub change: Change,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    /// What the display called itself when the rule was made (its model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl DisplayWhen {
    /// Whether the change from `before` to `after` connected displays is
    /// this one.
    pub fn fired(&self, before: &[String], after: &[String]) -> bool {
        match (&self.display, self.change) {
            (Some(name), Change::Connect) => !before.contains(name) && after.contains(name),
            (Some(name), Change::Disconnect) => before.contains(name) && !after.contains(name),
            (None, Change::Connect) => external(before) == 0 && external(after) > 0,
            (None, Change::Disconnect) => external(before) > 0 && external(after) == 0,
        }
    }
    /// "AOC 24G2W1G5 connects", "an external display connects", …
    pub fn summary(&self) -> String {
        let name = self
            .label
            .as_deref()
            .filter(|label| !label.is_empty())
            .or(self.display.as_deref());
        match (name, self.change) {
            (Some(name), Change::Connect) => format!("{name} connects"),
            (Some(name), Change::Disconnect) => format!("{name} disconnects"),
            (None, Change::Connect) => "An external display connects".into(),
            (None, Change::Disconnect) => "The last external display disconnects".into(),
        }
    }
    /// For history lines: "DP-2 connected", "External display connected".
    pub fn happened(&self) -> String {
        let name = self
            .label
            .as_deref()
            .filter(|label| !label.is_empty())
            .or(self.display.as_deref());
        match (name, self.change) {
            (Some(name), Change::Connect) => format!("{name} connected"),
            (Some(name), Change::Disconnect) => format!("{name} disconnected"),
            (None, Change::Connect) => "External display connected".into(),
            (None, Change::Disconnect) => "External displays disconnected".into(),
        }
    }
}

/// Built-in panels: everything else counts as an external display.
pub fn builtin(name: &str) -> bool {
    ["eDP", "LVDS", "DSI"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

fn external(names: &[String]) -> usize {
    names.iter().filter(|name| !builtin(name)).count()
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    Schedule {
        time: Time,
        days: Days,
    },
    Solar {
        event: super::solar::Event,
        days: Days,
        source: super::solar::Source,
    },
    Display(DisplayWhen),
}

impl Trigger {
    /// "Weekdays at 19:00" or "AOC 24G2W1G5 connects".
    pub fn summary(&self, h24: bool) -> String {
        match self {
            Self::Schedule { time, days } => format!("{} at {}", days.label(), time.format(h24)),
            Self::Solar { event, days, .. } => format!("{} · {}", days.label(), event.title()),
            Self::Display(when) => when.summary(),
        }
    }
    pub fn is_schedule(&self) -> bool {
        matches!(self, Self::Schedule { .. } | Self::Solar { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub enabled: bool,
    /// When this rule last became active (created, edited or turned on).
    pub since: i64,
    pub profile: ProfileRef,
    /// The profile's name when the rule was saved, shown if it goes missing.
    pub name: String,
    pub when: Trigger,
}

/// Automation stops acting from `since` until `until` (`None`: until the
/// person resumes). With `skip` it is not a pause: only the one scheduled
/// switch at `until - 1` is skipped, and display rules keep working.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pause {
    pub since: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<i64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip: bool,
}

impl Pause {
    /// Whether the moment `at` falls inside this pause (or skip).
    pub fn covers(&self, at: i64) -> bool {
        at >= self.since && self.until.is_none_or(|until| at < until)
    }
    /// Whether a scheduled switch at `at` is held back: anything inside a
    /// pause, or the one switch a skip names.
    pub fn stops_schedule(&self, at: i64) -> bool {
        if self.skip {
            self.covers(at) && self.until == Some(at + 1)
        } else {
            self.covers(at)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Automation {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub enabled: bool,
    /// When automation was last turned on or resumed.
    #[serde(default)]
    pub since: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused: Option<Pause>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_city: Option<crate::config::Location>,
}

impl Default for Automation {
    /// Off, without rules: what every upgrade and new install starts with.
    fn default() -> Self {
        Self {
            format: FORMAT.into(),
            version: VERSION,
            enabled: false,
            since: 0,
            paused: None,
            rules: Vec::new(),
            manual_city: None,
        }
    }
}

impl Automation {
    /// Reads and checks a settings file. A file from a newer version or one
    /// that fails a check is refused with a reason, never half-used.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("The automation settings are too large.".into());
        }
        let value: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|_| "The automation settings are damaged.".to_string())?;
        if value.get("format").and_then(|format| format.as_str()) != Some(FORMAT) {
            return Err("The automation settings are not recognized.".into());
        }
        if value
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|version| version > 3)
        {
            return Err("The automation settings come from a newer Halcyon.".into());
        }
        let automation: Self = serde_json::from_value(value)
            .map_err(|error| format!("The automation settings are damaged: {error}."))?;
        automation.validate()?;
        Ok(automation)
    }
    pub fn written_version(&self) -> u32 {
        if self.manual_city.is_some()
            || self
                .rules
                .iter()
                .any(|r| matches!(r.when, Trigger::Solar { .. }))
        {
            3
        } else if self
            .rules
            .iter()
            .any(|r| matches!(r.profile, ProfileRef::Moment(_)))
        {
            MOMENTS_VERSION
        } else {
            VERSION
        }
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        let mut record = self.clone();
        record.version = record.written_version();
        record.validate()?;
        serde_json::to_vec_pretty(&record).map_err(|error| error.to_string())
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT
            || ![VERSION, MOMENTS_VERSION, 3].contains(&self.version)
            || self.version < self.written_version()
        {
            return Err("The automation settings are not recognized.".into());
        }
        if self.rules.len() > MAX_RULES {
            return Err(format!("Keep at most {MAX_RULES} rules."));
        }
        if let Some(city) = &self.manual_city {
            super::solar::validate_city(city)?;
        }
        if let Some(pause) = &self.paused
            && pause.until.is_some_and(|until| until < pause.since)
        {
            return Err("The pause ends before it starts.".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for rule in &self.rules {
            let id_ok = rule.id.len() <= 16
                && rule.id.starts_with('r')
                && rule.id.len() > 1
                && rule.id[1..].bytes().all(|b| b.is_ascii_digit());
            if !id_ok || !ids.insert(rule.id.as_str()) {
                return Err("A rule has an invalid ID.".into());
            }
            if !rule.profile.valid() || !label_ok(&rule.name) || rule.name.is_empty() {
                return Err("A rule names an invalid profile.".into());
            }
            match &rule.when {
                Trigger::Schedule { days, .. } | Trigger::Solar { days, .. } if days.is_empty() => {
                    return Err("A scheduled rule needs at least one day.".into());
                }
                Trigger::Display(when) => {
                    if when
                        .display
                        .as_deref()
                        .is_some_and(|name| !crate::desktop::wallpaper::valid_output_name(name))
                        || when.label.as_deref().is_some_and(|label| !label_ok(label))
                    {
                        return Err("A display rule names an invalid display.".into());
                    }
                }
                Trigger::Schedule { .. } | Trigger::Solar { .. } => {}
            }
        }
        Ok(())
    }
    /// An unused rule ID.
    pub fn next_id(&self) -> String {
        let highest = self
            .rules
            .iter()
            .filter_map(|rule| rule.id[1..].parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        format!("r{}", highest + 1)
    }
    pub fn rule(&self, id: &str) -> Option<&Rule> {
        self.rules.iter().find(|rule| rule.id == id)
    }
    /// Enabled rules, in priority order.
    pub fn active(&self) -> impl Iterator<Item = &Rule> {
        self.rules.iter().filter(|rule| rule.enabled)
    }
    /// Whether anything would be watched or scheduled at all.
    pub fn running(&self) -> bool {
        self.enabled && self.active().next().is_some()
    }
    pub fn watches_displays(&self) -> bool {
        self.enabled
            && self
                .active()
                .any(|rule| matches!(rule.when, Trigger::Display(_)))
    }
    /// The pause in effect at `now`, if any (a skip is not a pause).
    pub fn paused_at(&self, now: i64) -> Option<&Pause> {
        self.paused
            .as_ref()
            .filter(|pause| !pause.skip && pause.covers(now))
    }
    /// The skip waiting for its switch at `now`, if any.
    pub fn skipping_at(&self, now: i64) -> Option<&Pause> {
        self.paused
            .as_ref()
            .filter(|pause| pause.skip && pause.covers(now))
    }
}

fn label_ok(text: &str) -> bool {
    text.chars().count() <= MAX_LABEL && !text.chars().any(char::is_control)
}

/// Monday-first weekday index of a date.
pub fn weekday_index(date: NaiveDate) -> usize {
    date.weekday().num_days_from_monday() as usize
}
