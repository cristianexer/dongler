"""GriTS — Grid Table Similarity for table structure recognition.

Implements the GriTS metric of Smock et al., "GriTS: Grid table similarity
metric for table structure recognition" (arXiv:2203.12555), operating over 2D
grids of cells.

Because the exact 2D most-similar-substructure (2D-MSS) problem is intractable,
this module uses the **factored 2D-MSS dynamic-programming approximation** — the
same approach taken by the official ``microsoft/table-transformer`` repository.
The factored alignment is computed as two nested longest-common-subsequence-style
dynamic programs:

* an inner DP aligns the *columns* of two rows, where the reward for matching a
  pair of cells is a configurable ``cell_similarity`` in ``[0, 1]``; and
* an outer DP aligns the *rows* of the two grids, where the reward for matching a
  pair of rows is the best inner column-alignment score between them.

The accumulated matched similarity is then combined with the cell counts of both
grids into an F-measure (the harmonic mean of precision and recall).

Three GriTS variants are provided:

* :func:`grits_con` — content similarity (edit similarity of cell text).
* :func:`grits_top` — topology/shape similarity (every present cell matches).
* :func:`grits_loc` — location similarity (IoU of cell bounding boxes).

A *grid* is a ``list[list[cell]]`` where each cell is either a ``str`` (its
content) or a ``dict`` of the form ``{"text": str, "bbox": (x, y, w, h)}`` with
``"bbox"`` optional. Ragged rows are padded with empty-string cells so the grid
is rectangular; padded positions count as present cells.

Degenerate inputs are handled without raising: an empty grid (or a grid with no
cells) yields a GriTS of ``0.0`` because there is nothing to match.
"""
from __future__ import annotations

from typing import Callable, List, Optional, Sequence, Tuple, Union

from ._common import edit_similarity

# A cell is either raw text or a dict carrying text and an optional bbox.
Cell = Union[str, dict]
Grid = Sequence[Sequence[Cell]]
BBox = Tuple[float, float, float, float]


def _cell_text(cell: Cell) -> str:
    """Return the textual content of ``cell`` (``""`` when absent)."""
    if isinstance(cell, dict):
        return str(cell.get("text", ""))
    if cell is None:
        return ""
    return str(cell)


def _cell_bbox(cell: Cell) -> Optional[BBox]:
    """Return ``cell``'s ``(x, y, w, h)`` bbox as floats, or ``None`` if absent."""
    if isinstance(cell, dict):
        bbox = cell.get("bbox")
        if bbox is None:
            return None
        values = tuple(bbox)
        if len(values) != 4:
            return None
        return (float(values[0]), float(values[1]), float(values[2]), float(values[3]))
    return None


def _iou(a: BBox, b: BBox) -> float:
    """Intersection-over-union of two ``(x, y, w, h)`` boxes in ``[0, 1]``."""
    ax, ay, aw, ah = a
    bx, by, bw, bh = b
    if aw <= 0 or ah <= 0 or bw <= 0 or bh <= 0:
        return 0.0
    ix1, iy1 = max(ax, bx), max(ay, by)
    ix2, iy2 = min(ax + aw, bx + bw), min(ay + ah, by + bh)
    iw, ih = max(0.0, ix2 - ix1), max(0.0, iy2 - iy1)
    intersection = iw * ih
    union = aw * ah + bw * bh - intersection
    if union <= 0:
        return 0.0
    return intersection / union


def _normalize_grid(grid: Grid) -> List[List[Cell]]:
    """Copy ``grid`` and right-pad ragged rows with ``""`` to a rectangle."""
    if not grid:
        return []
    width = max((len(row) for row in grid), default=0)
    normalized: List[List[Cell]] = []
    for row in grid:
        cells = list(row)
        if len(cells) < width:
            cells.extend([""] * (width - len(cells)))
        normalized.append(cells)
    return normalized


def _has_bbox(grid: Grid) -> bool:
    """Return ``True`` if any cell in ``grid`` carries a bounding box."""
    for row in grid or []:
        for cell in row:
            if _cell_bbox(cell) is not None:
                return True
    return False


