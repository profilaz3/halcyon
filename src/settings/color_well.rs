//! One color control for every color in Settings: a swatch that opens
//! suggested colors and COSMIC's color picker. A chosen color is handed to
//! the owning editor as the same `#RRGGBB` text its field used to receive,
//! so drafts, Apply and Undo work as before.
use crate::desktop::appearance::ColorRole;
use cosmic::iced::Color;
use cosmic::widget::color_picker::{ColorPickerModel, ColorPickerUpdate};

/// Which color the open picker edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The shared accent on Colors & theme.
    Accent,
    /// A theme's own color while choosing "Your color".
    ThemeCustom,
    /// One of the advanced theme colors in Theme details, with the alpha
    /// it keeps (255 for opaque roles).
    Role(ColorRole, u8),
    /// A color of a solid or gradient wallpaper.
    Stop(usize),
    /// The color around a wallpaper shown with Fit.
    Fit,
}

#[derive(Clone, Debug)]
pub enum Message {
    /// Open the picker for a color, starting from its current value; opening
    /// the one already open closes it.
    Open(Target, Option<[f32; 3]>),
    Picker(ColorPickerUpdate),
    /// A suggested color, chosen in one click.
    Pick([f32; 3]),
}

pub struct Well {
    pub target: Target,
    pub model: ColorPickerModel,
    /// The color the control had when it opened; its swatch is checked.
    pub current: Option<[f32; 3]>,
}

/// Calm, readable colors offered beside a wallpaper's own suggestions.
pub const QUICK: [[u8; 3]; 10] = [
    [0x63, 0xD0, 0xDF],
    [0x94, 0xEB, 0xEB],
    [0x7B, 0xA4, 0xF4],
    [0xA9, 0x8B, 0xF2],
    [0xE8, 0x7F, 0xC1],
    [0xF2, 0x7F, 0x7F],
    [0xF4, 0xA2, 0x61],
    [0xF2, 0xD1, 0x6B],
    [0x9C, 0xD6, 0x7A],
    [0xA6, 0xAD, 0xB8],
];

pub fn rgb(bytes: [u8; 3]) -> [f32; 3] {
    bytes.map(|value| f32::from(value) / 255.)
}

pub fn color(rgb: [f32; 3]) -> Color {
    Color::from_rgb(rgb[0], rgb[1], rgb[2])
}

impl Well {
    pub fn open(target: Target, current: Option<[f32; 3]>) -> Self {
        let mut model = ColorPickerModel::new("Hex", "RGB", None, current.map(color));
        let _ = model.update::<()>(ColorPickerUpdate::ToggleColorPicker);
        Self {
            target,
            model,
            current,
        }
    }

    /// Whether `rgb` is the color the control already has, to the byte.
    pub fn is_current(&self, rgb: [f32; 3]) -> bool {
        self.current.is_some_and(|current| {
            crate::settings::wallpaper::hex(current) == crate::settings::wallpaper::hex(rgb)
        })
    }
}

/// What a picker event means for Settings.
pub enum Outcome {
    /// Nothing leaves the picker yet.
    Pending,
    /// Hand this color to the owning editor and close.
    Chosen(Target, [f32; 3]),
    Closed,
}

