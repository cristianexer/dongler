import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))

from eval_metrics.grits import (
    _cell_bbox,
    _cell_text,
    _iou,
    factored_2d_similarity,
    grits_con,
    grits_from_similarity,
    grits_loc,
    grits_top,
)


# --------------------------------------------------------------------------- #
# Helpers
# --------------------------------------------------------------------------- #
def test_cell_text_from_str_and_dict():
    assert _cell_text("hi") == "hi"
    assert _cell_text({"text": "hi"}) == "hi"
    assert _cell_text({"bbox": (0, 0, 1, 1)}) == ""
    assert _cell_text(None) == ""


def test_cell_bbox_from_str_and_dict():
    assert _cell_bbox("hi") is None
    assert _cell_bbox({"text": "hi"}) is None
    assert _cell_bbox({"text": "hi", "bbox": (1, 2, 3, 4)}) == (1.0, 2.0, 3.0, 4.0)
    # Malformed bbox is ignored rather than crashing.
    assert _cell_bbox({"bbox": (1, 2, 3)}) is None


def test_iou_basic():
    assert _iou((0, 0, 10, 10), (0, 0, 10, 10)) == 1.0
    assert _iou((0, 0, 10, 10), (20, 20, 10, 10)) == 0.0  # disjoint
    # Half-overlap along x: intersection 50, union 150 => 1/3.
    assert abs(_iou((0, 0, 10, 10), (5, 0, 10, 10)) - (50.0 / 150.0)) < 1e-9
    # Degenerate (zero-area) box has no overlap.
    assert _iou((0, 0, 0, 0), (0, 0, 10, 10)) == 0.0


# --------------------------------------------------------------------------- #
# grits_from_similarity
# --------------------------------------------------------------------------- #
def test_grits_from_similarity_perfect_and_empty():
    assert grits_from_similarity(4.0, 4, 4) == 1.0
    assert grits_from_similarity(0.0, 0, 0) == 0.0  # no division by zero
    # P=1, R=4/6 => F = 0.8.
    assert abs(grits_from_similarity(4.0, 4, 6) - 0.8) < 1e-9


# --------------------------------------------------------------------------- #
# factored_2d_similarity
# --------------------------------------------------------------------------- #
def test_factored_counts_and_total_identity():
    g = [["a", "b"], ["c", "d"]]
    total, n_pred, n_gt = factored_2d_similarity(
        g, g, lambda x, y: 1.0 if _cell_text(x) == _cell_text(y) else 0.0
    )
    assert (n_pred, n_gt) == (4, 4)
    assert total == 4.0


def test_factored_pads_ragged_rows():
    ragged = [["a", "b"], ["c"]]
    total, n_pred, n_gt = factored_2d_similarity(ragged, ragged, lambda x, y: 1.0)
    # 2 rows x width 2 => 4 present positions after padding.
    assert n_pred == 4 and n_gt == 4


def test_factored_empty_grid():
    total, n_pred, n_gt = factored_2d_similarity([], [["a"]], lambda x, y: 1.0)
    assert (total, n_pred, n_gt) == (0.0, 0, 1)


# --------------------------------------------------------------------------- #
# Required GriTS assertions
# --------------------------------------------------------------------------- #
def test_identity_is_one():
    g = [["a", "b"], ["c", "d"]]
    assert grits_con(g, g) == 1.0
    assert grits_top(g, g) == 1.0


def test_top_identity_and_shape_mismatch():
    assert grits_top([["a", "b"], ["c", "d"]], [["a", "b"], ["c", "d"]]) == 1.0
    val = grits_top([["a", "b"], ["c", "d"]], [["a", "b", "x"], ["c", "d", "y"]])
    assert 0.0 < val < 1.0
    # Expected F-measure: P=1.0, R=4/6 => 0.8.
    assert abs(val - 0.8) < 1e-9


def test_con_partial_beats_disjoint():
    partial = grits_con([["cat", "dog"]], [["car", "dog"]])
    disjoint = grits_con([["cat", "dog"]], [["xxx", "yyy"]])
    assert 0.0 < partial < 1.0
    assert disjoint == 0.0
    assert partial > disjoint


def test_results_within_bounds():
    grids = [
        [["a", "b"], ["c", "d"]],
        [["cat", "dog"]],
        [["x"]],
        [["a", "b", "c"], ["d", "e", "f"]],
    ]
    for g in grids:
        assert grits_con(g, g) == 1.0
        assert grits_top(g, g) == 1.0
    for g1 in grids:
        for g2 in grids:
            for fn in (grits_con, grits_top):
                v = fn(g1, g2)
                assert 0.0 <= v <= 1.0


def test_symmetry():
    a = [["cat", "dog"], ["fish", "bird"]]
    b = [["car", "dog"], ["fish", "bird"]]
    assert abs(grits_con(a, b) - grits_con(b, a)) < 1e-9
    assert abs(grits_top(a, b) - grits_top(b, a)) < 1e-9


def test_disjoint_content_is_zero():
    assert grits_con([["abc"]], [["xyz"]]) == 0.0


# --------------------------------------------------------------------------- #
# grits_loc
# --------------------------------------------------------------------------- #
def test_loc_matching_bboxes_is_one():
    g = [[{"text": "a", "bbox": (0, 0, 10, 10)}]]
    assert grits_loc(g, g) == 1.0


def test_loc_none_without_bboxes():
    assert grits_loc([["a", "b"]], [["a", "b"]]) is None


def test_loc_partial_overlap_within_bounds():
    pred = [[{"text": "a", "bbox": (0, 0, 10, 10)}]]
    gt = [[{"text": "a", "bbox": (5, 0, 10, 10)}]]
    val = grits_loc(pred, gt)
    assert val is not None
    assert 0.0 < val < 1.0
    # Single matched cell => F equals the IoU itself (1/3).
    assert abs(val - (50.0 / 150.0)) < 1e-9


def test_loc_missing_bbox_on_one_side_contributes_zero():
    pred = [[{"text": "a", "bbox": (0, 0, 10, 10)}]]
    gt = [[{"text": "a"}]]  # has text but no bbox
    val = grits_loc(pred, gt)
    assert val == 0.0  # bbox present somewhere, but this pair has no overlap


def test_loc_symmetry():
    a = [[{"text": "a", "bbox": (0, 0, 10, 10)}, {"text": "b", "bbox": (10, 0, 10, 10)}]]
    b = [[{"text": "a", "bbox": (1, 0, 10, 10)}, {"text": "b", "bbox": (11, 0, 10, 10)}]]
    assert abs(grits_loc(a, b) - grits_loc(b, a)) < 1e-9
