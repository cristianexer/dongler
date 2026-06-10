import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))

from eval_metrics.layout import (
    _as_xywh,
    average_precision,
    iou,
    match_detections,
    mean_average_precision,
    mean_best_iou,
)


# --------------------------------------------------------------------------- #
# _as_xywh
# --------------------------------------------------------------------------- #
def test_as_xywh_from_tuple():
    assert _as_xywh((1, 2, 3, 4)) == (1.0, 2.0, 3.0, 4.0)


def test_as_xywh_from_dict():
    box = {"x": 1, "y": 2, "width": 3, "height": 4}
    assert _as_xywh(box) == (1.0, 2.0, 3.0, 4.0)


def test_as_xywh_dict_and_tuple_agree():
    assert _as_xywh((5, 6, 7, 8)) == _as_xywh(
        {"x": 5, "y": 6, "width": 7, "height": 8}
    )


# --------------------------------------------------------------------------- #
# iou (required reference values)
# --------------------------------------------------------------------------- #
def test_iou_identical_is_one():
    assert iou((0, 0, 10, 10), (0, 0, 10, 10)) == 1.0


def test_iou_self_is_one():
    assert iou((3, 4, 7, 9), (3, 4, 7, 9)) == 1.0


def test_iou_half_overlap_reference():
    # intersection 50, union 150 => 1/3
    assert abs(iou((0, 0, 10, 10), (5, 0, 10, 10)) - (50 / 150)) < 1e-9


def test_iou_disjoint_is_zero():
    assert iou((0, 0, 10, 10), (20, 20, 5, 5)) == 0.0


def test_iou_dict_input_matches_tuple():
    a = {"x": 0, "y": 0, "width": 10, "height": 10}
    b = {"x": 5, "y": 0, "width": 10, "height": 10}
    assert abs(iou(a, b) - (50 / 150)) < 1e-9


def test_iou_symmetric():
    a = (0, 0, 10, 10)
    b = (5, 0, 10, 10)
    assert iou(a, b) == iou(b, a)


def test_iou_zero_area_box_is_zero():
    assert iou((0, 0, 0, 10), (0, 0, 10, 10)) == 0.0
    assert iou((0, 0, 10, 10), (0, 0, 10, 0)) == 0.0


def test_iou_negative_area_box_is_zero():
    assert iou((0, 0, -5, 10), (0, 0, 10, 10)) == 0.0


def test_iou_touching_edges_no_overlap():
    # boxes share only an edge => zero area intersection
    assert iou((0, 0, 10, 10), (10, 0, 10, 10)) == 0.0


def test_iou_contained_box():
    # small box fully inside large box: inter=25, union=100 => 0.25
    assert abs(iou((0, 0, 10, 10), (0, 0, 5, 5)) - 0.25) < 1e-9


def test_iou_bounds():
    boxes = [(0, 0, 10, 10), (5, 5, 10, 10), (3, 1, 4, 9), (20, 20, 2, 2)]
    for a in boxes:
        for b in boxes:
            assert 0.0 <= iou(a, b) <= 1.0


# --------------------------------------------------------------------------- #
# mean_best_iou (required reference values)
# --------------------------------------------------------------------------- #
def test_mean_best_iou_perfect():
    assert mean_best_iou([(0, 0, 10, 10)], [(0, 0, 10, 10)]) == 1.0


def test_mean_best_iou_no_preds_is_zero():
    assert mean_best_iou([], [(0, 0, 10, 10)]) == 0.0


def test_mean_best_iou_empty_gt_is_none():
    assert mean_best_iou([(0, 0, 10, 10)], []) is None
    assert mean_best_iou([], []) is None


def test_mean_best_iou_averages_over_gt():
    preds = [(0, 0, 10, 10)]
    gts = [(0, 0, 10, 10), (100, 100, 10, 10)]  # one covered, one not
    # (1.0 + 0.0) / 2
    assert abs(mean_best_iou(preds, gts) - 0.5) < 1e-9


def test_mean_best_iou_takes_best_of_many_preds():
    preds = [(50, 50, 10, 10), (0, 0, 10, 10)]
    gts = [(0, 0, 10, 10)]
    assert mean_best_iou(preds, gts) == 1.0