/// Applies `message` to the open picker, `well`.
pub fn update(well: &mut Option<Well>, message: Message) -> Outcome {
    match message {
        Message::Open(target, current) => {
            if well.as_ref().is_some_and(|open| open.target == target) {
                *well = None;
                return Outcome::Closed;
            }
            *well = Some(Well::open(target, current));
            Outcome::Pending
        }
        Message::Picker(ColorPickerUpdate::Cancel) => {
            *well = None;
            Outcome::Closed
        }
        Message::Pick(rgb) => match well.take() {
            Some(open) => Outcome::Chosen(open.target, rgb),
            None => Outcome::Closed,
        },
        // Dragging or pressing Enter only moves the color; "Use color"
        // chooses it, so the picker stays open while you explore.
        Message::Picker(ColorPickerUpdate::ActionFinished) => Outcome::Pending,
        Message::Picker(ColorPickerUpdate::AppliedColor) => {
            let Some(mut open) = well.take() else {
                return Outcome::Closed;
            };
            let _ = open.model.update::<()>(ColorPickerUpdate::AppliedColor);
            match open.model.get_applied_color() {
                Some(chosen) => Outcome::Chosen(open.target, [chosen.r, chosen.g, chosen.b]),
                None => Outcome::Closed,
            }
        }
        Message::Picker(update) => {
            if let Some(open) = well {
                // Copy to clipboard is the only update with a task; the
                // picker's own field still shows the code to select.
                let _ = open.model.update::<()>(update);
            }
            Outcome::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_current_color_is_checked_to_the_byte() {
        let well = Well::open(Target::Accent, Some(rgb(QUICK[2])));
        assert!(well.is_current(rgb(QUICK[2])));
        assert!(well.is_current([
            0x7B as f32 / 255. + 0.001,
            0xA4 as f32 / 255.,
            0xF4 as f32 / 255.
        ]));
        assert!(!well.is_current(rgb(QUICK[3])));
        assert!(!Well::open(Target::Accent, None).is_current(rgb(QUICK[2])));
    }

    #[test]
    fn opening_toggles_and_choices_close_with_the_target() {
        let mut well = None;
        assert!(matches!(
            update(&mut well, Message::Open(Target::Fit, Some([0., 0., 0.]))),
            Outcome::Pending
        ));
        assert!(well.as_ref().is_some_and(|open| open.model.get_is_active()));
        // Opening the same color again closes it.
        assert!(matches!(
            update(&mut well, Message::Open(Target::Fit, None)),
            Outcome::Closed
        ));
        assert!(well.is_none());
        let _ = update(&mut well, Message::Open(Target::Stop(1), None));
        let Outcome::Chosen(target, rgb) = update(&mut well, Message::Pick(rgb(QUICK[0]))) else {
            panic!("a suggestion is chosen in one click");
        };
        assert_eq!(target, Target::Stop(1));
        assert_eq!(rgb, super::rgb(QUICK[0]));
        assert!(well.is_none());
    }

    #[test]
    fn dragging_keeps_the_picker_open_and_use_color_returns_it() {
        let mut well = None;
        let _ = update(&mut well, Message::Open(Target::Accent, Some([1., 0., 0.])));
        let blue = cosmic::cosmic_theme::palette::Hsv::new_srgb(240.0, 1.0, 1.0);
        let _ = update(
            &mut well,
            Message::Picker(ColorPickerUpdate::ActiveColor(blue)),
        );
        assert!(matches!(
            update(
                &mut well,
                Message::Picker(ColorPickerUpdate::ActionFinished)
            ),
            Outcome::Pending
        ));
        assert!(well.is_some(), "still open after a drag");
        let Outcome::Chosen(Target::Accent, [r, g, b]) =
            update(&mut well, Message::Picker(ColorPickerUpdate::AppliedColor))
        else {
            panic!("Use color returns the picked color");
        };
        assert!(r < 0.01 && g < 0.01 && b > 0.99, "{r} {g} {b}");
        assert!(well.is_none());
        let _ = update(&mut well, Message::Open(Target::Accent, None));
        assert!(matches!(
            update(&mut well, Message::Picker(ColorPickerUpdate::Cancel)),
            Outcome::Closed
        ));
        assert!(well.is_none());
    }
}

/// The `#RRGGBB` (or `#RRGGBBAA` for a translucent theme color) an editor's
/// field receives for `rgb`.
pub fn text(target: Target, rgb: [f32; 3]) -> String {
    let hex = crate::settings::wallpaper::hex(rgb);
    match target {
        Target::Role(role, alpha) if role.alpha() && alpha < 255 => format!("{hex}{alpha:02X}"),
        _ => hex,
    }
}
