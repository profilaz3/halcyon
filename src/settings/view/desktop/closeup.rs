//! The Dock and panel page's picture: a close-up of the selected bar on the
//! band of wallpaper where it really sits. Bars draw near their real size so
//! applets read; whole-desktop miniatures stay in `preview`. The setup
//! wizard draws every bar this way, each with its name.
use super::*;
use crate::desktop::panels::Background as PanelBackground;
use cosmic::iced::Point;
use cosmic::iced::widget::{pin, stack};
use cosmic::widget::responsive;
use dock_islands_core::{Section, Span};

/// Bars draw at about this share of their real size: a large dock's icons
/// land near 29 px. Small bars zoom a little more so their icons stay at
/// 18 px or more; the largest stop at 32 px.
const ZOOM: f32 = 0.6;
/// Stands in for the clock applet's icon: the panel shows the time as text.
const CLOCK: &str = "com.system76.CosmicAppletTime";
/// How far the wallpaper band zooms past fitting the screen's width.
const WALLPAPER_ZOOM: f32 = 1.6;

/// A bar's shape at close-up scale, in frame pixels.
struct Look {
    anchor: Anchor,
    /// Frame pixels per real pixel.
    zoom: f32,
    /// Applet ink on this bar's background.
    ink: Color,
    /// One applet's square slot across the bar.
    cell: f32,
    icon: f32,
    /// Across the bar, padding included.
    thickness: f32,
    /// Gap to the screen edge when floating.
    margin: f32,
    radius: f32,
    spacing: f32,
    padding: f32,
    extend: bool,
    opacity: f32,
    color: Color,
}

impl Look {
    fn of(draft: &PanelDraft) -> Self {
        let size = draft.size().known().cloned().unwrap_or(PanelSize::M);
        let padding = draft.padding().known().copied().unwrap_or(4) as f32;
        // The non-symbolic applet icon inside `PanelSize::thickness`.
        let icon = match size {
            PanelSize::XS => 24.,
            PanelSize::S => 32.,
            PanelSize::M => 40.,
            PanelSize::L => 48.,
            PanelSize::XL => 56.,
            PanelSize::Custom(_) => size.thickness() as f32 * 0.75,
        };
        let zoom = (icon * ZOOM).clamp(18., 32.) / icon;
        let icon = (icon * zoom).round();
        let cell = (size.thickness() as f32 * zoom).max(icon + 4.).round();
        let padding = (padding * zoom).round();
        let thickness = cell + padding * 2.;
        let margin = if draft.floating().known().copied().unwrap_or(false) {
            (f32::from(draft.margin().known().copied().unwrap_or(4)) * zoom).max(3.)
        } else {
            0.
        };
        let color = bar_color(draft.background().known());
        Self {
            anchor: draft.anchor().known().copied().unwrap_or(Anchor::Bottom),
            zoom,
            ink: crate::desktop_theme::ink(Color::WHITE, color),
            cell,
            icon,
            thickness,
            margin,
            radius: (draft.radius().known().copied().unwrap_or(0) as f32 * zoom)
                .min(thickness / 2.),
            spacing: draft.spacing().known().copied().unwrap_or(0) as f32 * zoom,
            padding,
            extend: draft.extend().known().copied().unwrap_or(true),
            opacity: draft.opacity().known().copied().unwrap_or(1.),
            color,
        }
    }

    fn horizontal(&self) -> bool {
        self.anchor.horizontal()
    }

    /// Room the bar takes from its edge.
    fn depth(&self) -> f32 {
        self.thickness + self.margin
    }

    /// The same bar drawn `factor` times as large, for a dock longer than
    /// the picture. Icons stay 18 px or more.
    fn shrunk(self, factor: f32) -> Self {
        let factor = factor.clamp((18. / self.icon).min(1.), 1.);
        Self {
            zoom: self.zoom * factor,
            cell: (self.cell * factor).round(),
            icon: (self.icon * factor).round(),
            thickness: (self.cell * factor).round() + (self.padding * factor).round() * 2.,
            margin: self.margin * factor,
            radius: self.radius * factor,
            spacing: self.spacing * factor,
            padding: (self.padding * factor).round(),
            ..self
        }
    }
}

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

