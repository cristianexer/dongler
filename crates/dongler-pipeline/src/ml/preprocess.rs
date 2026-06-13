//! Image preprocessing for ONNX detection models: RGB image → normalized NCHW
//! float tensor. The exact normalization (mean/std vs plain /255) and input
//! size are model-specific and pinned in the E2 bake-off; this provides the
//! standard RT-DETR default (resize to a fixed square, scale to 0..1).

use image::RgbImage;
use ndarray::Array4;

/// Detection decoded from a model's `(boxes, scores)` output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    /// Index into the model's class list.
    pub class_id: usize,
    pub score: f32,
    /// Box in input-image pixel coordinates (x, y, w, h), top-left origin.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Resize an RGB image to `size`×`size` and pack into an NCHW `[1, 3, size, size]`
/// f32 tensor with values scaled to `0.0..=1.0`. Returns the tensor and the
/// `(scale_x, scale_y)` used so detections can be mapped back to the original.
pub fn to_nchw_tensor(img: &RgbImage, size: u32) -> (Array4<f32>, (f32, f32)) {
    let (orig_w, orig_h) = (img.width().max(1), img.height().max(1));
    let resized = image::imageops::resize(img, size, size, image::imageops::FilterType::Triangle);
    let size_us = size as usize;
    let tensor = Array4::from_shape_fn((1, 3, size_us, size_us), |(_, c, y, x)| {
        let px = resized.get_pixel(x as u32, y as u32);
        px[c] as f32 / 255.0
    });
    let scale_x = orig_w as f32 / size as f32;
    let scale_y = orig_h as f32 / size as f32;
    (tensor, (scale_x, scale_y))
}

/// Decode RT-DETR-style outputs: `boxes` as `[N, 4]` normalized `cxcywh` in
/// `0..1` and `scores` as `[N, num_classes]`. Keeps detections whose top class
/// score ≥ `threshold`, mapping boxes into pixel coordinates of an
/// `input_w`×`input_h` image. (The model's own output layout is confirmed in
/// E2; this is the conventional decode.)
pub fn decode_detections(
    boxes_cxcywh: &[[f32; 4]],
    scores: &[Vec<f32>],
    threshold: f32,
    input_w: f32,
    input_h: f32,
) -> Vec<Detection> {
    let mut out = Vec::new();
    for (b, s) in boxes_cxcywh.iter().zip(scores.iter()) {
        let Some((class_id, &score)) = s
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        else {
            continue;
        };
        if score < threshold {
            continue;
        }
        let (cx, cy, w, h) = (b[0], b[1], b[2], b[3]);
        out.push(Detection {
            class_id,
            score,
            x: (cx - w / 2.0) * input_w,
            y: (cy - h / 2.0) * input_h,
            w: w * input_w,
            h: h * input_h,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn solid(w: u32, h: u32, color: [u8; 3]) -> RgbImage {
        RgbImage::from_pixel(w, h, Rgb(color))
    }

    #[test]
    fn tensor_has_correct_shape_and_range() {
        let img = solid(40, 20, [255, 128, 0]);
        let (t, (sx, sy)) = to_nchw_tensor(&img, 16);
        assert_eq!(t.shape(), &[1, 3, 16, 16]);
        // all values in [0,1]
        assert!(t.iter().all(|&v| (0.0..=1.0).contains(&v)));
        // red channel ~1.0, blue ~0.0
        assert!((t[[0, 0, 0, 0]] - 1.0).abs() < 1e-6);
        assert!(t[[0, 2, 0, 0]].abs() < 1e-6);
        assert!((sx - 40.0 / 16.0).abs() < 1e-6);
        assert!((sy - 20.0 / 16.0).abs() < 1e-6);
    }

    #[test]
    fn decode_filters_by_threshold_and_picks_argmax_class() {
        let boxes = vec![[0.5, 0.5, 0.5, 0.5], [0.1, 0.1, 0.1, 0.1]];
        let scores = vec![vec![0.1, 0.9], vec![0.2, 0.05]];
        let dets = decode_detections(&boxes, &scores, 0.5, 100.0, 100.0);
        assert_eq!(dets.len(), 1);
        let d = dets[0];
        assert_eq!(d.class_id, 1);
        assert!((d.score - 0.9).abs() < 1e-6);
        // cxcywh (0.5,0.5,0.5,0.5) on 100px -> x=25,y=25,w=50,h=50
        assert!((d.x - 25.0).abs() < 1e-4);
        assert!((d.w - 50.0).abs() < 1e-4);
    }

    #[test]
    fn decode_empty_when_all_below_threshold() {
        let boxes = vec![[0.5, 0.5, 0.2, 0.2]];
        let scores = vec![vec![0.3, 0.1]];
        assert!(decode_detections(&boxes, &scores, 0.5, 10.0, 10.0).is_empty());
    }
}
