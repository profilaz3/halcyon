//! The Clock page's face gallery and typeface strip. Each tile is a picture
//! drawn by the desktop's own clock renderer, so what a tile shows is what
//! choosing it puts on the desktop.
use super::*;
use crate::settings::FontTarget;
use crate::widget_style::clock_fonts;
use crate::widget_style::resolver::{ClockFace, ClockSurface};

/// A tile's picture height.
const PICTURE: f32 = 78.;
/// The room a tile's border, padding and name take around its picture.
const FRAME: f32 = 14.;
/// A tile is never narrower than this; the grid adds columns as room allows.
const MIN_TILE: f32 = 132.;

impl Settings {
    /// Face tiles: the look's own face first, then every face.
    pub(super) fn clock_face_gallery(&self, follow: &'static str) -> Element<'_, Message> {
        let target = self.clock_moment_target();
        let current = self.config.clock_layer(target).face;
        let accent = self.settings_accent();
        let choices: Vec<(Option<ClockFace>, &'static str, String)> =
            std::iter::once((None, follow, "match".to_string()))
                .chain(
                    ClockFace::ALL
                        .into_iter()
                        .map(|face| (Some(face), face.label(), face.key().to_string())),
                )
                .collect();
        let grid = cosmic::widget::responsive(move |bounds| {
            let width = tile_width(bounds.width);
            let tiles = choices
                .iter()
                .map(|(value, name, key)| {
                    let picture =
                        container(self.clock_face_picture(*value, [width - FRAME, PICTURE]))
                            .id(cosmic::widget::Id::new(format!("clock-face-{key}-picture")))
                            .into();
                    button::custom(
                        column::with_children(vec![picture, label(*name, 12., FG)]).spacing(6),
                    )
                    .id(cosmic::widget::Id::new(format!("clock-face-{key}")))
                    .class(super::profiles::tile_style(current == *value, accent))
                    .padding(7)
                    .width(width)
                    .on_press(self.clock_moment_edit(Message::ClockFace(*value)))
                    .into()
                })
                .collect::<Vec<_>>();
            row::with_children(tiles).spacing(8).wrap().into()
        })
        .height(Length::Shrink);
        container(grid)
            .id(cosmic::widget::Id::new("clock-face-gallery"))
            .width(Length::Fill)
            .into()
    }

    /// The clock as `face` would draw it on this desktop: the edited layer
    /// takes the face, the rest of the clock stays as it is.
    fn clock_face_picture(
        &self,
        face: Option<ClockFace>,
        size: [f32; 2],
    ) -> Element<'static, Message> {
        let mut config = self.canvas_config().clone();
        match self.clock_moment_target() {
            Some(crate::moments::Phase::Day) => config.moments.day.clock.face = face,
            Some(crate::moments::Phase::Night) => config.moments.night.clock.face = face,
            None => config.clock.face = face,
        }
        config.clock.surface = Some(ClockSurface::None);
        config.moments.day.clock.surface = None;
        config.moments.night.clock.surface = None;
        config.clock.scale = 100;
        config.clock.alignment = crate::config::ClockAlignment::Center;
        config.clock.center_vertically = true;
        // Split is the time beside the date; its picture always says so.
        config.clock.show_date = face == Some(ClockFace::Split);
        self.clock_tile_picture(&config, size)
    }

    /// Typeface tiles: the widget font, the shipped faces with what each is
    /// like, and every installed font behind the searchable picker.
    pub(super) fn clock_font_strip(&self) -> Element<'_, Message> {
        let current = self.config.clock_font.clone();
        let accent = self.settings_accent();
        let open = self.font_menu == Some(FontTarget::Clock);
        let installed = current
            .as_deref()
            .filter(|name| clock_fonts::bundled(name).is_none())
            .map(str::to_owned);
        let grid = cosmic::widget::responsive(move |bounds| {
            let width = tile_width(bounds.width);
            let size = [width - FRAME, PICTURE];
            let tile = |id: String, picture, name: String, note: &str, selected, press| {
                button::custom(
                    column::with_children(vec![
                        container(picture)
                            .id(cosmic::widget::Id::new(format!("{id}-picture")))
                            .into(),
                        column::with_children(vec![
                            label(name, 12., FG),
                            label(note.to_string(), 11., MUTED),
                        ])
                        .into(),
                    ])
                    .spacing(6),
                )
                .id(cosmic::widget::Id::new(id))
                .class(super::profiles::tile_style(selected, accent))
                .padding(7)
                .width(width)
                .on_press(press)
                .into()
            };
            let mut tiles: Vec<Element<'_, Message>> = vec![tile(
                "clock-font-widgets".into(),
                self.clock_font_picture(None, size),
                "Same as widgets".into(),
                "Follows the text font",
                current.is_none(),
                Message::WidgetFont(FontTarget::Clock, None),
            )];
            tiles.extend(clock_fonts::ALL.iter().map(|font| {
                tile(
                    format!(
                        "clock-font-{}",
                        font.family.to_lowercase().replace(' ', "-")
                    ),
                    self.clock_font_picture(Some(font.family), size),
                    font.family.into(),
                    font.kind,
                    current.as_deref() == Some(font.family),
                    Message::WidgetFont(FontTarget::Clock, Some(font.family.into())),
                )
            }));
            // An installed font in use shows as itself; otherwise the tile
            // opens every font on the system.
            tiles.push(match &installed {
                Some(name) => tile(
                    "clock-font-installed".into(),
                    self.clock_font_picture(Some(name), size),
                    name.clone(),
                    "Installed · change",
                    true,
                    Message::FontMenu(Some(FontTarget::Clock)),
                ),
                None => tile(
                    "clock-font-more".into(),
                    container(label(if open { "Close" } else { "Aa" }, 28., FG))
                        .center_x(size[0])
                        .center_y(size[1])
                        .into(),
                    "More fonts".into(),
                    "Any installed font",
                    open,
                    Message::FontMenu(Some(FontTarget::Clock)),
                ),
            });
            row::with_children(tiles).spacing(8).wrap().into()
        })
        .height(Length::Shrink);
        let mut rows = vec![
            container(grid)
                .id(cosmic::widget::Id::new("clock-font-gallery"))
                .width(Length::Fill)
                .into(),
        ];
        if open {
            rows.push(self.widget_font_row(
                FontTarget::Clock,
                "Installed fonts",
                self.config.clock_font.as_deref(),
                "Same as widgets",
            ));
        }
        column::with_children(rows).spacing(10).into()
    }

    /// The time in `font` (or the widget font), in the clock's current face.
    fn clock_font_picture(&self, font: Option<&str>, size: [f32; 2]) -> Element<'static, Message> {
        let mut config = self.canvas_config().clone();
        config.clock_font = font.map(str::to_owned);
        // A dial shows a font only in its numerals; the sample is the time.
        let face = {
            let _scope = crate::widget_style::scope(&config);
            crate::widget_style::presentation(&config, config.wallpaper_palette(), true)
                .clock_face
                .value
        };
        if face.dial() || face == ClockFace::Words {
            config.clock.face = Some(ClockFace::Digital);
            config.moments.day.clock.face = None;
            config.moments.night.clock.face = None;
        }
        config.clock.surface = Some(ClockSurface::None);
        config.moments.day.clock.surface = None;
        config.moments.night.clock.surface = None;
        config.clock.scale = 100;
        config.clock.show_date = false;
        config.clock.alignment = crate::config::ClockAlignment::Center;
        config.clock.center_vertically = true;
        self.clock_tile_picture(&config, size)
    }

    /// A clock drawn over a slice of this desktop's wallpaper, inside the
    /// tile's rounded rectangle.
    fn clock_tile_picture(
        &self,
        config: &crate::config::Config,
        size: [f32; 2],
    ) -> Element<'static, Message> {
        let clock = crate::app::settings_clock_tile(config, size).map(|_| Message::Preview);
        let radius = cosmic::theme::active().cosmic().corner_radii.radius_s;
        let background: Element<'static, Message> = match self
            .profiles
            .home_image()
            .and_then(|h| tile_backdrop(h, size))
        {
            Some(handle) => cosmic::widget::image(handle)
                .width(size[0])
                .height(size[1])
                .content_fit(cosmic::iced::ContentFit::Fill)
                .border_radius(radius)
                .into(),
            None => {
                let color = config.palette().background;
                container(cosmic::iced::widget::space())
                    .width(size[0])
                    .height(size[1])
                    .class(cosmic::theme::Container::custom(move |_| {
                        cosmic::iced::widget::container::Style {
                            background: Some(color.into()),
                            border: cosmic::iced::Border {
                                radius: radius.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    }))
                    .into()
            }
        };
        // Nothing the picture holds draws past it.
        container(
            cosmic::iced::widget::stack(vec![background, clock])
                .width(size[0])
                .height(size[1]),
        )
        .width(size[0])
        .height(size[1])
        .clip(true)
        .into()
    }
}