def _best_alignment_score(
    a: Sequence[Cell],
    b: Sequence[Cell],
    cell_similarity: Callable[[Cell, Cell], float],
) -> float:
    """Best monotonic alignment score between two sequences of cells.

    An LCS-style DP where matching ``a[i]`` with ``b[j]`` is rewarded by
    ``cell_similarity(a[i], b[j])`` and skipping a cell costs nothing.
    """
    na, nb = len(a), len(b)
    if na == 0 or nb == 0:
        return 0.0
    previous = [0.0] * (nb + 1)
    for i in range(1, na + 1):
        current = [0.0] * (nb + 1)
        ai = a[i - 1]
        for j in range(1, nb + 1):
            current[j] = max(
                previous[j],                                  # skip a[i-1]
                current[j - 1],                               # skip b[j-1]
                previous[j - 1] + cell_similarity(ai, b[j - 1]),  # match
            )
        previous = current
    return previous[nb]


def factored_2d_similarity(
    pred_grid: Grid,
    gt_grid: Grid,
    cell_similarity: Callable[[Cell, Cell], float],
) -> Tuple[float, int, int]:
    """Factored 2D-MSS alignment between two grids.

    Returns ``(total_matched_similarity, n_pred_cells, n_gt_cells)`` where the
    total is the score of the optimal monotonic alignment of row indices and
    column indices that maximizes the summed ``cell_similarity`` over aligned
    ``(i, j)`` pairs. The alignment is approximated by the standard factored DP:
    an outer LCS-style DP over rows whose row-vs-row reward is itself the best
    inner column-alignment score.

    Cell counts are taken after padding ragged rows to a rectangle.
    """
    pred = _normalize_grid(pred_grid)
    gt = _normalize_grid(gt_grid)
    n_pred = sum(len(row) for row in pred)
    n_gt = sum(len(row) for row in gt)
    if n_pred == 0 or n_gt == 0:
        return 0.0, n_pred, n_gt

    n_rows_pred, n_rows_gt = len(pred), len(gt)
    previous = [0.0] * (n_rows_gt + 1)
    for i in range(1, n_rows_pred + 1):
        current = [0.0] * (n_rows_gt + 1)
        pred_row = pred[i - 1]
        for ip in range(1, n_rows_gt + 1):
            row_score = _best_alignment_score(pred_row, gt[ip - 1], cell_similarity)
            current[ip] = max(
                previous[ip],               # skip pred row
                current[ip - 1],            # skip gt row
                previous[ip - 1] + row_score,  # align the two rows
            )
        previous = current
    total = previous[n_rows_gt]
    return total, n_pred, n_gt


def grits_from_similarity(total: float, n_pred: int, n_gt: int) -> float:
    """Combine matched similarity and cell counts into the GriTS F-measure."""
    precision = total / max(n_pred, 1)
    recall = total / max(n_gt, 1)
    if precision + recall > 0:
        return 2.0 * precision * recall / (precision + recall)
    return 0.0


def grits_con(pred_grid: Grid, gt_grid: Grid) -> float:
    """Content GriTS: cell similarity is the edit similarity of cell text."""

    def cell_similarity(a: Cell, b: Cell) -> float:
        return edit_similarity(_cell_text(a), _cell_text(b))

    total, n_pred, n_gt = factored_2d_similarity(pred_grid, gt_grid, cell_similarity)
    return grits_from_similarity(total, n_pred, n_gt)


def grits_top(pred_grid: Grid, gt_grid: Grid) -> float:
    """Topology GriTS: every pair of present cells matches with similarity 1.0.

    This rewards the shape of the best grid alignment regardless of content;
    empty-string cells still count as present positions.
    """

    def cell_similarity(_a: Cell, _b: Cell) -> float:
        return 1.0

    total, n_pred, n_gt = factored_2d_similarity(pred_grid, gt_grid, cell_similarity)
    return grits_from_similarity(total, n_pred, n_gt)


def grits_loc(pred_grid: Grid, gt_grid: Grid) -> Optional[float]:
    """Location GriTS: cell similarity is the IoU of cell bounding boxes.

    Cells without a bbox contribute ``0.0`` similarity. If neither grid carries
    any bounding box at all, location GriTS is undefined and ``None`` is
    returned.
    """
    if not _has_bbox(pred_grid) and not _has_bbox(gt_grid):
        return None

    def cell_similarity(a: Cell, b: Cell) -> float:
        bbox_a = _cell_bbox(a)
        bbox_b = _cell_bbox(b)
        if bbox_a is None or bbox_b is None:
            return 0.0
        return _iou(bbox_a, bbox_b)

    total, n_pred, n_gt = factored_2d_similarity(pred_grid, gt_grid, cell_similarity)
    return grits_from_similarity(total, n_pred, n_gt)
