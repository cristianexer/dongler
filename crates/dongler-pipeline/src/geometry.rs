//! Small axis-aligned-bounding-box helpers shared by reading order and fusion.
//!
//! All functions operate on [`dongler_core::ir::BBox`] (`x`, `y`, `width`,
//! `height`). The *coordinate convention is the caller's responsibility*:
//! reading order ([`crate::order`]) expects "screen" coordinates (y grows
//! downward, so the top of the page is the smallest `y`), while the PDF text
//! layer is bottom-left origin — the pipeline flips y once before ordering.

use dongler_core::ir::BBox;

/// Left edge.
#[inline]
pub fn left(b: &BBox) -> f32 {
    b.x
}

/// Right edge.
#[inline]
pub fn right(b: &BBox) -> f32 {
    b.x + b.width
}

/// Top edge (smaller value in screen coordinates).
#[inline]
pub fn top(b: &BBox) -> f32 {
    b.y
}

/// Bottom edge (larger value in screen coordinates).
#[inline]
pub fn bottom(b: &BBox) -> f32 {
    b.y + b.height
}

/// Center point `(cx, cy)`.
#[inline]
pub fn center(b: &BBox) -> (f32, f32) {
    (b.x + b.width / 2.0, b.y + b.height / 2.0)
}

/// Area (clamped to be non-negative).
#[inline]
pub fn area(b: &BBox) -> f32 {
    (b.width.max(0.0)) * (b.height.max(0.0))
}

/// Whether `(px, py)` lies inside `b` (inclusive of edges).
#[inline]
pub fn contains_point(b: &BBox, px: f32, py: f32) -> bool {
    px >= left(b) && px <= right(b) && py >= top(b) && py <= bottom(b)
}

/// Area of the intersection of two boxes (0 if disjoint).
pub fn intersection_area(a: &BBox, b: &BBox) -> f32 {
    let ix = (right(a).min(right(b)) - left(a).max(left(b))).max(0.0);
    let iy = (bottom(a).min(bottom(b)) - top(a).max(top(b))).max(0.0);
    ix * iy
}

/// Intersection-over-union of two boxes.
pub fn iou(a: &BBox, b: &BBox) -> f32 {
    let inter = intersection_area(a, b);
    let union = area(a) + area(b) - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Fraction of `inner` covered by `outer` (1.0 == fully contained).
pub fn coverage(outer: &BBox, inner: &BBox) -> f32 {
    let inner_area = area(inner);
    if inner_area <= 0.0 {
        0.0
    } else {
        intersection_area(outer, inner) / inner_area
    }
}

/// Squared euclidean distance between the centers of two boxes (cheap nearest
/// metric for orphan attachment).
pub fn center_distance_sq(a: &BBox, b: &BBox) -> f32 {
    let (ax, ay) = center(a);
    let (bx, by) = center(b);
    let dx = ax - bx;
    let dy = ay - by;
    dx * dx + dy * dy
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
    fn contains_point_inclusive_edges() {
        let b = bb(0.0, 0.0, 10.0, 10.0);
        assert!(contains_point(&b, 5.0, 5.0));
        assert!(contains_point(&b, 0.0, 0.0));
        assert!(contains_point(&b, 10.0, 10.0));
        assert!(!contains_point(&b, 11.0, 5.0));
    }

    #[test]
    fn iou_of_identical_is_one() {
        let b = bb(1.0, 2.0, 4.0, 4.0);
        assert!((iou(&b, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn iou_of_disjoint_is_zero() {
        assert_eq!(iou(&bb(0.0, 0.0, 1.0, 1.0), &bb(5.0, 5.0, 1.0, 1.0)), 0.0);
    }

    #[test]
    fn coverage_full_and_partial() {
        let outer = bb(0.0, 0.0, 10.0, 10.0);
        let inner = bb(2.0, 2.0, 2.0, 2.0);
        assert!((coverage(&outer, &inner) - 1.0).abs() < 1e-6);
        let half_out = bb(9.0, 0.0, 2.0, 2.0); // half overlaps
        assert!((coverage(&outer, &half_out) - 0.5).abs() < 1e-6);
    }
}
