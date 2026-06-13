#!/usr/bin/env python3
"""Table-reconstruction eval: extract EVERY table from a PDF, rebuild each in
Markdown + HTML, score intrinsic fidelity (no ground truth), and emit a scrollable
HTML gallery so you can judge whether the reconstructed tables still make sense.

  uv run python eval/table_recon_eval.py [pdf]   # default: eval/data/repro/msft.pdf

Outputs under eval/out/tables/<stem>/:
  report.html   gallery: each table rendered as HTML, its Markdown, quality flags,
                grouped by page with the source page image alongside
  summary.json  per-table metrics + flags
  summary.md    one-row-per-table scorecard

Fidelity is judged with intrinsic signals (the doc has no table ground truth):
letter-spacing, prose contamination, numeric coverage, structure, emptiness.
"""
from __future__ import annotations

import html as html_mod
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CLI = ROOT / "target" / "release" / "dongler"
DPI = 120

NUM_RE = re.compile(r"^\(?\$?-?[\d,]+(?:\.\d+)?\)?%?$|^—$|^-$|^\$$|^%$")


def sh(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, timeout=300, **kw)


def is_numeric(cell: str) -> bool:
    c = cell.strip()
    return bool(c) and bool(NUM_RE.match(c))


def letter_spaced(cell: str) -> bool:
    """Cell text like 'P r o d u c t i v i t y' — mostly single-char tokens."""
    toks = cell.split()
    if len(toks) < 4:
        return False
    singles = sum(1 for t in toks if len(t) == 1)
    return singles / len(toks) >= 0.6


def cell_grid(tbl: dict) -> list[list[str]]:
    rows = []
    if tbl.get("headers"):
        rows.append([str(c) for c in tbl["headers"]])
    for r in tbl.get("rows", []):
        rows.append([str(c) for c in r])
    return rows


def md_table(grid: list[list[str]]) -> str:
    if not grid:
        return ""
    width = max(len(r) for r in grid)
    def row(cells):
        cells = list(cells) + [""] * (width - len(cells))
        return "| " + " | ".join(c.replace("|", "\\|") for c in cells) + " |"
    out = [row(grid[0]), "| " + " | ".join(["---"] * width) + " |"]
    out += [row(r) for r in grid[1:]]
    return "\n".join(out)


def html_table(tbl: dict, grid: list[list[str]]) -> str:
    # Prefer pre-rendered HTML (carries colspan/rowspan); else build from grid.
    if tbl.get("html") and tbl["html"].strip():
        return tbl["html"]
    if not grid:
        return ""
    rows = []
    for i, r in enumerate(grid):
        tag = "th" if i == 0 else "td"
        cells = "".join(f"<{tag}>{html_mod.escape(c)}</{tag}>" for c in r)
        rows.append(f"<tr>{cells}</tr>")
    return "<table>" + "".join(rows) + "</table>"


def score(grid: list[list[str]]) -> dict:
    cells = [c for r in grid for c in r]
    nonempty = [c for c in cells if c.strip()]
    numeric = [c for c in nonempty if is_numeric(c)]
    ls = [c for c in nonempty if letter_spaced(c)]
    prose = [c for c in nonempty if len(c.split()) > 12]
    n = len(cells) or 1
    flags = []
    if grid and ls:
        flags.append("LETTER_SPACED")
    if prose:
        flags.append("PROSE_CONTAM")
    if nonempty and len(numeric) / len(nonempty) < 0.10 and len(grid) > 1:
        flags.append("NO_NUMBERS")
    if len(nonempty) / n < 0.30:
        flags.append("MOSTLY_EMPTY")
    rect = len({len(r) for r in grid}) == 1
    if not rect:
        flags.append("RAGGED")
    verdict = "clean" if not flags else ("broken" if {"LETTER_SPACED", "PROSE_CONTAM"} & set(flags) else "minor")
    return {
        "rows": len(grid),
        "cols": max((len(r) for r in grid), default=0),
        "nonempty": len(nonempty),
        "numeric_pct": round(len(numeric) / len(nonempty), 3) if nonempty else 0.0,
        "letter_spaced_cells": len(ls),
        "prose_cells": len(prose),
        "flags": flags,
        "verdict": verdict,
        "ls_examples": ls[:2],
        "prose_examples": [p[:60] for p in prose[:2]],
    }


def page_png(pdf: Path, page: int, out: Path) -> Path | None:
    prefix = out / f"page-{page}.src"
    sh(["pdftoppm", "-f", str(page), "-l", str(page), "-r", str(DPI), "-png",
        "-singlefile", str(pdf), str(prefix)])
    p = prefix.with_suffix(".png")
    return p if p.exists() else None


