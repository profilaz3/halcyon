use super::*;
use crate::desktop::appearance::SystemMode;
use crate::desktop::store::tests::Fixture;
use colors::Edit;
use serde_json::json;

fn fixture(name: &str) -> (Fixture, DesktopStore, ProfilesEditor) {
    let fixture = Fixture::new(name);
    fixture.system(
        "com.system76.CosmicPanel",
        "entries",
        "[\"Panel\",\"Dock\"]",
    );
    fixture.system(&PanelKind::Dock.namespace(), "anchor", "Bottom");
    fixture.system(&PanelKind::Dock.namespace(), "size", "L");
    fixture.system(&PanelKind::Panel.namespace(), "anchor", "Top");
    fixture.user("com.system76.CosmicTheme.Mode", "is_dark", "true");
    fixture.user("com.system76.CosmicTheme.Mode", "auto_switch", "false");
    let config = fixture.0.join("config.json");
    fs::write(&config, r#"{"theme":"teal","appearance":{"opacity":92},"future_field":73,"location":{"name":"Kept city","latitude":1.0,"longitude":2.0}}"#).unwrap();
    fixture.user(
        wallpaper::NAMESPACE,
        "all",
        &ron::to_string(&BgEntry::new(
            "all",
            BgSource::Color(wallpaper::BgColor::Single([0.15, 0.4, 0.7])),
        ))
        .unwrap(),
    );
    let store = fixture.store().with_app_config(config);
    let mut editor = ProfilesEditor::with_store(store.clone());
    editor.open();
    editor.enter(previews::PreviewKind::Theme);
    settle(&mut editor);
    (fixture, store, editor)
}

fn settle(editor: &mut ProfilesEditor) {
    for _ in 0..16 {
        let jobs = editor.take_theme_jobs();
        if jobs.is_empty() {
            return;
        }
        for job in jobs {
            editor.update(job.run());
        }
    }
    panic!("color analysis did not settle");
}

#[test]
fn colors_without_a_theme_stage_mode_scope_and_accent_until_one_apply_and_undo() {
    let (_fixture, store, mut editor) = fixture("colors-current-transaction");
    let config_before = fs::read(store.app_config().unwrap()).unwrap();
    let native_before = store.load(&AppearanceDraft::keys()).unwrap();
    editor.update(Message::Color(Edit::Mode(SystemMode::Light)));
    editor.update(Message::ThemeScope(true));
    editor.update(Message::ThemeAccent([94, 129, 172]));
    settle(&mut editor);
    assert_eq!(editor.color_mode(), Some(SystemMode::Light));
    assert!(editor.profile().unwrap().appearance_only);
    assert!(profiles::themes::family(editor.profile().unwrap()).is_none());
    assert_eq!(
        fs::read(store.app_config().unwrap()).unwrap(),
        config_before
    );
    assert_eq!(store.load(&AppearanceDraft::keys()).unwrap(), native_before);
    assert!(store.latest_change().unwrap().is_none());
    assert_eq!(
        editor.update(Message::Apply),
        Outcome::Changed,
        "{:?}",
        editor.notice
    );
    assert_eq!(
        AppearanceDraft::load(&store).unwrap().mode().known(),
        Some(&SystemMode::Light)
    );
    let (raw, config) = wallpaper::automatic::read_config(&store).unwrap();
    assert_eq!(config.appearance.accent, Some([94, 129, 172]));
    assert_eq!(config.appearance.opacity, 92);
    assert_eq!(raw["future_field"], 73);
    assert_eq!(config.location.unwrap().name, "Kept city");
    let receipt = editor.latest().unwrap();
    assert!(receipt.widget.is_some());
    assert!(
        receipt
            .change
            .entries()
            .iter()
            .any(|entry| entry.key.namespace() == "com.system76.CosmicTheme.Mode")
    );
    assert_eq!(editor.update(Message::Undo), Outcome::Changed);
    assert_eq!(
        fs::read(store.app_config().unwrap()).unwrap(),
        config_before
    );
    assert_eq!(store.load(&AppearanceDraft::keys()).unwrap(), native_before);
    assert!(
        store.latest_change().unwrap().is_none(),
        "the combined preview made one history entry"
    );
}

#[test]
fn colors_first_theme_preserves_staged_mode_scope_and_accent() {
    let (_fixture, store, mut editor) = fixture("colors-first-theme");
    let before = fs::read(store.app_config().unwrap()).unwrap();
    editor.update(Message::Color(Edit::Mode(SystemMode::Automatic)));
    editor.update(Message::ThemeScope(false));
    editor.update(Message::ThemeAccent([171, 90, 122]));
    editor.update(Message::Select(Selection::Preset("theme-soft")));
    settle(&mut editor);
    let profile = editor.profile().unwrap();
    assert_eq!(
        profiles::themes::family(profile),
        Some(crate::desktop_theme::Family::Soft)
    );
    assert_eq!(
        profile
            .widget_style
            .as_ref()
            .unwrap()
            .pointer("/appearance/accent"),
        Some(&json!([171, 90, 122]))
    );
    assert_eq!(
        profile
            .widget_style
            .as_ref()
            .unwrap()
            .pointer("/appearance/sync_colors"),
        Some(&json!(false))
    );
    assert_eq!(editor.color_mode(), Some(SystemMode::Automatic));
    assert_eq!(fs::read(store.app_config().unwrap()).unwrap(), before);
    assert!(store.latest_change().unwrap().is_none());
}

#[test]
fn colors_theme_mode_and_accent_apply_every_planned_theme_field_in_one_receipt() {
    let (_fixture, store, mut editor) = fixture("colors-whole-theme");
    let before = fs::read(store.app_config().unwrap()).unwrap();
    let mut keys = AppearanceDraft::keys();
    for kind in PanelKind::ALL {
        keys.extend(PanelDraft::keys(kind));
    }
    let native_before = store.load(&keys).unwrap();
    editor.update(Message::Select(Selection::Preset("theme-soft")));
    settle(&mut editor);
    editor.update(Message::Color(Edit::Mode(SystemMode::Light)));
    editor.update(Message::ThemeAccent([94, 129, 172]));
    let plan = editor.plan().unwrap().clone();
    assert!(
        plan.domains
            .iter()
            .any(|p| p.domain == Domain::Dock && !p.changes.is_empty())
    );
    assert!(
        plan.domains
            .iter()
            .any(|p| p.domain == Domain::Panel && !p.changes.is_empty())
    );
    assert_eq!(fs::read(store.app_config().unwrap()).unwrap(), before);
    assert_eq!(store.load(&keys).unwrap(), native_before);
    assert_eq!(
        editor.update(Message::Apply),
        Outcome::Changed,
        "{:?}",
        editor.notice
    );
    for (key, entry) in plan.patch().iter() {
        assert_eq!(store.read_user(key).unwrap(), entry.desired, "{key:?}");
    }
    let (raw, config) = wallpaper::automatic::read_config(&store).unwrap();
    for (pointer, entry) in plan.widget_patch().iter() {
        assert_eq!(widget::get(&raw, pointer), entry.desired, "{pointer}");
    }
    assert_eq!(
        config.appearance.family,
        Some(crate::desktop_theme::Family::Soft)
    );
    assert_eq!(raw["future_field"], 73);
    assert_eq!(editor.update(Message::Undo), Outcome::Changed);
    assert_eq!(fs::read(store.app_config().unwrap()).unwrap(), before);
    assert_eq!(store.load(&keys).unwrap(), native_before);
    assert!(store.latest_change().unwrap().is_none());
}

#[test]
fn colors_mode_conflict_blocks_accent_and_discard_keeps_the_newer_desktop() {
    let (fixture, store, mut editor) = fixture("colors-mode-conflict");
    let before = fs::read(store.app_config().unwrap()).unwrap();
    editor.update(Message::Color(Edit::Mode(SystemMode::Light)));
    editor.update(Message::ThemeAccent([94, 129, 172]));
    fixture.user("com.system76.CosmicTheme.Mode", "auto_switch", "true");
    let external = store.load(&AppearanceDraft::keys()).unwrap();
    editor.refresh();
    assert!(!editor.conflicts().is_empty());
    assert!(!editor.can_apply());
    assert_eq!(editor.update(Message::Apply), Outcome::None);
    assert_eq!(fs::read(store.app_config().unwrap()).unwrap(), before);
    assert_eq!(store.load(&AppearanceDraft::keys()).unwrap(), external);
    editor.update(Message::Discard);
    assert_eq!(fs::read(store.app_config().unwrap()).unwrap(), before);
    assert_eq!(store.load(&AppearanceDraft::keys()).unwrap(), external);
}

#[test]
fn colors_preview_options_survive_parking_restart_and_local_undo() {
    let (_fixture, store, mut editor) = fixture("colors-options-restart");
    let before = fs::read(store.app_config().unwrap()).unwrap();
    editor.update(Message::Color(Edit::Mode(SystemMode::Light)));
    editor.update(Message::Color(Edit::StyleBars(false)));
    assert!(editor.undo_step());
    assert_eq!(editor.color_options.style_bars, None);
    assert_eq!(editor.color_options.mode, Some(SystemMode::Light));
    editor.update(Message::ThemeAccent([94, 129, 172]));
    editor.park(previews::PreviewKind::Theme);
    // Session drafts use RON: native baseline keys are structured ConfigKeys.
    let encoded = ron::to_string(&editor.saved_previews()).unwrap();
    drop(editor);
    let mut reopened = ProfilesEditor::with_store(store.clone());
    reopened
        .restore_previews(ron::from_str(&encoded).unwrap())
        .unwrap();
    assert!(reopened.enter(previews::PreviewKind::Theme));
    settle(&mut reopened);
    assert_eq!(reopened.color_mode(), Some(SystemMode::Light));
    assert_eq!(
        reopened
            .profile()
            .unwrap()
            .widget_style
            .as_ref()
            .unwrap()
            .pointer("/appearance/accent"),
        Some(&json!([94, 129, 172]))
    );
    assert_eq!(fs::read(store.app_config().unwrap()).unwrap(), before);
    assert!(store.latest_change().unwrap().is_none());
}

#[test]
fn colors_excluded_parts_remove_mode_and_style_bars_writes() {
    let (_fixture, _store, mut editor) = fixture("colors-excluded-options");
    editor.update(Message::Color(Edit::Mode(SystemMode::Light)));
    editor.update(Message::Color(Edit::StyleBars(false)));
    editor.update(Message::Include(Domain::Appearance, false));
    editor.update(Message::Include(Domain::WidgetStyle, false));
    let plan = editor.plan().unwrap();
    assert!(!plan.patch().iter().any(|(key, entry)| key.namespace()
        == "com.system76.CosmicTheme.Mode"
        && entry.original != entry.desired));
    assert!(
        !plan
            .widget_patch()
            .iter()
            .any(|(pointer, entry)| pointer == "/appearance/style_bars"
                && entry.original != entry.desired)
    );
    assert_eq!(editor.color_mode(), Some(SystemMode::Dark));
    assert!(editor.color_style_bars(&Config::default()));
}

#[test]
fn colors_keep_current_projects_the_kept_accent_without_replacing_custom_text() {
    let (_fixture, store, mut editor) = fixture("colors-kept-display");
    editor.update(Message::ThemeCustom(Some("#5E81AC".into())));
    let (mut raw, config) = wallpaper::automatic::read_config(&store).unwrap();
    widget::put(&mut raw, "/appearance/accent", Some(json!([180, 80, 110]))).unwrap();
    fs::write(
        store.app_config().unwrap(),
        serde_json::to_vec(&raw).unwrap(),
    )
    .unwrap();
    editor.refresh();
    let conflict = editor
        .conflict_values()
        .iter()
        .find(|value| value.id == "widget:/appearance/accent")
        .expect("the newer accent conflicts with this preview")
        .id
        .clone();
    editor.update(Message::ResolveValueConflict(conflict, false));
    assert_eq!(
        editor.color_preferences(&config).accent,
        Some([180, 80, 110])
    );
    assert_eq!(
        editor.custom.as_deref(),
        Some("#5E81AC"),
        "the active text field keeps its input"
    );
    assert!(store.latest_change().unwrap().is_none());
}
