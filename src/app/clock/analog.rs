//! Clock hands are graphics; the readable time stays a native text element.
use super::*;
use cosmic::iced::{Rectangle, mouse};
use cosmic::widget::canvas;

struct Dial {
    reading: ClockReading,
    palette: Palette,
    face: ClockFace,
    font: Font,
    halo: Option<Color>,
}
impl<Message> canvas::Program<Message, cosmic::Theme> for Dial {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &cosmic::Renderer,
        _: &cosmic::Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let center = Point::new(bounds.width / 2., bounds.height / 2.);
        let radius = (bounds.width.min(bounds.height) / 2. - 2.).max(1.);
        let point = |turn: f32, fraction: f32| {
            let angle = turn * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            Point::new(
                center.x + angle.cos() * radius * fraction,
                center.y + angle.sin() * radius * fraction,
            )
        };
        let p = self.palette;
        // On the picture, every line gets a faint wider copy underneath in
        // the opposite tone, like the digital time's soft edge.
        let edge = (radius / 60.).clamp(1., 3.);
        let halo = self.halo;
        let line = |frame: &mut canvas::Frame, from: Point, to: Point, color: Color, width: f32| {
            let path = canvas::Path::line(from, to);
            if let Some(halo) = halo {
                frame.stroke(
                    &path,
                    canvas::Stroke::default()
                        .with_color(halo)
                        .with_width(width + edge * 2.)
                        .with_line_cap(canvas::LineCap::Round),
                );
            }
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_color(color)
                    .with_width(width)
                    .with_line_cap(canvas::LineCap::Round),
            );
        };
        let (hour, minute) = match self.face {
            ClockFace::Minimal => {
                // Four bars at the quarters, a dot at each other hour.
                for hour in 0..12 {
                    let turn = hour as f32 / 12.;
                    if hour % 3 == 0 {
                        line(
                            &mut frame,
                            point(turn, 0.78),
                            point(turn, 0.94),
                            p.foreground,
                            (radius * 0.045).clamp(1.5, 5.),
                        );
                    } else {
                        let dot = (radius * 0.022).clamp(1., 3.);
                        line(
                            &mut frame,
                            point(turn, 0.9),
                            point(turn, 0.9),
                            p.muted,
                            dot * 2.,
                        );
                    }
                }
                (
                    (0.5, p.foreground, (radius * 0.07).clamp(2., 8.)),
                    (0.8, p.foreground, (radius * 0.035).clamp(1.2, 4.)),
                )
            }
            ClockFace::Numerals => {
                for tick in 0..60 {
                    if tick % 5 != 0 {
                        let turn = tick as f32 / 60.;
                        line(
                            &mut frame,
                            point(turn, 0.93),
                            point(turn, 0.97),
                            Color { a: 0.7, ..p.muted },
                            0.6,
                        );
                    }
                }
                let size = (radius * 0.24).max(6.);
                for hour in 1..=12 {
                    let quarter = hour % 3 == 0;
                    let at = point(hour as f32 / 12., 0.76);
                    let mut numeral = canvas::Text {
                        content: hour.to_string(),
                        position: at,
                        color: if quarter { p.foreground } else { p.muted },
                        size: size.into(),
                        font: self.font,
                        align_x: cosmic::iced::advanced::text::Alignment::Center,
                        align_y: cosmic::iced::alignment::Vertical::Center,
                        ..Default::default()
                    };
                    if let Some(halo) = self.halo {
                        let color = numeral.color;
                        for (x, y) in [(1., 0.), (-1., 0.), (0., 1.), (0., -1.)] {
                            numeral.position = Point::new(at.x + x * edge, at.y + y * edge);
                            numeral.color = halo;
                            frame.fill_text(numeral.clone());
                        }
                        numeral.position = at;
                        numeral.color = color;
                    }
                    frame.fill_text(numeral);
                }
                (
                    (0.44, p.accent, (radius * 0.055).clamp(1.5, 6.)),
                    (0.62, p.foreground, (radius * 0.027).clamp(1., 3.5)),
                )
            }
            _ => {
                let ring = canvas::Path::circle(center, radius);
                if let Some(halo) = self.halo {
                    frame.stroke(
                        &ring,
                        canvas::Stroke::default()
                            .with_color(halo)
                            .with_width(0.8 + edge * 2.),
                    );
                }
                frame.stroke(
                    &ring,
                    canvas::Stroke::default()
                        .with_color(p.muted)
                        .with_width(0.8),
                );
                for tick in 0..60 {
                    let major = tick % 5 == 0;
                    let turn = tick as f32 / 60.;
                    line(
                        &mut frame,
                        point(turn, if major { 0.82 } else { 0.90 }),
                        point(turn, 0.94),
                        if major { p.foreground } else { p.muted },
                        if major { 1.4 } else { 0.6 },
                    );
                }
                (
                    (0.52, p.accent, (radius * 0.055).clamp(1.5, 6.)),
                    (0.77, p.foreground, (radius * 0.027).clamp(1., 3.5)),
                )
            }
        };
        for (turn, (length, color, width)) in [
            (self.reading.hour_turn, hour),
            (self.reading.minute_turn, minute),
        ] {
            line(
                &mut frame,
                point(turn, -0.09),
                point(turn, length),
                color,
                width,
            );
        }
        let hub = (radius * 0.035).clamp(2., 4.);
        if let Some(halo) = self.halo {
            frame.fill(&canvas::Path::circle(center, hub + edge), halo);
        }
        frame.fill(
            &canvas::Path::circle(center, hub),
            if self.face == ClockFace::Minimal {
                p.accent
            } else {
                p.foreground
            },
        );
        vec![frame.into_geometry()]
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn view(
    reading: ClockReading,
    face: ClockFace,
    width: f32,
    height: f32,
    p: Palette,
    font: Font,
    alignment: ClockAlignment,
    halo: Option<Color>,
) -> Element<'static, Message> {
    let caption_size = (height * 0.09).clamp(8., 13.);
    let diameter = width.min((height - caption_size * 1.2 - 3.).max(1.));
    let caption = reading.period.as_ref().map_or_else(
        || reading.time.clone(),
        |period| format!("{} {period}", reading.time),
    );
    container(
        column::with_children(vec![
            canvas::Canvas::new(Dial {
                reading,
                palette: p,
                face,
                font,
                halo,
            })
            .width(diameter)
            .height(diameter)
            .into(),
            clock_label(caption, caption_size, p.muted, font, halo),
        ])
        .align_x(Alignment::Center)
        .spacing(3),
    )
    .width(width)
    .height(height)
    .align_x(alignment.layout())
    .into()
}
