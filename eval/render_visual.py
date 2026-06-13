#!/usr/bin/env python3
"""Visual fidelity check: render `dongler convert` Markdown to an image and place
it beside the original PDF page.

  [ ORIGINAL pdf page ]  |  [ Markdown rendered to HTML via headless Chrome ]

Markdown -> HTML uses python-markdown (tables + md_in_html) so GFM pipe tables
AND the pipeline's embedded raw <table> spans both render the way a real Markdown
viewer (GitHub, VS Code) would. Run:

  uv run --with markdown python eval/render_visual.py <pdf> [<pdf> ...]

If no PDFs are given, it renders the saved sec-10k split pages under
eval/out/examples/_tmp/.
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import markdown  # provided via `uv run --with markdown`
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
CLI = ROOT / "target" / "debug" / "dongler"
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
OUT = ROOT / "eval" / "out" / "examples" / "render"
DPI = 130

CSS = """
body { font-family: -apple-system, Helvetica, Arial, sans-serif; font-size: 13px;
       margin: 24px; color: #111; line-height: 1.4; width: 900px; }
table { border-collapse: collapse; margin: 10px 0; }
th, td { border: 1px solid #bbb; padding: 3px 7px; text-align: left;
         vertical-align: top; font-size: 12px; }
th { background: #f3f4f6; }
h1,h2,h3 { margin: 10px 0 4px; }
"""


def run(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def convert_md(pdf: Path) -> str:
    return run([str(CLI), "convert", str(pdf), "--format", "markdown"]).stdout


def md_to_png(md: str, stem: str) -> Image.Image:
    html_body = markdown.markdown(md, extensions=["tables", "md_in_html"])
    html = f"<!doctype html><html><head><meta charset='utf-8'><style>{CSS}</style>" \
           f"</head><body>{html_body}</body></html>"
    hp = OUT / f"{stem}.render.html"
    pp = OUT / f"{stem}.render.png"
    hp.write_text(html)
    run([CHROME, "--headless", "--disable-gpu", "--hide-scrollbars",
         f"--screenshot={pp}", "--window-size=960,1400",
         "--default-background-color=FFFFFFFF", f"file://{hp}"])
    return Image.open(pp).convert("RGB")


def pdf_to_png(pdf: Path, stem: str) -> Image.Image:
    prefix = OUT / f"{stem}.source"
    run(["pdftoppm", "-r", str(DPI), "-png", str(pdf), str(prefix)])
    pages = sorted(OUT.glob(f"{stem}.source-*.png")) or sorted(OUT.glob(f"{stem}.source.png"))
    imgs = [Image.open(p).convert("RGB") for p in pages]
    if not imgs:
        return Image.new("RGB", (700, 400), "#fee")
    w = max(i.width for i in imgs)
    h = sum(i.height for i in imgs) + 8 * (len(imgs) - 1)
    canvas = Image.new("RGB", (w, h), "white")
    y = 0
    for im in imgs:
        canvas.paste(im, (0, y)); y += im.height + 8
    return canvas


def label(text: str, width: int, h: int = 30) -> Image.Image:
    img = Image.new("RGB", (width, h), "#1f2937")
    d = ImageDraw.Draw(img)
    try:
        f = ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial Bold.ttf", 18)
    except OSError:
        f = ImageFont.load_default()
    d.text((10, 5), text, fill="white", font=f)
    return img


def triptych(pdf: Path) -> Path:
    stem = pdf.stem
    src = pdf_to_png(pdf, stem)
    ren = md_to_png(convert_md(pdf), stem)
    colw = 920
    def fit(im: Image.Image) -> Image.Image:
        scale = colw / im.width
        return im.resize((colw, int(im.height * scale)))
    src_f, ren_f = fit(src), fit(ren)
    H = max(src_f.height, ren_f.height) + 30
    canvas = Image.new("RGB", (colw * 2 + 12, H), "white")
    canvas.paste(label("ORIGINAL PAGE", colw), (0, 0))
    canvas.paste(label("dongler convert -> Markdown -> HTML", colw), (colw + 12, 0))
    canvas.paste(src_f, (0, 30))
    canvas.paste(ren_f, (colw + 12, 30))
    out = OUT / f"{stem}.compare.png"
    canvas.save(out)
    print(f"wrote {out}")
    return out


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    args = sys.argv[1:]
    if args:
        pdfs = [Path(a) for a in args]
    else:
        pdfs = sorted((ROOT / "eval/out/examples/_tmp").glob("*.pdf"))
    for pdf in pdfs:
        triptych(pdf)


if __name__ == "__main__":
    main()
