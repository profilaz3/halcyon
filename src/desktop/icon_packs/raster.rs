//! Icons to square RGBA pictures: SVG through resvg, PNG through image,
//! both fitted and centered with their aspect kept. Pixels are straight
//! (not premultiplied) RGBA.
use image::RgbaImage;
use std::path::Path;

/// The largest icon file read.
const MAX_FILE: usize = 4 * 1024 * 1024;

/// The icon at `path` as a `size` by `size` picture.
pub fn load(path: &Path, size: u32) -> Option<RgbaImage> {
    if size == 0 || size > 1024 {
        return None;
    }
    let bytes = super::read(path, MAX_FILE)?;
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("svg" | "svgz") => svg(&bytes, size),
        _ => png(&bytes, size),
    }
}

/// Renders SVG (or gzipped SVG) data.
pub fn svg(bytes: &[u8], size: u32) -> Option<RgbaImage> {
    use resvg::{tiny_skia, usvg};
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(bytes, &options).ok()?;
    let (width, height) = (tree.size().width(), tree.size().height());
    if !(width > 0. && height > 0.) {
        return None;
    }
    let scale = size as f32 / width.max(height);
    let (dx, dy) = (
        (size as f32 - width * scale) / 2.,
        (size as f32 - height * scale) / 2.,
    );
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_row(scale, 0., 0., scale, dx, dy),
        &mut pixmap.as_mut(),
    );
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for pixel in pixmap.pixels() {
        let color = pixel.demultiply();
        data.extend_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
    }
    RgbaImage::from_raw(size, size, data)
}

/// Decodes a raster icon and fits it into the square.
pub fn png(bytes: &[u8], size: u32) -> Option<RgbaImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let picture = reader.decode().ok()?.to_rgba8();
    Some(fit(&picture, size))
}

/// Scales `picture` to fit a `size` square, centered on transparency.
pub fn fit(picture: &RgbaImage, size: u32) -> RgbaImage {
    let (width, height) = picture.dimensions();
    if width == size && height == size {
        return picture.clone();
    }
    let scale = size as f32 / width.max(height).max(1) as f32;
    let new_width = ((width as f32 * scale).round() as u32).clamp(1, size);
    let new_height = ((height as f32 * scale).round() as u32).clamp(1, size);
    let scaled = resize(picture, new_width, new_height);
    let mut out = RgbaImage::new(size, size);
    image::imageops::overlay(
        &mut out,
        &scaled,
        i64::from((size - new_width) / 2),
        i64::from((size - new_height) / 2),
    );
    out
}

/// Resizes with premultiplied alpha so transparent edges don't darken.
pub fn resize(picture: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    if picture.dimensions() == (width, height) {
        return picture.clone();
    }
    let mut premultiplied = picture.clone();
    for pixel in premultiplied.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for channel in 0..3 {
            pixel[channel] = ((u16::from(pixel[channel]) * alpha + 127) / 255) as u8;
        }
    }
    let filter = if width < picture.width() {
        image::imageops::FilterType::Lanczos3
    } else {
        image::imageops::FilterType::CatmullRom
    };
    let mut out = image::imageops::resize(&premultiplied, width, height, filter);
    for pixel in out.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        if alpha == 0 {
            *pixel = image::Rgba([0, 0, 0, 0]);
            continue;
        }
        for channel in 0..3 {
            pixel[channel] = ((u16::from(pixel[channel]) * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
    out
}

/// Encodes a picture as PNG.
pub fn encode(picture: &RgbaImage) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    picture
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .ok()?;
    Some(out)
}
