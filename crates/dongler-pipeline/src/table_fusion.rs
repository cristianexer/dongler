//! Text-snap fusion for tables (PRD §4.F/§4.G — "the Docling trick").
//!
//! Given a table's grid topology from the structure model ([`crate::table_structure`])
//! and the deterministic text-layer spans that fall inside the table region, fill
//! each cell's text **from the text layer**, never from the model. This is what
//! makes pipeline tables hallucination-free: the model decides *where the cells
//! are*; the born-digital text decides *what they say*.
//!
//! Pure module (no `ort`/`image`) so it is unit-tested in the default build. It
//! reuses [`crate::fusion::assign_spans`] at **cell granularity** (each cell is a
//! one-off region) and upholds the no-drop invariant: every span inside the
//! region lands in exactly one cell — spans that fall in a gap (gridline/padding)
//! attach to the nearest cell rather than vanishing.
//!
//! Coordinate convention: `cell_boxes` and `span_boxes` must share one space.
//! The orchestrator passes both in PDF user space (y-up); within-cell reading
//! order is therefore top-to-bottom = descending `y`, then left-to-right.

use crate::fusion::{assign_spans, Region, RegionClass};
use crate::geometry::center;
use crate::table_structure::TableCellPrediction;
use dongler_core::ir::{BBox, TableCell};

/// Fill the predicted grid with text-layer content.
///
/// * `cells` — predicted physical cells (topology + bbox), from the structure model.
/// * `span_boxes` / `span_texts` — text-layer spans inside the table region
///   (parallel slices; `span_texts[i]` is the text of `span_boxes[i]`).
/// * `attach_radius` — radius (coordinate units) within which a span outside every
///   cell still attaches to the nearest cell during [`assign_spans`].
///
/// Returns IR cells (one per predicted cell, spanned-over positions omitted) with
/// text snapped in. Returns an empty vec if `cells` is empty (caller keeps its
/// deterministic table). **No span is dropped** when at least one cell exists.
pub fn fill_cells_from_text_layer(
    cells: &[TableCellPrediction],
    span_boxes: &[BBox],
    span_texts: &[String],
    attach_radius: f32,
) -> Vec<TableCell> {
    if cells.is_empty() {
        return Vec::new();
    }
    debug_assert_eq!(span_boxes.len(), span_texts.len());

    let regions: Vec<Region> = cells
        .iter()
        .enumerate()
        .map(|(i, c)| Region {
            id: i,
            bbox: c.bbox,
            class: RegionClass::Table,
        })
        .collect();

    let assignment = assign_spans(&regions, span_boxes, attach_radius);

    // Bucket span indices per cell. Spans still unowned (orphans) are forced into
    // the nearest cell by center distance so no text-layer character is lost.
    let mut per_cell: Vec<Vec<usize>> = vec![Vec::new(); cells.len()];
    for (span_idx, owner) in assignment.span_region.iter().enumerate() {
        let cell_idx = match owner {
            Some(id) => *id,
            None => nearest_cell(cells, &span_boxes[span_idx]),
        };
        per_cell[cell_idx].push(span_idx);
    }

    cells
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let text = join_cell_text(&per_cell[i], span_boxes, span_texts);
            TableCell {
                row: c.row,
                column: c.col,
                text,
                bbox: Some(c.bbox),
                is_header: c.is_header,
                col_span: c.col_span,
                row_span: c.row_span,
            }
        })
        .collect()
}

