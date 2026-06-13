"""Run the dongler CLI over an eval suite and score every document.

Usage::

    python -m eval.harness.run --suite smoke --bin ./target/debug/dongler

The suite name resolves to ``eval/<suite>/`` (read by
:class:`~eval.harness.adapters.MarkdownGTAdapter`). For each sample we invoke
``<bin> extract <pdf> --format markdown``, capture stdout, compute per-document
metrics, and write three artifacts under ``eval/out/runs/<run_id>/``:

- ``per_doc.json``   -- one scored row per document (errors recorded, never raised)
- ``aggregate.json`` -- mean per metric + counts
- ``report.md``      -- human-readable summary

A per-document CLI failure (non-zero exit, timeout, crash) is caught and
recorded as a zero-score row carrying the error string; it never aborts the run.
"""
from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional

# Make the repo root importable so ``eval.metrics`` / ``eval.harness`` resolve
# whether invoked as a module or a script.
_REPO_ROOT = Path(__file__).resolve().parents[2]
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from eval.harness.adapters import MarkdownGTAdapter  # noqa: E402
from eval.harness.coverage import spurious_token_rate, token_coverage  # noqa: E402
from eval.metrics.table import teds  # noqa: E402
from eval.metrics.text import (  # noqa: E402
    bleu,
    char_error_rate,
    edit_similarity,
    word_error_rate,
)

# Metric keys that always appear on a scored row (TEDS is conditional).
TEXT_METRIC_KEYS = [
    "edit_similarity",
    "char_error_rate",
    "word_error_rate",
    "bleu",
    "token_coverage",
    "spurious_token_rate",
]
DEFAULT_TIMEOUT_S = 60


def extract_markdown(binary: str, pdf_path: str, timeout: float = DEFAULT_TIMEOUT_S) -> str:
    """Run ``<binary> extract <pdf> --format markdown`` and return stdout.

    Raises ``subprocess.CalledProcessError`` on non-zero exit and
    ``subprocess.TimeoutExpired`` on timeout; callers catch these per-doc.
    """
    result = subprocess.run(
        [binary, "extract", pdf_path, "--format", "markdown"],
        capture_output=True,
        text=True,
        timeout=timeout,
        check=True,
    )
    return result.stdout


def _has_table_gt(sample: Dict[str, Any]) -> Optional[str]:
    """Return GT table HTML to score against, or ``None`` if TEDS doesn't apply.

    TEDS is computed only when the sample meta marks a table *and* a table HTML
    ground truth is available (explicit ``gt_table_html`` or an HTML/grid table
    embedded in the GT markdown).
    """
    meta = sample.get("meta") or {}
    if not meta.get("has_table"):
        return None
    if meta.get("gt_table_html"):
        return str(meta["gt_table_html"])
    gt = sample.get("gt_markdown", "")
    if "<table" in gt.lower():
        return gt
    return None


def score_document(sample: Dict[str, Any], prediction: str) -> Dict[str, float]:
    """Compute all applicable metrics for one prediction against its GT."""
    gt = sample["gt_markdown"]
    scores: Dict[str, float] = {
        "edit_similarity": edit_similarity(gt, prediction),
        "char_error_rate": char_error_rate(gt, prediction),
        "word_error_rate": word_error_rate(gt, prediction),
        "bleu": bleu(gt, prediction),
        "token_coverage": token_coverage(gt, prediction),
        "spurious_token_rate": spurious_token_rate(gt, prediction),
    }
    gt_table_html = _has_table_gt(sample)
    if gt_table_html is not None:
        # TEDS compares HTML tables; score prediction markdown (which may embed
        # an HTML table) against the GT table HTML.
        scores["teds"] = teds(prediction, gt_table_html)
    return scores


def _zero_scores(sample: Dict[str, Any]) -> Dict[str, float]:
    """Worst-case scores for a failed document (records the failure honestly)."""
    scores = {
        "edit_similarity": 0.0,
        "char_error_rate": 1.0,
        "word_error_rate": 1.0,
        "bleu": 0.0,
        "token_coverage": 0.0,
        "spurious_token_rate": 0.0,  # nothing produced -> nothing spurious
    }
    if _has_table_gt(sample) is not None:
        scores["teds"] = 0.0
    return scores


