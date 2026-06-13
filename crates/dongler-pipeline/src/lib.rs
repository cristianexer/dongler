//! `dongler-pipeline` — the hybrid PDF-to-Markdown extraction pipeline (PRD §4).
//!
//! This crate orchestrates the stages on top of the salvaged `dongler-core`
//! parser and IR. The **default build is fully deterministic and model-free**:
//! triage → text layer → reading order (XY-Cut++) → IR v2 → render. The ML
//! stages (layout/OCR/table inference, PDF rasterization) live behind the `ml`
//! cargo feature so the default build pulls in no native ONNX Runtime / pdfium
//! dependency and the WASM/fast path is unaffected.
//!
//! ```no_run
//! use dongler_pipeline::Pipeline;
//! let pipeline = Pipeline::new();
//! let md = pipeline.convert_to_markdown(b"%PDF-1.4 ...", "doc.pdf").unwrap();
//! ```

pub mod fusion;
pub mod geometry;
pub mod order;
pub mod registry;
pub mod triage;

mod textprovider;
pub use textprovider::{DonglerCoreProvider, TextProvider};

#[cfg(feature = "ml")]
pub mod ml;

use dongler_core::ir::{Block, Provenance, TextSource};
use dongler_core::render::{JsonRenderer, MarkdownRenderer, Renderer};
use dongler_core::{BBox, Document, ExtractOptions, Result};

/// Default radius (in PDF points) within which an orphan span attaches to the
/// nearest region during fusion.
const DEFAULT_ATTACH_RADIUS: f32 = 12.0;

/// The extraction pipeline. Holds the configured text provider.
pub struct Pipeline {
    text_provider: Box<dyn TextProvider>,
    options: ExtractOptions,
    /// Orphan-attach radius used by the fusion stage (PRD §4.G).
    pub attach_radius: f32,
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl Pipeline {
    /// A pipeline using the default `dongler-core` text provider.
    pub fn new() -> Self {
        Self {
            text_provider: Box::new(DonglerCoreProvider),
            options: ExtractOptions::default(),
            attach_radius: DEFAULT_ATTACH_RADIUS,
        }
    }

    /// Use a custom text provider (e.g. a pdfium-backed fallback).
    pub fn with_text_provider(mut self, provider: Box<dyn TextProvider>) -> Self {
        self.text_provider = provider;
        self
    }

    /// Override extraction options.
    pub fn with_options(mut self, options: ExtractOptions) -> Self {
        self.options = options;
        self
    }

    /// Name of the active text provider.
    pub fn text_provider_name(&self) -> &'static str {
        self.text_provider.name()
    }

    /// Run the deterministic pipeline over document bytes, producing IR v2:
    /// extract the text layer, triage each page, reorder blocks into reading
    /// order, and stamp provenance.
    pub fn convert_bytes(&self, bytes: &[u8], filename: &str) -> Result<Document> {
        let mut document = self.text_provider.extract(bytes, filename, &self.options)?;
        let detector = self.text_provider.name().to_owned();

        for page in &mut document.pages {
            page.route = Some(triage::classify_page(page));
            reorder_reading_order(page);
            stamp_text_layer_provenance(page, &detector);
        }

        Ok(document)
    }

    /// Convert to Markdown (with embedded HTML tables, the PRD default).
    pub fn convert_to_markdown(&self, bytes: &[u8], filename: &str) -> Result<String> {
        let document = self.convert_bytes(bytes, filename)?;
        MarkdownRenderer.render(&document)
    }

    /// Convert to the IR v2 JSON.
    pub fn convert_to_json(&self, bytes: &[u8], filename: &str) -> Result<String> {
        let document = self.convert_bytes(bytes, filename)?;
        JsonRenderer.render(&document)
    }
}

