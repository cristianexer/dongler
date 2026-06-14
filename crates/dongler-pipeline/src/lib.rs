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
pub mod table_fusion;
pub mod table_structure;
pub mod triage;

mod textprovider;
pub use textprovider::{DonglerCoreProvider, TextProvider};

#[cfg(feature = "ml")]
pub mod ml;

use dongler_core::ir::{Block, BlockKind, Page, Provenance, TextBlock, TextSource};
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
            coalesce_heading_runs(page);
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

    /// Full hybrid conversion: the deterministic IR, then ML augmentation
    /// (currently table-structure recognition via SLANet-plus). Only available
    /// with the `ml` feature; the born-digital text + reading order are produced
    /// deterministically first, so ML failure degrades to the deterministic
    /// result (recorded as a document warning) rather than erroring.
    #[cfg(feature = "ml")]
    pub fn convert(&self, bytes: &[u8], filename: &str) -> Result<Document> {
        let mut document = self.convert_bytes(bytes, filename)?;
        if bytes.starts_with(b"%PDF") {
            if let Err(err) = self.apply_table_structure(bytes, &mut document) {
                document.warnings.push(dongler_core::ir::Warning {
                    code: "ml.table_structure".to_owned(),
                    severity: "warning".to_owned(),
                    message: format!("table-structure stage skipped: {err}"),
                    source_anchor: None,
                });
            }
        }
        Ok(document)
    }

    /// Replace each heuristic-detected table's grid with a SLANet-plus structure
    /// prediction whose cell text is snapped from the deterministic text layer
    /// (PRD §4.F). The heuristic table's bbox is reused as the region source; the
    /// model decides the grid, the text layer decides the content. Per-region
    /// failures keep that table's deterministic result.
    #[cfg(feature = "ml")]
    fn apply_table_structure(&self, bytes: &[u8], document: &mut Document) -> Result<()> {
        use crate::ml::{models, raster, tables::TableEngine};

        let has_table = document
            .pages
            .iter()
            .any(|p| p.blocks.iter().any(|b| matches!(b, Block::Table(t) if t.bbox.is_some())));
        if !has_table {
            return Ok(());
        }

        let entry = crate::registry::default_for(crate::registry::ModelTask::TableStructure)
            .ok_or_else(|| dongler_core::DonglerError::pdf("no default table-structure model"))?;
        let onnx = models::ensure_model(entry, SLANET_ONNX_FILE)
            .map_err(|e| dongler_core::DonglerError::pdf(e.to_string()))?;
        let mut engine = TableEngine::from_onnx_path(&onnx, crate::ml::tables::SLANET_INPUT)
            .map_err(|e| dongler_core::DonglerError::pdf(e.to_string()))?;
        let pdfium = raster::bind_pdfium().map_err(|e| dongler_core::DonglerError::pdf(e.to_string()))?;

        // Raw text-layer spans per page (kept even where tables consumed them).
        let page_spans = dongler_core::pdf::extract_pdf_spans(bytes)?;

        for page in &mut document.pages {
            let page_index = (page.number.saturating_sub(1)) as u16;
            let page_height = page.height.unwrap_or(0.0);
            let spans = page_spans.iter().find(|p| p.page_number == page.number);

            for block in &mut page.blocks {
                let Block::Table(table) = block else { continue };
                let Some(region) = table.bbox else { continue };
                if let Some(cells) = self.table_structure_for_region(
                    &mut engine,
                    &pdfium,
                    bytes,
                    page_index,
                    region,
                    page_height,
                    spans,
                ) {
                    if cells.is_empty() {
                        continue;
                    }
                    table.html = dongler_core::render::html_table_from_cells(
                        &cells,
                        table.caption.as_deref(),
                    );
                    table.cells = cells;
                    table.provenance = Some(Provenance {
                        text_source: TextSource::TextLayer,
                        detector: Some(format!("{}@{}", entry.name, entry.version)),
                        confidence: None,
                    });
                }
            }
        }
        Ok(())
    }

    /// Run SLANet on one table region and snap text into its cells. Returns
    /// `None` on any per-region ML/raster error so the caller keeps the
    /// deterministic table.
    #[cfg(feature = "ml")]
    #[allow(clippy::too_many_arguments)]
    fn table_structure_for_region(
        &self,
        engine: &mut crate::ml::tables::TableEngine,
        pdfium: &pdfium_render::prelude::Pdfium,
        bytes: &[u8],
        page_index: u16,
        region: BBox,
        page_height: f32,
        spans: Option<&dongler_core::pdf::PageSpans>,
    ) -> Option<Vec<dongler_core::ir::TableCell>> {
        let (crop, xform) =
            crate::ml::raster::render_region(pdfium, bytes, page_index, region, page_height, TABLE_RENDER_DPI)
                .ok()?;
        let structure = engine.run(&crop).ok()?;

        // Cell boxes (crop px) → PDF user space.
        let cells: Vec<crate::table_structure::TableCellPrediction> = structure
            .cells
            .iter()
            .map(|c| {
                let pdf = xform.px_to_pdf(c.bbox.x, c.bbox.y, c.bbox.width, c.bbox.height);
                crate::table_structure::TableCellPrediction { bbox: pdf, ..*c }
            })
            .collect();

        // Text-layer spans whose center falls inside the table region.
        let (span_boxes, span_texts): (Vec<BBox>, Vec<String>) = spans
            .map(|p| {
                p.spans
                    .iter()
                    .filter(|s| bbox_center_in(&s.bbox, &region))
                    .map(|s| (s.bbox, s.text.clone()))
                    .unzip()
            })
            .unwrap_or_default();

        Some(crate::table_fusion::fill_cells_from_text_layer(
            &cells,
            &span_boxes,
            &span_texts,
            self.attach_radius,
        ))
    }
}

