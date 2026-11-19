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
    pub(in crate::settings) fn finish_summary(&self, finish: Option<Finish>) -> String {
        let mut config = self.config.clone();
        config.materials.shared = finish;
        config.materials.widgets = None;
        let name = finish.map(|value| value.kind.title()).unwrap_or_else(|| {
            config.appearance.family.map_or("Style default", |family| family.title())
        });
        let opacity = (100. * config.palette().background.a).round() as u8;
        format!("{name}, {}% see-through", 100 - opacity)
    }

    pub(super) fn appearance_page(&self) -> Element<'_, Message> {
        let target = self.material_target;
        let selected = self
            .material_draft
            .or(self.config.materials.effective(target));
        let accent = self.settings_accent();
        let tile = |specimen, title, chosen, id, message| {
            material_tile(specimen, title, chosen, id, message, accent)
        };
        // The original style is a choice like the others, so the current state is
        // always one marked tile. For one part, it means "follow the rest".
        let own = self.material_draft.or(self.config.materials.get(target));
        let mut tiles: Vec<Element<'_, Message>> = vec![tile(
            self.material_specimen(if target == Target::Shared { None } else { self.config.materials.shared }),
            if target == Target::Shared { "Style default" } else { "Same as the rest" },
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
        let mut rows = vec![group(None, vec![
            field(
                Some("Easier to read".into()),
                Some(self.config.appearance.contrast_assist),
                detail_state("Easier to read", "Strengthens the background behind widget text."),
                container(self.search_target(
                    "contrast-assist",
                    container(toggler(self.config.appearance.contrast_assist).on_toggle(Message::ContrastAssist))
                        .id("material-readability").into(),
                )).width(52).into(),
            ),
        ])];
        if matches!(target, Target::Shared | Target::Widgets) {
            rows.push(group(
                None,
                vec![group_row_state(
                    "Widget glass",
                    "Glass finish, backdrop blur, tint and corners",
                    button::text("Edit glass")
                        .on_press(Message::SearchResult(
                            crate::settings::navigation::WIDGET_GLASS,
                        ))
                        .into(),
                )],
            ));
        }
        if scoped {
            rows.push(
                container(group(None, vec![group_row_state("Use on", state, scopes)]))
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
                    button::text("Islands panel setup")
                        .id("material-panel-setup".into())
                        .on_press(Message::SearchResult(crate::settings::navigation::ISLANDS_PANEL))
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
        // Widget cards' own corners; the older widget finish only matters
        // until a glass style is chosen.
        let mut advanced = vec![self.widget_corners()];
        if self.config.materials.effective(Target::Widgets).is_none() {
            advanced.push(self.legacy_widget_style());
        }
        rows.push(self.page_settings_advanced(Page::Appearance, advanced));
        // The dock’s and the windows’ own transparency live on their pages;
        // these rows say so and open each control.
        let elsewhere = |title: &'static str, place: &'static str, id: &'static str, hit| {
            group_row_state(
                title,
                place,
                container(button::text("Open").on_press(Message::SearchResult(hit)))
                    .id(cosmic::widget::Id::new(id))
                    .into(),
            )
        };
        rows.push(group(
            Some("Other surfaces"),
            vec![
                elsewhere(
                    "Dock and panel",
                    "Dock › See-through",
                    "material-open-dock",
                    crate::settings::navigation::DOCK_SEE_THROUGH,
                ),
                elsewhere(
                    "Windows",
                    "Windows › Frosted surfaces and blur",
                    "material-open-windows",
                    crate::settings::navigation::WINDOW_BLUR,
                ),
            ],
        ));
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
    /// Widget cards' corners alone, in the steps Appearance's Corners uses.
    /// A size set some other way is kept and shown as Custom.
    fn widget_corners(&self) -> Element<'_, Message> {
        use crate::settings::corners::Step;
        let radius = self.config.appearance.corner_radius;
        let mut steps: Vec<(String, bool, Option<Message>)> = Step::ALL
            .into_iter()
            .map(|step| {
                (
                    step.title().to_string(),
                    radius == step.widget(),
                    Some(Message::WidgetCorners(step.widget())),
                )
            })
            .collect();
        if !Step::ALL.iter().any(|step| step.widget() == radius) {
            steps.push((format!("Custom · {radius} px"), true, None));
        }
        container(group_row(
            "Widget corners",
            "",
            row::with_children(vec![segmented(steps, self.settings_accent())])
                .wrap()
                .into(),
        ))
        .id(cosmic::widget::Id::new("widget-corners"))
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
        // The sample weather follows the chosen units, like every preview.
        let units = self.config.weather;
        let deg = |c: f64| format!("{:.0}°", units.degrees(c).round());
        let (value, subtitle) = match self.appearance_preview {
            Widget::Clock => ("09:41".to_string(), "Wednesday, October 21".to_string()),
            Widget::Devbox => ("26%".to_string(), "Processor · Memory 8.2 / 32 GB".to_string()),
            _ => (deg(21.), format!("Partly cloudy · High {}", deg(23.))),
        };
        let card = crate::tools::cards::frame(
            column::with_children(vec![
                label(value, 28., p.foreground),
                label(subtitle, 12., p.muted),
            ]).spacing(3).into(),
            &config,
            96.,
        );
        self.preview_stage_padded(card, 12)
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
