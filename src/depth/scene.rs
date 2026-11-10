//! The foreground of any picture, from its depth, and where the clock can
//! tuck behind it. Ported from the depth spike (`render.py`): the mask is a
//! plane a little behind the nearest large structure, refined along the
//! picture's own edges; the verdict says honestly whether the clock reads as
//! sitting behind the scene where it is, somewhere close, or nowhere.
//!
//! Everything here is plain arithmetic on small grids, so the worker can run
//! it under its CPU limit and tests can drive it with synthetic depth.
use image::{GrayImage, RgbImage};
use serde::{Deserialize, Serialize};

/// The analysis works on about a 1920×1080 screen's worth of pixels: the
/// spike's radii and thresholds are in those pixels.
pub const WORK_AREA: f64 = 1920. * 1080.;
/// The clock's columns the foreground is judged in, as a share of the width
/// (the spike's Ridge clock ±150 px on a 1920 px screen).
const BAND: [f64; 2] = [0.2135, 0.7865];

/// The model's relative depth: larger is nearer.
pub struct Depth {
    pub width: u32,
    pub height: u32,
    pub values: Vec<f32>,
}

/// What a picture's analysis keeps: the picture-level numbers. The mask,
/// edges and work-size picture are stored beside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stats {
    /// The original picture's size, which the desktop scales and crops.
    pub source: [u32; 2],
    /// How cleanly depth splits into far and near (1 is two spikes).
    pub eta: f32,
    /// Share of the picture in front of the clock's plane.
    pub coverage: f32,
    /// Share of the top rows in front: a subject cut off by the top edge.
    pub top_touch: f32,
    /// Almost no structure at all, such as a gradient.
    pub flat: bool,
}

pub struct Scene {
    /// 255 where the foreground is in front of the clock.
    pub mask: GrayImage,
    /// 255 near a strong edge in the picture.
    pub edges: GrayImage,
    /// The picture at the mask's size, for lighting.
    pub picture: RgbImage,
    pub stats: Stats,
}

/// The work size for a picture: its shape at about [`WORK_AREA`] pixels,
/// never larger than the picture itself.
pub fn work_size(source: [u32; 2]) -> [u32; 2] {
    let area = f64::from(source[0]) * f64::from(source[1]);
    let k = (WORK_AREA / area.max(1.)).sqrt().min(1.);
    source.map(|v| ((f64::from(v) * k).round() as u32).max(16))
}

/// Pixels of work size per spike pixel.
fn unit(work: [u32; 2]) -> f64 {
    (f64::from(work[0]) * f64::from(work[1]) / WORK_AREA).sqrt()
}

/// A single-channel float image.
#[derive(Clone)]
pub(crate) struct Grid {
    pub w: usize,
    pub h: usize,
    pub v: Vec<f32>,
}
impl Grid {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            v: vec![0.; w * h],
        }
    }
    fn map(&self, f: impl Fn(f32) -> f32) -> Self {
        Self {
            w: self.w,
            h: self.h,
            v: self.v.iter().map(|v| f(*v)).collect(),
        }
    }
    fn zip(&self, other: &Self, f: impl Fn(f32, f32) -> f32) -> Self {
        Self {
            w: self.w,
            h: self.h,
            v: self
                .v
                .iter()
                .zip(&other.v)
                .map(|(a, b)| f(*a, *b))
                .collect(),
        }
    }
}

