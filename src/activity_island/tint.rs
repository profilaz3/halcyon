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

#[cfg(test)]
mod tests {
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
