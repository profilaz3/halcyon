use super::*;
use crate::recipes::{Arrangement, Kind, Part};
use crate::settings::recipes::Message as Edit;
fn edit(message: Edit) -> Message {
    Message::Recipe(message)
}
impl Settings {
    pub(super) fn recipe_page(&self) -> Element<'_, Message> {
        let editor = &self.recipe_editor;
        let recipe = &editor.draft;
        let busy = self.native_busy;
        let mut content = vec![heading("Widget builder", "")];
        if let Some(notice) = &editor.notice {
            content.push(label(
                notice,
                13.,
                if editor.error {
                    Color::from_rgb(1., 0.45, 0.4)
                } else {
                    MUTED
                },
            ));
        }
        if !self.design.managed {
            content.push(self.design_section());
        }
        let mut library = vec![
            row::with_children(
                Kind::ALL
                    .into_iter()
                    .map(|kind| {
                        button::text(format!("New {}", kind.title()))
                            .on_press_maybe((!busy).then(|| edit(Edit::New(kind))))
                            .into()
                    })
                    .collect::<Vec<_>>(),
            )
            .spacing(8)
            .wrap()
            .into(),
        ];
        if !editor.library_labels.is_empty() {
            let ids = editor.library_ids.clone();
            library.push(
                cosmic::widget::dropdown(
                    &editor.library_labels,
                    editor.library_ids.iter().position(|id| id == &recipe.id),
                    move |i| edit(Edit::Load(ids.get(i).cloned().unwrap_or_default())),
                )
                .into(),
            );
        }
        content.push(column::with_children(library).spacing(8).into());
        if recipe.kind == Kind::Calendar && self.config.glance.enabled {
            content.push(split(
                label("Calendar is currently part of Daily overview.", 12., MUTED),
                button::text("Widget visibility…")
                    .on_press(Message::Page(Page::Widgets))
                    .into(),
            ));
        }
        let name_editor: Element<'_, Message> = text_input("Widget name", &recipe.name)
            .id(cosmic::widget::Id::new("recipe-name"))
            .on_input(|value| edit(Edit::Name(value)))
            .into();
        let controls = segmented(
            vec![
                (
                    "Compact".into(),
                    !editor.wide,
                    Some(edit(Edit::Wide(false))),
                ),
                ("Wide".into(), editor.wide, Some(edit(Edit::Wide(true)))),
            ],
            self.settings_accent(),
        );
        let sample_size = if editor.wide {
            [520., 220.]
        } else {
            [320., 220.]
        };
        let sample =
            crate::app::settings_recipe_preview(&self.config, recipe, editor.missing, sample_size)
                .map(|_| Message::Preview);
        let sample = self.preview_stage(super::sample::thumbnail(
            sample,
            sample_size,
            [
                self.stage_width().max(240.) - 28.,
                self.stage_height().min(260.),
            ],
        ));
        let preview = group(
            None,
            vec![
                self.preview_header("Sample content · candidate widget", false),
                controls,
                sample,
                if matches!(recipe.kind, Kind::Weather | Kind::Calendar | Kind::System) {
                    segmented(
                        vec![
                            (
                                if recipe.kind == Kind::Calendar {
                                    "Sample agenda"
                                } else if recipe.kind == Kind::System {
                                    "Sample readings"
                                } else {
                                    "Sample weather"
                                }
                                .into(),
                                !editor.missing,
                                Some(edit(Edit::Missing(false))),
                            ),
                            (
                                if recipe.kind == Kind::Calendar {
                                    "Not connected"
                                } else if recipe.kind == Kind::System {
                                    "Unavailable"
                                } else {
                                    "Missing location"
                                }
                                .into(),
                                editor.missing,
                                Some(edit(Edit::Missing(true))),
                            ),
                        ],
                        self.settings_accent(),
                    )
                } else {
                    label("Uses your current style and font.", 12., MUTED)
                },
            ],
        );
        let wide = editor.wide;
        let arrangement = if wide { recipe.wide } else { recipe.compact };
        let mut parts = vec![split(
            label(
                if wide {
                    "Wide layout"
                } else {
                    "Compact layout"
                },
                14.,
                FG,
            ),
            segmented(
                Arrangement::ALL
                    .into_iter()
                    .map(|value| {
                        (
                            value.title().into(),
                            value == arrangement,
                            Some(edit(Edit::Arrangement(wide, value))),
                        )
                    })
                    .collect(),
                self.settings_accent(),
            ),
        )];
        for (i, part) in recipe.parts.iter().enumerate() {
            parts.push(split(
                label(part.title(), 14., FG),
                row::with_children(vec![
                    button::text("Up")
                        .on_press_maybe((!busy && i > 0).then(|| edit(Edit::Move(i, true))))
                        .into(),
                    button::text("Down")
                        .on_press_maybe(
                            (!busy && i + 1 < recipe.parts.len())
                                .then(|| edit(Edit::Move(i, false))),
                        )
                        .into(),
                    button::text("Remove")
                        .on_press_maybe(
                            (!busy && recipe.can_remove(i)).then(|| edit(Edit::Remove(i))),
                        )
                        .into(),
                ])
                .spacing(4)
                .wrap()
                .into(),
            ));
        }
        let available = recipe
            .kind
            .parts()
            .iter()
            .filter(|p| !recipe.parts.contains(p))
            .map(|p| {
                button::text(format!("+ {}", p.title()))
                    .on_press_maybe((!busy && recipe.parts.len() < 8).then(|| edit(Edit::Add(*p))))
                    .into()
            })
            .collect::<Vec<_>>();
        if !available.is_empty() {
            parts.push(row::with_children(available).spacing(4).wrap().into());
        }
        if recipe.parts.contains(&Part::Caption) {
            parts.push(
                text_input("Caption", &recipe.caption)
                    .on_input(|value| edit(Edit::Caption(value)))
                    .into(),
            );
        }
        parts.push(self.help("recipe-parts", "Parts", "Small widgets keep the main content and connection status. Split places the main content beside the remaining parts. Calendar follows its date and privacy settings. System monitor samples the readings you select; its first reading stays visible at small sizes."));
        let inspector: Element<'_, Message> =
            column::with_children(vec![name_editor, group(Some("Parts"), parts)])
                .spacing(16)
                .into();
        content.push(self.inspector_layout(preview, inspector, "recipe"));
        content.push(
            row::with_children(vec![
                button::text("Make a copy")
                    .on_press_maybe((!busy).then(|| edit(Edit::Duplicate)))
                    .into(),
                button::text("Reload library")
                    .on_press_maybe((!busy).then(|| edit(Edit::Reload)))
                    .into(),
                button::text("Delete saved widget…")
                    .on_press_maybe(
                        (!busy && editor.expected.is_some()).then(|| edit(Edit::Delete)),
                    )
                    .into(),
            ])
            .spacing(8)
            .wrap()
            .into(),
        );
        if editor.delete_review {
            content.push(group(
                Some("Delete this library copy?"),
                vec![
                    label(
                        "Applied widgets and saved setups keep their own copy.",
                        13.,
                        MUTED,
                    ),
                    button::destructive("Delete saved widget")
                        .on_press_maybe((!busy).then(|| edit(Edit::ConfirmDelete)))
                        .into(),
                ],
            ));
        }
        column::with_children(content).spacing(16).into()
    }
}

