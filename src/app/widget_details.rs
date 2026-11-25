//! Expanded readings live in a regular resizable window, leaving desktop cards stationary.
use super::*;
use cosmic::ApplicationExt;

/// A low battery, in the amber the cards warn in.
const LOW_BATTERY: Color = Color::from_rgb(233. / 255., 192. / 255., 124. / 255.);

pub(super) fn summary_line(
    value: impl Into<String>,
    size: f32,
    color: Color,
) -> Element<'static, Message> {
    text::body(value.into())
        .font(crate::widget_style::font())
        .size(size)
        .class(cosmic::theme::Text::Color(color))
        .wrapping(cosmic::iced::advanced::text::Wrapping::None)
        .ellipsize(cosmic::iced::advanced::text::Ellipsize::End(
            cosmic::iced::advanced::text::EllipsizeHeightLimit::Lines(1),
        ))
        .width(Length::Fill)
        .into()
}

impl Cards {
    pub(super) fn reading_key(
        &mut self,
        key: cosmic::iced::keyboard::Key,
        shift: bool,
    ) -> Task<Message> {
        use cosmic::iced::advanced::widget::operation::{focusable, scoped};
        use cosmic::iced::keyboard::{Key, key::Named};
        match key {
            Key::Named(Named::Escape) if self.calendar_details.take().is_some() => Task::none(),
            Key::Named(Named::Escape) => self.update(Message::CloseReading),
            Key::Named(Named::Tab) if shift => cosmic::iced::runtime::task::widget(scoped(
                "reading-controls".into(),
                focusable::focus_previous(),
            )),
            Key::Named(Named::Tab) => cosmic::iced::runtime::task::widget(scoped(
                "reading-controls".into(),
                focusable::focus_next(),
            )),
            _ => Task::none(),
        }
    }

    pub(super) fn open_reading(&mut self, widget: Widget) -> Task<Message> {
        if let Some((id, _)) = self.reading_window {
            self.reading_window = Some((id, widget));
            return Task::batch([
                window::gain_focus(id),
                self.set_window_title(format!("{} · Halcyon", widget.title()), id),
            ]);
        }
        let (id, open) = window::open(window::Settings {
            size: cosmic::iced::Size::new(520., 480.),
            platform_specific: window::settings::PlatformSpecific {
                application_id: "io.github.profilaz3.Halcyon".into(),
                ..Default::default()
            },
            ..Default::default()
        });
        self.reading_window = Some((id, widget));
        open.discard().chain(Task::batch([
            self.set_window_title(format!("{} · Halcyon", widget.title()), id),
            cosmic::iced::widget::operation::focus(cosmic::widget::Id::new("reading-close")),
        ]))
    }

