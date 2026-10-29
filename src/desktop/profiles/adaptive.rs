//! Curated appearance variants. Layout, providers, mode policy and Moments
//! remain owned by their existing editors; these are ordinary profile records.
use super::*;
use crate::desktop_theme::Family;
use serde_json::json;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Look {
    #[default]
    Tidal,
    Paper,
    Moss,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Day,
    Evening,
    Night,
}
impl Look {
    pub const ALL: [Self; 3] = [Self::Tidal, Self::Paper, Self::Moss];
    pub fn title(self) -> &'static str {
        match self {
            Self::Tidal => "Tidal",
            Self::Paper => "Paper",
            Self::Moss => "Moss",
        }
    }
    pub fn family(self) -> Family {
        match self {
            Self::Paper => Family::Paper,
            _ => Family::Soft,
        }
    }
    pub fn id(self, phase: Phase) -> &'static str {
        match (self, phase) {
            (Self::Tidal, Phase::Day) => "adaptive-tidal-day",
            (Self::Tidal, Phase::Evening) => "adaptive-tidal-evening",
            (Self::Tidal, Phase::Night) => "adaptive-tidal-night",
            (Self::Paper, Phase::Day) => "adaptive-paper-day",
            (Self::Paper, Phase::Evening) => "adaptive-paper-evening",
            (Self::Paper, Phase::Night) => "adaptive-paper-night",
            (Self::Moss, Phase::Day) => "adaptive-moss-day",
            (Self::Moss, Phase::Evening) => "adaptive-moss-evening",
            (Self::Moss, Phase::Night) => "adaptive-moss-night",
        }
    }
    /// Fixed accent plus matching wallpaper, independent of forecast data.
    pub fn colors(self, phase: Phase) -> ([u8; 3], [&'static str; 3]) {
        match (self, phase) {
            (Self::Tidal, Phase::Day) => ([49, 157, 165], ["#c7e7df", "#5caaa9", "#83b9b4"]),
            (Self::Tidal, Phase::Evening) => ([207, 159, 134], ["#866969", "#697e94", "#29485b"]),
            (Self::Tidal, Phase::Night) => ([111, 169, 192], ["#274558", "#162b3c", "#0a1728"]),
            (Self::Paper, Phase::Day) => ([165, 111, 66], ["#f6eddb", "#e5d4b6", "#c7ad87"]),
            (Self::Paper, Phase::Evening) => ([182, 119, 79], ["#9a7561", "#684e48", "#3e3439"]),
            (Self::Paper, Phase::Night) => ([193, 160, 122], ["#594b43", "#352f2d", "#211e20"]),
            (Self::Moss, Phase::Day) => ([99, 148, 111], ["#d0dfb8", "#88a88b", "#8fae99"]),
            (Self::Moss, Phase::Evening) => ([184, 167, 111], ["#7d8053", "#53684d", "#2e4940"]),
            (Self::Moss, Phase::Night) => ([137, 181, 154], ["#324c40", "#20382f", "#11251f"]),
        }
    }
}
impl Phase {
    pub const ALL: [Self; 3] = [Self::Day, Self::Evening, Self::Night];
    pub fn title(self) -> &'static str {
        match self {
            Self::Day => "Day",
            Self::Evening => "Evening",
            Self::Night => "Night",
        }
    }
    pub fn event(self) -> crate::automation::solar::Event {
        use crate::automation::solar::Event;
        match self {
            Self::Day => Event::Sunrise,
            Self::Evening => Event::GoldenHour,
            Self::Night => Event::Sunset,
        }
    }
}
pub fn preset(id: &str) -> Option<Preset> {
    Look::ALL.into_iter().find_map(|look| {
        Phase::ALL
            .into_iter()
            .find(|p| look.id(*p) == id)
            .map(|p| variant(look, p))
    })
}
pub fn presets() -> Vec<Preset> {
    Look::ALL
        .into_iter()
        .flat_map(|look| Phase::ALL.map(|phase| variant(look, phase)))
        .collect()
}
pub fn variant(look: Look, phase: Phase) -> Preset {
    let (seed, stops) = look.colors(phase);
    let mut profile = crate::automation::appearance::project(&themes::record(look.family()))
        .expect("curated projection");
    // This includes wallpaper, so it is a restricted setup rather than a saved theme.
    profile.appearance_only = false;
    profile.name = format!("{} · {}", look.title(), phase.title());
    themes::LookColor::Fixed(seed).set(&mut profile);
    profile.wallpaper = Some(WallpaperLook {
        background: Some(BackgroundLook {
            source: Some(SourceLook::Gradient(GradientLook {
                stops: stops.into_iter().map(|c| Rgb::Hex(c.into())).collect(),
                angle: 180.,
            })),
            ..Default::default()
        }),
        ..Default::default()
    });
    // Let wallpaper-aware text use the real variant polarity until analysis arrives.
    profile.widget_style.as_mut().unwrap()["appearance"]["wallpaper_light"] =
        json!(phase == Phase::Day);
    profile.version = profile.written_version();
    profile.validate().expect("curated adaptive profile");
    Preset {
        id: look.id(phase),
        description: "Colors and surface style follow the day. Your layout stays in place.",
        profile,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adaptive_variants_are_portable_and_preserve_geometry_mode_and_content() {
        for look in Look::ALL {
            let mut appearances = Vec::new();
            for phase in Phase::ALL {
                let p = variant(look, phase).profile;
                assert_eq!(import(&export(&p).unwrap().bytes).unwrap(), p);
                assert!(p.widget_layout.is_none() && p.windows.is_none() && p.workspaces.is_none());
                assert!(p.appearance.as_ref().unwrap().mode.is_none());
                for b in [p.panel.as_ref().unwrap(), p.dock.as_ref().unwrap()] {
                    assert!(
                        b.islands.is_none()
                            && b.spacing.is_none()
                            && b.margin.is_none()
                            && b.padding.is_none()
                            && b.size.is_none()
                    );
                }
                let style = p.widget_style.as_ref().unwrap();
                assert!(
                    style.get("clock").is_none()
                        && style.get("moments").is_none()
                        && style.get("recipes").is_none()
                );
                assert!(!appearances.contains(&p.wallpaper));
                appearances.push(p.wallpaper.clone());
            }
        }
    }
}