/// SLANet rasterization DPI for table-region crops.
#[cfg(feature = "ml")]
const TABLE_RENDER_DPI: f32 = 150.0;
/// SLANet ONNX artifact filename in the registry's HF repo (verified against
/// `bdatdo0601/slanet-1m-onnx`). The structure vocabulary is embedded in code
/// ([`crate::table_structure::slanet_char_dict`]), so no companion file.
#[cfg(feature = "ml")]
const SLANET_ONNX_FILE: &str = "slanet_1m.onnx";

/// True if `inner`'s center lies within `outer`.
#[cfg(feature = "ml")]
fn bbox_center_in(inner: &BBox, outer: &BBox) -> bool {
    let cx = inner.x + inner.width / 2.0;
    let cy = inner.y + inner.height / 2.0;
    cx >= outer.x && cx <= outer.x + outer.width && cy >= outer.y && cy <= outer.y + outer.height
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

/// Reassemble a multi-fragment heading (typically a paper/report title) that the
/// line-level extractor emitted as several `heading_n` blocks. Two things break a
/// title apart: it wraps across lines, and a wide intra-line gap (common in
/// centered titles) splits one visual row into side-by-side fragments. Left as-is,
/// the title renders as a stack of stray `##` lines.
///
/// We run two conservative sweeps: first merge same-row fragments left-to-right
/// (horizontal), then merge the resulting rows top-to-bottom (vertical). Every
/// merge requires the SAME heading level and reading-order adjacency, and is
/// tightly bounded in the perpendicular axis, so genuinely separate headings —
/// and multi-column body headings separated by a gutter — never join.
fn coalesce_heading_runs(page: &mut Page) {
    if page.blocks.len() < 2 {
        return;
    }
    let blocks = std::mem::take(&mut page.blocks);
    let blocks = merge_heading_sweep(blocks, same_row_fragment);
    let blocks = merge_heading_sweep(blocks, wrapped_line_below);
    page.blocks = blocks;
}

/// Upper bound on a coalesced heading's length. Real titles/headings are short;
/// a much longer run means the upstream classifier mislabeled body or math lines
/// as headings, so we stop merging rather than build one giant `##` block.
const MAX_HEADING_CHARS: usize = 240;

/// One coalescing sweep: fold each heading block into the previous one when they
/// share a level and `joins` reports them as fragments of the same heading.
fn merge_heading_sweep(
    blocks: Vec<Block>,
    joins: fn(&TextBlock, &TextBlock) -> bool,
) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::with_capacity(blocks.len());
    for block in blocks {
        if let Block::Text(curr) = &block {
            let level = BlockKind::parse(&curr.kind).heading_level();
            if let (Some(level), Some(Block::Text(prev))) = (level, out.last_mut()) {
                if BlockKind::parse(&prev.kind).heading_level() == Some(level)
                    && prev.text.len() + curr.text.len() <= MAX_HEADING_CHARS
                    && joins(prev, curr)
                {
                    merge_heading_into(prev, curr);
                    continue;
                }
            }
        }
        out.push(block);
    }
    out
}

/// `curr` sits on the same baseline row as `prev` and just to its right — one
/// visual heading row split by a wide intra-line gap.
fn same_row_fragment(prev: &TextBlock, curr: &TextBlock) -> bool {
    let (Some(a), Some(b)) = (prev.bbox, curr.bbox) else {
        return false;
    };
    let line_h = a.height.min(b.height).max(1.0);
    let min_w = a.width.min(b.width).max(1.0);
    // Strong vertical overlap == same row.
    let v_overlap = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
    if v_overlap < 0.5 * line_h {
        return false;
    }
    // Small horizontal separation (titles split mid-row leave only a few points;
    // a real two-column gutter is far wider than a line height).
    let separation = (b.x - (a.x + a.width)).max(a.x - (b.x + b.width));
    separation > -0.6 * min_w && separation < 1.0 * line_h
}

