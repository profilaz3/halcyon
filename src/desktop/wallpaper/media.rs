//! Wallpaper media: thumbnails, colour and fit previews, folder scans,
//! source checks and the runtime's current slideshow picture.
//!
//! Everything here reads only and never panics on a bad file. Decoding is
//! blocking; the Wallpaper page runs it off the UI thread. Only PNG and
//! JPEG are previewed, within the size, side and allocation bounds below;
//! the other types cosmic-bg shows are selectable with "No preview".
//! Colour previews follow cosmic-bg 91d525b `colored.rs` and fit previews
//! `scaler.rs`.
use super::{
    BgColor, BgSource, IMAGE_EXTENSIONS, LIMITS, Limits, ScalingMode, folder_has_image, is_image,
    linear_to_srgb, walk_with,
};
use crate::desktop::store::DesktopStore;
use std::io::Read;
use std::path::{Path, PathBuf};
pub(crate) mod worker;

/// Thumbnail bounds (aspect ratio kept).
pub const THUMB: (u32, u32) = (320, 180);
/// Large Settings scenes; the gallery keeps its small thumbnails.
pub const SCENE: (u32, u32) = (1280, 720);
pub const MAX_FILE_BYTES: u64 = 64 << 20;
pub const MAX_SIDE: u32 = 16384;
pub const MAX_ALLOC: u64 = 512 << 20;
/// What cosmic-bg shows (the file chooser filter).
pub const ACCEPTED: [&str; 7] = IMAGE_EXTENSIONS;
/// What this app previews and analyses.
pub const PREVIEWED: [&str; 3] = ["png", "jpg", "jpeg"];
/// Most pictures the COSMIC collection lists.
pub const MAX_SYSTEM: usize = 200;

pub const NO_PREVIEW: &str = "No preview";
pub const MISSING: &str = "Picture not found";
pub const TOO_LARGE: &str = "Too large to preview";

/// The bundled wallpapers' compiled-in thumbnails (`assets/previews`).
const BUNDLED: [(&str, &[u8]); 16] = [
    (
        "halcyon-night",
        include_bytes!("../../../assets/previews/halcyon-night-thumb.png"),
    ),
    (
        "halcyon-calm",
        include_bytes!("../../../assets/previews/halcyon-calm-thumb.png"),
    ),
    (
        "halcyon-evening",
        include_bytes!("../../../assets/previews/halcyon-evening-thumb.png"),
    ),
    (
        "halcyon-focus",
        include_bytes!("../../../assets/previews/halcyon-focus-thumb.png"),
    ),
    (
        "halcyon-workbench",
        include_bytes!("../../../assets/previews/halcyon-workbench-thumb.png"),
    ),
    (
        "halcyon-paper",
        include_bytes!("../../../assets/previews/halcyon-paper-thumb.png"),
    ),
    (
        "halcyon-rose",
        include_bytes!("../../../assets/previews/halcyon-rose-thumb.png"),
    ),
    (
        "halcyon-rain",
        include_bytes!("../../../assets/previews/halcyon-rain-thumb.png"),
    ),
    (
        "halcyon-snow",
        include_bytes!("../../../assets/previews/halcyon-snow-thumb.png"),
    ),
    (
        "halcyon-fog",
        include_bytes!("../../../assets/previews/halcyon-fog-thumb.png"),
    ),
    (
        "lavender",
        include_bytes!("../../../assets/previews/lavender-thumb.png"),
    ),
    (
        "amber",
        include_bytes!("../../../assets/previews/amber-thumb.png"),
    ),
    (
        "teal",
        include_bytes!("../../../assets/previews/teal-thumb.png"),
    ),
    (
        "ocean",
        include_bytes!("../../../assets/previews/ocean-thumb.png"),
    ),
    (
        "rose",
        include_bytes!("../../../assets/previews/rose-thumb.png"),
    ),
    (
        "graphite",
        include_bytes!("../../../assets/previews/graphite-thumb.png"),
    ),
];

/// 8-bit sRGB RGBA, row by row.
#[derive(Clone, PartialEq, Eq)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Pixels {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Pixels({}×{})", self.width, self.height)
    }
}

