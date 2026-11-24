//! Static, cover-coloured light caught by the facing edge of a Clock card.
use crate::widget_style::resolver::{Style, Surface};
use cosmic::iced::advanced::svg::{Handle, Svg};
use cosmic::iced::widget::{Container, canvas, stack};
use cosmic::iced::{Color, Length, Point, Rectangle, Size, mouse};
use cosmic::{Element, Renderer, Theme};
use std::cell::RefCell;

/// Distance between the two card edges in logical desktop pixels.
const REACH: f32 = 320.;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Reflection {
    source: Point,
    color: Color,
    opacity: f32,
}

impl Reflection {
    pub fn between(
        music: Rectangle,
        sleeve: Rectangle,
        clock: Rectangle,
        rgb: [u8; 3],
        strength: u8,
    ) -> Option<Self> {
        let dx = (music.x - clock.x - clock.width)
            .max(clock.x - music.x - music.width)
            .max(0.);
        let dy = (music.y - clock.y - clock.height)
            .max(clock.y - music.y - music.height)
            .max(0.);
        let distance = dx.hypot(dy);
        if strength == 0 || distance >= REACH || sleeve.width <= 0. || sleeve.height <= 0. {
            return None;
        }
        let t = distance / REACH;
        let falloff = 1. - t * t * (3. - 2. * t);
        let [r, g, b, a] = super::dark_glass::reflected_color(rgb);
        let center = sleeve.center();
        Some(Self {
            source: Point::new(music.x + center.x - clock.x, music.y + center.y - clock.y),
            color: Color::from_linear_rgba(r, g, b, 1.),
            opacity: (1.6 * a).min(0.8) * falloff * (f32::from(strength) / 100.),
        })
    }
}

/// The real clock and recipe renderers share this surface. The light goes
/// above the background and behind all text, hands and interactive controls.
pub(crate) fn panel<'a, M: 'a>(
    body: Container<'a, M, Theme, Renderer>,
    size: [f32; 2],
    style: Style,
    reflection: Option<Reflection>,
) -> Element<'a, M> {
    let radius = f32::from(style.radius.value);
    let background = match style.surface.value {
        Surface::None => None,
        Surface::Solid(color) => Some(color.into()),
    };
    let body = match reflection {
        Some(light) if background.is_some() => {
            let wash = canvas(Edge { light, radius })
                .width(Length::Fill)
                .height(Length::Fill);
            cosmic::widget::container(
                stack(vec![wash.into(), body.into()])
                    .width(size[0])
                    .height(size[1]),
            )
        }
        _ => body,
    };
    body.width(size[0])
        .height(size[1])
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background,
                border: cosmic::iced::Border {
                    radius: radius.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

struct Edge {
    light: Reflection,
    radius: f32,
}

#[derive(Clone, Copy, PartialEq)]
struct Mask {
    size: Size,
    radius: f32,
    contact: Point,
    angle: f32,
}

impl Mask {
    fn at(source: Point, size: Size, radius: f32) -> Option<Self> {
        let radius = radius.min(size.width.min(size.height) / 2.);
        // Project onto the rounded perimeter, including the curved corners.
        let x = source.x.clamp(radius, size.width - radius);
        let y = source.y.clamp(radius, size.height - radius);
        let (dx, dy) = (source.x - x, source.y - y);
        let length = dx.hypot(dy);
        if length <= radius {
            return None;
        }
        Some(Self {
            size,
            radius,
            contact: Point::new(
                (x + radius * dx / length).round(),
                (y + radius * dy / length).round(),
            ),
            angle: dy.atan2(dx).to_degrees().round(),
        })
    }

    fn handle(self) -> Handle {
        let Size {
            width: w,
            height: h,
        } = self.size;
        let Point { x, y } = self.contact;
        let (a, r) = (self.angle, self.radius);
        let depth = (w.min(h) * 0.18).clamp(6., 32.);
        let length = (w.min(h) * 0.80).clamp(42., 132.);
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><defs>
          <radialGradient id="g" gradientUnits="userSpaceOnUse" cx="0" cy="0" r="1" gradientTransform="translate({x} {y}) rotate({a}) scale({depth} {length})">
          <stop stop-color="white"/><stop offset=".38" stop-color="white" stop-opacity=".65"/>
          <stop offset=".78" stop-color="white" stop-opacity=".13"/><stop offset="1" stop-color="white" stop-opacity="0"/>
          </radialGradient><clipPath id="c"><rect width="{w}" height="{h}" rx="{r}"/></clipPath></defs>
          <g clip-path="url(#c)"><rect width="{w}" height="{h}" rx="{r}" fill="url(#g)" fill-opacity=".52" stroke="url(#g)" stroke-width="3"/></g></svg>"##
        );
        Handle::from_memory(svg.into_bytes())
    }
}

impl<M> canvas::Program<M, Theme> for Edge {
    type State = RefCell<Option<(Mask, Handle)>>;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let Some(mask) = Mask::at(self.light.source, bounds.size(), self.radius) else {
            return vec![];
        };
        let mut cached = state.borrow_mut();
        let (_, handle) = match cached.as_mut() {
            Some(entry) if entry.0 == mask => entry,
            _ => cached.insert((mask, mask.handle())),
        };
        let mut svg = Svg::new(handle.clone());
        svg.color = Some(self.light.color);
        svg.opacity = self.light.opacity;
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        frame.draw_svg(Rectangle::with_size(bounds.size()), svg);
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::iced::{Event, window};
    use std::time::Instant;

    #[test]
    fn facing_edge_follows_the_sleeve_and_rounds_the_corner() {
        let size = Size::new(240., 160.);
        for (source, expected) in [
            (Point::new(-40., 66.), Point::new(0., 66.)),
            (Point::new(270., 88.), Point::new(240., 88.)),
            (Point::new(78., -40.), Point::new(78., 0.)),
            (Point::new(134., 240.), Point::new(134., 160.)),
            (Point::new(-40., -40.), Point::new(5., 5.)),
        ] {
            assert_eq!(Mask::at(source, size, 16.).unwrap().contact, expected);
        }
        assert!(Mask::at(Point::new(120., 80.), size, 16.).is_none());
    }

    #[test]
    fn strength_and_card_distance_attenuate_without_requesting_frames() {
        let rect = |x| Rectangle::new(Point::new(x, 0.), Size::new(180., 100.));
        let light = |gap, strength| {
            Reflection::between(
                rect(0.),
                rect(0.),
                rect(180. + gap),
                [120, 80, 240],
                strength,
            )
        };
        let near = light(0., 100).unwrap();
        assert_eq!(light(0., 50).unwrap().opacity, near.opacity / 2.);
        assert!(light(REACH / 2., 100).unwrap().opacity < near.opacity);
        assert!(light(REACH, 100).is_none() && light(0., 0).is_none());
        let edge = Edge {
            light: near,
            radius: 16.,
        };
        assert!(
            <Edge as canvas::Program<(), Theme>>::update(
                &edge,
                &mut Default::default(),
                &Event::Window(window::Event::RedrawRequested(Instant::now())),
                rect(180.),
                mouse::Cursor::Unavailable
            )
            .is_none()
        );
    }
}
