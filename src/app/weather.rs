//! Immutable weather presentation. Time, readings and status are projected once;
//! drawing does not consult Config, the clock, a provider or another widget.
use super::*;
use crate::widget_presentation::{Context, Metrics};

#[derive(Clone, Copy)]
pub(super) struct Snapshot<'a> {
    pub weather: Option<&'a Weather>,
    pub updated_at: Option<&'a str>,
    pub failed: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct Forecast {
    title: String,
    icon: &'static str,
    low: String,
    high: String,
    range: Option<(f64, f64)>,
    condition: Option<&'static str>,
    /// `None` without a reading: the line is left out, not filled in.
    rain: Option<String>,
}
#[derive(Debug, Clone, PartialEq)]
enum Content {
    ChooseLocation,
    Loading,
    Unavailable,
    Reading {
        icon: &'static str,
        temperature: String,
        description: &'static str,
        summary: String,
        details: String,
        forecast: Vec<Forecast>,
        status: Option<String>,
    },
}

#[derive(Clone)]
pub(super) struct Presentation {
    recipe_config: Option<Config>,
    outlook: Option<outlook::Outlook>,
    now: chrono::NaiveDateTime,
    title: String,
    content: Content,
    palette: Palette,
    font: Font,
    appearance: Appearance,
    style: crate::widget_style::Context,
}

impl Presentation {
    pub fn new(config: &Config, snapshot: Snapshot<'_>, now: chrono::DateTime<Local>) -> Self {
        let _scope = crate::widget_style::scope(config);
        let resolved = crate::widget_style::presentation(config, config.palette(), false);
        let palette = resolved.palette();
        let content = match snapshot.weather {
            Some(w) => {
                let today = now.date_naive();
                let forecast = weather_forecast_days(&w.daily, today)
                    .take(4)
                    .map(|(i, day)| Forecast {
                        title: if day == today {
                            "Today".into()
                        } else if day == today + Days::days(1) {
                            "Tomorrow".into()
                        } else {
                            day.format("%a %-d").to_string()
                        },
                        icon: data::weather_icon(*w.daily.weather_code.get(i).unwrap_or(&3), true),
                        condition: w
                            .daily
                            .weather_code
                            .get(i)
                            .map(|code| data::weather_description(*code)),
                        low: forecast_temperature_in(
                            config.weather,
                            w.daily.temperature_2m_min.get(i),
                        ),
                        high: forecast_temperature_in(
                            config.weather,
                            w.daily.temperature_2m_max.get(i),
                        ),
                        range: outlook::limits(w, i),
                        rain: w
                            .daily
                            .precipitation_probability_max
                            .get(i)
                            .and_then(|v| *v)
                            .filter(|v| v.is_finite())
                            .map(|n| format!("Rain {n:.0}%")),
                    })
                    .collect();
                let wind = format!(
                    "Wind\u{a0}{}",
                    config.weather.wind(w.current.wind_speed_10m)
                );
                Content::Reading {
                    icon: data::weather_icon(w.current.weather_code, w.current.is_day != 0),
                    temperature: forecast_temperature_in(
                        config.weather,
                        Some(&w.current.temperature_2m),
                    ),
                    description: data::weather_description(w.current.weather_code),
                    // Written like the large reading: degrees without a unit letter.
                    summary: format!(
                        "Feels {} · Humidity {:.0}%",
                        forecast_temperature_in(
                            config.weather,
                            Some(&w.current.apparent_temperature)
                        ),
                        w.current.relative_humidity_2m
                    ),
                    details: weather_dew_point(config, &w.current)
                        .map(|dew| format!("{dew} · {wind}"))
                        .unwrap_or(wind),
                    forecast,
                    status: weather_status(config, snapshot.updated_at, snapshot.failed, now),
                }
            }
            None if config.location.is_none() => Content::ChooseLocation,
            None if snapshot.failed => Content::Unavailable,
            None => Content::Loading,
        };
        Self {
            recipe_config: config
                .recipes
                .weather
                .as_ref()
                .filter(|_| config.weather.layout == crate::config::WeatherLayout::Saved)
                .map(|_| config.clone()),
            outlook: snapshot
                .weather
                .filter(|_| config.weather.layout == crate::config::WeatherLayout::Outlook)
                .map(|w| outlook::Outlook::new(config, w, now.date_naive())),
            now: now.naive_local(),
            title: config
                .location
                .as_ref()
                .map_or_else(|| "Weather".into(), |l| l.name.clone()),
            content,
            palette,
            font: resolved.body.value,
            appearance: config.appearance,
            style: crate::widget_style::Context::capture(),
        }
    }

