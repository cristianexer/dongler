#!/usr/bin/env python3
"""Re-score saved e2e examples with a content-fair normalization.

The raw edit_similarity vs `pdftotext -layout` is unfair to a structured
Markdown extractor: the layout text is whitespace-padded and dongler emits
compact Markdown tables (whose `|`/`---` delimiters also count as "spurious").

This pass normalizes BOTH sides to comparable content streams:
  - lowercase
  - drop Markdown table-delimiter tokens (`|`, `---`, `:--`, `**`, `#`)
  - collapse all whitespace
then reports edit_similarity (content), token_coverage, and token_jaccard.
Reads the artifacts produced by run_e2e_examples.py; no PDF re-conversion.
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
from eval.metrics.text import edit_similarity  # noqa: E402

OUT = ROOT / "eval" / "out" / "examples"
_DELIM = re.compile(r"[|#*`]+|:?-{2,}:?")
_WS = re.compile(r"\s+")


def norm_text(s: str) -> str:
    s = _DELIM.sub(" ", s)
    return _WS.sub(" ", s).strip().lower()


def toks(s: str) -> list[str]:
    return norm_text(s).split()


def jaccard(a: set, b: set) -> float:
    return len(a & b) / len(a | b) if (a or b) else 1.0


def coverage(gt: list[str], pred: list[str]) -> float:
    from collections import Counter
    g, p = Counter(gt), Counter(pred)
    covered = sum(min(n, p.get(t, 0)) for t, n in g.items())
    return covered / sum(g.values()) if g else 1.0


def main() -> None:
    rows = []
    for f in sorted(OUT.rglob("*.md")):
        gt_f = f.with_suffix(".gt.txt")
        if f.name == "summary.md" or not gt_f.exists():
            continue
        md, gt = f.read_text(), gt_f.read_text()
        ng, nm = norm_text(gt), norm_text(md)
        gt_t, md_t = ng.split(), nm.split()
        ces = round(edit_similarity(ng, nm), 4) if max(len(ng), len(nm)) <= 20000 else None
        rows.append({
            "dataset": f.parent.name,
            "name": f.stem,
            "content_edit_sim": ces,
            "token_coverage": round(coverage(gt_t, md_t), 4),
            "token_jaccard": round(jaccard(set(gt_t), set(md_t)), 4),
            "gt_tokens": len(gt_t),
        })
    (OUT / "rescore.json").write_text(json.dumps(rows, indent=2))
    hdr = "| Dataset | Example | Content edit-sim | Token coverage | Token Jaccard | GT tokens |"
    sep = "| --- | --- | ---: | ---: | ---: | ---: |"
    lines = [hdr, sep]
    for r in rows:
        lines.append(f"| {r['dataset']} | {r['name']} | {r['content_edit_sim']} | "
                     f"{r['token_coverage']} | {r['token_jaccard']} | {r['gt_tokens']} |")
    (OUT / "rescore.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


if __name__ == "__main__":
    main()
