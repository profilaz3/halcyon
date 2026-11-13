//! The Devices card: a ring per connected device with its icon inside and
//! the battery underneath, in the card's own palette.
use super::{card_title, frame, label, mix};
use crate::config::{Config, Palette};
use crate::devices::Device;
use crate::drawer::Widget;
use cosmic::Element;
use cosmic::iced::advanced::{Layout, Renderer as _, layout, renderer, widget::Tree};
use cosmic::iced::{Alignment, Border, Color, Length, Rectangle, Size, mouse};
use cosmic::widget::{column, container, icon, row};

/// Tiles share the width; a tile is never narrower or wider than these.
const MIN_TILE: f32 = 54.;
const MAX_TILE: f32 = 136.;
/// The ring grows with the card, within these.
const MIN_RING: f32 = 38.;
const MAX_RING: f32 = 104.;
const GAP: f32 = 8.;
/// Between tiles side by side, kept tight so names get the width.
const COLUMN: f32 = 4.;
/// Between a tile's ring, percent and name.
const STACK: f32 = 3.;
const LINE: f32 = 1.25;
const TITLE_HEIGHT: f32 = 24.;
/// Low battery, in the same amber the other cards warn in, and red at 10%.
const URGENT: Color = Color::from_rgb(0.94, 0.42, 0.40);

/// A battery ring, lit clockwise from the top. Round dots, so it needs no
/// path support and stays a handful of quads.
pub(crate) struct Ring {
    pub size: f32,
    pub stroke: f32,
    pub fraction: f32,
    pub color: Color,
    pub track: Color,
}
impl<M> cosmic::iced::advanced::Widget<M, cosmic::Theme, cosmic::Renderer> for Ring {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.size), Length::Fixed(self.size))
    }
    fn layout(&mut self, _: &mut Tree, _: &cosmic::Renderer, _: &layout::Limits) -> layout::Node {
        layout::Node::new(Size::new(self.size, self.size))
    }
    fn draw(
        &self,
        _: &Tree,
        r: &mut cosmic::Renderer,
        _: &cosmic::Theme,
        _: &renderer::Style,
        l: Layout<'_>,
        _: mouse::Cursor,
        _: &Rectangle,
    ) {
        let centre = l.bounds().center();
        let radius = (self.size - self.stroke) / 2.;
        let dots = ((std::f32::consts::TAU * radius) / (self.stroke * 0.45))
            .ceil()
            .clamp(24., 180.) as usize;
        let lit = self.fraction.clamp(0., 1.);
        for i in 0..dots {
            let t = i as f32 / dots as f32;
            let angle = -std::f32::consts::FRAC_PI_2 + t * std::f32::consts::TAU;
            r.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: centre.x + radius * angle.cos() - self.stroke / 2.,
                        y: centre.y + radius * angle.sin() - self.stroke / 2.,
                        width: self.stroke,
                        height: self.stroke,
                    },
                    border: Border {
                        radius: (self.stroke / 2.).into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                if lit > 0. && t <= lit {
                    self.color
                } else {
                    self.track
                },
            );
        }
    }
}
impl<'a, M: 'a> From<Ring> for Element<'a, M> {
    fn from(value: Ring) -> Self {
        Element::new(value)
    }
}

/// The ring's colour: the accent, amber at 20% and red at 10% while it
/// runs down.
pub(crate) fn tone(device: &Device, accent: Color, warn: Color) -> Color {
    match device.low() {
        Some(level) if level <= crate::devices::LOW[1] => URGENT,
        Some(_) => warn,
        None => accent,
    }
}

/// The icon for a battery charging at about this level.
pub(crate) fn charging_icon(battery: u8) -> String {
    format!(
        "battery-level-{}-charging-symbolic",
        (u16::from(battery.min(100)) / 10) * 10
    )
}

