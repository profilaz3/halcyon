//! Halcyon's overlay icon theme. It sits over the chosen base theme and
//! holds only what Halcyon changes: app icons masked to one shape, and
//! folder icons recolored to the accent. Everything else falls through to
//! the base through `Inherits`.
//!
//! Two slot names alternate, so every regeneration is a new `icon_theme`
//! value and the desktop reloads its icons. The new slot is built in a
//! hidden staging folder and moved into place whole.
use super::{Paths, lookup::Resolver, raster};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const SLOTS: [&str; 2] = ["Halcyon-Shaped", "Halcyon-Shaped-Alt"];
/// The pixel sizes written, each a Fixed directory.
pub const SIZES: [u32; 3] = [64, 128, 256];
/// Icons are composed at the largest size, then scaled down.
const WORK: u32 = 256;
const MARKER: &str = "X-Halcyon-Overlay";

pub fn is_overlay(id: &str) -> bool {
    SLOTS.contains(&id)
}

/// The slot to build next: the one not in use.
pub fn next_slot(current: Option<&str>) -> &'static str {
    if current == Some(SLOTS[0]) {
        SLOTS[1]
    } else {
        SLOTS[0]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    #[default]
    Original,
    Circle,
    Squircle,
    Rounded,
}

impl Shape {
    pub const ALL: [Self; 4] = [Self::Original, Self::Circle, Self::Squircle, Self::Rounded];
    pub fn title(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::Circle => "Circle",
            Self::Squircle => "Squircle",
            Self::Rounded => "Rounded square",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Circle => "circle",
            Self::Squircle => "squircle",
            Self::Rounded => "rounded",
        }
    }
    /// Whether the normalized point (-1..1 on both axes) is inside.
    fn inside(self, x: f32, y: f32) -> bool {
        let (x, y) = (x.abs(), y.abs());
        match self {
            Self::Original => x <= 1. && y <= 1.,
            Self::Circle => x * x + y * y <= 1.,
            // A superellipse: softer than a rounded square, fuller than a
            // circle.
            Self::Squircle => x.powi(4) + y.powi(4) <= 1.,
            Self::Rounded => {
                const RADIUS: f32 = 0.34;
                if x > 1. || y > 1. {
                    return false;
                }
                let (dx, dy) = ((x - (1. - RADIUS)).max(0.), (y - (1. - RADIUS)).max(0.));
                dx * dx + dy * dy <= RADIUS * RADIUS
            }
        }
    }
}

/// What Halcyon remembers about icons, in its own config directory: the
/// theme chosen as the base, and the overlay's shape and tint.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Style {
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub shape: Shape,
    #[serde(default)]
    pub tint: bool,
}

impl Style {
    pub fn load(paths: &Paths) -> Self {
        super::read(&paths.style(), 64 * 1024)
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .map(|mut style| {
                style.base = style
                    .base
                    .filter(|base| super::valid_id(base) && !is_overlay(base));
                style
            })
            .unwrap_or_default()
    }
    pub fn save(&self, paths: &Paths) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        super::write_atomic(&paths.style(), &json).map_err(|error| error.to_string())
    }
    /// Whether an overlay is needed at all.
    pub fn active(&self) -> bool {
        self.shape != Shape::Original || self.tint
    }
    /// The style as the desktop has it: with `icon_theme` not an overlay
    /// (changed elsewhere, or never shaped), that theme is the base and
    /// nothing is shaped.
    pub fn effective(mut self, icon_theme: &str) -> Self {
        if !is_overlay(icon_theme) {
            return Self {
                base: Some(icon_theme.to_string()),
                ..Self::default()
            };
        }
        if self.base.is_none() {
            self.base = Some("Cosmic".into());
        }
        self
    }
}

