//! Profiles: capture fidelity, round trips, portability, import refusal,
//! missing components, read-only planning, combined apply/Undo and the
//! library. Every test writes only below its own temporary fixture root(s);
//! catalogs and the widget configuration are explicit fixture files.
use super::*;
use crate::desktop::appearance::catalog::{FontCatalog, IconCatalog};
use crate::desktop::appearance::{ColorRole, ThemeVariant};
use crate::desktop::applets::AppletCatalog;
use crate::desktop::store::tests::Fixture;
use crate::desktop::store::{DesktopError, DesktopStore, Owner};
use cosmic::cosmic_config::{Config, CosmicConfigEntry};
use cosmic::cosmic_theme::palette::{Srgb, Srgba};
use cosmic::cosmic_theme::{Roundness, ThemeBuilder};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

const DOCK: &str = "com.system76.CosmicPanel.Dock";
const APPLIST: &str = "com.system76.CosmicAppList";
const SECRETS: [&str; 4] = ["SENTINEL", "/home/", "future_setting", "refresh_intervals"];

#[test]
fn foundation_clock_profile_is_versioned_and_apply_undo_preserves_unowned_fields() {
    let desk = desk(
        "foundation-clock-profile",
        Srgb::new(0.3, 0.4, 0.7),
        Srgb::new(0.2, 0.4, 0.7),
        "teal",
    );
    let before = fs::read(&desk.config).unwrap();
    let mut profile = Profile::new("Paper clock").unwrap();
    profile.widget_style =
        Some(json!({"clock":{"face":"analog","weight":"normal","italic":false,"surface":"none"}}));
    let bytes = export(&profile).unwrap().bytes;
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap()["version"],
        9
    );
    let profile = import(&bytes).unwrap();
    let store = desk.store();
    let proposal = plan(
        &store,
        &desk.installed(),
        &profile,
        &[Domain::WidgetStyle].into(),
    );
    assert!(proposal.can_apply(), "{:?}", proposal.blocking_reasons());
    let change = apply(&store, &proposal).unwrap();
    let after: Value = serde_json::from_slice(&fs::read(&desk.config).unwrap()).unwrap();
    assert_eq!(after["clock"]["face"], "analog");
    let original: Value = serde_json::from_slice(&before).unwrap();
    for key in [
        "location",
        "enabled_widgets",
        "widget_order",
        "refresh_intervals",
    ] {
        assert_eq!(after.get(key), original.get(key), "{key}");
    }
    let captured = capture(&store, "Current").unwrap().profile;
    assert_eq!(captured.written_version(), 9);
    store.undo_latest(&change.id).unwrap();
    assert_eq!(fs::read(&desk.config).unwrap(), before);
    let mut old = profile.clone();
    old.version = 8;
    assert!(
        old.validate().is_err(),
        "old formats cannot carry new clock fields"
    );
}

#[test]
fn foundation_old_setups_leave_clock_overrides_and_explicit_reset_restores_inheritance() {
    let desk = desk(
        "foundation-clock-inheritance",
        Srgb::new(0.3, 0.4, 0.7),
        Srgb::new(0.2, 0.4, 0.7),
        "teal",
    );
    let mut raw: Value = serde_json::from_slice(&fs::read(&desk.config).unwrap()).unwrap();
    raw["clock"] = json!({"face":"analog","weight":"bold","italic":true,"surface":"none"});
    fs::write(&desk.config, raw.to_string()).unwrap();
    let store = desk.store();
    let old=import(br#"{"format":"desktop-cards-profile","version":1,"name":"Date","widget_style":{"clock":{"show_date":false}}}"#).unwrap();
    let proposal = plan(
        &store,
        &desk.installed(),
        &old,
        &[Domain::WidgetStyle].into(),
    );
    apply(&store, &proposal).unwrap();
    let after: Value = serde_json::from_slice(&fs::read(&desk.config).unwrap()).unwrap();
    assert_eq!(after["clock"]["face"], "analog");
    assert_eq!(after["clock"]["show_date"], false);
    let mut reset = Profile::new("Follow style").unwrap();
    reset.widget_style =
        Some(json!({"clock":{"face":null,"weight":null,"italic":null,"surface":null}}));
    let proposal = plan(
        &store,
        &desk.installed(),
        &reset,
        &[Domain::WidgetStyle].into(),
    );
    let change = apply(&store, &proposal).unwrap();
    let after: Value = serde_json::from_slice(&fs::read(&desk.config).unwrap()).unwrap();
    assert!(after["clock"]["face"].is_null());
    assert_eq!(
        capture(&store, "Reset clock")
            .unwrap()
            .profile
            .written_version(),
        9
    );
    store.undo_latest(&change.id).unwrap();
    let after: Value = serde_json::from_slice(&fs::read(&desk.config).unwrap()).unwrap();
    assert_eq!(after["clock"]["face"], "analog");
}

struct Desk {
    fixture: Fixture,
    config: PathBuf,
}

#[test]
fn old_palette_profiles_leave_follow_mode_while_explicit_follow_profiles_keep_it() {
    let desk = desk(
        "profile-widget-color-source",
        Srgb::new(0.3, 0.4, 0.7),
        Srgb::new(0.2, 0.4, 0.7),
        "teal",
    );
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&desk.config).unwrap()).unwrap();
    config["appearance"]["color_source"] = json!("desktop");
    config["appearance"]["accent"] = json!([4, 45, 67]);
    config["appearance"]["match_cosmic_colors"] = json!(true);
    fs::write(&desk.config, config.to_string()).unwrap();
    let store = desk.store();
    let scope = [Domain::WidgetStyle].into();
    let legacy = import(br#"{"format":"desktop-cards-profile","version":1,"name":"Older palette","widget_style":{"theme":"rose"}}"#).unwrap();
    let proposal = plan(&store, &desk.installed(), &legacy, &scope);
    assert!(
        proposal.patch().is_empty(),
        "widget style never changes desktop keys"
    );
    assert!(proposal.can_apply(), "{:?}", proposal.blocking_reasons());
    let change = apply(&store, &proposal).unwrap();
    let saved: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&desk.config).unwrap()).unwrap();
    assert_eq!(saved["appearance"]["color_source"], "custom");
    assert!(saved["appearance"].get("accent").is_none());
    assert_eq!(saved["theme"], "rose");
    assert_eq!(saved["appearance"]["match_cosmic_colors"], true);
    store.undo_latest(&change.id).unwrap();
    let restored: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&desk.config).unwrap()).unwrap();
    assert_eq!(restored["appearance"]["color_source"], "desktop");
    assert_eq!(restored["appearance"]["accent"], json!([4, 45, 67]));
    let explicit = import(br#"{"format":"desktop-cards-profile","version":1,"name":"Follow colors","widget_style":{"theme":"rose","appearance":{"color_source":"desktop"}}}"#).unwrap();
    let proposal = plan(&store, &desk.installed(), &explicit, &scope);
    assert!(proposal.can_apply(), "{:?}", proposal.blocking_reasons());
    apply(&store, &proposal).unwrap();
    let saved: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&desk.config).unwrap()).unwrap();
    assert_eq!(saved["appearance"]["color_source"], "desktop");
}

