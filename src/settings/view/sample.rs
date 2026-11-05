//! Fits the actual card renderer into a compact, non-interactive sample.
//! Live Music/Notes/Focus controls never pass through this widget.
use super::*;
use cosmic::iced::advanced::Renderer as _;
use cosmic::iced::advanced::{Layout, Widget, layout, renderer, widget::Tree};
use cosmic::iced::{Rectangle, Size, Transformation, mouse};

pub(super) fn fit<'a>(child: Element<'a, Message>, height: f32) -> Element<'a, Message> {
    Element::new(Sample {
        child,
        height,
        source: None,
    })
}

/// Scale the real component at its logical viewport into a desktop miniature.
/// Like compact samples, this adapter deliberately has no event forwarding.
pub(super) fn thumbnail<'a>(
    child: Element<'a, Message>,
    source: [f32; 2],
    target: [f32; 2],
) -> Element<'a, Message> {
    container(Element::new(Sample {
        child,
        height: target[1],
        source: Some(Size::new(source[0], source[1])),
    }))
    .width(target[0])
    .height(target[1])
    .into()
}

struct Sample<'a> {
    child: Element<'a, Message>,
    height: f32,
    source: Option<Size>,
}

impl Widget<Message, cosmic::Theme, cosmic::Renderer> for Sample<'_> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.child));
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let child = self.child.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &self.source.map_or_else(
                || layout::Limits::new(Size::new(360., 0.), Size::new(360., 2000.)),
                |size| layout::Limits::new(size, size),
            ),
        );
        let size = child.size();
        let scale = (limits.max().width / size.width.max(1.))
            .min(self.height / size.height.max(1.))
            .min(1.);
        layout::Node::with_children(
            Size::new(limits.max().width, size.height * scale),
            vec![child],
        )
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut cosmic::Renderer,
        theme: &cosmic::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        _: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let child = layout.children().next().unwrap();
        let original = child.bounds();
        let scale = (bounds.width / original.width.max(1.))
            .min(self.height / original.height.max(1.))
            .min(1.);
        let transform = Transformation::translate(
            bounds.x + (bounds.width - original.width * scale) / 2.,
            bounds.y,
        ) * Transformation::scale(scale)
            * Transformation::translate(-original.x, -original.y);
        if let Some(clip) = bounds.intersection(viewport) {
            // A faint outline of the card's own box, so a widget without a
            // background still reads as placed on purpose, not off-centre.
            if self.source.is_none() {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x: bounds.x + (bounds.width - original.width * scale) / 2.,
                            y: bounds.y,
                            width: original.width * scale,
                            height: original.height * scale,
                        },
                        border: cosmic::iced::Border {
                            color: Color { a: 0.14, ..FG },
                            width: 1.,
                            radius: (12. * scale).into(),
                        },
                        ..Default::default()
                    },
                    Color::TRANSPARENT,
                );
            }
            renderer.with_layer(clip, |renderer| {
                renderer.with_transformation(transform, |renderer| {
                    self.child.as_widget().draw(
                        &tree.children[0],
                        renderer,
                        theme,
                        style,
                        child,
                        mouse::Cursor::Unavailable,
                        &original,
                    );
                })
            });
        }
    }
}
