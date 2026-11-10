//! Scene lighting for the prepared Ridge clock. After the matte has cut the
//! clock, its visible face is relit from the picture it stands in: the sky's
//! own colour at each spot, a soft backlit rim, the picture's haze, shade where
//! the ridge meets the digits and a faint top-to-bottom falloff. Only RGB
//! changes; alpha (and therefore the readability fallback) stays the matte's.
//!
//! Everything is a crop of the picture at half resolution, a few separable
//! box blurs and per-pixel arithmetic, so a 2048x1024 raster stays cheap.
use super::Frame;
use crate::desktop::wallpaper::ScalingMode;
use crate::desktop::wallpaper::media::Pixels;
use serde::{Deserialize, Serialize};

/// The user's choice. `Off` keeps the plain masked clock byte for byte.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lighting {
    Off,
    #[default]
    Soft,
    Rich,
}
impl Lighting {
    pub const ALL: [Self; 3] = [Self::Off, Self::Soft, Self::Rich];
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Soft => "Soft",
            Self::Rich => "Rich",
        }
    }
    pub fn recipe(self) -> Option<Recipe> {
        #[cfg(test)]
        if self != Self::Off
            && let Some(tuned) = TUNE.with(|t| t.get())
        {
            return Some(tuned);
        }
        match self {
            Self::Off => None,
            Self::Soft => Some(SOFT),
            Self::Rich => Some(RICH),
        }
    }
}

/// Every tunable in one place. Strengths are 0 to 1 mixes; radii are logical
/// (100%) screen pixels, so outputs at 150% and 200% look the same.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Recipe {
    /// How far the digits take on the sky's colour around them.
    pub ambient: f32,
    /// Size of the neighbourhood that colour is taken from.
    pub ambient_radius: f32,
    /// How much the digits settle to the scene's brightness (0 keeps paper white).
    pub exposure: f32,
    /// Strength of the sky-coloured rim on glyph edges.
    pub wrap: f32,
    /// Width of that rim.
    pub wrap_radius: f32,
    /// How much the rim prefers edges that face the brighter sky (0 = all edges alike).
    pub wrap_facing: f32,
    /// Haze toward the local sky colour at the top of the face,
    pub haze_far: f32,
    /// and right above the ridge. Kept at or below 0.15 for readability.
    pub haze_near: f32,
    /// Picture height (fraction) over which haze grows toward the ridge.
    pub haze_span: f32,
    /// Darkening of the digits where the ridge cuts them.
    pub contact: f32,
    /// How far above the ridge that shade reaches.
    pub contact_radius: f32,
    /// Top-to-bottom falloff across the face (plus or minus this fraction).
    pub sheen: f32,
    /// Readability floor: lit glyph luminance stays at least
    /// `floor_ratio * background + floor_offset` (linear light).
    pub floor_ratio: f32,
    pub floor_offset: f32,
}

pub const SOFT: Recipe = Recipe {
    ambient: 0.42,
    ambient_radius: 70.,
    exposure: 0.15,
    wrap: 0.50,
    wrap_radius: 3.5,
    wrap_facing: 0.6,
    haze_far: 0.03,
    haze_near: 0.09,
    haze_span: 0.30,
    contact: 0.24,
    contact_radius: 26.,
    sheen: 0.025,
    floor_ratio: 1.5,
    floor_offset: 0.04,
};
pub const RICH: Recipe = Recipe {
    ambient: 0.62,
    ambient_radius: 60.,
    exposure: 0.30,
    wrap: 0.80,
    wrap_radius: 5.,
    wrap_facing: 0.75,
    haze_far: 0.05,
    haze_near: 0.14,
    haze_span: 0.30,
    contact: 0.38,
    contact_radius: 34.,
    sheen: 0.04,
    floor_ratio: 1.5,
    floor_offset: 0.04,
};

