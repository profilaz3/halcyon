//! Pure presentation resolution. Discovery, persistence and provider work stay
//! outside this module; a layer changes only the properties it actually owns.
use crate::config::Palette;
use cosmic::iced::{Color, Font};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockFace {
    Digital,
    Analog,
    /// Hours over minutes, large.
    Stacked,
    /// A dial with hands and four marks, no numbers.
    Minimal,
    /// A dial with the twelve hours written around it.
    Numerals,
    /// The time in words, to the nearest five minutes: "half past nine".
    Words,
    /// The time with the date beside it.
    Split,
}
impl ClockFace {
    /// Every face, in the order the gallery shows them.
    pub const ALL: [Self; 7] = [
        Self::Digital,
        Self::Stacked,
        Self::Split,
        Self::Words,
        Self::Analog,
        Self::Minimal,
        Self::Numerals,
    ];
    /// A face drawn as a dial with hands.
    pub fn dial(self) -> bool {
        matches!(self, Self::Analog | Self::Minimal | Self::Numerals)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Digital => "Digital",
            Self::Analog => "Analog",
            Self::Stacked => "Stacked",
            Self::Minimal => "Minimal",
            Self::Numerals => "Numbers",
            Self::Words => "Words",
            Self::Split => "Split",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Digital => "digital",
            Self::Analog => "analog",
            Self::Stacked => "stacked",
            Self::Minimal => "minimal",
            Self::Numerals => "numerals",
            Self::Words => "words",
            Self::Split => "split",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockWeight {
    Light,
    Normal,
    Bold,
}
impl ClockWeight {
    pub fn native(self) -> cosmic::iced::font::Weight {
        use cosmic::iced::font::Weight;
        match self {
            Self::Light => Weight::Light,
            Self::Normal => Weight::Normal,
            Self::Bold => Weight::Bold,
        }
    }
}

/// `None` is an explicit no-surface choice. An absent override means inherit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockSurface {
    None,
    Panel,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Surface {
    None,
    Solid(Color),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Recipe,
    Setup,
    Moment,
    Instance,
    MomentInstance,
    Accessibility,
    Capability,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Resolved<T> {
    pub value: T,
    pub source: Source,
    /// Retains the authoring source when a supported representation differs.
    pub requested_source: Source,
}
impl<T: Copy> Resolved<T> {
    fn new(value: T) -> Self {
        Self {
            value,
            source: Source::Recipe,
            requested_source: Source::Recipe,
        }
    }
    fn replace(&mut self, value: T, source: Source) {
        self.value = value;
        self.source = source;
        self.requested_source = source;
    }
}

macro_rules! properties {
    ($($field:ident: $kind:ty),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct Values { $(pub $field: $kind),+ }
        #[derive(Debug, Default, Clone, Copy, PartialEq)]
        pub struct Patch { $(pub $field: Option<$kind>),+ }
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct Style { $(pub $field: Resolved<$kind>),+, pub missing_font: Option<&'static str> }
        impl Style {
            fn from_defaults(values: Values) -> Self { Self { $($field: Resolved::new(values.$field)),+, missing_font: None } }
            fn apply(&mut self, patch: Patch, source: Source) { $(if let Some(value) = patch.$field { self.$field.replace(value, source); })+ }
        }
    }
}
properties! {
    ink: Color, muted: Color, accent: Color, surface: Surface,
    body: Font, display: Font, radius: u16, padding: u16, gap: u16,
    motion: bool, clock_face: ClockFace,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Layers {
    pub setup: Patch,
    pub moment: Patch,
    pub instance: Patch,
    pub moment_instance: Patch,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Constraints {
    pub reduce_motion: bool,
    /// An already selected accessible palette, not a wallpaper read.
    pub readable: Option<Palette>,
}

impl Style {
    pub fn palette(self) -> Palette {
        Palette {
            foreground: self.ink.value,
            muted: self.muted.value,
            accent: self.accent.value,
            background: match self.surface.value {
                Surface::None => Color::TRANSPARENT,
                Surface::Solid(c) => c,
            },
        }
    }
}

/// Resolve bounded typed values. The capability function queries an in-memory
/// catalogue supplied by the caller, never fontconfig, files or the network.
pub fn resolve(
    defaults: Values,
    layers: Layers,
    constraints: Constraints,
    available: impl Fn(&str) -> bool,
) -> Style {
    let mut result = Style::from_defaults(defaults);
    for (patch, source) in [
        (layers.setup, Source::Setup),
        (layers.moment, Source::Moment),
        (layers.instance, Source::Instance),
        (layers.moment_instance, Source::MomentInstance),
    ] {
        result.apply(patch, source);
    }
    if constraints.reduce_motion {
        result.motion.replace(false, Source::Accessibility);
    }
    if let Some(p) = constraints.readable {
        result.ink.replace(p.foreground, Source::Accessibility);
        result.muted.replace(p.muted, Source::Accessibility);
        result.accent.replace(p.accent, Source::Accessibility);
        result
            .surface
            .replace(Surface::Solid(p.background), Source::Accessibility);
    }
    // Fallback changes representation, not saved intent or authoring origin.
    for font in [&mut result.body, &mut result.display] {
        if let cosmic::iced::font::Family::Name(name) = font.value.family
            && !available(name)
        {
            result.missing_font = Some(name);
            font.value.family = cosmic::iced::font::Family::SansSerif;
            font.source = Source::Capability;
        }
    }
    result
}

/// Logical clock reading; minute resolution needs no animation/redraw timer.
#[derive(Debug, Clone, PartialEq)]
pub struct ClockReading {
    pub time: String,
    pub period: Option<String>,
    pub hour_turn: f32,
    pub minute_turn: f32,
    /// The hour as the clock shows it: "09" or "21", or "9" on a 12-hour clock.
    pub hours: String,
    pub minutes: String,
    /// The time in words, to the nearest five minutes.
    pub words: [String; 2],
    /// The weekday and the day of the month, for a face that shows the
    /// date beside the time; `None` until a date is given.
    pub date: Option<[String; 2]>,
}
impl ClockReading {
    pub fn at(now: chrono::NaiveTime, format_24h: bool) -> Self {
        use chrono::Timelike;
        Self {
            time: now
                .format(if format_24h { "%H:%M" } else { "%-I:%M" })
                .to_string(),
            period: (!format_24h).then(|| now.format("%p").to_string()),
            hour_turn: ((now.hour() % 12) as f32 + now.minute() as f32 / 60.) / 12.,
            minute_turn: now.minute() as f32 / 60.,
            hours: now
                .format(if format_24h { "%H" } else { "%-I" })
                .to_string(),
            minutes: now.format("%M").to_string(),
            words: in_words(now.hour(), now.minute()),
            date: None,
        }
    }
    /// The same reading with the date it is on.
    pub fn with_date(mut self, date: chrono::NaiveDate) -> Self {
        self.date = Some([
            date.format("%A").to_string(),
            date.format("%B %-d").to_string(),
        ]);
        self
    }
}

/// The hour names a words face uses, one to twelve.
pub const HOUR_WORDS: [&str; 12] = [
    "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
    "twelve",
];
/// Everything a words face says before the hour.
pub const PHRASES: [&str; 11] = [
    "five past",
    "ten past",
    "quarter past",
    "twenty past",
    "twenty-five past",
    "half past",
    "twenty-five to",
    "twenty to",
    "quarter to",
    "ten to",
    "five to",
];

/// The time in words on two lines, to the nearest five minutes: "half past"
/// over "nine", or "nine" over "o’clock". Read the same on a 12 or 24-hour
/// clock, as people say it.
pub fn in_words(hour: u32, minute: u32) -> [String; 2] {
    let rounded = (hour % 24) * 60 + minute % 60 + 2;
    let (mut hour, five) = (rounded / 60, (rounded % 60) / 5);
    if five > 6 {
        hour += 1;
    }
    let name = HOUR_WORDS[((hour + 11) % 12) as usize].to_string();
    let phrase = match five {
        0 => return [name, "o’clock".into()],
        1 | 11 => "five",
        2 | 10 => "ten",
        3 | 9 => "quarter",
        4 | 8 => "twenty",
        5 | 7 => "twenty-five",
        _ => return ["half past".into(), name],
    };
    let side = if five < 6 { "past" } else { "to" };
    [format!("{phrase} {side}"), name]
}

#[cfg(test)]
mod tests;
