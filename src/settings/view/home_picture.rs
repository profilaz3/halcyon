//! Home: the whole desktop in one picture. Each part is a sidebar page.
//! Picking a part lights it, names it and says how it is set; picking it
//! again, or Open, goes to its page. The picture draws cached images only
//! and has nothing that moves.
use super::activate::{activate, quiet};
use super::profiles::{card, scene_geometry, schematic_window};
use super::*;
use crate::desktop::panels::{Anchor, PanelDraft, PanelKind, PanelSize};
use cosmic::iced::widget::{pin, stack};
use cosmic::iced::{Point, Rectangle};
use cosmic::widget::{canvas, responsive};

/// The pages Home has a tile for, in sidebar order.
pub(in crate::settings) const PARTS: [Page; 10] = [
    Page::DesktopWallpaper,
    Page::WeatherEffects,
    Page::Overview,
    Page::DesktopPanel,
    Page::DesktopDock,
    Page::ActivityIsland,
    Page::Themes,
    Page::DesktopWindows,
    Page::DesktopMotion,
    Page::General,
];

/// What a click on a part does: select it (Home) or open its page (the
/// small picture on a part's own page).
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Picks {
    Select,
    Open,
}

impl Picks {
    fn message(self, page: Page) -> Message {
        match self {
            Self::Select => Message::HomePart(page),
            Self::Open => Message::Page(page),
        }
    }
}

pub(in crate::settings) fn part_id(page: Page) -> String {
    format!("home-part-{}", page.key())
}
pub(in crate::settings) fn tile_id(page: Page) -> String {
    format!("home-tile-{}", page.key())
}

/// A bar's box on the miniature: its edge, thickness and gap as the bar
/// preview draws them. Used to place the island and the name label.
fn bar_rect(draft: &PanelDraft, screen: [f32; 2], scale: f32) -> (Anchor, Rectangle) {
    let anchor = draft.anchor().known().copied().unwrap_or(Anchor::Bottom);
    let size = draft.size().known().cloned().unwrap_or(PanelSize::M);
    let padding = draft.padding().known().copied().unwrap_or(4) as f32;
    let zoom = (scale * 1.5).max(0.2);
    let thickness = ((size.thickness() as f32 + padding * 2.) * zoom).clamp(10., 36.);
    let margin = if draft.floating().known().copied().unwrap_or(false) {
        (f32::from(draft.margin().known().copied().unwrap_or(4)) * zoom).max(3.)
    } else {
        0.
    };
    let extend = draft.extend().known().copied().unwrap_or(true);
    let full = if anchor.horizontal() {
        screen[0]
    } else {
        screen[1]
    } - margin * 2.;
    let length = if extend { full } else { full * 0.5 };
    let along = if extend {
        margin
    } else {
        (full + margin * 2. - length) / 2.
    };
    let rect = match anchor {
        Anchor::Top => Rectangle::new(Point::new(along, margin), [length, thickness].into()),
        Anchor::Bottom => Rectangle::new(
            Point::new(along, screen[1] - thickness - margin),
            [length, thickness].into(),
        ),
        Anchor::Left => Rectangle::new(Point::new(margin, along), [thickness, length].into()),
        Anchor::Right => Rectangle::new(
            Point::new(screen[0] - thickness - margin, along),
            [thickness, length].into(),
        ),
    };
    (anchor, rect)
}

pub(super) fn has_island(draft: &PanelDraft) -> bool {
    draft.applets().known().is_some_and(|regions| {
        crate::desktop::panels::Region::ALL.iter().any(|r| {
            regions
                .list(*r)
                .iter()
                .any(|id| id == crate::activity_island::APPLET_ID)
        })
    })
}

