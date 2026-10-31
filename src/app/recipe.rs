//! Native parts use existing widget snapshots; rendering never fetches data.
use super::*;
pub(super) mod calendar;
pub(super) mod system;
use crate::recipes::{Arrangement, Kind, Part, Recipe};
use crate::widget_presentation::Context;
#[derive(Clone, Default)]
pub(super) struct Reading {
    pub system: Vec<system::Metric>,
    pub city: String,
    pub temperature: String,
    pub conditions: String,
    pub forecast: String,
    pub status: Option<String>,
    pub date: Option<String>,
    pub agenda: Vec<calendar::Entry>,
    pub agenda_total: usize,
    pub agenda_empty: String,
}
#[derive(Debug, Clone)]
pub(super) struct Plan {
    pub parts: Vec<Part>,
    pub split: bool,
    pub heights: Vec<f32>,
    pub width: f32,
    pub height: f32,
    pub compacted: bool,
}
fn minimum(part: Part) -> f32 {
    match part {
        Part::Cpu | Part::Memory | Part::Disk | Part::Network | Part::Gpu | Part::Uptime => 42.,
        Part::Face => 56.,
        Part::Agenda => 42.,
        Part::Reading => 36.,
        Part::Forecast => 36.,
        Part::Settings => 36.,
        Part::Divider => 1.,
        Part::Space => 8.,
        _ => 18.,
    }
}
pub(super) fn plan(recipe: &Recipe, size: [f32; 2], status: bool) -> Plan {
    let size = crate::widget_presentation::viewport(size, [360., 180.]);
    let width = (size[0] - 16.).max(1.);
    let height = (size[1] - 16. - if status { 36. } else { 0. }).max(1.);
    let mut parts = recipe.parts.clone();
    let mut split =
        recipe.arrangement(width + 16.) == Arrangement::Split && width >= 280. && parts.len() > 1;
    let required = |parts: &[Part], split: bool| {
        let main = minimum(recipe.primary());
        let rest: f32 = parts
            .iter()
            .filter(|p| **p != recipe.primary())
            .map(|p| minimum(*p) + 4.)
            .sum();
        if split {
            main.max(rest)
        } else {
            parts.iter().map(|p| minimum(*p) + 4.).sum()
        }
    };
    let compacted = required(&parts, split) > height;
    if compacted {
        parts = vec![recipe.primary()];
        split = false;
    }
    let mut heights: Vec<_> = parts.iter().map(|p| minimum(*p)).collect();
    let used = if split {
        heights
            .iter()
            .zip(&parts)
            .filter(|(_, p)| **p != recipe.primary())
            .map(|(h, _)| h + 4.)
            .sum::<f32>()
    } else {
        heights.iter().sum::<f32>() + 4. * parts.len().saturating_sub(1) as f32
    };
    if let Some(i) = parts.iter().position(|p| *p == recipe.primary()) {
        heights[i] = if split {
            height
        } else {
            (heights[i] + height - used).max(1.)
        };
    }
    Plan {
        parts,
        split,
        heights,
        width,
        height,
        compacted,
    }
}
pub(super) fn action_rect(recipe: &Recipe, size: [f32; 2]) -> Option<[u32; 4]> {
    let frame = plan(recipe, size, false);
    let i = frame.parts.iter().position(|p| *p == Part::Settings)?;
    let primary = recipe.primary();
    let (x, y, width) = if frame.split {
        let detail_height = frame
            .parts
            .iter()
            .zip(&frame.heights)
            .filter(|(p, _)| **p != primary)
            .map(|(_, h)| *h)
            .sum::<f32>()
            + 4. * frame.parts.len().saturating_sub(2) as f32;
        let before = frame.parts[..i]
            .iter()
            .zip(&frame.heights[..i])
            .filter(|(p, _)| **p != primary)
            .map(|(_, h)| *h + 4.)
            .sum::<f32>();
        (
            8. + frame.width * 0.55 + 6.,
            8. + (frame.height - detail_height).max(0.) / 2. + before,
            frame.width * 0.45 - 6.,
        )
    } else {
        (
            8.,
            8. + frame.heights[..i].iter().map(|h| *h + 4.).sum::<f32>(),
            frame.width,
        )
    };
    crate::widget_presentation::input_rect([x, y], [width, frame.heights[i]], size)
}
pub(super) fn view(
    config: &Config,
    recipe: &Recipe,
    now: chrono::NaiveDateTime,
    reading: Reading,
    context: Context,
    size: [f32; 2],
) -> Element<'static, Message> {
    let _scope = crate::widget_style::scope(config);
    let palette = if recipe.kind == Kind::Clock {
        config.wallpaper_palette()
    } else {
        config.palette()
    };
    let style = crate::widget_style::presentation(config, palette, recipe.kind == Kind::Clock);
    let p = style.palette();
    let font = style.body.value;
    let label =
        |value: String, height: f32, width: f32, large: bool| -> Element<'static, Message> {
            let wanted: f32 = if large { 36. } else { 14. };
            let size = if large {
                wanted
                    .min((height / 1.25).max(1.))
                    .min(wanted * (width / clock::measure(&value, wanted, font).max(1.)).min(1.))
            } else {
                wanted.min((height / 1.25).max(1.))
            };
            text::body(value)
                .size(size)
                .font(font)
                .line_height(1.25)
                .wrapping(cosmic::iced::advanced::text::Wrapping::None)
                .ellipsize(cosmic::iced::advanced::text::Ellipsize::End(
                    cosmic::iced::advanced::text::EllipsizeHeightLimit::Lines(1),
                ))
                .width(width)
                .class(cosmic::theme::Text::Color(p.foreground))
                .into()
        };
    let frame = plan(recipe, size, reading.status.is_some());
    let mut clock_config = config.clone();
    clock_config.recipes.clock = None;
    clock_config.clock.show_date = false;
    clock_config.clock.surface = Some(crate::widget_style::resolver::ClockSurface::None);
    clock_config.clock.depth = false;
    clock_config.clock.center_vertically = true;
    clock_config.moments.day.clock.surface =
        Some(crate::widget_style::resolver::ClockSurface::None);
    clock_config.moments.night.clock.surface =
        Some(crate::widget_style::resolver::ClockSurface::None);
    // Preserve the resolved Moment face/type while removing only this part's surface.
    clock_config.clock.face = Some(style.clock_face.value);
    let primary = recipe.primary();
    let part_width = |part: Part| {
        if frame.split {
            if part == primary {
                frame.width * 0.55 - 6.
            } else {
                frame.width * 0.45 - 6.
            }
        } else {
            frame.width
        }
    };
    let make = |part: Part, h: f32| -> Element<'static, Message> {
        let w = part_width(part);
        match part {
            Part::Face => clock::Presentation::part(&clock_config, p)
                .frame(now, [w, h])
                .view(),
            Part::Date => label(
                reading
                    .date
                    .clone()
                    .unwrap_or_else(|| now.format("%A, %B %-d").to_string()),
                h,
                w,
                false,
            ),
            Part::Agenda => calendar::part(&reading, w, h, p, font, context),
            Part::Cpu | Part::Memory | Part::Disk | Part::Network | Part::Gpu | Part::Uptime => {
                system::part(&reading, part, w, h, p, font)
            }
            Part::City => label(reading.city.clone(), h, w, false),
            Part::Reading => label(reading.temperature.clone(), h, w, true),
            Part::Conditions => label(reading.conditions.clone(), h, w, false),
            Part::Forecast => label(reading.forecast.clone(), h, w, false),
            Part::Caption => label(recipe.caption.clone(), h, w, false),
            Part::Divider => container(cosmic::widget::space().width(w).height(1))
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        background: Some(p.muted.into()),
                        ..Default::default()
                    }
                }))
                .into(),
            Part::Track
            | Part::Artwork
            | Part::Progress
            | Part::Playback
            | Part::Timer
            | Part::TimerControls
            | Part::SessionDots => cosmic::widget::space().into(),
            Part::Space => cosmic::widget::space().height(h).into(),
            Part::Settings => {
                let button = cosmic::widget::button::text("Widget settings")
                    .id(cosmic::widget::Id::new("recipe-settings"))
                    .class(action_style(p.foreground))
                    .width(w)
                    .height(h);
                if context == Context::Live {
                    button
                        .on_press(match recipe.kind {
                            Kind::Clock => Message::ClockSettings,
                            Kind::Weather => Message::WeatherSettings,
                            Kind::Calendar => Message::CalendarSettings,
                            Kind::System => Message::DevboxSettings,
                            Kind::Music | Kind::Focus => Message::Preview,
                        })
                        .into()
                } else {
                    label("Widget settings".into(), h, w, false)
                }
            }
        }
    };
    let mut pieces: Vec<_> = frame
        .parts
        .iter()
        .zip(&frame.heights)
        .map(|(p, h)| {
            (
                *p,
                container(make(*p, *h))
                    .width(part_width(*p))
                    .height(*h)
                    .into(),
            )
        })
        .collect::<Vec<(Part, Element<'static, Message>)>>();
    let content: Element<'static, Message> = if frame.split {
        let index = pieces.iter().position(|(p, _)| *p == primary).unwrap();
        let main = pieces.remove(index).1;
        row::with_children(vec![
            main,
            column::with_children(pieces.into_iter().map(|(_, e)| e).collect::<Vec<_>>())
                .spacing(4)
                .width(part_width(Part::Caption))
                .into(),
        ])
        .spacing(12)
        .align_y(Alignment::Center)
        .into()
    } else {
        column::with_children(pieces.into_iter().map(|(_, e)| e).collect::<Vec<_>>())
            .spacing(4)
            .into()
    };
    let mut rows = vec![
        container(content)
            .height(frame.height)
            .width(frame.width)
            .into(),
    ];
    if let Some(status) = reading.status {
        rows.push(
            container(
                text::body(status)
                    .size(11)
                    .font(font)
                    .class(cosmic::theme::Text::Color(p.muted)),
            )
            .id(cosmic::widget::Id::new(if recipe.kind == Kind::Weather {
                "recipe-weather-status"
            } else if recipe.kind == Kind::System {
                "recipe-system-status"
            } else {
                "recipe-calendar-status"
            }))
            .height(32)
            .into(),
        );
    }
    let background = style.surface.value;
    container(column::with_children(rows).spacing(4))
        .width(size[0])
        .height(size[1])
        .padding(8)
        .clip(true)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: match background {
                    crate::widget_style::resolver::Surface::None => None,
                    crate::widget_style::resolver::Surface::Solid(color) => Some(color.into()),
                },
                border: cosmic::iced::Border {
                    radius: f32::from(style.radius.value).into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

// Widget surfaces may be light while the application theme is dark (or vice versa).
// Keep the native button's focus and keyboard behavior, but use the resolved ink.
fn action_style(ink: Color) -> cosmic::theme::Button {
    let style = move |focused: bool, hovered: bool| cosmic::widget::button::Style {
        text_color: Some(ink),
        icon_color: Some(ink),
        background: Some(
            Color {
                a: if hovered { 0.10 } else { 0.04 },
                ..ink
            }
            .into(),
        ),
        border_radius: 6.0.into(),
        outline_width: if focused { 2. } else { 0. },
        outline_color: ink,
        ..Default::default()
    };
    cosmic::theme::Button::Custom {
        active: Box::new(move |focused, _| style(focused, false)),
        hovered: Box::new(move |focused, _| style(focused, true)),
        pressed: Box::new(move |focused, _| style(focused, true)),
        disabled: Box::new(move |_| style(false, false)),
    }
}
#[cfg(test)]
mod tests;