/// What the bar does when you aren't using it, said beside the picture.
fn caption(draft: &PanelDraft) -> Option<&'static str> {
    if !draft.listed() {
        return Some("Turned off");
    }
    match draft.autohide().known() {
        Some(AutoHide::Always) => Some("Hidden until you point at the edge"),
        Some(AutoHide::OnOverlap) => Some("Hides behind windows"),
        _ => None,
    }
}

/// How faint a bar draws: hidden and turned-off bars are see-through.
/// A bar that is turned off is only a faint shape, like the other bar. A
/// hidden bar is drawn whole: its note says when it shows.
fn presence(draft: &PanelDraft, subject: bool) -> f32 {
    let base = if subject { 1. } else { 0.4 };
    if draft.listed() { base } else { base * 0.35 }
}

fn applet_icons(draft: &PanelDraft, catalog: &AppletCatalog, region: Region) -> Vec<String> {
    draft
        .applets()
        .known()
        .map(|regions| {
            regions
                .list(region)
                .iter()
                .flat_map(|id| {
                    if id == CLOCK {
                        vec![CLOCK.to_string()]
                    } else {
                        catalog.icons(id)
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A run of applet icons, each in its square slot. Icons past `capacity`
/// fold into a "more" icon.
fn icon_run<'a>(names: &[String], look: &Look, capacity: usize) -> Element<'a, Message> {
    let capacity = capacity.max(1);
    let icons: Vec<Element<'a, Message>> = names
        .iter()
        .take(capacity)
        .enumerate()
        .map(|(n, name)| {
            let name = if names.len() > capacity && n + 1 == capacity {
                "view-more-symbolic"
            } else {
                name.as_str()
            };
            if name == CLOCK {
                return clock(look);
            }
            let ink = look.ink;
            let mut glyph = icon::from_name(name).size(look.icon as u16).icon();
            // Symbolic icons take the bar's ink, as on the desktop; app
            // icons keep their colours.
            if name.ends_with("-symbolic") {
                glyph = glyph.class(cosmic::theme::Svg::custom(move |_| {
                    cosmic::iced::widget::svg::Style { color: Some(ink) }
                }));
            }
            container(glyph)
                .width(look.cell)
                .height(look.cell)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into()
        })
        .collect();
    if look.horizontal() {
        row::with_children(icons)
            .spacing(look.spacing)
            .align_y(Alignment::Center)
            .into()
    } else {
        column::with_children(icons)
            .spacing(look.spacing)
            .align_x(Alignment::Center)
            .into()
    }
}

/// The clock applet: the date and time as the panel writes them, upright on
/// a side bar.
fn clock<'a>(look: &Look) -> Element<'a, Message> {
    let now = chrono::Local::now();
    let size = (look.icon * 0.62).clamp(11., 16.);
    let words = if look.horizontal() {
        now.format("%b %-d %H:%M").to_string()
    } else {
        now.format("%H\n%M").to_string()
    };
    let slot = container(label(words, size, look.ink))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);
    if look.horizontal() {
        slot.height(look.cell).padding([0., look.cell * 0.2]).into()
    } else {
        slot.width(look.cell).padding([look.cell * 0.2, 0.]).into()
    }
}

/// Length of the clock along the bar; one slot on a side bar.
fn clock_length(look: &Look) -> f32 {
    if look.horizontal() {
        let size = (look.icon * 0.62).clamp(11., 16.);
        ("Nov 12 22:32".len() as f32 * size * 0.58 + look.cell * 0.4).max(look.cell)
    } else {
        look.cell
    }
}

/// Length of a run of applets along the bar.
fn run_length(look: &Look, names: &[String]) -> f32 {
    if names.is_empty() {
        return 0.;
    }
    names
        .iter()
        .map(|name| {
            if name == CLOCK {
                clock_length(look)
            } else {
                look.cell
            }
        })
        .sum::<f32>()
        + (names.len() - 1) as f32 * look.spacing
}

/// How many slots fit in `length`.
fn capacity(look: &Look, length: f32) -> usize {
    ((length + look.spacing) / (look.cell + look.spacing))
        .floor()
        .max(1.) as usize
}

fn surface<'a>(
    content: Element<'a, Message>,
    look: &Look,
    size: [f32; 2],
    alpha: f32,
    outline: Option<Color>,
) -> Element<'a, Message> {
    let color = look.color;
    let fill = (look.opacity * alpha).clamp(0.06, 1.);
    let radius = look.radius;
    let ink = crate::desktop_theme::ink(Color::WHITE, color);
    container(content)
        .width(size[0])
        .height(size[1])
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .clip(true)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                icon_color: Some(Color { a: alpha, ..ink }),
                text_color: Some(Color { a: alpha, ..ink }),
                background: Some(Background::Color(Color { a: fill, ..color })),
                border: Border {
                    color: outline.unwrap_or(Color::from_rgba(1., 1., 1., 0.14 * alpha.min(1.))),
                    width: if outline.is_some() { 2. } else { 1. },
                    radius: radius.into(),
                },
                shadow: cosmic::iced::Shadow {
                    color: Color::from_rgba(0., 0., 0., 0.28 * alpha),
                    offset: cosmic::iced::Vector::new(0., 2.),
                    blur_radius: 10.,
                },
                ..Default::default()
            }
        }))
        .into()
}

