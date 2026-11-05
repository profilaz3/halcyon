//! Material is independent of color, typography, content and layout.
use crate::config::Palette;
use cosmic::iced::{Background, Color};
pub use dock_islands_core::material::{Finish, Kind};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Materials {
    pub shared: Option<Finish>,
    pub widgets: Option<Finish>,
    pub dock: Option<Finish>,
    pub panel: Option<Finish>,
}
impl Materials {
    pub fn widgets(self) -> Option<Finish> {
        self.widgets.or(self.shared)
    }
    pub fn validate(self) -> Result<(), String> {
        for f in [self.shared, self.widgets, self.dock, self.panel]
            .into_iter()
            .flatten()
        {
            f.validate()?;
        }
        Ok(())
    }
    pub fn get(self, target: Target) -> Option<Finish> {
        match target {
            Target::Shared => self.shared,
            Target::Widgets => self.widgets,
            Target::Dock => self.dock,
            Target::Panel => self.panel,
        }
    }
    pub fn effective(self, target: Target) -> Option<Finish> {
        self.get(target).or(self.shared)
    }
    pub fn set(&mut self, target: Target, finish: Option<Finish>) {
        match target {
            Target::Shared => self.shared = finish,
            Target::Widgets => self.widgets = finish,
            Target::Dock => self.dock = finish,
            Target::Panel => self.panel = finish,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Target {
    #[default]
    Shared,
    Widgets,
    Dock,
    Panel,
}
impl Target {
    pub const ALL: [Self; 4] = [Self::Shared, Self::Widgets, Self::Dock, Self::Panel];
    pub fn title(self) -> &'static str {
        match self {
            Self::Shared => "Everything",
            Self::Widgets => "Widgets",
            Self::Dock => "Dock",
            Self::Panel => "Panel",
        }
    }
    pub fn pointer(self) -> &'static str {
        match self {
            Self::Shared => "/materials/shared",
            Self::Widgets => "/materials/widgets",
            Self::Dock => "/materials/dock",
            Self::Panel => "/materials/panel",
        }
    }
}
pub fn palette(mut p: Palette, finish: Option<Finish>, assist: bool) -> Palette {
    if p.background.a == 0. {
        return p;
    }
    if let Some(f) = finish {
        p.background = Color::from(f.color(
            [
                p.background.r,
                p.background.g,
                p.background.b,
                p.background.a,
            ],
            [p.accent.r, p.accent.g, p.accent.b],
        ));
        // A dense neutral smoke remains readable in both native light and dark themes.
        // Color polarity belongs to the color theme, not the material.
        if assist {
            p = crate::widget_style::readable(p);
        }
    }
    p
}
pub fn background(color: Color, finish: Option<Finish>) -> Background {
    let Some(f) = finish.filter(|_| color.a > 0.) else {
        return Background::Color(color);
    };
    let mix = |amount: f32| {
        Color::from_rgba(
            color.r + (1. - color.r) * amount,
            color.g + (1. - color.g) * amount,
            color.b + (1. - color.b) * amount,
            color.a,
        )
    };
    cosmic::iced::gradient::Linear::new(std::f32::consts::FRAC_PI_2)
        .add_stop(0., mix(f.kind.highlight()))
        .add_stop(0.25, mix(f.kind.highlight() * 0.25))
        .add_stop(0.7, color)
        .add_stop(1., mix(f.kind.highlight() * 0.35))
        .into()
}

mod sheen;
pub use sheen::sheen;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn material_keeps_fonts_transparent_surfaces_and_color_policy_and_recolors_live() {
        for family in crate::desktop_theme::Family::ALL {
            let mut config = crate::config::Config::default();
            config.appearance.family = Some(family);
            config.appearance.accent = Some([30, 150, 170]);
            let legacy = config.palette();
            let legacy_glance = config.glance_panel();
            let font = {
                let _scope = crate::widget_style::scope(&config);
                crate::widget_style::font()
            };
            for kind in Kind::ALL {
                config.materials.shared = Some(Finish::new(kind));
                assert_eq!(config.glance_panel(), legacy_glance);
                assert_eq!(config.transparent_widgets(), legacy.background.a == 0.);
                assert_eq!(
                    {
                        let _scope = crate::widget_style::scope(&config);
                        crate::widget_style::font()
                    },
                    font
                );
                let old = config.palette();
                config.appearance.accent = Some([190, 70, 50]);
                assert_ne!(config.palette().accent, old.accent, "{family:?}");
                assert_eq!(config.materials.shared.unwrap().kind, kind);
                config.appearance.accent = Some([30, 150, 170]);
            }
            config.materials.shared = None;
            assert_eq!(config.palette(), legacy);
        }
    }
}
