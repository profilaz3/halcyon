//! What each style gives the dock and panel, and how a glass material from
//! another look (Rainy evening's Clear glass) is refitted when the style
//! changes. Isolated fixtures only.
use super::*;
use crate::desktop::store::tests::Fixture;
use crate::materials::{Finish, Kind, Target};
use dock_islands_core::material::BarMaterial;
use std::fs;

/// Rainy evening's bars: Clear glass at the most see-through the panel
/// accepts, a fifth of the accent.
const RAIN_BARS: Finish = Finish {
    kind: Kind::Clear,
    opacity: Some(45),
    tint: 20,
};

/// Bar opacity, radius, padding, spacing, island gap, floating and the
/// least opacity a glass material may give the bars, per style.
const EXPECTED: [(Family, f32, u32, u32, u32, u16, bool, u8); 14] = [
    (Family::Soft, 0.94, 20, 8, 6, 12, true, 85),
    (Family::Outline, 0.98, 5, 6, 4, 8, false, 90),
    (Family::Paper, 1., 12, 8, 6, 12, false, 90),
    (Family::Bare, 0.90, 24, 6, 8, 16, true, 60),
    (Family::Glass, 0.72, 18, 8, 6, 12, true, 45),
    (Family::DarkGlass, 0.72, 18, 8, 6, 12, true, 45),
    (Family::Frosted, 0.72, 18, 8, 6, 12, true, 45),
    (Family::Midnight, 1., 10, 6, 6, 8, false, 100),
    (Family::Terminal, 1., 0, 4, 4, 6, false, 100),
    (Family::Neon, 0.96, 12, 6, 6, 10, true, 90),
    (Family::Editorial, 1., 2, 6, 6, 8, false, 95),
    (Family::Retro, 1., 8, 6, 6, 10, true, 95),
    (Family::Pastel, 0.9, 28, 8, 8, 14, true, 80),
    (Family::Bauhaus, 1., 0, 6, 6, 8, false, 100),
];

/// See-through by design: glass cards or cards with no surface at all.
fn see_through(family: Family) -> bool {
    family.geometry().0 < 80 || family.spec().transparent
}

fn hex_rgb(choice: &Option<ColorChoice>) -> [u8; 3] {
    let Some(ColorChoice::Hex(hex)) = choice else {
        panic!("a fixed color, got {choice:?}");
    };
    let byte = |i: usize| u8::from_str_radix(&hex[1 + i * 2..3 + i * 2], 16).unwrap();
    [byte(0), byte(1), byte(2)]
}

/// The bar's fill drawn over `under` at `alpha`, as the compositor blends it.
fn over(fill: [u8; 3], alpha: f32, under: [u8; 3]) -> [u8; 3] {
    std::array::from_fn(|i| {
        (f32::from(fill[i]) * alpha + f32::from(under[i]) * (1. - alpha)).round() as u8
    })
}

fn lightness(rgb: [u8; 3]) -> f32 {
    desktop_theme::tone::lch(rgb).l
}

#[test]
fn every_style_writes_its_own_bar_shape_fill_and_glass_floor() {
    assert_eq!(EXPECTED.map(|row| row.0), Family::ALL);
    for (family, opacity, radius, padding, spacing, gap, floating, floor) in EXPECTED {
        let p = record(family);
        for bar in [p.dock.as_ref().unwrap(), p.panel.as_ref().unwrap()] {
            assert_eq!(bar.opacity, Some(opacity), "{family:?}");
            assert_eq!(bar.border_radius, Some(radius), "{family:?}");
            assert_eq!(bar.padding, Some(padding), "{family:?}");
            assert_eq!(bar.spacing, Some(spacing), "{family:?}");
            assert_eq!(bar.island_gap, Some(gap), "{family:?}");
            assert_eq!(bar.floating, Some(floating), "{family:?}");
            assert_eq!(bar.margin, floating.then_some(6), "{family:?}");
            // The bars paint the theme's background, so they follow mode.
            assert_eq!(bar.background, Some(Background::ThemeDefault), "{family:?}");
        }
        assert_eq!(family.spec().bar.glass_floor, floor, "{family:?}");
        // Glass never makes a bar more see-through than the style's own bar
        // and never below what the installed panel accepts.
        assert!(
            (dock_islands_core::material::BAR_MIN_OPACITY..=100).contains(&floor),
            "{family:?}"
        );
        assert!(f32::from(floor) <= opacity * 100. + 0.5, "{family:?}");
        // Square styles stay attached to the edge.
        if radius == 0 {
            assert!(!floating, "{family:?}");
        }

        // The bar's fill is the style's own card surface in the variant the
        // style owns, so the dock and the cards are one tone.
        let look = p.appearance.as_ref().unwrap();
        let owned = if family.light() {
            look.light.as_ref()
        } else {
            look.dark.as_ref()
        }
        .unwrap();
        let fill = hex_rgb(&owned.background);
        assert_eq!(fill, family.surface(family.accent()), "{family:?}");
        if family.light() {
            assert!(lightness(fill) > 0.9, "{family:?} light bar {fill:?}");
        } else {
            assert!(lightness(fill) < 0.36, "{family:?} dark bar {fill:?}");
        }
    }
}

