//! A column whose scrolling body is drawn before its fixed parts.
//!
//! The pinned libcosmic text input draws its value in a render layer of its
//! own, and a nested layer does not inherit the scrollable's clip. A field
//! scrolled across the edge of the scroll area therefore painted its text
//! over the pinned header and the sticky footer. Here the body is drawn
//! first and every other child afterwards in a later layer, on an opaque
//! window background, so the fixed parts cover anything that escapes.
//! Layout, events, focus and overlays are those of a plain column.
use super::Message;
use cosmic::iced::advanced::widget::{Operation, Tree};
use cosmic::iced::advanced::{
    Clipboard, Layout, Renderer as _, Shell, Widget, layout, overlay, renderer,
};
use cosmic::iced::{Alignment, Background, Color, Event, Length, Padding, Rectangle, Size, Vector};
use cosmic::iced::{mouse, widget::container};
use cosmic::{Element, Renderer, Theme};

pub(super) struct Layered<'a> {
    children: Vec<Element<'a, Message>>,
    body: usize,
}

/// `children` top to bottom; `body` is drawn first, the rest above it.
pub(super) fn layered<'a>(
    children: Vec<Element<'a, Message>>,
    body: usize,
) -> Element<'a, Message> {
    assert!(body < children.len());
    Element::new(Layered { children, body })
}

/// The window background, fully opaque, for the parts drawn above the body.
pub(super) fn opaque<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    cosmic::widget::container(content)
        .width(Length::Fill)
        .class(cosmic::theme::Container::custom(|theme: &Theme| {
            let base: Color = theme.cosmic().background(theme.transparent).base.into();
            container::Style {
                background: Some(Background::Color(Color { a: 1., ..base })),
                ..Default::default()
            }
        }))
        .into()
}

impl Widget<Message, Theme, Renderer> for Layered<'_> {
    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(self.children.as_mut_slice());
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::flex::resolve(
            layout::flex::Axis::Vertical,
            renderer,
            limits,
            Length::Fill,
            Length::Fill,
            Padding::ZERO,
            0.,
            Alignment::Start,
            &mut self.children,
            &mut tree.children,
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((child, state), layout) in self
                .children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                child
                    .as_widget_mut()
                    .operate(state, layout, renderer, operation);
            }
        });
    }

    fn a11y_nodes(
        &self,
        layout: Layout<'_>,
        tree: &Tree,
        cursor: mouse::Cursor,
    ) -> iced_accessibility::A11yTree {
        iced_accessibility::A11yTree::join(
            self.children
                .iter()
                .zip(&tree.children)
                .zip(layout.children())
                .map(|((child, state), layout)| child.as_widget().a11y_nodes(layout, state, cursor)),
        )
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
        for ((child, state), layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().update(
                state, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
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
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), layout)| {
                child
                    .as_widget()
                    .mouse_interaction(state, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
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
        let parts = || {
            self.children
                .iter()
                .zip(&tree.children)
                .zip(layout.children())
                .enumerate()
        };
        for (_, ((body, state), layout)) in parts().filter(|(index, _)| *index == self.body) {
            body.as_widget()
                .draw(state, renderer, theme, style, layout, cursor, viewport);
        }
        for (_, ((child, state), layout)) in parts().filter(|(index, _)| *index != self.body) {
            let Some(bounds) = layout.bounds().intersection(viewport) else {
                continue;
            };
            renderer.with_layer(bounds, |renderer| {
                child
                    .as_widget()
                    .draw(state, renderer, theme, style, layout, cursor, viewport);
            });
        }
    }

    fn drag_destinations(
        &self,
        state: &Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        dnd_rectangles: &mut cosmic::iced::advanced::clipboard::DndDestinationRectangles,
    ) {
        for ((child, child_layout), state) in self
            .children
            .iter()
            .zip(layout.children())
            .zip(state.children.iter())
        {
            child.as_widget().drag_destinations(
                state,
                child_layout.with_virtual_offset(layout.virtual_offset()),
                renderer,
                dnd_rectangles,
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
        overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}
