//! Weather Outlook uses the same observation and display units as Classic.
use super::*;
use cosmic::iced::{Point, Rectangle, mouse, widget::canvas};
use cosmic::widget::image::Handle;
use std::sync::OnceLock;

#[derive(Clone)]
pub(super) struct Outlook {
    art: Option<Art>,
    feels: Option<String>,
    dew: Option<String>,
    range: Option<(String, String, f32)>,
    days: Vec<Day>,
}

#[derive(Clone)]
struct Day {
    name: String,
    condition: String,
    low: String,
    high: String,
    values: Option<(f64, f64)>,
}

#[derive(Clone, Copy)]
enum Art {
    Sun,
    Clouds,
    Overcast,
    Fog,
    Rain,
    Snow,
    Night,
}

impl Art {
    fn for_weather(code: u32, day: bool) -> Self {
        match code {
            45 | 48 => Self::Fog,
            71..=77 | 85 | 86 => Self::Snow,
            51..=99 => Self::Rain,
            3 => Self::Overcast,
            _ if !day => Self::Night,
            1 | 2 => Self::Clouds,
            _ => Self::Sun,
        }
    }

    fn handle(self) -> Handle {
        static ART: [OnceLock<Handle>; 7] = [const { OnceLock::new() }; 7];
        ART[self as usize]
            .get_or_init(|| {
                let name = match self {
                    Self::Sun => "halcyon-paper",
                    Self::Clouds => "halcyon-rose",
                    Self::Overcast => "halcyon-focus",
                    Self::Fog => "halcyon-fog",
                    Self::Rain => "halcyon-rain",
                    Self::Snow => "halcyon-snow",
                    Self::Night => "halcyon-calm",
                };
                let mut pixels = crate::desktop::wallpaper::media::bundled_thumbnail(name)
                    .expect("compiled-in weather image");
                let width = pixels.width as usize;
                for (index, pixel) in pixels.rgba.chunks_exact_mut(4).enumerate() {
                    let x = (index % width) as f32 / width as f32;
                    let y = (index / width) as f32 / pixels.height as f32;
                    let edge = (x * 1.7).min(1.) * ((1. - x) * 7.).min(1.);
                    let vertical = (y * 7.).min(1.) * ((1. - y) * 3.).min(1.);
                    pixel[3] = (140. * edge * vertical) as u8;
                }
                Handle::from_rgba(pixels.width, pixels.height, pixels.rgba)
            })
            .clone()
    }
}

pub(super) fn limits(weather: &Weather, i: usize) -> Option<(f64, f64)> {
    let low = *weather.daily.temperature_2m_min.get(i)?;
    let high = *weather.daily.temperature_2m_max.get(i)?;
    (low.is_finite() && high.is_finite() && low <= high).then_some((low, high))
}

impl Outlook {
    pub(super) fn new(config: &Config, w: &Weather, today: NaiveDate) -> Self {
        let prefs = config.weather;
        let temp = |n: &f64| forecast_temperature_in(prefs, Some(n));
        let current = w.current.temperature_2m;
        let range = prefs
            .outlook_range
            .then(|| {
                let (i, _) =
                    weather_forecast_days(&w.daily, today).find(|(_, day)| *day == today)?;
                let (lo, hi) = limits(w, i)?;
                current.is_finite().then(|| {
                    let position = if hi > lo {
                        ((current - lo) / (hi - lo)).clamp(0., 1.) as f32
                    } else {
                        0.5
                    };
                    (temp(&lo), temp(&hi), position)
                })
            })
            .flatten();
        let days = if prefs.outlook_forecast {
            weather_forecast_days(&w.daily, today)
                .filter(|(_, day)| *day > today)
                .take(3)
                .map(|(i, day)| Day {
                    name: day.format("%a").to_string(),
                    condition: w
                        .daily
                        .weather_code
                        .get(i)
                        .map_or("—", |code| data::weather_description(*code))
                        .into(),
                    low: forecast_temperature_in(prefs, w.daily.temperature_2m_min.get(i)),
                    high: forecast_temperature_in(prefs, w.daily.temperature_2m_max.get(i)),
                    values: limits(w, i),
                })
                .collect()
        } else {
            Vec::new()
        };
        Self {
            art: prefs
                .outlook_artwork
                .then(|| Art::for_weather(w.current.weather_code, w.current.is_day != 0)),
            feels: ((current - w.current.apparent_temperature).abs() >= 3.)
                .then(|| format!("Feels like {}", temp(&w.current.apparent_temperature))),
            dew: weather_dew_point(config, &w.current),
            range,
            days,
        }
    }

