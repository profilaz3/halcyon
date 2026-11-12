//! The island's one palette. Every surface, text and line colour the island
//! draws comes from `Palette::new`; the activity accents live in `accent` and
//! are made readable on whichever surface the chosen style gives.
use super::preferences::IslandStyle;
use cosmic::iced::Color;

/// Activity accents. Colour comes from the content, never the chrome, so each
/// is a hue first; `Palette::readable` keeps it legible on either surface.
pub(super) mod accent {
    use cosmic::iced::Color;
    pub const IDLE: Color = Color::from_rgb(0.706, 0.745, 0.733);
    pub const SYSTEM: Color = Color::from_rgb(0.588, 0.776, 0.878);
    /// Music before its artwork gives a colour.
    pub const MUSIC: Color = Color::from_rgb(0.827, 0.424, 0.941);
    pub const FOCUS: Color = Color::from_rgb(0.745, 0.647, 0.922);
    /// A calendar event whose calendar has no colour of its own.
    pub const CALENDAR: Color = Color::from_rgb(0.353, 0.784, 0.980);
    pub const NOTIFICATION: Color = Color::from_rgb(0.925, 0.627, 0.690);
    pub const TIMER: Color = Color::from_rgb(1.0, 0.624, 0.039);
    pub const JOB: Color = Color::from_rgb(0.243, 0.651, 1.0);
    /// Finished, connected, a camera in use.
    pub const DONE: Color = Color::from_rgb(0.196, 0.843, 0.294);
    /// Urgent notifications and failures.
    pub const URGENT: Color = Color::from_rgb(1.0, 0.271, 0.227);
    pub const CAMERA: Color = DONE;
    pub const MICROPHONE: Color = TIMER;
}

/// The near-black island of the Dark style, as in the mockup.
const DARK_SURFACE: Color = Color::from_rgb(0.039, 0.039, 0.047);
const DARK_RAISED: Color = Color::from_rgb(0.106, 0.106, 0.125);
const DARK_TRACK: Color = Color::from_rgb(0.149, 0.149, 0.173);
const DARK_INK: Color = Color::from_rgb(0.961, 0.961, 0.969);
const DARK_QUIET: Color = Color::from_rgb(0.588, 0.588, 0.612);
const DARK_FAINT: Color = Color::from_rgb(0.357, 0.357, 0.373);

/// Under "Follow the theme" the host owns the resting background, including
/// Islands finishes, and the open surface uses the panel's native palette. The
/// Dark style paints its own near-black pill at rest and open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Palette {
    /// The open island, always opaque for reading.
    pub surface: Color,
    /// Tiles, chips and quiet discs on the surface.
    pub raised: Color,
    /// The unlit part of a ring or scrubber.
    pub track: Color,
    /// Titles and glyphs.
    pub ink: Color,
    /// Secondary lines; still readable text.
    pub quiet: Color,
    /// Decoration only, never the sole carrier of meaning.
    pub faint: Color,
    /// Hairlines and the island's edge.
    pub line: Color,
    /// The island paints its own pill at rest instead of leaving it to the host.
    pub own_pill: bool,
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgb(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
    )
}

