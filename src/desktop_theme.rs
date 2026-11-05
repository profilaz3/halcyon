//! Shared visual roles. Resolution is cached per configuration, never does I/O,
//! and keeps surface ink separate from text drawn directly on the wallpaper.
//!
//! One seed color (the wallpaper's or the theme's own) gives every role its
//! hue. Each role keeps its own lightness in OKLCH (see `tone`), so readable
//! text and native accents keep the color instead of fading it to gray.
pub mod tone;

use crate::config::{Appearance, Config, Palette};
use cosmic::iced::{
    Color, Font,
    font::{Family as FontFamily, Weight},
};
use serde::{Deserialize, Serialize};
use tone::Lch;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Soft,
    Outline,
    Paper,
    Bare,
    Glass,
    Frosted,
    Midnight,
    Terminal,
    Neon,
    Editorial,
    Retro,
    Pastel,
    Bauhaus,
}

/// What surrounds a card besides its border.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shadow {
    None,
    /// A blurred drop shadow.
    Soft,
    /// A solid offset shadow in the ink color.
    Hard,
    /// A blurred halo in the accent.
    Glow,
}
impl Shadow {
    /// Room a card leaves inside its surface so the shadow isn't clipped.
    pub fn margin(self) -> f32 {
        match self {
            Self::None => 0.,
            Self::Hard => 4.,
            Self::Soft | Self::Glow => 12.,
        }
    }
}

/// The generic family used when none of a style's named fonts is installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generic {
    Sans,
    Serif,
    Mono,
}

/// A style's typeface for one role: installed names in order of preference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontSpec {
    pub names: &'static [&'static str],
    pub generic: Generic,
    pub weight: Weight,
}
impl FontSpec {
    /// The first installed name, else the generic family; `installed` is
    /// asked about each name in turn.
    pub fn font(&self, installed: impl Fn(&str) -> bool) -> Font {
        let family = self
            .names
            .iter()
            .find(|name| installed(name))
            .map(|name| FontFamily::Name(name))
            .unwrap_or(match self.generic {
                Generic::Sans => FontFamily::SansSerif,
                Generic::Serif => FontFamily::Serif,
                Generic::Mono => FontFamily::Monospace,
            });
        Font {
            family,
            weight: self.weight,
            ..Font::DEFAULT
        }
    }
}

/// How the dock and panel look under a style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarSpec {
    pub opacity: f32,
    pub radius: u32,
    pub padding: u32,
    pub spacing: u32,
    pub island_gap: u16,
    /// Float away from the screen edge (round styles) or stay attached.
    pub floating: bool,
}

/// Everything one style decides, in one place. Adding a style is adding a
/// spec (and a `Family` variant); nothing else hardcodes per-style values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StyleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// The default seed color.
    pub accent: [u8; 3],
    /// Owns COSMIC's Light variant (and Light mode) instead of Dark.
    pub light: bool,
    /// Card surface in OKLCH: lightness, share of the seed's chroma, the
    /// chroma cap, and a fixed (a, b) shift (Paper's warmth).
    pub surface: (f32, f32, f32, (f32, f32)),
    /// Card opacity %, corner radius, border opacity %.
    pub geometry: (u8, u16, u8),
    pub border_width: f32,
    pub shadow: Shadow,
    /// Legacy frosted style classification; rendering is decorative and
    /// does not enable native backdrop effects.
    pub frosted: bool,
    /// Cards are transparent over the wallpaper by default.
    pub transparent: bool,
    pub body: FontSpec,
    pub clock: FontSpec,
    pub bar: BarSpec,
    /// The matching gradient wallpaper.
    pub wallpaper: ([u8; 3], [u8; 3]),
}

