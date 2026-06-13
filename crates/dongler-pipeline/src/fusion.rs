//! Fusion: assign deterministic text-layer spans to ML-detected layout regions
//! (PRD §4.G). This is the step that makes the hybrid pipeline trustworthy — it
//! snaps region content to real text rather than re-transcribing it, and it
//! enforces the hard invariant that **no span is ever silently dropped**.
//!
//! Geometry is done in a single coordinate space (the caller transforms layout
//! boxes into PDF/IR space first). An R*-tree over region boxes keeps the
//! per-span owner lookup at O(log n).

use crate::geometry::{area, center};
use dongler_core::ir::BBox;
use rstar::{PointDistance, RTree, RTreeObject, AABB};

/// Region priority classes used to resolve overlaps (higher wins), per PRD §4.G:
/// `table > figure > formula > text`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionClass {
    Table,
    Figure,
    Formula,
    Text,
    Other,
}

impl RegionClass {
    /// Higher priority claims a span when regions overlap.
    pub fn priority(self) -> u8 {
        match self {
            RegionClass::Table => 4,
            RegionClass::Figure => 3,
            RegionClass::Formula => 2,
            RegionClass::Text => 1,
            RegionClass::Other => 0,
        }
    }
}

/// A layout region a span can be assigned to.
#[derive(Debug, Clone, Copy)]
pub struct Region {
    /// Caller-assigned identifier (e.g. index into the layout result).
    pub id: usize,
    pub bbox: BBox,
    pub class: RegionClass,
}

/// The result of assigning spans to regions.
#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    /// `span_region[i]` is the region id owning span `i`, or `None` if the span
    /// is an orphan (no containing region and none within the attach radius).
    pub span_region: Vec<Option<usize>>,
    /// Indices of spans that became orphans. Orphans are NOT dropped — the
    /// pipeline emits them as fallback text blocks.
    pub orphans: Vec<usize>,
}

impl Assignment {
    /// The no-drop invariant: every input span is either owned by a region or
    /// recorded as an orphan. Total count is conserved.
    pub fn conserves_spans(&self, span_count: usize) -> bool {
        if self.span_region.len() != span_count {
            return false;
        }
        let owned = self.span_region.iter().filter(|r| r.is_some()).count();
        owned + self.orphans.len() == span_count
    }
}

struct Entry {
    idx: usize,
    bbox: BBox,
}

impl RTreeObject for Entry {
    type Envelope = AABB<[f32; 2]>;
    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(
            [self.bbox.x, self.bbox.y],
            [self.bbox.x + self.bbox.width, self.bbox.y + self.bbox.height],
        )
    }
}

impl PointDistance for Entry {
    fn distance_2(&self, point: &[f32; 2]) -> f32 {
        let cx = point[0].clamp(self.bbox.x, self.bbox.x + self.bbox.width);
        let cy = point[1].clamp(self.bbox.y, self.bbox.y + self.bbox.height);
        let dx = point[0] - cx;
        let dy = point[1] - cy;
        dx * dx + dy * dy
    }
}

/// Assign each span (by its bbox) to the best containing region; spans with no
/// container attach to the nearest region within `attach_radius` (in coordinate
/// units), otherwise become orphans. Never drops a span.
pub fn assign_spans(regions: &[Region], span_boxes: &[BBox], attach_radius: f32) -> Assignment {
    let entries: Vec<Entry> = regions
        .iter()
        .map(|r| Entry {
            idx: r.id,
            bbox: r.bbox,
        })
        .collect();
    // Map region id -> region for priority/area tie-breaking.
    let by_id = |id: usize| regions.iter().find(|r| r.id == id);

    let tree = RTree::bulk_load(
        regions
            .iter()
            .map(|r| Entry {
                idx: r.id,
                bbox: r.bbox,
            })
            .collect(),
    );

    let mut span_region = Vec::with_capacity(span_boxes.len());
    let mut orphans = Vec::new();

    for (i, sb) in span_boxes.iter().enumerate() {
        let (cx, cy) = center(sb);
        // All regions whose box contains the span center.
        let mut best: Option<&Region> = None;
        for e in tree.locate_all_at_point([cx, cy]) {
            if let Some(region) = by_id(e.idx) {
                best = Some(match best {
                    None => region,
                    Some(cur) => pick_better(cur, region),
                });
            }
        }

        if let Some(region) = best {
            span_region.push(Some(region.id));
            continue;
        }

        // No container: attach to nearest region within the radius.
        if let Some(nearest) = tree.nearest_neighbor([cx, cy]) {
            let d2 = nearest.distance_2(&[cx, cy]);
            if d2 <= attach_radius * attach_radius {
                span_region.push(Some(nearest.idx));
                continue;
            }
        }

        span_region.push(None);
        orphans.push(i);
    }

    // Silence unused warning when there are no regions.
    let _ = &entries;
    Assignment {
        span_region,
        orphans,
    }
}

