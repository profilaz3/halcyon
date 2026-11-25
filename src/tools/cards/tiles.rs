//! The Data tiles card: each tile's label, its value as text, a bar, a dial
//! or a line, and a word when the value may be old or couldn't be read.
use super::{WARN, card_title, fill, frame, label, light, line, mix, track};
use crate::config::{Config, Palette};
use crate::tiles::{Reading, Show, State, Tile};
use cosmic::Element;
use cosmic::iced::{Alignment, Color, Length, Point, Radians, Rectangle, mouse, widget::canvas};
use cosmic::widget::{button, column, container, row};
use std::time::{Duration, SystemTime};

const TITLE: f32 = 26.;
const TITLE_GAP: f32 = 8.;
const TILE_GAP: f32 = 10.;
const TWO_LINES: f32 = 42.;
const GAUGE: f32 = 46.;
const GRAPH: f32 = 28.;

fn height(show: Show) -> f32 {
    match show {
        Show::Text => TWO_LINES,
        Show::Progress { .. } => TWO_LINES + 10.,
        Show::Gauge { .. } => GAUGE,
        Show::Graph => TWO_LINES + 4. + GRAPH,
    }
}

/// The card's size for these tiles: one under another.
pub fn size(tiles: &[Tile]) -> [f32; 2] {
    if tiles.is_empty() {
        return [300., 128.];
    }
    let body: f32 =
        tiles.iter().map(|t| height(t.show)).sum::<f32>() + TILE_GAP * (tiles.len() - 1) as f32;
    [
        300.,
        f32::from(crate::widget_style::PADDING) * 2. + TITLE + TITLE_GAP + body,
    ]
}

/// "5 min", "2 h": how long ago, in the card's few words.
fn age(duration: Duration) -> String {
    let minutes = duration.as_secs() / 60;
    match minutes {
        0 => "under a minute".into(),
        1..=59 => format!("{minutes} min"),
        60..=2879 => format!("{} h", minutes / 60),
        _ => format!("{} days", minutes / 1440),
    }
}

/// What a tile says beside its label, and whether it is a warning.
fn status(tile: &Tile, reading: Option<&Reading>, now: SystemTime) -> Option<(String, bool)> {
    let reading = reading?;
    let old = reading.stale(tile, now);
    match (&reading.value, &reading.problem) {
        (Some(_), Some(problem)) => Some((
            format!("{} · {} old", problem.text(), age(old.unwrap_or_default())),
            true,
        )),
        (Some(_), None) => old.map(|old| (format!("Unchanged for {}", age(old)), false)),
        _ => None,
    }
}

struct Dial {
    fraction: f32,
    color: Color,
    track: Color,
}
impl<M> canvas::Program<M, cosmic::Theme> for Dial {
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
        let radius = bounds.width.min(bounds.height) / 2. - 4.;
        // Three quarters of a turn, open at the bottom.
        let start = std::f32::consts::PI * 0.75;
        let sweep = std::f32::consts::PI * 1.5;
        let arc = |to: f32| {
            canvas::Path::new(|b| {
                b.arc(canvas::path::Arc {
                    center,
                    radius,
                    start_angle: Radians(start),
                    end_angle: Radians(start + sweep * to),
                })
            })
        };
        let stroke = |color| {
            canvas::Stroke::default()
                .with_width(5.)
                .with_color(color)
                .with_line_cap(canvas::LineCap::Round)
        };
        frame.stroke(&arc(1.), stroke(self.track));
        if self.fraction > 0. {
            frame.stroke(&arc(self.fraction.clamp(0., 1.)), stroke(self.color));
        }
        vec![frame.into_geometry()]
    }
}

struct Line<'a> {
    values: &'a std::collections::VecDeque<f64>,
    color: Color,
}
impl<M> canvas::Program<M, cosmic::Theme> for Line<'_> {
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
        if self.values.len() < 2 {
            // The first reading: a dot where the line will start from.
            if !self.values.is_empty() {
                let at = Point::new(bounds.width - 3., bounds.height / 2.);
                frame.fill(&canvas::Path::circle(at, 2.4), self.color);
            }
            return vec![frame.into_geometry()];
        }
        let low = self.values.iter().copied().fold(f64::INFINITY, f64::min);
        let high = self
            .values
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let spread = high - low;
        let step = (bounds.width - 6.).max(0.) / (crate::tiles::HISTORY - 1) as f32;
        // The newest reading sits at the right edge; older ones lead to it.
        let first = crate::tiles::HISTORY - self.values.len();
        let points: Vec<Point> = self
            .values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let y = if spread > 0. {
                    (1. - (v - low) / spread) as f32
                } else {
                    0.5
                };
                Point::new(
                    3. + (first + i) as f32 * step,
                    3. + y * (bounds.height - 6.).max(0.),
                )
            })
            .collect();
        let path = canvas::Path::new(|b| {
            b.move_to(points[0]);
            for &point in &points[1..] {
                b.line_to(point);
            }
        });
        frame.fill(
            &canvas::Path::circle(points[points.len() - 1], 2.4),
            self.color,
        );
        frame.stroke(
            &path,
            canvas::Stroke::default()
                .with_width(1.8)
                .with_color(self.color),
        );
        vec![frame.into_geometry()]
    }
}