/// Mean over `[i - r, i + r]` along one axis, edges repeated.
fn box1(grid: &Grid, r: usize, vertical: bool) -> Grid {
    if r == 0 {
        return grid.clone();
    }
    let (n, lines) = if vertical {
        (grid.h, grid.w)
    } else {
        (grid.w, grid.h)
    };
    let at = |line: usize, i: usize| -> usize {
        if vertical {
            i * grid.w + line
        } else {
            line * grid.w + i
        }
    };
    let mut out = Grid::new(grid.w, grid.h);
    let norm = 1. / (2 * r + 1) as f64;
    for line in 0..lines {
        let clamp = |i: isize| -> usize { i.clamp(0, n as isize - 1) as usize };
        let mut sum = 0f64;
        for i in -(r as isize)..=(r as isize) {
            sum += f64::from(grid.v[at(line, clamp(i))]);
        }
        for i in 0..n {
            out.v[at(line, i)] = (sum * norm) as f32;
            sum += f64::from(grid.v[at(line, clamp(i as isize + r as isize + 1))]);
            sum -= f64::from(grid.v[at(line, clamp(i as isize - r as isize))]);
        }
    }
    out
}
fn boxed(grid: &Grid, r: usize) -> Grid {
    box1(&box1(grid, r, true), r, false)
}
/// Three box passes, close to a gaussian of `sigma`.
fn blur(grid: &Grid, sigma: f64) -> Grid {
    if sigma <= 0.3 {
        return grid.clone();
    }
    let r = ((sigma * 0.87).round() as usize).max(1);
    let mut out = grid.clone();
    for _ in 0..3 {
        out = boxed(&out, r);
    }
    out
}
/// He et al.'s guided filter with a grey guide: four box passes.
fn guided(guide: &Grid, src: &Grid, r: usize, eps: f32) -> Grid {
    let mean_i = boxed(guide, r);
    let mean_p = boxed(src, r);
    let cov = boxed(&guide.zip(src, |a, b| a * b), r)
        .zip(&mean_i.zip(&mean_p, |a, b| a * b), |a, b| a - b);
    let var = boxed(&guide.map(|a| a * a), r).zip(&mean_i, |a, m| a - m * m);
    let a = cov.zip(&var, |c, v| c / (v + eps));
    let b = mean_p.zip(&a.zip(&mean_i, |a, m| a * m), |p, am| p - am);
    let (a, b) = (boxed(&a, r), boxed(&b, r));
    let mut out = a.zip(guide, |a, g| a * g);
    for (o, b) in out.v.iter_mut().zip(&b.v) {
        *o = (*o + b).clamp(0., 1.);
    }
    out
}

/// numpy's linear percentile.
fn percentiles(values: &[f32], qs: &[f64]) -> Vec<f32> {
    if values.is_empty() {
        return vec![0.; qs.len()];
    }
    let mut v = values.to_vec();
    v.sort_unstable_by(f32::total_cmp);
    qs.iter()
        .map(|q| {
            let at = q / 100. * (v.len() - 1) as f64;
            let (lo, hi) = (at.floor() as usize, at.ceil() as usize);
            let t = (at - lo as f64) as f32;
            v[lo] + (v[hi] - v[lo]) * t
        })
        .collect()
}
fn percentile(values: &[f32], q: f64) -> f32 {
    percentiles(values, &[q])[0]
}

/// Otsu's threshold on 256 bins of `[0, 1]`, and how bimodal the values are:
/// between-class over total variance.
pub(crate) fn otsu(values: &[f32]) -> (f32, f32) {
    let mut hist = [0f64; 256];
    for v in values.iter().filter(|v| v.is_finite()) {
        if (0. ..=1.).contains(v) {
            hist[((v * 256.) as usize).min(255)] += 1.;
        }
    }
    let total: f64 = hist.iter().sum::<f64>().max(1.);
    let centre = |k: usize| (k as f64 + 0.5) / 256.;
    let mt: f64 = (0..256).map(|k| hist[k] / total * centre(k)).sum();
    let (mut w0, mut m0, mut best, mut at) = (0f64, 0f64, -1f64, 0usize);
    for k in 0..256 {
        w0 += hist[k] / total;
        m0 += hist[k] / total * centre(k);
        let w1 = 1. - w0;
        let sb = if w0 > 0. && w1 > 1e-12 {
            (mt * w0 - m0).powi(2) / (w0 * w1)
        } else {
            0.
        };
        if sb > best {
            best = sb;
            at = k;
        }
    }
    let n = values.len().max(1) as f64;
    let mean = values.iter().map(|v| f64::from(*v)).sum::<f64>() / n;
    let var = values
        .iter()
        .map(|v| (f64::from(*v) - mean).powi(2))
        .sum::<f64>()
        / n;
    (centre(at) as f32, (best.max(0.) / var.max(1e-9)) as f32)
}

