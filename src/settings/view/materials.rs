//! One material chooser, the same wallpaper and colors in every specimen.
use super::*;
use crate::materials::{Finish, Kind, Target};
/// Widgets may become nearly bare glass; the dock and panel stop where
/// the installed panel does.
fn see_through_limit(target: Target) -> u8 {
    use dock_islands_core::material::{BAR_MIN_OPACITY, MIN_OPACITY};
    100 - match target {
        Target::Dock | Target::Panel => BAR_MIN_OPACITY,
        Target::Shared | Target::Widgets => MIN_OPACITY,
    }
}
impl Settings {
    pub(super) fn appearance_page(&self) -> Element<'_, Message> {
        let target = self.material_target;
        let selected = self
            .material_draft
            .or(self.config.materials.effective(target));
        let accent = self.settings_accent();
        let tile = |specimen, title, chosen, id, message| {
            material_tile(specimen, title, chosen, id, message, accent)
        };
        // "No glass" is a choice like the others, so the current state is
        // always one marked tile. For one part, it means "follow the rest".
        let own = self.material_draft.or(self.config.materials.get(target));
        let mut tiles: Vec<Element<'_, Message>> = vec![tile(
            self.material_specimen(None),
            if target == Target::Shared { "No glass" } else { "Same as the rest" },
            own.is_none(),
            "material-none".to_owned(),
            Message::MaterialChoose(None),
        )];
        tiles.extend(Kind::ALL.into_iter().map(|kind| {
            let chosen = own.is_some_and(|f| f.kind == kind);
            tile(
                self.material_specimen(Some(
                    selected
                        .filter(|f| f.kind == kind)
                        .unwrap_or_else(|| Finish::new(kind)),
                )),
                kind.title(),
                chosen,
                format!("material-{}", kind.key()),
                Message::MaterialChoose(Some(kind)),
            )
        }));
        // Five across when there is room, otherwise rows of three with
        // equal widths.
        let per_row = if self.content_width() >= 900. { 5 } else { 3 };
        let mut lines = Vec::new();
        let mut tiles = tiles.into_iter().peekable();
        while tiles.peek().is_some() {
            let mut line: Vec<Element<'_, Message>> = tiles.by_ref().take(per_row).collect();
            while line.len() < per_row {
                line.push(cosmic::iced::widget::space().width(Length::Fill).into());
            }
            lines.push(row::with_children(line).spacing(12).width(Length::Fill).into());
        }
        let chooser = column::with_children(lines).spacing(12);
        let scopes = segmented(
            Target::ALL
                .into_iter()
                .filter(|t| {
                    self.desktop.islands.installation.installed()
                        || self.config.materials.get(*t).is_some()
                        || *t == target
                        || matches!(t, Target::Shared | Target::Widgets)
                })
                .map(|t| {
                    (
                        match t {
                            Target::Shared => "Widgets, dock and panel",
                            Target::Widgets => "Widgets only",
                            Target::Dock => "Dock only",
                            Target::Panel => "Panel only",
                        }
                        .to_owned(),
                        target == t,
                        Some(Message::MaterialTarget(t)),
                    )
                })
                .collect(),
            self.settings_accent(),
        );
        let state = if self.config.materials.get(target).is_some() {
            if target == Target::Shared {
                if [
                    self.config.materials.widgets,
                    self.config.materials.dock,
                    self.config.materials.panel,
                ]
                .into_iter()
                .any(|f| f.is_some())
                {
                    "Some parts have their own choice"
                } else {
                    "Widgets, dock and panel look the same"
                }
            } else {
                "Has its own choice"
            }
        } else if target == Target::Shared {
            ""
        } else {
            "Same as everything else"
        };
        // Without the panel add-on, glass reaches the widgets only, so a
        // choice of where to use it would offer nothing real.
        let scoped = self.desktop.islands.installation.installed()
            || target != Target::Shared
            || self.config.materials.dock.is_some()
            || self.config.materials.panel.is_some()
            || self.config.materials.widgets.is_some();
        let mut rows = Vec::new();
        if scoped {
            rows.push(
                container(group(None, vec![group_row("Use on", state, scopes)]))
                    .id(cosmic::widget::Id::new("material-scope"))
                    .width(Length::Fill)
                    .into(),
            );
        }
        // Each choice is one large material sample. No miniature desktop
        // or separate preview column competes with the comparison.
        rows.push(chooser.into());
        if let Some(finish) = selected {
            rows.push(group(
                Some("Fine-tune"),
                vec![
                    self.material_slider("See-through", format!("{}%", 100 - finish.opacity()), 0..=see_through_limit(target), 100 - finish.opacity(), true),
                    self.material_slider("Color tint", format!("{}%", finish.tint), 0..=100, finish.tint, false),
                    self.search_target("contrast-assist", split(
                        detail("Easier to read", "Adds a little more background behind small text on busy wallpapers"),
                        container(toggler(self.config.appearance.contrast_assist).on_toggle(Message::ContrastAssist)).id(cosmic::widget::Id::new("material-readability")).into(),
                    )),
                    container(split(
                        detail("Liquid motion", "Liquid glass moves gently"),
                        toggler(self.config.material_motion).on_toggle(Message::MaterialMotion).into(),
                    ))
                    .id(cosmic::widget::Id::new("material-motion"))
                    .width(Length::Fill)
                    .into(),
                    group_row(
                        if target == Target::Shared { "Start over" } else { "Same as everything else" },
                        if target == Target::Shared { "Go back to your style’s own look" } else { "Drop this part’s own choice" },
                        container(
                            button::text(if target == Target::Shared { "Reset" } else { "Use shared" })
                                .on_press_maybe(
                                    self.config
                                        .materials
                                        .get(target)
                                        .map(|_| Message::MaterialChoose(None)),
                                ),
                        )
                        .id(cosmic::widget::Id::new("material-reset"))
                        .into(),
                    ),
                ],
            ));
        }
        if matches!(target, Target::Dock | Target::Panel)
            && let Some(status) = self.desktop.islands.availability_message()
        {
            rows.push(desktop::banner(
                status,
                desktop::WARN,
                vec![
                    button::text("Set up")
                        .on_press(Message::OpenIslandsGuide)
                        .into(),
                    button::text("Check again")
                        .on_press(Message::DesktopEditor(
                            crate::settings::desktop::Message::Islands(
                                crate::settings::desktop::islands::Edit::Check,
                            ),
                        ))
                        .into(),
                ],
            ));
        }
        // The older widget finish only matters until a glass style is chosen.
        if self.config.materials.effective(Target::Widgets).is_none() {
            rows.push(
                self.page_settings_advanced(Page::Appearance, vec![self.legacy_widget_style()]),
            );
        }
        let controls = column::with_children(rows).spacing(12).into();
        column::with_children(vec![
            heading(
                "Glass and transparency",
                // Without the panel add-on, glass reaches the widgets only.
                if self.desktop.islands.installation.installed() {
                    "How see-through widgets, the dock and the panel are."
                } else {
                    "How see-through your widgets are."
                },
            ),
            controls,
        ])
        .spacing(16)
        .width(Length::Fill)
        .into()
    }
    pub(super) fn material_slider(
        &self,
        title: &'static str,
        value: String,
        range: std::ops::RangeInclusive<u8>,
        current: u8,
        opacity: bool,
    ) -> Element<'_, Message> {
        column::with_children(vec![
            split(
                label(title, 12., FG),
                label(value, 11., self.settings_accent()),
            ),
            container(
                slider(range, current, move |v| {
                    Message::MaterialValue(opacity, if opacity { 100 - v } else { v })
                })
                .on_release(Message::MaterialSave)
                .width(Length::Fill)
                .apply(|v| completed_slider(v.into(), Message::MaterialSave)),
            )
            .id(cosmic::widget::Id::new(if opacity {
                "material-transparency"
            } else {
                "material-tint"
            }))
            .into(),
        ])
        .spacing(5)
        .into()
    }
    fn material_specimen(&self, finish: Option<Finish>) -> Element<'_, Message> {
        let mut config = self.config.clone();
        config.materials.shared = finish;
        config.materials.widgets = None;
        let p = config.palette();
        let p = if p.background.a == 0. {
            crate::materials::palette(config.surface_palette(), finish, false)
        } else {
            p
        };
        // The desktop blurs what is behind glass; a sample can't, so a haze
        // stands in for the blur and makes each finish recognizable.
        // Each finish is drawn a little stronger than life so the five
        // read apart at a glance; the sliders still move them.
        let mut tone = p.background;
        let (toward, amount, alpha) = match finish.map(|f| f.kind) {
            Some(Kind::Frosted) => (1., 0.32, 1.),
            Some(Kind::Clear) => (1., 0.04, 0.55),
            Some(Kind::Smoked) => (0., 0.55, 0.95),
            Some(Kind::Liquid) => (1., 0.10, 0.75),
            // No glass is a solid card.
            None => (0., 0., 1.),
        };
        tone.r += (toward - tone.r) * amount;
        tone.g += (toward - tone.g) * amount;
        tone.b += (toward - tone.b) * amount;
        tone.a = if finish.is_some() { (tone.a * alpha).clamp(0.25, 1.) } else { 1. };
        let bg = crate::materials::background(tone, finish);
        // Clear and Liquid catch the light at the edge; Liquid most.
        let (rim, rim_width) = match finish.map(|f| f.kind) {
            Some(Kind::Liquid) => (0.65, 1.5),
            Some(Kind::Clear) => (0.4, 1.),
            _ => (0.15, 1.),
        };
        let surface = move |_: &cosmic::Theme| cosmic::iced::widget::container::Style {
            background: Some(bg),
            border: Border {
                color: Color::from_rgba(1., 1., 1., rim),
                width: rim_width,
                radius: 9.into(),
            },
            ..Default::default()
        };
        // The sample weather follows the chosen units, like every preview.
        let units = self.config.weather;
        let deg = |c: f64| format!("{:.0}°", units.degrees(c).round());
        let (value, subtitle) = match self.appearance_preview {
            Widget::Clock => ("09:41".to_string(), "Wednesday, October 21".to_string()),
            Widget::Devbox => ("26%".to_string(), "Processor · Memory 8.2 / 32 GB".to_string()),
            _ => (deg(21.), format!("Partly cloudy · High {}", deg(23.))),
        };
        let card = container(
            column::with_children(vec![
                label(value, 32., p.foreground),
                label(subtitle, 11., p.foreground),
            ])
            .spacing(4),
        )
        .width(Length::Fill)
        .height(96)
        .align_y(Alignment::Center)
        .padding([8, 16])
        .class(cosmic::theme::Container::custom(surface));
        // Bold stripes run behind every sample, so how much shows through
        // each finish is plain to see; a smooth picture would hide it.
        let stripes = [
            Color::from_rgb(0.96, 0.62, 0.40),
            Color::from_rgb(0.16, 0.22, 0.48),
            Color::from_rgb(0.99, 0.86, 0.55),
            Color::from_rgb(0.46, 0.30, 0.62),
            Color::from_rgb(0.30, 0.68, 0.82),
            Color::from_rgb(0.10, 0.12, 0.22),
            Color::from_rgb(0.92, 0.45, 0.52),
        ];
        let last = stripes.len() - 1;
        let backdrop = row::with_children(
            stripes
                .into_iter()
                .enumerate()
                .map(|(index, color)| {
                    // Round the outer stripes, since children aren't clipped
                    // to the frame's corners.
                    let radius = match index {
                        0 => cosmic::iced::border::Radius::from(10.).right(0.),
                        i if i == last => cosmic::iced::border::Radius::from(10.).left(0.),
                        _ => 0.into(),
                    };
                    container(cosmic::iced::widget::space())
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .class(cosmic::theme::Container::custom(move |_| {
                            cosmic::iced::widget::container::Style {
                                background: Some(Background::Color(color)),
                                border: Border {
                                    radius,
                                    ..Default::default()
                                },
                                ..Default::default()
                            }
                        }))
                        .into()
                })
                .collect::<Vec<_>>(),
        )
        .width(Length::Fill)
        .height(Length::Fill);
        container(
            cosmic::iced::widget::Stack::new()
                .push(container(backdrop).width(Length::Fill).height(Length::Fill))
                .push(container(card).padding(12).width(Length::Fill)),
        )
        .width(Length::Fill)
        .height(120)
        .clip(true)
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
    }
}

fn material_tile<'a>(
    specimen: Element<'a, Message>,
    title: &'static str,
    chosen: bool,
    id: String,
    message: Message,
    accent: Color,
) -> Element<'a, Message> {
    container(
        button::custom(
            column::with_children(vec![
                specimen,
                container(split(
                    label(title, 14., FG),
                    label(if chosen { "✓" } else { "" }, 14., accent),
                ))
                .padding([2, 6])
                .into(),
            ])
            .spacing(5),
        )
        .class(profiles::tile_style(chosen, accent))
        .selected(chosen)
        .padding(7)
        .width(Length::Fill)
        .on_press(message),
    )
    .id(cosmic::widget::Id::new(id))
    .width(Length::Fill)
    .into()
}
