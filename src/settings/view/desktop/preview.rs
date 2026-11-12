//! Whole-desktop miniatures of both bars, for Setups and sharing. The Dock
//! and panel page draws its own close-up in `closeup`.
use super::*;
use cosmic::iced::Point;
use cosmic::iced::widget::{pin, stack};

fn bar_color(background: Option<&PanelBackground>) -> Color {
    match background {
        Some(PanelBackground::Light) => Color::from_rgb(0.90, 0.90, 0.92),
        Some(PanelBackground::Color([r, g, b])) => Color::from_rgb(*r, *g, *b),
        Some(PanelBackground::Dark) => Color::from_rgb(0.06, 0.06, 0.08),
        _ => cosmic::theme::active()
            .current_container()
            .component
            .base
            .into(),
    }
}

/// Edge, thickness and edge gap of a bar on the miniature. Small bars are
/// exaggerated so applets remain recognizable.
fn bar_metrics(draft: &PanelDraft, scale: f32) -> (Anchor, f32, f32) {
    let anchor = draft.anchor().known().copied().unwrap_or(Anchor::Bottom);
    let size = draft.size().known().cloned().unwrap_or(PanelSize::M);
    let padding = draft.padding().known().copied().unwrap_or(4) as f32;
    // Slightly thicker than true scale so bars read, without covering
    // widgets placed just below them.
    let zoom = (scale * 1.5).max(0.2);
    let thickness = ((size.thickness() as f32 + padding * 2.) * zoom).clamp(10., 36.);
    let margin = if draft.floating().known().copied().unwrap_or(false) {
        (f32::from(draft.margin().known().copied().unwrap_or(4)) * zoom).max(3.)
    } else {
        0.
    };
    (anchor, thickness, margin)
}

/// cosmic-panel's `get_priority`: on a shared edge, higher priority sits
/// closer to the screen edge.
pub(super) fn bar_priority(draft: &PanelDraft) -> u32 {
    let mut priority = 0;
    if draft.extend().known().copied().unwrap_or(true) {
        priority += 10000;
    }
    if draft
        .autohide()
        .known()
        .is_none_or(|mode| *mode == AutoHide::Never)
    {
        priority += 1000;
    }
    if draft.margin().known().copied().unwrap_or(0) == 0 {
        priority += 200;
    }
    if !draft.floating().known().copied().unwrap_or(false) {
        priority += 100;
    }
    if draft.kind() == PanelKind::Panel {
        priority += 10;
    }
    priority
}

