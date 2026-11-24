//! Native wallpaper rain. No video, network, screen capture or input ownership.
//! Both renderers use the same background, controls and bounded GPU field.
mod atmosphere;
#[cfg(test)]
mod benchmark;
pub mod desk;
pub use atmosphere::{Clouds, Effect, Fog, FogColor, Snow};
mod glass;
pub use glass::{FinishLibrary, Glass, GlassMode};
pub mod gpu;
pub mod lifecycle;
pub mod simulation;
#[cfg(test)]
mod tests;
pub mod visibility;
pub mod weather;
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
    /// Confine the Rain effect to the glass behind desktop widgets.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub widget_glass: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glass: Option<Glass>,
    #[serde(skip_serializing_if = "FinishLibrary::is_empty")]
    pub finishes: FinishLibrary,
    /// Match the weather: while the Weather widget's reading is current, it
    /// picks the effect. Off by default; `weather::resolve` has the rules.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub weather: bool,
    /// Weather strength: the share of the setting light and moderate
    /// weather play at. Natural (40/70/100%) unless chosen.
    #[serde(skip_serializing_if = "weather::Strength::is_default")]
    pub weather_strength: weather::Strength,
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
            widget_glass: false,
            glass: None,
            finishes: FinishLibrary::default(),
            weather: false,
            weather_strength: weather::Strength::Natural,
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
            mist: 30,
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
        self.finishes.validate()?;
        if let Some(glass) = self.glass {
            glass.validate()?;
        }
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
    pub fn edit_finish(&self, id: Option<u32>) -> Self {
        let mut p = self.clone();
        if let Some(f) = id.and_then(|id| self.finishes.entries.get(&id)) {
            p.glass = f.glass;
            p.refraction = f.refraction;
        }
        p
    }
    pub fn store_finish(&self, mut draft: Self, id: Option<u32>) -> Self {
        if let Some(f) = id.and_then(|id| draft.finishes.entries.get_mut(&id)) {
            f.glass = draft.glass;
            f.refraction = draft.refraction;
            draft.glass = self.glass;
            draft.refraction = self.refraction;
        }
        draft
    }
    pub fn reflects_pointer(&self) -> bool {
        self.enabled
            && self.effect == Effect::Lens
            && (self.glass.is_none_or(|g| g.mode == GlassMode::Polished)
                || self.finishes.links.keys().any(|w| {
                    self.finishes
                        .linked(*w)
                        .is_some_and(|f| f.glass.is_none_or(|g| g.mode == GlassMode::Polished))
                }))
    }
    pub fn active_glass(&self) -> Option<Glass> {
        self.glass
            .filter(|_| self.enabled && self.effect == Effect::Lens)
    }
    pub fn confined_to_widgets(&self) -> bool {
        self.effect == Effect::Lens || (self.effect == Effect::Rain && self.widget_glass)
    }
    pub fn has_visible_effect(&self) -> bool {
        match self.effect {
            Effect::Lens => true,
            Effect::Rain => self.intensity > 0,
            Effect::Snow => self.snow.density > 0 && self.snow.opacity > 0,
            Effect::Fog => self.fog.opacity > 0 && self.fog.height > 0,
            Effect::Clouds => self.clouds.coverage > 0 && self.clouds.strength > 0,
        }
    }
    pub fn animates(&self) -> bool {
        match self.effect {
            Effect::Lens => false,
            Effect::Rain => true,
            Effect::Snow => self.snow.speed > 0,
            Effect::Fog => self.fog.speed > 0,
            Effect::Clouds => self.clouds.speed > 0,
        }
    }
    pub fn hybrid(&self) -> bool {
        self.effect == Effect::Rain && self.mode == Mode::Hybrid
    }
    /// An effect may play: one is chosen, or the weather may pick one.
    pub fn wanted(&self) -> bool {
        self.enabled || self.weather
    }
    /// Reduce motion, an unknown session, lock, sleep or (when asked) battery
    /// stop every effect at once, whoever chose it.
    pub fn paused(&self, motion: bool, state: lifecycle::State) -> bool {
        !state.ready
            || (!motion && self.effect != Effect::Lens)
            || state.locked
            || state.sleeping
            || (self.pause_on_battery && state.on_battery)
    }
    pub fn active(&self, motion: bool, state: lifecycle::State) -> bool {
        self.enabled && self.has_visible_effect() && !self.paused(motion, state)
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
    fn weather_matching_is_opt_in_and_omitted_until_chosen() {
        let p = Preferences::default();
        assert!(!p.weather && !p.wanted());
        assert!(!serde_json::to_string(&p).unwrap().contains("weather"));
        let p: Preferences = serde_json::from_str(r#"{"weather":true}"#).unwrap();
        assert!(p.weather && p.wanted() && !p.enabled);
        assert!(
            serde_json::to_string(&p)
                .unwrap()
                .contains(r#""weather":true"#)
        );
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
