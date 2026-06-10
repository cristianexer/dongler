import importlib.util
import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]


def load_runner():
    path = REPO_ROOT / "scripts" / "run-benchmarks.py"
    spec = importlib.util.spec_from_file_location("run_benchmarks", path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def write_doc(path: Path, blocks, *, page_number: int = 1) -> None:
    path.write_text(json.dumps({"pages": [{"number": page_number, "blocks": blocks}]}))


def test_text_metric_bundle_perfect_match(tmp_path):
    runner = load_runner()
    json_out = tmp_path / "doc.json"
    write_doc(json_out, [{"type": "text", "text": "hello world foo"}])
    bundle = runner.text_metric_bundle(runner.ReferenceTarget("hello world foo"), json_out)
    assert bundle is not None
    assert set(bundle) == {"char_error_rate", "word_error_rate", "edit_similarity", "bleu"}
    assert bundle["edit_similarity"] > 0.9
    assert bundle["char_error_rate"] < 0.1
    assert bundle["bleu"] == 1.0


def test_text_metric_bundle_mismatch_is_penalized(tmp_path):
    runner = load_runner()
    json_out = tmp_path / "doc.json"
    write_doc(json_out, [{"type": "text", "text": "hello world bar"}])
    bundle = runner.text_metric_bundle(runner.ReferenceTarget("hello world foo"), json_out)
    assert bundle is not None
    assert 0.0 <= bundle["edit_similarity"] < 1.0
    assert bundle["char_error_rate"] > 0.0
    assert 0.0 <= bundle["bleu"] < 1.0


def test_ground_truth_table_html_reads_sibling_html(tmp_path):
    runner = load_runner()
    document = tmp_path / "report.pdf"
    document.write_bytes(b"%PDF-1.4")
    (tmp_path / "report.tables.html").write_text(
        "<p>x</p><table><tr><td>a</td><td>b</td></tr></table>"
        "<table><tr><td>c</td></tr></table>"
    )
    tables = runner.ground_truth_table_html(document)
    assert len(tables) == 2
    assert tables[0].startswith("<table")


def test_ground_truth_table_html_reads_sibling_json(tmp_path):
    runner = load_runner()
    document = tmp_path / "report.pdf"
    (tmp_path / "report.tables.json").write_text(
        json.dumps(["<table><tr><td>a</td></tr></table>"])
    )
    tables = runner.ground_truth_table_html(document)
    assert tables == ["<table><tr><td>a</td></tr></table>"]


def test_ground_truth_table_html_absent_returns_empty(tmp_path):
    runner = load_runner()
    assert runner.ground_truth_table_html(tmp_path / "no-gt.pdf") == []


def test_table_metric_bundle_perfect_match(tmp_path):
    runner = load_runner()
    document = tmp_path / "report.pdf"
    document.write_bytes(b"%PDF-1.4")
    (tmp_path / "report.tables.html").write_text(
        "<table><tr><td>a</td><td>b</td></tr></table>"
    )
    json_out = tmp_path / "report.json"
    write_doc(json_out, [{"type": "table", "headers": ["a", "b"], "rows": []}])
    bundle = runner.table_metric_bundle(document, json_out)
    assert bundle is not None
    assert bundle["teds"] == 1.0
    assert bundle["teds_struct"] == 1.0
    assert bundle["grits_con"] == 1.0
    assert bundle["grits_top"] == 1.0
    assert bundle["gt_tables"] == 1
    assert bundle["pred_tables"] == 1


def test_table_metric_bundle_none_without_ground_truth(tmp_path):
    runner = load_runner()
    document = tmp_path / "report.pdf"
    json_out = tmp_path / "report.json"
    write_doc(json_out, [{"type": "table", "headers": ["a"], "rows": []}])
    assert runner.table_metric_bundle(document, json_out) is None


def test_table_node_to_grid_flattens_rows():
    runner = load_runner()
    node = runner.parse_html_table(
        "<table><tr><td>a</td><td>b</td></tr><tr><td>c</td><td>d</td></tr></table>"
    )
    assert runner.table_node_to_grid(node) == [["a", "b"], ["c", "d"]]


def test_ground_truth_boxes_parses_both_formats(tmp_path):
    runner = load_runner()
    document = tmp_path / "page.pdf"
    (tmp_path / "page.boxes.json").write_text(
        json.dumps(
            [
                {"box": [1, 2, 3, 4], "label": "text"},
                {"x": 5, "y": 6, "width": 7, "height": 8},
            ]
        )
    )
    boxes = runner.ground_truth_boxes_for_document(document)
    assert boxes == [
        {"box": (1.0, 2.0, 3.0, 4.0), "label": "text"},
        {"box": (5.0, 6.0, 7.0, 8.0), "label": "block"},
    ]


def test_ground_truth_boxes_absent_returns_none(tmp_path):
    runner = load_runner()
    assert runner.ground_truth_boxes_for_document(tmp_path / "none.pdf") is None


def test_layout_metric_bundle_perfect_match(tmp_path):
    runner = load_runner()
    document = tmp_path / "page.pdf"
    (tmp_path / "page.boxes.json").write_text(
        json.dumps([{"box": [10, 20, 30, 40], "label": "text"}])
    )
    json_out = tmp_path / "page.json"
    write_doc(
        json_out,
        [{"type": "text", "text": "x", "bbox": {"x": 10, "y": 20, "width": 30, "height": 40}}],
    )
    bundle = runner.layout_metric_bundle(document, json_out)
    assert bundle is not None
    assert bundle["mean_best_iou"] == 1.0
    assert bundle["map_localization"] == 1.0
    assert bundle["gt_boxes"] == 1
    assert bundle["pred_boxes"] == 1


def test_layout_metric_bundle_partial_overlap_is_penalized(tmp_path):
    runner = load_runner()
    document = tmp_path / "page.pdf"
    (tmp_path / "page.boxes.json").write_text(
        json.dumps([{"box": [0, 0, 10, 10]}])
    )
    json_out = tmp_path / "page.json"
    write_doc(
        json_out,
        [{"type": "text", "text": "x", "bbox": {"x": 5, "y": 0, "width": 10, "height": 10}}],
    )
    bundle = runner.layout_metric_bundle(document, json_out)
    assert bundle is not None
    assert 0.0 < bundle["mean_best_iou"] < 1.0


def test_layout_metric_bundle_none_without_ground_truth(tmp_path):
    runner = load_runner()
    document = tmp_path / "page.pdf"
    json_out = tmp_path / "page.json"
    write_doc(json_out, [{"type": "text", "text": "x"}])
    assert runner.layout_metric_bundle(document, json_out) is None


def test_markdown_table_includes_edit_sim_column():
    runner = load_runner()
    row = {
        "dataset": "Demo",
        "slug": "demo",
        "status": "ok",
        "local_bytes": 1024 * 1024,
        "documents_evaluated": 3,
        "pdfs_evaluated": 3,
        "parse_success_rate": 1.0,
        "bbox_block_rate": 0.5,
        "anchored_block_rate": 0.5,
        "pages_per_second": 10.0,
        "ground_truth_accuracy": 0.9,
        "text_metrics": {"edit_similarity": 0.87, "documents": 3},
    }
    table = runner.markdown_table([row], "2026-06-10", None)
    assert "Edit sim" in table
    assert "87" in table


def test_markdown_table_handles_missing_text_metrics():
    runner = load_runner()
    row = {
        "dataset": "Demo",
        "slug": "demo",
        "status": "ok",
        "local_bytes": 0,
        "documents_evaluated": 0,
        "pdfs_evaluated": 0,
        "parse_success_rate": None,
        "bbox_block_rate": None,
        "anchored_block_rate": None,
        "pages_per_second": None,
        "ground_truth_accuracy": None,
        "text_metrics": None,
    }
    table = runner.markdown_table([row], "2026-06-10", None)
    assert "Edit sim" in table
    # No crash and the empty cell renders as the n/a sentinel.
    assert "n/a" in table
