use super::*;
use cosmic::Application;
use cosmic::iced::mouse;
use cosmic::iced::widget::keyed_column;
use drawer::{DrawerItem, DrawerKey};

/// The edge menu's second group, in order.
use crate::config::edge_menu::ELSEWHERE;

impl Cards {
    pub(super) fn set_widget_visible(&mut self, widget: Widget, visible: bool) -> Task<Message> {
        let mut config = self.config.clone();
        if let Err(error) = crate::desktop::actions::in_action("Widget visibility", || {
            config.save_widget(widget, visible)
        }) {
            eprintln!("Could not save widget selection: {error}");
            self.raise_notice(
                "Couldn’t save the widget list",
                "Your change wasn’t kept.",
                error.to_string(),
                Some(Message::SetWidgetVisible(widget, visible)),
            );
            return Task::none();
        }
        self.apply_config(config)
    }
    /// Shows a failed desktop change in the edge menu, opening it.
    pub(super) fn raise_notice(
        &mut self,
        title: &'static str,
        body: &'static str,
        error: String,
        retry: Option<Message>,
    ) {
        self.notice = Some(Notice {
            title,
            body,
            error,
            retry,
            details: false,
        });
        self.sidebar.show(Instant::now());
    }
    pub(super) fn save_widget_order(&mut self, order: Vec<Widget>) -> Task<Message> {
        let original = self.config.ordered_right_cards();
        if let Err(error) = self.config.save_order(&order) {
            self.config.widget_order = original.iter().map(|w| w.key().into()).collect();
            eprintln!("Could not save widget order: {error}");
            self.raise_notice(
                "Couldn’t save the widget order",
                "The widgets are back in their saved order.",
                error.to_string(),
                Some(Message::SaveOrder(order)),
            );
        }
        Task::none()
    }
    /// Every place the keyboard can be in the edge menu, top down.
    pub(super) fn drawer_items(&self) -> Vec<DrawerItem> {
        if let Some(items) = self.menu_items() {
            return items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.focusable())
                .map(|(index, _)| DrawerItem::Entry(index))
                .chain([DrawerItem::Customize])
                .collect();
        }
        self.config
            .ordered_right_cards()
            .into_iter()
            .chain(ELSEWHERE)
            .map(DrawerItem::Widget)
            .chain([
                DrawerItem::Arrange,
                DrawerItem::Settings,
                DrawerItem::Customize,
            ])
            .collect()
    }
    pub(super) fn drawer_focus(&self) -> Option<DrawerItem> {
        let items = self.drawer_items();
        self.sidebar
            .focus
            .map(|index| items[index.min(items.len() - 1)])
    }
    /// `halcyon --toggle drawer`.
    pub(super) fn drawer_toggle(&mut self) -> Task<Message> {
        if self.sidebar.visible {
            return self.update(Message::HideControls);
        }
        if !self.controls_open {
            // From the keyboard, on the widgets' display.
            self.edge.menu_output = None;
        }
        self.sidebar.show(Instant::now());
        self.sidebar.focus = Some(0);
        if self.controls_open {
            // Still sliding out: the open surface takes the keys now.
            set_keyboard_interactivity(self.ids[2], KeyboardInteractivity::Exclusive)
        } else {
            Task::none()
        }
    }
    pub(super) fn drawer_key(&mut self, key: DrawerKey) -> Task<Message> {
        let Some(index) = self.sidebar.focus else {
            return Task::none();
        };
        if key == DrawerKey::Close {
            return self.update(Message::HideControls);
        }
        if self.notice.is_some() {
            return match key {
                DrawerKey::Activate => self.update(Message::NoticeRetry),
                _ => Task::none(),
            };
        }
        if self.arrange_recovery_view().is_some() {
            return Task::none();
        }
        let items = self.drawer_items();
        let index = index.min(items.len() - 1);
        match key {
            DrawerKey::Next => self.sidebar.focus = Some((index + 1) % items.len()),
            DrawerKey::Previous => {
                self.sidebar.focus = Some((index + items.len() - 1) % items.len())
            }
            DrawerKey::Activate => {
                return match items[index] {
                    DrawerItem::Widget(widget) => self.update(Message::ToggleWidget(widget)),
                    DrawerItem::Arrange => {
                        self.sidebar.hide();
                        self.update(Message::Arrange)
                    }
                    DrawerItem::Settings => {
                        self.sidebar.hide();
                        self.update(Message::Settings)
                    }
                    DrawerItem::Customize => self.update(Message::EdgeCustomize),
                    DrawerItem::Entry(entry) => self.update(Message::EdgeItem(entry)),
                };
            }
            DrawerKey::MoveUp | DrawerKey::MoveDown => {
                if let DrawerItem::Entry(entry) = items[index] {
                    return self.move_edge_item(entry, key == DrawerKey::MoveUp);
                }
                let DrawerItem::Widget(widget) = items[index] else {
                    return Task::none();
                };
                let mut order = self.config.ordered_right_cards();
                let Some(from) = order.iter().position(|w| *w == widget) else {
                    return Task::none();
                };
                if !self.rail_draggable(widget) {
                    return Task::none();
                }
                let to = if key == DrawerKey::MoveUp {
                    from.checked_sub(1)
                } else {
                    (from + 1 < order.len()).then_some(from + 1)
                };
                let Some(to) = to else {
                    return Task::none();
                };
                order.swap(from, to);
                // The keyboard stays on the widget it moved.
                self.sidebar.focus = Some(to);
                return self.save_widget_order(order);
            }
            DrawerKey::Close => {}
        }
        Task::none()
    }
    fn rail_draggable(&self, widget: Widget) -> bool {
        !self.custom_layout()
            && !(self.config.glance.enabled
                && matches!(widget, Widget::Clock | Widget::Weather | Widget::Calendar))
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
            .chain(self.effect_wake())
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
    /// Reads the system's “enable animations” off the update thread. A
    /// setting that can't be read leaves motion to Halcyon's own switches.
    pub(super) fn read_system_motion() -> Task<Message> {
        cosmic::task::future(async {
            let on = tokio::task::spawn_blocking(|| {
                crate::desktop::DesktopStore::from_env()
                    .ok()
                    .and_then(|store| {
                        crate::desktop::motion::AppAnimations::for_store(&store)
                            .read()
                            .ok()
                    })
                    .unwrap_or(true)
            })
            .await
            .unwrap_or(true);
            Message::SystemMotion(on)
        })
    }
    /// Every motion switch the desktop reads, off.
    pub(super) fn still(cfg: &mut Config) {
        cfg.material_motion = false;
        cfg.settings_animations = false;
        cfg.sidebar_slide = false;
    }
    pub(super) fn apply_config(&mut self, mut cfg: Config) -> Task<Message> {
        if !self.system_animations {
            Self::still(&mut cfg);
        }
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
                    Widget::Glance | Widget::Music | Widget::Focus | Widget::Devices => {}
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
        let old_edge = std::mem::replace(&mut self.config, cfg).edge_menu;
        self.refresh_display_layouts();
        self.reconcile_weather(now);
        if display_changed {
            tasks.push(self.reconcile_output());
        }
        tasks.push(self.wallpaper_colors_changed());
        tasks.push(self.edge_menu_changed(&old_edge));
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
        crate::instance::command_path()
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
            "drawer" => Some(Message::DrawerToggle),
            "rail" => Some(Message::ShowControls),
            "open" => match value["widget"].as_str() {
                None | Some("open") => Some(Message::ShowControls),
                Some(key) => Some(Message::FocusWidget(Widget::parse(key)?)),
            },
            "toggle" => Some(Message::ToggleWidget(Widget::parse(
                value["widget"].as_str()?,
            )?)),
            "focus" => Some(Message::FocusCommand(value["do"].as_str()?.to_string())),
            "timer" => timer_command(&value).map(|command| {
                Message::Activity(crate::activity_island::bridge::Event::Command(command))
            }),
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
    /// One rail row: handle (where dragging works), icon, name and a
    /// switch. Rows sit in a rounded group; `corners` rounds the group's ends.
    fn rail_button(
        &self,
        widget: Widget,
        draggable: bool,
        corners: [f32; 4],
    ) -> Element<'_, Message> {
        let draggable = draggable && self.rail_draggable(widget);
        let focused = self.drawer_focus() == Some(DrawerItem::Widget(widget));
        let p = self.config.surface_palette();
        let selected = if widget.is_tool() {
            self.config.widget_visible(widget)
        } else {
            self.config.data_visible(widget)
        };
        let dragged = self
            .sidebar
            .drag
            .as_ref()
            .is_some_and(|d| d.widget == widget && d.moved);
        let alert = widget == Widget::Calendar && self.meeting_alert();
        let color = if alert {
            mix(Color::from_rgb8(245, 190, 107), p.accent, 0.8)
        } else {
            p.accent
        };
        let handle: Element<'_, Message> = if draggable {
            cosmic::widget::icon::from_name("grip-lines-symbolic")
                .size(12)
                .icon()
                .class(cosmic::theme::Svg::custom(move |_| {
                    cosmic::iced::widget::svg::Style {
                        color: Some(p.muted),
                    }
                }))
                .into()
        } else {
            cosmic::iced::widget::space().width(12.).into()
        };
        let content = row::with_children(vec![
            handle,
            // The name stays readable when off; only the icon rests.
            cosmic::widget::icon::from_name(widget.icon())
                .size(16)
                .icon()
                .class(cosmic::theme::Svg::custom(move |_| {
                    cosmic::iced::widget::svg::Style {
                        color: Some(if selected { p.foreground } else { p.muted }),
                    }
                }))
                .into(),
            container(label(widget.title(), 13., p.foreground))
            .width(Length::Fill)
            .into(),
            switch(selected, color, p),
        ])
        .spacing(8)
        .align_y(Alignment::Center);
        mouse_area(
            container(content)
                .padding([0, 10, 0, 6])
                .width(Length::Fill)
                .height(Length::Fixed(drawer::ROW_HEIGHT))
                .align_y(Alignment::Center)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        icon_color: Some(if selected { p.foreground } else { p.muted }),
                        background: Some(Background::Color(if dragged {
                            mix(color, p.background, 0.24)
                        } else {
                            mix(p.foreground, p.background, 0.07)
                        })),
                        border: Border {
                            color,
                            width: if dragged || focused { 2. } else { 0. },
                            radius: corners.into(),
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
    pub(super) fn control_rail(&self) -> Element<'_, Message> {
        if !self.sidebar.visible && self.sidebar.progress == 0. {
            return text::body("").into();
        }
        if let Some(recovery) = self.arrange_recovery_view() {
            return recovery;
        }
        if let Some(notice) = self.notice_view() {
            return notice;
        }
        if let Some(items) = self.menu_items() {
            return if self.compact() {
                self.compact_rail(&items)
            } else {
                self.custom_rail(&items)
            };
        }
        let p = self.config.surface_palette();
        // Rounded ends for the first and last row of a group.
        let corners = |index: usize, count: usize| -> [f32; 4] {
            let top = if index == 0 { 12. } else { 0. };
            let bottom = if index + 1 == count { 12. } else { 0. };
            [top, top, bottom, bottom]
        };
        let right = self.config.ordered_right_cards();
        let count = right.len();
        let rows = keyed_column(
            right
                .into_iter()
                .enumerate()
                .map(|(index, widget)| (widget, self.rail_button(widget, true, corners(index, count)))),
        )
        .spacing(drawer::ROW_STEP - drawer::ROW_HEIGHT);
        let elsewhere = ELSEWHERE;
        let others = column::with_children(
            elsewhere
                .into_iter()
                .enumerate()
                .map(|(index, widget)| {
                    self.rail_button(widget, false, corners(index, elsewhere.len()))
                })
                .collect::<Vec<_>>(),
        )
        .spacing(drawer::ROW_STEP - drawer::ROW_HEIGHT);
        let heading = |title: &'static str| -> Element<'_, Message> {
            container(
                text::body(title)
                    .size(12.)
                    .font(cosmic::font::semibold())
                    .class(cosmic::theme::Text::Color(mix(p.foreground, p.muted, 0.5))),
            )
                .padding([0, 4])
                .height(Length::Fixed(drawer::SECTION_LABEL))
                .align_y(Alignment::End)
                .into()
        };
        let gap = |height: f32| -> Element<'_, Message> {
            cosmic::iced::widget::space().height(height).into()
        };
        let header = container(
            column::with_children(vec![
                label("Widgets", 15., p.foreground),
                label("Show or hide each widget", 12., p.muted),
            ])
            .spacing(2),
        )
        .padding([0, 4])
        .height(Length::Fixed(drawer::HEADER));
        let focus = self.drawer_focus();
        let action = |icon: &'static str, title: &'static str, item: DrawerItem, message| {
            let focused = focus == Some(item);
            mouse_area(
                container(
                    row::with_children(vec![
                        cosmic::widget::icon::from_name(icon).size(16).icon().into(),
                        label(title, 13., p.foreground),
                    ])
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .height(Length::Fixed(34.))
                .padding([0, 10])
                .align_y(Alignment::Center)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        icon_color: Some(p.foreground),
                        border: Border {
                            color: if focused {
                                p.accent
                            } else {
                                mix(p.foreground, p.background, 0.16)
                            },
                            width: if focused { 2. } else { 1. },
                            radius: 12.into(),
                        },
                        ..Default::default()
                    }
                })),
            )
            .on_press(message)
            .interaction(mouse::Interaction::Pointer)
        };
        let settings = row::with_children(vec![
            action(
                "view-grid-symbolic",
                "Arrange",
                DrawerItem::Arrange,
                Message::Arrange,
            )
            .into(),
            action(
                "preferences-system-symbolic",
                "Settings",
                DrawerItem::Settings,
                Message::Settings,
            )
            .into(),
        ])
        .spacing(8);
        self.rail_frame(
            column::with_children(vec![
                header.into(),
                gap(drawer::HEADER_GAP),
                heading("Right column"),
                gap(drawer::LABEL_GAP),
                rows.into(),
                gap(6.),
                container(label(
                    if self.custom_layout() {
                        "Your own layout. Move widgets in Settings."
                    } else {
                        "Drag to reorder"
                    },
                    12.,
                    p.muted,
                ))
                .padding([0, 4])
                .into(),
                gap(14.),
                heading("Other widgets"),
                gap(drawer::LABEL_GAP),
                others.into(),
                gap(12.),
                settings.into(),
                gap(8.),
                self.customize_link(),
            ])
            .into(),
        )
    }
    /// A failed desktop change, in the place Arrange explains its own.
    pub(super) fn notice_view(&self) -> Option<Element<'_, Message>> {
        let notice = self.notice.as_ref()?;
        let mut content = vec![
            text::heading(notice.title).into(),
            text::body(notice.body).into(),
        ];
        let mut buttons = Vec::new();
        if notice.retry.is_some() {
            buttons.push(
                cosmic::widget::button::suggested("Try again")
                    .id("notice-retry".into())
                    .on_press(Message::NoticeRetry)
                    .into(),
            );
        }
        buttons.push(
            cosmic::widget::button::standard("Dismiss")
                .id("notice-dismiss".into())
                .on_press(Message::NoticeDismiss)
                .into(),
        );
        content.push(row::with_children(buttons).spacing(8).into());
        content.push(
            cosmic::widget::button::link(if notice.details {
                "Hide details"
            } else {
                "Details"
            })
            .id("notice-details".into())
            .on_press(Message::NoticeDetails)
            .into(),
        );
        if notice.details {
            content.push(
                cosmic::widget::scrollable(text::caption(notice.error.as_str()))
                    .height(Length::Fill)
                    .into(),
            );
        }
        Some(
            container(column::with_children(content).spacing(10))
                .padding(12)
                .width(Length::Fill)
                .height(Length::Fill)
                .class(cosmic::theme::Container::Card)
                .into(),
        )
    }
    /// Strip `id` on the screen edge: clear, or the visible handle.
    pub(super) fn edge_handle(&self, id: window::Id) -> Element<'_, Message> {
        mouse_area(super::edge_compact::handle(&self.config))
            .on_enter(Message::EdgeHover(id, true))
            .on_exit(Message::EdgeHover(id, false))
            .on_press(Message::EdgeClick(id))
            .interaction(mouse::Interaction::Pointer)
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
            widgets.push(self.with_menu(Widget::Calendar, self.calendar_card()));
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
            (widget, self.with_menu(widget, view))
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
        if self.is_edge_strip(id) {
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
                let x = self.rail_x();
                let inside = (x..=x + self.rail_width() as f32).contains(&position.x)
                    && (0. ..=self.controls_height() as f32).contains(&position.y);
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
                                self.raise_notice(
                                    "Couldn’t save the widget order",
                                    "The widgets are back in their saved order.",
                                    error.to_string(),
                                    Some(Message::SaveOrder(order)),
                                );
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

/// A small on or off switch, drawn to match the system switches.
pub(super) fn switch<'a>(
    on: bool,
    color: Color,
    p: crate::config::Palette,
) -> Element<'a, Message> {
    let knob = container(cosmic::iced::widget::space().width(10.).height(10.)).class(
        cosmic::theme::Container::custom(move |_| cosmic::iced::widget::container::Style {
            background: Some(Background::Color(if on {
                p.background
            } else {
                mix(p.foreground, p.background, 0.8)
            })),
            border: Border {
                radius: 5.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }),
    );
    container(knob)
        .width(26.)
        .height(16.)
        .padding(3)
        .align_x(if on { Alignment::End } else { Alignment::Start })
        .align_y(Alignment::Center)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(if on {
                    color
                } else {
                    mix(p.foreground, p.background, 0.14)
                })),
                border: Border {
                    radius: 8.0.into(),
                    width: if on { 0. } else { 1. },
                    color: mix(p.foreground, p.background, 0.55),
                },
                ..Default::default()
            }
        }))
        .into()
}

