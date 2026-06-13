//! Layout detection (PRD §4.D). The default model is `docling-layout-heron`
//! (RT-DETRv2, MIT code / Apache-2.0 weights, official ONNX, NMS-free).
//!
//! The label set → [`RegionClass`] mapping and the detection→region conversion
//! are pure and unit-tested. The ONNX session wrapper ([`LayoutEngine`]) is
//! gated behind the `ml` feature; its `run_raw` returns each model output's
//! name/shape/data so the (model-specific) decode can be wired once the output
//! tensor layout is pinned in the E2 bake-off.

use crate::fusion::{Region, RegionClass};
use crate::ml::preprocess::Detection;
use dongler_core::ir::BBox;

/// docling-layout-heron class labels (RT-DETRv2). Order is confirmed against the
/// model card in E2; the mapping below is order-independent (keyed by name).
pub const HERON_LABELS: &[&str] = &[
    "caption",
    "footnote",
    "formula",
    "list_item",
    "page_footer",
    "page_header",
    "picture",
    "section_header",
    "table",
    "text",
    "title",
];

/// Map a layout label to a fusion priority class.
pub fn class_for_label(label: &str) -> RegionClass {
    match label {
        "table" => RegionClass::Table,
        "picture" => RegionClass::Figure,
        "formula" => RegionClass::Formula,
        "caption" | "footnote" | "list_item" | "page_footer" | "page_header"
        | "section_header" | "text" | "title" => RegionClass::Text,
        _ => RegionClass::Other,
    }
}

/// Convert model detections (in resized-input pixels) into fusion [`Region`]s in
/// the original image's pixel space, using `scale_x`/`scale_y` from preprocessing.
pub fn detections_to_regions(
    detections: &[Detection],
    labels: &[&str],
    scale_x: f32,
    scale_y: f32,
) -> Vec<Region> {
    detections
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let label = labels.get(d.class_id).copied().unwrap_or("");
            Region {
                id: i,
                class: class_for_label(label),
                bbox: BBox {
                    x: d.x * scale_x,
                    y: d.y * scale_y,
                    width: d.w * scale_x,
                    height: d.h * scale_y,
                },
            }
        })
        .collect()
}

#[cfg(feature = "ml")]
mod engine {
    use super::*;
    use crate::ml::preprocess::to_nchw_tensor;
    use crate::ml::MlError;
    use image::RgbImage;
    use ort::session::Session;

    /// A loaded layout-detection ONNX model.
    pub struct LayoutEngine {
        session: Session,
        /// Square input size the model expects (e.g. 640).
        pub input_size: u32,
        /// Class labels, indexed by the model's class ids.
        pub labels: Vec<String>,
    }

    impl LayoutEngine {
        /// Load an ONNX model from disk. The ONNX Runtime dylib is resolved by
        /// `ort` (bundled or via `ORT_DYLIB_PATH`).
        pub fn from_onnx_path(
            path: impl AsRef<std::path::Path>,
            input_size: u32,
        ) -> Result<Self, MlError> {
            let session = Session::builder()?.commit_from_file(path)?;
            Ok(Self {
                session,
                input_size,
                labels: HERON_LABELS.iter().map(|s| s.to_string()).collect(),
            })
        }

        /// Preprocess `img` and run the model, returning each output's
        /// `(name, shape, data)`. This is model-agnostic; the RT-DETR decode
        /// (via [`super::super::preprocess::decode_detections`]) is composed on
        /// top once E2 pins which outputs carry boxes vs scores.
        pub fn run_raw(
            &mut self,
            img: &RgbImage,
        ) -> Result<Vec<(String, Vec<i64>, Vec<f32>)>, MlError> {
            let (tensor, _scale) = to_nchw_tensor(img, self.input_size);
            let input = ort::value::Value::from_array(tensor)?;
            let outputs = self.session.run(ort::inputs![input])?;

            let mut result = Vec::new();
            for (name, value) in outputs.iter() {
                if let Ok((shape, data)) = value.try_extract_tensor::<f32>() {
                    result.push((
                        name.to_string(),
                        shape.iter().map(|&d| d as i64).collect(),
                        data.to_vec(),
                    ));
                }
            }
            if result.is_empty() {
                return Err(MlError::NoOutput);
            }
            Ok(result)
        }
    }
}

#[cfg(feature = "ml")]
pub use engine::LayoutEngine;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_mapping_priorities() {
        assert_eq!(class_for_label("table"), RegionClass::Table);
        assert_eq!(class_for_label("picture"), RegionClass::Figure);
        assert_eq!(class_for_label("formula"), RegionClass::Formula);
        assert_eq!(class_for_label("text"), RegionClass::Text);
        assert_eq!(class_for_label("section_header"), RegionClass::Text);
        assert_eq!(class_for_label("???"), RegionClass::Other);
    }

    #[test]
    fn detections_scale_back_to_original_pixels() {
        let dets = vec![Detection {
            class_id: 8, // "table"
            score: 0.9,
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
        }];
        let regions = detections_to_regions(&dets, HERON_LABELS, 2.0, 3.0);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].class, RegionClass::Table);
        assert_eq!(regions[0].bbox.x, 20.0);
        assert_eq!(regions[0].bbox.y, 60.0);
        assert_eq!(regions[0].bbox.width, 60.0);
        assert_eq!(regions[0].bbox.height, 120.0);
    }
}