fn resize_grid(grid: &Grid, size: [u32; 2]) -> Grid {
    let image = image::ImageBuffer::<image::Luma<f32>, Vec<f32>>::from_raw(
        grid.w as u32,
        grid.h as u32,
        grid.v.clone(),
    )
    .expect("grid buffer");
    let out = image::imageops::resize(
        &image,
        size[0],
        size[1],
        image::imageops::FilterType::CatmullRom,
    );
    Grid {
        w: size[0] as usize,
        h: size[1] as usize,
        v: out.into_raw(),
    }
}

/// Depth at the work size, stretched so the 1st and 99.5th percentiles are 0 and 1.
pub(crate) fn normalise(depth: &Depth, work: [u32; 2]) -> Option<Grid> {
    let raw = Grid {
        w: depth.width as usize,
        h: depth.height as usize,
        v: depth.values.clone(),
    };
    if raw.w == 0
        || raw.h == 0
        || raw.v.len() != raw.w * raw.h
        || raw.v.iter().any(|v| !v.is_finite())
    {
        return None;
    }
    // The image crate clamps float pixels to [0, 1] when resizing, so the
    // raw depth goes in the middle of that range, leaving room for the
    // filter's overshoot at sharp edges.
    let (lo, hi) = raw
        .v
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    let k = 0.5 / (hi - lo).max(1e-6);
    let d = resize_grid(&raw.map(|v| 0.25 + (v - lo) * k), work);
    let p = percentiles(&d.v, &[1., 99.5]);
    let span = (p[1] - p[0]).max(1e-6);
    Some(d.map(|v| ((v - p[0]) / span).clamp(0., 1.)))
}

/// The foreground: in front of a plane a little behind the nearest large
/// structure in the clock's columns.
pub(crate) fn foreground(dn: &Grid, guide: &Grid, unit: f64) -> (Grid, f32) {
    let cols = [
        (BAND[0] * dn.w as f64) as usize,
        ((BAND[1] * dn.w as f64) as usize).max((BAND[0] * dn.w as f64) as usize + 1),
    ];
    let area = unit * unit;
    let region: Vec<f32> = (0..dn.h)
        .flat_map(|y| dn.v[y * dn.w + cols[0]..y * dn.w + cols[1]].iter().copied())
        .collect();
    // The background is the farthest tight cluster (sky, a back wall): keep
    // splitting the far class until it is narrow, so a near lake or lawn
    // does not swallow the structure the clock should tuck behind.
    let (mut t0, _) = otsu(&region);
    for _ in 0..4 {
        let far: Vec<f32> = region.iter().copied().filter(|v| *v <= t0).collect();
        if (far.len() as f64) < 1000. * area {
            break;
        }
        let p = percentiles(&far, &[5., 95.]);
        if p[1] - p[0] < 0.06 {
            break;
        }
        let scaled: Vec<f32> = far
            .iter()
            .map(|v| (v / t0.max(1e-6)).clamp(0., 1.))
            .collect();
        t0 *= otsu(&scaled).0;
    }
    let (a, b): (Vec<f32>, Vec<f32>) = region.iter().partition(|v| **v <= t0);
    let eta = if a.is_empty() || b.is_empty() {
        0.
    } else {
        let mean = |v: &[f32]| v.iter().map(|x| f64::from(*x)).sum::<f64>() / v.len() as f64;
        let n = region.len() as f64;
        let all = mean(&region);
        let var = region
            .iter()
            .map(|v| (f64::from(*v) - all).powi(2))
            .sum::<f64>()
            / n;
        (a.len() as f64 * b.len() as f64 / (n * n) * (mean(&b) - mean(&a)).powi(2) / var.max(1e-9))
            as f32
    };
    let near = dn.map(|v| if v > t0 { 1. } else { 0. });
    let opened = blur(&near, 4. * unit).map(|v| if v > 0.95 { 1. } else { 0. });
    let large = blur(&opened, 4. * unit).map(|v| if v > 0.05 { 1. } else { 0. });
    let pick = |band: bool| -> Vec<f32> {
        (0..dn.h)
            .flat_map(|y| (0..dn.w).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                large.v[y * dn.w + x] > 0. && (!band || (cols[0]..cols[1]).contains(&x))
            })
            .map(|(x, y)| dn.v[y * dn.w + x])
            .collect()
    };
    let mut vals = pick(true);
    if (vals.len() as f64) < 2000. * area {
        vals = pick(false);
    }
    if (vals.len() as f64) < 2000. * area {
        return (Grid::new(dn.w, dn.h), eta);
    }
    let far = percentile(&vals, 3.);
    let behind: Vec<f32> = dn.v.iter().copied().filter(|v| *v <= t0).collect();
    let bg = if behind.is_empty() {
        0.
    } else {
        percentile(&behind, 50.)
    };
    let plane = far - 0.25 * (far - bg);
    let soft = dn.map(|v| ((v - plane) / 0.006 + 0.5).clamp(0., 1.));
    let refined = guided(guide, &soft, ((8. * unit).round() as usize).max(1), 4e-4);
    (refined.map(|v| ((v - 0.5) * 1.6 + 0.5).clamp(0., 1.)), eta)
}