impl Settings {
    pub(super) fn recipe_actions(&self) -> Element<'_, Message> {
        let recipe = &self.recipe_editor.draft;
        let busy = self.native_busy;
        let save = button::standard("Save to library").id(cosmic::widget::Id::new("recipe-save"));
        let apply = button::suggested(format!("Apply to {}", recipe.kind.title()))
            .id(cosmic::widget::Id::new("recipe-apply"));
        let mut actions = vec![
            if busy {
                save.into()
            } else {
                save.on_press(edit(Edit::Save)).into()
            },
            button::text("Use built-in")
                .on_press_maybe((!busy && self.design.managed).then(|| edit(Edit::UseBuiltin)))
                .into(),
        ];
        if let Some(undo) = self.page_undo_button() {
            actions.push(undo);
        }
        actions.push(cosmic::widget::space().width(Length::Fill).into());
        actions.push(
            apply
                .on_press_maybe((!busy && self.design.managed).then(|| edit(Edit::Apply)))
                .into(),
        );
        let actions: Element<'_, Message> = row::with_children(actions)
            .spacing(8)
            .align_y(Alignment::Center)
            .into();
        let mut content = Vec::new();
        if let Some(error) = self.page_error() {
            content.push(label(error, 12., desktop::ERROR));
        }
        content.push(actions);
        container(column::with_children(content).spacing(6))
            .padding([8, 20])
            .width(Length::Fill)
            .into()
    }
}