/// Reorder a page's blocks into reading order using XY-Cut++ over their bounding
/// boxes. PDF boxes are bottom-left origin; we flip y into screen coordinates
/// (top = smallest y) for the algorithm. If any block lacks a bbox we leave the
/// page order untouched (the legacy extraction order is already top-to-bottom).
fn reorder_reading_order(page: &mut dongler_core::ir::Page) {
    if page.blocks.len() < 2 {
        return;
    }
    let boxes: Option<Vec<BBox>> = page
        .blocks
        .iter()
        .map(block_bbox)
        .collect::<Option<Vec<_>>>();
    let Some(boxes) = boxes else {
        return;
    };

    // Flip y into screen space so "top of page" is the smallest y.
    let page_height = page
        .height
        .unwrap_or_else(|| boxes.iter().map(|b| b.y + b.height).fold(0.0, f32::max));
    let screen: Vec<BBox> = boxes
        .iter()
        .map(|b| BBox {
            x: b.x,
            y: page_height - (b.y + b.height),
            width: b.width,
            height: b.height,
        })
        .collect();

    let order = order::reading_order(&screen);

    // Apply the permutation.
    let mut reordered = Vec::with_capacity(page.blocks.len());
    let original = std::mem::take(&mut page.blocks);
    let mut slots: Vec<Option<Block>> = original.into_iter().map(Some).collect();
    for idx in order {
        if let Some(block) = slots[idx].take() {
            reordered.push(block);
        }
    }
    // Safety net: append anything not placed (should not happen — order is a
    // total permutation), preserving the no-drop guarantee at the block level.
    for slot in slots.into_iter().flatten() {
        reordered.push(slot);
    }
    page.blocks = reordered;
}

fn block_bbox(block: &Block) -> Option<BBox> {
    match block {
        Block::Text(t) => t.bbox,
        Block::Table(t) => t.bbox,
        Block::Figure(f) => f.bbox,
    }
}

/// Stamp text-layer provenance on blocks that don't already carry it. Text from
/// the born-digital provider is, by definition, `TextLayer` — it cannot be
/// hallucinated.
fn stamp_text_layer_provenance(page: &mut dongler_core::ir::Page, detector: &str) {
    let provenance = || Provenance {
        text_source: TextSource::TextLayer,
        detector: Some(detector.to_owned()),
        confidence: None,
    };
    for block in &mut page.blocks {
        match block {
            Block::Text(t) if t.provenance.is_none() => t.provenance = Some(provenance()),
            Block::Table(t) if t.provenance.is_none() => t.provenance = Some(provenance()),
            Block::Figure(f) if f.provenance.is_none() => f.provenance = Some(provenance()),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_pdf(text: &str) -> Vec<u8> {
        let content = format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET");
        let mut pdf = Vec::new();
        pdf.extend_from_slice(b"%PDF-1.4\n");
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
        pdf.extend_from_slice(
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
              /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n",
        );
        pdf.extend_from_slice(
            b"4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
        );
        pdf.extend_from_slice(
            format!(
                "5 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
                content.len(),
                content
            )
            .as_bytes(),
        );
        pdf.extend_from_slice(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
        pdf
    }

    #[test]
    fn convert_sets_route_and_provenance() {
        let pipeline = Pipeline::new();
        let doc = pipeline
            .convert_bytes(&minimal_pdf("Hello Pipeline"), "x.pdf")
            .expect("convert");
        assert_eq!(doc.schema_version, "dongler.ir.v2");
        let page = &doc.pages[0];
        assert_eq!(page.route, Some(dongler_core::ir::Route::BornDigital));
        match &page.blocks[0] {
            Block::Text(t) => {
                let prov = t.provenance.as_ref().expect("provenance");
                assert_eq!(prov.text_source, TextSource::TextLayer);
                assert_eq!(prov.detector.as_deref(), Some("dongler-core"));
            }
            other => panic!("expected text block, got {other:?}"),
        }
    }

    #[test]
    fn convert_to_markdown_contains_text() {
        let pipeline = Pipeline::new();
        let md = pipeline
            .convert_to_markdown(&minimal_pdf("Markdown Out"), "x.pdf")
            .expect("markdown");
        assert!(md.contains("Markdown Out"), "got: {md:?}");
    }

    #[test]
    fn block_count_is_preserved_through_reordering() {
        // Reading-order reordering must never add or drop blocks.
        let pipeline = Pipeline::new();
        let doc = pipeline
            .convert_bytes(&minimal_pdf("One Block"), "x.pdf")
            .expect("convert");
        assert_eq!(doc.pages[0].blocks.len(), 1);
    }
}
