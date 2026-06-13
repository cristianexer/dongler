"""Adapter abstraction shared by all dataset adapters."""
from __future__ import annotations

from typing import Any, Dict, Iterator, TypedDict


class Sample(TypedDict):
    """One scorable document.

    Keys
    ----
    id:
        Stable identifier (used as the per-doc row key in reports).
    pdf_path:
        Absolute path to the input PDF fed to the extractor.
    gt_markdown:
        Ground-truth markdown the extractor output is scored against.
    meta:
        Free-form tags (e.g. ``has_table``, ``route``, ``gt_table_html``).
    """

    id: str
    pdf_path: str
    gt_markdown: str
    meta: Dict[str, Any]


class Adapter:
    """Base class: an iterable of :class:`Sample` dicts."""

    name: str = "adapter"

    def __iter__(self) -> Iterator[Sample]:  # pragma: no cover - interface
        raise NotImplementedError

    def __len__(self) -> int:  # pragma: no cover - convenience
        return sum(1 for _ in self)
