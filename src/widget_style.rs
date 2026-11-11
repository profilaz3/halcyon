//! Shared card geometry and readable ink. Contrast assistance is opt-in and
//! uses a worst-case wallpaper range, so it needs no sampling or background work.
use crate::config::Palette;
use cosmic::iced::{Color, Font};
pub mod resolver;

pub const PADDING: u16 = 18;
pub const TITLE: f32 = 18.;
pub const SECONDARY: f32 = 12.;
pub const META: f32 = 10.;
/// Font families chosen in Settings › Widget style, over the theme's.
#[derive(Clone, Copy, Default)]
struct Families {
    body: Option<&'static str>,
    clock: Option<&'static str>,
    weight: Option<cosmic::iced::font::Weight>,
    italic: Option<bool>,
}
thread_local! {
    static ASSIST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static MOTION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static MATERIAL: std::cell::Cell<Option<crate::materials::Finish>> = const { std::cell::Cell::new(None) };
    static CONTEXT: std::cell::Cell<Option<crate::desktop_theme::Roles>> = const { std::cell::Cell::new(None) };
    static FAMILIES: std::cell::Cell<Families> = const { std::cell::Cell::new(Families { body: None, clock: None, weight: None, italic: None }) };
}
/// A lexical rendering/measurement context. The guard restores the prior
/// context, so a widget preview cannot recolor or change Settings typography.
pub struct Scope(
    Option<crate::desktop_theme::Roles>,
    Families,
    Option<crate::materials::Finish>,
    bool,
    bool,
);
/// An immutable rendering context captured with a component's inputs. Restoring
/// it while constructing the native tree cannot pick up another preview's style.
#[derive(Clone, Copy)]
pub struct Context {
    roles: Option<crate::desktop_theme::Roles>,
    families: Families,
    material: Option<crate::materials::Finish>,
    motion: bool,
    assist: bool,
}
impl Context {
    pub fn capture() -> Self {
        Self {
            roles: CONTEXT.get(),
            families: FAMILIES.get(),
            material: MATERIAL.get(),
            motion: MOTION.get(),
            assist: ASSIST.get(),
        }
    }
    pub fn enter(self) -> Scope {
        Scope(
            CONTEXT.replace(self.roles),
            FAMILIES.replace(self.families),
            MATERIAL.replace(self.material),
            MOTION.replace(self.motion),
            ASSIST.replace(self.assist),
        )
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        CONTEXT.set(self.0);
        FAMILIES.set(self.1);
        MATERIAL.set(self.2);
        MOTION.set(self.3);
        ASSIST.set(self.4);
    }
}
pub fn scope(config: &crate::config::Config) -> Scope {
    let clock = config.moment_clock();
    let families = Families {
        body: config.widget_font.as_deref().map(intern),
        clock: config.clock_font.as_deref().map(intern),
        weight: clock.weight.map(resolver::ClockWeight::native),
        italic: clock.italic,
    };
    Scope(
        CONTEXT.replace(config.theme_roles()),
        FAMILIES.replace(families),
        MATERIAL.replace(config.materials.widgets()),
        MOTION.replace(config.material_motion),
        ASSIST.replace(config.appearance.contrast_assist),
    )
}
/// One widget's own families over the widget fonts; None keeps the current.
pub struct Fonts(Families);
impl Drop for Fonts {
    fn drop(&mut self) {
        FAMILIES.set(self.0);
    }
}
pub fn fonts(body: Option<&str>, clock: Option<&str>) -> Fonts {
    let current = FAMILIES.get();
    Fonts(
        FAMILIES.replace(Families {
            body: body.map(intern).or(current.body),
            clock: clock
                .map(intern)
                .or(if body.is_some() { None } else { current.clock }),
            weight: current.weight,
            italic: current.italic,
        }),
    )
}
/// The style's border width and the shadow around a card drawn with
/// palette `p` (no shadow without a background).
pub fn edge(p: Palette) -> (f32, cosmic::iced::Shadow) {
    use crate::desktop_theme::Shadow;
    if MATERIAL.get().is_some() && p.background.a > 0. {
        return (
            1.,
            cosmic::iced::Shadow {
                color: Color::from_rgba(0., 0., 0., 0.22),
                offset: cosmic::iced::Vector::new(0., 3.),
                blur_radius: 9.,
            },
        );
    }
    let (width, kind) = CONTEXT
        .get()
        .map_or((1., Shadow::None), |r| (r.border_width, r.shadow));
    let kind = if p.background.a == 0. {
        Shadow::None
    } else {
        kind
    };
    let shadow = match kind {
        Shadow::None => cosmic::iced::Shadow::default(),
        Shadow::Soft => cosmic::iced::Shadow {
            color: Color::from_rgba(0., 0., 0., 0.38),
            offset: cosmic::iced::Vector::new(0., 4.),
            blur_radius: 10.,
        },
        Shadow::Hard => cosmic::iced::Shadow {
            color: Color {
                a: 1.,
                ..p.foreground
            },
            offset: cosmic::iced::Vector::new(3., 3.),
            blur_radius: 0.,
        },
        Shadow::Glow => cosmic::iced::Shadow {
            color: Color { a: 0.6, ..p.accent },
            offset: cosmic::iced::Vector::new(0., 0.),
            blur_radius: 10.,
        },
    };
    (width, shadow)
}
/// A rounded rectangle as plain rectangles, for a compositor blur region:
/// the same coalesced logical-pixel mask used by native Islands.
pub fn rounded_rects(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
) -> Vec<cosmic::iced::Rectangle> {
    dock_islands_core::rounded_region(
        x.ceil() as i32,
        y.ceil() as i32,
        width.floor() as i32,
        height.floor() as i32,
        radius as f64,
    )
    .into_iter()
    .map(|[x, y, w, h]| cosmic::iced::Rectangle {
        x: x as f32,
        y: y as f32,
        width: w as f32,
        height: h as f32,
    })
    .collect()
}
/// Room every widget surface keeps around its card for the style's shadow.
pub fn shadow_margin(config: &crate::config::Config) -> f32 {
    if config.materials.widgets().is_some() {
        12.
    } else {
        config.theme_roles().map_or(0., |r| r.shadow.margin())
    }
}
/// Font names live for the program: a few chosen families, kept once each.
pub(crate) fn intern(name: &str) -> &'static str {
    static NAMES: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut names = NAMES.lock().unwrap();
    if let Some(known) = names.iter().find(|known| **known == name) {
        return known;
    }
    let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
    names.push(leaked);
    leaked
}
fn with_family(font: Font, family: Option<&'static str>) -> Font {
    family.map_or(font, |name| Font {
        family: cosmic::iced::font::Family::Name(name),
        ..font
    })
}
fn default_font(weight: cosmic::iced::font::Weight) -> Font {
    // A missing named font can resolve digits to a color emoji face. Choose
    // an installed text family, retaining Fira on desktops that have it.
    crate::desktop_theme::load_installed_fonts();
    crate::desktop_theme::FontSpec {
        names: &["Fira Sans", "Open Sans", "DejaVu Sans"],
        generic: crate::desktop_theme::Generic::Sans,
        weight,
    }
    .font(crate::desktop_theme::font_installed)
}
pub fn font() -> Font {
    let theme = CONTEXT.get().map_or_else(
        || default_font(cosmic::iced::font::Weight::Normal),
        |r| r.body,
    );
    with_family(theme, FAMILIES.get().body)
}
pub fn clock_font() -> Font {
    let theme = CONTEXT.get().map_or_else(
        || default_font(cosmic::iced::font::Weight::Light),
        |r| r.clock,
    );
    // The clock follows the widget font unless it has its own.
    let families = FAMILIES.get();
    let mut font = with_family(theme, families.clock.or(families.body));
    if let Some(weight) = families.weight {
        font.weight = weight;
    }
    if let Some(italic) = families.italic {
        font.style = if italic {
            cosmic::iced::font::Style::Italic
        } else {
            cosmic::iced::font::Style::Normal
        };
    }
    font
}

