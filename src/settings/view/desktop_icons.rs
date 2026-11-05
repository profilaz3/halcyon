//! Desktop icons subpage: a schematic of the drafted icon size, spacing and
//! what the desktop shows, the controls, and the deferred-refresh note.
use super::desktop::{ERROR, WARN, banner, conflict_banner, kept};
use super::windows::{recovery_banner, store_pill};
use super::*;
use crate::desktop::desktop_icons::{DesktopIcons, SCALE_MAX, SCALE_MIN, SCALE_STEP, Show};
use crate::desktop::panels::Setting;
use crate::settings::desktop_icons::{Message as Edit, REFRESH_NOTE, UNAVAILABLE};

fn edit(message: Edit) -> Message {
    Message::DesktopIcons(message)
}

/// The preview's height; the width follows the page.
const PREVIEW_HEIGHT: f32 = 176.;

/// What the desktop shows, as real icons with names, laid out in columns
/// from the top left like the desktop does. Size and spacing scale it.
fn miniature<'a>(icons: DesktopIcons) -> Element<'a, Message> {
    let mut items: Vec<(&'static str, &'static str)> = Vec::new();
    if icons.show_content {
        items.extend([
            ("folder-documents", "Documents"),
            ("folder-pictures", "Photos"),
            ("text-x-generic", "Notes.txt"),
            ("application-pdf", "Report.pdf"),
        ]);
    }
    if icons.show_mounted_drives {
        items.push(("drive-removable-media", "USB drive"));
    }
    if icons.show_trash {
        items.push(("user-trash", "Trash"));
    }
    if items.is_empty() {
        return container(label("Nothing is shown on the desktop", 12., Color { a: 0.85, ..Color::WHITE }))
            .width(Length::Fill)
            .height(PREVIEW_HEIGHT)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into();
    }
    let size = f32::from(icons.icon_size.get()) / 100.;
    let spacing = f32::from(icons.grid_spacing.get()) / 100.;
    // Half of the desktop's own size, so changes stay visible.
    let side = (32. * size).clamp(16., 64.);
    let cell_w = (side * 1.9).max(64.) * spacing;
    let cell_h = (side + 24.) * spacing;
    let margin = 12.;
    let rows = (((PREVIEW_HEIGHT - 2. * margin) / cell_h).floor() as usize).max(1);
    let columns = items
        .chunks(rows)
        .map(|chunk| {
            column::with_children(
                chunk
                    .iter()
                    .map(|(name, title)| {
                        container(
                            column::with_children(vec![
                                icon::from_name(*name).size(side as u16).icon().into(),
                                text::body(*title)
                                    .size(11)
                                    .class(cosmic::theme::Text::Color(Color::WHITE))
                                    .into(),
                            ])
                            .spacing(3)
                            .align_x(Alignment::Center),
                        )
                        .width(cell_w)
                        .height(cell_h)
                        .center_x(cell_w)
                        .into()
                    })
                    .collect::<Vec<_>>(),
            )
            .into()
        })
        .collect::<Vec<_>>();
    container(row::with_children(columns))
        .padding(margin)
        .width(Length::Fill)
        .height(PREVIEW_HEIGHT)
        .clip(true)
        .into()
}

impl Settings {
    pub(super) fn desktop_icons_page(&self) -> Element<'_, Message> {
        let accent = self.settings_accent();
        let editor = &self.desktop_icons;
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        let heading = || {
            column::with_children(vec![split(
                heading("Icons on the desktop", "Files, folders and drives on top of the wallpaper."),
                store_pill(editor.store(), accent),
            )])
            .spacing(4)
            .width(Length::Fill)
        };
        if let Some(error) = editor.unavailable() {
            items.push(heading().into());
            items.push(banner(
                format!("Desktop icon settings are unavailable. {error}"),
                ERROR,
                Vec::new(),
            ));
            return column::with_children(items).spacing(14).into();
        }
        if editor.opened() && !editor.available() {
            items.push(heading().into());
            items.push(banner(UNAVAILABLE, WARN, Vec::new()));
            return column::with_children(items).spacing(14).into();
        }
        let Some(draft) = editor.draft() else {
            items.push(heading().into());
            items.push(label("Reading desktop icon settings…", 12., MUTED));
            return column::with_children(items).spacing(14).into();
        };
        let config = draft.config();
        let known = config.known().copied();
        items.push(heading().into());
        items.push(
            // Drawn on the same desktop backdrop as every other preview.
            container(self.preview_stage(miniature(known.unwrap_or_default())))
            .id(cosmic::widget::Id::new("desktop-scene"))
            .width(Length::Fill)
            .into(),
        );
        if let Some(banner) = recovery_banner(
            editor.recovery(),
            editor.writable(),
            edit(Edit::Recover),
            edit(Edit::KeepRecovery),
        ) {
            items.push(banner);
        }
        if !draft.conflicts().is_empty() {
            items.push(conflict_banner(
                crate::settings::system_appearance::describe(draft.conflicts()),
                edit(Edit::UseCurrent),
                edit(Edit::KeepMine),
            ));
        }
        if let Setting::Unsupported(raw) = &config {
            items.push(banner(
                "These desktop icon settings include values this version doesn’t recognize. They are kept as they are and can’t be changed here.",
                WARN,
                vec![kept(raw)],
            ));
        }
        let show = |title: &'static str, what: Show, value: Option<bool>| {
            let mut toggle = toggler(value.unwrap_or(false));
            if value.is_some() {
                toggle = toggle.on_toggle(move |on| edit(Edit::Show(what, on)));
            }
            split(label(title, 13., FG), toggle.into())
        };
        items.push(group(
            Some("Show on the desktop"),
            vec![
                show(
                    "Desktop folder",
                    Show::Folder,
                    known.map(|icons| icons.show_content),
                ),
                show(
                    "Mounted drives",
                    Show::Drives,
                    known.map(|icons| icons.show_mounted_drives),
                ),
                show("Trash", Show::Trash, known.map(|icons| icons.show_trash)),
            ],
        ));
        let scale = |title: &'static str, value: Option<u16>, message: fn(u16) -> Edit| {
            let value_label = match value {
                Some(value)
                    if value % SCALE_STEP != 0 || !(SCALE_MIN..=SCALE_MAX).contains(&value) =>
                {
                    pill(format!("{value}%, kept"), WARN)
                }
                Some(value) => label(format!("{value}%"), 12., accent),
                None => label("—", 12., MUTED),
            };
            let mut parts = vec![split(label(title, 13., FG), value_label)];
            // Kept settings: no slider to drag (the toggles are inert too).
            if let Some(value) = value {
                parts.push(
                    slider(
                        SCALE_MIN..=SCALE_MAX,
                        value.clamp(SCALE_MIN, SCALE_MAX),
                        move |value| edit(message(value)),
                    )
                    .step(SCALE_STEP)
                    .on_release(edit(Edit::Apply))
                    .width(Length::Fill)
                    .apply(|control| completed_slider(control.into(), edit(Edit::Apply))),
                );
            }
            column::with_children(parts).spacing(6).into()
        };
        items.push(group(
            Some("Size"),
            vec![
                scale(
                    "Icon size",
                    known.map(|icons| icons.icon_size.get()),
                    Edit::Size,
                ),
                scale(
                    "Spacing",
                    known.map(|icons| icons.grid_spacing.get()),
                    Edit::Spacing,
                ),
            ],
        ));
        items.push(label(REFRESH_NOTE, 12., MUTED));
        column::with_children(items).spacing(16).into()
    }
}
