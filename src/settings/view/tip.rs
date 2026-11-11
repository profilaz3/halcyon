//! Tooltips that also appear on keyboard focus. COSMIC's tooltips open only
//! under the pointer, so a keyboard user never saw what an icon button or a
//! "?" says, and they hide their control from screen readers. [`tip`] shows
//! its words above the control while the pointer is over it or it has
//! focus, and leaves the control in the accessibility tree.
use super::Message;
use cosmic::iced::advanced::widget::{Operation, Tree, operation, tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, overlay, renderer};
use cosmic::iced::{Event, Length, Point, Rectangle, Size, Vector, mouse};
use cosmic::widget::{container, text};
use cosmic::{Element, Renderer, Theme};
use iced_accessibility::A11yTree;

/// The widest a tooltip grows before its text wraps.
const MAX_WIDTH: f32 = 320.;
/// The space between a control and its tooltip.
const GAP: f32 = 4.;

fn label<'a>(words: &str) -> Element<'a, Message> {
    container(text::body(words.to_owned()).size(12))
        .max_width(MAX_WIDTH)
        .padding(8)
        .class(cosmic::theme::Container::Tooltip)
        .into()
}

/// `content` with `words` as its tooltip, on hover and on keyboard focus.
pub(in crate::settings) fn tip<'a>(
    content: impl Into<Element<'a, Message>>,
    words: &str,
) -> Element<'a, Message> {
    Element::new(Tip {
        content: content.into(),
        tip: label(words),
    })
}

struct Tip<'a> {
    content: Element<'a, Message>,
    tip: Element<'a, Message>,
}

/// Whether anything inside has keyboard focus.
struct Focused(bool);
impl Operation for Focused {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn focusable(
        &mut self,
        _id: Option<&cosmic::widget::Id>,
        _bounds: Rectangle,
        state: &mut dyn operation::Focusable,
    ) {
        self.0 |= state.is_focused();
    }
}

/// Whether the pointer is over the control.
#[derive(Default)]
struct Hovered(bool);

impl Widget<Message, Theme, Renderer> for Tip<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Hovered>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(Hovered::default())
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content), Tree::new(&self.tip)]
    }
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut [&mut self.content, &mut self.tip]);
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
        if let Event::Mouse(_) = event {
            let over = cursor.is_over(layout.bounds());
            let hovered = tree.state.downcast_mut::<Hovered>();
            if hovered.0 != over {
                hovered.0 = over;
                shell.request_redraw();
            }
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
    fn a11y_nodes(&self, layout: Layout<'_>, tree: &Tree, cursor: mouse::Cursor) -> A11yTree {
        self.content
            .as_widget()
            .a11y_nodes(layout, &tree.children[0], cursor)
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let hovered = tree.state.downcast_ref::<Hovered>().0;
        let [content, tip] = &mut tree.children[..] else {
            return None;
        };
        let mut focused = Focused(false);
        self.content
            .as_widget_mut()
            .operate(content, layout, renderer, &mut focused);
        // A list the control opened wins over its tooltip.
        let inner =
            self.content
                .as_widget_mut()
                .overlay(content, layout, renderer, viewport, translation);
        if inner.is_some() || !(hovered || focused.0) {
            return inner;
        }
        Some(overlay::Element::new(Box::new(Shown {
            tip: &mut self.tip,
            tree: tip,
            anchor: layout.bounds() + translation,
        })))
    }
}

impl<'a> From<Tip<'a>> for Element<'a, Message> {
    fn from(tip: Tip<'a>) -> Self {
        Element::new(tip)
    }
}

/// The tooltip of a focused control, above it, or below it at the top edge.
struct Shown<'b, 'a> {
    tip: &'b mut Element<'a, Message>,
    tree: &'b mut Tree,
    anchor: Rectangle,
}

/// Where a `size` tooltip goes for a control at `anchor` in a `window`.
fn place(anchor: Rectangle, size: Size, window: Size) -> Point {
    let x = (anchor.center_x() - size.width / 2.)
        .min(window.width - size.width)
        .max(0.);
    let above = anchor.y - GAP - size.height;
    let y = if above >= 0. {
        above
    } else {
        anchor.y + anchor.height + GAP
    };
    Point::new(x, y)
}

impl overlay::Overlay<Message, Theme, Renderer> for Shown<'_, '_> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let widest = Size::new(MAX_WIDTH.min(bounds.width), bounds.height);
        let limits = layout::Limits::new(Size::ZERO, widest);
        let node = self
            .tip
            .as_widget_mut()
            .layout(self.tree, renderer, &limits);
        let at = place(self.anchor, node.size(), bounds);
        node.move_to(at)
    }
    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.tip.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &layout.bounds(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tip_sits_above_its_control_and_inside_the_window() {
        let window = Size::new(820., 600.);
        let size = Size::new(120., 30.);
        let middle = Rectangle::new(Point::new(400., 300.), Size::new(20., 20.));
        assert_eq!(place(middle, size, window), Point::new(350., 266.));
        // At the top edge it goes below; at the right edge it moves left.
        let corner = Rectangle::new(Point::new(800., 4.), Size::new(20., 20.));
        assert_eq!(place(corner, size, window), Point::new(700., 28.));
    }
}