/// A desktop whose Light and Dark themes differ (accent, custom surface,
/// corners), with a Dock, a Panel, a toolkit font/icon theme and a widget
/// configuration that also holds private and unknown members.
fn desk(name: &str, accent_dark: Srgb, accent_light: Srgb, theme: &str) -> Desk {
    let fixture = Fixture::new(name);
    fixture.system(
        "com.system76.CosmicPanel",
        "entries",
        "[\"Panel\",\"Dock\"]",
    );
    fixture.system(DOCK, "anchor", "Bottom");
    fixture.system(DOCK, "size", "L");
    fixture.system(DOCK, "output", "Name(\"DP-1\")");
    fixture.system(DOCK, "plugins_center", &format!("Some([\"{APPLIST}\"])"));
    fixture.system("com.system76.CosmicPanel.Panel", "anchor", "Top");
    let root = fixture.0.join("config");
    for (variant, builder, accent) in [
        (ThemeVariant::Dark, ThemeBuilder::dark(), accent_dark),
        (ThemeVariant::Light, ThemeBuilder::light(), accent_light),
    ] {
        let mut builder = builder.accent(accent);
        if variant == ThemeVariant::Light {
            builder.bg_color = Some(Srgba::new(0.9, 0.88, 0.85, 1.));
            builder.corner_radii = Roundness::SlightlyRound.into();
        }
        let config =
            Config::with_custom_path(variant.builder_namespace(), 2, root.clone()).unwrap();
        builder.write_entry(&config).unwrap();
        let theme = builder.build();
        let config = Config::with_custom_path(variant.theme_namespace(), 2, root.clone()).unwrap();
        theme.write_entry(&config).unwrap();
    }
    // cosmic-settings may store a surface as exact components, not hex.
    let exact = Some(cosmic::cosmic_theme::color::ColorRepr::Rgba(Srgba::new(
        0.123, 0.456, 0.789, 0.5,
    )));
    fs::write(
        root.join("cosmic/com.system76.CosmicTheme.Light.Builder/v2/primary_container_bg"),
        crate::desktop::store::RawValue::encode(&exact)
            .unwrap()
            .as_str(),
    )
    .unwrap();
    fixture.user("com.system76.CosmicTheme.Mode", "is_dark", "true");
    let mut font = cosmic::config::CosmicTk::default().interface_font;
    font.family = "Fira Sans".into();
    fixture.user(
        "com.system76.CosmicTk",
        "interface_font",
        crate::desktop::store::RawValue::encode(&font)
            .unwrap()
            .as_str(),
    );
    fixture.user("com.system76.CosmicTk", "icon_theme", "\"Cosmic\"");
    let config = fixture.0.join("app/config.json");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(
        &config,
        serde_json::to_string_pretty(&json!({
            "theme": theme,
            "appearance": {"opacity": 88, "match_wallpaper": false},
            "enabled_widgets": ["clock", "devbox"],
            "widget_positions": {"clock": [0.1, 0.2]},
            "location": {"name": "SENTINEL-CITY", "latitude": 1.0, "longitude": 2.0},
            "project_roots": ["/home/sentinel/Projects"],
            "refresh_intervals": {"weather": 600},
            "future_setting": {"keep": true}
        }))
        .unwrap(),
    )
    .unwrap();
    Desk { fixture, config }
}

impl Desk {
    fn store(&self) -> DesktopStore {
        self.fixture.store().with_app_config(self.config.clone())
    }
    fn installed(&self) -> Installed {
        let icons = self.fixture.0.join("icons");
        fs::create_dir_all(icons.join("Cosmic")).unwrap();
        fs::write(
            icons.join("Cosmic/index.theme"),
            "[Icon Theme]\nName=Cosmic\nDirectories=scalable\n",
        )
        .unwrap();
        let applications = self.fixture.0.join("applications");
        fs::create_dir_all(&applications).unwrap();
        fs::write(
            applications.join(format!("{APPLIST}.desktop")),
            "[Desktop Entry]\nName=App List\nX-CosmicApplet=true\n",
        )
        .unwrap();
        Installed::new(
            FontCatalog::from_manifest(
                "interface\tFira Sans\t400\nmonospace\tNoto Sans Mono\t400\n",
            ),
            IconCatalog::discover(&[icons], &[]),
            AppletCatalog::discover(&[applications]),
        )
    }
    fn json(&self) -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(&self.config).unwrap()).unwrap()
    }
    /// The COSMIC user layer and the widget file, byte for byte.
    fn settings(&self) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
        let mut tree = self.fixture.tree();
        tree.retain(|path, _| {
            path.starts_with(self.fixture.0.join("config")) || *path == self.config
        });
        tree
    }
}

fn all() -> BTreeSet<Domain> {
    Domain::ALL.into_iter().collect()
}

#[test]
fn capture_keeps_distinct_light_and_dark_values_and_round_trips_exactly() {
    let desk = desk(
        "profile-capture",
        Srgb::new(0.2, 0.6, 0.4),
        Srgb::new(0.8, 0.3, 0.2),
        "teal",
    );
    let installed = desk.installed();
    let before = desk.fixture.tree();
    let captured = capture(&desk.store(), "My desk").unwrap();
    assert_eq!(desk.fixture.tree(), before, "capture writes nothing");
    let profile = captured.profile;
    let appearance = profile.appearance.as_ref().unwrap();
    let light = appearance.light.as_ref().unwrap();
    let dark = appearance.dark.as_ref().unwrap();
    assert_ne!(
        light.accent, dark.accent,
        "variants are captured independently"
    );
    assert_eq!(
        light.corners,
        Some(Corners::Preset(Roundness::SlightlyRound))
    );
    assert_eq!(dark.corners, Some(Corners::Preset(Roundness::Round)));
    assert_ne!(
        light.background,
        Some(ColorChoice::Auto),
        "custom role kept"
    );
    // A surface that is not 8-bit exact keeps its exact components.
    assert!(matches!(light.primary, Some(ColorChoice::Exact(_))));
    assert_eq!(
        appearance.interface_font.as_ref().unwrap().family,
        "Fira Sans"
    );
    let dock = profile.dock.as_ref().unwrap();
    assert_eq!(dock.anchor, Some(crate::desktop::panels::Anchor::Bottom));
    assert_eq!(
        dock.applets.as_ref().unwrap().center,
        vec![APPLIST.to_string()]
    );
    assert_eq!(profile.widget_style.as_ref().unwrap()["theme"], "teal");
    assert_eq!(
        profile.widget_layout.as_ref().unwrap()["widget_positions"]["clock"],
        json!([0.1, 0.2])
    );
    assert!(profile.widget_layout.as_ref().unwrap()["widget_positions"]["weather"].is_null());

    // Export → import is lossless for everything portable; this desktop's
    // own wallpaper (here the fallback, outside the fixture's system
    // backgrounds) and arrangement stay behind and are named.
    let exported = export(&profile).unwrap();
    let text = String::from_utf8(exported.bytes.clone()).unwrap();
    for secret in
        SECRETS
            .iter()
            .chain(&["DP-1", "\"output\"", "match_wallpaper", "location", "orion"])
    {
        assert!(!text.contains(secret), "export must not contain {secret}");
    }
    assert_eq!(exported.omitted, [OWN_WALLPAPER, DISPLAY_ARRANGEMENT]);
    assert_eq!(import(&exported.bytes).unwrap(), profile.portable().0);
    assert_eq!(
        import_local(&export_local(&profile).unwrap()).unwrap(),
        profile
    );

    // Applying a capture to the desktop it came from changes nothing.
    let plan = plan(&desk.store(), &installed, &profile, &all());
    assert!(
        plan.blocking_reasons().is_empty(),
        "{:?}",
        plan.blocking_reasons()
    );
    assert!(plan.is_empty(), "{:?}", plan.domains);
    assert_eq!(desk.fixture.tree(), before, "planning writes nothing");
}