#[cfg(test)]
thread_local! {
    /// Evidence runs try recipes without rebuilding.
    pub(super) static TUNE: std::cell::Cell<Option<Recipe>> = const { std::cell::Cell::new(None) };
}
#[cfg(test)]
impl Recipe {
    /// `name=value` pairs separated by commas, over this recipe.
    pub(super) fn tuned(mut self, spec: &str) -> Self {
        for pair in spec.split(',').filter(|p| !p.trim().is_empty()) {
            let (key, value) = pair.split_once('=').expect("name=value");
            let value: f32 = value.trim().parse().expect("number");
            let slot = match key.trim() {
                "ambient" => &mut self.ambient,
                "ambient_radius" => &mut self.ambient_radius,
                "exposure" => &mut self.exposure,
                "wrap" => &mut self.wrap,
                "wrap_radius" => &mut self.wrap_radius,
                "wrap_facing" => &mut self.wrap_facing,
                "haze_far" => &mut self.haze_far,
                "haze_near" => &mut self.haze_near,
                "haze_span" => &mut self.haze_span,
                "contact" => &mut self.contact,
                "contact_radius" => &mut self.contact_radius,
                "sheen" => &mut self.sheen,
                "floor_ratio" => &mut self.floor_ratio,
                "floor_offset" => &mut self.floor_offset,
                other => panic!("unknown recipe member {other}"),
            };
            *slot = value;
        }
        self
    }
}

const SRGB_STEPS: usize = 4096;
/// Working resolution of the picture crop: one cell per 4x4 source pixels.
const CELL: usize = 4;

pub(super) fn to_linear(v: u8) -> f32 {
    let v = f32::from(v) / 255.;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn to_srgb(v: f32) -> u8 {
    let v = v.clamp(0., 1.);
    let v = if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    };
    (v * 255. + 0.5) as u8
}
pub(super) fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}
fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
/// Colour at luminance 1, so tints shift hue without changing brightness.
fn chroma(c: [f32; 3]) -> [f32; 3] {
    let l = luma(c).max(1e-4);
    c.map(|v| (v / l).min(4.))
}
/// Rescale to luminance `target`; channels that overflow are clipped.
fn with_luma(c: [f32; 3], target: f32) -> [f32; 3] {
    let l = luma(c).max(1e-5);
    c.map(|v| (v * target / l).clamp(0., 1.))
}

/// Pull `lit` back toward the unlit `base` until it keeps its contrast with
/// a background of luminance `bg`: light ink stays at least
/// `floor_ratio * bg + floor_offset` (or as bright as it was), dark ink the
/// mirror of that. Lighting can soften the clock but never sink it.
pub(super) fn readable(lit: [f32; 3], base: [f32; 3], bg: f32, recipe: &Recipe) -> [f32; 3] {
    let (l, base_l) = (luma(lit), luma(base));
    let t = if base_l >= bg {
        let floor = (recipe.floor_ratio * bg + recipe.floor_offset).min(base_l);
        if l < floor {
            (floor - l) / (base_l - l).max(1e-5)
        } else {
            0.
        }
    } else {
        let ceiling = ((bg - recipe.floor_offset) / recipe.floor_ratio).max(base_l);
        if l > ceiling {
            (l - ceiling) / (l - base_l).max(1e-5)
        } else {
            0.
        }
    };
    lerp3(lit, base, t.clamp(0., 1.))
}

