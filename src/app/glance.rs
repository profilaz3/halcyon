//! The unified, content-sized clock, weather, and current agenda surface.
use super::*;
#[cfg(test)]
use crate::config::GlanceStyle;
use crate::config::{ClockAlignment, GlanceDensity};
use cosmic::iced::Size;
use cosmic::iced::advanced::text::{self as advanced_text, Paragraph};
use cosmic::iced::widget::rich_text;
use std::collections::BTreeMap;

const ROW_GAP: f32 = 3.;
const DAY_HEADING_HEIGHT: f32 = 22.;

type AgendaSpan = advanced_text::Span<'static, String, Font>;

pub(super) fn agenda_spans(
    event: &CalendarEvent,
    foreground: Color,
    accent: Color,
) -> Vec<AgendaSpan> {
    agenda_spans_private(event, foreground, accent, false)
}

pub(super) fn agenda_spans_private(
    event: &CalendarEvent,
    foreground: Color,
    accent: Color,
    private: bool,
) -> Vec<AgendaSpan> {
    if private {
        return vec![AgendaSpan::new("Busy").color(foreground)];
    }
    let mut spans = vec![
        AgendaSpan::new(event.title.clone())
            .color(foreground)
            .link(event.details_key()),
    ];
    if let Some(url) = event.join_url() {
        spans.push(AgendaSpan::new("  "));
        spans.push(
            AgendaSpan::new("Join\u{a0}↗")
                .size(12.)
                .color(accent)
                .link(url.to_string()),
        );
    }
    spans
}

pub(super) fn agenda_text_height(event: &CalendarEvent, size: f32, width: f32) -> f32 {
    agenda_paragraph(event, size, width).min_bounds().height
}

fn agenda_paragraph(
    event: &CalendarEvent,
    size: f32,
    width: f32,
) -> <cosmic::Renderer as advanced_text::Renderer>::Paragraph {
    type NativeParagraph = <cosmic::Renderer as advanced_text::Renderer>::Paragraph;
    let spans = agenda_spans(event, Color::WHITE, Color::WHITE);
    NativeParagraph::with_spans(advanced_text::Text {
        content: &spans,
        bounds: Size::new(width.max(1.), f32::INFINITY),
        size: size.into(),
        line_height: advanced_text::LineHeight::Relative(1.25),
        font: crate::widget_style::font(),
        align_x: advanced_text::Alignment::Left,
        align_y: cosmic::iced::alignment::Vertical::Top,
        shaping: advanced_text::Shaping::Auto,
        wrapping: advanced_text::Wrapping::WordOrGlyph,
        ellipsize: advanced_text::Ellipsize::None,
    })
}

#[cfg(test)]
fn clock_ink_offset(time: &str, size: f32) -> f32 {
    super::clock::ink_offset(time, size)
}

fn forecast_index(daily: &data::Daily, day: NaiveDate) -> Option<usize> {
    daily
        .time
        .iter()
        .position(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok() == Some(day))
}

impl Cards {
    fn glance_has_sections(&self) -> bool {
        self.config.glance.show_clock
            || self.config.glance.show_weather
            || self.config.glance.show_calendar
    }
    fn glance_compact(&self) -> bool {
        self.config.glance.density == GlanceDensity::Compact
    }
    fn glance_panel(&self) -> bool {
        self.config.glance_panel()
    }
    fn glance_palette(&self) -> Palette {
        if !self.glance_panel() {
            return self.config.wallpaper_palette();
        }
        // A panel of its own under a see-through style (Bare) takes the
        // style's filled surface, not its transparent card.
        if self.config.transparent_widgets() {
            let mut surface = self.config.surface_palette();
            surface.background.a = self.config.palette().background.a.max(0.85);
            surface
        } else {
            self.config.palette()
        }
    }
    fn glance_padding(&self) -> f32 {
        if self.glance_panel() {
            if self.glance_compact() { 16. } else { 20. }
        } else {
            // A small transparent gutter also keeps optically shifted glyphs
            // inside the native surface's clipping rectangle.
            4.
        }
    }
    pub(super) fn glance_auto_width(&self) -> f32 {
        self.config
            .glance
            .auto_width(self.glance_panel())
            .min((self.widget_screen(Widget::Glance)[0] - 48.).max(1.))
    }
    pub(super) fn glance_width(&self) -> f32 {
        self.explicit_size(Widget::Glance)
            .map(|s| s[0])
            .unwrap_or_else(|| self.glance_auto_width())
    }
    fn glance_side_by_side(&self) -> bool {
        self.glance_panel()
            && self.config.glance.show_clock
            && self.config.glance.show_weather
            && self.config.clock.alignment == ClockAlignment::Left
    }
    fn glance_dew_point(&self) -> Option<String> {
        self.weather
            .as_ref()
            .and_then(|weather| weather_dew_point_degrees(&self.config, &weather.current))
    }
    fn glance_weather_width(&self) -> f32 {
        let minimum: f32 = if self.glance_compact() { 76. } else { 94. };
        let Some(weather) = &self.weather else {
            return minimum;
        };
        // Match the rendered row, including fallback/custom font advances and
        // negative temperatures. A fixed estimate clips the final small number.
        let font = crate::widget_style::font();
        let mut width = super::clock::measure(
            data::weather_icon(weather.current.weather_code, weather.current.is_day != 0),
            22.,
            font,
        ) + 6.
            + super::clock::measure(
                &format!("{:.0}°", weather.current.temperature_2m),
                28.,
                font,
            );
        if let Some(dew_point) = self.glance_dew_point() {
            width += 6. + super::clock::measure(&dew_point, 28. * 0.55, font);
        }
        if let Some(status) = self.glance_weather_status() {
            for line in status.lines() {
                width = width.max(super::clock::measure(line, 10., font));
            }
        }
        width.ceil().max(minimum)
    }