const SOFT: StyleSpec = StyleSpec {
    id: "soft",
    title: "Soft",
    description: "Soft and rounded",
    accent: [157, 207, 190],
    light: false,
    surface: (0.305, 0.30, 0.034, (0., 0.)),
    geometry: (94, 25, 20),
    border_width: 1.,
    shadow: Shadow::None,
    frosted: false,
    transparent: false,
    body: FontSpec {
        names: &[],
        generic: Generic::Sans,
        weight: Weight::Normal,
    },
    clock: FontSpec {
        names: &[],
        generic: Generic::Sans,
        weight: Weight::Light,
    },
    bar: BarSpec {
        opacity: 0.94,
        radius: 20,
        padding: 8,
        spacing: 6,
        island_gap: 12,
        floating: true,
    },
    wallpaper: ([18, 27, 29], [40, 73, 67]),
};
const OUTLINE: StyleSpec = StyleSpec {
    id: "outline",
    title: "Outline",
    description: "Thin lines, typewriter text",
    accent: [164, 193, 223],
    light: false,
    surface: (0.218, 0.16, 0.020, (0., 0.)),
    geometry: (98, 5, 50),
    border_width: 1.,
    shadow: Shadow::None,
    frosted: false,
    transparent: false,
    body: FontSpec {
        names: &[],
        generic: Generic::Mono,
        weight: Weight::Normal,
    },
    clock: FontSpec {
        names: &[],
        generic: Generic::Mono,
        weight: Weight::Normal,
    },
    bar: BarSpec {
        opacity: 0.98,
        radius: 5,
        padding: 6,
        spacing: 4,
        island_gap: 8,
        floating: false,
    },
    wallpaper: ([12, 16, 21], [32, 40, 51]),
};
const PAPER: StyleSpec = StyleSpec {
    id: "paper",
    title: "Paper",
    description: "Warm paper, dark ink",
    accent: [60, 112, 91],
    light: true,
    surface: (0.965, 0.05, 0.006, (0.002, 0.012)),
    geometry: (100, 12, 28),
    border_width: 1.,
    shadow: Shadow::None,
    frosted: false,
    transparent: false,
    body: FontSpec {
        names: &[],
        generic: Generic::Sans,
        weight: Weight::Normal,
    },
    clock: FontSpec {
        names: &["Noto Serif"],
        generic: Generic::Serif,
        weight: Weight::Normal,
    },
    bar: BarSpec {
        opacity: 1.,
        radius: 12,
        padding: 8,
        spacing: 6,
        island_gap: 12,
        floating: false,
    },
    wallpaper: ([223, 222, 208], [240, 235, 223]),
};
const BARE: StyleSpec = StyleSpec {
    id: "bare",
    title: "Bare",
    description: "Open and quiet",
    accent: [209, 199, 158],
    light: false,
    surface: (0.282, 0.22, 0.026, (0., 0.)),
    geometry: (92, 24, 0),
    border_width: 1.,
    shadow: Shadow::None,
    frosted: false,
    transparent: true,
    body: FontSpec {
        names: &[],
        generic: Generic::Sans,
        weight: Weight::Normal,
    },
    clock: FontSpec {
        names: &[],
        generic: Generic::Sans,
        weight: Weight::Light,
    },
    bar: BarSpec {
        opacity: 0.90,
        radius: 24,
        padding: 6,
        spacing: 8,
        island_gap: 16,
        floating: true,
    },
    wallpaper: ([21, 29, 26], [61, 68, 51]),
};
const GLASS: StyleSpec = StyleSpec {
    id: "glass",
    title: "Frosted",
    description: "See-through cards",
    accent: [184, 162, 237],
    light: false,
    surface: (0.30, 0.40, 0.05, (0., 0.)),
    geometry: (70, 22, 40),
    border_width: 1.,
    shadow: Shadow::Soft,
    frosted: false,
    transparent: false,
    body: FontSpec {
        names: &["Fira Sans"],
        generic: Generic::Sans,
        weight: Weight::Normal,
    },
    clock: FontSpec {
        names: &["Fira Sans"],
        generic: Generic::Sans,
        weight: Weight::ExtraLight,
    },
    bar: BarSpec {
        opacity: 0.72,
        radius: 18,
        padding: 8,
        spacing: 6,
        island_gap: 12,
        floating: true,
    },
    wallpaper: ([29, 43, 74], [91, 75, 138]),
};
const FROSTED: StyleSpec = StyleSpec {
    id: "frosted",
    title: "Frosted glass",
    description: "Tinted, easy to read",
    frosted: true,
    wallpaper: ([30, 40, 60], [150, 100, 140]),
    ..GLASS
};
const MIDNIGHT: StyleSpec = StyleSpec {
    id: "midnight",
    title: "Midnight",
    description: "True black, thin text",
    accent: [122, 162, 255],
    light: false,
    surface: (0.0, 0.0, 0.0, (0., 0.)),
    geometry: (100, 14, 0),
    border_width: 1.,
    shadow: Shadow::None,
    frosted: false,
    transparent: false,
    body: FontSpec {
        names: &["Fira Sans"],
        generic: Generic::Sans,
        weight: Weight::Light,
    },
    // ExtraLight: Thin strokes vanish at widget sizes.
    clock: FontSpec {
        names: &["Fira Sans"],
        generic: Generic::Sans,
        weight: Weight::ExtraLight,
    },
    bar: BarSpec {
        opacity: 1.,
        radius: 10,
        padding: 6,
        spacing: 6,
        island_gap: 8,
        floating: false,
    },
    wallpaper: ([0, 0, 0], [10, 10, 16]),
};
const TERMINAL: StyleSpec = StyleSpec {
    id: "terminal",
    title: "Terminal",
    description: "Old green screen",
    accent: [142, 240, 168],
    light: false,
    surface: (0.16, 0.25, 0.04, (0., 0.)),
    geometry: (98, 0, 50),
    border_width: 1.,
    shadow: Shadow::None,
    frosted: false,
    transparent: false,
    body: FontSpec {
        names: &["Ubuntu Mono", "Fira Mono", "DejaVu Sans Mono"],
        generic: Generic::Mono,
        weight: Weight::Normal,
    },
    clock: FontSpec {
        names: &["Ubuntu Mono", "Fira Mono", "DejaVu Sans Mono"],
        generic: Generic::Mono,
        weight: Weight::Normal,
    },
    bar: BarSpec {
        opacity: 1.,
        radius: 0,
        padding: 4,
        spacing: 4,
        island_gap: 6,
        floating: false,
    },
    wallpaper: ([4, 12, 7], [10, 32, 18]),
};
const NEON: StyleSpec = StyleSpec {
    id: "neon",
    title: "Neon",
    description: "Glowing edges",
    accent: [255, 79, 216],
    light: false,
    surface: (0.17, 0.5, 0.08, (0., 0.)),
    geometry: (96, 12, 50),
    border_width: 1.5,
    shadow: Shadow::Glow,
    frosted: false,
    transparent: false,
    body: FontSpec {
        names: &["Fira Sans Condensed", "Fira Sans"],
        generic: Generic::Sans,
        weight: Weight::Normal,
    },
    clock: FontSpec {
        names: &["Fira Sans Condensed", "Fira Sans"],
        generic: Generic::Sans,
        weight: Weight::Semibold,
    },
    bar: BarSpec {
        opacity: 0.96,
        radius: 12,
        padding: 6,
        spacing: 6,
        island_gap: 10,
        floating: true,
    },
    wallpaper: ([11, 10, 20], [44, 14, 52]),
};
const EDITORIAL: StyleSpec = StyleSpec {
    id: "editorial",
    title: "Editorial",
    description: "Magazine serif",
    accent: [192, 57, 43],
    light: true,
    surface: (0.99, 0.02, 0.004, (0., 0.)),
    geometry: (100, 2, 35),
    border_width: 1.,
    shadow: Shadow::None,
    frosted: false,
    transparent: false,
    body: FontSpec {
        names: &["P052", "C059", "Noto Serif"],
        generic: Generic::Serif,
        weight: Weight::Normal,
    },
    clock: FontSpec {
        names: &["Noto Serif Display", "Noto Serif", "P052"],
        generic: Generic::Serif,
        weight: Weight::Normal,
    },
    bar: BarSpec {
        opacity: 1.,
        radius: 2,
        padding: 6,
        spacing: 6,
        island_gap: 8,
        floating: false,
    },
    wallpaper: ([242, 240, 234], [222, 218, 208]),
};
const RETRO: StyleSpec = StyleSpec {
    id: "retro",
    title: "Retro",
    description: "Bold outlines",
    accent: [217, 98, 43],
    light: true,
    surface: (0.965, 0.25, 0.03, (0.004, 0.02)),
    geometry: (100, 8, 50),
    border_width: 2.,
    shadow: Shadow::Hard,
    frosted: false,
    transparent: false,
    body: FontSpec {
        names: &["URW Bookman", "P052"],
        generic: Generic::Serif,
        weight: Weight::Normal,
    },
    clock: FontSpec {
        names: &["URW Bookman", "P052"],
        generic: Generic::Serif,
        weight: Weight::Semibold,
    },
    bar: BarSpec {
        opacity: 1.,
        radius: 8,
        padding: 6,
        spacing: 6,
        island_gap: 10,
        floating: true,
    },
    wallpaper: ([246, 227, 200], [232, 199, 154]),
};
const PASTEL: StyleSpec = StyleSpec {
    id: "pastel",
    title: "Pastel",
    description: "Round and friendly",
    accent: [185, 174, 240],
    light: true,
    surface: (0.975, 0.35, 0.03, (0., 0.)),
    geometry: (88, 26, 0),
    border_width: 1.,
    shadow: Shadow::Soft,
    frosted: false,
    transparent: false,
    body: FontSpec {
        names: &["Ubuntu", "Ubuntu Sans"],
        generic: Generic::Sans,
        weight: Weight::Normal,
    },
    clock: FontSpec {
        names: &["Ubuntu", "Ubuntu Sans"],
        generic: Generic::Sans,
        weight: Weight::Medium,
    },
    bar: BarSpec {
        opacity: 0.9,
        radius: 28,
        padding: 8,
        spacing: 8,
        island_gap: 14,
        floating: true,
    },
    wallpaper: ([253, 238, 243], [233, 240, 255]),
};
const BAUHAUS: StyleSpec = StyleSpec {
    id: "bauhaus",
    title: "Bauhaus",
    description: "Shapes and primary colors",
    accent: [209, 59, 43],
    light: true,
    surface: (0.985, 0.0, 0.0, (0., 0.)),
    geometry: (100, 0, 50),
    border_width: 3.,
    shadow: Shadow::None,
    frosted: false,
    transparent: false,
    body: FontSpec {
        names: &["URW Gothic"],
        generic: Generic::Sans,
        weight: Weight::Normal,
    },
    clock: FontSpec {
        names: &["URW Gothic"],
        generic: Generic::Sans,
        weight: Weight::Bold,
    },
    bar: BarSpec {
        opacity: 1.,
        radius: 0,
        padding: 6,
        spacing: 6,
        island_gap: 8,
        floating: false,
    },
    wallpaper: ([233, 228, 216], [214, 206, 190]),
};

