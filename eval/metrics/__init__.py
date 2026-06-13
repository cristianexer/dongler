"""Dongler evaluation metrics toolkit.

Pure, dependency-free implementations of the document-extraction accuracy
metrics used by the benchmark harness.

Submodules:
- ``text``   — CER, WER, edit similarity, BLEU.
- ``table``  — TEDS / TEDS-Struct via Zhang-Shasha tree edit distance.
- ``grits``  — GriTS topology/content/location (factored 2D-MSS).
- ``layout`` — bbox IoU, mean-best-IoU, average precision, COCO mAP.
- ``order``  — reading-order ARD, sequence BLEU, Kendall tau.
"""
from __future__ import annotations

from ._common import levenshtein, normalized_levenshtein
from .grits import (
    factored_2d_similarity,
    grits_con,
    grits_from_similarity,
    grits_loc,
    grits_top,
)
from .layout import (
    average_precision,
    iou,
    match_detections,
    mean_average_precision,
    mean_best_iou,
)
from .order import (
    kendall_tau,
    normalized_kendall_tau,
    reading_order_ard,
    reading_order_score,
    sequence_bleu,
)
from .table import (
    TableNode,
    grid_to_tree,
    parse_html_table,
    teds,
    teds_struct,
    zhang_shasha,
)
from .text import (
    bleu,
    bleu_tokens,
    char_error_rate,
    edit_similarity,
    normalized_edit_distance,
    word_error_rate,
)

__all__ = [
    # primitives
    "levenshtein",
    "normalized_levenshtein",
    # text
    "char_error_rate",
    "word_error_rate",
    "edit_similarity",
    "normalized_edit_distance",
    "bleu",
    "bleu_tokens",
    # table (TEDS)
    "teds",
    "teds_struct",
    "parse_html_table",
    "grid_to_tree",
    "zhang_shasha",
    "TableNode",
    # grits
    "grits_con",
    "grits_top",
    "grits_loc",
    "factored_2d_similarity",
    "grits_from_similarity",
    # layout
    "iou",
    "mean_best_iou",
    "match_detections",
    "average_precision",
    "mean_average_precision",
    # order
    "reading_order_ard",
    "reading_order_score",
    "sequence_bleu",
    "kendall_tau",
    "normalized_kendall_tau",
]
