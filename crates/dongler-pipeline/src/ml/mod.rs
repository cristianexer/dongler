//! ML stages (PRD §4.D–F): layout detection, OCR, and table structure via ONNX
//! Runtime (`ort`), plus PDF rasterization via `pdfium`. **Compiled only with the
//! `ml` cargo feature** so the default build and the WASM/fast path pull in no
//! native ONNX Runtime / pdfium dependency.
//!
//! Status: the deterministic, model-independent pieces — image preprocessing
//! into an NCHW tensor, detection decoding, and the label→region mapping — are
//! implemented and unit-tested. The model session wrapper
//! ([`layout::LayoutEngine`]) loads and runs an ONNX graph; the exact
//! input/output tensor names and normalization constants are model-specific and
//! are pinned during the E2 layout bake-off (see `experiments/` and the
//! registry).

pub mod layout;
pub mod preprocess;
pub mod raster;

/// Errors from the ML stages.
#[derive(Debug, thiserror::Error)]
pub enum MlError {
    #[error("onnx runtime error: {0}")]
    Ort(#[from] ort::Error),
    #[error("pdfium error: {0}")]
    Pdfium(#[from] pdfium_render::prelude::PdfiumError),
    #[error("model produced no usable output")]
    NoOutput,
}
