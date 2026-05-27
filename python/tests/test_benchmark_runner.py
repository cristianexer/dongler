import importlib.util
import json
import sys
from pathlib import Path


def load_runner():
    path = Path(__file__).resolve().parents[2] / "scripts" / "run-benchmarks.py"
    spec = importlib.util.spec_from_file_location("run_benchmarks", path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def test_summarize_document_counts_pages_blocks_anchors_and_warnings(tmp_path):
    runner = load_runner()
    result = tmp_path / "sample.json"
    result.write_text(
        json.dumps(
            {
                "pages": [
                    {
                        "blocks": [
                            {
                                "type": "text",
                                "bbox": {"x": 1, "y": 2, "width": 3, "height": 4},
                                "source_anchors": [{"page_number": 1}],
                            },
                            {"type": "text", "source_anchors": []},
                        ],
                        "warnings": [{"code": "page.warning"}],
                    },
                    {
                        "blocks": [
                            {"type": "table", "source_anchors": [{"page_number": 2}]}
                        ],
                        "warnings": [],
                    },
                ],
                "warnings": [{"code": "doc.warning"}],
            }
        )
    )

    summary = runner.summarize_document_json(result)

    assert summary.pages == 2
    assert summary.blocks == 3
    assert summary.bbox_blocks == 1
    assert summary.anchored_blocks == 2
    assert summary.warnings == 2
    assert summary.bbox_block_rate == 1 / 3
    assert summary.anchored_block_rate == 2 / 3


def test_native_coverage_score_uses_lowest_position_signal():
    runner = load_runner()

    assert runner.native_coverage_score(1.0, 0.75, 0.5) == 0.5
    assert runner.native_coverage_score(0.8, 0.75, 0.5) == 0.4
    assert runner.native_coverage_score(None, 0.75, 0.5) is None


def test_replace_readme_benchmark_section_is_stable():
    runner = load_runner()
    readme = "before\n\n<!-- BENCHMARKS:START -->\nold\n<!-- BENCHMARKS:END -->\n\nafter\n"
    table = "| Dataset | PDFs | Parse success |\n| --- | ---: | ---: |\n| demo | 1 | 100.0% |"

    updated = runner.replace_readme_benchmark_section(readme, table)

    assert "before" in updated
    assert table in updated
    assert "after" in updated
    assert "old" not in updated