    pub(super) fn view(
        &self,
        p: &Presentation,
        size: [f32; 2],
        temperature: &str,
        description: &str,
        status: Option<&str>,
    ) -> Element<'static, Message> {
        let width = (size[0] - 2. * crate::widget_style::PADDING as f32).max(1.);
        let height = (size[1] - 2. * crate::widget_style::PADDING as f32).max(1.);
        let wide = width >= 485. && !self.days.is_empty();
        let main_width = if wide { width * 0.50 } else { width };
        let ink = p.palette.foreground;
        let quiet = p.palette.muted;
        let mut details = vec![p.label(description, 14., ink)];
        if let Some(feels) = &self.feels {
            details.push(p.label(feels, 12., quiet));
        }
        if let Some(dew) = &self.dew {
            details.push(p.label(dew, 12., quiet));
        }
        let mut lines = vec![
            p.label(p.title.clone(), 16., ink),
            row::with_children(vec![
                p.label(temperature, 38., ink),
                column::with_children(details).into(),
            ])
            .spacing(9)
            .align_y(Alignment::Center)
            .into(),
        ];
        if let Some((lo, hi, position)) = &self.range {
            lines.push(
                row::with_children(vec![
                    p.label("Today", 12., quiet),
                    p.label(lo.clone(), 12., ink),
                    rail(0., 1., Some(*position), p.palette),
                    p.label(hi.clone(), 12., ink),
                ])
                .spacing(7)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        if let Some(s) = status {
            lines.push(p.label(s, 11., quiet));
        }
        let stacked =
            !wide && !self.days.is_empty() && height >= if status.is_some() { 230. } else { 196. };
        let main_height = if stacked { height - 96. } else { height };
        let content = container(column::with_children(lines).spacing(3))
            .width(Length::Fill)
            .height(Length::Fixed(main_height))
            .align_y(Alignment::Center);
        let current: Element<'static, Message> = if let Some(art) = self.art {
            let image = cosmic::widget::image(art.handle())
                .content_fit(cosmic::iced::ContentFit::Contain)
                .width(Length::Fixed(main_width * 0.64))
                .height(Length::Fixed(main_height.min(140.)));
            let bg = container(image)
                .width(Length::Fill)
                .height(Length::Fixed(main_height))
                .align_x(Alignment::End)
                .align_y(Alignment::Center);
            cosmic::iced::widget::stack![bg, content].into()
        } else {
            content.into()
        };
        let current = container(current).width(Length::Fixed(main_width));
        let mut body: Vec<Element<'static, Message>> = vec![current.into()];
        if wide || stacked {
            body.push(self.forecast(p));
        }
        let body: Element<'static, Message> = if wide {
            row::with_children(body)
                .spacing(14)
                .align_y(Alignment::Center)
                .into()
        } else {
            column::with_children(body).spacing(8).into()
        };
        card_frame_width(
            container(body)
                .height(Length::Fill)
                .align_y(Alignment::Center)
                .into(),
            p.palette,
            p.appearance,
            Length::Fixed(size[1]),
            Length::Fixed(size[0]),
        )
    }

    fn forecast(&self, p: &Presentation) -> Element<'static, Message> {
        let quiet = p.palette.muted;
        let ink = p.palette.foreground;
        let number = |v: String| {
            container(p.label(v, 12., ink))
                .width(33)
                .align_x(Alignment::End)
                .into()
        };
        let mut rows = vec![
            row::with_children(vec![
                container(p.label("Forecast", 12., quiet))
                    .width(Length::Fill)
                    .into(),
                number("Low".into()),
                number("High".into()),
            ])
            .spacing(5)
            .into(),
        ];
        let floor = self
            .days
            .iter()
            .filter_map(|d| d.values.map(|v| v.0))
            .fold(f64::INFINITY, f64::min);
        let ceiling = self
            .days
            .iter()
            .filter_map(|d| d.values.map(|v| v.1))
            .fold(f64::NEG_INFINITY, f64::max);
        for d in &self.days {
            let mut cells = vec![
                container(p.label(d.name.clone(), 12., ink))
                    .width(30)
                    .into(),
                container(p.label(d.condition.clone(), 12., quiet))
                    .width(Length::Fill)
                    .into(),
            ];
            if let Some((lo, hi)) = d.values
                && ceiling > floor
            {
                cells.push(
                    container(rail(
                        ((lo - floor) / (ceiling - floor)) as f32,
                        ((hi - floor) / (ceiling - floor)) as f32,
                        None,
                        p.palette,
                    ))
                    .width(24)
                    .into(),
                );
            }
            cells.extend([number(d.low.clone()), number(d.high.clone())]);
            rows.push(
                row::with_children(cells)
                    .align_y(Alignment::Center)
                    .spacing(5)
                    .into(),
            );
        }
        column::with_children(rows)
            .spacing(3)
            .width(Length::Fill)
            .into()
    }
}

struct Rail {
    start: f32,
    end: f32,
    marker: Option<f32>,
    palette: Palette,
}
pub(super) fn rail(
    start: f32,
    end: f32,
    marker: Option<f32>,
    palette: Palette,
) -> Element<'static, Message> {
    canvas::Canvas::new(Rail {
        start,
        end,
        marker,
        palette,
    })
    .width(Length::Fill)
    .height(12)
    .into()
}
impl canvas::Program<Message, cosmic::Theme> for Rail {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &cosmic::Renderer,
        _: &cosmic::Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let x = |v: f32| 3. + v * (bounds.width - 6.).max(0.);
        let y = bounds.height / 2.;
        for (lo, hi, color) in [
            (
                0.,
                1.,
                Color {
                    a: 0.25,
                    ..self.palette.muted
                },
            ),
            (self.start, self.end, self.palette.accent),
        ] {
            frame.stroke(
                &canvas::Path::line(Point::new(x(lo), y), Point::new(x(hi), y)),
                canvas::Stroke::default().with_width(3.).with_color(color),
            );
        }
        if let Some(value) = self.marker {
            frame.fill(
                &canvas::Path::circle(Point::new(x(value), y), 3.4),
                self.palette.foreground,
            );
        }
        vec![frame.into_geometry()]
    }
}
