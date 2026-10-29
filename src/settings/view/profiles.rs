//! Setup studio: compare the captured desktop with a local candidate, review
//! the planner’s included domains, and manage the portable setup library.
//! Scene objects only navigate; Apply and Cancel use the existing editor.
use super::desktop::{ERROR, WARN, banner, line};
use super::pages::section;
use super::system_appearance::{color, fill};
use super::*;
use crate::desktop::appearance::Preview;
use crate::desktop::profiles::{Domain, Preset, ProfilePlan};
use crate::drawer::Widget;
use crate::settings::profiles::{Message as Edit, NameAction, Selection, name_id};
use cosmic::iced::Point;
use cosmic::iced::widget::{pin, stack};
use cosmic::widget::responsive;

mod discovery;
/// Reasons shown before the rest is counted.
mod mix;
mod packages;

const SHORT: usize = 3;

fn edit(message: Edit) -> Message {
    Message::Profiles(message)
}

/// A selectable tile: a quiet outline, or the accent when selected.
pub(super) fn tile_style(selected: bool, _accent: Color) -> cosmic::theme::Button {
    let style = move |theme: &cosmic::Theme, focused: bool, hovered: bool| {
        let component = &theme.current_container().component;
        cosmic::widget::button::Style {
            background: Some(Background::Color(if hovered {
                component.hover.into()
            } else {
                component.base.into()
            })),
            border_color: if selected {
                theme.cosmic().accent.base.into()
            } else {
                component.divider.into()
            },
            border_width: if selected { 2. } else { 1. },
            border_radius: theme.cosmic().corner_radii.radius_s.into(),
            outline_width: if focused { 2. } else { 0. },
            outline_color: theme.cosmic().accent.base.into(),
            ..Default::default()
        }
    };
    cosmic::theme::Button::Custom {
        active: Box::new(move |focused, theme| style(theme, focused, false)),
        hovered: Box::new(move |focused, theme| style(theme, focused, true)),
        pressed: Box::new(move |focused, theme| style(theme, focused, true)),
        disabled: Box::new(move |theme| style(theme, false, false)),
    }
}

/// The bundled wallpaper a starter look shows, by palette name.
fn bundled_name(profile: &crate::desktop::profiles::Profile) -> Option<&str> {
    match profile
        .wallpaper
        .as_ref()?
        .background
        .as_ref()?
        .source
        .as_ref()?
    {
        crate::desktop::profiles::SourceLook::Bundled(name) => Some(name),
        _ => None,
    }
}
mod images;
#[cfg(test)]
pub(in crate::settings) use images::prepare as prepare_look_images;
pub(in crate::settings) use images::{load as load_look_images, ready as look_images_ready};
pub(super) fn look_image(id: &str, wallpaper: bool) -> Option<cosmic::widget::image::Handle> {
    images::look(id, wallpaper)
}
fn bundled_handle(name: &str) -> Option<cosmic::widget::image::Handle> {
    images::bundled(name)
}
fn swatch<'a>(color: Color) -> Element<'a, Message> {
    container(cosmic::iced::widget::space().width(12).height(12))
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(color)),
                border: Border {
                    radius: 6.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

/// Pure preset projection for the chooser. Sparse records retain unmentioned settings.
fn preset_config(config: &Config, preset: &Preset) -> Config {
    fn merge(base: &mut serde_json::Value, patch: &serde_json::Value) {
        if let (Some(base), Some(patch)) = (base.as_object_mut(), patch.as_object()) {
            for (key, value) in patch {
                merge(
                    base.entry(key.clone()).or_insert(serde_json::Value::Null),
                    value,
                );
            }
        } else {
            *base = patch.clone();
        }
    }
    let mut value = serde_json::to_value(config).expect("config serializes");
    for patch in [&preset.profile.widget_style, &preset.profile.widget_layout]
        .into_iter()
        .flatten()
    {
        merge(&mut value, patch);
    }
    let mut result: Config =
        serde_json::from_value(value).expect("built-in preset config is valid");
    result.desktop_palette = config.desktop_palette;
    result
}

/// Recover the comparison baseline from the same observation as the candidate.
/// Reversing only planned fields keeps system defaults and unrelated values.
/// This is a view projection, never an Undo or a write to the desktop.
pub(in crate::settings) fn comparison_current(
    candidate: &crate::settings::profiles::Preview,
    plan: Option<&ProfilePlan>,
) -> crate::settings::profiles::Preview {
    let Some(plan) = plan else {
        return candidate.clone();
    };
    comparison_from_patches(candidate, plan.patch(), plan.widget_patch())
}

fn comparison_from_patches(
    candidate: &crate::settings::profiles::Preview,
    native: &crate::desktop::store::DesktopPatch,
    widgets: &crate::desktop::store::widget::WidgetPatch,
) -> crate::settings::profiles::Preview {
    use crate::desktop::store::widget;
    let mut current = candidate.clone();
    current.bars = current
        .bars
        .into_iter()
        .map(|bar| {
            crate::desktop::panels::PanelDraft::from_snapshot(
                bar.kind(),
                original_snapshot(bar.into_snapshot(), native),
            )
        })
        .collect();
    current.appearance = current.appearance.map(|appearance| {
        crate::desktop::appearance::AppearanceDraft::from_snapshot(original_snapshot(
            appearance.into_snapshot(),
            native,
        ))
    });
    current.config = current.config.and_then(|config| {
        let mut value = serde_json::to_value(&config).ok()?;
        for (pointer, entry) in widgets.changes() {
            widget::put(&mut value, pointer, entry.original.clone()).ok()?;
        }
        let mut restored: Config = serde_json::from_value(value).ok()?;
        restored.desktop_palette = config.desktop_palette;
        Some(restored)
    });
    current
}

fn original_snapshot(
    mut snapshot: crate::desktop::store::Snapshot,
    native: &crate::desktop::store::DesktopPatch,
) -> crate::desktop::store::Snapshot {
    for (key, entry) in native.iter() {
        if let Some(observed) = snapshot.entry(key).cloned() {
            snapshot.insert(
                key.clone(),
                crate::desktop::store::KeyEntry {
                    user: entry.original.clone(),
                    system: observed.system,
                },
            );
        }
    }
    snapshot
}

/// A new candidate may remove custom sizes from the currently shown layout.
/// Start from the cached natural sizes so removal means automatic again.
pub(in crate::settings) fn scene_geometry(
    mut snapshot: crate::layout_state::Snapshot,
    config: &Config,
) -> crate::layout_state::Snapshot {
    for widget in &mut snapshot.widgets {
        if let Some(natural) = widget.natural_size {
            widget.size = natural;
        }
    }
    crate::layout_state::project(snapshot, config)
}

fn domain_hint(domain: Domain) -> &'static str {
    match domain {
        Domain::Appearance => "Native colors, fonts and window appearance",
        Domain::Wallpaper => "Backgrounds and any saved local display arrangement",
        Domain::Dock => "Dock appearance, behavior and saved contents",
        Domain::Panel => "Panel appearance, behavior and saved contents",
        Domain::Windows => "Tiling, focus and window controls",
        Domain::Workspaces => "Workspace behavior",
        Domain::DesktopIcons => "Desktop icon preferences",
        Domain::WidgetStyle => "Widget colors, typography and surface styling",
        Domain::WidgetLayout => "Replaces visible widgets and saved positions; your content stays",
    }
}

/// One reason per line (they are sentences), the first few and a count of
/// the rest, which the details list in full.
fn reasons(lines: &[String]) -> String {
    let mut text = lines[..lines.len().min(SHORT)].join("\n");
    if lines.len() > SHORT {
        text.push_str(&format!("\n+{} more in the details", lines.len() - SHORT));
    }
    text
}

/// One widget card on the miniature: the card palette and style, a sample
/// title and placeholder lines; never the user's own data.
pub(in crate::settings) fn card<'a>(
    widget: Widget,
    size: [f32; 2],
    config: &Config,
    scale: f32,
) -> Element<'a, Message> {
    card_at(widget, size, config, scale, None)
}