/// One bar drawn to scale on the miniature screen, `offset` pixels in from
/// its edge when another bar shares that edge.
fn preview_bar<'a>(
    installed_islands: bool,
    draft: &PanelDraft,
    catalog: &AppletCatalog,
    selected: bool,
    screen: [f32; 2],
    scale: f32,
    offset: f32,
    accent: Color,
    on_press: Option<Message>,
    output: &str,
) -> Element<'a, Message> {
    if installed_islands && draft.islands().known().is_some_and(|s| s.enabled) {
        return preview_islands(
            draft, catalog, screen, scale, offset, accent, on_press, None, None, output, false,
        );
    }
    let (anchor, thickness, margin) = bar_metrics(draft, scale);
    let horizontal = anchor.horizontal();
    // Slightly thicker than true scale so bars read, without covering
    // widgets placed just below them.
    let zoom = (scale * 1.5).max(0.2);
    let extend = draft.extend().known().copied().unwrap_or(true);
    let hides = draft.autohide().known().copied();
    let opacity = draft.opacity().known().copied().unwrap_or(1.)
        * if hides == Some(AutoHide::Always) {
            0.45
        } else {
            1.
        };
    let radius = (draft.radius().known().copied().unwrap_or(0) as f32 * zoom).min(thickness / 2.);
    let spacing = (draft.spacing().known().copied().unwrap_or(0) as f32 * zoom + 2.).min(10.);
    let icon_size = (thickness * 0.6).clamp(7., 22.);
    let background = bar_color(draft.background().known());
    let regions = draft.applets().known().cloned().unwrap_or_default();
    let group = |region: Region| -> Vec<Element<'a, Message>> {
        regions
            .list(region)
            .iter()
            .flat_map(|id| catalog.icons(id))
            .map(|name| icon::from_name(name).size(icon_size as u16).icon().into())
            .collect()
    };
    let count: usize = [&regions.start, &regions.center, &regions.end]
        .into_iter()
        .flatten()
        .map(|id| catalog.icons(id).len())
        .sum();
    let natural = count as f32 * (icon_size + spacing) + thickness + 24.;
    let length_limit = if horizontal { screen[0] } else { screen[1] } - margin * 2.;
    let length = if extend {
        length_limit
    } else {
        natural.clamp(thickness * 2., length_limit)
    };
    let groups = [Region::Start, Region::Center, Region::End];
    let content: Element<'a, Message> = if horizontal {
        let mut parts: Vec<Element<'a, Message>> = Vec::new();
        for (index, region) in groups.into_iter().enumerate() {
            if index > 0 {
                parts.push(
                    container(text::body(""))
                        .width(if extend {
                            Length::Fill
                        } else {
                            Length::Fixed(8.)
                        })
                        .into(),
                );
            }
            parts.push(
                row::with_children(group(region))
                    .spacing(spacing as u16)
                    .align_y(Alignment::Center)
                    .into(),
            );
        }
        row::with_children(parts)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .into()
    } else {
        let mut parts: Vec<Element<'a, Message>> = Vec::new();
        for (index, region) in groups.into_iter().enumerate() {
            if index > 0 {
                parts.push(
                    container(text::body(""))
                        .height(if extend {
                            Length::Fill
                        } else {
                            Length::Fixed(8.)
                        })
                        .into(),
                );
            }
            parts.push(
                column::with_children(group(region))
                    .spacing(spacing as u16)
                    .align_x(Alignment::Center)
                    .into(),
            );
        }
        column::with_children(parts)
            .align_x(Alignment::Center)
            .height(Length::Fill)
            .into()
    };
    let (width, height) = if horizontal {
        (length, thickness)
    } else {
        (thickness, length)
    };
    let border = if selected {
        Color { a: 0.95, ..accent }
    } else {
        Color::from_rgba(1., 1., 1., 0.10)
    };
    let bar = container(content)
        .width(width)
        .height(height)
        .padding(if horizontal { [0, 8] } else { [8, 0] })
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .clip(true)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                icon_color: Some(crate::desktop_theme::ink(Color::WHITE, background)),
                text_color: Some(crate::desktop_theme::ink(Color::WHITE, background)),
                background: Some(Background::Color(Color {
                    a: opacity.clamp(0.08, 1.),
                    ..background
                })),
                border: Border {
                    color: border,
                    width: if selected { 1.5 } else { 1. },
                    radius: radius.into(),
                },
                ..Default::default()
            }
        }));
    let centered = |total: f32, length: f32| {
        if extend {
            margin
        } else {
            ((total - length) / 2.).max(0.)
        }
    };
    let inset = margin + offset;
    let position = match anchor {
        Anchor::Top => Point::new(centered(screen[0], width), inset),
        Anchor::Bottom => Point::new(centered(screen[0], width), screen[1] - height - inset),
        Anchor::Left => Point::new(inset, centered(screen[1], height)),
        Anchor::Right => Point::new(screen[0] - width - inset, centered(screen[1], height)),
    };
    match on_press {
        Some(message) => pin(cosmic::widget::tooltip(
            button::custom(bar)
                .class(super::super::home::hotspot(accent, radius))
                .padding(0)
                .on_press(message)
                .width(width)
                .height(height),
            text::body(match draft.kind() {
                PanelKind::Dock => "Dock",
                PanelKind::Panel => "Panel",
            }),
            cosmic::widget::tooltip::Position::Bottom,
        ))
        .position(position)
        .into(),
        None => pin(bar).position(position).into(),
    }
}

