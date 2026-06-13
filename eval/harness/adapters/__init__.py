"""Dataset adapters for the eval harness.

An adapter turns an on-disk dataset into a uniform stream of samples. Each
sample is a dict ``{id, pdf_path, gt_markdown, meta}`` so the harness
(``run.py``) is dataset-agnostic.
"""
from __future__ import annotations

from .base import Adapter, Sample
from .markdown_gt import MarkdownGTAdapter

__all__ = ["Adapter", "Sample", "MarkdownGTAdapter"]