/// Index of the cell whose box center is nearest the span center. Used only for
/// orphan spans (those inside the table region but outside every cell box).
fn nearest_cell(cells: &[TableCellPrediction], span: &BBox) -> usize {
    let (sx, sy) = center(span);
    let mut best = 0;
    let mut best_d = f32::INFINITY;
    for (i, c) in cells.iter().enumerate() {
        let (cx, cy) = center(&c.bbox);
        let d = (cx - sx).powi(2) + (cy - sy).powi(2);
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best
}

/// Join a cell's spans into reading order: top-to-bottom (descending `y` in PDF
/// user space), then left-to-right. Spacing is **gap-aware** — many PDFs position
/// one glyph per run, so a naive space-between-spans yields `P A R T`. We insert a
/// space only when the horizontal gap to the previous span on the same line
/// exceeds a fraction of the glyph height (a word break); abutting glyphs glue
/// into words. A line change always inserts a space.
fn join_cell_text(span_idxs: &[usize], span_boxes: &[BBox], span_texts: &[String]) -> String {
    let mut idxs: Vec<usize> = span_idxs
        .iter()
        .copied()
        .filter(|&i| !span_texts[i].trim().is_empty())
        .collect();
    idxs.sort_by(|&a, &b| {
        let (ba, bb) = (&span_boxes[a], &span_boxes[b]);
        if (ba.y - bb.y).abs() < line_band(ba, bb) {
            // Same line → order left-to-right.
            ba.x.partial_cmp(&bb.x).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            // Different line → top first (larger y in PDF user space).
            bb.y.partial_cmp(&ba.y).unwrap_or(std::cmp::Ordering::Equal)
        }
    });

    let mut out = String::new();
    let mut prev: Option<&BBox> = None;
    for &idx in &idxs {
        let b = &span_boxes[idx];
        let t = span_texts[idx].trim();
        if let Some(p) = prev {
            let same_line = (p.y - b.y).abs() < line_band(p, b);
            let gap = b.x - (p.x + p.width);
            // Word break: different line, or a horizontal gap wider than ~a quarter
            // of the glyph height. Abutting / overlapping glyphs (gap ≤ that) glue.
            if !same_line || gap > 0.25 * b.height.max(p.height).max(1.0) {
                out.push(' ');
            }
        }
        out.push_str(t);
        prev = Some(b);
    }
    out.trim().to_string()
}

fn line_band(a: &BBox, b: &BBox) -> f32 {
    0.5 * a.height.max(b.height).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(row: usize, col: usize, x: f32, y: f32, w: f32, h: f32) -> TableCellPrediction {
        TableCellPrediction {
            bbox: BBox {
                x,
                y,
                width: w,
                height: h,
            },
            row,
            col,
            col_span: 1,
            row_span: 1,
            is_header: false,
        }
    }

    fn span(x: f32, y: f32, w: f32, h: f32, text: &str) -> (BBox, String) {
        (
            BBox {
                x,
                y,
                width: w,
                height: h,
            },
            text.to_string(),
        )
    }

    fn split(v: Vec<(BBox, String)>) -> (Vec<BBox>, Vec<String>) {
        v.into_iter().unzip()
    }

    #[test]
    fn snaps_each_span_into_its_containing_cell() {
        let cells = vec![
            cell(0, 0, 0.0, 0.0, 50.0, 20.0),
            cell(0, 1, 50.0, 0.0, 50.0, 20.0),
        ];
        let (boxes, texts) = split(vec![
            span(5.0, 5.0, 10.0, 10.0, "Revenue"),
            span(55.0, 5.0, 10.0, 10.0, "100"),
        ]);
        let out = fill_cells_from_text_layer(&cells, &boxes, &texts, 4.0);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "Revenue");
        assert_eq!(out[0].column, 0);
        assert_eq!(out[1].text, "100");
        assert_eq!(out[1].column, 1);
    }

    #[test]
    fn multiple_spans_in_a_cell_join_in_reading_order() {
        let cells = vec![cell(0, 0, 0.0, 0.0, 100.0, 40.0)];
        // y-up: top line has larger y. "Net" then "income" on top line, "total"
        // below. Provided out of order to exercise the sort.
        let (boxes, texts) = split(vec![
            span(0.0, 5.0, 30.0, 10.0, "total"),  // lower line
            span(30.0, 25.0, 30.0, 10.0, "income"), // top line, right
            span(0.0, 25.0, 25.0, 10.0, "Net"),   // top line, left
        ]);
        let out = fill_cells_from_text_layer(&cells, &boxes, &texts, 4.0);
        assert_eq!(out[0].text, "Net income total");
    }

    #[test]
    fn per_glyph_spans_glue_into_words_by_gap() {
        // One glyph per span (the per-glyph-Tj PDFs this fixes). Abutting glyphs
        // P,A,R,T (right edge == next left) glue; a wide gap before "X" breaks.
        let cells = vec![cell(0, 0, 0.0, 0.0, 100.0, 20.0)];
        let (boxes, texts) = split(vec![
            span(0.0, 5.0, 5.0, 10.0, "P"),
            span(5.0, 5.0, 5.0, 10.0, "A"),
            span(10.0, 5.0, 5.0, 10.0, "R"),
            span(15.0, 5.0, 5.0, 10.0, "T"),
            span(35.0, 5.0, 5.0, 10.0, "X"), // gap 35-20=15 >> 2.5 → word break
        ]);
        let out = fill_cells_from_text_layer(&cells, &boxes, &texts, 4.0);
        assert_eq!(out[0].text, "PART X");
    }

    #[test]
    fn orphan_span_in_a_gap_is_not_dropped() {
        // Two cells with a gridline gap between them; a span lands in the gap.
        let cells = vec![
            cell(0, 0, 0.0, 0.0, 40.0, 20.0),
            cell(0, 1, 60.0, 0.0, 40.0, 20.0),
        ];
        let (boxes, texts) = split(vec![
            span(10.0, 5.0, 5.0, 5.0, "A"),
            span(48.0, 5.0, 5.0, 5.0, "GAP"), // center ~50.5, outside both cells
        ]);
        let out = fill_cells_from_text_layer(&cells, &boxes, &texts, 2.0);
        let all: String = out.iter().map(|c| c.text.clone()).collect::<Vec<_>>().join("|");
        assert!(all.contains("GAP"), "gap span must survive, got: {all}");
        // No text lost: both tokens present across the cells.
        assert!(all.contains('A'));
    }

    #[test]
    fn empty_cells_input_returns_empty() {
        let (boxes, texts) = split(vec![span(0.0, 0.0, 1.0, 1.0, "x")]);
        assert!(fill_cells_from_text_layer(&[], &boxes, &texts, 4.0).is_empty());
    }

    #[test]
    fn end_to_end_slanet_grid_renders_to_spanning_html() {
        // A 2-column table whose header spans both columns — the exact shape
        // SLANet produces and that text-snap + the renderer must round-trip into
        // valid colspan HTML (the form TEDS scores).
        use crate::table_structure::{parse_structure_tokens, TableCellPrediction};
        let tokens: Vec<String> = [
            "<thead>", "<tr>", "<td", " colspan=\"2\"", ">", "</tr>", "</thead>", "<tbody>",
            "<tr>", "<td>", "<td>", "</tr>", "</tbody>",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        // Header box spans the top; two data cells below it.
        let boxes: Vec<BBox> = tokens
            .iter()
            .map(|t| match t.as_str() {
                "<td" => BBox { x: 0.0, y: 20.0, width: 100.0, height: 10.0 },
                "<td>" => BBox { x: 0.0, y: 0.0, width: 50.0, height: 10.0 },
                _ => BBox { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            })
            .collect();
        let pred: Vec<TableCellPrediction> = parse_structure_tokens(&tokens, &boxes);
        // Two data cells sit side by side on the lower band; header spans the top.
        let data_cells = pred.iter().filter(|c| !c.is_header).count();
        assert_eq!(data_cells, 2);

        let (sb, st) = split(vec![
            span(5.0, 22.0, 40.0, 6.0, "Results"),   // in header span
            span(5.0, 2.0, 20.0, 6.0, "Left"),       // data col 0
            span(60.0, 2.0, 20.0, 6.0, "Right"),     // data col 1 (x=60 → second half)
        ]);
        // Header box only covers x[0,100] y[20,30]; widen data cells so spans land.
        let mut cells = pred;
        for c in &mut cells {
            if !c.is_header {
                c.bbox.width = 50.0;
                if c.col == 1 {
                    c.bbox.x = 50.0;
                }
            }
        }
        let ir_cells = fill_cells_from_text_layer(&cells, &sb, &st, 4.0);
        let html = dongler_core::render::html_table_from_cells(&ir_cells, None)
            .expect("html");
        assert!(html.contains("colspan=\"2\""), "header should span 2 cols: {html}");
        assert!(html.contains("Results") && html.contains("Left") && html.contains("Right"), "{html}");
        // Header cell is a <th>; data cells are <td>.
        assert!(html.contains("<th colspan=\"2\">Results</th>"), "{html}");
    }

    #[test]
    fn preserves_span_attributes_from_prediction() {
        let mut c = cell(0, 0, 0.0, 0.0, 100.0, 20.0);
        c.col_span = 2;
        c.is_header = true;
        let (boxes, texts) = split(vec![span(5.0, 5.0, 10.0, 10.0, "Header")]);
        let out = fill_cells_from_text_layer(&[c], &boxes, &texts, 4.0);
        assert_eq!(out[0].col_span, 2);
        assert!(out[0].is_header);
        assert_eq!(out[0].text, "Header");
    }
}