/// The reported dock: Rainy evening's Clear glass at 45% left on the bars
/// when the style changes. The panel draws the record's opacity instead of
/// the style's own, so Midnight's black dock became 45% black over the
/// wallpaper: a pale, foggy bubble. Under a solid style the composite stays
/// at the bar's tone even over a white picture; a light style stays light
/// over a black one.
#[test]
fn glass_from_another_look_cannot_turn_a_solid_style_bar_into_haze() {
    for (family, opacity, ..) in EXPECTED {
        let bar = crate::desktop::materials::bar_record(RAIN_BARS, Some(family));
        bar.validate().unwrap();
        // The finish itself is the user's: kind and tint are kept.
        assert_eq!((bar.finish.kind, bar.finish.tint), (Kind::Clear, 20));
        // cosmic-panel-islands: the record's opacity times the bar opacity.
        let alpha = f32::from(bar.finish.opacity()) / 100. * opacity;
        let fill = family.surface(family.accent());
        let fill_l = lightness(fill);
        if see_through(family) {
            continue;
        }
        if family.light() {
            let shown = lightness(over(fill, alpha, [0, 0, 0]));
            assert!(
                shown > 0.7,
                "{family:?} over black: L {shown:.2}, alpha {alpha:.2}"
            );
        } else {
            for picture in [[255, 255, 255], [40, 62, 118]] {
                let shown = lightness(over(fill, alpha, picture));
                assert!(
                    shown < 0.5 && shown - fill_l < 0.2,
                    "{family:?} over {picture:?}: L {shown:.2} vs bar {fill_l:.2}, alpha {alpha:.2}"
                );
            }
        }
    }
    // Midnight and Terminal are solid: nothing of the picture shows.
    for family in [Family::Midnight, Family::Terminal, Family::Bauhaus] {
        let bar = crate::desktop::materials::bar_record(RAIN_BARS, Some(family));
        assert_eq!(bar.finish.opacity(), 100, "{family:?}");
    }
    // Glass styles keep the user's 45%.
    for family in [Family::Glass, Family::Frosted] {
        let bar = crate::desktop::materials::bar_record(RAIN_BARS, Some(family));
        assert_eq!(bar.finish.opacity, Some(45), "{family:?}");
    }
    // Without a style the panel's own floor still applies.
    let bar = crate::desktop::materials::bar_record(
        Finish {
            opacity: Some(20),
            ..RAIN_BARS
        },
        None,
    );
    assert_eq!(bar.finish.opacity, Some(45));
}

fn rainy(name: &str) -> (Fixture, DesktopStore, Installed) {
    let fixture = Fixture::new(name);
    crate::desktop::islands::fixture_installed(&fixture.0);
    let path = fixture.0.join("config.json");
    fs::write(
        &path,
        r#"{"appearance":{"family":"glass","color_source":"custom","sync_colors":true,"wallpaper_colors":false}}"#,
    )
    .unwrap();
    let store = fixture.store().with_app_config(path);
    for target in [Target::Dock, Target::Panel] {
        crate::desktop::materials::apply(&store, target, Some(RAIN_BARS)).unwrap();
    }
    let installed = Installed::discover(&store);
    (fixture, store, installed)
}

