//! The Clock page's stage: the screen the clock is on, with the clock where
//! it sits. Dragging the clock moves it on the desktop.
use super::*;
use crate::settings::clock_stage::{self as stage, Edit};
use cosmic::iced::widget::{pin, stack};
use cosmic::iced::{Point, mouse};

fn action(edit: Edit) -> Message {
    Message::ClockStage(edit)
}

impl Settings {
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
                let (content, at): (Element<'_, Message>, _) = match self.clock_stage.lit(&frame) {
                    Some(handle) => (
                        cosmic::widget::image(handle.clone())
                            .width(shown[0])
                            .height(shown[1])
                            .content_fit(cosmic::iced::ContentFit::Fill)
                            .into(),
                        frame.origin,
                    ),
                    None => (
                        profiles::card(Widget::Clock, shown, &self.config, scale),
                        position,
                    ),
                };
                // The button gives Tab focus, so arrow keys can move it;
                // its child owns the pointer press that starts a drag.
                let body = button::custom(mouse_area(content).on_press(action(Edit::Press)))
                    .class(home::hotspot(accent, 5.))
                    .id(cosmic::widget::Id::new(stage::CLOCK_ID))
                    .on_press(Message::Preview)
                    .padding(0)
                    .width(shown[0])
                    .height(shown[1]);
                let body = mouse_area(body).interaction(if self.clock_stage.dragging() {
                    mouse::Interaction::Grabbing
                } else {
                    mouse::Interaction::Grab
                });
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
                    .on_move(move |point| action(Edit::Pointer(point, scale))),
            )
            .id(cosmic::widget::Id::new("clock-stage"))
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .into()
        })
        .height(cap[1]);
        let mut parts = vec![picture.into()];
        if self.config.widget_visible(Widget::Clock) {
            parts.push(
                row::with_children(vec![
                    container(label("Drag the clock to move it", 12., MUTED))
                        .width(Length::Fill)
                        .into(),
                    button::text("Reset position")
                        .id(cosmic::widget::Id::new("clock-stage-reset"))
                        .on_press_maybe(
                            (self.clock_stage_moved()
                                && !self.clock_stage.dragging()
                                && !self.busy)
                                .then_some(action(Edit::Reset)),
                        )
                        .into(),
                ])
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
