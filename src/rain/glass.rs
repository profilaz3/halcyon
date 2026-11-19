//! The optical finish of a widget pane. An absent finish keeps the original Liquid lens.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlassMode {
    Smoked,
    Polished,
    Frosted,
}
impl GlassMode {
    pub const ALL: [Self; 3] = [Self::Smoked, Self::Polished, Self::Frosted];

    pub fn title(self) -> &'static str {
        match self {
            Self::Smoked => "Smoked",
            Self::Polished => "Polished",
            Self::Frosted => "Frosted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Glass {
    pub mode: GlassMode,
    pub frost: u8,
    /// Radius of the wallpaper blur in logical pixels.
    pub blur: u8,
    /// An explicit swatch. None follows the active widget theme.
    pub color: Option<[u8; 3]>,
    pub tint: u8,
    pub opacity: u8,
    /// None follows the widget style's corners.
    pub radius: Option<u8>,
}
impl Default for Glass {
    fn default() -> Self {
        Self {
            mode: GlassMode::Polished,
            frost: 0,
            blur: 0,
            color: None,
            tint: 12,
            opacity: 65,
            radius: None,
        }
    }
}
impl Glass {
    pub fn preset(self, mode: GlassMode) -> Self {
        let (frost, blur) = match mode {
            GlassMode::Smoked | GlassMode::Polished => (0, 0),
            GlassMode::Frosted => (35, 12),
        };
        Self {
            mode,
            frost,
            blur,
            ..self
        }
    }
    pub fn validate(self) -> Result<(), String> {
        if self.opacity > 100
            || self.frost > 100
            || self.tint > 100
            || self.blur > 32
            || self.radius.is_some_and(|r| r > 32)
        {
            return Err(
                "Glass frost and tint range from 0–100%, blur and corners from 0–32 px".into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rain::Preferences;

    #[test]
    fn widget_glass_preferences_read_old_lenses_and_validate_edits() {
        let mut old: Preferences =
            serde_json::from_str(r#"{"enabled":true,"effect":"lens"}"#).unwrap();
        assert!(old.active_glass().is_none());
        assert!(serde_json::to_value(&old).unwrap().get("glass").is_none());
        let legacy: Glass = serde_json::from_str(r#"{"color":[81,102,138],"tint":0}"#).unwrap();
        assert_eq!(legacy.color, Some([81, 102, 138]));
        assert_eq!(legacy.tint, 0);
        let glass = Glass {
            color: Some([188, 25, 150]),
            tint: 50,
            radius: Some(0),
            ..Glass::default()
        }
        .preset(GlassMode::Frosted);
        old.glass = Some(glass);
        assert_eq!(
            serde_json::from_value::<Preferences>(serde_json::to_value(&old).unwrap())
                .unwrap()
                .active_glass(),
            Some(glass)
        );
        assert!(Glass { blur: 33, ..glass }.validate().is_err());
        assert!(
            Glass {
                radius: Some(33),
                ..glass
            }
            .validate()
            .is_err()
        );
        assert!(Glass { tint: 101, ..glass }.validate().is_err());
        old.enabled = false;
        assert!(old.active_glass().is_none());
    }
}
