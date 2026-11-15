//! Window tone: per style window backgrounds, built by COSMIC itself, stay
//! readable and keep visible container steps across the slider.
use super::*;
use crate::desktop_theme::{WindowTone, tone};
use cosmic::cosmic_theme::palette::Srgba;
use cosmic::iced::Color;
use serde_json::json;

fn srgba([r, g, b]: [u8; 3]) -> Srgba {
    Srgba::new(
        f32::from(r) / 255.,
        f32::from(g) / 255.,
        f32::from(b) / 255.,
        1.,
    )
}
fn iced(c: Srgba) -> Color {
    Color::from_rgba(c.red, c.green, c.blue, 1.)
}

/// What a COSMIC window draws for a family at a tone: (background, on),
/// (primary, on), (secondary, on), built from Halcyon's written roles.
pub(super) fn window(family: Family, variant: ThemeVariant, tone: i8) -> [(Color, Color); 3] {
    let look = toned(family, None, tone);
    let look = look.variant(variant).expect("both variants");
    let role = |c: &Option<ColorChoice>, r| c.as_ref().unwrap().color(r).unwrap().unwrap();
    let theme = variant
        .default_builder()
        .accent(role(&look.accent, ColorRole::Accent).color)
        .bg_color(role(&look.background, ColorRole::Background))
        .primary_container_bg(role(&look.primary, ColorRole::Primary))
        .build();
    [
        theme.background(false),
        theme.primary(false),
        theme.secondary(false),
    ]
    .map(|c| (iced(c.base), iced(c.on)))
}

fn l(c: Color) -> f32 {
    tone::lch(tone::bytes(c)).l
}

/// Every style, both variants, at its default and both slider ends (and
/// past them, which clamps): text on the background and both containers
/// stays WCAG AA, and both containers stay visible against the background.
#[test]
fn window_tones_keep_text_readable_and_containers_visible() {
    for family in Family::ALL {
        for variant in ThemeVariant::ALL {
            let dark = variant == ThemeVariant::Dark;
            let (low, high) = family.tone_range(dark);
            assert!(low <= 0 && high >= 0 && low >= -10 && high <= 10);
            let mut previous = None;
            for t in [-10, low, 0, high, 10] {
                let at = format!("{} {variant:?} tone {t}", family.id());
                let w = window(family, variant, t);
                for (surface, on) in w {
                    assert!(tone::contrast(surface, on) >= 4.5, "{at}: text");
                }
                assert!(
                    tone::contrast(w[0].0, w[1].0) >= desktop_theme::CONTAINER_CONTRAST - 0.002,
                    "{at}: primary {:.3}",
                    tone::contrast(w[0].0, w[1].0)
                );
                // COSMIC derives the secondary container from the background.
                // On Midnight's pure black it stays only faintly apart, as it
                // did before Window tone; every other window keeps it visible.
                let secondary = tone::contrast(w[0].0, w[2].0);
                if l(w[0].0) < 0.01 {
                    assert!(secondary > 1.01, "{at}: secondary {secondary:.3}");
                } else {
                    assert!(secondary >= 1.1, "{at}: secondary {secondary:.3}");
                }
                // Lighter tones never make the background darker.
                let bg = l(w[0].0);
                if let Some(before) = previous {
                    assert!(bg + 1e-3 >= before, "{at}: order");
                }
                previous = Some(bg);
            }
            // The ends really move the background.
            let at = |t| l(window(family, variant, t)[0].0);
            if low < 0 {
                assert!(
                    at(low) < at(0) - 0.008 * f32::from(-low),
                    "{} {low}: {} from {}",
                    family.id(),
                    at(low),
                    at(0)
                );
            }
            if high > 0 {
                assert!(
                    at(high) > at(0) + 0.008 * f32::from(high),
                    "{} {high}: {} from {}",
                    family.id(),
                    at(high),
                    at(0)
                );
            }
        }
    }
}

/// Background and primary move together: away from near black the step
/// between them is the same one fixed lightness step at every tone.
#[test]
fn window_tone_keeps_container_steps() {
    for family in Family::ALL {
        let seed = family.accent();
        for dark in [false, true] {
            let (low, high) = family.tone_range(dark);
            for t in low..=high {
                let bg = tone::lch(family.window_for(seed, dark, t)).l;
                let primary = tone::lch(family.window_container_for(seed, dark, t)).l;
                let step = primary - bg;
                if dark {
                    // 8-bit rounding costs up to about 0.003.
                    assert!(step >= 0.034, "{} {t}: {step}", family.id());
                    // Below about L 0.16 the step grows to stay visible.
                    if bg >= 0.17 {
                        assert!((step - 0.038).abs() < 0.006, "{} {t}: {step}", family.id());
                    }
                } else {
                    assert!((step + 0.032).abs() < 0.006, "{} {t}: {step}", family.id());
                }
            }
        }
    }
}