def test_mean_best_iou_bounds():
    preds = [(0, 0, 10, 10), (5, 5, 3, 3)]
    gts = [(0, 0, 10, 10), (5, 0, 10, 10), (99, 99, 1, 1)]
    val = mean_best_iou(preds, gts)
    assert val is not None
    assert 0.0 <= val <= 1.0


# --------------------------------------------------------------------------- #
# match_detections (required reference values)
# --------------------------------------------------------------------------- #
def test_match_detections_perfect_plus_spurious():
    preds = [
        {"box": (0, 0, 10, 10), "score": 0.9, "label": 0},
        {"box": (50, 50, 10, 10), "score": 0.8, "label": 0},  # spurious
    ]
    gts = [{"box": (0, 0, 10, 10), "label": 0}]
    assert match_detections(preds, gts, 0.5) == (1, 1, 0)


def test_match_detections_spurious_high_score_first():
    # spurious pred outscores the good one; result must be unchanged
    preds = [
        {"box": (50, 50, 10, 10), "score": 0.99, "label": 0},  # spurious
        {"box": (0, 0, 10, 10), "score": 0.5, "label": 0},
    ]
    gts = [{"box": (0, 0, 10, 10), "label": 0}]
    assert match_detections(preds, gts, 0.5) == (1, 1, 0)


def test_match_detections_all_correct():
    preds = [
        {"box": (0, 0, 10, 10), "score": 0.9, "label": 0},
        {"box": (50, 50, 10, 10), "score": 0.8, "label": 0},
    ]
    gts = [
        {"box": (0, 0, 10, 10), "label": 0},
        {"box": (50, 50, 10, 10), "label": 0},
    ]
    assert match_detections(preds, gts, 0.5) == (2, 0, 0)


def test_match_detections_no_preds_all_fn():
    gts = [{"box": (0, 0, 10, 10), "label": 0}, {"box": (5, 0, 10, 10), "label": 0}]
    assert match_detections([], gts, 0.5) == (0, 0, 2)


def test_match_detections_no_gts_all_fp():
    preds = [
        {"box": (0, 0, 10, 10), "score": 0.9, "label": 0},
        {"box": (5, 0, 10, 10), "score": 0.8, "label": 0},
    ]
    assert match_detections(preds, [], 0.5) == (0, 2, 0)


def test_match_detections_label_mismatch_is_fp_and_fn():
    preds = [{"box": (0, 0, 10, 10), "score": 0.9, "label": 1}]
    gts = [{"box": (0, 0, 10, 10), "label": 0}]
    # wrong label => not a match: 1 FP, 1 FN
    assert match_detections(preds, gts, 0.5) == (0, 1, 1)


def test_match_detections_default_label():
    # missing label on pred defaults to 0 and matches gt label 0
    preds = [{"box": (0, 0, 10, 10), "score": 0.9}]
    gts = [{"box": (0, 0, 10, 10), "label": 0}]
    assert match_detections(preds, gts, 0.5) == (1, 0, 0)


def test_match_detections_one_gt_two_overlapping_preds():
    # only one pred can claim the gt; the other becomes FP
    preds = [
        {"box": (0, 0, 10, 10), "score": 0.9, "label": 0},
        {"box": (1, 1, 10, 10), "score": 0.8, "label": 0},
    ]
    gts = [{"box": (0, 0, 10, 10), "label": 0}]
    assert match_detections(preds, gts, 0.5) == (1, 1, 0)


def test_match_detections_below_threshold_is_fp():
    # small overlap below threshold
    preds = [{"box": (0, 0, 10, 10), "score": 0.9, "label": 0}]
    gts = [{"box": (8, 0, 10, 10), "label": 0}]  # IoU = 2*10 / (200-20) = 1/9
    assert match_detections(preds, gts, 0.5) == (0, 1, 1)


# --------------------------------------------------------------------------- #
# average_precision (required reference values)
# --------------------------------------------------------------------------- #
def _perfect_set():
    boxes = [(0, 0, 10, 10), (50, 50, 10, 10), (100, 0, 5, 5)]
    preds = [{"box": b, "score": 1.0, "label": 0} for b in boxes]
    gts = [{"box": b, "label": 0} for b in boxes]
    return preds, gts


