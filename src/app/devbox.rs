use super::history::MetricHistories;
use super::*;
use crate::config::{DevboxPreferences, DevboxSection};
use cosmic::iced::Size;
use cosmic::iced::advanced::text::{self as advanced_text, Paragraph};

// Primary readings share header and indicator slots, including optional swap.
const HEADER_HEIGHT: f32 = 16.;
const INDICATOR_HEIGHT: f32 = 5.;
const LINE_GAP: f32 = 4.;
const SECTION_GAP: f32 = 8.;
const EMPTY_HEIGHT: f32 = 20.;

impl Cards {
    pub(super) fn devbox_natural_height(&self) -> f32 {
        natural_height(&self.config, self.metrics.gpus.len())
    }

    pub(super) fn devbox(&self) -> Element<'_, Message> {
        if let Some(recipe) = &self.config.recipes.devbox {
            return recipe::view(
                &self.config,
                recipe,
                Local::now().naive_local(),
                recipe::system::reading(recipe, &self.metrics),
                crate::widget_presentation::Context::Live,
                [
                    self.widget_size(Widget::Devbox)[0],
                    self.card_height(Widget::Devbox),
                ],
            );
        }
        view(
            &self.config,
            &self.metrics,
            &self.histories,
            self.card_height(Widget::Devbox),
        )
    }
}

pub(super) fn natural_height(config: &Config, gpu_count: usize) -> f32 {
    crate::layout_state::devbox_height(config, gpu_count)
}

