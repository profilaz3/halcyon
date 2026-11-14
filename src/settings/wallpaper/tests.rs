use super::*;
use crate::desktop::store::tests::Fixture;
use crate::desktop::store::{ConfigKey, DesktopPatch, RawValue};
use crate::desktop::wallpaper::{NAMESPACE, list_key, output_key, same_key, shared_key};
use crate::settings::system_appearance::{self, AppearanceEditor};
use cosmic::cosmic_theme::palette::Srgba;
use std::fs;

#[test]
fn ux_native_wallpaper_and_effect_apply_and_undo_are_one_change() {
    let fixture = seeded("ux-effect-transaction");
    let store = fixture
        .store()
        .with_app_config(fixture.0.join("config.json"));
    fs::write(
        store.app_config().unwrap(),
        r#"{"theme":"rose","future":73}"#,
    )
    .unwrap();
    let before = fs::read(store.app_config().unwrap()).unwrap();
    let background = fs::read(store.user_path(&shared_key())).unwrap();
    let mut editor = WallpaperEditor::with_store(store.clone());
    let mut effect = crate::rain::Preferences::default();
    effect.enabled = true;
    effect.effect = crate::rain::Effect::Snow;
    editor.stage_effect(effect.clone()).unwrap();
    editor.update(Message::Select(Pick::Color(blue())));
    drive(&mut editor);
    assert_eq!(fs::read(store.app_config().unwrap()).unwrap(), before);
    assert_eq!(
        fs::read(store.user_path(&shared_key())).unwrap(),
        background
    );
    editor.update(Message::Apply);
    assert!(!editor.dirty(), "{:?}", editor.notice());
    assert_eq!(automatic::read_config(&store).unwrap().1.rain, effect);
    assert!(editor.latest().unwrap().widget.is_some());
    assert!(!editor.latest().unwrap().change.entries().is_empty());
    editor.update(Message::Undo);
    assert_eq!(fs::read(store.app_config().unwrap()).unwrap(), before);
    assert_eq!(
        fs::read(store.user_path(&shared_key())).unwrap(),
        background
    );
}

#[test]
fn ux_native_effect_conflict_prevents_partial_wallpaper_apply() {
    let fixture = seeded("ux-effect-conflict");
    let store = fixture
        .store()
        .with_app_config(fixture.0.join("config.json"));
    fs::write(store.app_config().unwrap(), "{}").unwrap();
    let background = fs::read(store.user_path(&shared_key())).unwrap();
    let mut editor = WallpaperEditor::with_store(store.clone());
    let mut wanted = crate::rain::Preferences::default();
    wanted.enabled = true;
    editor.stage_effect(wanted.clone()).unwrap();
    editor.update(Message::Select(Pick::Color(blue())));
    drive(&mut editor);
    let mut external = wanted.clone();
    external.effect = crate::rain::Effect::Clouds;
    fs::write(
        store.app_config().unwrap(),
        serde_json::to_vec(&serde_json::json!({"rain":external,"future":73})).unwrap(),
    )
    .unwrap();
    editor.update(Message::Apply);
    assert!(editor.effect_conflict());
    assert!(editor.dirty());
    assert_eq!(
        fs::read(store.user_path(&shared_key())).unwrap(),
        background
    );
    assert_eq!(automatic::read_config(&store).unwrap().1.rain, external);
    editor.update(Message::ResolveEffect(true));
    editor.update(Message::Apply);
    assert!(!editor.dirty(), "{:?}", editor.notice());
    assert_eq!(automatic::read_config(&store).unwrap().1.rain, wanted);
    assert_eq!(automatic::read_config(&store).unwrap().0["future"], 73);
}

fn png(path: &Path, rgb: [u8; 3]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbImage::from_pixel(64, 36, image::Rgb(rgb))
        .save(path)
        .unwrap();
}
fn entry(output: &str, source: &str) -> String {
    format!(
        "(output: \"{output}\", source: {source}, filter_by_theme: false, rotation_frequency: 900)"
    )
}
/// Two connected displays, a bundled wallpaper, a COSMIC background, a
/// slideshow folder and a shared background picture.
fn seeded(name: &str) -> Fixture {
    let fixture = Fixture::new(name);
    fs::write(fixture.0.join("outputs"), "DP-1\nHDMI-1\n").unwrap();
    png(
        &fixture.0.join("pictures/Desktop Cards/teal.jpg"),
        [20, 150, 140],
    );
    png(&fixture.0.join("backgrounds/cosmic/a.png"), [200, 120, 40]);
    png(&fixture.0.join("slides/one.png"), [60, 120, 230]);
    png(&fixture.0.join("slides/two.png"), [40, 200, 60]);
    fs::write(fixture.0.join("slides/notes.txt"), "text").unwrap();
    png(&fixture.0.join("home/current.png"), [90, 90, 90]);
    let current = fixture.0.join("home/current.png");
    fixture.user(
        NAMESPACE,
        "all",
        &entry("all", &format!("Path(\"{}\")", current.display())),
    );
    fixture
}
/// Runs the background work the editor asked for, like the app does.
fn drive(editor: &mut WallpaperEditor) {
    for _ in 0..64 {
        let jobs = editor.take_jobs();
        if jobs.is_empty() {
            return;
        }
        for job in jobs {
            editor.update(job.run());
        }
    }
    panic!("background work did not settle");
}
fn source(editor: &WallpaperEditor) -> BgSource {
    editor.entry().unwrap().known().unwrap().source.clone()
}
fn blue() -> BgColor {
    BgColor::Single(parse_rgb("#336699").unwrap())
}
/// The lively accent recommended for a color background.
fn recommended_of(color: BgColor) -> [u8; 3] {
    automatic::recommended(&crate::desktop::wallpaper::palette::from_color(&color)).unwrap()
}
/// The lively accent recommended for a picture of one color.
fn recommended_of_picture(rgb: [u8; 3]) -> [u8; 3] {
    let pixels = crate::desktop::wallpaper::media::Pixels {
        width: 4,
        height: 4,
        rgba: [rgb[0], rgb[1], rgb[2], 255].repeat(16),
    };
    automatic::recommended(&crate::desktop::wallpaper::palette::from_pixels(&pixels)).unwrap()
}
/// The tone a shared color gets in COSMIC `variant`.
fn native(variant: crate::desktop::appearance::ThemeVariant, rgb: [u8; 3]) -> [u8; 3] {
    crate::desktop::profiles::themes::native_accents(rgb)
        .into_iter()
        .find(|(v, _)| *v == variant)
        .unwrap()
        .1
}

#[test]
fn automatic_colors_editor_applies_wallpaper_and_adjusted_accent_once_and_undo_restores_both() {
    let fixture = seeded("automatic-editor-combined");
    let mut editor = WallpaperEditor::with_store(fixture.store());
    let store = fixture
        .store()
        .with_app_config(fixture.0.join("config.json"));
    fs::write(
        store.app_config().unwrap(),
        r#"{"theme":"rose","future":73}"#,
    )
    .unwrap();
    let before = fs::read(store.app_config().unwrap()).unwrap();
    let background = fs::read(store.user_path(&shared_key())).unwrap();
    editor.update(Message::AutomaticColors(true));
    editor.update(Message::Select(Pick::Color(blue())));
    drive(&mut editor);
    assert!(editor.can_apply());
    assert_eq!(editor.accent_color(), Some(recommended_of(blue())));
    editor.update(Message::AccentText("#D34762".into()));
    editor.update(Message::AccentSubmit);
    assert!(!editor.automatic_colors(&crate::config::Config::default()));
    editor.update(Message::Apply);
    assert!(!editor.dirty(), "{:?}", editor.notice());
    let (_, applied) = automatic::read_config(&store).unwrap();
    assert!(!applied.appearance.wallpaper_colors);
    assert_eq!(applied.appearance.accent, Some([211, 71, 98]));
    assert!(
        automatic::prepare(&store).unwrap().is_none(),
        "runtime preserves this wallpaper’s adjustment"
    );
    editor.update(Message::Undo);
    assert_eq!(fs::read(store.app_config().unwrap()).unwrap(), before);
    assert_eq!(
        fs::read(store.user_path(&shared_key())).unwrap(),
        background
    );
}