def run_suite(
    suite: str,
    binary: str,
    out_root: Path,
    run_id: str,
    timeout: float = DEFAULT_TIMEOUT_S,
) -> Dict[str, Any]:
    """Run one suite end-to-end and write artifacts. Returns the aggregate dict."""
    suite_dir = _REPO_ROOT / "eval" / suite
    adapter = MarkdownGTAdapter(suite_dir)

    rows: List[Dict[str, Any]] = []
    for sample in adapter:
        row: Dict[str, Any] = {"id": sample["id"], "error": None}
        try:
            prediction = extract_markdown(binary, sample["pdf_path"], timeout=timeout)
            row["scores"] = score_document(sample, prediction)
        except subprocess.TimeoutExpired:
            row["error"] = f"timeout after {timeout}s"
            row["scores"] = _zero_scores(sample)
        except subprocess.CalledProcessError as exc:
            stderr = (exc.stderr or "").strip()
            row["error"] = f"exit {exc.returncode}: {stderr[:500]}"
            row["scores"] = _zero_scores(sample)
        except Exception as exc:  # pragma: no cover - defensive catch-all
            row["error"] = f"{type(exc).__name__}: {exc}"
            row["scores"] = _zero_scores(sample)
        rows.append(row)

    aggregate = aggregate_rows(rows)

    run_dir = out_root / "runs" / run_id
    run_dir.mkdir(parents=True, exist_ok=True)

    per_doc = {"suite": suite, "run_id": run_id, "binary": binary, "documents": rows}
    (run_dir / "per_doc.json").write_text(json.dumps(per_doc, indent=2), encoding="utf-8")
    (run_dir / "aggregate.json").write_text(json.dumps(aggregate, indent=2), encoding="utf-8")
    (run_dir / "report.md").write_text(render_report(suite, run_id, rows, aggregate), encoding="utf-8")

    return aggregate


def aggregate_rows(rows: List[Dict[str, Any]]) -> Dict[str, Any]:
    """Mean per metric across docs + counts. Missing metrics (e.g. TEDS on
    non-table docs) are averaged only over the docs that have them."""
    metric_values: Dict[str, List[float]] = {}
    for row in rows:
        for key, value in (row.get("scores") or {}).items():
            metric_values.setdefault(key, []).append(value)

    means = {
        key: (statistics.fmean(values) if values else 0.0)
        for key, values in metric_values.items()
    }
    error_count = sum(1 for row in rows if row.get("error"))
    return {
        "document_count": len(rows),
        "error_count": error_count,
        "ok_count": len(rows) - error_count,
        "metrics": {"keys": sorted(metric_values), "mean": means},
        "metric_doc_counts": {key: len(values) for key, values in metric_values.items()},
    }


def render_report(
    suite: str, run_id: str, rows: List[Dict[str, Any]], aggregate: Dict[str, Any]
) -> str:
    """Render a markdown report (aggregate table + per-doc table)."""
    lines: List[str] = []
    lines.append(f"# Eval report — suite `{suite}`")
    lines.append("")
    lines.append(f"- run_id: `{run_id}`")
    lines.append(f"- documents: {aggregate['document_count']}")
    lines.append(f"- errors: {aggregate['error_count']}")
    lines.append("")
    lines.append("## Aggregate (mean per metric)")
    lines.append("")
    lines.append("| metric | mean | docs |")
    lines.append("|---|---|---|")
    means = aggregate["metrics"]["mean"]
    counts = aggregate["metric_doc_counts"]
    for key in aggregate["metrics"]["keys"]:
        lines.append(f"| {key} | {means[key]:.4f} | {counts[key]} |")
    lines.append("")
    lines.append("## Per-document")
    lines.append("")
    lines.append("| id | edit_sim | CER | WER | bleu | coverage | spurious | teds | error |")
    lines.append("|---|---|---|---|---|---|---|---|---|")
    for row in rows:
        s = row.get("scores") or {}

        def fmt(key: str) -> str:
            return f"{s[key]:.3f}" if key in s else "—"

        err = row.get("error") or ""
        lines.append(
            f"| {row['id']} | {fmt('edit_similarity')} | {fmt('char_error_rate')} "
            f"| {fmt('word_error_rate')} | {fmt('bleu')} | {fmt('token_coverage')} "
            f"| {fmt('spurious_token_rate')} | {fmt('teds')} | {err} |"
        )
    lines.append("")
    return "\n".join(lines)


def _default_run_id() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Run an eval suite and score documents.")
    parser.add_argument("--suite", default="smoke", help="suite name -> eval/<suite>/")
    parser.add_argument(
        "--bin", dest="binary", default="./target/debug/dongler", help="path to dongler CLI"
    )
    parser.add_argument("--run-id", default=None, help="run id (default: UTC timestamp)")
    parser.add_argument(
        "--out", default=None, help="output root (default: eval/out)"
    )
    parser.add_argument(
        "--timeout", type=float, default=DEFAULT_TIMEOUT_S, help="per-doc CLI timeout (s)"
    )
    args = parser.parse_args(argv)

    run_id = args.run_id or _default_run_id()
    out_root = Path(args.out) if args.out else (_REPO_ROOT / "eval" / "out")

    aggregate = run_suite(
        suite=args.suite,
        binary=args.binary,
        out_root=out_root,
        run_id=run_id,
        timeout=args.timeout,
    )

    run_dir = out_root / "runs" / run_id
    print(f"Run {run_id} -> {run_dir}")
    print(json.dumps(aggregate, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