def main() -> None:
    pdf = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "eval/data/repro/msft.pdf"
    out = ROOT / "eval" / "out" / "tables" / pdf.stem
    out.mkdir(parents=True, exist_ok=True)

    doc = json.loads(sh([str(CLI), "convert", str(pdf), "--format", "json"]).stdout)

    results = []
    for page in doc.get("pages", []):
        pnum = page.get("number", 0)
        tindex = 0
        for block in page.get("blocks", []):
            if block.get("type") != "table":
                continue
            tindex += 1
            grid = cell_grid(block)
            s = score(grid)
            s.update({"page": pnum, "table": tindex})
            s["_md"] = md_table(grid)
            s["_html"] = html_table(block, grid)
            results.append(s)

    # Render source page images for pages that have tables.
    pages_with_tables = sorted({r["page"] for r in results})
    for pnum in pages_with_tables:
        page_png(pdf, pnum, out)

    # Verdict tally.
    tally = {"clean": 0, "minor": 0, "broken": 0}
    for r in results:
        tally[r["verdict"]] += 1

    # ---- HTML gallery ----
    css = """
    body{font:14px -apple-system,Helvetica,Arial,sans-serif;margin:24px;color:#111;background:#fafafa}
    h1{margin:0 0 4px} .sub{color:#666;margin-bottom:20px}
    .page{display:flex;gap:20px;align-items:flex-start;margin:28px 0;padding-top:18px;border-top:2px solid #ddd}
    .src{flex:0 0 440px;position:sticky;top:10px} .src img{width:440px;border:1px solid #ccc}
    .tables{flex:1}
    .tbl{background:#fff;border:1px solid #ddd;border-radius:8px;padding:12px 14px;margin-bottom:16px}
    .tbl h3{margin:0 0 8px;font-size:14px}
    .badge{display:inline-block;padding:1px 8px;border-radius:10px;font-size:12px;font-weight:600;margin-left:8px}
    .clean{background:#dcfce7;color:#166534}.minor{background:#fef9c3;color:#854d0e}.broken{background:#fee2e2;color:#991b1b}
    .flag{display:inline-block;background:#fee2e2;color:#991b1b;border-radius:4px;padding:0 6px;font-size:11px;margin-right:4px}
    table{border-collapse:collapse;margin:6px 0;font-size:12px}
    th,td{border:1px solid #bbb;padding:2px 6px;text-align:left;vertical-align:top}
    th{background:#f3f4f6}
    details{margin-top:6px}pre{background:#f6f8fa;padding:8px;overflow:auto;font-size:11px;border-radius:4px}
    .meta{color:#666;font-size:12px}
    """
    parts = [f"<!doctype html><meta charset=utf-8><style>{css}</style>",
             f"<h1>Table reconstruction — {pdf.name}</h1>",
             f"<div class=sub>{len(results)} tables across {len(pages_with_tables)} pages &middot; "
             f"<b style='color:#166534'>{tally['clean']} clean</b> / "
             f"<b style='color:#854d0e'>{tally['minor']} minor</b> / "
             f"<b style='color:#991b1b'>{tally['broken']} broken</b></div>"]

    by_page: dict[int, list[dict]] = {}
    for r in results:
        by_page.setdefault(r["page"], []).append(r)

    for pnum in pages_with_tables:
        img = f"page-{pnum}.src.png"
        parts.append("<div class=page>")
        parts.append(f"<div class=src><div class=meta>page {pnum}</div><img src='{img}'></div>")
        parts.append("<div class=tables>")
        for r in by_page[pnum]:
            badge = f"<span class='badge {r['verdict']}'>{r['verdict']}</span>"
            flags = "".join(f"<span class=flag>{f}</span>" for f in r["flags"])
            parts.append("<div class=tbl>")
            parts.append(f"<h3>Table {r['table']} {badge}</h3>")
            parts.append(f"<div class=meta>{r['rows']}×{r['cols']} &middot; "
                         f"numeric {int(r['numeric_pct']*100)}% &middot; {r['nonempty']} cells {flags}</div>")
            parts.append(r["_html"])
            parts.append(f"<details><summary>Markdown</summary><pre>{html_mod.escape(r['_md'])}</pre></details>")
            parts.append("</div>")
        parts.append("</div></div>")

    (out / "report.html").write_text("".join(parts))

    # ---- summary.md / json ----
    clean = [r for r in results if not r.get("_md") is None]
    for r in results:
        r.pop("_md", None); r.pop("_html", None)
    (out / "summary.json").write_text(json.dumps(results, indent=2))
    lines = ["| Page | Tbl | Size | Numeric% | Verdict | Flags |", "| ---: | ---: | --- | ---: | --- | --- |"]
    for r in results:
        lines.append(f"| {r['page']} | {r['table']} | {r['rows']}×{r['cols']} | "
                     f"{int(r['numeric_pct']*100)}% | {r['verdict']} | {', '.join(r['flags'])} |")
    (out / "summary.md").write_text("\n".join(lines) + "\n")

    print(f"{len(results)} tables: {tally['clean']} clean / {tally['minor']} minor / {tally['broken']} broken")
    print(f"Report: {out/'report.html'}")


if __name__ == "__main__":
    main()
