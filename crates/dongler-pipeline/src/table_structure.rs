//! Pure table-structure decoding for SLANet-plus (PRD §4.F).
//!
//! SLANet is an attention sequence model with two heads. Per decode step it emits
//! a **structure token** (argmax over an HTML-token vocabulary shipped as a char
//! dict) and a **cell bbox**. This module turns those raw tensors into a typed
//! grid of [`TableCellPrediction`]s — *topology only*. Cell **content is never
//! taken from the model**; it is snapped from the deterministic text layer in
//! [`crate::table_fusion`]. That separation is what makes pipeline tables
//! hallucination-free (PRD §4.F, "the Docling trick").
//!
//! Everything here is pure (no `ort`, no `image`) so it compiles and is
//! unit-tested in the default, zero-ML build. The `ort` session that produces the
//! raw tensors lives behind the `ml` feature in `ml::tables` and feeds
//! [`decode_slanet`].
//!
//! ## What is verified vs. assumed
//! The grid-layout logic ([`parse_structure_tokens`]) is format-independent and
//! correct by construction. The tensor→token and bbox conventions in
//! [`decode_slanet`] follow the documented PP-Structure / RapidTable SLANet
//! format and are marked **VERIFY (spike PR0)** where they depend on the exact
//! ONNX artifact: structure logits `[1, T, V]`, one bbox `[1, T, 4]` per step
//! (`xyxy`, normalized to the model input), and a cell-opening token vocabulary
//! of `<td>`, `<td></td>`, and the `<td` + ` colspan/rowspan="n"` + `>` triple.

use dongler_core::ir::BBox;
use std::collections::HashSet;

/// A single physical table cell predicted by the structure model, in the model
/// input image's pixel space. `row`/`col` are zero-based grid coordinates of the
/// cell's top-left; spanned-over positions are not emitted (one struct per cell).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TableCellPrediction {
    pub bbox: BBox,
    pub row: usize,
    pub col: usize,
    pub col_span: usize,
    pub row_span: usize,
    pub is_header: bool,
}

/// The decoded structure of one table: its physical cells plus the raw token
/// stream (kept for debugging / eval provenance).
#[derive(Debug, Clone, PartialEq)]
pub struct TableStructure {
    pub cells: Vec<TableCellPrediction>,
    pub tokens: Vec<String>,
}

/// Structure tokens that terminate the sequence. PaddleOCR SLANet uses `eos`
/// (index 29); `sos` (index 0) is skipped, not terminal. The HTML variants are
/// defensive for other exports. The empty string guards an out-of-range id.
const EOS_TOKENS: &[&str] = &["eos", "</html>", "</body>", ""];

/// The fixed PaddleOCR SLANet structure-token vocabulary, indexed by class id:
/// `sos` (0) + the 28 table tokens from the model's `inference.yml` (1..=28) +
/// `eos` (29) — matching `TableLabelDecode.add_special_char`. The model's logits
/// have 30 classes; `decode_slanet` argmaxes into this list.
pub fn slanet_char_dict() -> Vec<String> {
    const TOKENS: &[&str] = &[
        "sos",
        "<thead>",
        "<tr>",
        "<td>",
        "</td>",
        "</tr>",
        "</thead>",
        "<tbody>",
        "</tbody>",
        "<td",
        " colspan=\"5\"",
        ">",
        " colspan=\"2\"",
        " colspan=\"3\"",
        " rowspan=\"2\"",
        " colspan=\"4\"",
        " colspan=\"6\"",
        " rowspan=\"3\"",
        " colspan=\"9\"",
        " colspan=\"10\"",
        " colspan=\"7\"",
        " rowspan=\"4\"",
        " rowspan=\"5\"",
        " rowspan=\"9\"",
        " colspan=\"8\"",
        " rowspan=\"8\"",
        " rowspan=\"6\"",
        " rowspan=\"7\"",
        " rowspan=\"10\"",
        "eos",
    ];
    TOKENS.iter().map(|s| s.to_string()).collect()
}

