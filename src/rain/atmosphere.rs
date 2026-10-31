//! Manual atmosphere preferences. Each effect retains its own controls.
use serde::{Deserialize, Serialize};
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    #[default]
    Rain,
    Snow,
    Fog,
    Clouds,
}
impl Effect {
    pub fn is_default(&self) -> bool {
        *self == Self::Rain
    }
    pub const fn title(self) -> &'static str {
        match self {
            Self::Rain => "Rain",
            Self::Snow => "Snow",
            Self::Fog => "Fog",
            Self::Clouds => "Cloud shadows",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Snow {
    pub density: u8,
    pub size: u8,
    pub speed: u8,
    pub wind: i8,
    pub opacity: u8,
}
impl Default for Snow {
    fn default() -> Self {
        Self {
            density: 55,
            size: 30,
            speed: 30,
            wind: 10,
            opacity: 80,
        }
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FogColor {
    #[default]
    Neutral,
    Cool,
    Warm,
}
impl FogColor {
    pub fn rgb(self) -> [f32; 4] {
        match self {
            Self::Neutral => [0.82, 0.84, 0.86, 0.],
            Self::Cool => [0.64, 0.75, 0.86, 0.],
            Self::Warm => [0.89, 0.82, 0.70, 0.],
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fog {
    pub opacity: u8,
    pub height: u8,
    pub speed: u8,
    pub wind: i8,
    pub color: FogColor,
}
impl Default for Fog {
    fn default() -> Self {
        Self {
            opacity: 35,
            height: 50,
            speed: 20,
            wind: 15,
            color: FogColor::Neutral,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Clouds {
    pub coverage: u8,
    pub softness: u8,
    pub speed: u8,
    pub wind: i8,
    pub strength: u8,
}
impl Default for Clouds {
    fn default() -> Self {
        Self {
            coverage: 45,
            softness: 65,
            speed: 20,
            wind: 20,
            strength: 25,
        }
    }
}
impl Snow {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}
impl Fog {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}
impl Clouds {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}