/// The style the desktop shows with `icon_theme`. An overlay's own record
/// comes first, so a slot that Undo brought back reads as what it was
/// built from; then `saved`, Halcyon's record; then Cosmic as the base.
pub fn desktop_style(paths: Option<&Paths>, icon_theme: &str, saved: &Style) -> Style {
    if is_overlay(icon_theme)
        && let Some(record) = paths.and_then(|paths| recorded(paths, icon_theme))
    {
        return Style {
            base: Some(record.base),
            shape: record.shape,
            tint: record.tint.is_some(),
        };
    }
    saved.clone().effective(icon_theme)
}

/// The theme a look names for `id`: the theme itself, or for one of
/// Halcyon's overlays (older looks saved its name) the base it was built
/// on, when that is still known here.
pub fn base_theme(paths: Option<&Paths>, id: &str) -> Option<String> {
    if !is_overlay(id) {
        return Some(id.to_string());
    }
    let paths = paths?;
    recorded(paths, id)
        .map(|record| record.base)
        .or_else(|| Style::load(paths).base)
}

/// An overlay a look's Apply builds before the desktop names it. The
/// request's app list is filled in when it runs, off the UI thread.
#[derive(Clone, Debug, PartialEq)]
pub struct LookBuild {
    pub paths: Paths,
    pub request: Request,
    /// The desktop's `icon_theme` when it was planned.
    pub current: Option<String>,
    /// The slot the plan names: the one not in use.
    pub slot: String,
}

impl LookBuild {
    /// Whether the slot already holds exactly this overlay.
    pub fn ready(&self) -> bool {
        let request = &self.request;
        recorded(&self.paths, &self.slot).is_some_and(|record| {
            record.base == request.base
                && record.shape == request.shape
                && record.tint == request.tint
                && record.dark == request.dark
        })
    }
    /// Builds the overlay unless it is there already. Blocking: icons are
    /// looked up and drawn here.
    pub fn run(
        &self,
        fixture: Option<&Path>,
        store: Option<&crate::desktop::store::DesktopStore>,
    ) -> Result<String, String> {
        if self.ready() {
            return Ok(self.slot.clone());
        }
        let mut request = self.request.clone();
        request.apps = super::app_names(fixture, store).0;
        let slot = generate(&self.paths, &request, self.current.as_deref(), &|_, _| {})?;
        if slot != self.slot {
            return Err("The icons changed while they were being adjusted".into());
        }
        Ok(slot)
    }
}

/// Folder icons recolored when the tint is on, in the `places` context.
pub const FOLDERS: [&str; 24] = [
    "folder",
    "folder-open",
    "folder-documents",
    "folder-download",
    "folder-downloads",
    "folder-music",
    "folder-pictures",
    "folder-images",
    "folder-videos",
    "folder-video",
    "folder-templates",
    "folder-publicshare",
    "folder-public",
    "folder-remote",
    "folder-network",
    "folder-desktop",
    "folder-documents-open",
    "folder-drag-accept",
    "folder-saved-search",
    "folder-recent",
    "folder-git",
    "user-home",
    "user-desktop",
    "user-bookmarks",
];

/// One overlay to build.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub base: String,
    pub shape: Shape,
    /// The accent the folders take, or `None` to leave folders alone.
    pub tint: Option<[u8; 3]>,
    /// Backdrop tiles follow the desktop's shade.
    pub dark: bool,
    /// App icon names, dock pins first.
    pub apps: Vec<String>,
}

/// The lookup chain the overlay inherits: the base and its parents, then
/// Cosmic and its parents, then hicolor.
pub fn chain(resolver: &mut Resolver, base: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in [base, "Cosmic"] {
        for theme in resolver.chain(id) {
            if theme != "hicolor" && !is_overlay(&theme) && !out.contains(&theme) {
                out.push(theme);
            }
        }
    }
    out.push("hicolor".into());
    out
}

