use super::{BINS, Circle};
use cosmic::iced::{Color, Point, Rectangle, mouse};
use cosmic::widget::canvas;
#[derive(Clone)]
pub struct Visual {
    pub bins: [f32; BINS],
    pub circle: Circle,
    pub color: Color,
    pub region: Option<super::Preferences>,
    pub backdrop: Option<Color>,
}
impl<M> canvas::Program<M, cosmic::Theme> for Visual {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &cosmic::Renderer,
        _: &cosmic::Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut f = canvas::Frame::new(renderer, bounds.size());
        let bounds = if let Some(p) = &self.region {
            let r = p.rect([bounds.width, bounds.height]);
            f.translate(cosmic::iced::Vector::new(r.x, r.y));
            r
        } else {
            bounds
        };
        if self.circle == Circle::Off {
            let step = bounds.width / BINS as f32;
            for (i, v) in self.bins.iter().enumerate() {
                let height = v.clamp(0., 1.) * bounds.height;
                if height > 0.1 {
                    f.fill_rectangle(
                        Point::new(i as f32 * step, bounds.height - height),
                        cosmic::iced::Size::new((step * 0.65).max(1.), height),
                        self.color,
                    );
                }
            }
        } else {
            let center = Point::new(bounds.width / 2., bounds.height / 2.);
            let radius = bounds.width.min(bounds.height) * 0.36;
            let amplitude = bounds.width.min(bounds.height) * 0.12;
            if let Some(color) = self.backdrop {
                f.fill(&canvas::Path::circle(center, radius - 4.), color);
            }
            for (i, v) in self.bins.iter().enumerate() {
                let angle =
                    i as f32 / BINS as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                let p = |r: f32| Point::new(center.x + r * angle.cos(), center.y + r * angle.sin());
                match self.circle {
                    Circle::Bars => f.stroke(
                        &canvas::Path::line(p(radius), p(radius + 2. + v * amplitude)),
                        canvas::Stroke::default()
                            .with_color(self.color)
                            .with_width(2.)
                            .with_line_cap(canvas::LineCap::Round),
                    ),
                    Circle::Dots => {
                        for ring in 0..3 {
                            f.fill(
                                &canvas::Path::circle(
                                    p(radius + ring as f32 * amplitude / 3. + v * 5.),
                                    1. + v * 2.,
                                ),
                                Color {
                                    a: self.color.a * (0.25 + v * 0.75),
                                    ..self.color
                                },
                            );
                        }
                    }
                    _ => {}
                }
            }
            if self.circle == Circle::Rings {
                for ring in 0..4 {
                    let energy = self.bins[ring * 12..(ring + 1) * 12].iter().sum::<f32>() / 12.;
                    f.stroke(
                        &canvas::Path::circle(
                            center,
                            radius + ring as f32 * amplitude / 4. + energy * 6.,
                        ),
                        canvas::Stroke::default()
                            .with_color(Color {
                                a: self.color.a * (0.25 + energy * 0.75),
                                ..self.color
                            })
                            .with_width(1. + energy),
                    );
                }
            }
        }
        vec![f.into_geometry()]
    }
}