#[test]
fn applying_a_capture_reproduces_it_elsewhere_and_restart_undo_restores_the_target() {
    let source = desk(
        "profile-source",
        Srgb::new(0.2, 0.6, 0.4),
        Srgb::new(0.8, 0.3, 0.2),
        "rose",
    );
    let profile = capture(&source.store(), "Source").unwrap().profile;
    let target = desk(
        "profile-target",
        Srgb::new(0.1, 0.1, 0.9),
        Srgb::new(0.1, 0.1, 0.9),
        "teal",
    );
    target.fixture.user(DOCK, "anchor", "Left");
    target.fixture.user(DOCK, "output", "Name(\"HDMI-A-1\")");
    // The target's light theme has no custom background.
    let light_bg = target
        .fixture
        .0
        .join("config/cosmic/com.system76.CosmicTheme.Light.Builder/v2/bg_color");
    fs::write(&light_bg, "None").unwrap();
    let before = target.settings();
    let installed = target.installed();
    let plan = plan(&target.store(), &installed, &profile, &all());
    assert!(plan.can_apply(), "{:?}", plan.blocking_reasons());
    assert!(
        plan.domains
            .iter()
            .any(|d| d.domain == Domain::Appearance && !d.changes.is_empty())
    );
    assert_eq!(target.settings(), before, "planning writes nothing");
    let latest = apply(&target.store(), &plan).unwrap();
    assert_eq!(latest.owner, Owner::Profile);
    assert_eq!(latest.label, "Profile “Source”");

    // The target now captures like the source in every included section,
    // keeping its own monitor binding and private widget data.
    let recaptured = capture(&target.store(), "Source").unwrap().profile;
    assert_eq!(recaptured.appearance, profile.appearance);
    assert_eq!(recaptured.dock, profile.dock);
    assert_eq!(recaptured.widget_style, profile.widget_style);
    assert_eq!(recaptured.widget_layout, profile.widget_layout);
    assert_eq!(
        target.fixture.read(DOCK, "output").as_deref(),
        Some("Name(\"HDMI-A-1\")")
    );
    let json = target.json();
    assert_eq!(json["location"]["name"], "SENTINEL-CITY");
    assert_eq!(json["project_roots"], json!(["/home/sentinel/Projects"]));
    assert_eq!(json["refresh_intervals"]["weather"], 600);
    assert_eq!(
        json["appearance"]["match_wallpaper"], false,
        "palette links preserved"
    );
    assert_eq!(json["future_setting"], json!({"keep": true}));

    // After a restart, Undo restores the target exactly.
    let restarted = target.store();
    let saved = restarted.latest_change().unwrap().unwrap();
    assert_eq!(saved.id, latest.id);
    restarted.undo_latest(&saved.id).unwrap();
    assert_eq!(target.settings(), before);
}

#[test]
fn only_selected_domains_are_planned_and_missing_sections_block() {
    let desk = desk(
        "profile-domains",
        Srgb::new(0.2, 0.6, 0.4),
        Srgb::new(0.8, 0.3, 0.2),
        "teal",
    );
    let mut profile = Profile::new("Widgets only").unwrap();
    profile.widget_style = Some(json!({"theme": "amber", "clock": {"format_24h": false}}));
    let selected: BTreeSet<Domain> = [Domain::WidgetStyle].into();
    let plan = plan(&desk.store(), &desk.installed(), &profile, &selected);
    assert!(plan.can_apply());
    assert!(
        plan.patch().is_empty(),
        "no COSMIC key in a widget-only plan"
    );
    assert_eq!(plan.domains.len(), 1);
    assert_eq!(
        plan.domains[0].changes,
        vec!["Card palette", "Clock 24-hour time"]
    );

    // An included domain the profile lacks blocks, and Apply writes nothing.
    let with_dock: BTreeSet<Domain> = [Domain::WidgetStyle, Domain::Dock].into();
    let blocked = super::plan(&desk.store(), &desk.installed(), &profile, &with_dock);
    assert!(!blocked.can_apply());
    let before = desk.fixture.tree();
    assert!(apply(&desk.store(), &blocked).is_err());
    assert_eq!(desk.fixture.tree(), before);

    // A Dock that isn't set up on the target blocks its domain too.
    let bare = Fixture::new("profile-no-dock");
    let store = bare.store();
    let mut dock_profile = Profile::new("Dock").unwrap();
    dock_profile.dock = Some(PanelLook {
        anchor: Some(crate::desktop::panels::Anchor::Left),
        ..Default::default()
    });
    let plan = super::plan(
        &store,
        &Installed::default(),
        &dock_profile,
        &[Domain::Dock].into(),
    );
    assert!(
        plan.domains[0].blocked[0].contains("isn’t set up"),
        "{:?}",
        plan.domains
    );
}

#[test]
fn missing_fonts_icons_and_applets_are_reported_kept_in_the_record_and_not_written() {
    let desk = desk(
        "profile-missing",
        Srgb::new(0.2, 0.6, 0.4),
        Srgb::new(0.8, 0.3, 0.2),
        "teal",
    );
    let mut profile = Profile::new("Elsewhere").unwrap();
    let mut font = cosmic::config::CosmicTk::default().interface_font;
    font.family = "Not Installed Sans".into();
    profile.appearance = Some(AppearanceLook {
        interface_font: Some(font),
        icon_theme: Some("Missing-Icons".into()),
        ..Default::default()
    });
    profile.dock = Some(PanelLook {
        applets: Some(Applets {
            start: vec!["org.example.Gone".into()],
            center: vec![APPLIST.into()],
            end: Vec::new(),
        }),
        ..Default::default()
    });
    let plan = plan(
        &desk.store(),
        &desk.installed(),
        &profile,
        &[Domain::Appearance, Domain::Dock].into(),
    );
    let warnings: Vec<&String> = plan.domains.iter().flat_map(|d| &d.warnings).collect();
    assert!(
        warnings.iter().any(|w| w.contains("Not Installed Sans")),
        "{warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| w.contains("Missing-Icons")),
        "{warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| w.contains("org.example.Gone")),
        "{warnings:?}"
    );
    assert!(
        plan.patch()
            .keys()
            .all(|key| key.key() != "interface_font" && key.key() != "icon_theme"),
        "missing components are never written"
    );
    let wings = plan
        .patch()
        .iter()
        .find(|(key, _)| key.key() == "plugins_wings")
        .map(|(_, entry)| entry.desired.clone().unwrap());
    assert!(wings.is_none_or(|value| !value.as_str().contains("Gone")));
    // The record keeps the identifiers for a computer that has them.
    let again = import(&export(&profile).unwrap().bytes).unwrap();
    assert_eq!(
        again.dock.unwrap().applets.unwrap().start,
        vec!["org.example.Gone".to_string()]
    );
}

#[test]
fn applying_never_touches_excluded_or_unknown_widget_settings() {
    let desk = desk(
        "profile-private",
        Srgb::new(0.2, 0.6, 0.4),
        Srgb::new(0.8, 0.3, 0.2),
        "teal",
    );
    let mut profile = Profile::new("Layout").unwrap();
    profile.widget_layout = Some(json!({
        "enabled_widgets": ["clock", "weather"],
        "widget_positions": {"clock": null, "weather": [0.7, 0.1]},
        "glance": {"enabled": true, "initialized": true, "show_weather": false}
    }));
    let before = desk.json();
    let plan = plan(
        &desk.store(),
        &desk.installed(),
        &profile,
        &[Domain::WidgetLayout].into(),
    );
    apply(&desk.store(), &plan).unwrap();
    let after = desk.json();
    for member in [
        "location",
        "project_roots",
        "refresh_intervals",
        "future_setting",
        "theme",
        "appearance",
    ] {
        assert_eq!(after[member], before[member], "{member} untouched");
    }
    assert!(
        after["widget_positions"].get("clock").is_none(),
        "null restores the default place"
    );
    assert_eq!(after["widget_positions"]["weather"], json!([0.7, 0.1]));
    assert_eq!(after["glance"]["show_weather"], false);
}

