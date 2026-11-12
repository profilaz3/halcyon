//! The artwork's dominant colour, for the island's music waves and scrubber.
//! Worked out once per picture from the thumbnail the Music card already
//! decodes; greys, near-black and near-white pixels do not vote.

const BINS: usize = 24;
/// Every n-th pixel is enough for a 64 px thumbnail and keeps this cheap.
const STEP: usize = 3;

/// The most common vivid hue, as the average colour of the pixels that
/// share it. None for greyscale or nearly empty pictures.
pub fn dominant(rgba: &[u8]) -> Option<[u8; 3]> {
    let mut weight = [0f32; BINS];
    let mut sums = [[0f32; 3]; BINS];
    let mut total = 0f32;
    let mut voters = 0usize;
    for px in rgba.chunks_exact(4).step_by(STEP) {
        if px[3] < 128 {
            continue;
        }
        let [r, g, b] = [px[0], px[1], px[2]].map(|c| c as f32 / 255.);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let chroma = max - min;
        let saturation = if max > 0. { chroma / max } else { 0. };
        if saturation < 0.25 || max < 0.2 || (max > 0.97 && saturation < 0.35) {
            continue;
        }
        let hue = if max == r {
            ((g - b) / chroma).rem_euclid(6.)
        } else if max == g {
            (b - r) / chroma + 2.
        } else {
            (r - g) / chroma + 4.
        } / 6.;
        let bin = ((hue * BINS as f32) as usize).min(BINS - 1);
        let w = saturation * max;
        weight[bin] += w;
        for (sum, c) in sums[bin].iter_mut().zip([r, g, b]) {
            *sum += c * w;
        }
        total += w;
        voters += 1;
    }
    let sampled = rgba.len() / 4 / STEP;
    // A few coloured specks on a grey cover are not its colour.
    if voters == 0 || voters * 20 < sampled.max(1) {
        return None;
    }
    let (bin, w) = weight
        .iter()
        .copied()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    if w <= 0. || total <= 0. {
        return None;
    }
    Some(sums[bin].map(|c| ((c / w) * 255.).round().clamp(0., 255.) as u8))
}

/// The island's artwork tile is drawn from this many pixels a side: enough
/// for a 44 px tile, small enough to cross the bridge in every snapshot.
pub const THUMB: usize = 28;

/// A square, centre-cropped RGB thumbnail of an RGBA picture, base64
/// encoded for the bridge. None for an empty or malformed picture.
pub fn thumbnail(rgba: &[u8], width: u32, height: u32) -> Option<String> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return None;
    }
    let side = w.min(h);
    let (x0, y0) = ((w - side) / 2, (h - side) / 2);
    let mut out = Vec::with_capacity(THUMB * THUMB * 3);
    for ty in 0..THUMB {
        let (ya, yb) = (y0 + ty * side / THUMB, y0 + ((ty + 1) * side / THUMB).max(ty * side / THUMB + 1));
        for tx in 0..THUMB {
            let (xa, xb) = (x0 + tx * side / THUMB, x0 + ((tx + 1) * side / THUMB).max(tx * side / THUMB + 1));
            let mut sum = [0u32; 3];
            let mut n = 0u32;
            for y in ya..yb.min(h) {
                for x in xa..xb.min(w) {
                    let i = (y * w + x) * 4;
                    for (s, c) in sum.iter_mut().zip(&rgba[i..i + 3]) {
                        *s += u32::from(*c);
                    }
                    n += 1;
                }
            }
            out.extend(sum.map(|s| (s / n.max(1)) as u8));
        }
    }
    Some(STANDARD.encode(out))
}

/// The thumbnail back as RGBA pixels, only when it has the expected size.
pub fn thumbnail_pixels(encoded: &str) -> Option<Vec<u8>> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    if encoded.len() > (THUMB * THUMB * 3).div_ceil(3) * 4 {
        return None;
    }
    let rgb = STANDARD.decode(encoded).ok()?;
    (rgb.len() == THUMB * THUMB * 3).then(|| {
        rgb.chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect()
    })
}

/// The artwork disc is drawn from this many pixels a side, so it stays smooth
/// at twice its 40 px size.
pub const DISC: usize = 96;