/// Figures in equal cells, so a percent never shifts sideways.
fn tabular<'a, M: 'a>(value: &str, size: f32, color: Color) -> Element<'a, M> {
    let size = size.max(crate::widget_style::META);
    row::with_children(
        value
            .chars()
            .map(|c| {
                container(
                    cosmic::widget::text::body(c.to_string())
                        .size(size)
                        .font(super::font())
                        .wrapping(cosmic::iced::advanced::text::Wrapping::None)
                        .class(cosmic::theme::Text::Color(crate::widget_style::semantic(
                            color,
                        ))),
                )
                .width(Length::Fixed(match c {
                    '0'..='9' => (size * 0.62).ceil(),
                    '%' => (size * 0.86).ceil(),
                    _ => (size * 0.5).ceil(),
                }))
                .align_x(Alignment::Center)
                .into()
            })
            .collect::<Vec<_>>(),
    )
    .align_y(Alignment::Center)
    .into()
}

/// One name on one line, in the card's font.
fn one_line(value: &str, size: f32) -> f32 {
    use cosmic::iced::advanced::text::{self as t, Paragraph as _};
    type Native = <cosmic::Renderer as t::Renderer>::Paragraph;
    Native::with_text(t::Text {
        content: value,
        bounds: Size::new(f32::INFINITY, f32::INFINITY),
        size: size.into(),
        line_height: t::LineHeight::Relative(LINE),
        font: super::font(),
        align_x: t::Alignment::Left,
        align_y: cosmic::iced::alignment::Vertical::Top,
        shaping: t::Shaping::Advanced,
        wrapping: t::Wrapping::None,
        ellipsize: t::Ellipsize::None,
    })
    .min_bounds()
    .width
}

/// A name as it fits a tile `width` wide: whole on one line, or with
/// `lines` of two, broken at the space that balances the lines, or after a
/// hyphen when no space will do. None when it has to be cut short.
pub(crate) fn fitted(name: &str, size: f32, width: f32, lines: u8) -> Option<String> {
    if one_line(name, size) <= width {
        return Some(name.to_owned());
    }
    if lines < 2 {
        return None;
    }
    name.char_indices()
        .filter_map(|(i, c)| match c {
            ' ' => Some((false, i)),
            '-' => Some((true, i + 1)),
            _ => None,
        })
        .map(|(hyphen, i)| (hyphen, name[..i].trim_end(), name[i..].trim_start()))
        .filter(|(_, first, second)| !first.is_empty() && !second.is_empty())
        .map(|(hyphen, first, second)| {
            let widest = one_line(first, size).max(one_line(second, size));
            (hyphen, widest, first, second)
        })
        .filter(|(_, widest, ..)| *widest <= width)
        .min_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)))
        .map(|(_, _, first, second)| format!("{first}\n{second}"))
}

/// How the shown devices sit in the card's body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Grid {
    pub shown: usize,
    pub columns: usize,
    pub tile: f32,
    pub ring: f32,
    /// The percent's size, which grows a little with the ring.
    pub figure: f32,
    pub name: f32,
    /// Two when a name needs a second line and there is room for it.
    pub lines: u8,
}
impl Grid {
    fn text_height(figure: f32, name: f32, lines: u8) -> f32 {
        2. * STACK + figure * 1.3 + f32::from(lines) * name * LINE
    }
    pub fn tile_height(&self) -> f32 {
        self.ring + Self::text_height(self.figure, self.name, self.lines)
    }
}

