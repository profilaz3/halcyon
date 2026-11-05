use super::*;
use crate::desktop::profiles::mix::{Component, SourceKind};
use crate::settings::profiles::mix::PAGE_SIZE;

impl Settings {
    pub(super) fn mix_board(&self) -> Element<'_, Message> {
        let editor = &self.profiles;
        let Some(draft) = &editor.mix else {
            return column::with_children(Vec::<Element<'_, Message>>::new()).into();
        };
        let accent = self.settings_accent();
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        let slots = Component::ALL
            .into_iter()
            .map(|part| {
                let body = column::with_children(vec![
                    label(part.title(), 12., MUTED),
                    label(draft.source_name(part).unwrap_or("Choose…"), 16., FG),
                ])
                .spacing(6);
                button::custom(body)
                    .class(tile_style(draft.choosing && draft.picking == part, accent))
                    .padding(14)
                    .width(Length::Fill)
                    .id(cosmic::widget::Id::new(format!(
                        "mix-slot-{}",
                        part.index()
                    )))
                    .on_press_maybe((!self.native_busy).then_some(edit(Edit::MixPart(part))))
                    .into()
            })
            .collect::<Vec<_>>();
        items.push(column::with_children(slots).spacing(8).into());
        if editor.name.is_some() {
            items.push(self.name_field("Save"));
        }
        if draft.choosing {
            if editor.profile().is_some() {
                items.push(
                    button::text("Back to preview")
                        .on_press(edit(Edit::MixClosePicker))
                        .into(),
                );
            }
            let all = editor.mix_choices();
            let pages = all.len().div_ceil(PAGE_SIZE).max(1);
            let page = draft.page.min(pages - 1);
            let search = text_input("Find a source…", &draft.query)
                .id(cosmic::widget::Id::new("mix-search"))
                .on_input(|s| edit(Edit::MixQuery(s)));
            items.push(search.into());
            let choices = all
                .into_iter()
                .skip(page * PAGE_SIZE)
                .take(PAGE_SIZE)
                .map(|(source, name)| {
                    let kind = match &source {
                        Selection::Current => "Captured now",
                        Selection::Preset(_) => "Built-in",
                        Selection::Saved(_) => "Saved setup",
                    };
                    let selected = draft.sources[draft.picking.index()]
                        .as_ref()
                        .is_some_and(|s| match &source {
                            Selection::Current => s.origin.kind == SourceKind::Desktop,
                            Selection::Preset(id) => {
                                s.origin.kind == SourceKind::Builtin
                                    && s.origin.id.as_deref() == Some(*id)
                            }
                            Selection::Saved(id) => {
                                s.origin.kind == SourceKind::Library
                                    && s.origin.id.as_deref() == Some(id.as_str())
                            }
                        });
                    let id = match &source {
                        Selection::Current => "desktop".into(),
                        Selection::Preset(id) => (*id).into(),
                        Selection::Saved(id) => id.as_str().to_string(),
                    };
                    button::custom(
                        column::with_children(vec![label(name, 14., FG), label(kind, 11., MUTED)])
                            .spacing(3),
                    )
                    .class(tile_style(selected, accent))
                    .padding([10, 14])
                    .width(Length::Fill)
                    .id(cosmic::widget::Id::new(format!("mix-source-{id}")))
                    .on_press_maybe(
                        (!self.native_busy).then_some(edit(Edit::MixPick(draft.picking, source))),
                    )
                    .into()
                })
                .collect::<Vec<_>>();
            if choices.is_empty() {
                items.push(label("No sources match this search.", 13., MUTED));
            } else {
                items.push(column::with_children(choices).spacing(8).into());
            }
            if pages > 1 {
                items.push(
                    row::with_children(vec![
                        button::text("Previous")
                            .on_press_maybe(
                                (page > 0).then_some(edit(Edit::MixPage(page.saturating_sub(1)))),
                            )
                            .into(),
                        label(format!("{} / {pages}", page + 1), 12., MUTED),
                        button::text("Next")
                            .on_press_maybe(
                                (page + 1 < pages).then_some(edit(Edit::MixPage(page + 1))),
                            )
                            .into(),
                    ])
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
                );
            }
        }
        if editor.profile().is_some() && !draft.choosing {
            items.push(self.selection_details());
        } else if editor.profile().is_none() {
            items.push(label(
                "Choose all three parts to preview the mix.",
                14.,
                MUTED,
            ));
        }
        self.inspector_layout(
            self.selection_scene(),
            column::with_children(items).spacing(14).into(),
            "mix",
        )
    }
}
