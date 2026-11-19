//! A transient layer on the invoking display. Cards share the desktop's data and actions.
use super::*;
use crate::config::edge_menu::{Item, Kind};
use cosmic::widget::button;

pub(super) struct Peek {
    pub id: window::Id,
    pub output: Option<crate::displays::WlOutput>,
    pub widgets: Vec<Widget>,
    pub page: usize,
    title: String,
    pub size: [f32; 2],
    pub started: Instant,
    pub entering: bool,
}

impl Cards {
    pub(super) fn close_peek(&mut self) -> Task<Message> {
        let save = if self.peek_note_surface().is_some() {
            self.note_focus_lost()
        } else {
            Task::none()
        };
        let close = self
            .peek
            .take()
            .map_or_else(Task::none, |p| edge::destroy(p.id));
        Task::batch([save, close])
    }

    pub(super) fn toggle_peek(
        &mut self,
        requested: Option<(String, Vec<Widget>)>,
    ) -> Task<Message> {
        if self.peek.is_some() {
            return self.close_peek();
        }
        let choice = |item: Item| match item.kind {
            Kind::Peek { name, widgets, .. } => Some((name, widgets)),
            _ => None,
        };
        let Some((title, widgets)) = requested
            .or_else(|| self.config.edge_items().into_iter().find_map(choice))
            .or_else(|| choice(Item::peek()))
        else {
            return Task::none();
        };
        if self.rain.lifecycle.locked || self.rain.lifecycle.sleeping || widgets.is_empty() {
            return Task::none();
        }
        let id = window::Id::unique();
        let output = self
            .edge
            .menu_output
            .as_ref()
            .or(self.selected_output.as_ref());
        let size = output
            .and_then(|o| o.info.logical_size)
            .map_or(self.screen_size, |(w, h)| [w as f32, h as f32]);
        let target = output.map_or_else(
            || SctkLayerSurfaceSettings::default().output,
            |o| IcedOutput::Output(o.handle.clone()),
        );
        self.peek = Some(Peek {
            id,
            output: output.map(|o| o.handle.clone()),
            widgets,
            title,
            page: 0,
            size,
            started: Instant::now(),
            entering: self.config.settings_animations,
        });
        self.sidebar.hide();
        get_layer_surface(SctkLayerSurfaceSettings {
            id,
            output: target,
            layer: Layer::Overlay,
            anchor: Anchor::LEFT | Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM,
            size: Some((Some(0), Some(0))),
            exclusive_zone: -1,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            namespace: "halcyon-widget-peek".into(),
            ..Default::default()
        })
    }

    fn peek_page_end(&self, peek: &Peek, first: usize) -> usize {
        let mut width = 0.;
        let available = (peek.size[0] - 64.).max(160.);
        for (at, &widget) in peek.widgets.iter().enumerate().skip(first) {
            let w = self.widget_size(widget)[0].min(available);
            if at != first && width + 12. + w > available {
                return at;
            }
            width += w + if at == first { 0. } else { 12. };
        }
        peek.widgets.len()
    }

    pub(super) fn peek_note_surface(&self) -> Option<window::Id> {
        let peek = self.peek.as_ref()?;
        let end = self.peek_page_end(peek, peek.page);
        peek.widgets[peek.page..end]
            .contains(&Widget::Notes)
            .then_some(peek.id)
    }

    pub(super) fn peek_view<'a>(&'a self, peek: &'a Peek) -> Element<'a, Message> {
        let end = peek.widgets.len();
        let first = peek.page.min(end.saturating_sub(1));
        let next = self.peek_page_end(peek, first);
        let available = (peek.size[0] - 64.).max(160.);
        let mut cards = Vec::new();
        let mut width = 0.;
        for &widget in &peek.widgets[first..next] {
            let [w, h] = self.widget_size(widget);
            let w = w.min(available);
            let h = h.min((peek.size[1] - 112.).max(90.));
            let card = if widget.is_tool() {
                self.tool_card_at(widget, [w, h])
            } else {
                self.widget_face(widget)
            };
            cards.push(container(card).width(w).height(h).clip(true).into());
            width += w + if cards.len() > 1 { 12. } else { 0. };
        }
        let palette = self.config.palette();
        let mut header = vec![
            text::title4(&peek.title)
                .class(cosmic::theme::Text::Color(palette.foreground))
                .into(),
            cosmic::widget::Space::new().width(Length::Fill).into(),
        ];
        if first > 0 {
            let mut previous = 0;
            while self.peek_page_end(peek, previous) < first {
                previous = self.peek_page_end(peek, previous);
            }
            header.push(
                button::text("Previous")
                    .on_press(Message::PeekPage(previous))
                    .into(),
            );
        }
        if next < end {
            header.push(
                button::text("More widgets")
                    .on_press(Message::PeekPage(next))
                    .into(),
            );
        }
        header.push(
            button::standard("Close")
                .on_press(Message::Peek(None))
                .into(),
        );
        let tray = column::with_children(vec![
            row::with_children(header)
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            row::with_children(cards)
                .spacing(12)
                .align_y(Alignment::Center)
                .into(),
        ])
        .spacing(12);
        let panel = mouse_area(
            container(tray)
                .id("peek-controls")
                .width((width + 28.).min(peek.size[0].max(200.)))
                .padding(14)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        text_color: Some(palette.foreground),
                        background: Some(Background::Color(Color {
                            a: 0.96,
                            ..palette.background
                        })),
                        border: Border {
                            radius: 20.0.into(),
                            width: 1.0,
                            color: Color {
                                a: 0.20,
                                ..palette.foreground
                            },
                        },
                        ..Default::default()
                    }
                })),
        )
        .on_press(Message::PeekInside);
        let opacity = if peek.entering {
            (peek.started.elapsed().as_secs_f32() / 0.116).clamp(0., 1.)
        } else {
            1.
        };
        let backdrop = mouse_area(
            container(text::body(""))
                .width(Length::Fill)
                .height(Length::Fill)
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        background: Some(Background::Color(Color::from_rgba(
                            0.02,
                            0.025,
                            0.035,
                            0.54 * opacity,
                        ))),
                        ..Default::default()
                    }
                })),
        )
        .on_press(Message::Peek(None));
        cosmic::iced::widget::Stack::with_children(vec![
            backdrop.into(),
            container(panel)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into(),
        ])
        .into()
    }
}
