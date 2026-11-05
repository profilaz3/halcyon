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
    temperatures: String,
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
                    .take(3)
                    .map(|(i, day)| Forecast {
                        title: if day == today {
                            "Today".into()
                        } else if day == today + Days::days(1) {
                            "Tomorrow".into()
                        } else {
                            day.format("%a %-d").to_string()
                        },
                        icon: data::weather_icon(*w.daily.weather_code.get(i).unwrap_or(&3), true),
                        temperatures: format!(
                            "{} / {}",
                            forecast_temperature_in(
                                config.weather,
                                w.daily.temperature_2m_max.get(i)
                            ),
                            forecast_temperature_in(
                                config.weather,
                                w.daily.temperature_2m_min.get(i)
                            )
                        ),
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
            recipe_config: config.recipes.weather.as_ref().map(|_| config.clone()),
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
                            .map(|d| format!("{} {}", d.title, d.temperatures))
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
        let title = self.label(self.title.clone(), crate::widget_style::TITLE, p.foreground);
        let mut rows = vec![match &self.content {
            Content::Reading {
                status: Some(status),
                ..
            } => spread(
                title,
                container(self.label(status.clone(), 11., p.muted))
                    .id(cosmic::widget::Id::new("weather-status"))
                    .into(),
            ),
            _ => title,
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
                rows.push(
                    row::with_children(vec![
                        self.label(*icon, 32., p.accent),
                        self.label(temperature.clone(), 44., p.foreground),
                        container(
                            column::with_children(vec![
                                self.label(*description, 12., p.foreground),
                                self.label(
                                    summary.clone(),
                                    crate::widget_style::SECONDARY,
                                    p.muted,
                                ),
                                container(self.label(
                                    details.clone(),
                                    crate::widget_style::SECONDARY,
                                    p.muted,
                                ))
                                .id(cosmic::widget::Id::new("weather-details"))
                                .into(),
                            ])
                            .spacing(2),
                        )
                        .width(Length::Fill)
                        .into(),
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
                } else {
                    rows.push(
                        row::with_children(
                            forecast
                                .iter()
                                .map(|day| {
                                    container(
                                        column::with_children(
                                            [
                                                // The icon sits beside its own day,
                                                // not at the column's far edge.
                                                Some(
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
                                                ),
                                                Some(self.label(
                                                    day.temperatures.clone(),
                                                    13.,
                                                    p.foreground,
                                                )),
                                                // 11px, not 12: a 12px line would
                                                // overflow the 180px card by a pixel.
                                                day.rain
                                                    .clone()
                                                    .map(|rain| self.label(rain, 11., p.muted)),
                                            ]
                                            .into_iter()
                                            .flatten(),
                                        )
                                        .spacing(2),
                                    )
                                    .width(Length::Fill)
                                    .into()
                                })
                                .collect::<Vec<_>>(),
                        )
                        .spacing(12)
                        .into(),
                    );
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
}

#[cfg(test)]
mod tests;