/// The middle of a `picture` sized image with the shape of a `tile`, as a
/// region of the image.
fn cover(picture: [u32; 2], tile: [f32; 2]) -> cosmic::iced::Rectangle<u32> {
    let [width, height] = picture.map(|side| side.max(1));
    let shape = tile[0].max(1.) / tile[1].max(1.);
    if width as f32 / height as f32 > shape {
        let cut = ((height as f32 * shape).round() as u32).clamp(1, width);
        cosmic::iced::Rectangle {
            x: (width - cut) / 2,
            y: 0,
            width: cut,
            height,
        }
    } else {
        let cut = ((width as f32 / shape).round() as u32).clamp(1, height);
        cosmic::iced::Rectangle {
            x: 0,
            y: (height - cut) / 2,
            width,
            height: cut,
        }
    }
}

/// The wallpaper cut to a tile's shape, so it draws exactly over the tile's
/// picture and its corners round with the tile. Covering a tile by drawing
/// the whole picture larger spills past it wherever the renderer does not
/// clip. Cut once per picture and tile shape.
fn tile_backdrop(
    handle: &cosmic::widget::image::Handle,
    tile: [f32; 2],
) -> Option<cosmic::widget::image::Handle> {
    use cosmic::widget::image::Handle;
    thread_local! {
        static CUT: std::cell::RefCell<Vec<((cosmic::iced::advanced::image::Id, [u32; 2]), Handle)>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }
    let Handle::Rgba {
        id,
        width,
        height,
        pixels,
    } = handle
    else {
        return None;
    };
    let key = (id.clone(), tile.map(|side| side.round() as u32));
    CUT.with_borrow_mut(|cut| {
        if let Some((_, handle)) = cut.iter().find(|(known, _)| *known == key) {
            return Some(handle.clone());
        }
        let region = cover([*width, *height], tile);
        let row = *width as usize * 4;
        if pixels.len() < row * *height as usize {
            return None;
        }
        let mut rgba = Vec::with_capacity(region.width as usize * region.height as usize * 4);
        for y in region.y..region.y + region.height {
            let start = y as usize * row + region.x as usize * 4;
            rgba.extend_from_slice(&pixels[start..start + region.width as usize * 4]);
        }
        let handle = Handle::from_rgba(region.width, region.height, rgba);
        if cut.len() >= 16 {
            cut.remove(0);
        }
        cut.push((key, handle.clone()));
        Some(handle)
    })
}