/// Where a piece `size` long sits on `edge`, `along` pixels in from the
/// frame's start and `inset` pixels in from its edge.
fn place(look: &Look, frame: [f32; 2], size: [f32; 2], along: f32, inset: f32) -> Point {
    match look.anchor {
        Anchor::Top => Point::new(along, inset),
        Anchor::Bottom => Point::new(along, frame[1] - size[1] - inset),
        Anchor::Left => Point::new(inset, along),
        Anchor::Right => Point::new(frame[0] - size[0] - inset, along),
    }
}

fn press<'a>(
    body: Element<'a, Message>,
    look: &Look,
    size: [f32; 2],
    accent: Color,
    message: Option<Message>,
) -> Element<'a, Message> {
    match message {
        Some(message) => button::custom(body)
            .class(super::super::home::hotspot(accent, look.radius))
            .padding(0)
            .on_press(message)
            .width(size[0])
            .height(size[1])
            .into(),
        None => body,
    }
}

/// One whole bar: its start, middle and end applets, edge to edge when it
/// extends, otherwise as long as its applets.
fn whole_bar<'a>(
    draft: &PanelDraft,
    catalog: &AppletCatalog,
    frame: [f32; 2],
    inset: f32,
    alpha: f32,
    accent: Color,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let mut look = Look::of(draft);
    let horizontal = look.horizontal();
    let major = if horizontal { frame[0] } else { frame[1] };
    // A faint bar is only a shape; icons can't fade with it.
    let groups = Region::ALL.map(|region| {
        if alpha < 0.5 {
            Vec::new()
        } else {
            applet_icons(draft, catalog, region)
        }
    });
    let natural = |look: &Look| {
        let gap = look.cell * 0.5;
        groups
            .iter()
            .filter(|names| !names.is_empty())
            .map(|names| run_length(look, names) + gap)
            .sum::<f32>()
            - gap
            + look.padding * 2.
    };
    // A dock longer than the picture draws a little smaller to fit whole.
    let room = (major - look.margin * 2.).max(look.thickness);
    // Rounding to whole pixels can leave it a little long; try again.
    for _ in 0..4 {
        if look.extend || natural(&look) <= room || look.icon <= 18. {
            break;
        }
        let factor = room / natural(&look) * 0.98;
        look = look.shrunk(factor);
    }
    let gap = look.cell * 0.5;
    let room = (major - look.margin * 2.).max(look.thickness);
    let natural = natural(&look);
    let length = if look.extend {
        room
    } else {
        natural.clamp(look.thickness * 2., room)
    };
    // Each run keeps to its share of the bar; a long one folds into "more".
    let share = ((length - look.padding * 2.) / 3.).max(look.cell);
    // A compact bar shares its whole length; what is left after earlier runs.
    let mut left = length - look.padding * 2.;
    let mut parts: Vec<Element<'a, Message>> = Vec::new();
    for (index, names) in groups.iter().enumerate() {
        if index > 0 && look.extend {
            parts.push(if horizontal {
                cosmic::widget::space().width(Length::Fill).into()
            } else {
                cosmic::widget::space().height(Length::Fill).into()
            });
        } else if !names.is_empty() && groups[..index].iter().any(|run| !run.is_empty()) {
            parts.push(if horizontal {
                cosmic::widget::space().width(gap).into()
            } else {
                cosmic::widget::space().height(gap).into()
            });
        }
        let limit = if look.extend {
            capacity(&look, share)
        } else {
            capacity(&look, left)
        };
        if !names.is_empty() {
            left -= run_length(&look, &names[..names.len().min(limit)]) + gap;
        }
        parts.push(icon_run(names, &look, limit));
    }
    let content: Element<'a, Message> = if horizontal {
        row::with_children(parts)
            .align_y(Alignment::Center)
            .padding([0., look.padding])
            .width(Length::Fill)
            .into()
    } else {
        column::with_children(parts)
            .align_x(Alignment::Center)
            .padding([look.padding, 0.])
            .height(Length::Fill)
            .into()
    };
    let size = if horizontal {
        [length, look.thickness]
    } else {
        [look.thickness, length]
    };
    let along = if look.extend {
        look.margin
    } else {
        ((major - length) / 2.).max(0.)
    };
    let body = surface(content, &look, size, alpha, None);
    pin(press(body, &look, size, accent, on_press))
        .position(place(&look, frame, size, along, look.margin + inset))
        .into()
}

