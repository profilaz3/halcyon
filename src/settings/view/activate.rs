//! Two small wrappers for Home's picture.
//!
//! [`activate`]: Space presses the focused part or tile. COSMIC buttons
//! answer Enter only, so a space press reaches them as Enter. [`quiet`]: a decorative copy drawn over the
//! picture to light the chosen part. It takes no events, has no focus or
//! accessible node, and is invisible to operations, so the real part below
//! it keeps the clicks and screen readers name each part once.
use super::Message;
use cosmic::iced::advanced::widget::Tree;
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, overlay, renderer};
use cosmic::iced::{Event, Length, Rectangle, Size, Vector, keyboard, mouse};
use cosmic::{Element, Renderer, Theme};

pub(super) fn activate<'a>(content: Element<'a, Message>) -> Element<'a, Message> {
    Element::new(Keys {
        content,
        quiet: false,
    })
}

/// Drawn only: see the module notes.
pub(super) fn quiet<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    Element::new(Keys {
        content: content.into(),
        quiet: true,
    })
}

struct Keys<'a> {
    content: Element<'a, Message>,
    quiet: bool,
}

impl Widget<Message, Theme, Renderer> for Keys<'_> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
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
        operation: &mut dyn cosmic::iced::advanced::widget::Operation,
    ) {
        if !self.quiet {
            self.content
                .as_widget_mut()
                .operate(&mut tree.children[0], layout, renderer, operation);
        }
    }
    fn a11y_nodes(
        &self,
        layout: Layout<'_>,
        tree: &Tree,
        cursor: mouse::Cursor,
    ) -> iced_accessibility::A11yTree {
        if self.quiet {
            return iced_accessibility::A11yTree::default();
        }
        self.content
            .as_widget()
            .a11y_nodes(layout, &tree.children[0], cursor)
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
        if self.quiet {
            return;
        }
        let mut space = None;
        if let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event
            && *key == keyboard::Key::Character(" ".into())
            && modifiers.is_empty()
        {
            let mut enter = event.clone();
            if let Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = &mut enter {
                *key = keyboard::Key::Named(keyboard::key::Named::Enter);
            }
            space = Some(enter);
        }
        let event = space.as_ref().unwrap_or(event);
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
        // A copy never shows hover: the part below it owns the pointer.
        let cursor = if self.quiet {
            mouse::Cursor::Unavailable
        } else {
            cursor
        };
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.quiet {
            return mouse::Interaction::None;
        }
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
        if !self.quiet {
            self.content.as_widget().drag_destinations(
                &tree.children[0],
                layout,
                renderer,
                rectangles,
            );
        }
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        if self.quiet {
            return None;
        }
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}
