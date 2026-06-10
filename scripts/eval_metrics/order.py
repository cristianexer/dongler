"""Reading-order metrics (dependency-free, stdlib only).

Every metric here compares two *orderings* of a set of element ids: a predicted
sequence and a ground-truth sequence. Element ids may be any hashable value and
are treated as opaque tokens.

When the two inputs describe different id sets, the rank-based metrics
(:func:`reading_order_ard`, :func:`reading_order_score`, :func:`kendall_tau`,
:func:`normalized_kendall_tau`) restrict the comparison to the *intersection* of
the two sets and document a degenerate value for the empty-intersection case.
:func:`sequence_bleu` instead follows the usual BLEU contract and compares the
raw token sequences without any intersection restriction.

Provided metrics:
- ``reading_order_ard``        — Average Relative Distance (lower is better).
- ``reading_order_score``      — ``1 - ARD`` (higher is better).
- ``sequence_bleu``            — n-gram BLEU with brevity penalty + smoothing.
- ``kendall_tau``              — tau-b rank correlation in ``[-1, 1]``.
- ``normalized_kendall_tau``   — Kendall tau rescaled to ``[0, 1]``.
"""
from __future__ import annotations

import math
from collections import Counter
from typing import Dict, Hashable, List, Sequence, Tuple

# Smoothing mass added to a zero-match n-gram precision (Chen & Cherry method 1).
_BLEU_SMOOTH_EPSILON = 0.1


def _rank_map(order: Sequence[Hashable]) -> Dict[Hashable, int]:
    """Map each id to its index in ``order`` (first occurrence wins)."""
    ranks: Dict[Hashable, int] = {}
    for index, element in enumerate(order):
        if element not in ranks:
            ranks[element] = index
    return ranks


def _common_ranks(
    pred_order: Sequence[Hashable], gt_order: Sequence[Hashable]
) -> Tuple[List[Hashable], Dict[Hashable, int], Dict[Hashable, int]]:
    """Return ids present in both orderings plus each ordering's rank map.

    The common-id list preserves the order of first appearance in
    ``pred_order``. Ranks are the indices within the *full* original lists, as
    specified by the ARD definition.
    """
    pred_ranks = _rank_map(pred_order)
    gt_ranks = _rank_map(gt_order)
    common = [element for element in pred_ranks if element in gt_ranks]
    return common, pred_ranks, gt_ranks


def reading_order_ard(
    pred_order: Sequence[Hashable], gt_order: Sequence[Hashable]
) -> float:
    """Average Relative Distance between two reading orders (LayoutReader).

    For every id present in both orderings, let ``p`` be its index in
    ``pred_order`` and ``g`` its index in ``gt_order``; the metric is
    ``mean(|p - g|) / n`` where ``n`` is the number of common ids. Lower is
    better: identical orders give ``0.0``. For shared id sets the value lies in
    roughly ``[0, 0.5]`` (a fully reversed order scores ``0.5``).

    Degenerate case: when the two id sets are disjoint there is nothing to align,
    so the function returns ``1.0`` (maximal disagreement).
    """
    common, pred_ranks, gt_ranks = _common_ranks(pred_order, gt_order)
    n = len(common)
    if n == 0:
        return 1.0
    total = sum(abs(pred_ranks[element] - gt_ranks[element]) for element in common)
    return (total / n) / n


def reading_order_score(
    pred_order: Sequence[Hashable], gt_order: Sequence[Hashable]
) -> float:
    """``1 - reading_order_ard``; higher is better, identical orders give ``1.0``.

    For shared id sets the value lies in roughly ``[0.5, 1.0]``; the disjoint
    degenerate case returns ``0.0``.
    """
    return 1.0 - reading_order_ard(pred_order, gt_order)


def _ngram_counts(seq: Sequence[Hashable], n: int) -> Counter:
    """Count the contiguous ``n``-grams of ``seq`` as tuples."""
    if n <= 0 or len(seq) < n:
        return Counter()
    return Counter(tuple(seq[i : i + n]) for i in range(len(seq) - n + 1))


