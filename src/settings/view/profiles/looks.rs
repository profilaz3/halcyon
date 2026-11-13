//! Curated Looks: the parts picker beside the preview and what the Looks
//! scene draws on top of the shared miniature. Every drawn part comes from
//! the preview, so it shows the look's value when its part is included and
//! the desktop's own value when it is left out.
use super::*;
use crate::desktop::desktop_icons::DesktopIcons;
use crate::desktop::panels::{Anchor, AutoHide, PanelDraft, PanelKind};
use crate::desktop::profiles::{IconShape, PanelLook, Profile};
use crate::settings::profiles::{Parts, Preview as ScenePreview};

/// A rectangle on the miniature: position and size.
pub(super) type Rect = (Point, [f32; 2]);

pub(super) fn part_icon(domain: Domain) -> &'static str {
    match domain {
        Domain::Dock => "view-app-grid-symbolic",
        Domain::Panel => "view-continuous-symbolic",
        Domain::Appearance => "applications-graphics-symbolic",
        Domain::Windows => "preferences-system-windows-symbolic",
        Domain::Workspaces => "view-paged-symbolic",
        Domain::Wallpaper => "image-x-generic-symbolic",
        Domain::DesktopIcons => "user-desktop-symbolic",
        Domain::WidgetStyle => "weather-few-clouds-symbolic",
        Domain::WidgetLayout => "view-grid-symbolic",
    }
}

fn sentence(items: Vec<String>) -> Option<String> {
    let mut text = items.into_iter().take(3).collect::<Vec<_>>().join(", ");
    let first = text.chars().next()?;
    text.replace_range(..first.len_utf8(), &first.to_uppercase().to_string());
    Some(text)
}

/// "halcyon-first-snow" as "First snow".
pub(super) fn picture_title(name: &str) -> String {
    let words = name
        .strip_prefix("halcyon-")
        .unwrap_or(name)
        .replace('-', " ");
    sentence(vec![words]).unwrap_or_default()
}

fn bar_note(look: &PanelLook, kind: PanelKind) -> Vec<String> {
    let mut items = Vec::new();
    if kind == PanelKind::Panel && look.islands.as_ref().is_some_and(|islands| islands.enabled) {
        items.push("islands".to_string());
    }
    match look.floating {
        Some(true) => items.push("floating".into()),
        Some(false) => items.push("edge to edge".into()),
        None => {}
    }
    if look.autohide == Some(AutoHide::Always) {
        items.push("hides until you need it".into());
    }
    match look.anchor {
        Some(Anchor::Bottom) if kind == PanelKind::Panel => items.push("at the bottom".into()),
        Some(Anchor::Top) if kind == PanelKind::Dock => items.push("at the top".into()),
        Some(Anchor::Left) => items.push("on the left".into()),
        Some(Anchor::Right) => items.push("on the right".into()),
        _ => {}
    }
    items
}

