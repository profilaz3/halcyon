//! Window tone through the Appearance editor: one applied change per
//! gesture, saved per style, undone from the footer, and never over a
//! Background chosen in Advanced theme colors. Isolated fixtures only.
use super::*;
use crate::desktop::store::tests::Fixture;
use crate::desktop_theme::{Family, WindowTone};
use profiles::themes::{self, ToneState};
use serde_json::json;

fn fixture(name: &str) -> (Fixture, DesktopStore, ProfilesEditor) {
    let fixture = Fixture::new(name);
    fixture.system(
        "com.system76.CosmicPanel",
        "entries",
        "[\"Panel\",\"Dock\"]",
    );
    fixture.system(&PanelKind::Dock.namespace(), "anchor", "Bottom");
    fixture.system(&PanelKind::Panel.namespace(), "anchor", "Top");
    fixture.user("com.system76.CosmicTheme.Mode", "is_dark", "true");
    fixture.user("com.system76.CosmicTheme.Mode", "auto_switch", "false");
    let config = fixture.0.join("config.json");
    fs::write(&config, r#"{"appearance":{"opacity":92}}"#).unwrap();
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

fn apply(editor: &mut ProfilesEditor, message: Message) {
    editor.update(message);
    settle(editor);
    assert!(editor.can_apply(), "{:?}", editor.notice);
    assert_eq!(
        editor.update(Message::Apply),
        Outcome::Changed,
        "{:?}",
        editor.notice
    );
}

fn style(editor: &mut ProfilesEditor, family: Family) {
    apply(editor, Message::Select(Selection::Preset(family.preset())));
}

/// A Dark builder value, as COSMIC stores it (v2).
fn builder(fixture: &Fixture, key: &str, value: &str) {
    let path = fixture
        .0
        .join("config/cosmic/com.system76.CosmicTheme.Dark.Builder/v2")
        .join(key);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, value).unwrap();
}

fn rgb(color: Option<cosmic::cosmic_theme::palette::Srgba>) -> [u8; 3] {
    let c = color.expect("a written color");
    [c.red, c.green, c.blue].map(|v| (v * 255.).round() as u8)
}

/// The dark window background and primary container Halcyon wrote.
fn window(store: &DesktopStore) -> ([u8; 3], [u8; 3]) {
    let draft = AppearanceDraft::load(store).unwrap();
    let get = |role| rgb(*draft.color(ThemeVariant::Dark, role).known().unwrap());
    (get(ColorRole::Background), get(ColorRole::Primary))
}

fn applied(store: &DesktopStore) -> crate::config::Appearance {
    wallpaper::automatic::read_config(store)
        .unwrap()
        .1
        .appearance
}

/// The app config, without the wallpaper color stamp that undo's
/// reconcile records.
fn config(store: &DesktopStore) -> serde_json::Value {
    let mut raw: serde_json::Value =
        serde_json::from_slice(&fs::read(store.app_config().unwrap()).unwrap()).unwrap();
    raw["appearance"]
        .as_object_mut()
        .unwrap()
        .remove("wallpaper_color_stamp");
    raw
}

/// Every native theme value COSMIC would read; a stored `None` reads the
/// same as no file.
fn native(store: &DesktopStore) -> Vec<Option<crate::desktop::store::RawValue>> {
    let keys = AppearanceDraft::keys();
    let snapshot = store.load(&keys).unwrap();
    let none = crate::desktop::store::RawValue::parse("None").unwrap();
    keys.iter()
        .map(|key| snapshot.user(key).filter(|v| **v != none).cloned())
        .collect()
}

/// The color the applied style derives its surfaces from.
fn seed(store: &DesktopStore, family: Family) -> [u8; 3] {
    applied(store).accent.unwrap_or_else(|| family.accent())
}

fn expected(store: &DesktopStore, family: Family, tone: i8) -> ([u8; 3], [u8; 3]) {
    let seed = seed(store, family);
    (
        family.window_for(seed, true, tone),
        family.window_container_for(seed, true, tone),
    )
}

#[test]
fn window_tone_applies_per_style_and_undoes_as_one_change() {
    let (_fixture, store, mut editor) = fixture("window-tone-per-style");
    style(&mut editor, Family::Bare);
    assert_eq!(window(&store), expected(&store, Family::Bare, 0));
    let config_before = config(&store);
    let native_before = native(&store);

    apply(&mut editor, Message::WindowTone(-6));
    assert_eq!(window(&store), expected(&store, Family::Bare, -6));
    let a = applied(&store);
    assert_eq!(a.window_tone.get(Family::Bare), -6);
    // The tone is the style's own, not an Advanced override.
    assert_eq!(a.overrides, 0);
    let draft = AppearanceDraft::load(&store).unwrap();
    assert_eq!(
        themes::tone_state(&draft, a, ThemeVariant::Dark),
        ToneState::Follows
    );
    for role in [ColorRole::Background, ColorRole::Primary] {
        assert!(themes::linked_surface(&draft, a, ThemeVariant::Dark, role));
    }
    // One history entry holds both the preference and the native colors.
    let receipt = editor.latest().unwrap();
    assert!(receipt.widget.is_some());
    assert!(
        receipt
            .change
            .entries()
            .iter()
            .any(|e| e.key.key() == "bg_color")
    );

    assert_eq!(editor.update(Message::Undo), Outcome::Changed);
    assert_eq!(config(&store), config_before);
    assert_eq!(native(&store), native_before);
    assert_eq!(window(&store), expected(&store, Family::Bare, 0));

    // Each style keeps its own tone across style switches.
    apply(&mut editor, Message::WindowTone(-6));
    style(&mut editor, Family::Terminal);
    assert_eq!(window(&store), expected(&store, Family::Terminal, 0));
    apply(&mut editor, Message::WindowTone(4));
    assert_eq!(window(&store), expected(&store, Family::Terminal, 4));
    style(&mut editor, Family::Bare);
    assert_eq!(window(&store), expected(&store, Family::Bare, -6));
    let a = applied(&store);
    assert_eq!(a.window_tone.get(Family::Bare), -6);
    assert_eq!(a.window_tone.get(Family::Terminal), 4);
    // Default returns the style to its own background.
    apply(&mut editor, Message::WindowTone(0));
    assert_eq!(window(&store), expected(&store, Family::Bare, 0));
    assert_eq!(applied(&store).window_tone.get(Family::Terminal), 4);
}

#[test]
fn advanced_background_wins_over_window_tone() {
    let (fixture, store, mut editor) = fixture("window-tone-override");
    style(&mut editor, Family::Bare);
    // A Background chosen in Advanced theme colors: the color and its
    // override bit, as that control commits them.
    builder(&fixture, "bg_color", "Some(\"#123456FF\")");
    let path = store.app_config().unwrap();
    let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let bit = crate::desktop::actions::override_for_native(
        &crate::desktop::appearance::theme::builder_key(ThemeVariant::Dark, "bg_color"),
    );
    raw["appearance"]["overrides"] = json!(bit);
    fs::write(&path, raw.to_string()).unwrap();
    let draft = AppearanceDraft::load(&store).unwrap();
    assert_eq!(
        themes::tone_state(&draft, applied(&store), ThemeVariant::Dark),
        ToneState::Explicit
    );
    editor.update(Message::Select(Selection::Current));
    apply(&mut editor, Message::WindowTone(-8));
    let (background, primary) = window(&store);
    assert_eq!(background, [0x12, 0x34, 0x56], "the explicit color wins");
    // The container has no override, so it still follows the tone.
    assert_eq!(primary, expected(&store, Family::Bare, -8).1);
    assert_eq!(applied(&store).window_tone.get(Family::Bare), -8);
}

#[test]
fn earlier_bare_background_still_follows_and_updates() {
    let (fixture, store, mut editor) = fixture("window-tone-earlier");
    style(&mut editor, Family::Bare);
    // What the previous release wrote for Bare's windows.
    let (background, primary) = Family::Bare.earlier_window(seed(&store, Family::Bare), true);
    for (key, [r, g, b]) in [("bg_color", background), ("primary_container_bg", primary)] {
        builder(
            &fixture,
            key,
            &format!("Some(\"#{r:02X}{g:02X}{b:02X}FF\")"),
        );
    }
    let a = applied(&store);
    let draft = AppearanceDraft::load(&store).unwrap();
    assert_eq!(
        themes::tone_state(&draft, a, ThemeVariant::Dark),
        ToneState::Earlier
    );
    assert!(
        !themes::native_custom(&draft, a).surfaces,
        "still the style's"
    );
    editor.update(Message::Select(Selection::Current));
    apply(&mut editor, Message::WindowTone(0));
    assert_eq!(window(&store), expected(&store, Family::Bare, 0));
}

#[test]
fn window_tone_travels_with_looks() {
    let (_fixture, store, mut editor) = fixture("window-tone-looks");
    style(&mut editor, Family::Bare);
    apply(&mut editor, Message::WindowTone(-6));
    let look = crate::desktop::profiles::capture(&store, "Saved")
        .unwrap()
        .profile;
    let tones = look
        .widget_style
        .as_ref()
        .and_then(|s| s.pointer("/appearance/window_tone"))
        .cloned();
    assert_eq!(tones, Some(json!({"bare": -6})));
    let theme = themes::capture(&store, "Saved theme").unwrap().profile;
    assert_eq!(themes::tones(&theme).map(|t| t.get(Family::Bare)), Some(-6));
    // An imported look carrying a tone is accepted; a damaged one is not.
    let mut imported = look.clone();
    imported.validate().unwrap();
    imported
        .widget_style
        .as_mut()
        .unwrap()
        .pointer_mut("/appearance/window_tone")
        .map(|v| *v = json!({"bare": 40}));
    assert!(imported.validate().is_err());
    assert_eq!(
        WindowTone::default().with(Family::Bare, -6).to_value(),
        json!({"bare": -6})
    );
}
