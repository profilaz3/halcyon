//! Native inspector with direct completed controls and local form validation.
use super::desktop::{ERROR, WARN, banner, conflict_banner, kept, line};
use super::pages::section;
use super::*;
use crate::desktop::appearance::catalog::{self, weight_title};
use crate::desktop::appearance::{
    self as domain, AppearanceDraft, ColorRole, FontRole, Preview, Surface, SystemMode,
    ThemeVariant, WindowEffect,
};
use crate::desktop::panels::Setting;
use crate::settings::system_appearance::{Message as Edit, Picker, picker_id, search_id};
use cosmic::cosmic_theme::palette::Srgba;
use cosmic::cosmic_theme::{Density, Roundness};
use cosmic::iced::font::Style;

fn edit(message: Edit) -> Message {
    Message::SystemAppearance(message)
}
pub(super) fn color(value: Srgba) -> Color {
    Color::from_rgba(value.red, value.green, value.blue, value.alpha)
}
pub(super) fn fill<'a>(
    content: Element<'a, Message>,
    background: Color,
    border: Color,
    width: f32,
    radius: f32,
) -> cosmic::widget::Container<'a, Message, cosmic::Theme> {
    // The shared settings tokens follow the light or dark COSMIC theme;
    // sample colors pass through unchanged.
    container(content).class(cosmic::theme::Container::custom(move |theme| {
        cosmic::iced::widget::container::Style {
            background: Some(Background::Color(surface_color(theme, background))),
            border: Border {
                color: surface_color(theme, border),
                width,
                radius: radius.into(),
            },
            ..Default::default()
        }
    }))
}
fn swatch<'a>(value: Color, size: u16) -> Element<'a, Message> {
    fill(
        cosmic::iced::widget::space()
            .width(size)
            .height(size)
            .into(),
        value,
        Color { a: 0.35, ..FG },
        1.,
        f32::from(size) / 2.,
    )
    .into()
}
/// Field label column: fixed width so controls line up.
fn field<'a>(title: &'static str, control: Element<'a, Message>) -> Element<'a, Message> {
    column::with_children(vec![label(title, 13., FG), control])
        .spacing(6)
        .width(Length::Fill)
        .into()
}
fn choice<'a, T: PartialEq + Copy>(
    setting: &Setting<T>,
    options: &[(&'static str, T)],
    message: impl Fn(T) -> Edit,
    accent: Color,
) -> Element<'a, Message> {
    let current = setting.known();
    let mut parts = vec![segmented(
        options
            .iter()
            .map(|(title, value)| {
                (
                    title.to_string(),
                    current == Some(value),
                    Some(edit(message(*value))),
                )
            })
            .collect(),
        accent,
    )];
    if let Setting::Unsupported(raw) = setting {
        parts.push(kept(raw));
    }
    row::with_children(parts)
        .spacing(8)
        .align_y(Alignment::Center)
        .wrap()
        .into()
}

impl Settings {
    fn appearance_accent(&self) -> Color {
        self.settings_accent()
    }

