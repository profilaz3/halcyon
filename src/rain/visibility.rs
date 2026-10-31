//! Iced's Shader ignores the scroll viewport. Gate its update and draw so a
//! scrolled-out Settings preview does not keep requesting animation frames.
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, renderer, widget::Tree};
use cosmic::iced::{Event, Length, Rectangle, Size, mouse};
use cosmic::{Element, Renderer, Theme};
pub fn visible<'a, M: 'a>(child: Element<'a, M>) -> Element<'a, M> {
    Element::new(Visible { child })
}
struct Visible<'a, M> {
    child: Element<'a, M>,
}
fn intersects(bounds: Rectangle, viewport: &Rectangle) -> bool {
    bounds
        .intersection(viewport)
        .is_some_and(|r| r.width > 0. && r.height > 0.)
}
impl<M> Widget<M, Theme, Renderer> for Visible<'_, M> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.child));
    }
    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        if intersects(layout.bounds(), viewport) {
            self.child.as_widget_mut().update(
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
        if intersects(layout.bounds(), viewport) {
            self.child.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout,
                cursor,
                viewport,
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::iced::advanced::{clipboard, renderer::Headless};
    use cosmic::iced::{widget::Shader, window};
    #[test]
    fn effect_scrolled_out_stops_requests_and_resumes_when_visible() {
        let renderer = cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
            cosmic::font::default(),
            cosmic::iced::Pixels(14.),
            Some("tiny-skia"),
        ))
        .unwrap();
        let scene = crate::rain::gpu::Scene::new(crate::desktop::wallpaper::media::Pixels {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 255],
        });
        let mut element: Element<'_, ()> = visible(
            Shader::new(crate::rain::gpu::Program {
                scene,
                preferences: Default::default(),
                running: true,
                fixed_time: None,
            })
            .width(100.)
            .height(100.)
            .into(),
        );
        let mut tree = Tree::new(&element);
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(200., 200.)),
        );
        let event = Event::Window(window::Event::RedrawRequested(std::time::Instant::now()));
        for (y, should_redraw) in [(0., true), (101., false), (99., true)] {
            let viewport = Rectangle {
                x: 0.,
                y,
                width: 200.,
                height: 100.,
            };
            let mut messages = Vec::new();
            let mut shell = Shell::new(&mut messages);
            element.as_widget_mut().update(
                &mut tree,
                &event,
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &renderer,
                &mut clipboard::Null,
                &mut shell,
                &viewport,
            );
            assert_eq!(
                shell.redraw_request() != window::RedrawRequest::Wait,
                should_redraw
            );
        }
    }
}
