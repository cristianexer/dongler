#!/usr/bin/env python3
"""Select a small, diverse set of pages from many 10-Ks for extraction iteration.

Iterating on one filing (Apple) overfits. This picks a handful of *varied* page
types from each of several companies across sectors — a cover/registration page,
the densest financial-statement/table pages, and a prose (MD&A) page — so the
extractor is exercised against real diversity. The page types are chosen by a
document-agnostic table-density score (multi-column numeric rows via pdftotext),
so nothing here is 10-K specific.

Writes eval/data/sec-10k/eval-pages.json: a list of {ticker, sector, pdf, pages}.
"""
from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CORPUS = REPO / "eval" / "data" / "sec-10k"

# One representative filing per sector bucket (diverse layouts, fonts, table styles).
COMPANIES = [
    "AAPL", "MSFT", "JPM", "XOM", "JNJ", "WMT", "BA", "NEE",
    "AMT", "VZ", "TSLA", "LIN", "UNH", "CAT", "PG", "GS",
]
PAGES_PER_COMPANY = 5


def page_count(pdf: Path) -> int:
    out = subprocess.run(["pdfinfo", str(pdf)], capture_output=True, text=True).stdout
    for line in out.splitlines():
        if line.startswith("Pages:"):
            return int(line.split(":", 1)[1].strip())
    return 0


def table_density(pdf: Path, page: int) -> int:
    txt = subprocess.run(
        ["pdftotext", "-f", str(page), "-l", str(page), "-layout", str(pdf), "-"],
        capture_output=True, text=True,
    ).stdout
    score = 0
    for line in txt.splitlines():
        cols = [c for c in re.split(r"\s{2,}", line.strip()) if c]
        numeric = sum(1 for c in cols if re.search(r"\d", c))
        if len(cols) >= 3 and numeric >= 2:
            score += 1
    return score


def pick_pages(pdf: Path, total: int) -> list[int]:
    """Cover page + 3 densest table pages + 1 prose page, de-duplicated."""
    scores = {p: table_density(pdf, p) for p in range(1, total + 1)}
    dense = sorted(scores, key=lambda p: (-scores[p], p))
    table_pages = [p for p in dense if scores[p] > 0][:3]
    # A prose page: lowest non-trivial density in the document's first half (MD&A).
    prose_candidates = sorted(
        (p for p in range(2, max(3, total // 2)) if p not in table_pages),
        key=lambda p: scores[p],
    )
    prose = prose_candidates[:1]
    chosen: list[int] = []
    for p in [1, *table_pages, *prose]:
        if p not in chosen and 1 <= p <= total:
            chosen.append(p)
    return chosen[:PAGES_PER_COMPANY]


def main() -> int:
    manifest = json.loads((CORPUS / "manifest.json").read_text())
    by_ticker = {m["ticker"]: m for m in manifest}
    out = []
    for ticker in COMPANIES:
        meta = by_ticker.get(ticker)
        if not meta:
            print(f"  {ticker}: not in corpus, skipping")
            continue
        pdf = REPO / meta["pdf"] if not Path(meta["pdf"]).is_absolute() else Path(meta["pdf"])
        if not pdf.exists():
            print(f"  {ticker}: pdf missing, skipping")
            continue
        total = page_count(pdf)
        pages = pick_pages(pdf, total)
        out.append({"ticker": ticker, "sector": meta["sector"], "pdf": str(pdf), "pages": pages})
        print(f"  {ticker} ({meta['sector']}): pages {pages} of {total}")
    dest = CORPUS / "eval-pages.json"
    dest.write_text(json.dumps(out, indent=2))
    print(f"\nwrote {dest} ({len(out)} companies, {sum(len(c['pages']) for c in out)} pages)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