/// `curr` is the next wrapped line of `prev`: directly below it (≤ ~one line gap)
/// and horizontally overlapping (same column).
fn wrapped_line_below(prev: &TextBlock, curr: &TextBlock) -> bool {
    let (Some(a), Some(b)) = (prev.bbox, curr.bbox) else {
        return false;
    };
    let line_h = a.height.min(b.height).max(1.0);
    // PDF coords are y-up, so `prev` (above) has the larger y; the gap between
    // prev's bottom edge and curr's top edge should be under ~one line. We allow
    // a generous negative bound because wrapped-title bboxes from the line
    // extractor often carry imprecise baselines and vertically overlap.
    let gap = a.y - (b.y + b.height);
    if !(gap > -1.25 * line_h && gap < 0.9 * line_h) {
        return false;
    }
    let overlap = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
    overlap > 0.3 * a.width.min(b.width).max(1.0)
}

/// Fold `curr`'s text and geometry into `prev` (the running heading).
fn merge_heading_into(prev: &mut TextBlock, curr: &TextBlock) {
    let left = prev.text.trim_end();
    let right = curr.text.trim_start();
    // Keep hyphenated breaks tight ("Attention" + "-Centric" → "Attention-Centric").
    let sep = if right.starts_with('-') || left.ends_with('-') {
        ""
    } else {
        " "
    };
    prev.text = format!("{left}{sep}{right}");
    if let (Some(a), Some(b)) = (prev.bbox, curr.bbox) {
        let x = a.x.min(b.x);
        let y = a.y.min(b.y);
        let width = (a.x + a.width).max(b.x + b.width) - x;
        let height = (a.y + a.height).max(b.y + b.height) - y;
        prev.bbox = Some(BBox {
            x,
            y,
            width,
            height,
        });
    }
    prev.lines.extend(curr.lines.iter().cloned());
    prev.source_anchors.extend(curr.source_anchors.iter().cloned());
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

    fn heading_block(text: &str, y: f32, h: f32) -> Block {
        Block::Text(TextBlock {
            text: text.to_owned(),
            kind: "heading_2".to_owned(),
            bbox: Some(BBox {
                x: 72.0,
                y,
                width: 300.0,
                height: h,
            }),
            ..Default::default()
        })
    }

    #[test]
    fn coalesce_merges_wrapped_title_lines() {
        // Three stacked heading_2 lines (y descending = down the page) that form
        // one wrapped title should merge into a single heading block.
        let mut page = Page {
            number: 1,
            height: Some(792.0),
            blocks: vec![
                heading_block("Incremental Coordination: Attention", 740.0, 14.0),
                heading_block("-Centric Speech Production", 724.0, 14.0),
                heading_block("in a Physically Situated Conversational", 708.0, 14.0),
            ],
            ..Default::default()
        };
        coalesce_heading_runs(&mut page);
        assert_eq!(page.blocks.len(), 1, "wrapped title should collapse to one");
        let Block::Text(t) = &page.blocks[0] else {
            panic!("expected text block");
        };
        assert_eq!(
            t.text,
            "Incremental Coordination: Attention-Centric Speech Production \
             in a Physically Situated Conversational"
        );
    }

    fn heading_at(text: &str, x: f32, y: f32, w: f32) -> Block {
        Block::Text(TextBlock {
            text: text.to_owned(),
            kind: "heading_2".to_owned(),
            bbox: Some(BBox {
                x,
                y,
                width: w,
                height: 16.6,
            }),
            ..Default::default()
        })
    }

    #[test]
    fn coalesce_rebuilds_title_split_into_row_and_column_fragments() {
        // Real olmOCR multi_column title geometry: two rows, each split into two
        // side-by-side fragments by a wide centered gap. Reading order is
        // row-major (r1-left, r1-right, r2-left, r2-right).
        let mut page = Page {
            number: 1,
            height: Some(792.0),
            blocks: vec![
                heading_at("Incremental Coordination: Attention", 90.1, 754.2, 235.3),
                heading_at("-Centric Speech Production", 327.8, 754.2, 180.2),
                heading_at("in a Physically Situated Conversational", 151.2, 737.3, 253.0),
                heading_at("Agent", 406.0, 737.3, 37.2),
            ],
            ..Default::default()
        };
        coalesce_heading_runs(&mut page);
        assert_eq!(page.blocks.len(), 1, "title should rebuild into one heading");
        let Block::Text(t) = &page.blocks[0] else {
            panic!("expected text block");
        };
        assert_eq!(
            t.text,
            "Incremental Coordination: Attention-Centric Speech Production \
             in a Physically Situated Conversational Agent"
        );
    }

    #[test]
    fn coalesce_keeps_separated_headings_apart() {
        // Two same-level headings with a full blank line between them are distinct
        // sections, not a wrapped title — they must stay separate.
        let mut page = Page {
            number: 1,
            height: Some(792.0),
            blocks: vec![
                heading_block("First Section", 740.0, 14.0),
                heading_block("Second Section", 680.0, 14.0),
            ],
            ..Default::default()
        };
        coalesce_heading_runs(&mut page);
        assert_eq!(page.blocks.len(), 2, "separated headings must not merge");
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
