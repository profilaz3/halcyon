//! Undo for immediate widget preferences. Each action declares the exact
//! configuration members it owns before it runs. A concurrent change to any
//! other member is never attributed to that action or reverted by its Undo.
use super::{AppearanceControl, Config, Message, Settings, tools};
use crate::drawer::Widget;
#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
use std::collections::BTreeSet;

#[cfg(test)]
const CONFLICT: &str =
    "This preference changed elsewhere. It was kept; Undo is no longer available for that edit.";

fn pointer(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| format!("/{}", part.replace('~', "~0").replace('/', "~1")))
        .collect()
}

fn glance_section(widget: Widget) -> Option<&'static str> {
    match widget {
        Widget::Clock => Some("show_clock"),
        Widget::Weather => Some("show_weather"),
        Widget::Calendar => Some("show_calendar"),
        _ => None,
    }
}

/// Fields directly written by one immediate preference action. Arrays and
/// records replaced by the writer (enabled widgets, order, location) remain
/// logical fields; nested options never expand to their whole parent object.
fn message_scope(config: &Config, message: &Message) -> Vec<String> {
    let mut fields = Vec::new();
    let mut add = |parts: &[&str]| fields.push(pointer(parts));
    match message {
        Message::Rain(super::rain::Edit::Commit | super::rain::Edit::Set(_)) => add(&["rain"]),
        Message::Moment(edit) => {
            if let Some(path) = super::moments::scope(edit) {
                fields.push(path);
            }
        }
        Message::WidgetDisplay(_) => add(&["widget_display"]),
        Message::WidgetMonitor(w, _) => add(&["widget_monitors", w.key(), "display"]),
        Message::WidgetMonitorHide(w, _) => add(&["widget_monitors", w.key(), "hide_disconnected"]),
        Message::Widget(widget, _) if widget.is_tool() => add(&[widget.key(), "enabled"]),
        Message::Widget(Widget::Glance, visible) => {
            add(&["glance", "enabled"]);
            if *visible && !config.glance.initialized {
                for key in ["initialized", "show_clock", "show_weather", "show_calendar"] {
                    add(&["glance", key]);
                }
            }
        }
        Message::Widget(widget, _) => {
            if let Some(key) = glance_section(*widget).filter(|_| config.glance.enabled) {
                add(&["glance", key]);
            } else {
                add(&["enabled_widgets"]);
            }
        }
        Message::RefreshInterval(widget, _) => {
            let key = if *widget == Widget::Glance {
                "clock"
            } else {
                widget.key()
            };
            add(&["refresh_intervals", key]);
        }
        Message::Move(..) => add(&["widget_order"]),
        Message::Devbox(key, _) => add(&["devbox", key]),
        Message::DevboxHistory(key, _) => add(&["devbox", "history", key]),
        Message::DevboxMove(..) => add(&["devbox", "order"]),
        Message::GlanceSection(key, _) => {
            if !config.glance.initialized
                && matches!(*key, "show_clock" | "show_weather" | "show_calendar")
            {
                for key in ["initialized", "show_clock", "show_weather", "show_calendar"] {
                    add(&["glance", key]);
                }
            }
            add(&["glance", key]);
        }
        Message::GlanceStyle(_) => add(&["glance", "style"]),
        Message::GlanceDensity(_) => add(&["glance", "density"]),
        Message::GlanceTomorrow(_) => add(&["glance", "show_tomorrow"]),
        Message::GlanceOpacity(_) => add(&["glance", "opacity"]),
        Message::CalendarPrivacy(_) => add(&["calendar_private"]),
        Message::Today(_) => add(&["calendar_show_today"]),
        Message::Tomorrow(_) => add(&["calendar_show_tomorrow"]),
        Message::ClockFormat(_) => add(&["clock", "format_24h"]),
        Message::ClockDate(_) => add(&["clock", "show_date"]),
        Message::WeatherDewPoint(_) => add(&["weather", "show_dew_point"]),
        Message::ClockAlignment(_) => add(&["clock", "alignment"]),
        Message::ClockScale(_) => add(&["clock", "scale"]),
        Message::ClockVertical(_) => add(&["clock", "center_vertically"]),
        Message::ClockDepth(_) => add(&["clock", "depth"]),
        edit if super::moments::clock_style_edit(edit).is_some() => {
            let (key, _) = super::moments::clock_style_edit(edit).unwrap();
            add(&["clock", key]);
        }
        Message::WidgetColorSource(_) => add(&["appearance", "color_source"]),
        Message::ContrastAssist(_) => add(&["appearance", "contrast_assist"]),
        Message::WidgetSurface(_) => add(&["appearance", "surface"]),
        Message::AppearancePreset(..) => {
            for key in ["opacity", "corner_radius", "border_opacity"] {
                add(&["appearance", key]);
            }
        }
        Message::ChooseCity(_) => add(&["location"]),
        Message::AddFolder | Message::FolderChosen(Ok(Some(_))) | Message::RemoveFolder(_) => {
            add(&["project_roots"]);
        }
        Message::SidebarSlide(_) => add(&["sidebar_slide"]),
        Message::SettingsAnimations(_) => add(&["settings_animations"]),
        Message::MaterialMotion(_) => add(&["material_motion"]),
        Message::Tools(tools::Message::Player(_))
        | Message::Tools(tools::Message::Card(crate::tools::cards::Action::MusicAuto)) => {
            add(&["music", "player"])
        }
        Message::Tools(tools::Message::Option(widget, key, _)) if widget.is_tool() => {
            add(&[widget.key(), key]);
        }
        _ => {}
    }
    fields
}

