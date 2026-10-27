//! Calendar recipes consume the existing cached agenda. Privacy is projected
//! before text or action identities enter the presentation snapshot.
use super::*;
#[derive(Clone, Default, Debug, PartialEq)]
pub(in crate::app) struct Entry {
    pub time: String,
    pub title: String,
    pub details: Option<String>,
}

pub(in crate::app) fn reading(
    config: &Config,
    data: Option<&CalendarData>,
    days: &[NaiveDate],
    now: chrono::DateTime<Local>,
    show_earlier: bool,
    connected: bool,
    failed: bool,
) -> Reading {
    let mut output = Reading {
        date: Some(
            days.first()
                .copied()
                .unwrap_or(now.date_naive())
                .format("%A, %-d %B")
                .to_string(),
        ),
        agenda_empty: if data.is_some() {
            "No upcoming events"
        } else if connected {
            "Loading your agenda…"
        } else {
            "Your agenda"
        }
        .into(),
        ..Default::default()
    };
    output.status = if !connected {
        Some("Connect a calendar in Settings".into())
    } else if failed {
        Some(
            if data.is_some() {
                "Couldn’t refresh · showing saved events"
            } else {
                "Couldn’t load events · check Calendar settings"
            }
            .into(),
        )
    } else {
        None
    };
    if let Some(data) = data {
        if output.status.is_none()
            && chrono::DateTime::parse_from_rfc3339(&data.updated_at)
                .is_ok_and(|t| (now - t.with_timezone(&Local)).num_hours() >= 24)
        {
            output.status = Some("Saved events · last refreshed over a day ago".into());
        }
        for day in days.iter().take(7) {
            for event in data.agenda_at(*day, now, show_earlier).events {
                output.agenda_total += 1;
                if output.agenda.len() >= 32 {
                    continue;
                }
                output.agenda.push(Entry {
                    time: if days.len() > 1 {
                        format!("{} · {}", day.format("%a"), event.time_label())
                    } else {
                        event.time_label()
                    },
                    title: if config.calendar_private {
                        "Busy".into()
                    } else {
                        event.title.chars().take(160).collect()
                    },
                    details: (!config.calendar_private).then(|| event.details_key()),
                });
            }
        }
    }
    output
}
impl Cards {
    pub(in crate::app) fn recipe_calendar(&self, recipe: &Recipe) -> Element<'static, Message> {
        let now = Local::now();
        let reading = reading(
            &self.config,
            self.calendar.as_ref(),
            &self.calendar_days(now.date_naive()),
            now,
            self.show_earlier,
            self.calendar_connection.connected(),
            self.calendar_error,
        );
        view(
            &self.config,
            recipe,
            now.naive_local(),
            reading,
            Context::Live,
            [
                self.widget_size(Widget::Calendar)[0],
                self.card_height(Widget::Calendar),
            ],
        )
    }
}
pub(in crate::app) fn sample(
    config: &Config,
    recipe: &Recipe,
    missing: bool,
    size: [f32; 2],
) -> Element<'static, Message> {
    let now = Local::now();
    let data = (!missing).then(super::super::settings_preview::SettingsSamples::calendar);
    let reading = reading(
        config,
        data.as_ref(),
        &[now.date_naive()],
        now,
        false,
        !missing,
        false,
    );
    view(
        config,
        recipe,
        now.naive_local(),
        reading,
        Context::LocalPreview,
        size,
    )
}
pub(super) fn part(
    reading: &Reading,
    width: f32,
    height: f32,
    p: Palette,
    font: Font,
    context: Context,
) -> Element<'static, Message> {
    let line = |text: String, size: f32, color| {
        text::body(text)
            .size(size)
            .font(font)
            .wrapping(cosmic::iced::advanced::text::Wrapping::None)
            .ellipsize(cosmic::iced::advanced::text::Ellipsize::End(
                cosmic::iced::advanced::text::EllipsizeHeightLimit::Lines(1),
            ))
            .class(cosmic::theme::Text::Color(color))
            .into()
    };
    if reading.agenda.is_empty() {
        return line(reading.agenda_empty.clone(), 14., p.muted);
    }
    let total = reading.agenda_total;
    let capacity = (height / 42.).floor().max(1.) as usize;
    let shown = if total > capacity && capacity > 1 {
        capacity - 1
    } else {
        capacity
    }
    .min(reading.agenda.len());
    let mut rows = Vec::new();
    for (i, entry) in reading.agenda.iter().take(shown).enumerate() {
        let body = column::with_children(vec![
            line(entry.time.clone(), 11., p.muted),
            line(entry.title.clone(), 14., p.foreground),
        ])
        .spacing(3)
        .width(Length::Fill);
        let content: Element<'static, Message> =
            if context == Context::Live && entry.details.is_some() {
                cosmic::widget::button::custom(body)
                    .padding(0)
                    .class(cosmic::theme::Button::Text)
                    .id(cosmic::widget::Id::new(format!(
                        "recipe-calendar-event-{i}"
                    )))
                    .on_press(Message::Open(entry.details.clone().unwrap()))
                    .width(Length::Fill)
                    .into()
            } else {
                body.into()
            };
        rows.push(container(content).height(36.).width(Length::Fill).into());
    }
    if total > shown && capacity > 1 {
        rows.push(line(format!("+{} more", total - shown), 11., p.muted));
    }
    container(column::with_children(rows).spacing(6))
        .width(width)
        .height(height)
        .clip(true)
        .into()
}