/// The island pill: round artwork and a level meter on a dark capsule.
fn island_pill<'a>(size: [f32; 2], dark: bool, accent: Color) -> Element<'a, Message> {
    let h = size[1];
    let art = container(text::body(""))
        .width(h * 0.6)
        .height(h * 0.6)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Gradient(cosmic::iced::Gradient::Linear(
                    cosmic::iced::gradient::Linear::new(cosmic::iced::Radians(0.8))
                        .add_stop(0., Color::from_rgb(0.97, 0.70, 0.40))
                        .add_stop(0.5, Color::from_rgb(0.96, 0.52, 0.37))
                        .add_stop(1., Color::from_rgb(0.49, 0.36, 0.65)),
                ))),
                border: Border {
                    radius: (h * 0.3).into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }));
    let level = |fraction: f32| -> Element<'a, Message> {
        container(text::body(""))
            .width((h * 0.1).max(1.5))
            .height(h * fraction)
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    background: Some(Background::Color(accent)),
                    border: Border {
                        radius: 1.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }))
            .into()
    };
    let meter = row::with_children(vec![level(0.3), level(0.5), level(0.38)])
        .spacing((h * 0.08).max(1.) as u16)
        .align_y(Alignment::End);
    let surface = if dark {
        Color::from_rgb(0.02, 0.03, 0.03)
    } else {
        Color::from_rgb(0.12, 0.13, 0.15)
    };
    container(
        row::with_children(vec![
            art.into(),
            cosmic::widget::space().width(Length::Fill).into(),
            meter.into(),
        ])
        .align_y(Alignment::Center),
    )
    .width(size[0])
    .height(h)
    .padding([0., h * 0.25])
    .align_y(Alignment::Center)
    .class(cosmic::theme::Container::custom(move |_| {
        cosmic::iced::widget::container::Style {
            background: Some(Background::Color(surface)),
            border: Border {
                radius: (h / 2.).into(),
                color: Color::from_rgba(1., 1., 1., 0.12),
                width: 1.,
            },
            ..Default::default()
        }
    }))
    .into()
}

/// The effect, frozen: streaks for rain, flakes for snow, bands for fog
/// and soft shade for clouds. Drawn once per size from a fixed pattern.
struct Weather {
    effect: crate::rain::Effect,
    strength: f32,
}

impl canvas::Program<Message, cosmic::Theme> for Weather {
    type State = canvas::Cache;
    fn draw(
        &self,
        cache: &canvas::Cache,
        renderer: &cosmic::Renderer,
        _: &cosmic::Theme,
        bounds: Rectangle,
        _: cosmic::iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        use crate::rain::Effect;
        let geometry = cache.draw(renderer, bounds.size(), |frame| {
            let [w, h] = [bounds.width, bounds.height];
            // A fixed sequence, so the picture is the same every time.
            let mut seed = 0x2545_f491_u32;
            let mut next = move || {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed % 10_000) as f32 / 10_000.
            };
            let alpha = self.strength;
            match self.effect {
                Effect::Rain => {
                    let length = (h * 0.06).clamp(6., 22.);
                    for _ in 0..((w * h) / 900.).clamp(40., 420.) as usize {
                        let x = next() * (w + length) - length * 0.2;
                        let y = next() * h * 0.82;
                        // Rain thins out toward the bottom.
                        let fade = 1. - y / (h * 0.82);
                        frame.stroke(
                            &canvas::Path::line(
                                Point::new(x, y),
                                Point::new(x - length * 0.22, y + length),
                            ),
                            canvas::Stroke::default()
                                .with_width(1.)
                                .with_color(Color::from_rgba(
                                    0.88,
                                    0.94,
                                    0.96,
                                    alpha * (0.35 + 0.65 * fade),
                                )),
                        );
                    }
                }
                Effect::Snow => {
                    for _ in 0..((w * h) / 1400.).clamp(30., 260.) as usize {
                        let radius = 0.8 + next() * (w / 400.).clamp(1., 2.4);
                        frame.fill(
                            &canvas::Path::circle(Point::new(next() * w, next() * h), radius),
                            Color::from_rgba(1., 1., 1., alpha * (0.5 + next() * 0.5)),
                        );
                    }
                }
                Effect::Fog => {
                    for band in 0..4 {
                        let y = h * (0.35 + band as f32 * 0.16);
                        frame.fill(
                            &canvas::Path::rectangle(Point::new(0., y), [w, h * 0.14].into()),
                            Color::from_rgba(0.92, 0.94, 0.96, alpha * 0.28),
                        );
                    }
                }
                Effect::Clouds => {
                    for _ in 0..5 {
                        let radius = w * (0.12 + next() * 0.1);
                        frame.fill(
                            &canvas::Path::circle(Point::new(next() * w, next() * h), radius),
                            Color::from_rgba(0., 0., 0., alpha * 0.22),
                        );
                    }
                }
            }
        });
        vec![geometry]
    }
}

/// A flat fill over the whole picture.
fn veil<'a>(screen: [f32; 2], color: Color) -> Element<'a, Message> {
    quiet(
        container(text::body(""))
            .width(screen[0])
            .height(screen[1])
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    background: Some(Background::Color(color)),
                    border: Border {
                        radius: 10.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })),
    )
}