fn bar_opacity(store: &DesktopStore, name: &str) -> Option<u8> {
    store
        .read_user(&crate::desktop::materials::key(name))
        .unwrap()
        .map(|raw| raw.decode::<BarMaterial>().unwrap().finish.opacity())
}

fn switch(store: &DesktopStore, installed: &Installed, family: Family) -> String {
    let mut p = record(family);
    preserve_custom(store, installed, &mut p).unwrap();
    let plan = plan(store, installed, &p, &p.domains().into_iter().collect());
    assert!(plan.can_apply(), "{:?}", plan.blocking_reasons());
    apply(store, &plan).unwrap().id
}

#[test]
fn style_switch_refits_bar_glass_and_keeps_the_users_finish() {
    let (_fixture, store, installed) = rainy("bar-glass-style-switch");
    assert_eq!(bar_opacity(&store, "Dock"), Some(45));
    for (family, want) in [
        (Family::Midnight, 100),
        (Family::Terminal, 100),
        (Family::Soft, 85),
        (Family::Paper, 90),
        (Family::Glass, 45),
    ] {
        switch(&store, &installed, family);
        for bar in ["Dock", "Panel"] {
            assert_eq!(bar_opacity(&store, bar), Some(want), "{family:?} {bar}");
        }
        // The saved choice is untouched: it comes back with a glass style.
        let (_, cfg) = crate::desktop::wallpaper::automatic::read_config(&store).unwrap();
        assert_eq!(cfg.materials.dock, Some(RAIN_BARS), "{family:?}");
        assert_eq!(cfg.materials.panel, Some(RAIN_BARS), "{family:?}");
    }
    // Undo of a style change restores the bars' glass with it.
    let change = switch(&store, &installed, Family::Midnight);
    assert_eq!(bar_opacity(&store, "Dock"), Some(100));
    store.undo_latest(&change).unwrap();
    assert_eq!(bar_opacity(&store, "Dock"), Some(45));
    let (_, cfg) = crate::desktop::wallpaper::automatic::read_config(&store).unwrap();
    assert_eq!(cfg.appearance.family, Some(Family::Glass));
}

#[test]
fn choosing_glass_under_a_solid_style_keeps_the_bar_solid() {
    let (_fixture, store, installed) = rainy("bar-glass-choice");
    switch(&store, &installed, Family::Midnight);
    let liquid = Finish::new(Kind::Liquid);
    crate::desktop::materials::apply(&store, Target::Shared, Some(liquid)).unwrap();
    // Dock and Panel have their own Clear glass, refitted to Midnight.
    assert_eq!(bar_opacity(&store, "Dock"), Some(100));
    crate::desktop::materials::apply(&store, Target::Dock, None).unwrap();
    // Following the shared Liquid glass: still Midnight's solid floor.
    let dock = store
        .read_user(&crate::desktop::materials::key("Dock"))
        .unwrap()
        .unwrap()
        .decode::<BarMaterial>()
        .unwrap();
    assert_eq!(dock.finish.kind, Kind::Liquid);
    assert_eq!(dock.finish.opacity(), 100);
}

#[test]
fn style_switch_leaves_absent_and_future_bar_records_alone() {
    let fixture = Fixture::new("bar-glass-future");
    crate::desktop::islands::fixture_installed(&fixture.0);
    let path = fixture.0.join("config.json");
    fs::write(
        &path,
        r#"{"appearance":{"family":"glass"},"materials":{"dock":{"kind":"clear","opacity":45,"tint":20}}}"#,
    )
    .unwrap();
    let store = fixture.store().with_app_config(path);
    let future = "(version:99,finish:(kind:liquid,opacity:None,tint:35))";
    fixture.user(dock_islands_core::material::NAMESPACE, "Dock", future);
    let installed = Installed::discover(&store);
    switch(&store, &installed, Family::Midnight);
    assert_eq!(
        fixture
            .read(dock_islands_core::material::NAMESPACE, "Dock")
            .as_deref(),
        Some(future)
    );
    // No record is created for a bar that had none.
    assert_eq!(bar_opacity(&store, "Panel"), None);
}
