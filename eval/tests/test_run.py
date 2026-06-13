"""End-to-end smoke run test (PRD §3.4)."""
from __future__ import annotations

import json
from pathlib import Path

import pytest

from eval.harness.adapters import MarkdownGTAdapter
from eval.harness.run import TEXT_METRIC_KEYS, run_suite

_REPO_ROOT = Path(__file__).resolve().parents[2]
_BIN = _REPO_ROOT / "target" / "debug" / "dongler"


@pytest.mark.skipif(not _BIN.exists(), reason="dongler CLI binary not built")
def test_smoke_run_produces_full_artifacts(tmp_path):
    aggregate = run_suite(
        suite="smoke",
        binary=str(_BIN),
        out_root=tmp_path,
        run_id="testrun",
    )

    run_dir = tmp_path / "runs" / "testrun"
    assert (run_dir / "per_doc.json").exists()
    assert (run_dir / "aggregate.json").exists()
    assert (run_dir / "report.md").exists()

    # one row per smoke doc
    smoke_docs = list(MarkdownGTAdapter(_REPO_ROOT / "eval" / "smoke"))
    per_doc = json.loads((run_dir / "per_doc.json").read_text())
    assert len(per_doc["documents"]) == len(smoke_docs)
    assert len(smoke_docs) >= 6  # the committed slice

    # aggregate carries every text metric key
    mean = aggregate["metrics"]["mean"]
    for key in TEXT_METRIC_KEYS:
        assert key in mean

    # no per-doc CLI errors expected on the committed smoke slice
    assert aggregate["error_count"] == 0

    # paragraph cases should score high edit_similarity (proves harness works)
    rows = {row["id"]: row for row in per_doc["documents"]}
    assert rows["a_single_paragraph"]["scores"]["edit_similarity"] > 0.95