fn directories(apps: bool, places: bool) -> Vec<(String, u32, &'static str)> {
    let mut dirs = Vec::new();
    for size in SIZES {
        if apps {
            dirs.push((format!("{size}x{size}/apps"), size, "Applications"));
        }
        if places {
            dirs.push((format!("{size}x{size}/places"), size, "Places"));
        }
    }
    dirs
}

/// The overlay's `index.theme`.
pub fn index_theme(base: &str, shape: Shape, tint: Option<[u8; 3]>) -> String {
    let mut inherits = vec![base];
    for parent in ["Cosmic", "hicolor"] {
        if !inherits.contains(&parent) {
            inherits.push(parent);
        }
    }
    let dirs = directories(shape != Shape::Original, tint.is_some());
    let mut text = format!(
        "[Icon Theme]\nName=Halcyon shaped icons\nComment=Icons from {base}, adjusted by Halcyon\nInherits={}\nHidden=true\nDirectories={}\n{MARKER}=true\nX-Halcyon-Base={base}\nX-Halcyon-Shape={}\n",
        inherits.join(","),
        dirs.iter()
            .map(|(dir, ..)| dir.as_str())
            .collect::<Vec<_>>()
            .join(","),
        shape.key(),
    );
    if let Some([r, g, b]) = tint {
        text.push_str(&format!("X-Halcyon-Tint=#{r:02x}{g:02x}{b:02x}\n"));
    }
    for (dir, size, context) in dirs {
        text.push_str(&format!(
            "\n[{dir}]\nSize={size}\nScale=1\nContext={context}\nType=Fixed\n"
        ));
    }
    text
}

/// The overlay's `index.theme` for `request`: [`index_theme`] with the
/// shade its tiles were drawn for, so a change of shade is noticed.
pub fn index_for(request: &Request) -> String {
    let text = index_theme(&request.base, request.shape, request.tint);
    let shade = format!(
        "X-Halcyon-Shade={}\n",
        if request.dark { "dark" } else { "light" }
    );
    match text.find("\n\n[") {
        Some(end) => format!("{}\n{shade}{}", &text[..end], &text[end + 1..]),
        None => text + &shade,
    }
}

/// What an overlay slot was built from, as its `index.theme` records it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recorded {
    pub base: String,
    pub shape: Shape,
    pub tint: Option<[u8; 3]>,
    pub dark: bool,
}

/// Reads the record of the overlay in `slot`, if Halcyon wrote it.
pub fn recorded(paths: &Paths, slot: &str) -> Option<Recorded> {
    if !is_overlay(slot) {
        return None;
    }
    let bytes = super::read(&paths.icons.join(slot).join("index.theme"), 64 * 1024)?;
    let text = String::from_utf8_lossy(&bytes);
    if !text.contains(&format!("{MARKER}=true")) {
        return None;
    }
    let value = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
            .map(str::trim)
    };
    let base = value("X-Halcyon-Base")
        .filter(|base| super::valid_id(base) && !is_overlay(base))?
        .to_string();
    let shape = Shape::ALL
        .into_iter()
        .find(|shape| Some(shape.key()) == value("X-Halcyon-Shape"))
        .unwrap_or_default();
    let tint = value("X-Halcyon-Tint").and_then(|hex| {
        let hex = hex.strip_prefix('#')?;
        let number = u32::from_str_radix(hex, 16)
            .ok()
            .filter(|_| hex.len() == 6)?;
        Some([(number >> 16) as u8, (number >> 8) as u8, number as u8])
    });
    Some(Recorded {
        base,
        shape,
        tint,
        dark: value("X-Halcyon-Shade") == Some("dark"),
    })
}

/// Whether `dir` is an overlay Halcyon wrote.
pub fn ours(dir: &Path) -> bool {
    super::read(&dir.join("index.theme"), 64 * 1024)
        .is_some_and(|bytes| String::from_utf8_lossy(&bytes).contains(&format!("{MARKER}=true")))
}

