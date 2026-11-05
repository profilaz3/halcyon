use super::*;
use crate::desktop::store::{ConfigKey, tests::Fixture};

const DOCK: &str = "com.system76.CosmicPanel.Dock";
const PANEL: &str = "com.system76.CosmicPanel.Panel";

fn fixture(name: &str) -> (Fixture, DesktopStore) {
    let fixture = Fixture::new(name);
    fixture.system(
        "com.system76.CosmicPanel",
        "entries",
        "[\"Panel\",\"Dock\"]",
    );
    fixture.system(DOCK, "anchor", "Bottom");
    fixture.system(PANEL, "anchor", "Top");
    fixture.user(DOCK, "padding", "4");
    fixture.user(PANEL, "padding", "4");
    let path = fixture.0.join("config.json");
    fs::write(&path, r#"{"appearance":{"opacity":92,"corner_radius":12}}"#).unwrap();
    let store = fixture.store().with_app_config(path);
    (fixture, store)
}

fn review(store: &DesktopStore) -> ProfilesEditor {
    let profile = profiles::import_local(
        serde_json::json!({
            "format": "desktop-cards-profile", "version": profiles::VERSION,
            "name": "Retained setup", "dock": {"padding": 8}, "panel": {"padding": 4},
            "widget_style": {"appearance": {"opacity": 70, "corner_radius": 12}}
        })
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    let id = ProfileLibrary::new(store).create(&profile).unwrap();
    let mut editor = ProfilesEditor::with_store(store.clone());
    editor.enter(PreviewKind::Setup);
    editor.update(Message::Select(Selection::Saved(id)));
    editor.update(Message::Include(Domain::Panel, true));
    editor
}

#[test]
fn preview_conflicts_are_per_value_and_survive_navigation_and_restart() {
    let (fixture, store) = fixture("ux-preview-three-way");
    let mut editor = review(&store);
    let selection = editor.selection().clone();
    editor.park(PreviewKind::Setup);
    fixture.user(DOCK, "padding", "6");
    fixture.user(PANEL, "padding", "9");
    fs::write(
        fixture.0.join("config.json"),
        r#"{"appearance":{"opacity":90,"corner_radius":18}}"#,
    )
    .unwrap();
    editor.enter(PreviewKind::Setup);
    assert_eq!(
        editor.conflict_values().len(),
        2,
        "untouched keys follow current"
    );
    assert!(!editor.can_apply());
    let panel = ConfigKey::new(PANEL, 1, "padding").unwrap();
    assert!(editor.plan().unwrap().patch().get(&panel).is_none());
    assert!(
        !editor
            .plan()
            .unwrap()
            .widget_patch()
            .changes()
            .any(|(p, _)| p == "/appearance/corner_radius")
    );
    editor.park(PreviewKind::Setup);
    editor.enter(PreviewKind::Setup);
    assert_eq!(editor.conflict_values().len(), 2);
    let saved = ron::to_string(&editor.saved_previews()).unwrap();
    let mut reopened = ProfilesEditor::with_store(store.clone());
    reopened
        .restore_previews(ron::from_str(&saved).unwrap())
        .unwrap();
    reopened.resume_kept_selection(&selection);
    assert_eq!(reopened.conflict_values().len(), 2);
    assert!(!reopened.can_apply());
    assert!(
        reopened.stale(),
        "restart still requires an explicit review"
    );
    reopened.update(Message::Replan);
    assert_eq!(
        reopened.conflict_values().len(),
        2,
        "review cannot accept conflicts"
    );
    let dock = reopened
        .conflict_values()
        .iter()
        .find(|v| v.domain == Domain::Dock)
        .unwrap()
        .id
        .clone();
    let opacity = reopened
        .conflict_values()
        .iter()
        .find(|v| v.domain == Domain::WidgetStyle)
        .unwrap()
        .id
        .clone();
    reopened.update(Message::ResolveValueConflict(dock, true));
    reopened.update(Message::ResolveValueConflict(opacity, false));
    assert!(reopened.can_apply());
    reopened.refresh();
    assert!(reopened.conflict_values().is_empty());
    assert_eq!(reopened.update(Message::Apply), Outcome::Changed);
    assert_eq!(fixture.read(DOCK, "padding").as_deref(), Some("8"));
    assert_eq!(fixture.read(PANEL, "padding").as_deref(), Some("9"));
    let config: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.0.join("config.json")).unwrap()).unwrap();
    assert_eq!(config["appearance"]["opacity"], 90);
    assert_eq!(config["appearance"]["corner_radius"], 18);
}

