use super::pages::section;
use super::*;

impl Settings {
    pub(in crate::settings) fn needs_setup(&self) -> bool {
        self.setup_requested
            || self.config.onboarded
                && !self
                    .setup_followups
                    .pending(&self.config, self.calendar.connection.connected())
                    .is_empty()
            || !self.config.setup_dismissed
                && !self.config.onboarded
                && (!self.calendar.connection.connected()
                    || self.config.location.is_none()
                    || (self.service_known && !self.service.running))
    }
    /// Short, optional content tasks; no provider request or preference write on view.
    pub(super) fn widget_followups(&self) -> Element<'_, Message> {
        let pending = self
            .setup_followups
            .pending(&self.config, self.calendar.connection.connected());
        let mut rows = vec![detail(
            "Finish your widgets",
            "Optional · add your own content",
        )];
        for task in pending {
            rows.push(split(
                label(task.title(), 14., FG),
                row::with_children(vec![
                    button::standard(task.action())
                        .id(cosmic::widget::Id::new(format!(
                            "setup-task-{}",
                            task.key()
                        )))
                        .on_press(Message::Page(task.page()))
                        .into(),
                    button::icon(icon::from_name("window-close-symbolic").size(14))
                        .id(cosmic::widget::Id::new(format!(
                            "setup-task-dismiss-{}",
                            task.key()
                        )))
                        .tooltip("Dismiss this reminder")
                        .on_press(Message::DismissSetupTask(task))
                        .into(),
                ])
                .spacing(6)
                .align_y(Alignment::Center)
                .into(),
            ));
        }
        section(rows)
    }

    pub(super) fn setup_card(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let mut startup = toggler(self.service.startup);
        if self.service_known && !self.busy && !preview() {
            startup = startup.on_toggle(Message::Startup);
        }
        let step = |number: &str,
                    title: &str,
                    description: String,
                    ready: bool,
                    button_text: &str,
                    action: Option<Message>| {
            split(
                row::with_children(vec![
                    pill(
                        if ready { "✓" } else { number },
                        if ready { accent } else { MUTED },
                    ),
                    detail(title.to_string(), description),
                ])
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
                button::standard(button_text.to_string())
                    .id(cosmic::widget::Id::new(format!("setup-step-{number}")))
                    .on_press_maybe(action)
                    .into(),
            )
        };
        section(vec![
            split(
                detail(
                    "Getting started",
                    "Make the desktop yours. Every step is optional.",
                ),
                button::text("Skip setup")
                    .id(cosmic::widget::Id::new("setup-skip"))
                    .on_press(Message::SetupDismissed(true))
                    .into(),
            ),
            step(
                "1",
                "Widgets · My widgets",
                if self.service.running {
                    "Running".into()
                } else {
                    "Choose which widgets to show.".into()
                },
                self.service.running,
                if self.service.running {
                    "Choose widgets"
                } else if !self.service_known {
                    "Checking…"
                } else {
                    "Show widgets"
                },
                if self.service.running {
                    Some(Message::Page(Page::Widgets))
                } else {
                    (self.service_known && !self.busy && !preview())
                        .then_some(Message::Desktop(true))
                },
            ),
            separator(),
            split(
                detail(
                    "App settings · Start at sign-in",
                    "Run Halcyon when you sign in.",
                ),
                startup.into(),
            ),
            separator(),
            step(
                "2",
                "Widgets · Weather",
                self.config
                    .location
                    .as_ref()
                    .map(|location| location.name.clone())
                    .unwrap_or_else(|| "Choose a city. No account or API key needed.".into()),
                self.config.location.is_some(),
                if self.config.location.is_some() {
                    "Change city"
                } else {
                    "Choose city"
                },
                Some(Message::Page(Page::Weather)),
            ),
            separator(),
            step(
                "3",
                "Appearance · Theme and colors",
                match self.config.appearance.family {
                    Some(family) if self.config.appearance.wallpaper_colors => {
                        format!("{} · colors from your wallpaper", family.title())
                    }
                    Some(family) => format!("{} theme", family.title()),
                    None => {
                        "Style widgets, dock and panel together, in your wallpaper’s colors.".into()
                    }
                },
                self.config.appearance.family.is_some(),
                if self.config.appearance.family.is_some() {
                    "Change theme"
                } else {
                    "Choose a theme"
                },
                Some(Message::Page(Page::Themes)),
            ),
            separator(),
            step(
                "4",
                "Widgets · Calendar",
                if self.calendar.connection.connected() {
                    self.calendar.connection.account.clone()
                } else {
                    "Connect Google Calendar or add a read-only calendar link.".into()
                },
                self.calendar.connection.connected(),
                if self.calendar.connection.connected() {
                    "Manage"
                } else {
                    "Set up calendar"
                },
                Some(Message::Page(Page::Calendar)),
            ),
            separator(),
            row::with_children(vec![
                button::text("Add project folders")
                    .on_press(Message::Page(Page::Projects))
                    .into(),
                button::text("More widgets")
                    .on_press(Message::Page(Page::Widgets))
                    .into(),
            ])
            .spacing(8)
            .wrap()
            .into(),
            label(
                "Move and resize widgets in Layout and displays. For quick controls, hold the pointer at the far-right screen edge for one second.",
                11.,
                MUTED,
            ),
        ])
    }

    pub(super) fn setup_prompt(&self) -> Element<'_, Message> {
        if self.config.onboarded
            && !self
                .setup_followups
                .pending(&self.config, self.calendar.connection.connected())
                .is_empty()
        {
            return self.widget_followups();
        }
        frame(
            split(
                detail(
                    "Getting started",
                    "Choose a weather city and connect your calendar.",
                ),
                row::with_children(vec![
                    button::standard("Continue setup")
                        .id(cosmic::widget::Id::new("setup-open"))
                        .on_press(Message::SetupOpen(true))
                        .into(),
                    button::icon(icon::from_name("window-close-symbolic").size(14))
                        .id(cosmic::widget::Id::new("setup-dismiss"))
                        .tooltip("Dismiss setup")
                        .on_press(Message::SetupDismissed(true))
                        .into(),
                ])
                .spacing(6)
                .align_y(Alignment::Center)
                .into(),
            ),
            SURFACE,
            LINE,
            12,
        )
    }
}