fn preview_islands<'a>(
    draft: &PanelDraft,
    catalog: &AppletCatalog,
    screen: [f32; 2],
    scale: f32,
    offset: f32,
    accent: Color,
    on_press: Option<Message>,
    measured: Option<&dock_islands_core::Layout>,
    selected: Option<dock_islands_core::Section>,
    output: &str,
    select_sections: bool,
) -> Element<'a, Message> {
    use dock_islands_core::{Section, Span};
    // Live measurements belong to the applied applets and size. Once those
    // change, estimate the draft so added/empty regions preview immediately.
    let measured = measured.filter(|_| {
        !draft.edited().any(|key| {
            matches!(
                key,
                "plugins_wings" | "plugins_center" | "size" | "padding" | "spacing" | "anchor"
            )
        })
    });
    let settings = draft.islands().known().cloned().unwrap_or_default();
    let (anchor, thickness, margin) = bar_metrics(draft, scale);
    let horizontal = anchor.horizontal();
    let major = if horizontal { screen[0] } else { screen[1] };
    let logical = measured
        .filter(|l| l.bounds.length > 0.)
        .map_or(major as f64 / scale as f64, |l| l.bounds.length);
    let natural = measured
        .filter(|l| l.spans.iter().any(Option::is_some))
        .map_or_else(
            || crate::settings::desktop::islands::preview_lengths(draft),
            |l| l.spans.map(|span| span.map_or(0., |span| span.length)),
        );
    let layout = dock_islands_core::resolve(
        &settings,
        output,
        Span {
            start: 0.,
            length: logical,
        },
        natural,
        32.,
    );
    let ratio = (major - margin * 2.) as f64 / logical.max(1.);
    // The positions and lengths above use desktop proportions. Keep the
    // cross-axis proportional too; the general bar preview exaggerates it
    // and would turn small one-applet islands into tall vertical pills.
    let thickness = thickness.min(
        layout
            .spans
            .iter()
            .flatten()
            .map(|span| (span.length * ratio) as f32)
            .fold(thickness, f32::min),
    );
    let regions = draft.applets().known().cloned().unwrap_or_default();
    let color = bar_color(draft.background().known());
    let opacity = draft.opacity().known().copied().unwrap_or(0.8);
    let mut layers = Vec::new();
    for (i, span) in layout.spans.iter().enumerate() {
        let Some(span) = span else {
            continue;
        };
        let length = (span.length * ratio) as f32;
        let (w, h) = if horizontal {
            (length, thickness)
        } else {
            (thickness, length)
        };
        let major_length = if horizontal { w } else { h };
        let icon_size =
            ((thickness * 0.58).clamp(7., 20.) as u16).min((major_length - 4.).max(1.) as u16);
        let spacing = (2. * scale * 2.).clamp(0.5, 2.);
        let capacity = ((major_length - 4. + spacing) / (icon_size as f32 + spacing))
            .floor()
            .max(1.) as usize;
        let ids: Vec<String> = regions
            .list(Region::ALL[i])
            .iter()
            .flat_map(|id| catalog.icons(id))
            .collect();
        let icons: Vec<Element<'a, Message>> = ids
            .iter()
            .take(capacity)
            .enumerate()
            .map(|(n, name)| {
                let name = if ids.len() > capacity && n + 1 == capacity {
                    "view-more-symbolic".to_string()
                } else {
                    name.clone()
                };
                icon::from_name(name).size(icon_size).icon().into()
            })
            .collect();
        let content: Element<'a, Message> = if horizontal {
            row::with_children(icons)
                .spacing(spacing)
                .align_y(Alignment::Center)
                .into()
        } else {
            column::with_children(icons)
                .spacing(spacing)
                .align_x(Alignment::Center)
                .into()
        };
        let is_selected = selected == Some(Section::ALL[i]);
        let radius =
            (draft.radius().known().copied().unwrap_or(12) as f32 * scale * 2.).min(thickness / 2.);
        let bg = container(content)
            .width(w)
            .height(h)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .clip(true)
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    icon_color: Some(crate::desktop_theme::ink(Color::WHITE, color)),
                    text_color: Some(crate::desktop_theme::ink(Color::WHITE, color)),
                    background: Some(Background::Color(Color {
                        a: opacity,
                        ..color
                    })),
                    border: Border {
                        color: if is_selected {
                            accent
                        } else {
                            Color::from_rgba(1., 1., 1., 0.18)
                        },
                        width: if is_selected { 2. } else { 1. },
                        radius: radius.into(),
                    },
                    ..Default::default()
                }
            }));
        let along = margin + (span.start * ratio) as f32;
        let pos = match anchor {
            Anchor::Top => Point::new(along, margin + offset),
            Anchor::Bottom => Point::new(along, screen[1] - h - margin - offset),
            Anchor::Left => Point::new(margin + offset, along),
            Anchor::Right => Point::new(screen[0] - w - margin - offset, along),
        };
        let message = on_press.clone().or_else(|| {
            select_sections.then(|| {
                edit(Edit::Islands(
                    crate::settings::desktop::islands::Edit::Select(Section::ALL[i]),
                ))
            })
        });
        let island: Element<'a, Message> = match message {
            Some(message) => button::custom(bg)
                .padding(0)
                .class(super::super::home::hotspot(accent, radius))
                .on_press(message)
                .width(w)
                .height(h)
                .into(),
            None => bg.into(),
        };
        layers.push(pin(island).position(pos).into());
    }
    stack(layers).width(screen[0]).height(screen[1]).into()
}

