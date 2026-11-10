//! Synthetic scenes: a sky over a hill whose top is at a known height, and
//! hand-made masks for the checks the spike missed.
use super::*;
use crate::depth::Frame;
use crate::desktop::wallpaper::ScalingMode;

/// A 960×540 picture shown on a 1920×1080 screen: one mask pixel is two
/// screen pixels.
const SOURCE: [u32; 2] = [960, 540];
const SCREEN: [f32; 2] = [1920., 1080.];

/// Sky above `horizon` (picture rows), a darker near hill below it.
fn hill(horizon: u32) -> (Depth, RgbImage) {
    let picture = RgbImage::from_fn(SOURCE[0], SOURCE[1], |_, y| {
        if y < horizon {
            image::Rgb([150, 190, 235])
        } else {
            image::Rgb([40, 70, 45])
        }
    });
    // The model's own output is smaller than the picture.
    let (w, h) = (546u32, 308u32);
    let line = horizon as f32 * h as f32 / SOURCE[1] as f32;
    let values = (0..h)
        .flat_map(|y| {
            (0..w).map(move |_| {
                if (y as f32) < line {
                    1.0
                } else {
                    6.0 + (y as f32 - line) * 0.01
                }
            })
        })
        .collect();
    (
        Depth {
            width: w,
            height: h,
            values,
        },
        picture,
    )
}

fn frame(origin: [f32; 2]) -> Frame {
    Frame {
        screen: SCREEN,
        origin,
        size: [800., 330.],
        scale: 1.,
    }
}

/// Four solid digit blocks, ink rows 60 to 250 of an 800×330 widget.
fn glyph_alpha() -> Vec<u8> {
    let mut alpha = vec![0u8; 800 * 330];
    for y in 60..250 {
        for (a, b) in [(100, 230), (260, 390), (420, 550), (570, 700)] {
            for x in a..b {
                alpha[y * 800 + x] = 255;
            }
        }
    }
    alpha
}

fn glyph<'a>(alpha: &'a [u8], frame: &'a Frame) -> Glyph<'a> {
    Glyph {
        alpha,
        width: 800,
        height: 330,
        frame,
        face: [40., 260.],
    }
}

fn judged(scene: &Scene, origin: [f32; 2]) -> Judgement {
    let alpha = glyph_alpha();
    let frame = frame(origin);
    judge(
        &scene.stats,
        &scene.mask,
        &scene.edges,
        &glyph(&alpha, &frame),
        &ScalingMode::Zoom,
    )
    .unwrap()
}

#[test]
fn own_wallpaper_mask_follows_the_hill_line() {
    let (depth, picture) = hill(300);
    let scene = derive(&depth, &picture).unwrap();
    assert_eq!(scene.mask.dimensions(), (960, 540));
    assert_eq!(scene.picture.dimensions(), (960, 540));
    assert_eq!(scene.stats.source, SOURCE);
    for x in [10, 480, 950] {
        assert!(
            scene.mask.get_pixel(x, 290)[0] < 32,
            "sky stays behind at {x}"
        );
        assert!(
            scene.mask.get_pixel(x, 310)[0] > 223,
            "hill is in front at {x}"
        );
    }
    assert!(
        (scene.stats.coverage - 240. / 540.).abs() < 0.02,
        "{}",
        scene.stats.coverage
    );
    assert!(scene.stats.eta > 0.8, "{}", scene.stats.eta);
    assert_eq!(scene.stats.top_touch, 0.);
    assert!(!scene.stats.flat);
    // The edges mark the hill line, not the flat sky.
    assert_eq!(scene.edges.get_pixel(480, 300)[0], 255);
    assert_eq!(scene.edges.get_pixel(480, 100)[0], 0);
}

#[test]
fn own_wallpaper_work_size_scales_down_only() {
    assert_eq!(work_size([960, 540]), [960, 540]);
    assert_eq!(work_size([3840, 2160]), [1920, 1080]);
    assert_eq!(work_size([1672, 941]), [1672, 941]);
    let tall = work_size([3000, 6000]);
    assert!((f64::from(tall[0]) * f64::from(tall[1]) - WORK_AREA).abs() / WORK_AREA < 0.01);
}

#[test]
fn own_wallpaper_flat_and_full_pictures_are_not_useful() {
    let picture = RgbImage::from_pixel(SOURCE[0], SOURCE[1], image::Rgb([90, 120, 160]));
    let depth = Depth {
        width: 64,
        height: 36,
        values: vec![3.; 64 * 36],
    };
    let scene = derive(&depth, &picture).unwrap();
    assert!(scene.stats.flat);
    let j = judged(&scene, [560., 380.]);
    assert_eq!(j.verdict, Verdict::NotUseful);
    assert_eq!(j.reason, Some(Reason::NoForeground));
    assert_eq!(
        j.reason.unwrap().sentence(),
        "This picture has no clear foreground"
    );

    let (depth, picture) = hill(300);
    let mut scene = derive(&depth, &picture).unwrap();
    scene.stats.coverage = 0.9;
    assert_eq!(
        judged(&scene, [560., 380.]).reason,
        Some(Reason::FillsPicture)
    );
}

#[test]
fn own_wallpaper_works_where_the_hill_covers_the_bottom_of_the_time() {
    // Hill line at screen row 600; ink 440 to 630, so its bottom third is behind.
    let (depth, picture) = hill(300);
    let scene = derive(&depth, &picture).unwrap();
    let j = judged(&scene, [560., 380.]);
    assert_eq!(j.verdict, Verdict::Works, "{j:?}");
    assert!(j.confidence >= 0.75, "{j:?}");
    assert_eq!(j.suggested, None);
    assert!(j.measures.hidden_top <= MAX_HIDDEN_TOP);
    assert!(j.measures.hidden_bottom > 0.3);
    assert_eq!(j.measures.ground, 1.);
    assert_eq!(j.measures.moved, 0.);
}