/// What the look sets in one part, in a few words ("Islands, floating").
pub(super) fn part_note(profile: &Profile, domain: Domain) -> String {
    use crate::desktop::profiles::{Corners, ModeName};
    let items: Vec<String> = match domain {
        Domain::Panel => profile
            .panel
            .as_ref()
            .map(|look| bar_note(look, PanelKind::Panel))
            .unwrap_or_default(),
        Domain::Dock => profile
            .dock
            .as_ref()
            .map(|look| bar_note(look, PanelKind::Dock))
            .unwrap_or_default(),
        Domain::Appearance => {
            let mut items = Vec::new();
            if let Some(look) = &profile.appearance {
                match look.mode {
                    Some(ModeName::Dark) => items.push("dark".to_string()),
                    Some(ModeName::Light) => items.push("light".into()),
                    Some(ModeName::Automatic) => items.push("light by day".into()),
                    None => items.push("colors".into()),
                }
                let variant = look.dark.as_ref().or(look.light.as_ref());
                match variant.and_then(|variant| variant.corners.as_ref()) {
                    Some(Corners::Preset(cosmic::cosmic_theme::Roundness::Round)) => {
                        items.push("round corners".into())
                    }
                    Some(Corners::Preset(cosmic::cosmic_theme::Roundness::SlightlyRound)) => {
                        items.push("soft corners".into())
                    }
                    Some(Corners::Preset(cosmic::cosmic_theme::Roundness::Square)) => {
                        items.push("square corners".into())
                    }
                    _ => {}
                }
                if let Some(font) = &look.interface_font {
                    items.push(font.family.clone());
                } else if let Some(theme) = &look.icon_theme {
                    items.push(format!("{theme} icons"));
                }
                if let Some(shape) = look.icon_shape.filter(|s| *s != IconShape::Original) {
                    items.push(format!("{} app icons", shape.title().to_lowercase()));
                }
                if look.folder_tint == Some(true) {
                    items.push("accent folders".into());
                }
            }
            items
        }
        Domain::Windows => {
            let mut items = Vec::new();
            if let Some(look) = &profile.windows {
                match look.active_hint {
                    Some(true) => items.push("active window outlined".to_string()),
                    Some(false) => items.push("no active outline".into()),
                    None => {}
                }
                match look.autotile {
                    Some(true) => items.push("tiling".into()),
                    Some(false) => items.push("free windows".into()),
                    None => {}
                }
                if look.show_minimize == Some(false) || look.show_maximize == Some(false) {
                    items.push("fewer title buttons".into());
                }
            }
            items
        }
        Domain::Workspaces => vec!["how workspaces are laid out".into()],
        Domain::Wallpaper => vec![match bundled_name(profile) {
            Some(name) => format!("{} picture", picture_title(name)),
            None => "its picture".into(),
        }],
        Domain::DesktopIcons => {
            let mut items = Vec::new();
            if let Some(look) = &profile.desktop_icons {
                match look.icon_size {
                    Some(size) if size < 100 => items.push("small icons".to_string()),
                    Some(size) if size > 100 => items.push("large icons".into()),
                    _ => {}
                }
                if look.show_folder == Some(false) {
                    items.push("a clear desktop".into());
                }
                if look.show_trash == Some(true) {
                    items.push("Trash shown".into());
                }
                if look.show_drives == Some(true) {
                    items.push("drives shown".into());
                }
            }
            items
        }
        Domain::WidgetStyle => {
            let style = profile.widget_style.as_ref();
            let on = style
                .and_then(|value| value.pointer("/rain/enabled"))
                .and_then(serde_json::Value::as_bool);
            let effect = style
                .and_then(|value| value.pointer("/rain/effect"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("rain");
            match on {
                Some(true) => vec![format!("{effect} on the wallpaper"), "card style".into()],
                Some(false) => vec!["card style".into(), "no weather effect".into()],
                None => vec!["card style".into()],
            }
        }
        Domain::WidgetLayout => vec!["which widgets show and where".into()],
    };
    sentence(items).unwrap_or_else(|| domain.title().to_string())
}

/// The quick choice that matches the parts now included, if any.
pub(super) fn matching_parts(
    profile: &Profile,
    included: impl Fn(Domain) -> bool,
) -> Option<Parts> {
    let domains = profile.domains();
    Parts::ALL.into_iter().find(|choice| {
        domains
            .iter()
            .all(|domain| choice.includes(*domain) == included(*domain))
    })
}

fn tinted(color: Color, alpha: f32) -> Color {
    Color {
        a: color.a * alpha,
        ..color
    }
}

fn block<'a>(
    size: [f32; 2],
    background: Color,
    border: Color,
    width: f32,
    radius: f32,
) -> Element<'a, Message> {
    container(
        cosmic::widget::space()
            .width(size[0].max(1.))
            .height(size[1].max(1.)),
    )
    .width(size[0].max(1.))
    .height(size[1].max(1.))
    .class(cosmic::theme::Container::custom(move |_| {
        cosmic::iced::widget::container::Style {
            background: Some(Background::Color(background)),
            border: Border {
                color: border,
                width,
                radius: radius.into(),
            },
            ..Default::default()
        }
    }))
    .into()
}

/// A small deterministic sequence in `0..1` for scattering particles.
fn scatter(count: usize, seed: u32) -> impl Iterator<Item = (f32, f32, f32)> {
    let mut state = seed.wrapping_mul(2_654_435_761).max(1);
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state % 10_000) as f32 / 10_000.
    };
    (0..count).map(move |_| (next(), next(), next()))
}