fn value_line<'a, M: 'a>(tile: &Tile, reading: Option<&Reading>, p: Palette) -> Element<'a, M> {
    let Some(reading) = reading else {
        return line("Reading…", 14., p.muted);
    };
    match (&reading.value, &reading.problem) {
        (Some(value), _) => {
            let mut parts = vec![light(value.text(), 20., p.foreground)];
            if !tile.unit.is_empty() {
                parts.push(label(tile.unit.clone(), 13., p.muted));
            }
            row::with_children(parts)
                .spacing(4)
                .align_y(Alignment::End)
                .height(Length::Fixed(26.))
                .into()
        }
        (None, Some(problem)) => container(line(problem.text(), 14., WARN))
            .height(Length::Fixed(26.))
            .align_y(Alignment::Center)
            .into(),
        (None, None) => container(line("Reading…", 14., p.muted))
            .height(Length::Fixed(26.))
            .align_y(Alignment::Center)
            .into(),
    }
}

fn tile_view<'a, M: 'a>(
    tile: &'a Tile,
    reading: Option<&'a Reading>,
    p: Palette,
    now: SystemTime,
) -> Element<'a, M> {
    let mut top = vec![
        container(line(tile.label.clone(), 13., p.muted))
            .width(Length::Fill)
            .into(),
    ];
    if let Some((words, warn)) = status(tile, reading, now) {
        top.push(line(words, 12., if warn { WARN } else { p.muted }));
    }
    let top = row::with_children(top)
        .spacing(6)
        .height(Length::Fixed(16.))
        .align_y(Alignment::Center);
    let lines = column::with_children(vec![top.into(), value_line(tile, reading, p)]);
    let number = reading.and_then(|r| r.value.as_ref()?.number());
    let accent = p.accent;
    let quiet = mix(p.accent, p.background, 0.12);
    match tile.show {
        Show::Text => lines.into(),
        Show::Progress { max } => column::with_children(vec![
            lines.into(),
            container(track(number.map_or(0., |n| (n / max) as f32), accent, p))
                .padding([6, 0, 0, 0])
                .into(),
        ])
        .into(),
        Show::Gauge { min, max } => row::with_children(vec![
            canvas::Canvas::new(Dial {
                fraction: number.map_or(0., |n| ((n - min) / (max - min)) as f32),
                color: accent,
                track: quiet,
            })
            .width(Length::Fixed(GAUGE))
            .height(Length::Fixed(GAUGE))
            .into(),
            container(lines).width(Length::Fill).into(),
        ])
        .spacing(10)
        .align_y(Alignment::Center)
        .height(Length::Fixed(GAUGE))
        .into(),
        Show::Graph => {
            let graph: Element<'a, M> = match reading {
                Some(r) => canvas::Canvas::new(Line {
                    values: &r.history,
                    color: accent,
                })
                .width(Length::Fill)
                .height(Length::Fixed(GRAPH))
                .into(),
                None => container(fill()).height(Length::Fixed(GRAPH)).into(),
            };
            column::with_children(vec![lines.into(), graph])
                .spacing(4)
                .into()
        }
    }
}

/// The card. `add` is what the empty card's button asks for; without it the
/// empty card only says where tiles are added.
pub fn tiles<'a, M: Clone + 'static>(
    state: &'a State,
    tiles: &'a [Tile],
    config: &Config,
    size: Option<[f32; 2]>,
    add: Option<M>,
) -> Element<'a, M> {
    let _scope = crate::widget_style::scope(config);
    let p = config.palette();
    let [_, height] = size.unwrap_or_else(|| self::size(tiles));
    let mut body = vec![
        container(card_title("Data tiles", p))
            .height(Length::Fixed(TITLE))
            .into(),
    ];
    if tiles.is_empty() {
        body.push(label(
            "Show numbers and words from JSON files other programs write.",
            13.,
            p.muted,
        ));
        body.push(match add {
            Some(add) => button::standard("Add a tile").on_press(add).into(),
            None => line("Add tiles in Settings › Data tiles.", 13., p.muted),
        });
        return frame(
            column::with_children(body).spacing(TITLE_GAP).into(),
            config,
            height,
        );
    }
    // The readings belong to the tiles they were read for; a tile edited a
    // moment ago shows Reading… until its file has been read.
    let current = state.tiles.as_slice() == tiles;
    let now = SystemTime::now();
    let list: Vec<Element<'a, M>> = tiles
        .iter()
        .enumerate()
        .map(|(i, tile)| tile_view(tile, current.then(|| state.reading(i)).flatten(), p, now))
        .collect();
    body.push(column::with_children(list).spacing(TILE_GAP).into());
    frame(
        column::with_children(body).spacing(TITLE_GAP).into(),
        config,
        height,
    )
}
