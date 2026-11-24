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
        animate,
        ..Sheen::plain(content)
    })
}

/// Report the sleeve's measured rectangle in Music-card coordinates. The
/// same named container follows the cover in compact, wide and recipe faces.
pub fn observe_artwork<'a, M: 'a>(
    content: Element<'a, M>,
    report: fn(Option<Rectangle>) -> M,
) -> Element<'a, M> {
    Element::new(Sheen {
        report_artwork: Some((cosmic::widget::Id::new("music-artwork"), report)),
        ..Sheen::plain(content)
    })
}
pub fn dark_glass<'a, Message: 'a>(
    content: Element<'a, Message>,
    accent: cosmic::iced::Color,
    radius: f32,
    backing: f32,
    animate: bool,
) -> Element<'a, Message> {
    Element::new(Sheen {
        content,
        report_artwork: None,
        animate: false,
        backing: (backing > 0.).then_some((radius, backing)),
        album: None,
        glass: Some(
            cosmic::iced::widget::shader(super::dark_glass::Program {
                accent,
                backing,
                radius,
                animate,
            })
            .into(),
        ),
    })
}
pub fn album_light<'a, M: 'a>(
    content: Element<'a, M>,
    program: super::dark_glass::AlbumLight,
) -> Element<'a, M> {
    Element::new(Sheen {
        content,
        report_artwork: None,
        animate: false,
        backing: None,
        album: Some((program.artwork_id.clone(), program.spread, program.radius)),
        glass: Some(cosmic::iced::widget::shader(program).into()),
    })
}
struct Sheen<'a, Message> {
    content: Element<'a, Message>,
    animate: bool,
    glass: Option<Element<'a, Message>>,
    album: Option<(cosmic::widget::Id, f32, f32)>,
    backing: Option<(f32, f32)>,
    report_artwork: Option<(cosmic::widget::Id, fn(Option<Rectangle>) -> Message)>,
}
impl<'a, M> Sheen<'a, M> {
    fn plain(content: Element<'a, M>) -> Self {
        Self {
            content,
            animate: false,
            glass: None,
            album: None,
            backing: None,
            report_artwork: None,
        }
    }
}
struct FindArtwork<'a> {
    id: &'a cosmic::widget::Id,
    bounds: Option<Rectangle>,
}
impl Operation for FindArtwork<'_> {
    fn container(&mut self, id: Option<&cosmic::widget::Id>, bounds: Rectangle) {
        if id == Some(self.id) {
            self.bounds = Some(bounds);
        }
    }

    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        if self.bounds.is_none() {
            visit(self);
        }
    }
}
#[derive(Default)]
struct State {
    hover: bool,
    software_mask: std::cell::RefCell<Option<([f32; 3], cosmic::iced::advanced::svg::Handle)>>,
    start: Option<std::time::Instant>,
    artwork: Option<Rectangle>,
    reported_artwork: Option<Option<Rectangle>>,
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
        let content = self.content.as_widget_mut();
        let node = content.layout(&mut tree.children[0], renderer, limits);
        let artwork_id = self
            .album
            .as_ref()
            .map(|(id, ..)| id)
            .or_else(|| self.report_artwork.as_ref().map(|(id, _)| id));
        if let Some(id) = artwork_id {
            let mut artwork = FindArtwork { id, bounds: None };
            let layout = Layout::new(&node);
            content.operate(&mut tree.children[0], layout, renderer, &mut artwork);
            let local = artwork.bounds.map(|mut b| {
                b.x -= layout.position().x;
                b.y -= layout.position().y;
                b
            });
            if self.album.is_some() {
                tree.children[1]
                    .state
                    .downcast_mut::<super::dark_glass::AlbumState>()
                    .center = local.map(|b| b.center());
            }
            if self.report_artwork.is_some() {
                tree.state.downcast_mut::<State>().artwork = local;
            }
        }
        node
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
        if let Some((_, report)) = &self.report_artwork
            && matches!(event, Event::Window(window::Event::RedrawRequested(_)))
            && state.reported_artwork != Some(state.artwork)
        {
            state.reported_artwork = Some(state.artwork);
            shell.publish(report(state.artwork));
        }
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
        if let Some((_, spread, radius)) = &self.album
            && matches!(renderer, Renderer::Secondary(_))
        {
            let state = tree.children[1]
                .state
                .downcast_ref::<super::dark_glass::AlbumState>();
            state.draw_software(renderer, layout.bounds(), viewport, *spread, *radius);
        } else if let Some(glass) = &self.glass {
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
        if let Some((radius, alpha)) = self.backing
            && matches!(renderer, Renderer::Secondary(_))
        {
            use cosmic::iced::advanced::svg::{Handle, Renderer as _, Svg};
            let bounds = layout.bounds();
            if let Some(clip) = bounds.intersection(viewport) {
                let (w, h) = (bounds.width, bounds.height);
                let shape = [w, h, radius];
                let mut mask = tree.state.downcast_ref::<State>().software_mask.borrow_mut();
                if mask.as_ref().is_none_or(|(key, _)| *key != shape) {
                    let inset = 6_f32.min(0.25 * w.min(h));
                    let (iw, ih, r) = (w - 2. * inset, h - 2. * inset, (radius - inset).max(0.));
                    let svg = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><defs><filter id="f" filterUnits="userSpaceOnUse" x="0" y="0" width="{w}" height="{h}"><feGaussianBlur stdDeviation="3"/></filter><clipPath id="c"><rect width="{w}" height="{h}" rx="{radius}"/></clipPath></defs><g clip-path="url(#c)"><rect x="{inset}" y="{inset}" width="{iw}" height="{ih}" rx="{r}" filter="url(#f)"/></g></svg>"##);
                    *mask = Some((shape, Handle::from_memory(svg.into_bytes())));
                }
                if let Some((_, handle)) = mask.as_ref() {
                    let mut svg = Svg::new(handle.clone());
                    svg.opacity = alpha;
                    renderer.draw_svg(svg, bounds, clip);
                }
            }
        }
        let draw_content = |renderer: &mut Renderer| {
            self.content.as_widget().draw(
                &tree.children[0], renderer, theme, style, layout, cursor, viewport,
            );
        };
        if self.backing.is_some() {
            // Keep the backing below every foreground quad, image and text run.
            // The native renderer batches each primitive type within a layer.
            use cosmic::iced::advanced::Renderer as _;
            renderer.with_layer(*viewport, draw_content);
        } else {
            draw_content(renderer);
        }
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