def test_average_precision_perfect_is_one():
    preds, gts = _perfect_set()
    assert average_precision(preds, gts) == 1.0


def test_average_precision_single_perfect():
    preds = [{"box": (0, 0, 10, 10), "score": 1.0, "label": 0}]
    gts = [{"box": (0, 0, 10, 10), "label": 0}]
    assert average_precision(preds, gts, 0.5) == 1.0


def test_average_precision_no_gts_is_zero():
    preds = [{"box": (0, 0, 10, 10), "score": 1.0, "label": 0}]
    assert average_precision(preds, []) == 0.0


def test_average_precision_no_preds_is_zero():
    gts = [{"box": (0, 0, 10, 10), "label": 0}]
    assert average_precision([], gts) == 0.0


def test_average_precision_half_missing_recall():
    # one gt detected perfectly, one gt completely missed: precision 1, recall 0.5
    preds = [{"box": (0, 0, 10, 10), "score": 0.9, "label": 0}]
    gts = [
        {"box": (0, 0, 10, 10), "label": 0},
        {"box": (100, 100, 10, 10), "label": 0},
    ]
    assert abs(average_precision(preds, gts, 0.5) - 0.5) < 1e-9


def test_average_precision_multi_class_average():
    # class 0 perfect (AP 1.0), class 1 missed entirely (AP 0.0) => mean 0.5
    preds = [{"box": (0, 0, 10, 10), "score": 0.9, "label": 0}]
    gts = [
        {"box": (0, 0, 10, 10), "label": 0},
        {"box": (50, 50, 10, 10), "label": 1},
    ]
    assert abs(average_precision(preds, gts, 0.5) - 0.5) < 1e-9


def test_average_precision_bounds():
    preds = [
        {"box": (0, 0, 10, 10), "score": 0.9, "label": 0},
        {"box": (40, 40, 10, 10), "score": 0.3, "label": 0},  # spurious
    ]
    gts = [
        {"box": (0, 0, 10, 10), "label": 0},
        {"box": (50, 50, 10, 10), "label": 0},
    ]
    ap = average_precision(preds, gts, 0.5)
    assert 0.0 <= ap <= 1.0


# --------------------------------------------------------------------------- #
# mean_average_precision (required reference values)
# --------------------------------------------------------------------------- #
def test_mean_average_precision_perfect_is_one():
    preds, gts = _perfect_set()
    assert mean_average_precision(preds, gts) == 1.0


def test_mean_average_precision_no_gts_is_zero():
    preds = [{"box": (0, 0, 10, 10), "score": 1.0, "label": 0}]
    assert mean_average_precision(preds, []) == 0.0


def test_mean_average_precision_custom_thresholds():
    preds, gts = _perfect_set()
    assert mean_average_precision(preds, gts, thresholds=[0.5, 0.75]) == 1.0


def test_mean_average_precision_empty_thresholds_is_zero():
    preds, gts = _perfect_set()
    assert mean_average_precision(preds, gts, thresholds=[]) == 0.0


def test_mean_average_precision_bounds():
    preds = [
        {"box": (0, 0, 10, 10), "score": 0.9, "label": 0},
        {"box": (3, 3, 10, 10), "score": 0.4, "label": 0},
    ]
    gts = [{"box": (0, 0, 10, 10), "label": 0}, {"box": (60, 60, 10, 10), "label": 0}]
    val = mean_average_precision(preds, gts)
    assert 0.0 <= val <= 1.0


def test_mean_average_precision_default_threshold_count():
    # tighter thresholds reduce AP for an imperfect box; mAP stays in-range and
    # below the loose-threshold AP.
    preds = [{"box": (0, 0, 10, 10), "score": 0.9, "label": 0}]
    gts = [{"box": (1, 0, 10, 10), "label": 0}]  # IoU = 9*10 / (200-90) = 90/110
    loose = average_precision(preds, gts, 0.5)
    coco = mean_average_precision(preds, gts)
    assert 0.0 <= coco <= loose <= 1.0