    pub fn metrics(&self) -> Metrics {
        crate::widget_presentation::metrics(Widget::Weather)
    }

    fn label(
        &self,
        value: impl Into<String>,
        size: f32,
        color: Color,
    ) -> Element<'static, Message> {
        text::body(value.into())
            .size(size.max(crate::widget_style::META))
            .line_height(1.25)
            .font(self.font)
            .class(cosmic::theme::Text::Color(crate::widget_style::semantic(
                color,
            )))
            .into()
    }
    pub fn view(&self, context: Context, size: [f32; 2]) -> Element<'static, Message> {
        let _scope = self.style.enter();
        if let Some(config) = &self.recipe_config {
            let mut reading = super::recipe::Reading {
                city: self.title.clone(),
                ..Default::default()
            };
            match &self.content {
                Content::Reading {
                    icon,
                    temperature,
                    description,
                    forecast,
                    status,
                    ..
                } => {
                    reading.temperature = format!("{icon} {temperature}");
                    reading.conditions = (*description).into();
                    reading.forecast = if forecast.is_empty() {
                        "Forecast unavailable".into()
                    } else {
                        forecast
                            .iter()
                            .take(2)
                            .map(|d| format!("{} {} / {}", d.title, d.high, d.low))
                            .collect::<Vec<_>>()
                            .join(" · ")
                    };
                    reading.status = status.clone();
                }
                Content::ChooseLocation => {
                    reading.temperature = "—".into();
                    reading.status = Some("Choose a city".into());
                }
                Content::Loading => {
                    reading.temperature = "—".into();
                    reading.status = Some("Checking the skies…".into());
                }
                Content::Unavailable => {
                    reading.temperature = "—".into();
                    reading.status = Some("Weather unavailable".into());
                }
            }
            return super::recipe::view(
                config,
                config.recipes.weather.as_ref().unwrap(),
                self.now,
                reading,
                context,
                size,
            );
        }
        let p = self.palette;
        let size = crate::widget_presentation::viewport(size, self.metrics().natural);
        if let (
            Some(outlook),
            Content::Reading {
                temperature,
                description,
                status,
                ..
            },
        ) = (&self.outlook, &self.content)
        {
            return outlook.view(self, size, temperature, description, status.as_deref());
        }
        let title = super::widget_details::summary_line(
            self.title.clone(),
            crate::widget_style::TITLE,
            crate::widget_style::semantic(p.foreground),
        );
        let (wide, mut warning) = match &self.content {
            Content::Reading {
                forecast, status, ..
            } => (
                size[0] >= 510. && !forecast.is_empty(),
                status.as_ref().map(|status| {
                    container(self.label(status.clone(), 11., p.muted))
                        .id(cosmic::widget::Id::new("weather-status"))
                        .into()
                }),
            ),
            _ => (false, None),
        };
        let mut rows = vec![if !wide && let Some(status) = warning.take() {
            spread(title, status)
        } else {
            title
        }];
        match &self.content {
            Content::Reading {
                icon,
                temperature,
                description,
                summary,
                details,
                forecast,
                ..
            } => {
                let padded_height = size[1] - 2. * f32::from(crate::widget_style::PADDING);
                // A fourth day is already in the provider response. Reveal it only
                // when there is room for another full forecast row.
                let count = if (wide && size[1] >= 224.) || size[1] >= 320. {
                    4
                } else {
                    3
                };
                let forecast = &forecast[..forecast.len().min(count)];
                let conditions = column::with_children(vec![
                    self.label(*description, 14., p.foreground),
                    self.label(summary.clone(), crate::widget_style::SECONDARY, p.muted),
                    container(self.label(details.clone(), crate::widget_style::SECONDARY, p.muted))
                        .id(cosmic::widget::Id::new("weather-details"))
                        .into(),
                ])
                .spacing(2);
                let reading = row::with_children(vec![
                    self.label(*icon, if wide { 40. } else { 32. }, p.accent),
                    self.label(
                        temperature.clone(),
                        if wide { 52. } else { 44. },
                        p.foreground,
                    ),
                ])
                .spacing(8)
                .align_y(Alignment::Center);

                if wide {
                    rows.push(reading.into());
                    rows.push(conditions.into());
                    let current = column::with_children(rows)
                        .spacing(3)
                        .width(Length::FillPortion(2));
                    let forecast = container(self.forecast_list(forecast, padded_height, warning))
                        .width(Length::FillPortion(3));
                    let body =
                        row::with_children(vec![current.into(), forecast.into()]).spacing(18);
                    return card_frame_width(
                        container(body)
                            .height(Length::Fill)
                            .align_y(Alignment::Center)
                            .into(),
                        p,
                        self.appearance,
                        Length::Fixed(size[1]),
                        Length::Fixed(size[0]),
                    );
                }

                rows.push(
                    row::with_children(vec![
                        reading.into(),
                        container(conditions).width(Length::Fill).into(),
                    ])
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into(),
                );
                if forecast.is_empty() {
                    rows.push(self.label(
                        "Forecast unavailable",
                        crate::widget_style::SECONDARY,
                        p.muted,
                    ));
                } else if size[1] >= 260. {
                    rows.push(self.forecast_list(forecast, padded_height - 99., None));
                    let body = column::with_children(rows).spacing(12).width(Length::Fill);
                    return card_frame_width(
                        container(body)
                            .height(Length::Fill)
                            .align_y(Alignment::Center)
                            .into(),
                        p,
                        self.appearance,
                        Length::Fixed(size[1]),
                        Length::Fixed(size[0]),
                    );
                } else {
                    let roomy = size[1] >= 205.;
                    if roomy {
                        rows.push(self.label("Forecast", crate::widget_style::TITLE, p.foreground));
                    }
                    let days = forecast
                        .iter()
                        .map(|day| {
                            let mut lines = vec![
                                row::with_children(vec![
                                    self.label(day.icon, 16., p.accent),
                                    self.label(
                                        day.title.clone(),
                                        crate::widget_style::SECONDARY,
                                        p.muted,
                                    ),
                                ])
                                .spacing(6)
                                .align_y(Alignment::Center)
                                .into(),
                                self.label(
                                    format!("{} / {}", day.high, day.low),
                                    13.,
                                    p.foreground,
                                ),
                            ];
                            if size[1] >= 226.
                                && let Some(condition) = day.condition
                            {
                                lines.push(self.label(condition, 12., p.muted));
                            }
                            if let Some(rain) = &day.rain {
                                lines.push(self.label(rain.clone(), 11., p.muted));
                            }
                            column::with_children(lines)
                                .spacing(2)
                                .width(Length::Fill)
                                .into()
                        })
                        .collect::<Vec<_>>();
                    rows.push(row::with_children(days).spacing(12).into());
                    if roomy {
                        return card_frame_width(
                            container(column::with_children(rows).spacing(7))
                                .height(Length::Fill)
                                .align_y(Alignment::Center)
                                .into(),
                            p,
                            self.appearance,
                            Length::Fixed(size[1]),
                            Length::Fixed(size[0]),
                        );
                    }
                }
            }
            Content::ChooseLocation => {
                let prompt = container(self.label("Choose your city →", 12., p.accent))
                    .padding([2, 8])
                    .align_x(Alignment::Center);
                let prompt: Element<'static, Message> = if context == Context::Live {
                    mouse_area(prompt)
                        .on_press(Message::WeatherSettings)
                        .interaction(cosmic::iced::mouse::Interaction::Pointer)
                        .into()
                } else {
                    prompt.into()
                };
                rows.push(
                    container(prompt)
                        .id(cosmic::widget::Id::new("weather-location-action"))
                        .into(),
                );
            }
            Content::Loading => rows.push(self.label("Checking the skies…", 14., p.muted)),
            Content::Unavailable => {
                rows.push(self.label("Weather unavailable — retrying soon", 14., p.muted))
            }
        }
        card_sized(
            rows,
            p,
            self.appearance,
            Widget::Weather,
            size[1],
            Length::Fixed(size[0]),
        )
    }
    /// The bars share the coldest low and warmest high among the visible days.
    fn forecast_list(
        &self,
        forecast: &[Forecast],
        height: f32,
        warning: Option<Element<'static, Message>>,
    ) -> Element<'static, Message> {
        let p = self.palette;
        let limits = forecast
            .iter()
            .filter_map(|d| d.range)
            .reduce(|(lo, hi), (next_lo, next_hi)| (lo.min(next_lo), hi.max(next_hi)));
        // Reserve the same width for endpoint labels on every day, including
        // mixed one-digit and below-zero readings, so the rails share an axis.
        let endpoint_width = |label: &str| 8. * label.chars().count() as f32 + 4.;
        let low_width = forecast
            .iter()
            .map(|day| endpoint_width(&day.low))
            .fold(0., f32::max);
        let high_width = forecast
            .iter()
            .map(|day| endpoint_width(&day.high))
            .fold(0., f32::max);
        let mut rows = vec![spread(
            self.label("Forecast", crate::widget_style::TITLE, p.foreground),
            warning.unwrap_or_else(|| self.label("Low – High", 12., p.muted)),
        )];
        for day in forecast {
            let mut name =
                column::with_children(vec![self.label(day.title.clone(), 15., p.foreground)]);
            if let Some(condition) = day.condition {
                name = name.push(self.label(condition, 12., p.muted));
            }
            let mut range = vec![
                container(self.label(day.low.clone(), 14., p.foreground))
                    .width(low_width)
                    .align_x(Alignment::End)
                    .into(),
            ];
            if let (Some((low, high)), Some((floor, ceiling))) = (day.range, limits)
                && ceiling > floor
            {
                let position = |v| ((v - floor) / (ceiling - floor)) as f32;
                range.push(outlook::rail(position(low), position(high), None, p));
            } else {
                range.push(
                    container(cosmic::widget::Space::new())
                        .width(Length::Fill)
                        .align_x(Alignment::Center)
                        .into(),
                );
            }
            range.push(
                container(self.label(day.high.clone(), 14., p.foreground))
                    .width(high_width)
                    .align_x(Alignment::End)
                    .into(),
            );
            let mut values = column::with_children(vec![
                row::with_children(range)
                    .spacing(5)
                    .align_y(Alignment::Center)
                    .into(),
            ])
            .width(Length::FillPortion(1))
            .align_x(Alignment::End);
            if let Some(rain) = &day.rain {
                values = values.push(self.label(rain.clone(), 12., p.muted));
            }
            rows.push(
                row::with_children(vec![
                    container(self.label(day.icon, 21., p.accent))
                        .width(25)
                        .into(),
                    name.width(Length::FillPortion(1)).into(),
                    values.into(),
                ])
                .spacing(7)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        let days = forecast.len() as f32;
        // City and forecast headings align. Distribute surplus space between
        // the days, with a cap to keep adjacent rows visually related.
        let spacing = ((height - 23. - 35. * days) / days).clamp(2., 14.);
        column::with_children(rows)
            .spacing(spacing)
            .width(Length::Fill)
            .into()
    }
}

mod outlook;
#[cfg(test)]
mod tests;