/// Project current preferences without introducing another applied document.
/// Call inside `scope(config)` to retain grouped-widget font overrides.
pub fn presentation(
    config: &crate::config::Config,
    palette: Palette,
    clock: bool,
) -> resolver::Style {
    use resolver::{ClockFace, ClockSurface, Constraints, Layers, Patch, Surface, Values};
    let effective = config.moment_clock();
    let palette = if clock && effective.surface == Some(ClockSurface::Panel) {
        config.palette()
    } else {
        palette
    };
    let surface = if clock || palette.background.a == 0. {
        Surface::None
    } else {
        Surface::Solid(palette.background)
    };
    let base = Values {
        ink: palette.foreground,
        muted: palette.muted,
        accent: palette.accent,
        surface,
        body: font(),
        display: clock_font(),
        radius: config.appearance.corner_radius,
        padding: if clock { 4 } else { PADDING },
        gap: 8,
        motion: config.material_motion,
        clock_face: ClockFace::Digital,
    };
    let mut layers = Layers::default();
    if config.appearance.family.is_some() {
        layers.setup = Patch {
            ink: Some(base.ink),
            muted: Some(base.muted),
            accent: Some(base.accent),
            body: Some(base.body),
            display: Some(base.display),
            ..Default::default()
        };
    }
    if config.widget_font.is_some() {
        layers.setup.body = Some(base.body);
        layers.setup.display = Some(base.display);
    }
    if config
        .moments
        .selected()
        .is_some_and(|look| look.family.is_some())
    {
        layers.moment = Patch {
            ink: Some(base.ink),
            muted: Some(base.muted),
            accent: Some(base.accent),
            surface: Some(base.surface),
            body: Some(base.body),
            display: Some(base.display),
            ..Default::default()
        };
    }
    if clock {
        layers.instance.clock_face = config.clock.face;
        if config.clock.weight.is_some()
            || config.clock.italic.is_some()
            || config.clock_font.is_some()
        {
            layers.instance.display = Some(base.display);
        }
        if let Some(surface) = config.clock.surface {
            layers.instance.surface = Some(match surface {
                ClockSurface::None => Surface::None,
                ClockSurface::Panel => Surface::Solid(config.palette().background),
            });
            if surface == ClockSurface::Panel {
                layers.instance.padding = Some(16);
            }
        }
    }
    if clock && let Some(look) = config.moments.selected() {
        layers.moment_instance.clock_face = look.clock.face;
        if look.clock.weight.is_some() || look.clock.italic.is_some() {
            layers.moment_instance.display = Some(base.display);
        }
        if let Some(surface) = look.clock.surface {
            layers.moment_instance.surface = Some(match surface {
                ClockSurface::None => Surface::None,
                ClockSurface::Panel => Surface::Solid(config.palette().background),
            });
            layers.moment_instance.padding = Some(if surface == ClockSurface::Panel {
                16
            } else {
                4
            });
        }
    }
    let constraints = Constraints {
        reduce_motion: !config.material_motion,
        readable: (clock && effective.surface.is_some() && config.appearance.contrast_assist)
            .then(|| readable(config.palette())),
    };
    resolver::resolve(
        base,
        layers,
        constraints,
        crate::desktop_theme::font_installed,
    )
}
pub fn semantic(color: Color) -> Color {
    CONTEXT.get().map_or(color, |r| {
        // Existing event notices, stale state and errors retain their semantic
        // hue, with ink appropriate to Paper and transparent widgets.
        if color.r > 0.75 && color.g > 0.5 && color.g < 0.82 && color.b < 0.55 {
            r.warning
        } else if color.r > 0.7 && color.g < 0.5 && color.b < 0.6 {
            r.error
        } else {
            color
        }
    })
}

