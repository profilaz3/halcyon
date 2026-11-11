use super::*;
use crate::recipes::{Arrangement, Finish, Kind, Part, Recipe, TimerFace};
use crate::settings::recipes::Message as Edit;
use crate::tools::cards::recipe::PreviewState;
fn edit(message: Edit) -> Message {
    Message::Recipe(message)
}
impl Settings {
    /// Make your own version of a widget: the sample on top, then what it
    /// is, its parts in order, and housekeeping last.
    pub(super) fn recipe_page(&self) -> Element<'_, Message> {
        let editor = &self.recipe_editor;
        let recipe = &editor.draft;
        let busy = self.native_busy;
        let accent = self.settings_accent();
        let mut content = vec![heading(
            "Widget builder",
            "Make your own version of a widget from parts.",
        )];
        if !self.design.managed {
            content.push(self.design_section(None));
        }
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
        let sample_size = if recipe.tool() && !editor.wide {
            recipe.tool_size()
        } else if editor.wide {
            [520., 200.]
        } else {
            // About the size each widget really has on the desktop.
            match recipe.kind {
                Kind::Clock => [320., 150.],
                Kind::Weather => [360., 180.],
                _ => [320., 200.],
            }
        };
        let sample = if recipe.tool() {
            crate::tools::cards::recipe::sample(
                &self.config,
                recipe,
                editor.preview_state,
                sample_size,
            )
            .map(|_| Message::Preview)
        } else {
            crate::app::settings_recipe_preview(&self.config, recipe, false, sample_size)
                .map(|_| Message::Preview)
        };
        // Tall widgets (a sleeve with artwork, a ring timer) get a taller
        // stage so their text stays readable.
        let stage_height = if sample_size[1] > 240. {
            (sample_size[1] * 0.85 + 28.).min(300.)
        } else {
            self.stage_height().min(260.).max(180.)
        };
        content.push(
            container(self.preview_stage(super::sample::thumbnail(
                sample,
                sample_size,
                [(self.content_width() - 60.).max(180.), stage_height - 28.],
            )))
            .id(cosmic::widget::Id::new("recipe-stage"))
            .width(Length::Fill)
            .into(),
        );
        let mut widget_rows = vec![group_row(
            "Type",
            "",
            segmented(
                Kind::ALL
                    .into_iter()
                    .map(|kind| {
                        (
                            if kind == Kind::System {
                                "System".into()
                            } else {
                                kind.title().into()
                            },
                            kind == recipe.kind,
                            (!busy && kind != recipe.kind).then(|| edit(Edit::New(kind))),
                        )
                    })
                    .collect(),
                accent,
            ),
        )];
        if !editor.library_labels.is_empty() {
            let ids = editor.library_ids.clone();
            widget_rows.push(group_row(
                "Saved designs",
                "",
                cosmic::widget::dropdown(
                    &editor.library_labels,
                    editor.library_ids.iter().position(|id| id == &recipe.id),
                    move |i| edit(Edit::Load(ids.get(i).cloned().unwrap_or_default())),
                )
                .into(),
            ));
        }
        widget_rows.push(group_row(
            "Name",
            "",
            container(
                text_input("My widget", &recipe.name)
                    .id(cosmic::widget::Id::new("recipe-name"))
                    .on_input(|value| edit(Edit::Name(value)))
                    // Enter saves, like Save below.
                    .on_submit_maybe((!busy).then_some(|_| edit(Edit::Save))),
            )
            .width(240)
            .into(),
        ));
        widget_rows.push(group_row(
            "Size",
            "",
            segmented(
                vec![
                    ("Small".into(), !editor.wide, Some(edit(Edit::Wide(false)))),
                    ("Wide".into(), editor.wide, Some(edit(Edit::Wide(true)))),
                ],
                accent,
            ),
        ));
        if recipe.tool() {
            let states = PreviewState::choices(recipe.kind);
            let labels: Vec<_> = states
                .iter()
                .map(|s| {
                    if *s == PreviewState::Active {
                        if recipe.kind == Kind::Music {
                            "Playing"
                        } else {
                            "Running"
                        }
                    } else {
                        s.title()
                    }
                })
                .collect();
            widget_rows.push(group_row(
                "Show it while",
                "",
                cosmic::widget::dropdown(
                    labels,
                    states.iter().position(|s| *s == editor.preview_state),
                    move |i| edit(Edit::PreviewState(states[i])),
                )
                .into(),
            ));
        }
        if recipe.kind == Kind::Calendar && self.config.glance.enabled {
            widget_rows.push(group_row_state(
                "Calendar is part of Daily overview now",
                "Your design shows when Calendar has its own card",
                button::text("Widgets")
                    .on_press(Message::Page(Page::Widgets))
                    .into(),
            ));
        }
        content.push(group(Some("Basics"), widget_rows));
        if recipe.tool() {
            let mut look = vec![group_row(
                "Start from",
                "",
                segmented(
                    Recipe::template_names(recipe.kind)
                        .iter()
                        .enumerate()
                        .map(|(i, name)| {
                            (
                                (*name).into(),
                                recipe.name == *name,
                                Some(edit(Edit::Template(i))),
                            )
                        })
                        .collect(),
                    accent,
                ),
            )];
            let mut finishes = vec![(
                "Same as others".into(),
                recipe.finish.is_none(),
                Some(edit(Edit::Finish(None))),
            )];
            finishes.extend(Finish::ALL.into_iter().map(|finish| {
                (
                    finish.title().into(),
                    recipe.finish == Some(finish),
                    Some(edit(Edit::Finish(Some(finish)))),
                )
            }));
            look.push(group_row(
                "Card style",
                "Pick a finish, or keep the one your other widgets use",
                segmented(finishes, accent),
            ));
            if recipe.kind == Kind::Focus {
                look.push(group_row(
                    "Timer",
                    "",
                    segmented(
                        vec![
                            (
                                "Ring".into(),
                                recipe.timer_face == Some(TimerFace::Ring),
                                Some(edit(Edit::TimerFace(TimerFace::Ring))),
                            ),
                            (
                                "Digits".into(),
                                recipe.timer_face != Some(TimerFace::Ring),
                                Some(edit(Edit::TimerFace(TimerFace::Digits))),
                            ),
                        ],
                        accent,
                    ),
                ));
            }
            content.push(group(Some("Look"), look));
        }
        let wide = editor.wide;
        let arrangement = if wide { recipe.wide } else { recipe.compact };
        let mut parts = vec![group_row_state(
            "Layout",
            if wide {
                "When the widget is wide"
            } else {
                "When the widget is small"
            },
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
                accent,
            ),
        )];
        // Every row has the same three buttons in the same place; a move
        // that can't happen is dimmed.
        let tool = |name: &'static str, tip: &'static str, message: Option<Message>| {
            let control: Element<'_, Message> = match message {
                Some(message) => cosmic::widget::tooltip(
                    button::icon(icon::from_name(name).size(16)).on_press(message),
                    text::body(tip),
                    cosmic::widget::tooltip::Position::Top,
                )
                .into(),
                None => container(
                    icon::from_name(name)
                        .size(16)
                        .icon()
                        // Symbolic tints ignore alpha, so dim with a solid grey.
                        .class(cosmic::theme::Svg::custom(|_| {
                            cosmic::iced::widget::svg::Style {
                                color: Some(Color::from_rgb(0.36, 0.36, 0.38)),
                            }
                        })),
                )
                .padding(8)
                .into(),
            };
            container(control)
                .width(36)
                .align_x(Alignment::Center)
                .into()
        };
        for (i, part) in recipe.parts.iter().enumerate() {
            parts.push(split(
                label(part.title(), 14., FG),
                row::with_children(vec![
                    tool(
                        "go-up-symbolic",
                        "Move up",
                        (!busy && i > 0).then(|| edit(Edit::Move(i, true))),
                    ),
                    tool(
                        "go-down-symbolic",
                        "Move down",
                        (!busy && i + 1 < recipe.parts.len()).then(|| edit(Edit::Move(i, false))),
                    ),
                    tool(
                        "edit-delete-symbolic",
                        "Remove",
                        (!busy && recipe.can_remove(i)).then(|| edit(Edit::Remove(i))),
                    ),
                ])
                .spacing(2)
                .into(),
            ));
            if *part == Part::Caption {
                parts.push(
                    text_input("Caption text", &recipe.caption)
                        .on_input(|value| edit(Edit::Caption(value)))
                        .on_submit_maybe((!busy).then_some(|_| edit(Edit::Save)))
                        .into(),
                );
            }
        }
        let available = recipe
            .kind
            .parts()
            .iter()
            .filter(|p| !recipe.parts.contains(p))
            .map(|p| {
                button::standard(p.title())
                    .leading_icon(icon::from_name("list-add-symbolic").size(14))
                    .on_press_maybe((!busy && recipe.parts.len() < 8).then(|| edit(Edit::Add(*p))))
                    .into()
            })
            .collect::<Vec<_>>();
        if !available.is_empty() {
            parts.push(
                column::with_children(vec![
                    label("Add a part", 14., FG),
                    row::with_children(available).spacing(6).wrap().into(),
                ])
                .spacing(8)
                .into(),
            );
        }
        content.push(group(Some("Parts, top to bottom"), parts));
        let mut more = vec![
            group_row(
                "Save without using it",
                "It waits under Saved designs",
                button::standard("Save")
                    .id(cosmic::widget::Id::new("recipe-save"))
                    .on_press_maybe((!busy).then(|| edit(Edit::Save)))
                    .into(),
            ),
            group_row(
                "Make a copy",
                "Start a new design from this one",
                button::standard("Copy")
                    .on_press_maybe((!busy).then(|| edit(Edit::Duplicate)))
                    .into(),
            ),
            group_row(
                format!("Go back to the built-in {}", recipe.kind.title().to_lowercase()),
                "Your designs stay saved",
                button::standard("Use built-in")
                    .on_press_maybe((!busy && self.design.managed).then(|| edit(Edit::UseBuiltin)))
                    .into(),
            ),
        ];
        if editor.expected.is_some() {
            more.push(group_row(
                "Delete this design",
                "",
                button::standard("Delete…")
                    .on_press_maybe((!busy).then(|| edit(Edit::Delete)))
                    .into(),
            ));
        }
        if editor.delete_review {
            more.push(split(
                detail_state(
                    "Delete this design?",
                    "Widgets already using it keep their own copy.",
                ),
                row::with_children(vec![
                    button::text("Cancel")
                        .on_press(edit(Edit::CancelDelete))
                        .into(),
                    button::destructive("Delete")
                        .on_press_maybe((!busy).then(|| edit(Edit::ConfirmDelete)))
                        .into(),
                ])
                .spacing(8)
                .into(),
            ));
        }
        content.push(group(Some("More"), more));
        column::with_children(content).spacing(16).into()
    }
}

