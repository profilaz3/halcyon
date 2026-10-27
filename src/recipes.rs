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
}
impl Kind {
    pub const ALL: [Self; 3] = [Self::Clock, Self::Weather, Self::Calendar];
    pub fn widget(self) -> Widget {
        match self {
            Self::Clock => Widget::Clock,
            Self::Weather => Widget::Weather,
            Self::Calendar => Widget::Calendar,
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Clock => "Clock",
            Self::Weather => "Weather",
            Self::Calendar => "Calendar",
        }
    }
    pub fn pointer(self) -> &'static str {
        match self {
            Self::Clock => "/recipes/clock",
            Self::Weather => "/recipes/weather",
            Self::Calendar => "/recipes/calendar",
        }
    }
    pub fn parts(self) -> &'static [Part] {
        match self {
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
        if self == Self::Calendar { 2 } else { 1 }
    }
    pub fn from_widget(widget: Widget) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.widget() == widget)
    }
    pub fn primary(self) -> Part {
        match self {
            Self::Clock => Part::Face,
            Self::Weather => Part::Reading,
            Self::Calendar => Part::Agenda,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
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
    pub fn title(self) -> &'static str {
        match self {
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
            Self::Split => "Face beside details",
        }
    }
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
                Kind::Calendar => vec![Part::Date, Part::Agenda, Part::Settings],
                Kind::Clock => vec![Part::Face, Part::Date],
                Kind::Weather => vec![Part::City, Part::Reading, Part::Conditions],
            },
            caption: "Make room for today".into(),
            compact: Arrangement::Stack,
            wide: Arrangement::Split,
        };
        recipe.seal();
        recipe
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
        if self.parts.is_empty()
            || self.parts.len() > 8
            || !self.parts.contains(&self.kind.primary())
        {
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
}
impl Applied {
    pub fn get(&self, kind: Kind) -> Option<&Recipe> {
        match kind {
            Kind::Clock => self.clock.as_ref(),
            Kind::Weather => self.weather.as_ref(),
            Kind::Calendar => self.calendar.as_ref(),
        }
    }
    pub fn set(&mut self, kind: Kind, value: Option<Recipe>) {
        match kind {
            Kind::Clock => self.clock = value,
            Kind::Weather => self.weather = value,
            Kind::Calendar => self.calendar = value,
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
