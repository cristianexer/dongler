#!/usr/bin/env python3
"""End-to-end fidelity demo: render real PDFs with `dongler convert` and score
the Markdown against each document's own text layer (pdftotext -layout).

Covers three real datasets: sec-10k (financial tables), docbank (academic prose),
olmocr-bench (multi-column / tables). Writes per-example artifacts under
eval/out/examples/<dataset>/ and a summary.json + summary.md.

Usage: uv run python eval/run_e2e_examples.py
"""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))

from eval.metrics.text import edit_similarity, char_error_rate  # noqa: E402
from eval.harness.coverage import token_coverage, spurious_token_rate  # noqa: E402

CLI = ROOT / "target" / "debug" / "dongler"
OUT = ROOT / "eval" / "out" / "examples"


def run(cmd: list[str]) -> str:
    return subprocess.run(cmd, capture_output=True, text=True, timeout=120).stdout


def pdftotext_layout(pdf: Path, first: int | None = None, last: int | None = None) -> str:
    cmd = ["pdftotext", "-layout"]
    if first:
        cmd += ["-f", str(first)]
    if last:
        cmd += ["-l", str(last)]
    cmd += [str(pdf), "-"]
    return run(cmd)


def split_page(pdf: Path, page: int, dest: Path) -> Path:
    """Extract a single page into its own 1-page PDF via qpdf."""
    dest.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["qpdf", str(pdf), "--pages", str(pdf), str(page), "--", str(dest)],
        check=True, capture_output=True,
    )
    return dest


def convert_md(pdf: Path) -> str:
    return run([str(CLI), "convert", str(pdf), "--format", "markdown"])


# Char-level Levenshtein is O(n*m); cap it so full-paper docs don't explode.
_EDIT_SIM_MAX_CHARS = 20000


def score(gt: str, pred: str) -> dict:
    out = {
        "token_coverage": round(token_coverage(gt, pred), 4),
        "spurious_token_rate": round(spurious_token_rate(gt, pred), 4),
        "gt_chars": len(gt),
        "pred_chars": len(pred),
    }
    if max(len(gt), len(pred)) <= _EDIT_SIM_MAX_CHARS:
        out["edit_similarity"] = round(edit_similarity(gt, pred), 4)
        out["char_error_rate"] = round(char_error_rate(gt, pred), 4)
    else:
        out["edit_similarity"] = None
        out["char_error_rate"] = None
    return out


def emit(dataset: str, name: str, pdf: Path, md: str, gt: str) -> dict:
    d = OUT / dataset
    d.mkdir(parents=True, exist_ok=True)
    (d / f"{name}.md").write_text(md)
    (d / f"{name}.gt.txt").write_text(gt)
    s = score(gt, md)
    s.update({"dataset": dataset, "name": name, "pdf": str(pdf.relative_to(ROOT))})
    print(f"[{dataset}] {name}: edit_sim={s['edit_similarity']} "
          f"coverage={s['token_coverage']} spurious={s['spurious_token_rate']}",
          flush=True)
    return s


def sec10k(results: list) -> None:
    pages = json.loads((ROOT / "eval/data/sec-10k/eval-pages.json").read_text())
    tmp = OUT / "_tmp"
    # pages[] = [cover, fin1, fin2, fin3, toc]; use the financial pages (idx 1..3)
    for rec in pages[:4]:
        ticker = rec["ticker"]
        pdf = Path(rec["pdf"])
        if not pdf.exists():
            continue
        page = rec["pages"][1]  # first financial/table-heavy page
        single = split_page(pdf, page, tmp / f"{ticker}_p{page}.pdf")
        md = convert_md(single)
        gt = pdftotext_layout(single)
        results.append(emit("sec-10k", f"{ticker}_page{page}", single, md, gt))


def docbank(results: list) -> None:
    sample_dir = ROOT / "eval/data/docbank/DocBank_samples/DocBank_samples"
    pdfs = sorted(sample_dir.glob("*.pdf"))[:2]
    for pdf in pdfs:
        md = convert_md(pdf)
        gt = pdftotext_layout(pdf)
        results.append(emit("docbank", pdf.stem, pdf, md, gt))


def olmocr(results: list) -> None:
    base = ROOT / "eval/data/olmocr-bench/bench_data/pdfs"
    picks = []
    for cat in ("multi_column", "headers_footers", "tables"):
        cands = sorted((base / cat).glob("*.pdf"))
        if cands:
            picks.append((cat, cands[0]))
    for cat, pdf in picks:
        md = convert_md(pdf)
        gt = pdftotext_layout(pdf)
        results.append(emit("olmocr-bench", f"{cat}__{pdf.stem[:16]}", pdf, md, gt))


def main() -> None:
    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir(parents=True)
    results: list = []
    sec10k(results)
    docbank(results)
    olmocr(results)
    (OUT / "summary.json").write_text(json.dumps(results, indent=2))
    # markdown summary
    lines = ["# E2E fidelity examples\n",
             "| Dataset | Example | Edit sim | Coverage | Spurious | CER |",
             "| --- | --- | ---: | ---: | ---: | ---: |"]
    for r in results:
        lines.append(f"| {r['dataset']} | {r['name']} | {r['edit_similarity']} | "
                     f"{r['token_coverage']} | {r['spurious_token_rate']} | {r['char_error_rate']} |")
    (OUT / "summary.md").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))


if __name__ == "__main__":
    main()
