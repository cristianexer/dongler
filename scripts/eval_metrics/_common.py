"""Shared primitives for the dongler eval metrics (dependency-free, stdlib only)."""
from __future__ import annotations

from typing import Sequence


def levenshtein(a: Sequence, b: Sequence) -> int:
    """Edit distance (insert/delete/substitute, unit cost) between two sequences.

    Works on strings (character edit distance) or lists of tokens (word edit
    distance). O(n*m) time, O(min(n, m)) memory.
    """
    if len(a) == 0:
        return len(b)
    if len(b) == 0:
        return len(a)
    # Keep the inner row over the shorter sequence to bound memory.
    if len(a) < len(b):
        a, b = b, a
    previous = list(range(len(b) + 1))
    for i in range(1, len(a) + 1):
        current = [i] + [0] * len(b)
        ai = a[i - 1]
        for j in range(1, len(b) + 1):
            cost = 0 if ai == b[j - 1] else 1
            current[j] = min(
                previous[j] + 1,        # deletion
                current[j - 1] + 1,     # insertion
                previous[j - 1] + cost,  # substitution
            )
        previous = current
    return previous[len(b)]


def normalized_levenshtein(a: Sequence, b: Sequence) -> float:
    """Edit distance normalized into ``[0, 1]`` by the longer length.

    Returns ``0.0`` for two empty inputs (defined as identical).
    """
    if len(a) == 0 and len(b) == 0:
        return 0.0
    return levenshtein(a, b) / max(len(a), len(b), 1)


def edit_similarity(a: Sequence, b: Sequence) -> float:
    """``1 - normalized_levenshtein``; ``1.0`` means identical."""
    return 1.0 - normalized_levenshtein(a, b)