impl Family {
    pub const ALL: [Self; 13] = [
        Self::Soft,
        Self::Outline,
        Self::Paper,
        Self::Bare,
        Self::Glass,
        Self::Frosted,
        Self::Midnight,
        Self::Terminal,
        Self::Neon,
        Self::Editorial,
        Self::Retro,
        Self::Pastel,
        Self::Bauhaus,
    ];
    pub fn spec(self) -> &'static StyleSpec {
        match self {
            Self::Soft => &SOFT,
            Self::Outline => &OUTLINE,
            Self::Paper => &PAPER,
            Self::Bare => &BARE,
            Self::Glass => &GLASS,
            Self::Frosted => &FROSTED,
            Self::Midnight => &MIDNIGHT,
            Self::Terminal => &TERMINAL,
            Self::Neon => &NEON,
            Self::Editorial => &EDITORIAL,
            Self::Retro => &RETRO,
            Self::Pastel => &PASTEL,
            Self::Bauhaus => &BAUHAUS,
        }
    }
    pub fn id(self) -> &'static str {
        self.spec().id
    }
    /// The built-in theme record's library ID.
    pub fn preset(self) -> &'static str {
        match self {
            Self::Soft => "theme-soft",
            Self::Outline => "theme-outline",
            Self::Paper => "theme-paper",
            Self::Bare => "theme-bare",
            Self::Glass => "theme-glass",
            Self::Frosted => "theme-frosted",
            Self::Midnight => "theme-midnight",
            Self::Terminal => "theme-terminal",
            Self::Neon => "theme-neon",
            Self::Editorial => "theme-editorial",
            Self::Retro => "theme-retro",
            Self::Pastel => "theme-pastel",
            Self::Bauhaus => "theme-bauhaus",
        }
    }
    pub fn title(self) -> &'static str {
        self.spec().title
    }
    pub fn description(self) -> &'static str {
        self.spec().description
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|f| f.id() == value || f.preset() == value)
    }
    pub fn accent(self) -> [u8; 3] {
        self.spec().accent
    }
    pub fn geometry(self) -> (u8, u16, u8) {
        self.spec().geometry
    }
    pub fn bar(self) -> (f32, u32, u32, u32) {
        let bar = self.spec().bar;
        (bar.opacity, bar.radius, bar.padding, bar.spacing)
    }
    pub fn wallpaper(self) -> ([u8; 3], [u8; 3]) {
        self.spec().wallpaper
    }
    /// The family's initial mode; native palettes exist for both modes.
    pub fn light(self) -> bool {
        self.spec().light
    }
    /// The card surface: the style's lightness, tinted by the seed's hue.
    pub fn surface(self, seed: [u8; 3]) -> [u8; 3] {
        self.surface_for(seed, !self.light())
    }
    /// Native surface in either mode. Keep the family's tint and its original
    /// polarity unchanged; the opposite variant uses a readable tonal range.
    pub fn surface_for(self, seed: [u8; 3], dark: bool) -> [u8; 3] {
        let seed = tone::lch(seed);
        let (mut l, share, most, (shift_a, shift_b)) = self.spec().surface;
        if dark == self.light() {
            l = if dark { 0.28 } else { 0.965 };
        }
        let c = (seed.c * share).min(most);
        let (a, b) = (c * seed.h.cos() + shift_a, c * seed.h.sin() + shift_b);
        tone::rgb(Lch {
            l,
            c: a.hypot(b),
            h: b.atan2(a),
        })
    }
    /// Containers inside COSMIC windows sit one step above the background.
    #[cfg(test)]
    pub fn container(self, seed: [u8; 3]) -> [u8; 3] {
        self.container_for(seed, !self.light())
    }
    pub fn container_for(self, seed: [u8; 3], dark: bool) -> [u8; 3] {
        let surface = self.surface_for(seed, dark);
        let step = if dark { 0.038 } else { -0.032 };
        tone::at(surface, tone::lch(surface).l + step)
    }
}