def sequence_bleu(
    pred_order: Sequence[Hashable],
    gt_order: Sequence[Hashable],
    max_n: int = 4,
) -> float:
    """BLEU score of ``pred_order`` (hypothesis) against ``gt_order`` (reference).

    Element ids are the tokens. Uniformly weighted modified n-gram precisions for
    ``n = 1 .. max_n`` are combined as a geometric mean and multiplied by the
    brevity penalty. Zero-match precisions are smoothed (Chen & Cherry method 1)
    so the score stays positive whenever any order matches; identical sequences
    score exactly ``1.0``.

    Degenerate cases: two empty sequences score ``1.0`` (identical); exactly one
    empty sequence scores ``0.0``. When the hypothesis is shorter than ``max_n``
    the effective order is clamped to ``len(pred_order)``.
    """
    if max_n < 1:
        raise ValueError("max_n must be >= 1")
    if not pred_order and not gt_order:
        return 1.0
    if not pred_order or not gt_order:
        return 0.0

    hyp_len = len(pred_order)
    ref_len = len(gt_order)
    effective_max = min(max_n, hyp_len)
    weight = 1.0 / effective_max

    log_precision_sum = 0.0
    for n in range(1, effective_max + 1):
        hyp_ngrams = _ngram_counts(pred_order, n)
        ref_ngrams = _ngram_counts(gt_order, n)
        total = sum(hyp_ngrams.values())
        if total == 0:
            continue
        matches = sum(
            min(count, ref_ngrams.get(ngram, 0)) for ngram, count in hyp_ngrams.items()
        )
        if matches == 0:
            precision = _BLEU_SMOOTH_EPSILON / total
        else:
            precision = matches / total
        log_precision_sum += weight * math.log(precision)

    brevity_penalty = 1.0 if hyp_len > ref_len else math.exp(1.0 - ref_len / hyp_len)
    return brevity_penalty * math.exp(log_precision_sum)


def kendall_tau(
    pred_order: Sequence[Hashable], gt_order: Sequence[Hashable]
) -> float:
    """Kendall tau-b rank correlation over the ids common to both orderings.

    Counts concordant and discordant pairs of common ids and normalizes by the
    tau-b denominator (which corrects for ties, though distinct ids never tie).
    Range is ``[-1, 1]``: an identical order gives ``1.0`` and a fully reversed
    order gives ``-1.0``.

    Degenerate case: fewer than two common ids leave no pair to compare, so the
    function returns ``0.0`` (no measurable correlation).
    """
    common, pred_ranks, gt_ranks = _common_ranks(pred_order, gt_order)
    n = len(common)
    if n < 2:
        return 0.0

    concordant = discordant = ties_pred = ties_gt = ties_both = 0
    for i in range(n):
        a = common[i]
        for j in range(i + 1, n):
            b = common[j]
            dx = pred_ranks[a] - pred_ranks[b]
            dy = gt_ranks[a] - gt_ranks[b]
            if dx == 0 and dy == 0:
                ties_both += 1
            elif dx == 0:
                ties_pred += 1
            elif dy == 0:
                ties_gt += 1
            elif (dx > 0) == (dy > 0):
                concordant += 1
            else:
                discordant += 1

    n0 = n * (n - 1) // 2
    n1 = ties_pred + ties_both  # pairs tied in pred
    n2 = ties_gt + ties_both    # pairs tied in gt
    denominator = math.sqrt((n0 - n1) * (n0 - n2))
    if denominator == 0:
        return 0.0
    return (concordant - discordant) / denominator


def normalized_kendall_tau(
    pred_order: Sequence[Hashable], gt_order: Sequence[Hashable]
) -> float:
    """Kendall tau rescaled from ``[-1, 1]`` to ``[0, 1]`` via ``(tau + 1) / 2``.

    Identical orders give ``1.0``, fully reversed orders give ``0.0``, and the
    degenerate (< 2 common ids) case gives ``0.5``.
    """
    return (kendall_tau(pred_order, gt_order) + 1.0) / 2.0