/// The weather effect over the wallpaper, below windows and widgets:
/// streaks for rain, flakes for snow, soft bands for fog and clouds.
pub(super) fn weather_layers<'a>(
    rain: &crate::rain::Preferences,
    screen: [f32; 2],
) -> Vec<Element<'a, Message>> {
    use crate::rain::Effect;
    if !rain.enabled {
        return Vec::new();
    }
    let strength = (f32::from(rain.intensity) / 100.).clamp(0.25, 1.);
    let area = screen[0] * screen[1];
    let mut layers = Vec::new();
    match rain.effect {
        Effect::Rain => {
            let count = ((area / 2600.) * strength).clamp(24., 140.) as usize;
            for (x, y, z) in scatter(count, 7) {
                let length = screen[1] * (0.03 + 0.04 * z);
                layers.push(
                    pin(block(
                        [1.2, length],
                        Color::from_rgba(0.85, 0.92, 1., 0.22 + 0.3 * z),
                        Color::TRANSPARENT,
                        0.,
                        0.6,
                    ))
                    .position(Point::new(x * screen[0], y * (screen[1] - length)))
                    .into(),
                );
            }
        }
        Effect::Snow => {
            let count = ((area / 2200.) * strength).clamp(30., 160.) as usize;
            for (x, y, z) in scatter(count, 11) {
                let size = 1.6 + 2.6 * z;
                layers.push(
                    pin(block(
                        [size, size],
                        Color::from_rgba(1., 1., 1., 0.55 + 0.4 * z),
                        Color::TRANSPARENT,
                        0.,
                        size / 2.,
                    ))
                    .position(Point::new(x * screen[0], y * screen[1]))
                    .into(),
                );
            }
        }
        Effect::Fog | Effect::Clouds => {
            let clouds = rain.effect == Effect::Clouds;
            let bands: &[(f32, f32)] = if clouds {
                &[(0.08, 0.22), (0.3, 0.18), (0.55, 0.2)]
            } else {
                &[(0.35, 0.3), (0.58, 0.32), (0.78, 0.26)]
            };
            for (index, (top, depth)) in bands.iter().enumerate() {
                let height = screen[1] * depth;
                let width = if clouds { screen[0] * 0.55 } else { screen[0] };
                let left = if clouds {
                    screen[0] * [0.05, 0.42, 0.18][index % 3]
                } else {
                    0.
                };
                let haze = Color::from_rgba(0.93, 0.95, 0.98, 0.32 * strength.max(0.6));
                let band = container(cosmic::widget::space().width(width).height(height))
                    .width(width)
                    .height(height)
                    .class(cosmic::theme::Container::custom(move |_| {
                        cosmic::iced::widget::container::Style {
                            background: Some(Background::Gradient(
                                cosmic::iced::gradient::Linear::new(0.)
                                    .add_stop(0., Color::TRANSPARENT)
                                    .add_stop(0.5, haze)
                                    .add_stop(1., Color::TRANSPARENT)
                                    .into(),
                            )),
                            border: Border {
                                radius: if clouds {
                                    (height / 2.).into()
                                } else {
                                    0.into()
                                },
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    }));
                layers.push(pin(band).position(Point::new(left, screen[1] * top)).into());
            }
        }
    }
    layers
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.0.x < b.0.x + b.1[0]
        && b.0.x < a.0.x + a.1[0]
        && a.0.y < b.0.y + b.1[1]
        && b.0.y < a.0.y + a.1[1]
}

/// Desktop icons as COSMIC Files lays them out: a column from the top
/// left, at the planned size and spacing, around the widgets. Returns the
/// icons and the area they cover.
pub(super) fn icon_layers<'a>(
    icons: &DesktopIcons,
    insets: [f32; 4],
    avoid: &[Rect],
    screen: [f32; 2],
    scale: f32,
    folder: Option<cosmic::widget::image::Handle>,
) -> (Vec<Element<'a, Message>>, Option<Rect>) {
    let mut items: Vec<(&'static str, &'static str)> = Vec::new();
    if icons.show_content {
        items.push(("folder", "Projects"));
        items.push(("text-x-generic", "Notes.txt"));
    }
    if icons.show_mounted_drives {
        items.push(("drive-harddisk", "Drive"));
    }
    if icons.show_trash {
        items.push(("user-trash", "Trash"));
    }
    if items.is_empty() {
        return (Vec::new(), None);
    }
    // Exaggerated like the bars so the size reads on the miniature.
    let zoom = (scale * 1.6).max(0.25);
    let size = (48. * f32::from(icons.icon_size.get()) / 100. * zoom).clamp(10., 56.);
    let spacing = f32::from(icons.grid_spacing.get()) / 100.;
    let cell = [size * 1.7 * spacing, (size + 14.) * 1.25 * spacing];
    let left = insets[3] + 6.;
    let top = insets[0] + 6.;
    let rows = (((screen[1] - insets[2] - top) / cell[1]).floor() as usize).max(1);
    let mut layers = Vec::new();
    let mut bounds: Option<Rect> = None;
    let mut slot = 0;
    for (name, title) in items {
        let position = loop {
            let column = slot / rows;
            let row = slot % rows;
            slot += 1;
            let position = Point::new(left + column as f32 * cell[0], top + row as f32 * cell[1]);
            if column > 6 || !avoid.iter().any(|area| overlaps((position, cell), *area)) {
                break position;
            }
        };
        let mut parts: Vec<Element<'a, Message>> = vec![match (&folder, name) {
            // The look's folder tint, drawn from its theme's folder.
            (Some(handle), "folder") => cosmic::widget::image(handle.clone())
                .width(Length::Fixed(size))
                .height(Length::Fixed(size))
                .into(),
            _ => icon::from_name(name).size(size as u16).icon().into(),
        }];
        if size >= 16. {
            parts.push(
                text::body(title)
                    .size((size / 4.).clamp(8., 11.))
                    .class(cosmic::theme::Text::Color(Color::from_rgba(
                        1., 1., 1., 0.92,
                    )))
                    .into(),
            );
        }
        layers.push(
            pin(container(
                column::with_children(parts)
                    .spacing(2)
                    .align_x(Alignment::Center),
            )
            .width(cell[0])
            .align_x(Alignment::Center))
            .position(position)
            .into(),
        );
        let area = (position, cell);
        bounds = Some(match bounds {
            None => area,
            Some((at, extent)) => {
                let x = at.x.min(area.0.x);
                let y = at.y.min(area.0.y);
                let right = (at.x + extent[0]).max(area.0.x + area.1[0]);
                let bottom = (at.y + extent[1]).max(area.0.y + area.1[1]);
                (Point::new(x, y), [right - x, bottom - y])
            }
        });
    }
    (layers, bounds)
}

