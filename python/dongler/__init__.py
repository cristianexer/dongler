from __future__ import annotations

import json
import os
from typing import Any, Union

from . import _dongler
from ._dongler import __version__, detect_format, to_json, to_latex, to_markdown


class DonglerDocument:
    def __init__(self, data: dict[str, Any]):
        self._data = data
        self._json = json.dumps(data)

    @property
    def metadata(self) -> dict[str, Any]:
        return self._data["metadata"]

    @property
    def pages(self) -> list[dict[str, Any]]:
        return self._data["pages"]

    def to_dict(self) -> dict[str, Any]:
        return json.loads(self._json)

    def to_markdown(self) -> str:
        return _dongler.document_to_markdown(self._json)

    def to_json(self) -> str:
        return _dongler.document_to_json(self._json)

    def to_latex(self) -> str:
        return _dongler.document_to_latex(self._json)


def parse_text(text: str) -> dict[str, Any]:
    return json.loads(_dongler.to_json(text))


PathLike = Union[str, os.PathLike[str]]


def load(path: PathLike, options: dict[str, Any] | None = None) -> DonglerDocument:
    path_string = os.fspath(path)
    options_json = None if options is None else json.dumps(options)
    return DonglerDocument(
        json.loads(_dongler.load_path_json_with_options(path_string, options_json))
    )


def load_many(paths: list[PathLike]) -> list[dict[str, Any]]:
    path_strings = [os.fspath(path) for path in paths]
    results = json.loads(_dongler.load_many_json(path_strings))

    for result in results:
        if result["document"] is not None:
            result["document"] = DonglerDocument(result["document"])

    return results


__all__ = [
    "DonglerDocument",
    "__version__",
    "detect_format",
    "load",
    "load_many",
    "parse_text",
    "to_json",
    "to_latex",
    "to_markdown",
]