/// Removes Halcyon's overlay slots, except `keep`. A folder of that name
/// Halcyon did not write is left alone.
pub fn remove_all(paths: &Paths, keep: Option<&str>) {
    for slot in SLOTS {
        if Some(slot) == keep {
            continue;
        }
        let dir = paths.icons.join(slot);
        if fs::symlink_metadata(&dir).is_ok_and(|meta| meta.is_dir()) && ours(&dir) {
            let _ = fs::remove_dir_all(&dir);
        }
    }
}

// Pixel helpers. Colors are 0..1 floats in sRGB.

fn hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.;
    let d = max - min;
    if d < 1e-6 {
        return (0., 0., l);
    }
    let s = d / (1. - (2. * l - 1.).abs()).max(1e-6);
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.)
    } else if max == g {
        (b - r) / d + 2.
    } else {
        (r - g) / d + 4.
    };
    (h * 60., s.min(1.), l)
}

fn rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let c = (1. - (2. * l - 1.).abs()) * s;
    let h = h.rem_euclid(360.) / 60.;
    let x = c * (1. - (h.rem_euclid(2.) - 1.).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    let m = l - c / 2.;
    (r + m, g + m, b + m)
}

fn unit(pixel: &Rgba<u8>) -> (f32, f32, f32) {
    (
        f32::from(pixel[0]) / 255.,
        f32::from(pixel[1]) / 255.,
        f32::from(pixel[2]) / 255.,
    )
}

fn byte(value: f32) -> u8 {
    (value.clamp(0., 1.) * 255.).round() as u8
}

fn smoothstep(low: f32, high: f32, value: f32) -> f32 {
    let t = ((value - low) / (high - low)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// Public HSL for previews and tests.
pub fn hue_saturation(color: [u8; 3]) -> (f32, f32, f32) {
    hsl(
        f32::from(color[0]) / 255.,
        f32::from(color[1]) / 255.,
        f32::from(color[2]) / 255.,
    )
}

/// Recolors the colored parts of `icon` toward `accent`. Hues move by the
/// same turn that takes the icon's main hue to the accent's, saturation
/// scales by the accent's, lightness (and with it the shading) stays, and
/// gray pixels stay gray. A folder drawn all in gray (COSMIC's own) has no
/// hue to turn, so its mid tones take the accent instead.
pub fn tint(icon: &mut RgbaImage, accent: [u8; 3]) {
    let (accent_h, accent_s, _) = hue_saturation(accent);
    // The icon's main hue: a saturation weighted circular mean.
    let (mut sx, mut sy, mut ss, mut weight) = (0f32, 0f32, 0f32, 0f32);
    let mut opaque = 0f32;
    for pixel in icon.pixels() {
        if pixel[3] < 32 {
            continue;
        }
        opaque += f32::from(pixel[3]) / 255.;
        let (r, g, b) = unit(pixel);
        let (h, s, l) = hsl(r, g, b);
        let w =
            smoothstep(0.08, 0.18, s) * s * (1. - (2. * l - 1.).abs()) * f32::from(pixel[3]) / 255.;
        sx += h.to_radians().cos() * w;
        sy += h.to_radians().sin() * w;
        ss += s * w;
        weight += w;
    }
    if weight < 0.02 * opaque {
        colorize(icon, accent_h, accent_s);
        return;
    }
    let reference_h = sy.atan2(sx).to_degrees();
    let reference_s = (ss / weight).max(0.05);
    let ratio = accent_s / reference_s;
    for pixel in icon.pixels_mut() {
        if pixel[3] == 0 {
            continue;
        }
        let (r, g, b) = unit(pixel);
        let (h, s, l) = hsl(r, g, b);
        let blend = smoothstep(0.08, 0.18, s);
        if blend <= 0. {
            continue;
        }
        let (nr, ng, nb) = rgb(accent_h + (h - reference_h), (s * ratio).clamp(0., 1.), l);
        pixel[0] = byte(r + (nr - r) * blend);
        pixel[1] = byte(g + (ng - g) * blend);
        pixel[2] = byte(b + (nb - b) * blend);
    }
}

/// Gives a gray icon the accent's hue: mid tones take most of the color,
/// while near white paper and near black marks stay neutral.
fn colorize(icon: &mut RgbaImage, hue: f32, saturation: f32) {
    for pixel in icon.pixels_mut() {
        if pixel[3] == 0 {
            continue;
        }
        let (r, g, b) = unit(pixel);
        let (_, s, l) = hsl(r, g, b);
        let tone = smoothstep(0.1, 0.3, l) * (1. - smoothstep(0.8, 0.94, l));
        if tone <= 0. {
            continue;
        }
        let (nr, ng, nb) = rgb(hue, (saturation * 0.85).max(s), l);
        pixel[0] = byte(r + (nr - r) * tone);
        pixel[1] = byte(g + (ng - g) * tone);
        pixel[2] = byte(b + (nb - b) * tone);
    }
}

/// Shape coverage for a `size` square, anti-aliased with 4x4 samples per
/// pixel, inside a margin of one sixteenth.
pub fn coverage(shape: Shape, size: u32) -> Vec<f32> {
    const SAMPLES: u32 = 4;
    let margin = size as f32 / 16.;
    let half = (size as f32 - 2. * margin) / 2.;
    let center = size as f32 / 2.;
    let mut out = Vec::with_capacity((size * size) as usize);
    for y in 0..size {
        for x in 0..size {
            let mut hits = 0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / SAMPLES as f32;
                    if shape.inside((px - center) / half, (py - center) / half) {
                        hits += 1;
                    }
                }
            }
            out.push(hits as f32 / (SAMPLES * SAMPLES) as f32);
        }
    }
    out
}

