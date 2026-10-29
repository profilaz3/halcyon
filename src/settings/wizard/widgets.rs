//! Bounded onboarding draft. Preview and commit share this placement plan.
use crate::desktop::store::widget::{self, WidgetGroup, WidgetPatch};
use crate::{config::Config, displays::Registry, drawer::Widget, layout_state::Snapshot};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const CHOICES: [Widget; 8] = [
    Widget::Clock,
    Widget::Weather,
    Widget::Calendar,
    Widget::Devbox,
    Widget::Notes,
    Widget::Focus,
    Widget::Music,
    Widget::Projects,
];
const DAILY: [Widget; 3] = [Widget::Clock, Widget::Weather, Widget::Calendar];
const VISIBILITY: [&str; 10] = [
    "/widgets_visible",
    "/enabled_widgets",
    "/glance/enabled",
    "/glance/initialized",
    "/glance/show_clock",
    "/glance/show_weather",
    "/glance/show_calendar",
    "/music/enabled",
    "/notes/enabled",
    "/focus/enabled",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub selected: Vec<String>,
    pub grouped: bool,
    pub order: Vec<String>,
    pub setup_order: Vec<String>,
    pub customized: bool,
    pub pending: bool,
    pub arranged: bool,
    #[serde(default)]
    pub arranged_on: Vec<String>,
    #[serde(skip)]
    pub target: String,
    pub display: Option<String>,
    // Read the old boolean only to migrate saved drafts once on resume.
    #[serde(default, skip_serializing)]
    pub move_here: bool,
    #[serde(default)]
    pub destinations: BTreeMap<String, Option<String>>,
    baseline: Value,
    #[serde(skip)]
    pub focused: Option<String>,
    #[serde(skip)]
    pub dragging: Option<String>,
    #[serde(skip)]
    pub over: Option<String>,
    #[serde(skip)]
    undo: Option<(Vec<String>, bool, bool, Vec<String>)>,
}

#[derive(Clone)]
pub struct Plan {
    pub config: Config,
    pub snapshot: Snapshot,
    pub visible: Vec<Widget>,
    pub elsewhere: Vec<Widget>,
    pub positions: BTreeMap<String, [f32; 2]>,
    pub error: Option<String>,
}