/// One channel on a small grid, blurred with three box passes (close to a gaussian).
struct Plane {
    w: usize,
    h: usize,
    v: Vec<f32>,
}
impl Plane {
    fn blurred(mut self, radius: f32) -> Self {
        // Three boxes of radius r have a standard deviation of about r.
        let r = radius.round().max(0.) as usize;
        if r == 0 {
            return self;
        }
        let mut scratch = vec![0.; self.w.max(self.h)];
        for _ in 0..3 {
            for y in 0..self.h {
                let row = &mut self.v[y * self.w..(y + 1) * self.w];
                box_line(row, 1, self.w, r, &mut scratch);
            }
            for x in 0..self.w {
                box_line(&mut self.v[x..], self.w, self.h, r, &mut scratch);
            }
        }
        self
    }
    fn sample(&self, x: f32, y: f32) -> f32 {
        let x = x.clamp(0., (self.w - 1) as f32);
        let y = y.clamp(0., (self.h - 1) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (tx, ty) = (x - x0 as f32, y - y0 as f32);
        let at = |x: usize, y: usize| self.v[y * self.w + x];
        (at(x0, y0) * (1. - tx) + at(x1, y0) * tx) * (1. - ty)
            + (at(x0, y1) * (1. - tx) + at(x1, y1) * tx) * ty
    }
}
/// In-place running-sum box filter along one strided line, edges clamped.
fn box_line(line: &mut [f32], stride: usize, n: usize, r: usize, scratch: &mut [f32]) {
    let at = |line: &[f32], i: isize| line[(i.clamp(0, n as isize - 1) as usize) * stride];
    let ri = r as isize;
    let mut sum = 0.;
    for i in -ri..=ri {
        sum += at(line, i);
    }
    let norm = 1. / (2 * r + 1) as f32;
    for i in 0..n as isize {
        scratch[i as usize] = sum * norm;
        sum += at(line, i + ri + 1) - at(line, i - ri);
    }
    for i in 0..n {
        line[i * stride] = scratch[i];
    }
}

/// The picture around the clock, prepared once per render.
struct Scene {
    /// Crop origin in source pixels and the source to logical scale.
    origin: [f32; 2],
    /// Sky colour from a wide, matte-excluded neighbourhood (linear RGB).
    ambient: [Plane; 3],
    /// Sky colour right behind a pixel (narrow, matte-excluded).
    near: [Plane; 3],
    /// Luminance of `near`, for the direction the light comes from.
    near_luma: Plane,
    /// Mountain colour near a pixel (matte-only), for tinted shade.
    rock: [Plane; 3],
    /// Soft matte coverage, for contact shade.
    contact: Plane,
    /// Topmost matte row per crop column (source pixels), for haze.
    ridge: Vec<f32>,
    source_height: f32,
}

#[allow(clippy::too_many_arguments)]
fn prepare(
    picture: &image::RgbImage,
    matte: &image::GrayImage,
    window: [f32; 4],
    cells_per_logical: f32,
    recipe: &Recipe,
) -> Option<Scene> {
    let (sw, sh) = (picture.width() as usize, picture.height() as usize);
    // Three box passes reach about 2.5 radii; beyond that the crop adds nothing.
    let source_per_logical = cells_per_logical * CELL as f32;
    let margin = (recipe.ambient_radius.max(recipe.contact_radius) * 2.5 * source_per_logical)
        .max(8.)
        .min(sw as f32);
    let x0 = ((window[0] - margin).floor().max(0.) as usize).min(sw - 1);
    let y0 = ((window[1] - margin).floor().max(0.) as usize).min(sh - 1);
    let x1 = ((window[2] + margin).ceil().max(0.) as usize).clamp(x0 + 1, sw);
    let y1 = ((window[3] + margin).ceil().max(0.) as usize).clamp(y0 + 1, sh);
    let (w, h) = ((x1 - x0).div_ceil(CELL), (y1 - y0).div_ceil(CELL));
    let lut: Vec<f32> = (0..=255).map(to_linear).collect();
    let (rgb_raw, matte_raw) = (picture.as_raw(), matte.as_raw());
    let n = w * h;
    let mut sky_rgb = [vec![0.; n], vec![0.; n], vec![0.; n]];
    let mut rock_rgb = [vec![0.; n], vec![0.; n], vec![0.; n]];
    let mut sky = vec![0.; n];
    let mut fg = vec![0.; n];
    for cy in 0..h {
        for cx in 0..w {
            let (mut rgb, mut cover, mut count) = ([0f32; 3], 0f32, 0f32);
            for y in (y0 + cy * CELL)..(y0 + cy * CELL + CELL).min(y1) {
                for x in (x0 + cx * CELL)..(x0 + cx * CELL + CELL).min(x1) {
                    let at = y * sw + x;
                    for c in 0..3 {
                        rgb[c] += lut[usize::from(rgb_raw[at * 3 + c])];
                    }
                    cover += f32::from(matte_raw[at]) / 255.;
                    count += 1.;
                }
            }
            let i = cy * w + cx;
            let cover = cover / count;
            for c in 0..3 {
                let v = rgb[c] / count;
                sky_rgb[c][i] = v * (1. - cover);
                rock_rgb[c][i] = v * cover;
            }
            sky[i] = 1. - cover;
            fg[i] = cover;
        }
    }
    let cells = |logical: f32| logical * cells_per_logical;
    let plane = |v: Vec<f32>| Plane { w, h, v };
    // Normalised convolution: the blurred colour of only the weighted cells.
    // Visible digits always sit next to sky, and cut ones near rock, so the
    // weight never vanishes where the result is read.
    let weighted = |rgb: &[Vec<f32>; 3], weight: &Vec<f32>, radius: f32| -> [Plane; 3] {
        let den = plane(weight.clone()).blurred(radius);
        [0, 1, 2].map(|c| {
            let mut num = plane(rgb[c].clone()).blurred(radius);
            for (v, d) in num.v.iter_mut().zip(&den.v) {
                *v = (*v / d.max(1e-3)).min(1.);
            }
            num
        })
    };
    let ambient = weighted(&sky_rgb, &sky, cells(recipe.ambient_radius));
    let near = weighted(&sky_rgb, &sky, cells(recipe.ambient_radius * 0.18).max(2.));
    let rock = weighted(&rock_rgb, &fg, cells(recipe.contact_radius));
    let contact = plane(fg).blurred(cells(recipe.contact_radius) * 0.5);
    let near_luma = plane(
        (0..n)
            .map(|i| luma([near[0].v[i], near[1].v[i], near[2].v[i]]))
            .collect(),
    );
    let ridge = (0..w)
        .map(|cx| {
            let x = (x0 + cx * CELL + CELL / 2).min(sw - 1);
            (0..sh)
                .find(|&y| matte_raw[y * sw + x] >= 128)
                .map_or(sh as f32 * 4., |y| y as f32)
        })
        .collect();
    Some(Scene {
        origin: [x0 as f32, y0 as f32],
        ambient,
        near,
        near_luma,
        rock,
        contact,
        ridge,
        source_height: sh as f32,
    })
}

/// What the picture contributes at one logical pixel of the face.
#[derive(Clone, Copy)]
struct Terms {
    ambient: [f32; 3],
    hue: [f32; 3],
    settle: f32,
    behind: [f32; 3],
    bg: f32,
    shade: f32,
    tint: [f32; 3],
    haze: f32,
    glow: f32,
    /// Sky luminance gradient, toward the light.
    sky: [f32; 2],
}

/// The clock's pre-matte coverage over `rows` plus `pad` rows on each side,
/// averaged into `k` x `k` blocks (logical pixels on HiDPI) and blurred by
/// `radius` blocks, for the rim and its normals. Row 0 starts at `rows[0] - pad`.
fn glyph_field(
    alpha: &[u8],
    width: usize,
    height: usize,
    rows: [usize; 2],
    pad: usize,
    k: usize,
    radius: f32,
) -> Plane {
    let y0 = rows[0].saturating_sub(pad);
    let y1 = (rows[1] + pad).min(height);
    let (w, h) = (width.div_ceil(k), (y1 - y0).max(1).div_ceil(k));
    let mut v = vec![0.; w * h];
    for y in y0..y1 {
        let row = &alpha[y * width..(y + 1) * width];
        let out = &mut v[(y - y0) / k * w..][..w];
        for (x, a) in row.iter().enumerate() {
            out[x / k] += f32::from(*a);
        }
    }
    let norm = 1. / (255. * (k * k) as f32);
    v.iter_mut().for_each(|a| *a *= norm);
    Plane { w, h, v }.blurred(radius)
}

/// Relight the visible face in place. `glyph` is the pre-matte alpha.
#[allow(clippy::too_many_arguments)]
pub(super) fn apply(
    pixels: &mut Pixels,
    glyph: &[u8],
    frame: &Frame,
    mode: &ScalingMode,
    picture: &image::RgbImage,
    matte: &image::GrayImage,
    face: [f32; 2],
    recipe: &Recipe,
) -> Option<()> {
    let (scale, offset) = super::transform(frame, mode, [matte.width(), matte.height()])?;
    if picture.dimensions() != matte.dimensions() {
        return None;
    }
    let (width, height) = (pixels.width as usize, pixels.height as usize);
    let fs = f64::from(frame.scale);
    let to_source = |px: f64, py: f64| -> [f32; 2] {
        let position = [px, py].map(|v| (v + 0.5) / fs);
        [0, 1].map(|i| {
            ((f64::from(frame.origin[i]) + position[i] - offset[i]) / scale[i] - 0.5) as f32
        })
    };
    // Rows of the time itself; the date and padding stay as they are.
    let rows = [
        ((f64::from(face[0]) * fs).floor().max(0.) as usize).min(height),
        ((f64::from(face[1]) * fs).ceil().max(0.) as usize).min(height),
    ];
    if rows[0] >= rows[1] {
        return Some(());
    }
    let top_left = to_source(0., rows[0] as f64);
    let bottom_right = to_source(width as f64, rows[1] as f64);
    // Source pixels per logical pixel are 1 / scale; cells are CELL source pixels.
    let cells_per_logical = (1. / scale[0] / CELL as f64) as f32;
    let scene = prepare(
        picture,
        matte,
        [top_left[0], top_left[1], bottom_right[0], bottom_right[1]],
        cells_per_logical,
        recipe,
    )?;
    let cell = |s: [f32; 2]| {
        [
            (s[0] - scene.origin[0]) / CELL as f32 - 0.5,
            (s[1] - scene.origin[1]) / CELL as f32 - 0.5,
        ]
    };
    let rgb = |planes: &[Plane; 3], c: [f32; 2]| planes.each_ref().map(|p| p.sample(c[0], c[1]));

    // The rim works on logical pixels, so it costs the same at any scale.
    let k = (frame.scale.round() as usize).max(1);
    let wrap_blocks = recipe.wrap_radius * frame.scale / k as f32;
    let pad = (recipe.wrap_radius * frame.scale * 3.).ceil() as usize;
    let band0 = rows[0].saturating_sub(pad);
    let field = glyph_field(glyph, width, height, rows, pad, k, wrap_blocks);
    let block = 1. / k as f32;
    // The brightest sky near the face sets the rim's reference.
    let mut brightest = 1e-3f32;
    for gy in (rows[0]..rows[1]).step_by(8) {
        for gx in (0..width).step_by(8) {
            let c = cell(to_source(gx as f64, gy as f64));
            brightest = brightest.max(scene.near_luma.sample(c[0], c[1]));
        }
    }
    let lut: Vec<f32> = (0..=255).map(to_linear).collect();
    // Back to sRGB through a table: 4096 steps are finer than 8-bit output.
    let srgb: Vec<u8> = (0..=SRGB_STEPS)
        .map(|i| to_srgb(i as f32 / SRGB_STEPS as f32))
        .collect();
    let encode = |v: f32| srgb[(v.clamp(0., 1.) * SRGB_STEPS as f32 + 0.5) as usize];
    let span = (rows[1] - rows[0]).max(1) as f32;
    // Everything the picture contributes varies over many pixels, so it is
    // worked out once per logical pixel (k x k device pixels) and only for
    // blocks the clock actually covers.
    let terms_at = |bx: usize, by: usize| -> Terms {
        let centre = |b: usize| (b * k) as f64 + (k as f64 - 1.) / 2.;
        let s = to_source(centre(bx), centre(by));
        let c = cell(s);
        let ambient = rgb(&scene.ambient, c);
        let behind = rgb(&scene.near, c);
        let near_ridge = (scene.contact.sample(c[0], c[1]) * 2.2).min(1.);
        let shade = recipe.contact * smoothstep(near_ridge);
        let tint = if shade > 0. {
            lerp3([1.; 3], chroma(rgb(&scene.rock, c)), 0.35).map(|v| v.min(1.6))
        } else {
            [1.; 3]
        };
        let sky = &scene.near_luma;
        let ridge_y =
            scene.ridge[((c[0] + 0.5).round().max(0.) as usize).min(scene.ridge.len() - 1)];
        let above = ((ridge_y - s[1]) / (scene.source_height * recipe.haze_span)).clamp(0., 1.);
        let haze = recipe.haze_far + (recipe.haze_near - recipe.haze_far) * (1. - above);
        Terms {
            ambient,
            hue: chroma(ambient),
            settle: 1. - recipe.exposure * (1. - (luma(ambient) / 0.35).sqrt().min(1.)),
            behind,
            bg: luma(behind),
            shade,
            tint,
            haze: haze.clamp(0., 0.15),
            glow: (luma(behind) / brightest).powf(0.7),
            sky: [
                sky.sample(c[0] + 1., c[1]) - sky.sample(c[0] - 1., c[1]),
                sky.sample(c[0], c[1] + 1.) - sky.sample(c[0], c[1] - 1.),
            ],
        }
    };
    let mut cache: Vec<Option<Terms>> = vec![None; width.div_ceil(k)];
    let mut cached_row = usize::MAX;
    for y in rows[0]..rows[1] {
        if y / k != cached_row {
            cached_row = y / k;
            cache.iter_mut().for_each(|t| *t = None);
        }
        let fy = ((y - band0) as f32 + 0.5) * block - 0.5;
        // 2. Surface: a faint falloff from the top of the face.
        let sheen = 1. + recipe.sheen * (0.5 - (y - rows[0]) as f32 / span) * 2.;
        for x in 0..width {
            let i = (y * width + x) * 4;
            if pixels.rgba[i + 3] == 0 {
                continue;
            }
            let at = cache[x / k].get_or_insert_with(|| terms_at(x / k, y / k));
            let base = [0, 1, 2].map(|c| lut[usize::from(pixels.rgba[i + c])]);
            let base_l = luma(base);

            // 1. Ambient: the sky's own colour here, at the glyph's brightness,
            //    gently settled toward the scene's exposure.
            let hue = at.hue;
            let tinted = lerp3(
                base,
                with_luma(
                    [base[0] * hue[0], base[1] * hue[1], base[2] * hue[2]],
                    base_l,
                ),
                recipe.ambient,
            );
            let mut lit = with_luma(tinted, luma(tinted) * at.settle);
            lit = lit.map(|v| (v * sheen).clamp(0., 1.));

            // 3. Contact: shade where the ridge meets the digits, tinted by the rock.
            if at.shade > 0. {
                let (shade, tint) = (at.shade, at.tint);
                lit = [0, 1, 2]
                    .map(|k| (lit[k] * (1. - shade) * (1. + (tint[k] - 1.) * shade)).clamp(0., 1.));
            }

            // 4. Backlight: the sky behind bleeds a thin rim onto the edges,
            //    more on the edges that face the brighter sky.
            let fx = (x as f32 + 0.5) * block - 0.5;
            let edge = smoothstep((1. - field.sample(fx, fy)) * 2.);
            if edge > 0. && recipe.wrap > 0. {
                // Outward normal points down the coverage gradient.
                let normal = [
                    field.sample(fx - 1., fy) - field.sample(fx + 1., fy),
                    field.sample(fx, fy - 1.) - field.sample(fx, fy + 1.),
                ];
                let nl = (normal[0].hypot(normal[1])).max(1e-4);
                let gl = at.sky[0].hypot(at.sky[1]) + 0.004;
                let facing = ((normal[0] * at.sky[0] + normal[1] * at.sky[1]) / (nl * gl)).max(0.);
                let weight = (1. - recipe.wrap_facing) + recipe.wrap_facing * facing;
                // The rim keeps the glyph's own brightness, so settled faces read as
                // lit from behind rather than tinted.
                let rim = with_luma(at.behind, base_l.max(at.bg));
                lit = lerp3(lit, rim, (recipe.wrap * edge * weight * at.glow).min(0.85));
            }

            // 5. Haze: toward the local sky, thicker near the ridge.
            lit = lerp3(lit, at.ambient, at.haze);

            // 6. Readability floor against the sky right behind.
            lit = readable(lit, base, at.bg, recipe);
            for k in 0..3 {
                pixels.rgba[i + k] = encode(lit[k]);
            }
        }
    }
    Some(())
}