fn luma(picture: &RgbImage) -> Grid {
    Grid {
        w: picture.width() as usize,
        h: picture.height() as usize,
        v: picture
            .pixels()
            .map(|p| {
                (f32::from(p[0]) * 0.2126 + f32::from(p[1]) * 0.7152 + f32::from(p[2]) * 0.0722)
                    / 255.
            })
            .collect(),
    }
}
fn gradient(l: &Grid) -> Grid {
    let mut out = Grid::new(l.w, l.h);
    for y in 0..l.h {
        for x in 0..l.w {
            let gx = if x > 0 && x + 1 < l.w {
                l.v[y * l.w + x + 1] - l.v[y * l.w + x - 1]
            } else {
                0.
            };
            let gy = if y > 0 && y + 1 < l.h {
                l.v[(y + 1) * l.w + x] - l.v[(y - 1) * l.w + x]
            } else {
                0.
            };
            out.v[y * l.w + x] = gx.hypot(gy);
        }
    }
    out
}

fn to_gray(grid: &Grid) -> GrayImage {
    GrayImage::from_raw(
        grid.w as u32,
        grid.h as u32,
        grid.v
            .iter()
            .map(|v| (v.clamp(0., 1.) * 255.).round() as u8)
            .collect(),
    )
    .expect("gray buffer")
}

/// The picture's foreground mask, edges and numbers from its depth.
pub fn derive(depth: &Depth, picture: &RgbImage) -> Option<Scene> {
    let source = [picture.width(), picture.height()];
    let work = work_size(source);
    let unit = unit(work);
    let small = image::imageops::resize(
        picture,
        work[0],
        work[1],
        image::imageops::FilterType::Triangle,
    );
    let dn = normalise(depth, work)?;
    let l = luma(&small);
    let (mask, eta) = foreground(&dn, &l, unit);
    let mag = gradient(&l);
    let threshold = percentile(&mag.v, 90.).max(0.02);
    let strong = mag.map(|v| if v > threshold { 1. } else { 0. });
    let edges =
        boxed(&strong, ((2. * unit).round() as usize).max(1)).map(|v| if v > 0. { 1. } else { 0. });
    let n = l.v.len() as f64;
    let mean = l.v.iter().map(|v| f64::from(*v)).sum::<f64>() / n;
    let std =
        (l.v.iter()
            .map(|v| (f64::from(*v) - mean).powi(2))
            .sum::<f64>()
            / n)
            .sqrt();
    let busy = mag.v.iter().filter(|v| **v > 0.04).count() as f64 / n;
    let solid = |v: &f32| *v > 0.5;
    let coverage = mask.v.iter().filter(|v| solid(v)).count() as f32 / mask.v.len() as f32;
    let top = (4. * unit).round().max(1.) as usize;
    let top_touch = mask.v[..top.min(mask.h) * mask.w]
        .iter()
        .filter(|v| solid(v))
        .count() as f32
        / (top.min(mask.h) * mask.w).max(1) as f32;
    Some(Scene {
        mask: to_gray(&mask),
        edges: to_gray(&edges),
        picture: small,
        stats: Stats {
            source,
            eta,
            coverage,
            top_touch,
            flat: std < 0.06 && busy < 0.01,
        },
    })
}

// ---------------------------------------------------------------- judging

/// Whether the effect is worth having for this clock on this picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// The scene covers the bottom of the time where the clock is.
    Works,
    /// It would, a little way from here.
    NeedsNudge,
    /// Nowhere useful; the clock stays plain.
    NotUseful,
}