#[test]
fn own_wallpaper_suggests_a_nearby_spot_and_moving_costs_no_confidence() {
    let (depth, picture) = hill(300);
    let scene = derive(&depth, &picture).unwrap();
    let here = judged(&scene, [560., 380.]);
    // High in the sky: nothing covers it, but lower down would.
    let j = judged(&scene, [560., 100.]);
    assert_eq!(j.verdict, Verdict::NeedsNudge, "{j:?}");
    let spot = j.suggested.unwrap();
    assert_eq!(spot[0], 560., "keeps its column");
    assert!(spot[1] > 300. && spot[1] < 450., "{spot:?}");
    assert!(j.measures.moved > 0.2);
    // The same confidence the clock would have there: moving is the nudge.
    let there = judged(&scene, spot);
    assert_eq!(there.verdict, Verdict::Works, "{there:?}");
    assert!(
        (j.confidence - here.confidence).abs() < 0.11,
        "{} {}",
        j.confidence,
        here.confidence
    );
    // Too low: the hill hides the time, so it moves up.
    let low = judged(&scene, [560., 700.]);
    assert_eq!(low.verdict, Verdict::NeedsNudge);
    assert!(low.suggested.unwrap()[1] < 450.);
}

/// A mask in front of everything below `horizon` screen rows, plus `extra`
/// rectangles in screen pixels.
fn mask_with(horizon: Option<u32>, extra: &[[u32; 4]]) -> GrayImage {
    GrayImage::from_fn(SOURCE[0], SOURCE[1], |x, y| {
        let (sx, sy) = (x * 2, y * 2);
        let ground = horizon.is_some_and(|h| sy >= h);
        let block = extra
            .iter()
            .any(|r| sx >= r[0] && sx < r[2] && sy >= r[1] && sy < r[3]);
        image::Luma([if ground || block { 255 } else { 0 }])
    })
}

fn measures(mask: &GrayImage, origin: [f32; 2]) -> Measures {
    let alpha = glyph_alpha();
    let frame = frame(origin);
    let g = glyph(&alpha, &frame);
    let ink = ink(&g).unwrap();
    let map = Map::new(
        &frame,
        &ScalingMode::Zoom,
        SOURCE,
        [mask.width(), mask.height()],
    )
    .unwrap();
    cover(&ink, &map, mask, origin.map(f64::from))
}

#[test]
fn own_wallpaper_a_subject_cutting_into_the_digits_is_caught() {
    // Ink is at screen 660..1260 by 440..630. A thin tower rises into one
    // digit: little of the top ink overall, most of one stretch of it.
    let tower = mask_with(Some(600), &[[872, 446, 886, 600]]);
    let m = measures(&tower, [560., 380.]);
    assert!(m.hidden_top <= MAX_HIDDEN_TOP, "{m:?}");
    assert!(m.cut > MAX_CUT, "{m:?}");
    assert!(!passes(&m));
    let clean = measures(&mask_with(Some(600), &[]), [560., 380.]);
    assert!(clean.cut < 0.05 && passes(&clean), "{clean:?}");
}

#[test]
fn own_wallpaper_a_floating_shape_is_not_ground() {
    // A band under the digits with sky below it: the time would sit on air.
    let band = mask_with(None, &[[600, 590, 1320, 640]]);
    let m = measures(&band, [560., 380.]);
    assert!(
        m.hidden_bottom >= MIN_HIDDEN_BOTTOM && m.hidden_top <= MAX_HIDDEN_TOP,
        "{m:?}"
    );
    assert!(m.ground < MIN_GROUND, "{m:?}");
    assert!(!passes(&m));
}

#[test]
fn own_wallpaper_no_low_foreground_has_no_spot() {
    // Only a foreground strip at the very bottom of the screen.
    let (depth, picture) = hill(520);
    let scene = derive(&depth, &picture).unwrap();
    let j = judged(&scene, [560., 380.]);
    assert_eq!(j.verdict, Verdict::NotUseful, "{j:?}");
    assert_eq!(j.reason, Some(Reason::NoSpot));
}

#[test]
fn own_wallpaper_otsu_and_percentiles_match_numpy() {
    let values: Vec<f32> = (0..1000).map(|i| if i < 600 { 0.2 } else { 0.8 }).collect();
    let (t, eta) = otsu(&values);
    assert!(t > 0.2 && t < 0.8, "{t}");
    assert!(eta > 0.99, "{eta}");
    // numpy.percentile([1, 2, 3, 4], [25, 50]) == [1.75, 2.5]
    assert_eq!(percentiles(&[4., 1., 3., 2.], &[25., 50.]), vec![1.75, 2.5]);
}

#[test]
fn own_wallpaper_a_ragged_outline_is_uneven() {
    let stats = Stats {
        source: SOURCE,
        eta: 0.7,
        coverage: 0.5,
        top_touch: 0.,
        flat: false,
    };
    let clean = Measures {
        hidden_bottom: 0.4,
        align: 0.8,
        ragged: 2.,
        ..Measures::default()
    };
    assert_eq!(confidence(&stats, &clean), 1.);
    let busy = Measures {
        ragged: 6.8,
        ..clean.clone()
    };
    assert!(confidence(&stats, &busy) >= WORKS);
    let figure = Measures {
        ragged: 9.1,
        ..clean.clone()
    };
    assert_eq!(confidence(&stats, &figure), 0.);
}