impl Pixels {
    fn filled(width: u32, height: u32, rgb: [u8; 3]) -> Self {
        Self {
            width,
            height,
            rgba: [rgb[0], rgb[1], rgb[2], 255].repeat((width * height) as usize),
        }
    }
    fn image(&self) -> Option<image::RgbaImage> {
        image::RgbaImage::from_raw(self.width, self.height, self.rgba.clone())
    }
    fn from_image(image: image::RgbaImage) -> Self {
        Self {
            width: image.width(),
            height: image.height(),
            rgba: image.into_raw(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Preview {
    Ready(Pixels),
    /// Why there is no picture: `NO_PREVIEW`, `MISSING` or `TOO_LARGE`.
    None(&'static str),
}

fn previewed(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            PREVIEWED
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

/// How much of a file is read to tell whether it is animated.
const MOTION_HEADER: u64 = 64 << 10;

/// Whether a picture is animated: an APNG (`acTL` before the image data) or
/// an animated WebP (`VP8X` animation flag or an `ANIM` chunk). cosmic-bg
/// 91d525b decodes one still frame and never redraws it, so these show
/// still. Reads at most `MOTION_HEADER` bytes; never panics.
pub fn animated(path: &Path) -> bool {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(MOTION_HEADER).read_to_end(&mut bytes))
        .is_ok()
        && animated_bytes(&bytes)
}

fn animated_bytes(bytes: &[u8]) -> bool {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if let Some(mut rest) = bytes.strip_prefix(PNG) {
        while rest.len() >= 8 {
            let length = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
            match &rest[4..8] {
                b"acTL" => return true,
                b"IDAT" | b"IEND" => return false,
                _ => {}
            }
            let Some(next) = length.checked_add(12).and_then(|skip| rest.get(skip..)) else {
                return false;
            };
            rest = next;
        }
        return false;
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        let mut rest = &bytes[12..];
        while rest.len() >= 8 {
            let length = u32::from_le_bytes([rest[4], rest[5], rest[6], rest[7]]) as usize;
            match &rest[..4] {
                b"VP8X" => {
                    if rest.get(8).is_some_and(|flags| flags & 0x02 != 0) {
                        return true;
                    }
                }
                b"ANIM" | b"ANMF" => return true,
                b"VP8 " | b"VP8L" => return false,
                _ => {}
            }
            let padded = length + (length & 1);
            let Some(next) = padded.checked_add(8).and_then(|skip| rest.get(skip..)) else {
                return false;
            };
            rest = next;
        }
    }
    false
}

/// A thumbnail within `THUMB` of a PNG or JPEG file. Blocking; never
/// panics. Larger files, sides or allocations are refused before decoding.
pub fn thumbnail(path: &Path) -> Preview {
    std::panic::catch_unwind(|| read_preview(path, THUMB)).unwrap_or(Preview::None(NO_PREVIEW))
}

/// Bounded scene image, decoded only by the existing background worker.
pub fn scene_preview(path: &Path) -> Preview {
    std::panic::catch_unwind(|| read_preview(path, SCENE)).unwrap_or(Preview::None(NO_PREVIEW))
}

fn read_preview(path: &Path, bounds: (u32, u32)) -> Preview {
    let Ok(metadata) = std::fs::metadata(path) else {
        return Preview::None(MISSING);
    };
    if !metadata.is_file() || !previewed(path) {
        return Preview::None(NO_PREVIEW);
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Preview::None(TOO_LARGE);
    }
    let mut bytes = Vec::new();
    let read = std::fs::File::open(path)
        .and_then(|file| file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes));
    match read {
        Ok(_) if bytes.len() as u64 > MAX_FILE_BYTES => Preview::None(TOO_LARGE),
        Ok(_) => decode_at(&bytes, bounds),
        Err(_) => Preview::None(NO_PREVIEW),
    }
}

/// Decodes PNG or JPEG bytes (by content) within the bounds and reduces
/// them to `THUMB`.
fn decode(bytes: &[u8]) -> Preview {
    decode_at(bytes, THUMB)
}

fn decode_at(bytes: &[u8], bounds: (u32, u32)) -> Preview {
    let Ok(mut reader) = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()
    else {
        return Preview::None(NO_PREVIEW);
    };
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
    ) {
        return Preview::None(NO_PREVIEW);
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    match reader.decode() {
        // Reduced to the bounds, never enlarged.
        Ok(image) if image.width() > bounds.0 || image.height() > bounds.1 => Preview::Ready(
            Pixels::from_image(image.thumbnail(bounds.0, bounds.1).to_rgba8()),
        ),
        Ok(image) => Preview::Ready(Pixels::from_image(image.to_rgba8())),
        Err(_) => Preview::None(NO_PREVIEW),
    }
}

/// The compiled-in thumbnail of bundled wallpaper `name`; `None` for a
/// name that is not built in.
pub fn bundled_thumbnail(name: &str) -> Option<Pixels> {
    if name == "halcyon-ridge" {
        return match decode(crate::depth::PICTURE) {
            Preview::Ready(p) => Some(p),
            _ => None,
        };
    }
    let (_, bytes) = BUNDLED.iter().find(|(known, _)| *known == name)?;
    match decode(bytes) {
        Preview::Ready(pixels) => Some(pixels),
        Preview::None(_) => None,
    }
}

/// Full bundled scenes are immutable public assets, decoded only in the child.
pub(crate) fn bundled_scene(name: &str) -> Option<Pixels> {
    if name == "halcyon-ridge" {
        return match decode_at(crate::depth::PICTURE, SCENE) {
            Preview::Ready(p) => Some(p),
            _ => None,
        };
    }
    macro_rules! scenes {
        ($($id:literal),*) => { [$(($id, &include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/wallpapers/", $id, ".jpg"))[..])),*] };
    }
    let scenes = scenes!(
        "halcyon-night",
        "halcyon-calm",
        "halcyon-evening",
        "halcyon-focus",
        "halcyon-workbench",
        "halcyon-paper",
        "halcyon-rose",
        "halcyon-rain",
        "halcyon-snow",
        "halcyon-fog",
        "lavender",
        "amber",
        "teal",
        "ocean",
        "rose",
        "graphite"
    );
    let (_, bytes) = scenes.iter().find(|(id, _)| *id == name)?;
    match decode_at(bytes, SCENE) {
        Preview::Ready(pixels) => Some(pixels),
        _ => None,
    }
}

fn byte(value: f32) -> u8 {
    (value.clamp(0., 1.) * 255.).round() as u8
}

/// A colour background at `width`×`height`, as cosmic-bg renders it: a
/// solid colour is sRGB as stored; gradient stops are linear RGB, blended
/// in linear light at the position `colored.rs` gives each pixel and
/// encoded to sRGB.
pub fn render_color(color: &BgColor, width: u32, height: u32) -> Pixels {
    let (width, height) = (width.max(1), height.max(1));
    let gradient = match color {
        BgColor::Single(rgb) => return Pixels::filled(width, height, rgb.map(byte)),
        BgColor::Gradient(gradient) => gradient,
    };
    let stops = &gradient.colors;
    if stops.is_empty() {
        return Pixels::filled(width, height, [0, 0, 0]);
    }
    // colorgrad's linear gradient: stops evenly over the domain [0, 1],
    // positions outside it clamped.
    let at = |position: f32| -> [u8; 3] {
        let last = stops.len() - 1;
        let position = if position.is_nan() {
            0.
        } else {
            position.clamp(0., 1.)
        };
        let scaled = position * last as f32;
        let index = (scaled.floor() as usize).min(last.saturating_sub(1));
        let (a, b) = (stops[index], stops[(index + 1).min(last)]);
        let t = if last == 0 { 0. } else { scaled - index as f32 };
        [0, 1, 2].map(|channel| byte(linear_to_srgb(a[channel] + (b[channel] - a[channel]) * t)))
    };
    let (w, h) = (width as f32, height as f32);
    let orientation = gradient.radius as u16;
    let angle = gradient.radius.to_radians();
    let (cos, sin) = (angle.cos(), angle.sin());
    const SCALE: f32 = 0.015;
    let (w_scale, h_scale) = (w / SCALE, h / SCALE);
    let remap = |t: f32, a: f32, b: f32, c: f32, d: f32| (t - a) * ((d - c) / (b - a)) + c;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let (x, y) = (x as f32, y as f32);
            let position = match orientation {
                0 => 1.0 - (y / h),
                90 => x / w,
                180 => y / h,
                270 => 1.0 - (x / w),
                _ => remap(
                    (x - w_scale) * cos - (y - h_scale) * sin,
                    -w_scale,
                    w_scale,
                    0.,
                    1.,
                ),
            };
            let [r, g, b] = at(position);
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    Pixels {
        width,
        height,
        rgba,
    }
}

/// `source` shown on a `width`×`height` display the way cosmic-bg scales
/// it: Zoom (Fill) covers and crops the centre, Fit letterboxes with its
/// sRGB colour, Stretch ignores the aspect ratio.
pub fn fit_preview(source: &Pixels, mode: &ScalingMode, width: u32, height: u32) -> Pixels {
    use image::imageops::{FilterType, crop_imm, overlay, resize};
    let (width, height) = (width.max(1), height.max(1));
    let Some(image) = source
        .image()
        .filter(|image| image.width() > 0 && image.height() > 0)
    else {
        return Pixels::filled(width, height, [0, 0, 0]);
    };
    let (w, h) = (f64::from(image.width()), f64::from(image.height()));
    let scaled = |ratio: f64| {
        (
            ((w * ratio).round() as u32).max(1),
            ((h * ratio).round() as u32).max(1),
        )
    };
    match mode {
        ScalingMode::Stretch => {
            Pixels::from_image(resize(&image, width, height, FilterType::Triangle))
        }
        ScalingMode::Zoom => {
            // Crop in source coordinates first. Resizing the full source
            // to cover the target can expand a tiny 1×720 image into a
            // multi-gigabyte intermediate. Both the crop and resize now
            // stay within the source and destination bounds respectively.
            let (cw, ch) = if w * f64::from(height) > h * f64::from(width) {
                (
                    ((h * f64::from(width) / f64::from(height)).round() as u32)
                        .clamp(1, image.width()),
                    image.height(),
                )
            } else {
                (
                    image.width(),
                    ((w * f64::from(height) / f64::from(width)).round() as u32)
                        .clamp(1, image.height()),
                )
            };
            let cropped = crop_imm(
                &image,
                (image.width() - cw) / 2,
                (image.height() - ch) / 2,
                cw,
                ch,
            );
            Pixels::from_image(resize(
                &cropped.to_image(),
                width,
                height,
                FilterType::Triangle,
            ))
        }
        ScalingMode::Fit(color) => {
            let ratio = (f64::from(width) / w).min(f64::from(height) / h);
            let (nw, nh) = scaled(ratio);
            let (nw, nh) = (nw.min(width), nh.min(height));
            let [r, g, b] = color.map(byte);
            let mut canvas =
                image::RgbaImage::from_pixel(width, height, image::Rgba([r, g, b, 255]));
            let resized = resize(&image, nw, nh, FilterType::Triangle);
            overlay(
                &mut canvas,
                &resized,
                i64::from((width - nw) / 2),
                i64::from((height - nh) / 2),
            );
            Pixels::from_image(canvas)
        }
    }
}

/// A bounded look at a slideshow folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FolderScan {
    /// Pictures cosmic-bg would show (by extension).
    pub images: usize,
    /// Other files: the slideshow skips a turn on each.
    pub others: usize,
    /// A walk bound was reached, so the counts are partial ("N+").
    pub truncated: bool,
    /// The first pictures in the runtime's (path) order, at most four.
    pub first: Vec<PathBuf>,
}

/// Counts a folder's files with the same bounded walk the source checks
/// use (`walk_folder`). Reads only.
pub fn scan_folder(path: &Path) -> Result<FolderScan, String> {
    scan_folder_with(path, LIMITS)
}
fn scan_folder_with(path: &Path, limits: Limits) -> Result<FolderScan, String> {
    let mut scan = FolderScan::default();
    let (walk, _) = walk_with(path, limits, |file| {
        if is_image(file) {
            scan.images += 1;
            if scan.first.len() < 4 {
                scan.first.push(file.to_path_buf());
            }
        } else {
            scan.others += 1;
        }
        true
    })?;
    scan.truncated = walk.truncated;
    Ok(scan)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceCheck {
    Ready,
    /// "This picture was moved or deleted".
    Missing,
    /// A folder whose complete scan found no pictures.
    Empty,
    /// A path that is not text can't be stored.
    NotText,
    Unreadable(String),
}

impl SourceCheck {
    /// The reason Apply is blocked, if it is.
    pub fn problem(&self) -> Option<String> {
        match self {
            Self::Ready => None,
            Self::Missing => Some("This picture was moved or deleted".into()),
            Self::Empty => Some("No pictures in this folder".into()),
            Self::NotText => Some("This picture’s location can’t be used".into()),
            Self::Unreadable(reason) => Some(reason.clone()),
        }
    }
}

/// Whether a source can be shown (read only; a file can still disappear
/// later). Folders use the bounded walk: one that is too large to check is
/// `Unreadable`, never "no pictures".
pub fn check_source(source: &BgSource) -> SourceCheck {
    let BgSource::Path(path) = source else {
        return SourceCheck::Ready;
    };
    if path.to_str().is_none() {
        return SourceCheck::NotText;
    }
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => SourceCheck::Ready,
        Ok(metadata) if metadata.is_dir() => match folder_has_image(path) {
            Ok(true) => SourceCheck::Ready,
            Ok(false) => SourceCheck::Empty,
            Err(reason) => SourceCheck::Unreadable(
                reason
                    .split_once(':')
                    .map_or(reason.clone(), |(first, _)| first.to_string()),
            ),
        },
        Ok(_) => SourceCheck::Unreadable("Not a picture or folder".into()),
        Err(_) => SourceCheck::Missing,
    }
}

/// cosmic-bg's state file: `<fixture>/cosmic-state/…` or the user's state
/// directory. A fixture never falls back to the live file.
pub fn state_path(store: &DesktopStore) -> Option<PathBuf> {
    const RELATIVE: &str = "com.system76.CosmicBackground/v1/wallpapers";
    match store.fixture_root() {
        Some(root) => Some(root.join("cosmic-state").join(RELATIVE)),
        None => dirs::state_dir().map(|dir| dir.join("cosmic").join(RELATIVE)),
    }
}

/// The picture cosmic-bg reports showing on `output` now (state file
/// `Vec<(String, Source)>`), read only and bounded.
pub fn current_slideshow_image(store: &DesktopStore, output: &str) -> Option<PathBuf> {
    let path = state_path(store)?;
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(256 * 1024)
        .read_to_string(&mut text)
        .ok()?;
    let state: Vec<(String, BgSource)> = ron::from_str(&text).ok()?;
    state.into_iter().find_map(|(name, source)| match source {
        BgSource::Path(path) if name == output => Some(path),
        _ => None,
    })
}

/// Pictures directly in, or one folder below, each of `dirs` (trusted
/// system backgrounds), in path order, at most `MAX_SYSTEM`.
pub fn system_wallpapers(dirs: &[PathBuf]) -> Vec<PathBuf> {
    const LISTING: usize = 1000;
    let listing = |dir: &Path| -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .take(LISTING)
                    .flatten()
                    .map(|entry| entry.path())
                    .collect()
            })
            .unwrap_or_default();
        paths.sort();
        paths
    };
    let mut found = Vec::new();
    for dir in dirs {
        for path in listing(dir) {
            if found.len() >= MAX_SYSTEM {
                return found;
            }
            match std::fs::metadata(&path) {
                Ok(metadata) if metadata.is_file() && is_image(&path) => found.push(path),
                Ok(metadata) if metadata.is_dir() => {
                    for inner in listing(&path) {
                        if found.len() >= MAX_SYSTEM {
                            return found;
                        }
                        if inner.is_file() && is_image(&inner) {
                            found.push(inner);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    found
}

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