impl Palette {
    pub fn new(context: &cosmic::applet::Context, style: IslandStyle) -> Self {
        match style {
            IslandStyle::Dark => Self::dark(),
            IslandStyle::Theme => Self::for_panel(context),
        }
    }
    pub fn dark() -> Self {
        Self {
            surface: DARK_SURFACE,
            raised: DARK_RAISED,
            track: DARK_TRACK,
            ink: DARK_INK,
            quiet: DARK_QUIET,
            faint: DARK_FAINT,
            line: Color {
                a: 0.09,
                ..Color::WHITE
            },
            own_pill: true,
        }
    }
    fn for_panel(context: &cosmic::applet::Context) -> Self {
        use cosmic::applet::cosmic_panel_config::CosmicPanelBackground;
        let theme = context.theme().unwrap_or_else(cosmic::theme::active);
        let native = theme.cosmic();
        let surface = match context.background {
            CosmicPanelBackground::Color(rgb) => Color::from_rgb(rgb[0], rgb[1], rgb[2]),
            _ => native.background(false).base.into(),
        };
        Self::on(surface, native.background(false).on.into())
    }
    /// The palette over a given opaque surface, with the theme's preferred ink
    /// when it reads and otherwise black or white.
    pub fn on(surface: Color, native_ink: Color) -> Self {
        let surface = Color { a: 1., ..surface };
        let ink = if contrast(native_ink, surface) >= 4.5 {
            native_ink
        } else if contrast(Color::WHITE, surface) > contrast(Color::BLACK, surface) {
            Color::WHITE
        } else {
            Color::BLACK
        };
        Self {
            surface,
            raised: mix(surface, ink, 0.08),
            track: mix(surface, ink, 0.16),
            ink,
            quiet: (72..=100)
                .map(|step| mix(surface, ink, step as f32 / 100.))
                .find(|quiet| contrast(*quiet, surface) >= 4.5)
                .unwrap_or(ink),
            faint: mix(surface, ink, 0.45),
            line: Color { a: 0.12, ..ink },
            own_pill: false,
        }
    }
    /// Strong but not stark: ink eased slightly toward the surface.
    pub fn soft_ink(self) -> Color {
        mix(self.ink, self.surface, 0.18)
    }
    /// A disc or chip behind an accent glyph: the accent, deep on the surface.
    pub fn deep(self, accent: Color) -> Color {
        mix(self.surface, accent, 0.22)
    }
    /// Urgent alerts use one red, kept readable on the surface.
    pub fn warning(self) -> Color {
        self.readable(accent::URGENT)
    }
    /// Keep an accent recognizable while its label stays readable on light,
    /// dark or explicitly coloured surfaces. Bounded, with no IO.
    pub fn readable(self, color: Color) -> Color {
        for step in 0..=20 {
            let candidate = mix(color, self.ink, step as f32 / 20.);
            if contrast(candidate, self.surface) >= 4.5 {
                return candidate;
            }
        }
        self.ink
    }
    /// A glyph or label on an accent fill: the surface or the ink, whichever reads.
    pub fn on_accent(self, fill: Color) -> Color {
        if contrast(self.surface, fill) >= contrast(self.ink, fill) {
            self.surface
        } else {
            self.ink
        }
    }
}

pub(super) fn contrast(a: Color, b: Color) -> f32 {
    let luminance = |c: Color| {
        let linear = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;
    const ACCENTS: [Color; 11] = [
        accent::IDLE,
        accent::SYSTEM,
        accent::MUSIC,
        accent::FOCUS,
        accent::CALENDAR,
        accent::NOTIFICATION,
        accent::TIMER,
        accent::JOB,
        accent::DONE,
        accent::URGENT,
        accent::MICROPHONE,
    ];
    fn surfaces() -> Vec<(&'static str, Palette)> {
        vec![
            ("dark style", Palette::dark()),
            (
                "theme light",
                Palette::on(
                    Color::from_rgb8(242, 242, 242),
                    Color::from_rgb8(30, 30, 30),
                ),
            ),
            (
                "theme dark",
                Palette::on(
                    Color::from_rgb8(27, 27, 27),
                    Color::from_rgb8(230, 230, 230),
                ),
            ),
            (
                "theme coloured",
                Palette::on(Color::from_rgb8(40, 90, 160), Color::WHITE),
            ),
        ]
    }
    #[test]
    fn accents_read_on_every_surface_and_stay_distinct() {
        for (name, p) in surfaces() {
            for a in ACCENTS {
                let r = p.readable(a);
                assert!(contrast(r, p.surface) >= 4.5, "{name}: {a:?} -> {r:?}");
            }
            assert!(contrast(p.ink, p.surface) >= 4.5, "{name} ink");
            assert!(contrast(p.quiet, p.surface) >= 4.5, "{name} quiet");
            assert!(contrast(p.warning(), p.surface) >= 4.5, "{name} warning");
            // Timer, job, done and urgent must not collapse into one colour.
            let key =
                [accent::TIMER, accent::JOB, accent::DONE, accent::URGENT].map(|a| p.readable(a));
            for i in 0..key.len() {
                for j in i + 1..key.len() {
                    let d = (key[i].r - key[j].r).abs()
                        + (key[i].g - key[j].g).abs()
                        + (key[i].b - key[j].b).abs();
                    assert!(d > 0.25, "{name}: accents {i} and {j} too close");
                }
            }
        }
    }
    #[test]
    fn the_dark_style_is_near_black_and_owns_its_pill() {
        let p = Palette::dark();
        assert!(p.own_pill);
        assert!(p.surface.r < 0.06 && p.surface.g < 0.06 && p.surface.b < 0.06);
        assert!(contrast(p.raised, p.surface) > 1.1);
        let light = Palette::on(Color::WHITE, Color::BLACK);
        assert!(!light.own_pill);
        assert_eq!(light.ink, Color::BLACK);
    }
    #[test]
    fn labels_on_accent_fills_read() {
        for (name, p) in surfaces() {
            for a in ACCENTS {
                let fill = p.readable(a);
                assert!(
                    contrast(p.on_accent(fill), fill) >= 3.,
                    "{name}: glyph on {fill:?}"
                );
            }
        }
    }
}