/// The listed bars of `drafts` on a miniature screen. Bars sharing an edge
/// stack outward by priority, like cosmic-panel; `selected` is drawn last,
/// above the other, and outlined. With `on_press`, each bar is a link.
pub(in crate::settings::view) fn bars<'a>(
    installed_islands: bool,
    drafts: Vec<&PanelDraft>,
    catalog: &AppletCatalog,
    selected: Option<PanelKind>,
    screen: [f32; 2],
    scale: f32,
    accent: Color,
    on_press: Option<fn(PanelKind) -> Message>,
) -> Vec<Element<'a, Message>> {
    bars_on_output(
        installed_islands,
        drafts,
        catalog,
        selected,
        screen,
        scale,
        accent,
        on_press,
        "",
    )
}

pub(in crate::settings::view) fn bars_on_output<'a>(
    installed_islands: bool,
    drafts: Vec<&PanelDraft>,
    catalog: &AppletCatalog,
    selected: Option<PanelKind>,
    screen: [f32; 2],
    scale: f32,
    accent: Color,
    on_press: Option<fn(PanelKind) -> Message>,
    output: &str,
) -> Vec<Element<'a, Message>> {
    let mut bars: Vec<&PanelDraft> = drafts.into_iter().filter(|draft| draft.listed()).collect();
    bars.sort_by_key(|draft| std::cmp::Reverse(bar_priority(draft)));
    let mut offsets = Vec::new();
    for (index, draft) in bars.iter().enumerate() {
        let (anchor, _, _) = bar_metrics(draft, scale);
        let offset: f32 = bars[..index]
            .iter()
            .map(|other| bar_metrics(other, scale))
            .filter(|(edge, _, _)| *edge == anchor)
            .map(|(_, thickness, margin)| thickness + margin)
            .sum();
        offsets.push((*draft, offset));
    }
    offsets.sort_by_key(|(draft, _)| Some(draft.kind()) == selected);
    offsets
        .into_iter()
        .map(|(draft, offset)| {
            preview_bar(
                installed_islands,
                draft,
                catalog,
                Some(draft.kind()) == selected,
                screen,
                scale,
                offset,
                accent,
                on_press.map(|message| message(draft.kind())),
                output,
            )
        })
        .collect()
}