    /// Global typography and the explicit color variant being edited.
    pub(super) fn system_appearance_header(&self) -> Element<'_, Message> {
        let accent = self.appearance_accent();
        let editor = &self.system_appearance;
        let title = split(
            heading("Fonts & icons", ""),
            super::windows::store_pill(editor.store(), accent),
        );
        let Some(draft) = editor.draft() else {
            return title;
        };
        let active = draft.active_variant();
        let selector = segmented(
            ThemeVariant::ALL
                .into_iter()
                .map(|variant| {
                    let edited = draft.edits().any(|(control, _)| {
                        matches!(control, domain::Control::Color(owner, _) if *owner == variant)
                    });
                    let mut title = variant.title().to_string();
                    if variant == active {
                        title.push_str(" · in use");
                    }
                    if editor.unfinished_variant(variant) {
                        title.push_str(" · finish color");
                    } else if edited {
                        title.push_str(" •");
                    }
                    (
                        title,
                        editor.variant == variant,
                        Some(edit(Edit::Variant(variant))),
                    )
                })
                .collect(),
            accent,
        );
        let system =
            button::text("Open COSMIC Settings ↗").on_press(Message::OpenDesktop("appearance"));
        column::with_children(vec![
            title,
            row::with_children(vec![
                label("Colors to edit", 12., MUTED),
                selector,
                system.into(),
            ])
            .spacing(10)
            .align_y(Alignment::Center)
            .wrap()
            .into(),
        ])
        .spacing(10)
        .width(Length::Fill)
        .into()
    }

    pub(super) fn system_appearance_page(&self) -> Element<'_, Message> {
        let editor = &self.system_appearance;
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        if let Some(error) = editor.unavailable() {
            items.push(banner(
                format!("Appearance settings are unavailable. {error}"),
                ERROR,
                Vec::new(),
            ));
            return column::with_children(items).spacing(16).into();
        }
        let Some(draft) = editor.draft() else {
            items.push(label("Reading appearance settings…", 12., MUTED));
            return column::with_children(items).spacing(16).into();
        };
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
        if !draft.conflicts().is_empty() {
            items.push(conflict_banner(
                crate::settings::system_appearance::describe(draft.conflicts()),
                edit(Edit::UseCurrent),
            ));
        }
        if editor.accent_choice.is_some() {
            items.push(banner(
                "Replace your unapplied accent with the wallpaper color?",
                WARN,
                vec![
                    button::text("Cancel")
                        .on_press(edit(Edit::KeepAccent))
                        .into(),
                    button::standard("Replace accent")
                        .on_press(edit(Edit::ReplaceAccent))
                        .into(),
                ],
            ));
        }
        let blocked = editor.blocked();
        if !blocked.is_empty() {
            let names = crate::settings::system_appearance::describe(
                &blocked.iter().map(|b| b.key.clone()).collect::<Vec<_>>(),
            );
            items.push(banner(
                format!(
                    "Can’t apply: this version doesn’t recognize part of {names}. It is kept as it is; discard these edits to continue."
                ),
                ERROR,
                vec![
                    button::text("Discard")
                        .on_press(edit(Edit::Discard))
                        .into(),
                ],
            ));
        }
        let scene = if self.scene_open() {
            column::with_children(vec![
                type_specimen(draft.preview(editor.variant), draft),
                self.disclosure(
                    "appearance-window-example",
                    "Window example",
                    sample_window(draft.preview(editor.variant), draft, 180.),
                ),
            ])
            .spacing(12)
            .into()
        } else {
            cosmic::widget::space().height(0).into()
        };
        items.push(
            self.inspector_layout(
                self.scene_frame(scene, "Sample · fonts and icons"),
                column::with_children(vec![
                    self.search_target("appearance-fonts", self.appearance_type(draft)),
                    self.search_target("appearance-colors", self.appearance_colors(draft)),
                    self.appearance_window(draft),
                    self.search_target("appearance-effects", self.appearance_effects(draft)),
                ])
                .spacing(16)
                .into(),
                "fonts",
            ),
        );
        column::with_children(items).spacing(16).into()
    }

    fn color_row(&self, draft: &AppearanceDraft, role: ColorRole) -> Element<'_, Message> {
        let editor = &self.system_appearance;
        let accent = self.appearance_accent();
        let variant = editor.variant;
        let setting = draft.color(variant, role);
        let (current, automatic) = match &setting {
            Setting::Known(Some(value)) => (*value, false),
            // An outline color only the derived theme holds is drawn, so the
            // outline does not follow yet; "Follow accent" relinks it.
            _ => (
                draft.derived_color(variant, role),
                role != ColorRole::Outline || draft.outline_follows(variant),
            ),
        };
        let error = editor.hex_error(role);
        let target = crate::settings::color_well::Target::Role(
            role,
            (current.alpha.clamp(0., 1.) * 255.).round() as u8,
        );
        let mut controls: Vec<Element<'_, Message>> = Vec::new();
        if let Setting::Unsupported(raw) = &setting {
            controls.push(swatch(color(current), 22));
            controls.push(kept(raw));
            return field(
                role.title(),
                row::with_children(controls).spacing(10).into(),
            );
        }
        controls.push(self.color_button(
            format!(
                "theme-role-{}",
                role.title().to_lowercase().replace(' ', "-")
            ),
            target,
            Some([current.red, current.green, current.blue]),
            true,
        ));
        controls.push(
            // COSMIC's "use the accent" for the outline is its derived value.
            button::custom(label(
                if role == ColorRole::Outline {
                    "Follow accent"
                } else {
                    "Auto"
                },
                12.,
                if automatic { FG } else { MUTED },
            ))
            .class(chip_style(automatic, accent))
            .padding([5, 10])
            .on_press(edit(Edit::Color(role, None)))
            .into(),
        );
        if error {
            controls.push(label(
                if role.alpha() {
                    "Use #RRGGBB or #RRGGBBAA"
                } else {
                    "Use #RRGGBB"
                },
                11.,
                ERROR,
            ));
        }
        let mut rows = vec![
            row::with_children(controls)
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
        ];
        if matches!(role, ColorRole::Accent | ColorRole::Outline) {
            let palette = variant.default_builder().build().palette;
            let presets = [
                palette.accent_blue,
                palette.accent_indigo,
                palette.accent_purple,
                palette.accent_pink,
                palette.accent_red,
                palette.accent_orange,
                palette.accent_yellow,
                palette.accent_green,
                palette.accent_warm_grey,
            ];
            rows.push(
                row::with_children(
                    presets
                        .into_iter()
                        .map(|preset| {
                            let selected = !automatic
                                && domain::hex(preset, false) == domain::hex(current, false);
                            button::custom(swatch(color(preset), 16))
                                .class(chip_style(selected, accent))
                                .padding(3)
                                .on_press(edit(Edit::Color(role, Some(preset))))
                                .into()
                        })
                        .collect::<Vec<_>>(),
                )
                .spacing(4)
                .wrap()
                .into(),
            );
        }
        if let Some(panel) = self.color_panel(target, &[]) {
            rows.push(panel);
        }
        field(role.title(), column::with_children(rows).spacing(6).into())
    }

    fn appearance_colors(&self, draft: &AppearanceDraft) -> Element<'_, Message> {
        let accent = self.appearance_accent();
        let editor = &self.system_appearance;
        let mut items = vec![
            label("Mode & colors", 16., FG),
            field(
                "Mode",
                choice(
                    &draft.mode(),
                    &SystemMode::ALL.map(|mode| (mode.title(), mode)),
                    Edit::Mode,
                    accent,
                ),
            ),
        ];
        if editor.variant != draft.active_variant() {
            items.push(label(
                format!(
                    "Editing {} colors. Your desktop remains {} until you change Mode.",
                    editor.variant.title(),
                    draft.active_variant().title()
                ),
                11.,
                MUTED,
            ));
        }
        items.push(separator());
        let roles = [
            ColorRole::Accent,
            ColorRole::Background,
            ColorRole::Primary,
            ColorRole::Secondary,
        ];
        // Existing per-variant drafts stay visible after common accent
        // selection moves to Wallpaper & colors, including unfinished text.
        let edited = roles.iter().any(|role| {
            draft.edited(domain::Control::Color(editor.variant, *role))
                || editor.hex_text(*role).is_some()
        });
        let open = editor.advanced_colors || edited;

        items.push(
            button::text(if open {
                "Advanced theme colors ▾"
            } else {
                "Advanced theme colors ▸"
            })
            .on_press(edit(Edit::AdvancedColors(!open)))
            .into(),
        );
        if open {
            items.push(label(
                "Surface overrides for the theme preview above. Choose the accent in Theme & colors.",
                11.,
                MUTED,
            ));
            for role in roles {
                if role == ColorRole::Accent
                    && !draft.edited(domain::Control::Color(editor.variant, role))
                    && editor.hex_text(role).is_none()
                {
                    continue;
                }
                items.push(
                    container(self.color_row(draft, role))
                        .id(cosmic::widget::Id::new(format!(
                            "appearance-color-{}",
                            role.key()
                        )))
                        .into(),
                );
            }
        }
        section(items)
    }

    fn font_row(&self, draft: &AppearanceDraft, role: FontRole) -> Element<'_, Message> {
        let accent = self.appearance_accent();
        let editor = &self.system_appearance;
        let setting = draft.font(role);
        let Setting::Known(font) = setting else {
            let Setting::Unsupported(raw) = setting else {
                unreachable!()
            };
            return field(role.title(), kept(&raw));
        };
        // Installed in either list counts: fontconfig and font files do not
        // always agree on which families are monospaced.
        let installed = editor
            .fonts
            .family(role.monospace(), &font.family)
            .or_else(|| editor.fonts.family(!role.monospace(), &font.family));
        let open = editor.picker == Some(Picker::Font(role));
        let mut first: Vec<Element<'_, Message>> = vec![
            container(
                button::custom(
                    row::with_children(vec![
                        container(line(font.family.clone(), 13., FG))
                            .width(Length::Fill)
                            .into(),
                        icon::from_name(if open {
                            "go-up-symbolic"
                        } else {
                            "go-down-symbolic"
                        })
                        .size(14)
                        .icon()
                        .into(),
                    ])
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .class(chip_style(open, accent))
                .padding([6, 12])
                .width(Length::Fill)
                .on_press(edit(Edit::Picker((!open).then_some(Picker::Font(role))))),
            )
            .width(Length::Fill)
            .into(),
        ];
        if installed.is_none()
            && !(editor.fonts.interface.is_empty() && editor.fonts.monospace.is_empty())
        {
            first.push(pill("Not installed · kept", WARN));
        }
        let mut rows = vec![
            row::with_children(first)
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
        ];
        let weights = installed
            .map(|family| family.weights.clone())
            .unwrap_or_default();
        if weights.len() > 1 || installed.is_some_and(|family| family.italic) {
            let mut controls = Vec::new();
            if weights.len() > 1 {
                controls.push(segmented(
                    weights
                        .iter()
                        .map(|weight| {
                            (
                                weight_title(*weight).to_string(),
                                *weight == font.weight,
                                Some(edit(Edit::FontWeight(role, *weight))),
                            )
                        })
                        .collect(),
                    accent,
                ));
            }
            if installed.is_some_and(|family| family.italic) {
                let italic = font.style != Style::Normal;
                controls.push(
                    button::custom(label("Italic", 12., if italic { FG } else { MUTED }))
                        .class(chip_style(italic, accent))
                        .padding([5, 10])
                        .on_press(edit(Edit::FontStyle(
                            role,
                            if italic { Style::Normal } else { Style::Italic },
                        )))
                        .into(),
                );
            }
            rows.push(
                row::with_children(controls)
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .wrap()
                    .into(),
            );
        }
        if open {
            let options: Vec<(String, String, bool)> = editor
                .fonts
                .families(role.monospace())
                .iter()
                .filter(|family| catalog::matches(&editor.search, &[&family.name]))
                .map(|family| {
                    (
                        family.name.clone(),
                        family.name.clone(),
                        family.name.eq_ignore_ascii_case(&font.family),
                    )
                })
                .collect();
            rows.push(self.picker_list(
                options,
                if editor.fonts.families(role.monospace()).is_empty() {
                    "No installed fonts were found."
                } else {
                    "No fonts match."
                },
                move |name| edit(Edit::FontFamily(role, name)),
            ));
        }
        field(role.title(), column::with_children(rows).spacing(8).into())
    }

    /// Search field plus a bounded, scrolling list. It is always the last
    /// child of its row, so named controls before it never change position.
    fn picker_list(
        &self,
        options: Vec<(String, String, bool)>,
        empty: &'static str,
        message: impl Fn(String) -> Message + 'static,
    ) -> Element<'_, Message> {
        let accent = self.appearance_accent();
        let editor = &self.system_appearance;
        let mut list: Vec<Element<'_, Message>> = Vec::new();
        let total = options.len();
        for (value, title, selected) in options.into_iter().take(200) {
            list.push(
                button::custom(
                    row::with_children(vec![
                        container(line(title, 13., if selected { FG } else { MUTED }))
                            .width(Length::Fill)
                            .into(),
                        label(if selected { "✓" } else { "" }, 13., accent),
                    ])
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .class(nav_style(selected, accent))
                .padding([5, 10])
                .width(Length::Fill)
                .on_press(message(value))
                .into(),
            );
        }
        if list.is_empty() {
            list.push(label(empty, 12., MUTED));
        } else if total > 200 {
            list.push(label(
                format!("{} more · refine the search", total - 200),
                11.,
                MUTED,
            ));
        }
        container(
            column::with_children(vec![
                text_input("Search", &editor.search)
                    .id(search_id())
                    .on_input(|text| edit(Edit::Search(text)))
                    .width(Length::Fill)
                    .into(),
                container(scrollable(column::with_children(list).spacing(2)).width(Length::Fill))
                    .max_height(200)
                    .into(),
            ])
            .spacing(6),
        )
        .id(picker_id())
        .into()
    }

    fn appearance_type(&self, draft: &AppearanceDraft) -> Element<'_, Message> {
        let accent = self.appearance_accent();
        let editor = &self.system_appearance;
        let icon_setting = draft.icon_theme();
        let icons: Element<'_, Message> = match icon_setting {
            Setting::Known(id) => {
                let theme = editor.icons.get(&id);
                let open = editor.picker == Some(Picker::Icons);
                let name = theme.map_or_else(|| id.clone(), |theme| theme.name.clone());
                let mut first: Vec<Element<'_, Message>> = vec![
                    container(
                        button::custom(
                            row::with_children(vec![
                                container(line(name, 13., FG)).width(Length::Fill).into(),
                                icon::from_name(if open {
                                    "go-up-symbolic"
                                } else {
                                    "go-down-symbolic"
                                })
                                .size(14)
                                .icon()
                                .into(),
                            ])
                            .spacing(8)
                            .align_y(Alignment::Center),
                        )
                        .class(chip_style(open, accent))
                        .padding([6, 12])
                        .width(Length::Fill)
                        .on_press(edit(Edit::Picker((!open).then_some(Picker::Icons)))),
                    )
                    .width(Length::Fill)
                    .into(),
                ];
                if theme.is_none() {
                    first.push(pill("Not installed · kept", WARN));
                }
                let mut rows = vec![
                    row::with_children(first)
                        .spacing(8)
                        .align_y(Alignment::Center)
                        .into(),
                ];
                if !editor.icons.errors.is_empty() {
                    rows.push(label(
                        format!(
                            "Some icon folders could not be read: {}",
                            editor.icons.errors.join("; ")
                        ),
                        11.,
                        WARN,
                    ));
                }
                if open {
                    let options = editor
                        .icons
                        .themes
                        .iter()
                        .filter(|theme| catalog::matches(&editor.search, &[&theme.name, &theme.id]))
                        .map(|theme| {
                            let title = if theme.name == theme.id {
                                theme.name.clone()
                            } else {
                                format!("{} · {}", theme.name, theme.id)
                            };
                            (theme.id.clone(), title, theme.id == id)
                        })
                        .collect();
                    rows.push(self.picker_list(
                        options,
                        if editor.icons.themes.is_empty() {
                            "No icon themes were found."
                        } else {
                            "No icon themes match."
                        },
                        |id| edit(Edit::IconTheme(id)),
                    ));
                }
                column::with_children(rows).spacing(8).into()
            }
            Setting::Unsupported(raw) => kept(&raw),
        };
        section(vec![
            label("Fonts & icons", 16., FG),
            self.font_row(draft, FontRole::Interface),
            self.font_row(draft, FontRole::Monospace),
            field("Icon theme", icons),
            label(
                "COSMIC apps use these. Other toolkits follow only where they read COSMIC settings. Text size follows display scaling.",
                11.,
                MUTED,
            ),
        ])
    }

    fn appearance_window(&self, draft: &AppearanceDraft) -> Element<'_, Message> {
        let accent = self.appearance_accent();
        let variant = self.system_appearance.variant;
        let width = draft.outline_width(variant);
        let mut items = vec![
            label("Window appearance", 16., FG),
            self.color_row(draft, ColorRole::Outline),
        ];
        items.push(match width {
            Setting::Known(value) => {
                let shown = value.min(16);
                let mut column_items = vec![
                    split(
                        label("Outline width", 12., FG),
                        label(
                            if value > 16 {
                                format!("{value} px · above range")
                            } else {
                                format!("{value} px")
                            },
                            11.,
                            accent,
                        ),
                    ),
                    slider(0..=16u32, shown, |value| edit(Edit::OutlineWidth(value)))
                        .on_release(edit(Edit::Apply))
                        .step(1u32)
                        .width(Length::Fill)
                        .apply(|control| completed_slider(control.into(), edit(Edit::Apply))),
                ];
                if let Setting::Known(gap) = draft.inner_gap(variant) {
                    let base = draft
                        .base_builder(variant)
                        .map(|builder| builder.gaps.1)
                        .unwrap_or(gap);
                    if gap != base {
                        column_items.push(label(
                            format!(
                                "Window gap widens from {base} to {gap} px to fit the outline."
                            ),
                            11.,
                            MUTED,
                        ));
                    }
                }
                column::with_children(column_items).spacing(7).into()
            }
            Setting::Unsupported(raw) => field("Outline width", kept(&raw)),
        });
        items.push(separator());
        items.push(field(
            "Roundness",
            choice(
                &draft.roundness(variant),
                &[
                    ("Round", Roundness::Round),
                    ("Slightly round", Roundness::SlightlyRound),
                    ("Square", Roundness::Square),
                ],
                Edit::Roundness,
                accent,
            ),
        ));
        if draft.custom_radii(variant) && !draft.edited(domain::Control::Roundness) {
            items.push(label(
                "Custom corner sizes are kept until you choose one.",
                11.,
                MUTED,
            ));
        }
        items.push(field(
            "Density",
            choice(
                &draft.density(),
                &[
                    ("Compact", Density::Compact),
                    ("Standard", Density::Standard),
                    ("Spacious", Density::Spacious),
                ],
                Edit::Density,
                accent,
            ),
        ));
        items.push(label(
            "Dock & Panel keep their own corners and spacing.",
            11.,
            MUTED,
        ));
        items.push(separator());
        for effect in WindowEffect::ALL {
            items.push(match draft.window(effect) {
                Setting::Known(on) => split(
                    label(effect.title(), 13., FG),
                    toggler(on)
                        .on_toggle(move |on| edit(Edit::Window(effect, on)))
                        .into(),
                ),
                Setting::Unsupported(raw) => split(label(effect.title(), 13., FG), kept(&raw)),
            });
        }
        section(items)
    }

    fn appearance_effects(&self, draft: &AppearanceDraft) -> Element<'_, Message> {
        let accent = self.appearance_accent();
        let editor = &self.system_appearance;
        let open = editor.effects;
        let toggle = button::custom(
            row::with_children(vec![
                label("Effects", 16., FG),
                container(text::body("")).width(Length::Fill).into(),
                label("Frosted surfaces and blur", 11., MUTED),
                icon::from_name(if open {
                    "go-up-symbolic"
                } else {
                    "go-down-symbolic"
                })
                .size(16)
                .icon()
                .into(),
            ])
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .class(nav_style(false, accent))
        .padding([4, 4])
        .width(Length::Fill)
        .on_press(edit(Edit::Effects(!open)));
        if !open {
            return section(vec![toggle.into()]);
        }
        let variant = editor.variant;
        let mut items: Vec<Element<'_, Message>> =
            vec![toggle.into(), label("FROSTED", 10., MUTED)];
        for surface in Surface::ALL {
            items.push(match draft.frosted(variant, surface) {
                // COSMIC panel 1.10 aborts when its frosted surface is
                // unmapped on the current compositor (screen blank, display
                // change), taking every applet with it. It can be turned off
                // here, never on, until a fixed panel is installed.
                Setting::Known(false) if surface == Surface::Panel => split(
                    detail(
                        surface.title(),
                        "Off: the installed COSMIC panel crashes with a frosted background",
                    ),
                    toggler(false).into(),
                ),
                Setting::Known(on) => split(
                    label(surface.title(), 13., FG),
                    toggler(on)
                        .on_toggle(move |on| edit(Edit::Frosted(surface, on)))
                        .into(),
                ),
                Setting::Unsupported(raw) => split(label(surface.title(), 13., FG), kept(&raw)),
            });
        }
        let any = Surface::ALL
            .into_iter()
            .any(|surface| draft.frosted(variant, surface).known() == Some(&true));
        items.push(separator());
        match draft.blur(variant) {
            Setting::Known(strength) => {
                let step = strength as u8;
                items.push(
                    column::with_children(vec![
                        split(
                            label("Blur strength", 12., FG),
                            label(format!("{} of 13", step), 11., accent),
                        ),
                        slider(0..=13u8, step, |value| edit(Edit::Blur(value)))
                            .on_release(edit(Edit::Apply))
                            .step(1u8)
                            .width(Length::Fill)
                            .apply(|control| completed_slider(control.into(), edit(Edit::Apply))),
                    ])
                    .spacing(7)
                    .into(),
                );
            }
            Setting::Unsupported(raw) => {
                items.push(split(label("Blur strength", 12., FG), kept(&raw)));
            }
        }
        match draft.glass(variant) {
            Setting::Known(glass) => {
                let glass = glass.min(100) as u8;
                items.push(
                    column::with_children(vec![
                        split(
                            label("Glass opacity", 12., FG),
                            label(format!("{glass}%"), 11., accent),
                        ),
                        slider(0..=100u8, glass, |value| edit(Edit::Glass(value / 5 * 5)))
                            .on_release(edit(Edit::Apply))
                            .step(5u8)
                            .width(Length::Fill)
                            .apply(|control| completed_slider(control.into(), edit(Edit::Apply))),
                    ])
                    .spacing(7)
                    .into(),
                );
            }
            Setting::Unsupported(raw) => {
                items.push(split(label("Glass opacity", 12., FG), kept(&raw)));
            }
        }
        items.push(label(
            if any {
                "Applies to both themes. Blur and glass affect frosted surfaces only."
            } else {
                "Applies to both themes. Turn on a frosted surface to use blur and glass."
            },
            11.,
            MUTED,
        ));
        section(items)
    }
}