/// The islands of a segmented bar. The layout's positions are shares of
/// the screen, so resolving them over the frame keeps their arrangement.
#[allow(clippy::too_many_arguments)]
fn islands<'a>(
    draft: &PanelDraft,
    catalog: &AppletCatalog,
    frame: [f32; 2],
    inset: f32,
    alpha: f32,
    accent: Color,
    measured: Option<&dock_islands_core::Layout>,
    selected: Option<Section>,
    output: &str,
    on_press: Option<Message>,
    selectable: bool,
) -> Vec<Element<'a, Message>> {
    let look = Look::of(draft);
    let horizontal = look.horizontal();
    let major = if horizontal { frame[0] } else { frame[1] };
    // Live measurements belong to the applied applets and size. Once those
    // change, estimate the draft so added or emptied regions show at once.
    let measured = measured.filter(|_| {
        !draft.edited().any(|key| {
            matches!(
                key,
                "plugins_wings" | "plugins_center" | "size" | "padding" | "spacing" | "anchor"
            )
        })
    });
    let natural = measured
        .filter(|l| l.spans.iter().any(Option::is_some))
        .map_or_else(
            || {
                // The estimate counts square slots; the clock is wider.
                let mut lengths = crate::settings::desktop::islands::preview_lengths(draft);
                for (i, length) in lengths.iter_mut().enumerate() {
                    let clocks = applet_icons(draft, catalog, Region::ALL[i])
                        .iter()
                        .filter(|name| *name == CLOCK)
                        .count();
                    *length +=
                        clocks as f64 * ((clock_length(&look) - look.cell) / look.zoom) as f64;
                }
                lengths
            },
            |l| l.spans.map(|span| span.map_or(0., |span| span.length)),
        );
    let settings = draft.islands().known().cloned().unwrap_or_default();
    let room = (major - look.margin * 2.).max(1.);
    let layout = dock_islands_core::resolve(
        &settings,
        output,
        Span {
            start: 0.,
            length: (room / look.zoom) as f64,
        },
        natural,
        32.,
    );
    let mut layers = Vec::new();
    for (i, span) in layout.spans.iter().enumerate() {
        let Some(span) = span else {
            continue;
        };
        let length = ((span.length as f32) * look.zoom).max(look.thickness);
        let size = if horizontal {
            [length, look.thickness]
        } else {
            [look.thickness, length]
        };
        let names = if alpha < 0.5 {
            Vec::new()
        } else {
            applet_icons(draft, catalog, Region::ALL[i])
        };
        let run = icon_run(&names, &look, capacity(&look, length - look.padding * 2.));
        let outline = (selected == Some(Section::ALL[i])).then_some(accent);
        let body = surface(run, &look, size, alpha, outline);
        let message = on_press.clone().or_else(|| {
            selectable.then(|| {
                edit(Edit::Islands(
                    crate::settings::desktop::islands::Edit::Select(Section::ALL[i]),
                ))
            })
        });
        let along = look.margin + span.start as f32 * look.zoom;
        layers.push(
            pin(press(body, &look, size, accent, message))
                .position(place(&look, frame, size, along, look.margin + inset))
                .into(),
        );
    }
    layers
}