/// The shape's coverage at `WORK` size, computed once.
pub fn mask(shape: Shape) -> &'static [f32] {
    use std::sync::OnceLock;
    static MASKS: [OnceLock<Vec<f32>>; 4] = [const { OnceLock::new() }; 4];
    let index = Shape::ALL
        .iter()
        .position(|item| *item == shape)
        .unwrap_or(0);
    MASKS[index].get_or_init(|| coverage(shape, WORK))
}

/// The box around the visible pixels.
fn bounds(icon: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, pixel) in icon.enumerate_pixels() {
        if pixel[3] > 24 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x0 <= x1 && y0 <= y1).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

/// The average color of the icon's outer edge, sampled inward from its
/// box along rays from the center.
fn edge_color(icon: &RgbaImage, (x, y, w, h): (u32, u32, u32, u32)) -> ([f32; 3], f32) {
    let (cx, cy) = (x as f32 + w as f32 / 2., y as f32 + h as f32 / 2.);
    let reach = (w.max(h) as f32) * 0.75;
    let mut sum = [0f32; 3];
    let mut count = 0f32;
    let mut seen = Vec::with_capacity(96);
    for step in 0..96 {
        let angle = step as f32 / 96. * std::f32::consts::TAU;
        let (dx, dy) = (angle.cos(), angle.sin());
        let mut t = reach;
        while t > 0. {
            let (px, py) = (cx + dx * t, cy + dy * t);
            if px >= 0. && py >= 0. && (px as u32) < icon.width() && (py as u32) < icon.height() {
                let pixel = icon.get_pixel(px as u32, py as u32);
                if pixel[3] > 230 {
                    // A little inside the edge, past any outline.
                    let (qx, qy) = (cx + dx * (t - 3.), cy + dy * (t - 3.));
                    let pixel = icon.get_pixel(qx.max(0.) as u32, qy.max(0.) as u32);
                    let (r, g, b) = unit(pixel);
                    sum[0] += r;
                    sum[1] += g;
                    sum[2] += b;
                    count += 1.;
                    seen.push([r, g, b]);
                    break;
                }
            }
            t -= 1.;
        }
    }
    if count == 0. {
        return ([0.5; 3], 1.);
    }
    let mean = sum.map(|channel| channel / count);
    // How far the rim strays from its mean: low for a one color rim.
    let spread = seen
        .iter()
        .map(|c| {
            ((c[0] - mean[0]).powi(2) + (c[1] - mean[1]).powi(2) + (c[2] - mean[2]).powi(2)).sqrt()
        })
        .sum::<f32>()
        / count;
    (mean, spread)
}

