use super::*;
use cosmic::Application;
use cosmic::iced::mouse;
use cosmic::iced::widget::keyed_column;

impl Cards {
    pub(super) fn set_widget_visible(&mut self, widget: Widget, visible: bool) -> Task<Message> {
        let mut config = self.config.clone();
        if let Err(error) = crate::desktop::actions::in_action("Widget visibility", || {
            config.save_widget(widget, visible)
        }) {
            eprintln!("Could not save widget selection: {error}");
            return Task::none();
        }
        self.apply_config(config)
    }
    pub(super) fn next_wake(&self) -> Option<Instant> {
        let visible = |widget| {
            if widget == Widget::Calendar {
                self.calendar_demand()
            } else {
                self.provider_visible(widget)
            }
        };
        [
            (self.provider_visible(Widget::Clock)).then_some(self.next_clock),
            (visible(Widget::Calendar)).then_some(self.next_calendar_view),
            (visible(Widget::Devbox)
                && self.config.devbox_sampling().needs_sample()
                && !self.metrics_fetching)
                .then_some(self.next_metrics),
            (visible(Widget::Devbox)
                && self.config.devbox_sampling().services
                && !self.service_fetching)
                .then_some(self.next_service),
            self.provider_visible(Widget::Weather)
                .then(|| self.weather_hub.deadline())
                .flatten(),
            (visible(Widget::Projects)
                && !self.config.project_roots.is_empty()
                && !self.projects_fetching)
                .then_some(self.next_projects),
            (visible(Widget::Calendar)
                && self.calendar_connection.connected()
                && self.calendar_can_start())
            .then_some(self.next_calendar),
            (visible(Widget::Calendar)
                && self.calendar_connection.connected()
                && self.month != Local::now().date_naive().with_day(1).unwrap()
                && self.today_calendar_can_start(&Local::now().format("%Y-%m").to_string()))
            .then_some(self.next_today_calendar),
        ]
        .into_iter()
        .flatten()
        .min()
    }
    /// The next moment anything is due: the one wake subscription.
    pub(super) fn wake_deadline(&self) -> Option<Instant> {
        self.next_wake()
            .into_iter()
            .chain(self.tools_wake())
            .chain(self.automation_wake())
            .min()
    }
    /// Deadlines are monotonic, and the monotonic clock stops in suspend.
    /// A wake that came `early` by the wall clock, or a wall clock that
    /// moved on further than the monotonic one since the last wake, means
    /// that much time was lost: every deadline moves that much closer, so
    /// the wake that fired is handled (and a new one armed) instead of
    /// every handler finding nothing due yet.
    pub(super) fn resumed(&mut self, early: Duration, now: Instant, wall: SystemTime) {
        let (then, then_wall) = std::mem::replace(&mut self.woke_at, (now, wall));
        let slept = wall
            .duration_since(then_wall)
            .unwrap_or_default()
            .saturating_sub(now.saturating_duration_since(then));
        // Below a second it is clock drift, not a suspend.
        let lost = if slept >= Duration::from_secs(1) {
            slept.max(early)
        } else {
            early
        };
        if lost.is_zero() {
            return;
        }
        for deadline in [
            &mut self.next_clock,
            &mut self.next_calendar_view,
            &mut self.next_metrics,
            &mut self.next_service,
            &mut self.next_projects,
            &mut self.next_calendar,
            &mut self.next_today_calendar,
        ] {
            *deadline = deadline.checked_sub(lost).unwrap_or(now);
        }
        self.weather_hub.resumed(lost, now);
        self.automator.resumed(lost, now);
    }
    pub(super) fn apply_config(&mut self, mut cfg: Config) -> Task<Message> {
        let mut tasks = Vec::new();
        let now = Instant::now();
        if self.sidebar.drag.is_some() {
            cfg.widget_order = self.config.widget_order.clone();
        }
        if cfg.devbox_sampling() != self.config.devbox_sampling() {
            self.next_metrics = now;
            self.next_service = now;
        }
        if cfg.location != self.config.location {
            let cached = cfg.location.as_ref().and_then(WeatherSnapshot::cached);
            self.weather = cached.as_ref().map(|snapshot| snapshot.weather.clone());
            self.weather_at = cached.map(|snapshot| snapshot.updated_at);
            self.weather_error = false;
        }
        if cfg.project_roots != self.config.project_roots {
            // A new folder selection must not keep showing checkouts from the
            // old selection while its first scan is in flight.
            self.projects = cfg.project_roots.is_empty().then(ProjectsData::default);
            self.projects_error = false;
            self.all_projects = false;
            self.next_projects = now;
        }
        for widget in Widget::ALL {
            if (cfg.data_visible(widget) && !self.config.data_visible(widget))
                || cfg.refresh_intervals.seconds(widget)
                    != self.config.refresh_intervals.seconds(widget)
            {
                match widget {
                    Widget::Devbox => {
                        self.next_metrics = now;
                        self.next_service = now;
                    }
                    Widget::Weather => self.weather_hub.due_now(now),
                    Widget::Calendar => {
                        self.next_calendar = now;
                        self.next_today_calendar = now;
                        self.next_calendar_view = now;
                    }
                    Widget::Projects => self.next_projects = now,
                    Widget::Clock => self.next_clock = now,
                    Widget::Notes => self.reload_note(),
                    Widget::Glance | Widget::Music | Widget::Focus => {}
                }
            }
        }
        if cfg.calendar_show_today != self.config.calendar_show_today
            || cfg.calendar_show_tomorrow != self.config.calendar_show_tomorrow
        {
            tasks.push(self.calendar_overview());
        }
        self.histories
            .reconcile(&cfg.devbox_sampling(), cfg.widget_visible(Widget::Devbox));
        let notes_shown = cfg.widget_visible(Widget::Notes);
        let notes_changed = notes_shown != self.config.widget_visible(Widget::Notes);
        let display_changed = self.config.widget_display != cfg.widget_display
            || self.config.widget_monitors != cfg.widget_monitors;
        self.config = cfg;
        self.refresh_display_layouts();
        self.reconcile_weather(now);
        if display_changed {
            tasks.push(self.reconcile_output());
        }
        tasks.push(self.wallpaper_colors_changed());
        if !self.provider_visible(Widget::Music) {
            // Followed again from scratch when shown.
            self.tools.feed = None;
        }
        if notes_changed {
            // A hidden note never keeps the keyboard; its draft waits,
            // paused, for an explicit Resume once the note is back.
            if !notes_shown {
                self.pause_note();
            }
            tasks.push(self.note_surface_mode());
        }
        tasks.push(self.sync_widget_surfaces());
        Task::batch(tasks)
    }
    pub(super) fn capture_if_changed(&mut self) -> Task<Message> {
        if std::env::var_os("DESKTOP_CARDS_CAPTURE_DIR").is_none() {
            return Task::none();
        }
        let key = format!(
            "{}:{:?}:{:?}:{}:{}:{:?}:{}:{}:{}:{:?}",
            self.tools.capture_key(),
            self.config,
            self.weather_at,
            self.selected_day,
            self.sidebar.visible,
            self.projects.as_ref().map(|p| &p.updated_at),
            self.calendar_home,
            self.calendar
                .as_ref()
                .map(|c| c.updated_at.as_str())
                .unwrap_or(""),
            self.show_earlier,
            self.expanded_projects
        );
        if self.capture_key.as_deref() == Some(&key) {
            return Task::none();
        }
        self.capture_key = Some(key);
        cosmic::task::future(async {
            tokio::time::sleep(Duration::from_millis(250)).await;
            Message::CaptureScreens
        })
    }
    pub(super) fn refresh_visible_data(&mut self) -> Vec<Task<Message>> {
        let mut tasks = Vec::new();
        let now = Instant::now();
        if self.provider_visible(Widget::Clock) && now >= self.next_clock {
            self.clock_at = Local::now();
            self.next_clock = crate::schedule::next_clock_tick(
                now,
                self.config.refresh_intervals.duration(Widget::Clock),
            );
        }
        if self.calendar_demand() && now >= self.next_calendar_view {
            // Local meeting highlights and midnight transitions stay timely even
            // when the Google fetch interval is long.
            self.next_calendar_view =
                crate::schedule::next_clock_tick(now, Duration::from_secs(60));
            // Event rows can shrink or grow as meetings end or become imminent.
            // Keep custom surfaces and the Settings miniature in sync as well.
            tasks.push(self.sync_widget_surfaces());
        }
        if self.provider_visible(Widget::Devbox) {
            if self.config.devbox_sampling().needs_sample()
                && !self.metrics_fetching
                && now >= self.next_metrics
            {
                self.metrics_fetching = true;
                self.next_metrics = now + self.config.refresh_intervals.duration(Widget::Devbox);
                let mut sampler = std::mem::take(&mut self.sampler);
                let previous = self.metrics.clone();
                let preferences = self.config.devbox_sampling().into_owned();
                tasks.push(cosmic::task::future(async move {
                    Message::Metrics(
                        tokio::task::spawn_blocking(move || {
                            let metrics = sampler.sample(&previous, &preferences);
                            (sampler, metrics)
                        })
                        .await
                        .map_err(|e| e.to_string()),
                    )
                }));
            }
            if self.config.devbox_sampling().services
                && !self.service_fetching
                && now >= self.next_service
            {
                self.service_fetching = true;
                self.next_service = now
                    + self
                        .config
                        .refresh_intervals
                        .duration(Widget::Devbox)
                        .max(Duration::from_secs(15));
                tasks.push(cosmic::task::future(async {
                    Message::Service(data::t3_active().await)
                }));
            }
        }
        tasks.extend(self.refresh_weather(now));
        if self.provider_visible(Widget::Projects)
            && !self.config.project_roots.is_empty()
            && !self.projects_fetching
            && now >= self.next_projects
        {
            tasks.push(self.projects_task());
        }
        if self.calendar_demand() && self.calendar_connection.connected() {
            let today = Local::now().date_naive();
            let month = today.format("%Y-%m").to_string();
            if self.month != today.with_day(1).unwrap()
                && self.today_calendar_can_start(&month)
                && now >= self.next_today_calendar
            {
                self.today_fetching = Some(month.clone());
                tasks.push(self.calendar_month_task(month));
            }
            if self.calendar_can_start() && now >= self.next_calendar {
                tasks.push(self.calendar_task());
            }
        }
        tasks
    }
    pub(super) fn projects_task(&mut self) -> Task<Message> {
        self.projects_fetching = true;
        let roots = self.config.project_roots.clone();
        cosmic::task::future(async move {
            let result = data::fetch_projects(roots.clone()).await;
            Message::Projects(roots, result)
        })
    }
    pub(super) fn command_path() -> Option<std::path::PathBuf> {
        if std::env::var_os("DESKTOP_CARDS_CONFIG").is_some() {
            Config::path()
                .and_then(|p| p.parent().map(|parent| parent.join("desktop-command.json")))
        } else {
            crate::paths::runtime_dir().map(|dir| dir.join("command.json"))
        }
    }
    pub(super) fn read_command() -> Option<Message> {
        let path = Self::command_path()?;
        let bytes = std::fs::read(&path).ok()?;
        let _ = std::fs::remove_file(path);
        let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        match value["action"].as_str()? {
            "arrange" => Some(match value["display"].as_str() {
                Some(display) if crate::displays::valid_name(display) => {
                    Message::ArrangeDisplay(display.into())
                }
                _ => Message::Arrange,
            }),
            "reset-layout" => Some(Message::ResetLayout),
            "close" => Some(Message::HideControls),
            "rail" => Some(Message::ShowControls),
            "open" => match value["widget"].as_str() {
                None | Some("open") => Some(Message::ShowControls),
                Some(key) => Some(Message::FocusWidget(Widget::parse(key)?)),
            },
            "toggle" => Some(Message::ToggleWidget(Widget::parse(
                value["widget"].as_str()?,
            )?)),
            "focus" => Some(Message::FocusCommand(value["do"].as_str()?.to_string())),
            "capture" if std::env::var_os("DESKTOP_CARDS_CAPTURE_DIR").is_some() => {
                Some(Message::RequestCapture)
            }
            "preview-calendar"
                if std::env::var_os("DESKTOP_CARDS_CAPTURE_DIR").is_some()
                    && std::env::var_os("DESKTOP_CARDS_CONFIG").is_some() =>
            {
                serde_json::from_value(value["data"].clone())
                    .ok()
                    .map(Message::PreviewCalendar)
            }
            "preview-day" if std::env::var_os("DESKTOP_CARDS_CAPTURE_DIR").is_some() => Some(
                Message::Day(NaiveDate::parse_from_str(value["date"].as_str()?, "%Y-%m-%d").ok()?),
            ),
            _ => None,
        }
    }
    pub(super) fn meeting_alert(&self) -> bool {
        if !self.calendar_demand() {
            return false;
        }
        let now = Local::now();
        self.calendar_today.as_ref().is_some_and(|calendar| {
            calendar.events.iter().any(|event| {
                matches!(
                    event.meeting_state_at(now, self.config.meeting_notice_minutes),
                    MeetingState::Soon(_) | MeetingState::Live
                )
            })
        })
    }
    fn rail_button(&self, widget: Widget, draggable: bool) -> Element<'_, Message> {
        let draggable = draggable
            && !self.custom_layout()
            && !(self.config.glance.enabled
                && matches!(widget, Widget::Clock | Widget::Weather | Widget::Calendar));
        let p = self.config.surface_palette();
        let selected = self.config.data_visible(widget);
        let dragged = self
            .sidebar
            .drag
            .as_ref()
            .is_some_and(|d| d.widget == widget && d.moved);
        let alert = widget == Widget::Calendar && self.meeting_alert();
        let color = if alert {
            Color::from_rgb8(245, 190, 107)
        } else {
            p.accent
        };
        let content = row::with_children(vec![
            label(if draggable { "⠿" } else { " " }, 13., p.muted),
            cosmic::widget::icon::from_name(widget.icon())
                .size(18)
                .icon()
                .into(),
            container(label(
                widget.title(),
                14.,
                if selected { p.foreground } else { p.muted },
            ))
            .width(Length::Fill)
            .into(),
            label(
                if selected { "✓" } else { "+" },
                13.,
                if selected { color } else { p.muted },
            ),
        ])
        .spacing(7)
        .align_y(Alignment::Center);
        mouse_area(
            container(content)
                .padding([0, 8])
                .width(Length::Fill)
                .height(Length::Fixed(drawer::ROW_HEIGHT))
                .align_y(Alignment::Center)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        icon_color: Some(if selected { color } else { p.muted }),
                        background: (selected || dragged).then_some(Background::Color(mix(
                            color,
                            p.background,
                            if dragged { 0.24 } else { 0.10 },
                        ))),
                        border: Border {
                            color: Color {
                                a: if dragged { 0.95 } else { 0.30 },
                                ..color
                            },
                            width: if dragged {
                                2.
                            } else if selected {
                                1.
                            } else {
                                0.
                            },
                            radius: 12.into(),
                        },
                        ..Default::default()
                    }
                })),
        )
        .on_press(if draggable {
            Message::SidebarPress(widget)
        } else {
            Message::ToggleWidget(widget)
        })
        .interaction(if dragged {
            mouse::Interaction::Grabbing
        } else if draggable {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::Pointer
        })
        .into()
    }
    /// Music, Notes and Focus side by side: click to show or hide.
    fn tool_toggles(&self) -> Element<'_, Message> {
        let p = self.config.surface_palette();
        row::with_children(Widget::TOOLS.map(|tool| {
            let selected = self.config.widget_visible(tool);
            mouse_area(
                container(
                    column::with_children(vec![
                        cosmic::widget::icon::from_name(tool.icon())
                            .size(16)
                            .icon()
                            .into(),
                        label(
                            tool.title(),
                            10.,
                            if selected { p.foreground } else { p.muted },
                        ),
                    ])
                    .spacing(3)
                    .align_x(Alignment::Center),
                )
                .width(Length::Fill)
                .height(Length::Fixed(drawer::ROW_HEIGHT))
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        icon_color: Some(if selected { p.accent } else { p.muted }),
                        background: selected.then_some(Background::Color(mix(
                            p.accent,
                            p.background,
                            0.10,
                        ))),
                        border: Border {
                            color: Color {
                                a: 0.30,
                                ..p.accent
                            },
                            width: if selected { 1. } else { 0. },
                            radius: 12.into(),
                        },
                        ..Default::default()
                    }
                })),
            )
            .on_press(Message::ToggleWidget(tool))
            .interaction(mouse::Interaction::Pointer)
            .into()
        }))
        .spacing(6)
        .into()
    }
    pub(super) fn control_rail(&self) -> Element<'_, Message> {
        if !self.sidebar.visible && self.sidebar.progress == 0. {
            return text::body("").into();
        }
        if let Some(recovery) = self.arrange_recovery_view() {
            return recovery;
        }
        let p = self.config.surface_palette();
        let rows = keyed_column(
            self.config
                .ordered_right_cards()
                .into_iter()
                .map(|widget| (widget, self.rail_button(widget, true))),
        )
        .spacing(6)
        .height(Length::Fixed(144.));
        let header = container(
            column::with_children(vec![
                label("DESKTOP", 12., p.accent),
                label("Drag the right-side cards", 10., p.muted),
            ])
            .spacing(4),
        )
        .height(Length::Fixed(42.));
        let settings = mouse_area(
            container(
                row::with_children(vec![
                    cosmic::widget::icon::from_name("preferences-system-symbolic")
                        .size(17)
                        .icon()
                        .into(),
                    label("Settings", 12., p.muted),
                ])
                .spacing(9)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fixed(32.))
            .padding([0, 10])
            .align_y(Alignment::Center),
        )
        .on_press(Message::Settings)
        .interaction(mouse::Interaction::Pointer);
        mouse_area(
            container(
                column::with_children(vec![
                    header.into(),
                    rows.into(),
                    self.rail_button(Widget::Calendar, false),
                    self.rail_button(Widget::Clock, false),
                    self.rail_button(Widget::Glance, false),
                    self.tool_toggles(),
                    settings.into(),
                    label(
                        if self.custom_layout() {
                            "Custom layout · click to show"
                        } else {
                            "Click to show · drag to move"
                        },
                        9.,
                        p.muted,
                    ),
                ])
                .spacing(10),
            )
            .padding(12)
            .width(Length::Fill)
            .height(Length::Fill)
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    background: Some(Background::Color(Color {
                        a: 0.98,
                        ..p.background
                    })),
                    icon_color: Some(p.muted),
                    border: Border {
                        color: Color { a: 0.4, ..p.accent },
                        width: 1.,
                        radius: [20., 0., 0., 20.].into(),
                    },
                    ..Default::default()
                }
            })),
        )
        .on_enter(Message::Hover(false, true))
        .on_exit(Message::Hover(false, false))
        .into()
    }
    pub(super) fn edge_handle(&self) -> Element<'_, Message> {
        mouse_area(
            container(text::body(""))
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .on_enter(Message::Hover(true, true))
        .on_exit(Message::Hover(true, false))
        .into()
    }
    pub(super) fn visible_cards(&self) -> Vec<Widget> {
        self.config
            .ordered_right_cards()
            .into_iter()
            .filter(|w| self.config.widget_visible(*w))
            .collect()
    }
    pub(super) fn card_height(&self, widget: Widget) -> f32 {
        if self.custom_layout() || self.layout_editor.active {
            return self.widget_size(widget)[1];
        }
        if widget == Widget::Calendar {
            return self.calendar_height.max(1.);
        }
        self.widget_size(widget)[1]
    }
    pub(super) fn calendar_widget(&self) -> Element<'_, Message> {
        let mut widgets = Vec::new();
        if self.config.widget_visible(Widget::Calendar) {
            widgets.push(self.calendar_card());
        }
        container(column::with_children(widgets).width(Length::Fill))
            .height(Length::Fill)
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .into()
    }
    pub(super) fn desktop_widgets(&self) -> Element<'_, Message> {
        let widgets = self.visible_cards();
        if widgets.is_empty() {
            return text::body("").into();
        }
        let cards = keyed_column(widgets.into_iter().map(|widget| {
            let view = match widget {
                Widget::Projects => self.projects_card(),
                Widget::Devbox => self.devbox(),
                Widget::Weather => self.weather_card(),
                _ => unreachable!("Calendar and Clock live on the left; tools float"),
            };
            (widget, view)
        }))
        .spacing(drawer::CARD_GAP)
        .width(Length::Fill);
        container(cards)
            .padding(crate::widget_style::shadow_margin(&self.config))
            .height(Length::Fill)
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .into()
    }
    pub(super) fn sidebar_pointer(&mut self, id: window::Id, event: mouse::Event) -> Task<Message> {
        let now = Instant::now();
        if id == self.ids[3] {
            match event {
                mouse::Event::CursorEntered | mouse::Event::CursorMoved { .. }
                    if !self.sidebar.edge_hovered =>
                {
                    self.sidebar.hover(true, true, now)
                }
                mouse::Event::CursorLeft => self.sidebar.hover(true, false, now),
                _ => {}
            }
        }
        if id != self.ids[2] {
            if matches!(event, mouse::Event::ButtonReleased(mouse::Button::Left)) {
                if let Some(drag) = self.sidebar.drag.take() {
                    self.config.widget_order =
                        drag.original_order.iter().map(|w| w.key().into()).collect();
                    self.sidebar.dismiss_if_outside(now);
                }
            }
            return Task::none();
        }
        match event {
            mouse::Event::CursorEntered => self.sidebar.hover(false, true, now),
            mouse::Event::CursorLeft => self.sidebar.hover(false, false, now),
            mouse::Event::CursorMoved { position } => {
                self.sidebar.pointer = position;
                let inside = (0. ..=drawer::SIDEBAR_WIDTH as f32).contains(&position.x)
                    && (0. ..=drawer::SIDEBAR_HEIGHT as f32).contains(&position.y);
                if inside != self.sidebar.hovered {
                    self.sidebar.hover(false, inside, now);
                }
                let mut order = self.config.ordered_right_cards();
                if let Some(drag) = &mut self.sidebar.drag {
                    drag.preview(position, &mut order);
                    self.config.widget_order = order.iter().map(|w| w.key().into()).collect();
                }
            }
            mouse::Event::ButtonReleased(mouse::Button::Left) => {
                if let Some(drag) = self.sidebar.drag.take() {
                    let original = drag.original_order.clone();
                    let result =
                        drag.finish(self.sidebar.pointer, self.config.ordered_right_cards());
                    self.sidebar.dismiss_if_outside(now);
                    match result {
                        DragResult::Click(widget) => {
                            return self.update(Message::ToggleWidget(widget));
                        }
                        DragResult::Cancel(order) => {
                            self.config.widget_order =
                                order.iter().map(|w| w.key().into()).collect()
                        }
                        DragResult::Reorder(order) => {
                            if let Err(error) = self.config.save_order(&order) {
                                self.config.widget_order =
                                    original.iter().map(|w| w.key().into()).collect();
                                eprintln!("Could not save widget order: {error}");
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        Task::none()
    }
}