#[test]
fn import_refuses_future_unknown_malformed_oversized_and_recovery_records() {
    let base = json!({"format": "desktop-cards-profile", "version": 1, "name": "Ok"});
    assert!(import(serde_json::to_string(&base).unwrap().as_bytes()).is_ok());
    let with = |patch: serde_json::Value| {
        let mut value = base.clone();
        for (key, member) in patch.as_object().unwrap() {
            value[key] = member.clone();
        }
        serde_json::to_vec(&value).unwrap()
    };
    let cases: Vec<(&str, Vec<u8>, bool)> = vec![
        ("future version", with(json!({"version": VERSION + 1})), true),
        ("unknown section", with(json!({"screensaver": {"path": "/etc/passwd"}})), true),
        ("later section", with(json!({"wallpaper": {"path": "/etc/passwd"}})), false),
        ("hook", with(json!({"post_apply": "rm -rf ~"})), true),
        ("monitor binding", with(json!({"dock": {"output": {"Name": "DP-1"}}})), false),
        ("private widget data", with(json!({"widget_style": {"location": {"name": "x"}}})), false),
        ("layout member in style", with(json!({"widget_style": {"enabled_widgets": []}})), false),
        ("bad value", with(json!({"widget_style": {"appearance": {"opacity": 5}}})), false),
        ("bad color", with(json!({"appearance": {"dark": {"accent": "#zzzzzz"}}})), false),
        ("blur step", with(json!({"appearance": {"light": {"blur": 40}}})), false),
        ("path-like icon theme", with(json!({"appearance": {"icon_theme": "../../x"}})), false),
        ("huge radius", with(json!({"dock": {"border_radius": 100000}})), false),
        ("name with line break", with(json!({"name": "a\nb"})), false),
        ("empty name", with(json!({"name": ""})), false),
        ("not json", b"\x00\x01".to_vec(), false),
        ("journal", br#"{"format":"desktop-cards-journal","version":1,"id":"x","operation":"apply","created":"","entries":[]}"#.to_vec(), false),
        ("undo record", br#"{"format":"desktop-cards-undo","version":1}"#.to_vec(), false),
        ("oversized", vec![b' '; MAX_PROFILE_BYTES + 1], false),
    ];
    for (name, bytes, unsupported) in cases {
        match import(&bytes) {
            Err(ProfileError::Unsupported(_)) if unsupported => {}
            Err(ProfileError::Invalid(_)) if !unsupported => {}
            other => panic!("{name}: {other:?}"),
        }
    }
}

#[test]
fn library_stores_named_profiles_by_generated_id_and_loading_creates_nothing() {
    let fixture = Fixture::new("profile-library");
    let store = fixture.store();
    let library = ProfileLibrary::new(&store);
    let before = fixture.tree();
    assert_eq!(library.list().unwrap(), Vec::new());
    assert!(matches!(
        library.load(&ProfileId::parse("p1").unwrap()),
        Err(ProfileError::NotFound(_))
    ));
    assert_eq!(fixture.tree(), before, "discovery creates nothing");

    let profile = Profile::new("Desk").unwrap();
    let first = library.create(&profile).unwrap();
    let second = library.create(&profile).unwrap();
    assert_ne!(first, second, "equal names never share a file");
    let dir = fixture.0.join("state/profiles");
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let file = dir.join(format!("{}.json", first.as_str()));
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );

    library.rename(&second, "Another").unwrap();
    let copy = library.duplicate(&first, "Desk copy").unwrap();
    let names: Vec<String> = library
        .list()
        .unwrap()
        .into_iter()
        .map(|entry| entry.profile.unwrap().name)
        .collect();
    assert_eq!(names, ["Another", "Desk", "Desk copy"]);
    library.delete(&first).unwrap();
    assert_eq!(library.list().unwrap().len(), 2);
    assert_eq!(library.load(&copy).unwrap().name, "Desk copy");
    assert!(library.rename(&copy, "bad\nname").is_err());
    // A damaged file is reported in place, not dropped.
    fs::write(dir.join("pbroken.json"), "{").unwrap();
    assert!(
        library
            .list()
            .unwrap()
            .iter()
            .any(|entry| entry.profile.is_err())
    );
    // Only plain generated-style IDs address files.
    for bad in ["../x", "a/b", ".hidden", "", "UPPER", "-x", "x.json"] {
        assert!(ProfileId::parse(bad).is_err(), "{bad}");
    }
    // A read-only store lists but never writes.
    let read_only = fixture.store().read_only("Preview");
    let tree = fixture.tree();
    assert!(ProfileLibrary::new(&read_only).create(&profile).is_err());
    assert_eq!(fixture.tree(), tree);
}

#[test]
fn library_escapes_through_symlinks_are_refused_before_anything_is_created() {
    use std::os::unix::fs::symlink;
    let profile = Profile::new("Desk").unwrap();
    // The state folder, or the library folder, leads into a second root.
    for (name, link) in [("state", "state"), ("library", "state/profiles")] {
        let fixture = Fixture::new("profile-escape");
        let outside = Fixture::new("profile-escape-outside");
        let path = fixture.0.join(link);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        symlink(&outside.0, &path).unwrap();
        let store = fixture.store();
        let library = ProfileLibrary::new(&store);
        let inside = fixture.tree();
        assert!(library.create(&profile).is_err(), "{name}");
        assert!(library.list().is_err() || name == "state", "{name}");
        assert_eq!(
            fs::read_dir(&outside.0).unwrap().count(),
            0,
            "{name}: nothing outside"
        );
        assert_eq!(fixture.tree(), inside, "{name}: nothing inside");
    }
}

#[test]
fn a_stale_plan_conflicts_instead_of_overwriting_newer_changes() {
    let desk = desk(
        "profile-stale",
        Srgb::new(0.2, 0.6, 0.4),
        Srgb::new(0.8, 0.3, 0.2),
        "teal",
    );
    let mut profile = Profile::new("Stale").unwrap();
    profile.widget_style = Some(json!({"theme": "amber"}));
    profile.dock = Some(PanelLook {
        anchor: Some(crate::desktop::panels::Anchor::Top),
        ..Default::default()
    });
    let plan = plan(
        &desk.store(),
        &desk.installed(),
        &profile,
        &[Domain::Dock, Domain::WidgetStyle].into(),
    );
    // The widget autosave changes the palette after the preview.
    let mut value = desk.json();
    value["theme"] = json!("graphite");
    fs::write(&desk.config, serde_json::to_string(&value).unwrap()).unwrap();
    let before = desk.settings();
    match apply(&desk.store(), &plan) {
        Err(DesktopError::FieldConflict { fields, .. }) => assert_eq!(fields, ["Card palette"]),
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert_eq!(desk.settings(), before, "nothing written");
}

#[test]
fn intent_values_become_exact_per_variant_values() {
    let look = VariantLook {
        accent: Some(ColorChoice::Hex("#E66100".into())),
        corners: Some(Corners::Preset(Roundness::Square)),
        glass: Some(Glass::Percent(40)),
        spacing: Some(SpacingChoice::Preset(
            cosmic::cosmic_theme::Density::Compact,
        )),
        ..Default::default()
    };
    let values = look.values(ThemeVariant::Dark).unwrap();
    assert_eq!(
        values.radii,
        Some(cosmic::cosmic_theme::CornerRadii::from(Roundness::Square))
    );
    assert_eq!(
        values.alpha,
        Some(crate::desktop::appearance::glass_alpha_map(40))
    );
    assert_eq!(
        values.colors[&ColorRole::Accent],
        crate::desktop::appearance::parse_hex("#E66100", false)
    );
    // And an exact capture of them reads back as the same intents.
    assert_eq!(VariantLook::of(&values).corners, look.corners);
    assert_eq!(VariantLook::of(&values).glass, look.glass);
}

/// Version 2 adds Windows, Workspaces and per-variant gaps; version 1
/// records (presets, older exports and library copies) stay readable as
/// they are and gain no new defaults. Capture → export → import is exact,
/// only selected domains apply, the transition rule holds for profiles,
/// and each new owner's Undo survives a restart.
#[test]
fn windows_workspaces_profile_roundtrip_and_legacy_import() {
    use crate::desktop::windows::{Behavior, TRANSITION};
    const COMP: &str = "com.system76.CosmicComp";
    let source = desk(
        "profile-windows",
        Srgb::new(0.2, 0.6, 0.4),
        Srgb::new(0.8, 0.3, 0.2),
        "teal",
    );
    source.fixture.user(COMP, "autotile", "true");
    source
        .fixture
        .user(COMP, "autotile_behavior", "PerWorkspace");
    source
        .fixture
        .user(COMP, "focus_follows_cursor_delay", "400");
    source.fixture.user(
        COMP,
        "workspaces",
        "(workspace_mode:Global,workspace_layout:Horizontal,action_on_typing:OpenLauncher,workspace_wraparound:false)",
    );
    source.fixture.user(
        COMP,
        "xkb_config",
        "(rules:\"\",model:\"\",layout:\"SENTINEL\",variant:\"\",options:None)",
    );
    let store = source.store();
    let captured = capture(&store, "Tiled").unwrap();
    let profile = captured.profile;
    // A capture carries the version it is stored as: windows need 2.
    assert_eq!(profile.version, profile.written_version());
    assert!(profile.version >= VERSION_2 && profile.version <= VERSION);
    let windows = profile.windows.as_ref().unwrap();
    assert_eq!(windows.autotile, Some(true));
    assert_eq!(windows.autotile_behavior, Some(TilingScope::PerWorkspace));
    assert_eq!(windows.focus_follows_cursor_delay, Some(400));
    assert_eq!(windows.show_minimize, Some(true), "the pinned default");
    let workspaces = profile.workspaces.as_ref().unwrap();
    assert_eq!(workspaces.workspace_mode, Some(WorkspaceModeName::Global));
    assert_eq!(
        workspaces.action_on_typing,
        Some(TypingActionName::OpenLauncher)
    );
    assert_eq!(workspaces.workspace_wraparound, Some(false));
    let light = profile.appearance.as_ref().unwrap().light.as_ref().unwrap();
    assert!(light.gaps.is_some(), "gaps live under Appearance");
    let bytes = export(&profile).unwrap().bytes;
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(
        text.contains("\"version\": 5"),
        "captured automatic widget sizes require version 5"
    );
    assert!(
        text.contains("\"autotile_behavior\": \"per_workspace\""),
        "{text}"
    );
    for private in ["SENTINEL", "xkb", "pinned", "\"output\""] {
        assert!(!text.contains(private), "{private}");
    }
    assert_eq!(import(&bytes).unwrap(), profile.portable().0);
    // The capture applied where it came from changes nothing.
    let same = plan(&store, &source.installed(), &profile, &all());
    assert!(
        same.is_empty() && same.blocking_reasons().is_empty(),
        "{:?}",
        same.domains
    );

    // Version 1: readable as it is, no new members allowed, never migrated
    // on read; its export is version 2 with the same members.
    let old = br#"{"format":"desktop-cards-profile","version":1,"name":"Old","dock":{"anchor":"Left"},"appearance":{"dark":{"outline_width":2}}}"#;
    let legacy = import(old).unwrap();
    assert_eq!(legacy.version, LEGACY_VERSION);
    assert_eq!(legacy.domains(), [Domain::Dock, Domain::Appearance]);
    let reexported = import(&export(&legacy).unwrap().bytes).unwrap();
    assert_eq!(reexported.version, VERSION_2, "no version 3 sections");
    assert_eq!(
        (reexported.dock, reexported.appearance),
        (legacy.dock.clone(), legacy.appearance.clone())
    );
    for (name, record, unsupported) in [
        (
            "v1 windows",
            r#"{"format":"desktop-cards-profile","version":1,"name":"X","windows":{"autotile":true}}"#,
            false,
        ),
        (
            "v1 workspaces",
            r#"{"format":"desktop-cards-profile","version":1,"name":"X","workspaces":{}}"#,
            false,
        ),
        (
            "v1 gaps",
            r#"{"format":"desktop-cards-profile","version":1,"name":"X","appearance":{"light":{"gaps":[1,2]}}}"#,
            false,
        ),
        (
            "v2 unknown section",
            r#"{"format":"desktop-cards-profile","version":2,"name":"X","shortcuts":{}}"#,
            true,
        ),
        (
            "v2 unknown member",
            r#"{"format":"desktop-cards-profile","version":2,"name":"X","windows":{"tiling_rules":[]}}"#,
            false,
        ),
        (
            "v2 unknown variant",
            r#"{"format":"desktop-cards-profile","version":2,"name":"X","workspaces":{"workspace_mode":"diagonal"}}"#,
            false,
        ),
        (
            "v2 delay range",
            r#"{"format":"desktop-cards-profile","version":2,"name":"X","windows":{"focus_follows_cursor_delay":999999}}"#,
            false,
        ),
        (
            "v2 gap range",
            r#"{"format":"desktop-cards-profile","version":2,"name":"X","appearance":{"dark":{"gaps":[501,8]}}}"#,
            false,
        ),
        (
            "future version",
            r#"{"format":"desktop-cards-profile","version":999,"name":"X"}"#,
            true,
        ),
    ] {
        match import(record.as_bytes()) {
            Err(ProfileError::Unsupported(_)) if unsupported => {}
            Err(ProfileError::Invalid(_)) if !unsupported => {}
            other => panic!("{name}: {other:?}"),
        }
    }

    // Starter looks are current records with a wallpaper; the Clean desktop
    // switch is a version 4 record that also hides the desktop tools. None
    // of them touches window behavior or workspaces.
    let fresh = desk(
        "profile-presets",
        Srgb::new(0.2, 0.6, 0.4),
        Srgb::new(0.8, 0.3, 0.2),
        "teal",
    );
    for preset in presets() {
        let expected = if preset.id == "presentation" {
            format::VERSION_4
        } else if matches!(preset.id, "tidal" | "paper-studio" | "moss" | "carbon") {
            format::VERSION_9
        } else {
            format::VERSION_7
        };
        assert_eq!(preset.profile.version, expected, "{}", preset.id);
        assert!(preset.profile.windows.is_none() && preset.profile.workspaces.is_none());
        let planned = plan(&fresh.store(), &fresh.installed(), &preset.profile, &all());
        let keys: Vec<_> = planned.patch().keys().collect();
        assert!(
            keys.iter()
                .all(|key| key.namespace() != COMP || key.key() == "appearance_settings"),
            "{}: {keys:?}",
            preset.id,
        );
        assert!(
            keys.iter().all(|key| key.key() != "gaps")
                || keys.iter().any(|key| key.key() == "active_hint"),
            "{}: gaps only follow an outline change",
            preset.id
        );
    }

    // Selected domains only; Undo after a restart, for the profile and for
    // each new page owner.
    let target = desk(
        "profile-windows-target",
        Srgb::new(0.1, 0.1, 0.9),
        Srgb::new(0.1, 0.1, 0.9),
        "teal",
    );
    target
        .fixture
        .user(COMP, "autotile_behavior", "PerWorkspace");
    let before = target.settings();
    let selected: BTreeSet<Domain> = [Domain::Windows, Domain::Workspaces].into();
    let chosen = plan(&target.store(), &target.installed(), &profile, &selected);
    assert!(chosen.can_apply(), "{:?}", chosen.blocking_reasons());
    assert!(
        chosen.patch().keys().all(|key| key.namespace() == COMP),
        "{:?}",
        chosen.patch().keys().collect::<Vec<_>>()
    );
    let latest = apply(&target.store(), &chosen).unwrap();
    assert_eq!(
        target.fixture.read(COMP, "autotile").as_deref(),
        Some("true")
    );
    assert!(
        target
            .fixture
            .read(COMP, "workspaces")
            .unwrap()
            .contains("Global")
    );
    assert!(latest.change.entries().iter().all(|entry| {
        Behavior::of(&entry.key).is_some() || entry.key == crate::desktop::workspaces::key()
    }));
    let restarted = target.store();
    restarted
        .undo_latest(&restarted.latest_change().unwrap().unwrap().id)
        .unwrap();
    assert_eq!(target.settings(), before);
    for owner in [Owner::Windows, Owner::Workspaces] {
        assert_eq!(Owner::parse(owner.key()), Some(owner));
        let mut patch = crate::desktop::store::DesktopPatch::new();
        let key = if owner == Owner::Windows {
            Behavior::ActiveHint.key()
        } else {
            crate::desktop::workspaces::key()
        };
        let value = if owner == Owner::Windows {
            "false"
        } else {
            "(workspace_mode:Global)"
        };
        patch.set(
            key,
            None,
            crate::desktop::store::RawValue::parse(value).ok(),
        );
        let change = target
            .store()
            .apply_owned(&patch, None, owner, owner.key())
            .unwrap();
        let restarted = target.store();
        let saved = restarted.latest_change().unwrap().unwrap();
        assert_eq!(
            (saved.owner, saved.id.as_str()),
            (owner, change.id.as_str())
        );
        restarted.undo_latest(&saved.id).unwrap();
        assert_eq!(target.settings(), before, "{owner:?}");
    }

    // Tiling and its scope changing together is refused for profiles too.
    let flip = desk(
        "profile-windows-flip",
        Srgb::new(0.1, 0.1, 0.9),
        Srgb::new(0.1, 0.1, 0.9),
        "teal",
    );
    let refused = plan(
        &flip.store(),
        &flip.installed(),
        &profile,
        &[Domain::Windows].into(),
    );
    let part = refused
        .domains
        .iter()
        .find(|d| d.domain == Domain::Windows)
        .unwrap();
    assert!(!refused.can_apply());
    // Toward PerWorkspace the page applies the scope first, so Profiles
    // send people there for it; toward Global it is tiling.
    assert!(
        part.blocked
            .iter()
            .any(|reason: &String| reason
                .contains("Change the tiling scope on the Windows page first")),
        "{:?}",
        part.blocked
    );
    assert!(
        !part
            .blocked
            .iter()
            .any(|reason| reason.starts_with(TRANSITION))
    );
    let files = flip.settings();
    assert!(matches!(
        apply(&flip.store(), &refused),
        Err(DesktopError::InvalidValue { .. })
    ));
    assert_eq!(flip.settings(), files);
    let mut untiled = Profile::new("Untiled").unwrap();
    untiled.windows = Some(WindowsBehaviorLook {
        autotile: Some(false),
        autotile_behavior: Some(TilingScope::Global),
        ..Default::default()
    });
    let refused = plan(
        &store,
        &source.installed(),
        &untiled,
        &[Domain::Windows].into(),
    );
    assert!(!refused.can_apply());
    assert!(
        refused
            .blocking_reasons()
            .iter()
            .any(|reason| reason.contains("Turn automatic tiling off on the Windows page first")),
        "{:?}",
        refused.blocking_reasons()
    );
}

/// Floating-app rules (private titles), custom shortcut commands, their
/// names and paused commands are local: capture and export never carry
/// them, applying a captured profile never touches them, and editing,
/// applying and undoing them never runs a command.
#[test]
fn local_rules_and_commands_never_enter_profiles_or_run() {
    use crate::desktop::shortcuts::{
        self, Binding, Choice, NAMESPACE as KEYS, PAUSED_NAMESPACE, ShortcutsDraft,
    };
    use crate::desktop::window_rules::{
        self, CustomRule, NAMESPACE as RULES, WindowRulesDraft, exact_app,
    };
    let source = desk(
        "profile-local-source",
        Srgb::new(0.2, 0.6, 0.4),
        Srgb::new(0.8, 0.3, 0.2),
        "rose",
    );
    let ran = source.fixture.0.join("RAN-SENTINEL");
    let command = format!("touch {} # CMD-SENTINEL", ran.display());
    source.fixture.user(
        RULES,
        "tiling_exception_custom",
        r#"[(appid: "^org\\.sentinel\\.Private$", title: "PRIVATE-TITLE-SENTINEL /home/sentinel/secret.txt", enabled: true)]"#,
    );
    source.fixture.user(
        KEYS,
        "custom",
        &format!(
            r#"{{(modifiers: [Super, Ctrl], key: "k", description: Some("NAME-SENTINEL")): Spawn("{command}")}}"#
        ),
    );
    source.fixture.user(
        PAUSED_NAMESPACE,
        "disabled_commands",
        r#"[(binding: (modifiers: [Super, Ctrl], key: "p", description: Some("PAUSED-NAME-SENTINEL")), command: "PAUSED-SENTINEL /home/sentinel/run.sh")]"#,
    );
    let store = source.store();
    let captured = capture(&store, "Local").unwrap();
    let exported = String::from_utf8(export(&captured.profile).unwrap().bytes).unwrap();
    for secret in [
        "PRIVATE-TITLE-SENTINEL",
        "org.sentinel",
        "CMD-SENTINEL",
        "NAME-SENTINEL",
        "PAUSED-SENTINEL",
        "/home/sentinel",
        "WindowRules",
        "Shortcuts",
        "Spawn",
    ] {
        assert!(!exported.contains(secret), "{secret} leaked:\n{exported}");
        assert!(
            !captured.warnings.iter().any(|w| w.contains(secret)),
            "{secret}"
        );
    }

    // Applying a captured profile elsewhere leaves the target's local data alone.
    let target = desk(
        "profile-local-target",
        Srgb::new(0.1, 0.1, 0.9),
        Srgb::new(0.1, 0.1, 0.9),
        "teal",
    );
    target.fixture.user(
        KEYS,
        "custom",
        r#"{(modifiers: [Super], key: "y"): Spawn("target-own")}"#,
    );
    let plan = plan(
        &target.store(),
        &target.installed(),
        &captured.profile,
        &all(),
    );
    assert!(plan.can_apply(), "{:?}", plan.blocking_reasons());
    apply(&target.store(), &plan).unwrap();
    assert_eq!(
        target.fixture.read(KEYS, "custom").as_deref(),
        Some(r#"{(modifiers: [Super], key: "y"): Spawn("target-own")}"#)
    );
    assert_eq!(target.fixture.read(RULES, "tiling_exception_custom"), None);
    assert_eq!(
        target.fixture.read(PAUSED_NAMESPACE, "disabled_commands"),
        None
    );

    // Editing, Apply and Undo of both pages run nothing.
    let mail = Binding::parse("Super+Ctrl+K").unwrap();
    let mut keys = ShortcutsDraft::load(&store).unwrap();
    keys.set_running(mail.ident(), false, Choice::default())
        .unwrap();
    keys.save_command(
        None,
        "Another",
        &Binding::parse("Super+Shift+F9").unwrap(),
        &command,
        Choice::default(),
    )
    .unwrap();
    let change = store
        .apply_owned(&keys.plan().patch, None, Owner::Shortcuts, "Shortcuts")
        .unwrap();
    store.undo_latest(&change.id).unwrap();
    let mut rules = WindowRulesDraft::load(&store).unwrap();
    rules
        .save(
            None,
            CustomRule {
                appid: exact_app("org.example.Other"),
                title: String::new(),
                enabled: true,
            },
        )
        .unwrap();
    let change = store
        .apply_owned(
            &rules.plan().patch,
            None,
            Owner::WindowRules,
            "Floating apps",
        )
        .unwrap();
    store.undo_latest(&change.id).unwrap();
    assert!(!ran.exists(), "no command was run");
    assert!(shortcuts::key_label(&shortcuts::custom_key()).is_some());
    assert!(window_rules::key_label(&window_rules::custom_key()).is_some());
}

#[test]
fn desktop_tools_join_profiles_only_once_set_up_and_need_version_4() {
    let desk = desk(
        "profile-tools",
        Srgb::new(0.2, 0.6, 0.4),
        Srgb::new(0.8, 0.3, 0.2),
        "teal",
    );
    // Never set up: no tool members, so the record keeps an older format.
    let plain = capture(&desk.store(), "Plain").unwrap().profile;
    let layout = plain.widget_layout.as_ref().unwrap();
    assert!(layout.get("music").is_none() && layout.get("focus").is_none());
    assert_eq!(plain.version, 5);
    assert!(plain.widget_layout.as_ref().unwrap()["widget_sizes"]["clock"].is_null());
    let bytes = export(&plain).unwrap().bytes;
    assert!(!String::from_utf8_lossy(&bytes).contains("\"notes\""));

    // Set up Music and Notes: their visibility, place and card options go
    // in (never the note itself), and the record is version 4.
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&desk.config).unwrap()).unwrap();
    config["music"] = json!({"enabled": true, "player": "spotify", "show_artwork": false});
    config["notes"] = json!({"enabled": false, "size": "large"});
    config["widget_positions"]["music"] = json!([0.5, 0.9]);
    fs::write(&desk.config, config.to_string()).unwrap();
    let tooled = capture(&desk.store(), "Tools").unwrap().profile;
    assert_eq!(tooled.version, 5);
    let layout = tooled.widget_layout.as_ref().unwrap();
    assert_eq!(layout["music"]["enabled"], true);
    assert_eq!(layout["notes"]["enabled"], false);
    assert_eq!(layout["widget_positions"]["music"], json!([0.5, 0.9]));
    assert!(layout.get("focus").is_none());
    let style = tooled.widget_style.as_ref().unwrap();
    assert_eq!(style["music"]["show_artwork"], false);
    assert_eq!(style["notes"]["size"], "large");
    let exported = String::from_utf8(export(&tooled).unwrap().bytes).unwrap();
    assert!(
        !exported.contains("spotify"),
        "the player choice is this computer's"
    );
    assert!(exported.contains("\"version\": 5"));
    assert_eq!(import(exported.as_bytes()).unwrap(), tooled.portable().0);

    // An older record can't carry tool members.
    let mut old: serde_json::Value = serde_json::from_str(&exported).unwrap();
    old["version"] = json!(3);
    assert!(matches!(
        import(old.to_string().as_bytes()),
        Err(ProfileError::Invalid(message)) if message.contains("version 4")
    ));
}

#[test]
fn older_profiles_leave_tools_alone_and_presentation_hides_them() {
    let desk = desk(
        "profile-tools-apply",
        Srgb::new(0.2, 0.6, 0.4),
        Srgb::new(0.8, 0.3, 0.2),
        "teal",
    );
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&desk.config).unwrap()).unwrap();
    config["notes"] = json!({"enabled": true});
    config["focus"] = json!({"enabled": true, "focus_minutes": 50});
    fs::write(&desk.config, config.to_string()).unwrap();
    let layout: BTreeSet<Domain> = [Domain::WidgetLayout].into();

    // A version 1 record (the earlier Workbench preset) lists visible
    // widgets the old way; the tools stay as they are.
    let workbench = import(
        br##"{
  "format": "desktop-cards-profile", "version": 1, "name": "Workbench",
  "widget_layout": { "enabled_widgets": ["projects", "devbox"],
                     "glance": { "enabled": true, "initialized": true } }
}"##,
    )
    .unwrap();
    let plan = super::plan(&desk.store(), &desk.installed(), &workbench, &layout);
    assert!(plan.can_apply(), "{:?}", plan.blocking_reasons());
    apply(&desk.store(), &plan).unwrap();
    let saved = crate::config::check_config_value(
        &serde_json::from_str(&fs::read_to_string(&desk.config).unwrap()).unwrap(),
    )
    .unwrap();
    assert!(saved.notes.enabled && saved.focus.enabled);
    assert_eq!(saved.focus.focus_minutes, 50);

    // Presentation clears the screen, tools included, and Undo brings them back.
    let presentation = preset("presentation").unwrap().profile;
    let plan = super::plan(&desk.store(), &desk.installed(), &presentation, &layout);
    let changes = &plan.domains[0].changes;
    assert!(changes.contains(&"Notes".to_string()), "{changes:?}");
    assert!(changes.contains(&"Focus timer".to_string()), "{changes:?}");
    assert!(
        !changes.contains(&"Music".to_string()),
        "Music is already off"
    );
    let latest = apply(&desk.store(), &plan).unwrap();
    let hidden = crate::config::check_config_value(
        &serde_json::from_str(&fs::read_to_string(&desk.config).unwrap()).unwrap(),
    )
    .unwrap();
    assert!(!hidden.notes.enabled && !hidden.focus.enabled);
    assert_eq!(
        hidden.focus.focus_minutes, 50,
        "durations aren't part of profiles"
    );
    desk.store().undo_latest(&latest.id).unwrap();
    let restored = crate::config::check_config_value(
        &serde_json::from_str(&fs::read_to_string(&desk.config).unwrap()).unwrap(),
    )
    .unwrap();
    assert!(restored.notes.enabled && restored.focus.enabled);
}

