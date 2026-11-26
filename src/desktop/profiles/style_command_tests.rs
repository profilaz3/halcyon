//! Applying a style by name, as `halcyon --style` does: found by name or
//! id, fitted as Settings › Appearance fits it, and applied once.
use super::*;
use crate::desktop::store::tests::Fixture;
use std::fs;

fn desk(name: &str) -> (Fixture, DesktopStore, Installed) {
    let fixture = Fixture::new(name);
    crate::desktop::islands::fixture_installed(&fixture.0);
    let path = fixture.0.join("config.json");
    fs::write(
        &path,
        r#"{"appearance":{"family":"glass","color_source":"custom","sync_colors":true,"wallpaper_colors":false}}"#,
    )
    .unwrap();
    let store = fixture.store().with_app_config(path);
    let installed = Installed::discover(&store);
    (fixture, store, installed)
}

#[test]
fn styles_are_found_by_name_or_id() {
    let choices = style_choices();
    for family in Family::ALL {
        assert!(
            choices
                .iter()
                .any(|(id, name)| id == family.preset() && name == family.title())
        );
    }
    for preset in named() {
        assert!(
            choices.iter().any(|(id, _)| id == preset.id),
            "{}",
            preset.id
        );
    }
    assert_eq!(
        find_style(" porcelain ").unwrap(),
        Family::Porcelain.preset()
    );
    assert_eq!(
        find_style("Mica Slate").unwrap(),
        Family::MicaSlate.preset()
    );
    assert_eq!(find_style("theme-soft").unwrap(), Family::Soft.preset());
    let missing = find_style("Plaid").unwrap_err();
    assert!(missing.contains("halcyon --styles"), "{missing}");
}

#[test]
fn a_style_keeps_your_colors_and_bar_shape_unless_asked() {
    let (_fixture, store, installed) = desk("style-command-fit");
    let record = record(Family::Porcelain);
    let kept = style_profile(&store, &installed, Family::Porcelain.preset(), false).unwrap();
    // Your own color choices come along, as when picked in Settings.
    assert_eq!(
        kept.widget_style
            .as_ref()
            .unwrap()
            .pointer("/appearance/sync_colors"),
        Some(&serde_json::json!(true))
    );
    for bar in [&kept.dock, &kept.panel].into_iter().flatten() {
        assert!(bar.floating.is_none() && bar.margin.is_none() && bar.border_radius.is_none());
    }
    let own = style_profile(&store, &installed, Family::Porcelain.preset(), true).unwrap();
    assert_eq!(
        own.dock.as_ref().unwrap().floating,
        record.dock.as_ref().unwrap().floating
    );
    assert_eq!(
        own.panel.as_ref().unwrap().border_radius,
        record.panel.as_ref().unwrap().border_radius
    );
    let unknown = style_profile(&store, &installed, "theme-plaid", false).unwrap_err();
    assert!(unknown.contains("theme-plaid"), "{unknown}");
}

#[test]
fn a_style_applies_once_like_settings() {
    use crate::automation::run::{Ran, run_style};
    let (fixture, store, installed) = desk("style-command-apply");
    let held = Default::default();
    let ran = run_style(&store, &installed, Family::Porcelain.preset(), false, &held);
    assert!(matches!(ran, Ran::Applied { .. }), "{ran:?}");
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.0.join("config.json")).unwrap()).unwrap();
    assert_eq!(saved["appearance"]["family"], "porcelain");
    assert_eq!(
        run_style(&store, &installed, Family::Porcelain.preset(), false, &held),
        Ran::AlreadySet
    );
    // A part Settings holds unapplied waits instead of being overwritten.
    let held: std::collections::BTreeSet<Domain> = [Domain::WidgetStyle].into_iter().collect();
    let ran = run_style(&store, &installed, Family::Paper.preset(), false, &held);
    assert!(matches!(ran, Ran::Waiting(_)), "{ran:?}");
}
