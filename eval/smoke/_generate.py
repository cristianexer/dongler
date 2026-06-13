"""Generate the committed smoke-slice PDFs with a pure-Python PDF writer.

No third-party libraries: we hand-build a minimal PDF-1.4 (Catalog / Pages /
Page / Helvetica font / Contents) with a MediaBox of [0 0 612 792], modelled on
``python/tests/test_dongler.py``'s ``minimal_pdf`` but extended to draw multiple
lines (one ``Td`` to set the start, then ``T*`` per subsequent line using the
text-leading ``TL`` operator).

Run from the repo root::

    python eval/smoke/_generate.py

This (re)writes every ``<name>.pdf`` next to its committed ``<name>.gt.md``.
The PDFs are tiny and license-clean (we authored them).
"""
from __future__ import annotations

from pathlib import Path
from typing import List

SMOKE_DIR = Path(__file__).resolve().parent


def _escape(text: str) -> str:
    """Escape characters special to a PDF literal string ``(...)``."""
    return text.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")


def minimal_pdf(
    lines: List[str],
    *,
    leading: int = 18,
    x: int = 72,
    y: int = 720,
    size: int = 12,
) -> bytes:
    """Build a single-page PDF drawing ``lines``, each on its own text line.

    The content stream opens a text object (``BT``), selects Helvetica at
    ``size`` pt, sets the leading (``TL``), positions the first line with ``Td``,
    then emits ``(line) Tj`` for the first line and ``T* (line) Tj`` for each
    subsequent line (``T*`` advances down by the leading). Page geometry is the
    standard US-Letter MediaBox ``[0 0 612 792]``.
    """
    ops: List[str] = ["BT", f"/F1 {size} Tf", f"{leading} TL", f"{x} {y} Td"]
    for index, line in enumerate(lines):
        if index > 0:
            ops.append("T*")
        ops.append(f"({_escape(line)}) Tj")
    ops.append("ET")
    content = "\n".join(ops)
    content_bytes = content.encode("latin-1")

    return (
        b"%PDF-1.4\n"
        b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n"
        b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n"
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n"
        b"4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n"
        + (f"5 0 obj\n<< /Length {len(content_bytes)} >>\nstream\n").encode("latin-1")
        + content_bytes
        + b"\nendstream\nendobj\n"
        + b"trailer\n<< /Root 1 0 R >>\n%%EOF\n"
    )


# Each case: (name, list-of-lines-to-draw, leading).
# Leading controls whether dongler groups lines into one block (tight) or
# treats each as its own paragraph (loose). Cases are deliberately tiny so the
# paragraph cases score high edit_similarity, proving the harness works.
CASES = {
    # (a) single paragraph
    "a_single_paragraph": (["The quick brown fox jumps over the lazy dog."], 18),
    # (b) H1 heading + paragraph
    "b_heading_paragraph": (["Introduction", "This document explains the basics."], 24),
    # (c) bulleted list
    "c_bulleted_list": (["- Apples", "- Bread", "- Milk"], 18),
    # (d) two stacked paragraphs
    "d_two_paragraphs": (
        ["First paragraph of the document.", "Second paragraph of the document."],
        24,
    ),
    # (e) heading hierarchy (H1 + H2)
    "e_heading_hierarchy": (
        ["Chapter One", "Section A", "Content under section A."],
        24,
    ),
    # (f) a short multi-line block (tight leading -> one visual block)
    "f_multiline_block": (
        ["Roses are red", "Violets are blue", "Sugar is sweet"],
        14,
    ),
}


def main() -> None:
    for name, (lines, leading) in CASES.items():
        pdf_bytes = minimal_pdf(lines, leading=leading)
        out_path = SMOKE_DIR / f"{name}.pdf"
        out_path.write_bytes(pdf_bytes)
        print(f"wrote {out_path.name} ({len(pdf_bytes)} bytes)")


if __name__ == "__main__":
    main()
