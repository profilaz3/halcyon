//! Color-independent finishes. Separate namespace leaves version-1 layouts readable.
use serde::{Deserialize, Serialize};
pub const NAMESPACE: &str = "io.github.profilaz3.Halcyon.Materials";
/// How see-through a widget surface may be.
pub const MIN_OPACITY: u8 = 10;
/// Dock and panel records keep the floor the installed panel accepts.
pub const BAR_MIN_OPACITY: u8 = 45;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Frosted,
    Clear,
    Smoked,
    Liquid,
}
impl Kind {
    pub const ALL: [Self; 4] = [Self::Frosted, Self::Clear, Self::Smoked, Self::Liquid];
    pub fn title(self) -> &'static str {
        match self {
            Self::Frosted => "Frosted",
            Self::Clear => "Clear glass",
            Self::Smoked => "Smoked glass",
            Self::Liquid => "Liquid",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Frosted => "frosted",
            Self::Clear => "clear",
            Self::Smoked => "smoked",
            Self::Liquid => "liquid",
        }
    }
    pub fn opacity(self) -> u8 {
        match self {
            Self::Frosted => 82,
            Self::Clear => 62,
            Self::Smoked => 94,
            Self::Liquid => 76,
        }
    }
    pub fn highlight(self) -> f32 {
        match self {
            Self::Frosted => 0.035,
            Self::Clear => 0.06,
            Self::Smoked => 0.025,
            Self::Liquid => 0.13,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finish {
    pub kind: Kind,
    pub opacity: Option<u8>,
    pub tint: u8,
}
impl Finish {
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
            opacity: None,
            tint: 35,
        }
    }
    pub fn opacity(self) -> u8 {
        self.opacity
            .unwrap_or(self.kind.opacity())
            .clamp(MIN_OPACITY, 100)
    }
    pub fn validate(self) -> Result<(), String> {
        if self.opacity.is_some_and(|v| !(MIN_OPACITY..=100).contains(&v)) || self.tint > 100 {
            Err(format!(
                "Material opacity must be {MIN_OPACITY} to 100 and tint 0 to 100"
            ))
        } else {
            Ok(())
        }
    }
    /// Native/app palette remains the color owner; only its accent's surface contribution changes.
    pub fn color(self, mut color: [f32; 4], accent: [f32; 3]) -> [f32; 4] {
        let amount = self.tint.min(100) as f32 / 100. * 0.16;
        for (c, accent) in color[..3].iter_mut().zip(accent) {
            *c = (*c * (1. - amount) + accent * amount).clamp(0., 1.);
        }
        color[3] = self.opacity() as f32 / 100.;
        color
    }
}
/// Versioned independent per-bar record; unknown versions are preserved, ignored by renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarMaterial {
    pub version: u32,
    pub finish: Finish,
}
impl BarMaterial {
    /// A bar record for a finish, raised to the floor bars accept.
    pub fn new(mut finish: Finish) -> Self {
        if finish.opacity() < BAR_MIN_OPACITY {
            finish.opacity = Some(BAR_MIN_OPACITY);
        }
        Self { version: 1, finish }
    }
    pub fn validate(self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Unsupported material version".into());
        }
        if self.finish.opacity.is_some_and(|v| v < BAR_MIN_OPACITY) {
            return Err(format!(
                "Dock and panel opacity must be {BAR_MIN_OPACITY} to 100"
            ));
        }
        self.finish.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn four_finishes_and_tint_work_even_on_a_neutral_native_theme() {
        let base = [0.13, 0.13, 0.13, 1.];
        let accent = [0.2, 0.7, 0.8];
        let mut alphas = Vec::new();
        for kind in Kind::ALL {
            let mut f = Finish::new(kind);
            assert!(f.validate().is_ok());
            let tinted = f.color(base, accent);
            f.tint = 0;
            let neutral = f.color(base, accent);
            assert_ne!(neutral, tinted);
            assert_eq!(&neutral[..3], &base[..3]);
            assert!(!alphas.contains(&neutral[3]));
            alphas.push(neutral[3]);
            f.opacity = Some(10);
            assert_eq!(f.color(base, accent)[3], 0.10);
        }
    }
    #[test]
    fn material_schema_rejects_unknown_versions_and_out_of_range_values() {
        let mut record = BarMaterial {
            version: 1,
            finish: Finish::new(Kind::Liquid),
        };
        record.version = 2;
        assert!(record.validate().is_err());
        record.version = 1;
        record.finish.tint = 101;
        assert!(record.validate().is_err());
        record.finish.tint = 35;
        record.finish.opacity = Some(44);
        assert!(record.validate().is_err());
        // Widgets may go further; bars keep the floor the panel accepts.
        let mut clear = Finish::new(Kind::Clear);
        clear.opacity = Some(MIN_OPACITY);
        assert!(clear.validate().is_ok());
        clear.opacity = Some(MIN_OPACITY - 1);
        assert!(clear.validate().is_err());
        clear.opacity = Some(20);
        let bar = BarMaterial::new(clear);
        assert_eq!(bar.finish.opacity, Some(BAR_MIN_OPACITY));
        assert!(bar.validate().is_ok());
        clear.opacity = Some(70);
        assert_eq!(BarMaterial::new(clear).finish.opacity, Some(70));
        // An unset opacity keeps following its kind.
        assert_eq!(BarMaterial::new(Finish::new(Kind::Smoked)).finish.opacity, None);
    }
}