/// The same card is used on the desktop and in Settings; rendering only reads
/// the supplied metrics and histories, and never starts a sample.
pub(super) fn view<'a>(
    config: &Config,
    m: &Metrics,
    histories: &'a MetricHistories,
    height: f32,
) -> Element<'a, Message> {
    let _style = crate::widget_style::scope(config);
    let p = config.palette();
    let preferences = &config.devbox;
    let indicator_height = indicator_height(preferences);
    let metric_section = |header, indicator| metric_row(header, indicator, indicator_height);
    let indicator = |section, fallback| {
        if MetricHistories::enabled(preferences, section) {
            histories.view(section, None, p)
        } else {
            fallback
        }
    };
    let mut rows = Vec::new();
    for section in preferences
        .ordered_sections()
        .into_iter()
        .filter(|section| section.enabled(preferences))
    {
        rows.push(match section {
            DevboxSection::Cpu => {
                let temperature = m
                    .cpu_temperature
                    .map(|value| format!(" · {value:.0}°C"))
                    .unwrap_or_default();
                let color = if m.cpu_temperature.is_some_and(|value| value >= 85.) {
                    Color::from_rgb8(244, 132, 116)
                } else {
                    p.foreground
                };
                metric_section(
                    spread(
                        metric_name(section, "CPU", p),
                        metric_text(format!("{:.0}%{temperature}", m.cpu), 12., color),
                    ),
                    indicator(section, bar(&[(m.cpu / 100., p.accent)], p)),
                )
            }
            DevboxSection::Memory => {
                let bar_indicator = if m.memory_total > 0. {
                    let mut segments = vec![(m.memory_used / m.memory_total, p.accent)];
                    if preferences.memory_cache {
                        segments.push((
                            m.memory_cache / m.memory_total,
                            mix(p.accent, p.background, 0.45),
                        ));
                    }
                    bar(&segments, p)
                } else {
                    neutral_rule(p)
                };
                // Every reading sits in the RAM row: swap, cache, then used of total.
                metric_section(
                    memory_header(
                        memory_summary(m),
                        preferences.swap.then(|| swap_summary(m.swap)),
                        (preferences.memory_cache && m.memory_total > 0.)
                            .then(|| format!("Cache {:.1}", m.memory_cache)),
                        p,
                    ),
                    indicator(section, bar_indicator),
                )
            }
            DevboxSection::Disk => {
                let (summary, bar_indicator) = m
                    .disk
                    .map(|(used, total)| {
                        (
                            format!("{used:.1} / {total:.0} GiB"),
                            bar(&[((used / total.max(1.)) as f32, p.accent)], p),
                        )
                    })
                    .unwrap_or_else(|| ("Unavailable".into(), neutral_rule(p)));
                metric_section(
                    spread(
                        metric_name(section, "Disk", p),
                        metric_text(summary, 12., p.foreground),
                    ),
                    indicator(section, bar_indicator),
                )
            }
            DevboxSection::Network => {
                let rates = match &m.network {
                    Some(network) if network.interfaces == 0 => "No active link".into(),
                    Some(network) => match (network.download, network.upload) {
                        (Some(download), Some(upload)) => {
                            format!("↓ {}   ↑ {}", throughput(download), throughput(upload))
                        }
                        _ => "Measuring…".into(),
                    },
                    None => "Unavailable".into(),
                };
                metric_section(
                    spread(
                        metric_name(section, "Network", p),
                        metric_text(rates, 12., p.foreground),
                    ),
                    indicator(section, neutral_rule(p)),
                )
            }
            DevboxSection::Gpu => {
                if m.gpus.is_empty() {
                    metric_section(
                        spread(
                            metric_name(section, "GPU", p),
                            metric_text("No GPU detected", 12., p.muted),
                        ),
                        indicator(section, neutral_rule(p)),
                    )
                } else {
                    let devices: Vec<Element<'_, Message>> = m
                        .gpus
                        .iter()
                        .enumerate()
                        .map(|(index, gpu)| {
                            let title = gpu_title(index, m.gpus.len(), &gpu.name);
                            let mut status = if gpu.asleep {
                                "Sleeping".into()
                            } else {
                                gpu.utilization
                                    .map(|load| format!("{load:.0}%"))
                                    .unwrap_or_else(|| "—".into())
                            };
                            if let Some(temperature) = gpu.temperature {
                                status.push_str(&format!(" · {temperature:.0}°C"));
                            }
                            let mut values = Vec::new();
                            if !gpu.asleep {
                                let memory = gpu
                                    .memory
                                    .map(|(used, total)| {
                                        format!(
                                            "{:.2} / {:.2} GiB",
                                            used as f64 / 1_073_741_824.,
                                            total as f64 / 1_073_741_824.
                                        )
                                    })
                                    .unwrap_or_else(|| "VRAM —".into());
                                values.push(metric_text(memory, 10., p.muted));
                            }
                            values.push(metric_text(status, 12., p.foreground));
                            let indicator = if MetricHistories::enabled(preferences, section) {
                                histories.view(section, Some(&gpu.id), p)
                            } else {
                                gpu.utilization
                                    .map(|load| bar(&[(load / 100., p.accent)], p))
                                    .unwrap_or_else(|| neutral_rule(p))
                            };
                            metric_section(
                                spread(
                                    metric_name(section, title, p),
                                    row::with_children(values)
                                        .spacing(10)
                                        .align_y(Alignment::Center)
                                        .into(),
                                ),
                                indicator,
                            )
                        })
                        .collect();
                    column::with_children(devices).spacing(SECTION_GAP).into()
                }
            }
            DevboxSection::Uptime => metric_section(
                spread(
                    metric_name(section, "Uptime", p),
                    metric_text(
                        if m.uptime.is_empty() {
                            "—"
                        } else {
                            &m.uptime
                        },
                        12.,
                        p.foreground,
                    ),
                ),
                neutral_rule(p),
            ),
            DevboxSection::Services => {
                let dictation = match m.dictation.as_str() {
                    "idle" => "Ready",
                    "recording" => "Recording",
                    "transcribing" => "Transcribing",
                    _ => "Offline",
                };
                metric_section(
                    spread(
                        metric_name(section, "Services", p),
                        row::with_children(vec![
                            service_status("T3", if m.t3 { "On" } else { "Off" }, m.t3, p),
                            service_status(
                                "Dictation",
                                dictation,
                                matches!(
                                    m.dictation.as_str(),
                                    "idle" | "recording" | "transcribing"
                                ),
                                p,
                            ),
                        ])
                        .spacing(10)
                        .align_y(Alignment::Center)
                        .into(),
                    ),
                    neutral_rule(p),
                )
            }
        });
    }
    if rows.is_empty() {
        rows.push(
            container(spread(
                metric_text("All Devbox sections are hidden", 12., p.muted),
                action("Settings", Message::Settings, p.accent),
            ))
            .height(Length::Fixed(EMPTY_HEIGHT))
            .align_y(Alignment::Center)
            .into(),
        );
    }
    card(rows, p, config.appearance, Widget::Devbox, height)
}