/// The largest rings that fit `count` devices in rows, given each name's
/// width on one line at the small and large name sizes.
fn arrange(count: usize, inner: [f32; 2], widths: &dyn Fn(f32) -> f32) -> Option<Grid> {
    let mut best: Option<Grid> = None;
    for rows in 1..=count {
        let columns = count.div_ceil(rows);
        if rows > 1 && count.div_ceil(rows - 1) == columns {
            continue;
        }
        let tile = ((inner[0] - COLUMN * (columns - 1) as f32) / columns as f32).min(MAX_TILE);
        if tile < MIN_TILE {
            continue;
        }
        let row = (inner[1] - GAP * (rows - 1) as f32) / rows as f32;
        let name = if tile >= 100. {
            crate::widget_style::SECONDARY
        } else {
            crate::widget_style::META
        };
        let wide = widths(name) > tile;
        let fit = |lines: u8| {
            let mut ring = (tile - 10.).min(MAX_RING);
            for _ in 0..2 {
                let figure = (ring * 0.24).clamp(crate::widget_style::SECONDARY, 18.);
                ring = ring.min(row - Grid::text_height(figure, name, lines));
            }
            let ring = ring.floor();
            (ring >= MIN_RING).then(|| Grid {
                shown: count,
                columns,
                tile,
                ring,
                figure: (ring * 0.24).clamp(crate::widget_style::SECONDARY, 18.),
                name,
                lines,
            })
        };
        // Wrap to two lines before cutting a name short.
        let Some(grid) = (if wide {
            fit(2).or_else(|| fit(1))
        } else {
            fit(1)
        }) else {
            continue;
        };
        if best.is_none_or(|b| grid.ring > b.ring) {
            best = Some(grid);
        }
    }
    best
}

/// The layout for this many devices at this size: every device when they
/// fit, otherwise as many as fit at the smallest ring.
pub(crate) fn grid(list: &[Device], width: f32, height: f32) -> Grid {
    let pad = 2. * f32::from(super::PADDING);
    let inner = [
        (width - pad).max(MIN_TILE),
        (height - pad - TITLE_HEIGHT - GAP).max(0.),
    ];
    let widths = |size: f32| {
        list.iter()
            .map(|d| one_line(&d.name, size).ceil())
            .fold(0., f32::max)
    };
    (1..=list.len().max(1))
        .rev()
        .find_map(|count| arrange(count, inner, &|size| widths(size)))
        .unwrap_or(Grid {
            shown: 1,
            columns: 1,
            tile: inner[0].min(MAX_TILE),
            ring: MIN_RING,
            figure: crate::widget_style::SECONDARY,
            name: crate::widget_style::META,
            lines: 1,
        })
}

fn tile<'a, M: 'a>(device: &Device, p: Palette, g: Grid) -> Element<'a, M> {
    // Through the style's semantic roles, so amber and red stay readable on
    // light cards as the other cards' warnings do.
    let tone = crate::widget_style::semantic(tone(device, p.accent, super::WARN));
    let ring = Ring {
        size: g.ring,
        stroke: (g.ring / 11.).clamp(4., 8.),
        fraction: device.battery.map_or(0., |b| f32::from(b) / 100.),
        color: tone,
        track: mix(p.accent, p.background, 0.16),
    };
    let glyph = container(
        icon::from_name(device.kind.icon())
            .size((g.ring * 0.42).round().clamp(18., 44.) as u16)
            .icon(),
    )
    .width(Length::Fixed(g.ring))
    .height(Length::Fixed(g.ring))
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .class(cosmic::theme::Container::custom(move |_| {
        cosmic::iced::widget::container::Style {
            icon_color: Some(p.foreground),
            ..Default::default()
        }
    }));
    let mut reading: Vec<Element<'a, M>> = vec![tabular(
        &device.percent().unwrap_or_else(|| "–".into()),
        g.figure,
        p.foreground,
    )];
    if device.charging() {
        if let Some(battery) = device.battery {
            reading.push(
                container(
                    icon::from_name(charging_icon(battery))
                        .size(g.figure.round() as u16)
                        .icon(),
                )
                .class(cosmic::theme::Container::custom(move |_| {
                    cosmic::iced::widget::container::Style {
                        icon_color: Some(p.muted),
                        ..Default::default()
                    }
                }))
                .into(),
            );
        }
    }
    use cosmic::iced::advanced::text as t;
    // Whole when it fits, on lines broken here, so the renderer never
    // trims a name it does not need to; cut short only past two lines.
    let whole = fitted(&device.name, g.name, g.tile, g.lines);
    let (wrapping, ellipsize) = match (&whole, g.lines) {
        (Some(_), _) => (t::Wrapping::None, t::Ellipsize::None),
        (None, 1) => (
            t::Wrapping::None,
            t::Ellipsize::End(t::EllipsizeHeightLimit::Lines(1)),
        ),
        (None, lines) => (
            t::Wrapping::WordOrGlyph,
            t::Ellipsize::End(t::EllipsizeHeightLimit::Lines(usize::from(lines))),
        ),
    };
    let name = cosmic::widget::text::body(whole.unwrap_or_else(|| device.name.clone()))
        .size(g.name)
        .line_height(LINE)
        .font(super::font())
        .align_x(Alignment::Center)
        .wrapping(wrapping)
        .ellipsize(ellipsize)
        .width(Length::Fill)
        .class(cosmic::theme::Text::Color(crate::widget_style::semantic(
            p.muted,
        )));
    column::with_children(vec![
        cosmic::iced::widget::stack![Element::from(ring), glyph].into(),
        row::with_children(reading)
            .spacing(2)
            .align_y(Alignment::Center)
            .into(),
        container(name)
            .width(Length::Fixed(g.tile))
            .height(Length::Fixed(f32::from(g.lines) * g.name * LINE))
            .align_x(Alignment::Center)
            .into(),
    ])
    .spacing(STACK)
    .width(Length::Fixed(g.tile))
    .align_x(Alignment::Center)
    .into()
}

