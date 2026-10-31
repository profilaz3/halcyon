//! Native wallpaper rain. No video, network, screen capture or input ownership.
//! Both renderers use the same background, controls and bounded GPU field.
mod atmosphere;
#[cfg(test)]
mod benchmark;
pub use atmosphere::{Clouds, Effect, Fog, FogColor, Snow};
pub mod gpu;
pub mod lifecycle;
pub mod simulation;
#[cfg(test)]
mod tests;
pub mod visibility;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Procedural,
    Hybrid,
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Eco,
    #[default]
    Balanced,
    Smooth,
}
impl Quality {
    pub fn fps(self) -> u32 {
        if self == Self::Smooth { 60 } else { 30 }
    }
    pub fn scale(self) -> f32 {
        if self == Self::Eco { 0.33 } else { 0.5 }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub enabled: bool,
    pub mode: Mode,
    #[serde(skip_serializing_if = "Effect::is_default")]
    pub effect: Effect,
    #[serde(skip_serializing_if = "Snow::is_default")]
    pub snow: Snow,
    #[serde(skip_serializing_if = "Fog::is_default")]
    pub fog: Fog,
    #[serde(skip_serializing_if = "Clouds::is_default")]
    pub clouds: Clouds,
    pub intensity: u8,
    pub trails: u8,
    pub size: u8,
    pub speed: u8,
    pub wind: i8,
    pub mist: u8,
    pub refraction: u8,
    pub quality: Quality,
    pub pause_on_battery: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: Mode::Procedural,
            effect: Effect::Rain,
            snow: Snow::default(),
            fog: Fog::default(),
            clouds: Clouds::default(),
            intensity: 45,
            trails: 35,
            size: 30,
            speed: 45,
            wind: 0,
            mist: 15,
            refraction: 55,
            quality: Quality::Balanced,
            pause_on_battery: true,
        }
    }
}
impl Preferences {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn validate(&self) -> Result<(), String> {
        if [
            self.snow.density,
            self.snow.size,
            self.snow.speed,
            self.snow.opacity,
            self.fog.opacity,
            self.fog.height,
            self.fog.speed,
            self.clouds.coverage,
            self.clouds.softness,
            self.clouds.speed,
            self.clouds.strength,
            self.intensity,
            self.trails,
            self.size,
            self.speed,
            self.mist,
            self.refraction,
        ]
        .into_iter()
        .any(|n| n > 100)
            || [self.wind, self.snow.wind, self.fog.wind, self.clouds.wind]
                .into_iter()
                .any(|w| !(-100..=100).contains(&w))
        {
            return Err("Wallpaper effect controls must be within their ranges".into());
        }
        Ok(())
    }
    pub fn has_visible_effect(&self) -> bool {
        match self.effect {
            Effect::Rain => self.intensity > 0,
            Effect::Snow => self.snow.density > 0 && self.snow.opacity > 0,
            Effect::Fog => self.fog.opacity > 0 && self.fog.height > 0,
            Effect::Clouds => self.clouds.coverage > 0 && self.clouds.strength > 0,
        }
    }
    pub fn animates(&self) -> bool {
        match self.effect {
            Effect::Rain => true,
            Effect::Snow => self.snow.speed > 0,
            Effect::Fog => self.fog.speed > 0,
            Effect::Clouds => self.clouds.speed > 0,
        }
    }
    pub fn hybrid(&self) -> bool {
        self.effect == Effect::Rain && self.mode == Mode::Hybrid
    }
    pub fn active(&self, motion: bool, state: lifecycle::State) -> bool {
        self.enabled
            && state.ready
            && motion
            && self.has_visible_effect()
            && !state.locked
            && !state.sleeping
            && (!self.pause_on_battery || !state.on_battery)
    }
}

#[cfg(test)]
mod preferences_tests {
    use super::*;
    #[test]
    fn rain_preferences_are_opt_in_and_bounded() {
        let mut p = Preferences::default();
        assert!(!p.enabled);
        assert!(p.validate().is_ok());
        p.intensity = 101;
        assert!(p.validate().is_err());
        assert!(serde_json::from_str::<Preferences>(r#"{"video":"/tmp/a"}"#).is_err());
    }
    #[test]
    fn rain_trails_default_for_existing_preferences_and_validate() {
        let mut p: Preferences = serde_json::from_str(r#"{"intensity":70}"#).unwrap();
        assert_eq!(p.trails, 35);
        p.trails = 0;
        let encoded = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Preferences>(&encoded).unwrap(), p);
        p.trails = 101;
        assert!(p.validate().is_err());
    }
    #[test]
    fn rain_policy_stops_motion_and_respects_battery_opt_out() {
        let mut p = Preferences {
            enabled: true,
            ..Default::default()
        };
        let mut s = lifecycle::State {
            ready: true,
            ..Default::default()
        };
        assert!(!p.active(true, lifecycle::State::default()));
        assert!(p.active(true, s));
        assert!(!p.active(false, s));
        s.on_battery = true;
        assert!(!p.active(true, s));
        p.pause_on_battery = false;
        assert!(p.active(true, s));
        s.locked = true;
        assert!(!p.active(true, s));
        s.locked = false;
        s.sleeping = true;
        assert!(!p.active(true, s));
    }
}