impl Settings {
    /// Capture before dispatch: debounce handlers consume their pending list,
    /// and drag handlers clear their source when finishing.
    pub(super) fn preference_scope(&self, message: &Message) -> Vec<String> {
        let mut fields = message_scope(&self.config, message);
        if matches!(message, Message::Placement(super::placement::Message::Save)) {
            fields.extend(self.placement.changed_fields());
        }
        if matches!(message, Message::DragDrop) {
            if self.devbox_reorder.source.is_some() {
                fields.push(pointer(&["devbox", "order"]));
            } else if self.reorder.source.is_some() {
                fields.push(pointer(&["widget_order"]));
            }
        }
        let flushes = match message {
            Message::SaveNotice
            | Message::ContrastAssist(_)
            | Message::WidgetSurface(_)
            | Message::SaveAppearance
            | Message::FlushEdits
            | Message::Page(_)
            | Message::EditWidgetLayout(_)
            | Message::Back
            | Message::TryPreset(_)
            | Message::OpenBar(_)
            | Message::SearchResult(_)
            | Message::SearchFirst
            | Message::WidgetColorSource(_)
            | Message::AppearancePreset(..) => true,
            Message::CommitEdits(generation) => *generation == self.edit_generation,
            _ => false,
        };
        if flushes {
            if self.rain.draft.is_some() {
                fields.push("/rain".into());
            }
            fields.extend(self.appearance_pending.iter().map(|control| {
                pointer(&[
                    "appearance",
                    match control {
                        AppearanceControl::Opacity => "opacity",
                        AppearanceControl::Corners => "corner_radius",
                        AppearanceControl::Border => "border_opacity",
                    },
                ])
            }));
            if self.notice_editing {
                fields.push(pointer(&["meeting_notice_minutes"]));
            }
        }
        fields.sort();
        fields.dedup();
        fields
    }

    pub(super) fn undo_preferences(&mut self) {
        let task = self.undo_desktop_change();
        self.native_tasks.push(task);
    }
}

#[cfg(test)]
#[derive(Debug, Clone)]
struct Change {
    pointer: String,
    previous: Option<Value>,
    expected: Option<Value>,
}

/// Session-local, compare-and-swap Undo. Field-level scope is fixed when the
/// action starts; the conflict check and all writes share the config lock.
#[cfg(test)]
#[derive(Debug, Clone)]
pub(super) struct PreferenceUndo {
    changes: Vec<Change>,
}

