//! Shared widget editor and wallpaper stage. The card is the desktop renderer;
//! the cached wallpaper is decoded by the existing background worker.
use super::*;

impl Settings {
    pub(super) fn preview_stage<'a>(&self, card: Element<'a, Message>) -> Element<'a, Message> {
        container(self.preview_stage_padded(card, 14))
            .id(cosmic::widget::Id::new("widget-preview-stage"))
            .width(Length::Fill)
            .into()
    }
    pub(super) fn preview_stage_padded<'a>(
        &self,
        card: Element<'a, Message>,
        padding: u16,
    ) -> Element<'a, Message> {
        let background: Element<'a, Message> = match self.profiles.home_image() {
            Some(handle) => cosmic::widget::image(handle.clone())
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(cosmic::iced::ContentFit::Cover)
                .border_radius(10.)
                .into(),
            None => container(cosmic::iced::widget::space())
                .width(Length::Fill)
                .height(Length::Fill)
                // Without a picture, a dusk sky: glass and tints need
                // something colorful behind them to look like anything.
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
        container(
            cosmic::iced::widget::Stack::new()
                .push(
                    container(card)
                        .padding(padding)
                        .width(Length::Fill)
                        .align_x(Alignment::Center),
                )
                .push_under(background)
                // Iced's first stack layer draws directly. Keep the image in a
                // later layer so Cover is clipped by a real renderer layer.
                .push_under(
                    cosmic::iced::widget::space()
                        .width(Length::Fill)
                        .height(Length::Fill),
                )
                .width(Length::Fill)
                .clip(true),
        )
        .width(Length::Fill)
        .into()
    }
}
