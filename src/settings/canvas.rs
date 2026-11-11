//! A local composition of existing widget style and placement fields. Config
//! remains authoritative; one guarded store commit owns Apply and its Undo.
use super::{Config, Message as AppMessage, Settings, placement};
use crate::desktop::{actions, store::widget};
use cosmic::app::Task;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(1);
const ROOTS: [&str; 10] = [
    "clock",
    "moments",
    "widget_font",
    "clock_font",
    "widget_positions",
    "widget_sizes",
    "placement",
    "monitor_layouts",
    "widget_monitors",
    "widget_display",
];
const STYLES: [&str; 7] = [
    "/clock/face",
    "/clock/surface",
    "/clock/weight",
    "/clock/italic",
    "/clock/scale",
    "/clock_font",
    "/widget_font",
];
type Styles = BTreeMap<String, Value>;

#[derive(Clone, Debug)]
pub enum Message {
    Start,
    Style(&'static str, Value),
    Reset(&'static str),
    Undo,
    Apply(u64),
    Cancel,
}

pub(super) struct Draft {
    pub(super) id: u64,
    pub(super) moment: Option<crate::moments::Phase>,
    baseline: Value,
    styles: Styles,
    pub(super) projected: Config,
    history: Vec<(placement::EditState, Styles)>,
    frame: Frame,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Saved {
    // JSON stays a string inside the RON envelope; serde_json::Value requires
    // deserialize_any, which RON cannot reliably provide for nested values.
    #[serde(default)]
    moment: Option<crate::moments::Phase>,
    baseline: String,
    styles: String,
    display: Option<String>,
    frame: Frame,
    layout: placement::SavedPlacement,
}
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    screen: [f32; 2],
    output: Option<String>,
}
impl Frame {
    fn of(editor: &placement::PlacementEditor) -> Self {
        let snapshot = editor.preview_snapshot();
        Self {
            screen: snapshot.screen,
            output: snapshot.display.as_ref().and_then(|d| d.output.clone()),
        }
    }
    fn valid(&self) -> bool {
        self.screen
            .iter()
            .all(|v| v.is_finite() && *v > 0. && *v <= 32768.)
            && self
                .output
                .as_ref()
                .is_none_or(|v| crate::displays::valid_name(v))
    }
}
impl Draft {
    fn new(config: &Config, frame: Frame) -> Result<Self, String> {
        let path = Config::path().ok_or("Configuration directory unavailable")?;
        let raw =
            crate::config::parse_config_text(crate::config::read_config_text(&path)?.as_deref())?;
        let mut baseline = json!({});
        for key in ROOTS {
            if let Some(value) = raw.get(key) {
                baseline[key] = value.clone();
            }
        }
        Ok(Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            moment: config.moments.active,
            baseline,
            styles: BTreeMap::new(),
            projected: config.clone(),
            history: Vec::new(),
            frame,
        })
    }
    pub(super) fn remember(&mut self, geometry: placement::EditState) {
        if self.history.len() == 50 {
            self.history.remove(0);
        }
        self.history.push((geometry, self.styles.clone()));
    }
    fn refresh(&mut self, config: &Config) -> Result<(), String> {
        let mut raw = serde_json::to_value(config).map_err(|e| e.to_string())?;
        for (pointer, value) in &self.styles {
            widget::put(&mut raw, pointer, Some(value.clone()))?;
        }
        let mut projected: Config = serde_json::from_value(raw).map_err(|e| e.to_string())?;
        projected.desktop_palette = config.desktop_palette.clone();
        // A scheduled switch cannot retarget an existing composition.
        projected.moments.active = self.moment;
        self.projected = projected;
        Ok(())
    }
    fn target(&self, pointer: &str) -> String {
        if let Some(key) = pointer.strip_prefix("/clock/")
            && crate::moments::Moments::clock_member(key)
        {
            crate::moments::Moments::pointer(self.moment, key)
        } else {
            pointer.into()
        }
    }
    fn style(&mut self, pointer: &str, value: Value) -> Result<(), String> {
        if !STYLES.contains(&pointer) {
            return Err("Unsupported canvas style".into());
        }
        self.restore_style(&self.target(pointer), value)
    }
    fn restore_style(&mut self, pointer: &str, value: Value) -> Result<(), String> {
        // Saved pointers already name their layer. Never reinterpret them using
        // the active moment when this draft is reopened.
        let monitor = pointer
            .strip_prefix("/widget_monitors/")
            .and_then(|p| p.split_once('/'))
            .is_some_and(|(key, field)| {
                crate::drawer::Widget::parse(key).is_some()
                    && matches!(field, "display" | "hide_disconnected")
            });
        if (!STYLES.iter().any(|p| self.target(p) == pointer) && !monitor)
            || !widget::field(pointer).is_some_and(|field| field.accepts(&value))
        {
            return Err("Unsupported canvas style".into());
        }
        if widget::get(&self.baseline, pointer).unwrap_or(Value::Null) == value {
            self.styles.remove(pointer);
        } else {
            self.styles.insert(pointer.into(), value);
        }
        Ok(())
    }
}

pub(super) struct Apply {
    pub(super) id: u64,
    baseline: Value,
    styles: Styles,
    layout: placement::SavedPlacement,
}
impl Apply {
    pub(super) fn run(self) -> Result<Config, String> {
        let path = Config::path().ok_or("Configuration directory unavailable")?;
        let p = self.layout;
        actions::in_action("Widget positions", || {
            actions::guarded(Some(&self.baseline), || {
                Config::edit_layout_context_at(
                    &path,
                    &p.contexts,
                    &p.positions,
                    &p.saved_positions,
                    &p.sizes,
                    &p.saved_sizes,
                    &p.materialized,
                    crate::config::PlacementPreferences {
                        snap: p.snap,
                        columns: p.columns,
                    },
                    crate::config::PlacementPreferences {
                        snap: p.saved_snap,
                        columns: p.saved_columns,
                    },
                    |raw| {
                        for (pointer, value) in self.styles {
                            widget::put(raw, &pointer, Some(value))?;
                        }
                        Ok(())
                    },
                )
            })
        })
    }
}

/// Used only inside the existing isolated capture-command admission.
pub(super) fn capture_style(value: &Value) -> Option<Message> {
    let pointer = STYLES
        .iter()
        .copied()
        .find(|p| value.get("field").and_then(Value::as_str) == Some(*p))?;
    Some(Message::Style(pointer, value.get("value")?.clone()))
}
impl Settings {
    /// Widget location belongs to the same placement preview as its geometry.
    /// Existing monitor assignments outside Position keep their live contract.
    pub(super) fn preview_widget_monitor(
        &mut self,
        widget: crate::drawer::Widget,
        field: &str,
        value: Value,
    ) {
        if self.canvas.is_none() {
            let _ = self.update_canvas(Message::Start);
        }
        self.placement.canvas_stop_gesture();
        let geometry = self.placement.state();
        let Some(draft) = self.canvas.as_mut() else {
            return;
        };
        let before = draft.styles.clone();
        match draft.restore_style(&format!("/widget_monitors/{}/{field}", widget.key()), value) {
            Ok(()) => {
                if draft.styles != before {
                    let next = std::mem::replace(&mut draft.styles, before);
                    draft.remember(geometry);
                    draft.styles = next;
                }
                self.refresh_canvas();
                self.status = "Previewing widget location · Apply when ready".into();
                self.error = false;
            }
            Err(error) => self.failed(error),
        }
    }
    pub(super) fn canvas_retarget(&mut self) {
        if !self.canvas_dirty()
            && let Some(draft) = self.canvas.as_mut()
        {
            draft.frame = Frame::of(&self.placement);
        }
    }
    pub(super) fn canvas_config(&self) -> &Config {
        if self.canvas_visible() {
            self.canvas
                .as_ref()
                .map_or(&self.config, |draft| &draft.projected)
        } else {
            &self.config
        }
    }
    pub(super) fn canvas_dirty(&self) -> bool {
        self.canvas
            .as_ref()
            .is_some_and(|d| !d.styles.is_empty() || self.placement.dirty())
    }
    pub(super) fn canvas_can_undo(&self) -> bool {
        self.canvas.as_ref().is_some_and(|d| !d.history.is_empty())
    }
    pub(super) fn refresh_canvas(&mut self) {
        if let Some(draft) = self.canvas.as_mut() {
            if let Err(error) = draft.refresh(&self.config) {
                self.failed(error);
                return;
            }
            self.placement.canvas_project(&draft.projected);
        }
    }
    pub(super) fn update_canvas(&mut self, event: Message) -> Task<AppMessage> {
        match event {
            Message::Start => {
                if self.canvas.is_some() {
                    return Task::none();
                }
                if self.placement.dirty() || self.placement.dragging() {
                    self.failed("Finish or cancel the current arrangement first.".into());
                    return Task::none();
                }
                match Config::try_load() {
                    Ok(config) => {
                        self.config = config;
                        self.placement.canvas_end(&self.config);
                    }
                    Err(e) => {
                        self.failed(e);
                        return Task::none();
                    }
                }
                match Draft::new(&self.config, Frame::of(&self.placement)) {
                    Ok(draft) => {
                        self.placement.canvas_end(&self.config);
                        self.canvas = Some(draft);
                        self.arranging = true;
                        self.status.clear();
                        self.error = false;
                    }
                    Err(e) => self.failed(e),
                }
            }
            Message::Style(pointer, _) | Message::Reset(pointer) if self.canvas.is_none() => {
                let _ = pointer;
            }
            Message::Style(pointer, value) => {
                self.placement.canvas_stop_gesture();
                let geometry = self.placement.state();
                let draft = self.canvas.as_mut().unwrap();
                let before = draft.styles.clone();
                match draft.style(pointer, value) {
                    Ok(()) => {
                        if draft.styles != before {
                            let next = std::mem::replace(&mut draft.styles, before);
                            draft.remember(geometry);
                            draft.styles = next;
                        }
                        self.refresh_canvas();
                        self.status = "Previewing · Apply when ready".into();
                        self.error = false;
                    }
                    Err(e) => self.failed(e),
                }
            }
            Message::Reset(pointer) => {
                return self.update_canvas(Message::Style(pointer, Value::Null));
            }
            Message::Undo => {
                self.placement.canvas_stop_gesture();
                if let Some(draft) = self.canvas.as_mut()
                    && let Some((geometry, styles)) = draft.history.pop()
                {
                    self.placement.restore(geometry);
                    draft.styles = styles;
                    self.refresh_canvas();
                    self.status = "Preview change undone".into();
                    self.error = false;
                }
            }
            Message::Cancel => {
                if self.canvas.take().is_some() {
                    self.placement.canvas_end(&self.config);
                    self.font_menu = None;
                    self.arranging = false;
                    self.status = "Widget moves discarded".into();
                    self.error = false;
                }
            }
            Message::Apply(id) => {
                let Some(draft) = self.canvas.as_ref().filter(|d| d.id == id) else {
                    return Task::none();
                };
                if !self.canvas_dirty() || self.placement.dragging() {
                    return Task::none();
                }
                if Frame::of(&self.placement) != draft.frame
                    || (!self.displays.outputs().is_empty()
                        && self.displays.contexts(&self.config)
                            != self.placement.canvas_snapshot().contexts)
                {
                    self.failed("Displays changed. Your draft is kept; reconnect the original displays or cancel and start again.".into());
                    return Task::none();
                }
                let apply = Apply {
                    id,
                    baseline: draft.baseline.clone(),
                    styles: draft.styles.clone(),
                    layout: self.placement.canvas_apply_snapshot(),
                };
                #[cfg(test)]
                if !self.native_worker_needed() {
                    return self.finish_canvas(id, apply.run());
                }
                return self.background_canvas(apply);
            }
        }
        Task::none()
    }
    /// Leaving the arranging view keeps what you did: changes are saved
    /// (and Undo can take them back); with none, it just closes.
    pub(super) fn canvas_done(&self) -> Option<AppMessage> {
        let draft = self.canvas.as_ref()?;
        if self.native_busy || self.placement.dragging() {
            return None;
        }
        Some(AppMessage::Canvas(if self.canvas_dirty() {
            Message::Apply(draft.id)
        } else {
            Message::Cancel
        }))
    }
    pub(super) fn finish_canvas(
        &mut self,
        id: u64,
        result: Result<Config, String>,
    ) -> Task<AppMessage> {
        if self.canvas.as_ref().is_none_or(|d| d.id != id) {
            return Task::none();
        }
        match result {
            Ok(config) => {
                self.config = config;
                self.canvas = None;
                self.arranging = false;
                self.placement.canvas_end(&self.config);
                self.font_menu = None;
                let task = self.after_desktop_change();
                self.status = "Widget positions saved · Undo is available".into();
                self.error = false;
                task
            }
            Err(e) => {
                self.failed(e);
                Task::none()
            }
        }
    }
    pub(super) fn canvas_saved(&self) -> Option<Saved> {
        let draft = self.canvas.as_ref().filter(|_| self.canvas_dirty())?;
        Some(Saved {
            moment: draft.moment,
            baseline: serde_json::to_string(&draft.baseline).ok()?,
            styles: serde_json::to_string(&draft.styles).ok()?,
            display: self.placement.display.clone(),
            frame: draft.frame.clone(),
            layout: self.placement.canvas_snapshot(),
        })
    }
    pub(super) fn restore_canvas(&mut self, saved: Saved) -> Result<(), String> {
        if !saved.frame.valid()
            || saved.baseline.len() > 128 * 1024
            || saved.styles.len() > 8192
            || saved
                .display
                .as_ref()
                .is_some_and(|v| !crate::displays::valid_name(v))
        {
            return Err("Invalid canvas draft".into());
        }
        if self.canvas.is_none() && self.placement.dirty() {
            return Err("Finish the recovered layout before reopening this canvas.".into());
        }
        let baseline: Value = serde_json::from_str(&saved.baseline).map_err(|e| e.to_string())?;
        if baseline
            .as_object()
            .is_none_or(|m| m.keys().any(|key| !ROOTS.contains(&key.as_str())))
        {
            return Err("Unsupported canvas baseline".into());
        }
        let styles: Styles = serde_json::from_str(&saved.styles).map_err(|e| e.to_string())?;
        let mut draft = Draft {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            moment: saved.moment,
            baseline,
            styles: BTreeMap::new(),
            projected: self.config.clone(),
            history: Vec::new(),
            frame: saved.frame,
        };
        for (key, value) in styles {
            draft.restore_style(&key, value)?;
        }
        draft.refresh(&self.config)?;
        // Validate the whole record in a temporary editor before replacing any
        // recoverable placement already held by this window.
        let mut placement = placement::PlacementEditor::new(&self.config);
        placement.display = saved.display;
        placement.update_displays(&self.config, &self.displays);
        placement.restore_session(saved.layout)?;
        self.placement = placement;
        self.canvas = Some(draft);
        self.arranging = true;
        self.refresh_canvas();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