#[test]
fn sizes_are_layout_members_and_capture_import_apply_and_undo_preserve_automatic() {
    use serde_json::Value;
    let desk = desk(
        "profile-resize",
        Srgb::new(0.2, 0.6, 0.4),
        Srgb::new(0.8, 0.3, 0.2),
        "teal",
    );
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&desk.config).unwrap()).unwrap();
    config["music"] = json!({"enabled":true});
    config["widget_sizes"] = json!({"weather":[480,320],"music":[480,320]});
    fs::write(&desk.config, config.to_string()).unwrap();
    let captured = capture(&desk.store(), "Sized").unwrap().profile;
    assert_eq!(captured.version, 5);
    assert_eq!(
        captured.widget_layout.as_ref().unwrap()["widget_sizes"]["music"],
        json!([480, 320])
    );
    assert_eq!(
        captured.widget_layout.as_ref().unwrap()["widget_sizes"]["clock"],
        Value::Null
    );
    let imported = import(&export(&captured).unwrap().bytes).unwrap();
    assert_eq!(imported.widget_layout, captured.widget_layout);
    config["widget_sizes"] = json!({"clock":[600,300],"weather":[360,180]});
    fs::write(&desk.config, config.to_string()).unwrap();
    let planned = plan(
        &desk.store(),
        &desk.installed(),
        &imported,
        &[Domain::WidgetLayout].into(),
    );
    let change = apply(&desk.store(), &planned).unwrap();
    let raw: Value = serde_json::from_str(&fs::read_to_string(&desk.config).unwrap()).unwrap();
    assert_eq!(raw["widget_sizes"]["weather"], json!([480, 320]));
    assert!(raw["widget_sizes"].get("clock").is_none());
    desk.store().undo_latest(&change.id).unwrap();
    let raw: Value = serde_json::from_str(&fs::read_to_string(&desk.config).unwrap()).unwrap();
    assert_eq!(raw["widget_sizes"], config["widget_sizes"]);
    let old = import(br#"{"format":"desktop-cards-profile","version":4,"name":"Old","widget_layout":{"placement":{"snap":true}}}"#).unwrap();
    assert!(
        old.widget_layout
            .as_ref()
            .unwrap()
            .get("widget_sizes")
            .is_none()
    );
    assert!(import(br#"{"format":"desktop-cards-profile","version":4,"name":"Invalid","widget_layout":{"widget_sizes":{"music":[480,320]}}}"#).is_err());
}

#[test]
fn islands_profiles_are_versioned_and_export_no_monitor_bindings() {
    let mut profile = Profile::new("Islands").unwrap();
    let mut islands = dock_islands_core::Settings {
        enabled: true,
        ..Default::default()
    };
    islands.layout_mut("Private display").separate.positions = [800, 4000, 9500];
    profile.dock = Some(PanelLook {
        islands: Some(islands.clone()),
        ..Default::default()
    });
    assert_eq!(profile.written_version(), 6);
    let exported = export(&profile).unwrap();
    let json = String::from_utf8(exported.bytes.clone()).unwrap();
    assert!(!json.contains("Private display"));
    let imported = import(&exported.bytes).unwrap();
    assert_eq!(imported.dock.unwrap().islands.unwrap(), islands.portable());
    profile.version = 5;
    assert!(profile.validate().is_err());
}

#[test]
fn multi_monitor_setups_capture_effective_layout_and_apply_with_undo() {
    let desk = desk(
        "monitor-profile",
        Srgb::new(0.2, 0.6, 0.4),
        Srgb::new(0.8, 0.3, 0.2),
        "teal",
    );
    let mut config = desk.json();
    config["widget_monitors"] = json!({"weather":{"display":"DP-2","hide_disconnected":true}});
    config["monitor_layouts"] = json!({"weather":{
        "DP-2":{"position":[0.25,0.5],"size":[480,320]},
        "eDP-1":{"position":[0.5,0.25],"size":[320,200]}
    }});
    fs::write(&desk.config, config.to_string()).unwrap();
    let profile = capture(&desk.store(), "Across screens").unwrap().profile;
    assert_eq!(
        profile.widget_layout.as_ref().unwrap()["widget_sizes"]["weather"],
        json!([480.0, 320.0])
    );
    let exported = export(&profile).unwrap();
    assert!(!String::from_utf8_lossy(&exported.bytes).contains("DP-2"));
    config["monitor_layouts"]["weather"]["DP-2"]["size"] = json!([600, 400]);
    fs::write(&desk.config, config.to_string()).unwrap();
    let planned = plan(
        &desk.store(),
        &desk.installed(),
        &profile,
        &[Domain::WidgetLayout].into(),
    );
    let change = apply(&desk.store(), &planned).unwrap();
    let applied = desk.json();
    for display in ["DP-2", "eDP-1"] {
        assert_eq!(
            applied["monitor_layouts"]["weather"][display]["size"],
            json!([480.0, 320.0])
        );
    }
    assert_eq!(applied["widget_monitors"], config["widget_monitors"]);
    desk.store().undo_latest(&change.id).unwrap();
    assert_eq!(desk.json()["monitor_layouts"], config["monitor_layouts"]);
}

#[test]
fn fb15_moments_profile_requires_v12_preserves_private_state_and_undo() {
    let desk = desk(
        "moments-profile",
        Srgb::new(0.3, 0.4, 0.7),
        Srgb::new(0.2, 0.4, 0.7),
        "teal",
    );
    let before = fs::read(&desk.config).unwrap();
    let mut p = Profile::new("Day and Night").unwrap();
    p.widget_style = Some(
        json!({"moments":{"active":"day","day":{"clock":{"face":"analog"}},"night":{"family":"midnight"}}}),
    );
    let bytes = export(&p).unwrap().bytes;
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap()["version"],
        12
    );
    p.version = 11;
    assert!(p.validate().is_err());
    let p = import(&bytes).unwrap();
    let store = desk.store();
    let proposal = plan(&store, &desk.installed(), &p, &[Domain::WidgetStyle].into());
    assert!(proposal.can_apply(), "{:?}", proposal.blocking_reasons());
    let changed = apply(&store, &proposal).unwrap();
    let after: Value = serde_json::from_slice(&fs::read(&desk.config).unwrap()).unwrap();
    let original: Value = serde_json::from_slice(&before).unwrap();
    for (key, value) in original.as_object().unwrap() {
        assert_eq!(after.get(key), Some(value), "{key}");
    }
    let captured = capture(&store, "Current").unwrap().profile;
    assert_eq!(captured.written_version(), 12);
    assert_eq!(
        captured.widget_style.as_ref().unwrap()["moments"]["day"]["family"],
        "paper"
    );
    let portable = String::from_utf8(export(&captured).unwrap().bytes).unwrap();
    for secret in SECRETS {
        assert!(!portable.contains(secret), "{secret}");
    }
    store.undo_latest(&changed.id).unwrap();
    assert_eq!(fs::read(&desk.config).unwrap(), before);
    assert!(capture(&store, "Old").unwrap().profile.written_version() < 12);
}

#[test]
fn rain_setup_capture_keeps_explicit_off_and_applies_with_undo() {
    let desk = desk(
        "rain-reset",
        Srgb::new(0.3, 0.4, 0.7),
        Srgb::new(0.2, 0.4, 0.7),
        "teal",
    );
    let mut raw: Value = serde_json::from_slice(&fs::read(&desk.config).unwrap()).unwrap();
    raw["rain"] = serde_json::to_value(crate::rain::Preferences::default()).unwrap();
    fs::write(&desk.config, serde_json::to_vec(&raw).unwrap()).unwrap();
    let store = desk.store();
    let captured = capture(&store, "Rain off").unwrap().profile;
    assert_eq!(captured.written_version(), 19);
    assert_eq!(
        captured.widget_style.as_ref().unwrap()["rain"]["enabled"],
        false
    );
    raw["rain"]["enabled"] = true.into();
    fs::write(&desk.config, serde_json::to_vec(&raw).unwrap()).unwrap();
    let before = fs::read(&desk.config).unwrap();
    let proposal = plan(
        &store,
        &desk.installed(),
        &captured,
        &[Domain::WidgetStyle].into(),
    );
    assert!(proposal.can_apply(), "{:?}", proposal.blocking_reasons());
    let receipt = apply(&store, &proposal).unwrap();
    let after: Value = serde_json::from_slice(&fs::read(&desk.config).unwrap()).unwrap();
    assert_eq!(after["rain"]["enabled"], false);
    store.undo_latest(&receipt.id).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&desk.config).unwrap()).unwrap(),
        serde_json::from_slice::<Value>(&before).unwrap()
    );
}