/// The thumbnail as a round picture: smoothly enlarged, with an
/// anti-aliased edge. Colour runs right to the edge, so the soft rim takes
/// the picture's own colour and never a light or dark fringe.
pub fn disc(pixels: &[u8]) -> Option<Vec<u8>> {
    if pixels.len() != THUMB * THUMB * 4 {
        return None;
    }
    let scale = THUMB as f32 / DISC as f32;
    let centre = DISC as f32 / 2.;
    let sample = |x: usize, y: usize, c: usize| f32::from(pixels[(y * THUMB + x) * 4 + c]);
    let mut out = Vec::with_capacity(DISC * DISC * 4);
    for y in 0..DISC {
        let sy = ((y as f32 + 0.5) * scale - 0.5).clamp(0., (THUMB - 1) as f32);
        let (y0, fy) = (sy.floor() as usize, sy.fract());
        let y1 = (y0 + 1).min(THUMB - 1);
        for x in 0..DISC {
            let sx = ((x as f32 + 0.5) * scale - 0.5).clamp(0., (THUMB - 1) as f32);
            let (x0, fx) = (sx.floor() as usize, sx.fract());
            let x1 = (x0 + 1).min(THUMB - 1);
            for c in 0..3 {
                let top = sample(x0, y0, c) * (1. - fx) + sample(x1, y0, c) * fx;
                let bottom = sample(x0, y1, c) * (1. - fx) + sample(x1, y1, c) * fx;
                out.push((top * (1. - fy) + bottom * fy).round() as u8);
            }
            let distance =
                ((x as f32 + 0.5 - centre).powi(2) + (y as f32 + 0.5 - centre).powi(2)).sqrt();
            out.push(((centre - distance + 0.5).clamp(0., 1.) * 255.).round() as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn artwork_disc_is_round_with_a_soft_edge_in_the_picture_colour() {
        let pixels: Vec<u8> = (0..super::THUMB * super::THUMB)
            .flat_map(|_| [30, 200, 90, 255])
            .collect();
        let disc = super::disc(&pixels).expect("disc");
        let side = super::DISC;
        assert_eq!(disc.len(), side * side * 4);
        let at = |x: usize, y: usize| &disc[(y * side + x) * 4..(y * side + x) * 4 + 4];
        for (x, y) in [(0, 0), (side - 1, 0), (0, side - 1), (side - 1, side - 1)] {
            assert_eq!(at(x, y)[3], 0, "corner {x},{y} is clear");
        }
        assert_eq!(at(side / 2, side / 2), &[30, 200, 90, 255]);
        // Along the rim some pixels are partly covered, and every one keeps
        // the picture's colour rather than a fringe.
        let rim: Vec<&[u8]> = (0..side)
            .map(|y| at(side / 2 + (side as f32 / 2. * 0.7071) as usize, y))
            .filter(|p| p[3] > 0 && p[3] < 255)
            .collect();
        assert!(!rim.is_empty());
        assert!(rim.iter().all(|p| p[..3] == [30, 200, 90]));
        assert!(super::disc(&pixels[..8]).is_none());
    }
    #[test]
    fn thumbnails_round_trip_and_reject_malformed_input() {
        let (w, h) = (90u32, 60u32);
        let rgba: Vec<u8> = (0..w * h).flat_map(|_| [200, 40, 120, 255]).collect();
        let encoded = super::thumbnail(&rgba, w, h).expect("thumbnail");
        let pixels = super::thumbnail_pixels(&encoded).expect("pixels");
        assert_eq!(pixels.len(), super::THUMB * super::THUMB * 4);
        assert_eq!(&pixels[..4], &[200, 40, 120, 255]);
        assert!(super::thumbnail(&rgba[..10], w, h).is_none());
        assert!(super::thumbnail_pixels("not base64!").is_none());
        assert!(super::thumbnail_pixels("AAAA").is_none());
    }
    use super::*;
    fn picture(pixels: &[([u8; 3], usize)]) -> Vec<u8> {
        pixels
            .iter()
            .flat_map(|(c, n)| std::iter::repeat_n([c[0], c[1], c[2], 255], *n))
            .flatten()
            .collect()
    }
    #[test]
    fn tint_takes_the_most_common_vivid_hue_not_the_greys() {
        let art = picture(&[
            ([20, 20, 20], 3000),
            ([240, 240, 240], 2000),
            ([30, 90, 220], 600),
            ([220, 40, 40], 300),
        ]);
        let [r, g, b] = dominant(&art).unwrap();
        assert!(b > 200 && r < 60 && g < 120, "{r} {g} {b}");
    }
    #[test]
    fn tint_is_absent_for_greyscale_or_speckled_art() {
        assert_eq!(dominant(&picture(&[([128, 128, 128], 4096)])), None);
        assert_eq!(
            dominant(&picture(&[([128, 128, 128], 4000), ([255, 0, 0], 30)])),
            None
        );
        assert_eq!(dominant(&[]), None);
        assert_eq!(dominant(&[255, 0, 0, 0].repeat(64)), None, "transparent");
    }
}
