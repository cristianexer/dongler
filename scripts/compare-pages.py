#!/usr/bin/env python3
"""Page-aligned visual diff: original PDF page vs dongler's Markdown and LaTeX.

For each selected page of a PDF this builds a single side-by-side triptych PNG:

    [ ORIGINAL page N ] [ extracted MARKDOWN, rendered ] [ extracted LaTeX, rendered ]

Unlike a whole-document render (whose page breaks never line up with the source),
this splits the PDF into single-page PDFs first, so every comparison is truly
page-aligned. Pages are chosen by a *document-agnostic* table-density heuristic
(rows that look like multi-column numeric tables, measured with pdftotext) so the
tool surfaces the table-heavy pages that stress extraction — for any PDF, not just
10-Ks.

Run it with Pillow available, e.g.:
    uv run --no-project --with pillow python scripts/compare-pages.py FILE.pdf --table-pages 4

Outputs under --out-dir/<stem>/:
    page-<N>.triptych.png   the side-by-side image
    page-<N>.md / .tex      the extracted text for that page
    summary.json            per-page scores + artifact paths
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import re
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

REPO = Path(__file__).resolve().parent.parent

# Reuse the markdown->LaTeX shim + escaping from the existing comparison script
# (filename has dashes, so load it by path).
_spec = importlib.util.spec_from_file_location(
    "render_extraction_comparison", REPO / "scripts" / "render-extraction-comparison.py"
)
_rec = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_rec)
markdown_to_latex_document = _rec.markdown_to_latex_document


def run(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def page_count(pdf: Path) -> int:
    out = run(["pdfinfo", str(pdf)]).stdout
    for line in out.splitlines():
        if line.startswith("Pages:"):
            return int(line.split(":", 1)[1].strip())
    return 0


def page_text_layout(pdf: Path, page: int) -> str:
    return run(["pdftotext", "-f", str(page), "-l", str(page), "-layout", str(pdf), "-"]).stdout


def table_density_score(text: str) -> int:
    """Count rows that look like multi-column numeric table rows (doc-agnostic)."""
    score = 0
    for line in text.splitlines():
        line = line.rstrip()
        if not line.strip():
            continue
        cols = [c for c in re.split(r"\s{2,}", line.strip()) if c]
        numeric = sum(1 for c in cols if re.search(r"\d", c))
        if len(cols) >= 3 and numeric >= 2:
            score += 1
    return score


def pick_pages(pdf: Path, k: int, total: int) -> list[tuple[int, int]]:
    scored = []
    for p in range(1, total + 1):
        scored.append((p, table_density_score(page_text_layout(pdf, p))))
    # Prefer the densest table pages; skip the cover (page 1) when we have choice.
    ranked = sorted(scored, key=lambda ps: (-ps[1], ps[0]))
    ranked = [ps for ps in ranked if ps[1] > 0] or ranked
    return ranked[:k]


def render_page_image(pdf: Path, page: int, prefix: Path, dpi: int) -> Image.Image | None:
    """Render a single page of the full PDF (no splitting — Chrome PDFs resist it)."""
    run(["pdftoppm", "-f", str(page), "-l", str(page), "-r", str(dpi), "-png", str(pdf), str(prefix)], check=False)
    pages = sorted(prefix.parent.glob(f"{prefix.name}-*.png"))
    if not pages:
        return None
    return Image.open(pages[0]).convert("RGB")


def extract(cli: Path, pdf: Path, fmt: str, page: int) -> str:
    r = run([str(cli), "extract", str(pdf), "--format", fmt, "--pages", str(page)])
    return r.stdout if r.returncode == 0 else f"% extraction failed: {r.stderr.strip()}"


def pdf_to_image(pdf: Path, prefix: Path, dpi: int) -> Image.Image | None:
    """Render every page of `pdf` and stack them vertically into one image."""
    run(["pdftoppm", "-r", str(dpi), "-png", str(pdf), str(prefix)], check=False)
    pages = sorted(prefix.parent.glob(f"{prefix.name}-*.png"))
    if not pages:
        single = prefix.with_suffix(".png")
        pages = [single] if single.exists() else []
    if not pages:
        return None
    imgs = [Image.open(p).convert("RGB") for p in pages]
    width = max(i.width for i in imgs)
    total_h = sum(i.height for i in imgs) + 8 * (len(imgs) - 1)
    canvas = Image.new("RGB", (width, total_h), "white")
    y = 0
    for im in imgs:
        canvas.paste(im, (0, y))
        y += im.height + 8
    return canvas


def compile_latex_to_image(tex: str, work: Path, name: str, dpi: int) -> Image.Image:
    tex_path = work / f"{name}.tex"
    tex_path.write_text(tex)
    proc = run(
        ["pdflatex", "-interaction=nonstopmode", "-halt-on-error",
         f"-output-directory={work}", str(tex_path)],
        cwd=work,
    )
    pdf = work / f"{name}.pdf"
    if pdf.exists():
        img = pdf_to_image(pdf, work / f"{name}-img", dpi)
        if img:
            return img
    return error_image(f"LaTeX render failed\n\n{(proc.stdout or proc.stderr)[-1500:]}")


def error_image(message: str, width: int = 900) -> Image.Image:
    lines = []
    for raw in message.splitlines():
        while len(raw) > 110:
            lines.append(raw[:110])
            raw = raw[110:]
        lines.append(raw)
    img = Image.new("RGB", (width, 20 + 14 * len(lines)), "#fff0f0")
    draw = ImageDraw.Draw(img)
    for i, line in enumerate(lines):
        draw.text((8, 8 + 14 * i), line, fill="#8b0000")
    return img


def label_strip(text: str, width: int, height: int = 34) -> Image.Image:
    img = Image.new("RGB", (width, height), "#1f2937")
    draw = ImageDraw.Draw(img)
    try:
        font = ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial Bold.ttf", 20)
    except OSError:
        font = ImageFont.load_default()
    draw.text((10, 6), text, fill="white", font=font)
    return img


def column(image: Image.Image, title: str, col_w: int, col_h: int) -> Image.Image:
    scale = min(col_w / image.width, col_h / image.height)
    scaled = image.resize((max(1, int(image.width * scale)), max(1, int(image.height * scale))))
    canvas = Image.new("RGB", (col_w, col_h + 34), "white")
    canvas.paste(label_strip(title, col_w), (0, 0))
    canvas.paste(scaled, ((col_w - scaled.width) // 2, 34))
    return canvas


def triptych(original: Image.Image, md: Image.Image, latex: Image.Image, page: int) -> Image.Image:
    col_w = 760
    col_h = max(original.height, md.height, latex.height)
    col_h = min(col_h, 2400)
    cols = [
        column(original, f"ORIGINAL  p.{page}", col_w, col_h),
        column(md, "MARKDOWN → rendered", col_w, col_h),
        column(latex, "LATEX → rendered", col_w, col_h),
    ]
    gap = 12
    out = Image.new("RGB", (col_w * 3 + gap * 2, col_h + 34), "#d1d5db")
    for i, c in enumerate(cols):
        out.paste(c, (i * (col_w + gap), 0))
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("document")
    ap.add_argument("--cli", default=str(REPO / "target" / "release" / "dongler"))
    ap.add_argument("--out-dir", default=str(REPO / "eval" / "out" / "visual-comparisons"))
    ap.add_argument("--pages", default="", help="comma-separated explicit page numbers")
    ap.add_argument("--table-pages", type=int, default=4, help="auto-pick N densest table pages")
    ap.add_argument("--dpi", type=int, default=110)
    args = ap.parse_args()

    pdf = Path(args.document)
    cli = Path(args.cli)
    out_dir = Path(args.out_dir) / pdf.stem
    work = out_dir / "_work"
    work.mkdir(parents=True, exist_ok=True)

    total = page_count(pdf)
    if args.pages.strip():
        chosen = [(int(p), -1) for p in args.pages.split(",") if p.strip()]
    else:
        chosen = pick_pages(pdf, args.table_pages, total)

    summary = {"document": str(pdf), "pages_total": total, "pages": []}
    for page, score in chosen:
        md = extract(cli, pdf, "markdown", page)
        latex = extract(cli, pdf, "latex", page)
        (out_dir / f"page-{page}.md").write_text(md)
        (out_dir / f"page-{page}.tex").write_text(latex)

        original_img = render_page_image(pdf, page, work / f"orig-{page}", args.dpi) or error_image("no original render")
        md_img = compile_latex_to_image(markdown_to_latex_document(md), work, f"md-{page}", args.dpi)
        latex_img = compile_latex_to_image(latex, work, f"tex-{page}", args.dpi)

        out_png = out_dir / f"page-{page}.triptych.png"
        triptych(original_img, md_img, latex_img, page).save(out_png)
        summary["pages"].append({"page": page, "table_score": score, "triptych": str(out_png)})
        print(f"  page {page} (table_score={score}) -> {out_png}")

    (out_dir / "summary.json").write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
