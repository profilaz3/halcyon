//! The shared color control: a swatch with its code, and under it, when
//! open, suggested colors and COSMIC's color picker.
use super::system_appearance::fill;
use super::*;
use crate::settings::color_well::{self as well, Target};

fn send(message: well::Message) -> Message {
    Message::ColorWell(message)
}

fn dot<'a>(rgb: Option<[f32; 3]>, size: u16) -> Element<'a, Message> {
    swatch(rgb, size, false)
}

/// A round swatch, with a check mark when it is the current color.
fn swatch<'a>(rgb: Option<[f32; 3]>, size: u16, checked: bool) -> Element<'a, Message> {
    let inside: Element<'a, Message> = if checked {
        // Dark on light colors, white on dark ones.
        let [r, g, b] = rgb.unwrap_or_default();
        let mark = if 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.5 {
            Color::from_rgb(0.04, 0.05, 0.1)
        } else {
            Color::WHITE
        };
        container(
            icon::from_name("object-select-symbolic")
                .size(14)
                .icon()
                .class(cosmic::theme::Svg::custom(move |_| {
                    cosmic::iced::widget::svg::Style { color: Some(mark) }
                })),
        )
        .center_x(size)
        .center_y(size)
        .into()
    } else {
        cosmic::iced::widget::space()
            .width(size)
            .height(size)
            .into()
    };
    fill(
        inside,
        rgb.map_or(Color::TRANSPARENT, well::color),
        Color { a: 0.35, ..FG },
        1.,
        f32::from(size) / 2.,
    )
    .into()
}

impl Settings {
    /// A swatch and its code; pressing it opens suggestions and the picker.
    pub(super) fn color_button(
        &self,
        id: impl Into<String>,
        target: Target,
        current: Option<[f32; 3]>,
        enabled: bool,
    ) -> Element<'_, Message> {
        let open = self
            .color_well
            .as_ref()
            .is_some_and(|well| well.target == target);
        button::custom(
            row::with_children(vec![
                dot(current, 20),
                label(
                    current.map_or_else(|| "Choose".to_string(), crate::settings::wallpaper::hex),
                    12.,
                    if current.is_some() { FG } else { MUTED },
                ),
                icon::from_name(if open {
                    "go-up-symbolic"
                } else {
                    "go-down-symbolic"
                })
                .size(12)
                .icon()
                .into(),
            ])
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .id(cosmic::widget::Id::new(id.into()))
        .class(chip_style(open, self.settings_accent()))
        .padding([4, 10, 4, 6])
        .on_press_maybe(enabled.then(|| send(well::Message::Open(target, current))))
        .into()
    }

    /// Under the control that opened it: the wallpaper's colors and a few
    /// calm ones, one click each, then the picker for anything else.
    pub(in crate::settings) fn color_panel(
        &self,
        target: Target,
        from_wallpaper: &[[f32; 3]],
    ) -> Option<Element<'_, Message>> {
        let open = self
            .color_well
            .as_ref()
            .filter(|well| well.target == target)?;
        // Each swatch says its code, on hover, on focus and to a screen
        // reader, and the one the control already has is checked.
        let chip = |rgb: [f32; 3], index: usize, group: &str| -> Element<'_, Message> {
            let code = crate::settings::wallpaper::hex(rgb);
            let current = open.is_current(rgb);
            let swatch = button::custom(swatch(Some(rgb), 26, current))
                .id(cosmic::widget::Id::new(format!("color-{group}-{index}")))
                .name(format!("Color: {code}"))
                .class(chip_style(current, self.settings_accent()))
                .padding(3)
                .on_press(send(well::Message::Pick(rgb)));
            super::tip::tip(swatch, &code)
        };
        let mut rows: Vec<Element<'_, Message>> = Vec::new();
        if !from_wallpaper.is_empty() {
            rows.push(label("From your wallpaper", 12., MUTED));
            rows.push(
                row::with_children(
                    from_wallpaper
                        .iter()
                        .take(8)
                        .enumerate()
                        .map(|(index, rgb)| chip(*rgb, index, "wallpaper")),
                )
                .spacing(4)
                .wrap()
                .into(),
            );
        }
        rows.push(label("Suggested", 12., MUTED));
        rows.push(
            row::with_children(
                well::QUICK
                    .iter()
                    .enumerate()
                    .map(|(index, bytes)| chip(well::rgb(*bytes), index, "quick")),
            )
            .spacing(4)
            .wrap()
            .into(),
        );
        rows.push(label("Any color", 12., MUTED));
        // The picker's switch and buttons fill their width; keep them to the
        // picker's own width.
        rows.push(
            container(
                open.model
                    .builder(|update| Message::ColorWell(well::Message::Picker(update)))
                    .width(Length::Fixed(300.))
                    .height(Length::Fixed(180.))
                    .save_label("Use color")
                    .cancel_label("Cancel")
                    .build("Recent colors", "Copy", "Copied"),
            )
            .width(Length::Fixed(300.))
            .into(),
        );
        Some(
            container(column::with_children(rows).spacing(8))
                .id(cosmic::widget::Id::new("color-panel"))
                .padding(14)
                .width(Length::Fill)
                .class(cosmic::theme::Container::Card)
                .into(),
        )
    }
}