#[test]
fn automatic_colors_revisits_wallpaper_after_settings_apply_and_restart() {
    let fixture = seeded("automatic-revisit-settings");
    let path = fixture.0.join("config.json");
    fs::write(&path, r#"{"appearance":{"wallpaper_colors":true}}"#).unwrap();
    let store = fixture.store().with_app_config(path.clone());
    let original = fs::read_to_string(store.user_path(&shared_key())).unwrap();
    automatic::commit(&store, automatic::prepare(&store).unwrap().unwrap()).unwrap();
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    editor.update(Message::Select(Pick::Color(blue())));
    drive(&mut editor);
    editor.update(Message::Apply);
    assert!(!editor.dirty(), "{:?}", editor.notice());
    assert_eq!(
        automatic::read_config(&store).unwrap().1.appearance.accent,
        Some(recommended_of(blue()))
    );
    drop(editor);
    drop(store);
    let restarted = fixture.store().with_app_config(path);
    assert!(automatic::prepare(&restarted).unwrap().is_none());
    fixture.user(NAMESPACE, "all", &original);
    let before = fixture.tree();
    let plan = automatic::prepare(&restarted)
        .unwrap()
        .expect("returning to a previously followed wallpaper must update colors");
    assert_eq!(fixture.tree(), before, "preparation is read-only");
    automatic::commit(&restarted, plan).unwrap();
    assert_eq!(
        automatic::read_config(&restarted)
            .unwrap()
            .1
            .appearance
            .accent,
        Some([90, 90, 90])
    );
}

#[test]
fn automatic_colors_revisits_wallpaper_after_settings_undo_and_restart() {
    let fixture = seeded("automatic-revisit-settings-undo");
    let path = fixture.0.join("config.json");
    fs::write(&path, r#"{"appearance":{"wallpaper_colors":true}}"#).unwrap();
    let store = fixture.store().with_app_config(path);
    automatic::commit(&store, automatic::prepare(&store).unwrap().unwrap()).unwrap();
    let before = fs::read(store.app_config().unwrap()).unwrap();
    let original = fs::read(store.user_path(&shared_key())).unwrap();
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    editor.update(Message::Select(Pick::Color(blue())));
    drive(&mut editor);
    editor.update(Message::Apply);
    assert!(!editor.dirty(), "{:?}", editor.notice());
    let next = fs::read_to_string(store.user_path(&shared_key())).unwrap();
    drop(editor);
    let mut reopened = WallpaperEditor::with_store(fixture.store());
    drive(&mut reopened);
    reopened.update(Message::Undo);
    assert_eq!(fs::read(store.app_config().unwrap()).unwrap(), before);
    assert_eq!(fs::read(store.user_path(&shared_key())).unwrap(), original);
    drop(reopened);
    assert!(automatic::prepare(&store).unwrap().is_none());
    fixture.user(NAMESPACE, "all", &next);
    let plan = automatic::prepare(&store)
        .unwrap()
        .expect("Undo must not suppress a later return to its former wallpaper");
    automatic::commit(&store, plan).unwrap();
    assert_eq!(
        automatic::read_config(&store).unwrap().1.appearance.accent,
        Some(recommended_of(blue()))
    );
}

#[test]
fn automatic_colors_editor_drops_stale_analysis_and_recommends_again_on_next_wallpaper() {
    let fixture = seeded("automatic-editor-stale");
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    editor.update(Message::AutomaticColors(true));
    let stale = editor
        .take_jobs()
        .into_iter()
        .find(|job| matches!(job, Job::Automatic { .. }))
        .unwrap();
    editor.update(Message::Select(Pick::Color(blue())));
    let latest = editor.automatic_analysis;
    editor.update(stale.run());
    assert_eq!(editor.automatic_analysis, latest);
    drive(&mut editor);
    assert_eq!(editor.accent_color(), Some(recommended_of(blue())));
    editor.update(Message::AccentText("#DD6688".into()));
    editor.update(Message::AccentSubmit);
    assert!(
        !editor.automatic_colors(&crate::config::Config::default()),
        "a completed custom color is fixed"
    );
    editor.update(Message::AutomaticColors(true));
    editor.update(Message::Select(Pick::Color(BgColor::Single([
        0.1, 0.7, 0.4,
    ]))));
    drive(&mut editor);
    assert_eq!(
        editor.accent_color(),
        Some(recommended_of(BgColor::Single([0.1, 0.7, 0.4])))
    );
    assert!(editor.automatic_colors(&crate::config::Config::default()));
    editor.update(Message::AccentText("#DD6688".into()));
    editor.update(Message::AccentSubmit);
    editor.update(Message::SuggestAccent);
    assert_eq!(
        editor.accent_color(),
        Some(recommended_of(BgColor::Single([0.1, 0.7, 0.4])))
    );
    editor.update(Message::AutomaticColors(false));
    editor.update(Message::Select(Pick::Color(blue())));
    drive(&mut editor);
    assert_eq!(
        editor.accent_color(),
        Some(recommended_of(BgColor::Single([0.1, 0.7, 0.4]))),
        "off keeps the fixed color"
    );
}

#[test]
fn automatic_colors_recovered_hex_draft_and_deterministic_display_source_are_kept() {
    let fixture = seeded("automatic-editor-recovery");
    let mut editor = WallpaperEditor::with_store(fixture.store());
    editor.update(Message::AutomaticColors(true));
    editor.update(Message::Same(false));
    editor.update(Message::Display(Target::Output("HDMI-1".into())));
    editor.update(Message::Select(Pick::Color(blue())));
    drive(&mut editor);
    assert_eq!(editor.color_source_label(), Some("DP-1"));
    assert_ne!(
        editor.accent_color(),
        Some([51, 102, 153]),
        "previewing HDMI does not move the global source"
    );
    editor.update(Message::AccentText("#12".into()));
    editor.update(Message::AccentSubmit);
    let saved = editor.session_draft().unwrap();
    let saved = ron::from_str(&ron::to_string(&saved).unwrap()).unwrap();
    let mut reopened = WallpaperEditor::with_store(fixture.store());
    reopened.restore_session(saved).unwrap();
    drive(&mut reopened);
    assert!(reopened.automatic_colors(&crate::config::Config::default()));
    assert_eq!(
        reopened.color_text(&crate::config::Config::default()),
        "#12"
    );
    assert!(!reopened.can_apply());
    reopened.update(Message::AccentText("#123456".into()));
    reopened.update(Message::AccentSubmit);
    assert!(reopened.can_apply());
}

#[test]
fn automatic_colors_pending_manual_choice_survives_an_external_wallpaper_refresh() {
    let fixture = seeded("automatic-pending-choice");
    let mut editor = WallpaperEditor::with_store(fixture.store());
    editor.update(Message::AutomaticColors(true));
    drive(&mut editor);
    editor.update(Message::AccentText("#DD6688".into()));
    editor.update(Message::AccentSubmit);
    fixture.user(
        NAMESPACE,
        "all",
        &entry("all", "Color(Single((0.2, 0.4, 0.6)))"),
    );
    editor.update(Message::Refresh);
    drive(&mut editor);
    assert_eq!(editor.accent_color(), Some([221, 102, 136]));
    editor.update(Message::Apply);
    assert!(!editor.dirty(), "{:?}", editor.notice());
    assert!(
        automatic::prepare(editor.store().unwrap())
            .unwrap()
            .is_none()
    );
}

#[test]
fn automatic_colors_apply_rechecks_a_picture_replaced_before_watcher_delivery() {
    let fixture = seeded("automatic-apply-recheck");
    let mut editor = WallpaperEditor::with_store(fixture.store());
    editor.update(Message::AutomaticColors(true));
    drive(&mut editor);
    let stamp = editor.colors.as_ref().unwrap().stamp;
    let picture = fixture.0.join("home/current.png");
    png(&picture, [210, 60, 100]);
    fs::write(&picture, [fs::read(&picture).unwrap(), vec![0; 7]].concat()).unwrap();
    editor.update(Message::Apply);
    assert!(editor.dirty());
    assert!(editor.latest().is_none());
    drive(&mut editor);
    assert_ne!(editor.colors.as_ref().unwrap().stamp, stamp);
    assert_eq!(
        editor.accent_color(),
        Some(recommended_of_picture([210, 60, 100]))
    );
    editor.update(Message::Apply);
    assert!(!editor.dirty());
}

#[test]
fn wallpaper_colors_refresh_when_the_picture_changes_in_place() {
    let fixture = seeded("wallpaper-replaced-picture");
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    let before = editor.suggestion().unwrap().clone();
    let preview = editor.shown().handle.clone();
    let path = fixture.0.join("home/current.png");
    png(&path, [20, 150, 140]);
    // A deterministic version change even on a coarse timestamp filesystem.
    fs::write(&path, [fs::read(&path).unwrap(), vec![0; 7]].concat()).unwrap();
    editor.update(Message::Refresh);
    drive(&mut editor);
    assert_ne!(
        editor.suggestion(),
        Some(&before),
        "colors follow the new pixels"
    );
    assert_ne!(
        editor.shown().handle,
        preview,
        "preview follows the new pixels"
    );
}

#[test]
fn wallpaper_and_accent_apply_and_undo_as_one_change() {
    use crate::desktop::appearance::{AppearanceDraft, ColorRole, ThemeVariant, hex as color_hex};
    let fixture = seeded("wallpaper-accent-transaction");
    let app_config = fixture.0.join("config.json");
    let original = r#"{"theme":"teal","appearance":{"opacity":83,"color_source":"desktop","future_tint":17},"future_field":42}"#;
    fs::write(&app_config, original).unwrap();
    let mut editor =
        WallpaperEditor::with_store(fixture.store().with_app_config(app_config.clone()));
    drive(&mut editor);
    let config_before = fs::read(fixture.store().user_path(&shared_key())).unwrap();
    editor.update(Message::Select(Pick::Color(blue())));
    drive(&mut editor);
    let before = fixture.tree();
    editor.update(Message::SuggestAccent);
    assert_eq!(fixture.tree(), before, "selection never writes");
    editor.update(Message::Apply);
    let theme = AppearanceDraft::load(&fixture.store()).unwrap();
    for variant in [ThemeVariant::Light, ThemeVariant::Dark] {
        let Setting::Known(Some(color)) = theme.color(variant, ColorRole::Accent) else {
            panic!("selected accent must be applied to both variants");
        };
        let [r, g, b] = native(variant, recommended_of(blue()));
        assert_eq!(color_hex(color, false), format!("#{r:02X}{g:02X}{b:02X}"));
    }
    assert!(!editor.dirty());
    assert!(editor.can_undo());
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&app_config).unwrap()).unwrap();
    assert_eq!(
        saved["appearance"]["accent"],
        serde_json::json!(recommended_of(blue()))
    );
    assert_eq!(saved["appearance"]["color_source"], "custom");
    assert_eq!(saved["appearance"]["opacity"], 83);
    assert_eq!(saved["appearance"]["future_tint"], 17);
    assert_eq!(saved["future_field"], 42);
    let change = editor.latest().unwrap();
    assert!(
        change.widget.is_some(),
        "one transaction owns app and desktop colors"
    );
    assert_eq!(change.owner, Owner::Wallpaper);
    assert!(
        change
            .change
            .entries()
            .iter()
            .any(|item| item.key == shared_key())
    );
    assert!(
        change
            .change
            .entries()
            .iter()
            .any(|item| item.key.namespace().contains("CosmicTheme"))
    );
    editor.update(Message::Undo);
    assert_eq!(
        fs::read(fixture.store().user_path(&shared_key())).unwrap(),
        config_before
    );
    assert_eq!(fs::read_to_string(&app_config).unwrap(), original);
    let restored = AppearanceDraft::load(&fixture.store()).unwrap();
    assert_eq!(
        restored.color(ThemeVariant::Dark, ColorRole::Accent),
        Setting::Known(None)
    );
}

#[test]
fn colors_apply_keeps_pending_background_and_concurrent_preferences() {
    let fixture = seeded("colors-background-kept");
    let config_path = fixture.0.join("config.json");
    let original = serde_json::json!({"theme":"teal","appearance":{"accent":[90,90,90],"opacity":83},"future":17});
    fs::write(&config_path, original.to_string()).unwrap();
    let store = fixture.store().with_app_config(config_path.clone());
    let background = fs::read(store.user_path(&shared_key())).unwrap();
    let mut editor = WallpaperEditor::with_store(store);
    editor.update(Message::Select(Pick::Color(blue())));
    editor.update(Message::Palette("rose".into()));
    // An unrelated, newer preference must not be replaced by the draft.
    let mut concurrent = original.clone();
    concurrent["appearance"]["opacity"] = serde_json::json!(91);
    concurrent["future"] = serde_json::json!(42);
    fs::write(&config_path, concurrent.to_string()).unwrap();
    editor.update(Message::ApplyColors);
    assert!(
        editor.colors_dirty() && editor.background_dirty(),
        "preview is indivisible"
    );
    assert_eq!(source(&editor), BgSource::Color(blue()));
    assert_eq!(
        fs::read(editor.store().unwrap().user_path(&shared_key())).unwrap(),
        background
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(&config_path).unwrap()).unwrap(),
        concurrent
    );
    editor.update(Message::Apply);
    assert!(!editor.dirty(), "{:?}", editor.notice());
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    assert_eq!(saved["theme"], "rose");
    assert!(saved["appearance"]["accent"].is_null());
    assert_eq!(saved["appearance"]["opacity"], 91);
    assert_eq!(saved["future"], 42);
    assert!(editor.can_undo());
    editor.update(Message::Undo);
    let restored: serde_json::Value =
        serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    assert_eq!(restored, concurrent);
    assert_eq!(
        fs::read(editor.store().unwrap().user_path(&shared_key())).unwrap(),
        background
    );
}

#[test]
fn widget_only_colors_do_not_write_cosmic_and_scope_survives_session() {
    let fixture = seeded("colors-widget-only");
    let mut editor = WallpaperEditor::with_store(fixture.store());
    editor.update(Message::Palette("ocean".into()));
    editor.update(Message::SyncColors(false));
    assert!(!editor.accent_dirty());
    let saved = ron::to_string(&editor.session_draft().unwrap()).unwrap();
    let mut editor = WallpaperEditor::with_store(fixture.store());
    editor
        .restore_session(ron::from_str(&saved).unwrap())
        .unwrap();
    assert!(!editor.sync_colors(&crate::config::Config::default()));
    let before = fixture.tree();
    editor.update(Message::ApplyColors);
    assert!(!editor.dirty());
    assert!(editor.latest().unwrap().change.entries().is_empty());
    assert!(editor.latest().unwrap().widget.is_some());
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.0.join("config.json")).unwrap()).unwrap();
    assert_eq!(saved["theme"], "ocean");
    assert_eq!(saved["appearance"]["sync_colors"], false);
    editor.update(Message::Undo);
    for (path, bytes) in before {
        assert_eq!(fs::read(path).ok(), bytes);
    }
    assert!(
        !fixture.0.join("config.json").exists(),
        "Undo restores an absent app config"
    );
}