/// Why a picture is not useful, for the one plain sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reason {
    /// Nothing stands in front of anything.
    NoForeground,
    /// The foreground fills nearly the whole picture.
    FillsPicture,
    /// No place where the scene covers only the bottom of the time.
    NoSpot,
    /// The foreground's outline does not follow the picture.
    Uneven,
}
impl Reason {
    pub fn sentence(self) -> &'static str {
        match self {
            Self::NoForeground => "This picture has no clear foreground",
            Self::FillsPicture => "The foreground fills almost the whole picture",
            Self::NoSpot => "Nothing in this picture sits low enough to tuck the clock behind",
            Self::Uneven => "The foreground’s outline is too uneven to cover the clock cleanly",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Judgement {
    pub verdict: Verdict,
    /// 0 to 1: how much the outline and depth can be trusted.
    pub confidence: f32,
    /// For [`Verdict::NeedsNudge`]: the clock's origin in screen pixels.
    pub suggested: Option<[f32; 2]>,
    pub reason: Option<Reason>,
    /// The measures behind it, for evaluation and tests.
    pub measures: Measures,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Measures {
    /// Hidden share of the top two thirds of the digits' ink.
    pub hidden_top: f32,
    /// Hidden share of the bottom third.
    pub hidden_bottom: f32,
    /// Hidden share of all the ink.
    pub hidden_all: f32,
    /// The most any one stretch of the digits has its top two thirds hidden.
    pub cut: f32,
    /// Share of the row under the digits that is foreground.
    pub ground: f32,
    /// Outline pixels on strong picture edges.
    pub align: f32,
    /// Outline pixels per column near the clock.
    pub ragged: f32,
    /// How far the spot is from where the clock is, in screen heights.
    pub moved: f32,
}

/// Limits a spot has to meet. `hidden_top` and `hidden_bottom` are the
/// spike's; `cut` and `ground` catch a head poking into the digits and a
/// subject floating free of the ground (spike inputs yqkl97 and k8jox7).
pub const MAX_HIDDEN_TOP: f32 = 0.03;
pub const MIN_HIDDEN_BOTTOM: f32 = 0.15;
pub const MAX_HIDDEN_ALL: f32 = 0.45;
pub const MAX_CUT: f32 = 0.35;
pub const MIN_GROUND: f32 = 0.5;
pub const WORKS: f32 = 0.35;
/// Outline pixels per column past which the foreground is a figure or
/// foliage poking into the time, not a skyline (the evaluation's person
/// cutting into the digits measured 9.1; the busiest good skyline 6.8).
pub const MAX_RAGGED: f32 = 8.;

/// The clock as the judge sees it: its straight alpha, the face rows of
/// the time, and where it is on the screen.
pub struct Glyph<'a> {
    pub alpha: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub frame: &'a super::Frame,
    pub face: [f32; 2],
}

/// Screen to mask coordinates for one frame and scaling mode.
#[derive(Clone, Copy)]
pub(crate) struct Map {
    /// Screen pixels per mask pixel, and the picture's offset on screen.
    pub scale: [f64; 2],
    pub offset: [f64; 2],
}
impl Map {
    pub fn new(
        frame: &super::Frame,
        mode: &crate::desktop::wallpaper::ScalingMode,
        source: [u32; 2],
        raster: [u32; 2],
    ) -> Option<Self> {
        let (scale, offset) = super::transform_to(frame, mode, source, raster)?;
        Some(Self { scale, offset })
    }
    /// Mask coordinates of a logical screen point, pixel centres at .5.
    pub fn to_mask(&self, point: [f64; 2]) -> [f64; 2] {
        [0, 1].map(|i| (point[i] - self.offset[i]) / self.scale[i] - 0.5)
    }
}

struct Sample {
    /// Logical screen point relative to the frame origin.
    at: [f64; 2],
    weight: f32,
    top: bool,
    group: usize,
}

struct Ink {
    samples: Vec<Sample>,
    groups: usize,
    /// Logical rows and columns of the ink, relative to the frame origin.
    rows: [f64; 2],
    cols: [f64; 2],
}

fn ink(glyph: &Glyph) -> Option<Ink> {
    let (w, h) = (glyph.width as usize, glyph.height as usize);
    if glyph.alpha.len() != w * h {
        return None;
    }
    let s = f64::from(glyph.frame.scale);
    let face = [
        ((f64::from(glyph.face[0]) * s).floor().max(0.) as usize).min(h),
        ((f64::from(glyph.face[1]) * s).ceil().max(0.) as usize).min(h),
    ];
    let solid = |x: usize, y: usize| glyph.alpha[y * w + x] > 64;
    let rows: Vec<usize> = (face[0]..face[1])
        .filter(|&y| (0..w).any(|x| solid(x, y)))
        .collect();
    let (top, bottom) = (*rows.first()?, *rows.last()? + 1);
    let cols: Vec<usize> = (0..w)
        .filter(|&x| (top..bottom).any(|y| solid(x, y)))
        .collect();
    let (left, right) = (*cols.first()?, *cols.last()? + 1);
    let split = top as f64 + (bottom - top) as f64 * 2. / 3.;
    // About 6,000 samples whatever the clock's size.
    let stride = ((((right - left) * (bottom - top)) as f64 / 6000.)
        .sqrt()
        .floor() as usize)
        .max(1);
    let group_width = ((bottom - top) / 8).max(1);
    let groups = (right - left).div_ceil(group_width);
    let mut samples = Vec::new();
    for y in (top..bottom).step_by(stride) {
        for x in (left..right).step_by(stride) {
            let a = glyph.alpha[y * w + x];
            if a == 0 {
                continue;
            }
            samples.push(Sample {
                at: [(x as f64 + 0.5) / s, (y as f64 + 0.5) / s],
                weight: f32::from(a) / 255.,
                top: (y as f64 + 0.5) < split,
                group: (x - left) / group_width,
            });
        }
    }
    Some(Ink {
        samples,
        groups,
        rows: [top as f64 / s, bottom as f64 / s],
        cols: [left as f64 / s, right as f64 / s],
    })
}

fn bilinear(mask: &GrayImage, p: [f64; 2]) -> f32 {
    let (w, h) = (mask.width() as f64, mask.height() as f64);
    if !(p[0] >= -0.5 && p[1] >= -0.5 && p[0] < w - 0.5 && p[1] < h - 0.5) {
        return 0.;
    }
    let x = p[0].clamp(0., w - 1.);
    let y = p[1].clamp(0., h - 1.);
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = (
        (x0 + 1).min(mask.width() - 1),
        (y0 + 1).min(mask.height() - 1),
    );
    let (tx, ty) = ((x - x.floor()) as f32, (y - y.floor()) as f32);
    let at = |x, y| f32::from(mask.get_pixel(x, y)[0]) / 255.;
    (at(x0, y0) * (1. - tx) + at(x1, y0) * tx) * (1. - ty)
        + (at(x0, y1) * (1. - tx) + at(x1, y1) * tx) * ty
}

/// How the ink at `origin` meets the mask.
fn cover(ink: &Ink, map: &Map, mask: &GrayImage, origin: [f64; 2]) -> Measures {
    let (mut top, mut top_all, mut bottom, mut bottom_all) = (0f32, 0f32, 0f32, 0f32);
    let mut groups = vec![[0f32; 2]; ink.groups];
    for s in &ink.samples {
        let hidden = bilinear(
            mask,
            map.to_mask([origin[0] + s.at[0], origin[1] + s.at[1]]),
        ) * s.weight;
        if s.top {
            top += hidden;
            top_all += s.weight;
            groups[s.group][0] += hidden;
            groups[s.group][1] += s.weight;
        } else {
            bottom += hidden;
            bottom_all += s.weight;
        }
    }
    // A stretch counts once it holds a fair share of the top ink, so a
    // stray antialiased pixel cannot decide it.
    let fair = top_all / ink.groups.max(1) as f32 * 0.25;
    let cut = groups
        .iter()
        .filter(|g| g[1] > fair)
        .map(|g| g[0] / g[1])
        .fold(0f32, f32::max);
    let height = ink.rows[1] - ink.rows[0];
    let y = origin[1] + ink.rows[1] + height * 0.2;
    let points = 32;
    let ground = (0..points)
        .filter(|i| {
            let x = origin[0]
                + ink.cols[0]
                + (ink.cols[1] - ink.cols[0]) * (*i as f64 + 0.5) / points as f64;
            bilinear(mask, map.to_mask([x, y])) > 0.5
        })
        .count() as f32
        / points as f32;
    Measures {
        hidden_top: top / top_all.max(1e-6),
        hidden_bottom: bottom / bottom_all.max(1e-6),
        hidden_all: (top + bottom) / (top_all + bottom_all).max(1e-6),
        cut,
        ground,
        ..Default::default()
    }
}

fn passes(m: &Measures) -> bool {
    m.hidden_top <= MAX_HIDDEN_TOP
        && m.cut <= MAX_CUT
        && m.hidden_bottom >= MIN_HIDDEN_BOTTOM
        && m.hidden_all <= MAX_HIDDEN_ALL
        && m.ground >= MIN_GROUND
}

/// The outline's fit near a spot: on strong picture edges, and how many
/// outline pixels each column holds (one clean skyline is about 1.2 to 2).
fn outline(
    ink: &Ink,
    map: &Map,
    mask: &GrayImage,
    edges: &GrayImage,
    origin: [f64; 2],
) -> (f32, f32) {
    let (w, h) = (mask.width() as i64, mask.height() as i64);
    let unit = unit([mask.width(), mask.height()]);
    let a = map.to_mask([origin[0] + ink.cols[0], origin[1]]);
    let b = map.to_mask([
        origin[0] + ink.cols[1],
        origin[1] + ink.rows[0] + (ink.rows[1] - ink.rows[0]) * 2.5,
    ]);
    let x0 = ((a[0] - 150. * unit).floor() as i64).clamp(0, w);
    let x1 = ((b[0] + 150. * unit).ceil() as i64).clamp(0, w);
    let y1 = (b[1].ceil() as i64).clamp(0, h);
    if x1 - x0 < 2 || y1 < 2 {
        return (0., 0.);
    }
    let solid = |x: i64, y: i64| -> f32 {
        if mask.get_pixel(x.clamp(0, w - 1) as u32, y.clamp(0, h - 1) as u32)[0] > 127 {
            1.
        } else {
            0.
        }
    };
    let (mut boundary, mut on_edge) = (0u32, 0u32);
    for y in 0..y1 {
        for x in x0..x1 {
            let mut sum = 0.;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    sum += solid(x + dx, y + dy);
                }
            }
            let mean = sum / 9.;
            if mean > 0.05 && mean < 0.95 {
                boundary += 1;
                if edges.get_pixel(x as u32, y as u32)[0] > 127 {
                    on_edge += 1;
                }
            }
        }
    }
    let align = if boundary == 0 {
        0.
    } else {
        on_edge as f32 / boundary as f32
    };
    (align, boundary as f32 / (x1 - x0) as f32)
}

