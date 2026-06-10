import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))

from eval_metrics.table import (
    TableNode,
    grid_to_tree,
    parse_html_table,
    teds,
    teds_struct,
    zhang_shasha,
)


# --------------------------------------------------------------------------- #
# TableNode
# --------------------------------------------------------------------------- #
def test_tablenode_defaults():
    node = TableNode("td")
    assert node.tag == "td"
    assert node.content == ""
    assert node.colspan == 1
    assert node.rowspan == 1
    assert node.children == []


def test_tablenode_children_not_shared():
    a = TableNode("table")
    b = TableNode("table")
    a.children.append(TableNode("tr"))
    assert b.children == []  # mutable default must not leak between instances


def test_count_subtree_nodes():
    tree = TableNode("table", children=[TableNode("tr", children=[TableNode("td")])])
    assert tree.count() == 3
    assert TableNode("td").count() == 1


# --------------------------------------------------------------------------- #
# HTML parsing
# --------------------------------------------------------------------------- #
def test_parse_basic_table():
    tree = parse_html_table("<table><tr><td>A</td><td>B</td></tr></table>")
    assert tree.tag == "table"
    assert len(tree.children) == 1
    row = tree.children[0]
    assert row.tag == "tr"
    assert [c.tag for c in row.children] == ["td", "td"]
    assert [c.content for c in row.children] == ["A", "B"]
    assert tree.count() == 4


def test_parse_spans_and_th():
    html = '<table><tr><th colspan="2" rowspan="3">H</th></tr></table>'
    tree = parse_html_table(html)
    cell = tree.children[0].children[0]
    assert cell.tag == "th"
    assert cell.colspan == 2
    assert cell.rowspan == 3
    assert cell.content == "H"


def test_parse_flattens_thead_tbody():
    html = (
        "<table><thead><tr><th>H</th></tr></thead>"
        "<tbody><tr><td>X</td></tr><tr><td>Y</td></tr></tbody></table>"
    )
    tree = parse_html_table(html)
    # thead/tbody wrappers are flattened -> three rows directly under table.
    assert [c.tag for c in tree.children] == ["tr", "tr", "tr"]
    assert tree.children[0].children[0].tag == "th"
    assert tree.children[1].children[0].content == "X"


def test_parse_whitespace_normalized_and_inline_tags_ignored():
    tree = parse_html_table("<table><tr><td>  hello   <b>world</b>  </td></tr></table>")
    assert tree.children[0].children[0].content == "hello world"


def test_parse_bad_span_defaults_to_one():
    tree = parse_html_table('<table><tr><td colspan="oops">A</td></tr></table>')
    assert tree.children[0].children[0].colspan == 1


def test_parse_empty_and_none():
    assert parse_html_table("").tag == "table"
    assert parse_html_table("").count() == 1
    assert parse_html_table(None).count() == 1


# --------------------------------------------------------------------------- #
# grid_to_tree
# --------------------------------------------------------------------------- #
def test_grid_to_tree_structure_and_headers():
    tree = grid_to_tree([["a", "b"], ["c", "d"]], header_rows=1)
    assert tree.tag == "table"
    assert [c.tag for c in tree.children] == ["tr", "tr"]
    assert [c.tag for c in tree.children[0].children] == ["th", "th"]
    assert [c.tag for c in tree.children[1].children] == ["td", "td"]
    assert tree.children[1].children[0].content == "c"
    # 1 table + 2 tr + 4 cells
    assert tree.count() == 7


def test_grid_to_tree_empty():
    tree = grid_to_tree([])
    assert tree.tag == "table"
    assert tree.children == []
    assert tree.count() == 1


# --------------------------------------------------------------------------- #
# Zhang-Shasha (hand-checked)
# --------------------------------------------------------------------------- #
def _tag_cost(a, b):
    return 0.0 if a.tag == b.tag else 1.0


def test_zss_identical_is_zero():
    t = parse_html_table("<table><tr><td>A</td><td>B</td></tr></table>")
    t2 = parse_html_table("<table><tr><td>A</td><td>B</td></tr></table>")
    assert zhang_shasha(t, t2, _tag_cost) == 0.0


def test_zss_relabel_one_leaf():
    t1 = TableNode("a")
    t2 = TableNode("b")
    assert zhang_shasha(t1, t2, _tag_cost) == 1.0
    # arbitrary relabel cost flows straight through for a single-node relabel.
    assert zhang_shasha(t1, t2, lambda a, b: 0.25) == 0.25


