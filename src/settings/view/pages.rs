use super::*;
use cosmic::Apply;

/// Transient App settings navigation, independent of saved configuration.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppSettingsPage {
    /// The stable Settings landing.
    #[default]
    Home,
    Operation,
    Interface,
    Privacy,
    Updates,
    /// This version's release notes, opened from Updates.
    WhatsNew,
    Help,
}

impl AppSettingsPage {
    pub const ALL: [Self; 5] = [
        Self::Operation,
        Self::Interface,
        Self::Privacy,
        Self::Updates,
        Self::Help,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Home => "Halcyon",
            Self::Operation => "Startup",
            Self::Interface => "Interface",
            Self::Privacy => "Privacy",
            Self::Updates => "Updates",
            Self::WhatsNew => "What’s new",
            Self::Help => "Help",
        }
    }

    pub fn tab_title(self) -> &'static str {
        match self {
            Self::Operation => "Startup",
            Self::Updates => "Updates",
            Self::Help => "Help",
            page => page.title(),
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Operation => "operation",
            Self::Interface => "interface",
            Self::Privacy => "privacy",
            Self::Updates => "updates",
            Self::WhatsNew => "whats-new",
            Self::Help => "help",
        }
    }
}

pub(super) fn section<'a>(items: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    frame(
        column::with_children(items)
            .spacing(10)
            .width(Length::Fill)
            .into(),
        SURFACE,
        LINE,
        14,
    )
}

fn widget_page(widget: Widget) -> Page {
    match widget {
        Widget::Calendar => Page::Calendar,
        Widget::Clock => Page::Clock,
        Widget::Weather => Page::Weather,
        Widget::Devbox => Page::Devbox,
        Widget::Projects => Page::Projects,
        Widget::Glance => Page::Glance,
        Widget::Music => Page::Music,
        Widget::Notes => Page::Notes,
        Widget::Focus => Page::Focus,
    }
}

