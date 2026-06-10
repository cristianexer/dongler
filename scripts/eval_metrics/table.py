"""TEDS — Tree-Edit-Distance-based Similarity for tables (stdlib only).

Self-contained implementation of TEDS / TEDS-Struct (Zhou et al., PubTabNet,
arXiv:1911.10683). A table is modelled as a small ``TableNode`` tree
(``table`` -> ``tr`` -> ``td``/``th``) and compared with the classic
Zhang-Shasha ordered tree edit distance.

TEDS = 1 - EditDistance(pred, gt) / max(nodes(pred), nodes(gt))

The canonical TEDS cost model is used:
- tags differ                              -> 1.0
- both cell tags (td/th):
    * (colspan, rowspan) differ            -> 1.0
    * structure_only                       -> 0.0
    * otherwise normalized char edit dist of the cell contents (in [0, 1])
- otherwise (same non-cell tag, e.g. table/tr) -> 0.0
"""
from __future__ import annotations

from html.parser import HTMLParser
from typing import Callable, List, Optional, Sequence, Tuple, Union

from ._common import normalized_levenshtein

__all__ = [
    "TableNode",
    "parse_html_table",
    "grid_to_tree",
    "zhang_shasha",
    "teds",
    "teds_struct",
]

# A TEDS input may be raw HTML or an already-built tree.
TableInput = Union[str, "TableNode"]


class TableNode:
    """A lightweight ordered tree node describing a table fragment.

    Attributes
    ----------
    tag:
        Element name, e.g. ``"table"``, ``"tr"``, ``"td"`` or ``"th"``.
    content:
        Whitespace-normalized text content (only meaningful for cells).
    colspan / rowspan:
        Cell spans (``>= 1``); ``1`` for non-cell nodes.
    children:
        Ordered child nodes.
    """

    def __init__(
        self,
        tag: str,
        content: str = "",
        colspan: int = 1,
        rowspan: int = 1,
        children: Optional[List["TableNode"]] = None,
    ) -> None:
        self.tag = tag
        self.content = content
        self.colspan = colspan
        self.rowspan = rowspan
        self.children: List["TableNode"] = children if children is not None else []

    def count(self) -> int:
        """Number of nodes in the subtree rooted at this node (incl. self)."""
        return 1 + sum(child.count() for child in self.children)

    def __repr__(self) -> str:  # pragma: no cover - debugging aid
        return (
            f"TableNode(tag={self.tag!r}, content={self.content!r}, "
            f"colspan={self.colspan}, rowspan={self.rowspan}, "
            f"children={len(self.children)})"
        )


# --------------------------------------------------------------------------- #
# HTML parsing
# --------------------------------------------------------------------------- #
def _parse_span(value: Optional[str]) -> int:
    """Parse a colspan/rowspan attribute into an ``int >= 1`` (default ``1``)."""
    if value is None:
        return 1
    try:
        span = int(str(value).strip())
    except (TypeError, ValueError):
        return 1
    return span if span >= 1 else 1


class _TableHTMLParser(HTMLParser):
    """Minimal table parser building a ``table -> tr -> td/th`` tree.

    ``thead``/``tbody``/``tfoot`` wrappers are flattened (their ``tr`` children
    attach directly to the table); unknown tags are ignored but the text inside
    an open cell is still captured.
    """

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.root = TableNode("table")
        self._current_tr: Optional[TableNode] = None
        self._current_cell: Optional[TableNode] = None
        self._buf: List[str] = []

    def handle_starttag(self, tag: str, attrs) -> None:  # type: ignore[override]
        tag = tag.lower()
        if tag == "tr":
            self._current_tr = TableNode("tr")
            self.root.children.append(self._current_tr)
            self._current_cell = None
            self._buf = []
        elif tag in ("td", "th"):
            attr_map = dict(attrs)
            cell = TableNode(
                tag,
                colspan=_parse_span(attr_map.get("colspan")),
                rowspan=_parse_span(attr_map.get("rowspan")),
            )
            if self._current_tr is None:
                # Defensive: a cell outside any row gets an implicit row.
                self._current_tr = TableNode("tr")
                self.root.children.append(self._current_tr)
            self._current_tr.children.append(cell)
            self._current_cell = cell
            self._buf = []
        # table / thead / tbody / tfoot / inline markup: ignored as containers.

    def handle_startendtag(self, tag: str, attrs) -> None:  # type: ignore[override]
        self.handle_starttag(tag, attrs)
        self.handle_endtag(tag)

    def handle_data(self, data: str) -> None:
        if self._current_cell is not None:
            self._buf.append(data)

    def handle_endtag(self, tag: str) -> None:
        tag = tag.lower()
        if tag in ("td", "th"):
            if self._current_cell is not None:
                self._current_cell.content = " ".join("".join(self._buf).split())
                self._current_cell = None
                self._buf = []
        elif tag == "tr":
            self._current_tr = None
            self._current_cell = None
            self._buf = []


def parse_html_table(html_text: str) -> TableNode:
    """Parse an HTML table string into a ``TableNode`` tree.

    Always returns a ``"table"`` root; empty or ``None`` input yields a root
    with no children. Cell text is whitespace-normalized.
    """
    parser = _TableHTMLParser()
    parser.feed(html_text or "")
    parser.close()
    return parser.root


def grid_to_tree(grid: Sequence[Sequence[str]], header_rows: int = 0) -> TableNode:
    """Build a tree from a 2D grid of cell texts.

    Each grid row becomes a ``tr``; cells in the first ``header_rows`` rows are
    ``th`` and the rest ``td``. All spans are ``1``.
    """
    root = TableNode("table")
    for row_index, row in enumerate(grid):
        tr = TableNode("tr")
        cell_tag = "th" if row_index < header_rows else "td"
        for value in row:
            tr.children.append(TableNode(cell_tag, content=str(value)))
        root.children.append(tr)
    return root