pub(in crate::settings) fn card_at<'a>(
    widget: Widget,
    size: [f32; 2],
    config: &Config,
    scale: f32,
    now: Option<chrono::DateTime<chrono::Local>>,
) -> Element<'a, Message> {
    if matches!(widget, Widget::Calendar | Widget::Devbox)
        && let Some(recipe) =
            crate::recipes::Kind::from_widget(widget).and_then(|kind| config.recipes.get(kind))
    {
        let scale = if scale.is_finite() && scale > 0. {
            scale
        } else {
            1.
        };
        let source = [(size[0] / scale).max(1.), (size[1] / scale).max(1.)];
        return super::sample::thumbnail(
            crate::app::settings_recipe_preview(config, recipe, false, source)
                .map(|_| Message::Preview),
            source,
            size,
        );
    }
    if matches!(widget, Widget::Clock | Widget::Weather) {
        let scale = if scale.is_finite() && scale > 0. {
            scale
        } else {
            1.
        };
        let source = crate::widget_presentation::viewport(
            [size[0] / scale, size[1] / scale],
            crate::widget_presentation::metrics(widget).natural,
        );
        let preview = match (widget, now) {
            (Widget::Clock, Some(now)) => {
                crate::app::settings_clock_preview_at(config, source, now.naive_local())
            }
            (Widget::Clock, None) => crate::app::settings_clock_preview(config, source),
            (_, Some(now)) => crate::app::settings_weather_preview_at(config, source, now),
            (_, None) => crate::app::settings_weather_preview_sized(config, source),
        }
        .map(|_| Message::Preview);
        return super::sample::thumbnail(preview, source, size);
    }
    // At a glance can keep its own background under a widget style.
    let transparent = if widget == Widget::Glance {
        !config.glance_panel()
    } else {
        widget == Widget::Clock || config.transparent_widgets()
    };
    let palette = if transparent {
        config.wallpaper_palette()
    } else {
        config.palette()
    };
    let style = &config.appearance;
    // The style's border width and shadow, scaled to the miniature.
    let edge = {
        let _style = crate::widget_style::scope(config);
        crate::widget_style::edge(Palette {
            background: if transparent {
                Color::TRANSPARENT
            } else {
                palette.background
            },
            ..palette
        })
    };
    let background = if transparent {
        Color::TRANSPARENT
    } else if widget == Widget::Glance && config.appearance.family.is_none() {
        Color {
            a: if style.contrast_assist {
                palette
                    .background
                    .a
                    .max(config.glance.opacity as f32 / 100.)
            } else {
                config.glance.opacity as f32 / 100.
            },
            ..palette.background
        }
    } else {
        palette.background
    };
    let border = Color {
        a: if transparent {
            0.
        } else {
            f32::from(style.border_opacity.min(50)) / 100.
        },
        ..palette.accent
    };
    let radius = (f32::from(style.corner_radius) * scale)
        .max(0.)
        .min((size[1] / 3.).max(0.));
    let (title, detail) = match widget {
        Widget::Clock => ("09:41", "Monday, 19 October"),
        Widget::Glance if config.glance.show_clock => (
            "09:41",
            if config.glance.show_weather {
                "21° · Partly cloudy"
            } else if config.glance.show_calendar {
                "10:00 Design review"
            } else if config.clock.show_date {
                "Monday, 19 October"
            } else {
                ""
            },
        ),
        Widget::Glance if config.glance.show_weather => ("21°", "Partly cloudy"),
        Widget::Glance => ("Today", "10:00 Design review"),
        Widget::Weather => ("21°", "Partly cloudy"),
        Widget::Calendar => ("Today", "10:00  Design review"),
        Widget::Devbox => ("CPU 26%", "RAM 8.2 / 32 GiB"),
        Widget::Projects => ("Atlas", "main · 2 changes"),
        Widget::Music => ("Now playing", "Artist · Album"),
        Widget::Focus => ("25:00", "Focus"),
        Widget::Notes => ("Notes", "Plan the next release"),
    };
    let main_size = if matches!(widget, Widget::Clock | Widget::Glance | Widget::Weather) {
        (52. * scale).clamp(4., 32.)
    } else {
        (34. * scale).clamp(4., 18.)
    };
    let secondary_size = (26. * scale).clamp(3.8, 14.);
    let pad = (18. * scale).clamp(1., 12.);
    let font = config
        .theme_roles()
        .map_or(cosmic::iced::Font::DEFAULT, |r| {
            if widget == Widget::Clock || widget == Widget::Glance {
                r.clock
            } else {
                r.body
            }
        });
    let gap = (6. * scale).max(1.);
    let mut used = main_size * 1.25;
    let available = (size[1] - pad * 2.).max(0.);
    let mut content = vec![
        text::body(title)
            .size(main_size)
            .line_height(1.25)
            .font(font)
            .class(cosmic::theme::Text::Color(palette.foreground))
            .into(),
    ];
    if available >= used + gap + secondary_size * 1.25 {
        content.push(line(detail, secondary_size, palette.muted));
        used += gap + secondary_size * 1.25;
    }
    // The large scene should read like a desktop, not a collection of empty
    // blocks. Keep illustrative content bounded and omit it from small tiles.
    if size[0] >= 130. && size[1] >= 110. {
        let extra: &[&str] = match widget {
            Widget::Calendar => &["14:30  Project planning", "16:00  Focus time"],
            Widget::Devbox => &[
                "Disk       166 / 512 GiB",
                "Network    2.4 MB/s",
                "Uptime     3h 21m",
            ],
            Widget::Projects => &["●  Development server", "Last edited today"],
            Widget::Notes => &["[ ] Review the details", "[ ] Make something good"],
            _ => &[],
        };
        let separator = (12. * scale).max(3.);
        let room = (available - used - gap - separator).max(0.);
        let count = (room / (gap + secondary_size * 1.25)).floor() as usize;
        if count > 0 && !extra.is_empty() {
            content.push(cosmic::widget::space().height(separator).into());
            for value in extra.iter().take(count) {
                content.push(line(*value, secondary_size, palette.muted));
            }
        }
    }
    container(column::with_children(content).spacing(gap))
        .width(size[0].max(1.))
        .height(size[1].max(1.))
        .padding(pad)
        .clip(true)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(background)),
                border: Border {
                    color: border,
                    width: if transparent {
                        1.
                    } else {
                        (edge.0 * scale * 2.).max(1.)
                    },
                    radius: radius.into(),
                },
                shadow: cosmic::iced::Shadow {
                    blur_radius: edge.1.blur_radius * scale,
                    offset: edge.1.offset * scale,
                    ..edge.1
                },
                ..Default::default()
            }
        }))
        .into()
}

/// The selected theme as a small window without words: its surfaces,
/// accent, corners, outline and title dots, with an accent pill and quiet
/// lines where the text would be. Sized for the miniature.
pub(super) fn schematic_window<'a>(preview: Preview, size: [f32; 2]) -> Element<'a, Message> {
    let [width, height] = size;
    let pad = (height / 10.).clamp(4., 8.);
    let gap = (height / 14.).clamp(3., 6.);
    let inset = (height / 16.).clamp(3., 6.);
    let dot = (height / 14.).clamp(4., 8.);
    let bar = (height / 26.).clamp(2., 4.);
    let pill = (height / 9.).clamp(5., 10.);
    let radius_s = preview.radius_s.min(pill / 2.);
    let faint = |tone, alpha: f32| {
        let tone = color(tone);
        Color {
            a: tone.a * alpha,
            ..tone
        }
    };
    let block = |width: f32, height: f32, tone: Color, radius: f32| -> Element<'a, Message> {
        fill(
            cosmic::iced::widget::space()
                .width(width.max(1.))
                .height(height)
                .into(),
            tone,
            Color::TRANSPARENT,
            0.,
            radius,
        )
        .into()
    };
    let title_bar = row::with_children(vec![
        if width >= 170. {
            label("Files", 11., color(preview.on_background))
        } else {
            block(
                width * 0.22,
                bar,
                faint(preview.on_background, 0.45),
                bar / 2.,
            )
        },
        cosmic::iced::widget::space().width(Length::Fill).into(),
        block(dot, dot, faint(preview.on_background, 0.5), dot / 2.),
        block(dot, dot, faint(preview.on_background, 0.5), dot / 2.),
        block(dot, dot, color(preview.accent), dot / 2.),
    ])
    .spacing(dot * 0.6)
    .align_y(Alignment::Center);
    let inner = (width - 2. * pad - gap).max(2.);
    let side = inner * 0.4;
    let main = inner - side;
    let line_in = |panel: f32, fraction: f32| (panel - 2. * inset) * fraction;
    let sidebar = fill(
        column::with_children(vec![
            if width >= 170. {
                fill(
                    label("Home", 10., color(preview.on_accent)),
                    color(preview.accent),
                    Color::TRANSPARENT,
                    0.,
                    radius_s,
                )
                .padding([3, 5])
                .width(Length::Fill)
                .into()
            } else {
                block(line_in(side, 1.), pill, color(preview.accent), radius_s)
            },
            if width >= 170. {
                column::with_children(vec![
                    line("Documents", 10., color(preview.on_primary)),
                    line("Pictures", 10., color(preview.on_primary)),
                    line("Downloads", 10., color(preview.on_primary)),
                ])
                .spacing(6)
                .into()
            } else {
                block(
                    line_in(side, 0.65),
                    bar,
                    faint(preview.on_primary, 0.4),
                    bar / 2.,
                )
            },
        ])
        .spacing(gap)
        .into(),
        color(preview.primary),
        Color::TRANSPARENT,
        0.,
        radius_s,
    )
    .padding(inset)
    .width(side)
    .height(Length::Fill);
    let card = fill(
        column::with_children(vec![
            if width >= 170. {
                column::with_children(vec![
                    line("Recent files", 11., color(preview.on_secondary)),
                    cosmic::widget::space().height(4).into(),
                    line("Design notes", 10., color(preview.on_secondary)),
                    line("Ideas.txt", 10., faint(preview.on_secondary, 0.75)),
                    line("Screenshots", 10., faint(preview.on_secondary, 0.75)),
                ])
                .spacing(6)
                .into()
            } else {
                block(
                    line_in(main, 0.8),
                    bar,
                    faint(preview.on_secondary, 0.6),
                    bar / 2.,
                )
            },
            block(
                line_in(main, 0.55),
                bar,
                faint(preview.on_secondary, 0.35),
                bar / 2.,
            ),
        ])
        .spacing(gap)
        .into(),
        color(preview.secondary),
        Color::TRANSPARENT,
        0.,
        radius_s,
    )
    .padding(inset)
    .width(main)
    .height(Length::Fill);
    fill(
        column::with_children(vec![
            title_bar.into(),
            row::with_children(vec![sidebar.into(), card.into()])
                .spacing(gap)
                .height(Length::Fill)
                .into(),
        ])
        .spacing(gap)
        .into(),
        color(preview.background),
        color(preview.outline),
        (preview.outline_width as f32).clamp(0., 3.),
        preview.radius_m.min(height / 6.),
    )
    .padding(pad)
    .width(width)
    .height(height)
    .clip(true)
    .into()
}