    fn glance_weather_status(&self) -> Option<String> {
        self.weather.as_ref()?;
        weather_status(
            &self.config,
            self.weather_at.as_deref(),
            self.weather_error,
            Local::now(),
        )
    }
    fn glance_weather_status_height(&self) -> f32 {
        self.glance_weather_status()
            .map(|status| status.lines().count() as f32 * 12.5 + 4.)
            .unwrap_or(0.)
    }
    fn glance_weather_block_height(&self) -> f32 {
        (if self.glance_compact() { 48. } else { 65. }) + self.glance_weather_status_height()
    }
    fn glance_clock_width(&self, width: f32) -> f32 {
        (width
            - self.glance_padding() * 2.
            - if self.glance_side_by_side() {
                self.glance_weather_width() + 8.
            } else {
                0.
            })
        .max(1.)
    }
    fn glance_clock_size_at_width(&self, width: f32) -> f32 {
        let base = match (self.glance_panel(), self.glance_compact()) {
            (true, true) => 60.,
            (true, false) => 76.,
            (false, true) => 72.,
            (false, false) => 104.,
        };
        super::clock::Presentation::grouped(&self.config, self.glance_palette()).fit(
            self.glance_clock_width(width),
            base * f32::from(self.config.clock.scale) / 100.,
        )
    }
    fn glance_clock_size(&self) -> f32 {
        self.glance_clock_size_at_width(self.glance_width())
    }
    fn glance_divider_height(&self) -> f32 {
        if self.glance_panel() { 20. } else { 14. }
    }
    fn glance_clock_row_height(&self) -> f32 {
        if self.glance_side_by_side() {
            self.glance_clock_height()
                .max(self.glance_weather_block_height())
        } else {
            self.glance_clock_height()
        }
    }
    fn glance_clock_height(&self) -> f32 {
        self.glance_clock_size() * 1.05
    }
    fn glance_date_height(&self) -> f32 {
        if self.config.glance.show_clock && self.config.clock.show_date {
            if self.glance_compact() {
                20.
            } else if self.glance_panel() {
                21.
            } else {
                27.
            }
        } else {
            0.
        }
    }
    fn glance_weather_height(&self) -> f32 {
        if !self.config.glance.show_weather {
            0.
        } else {
            (if self.glance_compact() { 22. } else { 43. }) + self.glance_weather_status_height()
        }
    }
    fn glance_header_height(&self) -> f32 {
        self.glance_header_height_at_width(self.glance_width())
    }
    fn glance_header_height_at_width(&self, width: f32) -> f32 {
        let preferences = &self.config.glance;
        let clock = if preferences.show_clock {
            self.glance_clock_size_at_width(width) * 1.05
        } else {
            0.
        };
        let date = self.glance_date_height();
        let weather = self.glance_weather_height();
        if self.glance_side_by_side() {
            clock.max(self.glance_weather_block_height()) + date
        } else if self.glance_compact() {
            clock + date.max(weather)
        } else {
            clock + date + weather
        }
    }
    pub(super) fn glance_days(&self, today: NaiveDate) -> Vec<NaiveDate> {
        if !self.config.glance.show_calendar {
            return Vec::new();
        }
        let mut days = vec![today];
        if self.config.glance.show_tomorrow {
            days.push(today + Days::days(1));
        }
        days
    }
    fn glance_calendar(&self) -> Option<&CalendarData> {
        let month = Local::now().format("%Y-%m").to_string();
        self.calendar_today
            .as_ref()
            .filter(|calendar| calendar.month == month)
            .or_else(|| {
                self.calendar
                    .as_ref()
                    .filter(|calendar| calendar.month == month)
            })
    }
    #[cfg(test)]
    pub(super) fn glance_events(&self, day: NaiveDate) -> Vec<&CalendarEvent> {
        self.glance_day_agenda(day).events
    }
    fn glance_day_agenda(&self, day: NaiveDate) -> data::CalendarAgenda<'_> {
        self.glance_calendar()
            .map(|calendar| calendar.agenda_at(day, Local::now(), self.show_earlier))
            .unwrap_or_default()
    }
    fn glance_title_width(&self, _event: &CalendarEvent, width: f32) -> f32 {
        // Time, marker, two gaps and the scrollbar gutter. Join shares the
        // title's paragraph; it never reserves a separate column.
        (width - self.glance_padding() * 2. - 44. - 2. - 16. - 4.).max(20.)
    }
    fn glance_event_font(&self) -> f32 {
        if self.glance_compact() && !self.glance_panel() {
            14.
        } else {
            15.
        }
    }
    fn glance_event_padding(&self) -> f32 {
        if self.glance_compact() && !self.glance_panel() {
            4.
        } else {
            6.
        }
    }
    fn glance_event_height(&self, event: &CalendarEvent) -> f32 {
        self.glance_event_height_at_width(event, self.glance_width())
    }
    fn glance_event_height_at_width(&self, event: &CalendarEvent, width: f32) -> f32 {
        let _style = crate::widget_style::scope(&self.config);
        let visible = if self.config.calendar_private {
            event.private_copy()
        } else {
            event.clone()
        };
        let title = agenda_paragraph(
            &visible,
            self.glance_event_font(),
            self.glance_title_width(event, width),
        )
        .min_bounds()
        .height;
        let status = matches!(
            event.meeting_state_at(Local::now(), self.config.meeting_notice_minutes),
            MeetingState::Soon(_) | MeetingState::Live
        );
        let detail = title + if status { 15. } else { 0. };
        let timing: f32 = if self.glance_compact() || event.all_day {
            20.
        } else {
            30.
        };
        detail.max(timing) + self.glance_event_padding() * 2. + self.event_details_height(event)
    }
    fn glance_agenda_height(&self) -> f32 {
        self.glance_agenda_height_at_width(self.glance_width())
    }
    fn glance_agenda_height_at_width(&self, width: f32) -> f32 {
        let days = self.glance_days(Local::now().date_naive());
        let mut rows = Vec::new();
        for day in &days {
            rows.push(DAY_HEADING_HEIGHT);
            let agenda = self.glance_day_agenda(*day);
            let events = agenda.events;
            if events.is_empty() {
                rows.push(36.);
            } else {
                rows.extend(
                    events
                        .into_iter()
                        .map(|event| self.glance_event_height_at_width(event, width)),
                );
            }
        }
        rows.iter().sum::<f32>() + rows.len().saturating_sub(1) as f32 * ROW_GAP
    }
    pub(super) fn glance_automatic_height(&self) -> f32 {
        self.glance_height_at_width(self.glance_auto_width())
    }
    pub(super) fn glance_natural_height(&self) -> f32 {
        self.glance_height_at_width(self.glance_width())
    }
    fn glance_height_at_width(&self, width: f32) -> f32 {
        if !self.glance_has_sections() {
            return 1.;
        }
        let header = self.glance_header_height_at_width(width);
        let agenda = if self.config.glance.show_calendar {
            self.glance_agenda_height_at_width(width)
                + if header > 0. {
                    self.glance_divider_height()
                } else {
                    0.
                }
        } else {
            0.
        };
        (header + agenda + 2. * self.glance_padding())
            .max(1.)
            .ceil()
    }
    pub(super) fn glance_position(&self, positions: &BTreeMap<String, [f32; 2]>) -> [f32; 2] {
        let size = [self.glance_width(), 1.];
        positions
            .get("glance")
            .map(|position| {
                let point = crate::layout_state::decode_position(
                    Widget::Glance,
                    *position,
                    size,
                    self.widget_screen(Widget::Glance),
                );
                if let Some(size) = self.explicit_size(Widget::Glance) {
                    crate::layout::clamp(point, size, self.widget_screen(Widget::Glance))
                } else {
                    point
                }
            })
            .unwrap_or_else(|| {
                crate::layout::clamp(
                    [82., 88.],
                    self.explicit_size(Widget::Glance).unwrap_or(size),
                    self.widget_screen(Widget::Glance),
                )
            })
    }
    pub(super) fn glance_size(&self) -> [f32; 2] {
        if let Some(size) = self.explicit_size(Widget::Glance) {
            return size;
        }
        if !self.glance_has_sections() {
            return [1., 1.];
        }
        let positions = if self.layout_editor.active {
            self.layout_editor.positions()
        } else {
            self.effective_positions()
        };
        let position = self.glance_position(positions);
        [
            self.glance_width(),
            self.glance_natural_height()
                .min((self.widget_screen(Widget::Glance)[1] - position[1] - 48.).max(1.)),
        ]
    }
    fn glance_needs_outer_scroll(&self, height: f32) -> bool {
        let header = self.glance_header_height();
        let divider = if header > 0. {
            self.glance_divider_height()
        } else {
            0.
        };
        self.config.glance.show_calendar
            && height - 2. * self.glance_padding() < header + divider + DAY_HEADING_HEIGHT + 36.
    }
    pub(super) fn seed_glance_position(&mut self) -> Option<Config> {
        if !self.output_known
            || !self.config.glance.enabled
            || self.config.widget_positions.contains_key("glance")
        {
            return None;
        }
        let clock = self.position_from(Widget::Clock, self.effective_positions());
        let encoded = crate::layout_state::encode_position(
            Widget::Glance,
            clock,
            [self.glance_width(), 1.],
            self.widget_screen(Widget::Glance),
        );
        match Config::seed_glance_position(encoded) {
            Ok(config) => Some(config),
            Err(error) => {
                eprintln!("Could not save initial Glance placement: {error}");
                self.config
                    .widget_positions
                    .insert("glance".into(), encoded);
                None
            }
        }
    }
    /// Normal layouts restrict pointer input to the weather chooser and agenda.
    /// The short-view outer scroller is interactive throughout: event and calendar
    /// actions can scroll into the area initially occupied by the clock.
    pub(super) fn glance_input_rects(&self) -> Vec<[u32; 4]> {
        if !self.config.glance.enabled || !self.glance_has_sections() {
            return Vec::new();
        }
        let [width, height] = self.glance_size();
        if self.glance_needs_outer_scroll(height) {
            return vec![[0, 0, width.ceil() as u32, height.ceil() as u32]];
        }
        let padding = self.glance_padding();
        let header = self.glance_header_height();
        let mut rects = Vec::new();
        let mut push = |x: f32, y: f32, w: f32, h: f32| {
            let y = y.max(0.);
            let h = h.min(height - y).max(0.);
            if w > 0. && h > 0. {
                rects.push([
                    x.max(0.).floor() as u32,
                    y.floor() as u32,
                    w.ceil() as u32,
                    h.ceil() as u32,
                ]);
            }
        };
        if self.config.glance.show_weather {
            if self.glance_side_by_side() {
                let weather_width = self.glance_weather_width();
                push(
                    width - padding - weather_width,
                    padding,
                    weather_width,
                    self.glance_clock_row_height(),
                );
            } else {
                let clock = if self.config.glance.show_clock {
                    self.glance_clock_height()
                } else {
                    0.
                };
                let date = if self.glance_compact() {
                    0.
                } else {
                    self.glance_date_height()
                };
                push(
                    padding,
                    padding + clock + date,
                    width - 2. * padding,
                    self.glance_weather_height(),
                );
            }
        }
        if self.config.glance.show_calendar {
            let start = padding
                + header
                + if header > 0. {
                    self.glance_divider_height()
                } else {
                    0.
                };
            push(padding, start, width - 2. * padding, height - start);
        }
        rects
    }
    fn glance_clock(&self) -> Element<'_, Message> {
        super::clock::Presentation::grouped(&self.config, self.glance_palette()).face(
            crate::widget_style::resolver::ClockReading::at(
                self.clock_at.time(),
                self.config.clock.format_24h,
            ),
            self.glance_clock_width(self.glance_width()),
            self.glance_clock_size(),
        )
    }
    fn glance_weather(&self, block: bool) -> Element<'_, Message> {
        let p = self.glance_palette();
        let compact = self.glance_compact();
        let dew_point = self.glance_dew_point();
        let mut rows = Vec::new();
        match (&self.config.location, &self.weather) {
            (_, Some(weather)) => {
                let forecast = forecast_index(&weather.daily, Local::now().date_naive());
                let temperature_size = if block {
                    28.
                } else if compact {
                    16.
                } else {
                    20.
                };
                let mut temperature = vec![
                    label(
                        data::weather_icon(
                            weather.current.weather_code,
                            weather.current.is_day != 0,
                        ),
                        if block { 22. } else { 18. },
                        p.accent,
                    ),
                    label(
                        format!("{:.0}°", weather.current.temperature_2m),
                        temperature_size,
                        p.foreground,
                    ),
                ];
                if let Some(dew_point) = dew_point {
                    temperature.push(
                        container(label(
                            dew_point,
                            (temperature_size * 0.55).max(10.),
                            p.muted,
                        ))
                        .id(cosmic::widget::Id::new("glance-dew-point"))
                        .into(),
                    );
                }
                rows.push(
                    row::with_children(temperature)
                        .spacing(6)
                        .align_y(Alignment::Center)
                        .into(),
                );
                if !compact {
                    let detail = if block {
                        data::weather_description(weather.current.weather_code).to_string()
                    } else {
                        let high = forecast_temperature(
                            weather
                                .daily
                                .temperature_2m_max
                                .get(forecast.unwrap_or(usize::MAX)),
                        );
                        let low = forecast_temperature(
                            weather
                                .daily
                                .temperature_2m_min
                                .get(forecast.unwrap_or(usize::MAX)),
                        );
                        let rain = weather
                            .daily
                            .precipitation_probability_max
                            .get(forecast.unwrap_or(usize::MAX))
                            .and_then(|v| *v)
                            .map(|v| format!(" · Rain {v:.0}%"))
                            .unwrap_or_default();
                        format!(
                            "{} · {high} / {low}{rain}",
                            data::weather_description(weather.current.weather_code)
                        )
                    };
                    rows.push(
                        text::body(detail)
                            .size(if block { 11. } else { 12. })
                            .line_height(1.25)
                            .font(crate::widget_style::font())
                            .wrapping(advanced_text::Wrapping::None)
                            .ellipsize(advanced_text::Ellipsize::End(
                                advanced_text::EllipsizeHeightLimit::Lines(1),
                            ))
                            .class(cosmic::theme::Text::Color(p.muted))
                            .into(),
                    );
                    if block {
                        let high = forecast_temperature(
                            weather
                                .daily
                                .temperature_2m_max
                                .get(forecast.unwrap_or(usize::MAX)),
                        );
                        let low = forecast_temperature(
                            weather
                                .daily
                                .temperature_2m_min
                                .get(forecast.unwrap_or(usize::MAX)),
                        );
                        rows.push(label(format!("{high} / {low}"), 11., p.muted));
                    }
                }
            }
            (Some(_), None) => rows.push(label(
                if self.weather_error {
                    "Weather unavailable"
                } else {
                    "Checking the skies…"
                },
                11.,
                p.muted,
            )),
            (None, None) => rows.push(label("Choose city ↗", 12., p.accent)),
        }
        let status = self.glance_weather_status();
        if let Some(status) = &status {
            rows.push(
                container(label(status.clone(), 10., p.muted))
                    .id(cosmic::widget::Id::new("glance-weather-status"))
                    .into(),
            );
        }
        let content: Element<'_, Message> = if block {
            column::with_children(rows)
                .spacing(2)
                .align_x(Alignment::End)
                .into()
        } else if compact && status.is_none() {
            row::with_children(rows)
                .spacing(6)
                .align_y(Alignment::Center)
                .into()
        } else {
            // Keep the weather aligned to the same optical edge as the date.
            column::with_children(rows).spacing(1).into()
        };
        mouse_area(content)
            .on_press(Message::WeatherSettings)
            .interaction(cosmic::iced::mouse::Interaction::Pointer)
            .into()
    }
    fn glance_header(&self) -> Element<'_, Message> {
        let p = self.glance_palette();
        let preferences = &self.config.glance;
        let compact = self.glance_compact();
        let mut rows = Vec::new();
        let date = || {
            label(
                (if self.config.glance.show_clock {
                    self.clock_at
                } else {
                    Local::now()
                })
                .format(if compact { "%a, %-d %b" } else { "%A, %B %-d" })
                .to_string(),
                if compact {
                    16.
                } else if self.glance_panel() {
                    15.
                } else {
                    20.
                },
                p.muted,
            )
        };
        if self.glance_side_by_side() {
            let weather_width = self.glance_weather_width();
            rows.push(
                row::with_children(vec![
                    container(self.glance_clock()).width(Length::Fill).into(),
                    container(self.glance_weather(true))
                        .width(weather_width)
                        .align_y(Alignment::Center)
                        .height(self.glance_clock_row_height())
                        .into(),
                ])
                .spacing(8)
                .height(self.glance_clock_row_height())
                .into(),
            );
            if self.config.clock.show_date {
                let date = if !compact {
                    spread(
                        date(),
                        label(
                            self.config
                                .location
                                .as_ref()
                                .map(|location| location.name.as_str())
                                .unwrap_or(""),
                            11.,
                            p.muted,
                        ),
                    )
                } else {
                    date()
                };
                rows.push(container(date).height(self.glance_date_height()).into());
            }
        } else {
            if preferences.show_clock {
                rows.push(self.glance_clock());
            }
            if compact {
                let mut sub = Vec::new();
                if preferences.show_clock && self.config.clock.show_date {
                    sub.push(date());
                }
                if preferences.show_weather {
                    if !sub.is_empty() {
                        sub.push(label("·", 14., p.muted));
                    }
                    sub.push(self.glance_weather(false));
                }
                if !sub.is_empty() {
                    rows.push(
                        container(
                            row::with_children(sub)
                                .spacing(8)
                                .align_y(Alignment::Center),
                        )
                        .width(Length::Fill)
                        .align_x(self.config.clock.alignment.layout())
                        .height(self.glance_date_height().max(self.glance_weather_height()))
                        .into(),
                    );
                }
            } else {
                if preferences.show_clock && self.config.clock.show_date {
                    rows.push(
                        container(date())
                            .width(Length::Fill)
                            .align_x(self.config.clock.alignment.layout())
                            .height(self.glance_date_height())
                            .into(),
                    );
                }
                if preferences.show_weather {
                    rows.push(
                        container(self.glance_weather(false))
                            .width(Length::Fill)
                            .align_x(self.config.clock.alignment.layout())
                            .height(self.glance_weather_height())
                            .into(),
                    );
                }
            }
        }
        column::with_children(rows)
            .height(self.glance_header_height())
            .width(Length::Fill)
            .into()
    }
    fn glance_event(&self, event: &CalendarEvent) -> Element<'_, Message> {
        let p = self.glance_palette();
        let state = event.meeting_state_at(Local::now(), self.config.meeting_notice_minutes);
        let highlighted = matches!(state, MeetingState::Soon(_) | MeetingState::Live);
        let color = if matches!(state, MeetingState::Soon(_)) {
            crate::widget_style::semantic(Color::from_rgb8(245, 190, 107))
        } else {
            p.accent
        };
        let mut timing = vec![label(
            event.time_label(),
            12.,
            if event.finished() { p.muted } else { color },
        )];
        if !self.glance_compact() && !event.all_day {
            let end = chrono::DateTime::parse_from_rfc3339(&event.end)
                .map(|date| date.with_timezone(&Local).format("%H:%M").to_string())
                .unwrap_or_default();
            timing.push(label(end, 10., p.muted));
        }
        let mut detail = vec![
            rich_text(agenda_spans_private(
                event,
                if event.finished() {
                    p.muted
                } else {
                    p.foreground
                },
                color,
                self.config.calendar_private,
            ))
            .size(self.glance_event_font())
            .line_height(1.25)
            .font(crate::widget_style::font())
            .wrapping(advanced_text::Wrapping::WordOrGlyph)
            .width(Length::Fill)
            .on_link_click(Message::Open)
            .into(),
        ];
        if highlighted {
            detail.push(label(
                match state {
                    MeetingState::Soon(minutes) => format!("IN {minutes} MIN"),
                    _ => "LIVE NOW".into(),
                },
                10.,
                color,
            ));
        }
        let marker_color = event.source_color().unwrap_or(color);
        let marker = container(text::body(""))
            .width(2.)
            .height(if self.glance_compact() { 20. } else { 26. })
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    background: Some(Background::Color(Color {
                        a: if highlighted { 0.9 } else { 0.3 },
                        ..marker_color
                    })),
                    border: Border {
                        radius: 1.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }));
        let title = container(column::with_children(detail).spacing(2))
            .max_width(self.glance_title_width(event, self.glance_width()))
            .width(Length::Fill);
        let cells = vec![
            container(column::with_children(timing).spacing(1))
                .width(44.)
                .into(),
            marker.into(),
            title.into(),
        ];
        let panel = self.glance_panel();
        let mut content = vec![
            row::with_children(cells)
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
        ];
        if self.event_details_open(event) {
            content.push(self.event_details_view(event, p));
        }
        container(column::with_children(content).spacing(0))
            .padding([self.glance_event_padding(), 0.])
            .width(Length::Fill)
            .height(self.glance_event_height(event))
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    background: (panel && highlighted)
                        .then_some(Background::Color(Color { a: 0.1, ..color })),
                    border: Border {
                        color: Color {
                            a: if panel && highlighted { 0.45 } else { 0. },
                            ..color
                        },
                        width: 1.,
                        radius: 10.into(),
                    },
                    ..Default::default()
                }
            }))
            .into()
    }
    fn glance_agenda(&self) -> Element<'_, Message> {
        let p = self.glance_palette();
        let today = Local::now().date_naive();
        let days = self.glance_days(today);
        let mut rows = Vec::new();
        for day in &days {
            rows.push(self.glance_day_heading(*day, today));
            let agenda = self.glance_day_agenda(*day);
            let events = agenda.events;
            if events.is_empty() {
                if self.glance_calendar().is_none()
                    && (!self.calendar_connection.connected() || self.calendar_today_error)
                {
                    rows.push(
                        container(action(
                            if self.calendar_connection.connected() {
                                "Check calendar connection →"
                            } else {
                                "Connect your calendar →"
                            },
                            Message::CalendarSettings,
                            p.accent,
                        ))
                        .height(36.)
                        .align_y(Alignment::Center)
                        .into(),
                    );
                    continue;
                }
                let value = if self.glance_calendar().is_none() {
                    if self.calendar_today_error {
                        "Calendar unavailable · retrying soon"
                    } else {
                        "Syncing your calendar…"
                    }
                } else if agenda.earlier > 0 {
                    "All done for today"
                } else if *day == today {
                    "No events today"
                } else {
                    "No events tomorrow"
                };
                rows.push(
                    container(label(value, 14., p.muted))
                        .height(36.)
                        .align_y(Alignment::Center)
                        .into(),
                );
            } else {
                rows.extend(events.into_iter().map(|event| self.glance_event(event)));
            }
        }
        column::with_children(rows)
            .spacing(ROW_GAP)
            .width(Length::Fill)
            .into()
    }
    fn glance_day_heading(&self, day: NaiveDate, today: NaiveDate) -> Element<'_, Message> {
        let p = self.glance_palette();
        let mut url = reqwest::Url::parse(&format!(
            "https://calendar.google.com/calendar/u/0/r/day/{}/{}/{}",
            day.format("%Y"),
            day.format("%-m"),
            day.format("%-d")
        ))
        .unwrap();
        if let Some(calendar) = self.glance_calendar() {
            if !calendar.account.is_empty() {
                url.query_pairs_mut()
                    .append_pair("authuser", &calendar.account);
            }
        }
        let google_link = self.calendar_connection.backend == crate::calendar::Backend::Google
            && !self.config.calendar_private;
        let heading: Element<'_, Message> = if google_link {
            mouse_area(label(
                if day == today {
                    "TODAY ↗"
                } else {
                    "TOMORROW ↗"
                },
                10.,
                p.accent,
            ))
            .on_press(Message::Open(url.into()))
            .interaction(cosmic::iced::mouse::Interaction::Pointer)
            .into()
        } else {
            label(
                if day == today { "TODAY" } else { "TOMORROW" },
                10.,
                p.accent,
            )
        };
        // Earlier events toggle beside the day name instead of taking a row.
        let earlier = self.glance_day_agenda(day).earlier;
        let heading = if earlier > 0 {
            row::with_children(vec![heading, self.earlier_inline(earlier, p)])
                .spacing(6)
                .align_y(Alignment::Center)
                .into()
        } else {
            heading
        };
        let content = if day == today && self.glance_panel() {
            spread(
                heading,
                timestamp(
                    self.glance_calendar().map(|c| c.updated_at.as_str()),
                    self.calendar_today_error,
                    "Calendar",
                    p,
                ),
            )
        } else {
            heading
        };
        container(content)
            .height(DAY_HEADING_HEIGHT)
            .align_y(Alignment::Center)
            .into()
    }
    pub(super) fn glance(&self) -> Element<'_, Message> {
        let _style = crate::widget_style::scope(&self.config);
        let _fonts = crate::widget_style::fonts(
            self.config.glance.font.as_deref(),
            self.config.glance.clock_font.as_deref(),
        );
        if !self.glance_has_sections() {
            return empty_surface();
        }
        let p = self.glance_palette();
        let [width, height] = self.glance_size();
        let padding = self.glance_padding();
        let header_height = self.glance_header_height();
        let mut content = vec![self.glance_header()];
        let divider = if header_height > 0. && self.config.glance.show_calendar {
            self.glance_divider_height()
        } else {
            0.
        };
        if divider > 0. {
            let panel = self.glance_panel();
            let rule = container(text::body(""))
                .width(if panel {
                    Length::Fill
                } else {
                    Length::Fixed(56.)
                })
                .height(1.)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        background: Some(Background::Color(Color {
                            a: if panel { 0.13 } else { 0.45 },
                            ..p.accent
                        })),
                        ..Default::default()
                    }
                }));
            content.push(
                container(rule)
                    .height(divider)
                    .align_y(Alignment::Center)
                    .into(),
            );
        }
        let short = self.glance_needs_outer_scroll(height);
        if self.config.glance.show_calendar {
            if short {
                content.push(self.glance_agenda());
            } else {
                let available = (height - 2. * padding - header_height - divider).max(1.);
                content.push(
                    scrollable(
                        container(self.glance_agenda())
                            .padding([0, 4, 0, 0])
                            .width(Length::Fill),
                    )
                    .id(cosmic::widget::Id::new("glance-agenda"))
                    .height(self.glance_agenda_height().min(available))
                    .width(Length::Fill)
                    .scroller_width(2.)
                    .scrollbar_width(2.)
                    .scrollbar_padding(1.)
                    .into(),
                );
            }
        }
        let content: Element<'_, Message> =
            column::with_children(content).width(Length::Fill).into();
        let content = if short {
            scrollable(content)
                .id(cosmic::widget::Id::new("glance-small-screen"))
                .height(Length::Fill)
                .width(Length::Fill)
                .scroller_width(2.)
                .scrollbar_width(2.)
                .into()
        } else {
            content
        };
        let panel = self.glance_panel();
        let opacity = f32::from(self.config.glance.opacity.min(100)) / 100.;
        let opacity = if self.config.appearance.contrast_assist {
            opacity.max(p.background.a)
        } else {
            opacity
        };
        let themed = self.config.moment_appearance().family.is_some()
            || self.config.materials.widgets().is_some();
        let opacity = if themed { p.background.a } else { opacity };
        let outline = if themed {
            self.config.appearance.border_opacity as f32 / 100.
        } else {
            0.14
        };
        let radius: f32 = if themed {
            crate::widget_style::corner_radius(&self.config)
        } else if self.glance_compact() {
            20.
        } else {
            24.
        };
        let edge = crate::widget_style::edge(Palette {
            background: Color {
                a: if panel { opacity } else { 0. },
                ..p.background
            },
            ..p
        });
        let background = crate::widget_style::background(Color {
            a: opacity,
            ..p.background
        });
        let material_border = crate::widget_style::border(Color {
            a: outline,
            ..p.accent
        });
        let content = container(column::with_children(vec![content]).width(Length::Fill))
            .padding(padding)
            .width(width)
            .height(height)
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    text_color: Some(p.foreground),
                    icon_color: Some(p.foreground),
                    background: panel.then_some(background),
                    border: Border {
                        color: if panel {
                            material_border
                        } else {
                            Color::TRANSPARENT
                        },
                        width: if panel { edge.0 } else { 0. },
                        radius: radius.into(),
                    },
                    shadow: edge.1,
                    ..Default::default()
                }
            }))
            .into();
        crate::widget_style::surface(
            content,
            Palette {
                background: Color {
                    a: if panel { opacity } else { 0. },
                    ..p.background
                },
                ..p
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::Application;

    fn app() -> Cards {
        let (mut cards, _) = Cards::init(
            Core::default(),
            Config {
                enabled_widgets: vec![],
                ..Config::default()
            },
        );
        cards.config.glance.enabled = true;
        cards.config.glance.initialized = true;
        cards
    }

    #[test]
    fn glance_can_stay_transparent_under_a_panel_widget_style() {
        use crate::desktop_theme::Family;
        let mut cards = app();
        cards.config.appearance.family = Some(Family::Paper);
        cards.config.glance.style = GlanceStyle::Transparent;
        // By default the widget style's surface decides.
        assert!(cards.glance_panel());
        assert!(cards.glance_palette().background.a > 0.);
        cards.config.glance.own_background = true;
        assert!(!cards.glance_panel());
        assert_eq!(cards.glance_palette().background.a, 0.);
        assert_eq!(cards.glance_palette(), cards.config.wallpaper_palette());
        assert_eq!(cards.glance_auto_width(), 280.);
        // Other widgets keep the style's panel.
        assert!(cards.config.palette().background.a > 0.);
        // And the reverse: a panel glance under a transparent style.
        cards.config.appearance.family = Some(Family::Bare);
        cards.config.glance.style = GlanceStyle::Panel;
        assert!(cards.glance_panel());
        // Its panel is filled, though Bare's own cards are see-through.
        assert!(cards.glance_palette().background.a >= 0.85);
        cards.config.glance.own_background = false;
        assert!(!cards.glance_panel());
    }

    #[test]
    fn glance_fonts_override_the_widget_fonts_only_inside_the_glance() {
        use cosmic::iced::font::Family;
        let mut config = Config {
            widget_font: Some("Inter".into()),
            clock_font: Some("Lora".into()),
            ..Config::default()
        };
        let _style = crate::widget_style::scope(&config);
        {
            let _fonts = crate::widget_style::fonts(None, None);
            assert_eq!(crate::widget_style::font().family, Family::Name("Inter"));
            assert_eq!(
                crate::widget_style::clock_font().family,
                Family::Name("Lora")
            );
        }
        config.glance.font = Some("Hack".into());
        {
            let _fonts = crate::widget_style::fonts(config.glance.font.as_deref(), None);
            assert_eq!(crate::widget_style::font().family, Family::Name("Hack"));
            // The glance clock follows the glance's own font.
            assert_eq!(
                crate::widget_style::clock_font().family,
                Family::Name("Hack")
            );
            let _clock = crate::widget_style::fonts(None, Some("Merriweather"));
            assert_eq!(
                crate::widget_style::clock_font().family,
                Family::Name("Merriweather")
            );
        }
        assert_eq!(crate::widget_style::font().family, Family::Name("Inter"));
        assert_eq!(
            crate::widget_style::clock_font().family,
            Family::Name("Lora")
        );
    }

    #[test]
    fn dew_point_fits_overview_header_at_small_widths_and_clock_sizes() {
        use crate::native_test::Native;
        let mut cards = app();
        cards.config.glance.show_calendar = false;
        cards.config.weather.show_dew_point = true;
        cards.weather = Some(serde_json::from_value(serde_json::json!({
            "current":{"temperature_2m":-10.0,"apparent_temperature":-15.0,"weather_code":2,
                "wind_speed_10m":9.0,"relative_humidity_2m":62.0,"dew_point_2m":-25.4,"is_day":1},
            "daily":{"time":[],"temperature_2m_max":[],"temperature_2m_min":[],
                "weather_code":[],"precipitation_probability_max":[]}
        })).unwrap());
        for family in [None, Some("DejaVu Sans")] {
            cards.config.widget_font = family.map(str::to_owned);
            let _style = crate::widget_style::scope(&cards.config);
            for style in [GlanceStyle::Transparent, GlanceStyle::Panel] {
                cards.config.glance.style = style;
                for density in [GlanceDensity::Compact, GlanceDensity::Comfortable] {
                    cards.config.glance.density = density;
                    for width in [260., 360.] {
                        cards
                            .config
                            .widget_sizes
                            .insert("glance".into(), [width, 300.]);
                        for scale in [50, 100] {
                            cards.config.clock.scale = scale;
                            let height = cards.glance_header_height();
                            let mut native = Native::new(Size::new(width, height));
                            let dew = native.bounds(cards.glance_header(), "glance-dew-point");
                            assert!(
                                dew.x >= 0.
                                && dew.y >= 0.
                                && dew.x + dew.width <= width + 0.5
                                && dew.y + dew.height <= height + 0.5
                                // One small line beside the temperature,
                                // wide enough for "-25°" without clipping.
                                && dew.height < 20.
                                && dew.width > 18.,
                                "{style:?}/{density:?}/{width}/{scale}: {dew:?}, height={height}"
                            );
                            if scale == 100 {
                                native.capture(
                                    cards.glance_header(),
                                    &cosmic::Theme::dark(),
                                    &format!("dew-point-overview-{style:?}-{density:?}-{width}"),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn earlier_toggle_sits_beside_the_day_heading() {
        use crate::native_test::Native;
        use chrono::TimeZone;
        let mut cards = app();
        let now = Local::now();
        let today = now.date_naive();
        let midnight = Local
            .from_local_datetime(&today.and_hms_opt(0, 0, 0).unwrap())
            .unwrap();
        cards.calendar = Some(CalendarData {
            month: today.format("%Y-%m").to_string(),
            updated_at: now.to_rfc3339(),
            events: vec![CalendarEvent {
                title: "Morning review".into(),
                start: midnight.to_rfc3339(),
                end: midnight.to_rfc3339(),
                all_day: false,
                url: String::new(),
                meeting_url: None,
                ..Default::default()
            }],
            ..Default::default()
        });
        cards.calendar_today = cards.calendar.clone();
        cards.show_earlier = false;
        cards.config.glance.show_clock = false;
        cards.config.glance.show_weather = false;
        cards.config.glance.show_calendar = true;
        cards.config.glance.show_tomorrow = false;
        for backend in [
            crate::calendar::Backend::Subscription,
            crate::calendar::Backend::Google,
        ] {
            cards.calendar_connection.backend = backend.clone();
            for style in [GlanceStyle::Transparent, GlanceStyle::Panel] {
                cards.config.glance.style = style;
                let [width, height] = cards.glance_size();
                let mut native = Native::new(Size::new(width, height));
                native.capture(
                    cards.glance(),
                    &cosmic::Theme::dark(),
                    &format!("glance-earlier-inline-{backend:?}-{style:?}"),
                );
                let earlier = native.bounds(cards.glance(), "calendar-earlier");
                // On the TODAY line, not a row of its own below the day.
                assert!(
                    earlier.y + earlier.height <= cards.glance_padding() + DAY_HEADING_HEIGHT + 1.,
                    "{style:?}: {earlier:?}"
                );
                let day = native.text_bounds(
                    cards.glance(),
                    if backend == crate::calendar::Backend::Google {
                        "TODAY ↗"
                    } else {
                        "TODAY"
                    },
                );
                assert!(
                    earlier.x >= day.x + day.width + 4.,
                    "Earlier stays beside the actual heading: {day:?} {earlier:?}"
                );
            }
        }
    }

    #[test]
    fn inline_join_wraps_with_the_title_and_keeps_distinct_click_targets() {
        use crate::native_test::Native;
        use cosmic::iced::{Event, Point, mouse};
        let mut cards = app();
        let today = Local::now().date_naive();
        let theme = cosmic::Theme::dark();
        for style in [GlanceStyle::Transparent, GlanceStyle::Panel] {
            cards.config.glance.style = style;
            for width in [260., 280., 360.] {
                cards
                    .config
                    .widget_sizes
                    .insert("glance".into(), [width, 300.]);
                for title in [
                    "Daily sync",
                    "Review the release plan and resolve all remaining integration issues together",
                ] {
                    let event = CalendarEvent {
                        title: title.into(),
                        start: today.to_string(),
                        end: (today + Days::days(1)).to_string(),
                        all_day: true,
                        url: "https://calendar.google.com/event?fixture".into(),
                        meeting_url: Some("https://meet.google.com/fixture".into()),
                        ..Default::default()
                    };
                    let title_width = cards.glance_title_width(&event, width);
                    let paragraph =
                        agenda_paragraph(&event, cards.glance_event_font(), title_width);
                    let title_height = paragraph.min_bounds().height;
                    let height = cards.glance_event_height(&event);
                    let row_width = width - 2. * cards.glance_padding() - 4.;
                    let mut native = Native::new(Size::new(row_width, height));
                    let details_key = event.details_key();
                    for (span, expected) in
                        [(0, details_key.as_str()), (2, event.join_url().unwrap())]
                    {
                        let bounds = paragraph.span_bounds(span);
                        assert!(!bounds.is_empty());
                        let r = bounds[0];
                        let point = Point::new(
                            62. + r.center_x(),
                            (height - title_height) / 2. + r.center_y(),
                        );
                        assert!(
                            point.x < row_width && point.y < height,
                            "wrapped link stays inside row"
                        );
                        let mut messages = Vec::new();
                        for input in [
                            mouse::Event::CursorMoved { position: point },
                            mouse::Event::ButtonPressed(mouse::Button::Left),
                            mouse::Event::ButtonReleased(mouse::Button::Left),
                        ] {
                            messages.extend(
                                native
                                    .event(cards.glance_event(&event), Event::Mouse(input), point)
                                    .0,
                            );
                        }
                        assert!(
                            messages
                                .iter()
                                .any(|m| matches!(m, Message::Open(url) if url == expected)),
                            "link {span} {style:?} {width} {title}: {messages:?}"
                        );
                    }
                    native.capture(
                        cards.glance_event(&event),
                        &theme,
                        &format!("inline-join-{style:?}-{width}-{}", title.len()),
                    );
                }
            }
        }
        // Create a complete transparent example with deliberately fictional meetings.
        cards.config.glance.style = GlanceStyle::Transparent;
        cards.config.widget_sizes.remove("glance");
        cards.calendar_today = Some(CalendarData {
            month: today.format("%Y-%m").to_string(),
            updated_at: Local::now().to_rfc3339(),
            events: ["Daily sync", "Voice agent review", "Plan the next release"]
                .into_iter()
                .map(|title| CalendarEvent {
                    title: title.into(),
                    start: today.to_string(),
                    end: (today + Days::days(1)).to_string(),
                    all_day: true,
                    url: "https://calendar.google.com/event?fixture".into(),
                    meeting_url: Some("https://meet.google.com/fixture".into()),
                    ..Default::default()
                })
                .collect(),
            ..CalendarData::default()
        });
        let size = cards.glance_size();
        Native::new(Size::new(size[0], size[1])).capture(
            cards.glance(),
            &theme,
            "glance-compact-example",
        );
    }

    #[test]
    fn cached_forecast_matches_the_requested_date_and_missing_days_stay_unavailable() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let daily = data::Daily {
            time: vec!["2026-10-18".into(), "2026-10-19".into()],
            temperature_2m_max: vec![11., 15.],
            temperature_2m_min: vec![6., 8.],
            weather_code: vec![0, 3],
            precipitation_probability_max: vec![Some(0.), Some(30.)],
        };
        assert_eq!(forecast_index(&daily, day), Some(1));
        assert_eq!(forecast_index(&daily, day + Days::days(1)), None);
        assert_eq!(forecast_index(&daily, day - Days::days(1)), Some(0));
    }
    #[test]
    fn no_enabled_sections_leave_a_transparent_pixel_and_preserve_placement() {
        let mut cards = app();
        cards
            .config
            .widget_positions
            .insert("glance".into(), [0.4, 0.6]);
        for style in [GlanceStyle::Transparent, GlanceStyle::Panel] {
            for density in [GlanceDensity::Compact, GlanceDensity::Comfortable] {
                cards.config.glance.style = style;
                cards.config.glance.density = density;
                let position = cards.glance_position(&cards.config.widget_positions);
                cards.config.glance.show_clock = false;
                cards.config.glance.show_weather = false;
                cards.config.glance.show_calendar = false;
                assert_eq!(cards.glance_natural_height(), 1.);
                assert_eq!(cards.glance_size(), [1., 1.]);
                assert!(cards.glance_input_rects().is_empty());
                assert!(cards.config.glance.enabled);
                assert_eq!(cards.config.widget_positions["glance"], [0.4, 0.6]);
                assert_eq!(
                    cards.glance_position(&cards.config.widget_positions),
                    position
                );
                cards.config.glance.show_clock = true;
                assert!(cards.glance_size()[0] > 1. && cards.glance_size()[1] > 1.);
                assert_eq!(
                    cards.glance_position(&cards.config.widget_positions),
                    position
                );
            }
        }
    }
    #[test]
    fn both_densities_can_include_tomorrow_without_changing_day_order() {
        let mut cards = app();
        let today = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
        for density in [GlanceDensity::Compact, GlanceDensity::Comfortable] {
            cards.config.glance.density = density;
            cards.config.glance.show_tomorrow = false;
            assert_eq!(cards.glance_days(today), vec![today]);
            cards.config.glance.show_tomorrow = true;
            assert_eq!(
                cards.glance_days(today),
                vec![today, NaiveDate::from_ymd_opt(2027, 1, 1).unwrap()]
            );
        }
        cards.config.glance.show_calendar = false;
        assert!(cards.glance_days(today).is_empty());
    }

    #[test]
    fn compact_tomorrow_agenda_grows_and_scrolls_with_real_event_rows() {
        let mut cards = app();
        let today = Local::now().date_naive();
        let tomorrow = today + Days::days(1);
        cards.config.glance.show_tomorrow = false;
        cards.calendar_today = Some(CalendarData {
            month: today.format("%Y-%m").to_string(),
            events: (0..18)
                .map(|index| CalendarEvent {
                    title: format!("Tomorrow meeting {index}"),
                    start: format!("{tomorrow}T09:00:00+00:00"),
                    end: format!("{tomorrow}T09:30:00+00:00"),
                    all_day: false,
                    url: "https://calendar.google.com/event?fixture".into(),
                    meeting_url: None,
                    ..Default::default()
                })
                .collect(),
            ..CalendarData::default()
        });
        let today_height = cards.glance_natural_height();
        cards.config.glance.show_tomorrow = true;
        assert_eq!(cards.glance_events(tomorrow).len(), 18);
        assert!(cards.glance_natural_height() > today_height + 400.);
        cards.screen_size = [820., 600.];
        let size = cards.glance_size();
        assert!(size[1] < cards.glance_natural_height());
        assert!(size[1] <= 552.);
        let mut native = crate::native_test::Native::new(Size::new(size[0], size[1]));
        native.capture(
            cards.glance(),
            &cosmic::Theme::dark(),
            "compact-tomorrow-scroll",
        );
        let before_scroll = native.render(cards.glance(), &cosmic::Theme::dark());
        let point = cosmic::iced::Point::new(size[0] / 2., size[1] - 24.);
        native.event(
            cards.glance(),
            cosmic::iced::Event::Mouse(cosmic::iced::mouse::Event::WheelScrolled {
                delta: cosmic::iced::mouse::ScrollDelta::Lines { x: 0., y: -100. },
            }),
            point,
        );
        assert!(
            native.render(cards.glance(), &cosmic::Theme::dark()) != before_scroll,
            "The compact agenda scrolls to later Tomorrow events"
        );
        native.capture(
            cards.glance(),
            &cosmic::Theme::dark(),
            "compact-tomorrow-scroll-bottom",
        );
        cards.calendar_today.as_mut().unwrap().events.truncate(2);
        for style in [GlanceStyle::Transparent, GlanceStyle::Panel] {
            cards.config.glance.style = style;
            let size = cards.glance_size();
            crate::native_test::Native::new(Size::new(size[0], size[1])).capture(
                cards.glance(),
                &cosmic::Theme::dark(),
                &format!("compact-tomorrow-{style:?}"),
            );
        }
    }
    #[test]
    fn full_agenda_grows_down_from_its_saved_anchor_then_scrolls_at_the_screen_edge() {
        let mut cards = app();
        let today = Local::now().date_naive();
        let encoded = crate::layout_state::encode_position(
            Widget::Glance,
            [400., 220.],
            [cards.glance_width(), 200.],
            cards.screen_size,
        );
        cards
            .config
            .widget_positions
            .insert("glance".into(), encoded);
        cards.calendar_today = Some(CalendarData {
            month: today.format("%Y-%m").to_string(),
            ..CalendarData::default()
        });
        let small = cards.glance_size();
        cards.calendar_today.as_mut().unwrap().events = (0..40)
            .map(|index| CalendarEvent {
                title: format!("Meeting {index}"),
                start: today.to_string(),
                end: (today + Days::days(1)).to_string(),
                all_day: true,
                url: String::new(),
                meeting_url: None,
                ..Default::default()
            })
            .collect();
        assert_eq!(cards.glance_events(today).len(), 40);
        assert!(cards.glance_natural_height() > 1080.);
        assert_eq!(
            cards.glance_position(&cards.config.widget_positions),
            [400., 220.]
        );
        assert!(cards.glance_size()[1] > small[1]);
        assert_eq!(cards.glance_size()[1], 1080. - 220. - 48.);
    }
    #[test]
    fn clock_area_is_click_through_but_weather_and_agenda_keep_input() {
        let mut cards = app();
        // This scenario needs an empty agenda, not the machine's cached events.
        cards.calendar = None;
        cards.calendar_today = None;
        cards.weather = None;
        let padding = cards.glance_padding();
        let clock_bottom = padding + cards.glance_clock_height();
        assert!(
            cards
                .glance_input_rects()
                .iter()
                .all(|r| r[1] as f32 >= clock_bottom.floor()),
            "clock bottom={clock_bottom}, size={:?}, header={}, rects={:?}",
            cards.glance_size(),
            cards.glance_header_height(),
            cards.glance_input_rects()
        );
        cards.config.glance.show_calendar = false;
        cards.config.glance.show_weather = false;
        assert!(cards.glance_input_rects().is_empty());
        cards.config.glance.show_weather = true;
        cards.config.glance.style = GlanceStyle::Panel;
        let regions = cards.glance_input_rects();
        assert_eq!(regions.len(), 1);
        assert!(regions[0][0] > 150);
    }
    #[test]
    fn short_viewport_keeps_scrolled_event_targets_interactive() {
        let mut cards = app();
        for style in [GlanceStyle::Transparent, GlanceStyle::Panel] {
            for density in [GlanceDensity::Compact, GlanceDensity::Comfortable] {
                cards.config.glance.style = style;
                cards.config.glance.density = density;
                let width = cards.glance_width();
                let position = crate::layout_state::encode_position(
                    Widget::Glance,
                    [82., 932.],
                    [width, 180.],
                    cards.screen_size,
                );
                cards
                    .config
                    .widget_positions
                    .insert("glance".into(), position);
                assert_eq!(cards.glance_size()[1], 100.);
                // Events, Join and the calendar heading can scroll through
                // the old clock region; no visible portion may pass clicks
                // through to the desktop while this outer scroller is used.
                assert_eq!(
                    cards.glance_input_rects(),
                    vec![[0, 0, width.ceil() as u32, 100]]
                );
            }
        }
    }
    #[test]
    fn first_enable_uses_clock_absolute_origin_and_keeps_both_placements() {
        if std::env::var_os("DESKTOP_CARDS_GLANCE_SEED_TEST").is_none() {
            let dir = std::env::temp_dir()
                .join(format!("desktop-cards-glance-seed-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("config.json");
            std::fs::write(&path, r#"{"enabled_widgets":["clock","weather","calendar"],"glance":{"enabled":true,"initialized":true},"widget_positions":{"clock":[0.3,0.7],"weather":[0.8,0.1]},"future_option":42}"#).unwrap();
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "app::glance::tests::first_enable_uses_clock_absolute_origin_and_keeps_both_placements"])
                .env("DESKTOP_CARDS_GLANCE_SEED_TEST", "1").env("DESKTOP_CARDS_CONFIG", &path).status().unwrap();
            let _ = std::fs::remove_dir_all(dir);
            assert!(result.success());
            return;
        }
        let (mut cards, _) = Cards::init(Core::default(), Config::load());
        cards.screen_size = [2560., 1440.];
        let clock_origin = cards.position_from(Widget::Clock, &cards.config.widget_positions);
        let original = cards.config.widget_positions.clone();
        let _ = cards.update(Message::SurfaceResized(
            cards.ids[5],
            Size::new(2560., 1440.),
        ));
        // Geometry is authoritative only after the compositor identifies the
        // selected output. An early resize must not seed a saved placement.
        assert!(!cards.output_known);
        assert!(!cards.config.widget_positions.contains_key("glance"));
        let wire = crate::displays::test_support::Wire::new();
        let _ = cards.update(Message::OutputChanged(
            crate::displays::OutputEvent::Created(Some(crate::displays::test_support::info(
                Some("DP-1"),
                (2560, 1440),
            ))),
            wire.output(1),
        ));
        let _ = cards.update(Message::SurfaceResized(
            cards.ids[5],
            Size::new(2560., 1440.),
        ));
        assert!(cards.output_known);
        let seeded = cards.config.widget_positions["glance"];
        let glance_origin = cards.glance_position(&cards.config.widget_positions);
        assert!((glance_origin[0] - clock_origin[0]).abs() < 0.001);
        assert!((glance_origin[1] - clock_origin[1]).abs() < 0.001);
        for (key, position) in &original {
            assert_eq!(cards.config.widget_positions[key], *position);
        }
        let mut config = cards.config.clone();
        config.save_widget(Widget::Glance, false).unwrap();
        let _ = cards.apply_config(config);
        assert!(cards.config.widget_visible(Widget::Clock));
        assert_eq!(
            cards.position_from(Widget::Clock, &cards.config.widget_positions),
            clock_origin
        );
        let mut config = cards.config.clone();
        config.save_widget(Widget::Glance, true).unwrap();
        let _ = cards.apply_config(config);
        assert_eq!(cards.config.widget_positions["glance"], seeded);
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(Config::path().unwrap()).unwrap()).unwrap();
        assert_eq!(saved["future_option"], 42);
    }
    #[test]
    fn hidden_individual_surfaces_still_schedule_enabled_glance_sections() {
        let (mut cards, _) = Cards::init(
            Core::default(),
            Config {
                enabled_widgets: vec![],
                location: Some(Location {
                    name: "Test".into(),
                    latitude: 0.,
                    longitude: 0.,
                    timezone: None,
                }),
                ..Config::default()
            },
        );
        cards.calendar_connection = crate::calendar::Connection {
            backend: crate::calendar::Backend::Google,
            account: "sample@example.com".into(),
            revision: "test".into(),
            ..Default::default()
        };
        let mut config = cards.config.clone();
        config.glance.enabled = true;
        config.glance.initialized = true;
        let _ = cards.apply_config(config);
        assert!(!cards.config.widget_visible(Widget::Clock));
        assert!(!cards.config.widget_visible(Widget::Weather));
        assert!(!cards.config.widget_visible(Widget::Calendar));
        let _ = cards.refresh_visible_data();
        assert!(cards.weather_fetching() && cards.calendar_fetching.is_some());
        assert!(cards.next_wake().is_some());
        cards.config.glance.show_clock = false;
        cards.config.glance.show_weather = false;
        cards.config.glance.show_calendar = false;
        assert!(cards.next_wake().is_none());
        assert!(cards.refresh_visible_data().is_empty());
        assert!(cards.config.enabled_widgets.is_empty());
    }
    #[test]
    fn clock_optical_alignment_accounts_for_the_leading_digit() {
        assert!((clock_ink_offset("01:00", 100.) - 6.6).abs() < 0.01);
        assert!((clock_ink_offset("11:00", 100.) - 4.3).abs() < 0.01);
        assert!((clock_ink_offset("23:00", 100.) - 3.3).abs() < 0.01);
    }
    #[test]
    fn glance_keeps_current_agenda_while_the_standalone_calendar_is_browsed() {
        let mut cards = app();
        let today = Local::now().date_naive();
        cards.calendar_today = Some(CalendarData {
            month: today.format("%Y-%m").to_string(),
            events: vec![CalendarEvent {
                title: "Today".into(),
                start: today.to_string(),
                end: (today + Days::days(1)).to_string(),
                all_day: true,
                url: String::new(),
                meeting_url: None,
                ..Default::default()
            }],
            ..CalendarData::default()
        });
        let _ = cards.update(Message::Day(today + Days::days(90)));
        assert_eq!(cards.glance_events(today).len(), 1);
        assert_eq!(cards.glance_days(today), vec![today]);
    }
    #[test]
    fn all_native_variants_reconcile_when_sections_and_agendas_change() {
        use cosmic::iced::advanced::widget::Tree;
        let mut cards = app();
        let mut tree = Tree::new(cards.glance().as_widget());
        for style in [GlanceStyle::Transparent, GlanceStyle::Panel] {
            for density in [GlanceDensity::Compact, GlanceDensity::Comfortable] {
                for sections in [
                    [true, true, true],
                    [false, true, true],
                    [true, false, true],
                    [true, true, false],
                    [false, false, false],
                ] {
                    cards.config.glance.style = style;
                    cards.config.glance.density = density;
                    [
                        cards.config.glance.show_clock,
                        cards.config.glance.show_weather,
                        cards.config.glance.show_calendar,
                    ] = sections;
                    tree.diff(cards.glance().as_widget_mut());
                    cards.screen_size = [380., 230.];
                    tree.diff(cards.glance().as_widget_mut());
                    cards.screen_size = [1920., 1080.];
                }
            }
        }
    }
}
