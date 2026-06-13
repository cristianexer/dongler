"""Diff regression-gating tests (PRD §3.5)."""
from __future__ import annotations

from eval.harness.diff import compute_diff


def _per_doc(scores_by_id):
    return {"documents": [{"id": k, "scores": v} for k, v in scores_by_id.items()]}


def test_identical_runs_report_clean():
    run = _per_doc(
        {
            "doc1": {"edit_similarity": 0.9},
            "doc2": {"edit_similarity": 0.5},
        }
    )
    diff = compute_diff(run, run)
    assert diff["regressed"] == []
    assert len(diff["unchanged"]) == 2
    assert diff["improved"] == []


def test_detects_regression_on_degraded_run():
    baseline = _per_doc(
        {
            "doc1": {"edit_similarity": 0.90},
            "doc2": {"edit_similarity": 0.80},
        }
    )
    candidate = _per_doc(
        {
            "doc1": {"edit_similarity": 0.90},  # unchanged
            "doc2": {"edit_similarity": 0.40},  # regressed by 0.40
        }
    )
    diff = compute_diff(baseline, candidate)
    assert len(diff["regressed"]) == 1
    worst_id, worst_delta = diff["regressed"][0]
    assert worst_id == "doc2"
    assert worst_delta < -0.005


def test_detects_improvement():
    baseline = _per_doc({"doc1": {"edit_similarity": 0.40}})
    candidate = _per_doc({"doc1": {"edit_similarity": 0.90}})
    diff = compute_diff(baseline, candidate)
    assert len(diff["improved"]) == 1
    assert diff["regressed"] == []


def test_epsilon_treats_tiny_deltas_as_unchanged():
    baseline = _per_doc({"doc1": {"edit_similarity": 0.9000}})
    candidate = _per_doc({"doc1": {"edit_similarity": 0.9030}})  # +0.003 < eps
    diff = compute_diff(baseline, candidate, epsilon=0.005)
    assert diff["unchanged"] == ["doc1"]
    assert diff["regressed"] == []
    assert diff["improved"] == []
