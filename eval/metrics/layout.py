"""Bounding-box detection metrics (COCO-style) for dongler layout evaluation.

Boxes use the same top-left-origin convention as dongler's ``BBox``:
``(x, y, width, height)``. A box may be supplied either as an ``(x, y, w, h)``
sequence or as a ``{"x", "y", "width", "height"}`` mapping; both are normalized
through :func:`_as_xywh`.

Detections (for :func:`match_detections`, :func:`average_precision`,
:func:`mean_average_precision`) are dicts:
- predictions: ``{"box": <box>, "score": float, "label": <hashable, default 0>}``
- ground truth: ``{"box": <box>, "label": <hashable, default 0>}``

Pure stdlib, dependency-free.
"""
from __future__ import annotations

from typing import Dict, List, Mapping, Optional, Sequence, Tuple, Union

Number = Union[int, float]
BoxLike = Union[Sequence[Number], Mapping[str, Number]]
Detection = Mapping[str, object]


def _as_xywh(box: BoxLike) -> Tuple[float, float, float, float]:
    """Normalize a box to an ``(x, y, w, h)`` tuple of floats.

    Accepts a 4-element sequence ``(x, y, w, h)`` or a mapping with keys
    ``x``, ``y``, ``width``, ``height``.
    """
    if isinstance(box, Mapping):
        return (
            float(box["x"]),
            float(box["y"]),
            float(box["width"]),
            float(box["height"]),
        )
    x, y, w, h = box
    return (float(x), float(y), float(w), float(h))


def _label(det: Detection) -> object:
    """Return a detection's label, defaulting to ``0`` when absent."""
    return det.get("label", 0)  # type: ignore[attr-defined]


def _score(det: Detection) -> float:
    """Return a detection's confidence score, defaulting to ``0.0``."""
    return float(det.get("score", 0.0))  # type: ignore[attr-defined]


def iou(box_a: BoxLike, box_b: BoxLike) -> float:
    """Intersection-over-union of two ``xywh`` boxes.

    Returns ``0.0`` if either box has non-positive area or the boxes do not
    overlap. ``iou(b, b) == 1.0`` for any box of positive area. Result lies in
    ``[0, 1]``.
    """
    ax, ay, aw, ah = _as_xywh(box_a)
    bx, by, bw, bh = _as_xywh(box_b)
    if aw <= 0 or ah <= 0 or bw <= 0 or bh <= 0:
        return 0.0

    ix1 = max(ax, bx)
    iy1 = max(ay, by)
    ix2 = min(ax + aw, bx + bw)
    iy2 = min(ay + ah, by + bh)
    iw = ix2 - ix1
    ih = iy2 - iy1
    if iw <= 0 or ih <= 0:
        return 0.0

    inter = iw * ih
    union = aw * ah + bw * bh - inter
    if union <= 0:
        return 0.0
    return inter / union


def mean_best_iou(
    pred_boxes: Sequence[BoxLike],
    gt_boxes: Sequence[BoxLike],
) -> Optional[float]:
    """Recall-oriented coverage: average over ground truth of best pred IoU.

    For each ground-truth box, take the maximum IoU against any predicted box
    (``0.0`` when there are no predictions), then average over all ground-truth
    boxes. Returns ``None`` when ``gt_boxes`` is empty (the average is
    undefined). Result lies in ``[0, 1]``.
    """
    if not gt_boxes:
        return None
    total = 0.0
    for gt in gt_boxes:
        best = 0.0
        for pred in pred_boxes:
            cur = iou(pred, gt)
            if cur > best:
                best = cur
        total += best
    return total / len(gt_boxes)


def _greedy_match_class(
    preds: Sequence[Detection],
    gts: Sequence[Detection],
    iou_threshold: float,
) -> Tuple[List[bool], int]:
    """Greedily match predictions to ground truth within a single class.

    Predictions are processed in descending score order; each matches the
    highest-IoU unmatched ground-truth box with IoU ``>= iou_threshold``.

    Returns ``(flags, num_fn)`` where ``flags`` lists ``True`` (true positive)
    or ``False`` (false positive) per prediction in score-desc order, and
    ``num_fn`` is the number of unmatched ground-truth boxes.
    """
    pred_boxes = [_as_xywh(p["box"]) for p in preds]  # type: ignore[index]
    gt_boxes = [_as_xywh(g["box"]) for g in gts]  # type: ignore[index]
    order = sorted(range(len(preds)), key=lambda i: _score(preds[i]), reverse=True)

    matched = [False] * len(gt_boxes)
    flags: List[bool] = []
    for i in order:
        best_iou = 0.0
        best_j = -1
        for j, gb in enumerate(gt_boxes):
            if matched[j]:
                continue
            cur = iou(pred_boxes[i], gb)
            if cur > best_iou:
                best_iou = cur
                best_j = j
        if best_j >= 0 and best_iou >= iou_threshold:
            matched[best_j] = True
            flags.append(True)
        else:
            flags.append(False)
    return flags, matched.count(False)