/// A highlight over a transparent card is a translucent ink layer. Mixing
/// against transparent black as if it were an opaque surface hides dark text.
pub fn tint(ink: Color, surface: Color, amount: f32) -> Color {
    if surface.a == 0. {
        Color {
            a: ink.a * amount,
            ..ink
        }
    } else {
        blend(ink, surface, amount)
    }
}

pub fn on_accent(palette: Palette) -> Color {
    if palette.background.a == 0. {
        crate::desktop_theme::ink(palette.foreground, palette.accent)
    } else {
        palette.background
    }
}

pub(crate) fn luminance(c: Color) -> f32 {
    let linear = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
}
fn blend(front: Color, back: Color, alpha: f32) -> Color {
    Color::from_rgb(
        front.r * alpha + back.r * (1. - alpha),
        front.g * alpha + back.g * (1. - alpha),
        front.b * alpha + back.b * (1. - alpha),
    )
}
fn ratio(a: Color, b: Color) -> f32 {
    let a = luminance(a);
    let b = luminance(b);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
fn readable_ink(ink: Color, background: Color) -> Color {
    let target = if luminance(background) < 0.179 {
        Color::WHITE
    } else {
        Color::BLACK
    };
    (0..=100)
        .map(|n| blend(target, ink, n as f32 / 100.))
        .find(|candidate| ratio(*candidate, background) >= 4.55)
        .unwrap_or(target)
}
fn worst_ratio(ink: Color, background: Color, alpha: f32) -> f32 {
    let dark = blend(background, Color::BLACK, alpha);
    let light = blend(background, Color::WHITE, alpha);
    let l = luminance(ink);
    if (luminance(dark)..=luminance(light)).contains(&l) {
        1.
    } else {
        ratio(ink, dark).min(ratio(ink, light))
    }
}
/// A see-through card stays readable over any picture: it gets just
/// enough fill that its text and accent, in their own colours, pass 4.5:1
/// whether the wallpaper behind is white or black, keeping as much of the
/// see-through look as that allows. Secondary text leans toward black or
/// white at that fill until it passes too. Opaque and transparent cards are
/// left alone. (Readability help, `readable`, also keeps secondary text's
/// own colour.)
pub fn legible(mut p: Palette) -> Palette {
    let fill = p.background.a;
    if fill <= 0. || fill >= 1. {
        return p;
    }
    let start = (fill * 100. - 1e-3).ceil().max(0.) as u32;
    let fill = (start..=100)
        .map(|n| n as f32 / 100.)
        .find(|&alpha| {
            [p.foreground, p.accent]
                .into_iter()
                .all(|ink| worst_ratio(ink, p.background, alpha) >= 4.5)
        })
        .unwrap_or(1.)
        .max(p.background.a);
    p.background.a = fill;
    let target = if luminance(p.foreground) > luminance(p.background) {
        Color::WHITE
    } else {
        Color::BLACK
    };
    p.muted = (0..=100)
        .map(|n| blend(target, p.muted, n as f32 / 100.))
        .find(|candidate| worst_ratio(*candidate, p.background, fill) >= 4.5)
        .unwrap_or(target);
    p
}
pub fn readable(mut p: Palette) -> Palette {
    p.foreground = readable_ink(p.foreground, p.background);
    p.muted = readable_ink(p.muted, p.background);
    p.accent = readable_ink(p.accent, p.background);
    let start = (p.background.a.clamp(0., 1.) * 100.).ceil() as u32;
    p.background.a = (start..=100)
        .map(|n| n as f32 / 100.)
        .find(|&alpha| {
            [p.foreground, p.muted, p.accent]
                .into_iter()
                .all(|ink| worst_ratio(ink, p.background, alpha) >= 4.5)
        })
        .unwrap_or(1.);
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn assisted_text_is_readable_over_bright_dark_and_colored_wallpapers() {
        for name in Palette::CHOICES.map(|(name, _)| name) {
            let mut p = Palette::named(name);
            p.background.a = 0.7;
            let p = readable(p);
            for wallpaper in [
                Color::BLACK,
                Color::WHITE,
                Color::from_rgb(1., 0., 0.),
                Color::from_rgb(0., 1., 0.),
                Color::from_rgb(0., 0., 1.),
            ] {
                let background = blend(p.background, wallpaper, p.background.a);
                for ink in [p.foreground, p.muted, p.accent] {
                    assert!(ratio(ink, background) >= 4.5, "{name}");
                }
            }
        }
        let light = Palette {
            background: Color::WHITE,
            foreground: Color::from_rgb(0.1, 0.1, 0.1),
            muted: Color::from_rgb(0.6, 0.6, 0.6),
            accent: Color::from_rgb(0.3, 0.8, 0.8),
        };
        let p = readable(light);
        for ink in [p.foreground, p.muted, p.accent] {
            assert!(ratio(ink, p.background) >= 4.5)
        }
    }
    #[test]
    fn decorative_material_scrims_keep_light_and_dark_text_readable() {
        for family in crate::desktop_theme::Family::ALL {
            for kind in crate::materials::Kind::ALL {
                let mut config = crate::config::Config::default();
                config.appearance.family = Some(family);
                config.materials.shared = Some(crate::materials::Finish::new(kind));
                let saved = config.materials;
                let unassisted = config.palette();
                config.appearance.contrast_assist = true;
                let p = config.palette();
                assert_eq!(config.materials, saved);
                if p.background.a == 0. {
                    assert_eq!(unassisted.background.a, 0.);
                    continue;
                }
                for wallpaper in [Color::BLACK, Color::WHITE] {
                    let bg = blend(p.background, wallpaper, p.background.a);
                    for ink in [p.foreground, p.muted, p.accent] {
                        assert!(ratio(ink, bg) >= 4.5, "{family:?}/{kind:?}");
                    }
                }
                let _scope = scope(&config);
                assert!(matches!(
                    background(p.background),
                    cosmic::iced::Background::Color(_)
                ));
            }
        }
    }

    #[test]
    fn assistance_is_optional_and_does_not_replace_the_saved_opacity() {
        let mut config = crate::config::Config::default();
        config.appearance.opacity = 70;
        let original = config.palette();
        assert!(!config.appearance.contrast_assist);
        config.appearance.contrast_assist = true;
        assert!(config.palette().background.a >= original.background.a);
        assert_eq!(config.appearance.opacity, 70);
        config.appearance.contrast_assist = false;
        assert_eq!(config.palette(), original);
    }
}

#[cfg(test)]
mod edge_tests {
    use super::*;
    #[test]
    fn blur_regions_stay_inside_the_rounded_card() {
        let (x, y, w, h, r) = (12., 12., 300., 200., 22.);
        let rects = rounded_rects(x, y, w, h, r);
        for rect in &rects {
            assert!(rect.x >= x && rect.y >= y, "{rect:?}");
            assert!(rect.x + rect.width <= x + w + 0.01 && rect.y + rect.height <= y + h + 0.01);
            // Every corner of every rectangle is inside the rounded shape.
            for (px, py) in [
                (rect.x + 0.5, rect.y + 0.5),
                (rect.x + rect.width - 0.5, rect.y + 0.5),
                (rect.x + 0.5, rect.y + rect.height - 0.5),
                (rect.x + rect.width - 0.5, rect.y + rect.height - 0.5),
            ] {
                let cx = px.clamp(x + r, x + w - r);
                let cy = py.clamp(y + r, y + h - r);
                assert!(
                    (px - cx).hypot(py - cy) <= r + 0.01,
                    "({px},{py}) outside the curve"
                );
            }
        }
        // The middle band covers the full width.
        assert!(
            rects
                .iter()
                .any(|rect| rect.width == w && rect.height == h - 2. * r)
        );
        // Square cards are one rectangle.
        assert_eq!(rounded_rects(0., 0., 100., 50., 0.).len(), 1);
    }
}

#[cfg(test)]
mod font_tests {
    use super::*;
    use cosmic::iced::font::Family;

    #[test]
    fn chosen_fonts_override_the_style_and_the_clock_follows_the_text_font() {
        let mut config = crate::config::Config::default();
        let theme = {
            let _scope = scope(&config);
            (font(), clock_font())
        };
        config.widget_font = Some("Inter".into());
        {
            let _scope = scope(&config);
            assert_eq!(font().family, Family::Name("Inter"));
            assert_eq!(
                clock_font().family,
                Family::Name("Inter"),
                "clock follows text"
            );
            assert_eq!(
                clock_font().weight,
                theme.1.weight,
                "the style's weight stays"
            );
        }
        config.clock_font = Some("Noto Serif".into());
        {
            let _scope = scope(&config);
            assert_eq!(font().family, Family::Name("Inter"));
            assert_eq!(clock_font().family, Family::Name("Noto Serif"));
        }
        // Leaving the scope restores whatever was there before.
        assert_eq!(font(), theme.0);
        config.widget_font = None;
        config.clock_font = None;
        let _scope = scope(&config);
        assert_eq!(font(), theme.0);
        assert_eq!(clock_font(), theme.1);
    }
}

/// Capture while the widget's lexical style scope is active.
pub fn background(color: Color) -> cosmic::iced::Background {
    crate::materials::background(color, if ASSIST.get() { None } else { MATERIAL.get() })
}
pub fn border(color: Color) -> Color {
    MATERIAL.get().map_or(color, |f| {
        Color::from_rgba(
            1.,
            1.,
            1.,
            color.a
                * match f.kind {
                    crate::materials::Kind::Liquid => 1.6,
                    crate::materials::Kind::Clear => 1.2,
                    _ => 0.6,
                },
        )
    })
}

pub fn surface<'a, M: 'a>(content: cosmic::Element<'a, M>, p: Palette) -> cosmic::Element<'a, M> {
    crate::materials::sheen(
        content,
        p.background.a > 0.
            && MOTION.get()
            && MATERIAL
                .get()
                .is_some_and(|f| f.kind == crate::materials::Kind::Liquid),
    )
}

pub fn radius(value: u16) -> f32 {
    let radius = f32::from(value.min(32));
    if MATERIAL
        .get()
        .is_some_and(|f| f.kind == crate::materials::Kind::Liquid)
    {
        radius.max(24.)
    } else {
        radius
    }
}
pub fn corner_radius(config: &crate::config::Config) -> f32 {
    let _scope = scope(config);
    radius(config.appearance.corner_radius)
}