impl Settings {
    /// Fit the tiles to the actual content width, including the sidebar,
    /// padding, scrollbar allowance and gaps: four styles on Colors & theme,
    /// two spacious desktop compositions on Setups.
    fn tile_width(&self) -> f32 {
        let available = self.content_width() - 14.;
        let minimum = if self.page == Page::Themes {
            168.
        } else {
            248.
        };
        let count = ((available + 16.) / (minimum + 16.)).floor().max(1.);
        ((available - (count - 1.) * 16.) / count).floor()
    }

    /// The selection over the current desktop: the planned wallpaper (or a
    /// neutral backdrop), a sample window in the theme, the visible cards
    /// where they sit, and both bars.
    pub(super) fn profile_preview(&self, height: f32) -> Element<'_, Message> {
        if !self.scene_open() {
            return cosmic::widget::space().into();
        }
        let editor = &self.profiles;
        let Some(preview) = editor.preview() else {
            return label("Reading your desktop…", 12., MUTED);
        };
        self.profile_scene(
            preview.clone(),
            height,
            false,
            !matches!(self.page, Page::DesktopProfiles | Page::AdaptiveLooks),
        )
    }

    fn profile_scene(
        &self,
        preview: crate::settings::profiles::Preview,
        height: f32,
        current: bool,
        interactive: bool,
    ) -> Element<'_, Message> {
        let editor = &self.profiles;
        let mut config = preview
            .config
            .clone()
            .unwrap_or_else(|| self.config.clone());
        // Wallpaper polarity is derived state, not a style choice. Only
        // project a bundled candidate's known polarity when that wallpaper
        // is actually in the plan; excluded/missing backgrounds keep current
        // contrast. Runtime analysis owns the persisted value after Apply.
        if !current
            && editor.included(Domain::Wallpaper)
            && let Some(name) = editor.profile().and_then(bundled_name)
            && crate::desktop::wallpaper::bundled_name(name).is_some()
        {
            config.appearance.wallpaper_light = name == "halcyon-paper";
        }
        let accent = config.palette().accent;
        let mut bars = preview.bars.clone();
        if self.page == Page::DesktopProfiles {
            bars.retain(|bar| {
                bar.autohide().known() != Some(&crate::desktop::panels::AutoHide::Always)
            });
        }
        if let Some(appearance) = &preview.appearance {
            for bar in &mut bars {
                use crate::desktop::panels::Background as BarBackground;
                let variant = match bar.background().known() {
                    Some(BarBackground::Light) => crate::desktop::appearance::ThemeVariant::Light,
                    Some(BarBackground::Dark) => crate::desktop::appearance::ThemeVariant::Dark,
                    Some(BarBackground::ThemeDefault) => appearance.active_variant(),
                    _ => continue,
                };
                let c = appearance.preview(variant).background;
                bar.set_background(BarBackground::Color([c.red, c.green, c.blue]));
            }
        }
        let layout = scene_geometry(self.placement.preview_snapshot().clone(), &config);
        let screen_size = layout.screen;
        // An active COSMIC window shows the planned window colors and its
        // outline, which a theme now coordinates too.
        let show_window = self.page != Page::DesktopProfiles;
        let mut objects: Vec<Element<'_, Message>> = Vec::new();
        if interactive {
            objects.push(
                button::text("Wallpaper")
                    .id(cosmic::widget::Id::new("scene-open-wallpaper"))
                    .on_press(Message::Page(Page::DesktopWallpaper))
                    .into(),
            );
            objects.push(
                button::text("Window appearance")
                    .id(cosmic::widget::Id::new("scene-open-window"))
                    .on_press(Message::Page(Page::DesktopAppearance))
                    .into(),
            );
            for bar in &bars {
                if bar.listed() {
                    objects.push(
                        button::text(bar.kind().title())
                            .on_press(Message::OpenBar(bar.kind()))
                            .into(),
                    );
                }
            }
            for geometry in &layout.widgets {
                if let Some(widget) =
                    Widget::parse(&geometry.key).filter(|w| config.widget_visible(*w))
                {
                    objects.push(
                        button::text(widget.title())
                            .on_press(Message::Page(super::home::widget_page(widget)))
                            .into(),
                    );
                }
            }
        }
        let scene = responsive(move |available| {
            let scale = (available.width.max(1.) / screen_size[0].max(1.))
                .min(available.height.max(1.) / screen_size[1].max(1.));
            let screen = [screen_size[0] * scale, screen_size[1] * scale];
            let wallpaper = if current {
                editor.home_image()
            } else {
                editor.backdrop_image().or_else(|| {
                    // A theme must wait for its analyzed image/version. Home
                    // may still hold an older picture at the very same path.
                    if editor.profile().is_some_and(|p| p.appearance_only) {
                        None
                    } else {
                        editor.home_image()
                    }
                })
            }
            .map(|handle| {
                cosmic::widget::image(handle.clone())
                    .width(screen[0])
                    .height(screen[1])
                    .content_fit(cosmic::iced::ContentFit::Cover)
                    .border_radius(9.)
                    .into()
            });
            let backdrop = container(text::body(""))
                .width(screen[0])
                .height(screen[1])
                .class(cosmic::theme::Container::custom(|theme| {
                    cosmic::iced::widget::container::Style {
                        background: Some(Background::Gradient(
                            cosmic::iced::gradient::Linear::new(2.4)
                                .add_stop(0., theme.cosmic().primary(theme.transparent).base.into())
                                .add_stop(
                                    1.,
                                    theme.cosmic().background(theme.transparent).base.into(),
                                )
                                .into(),
                        )),
                        border: Border {
                            color: theme.current_container().component.divider.into(),
                            width: 1.,
                            radius: 9.into(),
                        },
                        ..Default::default()
                    }
                }));
            let background = wallpaper.unwrap_or_else(|| backdrop.into());
            let background = if interactive {
                button::custom(background)
                    .class(super::home::hotspot(accent, 9.))
                    .padding(0)
                    .width(screen[0])
                    .height(screen[1])
                    .on_press(Message::Page(Page::DesktopWallpaper))
                    .into()
            } else {
                background
            };
            let mut layers: Vec<Element<'_, Message>> = vec![
                cosmic::widget::space()
                    .width(screen[0])
                    .height(screen[1])
                    .into(),
                background,
            ];
            for geometry in &layout.widgets {
                let Some(widget) = Widget::parse(&geometry.key) else {
                    continue;
                };
                if !config.widget_visible(widget) {
                    continue;
                }
                let size = [geometry.size[0] * scale, geometry.size[1] * scale];
                let sample = if widget == Widget::Clock {
                    self.depth_preview
                        .image(current)
                        .map(|handle| {
                            cosmic::widget::image(handle.clone())
                                .width(size[0])
                                .height(size[1])
                                .content_fit(cosmic::iced::ContentFit::Fill)
                                .into()
                        })
                        .unwrap_or_else(|| card(widget, size, &config, scale))
                } else {
                    card(widget, size, &config, scale)
                };
                let sample = if interactive {
                    button::custom(sample)
                        .class(super::home::hotspot(accent, 6.))
                        .padding(0)
                        .on_press(Message::Page(super::home::widget_page(widget)))
                        .into()
                } else {
                    sample
                };
                layers.push(
                    pin(sample)
                        .position(Point::new(
                            geometry.position[0] * scale,
                            geometry.position[1] * scale,
                        ))
                        .into(),
                );
            }
            // Windows sit above the cards, as on the desktop; centered,
            // where cards rarely are.
            if let Some(draft) = preview.appearance.as_ref().filter(|_| show_window) {
                let size = [screen[0] * 0.36, (screen[1] * 0.34).max(64.)];
                let window = schematic_window(draft.preview(draft.active_variant()), size);
                let window = if interactive {
                    button::custom(window)
                        .class(super::home::hotspot(accent, 6.))
                        .padding(0)
                        .on_press(Message::Page(Page::DesktopAppearance))
                        .into()
                } else {
                    window
                };
                layers.push(
                    pin(window)
                        .position(Point::new(
                            (screen[0] - size[0]) / 2.,
                            (screen[1] - size[1]) * 0.42,
                        ))
                        .into(),
                );
            }
            layers.extend(super::desktop::preview::bars_on_output(
                self.desktop.islands.installation.installed(),
                bars.iter().collect(),
                &editor.installed.applets,
                None,
                screen,
                scale,
                accent,
                interactive.then_some(Message::OpenBar as fn(_) -> Message),
                self.placement.display.as_deref().unwrap_or(""),
            ));
            container(stack(layers).width(screen[0]).height(screen[1]).clip(true))
                .id(cosmic::widget::Id::new(if current {
                    "profile-scene-current"
                } else {
                    "profile-scene-candidate"
                }))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into()
        })
        .width(Length::Fill)
        .height(height);
        if interactive {
            let mut content = vec![scene.into()];
            content.push(
                button::text(if self.scene_objects_open {
                    "Hide shortcuts"
                } else {
                    "Customize an element…"
                })
                .id(cosmic::widget::Id::new("scene-objects-toggle"))
                .leading_icon(icon::from_name("document-edit-symbolic").size(16))
                .on_press(Message::ToggleSceneObjects)
                .into(),
            );
            if self.scene_objects_open {
                content.push(row::with_children(objects).spacing(6).wrap().into());
            }
            column::with_children(content).spacing(8).into()
        } else {
            scene.into()
        }
    }

    /// A starter look's or style's accent, as its tile shows it.
    pub(super) fn preset_color(&self, preset: &Preset) -> Color {
        self.tile_config(preset).palette().accent
    }
    fn tile_config(&self, preset: &Preset) -> Config {
        let mut config = preset_config(&self.config, preset);
        // A catalogue specimen describes the recipe even when the live desktop is hidden.
        config.widgets_visible = true;
        // Family specimens stay legible while the actual selection's backdrop
        // is loading or changing. The larger preview uses the planned desktop.
        if let Some(family) = crate::desktop::profiles::themes::family(&preset.profile) {
            config.appearance.wallpaper_light = family == crate::desktop_theme::Family::Paper;
        }
        config
    }

    /// A miniature desktop: the look's wallpaper (or style colors) with its
    /// widget cards where the layout puts them.
    pub(crate) fn preset_thumbnail<'a>(
        &'a self,
        preset: &'a Preset,
        height: f32,
    ) -> Element<'a, Message> {
        // Render the actual recipe. The old baked desktop images all used
        // one composition, so they cannot represent purpose-led setups.
        let starter =
            !preset.profile.appearance_only && preset.id != crate::desktop::profiles::CLEAN;
        let installed = self.desktop.islands.installation.installed();
        let panel = self.setup_panel_baseline().and_then(|applied| {
            let look = preset.profile.panel.as_ref()?;
            let mut specimen =
                crate::desktop::profiles::preview_panel_style(applied, look, installed);
            if let Some(mut islands) = specimen.islands().known().cloned() {
                // A catalogue card illustrates the portable composition. The
                // selected review uses this display's actual retained override.
                islands.outputs.clear();
                specimen.set_islands(islands);
            }
            Some(specimen)
        });
        let has_islands = installed
            && panel
                .as_ref()
                .and_then(|p| p.islands().known().cloned())
                .is_some_and(|s| s.enabled);
        let config = self.tile_config(preset);
        let family = crate::desktop::profiles::themes::family(&preset.profile);
        let mut specimen = self.placement.preview_snapshot().clone();
        // Catalogue compositions use a consistent canvas. Review still uses
        // the actual display and its effective geometry.
        specimen.screen = [1920., 1080.];
        specimen.display = None;
        specimen.widget_displays.clear();
        let geometry = scene_geometry(specimen, &config);
        // A starter look shows its own wallpaper; a style shows its colors.
        let wallpaper = look_image(preset.id, true)
            .or_else(|| bundled_name(&preset.profile).and_then(bundled_handle))
            .or_else(|| {
                family
                    .is_none()
                    .then(|| self.profiles.home_image().cloned())
                    .flatten()
            });
        responsive(move |bounds| {
            let scale = (bounds.width / geometry.screen[0]).min(bounds.height / geometry.screen[1]);
            let screen = [geometry.screen[0] * scale, geometry.screen[1] * scale];
            let background: Element<'_, Message> = match &wallpaper {
                Some(handle) => cosmic::widget::image(handle.clone())
                    .width(screen[0])
                    .height(screen[1])
                    .content_fit(cosmic::iced::ContentFit::Cover)
                    .border_radius(6.)
                    .into(),
                None if family.is_some() => {
                    let (a, b) = family.unwrap().wallpaper();
                    container(cosmic::iced::widget::space())
                        .class(cosmic::theme::Container::custom(move |_| {
                            cosmic::iced::widget::container::Style {
                                background: Some(Background::Gradient(
                                    cosmic::iced::gradient::Linear::new(2.4)
                                        .add_stop(0., Color::from_rgb8(a[0], a[1], a[2]))
                                        .add_stop(1., Color::from_rgb8(b[0], b[1], b[2]))
                                        .into(),
                                )),
                                border: Border {
                                    radius: 6.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }
                        }))
                        .width(screen[0])
                        .height(screen[1])
                        .into()
                }
                None => fill(
                    cosmic::iced::widget::space().into(),
                    Color::from_rgb(0.12, 0.13, 0.15),
                    Color::TRANSPARENT,
                    0.,
                    6.,
                )
                .width(screen[0])
                .height(screen[1])
                .into(),
            };
            let mut layers = vec![background];
            if starter {
                let bar_height = (screen[1] * 0.04).max(6.);
                let light = config.appearance.wallpaper_light;
                let bar_fill = if light {
                    Color::from_rgba(0.98, 0.95, 0.89, 0.94)
                } else {
                    Color::from_rgba(0.08, 0.09, 0.11, 0.92)
                };
                if has_islands && let Some(draft) = &panel {
                    let mut draft = draft.clone();
                    draft.set_background(crate::desktop::panels::Background::Color([
                        bar_fill.r, bar_fill.g, bar_fill.b,
                    ]));
                    let rendered = super::desktop::preview::bars(
                        installed,
                        vec![&draft],
                        &self.profiles.installed.applets,
                        None,
                        screen,
                        scale,
                        config.palette().accent,
                        None,
                    );
                    layers.push(
                        container(stack(rendered).width(screen[0]).height(screen[1]))
                            .id(cosmic::widget::Id::new("starter-islands-panel"))
                            .into(),
                    );
                } else {
                    layers.push(
                        pin(container(
                            fill(
                                cosmic::widget::space().into(),
                                bar_fill,
                                Color::TRANSPARENT,
                                0.,
                                0.,
                            )
                            .width(screen[0])
                            .height(bar_height),
                        )
                        .id(cosmic::widget::Id::new("starter-standard-panel")))
                        .position(Point::ORIGIN)
                        .into(),
                    );
                }
                if !matches!(
                    preset.profile.dock.as_ref().and_then(|d| d.autohide),
                    Some(crate::desktop::panels::AutoHide::Always)
                ) {
                    layers.push(
                        pin(fill(
                            cosmic::widget::space().into(),
                            bar_fill,
                            Color::TRANSPARENT,
                            0.,
                            3.,
                        )
                        .width(screen[0] * 0.3)
                        .height(bar_height))
                        .position(Point::new(screen[0] * 0.35, screen[1] - bar_height - 3.))
                        .into(),
                    );
                }
            }

            for geometry in &geometry.widgets {
                if let Some(widget) =
                    Widget::parse(&geometry.key).filter(|widget| config.widget_visible(*widget))
                {
                    layers.push(
                        pin(card(
                            widget,
                            [geometry.size[0] * scale, geometry.size[1] * scale],
                            &config,
                            scale,
                        ))
                        .position(Point::new(
                            geometry.position[0] * scale,
                            geometry.position[1] * scale,
                        ))
                        .into(),
                    );
                }
            }
            container(stack(layers).width(screen[0]).height(screen[1]).clip(true))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into()
        })
        .height(height)
        .into()
    }

    pub(super) fn preset_tile<'a>(&'a self, preset: &'a Preset) -> Element<'a, Message> {
        let selected = *self.profiles.selection() == Selection::Preset(preset.id);
        let color = self.preset_color(preset);
        let thumbnail = self.preset_thumbnail(
            preset,
            if self.page == Page::Themes {
                76.
            } else {
                ((self.tile_width() - 24.) * 9. / 16.).round()
            },
        );
        let mut parts = vec![
            thumbnail.into(),
            row::with_children(vec![
                container(line(
                    preset.profile.name.clone(),
                    if self.page == Page::Themes { 15. } else { 20. },
                    FG,
                ))
                .width(Length::Fill)
                .into(),
                label(if selected { "✓" } else { "" }, 12., self.settings_accent()),
                swatch(color),
            ])
            .spacing(6)
            .align_y(Alignment::Center)
            .into(),
        ];
        if self.page != Page::Themes {
            parts.push(label(
                crate::desktop::profiles::occasion(preset.id),
                13.,
                MUTED,
            ));
            parts.push(label(self.setup_tools(preset), 12., MUTED));
            parts.push(
                row::with_children(vec![
                    container(cosmic::widget::space())
                        .width(Length::Fill)
                        .into(),
                    label("Explore setup", 12., self.settings_accent()),
                    icon::from_name("go-next-symbolic").size(14).into(),
                ])
                .spacing(6)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        button::custom(column::with_children(parts).spacing(10))
            .id(cosmic::widget::Id::new(format!("preset-{}", preset.id)))
            .class(tile_style(selected, self.settings_accent()))
            .selected(selected)
            .padding(if self.page == Page::Themes { 8 } else { 12 })
            .width(self.tile_width())
            .on_press(edit(Edit::Select(Selection::Preset(preset.id))))
            .into()
    }

    /// Describes the fitted composition, including unavailable Projects.
    fn setup_tools(&self, preset: &Preset) -> String {
        let mut config = self.tile_config(preset);
        config.widgets_visible = true;
        Widget::ALL
            .into_iter()
            .filter(|w| config.widget_visible(*w))
            .map(|w| w.title())
            .collect::<Vec<_>>()
            .join(" · ")
    }

    pub(super) fn name_field(&self, save: &'static str) -> Element<'_, Message> {
        let text = self
            .profiles
            .name
            .as_ref()
            .map(|name| name.text.clone())
            .unwrap_or_default();
        row::with_children(vec![
            text_input(
                if self.page == Page::Themes {
                    "Theme name"
                } else {
                    "Setup name"
                },
                text,
            )
            .id(name_id())
            .on_input(|value| edit(Edit::NameInput(value)))
            .on_submit(|_| edit(Edit::NameSubmit))
            .width(Length::Fill)
            .into(),
            button::text("Cancel")
                .on_press(edit(Edit::Name(None)))
                .into(),
            button::suggested(save)
                .on_press(edit(Edit::NameSubmit))
                .into(),
        ])
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }

    pub(super) fn library_section(&self) -> Element<'_, Message> {
        let editor = &self.profiles;
        let accent = self.settings_accent();
        let writable = editor.writable();
        let empty_setup = self.page != Page::Themes
            && editor.library().is_ok_and(|entries| {
                !entries
                    .iter()
                    .any(|entry| entry.profile.as_ref().map_or(true, |p| !p.appearance_only))
            });
        let mut items: Vec<Element<'_, Message>> = vec![split(
            label(
                if self.page == Page::Themes {
                    "Your themes"
                } else {
                    "Your setups"
                },
                15.,
                FG,
            ),
            row::with_children(vec![
                button::text("Import…")
                    .leading_icon(icon::from_name("document-open-symbolic").size(14))
                    .on_press_maybe((writable && !editor.busy()).then_some(edit(
                        if self.page == Page::Themes {
                            Edit::ImportTheme
                        } else {
                            Edit::Import
                        },
                    )))
                    .into(),
                button::text(if self.page == Page::Themes {
                    "Save theme…"
                } else {
                    "Save setup…"
                })
                .leading_icon(icon::from_name("list-add-symbolic").size(14))
                .on_press_maybe(writable.then_some(edit(Edit::Name(Some(
                    if self.page == Page::Themes {
                        NameAction::CaptureTheme
                    } else {
                        NameAction::Capture
                    },
                )))))
                .into(),
            ])
            .spacing(6)
            .into(),
        )];
        if empty_setup {
            items.clear();
            items.push(label("Your setups", 15., FG));
        }
        if editor.name.as_ref().is_some_and(|name| {
            matches!(name.action, NameAction::Capture | NameAction::CaptureTheme)
        }) {
            items.push(self.name_field("Save"));
        }
        match editor.library() {
            Err(error) => items.push(banner(
                format!("Saved setups can’t be read. {error}"),
                ERROR,
                Vec::new(),
            )),
            Ok(entries)
                if !entries.iter().any(|entry| {
                    entry
                        .profile
                        .as_ref()
                        .map_or(true, |p| p.appearance_only == (self.page == Page::Themes))
                }) =>
            {
                if empty_setup {
                    let thumbnail = editor
                        .preview()
                        .map(|preview| self.profile_scene(preview.clone(), 126., true, false))
                        .unwrap_or_else(|| label("Your desktop", 14., MUTED));
                    items.push(
                        row::with_children(vec![
                            container(thumbnail).width(220).into(),
                            container(
                                column::with_children(vec![
                                    label("Save your current desktop", 20., FG),
                                    label("Keep a setup you can apply again.", 14., MUTED),
                                    row::with_children(vec![
                                        button::suggested("Save setup…")
                                            .id(cosmic::widget::Id::new("setup-save-current"))
                                            .on_press_maybe((writable && !editor.busy()).then_some(
                                                edit(Edit::Name(Some(NameAction::Capture))),
                                            ))
                                            .into(),
                                        button::text("Import…")
                                            .id(cosmic::widget::Id::new("setup-import"))
                                            .on_press_maybe(
                                                (writable && !editor.busy())
                                                    .then_some(edit(Edit::Import)),
                                            )
                                            .into(),
                                    ])
                                    .spacing(8)
                                    .wrap()
                                    .into(),
                                    label("Current layout · sample content", 11., MUTED),
                                ])
                                .spacing(10),
                            )
                            .width(Length::Fill)
                            .into(),
                        ])
                        .spacing(20)
                        .align_y(Alignment::Center)
                        .wrap()
                        .into(),
                    );
                } else {
                    items.push(container(column::with_children(vec![
                        icon::from_name("document-save-symbolic").size(32).into(),
                        label("Keep a look you love", 20., FG),
                        label("Save your current settings, then come back to them whenever you like.", 14., MUTED),
                    ]).spacing(12)).padding([28, 16]).width(Length::Fill).into());
                }
            }
            Ok(entries) => {
                for entry in entries {
                    if entry
                        .profile
                        .as_ref()
                        .is_ok_and(|p| p.appearance_only != (self.page == Page::Themes))
                    {
                        continue;
                    }
                    let selected = *editor.selection() == Selection::Saved(entry.id.clone());
                    let (name, tone) = match &entry.profile {
                        Ok(profile) => (profile.name.clone(), if selected { FG } else { MUTED }),
                        Err(_) => ("Unreadable setup".to_string(), WARN),
                    };
                    let mut parts = vec![
                        container(
                            column::with_children(vec![
                                line(name, 16., tone),
                                label(
                                    entry
                                        .profile
                                        .as_ref()
                                        .map(|p| {
                                            p.domains()
                                                .iter()
                                                .map(|d| d.title())
                                                .collect::<Vec<_>>()
                                                .join(" · ")
                                        })
                                        .unwrap_or_else(|_| "Needs attention".into()),
                                    12.,
                                    MUTED,
                                ),
                            ])
                            .spacing(5),
                        )
                        .width(Length::Fill)
                        .into(),
                        icon::from_name("go-next-symbolic").size(16).into(),
                    ];
                    if entry.profile.is_err() {
                        parts.push(label(entry.id.as_str().to_string(), 10., MUTED));
                    }
                    items.push(
                        button::custom(
                            row::with_children(parts)
                                .spacing(10)
                                .align_y(Alignment::Center),
                        )
                        .class(nav_style(selected, accent))
                        .selected(selected)
                        .padding([14, 12])
                        .width(Length::Fill)
                        .on_press(edit(Edit::Select(Selection::Saved(entry.id.clone()))))
                        .into(),
                    );
                }
            }
        }
        section(items)
    }

    /// The selection: miniature, name and actions, parts to include, what
    /// blocks it, missing components and (on demand) every change.
    fn selection_scene(&self) -> Element<'_, Message> {
        let editor = &self.profiles;
        let accent = self.settings_accent();
        let height = self.stage_height();
        let mut content = Vec::new();
        let choices = row::with_children(vec![
            button::custom(label("Preview", 13., FG))
                .class(nav_style(!editor.compare_current, accent))
                .selected(!editor.compare_current)
                .padding([8, 14])
                .id(cosmic::widget::Id::new("setup-show-candidate"))
                .on_press(edit(Edit::Compare(false)))
                .into(),
            button::custom(label("Current", 13., FG))
                .class(nav_style(editor.compare_current, accent))
                .selected(editor.compare_current)
                .padding([8, 14])
                .id(cosmic::widget::Id::new("setup-show-current"))
                .on_press(edit(Edit::Compare(true)))
                .into(),
        ])
        .spacing(6)
        .into();
        content.push(split(
            choices,
            button::text(if self.scene_open() {
                "Hide preview"
            } else {
                "Show preview"
            })
            .id(cosmic::widget::Id::new("scene-toggle"))
            .on_press(Message::ToggleScene)
            .into(),
        ));
        let scene = if editor.compare_current {
            editor
                .preview()
                .map(|preview| {
                    self.profile_scene(
                        comparison_current(preview, editor.plan()),
                        height,
                        true,
                        false,
                    )
                })
                .unwrap_or_else(|| label("Reading your desktop…", 12., MUTED))
        } else {
            self.profile_preview(height)
        };
        if self.scene_open() {
            content.push(
                container(scene)
                    .id(cosmic::widget::Id::new("desktop-scene"))
                    .into(),
            );
            content.push(label(
                format!("Sample desktop · {}", self.setup_islands_status()),
                12.,
                MUTED,
            ));
        }
        column::with_children(content).spacing(12).into()
    }
    pub(super) fn selection_section(&self) -> Element<'_, Message> {
        let mut content = Vec::new();
        if let Some(profile) = self.profiles.profile() {
            let title = label(&profile.name, 26., FG);
            content.push(if let Some(description) = self.profiles.description() {
                split(
                    title,
                    super::help::explanation(
                        "setup-about",
                        "About this setup",
                        description,
                        self.help_open == Some("setup-about"),
                    ),
                )
            } else {
                title
            });
        }
        content.push(self.inspector_layout(
            self.selection_scene(),
            self.selection_details(),
            "setup",
        ));
        column::with_children(content).spacing(16).into()
    }
    fn selection_details(&self) -> Element<'_, Message> {
        let editor = &self.profiles;
        let accent = self.settings_accent();
        let selection = editor.selection();
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        if let Some(error) = editor.load_error() {
            items.push(label("Unreadable setup", 18., WARN));
            items.push(banner(
                format!("This setup can’t be read: {error}"),
                ERROR,
                vec![
                    button::text("Delete")
                        .on_press_maybe(editor.writable().then_some(edit(Edit::Delete(false))))
                        .into(),
                ],
            ));
            if editor.confirm_delete {
                items.push(self.delete_confirm("this unreadable setup".into()));
            }
            return section(items);
        }
        let Some(profile) = editor.profile() else {
            items.push(detail(
                "Current desktop",
                "Choose a setup to compare its changes.",
            ));
            return section(items);
        };
        let saved = matches!(selection, Selection::Saved(_));
        let writable = editor.writable();
        let available = writable
            && !self.native_busy
            && !editor.busy()
            && !editor.theme_loading()
            && editor.theme_problem().is_none();
        let mut library_actions: Vec<Element<'_, Message>> = Vec::new();
        if editor.mix.is_none() {
            library_actions.push(
                button::text("Save a copy…")
                    .leading_icon(icon::from_name("edit-copy-symbolic").size(14))
                    .on_press_maybe(
                        available.then_some(edit(Edit::Name(Some(NameAction::Duplicate)))),
                    )
                    .into(),
            );
            if self.page != Page::Themes {
                library_actions.push(
                    button::text("Remix…")
                        .id(cosmic::widget::Id::new("setup-remix"))
                        .on_press_maybe(available.then_some(edit(Edit::RemixSelected)))
                        .into(),
                );
            }
        }
        if let Selection::Saved(id) = selection {
            library_actions.push(
                button::text("Rename…")
                    .on_press_maybe(
                        available.then(|| edit(Edit::Name(Some(NameAction::Rename(id.clone()))))),
                    )
                    .into(),
            );
        }
        if saved && !matches!(selection, Selection::Saved(id) if id.as_str() == "before-themes") {
            library_actions.push(
                button::text("Delete…")
                    .leading_icon(icon::from_name("user-trash-symbolic").size(14))
                    .on_press_maybe(available.then_some(edit(Edit::Delete(false))))
                    .into(),
            );
        }
        let mut export_actions: Vec<Element<'_, Message>> = vec![
            button::text(if editor.sharing() {
                "Cancel image"
            } else {
                "Save image…"
            })
            .id(cosmic::widget::Id::new("profiles-share-image"))
            .on_press_maybe(
                (available || editor.sharing()).then_some(edit(if editor.sharing() {
                    Edit::CancelShare
                } else {
                    Edit::ShareImage
                })),
            )
            .into(),
            button::text(if profile.appearance_only {
                "Theme file (.json)…"
            } else {
                "Setup file (.json)…"
            })
            .id(cosmic::widget::Id::new("setup-export-file"))
            .leading_icon(icon::from_name("document-save-as-symbolic").size(14))
            .on_press_maybe(available.then_some(edit(Edit::Export)))
            .into(),
        ];
        if self.page != Page::Themes {
            export_actions.push(
                button::text("Portable package (.halcyon)…")
                    .id(cosmic::widget::Id::new("package-open"))
                    .on_press_maybe(available.then_some(edit(Edit::Package(
                        crate::settings::profiles::packages::Edit::Open,
                    ))))
                    .into(),
            );
        }
        let mut more = Vec::new();
        if !library_actions.is_empty() {
            more.push(label("Library · save without applying", 12., MUTED));
            more.push(row::with_children(library_actions).spacing(6).wrap().into());
        }
        more.push(label("Export", 12., MUTED));
        more.push(row::with_children(export_actions).spacing(6).wrap().into());
        more.push(label(
            "Images use sample content. Personal pictures stay on this computer.",
            12.,
            MUTED,
        ));
        if let Selection::Preset(id) = selection
            && let Some(hint) = crate::desktop::profiles::preset(id).and_then(|preset| {
                crate::desktop::profiles::missing(&preset, &editor.bundled_dir(), editor.islands())
            })
        {
            items.push(label(hint, 12., WARN));
        }

        if editor.mix.is_none()
            && let Some(origin) = &profile.composition
        {
            items.push(
                row::with_children(vec![
                    detail("Picture from", origin.picture.name.clone()),
                    detail("Style from", origin.style.name.clone()),
                    detail("Layout from", origin.layout.name.clone()),
                ])
                .spacing(16)
                .wrap()
                .into(),
            );
        }
        items.push(self.disclosure(
            "setup-library-actions",
            "Save & export",
            column::with_children(more).spacing(8).into(),
        ));
        if let Some(origin) = &profile.package {
            items.push(label(
                format!(
                    "Credits: {}",
                    if origin.credits.author.is_empty() && origin.credits.license.is_empty() {
                        "not provided".into()
                    } else {
                        [&origin.credits.author, &origin.credits.license]
                            .into_iter()
                            .filter(|s| !s.is_empty())
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .join(" · ")
                    }
                ),
                12.,
                MUTED,
            ));
            if !origin.credits.source.is_empty() {
                items.push(label(&origin.credits.source, 12., MUTED));
            }
        }
        if self.page != Page::Themes {
            items.push(self.setup_sources());
        }

        if editor.mix.is_none()
            && editor.name.as_ref().is_some_and(|name| {
                !matches!(name.action, NameAction::Capture | NameAction::CaptureTheme)
            })
        {
            items.push(self.name_field("Save"));
        }
        if editor.confirm_delete {
            items.push(self.delete_confirm(format!("“{}”", profile.name)));
        }
        items.push(separator());
        if let Selection::Preset(id) = selection
            && let Some(description) = crate::desktop::profiles::islands_description(id)
        {
            let preview_panel = editor.preview().and_then(|p| {
                p.bars
                    .iter()
                    .find(|b| b.kind() == crate::desktop::panels::PanelKind::Panel)
            });
            let customized = preview_panel
                .and_then(|p| p.islands().known().cloned())
                .is_some_and(|s| {
                    self.placement
                        .display
                        .as_ref()
                        .is_some_and(|output| s.outputs.contains_key(output))
                });
            items.push(detail("Panel composition", description));
            items.push(label(if !editor.included(Domain::Panel) {
                "Panel is excluded, so your current arrangement stays."
            } else if customized {
                "This display keeps its custom island positions. The setup changes the default arrangement."
            } else if !self.desktop.islands.installation.installed() {
                "The standard panel is used until Islands is installed and enabled."
            } else if preview_panel.and_then(|p| p.islands().known().cloned()).is_some_and(|s| !s.enabled) {
                "This saves the arrangement for when you turn Islands on in Dock & panel."
            } else {
                "Applies with Panel below. Your applets and display-specific arrangements are kept."
            }, 12., MUTED));
        }
        let mut review = Vec::new();
        if !profile.appearance_only {
            review.push(self.parts(profile, accent));
        }
        review.extend(self.plan_states());
        review.extend(items);
        column::with_children(review).spacing(16).into()
    }

    pub(super) fn delete_confirm(&self, name: String) -> Element<'_, Message> {
        let rules = match self.profiles.rules_using {
            0 => String::new(),
            1 => " An automation rule uses it and will stop working.".into(),
            count => format!(" {count} automation rules use it and will stop working."),
        };
        banner(
            format!(
                "Delete {name} from your {}?{rules}",
                if self.page == Page::Themes {
                    "themes"
                } else {
                    "setups"
                }
            ),
            WARN,
            vec![
                button::text("Cancel")
                    .on_press(edit(Edit::Delete(false)))
                    .into(),
                button::destructive("Delete")
                    .on_press(edit(Edit::Delete(true)))
                    .into(),
            ],
        )
    }

    /// Only actual capture domains are offered. Counts and restrictions come
    /// from the same plan that gates Apply; an omitted domain stays untouched.
    fn parts<'a>(
        &'a self,
        profile: &'a crate::desktop::profiles::Profile,
        _accent: Color,
    ) -> Element<'a, Message> {
        let plan = self.profiles.plan();
        let mut rows = vec![self.help(
            "setup-include",
            "Include",
            "Turn off a part to keep its current settings. Apply changes only the included parts.",
        )];
        for domain in profile.domains() {
            let on = self.profiles.included(domain);
            let part = plan.and_then(|plan| plan.domains.iter().find(|p| p.domain == domain));
            let status = if !on {
                Some(("Keep current", MUTED))
            } else if part.is_some_and(|part| !part.blocked.is_empty()) {
                Some(("Needs attention", WARN))
            } else {
                None
            };
            let mut line = vec![
                container(
                    cosmic::widget::checkbox(on)
                        .label(domain.title())
                        .on_toggle(move |on| edit(Edit::Include(domain, on))),
                )
                .id(cosmic::widget::Id::new(format!(
                    "setup-include-{}",
                    format!("{domain:?}").to_lowercase()
                )))
                .width(Length::Fill)
                .into(),
            ];
            if let Some((text, tone)) = status {
                line.push(label(text, 12., tone));
            }
            rows.push(
                row::with_children(line)
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
            );
        }
        if profile.local_member().is_some() {
            rows.push(label(
                "Personal pictures and display arrangements stay on this computer and are omitted on export.",
                12., MUTED,
            ));
        }
        column::with_children(rows).spacing(8).into()
    }

    /// Blockers, overlapping drafts, a stale preview and missing components
    /// stay visible; every change is listed on demand.
    pub(super) fn plan_states(&self) -> Vec<Element<'_, Message>> {
        let editor = &self.profiles;
        let mut items = Vec::new();
        let Some(plan) = editor.plan() else {
            return Vec::new();
        };
        if editor.stale() {
            items.push(banner(
                "Review the current changes before applying.",
                WARN,
                vec![
                    button::standard("Review changes")
                        .on_press(edit(Edit::Replan))
                        .into(),
                ],
            ));
        }
        let blocked = plan.blocking_reasons();
        if !blocked.is_empty() {
            // A part that can't be applied can be left out to apply the rest.
            let others = plan.domains.iter().any(|part| part.blocked.is_empty());
            let actions = plan
                .domains
                .iter()
                .filter(|part| !part.blocked.is_empty() && others)
                .map(|part| {
                    button::standard(format!("Leave out {}", part.domain.title()))
                        .on_press(edit(Edit::Include(part.domain, false)))
                        .into()
                })
                .collect();
            items.push(banner(reasons(&blocked), WARN, actions));
        }
        let warnings: Vec<String> = plan
            .domains
            .iter()
            .flat_map(|part| part.warnings.iter().cloned())
            .collect();
        // One line for missing components; the rest is in details.
        if let Some(first) = warnings.first() {
            let mut parts = vec![
                container(line(first.clone(), 12., WARN))
                    .width(Length::Fill)
                    .into(),
            ];
            if warnings.len() > 1 {
                parts.push(label(format!("+{} more", warnings.len() - 1), 12., MUTED));
            }
            items.push(
                row::with_children(parts)
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
            );
        }
        if blocked.is_empty() && plan.is_empty() {
            items.push(label("Already matches your desktop.", 12., MUTED));
        }
        if plan.domains.iter().any(|part| {
            !part.changes.is_empty() || !part.warnings.is_empty() || !part.blocked.is_empty()
        }) {
            items.push(
                button::text(if editor.details {
                    "Hide changes"
                } else {
                    "Review changes"
                })
                .trailing_icon(
                    icon::from_name(if editor.details {
                        "go-up-symbolic"
                    } else {
                        "go-down-symbolic"
                    })
                    .size(14),
                )
                .on_press(edit(Edit::Details(!editor.details)))
                .into(),
            );
        }
        if editor.details {
            items.push(plan_details(plan));
        }
        items
    }

    /// Setups can open before the Dock editor. Its own cached observation is
    /// authoritative, including the baseline behind a selected candidate.
    fn setup_panel_baseline(&self) -> Option<crate::desktop::panels::PanelDraft> {
        let kind = crate::desktop::panels::PanelKind::Panel;
        let panel = self
            .profiles
            .preview()?
            .bars
            .iter()
            .find(|p| p.kind() == kind)?;
        let mut snapshot = panel.clone().into_snapshot();
        if let Some(plan) = self.profiles.plan() {
            snapshot = original_snapshot(snapshot, plan.patch());
        }
        Some(crate::desktop::panels::PanelDraft::from_snapshot(
            kind, snapshot,
        ))
    }

    fn setup_islands_status(&self) -> &'static str {
        if !self.desktop.islands.installation.installed() {
            "Standard panel · Islands unavailable"
        } else if self
            .setup_panel_baseline()
            .and_then(|p| p.islands().known().cloned())
            .is_some_and(|s| s.enabled)
        {
            "Islands layout · your applet groups"
        } else {
            "Standard panel · Islands is off"
        }
    }

    pub(super) fn profiles_page(&self) -> Element<'_, Message> {
        if let Some(package) = &self.profiles.package {
            return self.package_page(package);
        }
        let editor = &self.profiles;
        let accent = self.settings_accent();
        let reviewing = editor.mix.is_some()
            || editor.profile().is_some()
            || editor.selection() != &Selection::Current
            || editor.load_error().is_some();
        let mut items: Vec<Element<'_, Message>> = if reviewing {
            vec![split(
                if editor.mix.is_some() {
                    row::with_children(vec![
                        button::text("Setups")
                            .leading_icon(icon::from_name("go-previous-symbolic").size(16))
                            .id(cosmic::widget::Id::new("setups-back"))
                            .on_press(edit(Edit::Discard))
                            .into(),
                        label("/ Remix", 18., FG),
                    ])
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into()
                } else {
                    button::text("Back to setups")
                        .leading_icon(icon::from_name("go-previous-symbolic").size(16))
                        .id(cosmic::widget::Id::new("setups-back"))
                        .on_press(edit(Edit::Discard))
                        .into()
                },
                super::windows::store_pill(editor.store(), accent),
            )]
        } else {
            vec![
                split(
                    heading("Setups", ""),
                    super::windows::store_pill(editor.store(), accent),
                ),
                container(label(
                    if editor.saved_collection {
                        self.setup_islands_status().to_owned()
                    } else {
                        format!("Built-in looks · {}", self.setup_islands_status())
                    },
                    14.,
                    MUTED,
                ))
                .id(cosmic::widget::Id::new("setups-welcome"))
                .into(),
            ]
        };
        if let Some(error) = editor.unavailable() {
            items.push(banner(
                format!("Desktop settings are unavailable. {error}"),
                ERROR,
                Vec::new(),
            ));
            return column::with_children(items).spacing(16).into();
        }
        match editor.recovery() {
            Some(Ok(_)) => items.push(banner(
                "An earlier change was interrupted.",
                WARN,
                vec![
                    button::text("Keep current")
                        .on_press_maybe(editor.writable().then_some(edit(Edit::KeepRecovery)))
                        .into(),
                    button::standard("Restore previous")
                        .on_press_maybe(editor.writable().then_some(edit(Edit::Recover)))
                        .into(),
                ],
            )),
            Some(Err(error)) => items.push(banner(
                error.clone(),
                ERROR,
                vec![
                    button::text("Keep current")
                        .on_press_maybe(editor.writable().then_some(edit(Edit::KeepRecovery)))
                        .into(),
                ],
            )),
            None => {}
        }
        if !editor.notes.is_empty() {
            items.push(
                column::with_children(
                    editor
                        .notes
                        .iter()
                        .map(|note| label(note.clone(), 12., MUTED))
                        .collect::<Vec<_>>(),
                )
                .spacing(4)
                .into(),
            );
        }
        if editor.mix.is_some() {
            items.push(self.mix_board());
        } else if reviewing {
            items.push(self.selection_section());
        } else {
            items.push(
                row::with_children(vec![
                    button::text("Remix…")
                        .id(cosmic::widget::Id::new("mix-start"))
                        .on_press_maybe((!self.native_busy).then_some(edit(Edit::MixStart)))
                        .into(),
                    button::custom(label("Discover", 14., FG))
                        .class(nav_style(!editor.saved_collection, accent))
                        .selected(!editor.saved_collection)
                        .padding([9, 16])
                        .id(cosmic::widget::Id::new("setups-explore"))
                        .on_press(edit(Edit::Collection(false)))
                        .into(),
                    button::custom(label("Your setups", 14., FG))
                        .class(nav_style(editor.saved_collection, accent))
                        .selected(editor.saved_collection)
                        .padding([9, 16])
                        .id(cosmic::widget::Id::new("setups-saved"))
                        .on_press(edit(Edit::Collection(true)))
                        .into(),
                ])
                .spacing(6)
                .into(),
            );
            if editor.saved_collection {
                items.push(self.library_section());
                items.push(self.schedules_summary());
            } else {
                items.push(self.discovery_gallery());
                // Pair calm and active compositions; the IDs remain stable for schedules.
                let looks = ["halcyon", "focus", "paper", "rose", "calm", "evening"]
                    .into_iter()
                    .filter_map(|id| editor.presets().iter().find(|p| p.id == id))
                    .filter(|p| {
                        crate::desktop::profiles::discovery::matches(p, &editor.discovery_query)
                    })
                    .map(|preset| self.preset_tile(preset))
                    .collect::<Vec<_>>();
                items.push(row::with_children(looks).spacing(16).wrap().into());
                items.push(label(
                    "Illustrative layouts · your content stays yours",
                    12.,
                    MUTED,
                ));
                items.push(self.quick_switches());
            }
        }
        column::with_children(items)
            .spacing(if reviewing { 20 } else { 16 })
            .into()
    }

    /// One-tap states that are not looks, such as hiding everything.
    fn quick_switches(&self) -> Element<'_, Message> {
        let id = crate::desktop::profiles::CLEAN;
        let selected = *self.profiles.selection() == Selection::Preset(id);
        let rows = self
            .profiles
            .presets()
            .iter()
            .filter(|preset| preset.id == id)
            .map(|preset| {
                split(
                    detail(preset.profile.name.clone(), preset.description),
                    button::text(if selected { "Chosen" } else { "Choose" })
                        .id(cosmic::widget::Id::new(format!("preset-{}", preset.id)))
                        .on_press_maybe(
                            (!selected).then_some(edit(Edit::Select(Selection::Preset(id)))),
                        )
                        .into(),
                )
            })
            .collect();
        column::with_children(vec![label("Quick switches", 14., FG), section(rows)])
            .spacing(8)
            .into()
    }

    /// The schedule rules at a glance; editing stays on its own page.
    fn schedules_summary(&self) -> Element<'_, Message> {
        let h24 = self.config.clock.format_24h;
        let mut rows: Vec<Element<'_, Message>> = Vec::new();
        match self.automation.settings() {
            Ok(settings) if !settings.rules.is_empty() => {
                for rule in settings.rules.iter().take(4) {
                    let name = self.automation.profile_name(&rule.profile, &rule.name);
                    rows.push(split(
                        detail(
                            format!("{} → {name}", rule.when.summary(h24)),
                            if !settings.enabled {
                                "Automatic switching is off"
                            } else if rule.enabled {
                                "On"
                            } else {
                                "Off"
                            },
                        ),
                        cosmic::widget::Space::new().width(0).into(),
                    ));
                }
                if settings.rules.len() > 4 {
                    rows.push(label(
                        format!("{} more", settings.rules.len() - 4),
                        12.,
                        MUTED,
                    ));
                }
            }
            _ => rows.push(label(
                "Use a setup at a time of day, or when a display connects.",
                12.,
                MUTED,
            )),
        }
        column::with_children(vec![
            split(
                label("Automatic switching", 14., FG),
                button::text("Manage automation")
                    .trailing_icon(icon::from_name("go-next-symbolic").size(14))
                    .id(cosmic::widget::Id::new("presets-schedules"))
                    .on_press(Message::Page(Page::DesktopAutomation))
                    .into(),
            ),
            section(rows),
        ])
        .spacing(8)
        .into()
    }

    pub(super) fn profiles_actions(&self) -> Element<'_, Message> {
        let editor = &self.profiles;
        if editor.package.is_some() {
            return column::with_children(vec![self.simple_status(), self.package_actions()])
                .into();
        }
        // Repair/reset previews can be based on Current without selecting a
        // preset. Their active profile still needs an explicit confirmation.
        if *editor.selection() == Selection::Current
            && editor.profile().is_none()
            && editor.mix.is_none()
        {
            return self.simple_status();
        }
        let problem = self.page_error().unwrap_or_else(|| {
            editor.theme_problem().unwrap_or(if editor.theme_loading() {
                "Finding wallpaper colors…"
            } else {
                "Local preview"
            })
        });
        let mut actions: Vec<Element<'_, Message>> = vec![
            container(label(problem, 12., MUTED))
                .width(Length::Fill)
                .into(),
        ];
        if editor.mix.is_some() && editor.profile().is_some() {
            actions.push(
                button::text(if editor.name.is_some() {
                    "Save setup"
                } else {
                    "Save setup…"
                })
                .id(cosmic::widget::Id::new("mix-save"))
                .on_press_maybe(
                    (editor.writable()
                        && !self.native_busy
                        && !editor.theme_loading()
                        && editor.theme_problem().is_none())
                    .then_some(edit(if editor.name.is_some() {
                        Edit::NameSubmit
                    } else {
                        Edit::Name(Some(NameAction::SaveMix))
                    })),
                )
                .into(),
            );
        }
        actions.push(
            button::text("Cancel")
                .on_press_maybe((!self.native_busy).then_some(edit(Edit::Discard)))
                .into(),
        );
        actions.push(
            button::suggested(if self.page == Page::Themes {
                "Apply theme"
            } else {
                "Apply setup"
            })
            .id("profiles-confirm".into())
            .on_press_maybe((editor.can_apply() && !self.native_busy).then_some(edit(Edit::Apply)))
            .into(),
        );
        container(
            row::with_children(actions)
                .spacing(10)
                .align_y(Alignment::Center),
        )
        .padding([8, 22])
        .width(Length::Fill)
        .into()
    }
}