#[cfg(test)]
impl PreferenceUndo {
    pub(super) fn between_scoped(
        before: &Config,
        after: &Config,
        scopes: &[String],
    ) -> Option<Self> {
        let before = serde_json::to_value(before).ok()?;
        let after = serde_json::to_value(after).ok()?;
        let changes = scopes
            .iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter_map(|pointer| {
                let previous = before.pointer(pointer).cloned();
                let expected = after.pointer(pointer).cloned();
                (previous != expected).then(|| Change {
                    pointer: pointer.clone(),
                    previous: previous.clone(),
                    expected: expected.clone(),
                })
            })
            .collect::<Vec<_>>();
        (!changes.is_empty()).then_some(Self { changes })
    }

    fn apply(&self, path: &std::path::Path) -> Result<(), String> {
        crate::config::patch_config(path, |raw| {
            // Compare effective values, so a setting omitted from an older
            // config is equivalent to its default. Patch only the owned fields
            // into the raw record to keep unrecognized future members intact.
            let effective = serde_json::to_value(
                serde_json::from_value::<Config>(raw.clone()).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            for change in &self.changes {
                if effective.pointer(&change.pointer) != change.expected.as_ref() {
                    return Err(CONFLICT.into());
                }
            }
            for change in &self.changes {
                let parts = change
                    .pointer
                    .strip_prefix('/')
                    .ok_or("Invalid preference path")?
                    .split('/')
                    .map(|part| part.replace("~1", "/").replace("~0", "~"))
                    .collect::<Vec<_>>();
                let mut cursor = &mut *raw;
                for part in &parts[..parts.len() - 1] {
                    let object = cursor.as_object_mut().ok_or("Invalid preference section")?;
                    cursor = object
                        .entry(part.clone())
                        .or_insert_with(|| serde_json::json!({}));
                }
                let object = cursor.as_object_mut().ok_or("Invalid preference section")?;
                let key = &parts[parts.len() - 1];
                if let Some(previous) = &change.previous {
                    object.insert(key.clone(), previous.clone());
                } else {
                    object.remove(key);
                }
            }
            Ok(())
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::{Application, app::Core};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Fixture(PathBuf);
    impl Fixture {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "cards-preference-undo-{name}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn config(&self, config: &Config) -> PathBuf {
            let path = self.0.join("config.json");
            std::fs::write(&path, serde_json::to_vec(config).unwrap()).unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn unrelated_interleaved_clock_change_is_never_owned_by_cpu_undo() {
        let fixture = Fixture::new("interleaved");
        let before = Config::default();
        let mut after = before.clone();
        after.devbox.cpu = !before.devbox.cpu;
        // Another writer changes the clock between the snapshots around CPU.
        after.clock.show_date = !before.clock.show_date;
        let scope = message_scope(&before, &Message::Devbox("cpu", after.devbox.cpu));
        let undo = PreferenceUndo::between_scoped(&before, &after, &scope).unwrap();
        let path = fixture.config(&after);
        let mut raw: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        raw["devbox"]["future_metric"] = serde_json::json!(42);
        std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
        undo.apply(&path).unwrap();
        let raw: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(raw["devbox"]["cpu"], before.devbox.cpu);
        assert_eq!(raw["clock"]["show_date"], after.clock.show_date);
        assert_eq!(raw["devbox"]["future_metric"], 42);
    }

    #[test]
    fn scoped_history_undo_keeps_sibling_changes_and_refuses_owned_conflicts() {
        let fixture = Fixture::new("nested");
        let before = Config::default();
        let mut after = before.clone();
        after.devbox.history.cpu = !before.devbox.history.cpu;
        after.devbox.history.memory = !before.devbox.history.memory;
        let scope = message_scope(
            &before,
            &Message::DevboxHistory("cpu", after.devbox.history.cpu),
        );
        let undo = PreferenceUndo::between_scoped(&before, &after, &scope).unwrap();
        let path = fixture.config(&after);
        undo.apply(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let raw: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(raw["devbox"]["history"]["cpu"], before.devbox.history.cpu);
        assert_eq!(
            raw["devbox"]["history"]["memory"],
            after.devbox.history.memory
        );
        assert_eq!(undo.apply(&path).unwrap_err(), CONFLICT);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn combined_and_tool_visibility_own_only_the_members_their_writers_change() {
        let mut config = Config::default();
        let fields = message_scope(&config, &Message::Widget(Widget::Glance, true));
        assert_eq!(fields.len(), 5);
        assert!(fields.contains(&"/glance/initialized".into()));
        assert!(!fields.contains(&"/enabled_widgets".into()));
        config.glance.enabled = true;
        config.glance.initialized = true;
        assert_eq!(
            message_scope(&config, &Message::Widget(Widget::Weather, false)),
            ["/glance/show_weather"]
        );
        assert_eq!(
            message_scope(&config, &Message::Widget(Widget::Glance, false)),
            ["/glance/enabled"]
        );
        assert_eq!(
            message_scope(&config, &Message::Widget(Widget::Focus, true)),
            ["/focus/enabled"]
        );
        assert_eq!(
            message_scope(&config, &Message::Tools(tools::Message::Player(None))),
            ["/music/player"]
        );
        assert!(message_scope(&config, &Message::Tools(tools::Message::Refresh)).is_empty());
    }

    fn isolated(name: &str) -> bool {
        if std::env::var("DESKTOP_CARDS_PREFERENCE_UNDO_TEST").as_deref() == Ok(name) {
            return false;
        }
        let fixture = Fixture::new(name);
        let path = fixture.config(&Config::default());
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("settings::preference_undo::tests::{name}"),
            ])
            .env("DESKTOP_CARDS_PREFERENCE_UNDO_TEST", name)
            .env("DESKTOP_CARDS_CONFIG", path)
            .env(
                "DESKTOP_CARDS_SETTINGS_CAPTURE_DIR",
                fixture.0.join("captures"),
            )
            .env_remove("DESKTOP_CARDS_DESKTOP_ROOT")
            .status()
            .unwrap();
        assert!(status.success());
        true
    }

    #[test]
    fn music_auto_can_be_undone_without_reverting_new_artwork_preferences() {
        if isolated("music_auto_can_be_undone_without_reverting_new_artwork_preferences") {
            return;
        }
        Config::save_tool(Widget::Music, serde_json::json!({"player": "firefox"})).unwrap();
        let (mut app, _) = Settings::init(Core::default(), super::super::Page::Music);
        let _ = app.update(Message::Tools(tools::Message::Card(
            crate::tools::cards::Action::MusicAuto,
        )));
        assert!(Config::load().music.player.is_none());
        assert!(app.preference_undo.is_some());
        Config::save_tool(Widget::Music, serde_json::json!({"show_artwork": false})).unwrap();
        super::super::tests::send_settings(&mut app, Message::UndoPreferences);
        let restored = Config::load();
        assert!(!app.error, "{}", app.status);
        assert_eq!(restored.music.player.as_deref(), Some("firefox"));
        assert!(!restored.music.show_artwork);
        assert!(!app.error, "{}", app.status);
    }

    #[test]
    fn clock_controls_save_independently_and_undo_without_losing_other_preferences() {
        if isolated("clock_controls_save_independently_and_undo_without_losing_other_preferences") {
            return;
        }
        let (mut app, _) = Settings::init(Core::default(), super::super::Page::Clock);
        let _ = app.update(Message::ClockAlignment(
            crate::config::ClockAlignment::Center,
        ));
        let _ = app.update(Message::ClockScale(150));
        assert_eq!(Config::load().clock.scale, 150);
        assert_eq!(
            Config::load().clock.alignment,
            crate::config::ClockAlignment::Center
        );
        super::super::tests::send_settings(&mut app, Message::UndoPreferences);
        assert_eq!(Config::load().clock.scale, 100);
        assert_eq!(
            Config::load().clock.alignment,
            crate::config::ClockAlignment::Center
        );
        let _ = app.update(Message::ClockVertical(true));
        assert!(Config::load().clock.center_vertically);
        let before = std::fs::read(Config::path().unwrap()).unwrap();
        let _ = app.update(Message::ClockScale(0));
        assert!(app.error);
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), before);
    }

    #[test]
    fn foundation_clock_choices_save_reset_and_undo_without_touching_date() {
        if isolated("foundation_clock_choices_save_reset_and_undo_without_touching_date") {
            return;
        }
        use crate::widget_style::resolver::{ClockFace, ClockSurface, ClockWeight};
        let (mut app, _) = Settings::init(Core::default(), super::super::Page::Clock);
        for message in [
            Message::ClockFace(Some(ClockFace::Analog)),
            Message::ClockWeight(Some(ClockWeight::Bold)),
            Message::ClockItalic(Some(true)),
            Message::ClockSurface(Some(ClockSurface::Panel)),
        ] {
            super::super::tests::send_settings(&mut app, message);
            assert!(!app.error, "{}", app.status);
        }
        let config = Config::load();
        assert_eq!(config.clock.face, Some(ClockFace::Analog));
        assert_eq!(config.clock.surface, Some(ClockSurface::Panel));
        Config::save_clock(serde_json::json!({"show_date":false})).unwrap();
        for _ in 0..4 {
            super::super::tests::send_settings(&mut app, Message::UndoPreferences);
            assert!(!app.error, "{}", app.status);
        }
        let clock = Config::load().clock;
        assert_eq!(
            (clock.face, clock.weight, clock.italic, clock.surface),
            (None, None, None, None)
        );
        assert!(!clock.show_date);
        super::super::tests::send_settings(&mut app, Message::ClockFace(Some(ClockFace::Analog)));
        super::super::tests::send_settings(&mut app, Message::ClockFace(None));
        assert_eq!(Config::load().clock.face, None);
        super::super::tests::send_settings(&mut app, Message::UndoPreferences);
        assert_eq!(Config::load().clock.face, Some(ClockFace::Analog));
    }

    #[test]
    fn fb14_depth_choice_undo_keeps_newer_clock_content() {
        if isolated("fb14_depth_choice_undo_keeps_newer_clock_content") {
            return;
        }
        let (mut app, _) = Settings::init(Core::default(), super::super::Page::Clock);
        super::super::tests::send_settings(&mut app, Message::ClockDepth(true));
        assert!(!app.error, "{}", app.status);
        assert!(Config::load().clock.depth);
        Config::save_clock(serde_json::json!({"show_date":false})).unwrap();
        super::super::tests::send_settings(&mut app, Message::UndoPreferences);
        assert!(!app.error, "{}", app.status);
        let clock = Config::load().clock;
        assert!(!clock.depth && !clock.show_date);
    }

    #[test]
    fn undo_during_debounce_reverts_the_latest_slider_without_undoing_older_clock_edit() {
        if isolated(
            "undo_during_debounce_reverts_the_latest_slider_without_undoing_older_clock_edit",
        ) {
            return;
        }
        let before = Config::load();
        let (mut app, _) = Settings::init(Core::default(), super::super::Page::Appearance);
        let _ = app.update(Message::ClockDate(!before.clock.show_date));
        assert!(app.preference_undo.is_some());
        let _ = app.update(Message::AppearanceValue(AppearanceControl::Opacity, 83));
        assert_eq!(Config::load().appearance.opacity, before.appearance.opacity);
        super::super::tests::send_settings(&mut app, Message::UndoPreferences);
        let after = Config::load();
        assert_eq!(after.appearance.opacity, before.appearance.opacity);
        assert_eq!(after.clock.show_date, !before.clock.show_date);
        assert!(!app.error, "{}", app.status);
        assert!(app.preference_undo.is_none());
    }

    #[test]
    fn failed_slider_flush_does_not_undo_the_older_preference_or_clear_its_error() {
        if isolated("failed_slider_flush_does_not_undo_the_older_preference_or_clear_its_error") {
            return;
        }
        let before = Config::load();
        let (mut app, _) = Settings::init(Core::default(), super::super::Page::Appearance);
        let _ = app.update(Message::ClockDate(!before.clock.show_date));
        let _ = app.update(Message::AppearanceValue(AppearanceControl::Opacity, 83));
        // Keep reads possible, but make the next locked write fail reliably.
        let path = Config::path().unwrap();
        let mut lock = path.as_os_str().to_os_string();
        lock.push(".lock");
        let lock = PathBuf::from(lock);
        std::fs::remove_file(&lock).unwrap();
        std::fs::create_dir(&lock).unwrap();
        super::super::tests::send_settings(&mut app, Message::UndoPreferences);
        let after = Config::load();
        assert_eq!(after.clock.show_date, !before.clock.show_date);
        assert_eq!(after.appearance.opacity, before.appearance.opacity);
        assert!(app.error);
        assert!(app.preference_undo.is_some());
    }
}
