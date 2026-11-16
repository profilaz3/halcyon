//! A 220 ms edge glint on pointer entry; no text animation or idle redraws.
use cosmic::iced::advanced::widget::{Operation, Tree, tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, overlay, renderer};
use cosmic::iced::{Event, Length, Rectangle, Size, Vector, mouse, window};
use cosmic::{Element, Renderer, Theme};

pub fn sheen<'a, Message: 'a>(
    content: Element<'a, Message>,
    animate: bool,
) -> Element<'a, Message> {
    Element::new(Sheen {
        content,
        animate,
        glass: None,
    })
}
pub fn dark_glass<'a, Message: 'a>(
    content: Element<'a, Message>,
    accent: cosmic::iced::Color,
    radius: f32,
    animate: bool,
) -> Element<'a, Message> {
    Element::new(Sheen {
        content,
        animate: false,
        glass: Some(
            cosmic::iced::widget::shader(super::dark_glass::Program {
                accent,
                radius,
                animate,
            })
            .into(),
        ),
    })
}
struct Sheen<'a, Message> {
    content: Element<'a, Message>,
    animate: bool,
    glass: Option<Element<'a, Message>>,
}
#[derive(Default)]
struct State {
    hover: bool,
    start: Option<std::time::Instant>,
}
impl State {
    fn strength(&self) -> f32 {
        self.start.map_or(0., |start| {
            (1. - start.elapsed().as_secs_f32() / 0.22).max(0.)
        })
    }
}
impl<Message> Widget<Message, Theme, Renderer> for Sheen<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        let mut children = vec![Tree::new(&self.content)];
        if let Some(glass) = &self.glass {
            children.push(Tree::new(glass));
        }
        children
    }
    fn diff(&mut self, tree: &mut Tree) {
        tree.children[0].diff(self.content.as_widget_mut());
        if let Some(glass) = &mut self.glass {
            if tree.children.len() == 1 {
                tree.children.push(Tree::new(&*glass));
            } else {
                tree.children[1].diff(glass.as_widget_mut());
            }
        } else {
            tree.children.truncate(1);
        }
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if let Some(glass) = &mut self.glass {
            glass.as_widget_mut().update(
                &mut tree.children[1],
                event,
                layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        let state = tree.state.downcast_mut::<State>();
        let hover = cursor.is_over(layout.bounds());
        if hover != state.hover {
            state.hover = hover;
            if self.animate && hover {
                state.start = Some(std::time::Instant::now());
                shell.request_redraw();
            }
        }
        if !self.animate {
            state.start = None;
        }
        if matches!(event, Event::Window(window::Event::RedrawRequested(_))) {
            if state.strength() > 0. {
                shell.request_redraw();
            } else {
                state.start = None;
            }
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        // Decoration is behind the card and its fully opaque text and controls.
        if let Some(glass) = &self.glass {
            glass.as_widget().draw(
                &tree.children[1],
                renderer,
                theme,
                style,
                layout,
                cursor,
                viewport,
            );
        }
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        let strength = if self.animate {
            tree.state.downcast_ref::<State>().strength()
        } else {
            0.
        };
        if strength > 0. {
            use cosmic::iced::advanced::Renderer as _;
            let b = layout.bounds();
            let bounds = Rectangle {
                x: b.x + 28.,
                y: b.y + 1.,
                width: (b.width - 56.).max(0.),
                height: 1.5,
            };
            if bounds.intersects(viewport) {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds,
                        border: cosmic::iced::Border {
                            radius: 1.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    cosmic::iced::Color::from_rgba(1., 1., 1., strength * 0.22),
                );
            }
        }
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
    fn drag_destinations(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        rectangles: &mut cosmic::iced::advanced::clipboard::DndDestinationRectangles,
    ) {
        self.content
            .as_widget()
            .drag_destinations(&tree.children[0], layout, renderer, rectangles);
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn material_sheen_settles_without_an_idle_animation_clock() {
        let mut state = State::default();
        assert_eq!(state.strength(), 0.);
        state.start = Some(std::time::Instant::now());
        assert!(state.strength() > 0.9);
        state.start = Some(std::time::Instant::now() - std::time::Duration::from_millis(221));
        assert_eq!(state.strength(), 0.);
    }
}