/// Every part: what would change, what to know and what prevents it.
fn plan_details<'a>(plan: &ProfilePlan) -> Element<'a, Message> {
    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    for part in &plan.domains {
        let count = part.changes.len();
        let mut lines = vec![
            label(
                format!(
                    "{} · {} {}",
                    part.domain.title(),
                    count,
                    if count == 1 { "change" } else { "changes" }
                ),
                12.,
                FG,
            ),
            label(domain_hint(part.domain), 11., MUTED),
        ];
        if part.changes.is_empty() && part.blocked.is_empty() {
            lines.push(label("No changes", 11., MUTED));
        } else if !part.changes.is_empty() {
            lines.push(label(part.changes.join(", "), 11., MUTED));
        }
        for warning in &part.warnings {
            lines.push(label(warning.clone(), 11., WARN));
        }
        for reason in &part.blocked {
            lines.push(label(reason.clone(), 11., ERROR));
        }
        for note in &part.notes {
            lines.push(label(note.clone(), 11., MUTED));
        }
        rows.push(column::with_children(lines).spacing(3).into());
    }
    column::with_children(rows).spacing(10).into()
}

#[cfg(test)]
mod image_tests {
    use super::*;

    #[test]
    fn fb13_illustrative_readings_never_leave_partial_lines_in_short_cards() {
        use cosmic::iced::{Rectangle, Size, advanced::widget::Operation};
        use std::sync::{Arc, Mutex};
        struct Texts(Arc<Mutex<Vec<Rectangle>>>);
        impl Operation for Texts {
            fn traverse(&mut self, f: &mut dyn FnMut(&mut dyn Operation)) {
                f(self);
            }
            fn text(&mut self, _: Option<&cosmic::widget::Id>, bounds: Rectangle, _: &str) {
                self.0.lock().unwrap().push(bounds);
            }
        }
        for (size, scale) in [([180., 113.], 0.5), ([240., 180.], 2. / 3.)] {
            let config = Config::default();
            let mut native = crate::native_test::Native::new(Size::new(size[0], size[1]));
            let bounds = Arc::new(Mutex::new(Vec::new()));
            native.operate(
                card(Widget::Devbox, size, &config, scale),
                Box::new(Texts(bounds.clone())),
            );
            let bounds = bounds.lock().unwrap();
            assert!(bounds.len() >= 3, "title, detail and at least one reading");
            for rect in bounds.iter() {
                assert!(
                    rect.y >= 0. && rect.y + rect.height <= size[1],
                    "{size:?}: {rect:?}"
                );
                assert!(
                    rect.height >= 1.25 * (26. * scale).min(14.) - 0.01,
                    "partial line: {rect:?}"
                );
            }
        }
    }