/// Higher priority wins; on a tie the smaller region wins (more specific).
fn pick_better<'a>(a: &'a Region, b: &'a Region) -> &'a Region {
    let pa = a.class.priority();
    let pb = b.class.priority();
    if pb > pa {
        b
    } else if pb < pa {
        a
    } else if area(&b.bbox) < area(&a.bbox) {
        b
    } else {
        a
    }
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
    fn span_assigned_to_containing_region() {
        let regions = vec![Region {
            id: 7,
            bbox: bb(0.0, 0.0, 100.0, 100.0),
            class: RegionClass::Text,
        }];
        let spans = vec![bb(10.0, 10.0, 5.0, 5.0)];
        let a = assign_spans(&regions, &spans, 5.0);
        assert_eq!(a.span_region, vec![Some(7)]);
        assert!(a.orphans.is_empty());
        assert!(a.conserves_spans(1));
    }

    #[test]
    fn table_wins_over_overlapping_text() {
        // A table region nested inside a big text region; a span in the overlap
        // must go to the table (higher priority).
        let regions = vec![
            Region {
                id: 0,
                bbox: bb(0.0, 0.0, 200.0, 200.0),
                class: RegionClass::Text,
            },
            Region {
                id: 1,
                bbox: bb(50.0, 50.0, 50.0, 50.0),
                class: RegionClass::Table,
            },
        ];
        let spans = vec![bb(60.0, 60.0, 4.0, 4.0)];
        let a = assign_spans(&regions, &spans, 5.0);
        assert_eq!(a.span_region, vec![Some(1)]);
    }

    #[test]
    fn nearby_orphan_attaches_within_radius() {
        let regions = vec![Region {
            id: 3,
            bbox: bb(0.0, 0.0, 100.0, 100.0),
            class: RegionClass::Text,
        }];
        // center at (105,50): 5 units to the right edge → within radius 10.
        let spans = vec![bb(103.0, 48.0, 4.0, 4.0)];
        let a = assign_spans(&regions, &spans, 10.0);
        assert_eq!(a.span_region, vec![Some(3)]);
        assert!(a.orphans.is_empty());
    }

    #[test]
    fn far_span_becomes_orphan_but_is_not_dropped() {
        let regions = vec![Region {
            id: 0,
            bbox: bb(0.0, 0.0, 10.0, 10.0),
            class: RegionClass::Text,
        }];
        let spans = vec![bb(500.0, 500.0, 4.0, 4.0)];
        let a = assign_spans(&regions, &spans, 5.0);
        assert_eq!(a.span_region, vec![None]);
        assert_eq!(a.orphans, vec![0]);
        assert!(a.conserves_spans(1)); // conserved: orphaned, not dropped
    }

    #[test]
    fn no_regions_orphans_everything_without_dropping() {
        let spans = vec![bb(0.0, 0.0, 1.0, 1.0), bb(5.0, 5.0, 1.0, 1.0)];
        let a = assign_spans(&[], &spans, 5.0);
        assert_eq!(a.orphans, vec![0, 1]);
        assert!(a.conserves_spans(2));
    }

    #[test]
    fn conservation_holds_on_mixed_input() {
        let regions = vec![
            Region {
                id: 0,
                bbox: bb(0.0, 0.0, 50.0, 50.0),
                class: RegionClass::Text,
            },
            Region {
                id: 1,
                bbox: bb(100.0, 100.0, 50.0, 50.0),
                class: RegionClass::Figure,
            },
        ];
        let spans = vec![
            bb(10.0, 10.0, 2.0, 2.0),   // in region 0
            bb(120.0, 120.0, 2.0, 2.0), // in region 1
            bb(300.0, 300.0, 2.0, 2.0), // orphan
        ];
        let a = assign_spans(&regions, &spans, 5.0);
        assert!(a.conserves_spans(3));
        assert_eq!(a.span_region[0], Some(0));
        assert_eq!(a.span_region[1], Some(1));
        assert_eq!(a.span_region[2], None);
    }
}