/// The Devices card. Rings grow with the card and the tiles sit centred in
/// its body. Nothing connected says so; more devices than fit say how many
/// more in the header.
pub fn devices<'a, M: Clone + 'a>(
    list: &[Device],
    config: &Config,
    dimensions: Option<[f32; 2]>,
) -> Element<'a, M> {
    let _style = crate::widget_style::scope(config);
    let p = config.palette();
    let [width, height] = dimensions.unwrap_or_else(|| super::size(Widget::Devices, config));
    let g = grid(list, width, height);
    let shown = list.len().min(g.shown);
    let mut header: Vec<Element<'a, M>> = vec![
        container(card_title("Devices", p))
            .width(Length::Fill)
            .into(),
    ];
    if list.len() > shown {
        header.push(label(
            format!("{} more", list.len() - shown),
            crate::widget_style::SECONDARY,
            p.muted,
        ));
    }
    let body: Element<'a, M> = if list.is_empty() {
        container(label(
            "No devices connected",
            crate::widget_style::SECONDARY,
            p.muted,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    } else {
        let rows = list[..shown]
            .chunks(g.columns)
            .map(|chunk| {
                row::with_children(chunk.iter().map(|d| tile(d, p, g)).collect::<Vec<_>>())
                    .spacing(COLUMN)
                    .into()
            })
            .collect::<Vec<Element<'a, M>>>();
        container(
            column::with_children(rows)
                .spacing(GAP)
                .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    };
    frame(
        column::with_children(vec![
            row::with_children(header)
                .spacing(8)
                .align_y(Alignment::Center)
                .height(Length::Fixed(TITLE_HEIGHT))
                .into(),
            body,
        ])
        .spacing(GAP)
        .height(Length::Fill)
        .into(),
        config,
        height,
    )
}

/// Three devices for previews: headphones charging, a mouse running low
/// and a phone.
pub fn sample() -> Vec<Device> {
    use crate::devices::Kind;
    let device = |id: &str, name: &str, kind, battery, charging| Device {
        id: id.into(),
        name: name.into(),
        kind,
        battery: Some(battery),
        charging: Some(charging),
        ..Default::default()
    };
    vec![
        device(
            "bt:AA:BB:CC:DD:EE:01",
            "Sony WH-1000XM5",
            Kind::Headphones,
            80,
            true,
        ),
        device(
            "bt:AA:BB:CC:DD:EE:02",
            "MX Anywhere 3",
            Kind::Mouse,
            15,
            false,
        ),
        device("kdeconnect:pixel", "Pixel 8", Kind::Phone, 64, false),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_warn_as_the_battery_runs_down() {
        let accent = Color::WHITE;
        let warn = super::super::WARN;
        let mut device = Device {
            battery: Some(50),
            charging: Some(false),
            ..Default::default()
        };
        assert_eq!(tone(&device, accent, warn), accent);
        device.battery = Some(20);
        assert_eq!(tone(&device, accent, warn), warn);
        device.battery = Some(9);
        assert_eq!(tone(&device, accent, warn), URGENT);
        device.charging = Some(true);
        assert_eq!(tone(&device, accent, warn), accent);
        assert_eq!(charging_icon(87), "battery-level-80-charging-symbolic");
        assert_eq!(charging_icon(100), "battery-level-100-charging-symbolic");
    }

    #[test]
    fn the_card_shows_as_many_devices_as_fit() {
        let names = |size: f32| size * 8.;
        // Three fit the default card on one row, and four still do.
        let default = [280. - 36., 160. - 36. - TITLE_HEIGHT - GAP];
        let three = arrange(3, default, &names).unwrap();
        assert_eq!(three.columns, 3);
        assert!(three.tile_height() <= default[1]);
        let four = arrange(4, default, &names).unwrap();
        assert_eq!(four.columns, 4);
        assert!(four.tile * 4. + COLUMN * 3. <= default[0] + 0.5);
        // A larger card grows the rings, up to a limit.
        let large = arrange(3, [420. - 36., 260. - 36. - TITLE_HEIGHT - GAP], &names).unwrap();
        assert!(large.ring > three.ring + 20., "{large:?} {three:?}");
        let huge = arrange(3, [2000., 2000.], &names).unwrap();
        assert!(huge.ring <= MAX_RING && huge.tile <= MAX_TILE);
        // A long name takes two lines when there is room, and one when not.
        let long = |size: f32| size * 30.;
        assert_eq!(arrange(3, [384., 192.], &long).unwrap().lines, 2);
        assert_eq!(arrange(3, [244., 80.], &long).unwrap().lines, 1);
        // A name is broken at the space that balances its two lines, and
        // only cut short when two lines are still too narrow. Widths come
        // from the font that is installed, so the test holds with any font.
        let w = |text: &str| one_line(text, 10.);
        let sony = "Sony WH-1000XM5";
        assert_eq!(
            fitted("Pixel 8", 10., w("Pixel 8") + 1., 2).as_deref(),
            Some("Pixel 8")
        );
        assert_eq!(
            fitted(sony, 10., w("WH-1000XM5") + 1., 2).as_deref(),
            Some("Sony\nWH-1000XM5")
        );
        let anywhere = w("MX").max(w("Anywhere 3")) + 1.;
        assert!(anywhere < w("MX Anywhere 3") && anywhere < w("MX Anywhere"));
        assert_eq!(
            fitted("MX Anywhere 3", 10., anywhere, 2).as_deref(),
            Some("MX\nAnywhere 3")
        );
        // A hyphen is a break too, when the word after a space is too wide.
        let hyphen = w("Sony WH-").max(w("1000XM5")) + 1.;
        assert!(hyphen < w("WH-1000XM5"), "this font breaks the premise");
        assert_eq!(
            fitted(sony, 10., hyphen, 2).as_deref(),
            Some("Sony WH-\n1000XM5")
        );
        assert_eq!(fitted(sony, 10., w(sony) - 1., 1), None);
        assert_eq!(fitted(sony, 10., w("1000XM5") - 1., 2), None);
        // Too many for the card: none of these fit, so fewer are shown.
        assert!(arrange(16, default, &names).is_none());
    }
}
