//! Readings from the existing sampler. Previews use fixed sample values only.
use super::*;
#[derive(Clone, Debug)]
pub(in crate::app) struct Metric {
    pub part: Part,
    pub value: String,
    pub fraction: Option<f32>,
}
fn percent(value: f32) -> Option<(String, Option<f32>)> {
    (value.is_finite() && (0. ..=100.).contains(&value))
        .then(|| (format!("{value:.0}%"), Some(value / 100.)))
}
pub(in crate::app) fn reading(recipe: &Recipe, m: &Metrics) -> Reading {
    let mut output = Reading::default();
    for part in recipe.parts.iter().copied().filter(|p| p.is_metric()) {
        let value = match part {
            Part::Cpu => m.cpu_sampled.then(|| percent(m.cpu)).flatten(),
            Part::Memory => (m.memory_sampled && m.memory_total > 0. && m.memory_used.is_finite())
                .then(|| {
                    (
                        format!("{:.1} / {:.0} GiB", m.memory_used, m.memory_total),
                        Some((m.memory_used / m.memory_total).clamp(0., 1.)),
                    )
                }),
            Part::Disk => m
                .disk
                .filter(|(used, total)| used.is_finite() && total.is_finite() && *total > 0.)
                .map(|(used, total)| {
                    (
                        format!("{used:.0} / {total:.0} GiB"),
                        Some((used / total).clamp(0., 1.) as f32),
                    )
                }),
            Part::Network => m
                .network
                .as_ref()
                .and_then(|n| n.download.zip(n.upload))
                .filter(|(down, up)| down.is_finite() && up.is_finite() && *down >= 0. && *up >= 0.)
                .map(|(down, up)| (format!("↓ {}  ↑ {}", rate(down), rate(up)), None)),
            // A bounded summary avoids silently selecting one physical GPU.
            Part::Gpu => {
                let values: Vec<_> = m
                    .gpus
                    .iter()
                    .map(|g| {
                        if g.asleep {
                            Some(("Asleep".into(), None))
                        } else {
                            g.utilization.and_then(percent)
                        }
                    })
                    .collect();
                if values.is_empty() || values.iter().any(Option::is_none) {
                    None
                } else if values.len() == 1 {
                    values.into_iter().next().flatten()
                } else {
                    Some((
                        format!(
                            "{} GPUs · {}",
                            values.len(),
                            values
                                .into_iter()
                                .take(4)
                                .map(|v| v.unwrap().0)
                                .collect::<Vec<_>>()
                                .join(" / ")
                        ),
                        None,
                    ))
                }
            }
            Part::Uptime => (!m.uptime.is_empty()).then(|| (m.uptime.clone(), None)),
            _ => None,
        };
        let (value, fraction) = value.unwrap_or_else(|| {
            output.status = Some("Some readings unavailable".into());
            ("—".into(), None)
        });
        output.system.push(Metric {
            part,
            value,
            fraction,
        });
    }
    output
}
fn rate(bytes: f64) -> String {
    if bytes >= 1_048_576. {
        format!("{:.1} MiB/s", bytes / 1_048_576.)
    } else {
        format!("{:.0} KiB/s", bytes / 1024.)
    }
}
pub(in crate::app) fn sample(
    config: &Config,
    recipe: &Recipe,
    missing: bool,
    size: [f32; 2],
) -> Element<'static, Message> {
    let empty = Metrics::default();
    let metrics = if missing {
        &empty
    } else {
        &super::super::settings_preview::monitor_sample().metrics
    };
    view(
        config,
        recipe,
        Local::now().naive_local(),
        reading(recipe, metrics),
        Context::LocalPreview,
        size,
    )
}
pub(super) fn part(
    reading: &Reading,
    part: Part,
    width: f32,
    height: f32,
    p: Palette,
    font: Font,
) -> Element<'static, Message> {
    let metric = reading.system.iter().find(|m| m.part == part);
    let value = metric.map_or("—", |m| m.value.as_str());
    let wanted = if height >= 80. { 32. } else { 18. };
    let size = (wanted * (width / clock::measure(value, wanted, font).max(1.)).min(1.)).max(9.);
    let label = |value: String, size: f32, color| {
        text::body(value)
            .font(font)
            .size(size)
            .wrapping(cosmic::iced::advanced::text::Wrapping::None)
            .ellipsize(cosmic::iced::advanced::text::Ellipsize::End(
                cosmic::iced::advanced::text::EllipsizeHeightLimit::Lines(1),
            ))
            .class(cosmic::theme::Text::Color(color))
            .width(Length::Fill)
    };
    let mut rows: Vec<Element<'static, Message>> = vec![
        label(part.title().into(), 11., p.muted).into(),
        label(value.into(), size, p.foreground).into(),
    ];
    if height >= 54.
        && let Some(fraction) = metric.and_then(|m| m.fraction)
    {
        rows.push(bar(&[(fraction, p.accent)], p));
    }
    container(column::with_children(rows).spacing(3))
        .width(width)
        .height(height)
        .center_y(height)
        .clip(true)
        .into()
}