#[test]
fn concurrent_app_color_conflict_prevents_every_color_write() {
    let fixture = seeded("colors-app-conflict");
    let path = fixture.0.join("config.json");
    fs::write(&path, r#"{"theme":"rose","appearance":{"accent":null,"color_source":"custom","sync_colors":true}}"#).unwrap();
    let mut editor = WallpaperEditor::with_store(fixture.store());
    editor.update(Message::Palette("rose".into()));
    assert!(
        editor
            .colors
            .as_ref()
            .unwrap()
            .patch()
            .unwrap()
            .changes()
            .all(|(pointer, _)| pointer == "/appearance/color_policy"),
        "resync upgrades only explicit policy; all color values remain guards"
    );
    fs::write(&path, r#"{"theme":"amber","future":42}"#).unwrap();
    let before = fixture.tree();
    editor.update(Message::ApplyColors);
    assert!(editor.colors_conflict());
    assert!(!editor.can_apply_colors());
    for (path, bytes) in before {
        assert_eq!(fs::read(path).ok(), bytes);
    }
    assert!(editor.latest().is_none());
    editor.update(Message::UseCurrent);
    assert!(!editor.colors_dirty());
}

#[test]
fn unavailable_background_never_enables_color_apply_or_panics() {
    let fixture = seeded("colors-unavailable-background");
    let path = fixture.store().user_path(&shared_key());
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let mut editor = WallpaperEditor::with_store(fixture.store());
    assert!(editor.draft().is_none());
    editor.update(Message::Palette("rose".into()));
    assert!(!editor.can_apply());
    assert!(!editor.can_apply_colors());
    let before = fixture.tree();
    editor.update(Message::ApplyColors);
    editor.update(Message::Apply);
    assert_eq!(fixture.tree(), before);
}

#[test]
fn already_matching_native_colors_stay_pinned_until_shared_apply() {
    for refresh in [false, true] {
        for one_matching in [false, true] {
            let fixture = seeded("colors-native-guards");
            let mut initial = WallpaperEditor::with_store(fixture.store());
            initial.update(Message::Palette("teal".into()));
            initial.update(Message::ApplyColors);
            let path = fixture.0.join("config.json");
            fs::write(&path, r#"{"theme":"rose"}"#).unwrap();
            let dark =
                ConfigKey::new("com.system76.CosmicTheme.Dark.Builder", 2, "accent").unwrap();
            let light =
                ConfigKey::new("com.system76.CosmicTheme.Light.Builder", 2, "accent").unwrap();
            if one_matching {
                fs::write(fixture.store().user_path(&light), "Some(\"#FF8800FF\")").unwrap();
            }
            let mut editor = WallpaperEditor::with_store(fixture.store());
            editor.update(Message::Palette("teal".into()));
            let patch = editor.accent_patch().unwrap();
            assert!(
                patch.guards().any(|(key, _)| key == &dark),
                "matching variant remains guarded"
            );
            fs::write(fixture.store().user_path(&dark), "Some(\"#FF8800FF\")").unwrap();
            if refresh {
                editor.update(Message::Refresh);
            }
            let before = fs::read(&path).unwrap();
            let dark_before = fs::read(fixture.store().user_path(&dark)).unwrap();
            editor.update(Message::ApplyColors);
            assert_eq!(
                fs::read(&path).unwrap(),
                before,
                "native conflict prevents app write"
            );
            assert_eq!(
                fs::read(fixture.store().user_path(&dark)).unwrap(),
                dark_before
            );
            assert!(editor.colors_dirty());
            assert!(!editor.accent_conflicts().is_empty());
            assert!(!editor.can_apply_colors());
        }
    }
}

#[test]
fn no_op_color_apply_rechecks_both_participants_before_reporting_success() {
    for changed in ["app", "native", "unreadable", "none"] {
        let fixture = seeded("colors-no-op-check");
        let path = fixture.0.join("config.json");
        let mut editor = WallpaperEditor::with_store(fixture.store());
        editor.update(Message::Palette("teal".into()));
        editor.update(Message::ApplyColors);
        let latest = editor.latest().unwrap().id.clone();
        editor.update(Message::Palette("teal".into()));
        assert!(editor.colors.as_ref().unwrap().patch().unwrap().is_empty());
        assert!(editor.accent_patch().unwrap().is_empty());
        match changed {
            "app" => fs::write(&path, r#"{"theme":"rose"}"#).unwrap(),
            "unreadable" => fs::write(&path, "{").unwrap(),
            "native" => {
                let key =
                    ConfigKey::new("com.system76.CosmicTheme.Dark.Builder", 2, "accent").unwrap();
                fs::write(fixture.store().user_path(&key), "Some(\"#FF8800FF\")").unwrap();
            }
            _ => {}
        }
        let before = fixture.tree();
        editor.update(Message::ApplyColors);
        assert_eq!(fixture.tree(), before);
        assert_eq!(
            editor.latest().unwrap().id,
            latest,
            "no-op never replaces Undo"
        );
        assert_eq!(editor.colors_dirty(), changed != "none");
        assert_eq!(
            matches!(editor.notice(), Some(Notice::Done(_))),
            changed == "none"
        );
    }
}

#[test]
fn wallpaper_accent_choice_survives_session_and_conflicts_never_partly_apply() {
    let fixture = seeded("wallpaper-accent-conflict");
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    // Neutral images used to make the only accent action unavailable.
    assert_eq!(editor.suggestion().unwrap().accent, None);
    editor.update(Message::Accent([90, 90, 90]));
    assert!(editor.accent_dirty());
    assert!(editor.can_apply(), "an accent alone can be applied");
    editor.update(Message::Accent([255, 0, 0]));
    assert_eq!(
        editor.accent_color(),
        Some([90, 90, 90]),
        "reject stale/unknown swatches"
    );
    editor.update(Message::Select(Pick::Color(blue())));
    let saved = editor.session_draft().unwrap();
    let saved = ron::from_str(&ron::to_string(&saved).unwrap()).unwrap();
    let mut restored = WallpaperEditor::with_store(fixture.store());
    restored.restore_session(saved).unwrap();
    assert_eq!(restored.accent_color(), Some([90, 90, 90]));
    assert!(restored.accent_dirty());
    // Changing an accent after staging must leave the background untouched.
    let accent_file = fixture
        .store()
        .user_path(&ConfigKey::new("com.system76.CosmicTheme.Dark.Builder", 2, "accent").unwrap());
    fs::create_dir_all(accent_file.parent().unwrap()).unwrap();
    fs::write(&accent_file, "Some(\"#FF8800FF\")").unwrap();
    let files = || {
        fixture
            .tree()
            .into_iter()
            .filter(|(path, bytes)| {
                bytes.is_some()
                    && !(path.extension().is_some_and(|ext| ext == "lock")
                        && bytes.as_ref().is_some_and(Vec::is_empty))
            })
            .collect::<BTreeMap<_, _>>()
    };
    let before = files();
    restored.update(Message::Apply);
    assert_eq!(
        files(),
        before,
        "theme conflict aborts the whole transaction"
    );
    assert!(restored.dirty());
    assert!(!restored.accent_conflicts().is_empty());
    assert!(!restored.can_apply());
    restored.update(Message::UseCurrent);
    assert!(!restored.accent_dirty());
    assert!(
        restored.core.dirty(),
        "the unrelated wallpaper edit remains"
    );
    restored.update(Message::Discard);
    assert!(!restored.dirty());
}

#[test]
fn wallpaper_slideshow_colors_follow_the_current_picture() {
    let fixture = seeded("wallpaper-slideshow-colors");
    let mut editor = WallpaperEditor::with_store(fixture.store());
    editor.update(Message::Select(Pick::Folder(fixture.0.join("slides"))));
    drive(&mut editor);
    let before = editor.suggestion().unwrap().clone();
    let state = media::state_path(&fixture.store()).unwrap();
    fs::create_dir_all(state.parent().unwrap()).unwrap();
    fs::write(
        &state,
        ron::to_string(&vec![(
            "DP-1",
            BgSource::Path(fixture.0.join("slides/two.png")),
        )])
        .unwrap(),
    )
    .unwrap();
    editor.update(Message::Refresh);
    drive(&mut editor);
    assert_ne!(editor.suggestion(), Some(&before));
    assert_eq!(editor.shown().image, Some(fixture.0.join("slides/two.png")));
    assert!(editor.watch_paths().contains(&state));
}

#[test]
fn interrupted_wallpaper_and_accent_apply_restores_both_domains() {
    use crate::desktop::store::Fault;
    let fixture = seeded("wallpaper-accent-interrupted");
    fs::write(
        fixture.0.join("config.json"),
        r#"{"theme":"teal","future":42}"#,
    )
    .unwrap();
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    let config_files = || {
        fixture
            .tree()
            .into_iter()
            .filter(|(path, bytes)| {
                (path.starts_with(fixture.0.join("config"))
                    || *path == fixture.0.join("config.json"))
                    && bytes.is_some()
            })
            .collect::<BTreeMap<_, _>>()
    };
    let before = config_files();
    editor.update(Message::Select(Pick::Color(blue())));
    drive(&mut editor);
    editor.update(Message::SuggestAccent);
    editor
        .store()
        .unwrap()
        .inject(Some(Fault::InterruptBeforeRecord));
    editor.update(Message::Apply);
    assert!(editor.recovery().is_some());
    assert!(!editor.can_apply());
    assert_ne!(
        config_files(),
        before,
        "interruption occurred after both domains were written"
    );
    editor.store().unwrap().inject(None);
    editor.update(Message::Recover);
    assert_eq!(
        config_files(),
        before,
        "recovery restores background and theme keys"
    );
    assert!(editor.recovery().is_none());
}

#[test]
fn preview_and_analysis_write_nothing() {
    let fixture = seeded("wallpaper-page-preview");
    let before = fixture.tree();
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    for tab in [Tab::Bundled, Tab::Cosmic, Tab::Yours, Tab::Colors] {
        editor.update(Message::Tab(tab));
        drive(&mut editor);
        assert!(!editor.tiles().is_empty(), "{tab:?} has tiles");
        for tile in editor.tiles().to_vec().into_iter().take(3) {
            editor.update(Message::Select(tile.pick));
            drive(&mut editor);
        }
    }
    editor.update(Message::Same(false));
    editor.update(Message::Display(Target::Output("HDMI-1".into())));
    editor.update(Message::Select(Pick::Folder(fixture.0.join("slides"))));
    drive(&mut editor);
    let scan = editor
        .scan(&fixture.0.join("slides"))
        .unwrap()
        .as_ref()
        .unwrap();
    assert_eq!((scan.images, scan.others), (2, 1));
    assert!(editor.shown().folder && editor.shown().handle.is_some());
    assert!(editor.suggestion().is_some_and(|s| !s.swatches.is_empty()));
    editor.update(Message::SuggestAccent);
    assert!(editor.accent_dirty());
    assert!(editor.dirty(), "only a draft");
    assert_eq!(
        fixture.tree(),
        before,
        "config, state and pictures untouched"
    );
}

#[test]
fn selecting_bundled_tile_drafts_shared_source_with_installed_path() {
    let fixture = seeded("wallpaper-page-bundled");
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    let teal = editor
        .tiles()
        .iter()
        .find(|tile| tile.pick == Pick::Bundled("teal".into()))
        .cloned()
        .unwrap();
    assert!(!teal.missing);
    assert!(editor.thumb(&teal).is_some(), "compiled-in thumbnail");
    editor.update(Message::Select(teal.pick));
    drive(&mut editor);
    let installed = fixture.0.join("pictures/Desktop Cards/teal.jpg");
    assert_eq!(source(&editor), BgSource::Path(installed));
    assert!(
        editor.shown().handle.is_some(),
        "preview from the bundled thumbnail"
    );
    assert!(editor.can_apply());
    // A bundled wallpaper that isn't installed is shown as missing; Apply
    // waits and Reinstall goes to the controller.
    editor.update(Message::Select(Pick::Bundled("amber".into())));
    drive(&mut editor);
    assert!(!editor.can_apply());
    assert!(
        editor
            .source_problem()
            .unwrap()
            .contains("moved or deleted")
    );
    assert!(
        editor
            .tiles()
            .iter()
            .any(|tile| tile.pick == Pick::Bundled("amber".into()) && tile.missing)
    );
    assert_eq!(
        editor.update(Message::Reinstall),
        Outcome::Controller(vec!["wallpapers-install".into()])
    );
    editor.update(Message::Select(Pick::Bundled("unknown".into())));
    assert_eq!(
        source(&editor),
        BgSource::Path(fixture.0.join("pictures/Desktop Cards/amber.jpg")),
        "an unknown name is no edit"
    );
}

#[test]
fn separate_display_flow_writes_entry_list_and_mode() {
    let fixture = seeded("wallpaper-page-separate");
    let store = fixture.store();
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    let before = fixture.tree();
    editor.update(Message::Same(false));
    editor.update(Message::Display(Target::Output("DP-1".into())));
    editor.update(Message::Tab(Tab::Colors));
    editor.update(Message::Select(Pick::Color(blue())));
    drive(&mut editor);
    assert_eq!(source(&editor), BgSource::Color(blue()));
    assert!(editor.can_apply(), "{:?}", editor.blocked());
    editor.update(Message::Apply);
    let latest = editor.latest().cloned().expect("owned change");
    assert_eq!(latest.owner, Owner::Wallpaper);
    let keys: Vec<ConfigKey> = latest
        .change
        .entries()
        .iter()
        .map(|entry| entry.key.clone())
        .collect();
    assert_eq!(
        keys,
        vec![output_key("DP-1").unwrap(), list_key(), same_key()]
    );
    let read = |key: &ConfigKey| fs::read_to_string(store.user_path(key)).unwrap();
    assert!(read(&output_key("DP-1").unwrap()).contains("output: \"DP-1\""));
    assert!(read(&list_key()).contains("DP-1"));
    assert_eq!(read(&same_key()).trim(), "false");
    assert!(!editor.dirty() && editor.can_undo());
    editor.update(Message::Undo);
    let mut after = fixture.tree();
    // Undo leaves the store's own state (journal, lock, Undo record).
    after.retain(|path, _| !path.starts_with(fixture.0.join("state")));
    let mut expected = before.clone();
    expected.retain(|path, _| !path.starts_with(fixture.0.join("state")));
    let config = |tree: &std::collections::BTreeMap<PathBuf, Option<Vec<u8>>>| {
        tree.iter()
            .filter(|(path, contents)| {
                contents.is_some() && path.starts_with(fixture.0.join("config"))
            })
            .map(|(path, contents)| (path.clone(), contents.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(config(&after), config(&expected), "Undo restores the keys");
}

#[test]
fn wallpaper_page_undo_offers_only_its_own_change() {
    let fixture = seeded("wallpaper-page-owner");
    let store = fixture.store();
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    editor.update(Message::Fit(FitChoice::Stretch));
    editor.update(Message::Apply);
    assert!(editor.can_undo());
    // Another page's change becomes the latest one.
    let opacity = ConfigKey::new("com.system76.CosmicPanel.Dock", 1, "opacity").unwrap();
    let mut patch = DesktopPatch::new();
    patch.set(opacity, None, Some(RawValue::parse("0.5").unwrap()));
    store
        .apply_owned(&patch, None, Owner::DockPanel, "Dock & Panel")
        .unwrap();
    editor.update(Message::Refresh);
    assert!(!editor.can_undo());
    assert_eq!(
        editor.elsewhere(),
        Some(&(Owner::DockPanel, "Dock opacity".to_string()))
    );
    assert_eq!(
        crate::settings::recovery::owner_page(Owner::Wallpaper),
        crate::settings::Page::DesktopWallpaper
    );
}

#[test]
fn outside_change_to_an_edited_background_keeps_other_edits() {
    let fixture = seeded("wallpaper-page-conflict");
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    editor.update(Message::Select(Pick::Color(blue())));
    fixture.user(
        NAMESPACE,
        "all",
        &entry("all", "Color(Single((0.9, 0.1, 0.1)))"),
    );
    editor.update(Message::Refresh);
    assert_eq!(editor.draft().unwrap().conflicts(), &[shared_key()]);
    let before = fixture.tree();
    editor.update(Message::Apply);
    assert_eq!(fixture.tree(), before, "a conflict writes nothing");
    editor.update(Message::UseCurrent);
    assert!(!editor.dirty());
    assert_eq!(
        source(&editor),
        BgSource::Color(BgColor::Single([0.9, 0.1, 0.1]))
    );
}

/// Use as accent changes the System appearance draft only: both variants'
/// accents, other edits kept, nothing written. An unapplied accent (chosen
/// or typed) waits for Replace accent / Cancel.
#[test]
fn suggest_accent_sets_both_variants_in_appearance_draft_only() {
    use crate::desktop::appearance::{ColorRole, ThemeVariant, hex as color_hex, parse_hex};
    use system_appearance::Message as Look;
    let fixture = Fixture::new("wallpaper-accent");
    fixture.user("com.system76.CosmicTheme.Mode", "is_dark", "true");
    let mut appearance = AppearanceEditor::with_store(fixture.store());
    appearance.update(Look::Color(
        ColorRole::Background,
        parse_hex("#202020", true),
    ));
    let before = fixture.tree();
    let accent = |editor: &AppearanceEditor, variant| match editor
        .draft()
        .unwrap()
        .color(variant, ColorRole::Accent)
    {
        Setting::Known(Some(color)) => color_hex(color, false),
        other => panic!("{other:?}"),
    };
    let background = |editor: &AppearanceEditor| {
        editor
            .draft()
            .unwrap()
            .color(ThemeVariant::Dark, ColorRole::Background)
    };
    let kept_background = background(&appearance);
    let wallpaper = Srgba::new(0.2, 0.4, 0.6, 1.0);
    appearance.suggest_accent(wallpaper);
    assert_eq!(accent(&appearance, ThemeVariant::Light), "#336699");
    assert_eq!(accent(&appearance, ThemeVariant::Dark), "#336699");
    assert_eq!(background(&appearance), kept_background, "other edits stay");
    assert_eq!(
        appearance.notice,
        Some(Notice::Done(system_appearance::ACCENT_NOTICE.into()))
    );
    assert!(appearance.dirty());

    // Typed accent text is pending: the next suggestion waits.
    appearance.update(Look::Variant(ThemeVariant::Light));
    appearance.update(Look::HexInput(ColorRole::Accent, "#12".into()));
    let other = Srgba::new(0.8, 0.1, 0.1, 1.0);
    appearance.suggest_accent(other);
    assert_eq!(appearance.accent_choice, Some(other));
    assert_eq!(
        accent(&appearance, ThemeVariant::Dark),
        "#336699",
        "not replaced yet"
    );
    appearance.update(Look::KeepAccent);
    assert_eq!(appearance.accent_choice, None);
    assert_eq!(
        appearance.hex_text(ColorRole::Accent),
        Some("#12"),
        "typed text kept"
    );
    assert_eq!(accent(&appearance, ThemeVariant::Light), "#336699");
    assert_eq!(background(&appearance), kept_background);
    appearance.suggest_accent(other);
    appearance.update(Look::ReplaceAccent);
    assert_eq!(accent(&appearance, ThemeVariant::Light), "#CC1A1A");
    assert_eq!(accent(&appearance, ThemeVariant::Dark), "#CC1A1A");
    assert_eq!(appearance.hex_text(ColorRole::Accent), None);
    assert_eq!(background(&appearance), kept_background);
    assert_eq!(fixture.tree(), before, "no store write");
}

#[test]
fn empty_or_missing_sources_block_apply() {
    let fixture = seeded("wallpaper-page-sources");
    let empty = fixture.0.join("empty");
    fs::create_dir_all(&empty).unwrap();
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    editor.update(Message::Select(Pick::Image(
        fixture.0.join("slides/one.png"),
    )));
    drive(&mut editor);
    editor.update(Message::Apply);
    let earlier = editor.latest().cloned().expect("applied");
    let gone = fixture.0.join("gone.png");
    editor.update(Message::Select(Pick::Image(gone.clone())));
    drive(&mut editor);
    assert_eq!(
        media::check_source(&BgSource::Path(gone.clone())),
        media::SourceCheck::Missing
    );
    assert!(!editor.can_apply());
    assert_eq!(
        editor.source_problem(),
        Some("This picture was moved or deleted")
    );
    assert_eq!(editor.shown().note, Some(media::MISSING));
    let files = fixture.tree();
    editor.update(Message::Apply);
    assert_eq!(fixture.tree(), files, "nothing written");
    assert!(editor.dirty(), "the draft is kept");
    assert_eq!(
        editor.latest().cloned(),
        Some(earlier.clone()),
        "earlier Undo kept"
    );

    editor.update(Message::Select(Pick::Folder(empty.clone())));
    assert_eq!(editor.source_problem(), Some("Checking the folder…"));
    drive(&mut editor);
    assert_eq!(
        media::check_source(&BgSource::Path(empty.clone())),
        media::SourceCheck::Empty
    );
    assert_eq!(editor.source_problem(), Some("No pictures in this folder"));
    assert!(!editor.can_apply());

    // A scan cut short before any picture is "too large", not "empty".
    let large = fixture.0.join("large");
    fs::create_dir_all(&large).unwrap();
    editor.update(Message::Select(Pick::Folder(large.clone())));
    let (id, path) = editor.scanning.clone().expect("scan pending");
    editor.take_jobs();
    editor.update(Message::Scanned(
        id,
        path,
        Ok(FolderScan {
            images: 0,
            others: 10_001,
            truncated: true,
            first: Vec::new(),
            ..FolderScan::default()
        }),
    ));
    assert_eq!(
        editor.source_problem(),
        Some("This folder is too large to check")
    );
    assert!(editor.dirty());
    assert_eq!(editor.latest().cloned(), Some(earlier));
}

#[test]
fn stale_async_results_are_ignored() {
    let fixture = seeded("wallpaper-page-stale");
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    // A thumbnail nobody asked for is not cached.
    let stray = fixture.0.join("slides/one.png");
    editor.update(Message::Thumb(
        9_999,
        stray.clone(),
        Preview::Ready(Pixels {
            width: 1,
            height: 1,
            rgba: vec![1, 2, 3, 255],
        }),
    ));
    assert!(editor.thumbs.map.keys().all(|key| key.0 != stray));
    // A picture that changed while it was decoded is decoded again.
    editor.update(Message::Tab(Tab::Cosmic));
    let jobs = editor.take_jobs();
    let picture = fixture.0.join("backgrounds/cosmic/a.png");
    let job = jobs
        .iter()
        .find(|job| matches!(job, Job::Thumb { path, .. } if *path == picture))
        .cloned()
        .expect("thumbnail job");
    std::thread::sleep(std::time::Duration::from_millis(20));
    png(&picture, [1, 2, 3]);
    fs::write(&picture, [fs::read(&picture).unwrap(), vec![0; 7]].concat()).unwrap();
    editor.update(job.run());
    assert!(editor.thumbs.map.keys().all(|key| key.0 != picture));

    // Analysis of an earlier selection never replaces the current one.
    editor.update(Message::Select(Pick::Color(blue())));
    let first = editor.take_jobs();
    let red = BgColor::Single([0.8, 0.1, 0.1]);
    editor.update(Message::Select(Pick::Color(red.clone())));
    let second = editor.take_jobs();
    let analysis = |jobs: &[Job]| {
        jobs.iter()
            .find(|job| matches!(job, Job::Analyse { .. }))
            .cloned()
            .unwrap()
    };
    editor.update(analysis(&first).run());
    assert_eq!(editor.suggestion(), None, "stale analysis dropped");
    editor.update(analysis(&second).run());
    assert_eq!(editor.suggestion().unwrap().swatches[0].rgb, [204, 26, 26]);

    // A file-chooser result for an older request is ignored.
    let Outcome::Dialog(request) = editor.update(Message::AddPicture) else {
        panic!("dialog");
    };
    let before = fixture.tree();
    editor.update(Message::Picked(request.id + 1, Ok(Some(stray.clone()))));
    assert_eq!(fixture.tree(), before);
    assert!(editor.collection.as_ref().unwrap().items().is_empty());
    // A scan result for another request is ignored.
    editor.update(Message::Scanned(
        424_242,
        fixture.0.join("empty-x"),
        Ok(FolderScan::default()),
    ));
    assert!(editor.scan(&fixture.0.join("empty-x")).is_none());
}

/// Typed colours: a complete colour goes into the draft; other text stays
/// visible, never applies an earlier value and blocks Apply until fixed.
#[test]
fn invalid_color_text_keeps_draft_and_blocks_apply() {
    let fixture = seeded("wallpaper-page-typed");
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    editor.update(Message::Tab(Tab::Colors));
    editor.update(Message::Stop(0, "#336699".into()));
    assert_eq!(source(&editor), BgSource::Color(blue()));
    assert!(editor.can_apply());
    editor.update(Message::Stop(0, "#33669".into()));
    assert_eq!(editor.custom().unwrap().text(0), "#33669");
    assert!(editor.typed_error().is_some());
    assert!(!editor.can_apply());
    let before = fixture.tree();
    editor.update(Message::Apply);
    assert_eq!(fixture.tree(), before, "nothing applied");
    // Navigation keeps the text.
    editor.update(Message::Tab(Tab::Bundled));
    editor.update(Message::Tab(Tab::Colors));
    assert_eq!(editor.custom().unwrap().text(0), "#33669");
    editor.update(Message::Stops(2));
    editor.update(Message::Stop(0, "#336699".into()));
    editor.update(Message::Stop(1, "#000000".into()));
    editor.update(Message::Direction(90));
    let BgSource::Color(BgColor::Gradient(gradient)) = source(&editor) else {
        panic!("gradient");
    };
    assert_eq!(gradient.radius, 90.);
    assert_eq!(
        hex(gradient.colors[0].map(wallpaper::linear_to_srgb)),
        "#336699"
    );
    assert!(editor.can_apply());
    editor.update(Message::Apply);
    assert!(
        !editor.dirty() && editor.custom().is_none(),
        "typed state settles"
    );
    editor.update(Message::Undo);
    assert!(editor.custom().is_none());

    // Letterbox colour.
    editor.update(Message::Select(Pick::Image(
        fixture.0.join("slides/one.png"),
    )));
    editor.update(Message::Fit(FitChoice::Fit));
    editor.update(Message::FitColor("#zz".into()));
    assert!(!editor.can_apply());
    editor.update(Message::FitColor("#101010".into()));
    assert!(matches!(
        editor.entry(),
        Some(Setting::Known(entry)) if entry.scaling_mode == ScalingMode::Fit(parse_rgb("#101010").unwrap())
    ));
    assert!(editor.can_apply());
    editor.update(Message::FitColor("#1010".into()));
    editor.update(Message::Discard);
    assert!(!editor.dirty() && editor.typed_error().is_none() && editor.fit_text().is_none());
}

#[test]
fn stored_gradient_stops_and_angle_are_kept_until_an_explicit_edit() {
    let fixture = seeded("wallpaper-page-gradient");
    let stops =
        "[(0.1, 0.2, 0.3), (0.2, 0.3, 0.4), (0.3, 0.4, 0.5), (0.4, 0.5, 0.6), (0.5, 0.6, 0.7)]";
    fixture.user(
        NAMESPACE,
        "all",
        &entry(
            "all",
            &format!("Color(Gradient((colors: {stops}, radius: 45.0)))"),
        ),
    );
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    assert!(!editor.dirty(), "opening never truncates");
    editor.update(Message::Tab(Tab::Colors));
    editor.update(Message::Stop(0, "#FFFFFF".into()));
    assert!(
        !editor.dirty(),
        "stops beyond three aren't edited one by one"
    );
    editor.update(Message::Direction(90));
    let BgSource::Color(BgColor::Gradient(gradient)) = source(&editor) else {
        panic!("gradient");
    };
    assert_eq!(gradient.colors.len(), 5, "direction keeps every stop");
    assert_eq!(gradient.radius, 90.);
    editor.update(Message::Discard);
    editor.update(Message::Stops(2));
    let BgSource::Color(BgColor::Gradient(gradient)) = source(&editor) else {
        panic!("gradient");
    };
    assert_eq!(
        gradient.colors,
        vec![[0.1, 0.2, 0.3], [0.2, 0.3, 0.4]],
        "exact values"
    );
    assert_eq!(gradient.radius, 45., "custom angle kept");
}

#[test]
fn collection_changes_write_only_the_collection_file() {
    let fixture = seeded("wallpaper-page-collection");
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    let picture = fixture.0.join("slides/one.png");
    let before = fixture.tree();
    let Outcome::Dialog(request) = editor.update(Message::AddPicture) else {
        panic!("dialog");
    };
    assert!(!request.folder);
    editor.update(Message::Picked(request.id, Ok(Some(picture.clone()))));
    let file = fixture.0.join("state").join(collection::FILE);
    let changed: Vec<PathBuf> = fixture
        .tree()
        .into_iter()
        .filter(|(path, contents)| before.get(path) != Some(contents))
        .map(|(path, _)| path)
        .collect();
    assert_eq!(changed, vec![fixture.0.join("state"), file.clone()]);
    assert_eq!(editor.tab, Tab::Yours);
    assert_eq!(
        source(&editor),
        BgSource::Path(picture.clone()),
        "selected (draft only)"
    );
    editor.update(Message::Remove(picture.clone()));
    assert!(
        Collection::load(&fixture.store())
            .unwrap()
            .items()
            .is_empty()
    );

    // Preview: Add works for the session only.
    let preview = Fixture::new("wallpaper-page-collection-preview");
    fs::write(preview.0.join("outputs"), "DP-1\n").unwrap();
    let before = preview.tree();
    let mut editor = WallpaperEditor::with_store(preview.store().read_only("Preview"));
    drive(&mut editor);
    let Outcome::Dialog(request) = editor.update(Message::AddFolder) else {
        panic!("dialog");
    };
    editor.update(Message::Picked(
        request.id,
        Ok(Some(fixture.0.join("slides"))),
    ));
    assert_eq!(
        editor.notice(),
        Some(&Notice::Done(ADDED_IN_PREVIEW.into()))
    );
    assert!(
        editor
            .tiles()
            .iter()
            .any(|tile| tile.pick == Pick::Folder(fixture.0.join("slides")))
    );
    assert_eq!(
        editor.update(Message::Reinstall),
        Outcome::None,
        "refused in preview"
    );
    assert_eq!(preview.tree(), before);
}

// Review R1 regressions.

/// Like `drive`, checking the media bounds after every step; returns the
/// jobs it ran.
fn drive_bounded(editor: &mut WallpaperEditor) -> Vec<Job> {
    let mut ran = Vec::new();
    for _ in 0..256 {
        let jobs = editor.take_jobs();
        if jobs.is_empty() {
            return ran;
        }
        for job in jobs {
            assert!(editor.decoding.len() <= DECODES, "decodes in flight");
            ran.push(job.clone());
            editor.update(job.run());
            assert!(editor.thumbs.len() <= CACHE, "cache bound");
        }
    }
    panic!("background work did not settle");
}
fn gradient(colors: &str, radius: f32) -> String {
    entry(
        "all",
        &format!("Color(Gradient((colors: {colors}, radius: {radius:.1})))"),
    )
}
fn stops(editor: &WallpaperEditor) -> (Vec<String>, f32) {
    let BgSource::Color(BgColor::Gradient(gradient)) = source(editor) else {
        panic!("gradient: {:?}", source(editor));
    };
    (
        gradient
            .colors
            .iter()
            .map(|rgb| hex(rgb.map(wallpaper::linear_to_srgb)))
            .collect(),
        gradient.radius,
    )
}

/// I-1: every picture of a large tab gets a thumbnail, within the bounds.
#[test]
fn gallery_pages_reach_every_picture_within_bounds() {
    let fixture = seeded("wallpaper-page-many");
    for index in 0..44u8 {
        png(
            &fixture.0.join(format!("backgrounds/many/{index:02}.png")),
            [index * 5, 80, 120],
        );
    }
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive_bounded(&mut editor);
    editor.update(Message::Tab(Tab::Cosmic));
    drive_bounded(&mut editor);
    // "All as slideshow" and 45 pictures: pages of 20.
    assert_eq!((editor.tiles().len(), editor.pages()), (46, 3));
    assert_eq!(editor.page_tiles().len(), PAGE);
    let mut loaded = BTreeSet::new();
    for page in 0..editor.pages() {
        editor.update(Message::Page(page));
        drive_bounded(&mut editor);
        for tile in editor.page_tiles() {
            if let Pick::Image(path) = &tile.pick {
                assert!(editor.thumb(tile).is_some(), "{path:?} has a thumbnail");
                loaded.insert(path.clone());
            }
        }
    }
    assert_eq!(loaded.len(), 45, "every picture is reachable");
    // Returning to a page settles from the cache.
    editor.update(Message::Page(0));
    let again = drive_bounded(&mut editor);
    assert!(
        !again.iter().any(|job| matches!(job, Job::Thumb { .. })),
        "{again:?}"
    );
    // A page past the end clamps; another tab starts on its first page; a
    // tab that shrinks clamps.
    editor.update(Message::Page(99));
    assert_eq!(editor.page(), 2);
    editor.update(Message::Tab(Tab::Colors));
    assert_eq!(editor.page(), 0);
    editor.update(Message::Tab(Tab::Cosmic));
    editor.update(Message::Page(2));
    fs::remove_dir_all(fixture.0.join("backgrounds/many")).unwrap();
    editor.update(Message::Refresh);
    drive_bounded(&mut editor);
    assert_eq!((editor.page(), editor.pages()), (0, 1));
    let images: Vec<Tile> = editor
        .tiles()
        .iter()
        .filter(|tile| matches!(tile.pick, Pick::Image(_)))
        .cloned()
        .collect();
    assert_eq!(images.len(), 1);
    let waiting: Vec<&Tile> = images
        .iter()
        .filter(|tile| editor.thumb(tile).is_none() && editor.thumb_note(tile).is_none())
        .collect();
    assert!(waiting.is_empty(), "{} tiles never load", waiting.len());
}

/// M-1: more folder tiles than kept scans settle.
#[test]
fn many_folder_tiles_settle() {
    let fixture = seeded("wallpaper-page-folders");
    let mut collection = Collection::default();
    for index in 0..33 {
        let folder = fixture.0.join(format!("folders/{index:02}"));
        png(&folder.join("a.png"), [10, 20, 30]);
        assert!(collection.add(Item {
            kind: Kind::Folder,
            path: folder,
        }));
    }
    collection.save(&fixture.store()).unwrap();
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    editor.update(Message::Tab(Tab::Yours));
    let last = fixture.0.join("folders/32");
    editor.update(Message::Select(Pick::Folder(last.clone())));
    let ran = drive_bounded(&mut editor);
    let scans = |ran: &[Job]| {
        ran.iter()
            .filter(|job| matches!(job, Job::Scan { .. }))
            .count()
    };
    // Page 0's 20 folders and the selected one on page 1.
    assert_eq!((editor.page(), editor.pages()), (0, 2));
    assert_eq!(scans(&ran), 21);
    assert!(
        editor.scan(&last).is_some(),
        "the selected folder is checked"
    );
    assert!(editor.shown().folder);
    assert_eq!(editor.source_problem(), None);
    assert!(editor.can_apply());
    editor.update(Message::Page(1));
    assert_eq!(scans(&drive_bounded(&mut editor)), 12);
    editor.update(Message::Page(0));
    assert_eq!(scans(&drive_bounded(&mut editor)), 0, "settled");

    // A draft over the display limit (blocked anyway) wants more folders
    // than are kept; the scans still end.
    let names: Vec<String> = (1..=40).map(|index| format!("DP-{index}")).collect();
    fs::write(fixture.0.join("outputs"), names.join("\n")).unwrap();
    editor.update(Message::Refresh);
    editor.update(Message::Same(false));
    for (index, name) in names.iter().enumerate() {
        let folder = fixture.0.join(format!("over/{index:02}"));
        png(&folder.join("a.png"), [10, 20, 30]);
        editor.update(Message::Display(Target::Output(name.clone())));
        editor.update(Message::Select(Pick::Folder(folder)));
    }
    assert!(scans(&drive_bounded(&mut editor)) <= SCANS);
    assert!(!editor.can_apply(), "more than 16 displays");
    assert!(editor.scans.len() <= SCANS);
}

/// M-3: unfinished stop text rebases onto an outside change of the other
/// stops and the angle.
#[test]
fn unfinished_stop_rebases_onto_outside_change() {
    let fixture = seeded("wallpaper-page-rebase");
    fixture.user(
        NAMESPACE,
        "all",
        &gradient("[(0.1, 0.2, 0.3), (0.4, 0.5, 0.6)]", 180.),
    );
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    editor.update(Message::Tab(Tab::Colors));
    editor.update(Message::Stop(0, "#12".into()));
    assert!(editor.typed_error().is_some());
    fixture.user(
        NAMESPACE,
        "all",
        &gradient("[(0.1, 0.2, 0.3), (0.7, 0.8, 0.9)]", 90.),
    );
    editor.update(Message::Refresh);
    let outside = hex([0.7f32, 0.8, 0.9].map(wallpaper::linear_to_srgb));
    assert_eq!(editor.custom().unwrap().text(0), "#12", "unfinished kept");
    assert_eq!(editor.custom().unwrap().text(1), outside, "field rebased");
    editor.update(Message::Stop(0, "#123456".into()));
    assert_eq!(stops(&editor), (vec!["#123456".into(), outside], 90.));
}

/// M-3: Refresh keeps a real pending edit of another stop, made while the
/// first is unfinished, also over an outside change of the angle.
#[test]
fn refresh_keeps_pending_stop_edit_behind_unfinished_text() {
    let fixture = seeded("wallpaper-page-pending");
    fixture.user(
        NAMESPACE,
        "all",
        &gradient("[(0.1, 0.2, 0.3), (0.4, 0.5, 0.6)]", 180.),
    );
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    editor.update(Message::Tab(Tab::Colors));
    editor.update(Message::Stop(0, "#12".into()));
    editor.update(Message::Stop(1, "#ABCDEF".into()));
    editor.update(Message::Refresh);
    assert_eq!(editor.custom().unwrap().text(1), "#ABCDEF");
    fixture.user(
        NAMESPACE,
        "all",
        &gradient("[(0.1, 0.2, 0.3), (0.4, 0.5, 0.6)]", 270.),
    );
    editor.update(Message::Refresh);
    editor.update(Message::Stop(0, "#123456".into()));
    assert_eq!(
        stops(&editor),
        (vec!["#123456".into(), "#ABCDEF".into()], 270.)
    );
}

/// M-3: Use current accepts the outside values; the replaced draft values
/// don't come back when the unfinished stop is completed.
#[test]
fn use_current_does_not_resurrect_replaced_stops() {
    let fixture = seeded("wallpaper-page-use-current");
    fixture.user(
        NAMESPACE,
        "all",
        &gradient("[(0.1, 0.2, 0.3), (0.4, 0.5, 0.6)]", 180.),
    );
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    editor.update(Message::Tab(Tab::Colors));
    editor.update(Message::Stop(1, "#445566".into()));
    assert!(editor.dirty() && editor.typed_error().is_none());
    editor.update(Message::Stop(0, "#12".into()));
    fixture.user(
        NAMESPACE,
        "all",
        &gradient("[(0.2, 0.2, 0.2), (0.9, 0.9, 0.9)]", 270.),
    );
    editor.update(Message::Refresh);
    assert_eq!(editor.draft().unwrap().conflicts(), &[shared_key()]);
    assert_eq!(editor.custom().unwrap().text(1), "#445566", "still pending");
    editor.update(Message::UseCurrent);
    let outside = hex([0.9f32, 0.9, 0.9].map(wallpaper::linear_to_srgb));
    assert_eq!(editor.custom().unwrap().text(0), "#12");
    assert_eq!(editor.custom().unwrap().text(1), outside);
    editor.update(Message::Stop(0, "#123456".into()));
    assert_eq!(stops(&editor), (vec!["#123456".into(), outside], 270.));
}

/// M-4: typed state of a display the page no longer reaches doesn't block
/// Apply: Same on all displays, a display that disappears, a letterbox
/// field whose background stopped being a picture.
#[test]
fn unreachable_typed_state_is_pruned() {
    let fixture = seeded("wallpaper-page-prune");
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    editor.update(Message::Fit(FitChoice::Stretch));
    editor.update(Message::Same(false));
    editor.update(Message::Display(Target::Output("DP-1".into())));
    editor.update(Message::Select(Pick::Image(
        fixture.0.join("slides/one.png"),
    )));
    editor.update(Message::Fit(FitChoice::Fit));
    editor.update(Message::FitColor("#zz".into()));
    editor.update(Message::Tab(Tab::Colors));
    editor.update(Message::Stop(0, "#12".into()));
    editor.update(Message::Display(Target::Shared));
    assert_eq!(
        editor.typed_error().as_deref(),
        Some("Finish the color code (#RRGGBB) for DP-1 or pick a color"),
        "another display's text stays and is named"
    );
    editor.update(Message::Same(true));
    drive(&mut editor);
    assert_eq!(editor.typed_error(), None);
    assert!(editor.can_apply(), "the shared edit applies");

    // A display that disappears takes its typed text along.
    editor.update(Message::Same(false));
    editor.update(Message::Display(Target::Output("HDMI-1".into())));
    editor.update(Message::Stop(0, "#12".into()));
    assert!(editor.typed_error().is_some());
    fs::write(fixture.0.join("outputs"), "DP-1\n").unwrap();
    editor.update(Message::Refresh);
    assert_eq!(*editor.target(), Target::Shared);
    assert_eq!(editor.typed_error(), None);

    // A letterbox field whose background became a colour.
    let fixture = seeded("wallpaper-page-prune-fit");
    let current = fixture.0.join("home/current.png");
    fixture.user(
        NAMESPACE,
        "all",
        &format!(
            "(output: \"all\", source: Path(\"{}\"), filter_by_theme: false, rotation_frequency: 900, scaling_mode: Fit((0.0, 0.0, 0.0)))",
            current.display()
        ),
    );
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    editor.update(Message::FitColor("#zz".into()));
    assert!(editor.typed_error().is_some() && !editor.core.dirty());
    fixture.user(
        NAMESPACE,
        "all",
        &entry("all", "Color(Single((0.9, 0.1, 0.1)))"),
    );
    editor.update(Message::Refresh);
    assert_eq!(editor.typed_error(), None);
}

/// M-6: a missing current background is shown as missing (also a bundled
/// one) and warns without blocking an unrelated edit.
#[test]
fn missing_current_background_is_truthful_and_does_not_block() {
    let fixture = seeded("wallpaper-page-missing-current");
    let amber = fixture.0.join("pictures/Desktop Cards/amber.jpg");
    fixture.user(
        NAMESPACE,
        "all",
        &entry("all", &format!("Path(\"{}\")", amber.display())),
    );
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    assert_eq!(editor.shown().note, Some(media::MISSING));
    assert!(editor.shown().handle.is_none(), "no healthy preview");
    assert!(
        editor.shown().warning && editor.shown().bundled,
        "Reinstall"
    );
    let tile = Tile {
        pick: Pick::Bundled("amber".into()),
        label: String::new(),
        missing: true,
        external: false,
    };
    assert!(editor.thumb(&tile).is_some(), "the tile keeps its choice");
    editor.update(Message::Fit(FitChoice::Stretch));
    drive(&mut editor);
    assert!(editor.can_apply(), "an unrelated edit applies");
    assert_eq!(editor.source_problem(), None);
    assert!(editor.shown().warning);

    // A newly selected missing picture blocks (one banner, no warning).
    editor.update(Message::Select(Pick::Image(fixture.0.join("gone.png"))));
    drive(&mut editor);
    assert!(!editor.can_apply());
    assert_eq!(
        editor.source_problem(),
        Some("This picture was moved or deleted")
    );
    assert!(!editor.shown().warning && !editor.shown().bundled);
    assert_eq!(editor.shown().note, Some(media::MISSING));
}

/// M-5: only items of the app's collection offer Remove; a configured or
/// COSMIC Settings picture is no member. Add shows its tile's page.
#[test]
fn remove_is_offered_only_for_collection_members() {
    let fixture = seeded("wallpaper-page-members");
    let external = fixture.0.join("home/external.png");
    png(&external, [1, 2, 3]);
    fixture.user(
        "com.system76.CosmicSettings.Wallpaper",
        "custom-images",
        &format!("[\"{}\"]", external.display()),
    );
    let mut collection = Collection::default();
    for index in 0..25 {
        assert!(collection.add(Item {
            kind: Kind::Image,
            path: fixture.0.join(format!("saved/{index:02}.png")),
        }));
    }
    collection.save(&fixture.store()).unwrap();
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    editor.update(Message::Tab(Tab::Yours));
    let current = fixture.0.join("home/current.png");
    let yours: Vec<PathBuf> = editor
        .tiles()
        .iter()
        .filter_map(|tile| match &tile.pick {
            Pick::Image(path) => Some(path.clone()),
            _ => None,
        })
        .collect();
    assert!(yours.contains(&current) && yours.contains(&external));
    assert!(!editor.in_collection(&current), "configured only");
    assert!(!editor.in_collection(&external), "COSMIC Settings' list");
    assert!(editor.in_collection(&fixture.0.join("saved/00.png")));
    // Add selects the picture and shows the page holding its tile.
    let added = fixture.0.join("slides/one.png");
    let Outcome::Dialog(request) = editor.update(Message::AddPicture) else {
        panic!("dialog");
    };
    editor.update(Message::Picked(request.id, Ok(Some(added.clone()))));
    assert!(editor.in_collection(&added));
    assert_eq!(editor.page(), 1);
    assert!(
        editor
            .page_tiles()
            .iter()
            .any(|tile| tile.pick == Pick::Image(added.clone()))
    );
    // A session (preview) item is a member too.
    let preview = seeded("wallpaper-page-members-preview");
    let mut editor = WallpaperEditor::with_store(preview.store().read_only("Preview"));
    drive(&mut editor);
    let Outcome::Dialog(request) = editor.update(Message::AddPicture) else {
        panic!("dialog");
    };
    editor.update(Message::Picked(request.id, Ok(Some(added.clone()))));
    assert!(editor.session() && editor.in_collection(&added));
    assert!(!editor.in_collection(&preview.0.join("home/current.png")));
}

#[test]
fn an_animated_picture_is_marked_as_shown_still_and_checked_once() {
    let fixture = seeded("wallpaper-page-animated");
    let moving = fixture.0.join("home/moving.webp");
    let mut bytes = b"RIFF\x12\0\0\0WEBPVP8X\x0a\0\0\0".to_vec();
    bytes.extend([0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    fs::write(&moving, bytes).unwrap();
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    assert!(!editor.shown().animated, "the still PNG shows as it is");
    editor.update(Message::Select(Pick::Image(moving.clone())));
    let jobs = drive_bounded(&mut editor);
    assert_eq!(
        jobs.iter()
            .filter(|job| matches!(job, Job::Motion { path } if *path == moving))
            .count(),
        1
    );
    assert!(editor.shown().animated);
    assert_eq!(editor.shown().note, Some(media::NO_PREVIEW));
    // Checked once per file version; a slideshow folder is never marked.
    editor.update(Message::Refresh);
    assert!(
        !drive_bounded(&mut editor)
            .iter()
            .any(|job| matches!(job, Job::Motion { .. }))
    );
    editor.update(Message::Select(Pick::Folder(fixture.0.join("slides"))));
    drive(&mut editor);
    assert!(!editor.shown().animated);
}

#[test]
fn automatic_colors_advanced_manual_apply_wins_for_the_current_wallpaper() {
    let fixture = seeded("automatic-advanced");
    let path = fixture.0.join("config.json");
    fs::write(&path, r#"{"appearance":{"wallpaper_colors":true}}"#).unwrap();
    let store = fixture.store().with_app_config(path.clone());
    automatic::commit(&store, automatic::prepare(&store).unwrap().unwrap()).unwrap();
    let mut editor = AppearanceEditor::with_store(fixture.store().with_app_config(path));
    editor.update(system_appearance::Message::Color(
        ColorRole::Accent,
        crate::desktop::appearance::parse_hex("#D34762", false),
    ));
    fixture.user(
        NAMESPACE,
        "all",
        &entry("all", "Color(Single((0.1, 0.6, 0.4)))"),
    );
    editor.update(system_appearance::Message::Apply);
    assert!(!editor.dirty(), "{:?}", editor.notice);
    assert!(
        automatic::prepare(&store).unwrap().is_none(),
        "applied advanced color wins after a held wallpaper change"
    );
    editor.update(system_appearance::Message::Undo);
    assert!(
        automatic::prepare(&store).unwrap().is_none(),
        "Undo also stays put"
    );
    fixture.user(
        NAMESPACE,
        "all",
        &entry("all", "Color(Single((0.6, 0.2, 0.7)))"),
    );
    assert!(
        automatic::prepare(&store).unwrap().is_some(),
        "next wallpaper resumes the link"
    );
}

#[test]
fn automatic_colors_revisits_prior_wallpaper_after_advanced_manual_apply() {
    let fixture = seeded("automatic-revisit-advanced");
    let path = fixture.0.join("config.json");
    fs::write(&path, r#"{"appearance":{"wallpaper_colors":true}}"#).unwrap();
    let store = fixture.store().with_app_config(path.clone());
    let original = fs::read_to_string(store.user_path(&shared_key())).unwrap();
    automatic::commit(&store, automatic::prepare(&store).unwrap().unwrap()).unwrap();
    let mut editor = AppearanceEditor::with_store(fixture.store().with_app_config(path));
    editor.update(system_appearance::Message::Color(
        ColorRole::Accent,
        crate::desktop::appearance::parse_hex("#D34762", false),
    ));
    fixture.user(
        NAMESPACE,
        "all",
        &entry("all", "Color(Single((0.1, 0.6, 0.4)))"),
    );
    editor.update(system_appearance::Message::Apply);
    assert!(!editor.dirty(), "{:?}", editor.notice);
    assert!(automatic::prepare(&store).unwrap().is_none());
    drop(editor);
    fixture.user(NAMESPACE, "all", &original);
    let plan = automatic::prepare(&store).unwrap().expect(
        "an old config stamp must not preserve an advanced override after another wallpaper",
    );
    automatic::commit(&store, plan).unwrap();
    let appearance = AppearanceDraft::load(&store).unwrap();
    for variant in [
        crate::desktop::appearance::ThemeVariant::Light,
        crate::desktop::appearance::ThemeVariant::Dark,
    ] {
        let color = appearance
            .color(variant, ColorRole::Accent)
            .known()
            .unwrap()
            .unwrap();
        assert_eq!(
            [color.red, color.green, color.blue].map(|v| (v * 255.).round() as u8),
            if variant == crate::desktop::appearance::ThemeVariant::Dark {
                [211, 71, 98]
            } else {
                native(variant, [90, 90, 90])
            }
        );
    }
}

fn review_r1_soft_store(name: &str) -> (Fixture, DesktopStore) {
    let fixture = seeded(name);
    let store = fixture
        .store()
        .with_app_config(fixture.0.join("config.json"));
    fs::write(store.app_config().unwrap(), "{}").unwrap();
    let p = crate::desktop::profiles::themes::record(crate::desktop_theme::Family::Soft);
    let plan = crate::desktop::profiles::plan(
        &store,
        &crate::desktop::profiles::Installed::discover(&store),
        &p,
        &p.domains().into_iter().collect(),
    );
    crate::desktop::profiles::apply(&store, &plan).unwrap();
    (fixture, store)
}

#[test]
fn review_r1_shared_color_draft_must_restore_after_close() {
    let (fixture, store) = review_r1_soft_store("review-r1-wallpaper-recovery");
    let mut editor = WallpaperEditor::with_store(
        fixture
            .store()
            .with_app_config(fixture.0.join("config.json")),
    );
    drive(&mut editor);
    editor.update(Message::AccentText("#FF0000".into()));
    editor.update(Message::AccentSubmit);
    assert!(editor.can_apply());
    let saved = editor.session_draft().expect("a pending color choice");
    let serialized = ron::to_string(&saved).unwrap();
    let restored = ron::from_str(&serialized).unwrap();
    let baseline = saved.accent.as_ref().unwrap().snapshot().clone();
    let mut reopened = WallpaperEditor::with_store(
        fixture
            .store()
            .with_app_config(fixture.0.join("config.json")),
    );
    let result = reopened.restore_session(restored);
    assert!(
        result.is_ok(),
        "a native generated theme-color draft must survive reopening: {result:?}"
    );
    assert_eq!(reopened.accent.as_ref().unwrap().snapshot(), &baseline);
    drive(&mut reopened);
    // A second choice after recovery must still own the same linked roles.
    reopened.update(Message::AccentText("#0000FF".into()));
    reopened.update(Message::AccentSubmit);
    assert!(reopened.can_apply(), "{:?}", reopened.notice());
    reopened.update(Message::Apply);
    assert!(!reopened.dirty(), "{:?}", reopened.notice());
    assert_eq!(
        automatic::read_config(&store).unwrap().1.appearance.accent,
        Some([0, 0, 255])
    );
    assert_linked_surfaces(&store, [0, 0, 255]);
}

#[test]
fn review_r1_second_shared_color_must_update_same_linked_surfaces() {
    let (fixture, store) = review_r1_soft_store("review-r1-color-repeat");
    let mut editor = WallpaperEditor::with_store(
        fixture
            .store()
            .with_app_config(fixture.0.join("config.json")),
    );
    drive(&mut editor);
    editor.update(Message::AccentText("#FF0000".into()));
    editor.update(Message::AccentSubmit);
    editor.update(Message::AccentText("#0000FF".into()));
    editor.update(Message::AccentSubmit);
    assert!(editor.can_apply());
    editor.update(Message::Apply);
    assert!(!editor.dirty(), "{:?}", editor.notice());
    let (_, config) = automatic::read_config(&store).unwrap();
    assert_eq!(config.appearance.accent, Some([0, 0, 255]));
    let actual = AppearanceDraft::load(&store)
        .unwrap()
        .color(ThemeVariant::Dark, ColorRole::Background)
        .known()
        .copied()
        .flatten()
        .unwrap();
    let expected = crate::desktop::profiles::themes::appearance(
        crate::desktop_theme::Family::Soft,
        Some([0, 0, 255]),
    )
    .dark
    .unwrap()
    .background
    .unwrap()
    .color(ColorRole::Background)
    .unwrap()
    .unwrap();
    assert!(
        (actual.red - expected.red).abs() < 0.005
            && (actual.green - expected.green).abs() < 0.005
            && (actual.blue - expected.blue).abs() < 0.005,
        "second choice left the first choice's native background: actual {actual:?}, expected {expected:?}"
    );
}

fn assert_linked_surfaces(store: &DesktopStore, rgb: [u8; 3]) {
    let draft = AppearanceDraft::load(store).unwrap();
    let config = automatic::read_config(store).unwrap().1;
    let expected =
        crate::desktop::profiles::themes::appearance(config.appearance.family.unwrap(), Some(rgb));
    for variant in ThemeVariant::ALL {
        let expected = expected.variant(variant).unwrap();
        for (role, color) in [
            (ColorRole::Background, &expected.background),
            (ColorRole::Primary, &expected.primary),
        ] {
            if let Some(color) = color {
                assert_eq!(
                    draft.color(variant, role).known().copied(),
                    Some(color.color(role).unwrap())
                );
            }
        }
    }
}

#[test]
fn review_r1_repeated_swatches_and_hex_keep_custom_surfaces_and_automatic_updates() {
    let (fixture, store) = review_r1_soft_store("review-r1-repeated-custom");
    let mut custom = AppearanceDraft::load(&store).unwrap();
    let custom_color = crate::desktop::appearance::parse_hex("#315274", true).unwrap();
    custom.set(
        Control::Color(ThemeVariant::Dark, ColorRole::Primary),
        crate::desktop::appearance::Value::Color(Some(custom_color)),
    );
    store.apply(&custom.plan().patch).unwrap();
    let original_primary = AppearanceDraft::load(&store)
        .unwrap()
        .color(ThemeVariant::Dark, ColorRole::Primary);
    let mut editor = WallpaperEditor::with_store(
        fixture
            .store()
            .with_app_config(fixture.0.join("config.json")),
    );
    drive(&mut editor);
    for text in ["#FF0000", "#0000FF", "#00FF00"] {
        editor.update(Message::AccentText(text.into()));
        editor.update(Message::AccentSubmit);
    }
    // Use actual suggested swatches, then another hex choice in the same draft.
    editor.update(Message::Select(Pick::Color(BgColor::Single([
        0.8, 0.2, 0.4,
    ]))));
    drive(&mut editor);
    let swatch = editor.suggestion().unwrap().swatches[0].rgb;
    editor.update(Message::Accent(swatch));
    editor.update(Message::AccentText("#0000FF".into()));
    editor.update(Message::AccentSubmit);
    let saved = ron::from_str(&ron::to_string(&editor.session_draft().unwrap()).unwrap()).unwrap();
    let mut reopened = WallpaperEditor::with_store(
        fixture
            .store()
            .with_app_config(fixture.0.join("config.json")),
    );
    reopened.restore_session(saved).unwrap();
    drive(&mut reopened);
    reopened.update(Message::AccentText("#00FF00".into()));
    reopened.update(Message::AccentSubmit);
    reopened.update(Message::Apply);
    assert!(!reopened.dirty(), "{:?}", reopened.notice());
    let applied = AppearanceDraft::load(&store).unwrap();
    assert_eq!(
        applied.color(ThemeVariant::Dark, ColorRole::Primary),
        original_primary
    );
    let expected = crate::desktop::profiles::themes::appearance(
        crate::desktop_theme::Family::Soft,
        Some([0, 255, 0]),
    );
    assert_eq!(
        applied
            .color(ThemeVariant::Dark, ColorRole::Background)
            .known()
            .copied(),
        Some(
            expected
                .dark
                .unwrap()
                .background
                .unwrap()
                .color(ColorRole::Background)
                .unwrap()
        )
    );
    reopened.update(Message::AutomaticColors(true));
    drive(&mut reopened);
    reopened.update(Message::Apply);
    assert!(!reopened.dirty(), "{:?}", reopened.notice());
    drop(reopened);
    fixture.user(
        NAMESPACE,
        "all",
        &entry("all", "Color(Single((0.2, 0.1, 0.7)))"),
    );
    automatic::commit(&store, automatic::prepare(&store).unwrap().unwrap()).unwrap();
    let config = automatic::read_config(&store).unwrap().1;
    let expected = crate::desktop::profiles::themes::appearance(
        crate::desktop_theme::Family::Soft,
        config.appearance.accent,
    );
    let applied = AppearanceDraft::load(&store).unwrap();
    assert_eq!(
        applied.color(ThemeVariant::Dark, ColorRole::Primary),
        original_primary
    );
    assert_eq!(
        applied
            .color(ThemeVariant::Dark, ColorRole::Background)
            .known()
            .copied(),
        Some(
            expected
                .dark
                .unwrap()
                .background
                .unwrap()
                .color(ColorRole::Background)
                .unwrap()
        )
    );
}

#[test]
fn review_r1_recovery_rejects_unowned_roles_and_preserves_external_conflicts() {
    let (fixture, store) = review_r1_soft_store("review-r1-recovery-boundary");
    let mut editor = WallpaperEditor::with_store(
        fixture
            .store()
            .with_app_config(fixture.0.join("config.json")),
    );
    drive(&mut editor);
    editor.update(Message::AccentText("#FF0000".into()));
    editor.update(Message::AccentSubmit);
    let saved = editor.session_draft().unwrap();
    for control in [
        Control::OutlineWidth,
        Control::Color(ThemeVariant::Dark, ColorRole::Secondary),
        Control::Color(ThemeVariant::Light, ColorRole::Secondary),
    ] {
        let mut unrelated: SavedWallpaper =
            ron::from_str(&ron::to_string(&saved).unwrap()).unwrap();
        unrelated.accent.as_mut().unwrap().set(
            control,
            if control == Control::OutlineWidth {
                crate::desktop::appearance::Value::Number(4)
            } else {
                crate::desktop::appearance::Value::Color(crate::desktop::appearance::parse_hex(
                    "#123456", true,
                ))
            },
        );
        let mut reopened = WallpaperEditor::with_store(
            fixture
                .store()
                .with_app_config(fixture.0.join("config.json")),
        );
        assert!(reopened.restore_session(unrelated).is_err(), "{control:?}");
    }
    let mut external = AppearanceDraft::load(&store).unwrap();
    external.set(
        Control::Color(ThemeVariant::Dark, ColorRole::Background),
        crate::desktop::appearance::Value::Color(crate::desktop::appearance::parse_hex(
            "#123456", true,
        )),
    );
    store.apply(&external.plan().patch).unwrap();
    let before = fs::read(store.app_config().unwrap()).unwrap();
    let mut reopened = WallpaperEditor::with_store(
        fixture
            .store()
            .with_app_config(fixture.0.join("config.json")),
    );
    reopened.restore_session(saved).unwrap();
    reopened.update(Message::AccentText("#0000FF".into()));
    reopened.update(Message::AccentSubmit);
    reopened.update(Message::Apply);
    assert!(reopened.dirty());
    assert!(!reopened.accent_conflicts().is_empty());
    assert_eq!(fs::read(store.app_config().unwrap()).unwrap(), before);
}

#[test]
fn review_r1_keep_my_colors_adopts_the_current_family_without_replacing_its_geometry() {
    let (fixture, store) = review_r1_soft_store("review-r1-keep-colors-family");
    let mut editor = WallpaperEditor::with_store(
        fixture
            .store()
            .with_app_config(fixture.0.join("config.json")),
    );
    drive(&mut editor);
    editor.update(Message::AccentText("#0000FF".into()));
    editor.update(Message::AccentSubmit);
    let saved = ron::from_str(&ron::to_string(&editor.session_draft().unwrap()).unwrap()).unwrap();
    let p = crate::desktop::profiles::themes::record(crate::desktop_theme::Family::Paper);
    let plan = crate::desktop::profiles::plan(
        &store,
        &crate::desktop::profiles::Installed::discover(&store),
        &p,
        &p.domains().into_iter().collect(),
    );
    crate::desktop::profiles::apply(&store, &plan).unwrap();
    let mut reopened = WallpaperEditor::with_store(
        fixture
            .store()
            .with_app_config(fixture.0.join("config.json")),
    );
    reopened.restore_session(saved).unwrap();
    assert!(!reopened.can_apply());
    reopened.update(Message::KeepColors);
    assert_eq!(
        reopened.colors.as_ref().unwrap().family,
        Some(crate::desktop_theme::Family::Paper)
    );
    drive(&mut reopened);
    reopened.update(Message::Apply);
    assert!(!reopened.dirty(), "{:?}", reopened.notice());
    let config = automatic::read_config(&store).unwrap().1;
    assert_eq!(
        config.appearance.family,
        Some(crate::desktop_theme::Family::Paper)
    );
    assert_eq!(config.appearance.corner_radius, 12);
    assert_linked_surfaces(&store, [0, 0, 255]);
}

#[test]
fn match_accent_links_the_window_outline_with_shared_colors_and_recovers() {
    use crate::desktop::appearance::{AppearanceDraft, Control, ThemeVariant, Value, parse_hex};
    let fixture = seeded("outline-match-accent");
    let app_config = fixture.0.join("config.json");
    fs::write(
        &app_config,
        r#"{"theme":"teal","appearance":{"sync_colors":true}}"#,
    )
    .unwrap();
    let hint = fixture
        .0
        .join("config/cosmic/com.system76.CosmicTheme.Dark.Builder/v2/window_hint");
    fs::create_dir_all(hint.parent().unwrap()).unwrap();
    fs::write(&hint, "Some(\"#63D0DFFF\")").unwrap();
    let store = || fixture.store().with_app_config(app_config.clone());
    let outline = |variant| {
        AppearanceDraft::load(&store())
            .unwrap()
            .color(variant, ColorRole::Outline)
            .known()
            .copied()
    };
    let stale = outline(ThemeVariant::Dark);
    let mut editor = WallpaperEditor::with_store(store());
    editor.open();
    drive(&mut editor);
    assert!(
        editor.outline_custom(),
        "a separate outline color is noticed"
    );
    assert!(!editor.dirty(), "opening only reads");
    editor.update(Message::LinkOutline(true));
    assert!(editor.outline_linked() && editor.can_apply());
    // Recovery keeps the link and refuses an unrelated outline color.
    let saved = ron::to_string(&editor.session_draft().unwrap()).unwrap();
    let mut recovered = WallpaperEditor::with_store(store());
    recovered
        .restore_session(ron::from_str(&saved).unwrap())
        .unwrap();
    assert!(recovered.outline_linked());
    let mut forged: SavedWallpaper = ron::from_str(&saved).unwrap();
    forged.accent.as_mut().unwrap().set(
        Control::Color(ThemeVariant::Dark, ColorRole::Outline),
        Value::Color(parse_hex("#FF0000", false)),
    );
    assert!(
        WallpaperEditor::with_store(store())
            .restore_session(forged)
            .is_err()
    );
    let before = fixture.tree();
    editor.update(Message::Apply);
    assert!(!editor.dirty(), "{:?}", editor.notice());
    for variant in [ThemeVariant::Light, ThemeVariant::Dark] {
        assert_eq!(
            outline(variant),
            Some(None),
            "{variant:?} follows the accent"
        );
    }
    assert!(!editor.outline_custom());
    editor.update(Message::Undo);
    assert_eq!(outline(ThemeVariant::Dark), stale);
    let theme_files = |tree: std::collections::BTreeMap<PathBuf, Option<Vec<u8>>>| {
        tree.into_iter()
            .filter(|(path, bytes)| {
                bytes.is_some() && path.to_string_lossy().contains("CosmicTheme")
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        theme_files(fixture.tree()),
        theme_files(before),
        "Undo restores the theme files exactly"
    );
}

/// On a grey wallpaper a theme keeps its own color whichever page changed
/// the wallpaper: Wallpaper & colors stores what Themes and the runtime use,
/// and an explicit adjustment for that wallpaper stays exact.
#[test]
fn a_grey_wallpaper_keeps_the_theme_color_on_wallpaper_and_colors() {
    use crate::desktop::appearance::{AppearanceDraft, ThemeVariant};
    use crate::desktop::profiles::{self, Installed, themes};
    use crate::desktop_theme::Family;
    let grey = BgColor::Single([0.8, 0.8, 0.8]);
    let suggestion = palette::from_color(&grey);
    assert!(suggestion.accents.is_empty() && !suggestion.swatches.is_empty());
    let soft = Family::Soft.accent();
    assert_eq!(
        automatic::recommended_for(&suggestion, Some(Family::Soft)),
        Some(soft),
        "what Themes and the runtime use"
    );
    for themed in [true, false] {
        let fixture = seeded(&format!("grey-wallpaper-theme-color-{themed}"));
        let config_path = fixture.0.join("config.json");
        fs::write(&config_path, r#"{"appearance":{"wallpaper_colors":true}}"#).unwrap();
        let store = || fixture.store().with_app_config(config_path.clone());
        if themed {
            let mut p = themes::record(Family::Soft);
            themes::LookColor::Fixed([94, 129, 172]).set(&mut p);
            themes::LookColor::Wallpaper.set(&mut p);
            let plan = profiles::plan(
                &store(),
                &Installed::discover(&store()),
                &p,
                &p.domains().into_iter().collect(),
            );
            profiles::apply(&store(), &plan).unwrap();
        }
        let expected = if themed {
            soft
        } else {
            automatic::recommended(&suggestion).unwrap()
        };
        let mut editor = WallpaperEditor::with_store(store());
        editor.open();
        drive(&mut editor);
        assert!(!editor.dirty(), "opening only reads");
        editor.update(Message::Select(Pick::Color(grey.clone())));
        drive(&mut editor);
        assert_eq!(editor.accent_color(), Some(expected), "themed: {themed}");
        // An explicit color for this wallpaper is kept through a refresh;
        // "Use recommended" gives the recommendation back.
        editor.update(Message::AccentText("#5E81AC".into()));
        editor.update(Message::AccentSubmit);
        editor.update(Message::Refresh);
        drive(&mut editor);
        assert_eq!(editor.accent_color(), Some([94, 129, 172]));
        editor.update(Message::SuggestAccent);
        assert_eq!(editor.accent_color(), Some(expected), "themed: {themed}");
        assert!(editor.can_apply(), "{:?}", editor.notice());
        editor.update(Message::Apply);
        assert!(!editor.dirty(), "{:?}", editor.notice());
        let (_, config) = automatic::read_config(&store()).unwrap();
        assert_eq!(config.appearance.accent, Some(expected), "themed: {themed}");
        assert!(!config.appearance.wallpaper_colors);
        // A theme's wallpaper text follows the light grey too.
        assert_eq!(config.appearance.wallpaper_light, themed);
        // The runtime has nothing left to correct, and COSMIC's accents are
        // the theme's tones of that color.
        assert!(automatic::prepare(&store()).unwrap().is_none());
        if themed {
            let native = AppearanceDraft::load(&store()).unwrap();
            for variant in ThemeVariant::ALL {
                assert!(themes::linked_accent(&native, config.appearance, variant));
            }
        }
    }
}

/// An outline color only COSMIC's derived theme holds (another tool wrote
/// it) is noticed, and Match accent relinks it in the same color Apply.
#[test]
fn match_accent_relinks_an_outline_only_the_derived_theme_colors() {
    use crate::desktop::appearance::{AppearanceDraft, ThemeVariant};
    let fixture = seeded("outline-match-accent-derived");
    let app_config = fixture.0.join("config.json");
    fs::write(
        &app_config,
        r#"{"theme":"teal","appearance":{"sync_colors":true}}"#,
    )
    .unwrap();
    let hint = fixture
        .0
        .join("config/cosmic/com.system76.CosmicTheme.Dark/v2/window_hint");
    fs::create_dir_all(hint.parent().unwrap()).unwrap();
    fs::write(
        &hint,
        "Some((red:0.3882353,green:0.8156863,blue:0.8745098))",
    )
    .unwrap();
    let original = fs::read(&hint).unwrap();
    let store = || fixture.store().with_app_config(app_config.clone());
    let follows = || {
        let draft = AppearanceDraft::load(&store()).unwrap();
        let preview = draft.preview(ThemeVariant::Dark);
        draft.outline_follows(ThemeVariant::Dark) && preview.outline == preview.accent
    };
    assert!(!follows());
    let mut editor = WallpaperEditor::with_store(store());
    editor.open();
    drive(&mut editor);
    assert!(editor.outline_custom(), "COSMIC draws that color");
    assert!(!editor.dirty(), "opening only reads");
    editor.update(Message::LinkOutline(true));
    assert!(editor.outline_linked() && editor.can_apply());
    editor.update(Message::Apply);
    assert!(!editor.dirty(), "{:?}", editor.notice());
    assert!(follows());
    assert!(!editor.outline_custom());
    editor.update(Message::Undo);
    assert_eq!(
        fs::read(&hint).unwrap(),
        original,
        "Undo restores it exactly"
    );
}

/// A theme draws wallpaper text for the wallpaper's polarity. A new
/// wallpaper chosen on Wallpaper & colors settles it in the same Apply and
/// Undo, as Themes and the runtime do, and a fixed color stays as it is.
#[test]
fn a_new_wallpaper_settles_the_theme_polarity_in_the_same_apply() {
    use crate::desktop::profiles::{self, Installed, themes};
    use crate::desktop_theme::Family;
    let fixture = seeded("theme-polarity-follows-wallpaper");
    let config_path = fixture.0.join("config.json");
    fs::write(&config_path, "{}").unwrap();
    let store = || fixture.store().with_app_config(config_path.clone());
    let mut p = themes::record(Family::Bare);
    themes::LookColor::Fixed([94, 129, 172]).set(&mut p);
    let plan = profiles::plan(
        &store(),
        &Installed::discover(&store()),
        &p,
        &p.domains().into_iter().collect(),
    );
    profiles::apply(&store(), &plan).unwrap();
    let config = || automatic::read_config(&store()).unwrap();
    assert!(
        !config().1.appearance.wallpaper_light,
        "the seeded picture is dark"
    );
    let before = config().0;
    let mut editor = WallpaperEditor::with_store(store());
    editor.open();
    drive(&mut editor);
    assert!(!editor.dirty(), "opening only reads");
    editor.update(Message::Select(Pick::Color(BgColor::Single([
        0.85, 0.85, 0.8,
    ]))));
    assert!(
        !editor.can_apply(),
        "Apply waits for the new wallpaper's analysis"
    );
    drive(&mut editor);
    assert!(editor.can_apply(), "{:?}", editor.notice());
    assert!(!editor.colors_dirty(), "a fixed color is not restaged");
    editor.update(Message::Apply);
    assert!(!editor.dirty(), "{:?}", editor.notice());
    let (_, applied) = config();
    assert!(
        applied.appearance.wallpaper_light,
        "dark ink on a light wallpaper"
    );
    assert_eq!(applied.appearance.accent, Some([94, 129, 172]));
    assert!(!applied.appearance.wallpaper_colors);
    assert!(
        automatic::prepare(&store()).unwrap().is_none(),
        "nothing left for the runtime to correct"
    );
    editor.update(Message::Undo);
    assert_eq!(
        config().0,
        before,
        "one Undo restores the wallpaper and its polarity"
    );
}

#[test]
fn repair_r1_retained_accent_submission_reloads_model_but_guards_later_writers() {
    let fixture = seeded("repair-accent-submit");
    let path = fixture.0.join("config.json");
    fs::write(
        &path,
        r#"{"appearance":{"accent":[20,30,40],"sync_colors":true}}"#,
    )
    .unwrap();
    let store = fixture.store().with_app_config(path.clone());
    let mut editor = WallpaperEditor::with_store(fixture.store().with_app_config(path.clone()));
    editor.update(Message::AccentText("#336699".into()));
    editor.update(Message::SyncColors(false));
    editor.update(Message::ApplyColors);
    drive(&mut editor);
    assert!(editor.colors.is_none());
    assert!(editor.valid_accent_input());
    editor.update(Message::AccentSubmit);
    // A same-field writer after the completed intent was prepared is refused.
    let (mut raw, _) = automatic::read_config(&store).unwrap();
    raw["appearance"]["accent"] = serde_json::json!([99, 88, 77]);
    fs::write(&path, serde_json::to_string(&raw).unwrap()).unwrap();
    editor.update(Message::ApplyColors);
    drive(&mut editor);
    assert_eq!(
        automatic::read_config(&store).unwrap().1.appearance.accent,
        Some([99, 88, 77])
    );
    assert!(editor.colors_conflict);
}

/// Moves folders' modification times a minute back, as if written earlier
/// (a folder written just now is rescanned until it settles).
fn settle(dirs: &[&Path]) {
    let earlier = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
    for dir in dirs {
        fs::File::open(dir).unwrap().set_modified(earlier).unwrap();
    }
}

/// The reported loop: a slideshow folder selected (not applied yet) went
/// back to "Checking the folder…" and "Loading…" on every refresh, and
/// each finished scan changed the watched picture, which refreshed again.
/// Now a folder is scanned once; refreshes (a cosmic-bg state rewrite, a
/// slideshow turn, the watch starting over) recheck it in the background,
/// keep the banner away and the preview shown, and only a real change to
/// the folder scans it again.
#[test]
fn refresh_keeps_a_checked_folder_and_rescans_only_a_changed_one() {
    let fixture = seeded("wallpaper-page-folder-cache");
    let folder = fixture.0.join("slideshow");
    // 172 pictures as in the report: three real ones first, then names
    // only (scans go by extension).
    for index in 0..3u8 {
        png(
            &folder.join(format!("{index:03}.png")),
            [40 * index, 90, 160],
        );
    }
    for index in 3..172 {
        fs::write(folder.join(format!("{index:03}.jpg")), "jpg").unwrap();
    }
    settle(&[&folder]);
    let store = fixture.store();
    let state = media::state_path(&store).unwrap();
    fs::create_dir_all(state.parent().unwrap()).unwrap();
    let applied = fixture.0.join("home/current.png");
    let state_text = format!("[\n    (\"DP-1\", Path(\"{}\")),\n]", applied.display());
    fs::write(&state, &state_text).unwrap();

    let mut editor = WallpaperEditor::with_store(store);
    drive(&mut editor);
    editor.update(Message::Select(Pick::Folder(folder.clone())));
    // The first look at a new folder is the only time the banner shows.
    assert_eq!(editor.source_problem(), Some("Checking the folder…"));
    let first = drive_bounded(&mut editor);
    assert_eq!(
        first
            .iter()
            .filter(|job| matches!(job, Job::Scan { path, .. } if *path == folder))
            .count(),
        1
    );
    let scan = editor.scan(&folder).unwrap().as_ref().unwrap().clone();
    assert_eq!((scan.images, scan.others), (172, 0));
    assert_eq!(editor.source_problem(), None);
    let shown = editor.shown().image.clone();
    assert_eq!(shown, Some(folder.join("000.png")));
    assert!(editor.shown().handle.is_some(), "the preview shows");
    assert_eq!(editor.shown().note, None);

    // cosmic-bg rewrites its state with the same content, and the watch
    // starts over whenever the shown picture changes: refresh many times.
    for _ in 0..5 {
        fs::write(&state, &state_text).unwrap();
        editor.update(Message::Refresh);
        assert_eq!(editor.source_problem(), None, "no banner on a recheck");
        assert_eq!(editor.shown().image, shown);
        assert!(editor.shown().handle.is_some(), "no Loading… on a recheck");
        let ran = drive_bounded(&mut editor);
        assert!(
            !ran.iter().any(|job| matches!(job, Job::Scan { .. })),
            "an unchanged folder is not scanned again: {ran:?}"
        );
        assert!(
            ran.iter()
                .any(|job| matches!(job, Job::Recheck { path, .. } if *path == folder)),
            "it is rechecked in the background"
        );
        assert_eq!(editor.scan(&folder).unwrap().as_ref().unwrap(), &scan);
        assert!(editor.dirty(), "the draft is kept");
    }

    // A real change: a picture added. The recheck scans again, and the
    // banner never shows meanwhile.
    fs::write(folder.join("172.jpg"), "jpg").unwrap();
    editor.update(Message::Refresh);
    assert_eq!(editor.source_problem(), None);
    let jobs = editor.take_jobs();
    let recheck = jobs
        .iter()
        .find(|job| matches!(job, Job::Recheck { path, .. } if *path == folder))
        .cloned()
        .expect("a background recheck");
    for job in jobs {
        if job != recheck {
            editor.update(job.run());
        }
    }
    let result = recheck.run();
    assert!(
        matches!(&result, Message::Rechecked(_, _, Some(Ok(scan))) if scan.images == 173),
        "{result:?}"
    );
    editor.update(result);
    drive(&mut editor);
    assert_eq!(editor.scan(&folder).unwrap().as_ref().unwrap().images, 173);
    assert_eq!(editor.source_problem(), None);
    assert_eq!(editor.shown().image, shown);
    assert!(editor.shown().handle.is_some());
}

/// A recheck of a folder that disappeared reports it, and one whose
/// result arrives for an older request is dropped like other results.
#[test]
fn recheck_reports_a_removed_folder_and_drops_stale_results() {
    let fixture = seeded("wallpaper-page-folder-gone");
    let folder = fixture.0.join("slides");
    settle(&[&folder]);
    let mut editor = WallpaperEditor::with_store(fixture.store());
    drive(&mut editor);
    editor.update(Message::Select(Pick::Folder(folder.clone())));
    drive(&mut editor);
    assert_eq!(editor.source_problem(), None);
    let scan = editor.scan(&folder).unwrap().clone();

    editor.update(Message::Rechecked(
        424_242,
        folder.clone(),
        Some(Ok(FolderScan::default())),
    ));
    assert_eq!(editor.scan(&folder), Some(&scan), "stale recheck dropped");

    fs::rename(&folder, fixture.0.join("moved")).unwrap();
    editor.update(Message::Refresh);
    drive(&mut editor);
    assert!(editor.source_problem().is_some(), "the move is reported");
    assert!(!editor.can_apply());
}
