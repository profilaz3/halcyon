//! Depth Anything V2 Small on one picture, with tract (pure Rust). Runs only
//! in the scene worker child; see docs/future-builder/FB14.md for why tract.
use super::scene::Depth;
use image::RgbImage;

const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];
/// The model sees the picture with its short side at this many pixels.
/// Smaller was no worse on the Ridge and keeps a run under half a second.
pub const SHORT_SIDE: u32 = 308;
/// A very wide picture keeps at most this long side.
pub const LONG_SIDE: u32 = 1092;

/// The model's input size: the picture's shape, short side [`SHORT_SIDE`],
/// both sides multiples of 14 (the model's patch).
pub fn input_size(source: [u32; 2]) -> [u32; 2] {
    let short = source[0].min(source[1]).max(1) as f64;
    let k = f64::from(SHORT_SIDE) / short;
    let side = |v: u32| {
        let v = (f64::from(v) * k / 14.).round().max(1.) as u32 * 14;
        v.min(LONG_SIDE)
    };
    [side(source[0]), side(source[1])]
}

/// Planar, normalised RGB at the input size.
pub fn preprocess(picture: &RgbImage) -> ([u32; 2], Vec<f32>) {
    let size = input_size([picture.width(), picture.height()]);
    let small = image::imageops::resize(
        picture,
        size[0],
        size[1],
        image::imageops::FilterType::CatmullRom,
    );
    let n = (size[0] * size[1]) as usize;
    let mut planes = vec![0f32; 3 * n];
    for (i, p) in small.pixels().enumerate() {
        for c in 0..3 {
            planes[c * n + i] = (f32::from(p[c]) / 255. - MEAN[c]) / STD[c];
        }
    }
    (size, planes)
}

/// Loads the model from its verified bytes and runs it once.
pub fn run(model: &[u8], picture: &RgbImage, threads: usize) -> Result<Depth, String> {
    use tract_onnx::prelude::*;
    if threads > 1 {
        tract_onnx::prelude::multithread::set_default_executor(
            tract_onnx::prelude::multithread::Executor::multithread(threads),
        );
    }
    let (size, input) = preprocess(picture);
    let (w, h) = (size[0] as usize, size[1] as usize);
    let plan = tract_onnx::onnx()
        .with_ignore_output_shapes(true)
        .with_ignore_value_info(true)
        .model_for_read(&mut std::io::Cursor::new(model))
        .and_then(|m| m.with_input_fact(0, f32::fact([1, 3, h, w]).into()))
        .and_then(|m| m.into_optimized())
        .and_then(|m| m.into_runnable())
        .map_err(|e| format!("model: {e}"))?;
    let tensor: Tensor = tract_ndarray::Array4::from_shape_vec((1, 3, h, w), input)
        .map_err(|e| e.to_string())?
        .into();
    let out = plan
        .run(tvec!(tensor.into()))
        .map_err(|e| format!("run: {e}"))?;
    let view = out
        .first()
        .ok_or("no output")?
        .to_plain_array_view::<f32>()
        .map_err(|e| e.to_string())?;
    let shape = view.shape().to_vec();
    let (oh, ow) = match shape.as_slice() {
        [.., oh, ow] => (*oh, *ow),
        _ => return Err("output shape".into()),
    };
    let values: Vec<f32> = view.iter().copied().collect();
    if values.len() != oh * ow || values.iter().any(|v| !v.is_finite()) {
        return Err("output values".into());
    }
    Ok(Depth {
        width: ow as u32,
        height: oh as u32,
        values,
    })
}
