"""Adapter for directories of ``<name>.pdf`` + ``<name>.gt.md`` pairs.

Layout::

    <dir>/
      doc1.pdf
      doc1.gt.md
      doc1.meta.json        # optional: {"has_table": true, "route": "born_digital", ...}
      doc2.pdf
      doc2.gt.md

The optional ``<name>.meta.json`` carries tags consumed by the harness, e.g.
``has_table`` (gate TEDS computation) and ``gt_table_html`` (the GT table HTML
to score against). Any keys are passed through verbatim in ``sample["meta"]``.
"""
from __future__ import annotations

import json
from pathlib import Path
from typing import Iterator

from .base import Adapter, Sample


class MarkdownGTAdapter(Adapter):
    """Yield one :class:`Sample` per ``<name>.pdf`` that has a ``<name>.gt.md``."""

    name = "markdown_gt"

    def __init__(self, root: str | Path) -> None:
        self.root = Path(root)
        if not self.root.is_dir():
            raise NotADirectoryError(f"adapter root is not a directory: {self.root}")

    def __iter__(self) -> Iterator[Sample]:
        for pdf_path in sorted(self.root.glob("*.pdf")):
            stem = pdf_path.stem
            gt_path = self.root / f"{stem}.gt.md"
            if not gt_path.exists():
                # No ground truth -> not scorable; skip silently.
                continue
            gt_markdown = gt_path.read_text(encoding="utf-8")

            meta = {}
            meta_path = self.root / f"{stem}.meta.json"
            if meta_path.exists():
                meta = json.loads(meta_path.read_text(encoding="utf-8"))

            yield Sample(
                id=stem,
                pdf_path=str(pdf_path.resolve()),
                gt_markdown=gt_markdown,
                meta=meta,
            )
