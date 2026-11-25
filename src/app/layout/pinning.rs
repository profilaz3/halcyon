//! Single-widget placement from the panel. Visibility and position are saved together.
use super::*;
use cosmic::iced::widget::canvas;

struct PinOutline {
    accent: Color,
    radius: f32,
    deadline: Option<Instant>,
}

const FADE: Duration = Duration::from_millis(350);

impl canvas::Program<Message, cosmic::Theme> for PinOutline {
    type State = ();

    fn update(
        &self,
        _: &mut (),
        event: &canvas::Event,
        _: cosmic::iced::Rectangle,
        _: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let canvas::Event::Window(window::Event::RedrawRequested(now)) = event else {
            return None;
        };
        let deadline = self.deadline?;
        if *now >= deadline {
            return None;
        }
        let fade_start = deadline - FADE;
        let next = if *now < fade_start {
            fade_start
        } else {
            (*now + Duration::from_millis(32)).min(deadline)
        };
        Some(canvas::Action::request_redraw_at(next))
    }

    fn draw(
        &self,
        _: &(),
        renderer: &cosmic::Renderer,
        _: &cosmic::Theme,
        bounds: cosmic::iced::Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let strength = self.deadline.map_or(1., |end| {
            (end.saturating_duration_since(Instant::now()).as_secs_f32() / FADE.as_secs_f32())
                .min(1.)
        });
        let size =
            cosmic::iced::Size::new((bounds.width - 2.).max(0.), (bounds.height - 2.).max(0.));
        let path = canvas::Path::rounded_rectangle(Point::new(1., 1.), size, self.radius.into());
        frame.stroke(
            &path,
            canvas::Stroke::default()
                .with_width(2.)
                .with_color(self.accent.scale_alpha(strength)),
        );
        vec![frame.into_geometry()]
    }
}

impl Cards {
    pub(in crate::app) fn pin_from_island(&mut self, widget: Widget) -> Task<Message> {
        if self
            .layout_editor
            .pinning
            .is_some_and(|(placing, _)| placing == widget)
        {
            return Task::none();
        }
        if self.layout_editor.active
            && self.layout_editor.pinning.is_none()
            && self.arranging_widget(widget)
        {
            self.layout_editor.selected = Some(widget);
            return self.layout_front();
        }
        if self.tool_shown(widget) {
            if self.placed_position(widget)[0] < 0. {
                self.raise_notice(
                    "Widget is already pinned",
                    "Make room for it in Arrange widgets.",
                    String::new(),
                    Some(Message::Arrange),
                );
                return Task::none();
            }
            return self.highlight_pin(widget);
        }
        let shown_elsewhere = format!(
            "{} is already on the desktop and appears when it has something to show.",
            widget.title()
        );
        let problem = if !self.config.widgets_visible {
            Some("Turn on desktop widgets in Halcyon Settings first.".to_string())
        } else if self.config.glance.enabled
            && matches!(widget, Widget::Clock | Widget::Weather | Widget::Calendar)
        {
            // Pinning would add a second copy beside the overview's own.
            Some("The Daily overview already shows it. Turn the overview off in Settings to pin it on its own.".to_string())
        } else if !self.output_known || !self.widget_available(widget) {
            Some(
                "The widget’s display is unavailable. Reconnect it or choose another display in Settings."
                    .to_string(),
            )
        } else if self.layout_editor.active
            || self.paused_layout.is_some()
            || self.pending_arrange.is_some()
        {
            Some(
                "Finish the current widget arrangement, then choose Pin to desktop again."
                    .to_string(),
            )
        } else if widget == Widget::Music && self.config.widget_visible(widget) {
            Some(
                "Music is already on the desktop and appears when a player has a track."
                    .to_string(),
            )
        } else if self.config.widget_visible(widget) {
            Some(shown_elsewhere)
        } else {
            None
        };
        if let Some(reason) = problem {
            self.raise_notice("Widget placement", reason, String::new(), None);
            return Task::none();
        }
        let display = self
            .widget_output(widget)
            .filter(|output| {
                Some(&output.handle) != self.selected_output.as_ref().map(|o| &o.handle)
            })
            .and_then(|output| self.displays.key(output));
        self.arrange_pick = None;
        self.pending_pin = Some(widget);
        match display {
            Some(key) => self.arrange_on_display(key),
            None => self.arrange(),
        }
    }

