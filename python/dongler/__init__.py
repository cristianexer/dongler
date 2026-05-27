from __future__ import annotations

import json
from typing import Any

from . import _dongler
from ._dongler import __version__, detect_format, to_json, to_latex, to_markdown


def parse_text(text: str) -> dict[str, Any]:
    return json.loads(_dongler.to_json(text))


__all__ = [
    "__version__",
    "detect_format",
    "parse_text",
    "to_json",
    "to_latex",
    "to_markdown",
]
