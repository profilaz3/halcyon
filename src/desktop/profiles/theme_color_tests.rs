//! Native color roles a shared theme coordinates: the active-window outline
//! and accents readable in each COSMIC variant. Isolated fixtures only.
use super::*;
use crate::desktop::appearance::{Control, Value as Edit};
use crate::desktop::store::tests::Fixture;
use cosmic::cosmic_theme::palette::{FromColor, Oklab, Srgb};
use std::fs;

fn write_v2(fixture: &Fixture, namespace: &str, key: &str, value: &str) {
    let path = fixture
        .0
        .join("config/cosmic")
        .join(namespace)
        .join("v2")
        .join(key);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, value).unwrap();
}

fn lightness(color: cosmic::cosmic_theme::palette::Srgba) -> f32 {
    Oklab::from_color(Srgb::new(color.red, color.green, color.blue)).l
}

/// The reported desktop: an exact dark wallpaper swatch as the accent on
/// both variants and an older custom active-window outline color.
fn reported(name: &str, dark: bool) -> (Fixture, DesktopStore, Installed) {
    let fixture = Fixture::new(name);
    for variant in ["Dark", "Light"] {
        write_v2(
            &fixture,
            &format!("com.system76.CosmicTheme.{variant}.Builder"),
            "accent",
            "Some(\"#40352CFF\")",
        );
    }
    write_v2(
        &fixture,
        "com.system76.CosmicTheme.Dark.Builder",
        "window_hint",
        "Some(\"#63D0DFFF\")",
    );
    fixture.user(
        "com.system76.CosmicTheme.Mode",
        "is_dark",
        &dark.to_string(),
    );
    let path = fixture.0.join("config.json");
    fs::write(
        &path,
        r#"{"theme":"amber","appearance":{"accent":[64,53,44],"color_source":"custom","sync_colors":true,"wallpaper_colors":true}}"#,
    )
    .unwrap();
    let store = fixture.store().with_app_config(path);
    let installed = Installed::discover(&store);
    (fixture, store, installed)
}