/// Two tiled windows in the planned theme: colors, corners, outline, the
/// inner gap between them and, with the active hint on, the accent ring
/// around the focused one. Title buttons follow the window settings.
pub(super) fn window_pair<'a>(
    preview: &ScenePreview,
    insets: [f32; 4],
    screen: [f32; 2],
    scale: f32,
) -> Option<(Element<'a, Message>, Rect)> {
    use crate::desktop::windows::Behavior;
    let appearance = preview.appearance.as_ref()?;
    let variant = appearance.active_variant();
    let colors = appearance.preview(variant);
    let (_, inner) = appearance.gaps(variant).known().copied().unwrap_or((0, 8));
    let flag = |behavior, default| {
        preview
            .windows
            .as_ref()
            .and_then(|windows| windows.flag(behavior).known().copied())
            .unwrap_or(default)
    };
    let hint = flag(Behavior::ActiveHint, true);
    let buttons = 1
        + usize::from(flag(Behavior::ShowMinimize, true))
        + usize::from(flag(Behavior::ShowMaximize, true));
    let zoom = (scale * 2.).max(0.3);
    let gap = if inner == 0 {
        0.
    } else {
        (inner as f32 * zoom).max(1.5)
    };
    let free = [screen[0], screen[1] - insets[0] - insets[2]];
    let size = [screen[0] * 0.46, (free[1] * 0.5).max(70.)];
    let at = Point::new(
        (screen[0] - size[0]) / 2.,
        insets[0] + (free[1] - size[1]) * 0.62,
    );
    let first = ((size[0] - gap) * 0.6).floor();
    let second = size[0] - gap - first;
    let ring = if hint {
        (colors.outline_width.max(1) as f32 * zoom * 0.8).clamp(1.5, 3.)
    } else {
        0.
    };
    let radius = colors.radius_m.min(size[1] / 6.);
    let accent = color(colors.accent);
    let focused = container(super::schematic_window_with(
        colors.clone(),
        [first - 2. * ring, size[1] - 2. * ring],
        buttons,
    ))
    .padding(ring)
    .class(cosmic::theme::Container::custom(move |_| {
        cosmic::iced::widget::container::Style {
            border: Border {
                color: accent,
                width: ring,
                radius: (radius + ring).into(),
            },
            ..Default::default()
        }
    }));
    let other = super::schematic_window_with(colors, [second, size[1]], buttons);
    let pair = row::with_children(vec![focused.into(), other])
        .spacing(gap)
        .width(size[0])
        .height(size[1]);
    Some((pin(pair).position(at).into(), (at, size)))
}