    #[test]
    fn fb06_setup_clock_thumbnail_uses_the_selected_native_face() {
        use crate::native_test::Native;
        use cosmic::iced::Size;
        let mut config = Config::default();
        let mut native = Native::new(Size::new(245., 100.));
        config.clock.face = Some(crate::widget_style::resolver::ClockFace::Digital);
        let digital = native.render(
            card(Widget::Clock, [245., 100.], &config, 0.5),
            &cosmic::Theme::dark(),
        );
        config.clock.face = Some(crate::widget_style::resolver::ClockFace::Analog);
        let analog = native.render(
            card(Widget::Clock, [245., 100.], &config, 0.5),
            &cosmic::Theme::dark(),
        );
        assert!(
            digital
                .chunks_exact(4)
                .zip(analog.chunks_exact(4))
                .filter(|(a, b)| a != b)
                .count()
                > 100,
            "setup scene retained a digital placeholder"
        );
    }

    #[test]
    fn redesign_candidate_removing_custom_size_returns_to_natural_geometry() {
        let automatic = Config::default();
        let mut snapshot = crate::layout_state::read(&automatic);
        let clock = snapshot
            .widgets
            .iter_mut()
            .find(|w| w.key == "clock")
            .unwrap();
        clock.natural_size = Some([360., 140.]);
        clock.size = [800., 360.];
        let candidate = scene_geometry(snapshot.clone(), &automatic);
        assert_eq!(
            candidate
                .widgets
                .iter()
                .find(|w| w.key == "clock")
                .unwrap()
                .size,
            [360., 140.]
        );
        let mut custom = automatic.clone();
        custom.widget_sizes.insert("clock".into(), [800., 360.]);
        let current = scene_geometry(snapshot, &custom);
        assert_eq!(
            current
                .widgets
                .iter()
                .find(|w| w.key == "clock")
                .unwrap()
                .size,
            crate::layout::sized(Widget::Clock, [800., 360.], current.screen)
        );
    }

