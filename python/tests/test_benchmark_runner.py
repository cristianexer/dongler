import importlib.util
import json
import subprocess
import sys
from pathlib import Path


def load_runner():
    path = Path(__file__).resolve().parents[2] / "scripts" / "run-benchmarks.py"
    spec = importlib.util.spec_from_file_location("run_benchmarks", path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def load_downloader():
    path = Path(__file__).resolve().parents[2] / "scripts" / "download-benchmark-data.py"
    spec = importlib.util.spec_from_file_location("download_benchmark_data", path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def load_visual_renderer():
    path = (
        Path(__file__).resolve().parents[2]
        / "scripts"
        / "render-extraction-comparison.py"
    )
    spec = importlib.util.spec_from_file_location("render_extraction_comparison", path)
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


def test_extraction_accuracy_uses_token_f1():
    runner = load_runner()

    assert runner.extraction_accuracy("Alpha beta beta", "alpha beta") == 0.8
    assert runner.extraction_accuracy("Alpha", "") == 0.0
    assert runner.extraction_accuracy("", "Alpha") is None


def test_extraction_accuracy_normalizes_pdf_ligature_controls():
    runner = load_runner()

    assert (
        runner.extraction_accuracy(
            "Classi\u0002cation with \u0003oating point",
            "Classification with floating point",
        )
        == 1.0
    )


def test_full_image_geometry_accuracy_uses_best_page_extent_iou(tmp_path):
    runner = load_runner()
    json_out = tmp_path / "image.json"
    json_out.write_text(
        json.dumps(
            {
                "pages": [
                    {
                        "width": 100,
                        "height": 50,
                        "blocks": [
                            {
                                "type": "figure",
                                "bbox": {"x": 0, "y": 0, "width": 100, "height": 50},
                            }
                        ],
                    }
                ]
            }
        )
    )

    assert runner.full_image_geometry_accuracy(json_out) == 1.0


def test_reference_text_uses_ocr_text_instead_of_csv_coordinates():
    runner = load_runner()

    text = runner.structured_delimited_reference_text(
        "1,2,3,2,3,4,1,4,Store Name\n5,6,7,6,7,8,5,8,Total,RM 10.00\n",
        ",",
    )

    assert text == "Store Name Total RM 10.00"


def test_reference_text_uses_funsd_form_text_without_labels():
    runner = load_runner()

    text = runner.json_reference_text(
        json.dumps(
            {
                "form": [
                    {"text": "Date:", "label": "question", "box": [1, 2, 3, 4]},
                    {"text": "2026-05-28", "label": "answer", "id": 2},
                ]
            }
        )
    )

    assert text == "Date: 2026-05-28"


def test_reference_text_uses_docbank_token_labels_without_coordinates():
    runner = load_runner()

    text = runner.docbank_token_label_reference_text(
        "Hello\t10\t20\t30\t40\t0\t0\t0\tCMR10\tparagraph\n"
        "world\t35\t20\t70\t40\t0\t0\t0\tCMR10\tparagraph\n"
    )

    assert text == "Hello world"


def test_benchmark_dataset_scores_related_docbank_page_reference(tmp_path, monkeypatch):
    runner = load_runner()
    data_root = tmp_path / "data"
    doc_dir = data_root / "docbank"
    doc_dir.mkdir(parents=True)
    pdf = doc_dir / "paper_black.pdf"
    pdf.write_bytes(b"%PDF")
    (doc_dir / "paper_1.txt").write_text(
        "Reference\t10\t20\t30\t40\t0\t0\t0\tCMR10\tparagraph\n"
        "page\t35\t20\t70\t40\t0\t0\t0\tCMR10\tparagraph\n"
    )

    def fake_run_document(_cli, _path, json_out):
        json_out.write_text(
            json.dumps(
                {
                    "pages": [
                        {
                            "number": 1,
                            "blocks": [
                                {"type": "text", "text": "wrong text", "source_anchors": [{}]}
                            ],
                        },
                        {
                            "number": 2,
                            "blocks": [
                                {
                                    "type": "text",
                                    "text": "Reference page",
                                    "source_anchors": [{}],
                                }
                            ],
                        },
                    ]
                }
            )
        )
        return True, 0.25, None

    monkeypatch.setattr(runner, "run_document", fake_run_document)

    result = runner.benchmark_dataset(
        {
            "name": "DocBank",
            "slug": "docbank",
            "task": "token-level labels",
            "local_dir": "docbank",
        },
        data_root,
        tmp_path / "out",
        tmp_path / "dongler",
        max_pdfs=10,
    )

    assert result["status"] == "ok"
    assert result["ground_truth_documents"] == 1
    assert result["ground_truth_accuracy"] == 1.0


def test_olmocr_unit_tests_score_presence_absence_and_order(tmp_path):
    runner = load_runner()
    local_dir = tmp_path / "olmocr-bench"
    pdf_dir = local_dir / "bench_data" / "pdfs" / "baseline"
    pdf_dir.mkdir(parents=True)
    pdf = pdf_dir / "sample.pdf"
    pdf.write_bytes(b"%PDF")
    (local_dir / "bench_data" / "baseline.jsonl").write_text(
        "\n".join(
            [
                json.dumps(
                    {
                        "pdf": "baseline/sample.pdf",
                        "type": "present",
                        "text": "Alpha marker",
                    }
                ),
                json.dumps(
                    {
                        "pdf": "baseline/sample.pdf",
                        "type": "absent",
                        "text": "Removed footer",
                    }
                ),
                json.dumps(
                    {
                        "pdf": "baseline/sample.pdf",
                        "type": "order",
                        "before": "Alpha marker",
                        "after": "Omega marker",
                    }
                ),
            ]
        )
    )
    json_out = tmp_path / "sample.json"
    json_out.write_text(
        json.dumps(
            {
                "pages": [
                    {
                        "blocks": [
                            {
                                "type": "text",
                                "text": "Alpha marker then some body text and finally Omega marker.",
                            }
                        ]
                    }
                ]
            }
        )
    )

    tests = runner.olmocr_tests_for_document(pdf, local_dir)

    assert [test["type"] for test in tests] == ["present", "absent", "order"]
    assert runner.olmocr_unit_pass_rate(tests, json_out) == 1.0


def test_olmocr_unit_tests_honor_fuzzy_diff_and_edge_windows(tmp_path):
    runner = load_runner()
    json_out = tmp_path / "windows.json"
    json_out.write_text(
        json.dumps(
            {
                "pages": [
                    {
                        "blocks": [
                            {
                                "type": "text",
                                "text": "Alpba marker middle body terminal footer",
                            }
                        ]
                    }
                ]
            }
        )
    )
    tests = [
        {"type": "present", "text": "Alpha marker", "max_diffs": 1},
        {"type": "present", "text": "Alpba marker", "first_n": 12},
        {"type": "absent", "text": "middle body", "first_n": 12},
        {"type": "present", "text": "terminal footer", "last_n": 15},
    ]

    assert runner.olmocr_unit_pass_rate(tests, json_out) == 1.0


def test_olmocr_unit_tests_score_table_neighbors_from_ir_cells(tmp_path):
    runner = load_runner()
    json_out = tmp_path / "table.json"
    json_out.write_text(
        json.dumps(
            {
                "pages": [
                    {
                        "blocks": [
                            {
                                "type": "table",
                                "headers": ["Name", "Value"],
                                "rows": [["Alpha", "42"], ["Beta", "99"]],
                            }
                        ]
                    }
                ]
            }
        )
    )
    tests = [
        {
            "type": "table",
            "cell": "42",
            "left": "Alpha",
            "top_heading": "Value",
            "up": "Value",
        },
        {"type": "table", "cell": "99", "left": "Missing"},
    ]

    assert runner.olmocr_unit_pass_rate(tests, json_out) == 0.5


def test_olmocr_unit_check_report_records_failed_checks_with_windows(tmp_path):
    runner = load_runner()
    json_out = tmp_path / "sample.json"
    json_out.write_text(
        json.dumps(
            {
                "pages": [
                    {
                        "blocks": [
                            {
                                "type": "text",
                                "text": "Alpha marker appears before the body but the target phrase is absent.",
                            }
                        ]
                    }
                ]
            }
        )
    )
    tests = [
        {"type": "present", "text": "missing target phrase"},
        {"type": "order", "before": "body", "after": "Alpha marker"},
        {"type": "math", "math": r"\lambda_i \in \mathbb{F}_q"},
    ]

    report = runner.olmocr_unit_check_report(
        tests,
        json_out,
        source_pdf="bench_data/pdfs/arxiv_math/sample.pdf",
        split="arxiv_math",
    )

    assert report.pass_rate == 0.0
    assert report.scored == 3
    assert [failure["check_type"] for failure in report.failures] == [
        "present",
        "order",
        "math",
    ]
    assert report.failures[0]["source_pdf"] == "bench_data/pdfs/arxiv_math/sample.pdf"
    assert report.failures[0]["split"] == "arxiv_math"
    assert report.failures[0]["expected"] == "missing target phrase"
    assert "Alpha marker appears" in report.failures[0]["dongler_window"]
    assert report.failures[1]["expected"] == "body before Alpha marker"
    assert report.failures[1]["likely_failure_category"] == "reading order"
    assert report.failures[2]["likely_failure_category"] == "math preservation"


def test_olmocr_failure_report_keeps_image_placeholders_as_ocr_missing(tmp_path):
    runner = load_runner()
    json_out = tmp_path / "image-only.json"
    json_out.write_text(
        json.dumps(
            {
                "pages": [
                    {
                        "blocks": [
                            {
                                "type": "figure",
                                "alt_text": "Image image-1-Im1",
                                "image_ref": "image-1-Im1",
                            }
                        ]
                    }
                ]
            }
        )
    )

    report = runner.olmocr_unit_check_report(
        [{"type": "present", "text": "missing scanned text"}],
        json_out,
    )

    assert report.failures[0]["likely_failure_category"] == "scanned/OCR missing"


def test_olmocr_unit_check_report_records_best_reference_window(tmp_path):
    runner = load_runner()
    json_out = tmp_path / "sample.json"
    json_out.write_text(
        json.dumps(
            {
                "pages": [
                    {
                        "blocks": [
                            {
                                "type": "text",
                                "text": "Dongler text does not contain the checked marker.",
                            }
                        ]
                    }
                ]
            }
        )
    )
    tests = [{"type": "present", "text": "reference target phrase"}]

    report = runner.olmocr_unit_check_report(
        tests,
        json_out,
        reference_outputs={
            "pypdf": "An unrelated extraction window.",
            "pdfminer": "Before the reference target phrase and after it.",
        },
    )

    assert report.failures[0]["reference_extractor"] == "pdfminer"
    assert "reference target phrase" in report.failures[0]["reference_window"]


def test_olmocr_reference_window_matches_unicode_math_aliases(tmp_path):
    runner = load_runner()
    json_out = tmp_path / "sample.json"
    json_out.write_text(json.dumps({"pages": [{"blocks": [{"type": "text", "text": ""}]}]}))

    report = runner.olmocr_unit_check_report(
        [{"type": "math", "math": r"\theta_1 + ... + \theta_n"}],
        json_out,
        reference_outputs={
            "pypdf": (
                "preface " * 80
                + "The nearby formula is θ_1 + ... + θ_n in the source."
            )
        },
    )

    assert report.failures[0]["reference_extractor"] == "pypdf"
    assert "θ_1 + ... + θ_n" in report.failures[0]["reference_window"]


def test_olmocr_reference_window_is_empty_when_no_reference_matches(tmp_path):
    runner = load_runner()
    json_out = tmp_path / "sample.json"
    json_out.write_text(json.dumps({"pages": [{"blocks": [{"type": "text", "text": ""}]}]}))

    report = runner.olmocr_unit_check_report(
        [{"type": "present", "text": "missing target marker"}],
        json_out,
        reference_outputs={
            "pypdf": "An unrelated page opening with no useful overlap.",
            "pdfminer": "Another extraction that still does not contain the target.",
        },
    )

    assert report.failures[0]["reference_extractor"] is None
    assert report.failures[0]["reference_window"] is None


def test_benchmark_dataset_includes_olmocr_failure_reports(tmp_path, monkeypatch):
    runner = load_runner()
    data_root = tmp_path / "data"
    local_dir = data_root / "olmocr-bench"
    pdf_dir = local_dir / "bench_data" / "pdfs" / "multi_column"
    pdf_dir.mkdir(parents=True)
    pdf = pdf_dir / "sample.pdf"
    pdf.write_bytes(b"%PDF")
    (local_dir / "bench_data" / "multi_column.jsonl").write_text(
        "\n".join(
            [
                json.dumps(
                    {
                        "pdf": "multi_column/sample.pdf",
                        "type": "present",
                        "text": "visible phrase",
                    }
                ),
                json.dumps(
                    {
                        "pdf": "multi_column/sample.pdf",
                        "type": "order",
                        "before": "left column",
                        "after": "right column",
                    }
                ),
            ]
        )
    )

    def fake_run_document(_cli, _path, json_out):
        json_out.write_text(
            json.dumps(
                {
                    "pages": [
                        {
                            "blocks": [
                                {
                                    "type": "text",
                                    "text": "visible phrase right column then left column",
                                }
                            ]
                        }
                    ]
                }
            )
        )
        return True, 0.25, None

    monkeypatch.setattr(runner, "run_document", fake_run_document)

    result = runner.benchmark_dataset(
        {
            "name": "olmOCR-Bench",
            "slug": "olmocr-bench",
            "task": "unit-style OCR/PDF parsing tests",
            "local_dir": "olmocr-bench",
        },
        data_root,
        tmp_path / "out",
        tmp_path / "dongler",
        max_pdfs=10,
    )

    assert result["ground_truth_accuracy"] == 0.5
    assert result["olmocr_failure_count"] == 1
    assert result["olmocr_failures"][0]["check_type"] == "order"
    assert result["olmocr_failures"][0]["split"] == "multi_column"
    assert "right column then left column" in result["olmocr_failures"][0]["dongler_window"]


def test_benchmark_dataset_adds_reference_windows_when_enabled(tmp_path, monkeypatch):
    runner = load_runner()
    data_root = tmp_path / "data"
    local_dir = data_root / "olmocr-bench"
    pdf_dir = local_dir / "bench_data" / "pdfs" / "present"
    pdf_dir.mkdir(parents=True)
    pdf = pdf_dir / "sample.pdf"
    pdf.write_bytes(b"%PDF")
    (local_dir / "bench_data" / "present.jsonl").write_text(
        json.dumps(
            {
                "pdf": "present/sample.pdf",
                "type": "present",
                "text": "reference only marker",
            }
        )
        + "\n"
    )

    def fake_run_document(_cli, _path, json_out):
        json_out.write_text(
            json.dumps(
                {
                    "pages": [
                        {
                            "blocks": [
                                {"type": "text", "text": "Dongler missed the marker."}
                            ]
                        }
                    ]
                }
            )
        )
        return True, 0.25, None

    def fake_reference_outputs(path, extractors):
        assert path == pdf
        assert extractors == ["pdfminer"]
        return {"pdfminer": "A reference only marker appears here."}

    monkeypatch.setattr(runner, "run_document", fake_run_document)
    monkeypatch.setattr(
        runner,
        "reference_extractor_outputs_for_pdf",
        fake_reference_outputs,
    )

    result = runner.benchmark_dataset(
        {
            "name": "olmOCR-Bench",
            "slug": "olmocr-bench",
            "task": "unit-style OCR/PDF parsing tests",
            "local_dir": "olmocr-bench",
        },
        data_root,
        tmp_path / "out",
        tmp_path / "dongler",
        max_pdfs=10,
        reference_extractors=["pdfminer"],
    )

    assert result["olmocr_failures"][0]["reference_extractor"] == "pdfminer"
    assert "reference only marker" in result["olmocr_failures"][0]["reference_window"]


def test_benchmark_dataset_can_enable_ocr_fallback_for_child_extractions(
    tmp_path, monkeypatch
):
    runner = load_runner()
    data_root = tmp_path / "data"
    local_dir = data_root / "olmocr-bench"
    pdf_dir = local_dir / "bench_data" / "pdfs" / "old_scans"
    pdf_dir.mkdir(parents=True)
    pdf = pdf_dir / "sample.pdf"
    pdf.write_bytes(b"%PDF")
    (local_dir / "bench_data" / "old_scans.jsonl").write_text(
        json.dumps(
            {
                "pdf": "old_scans/sample.pdf",
                "type": "present",
                "text": "recognized OCR text",
            }
        )
        + "\n"
    )
    monkeypatch.delenv("DONGLER_OCR_FALLBACK", raising=False)

    def fake_run_document(_cli, _path, json_out):
        assert runner.os.environ["DONGLER_OCR_FALLBACK"] == "1"
        json_out.write_text(
            json.dumps(
                {
                    "pages": [
                        {
                            "blocks": [
                                {"type": "text", "text": "recognized OCR text"}
                            ]
                        }
                    ]
                }
            )
        )
        return True, 0.25, None

    monkeypatch.setattr(runner, "run_document", fake_run_document)

    result = runner.benchmark_dataset(
        {
            "name": "olmOCR-Bench",
            "slug": "olmocr-bench",
            "task": "unit-style OCR/PDF parsing tests",
            "local_dir": "olmocr-bench",
        },
        data_root,
        tmp_path / "out",
        tmp_path / "dongler",
        max_pdfs=10,
        ocr_fallback=True,
    )

    assert result["ground_truth_accuracy"] == 1.0
    assert "DONGLER_OCR_FALLBACK" not in runner.os.environ


def test_benchmark_dataset_scores_olmocr_jsonl_unit_checks(tmp_path, monkeypatch):
    runner = load_runner()
    data_root = tmp_path / "data"
    local_dir = data_root / "olmocr-bench"
    pdf_dir = local_dir / "bench_data" / "pdfs" / "baseline"
    pdf_dir.mkdir(parents=True)
    (pdf_dir / "sample.pdf").write_bytes(b"%PDF")
    (local_dir / "bench_data" / "baseline.jsonl").write_text(
        json.dumps(
            {
                "pdf": "baseline/sample.pdf",
                "type": "present",
                "text": "semantic markdown",
            }
        )
        + "\n"
    )

    def fake_run_document(_cli, _path, json_out):
        json_out.write_text(
            json.dumps(
                {
                    "pages": [
                        {
                            "blocks": [
                                {
                                    "type": "text",
                                    "text": "The output contains semantic markdown.",
                                }
                            ]
                        }
                    ]
                }
            )
        )
        return True, 0.25, None

    monkeypatch.setattr(runner, "run_document", fake_run_document)

    result = runner.benchmark_dataset(
        {
            "name": "olmOCR-Bench",
            "slug": "olmocr-bench",
            "task": "unit-style OCR/PDF parsing tests",
            "local_dir": "olmocr-bench",
        },
        data_root,
        tmp_path / "out",
        tmp_path / "dongler",
        max_pdfs=10,
    )

    assert result["status"] == "ok"
    assert result["ground_truth_documents"] == 1
    assert result["ground_truth_accuracy"] == 1.0
    assert result["ground_truth_metric"] == "olmocr_unit_pass_rate"


def test_benchmark_dataset_weights_olmocr_accuracy_by_check_count(
    tmp_path, monkeypatch
):
    runner = load_runner()
    data_root = tmp_path / "data"
    local_dir = data_root / "olmocr-bench"
    pdf_dir = local_dir / "bench_data" / "pdfs" / "baseline"
    pdf_dir.mkdir(parents=True)
    (pdf_dir / "many.pdf").write_bytes(b"%PDF")
    (pdf_dir / "one.pdf").write_bytes(b"%PDF")
    checks = []
    for index in range(10):
        checks.append(
            {
                "pdf": "baseline/many.pdf",
                "type": "present",
                "text": f"pass {index}",
            }
        )
    checks.append(
        {
            "pdf": "baseline/one.pdf",
            "type": "present",
            "text": "missing",
        }
    )
    (local_dir / "bench_data" / "baseline.jsonl").write_text(
        "\n".join(json.dumps(check) for check in checks)
    )

    def fake_run_document(_cli, path, json_out):
        if path.name == "many.pdf":
            text = " ".join(f"pass {index}" for index in range(10))
        else:
            text = "different"
        json_out.write_text(
            json.dumps({"pages": [{"blocks": [{"type": "text", "text": text}]}]})
        )
        return True, 0.25, None

    monkeypatch.setattr(runner, "run_document", fake_run_document)

    result = runner.benchmark_dataset(
        {
            "name": "olmOCR-Bench",
            "slug": "olmocr-bench",
            "task": "unit-style OCR/PDF parsing tests",
            "local_dir": "olmocr-bench",
        },
        data_root,
        tmp_path / "out",
        tmp_path / "dongler",
        max_pdfs=10,
    )

    assert result["ground_truth_documents"] == 2
    assert result["ground_truth_accuracy"] == 10 / 11


def test_benchmark_dataset_balances_pdf_selection_across_subdirectories(
    tmp_path, monkeypatch
):
    runner = load_runner()
    data_root = tmp_path / "data"
    local_dir = data_root / "split-dataset"
    for split in ["alpha", "beta"]:
        split_dir = local_dir / split
        split_dir.mkdir(parents=True)
        for index in range(3):
            (split_dir / f"{index}.pdf").write_bytes(b"%PDF")
    evaluated = []

    def fake_run_document(_cli, path, json_out):
        evaluated.append(path.relative_to(local_dir).as_posix())
        json_out.write_text(json.dumps({"pages": [{"blocks": []}]}))
        return True, 0.25, None

    monkeypatch.setattr(runner, "run_document", fake_run_document)

    runner.benchmark_dataset(
        {
            "name": "Split Dataset",
            "slug": "split-dataset",
            "task": "balanced selection",
            "local_dir": "split-dataset",
        },
        data_root,
        tmp_path / "out",
        tmp_path / "dongler",
        max_pdfs=4,
    )

    assert evaluated == [
        "alpha/0.pdf",
        "beta/0.pdf",
        "alpha/1.pdf",
        "beta/1.pdf",
    ]


def test_benchmark_dataset_evaluates_all_pdfs_without_limit(tmp_path, monkeypatch):
    runner = load_runner()
    data_root = tmp_path / "data"
    local_dir = data_root / "all-dataset"
    for split in ["alpha", "beta"]:
        split_dir = local_dir / split
        split_dir.mkdir(parents=True)
        for index in range(3):
            (split_dir / f"{index}.pdf").write_bytes(b"%PDF")
    evaluated = []

    def fake_run_document(_cli, path, json_out):
        evaluated.append(path.relative_to(local_dir).as_posix())
        json_out.write_text(json.dumps({"pages": [{"blocks": []}]}))
        return True, 0.25, None

    monkeypatch.setattr(runner, "run_document", fake_run_document)

    result = runner.benchmark_dataset(
        {
            "name": "All Dataset",
            "slug": "all-dataset",
            "task": "uncapped selection",
            "local_dir": "all-dataset",
        },
        data_root,
        tmp_path / "out",
        tmp_path / "dongler",
        max_pdfs=None,
    )

    assert result["documents_evaluated"] == 6
    assert evaluated == [
        "alpha/0.pdf",
        "alpha/1.pdf",
        "alpha/2.pdf",
        "beta/0.pdf",
        "beta/1.pdf",
        "beta/2.pdf",
    ]


def test_benchmark_dataset_balances_olmocr_pdf_selection_across_splits(
    tmp_path, monkeypatch
):
    runner = load_runner()
    data_root = tmp_path / "data"
    local_dir = data_root / "olmocr-bench"
    for split in ["arxiv_math", "tables"]:
        split_dir = local_dir / "bench_data" / "pdfs" / split
        split_dir.mkdir(parents=True)
        for index in range(3):
            (split_dir / f"{index}.pdf").write_bytes(b"%PDF")
    evaluated = []

    def fake_run_document(_cli, path, json_out):
        evaluated.append(path.relative_to(local_dir / "bench_data" / "pdfs").as_posix())
        json_out.write_text(json.dumps({"pages": [{"blocks": []}]}))
        return True, 0.25, None

    monkeypatch.setattr(runner, "run_document", fake_run_document)

    runner.benchmark_dataset(
        {
            "name": "olmOCR-Bench",
            "slug": "olmocr-bench",
            "task": "balanced selection",
            "local_dir": "olmocr-bench",
        },
        data_root,
        tmp_path / "out",
        tmp_path / "dongler",
        max_pdfs=4,
    )

    assert evaluated == [
        "arxiv_math/0.pdf",
        "tables/0.pdf",
        "arxiv_math/1.pdf",
        "tables/1.pdf",
    ]


def test_download_defaults_include_readoc_markdown_subset(tmp_path, monkeypatch):
    downloader = load_downloader()
    manifest = {
        "datasets": [
            {
                "name": "READoc",
                "slug": "readoc",
                "download": {
                    "kind": "hf",
                    "repo": "lazyc/READoc",
                    "include": ["github_ground_truth/*.md", "arxiv_ground_truth/*.md"],
                },
            }
        ]
    }
    (tmp_path / "manifest.json").write_text(json.dumps(manifest))
    selected = []

    def fake_download_dataset(dataset, _data_root, _repo_root, _budget_bytes):
        selected.append(dataset)
        return {
            "dataset": dataset["name"],
            "slug": dataset["slug"],
            "status": "downloaded",
            "bytes_before": 0,
            "bytes_after": 1,
            "error": None,
            "local_dir": str(tmp_path / dataset["slug"]),
        }

    monkeypatch.chdir(tmp_path)
    monkeypatch.setattr(downloader, "download_dataset", fake_download_dataset)
    monkeypatch.setattr(
        sys,
        "argv",
        ["download-benchmark-data.py", "--manifest", "manifest.json"],
    )

    assert downloader.main() == 0
    assert [dataset["slug"] for dataset in selected] == ["readoc"]
    assert selected[0]["download"]["include"] == [
        "github_ground_truth/*.md",
        "arxiv_ground_truth/*.md",
    ]


def test_replace_readme_benchmark_section_is_stable():
    runner = load_runner()
    readme = "before\n\n<!-- BENCHMARKS:START -->\nold\n<!-- BENCHMARKS:END -->\n\nafter\n"
    table = "| Dataset | PDFs | Parse success |\n| --- | ---: | ---: |\n| demo | 1 | 100.0% |"

    updated = runner.replace_readme_benchmark_section(readme, table)

    assert "before" in updated
    assert table in updated
    assert "after" in updated
    assert "old" not in updated


def test_markdown_table_uses_compact_renderable_columns():
    runner = load_runner()
    table = runner.markdown_table(
        [
            {
                "dataset": "DemoBench",
                "task": "layout | extraction",
                "status": "ok",
                "local_bytes": 10 * 1024 * 1024,
                "pdfs_found": 4,
                "images_found": 2,
                "pdfs_evaluated": 2,
                "documents_evaluated": 2,
                "parse_success_rate": 1.0,
                "bbox_block_rate": 0.5,
                "anchored_block_rate": 0.75,
                "native_coverage_score": 0.5,
                "pages_per_second": 42.25,
                "ground_truth_documents": 2,
                "ground_truth_accuracy": 0.91,
                "notes": "long note | with pipes that should not widen the table",
            }
        ],
        "2026-05-28 17:08:41 BST",
        50,
    )

    assert "Coverage is `parse / bbox / anchors`" in table
    header = next(line for line in table.splitlines() if line.startswith("| Dataset |"))
    assert header == (
        "| Dataset | Status | Local data | Docs eval | Coverage | Pages/sec | GT accuracy |"
    )
    assert "Task" not in header
    assert "Notes" not in header
    assert {line.count("|") for line in table.splitlines() if line.startswith("|")} == {8}
    assert "| DemoBench | ok | 10.0 MB | 2 | 100.0% / 50.0% / 75.0% | 42.25 | 91.0% |" in table
    assert "long note" not in table


def test_markdown_table_describes_uncapped_benchmarks():
    runner = load_runner()
    table = runner.markdown_table(
        [
            {
                "dataset": "DemoBench",
                "status": "ok",
                "local_bytes": 10 * 1024 * 1024,
                "pdfs_evaluated": 3,
                "documents_evaluated": 3,
                "parse_success_rate": 1.0,
                "bbox_block_rate": 1.0,
                "anchored_block_rate": 1.0,
                "pages_per_second": 25.0,
                "ground_truth_accuracy": None,
            }
        ],
        "2026-05-28 17:08:41 BST",
        None,
    )

    assert "All discovered files per dataset" in table
    assert "Cap:" not in table


def test_benchmark_dataset_evaluates_images_when_no_pdfs(tmp_path, monkeypatch):
    runner = load_runner()
    data_root = tmp_path / "data"
    image_dir = data_root / "image-dataset"
    image_dir.mkdir(parents=True)
    (image_dir / "sample.png").write_bytes(b"fake image bytes")

    def fake_run_document(_cli, _path, json_out):
        json_out.write_text(
            json.dumps(
                {
                    "pages": [
                        {
                            "width": 20,
                            "height": 10,
                            "blocks": [
                                {
                                    "type": "figure",
                                    "bbox": {"x": 0, "y": 0, "width": 20, "height": 10},
                                }
                            ],
                        }
                    ],
                    "warnings": [],
                }
            )
        )
        return True, 0.25, None

    monkeypatch.setattr(runner, "run_document", fake_run_document)

    result = runner.benchmark_dataset(
        {
            "name": "Image Dataset",
            "slug": "image-dataset",
            "task": "image extraction",
            "local_dir": "image-dataset",
        },
        data_root,
        tmp_path / "out",
        tmp_path / "dongler",
        max_pdfs=10,
    )

    assert result["status"] == "ok"
    assert result["pdfs_found"] == 0
    assert result["images_found"] == 1
    assert result["documents_evaluated"] == 1
    assert result["parse_success_rate"] == 1.0
    assert result["pages_per_second"] == 4.0
    assert result["ground_truth_documents"] == 1
    assert result["ground_truth_accuracy"] == 1.0
    assert result["ground_truth_metric"] == "full_image_iou"


def test_discover_images_ignores_macos_resource_fork_sidecars(tmp_path):
    runner = load_runner()
    (tmp_path / "scan.png").write_bytes(b"png")
    (tmp_path / "._scan.png").write_bytes(b"sidecar")

    images = runner.discover_images(tmp_path)

    assert images == [tmp_path / "scan.png"]


def test_discover_supported_documents_includes_compound_gzip_suffixes(tmp_path):
    runner = load_runner()
    (tmp_path / "papers.jsonl.gz").write_bytes(b"gz")
    (tmp_path / "article.nxml.gz").write_bytes(b"gz")
    (tmp_path / "2401.00001.gz").write_bytes(b"gz")
    (tmp_path / "archive.zip").write_bytes(b"zip")

    documents = runner.discover_supported_documents(tmp_path)

    assert documents == [
        tmp_path / "2401.00001.gz",
        tmp_path / "archive.zip",
        tmp_path / "article.nxml.gz",
        tmp_path / "papers.jsonl.gz",
    ]


def test_benchmark_dataset_evaluates_supported_documents_when_no_pdfs_or_images(
    tmp_path, monkeypatch
):
    runner = load_runner()
    data_root = tmp_path / "data"
    doc_dir = data_root / "document-dataset"
    doc_dir.mkdir(parents=True)
    (doc_dir / "report.docx").write_bytes(b"docx")

    def fake_run_document(_cli, _path, json_out):
        json_out.write_text(json.dumps({"pages": [{"blocks": [{"type": "text"}]}]}))
        return True, 0.5, None

    monkeypatch.setattr(runner, "run_document", fake_run_document)

    result = runner.benchmark_dataset(
        {
            "name": "Document Dataset",
            "slug": "document-dataset",
            "task": "document extraction",
            "local_dir": "document-dataset",
        },
        data_root,
        tmp_path / "out",
        tmp_path / "dongler",
        max_pdfs=10,
    )

    assert result["status"] == "ok"
    assert result["documents_found"] == 1
    assert result["documents_evaluated"] == 1
    assert result["pages_per_second"] == 2.0
    assert result["bbox_block_rate"] is None
    assert result["native_coverage_score"] is None


def test_benchmark_dataset_scores_markdown_ground_truth_accuracy(tmp_path, monkeypatch):
    runner = load_runner()
    data_root = tmp_path / "data"
    doc_dir = data_root / "readoc"
    doc_dir.mkdir(parents=True)
    (doc_dir / "sample.md").write_text("# Title\n\nBody text")

    def fake_run_document(_cli, _path, json_out):
        json_out.write_text(
            json.dumps(
                {
                    "pages": [
                        {
                            "blocks": [
                                {"type": "text", "text": "Title", "source_anchors": [{}]},
                                {"type": "text", "text": "Body text", "source_anchors": [{}]},
                            ]
                        }
                    ]
                }
            )
        )
        return True, 0.25, None

    monkeypatch.setattr(runner, "run_document", fake_run_document)

    result = runner.benchmark_dataset(
        {
            "name": "READoc",
            "slug": "readoc",
            "task": "structured extraction",
            "local_dir": "readoc",
        },
        data_root,
        tmp_path / "out",
        tmp_path / "dongler",
        max_pdfs=10,
    )

    assert result["status"] == "ok"
    assert result["ground_truth_documents"] == 1
    assert result["ground_truth_accuracy"] == 1.0
    assert result["ground_truth_accuracy_by_document"][0]["document"].endswith("sample.md")


def test_benchmark_dataset_can_emit_visual_comparison_artifacts(tmp_path, monkeypatch):
    runner = load_runner()
    data_root = tmp_path / "data"
    doc_dir = data_root / "pdf-dataset"
    doc_dir.mkdir(parents=True)
    (doc_dir / "sample.pdf").write_bytes(b"%PDF")

    def fake_run_document(_cli, _path, json_out):
        json_out.write_text(
            json.dumps(
                {
                    "pages": [
                        {
                            "blocks": [
                                {"type": "text", "text": "Body", "source_anchors": [{}]}
                            ]
                        }
                    ]
                }
            )
        )
        return True, 0.25, None

    def fake_render_visual_comparison(_repo_root, _cli, document, visual_out_dir):
        return {
            "source": str(document),
            "original_png": str(visual_out_dir / "sample" / "original.png"),
            "markdown_png": str(visual_out_dir / "sample" / "markdown.png"),
            "latex_png": str(visual_out_dir / "sample" / "latex.png"),
        }, None

    monkeypatch.setattr(runner, "run_document", fake_run_document)
    monkeypatch.setattr(runner, "render_visual_comparison", fake_render_visual_comparison)

    result = runner.benchmark_dataset(
        {
            "name": "PDF Dataset",
            "slug": "pdf-dataset",
            "task": "document extraction",
            "local_dir": "pdf-dataset",
        },
        data_root,
        tmp_path / "out",
        tmp_path / "dongler",
        max_pdfs=10,
        repo_root=tmp_path,
        visual_samples=1,
        visual_out_dir=tmp_path / "visuals",
    )

    assert result["status"] == "ok"
    assert len(result["visual_comparisons"]) == 1
    assert result["visual_comparisons"][0]["document"].endswith("sample.pdf")
    assert result["visual_comparisons"][0]["artifacts"]["markdown_png"].endswith("markdown.png")


def test_prepare_visual_output_dir_removes_stale_artifacts_when_enabled(tmp_path):
    runner = load_runner()
    visual_dir = tmp_path / "visuals"
    stale = visual_dir / "old-dataset" / "extracted.md"
    stale.parent.mkdir(parents=True)
    stale.write_text("")

    runner.prepare_visual_output_dir(visual_dir, enabled=True)

    assert visual_dir.exists()
    assert not stale.exists()


def test_visual_comparison_writes_structured_latex_error_artifact(tmp_path, monkeypatch):
    renderer = load_visual_renderer()
    document = tmp_path / "sample.pdf"
    document.write_bytes(b"%PDF")

    def fake_run_extract(_cli, _document, output_format):
        return "broken $ markdown" if output_format == "markdown" else "safe latex"

    def fake_render_pdf_first_page(_pdf, output_prefix):
        png = output_prefix.with_suffix(".png")
        png.write_bytes(b"png")
        return png

    def fake_compile_latex(tex_path, output_dir):
        if tex_path.name == "extracted-markdown-render.tex":
            raise subprocess.CalledProcessError(
                1,
                ["pdflatex", str(tex_path)],
                output="compile stdout",
                stderr="compile stderr",
            )
        pdf = output_dir / f"{tex_path.stem}.pdf"
        pdf.write_bytes(b"%PDF")
        return pdf

    monkeypatch.setattr(renderer, "run_extract", fake_run_extract)
    monkeypatch.setattr(renderer, "render_pdf_first_page", fake_render_pdf_first_page)
    monkeypatch.setattr(renderer, "compile_latex", fake_compile_latex)
    monkeypatch.setattr(
        sys,
        "argv",
        [
            "render-extraction-comparison.py",
            str(document),
            "--cli",
            str(tmp_path / "dongler"),
            "--out-dir",
            str(tmp_path / "visuals"),
        ],
    )

    assert renderer.main() == 0

    manifest = json.loads(
        (tmp_path / "visuals" / "sample" / "comparison.json").read_text()
    )
    assert manifest["source"] == str(document)
    assert "markdown_render_error" in manifest
    error = json.loads(Path(manifest["markdown_render_error"]).read_text())
    assert error["stage"] == "markdown_latex_compile"
    assert "compile stderr" in error["stderr"]
    assert manifest["latex_png"].endswith("latex.png")


def test_benchmark_dataset_scores_structured_json_geometry_when_available(
    tmp_path, monkeypatch
):
    runner = load_runner()
    data_root = tmp_path / "data"
    doc_dir = data_root / "json-dataset"
    doc_dir.mkdir(parents=True)
    (doc_dir / "annotations.json").write_text("[]")

    def fake_run_document(_cli, _path, json_out):
        json_out.write_text(
            json.dumps(
                {
                    "pages": [
                        {
                            "blocks": [
                                {
                                    "type": "text",
                                    "bbox": {
                                        "x": 0,
                                        "y": 0,
                                        "width": 10,
                                        "height": 10,
                                    },
                                    "source_anchors": [
                                        {
                                            "page_number": 1,
                                            "bbox": {
                                                "x": 0,
                                                "y": 0,
                                                "width": 10,
                                                "height": 10,
                                            },
                                        }
                                    ],
                                }
                            ]
                        }
                    ]
                }
            )
        )
        return True, 0.25, None

    monkeypatch.setattr(runner, "run_document", fake_run_document)

    result = runner.benchmark_dataset(
        {
            "name": "JSON Dataset",
            "slug": "json-dataset",
            "task": "annotation extraction",
            "local_dir": "json-dataset",
        },
        data_root,
        tmp_path / "out",
        tmp_path / "dongler",
        max_pdfs=10,
    )

    assert result["status"] == "ok"
    assert result["documents_found"] == 1
    assert result["documents_evaluated"] == 1
    assert result["bbox_block_rate"] == 1.0
    assert result["anchored_block_rate"] == 1.0
    assert result["native_coverage_score"] == 1.0


def test_benchmark_dataset_prefers_structured_annotations_over_images(
    tmp_path, monkeypatch
):
    runner = load_runner()
    data_root = tmp_path / "data"
    doc_dir = data_root / "form-dataset"
    doc_dir.mkdir(parents=True)
    (doc_dir / "scan.png").write_bytes(b"png")
    (doc_dir / "annotations.json").write_text("{}")
    evaluated = []

    def fake_run_document(_cli, path, json_out):
        evaluated.append(path)
        json_out.write_text(json.dumps({"pages": [{"blocks": []}]}))
        return True, 0.25, None

    monkeypatch.setattr(runner, "run_document", fake_run_document)

    result = runner.benchmark_dataset(
        {
            "name": "Form Dataset",
            "slug": "form-dataset",
            "task": "form extraction",
            "local_dir": "form-dataset",
        },
        data_root,
        tmp_path / "out",
        tmp_path / "dongler",
        max_pdfs=10,
    )

    assert result["status"] == "ok"
    assert result["images_found"] == 1
    assert result["documents_found"] == 1
    assert evaluated == [doc_dir / "annotations.json"]


def test_benchmark_dataset_continues_with_other_documents_after_structured_subset(
    tmp_path, monkeypatch
):
    runner = load_runner()
    data_root = tmp_path / "data"
    doc_dir = data_root / "mixed-dataset"
    doc_dir.mkdir(parents=True)
    (doc_dir / "article.xml").write_text("<article><p>xml</p></article>")
    (doc_dir / "target.txt").write_text("text")
    evaluated = []

    def fake_run_document(_cli, path, json_out):
        evaluated.append(path.name)
        json_out.write_text(json.dumps({"pages": [{"blocks": []}]}))
        return True, 0.25, None

    monkeypatch.setattr(runner, "run_document", fake_run_document)

    result = runner.benchmark_dataset(
        {
            "name": "Mixed Dataset",
            "slug": "mixed-dataset",
            "task": "document extraction",
            "local_dir": "mixed-dataset",
        },
        data_root,
        tmp_path / "out",
        tmp_path / "dongler",
        max_pdfs=10,
    )

    assert result["status"] == "ok"
    assert result["documents_found"] == 2
    assert evaluated == ["article.xml", "target.txt"]