fn memory_summary(metrics: &Metrics) -> String {
    if metrics.memory_total <= 0. {
        "Unavailable".into()
    } else {
        format!(
            "{:.1} / {:.0} GiB",
            metrics.memory_used, metrics.memory_total
        )
    }
}

fn swap_summary(swap: Option<(f32, f32)>) -> String {
    match swap {
        Some((_, total)) if total <= 0. => "Swap off".into(),
        Some((used, total)) => format!("Swap {used:.1}/{total:.1}"),
        None => "Swap —".into(),
    }
}

fn memory_header(
    memory: String,
    swap: Option<String>,
    cache: Option<String>,
    p: Palette,
) -> Element<'static, Message> {
    cosmic::widget::responsive(move |bounds| {
        let measure = |value: &str, size: f32| {
            type NativeParagraph = <cosmic::Renderer as advanced_text::Renderer>::Paragraph;
            NativeParagraph::with_text(advanced_text::Text {
                content: value,
                bounds: Size::new(f32::INFINITY, f32::INFINITY),
                size: size.into(),
                line_height: advanced_text::LineHeight::Relative(1.25),
                font: crate::widget_style::font(),
                align_x: advanced_text::Alignment::Left,
                align_y: cosmic::iced::alignment::Vertical::Top,
                shaping: advanced_text::Shaping::Auto,
                wrapping: advanced_text::Wrapping::None,
                ellipsize: advanced_text::Ellipsize::None,
            })
            .min_bounds()
            .width
        };
        let gap = 8.;
        let gaps = gap * (usize::from(swap.is_some()) + usize::from(cache.is_some())) as f32;
        // Reserve the icon, name and spread gap; fit only when the full readings
        // need more room. Swap stays smaller than RAM at every supported width.
        let available = (bounds.width - 16. - 6. - measure("RAM", 12.) - 8. - gaps - 2.).max(1.);
        let needed = measure(&memory, 12.)
            + swap.as_ref().map_or(0., |value| measure(value, 10.))
            + cache.as_ref().map_or(0., |value| measure(value, 10.));
        let scale = (available / needed.max(1.)).min(1.);
        let reading = |value: String, size, color, id: &'static str| {
            container(
                text::body(value)
                    .size(size)
                    .line_height(1.25)
                    .font(crate::widget_style::font())
                    .wrapping(advanced_text::Wrapping::None)
                    .class(cosmic::theme::Text::Color(color)),
            )
            .id(cosmic::widget::Id::new(id))
            .into()
        };
        let mut values = Vec::new();
        if let Some(swap) = &swap {
            values.push(reading(swap.clone(), 10. * scale, p.muted, "swap-summary"));
        }
        if let Some(cache) = &cache {
            values.push(reading(
                cache.clone(),
                10. * scale,
                p.muted,
                "cache-summary",
            ));
        }
        values.push(reading(
            memory.clone(),
            12. * scale,
            p.foreground,
            "ram-summary",
        ));
        spread(
            metric_name(DevboxSection::Memory, "RAM", p),
            row::with_children(values)
                .spacing(gap)
                .align_y(Alignment::Center)
                .into(),
        )
    })
    .into()
}