    #[test]
    fn comparison_restores_captured_defaults_without_mutating_candidate() {
        use crate::desktop::panels::{PanelDraft, PanelKind};
        use crate::desktop::store::{
            ConfigKey, DesktopPatch, KeyEntry, RawValue, Snapshot, widget::WidgetPatch,
        };
        use serde_json::json;
        let opacity = ConfigKey::new(PanelKind::Dock.namespace(), 1, "opacity").unwrap();
        let output = ConfigKey::new(PanelKind::Dock.namespace(), 1, "output").unwrap();
        let proposed = RawValue::parse("0.2").unwrap();
        let inherited = RawValue::parse("0.9").unwrap();
        let binding = RawValue::parse("Name(\"DP-1\")").unwrap();
        let mut snapshot = Snapshot::default();
        snapshot.insert(
            opacity.clone(),
            KeyEntry {
                user: Some(proposed.clone()),
                system: Some(inherited.clone()),
            },
        );
        snapshot.insert(
            output.clone(),
            KeyEntry {
                user: Some(binding.clone()),
                system: None,
            },
        );
        let mut native = DesktopPatch::new();
        native.set(opacity.clone(), None, Some(proposed));
        let defaults = Config::default();
        let mut config = defaults.clone();
        config.clock.format_24h = !defaults.clock.format_24h;
        config.clock.show_date = !defaults.clock.show_date;
        let mut widgets = WidgetPatch::new();
        widgets
            .set(
                "/clock/format_24h",
                None,
                Some(json!(config.clock.format_24h)),
            )
            .unwrap();
        let candidate = crate::settings::profiles::Preview {
            bars: vec![PanelDraft::from_snapshot(PanelKind::Dock, snapshot.clone())],
            appearance: None,
            config: Some(config.clone()),
        };
        let current = comparison_from_patches(&candidate, &native, &widgets);
        let restored = current.bars[0].clone().into_snapshot();
        assert!(restored.entry(&opacity).unwrap().user.is_none());
        assert_eq!(restored.effective(&opacity), Some(&inherited));
        assert_eq!(restored.effective(&output), Some(&binding));
        let current_config = current.config.unwrap();
        assert_eq!(current_config.clock.format_24h, defaults.clock.format_24h);
        assert_eq!(current_config.clock.show_date, config.clock.show_date);
        assert_eq!(candidate.bars[0].clone().into_snapshot(), snapshot);
        assert_eq!(
            serde_json::to_value(candidate.config.as_ref().unwrap()).unwrap(),
            serde_json::to_value(&config).unwrap()
        );
    }

    #[test]
    fn look_handles_reuse_the_same_decoded_pixels() {
        use cosmic::widget::image::Handle;
        images::prepare();
        for id in ["halcyon", "calm", "evening", "focus", "paper", "rose"] {
            for wallpaper in [false, true] {
                let first = look_image(id, wallpaper).unwrap();
                let second = look_image(id, wallpaper).unwrap();
                assert_eq!(first.id(), second.id());
                let (
                    Handle::Rgba {
                        pixels: a,
                        width,
                        height,
                        ..
                    },
                    Handle::Rgba { pixels: b, .. },
                ) = (&first, &second)
                else {
                    panic!("decoded pixels");
                };
                assert_eq!(
                    a.as_ptr(),
                    b.as_ptr(),
                    "handles share the only cached pixel buffer"
                );
                assert_eq!(a.len(), *width as usize * *height as usize * 4);
            }
        }
        assert!(look_image("unknown", true).is_none());
    }
}