/// A small window drawn from the draft: title bar inside the active outline,
/// a primary sidebar, a secondary card with text, a button and the accent.
/// At its smallest it keeps one sidebar item fewer and only the accent
/// button rather than squeezing every label.
fn type_specimen<'a>(preview: Preview, draft: &AppearanceDraft) -> Element<'a, Message> {
    let font = |role| match draft.font(role) {
        Setting::Known(font) => cosmic::iced::Font::from(font),
        Setting::Unsupported(_) => cosmic::iced::Font::default(),
    };
    let line = |value: &'static str, size, role| -> Element<'a, Message> {
        text::body(value)
            .size(size)
            .font(font(role))
            .class(cosmic::theme::Text::Color(color(preview.on_background)))
            .into()
    };
    let mut rows = vec![
        line("Make yourself at home", 24., FontRole::Interface),
        line(
            "The quick brown fox jumps over the lazy dog.",
            15.,
            FontRole::Interface,
        ),
        line("0123456789 · 09:41", 19., FontRole::Interface),
        line("Aa Bb Cc  •  {} [] /", 14., FontRole::Monospace),
    ];
    if let Setting::Known(theme) = draft.icon_theme() {
        let icons: Vec<Element<'a, Message>> =
            ["folder", "text-x-generic", "image-x-generic", "user-trash"]
                .into_iter()
                .filter_map(|name| {
                    freedesktop_icons::lookup(name)
                        .with_theme(&theme)
                        .with_size(32)
                        .with_cache()
                        .find()
                })
                .map(|path| icon::from_path(path).icon().size(32).into())
                .collect();
        if icons.is_empty() {
            rows.push(label("Icon preview unavailable", 12., MUTED));
        } else {
            rows.push(row::with_children(icons).spacing(16).wrap().into());
        }
    }
    fill(
        column::with_children(rows).spacing(14).into(),
        color(preview.background),
        Color::TRANSPARENT,
        0.,
        12.,
    )
    .padding(16)
    .width(Length::Fill)
    .into()
}