#[test]
fn preview_equal_current_settles_and_excluded_part_clears_conflicts() {
    let (fixture, store) = fixture("ux-preview-settled");
    let mut editor = review(&store);
    editor.park(PreviewKind::Setup);
    fixture.user(DOCK, "padding", "8");
    fixture.user(PANEL, "padding", "7");
    editor.enter(PreviewKind::Setup);
    assert!(editor.conflict_values().is_empty());
    fixture.user(DOCK, "padding", "6");
    editor.refresh();
    assert!(editor.conflicts().contains(&Domain::Dock));
    editor.update(Message::Include(Domain::Dock, false));
    assert!(!editor.conflicts().contains(&Domain::Dock));
}

#[test]
fn partial_mix_and_its_undo_survive_leaving_and_restarting() {
    let (_fixture, store) = fixture("ux-preview-partial-mix");
    let mut editor = ProfilesEditor::with_store(store.clone());
    editor.enter(PreviewKind::Setup);
    editor.update(Message::MixStart);
    editor.update(Message::MixPick(
        profiles::mix::Component::Style,
        Selection::Preset("calm"),
    ));
    assert!(editor.can_undo_step());
    assert!(
        editor
            .mix
            .as_ref()
            .unwrap()
            .source_name(profiles::mix::Component::Style)
            .is_some()
    );
    editor.park(PreviewKind::Setup);
    let saved = ron::to_string(&editor.saved_previews()).unwrap();
    let mut reopened = ProfilesEditor::with_store(store);
    reopened
        .restore_previews(ron::from_str(&saved).unwrap())
        .unwrap();
    reopened.enter(PreviewKind::Setup);
    assert!(
        reopened
            .mix
            .as_ref()
            .unwrap()
            .source_name(profiles::mix::Component::Style)
            .is_some()
    );
    reopened.update(Message::MixPick(
        profiles::mix::Component::Layout,
        Selection::Preset("calm"),
    ));
    assert!(reopened.undo_step());
    assert!(
        reopened
            .mix
            .as_ref()
            .unwrap()
            .source_name(profiles::mix::Component::Layout)
            .is_none()
    );
    assert!(reopened.keeps_intent(PreviewKind::Setup));
    assert!(reopened.park(PreviewKind::Setup).is_some());
}

#[test]
fn legacy_selection_migrates_into_retained_preview() {
    let (_fixture, store) = fixture("ux-preview-legacy");
    let editor = review(&store);
    let legacy = editor.session_draft().unwrap();
    let mut reopened = ProfilesEditor::with_store(store);
    reopened.restore_session(legacy).unwrap();
    assert!(reopened.keeps_intent(PreviewKind::Setup));
    assert_eq!(reopened.saved_previews().len(), 1);
    assert!(reopened.park(PreviewKind::Setup).is_some());
}

#[test]
fn another_kept_selection_does_not_overwrite_the_live_review() {
    let (_fixture, store) = fixture("ux-preview-resume-other");
    let mut editor = review(&store);
    let first = editor.selection().clone();
    editor.park(PreviewKind::Setup);
    editor.update(Message::Select(Selection::Preset("calm")));
    editor.update(Message::Include(Domain::Wallpaper, false));
    editor.resume_kept_selection(&first);
    assert_eq!(editor.selection(), &first);
    assert!(
        editor
            .parked()
            .iter()
            .any(|p| p.selection() == &Selection::Preset("calm"))
    );
}

