"""Compare two eval runs and gate on regressions (PRD §3.5).

Usage::

    python -m eval.harness.diff <baseline_run_id> <candidate_run_id>

Loads ``per_doc.json`` from both runs, classifies each shared document as
improved / regressed / unchanged by its ``edit_similarity`` delta (beyond an
epsilon, default 0.005), prints the counts and the top-10 worst regressions,
writes ``diff.md`` into the candidate run dir, and exits non-zero if any
document regressed beyond epsilon (so it can gate CI).
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

_REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_EPSILON = 0.005
PRIMARY_METRIC = "edit_similarity"


def _load_per_doc(run_id: str, out_root: Path) -> Dict[str, Any]:
    path = out_root / "runs" / run_id / "per_doc.json"
    if not path.exists():
        raise FileNotFoundError(f"no per_doc.json for run '{run_id}' at {path}")
    return json.loads(path.read_text(encoding="utf-8"))


def _scores_by_id(per_doc: Dict[str, Any]) -> Dict[str, Dict[str, float]]:
    return {row["id"]: (row.get("scores") or {}) for row in per_doc.get("documents", [])}


def compute_diff(
    baseline: Dict[str, Any],
    candidate: Dict[str, Any],
    epsilon: float = DEFAULT_EPSILON,
    metric: str = PRIMARY_METRIC,
) -> Dict[str, Any]:
    """Classify shared docs and collect regressions, sorted worst-first."""
    base_scores = _scores_by_id(baseline)
    cand_scores = _scores_by_id(candidate)
    shared = sorted(set(base_scores) & set(cand_scores))

    improved: List[Tuple[str, float]] = []
    regressed: List[Tuple[str, float]] = []
    unchanged: List[str] = []

    for doc_id in shared:
        base_val = base_scores[doc_id].get(metric, 0.0)
        cand_val = cand_scores[doc_id].get(metric, 0.0)
        delta = cand_val - base_val
        if delta > epsilon:
            improved.append((doc_id, delta))
        elif delta < -epsilon:
            regressed.append((doc_id, delta))
        else:
            unchanged.append(doc_id)

    regressed.sort(key=lambda item: item[1])  # most negative first
    improved.sort(key=lambda item: item[1], reverse=True)

    return {
        "metric": metric,
        "epsilon": epsilon,
        "shared": shared,
        "only_in_baseline": sorted(set(base_scores) - set(cand_scores)),
        "only_in_candidate": sorted(set(cand_scores) - set(base_scores)),
        "improved": improved,
        "regressed": regressed,
        "unchanged": unchanged,
    }


def render_diff_md(baseline_id: str, candidate_id: str, diff: Dict[str, Any]) -> str:
    lines: List[str] = []
    lines.append(f"# Eval diff — `{baseline_id}` → `{candidate_id}`")
    lines.append("")
    lines.append(f"- metric: `{diff['metric']}`  (epsilon = {diff['epsilon']})")
    lines.append(f"- improved: {len(diff['improved'])}")
    lines.append(f"- regressed: {len(diff['regressed'])}")
    lines.append(f"- unchanged: {len(diff['unchanged'])}")
    if diff["only_in_baseline"]:
        lines.append(f"- only in baseline: {', '.join(diff['only_in_baseline'])}")
    if diff["only_in_candidate"]:
        lines.append(f"- only in candidate: {', '.join(diff['only_in_candidate'])}")
    lines.append("")
    lines.append("## Top-10 worst regressions")
    lines.append("")
    if not diff["regressed"]:
        lines.append("_None._")
    else:
        lines.append("| id | delta |")
        lines.append("|---|---|")
        for doc_id, delta in diff["regressed"][:10]:
            lines.append(f"| {doc_id} | {delta:+.4f} |")
    lines.append("")
    return "\n".join(lines)


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Diff two eval runs; gate on regressions.")
    parser.add_argument("baseline_run_id")
    parser.add_argument("candidate_run_id")
    parser.add_argument("--epsilon", type=float, default=DEFAULT_EPSILON)
    parser.add_argument("--metric", default=PRIMARY_METRIC)
    parser.add_argument("--out", default=None, help="output root (default: eval/out)")
    args = parser.parse_args(argv)

    out_root = Path(args.out) if args.out else (_REPO_ROOT / "eval" / "out")
    baseline = _load_per_doc(args.baseline_run_id, out_root)
    candidate = _load_per_doc(args.candidate_run_id, out_root)

    diff = compute_diff(baseline, candidate, epsilon=args.epsilon, metric=args.metric)

    print(f"Diff {args.baseline_run_id} -> {args.candidate_run_id} (metric={args.metric})")
    print(f"  improved:  {len(diff['improved'])}")
    print(f"  regressed: {len(diff['regressed'])}")
    print(f"  unchanged: {len(diff['unchanged'])}")
    if diff["regressed"]:
        print("  top-10 worst regressions:")
        for doc_id, delta in diff["regressed"][:10]:
            print(f"    {doc_id}: {delta:+.4f}")

    diff_md = render_diff_md(args.baseline_run_id, args.candidate_run_id, diff)
    cand_dir = out_root / "runs" / args.candidate_run_id
    cand_dir.mkdir(parents=True, exist_ok=True)
    (cand_dir / "diff.md").write_text(diff_md, encoding="utf-8")

    # Non-zero exit gates CI when any document regressed beyond epsilon.
    return 1 if diff["regressed"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
