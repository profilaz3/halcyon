//! One clock renderer for the desktop, Daily overview and Settings preview.
use super::*;
use crate::config::{ClockAlignment, ClockPreferences};
use crate::widget_style::resolver::{ClockFace, ClockReading, Surface};
use cosmic::iced::advanced::text::{self as advanced_text, Paragraph};
use cosmic::iced::widget::{pin, stack};
use cosmic::iced::{Point, Size};
mod analog;
#[cfg(test)]
mod foundation_tests;

#[cfg(test)]
fn supported_font(mut font: Font) -> Font {
    if let cosmic::iced::font::Family::Name(name) = font.family
        && !crate::desktop_theme::font_installed(name)
    {
        font.family = cosmic::iced::font::Family::SansSerif;
    }
    font
}
#[cfg(test)]
fn font() -> Font {
    supported_font(crate::widget_style::clock_font())
}

/// All style choices are captured before measurement; no Config or ambient
/// font context is consulted while constructing the resulting frame.
#[derive(Clone, Copy)]
pub(super) struct Presentation {
    preferences: ClockPreferences,
    style: crate::widget_style::resolver::Style,
    body_ink: Color,
    legacy_bearing: bool,
    /// A soft edge in the opposite tone behind clock text drawn straight on
    /// the picture, so it stays readable where the picture matches its ink.
    halo: Option<Color>,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Geometry {
    pub viewport: [f32; 2],
    pub content_width: f32,
    pub face_size: f32,
    pub date_size: f32,
    pub padding: f32,
    pub gap: f32,
}
pub(super) struct Frame {
    presentation: Presentation,
    reading: ClockReading,
    date: String,
    pub geometry: Geometry,
}
impl Presentation {
    pub fn new(config: &Config) -> Self {
        Self::project(config, config.wallpaper_palette(), false)
    }
    pub fn part(config: &Config, palette: Palette) -> Self {
        Self::project(config, palette, false)
    }
    pub fn grouped(config: &Config, palette: Palette) -> Self {
        Self::project(config, palette, true)
    }
    fn project(config: &Config, palette: Palette, grouped: bool) -> Self {
        let _scope = crate::widget_style::scope(config);
        let _fonts = grouped.then(|| {
            crate::widget_style::fonts(
                config.glance.font.as_deref(),
                config.glance.clock_font.as_deref(),
            )
        });
        let mut style = crate::widget_style::presentation(config, palette, true);
        if grouped {
            // The group owns its surface and ink; Clock owns face and type.
            style.ink.value = palette.foreground;
            style.muted.value = palette.muted;
            style.accent.value = palette.accent;
        }
        Self {
            preferences: config.clock,
            body_ink: crate::widget_style::semantic(style.muted.value),
            style,
            legacy_bearing: config.moment_appearance().family.is_none()
                && config.moment_clock().weight.is_none()
                && config.moment_clock().italic.is_none(),
            halo: (!grouped && style.surface.value == Surface::None)
                .then(|| halo_for(style.ink.value)),
        }
    }
    pub fn metrics(self) -> crate::widget_presentation::Metrics {
        crate::widget_presentation::metrics(Widget::Clock)
    }
    pub fn fit(self, width: f32, requested: f32) -> f32 {
        if self.style.clock_face.value == ClockFace::Analog {
            return requested.min(width.max(1.) / 1.05);
        }
        fit_with(
            &self.preferences,
            width,
            requested,
            self.style.display.value,
            self.style.body.value,
        )
    }
}
fn clock_label(
    value: impl Into<String>,
    size: f32,
    color: Color,
    font: Font,
    halo: Option<Color>,
) -> Element<'static, Message> {
    let value = value.into();
    let size = size.max(crate::widget_style::META);
    haloed(
        |color| {
            text::body(value.clone())
                .size(size)
                .line_height(1.25)
                .font(font)
                .class(cosmic::theme::Text::Color(color))
                .into()
        },
        color,
        halo,
        size,
    )
}
/// Black behind light ink and white behind dark ink, faint enough to read
/// as a soft shadow rather than an outline.
fn halo_for(ink: Color) -> Color {
    let tone = if crate::widget_style::luminance(ink) > 0.18 {
        Color::BLACK
    } else {
        Color::WHITE
    };
    Color { a: 0.22, ..tone }
}
/// `glyphs` in `ink`, over eight faint copies in `halo` nudged a few pixels
/// out, scaled to the text. The text keeps its own size and place.
fn haloed(
    glyphs: impl Fn(Color) -> Element<'static, Message>,
    ink: Color,
    halo: Option<Color>,
    size: f32,
) -> Element<'static, Message> {
    let Some(halo) = halo else {
        return glyphs(ink);
    };
    let r = (size / 60.).clamp(1., 3.);
    let edge = [
        (1., 0.),
        (-1., 0.),
        (0., 1.),
        (0., -1.),
        (0.7, 0.7),
        (-0.7, 0.7),
        (0.7, -0.7),
        (-0.7, -0.7),
    ]
    .map(|(x, y)| pin(glyphs(halo)).position(Point::new(x * r, y * r)).into());
    stack(vec![glyphs(ink)])
        .push_under(Element::new(Unread(stack(edge).into())))
        .into()
}
/// Draws its content but reports none of it to widget operations, so the
/// halo's copies of the time are not found, focused or read aloud: only the
/// ink text above them is.
struct Unread(Element<'static, Message>);
impl cosmic::iced::advanced::Widget<Message, cosmic::Theme, cosmic::Renderer> for Unread {
    fn children(&self) -> Vec<cosmic::iced::advanced::widget::Tree> {
        vec![cosmic::iced::advanced::widget::Tree::new(&self.0)]
    }
    fn diff(&mut self, tree: &mut cosmic::iced::advanced::widget::Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.0));
    }
    fn size(&self) -> Size<Length> {
        self.0.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut cosmic::iced::advanced::widget::Tree,
        renderer: &cosmic::Renderer,
        limits: &cosmic::iced::advanced::layout::Limits,
    ) -> cosmic::iced::advanced::layout::Node {
        let child = self
            .0
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        cosmic::iced::advanced::layout::Node::with_children(child.size(), vec![child])
    }
    fn draw(
        &self,
        tree: &cosmic::iced::advanced::widget::Tree,
        renderer: &mut cosmic::Renderer,
        theme: &cosmic::Theme,
        style: &cosmic::iced::advanced::renderer::Style,
        layout: cosmic::iced::advanced::Layout<'_>,
        cursor: cosmic::iced::mouse::Cursor,
        viewport: &cosmic::iced::Rectangle,
    ) {
        if let Some(child) = layout.children().next() {
            self.0.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                child,
                cursor,
                viewport,
            );
        }
    }
}
type NativeParagraph = <cosmic::Renderer as advanced_text::Renderer>::Paragraph;
fn paragraph(value: &str, size: f32, font: Font) -> NativeParagraph {
    NativeParagraph::with_text(advanced_text::Text {
        content: value,
        bounds: Size::new(f32::INFINITY, f32::INFINITY),
        size: size.into(),
        line_height: advanced_text::LineHeight::Relative(1.05),
        font,
        align_x: advanced_text::Alignment::Left,
        align_y: cosmic::iced::alignment::Vertical::Top,
        shaping: advanced_text::Shaping::Auto,
        wrapping: advanced_text::Wrapping::None,
        ellipsize: advanced_text::Ellipsize::None,
    })
}

