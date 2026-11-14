//! Local display choices. Portable setups never need these machine identities.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct WidgetMonitor {
    /// None inherits the default widget display.
    pub display: Option<String>,
    pub hide_disconnected: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MonitorLayout {
    pub position: Option<[f32; 2]>,
    pub size: Option<[f32; 2]>,
}
pub type Contexts = BTreeMap<String, String>;

impl Config {
    pub fn save_monitor_field(
        widget: Widget,
        field: &str,
        value: serde_json::Value,
    ) -> Result<Self, String> {
        if !matches!(field, "display" | "hide_disconnected") {
            return Err("Unknown display option".into());
        }
        let path = Self::path().ok_or("Configuration directory unavailable")?;
        let saved = super::patch_config(&path, |raw| {
            let root = raw
                .as_object_mut()
                .ok_or("Configuration must be an object")?;
            let monitors = root
                .entry("widget_monitors")
                .or_insert_with(|| serde_json::json!({}))
                .as_object_mut()
                .ok_or("Widget displays must be an object")?;
            let monitor = monitors
                .entry(widget.key())
                .or_insert_with(|| serde_json::json!({}))
                .as_object_mut()
                .ok_or("Widget display must be an object")?;
            monitor.insert(field.into(), value);
            Ok(())
        })?;
        Self::from_saved_value(saved)
    }
    pub fn multi_monitor(&self) -> bool {
        !self.widget_monitors.is_empty() || !self.monitor_layouts.is_empty()
    }
    pub fn monitor_preference(&self, widget: Widget) -> Option<&str> {
        self.widget_monitors
            .get(widget.key())
            .and_then(|m| m.display.as_deref())
            .or(self.widget_display.as_deref())
    }
    pub fn on_displays(&self, contexts: &Contexts) -> Self {
        let mut config = self.clone();
        for (widget, display) in contexts {
            if let Some(layout) = self
                .monitor_layouts
                .get(widget)
                .and_then(|m| m.get(display))
            {
                for (map, value) in [
                    (&mut config.widget_positions, layout.position),
                    (&mut config.widget_sizes, layout.size),
                ] {
                    if let Some(value) = value {
                        map.insert(widget.clone(), value);
                    } else {
                        map.remove(widget);
                    }
                }
            }
        }
        config
    }
    pub(super) fn validate_monitors(&self) -> Result<(), String> {
        if self.widget_monitors.len() > Widget::ALL.len()
            || self.monitor_layouts.len() > Widget::ALL.len()
        {
            return Err("Too many saved display layouts".into());
        }
        for (key, monitor) in &self.widget_monitors {
            if Widget::parse(key).is_none()
                || monitor
                    .display
                    .as_deref()
                    .is_some_and(|s| !crate::displays::valid_name(s))
            {
                return Err("Invalid widget display".into());
            }
        }
        for (key, layouts) in &self.monitor_layouts {
            if Widget::parse(key).is_none() || layouts.len() > 32 {
                return Err("Invalid display layout".into());
            }
            for (display, layout) in layouts {
                if !crate::displays::valid_name(display)
                    || layout.position.is_some_and(|p| {
                        p.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                    })
                    || layout.size.is_some_and(|s| !crate::layout::valid_size(s))
                {
                    return Err("Invalid saved widget geometry".into());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multi_monitor_layouts_preserve_original_and_other_displays() {
        let dir = crate::test_env::TempPath::new("multilayout");
        let path = dir.join("config.json");
        std::fs::write(&path,r#"{"widget_positions":{"weather":[0.8,0.2]},"widget_sizes":{"weather":[500,300]},"widget_monitors":{"weather":{"display":"DP-2"}},"future":{"keep":true}}"#).unwrap();
        let mut contexts = Contexts::from([("weather".into(), "eDP-1".into())]);
        let positions = BTreeMap::from([("weather".into(), [0.1, 0.4])]);
        let baseline = BTreeMap::from([("weather".into(), [0.8, 0.2])]);
        let sizes = BTreeMap::from([("weather".into(), [320., 220.])]);
        let baseline_sizes = BTreeMap::from([("weather".into(), [500., 300.])]);
        let saved = Config::save_layout_context_at(
            &path,
            &contexts,
            &positions,
            &baseline,
            &sizes,
            &baseline_sizes,
            &Default::default(),
            PlacementPreferences::default(),
            PlacementPreferences::default(),
        )
        .unwrap();
        assert_eq!(saved.widget_positions["weather"], [0.8, 0.2]);
        assert_eq!(saved.widget_sizes["weather"], [500., 300.]);
        assert_eq!(
            saved.on_displays(&contexts).widget_positions["weather"],
            [0.1, 0.4]
        );
        contexts.insert("weather".into(), "DP-2".into());
        assert_eq!(
            saved.on_displays(&contexts).widget_positions["weather"],
            [0.8, 0.2]
        );
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(raw["future"]["keep"], true);
        assert!(raw["monitor_layouts"]["weather"].get("eDP-1").is_some());
        // Explicit automatic size on the temporary display survives reload.
        contexts.insert("weather".into(), "eDP-1".into());
        let saved = Config::save_layout_context_at(
            &path,
            &contexts,
            &positions,
            &positions,
            &Default::default(),
            &sizes,
            &Default::default(),
            PlacementPreferences::default(),
            PlacementPreferences::default(),
        )
        .unwrap();
        assert!(
            !saved
                .on_displays(&contexts)
                .widget_sizes
                .contains_key("weather")
        );
        assert_eq!(saved.widget_sizes["weather"], [500., 300.]);
    }
    #[test]
    fn multi_monitor_rejects_invalid_layout_and_portable_profiles_exclude_local_fields() {
        let mut config = Config::default();
        config.monitor_layouts.insert(
            "weather".into(),
            BTreeMap::from([(
                "DP-1".into(),
                MonitorLayout {
                    position: Some([f32::NAN, 0.]),
                    size: None,
                },
            )]),
        );
        assert!(config.validate().is_err());
        for group in [
            crate::desktop::store::widget::WidgetGroup::Style,
            crate::desktop::store::widget::WidgetGroup::Layout,
        ] {
            assert!(
                crate::desktop::store::widget::fields(group)
                    .all(|f| !f.pointer.contains("monitor"))
            );
        }
        assert!(crate::desktop::store::widget::field("/monitor_layouts/weather").is_some());
    }
}