fn sample_window<'a>(
    preview: Preview,
    draft: &AppearanceDraft,
    height: f32,
) -> Element<'a, Message> {
    let font = |role: FontRole| match draft.font(role) {
        Setting::Known(font) => cosmic::iced::Font::from(font),
        Setting::Unsupported(_) => cosmic::iced::Font::default(),
    };
    let compact = height < 125.;
    let text_in =
        |value: &'static str, size: f32, tone: Srgba, role: FontRole| -> Element<'a, Message> {
            let mut text = text::body(value)
                .size(size)
                .font(font(role))
                .wrapping(cosmic::iced::advanced::text::Wrapping::None)
                .class(cosmic::theme::Text::Color(color(tone)));
            if compact {
                // Lines sized to the small text, not the body text.
                text = text.line_height(cosmic::iced::widget::text::LineHeight::Relative(1.2));
            }
            text.into()
        };
    let radius_m = preview.radius_m.min(height / 6.);
    let radius_s = preview.radius_s.min(height / 10.);
    let dot = |tone: Srgba| -> Element<'a, Message> {
        fill(
            cosmic::iced::widget::space().width(8).height(8).into(),
            color(tone),
            Color::TRANSPARENT,
            0.,
            4.,
        )
        .into()
    };
    let title_bar = row::with_children(vec![
        text_in("Files", 11., preview.on_background, FontRole::Interface),
        container(text::body("")).width(Length::Fill).into(),
        dot(Srgba {
            alpha: 0.5,
            ..preview.on_background
        }),
        dot(Srgba {
            alpha: 0.5,
            ..preview.on_background
        }),
        dot(preview.accent),
    ])
    .spacing(5)
    .align_y(Alignment::Center);
    let mut places = vec![
        fill(
            text_in("Home", 10., preview.on_accent, FontRole::Interface),
            color(preview.accent),
            Color::TRANSPARENT,
            0.,
            radius_s,
        )
        .padding([2, 6])
        .width(Length::Fill)
        .into(),
        text_in("Music", 10., preview.on_primary, FontRole::Interface),
    ];
    if !compact {
        places.push(text_in(
            "Notes",
            10.,
            preview.on_primary,
            FontRole::Interface,
        ));
    }
    let sidebar = fill(
        column::with_children(places).spacing(4).into(),
        color(preview.primary),
        Color::TRANSPARENT,
        0.,
        radius_s,
    )
    .padding(6)
    .width(Length::FillPortion(2))
    .height(Length::Fill);
    let mut buttons: Vec<Element<'a, Message>> = Vec::new();
    if !compact {
        buttons.push(
            fill(
                text_in("Cancel", 10., preview.on_button, FontRole::Interface),
                color(preview.button),
                Color::TRANSPARENT,
                0.,
                radius_s,
            )
            .padding([2, 8])
            .into(),
        );
    }
    buttons.push(
        fill(
            text_in("Save", 10., preview.on_accent, FontRole::Interface),
            color(preview.accent),
            Color::TRANSPARENT,
            0.,
            radius_s,
        )
        .padding([2, 8])
        .into(),
    );
    let card = fill(
        column::with_children(vec![
            text_in(
                "Aa Sample text",
                11.,
                preview.on_secondary,
                FontRole::Interface,
            ),
            text_in("fn main()", 10., preview.on_secondary, FontRole::Monospace),
            cosmic::iced::widget::space().height(Length::Fill).into(),
            row::with_children(buttons).spacing(6).into(),
        ])
        .spacing(3)
        .into(),
        color(preview.secondary),
        Color::TRANSPARENT,
        0.,
        radius_s,
    )
    .padding(6)
    .width(Length::FillPortion(3))
    .height(Length::Fill);
    let outline = (preview.outline_width as f32).clamp(0., 6.);
    fill(
        column::with_children(vec![
            title_bar.into(),
            row::with_children(vec![sidebar.into(), card.into()])
                .spacing(6)
                .height(Length::Fill)
                .into(),
        ])
        .spacing(6)
        .into(),
        color(preview.background),
        color(preview.outline),
        outline,
        radius_m,
    )
    .padding(8)
    .width(Length::Fill)
    .height(height)
    .into()
}