pub(super) fn measure(value: &str, size: f32, font: Font) -> f32 {
    paragraph(value, size, font).min_bounds().width
}

fn ink_edges(value: &str, size: f32, font: Font) -> [f32; 2] {
    use cosmic::iced::advanced::graphics::text::{cosmic_text::SwashCache, font_system};
    thread_local! {
        static INK: std::cell::RefCell<SwashCache> = std::cell::RefCell::new(SwashCache::new());
    }
    let paragraph = paragraph(value, size, font);
    INK.with_borrow_mut(|cache| {
        // Resizing must not accumulate a cache entry for every historical size.
        if cache.image_cache.len() > 128 {
            cache.image_cache.clear();
        }
        let mut system = font_system().write().expect("Write font system");
        let mut edges = [f32::INFINITY, f32::NEG_INFINITY];
        for glyph in paragraph.buffer().layout_runs().flat_map(|run| run.glyphs) {
            let physical = glyph.physical((0., 0.), 1.);
            if let Some(image) = cache.get_image(system.raw(), physical.cache_key) {
                let left = (physical.x + image.placement.left) as f32;
                edges[0] = edges[0].min(left);
                edges[1] = edges[1].max(left + image.placement.width as f32);
            }
        }
        if edges[0].is_finite() {
            edges
        } else {
            [0., paragraph.min_bounds().width]
        }
    })
}

