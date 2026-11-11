//! Short, interruptible disclosure motion. Redraws stop at rest; closed content
//! cannot receive input or focus. Reduced motion changes size immediately.
use super::Message;
use cosmic::iced::advanced::Renderer as _;
use cosmic::iced::advanced::widget::{Operation, Tree, tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, overlay, renderer};
use cosmic::iced::{Event, Length, Rectangle, Size, Vector, mouse, window};
use cosmic::{Element, Renderer, Theme};
use std::time::{Duration, Instant};

const DURATION: Duration = Duration::from_millis(160);
struct Progress {
    from: f32,
    to: f32,
    start: Instant,
}
impl Progress {
    fn new(open: bool, now: Instant) -> Self {
        let to = if open { 1. } else { 0. };
        Self {
            from: to,
            to,
            start: now,
        }
    }
    fn value(&self, now: Instant) -> f32 {
        let t = (now.saturating_duration_since(self.start).as_secs_f32() / DURATION.as_secs_f32())
            .min(1.);
        self.from + (self.to - self.from) * (1. - (1. - t).powi(3))
    }
    fn active(&self, now: Instant) -> bool {
        self.from != self.to && now.saturating_duration_since(self.start) < DURATION
    }
    fn set(&mut self, open: bool, animate: bool, now: Instant) {
        let to = if open { 1. } else { 0. };
        if !animate {
            self.from = to;
            self.to = to;
        } else if to != self.to {
            self.from = self.value(now);
            self.to = to;
            self.start = now;
        }
    }
}
struct Reveal<'a> {
    child: Element<'a, Message>,
    open: bool,
    animate: bool,
}
pub(super) fn reveal<'a>(
    child: Element<'a, Message>,
    open: bool,
    animate: bool,
) -> Element<'a, Message> {
    Element::new(Reveal {
        child,
        open,
        animate,
    })
}
impl Widget<Message, Theme, Renderer> for Reveal<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Progress>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(Progress::new(self.open, Instant::now()))
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }
    fn diff(&mut self, tree: &mut Tree) {
        tree.state
            .downcast_mut::<Progress>()
            .set(self.open, self.animate, Instant::now());
        tree.children[0].diff(&mut self.child);
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let child = self
            .child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        let progress = tree.state.downcast_ref::<Progress>().value(Instant::now());
        layout::Node::with_children(
            Size::new(child.size().width, child.size().height * progress),
            vec![child],
        )
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if self.open && !tree.state.downcast_ref::<Progress>().active(Instant::now()) {
            self.child.as_widget_mut().operate(
                &mut tree.children[0],
                layout.children().next().unwrap(),
                renderer,
                operation,
            );
        }
    }
    fn a11y_nodes(
        &self,
        layout: Layout<'_>,
        tree: &Tree,
        cursor: mouse::Cursor,
    ) -> iced_accessibility::A11yTree {
        if !self.open {
            return iced_accessibility::A11yTree::default();
        }
        self.child.as_widget().a11y_nodes(
            layout.children().next().unwrap(),
            &tree.children[0],
            cursor,
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
        let progress = tree.state.downcast_mut::<Progress>();
        if matches!(event, Event::Window(window::Event::RedrawRequested(_)))
            && progress.from != progress.to
        {
            shell.invalidate_layout();
            if progress.active(Instant::now()) {
                shell.request_redraw();
            } else {
                progress.from = progress.to;
            }
        }
        if self.open && !progress.active(Instant::now()) {
            self.child.as_widget_mut().update(
                &mut tree.children[0],
                event,
                layout.children().next().unwrap(),
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
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
        if self.open && !tree.state.downcast_ref::<Progress>().active(Instant::now()) {
            self.child.as_widget().mouse_interaction(
                &tree.children[0],
                layout.children().next().unwrap(),
                cursor,
                viewport,
                renderer,
            )
        } else {
            mouse::Interaction::default()
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
        if let Some(bounds) = layout
            .bounds()
            .intersection(viewport)
            .filter(|rect| rect.height > 0.)
        {
            renderer.with_layer(bounds, |renderer| {
                self.child.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    layout.children().next().unwrap(),
                    cursor,
                    &bounds,
                )
            });
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
        if self.open && !tree.state.downcast_ref::<Progress>().active(Instant::now()) {
            self.child.as_widget_mut().overlay(
                &mut tree.children[0],
                layout.children().next().unwrap(),
                renderer,
                viewport,
                translation,
            )
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disclosure_reverses_without_jumping_and_stops_at_rest() {
        let now = Instant::now();
        let mut p = Progress::new(false, now);
        p.set(true, true, now);
        let mid = now + Duration::from_millis(60);
        let value = p.value(mid);
        assert!(value > 0. && value < 1.);
        p.set(false, true, mid);
        assert_eq!(p.value(mid), value);
        assert_eq!(p.value(mid + DURATION), 0.);
        assert!(!p.active(mid + DURATION));
    }
    #[test]
    fn reduced_motion_finishes_immediately_even_during_transition() {
        let now = Instant::now();
        let mut p = Progress::new(false, now);
        p.set(true, true, now);
        p.set(true, false, now + Duration::from_millis(20));
        assert_eq!(p.value(now), 1.);
        assert!(!p.active(now));
        p.set(false, false, now);
        assert_eq!(p.value(now), 0.);
        assert!(!p.active(now));
    }
}
