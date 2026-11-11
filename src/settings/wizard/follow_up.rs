//! Optional content tasks for selected widgets, listed on the wizard's Done step.
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

#[cfg(test)]
mod tests {
    use super::*;
    fn pending(config: &Config, calendar_connected: bool) -> Vec<Task> {
        Task::ALL
            .into_iter()
            .filter(|task| task.needed(config, calendar_connected))
            .collect()
    }
    #[test]
    fn polish050_followups_only_include_selected_unconfigured_content() {
        let mut config = Config::default();
        config.widgets_visible = true;
        config.glance.enabled = false;
        config.enabled_widgets = vec!["clock".into(), "weather".into(), "projects".into()];
        config.location = None;
        config.project_roots.clear();
        assert_eq!(pending(&config, false), vec![Task::Weather, Task::Projects]);
        config.project_roots.push("~/Projects".into());
        assert_eq!(pending(&config, false), vec![Task::Weather]);
        config.widgets_visible = false;
        assert!(pending(&config, false).is_empty());
        assert!(config.widget_selected(Widget::Weather));
    }
    #[test]
    fn polish050_followups_honor_group_sections() {
        let mut config = Config::default();
        config.widgets_visible = true;
        config.glance.enabled = true;
        config.glance.show_weather = true;
        config.glance.show_calendar = true;
        config.location = None;
        config.enabled_widgets.clear();
        assert_eq!(pending(&config, false), vec![Task::Weather, Task::Calendar]);
        assert_eq!(pending(&config, true), vec![Task::Weather]);
        config.glance.show_calendar = false;
        config.glance.show_weather = false;
        assert!(pending(&config, false).is_empty());
    }
}