/// Decode SLANet's two output tensors into a typed grid.
///
/// * `structure_logits` / `structure_shape` — `[1, T, V]` (or `[T, V]`) logits.
/// * `bbox_output` / `bbox_shape` — `[1, T, 4]` (or `[T, 4]`) cell boxes as
///   `xyxy`, normalized `0..1` to the model input. **VERIFY (spike PR0):** some
///   exports emit pixel coords or `cxcywh`; flip `BBOX_NORMALIZED` accordingly.
/// * `char_dict` — vocabulary, indexed by class id, mapping to token strings.
/// * `input_w` / `input_h` — model input pixel dimensions, used to denormalize
///   boxes into input-image pixel space.
///
/// Returns topology only; see module docs.
pub fn decode_slanet(
    structure_logits: &[f32],
    structure_shape: &[i64],
    bbox_output: &[f32],
    bbox_shape: &[i64],
    char_dict: &[String],
    input_w: f32,
    input_h: f32,
) -> TableStructure {
    let (steps, vocab) = seq_dims(structure_shape);
    let (bbox_steps, bbox_stride) = seq_dims(bbox_shape);
    if steps == 0 || vocab == 0 {
        return TableStructure {
            cells: Vec::new(),
            tokens: Vec::new(),
        };
    }

    // Argmax each timestep into a token string; collect the per-step bbox so the
    // grid walker can attach one to each cell-opening token.
    let mut tokens: Vec<String> = Vec::with_capacity(steps);
    let mut step_boxes: Vec<BBox> = Vec::with_capacity(steps);
    for t in 0..steps {
        let row = &structure_logits[t * vocab..(t + 1) * vocab];
        let idx = argmax(row);
        let token = char_dict.get(idx).cloned().unwrap_or_default();
        if EOS_TOKENS.contains(&token.as_str()) {
            break;
        }
        tokens.push(token);
        step_boxes.push(box_at(bbox_output, bbox_steps, bbox_stride, t, input_w, input_h));
    }

    let cells = parse_structure_tokens(&tokens, &step_boxes);
    TableStructure { cells, tokens }
}

/// Walk a structure-token stream into a physical grid, resolving `colspan`/
/// `rowspan` with the standard HTML occupancy algorithm. `step_boxes[i]` is the
/// bbox predicted at token `i`; the bbox of a cell is taken from its opening
/// token's step. This function is the format-independent core and is exhaustively
/// unit-tested.
pub fn parse_structure_tokens(tokens: &[String], step_boxes: &[BBox]) -> Vec<TableCellPrediction> {
    let mut cells = Vec::new();
    let mut occupied: HashSet<(usize, usize)> = HashSet::new();
    let mut in_header = false;
    // row index of the current `<tr>`; -1 (via Option) before the first row.
    let mut row: Option<usize> = None;
    let mut col_cursor = 0usize;

    let mut i = 0usize;
    while i < tokens.len() {
        let tok = tokens[i].trim();
        match classify(tok) {
            TokKind::TheadStart => in_header = true,
            TokKind::TheadEnd => in_header = false,
            TokKind::RowStart => {
                row = Some(row.map_or(0, |r| r + 1));
                col_cursor = 0;
            }
            TokKind::CellEmpty | TokKind::CellOpen => {
                let r = row.unwrap_or(0);
                // A bare `<td` opener is followed by attribute tokens and a `>`;
                // consume them to read spans and to land `i` on the closing `>`.
                let (col_span, row_span, consumed) = if matches!(classify(tok), TokKind::CellOpen)
                    && tok == "<td"
                {
                    read_spans(tokens, i)
                } else {
                    (1, 1, 0)
                };
                let bbox = step_boxes.get(i).copied().unwrap_or(ZERO_BBOX);

                // Skip grid columns already taken by a rowspan from a row above.
                while occupied.contains(&(r, col_cursor)) {
                    col_cursor += 1;
                }
                let col_span = col_span.max(1);
                let row_span = row_span.max(1);
                for dr in 0..row_span {
                    for dc in 0..col_span {
                        occupied.insert((r + dr, col_cursor + dc));
                    }
                }
                cells.push(TableCellPrediction {
                    bbox,
                    row: r,
                    col: col_cursor,
                    col_span,
                    row_span,
                    is_header: in_header,
                });
                col_cursor += col_span;
                i += consumed;
            }
            TokKind::Other => {}
        }
        i += 1;
    }
    cells
}