/// The accent ring around a lit part, with a soft halo outside it.
fn ring<'a>(rect: Rectangle, accent: Color, radius: f32) -> Vec<Element<'a, Message>> {
    let outline = |grow: f32, width: f32, color: Color| -> Element<'a, Message> {
        pin(quiet(
            container(text::body(""))
                .width(rect.width + grow * 2.)
                .height(rect.height + grow * 2.)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        border: Border {
                            color,
                            width,
                            radius: (radius + grow).into(),
                        },
                        ..Default::default()
                    }
                })),
        ))
        .position(Point::new(rect.x - grow, rect.y - grow))
        .into()
    };
    vec![
        outline(5., 4., Color { a: 0.28, ..accent }),
        outline(2., 2., accent),
    ]
}

/// The lit part's name in a small accent pill, below the part when it
/// fits and above it otherwise.
fn tag<'a>(
    name: &'static str,
    rect: Rectangle,
    screen: [f32; 2],
    accent: Color,
) -> Element<'a, Message> {
    let width = name.chars().count() as f32 * 6.6 + 20.;
    let height = 20.;
    let x = (rect.center_x() - width / 2.).clamp(4., (screen[0] - width - 4.).max(4.));
    let below = rect.y + rect.height + 8.;
    let y = if below + height <= screen[1] - 4. {
        below
    } else if rect.y - height - 8. >= 4. {
        rect.y - height - 8.
    } else {
        (rect.center_y() - height / 2.).max(4.)
    };
    pin(quiet(
        container(text::body(name).size(12).font(cosmic::font::semibold()))
            .padding([2, 10])
            .class(cosmic::theme::Container::custom(move |theme| {
                cosmic::iced::widget::container::Style {
                    text_color: Some(theme.cosmic().on_accent_color().into()),
                    background: Some(Background::Color(accent)),
                    border: Border {
                        radius: 10.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })),
    ))
    .position(Point::new(x, y))
    .into()
}

/// A card for one tile or the chosen part's row.
fn surface<'a>(selected: bool, accent: Color) -> cosmic::theme::Button {
    let style = move |theme: &cosmic::Theme, hovered: bool, focused: bool| {
        let container = theme.current_container();
        cosmic::widget::button::Style {
            background: Some(Background::Color(if hovered || selected {
                container.component.hover.into()
            } else {
                container.component.base.into()
            })),
            border_color: if selected || focused {
                accent
            } else {
                Color::TRANSPARENT
            },
            border_width: if selected || focused { 2. } else { 0. },
            border_radius: 10.0.into(),
            ..Default::default()
        }
    };
    cosmic::theme::Button::Custom {
        active: Box::new(move |focused, theme| style(theme, false, focused)),
        hovered: Box::new(move |focused, theme| style(theme, true, focused)),
        pressed: Box::new(move |focused, theme| style(theme, true, focused)),
        disabled: Box::new(move |theme| style(theme, false, false)),
    }
}