    fn highlight_pin(&mut self, widget: Widget) -> Task<Message> {
        let duration = std::time::Duration::from_millis(1250);
        let deadline = Instant::now() + duration;
        let previous = self.clear_pin_highlight();
        self.pin_highlight = Some((widget, deadline));
        Task::batch([
            previous.chain(set_layer(self.widget_id(widget), Layer::Top)),
            cosmic::task::future(async move {
                tokio::time::sleep(duration).await;
                Message::PinHighlightExpired(deadline)
            }),
        ])
    }

    pub(in crate::app) fn clear_pin_highlight(&mut self) -> Task<Message> {
        if let Some((widget, _)) = self.pin_highlight.take()
            && self.interacting_tool != Some(widget)
        {
            return set_layer(self.widget_id(widget), Layer::Bottom);
        }
        Task::none()
    }

    pub(super) fn move_pin(&mut self, widget: Widget, target: [f32; 2]) {
        self.layout_editor.pinning = Some((
            widget,
            crate::layout::clamp(target, self.widget_size(widget), self.screen_size),
        ));
    }

    /// Pointer and keyboard share the same preview and final snapped position.
    fn pin_position(&self, widget: Widget, at: [f32; 2]) -> Option<[f32; 2]> {
        let size = self.widget_size(widget);
        let at = if self.layout_editor.grid.snap {
            crate::layout::snap(at, size, self.screen_size, self.layout_editor.grid.columns)
        } else {
            at
        };
        let others: Vec<_> = Widget::ALL
            .into_iter()
            .filter(|other| *other != widget && self.arranging_widget(*other))
            .map(|other| {
                let [x, y] = self.position_from(other, &self.layout_editor.draft);
                let [w, h] = self.widget_size(other);
                [x, y, w, h]
            })
            .collect();
        crate::layout::free_spot(at, size, &others, self.screen_size)
    }

    pub(super) fn place_pin(&mut self) -> Task<Message> {
        let Some((widget, at)) = self.layout_editor.pinning else {
            return Task::none();
        };
        if !self.addable_widgets().contains(&widget) {
            return self.finish_arrange(false);
        }
        let Some(point) = self.pin_position(widget, at) else {
            return Task::none();
        };
        let encoded = crate::layout_state::encode_position(
            widget,
            point,
            self.widget_size(widget),
            self.screen_size,
        );
        self.layout_editor
            .draft
            .insert(widget.key().into(), encoded);
        self.layout_editor.shown.insert(widget.key().into(), true);
        let apply = self.commit_arrange_shown();
        if self.layout_editor.error.is_some() {
            return apply;
        }
        self.layout_editor.pinning = None;
        let finish = self.finish_arrange(true);
        Task::batch([apply, finish, self.highlight_pin(widget)])
    }