def match_detections(
    preds: Sequence[Detection],
    gts: Sequence[Detection],
    iou_threshold: float,
) -> Tuple[int, int, int]:
    """Greedily match predictions to ground truth across all classes.

    Predictions only match ground-truth boxes of the *same* label. Within each
    label, predictions are taken in descending score order and matched to the
    highest-IoU unmatched ground-truth box with IoU ``>= iou_threshold`` (a true
    positive); otherwise the prediction is a false positive. Unmatched ground
    truth boxes are false negatives.

    Returns ``(tp, fp, fn)``.
    """
    labels = {_label(p) for p in preds} | {_label(g) for g in gts}
    tp = fp = fn = 0
    for lab in labels:
        p = [x for x in preds if _label(x) == lab]
        g = [x for x in gts if _label(x) == lab]
        flags, num_fn = _greedy_match_class(p, g, iou_threshold)
        tp += sum(1 for f in flags if f)
        fp += sum(1 for f in flags if not f)
        fn += num_fn
    return tp, fp, fn


def _pr_area(recalls: Sequence[float], precisions: Sequence[float]) -> float:
    """All-points area under the precision-recall curve (VOC/COCO style).

    The precision envelope is made monotonically non-increasing from the right
    before integrating over recall.
    """
    mrec = [0.0] + list(recalls) + [1.0]
    mpre = [0.0] + list(precisions) + [0.0]
    for i in range(len(mpre) - 1, 0, -1):
        mpre[i - 1] = max(mpre[i - 1], mpre[i])
    ap = 0.0
    for i in range(1, len(mrec)):
        if mrec[i] != mrec[i - 1]:
            ap += (mrec[i] - mrec[i - 1]) * mpre[i]
    return ap


def _class_ap(
    preds: Sequence[Detection],
    gts: Sequence[Detection],
    iou_threshold: float,
) -> float:
    """Average precision for a single class at one IoU threshold."""
    n_gt = len(gts)
    if n_gt == 0 or not preds:
        return 0.0
    flags, _ = _greedy_match_class(preds, gts, iou_threshold)
    cum_tp = 0
    cum_fp = 0
    recalls: List[float] = []
    precisions: List[float] = []
    for is_tp in flags:
        if is_tp:
            cum_tp += 1
        else:
            cum_fp += 1
        recalls.append(cum_tp / n_gt)
        precisions.append(cum_tp / (cum_tp + cum_fp))
    return _pr_area(recalls, precisions)


def average_precision(
    preds: Sequence[Detection],
    gts: Sequence[Detection],
    iou_threshold: float = 0.5,
) -> float:
    """Average precision over a single detection set at one IoU threshold.

    Computes per-class AP (all-points PR interpolation) over the classes present
    in the ground truth, then averages across those classes. Returns ``1.0`` for
    perfect detections and ``0.0`` when there is no ground truth. Result lies in
    ``[0, 1]``.
    """
    if not gts:
        return 0.0
    labels = sorted({_label(g) for g in gts}, key=repr)
    aps = []
    for lab in labels:
        p = [x for x in preds if _label(x) == lab]
        g = [x for x in gts if _label(x) == lab]
        aps.append(_class_ap(p, g, iou_threshold))
    return sum(aps) / len(aps)


def mean_average_precision(
    preds: Sequence[Detection],
    gts: Sequence[Detection],
    thresholds: Optional[Sequence[float]] = None,
) -> float:
    """COCO mAP@[.5:.95]: AP averaged over IoU thresholds (and classes).

    Defaults to thresholds ``[0.50, 0.55, ..., 0.95]``. Returns ``1.0`` for
    perfect detections, ``0.0`` when there is no ground truth or no thresholds.
    Result lies in ``[0, 1]``.
    """
    if thresholds is None:
        thresholds = [round(0.50 + 0.05 * i, 2) for i in range(10)]
    if not thresholds:
        return 0.0
    return sum(average_precision(preds, gts, t) for t in thresholds) / len(thresholds)