/// The icon's most common saturated color, or `None` when it is mostly
/// gray.
fn dominant(icon: &RgbaImage) -> Option<[f32; 3]> {
    let mut buckets = [(0f32, [0f32; 3]); 24];
    let mut total = 0f32;
    for pixel in icon.pixels() {
        if pixel[3] < 128 {
            continue;
        }
        let (r, g, b) = unit(pixel);
        let (h, s, l) = hsl(r, g, b);
        total += 1.;
        let weight = smoothstep(0.15, 0.4, s) * (1. - (2. * l - 1.).abs());
        if weight <= 0. {
            continue;
        }
        let bucket = &mut buckets[((h / 15.) as usize).min(23)];
        bucket.0 += weight;
        bucket.1[0] += r * weight;
        bucket.1[1] += g * weight;
        bucket.1[2] += b * weight;
    }
    let (weight, sum) = buckets.iter().copied().max_by(|a, b| a.0.total_cmp(&b.0))?;
    (total > 0. && weight / total > 0.12).then(|| sum.map(|channel| channel / weight))
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// The tile behind an icon that doesn't fill its shape: a pale tint of its
/// main color in light mode, graphite with a hint of it in dark mode (a
/// deeper tint turns warm colors brown), or a neutral gray.
fn backdrop(icon: &RgbaImage, dark: bool) -> ([f32; 3], [f32; 3]) {
    let white = [1.; 3];
    match (dominant(icon), dark) {
        (Some(color), false) => (mix(color, white, 0.88), mix(color, white, 0.78)),
        (Some(color), true) => (mix(color, [0.3; 3], 0.9), mix(color, [0.21; 3], 0.92)),
        (None, false) => ([0.98; 3], [0.92; 3]),
        (None, true) => ([0.33; 3], [0.25; 3]),
    }
}

/// Paints `src` (straight alpha) over `dst` at an offset, source over.
fn over(dst: &mut RgbaImage, src: &RgbaImage, left: i64, top: i64) {
    image::imageops::overlay(dst, src, left, top);
}

/// One app icon at `WORK` size masked to `shape`. Icons that already fill
/// a square are scaled to cover the shape, and a rounder silhouette with a
/// one color rim is set over that color, so a round icon becomes a full
/// shape; any other icon is set inside the shape on a tile, a round one
/// with a multicolored rim nearly filling it.
pub fn shape_icon(icon: &RgbaImage, shape: Shape, dark: bool, mask: &[f32]) -> RgbaImage {
    let size = icon.width();
    let Some(bbox) = bounds(icon) else {
        return icon.clone();
    };
    let (bx, by, bw, bh) = bbox;
    let mut solid = 0u32;
    for y in by..by + bh {
        for x in bx..bx + bw {
            if icon.get_pixel(x, y)[3] > 200 {
                solid += 1;
            }
        }
    }
    let fill = solid as f32 / (bw * bh) as f32;
    let aspect = bw as f32 / bh as f32;
    let center = icon.get_pixel(bx + bw / 2, by + bh / 2)[3] > 200;
    let margin = size / 16;
    let side = size - 2 * margin;
    let crop = image::imageops::crop_imm(icon, bx, by, bw, bh).to_image();
    let mut out = RgbaImage::new(size, size);
    let round = (0.86..=1.16).contains(&aspect) && fill >= 0.76 && center;
    let (edge, spread) = edge_color(icon, bbox);
    if round && (fill >= 0.94 || spread < 0.14) {
        let color = Rgba([byte(edge[0]), byte(edge[1]), byte(edge[2]), 255]);
        for pixel in out.pixels_mut() {
            *pixel = color;
        }
        // A square icon is scaled to cover the shape (corners fall to the
        // mask), a rounder one to fit inside it.
        let target = if fill >= 0.94 { side + side / 32 } else { side };
        let scale = target as f32 / bw.max(bh) as f32;
        let (w, h) = (
            ((bw as f32 * scale).round() as u32).max(1),
            ((bh as f32 * scale).round() as u32).max(1),
        );
        let scaled = raster::resize(&crop, w, h);
        over(
            &mut out,
            &scaled,
            (i64::from(size) - i64::from(w)) / 2,
            (i64::from(size) - i64::from(h)) / 2,
        );
    } else {
        let (top, bottom) = backdrop(icon, dark);
        for (_, y, pixel) in out.enumerate_pixels_mut() {
            let t = y as f32 / size as f32;
            let color = mix(top, bottom, t);
            *pixel = Rgba([byte(color[0]), byte(color[1]), byte(color[2]), 255]);
        }
        let share = match (round, shape) {
            (true, Shape::Circle) => 0.76,
            (true, _) => 0.82,
            (false, Shape::Circle) => 0.66,
            (false, _) => 0.72,
        };
        let target = (side as f32 * share) as u32;
        let scale = target as f32 / bw.max(bh) as f32;
        let (w, h) = (
            ((bw as f32 * scale).round() as u32).max(1),
            ((bh as f32 * scale).round() as u32).max(1),
        );
        let scaled = raster::resize(&crop, w, h);
        over(
            &mut out,
            &scaled,
            (i64::from(size) - i64::from(w)) / 2,
            (i64::from(size) - i64::from(h)) / 2,
        );
    }
    for (pixel, cover) in out.pixels_mut().zip(mask) {
        pixel[3] = (f32::from(pixel[3]) * cover).round() as u8;
        if pixel[3] == 0 {
            *pixel = Rgba([0, 0, 0, 0]);
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Context {
    Apps,
    Places,
}

struct Job {
    name: String,
    source: PathBuf,
    context: Context,
}

/// The icons the overlay will hold, resolved in the base chain.
fn jobs(paths: &Paths, request: &Request) -> Vec<Job> {
    let mut resolver = Resolver::new(paths.search.clone());
    let chain = chain(&mut resolver, &request.base);
    let mut jobs: Vec<Job> = Vec::new();
    if request.shape != Shape::Original {
        for name in &request.apps {
            if jobs.iter().any(|job| job.name == *name) {
                continue;
            }
            if let Some((_, source)) = resolver.find(&chain, name, WORK) {
                jobs.push(Job {
                    name: name.clone(),
                    source,
                    context: Context::Apps,
                });
            }
        }
    }
    if request.tint.is_some() {
        for name in FOLDERS {
            if let Some((_, source)) = resolver.find(&chain, name, WORK) {
                jobs.push(Job {
                    name: name.to_string(),
                    source,
                    context: Context::Places,
                });
            }
        }
    }
    jobs
}

/// The finished icon for one job at `WORK` size.
fn render(job: &Job, request: &Request, mask: &[f32]) -> Option<RgbaImage> {
    let mut icon = raster::load(&job.source, WORK)?;
    match job.context {
        Context::Apps => Some(shape_icon(&icon, request.shape, request.dark, mask)),
        Context::Places => {
            tint(&mut icon, request.tint?);
            Some(icon)
        }
    }
}

/// The preview of one app icon, as the overlay would draw it, at `size`.
pub fn preview(source: &Path, shape: Shape, dark: bool, size: u32) -> Option<RgbaImage> {
    let icon = raster::load(source, WORK)?;
    let icon = if shape == Shape::Original {
        icon
    } else {
        shape_icon(&icon, shape, dark, mask(shape))
    };
    Some(raster::resize(&icon, size, size))
}

/// The preview of one folder icon, tinted or not, at `size`.
pub fn preview_folder(source: &Path, accent: Option<[u8; 3]>, size: u32) -> Option<RgbaImage> {
    let mut icon = raster::load(source, WORK)?;
    if let Some(accent) = accent {
        tint(&mut icon, accent);
    }
    Some(raster::resize(&icon, size, size))
}

/// The nice value for background icon work: ten above the calling thread's.
fn background_nice() -> i32 {
    // getpriority can return -1 as a value; errno tells the cases apart, and
    // either way the result below is a valid, lower priority.
    let current = unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) };
    (current + 10).clamp(10, 19)
}

/// On Linux a thread has its own nice value; 0 means the calling thread.
fn set_thread_nice(nice: i32) {
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, nice);
    }
}

