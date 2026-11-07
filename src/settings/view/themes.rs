//! Appearance: one page for the look of widgets, dock, panel and windows.
//! Style, color and mode apply as soon as they are chosen (one recoverable
//! desktop transaction each, undone from the footer); surfaces, fonts and
//! icons open as detail subpages.
use super::desktop::{ERROR, WARN, banner, line};
use super::profiles::tile_style;
use super::system_appearance::fill;
use super::*;
use crate::desktop::profiles::themes;
use crate::desktop::wallpaper::automatic;
use crate::desktop_theme::{Family, tone};
use crate::settings::profiles::{Message as Edit, NameAction, Selection};
use cosmic::iced::widget::stack;

fn edit(message: Edit) -> Message {
    Message::Profiles(message)
}

/// A segmented choice as a row's trailing control. Without the wrapping
/// row, a trailing segmented control lays out with no room for its labels.
fn choice_row(control: Element<'_, Message>) -> Element<'_, Message> {
    row::with_children(vec![control])
        .align_y(Alignment::Center)
        .wrap()
        .into()
}

/// What the page shows: the selected look, otherwise the applied one.
struct Look {
    family: Option<Family>,
    /// Colors follow the wallpaper.
    auto: bool,
    accent: Option<[u8; 3]>,
    selected: bool,
}

/// The color the look uses, as the color row names it.
#[derive(Clone, Copy, PartialEq)]
enum Choice {
    Wallpaper,
    Hue(usize),
    Style,
    Custom,
}

impl Settings {
    fn look(&self) -> Look {
        if self.profiles.profile().is_some_and(|p| p.appearance_only) {
            let appearance = self.profiles.color_preferences(&self.config);
            return Look {
                family: appearance.family,
                auto: appearance.wallpaper_colors,
                accent: appearance.accent,
                selected: true,
            };
        }
        let a = &self.config.appearance;
        // Before the first theme, specimens show the colors choosing one
        // would start with (see `ProfilesEditor::first_theme_colors`); after
        // it, choosing another style keeps this color.
        let first = a.family.is_none()
            && (a.wallpaper_colors
                || (a.accent.is_none()
                    && self
                        .profiles
                        .suggestion()
                        .is_some_and(|s| !s.swatches.is_empty())));
        Look {
            family: a.family,
            auto: (a.family.is_some() && a.wallpaper_colors) || first,
            accent: a.accent,
            selected: false,
        }
    }
    /// The wallpaper's lively hues, besides the one "From wallpaper" uses.
    fn wallpaper_hues(&self) -> Vec<[u8; 3]> {
        self.profiles
            .suggestion()
            .map(|s| s.accents.iter().skip(1).take(3).copied().collect())
            .unwrap_or_default()
    }
    fn choice(&self, look: &Look) -> Choice {
        if look.auto {
            return Choice::Wallpaper;
        }
        let choice = themes::LookColor::applied(self.profiles.color_preferences(&self.config));
        match choice {
            themes::LookColor::Wallpaper => Choice::Wallpaper,
            themes::LookColor::Style => Choice::Style,
            themes::LookColor::Fixed(rgb) => self
                .wallpaper_hues()
                .iter()
                .position(|hue| *hue == rgb)
                .map_or(Choice::Custom, Choice::Hue),
        }
    }
    /// The seed color a look would use for `family`. A style's own color is
    /// each style's own, as choosing that style gives it.
    fn look_accent(&self, look: &Look, family: Family) -> Option<[u8; 3]> {
        if look.auto {
            self.profiles
                .suggestion()
                .and_then(|s| automatic::recommended_for(s, Some(family)))
        } else {
            let policy = themes::LookColor::applied(self.profiles.color_preferences(&self.config));
            match policy {
                themes::LookColor::Fixed(rgb) => Some(rgb),
                _ => None,
            }
        }
    }

    pub(super) fn themes_page(&self) -> Element<'_, Message> {
        let editor = &self.profiles;
        let look = self.look();
        let mut rows = vec![heading(
            "Appearance",
            "Style, colors and fonts for your desktop and apps.",
        )];
        if let Some(error) = editor.unavailable() {
            rows.push(banner(error.to_string(), ERROR, vec![]));
            return column::with_children(rows).spacing(16).into();
        }
        if editor.recovery().is_some() {
            rows.push(banner(
                "An earlier desktop change was interrupted.",
                WARN,
                vec![
                    button::text("Keep current")
                        .on_press(edit(Edit::KeepRecovery))
                        .into(),
                    button::standard("Restore previous")
                        .on_press(edit(Edit::Recover))
                        .into(),
                ],
            ));
        }
        rows.push(self.theme_stage(&look));
        rows.push(self.appearance_styles(&look));
        rows.push(self.appearance_color(&look));
        rows.push(self.appearance_surfaces());
        rows.push(self.appearance_windows());
        if let Some(p) = editor.profile().filter(|p| !p.appearance_only) {
            rows.push(banner(
                format!("“{}” is a whole-desktop look. Open it in Looks.", p.name),
                WARN,
                vec![
                    button::text("Open Looks")
                        .on_press(Message::Page(Page::DesktopProfiles))
                        .into(),
                ],
            ));
        }
        if look.family == Some(Family::Paper)
            && editor.installed.fonts.family(false, "Noto Serif").is_none()
        {
            rows.push(label(
                "Noto Serif isn’t installed, so the clock uses the system font.",
                12.,
                MUTED,
            ));
        }
        for note in &editor.notes {
            rows.push(label(note.clone(), 12., MUTED));
        }
        if let Some(error) = editor.theme_problem() {
            rows.push(banner(
                error,
                ERROR,
                vec![
                    button::text("Use the theme’s color")
                        .on_press(edit(Edit::ThemeColors(false)))
                        .into(),
                ],
            ));
        }
        // Saving and sharing happen in Looks. Themes saved by earlier
        // versions stay reachable here.
        if editor.library().is_ok_and(|entries| {
            entries
                .iter()
                .any(|entry| entry.profile.as_ref().is_ok_and(|p| p.appearance_only))
        }) {
            rows.push(self.library_section());
        }
        column::with_children(rows).spacing(20).into()
    }

    /// Every style as a small specimen; choosing one applies it.
    fn appearance_styles(&self, look: &Look) -> Element<'_, Message> {
        // The same width as the rows below, so the right edges line up.
        let content = (self.content_width() - 2.).max(320.);
        let columns = (((content + 10.) / 178.).floor() as usize).clamp(2, 6);
        let tile = ((content - 10. * (columns - 1) as f32) / columns as f32).floor();
        // One row of styles, the chosen one included; the rest on request.
        let all = self.open_sections.contains("appearance-all-styles");
        let applied = self.config.appearance.family;
        let mut families: Vec<Family> = Family::ALL.into_iter().collect();
        if !all {
            if let Some(index) = applied.and_then(|f| families.iter().position(|x| *x == f))
                && index >= columns
            {
                let chosen = families.remove(index);
                families.insert(columns - 1, chosen);
            }
            families.truncate(columns);
        }
        let tiles: Vec<Element<'_, Message>> = families
            .into_iter()
            .map(|family| self.style_tile(family, look, tile))
            .collect();
        column::with_children(vec![
            split(
                text::heading("Style").into(),
                button::text(if all {
                    "Show fewer".to_string()
                } else {
                    format!("Show all {}", Family::ALL.len())
                })
                .id(cosmic::widget::Id::new("appearance-all-styles"))
                .on_press(Message::ToggleSection("appearance-all-styles"))
                .into(),
            ),
            row::with_children(tiles)
                .spacing(10)
                .wrap()
                .vertical_spacing(10)
                .into(),
        ])
        .spacing(10)
        .into()
    }

    /// One color, light or dark, and where the color is used.
    fn appearance_color(&self, look: &Look) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let mut rows = self.look_colors(look);
        rows.push(group_row(
            "Mode",
            "Automatic turns dark after sunset",
            container(choice_row(segmented(
                crate::desktop::appearance::SystemMode::ALL
                    .into_iter()
                    .map(|mode| {
                        (
                            mode.title().into(),
                            self.profiles.color_mode() == Some(mode),
                            Some(edit(Edit::Color(
                                crate::settings::profiles::colors::Edit::Mode(mode),
                            ))),
                        )
                    })
                    .collect(),
                accent,
            )))
            .id(cosmic::widget::Id::new("appearance-mode"))
            .into(),
        ));
        if self.config.appearance.family.is_some()
            && self.config.appearance.native_mode_palettes == 0
        {
            rows.push(group_row(
                "Dock and panel don’t follow light and dark yet",
                "",
                button::standard("Fix")
                    .id("theme-matching-repair".into())
                    .on_press(edit(Edit::RepairThemeMatching))
                    .into(),
            ));
        }
        let sync = self.profiles.color_preferences(&self.config).sync_colors;
        rows.push(group_row(
            "Use these colors in apps",
            "Off keeps COSMIC’s own colors in apps",
            container(toggler(sync).on_toggle(|on| edit(Edit::ThemeScope(on))))
                .id(cosmic::widget::Id::new("colors-scope"))
                .into(),
        ));
        group(Some("Color"), rows)
    }

    /// The shared material and widget corners; details open on a subpage.
    fn appearance_surfaces(&self) -> Element<'_, Message> {
        use crate::materials::Target;
        let accent = self.settings_accent();
        let shared = self.config.materials.effective(Target::Shared);
        let finish = self
            .material_draft
            .filter(|_| self.material_target == Target::Shared)
            .or(shared);
        let mut rows = vec![self.link_row_state(
            "Glass and transparency",
            match finish {
                Some(finish) => format!(
                    "{}, {}% see-through",
                    finish.kind.title(),
                    100 - finish.opacity()
                ),
                None => "No glass".to_string(),
            },
            Message::Page(Page::Appearance),
            "appearance-surfaces".into(),
        )];
        let radius = self.config.appearance.corner_radius;
        rows.push(group_row(
            "Widget corners",
            "",
            container(choice_row(segmented(
                [("Square", 0u16), ("Rounded", 8), ("Very round", 16)]
                    .into_iter()
                    .map(|(title, value)| {
                        let on = match value {
                            0 => radius < 4,
                            8 => (4..12).contains(&radius),
                            _ => radius >= 12,
                        };
                        (title.into(), on, Some(Message::WidgetCorners(value)))
                    })
                    .collect(),
                accent,
            )))
            .id(cosmic::widget::Id::new("appearance-corners"))
            .into(),
        ));
        // A day or night widget style overrides the style above, so while
        // one shows, say so; choosing them lives on Looks › Time of day.
        if let Some(phase) = self.config.moments.active {
            rows.push(self.link_row_state(
                "Day and night widget style",
                format!("Showing the {} style · Time of day", phase.title().to_lowercase()),
                Message::Page(Page::Moments),
                "appearance-moments".into(),
            ));
        }
        group(Some("Widgets and bars"), rows)
    }

    /// Window outline and density; fonts and icons open on a subpage.
    fn appearance_windows(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let mut rows = Vec::new();
        if let Some(draft) = self.system_appearance.draft() {
            rows.push(group_row(
                "Density",
                "Spacing in apps",
                super::system_appearance::choice(
                    &draft.density(),
                    &[
                        ("Compact", cosmic::cosmic_theme::Density::Compact),
                        ("Standard", cosmic::cosmic_theme::Density::Standard),
                        ("Spacious", cosmic::cosmic_theme::Density::Spacious),
                    ],
                    crate::settings::system_appearance::Message::Density,
                    accent,
                ),
            ));
        }
        rows.push(self.subpage_row(
            "Fonts and icons",
            "Text fonts and icon theme",
            Page::DesktopAppearance,
            "themes-fonts-icons",
        ));
        group(Some("Text and icons"), rows)
    }

    /// A row that opens a detail page.
    pub(super) fn subpage_row(
        &self,
        title: &'static str,
        description: &'static str,
        page: Page,
        id: &'static str,
    ) -> Element<'_, Message> {
        button::custom(
            row::with_children(vec![
                detail(title, description),
                icon::from_name("go-next-symbolic").size(16).icon().into(),
            ])
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .id(cosmic::widget::Id::new(id))
        .class(nav_style(false, self.settings_accent()))
        .padding([4, 0])
        .width(Length::Fill)
        .on_press(Message::Page(page))
        .into()
    }

    /// The live preview of what the desktop looks like now. It has no live
    /// desktop side effects.
    fn theme_stage(&self, look: &Look) -> Element<'_, Message> {
        // Always shown, without a header: the style tiles name the current
        // style, and parts of the scene are clicked to edit them.
        let height = self.stage_height();
        let scene = match self.profiles.preview() {
            Some(preview) => self.profile_scene(preview.clone(), height, false, true),
            None => label("Reading your desktop…", 12., MUTED),
        };
        let mut parts = vec![container(scene)
            .id(cosmic::widget::Id::new("desktop-scene"))
            .width(Length::Fill)
            .into()];
        // A saved theme keeps its name and library actions.
        if matches!(self.profiles.selection(), Selection::Saved(_)) {
            parts.push(pages::section(vec![self.theme_caption(look)]));
        }
        column::with_children(parts).spacing(12).into()
    }

    /// The look's name and what can be done with it.
    fn theme_caption(&self, look: &Look) -> Element<'_, Message> {
        let editor = &self.profiles;
        let selection = editor.selection();
        let (title, subtitle) = match (editor.profile(), look.family) {
            (Some(p), family) if p.appearance_only => (
                p.name.clone(),
                match (selection, family) {
                    (Selection::Saved(id), _) if id.as_str() == themes::BEFORE_THEMES => {
                        "Your look before the first theme".to_string()
                    }
                    (Selection::Saved(_), _) => "Saved on this computer".to_string(),
                    (_, Some(family)) => family.description().to_string(),
                    (_, None) => String::new(),
                },
            ),
            (None, Some(family)) => (
                format!("{} · on your desktop", family.title()),
                "Your applied theme".into(),
            ),
            _ => (
                "Your desktop".into(),
                "Choose a theme to preview it here".into(),
            ),
        };
        let mut actions: Vec<Element<'_, Message>> = Vec::new();
        if editor.profile().is_some_and(|p| p.appearance_only) {
            let writable = editor.writable();
            let action = |name: &'static str, tip: &'static str, message: Option<Message>| {
                button::icon(icon::from_name(name).size(16))
                    .tooltip(tip)
                    .on_press_maybe(message)
                    .into()
            };
            actions.push(action(
                "edit-copy-symbolic",
                "Save a copy",
                writable.then_some(edit(Edit::Name(Some(NameAction::Duplicate)))),
            ));
            if let Selection::Saved(id) = selection {
                actions.push(action(
                    "document-edit-symbolic",
                    "Rename",
                    writable.then(|| edit(Edit::Name(Some(NameAction::Rename(id.clone()))))),
                ));
            }
            actions.push(action(
                "document-save-as-symbolic",
                "Export…",
                (writable && !editor.busy()).then_some(edit(Edit::Export)),
            ));
            if matches!(selection, Selection::Saved(id) if id.as_str() != themes::BEFORE_THEMES) {
                actions.push(action(
                    "user-trash-symbolic",
                    "Delete…",
                    writable.then_some(edit(Edit::Delete(false))),
                ));
            }
        }
        let mut items = vec![
            row::with_children(vec![
                column::with_children(vec![label(title, 15., FG), label(subtitle, 12., MUTED)])
                    .spacing(2)
                    .width(Length::Fill)
                    .into(),
                row::with_children(actions).spacing(2).into(),
            ])
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        ];
        if editor.name.as_ref().is_some_and(|name| {
            !matches!(name.action, NameAction::Capture | NameAction::CaptureTheme)
        }) {
            items.push(self.name_field("Save"));
        }
        if editor.confirm_delete
            && let Some(p) = editor.profile()
        {
            items.push(self.delete_confirm(format!("“{}”", p.name)));
        }
        column::with_children(items).spacing(10).into()
    }

    /// The configuration a style would draw with: the style's geometry and
    /// fonts (not the user's overrides) in the look's color.
    fn style_config(&self, family: Family, look: &Look) -> Config {
        let mut config = self.config.clone();
        let a = &mut config.appearance;
        a.family = Some(family);
        (a.opacity, a.corner_radius, a.border_opacity) = family.geometry();
        a.surface = None;
        a.contrast_assist = false;
        a.accent = self.look_accent(look, family);
        a.wallpaper_light = family.light();
        config.widget_font = None;
        config.clock_font = None;
        config
    }

    /// A style as a small desktop `width` wide, on its own wallpaper: the
    /// panel with workspaces, clock and tray; a card with the time, date and
    /// an event and a second card, in the style's fonts, borders and
    /// shadows; and the dock. The dock and panel use the style's color.
    fn style_specimen(&self, family: Family, look: &Look, width: f32) -> Element<'_, Message> {
        let config = self.style_config(family, look);
        let spec = family.spec();
        let roles = config.theme_roles().expect("a family is set");
        let card = roles.card;
        let (border_width, shadow) = {
            let _style = crate::widget_style::scope(&config);
            crate::widget_style::edge(card)
        };
        let height = (width * 0.62).round();
        let s = height / 160.;
        let border = Color {
            a: if roles.transparent {
                0.
            } else {
                f32::from(spec.geometry.2.min(50)) / 100.
            },
            ..card.accent
        };
        let radius = f32::from(spec.geometry.1.min(32)) * s * 0.6;
        let card_style = move |_: &cosmic::Theme| cosmic::iced::widget::container::Style {
            background: Some(Background::Color(card.background)),
            border: Border {
                color: border,
                width: if roles.transparent { 0. } else { border_width },
                radius: radius.into(),
            },
            shadow: cosmic::iced::Shadow {
                blur_radius: shadow.blur_radius * s * 0.8,
                offset: shadow.offset * s * 0.8,
                ..shadow
            },
            ..Default::default()
        };
        let text = |value: &'static str, size: f32, font, color| -> Element<'static, Message> {
            text::body(value)
                .size((size * s).round().max(6.))
                .font(font)
                .wrapping(cosmic::iced::advanced::text::Wrapping::None)
                .class(cosmic::theme::Text::Color(color))
                .into()
        };
        let block = |w: f32, h: f32, color: Color, r: f32| -> Element<'static, Message> {
            fill(
                cosmic::iced::widget::space()
                    .width(w * s)
                    .height(h * s)
                    .into(),
                color,
                Color::TRANSPARENT,
                0.,
                r * s,
            )
            .into()
        };
        // The bars in the style's own color (see `themes::record`).
        let bar = spec.bar;
        let tone = {
            let [r, g, b] = family.surface(family.accent());
            Color::from_rgba8(r, g, b, bar.opacity)
        };
        let bar_radius = (bar.radius as f32 * s * 0.5).min(8. * s);
        let ink = card.foreground;
        let bar_style = move |_: &cosmic::Theme| cosmic::iced::widget::container::Style {
            background: Some(Background::Color(tone)),
            border: Border {
                radius: bar_radius.into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let dot_radius = if bar.radius == 0 { 0. } else { 3. };
        let panel = container(
            row::with_children(vec![
                row::with_children(vec![
                    block(6., 6., card.accent, dot_radius),
                    block(6., 6., Color { a: 0.45, ..ink }, dot_radius),
                ])
                .spacing(4. * s)
                .width(Length::Fill)
                .into(),
                text("09:41", 9., roles.body, ink),
                container(
                    row::with_children(vec![
                        block(7., 7., Color { a: 0.7, ..ink }, dot_radius * 0.6),
                        block(7., 7., Color { a: 0.7, ..ink }, dot_radius * 0.6),
                    ])
                    .spacing(4. * s),
                )
                .width(Length::Fill)
                .align_x(Alignment::End)
                .into(),
            ])
            .align_y(Alignment::Center),
        )
        .padding([2. * s, 8. * s])
        .width(Length::Fill)
        .class(cosmic::theme::Container::custom(bar_style));
        let event = row::with_children(vec![
            block(3., 12., card.accent, 1.5),
            text("Standup", 10., roles.body, card.foreground),
        ])
        .spacing(5. * s)
        .align_y(Alignment::Center);
        let main = container(
            column::with_children(vec![
                text("09:41", 30., roles.clock, card.foreground),
                text("Wed · Sunny", 10., roles.body, card.muted),
                event.into(),
            ])
            .spacing(4. * s),
        )
        .padding([9. * s, 11. * s])
        .width(118. * s)
        .class(cosmic::theme::Container::custom(card_style));
        let side = container(
            column::with_children(vec![
                block(52., 4., card.accent, 2.),
                block(
                    38.,
                    4.,
                    Color {
                        a: 0.6,
                        ..card.muted
                    },
                    2.,
                ),
                block(
                    46.,
                    4.,
                    Color {
                        a: 0.6,
                        ..card.muted
                    },
                    2.,
                ),
            ])
            .spacing(6. * s),
        )
        .padding([10. * s, 10. * s])
        .width(76. * s)
        .height(52. * s)
        .class(cosmic::theme::Container::custom(card_style));
        let icon_radius = if bar.radius == 0 { 0. } else { 3.5 };
        let dock = container(
            row::with_children(vec![
                block(11., 11., card.accent, icon_radius),
                block(11., 11., Color { a: 0.8, ..ink }, icon_radius),
                block(11., 11., Color { a: 0.55, ..ink }, icon_radius),
                block(11., 11., Color { a: 0.55, ..ink }, icon_radius),
            ])
            .spacing(5. * s),
        )
        .padding([4. * s, 7. * s])
        .class(cosmic::theme::Container::custom(bar_style));
        let (top, bottom) = spec.wallpaper;
        let stage = container(cosmic::iced::widget::space())
            .width(width)
            .height(height)
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    background: Some(Background::Gradient(
                        cosmic::iced::gradient::Linear::new(2.4)
                            .add_stop(0., Color::from_rgb8(top[0], top[1], top[2]))
                            .add_stop(1., Color::from_rgb8(bottom[0], bottom[1], bottom[2]))
                            .into(),
                    )),
                    border: Border {
                        radius: 7.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }));
        let floating = if bar.floating { 5. * s } else { 0. };
        stack(vec![
            stage.into(),
            container(panel)
                .padding([floating, floating])
                .width(width)
                .into(),
            container({
                // A small specimen keeps just the main card.
                let mut cards: Vec<Element<'_, Message>> = vec![main.into()];
                if width >= 180. {
                    cards.push(
                        container(side)
                            .width(Length::Fill)
                            .align_x(Alignment::End)
                            .into(),
                    );
                }
                row::with_children(cards).align_y(Alignment::Start)
            })
            .padding([24. * s, 12. * s, 0., 12. * s])
            .width(width)
            .height(height)
            .into(),
            container(dock)
                .padding([0., 0., 5. * s + floating * 0.4, 0.])
                .width(width)
                .height(height)
                .align_x(Alignment::Center)
                .align_y(Alignment::End)
                .into(),
        ])
        .width(width)
        .height(height)
        .clip(true)
        .into()
    }

    /// A style to choose: its small desktop, name and description.
    fn style_tile(&self, family: Family, look: &Look, width: f32) -> Element<'_, Message> {
        let editor = &self.profiles;
        let selected = *editor.selection() == Selection::Preset(family.preset())
            || (look.selected
                && look.family == Some(family)
                && matches!(editor.selection(), Selection::Preset(_)));
        let applied = self.config.appearance.family == Some(family);
        let selected = selected || applied;
        let marker: Element<'_, Message> = if applied {
            label("Current", 11., self.settings_accent())
        } else {
            cosmic::iced::widget::space().into()
        };
        container(
            button::custom(
                column::with_children(vec![
                    self.style_specimen(family, look, width - 16.),
                    row::with_children(vec![
                        container(line(family.title(), 14., FG))
                            .width(Length::Fill)
                            .into(),
                        marker,
                    ])
                    .spacing(4)
                    .align_y(Alignment::Center)
                    .into(),
                    line(family.description(), 12., MUTED),
                ])
                .spacing(6),
            )
            .class(tile_style(selected, self.settings_accent()))
            .selected(selected)
            .padding(8)
            .width(Length::Fill)
            .on_press(edit(Edit::Select(Selection::Preset(family.preset())))),
        )
        .id(cosmic::widget::Id::new(format!(
            "preset-{}",
            family.preset()
        )))
        .width(width)
        .into()
    }

    /// One color choice: from the wallpaper (it follows changes), one of
    /// its other hues, the style's own color, or a custom one; then the
    /// optional matching wallpaper.
    fn look_colors(&self, look: &Look) -> Vec<Element<'_, Message>> {
        let family = look.family;
        let accent = self.settings_accent();
        let choice = self.choice(look);
        let chip = |content: Element<'static, Message>,
                    selected: bool,
                    message: Message,
                    id: String|
         -> Element<'static, Message> {
            container(
                button::custom(content)
                    .class(tile_style(selected, accent))
                    .selected(selected)
                    .padding([5, 10])
                    .on_press(message),
            )
            .id(cosmic::widget::Id::new(id))
            .into()
        };
        let named = |rgb: Option<[u8; 3]>, title: String| -> Element<'static, Message> {
            let mut parts: Vec<Element<'static, Message>> = Vec::new();
            if let Some(rgb) = rgb {
                parts.push(dot(tone::color(rgb), 16.));
            }
            parts.push(label(title, 13., FG));
            row::with_children(parts)
                .spacing(7)
                .align_y(Alignment::Center)
                .into()
        };
        let auto_rgb = self
            .profiles
            .suggestion()
            .and_then(|s| automatic::recommended_for(s, family));
        let mut chips = vec![chip(
            named(auto_rgb, "From wallpaper".into()),
            choice == Choice::Wallpaper,
            edit(Edit::ThemeColors(true)),
            "theme-color-wallpaper".into(),
        )];
        for (index, hue) in self.wallpaper_hues().into_iter().enumerate() {
            let [r, g, b] = hue;
            chips.push(
                cosmic::widget::tooltip(
                    chip(
                        dot(tone::color(hue), 16.),
                        choice == Choice::Hue(index),
                        edit(Edit::ThemeAccent(hue)),
                        format!("theme-color-hue-{index}"),
                    ),
                    text::body(format!(
                        "Another color from your wallpaper · #{r:02X}{g:02X}{b:02X}"
                    ))
                    .size(12),
                    cosmic::widget::tooltip::Position::Top,
                )
                .into(),
            );
        }
        chips.push(chip(
            named(
                Some(family.map_or_else(
                    || crate::desktop_theme::bytes(Palette::named(&self.config.theme).accent),
                    Family::accent,
                )),
                family.map_or("Style color", Family::title).into(),
            ),
            choice == Choice::Style,
            edit(Edit::ThemeColors(false)),
            "theme-color-style".into(),
        ));
        let custom_rgb = (choice == Choice::Custom).then_some(look.accent).flatten();
        chips.push(chip(
            named(custom_rgb, "Custom".into()),
            choice == Choice::Custom,
            edit(Edit::ThemeCustom(Some(
                custom_rgb
                    .or(look.accent)
                    .map(|[r, g, b]| format!("#{r:02X}{g:02X}{b:02X}"))
                    .unwrap_or_default(),
            ))),
            "theme-color-custom".into(),
        ));
        let neutral = self
            .profiles
            .suggestion()
            .is_some_and(|s| s.accents.is_empty() && !s.swatches.is_empty());
        // A wallpaper with no usable color, or a problem reading it, stays in
        // view; what each choice means sits behind "?".
        let live = matches!(choice, Choice::Wallpaper) && (neutral || auto_rgb.is_none());
        let description = match choice {
            Choice::Wallpaper if neutral => format!(
                "This wallpaper has no strong color, so {}’s own is used until it changes",
                family.map_or("your palette", Family::title)
            ),
            Choice::Wallpaper if auto_rgb.is_some() => {
                "Follows your wallpaper, including when it changes".to_string()
            }
            Choice::Wallpaper => self
                .profiles
                .theme_problem()
                .or(self.profiles.colors_unavailable())
                .map_or_else(
                    || "Finding your wallpaper’s colors…".to_string(),
                    str::to_string,
                ),
            Choice::Hue(_) => "A color from your wallpaper. It stays as it is".to_string(),
            Choice::Style => match family {
                Some(family) => format!("Highlights and buttons, from the {} style", family.title()),
                None => "Highlights and buttons, from your style".to_string(),
            },
            Choice::Custom => "Your color".to_string(),
        };
        // Labelled like every other row: the name left, the choices right.
        let mut rows = vec![split(
            if live {
                detail_state("Accent color", description)
            } else {
                detail("Accent color", description)
            },
            row::with_children(chips)
                .spacing(4)
                .wrap()
                .vertical_spacing(4)
                .into(),
        )];
        if let Some(text) = &self.profiles.custom {
            let target = crate::settings::color_well::Target::ThemeCustom;
            rows.push(
                row::with_children(vec![
                    self.color_button(
                        "theme-color-hex",
                        target,
                        crate::settings::wallpaper::parse_rgb(text),
                        true,
                    ),
                    button::text("Done")
                        .on_press(edit(Edit::ThemeCustom(None)))
                        .into(),
                ])
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            );
            if let Some(panel) = self.color_panel(target, &[]) {
                rows.push(panel);
            }
        }
        rows
    }
}