fn gpu_title(index: usize, count: usize, name: &str) -> String {
    let label = if count == 1 {
        "GPU".into()
    } else {
        format!("GPU {}", index + 1)
    };
    let name = name
        .split(" [")
        .next()
        .unwrap_or(name)
        .trim()
        .trim_start_matches("NVIDIA ")
        .trim_start_matches("AMD ")
        .trim_start_matches("ATI ")
        .trim_start_matches("GeForce ")
        .trim_start_matches("Radeon ");
    if name.is_empty() {
        label
    } else {
        // Keep the device identifiable without crowding inline memory readings.
        let short: String = name.chars().take(8).collect();
        format!(
            "{label} · {short}{}",
            if name.chars().count() > 8 { "…" } else { "" }
        )
    }
}

fn metric_text<'a>(value: impl Into<String>, size: f32, color: Color) -> Element<'a, Message> {
    text::body(value.into())
        .size(size)
        .line_height(1.25)
        .font(crate::widget_style::font())
        .class(cosmic::theme::Text::Color(color))
        .into()
}

fn metric_name<'a>(
    section: DevboxSection,
    title: impl Into<String>,
    p: Palette,
) -> Element<'a, Message> {
    let svg: &'static [u8] = match section {
        DevboxSection::Cpu => include_bytes!("../../assets/devbox/cpu.svg"),
        DevboxSection::Memory => include_bytes!("../../assets/devbox/memory.svg"),
        DevboxSection::Disk => include_bytes!("../../assets/devbox/disk.svg"),
        DevboxSection::Network => include_bytes!("../../assets/devbox/network.svg"),
        DevboxSection::Gpu => include_bytes!("../../assets/devbox/gpu.svg"),
        DevboxSection::Uptime => include_bytes!("../../assets/devbox/uptime.svg"),
        DevboxSection::Services => include_bytes!("../../assets/devbox/services.svg"),
    };
    row::with_children(vec![
        container(
            cosmic::widget::icon::from_svg_bytes(svg)
                .symbolic(true)
                .icon()
                .size(16)
                .class(cosmic::theme::Svg::custom(move |_| {
                    cosmic::iced::widget::svg::Style {
                        color: Some(p.muted),
                    }
                })),
        )
        .width(16)
        .height(16)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into(),
        metric_text(title, 12., p.muted),
    ])
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

fn indicator_height(preferences: &DevboxPreferences) -> f32 {
    if MetricHistories::any_enabled(preferences) {
        history::HEIGHT
    } else {
        INDICATOR_HEIGHT
    }
}

fn metric_row<'a>(
    header: Element<'a, Message>,
    indicator: Element<'a, Message>,
    height: f32,
) -> Element<'a, Message> {
    column::with_children(vec![
        container(header)
            .height(Length::Fixed(HEADER_HEIGHT))
            .align_y(Alignment::Center)
            .into(),
        container(indicator)
            .width(Length::Fill)
            .height(Length::Fixed(height))
            .align_y(Alignment::Center)
            .into(),
    ])
    .spacing(LINE_GAP)
    .into()
}

fn neutral_rule<'a>(p: Palette) -> Element<'a, Message> {
    // No fill or invented percentage for network, uptime and services.
    // Their neutral track occupies the same space as utilization indicators.
    bar(&[], p)
}

