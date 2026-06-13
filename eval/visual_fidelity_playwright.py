#!/usr/bin/env python3
"""Playwright visual-fidelity harness: render every PDF page beside dongler's
Markdown -> HTML and flag low-fidelity pages.

For each page:
  [ ORIGINAL pdf page (pdftoppm) ]  |  [ dongler convert -> Markdown -> HTML (Playwright) ]
plus a per-page "glued words" count (tokens dongler emits that are two
ground-truth words concatenated — the word-spacing failure) and a coarse
visual-coverage note.

Unlike the token-coverage metrics (which reward "the words are present
somewhere" and hid the spacing/table failures), this surfaces what a human
actually sees. Run:

  uv run python eval/visual_fidelity_playwright.py <pdf> [--pages 1-10] [--worst N]

Outputs eval/out/visual/<stem>/page-<N>.compare.png + scorecard.json.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent.parent
CLI = ROOT / "target" / "debug" / "dongler"
DPI = 140

CSS = """
body { font: 13px -apple-system, Helvetica, Arial, sans-serif; margin: 22px;
       color: #111; line-height: 1.4; width: 900px; }
table { border-collapse: collapse; margin: 8px 0; }
th, td { border: 1px solid #bbb; padding: 3px 7px; font-size: 12px; vertical-align: top; }
th { background: #f3f4f6; }
h1,h2,h3 { margin: 8px 0 3px; }
"""


def sh(cmd: list[str]) -> str:
    return subprocess.run(cmd, capture_output=True, text=True, timeout=120).stdout


def page_count(pdf: Path) -> int:
    for line in sh(["pdfinfo", str(pdf)]).splitlines():
        if line.startswith("Pages:"):
            return int(line.split(":")[1])
    return 0


def split_page(pdf: Path, page: int, dest: Path) -> None:
    subprocess.run(["qpdf", str(pdf), "--pages", str(pdf), str(page), "--", str(dest)],
                   check=True, capture_output=True)


def convert_md(pdf: Path) -> str:
    return sh([str(CLI), "convert", str(pdf), "--format", "markdown"])


def gt_text(pdf: Path) -> str:
    return sh(["pdftotext", "-layout", str(pdf), "-"])


def glued_words(md: str, gt: str) -> list[str]:
    """Tokens dongler emits that are two ground-truth words concatenated."""
    gtset = {w.lower() for w in re.findall(r"[A-Za-z]+", gt) if len(w) > 1}
    out = []
    for tok in re.findall(r"[A-Za-z]{8,}", md):
        low = tok.lower()
        if low in gtset:
            continue
        for i in range(3, len(low) - 2):
            if low[:i] in gtset and low[i:] in gtset:
                out.append(f"{low[:i]}|{low[i:]}")
                break
    return out


def render_md_png(md: str, page: "Page", out_png: Path) -> None:  # noqa: F821
    import markdown
    body = markdown.markdown(md, extensions=["tables", "md_in_html"])
    page.set_content(f"<style>{CSS}</style><body>{body}</body>")
    page.screenshot(path=str(out_png), full_page=True)


def stitch(src: Path, ren: Path, out: Path) -> None:
    from PIL import Image, ImageDraw, ImageFont
    s, r = Image.open(src).convert("RGB"), Image.open(ren).convert("RGB")
    colw = 900

    def fit(im):
        return im.resize((colw, int(im.height * colw / im.width)))
    s, r = fit(s), fit(r)

    def lbl(text):
        b = Image.new("RGB", (colw, 28), "#1f2937")
        d = ImageDraw.Draw(b)
        try:
            f = ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial Bold.ttf", 16)
        except OSError:
            f = ImageFont.load_default()
        d.text((8, 5), text, fill="white", font=f)
        return b
    h = max(s.height, r.height) + 28
    canvas = Image.new("RGB", (colw * 2 + 10, h), "white")
    canvas.paste(lbl("ORIGINAL PDF PAGE"), (0, 0))
    canvas.paste(lbl("dongler convert -> Markdown -> HTML"), (colw + 10, 0))
    canvas.paste(s, (0, 28))
    canvas.paste(r, (colw + 10, 28))
    canvas.save(out)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("pdf")
    ap.add_argument("--pages", default="", help="e.g. 1-10 (default: all, capped 30)")
    ap.add_argument("--worst", type=int, default=0, help="only render the N worst pages")
    args = ap.parse_args()

    pdf = Path(args.pdf)
    out = ROOT / "eval" / "out" / "visual" / pdf.stem
    out.mkdir(parents=True, exist_ok=True)
    tmp = out / "_tmp"
    tmp.mkdir(exist_ok=True)

    total = page_count(pdf)
    if args.pages:
        a, _, b = args.pages.partition("-")
        pages = list(range(int(a), int(b or a) + 1))
    else:
        pages = list(range(1, min(total, 30) + 1))

    # Score every candidate page first (cheap), then render.
    scored = []
    for p in pages:
        sp = tmp / f"p{p}.pdf"
        split_page(pdf, p, sp)
        md, gt = convert_md(sp), gt_text(sp)
        glued = glued_words(md, gt)
        scored.append({"page": p, "glued": len(glued), "examples": glued[:8], "pdf": str(sp)})

    if args.worst:
        scored.sort(key=lambda r: -r["glued"])
        render = scored[:args.worst]
    else:
        render = scored

    with sync_playwright() as pw:
        browser = pw.chromium.launch()
        page = browser.new_page(viewport={"width": 960, "height": 1400})
        for row in render:
            p = row["page"]
            sp = Path(row["pdf"])
            subprocess.run(["pdftoppm", "-r", str(DPI), "-png", "-singlefile",
                            str(sp), str(tmp / f"src{p}")], check=True, capture_output=True)
            render_md_png(convert_md(sp), page, tmp / f"ren{p}.png")
            stitch(tmp / f"src{p}.png", tmp / f"ren{p}.png", out / f"page-{p}.compare.png")
            print(f"page {p}: glued_words={row['glued']}  e.g. {row['examples'][:5]}")
        browser.close()

    (out / "scorecard.json").write_text(json.dumps(scored, indent=2))
    total_glued = sum(r["glued"] for r in scored)
    print(f"\nTotal glued words across {len(scored)} pages: {total_glued}")
    print(f"Artifacts: {out}")


if __name__ == "__main__":
    main()