pub(super) fn ink_offset(time: &str, size: f32) -> f32 {
    let bearing = match time.as_bytes().first() {
        Some(b'0' | b'6') => 66.,
        Some(b'1') => 43.,
        Some(b'2') => 33.,
        Some(b'3') => 20.,
        Some(b'4') => 45.,
        Some(b'5') => 34.,
        Some(b'7') => 30.,
        Some(b'8' | b'9') => 53.,
        _ => 0.,
    };
    size * bearing / 1000.
}

/// Fit a full five-character time (and AM/PM) before rendering, so narrow
/// frames never clip or wrap and the font does not jump on each minute tick.
#[cfg(test)]
pub(super) fn fit(preferences: &ClockPreferences, width: f32, requested: f32) -> f32 {
    fit_with(
        preferences,
        width,
        requested,
        font(),
        crate::widget_style::font(),
    )
}
fn fit_with(
    preferences: &ClockPreferences,
    width: f32,
    requested: f32,
    display: Font,
    body: Font,
) -> f32 {
    let width = (width - 2.).max(1.);
    let time_width = measure("00:00", requested, display);
    let mut needed = time_width;
    let gap = if preferences.format_24h { 0. } else { 6. };
    if !preferences.format_24h {
        needed += measure("PM", period_size(requested), body);
    }
    let mut ratio = ((width - gap).max(1.) / needed.max(1.)).min(1.);
    if !preferences.format_24h {
        // The small label stops shrinking at META, even when the clock does not.
        let minimum_period = measure("PM", crate::widget_style::META, body);
        ratio = ratio.min((width - gap - minimum_period).max(1.) / time_width.max(1.));
    }
    (requested * ratio).max(1.)
}

fn period_size(clock_size: f32) -> f32 {
    (clock_size * 0.2).max(crate::widget_style::META)
}

impl Presentation {
    pub fn face(self, reading: ClockReading, width: f32, size: f32) -> Element<'static, Message> {
        let p = self.style.palette();
        if self.style.clock_face.value == ClockFace::Analog {
            return analog::view(
                reading.clone(),
                width,
                size * 1.05,
                p,
                self.style.display.value,
                self.preferences.alignment,
            );
        }
        let ClockReading { time, period, .. } = reading;
        let mut content_width = measure(&time, size, self.style.display.value);
        // Center visible glyphs, not their advances: a large leading digit and a
        // small AM/PM have very different side bearings, especially with fallbacks.
        let mut ink = if self.preferences.alignment == ClockAlignment::Center {
            ink_edges(&time, size, self.style.display.value)
        } else {
            [0., content_width]
        };
        let offset = if self.legacy_bearing {
            ink_offset(&time, size)
        } else {
            0.
        };
        let display = self.style.display.value;
        let mut items = vec![haloed(
            |color| {
                text::body(time.clone())
                    .size(size)
                    .line_height(1.05)
                    .font(display)
                    .wrapping(advanced_text::Wrapping::None)
                    .class(cosmic::theme::Text::Color(color))
                    .into()
            },
            p.foreground,
            self.halo,
            size,
        )];
        if let Some(period) = period {
            if self.preferences.alignment == ClockAlignment::Center {
                let period_ink = ink_edges(&period, period_size(size), self.style.body.value);
                ink[1] = ink[1].max(content_width + 6. + period_ink[1]);
            }
            content_width += 6. + measure(&period, period_size(size), self.style.body.value);
            items.push(
                container(clock_label(
                    period,
                    period_size(size),
                    self.body_ink,
                    self.style.body.value,
                    self.halo,
                ))
                .padding([0., 0., size * 0.08, 0.])
                .into(),
            );
        }
        let x = match self.preferences.alignment {
            ClockAlignment::Left => -offset,
            ClockAlignment::Center => (width - ink[0] - ink[1]) / 2.,
            ClockAlignment::Right => width - content_width,
        };
        let content = row::with_children(items).spacing(6).align_y(Alignment::End);
        stack(vec![pin(content).position(Point::new(x, 0.)).into()])
            .width(width)
            .height(size * 1.05)
            .into()
    }
}

