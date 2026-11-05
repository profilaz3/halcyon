use super::*;
impl Settings {
    pub(super) fn setup_sources(&self) -> Element<'_, Message> {
        let sources = self.profiles.discovery_sources();
        if sources.is_empty() {
            return cosmic::widget::space().height(0).into();
        }
        let mut rows = vec![label("Sources and revisions", 16., FG)];
        for source in sources {
            let credits = &source.attribution.credits;
            let name = if credits.author.is_empty() {
                "Uncredited source"
            } else {
                &credits.author
            };
            let mut row_items = vec![
                column::with_children(vec![
                    label(format!("{} · {}", name, credits.license), 14., FG),
                    label(
                        crate::settings::profiles::ProfilesEditor::revision_label(
                            &source.attribution.identity,
                        ),
                        12.,
                        MUTED,
                    ),
                ])
                .spacing(4)
                .width(Length::Fill)
                .into(),
            ];
            if let Some(selection) = source.exact {
                row_items.push(
                    button::text(if matches!(selection, Selection::Preset(_)) {
                        "View source"
                    } else {
                        "Open local copy"
                    })
                    .on_press(edit(Edit::Select(selection)))
                    .into(),
                );
            } else {
                row_items.push(label("Source not in library", 12., MUTED));
            }
            if let Some(selection) = source.latest {
                row_items.push(
                    button::text("Review update")
                        .on_press(edit(Edit::Select(selection)))
                        .into(),
                );
            }
            rows.push(
                row::with_children(row_items)
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .wrap()
                    .into(),
            );
            if !credits.source.is_empty() {
                rows.push(label(credits.source.clone(), 12., MUTED));
            }
        }
        rows.push(label(
            "Opening a source leaves your saved copy unchanged.",
            12.,
            MUTED,
        ));
        self.disclosure("setup-sources", "Sources and revisions", section(rows))
    }
}
