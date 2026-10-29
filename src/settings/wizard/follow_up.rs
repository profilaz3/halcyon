//! Optional content tasks for selected widgets. Dismissals contain no provider data.
use super::super::Page;
use crate::{config::Config, drawer::Widget};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Task {
    Weather,
    Calendar,
    Projects,
}

impl Task {
    pub const ALL: [Self; 3] = [Self::Weather, Self::Calendar, Self::Projects];
    pub fn key(self) -> &'static str {
        match self {
            Self::Weather => "weather",
            Self::Calendar => "calendar",
            Self::Projects => "projects",
        }
    }
    fn preference(self) -> &'static str {
        match self {
            Self::Weather => "setup_followup_weather_dismissed",
            Self::Calendar => "setup_followup_calendar_dismissed",
            Self::Projects => "setup_followup_projects_dismissed",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Weather => "Choose a weather city",
            Self::Calendar => "Connect your calendar",
            Self::Projects => "Choose project folders",
        }
    }
    pub fn action(self) -> &'static str {
        match self {
            Self::Weather => "Choose city",
            Self::Calendar => "Set up calendar",
            Self::Projects => "Choose folders",
        }
    }
    pub fn page(self) -> Page {
        match self {
            Self::Weather => Page::Weather,
            Self::Calendar => Page::Calendar,
            Self::Projects => Page::Projects,
        }
    }
    pub fn needed(self, config: &Config, calendar_connected: bool) -> bool {
        let (widget, missing) = match self {
            Self::Weather => (Widget::Weather, config.location.is_none()),
            Self::Calendar => (Widget::Calendar, !calendar_connected),
            Self::Projects => (Widget::Projects, config.project_roots.is_empty()),
        };
        config.data_visible(widget) && missing
    }
}

#[derive(Clone, Debug, Default)]
pub struct State {
    dismissed: [bool; 3],
}

impl State {
    pub fn load() -> Self {
        let raw = super::widgets::raw().unwrap_or_default();
        Self::from_raw(&raw)
    }
    fn from_raw(raw: &serde_json::Value) -> Self {
        Self {
            dismissed: Task::ALL.map(|task| {
                raw.get(task.preference())
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            }),
        }
    }
    pub fn pending(&self, config: &Config, calendar_connected: bool) -> Vec<Task> {
        Task::ALL
            .into_iter()
            .enumerate()
            .filter_map(|(index, task)| {
                (!self.dismissed[index] && task.needed(config, calendar_connected)).then_some(task)
            })
            .collect()
    }
    pub fn dismiss(&mut self, task: Task) -> Result<Config, String> {
        // Patch one allowlisted boolean through the existing guarded config writer.
        // Unknown preferences, notes and provider records are never replaced.
        let config = Config::save_changes(serde_json::json!({ (task.preference()): true }))?;
        self.dismissed[Task::ALL.iter().position(|t| *t == task).unwrap()] = true;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn polish050_followups_only_include_selected_unconfigured_content() {
        let mut config = Config::default();
        config.widgets_visible = true;
        config.glance.enabled = false;
        config.enabled_widgets = vec!["clock".into(), "weather".into(), "projects".into()];
        config.location = None;
        config.project_roots.clear();
        let tasks = State::default();
        assert_eq!(
            tasks.pending(&config, false),
            vec![Task::Weather, Task::Projects]
        );
        config.project_roots.push("~/Projects".into());
        assert_eq!(tasks.pending(&config, false), vec![Task::Weather]);
        config.widgets_visible = false;
        assert!(tasks.pending(&config, false).is_empty());
        assert!(config.widget_selected(Widget::Weather));
    }
    #[test]
    fn polish050_followups_honor_group_sections_and_individual_dismissal() {
        let mut config = Config::default();
        config.widgets_visible = true;
        config.glance.enabled = true;
        config.glance.show_weather = true;
        config.glance.show_calendar = true;
        config.location = None;
        config.enabled_widgets.clear();
        let tasks = State::from_raw(&serde_json::json!({
            "setup_followup_weather_dismissed": true,
            "setup_followup_calendar_dismissed": "invalid",
            "unknown_future_field": "retained"
        }));
        assert_eq!(tasks.pending(&config, false), vec![Task::Calendar]);
        assert!(tasks.pending(&config, true).is_empty());
        config.glance.show_calendar = false;
        assert!(tasks.pending(&config, false).is_empty());
    }
}