/// How far the outline and depth can be trusted, from 1 down. Calibrated on
/// the spike's pictures (see docs/future-builder/FB14.md): moving the clock
/// is the nudge itself, so it costs nothing here.
fn confidence(stats: &Stats, m: &Measures) -> f32 {
    if m.ragged > MAX_RAGGED {
        return 0.;
    }
    let mut c = 1f32;
    if m.align < 0.45 {
        c -= 0.3;
    }
    if stats.eta < 0.35 {
        c -= 0.1;
    }
    if m.ragged > 6.5 {
        c -= 0.2;
    } else if m.ragged > 4.5 {
        c -= 0.1;
    }
    if stats.top_touch > 0.3 {
        c -= 0.15;
    }
    if m.hidden_bottom < 0.2 {
        c -= 0.15;
    }
    c.max(0.)
}

/// The verdict for the clock where it is, and a nearby spot when it only
/// needs a nudge.
pub fn judge(
    stats: &Stats,
    mask: &GrayImage,
    edges: &GrayImage,
    glyph: &Glyph,
    mode: &crate::desktop::wallpaper::ScalingMode,
) -> Option<Judgement> {
    let not_useful = |reason, measures| Judgement {
        verdict: Verdict::NotUseful,
        confidence: 0.,
        suggested: None,
        reason: Some(reason),
        measures,
    };
    if stats.flat || stats.coverage < 0.02 {
        return Some(not_useful(Reason::NoForeground, Measures::default()));
    }
    if stats.coverage > 0.85 {
        return Some(not_useful(Reason::FillsPicture, Measures::default()));
    }
    let map = Map::new(
        glyph.frame,
        mode,
        stats.source,
        [mask.width(), mask.height()],
    )?;
    let ink = ink(glyph)?;
    let frame = glyph.frame;
    let here = frame.origin.map(f64::from);
    let screen = frame.screen.map(f64::from);
    let size = frame.size.map(f64::from);
    let finish = |origin: [f64; 2], mut m: Measures| -> Measures {
        let (align, ragged) = outline(&ink, &map, mask, edges, origin);
        m.align = align;
        m.ragged = ragged;
        m.moved = ((origin[0] - here[0]).hypot(origin[1] - here[1]) / screen[1]) as f32;
        m
    };
    let current = cover(&ink, &map, mask, here);
    if passes(&current) {
        let m = finish(here, current);
        let confidence = confidence(stats, &m);
        return Some(if confidence >= WORKS {
            Judgement {
                verdict: Verdict::Works,
                confidence,
                suggested: None,
                reason: None,
                measures: m,
            }
        } else {
            Judgement {
                confidence,
                ..not_useful(Reason::Uneven, m)
            }
        });
    }
    // Search the clock's own column and three others, from near the top to
    // the middle of the screen, then refine around the best.
    let max = [screen[0] - size[0], screen[1] - size[1]].map(|v| v.max(0.).floor());
    let centre_x = |share: f64| {
        (screen[0] * share - (ink.cols[0] + ink.cols[1]) / 2.)
            .clamp(0., max[0])
            .round()
    };
    let mut xs = vec![here[0], centre_x(0.33), centre_x(0.5), centre_x(0.67)];
    xs.dedup_by(|a, b| (*a - *b).abs() < 1.);
    let ink_height = ink.rows[1] - ink.rows[0];
    let lowest = (screen[1] * 0.5 - ink.rows[0]).min(max[1]);
    let highest = (screen[1] * 0.04 - ink.rows[0]).max(0.);
    let score = |origin: [f64; 2], m: &Measures| -> f64 {
        let moved = (origin[0] - here[0]).hypot(origin[1] - here[1]) / screen[1];
        f64::from(m.hidden_bottom.min(0.55))
            - 0.15 * moved
            - if (origin[0] - here[0]).abs() >= 1. {
                0.05
            } else {
                0.
            }
    };
    let try_at = |origin: [f64; 2]| -> Option<([f64; 2], Measures, f64)> {
        if origin[0] < 0. || origin[1] < 0. || origin[0] > max[0] || origin[1] > max[1] {
            return None;
        }
        let m = cover(&ink, &map, mask, origin);
        if !passes(&m) {
            return None;
        }
        let s = score(origin, &m);
        Some((origin, m, s))
    };
    let mut best: Option<([f64; 2], Measures, f64)> = None;
    let keep = |found: Option<([f64; 2], Measures, f64)>,
                best: &mut Option<([f64; 2], Measures, f64)>| {
        if let Some(found) = found
            && best.as_ref().is_none_or(|b| found.2 > b.2)
        {
            *best = Some(found);
        }
    };
    let step = (ink_height / 24.).max(4. * screen[1] / 1080.).max(1.);
    for &x in &xs {
        let mut y = highest;
        while y <= lowest {
            keep(try_at([x, y.round()]), &mut best);
            y += step;
        }
    }
    if let Some(origin) = best.as_ref().map(|b| b.0) {
        let fine = (step / 4.).max(1.);
        for i in -4..=4 {
            keep(
                try_at([origin[0], (origin[1] + f64::from(i) * fine).round()]),
                &mut best,
            );
        }
    }
    let Some((origin, m, _)) = best else {
        return Some(not_useful(Reason::NoSpot, current));
    };
    let m = finish(origin, m);
    let confidence = confidence(stats, &m);
    Some(if confidence >= WORKS {
        Judgement {
            verdict: Verdict::NeedsNudge,
            confidence,
            suggested: Some(origin.map(|v| v as f32)),
            reason: None,
            measures: m,
        }
    } else {
        Judgement {
            confidence,
            ..not_useful(Reason::Uneven, m)
        }
    })
}

#[cfg(test)]
mod tests;