impl Settings {
    /// The page Settings opens on.
    pub(super) fn home_page(&self) -> Element<'_, Message> {
        let selected = self.home_part;
        let width = self.content_width();
        // 16:9, and short enough that the chosen part's row stays in view.
        let height = (width * 9. / 16.)
            .min((self.window.height - 250.).max(200.))
            .max(160.);
        let picture = container(self.home_scene(height, selected, Picks::Select))
            .id(cosmic::widget::Id::new("home-picture"))
            .width(Length::Fill);
        let mut parts = vec![heading(
            "Home",
            "Everything Halcyon changes, in one picture. Pick a part to change it.",
        )];
        // Settings opens here, so first-run setup and what's new wait here.
        if self.needs_setup() {
            parts.push(self.setup_card());
        }
        parts.extend(self.updated_card());
        parts.extend([
            picture.into(),
            self.home_detail(selected),
            self.home_tiles(selected),
        ]);
        let body = column::with_children(parts).spacing(12);
        container(activate(body.into()))
            .id(cosmic::widget::Id::new("home"))
            .width(Length::Fill)
            .into()
    }

    /// The same picture, small, at the top of a part's own page with that
    /// part lit. Each part opens its page.
    pub(super) fn home_glance(&self, part: Page) -> Element<'_, Message> {
        let height = (self.content_width() * 9. / 16.).clamp(150., 220.);
        container(activate(self.home_scene(height, Some(part), Picks::Open)))
            .id(cosmic::widget::Id::new(format!("home-glance-{}", part.key())))
            .width(Length::Fill)
            .into()
    }

    /// The bars as Home draws them: the live editor drafts.
    fn home_bars(&self) -> Vec<&PanelDraft> {
        PanelKind::ALL
            .into_iter()
            .filter_map(|kind| self.desktop.draft(kind))
            .collect()
    }

    fn effects_on(&self) -> Option<&crate::rain::Preferences> {
        Some(self.wallpaper.effect().unwrap_or(&self.config.rain)).filter(|effect| effect.enabled)
    }

    fn home_scene(&self, height: f32, lit: Option<Page>, picks: Picks) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let config = &self.config;
        let layout = scene_geometry(self.placement.preview_snapshot().clone(), config);
        let screen_size = layout.screen;
        let bars = self.home_bars();
        let island = bars
            .iter()
            .find(|draft| draft.kind() == PanelKind::Panel && draft.listed())
            .filter(|draft| has_island(draft))
            .copied();
        let effect = self.effects_on().map(|effect| effect.effect);
        let window = self
            .system_appearance
            .draft()
            .map(|draft| draft.preview(draft.active_variant()));
        // Only parts drawn in the picture light up in it.
        let lit = lit.filter(|page| match page {
            Page::ActivityIsland => island.is_some(),
            Page::WeatherEffects => effect.is_some(),
            Page::DesktopMotion | Page::General => false,
            Page::DesktopPanel | Page::DesktopDock => bars
                .iter()
                .any(|draft| Some(draft.kind()) == page.bar() && draft.listed()),
            _ => true,
        });
        responsive(move |available| {
            let scale = (available.width.max(1.) / screen_size[0].max(1.))
                .min(available.height.max(1.) / screen_size[1].max(1.));
            let screen = [screen_size[0] * scale, screen_size[1] * scale];
            let hotspot = |content: Element<'static, Message>,
                           page: Page,
                           id: String,
                           name: String,
                           radius: f32,
                           size: [f32; 2]|
             -> Element<'static, Message> {
                button::custom(content)
                    .class(super::home::hotspot(accent, radius))
                    .padding(0)
                    .width(size[0])
                    .height(size[1])
                    .id(cosmic::widget::Id::new(id))
                    .name(name)
                    .selected(lit == Some(page))
                    .on_press(picks.message(page))
                    .into()
            };
            let mut layers: Vec<Element<'_, Message>> = vec![
                cosmic::widget::space()
                    .width(screen[0])
                    .height(screen[1])
                    .into(),
            ];
            // Wallpaper: the whole picture; empty space picks it.
            let backdrop = self.home_backdrop(screen).into_iter().next();
            if let Some(backdrop) = backdrop {
                layers.push(
                    button::custom(backdrop)
                        .class(super::home::hotspot(accent, 10.))
                        .padding(0)
                        .width(screen[0])
                        .height(screen[1])
                        .id(cosmic::widget::Id::new(part_id(Page::DesktopWallpaper)))
                        .name("Wallpaper")
                        .selected(lit == Some(Page::DesktopWallpaper))
                        .on_press(picks.message(Page::DesktopWallpaper))
                        .into(),
                );
            }
            // Widgets where they sit on the desktop.
            let mut cards = Vec::new();
            for geometry in &layout.widgets {
                let Some(widget) = Widget::parse(&geometry.key) else {
                    continue;
                };
                if !config.widgets_visible || !config.widget_visible(widget) {
                    continue;
                }
                let size = [geometry.size[0] * scale, geometry.size[1] * scale];
                let at = Point::new(geometry.position[0] * scale, geometry.position[1] * scale);
                cards.push((widget, Rectangle::new(at, size.into())));
            }
            let draw_card = |widget: Widget, size: [f32; 2]| -> Element<'static, Message> {
                if widget == Widget::Clock
                    && let Some(handle) = self.depth_preview.image(true)
                {
                    return cosmic::widget::image(handle.clone())
                        .width(size[0])
                        .height(size[1])
                        .content_fit(cosmic::iced::ContentFit::Fill)
                        .into();
                }
                card(widget, size, config, scale)
            };
            for (widget, rect) in &cards {
                let size = [rect.width, rect.height];
                layers.push(
                    pin(hotspot(
                        draw_card(*widget, size),
                        Page::Overview,
                        format!("home-part-widget-{}", widget.key()),
                        format!("Widgets: {}", widget.title()),
                        6.,
                        size,
                    ))
                    .position(rect.position())
                    .into(),
                );
            }
            // An app window, in the planned window colors.
            let window_size = [screen[0] * 0.36, (screen[1] * 0.34).max(48.)];
            let window_rect = Rectangle::new(
                Point::new(
                    (screen[0] - window_size[0]) / 2.,
                    (screen[1] - window_size[1]) * 0.42,
                ),
                window_size.into(),
            );
            let draw_window = || -> Option<Element<'static, Message>> {
                window.map(|preview| schematic_window(preview, window_size))
            };
            if let Some(drawn) = draw_window() {
                layers.push(
                    pin(hotspot(
                        drawn,
                        Page::DesktopWindows,
                        part_id(Page::DesktopWindows),
                        "Windows".into(),
                        6.,
                        window_size,
                    ))
                    .position(window_rect.position())
                    .into(),
                );
            }
            // The bars at their real edges.
            let output = self.placement.display.as_deref().unwrap_or("");
            let installed = self.desktop.islands.installation.installed();
            layers.extend(super::desktop::preview::bars_on_output(
                installed,
                bars.clone(),
                &self.desktop.catalog,
                None,
                screen,
                scale,
                accent,
                None,
                output,
            ));
            // Each bar picks its own page, under a named, focusable spot.
            for draft in bars.iter().filter(|draft| draft.listed()) {
                let page = Page::of_bar(draft.kind());
                let rect = bar_rect(draft, screen, scale).1;
                let size = [rect.width.max(8.), rect.height.max(8.)];
                layers.push(
                    pin(hotspot(
                        cosmic::iced::widget::space().width(size[0]).height(size[1]).into(),
                        page,
                        part_id(page),
                        page.nav_title().into(),
                        rect.height.min(rect.width) / 2.,
                        size,
                    ))
                    .position(rect.position())
                    .into(),
                );
            }
            // The island over the panel's middle.
            let island_rect = island.map(|panel| {
                let (anchor, rect) = bar_rect(panel, screen, scale);
                if anchor.horizontal() {
                    let h = rect.height * 0.78;
                    let w = (h * 3.4).max(36.);
                    Rectangle::new(
                        Point::new(rect.center_x() - w / 2., rect.center_y() - h / 2.),
                        [w, h].into(),
                    )
                } else {
                    let w = rect.width * 0.78;
                    let h = (w * 2.2).max(28.);
                    Rectangle::new(
                        Point::new(rect.center_x() - w / 2., rect.center_y() - h / 2.),
                        [w, h].into(),
                    )
                }
            });
            let dark_island =
                config.activity_island.style == crate::activity_island::preferences::IslandStyle::Dark;
            if let Some(rect) = island_rect {
                let size = [rect.width, rect.height];
                layers.push(
                    pin(hotspot(
                        island_pill(size, dark_island, accent),
                        Page::ActivityIsland,
                        part_id(Page::ActivityIsland),
                        "Activity island".into(),
                        rect.height.min(rect.width) / 2.,
                        size,
                    ))
                    .position(rect.position())
                    .into(),
                );
            }
            // The effect over everything, with a badge to pick it by.
            let top_inset = bars
                .iter()
                .filter(|draft| draft.listed())
                .map(|draft| bar_rect(draft, screen, scale))
                .filter(|(anchor, _)| *anchor == Anchor::Top)
                .map(|(_, rect)| rect.y + rect.height)
                .fold(0., f32::max);
            let badge = (screen[1] * 0.085).clamp(22., 34.);
            let badge_rect = Rectangle::new(
                Point::new(
                    screen[0] - badge - screen[0] * 0.025,
                    top_inset + screen[1] * 0.04,
                ),
                [badge, badge].into(),
            );
            let weather = |strength: f32| -> Option<Element<'static, Message>> {
                effect.map(|effect| {
                    quiet(
                        canvas::Canvas::new(Weather { effect, strength })
                            .width(screen[0])
                            .height(screen[1]),
                    )
                })
            };
            let draw_badge = || -> Element<'static, Message> {
                container(
                    icon::from_name(Page::WeatherEffects.nav_icon())
                        .size((badge * 0.55) as u16)
                        .icon(),
                )
                .width(badge)
                .height(badge)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .class(cosmic::theme::Container::custom(|_| {
                    cosmic::iced::widget::container::Style {
                        icon_color: Some(Color::WHITE),
                        background: Some(Background::Color(Color::from_rgba(
                            0.05, 0.07, 0.09, 0.72,
                        ))),
                        border: Border {
                            radius: 999.0.into(),
                            color: Color::from_rgba(1., 1., 1., 0.18),
                            width: 1.,
                        },
                        ..Default::default()
                    }
                }))
                .into()
            };
            if let Some(overlay) = weather(0.55) {
                layers.push(overlay);
                layers.push(
                    pin(hotspot(
                        draw_badge(),
                        Page::WeatherEffects,
                        part_id(Page::WeatherEffects),
                        "Effects".into(),
                        badge / 2.,
                        [badge, badge],
                    ))
                    .position(badge_rect.position())
                    .into(),
                );
            }
            // The chosen part: everything else dims, it is redrawn above the
            // dimming with an accent ring and its name.
            let bar_of = |kind: PanelKind| {
                bars.iter()
                    .find(|draft| draft.kind() == kind && draft.listed())
                    .copied()
            };
            let lit_bar = |kind: PanelKind| -> Vec<Element<'static, Message>> {
                bar_of(kind)
                    .map(|draft| {
                        super::desktop::preview::bars_on_output(
                            installed,
                            vec![draft],
                            &self.desktop.catalog,
                            Some(kind),
                            screen,
                            scale,
                            accent,
                            None,
                            output,
                        )
                        .into_iter()
                        .map(quiet)
                        .collect()
                    })
                    .unwrap_or_default()
            };
            let bar_box = |kind: PanelKind| bar_of(kind).map(|d| bar_rect(d, screen, scale).1);
            let lit_card = |widget: Widget, rect: Rectangle| -> Element<'static, Message> {
                pin(quiet(draw_card(widget, [rect.width, rect.height])))
                    .position(rect.position())
                    .into()
            };
            let lit_window = || -> Option<Element<'static, Message>> {
                draw_window().map(|drawn| {
                    pin(quiet(drawn))
                        .position(window_rect.position())
                        .into()
                })
            };
            let lit_island = |rect: Rectangle| -> Element<'static, Message> {
                pin(quiet(island_pill(
                    [rect.width, rect.height],
                    dark_island,
                    accent,
                )))
                .position(rect.position())
                .into()
            };
            let shade = |alpha: f32| veil(screen, Color::from_rgba(0., 0., 0., alpha));
            match lit {
                Some(Page::DesktopWallpaper) => {
                    // The picture itself comes forward, the parts stay hinted.
                    if let Some(handle) = self.profiles.home_image() {
                        layers.push(quiet(
                            cosmic::widget::image(handle.clone())
                                .width(screen[0])
                                .height(screen[1])
                                .content_fit(cosmic::iced::ContentFit::Cover)
                                .border_radius(10.)
                                .opacity(0.78),
                        ));
                    } else {
                        layers.push(shade(0.3));
                    }
                    layers.extend(ring(
                        Rectangle::new(Point::ORIGIN, screen.into()),
                        accent,
                        10.,
                    ));
                    layers.push(tag(
                        "Wallpaper",
                        Rectangle::new(
                            Point::new(screen[0] / 2., screen[1] * 0.5),
                            [0., 0.].into(),
                        ),
                        screen,
                        accent,
                    ));
                }
                Some(Page::Overview) => {
                    layers.push(shade(0.42));
                    let mut union: Option<Rectangle> = None;
                    for (widget, rect) in &cards {
                        layers.push(lit_card(*widget, *rect));
                        layers.extend(ring(*rect, accent, 6.));
                        union = Some(union.map_or(*rect, |u| u.union(rect)));
                    }
                    if let Some(union) = union {
                        layers.push(tag("Widgets", union, screen, accent));
                    }
                }
                Some(Page::DesktopWindows) => {
                    layers.push(shade(0.42));
                    layers.extend(lit_window());
                    layers.extend(ring(window_rect, accent, 6.));
                    layers.push(tag("Windows", window_rect, screen, accent));
                }
                Some(page @ (Page::DesktopPanel | Page::DesktopDock)) => {
                    let kind = page.bar().unwrap_or(PanelKind::Panel);
                    layers.push(shade(0.42));
                    layers.extend(lit_bar(kind));
                    if let Some(rect) = bar_box(kind) {
                        layers.extend(ring(rect, accent, rect.height.min(rect.width) / 2.));
                        layers.push(tag(page.title(), rect, screen, accent));
                    }
                }
                Some(Page::ActivityIsland) => {
                    layers.push(shade(0.42));
                    if let Some(rect) = island_rect {
                        layers.push(lit_island(rect));
                        layers.extend(ring(rect, accent, rect.height.min(rect.width) / 2.));
                        layers.push(tag("Activity island", rect, screen, accent));
                    }
                }
                Some(Page::WeatherEffects) => {
                    layers.push(shade(0.18));
                    layers.extend(weather(0.95));
                    layers.extend(ring(badge_rect, accent, badge / 2.));
                    layers.push(tag("Effects", badge_rect, screen, accent));
                }
                Some(Page::Themes) => {
                    // Appearance styles all the chrome: bars, island, window
                    // and widget cards light together.
                    layers.push(shade(0.24));
                    for (widget, rect) in &cards {
                        layers.push(lit_card(*widget, *rect));
                        layers.extend(ring(*rect, accent, 6.));
                    }
                    if window.is_some() {
                        layers.extend(lit_window());
                        layers.extend(ring(window_rect, accent, 6.));
                    }
                    for kind in PanelKind::ALL {
                        layers.extend(lit_bar(kind));
                        if let Some(rect) = bar_box(kind) {
                            layers.extend(ring(rect, accent, rect.height.min(rect.width) / 2.));
                        }
                    }
                    if let Some(rect) = island_rect {
                        layers.push(lit_island(rect));
                    }
                    layers.push(tag("Appearance", window_rect, screen, accent));
                }
                _ => {}
            }
            container(stack(layers).width(screen[0]).height(screen[1]).clip(true))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into()
        })
        .width(Length::Fill)
        .height(height)
        .into()
    }

    /// Under the picture: the chosen part's icon, name, how it is set now,
    /// and Open.
    fn home_detail(&self, selected: Option<Page>) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let Some(page) = selected else {
            return container(label(
                "Pick a part of the picture, or a tile below.",
                CAPTION,
                MUTED,
            ))
            .id(cosmic::widget::Id::new("home-detail"))
            .padding([12, 14])
            .height(60)
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .class(cosmic::theme::Container::Card)
            .into();
        };
        let badge = container(icon::from_name(page.nav_icon()).size(18).icon())
            .width(34)
            .height(34)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    icon_color: Some(accent),
                    background: Some(Background::Color(Color { a: 0.16, ..accent })),
                    border: Border {
                        radius: 9.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }));
        container(
            row::with_children(vec![
                badge.into(),
                column::with_children(vec![
                    label(page.nav_title(), 14., FG),
                    label(self.part_state(page), CAPTION, MUTED),
                ])
                .spacing(1)
                .width(Length::Fill)
                .into(),
                button::suggested("Open")
                    .id(cosmic::widget::Id::new("home-open"))
                    .name(format!("Open {}", page.nav_title()))
                    .on_press(Message::Page(page))
                    .into(),
            ])
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .id(cosmic::widget::Id::new("home-detail"))
        .padding([12, 14])
        .height(60)
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .class(cosmic::theme::Container::Card)
        .into()
    }

    /// One tile per sidebar part: name and a short state.
    fn home_tiles(&self, selected: Option<Page>) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let columns = ((self.content_width() + 8.) / 158.).floor().clamp(2., 5.) as usize;
        let tiles: Vec<Element<'_, Message>> = PARTS
            .iter()
            .map(|page| {
                let page = *page;
                let chosen = selected == Some(page);
                button::custom(
                    column::with_children(vec![
                        label(page.nav_title(), 13., FG),
                        cosmic::widget::text::caption(self.part_state(page))
                            .class(text_style(MUTED))
                            .wrapping(cosmic::iced::widget::text::Wrapping::None)
                            .into(),
                    ])
                    .spacing(2),
                )
                .id(cosmic::widget::Id::new(tile_id(page)))
                .name(page.nav_title())
                .selected(chosen)
                .class(surface(chosen, accent))
                .padding([9, 12])
                .width(Length::Fill)
                .on_press(Message::HomePart(page))
                .into()
            })
            .collect();
        let mut rows = Vec::new();
        let mut tiles = tiles.into_iter().peekable();
        while tiles.peek().is_some() {
            let mut line: Vec<Element<'_, Message>> = tiles.by_ref().take(columns).collect();
            while line.len() < columns {
                line.push(cosmic::widget::space().width(Length::Fill).into());
            }
            rows.push(
                cosmic::widget::row::with_children(line)
                    .spacing(8)
                    .width(Length::Fill)
                    .into(),
            );
        }
        container(column::with_children(rows).spacing(8))
            .id(cosmic::widget::Id::new("home-tiles"))
            .width(Length::Fill)
            .into()
    }

    /// How a part is set now, in a few words.
    pub(in crate::settings) fn part_state(&self, page: Page) -> String {
        let config = &self.config;
        let bar = |kind: PanelKind| -> String {
            let Some(draft) = self.desktop.draft(kind) else {
                return "Reading…".into();
            };
            if !draft.listed() {
                return "Off".into();
            }
            let edge = draft
                .anchor()
                .known()
                .map(|anchor| anchor.title())
                .unwrap_or("Custom");
            let shape = if draft.floating().known().copied().unwrap_or(false) {
                "floating"
            } else if draft.extend().known().copied().unwrap_or(true) {
                "edge to edge"
            } else {
                "centered"
            };
            format!("{edge} · {shape}")
        };
        match page {
            Page::DesktopWallpaper => match self.wallpaper.current() {
                Some(crate::desktop::wallpaper::BgSource::Color(_)) => "A solid color".into(),
                Some(source @ crate::desktop::wallpaper::BgSource::Path(path)) => self
                    .wallpaper
                    .tiles()
                    .iter()
                    .find(|tile| tile.source(self.wallpaper.bundled_dir()) == *source)
                    .map(|tile| tile.label.clone())
                    .or_else(|| {
                        path.file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                    })
                    .unwrap_or_else(|| "A picture".into()),
                None => "Your desktop picture".into(),
            },
            Page::WeatherEffects => match self.effects_on() {
                None => "Off".into(),
                Some(effect) if effect.weather => "Match the weather".into(),
                Some(effect) => {
                    use crate::rain::Effect;
                    match effect.effect {
                        Effect::Rain => "Rain",
                        Effect::Snow => "Snow",
                        Effect::Fog => "Fog",
                        Effect::Clouds => "Cloud shadows",
                    }
                    .into()
                }
            },
            Page::Overview => {
                let shown = Widget::ALL
                    .iter()
                    .filter(|widget| config.widget_visible(**widget))
                    .count();
                if !config.widgets_visible {
                    "Hidden".into()
                } else if shown == 1 {
                    "1 on the desktop".into()
                } else {
                    format!("{shown} on the desktop")
                }
            }
            Page::DesktopPanel => bar(PanelKind::Panel),
            Page::DesktopDock => bar(PanelKind::Dock),
            Page::ActivityIsland => {
                let on = self
                    .desktop
                    .draft(PanelKind::Panel)
                    .is_some_and(|draft| draft.listed() && has_island(draft));
                if !on {
                    "Not on the panel".into()
                } else if config.activity_island.style
                    == crate::activity_island::preferences::IslandStyle::Dark
                {
                    "On · dark".into()
                } else {
                    "On · follows the theme".into()
                }
            }
            Page::Themes => match self.system_appearance.draft().map(|d| d.mode()) {
                Some(crate::desktop::panels::Setting::Known(mode)) => match mode {
                    crate::desktop::appearance::SystemMode::Light => "Light style".into(),
                    crate::desktop::appearance::SystemMode::Dark => "Dark style".into(),
                    crate::desktop::appearance::SystemMode::Automatic => {
                        "Light by day, dark at night".into()
                    }
                },
                _ => "Colors, glass and fonts".into(),
            },
            Page::DesktopWindows => match self
                .windows
                .draft()
                .map(|draft| draft.value(crate::desktop::windows::Behavior::Autotile))
            {
                Some(crate::desktop::panels::Setting::Known(crate::desktop::windows::Choice::Flag(
                    true,
                ))) => "Tiling on".into(),
                Some(crate::desktop::panels::Setting::Known(_)) => "Floating windows".into(),
                _ => "Tiling, focus and gaps".into(),
            },
            Page::DesktopMotion => match self.motion.shown() {
                Some(true) => "Full motion".into(),
                Some(false) => "Reduced motion".into(),
                None => "Motion and readability".into(),
            },
            Page::General => format!("Version {}", env!("CARGO_PKG_VERSION")),
            _ => String::new(),
        }
    }
}