pub fn raw() -> Result<Value, String> {
    let path = Config::path().ok_or("Configuration directory unavailable")?;
    crate::config::parse_config_text(crate::config::read_config_text(&path)?.as_deref())
}
fn baseline(raw: &Value) -> Value {
    let mut out = serde_json::json!({});
    for field in widget::FIELDS
        .iter()
        .chain(widget::LOCAL_FIELDS)
        .filter(|f| f.group == WidgetGroup::Layout || f.pointer == "/widgets_visible")
    {
        if let Some(value) = widget::get(raw, field.pointer) {
            // The draft never carries notes, providers, folders or any content.
            let _ = widget::put(&mut out, field.pointer, Some(value));
        }
    }
    out
}
impl Draft {
    pub fn new(config: &Config, raw: &Value, snapshot: &Snapshot) -> Self {
        let selected = CHOICES
            .into_iter()
            .filter(|w| config.widget_selected(*w))
            .map(|w| w.key().to_owned())
            .collect();
        let mut order: Vec<_> = snapshot
            .widgets
            .iter()
            .filter(|g| Widget::parse(&g.key).is_some_and(|w| config.widget_visible(w)))
            .collect();
        // The current arrangement is the initial order; no automatic move yet.
        order.sort_by(|a, b| {
            a.position[0]
                .total_cmp(&b.position[0])
                .then(a.position[1].total_cmp(&b.position[1]))
        });
        let order: Vec<_> = order.iter().map(|g| g.key.clone()).collect();
        Self {
            selected,
            grouped: config.glance.enabled,
            setup_order: order.clone(),
            order,
            customized: false,
            pending: false,
            arranged: false,
            arranged_on: Vec::new(),
            target: String::new(),
            display: config.widget_display.clone(),
            move_here: false,
            destinations: BTreeMap::new(),
            baseline: baseline(raw),
            focused: None,
            dragging: None,
            over: None,
            undo: None,
        }
    }
    pub fn valid(&self) -> bool {
        let keys = |list: &[String], choices: bool| {
            list.len() <= 9
                && list.iter().collect::<BTreeSet<_>>().len() == list.len()
                && list
                    .iter()
                    .all(|s| Widget::parse(s).is_some_and(|w| !choices || CHOICES.contains(&w)))
        };
        keys(&self.selected, true)
            && keys(&self.order, false)
            && keys(&self.setup_order, false)
            && self
                .display
                .as_deref()
                .is_none_or(crate::displays::valid_name)
            && self.destinations.len() <= Widget::ALL.len()
            && self.destinations.iter().all(|(key, display)| {
                Widget::parse(key).is_some()
                    && display.as_deref().is_none_or(crate::displays::valid_name)
            })
            && self.arranged_on.len() <= 32
            && self
                .arranged_on
                .iter()
                .all(|s| s.is_empty() || crate::displays::valid_name(s))
            && self.baseline.to_string().len() <= 128 * 1024
            && baseline(&self.baseline) == self.baseline
            && widget::FIELDS
                .iter()
                .chain(widget::LOCAL_FIELDS)
                .all(|f| widget::get(&self.baseline, f.pointer).is_none_or(|v| f.accepts(&v)))
    }
    pub fn migrate_move(&mut self) {
        if self.move_here {
            for w in self.active() {
                self.destinations
                    .entry(w.key().into())
                    .or_insert_with(|| self.display.clone());
            }
            self.move_here = false;
        }
    }
    pub fn selected(&self, widget: Widget) -> bool {
        self.selected.iter().any(|k| k == widget.key())
    }
    pub fn active(&self) -> Vec<Widget> {
        let mut active: Vec<_> = CHOICES
            .into_iter()
            .filter(|w| self.selected(*w) && !(self.grouped && DAILY.contains(w)))
            .collect();
        if self.grouped && DAILY.into_iter().any(|w| self.selected(w)) {
            active.insert(0, Widget::Glance);
        }
        let mut result: Vec<_> = self
            .order
            .iter()
            .filter_map(|s| Widget::parse(s))
            .filter(|w| active.contains(w))
            .collect();
        for w in active {
            if !result.contains(&w) {
                result.push(w);
            }
        }
        result
    }
    pub fn toggle(&mut self, w: Widget) {
        if !CHOICES.contains(&w) {
            return;
        }
        self.undo = None;
        if self.selected(w) {
            self.selected.retain(|s| s != w.key());
        } else {
            self.selected.push(w.key().into());
        }
        self.edit();
    }
    fn edit(&mut self) {
        self.customized = true;
        self.pending = true;
        self.cancel_drag();
    }
    pub fn group(&mut self, on: bool) {
        if self.grouped != on {
            self.undo = None;
            self.grouped = on;
            self.edit();
        }
    }
    pub fn cancel_drag(&mut self) {
        self.dragging = None;
        self.over = None;
    }
    pub fn move_to(&mut self, key: &str, target: &str, visible: &[Widget]) {
        let mut order: Vec<_> = self.active().iter().map(|w| w.key().to_owned()).collect();
        if key == target
            || !visible.iter().any(|w| w.key() == key)
            || !visible.iter().any(|w| w.key() == target)
        {
            return;
        }
        let Some(from) = order.iter().position(|s| s == key) else {
            return;
        };
        let Some(to) = order.iter().position(|s| s == target) else {
            return;
        };
        self.undo = Some((
            self.order.clone(),
            self.arranged,
            self.pending,
            self.arranged_on.clone(),
        ));
        let item = order.remove(from);
        order.insert(to, item);
        self.order = order;
        self.focused = Some(key.into());
        self.arrange_target();
        self.edit();
    }
    pub fn reset_order(&mut self, visible: &[Widget]) {
        self.undo = Some((
            self.order.clone(),
            self.arranged,
            self.pending,
            self.arranged_on.clone(),
        ));
        // Replace only this display's subsequence, including newly enabled cards.
        let mut order: Vec<_> = self.active().iter().map(|w| w.key().to_owned()).collect();
        let local = |key: &String| visible.iter().any(|w| w.key() == key);
        let mut setup: Vec<_> = self
            .setup_order
            .iter()
            .filter(|key| local(key))
            .cloned()
            .collect();
        for key in order.iter().filter(|key| local(key)) {
            if !setup.contains(key) {
                setup.push(key.clone());
            }
        }
        let mut setup = setup.into_iter();
        for key in &mut order {
            if local(key) {
                *key = setup.next().expect("complete local order");
            }
        }
        self.order = order;
        self.arrange_target();
        self.edit();
    }
    pub fn undo(&mut self) {
        if let Some((order, arranged, pending, arranged_on)) = self.undo.take() {
            self.order = order;
            self.arranged = arranged;
            self.pending = pending;
            self.arranged_on = arranged_on;
            self.cancel_drag();
        }
    }
    fn arrange_target(&mut self) {
        self.arranged = true;
        if !self.arranged_on.contains(&self.target) && self.arranged_on.len() < 32 {
            self.arranged_on.push(self.target.clone());
        }
    }
    pub fn refresh_current(&mut self, config: &Config, raw: &Value, snapshot: &Snapshot) {
        if self.pending {
            return;
        }
        let fresh = Self::new(config, raw, snapshot);
        self.selected = fresh.selected;
        self.grouped = fresh.grouped;
        self.baseline = fresh.baseline;
        if !self.customized {
            self.order = fresh.order;
            self.setup_order = fresh.setup_order;
        }
    }
    pub fn can_undo(&self) -> bool {
        self.undo.is_some()
    }
    pub fn committed(&mut self, raw: &Value) {
        self.baseline = baseline(raw);
        self.pending = false;
        self.arranged = false;
        self.arranged_on.clear();
        self.move_here = false;
        self.destinations.clear();
        self.undo = None;
        self.cancel_drag();
    }
    pub fn choose_display(&mut self, display: Option<String>) {
        self.display = display;
        self.cancel_drag();
        self.undo = None;
    }
    pub fn move_here(&mut self) {
        self.undo = None;
        // Capture both the set and the actual destination, independently of preview.
        let destination = (!self.target.is_empty()).then(|| self.target.clone());
        for w in self.active() {
            self.destinations
                .insert(w.key().into(), destination.clone());
        }
        self.arrange_target();
        self.edit();
    }
    pub fn config(&self, current: &Config) -> Config {
        let mut config = current.clone();
        if !self.pending {
            return config;
        }
        config.widgets_visible = !self.selected.is_empty();
        config.enabled_widgets = self
            .selected
            .iter()
            .filter(|s| Widget::parse(s).is_some_and(|w| !w.is_tool()))
            .cloned()
            .collect();
        config.glance.enabled = self.grouped && DAILY.into_iter().any(|w| self.selected(w));
        config.glance.initialized = true;
        config.glance.show_clock = self.selected(Widget::Clock);
        config.glance.show_weather = self.selected(Widget::Weather);
        config.glance.show_calendar = self.selected(Widget::Calendar);
        config.music.enabled = self.selected(Widget::Music);
        config.notes.enabled = self.selected(Widget::Notes);
        config.focus.enabled = self.selected(Widget::Focus);
        for (key, destination) in &self.destinations {
            config
                .widget_monitors
                .entry(key.clone())
                .or_default()
                .display = destination.clone();
        }
        config
    }
    pub fn plan(
        &self,
        current: &Config,
        mut snapshot: Snapshot,
        displays: &Registry,
        insets: [f32; 4],
    ) -> Plan {
        let config = self.config(current);
        let contexts = if displays.outputs().is_empty() {
            snapshot.widget_displays.clone()
        } else {
            displays.placements(&config)
        };
        let output = snapshot.display.as_ref().and_then(|d| d.output.as_ref());
        let arranged = self.arranged
            && self.arranged_on.iter().any(|d| {
                d == output.map(String::as_str).unwrap_or("")
                    || (!d.is_empty() && displays.same_choice(Some(d), output.map(String::as_str)))
            });
        let on_display = |w: &Widget| contexts.is_empty() || contexts.get(w.key()) == output;
        let visible: Vec<_> = self.active().into_iter().filter(on_display).collect();
        let elsewhere: Vec<_> = self
            .active()
            .into_iter()
            .filter(|w| !on_display(w))
            .collect();
        snapshot.widget_displays = contexts.clone();
        let effective = if displays.outputs().is_empty() {
            config.on_displays(&contexts)
        } else {
            displays.effective_config(&config)
        };
        snapshot = crate::layout_state::project(snapshot, &effective);
        // Even with hidden bars, normalized runtime positions reserve these
        // desktop edges. Pack inside the same bounds before encoding positions.
        let [horizontal, vertical] = crate::layout::EDGE_GAPS;
        let [left, top, right, bottom] = [
            insets[0].max(horizontal),
            insets[1].max(vertical),
            insets[2].max(horizontal),
            insets[3].max(vertical),
        ];
        let available = [
            snapshot.screen[0] - left - right,
            snapshot.screen[1] - top - bottom,
        ];
        let mut positions = BTreeMap::new();
        let mut error = None;
        let mut occupied = Vec::new();
        let prior: Config = serde_json::from_value(self.baseline.clone()).unwrap_or_default();
        for g in &snapshot.widgets {
            if Widget::parse(&g.key)
                .is_some_and(|w| visible.contains(&w) && prior.widget_visible(w))
                && !arranged
            {
                occupied.push([
                    g.position[0] - left,
                    g.position[1] - top,
                    g.size[0],
                    g.size[1],
                ]);
            }
        }
        let (mut x, mut y, mut column) = (0f32, 0f32, 0f32);
        for w in &visible {
            let g = snapshot
                .widgets
                .iter_mut()
                .find(|g| g.key == w.key())
                .expect("complete geometry");
            if !self.pending || (!arranged && prior.widget_visible(*w)) {
                continue;
            }
            let minimum = crate::layout::minimum_size(*w);
            let size = [g.size[0].max(minimum[0]), g.size[1].max(minimum[1])];
            let fits = available.iter().all(|n| *n > 0.)
                && size[0] <= available[0]
                && size[1] <= available[1];
            let point = if !fits {
                None
            } else if arranged {
                if y > 0. && y + size[1] > available[1] {
                    x += column + 20.;
                    y = 0.;
                    column = 0.;
                }
                if x + size[0] > available[0] {
                    None
                } else {
                    let p = [x, y];
                    y += size[1] + 20.;
                    column = column.max(size[0]);
                    Some(p)
                }
            } else {
                crate::layout::free_spot_in_area(
                    [g.position[0] - left, g.position[1] - top],
                    size,
                    &occupied,
                    available,
                )
            };
            let Some(point) = point else {
                error = Some(format!(
                    "{} needs more room. Choose fewer widgets or another display.",
                    w.title()
                ));
                break;
            };
            occupied.push([point[0], point[1], size[0], size[1]]);
            g.position = [point[0] + left, point[1] + top];
            // Do not change saved sizes. The plan uses the same effective dimensions as the desktop.
            positions.insert(
                w.key().into(),
                crate::layout_state::encode_position(*w, g.position, g.size, snapshot.screen),
            );
        }
        Plan {
            config,
            snapshot,
            visible,
            elsewhere,
            positions,
            error,
        }
    }
    pub fn patch(&self, plan: &Plan, displays: &Registry) -> Result<WidgetPatch, String> {
        if let Some(error) = &plan.error {
            return Err(error.clone());
        }
        let mut patch = WidgetPatch::new();
        if !self.pending {
            return Ok(patch);
        }
        let desired = serde_json::to_value(&plan.config).map_err(|e| e.to_string())?;
        let defaults = serde_json::to_value(
            serde_json::from_value::<Config>(self.baseline.clone()).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let mut set = |pointer: &str, value: Option<Value>| -> Result<(), String> {
            let before = widget::get(&self.baseline, pointer);
            let value = if widget::get(&defaults, pointer) == value {
                before.clone()
            } else {
                value
            };
            patch.set(pointer, before, value)
        };
        for pointer in VISIBILITY {
            set(pointer, widget::get(&desired, pointer))?;
        }
        let contexts = if displays.outputs().is_empty() {
            plan.snapshot.widget_displays.clone()
        } else {
            displays.contexts(&plan.config)
        };
        for (key, position) in &plan.positions {
            if let Some(display) = contexts.get(key) {
                let pointer = format!("/monitor_layouts/{key}");
                let mut layouts =
                    widget::get(&self.baseline, &pointer).unwrap_or_else(|| serde_json::json!({}));
                let effective = if displays.outputs().is_empty() {
                    plan.config.on_displays(&contexts)
                } else {
                    displays.effective_config(&plan.config)
                };
                let map = layouts
                    .as_object_mut()
                    .ok_or("Invalid saved monitor layouts")?;
                let layout = map
                    .entry(display.clone())
                    .or_insert_with(|| serde_json::json!({"size":effective.widget_sizes.get(key)}));
                layout
                    .as_object_mut()
                    .ok_or("Invalid saved monitor layout")?
                    .insert("position".into(), serde_json::json!(position));
                set(&pointer, Some(layouts))?;
            } else {
                set(
                    &format!("/widget_positions/{key}"),
                    Some(serde_json::json!(position)),
                )?;
            }
        }
        for (key, destination) in &self.destinations {
            set(
                &format!("/widget_monitors/{key}/display"),
                Some(serde_json::json!(destination)),
            )?;
        }
        // Geometry and placement policy are dependencies, even when we only write positions.
        for pointer in ["/widget_display".to_owned()].into_iter().chain(
            Widget::ALL
                .iter()
                .filter(|w| plan.visible.contains(w) || plan.positions.contains_key(w.key()))
                .flat_map(|w| {
                    [
                        format!("/widget_sizes/{}", w.key()),
                        format!("/widget_positions/{}", w.key()),
                        format!("/monitor_layouts/{}", w.key()),
                        format!("/widget_monitors/{}/display", w.key()),
                        format!("/widget_monitors/{}/hide_disconnected", w.key()),
                    ]
                }),
        ) {
            if !patch.iter().any(|(p, _)| p == pointer) {
                let before = widget::get(&self.baseline, &pointer);
                patch.set(&pointer, before.clone(), before)?;
            }
        }
        Ok(patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn polish050_hidden_widgets_keep_saved_section_choices_without_running_providers() {
        let raw = serde_json::json!({
            "widgets_visible": false, "enabled_widgets": ["clock", "calendar", "projects"],
            "glance": {"enabled": true, "show_clock": true, "show_weather": true, "show_calendar": false},
            "notes": {"enabled": true}
        });
        let config: Config = serde_json::from_value(raw.clone()).unwrap();
        assert!(config.widget_selected(Widget::Glance));
        assert!(config.widget_selected(Widget::Weather));
        assert!(!config.widget_selected(Widget::Calendar));
        assert!(config.widget_selected(Widget::Notes));
        assert!(Widget::ALL.into_iter().all(|w| !config.data_visible(w)));
        let (_, _, snapshot, _) = fixture();
        let mut draft = Draft::new(&config, &raw, &snapshot);
        assert!(draft.selected(Widget::Clock) && draft.selected(Widget::Weather));
        assert!(!draft.selected(Widget::Calendar));
        assert!(!draft.pending && !draft.config(&config).widgets_visible);
        draft.group(false);
        assert!(draft.selected(Widget::Weather));
        assert!(!draft.selected(Widget::Calendar));
        assert!(draft.active().contains(&Widget::Weather));
        draft.group(true);
        assert!(draft.active().contains(&Widget::Glance));
        assert!(!draft.active().contains(&Widget::Weather));
        assert_eq!(draft.config(&config).notes.enabled, config.notes.enabled);
    }

    #[test]
    fn onboarding_small_display_uses_space_between_reserved_edges() {
        let raw = serde_json::json!({
            "enabled_widgets":["clock"], "glance":{"enabled":false},
            "widget_sizes":{"clock":[360.,200.],"weather":[360.,200.]},
            "widget_positions":{"clock":[0.,32. / 304.]}
        });
        let config: Config = serde_json::from_value(raw.clone()).unwrap();
        let (_, _, mut snapshot, _) = fixture();
        snapshot.screen = [800., 600.];
        let mut draft = Draft::new(&config, &raw, &snapshot);
        draft.toggle(Widget::Weather);
        let plan = draft.plan(
            &config,
            snapshot,
            &Registry::default(),
            [24., 80., 24., 80.],
        );
        assert!(
            plan.error.is_none(),
            "There is room beside or below Clock: {:?}",
            plan.error
        );
        let clock = plan
            .snapshot
            .widgets
            .iter()
            .find(|g| g.key == "clock")
            .unwrap();
        let weather = plan
            .snapshot
            .widgets
            .iter()
            .find(|g| g.key == "weather")
            .unwrap();
        assert_eq!(clock.position, [24., 80.]);
        assert!(weather.position[0] >= 24. && weather.position[0] + weather.size[0] <= 776.);
        assert!(weather.position[1] >= 80. && weather.position[1] + weather.size[1] <= 520.);
        assert!((0..2).any(|axis| clock.position[axis]
            + clock.size[axis]
            + crate::layout::WIDGET_GAP
            <= weather.position[axis]
            || weather.position[axis] + weather.size[axis] + crate::layout::WIDGET_GAP
                <= clock.position[axis]));
    }
    fn fixture() -> (Config, Value, Snapshot, Draft) {
        let raw = serde_json::json!({"enabled_widgets":["clock","weather"],"glance":{"enabled":false},"notes":{"enabled":true},"widget_positions":{"notes":[0.8,0.7]},"project_roots":["/keep"],"location":{"name":"Zagreb","latitude":45.8,"longitude":16.0},"unknown":{"keep":true}});
        let c: Config = serde_json::from_value(raw.clone()).unwrap();
        let s = Snapshot {
            screen: [1920., 1080.],
            monitors: Default::default(),
            widget_displays: Default::default(),
            display: None,
            widgets: Widget::ALL
                .into_iter()
                .map(|w| crate::layout_state::WidgetGeometry {
                    key: w.key().into(),
                    size: [320., 180.],
                    natural_size: Some([320., 180.]),
                    natural_height: None,
                    position: [50., 50.],
                })
                .collect(),
        };
        let d = Draft::new(&c, &raw, &s);
        (c, raw, s, d)
    }
    #[test]
    fn grouped_sections_and_every_tool_preserve_content() {
        let (c, raw, s, mut d) = fixture();
        for w in CHOICES {
            if !d.selected(w) {
                d.toggle(w);
            }
        }
        d.group(true);
        assert_eq!(d.active().len(), 6);
        assert!(!d.active().contains(&Widget::Clock));
        let projected = d.config(&c);
        assert!(
            projected.glance.show_clock
                && projected.glance.show_weather
                && projected.glance.show_calendar
        );
        assert_eq!(projected.project_roots, c.project_roots);
        assert_eq!(projected.location, c.location);
        d.group(false);
        assert_eq!(d.active().len(), 8);
        let p = d.plan(&c, s, &Registry::default(), [24.; 4]);
        assert!(p.error.is_none(), "{:?}", p.error);
        let patch = d.patch(&p, &Registry::default()).unwrap();
        let mut applied = raw.clone();
        for (ptr, e) in patch.changes() {
            widget::put(&mut applied, ptr, e.desired.clone()).unwrap();
        }
        assert_eq!(applied["unknown"], raw["unknown"]);
        assert_eq!(applied["location"], raw["location"]);
        assert!(!d.baseline.to_string().contains("Zagreb"));
        assert!(!d.baseline.to_string().contains("/keep"));
    }
    #[test]
    fn reorder_projects_the_same_normalized_positions_that_are_written() {
        let (c, _, s, mut d) = fixture();
        let before = d.active();
        d.move_to("notes", before[0].key(), &before);
        let plan = d.plan(&c, s.clone(), &Registry::default(), [24., 80., 24., 80.]);
        assert!(plan.error.is_none());
        assert_eq!(plan.visible[0], Widget::Notes);
        let patch = d.patch(&plan, &Registry::default()).unwrap();
        for w in &plan.visible {
            let g = plan
                .snapshot
                .widgets
                .iter()
                .find(|g| g.key == w.key())
                .unwrap();
            let ptr = format!("/widget_positions/{}", w.key());
            let saved: [f32; 2] = serde_json::from_value(
                patch
                    .iter()
                    .find(|(p, _)| *p == ptr)
                    .unwrap()
                    .1
                    .desired
                    .clone()
                    .unwrap(),
            )
            .unwrap();
            let actual = crate::layout_state::decode_position(*w, saved, g.size, s.screen);
            assert!(
                (actual[0] - g.position[0]).abs() < 0.01
                    && (actual[1] - g.position[1]).abs() < 0.01
            );
        }
        d.undo();
        assert_eq!(d.active(), before);
        assert!(!d.pending);
    }
    #[test]
    fn selecting_a_new_widget_keeps_existing_manual_positions_and_hidden_sizes() {
        let (mut c, _, s, mut d) = fixture();
        c.widget_sizes.insert("focus".into(), [360., 240.]);
        d.toggle(Widget::Music);
        let plan = d.plan(&c, s, &Registry::default(), [24.; 4]);
        assert!(plan.error.is_none());
        assert_eq!(plan.positions.keys().collect::<Vec<_>>(), vec!["music"]);
        assert_eq!(plan.config.widget_positions, c.widget_positions);
        assert_eq!(plan.config.widget_sizes, c.widget_sizes);
    }
    #[test]
    fn overflow_blocks_transaction_without_omitting_selected_widgets() {
        let (c, _, mut s, mut d) = fixture();
        s.screen = [400., 300.];
        d.reset_order(&d.active());
        let p = d.plan(&c, s, &Registry::default(), [24.; 4]);
        assert!(p.error.is_some());
        assert_eq!(p.visible.len(), 3);
        assert!(d.patch(&p, &Registry::default()).is_err());
    }
    #[test]
    fn empty_selection_is_valid_and_skips_placement() {
        let (c, _, s, mut d) = fixture();
        for w in CHOICES {
            if d.selected(w) {
                d.toggle(w);
            }
        }
        let p = d.plan(&c, s, &Registry::default(), [24.; 4]);
        assert!(p.visible.is_empty());
        assert!(p.positions.is_empty());
        assert!(p.error.is_none());
        assert!(!p.config.widgets_visible);
        assert!(!p.config.glance.enabled);
        assert!(!d.patch(&p, &Registry::default()).unwrap().is_empty());
    }
    #[test]
    fn persistent_draft_is_bounded_and_has_no_content_or_drag_state() {
        let (_, _, _, mut d) = fixture();
        d.dragging = Some("clock".into());
        d.focused = Some("clock".into());
        let serialized = serde_json::to_value(&d).unwrap();
        assert!(serialized.get("dragging").is_none());
        let restored: Draft = serde_json::from_value(serialized.clone()).unwrap();
        assert!(restored.valid());
        assert!(restored.dragging.is_none());
        let mut bad = serialized;
        bad["selected"] = serde_json::json!(["clock", "clock"]);
        assert!(!serde_json::from_value::<Draft>(bad).unwrap().valid());
        d.baseline["project_roots"] = serde_json::json!(["/private"]);
        assert!(!d.valid());
    }
    #[test]
    fn monitor_layout_keeps_other_displays_and_other_widgets() {
        let (_, mut raw, mut s, mut d) = fixture();
        raw["monitor_layouts"] = serde_json::json!({"clock":{"A":{"position":[0.5,0.5],"size":[320.,180.]},"B":{"position":[0.2,0.3]}}});
        let c = serde_json::from_value(raw.clone()).unwrap();
        d.baseline = baseline(&raw);
        s.monitors.insert("A".into(), s.screen);
        s.monitors.insert("B".into(), s.screen);
        s.display = Some(crate::layout_state::DisplayState {
            output: Some("A".into()),
            ready: true,
            epoch: 1,
        });
        for w in Widget::ALL {
            s.widget_displays.insert(
                w.key().into(),
                if w == Widget::Notes { "B" } else { "A" }.into(),
            );
        }
        d.target = "A".into();
        d.reset_order(&d.active());
        let p = d.plan(&c, s, &Registry::default(), [24.; 4]);
        assert_eq!(p.elsewhere, vec![Widget::Notes]);
        assert!(!p.positions.contains_key("notes"));
        let patch = d.patch(&p, &Registry::default()).unwrap();
        let value = patch
            .iter()
            .find(|(p, _)| *p == "/monitor_layouts/clock")
            .unwrap()
            .1
            .desired
            .as_ref()
            .unwrap();
        assert_eq!(value["B"], raw["monitor_layouts"]["clock"]["B"]);
        assert_eq!(
            value["A"]["size"],
            raw["monitor_layouts"]["clock"]["A"]["size"]
        );
    }
    #[test]
    fn auditfix_ob01_preview_does_not_retarget_captured_moves_or_new_selections() {
        let (c, raw, snapshot, mut draft) = fixture();
        draft.target = "B".into();
        draft.choose_display(Some("B".into()));
        draft.move_here();
        draft.choose_display(Some("A".into()));
        draft.target = "A".into();
        draft.toggle(Widget::Focus);
        let serialized = serde_json::to_value(&draft).unwrap();
        let draft: Draft = serde_json::from_value(serialized).unwrap();
        assert!(draft.valid());
        let plan = draft.plan(&c, snapshot, &Registry::default(), [24.; 4]);
        let patch = draft.patch(&plan, &Registry::default()).unwrap();
        let mut applied = raw;
        for (pointer, entry) in patch.changes() {
            widget::put(&mut applied, pointer, entry.desired.clone()).unwrap();
        }
        for key in ["clock", "weather", "notes"] {
            assert_eq!(
                plan.config.widget_monitors[key].display.as_deref(),
                Some("B")
            );
            assert_eq!(applied["widget_monitors"][key]["display"], "B");
        }
        assert!(!draft.destinations.contains_key("focus"));
        assert_eq!(draft.display.as_deref(), Some("A"));
    }

    #[test]
    fn auditfix_ob02_setup_order_preserves_other_display_and_local_undo() {
        let (c, _, snapshot, mut draft) = fixture();
        draft.toggle(Widget::Focus);
        draft.setup_order = ["clock", "weather", "notes", "focus"]
            .map(String::from)
            .to_vec();
        draft.order = draft.setup_order.clone();
        draft.target = "A".into();
        draft.move_to("weather", "clock", &[Widget::Clock, Widget::Weather]);
        draft.target = "B".into();
        draft.move_to("focus", "notes", &[Widget::Notes, Widget::Focus]);
        let before = draft.order.clone();
        let mut a = snapshot.clone();
        a.display = Some(crate::layout_state::DisplayState {
            output: Some("A".into()),
            ready: true,
            epoch: 1,
        });
        for w in [Widget::Clock, Widget::Weather, Widget::Notes, Widget::Focus] {
            a.widget_displays.insert(
                w.key().into(),
                if matches!(w, Widget::Clock | Widget::Weather) {
                    "A"
                } else {
                    "B"
                }
                .into(),
            );
        }
        let before_plan = draft.plan(&c, a.clone(), &Registry::default(), [24.; 4]);
        draft.reset_order(&[Widget::Notes, Widget::Focus]);
        assert_eq!(draft.order, ["weather", "clock", "notes", "focus"]);
        assert_eq!(draft.arranged_on, ["A", "B"]);
        let after_plan = draft.plan(&c, a, &Registry::default(), [24.; 4]);
        assert_eq!(before_plan.positions, after_plan.positions);
        draft.undo();
        assert_eq!(draft.order, before);
    }

    #[test]
    fn auditfix_ob03_pack_uses_content_height_and_preserves_saved_viewports() {
        let (_, mut raw, mut snapshot, _) = fixture();
        raw["enabled_widgets"] = serde_json::json!(["devbox"]);
        raw["widget_sizes"] = serde_json::json!({"devbox":[360,180],"notes":[360,180]});
        let config: Config = serde_json::from_value(raw.clone()).unwrap();
        let system = snapshot
            .widgets
            .iter_mut()
            .find(|g| g.key == "devbox")
            .unwrap();
        system.size = [360., 226.];
        system.natural_size = Some([360., 226.]);
        system.natural_height = Some(226.);
        let mut draft = Draft::new(&config, &raw, &snapshot);
        draft.setup_order = vec!["devbox".into(), "notes".into()];
        draft.reset_order(&draft.active());
        let plan = draft.plan(
            &config,
            snapshot.clone(),
            &Registry::default(),
            [24., 88., 24., 88.],
        );
        assert!(plan.error.is_none(), "{:?}", plan.error);
        let geometry = |key| plan.snapshot.widgets.iter().find(|g| g.key == key).unwrap();
        assert_eq!(geometry("devbox").size, [360., 226.]);
        assert_eq!(geometry("devbox").position, [24., 88.]);
        assert_eq!(geometry("notes").position, [24., 334.]);
        let mut applied = raw.clone();
        for (pointer, entry) in draft.patch(&plan, &Registry::default()).unwrap().changes() {
            widget::put(&mut applied, pointer, entry.desired.clone()).unwrap();
        }
        assert_eq!(applied["widget_sizes"], raw["widget_sizes"]);
        for widget in [Widget::Devbox, Widget::Notes] {
            let content = (widget == Widget::Devbox).then_some(226.);
            let size = crate::layout_state::effective_explicit_size(
                widget,
                [360., 180.],
                content,
                snapshot.screen,
            );
            let actual = crate::layout_state::decode_position(
                widget,
                plan.positions[widget.key()],
                size,
                snapshot.screen,
            );
            assert!((actual[1] - geometry(widget.key()).position[1]).abs() < 0.01);
        }
        snapshot.screen = [420., 420.];
        assert!(
            draft
                .plan(&config, snapshot, &Registry::default(), [24.; 4])
                .error
                .is_some()
        );
    }
}