const ZERO_BBOX: BBox = BBox {
    x: 0.0,
    y: 0.0,
    width: 0.0,
    height: 0.0,
};

enum TokKind {
    TheadStart,
    TheadEnd,
    RowStart,
    /// `<td></td>` — an empty cell in a single token.
    CellEmpty,
    /// `<td>` or `<td` (the latter carries following span attribute tokens).
    CellOpen,
    Other,
}

fn classify(tok: &str) -> TokKind {
    match tok {
        "<thead>" => TokKind::TheadStart,
        "</thead>" => TokKind::TheadEnd,
        "<tr>" => TokKind::RowStart,
        "<td></td>" => TokKind::CellEmpty,
        "<td>" | "<td" => TokKind::CellOpen,
        _ => TokKind::Other,
    }
}

/// From a `<td` opener at index `i`, read ` colspan="n"` / ` rowspan="n"` tokens
/// up to the closing `>`. Returns `(col_span, row_span, tokens_consumed)` where
/// `tokens_consumed` counts the attribute and `>` tokens after the opener.
fn read_spans(tokens: &[String], i: usize) -> (usize, usize, usize) {
    let mut col_span = 1;
    let mut row_span = 1;
    let mut consumed = 0;
    let mut j = i + 1;
    while j < tokens.len() {
        let t = tokens[j].trim();
        if let Some(n) = parse_span_attr(t, "colspan") {
            col_span = n;
        } else if let Some(n) = parse_span_attr(t, "rowspan") {
            row_span = n;
        } else if t == ">" {
            consumed += 1;
            break;
        } else {
            // Unknown token inside the opener — stop defensively.
            break;
        }
        consumed += 1;
        j += 1;
    }
    (col_span, row_span, consumed)
}