/// Bare takes Terminal's depth in its own quiet tint; no other style's
/// default background moves.
#[test]
fn bare_windows_take_terminal_depth_in_their_own_tint() {
    let bare = tone::lch(Family::Bare.window_for(Family::Bare.accent(), true, 0));
    let terminal = tone::lch(Family::Terminal.window_for(Family::Terminal.accent(), true, 0));
    assert!(
        (bare.l - terminal.l).abs() < 0.01,
        "{} vs {}",
        bare.l,
        terminal.l
    );
    assert!(bare.c <= 0.026 + 0.002, "low chroma: {}", bare.c);
    let seed = tone::lch(Family::Bare.accent());
    let hue = (bare.h - seed.h).sin().abs();
    assert!(hue < 0.1, "Bare's own hue");
    // Before: lighter, like the reported window.
    let (earlier, _) = Family::Bare.earlier_window(Family::Bare.accent(), true);
    assert!((tone::lch(earlier).l - 0.282).abs() < 0.005);
    for family in Family::ALL {
        let (old, _) = family.earlier_window(family.accent(), !family.light());
        let now = family.window_for(family.accent(), !family.light(), 0);
        assert_eq!(old == now, family != Family::Bare, "{}", family.id());
    }
}

/// The darkest Bare is darker still, and both ends stay inside the limits.
#[test]
fn window_tone_limits_follow_each_style() {
    assert_eq!(Family::Bare.tone_range(true), (-6, 10));
    assert_eq!(Family::Terminal.tone_range(true), (-6, 10));
    assert_eq!(Family::Midnight.tone_range(true), (0, 10));
    assert_eq!(Family::Soft.tone_range(true), (-10, 10));
    assert_eq!(Family::Paper.tone_range(false), (-10, 3));
    assert_eq!(Family::Editorial.tone_range(false), (-10, 1));
    // Beyond the range nothing moves further.
    let seed = Family::Bare.accent();
    assert_eq!(
        Family::Bare.window_for(seed, true, -10),
        Family::Bare.window_for(seed, true, -6)
    );
}

#[test]
fn window_tones_round_trip_and_reject_damage() {
    let tones = WindowTone::default()
        .with(Family::Bare, -6)
        .with(Family::Paper, 2)
        .with(Family::Soft, 40);
    assert_eq!(tones.get(Family::Soft), WindowTone::MAX);
    let value = tones.to_value();
    assert_eq!(value, json!({"soft": 10, "paper": 2, "bare": -6}));
    assert_eq!(WindowTone::from_value(&value), tones);
    assert!(WindowTone::accepts(&value));
    assert!(!WindowTone::accepts(&json!({"bare": 11})));
    assert!(!WindowTone::accepts(&json!({"theme-bare": 1})));
    assert!(!WindowTone::accepts(&json!([1])));
    // Preferences stay readable when an entry is damaged.
    let lenient = WindowTone::from_value(&json!({"bare": -40, "unknown": 3, "paper": "x"}));
    assert_eq!(lenient, WindowTone::default().with(Family::Bare, -10));
    let a: crate::config::Appearance =
        serde_json::from_value(json!({"window_tone": {"terminal": 4}})).unwrap();
    assert_eq!(a.window_tone.get(Family::Terminal), 4);
    assert_eq!(
        serde_json::to_value(crate::config::Appearance::default())
            .unwrap()
            .get("window_tone"),
        None,
        "left out at the defaults, so older files keep their bytes"
    );
}

/// A record moves only window surfaces that follow its style.
#[test]
fn set_tone_keeps_surfaces_the_style_never_derived() {
    let mut p = record(Family::Bare);
    set_tone(&mut p, -4, WindowTone::default());
    let dark = p.appearance.as_ref().unwrap().dark.as_ref().unwrap();
    assert_eq!(
        dark.background,
        toned(Family::Bare, None, -4).dark.unwrap().background
    );
    assert_eq!(tones(&p).unwrap().get(Family::Bare), -4);
    let custom = Some(ColorChoice::Hex("#123456".into()));
    p.appearance
        .as_mut()
        .unwrap()
        .dark
        .as_mut()
        .unwrap()
        .background = custom.clone();
    set_tone(&mut p, 3, WindowTone::default());
    let dark = p.appearance.as_ref().unwrap().dark.as_ref().unwrap();
    assert_eq!(dark.background, custom);
    assert_eq!(
        dark.primary,
        toned(Family::Bare, None, 3).dark.unwrap().primary
    );
}
