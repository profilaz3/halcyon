//! The Clock page's stage: the screen the clock is on, with the clock where
//! it sits. Dragging the clock moves it on the desktop, and dragging its
//! corner handle resizes it. When the scene would
//! cover it a little way off, an outline marks that spot and a button moves
//! it there.
use super::*;
use crate::settings::clock_stage::{self as stage, Edit};
use cosmic::iced::widget::{pin, stack};
use cosmic::iced::{Point, mouse};

fn action(edit: Edit) -> Message {
    Message::ClockStage(edit)
}

/// The corner handle's side, in stage pixels.
const HANDLE: f32 = 16.;

/// What the stage says under the picture: the pointer and the keys.
pub(crate) const STAGE_HINT: &str = "Drag the clock to move it, or drag its corner to resize. \
With the keyboard, Tab selects the clock, arrow keys move it, + and − resize it, and Esc cancels a drag.";

/// The stage clock's resize handle: a small chip at its lower-right corner,
/// with the resize cursor. It is always there, faint until the pointer or
/// keyboard is on the clock, so the corner reads as something to grab.
fn resize_handle<'a>(accent: Color, strong: bool) -> Element<'a, Message> {
    let (fill, edge, mark) = if strong {
        (1., 1., 1.)
    } else {
        (0.55, 0.6, 0.85)
    };
    mouse_area(
        container(label(
            "◢",
            CAPTION,
            Color {
                a: mark,
                ..Color::WHITE
            },
        ))
        .width(HANDLE)
        .height(HANDLE)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .id(cosmic::widget::Id::new("clock-stage-handle"))
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: Some(Background::Color(Color { a: fill, ..accent })),
                border: Border {
                    color: Color {
                        a: edge,
                        ..Color::WHITE
                    },
                    width: 1.,
                    radius: [4., 0., 4., 0.].into(),
                },
                ..Default::default()
            }
        })),
    )
    .on_press(action(Edit::ResizePress))
    .interaction(mouse::Interaction::ResizingDiagonallyDown)
    .into()
}