fn service_status<'a>(name: &str, status: &str, active: bool, p: Palette) -> Element<'a, Message> {
    row::with_children(vec![
        metric_text("●", 7., if active { p.accent } else { p.muted }),
        metric_text(format!("{name} {status}"), 12., p.foreground),
    ])
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

fn throughput(bytes_per_second: f64) -> String {
    if bytes_per_second >= 1_073_741_824. {
        format!("{:.1} GiB/s", bytes_per_second / 1_073_741_824.)
    } else if bytes_per_second >= 1_048_576. {
        format!("{:.1} MiB/s", bytes_per_second / 1_048_576.)
    } else if bytes_per_second >= 1_024. {
        format!("{:.0} KiB/s", bytes_per_second / 1_024.)
    } else {
        format!("{bytes_per_second:.0} B/s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::Application;

    #[test]
    fn gpu_labels_keep_the_model_in_the_limited_name_space() {
        assert_eq!(gpu_title(0, 1, "NVIDIA GeForce RTX 5090"), "GPU · RTX 5090");
        assert_eq!(
            gpu_title(0, 1, "Cezanne [Radeon Vega Series]"),
            "GPU · Cezanne"
        );
    }

    #[test]
    fn a_smaller_saved_size_grows_to_fit_every_reading() {
        // The saved size predates sections turned on later: the card expands
        // instead of scrolling its readings.
        let config = serde_json::from_value(serde_json::json!({
            "enabled_widgets":["devbox"],
            "widget_sizes":{"devbox":[360.0, 204.0]},
            "devbox":{"cpu":true,"memory":true,"disk":true,"network":true,
                "gpu":true,"uptime":true,"services":true,"swap":true,"memory_cache":true}
        }))
        .unwrap();
        let (mut cards, _) = Cards::init(Core::default(), config);
        cards.screen_size = [1920., 1080.];
        cards.metrics.gpus = vec![Default::default()];
        let natural = cards.devbox_natural_height();
        assert!(natural > 204., "{natural}");
        assert_eq!(cards.widget_size(Widget::Devbox), [360., natural]);
        // A larger saved size is kept, and a tiny screen caps the growth.
        cards
            .config
            .widget_sizes
            .insert("devbox".into(), [360., 400.]);
        assert_eq!(cards.widget_size(Widget::Devbox)[1], 400.);
        cards
            .config
            .widget_sizes
            .insert("devbox".into(), [360., 100.]);
        cards.screen_size = [800., 300.];
        assert!(cards.widget_size(Widget::Devbox)[1] <= 300. - 96. + 0.01);
    }

    #[test]
    fn devbox_primary_metrics_share_slots_and_memory_needs_no_extra_row() {
        for key in [
            "cpu", "memory", "disk", "network", "gpu", "uptime", "services",
        ] {
            let mut devbox = serde_json::json!({
                "cpu":false, "memory":false, "disk":false, "network":false,
                "gpu":false, "uptime":false, "services":false, "swap":false
            });
            devbox[key] = true.into();
            let config = serde_json::from_value(serde_json::json!({
                "enabled_widgets":[], "devbox":devbox
            }))
            .unwrap();
            let (mut cards, _) = Cards::init(Core::default(), config);
            cards.metrics.gpus = vec![Default::default()];
            assert_eq!(
                cards.devbox_natural_height(),
                61.,
                "{key} reserves its metric slot and category labels"
            );
            cards.config.devbox.swap = true;
            cards.config.devbox.memory_cache = true;
            assert_eq!(
                cards.devbox_natural_height(),
                61.,
                "optional swap and cache fit inside the RAM header"
            );
        }
    }

    #[test]
    fn history_reserves_equal_indicator_slots_for_all_visible_sections() {
        for second in ["memory", "disk", "network", "gpu", "uptime", "services"] {
            let mut devbox = serde_json::json!({
                "cpu":true, "memory":false, "disk":false, "network":false,
                "gpu":false, "uptime":false, "services":false,
                "swap":false,
                "history":{"cpu":true}
            });
            devbox[second] = true.into();
            let config = serde_json::from_value(serde_json::json!({
                "enabled_widgets":[], "devbox":devbox
            }))
            .unwrap();
            let (mut cards, _) = Cards::init(Core::default(), config);
            cards.metrics.gpus = vec![Default::default()];
            assert_eq!(
                cards.devbox_natural_height(),
                112.,
                "CPU and {second} share indicator height; RAM keeps its labels"
            );
            for _ in 0..400 {
                cards
                    .histories
                    .record(&cards.metrics, &cards.config.devbox, true);
            }
            assert_eq!(
                cards.devbox_natural_height(),
                112.,
                "sample count must not resize the card"
            );
            cards.config.devbox.cpu = false;
            assert_eq!(
                cards.devbox_natural_height(),
                61.,
                "a hidden CPU history must not enlarge {second}"
            );
        }
    }
}
