//! Reading-order determination by recursive XY-cut with the XY-Cut++ refinement
//! of pre-extracting full-width "cross-layout" elements (page headers/footers,
//! full-width figures) before cutting.
//!
//! Per PRD §4.H this replaces the legacy 2-column-only heuristic. It operates on
//! detected/typed blocks (a handful per page), not raw lines, which is far more
//! robust. Input boxes are in **screen coordinates** (y grows downward: the top
//! of the page is the smallest `y`); the pipeline flips the bottom-left-origin
//! PDF boxes once before calling this.
//!
//! The result is the permutation of input indices in reading order.

use crate::geometry::{bottom, left, right, top};
use dongler_core::ir::BBox;

/// Minimum whitespace gap (in the box's own units, typically PDF points) for a
/// cut to be considered meaningful.
const MIN_GAP: f32 = 2.0;

/// Fraction of the region width a box must span to be treated as a full-width
/// cross-layout element and pulled out of the column structure first.
const FULL_WIDTH_RATIO: f32 = 0.85;

/// Returns the input indices `0..boxes.len()` permuted into reading order.
pub fn reading_order(boxes: &[BBox]) -> Vec<usize> {
    let indices: Vec<usize> = (0..boxes.len()).collect();
    let mut out = Vec::with_capacity(boxes.len());
    recurse(boxes, &indices, &mut out);
    out
}

fn recurse(boxes: &[BBox], indices: &[usize], out: &mut Vec<usize>) {
    if indices.len() <= 1 {
        out.extend_from_slice(indices);
        return;
    }

    let region_left = fold_min(indices.iter().map(|&i| left(&boxes[i])));
    let region_right = fold_max(indices.iter().map(|&i| right(&boxes[i])));
    let region_width = (region_right - region_left).max(0.0);

    // Sort top-to-bottom (ties left-to-right) for the spanning-element pass.
    let mut sorted = indices.to_vec();
    sorted.sort_by(|&a, &b| {
        top(&boxes[a])
            .partial_cmp(&top(&boxes[b]))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                left(&boxes[a])
                    .partial_cmp(&left(&boxes[b]))
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });

    // XY-Cut++ refinement: peel full-width elements off the top (emitted first)
    // and the bottom (emitted last) so they don't block a vertical column cut.
    let is_full_width =
        |i: usize| region_width > 0.0 && boxes[i].width >= FULL_WIDTH_RATIO * region_width;

    let mut lead = 0;
    while lead < sorted.len() && is_full_width(sorted[lead]) {
        lead += 1;
    }
    let mut tail = sorted.len();
    while tail > lead && is_full_width(sorted[tail - 1]) {
        tail -= 1;
    }

    if lead > 0 || tail < sorted.len() {
        // We removed at least one spanning box → recurse on the strictly smaller
        // middle, preserving order: leading spanners, middle, trailing spanners.
        out.extend_from_slice(&sorted[..lead]);
        let middle = &sorted[lead..tail];
        if !middle.is_empty() {
            recurse(boxes, middle, out);
        }
        out.extend_from_slice(&sorted[tail..]);
        return;
    }

    // No spanning elements: choose the larger of the best horizontal / vertical
    // whitespace gap and cut along it (classic XY-cut).
    let h = best_gap(indices.iter().map(|&i| (top(&boxes[i]), bottom(&boxes[i]))));
    let v = best_gap(indices.iter().map(|&i| (left(&boxes[i]), right(&boxes[i]))));

    match (h, v) {
        (Some((hgap, hpos)), v_opt)
            if hgap >= MIN_GAP && hgap >= v_opt.map(|(g, _)| g).unwrap_or(0.0) =>
        {
            // Horizontal cut: top band before bottom band.
            let (above, below): (Vec<usize>, Vec<usize>) =
                indices.iter().partition(|&&i| bottom(&boxes[i]) <= hpos);
            recurse(boxes, &above, out);
            recurse(boxes, &below, out);
        }
        (_, Some((vgap, vpos))) if vgap >= MIN_GAP => {
            // Vertical cut: left column before right column.
            let (lhs, rhs): (Vec<usize>, Vec<usize>) =
                indices.iter().partition(|&&i| right(&boxes[i]) <= vpos);
            recurse(boxes, &lhs, out);
            recurse(boxes, &rhs, out);
        }
        _ => {
            // No meaningful gap: emit in top-then-left order.
            out.extend_from_slice(&sorted);
        }
    }
}

