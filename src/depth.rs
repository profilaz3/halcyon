//! Prepared wallpaper depth. Only the clock's alpha changes: no wallpaper
//! repaint, new surface, provider or input region. All raster work is child-owned.
use crate::config::Config;
use crate::desktop::store::{DesktopStore, widget};
use crate::desktop::wallpaper::media::{Pixels, worker};
use crate::desktop::wallpaper::{self, BgEntry, BgSource, ScalingMode, Target, WallpaperDraft};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;

pub const PICTURE: &[u8] = include_bytes!("../assets/wallpapers/halcyon-ridge.png");
const MATTE: &[u8] = include_bytes!("../assets/depth/halcyon-ridge.png");
mod lighting;
pub use lighting::Lighting;
pub const MAX_INPUT: usize = 270_336;
pub const MAX_RASTER: [u32; 2] = [2048, 1024];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub screen: [f32; 2],
    pub origin: [f32; 2],
    pub size: [f32; 2],
    pub scale: f32,
}
impl Frame {
    pub fn pixels(&self) -> Option<[u32; 2]> {
        if !self
            .screen
            .into_iter()
            .all(|v| v.is_finite() && (1. ..=8192.).contains(&v))
            || !self.origin.into_iter().all(|v| v.is_finite() && v >= 0.)
            || !self.size.into_iter().all(|v| v.is_finite() && v >= 1.)
            || ![1., 1.5, 2.].contains(&self.scale)
            || (0..2).any(|i| self.origin[i] + self.size[i] > self.screen[i] + 0.01)
        {
            return None;
        }
        let pixels = self.size.map(|v| (v * self.scale).ceil() as u32);
        (pixels[0] <= MAX_RASTER[0] && pixels[1] <= MAX_RASTER[1]).then_some(pixels)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub frame: Frame,
    pub background: BgEntry,
    pub style: serde_json::Value,
    pub time: String,
}
impl Request {
    pub fn new(
        config: &Config,
        frame: Frame,
        background: BgEntry,
        time: chrono::NaiveDateTime,
    ) -> Option<Self> {
        frame.pixels()?;
        if !eligible(config) {
            return None;
        }
        let value = serde_json::to_value(config).ok()?;
        let mut style = serde_json::json!({});
        for field in widget::fields(widget::WidgetGroup::Style) {
            if let Some(value) = widget::get(&value, field.pointer) {
                widget::put(&mut style, field.pointer, Some(value)).ok()?;
            }
        }
        Some(Self {
            frame,
            background,
            style,
            time: time.format("%Y-%m-%dT%H:%M:00").to_string(),
        })
    }
    fn config(&self) -> Option<Config> {
        if !self.style.is_object() {
            return None;
        }
        let mut value = serde_json::to_value(Config::default()).ok()?;
        for field in widget::fields(widget::WidgetGroup::Style) {
            if let Some(member) = widget::get(&self.style, field.pointer) {
                widget::put(&mut value, field.pointer, Some(member)).ok()?;
            }
        }
        crate::config::check_config_value(&value).ok()?;
        let config = Config::from_bytes(&serde_json::to_vec(&value).ok()?).ok()?;
        eligible(&config).then_some(config)
    }
}

pub fn eligible(config: &Config) -> bool {
    use crate::widget_style::resolver::{ClockFace, Surface};
    let _scope = crate::widget_style::scope(config);
    let style = crate::widget_style::presentation(config, config.wallpaper_palette(), true);
    config.clock.depth
        && config.recipes.clock.is_none()
        && !config.glance.enabled
        && style.clock_face.value == ClockFace::Digital
        && style.surface.value == Surface::None
        && !config.appearance.contrast_assist
}

/// Read exactly the output being rendered, without discovering other displays.
pub fn observe(output: &str) -> Option<BgEntry> {
    let store = DesktopStore::from_env().ok()?;
    observe_from(&store, output)
}
fn observe_from(store: &DesktopStore, output: &str) -> Option<BgEntry> {
    let mut keys = WallpaperDraft::keys();
    keys.push(wallpaper::output_key(output).ok()?);
    let snapshot = store.load(&keys).ok()?;
    WallpaperDraft::from_parts(snapshot, vec![output.into()])
        .effective(&Target::Output(output.into()))
        .known()
        .cloned()
}

/// Immutable identity, checked on one bounded regular-file descriptor. A renamed
/// identical copy is valid; a replaced file, folder, device or other picture is not.
pub fn matches_picture(entry: &BgEntry) -> bool {
    use std::os::unix::fs::OpenOptionsExt;
    let BgSource::Path(path) = &entry.source else {
        return false;
    };
    let Ok(file) = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
    else {
        return false;
    };
    let Ok(meta) = file.metadata() else {
        return false;
    };
    if !meta.is_file() || meta.len() != PICTURE.len() as u64 {
        return false;
    }
    let mut bytes = Vec::new();
    file.take(PICTURE.len() as u64 + 1)
        .read_to_end(&mut bytes)
        .is_ok()
        && bytes.len() == PICTURE.len()
        && Sha256::digest(&bytes) == Sha256::digest(PICTURE)
}

/// COSMIC scales then center-crops. Keep the rounded resized dimensions and
/// integer offset, rather than rounding a source crop before scaling.
fn transform(frame: &Frame, mode: &ScalingMode, source: [u32; 2]) -> Option<([f64; 2], [f64; 2])> {
    frame.pixels()?;
    let factor = f64::from(frame.scale);
    let target = frame.screen.map(|v| (f64::from(v) * factor).round());
    let source = source.map(f64::from);
    let ratio = match mode {
        ScalingMode::Zoom => (target[0] / source[0]).max(target[1] / source[1]),
        ScalingMode::Fit(_) => (target[0] / source[0]).min(target[1] / source[1]),
        ScalingMode::Stretch => return None,
    };
    let scaled = source.map(|v| (v * ratio).round().max(1.));
    let offset = [0, 1]
        .map(|i| ((target[i] - scaled[i]).abs() / 2.).floor() * (target[i] - scaled[i]).signum());
    Some((
        [
            scaled[0] / source[0] / factor,
            scaled[1] / source[1] / factor,
        ],
        offset.map(|v| v / factor),
    ))
}

/// One-channel prepared mask; only antialiasing changes alpha. The generated
/// asset's tiny near-black/white noise is normalized, not interpreted as texture.
fn mask() -> Option<image::GrayImage> {
    let image = image::load_from_memory_with_format(MATTE, image::ImageFormat::Png).ok()?;
    if image.width() != 1672
        || image.height() != 941
        || u64::from(image.width()) * u64::from(image.height()) > 8_500_000
    {
        return None;
    }
    let mut image = image.to_luma8();
    for pixel in image.pixels_mut() {
        let value = &mut pixel[0];
        *value = if *value < 16 {
            0
        } else if *value > 239 {
            255
        } else {
            *value
        };
    }
    Some(image)
}
/// The compiled Ridge picture, decoded only when scene lighting is on.
fn picture() -> Option<image::RgbImage> {
    let image = image::load_from_memory_with_format(PICTURE, image::ImageFormat::Png).ok()?;
    (image.width() == 1672 && image.height() == 941).then(|| image.to_rgb8())
}

fn apply_mask(
    pixels: &mut Pixels,
    frame: &Frame,
    mode: &ScalingMode,
    mask: &image::GrayImage,
    face: Option<[f32; 2]>,
) -> Option<()> {
    let (scale, offset) = transform(frame, mode, [mask.width(), mask.height()])?;
    let alpha = |x: f64, y: f64| -> f64 {
        if x < 0. || y < 0. || x >= f64::from(mask.width()) || y >= f64::from(mask.height()) {
            return 0.;
        }
        let x0 = x.floor() as u32;
        let y0 = y.floor() as u32;
        let x1 = (x0 + 1).min(mask.width() - 1);
        let y1 = (y0 + 1).min(mask.height() - 1);
        let tx = x.fract();
        let ty = y.fract();
        let at = |x, y| f64::from(mask.get_pixel(x, y)[0]);
        (at(x0, y0) * (1. - tx) + at(x1, y0) * tx) * (1. - ty)
            + (at(x0, y1) * (1. - tx) + at(x1, y1) * tx) * ty
    };
    for (i, pixel) in pixels.rgba.chunks_exact_mut(4).enumerate() {
        let x = (i as u32 % pixels.width) as f64;
        let y = (i as u32 / pixels.width) as f64;
        let position = [x, y].map(|v| (v + 0.5) / f64::from(frame.scale));
        if face.is_some_and(|rows| {
            position[1] < f64::from(rows[0]) || position[1] >= f64::from(rows[1])
        }) {
            continue;
        }
        let source =
            [0, 1].map(|i| (f64::from(frame.origin[i]) + position[i] - offset[i]) / scale[i] - 0.5);
        let keep = (1. - alpha(source[0], source[1]) / 255.).clamp(0., 1.);
        // Headless screenshot pixels are straight RGBA. Do not darken RGB.
        pixel[3] = (f64::from(pixel[3]) * keep).round() as u8;
    }
    Some(())
}

pub fn render(request: &Request) -> Option<Pixels> {
    if !matches_picture(&request.background) {
        return None;
    }
    render_clock(request)
}
/// Public prepared pairs never consult a local path. Call only when the
/// selected public background is the compiled Ridge asset.
pub(crate) fn public_clock(
    config: &Config,
    frame: Frame,
    mode: ScalingMode,
    time: chrono::NaiveDateTime,
) -> Option<Pixels> {
    let mut entry = BgEntry::new("all", BgSource::Color(wallpaper::BgColor::Single([0.; 3])));
    entry.scaling_mode = mode;
    render_clock(&Request::new(config, frame, entry, time)?)
}
fn render_clock(request: &Request) -> Option<Pixels> {
    use cosmic::iced::advanced::renderer::{Headless, Style};
    use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
    use cosmic::iced::{Color, Size, mouse};
    let size = request.frame.pixels()?;
    let config = request.config()?;
    let mask = mask()?;
    transform(
        &request.frame,
        &request.background.scaling_mode,
        [mask.width(), mask.height()],
    )?;
    let time = chrono::NaiveDateTime::parse_from_str(&request.time, "%Y-%m-%dT%H:%M:%S").ok()?;
    crate::desktop_theme::load_installed_fonts();
    let mut renderer =
        cosmic::iced::futures::executor::block_on(<cosmic::Renderer as Headless>::new(
            cosmic::font::default(),
            cosmic::iced::Pixels(14.),
            Some("tiny-skia"),
        ))?;
    let view = crate::app::settings_clock_preview_at(&config, request.frame.size, time);
    let mut ui = UserInterface::build(
        view,
        Size::new(request.frame.size[0], request.frame.size[1]),
        Cache::default(),
        &mut renderer,
    );
    ui.draw(
        &mut renderer,
        &cosmic::Theme::dark(),
        &Style::default(),
        mouse::Cursor::Unavailable,
    );
    let mut pixels = Pixels {
        width: size[0],
        height: size[1],
        rgba: renderer.screenshot(
            Size::new(size[0], size[1]),
            request.frame.scale,
            Color::TRANSPARENT,
        ),
    };
    // Tiny-skia's screenshot is premultiplied; native image handles consume
    // straight RGBA. Normalize before reducing alpha, so edges stay untinted.
    for p in pixels.rgba.chunks_exact_mut(4) {
        let a = u32::from(p[3]);
        for c in &mut p[..3] {
            *c = if a == 0 {
                0
            } else {
                ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8
            };
        }
    }
    let face = crate::app::settings_clock_face_rows(&config, request.frame.size, time);
    let face_ink = |pixels: &Pixels| -> u64 {
        pixels
            .rgba
            .chunks_exact(4)
            .enumerate()
            .filter(|(i, _)| {
                let y = (*i as u32 / pixels.width) as f32 / request.frame.scale;
                y >= face[0] && y < face[1]
            })
            .map(|(_, p)| u64::from(p[3]))
            .sum()
    };
    let before = face_ink(&pixels);
    let recipe = config.clock.lighting.recipe();
    let glyph: Vec<u8> = if recipe.is_some() {
        pixels.rgba.chunks_exact(4).map(|p| p[3]).collect()
    } else {
        Vec::new()
    };
    apply_mask(
        &mut pixels,
        &request.frame,
        &request.background.scaling_mode,
        &mask,
        Some(face),
    )?;
    let after = face_ink(&pixels);
    // Keep the time readable. A clock moved mostly into the foreground uses
    // its plain presentation instead of becoming an invisible widget.
    if before == 0 || after * 100 < before * 55 {
        return None;
    }
    // Lighting only recolours what the matte left visible. If it cannot run,
    // the plain masked clock is still correct.
    if let Some(recipe) = recipe
        && let Some(picture) = picture()
    {
        let _ = lighting::apply(
            &mut pixels,
            &glyph,
            &request.frame,
            &request.background.scaling_mode,
            &picture,
            &mask,
            face,
            &recipe,
        );
    }
    Some(pixels)
}

pub(crate) fn main() -> ! {
    worker::limits();
    let result = (|| -> Option<()> {
        let mut bytes = Vec::new();
        std::io::stdin()
            .take(MAX_INPUT as u64 + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > MAX_INPUT {
            return None;
        }
        let request: Request = serde_json::from_slice(&bytes).ok()?;
        worker::ready()?;
        worker::write_pixels(&render(&request)?)
    })();
    std::process::exit(if result.is_some() { 0 } else { 2 });
}

#[cfg(test)]
mod tests;