/// Font family names the renderer has, loaded once at startup (see
/// `load_installed_fonts`). Before that, styles use their generic fallback.
static INSTALLED: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
/// Reads the renderer's font database once. Call early, outside drawing.
pub fn load_installed_fonts() {
    if INSTALLED.get().is_some() {
        return;
    }
    let system = cosmic::iced::advanced::graphics::text::font_system();
    let Ok(mut system) = system.try_write() else {
        return;
    };
    let mut names: Vec<String> = system
        .raw()
        .db()
        .faces()
        .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
        .collect();
    names.sort();
    names.dedup();
    let _ = INSTALLED.set(names);
}
pub fn font_installed(name: &str) -> bool {
    INSTALLED.get().is_some_and(|names| {
        names
            .binary_search_by(|known| known.as_str().cmp(name))
            .is_ok()
    })
}

/// Exact face support from the renderer's already loaded database. This does
/// not run font discovery or load a file while building Settings controls.
pub fn font_style_available(font: Font) -> bool {
    use cosmic::iced::advanced::graphics::text::{self, cosmic_text::fontdb};
    let attrs = text::to_attributes(font);
    let system = text::font_system().read().expect("Read font catalogue");
    let query = fontdb::Query {
        families: &[attrs.family],
        weight: attrs.weight,
        stretch: attrs.stretch,
        style: attrs.style,
    };
    system
        .db()
        .query(&query)
        .and_then(|id| system.db().face(id))
        .is_some_and(|face| {
            face.weight == attrs.weight
                && face.style == attrs.style
                && face.stretch == attrs.stretch
        })
}

