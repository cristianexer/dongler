"""Token-coverage and spurious-token metrics (PRD §3.2).

These are the metric family the legacy harness never had, now table stakes
(SCORE-Bench's argument). They operationalize the pipeline invariants
"no text dropped" (coverage) and "no text invented" (spurious rate).

Both operate on whitespace tokens as *multisets* (a token that appears twice
in the ground truth must appear twice in the prediction to be fully covered),
which makes them sensitive to dropped/duplicated content that a set-based
overlap would hide.
"""
from __future__ import annotations

from collections import Counter
from typing import List

__all__ = [
    "tokenize",
    "token_coverage",
    "spurious_token_rate",
]


def tokenize(text: str) -> List[str]:
    """Split ``text`` into whitespace-delimited tokens (empty list for blank)."""
    return text.split()


def token_coverage(gt: str, prediction: str) -> float:
    """Fraction of ground-truth tokens present in the prediction (multiset).

    Computes ``sum_t min(gt_count[t], pred_count[t]) / sum_t gt_count[t]`` over
    the whitespace-token multisets. Range ``[0, 1]`` where ``1.0`` means every
    GT token (with multiplicity) is accounted for in the prediction -- i.e. no
    source text was dropped. Higher is better.

    Empty-GT rule: when the ground truth has no tokens, coverage is undefined,
    so we return ``1.0`` (there was nothing to cover).
    """
    gt_counts = Counter(tokenize(gt))
    total = sum(gt_counts.values())
    if total == 0:
        return 1.0
    pred_counts = Counter(tokenize(prediction))
    covered = sum(min(count, pred_counts.get(token, 0)) for token, count in gt_counts.items())
    return covered / total


def spurious_token_rate(gt: str, prediction: str) -> float:
    """Fraction of prediction tokens not attributable to the GT (multiset).

    Computes ``(sum_t pred_count[t] - sum_t min(gt_count[t], pred_count[t])) /
    sum_t pred_count[t]`` over the whitespace-token multisets -- i.e. the share
    of predicted tokens that have no matching GT token (with multiplicity).
    Range ``[0, 1]`` where ``0.0`` means the prediction invented nothing.
    Lower is better.

    Empty-prediction rule: when the prediction has no tokens, the rate is
    undefined, so we return ``0.0`` (nothing was produced, so nothing spurious).
    """
    pred_counts = Counter(tokenize(prediction))
    total = sum(pred_counts.values())
    if total == 0:
        return 0.0
    gt_counts = Counter(tokenize(gt))
    attributable = sum(min(count, gt_counts.get(token, 0)) for token, count in pred_counts.items())
    return (total - attributable) / total
