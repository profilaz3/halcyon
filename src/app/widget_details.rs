//! Expanded readings live in a regular resizable window, leaving desktop cards stationary.
use super::*;
use cosmic::ApplicationExt;

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
