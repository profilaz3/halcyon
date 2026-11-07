//! Explanations behind a small "?" beside a setting's title.
//!
//! Descriptions that only explain a setting stay out of the way: hovering
//! the "?" shows the text as a tooltip, and clicking it (or pressing Enter
//! while it has focus) shows it under the title until clicked again. Live
//! state never goes here; it stays visible through `detail_state`.
//!
//! The view functions that draw rows are free functions with no access to
//! `Settings`, so each frame records whether explanations are always shown
//! and which one is open before the page is built.
use super::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

thread_local! {
    static FRAME: Cell<Frame> = const { Cell::new(Frame { show: true, open: None }) };
    /// How often each explanation was drawn this frame, so a repeat gets
    /// its own widget id.
    static DRAWN: RefCell<HashMap<u64, u32>> = RefCell::new(HashMap::new());
}

/// What the page being built shows of each explanation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Frame {
    /// Every explanation is shown as a line under its title.
    pub show: bool,
    /// The one explanation opened with its "?".
    pub open: Option<u64>,
}

/// Sets what the next built page shows. Called once per frame before any
/// row is built.
pub(super) fn begin(frame: Frame) {
    FRAME.with(|current| current.set(frame));
    DRAWN.with(|drawn| drawn.borrow_mut().clear());
}

pub(super) fn current() -> Frame {
    FRAME.with(Cell::get)
}

/// A stable key for one explanation, the same on every frame. Only the
/// title counts: some explanations change with the row's value, and an open
/// one stays open while the value is changed.
pub(in crate::settings) fn key(title: &str) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    title.hash(&mut hasher);
    hasher.finish()
}

/// The widget id of an explanation's first "?" on a page, for tests and
/// focus.
pub(in crate::settings) fn id(key: u64) -> cosmic::widget::Id {
    cosmic::widget::Id::new(format!("explain-{key:016x}"))
}

/// The id for the next "?" with this key: the same text drawn twice on one
/// page still gets distinct widget ids.
fn next_id(key: u64) -> cosmic::widget::Id {
    let repeat = DRAWN.with(|drawn| {
        let mut drawn = drawn.borrow_mut();
        let count = drawn.entry(key).or_insert(0);
        *count += 1;
        *count
    });
    if repeat == 1 {
        id(key)
    } else {
        cosmic::widget::Id::new(format!("explain-{key:016x}-{repeat}"))
    }
}

fn style(open: bool) -> cosmic::theme::Button {
    let style = move |theme: &cosmic::Theme, focused: bool, hovered: bool| {
        let component = &theme.current_container().component;
        let accent: Color = theme.cosmic().accent.base.into();
        cosmic::widget::button::Style {
            background: Some(Background::Color(if open {
                Color { a: 0.16, ..accent }
            } else if hovered {
                component.hover.into()
            } else {
                Color::TRANSPARENT
            })),
            border_color: if open { accent } else { component.divider.into() },
            border_width: 1.,
            border_radius: 9.0.into(),
            outline_width: if focused { 2. } else { 0. },
            outline_color: accent,
            text_color: Some(if open {
                theme.cosmic().accent_text_color().into()
            } else {
                theme.cosmic().on_bg_color().into()
            }),
            ..Default::default()
        }
    };
    cosmic::theme::Button::Custom {
        active: Box::new(move |focused, theme| style(theme, focused, false)),
        hovered: Box::new(move |focused, theme| style(theme, focused, true)),
        pressed: Box::new(move |focused, theme| style(theme, focused, true)),
        disabled: Box::new(move |theme| style(theme, false, false)),
    }
}

/// The "?" beside a title: tooltip on hover, toggles the line on press.
fn button<'a>(key: u64, description: &str, open: bool) -> Element<'a, Message> {
    let mark = container(text::body("?").size(11).line_height(1.0))
        .center_x(Length::Fixed(18.))
        .center_y(Length::Fixed(18.));
    let control = cosmic::widget::button::custom(mark)
        .id(next_id(key))
        .padding(0)
        .class(style(open))
        .on_press(Message::Explain(key));
    // Open, the text is already on screen, so the tooltip says how to hide
    // it. Both states keep the same widgets, so keyboard focus stays put.
    let tip = if open { "Hide explanation" } else { description };
    cosmic::widget::tooltip(
        control,
        container(text::body(tip.to_owned()).size(12)).max_width(320),
        cosmic::widget::tooltip::Position::Top,
    )
    .padding(8)
    .gap(4)
    .into()
}

/// A title with its explanation behind a "?", or shown under it when the
/// frame says so.
pub(super) fn titled<'a>(
    title: Element<'a, Message>,
    title_text: &str,
    description: String,
    size: f32,
    spacing: u16,
) -> Element<'a, Message> {
    if description.is_empty() {
        return column::with_children(vec![title])
            .spacing(spacing)
            .width(Length::Fill)
            .into();
    }
    let frame = current();
    if frame.show {
        return column::with_children(vec![title, label(description, size, MUTED)])
            .spacing(spacing)
            .width(Length::Fill)
            .into();
    }
    let key = key(title_text);
    let open = frame.open == Some(key);
    // Wraps so a long title never squeezes the "?" to nothing.
    let head = row::with_children(vec![title, button(key, &description, open)])
        .spacing(6)
        .align_y(Alignment::Center)
        .wrap();
    let mut lines: Vec<Element<'a, Message>> = vec![head.into()];
    if open {
        lines.push(label(description, size, MUTED));
    }
    column::with_children(lines)
        .spacing(spacing)
        .width(Length::Fill)
        .into()
}
