//! One material chooser, the same wallpaper and colors in every specimen.
use super::*;
use crate::materials::{Finish, Kind, Target};
impl Settings {
    pub(super) fn appearance_page(&self) -> Element<'_, Message> {
        let target = self.material_target;
        let selected = self
            .material_draft
            .or(self.config.materials.effective(target));
        let tiles = Kind::ALL
            .into_iter()
            .map(|kind| {
                let chosen = selected.is_some_and(|f| f.kind == kind);
                container(
                    button::custom(
                        column::with_children(vec![
                            super::sample::fit(
                                self.material_specimen(
                                    selected
                                        .filter(|f| f.kind == kind)
                                        .unwrap_or_else(|| Finish::new(kind)),
                                ),
                                52.,
                            ),
                            split(
                                label(kind.title(), 14., FG),
                                label(if chosen { "✓" } else { "" }, 14., self.settings_accent()),
                            ),
                        ])
                        .spacing(5),
                    )
                    .class(profiles::tile_style(chosen, self.settings_accent()))
                    .selected(chosen)
                    .padding(7)
                    .width(Length::Fill)
                    .on_press(Message::MaterialChoose(Some(kind))),
                )
                .id(cosmic::widget::Id::new(format!("material-{}", kind.key())))
                .width(Length::Fill)
                .into()
            })
            .collect::<Vec<_>>();
        let mut tiles = tiles.into_iter();
        let chooser = column::with_children(vec![
            adaptive::columns(
                tiles.next().unwrap(),
                tiles.next().unwrap(),
                [132., 132.],
                false,
            ),
            adaptive::columns(
                tiles.next().unwrap(),
                tiles.next().unwrap(),
                [132., 132.],
                false,
            ),
        ])
        .spacing(12);
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
                        t.title().to_owned(),
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
                    "Shared material · some surfaces use their own"
                } else {
                    "Shared by widgets, dock and panel"
                }
            } else {
                "Uses its own material"
            }
        } else if target == Target::Shared {
            "Your existing style is kept"
        } else {
            "Follows the shared material"
        };
        let scene = if self.scene_open() {
            self.preview_stage(
                container(
                    container(super::sample::fit(
                        self.card_preview(),
                        if self.window.height < 760. {
                            110.
                        } else {
                            180.
                        },
                    ))
                    .width(Length::Fill)
                    .max_width(360),
                )
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .into(),
            )
        } else {
            cosmic::widget::space().height(0).into()
        };
        let preview_choice = row::with_children(vec![
            label("Preview widget", 12., MUTED),
            segmented(
                [Widget::Weather, Widget::Devbox, Widget::Clock]
                    .into_iter()
                    .map(|w| {
                        (
                            w.title().into(),
                            self.appearance_preview == w,
                            Some(Message::AppearancePreview(w)),
                        )
                    })
                    .collect(),
                self.settings_accent(),
            ),
        ])
        .spacing(12)
        .align_y(Alignment::Center)
        .wrap()
        .into();
        let mut rows = vec![
            column::with_children(vec![
                container(split(label("Applies to", 14., FG), scopes))
                    .id(cosmic::widget::Id::new("material-scope"))
                    .width(Length::Fill)
                    .into(),
                label(state, 13., MUTED),
            ])
            .spacing(8)
            .into(),
            // Choosing a finish is the primary control, not part of the
            // specimen. Keep its two-column grid in the wider control area.
            chooser.into(),
            split(
                label(
                    if self.material_draft.is_some() {
                        "Adjusting selected surface"
                    } else {
                        "Customize the selected surface"
                    },
                    12.,
                    MUTED,
                ),
                row::with_children(vec![
                    container(
                        button::text(if self.material_adjust {
                            "Done"
                        } else {
                            "Adjust"
                        })
                        .on_press_maybe(
                            selected.map(|_| Message::MaterialAdjust(!self.material_adjust)),
                        ),
                    )
                    .id(cosmic::widget::Id::new("material-adjust"))
                    .into(),
                    container(
                        button::text(if target == Target::Shared {
                            "Reset"
                        } else {
                            "Use shared"
                        })
                        .on_press_maybe(
                            self.config
                                .materials
                                .get(target)
                                .map(|_| Message::MaterialChoose(None)),
                        ),
                    )
                    .id(cosmic::widget::Id::new("material-reset"))
                    .into(),
                ])
                .spacing(4)
                .into(),
            ),
        ];
        if self.material_adjust
            && let Some(finish) = selected
        {
            rows.push(pages::section(vec![
                self.material_slider("Transparency", format!("{}%", 100 - finish.opacity()), 0..=55, 100 - finish.opacity(), true),
                self.material_slider("Color tint", format!("{}%", finish.tint), 0..=100, finish.tint, false),
                container(split(label("Liquid widget motion", 12., FG), toggler(self.config.material_motion).on_toggle(Message::MaterialMotion).into()))
                    .id(cosmic::widget::Id::new("material-motion"))
                    .width(Length::Fill)
                    .into(),
                self.search_target("contrast-assist", split(self.help("material-readability", "Widget readability", "Adds enough opacity for small text on bright or busy wallpaper. Your transparency setting is kept."),
                    container(toggler(self.config.appearance.contrast_assist).on_toggle(Message::ContrastAssist)).id(cosmic::widget::Id::new("material-readability")).into())),
            ]));
        }
        rows.push(label(
            if self.material_native {
                "Decorative widget glass · bar rendering follows the running companion."
            } else {
                if self.desktop.islands.installation.installed() {
                    "Decorative widget glass. Dock & panel finishes need a running capable host."
                } else {
                    "Decorative widget glass. Stock dock & panel follow COSMIC appearance."
                }
            },
            11.,
            MUTED,
        ));
        if matches!(target, Target::Dock | Target::Panel)
            && let Some(status) = self.desktop.islands.availability_message()
        {
            rows.push(desktop::banner(
                status,
                desktop::WARN,
                vec![
                    button::text("Setup & updates")
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
        rows.push(self.page_settings_advanced(Page::Appearance, vec![self.legacy_widget_style()]));
        let controls = column::with_children(rows).spacing(12).into();
        column::with_children(vec![
            heading("Surface finishes", ""),
            self.inspector_layout(
                column::with_children(vec![
                    self.scene_frame(scene, "Decorative finish"),
                    preview_choice,
                ])
                .spacing(16)
                .into(),
                controls,
                "finish",
            ),
        ])
        .spacing(16)
        .width(Length::Fill)
        .into()
    }
    fn material_slider(
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
    fn material_specimen(&self, finish: Finish) -> Element<'_, Message> {
        let mut config = self.config.clone();
        config.materials.shared = Some(finish);
        config.materials.widgets = None;
        let p = config.palette();
        let p = if p.background.a == 0. {
            crate::materials::palette(config.surface_palette(), Some(finish), false)
        } else {
            p
        };
        let bg = crate::materials::background(p.background, Some(finish));
        let surface = move |_: &cosmic::Theme| cosmic::iced::widget::container::Style {
            background: Some(bg),
            border: Border {
                color: Color::from_rgba(
                    1.,
                    1.,
                    1.,
                    if finish.kind == Kind::Liquid {
                        0.35
                    } else {
                        0.15
                    },
                ),
                width: 1.,
                radius: 9.into(),
            },
            ..Default::default()
        };
        let bar = |width| {
            container(cosmic::iced::widget::space().width(width).height(5))
                .padding([2, 5])
                .class(cosmic::theme::Container::custom(surface))
        };
        let card = container(
            column::with_children(vec![
                label("09:41", 19., p.foreground),
                label("Wed 21 · 18°", 9., p.foreground),
            ])
            .spacing(3),
        )
        .padding([6, 9])
        .class(cosmic::theme::Container::custom(surface));
        if self.material_adjust {
            let compact = container(label("09:41", 19., p.foreground))
                .padding([6, 9])
                .class(cosmic::theme::Container::custom(surface));
            return self.preview_stage_padded(compact.into(), 6);
        }
        let contents = column::with_children(vec![
            bar(Length::Fill).into(),
            card.into(),
            container(bar(Length::Fixed(28.)))
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .into(),
        ])
        .spacing(5);
        // Every sample uses the same cached wallpaper, never the style's bundled one.
        self.preview_stage_padded(contents.into(), 6)
    }
}