# --------------------------------------------------------------------------- #
# Zhang-Shasha tree edit distance
# --------------------------------------------------------------------------- #
def _postorder(root: TableNode) -> Tuple[List[TableNode], List[int]]:
    """Return ``(nodes, lmlds)`` in post-order.

    ``nodes[k]`` is the ``(k+1)``-th node in post-order; ``lmlds[k]`` is the
    1-based post-order index of that node's leftmost-leaf descendant.
    """
    nodes: List[TableNode] = []
    lmlds: List[int] = []
    counter = [0]

    def visit(node: TableNode) -> int:
        child_lmlds: List[int] = [visit(child) for child in node.children]
        counter[0] += 1
        index = counter[0]
        nodes.append(node)
        lmlds.append(child_lmlds[0] if child_lmlds else index)
        return lmlds[-1]

    visit(root)
    return nodes, lmlds


def _keyroots(lmlds: List[int]) -> List[int]:
    """Key roots: the largest post-order index for each distinct leftmost leaf."""
    last_for_lmld = {}
    for index in range(1, len(lmlds) + 1):
        last_for_lmld[lmlds[index - 1]] = index
    return sorted(last_for_lmld.values())


def zhang_shasha(
    t1: TableNode,
    t2: TableNode,
    update_cost: Callable[[TableNode, TableNode], float],
) -> float:
    """Classic Zhang-Shasha ordered tree edit distance.

    Insert and delete each cost ``1`` per node; ``update_cost(n1, n2)`` is the
    relabel cost. Symmetric whenever ``update_cost`` is symmetric.
    """
    nodes1, lmld1 = _postorder(t1)
    nodes2, lmld2 = _postorder(t2)
    n, m = len(nodes1), len(nodes2)
    if n == 0 and m == 0:
        return 0.0

    insert_cost = delete_cost = 1.0
    keyroots1 = _keyroots(lmld1)
    keyroots2 = _keyroots(lmld2)

    # treedist[i][j] (1-based) = edit distance between subtrees rooted at i, j.
    treedist = [[0.0] * (m + 1) for _ in range(n + 1)]

    for i in keyroots1:
        li = lmld1[i - 1]
        rows = i - li + 1
        for j in keyroots2:
            lj = lmld2[j - 1]
            cols = j - lj + 1
            # forestdist over offsets: fd[a][b] maps to absolute (li-1+a, lj-1+b);
            # offset 0 represents the empty forest.
            fd = [[0.0] * (cols + 1) for _ in range(rows + 1)]
            for a in range(1, rows + 1):
                fd[a][0] = fd[a - 1][0] + delete_cost
            for b in range(1, cols + 1):
                fd[0][b] = fd[0][b - 1] + insert_cost
            for a in range(1, rows + 1):
                i1 = li - 1 + a
                for b in range(1, cols + 1):
                    j1 = lj - 1 + b
                    deletion = fd[a - 1][b] + delete_cost
                    insertion = fd[a][b - 1] + insert_cost
                    if lmld1[i1 - 1] == li and lmld2[j1 - 1] == lj:
                        relabel = fd[a - 1][b - 1] + update_cost(
                            nodes1[i1 - 1], nodes2[j1 - 1]
                        )
                        value = min(deletion, insertion, relabel)
                        fd[a][b] = value
                        treedist[i1][j1] = value
                    else:
                        prev_a = lmld1[i1 - 1] - li
                        prev_b = lmld2[j1 - 1] - lj
                        match = fd[prev_a][prev_b] + treedist[i1][j1]
                        fd[a][b] = min(deletion, insertion, match)

    return treedist[n][m]


# --------------------------------------------------------------------------- #
# TEDS
# --------------------------------------------------------------------------- #
_CELL_TAGS = ("td", "th")


def _as_tree(value: TableInput) -> TableNode:
    return value if isinstance(value, TableNode) else parse_html_table(value)


def _teds_update_cost(n1: TableNode, n2: TableNode, structure_only: bool) -> float:
    """Canonical TEDS relabel cost between two nodes."""
    if n1.tag != n2.tag:
        return 1.0
    if n1.tag in _CELL_TAGS:
        if (n1.colspan, n1.rowspan) != (n2.colspan, n2.rowspan):
            return 1.0
        if structure_only:
            return 0.0
        return normalized_levenshtein(n1.content, n2.content)
    return 0.0


def teds(pred: TableInput, gt: TableInput, structure_only: bool = False) -> float:
    """Tree-Edit-Distance-based Similarity between two tables.

    ``pred``/``gt`` may each be an HTML string or a ``TableNode``. The result is
    in ``[0, 1]`` (``1.0`` = identical). When ``structure_only`` is true, cell
    text is ignored and only the table layout is scored.
    """
    tree_pred = _as_tree(pred)
    tree_gt = _as_tree(gt)
    denominator = max(tree_pred.count(), tree_gt.count())
    if denominator == 0:
        return 1.0  # two empty trees are defined as identical.

    distance = zhang_shasha(
        tree_pred,
        tree_gt,
        lambda a, b: _teds_update_cost(a, b, structure_only),
    )
    score = 1.0 - distance / denominator
    # Clamp to guard against degenerate trees where the edit distance can
    # exceed max(nodes); well-formed tables stay naturally within [0, 1].
    return max(0.0, min(1.0, score))


def teds_struct(pred: TableInput, gt: TableInput) -> float:
    """Structure-only TEDS (cell contents ignored)."""
    return teds(pred, gt, structure_only=True)
