//! Arrow-key navigation for the split sidebar, scoped to its focused buttons.
use super::Message;
use cosmic::iced::advanced::widget::{Operation, Tree, operation::Focusable};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, overlay, renderer};
use cosmic::iced::{Event, Length, Rectangle, Size, Vector, keyboard, mouse};
use cosmic::widget::Id;
use cosmic::{Element, Renderer, Theme};

pub(super) fn navigation<'a>(
    content: Element<'a, Message>,
    entries: Vec<(Id, Message)>,
) -> Element<'a, Message> {
    Element::new(Navigation { content, entries })
}

struct Navigation<'a> {
    content: Element<'a, Message>,
    entries: Vec<(Id, Message)>,
}

#[derive(Default)]
struct Focused(Option<Id>);
impl Operation for Focused {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }
    fn focusable(&mut self, id: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
        if state.is_focused() {
            self.0 = id.cloned();
        }
    }
}

struct Clicked {
    point: cosmic::iced::Point,
    id: Option<Id>,
}
impl Operation for Clicked {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }
    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, _: &mut dyn Focusable) {
        if bounds.contains(self.point) {
            self.id = id.cloned();
        }
    }
}

impl Widget<Message, Theme, Renderer> for Navigation<'_> {
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
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn a11y_nodes(
        &self,
        layout: Layout<'_>,
        tree: &Tree,
        cursor: mouse::Cursor,
    ) -> iced_accessibility::A11yTree {
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
        use keyboard::key::Named;
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key:
                keyboard::Key::Named(
                    key @ (Named::ArrowUp | Named::ArrowDown | Named::Home | Named::End),
                ),
            modifiers,
            ..
        }) = event
            && modifiers.is_empty()
        {
            let mut focused = Focused::default();
            self.operate(tree, layout, renderer, &mut focused);
            if let Some(index) = self
                .entries
                .iter()
                .position(|(id, _)| Some(id) == focused.0.as_ref())
            {
                let next = match key {
                    Named::ArrowUp => index.saturating_sub(1),
                    Named::ArrowDown => (index + 1).min(self.entries.len() - 1),
                    Named::Home => 0,
                    _ => self.entries.len() - 1,
                };
                let (id, open) = self.entries[next].clone();
                let mut focus = cosmic::iced::advanced::widget::operation::focusable::focus(id);
                self.operate(tree, layout, renderer, &mut focus);
                shell.publish(open);
                shell.capture_event();
                return;
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
        // COSMIC's ordinary buttons unfocus on a pointer press. Navigation
        // keeps the clicked entry focused so arrows can continue from it.
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) && let Some(point) = cursor.position()
            && layout.bounds().contains(point)
        {
            let mut clicked = Clicked { point, id: None };
            self.operate(tree, layout, renderer, &mut clicked);
            if let Some(id) = clicked.id {
                let mut focus = cosmic::iced::advanced::widget::operation::focusable::focus(id);
                self.operate(tree, layout, renderer, &mut focus);
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