/// `halcyon-ctl timer <minutes>`, `timer pause|resume|cancel`.
fn timer_command(value: &serde_json::Value) -> Option<crate::activity_island::bridge::Command> {
    use crate::activity_island::bridge::Command;
    Some(match value["do"].as_str() {
        Some("cancel") => Command::TimerCancel,
        Some("pause") => Command::TimerPause,
        Some("resume") => Command::TimerResume,
        Some(_) => return None,
        None => {
            let minutes = value["minutes"]
                .as_f64()
                .filter(|m| *m > 0. && *m <= 1440.)?;
            Command::TimerStart {
                ms: (minutes * 60_000.).round() as i64,
            }
        }
    })
}

#[cfg(test)]
mod timer_command_tests {
    use super::timer_command;
    use crate::activity_island::bridge::Command;
    use serde_json::json;

    #[test]
    fn timer_commands_start_in_minutes_and_reject_odd_values() {
        assert!(matches!(
            timer_command(&json!({"action": "timer", "minutes": 5})),
            Some(Command::TimerStart { ms: 300_000 })
        ));
        assert!(matches!(
            timer_command(&json!({"action": "timer", "minutes": 2.5})),
            Some(Command::TimerStart { ms: 150_000 })
        ));
        assert!(matches!(
            timer_command(&json!({"action": "timer", "do": "cancel"})),
            Some(Command::TimerCancel)
        ));
        assert!(matches!(
            timer_command(&json!({"action": "timer", "do": "pause"})),
            Some(Command::TimerPause)
        ));
        assert!(matches!(
            timer_command(&json!({"action": "timer", "do": "resume"})),
            Some(Command::TimerResume)
        ));
        for odd in [
            json!({"action": "timer", "minutes": 0}),
            json!({"action": "timer", "minutes": -1}),
            json!({"action": "timer", "minutes": 1441}),
            json!({"action": "timer", "minutes": "5"}),
            json!({"action": "timer", "do": "explode"}),
            json!({"action": "timer"}),
        ] {
            assert!(timer_command(&odd).is_none(), "{odd}");
        }
    }
}