impl Settings {
    pub(super) fn widget_heading(
        &self,
        widget: Widget,
        title: &'static str,
    ) -> Element<'_, Message> {
        let combined = self.config.glance.enabled
            && matches!(widget, Widget::Clock | Widget::Weather | Widget::Calendar);
        // Every widget page says in one line what the widget is for.
        let blurb = match widget {
            Widget::Clock => "The time and date on your desktop.",
            Widget::Weather => "Today’s weather where you are.",
            Widget::Calendar => "Your next events at a glance.",
            Widget::Glance => "Clock, weather and calendar in one card.",
            Widget::Music => "What’s playing, with play and pause.",
            Widget::Notes => "A note that stays on your desktop.",
            Widget::Focus => "A timer for focused work and breaks.",
            Widget::Projects => "For people who write code: what changed in each project.",
            Widget::Devbox => "How busy your computer is.",
        };
        let header = split(
            heading(title, blurb),
            row::with_children(vec![
                label(
                    if !self.config.widgets_visible {
                        "All widgets are hidden"
                    } else if combined {
                        "Part of Daily overview"
                    } else {
                        "Show on desktop"
                    },
                    12.,
                    MUTED,
                ),
                toggler(self.config.widget_selected(widget))
                    .on_toggle(move |visible| Message::Widget(widget, visible))
                    .into(),
            ])
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        );
        header
    }

    /// Selection is independent of the desktop's global visibility. Daily overview
    /// owns its three section toggles; standalone widgets keep their own settings.
    pub(super) fn widgets_page(&self) -> Element<'_, Message> {
        let group = |title: &'static str, widgets: &[Widget]| -> Element<'_, Message> {
            cosmic::widget::settings::section()
                .title(title)
                .extend(widgets.iter().map(|&widget| self.widget_list_row(widget)))
                .into()
        };
        // With Daily overview on, the trio sits indented under it as its
        // parts; otherwise they are ordinary rows like every other widget.
        let trio = [Widget::Clock, Widget::Weather, Widget::Calendar];
        let daily = if self.config.glance.enabled {
            cosmic::widget::settings::section()
                .title("Daily overview")
                .add(self.widget_list_row(Widget::Glance))
                .add(
                    container(
                        column::with_children(
                            trio.into_iter()
                                .map(|w| self.widget_list_row(w))
                                .collect::<Vec<_>>(),
                        )
                        .spacing(8),
                    )
                    .padding([0, 0, 0, 24]),
                )
                .into()
        } else {
            group("Daily", &[Widget::Glance, Widget::Clock, Widget::Weather, Widget::Calendar])
        };
        // One column of widgets: the daily trio, then everything else.
        let more = group(
            "More widgets",
            &[
                Widget::Devbox,
                Widget::Projects,
                Widget::Music,
                Widget::Notes,
                Widget::Focus,
            ],
        );
        let visibility = super::group(
            None,
            vec![split(
                if self.config.widgets_visible {
                    detail("Show widgets", "Turn off to hide every widget at once")
                } else {
                    detail_state("Show widgets", "Widgets are hidden; your choices are kept")
                },
                container(toggler(self.config.widgets_visible).on_toggle(Message::Desktop))
                    .id(cosmic::widget::Id::new("widgets-show-all"))
                    .into(),
            )],
        );
        // How widgets behave on the desktop lives with the widgets; Halcyon’s
        // Interface tab is about this window only.
        let behave = container(super::group(
            Some("How widgets behave"),
            vec![
                group_row(
                    "Widget controls",
                    "A side panel to show, hide and reorder widgets. Hold the pointer at the right edge for a second, or use Open now.",
                    button::standard("Open now")
                        .on_press_maybe(
                            (self.service_known && self.service.running && !self.busy && !preview())
                                .then_some(Message::ShowControls),
                        )
                        .into(),
                ),
                switch(
                    "Slide in",
                    "Animate the controls as they open",
                    self.config.sidebar_slide,
                    Message::SidebarSlide,
                ),
            ],
        ))
        .id(cosmic::widget::Id::new("widgets-behave"))
        .into();
        container(column::with_children(vec![visibility, daily, more, behave]).spacing(20))
            .id(cosmic::widget::Id::new("desktop-widgets"))
            .into()
    }
    fn widget_list_row(&self, widget: Widget) -> Element<'_, Message> {
        let on = self.config.widget_selected(widget);
        let inside = self.config.glance.enabled
            && matches!(widget, Widget::Clock | Widget::Weather | Widget::Calendar);
        let readiness = if widget == Widget::Notes && self.tools.unsaved() {
            "Unsaved note"
        } else if widget == Widget::Calendar && !self.calendar.connection.connected() {
            "Connect your calendar"
        } else if widget == Widget::Weather && self.config.location.is_none() {
            "Choose a city"
        } else if widget == Widget::Projects && self.config.project_roots.is_empty() {
            "Choose folders"
        } else {
            ""
        };
        let status = if widget == Widget::Glance {
            if self.config.glance.enabled {
                "One card instead of separate Clock, Weather and Calendar".to_string()
            } else {
                "Put Clock, Weather and Calendar in one card".to_string()
            }
        } else if inside {
            if readiness.is_empty() {
                "Part of Daily overview".to_string()
            } else {
                format!("Part of Daily overview. {readiness}")
            }
        } else {
            readiness.to_string()
        };
        row::with_children(vec![
            button::custom(
                row::with_children(vec![
                    icon::from_name(widget.icon()).size(20).icon().into(),
                    if widget == Widget::Glance {
                        detail(widget.title(), status)
                    } else {
                        detail_state(widget.title(), status)
                    },
                    icon::from_name("go-next-symbolic").size(16).icon().into(),
                ])
                .spacing(14)
                .align_y(Alignment::Center),
            )
            .id(cosmic::widget::Id::new(format!(
                "widget-open-{}",
                widget.key()
            )))
            .class(nav_style(false, self.settings_accent()))
            .padding([4, 0])
            .width(Length::Fill)
            .on_press(Message::Page(widget_page(widget)))
            .into(),
            container(toggler(on).on_toggle(move |value| Message::Widget(widget, value)))
                .id(cosmic::widget::Id::new(format!(
                    "widget-toggle-{}",
                    widget.key()
                )))
                .into(),
        ])
        .spacing(18)
        .align_y(Alignment::Center)
        .into()
    }

    pub(super) fn calendar_page(&self) -> Element<'_, Message> {
        let notice = row::with_children(vec![
            slider(1u16..=60, self.notice, Message::Notice)
                .name("Highlight meetings")
                .on_release(Message::SaveNotice)
                .step(1u16)
                .width(Length::Fill)
                .apply(|control| completed_slider(control.into(), Message::SaveNotice)),
            container(label(
                format!("{} min", self.notice),
                13.,
                self.settings_accent(),
            ))
            .width(52)
            .align_right(52)
            .into(),
        ])
        .spacing(12)
        .align_y(Alignment::Center);
        let mut events = if self.config.glance.enabled {
            vec![
                split(detail("Tomorrow", "Today is always shown"),
                    container(toggler(self.config.glance.show_tomorrow).on_toggle(Message::GlanceTomorrow))
                        .id(cosmic::widget::Id::new("glance-tomorrow")).into()),
            ]
        } else {
            vec![
                switch("Today", "", self.config.calendar_show_today, Message::Today),
                switch(
                    "Tomorrow",
                    "",
                    self.config.calendar_show_tomorrow,
                    Message::Tomorrow,
                ),
            ]
        };
        events.push(switch(
            "Hide event details",
            "Show Busy instead of titles",
            self.config.calendar_private,
            Message::CalendarPrivacy,
        ));
        events.push(
            column::with_children(vec![
                detail("Highlight meetings", "Starts this many minutes before each one"),
                notice.into(),
            ])
            .spacing(6)
            .into(),
        );
        let connected = self.calendar.connection.connected();
        let mut sections = vec![self.calendar_connection()];
        if connected {
            sections.push(group(Some("Agenda"), events));
        }
        sections.push(self.page_settings_advanced(
            Page::Calendar,
            vec![
                self.refresh_control(Widget::Calendar),
                self.calendar_publisher_setup(),
            ],
        ));
        let preview = Some(super::sample::fit(
            self.samples.calendar_view().map(|_| Message::Preview),
            250.,
        ));
        self.widget_template(Widget::Calendar, "Calendar", preview, sections)
    }

    pub(super) fn weather_page(&self) -> Element<'_, Message> {
        let location = self
            .config
            .location
            .as_ref()
            .map(|loc| loc.name.as_str())
            .unwrap_or("Your city");
        let state = &self.city_search;
        let editing = self.city_editing || self.config.location.is_none();
        // With no city yet the search box is the whole row; the city
        // already chosen for time of day is offered beside it.
        let mut location_controls = Vec::new();
        if self.config.location.is_some() {
            location_controls.push(split(
                label(location, 14., FG),
                button::text(if editing { "Cancel" } else { "Change" })
                    .on_press(Message::EditCity(!editing))
                    .into(),
            ));
        }
        if editing {
            let ready = state.query.trim().chars().count() >= 2 && !state.searching;
            let search = text_input("Search for your city", &state.query)
                .id(cosmic::widget::Id::new("weather-city"))
                .on_input(Message::CityInput)
                .on_submit(|_| Message::SearchCity)
                .width(Length::Fill);
            let mut search_row = vec![search.into()];
            if !state.query.is_empty() {
                search_row.push(
                    button::icon(icon::from_name("edit-clear-symbolic"))
                        .on_press(Message::ClearCitySearch)
                        .apply(|control| super::tip::tip(control, "Clear search")),
                );
            }
            if self.config.location.is_none()
                && state.query.is_empty()
                && let Ok(city) = self
                    .automation
                    .solar
                    .city(crate::automation::solar::Source::Manual)
            {
                search_row.push(
                    button::standard(format!("Use {}", city.name))
                        .id(cosmic::widget::Id::new("weather-use-schedule-city"))
                        .on_press(Message::UseScheduleCity)
                        .into(),
                );
            }
            // A Search button, as on the time-of-day page; Retry after an error.
            {
                search_row.push(
                    button::standard(if state.error.is_some() { "Retry" } else { "Search" })
                        .on_press_maybe(ready.then_some(Message::SearchCity))
                        .into(),
                );
            }
            location_controls.push(
                row::with_children(search_row)
                    .spacing(6)
                    .align_y(Alignment::Center)
                    .into(),
            );
            if state.searching || state.deadline.is_some() {
                location_controls.push(label("Searching cities…", 12., MUTED));
            } else if let Some(error) = &state.error {
                location_controls.push(label(error, 12., Color::from_rgb8(245, 149, 138)));
            } else if !self.cities.is_empty() {
                for (index, city) in self.cities.iter().enumerate() {
                    let selected = self.config.location.as_ref().is_some_and(|location| {
                        location.latitude == city.latitude && location.longitude == city.longitude
                    });
                    location_controls.push(
                        button::custom(split(
                            detail_state(city.name.clone(), city.region()),
                            label(
                                if selected { "Current" } else { "Choose →" },
                                12.,
                                self.settings_accent(),
                            ),
                        ))
                        .class(nav_style(selected, self.settings_accent()))
                        .padding([8, 4])
                        .width(Length::Fill)
                        .on_press(Message::ChooseCity(index))
                        .into(),
                    );
                }
            } else if state.searched {
                location_controls.push(detail_state(
                    "No cities found",
                    "Try another spelling or a nearby town.",
                ));
            }
        }
        let units = self.config.weather.fahrenheit;
        location_controls.push(split(
            label("Units", 14., FG),
            container(segmented(
                vec![
                    ("°C · km/h".into(), !units, Some(Message::WeatherFahrenheit(false))),
                    ("°F · mph".into(), units, Some(Message::WeatherFahrenheit(true))),
                ],
                self.settings_accent(),
            ))
            .id(cosmic::widget::Id::new("weather-units"))
            .into(),
        ));
        let controls = vec![
            self.search_target("weather-location", group(Some("Location"), location_controls)),
            self.page_settings_advanced(
                Page::Weather,
                vec![
                    self.search_target(
                        "weather-dew-point",
                        split(
                            detail("Dew point", "Shows how humid it feels"),
                            container(
                                toggler(self.config.weather.show_dew_point)
                                    .on_toggle(Message::WeatherDewPoint),
                            )
                            .id(cosmic::widget::Id::new("weather-dew-point-toggle"))
                            .into(),
                        ),
                    ),
                    self.refresh_control(Widget::Weather),
                    button::text("Weather data from Open-Meteo ↗")
                        .on_press(Message::OpenWeatherSource)
                        .into(),
                ],
            ),
        ];
        let preview = crate::app::settings_weather_preview(&self.config).map(|_| Message::Preview);
        let preview = Some(super::sample::fit(preview, 220.));
        // The preview's conditions are made up; say so, and before a city is
        // chosen say how to see real ones.
        let note = if self.config.location.is_none() {
            "Example weather. Choose your city below to see yours."
        } else {
            "Example weather"
        };
        let mut controls = controls;
        controls.insert(
            0,
            container(label(note, 12., MUTED))
                .id(cosmic::widget::Id::new("weather-preview-note"))
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .into(),
        );
        self.widget_template(Widget::Weather, "Weather", preview, controls)
    }

    pub(super) fn projects_page(&self) -> Element<'_, Message> {
        let mut roots = Vec::new();
        for root in &self.config.project_roots {
            let name = PathBuf::from(root)
                .file_name()
                .and_then(|part| part.to_str())
                .unwrap_or(root)
                .to_string();
            roots.push(split(
                row::with_children(vec![
                    icon::from_name("folder-symbolic").size(22).icon().into(),
                    detail_state(name, root.clone()),
                ])
                .spacing(12)
                .align_y(Alignment::Center)
                .into(),
                button::text("Remove")
                    .on_press(Message::RemoveFolder(root.clone()))
                    .into(),
            ));
        }
        // Typing a path is the fallback when the folder chooser can't open.
        if self.folder_typing || !self.folder_input.is_empty() {
        roots.push(
            row::with_children(vec![
                text_input("Folder path, for example ~/Projects", &self.folder_input)
                    .id(cosmic::widget::Id::new("project-folder-input"))
                    .on_input(Message::FolderInput)
                    .on_submit(|_| Message::AddFolder)
                    .width(Length::Fill)
                    .into(),
                button::standard("Add")
                    .on_press_maybe(
                        (!self.folder_input.trim().is_empty()).then_some(Message::AddFolder),
                    )
                    .into(),
            ])
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),
        );
        }
        roots.push(split(
            detail(
                "Add a folder",
                if self.config.project_roots.is_empty() {
                    "Pick the folder that holds your code to see what changed in each project"
                } else {
                    "Each project inside it shows on the card"
                },
            ),
            button::standard(if self.folder_pending {
                "Choosing folder…"
            } else {
                "Choose…"
            })
            .id(cosmic::widget::Id::new("project-choose-folders"))
            .on_press_maybe((!self.folder_pending).then_some(Message::BrowseFolder))
            .into(),
        ));
        let preview = || {
            super::sample::fit(
                crate::app::settings_projects_preview(&self.config).map(|_| Message::Preview),
                200.,
            )
        };
        self.widget_template(
            Widget::Projects,
            "Projects",
            Some(preview()),
            vec![
                group(Some("Project folders"), roots),
                self.page_settings_advanced(
                    Page::Projects,
                    vec![self.refresh_control(Widget::Projects)],
                ),
            ],
        )
    }

    pub(super) fn refresh_control(&self, widget: Widget) -> Element<'_, Message> {
        let current = self.config.refresh_intervals.seconds(widget);
        let default = RefreshIntervals::default().seconds(widget);
        let p = self.config.palette();
        let mut choices = RefreshIntervals::choices(widget).to_vec();
        if !choices.contains(&current) {
            choices.push(current);
            choices.sort_unstable();
        }
        let buttons = segmented(
            choices
                .into_iter()
                .map(|seconds| {
                    (
                        RefreshIntervals::label(seconds),
                        seconds == current,
                        Some(Message::RefreshInterval(widget, seconds)),
                    )
                })
                .collect(),
            p.accent,
        );
        let reset: Element<'_, Message> = if current == default {
            pill("Default", MUTED)
        } else {
            button::text(format!("Reset · {}", RefreshIntervals::label(default)))
                .on_press(Message::RefreshInterval(widget, default))
                .into()
        };
        group(
            None,
            vec![
                split(
                    detail("Update every", "Updates pause while the widget is hidden"),
                    reset,
                ),
                buttons,
                if self.config.data_visible(widget) {
                    cosmic::iced::widget::space().height(0).into()
                } else {
                    label("Paused while hidden", CAPTION, MUTED)
                },
            ],
        )
    }

    pub(super) fn devbox_page(&self) -> Element<'_, Message> {
        let preview =
            || crate::app::settings_devbox_preview(&self.config).map(|_| Message::Preview);
        if let Some(recipe) = &self.config.recipes.devbox {
            return self.widget_template(
                Widget::Devbox,
                "System monitor",
                Some(super::sample::fit(preview(), 220.)),
                vec![
                    group(
                        Some("Your own design"),
                        vec![group_row(
                            recipe.name.clone(),
                            "Choose and order readings in the widget builder",
                            button::text("Edit widget…")
                                .id(cosmic::widget::Id::new("system-recipe-edit"))
                                .on_press(Message::OpenRecipe(crate::recipes::Kind::System))
                                .into(),
                        )],
                    ),
                    self.page_settings_advanced(
                        Page::Devbox,
                        vec![self.refresh_control(Widget::Devbox)],
                    ),
                ],
            );
        }
        let order = self.config.devbox.ordered_sections();
        let mut rows = Vec::new();
        for (index, metric) in order.iter().copied().enumerate() {
            let mut reading = vec![self.devbox_metric_row(metric, index, order.len())];
            let visible = metric.enabled(&self.config.devbox);
            let searched = self.search_selected.is_some_and(|hit| {
                hit.open == crate::settings::navigation::SearchOpen::Metric(metric)
            });
            {
                let open = (visible || searched)
                    && self.devbox_details.contains(&metric)
                    && !self.devbox_reordering;
                let d = &self.config.devbox;
                let mut details = Vec::new();
                if !visible {
                    details.push(label(
                        "Turn on this reading to change its details.",
                        11.,
                        MUTED,
                    ));
                }
                if metric == DevboxSection::Memory {
                    details.push(split(
                        label("Show cache", 12., FG),
                        toggler(d.memory_cache)
                            .on_toggle_maybe(
                                visible.then_some(|value| Message::Devbox("memory_cache", value)),
                            )
                            .into(),
                    ));
                    details.push(split(
                        label("Show swap", 12., FG),
                        toggler(d.swap)
                            .on_toggle_maybe(
                                visible.then_some(|value| Message::Devbox("swap", value)),
                            )
                            .into(),
                    ));
                }
                let history = match metric {
                    DevboxSection::Cpu => Some(d.history.cpu),
                    DevboxSection::Memory => Some(d.history.memory),
                    DevboxSection::Disk => Some(d.history.disk),
                    DevboxSection::Network => Some(d.history.network),
                    DevboxSection::Gpu => Some(d.history.gpu),
                    _ => None,
                };
                if let Some(on) = history {
                    details.push(split(
                        label("History graph", 12., FG),
                        toggler(on)
                            .on_toggle_maybe(visible.then_some(move |value| {
                                Message::DevboxHistory(metric.key(), value)
                            }))
                            .into(),
                    ));
                }
                if metric == DevboxSection::Gpu {
                    details.push(label(
                        "Available readings depend on your GPU and driver.",
                        11.,
                        MUTED,
                    ));
                }
                if !details.is_empty() {
                    reading.push(super::reveal::reveal(
                        container(column::with_children(details).spacing(7))
                            .padding([2, 6, 10, 30])
                            .into(),
                        open,
                        self.config.settings_animations && self.system_animations,
                    ));
                }
            }
            rows.push(self.search_target(
                &format!("devbox-{}", metric.key()),
                column::with_children(reading).into(),
            ));
        }
        // One list reads top to bottom; two columns made the order unclear.
        let readings = group(None, rows);
        // The list's own header: its name, with Reorder on the right.
        let summary = split(
            text::heading("Readings").into(),
            container(
                button::text(if self.devbox_reordering { "Done" } else { "Reorder" })
                    .id(cosmic::widget::Id::new("devbox-reorder"))
                    .on_press(Message::DevboxReorder(!self.devbox_reordering)),
            )
            .id(cosmic::widget::Id::new("devbox-reorder-anchor"))
            .into(),
        );
        let readings = column::with_children(vec![summary, readings]).spacing(6).into();
        self.widget_template(
            Widget::Devbox,
            "System monitor",
            Some(super::sample::fit(preview(), 220.)),
            vec![
                readings,
                self.page_settings_advanced(
                    Page::Devbox,
                    vec![self.refresh_control(Widget::Devbox)],
                ),
            ],
        )
    }

    fn devbox_metric_row(
        &self,
        section: DevboxSection,
        index: usize,
        order_len: usize,
    ) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let visible = section.enabled(&self.config.devbox);
        let dragging = self.devbox_reorder.source == Some(section);
        let target = self.devbox_reorder.source.is_some()
            && self.devbox_reorder.target == Some(section)
            && !dragging;
        let idle = self.devbox_reorder.source.is_none();
        let has_details = !matches!(section, DevboxSection::Uptime | DevboxSection::Services);
        let name: Element<'_, Message> = if self.devbox_reordering {
            mouse_area(
                container(
                    row::with_children(vec![
                        icon::from_name("view-more-symbolic").size(16).icon().into(),
                        label(section.title(), 14., if visible { FG } else { MUTED }),
                    ])
                    .spacing(10),
                )
                .width(Length::Fill)
                .height(40)
                .align_y(Alignment::Center),
            )
            .on_press(Message::DevboxDragStart(section))
            .interaction(cosmic::iced::mouse::Interaction::Grab)
            .into()
        } else if has_details {
            button::custom(
                row::with_children(vec![
                    container(label(
                        section.title(),
                        14.,
                        if visible { FG } else { MUTED },
                    ))
                    .width(Length::Fill)
                    .into(),
                    // Opens in place, so it uses the fold-away arrows.
                    icon::from_name(if self.devbox_details.contains(&section) {
                        "go-up-symbolic"
                    } else {
                        "go-down-symbolic"
                    })
                    .size(14)
                    .icon()
                    .into(),
                ])
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .id(cosmic::widget::Id::new(format!(
                "devbox-details-{}",
                section.key()
            )))
            .class(nav_style(false, accent))
            .padding([6, 0])
            .width(Length::Fill)
            .on_press(Message::DevboxDetails(section))
            .into()
        } else {
            // Same padding and chevron space as rows with details, so every
            // switch lines up.
            container(
                row::with_children(vec![
                    container(if section == DevboxSection::Services {
                        detail(section.title(), "Apps running quietly in the background")
                    } else {
                        label(section.title(), 14., if visible { FG } else { MUTED })
                    })
                    .width(Length::Fill)
                    .into(),
                    cosmic::iced::widget::space().width(14).into(),
                ])
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([6, 0])
            .width(Length::Fill)
            .into()
        };
        let mut parts = vec![name];
        if self.devbox_reordering {
            if target || dragging {
                parts.push(label(
                    if target { "Drop here" } else { "Moving" },
                    11.,
                    accent,
                ));
            }
            for (direction, title, enabled) in [
                (-1, "go-up-symbolic", index > 0),
                (1, "go-down-symbolic", index + 1 < order_len),
            ] {
                parts.push(
                    button::icon(icon::from_name(title).size(14))
                        .extra_small()
                        .id(cosmic::widget::Id::new(format!(
                            "devbox-move-{}-{direction}",
                            section.key()
                        )))
                        .on_press_maybe(
                            (enabled && idle).then_some(Message::DevboxMove(section, direction)),
                        )
                        .apply(|control| {
                            super::tip::tip(
                                control,
                                if direction < 0 {
                                    "Move up"
                                } else {
                                    "Move down"
                                },
                            )
                        }),
                );
            }
        }
        parts.push(
            toggler(visible)
                .on_toggle(move |value| Message::Devbox(section.key(), value))
                .into(),
        );
        // Same gap before the switch as the Widgets list.
        let content = container(
            row::with_children(parts)
                .spacing(if self.devbox_reordering { 4 } else { 18 })
                .align_y(Alignment::Center),
        )
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(Color {
                    a: if target {
                        0.12
                    } else if dragging {
                        0.06
                    } else {
                        0.
                    },
                    ..accent
                })),
                border: Border {
                    color: if target { accent } else { Color::TRANSPARENT },
                    width: 1.,
                    radius: 12.into(),
                },
                ..Default::default()
            }
        }));
        mouse_area(content)
            .on_enter(Message::DevboxDragOver(section))
            .on_move(move |_| Message::DevboxDragOver(section))
            .on_exit(Message::DevboxDragLeave(section))
            .into()
    }

    pub(super) fn glance_page(&self) -> Element<'_, Message> {
        let g = &self.config.glance;
        let shown = |widget, selected| {
            if g.initialized {
                selected
            } else {
                self.config.widget_enabled(widget)
            }
        };
        let show_calendar = shown(Widget::Calendar, g.show_calendar);
        let accent = self.settings_accent();
        // With a widget style applied, this widget can still keep its own
        // background, such as transparent over the wallpaper.
        let family = self.config.appearance.family;
        let own = family.is_none() || g.own_background;
        let mut choices: Vec<(String, bool, Option<Message>)> = Vec::new();
        if let Some(f) = family {
            choices.push((
                format!("Like {}", f.title()),
                !own,
                Some(Message::GlanceMatchWidgets),
            ));
        }
        for (title, style) in [
            ("Transparent", GlanceStyle::Transparent),
            ("Card", GlanceStyle::Panel),
        ] {
            choices.push((
                title.into(),
                own && g.style == style,
                Some(Message::GlanceStyle(style)),
            ));
        }
        // With a widget style applied, one switch follows it and hides the
        // card's own background and fonts.
        let follows = family.is_some()
            && !self.glance_custom
            && crate::settings::profiles::LookStyle::of(&self.config).glance
                == self.profiles.applied_style().glance;
        let mut appearance = Vec::new();
        if family.is_some() {
            appearance.push(split(
                detail("Match the desktop style", "Background and fonts come from the look"),
                container(toggler(follows).on_toggle(Message::GlanceFollowStyle))
                    .id(cosmic::widget::Id::new("glance-follow-style"))
                    .into(),
            ));
        }
        if !follows {
            appearance.push(split(
                label("Background", 14., FG),
                container(segmented(choices, accent))
                    .id(cosmic::widget::Id::new("glance-background"))
                    .into(),
            ));
        }
        if family.is_none() && g.style == GlanceStyle::Panel {
            let mut choices = vec![50, 70, 85, 100];
            if !choices.contains(&g.opacity) {
                choices.push(g.opacity);
                choices.sort_unstable();
            }
            appearance.push(split(
                label("Opacity", 14., FG),
                segmented(
                    choices
                        .into_iter()
                        .map(|opacity| {
                            (
                                format!("{opacity}%"),
                                opacity == g.opacity,
                                Some(Message::GlanceOpacity(opacity)),
                            )
                        })
                        .collect(),
                    accent,
                ),
            ));
        }
        appearance.push(split(
            label("Spacing", 14., FG),
            segmented(
                [
                    ("Compact", GlanceDensity::Compact),
                    ("Comfortable", GlanceDensity::Comfortable),
                ]
                .into_iter()
                .map(|(title, density)| {
                    (
                        title.into(),
                        g.density == density,
                        Some(Message::GlanceDensity(density)),
                    )
                })
                .collect(),
                accent,
            ),
        ));
        let mut sections = Vec::new();
        for (widget, page, key, on) in [
            (
                Widget::Clock,
                Page::Clock,
                "show_clock",
                shown(Widget::Clock, g.show_clock),
            ),
            (
                Widget::Weather,
                Page::Weather,
                "show_weather",
                shown(Widget::Weather, g.show_weather),
            ),
            (
                Widget::Calendar,
                Page::Calendar,
                "show_calendar",
                show_calendar,
            ),
        ] {
            let _ = page;
            sections.push(split(
                label(widget.title(), 14., FG),
                toggler(on)
                    .on_toggle(move |value| Message::GlanceSection(key, value))
                    .into(),
            ));
        }
        if show_calendar {
            sections.push(split(
                label("Tomorrow’s events", 14., FG),
                container(toggler(g.show_tomorrow).on_toggle(Message::GlanceTomorrow))
                    .id(cosmic::widget::Id::new("glance-tomorrow"))
                    .into(),
            ));
        }
        // The cached sample switches to the actual combined renderer when
        // Daily overview is enabled; opening this editor starts no providers.
        // Shown whether or not it is on, so the switches have something to change.
        let preview = Some(super::sample::fit(
            self.samples.overview_view().map(|_| Message::Preview),
            240.,
        ));
        let mut parts = vec![group(Some("Include in this card"), sections)];
        if g.enabled {
            parts.push(group(Some("Look"), appearance));
            if !follows {
                parts.push(self.glance_text_section());
            }
        }
        self.widget_template(Widget::Glance, "Daily overview", preview, parts)
    }

    pub(super) fn clock_page(&self) -> Element<'_, Message> {
        use crate::widget_style::resolver::{ClockFace, ClockSurface, ClockWeight};
        let clock = self.config.clock_layer(self.clock_moment_target());
        let follow = if self.clock_moment_target().is_some() {
            "Usual style"
        } else {
            "Match style"
        };
        let (clock_font, missing_font) = {
            let _scope = crate::widget_style::scope(&self.config);
            let style = crate::widget_style::presentation(
                &self.config,
                self.config.wallpaper_palette(),
                true,
            );
            (style.display.value, style.missing_font)
        };
        // Following the look hides the clock's own style choices; editing a
        // time-of-day layer always shows them.
        let follows = self.clock_moment_target().is_none() && !self.clock_custom && {
            let (now, look) = (
                crate::settings::profiles::LookStyle::of(&self.config),
                self.profiles.applied_style(),
            );
            now.clock == look.clock && now.clock_font == look.clock_font
        };
        let mut controls: Vec<Element<'_, Message>> = Vec::new();
        if !follows {
            controls.push(self.widget_font_row(
                super::super::FontTarget::Clock,
                "Font",
                self.config.clock_font.as_deref(),
                "Same as widgets",
            ));
        }
        controls.extend([
            // A slider, like every other percentage in the app.
            column::with_children(vec![
                split(
                    label(
                        if clock.face == Some(ClockFace::Analog) {
                            "Face size"
                        } else {
                            "Text size"
                        },
                        14.,
                        FG,
                    ),
                    row::with_children(
                        // Reset appears only once the size has been changed.
                        (clock.scale != 100)
                            .then(|| {
                                button::text("Reset")
                                    .id(cosmic::widget::Id::new("clock-scale-reset"))
                                    .on_press(Message::ClockScale(100))
                                    .into()
                            })
                            .into_iter()
                            .chain([label(
                                format!("{}%", clock.scale),
                                13.,
                                self.settings_accent(),
                            )])
                            .collect::<Vec<Element<'_, Message>>>(),
                    )
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
                ),
                container(
                    slider(50u16..=200, clock.scale.clamp(50, 200), Message::ClockScale)
                        .name(if clock.face == Some(ClockFace::Analog) {
                            "Face size"
                        } else {
                            "Text size"
                        })
                        .step(10u16)
                        .width(Length::Fill),
                )
                .id(cosmic::widget::Id::new("clock-scale"))
                .into(),
                // One size model: the box is the clock's size, and the
                // stage corner grows it with the digits. This only fits
                // the time or face inside that box.
                label(
                    if clock.face == Some(ClockFace::Analog) {
                        "Sizes the face inside its box. To make the whole clock bigger, drag its corner above."
                    } else {
                        "Sizes the time inside its box. To make the whole clock bigger, drag its corner above."
                    },
                    CAPTION,
                    MUTED,
                ),
            ])
            .spacing(6)
            .into(),
            split(
                label("Alignment", 14., FG),
                segmented(
                    [
                        ("Left", ClockAlignment::Left),
                        ("Center", ClockAlignment::Center),
                        ("Right", ClockAlignment::Right),
                    ]
                    .into_iter()
                    .map(|(name, value)| {
                        (
                            name.into(),
                            clock.alignment == value,
                            Some(Message::ClockAlignment(value)),
                        )
                    })
                    .collect(),
                    self.settings_accent(),
                ),
            ),
        ]);
        if !follows {
            controls.push(split(
                label("Weight", 14., FG),
                segmented(
                    [
                        (follow, None),
                        ("Light", Some(ClockWeight::Light)),
                        ("Regular", Some(ClockWeight::Normal)),
                        ("Bold", Some(ClockWeight::Bold)),
                    ]
                    .into_iter()
                    .map(|(name, value)| {
                        (
                            name.into(),
                            clock.weight == value,
                            value
                                .is_none_or(|weight| {
                                    crate::desktop_theme::font_style_available(cosmic::iced::Font {
                                        weight: weight.native(),
                                        ..clock_font
                                    })
                                })
                                .then_some(self.clock_moment_edit(Message::ClockWeight(value))),
                        )
                    })
                    .collect(),
                    self.settings_accent(),
                ),
            ));
            controls.push(split(
                label("Letter style", 14., FG),
                segmented(
                    [
                        (follow, None),
                        ("Upright", Some(false)),
                        ("Italic", Some(true)),
                    ]
                    .into_iter()
                    .map(|(name, value)| {
                        (
                            name.into(),
                            clock.italic == value,
                            value
                                .is_none_or(|italic| {
                                    crate::desktop_theme::font_style_available(cosmic::iced::Font {
                                        style: if italic {
                                            cosmic::iced::font::Style::Italic
                                        } else {
                                            cosmic::iced::font::Style::Normal
                                        },
                                        ..clock_font
                                    })
                                })
                                .then_some(self.clock_moment_edit(Message::ClockItalic(value))),
                        )
                    })
                    .collect(),
                    self.settings_accent(),
                ),
            ));
        }
        if missing_font.is_some() {
            controls.push(label(
                "Selected font unavailable. Using the system sans-serif.",
                12.,
                MUTED,
            ));
        } else if !crate::desktop_theme::font_style_available(clock_font) {
            controls.push(label("Using the closest installed font style.", 12., MUTED));
        }
        if !self.config.glance.enabled {
            controls.push(switch(
                "Center vertically",
                "In the middle of its card",
                clock.center_vertically,
                Message::ClockVertical,
            ));
        }
        let content = vec![
            switch("24-hour time", "Show 14:00 instead of 2:00 PM", clock.format_24h, Message::ClockFormat),
            switch("Show the date", "", clock.show_date, Message::ClockDate),
        ];
        let scene_row = clock.depth || self.picture_wallpaper();
        let mut face_rows: Vec<Element<'_, Message>> = Vec::new();
        if self.clock_moment_target().is_none() {
            face_rows.push(split(
                detail(
                    "Match the desktop style",
                    "Font, weight, letter style, type and background come from the look",
                ),
                container(toggler(follows).on_toggle(Message::ClockFollowStyle))
                    .id(cosmic::widget::Id::new("clock-follow-style"))
                    .into(),
            ));
        }
        if !follows {
            face_rows.extend([
                split(label("Type", 14., FG), segmented(
                    [(follow, None), ("Digital", Some(ClockFace::Digital)), ("Analog", Some(ClockFace::Analog))]
                        .into_iter().map(|(name, value)| (name.into(), clock.face == value,
                            Some(self.clock_moment_edit(Message::ClockFace(value)))))
                        .collect(), self.settings_accent())),
                split(detail("Background", if self.config.glance.enabled {
                    "For the separate clock; Daily overview keeps its own"
                } else {
                    "Behind the time"
                }), segmented(
                    [(follow, None), ("None", Some(ClockSurface::None)), ("Card", Some(ClockSurface::Panel))]
                        .into_iter().map(|(name, value)| (name.into(), clock.surface == value,
                            Some(self.clock_moment_edit(Message::ClockSurface(value)))))
                        .collect(), self.settings_accent())),
            ]);
        }
        if scene_row {
            let switch = toggler(clock.depth).on_toggle(Message::ClockDepth);
            // Once it's on, a blocked switch says why and offers the one fix.
            let block = clock.depth.then(|| self.depth_block()).flatten();
            let lighting = clock.depth && block.is_none();
            const TITLE: &str = "Let the scene cover the clock";
            face_rows.push(match block {
                // Two ways out get a row of their own under the switch.
                Some(super::depth::Block {
                    reason,
                    fix: Some((fix, message)),
                    alternative: Some((other, instead)),
                }) => column::with_children(vec![
                    split(detail_state(TITLE, reason), switch.into()),
                    container(
                        row::with_children(vec![
                            container(button::standard(fix).on_press(message))
                                .id(cosmic::widget::Id::new("clock-depth-fix"))
                                .into(),
                            container(button::standard(other).on_press(instead))
                                .id(cosmic::widget::Id::new("clock-depth-alternative"))
                                .into(),
                        ])
                        .spacing(8)
                        .wrap(),
                    )
                    .width(Length::Fill)
                    .align_x(Alignment::End)
                    .into(),
                ])
                .spacing(8)
                .into(),
                Some(super::depth::Block {
                    reason,
                    fix: Some((fix, message)),
                    alternative: None,
                }) => split(
                    detail_state(TITLE, reason),
                    cosmic::widget::row![
                        container(button::standard(fix).on_press(message))
                            .id(cosmic::widget::Id::new("clock-depth-fix")),
                        switch,
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .into(),
                ),
                Some(super::depth::Block { reason, fix: None, .. }) => {
                    split(detail_state(TITLE, reason), switch.into())
                }
                None => split(
                    detail(
                        TITLE,
                        "The picture’s foreground covers the bottom of the time",
                    ),
                    switch.into(),
                ),
            });
            if (clock.depth && !self.ridge_wallpaper())
                || self.scenes.model != super::depth::ModelState::Missing
            {
                face_rows.push(self.scene_model_row());
            }
            if lighting {
                face_rows.push(split(
                    label("Scene lighting", 14., FG),
                    container(segmented(
                        crate::depth::Lighting::ALL
                            .into_iter()
                            .map(|value| {
                                (
                                    value.label().into(),
                                    clock.lighting == value,
                                    Some(Message::ClockLighting(value)),
                                )
                            })
                            .collect(),
                        self.settings_accent(),
                    ))
                    .id(cosmic::widget::Id::new("clock-lighting"))
                    .into(),
                ));
            }
        }
        let face = self.search_target("clock-face", group(Some("Face"), face_rows));
        self.widget_template(
            Widget::Clock,
            "Clock",
            Some(self.clock_stage_view()),
            vec![
                self.moment_clock_target_view(),
                group(Some("Time and date"), content),
                face,
                group(Some("Text"), controls),
            ],
        )
    }

    fn settings_row<'a>(
        &self,
        symbol: &'static str,
        title: &'static str,
        description: String,
        live: bool,
        message: Message,
        id: String,
    ) -> Element<'a, Message> {
        button::custom(
            row::with_children(vec![
                icon::from_name(symbol).size(20).icon().into(),
                if live {
                    detail_state(title, description)
                } else {
                    detail(title, description)
                },
                icon::from_name("go-next-symbolic").size(16).icon().into(),
            ])
            .spacing(14)
            .align_y(Alignment::Center),
        )
        .id(cosmic::widget::Id::new(id))
        .class(nav_style(false, self.settings_accent()))
        .padding([6, 4])
        .width(Length::Fill)
        .on_press(message)
        .into()
    }

    /// The Halcyon landing: one list of how the app itself runs.
    fn settings_home(&self) -> Element<'_, Message> {
        let row = |symbol, page: AppSettingsPage, description: String, live: bool| {
            self.settings_row(
                symbol,
                page.tab_title(),
                description,
                live,
                Message::AppSettingsPage(page),
                format!("app-settings-{}", page.key()),
            )
        };
        // One list, no group heading; Accessibility has its own entry.
        let halcyon = cosmic::widget::settings::section()
            .add(row(
                "system-log-out-symbolic",
                AppSettingsPage::Operation,
                if self.service.startup {
                    "Starts at sign-in · Set up again".into()
                } else {
                    "Running, sign-in and set up again".into()
                },
                self.service.startup,
            ))
            .add(row(
                "view-dual-symbolic",
                AppSettingsPage::Interface,
                "This window".into(),
                false,
            ))
            .add(row(
                "software-update-available-symbolic",
                AppSettingsPage::Updates,
                self.updates_summary_line(),
                true,
            ))
            .add(row(
                "security-medium-symbolic",
                AppSettingsPage::Privacy,
                if self.usage_enabled {
                    "Sharing usage data".into()
                } else {
                    "Nothing is shared".into()
                },
                true,
            ))
            .add(row(
                "help-browser-symbolic",
                AppSettingsPage::Help,
                "Get help or report a problem".into(),
                false,
            ));
        column::with_children(vec![
            heading("Halcyon", "How Halcyon starts, updates and behaves."),
            halcyon.into(),
        ])
        .spacing(16)
        .into()
    }

    pub(super) fn general_page(&self) -> Element<'_, Message> {
        let selected = self.app_settings_page;
        let content = match selected {
            AppSettingsPage::Home => return self.settings_home(),
            AppSettingsPage::Operation => self.app_operation_page(),
            AppSettingsPage::Interface => self.app_interface_page(),
            AppSettingsPage::Privacy => self.privacy_section(),
            AppSettingsPage::Updates => self.updates_section(),
            AppSettingsPage::WhatsNew => self.whats_new_page(),
            AppSettingsPage::Help => self.help_page(),
        };
        let mut parts = Vec::new();
        if !(selected == AppSettingsPage::Help && self.support.report.is_some())
        {
            parts.push(heading(
                selected.title(),
                match selected {
                    AppSettingsPage::Operation => "How Halcyon runs and starts.",
                    AppSettingsPage::Interface => "How this window looks and behaves.",
                    AppSettingsPage::Privacy => {
                        "What Halcyon shares, and what stays on your computer."
                    }
                    AppSettingsPage::Updates => "Which version you have and how it updates.",
                    AppSettingsPage::WhatsNew => "New in this version.",
                    AppSettingsPage::Help => "Get help, or tell us about a problem.",
                    AppSettingsPage::Home => "",
                },
            ));
        }
        parts.push(content);
        column::with_children(parts).spacing(20).into()
    }

    fn app_operation_page(&self) -> Element<'_, Message> {
        let mut running = toggler(self.service.running);
        let mut startup = toggler(self.service.startup);
        if self.service_known && !self.busy && !preview() {
            running = running.on_toggle(Message::Background);
            startup = startup.on_toggle(Message::Startup);
        }
        // Why it can't be changed yet stays visible; the explanation hides.
        let background = if preview() {
            group_row_state(
                "Keep running in the background",
                "Not available in this preview",
                running.into(),
            )
        } else if !self.service_known {
            group_row_state("Keep running in the background", "Checking…", running.into())
        } else {
            group_row(
                "Keep running in the background",
                "Wallpaper colors and schedules keep working while widgets are hidden",
                running.into(),
            )
        };
        column::with_children(vec![
            group(
                Some("Running"),
                vec![
                    // Show widgets lives on the Widgets page only.
                    background,
                    group_row("Start when you sign in", "", startup.into()),
                ],
            ),
            group(
                Some("Setting up"),
                vec![
                    group_row(
                        "Set up again",
                        "Pick a look, dock, widgets and weather step by step",
                        button::standard("Start")
                            .on_press_maybe((!self.busy).then_some(Message::OpenWizard))
                            .into(),
                    ),
                ],
            ),
        ])
        .spacing(16)
        .into()
    }

    fn app_interface_page(&self) -> Element<'_, Message> {
        column::with_children(vec![
            group(Some("This window"), vec![
                self.search_target("settings-motion", group_row(
                    "Animate settings",
                    "Sections slide open when you expand them",
                    toggler(self.config.settings_animations).on_toggle(Message::SettingsAnimations).into(),
                )),
                self.search_target("settings-explanations", group_row(
                    "Show explanations",
                    "Show what each setting does under its name. Off, point at or click ? beside a name to read it.",
                    container(toggler(self.config.show_explanations).on_toggle(Message::ShowExplanations))
                        .id(cosmic::widget::Id::new("settings-explanations-toggle"))
                        .into(),
                )),
            ]),
        ])
        .spacing(16)
        .into()
    }

    /// Everything waiting, each with Resume. Applying and discarding stay on
    /// the screen that owns the preview.
    pub(super) fn not_applied_section(&self) -> Element<'_, Message> {
        let kept = self.kept_previews();
        let mut rows: Vec<Element<'_, Message>> = kept
            .iter()
            .map(|item| {
                group_row_state(
                    item.title.clone(),
                    if item.conflict {
                        "Changed on your desktop too · choose what to keep"
                    } else {
                        "Not applied"
                    },
                    button::text("Resume")
                        .id(cosmic::widget::Id::new(format!("resume-{}", item.title)))
                        .on_press(Message::ResumeKept(item.target.clone()))
                        .into(),
                )
            })
            .collect();
        if rows.is_empty() {
            rows.push(detail_state(
                "Nothing waiting",
                "Previews you leave stay listed here.",
            ));
        }
        group(Some("Not applied yet"), rows)
    }

}
