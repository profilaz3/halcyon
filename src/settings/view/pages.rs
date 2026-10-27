use super::*;

/// Transient App settings navigation, independent of saved configuration.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppSettingsPage {
    #[default]
    Operation,
    Interface,
    Privacy,
    Updates,
    Restore,
    Help,
}

impl AppSettingsPage {
    pub const ALL: [Self; 6] = [
        Self::Operation,
        Self::Interface,
        Self::Privacy,
        Self::Updates,
        Self::Restore,
        Self::Help,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Operation => "Startup & widgets",
            Self::Interface => "Interface",
            Self::Privacy => "Privacy",
            Self::Updates => "Updates & installation",
            Self::Restore => "Restore & recovery",
            Self::Help => "Help & diagnostics",
        }
    }

    pub fn tab_title(self) -> &'static str {
        match self {
            Self::Operation => "Startup",
            Self::Updates => "Updates",
            Self::Restore => "Recovery",
            Self::Help => "Help",
            page => page.title(),
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Operation => "operation",
            Self::Interface => "interface",
            Self::Privacy => "privacy",
            Self::Updates => "updates",
            Self::Restore => "restore",
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

pub(super) fn group_label<'a>(title: &'static str) -> Element<'a, Message> {
    container(label(title, 10., MUTED))
        .padding([10, 6, 2, 6])
        .into()
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
    pub(super) fn widget_style_and_layout(&self, widget: Widget) -> Element<'_, Message> {
        group(
            Some("Surface & placement"),
            vec![
                group_row(
                    "Surface finish",
                    "Opacity, corners, outline and readability",
                    button::text("Widget style")
                        .on_press(Message::Page(Page::Appearance))
                        .into(),
                ),
                group_row(
                    "Position & size",
                    "Move or resize this widget in Layout and displays",
                    button::text("Move & resize")
                        .id(cosmic::widget::Id::new(format!(
                            "{}-edit-layout",
                            widget.key()
                        )))
                        .on_press(Message::EditWidgetLayout(widget))
                        .into(),
                ),
            ],
        )
    }

    pub(super) fn widget_heading(
        &self,
        widget: Widget,
        title: &'static str,
    ) -> Element<'_, Message> {
        let combined = self.config.glance.enabled
            && matches!(widget, Widget::Clock | Widget::Weather | Widget::Calendar);
        let header = split(
            heading(title, ""),
            row::with_children(vec![
                label(
                    if combined {
                        "Part of At a glance"
                    } else {
                        "On desktop"
                    },
                    11.,
                    MUTED,
                ),
                toggler(self.config.data_visible(widget))
                    .on_toggle(move |visible| Message::Widget(widget, visible))
                    .into(),
            ])
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        );
        if !combined
            && (self.displays.outputs().len() > 1
                || self.config.widget_monitors.contains_key(widget.key()))
        {
            column::with_children(vec![
                header,
                group(
                    Some("Widget location"),
                    vec![self.widget_display_controls(widget)],
                ),
            ])
            .spacing(8)
            .into()
        } else {
            header
        }
    }

    /// Each widget has one row: its name opens settings and its switch controls visibility.
    pub(super) fn widgets_page(&self) -> Element<'_, Message> {
        let group = |title: &'static str, widgets: &[Widget]| -> Element<'_, Message> {
            cosmic::widget::settings::section()
                .title(title)
                .extend(widgets.iter().map(|&widget| self.widget_list_row(widget)))
                .into()
        };
        let information = group(
            "Information",
            &[
                Widget::Glance,
                Widget::Clock,
                Widget::Calendar,
                Widget::Weather,
                Widget::Devbox,
                Widget::Projects,
            ],
        );
        let tools = group("Tools", &[Widget::Music, Widget::Notes, Widget::Focus]);
        let groups =
            super::desktop::inspector::paired(self.window.width >= 1100., information, tools);
        column::with_children(vec![
            heading(
                "My widgets",
                "Choose a widget to customize it. Use its switch to show or hide it.",
            ),
            groups,
        ])
        .spacing(16)
        .into()
    }
    fn widget_list_row(&self, widget: Widget) -> Element<'_, Message> {
        let on = self.config.data_visible(widget);
        let inside = self.config.glance.enabled
            && matches!(widget, Widget::Clock | Widget::Weather | Widget::Calendar);
        let status = if widget == Widget::Notes && self.tools.dirty() {
            "Unsaved note"
        } else if widget == Widget::Calendar && !self.calendar.connection.connected() {
            "Connect your calendar"
        } else if widget == Widget::Weather && self.config.location.is_none() {
            "Choose a city"
        } else if widget == Widget::Projects && self.config.project_roots.is_empty() {
            "Choose folders"
        } else if inside {
            "Part of At a glance"
        } else if widget == Widget::Glance {
            "Clock, weather and calendar in one widget"
        } else {
            ""
        };
        row::with_children(vec![
            button::custom(
                row::with_children(vec![
                    icon::from_name(widget.icon()).size(20).icon().into(),
                    detail(widget.title(), status),
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

    pub(super) fn widget_order_controls(&self) -> Element<'_, Message> {
        let order = self.config.ordered_right_cards();
        let custom_positions = self.config.has_custom_positions();
        // Column order only applies to the default layout; a custom layout or
        // an unsaved placement draft owns positions instead.
        let reorderable = !custom_positions && !self.placement_dirty();
        let mut rows = Vec::new();
        let settings_widgets: Vec<Widget> = if self.config.glance.enabled {
            [Widget::Glance]
                .into_iter()
                .chain(order.iter().copied())
                .collect()
        } else {
            [Widget::Clock, Widget::Calendar]
                .into_iter()
                .chain(order.iter().copied())
                .collect()
        };
        for widget in settings_widgets {
            if reorderable
                && (matches!(widget, Widget::Clock | Widget::Glance) || widget == order[0])
            {
                rows.push(group_label(
                    if matches!(widget, Widget::Clock | Widget::Glance) {
                        "LEFT"
                    } else {
                        "RIGHT COLUMN"
                    },
                ));
            } else if !rows.is_empty() {
                rows.push(separator());
            }
            rows.push(self.widget_row(
                widget,
                order.iter().position(|item| *item == widget),
                reorderable,
            ));
        }
        rows.push(group_label("FREE PLACEMENT"));
        for tool in Widget::TOOLS {
            if tool != Widget::Music {
                rows.push(separator());
            }
            rows.push(self.widget_row(tool, None, reorderable));
        }
        let note = if reorderable {
            "Drag the handles to reorder the right column."
        } else if custom_positions {
            "Custom layout · Reset it to reorder the column."
        } else {
            "Finish or cancel the current arrangement to reorder."
        };
        column::with_children(vec![frame(
            column::with_children(vec![
                label("Card order", 16., FG),
                column::with_children(rows).spacing(2).into(),
                label(note, 11., MUTED),
            ])
            .spacing(8)
            .into(),
            SURFACE,
            LINE,
            16,
        )])
        .spacing(16)
        .into()
    }

    fn widget_row(
        &self,
        widget: Widget,
        index: Option<usize>,
        reorderable: bool,
    ) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let visible = self.config.widget_visible(widget);
        let order_len = self.config.ordered_right_cards().len();
        let handle = index.is_some() && reorderable;
        let dragging = self.reorder.source == Some(widget);
        let target =
            self.reorder.source.is_some() && self.reorder.target == Some(widget) && !dragging;
        let mut parts: Vec<Element<'_, Message>> = Vec::new();
        if handle {
            parts.push(
                mouse_area(
                    container(icon::from_name("grip-lines-symbolic").size(16).icon()).padding(6),
                )
                .on_press(Message::DragStart(widget))
                .interaction(if dragging {
                    cosmic::iced::mouse::Interaction::Grabbing
                } else {
                    cosmic::iced::mouse::Interaction::Grab
                })
                .into(),
            );
        } else if reorderable {
            parts.push(container(text::body("")).width(28).into());
        }
        parts.push(icon::from_name(widget.icon()).size(18).icon().into());
        parts.push(
            container(label(widget.title(), 14., if visible { FG } else { MUTED }))
                .width(Length::Fill)
                .into(),
        );
        if target || dragging || !visible {
            parts.push(pill(
                if target {
                    "Drop here"
                } else if dragging {
                    "Esc cancels"
                } else {
                    "Hidden"
                },
                if target { accent } else { MUTED },
            ));
        }
        if let Some(index) = index.filter(|_| reorderable) {
            let idle = self.reorder.source.is_none();
            parts.push(
                button::icon(icon::from_name("go-up-symbolic"))
                    .tooltip(format!("Move {} up", widget.title()))
                    .on_press_maybe((index > 0 && idle).then_some(Message::Move(widget, -1)))
                    .into(),
            );
            parts.push(
                button::icon(icon::from_name("go-down-symbolic"))
                    .tooltip(format!("Move {} down", widget.title()))
                    .on_press_maybe(
                        (index + 1 < order_len && idle).then_some(Message::Move(widget, 1)),
                    )
                    .into(),
            );
        }
        parts.push(
            button::icon(icon::from_name("go-next-symbolic"))
                .tooltip(format!("{} settings", widget.title()))
                .on_press(Message::Page(widget_page(widget)))
                .into(),
        );
        let content = container(
            row::with_children(parts)
                .spacing(10)
                .align_y(Alignment::Center),
        )
        .padding([2, 6])
        .height(44)
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
        if handle {
            mouse_area(content)
                .on_enter(Message::DragOver(widget))
                .on_move(move |_| Message::DragOver(widget))
                .on_exit(Message::DragLeave(widget))
                .into()
        } else {
            content.into()
        }
    }

    pub(super) fn calendar_page(&self) -> Element<'_, Message> {
        let notice = row::with_children(vec![
            slider(1u16..=60, self.notice, Message::Notice)
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
                self.help("calendar-days", "At a glance agenda",
                    "Today is always shown. These days belong to At a glance; the separate Calendar keeps its own choices."),
                split(label("Tomorrow", 14., FG),
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
                self.help("meeting-highlight", "Highlight before",
                    "Meetings are highlighted this many minutes before they start. This also applies to At a glance."),
                notice.into(),
            ])
            .spacing(6)
            .into(),
        );
        let mut style = vec![group_row(
            "Card style",
            "Opacity, corners and outline",
            button::text("Widget style")
                .trailing_icon(icon::from_name("go-next-symbolic").size(14))
                .on_press(Message::Page(Page::Appearance))
                .into(),
        )];
        if self.config.glance.enabled {
            style.push(group_row(
                "At a glance",
                "Layout and surface",
                button::text("At a glance")
                    .trailing_icon(icon::from_name("go-next-symbolic").size(14))
                    .on_press(Message::Page(Page::Glance))
                    .into(),
            ));
        }
        let mut items = Vec::new();
        if self.calendar.connection.connected() {
            items.push(group(Some("Agenda & privacy"), events));
        }
        items.push(group(Some("Appearance"), style));
        items.push(group(
            Some("Widget placement"),
            vec![group_row(
                "Position & size",
                "Move or resize Calendar in Layout and displays",
                button::text("Move & resize")
                    .on_press(Message::EditWidgetLayout(Widget::Calendar))
                    .into(),
            )],
        ));
        items.push(self.page_settings_advanced(
            Page::Calendar,
            vec![
                self.refresh_control(Widget::Calendar),
                self.calendar_publisher_setup(),
            ],
        ));
        column::with_children(vec![
            self.widget_heading(Widget::Calendar, "Calendar"),
            self.inspector_layout(
                column::with_children(vec![
                    self.calendar_connection(),
                    self.scene_frame(
                        self.preview_stage(super::sample::fit(
                            self.samples.calendar_view().map(|_| Message::Preview),
                            180.,
                        )),
                        "Sample agenda",
                    ),
                ])
                .spacing(16)
                .into(),
                column::with_children(items).spacing(12).into(),
                "calendar",
            ),
        ])
        .spacing(16)
        .into()
    }

    pub(super) fn weather_page(&self) -> Element<'_, Message> {
        let location = self
            .config
            .location
            .as_ref()
            .map(|loc| loc.name.as_str())
            .unwrap_or("Choose a city");
        let state = &self.city_search;
        let editing = self.city_editing || self.config.location.is_none();
        let mut location_controls = vec![split(
            detail(location, "°C · km/h"),
            button::text(if editing { "Cancel" } else { "Change" })
                .on_press_maybe(
                    (!editing || self.config.location.is_some())
                        .then_some(Message::EditCity(!editing)),
                )
                .into(),
        )];
        if editing {
            let ready = state.query.trim().chars().count() >= 2 && !state.searching;
            let search = text_input("City name", &state.query)
                .id(cosmic::widget::Id::new("weather-city"))
                .on_input(Message::CityInput)
                .on_submit(|_| Message::SearchCity)
                .width(Length::Fill);
            let mut search_row = vec![search.into()];
            if !state.query.is_empty() {
                search_row.push(
                    button::icon(icon::from_name("edit-clear-symbolic"))
                        .tooltip("Clear search")
                        .on_press(Message::ClearCitySearch)
                        .into(),
                );
            }
            if state.error.is_some() {
                search_row.push(
                    button::standard("Retry")
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
                            detail(city.name.clone(), city.region()),
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
                location_controls.push(detail(
                    "No cities found",
                    "Try another spelling or a nearby town.",
                ));
            } else {
                location_controls.push(label("Search by city name.", 12., MUTED));
            }
        }
        let controls = column::with_children(vec![
            self.search_target("weather-location", group(Some("Location"), location_controls)),
            self.search_target(
                "weather-dew-point",
                group(
                    Some("Weather details"),
                    vec![split(
                        self.help(
                            "dew-point",
                            "Dew point",
                            "The temperature at which moisture condenses. Shown in Weather and At a glance when available.",
                        ),
                        container(
                            toggler(self.config.weather.show_dew_point)
                                .on_toggle(Message::WeatherDewPoint),
                        )
                        .id(cosmic::widget::Id::new("weather-dew-point-toggle"))
                        .into(),
                    )],
                ),
            ),
            self.widget_style_and_layout(Widget::Weather),
            self.page_settings_advanced(Page::Weather, vec![self.refresh_control(Widget::Weather)]),
            button::text("Weather by Open-Meteo ↗")
                .on_press(Message::OpenWeatherSource)
                .into(),
        ])
        .spacing(12)
        .into();
        column::with_children(vec![
            self.widget_heading(Widget::Weather, "Weather"),
            self.widget_settings_body(
                crate::app::settings_weather_preview(&self.config).map(|_| Message::Preview),
                controls,
            ),
        ])
        .spacing(14)
        .into()
    }

    pub(super) fn projects_page(&self) -> Element<'_, Message> {
        let mut roots = vec![split(
            detail("Project folders", ""),
            pill(
                self.config.project_roots.len().to_string(),
                self.settings_accent(),
            ),
        )];
        for root in &self.config.project_roots {
            let name = PathBuf::from(root)
                .file_name()
                .and_then(|part| part.to_str())
                .unwrap_or(root)
                .to_string();
            roots.push(split(
                row::with_children(vec![
                    icon::from_name("folder-symbolic").size(22).icon().into(),
                    detail(name, root.clone()),
                ])
                .spacing(12)
                .align_y(Alignment::Center)
                .into(),
                button::text("Remove")
                    .on_press(Message::RemoveFolder(root.clone()))
                    .into(),
            ));
        }
        if self.config.project_roots.is_empty() {
            roots.push(label(
                "Choose a folder containing your repositories.",
                12.,
                MUTED,
            ));
        }
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
        roots.push(
            button::standard(if self.folder_pending {
                "Choosing folder…"
            } else {
                "Browse folders…"
            })
            .on_press_maybe((!self.folder_pending).then_some(Message::BrowseFolder))
            .into(),
        );
        column::with_children(vec![
            self.widget_heading(Widget::Projects, "Projects"),
            self.inspector_layout(
                group(Some("Project folders"), roots),
                column::with_children(vec![
                    self.scene_frame(
                        self.preview_stage(super::sample::fit(
                            crate::app::settings_projects_preview(&self.config)
                                .map(|_| Message::Preview),
                            150.,
                        )),
                        "Sample projects",
                    ),
                    self.widget_style_and_layout(Widget::Projects),
                    self.page_settings_advanced(
                        Page::Projects,
                        vec![self.refresh_control(Widget::Projects)],
                    ),
                ])
                .spacing(16)
                .into(),
                "projects",
            ),
        ])
        .spacing(16)
        .into()
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
            Some("Refresh"),
            vec![
                split(
                    self.help(
                        "refresh-interval",
                        "Update interval",
                        "Updates pause while the widget is hidden.",
                    ),
                    reset,
                ),
                buttons,
                if self.config.data_visible(widget) {
                    cosmic::iced::widget::space().height(0).into()
                } else {
                    label("Paused while hidden", 11., MUTED)
                },
            ],
        )
    }

    pub(super) fn devbox_page(&self) -> Element<'_, Message> {
        if let Some(recipe) = &self.config.recipes.devbox {
            return column::with_children(vec![
                self.widget_heading(Widget::Devbox, "System monitor"),
                self.widget_settings_body(
                    crate::app::settings_devbox_preview(&self.config).map(|_| Message::Preview),
                    column::with_children(vec![
                        group(Some("Personal widget"), vec![split(label(&recipe.name, 14., FG), button::text("Edit widget…").id(cosmic::widget::Id::new("system-recipe-edit")).on_press(Message::OpenRecipe(crate::recipes::Kind::System)).into()), self.help("system-recipe-readings", "Readings", "Choose and order readings in Widget builder. Use built-in there restores your previous monitor settings.")]),
                        self.widget_style_and_layout(Widget::Devbox),
                        self.page_settings_advanced(Page::Devbox, vec![self.refresh_control(Widget::Devbox)]),
                    ]).spacing(12).into(),
                ),
            ]).spacing(14).into();
        }
        let order = self.config.devbox.ordered_sections();
        let mut rows = Vec::new();
        for (index, metric) in order.iter().copied().enumerate() {
            if index > 0 {
                rows.push(separator());
            }
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
        let controls = column::with_children(vec![
            group(Some("Readings"), vec![split(self.help("devbox-readings", "Readings", "Open a reading for its options. Reorder lets you drag rows or move them with the arrow buttons."),
                self.search_target("devbox-reorder-anchor", button::text(if self.devbox_reordering {"Done"} else {"Reorder"})
                    .id(cosmic::widget::Id::new("devbox-reorder"))
                    .on_press(Message::DevboxReorder(!self.devbox_reordering)).into())),
                column::with_children(rows).spacing(2).into()]),
            self.widget_style_and_layout(Widget::Devbox),
            self.page_settings_advanced(Page::Devbox, vec![self.refresh_control(Widget::Devbox)]),
        ])
        .spacing(12)
        .into();
        column::with_children(vec![
            self.widget_heading(Widget::Devbox, "System monitor"),
            self.widget_settings_body(
                crate::app::settings_devbox_preview(&self.config).map(|_| Message::Preview),
                controls,
            ),
        ])
        .spacing(14)
        .into()
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
            button::custom(split(
                label(section.title(), 14., if visible { FG } else { MUTED }),
                icon::from_name(if self.devbox_details.contains(&section) {
                    "go-down-symbolic"
                } else {
                    "go-next-symbolic"
                })
                .size(14)
                .icon()
                .into(),
            ))
            .id(cosmic::widget::Id::new(format!(
                "devbox-details-{}",
                section.key()
            )))
            .class(nav_style(false, accent))
            .padding([10, 2])
            .width(Length::Fill)
            .on_press(Message::DevboxDetails(section))
            .into()
        } else {
            container(label(
                section.title(),
                14.,
                if visible { FG } else { MUTED },
            ))
            .width(Length::Fill)
            .height(40)
            .align_y(Alignment::Center)
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
                        .tooltip(if direction < 0 {
                            "Move up"
                        } else {
                            "Move down"
                        })
                        .on_press_maybe(
                            (enabled && idle).then_some(Message::DevboxMove(section, direction)),
                        )
                        .into(),
                );
            }
        }
        parts.push(
            toggler(visible)
                .on_toggle(move |value| Message::Devbox(section.key(), value))
                .into(),
        );
        let content = container(
            row::with_children(parts)
                .spacing(4)
                .align_y(Alignment::Center),
        )
        .padding([2, 6])
        .height(44)
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

    pub(super) fn desktop_page(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let categories = [
            (
                Page::DesktopAppearance,
                "preferences-color-symbolic",
                "Theme & fonts",
                "Dark or light, fonts, icons and effects",
                self.system_appearance.dirty(),
            ),
            (
                Page::DesktopWallpaper,
                "preferences-desktop-wallpaper-symbolic",
                "Wallpaper & colors",
                "Backgrounds, shared accent and desktop items",
                self.wallpaper.dirty() || self.desktop_icons.dirty(),
            ),
            (
                Page::DesktopDock,
                "view-app-grid-symbolic",
                "Dock & panel",
                "Position, style and applets",
                self.desktop.dirty(),
            ),
            (
                Page::DesktopWindows,
                "preferences-window-management-symbolic",
                "Windows & workspaces",
                "Tiling, focus, workspaces and floating-app rules",
                self.windows.dirty() || self.workspaces.dirty() || self.window_rules.dirty(),
            ),
            (
                Page::DesktopShortcuts,
                "input-keyboard-symbolic",
                "Shortcuts",
                "Keyboard shortcuts and custom commands",
                self.shortcuts.dirty(),
            ),
            (
                Page::DesktopMotion,
                "preferences-desktop-accessibility-symbolic",
                "Accessibility",
                "Reduce motion in supported apps",
                false,
            ),
        ];
        let links = categories
            .into_iter()
            .map(|(page, symbol, title, description, dirty)| {
                button::custom(
                    row::with_children(vec![
                        icon::from_name(symbol).size(24).icon().into(),
                        detail(title, description),
                        if dirty {
                            pill("Not applied", accent)
                        } else {
                            icon::from_name("go-next-symbolic").size(16).icon().into()
                        },
                    ])
                    .spacing(16)
                    .align_y(Alignment::Center),
                )
                .id(cosmic::widget::Id::new(format!(
                    "desktop-open-{}",
                    page.key()
                )))
                .class(nav_style(false, accent))
                .padding([4, 0])
                .width(Length::Fill)
                .on_press(Message::Page(page))
            });
        column::with_children(vec![
            heading("Desktop", ""),
            cosmic::widget::settings::section().extend(links).into(),
            column::with_children(vec![
                label("More in COSMIC Settings", 13., MUTED),
                row::with_children(
                    [
                        ("keyboard", "Keyboard layouts"),
                        ("displays", "Displays"),
                        ("startup-apps", "Startup applications"),
                    ]
                    .into_iter()
                    .map(|(page, title)| {
                        button::text(format!("{title} ↗"))
                            .on_press(Message::OpenDesktop(page))
                            .into()
                    })
                    .collect::<Vec<_>>(),
                )
                .spacing(8)
                .wrap()
                .into(),
            ])
            .spacing(6)
            .into(),
        ])
        .spacing(16)
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
            ("Panel", GlanceStyle::Panel),
        ] {
            choices.push((
                title.into(),
                own && g.style == style,
                Some(Message::GlanceStyle(style)),
            ));
        }
        let mut appearance = vec![split(
            label("Background", 14., FG),
            container(segmented(choices, accent))
                .id(cosmic::widget::Id::new("glance-background"))
                .into(),
        )];
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
        let mut sections = vec![self.help("glance-sections", "Sections",
            "Clock appearance, weather city and calendar account are shared with the separate cards. Turning At a glance off restores those cards and their saved positions.")];
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
            sections.push(split(
                label(widget.title(), 14., FG),
                row::with_children(vec![
                    button::text("Settings")
                        .tooltip(format!("{} settings", widget.title()))
                        .on_press(Message::Page(page))
                        .into(),
                    toggler(on)
                        .on_toggle(move |value| Message::GlanceSection(key, value))
                        .into(),
                ])
                .spacing(12)
                .align_y(Alignment::Center)
                .into(),
            ));
        }
        if show_calendar {
            sections.push(split(
                label("Tomorrow", 14., FG),
                container(toggler(g.show_tomorrow).on_toggle(Message::GlanceTomorrow))
                    .id(cosmic::widget::Id::new("glance-tomorrow"))
                    .into(),
            ));
        }
        let controls = column::with_children(vec![
            group(Some("Sections"), sections),
            group(Some("Appearance"), appearance),
            self.glance_text_section(),
            self.widget_style_and_layout(Widget::Glance),
        ])
        .spacing(16)
        .into();
        // The cached sample switches to the actual combined renderer when
        // At a glance is enabled; opening this editor starts no providers.
        let body = if g.enabled {
            self.widget_settings_body(
                self.samples.calendar_view().map(|_| Message::Preview),
                controls,
            )
        } else {
            controls
        };
        column::with_children(vec![
            self.widget_heading(Widget::Glance, "At a glance"),
            label(
                if g.enabled {
                    "Clock, weather and calendar together in one widget. Turn it off to show them as separate cards again."
                } else {
                    "Turn it on to show clock, weather and calendar together in one widget instead of separate cards."
                },
                13.,
                MUTED,
            ),
            body,
        ]).spacing(16).into()
    }

    pub(super) fn clock_page(&self) -> Element<'_, Message> {
        use crate::widget_style::resolver::{ClockFace, ClockSurface, ClockWeight};
        let clock = self.config.clock_layer(self.clock_moment_target());
        let follow = if self.clock_moment_target().is_some() {
            "Use base widget style"
        } else {
            "Follow style"
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
        let preview_height = if self.config.moment_clock().face == Some(ClockFace::Analog) {
            176.
        } else {
            112.
        };
        let mut controls = vec![
            self.widget_font_row(
                super::super::FontTarget::Clock,
                "Font",
                self.config.clock_font.as_deref(),
                "Follow style",
            ),
            split(
                label(
                    if clock.face == Some(ClockFace::Analog) {
                        "Face size"
                    } else {
                        "Text size"
                    },
                    15.,
                    FG,
                ),
                row::with_children(vec![
                    button::text("−")
                        .id(cosmic::widget::Id::new("clock-scale-down"))
                        .on_press_maybe(
                            (clock.scale > 50).then_some(Message::ClockScale(
                                clock.scale.saturating_sub(10).max(50),
                            )),
                        )
                        .into(),
                    label(format!("{}%", clock.scale), 14., FG),
                    button::text("+")
                        .id(cosmic::widget::Id::new("clock-scale-up"))
                        .on_press_maybe(
                            (clock.scale < 200)
                                .then_some(Message::ClockScale((clock.scale + 10).min(200))),
                        )
                        .into(),
                    button::text("Reset")
                        .on_press_maybe((clock.scale != 100).then_some(Message::ClockScale(100)))
                        .into(),
                ])
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            ),
            split(
                label("Alignment", 15., FG),
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
        ];
        controls.push(split(
            label("Weight", 15., FG),
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
            label("Letter style", 15., FG),
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
                "",
                clock.center_vertically,
                Message::ClockVertical,
            ));
        }
        let content = vec![
            switch("24-hour time", "", clock.format_24h, Message::ClockFormat),
            switch("Show date", "", clock.show_date, Message::ClockDate),
        ];
        column::with_children(vec![
            self.widget_heading(Widget::Clock, "Clock"),
            self.moment_clock_target_view(),
            self.widget_settings_body(
                cosmic::widget::responsive(move |bounds| {
                    crate::app::settings_clock_preview(&self.config, [bounds.width, preview_height])
                        .map(|_| Message::Preview)
                }).height(preview_height).into(),
                column::with_children(vec![
                    group(Some("Face"),vec![segmented(
                        [(follow,None),("Digital",Some(ClockFace::Digital)),("Analog",Some(ClockFace::Analog))].into_iter()
                            .map(|(name,value)|(name.into(),clock.face==value,Some(self.clock_moment_edit(Message::ClockFace(value))))).collect(),self.settings_accent())]),
                    group(Some("Time & date"), content),
                    group(Some("Size & type"), controls),
                    group(Some("Clock surface"),vec![segmented(
                        [(follow,None),("None",Some(ClockSurface::None)),("Panel",Some(ClockSurface::Panel))].into_iter()
                            .map(|(name,value)|(name.into(),clock.surface==value,Some(self.clock_moment_edit(Message::ClockSurface(value))))).collect(),self.settings_accent()),
                        label(if self.config.glance.enabled {"The separate clock uses this surface. At a glance keeps its shared background."} else {"Readability assistance can add a background."},12.,MUTED)]),
                    group(Some("Depth"),vec![switch("Behind the landscape", "Requires Ridge wallpaper, a digital clock, no surface and auto-hiding bars. Other backgrounds, unsupported displays and Arrange show a plain clock.", clock.depth, Message::ClockDepth)]),
                    self.help("clock-position", "Position & size", "Move, resize or center the widget in Layout. In At a glance, the whole card moves together."),
                    self.widget_style_and_layout(Widget::Clock),
                    self.page_settings_advanced(Page::Clock, vec![self.refresh_control(Widget::Clock)])])
                    .spacing(16).into()),
        ])
        .spacing(16)
        .into()
    }

    pub(super) fn general_page(&self) -> Element<'_, Message> {
        let selected = self.app_settings_page;
        let choices = row::with_children(
            AppSettingsPage::ALL
                .into_iter()
                .map(|page| {
                    button::custom(text::body(page.tab_title()))
                        .id(cosmic::widget::Id::new(format!(
                            "app-settings-{}",
                            page.key()
                        )))
                        .class(nav_style(selected == page, self.settings_accent()))
                        .selected(selected == page)
                        .on_press(Message::AppSettingsPage(page))
                        .into()
                })
                .collect::<Vec<_>>(),
        )
        .spacing(6)
        .wrap();
        let content = match selected {
            AppSettingsPage::Operation => self.app_operation_page(),
            AppSettingsPage::Interface => self.app_interface_page(),
            AppSettingsPage::Privacy => self.privacy_section(),
            AppSettingsPage::Updates => {
                self.setting_columns(self.updates_section(), self.addons_section())
            }
            AppSettingsPage::Restore if self.support.recent_open => self.recent_changes_page(),
            AppSettingsPage::Restore => self.app_restore_page(),
            AppSettingsPage::Help => self.help_page(),
        };
        let mut parts = vec![choices.into()];
        if !(selected == AppSettingsPage::Restore && self.support.recent_open)
            && !(selected == AppSettingsPage::Help && self.support.report.is_some())
        {
            parts.push(heading(selected.title(), ""));
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
        let state = if preview() {
            "Preview"
        } else if !self.service_known {
            "Checking…"
        } else if self.service.running {
            "Running"
        } else {
            "Stopped"
        };
        self.setting_columns(
            group(
                Some("Widgets & background operation"),
                vec![
                    split(
                        label("Show widgets", 14., FG),
                        toggler(self.config.widgets_visible)
                            .on_toggle(Message::Desktop)
                            .into(),
                    ),
                    split(
                        detail(
                            "Run in background",
                            "Keeps wallpaper colors and schedules active when widgets are hidden.",
                        ),
                        row::with_children(vec![
                            pill(
                                state,
                                if self.service_known && self.service.running {
                                    self.settings_accent()
                                } else {
                                    MUTED
                                },
                            ),
                            running.into(),
                        ])
                        .spacing(8)
                        .align_y(Alignment::Center)
                        .into(),
                    ),
                    split(label("Start at sign-in", 14., FG), startup.into()),
                ],
            ),
            group(
                Some("Getting started"),
                vec![
                    split(
                        detail(
                            "First-run setup",
                            "Look, dock and panel, and weather, one step at a time.",
                        ),
                        button::standard("Run setup again")
                            .on_press_maybe((!self.busy).then_some(Message::OpenWizard))
                            .into(),
                    ),
                    split(
                        detail("Setup checklist", "Continue setting up your desktop."),
                        button::text("Show setup")
                            .on_press(Message::SetupOpen(true))
                            .into(),
                    ),
                ],
            ),
        )
    }

    fn app_interface_page(&self) -> Element<'_, Message> {
        self.setting_columns(
            group(Some("Edge menu"), vec![
                split(self.help("edge-menu", "Edge menu", "Hold the pointer at the far-right screen edge for one second to open widget controls."),
                    button::standard("Show menu")
                        .on_press_maybe((self.service_known && self.service.running && !self.busy && !preview())
                            .then_some(Message::ShowControls)).into()),
                switch("Animate the edge menu", "", self.config.sidebar_slide, Message::SidebarSlide),
            ]),
            group(Some("Settings interface"), vec![
                self.search_target("settings-motion", split(self.help("settings-motion", "Animate settings",
                    "Briefly reveals options when you expand a section. Reduce motion in Accessibility always takes precedence."),
                    toggler(self.config.settings_animations).on_toggle(Message::SettingsAnimations).into())),
                group_row("Reduce motion", "App motion and readability controls",
                    button::text("Accessibility")
                        .on_press(Message::Page(Page::DesktopMotion)).into()),
            ]),
        )
    }

    fn app_restore_page(&self) -> Element<'_, Message> {
        let mut items = vec![
            group(
                None,
                vec![group_row(
                    "Recent changes",
                    "See what changed, why it changed, and whether Undo is available.",
                    button::text("View changes")
                        .id(cosmic::widget::Id::new("recovery-recent-changes"))
                        .on_press(Message::Support(crate::settings::support::Message::Recent(
                            true,
                        )))
                        .into(),
                )],
            ),
            self.design_section(),
            self.desktop_recovery_section(),
            group(
                Some("Unfinished work"),
                vec![split(
                    if self.recovery_sheet.rows.is_empty() {
                        detail(
                            "Recovery",
                            "Review recovered work from the previous version.",
                        )
                    } else {
                        detail(
                            "Recovery",
                            format!(
                                "{} unfinished {} from the previous version",
                                self.recovery_sheet.rows.len(),
                                if self.recovery_sheet.rows.len() == 1 {
                                    "edit"
                                } else {
                                    "edits"
                                }
                            ),
                        )
                    },
                    button::text("Review unfinished work")
                        .on_press(Message::Page(Page::PendingChanges))
                        .into(),
                )],
            ),
            group(
                Some("Restore original wallpaper"),
                vec![split(
                    self.help(
                        "restore-wallpaper",
                        "Original wallpaper",
                        "Restore the wallpaper from before Halcyon was installed.",
                    ),
                    button::text("Restore")
                        .on_press_maybe(
                            (!self.busy && !preview()).then_some(Message::Restore(true)),
                        )
                        .into(),
                )],
            ),
        ];
        if self.desktop_recovery.plan.is_none() && self.desktop_recovery.problem.is_none() {
            let mut routine = items.split_off(2);
            let backup = routine.remove(0);
            items.push(
                self.setting_columns(backup, column::with_children(routine).spacing(16).into()),
            );
        }
        column::with_children(items).spacing(16).into()
    }
}