    pub(super) fn pin_outline<'a>(&self, card: Element<'a, Message>) -> Element<'a, Message> {
        let accent = self.config.surface_palette().accent;
        let radius = crate::widget_style::corner_radius(&self.config);
        let deadline = self
            .pin_highlight
            .filter(|_| self.config.material_motion && self.layout_editor.pinning.is_none())
            .map(|(_, d)| d);
        let outline = cosmic::iced::widget::canvas(PinOutline {
            accent,
            radius,
            deadline,
        })
        .width(Length::Fill)
        .height(Length::Fill);
        stack(vec![card, outline.into()]).into()
    }

    pub(super) fn pin_view(&self, widget: Widget, at: [f32; 2]) -> Element<'_, Message> {
        let free = self.pin_position(widget, at);
        let at = free.unwrap_or(at);
        let [w, h] = self.widget_size(widget);
        let card = container(self.tool_card(widget)).width(w).height(h);
        let guide = self.layout_editor.error.as_ref().map_or_else(
            || {
                if free.is_none() {
                    return "No room on this display. Press Esc, then move or resize a widget."
                        .into();
                }
                format!(
                    "Place {}  ·  Click or Enter to pin  ·  Arrow keys to move  ·  Esc to cancel",
                    widget.title()
                )
            },
            |error| {
                format!(
                    "Couldn’t pin {}. {error}  ·  Enter to retry  ·  Esc to cancel",
                    widget.title()
                )
            },
        );
        let hint = container(text::body(guide))
            .padding([9, 16])
            .class(cosmic::theme::Container::Card);
        let ground = cosmic::iced::widget::space()
            .width(Length::Fill)
            .height(Length::Fill);
        // The transparent top layer keeps controls in the preview from activating.
        stack(vec![
            ground.into(),
            pin(self.pin_outline(card.into()))
                .position(Point::new(at[0], at[1]))
                .into(),
            container(hint)
                .padding(12)
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .into(),
            opaque(
                cosmic::iced::widget::space()
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .into(),
        ])
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity_island::bridge::{Command, Event};
    use crate::displays::test_support::isolated;
    use cosmic::Application;

    #[test]
    fn pin_from_island_is_cancellable_persistent_and_idempotent() {
        if isolated(
            "app::layout::pinning::tests::pin_from_island_is_cancellable_persistent_and_idempotent",
        ) {
            return;
        }
        let config = Config::save_changes(serde_json::json!({
            "widgets_visible": true, "messages":{"enabled":false}, "activity_island":{"messages":true},
            "placement":{"snap":false}
        })).unwrap();
        let (mut desktop, _) = Cards::init(Core::default(), config);
        desktop.output_known = true;
        desktop.screen_size = [1366., 768.];
        let command = || Event::Command(Command::PinMessages);
        let _ = desktop.activity_message(command());
        assert!(desktop.layout_editor.pinning.is_some());
        let mut native = crate::native_test::Native::new(cosmic::iced::Size::new(1366., 768.));
        native.capture(
            desktop.layout_view(),
            &cosmic::Theme::dark(),
            "pin-messages-placement",
        );
        let title = native.text_bounds(desktop.layout_view(), "Messages");
        assert!(
            title.width > 0. && title.height > 0.,
            "the placement preview shows the real card"
        );

        let _ = desktop.layout_key(keyboard::Key::Named(keyboard::key::Named::Escape), false);
        assert!(!desktop.layout_editor.active);
        assert!(!Config::try_load().unwrap().widget_visible(Widget::Messages));
        let _ = desktop.activity_message(command());
        let _ = desktop.layout_pointer(mouse::Event::CursorMoved {
            position: Point::new(530., 360.),
        });
        let _ = desktop.layout_key(
            keyboard::Key::Named(keyboard::key::Named::ArrowRight),
            false,
        );
        let (widget, at) = desktop.layout_editor.pinning.unwrap();
        let expected = desktop.pin_position(widget, at).unwrap();
        let _ = desktop.layout_pointer(mouse::Event::ButtonPressed(mouse::Button::Left));
        assert!(!desktop.layout_editor.active);
        let saved = Config::try_load().unwrap();
        assert!(saved.widget_visible(widget));
        let point = crate::layout::decode(
            saved.widget_positions[widget.key()],
            desktop.widget_size(widget),
            desktop.screen_size,
        );
        assert!((point[0] - expected[0]).abs() < 1. && (point[1] - expected[1]).abs() < 1.);
        let bytes = std::fs::read(Config::path().unwrap()).unwrap();
        let _ = desktop.activity_message(command());
        assert!(!desktop.layout_editor.active, "already placed");
        assert_eq!(desktop.pin_highlight.map(|x| x.0), Some(widget));
        assert_eq!(std::fs::read(Config::path().unwrap()).unwrap(), bytes);
    }
}
