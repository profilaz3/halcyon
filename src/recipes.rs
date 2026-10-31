//! Bounded native presentation recipes. No provider bindings or executable parts.
use crate::drawer::Widget;
use serde::{Deserialize, Serialize};
pub mod library;
#[cfg(test)]
mod tests;
pub const MAX_BYTES: usize = 16 * 1024;
pub const MAX_LIBRARY: usize = 32;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Clock,
    Weather,
    Calendar,
    System,
    Music,
    Focus,
}
impl Kind {
    pub const ALL: [Self; 6] = [
        Self::Clock,
        Self::Weather,
        Self::Calendar,
        Self::System,
        Self::Music,
        Self::Focus,
    ];
    pub fn widget(self) -> Widget {
        match self {
            Self::Clock => Widget::Clock,
            Self::Weather => Widget::Weather,
            Self::Calendar => Widget::Calendar,
            Self::System => Widget::Devbox,
            Self::Music => Widget::Music,
            Self::Focus => Widget::Focus,
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Clock => "Clock",
            Self::Weather => "Weather",
            Self::Calendar => "Calendar",
            Self::System => "System monitor",
            Self::Music => "Music",
            Self::Focus => "Focus",
        }
    }
    pub fn pointer(self) -> &'static str {
        match self {
            Self::Clock => "/recipes/clock",
            Self::Weather => "/recipes/weather",
            Self::Calendar => "/recipes/calendar",
            Self::System => "/recipes/devbox",
            Self::Music => "/recipes/music",
            Self::Focus => "/recipes/focus",
        }
    }
    pub fn parts(self) -> &'static [Part] {
        match self {
            Self::Music => &[
                Part::Track,
                Part::Artwork,
                Part::Progress,
                Part::Playback,
                Part::Caption,
                Part::Divider,
                Part::Space,
            ],
            Self::Focus => &[
                Part::Timer,
                Part::Caption,
                Part::SessionDots,
                Part::TimerControls,
                Part::Divider,
                Part::Space,
            ],
            Self::System => &[
                Part::Cpu,
                Part::Memory,
                Part::Disk,
                Part::Network,
                Part::Gpu,
                Part::Uptime,
                Part::Caption,
                Part::Divider,
                Part::Space,
                Part::Settings,
            ],
            Self::Calendar => &[
                Part::Date,
                Part::Agenda,
                Part::Caption,
                Part::Divider,
                Part::Space,
                Part::Settings,
            ],
            Self::Clock => &[
                Part::Face,
                Part::Date,
                Part::Caption,
                Part::Divider,
                Part::Space,
                Part::Settings,
            ],
            Self::Weather => &[
                Part::City,
                Part::Reading,
                Part::Conditions,
                Part::Forecast,
                Part::Caption,
                Part::Divider,
                Part::Space,
                Part::Settings,
            ],
        }
    }
    pub fn version(self) -> u32 {
        if matches!(self, Self::Music | Self::Focus) {
            3
        } else if matches!(self, Self::Calendar | Self::System) {
            2
        } else {
            1
        }
    }
    pub fn from_widget(widget: Widget) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.widget() == widget)
    }
    pub fn primary(self) -> Part {
        match self {
            Self::Clock => Part::Face,
            Self::Weather => Part::Reading,
            Self::Calendar => Part::Agenda,
            Self::System => Part::Cpu,
            Self::Music => Part::Track,
            Self::Focus => Part::Timer,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    Track,
    Artwork,
    Progress,
    Playback,
    Timer,
    TimerControls,
    SessionDots,
    Cpu,
    Memory,
    Disk,
    Network,
    Gpu,
    Uptime,
    Face,
    Agenda,
    Date,
    City,
    Reading,
    Conditions,
    Forecast,
    Caption,
    Divider,
    Space,
    Settings,
}
impl Part {
    pub fn is_metric(self) -> bool {
        matches!(
            self,
            Self::Cpu | Self::Memory | Self::Disk | Self::Network | Self::Gpu | Self::Uptime
        )
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Track => "Track & player",
            Self::Artwork => "Artwork",
            Self::Progress => "Track progress",
            Self::Playback => "Playback controls",
            Self::Timer => "Time & phase",
            Self::TimerControls => "Timer controls",
            Self::SessionDots => "Sessions today",
            Self::Cpu => "CPU",
            Self::Memory => "Memory",
            Self::Disk => "Disk",
            Self::Network => "Network",
            Self::Gpu => "GPU",
            Self::Uptime => "Uptime",
            Self::Face => "Clock face",
            Self::Agenda => "Agenda",
            Self::Date => "Date",
            Self::City => "City",
            Self::Reading => "Temperature & icon",
            Self::Conditions => "Conditions",
            Self::Forecast => "Forecast",
            Self::Caption => "Caption",
            Self::Divider => "Divider",
            Self::Space => "Space",
            Self::Settings => "Open settings",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arrangement {
    Stack,
    Split,
}
impl Arrangement {
    pub const ALL: [Self; 2] = [Self::Stack, Self::Split];
    pub fn title(self) -> &'static str {
        match self {
            Self::Stack => "Stacked",
            Self::Split => "Side by side",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Finish {
    Tidal,
    Paper,
    Carbon,
}
impl Finish {
    pub const ALL: [Self; 3] = [Self::Tidal, Self::Paper, Self::Carbon];
    pub fn title(self) -> &'static str {
        match self {
            Self::Tidal => "Tidal",
            Self::Paper => "Paper",
            Self::Carbon => "Carbon",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerFace {
    Ring,
    Digits,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub version: u32,
    pub id: String,
    pub revision: String,
    pub name: String,
    pub kind: Kind,
    pub parts: Vec<Part>,
    pub caption: String,
    pub compact: Arrangement,
    pub wide: Arrangement,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<Finish>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timer_face: Option<TimerFace>,
}
pub fn valid_id(s: &str) -> bool {
    s.strip_prefix("r-").is_some_and(|s| {
        s.len() == 32
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
pub fn new_id() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok(format!(
        "r-{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}
impl Recipe {
    pub fn starter(kind: Kind) -> Self {
        let mut recipe = Self {
            version: kind.version(),
            id: new_id().unwrap_or_default(),
            revision: String::new(),
            name: format!("My {}", kind.title().to_lowercase()),
            kind,
            parts: match kind {
                Kind::Music => vec![Part::Artwork, Part::Track, Part::Progress, Part::Playback],
                Kind::Focus => vec![
                    Part::Timer,
                    Part::Caption,
                    Part::TimerControls,
                    Part::SessionDots,
                ],
                Kind::System => vec![Part::Cpu, Part::Memory, Part::Network],
                Kind::Calendar => vec![Part::Date, Part::Agenda, Part::Settings],
                Kind::Clock => vec![Part::Face, Part::Date],
                Kind::Weather => vec![Part::City, Part::Reading, Part::Conditions],
            },
            caption: "Make room for today".into(),
            compact: Arrangement::Stack,
            wide: Arrangement::Split,
            finish: None,
            timer_face: (kind == Kind::Focus).then_some(TimerFace::Ring),
        };
        if kind == Kind::Focus {
            recipe.caption = "Deep work".into();
        }
        if kind == Kind::Music {
            recipe.name = "Sleeve".into();
        }
        if kind == Kind::Focus {
            recipe.name = "Orbit".into();
        }
        recipe.seal();
        recipe
    }
    pub fn template_names(kind: Kind) -> &'static [&'static str] {
        match kind {
            Kind::Music => &["Sleeve", "Player", "Quiet"],
            Kind::Focus => &["Orbit", "Type", "Edge"],
            _ => &[],
        }
    }
    pub fn template(kind: Kind, index: usize) -> Self {
        let mut r = Self::starter(kind);
        if let Some(name) = Self::template_names(kind).get(index) {
            r.name = (*name).into();
        }
        match (kind, index) {
            (Kind::Music, 1) => r.compact = Arrangement::Split,
            (Kind::Music, 2) => {
                r.parts = vec![Part::Track, Part::Playback, Part::Progress];
                r.compact = Arrangement::Split;
            }
            (Kind::Focus, 1) => {
                r.timer_face = Some(TimerFace::Digits);
                r.parts = vec![Part::Timer, Part::Caption, Part::TimerControls];
            }
            (Kind::Focus, 2) => {
                r.timer_face = Some(TimerFace::Digits);
                r.parts = vec![Part::Timer, Part::TimerControls];
                r.compact = Arrangement::Split;
            }
            _ => {}
        }
        r.seal();
        r
    }
    pub fn tool_size(&self) -> [f32; 2] {
        match self.kind {
            Kind::Music
                if self.parts.contains(&Part::Artwork) && self.compact == Arrangement::Stack =>
            {
                [320., 400.]
            }
            Kind::Music => [360., 200.],
            Kind::Focus if self.timer_face == Some(TimerFace::Ring) => [280., 320.],
            Kind::Focus if self.compact == Arrangement::Split => [320., 160.],
            Kind::Focus => [280., 240.],
            _ => [320., self.natural_height()],
        }
    }
    pub fn tool(&self) -> bool {
        matches!(self.kind, Kind::Music | Kind::Focus)
    }
    /// The first selected metric stays visible when a system widget is small.
    pub fn primary(&self) -> Part {
        if self.kind == Kind::System {
            self.parts
                .iter()
                .copied()
                .find(|p| p.is_metric())
                .unwrap_or(Part::Cpu)
        } else {
            self.kind.primary()
        }
    }
    pub fn can_remove(&self, index: usize) -> bool {
        self.parts.get(index).is_some_and(|part| {
            if self.kind == Kind::System {
                !part.is_metric() || self.parts.iter().filter(|p| p.is_metric()).count() > 1
            } else {
                *part != self.kind.primary()
            }
        })
    }
    pub fn natural_height(&self) -> f32 {
        (16. + self
            .parts
            .iter()
            .map(|p| match p {
                Part::Divider => 1.,
                Part::Space => 8.,
                Part::Caption => 18.,
                _ => 42.,
            })
            .sum::<f32>()
            + 4. * self.parts.len().saturating_sub(1) as f32)
            .max(160.)
    }
    fn digest(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut value = self.clone();
        value.revision.clear();
        Sha256::digest(serde_json::to_vec(&value).expect("recipe has no non-finite values"))
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
    pub fn seal(&mut self) {
        self.revision = self.digest();
    }
    pub fn validate(&self) -> Result<(), String> {
        if (!self.tool() && self.finish.is_some())
            || (self.kind != Kind::Focus && self.timer_face.is_some())
        {
            return Err("Unsupported presentation choice for this widget".into());
        }
        if self.version != self.kind.version() {
            return Err("This widget recipe needs a compatible Halcyon version".into());
        }
        if !valid_id(&self.id) || self.revision != self.digest() {
            return Err("Invalid widget recipe identity".into());
        }
        if self.name.trim().is_empty()
            || self.name.len() > 80
            || self.name.chars().any(char::is_control)
            || self.caption.len() > 120
            || self.caption.chars().any(char::is_control)
        {
            return Err("Use a short name and a single-line caption".into());
        }
        if self.parts.is_empty() || self.parts.len() > 8 || !self.parts.contains(&self.primary()) {
            return Err("Keep the main content in the widget".into());
        }
        for (i, p) in self.parts.iter().enumerate() {
            if !self.kind.parts().contains(p) || self.parts[..i].contains(p) {
                return Err("A native part can appear once in its supported widget".into());
            }
        }
        Ok(())
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("Widget recipe is too large".into());
        }
        let r: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        r.validate()?;
        Ok(r)
    }
    pub fn arrangement(&self, width: f32) -> Arrangement {
        if width >= 420. {
            self.wide
        } else {
            self.compact
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Applied {
    pub clock: Option<Recipe>,
    pub weather: Option<Recipe>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calendar: Option<Recipe>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devbox: Option<Recipe>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub music: Option<Recipe>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<Recipe>,
}
impl Applied {
    pub fn get(&self, kind: Kind) -> Option<&Recipe> {
        match kind {
            Kind::Clock => self.clock.as_ref(),
            Kind::Weather => self.weather.as_ref(),
            Kind::Calendar => self.calendar.as_ref(),
            Kind::System => self.devbox.as_ref(),
            Kind::Music => self.music.as_ref(),
            Kind::Focus => self.focus.as_ref(),
        }
    }
    pub fn set(&mut self, kind: Kind, value: Option<Recipe>) {
        match kind {
            Kind::Clock => self.clock = value,
            Kind::Weather => self.weather = value,
            Kind::Calendar => self.calendar = value,
            Kind::System => self.devbox = value,
            Kind::Music => self.music = value,
            Kind::Focus => self.focus = value,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        for k in Kind::ALL {
            if let Some(r) = self.get(k) {
                r.validate()?;
                if r.kind != k {
                    return Err("Widget recipe kind does not match its instance".into());
                }
            }
        }
        Ok(())
    }
}
pub fn accepts(value: &serde_json::Value, kind: Kind) -> bool {
    value.is_null()
        || serde_json::from_value::<Recipe>(value.clone())
            .is_ok_and(|r| r.kind == kind && r.validate().is_ok())
}