def test_zss_delete_one_node():
    t1 = TableNode("table", children=[TableNode("tr")])
    t2 = TableNode("table")
    assert zhang_shasha(t1, t2, _tag_cost) == 1.0
    assert zhang_shasha(t2, t1, _tag_cost) == 1.0  # symmetry of insert/delete


def test_zss_symmetry():
    a = parse_html_table("<table><tr><td>A</td></tr><tr><td>B</td></tr></table>")
    b = parse_html_table("<table><tr><td>X</td><td>Y</td></tr></table>")
    cost = lambda n1, n2: _tag_cost(n1, n2)
    assert zhang_shasha(a, b, cost) == zhang_shasha(b, a, cost)


def test_zss_empty_trees():
    # Single-node "empty" tables (only the table root).
    assert zhang_shasha(TableNode("table"), TableNode("table"), _tag_cost) == 0.0


# --------------------------------------------------------------------------- #
# TEDS — required reference assertions
# --------------------------------------------------------------------------- #
def test_teds_identical_is_one():
    html = "<table><tr><td>A</td><td>B</td></tr></table>"
    assert teds(html, html) == 1.0


def test_teds_single_cell_content_differs():
    a = "<table><tr><td>A</td></tr></table>"
    b = "<table><tr><td>B</td></tr></table>"
    # nodes = 3 each; only td content differs (edit distance 1.0) => 1 - 1/3.
    assert abs(teds(a, b) - (1.0 - 1.0 / 3.0)) < 1e-9


def test_teds_struct_ignores_content():
    a = "<table><tr><td>A</td></tr></table>"
    b = "<table><tr><td>B</td></tr></table>"
    assert teds_struct(a, b) == 1.0


def test_teds_2x2_vs_1x1_strictly_between():
    big = "<table><tr><td>a</td><td>b</td></tr><tr><td>c</td><td>d</td></tr></table>"
    small = "<table><tr><td>a</td></tr></table>"
    score = teds(big, small)
    assert 0.0 < score < 1.0


# --------------------------------------------------------------------------- #
# TEDS — properties / edge cases
# --------------------------------------------------------------------------- #
def test_teds_bounds_and_symmetry():
    a = "<table><tr><td>hello</td><td>world</td></tr></table>"
    b = "<table><tr><th>HELLO</th></tr><tr><td>x</td></tr></table>"
    for pred, gt in ((a, a), (a, b), (b, a)):
        score = teds(pred, gt)
        assert 0.0 <= score <= 1.0
    assert abs(teds(a, b) - teds(b, a)) < 1e-12
    assert abs(teds_struct(a, b) - teds_struct(b, a)) < 1e-12


def test_teds_accepts_tablenode_inputs():
    tree = grid_to_tree([["A", "B"]])
    html = "<table><tr><td>A</td><td>B</td></tr></table>"
    assert teds(tree, html) == 1.0
    assert teds(tree, tree) == 1.0


def test_teds_disjoint_tags_worst_content():
    # Same shape, different (colspan,rowspan) => cell relabel cost 1.0.
    a = '<table><tr><td colspan="2">A</td></tr></table>'
    b = '<table><tr><td colspan="1">A</td></tr></table>'
    # nodes = 3 each, cell relabel cost 1.0 => 1 - 1/3.
    assert abs(teds(a, b) - (1.0 - 1.0 / 3.0)) < 1e-9


def test_teds_two_empty_tables_are_identical():
    assert teds("", "") == 1.0
    assert teds("<table></table>", "<table></table>") == 1.0


def test_teds_completely_disjoint_low_score():
    a = "<table><tr><td>A</td></tr></table>"
    b = "<table><tr><td>Z</td></tr></table>"
    # td 'A' vs 'Z': normalized edit distance 1.0 -> 1 - 1/3.
    assert abs(teds(a, b) - (1.0 - 1.0 / 3.0)) < 1e-9
    assert teds_struct(a, b) == 1.0


def test_teds_partial_content_match():
    a = "<table><tr><td>abcd</td></tr></table>"
    b = "<table><tr><td>abxd</td></tr></table>"
    # one char differs out of 4 -> normalized edit dist 0.25 -> 1 - 0.25/3.
    assert abs(teds(a, b) - (1.0 - 0.25 / 3.0)) < 1e-9
