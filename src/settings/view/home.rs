//! Layout edits widgets on their wallpaper. Decorative windows and shell bars
//! belong to their own settings previews, not the widget placement canvas.
//! The landing picture opens an explicit local Canvas draft with Apply/Cancel.
use super::*;

/// A part of the picture you can click: outlined on hover or keyboard focus.
/// Keep the focus ring inside the hit area so the picture's clip retains it.
pub(in crate::settings) fn hotspot(accent: Color, radius: f32) -> cosmic::theme::Button {
    let style = move |focused: bool, hovered: bool| cosmic::widget::button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        border_color: if focused || hovered {
            accent
        } else {
            Color::TRANSPARENT
        },
        border_width: if focused {
            3.
        } else if hovered {
            2.
        } else {
            0.
        },
        border_radius: radius.into(),
        ..Default::default()
    };
    cosmic::theme::Button::Custom {
        active: Box::new(move |focused, _| style(focused, false)),
        hovered: Box::new(move |focused, _| style(focused, true)),
        pressed: Box::new(move |focused, _| style(focused, true)),
        disabled: Box::new(move |_| style(false, false)),
    }
}

pub(in crate::settings) fn widget_page(widget: Widget) -> Page {
    Page::parse(widget.key())
}

impl Settings {
    /// The Desktop landing: your real desktop, large, with everything on it
    /// one click away. Widgets are named right beside the picture.
    pub(super) fn overview_page(&self) -> Element<'_, Message> {
        if self.canvas_visible() {
            return self.canvas_page();
        }
        // Arranging happens on the desktop itself; the list in Settings
        // stays one click away for arranging with the keyboard.
        let header = split(
            heading("Widgets", "Click a widget in the picture to change it."),
            row::with_children(vec![
                button::text("Arrange with keyboard")
                    .id(cosmic::widget::Id::new("canvas-start"))
                    .on_press(Message::Canvas(crate::settings::canvas::Message::Start))
                    .into(),
                button::standard("Arrange widgets")
                    .id(cosmic::widget::Id::new("arrange-widgets"))
                    .on_press_maybe((!self.busy).then_some(Message::ArrangeWidgets))
                    .into(),
            ])
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        );
        // Your real wallpaper, bars and widgets; every object opens its editor.
        let height = (self.content_width() * 0.4).clamp(200., 340.);
        let scene = match self.profiles.preview() {
            Some(preview) => self.profile_scene(preview.clone(), height, true, true),
            None => self.placement_picture(height, false),
        };
        let mut parts = vec![header];
        let setup = self.needs_setup();
        if setup {
            parts.push(self.setup_card());
        }
        parts.extend(self.updated_card());
        parts.extend(self.widgets_stopped_banner(!setup));
        if self.displays.outputs().len() > 1 {
            parts.push(self.monitor_overview());
        }
        parts.push(
            container(scene)
                .id(cosmic::widget::Id::new("desktop-scene"))
                .width(Length::Fill)
                .into(),
        );
        parts.push(self.widgets_page());
        column::with_children(parts).spacing(20).into()
    }

    /// The real wallpaper behind the shared placement editor. The backdrop
    /// deliberately has no actions: clicking empty space clears selection.
    pub(in crate::settings) fn home_backdrop(&self, size: [f32; 2]) -> Vec<Element<'_, Message>> {
        // A missing image must not substitute Settings' own light/dark theme
        // for the desktop: transparent white clock text still needs dark ink
        // contrast. This neutral fallback is labelled by the canvas editor.
        // A soft diagonal wash rather than flat black or white, so the
        // fallback still reads as a desktop.
        // Light text sits on the same dusk sky as every other preview.
        let [top, middle, bottom] =
            if crate::desktop_theme::luminance(self.canvas_config().wallpaper_palette().foreground)
                > 0.18
            {
                [
                    Color::from_rgb(0.16, 0.24, 0.50),
                    Color::from_rgb(0.45, 0.29, 0.55),
                    Color::from_rgb(0.80, 0.47, 0.36),
                ]
            } else {
                [
                    Color::from_rgb(0.90, 0.92, 0.95),
                    Color::from_rgb(0.81, 0.84, 0.89),
                    Color::from_rgb(0.72, 0.76, 0.83),
                ]
            };
        let fallback = Background::Gradient(cosmic::iced::Gradient::Linear(
            cosmic::iced::gradient::Linear::new(cosmic::iced::Radians(2.4))
                .add_stop(0., top)
                .add_stop(0.55, middle)
                .add_stop(1., bottom),
        ));
        let background: Element<'_, Message> = match self.profiles.home_image() {
            Some(handle) => cosmic::widget::image(handle.clone())
                .width(size[0])
                .height(size[1])
                .content_fit(cosmic::iced::ContentFit::Cover)
                .border_radius(10.)
                .into(),
            None => container(text::body(""))
                .width(size[0])
                .height(size[1])
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        background: Some(fallback),
                        border: Border {
                            radius: 10.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                }))
                .into(),
        };
        vec![background]
    }
}
