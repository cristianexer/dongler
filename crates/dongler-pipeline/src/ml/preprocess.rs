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

/// Aspect-preserving resize metadata, recording how a source image was mapped
/// into a square model input so model-space coordinates can be mapped back.
/// The source is scaled by `scale` (longest side → `target`) and padded to the
/// square with `pad_x`/`pad_y` (in model-input pixels). SLANet pads at the
/// top-left (`pad_x = pad_y = 0`), confirmed against the model's `inference.yml`
/// (`PaddingTableImage`, `ResizeTableImage max_len: 488`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResizeMeta {
    pub scale: f32,
    pub pad_x: f32,
    pub pad_y: f32,
    pub target: u32,
}

impl ResizeMeta {
    /// Map a model-input pixel coordinate back to source-image pixels: undo the
    /// letterbox pad, then the scale.
    pub fn input_to_src(&self, x: f32, y: f32) -> (f32, f32) {
        (
            (x - self.pad_x) / self.scale,
            (y - self.pad_y) / self.scale,
        )
    }

    /// Map a model-input pixel box `(x, y, w, h)` back to a source-image box.
    pub fn input_box_to_src(&self, x: f32, y: f32, w: f32, h: f32) -> (f32, f32, f32, f32) {
        let (sx, sy) = self.input_to_src(x, y);
        (sx, sy, w / self.scale, h / self.scale)
    }
}

/// Resize `img` aspect-preserving so the longest side is `target`, pad to a
/// `target`×`target` square at the **top-left** (content origin `(0,0)`),
/// normalize with per-channel `mean`/`std` (values scaled to `0..1` first), and
/// pack into an NCHW `[1, 3, target, target]` tensor. When `bgr` is set the
/// channel order is flipped (model channel 0 ← blue) to match a model trained on
/// OpenCV BGR input. Returns the tensor and a [`ResizeMeta`] for inverse mapping.
///
/// This mirrors SLANet's documented `inference.yml` pipeline exactly:
/// `DecodeImage(BGR)` → `ResizeTableImage(max_len=488)` → `NormalizeImage(mean,
/// std, scale=1/255)` → `PaddingTableImage(488)` (top-left) → `ToCHWImage`.
pub fn to_nchw_normalized(
    img: &RgbImage,
    target: u32,
    mean: [f32; 3],
    std: [f32; 3],
    bgr: bool,
) -> (Array4<f32>, ResizeMeta) {
    let target = target.max(1);
    let (ow, oh) = (img.width().max(1), img.height().max(1));
    let scale = (target as f32 / ow as f32).min(target as f32 / oh as f32);
    let nw = ((ow as f32 * scale).round() as u32).clamp(1, target);
    let nh = ((oh as f32 * scale).round() as u32).clamp(1, target);
    let resized = image::imageops::resize(img, nw, nh, image::imageops::FilterType::Triangle);

    let t = target as usize;
    let tensor = Array4::from_shape_fn((1, 3, t, t), |(_, c, y, x)| {
        let (xi, yi) = (x as u32, y as u32);
        // Top-left placement: content occupies [0,nw)×[0,nh); the rest is zero pad.
        if xi >= nw || yi >= nh {
            return (0.0 - mean[c]) / std[c];
        }
        // For a BGR model, channel c reads RGB index (2 - c): channel 0 ← blue.
        let src = if bgr { 2 - c } else { c };
        let raw = resized.get_pixel(xi, yi)[src] as f32 / 255.0;
        (raw - mean[c]) / std[c]
    });

    (
        tensor,
        ResizeMeta {
            scale,
            pad_x: 0.0,
            pad_y: 0.0,
            target,
        },
    )
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

    #[test]
    fn normalized_tensor_shape_and_top_left_padding() {
        // 40x20 image into a 100x100 square: scale = 2.5, nw=100, nh=50. SLANet
        // pads at the top-left, so content occupies rows [0,50) and pad_x=pad_y=0.
        let img = solid(40, 20, [255, 255, 255]);
        let (t, meta) = to_nchw_normalized(&img, 100, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0], false);
        assert_eq!(t.shape(), &[1, 3, 100, 100]);
        assert!((meta.scale - 2.5).abs() < 1e-6);
        assert_eq!(meta.pad_x, 0.0);
        assert_eq!(meta.pad_y, 0.0);
        // White content normalizes to 1.0; the bottom pad band is 0.
        assert!((t[[0, 0, 10, 10]] - 1.0).abs() < 1e-6); // inside (y=10 < 50)
        assert!(t[[0, 0, 60, 10]].abs() < 1e-6); // pad band (y=60 >= 50)
    }

    #[test]
    fn bgr_flips_channel_order() {
        // Pure-red RGB pixel. With bgr=true, model channel 0 (blue slot) reads the
        // image's blue (0.0) and channel 2 reads red (1.0).
        let img = solid(8, 8, [255, 0, 0]);
        let (t, _) = to_nchw_normalized(&img, 8, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0], true);
        assert!(t[[0, 0, 0, 0]].abs() < 1e-6); // channel 0 ← blue = 0
        assert!((t[[0, 2, 0, 0]] - 1.0).abs() < 1e-6); // channel 2 ← red = 1
    }

    #[test]
    fn resize_meta_inverse_round_trips_a_box() {
        let meta = ResizeMeta {
            scale: 2.5,
            pad_x: 0.0,
            pad_y: 25.0,
            target: 100,
        };
        // A model-input box at (10, 35) size (50, 25) → source (4, 4) size (20, 10).
        let (x, y, w, h) = meta.input_box_to_src(10.0, 35.0, 50.0, 25.0);
        assert!((x - 4.0).abs() < 1e-6);
        assert!((y - 4.0).abs() < 1e-6);
        assert!((w - 20.0).abs() < 1e-6);
        assert!((h - 10.0).abs() < 1e-6);
    }
}