/// Builds the overlay for `request` into the slot not in use and returns
/// its id. `current` is the desktop's `icon_theme`. `progress` hears
/// (done, total) as icons finish, from worker threads.
pub fn generate(
    paths: &Paths,
    request: &Request,
    current: Option<&str>,
    progress: &(dyn Fn(usize, usize) + Sync),
) -> Result<String, String> {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    if !super::valid_id(&request.base) || is_overlay(&request.base) {
        return Err(format!("{} can’t be the base theme", request.base));
    }
    let slot = next_slot(current);
    let target = paths.icons.join(slot);
    if fs::symlink_metadata(&target).is_ok() && !ours(&target) {
        return Err(format!(
            "A theme named {slot} is already installed and isn’t Halcyon’s"
        ));
    }
    let jobs = jobs(paths, request);
    let total = jobs.len();
    progress(0, total);
    fs::create_dir_all(&paths.icons).map_err(|error| error.to_string())?;
    let staging = paths
        .icons
        .join(format!(".halcyon-overlay-{}", super::unique()));
    let result = (|| {
        let folders: Vec<String> =
            directories(request.shape != Shape::Original, request.tint.is_some())
                .into_iter()
                .map(|(dir, ..)| dir)
                .collect();
        for dir in &folders {
            fs::create_dir_all(staging.join(dir)).map_err(|error| error.to_string())?;
        }
        let mask = mask(request.shape);
        let done = AtomicUsize::new(0);
        let threads = std::thread::available_parallelism()
            .map(|count| (count.get() / 2).max(1))
            .unwrap_or(2);
        // Background work: ten steps nicer than the caller, as depth
        // analysis runs, so drawing icons never competes with the desktop.
        let nice = background_nice();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|index| format!("halcyon-icons-{index}"))
            .start_handler(move |_| set_thread_nice(nice))
            .build()
            .map_err(|error| error.to_string())?;
        pool.install(|| {
            jobs.par_iter().for_each(|job| {
                if let Some(icon) = render(job, request, mask) {
                    let context = match job.context {
                        Context::Apps => "apps",
                        Context::Places => "places",
                    };
                    for size in SIZES {
                        let scaled = raster::resize(&icon, size, size);
                        if let Some(png) = raster::encode(&scaled) {
                            let path = staging
                                .join(format!("{size}x{size}"))
                                .join(context)
                                .join(format!("{}.png", job.name));
                            let _ = fs::write(path, png);
                        }
                    }
                }
                progress(done.fetch_add(1, Ordering::Relaxed) + 1, total);
            });
        });
        fs::write(staging.join("index.theme"), index_for(request))
            .map_err(|error| error.to_string())?;
        // On a pool thread, so a gtk-update-icon-cache it starts is niced too.
        pool.install(|| super::cache::write(&staging, &folders))?;
        if fs::symlink_metadata(&target).is_ok() {
            fs::remove_dir_all(&target).map_err(|error| error.to_string())?;
        }
        fs::rename(&staging, &target).map_err(|error| error.to_string())?;
        super::cache::settle(&target)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result.map(|()| slot.to_string())
}
