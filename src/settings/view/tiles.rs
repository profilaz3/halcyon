//! The Data tiles page: the tiles with a way to remove each, and a form that
//! adds one from a JSON file, listing the fields the file holds (ADR 0015).
use super::desktop::WARN;
use super::*;
use crate::settings::tools::Message as Edit;
use crate::settings::tools::tile_editor::{Message as Tile, SHOWS};
use crate::tools::cards;

fn edit(message: Tile) -> Message {
    Message::Tools(Edit::Tile(message))
}

/// A form field; Enter adds the tile.
fn input<'a>(
    id: &'static str,
    hint: &'static str,
    value: &'a str,
    on: fn(String) -> Tile,
) -> Element<'a, Message> {
    text_input(hint, value)
        .id(id.into())
        .on_input(move |s| edit(on(s)))
        .on_submit(|_| edit(Tile::Add))
        .width(Length::Fixed(240.))
        .into()
}

/// Fields listed before the rest are left to typing.
const LISTED: usize = 12;

fn every(seconds: u32) -> String {
    match seconds {
        s if s < 60 => format!("{s} s"),
        s if s < 3600 => format!("{} min", s / 60),
        s => format!("{} h", s / 3600),
    }
}

impl Settings {
    pub(super) fn tiles_page(&self) -> Element<'_, Message> {
        let editor = &self.tools.tiles;
        let saved = &self.config.tiles.tiles;
        let size = Some(self.tool_preview_size(Widget::Tiles));
        let mut sections = Vec::new();
        let preview = if saved.is_empty() {
            static SAMPLE: std::sync::OnceLock<(Vec<crate::tiles::Tile>, crate::tiles::State)> =
                std::sync::OnceLock::new();
            let (tiles, state) = SAMPLE.get_or_init(crate::tiles::sample);
            sections.push(self.sample_note("tiles-sample", Some("add a tile below")));
            let mut cfg = self.config.clone();
            cfg.tiles.tiles = tiles.clone();
            cards::tiles(
                state,
                tiles,
                &cfg,
                Some(cards::size(Widget::Tiles, &cfg)),
                None,
            )
        } else {
            cards::tiles(&editor.preview, saved, &self.config, size, None)
        };
        let accent = self.settings_accent();