    pub(super) fn reading_view(&self, widget: Widget) -> Element<'_, Message> {
        let p = self.config.surface_palette();
        let content = match widget {
            Widget::Devbox => devbox::view(
                &self.config,
                &self.metrics,
                &self.histories,
                self.devbox_natural_height(),
            ),
            Widget::Projects => self.full_projects(),
            Widget::Devices => self.devices_reading(p),
            Widget::Folder => {
                let heading = self
                    .folder_shown()
                    .as_deref()
                    .and_then(std::path::Path::file_name)
                    .map_or_else(|| "Folder".into(), |n| n.to_string_lossy().into_owned());
                return self.reading_frame(&heading, self.folder_reading(p));
            }
            Widget::Weather => {
                let _card = crate::widget_style::for_card(Widget::Weather);
                let presentation = weather::Presentation::new(
                    &self.config,
                    weather::Snapshot {
                        weather: self.weather.as_ref(),
                        updated_at: self.weather_at.as_deref(),
                        failed: self.weather_error,
                    },
                    Local::now(),
                );
                return self.reading_frame(&presentation.title(), presentation.full_reading(p));
            }
            Widget::Calendar | Widget::Glance => {
                if self.calendar_details.is_some()
                    && !self.config.calendar_private
                    && let Some(event) = self.calendar.iter()
                        .chain(self.calendar_today.iter())
                        .chain(self.calendar_ahead.iter())
                        .flat_map(|calendar| &calendar.events)
                        .find(|event| self.event_details_open(event))
                {
                    return self.reading_frame(&event.title, self.event_details_view(event, p));
                }
                let today = Local::now().date_naive();
                let days = if widget == Widget::Glance {
                    self.glance_days(today)
                } else {
                    self.calendar_days(today)
                };
                if days.is_empty() {
                    return self
                        .reading_frame("Agenda", label("No agenda days selected", 13., p.muted));
                }
                if days
                    .iter()
                    .all(|day| self.calendar_month_data(*day).is_none())
                {
                    return self.reading_frame("Agenda", self.missing_agenda(p));
                }
                let mut rows = Vec::new();
                for day in days {
                    rows.push(label(
                        day.format("%A, %B %-d").to_string(),
                        16.,
                        p.foreground,
                    ));
                    let agenda = if widget == Widget::Glance {
                        self.glance_day_agenda(day)
                    } else {
                        self.calendar_agenda(day)
                    };
                    if agenda.earlier > 0 {
                        rows.push(self.earlier_control(agenda.earlier, p));
                    }
                    if agenda.events.is_empty() && agenda.earlier == 0 {
                        rows.push(if self.calendar_month_data(day).is_none() {
                            self.missing_agenda(p)
                        } else {
                            label("No events", 13., p.muted)
                        });
                    }
                    for event in agenda.events {
                        let private = self.config.calendar_private;
                        let title = if private { "Busy" } else { &event.title };
                        let mut words = vec![label(title.to_owned(), 15., p.foreground)];
                        if !private
                            && self.config.calendar_show_location
                            && let Some(place) = event.place()
                        {
                            words.push(label(place.to_owned(), 12., p.muted));
                        }
                        let row = row::with_children(vec![
                            container(label(
                                event.time_label(self.config.clock.format_24h),
                                12.,
                                p.accent,
                            ))
                            .width(62.)
                            .into(),
                            column::with_children(words)
                                .spacing(3)
                                .width(Length::Fill)
                                .into(),
                        ])
                        .spacing(8)
                        .align_y(Alignment::Center);
                        rows.push(if private {
                            row.into()
                        } else {
                            cosmic::widget::button::custom(row)
                                .name(format!("Event details: {title}"))
                                .class(cosmic::theme::Button::Text)
                                .padding([4, 0])
                                .width(Length::Fill)
                                .on_press(Message::AgendaLink(widget, event.details_key()))
                                .into()
                        });
                    }
                }
                column::with_children(rows).spacing(8).into()
            }
            _ => empty_surface(),
        };
        let heading = if matches!(widget, Widget::Glance | Widget::Calendar) {
            "Agenda"
        } else {
            widget.title()
        };
        self.reading_frame(heading, content)
    }

    fn reading_frame<'a>(
        &self,
        heading: &str,
        content: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let p = self.config.surface_palette();
        container(
            column::with_children(vec![
                spread(
                    label(heading.to_string(), 20., p.foreground),
                    cosmic::widget::button::text("Close")
                        .id("reading-close".into())
                        .on_press(Message::CloseReading)
                        .into(),
                ),
                scrollable(content).height(Length::Fill).into(),
            ])
            .spacing(12),
        )
        .id("reading-controls")
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(Color {
                    a: 1.,
                    ..p.background
                })),
                ..Default::default()
            }
        }))
        .into()
    }

    /// Every connected device, each with its kind and battery: what the card
    /// leaves out when more are connected than its rings fit.
    fn devices_reading(&self, p: Palette) -> Element<'static, Message> {
        let list = &self.tools.devices;
        if list.is_empty() {
            return column::with_children(vec![
                label("No devices connected", 15., p.foreground),
                label(crate::tools::cards::DEVICE_SOURCES, 13., p.muted),
            ])
            .spacing(6)
            .into();
        }
        let rows = list
            .iter()
            .map(|device| {
                let low = device.low();
                let (battery, color) = match device.percent() {
                    Some(percent) => (
                        percent,
                        if low.is_some() { LOW_BATTERY } else { p.foreground },
                    ),
                    None => ("–".to_string(), p.muted),
                };
                let state = if device.charging() {
                    "Charging"
                } else if low.is_some_and(|level| level <= crate::devices::LOW[1]) {
                    "Battery very low"
                } else if low.is_some() {
                    "Battery low"
                } else if device.battery.is_none() {
                    "Battery not reported"
                } else {
                    ""
                };
                let mut reading = vec![label(battery, 15., color)];
                if !state.is_empty() {
                    reading.push(label(state, 12., p.muted));
                }
                row::with_children(vec![
                    container(
                        cosmic::widget::icon::from_name(device.kind.icon())
                            .size(24)
                            .icon(),
                    )
                    .class(cosmic::theme::Container::custom(move |_| {
                        cosmic::iced::widget::container::Style {
                            icon_color: Some(p.foreground),
                            ..Default::default()
                        }
                    }))
                    .into(),
                    column::with_children(vec![
                        label(device.name.clone(), 15., p.foreground),
                        label(device.kind.label(), 12., p.muted),
                    ])
                    .spacing(2)
                    .width(Length::Fill)
                    .into(),
                    column::with_children(reading)
                        .spacing(2)
                        .align_x(Alignment::End)
                        .into(),
                ])
                .spacing(12)
                .align_y(Alignment::Center)
                .into()
            })
            .collect::<Vec<_>>();
        column::with_children(rows).spacing(14).into()
    }

    /// Every entry of the shown folder, newest first, with what one can do
    /// with each: open it (a folder opens here), copy its path, or start a
    /// terminal in a folder. The path back up the chosen folder leads the way.
    fn folder_reading(&self, p: Palette) -> Element<'static, Message> {
        let (Some(root), Some(shown)) = (self.config.folder.path.clone(), self.folder_shown())
        else {
            return label("Choose a folder in Settings › Folder", 14., p.muted);
        };
        let link = |text: String, message: Message| -> Element<'static, Message> {
            cosmic::widget::button::custom(label(text, 13., p.accent))
                .class(cosmic::theme::Button::Text)
                .padding([2, 4])
                .on_press(message)
                .into()
        };
        let icon_button = |icon: &'static str,
                           tip: String,
                           message: Message|
         -> Element<'static, Message> {
            cosmic::widget::button::icon(cosmic::widget::icon::from_name(icon).size(16))
                .extra_small()
                .tooltip(tip.clone())
                .name(tip)
                .on_press(message)
                .into()
        };
        let url = |path: &std::path::Path| {
            reqwest::Url::from_file_path(path)
                .ok()
                .map(|url| Message::Open(url.into()))
        };
        // The chosen folder, then each folder down to the one shown.
        let mut crumbs = vec![link(
            root.file_name()
                .map_or_else(|| root.to_string_lossy(), |n| n.to_string_lossy())
                .into_owned(),
            Message::FolderBrowse(None),
        )];
        if let Ok(rest) = shown.strip_prefix(&root) {
            let mut at = root.clone();
            for part in rest.components() {
                at.push(part);
                crumbs.push(label("›", 13., p.muted));
                crumbs.push(link(
                    part.as_os_str().to_string_lossy().into_owned(),
                    Message::FolderBrowse(Some(at.clone())),
                ));
            }
        }
        let mut here = vec![
            row::with_children(crumbs)
                .spacing(2)
                .align_y(Alignment::Center)
                .wrap()
                .into(),
        ];
        let mut tools: Vec<Element<'static, Message>> = Vec::new();
        if let Some(open) = url(&shown) {
            tools.push(
                cosmic::widget::button::text("Open in Files")
                    .on_press(open)
                    .into(),
            );
        }
        tools.push(
            cosmic::widget::button::text("Terminal here")
                .on_press(Message::Terminal(shown.clone()))
                .into(),
        );
        tools.push(
            cosmic::widget::button::text("Copy path")
                .on_press(Message::CopyPath(shown.clone()))
                .into(),
        );
        here.push(row::with_children(tools).spacing(6).wrap().into());
        let Some(listing) = self
            .tools
            .folder_reading
            .as_ref()
            .filter(|listing| listing.source == shown)
        else {
            here.push(label("Reading folder…", 13., p.muted));
            return column::with_children(here).spacing(10).into();
        };
        if let Some(problem) = &listing.problem {
            here.push(label(problem.clone(), 13., p.muted));
            return column::with_children(here).spacing(10).into();
        }
        here.push(label(
            match listing.count {
                0 => "This folder is empty.".to_string(),
                1 => "1 item".to_string(),
                n if n > listing.entries.len() => {
                    format!("{n} items · the newest {} shown", listing.entries.len())
                }
                n => format!("{n} items"),
            },
            12.,
            p.muted,
        ));
        for entry in &listing.entries {
            let path = entry.path.clone();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let when = chrono::DateTime::<Local>::from(crate::folder::modified(entry))
                .format("%b %-d, %H:%M")
                .to_string();
            let detail = if entry.directory {
                format!("Folder · {when}")
            } else {
                format!("{} · {when}", size_text(crate::folder::bytes(entry)))
            };
            let mut actions = Vec::new();
            if entry.directory {
                actions.push(icon_button(
                    "go-next-symbolic",
                    format!("Open {name} here"),
                    Message::FolderBrowse(Some(path.clone())),
                ));
                actions.push(icon_button(
                    "utilities-terminal-symbolic",
                    format!("Terminal in {name}"),
                    Message::Terminal(path.clone()),
                ));
            } else if let Some(open) = url(&path) {
                actions.push(icon_button("document-open-symbolic", format!("Open {name}"), open));
            }
            actions.push(icon_button(
                "edit-copy-symbolic",
                format!("Copy the path of {name}"),
                Message::CopyPath(path.clone()),
            ));
            here.push(
                row::with_children(vec![
                    cosmic::widget::icon::from_name(if entry.directory {
                        "folder-symbolic"
                    } else {
                        "text-x-generic-symbolic"
                    })
                    .size(18)
                    .into(),
                    column::with_children(vec![
                        label(name, 14., p.foreground),
                        label(detail, 11., p.muted),
                    ])
                    .spacing(1)
                    .width(Length::Fill)
                    .into(),
                    row::with_children(actions).spacing(2).into(),
                ])
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        column::with_children(here).spacing(10).into()
    }

    fn missing_agenda(&self, p: Palette) -> Element<'static, Message> {
        let action = if let Some((reason, fix)) = self.calendar_needs_you() {
            format!("{reason} · {fix} →")
        } else if !self.calendar_connection.connected() {
            "Connect your calendar →".into()
        } else if self.calendar_problem.is_some() {
            "Check calendar connection →".into()
        } else {
            return label("Checking calendar…", 13., p.muted);
        };
        cosmic::widget::button::custom(label(&action, 13., p.accent))
            .name(action)
            .class(cosmic::theme::Button::Text)
            .padding([2, 0])
            .on_press(Message::CalendarSettings)
            .into()
    }

    pub(super) fn compact_agenda(
        &self,
        widget: Widget,
        days: &[NaiveDate],
        height: f32,
        p: Palette,
    ) -> Element<'_, Message> {
        let today = Local::now().date_naive();
        let mut entries = Vec::new();
        for &day in days {
            let agenda = if widget == Widget::Glance {
                self.glance_day_agenda(day)
            } else {
                self.calendar_agenda(day)
            };
            entries.extend(agenda.events.into_iter().map(|e| (day, e)));
        }
        // A timed appointment happening now or next takes priority over all-day reminders.
        let now = Local::now();
        entries.sort_by(|(a_day, a), (b_day, b)| {
            (a.finished_at(now), *a_day, a.all_day)
                .cmp(&(b.finished_at(now), *b_day, b.all_day))
                .then_with(|| a.chronological_order(b))
        });
        let capacity = ((height - 24.) / 23.).floor().max(1.) as usize;
        let mut rows = Vec::new();
        for (day, event) in entries.iter().take(capacity) {
            let time = event.time_label(self.config.clock.format_24h);
            let when = if *day == today {
                time
            } else {
                format!("{} {time}", day.format("%a"))
            };
            let title = if self.config.calendar_private {
                "Busy"
            } else {
                event.title.as_str()
            };
            rows.push(
                row::with_children(vec![
                    label(when, 11., p.accent),
                    summary_line(title, 14., p.foreground),
                ])
                .spacing(7)
                .align_y(Alignment::Center)
                .height(20.)
                .into(),
            );
        }
        if entries.is_empty() {
            if days
                .iter()
                .any(|day| self.calendar_month_data(*day).is_none())
            {
                return self.missing_agenda(p);
            }
            rows.push(label("No upcoming events", 13., p.muted));
        }
        let more = entries.len().saturating_sub(capacity);
        rows.push(action(
            if more > 0 {
                format!("{more} more · Open agenda")
            } else {
                "Open agenda".into()
            },
            Message::ReadingDetails(widget),
            p.accent,
        ));
        column::with_children(rows)
            .spacing(3)
            .width(Length::Fill)
            .into()
    }
}

/// A file size as people read it: 512 B, 3.4 KB, 12 MB.
pub(super) fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.;
    let mut unit = 0;
    while value >= 1024. && unit + 1 < UNITS.len() {
        value /= 1024.;
        unit += 1;
    }
    if value < 10. {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}