/// Largest gap in the union of 1-D intervals `[lo, hi]`. Returns `(gap, position)`
/// where `position` is the midpoint of the empty band, or `None` if the
/// intervals form a single contiguous run (no gap).
fn best_gap(intervals: impl Iterator<Item = (f32, f32)>) -> Option<(f32, f32)> {
    let mut ivals: Vec<(f32, f32)> = intervals.collect();
    if ivals.len() < 2 {
        return None;
    }
    ivals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut cur_hi = ivals[0].1;
    let mut best: Option<(f32, f32)> = None;
    for &(lo, hi) in &ivals[1..] {
        if lo > cur_hi {
            let gap = lo - cur_hi;
            let pos = (cur_hi + lo) / 2.0;
            if best.map(|(g, _)| gap > g).unwrap_or(true) {
                best = Some((gap, pos));
            }
        }
        if hi > cur_hi {
            cur_hi = hi;
        }
    }
    best
}

fn fold_min(it: impl Iterator<Item = f32>) -> f32 {
    it.fold(f32::INFINITY, f32::min)
}

fn fold_max(it: impl Iterator<Item = f32>) -> f32 {
    it.fold(f32::NEG_INFINITY, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bb(x: f32, y: f32, w: f32, h: f32) -> BBox {
        BBox {
            x,
            y,
            width: w,
            height: h,
        }
    }

    #[test]
    fn single_column_orders_top_to_bottom() {
        // Provided out of order; expect top→bottom.
        let boxes = vec![
            bb(0.0, 100.0, 100.0, 20.0), // middle
            bb(0.0, 0.0, 100.0, 20.0),   // top
            bb(0.0, 200.0, 100.0, 20.0), // bottom
        ];
        assert_eq!(reading_order(&boxes), vec![1, 0, 2]);
    }

    #[test]
    fn two_columns_read_left_then_right() {
        // Left column x[0,100], right column x[200,300]; each two rows.
        let boxes = vec![
            bb(200.0, 0.0, 100.0, 20.0),   // 0 right-top
            bb(0.0, 0.0, 100.0, 20.0),     // 1 left-top
            bb(200.0, 50.0, 100.0, 20.0),  // 2 right-bottom
            bb(0.0, 50.0, 100.0, 20.0),    // 3 left-bottom
        ];
        // left-top, left-bottom, right-top, right-bottom
        assert_eq!(reading_order(&boxes), vec![1, 3, 0, 2]);
    }

    #[test]
    fn full_width_header_then_two_columns() {
        let boxes = vec![
            bb(0.0, 0.0, 300.0, 20.0),    // 0 header (full width)
            bb(200.0, 100.0, 100.0, 20.0), // 1 right-top
            bb(0.0, 100.0, 100.0, 20.0),   // 2 left-top
            bb(200.0, 150.0, 100.0, 20.0), // 3 right-bottom
            bb(0.0, 150.0, 100.0, 20.0),   // 4 left-bottom
        ];
        // header, then left column, then right column
        assert_eq!(reading_order(&boxes), vec![0, 2, 4, 1, 3]);
    }

    #[test]
    fn header_columns_and_footer() {
        let boxes = vec![
            bb(0.0, 300.0, 300.0, 20.0),   // 0 footer (full width, bottom)
            bb(0.0, 0.0, 300.0, 20.0),     // 1 header (full width, top)
            bb(0.0, 100.0, 100.0, 20.0),   // 2 left
            bb(200.0, 100.0, 100.0, 20.0), // 3 right
        ];
        assert_eq!(reading_order(&boxes), vec![1, 2, 3, 0]);
    }

    #[test]
    fn empty_and_singleton() {
        assert_eq!(reading_order(&[]), Vec::<usize>::new());
        assert_eq!(reading_order(&[bb(0.0, 0.0, 1.0, 1.0)]), vec![0]);
    }

    #[test]
    fn permutation_is_total_and_unique() {
        // Three stacked rows of two columns, scrambled.
        let mut boxes = Vec::new();
        for row in 0..3 {
            boxes.push(bb(0.0, row as f32 * 50.0, 100.0, 20.0));
            boxes.push(bb(200.0, row as f32 * 50.0, 100.0, 20.0));
        }
        let order = reading_order(&boxes);
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..boxes.len()).collect::<Vec<_>>());
    }
}