/// A soft accent ring around a part the parts list points at.
pub(super) fn ring<'a>(area: Rect, accent: Color, radius: f32) -> Element<'a, Message> {
    let pad = 3.;
    let size = [area.1[0] + 2. * pad, area.1[1] + 2. * pad];
    pin(block(size, tinted(accent, 0.14), accent, 2., radius + pad))
        .position(Point::new(area.0.x - pad, area.0.y - pad))
        .into()
}

/// Where each part sits on the miniature, for the ring.
pub(super) fn part_areas(
    domain: Domain,
    bars: &[&PanelDraft],
    widgets: &[Rect],
    windows: Option<Rect>,
    icons: Option<Rect>,
    screen: [f32; 2],
    scale: f32,
) -> Vec<Rect> {
    let band = |kind: PanelKind| {
        super::super::desktop::preview::bar_bands(bars, screen, scale)
            .into_iter()
            .filter(move |(bar, _, _)| *bar == kind)
            .map(|(_, at, size)| (at, size))
    };
    match domain {
        Domain::Wallpaper => vec![(Point::new(3., 3.), [screen[0] - 6., screen[1] - 6.])],
        Domain::Panel => band(PanelKind::Panel).collect(),
        Domain::Dock => band(PanelKind::Dock).collect(),
        Domain::Appearance | Domain::Windows => windows.into_iter().collect(),
        Domain::DesktopIcons => icons.into_iter().collect(),
        Domain::WidgetStyle | Domain::WidgetLayout => widgets.to_vec(),
        Domain::Workspaces => band(PanelKind::Panel).collect(),
    }
}

/// What the picture can't show on its own: type, icons, light or dark,
/// the weather effect and a dock that hides. `(text, from the look, the
/// icons fact)`.
pub(super) fn facts(
    preview: &ScenePreview,
    config: &Config,
    from_look: impl Fn(Domain) -> bool,
) -> Vec<(String, bool, bool)> {
    use crate::desktop::appearance::{FontRole, ThemeVariant};
    let mut facts = Vec::new();
    if let Some(appearance) = &preview.appearance {
        let look = from_look(Domain::Appearance);
        if let Some(font) = appearance.font(FontRole::Interface).known() {
            facts.push((format!("Aa {}", font.family), look, false));
        }
        // The base theme with its shape and tint, never Halcyon's overlay.
        let theme = preview
            .icons
            .as_ref()
            .and_then(|style| style.base.clone())
            .or_else(|| appearance.icon_theme().known().cloned())
            .filter(|theme| !crate::desktop::icon_packs::overlay::is_overlay(theme));
        if let Some(theme) = theme {
            let mut items = vec![format!("{theme} icons")];
            if let Some(style) = &preview.icons {
                items.extend(crate::settings::profiles::icon_art::words(style));
            }
            facts.push((items.join(", "), look, true));
        }
        facts.push((
            match appearance.active_variant() {
                ThemeVariant::Dark => "Dark".to_string(),
                ThemeVariant::Light => "Light".to_string(),
            },
            look,
            false,
        ));
    }
    let weather = config.rain.enabled.then(|| config.rain.effect.title());
    facts.push((
        match weather {
            Some(effect) => format!("{effect} effect"),
            None => "No weather effect".into(),
        },
        from_look(Domain::WidgetStyle),
        false,
    ));
    for bar in &preview.bars {
        if bar.listed() && bar.autohide().known() == Some(&AutoHide::Always) {
            let domain = match bar.kind() {
                PanelKind::Dock => Domain::Dock,
                PanelKind::Panel => Domain::Panel,
            };
            facts.push((
                format!("{} hides until you need it", bar.kind().title()),
                from_look(domain),
                false,
            ));
        }
    }
    facts
}