        let mut list = Vec::new();
        for (index, tile) in saved.iter().enumerate() {
            let file = tile.path().file_name().map_or_else(
                || tile.path().display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            let field = if tile.field.is_empty() {
                "whole file".to_owned()
            } else {
                tile.field.clone()
            };
            let name = format!("Remove {}", tile.label);
            list.push(
                row::with_children(vec![
                    container(
                        column::with_children(vec![
                            label(tile.label.clone(), 14., FG),
                            label(
                                format!(
                                    "{} · {file} · {field} · every {}",
                                    tile.show.label(),
                                    every(tile.every)
                                ),
                                12.,
                                MUTED,
                            ),
                        ])
                        .spacing(1),
                    )
                    .width(Length::Fill)
                    .into(),
                    button::icon(icon::from_name("list-remove-symbolic").size(16))
                        .extra_small()
                        .name(name)
                        .id(format!("tile-remove-{index}").into())
                        .on_press(edit(Tile::Remove(index)))
                        .into(),
                ])
                .spacing(6)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        if list.is_empty() {
            list.push(label("No tiles yet. Add one below.", 13., MUTED));
        }
        sections.push(self.search_target("tiles-list", group(Some("Your tiles"), list)));

        let draft = &editor.draft;
        let mut form = vec![group_row(
            "Start from",
            "Fills the form; choose the file yourself",
            segmented(
                crate::tiles::TEMPLATES
                    .iter()
                    .enumerate()
                    .map(|(i, t)| (t.name.into(), false, Some(edit(Tile::Template(i)))))
                    .collect(),
                accent,
            ),
        )];
        form.push(group_row(
            "File",
            "A JSON file another program writes",
            row::with_children(vec![
                text_input("/home/you/status.json", &draft.path)
                    .id("tile-path".into())
                    .on_input(|s| edit(Tile::Path(s)))
                    .on_submit(|_| edit(Tile::ReadPath))
                    .width(Length::Fixed(240.))
                    .into(),
                button::standard("Choose file…")
                    .on_press_maybe((!editor.choosing).then(|| edit(Tile::Choose)))
                    .into(),
            ])
            .spacing(6)
            .align_y(Alignment::Center)
            .into(),
        ));
        match &editor.fields {
            Some((_, Err(problem))) => form.push(label(problem.clone(), 13., WARN)),
            Some((_, Ok(fields))) if fields.is_empty() => {
                form.push(label("This file holds no single values.", 13., WARN));
            }
            Some((_, Ok(fields))) => {
                let mut rows = vec![label("Fields in this file", 13., MUTED)];
                for (index, (path, value)) in fields.iter().take(LISTED).enumerate() {
                    let chosen = draft.field == *path;
                    rows.push(
                        button::custom(
                            row::with_children(vec![
                                container(label(path.clone(), 13., FG))
                                    .width(Length::Fill)
                                    .into(),
                                label(value.text(), 13., MUTED),
                            ])
                            .spacing(12),
                        )
                        .class(if chosen {
                            cosmic::theme::Button::Standard
                        } else {
                            cosmic::theme::Button::Text
                        })
                        .id(format!("tile-field-{index}").into())
                        .width(Length::Fill)
                        .on_press(edit(Tile::Field(path.clone())))
                        .into(),
                    );
                }
                if fields.len() > LISTED {
                    rows.push(label(
                        format!("{} more: type the field below.", fields.len() - LISTED),
                        12.,
                        MUTED,
                    ));
                }
                form.push(column::with_children(rows).spacing(2).into());
            }
            None => {}
        }
        form.push(group_row(
            "Field",
            "Names joined by dots; a number picks a place in a list",
            input("tile-field", "build.result", &draft.field, Tile::Field),
        ));
        form.push(group_row(
            "Name",
            "Shown above the value",
            input("tile-label", "Last build", &draft.label, Tile::Label),
        ));
        form.push(group_row(
            "Show as",
            "",
            segmented(
                SHOWS
                    .iter()
                    .enumerate()
                    .map(|(i, name)| {
                        (
                            (*name).into(),
                            usize::from(draft.show) == i,
                            Some(edit(Tile::Show(i as u8))),
                        )
                    })
                    .collect(),
                accent,
            ),
        ));
        if draft.show == 2 {
            form.push(group_row(
                "Smallest value",
                "Where the dial starts",
                input("tile-min", "0", &draft.min, Tile::Min),
            ));
        }
        if matches!(draft.show, 1 | 2) {
            form.push(group_row(
                "Largest value",
                if draft.show == 1 {
                    "A full bar"
                } else {
                    "Where the dial ends"
                },
                input("tile-max", "100", &draft.max, Tile::Max),
            ));
        }
        form.push(group_row(
            "Unit",
            "Shown after the value, like % or GB",
            input("tile-unit", "%", &draft.unit, Tile::Unit),
        ));
        form.push(group_row(
            "Read every",
            "",
            segmented(
                crate::tiles::EVERY
                    .iter()
                    .map(|&s| (every(s), draft.every == s, Some(edit(Tile::Every(s)))))
                    .collect(),
                accent,
            ),
        ));
        if let Some(error) = &editor.error {
            form.push(label(error.clone(), 13., WARN));
        }
        form.push(
            row::with_children(vec![
                container(label(
                    "Halcyon reads the file only while the widget shows and never runs it. \
                     Paths stay on this computer, out of shared Looks.",
                    12.,
                    MUTED,
                ))
                .width(Length::Fill)
                .into(),
                button::suggested("Add tile")
                    .id("tile-add".into())
                    .on_press(edit(Tile::Add))
                    .into(),
            ])
            .spacing(12)
            .align_y(Alignment::Center)
            .into(),
        );
        sections.push(self.search_target("tiles-add", group(Some("Add a tile"), form)));
        self.widget_template(Widget::Tiles, "Data tiles", Some(preview), sections)
    }
}