/// The widest tile that keeps whole columns of at least `MIN_TILE`.
fn tile_width(available: f32) -> f32 {
    let columns = ((available + 8.) / (MIN_TILE + 8.)).floor().clamp(1., 4.);
    ((available - (columns - 1.) * 8.) / columns)
        .floor()
        .max(1.)
}

#[cfg(test)]
mod tests {
    use super::{cover, tile_width};

    #[test]
    fn a_tile_shows_the_middle_of_the_picture_in_its_shape() {
        let wide = cover([1920, 1080], [118., 78.]);
        assert_eq!((wide.y, wide.height), (0, 1080));
        assert_eq!(wide.width, (1080. * 118. / 78f32).round() as u32);
        assert_eq!(wide.x, (1920 - wide.width) / 2);
        let tall = cover([600, 1200], [118., 78.]);
        assert_eq!((tall.x, tall.width), (0, 600));
        assert_eq!(tall.height, (600. * 78. / 118f32).round() as u32);
        assert_eq!(tall.y, (1200 - tall.height) / 2);
    }

    #[test]
    fn tiles_fill_whole_columns() {
        assert_eq!(tile_width(100.), 100.);
        assert_eq!(tile_width(280.), 136.);
        assert!(tile_width(600.) >= super::MIN_TILE);
        // Never more than four across, so pictures stay legible.
        assert_eq!(tile_width(2000.), ((2000. - 24.) / 4f32).floor());
    }
}