impl Presentation {
    pub fn frame(self, now: chrono::NaiveDateTime, size: [f32; 2]) -> Frame {
        let size = crate::widget_presentation::viewport(size, self.metrics().natural);
        let padding = f32::from(self.style.padding.value);
        let width = (size[0] - padding * 2.).max(1.);
        let height = (size[1] - padding * 2.).max(1.);
        let scale = (size[0] / self.metrics().natural[0])
            .min(size[1] / self.metrics().natural[1])
            .clamp(0.1, 4.)
            * f32::from(self.preferences.scale)
            / 100.;
        let mut font_size = self.fit(width, 104. * scale);
        let date = now.format("%A, %B %-d").to_string();
        let gap = if self.preferences.show_date {
            f32::from(self.style.gap.value)
        } else {
            0.
        };
        if self.preferences.show_date {
            let date_width = measure(&date, 20. * font_size / 104., self.style.body.value);
            font_size *= (width / date_width.max(1.)).min(1.);
        }
        let height_per_em = 1.05
            + if self.preferences.show_date {
                25. / 104.
            } else {
                0.
            };
        let date_size = if self.style.clock_face.value == ClockFace::Analog {
            // The dial owns the space; its date stays a readable caption rather
            // than inheriting the digital face's large-type proportions.
            let requested = (height * 0.08).clamp(9., 18.);
            let date_size = requested
                * (width / measure(&date, requested, self.style.body.value).max(1.)).min(1.);
            let date_height = if self.preferences.show_date {
                date_size * 1.25
            } else {
                0.
            };
            let available = width.min((height - gap - date_height).max(1.)) / 1.05;
            font_size = (available * 0.9 * f32::from(self.preferences.scale) / 100.).min(available);
            date_size
        } else {
            font_size = font_size.min((height - gap).max(1.) / height_per_em);
            20. * font_size / 104.
        };
        Frame {
            presentation: self,
            reading: ClockReading::at(now.time(), self.preferences.format_24h),
            date,
            geometry: Geometry {
                viewport: size,
                content_width: width,
                face_size: font_size,
                date_size,
                padding,
                gap,
            },
        }
    }
}
impl Frame {
    pub(super) fn face_rows(&self) -> [f32; 2] {
        let g = self.geometry;
        let face = g.face_size * 1.05;
        let content = face
            + if self.presentation.preferences.show_date {
                g.gap + g.date_size * 1.25
            } else {
                0.
            };
        let top = g.padding
            + if self.presentation.preferences.center_vertically {
                ((g.viewport[1] - 2. * g.padding - content) / 2.).max(0.)
            } else {
                0.
            };
        [top, top + face]
    }
    pub fn view(self) -> Element<'static, Message> {
        let Self {
            presentation,
            reading,
            date,
            geometry,
        } = self;
        let Geometry {
            viewport: size,
            content_width: width,
            face_size: font_size,
            date_size,
            padding,
            gap,
        } = geometry;
        let mut content = vec![presentation.face(reading, width, font_size)];
        if presentation.preferences.show_date {
            content.push(
                container(clock_label(
                    date,
                    date_size,
                    presentation.body_ink,
                    presentation.style.body.value,
                    presentation.halo,
                ))
                .width(Length::Fill)
                .align_x(presentation.preferences.alignment.layout())
                .into(),
            );
        }
        container(
            column::with_children(content)
                .spacing(gap)
                .width(Length::Fill),
        )
        .width(size[0])
        .height(size[1])
        .padding(padding)
        .class(cosmic::theme::Container::custom(move |_| {
            cosmic::iced::widget::container::Style {
                background: match presentation.style.surface.value {
                    Surface::None => None,
                    Surface::Solid(color) => Some(color.into()),
                },
                border: cosmic::iced::Border {
                    radius: f32::from(presentation.style.radius.value).into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .align_y(if presentation.preferences.center_vertically {
            Alignment::Center
        } else {
            Alignment::Start
        })
        .into()
    }
}

/// Compatibility entry for existing callers; both consumers use the same frame.
pub(crate) fn view(
    config: &Config,
    now: chrono::NaiveDateTime,
    size: [f32; 2],
) -> Element<'static, Message> {
    view_context(config, now, size, crate::widget_presentation::Context::Live)
}
pub(super) fn view_context(
    config: &Config,
    now: chrono::NaiveDateTime,
    size: [f32; 2],
    context: crate::widget_presentation::Context,
) -> Element<'static, Message> {
    if let Some(recipe) = config.recipes.clock.as_ref() {
        return super::recipe::view(
            config,
            recipe,
            now,
            super::recipe::Reading::default(),
            context,
            size,
        );
    }
    Presentation::new(config).frame(now, size).view()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_test::Native;
    pub(super) fn ink(pixels: &[u8], width: usize) -> [usize; 4] {
        let mut result = [usize::MAX, usize::MAX, 0, 0];
        for (i, pixel) in pixels.chunks_exact(4).enumerate() {
            if pixel[3] > 40 {
                let (x, y) = (i % width, i / width);
                result[0] = result[0].min(x);
                result[1] = result[1].min(y);
                result[2] = result[2].max(x);
                result[3] = result[3].max(y);
            }
        }
        assert!(result[0] < result[2], "Clock must be visible");
        result
    }
    fn capture_alignment_failure(
        config: &Config,
        now: chrono::NaiveDateTime,
        size: [f32; 2],
        pixels: &[u8],
    ) {
        use cosmic::iced::advanced::graphics::text::font_system;
        let _style = crate::widget_style::scope(config);
        let scale = (size[0] / 490.).min(size[1] / 200.).clamp(0.1, 4.)
            * f32::from(config.clock.scale)
            / 100.;
        let font_size = fit(&config.clock, size[0] - 8., 104. * scale).min((size[1] - 8.) / 1.05);
        let (time, period) = clock_labels(now.time(), config.clock.format_24h);
        let mut report = format!(
            "size={size:?} clock_size={font_size} requested_clock={:?} requested_body={:?} renderer_default={:?}\n",
            font(),
            crate::widget_style::font(),
            cosmic::font::default(),
        );
        for (value, size, selected_font) in [
            (time, font_size, font()),
            (
                period.unwrap_or_default(),
                period_size(font_size),
                crate::widget_style::font(),
            ),
        ] {
            let paragraph = paragraph(&value, size, selected_font);
            report.push_str(&format!(
                "text={value:?} min={:?} ink={:?}\n",
                paragraph.min_bounds(),
                ink_edges(&value, size, selected_font)
            ));
            for run in paragraph.buffer().layout_runs() {
                for glyph in run.glyphs {
                    report.push_str(&format!("{glyph:?}\n"));
                }
            }
        }
        for face in font_system().read().unwrap().db().faces() {
            report.push_str(&format!(
                "font_id={:?} families={:?} weight={:?} style={:?} stretch={:?}\n",
                face.id, face.families, face.weight, face.style, face.stretch,
            ));
        }
        eprintln!("{report}");
        let directory = std::path::Path::new("target/test-failures");
        std::fs::create_dir_all(directory).unwrap();
        std::fs::write(directory.join("clock-fonts.txt"), report).unwrap();
        image::save_buffer(
            directory.join("clock-alignment.png"),
            pixels,
            size[0] as u32,
            size[1] as u32,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }

    #[test]
    fn resized_clock_fits_and_aligns_the_actual_glyphs() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 10, 21)
            .unwrap()
            .and_hms_opt(23, 58, 0)
            .unwrap();
        let theme = cosmic::Theme::dark();
        let mut config = Config::default();
        for family in [None, Some("DejaVu Sans")] {
            config.widget_font = family.map(str::to_owned);
            for size in [[160., 100.], [300., 120.], [490., 200.], [620., 280.]] {
                let mut native = Native::new(Size::new(size[0], size[1]));
                for format in [true, false] {
                    for scale in [50, 100, 200] {
                        config.clock.format_24h = format;
                        config.clock.scale = scale;
                        for alignment in [
                            ClockAlignment::Left,
                            ClockAlignment::Center,
                            ClockAlignment::Right,
                        ] {
                            config.clock.alignment = alignment;
                            config.clock.show_date = false;
                            config.clock.center_vertically = true;
                            let pixels = native.render(view(&config, now, size), &theme);
                            let bounds = ink(&pixels, size[0] as usize);
                            assert!(
                                bounds[0] >= 2 && bounds[2] < size[0] as usize - 2,
                                "horizontal clipping {size:?} {format} {scale} {alignment:?} {bounds:?}"
                            );
                            assert!(
                                bounds[1] > 2 && bounds[3] < size[1] as usize - 2,
                                "vertical clipping {bounds:?}"
                            );
                            if alignment == ClockAlignment::Center {
                                if ((bounds[0] + bounds[2]) as f32 / 2. - size[0] / 2.).abs() >= 7.
                                {
                                    capture_alignment_failure(&config, now, size, &pixels);
                                }
                                assert!(
                                    ((bounds[0] + bounds[2]) as f32 / 2. - size[0] / 2.).abs() < 7.,
                                    "clock is not centered {family:?}/{size:?}/{format}/{scale}: {bounds:?}"
                                );
                            }
                            config.clock.show_date = true;
                            let bounds = ink(
                                &native.render(view(&config, now, size), &theme),
                                size[0] as usize,
                            );
                            assert!(
                                bounds[3] < size[1] as usize - 2,
                                "date must fit: {size:?} {bounds:?}"
                            );
                        }
                    }
                }
                config.clock.scale = 130;
                config.clock.alignment = ClockAlignment::Center;
                native.capture(
                    view(&config, now, size),
                    &theme,
                    &format!("clock-centered-{}", size[0]),
                );
            }
        }
        config.widget_font = None;
        let mut native = Native::new(Size::new(360., 148.));
        config.clock = ClockPreferences::default();
        let small = ink(
            &native.render(view(&config, now, [360., 148.]), &theme),
            360,
        );
        config.clock.scale = 150;
        let large = ink(
            &native.render(view(&config, now, [360., 148.]), &theme),
            360,
        );
        assert!(
            large[2] - large[0] > small[2] - small[0] + 40,
            "Text size changes visible glyphs"
        );
    }

    /// One size model: the box is the clock's size. Growing it at the
    /// same shape, as the Clock stage corner and + do, grows the digits and
    /// the dial with it; the Text size slider only fits them inside it.
    #[test]
    fn a_bigger_box_at_the_same_shape_means_bigger_digits() {
        use crate::widget_style::resolver::ClockFace;
        let now = chrono::NaiveDate::from_ymd_opt(2026, 3, 4)
            .unwrap()
            .and_hms_opt(10, 8, 0)
            .unwrap();
        let mut config = Config::default();
        for face in [ClockFace::Digital, ClockFace::Analog] {
            config.clock.face = Some(face);
            let face_size = |config: &Config, size: [f32; 2]| {
                let _style = crate::widget_style::scope(config);
                Presentation::new(config)
                    .frame(now, size)
                    .geometry
                    .face_size
            };
            let natural = face_size(&config, [490., 200.]);
            let tenth = face_size(&config, [539., 220.]);
            let larger = face_size(&config, [686., 280.]);
            assert!(
                natural < tenth && tenth < larger,
                "{face:?}: {natural} -> {tenth} -> {larger}"
            );
            // The digits scale with the box; the dial, which also makes
            // room for the date, grows at least as fast.
            let least = if face == ClockFace::Digital { 0.98 } else { 1. };
            assert!(
                tenth / natural >= 1.1 * least,
                "{face:?}: {natural} -> {tenth}"
            );
            assert!(
                larger / natural >= 1.4 * least,
                "{face:?}: {natural} -> {larger}"
            );
            if face == ClockFace::Digital {
                assert!(tenth / natural <= 1.12 && larger / natural <= 1.43);
            }
        }
    }

    /// A clock drawn straight on the picture gets a faint edge in the
    /// opposite tone, so light digits stay readable on a light patch of
    /// wallpaper and dark ones on a dark patch. A clock on a card, or in a
    /// group that owns its own card, doesn't need one.
    #[test]
    fn a_transparent_clock_has_a_soft_edge_in_the_opposite_tone() {
        use crate::widget_style::resolver::ClockSurface;
        let now = chrono::NaiveDate::from_ymd_opt(2026, 3, 4)
            .unwrap()
            .and_hms_opt(10, 8, 0)
            .unwrap();
        let theme = cosmic::Theme::dark();
        let mut native = Native::new(Size::new(490., 200.));
        let mut config = Config::default();
        // Pixels darker than the ink: only the edge draws them.
        let mut edge = |config: &Config| {
            let pixels = native.render(view(config, now, [490., 200.]), &theme);
            pixels
                .chunks_exact(4)
                .filter(|p| p[3] > 20 && p[0].max(p[1]).max(p[2]) < 90)
                .count()
        };
        for light in [false, true] {
            config.appearance.wallpaper_light = light;
            let ink = config.wallpaper_palette().foreground;
            let halo = Presentation::new(&config).halo.expect("transparent clock");
            assert_eq!(
                halo.r > 0.5,
                crate::widget_style::luminance(ink) < 0.18,
                "light: {light}"
            );
            assert!(halo.a > 0. && halo.a < 0.3, "{halo:?}");
        }
        config.appearance.wallpaper_light = false;
        assert!(edge(&config) > 200, "light digits get a dark edge");
        // The edge is drawn, not read: search and screen readers find the
        // time once.
        native.text_bounds(view(&config, now, [490., 200.]), "10:08");
        let palette = config.wallpaper_palette();
        assert!(Presentation::grouped(&config, palette).halo.is_none());
        config.clock.surface = Some(ClockSurface::Panel);
        assert!(Presentation::new(&config).halo.is_none());
    }
}
