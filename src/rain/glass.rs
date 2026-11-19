//! The optical finish of a widget pane. An absent finish keeps the original Liquid lens.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlassMode {
    Smoked,
    Polished,
    Frosted,
    Crystal,
}
impl GlassMode {
    pub const ALL: [Self; 4] = [Self::Smoked, Self::Polished, Self::Crystal, Self::Frosted];

    pub fn title(self) -> &'static str {
        match self {
            Self::Smoked => "Smoked",
            Self::Polished => "Polished",
            Self::Frosted => "Frosted",
            Self::Crystal => "Crystal",
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
        if mode == GlassMode::Crystal {
            return Self { mode, frost: 0, blur: 0, tint: 3, opacity: 30, ..self };
        }
        let (frost, blur) = match mode {
            GlassMode::Frosted => (35, 12),
            _ => (0, 0),
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

/// Stable keys survive renames and travel with a shared Look.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FinishLibrary {
    pub entries: std::collections::BTreeMap<u32, NamedFinish>,
    pub links: std::collections::BTreeMap<crate::drawer::Widget, u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedFinish {
    pub name: String,
    pub glass: Option<Glass>,
    pub refraction: u8,
}

impl FinishLibrary {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.links.is_empty()
    }

    pub fn valid_name(&self, name: &str, excluding: Option<u32>) -> bool {
        let name = name.trim();
        !name.is_empty()
            && name.chars().count() <= 48
            && !name.chars().any(char::is_control)
            && !name.eq_ignore_ascii_case("Shared finish")
            && !self.entries.iter().any(|(id, f)| {
                Some(*id) != excluding && f.name.to_lowercase() == name.to_lowercase()
            })
    }

    pub fn copy_name(&self, id: u32) -> Option<String> {
        let base = &self.entries.get(&id)?.name;
        for n in 1..=33 {
            let suffix = if n == 1 {
                " copy".into()
            } else {
                format!(" copy {n}")
            };
            let name = format!(
                "{}{suffix}",
                base.chars()
                    .take(48 - suffix.len())
                    .collect::<String>()
                    .trim_end()
            );
            if self.valid_name(&name, None) {
                return Some(name);
            }
        }
        None
    }

    pub fn insert(&mut self, name: &str, glass: Option<Glass>, refraction: u8) -> Option<u32> {
        if !self.valid_name(name, None) || self.entries.len() >= 32 {
            return None;
        }
        let id = loop {
            let key = getrandom::u32().ok()?;
            if key != 0 && !self.entries.contains_key(&key) {
                break key;
            }
        };
        self.entries.insert(
            id,
            NamedFinish {
                name: name.trim().to_owned(),
                glass,
                refraction,
            },
        );
        Some(id)
    }

    pub fn remove(&mut self, id: u32) {
        self.links.retain(|_, link| *link != id);
        self.entries.remove(&id);
    }

    pub fn linked(&self, widget: crate::drawer::Widget) -> Option<&NamedFinish> {
        self.entries.get(self.links.get(&widget)?)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.entries.len() > 32 || self.links.values().any(|id| !self.entries.contains_key(id)) {
            return Err("Invalid glass finish library or widget assignment".into());
        }
        for (&id, finish) in &self.entries {
            if id == 0 || !self.valid_name(&finish.name, Some(id)) || finish.refraction > 100 {
                return Err("Invalid saved glass finish".into());
            }
            if let Some(glass) = finish.glass {
                glass.validate()?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod library_tests {
    use super::*;
    use crate::{
        drawer::Widget,
        rain::{Effect, Preferences},
    };
    #[test]
    fn linked_optics_round_trip_edit_copy_rename_and_remove() {
        let mut p = Preferences {
            enabled: true,
            effect: Effect::Lens,
            ..Default::default()
        };
        let glass = Some(Glass::default().preset(GlassMode::Frosted));
        let id = p.finishes.insert("Evening frost", glass, 30).unwrap();
        for w in [Widget::Weather, Widget::Messages] {
            p.finishes.links.insert(w, id);
        }
        let other = p
            .finishes
            .insert("Polished", Some(Glass::default()), 70)
            .unwrap();
        p.finishes.links.insert(Widget::Music, other);
        let mut draft = p.edit_finish(Some(id));
        draft.glass.as_mut().unwrap().opacity = 81;
        p = p.store_finish(draft, Some(id));
        p.finishes.entries.get_mut(&id).unwrap().name = "Night".into();
        assert_eq!(p.glass, None);
        assert_eq!(
            p.finishes.linked(Widget::Weather),
            p.finishes.linked(Widget::Messages)
        );
        assert_eq!(
            p.finishes
                .linked(Widget::Weather)
                .unwrap()
                .glass
                .unwrap()
                .opacity,
            81
        );
        assert_ne!(
            p.finishes.linked(Widget::Music),
            p.finishes.linked(Widget::Weather)
        );
        let mut profile = crate::desktop::profiles::Profile::new("Evening").unwrap();
        profile.widget_style = Some(serde_json::json!({"rain":p}));
        let bytes = crate::desktop::profiles::export(&profile).unwrap().bytes;
        let imported = crate::desktop::profiles::import(&bytes).unwrap();
        assert_eq!(imported.version, 27);
        let restored: Preferences =
            serde_json::from_value(imported.widget_style.unwrap()["rain"].clone()).unwrap();
        assert_eq!(restored, p);
        p.finishes.remove(id);
        assert!(p.finishes.linked(Widget::Weather).is_none());
        assert!(p.finishes.linked(Widget::Music).is_some());
        p.validate().unwrap();
    }
}