/// COSMIC's own palettes use a pastel accent in Dark and a deep one in Light,
/// so one shared color becomes two native accents of the same hue.
pub fn native_accent(seed: [u8; 3], dark: bool) -> [u8; 3] {
    if dark {
        tone::within(seed, 0.72..=0.88)
    } else {
        tone::within(seed, 0.38..=0.53)
    }
}

/// The previous release's native roles: `(accent, surface)` for both
/// variants. Only used to recognize surfaces it applied as still linked.
pub fn previous_native(family: Family, seed: Option<[u8; 3]>) -> ([u8; 3], [u8; 3]) {
    let accent = rgb(seed.unwrap_or_else(|| family.accent()));
    let surface = if family == Family::Paper {
        mix(accent, rgb([250, 247, 239]), 0.025)
    } else {
        let base = match family {
            Family::Soft => [29, 36, 37],
            Family::Outline => [18, 22, 28],
            _ => [34, 33, 29],
        };
        mix(
            accent,
            rgb(base),
            if family == Family::Soft { 0.055 } else { 0.025 },
        )
    };
    (bytes(ink(accent, surface)), bytes(surface))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Panel,
    Transparent,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Roles {
    pub card: Palette,
    /// Filled counterpart for the drawer and layout controls.
    pub surface: Palette,
    pub wallpaper: Palette,
    pub body: Font,
    pub clock: Font,
    pub transparent: bool,
    pub warning: Color,
    pub error: Color,
    pub border_width: f32,
    pub shadow: Shadow,
    pub frosted: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Cache {
    appearance: Appearance,
    roles: Roles,
}
fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgb8(c[0], c[1], c[2])
}
pub fn bytes(c: Color) -> [u8; 3] {
    tone::bytes(c)
}
fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgb(
        a.r * t + b.r * (1. - t),
        a.g * t + b.g * (1. - t),
        a.b * t + b.b * (1. - t),
    )
}
pub fn luminance(c: Color) -> f32 {
    tone::luminance(c)
}
pub fn contrast(a: Color, b: Color) -> f32 {
    tone::contrast(a, b)
}
pub fn ink(c: Color, bg: Color) -> Color {
    let target = if luminance(bg) > 0.179 {
        Color::BLACK
    } else {
        Color::WHITE
    };
    (0..=100)
        .map(|n| mix(target, c, n as f32 / 100.))
        .find(|c| contrast(*c, bg) >= 4.6)
        .unwrap_or(target)
}
/// The seed's hue at `l`, with at most `most` chroma: tinted text and ink.
fn hued(seed: Lch, l: f32, most: f32) -> [u8; 3] {
    tone::rgb(Lch {
        l,
        c: (seed.c * 0.25).min(most),
        h: seed.h,
    })
}
fn readable(rgb: [u8; 3], bg: Color) -> Color {
    tone::color(tone::readable(rgb, bg, 4.6))
}
pub fn resolve(a: Appearance) -> Roles {
    let family = a.family.expect("shared theme selected");
    let spec = family.spec();
    let seed = a.accent.unwrap_or_else(|| family.accent());
    let tint = tone::lch(seed);
    // Light styles draw dark ink on a light sheet.
    let paper = spec.light;
    let surface = tone::color(family.surface(seed));
    let foreground = tone::color(if paper {
        hued(tint, 0.30, 0.03)
    } else {
        hued(tint, 0.955, 0.012)
    });
    let muted = readable(
        if paper {
            hued(tint, 0.47, 0.03)
        } else {
            hued(tint, 0.80, 0.03)
        },
        surface,
    );
    let transparent = a.surface.unwrap_or(if spec.transparent {
        Surface::Transparent
    } else {
        Surface::Panel
    }) == Surface::Transparent
        && !a.contrast_assist;
    let mut card = Palette {
        background: Color {
            a: a.opacity as f32 / 100.,
            ..surface
        },
        foreground,
        muted,
        accent: readable(seed, surface),
    };
    if a.contrast_assist {
        card = crate::widget_style::readable(card);
    }
    // Drawer and layout controls stay opaque even over see-through cards.
    let mut filled = card;
    filled.background.a = filled.background.a.max(0.92);
    let wall_bg = if a.wallpaper_light {
        rgb([238, 236, 226])
    } else {
        rgb([24, 35, 34])
    };
    let wallpaper = Palette {
        background: Color::TRANSPARENT,
        foreground: tone::color(if a.wallpaper_light {
            hued(tint, 0.31, 0.025)
        } else {
            hued(tint, 0.975, 0.008)
        }),
        muted: tone::color(if a.wallpaper_light {
            hued(tint, 0.42, 0.03)
        } else {
            hued(tint, 0.885, 0.02)
        }),
        accent: readable(seed, wall_bg),
    };
    if transparent {
        card = wallpaper;
    }
    let body = spec.body.font(font_installed);
    let clock = spec.clock.font(font_installed);
    let bg = if transparent { wall_bg } else { surface };
    Roles {
        card,
        surface: filled,
        wallpaper,
        body,
        clock,
        transparent,
        warning: readable([224, 156, 45], bg),
        error: readable([215, 68, 70], bg),
        border_width: spec.border_width,
        shadow: if transparent {
            Shadow::None
        } else {
            spec.shadow
        },
        frosted: spec.frosted && !transparent,
    }
}
impl Config {
    pub fn theme_roles(&self) -> Option<Roles> {
        let appearance = self.moment_appearance();
        appearance.family?;
        let mut cache = self.theme_cache.borrow_mut();
        if cache
            .as_ref()
            .is_none_or(|cached| cached.appearance != appearance)
        {
            *cache = Some(Cache {
                appearance,
                roles: resolve(appearance),
            });
        }
        cache.as_ref().map(|c| {
            let mut roles = c.roles;
            roles.card = crate::materials::palette(
                roles.card,
                self.materials.widgets(),
                self.appearance.contrast_assist,
            );
            if self.materials.widgets().is_some() {
                roles.frosted = !roles.transparent;
            }
            roles
        })
    }
    pub fn wallpaper_palette(&self) -> Palette {
        self.theme_roles()
            .map_or_else(|| self.palette().on_wallpaper(), |r| r.wallpaper)
    }
    pub fn surface_palette(&self) -> Palette {
        self.theme_roles().map_or_else(
            || {
                let mut p = self.palette();
                if self.materials.widgets().is_some() {
                    p.background.a = p.background.a.max(0.92);
                }
                p
            },
            |r| r.surface,
        )
    }
    /// Whether At a glance draws a panel: its own choice without a widget
    /// style, or when it keeps its own background; otherwise the style's.
    pub fn glance_panel(&self) -> bool {
        let own = self.glance.style == crate::config::GlanceStyle::Panel;
        match self.theme_roles() {
            Some(roles) if !self.glance.own_background => !roles.transparent,
            _ => own,
        }
    }
    pub fn transparent_widgets(&self) -> bool {
        self.theme_roles().is_some_and(|r| r.transparent)
    }
    pub fn widget_custom(&self) -> bool {
        self.appearance.family.is_some_and(|f| {
            let (opacity, radius, border) = f.geometry();
            self.appearance.overrides & 0x1f != 0
                || self.appearance.surface.is_some()
                || self.appearance.contrast_assist
                || (
                    self.appearance.opacity,
                    self.appearance.corner_radius,
                    self.appearance.border_opacity,
                ) != (opacity, radius, border)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_style_has_a_distinct_complete_spec() {
        let mut ids = Vec::new();
        for family in Family::ALL {
            let spec = family.spec();
            assert!(!ids.contains(&spec.id), "{} repeats", spec.id);
            ids.push(spec.id);
            assert_eq!(Family::parse(spec.id), Some(family));
            assert_eq!(Family::parse(family.preset()), Some(family));
            assert!(!spec.title.is_empty() && !spec.description.is_empty());
            assert!((0.5..=3.).contains(&spec.border_width), "{}", spec.id);
            let (opacity, radius, border) = spec.geometry;
            assert!((70..=100).contains(&opacity) && radius <= 32 && border <= 50);
            assert!((0.0..=1.0).contains(&spec.bar.opacity));
            // Serialized ids stay stable: they are stored in preferences.
            assert_eq!(
                serde_json::to_value(family).unwrap(),
                serde_json::json!(spec.id)
            );
            // A frosted style retains its filled decorative card.
            assert!(!(spec.frosted && spec.transparent), "{}", spec.id);
        }
    }

    #[test]
    fn style_fonts_use_the_first_installed_name_or_the_generic_family() {
        let spec = FontSpec {
            names: &["Missing Font", "URW Gothic", "Fira Sans"],
            generic: Generic::Serif,
            weight: Weight::Bold,
        };
        let font = spec.font(|name| name == "URW Gothic" || name == "Fira Sans");
        assert_eq!(font.family, FontFamily::Name("URW Gothic"));
        assert_eq!(font.weight, Weight::Bold);
        let fallback = spec.font(|_| false);
        assert_eq!(fallback.family, FontFamily::Serif);
        assert_eq!(fallback.weight, Weight::Bold);
    }

    #[test]
    fn shared_themes_keep_legacy_and_cache_readable_roles() {
        let old: Config = serde_json::from_str("{}").unwrap();
        assert!(old.theme_roles().is_none());
        for family in Family::ALL {
            for light in [false, true] {
                let mut c = old.clone();
                c.appearance.family = Some(family);
                c.appearance.wallpaper_light = light;
                let (o, r, b) = family.geometry();
                c.appearance.opacity = o;
                c.appearance.corner_radius = r;
                c.appearance.border_opacity = b;
                let roles = c.theme_roles().unwrap();
                assert_eq!(c.theme_roles(), Some(roles));
                let bg = if roles.transparent {
                    if light {
                        rgb([238, 236, 226])
                    } else {
                        rgb([24, 35, 34])
                    }
                } else {
                    roles.card.background
                };
                for color in [
                    roles.card.foreground,
                    roles.card.muted,
                    roles.card.accent,
                    roles.warning,
                    roles.error,
                ] {
                    assert!(contrast(color, bg) >= 4.5, "{family:?} {light}: {color:?}");
                }
                assert_eq!(roles.transparent, family.spec().transparent);
                let controls = c.surface_palette();
                assert!(controls.background.a > 0.9);
                assert!(contrast(controls.foreground, controls.background) >= 4.5);
                if roles.transparent {
                    let checkmark = crate::widget_style::on_accent(roles.card);
                    assert_eq!(checkmark.a, 1.);
                    assert!(contrast(checkmark, roles.card.accent) >= 4.5);
                }
                assert!(!c.widget_custom());
                c.appearance.corner_radius = 1;
                assert!(c.widget_custom());
                assert!(
                    serde_json::to_value(c)
                        .unwrap()
                        .get("theme_cache")
                        .is_none()
                );
            }
        }
    }

    #[test]
    fn wallpaper_seeds_give_lively_readable_roles_with_distinct_families() {
        let seeds = [
            [217, 165, 121], // amber
            [108, 192, 208], // teal
            [221, 155, 180], // rose
            [228, 164, 6],   // vivid yellow
            [165, 184, 218], // steel
            [64, 53, 44],    // the reported dark swatch
            [250, 250, 250], // near white
            [94, 129, 172],  // custom blue
        ];
        for seed in seeds {
            let tint = tone::lch(seed);
            for family in Family::ALL {
                for light in [false, true] {
                    let (opacity, corner_radius, border_opacity) = family.geometry();
                    let roles = resolve(Appearance {
                        family: Some(family),
                        accent: Some(seed),
                        wallpaper_light: light,
                        opacity,
                        corner_radius,
                        border_opacity,
                        ..Appearance::default()
                    });
                    let surface = tone::color(family.surface(seed));
                    let bg = if roles.transparent {
                        if light {
                            rgb([238, 236, 226])
                        } else {
                            rgb([24, 35, 34])
                        }
                    } else {
                        surface
                    };
                    for ink in [
                        roles.card.foreground,
                        roles.card.muted,
                        roles.card.accent,
                        roles.warning,
                        roles.error,
                    ] {
                        assert!(
                            contrast(ink, bg) >= 4.5,
                            "{family:?} {seed:?} {light}: {ink:?}"
                        );
                    }
                    // Readable ink keeps the color rather than graying it.
                    if tint.c >= 0.05 {
                        let accent = tone::lch(bytes(roles.card.accent));
                        assert!(accent.c >= tint.c * 0.55, "{family:?} {seed:?}: {accent:?}");
                    }
                    let lch = tone::lch(family.surface(seed));
                    match family {
                        Family::Paper => {
                            assert!(lch.l > 0.95);
                            let b = lch.c * lch.h.sin();
                            assert!(b > -0.001, "Paper never turns cool: {seed:?}");
                        }
                        Family::Outline => assert!(lch.l < 0.23 && lch.c <= 0.021),
                        Family::Soft => assert!((0.29..0.32).contains(&lch.l) && lch.c <= 0.035),
                        Family::Bare => assert!(lch.l < 0.3),
                        // Every other style: its spec's lightness, light
                        // styles light and dark styles dark.
                        _ => {
                            let (l, ..) = family.spec().surface;
                            assert!((lch.l - l).abs() < 0.02, "{family:?} {lch:?}");
                            assert_eq!(lch.l > 0.5, family.light(), "{family:?}");
                        }
                    }
                    // A colorful seed tints Soft's surface toward its own hue.
                    if family == Family::Soft && tint.c >= 0.08 {
                        let turn = (lch.h - tint.h).rem_euclid(std::f32::consts::TAU);
                        assert!(turn.min(std::f32::consts::TAU - turn) < 0.1);
                        assert!(lch.c >= 0.02, "visibly tinted: {seed:?}");
                    }
                }
            }
            // Native accents: a pastel tone for Dark, a deep one for Light.
            let dark = tone::lch(native_accent(seed, true));
            let light = tone::lch(native_accent(seed, false));
            assert!(dark.l >= 0.71 && light.l <= 0.54, "{seed:?}");
            assert!(
                contrast(tone::color(native_accent(seed, false)), Color::WHITE) >= 4.5,
                "{seed:?}"
            );
        }
    }
}