impl Settings {
    /// The scene model: what it is, its size, that it stays on this
    /// computer, and the one thing to do with it now.
    pub(super) fn scene_model_row(&self) -> Element<'_, Message> {
        use crate::depth::model::{Error, SIZE};
        use crate::settings::depth::{ModelState, Scene};
        let mb = |bytes: u64| bytes / 1_000_000;
        let (state, button, event) = match self.scenes.model {
            ModelState::Missing => (
                "Downloads a 99 MB model from Hugging Face once. Your pictures are analyzed only on this computer, and nothing is uploaded.".into(),
                "Download",
                Scene::Download,
            ),
            ModelState::Downloading(bytes) => (
                format!("Downloading… {} of {} MB", mb(bytes), mb(SIZE)),
                "Cancel",
                Scene::Cancel,
            ),
            ModelState::Failed(Error::Cancelled) => (
                "Download cancelled. It’s 99 MB from Hugging Face, and your pictures are analyzed only on this computer.".into(),
                "Download",
                Scene::Download,
            ),
            ModelState::Failed(error) => (error.sentence().into(), "Try again", Scene::Download),
            ModelState::Ready => (
                "On this computer. Removing it also forgets the pictures it looked at.".into(),
                "Remove",
                Scene::Remove,
            ),
        };
        split(
            detail_state("Scene model", state),
            container(
                button::standard(button)
                    .on_press_maybe((!self.busy).then_some(Message::SceneModel(event))),
            )
            .id(cosmic::widget::Id::new("clock-scene-model"))
            .into(),
        )
    }
    pub(super) fn clock_stage_view(&self) -> Element<'_, Message> {
        let screen = stage::known(self.placement.preview_snapshot().screen);
        let cap = stage::stage_size([self.content_width(), f32::INFINITY], screen);
        let accent = self.settings_accent();
        let picture = cosmic::widget::responsive(move |available| {
            let size = stage::stage_size([available.width, cap[1]], screen);
            let [width, height] = size;
            let scale = stage::scale(size, screen);
            let backdrop: Element<'_, Message> =
                match (self.clock_stage.backdrop(), self.profiles.home_image()) {
                    // Rendered at the screen's shape with its own fit.
                    (Some(handle), _) => cosmic::widget::image(handle.clone())
                        .width(width)
                        .height(height)
                        .content_fit(cosmic::iced::ContentFit::Fill)
                        .border_radius(10.)
                        .into(),
                    // The Home picture: the same fit on a 16:9 screen, and the
                    // closest thing elsewhere until the stage has its own.
                    (None, Some(handle)) => cosmic::widget::image(handle.clone())
                        .width(width)
                        .height(height)
                        .content_fit(cosmic::iced::ContentFit::Cover)
                        .border_radius(10.)
                        .into(),
                    (None, None) => container(cosmic::iced::widget::space())
                        .width(width)
                        .height(height)
                        .class(cosmic::theme::Container::custom(|_| {
                            cosmic::iced::widget::container::Style {
                                background: Some(Background::Gradient(
                                    cosmic::iced::gradient::Linear::new(2.4)
                                        .add_stop(0., Color::from_rgb(0.16, 0.24, 0.50))
                                        .add_stop(0.55, Color::from_rgb(0.50, 0.32, 0.60))
                                        .add_stop(1., Color::from_rgb(0.93, 0.55, 0.40))
                                        .into(),
                                )),
                                border: Border {
                                    radius: 10.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }
                        }))
                        .into(),
                };
            // Iced draws the first stack layer directly; the picture goes in
            // a later one so its rounded corners clip.
            let mut layers: Vec<Element<'_, Message>> = vec![
                cosmic::widget::space().width(width).height(height).into(),
                backdrop,
            ];
            if self.config.widget_visible(Widget::Clock)
                && let Some((position, clock)) = self.clock_stage_place()
            {
                let frame = stage::frame(position, clock, screen);
                let shown = clock.map(|v| v * scale);
                if let Some(spot) = self.clock_stage.suggested(&frame)
                    && !self.clock_stage.dragging()
                {
                    let at = stage::to_stage(spot, scale);
                    layers.push(
                        pin(container(cosmic::iced::widget::space())
                            .width(shown[0])
                            .height(shown[1])
                            .class(cosmic::theme::Container::custom(move |_| {
                                cosmic::iced::widget::container::Style {
                                    background: Some(Background::Color(Color {
                                        a: 0.12,
                                        ..accent
                                    })),
                                    border: Border {
                                        color: accent,
                                        width: 1.5,
                                        radius: 5.into(),
                                    },
                                    ..Default::default()
                                }
                            })))
                        .position(Point::new(at[0], at[1]))
                        .into(),
                    );
                }
                let resizing = self.placement.resizing();
                let lit = self
                    .clock_stage
                    .lit(&frame)
                    .map(|handle| (handle, frame.origin));
                // Mid-resize, the last lit clock stretched to the new box
                // until the new size is drawn.
                let lit = lit.or_else(|| {
                    resizing
                        .then(|| self.clock_stage.last_lit())
                        .flatten()
                        .map(|handle| (handle, position))
                });
                let (content, at): (Element<'_, Message>, _) = match lit {
                    Some((handle, at)) => (
                        cosmic::widget::image(handle.clone())
                            .width(shown[0])
                            .height(shown[1])
                            .content_fit(cosmic::iced::ContentFit::Fill)
                            .into(),
                        at,
                    ),
                    None => (
                        profiles::card(Widget::Clock, shown, &self.config, scale),
                        position,
                    ),
                };
                // The button gives Tab focus, so arrow keys can move it;
                // its child owns the pointer press that starts a drag.
                // Enter or Space only selects it, which shows its outline.
                let body = button::custom(mouse_area(content).on_press(action(Edit::Press)))
                    .class(home::hotspot(accent, 5.))
                    .id(cosmic::widget::Id::new(stage::CLOCK_ID))
                    .on_press(action(Edit::Focus(true)))
                    .padding(0)
                    .width(shown[0])
                    .height(shown[1]);
                let mut parts: Vec<Element<'_, Message>> = vec![body.into()];
                let strong = self.clock_stage.handle();
                if strong {
                    // The box the handle resizes; a clock with no surface
                    // has no edge of its own.
                    parts.push(
                        container(cosmic::iced::widget::space())
                            .width(shown[0])
                            .height(shown[1])
                            .class(cosmic::theme::Container::custom(move |_| {
                                cosmic::iced::widget::container::Style {
                                    border: Border {
                                        color: Color { a: 0.7, ..accent },
                                        width: 1.,
                                        radius: 5.into(),
                                    },
                                    ..Default::default()
                                }
                            }))
                            .into(),
                    );
                }
                parts.push(
                    pin(resize_handle(accent, strong))
                        .position(Point::new(
                            (shown[0] - HANDLE).max(0.),
                            (shown[1] - HANDLE).max(0.),
                        ))
                        .into(),
                );
                // Hover comes from the stage's own pointer: an enter handler
                // here would swallow the move the stage needs.
                let body = mouse_area(stack(parts).width(shown[0]).height(shown[1])).interaction(
                    if resizing {
                        mouse::Interaction::ResizingDiagonallyDown
                    } else if self.clock_stage.dragging() {
                        mouse::Interaction::Grabbing
                    } else {
                        mouse::Interaction::Grab
                    },
                );
                let at = stage::to_stage(at, scale);
                layers.push(pin(body).position(Point::new(at[0], at[1])).into());
            }
            // A thin frame, like the edge of a screen.
            layers.push(
                container(cosmic::iced::widget::space())
                    .width(width)
                    .height(height)
                    .class(cosmic::theme::Container::custom(|_| {
                        cosmic::iced::widget::container::Style {
                            border: Border {
                                color: Color { a: 0.22, ..FG },
                                width: 1.,
                                radius: 10.into(),
                            },
                            ..Default::default()
                        }
                    }))
                    .into(),
            );
            container(
                mouse_area(stack(layers).width(width).height(height).clip(true))
                    .on_move(move |point| action(Edit::Pointer(point, scale)))
                    .on_exit(action(Edit::Hover(false))),
            )
            .id(cosmic::widget::Id::new("clock-stage"))
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .into()
        })
        .height(cap[1]);
        let mut parts = vec![picture.into()];
        if self.config.widget_visible(Widget::Clock) {
            let suggested = self.clock_stage_place().and_then(|(position, clock)| {
                self.clock_stage
                    .suggested(&stage::frame(position, clock, screen))
            });
            parts.push(
                container(label(STAGE_HINT, CAPTION, MUTED))
                    .id(cosmic::widget::Id::new("clock-stage-hint"))
                    .width(cap[0])
                    .into(),
            );
            let mut buttons: Vec<Element<'_, Message>> =
                vec![cosmic::widget::space().width(Length::Fill).into()];
            if suggested.is_some() {
                buttons.push(
                    button::suggested("Move to suggested spot")
                        .id(cosmic::widget::Id::new("clock-stage-suggested"))
                        .on_press_maybe(
                            (!self.clock_stage.dragging() && !self.busy)
                                .then_some(action(Edit::Suggested)),
                        )
                        .into(),
                );
            }
            parts.push(
                row::with_children(
                    buttons
                        .into_iter()
                        .chain([button::text("Reset position")
                            .id(cosmic::widget::Id::new("clock-stage-reset"))
                            .on_press_maybe(
                                (self.clock_stage_moved()
                                    && !self.clock_stage.dragging()
                                    && !self.busy)
                                    .then_some(action(Edit::Reset)),
                            )
                            .into()])
                        .collect::<Vec<_>>(),
                )
                .spacing(8)
                .align_y(Alignment::Center)
                .width(cap[0])
                .into(),
            );
        }
        column::with_children(parts)
            .spacing(8)
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .into()
    }
}
