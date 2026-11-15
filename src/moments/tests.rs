use super::*;
use crate::widget_style::{
    self,
    resolver::{Source, Surface},
};
use serde_json::json;
fn style(config: &Config) -> crate::widget_style::resolver::Style {
    let _scope = widget_style::scope(config);
    widget_style::presentation(config, config.wallpaper_palette(), true)
}
#[test]
fn fb15_absent_moments_preserve_legacy_and_partial_fields_keep_phase_defaults() {
    let old = Config::from_bytes(br#"{"clock":{"face":"analog"}}"#).unwrap();
    assert_eq!(old.moments.active, None);
    assert_eq!(old.moment_clock(), old.clock);
    assert_eq!(old.moment_appearance(), old.appearance);
    let partial: Config = serde_json::from_value(
        json!({"moments":{"active":"day","day":{"clock":{"face":"digital"}}}}),
    )
    .unwrap();
    assert_eq!(partial.moments.day.family, Some(Family::Paper));
    assert_eq!(partial.moments.night.family, Some(Family::Midnight));
    let inherit: Config =
        serde_json::from_value(json!({"moments":{"day":{"family":null}}})).unwrap();
    assert_eq!(inherit.moments.day.family, None);
    for bad in [
        json!({"active":"sunrise"}),
        json!({"day":{"command":"sh"}}),
        json!({"night":{"clock":{"face":"html"}}}),
        json!({"day":null}),
    ] {
        assert!(serde_json::from_value::<Config>(json!({"moments":bad})).is_err());
    }
}
#[test]
fn fb15_precedence_reset_and_readability_keep_base_clock_intact() {
    let mut c = Config::default();
    c.clock.face = Some(ClockFace::Analog);
    c.clock.weight = Some(ClockWeight::Bold);
    c.clock.surface = Some(ClockSurface::Panel);
    let base = c.clock;
    c.moments.active = Some(Phase::Day);
    let day = style(&c);
    assert_eq!(day.clock_face.value, ClockFace::Analog);
    assert_eq!(day.clock_face.source, Source::Instance);
    assert_eq!(day.display.value.weight, ClockWeight::Bold.native());
    assert_eq!(day.ink.source, Source::Moment);
    c.moments.day.clock.face = Some(ClockFace::Digital);
    c.moments.day.clock.weight = Some(ClockWeight::Light);
    c.moments.day.clock.surface = Some(ClockSurface::None);
    let overridden = style(&c);
    assert_eq!(overridden.clock_face.value, ClockFace::Digital);
    assert_eq!(overridden.clock_face.source, Source::MomentInstance);
    assert_eq!(overridden.display.value.weight, ClockWeight::Light.native());
    assert_eq!(overridden.surface.value, Surface::None);
    assert_eq!(overridden.padding.value, 4);
    c.appearance.contrast_assist = true;
    assert_eq!(style(&c).surface.source, Source::Accessibility);
    c.appearance.contrast_assist = false;
    c.moments.day.clock = Clock::default();
    assert_eq!(style(&c).clock_face.value, ClockFace::Analog);
    c.moments.active = Some(Phase::Night);
    assert_eq!(c.clock, base);
    assert_ne!(style(&c).ink.value, day.ink.value);
    c.moments.active = None;
    assert_eq!(c.moment_clock(), base);
}
#[test]
fn fb15_depth_falls_back_for_a_moment_card_and_returns_on_reset() {
    let mut c = Config::default();
    c.clock.depth = true;
    c.clock.surface = Some(ClockSurface::None);
    c.moments.active = Some(Phase::Day);
    assert!(crate::depth::eligible(&c));
    // Every face stands behind the scene; a moment's card does not.
    c.moments.day.clock.face = Some(ClockFace::Analog);
    assert!(crate::depth::eligible(&c));
    c.moments.day.clock.surface = Some(ClockSurface::Panel);
    assert!(!crate::depth::eligible(&c));
    c.moments.day.clock.surface = None;
    assert!(crate::depth::eligible(&c));
    assert!(c.clock.depth);
}
