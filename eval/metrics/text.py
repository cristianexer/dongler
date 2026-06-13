"""Text-similarity metrics: CER, WER, edit similarity, and sentence BLEU.

Dependency-free (stdlib only) implementations of the character/word accuracy
scores used by the document-extraction benchmark harness. All scores are
deterministic and operate on plain ``str`` inputs (BLEU additionally exposes a
token-sequence entry point for reuse by other modules).
"""
from __future__ import annotations

import math
from collections import Counter
from typing import List

from ._common import edit_similarity as _char_edit_similarity
from ._common import levenshtein

__all__ = [
    "char_error_rate",
    "word_error_rate",
    "edit_similarity",
    "normalized_edit_distance",
    "bleu",
    "bleu_tokens",
]


def char_error_rate(reference: str, hypothesis: str) -> float:
    """Character Error Rate: char edit distance divided by reference length.

    ``CER = levenshtein(reference, hypothesis) / max(len(reference), 1)``.
    Lower is better; ``0.0`` means a perfect match. The raw ratio is returned
    (it can exceed ``1.0`` when the hypothesis is much longer than the
    reference) -- it is *not* clamped.

    Empty-reference rule: when ``reference == ""`` the ratio is undefined, so we
    return ``0.0`` if the hypothesis is also empty (perfect) and ``1.0``
    otherwise (everything produced is spurious).
    """
    if len(reference) == 0:
        return 0.0 if hypothesis == "" else 1.0
    return levenshtein(reference, hypothesis) / len(reference)


def word_error_rate(reference: str, hypothesis: str) -> float:
    """Word Error Rate: word edit distance divided by reference word count.

    Both strings are split on runs of whitespace into word lists, then
    ``WER = levenshtein(ref_words, hyp_words) / max(len(ref_words), 1)``.
    Lower is better; the raw ratio is returned (not clamped).

    Empty-reference rule mirrors :func:`char_error_rate`: when the reference has
    no words, return ``0.0`` if the hypothesis also has no words else ``1.0``.
    """
    ref_words = reference.split()
    hyp_words = hypothesis.split()
    if len(ref_words) == 0:
        return 0.0 if len(hyp_words) == 0 else 1.0
    return levenshtein(ref_words, hyp_words) / len(ref_words)


def edit_similarity(reference: str, hypothesis: str) -> float:
    """Character-level edit similarity ``1 - normalized_levenshtein``.

    Range ``[0, 1]`` where ``1.0`` means identical strings. This is the
    "1 - normalized edit distance" text score reported by OmniDocBench/READoc.
    Two empty strings are defined as identical and score ``1.0``.
    """
    return _char_edit_similarity(reference, hypothesis)


def normalized_edit_distance(reference: str, hypothesis: str) -> float:
    """Normalized character edit distance ``1 - edit_similarity``.

    The distance form of :func:`edit_similarity`; range ``[0, 1]`` where ``0.0``
    means identical strings and ``1.0`` means maximally dissimilar.
    """
    return 1.0 - edit_similarity(reference, hypothesis)


def _ngram_counts(tokens: List[str], n: int) -> Counter:
    """Multiset of contiguous ``n``-grams in ``tokens`` (empty if too short)."""
    if n <= 0 or len(tokens) < n:
        return Counter()
    return Counter(tuple(tokens[i : i + n]) for i in range(len(tokens) - n + 1))


def bleu_tokens(
    reference_tokens: List[str],
    hypothesis_tokens: List[str],
    max_n: int = 4,
) -> float:
    """Single-sentence BLEU over arbitrary token sequences.

    Computes modified n-gram precisions for ``n = 1..max_n`` with a brevity
    penalty and Chen & Cherry (2014) "method 1" floor smoothing: precisions that
    come out exactly zero (a higher-order n-gram is simply absent from the
    matches) are replaced by a small ``epsilon / denominator`` term so the
    geometric mean never collapses to zero. Identical non-trivial sequences
    score exactly ``1.0`` because their precisions are all genuinely ``1.0`` and
    are left untouched by the smoothing. Range ``[0, 1]``.

    Degenerate cases: identical (including both-empty) sequences score ``1.0``;
    an empty hypothesis or an empty reference (but not both) scores ``0.0``. The
    effective maximum order is capped at the hypothesis length so orders with no
    n-grams at all do not force a division by zero.
    """
    ref_len = len(reference_tokens)
    hyp_len = len(hypothesis_tokens)

    if ref_len == 0 or hyp_len == 0:
        # Identical only when both are empty.
        return 1.0 if ref_len == 0 and hyp_len == 0 else 0.0

    epsilon = 0.1
    effective_max_n = min(max_n, hyp_len)

    log_precision_sum = 0.0
    for n in range(1, effective_max_n + 1):
        hyp_ngrams = _ngram_counts(hypothesis_tokens, n)
        ref_ngrams = _ngram_counts(reference_tokens, n)
        denominator = sum(hyp_ngrams.values())
        if denominator == 0:
            # No n-grams of this order in the hypothesis; skip the order.
            continue
        clipped = 0
        for ngram, count in hyp_ngrams.items():
            clipped += min(count, ref_ngrams.get(ngram, 0))
        precision = (clipped if clipped > 0 else epsilon) / denominator
        log_precision_sum += math.log(precision)

    geometric_mean = math.exp(log_precision_sum / effective_max_n)

    # Brevity penalty: penalize hypotheses shorter than the reference.
    if hyp_len > ref_len:
        brevity_penalty = 1.0
    else:
        brevity_penalty = math.exp(1.0 - ref_len / hyp_len)

    return brevity_penalty * geometric_mean


def bleu(reference: str, hypothesis: str, max_n: int = 4) -> float:
    """Single-sentence BLEU over whitespace tokens.

    Thin wrapper that tokenizes both strings on whitespace and delegates to
    :func:`bleu_tokens`. Identical non-trivial text scores exactly ``1.0``;
    range ``[0, 1]``.
    """
    return bleu_tokens(reference.split(), hypothesis.split(), max_n)
