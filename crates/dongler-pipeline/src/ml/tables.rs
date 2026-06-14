//! Table-structure inference (PRD §4.F): the SLANet-plus ONNX session that
//! produces the raw structure/bbox tensors, decoded by the pure
//! [`crate::table_structure`] module. Behind the `ml` feature.
//!
//! The engine returns cell boxes in **crop pixel space** (top-left origin); the
//! orchestrator maps them to PDF user space via
//! [`crate::ml::raster::RegionTransform`]. The model emits topology + geometry
//! only — cell text is snapped from the text layer in [`crate::table_fusion`].

use crate::ml::preprocess::to_nchw_normalized;
use crate::ml::MlError;
use crate::table_structure::{decode_slanet, slanet_char_dict, TableStructure};
use image::RgbImage;
use ort::session::Session;
use std::path::Path;

/// ImageNet mean/std (applied to BGR channels in order), from the model's
/// `inference.yml` `NormalizeImage`.
pub const SLANET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
pub const SLANET_STD: [f32; 3] = [0.229, 0.224, 0.225];
/// SLANet square input side (`ResizeTableImage max_len: 488`).
pub const SLANET_INPUT: u32 = 488;

/// A loaded SLANet table-structure model.
pub struct TableEngine {
    session: Session,
    input_size: u32,
    mean: [f32; 3],
    std: [f32; 3],
    /// Structure-token vocabulary, indexed by class id (`sos` + 28 tokens + `eos`).
    char_dict: Vec<String>,
}

impl TableEngine {
    /// Load the SLANet ONNX model. The structure-token vocabulary is the fixed
    /// PaddleOCR SLANet dictionary ([`slanet_char_dict`]); no companion file.
    pub fn from_onnx_path(
        model_path: impl AsRef<Path>,
        input_size: u32,
    ) -> Result<Self, MlError> {
        let session = Session::builder()?.commit_from_file(model_path)?;
        Ok(Self {
            session,
            input_size,
            mean: SLANET_MEAN,
            std: SLANET_STD,
            char_dict: slanet_char_dict(),
        })
    }

    /// Run table-structure recognition on a region crop. Returns the decoded grid
    /// with cell boxes mapped back to **crop pixel space**.
    pub fn run(&mut self, crop: &RgbImage) -> Result<TableStructure, MlError> {
        // SLANet expects BGR (its `DecodeImage` uses `img_mode: BGR`).
        let (tensor, _meta) =
            to_nchw_normalized(crop, self.input_size, self.mean, self.std, true);
        let input = ort::value::Value::from_array(tensor)?;
        let outputs = self.session.run(ort::inputs![input])?;

        // Collect f32 outputs as (shape, data). SLANet emits two: structure logits
        // [1, T, 30] and per-step bbox [1, T, 4], in the order [bbox, structure].
        // We identify them by trailing dimension so output order is irrelevant.
        let mut tensors: Vec<(Vec<i64>, Vec<f32>)> = Vec::new();
        for (_name, value) in outputs.iter() {
            if let Ok((shape, data)) = value.try_extract_tensor::<f32>() {
                tensors.push((shape.iter().copied().collect(), data.to_vec()));
            }
        }
        let (struct_shape, struct_data, bbox_shape, bbox_data) = pick_outputs(&tensors)?;

        // SLANet's bbox head outputs `xyxy` normalized to the original crop
        // (PaddleOCR / rapid_table convention), so scaling by the crop's own
        // pixel dimensions yields boxes directly in crop pixel space — no
        // resize-inverse needed.
        let (cw, ch) = (crop.width() as f32, crop.height() as f32);
        Ok(decode_slanet(
            struct_data,
            struct_shape,
            bbox_data,
            bbox_shape,
            &self.char_dict,
            cw,
            ch,
        ))
    }
}

/// `(structure_shape, structure_data, bbox_shape, bbox_data)` borrowed from the
/// session outputs.
type PickedOutputs<'a> = (&'a [i64], &'a [f32], &'a [i64], &'a [f32]);

/// Identify which output is the structure logits (largest trailing dim) and which
/// is the bbox head (trailing dim 4 or 8). Returns borrowed slices.
fn pick_outputs(tensors: &[(Vec<i64>, Vec<f32>)]) -> Result<PickedOutputs<'_>, MlError> {
    let last = |s: &[i64]| s.last().copied().unwrap_or(0);
    let bbox = tensors
        .iter()
        .find(|(s, _)| matches!(last(s), 4 | 8))
        .ok_or(MlError::NoOutput)?;
    let structure = tensors
        .iter()
        .filter(|(s, _)| !matches!(last(s), 4 | 8))
        .max_by_key(|(s, _)| last(s))
        .ok_or(MlError::NoOutput)?;
    Ok((&structure.0, &structure.1, &bbox.0, &bbox.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_structure_and_bbox_by_trailing_dim() {
        let tensors = vec![
            (vec![1, 5, 4], vec![0.0; 20]),   // bbox
            (vec![1, 5, 30], vec![0.0; 150]), // structure (vocab 30)
        ];
        let (ss, _sd, bs, _bd) = pick_outputs(&tensors).unwrap();
        assert_eq!(ss.last(), Some(&30));
        assert_eq!(bs.last(), Some(&4));
    }

    #[test]
    fn errors_when_no_bbox_output() {
        let tensors = vec![(vec![1, 5, 30], vec![0.0; 150])];
        assert!(pick_outputs(&tensors).is_err());
    }
}