#[test]
fn selecting_the_same_theme_does_not_accept_conflicts_or_reset_choices() {
    let (fixture, store) = fixture("ux-preview-same-theme");
    let mut profile = Profile::new("Theme test").unwrap();
    profile.appearance_only = true;
    profile.widget_style = Some(serde_json::json!({"appearance": {"opacity": 70}}));
    let id = ProfileLibrary::new(&store).create(&profile).unwrap();
    let selection = Selection::Saved(id);
    let mut editor = ProfilesEditor::with_store(store);
    editor.enter(PreviewKind::Theme);
    editor.update(Message::Select(selection.clone()));
    editor.custom = Some("#123".into());
    fs::write(
        fixture.0.join("config.json"),
        r#"{"appearance":{"opacity":90}}"#,
    )
    .unwrap();
    editor.refresh();
    assert_eq!(editor.conflict_values().len(), 1);
    let baseline = editor.preview_baseline.clone();
    editor.update(Message::Select(selection.clone()));
    assert_eq!(editor.preview_baseline, baseline);
    assert_eq!(editor.custom.as_deref(), Some("#123"));
    assert_eq!(editor.conflict_values().len(), 1);
    editor.update(Message::Discard);
    editor.update(Message::Select(selection));
    assert!(
        editor.conflict_values().is_empty(),
        "discard starts a new review"
    );
}

#[test]
fn incomplete_custom_color_survives_navigation_and_restart_without_a_profile() {
    let (fixture, store) = fixture("ux-preview-partial-custom");
    let before = fs::read(fixture.0.join("config.json")).unwrap();
    let mut editor = ProfilesEditor::with_store(store.clone());
    editor.enter(PreviewKind::Theme);
    editor.update(Message::ThemeCustom(Some("#12".into())));
    assert!(editor.profile().is_none());
    assert!(editor.review_open());
    assert!(editor.keeps_intent(PreviewKind::Theme));
    assert!(!editor.can_apply());
    assert_eq!(editor.update(Message::Apply), Outcome::None);
    editor.park(PreviewKind::Theme);
    assert!(editor.enter(PreviewKind::Theme));
    assert_eq!(editor.custom.as_deref(), Some("#12"));
    editor.refresh();
    assert_eq!(editor.custom.as_deref(), Some("#12"));
    let saved = ron::to_string(&editor.saved_previews()).unwrap();
    let mut reopened = ProfilesEditor::with_store(store.clone());
    reopened
        .restore_previews(ron::from_str(&saved).unwrap())
        .unwrap();
    assert!(reopened.enter(PreviewKind::Theme));
    assert_eq!(reopened.custom.as_deref(), Some("#12"));
    reopened.update(Message::Replan);
    assert!(!reopened.can_apply());
    assert_eq!(reopened.update(Message::Apply), Outcome::None);
    assert_eq!(fs::read(fixture.0.join("config.json")).unwrap(), before);
    assert!(store.latest_change().unwrap().is_none());
    reopened.update(Message::Discard);
    assert!(!reopened.review_open());
    assert!(reopened.saved_previews().is_empty());
}

#[test]
fn incomplete_custom_color_blocks_an_existing_applicable_preview() {
    let (fixture, store) = fixture("ux-preview-partial-custom-existing");
    let before = fs::read(fixture.0.join("config.json")).unwrap();
    let mut editor = review(&store);
    assert!(editor.can_apply());
    editor.update(Message::ThemeCustom(Some("#12".into())));
    assert!(editor.plan().unwrap().can_apply());
    assert!(!editor.can_apply());
    assert_eq!(editor.update(Message::Apply), Outcome::None);
    assert_eq!(fs::read(fixture.0.join("config.json")).unwrap(), before);
    assert!(store.latest_change().unwrap().is_none());
    editor.update(Message::ThemeCustom(None));
    assert!(editor.can_apply());
}
