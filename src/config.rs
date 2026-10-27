use crate::drawer::Widget;
use cosmic::iced::Color;
use serde::{Deserialize, Serialize};

pub mod monitors;
mod widget_colors;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Location {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    /// Local city metadata. Older preferences remain readable; solar rules
    /// require an explicit city confirmation when its zone is missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(skip)]
    pub design_problem: Option<String>,
    pub theme: String,
    /// Read-only desktop colors, cached outside the saved configuration.
    /// Rendering never reads files or changes COSMIC preferences.
    #[serde(skip)]
    pub desktop_palette: Option<Palette>,
    #[serde(skip)]
    pub theme_cache: std::cell::RefCell<Option<crate::desktop_theme::Cache>>,
    /// Widget choices the saved file holds: the person's own, even where
    /// they equal the defaults. Not saved; filled when a file is read.
    #[serde(skip)]
    pub saved: SavedChoices,
    pub location: Option<Location>,
    pub enabled_widgets: Vec<String>,
    pub widgets_visible: bool,
    pub widget_order: Vec<String>,
    pub meeting_notice_minutes: i64,
    pub calendar_show_today: bool,
    pub calendar_show_tomorrow: bool,
    pub calendar_private: bool,
    pub project_roots: Vec<String>,
    pub refresh_intervals: RefreshIntervals,
    pub appearance: Appearance,
    pub materials: crate::materials::Materials,
    #[serde(default = "yes")]
    pub material_motion: bool,
    /// Widget text font family; `None` uses the widget style's own.
    pub widget_font: Option<String>,
    /// The clock's font family; `None` follows `widget_font`, then the style.
    pub clock_font: Option<String>,
    pub clock: ClockPreferences,
    pub moments: crate::moments::Moments,
    pub recipes: crate::recipes::Applied,
    pub weather: WeatherPreferences,
    pub glance: GlancePreferences,
    pub devbox: DevboxPreferences,
    /// Local compositor connection name; None chooses a deterministic fallback.
    pub widget_display: Option<String>,
    pub widget_monitors: std::collections::BTreeMap<String, monitors::WidgetMonitor>,
    pub monitor_layouts: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, monitors::MonitorLayout>,
    >,
    pub widget_positions: std::collections::BTreeMap<String, [f32; 2]>,
    /// Explicit logical-pixel dimensions; absence preserves content-sized layout.
    pub widget_sizes: std::collections::BTreeMap<String, [f32; 2]>,
    pub placement: PlacementPreferences,
    pub music: MusicPreferences,
    pub notes: NotesPreferences,
    pub focus: FocusPreferences,
    /// The edge sidebar slides in and out; off, it appears and hides at
    /// once (Pop!_OS › Motion).
    pub sidebar_slide: bool,
    pub settings_animations: bool,
    pub setup_dismissed: bool,
    /// First-run setup was finished, skipped or cancelled.
    pub onboarded: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            design_problem: None,
            theme: "lavender".into(),
            desktop_palette: None,
            saved: SavedChoices::default(),
            theme_cache: Default::default(),
            location: None,
            widgets_visible: true,
            enabled_widgets: Widget::ALL
                .into_iter()
                .filter(|widget| !widget.floating())
                .map(|w| w.key().into())
                .collect(),
            widget_order: ["weather", "devbox", "calendar", "projects"]
                .map(String::from)
                .to_vec(),
            meeting_notice_minutes: 10,
            calendar_show_today: true,
            calendar_show_tomorrow: false,
            calendar_private: false,
            project_roots: vec![],
            refresh_intervals: RefreshIntervals::default(),
            appearance: Appearance::default(),
            materials: Default::default(),
            material_motion: true,
            widget_font: None,
            clock_font: None,
            clock: ClockPreferences::default(),
            moments: Default::default(),
            recipes: Default::default(),
            weather: WeatherPreferences::default(),
            glance: GlancePreferences::default(),
            devbox: DevboxPreferences::default(),
            widget_display: None,
            widget_monitors: Default::default(),
            monitor_layouts: Default::default(),
            widget_positions: std::collections::BTreeMap::new(),
            widget_sizes: Default::default(),
            placement: PlacementPreferences::default(),
            music: MusicPreferences::default(),
            notes: NotesPreferences::default(),
            focus: FocusPreferences::default(),
            sidebar_slide: true,
            settings_animations: true,
            setup_dismissed: false,
            onboarded: false,
        }
    }
}
impl Config {
    /// Personal system widgets reuse the same sampler and only request their parts.
    /// Built-in preferences stay intact for Use built-in and Undo.
    pub fn devbox_sampling(&self) -> std::borrow::Cow<'_, DevboxPreferences> {
        let Some(recipe) = &self.recipes.devbox else {
            return std::borrow::Cow::Borrowed(&self.devbox);
        };
        use crate::recipes::Part;
        let has = |part| recipe.parts.contains(&part);
        std::borrow::Cow::Owned(DevboxPreferences {
            cpu: has(Part::Cpu),
            memory: has(Part::Memory),
            memory_cache: false,
            swap: false,
            disk: has(Part::Disk),
            network: has(Part::Network),
            gpu: has(Part::Gpu),
            uptime: has(Part::Uptime),
            services: false,
            history: Default::default(),
            order: Vec::new(),
        })
    }

    pub fn path() -> Option<std::path::PathBuf> {
        std::env::var_os("DESKTOP_CARDS_CONFIG")
            .map(std::path::PathBuf::from)
            .or_else(|| crate::paths::config_dir().map(|dir| dir.join("config.json")))
    }
    pub fn load() -> Self {
        let problem = crate::design::commands::pending_problem();
        match (Self::try_load(), problem) {
            (Ok(config), None) => config,
            (result, pending) => {
                let error = pending.or_else(|| result.err());
                if crate::design::commands::pending_problem().is_some()
                    || Self::path()
                        .as_deref()
                        .is_some_and(crate::design::storage::managed)
                {
                    Self {
                        design_problem: error,
                        widgets_visible: false,
                        enabled_widgets: vec![],
                        onboarded: true,
                        ..Self::default()
                    }
                } else {
                    Self::default()
                }
            }
        }
    }
    pub fn try_load() -> Result<Self, String> {
        let path = Self::path().ok_or("Configuration directory unavailable")?;
        Self::read_at(&path)
    }
    fn read_at(path: &std::path::Path) -> Result<Self, String> {
        let text = read_config_text(path)?.ok_or("No saved settings")?;
        let mut config = Self::from_bytes(text.as_bytes())?;
        config.validate()?;
        config.normalize_policy();
        let _ = config.refresh_desktop_palette();
        Ok(config)
    }
    /// A saved file's contents, remembering which widget choices it holds.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let saved = SavedChoices::of(&value);
        let mut config: Self = serde_json::from_value(value).map_err(|e| e.to_string())?;
        config.saved = saved;
        Ok(config)
    }
    pub(crate) fn from_saved_value(value: serde_json::Value) -> Result<Self, String> {
        let saved = SavedChoices::of(&value);
        let mut config: Self = serde_json::from_value(value).map_err(|e| e.to_string())?;
        config.saved = saved;
        config.normalize_policy();
        let _ = config.refresh_desktop_palette();
        Ok(config)
    }
    /// Old files infer the source; explicit policy is authoritative thereafter.
    fn normalize_policy(&mut self) {
        if let Some(policy) = self.appearance.color_policy {
            self.appearance.wallpaper_colors = policy == ColorPolicy::Wallpaper;
            if policy == ColorPolicy::Theme {
                self.appearance.accent = None;
            }
        }
    }
    fn validate(&self) -> Result<(), String> {
        self.recipes.validate()?;
        self.validate_monitors()?;
        if self
            .widget_display
            .as_deref()
            .is_some_and(|name| !crate::displays::valid_name(name))
        {
            return Err("Choose a valid display connection name".into());
        }
        if !matches!(self.placement.columns, 12 | 16 | 24) {
            return Err("Choose a supported desktop grid size".into());
        }
        if !(1..=60).contains(&self.meeting_notice_minutes) {
            return Err("Meeting highlights must be between 1 and 60 minutes".into());
        }
        if !(70..=100).contains(&self.appearance.opacity) {
            return Err("Card opacity must be between 70 and 100 percent".into());
        }
        if self.appearance.corner_radius > 32 {
            return Err("Card corners must be between 0 and 32 pixels".into());
        }
        if self.appearance.border_opacity > 50 {
            return Err("Card outlines must be between 0 and 50 percent".into());
        }
        if self.glance.opacity > 100 {
            return Err("Glance panel opacity must be between 0 and 100 percent".into());
        }
        if !(50..=200).contains(&self.clock.scale) {
            return Err("Clock size must be between 50 and 200 percent".into());
        }
        for position in self.widget_positions.values() {
            if position
                .iter()
                .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
            {
                return Err("Widget positions must be within the desktop".into());
            }
        }
        if self
            .widget_sizes
            .iter()
            .any(|(key, size)| Widget::parse(key).is_none() || !crate::layout::valid_size(*size))
        {
            return Err(
                "Widget sizes must contain two finite dimensions between 1 and 32768 pixels".into(),
            );
        }
        self.materials.validate()?;
        self.music.validate()?;
        self.focus.validate()?;
        for widget in Widget::ALL.into_iter().filter(|widget| !widget.is_tool()) {
            let (min, max) = RefreshIntervals::bounds(widget);
            if !(min..=max).contains(&self.refresh_intervals.seconds(widget)) {
                return Err(format!(
                    "{} refresh must be between {min} and {max} seconds",
                    widget.title()
                ));
            }
        }
        Ok(())
    }
    pub fn palette(&self) -> Palette {
        if let Some(roles) = self.theme_roles() {
            return roles.card;
        }
        let mut palette = match self.appearance.color_source {
            ColorSource::Desktop => self
                .desktop_palette
                .unwrap_or_else(|| Palette::named(&self.theme)),
            ColorSource::Custom => self
                .appearance
                .accent
                .map_or_else(|| Palette::named(&self.theme), Palette::from_accent),
        };
        palette.background.a = f32::from(self.appearance.opacity.clamp(70, 100)) / 100.;
        if self.appearance.contrast_assist {
            palette = crate::widget_style::readable(palette);
        }
        crate::materials::palette(
            palette,
            self.materials.widgets(),
            self.appearance.contrast_assist,
        )
    }
    pub fn widget_visible(&self, widget: Widget) -> bool {
        if !self.widgets_visible {
            return false;
        }
        if widget == Widget::Glance || widget.is_tool() {
            return self.widget_enabled(widget);
        }
        if self.glance.enabled
            && matches!(widget, Widget::Clock | Widget::Weather | Widget::Calendar)
        {
            return false;
        }
        self.widget_enabled(widget)
    }
    /// Saved individual visibility is retained while Glance combines its sections.
    pub fn widget_enabled(&self, widget: Widget) -> bool {
        match widget {
            Widget::Glance => return self.glance.enabled,
            Widget::Music => return self.music.enabled,
            Widget::Notes => return self.notes.enabled,
            Widget::Focus => return self.focus.enabled,
            _ => {}
        }
        self.enabled_widgets.iter().any(|key| key == widget.key())
    }
    pub fn data_visible(&self, widget: Widget) -> bool {
        if !self.widgets_visible {
            return false;
        }
        if self.glance.enabled {
            match widget {
                Widget::Clock => return self.glance.show_clock,
                Widget::Weather => return self.glance.show_weather,
                Widget::Calendar => return self.glance.show_calendar,
                _ => {}
            }
        }
        self.widget_visible(widget)
    }
    /// Glance and the tools are placed by their own position in every mode.
    pub fn has_custom_positions(&self) -> bool {
        self.widget_positions
            .keys()
            .chain(self.widget_sizes.keys())
            .chain(self.monitor_layouts.iter().filter_map(|(key, layouts)| {
                layouts
                    .values()
                    .any(|layout| layout.position.is_some() || layout.size.is_some())
                    .then_some(key)
            }))
            .any(|key| Widget::parse(key).is_none_or(|widget| !widget.floating()))
    }
    pub fn save_changes(changes: serde_json::Value) -> Result<Self, String> {
        let path = Self::path().ok_or("Configuration directory unavailable")?;
        Self::save_changes_at(&path, changes)
    }
    fn save_changes_at(path: &std::path::Path, changes: serde_json::Value) -> Result<Self, String> {
        let changes = changes.as_object().ok_or("Settings must be an object")?;
        let value = patch_config(path, |value| {
            for (key, change) in changes {
                value[key] = change.clone();
            }
            Ok(())
        })?;
        Self::from_saved_value(value)
    }
    pub fn save_refresh(widget: Widget, seconds: u64) -> Result<Self, String> {
        let path = Self::path().ok_or("Configuration directory unavailable")?;
        Self::save_refresh_at(&path, widget, seconds)
    }
    pub fn save_appearance(changes: serde_json::Value) -> Result<Self, String> {
        let path = Self::path().ok_or("Configuration directory unavailable")?;
        Self::save_section_at(&path, "appearance", changes)
    }
    /// Legacy direct palette entrypoint. Graphical shared Colors uses the
    /// desktop journal; this local-only writer also clears a picked accent.
    #[allow(dead_code)]
    pub fn save_widget_theme(theme: &str) -> Result<Self, String> {
        let path = Self::path().ok_or("Configuration directory unavailable")?;
        Self::save_widget_theme_at(&path, theme)
    }
    fn save_widget_theme_at(path: &std::path::Path, theme: &str) -> Result<Self, String> {
        if !Palette::CHOICES.iter().any(|(name, _)| *name == theme) {
            return Err("Choose a supported widget palette".into());
        }
        let value = patch_config(path, |value| {
            value["theme"] = theme.into();
            if value.get("appearance").is_none() {
                value["appearance"] = serde_json::json!({});
            }
            value["appearance"]
                .as_object_mut()
                .ok_or("Appearance settings must be an object")?
                .insert(
                    "color_source".into(),
                    serde_json::json!(ColorSource::Custom),
                );
            value["appearance"]
                .as_object_mut()
                .expect("checked")
                .remove("accent");
            Ok(())
        })?;
        Self::from_saved_value(value)
    }
    pub fn save_widget_color_source(source: ColorSource) -> Result<Self, String> {
        Self::save_appearance(serde_json::json!({"color_source": source}))
    }
    pub fn save_clock(changes: serde_json::Value) -> Result<Self, String> {
        let path = Self::path().ok_or("Configuration directory unavailable")?;
        Self::save_section_at(&path, "clock", changes)
    }
    pub fn save_weather(changes: serde_json::Value) -> Result<Self, String> {
        let path = Self::path().ok_or("Configuration directory unavailable")?;
        Self::save_section_at(&path, "weather", changes)
    }
    pub fn save_glance(changes: serde_json::Value) -> Result<Self, String> {
        let path = Self::path().ok_or("Configuration directory unavailable")?;
        Self::save_glance_at(&path, changes)
    }
    fn save_glance_at(path: &std::path::Path, changes: serde_json::Value) -> Result<Self, String> {
        let changes = changes
            .as_object()
            .ok_or("Glance settings must be an object")?;
        let value = patch_config(path, |value| {
            let latest: Self =
                serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
            if value.get("glance").is_none() {
                value["glance"] = serde_json::json!({});
            }
            let glance = value["glance"]
                .as_object_mut()
                .ok_or("Glance settings must be an object")?;
            // Editing sections before the first enable must not discard those
            // choices when the user subsequently turns the combined widget on.
            if !latest.glance.initialized
                && ["show_clock", "show_weather", "show_calendar"]
                    .iter()
                    .any(|key| changes.contains_key(*key))
            {
                for (widget, key) in [
                    (Widget::Clock, "show_clock"),
                    (Widget::Weather, "show_weather"),
                    (Widget::Calendar, "show_calendar"),
                ] {
                    glance.insert(key.into(), latest.widget_enabled(widget).into());
                }
                glance.insert("initialized".into(), true.into());
            }
            for (key, change) in changes {
                glance.insert(key.clone(), change.clone());
            }
            Ok(())
        })?;
        Self::from_saved_value(value)
    }
    /// The runtime knows the measured Clock origin; insert its initial Glance
    /// position without overwriting a placement saved concurrently in Settings.
    pub fn seed_glance_position(position: [f32; 2]) -> Result<Self, String> {
        let path = Self::path().ok_or("Configuration directory unavailable")?;
        Self::seed_glance_position_at(&path, position)
    }
    fn seed_glance_position_at(path: &std::path::Path, position: [f32; 2]) -> Result<Self, String> {
        let value = patch_config(path, |value| {
            if value.get("widget_positions").is_none() {
                value["widget_positions"] = serde_json::json!({});
            }
            value["widget_positions"]
                .as_object_mut()
                .ok_or("Widget positions must be an object")?
                .entry("glance")
                .or_insert_with(|| serde_json::json!(position));
            Ok(())
        })?;
        Self::from_saved_value(value)
    }
    pub fn save_devbox_history(key: &str, enabled: bool) -> Result<Self, String> {
        let path = Self::path().ok_or("Configuration directory unavailable")?;
        Self::save_devbox_history_at(&path, key, enabled)
    }
    fn save_devbox_history_at(
        path: &std::path::Path,
        key: &str,
        enabled: bool,
    ) -> Result<Self, String> {
        if !matches!(key, "cpu" | "memory" | "disk" | "network" | "gpu") {
            return Err("Choose a supported history graph".into());
        }
        let value = patch_config(path, |value| {
            if value.get("devbox").is_none() {
                value["devbox"] = serde_json::json!({});
            }
            let devbox = value["devbox"]
                .as_object_mut()
                .ok_or("Devbox settings must be an object")?;
            let history = devbox
                .entry("history")
                .or_insert_with(|| serde_json::json!({}));
            history
                .as_object_mut()
                .ok_or("History settings must be an object")?
                .insert(key.into(), enabled.into());
            Ok(())
        })?;
        Self::from_saved_value(value)
    }
    /// Saves members of a tool's section (`music`, `notes` or `focus`).
    pub fn save_tool(widget: Widget, changes: serde_json::Value) -> Result<Self, String> {
        if !widget.is_tool() {
            return Err("Choose a desktop tool".into());
        }
        let path = Self::path().ok_or("Configuration directory unavailable")?;
        Self::save_section_at(&path, widget.key(), changes)
    }
    pub fn save_devbox(changes: serde_json::Value) -> Result<Self, String> {
        let path = Self::path().ok_or("Configuration directory unavailable")?;
        Self::save_section_at(&path, "devbox", changes)
    }
    #[cfg(test)]
    fn save_layout_draft_at(
        path: &std::path::Path,
        positions: &std::collections::BTreeMap<String, [f32; 2]>,
        baseline_positions: &std::collections::BTreeMap<String, [f32; 2]>,
        sizes: &std::collections::BTreeMap<String, [f32; 2]>,
        baseline_sizes: &std::collections::BTreeMap<String, [f32; 2]>,
        implicit_positions: &std::collections::BTreeMap<String, [f32; 2]>,
        placement: PlacementPreferences,
        baseline_placement: PlacementPreferences,
    ) -> Result<Self, String> {
        Self::save_layout_context_at(
            path,
            &Default::default(),
            positions,
            baseline_positions,
            sizes,
            baseline_sizes,
            implicit_positions,
            placement,
            baseline_placement,
        )
    }
    pub fn save_layout_on_displays(
        contexts: &monitors::Contexts,
        positions: &std::collections::BTreeMap<String, [f32; 2]>,
        baseline_positions: &std::collections::BTreeMap<String, [f32; 2]>,
        sizes: &std::collections::BTreeMap<String, [f32; 2]>,
        baseline_sizes: &std::collections::BTreeMap<String, [f32; 2]>,
        implicit_positions: &std::collections::BTreeMap<String, [f32; 2]>,
        placement: PlacementPreferences,
        baseline_placement: PlacementPreferences,
    ) -> Result<Self, String> {
        let path = Self::path().ok_or("Configuration directory unavailable")?;
        Self::save_layout_context_at(
            &path,
            contexts,
            positions,
            baseline_positions,
            sizes,
            baseline_sizes,
            implicit_positions,
            placement,
            baseline_placement,
        )
    }
    fn save_layout_context_at(
        path: &std::path::Path,
        contexts: &monitors::Contexts,
        positions: &std::collections::BTreeMap<String, [f32; 2]>,
        baseline_positions: &std::collections::BTreeMap<String, [f32; 2]>,
        sizes: &std::collections::BTreeMap<String, [f32; 2]>,
        baseline_sizes: &std::collections::BTreeMap<String, [f32; 2]>,
        implicit_positions: &std::collections::BTreeMap<String, [f32; 2]>,
        placement: PlacementPreferences,
        baseline_placement: PlacementPreferences,
    ) -> Result<Self, String> {
        Self::edit_layout_context_at(
            path,
            contexts,
            positions,
            baseline_positions,
            sizes,
            baseline_sizes,
            implicit_positions,
            placement,
            baseline_placement,
            |_| Ok(()),
        )
    }
    pub(crate) fn edit_layout_context_at(
        path: &std::path::Path,
        contexts: &monitors::Contexts,
        positions: &std::collections::BTreeMap<String, [f32; 2]>,
        baseline_positions: &std::collections::BTreeMap<String, [f32; 2]>,
        sizes: &std::collections::BTreeMap<String, [f32; 2]>,
        baseline_sizes: &std::collections::BTreeMap<String, [f32; 2]>,
        implicit_positions: &std::collections::BTreeMap<String, [f32; 2]>,
        placement: PlacementPreferences,
        baseline_placement: PlacementPreferences,
        extra: impl FnOnce(&mut serde_json::Value) -> Result<(), String>,
    ) -> Result<Self, String> {
        // Apply only edits made in this draft while holding the shared file lock.
        // Another editor may have moved untouched cards or changed the grid.
        let value = patch_config(path, |value| {
            let original_positions = value.get("widget_positions").cloned();
            let original_sizes = value.get("widget_sizes").cloned();
            let projected = serde_json::from_value::<Config>(value.clone())
                .map_err(|e| e.to_string())?
                .on_displays(contexts);
            // Project only monitor-owned members. Re-serializing unrelated
            // f32 values changes JSON precision and creates false conflicts.
            for key in contexts.keys() {
                for (section, map) in [
                    ("widget_positions", &projected.widget_positions),
                    ("widget_sizes", &projected.widget_sizes),
                ] {
                    if value.get(section).is_none() {
                        value[section] = serde_json::json!({});
                    }
                    let members = value[section]
                        .as_object_mut()
                        .ok_or("Invalid widget layout")?;
                    if let Some(entry) = map.get(key) {
                        members.insert(key.clone(), serde_json::json!(entry));
                    } else {
                        members.remove(key);
                    }
                }
            }
            let sizes_changed = sizes != baseline_sizes;
            if positions != baseline_positions || (sizes_changed && !implicit_positions.is_empty())
            {
                if value.get("widget_positions").is_none() {
                    value["widget_positions"] = serde_json::json!({});
                }
                let latest = value["widget_positions"]
                    .as_object_mut()
                    .ok_or("Widget positions must be an object")?;
                for key in baseline_positions.keys() {
                    if !positions.contains_key(key) {
                        latest.remove(key);
                    }
                }
                for (key, position) in positions {
                    if baseline_positions.get(key) != Some(position) {
                        latest.insert(key.clone(), serde_json::json!(position));
                    }
                }
                // A resized neighbor can change an automatic stack's anchors.
                // Pin the unchanged positions shown in the draft, but only if
                // they were implicit and no other editor has since placed them.
                for (key, position) in implicit_positions {
                    if sizes_changed && positions.get(key) == Some(position) {
                        latest
                            .entry(key.clone())
                            .or_insert_with(|| serde_json::json!(position));
                    }
                }
            }
            if sizes != baseline_sizes {
                if value.get("widget_sizes").is_none() {
                    value["widget_sizes"] = serde_json::json!({});
                }
                let latest = value["widget_sizes"]
                    .as_object_mut()
                    .ok_or("Widget sizes must be an object")?;
                for key in baseline_sizes.keys() {
                    if !sizes.contains_key(key) {
                        latest.remove(key);
                    }
                }
                for (key, size) in sizes {
                    if baseline_sizes.get(key) != Some(size) {
                        latest.insert(key.clone(), serde_json::json!(size));
                    }
                }
            }
            if placement != baseline_placement {
                if value.get("placement").is_none() {
                    value["placement"] = serde_json::json!({});
                }
                let latest = value["placement"]
                    .as_object_mut()
                    .ok_or("Placement settings must be an object")?;
                if placement.snap != baseline_placement.snap {
                    latest.insert("snap".into(), placement.snap.into());
                }
                if placement.columns != baseline_placement.columns {
                    latest.insert("columns".into(), placement.columns.into());
                }
            }
            for (key, display) in contexts {
                let before_position = projected.widget_positions.get(key).copied();
                let before_size = projected.widget_sizes.get(key).copied();
                let position: Option<[f32; 2]> = value["widget_positions"]
                    .get(key)
                    .map(|v| serde_json::from_value(v.clone()))
                    .transpose()
                    .map_err(|e| e.to_string())?;
                let size: Option<[f32; 2]> = value["widget_sizes"]
                    .get(key)
                    .map(|v| serde_json::from_value(v.clone()))
                    .transpose()
                    .map_err(|e| e.to_string())?;
                if position != before_position || size != before_size {
                    if value.get("monitor_layouts").is_none() {
                        value["monitor_layouts"] = serde_json::json!({});
                    }
                    if value["monitor_layouts"].get(key).is_none() {
                        value["monitor_layouts"][key] = serde_json::json!({});
                    }
                    value["monitor_layouts"][key][display] =
                        serde_json::json!(monitors::MonitorLayout { position, size });
                }
                // Temporary placements never overwrite the original/default layout.
                for (section, original) in [
                    ("widget_positions", &original_positions),
                    ("widget_sizes", &original_sizes),
                ] {
                    if let Some(old) = original.as_ref().and_then(|m| m.get(key)) {
                        value[section][key] = old.clone();
                    } else {
                        value[section].as_object_mut().unwrap().remove(key);
                    }
                }
            }
            extra(value)
        })?;
        Self::from_saved_value(value)
    }
    fn save_section_at(
        path: &std::path::Path,
        section: &str,
        changes: serde_json::Value,
    ) -> Result<Self, String> {
        let changes = changes.as_object().ok_or("Settings must be an object")?;
        let value = patch_config(path, |value| {
            if value.get(section).is_none() {
                value[section] = serde_json::json!({});
            }
            let settings = value[section]
                .as_object_mut()
                .ok_or_else(|| format!("{section} settings must be an object"))?;
            for (key, change) in changes {
                settings.insert(key.clone(), change.clone());
            }
            Ok(())
        })?;
        Self::from_saved_value(value)
    }
    fn save_refresh_at(
        path: &std::path::Path,
        widget: Widget,
        seconds: u64,
    ) -> Result<Self, String> {
        let value = patch_config(path, |value| {
            if value.get("refresh_intervals").is_none() {
                value["refresh_intervals"] = serde_json::json!({});
            }
            let intervals = value["refresh_intervals"]
                .as_object_mut()
                .ok_or("Refresh intervals must be an object")?;
            let key = if widget == Widget::Glance {
                "clock"
            } else {
                widget.key()
            };
            intervals.insert(key.into(), seconds.into());
            Ok(())
        })?;
        Self::from_saved_value(value)
    }
    pub fn clock_margin(&self) -> i32 {
        if self.widget_visible(Widget::Clock) {
            82
        } else {
            -500
        }
    }
    pub fn calendar_margin(&self) -> i32 {
        if self.widget_visible(Widget::Calendar) {
            42
        } else {
            -368
        }
    }
    pub fn save_widget(&mut self, widget: Widget, visible: bool) -> Result<(), String> {
        let path = Self::path().ok_or("Configuration directory unavailable")?;
        self.save_widget_at(&path, widget, visible)
    }
    fn save_widget_at(
        &mut self,
        path: &std::path::Path,
        widget: Widget,
        visible: bool,
    ) -> Result<(), String> {
        let value = patch_config(path, |value| {
            if widget.is_tool() {
                // Tools keep their visibility in their own section, so older
                // profiles (which list `enabled_widgets`) leave them alone.
                let key = widget.key();
                if value.get(key).is_none() {
                    value[key] = serde_json::json!({});
                }
                value[key]
                    .as_object_mut()
                    .ok_or_else(|| format!("{} settings must be an object", widget.title()))?
                    .insert("enabled".into(), visible.into());
                return Ok(());
            }
            let latest: Self =
                serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
            let section = match widget {
                Widget::Clock => Some("show_clock"),
                Widget::Weather => Some("show_weather"),
                Widget::Calendar => Some("show_calendar"),
                _ => None,
            };
            if widget == Widget::Glance || (latest.glance.enabled && section.is_some()) {
                if value.get("glance").is_none() {
                    value["glance"] = serde_json::json!({});
                }
                let glance = value["glance"]
                    .as_object_mut()
                    .ok_or("Glance settings must be an object")?;
                if widget == Widget::Glance {
                    if visible && !latest.glance.initialized {
                        for (widget, key) in [
                            (Widget::Clock, "show_clock"),
                            (Widget::Weather, "show_weather"),
                            (Widget::Calendar, "show_calendar"),
                        ] {
                            glance.insert(key.into(), latest.widget_enabled(widget).into());
                        }
                        glance.insert("initialized".into(), true.into());
                    }
                    glance.insert("enabled".into(), visible.into());
                } else if let Some(key) = section {
                    glance.insert(key.into(), visible.into());
                }
                return Ok(());
            }
            let mut widgets: Vec<String> = match value.get("enabled_widgets") {
                Some(list) => serde_json::from_value(list.clone()).map_err(|e| e.to_string())?,
                None => Self::default().enabled_widgets,
            };
            widgets.retain(|key| key != widget.key());
            if visible {
                widgets.push(widget.key().into());
            }
            value["enabled_widgets"] = serde_json::json!(widgets);
            Ok(())
        })?;
        *self = Self::from_saved_value(value)?;
        Ok(())
    }
    pub fn ordered_cards(&self) -> Vec<Widget> {
        let mut order = Vec::new();
        for widget in self
            .widget_order
            .iter()
            .filter_map(|key| Widget::parse(key))
            .chain(Widget::CARDS)
        {
            if Widget::CARDS.contains(&widget) && !order.contains(&widget) {
                order.push(widget);
            }
        }
        order
    }
    pub fn save_order(&mut self, order: &[Widget]) -> Result<(), String> {
        let path = Self::path().ok_or("Configuration directory unavailable")?;
        self.save_order_at(&path, order)
    }
    pub fn ordered_right_cards(&self) -> Vec<Widget> {
        self.ordered_cards()
            .into_iter()
            .filter(|widget| matches!(widget, Widget::Weather | Widget::Devbox | Widget::Projects))
            .filter(|widget| !self.glance.enabled || *widget != Widget::Weather)
            .collect()
    }
    fn save_order_at(&mut self, path: &std::path::Path, order: &[Widget]) -> Result<(), String> {
        let saved = patch_config(path, |value| {
            let latest: Self =
                serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
            let keys: Vec<String> = if latest.glance.enabled && !order.contains(&Widget::Weather) {
                // Weather belongs to Glance for now. Reorder only the visible
                // right-column slots, keeping Weather's old slot for restoration.
                let mut moved = order.iter();
                latest
                    .ordered_cards()
                    .into_iter()
                    .filter(|widget| {
                        matches!(widget, Widget::Weather | Widget::Devbox | Widget::Projects)
                    })
                    .map(|widget| {
                        if order.contains(&widget) {
                            *moved.next().unwrap_or(&widget)
                        } else {
                            widget
                        }
                    })
                    .map(|widget| widget.key().into())
                    .collect()
            } else {
                order.iter().map(|widget| widget.key().into()).collect()
            };
            value["widget_order"] = serde_json::json!(keys);
            Ok(())
        })?;
        self.widget_order = serde_json::from_value(saved["widget_order"].clone())
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PlacementPreferences {
    pub snap: bool,
    /// Retained for configurations written before Fine became the only grid.
    pub columns: u8,
}
impl PlacementPreferences {
    pub const FINE_COLUMNS: u8 = 24;
}
impl Default for PlacementPreferences {
    fn default() -> Self {
        Self {
            snap: true,
            columns: Self::FINE_COLUMNS,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColorSource {
    /// Existing installations keep their selected palette on upgrade.
    #[default]
    Custom,
    Desktop,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Appearance {
    /// None preserves the pre-Themes rendering exactly.
    pub family: Option<crate::desktop_theme::Family>,
    /// Explicit generation provenance. Legacy equal colors are not ownership.
    pub native_mode_palettes: u8,
    /// Explicit source choice; None adopts the older release's inference once.
    pub color_policy: Option<ColorPolicy>,
    /// Explicit per-property customizations (desktop::actions owns bit names).
    pub overrides: u64,
    pub surface: Option<crate::desktop_theme::Surface>,
    /// Cached wallpaper polarity, resolved off the rendering thread.
    pub wallpaper_light: bool,
    pub opacity: u8,
    pub corner_radius: u16,
    pub border_opacity: u8,
    /// Opt-in: enough surface opacity for text over any wallpaper.
    pub contrast_assist: bool,
    pub color_source: ColorSource,
    /// The app-owned source color. Widget ink is adjusted for contrast.
    pub accent: Option<[u8; 3]>,
    /// Default scope for an explicit color Apply.
    pub sync_colors: bool,
    /// Explicit opt-in: derive app-owned colors when the wallpaper changes.
    pub wallpaper_colors: bool,
    /// Source version acknowledged by a color Apply, including manual tweaks.
    pub wallpaper_color_stamp: u64,
    /// Legacy command-line palette links. Widget style never applies them.
    pub match_wallpaper: bool,
    #[serde(default)]
    pub match_cosmic_colors: bool,
    /// Choosing a widget style also styles the dock and panel.
    pub style_bars: bool,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            family: None,
            native_mode_palettes: 0,
            color_policy: None,
            overrides: 0,
            surface: None,
            wallpaper_light: false,
            opacity: 92,
            corner_radius: 24,
            border_opacity: 20,
            contrast_assist: false,
            color_source: ColorSource::Custom,
            accent: None,
            sync_colors: true,
            wallpaper_colors: false,
            wallpaper_color_stamp: 0,
            match_wallpaper: true,
            match_cosmic_colors: false,
            style_bars: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColorPolicy {
    Wallpaper,
    Theme,
    Custom,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct WeatherPreferences {
    /// Shared by the standalone weather card and Daily overview.
    pub show_dew_point: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ClockPreferences {
    pub format_24h: bool,
    pub show_date: bool,
    pub alignment: ClockAlignment,
    pub scale: u16,
    pub center_vertically: bool,
    pub face: Option<crate::widget_style::resolver::ClockFace>,
    pub weight: Option<crate::widget_style::resolver::ClockWeight>,
    pub italic: Option<bool>,
    pub surface: Option<crate::widget_style::resolver::ClockSurface>,
    /// Prepared wallpaper occlusion; unsupported contexts keep the plain clock.
    pub depth: bool,
}
impl Default for ClockPreferences {
    fn default() -> Self {
        Self {
            format_24h: true,
            show_date: true,
            alignment: ClockAlignment::Left,
            scale: 100,
            center_vertically: false,
            face: None,
            weight: None,
            italic: None,
            surface: None,
            depth: false,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClockAlignment {
    #[default]
    Left,
    Center,
    Right,
}
impl ClockAlignment {
    pub fn layout(self) -> cosmic::iced::Alignment {
        match self {
            Self::Left => cosmic::iced::Alignment::Start,
            Self::Center => cosmic::iced::Alignment::Center,
            Self::Right => cosmic::iced::Alignment::End,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GlanceStyle {
    #[default]
    Transparent,
    Panel,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GlanceDensity {
    #[default]
    Compact,
    Comfortable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct GlancePreferences {
    pub enabled: bool,
    pub initialized: bool,
    pub style: GlanceStyle,
    pub density: GlanceDensity,
    pub show_clock: bool,
    pub show_weather: bool,
    pub show_calendar: bool,
    pub show_tomorrow: bool,
    pub opacity: u8,
    /// With a widget style applied, `style` still decides this widget's
    /// background instead of the style's surface.
    pub own_background: bool,
    /// Font families for this widget only, over the widget fonts.
    pub font: Option<String>,
    pub clock_font: Option<String>,
}
impl GlancePreferences {
    pub fn auto_width(&self, panel: bool) -> f32 {
        match (panel, self.density) {
            (false, GlanceDensity::Compact) => 280.,
            (false, GlanceDensity::Comfortable) => 320.,
            (true, GlanceDensity::Compact) => 312.,
            (true, GlanceDensity::Comfortable) => 360.,
        }
    }
}
impl Default for GlancePreferences {
    fn default() -> Self {
        Self {
            enabled: false,
            initialized: false,
            style: GlanceStyle::Transparent,
            density: GlanceDensity::Compact,
            show_clock: true,
            show_weather: true,
            show_calendar: true,
            show_tomorrow: false,
            opacity: 85,
            own_background: false,
            font: None,
            clock_font: None,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct DevboxHistoryPreferences {
    pub cpu: bool,
    pub memory: bool,
    pub disk: bool,
    pub network: bool,
    pub gpu: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevboxSection {
    Cpu,
    Memory,
    Disk,
    Network,
    Gpu,
    Uptime,
    Services,
}
impl DevboxSection {
    pub const ALL: [Self; 7] = [
        Self::Cpu,
        Self::Memory,
        Self::Disk,
        Self::Network,
        Self::Gpu,
        Self::Uptime,
        Self::Services,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Memory => "memory",
            Self::Disk => "disk",
            Self::Network => "network",
            Self::Gpu => "gpu",
            Self::Uptime => "uptime",
            Self::Services => "services",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Memory => "Memory",
            Self::Disk => "Disk",
            Self::Network => "Network",
            Self::Gpu => "GPU",
            Self::Uptime => "Uptime",
            Self::Services => "Services",
        }
    }

    pub fn enabled(self, preferences: &DevboxPreferences) -> bool {
        match self {
            Self::Cpu => preferences.cpu,
            Self::Memory => preferences.memory,
            Self::Disk => preferences.disk,
            Self::Network => preferences.network,
            Self::Gpu => preferences.gpu,
            Self::Uptime => preferences.uptime,
            Self::Services => preferences.services,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct DevboxPreferences {
    pub cpu: bool,
    pub memory: bool,
    pub memory_cache: bool,
    pub swap: bool,
    pub disk: bool,
    pub network: bool,
    pub gpu: bool,
    pub uptime: bool,
    pub services: bool,
    pub history: DevboxHistoryPreferences,
    pub order: Vec<String>,
}
impl DevboxPreferences {
    pub fn ordered_sections(&self) -> Vec<DevboxSection> {
        // Keep hidden sections in place so enabling one restores its position.
        let mut sections = Vec::with_capacity(DevboxSection::ALL.len());
        for key in &self.order {
            if let Some(section) = DevboxSection::ALL
                .into_iter()
                .find(|section| section.key() == key)
                && !sections.contains(&section)
            {
                sections.push(section);
            }
        }
        for section in DevboxSection::ALL {
            if !sections.contains(&section) {
                sections.push(section);
            }
        }
        sections
    }

    pub fn needs_sample(&self) -> bool {
        self.cpu
            || self.memory
            || self.disk
            || self.network
            || self.gpu
            || self.uptime
            || self.services
    }
}
impl Default for DevboxPreferences {
    fn default() -> Self {
        Self {
            cpu: true,
            memory: true,
            memory_cache: true,
            swap: true,
            disk: true,
            network: true,
            gpu: true,
            uptime: true,
            services: false,
            history: DevboxHistoryPreferences::default(),
            order: DevboxSection::ALL
                .into_iter()
                .map(|section| section.key().into())
                .collect(),
        }
    }
}

/// Music: which player to follow and what the card shows. Visibility is
/// `enabled`; the player choice is this computer's own and never part of a
/// profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MusicPreferences {
    pub enabled: bool,
    /// The preferred player's bus name without `org.mpris.MediaPlayer2.`,
    /// for example `spotify`; `None` follows whichever player is playing.
    pub player: Option<String>,
    pub show_artwork: bool,
    pub show_progress: bool,
}
impl Default for MusicPreferences {
    fn default() -> Self {
        Self {
            enabled: false,
            player: None,
            show_artwork: true,
            show_progress: true,
        }
    }
}
impl MusicPreferences {
    fn validate(&self) -> Result<(), String> {
        if let Some(player) = &self.player
            && !valid_player_key(player)
        {
            return Err("Choose a media player by its bus name".into());
        }
        Ok(())
    }
}
/// A bus name element sequence as MPRIS players use after
/// `org.mpris.MediaPlayer2.` (`spotify`, `firefox.instance_1_23`).
pub fn valid_player_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 200
        && key.split('.').all(|part| {
            !part.is_empty()
                && !part.starts_with(|c: char| c.is_ascii_digit())
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotesSize {
    Small,
    #[default]
    Medium,
    Large,
}
impl NotesSize {
    pub const ALL: [Self; 3] = [Self::Small, Self::Medium, Self::Large];
    pub fn key(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Small => "Small",
            Self::Medium => "Medium",
            Self::Large => "Large",
        }
    }
    /// The card's logical size on the desktop.
    pub fn card(self) -> [f32; 2] {
        match self {
            Self::Small => [280., 200.],
            Self::Medium => [320., 280.],
            Self::Large => [360., 400.],
        }
    }
}

/// Notes: the card's visibility and size. The note itself lives in a
/// private file of its own (`tools::notes`), never in this configuration.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct NotesPreferences {
    pub enabled: bool,
    pub size: NotesSize,
}

/// Focus timer durations and behavior. The running session (phase, end
/// time, completed rounds) is runtime state in its own file.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct FocusPreferences {
    pub enabled: bool,
    pub focus_minutes: u16,
    pub break_minutes: u16,
    pub long_break_minutes: u16,
    /// Focus rounds before a long break; 0 means breaks are always short.
    pub long_break_every: u8,
    /// Start the break as soon as a focus round ends.
    pub auto_start_breaks: bool,
    /// A desktop notification when a round ends.
    pub notify: bool,
}
impl Default for FocusPreferences {
    fn default() -> Self {
        Self {
            enabled: false,
            focus_minutes: 25,
            break_minutes: 5,
            long_break_minutes: 15,
            long_break_every: 4,
            auto_start_breaks: false,
            notify: true,
        }
    }
}
impl FocusPreferences {
    pub const FOCUS_CHOICES: [u16; 6] = [15, 25, 30, 45, 50, 90];
    pub const BREAK_CHOICES: [u16; 4] = [3, 5, 10, 15];
    pub const LONG_BREAK_CHOICES: [u16; 4] = [10, 15, 20, 30];
    fn validate(&self) -> Result<(), String> {
        if !(1..=180).contains(&self.focus_minutes) {
            return Err("Focus rounds must be between 1 and 180 minutes".into());
        }
        if !(1..=60).contains(&self.break_minutes) {
            return Err("Breaks must be between 1 and 60 minutes".into());
        }
        if !(1..=90).contains(&self.long_break_minutes) {
            return Err("Long breaks must be between 1 and 90 minutes".into());
        }
        if self.long_break_every > 12 {
            return Err("Long breaks can follow at most 12 focus rounds".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RefreshIntervals {
    pub devbox: u64,
    pub weather: u64,
    pub calendar: u64,
    pub projects: u64,
    pub clock: u64,
}
impl Default for RefreshIntervals {
    fn default() -> Self {
        Self {
            devbox: 3,
            weather: 900,
            calendar: 300,
            projects: 30,
            clock: 60,
        }
    }
}
impl RefreshIntervals {
    pub fn seconds(&self, widget: Widget) -> u64 {
        match widget {
            Widget::Devbox => self.devbox,
            Widget::Weather => self.weather,
            Widget::Calendar => self.calendar,
            Widget::Projects => self.projects,
            Widget::Clock | Widget::Glance | Widget::Music | Widget::Notes | Widget::Focus => {
                self.clock
            }
        }
    }
    pub fn duration(&self, widget: Widget) -> std::time::Duration {
        let (min, max) = Self::bounds(widget);
        std::time::Duration::from_secs(self.seconds(widget).clamp(min, max))
    }
    pub fn bounds(widget: Widget) -> (u64, u64) {
        match widget {
            Widget::Devbox
            | Widget::Clock
            | Widget::Glance
            | Widget::Music
            | Widget::Notes
            | Widget::Focus => (1, 60),
            Widget::Weather => (300, 10800),
            Widget::Calendar => (60, 3600),
            Widget::Projects => (10, 600),
        }
    }
    pub fn choices(widget: Widget) -> &'static [u64] {
        match widget {
            Widget::Devbox => &[1, 2, 3, 5, 10, 30, 60],
            Widget::Weather => &[300, 900, 1800, 3600],
            Widget::Calendar => &[60, 120, 300, 600, 1800],
            Widget::Projects => &[10, 30, 60, 120, 300],
            Widget::Clock | Widget::Glance | Widget::Music | Widget::Notes | Widget::Focus => {
                &[1, 15, 30, 60]
            }
        }
    }
    pub fn label(seconds: u64) -> String {
        if seconds % 3600 == 0 {
            format!("{} h", seconds / 3600)
        } else if seconds % 60 == 0 {
            format!("{} min", seconds / 60)
        } else {
            format!("{seconds} s")
        }
    }
}

/// The largest configuration file the shared writer reads or writes.
pub const MAX_CONFIG_BYTES: usize = 1024 * 1024;

/// An exclusive hold on `config.json.lock`, released on drop.
pub struct ConfigLock {
    _file: std::fs::File,
}

thread_local! {
    static NONBLOCKING_LOCKS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A synchronous input-owning dispatch must never wait for another settings
/// writer. Nested scopes restore the caller's policy, including on unwind;
/// workers and other threads retain the normal blocking transaction contract.
pub(crate) fn with_nonblocking_locks<T>(run: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            NONBLOCKING_LOCKS.with(|slot| slot.set(self.0));
        }
    }
    let _restore = Restore(NONBLOCKING_LOCKS.with(|slot| slot.replace(true)));
    run()
}

pub(crate) fn settings_lock_operation() -> libc::c_int {
    libc::LOCK_EX | NONBLOCKING_LOCKS.with(|slot| if slot.get() { libc::LOCK_NB } else { 0 })
}

/// Takes the shared configuration lock. The Python controller uses this exact
/// sibling filename and flock contract for all configuration edits, and the
/// lock inode stays stable across atomic renames of the file itself.
pub fn lock_config(path: &std::path::Path) -> Result<ConfigLock, String> {
    use std::os::fd::AsRawFd;
    let mut lock_name = path.as_os_str().to_os_string();
    lock_name.push(".lock");
    let lock = crate::desktop::recovery::open_lock_file(std::path::Path::new(&lock_name))
        .map_err(|e| e.to_string())?;
    loop {
        // SAFETY: lock owns a valid descriptor and remains alive through rename.
        if unsafe { libc::flock(lock.as_raw_fd(), settings_lock_operation()) } == 0 {
            return Ok(ConfigLock { _file: lock });
        }
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::WouldBlock {
            return Err("Settings are busy in another process. Try again when it finishes.".into());
        }
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(error.to_string());
        }
    }
}

/// The configuration file's text, or `None` when it does not exist.
pub fn read_config_text(path: &std::path::Path) -> Result<Option<String>, String> {
    crate::design::storage::projected_text(path)
}

/// Physical legacy/preferences bytes. Only the transaction participant and
/// migration use this; editors consume `read_config_text`.
pub(crate) fn read_raw_config_text(path: &std::path::Path) -> Result<Option<String>, String> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES as u64 {
        return Err("Configuration is not a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_CONFIG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err("Configuration file is too large".into());
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "Configuration is not valid UTF-8".into())
}

/// Parses configuration text as a JSON object; a missing file is `{}`.
pub fn parse_config_text(text: Option<&str>) -> Result<serde_json::Value, String> {
    let value = match text {
        Some(text) => serde_json::from_str::<serde_json::Value>(text).map_err(|e| e.to_string())?,
        None => serde_json::json!({}),
    };
    if !value.is_object() {
        return Err("Configuration must be a JSON object".into());
    }
    Ok(value)
}

/// The same validation every configuration write passes.
pub fn check_config_value(value: &serde_json::Value) -> Result<Config, String> {
    let mut config: Config = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    config.validate()?;
    config.normalize_policy();
    Ok(config)
}

/// The bytes the shared writer stores for `value`.
pub fn config_text(value: &serde_json::Value) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|e| e.to_string())
}

/// Atomically replaces the configuration file (`None` removes it). The
/// caller holds the configuration lock.
pub fn write_config_text(path: &std::path::Path, text: Option<&str>) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let Some(text) = text else {
        return match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        };
    };
    if text.len() > MAX_CONFIG_BYTES {
        return Err("Configuration file is too large".into());
    }
    static NEXT_TEMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let (temporary, mut file) = loop {
        let mut name = path.as_os_str().to_os_string();
        name.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let temporary = std::path::PathBuf::from(name);
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    };
    let result = (|| {
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    Ok(())
}

// Patch the latest file so toggles preserve settings changed by the controller.
pub(crate) fn patch_config(
    path: &std::path::Path,
    change: impl FnOnce(&mut serde_json::Value) -> Result<(), String>,
) -> Result<serde_json::Value, String> {
    if let Some(label) = crate::desktop::actions::current_label() {
        return crate::desktop::actions::preferences(path, &label, change);
    }
    // Once migrated, every edit (including local provider preferences) joins
    // the same journal; there is never a writable legacy widget mirror.
    if crate::design::storage::read(path)?.is_some() {
        return crate::desktop::actions::preferences(path, "Widget settings", change);
    }
    let lock = lock_config(path)?;
    if crate::design::storage::managed(path) {
        drop(lock);
        return crate::desktop::actions::preferences(path, "Widget settings", change);
    }
    let _lock = lock;
    let mut value = parse_config_text(read_config_text(path)?.as_deref())?;
    let before = value.clone();
    change(&mut value)?;
    check_config_value(&value)?;
    if Config::path().as_deref() == Some(path)
        && crate::desktop::recovery::app_pointers()
            .any(|pointer| before.pointer(pointer) != value.pointer(pointer))
    {
        let store = crate::desktop::DesktopStore::from_env().map_err(|e| e.to_string())?;
        // Alternate app configs without a fixture remain local-only previews.
        if store.writable() {
            store.capture_config_locked().map_err(|e| e.to_string())?;
        }
    }
    write_config_text(path, Some(&config_text(&value)?))?;
    Ok(value)
}
/// Which widget choices a saved configuration holds. Writes save only the
/// keys that changed, so a present key is a choice someone made.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SavedChoices {
    /// The list of widgets on the desktop.
    pub widgets: bool,
    /// Anything about At a glance.
    pub glance: bool,
    /// The system monitor's rows.
    pub devbox: bool,
}

impl SavedChoices {
    pub fn of(value: &serde_json::Value) -> Self {
        let set = |key: &str| {
            value
                .get(key)
                .is_some_and(|member| member.as_object().is_none_or(|object| !object.is_empty()))
        };
        Self {
            widgets: set("enabled_widgets"),
            glance: set("glance"),
            devbox: set("devbox"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub accent: Color,
    pub background: Color,
    pub foreground: Color,
    pub muted: Color,
}
fn rgb(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}
impl Palette {
    pub const CHOICES: [(&'static str, &'static str); 6] = [
        ("lavender", "Lavender"),
        ("amber", "Amber"),
        ("teal", "Teal"),
        ("ocean", "Ocean blue"),
        ("rose", "Rose"),
        ("graphite", "Graphite"),
    ];

    pub fn named(name: &str) -> Self {
        let (accent, bg, fg, muted) = match name {
            "amber" => (0xe9b887, 0x302822, 0xf5e9de, 0xbaaa9b),
            "teal" => (0x6ee7ce, 0x10272d, 0xdff7f1, 0x8fbbb5),
            "ocean" => (0x88bdff, 0x1b2b40, 0xe8f2ff, 0xa0b5cf),
            "rose" => (0xf2acc0, 0x33232d, 0xfbecf2, 0xc5a5b3),
            "graphite" => (0xc5cdd8, 0x24282e, 0xf0f2f5, 0xa9b2bf),
            _ => (0xb8a2ed, 0x222230, 0xf0edf7, 0xa7a1b8),
        };
        Self {
            accent: rgb(accent),
            background: Color { a: 0.92, ..rgb(bg) },
            foreground: rgb(fg),
            muted: rgb(muted),
        }
    }
}

#[cfg(test)]
mod palette_tests {
    use super::*;

    #[test]
    fn widget_sizes_survive_roundtrip_and_select_individual_surfaces() {
        let config: Config = serde_json::from_value(serde_json::json!({
            "widget_sizes": {"weather": [480., 320.], "music": [400., 280.]}
        }))
        .unwrap();
        let encoded = serde_json::to_value(&config).unwrap();
        assert_eq!(
            encoded["widget_sizes"]["weather"],
            serde_json::json!([480., 320.])
        );
        assert!(config.has_custom_positions());
        assert!(Config::default().validate().is_ok());
    }

    #[test]
    fn every_palette_has_a_distinct_accent_and_readable_card_text() {
        fn luminance(color: Color) -> f32 {
            let linear = |value: f32| {
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
        }
        let mut accents = Vec::new();
        for (name, _) in Palette::CHOICES {
            let palette = Palette::named(name);
            assert!(
                !accents.contains(&palette.accent),
                "{name} must have its own palette"
            );
            accents.push(palette.accent);
            for text in [palette.foreground, palette.muted, palette.accent] {
                let contrast = (luminance(text) + 0.05) / (luminance(palette.background) + 0.05);
                assert!(contrast >= 4.5, "{name} card text contrast is {contrast}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_draft_merges_per_widget_and_rejects_invalid_dimensions_atomically() {
        use std::collections::BTreeMap;
        let path =
            std::env::temp_dir().join(format!("cards-size-merge-{}.json", std::process::id()));
        std::fs::write(&path, r#"{"widget_sizes":{"weather":[400,300],"notes":[500,500]},"future":42,"placement":{"snap":false}}"#).unwrap();
        let baseline = BTreeMap::from([
            ("weather".into(), [360., 180.]),
            ("notes".into(), [360., 300.]),
        ]);
        let sizes = BTreeMap::from([
            ("weather".into(), [480., 320.]),
            ("notes".into(), [360., 300.]),
        ]);
        let grid = PlacementPreferences::default();
        let saved = Config::save_layout_draft_at(
            &path,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &sizes,
            &baseline,
            &Default::default(),
            grid,
            grid,
        )
        .unwrap();
        assert_eq!(saved.widget_sizes["weather"], [480., 320.]);
        assert_eq!(saved.widget_sizes["notes"], [500., 500.]);
        assert!(!saved.placement.snap);
        let before = std::fs::read(&path).unwrap();
        for size in [[0., 100.], [-10., 100.], [100., 32769.], [100., f32::NAN]] {
            let invalid = BTreeMap::from([("weather".into(), size)]);
            assert!(
                Config::save_layout_draft_at(
                    &path,
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    &invalid,
                    &sizes,
                    &Default::default(),
                    grid,
                    grid
                )
                .is_err()
            );
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
        let automatic = BTreeMap::from([("notes".into(), [360., 300.])]);
        let saved = Config::save_layout_draft_at(
            &path,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &automatic,
            &sizes,
            &Default::default(),
            grid,
            grid,
        )
        .unwrap();
        assert!(!saved.widget_sizes.contains_key("weather"));
        assert_eq!(saved.widget_sizes["notes"], [500., 500.]);
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(raw["future"], 42);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn devbox_order_ignores_unknown_and_duplicate_keys_then_appends_missing_sections() {
        let preferences: DevboxPreferences = serde_json::from_value(serde_json::json!({
            "services":false,
            "order":["gpu", "future_section", "services", "gpu", "cpu"]
        }))
        .unwrap();
        assert_eq!(
            preferences.ordered_sections(),
            vec![
                DevboxSection::Gpu,
                DevboxSection::Services,
                DevboxSection::Cpu,
                DevboxSection::Memory,
                DevboxSection::Disk,
                DevboxSection::Network,
                DevboxSection::Uptime,
            ]
        );
        let visible: Vec<_> = preferences
            .ordered_sections()
            .into_iter()
            .filter(|section| section.enabled(&preferences))
            .map(DevboxSection::key)
            .collect();
        assert_eq!(
            visible,
            ["gpu", "cpu", "memory", "disk", "network", "uptime"]
        );
    }

    #[test]
    fn devbox_empty_order_falls_back_to_all_sections() {
        let preferences: DevboxPreferences =
            serde_json::from_value(serde_json::json!({"order":[]})).unwrap();
        let keys: Vec<_> = preferences
            .ordered_sections()
            .into_iter()
            .map(DevboxSection::key)
            .collect();
        assert_eq!(
            keys,
            [
                "cpu", "memory", "disk", "network", "gpu", "uptime", "services"
            ]
        );
    }

    #[test]
    fn devbox_memory_detail_flags_do_not_require_sampling_without_memory() {
        let mut preferences: DevboxPreferences = serde_json::from_value(serde_json::json!({
            "cpu":false, "memory":false, "disk":false, "network":false,
            "gpu":false, "uptime":false, "services":false,
            "memory_cache":true, "swap":true
        }))
        .unwrap();
        assert!(!preferences.needs_sample());
        preferences.memory = true;
        preferences.memory_cache = false;
        preferences.swap = false;
        assert!(preferences.needs_sample());
    }

    #[test]
    fn partial_devbox_settings_enable_hardware_but_leave_personal_services_off() {
        for source in [
            serde_json::json!({"theme":"teal"}),
            serde_json::json!({"devbox":{"gpu":false}}),
        ] {
            let config: Config = serde_json::from_value(source.clone()).unwrap();
            let saved = serde_json::to_value(config).unwrap();
            for key in [
                "cpu",
                "memory",
                "memory_cache",
                "swap",
                "disk",
                "network",
                "uptime",
                "services",
            ] {
                assert_eq!(saved["devbox"][key], key != "services", "{key}");
            }
            assert_eq!(
                saved["devbox"]["gpu"],
                source["devbox"].get("gpu").cloned().unwrap_or(true.into())
            );
            assert_eq!(
                saved["devbox"]["order"],
                serde_json::json!([
                    "cpu", "memory", "disk", "network", "gpu", "uptime", "services"
                ])
            );
        }
    }

    #[test]
    fn devbox_detail_changes_preserve_order_hidden_sections_and_future_settings() {
        let file = TestFile::new();
        std::fs::write(
            &file.0,
            serde_json::json!({
                "theme":"amber",
                "devbox": {
                    "cpu":false, "services":false, "future_sensor":42,
                    "order":["gpu", "services", "memory", "cpu"]
                },
                "future_setting":"keep"
            })
            .to_string(),
        )
        .unwrap();
        let updated = Config::save_section_at(
            &file.0,
            "devbox",
            serde_json::json!({"memory_cache":false, "swap":false}),
        )
        .unwrap();
        let serialized = serde_json::to_value(updated).unwrap();
        assert_eq!(serialized["devbox"]["memory_cache"], false);
        assert_eq!(serialized["devbox"]["swap"], false);
        assert_eq!(serialized["devbox"]["cpu"], false);
        assert_eq!(serialized["devbox"]["services"], false);
        assert_eq!(
            serialized["devbox"]["order"],
            serde_json::json!(["gpu", "services", "memory", "cpu"])
        );
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file.0).unwrap()).unwrap();
        assert_eq!(raw["devbox"]["future_sensor"], 42);
        assert_eq!(raw["future_setting"], "keep");
    }

    #[test]
    fn devbox_changes_merge_and_reject_invalid_values_before_writing() {
        let file = TestFile::new();
        std::fs::write(
            &file.0,
            serde_json::json!({
                "theme":"amber", "devbox":{"cpu":false, "future_sensor":42},
                "future_setting":"keep"
            })
            .to_string(),
        )
        .unwrap();
        let updated = Config::save_section_at(
            &file.0,
            "devbox",
            serde_json::json!({"network":false, "gpu":true}),
        )
        .unwrap();
        let serialized = serde_json::to_value(updated).unwrap();
        assert_eq!(serialized["devbox"]["cpu"], false);
        assert_eq!(serialized["devbox"]["network"], false);
        assert_eq!(serialized["devbox"]["gpu"], true);
        let saved = std::fs::read(&file.0).unwrap();
        let raw: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(raw["devbox"]["future_sensor"], 42);
        assert_eq!(raw["future_setting"], "keep");
        assert!(
            Config::save_section_at(&file.0, "devbox", serde_json::json!({"memory":"yes"}),)
                .is_err()
        );
        assert_eq!(std::fs::read(&file.0).unwrap(), saved);
    }

    #[test]
    fn saved_positions_reject_invalid_coordinates_without_losing_existing_preferences() {
        let file = TestFile::new();
        std::fs::write(&file.0, r#"{"theme":"teal","future_setting":42}"#).unwrap();
        let saved = std::fs::read(&file.0).unwrap();
        for point in [
            serde_json::json!([-0.1, 0.5]),
            serde_json::json!([0.5, 1.1]),
        ] {
            assert!(
                Config::save_changes_at(
                    &file.0,
                    serde_json::json!({"widget_positions":{"weather":point}})
                )
                .is_err()
            );
            assert_eq!(std::fs::read(&file.0).unwrap(), saved);
        }
        let config = Config::save_changes_at(
            &file.0,
            serde_json::json!({"widget_positions":{"weather":[0.5,0.4]}}),
        )
        .unwrap();
        assert_eq!(config.widget_positions["weather"], [0.5, 0.4]);
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file.0).unwrap()).unwrap();
        assert_eq!(raw["future_setting"], 42);
    }

    #[test]
    fn grid_layout_save_is_atomic_and_preserves_unrelated_nested_preferences() {
        let file = TestFile::new();
        std::fs::write(
            &file.0,
            r#"{"theme":"teal","placement":{"future_option":42},"devbox":{"gpu":false}}"#,
        )
        .unwrap();
        let positions = [("weather".into(), [0.5, 0.4])].into_iter().collect();
        let saved = Config::save_layout_draft_at(
            &file.0,
            &positions,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            PlacementPreferences {
                snap: true,
                columns: 24,
            },
            PlacementPreferences::default(),
        )
        .unwrap();
        assert_eq!(saved.widget_positions, positions);
        assert_eq!(saved.placement.columns, 24);
        assert!(!saved.devbox.gpu);
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file.0).unwrap()).unwrap();
        assert_eq!(raw["placement"]["future_option"], 42);
        let before = std::fs::read(&file.0).unwrap();
        assert!(
            Config::save_layout_draft_at(
                &file.0,
                &positions,
                &Default::default(),
                &Default::default(),
                &Default::default(),
                &Default::default(),
                PlacementPreferences {
                    snap: true,
                    columns: 0
                },
                PlacementPreferences::default(),
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&file.0).unwrap(), before);
        let defaults: Config = serde_json::from_str("{}").unwrap();
        assert!(defaults.placement.snap);
        assert_eq!(defaults.placement.columns, 24);
    }

    #[test]
    fn layout_draft_grid_changes_preserve_concurrent_positions_and_grid_preferences() {
        let file = TestFile::new();
        std::fs::write(
            &file.0,
            serde_json::json!({
                "theme":"ocean",
                "widget_positions":{"weather":[0.7,0.8], "projects":[0.8,0.6]},
                "placement":{"snap":true,"columns":24,"future_option":17}
            })
            .to_string(),
        )
        .unwrap();
        let baseline = [
            ("weather".into(), [0.1, 0.2]),
            ("calendar".into(), [0.3, 0.4]),
        ]
        .into_iter()
        .collect();
        let saved = Config::save_layout_draft_at(
            &file.0,
            &baseline,
            &baseline,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            PlacementPreferences {
                snap: false,
                columns: 24,
            },
            PlacementPreferences::default(),
        )
        .unwrap();
        assert_eq!(saved.widget_positions["weather"], [0.7, 0.8]);
        assert_eq!(saved.widget_positions["projects"], [0.8, 0.6]);
        assert!(!saved.widget_positions.contains_key("calendar"));
        assert!(!saved.placement.snap);
        assert_eq!(saved.placement.columns, 24);
        assert_eq!(saved.theme, "ocean");
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file.0).unwrap()).unwrap();
        assert_eq!(raw["placement"]["future_option"], 17);
    }

    #[test]
    fn layout_draft_merges_local_moves_and_reset_removals_per_widget() {
        let file = TestFile::new();
        std::fs::write(
            &file.0,
            serde_json::json!({
                "widget_positions":{
                    "weather":[0.9,0.9], "calendar":[0.4,0.5],
                    "clock":[0.5,0.5], "projects":[0.8,0.8]
                },
                "placement":{"snap":false,"columns":12},
                "future_option":42
            })
            .to_string(),
        )
        .unwrap();
        let baseline = [
            ("weather".into(), [0.1, 0.2]),
            ("calendar".into(), [0.3, 0.4]),
            ("clock".into(), [0.2, 0.3]),
        ]
        .into_iter()
        .collect();
        let draft = [
            ("weather".into(), [0.6, 0.6]),
            ("clock".into(), [0.2, 0.3]),
            ("devbox".into(), [0.7, 0.4]),
        ]
        .into_iter()
        .collect();
        let saved = Config::save_layout_draft_at(
            &file.0,
            &draft,
            &baseline,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            PlacementPreferences::default(),
            PlacementPreferences::default(),
        )
        .unwrap();
        assert_eq!(saved.widget_positions["weather"], [0.6, 0.6]);
        assert_eq!(saved.widget_positions["clock"], [0.5, 0.5]);
        assert_eq!(saved.widget_positions["projects"], [0.8, 0.8]);
        assert_eq!(saved.widget_positions["devbox"], [0.7, 0.4]);
        assert!(!saved.widget_positions.contains_key("calendar"));
        assert_eq!(
            saved.placement,
            PlacementPreferences {
                snap: false,
                columns: 12
            }
        );
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file.0).unwrap()).unwrap();
        assert_eq!(raw["future_option"], 42);
    }

    #[test]
    fn layout_draft_rejects_invalid_updates_without_writing() {
        let file = TestFile::new();
        std::fs::write(
            &file.0,
            r#"{"widget_positions":{"weather":[0.4,0.5]},"future_option":42}"#,
        )
        .unwrap();
        let before = std::fs::read(&file.0).unwrap();
        let baseline = [("weather".into(), [0.1, 0.2])].into_iter().collect();
        for point in [[-0.1, 0.5], [0.5, 1.1], [f32::NAN, 0.5]] {
            let draft = [("weather".into(), point)].into_iter().collect();
            assert!(
                Config::save_layout_draft_at(
                    &file.0,
                    &draft,
                    &baseline,
                    &Default::default(),
                    &Default::default(),
                    &Default::default(),
                    PlacementPreferences::default(),
                    PlacementPreferences::default(),
                )
                .is_err()
            );
            assert_eq!(std::fs::read(&file.0).unwrap(), before);
        }
        assert!(
            Config::save_layout_draft_at(
                &file.0,
                &baseline,
                &baseline,
                &Default::default(),
                &Default::default(),
                &Default::default(),
                PlacementPreferences {
                    snap: true,
                    columns: 0
                },
                PlacementPreferences::default(),
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&file.0).unwrap(), before);
    }

    struct TestFile(std::path::PathBuf);
    impl TestFile {
        fn new() -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            Self(std::env::temp_dir().join(format!(
                "desktop-cards-{}-{unique}.json",
                std::process::id()
            )))
        }
    }
    impl Drop for TestFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
            let mut lock = self.0.as_os_str().to_os_string();
            lock.push(".lock");
            let _ = std::fs::remove_file(lock);
        }
    }

    #[test]
    fn glance_first_placement_never_overwrites_another_editors_position() {
        let file = TestFile::new();
        std::fs::write(
            &file.0,
            r#"{"widget_positions":{"clock":[0.1,0.2]},"future_setting":42}"#,
        )
        .unwrap();
        Config::seed_glance_position_at(&file.0, [0.15, 0.25]).unwrap();
        let config = Config::seed_glance_position_at(&file.0, [0.8, 0.9]).unwrap();
        assert_eq!(config.widget_positions["glance"], [0.15, 0.25]);
        assert_eq!(config.widget_positions["clock"], [0.1, 0.2]);
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file.0).unwrap()).unwrap();
        assert_eq!(raw["future_setting"], 42);
    }

    #[test]
    fn glance_section_edits_before_first_enable_are_kept() {
        let file = TestFile::new();
        std::fs::write(&file.0, r#"{"enabled_widgets":["clock","devbox"]}"#).unwrap();
        let mut config =
            Config::save_glance_at(&file.0, serde_json::json!({"show_calendar":true})).unwrap();
        config
            .save_widget_at(&file.0, Widget::Glance, true)
            .unwrap();
        assert!(config.glance.show_clock && config.glance.show_calendar);
        assert!(!config.glance.show_weather);
    }

    #[test]
    fn glance_restores_original_widgets_and_uses_independent_section_choices() {
        let file = TestFile::new();
        let original = serde_json::json!({
            "enabled_widgets": ["clock", "calendar", "devbox"],
            "widget_positions": {"clock":[0.1,0.2], "calendar":[0.2,0.5]},
            "glance": {"future_style": "kept"},
            "future_setting": 42
        });
        std::fs::write(&file.0, original.to_string()).unwrap();
        // Start with an older in-memory config to exercise the latest-file merge.
        let mut config = Config::default();
        config
            .save_widget_at(&file.0, Widget::Glance, true)
            .unwrap();
        assert!(config.glance.initialized && config.glance.enabled);
        assert!(config.glance.show_clock && config.glance.show_calendar);
        assert!(!config.glance.show_weather);
        assert!(!config.widget_visible(Widget::Clock));
        assert!(config.widget_enabled(Widget::Clock));
        assert!(config.data_visible(Widget::Clock));
        config
            .save_widget_at(&file.0, Widget::Weather, true)
            .unwrap();
        config
            .save_widget_at(&file.0, Widget::Calendar, false)
            .unwrap();
        assert!(config.data_visible(Widget::Weather));
        assert!(!config.data_visible(Widget::Calendar));
        config
            .save_widget_at(&file.0, Widget::Glance, false)
            .unwrap();
        assert!(config.widget_visible(Widget::Clock));
        assert!(config.widget_visible(Widget::Calendar));
        assert!(!config.widget_visible(Widget::Weather));
        config
            .save_widget_at(&file.0, Widget::Glance, true)
            .unwrap();
        assert!(config.glance.show_weather);
        assert!(!config.glance.show_calendar);
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file.0).unwrap()).unwrap();
        assert_eq!(raw["enabled_widgets"], original["enabled_widgets"]);
        assert_eq!(raw["widget_positions"], original["widget_positions"]);
        assert_eq!(raw["glance"]["future_style"], "kept");
        assert_eq!(raw["future_setting"], 42);
    }

    #[test]
    fn new_features_are_opt_in_for_existing_settings() {
        let config: Config = serde_json::from_value(serde_json::json!({"theme":"ocean"})).unwrap();
        assert!(!config.glance.enabled);
        assert!(!config.glance.initialized);
        assert_eq!(config.glance.style, GlanceStyle::Transparent);
        assert_eq!(config.glance.density, GlanceDensity::Compact);
        assert_eq!(config.devbox.history, DevboxHistoryPreferences::default());
        assert!(config.widget_visible(Widget::Clock));
        assert!(!config.enabled_widgets.iter().any(|key| key == "glance"));
    }

    #[test]
    fn individual_history_toggles_preserve_other_graphs_and_unknown_settings() {
        let file = TestFile::new();
        std::fs::write(&file.0, serde_json::json!({
            "devbox": {"gpu": false, "history": {"memory":true, "future_trace":17}, "order":["memory","cpu"]},
            "future_setting":42
        }).to_string()).unwrap();
        Config::save_devbox_history_at(&file.0, "cpu", true).unwrap();
        let config = Config::save_devbox_history_at(&file.0, "network", true).unwrap();
        assert!(
            config.devbox.history.cpu
                && config.devbox.history.memory
                && config.devbox.history.network
        );
        assert!(!config.devbox.gpu);
        assert_eq!(config.devbox.order, ["memory", "cpu"]);
        let bytes = std::fs::read(&file.0).unwrap();
        let raw: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(raw["devbox"]["history"]["future_trace"], 17);
        assert_eq!(raw["future_setting"], 42);
        assert!(Config::save_devbox_history_at(&file.0, "unknown", true).is_err());
        assert_eq!(std::fs::read(&file.0).unwrap(), bytes);
    }

    #[test]
    fn invalid_glance_preferences_leave_existing_settings_untouched() {
        let file = TestFile::new();
        let original =
            serde_json::json!({"glance":{"density":"comfortable", "show_tomorrow":true}})
                .to_string();
        std::fs::write(&file.0, &original).unwrap();
        for changes in [
            serde_json::json!({"opacity":101}),
            serde_json::json!({"style":"unknown"}),
            serde_json::json!({"show_calendar":"yes"}),
        ] {
            assert!(Config::save_section_at(&file.0, "glance", changes).is_err());
            assert_eq!(std::fs::read_to_string(&file.0).unwrap(), original);
        }
    }

    #[test]
    fn legacy_settings_keep_their_appearance_and_clock_defaults() {
        let config: Config = serde_json::from_value(serde_json::json!({
            "theme":"teal", "enabled_widgets":["calendar", "clock"]
        }))
        .unwrap();
        assert_eq!(config.appearance, Appearance::default());
        assert!(!config.appearance.match_cosmic_colors);
        assert_eq!(config.clock, ClockPreferences::default());
        assert_eq!(
            config.palette().background.a,
            Palette::named("teal").background.a
        );
        let translucent = Config {
            appearance: Appearance {
                opacity: 70,
                ..Appearance::default()
            },
            ..config
        };
        assert_eq!(translucent.palette().background.a, 0.70);
        assert_eq!(Palette::named("teal").background.a, 0.92);
    }

    #[test]
    fn widget_palette_selection_keeps_legacy_links_inert_and_unrelated_members_intact() {
        let file = TestFile::new();
        let original = serde_json::json!({
            "theme": "amber",
            "appearance": {
                "opacity": 84, "color_source": "desktop", "corner_radius": 12,
                "match_wallpaper": true, "match_cosmic_colors": true, "future_tint": 0.4
            },
            "widget_positions": {"calendar": [0.2,0.3]}, "future_setting": 42
        });
        std::fs::write(&file.0, original.to_string()).unwrap();
        // Widget selection calls only the atomic app-config writer. The
        // legacy links remain available to an explicit CLI command.
        let config = Config::save_widget_theme_at(&file.0, "rose").unwrap();
        assert_eq!(config.appearance.color_source, ColorSource::Custom);
        assert_eq!(config.palette().accent, Palette::named("rose").accent);
        assert_eq!(config.appearance.opacity, 84);
        assert!(config.appearance.match_wallpaper && config.appearance.match_cosmic_colors);
        let saved = std::fs::read(&file.0).unwrap();
        let raw: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(raw["appearance"]["future_tint"], 0.4);
        assert_eq!(raw["widget_positions"], original["widget_positions"]);
        assert_eq!(raw["future_setting"], 42);
        assert!(Config::save_widget_theme_at(&file.0, "unknown").is_err());
        assert_eq!(std::fs::read(&file.0).unwrap(), saved);
        assert!(
            Config::save_section_at(
                &file.0,
                "appearance",
                serde_json::json!({"color_source":"future-mode"})
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&file.0).unwrap(), saved);
    }

    #[test]
    fn reading_an_old_linked_configuration_preserves_its_look_and_file_bytes() {
        let file = TestFile::new();
        let original = r#"{"theme":"ocean","appearance":{"opacity":79,"match_wallpaper":true,"match_cosmic_colors":true}}"#;
        std::fs::write(&file.0, original).unwrap();
        let config = Config::read_at(&file.0).unwrap();
        assert_eq!(config.appearance.color_source, ColorSource::Custom);
        assert_eq!(config.palette().accent, Palette::named("ocean").accent);
        assert_eq!(config.palette().background.a, 0.79);
        assert!(config.desktop_palette.is_none());
        assert_eq!(std::fs::read_to_string(&file.0).unwrap(), original);
    }

    #[test]
    fn nested_appearance_and_clock_edits_preserve_newer_settings_and_validate_before_write() {
        let file = TestFile::new();
        std::fs::write(
            &file.0,
            serde_json::json!({
                "theme":"amber", "project_roots":["/projects/Voice-Agent"],
                "appearance":{"opacity":85, "match_wallpaper":false, "match_cosmic_colors":true, "future_tint":0.4},
                "clock":{"format_24h":false, "future_style":"minimal"},
                "future_setting":42
            })
            .to_string(),
        )
        .unwrap();
        let config = Config::save_section_at(
            &file.0,
            "appearance",
            serde_json::json!({"corner_radius":16, "border_opacity":0}),
        )
        .unwrap();
        assert_eq!(config.appearance.opacity, 85);
        assert_eq!(config.appearance.corner_radius, 16);
        assert!(!config.appearance.match_wallpaper);
        assert!(config.appearance.match_cosmic_colors);
        let config =
            Config::save_section_at(&file.0, "clock", serde_json::json!({"show_date":false}))
                .unwrap();
        assert!(!config.clock.format_24h && !config.clock.show_date);
        assert_eq!(config.project_roots, ["/projects/Voice-Agent"]);
        let saved = std::fs::read(&file.0).unwrap();
        let raw: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(raw["appearance"]["future_tint"], 0.4);
        assert_eq!(raw["clock"]["future_style"], "minimal");
        assert_eq!(raw["future_setting"], 42);
        for (section, invalid) in [
            ("appearance", serde_json::json!({"opacity":69})),
            ("appearance", serde_json::json!({"opacity":101})),
            ("appearance", serde_json::json!({"corner_radius":33})),
            ("appearance", serde_json::json!({"border_opacity":51})),
            ("appearance", serde_json::json!({"match_wallpaper":"yes"})),
            (
                "appearance",
                serde_json::json!({"match_cosmic_colors":"yes"}),
            ),
            ("clock", serde_json::json!({"format_24h":"yes"})),
        ] {
            assert!(Config::save_section_at(&file.0, section, invalid).is_err());
            assert_eq!(std::fs::read(&file.0).unwrap(), saved);
        }
    }

    #[test]
    fn cosmic_color_matching_is_opt_in_and_preserves_other_appearance_preferences() {
        let file = TestFile::new();
        std::fs::write(
            &file.0,
            r#"{"theme":"amber","appearance":{"opacity":85,"match_wallpaper":false,"future_tint":0.4},"future_setting":42}"#,
        )
        .unwrap();
        assert!(
            !Config::read_at(&file.0)
                .unwrap()
                .appearance
                .match_cosmic_colors
        );
        for enabled in [true, false] {
            let config = Config::save_section_at(
                &file.0,
                "appearance",
                serde_json::json!({"match_cosmic_colors":enabled}),
            )
            .unwrap();
            assert_eq!(config.appearance.match_cosmic_colors, enabled);
            assert_eq!(config.appearance.opacity, 85);
            assert!(!config.appearance.match_wallpaper);
            assert_eq!(config.theme, "amber");
            let raw: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&file.0).unwrap()).unwrap();
            assert_eq!(raw["appearance"]["future_tint"], 0.4);
            assert_eq!(raw["future_setting"], 42);
        }
    }

    #[test]
    fn concurrent_config_writers_preserve_every_update() {
        let file = TestFile::new();
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for i in 0..8 {
                let path = &file.0;
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    patch_config(path, |value| {
                        let count = value["counter"].as_u64().unwrap_or_default();
                        std::thread::sleep(std::time::Duration::from_millis(2));
                        value["counter"] = (count + 1).into();
                        value[format!("writer_{i}")] = true.into();
                        Ok(())
                    })
                    .unwrap();
                });
            }
        });
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file.0).unwrap()).unwrap();
        assert_eq!(raw["counter"], 8);
        for i in 0..8 {
            assert_eq!(raw[format!("writer_{i}")], true);
        }
        Config::read_at(&file.0).unwrap();
    }

    #[test]
    fn refresh_changes_preserve_other_intervals_and_reject_unbounded_polling() {
        let file = TestFile::new();
        std::fs::write(
            &file.0,
            serde_json::json!({
                "theme":"teal", "enabled_widgets":["clock"],
                "refresh_intervals":{"weather":1800, "future_widget":42}
            })
            .to_string(),
        )
        .unwrap();
        let config = Config::save_refresh_at(&file.0, Widget::Devbox, 10).unwrap();
        assert_eq!(config.refresh_intervals.devbox, 10);
        assert_eq!(config.refresh_intervals.weather, 1800);
        assert_eq!(config.refresh_intervals.calendar, 300);
        assert_eq!(config.enabled_widgets, ["clock"]);
        let saved = std::fs::read(&file.0).unwrap();
        let raw: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(raw["refresh_intervals"]["future_widget"], 42);
        for (widget, seconds) in [
            (Widget::Devbox, 0),
            (Widget::Weather, 1),
            (Widget::Clock, 600),
        ] {
            assert!(Config::save_refresh_at(&file.0, widget, seconds).is_err());
            assert_eq!(std::fs::read(&file.0).unwrap(), saved);
        }
        std::fs::write(&file.0, "{ incomplete").unwrap();
        assert!(Config::read_at(&file.0).is_err());
    }
    #[test]
    fn graphical_settings_patch_preserves_other_fields_and_rejects_bad_values() {
        let file = TestFile::new();
        std::fs::write(
            &file.0,
            serde_json::to_vec(&serde_json::json!({
                "theme": "amber", "enabled_widgets": ["weather", "clock"],
                "project_roots": ["/custom/project"], "future_setting": {"keep": true}
            }))
            .unwrap(),
        )
        .unwrap();
        let initial: Config = serde_json::from_slice(&std::fs::read(&file.0).unwrap()).unwrap();
        assert!(initial.calendar_show_today);
        assert!(!initial.calendar_show_tomorrow);
        let saved = Config::save_changes_at(
            &file.0,
            serde_json::json!({
                "calendar_show_tomorrow": true, "meeting_notice_minutes": 15
            }),
        )
        .unwrap();
        assert!(saved.calendar_show_today && saved.calendar_show_tomorrow);
        assert_eq!(saved.theme, "amber");
        assert_eq!(saved.enabled_widgets, ["weather", "clock"]);
        let bytes = std::fs::read(&file.0).unwrap();
        let raw: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(raw["future_setting"]["keep"], true);
        for invalid in [
            serde_json::json!({"calendar_show_today": "yes"}),
            serde_json::json!({"meeting_notice_minutes": 0}),
            serde_json::json!({"meeting_notice_minutes": 61}),
        ] {
            assert!(Config::save_changes_at(&file.0, invalid).is_err());
            assert_eq!(std::fs::read(&file.0).unwrap(), bytes);
        }
    }
    #[test]
    fn widget_selection_survives_reload_and_preserves_newer_settings() {
        let file = TestFile::new();
        let mut config = Config::default();
        // Simulate settings changed by the controller after this Config was loaded.
        let latest = serde_json::json!({
            "theme": "teal", "enabled_widgets": ["calendar", "clock"],
            "project_roots": ["/projects/Voice-Agent"], "future_setting": 42,
        });
        std::fs::write(&file.0, latest.to_string()).unwrap();
        config
            .save_widget_at(&file.0, Widget::Weather, true)
            .unwrap();
        config
            .save_widget_at(&file.0, Widget::Calendar, false)
            .unwrap();
        let bytes = std::fs::read(&file.0).unwrap();
        let reloaded: Config = serde_json::from_slice(&bytes).unwrap();
        assert!(reloaded.widget_visible(Widget::Weather));
        assert!(reloaded.widget_visible(Widget::Clock));
        assert!(!reloaded.widget_visible(Widget::Calendar));
        assert!(!reloaded.widget_visible(Widget::Projects));
        assert_eq!(reloaded.theme, "teal");
        assert_eq!(reloaded.project_roots, ["/projects/Voice-Agent"]);
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["future_setting"], 42);
    }

    #[test]
    fn invalid_settings_are_not_overwritten_by_a_toggle() {
        let file = TestFile::new();
        let mut config = Config::default();
        for invalid in ["{unfinished", "[]", r#"{"enabled_widgets":"invalid"}"#] {
            std::fs::write(&file.0, invalid).unwrap();
            assert!(
                config
                    .save_widget_at(&file.0, Widget::Clock, false)
                    .is_err()
            );
            assert_eq!(std::fs::read_to_string(&file.0).unwrap(), invalid);
            assert!(config.widget_visible(Widget::Clock));
        }
    }

    #[test]
    fn reordering_survives_reload_without_changing_visibility_or_other_settings() {
        let file = TestFile::new();
        let mut config = Config::default();
        std::fs::write(
            &file.0,
            serde_json::json!({
                "enabled_widgets": ["weather", "calendar"], "theme": "teal",
                "project_roots": ["/projects/Voice-Agent"], "future_setting": 42
            })
            .to_string(),
        )
        .unwrap();
        let order = [
            Widget::Calendar,
            Widget::Projects,
            Widget::Weather,
            Widget::Devbox,
        ];
        config.save_order_at(&file.0, &order).unwrap();
        let bytes = std::fs::read(&file.0).unwrap();
        let reloaded: Config = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(reloaded.ordered_cards(), order);
        assert_eq!(reloaded.enabled_widgets, ["weather", "calendar"]);
        assert_eq!(reloaded.project_roots, ["/projects/Voice-Agent"]);
        assert_eq!(reloaded.theme, "teal");
        let raw: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(raw["future_setting"], 42);
    }

    #[test]
    fn incomplete_or_duplicate_order_still_exposes_every_card_once() {
        let config = Config {
            widget_order: ["clock", "calendar", "calendar", "unknown"]
                .map(String::from)
                .to_vec(),
            ..Config::default()
        };
        assert_eq!(config.ordered_cards(), Widget::CARDS);
    }

    #[test]
    fn automatic_wallpaper_colors_are_opt_in_and_survive_reload() {
        let old: Config = serde_json::from_value(serde_json::json!({
            "appearance": {"sync_colors": true, "match_wallpaper": true}
        }))
        .unwrap();
        let old = serde_json::to_value(old).unwrap();
        assert_eq!(old["appearance"]["wallpaper_colors"], false);
        let enabled: Config = serde_json::from_value(serde_json::json!({
            "appearance": {"wallpaper_colors": true, "wallpaper_color_stamp": 42}
        }))
        .unwrap();
        let saved = serde_json::to_value(enabled).unwrap();
        assert_eq!(saved["appearance"]["wallpaper_colors"], true);
        assert_eq!(saved["appearance"]["wallpaper_color_stamp"], 42);
    }
}

#[cfg(test)]
mod material_migration_tests {
    #[test]
    fn material_preferences_round_trip_without_changing_legacy_appearance() {
        let original = serde_json::json!({"appearance":{"opacity":83,"family":"paper"},"materials":{"shared":{"kind":"liquid","opacity":null,"tint":35},"widgets":null,"dock":null,"panel":null}});
        let config: super::Config = serde_json::from_value(original.clone()).unwrap();
        let saved = serde_json::to_value(config).unwrap();
        assert_eq!(saved["materials"], original["materials"]);
        assert_eq!(saved["appearance"]["opacity"], 83);
    }
}

fn yes() -> bool {
    true
}

#[cfg(test)]
mod auditfix_tests {
    use super::*;

    #[test]
    fn auditfix_w02_global_visibility_gates_grouped_providers_without_erasing_choices() {
        let mut config = Config::default();
        config.glance.enabled = true;
        config.glance.show_clock = true;
        config.glance.show_weather = true;
        config.glance.show_calendar = true;
        for widget in [Widget::Clock, Widget::Weather, Widget::Calendar] {
            assert!(config.data_visible(widget));
        }
        config.widgets_visible = false;
        assert!(
            Widget::ALL
                .into_iter()
                .all(|widget| !config.data_visible(widget))
        );
        assert!(
            config.glance.enabled
                && config.glance.show_clock
                && config.glance.show_weather
                && config.glance.show_calendar
        );
        config.widgets_visible = true;
        config.glance.show_weather = false;
        assert!(config.data_visible(Widget::Clock) && config.data_visible(Widget::Calendar));
        assert!(!config.data_visible(Widget::Weather));
    }
}
