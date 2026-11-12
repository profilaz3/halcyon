//! The Devices card: a ring per connected device with its icon inside and
//! the battery underneath, in the card's own palette.
use super::{card_title, frame, label, line, mix};
use crate::config::{Config, Palette};
use crate::devices::Device;
use crate::drawer::Widget;
use cosmic::Element;
use cosmic::iced::advanced::{Layout, Renderer as _, layout, renderer, widget::Tree};
use cosmic::iced::{Alignment, Border, Color, Length, Rectangle, Size, mouse};
use cosmic::widget::{column, container, icon, row};

/// Each device's column: a ring and two lines of text under it.
const TILE: f32 = 64.;
const RING: f32 = 46.;
const GAP: f32 = 8.;
/// Ring, percent and name, with the gaps between them.
const TILE_HEIGHT: f32 = RING + 6. + 18. + 16.;
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

/// How many tiles fit: columns across, rows down.
pub(crate) fn fits(width: f32, height: f32) -> usize {
    let inner = |side: f32| side - 2. * f32::from(super::PADDING);
    let columns = ((inner(width) + GAP) / (TILE + GAP)).floor().max(1.) as usize;
    let rows = ((inner(height) - TITLE_HEIGHT - GAP + GAP) / (TILE_HEIGHT + GAP))
        .floor()
        .max(1.) as usize;
    columns * rows
}

fn tile<'a, M: 'a>(device: &Device, p: Palette) -> Element<'a, M> {
    let tone = tone(device, p.accent, super::WARN);
    let ring = Ring {
        size: RING,
        stroke: 4.,
        fraction: device.battery.map_or(0., |b| f32::from(b) / 100.),
        color: tone,
        track: mix(p.accent, p.background, 0.16),
    };
    let glyph = container(icon::from_name(device.kind.icon()).size(20).icon())
        .width(Length::Fixed(RING))
        .height(Length::Fixed(RING))
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
        crate::widget_style::SECONDARY,
        p.foreground,
    )];
    if device.charging() {
        if let Some(battery) = device.battery {
            reading.push(
                container(icon::from_name(charging_icon(battery)).size(12).icon())
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
    column::with_children(vec![
        cosmic::iced::widget::stack![Element::from(ring), glyph].into(),
        row::with_children(reading)
            .spacing(2)
            .align_y(Alignment::Center)
            .into(),
        container(line(
            device.name.clone(),
            crate::widget_style::META,
            p.muted,
        ))
        .width(Length::Fixed(TILE))
        .align_x(Alignment::Center)
        .into(),
    ])
    .spacing(3)
    .width(Length::Fixed(TILE))
    .align_x(Alignment::Center)
    .into()
}

/// The Devices card. Nothing connected says so; more devices than fit say
/// how many more in the header.
pub fn devices<'a, M: Clone + 'a>(
    list: &[Device],
    config: &Config,
    dimensions: Option<[f32; 2]>,
) -> Element<'a, M> {
    let _style = crate::widget_style::scope(config);
    let p = config.palette();
    let [width, height] = dimensions.unwrap_or_else(|| super::size(Widget::Devices, config));
    let room = fits(width, height);
    let shown = list.len().min(room);
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
        .align_y(Alignment::Center)
        .into()
    } else {
        container(
            row::with_children(
                list.iter()
                    .take(shown)
                    .map(|d| tile(d, p))
                    .collect::<Vec<_>>(),
            )
            .spacing(GAP)
            .wrap(),
        )
        .width(Length::Fill)
        .height(Length::Fill)
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
            "WH-1000XM5",
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
        // The default card fits a row of three or four.
        assert!((3..=4).contains(&fits(280., 160.)));
        // Taller cards take a second row.
        assert!(fits(280., 280.) > fits(280., 160.));
        assert!(fits(10., 10.) >= 1);
    }
}