#[test]
fn applying_a_theme_links_the_window_outline_and_keeps_accents_readable_in_both_modes() {
    let mut failures = Vec::new();
    for dark in [true, false] {
        for family in Family::ALL {
            let (_fixture, store, installed) =
                reported(&format!("theme-outline-{}-{dark}", family.id()), dark);
            let mut p = record(family);
            preserve_custom(&store, &installed, &mut p).unwrap();
            let plan = plan(&store, &installed, &p, &p.domains().into_iter().collect());
            assert!(plan.can_apply(), "{:?}", plan.blocking_reasons());
            apply(&store, &plan).unwrap();
            let draft = AppearanceDraft::load(&store).unwrap();
            let mode = if dark { "dark mode" } else { "light mode" };
            for variant in ThemeVariant::ALL {
                let outline = draft.color(variant, ColorRole::Outline).known().copied();
                if outline != Some(None) {
                    failures.push(format!(
                        "{family:?} in {mode}: {variant:?} active window outline stays {:?} instead of following the accent",
                        outline.flatten().map(|c| appearance::hex(c, false))
                    ));
                }
            }
            let accent = |variant| {
                draft
                    .color(variant, ColorRole::Accent)
                    .known()
                    .copied()
                    .flatten()
                    .expect("themes set an accent")
            };
            // COSMIC's own palettes: pastel accents in Dark, deep ones in Light.
            for (variant, ok, want) in [
                (ThemeVariant::Dark, 0.68..=1.0, ">= 0.68"),
                (ThemeVariant::Light, 0.0..=0.56, "<= 0.56"),
            ] {
                let color = accent(variant);
                let l = lightness(color);
                if !ok.contains(&l) {
                    failures.push(format!(
                        "{family:?} in {mode}: {variant:?} accent {} has Oklab L {l:.2}, want {want}",
                        appearance::hex(color, false)
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

fn outline(
    store: &DesktopStore,
    variant: ThemeVariant,
) -> Option<Option<cosmic::cosmic_theme::palette::Srgba>> {
    AppearanceDraft::load(store)
        .unwrap()
        .color(variant, ColorRole::Outline)
        .known()
        .copied()
}
fn switch(store: &DesktopStore, installed: &Installed, family: Family) {
    let mut p = record(family);
    preserve_custom(store, installed, &mut p).unwrap();
    let plan = plan(store, installed, &p, &p.domains().into_iter().collect());
    assert!(plan.can_apply(), "{:?}", plan.blocking_reasons());
    apply(store, &plan).unwrap();
}

#[test]
fn style_changes_keep_an_outline_chosen_after_themes_and_link_an_older_one() {
    let (fixture, store, installed) = reported("theme-outline-preserve", true);
    switch(&store, &installed, Family::Soft);
    assert_eq!(outline(&store, ThemeVariant::Dark), Some(None));
    // Chosen in Theme & fonts after the first theme: kept like any Custom.
    let chosen = appearance::parse_hex("#FFAD00", false);
    let mut native = AppearanceDraft::load(&store).unwrap();
    native.set(
        Control::Color(ThemeVariant::Dark, ColorRole::Outline),
        Edit::Color(chosen),
    );
    store.apply(&native.plan().patch).unwrap();
    switch(&store, &installed, Family::Paper);
    assert_eq!(outline(&store, ThemeVariant::Dark), Some(chosen));
    assert_eq!(outline(&store, ThemeVariant::Light), Some(None));
    // The outline that predates themes (still the one Before Themes keeps)
    // follows the accent on the next style change, as an older release
    // applied themes without linking it.
    write_v2(
        &fixture,
        "com.system76.CosmicTheme.Dark.Builder",
        "window_hint",
        "Some(\"#63D0DFFF\")",
    );
    switch(&store, &installed, Family::Outline);
    assert_eq!(outline(&store, ThemeVariant::Dark), Some(None));
}

#[test]
fn legacy_surface_roles_require_explicit_repair_before_recoloring() {
    for family in Family::ALL {
        let (fixture, store, installed) =
            reported(&format!("theme-previous-release-{}", family.id()), true);
        // Write the previous release's derivation as if it applied `family`.
        let (accent, surface) = desktop_theme::previous_native(family, None);
        let hex = |[r, g, b]: [u8; 3]| format!("Some(\"#{r:02X}{g:02X}{b:02X}FF\")");
        let owned = if family.light() { "Light" } else { "Dark" };
        for variant in ["Dark", "Light"] {
            write_v2(
                &fixture,
                &format!("com.system76.CosmicTheme.{variant}.Builder"),
                "accent",
                &hex(accent),
            );
        }
        for key in ["bg_color", "primary_container_bg"] {
            write_v2(
                &fixture,
                &format!("com.system76.CosmicTheme.{owned}.Builder"),
                key,
                &hex(surface),
            );
        }
        let mut config: serde_json::Value =
            serde_json::from_slice(&fs::read(store.app_config().unwrap()).unwrap()).unwrap();
        config["appearance"]["family"] = serde_json::json!(family);
        config["appearance"]["accent"] = serde_json::Value::Null;
        config["appearance"]["wallpaper_colors"] = false.into();
        fs::write(store.app_config().unwrap(), config.to_string()).unwrap();
        let (_, typed) = crate::desktop::wallpaper::automatic::read_config(&store).unwrap();
        let draft = AppearanceDraft::load(&store).unwrap();
        let state = native_custom(&draft, typed.appearance);
        assert!(
            state.surfaces,
            "{family:?}: legacy surfaces require reviewed repair"
        );
        // The stale outline is the one thing that does not follow yet.
        assert!(state.windows, "{family:?}");
        // A recolor moves previous-release surfaces to the current tones.
        let mut next = AppearanceDraft::load(&store).unwrap();
        linked_colors(&mut next, typed.appearance, [200, 120, 60]);
        let variant = if family.light() {
            ThemeVariant::Light
        } else {
            ThemeVariant::Dark
        };
        let moved = next
            .color(variant, ColorRole::Background)
            .known()
            .copied()
            .flatten();
        let target = surface;
        assert!(
            moved.is_some_and(|c| {
                [c.red, c.green, c.blue].map(|v| (v * 255.).round() as u8) == target
            }),
            "{family:?}: {moved:?}"
        );
        drop(installed);
    }
}

#[test]
fn unset_window_surfaces_follow_the_style_and_wallpaper_tint() {
    // COSMIC's default leaves the window background unset. That is no one's
    // choice, so windows take the style's tint instead of staying gray.
    let (fixture, store, installed) = reported("theme-unset-surfaces", true);
    for key in ["bg_color", "primary_container_bg"] {
        write_v2(
            &fixture,
            "com.system76.CosmicTheme.Dark.Builder",
            key,
            "None",
        );
    }
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(store.app_config().unwrap()).unwrap()).unwrap();
    config["appearance"]["family"] = serde_json::json!(Family::Soft);
    config["appearance"]["native_mode_palettes"] = 1.into();
    config["appearance"]["accent"] = serde_json::json!([224, 138, 243]);
    fs::write(store.app_config().unwrap(), config.to_string()).unwrap();
    let (_, typed) = crate::desktop::wallpaper::automatic::read_config(&store).unwrap();
    let draft = AppearanceDraft::load(&store).unwrap();
    assert!(!native_custom(&draft, typed.appearance).surfaces);
    let mut next = AppearanceDraft::load(&store).unwrap();
    linked_colors(&mut next, typed.appearance, [90, 140, 200]);
    let byte = |role| {
        next.color(ThemeVariant::Dark, role)
            .known()
            .copied()
            .flatten()
            .map(|c| [c.red, c.green, c.blue].map(|v| (v * 255.).round() as u8))
    };
    assert_eq!(
        byte(ColorRole::Background),
        Some(Family::Soft.surface([90, 140, 200]))
    );
    assert_eq!(
        byte(ColorRole::Primary),
        Some(Family::Soft.container([90, 140, 200]))
    );
    // An explicit background stays the user's own.
    write_v2(
        &fixture,
        "com.system76.CosmicTheme.Dark.Builder",
        "bg_color",
        "Some(\"#102030FF\")",
    );
    let mut kept = AppearanceDraft::load(&store).unwrap();
    linked_colors(&mut kept, typed.appearance, [90, 140, 200]);
    assert!(!kept.edited(Control::Color(ThemeVariant::Dark, ColorRole::Background)));
    drop(installed);
}

#[test]
fn material_survives_theme_apply_and_captured_setup_restores_native_bars() {
    use crate::materials::{Finish, Kind, Target};
    let (fixture, store, installed) = reported("material-profile", true);
    crate::desktop::islands::fixture_installed(&fixture.0);
    crate::desktop::materials::apply(&store, Target::Shared, Some(Finish::new(Kind::Liquid)))
        .unwrap();
    let theme = record(Family::Soft);
    let proposal = plan(
        &store,
        &installed,
        &theme,
        &theme.domains().into_iter().collect(),
    );
    assert!(proposal.can_apply(), "{:?}", proposal.blocking_reasons());
    apply(&store, &proposal).unwrap();
    let (_, cfg) = crate::desktop::wallpaper::automatic::read_config(&store).unwrap();
    assert_eq!(cfg.materials.shared.unwrap().kind, Kind::Liquid);
    let captured = crate::desktop::profiles::capture(&store, "Material setup")
        .unwrap()
        .profile;
    assert_eq!(captured.written_version(), 8);
    crate::desktop::materials::apply(&store, Target::Shared, Some(Finish::new(Kind::Smoked)))
        .unwrap();
    let path = store.app_config().unwrap();
    let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    raw["material_motion"] = false.into();
    fs::write(path, raw.to_string()).unwrap();
    let proposal = plan(&store, &installed, &captured, &[Domain::WidgetStyle].into());
    assert!(proposal.can_apply(), "{:?}", proposal.blocking_reasons());
    let change = apply(&store, &proposal).unwrap();
    let (_, cfg) = crate::desktop::wallpaper::automatic::read_config(&store).unwrap();
    assert!(
        cfg.material_motion,
        "the captured Liquid setup restores enabled motion"
    );
    for bar in ["Dock", "Panel"] {
        let material = store
            .read_user(&crate::desktop::materials::key(bar))
            .unwrap()
            .unwrap()
            .decode::<dock_islands_core::material::BarMaterial>()
            .unwrap();
        assert_eq!(material.finish.kind, Kind::Liquid);
    }
    store.undo_latest(&change.id).unwrap();
    let (_, cfg) = crate::desktop::wallpaper::automatic::read_config(&store).unwrap();
    assert_eq!(cfg.materials.shared.unwrap().kind, Kind::Smoked);
    assert!(!cfg.material_motion, "Undo restores disabled motion");
}

#[test]
fn material_motion_capture_restores_explicit_values_and_material_defaults() {
    use serde_json::json;
    // A configured preference belongs to the setup even without a finish.
    // A finish also owns the effective default when motion was never configured.
    for (has_material, configured, expected) in [
        (false, Some(true), true),
        (false, Some(false), false),
        (true, Some(true), true),
        (true, Some(false), false),
        (true, None, true),
    ] {
        let fixture = Fixture::new(&format!("material-motion-{has_material}-{configured:?}"));
        let path = fixture.0.join("config.json");
        let mut raw = json!({"future":{"keep":true}});
        if has_material {
            raw["materials"] = json!({"widgets":{"kind":"liquid","opacity":null,"tint":35}});
        }
        if let Some(on) = configured {
            raw["material_motion"] = on.into();
        }
        fs::write(&path, raw.to_string()).unwrap();
        let store = fixture.store().with_app_config(path.clone());
        let installed = Installed::discover(&store);
        let captured = crate::desktop::profiles::capture(&store, "Motion setup")
            .unwrap()
            .profile;
        assert_eq!(captured.version, 8);
        assert_eq!(
            captured.widget_style.as_ref().unwrap()["material_motion"],
            expected
        );
        let imported = import_local(&export_local(&captured).unwrap()).unwrap();
        assert_eq!(imported, captured);
        let unchanged = plan(&store, &installed, &imported, &[Domain::WidgetStyle].into());
        assert!(unchanged.blocking_reasons().is_empty());
        assert!(
            unchanged.is_empty(),
            "capturing a default must not create a write"
        );

        raw["material_motion"] = (!expected).into();
        fs::write(&path, raw.to_string()).unwrap();
        let proposal = plan(&store, &installed, &imported, &[Domain::WidgetStyle].into());
        assert!(proposal.can_apply(), "{:?}", proposal.blocking_reasons());
        let change = apply(&store, &proposal).unwrap();
        let (_, cfg) = crate::desktop::wallpaper::automatic::read_config(&store).unwrap();
        assert_eq!(cfg.material_motion, expected);
        store.undo_latest(&change.id).unwrap();
        let restored: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            restored, raw,
            "Undo restores motion and preserves unknown fields"
        );
    }
}

#[test]
fn material_motion_capture_keeps_genuinely_legacy_setups_on_their_old_format() {
    use serde_json::json;
    for (name, raw) in [
        ("absent", json!({})),
        ("empty", json!({"materials":{}})),
        (
            "null",
            json!({"materials":{"shared":null,"widgets":null,"dock":null,"panel":null}}),
        ),
    ] {
        let fixture = Fixture::new(&format!("material-motion-legacy-{name}"));
        let path = fixture.0.join("config.json");
        fs::write(&path, raw.to_string()).unwrap();
        let store = fixture.store().with_app_config(path.clone());
        let installed = Installed::discover(&store);
        let captured = crate::desktop::profiles::capture(&store, "Legacy setup")
            .unwrap()
            .profile;
        assert_eq!(captured.version, 5, "default widget sizes retain format 5");
        let style = captured.widget_style.as_ref().unwrap();
        assert!(style.get("materials").is_none());
        assert!(style.get("material_motion").is_none());
        let imported = import_local(&export_local(&captured).unwrap()).unwrap();
        assert_eq!(imported, captured);
        let mut disabled = raw;
        disabled["material_motion"] = false.into();
        fs::write(&path, disabled.to_string()).unwrap();
        let proposal = plan(&store, &installed, &imported, &[Domain::WidgetStyle].into());
        assert!(proposal.blocking_reasons().is_empty());
        assert!(
            proposal.is_empty(),
            "legacy omission leaves motion unchanged"
        );
    }
}

/// Six setup profiles must carry native palettes in both polarities. No
/// running widget process participates in a mode switch.
#[test]
fn preset_mode_roundtrip_preserves_bar_foreground_contrast() {
    use crate::desktop::panels::Background;
    for preset in crate::desktop::profiles::presets()
        .into_iter()
        .filter(|p| p.id != CLEAN && !p.profile.appearance_only)
    {
        let fixture = Fixture::new(&format!("mode-roundtrip-{}", preset.id));
        fixture.system(
            "com.system76.CosmicPanel",
            "entries",
            "[\"Panel\",\"Dock\"]",
        );
        fixture.system("com.system76.CosmicPanel.Dock", "anchor", "Bottom");
        fixture.system("com.system76.CosmicPanel.Panel", "anchor", "Top");
        fixture.user(
            "com.system76.CosmicPanel.Dock",
            "background",
            "Color((0.1,0.2,0.3))",
        );
        fixture.user(
            "com.system76.CosmicPanel.Panel",
            "background",
            "Color((0.1,0.2,0.3))",
        );
        let path = fixture.0.join("config.json");
        fs::write(&path, "{}").unwrap();
        let store = fixture.store().with_app_config(path);
        let installed = Installed::discover(&store);
        let preset = crate::desktop::profiles::fitted(
            preset,
            &crate::config::Config::default(),
            &installed.bundled_dir,
            false,
        );
        let selected = [
            Domain::Dock,
            Domain::Panel,
            Domain::Appearance,
            Domain::WidgetStyle,
        ]
        .into_iter()
        .collect();
        let planned = plan(&store, &installed, &preset.profile, &selected);
        assert!(planned.can_apply(), "{:?}", planned.blocking_reasons());
        apply(&store, &planned).unwrap();
        for dark in [false, true, false] {
            fixture.user(
                "com.system76.CosmicTheme.Mode",
                "is_dark",
                &dark.to_string(),
            );
            let draft = AppearanceDraft::load(&store).unwrap();
            let variant = if dark {
                ThemeVariant::Dark
            } else {
                ThemeVariant::Light
            };
            let preview = draft.preview(variant);
            for bar in ["Dock", "Panel"] {
                let background: Background = ron::from_str(
                    &fixture
                        .read(&format!("com.system76.CosmicPanel.{bar}"), "background")
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(background, Background::ThemeDefault, "{} {bar}", preset.id);
            }
            let color = |c: cosmic::cosmic_theme::palette::Srgba| {
                cosmic::iced::Color::from_rgb(c.red, c.green, c.blue)
            };
            assert!(
                desktop_theme::contrast(color(preview.background), color(preview.on_background))
                    >= 4.5,
                "{} {variant:?}: unreadable bar",
                preset.id
            );
            assert_eq!(
                lightness(preview.background) < 0.5,
                dark,
                "{} {variant:?}",
                preset.id
            );
            assert!(
                draft
                    .color(variant, ColorRole::Background)
                    .known()
                    .unwrap()
                    .is_some(),
                "{} {variant:?}: missing family surface",
                preset.id
            );
        }
    }
}

#[test]
fn external_mode_change_works_without_widget_process() {
    // Change only COSMIC's native mode file after applying a family. The
    // stored native roles must suffice, including automatic mode's events.
    let (fixture, store, installed) = reported("external-theme-mode", true);
    switch(&store, &installed, Family::Paper);
    let snapshot = fixture.tree();
    for dark in [true, false] {
        fixture.user("com.system76.CosmicTheme.Mode", "auto_switch", "true");
        fixture.user(
            "com.system76.CosmicTheme.Mode",
            "is_dark",
            &dark.to_string(),
        );
        let variant = if dark {
            ThemeVariant::Dark
        } else {
            ThemeVariant::Light
        };
        let preview = AppearanceDraft::load(&store).unwrap().preview(variant);
        assert_eq!(lightness(preview.background) < 0.5, dark);
        assert!(
            AppearanceDraft::load(&store)
                .unwrap()
                .color(variant, ColorRole::Primary)
                .known()
                .unwrap()
                .is_some(),
            "family container exists in {variant:?}"
        );
    }
    for (path, bytes) in snapshot {
        if path.is_file() && !path.to_string_lossy().contains("CosmicTheme.Mode") {
            assert_eq!(fs::read(path).unwrap(), bytes.unwrap());
        }
    }
}

#[test]
fn mode_change_preserves_explicit_surface_overrides() {
    use crate::desktop::appearance::{SystemMode, Value};
    let (fixture, store, installed) = reported("mode-custom-surface", true);
    switch(&store, &installed, Family::Soft);
    let custom = "Color((0.1, 0.2, 0.3))";
    fixture.user("com.system76.CosmicPanel.Dock", "background", custom);
    write_v2(
        &fixture,
        "com.system76.CosmicTheme.Light.Builder",
        "bg_color",
        "Some(\"#EFE0D0FF\")",
    );
    let before = fixture.tree();
    let mut draft = AppearanceDraft::load(&store).unwrap();
    draft.set(Control::Mode, Value::Mode(SystemMode::Light));
    store.apply(&draft.plan().patch).unwrap();
    for (path, bytes) in before {
        if path.is_file()
            && path.starts_with(store.user_root())
            && !path.to_string_lossy().contains("CosmicTheme.Mode")
        {
            assert_eq!(fs::read(path).unwrap(), bytes.unwrap());
        }
    }
}

#[test]
fn legacy_surface_equality_does_not_authorize_mode_migration() {
    let (fixture, store, _installed) = reported("legacy-mode-ownership", true);
    // No current generation marker or transaction proof. Even equal colors
    // might have been entered deliberately outside Halcyon.
    let family = Family::Soft;
    let mut raw: serde_json::Value =
        serde_json::from_slice(&fs::read(store.app_config().unwrap()).unwrap()).unwrap();
    raw["appearance"]["family"] = serde_json::json!(family);
    raw["appearance"]["accent"] = serde_json::Value::Null;
    fs::write(store.app_config().unwrap(), raw.to_string()).unwrap();
    let [r, g, b] = family.surface(family.accent());
    write_v2(
        &fixture,
        "com.system76.CosmicTheme.Dark.Builder",
        "bg_color",
        &format!("Some(\"#{r:02X}{g:02X}{b:02X}FF\")"),
    );
    let (_, cfg) = crate::desktop::wallpaper::automatic::read_config(&store).unwrap();
    let mut draft = AppearanceDraft::load(&store).unwrap();
    linked_colors(&mut draft, cfg.appearance, [200, 90, 30]);
    assert!(
        !draft.dirty(),
        "old equal colors alone do not authorize repair"
    );
}

#[test]
fn mode_change_then_wallpaper_update_then_undo_preserves_current_palette() {
    use crate::desktop::appearance::SystemMode;
    use crate::desktop::wallpaper::{self, BgColor, BgEntry, BgSource, automatic};
    let (fixture, store, installed) = reported("mode-wallpaper-undo", true);
    let set_wallpaper = |rgb| {
        fixture.user(
            wallpaper::NAMESPACE,
            "all",
            &ron::to_string(&BgEntry::new("all", BgSource::Color(BgColor::Single(rgb)))).unwrap(),
        )
    };
    set_wallpaper([0.2, 0.5, 0.7]);
    switch(&store, &installed, Family::Soft);
    automatic::commit(&store, automatic::prepare(&store).unwrap().unwrap()).unwrap();
    let mut mode = AppearanceDraft::load(&store).unwrap();
    mode.set(Control::Mode, Edit::Mode(SystemMode::Light));
    let changed = store
        .apply_owned(
            &mode.plan().patch,
            None,
            crate::desktop::Owner::Appearance,
            "Light mode",
        )
        .unwrap();
    set_wallpaper([0.8, 0.3, 0.2]);
    automatic::commit(&store, automatic::prepare(&store).unwrap().unwrap()).unwrap();
    let (_, before) = automatic::read_config(&store).unwrap();
    let colors = AppearanceDraft::load(&store).unwrap();
    let native: Vec<_> = ThemeVariant::ALL
        .into_iter()
        .map(|v| (v, colors.preview(v).background, colors.preview(v).accent))
        .collect();
    store.undo_latest(&changed.id).unwrap();
    let (_, after) = automatic::read_config(&store).unwrap();
    assert_eq!(after.appearance.accent, before.appearance.accent);
    let restored = AppearanceDraft::load(&store).unwrap();
    assert_eq!(restored.mode().known(), Some(&SystemMode::Dark));
    for (variant, surface, accent) in native {
        assert_eq!(restored.preview(variant).background, surface);
        assert_eq!(restored.preview(variant).accent, accent);
    }
}

#[test]
fn explicit_theme_repair_is_reviewable_preserves_custom_leaves_and_captures_first() {
    let (fixture, store, installed) = reported("mode-repair", true);
    fixture.system(
        "com.system76.CosmicPanel",
        "entries",
        "[\"Dock\",\"Panel\"]",
    );
    fixture.system("com.system76.CosmicPanel.Dock", "anchor", "Bottom");
    fixture.system("com.system76.CosmicPanel.Panel", "anchor", "Top");
    fixture.user(
        "com.system76.CosmicPanel.Dock",
        "background",
        "Color((0.1,0.2,0.3))",
    );
    fixture.user(
        "com.system76.CosmicPanel.Panel",
        "background",
        "Color((0.2,0.3,0.4))",
    );
    let mut raw: serde_json::Value =
        serde_json::from_slice(&fs::read(store.app_config().unwrap()).unwrap()).unwrap();
    raw["appearance"]["family"] = serde_json::json!(Family::Soft);
    raw["appearance"]["overrides"] = serde_json::json!(1u64 << 12);
    fs::write(store.app_config().unwrap(), raw.to_string()).unwrap();
    let before = fixture.tree();
    let repair = repair_matching(&store).unwrap();
    let proposed = plan(
        &store,
        &installed,
        &repair,
        &repair.domains().into_iter().collect(),
    );
    assert_eq!(fixture.tree(), before, "review is read-only");
    assert!(proposed.can_apply(), "{:?}", proposed.blocking_reasons());
    apply(&store, &proposed).unwrap();
    assert_eq!(
        fixture
            .read("com.system76.CosmicPanel.Dock", "background")
            .unwrap(),
        "Color((0.1,0.2,0.3))"
    );
    assert_eq!(
        fixture
            .read("com.system76.CosmicPanel.Panel", "background")
            .unwrap(),
        "ThemeDefault"
    );
    let info = crate::desktop::recovery::load_baseline(&store)
        .unwrap()
        .unwrap();
    let saved = info
        .domains
        .iter()
        .flat_map(|d| &d.native)
        .find(|r| {
            r.supported_key().as_ref()
                == Some(
                    &crate::desktop::ConfigKey::new(
                        "com.system76.CosmicPanel.Panel",
                        1,
                        "background",
                    )
                    .unwrap(),
                )
        })
        .unwrap();
    assert_eq!(saved.contents().unwrap().unwrap(), b"Color((0.2,0.3,0.4))");
    let (_, cfg) = crate::desktop::wallpaper::automatic::read_config(&store).unwrap();
    assert_eq!(cfg.appearance.native_mode_palettes, 1);
    assert_eq!(cfg.appearance.overrides, 1u64 << 12);
}

/// Export real profile planner output to an explicitly selected private
/// scenario directory; no installed or live configuration is consulted.
#[test]
#[ignore = "native scenario fixture export; requires HALCYON_MODE_EVIDENCE"]
fn native_theme_mode_fixtures() {
    let destination = std::path::PathBuf::from(
        std::env::var_os("HALCYON_MODE_EVIDENCE").expect("explicit fixture destination"),
    );
    assert!(destination.is_absolute());
    for preset in crate::desktop::profiles::presets()
        .into_iter()
        .filter(|p| p.id != CLEAN && !p.profile.appearance_only)
    {
        let fixture = Fixture::new("mode-export");
        fixture.system(
            "com.system76.CosmicPanel",
            "entries",
            "[\"Panel\",\"Dock\"]",
        );
        fixture.system("com.system76.CosmicPanel.Dock", "anchor", "Bottom");
        fixture.system("com.system76.CosmicPanel.Panel", "anchor", "Top");
        fixture.user(
            "com.system76.CosmicPanel.Dock",
            "background",
            "Color((0.1,0.2,0.3))",
        );
        fixture.user(
            "com.system76.CosmicPanel.Panel",
            "background",
            "Color((0.1,0.2,0.3))",
        );
        let config = fixture.0.join("config.json");
        fs::write(&config, "{}").unwrap();
        let store = fixture.store().with_app_config(config);
        let installed = Installed::discover(&store);
        let preset = crate::desktop::profiles::fitted(
            preset,
            &crate::config::Config::default(),
            &installed.bundled_dir,
            false,
        );
        let selected = [
            Domain::Dock,
            Domain::Panel,
            Domain::Appearance,
            Domain::WidgetStyle,
        ]
        .into_iter()
        .collect();
        let proposed = plan(&store, &installed, &preset.profile, &selected);
        assert!(proposed.can_apply(), "{:?}", proposed.blocking_reasons());
        apply(&store, &proposed).unwrap();
        let output = destination.join(preset.id);
        for (path, bytes) in fixture.tree() {
            if let Some(bytes) = bytes
                && path.starts_with(store.user_root())
            {
                let to = output
                    .join("cosmic")
                    .join(path.strip_prefix(store.user_root()).unwrap());
                fs::create_dir_all(to.parent().unwrap()).unwrap();
                fs::write(to, bytes).unwrap();
            }
        }
        fs::write(
            output.join("profile.json"),
            serde_json::to_vec_pretty(&preset.profile).unwrap(),
        )
        .unwrap();
        let draft = AppearanceDraft::load(&store).unwrap();
        let rgb = |c: cosmic::cosmic_theme::palette::Srgba| {
            [c.red, c.green, c.blue].map(|v| (v * 255.).round() as u8)
        };
        let expected: serde_json::Value = ThemeVariant::ALL.into_iter().map(|v| {
            let p = draft.preview(v);
            (v.title().to_lowercase(), serde_json::json!({"background":rgb(p.background),"foreground":rgb(p.on_background)}))
        }).collect();
        fs::write(
            output.join("expected.json"),
            serde_json::to_vec_pretty(&expected).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn review_r1_provenance_requires_complete_accepted_native_domains() {
    for excluded in [Domain::Appearance, Domain::Dock, Domain::Panel] {
        let (_fixture, store, installed) =
            reported(&format!("review-r1-exclude-{excluded:?}"), true);
        let p = record(Family::Paper);
        let selected = p.domains().into_iter().filter(|d| *d != excluded).collect();
        let proposed = plan(&store, &installed, &p, &selected);
        assert!(proposed.can_apply(), "{:?}", proposed.blocking_reasons());
        apply(&store, &proposed).unwrap();
        let (_, cfg) = crate::desktop::wallpaper::automatic::read_config(&store).unwrap();
        assert_eq!(
            cfg.appearance.native_mode_palettes, 0,
            "excluded {excluded:?}"
        );
    }
}

#[test]
fn review_r1_widget_scope_keeps_only_existing_unchanged_ownership() {
    let (_fixture, store, installed) = reported("review-r1-existing-proof", true);
    switch(&store, &installed, Family::Soft);
    let (_, cfg) = crate::desktop::wallpaper::automatic::read_config(&store).unwrap();
    assert_eq!(cfg.appearance.native_mode_palettes, 1);
    let mut p = capture(&store, "Same owner").unwrap().profile;
    // A harmless widget change can retain proof for the same native owner.
    p.widget_style.as_mut().unwrap()["appearance"]["opacity"] = json!(80);
    let proposed = plan(
        &store,
        &installed,
        &p,
        &[Domain::WidgetStyle].into_iter().collect(),
    );
    apply(&store, &proposed).unwrap();
    let (_, cfg) = crate::desktop::wallpaper::automatic::read_config(&store).unwrap();
    assert_eq!(cfg.appearance.native_mode_palettes, 1);
    let p = record(Family::Paper);
    let proposed = plan(
        &store,
        &installed,
        &p,
        &[Domain::WidgetStyle].into_iter().collect(),
    );
    apply(&store, &proposed).unwrap();
    let (_, cfg) = crate::desktop::wallpaper::automatic::read_config(&store).unwrap();
    assert_eq!(
        cfg.appearance.native_mode_palettes, 0,
        "new widget family cannot inherit another family's native proof"
    );
}

#[test]
fn review_r1_generation_noop_roles_remain_guarded_until_commit() {
    for changed in ["role", "bar", "owner"] {
        let (fixture, store, installed) =
            reported(&format!("review-r1-proof-guard-{changed}"), true);
        switch(&store, &installed, Family::Soft);
        let mut raw: serde_json::Value =
            serde_json::from_slice(&fs::read(store.app_config().unwrap()).unwrap()).unwrap();
        raw["appearance"]["native_mode_palettes"] = json!(0);
        fs::write(store.app_config().unwrap(), raw.to_string()).unwrap();
        let repair = repair_matching(&store).unwrap();
        let proposed = plan(
            &store,
            &installed,
            &repair,
            &repair.domains().into_iter().collect(),
        );
        assert!(proposed.can_apply());
        match changed {
            "role" => write_v2(
                &fixture,
                "com.system76.CosmicTheme.Light.Builder",
                "bg_color",
                "Some(\"#123456FF\")",
            ),
            "bar" => fixture.user("com.system76.CosmicPanel.Dock", "anchor", "Bottom"),
            _ => {
                raw["appearance"]["family"] = json!(Family::Paper);
                fs::write(store.app_config().unwrap(), raw.to_string()).unwrap();
            }
        }
        let before = fixture.tree();
        assert!(apply(&store, &proposed).is_err(), "changed {changed}");
        assert_eq!(fixture.tree(), before);
        let (_, cfg) = crate::desktop::wallpaper::automatic::read_config(&store).unwrap();
        assert_eq!(cfg.appearance.native_mode_palettes, 0);
    }
}

#[test]
fn applying_frosted_colors_preserves_existing_system_effect_preferences() {
    for enabled in [false, true] {
        let (fixture, store, installed) = reported(&format!("decorative-frosted-{enabled}"), true);
        for variant in ThemeVariant::ALL {
            for surface in appearance::Surface::ALL {
                write_v2(
                    &fixture,
                    variant.builder_namespace(),
                    surface.key(),
                    &enabled.to_string(),
                );
            }
        }
        let mut profile = record(Family::Frosted);
        preserve_custom(&store, &installed, &mut profile).unwrap();
        let planned = plan(
            &store,
            &installed,
            &profile,
            &profile.domains().into_iter().collect(),
        );
        assert!(planned.can_apply(), "{:?}", planned.blocking_reasons());
        apply(&store, &planned).unwrap();
        let draft = AppearanceDraft::load(&store).unwrap();
        for variant in ThemeVariant::ALL {
            for surface in appearance::Surface::ALL {
                assert_eq!(draft.frosted(variant, surface).known(), Some(&enabled));
            }
        }
    }
}

fn legacy_frosted_theme_bytes(on: bool) -> Vec<u8> {
    serde_json::to_vec_pretty(&json!({
        "format": "desktop-cards-profile",
        "version": 7,
        "name": "Legacy colors",
        "appearance_only": true,
        "appearance": {"dark": {
            "accent": "#247a91",
            "frosted": {"panel": false, "applets": on, "system_interface": on, "windows": on}
        }}
    }))
    .unwrap()
}

#[test]
fn legacy_frosted_themes_import_without_weakening_validation() {
    for on in [false, true] {
        let bytes = legacy_frosted_theme_bytes(on);
        let shared = import(&bytes).unwrap();
        let local = import_local(&bytes).unwrap();
        assert_eq!(shared, local);
        assert_eq!(
            local
                .appearance
                .as_ref()
                .unwrap()
                .dark
                .as_ref()
                .unwrap()
                .frosted,
            Some(FrostedLook {
                panel: Some(false),
                applets: Some(on),
                system_interface: Some(on),
                windows: Some(on),
            })
        );
        assert_eq!(import_local(&export_local(&local).unwrap()).unwrap(), local);
        // The historical allowlist still rejects global panel On, unrelated
        // fields and unknown fields. Do not normalize malformed input away.
        for (pointer, value) in [
            ("/appearance/dark/frosted/panel", json!(true)),
            ("/appearance/dark/outline_width", json!(2)),
            ("/appearance/dark/frosted/future_effect", json!(true)),
        ] {
            let mut invalid: Value = serde_json::from_slice(&bytes).unwrap();
            widget::put(&mut invalid, pointer, Some(value)).unwrap();
            let invalid = serde_json::to_vec(&invalid).unwrap();
            assert!(import(&invalid).is_err(), "{pointer}");
            assert!(import_local(&invalid).is_err(), "{pointer}");
        }
    }
}

#[test]
fn legacy_frosted_baseline_load_and_theme_apply_preserve_bytes_and_system_effects() {
    for legacy_on in [false, true] {
        for current_on in [false, true] {
            let (fixture, store, installed) = reported(
                &format!("legacy-frosted-baseline-{legacy_on}-{current_on}"),
                true,
            );
            for variant in ThemeVariant::ALL {
                for surface in appearance::Surface::ALL {
                    write_v2(
                        &fixture,
                        variant.builder_namespace(),
                        surface.key(),
                        &current_on.to_string(),
                    );
                }
            }
            let bytes = legacy_frosted_theme_bytes(legacy_on);
            let library = ProfileLibrary::new(&store);
            let id = ProfileId::parse(BEFORE_THEMES).unwrap();
            let path = store
                .state_dir()
                .join(crate::desktop::store::PROFILES_DIR)
                .join("before-themes.json");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, &bytes).unwrap();
            let legacy = library.load(&id).unwrap();
            assert_eq!(legacy, import(&bytes).unwrap());
            let entries = library.list().unwrap();
            assert_eq!(
                entries
                    .iter()
                    .find(|entry| entry.id == id)
                    .unwrap()
                    .profile
                    .as_ref()
                    .unwrap(),
                &legacy
            );
            assert_eq!(fs::read(&path).unwrap(), bytes);

            // A new built-in must get past retain_before_themes with the old
            // baseline present. Applying that legacy baseline is color-only too.
            for profile in [record(Family::Paper), legacy.clone()] {
                let before = AppearanceDraft::load(&store)
                    .unwrap()
                    .color(ThemeVariant::Dark, ColorRole::Accent)
                    .known()
                    .copied();
                let planned = plan(
                    &store,
                    &installed,
                    &profile,
                    &profile.domains().into_iter().collect(),
                );
                assert!(planned.can_apply(), "{:?}", planned.blocking_reasons());
                assert!(
                    !planned
                        .patch()
                        .iter()
                        .any(|(key, _)| key.namespace().ends_with(".Builder")
                            && key.key().starts_with("frosted"))
                );
                apply(&store, &planned).unwrap();
                let draft = AppearanceDraft::load(&store).unwrap();
                assert_ne!(
                    draft
                        .color(ThemeVariant::Dark, ColorRole::Accent)
                        .known()
                        .copied(),
                    before
                );
                for variant in ThemeVariant::ALL {
                    for surface in appearance::Surface::ALL {
                        assert_eq!(draft.frosted(variant, surface).known(), Some(&current_on));
                    }
                }
                assert_eq!(
                    fs::read(&path).unwrap(),
                    bytes,
                    "baseline bytes must never be rewritten"
                );
                assert_eq!(library.load(&id).unwrap(), legacy);
            }
        }
    }
}

/// Keys whose change makes cosmic-panel rebuild a bar and restart every
/// item on it (its `must_recreate`).
const RESTARTING: [&str; 10] = [
    "size",
    "size_center",
    "size_wings",
    "spacing",
    "border_width",
    "background",
    "plugins_center",
    "plugins_wings",
    "anchor",
    "output",
];

/// Each restarting bar file's inode: a rewrite renames a new file in.
fn bar_files(fixture: &Fixture) -> Vec<(String, u64)> {
    use std::os::unix::fs::MetadataExt;
    let mut found = Vec::new();
    for bar in ["Dock", "Panel"] {
        let dir = fixture
            .0
            .join("config/cosmic")
            .join(format!("com.system76.CosmicPanel.{bar}"))
            .join("v1");
        for key in RESTARTING {
            if let Ok(meta) = fs::metadata(dir.join(key)) {
                found.push((format!("{bar}/{key}"), meta.ino()));
            }
        }
    }
    found
}

/// Plans `family` and applies it unless there is nothing to change; answers
/// the restarting panel keys the plan writes.
fn restyle(store: &DesktopStore, installed: &Installed, family: Family) -> Vec<String> {
    let mut p = record(family);
    preserve_custom(store, installed, &mut p).unwrap();
    let plan = plan(store, installed, &p, &p.domains().into_iter().collect());
    assert!(
        plan.blocking_reasons().is_empty(),
        "{:?}",
        plan.blocking_reasons()
    );
    let keys = plan
        .patch()
        .iter()
        .map(|(key, _)| key)
        .filter(|key| key.namespace().starts_with("com.system76.CosmicPanel."))
        .filter(|key| RESTARTING.contains(&key.key()))
        .map(|key| format!("{}/{}", key.namespace(), key.key()))
        .collect();
    if !plan.is_empty() {
        apply(store, &plan).unwrap();
    }
    keys
}

/// Applying a style again, or switching to one with the same bar spacing,
/// writes no key that restarts the panel, so the island and other panel
/// items keep running.
#[test]
fn reapplying_a_style_or_one_with_the_same_spacing_does_not_restart_the_panel() {
    let (fixture, store, installed) = reported("theme-no-panel-restart", true);
    fixture.system(
        "com.system76.CosmicPanel",
        "entries",
        "[\"Panel\", \"Dock\"]",
    );
    for (bar, anchor, spacing) in [("Panel", "Top", "2"), ("Dock", "Bottom", "4")] {
        let namespace = format!("com.system76.CosmicPanel.{bar}");
        fixture.system(&namespace, "anchor", anchor);
        fixture.system(&namespace, "spacing", spacing);
        fixture.system(&namespace, "size", "M");
        fixture.system(&namespace, "background", "Dark");
    }
    let spacing = |family: Family| family.bar().3;
    let (first, second) = Family::ALL
        .iter()
        .flat_map(|a| Family::ALL.iter().map(move |b| (*a, *b)))
        .find(|(a, b)| a != b && spacing(*a) == spacing(*b))
        .unwrap();
    assert!(!restyle(&store, &installed, first).is_empty());
    let before = bar_files(&fixture);
    assert!(!before.is_empty(), "the first style writes the bars");

    assert_eq!(restyle(&store, &installed, first), Vec::<String>::new());
    assert_eq!(bar_files(&fixture), before, "applying {first:?} twice");

    assert_eq!(restyle(&store, &installed, second), Vec::<String>::new());
    assert_eq!(bar_files(&fixture), before, "{first:?} then {second:?}");
}