/// Parse ` colspan="2"` / `colspan="2"` style attribute tokens.
fn parse_span_attr(tok: &str, attr: &str) -> Option<usize> {
    let t = tok.trim();
    let rest = t.strip_prefix(attr)?;
    // rest looks like `="2"`
    let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// Interpret a 2- or 3-D shape as `(sequence_len, last_dim)`, ignoring a leading
/// batch dim of 1.
fn seq_dims(shape: &[i64]) -> (usize, usize) {
    match shape {
        [t, d] => (*t as usize, *d as usize),
        [_b, t, d] => (*t as usize, *d as usize),
        _ => (0, 0),
    }
}

fn argmax(row: &[f32]) -> usize {
    let mut best = 0;
    let mut best_v = f32::NEG_INFINITY;
    for (i, &v) in row.iter().enumerate() {
        if v > best_v {
            best_v = v;
            best = i;
        }
    }
    best
}

/// Read the bbox at timestep `t` as `xyxy` normalized to `input`, returning an
/// `x/y/width/height` box in input-image pixel space. **VERIFY (spike PR0):**
/// set `BBOX_NORMALIZED=false` if the export emits pixel coords already.
const BBOX_NORMALIZED: bool = true;
fn box_at(bbox: &[f32], steps: usize, stride: usize, t: usize, input_w: f32, input_h: f32) -> BBox {
    if stride < 4 || t >= steps {
        return ZERO_BBOX;
    }
    let base = t * stride;
    let (x1, y1, x2, y2) = (bbox[base], bbox[base + 1], bbox[base + 2], bbox[base + 3]);
    let (sx, sy) = if BBOX_NORMALIZED {
        (input_w, input_h)
    } else {
        (1.0, 1.0)
    };
    let (x1, y1, x2, y2) = (x1 * sx, y1 * sy, x2 * sx, y2 * sy);
    BBox {
        x: x1.min(x2),
        y: y1.min(y2),
        width: (x2 - x1).abs(),
        height: (y2 - y1).abs(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(x: f32, y: f32, w: f32, h: f32) -> BBox {
        BBox {
            x,
            y,
            width: w,
            height: h,
        }
    }

    fn toks(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn simple_2x2_grid() {
        let t = toks(&[
            "<thead>", "<tr>", "<td>", "<td>", "</tr>", "</thead>", "<tbody>", "<tr>", "<td>",
            "<td>", "</tr>", "</tbody>",
        ]);
        let boxes: Vec<BBox> = t.iter().map(|_| b(0.0, 0.0, 1.0, 1.0)).collect();
        let cells = parse_structure_tokens(&t, &boxes);
        assert_eq!(cells.len(), 4);
        assert_eq!((cells[0].row, cells[0].col), (0, 0));
        assert!(cells[0].is_header);
        assert_eq!((cells[1].row, cells[1].col), (0, 1));
        assert_eq!((cells[2].row, cells[2].col), (1, 0));
        assert!(!cells[2].is_header);
        assert_eq!((cells[3].row, cells[3].col), (1, 1));
    }

    #[test]
    fn empty_cell_token_counts_as_a_cell() {
        let t = toks(&["<tr>", "<td>", "<td></td>", "</tr>"]);
        let boxes: Vec<BBox> = t.iter().map(|_| b(0.0, 0.0, 1.0, 1.0)).collect();
        let cells = parse_structure_tokens(&t, &boxes);
        assert_eq!(cells.len(), 2);
        assert_eq!((cells[1].row, cells[1].col), (0, 1));
    }

    #[test]
    fn colspan_advances_cursor_and_omits_spanned_positions() {
        // Row 0: one cell spanning 2 columns. Row 1: two ordinary cells.
        let t = toks(&[
            "<tr>", "<td", " colspan=\"2\"", ">", "</tr>", "<tr>", "<td>", "<td>", "</tr>",
        ]);
        let boxes: Vec<BBox> = t.iter().map(|_| b(0.0, 0.0, 1.0, 1.0)).collect();
        let cells = parse_structure_tokens(&t, &boxes);
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0].col_span, 2);
        assert_eq!((cells[0].row, cells[0].col), (0, 0));
        // Next row's cells occupy columns 0 and 1 normally.
        assert_eq!((cells[1].row, cells[1].col), (1, 0));
        assert_eq!((cells[2].row, cells[2].col), (1, 1));
    }

    #[test]
    fn rowspan_blocks_column_in_following_row() {
        // Row 0: cell A (rowspan 2) at col 0, cell B at col 1.
        // Row 1: a single cell must land at col 1 (col 0 still occupied by A).
        let t = toks(&[
            "<tr>", "<td", " rowspan=\"2\"", ">", "<td>", "</tr>", "<tr>", "<td>", "</tr>",
        ]);
        let boxes: Vec<BBox> = t.iter().map(|_| b(0.0, 0.0, 1.0, 1.0)).collect();
        let cells = parse_structure_tokens(&t, &boxes);
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0].row_span, 2);
        assert_eq!((cells[0].row, cells[0].col), (0, 0));
        assert_eq!((cells[1].row, cells[1].col), (0, 1));
        // Third cell skips the rowspan-occupied (1,0) and lands at (1,1).
        assert_eq!((cells[2].row, cells[2].col), (1, 1));
    }

    #[test]
    fn cell_bbox_comes_from_opening_token_step() {
        let t = toks(&["<tr>", "<td>", "<td>", "</tr>"]);
        let boxes = vec![
            b(0.0, 0.0, 0.0, 0.0),   // <tr>
            b(10.0, 20.0, 5.0, 6.0), // first <td>
            b(30.0, 40.0, 7.0, 8.0), // second <td>
            b(0.0, 0.0, 0.0, 0.0),   // </tr>
        ];
        let cells = parse_structure_tokens(&t, &boxes);
        assert_eq!(cells[0].bbox, b(10.0, 20.0, 5.0, 6.0));
        assert_eq!(cells[1].bbox, b(30.0, 40.0, 7.0, 8.0));
    }

    #[test]
    fn decode_argmaxes_logits_and_denormalizes_boxes() {
        // Vocab: [0]="<tr>", [1]="<td>", [2]="</tr>", [3]="eos".
        let dict = toks(&["<tr>", "<td>", "</tr>", "eos"]);
        // 3 real steps then eos. logits [1, 4, 4].
        let logits = vec![
            5.0, 0.0, 0.0, 0.0, // <tr>
            0.0, 5.0, 0.0, 0.0, // <td>
            0.0, 0.0, 5.0, 0.0, // </tr>
            0.0, 0.0, 0.0, 5.0, // <eos> -> stop
        ];
        // bbox [1, 4, 4], xyxy normalized. The <td> at step 1 → (0.1,0.2,0.3,0.5).
        let bbox = vec![
            0.0, 0.0, 0.0, 0.0, //
            0.1, 0.2, 0.3, 0.5, //
            0.0, 0.0, 0.0, 0.0, //
            0.0, 0.0, 0.0, 0.0, //
        ];
        let s = decode_slanet(&logits, &[1, 4, 4], &bbox, &[1, 4, 4], &dict, 100.0, 200.0);
        assert_eq!(s.tokens, toks(&["<tr>", "<td>", "</tr>"]));
        assert_eq!(s.cells.len(), 1);
        let c = s.cells[0];
        // 0.1*100=10, 0.2*200=40, w=(0.3-0.1)*100=20, h=(0.5-0.2)*200=60
        assert!((c.bbox.x - 10.0).abs() < 1e-4);
        assert!((c.bbox.y - 40.0).abs() < 1e-4);
        assert!((c.bbox.width - 20.0).abs() < 1e-4);
        assert!((c.bbox.height - 60.0).abs() < 1e-4);
    }

    #[test]
    fn slanet_dict_has_30_tokens_with_sos_and_eos() {
        let d = slanet_char_dict();
        assert_eq!(d.len(), 30, "sos + 28 table tokens + eos");
        assert_eq!(d[0], "sos");
        assert_eq!(d[29], "eos");
        assert_eq!(d[3], "<td>");
        assert_eq!(d[9], "<td");
    }

    #[test]
    fn decode_with_real_slanet_dict_builds_grid_and_stops_at_eos() {
        let dict = slanet_char_dict();
        let v = dict.len(); // 30
        let idx = |tok: &str| dict.iter().position(|t| t == tok).unwrap();
        // Sequence: <tr> <td> <td> </tr> eos  → one row, two cells.
        let seq = ["<tr>", "<td>", "<td>", "</tr>", "eos"];
        let mut logits = vec![0.0f32; seq.len() * v];
        for (t, tok) in seq.iter().enumerate() {
            logits[t * v + idx(tok)] = 9.0;
        }
        let bbox = vec![0.0f32; seq.len() * 4];
        let s = decode_slanet(&logits, &[1, seq.len() as i64, v as i64], &bbox, &[1, seq.len() as i64, 4], &dict, 10.0, 10.0);
        assert_eq!(s.tokens, toks(&["<tr>", "<td>", "<td>", "</tr>"]), "stops at eos");
        assert_eq!(s.cells.len(), 2);
        assert_eq!((s.cells[1].row, s.cells[1].col), (0, 1));
    }

    #[test]
    fn parse_span_attr_handles_quotes_and_spaces() {
        assert_eq!(parse_span_attr(" colspan=\"3\"", "colspan"), Some(3));
        assert_eq!(parse_span_attr("rowspan=\"2\"", "rowspan"), Some(2));
        assert_eq!(parse_span_attr(" colspan=\"3\"", "rowspan"), None);
    }
}