impl Settings {
    pub(super) fn recipe_actions(&self) -> Element<'_, Message> {
        let recipe = &self.recipe_editor.draft;
        let busy = self.native_busy;
        let apply = button::suggested("Apply design")
            .id(cosmic::widget::Id::new("recipe-apply"));
        let applied = self.config.recipes.get(recipe.kind) == Some(recipe);
        let actions = self.preview_bar(
            label(
                if applied {
                    format!("Your {} uses this design", recipe.kind.title())
                } else if !self.design.managed && self.design.review {
                    "Update widgets above to use this design".into()
                } else if !self.design.managed {
                    "Press Get ready… above to use this design".into()
                } else {
                    format!("Not on your {} yet", recipe.kind.title())
                },
                12.,
                MUTED,
            ),
            self.recipe_editor.can_undo_step(),
            vec![
            ],
            edit(Edit::Discard),
            "recipe-discard",
            apply
                .on_press_maybe((!busy && self.design.managed && !applied).then(|| edit(Edit::Apply)))
                .into(),
        );
        let mut content = Vec::new();
        if let Some(error) = self
            .recipe_editor
            .error
            .then_some(self.recipe_editor.notice.as_deref())
            .flatten()
            .or_else(|| self.page_error())
        {
            content.push(self.error_line(error));
        }
        content.push(actions);
        column::with_children(content).spacing(6).into()
    }
}