/// The part of the wallpaper behind `frame`: a band along `anchor`, zoomed
/// past the screen's width so the bar and the desktop share one scale.
/// Without an anchor, the middle of the screen.
fn wallpaper_band(
    handle: &cosmic::widget::image::Handle,
    anchor: Option<Anchor>,
    screen: [f32; 2],
    frame: [f32; 2],
) -> Option<cosmic::iced::Rectangle<u32>> {
    let cosmic::widget::image::Handle::Rgba { width, height, .. } = handle else {
        return None;
    };
    let image = [*width as f32, *height as f32];
    let [sw, sh] = [screen[0].max(1.), screen[1].max(1.)];
    // The visible part of the screen, matching the frame's shape.
    let mut seen = [
        sw / WALLPAPER_ZOOM,
        sw / WALLPAPER_ZOOM * frame[1] / frame[0],
    ];
    if seen[1] > sh {
        seen = [sh * frame[0] / frame[1], sh];
    }
    if seen[0] > sw {
        seen = [sw, sw * frame[1] / frame[0]];
    }
    let corner = match anchor {
        Some(Anchor::Top) => [(sw - seen[0]) / 2., 0.],
        Some(Anchor::Bottom) => [(sw - seen[0]) / 2., sh - seen[1]],
        Some(Anchor::Left) => [0., (sh - seen[1]) / 2.],
        Some(Anchor::Right) => [sw - seen[0], (sh - seen[1]) / 2.],
        None => [(sw - seen[0]) / 2., (sh - seen[1]) / 2.],
    };
    // The desktop covers the screen with the wallpaper, centred.
    let cover = (sw / image[0]).max(sh / image[1]);
    let shift = [(image[0] * cover - sw) / 2., (image[1] * cover - sh) / 2.];
    let x = ((corner[0] + shift[0]) / cover).max(0.);
    let y = ((corner[1] + shift[1]) / cover).max(0.);
    let w = (seen[0] / cover).min(image[0] - x).max(1.);
    let h = (seen[1] / cover).min(image[1] - y).max(1.);
    Some(cosmic::iced::Rectangle {
        x: x as u32,
        y: y as u32,
        width: w as u32,
        height: h as u32,
    })
}

impl Settings {
    /// The picture's height: a band a little deeper than the bars on a top
    /// or bottom edge; taller for a side, so the bar has length to show.
    pub(super) fn bar_closeup_height(&self) -> f32 {
        let editor = &self.desktop;
        let Some(draft) = editor.current() else {
            return 140.;
        };
        let look = Look::of(draft);
        if !look.horizontal() {
            return (self.content_width() * 0.42).clamp(220., 300.);
        }
        let shared: f32 = PanelKind::ALL
            .into_iter()
            .filter(|kind| *kind != editor.selected)
            .filter_map(|kind| editor.draft(kind))
            .filter(|other| other.listed())
            .map(|other| Look::of(other))
            .filter(|other| other.anchor == look.anchor)
            .map(|other| other.depth())
            .sum();
        (look.depth() + shared + 76.).clamp(112., 168.)
    }

