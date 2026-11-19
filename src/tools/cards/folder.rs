use super::{frame, line};
use crate::{
    config::Config,
    folder::{Listing, layout},
};
use cosmic::{
    Element,
    iced::{Alignment, Length},
    widget::{button, column, container, icon, image, row},
};
use std::path::Path;

/// Uses whole rows only. A missing thumbnail reserves the same space as an image.
pub fn folder<'a, M: Clone + 'static>(
    listing: &Listing,
    config: &Config,
    size: [f32; 2],
    open: impl Fn(&Path) -> Option<M>,
    choose: Option<M>,
) -> Element<'a, M> {
    let _card = crate::widget_style::for_card(crate::drawer::Widget::Folder);
    let _style = crate::widget_style::scope(config);
    let palette = config.palette();
    let (cols, rows) = layout(size);
    let path = config.folder.path.as_deref();
    let current = path.is_some_and(|p| p == listing.source)
        && listing.show_hidden == config.folder.show_hidden;
    let can_open = path.is_some() && !(current && listing.problem.is_some());
    let title = path
        .and_then(Path::file_name)
        .map_or_else(|| "Folder".into(), |n| n.to_string_lossy().into_owned());
    let head = row::with_children(vec![
        container(line(title, 17., palette.foreground))
            .width(Length::Fill)
            .into(),
        button::text(if can_open {
            "Open folder"
        } else {
            "Choose folder"
        })
        .on_press_maybe(if can_open {
            path.and_then(&open)
        } else {
            choose
        })
        .into(),
    ])
    .spacing(6)
    .align_y(Alignment::Center);
    let mut body = vec![head.into()];
    let message = if path.is_none() {
        Some("Choose a folder to see its recent files.")
    } else if !current {
        Some("Reading folder…")
    } else if let Some(e) = &listing.problem {
        Some(e.as_str())
    } else if listing.count == 0 {
        Some("This folder is empty.")
    } else {
        None
    };
    if let Some(s) = message {
        body.push(line(s, 13., palette.muted));
    } else {
        for chunk in listing
            .entries
            .iter()
            .take(cols * rows)
            .collect::<Vec<_>>()
            .chunks(cols)
        {
            let mut entries = Vec::new();
            for entry in chunk {
                let name = entry
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let glyph = if entry.directory {
                    "folder-symbolic"
                } else {
                    match entry
                        .path
                        .extension()
                        .and_then(|v| v.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .as_str()
                    {
                        "png" | "jpg" | "jpeg" | "webp" | "svg" => "image-x-generic-symbolic",
                        "mp3" | "flac" | "wav" | "ogg" => "audio-x-generic-symbolic",
                        "mp4" | "mkv" | "webm" => "video-x-generic-symbolic",
                        "zip" | "gz" | "tar" | "xz" => "package-x-generic-symbolic",
                        _ => "text-x-generic-symbolic",
                    }
                };
                let visual: Element<'a, M> = if cols > 1
                    && let Some(h) = entry.thumbnail.clone()
                {
                    image(h)
                        .content_fit(cosmic::iced::ContentFit::Cover)
                        .width(Length::Fill)
                        .height(48.)
                        .into()
                } else {
                    container(
                        icon::from_name(glyph)
                            .size(if cols > 1 { 32 } else { 18 })
                            .icon(),
                    )
                    .center_x(if cols > 1 {
                        Length::Fill
                    } else {
                        Length::Fixed(22.)
                    })
                    .height(if cols > 1 { 48. } else { 22. })
                    .align_y(Alignment::Center)
                    .into()
                };
                let label = line(name.clone(), 13., palette.foreground);
                let face: Element<'a, M> = if cols > 1 {
                    column::with_children(vec![visual, label]).spacing(4).into()
                } else {
                    row::with_children(vec![visual, label])
                        .spacing(8)
                        .align_y(Alignment::Center)
                        .into()
                };
                entries.push(
                    button::custom(face)
                        .class(tile_style(palette, cols > 1))
                        .name(name)
                        .width(Length::FillPortion(1))
                        .height(if cols > 1 { 80. } else { 32. })
                        .padding([4, 8])
                        .on_press_maybe(open(&entry.path))
                        .into(),
                );
            }
            for _ in chunk.len()..cols {
                entries.push(
                    container(cosmic::widget::Space::new())
                        .width(Length::FillPortion(1))
                        .into(),
                );
            }
            body.push(row::with_children(entries).spacing(6).into());
        }
    }
    frame(
        column::with_children(body).spacing(4).into(),
        config,
        size[1],
    )
}

fn tile_style(p: crate::config::Palette, wide: bool) -> cosmic::theme::Button {
    let style = move |hover: bool| button::Style {
        background: Some(cosmic::iced::Background::Color(cosmic::iced::Color {
            a: if hover {
                0.14
            } else if wide {
                0.055
            } else {
                0.025
            },
            ..if hover { p.accent } else { p.foreground }
        })),
        border_radius: 9.0.into(),
        icon_color: Some(p.muted),
        ..Default::default()
    };
    cosmic::theme::Button::Custom {
        active: Box::new(move |_, _| style(false)),
        hovered: Box::new(move |_, _| style(true)),
        pressed: Box::new(move |_, _| style(true)),
        disabled: Box::new(move |_| style(false)),
    }
}