    pub(super) fn bar_closeup(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let screen = self.placement.screen();
        let editor = &self.desktop;
        let height = self.bar_closeup_height();
        responsive(move |available| {
            let frame = [available.width.max(1.), height];
            let Some(subject) = editor.current() else {
                return cosmic::widget::space().into();
            };
            let look = Look::of(subject);
            let mut layers: Vec<Element<'_, Message>> = vec![
                cosmic::widget::space()
                    .width(frame[0])
                    .height(frame[1])
                    .into(),
            ];
            // The already decoded wallpaper: no image work starts here.
            let band = self.profiles.home_image().and_then(|handle| {
                Some((
                    handle,
                    wallpaper_band(handle, Some(look.anchor), screen, frame)?,
                ))
            });
            match band {
                Some((handle, crop)) => layers.push(
                    cosmic::widget::image(handle.clone())
                        .crop(crop)
                        .width(frame[0])
                        .height(frame[1])
                        .content_fit(cosmic::iced::ContentFit::Cover)
                        .border_radius(10.)
                        .into(),
                ),
                None => layers.extend(self.home_backdrop(frame)),
            }
            // On a shared edge the bar nearer the screen edge has priority,
            // like cosmic-panel. The other bar only shows when it is there.
            let mut bars: Vec<&PanelDraft> = PanelKind::ALL
                .into_iter()
                .filter_map(|kind| editor.draft(kind))
                .filter(|draft| {
                    draft.kind() == editor.selected
                        || (draft.listed() && Look::of(draft).anchor == look.anchor)
                })
                .collect();
            bars.sort_by_key(|draft| std::cmp::Reverse(super::preview::bar_priority(draft)));
            let mut inset = 0.;
            let mut drawn: Vec<Element<'_, Message>> = Vec::new();
            for draft in bars {
                let subject = draft.kind() == editor.selected;
                let alpha = presence(draft, subject);
                let islands_on = editor.islands.installation.installed()
                    && draft.islands().known().is_some_and(|s| s.enabled);
                let on_press = (!subject).then(|| edit(Edit::Select(draft.kind())));
                if islands_on {
                    drawn.extend(islands(
                        draft,
                        &editor.catalog,
                        frame,
                        inset,
                        alpha,
                        accent,
                        subject
                            .then_some(editor.islands.geometry.as_ref())
                            .flatten(),
                        subject.then_some(editor.islands.selected).flatten(),
                        &editor.island_output(),
                        on_press,
                        true,
                    ));
                } else {
                    drawn.push(whole_bar(
                        draft,
                        &editor.catalog,
                        frame,
                        inset,
                        alpha,
                        accent,
                        on_press,
                    ));
                }
                let other = Look::of(draft);
                if draft.listed() {
                    inset += other.depth();
                }
            }
            // The subject draws above the faint bar it shares an edge with.
            drawn.reverse();
            layers.extend(drawn);
            if let Some(words) = caption(subject) {
                layers.push(note(words, &look, frame));
            }
            container(stack(layers).width(frame[0]).height(frame[1]).clip(true))
                .width(Length::Fill)
                .height(frame[1])
                .class(cosmic::theme::Container::custom(|_| {
                    cosmic::iced::widget::container::Style {
                        border: Border {
                            radius: 10.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                }))
                .into()
        })
        .width(Length::Fill)
        .height(height)
        .into()
    }
}

/// Light words on a dark pill, readable on any wallpaper.
fn chip<'a>(words: impl Into<String>) -> cosmic::widget::Container<'a, Message, cosmic::Theme> {
    container(label(words.into(), 12., Color::WHITE))
        .padding([4, 10])
        .class(cosmic::theme::Container::custom(|_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(Color::from_rgba(0.05, 0.05, 0.07, 0.72))),
                border: Border {
                    radius: 12.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
}

/// A small chip on the far side from the bar.
fn note<'a>(words: &'static str, look: &Look, frame: [f32; 2]) -> Element<'a, Message> {
    let chip = chip(words);
    let at = match look.anchor {
        Anchor::Top => Point::new(12., frame[1] - 36.),
        Anchor::Bottom | Anchor::Left => Point::new(frame[0] - 12. - 260., 12.),
        Anchor::Right => Point::new(12., 12.),
    };
    pin(container(chip).width(260.).align_x(match look.anchor {
        Anchor::Bottom | Anchor::Left => Alignment::End,
        _ => Alignment::Start,
    }))
    .position(at)
    .into()
}

/// What a bar is called in the setup wizard's picture, with what it does
/// when you aren't using it.
fn named(draft: &PanelDraft) -> String {
    let name = match draft.kind() {
        PanelKind::Dock => "Dock",
        PanelKind::Panel => "Panel",
    };
    match caption(draft) {
        Some(words) => format!("{name}: {}", words.to_lowercase()),
        None => name.to_string(),
    }
}

impl Settings {
    /// The setup wizard's picture: every bar close up on `wallpaper`, as the
    /// wizard's choices would leave it, each named beside it. Nothing in it
    /// can be pressed.
    pub(in crate::settings::view) fn wizard_bars_closeup<'a>(
        &'a self,
        drafts: Vec<PanelDraft>,
        wallpaper: Option<cosmic::widget::image::Handle>,
        height: f32,
    ) -> Element<'a, Message> {
        let accent = self.settings_accent();
        let screen = self.placement.screen();
        let editor = &self.desktop;
        let installed = editor.islands.installation.installed();
        responsive(move |available| {
            let frame = [available.width.max(1.), height];
            let mut layers: Vec<Element<'_, Message>> = vec![
                cosmic::widget::space()
                    .width(frame[0])
                    .height(frame[1])
                    .into(),
            ];
            let band = wallpaper
                .as_ref()
                .and_then(|handle| Some((handle, wallpaper_band(handle, None, screen, frame)?)));
            match band {
                Some((handle, crop)) => layers.push(
                    cosmic::widget::image(handle.clone())
                        .crop(crop)
                        .width(frame[0])
                        .height(frame[1])
                        .content_fit(cosmic::iced::ContentFit::Cover)
                        .border_radius(10.)
                        .into(),
                ),
                None => layers.extend(self.home_backdrop(frame)),
            }
            // Bars sharing an edge stack outward by priority, like
            // cosmic-panel; a bar that is turned off is a faint shape.
            let mut bars: Vec<&PanelDraft> = drafts.iter().collect();
            bars.sort_by_key(|draft| std::cmp::Reverse(super::preview::bar_priority(draft)));
            let mut insets: Vec<(Anchor, f32, usize)> = Vec::new();
            let mut chips: Vec<Element<'_, Message>> = Vec::new();
            for draft in bars {
                let look = Look::of(draft);
                let at = insets.iter().position(|(edge, _, _)| *edge == look.anchor);
                let (inset, shared) = at.map_or((0., 0), |i| (insets[i].1, insets[i].2));
                let alpha = presence(draft, true);
                let islands_on = installed && draft.islands().known().is_some_and(|s| s.enabled);
                if islands_on {
                    layers.extend(islands(
                        draft,
                        &editor.catalog,
                        frame,
                        inset,
                        alpha,
                        accent,
                        None,
                        None,
                        &editor.island_output(),
                        None,
                        false,
                    ));
                } else {
                    layers.push(whole_bar(
                        draft,
                        &editor.catalog,
                        frame,
                        inset,
                        alpha,
                        accent,
                        None,
                    ));
                }
                let depth = if draft.listed() { look.depth() } else { 0. };
                match at {
                    Some(i) => {
                        insets[i].1 += depth;
                        insets[i].2 += 1;
                    }
                    None => insets.push((look.anchor, depth, 1)),
                }
                // Beside the bar, toward the middle of the picture. A second
                // bar on the same edge names itself at the other end.
                let off = inset + look.depth().max(look.thickness) + 8.;
                let centred = !look.extend && shared == 0;
                let along = match (centred, shared) {
                    (true, _) => Alignment::Center,
                    (false, 0) => Alignment::Start,
                    _ => Alignment::End,
                };
                let tag = chip(named(draft));
                let tag: Element<'_, Message> = if look.horizontal() {
                    let y = match look.anchor {
                        Anchor::Top => off,
                        _ => frame[1] - off - CHIP_HEIGHT,
                    };
                    pin(container(tag)
                        .width(frame[0])
                        .padding([0., 12.])
                        .align_x(along))
                    .position(Point::new(0., y))
                    .into()
                } else {
                    let x = match look.anchor {
                        Anchor::Left => off,
                        _ => 0.,
                    };
                    let width = (frame[0] - off).max(1.);
                    pin(container(tag)
                        .width(width)
                        .height(frame[1])
                        .padding([12., 0.])
                        .align_x(if look.anchor == Anchor::Left {
                            Alignment::Start
                        } else {
                            Alignment::End
                        })
                        .align_y(along))
                    .position(Point::new(x, 0.))
                    .into()
                };
                chips.push(tag);
            }
            layers.extend(chips);
            container(stack(layers).width(frame[0]).height(frame[1]).clip(true))
                .width(Length::Fill)
                .height(frame[1])
                .into()
        })
        .width(Length::Fill)
        .height(height)
        .into()
    }
}

/// A chip's height: 12 px words and its padding.
const CHIP_HEIGHT: f32 = 24.;
