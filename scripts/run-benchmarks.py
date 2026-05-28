#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import dataclasses
import gzip
import hashlib
import html
import json
import os
import re
import subprocess
import time
from collections import Counter
from pathlib import Path
from typing import Any


START_MARKER = "<!-- BENCHMARKS:START -->"
END_MARKER = "<!-- BENCHMARKS:END -->"
IMAGE_SUFFIXES = {".png", ".jpg", ".jpeg", ".tif", ".tiff", ".gif", ".bmp", ".webp"}
DOCUMENT_SUFFIXES = {
    ".txt",
    ".text",
    ".md",
    ".markdown",
    ".html",
    ".htm",
    ".eml",
    ".docx",
    ".xlsx",
    ".pptx",
    ".odt",
    ".ods",
    ".odp",
    ".tar",
    ".tgz",
    ".gz",
    ".zip",
    ".json",
    ".jsonl",
    ".ndjson",
    ".csv",
    ".tsv",
    ".xml",
    ".nxml",
    ".tei",
    ".tex",
    ".latex",
    ".ltx",
}
STRUCTURED_DOCUMENT_SUFFIXES = {
    ".json",
    ".jsonl",
    ".ndjson",
    ".csv",
    ".tsv",
    ".xml",
    ".nxml",
    ".tei",
}
REFERENCE_SUFFIXES = {
    ".txt",
    ".text",
    ".md",
    ".markdown",
    ".html",
    ".htm",
    ".json",
    ".jsonl",
    ".ndjson",
    ".csv",
    ".tsv",
    ".xml",
    ".nxml",
    ".tei",
    ".tex",
    ".latex",
    ".ltx",
}


@dataclasses.dataclass
class DocumentSummary:
    pages: int = 0
    blocks: int = 0
    bbox_blocks: int = 0
    anchored_blocks: int = 0
    warnings: int = 0

    @property
    def bbox_block_rate(self) -> float:
        return self.bbox_blocks / self.blocks if self.blocks else 0.0

    @property
    def anchored_block_rate(self) -> float:
        return self.anchored_blocks / self.blocks if self.blocks else 0.0


@dataclasses.dataclass
class ReferenceTarget:
    text: str
    page_numbers: list[int] = dataclasses.field(default_factory=list)


def summarize_document_json(path: Path) -> DocumentSummary:
    doc = json.loads(path.read_text())
    pages = doc.get("pages", [])
    blocks = 0
    bbox_blocks = 0
    anchored_blocks = 0
    warnings = len(doc.get("warnings", []))
    for page in pages:
        page_blocks = page.get("blocks", [])
        blocks += len(page_blocks)
        warnings += len(page.get("warnings", []))
        for block in page_blocks:
            if block.get("bbox"):
                bbox_blocks += 1
            if block.get("source_anchors"):
                anchored_blocks += 1
    return DocumentSummary(
        pages=len(pages),
        blocks=blocks,
        bbox_blocks=bbox_blocks,
        anchored_blocks=anchored_blocks,
        warnings=warnings,
    )


def extracted_text_from_document_json(path: Path, page_number: int | None = None) -> str:
    doc = json.loads(path.read_text())
    parts: list[str] = []
    for page in doc.get("pages", []):
        if page_number is not None and page.get("number") != page_number:
            continue
        for block in page.get("blocks", []):
            block_type = block.get("type")
            if block_type == "text":
                parts.append(str(block.get("text", "")))
            elif block_type == "table":
                rows = []
                headers = block.get("headers") or []
                if headers:
                    rows.append(" ".join(str(cell) for cell in headers))
                rows.extend(
                    " ".join(str(cell) for cell in row)
                    for row in block.get("rows", [])
                )
                parts.append("\n".join(row for row in rows if row.strip()))
            elif block_type == "figure":
                caption = block.get("caption") or block.get("alt_text") or ""
                parts.append(str(caption))
    return "\n\n".join(part for part in parts if part.strip())


def reference_text_for_document(document: Path, local_dir: Path) -> str | None:
    target = reference_target_for_document(document, local_dir)
    return target.text if target else None


def reference_target_for_document(document: Path, local_dir: Path) -> ReferenceTarget | None:
    direct = reference_text_from_path(document)
    if direct:
        return ReferenceTarget(direct)

    for suffix in REFERENCE_SUFFIXES:
        candidate = document.with_suffix(suffix)
        if candidate != document and candidate.exists():
            text = reference_text_from_path(candidate)
            if text:
                return ReferenceTarget(text, page_numbers_from_related_reference(document, candidate))

    stem = document.stem
    for candidate in sorted(local_dir.rglob(f"{stem}.*")):
        if candidate == document or not candidate.is_file():
            continue
        if document_logical_suffix(candidate) not in REFERENCE_SUFFIXES:
            continue
        text = reference_text_from_path(candidate)
        if text:
            return ReferenceTarget(text, page_numbers_from_related_reference(document, candidate))

    for candidate in related_page_reference_candidates(document, local_dir):
        text = reference_text_from_path(candidate)
        if text:
            return ReferenceTarget(text, page_numbers_from_related_reference(document, candidate))
    return None


def reference_text_from_path(path: Path) -> str | None:
    suffix = document_logical_suffix(path)
    if suffix not in REFERENCE_SUFFIXES:
        return None
    try:
        text = read_reference_text(path)
    except (OSError, UnicodeDecodeError, gzip.BadGzipFile):
        return None

    if suffix in {".txt", ".text", ".md", ".markdown"}:
        docbank_text = docbank_token_label_reference_text(text)
        if docbank_text:
            return docbank_text
        return text.strip() or None
    if suffix in {".tex", ".latex", ".ltx"}:
        return latex_reference_text(text)
    if suffix in {".html", ".htm", ".xml", ".nxml", ".tei"}:
        return tagged_reference_text(text)
    if suffix in {".json", ".jsonl", ".ndjson"}:
        return json_reference_text(text)
    if suffix in {".csv", ".tsv"}:
        delimiter = "\t" if suffix == ".tsv" else ","
        structured_text = structured_delimited_reference_text(text, delimiter)
        if structured_text:
            return structured_text
        return delimited_reference_text(text, delimiter)
    return None


def related_page_reference_candidates(document: Path, local_dir: Path) -> list[Path]:
    stem = document.stem
    prefixes = []
    for marker in ["_black", "_color"]:
        if marker in stem:
            prefixes.append(stem.split(marker)[0])
    candidates = []
    for prefix in prefixes:
        candidates.extend(sorted(local_dir.rglob(f"{prefix}_*.txt")))
    return [
        candidate
        for candidate in candidates
        if candidate.is_file()
        and candidate != document
        and document_logical_suffix(candidate) in REFERENCE_SUFFIXES
    ]


def page_numbers_from_related_reference(document: Path, reference: Path) -> list[int]:
    document_stem = document.stem
    reference_stem = reference.stem
    prefixes = [document_stem]
    for marker in ["_black", "_color"]:
        if marker in document_stem:
            prefixes.append(document_stem.split(marker)[0])
    for prefix in prefixes:
        if not reference_stem.startswith(prefix + "_"):
            continue
        suffix = reference_stem[len(prefix) + 1 :]
        if suffix.isdigit():
            page_index = int(suffix)
            candidates = [page_index]
            if page_index >= 0:
                candidates.append(page_index + 1)
            return sorted({page for page in candidates if page > 0})
    return []


def read_reference_text(path: Path) -> str:
    if path.suffix.lower() == ".gz":
        with gzip.open(path, "rt", encoding="utf-8", errors="replace") as handle:
            return handle.read()
    return path.read_text(encoding="utf-8", errors="replace")


def tagged_reference_text(text: str) -> str | None:
    text = re.sub(r"(?is)<script.*?</script>|<style.*?</style>", " ", text)
    text = re.sub(r"(?s)<[^>]+>", " ", text)
    text = html.unescape(text)
    return normalize_reference_whitespace(text) or None


def docbank_token_label_reference_text(text: str) -> str | None:
    tokens = []
    non_empty_lines = 0
    docbank_lines = 0
    for line in text.splitlines():
        if not line.strip():
            continue
        non_empty_lines += 1
        cells = line.split("\t")
        if len(cells) < 10:
            continue
        if not all(is_number(cell) for cell in cells[1:5]):
            continue
        token = cells[0].strip()
        if not token:
            continue
        docbank_lines += 1
        tokens.append(token)
    if non_empty_lines == 0 or docbank_lines / non_empty_lines < 0.8:
        return None
    return normalize_reference_whitespace(" ".join(tokens)) or None


def latex_reference_text(text: str) -> str | None:
    text = re.sub(r"(?<!\\)%.*", " ", text)
    text = re.sub(
        r"\\(?:part|chapter|section|subsection|subsubsection|paragraph|subparagraph)\*?(?:\[[^\]]*\])?\{([^{}]*)\}",
        r" \1 ",
        text,
    )
    text = re.sub(r"\\(?:title|caption|item)(?:\[[^\]]*\])?\{?([^{}\n]*)\}?", r" \1 ", text)
    text = re.sub(r"\\[a-zA-Z]+\*?(?:\[[^\]]*\])?", " ", text)
    text = re.sub(r"\\([%&_#$])", r"\1", text)
    text = re.sub(r"[{}$]", " ", text)
    return normalize_reference_whitespace(text) or None


def json_reference_text(text: str) -> str | None:
    parts: list[str] = []
    try:
        collect_json_reference_text(json.loads(text), parts)
    except json.JSONDecodeError:
        for line in text.splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                collect_json_reference_text(json.loads(line), parts)
            except json.JSONDecodeError:
                continue
    return normalize_reference_whitespace("\n\n".join(parts)) or None


def collect_json_reference_text(value: Any, parts: list[str]) -> None:
    if isinstance(value, dict):
        if isinstance(value.get("form"), list):
            for item in value["form"]:
                if isinstance(item, dict):
                    text = normalize_reference_whitespace(str(item.get("text", "")))
                    if text:
                        parts.append(text)
            return
        preferred = [
            "title",
            "abstract",
            "body_text",
            "text",
            "content",
            "caption",
            "html",
            "latex",
            "tokens",
            "words",
            "cells",
        ]
        seen = set()
        before = len(parts)
        for key in preferred:
            if key in value:
                seen.add(key)
                collect_json_reference_text(value[key], parts)
        if len(parts) != before:
            return
        for key, child in value.items():
            if key not in seen:
                collect_json_reference_text(child, parts)
    elif isinstance(value, list):
        for item in value:
            collect_json_reference_text(item, parts)
    elif isinstance(value, str):
        text = normalize_reference_whitespace(value)
        if text:
            parts.append(text)


def delimited_reference_text(text: str, delimiter: str) -> str | None:
    rows = csv.reader(text.splitlines(), delimiter=delimiter)
    parts = []
    for row in rows:
        cleaned = " ".join(cell.strip() for cell in row if cell.strip())
        if cleaned:
            parts.append(cleaned)
    return normalize_reference_whitespace("\n".join(parts)) or None


def structured_delimited_reference_text(text: str, delimiter: str) -> str | None:
    rows = [
        [cell.strip() for cell in row]
        for row in csv.reader(text.splitlines(), delimiter=delimiter)
        if any(cell.strip() for cell in row)
    ]
    if not rows:
        return None

    ocr_rows = []
    for row in rows:
        if len(row) < 9 or not all(is_number(cell) for cell in row[:8]):
            ocr_rows = []
            break
        ocr_text = " ".join(cell for cell in row[8:] if cell)
        if ocr_text:
            ocr_rows.append(ocr_text)
    if ocr_rows:
        return normalize_reference_whitespace("\n".join(ocr_rows)) or None

    headers = [header.strip().lower() for header in rows[0]]
    if "text" in headers:
        text_index = headers.index("text")
        parts = []
        for row in rows[1:]:
            if len(row) > text_index:
                text = " ".join(cell for cell in row[text_index:] if cell)
                if text:
                    parts.append(text)
        if parts:
            return normalize_reference_whitespace("\n".join(parts)) or None
    return None


def is_number(text: str) -> bool:
    try:
        float(text)
        return True
    except ValueError:
        return False


def normalize_reference_whitespace(text: str) -> str:
    return " ".join(text.split())


def extraction_accuracy(reference_text: str, extracted_text: str) -> float | None:
    reference_tokens = accuracy_tokens(reference_text)
    if not reference_tokens:
        return None
    extracted_tokens = accuracy_tokens(extracted_text)
    if not extracted_tokens:
        return 0.0
    reference_counts = Counter(reference_tokens)
    extracted_counts = Counter(extracted_tokens)
    overlap = sum((reference_counts & extracted_counts).values())
    if overlap == 0:
        return 0.0
    precision = overlap / len(extracted_tokens)
    recall = overlap / len(reference_tokens)
    return 2 * precision * recall / (precision + recall)


def accuracy_tokens(text: str) -> list[str]:
    text = normalize_accuracy_ligatures(text)
    return re.findall(r"[a-z0-9]+", text.lower())


def normalize_accuracy_ligatures(text: str) -> str:
    return str(text).replace("\u0002", "fi").replace("\u0003", "fl")


def best_extraction_accuracy(
    reference: ReferenceTarget,
    json_out: Path,
) -> float | None:
    if reference.page_numbers:
        scored = [
            extraction_accuracy(reference.text, extracted_text_from_document_json(json_out, page_number))
            for page_number in reference.page_numbers
        ]
        scored = [score for score in scored if score is not None]
        if scored:
            return max(scored)
    return extraction_accuracy(reference.text, extracted_text_from_document_json(json_out))


def olmocr_document_key(document: Path, local_dir: Path) -> str:
    pdf_root = local_dir / "bench_data" / "pdfs"
    try:
        return document.relative_to(pdf_root).as_posix()
    except ValueError:
        return document.name


def olmocr_tests_for_document(document: Path, local_dir: Path) -> list[dict[str, Any]]:
    index = index_olmocr_tests(local_dir)
    key = olmocr_document_key(document, local_dir)
    tests = index.get(key, [])
    if tests:
        return tests
    return index.get(document.name, [])


def index_olmocr_tests(local_dir: Path) -> dict[str, list[dict[str, Any]]]:
    bench_dir = local_dir / "bench_data"
    if not bench_dir.exists():
        return {}
    tests_by_pdf: dict[str, list[dict[str, Any]]] = {}
    for path in sorted(bench_dir.glob("*.jsonl")):
        try:
            content = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for test in iter_json_objects(content):
            if not isinstance(test, dict):
                continue
            if olmocr_test_is_disabled(test):
                continue
            pdf = str(
                test.get("pdf")
                or test.get("pdf_path")
                or test.get("document")
                or ""
            ).strip()
            if not pdf:
                continue
            normalized_pdf = pdf.replace("\\", "/").lstrip("/")
            tests_by_pdf.setdefault(normalized_pdf, []).append(test)
            tests_by_pdf.setdefault(Path(normalized_pdf).name, []).append(test)
    return tests_by_pdf


def iter_json_objects(content: str) -> list[Any]:
    decoder = json.JSONDecoder()
    objects = []
    index = 0
    while index < len(content):
        while index < len(content) and content[index].isspace():
            index += 1
        if index >= len(content):
            break
        try:
            value, index = decoder.raw_decode(content, index)
        except json.JSONDecodeError:
            next_line = content.find("\n", index)
            if next_line == -1:
                break
            index = next_line + 1
            continue
        objects.append(value)
    return objects


def olmocr_test_is_disabled(test: dict[str, Any]) -> bool:
    checked = test.get("checked")
    if not isinstance(checked, str):
        return False
    return checked.strip().lower() in {"disabled", "rejected", "skip", "skipped"}


def olmocr_unit_pass_rate(tests: list[dict[str, Any]], json_out: Path) -> float | None:
    if not tests:
        return None
    try:
        document = json.loads(json_out.read_text())
    except (OSError, json.JSONDecodeError):
        return None
    extracted_text = extracted_text_from_document(document)
    table_grids = table_grids_from_document(document)
    passed = 0
    scored = 0
    for test in tests:
        result = evaluate_olmocr_unit_test(test, extracted_text, table_grids)
        if result is None:
            continue
        scored += 1
        if result:
            passed += 1
    if scored == 0:
        return None
    return passed / scored


def extracted_text_from_document(document: dict[str, Any]) -> str:
    parts: list[str] = []
    for page in document.get("pages", []):
        for block in page.get("blocks", []):
            block_type = block.get("type")
            if block_type == "text":
                parts.append(str(block.get("text", "")))
            elif block_type == "table":
                rows = table_grid_from_block(block)
                parts.extend(" ".join(cell for cell in row if cell.strip()) for row in rows)
            elif block_type == "figure":
                caption = block.get("caption") or block.get("alt_text") or ""
                parts.append(str(caption))
    return "\n\n".join(part for part in parts if part.strip())


def table_grids_from_document(document: dict[str, Any]) -> list[list[list[str]]]:
    grids = []
    for page in document.get("pages", []):
        for block in page.get("blocks", []):
            if block.get("type") == "table":
                grid = table_grid_from_block(block)
                if grid:
                    grids.append(grid)
    return grids


def table_grid_from_block(block: dict[str, Any]) -> list[list[str]]:
    rows: list[list[str]] = []
    headers = block.get("headers")
    if isinstance(headers, list) and headers:
        rows.append([str(cell) for cell in headers])
    for row in block.get("rows") or []:
        if isinstance(row, list):
            rows.append([str(cell) for cell in row])

    cells = block.get("cells")
    if isinstance(cells, list) and cells:
        max_row = max(
            (int(cell.get("row", -1)) for cell in cells if isinstance(cell, dict)),
            default=-1,
        )
        max_column = max(
            (int(cell.get("column", -1)) for cell in cells if isinstance(cell, dict)),
            default=-1,
        )
        if max_row >= 0 and max_column >= 0:
            cell_rows = [["" for _ in range(max_column + 1)] for _ in range(max_row + 1)]
            for cell in cells:
                if not isinstance(cell, dict):
                    continue
                row = int(cell.get("row", -1))
                column = int(cell.get("column", -1))
                if row >= 0 and column >= 0:
                    cell_rows[row][column] = str(cell.get("text", ""))
            if len(cell_rows) > len(rows):
                rows = cell_rows
    return rows


def evaluate_olmocr_unit_test(
    test: dict[str, Any],
    extracted_text: str,
    table_grids: list[list[list[str]]],
) -> bool | None:
    test_type = str(test.get("type", "")).strip().lower()
    if test_type == "present":
        text = olmocr_expected_text(test, ["text", "substring", "value", "expected"])
        return olmocr_contains(extracted_text, text, test) if text else None
    if test_type == "absent":
        text = olmocr_expected_text(test, ["text", "substring", "value", "expected"])
        return (not olmocr_contains(extracted_text, text, test)) if text else None
    if test_type == "order":
        before = olmocr_expected_text(test, ["before", "first"])
        after = olmocr_expected_text(test, ["after", "second"])
        if not before or not after:
            return None
        before_index = olmocr_find(extracted_text, before, test)
        after_index = olmocr_find(extracted_text, after, test)
        return before_index >= 0 and after_index >= 0 and before_index < after_index
    if test_type == "math":
        text = olmocr_expected_text(test, ["math", "text", "formula", "expression", "expected"])
        if not text:
            return None
        return normalize_math_text(text) in normalize_math_text(extracted_text)
    if test_type == "table":
        return evaluate_olmocr_table_test(test, extracted_text, table_grids)
    return None


def evaluate_olmocr_table_test(
    test: dict[str, Any],
    extracted_text: str,
    table_grids: list[list[list[str]]],
) -> bool | None:
    cell = olmocr_expected_text(test, ["cell", "text", "value", "expected"])
    if not cell:
        return None
    for grid in table_grids:
        if table_grid_satisfies_olmocr_test(grid, test, cell):
            return True
    expected_values = [cell]
    for key in ["up", "down", "left", "right", "top_heading", "left_heading"]:
        value = olmocr_expected_text(test, [key])
        if value:
            expected_values.append(value)
    return all(olmocr_contains(extracted_text, value, test) for value in expected_values)


def table_grid_satisfies_olmocr_test(
    grid: list[list[str]],
    test: dict[str, Any],
    cell: str,
) -> bool:
    for row_index, row in enumerate(grid):
        for column_index, value in enumerate(row):
            if not olmocr_cell_matches(value, cell, test):
                continue
            if not check_adjacent_cell(grid, row_index, column_index, -1, 0, test, "up"):
                continue
            if not check_adjacent_cell(grid, row_index, column_index, 1, 0, test, "down"):
                continue
            if not check_adjacent_cell(grid, row_index, column_index, 0, -1, test, "left"):
                continue
            if not check_adjacent_cell(grid, row_index, column_index, 0, 1, test, "right"):
                continue
            if not check_heading_cells(grid, row_index, column_index, test, "top_heading", "column"):
                continue
            if not check_heading_cells(grid, row_index, column_index, test, "left_heading", "row"):
                continue
            return True
    return False


def check_adjacent_cell(
    grid: list[list[str]],
    row: int,
    column: int,
    row_delta: int,
    column_delta: int,
    test: dict[str, Any],
    key: str,
) -> bool:
    expected = olmocr_expected_text(test, [key])
    if not expected:
        return True
    adjacent_row = row + row_delta
    adjacent_column = column + column_delta
    if adjacent_row < 0 or adjacent_row >= len(grid):
        return False
    if adjacent_column < 0 or adjacent_column >= len(grid[adjacent_row]):
        return False
    return olmocr_cell_matches(grid[adjacent_row][adjacent_column], expected, test)


def check_heading_cells(
    grid: list[list[str]],
    row: int,
    column: int,
    test: dict[str, Any],
    key: str,
    direction: str,
) -> bool:
    expected = olmocr_expected_text(test, [key])
    if not expected:
        return True
    if direction == "column":
        candidates = [grid[index][column] for index in range(row) if column < len(grid[index])]
    else:
        candidates = grid[row][:column]
    return any(olmocr_cell_matches(candidate, expected, test) for candidate in candidates)


def olmocr_expected_text(test: dict[str, Any], keys: list[str]) -> str | None:
    for key in keys:
        value = test.get(key)
        if value is None:
            continue
        text = str(value).strip()
        if text:
            return text
    return None


def olmocr_contains(haystack: str, needle: str, test: dict[str, Any]) -> bool:
    return olmocr_find(haystack, needle, test) >= 0


def olmocr_find(haystack: str, needle: str, test: dict[str, Any]) -> int:
    if not needle:
        return -1
    normalized_haystack = normalize_olmocr_text(olmocr_window_text(haystack, test), test)
    normalized_needle = normalize_olmocr_text(needle, test)
    exact_index = normalized_haystack.find(normalized_needle)
    if exact_index >= 0:
        return exact_index
    max_diffs = int(test.get("max_diffs") or 0)
    if max_diffs <= 0:
        return -1
    return fuzzy_token_find(normalized_haystack, normalized_needle, max_diffs)


def olmocr_window_text(text: str, test: dict[str, Any]) -> str:
    first_n = numeric_value(test.get("first_n"))
    last_n = numeric_value(test.get("last_n"))
    if first_n is not None and first_n > 0:
        return str(text)[: int(first_n)]
    if last_n is not None and last_n > 0:
        return str(text)[-int(last_n) :]
    return str(text)


def fuzzy_token_find(haystack: str, needle: str, max_diffs: int) -> int:
    haystack_tokens = haystack.split()
    needle_tokens = needle.split()
    if not haystack_tokens or not needle_tokens:
        return -1
    token_offsets = token_start_offsets(haystack, haystack_tokens)
    target_len = len(needle_tokens)
    min_len = max(1, target_len - 2)
    max_len = min(len(haystack_tokens), target_len + 2)
    for start in range(len(haystack_tokens)):
        for length in range(min_len, max_len + 1):
            end = start + length
            if end > len(haystack_tokens):
                continue
            candidate = " ".join(haystack_tokens[start:end])
            if bounded_levenshtein(candidate, needle, max_diffs) <= max_diffs:
                return token_offsets[start]
    return -1


def token_start_offsets(text: str, tokens: list[str]) -> list[int]:
    offsets = []
    search_start = 0
    for token in tokens:
        offset = text.find(token, search_start)
        if offset < 0:
            offset = search_start
        offsets.append(offset)
        search_start = offset + len(token)
    return offsets


def bounded_levenshtein(left: str, right: str, limit: int) -> int:
    if abs(len(left) - len(right)) > limit:
        return limit + 1
    previous = list(range(len(right) + 1))
    for left_index, left_char in enumerate(left, start=1):
        current = [left_index]
        row_min = current[0]
        for right_index, right_char in enumerate(right, start=1):
            cost = 0 if left_char == right_char else 1
            current.append(
                min(
                    previous[right_index] + 1,
                    current[right_index - 1] + 1,
                    previous[right_index - 1] + cost,
                )
            )
            row_min = min(row_min, current[-1])
        if row_min > limit:
            return limit + 1
        previous = current
    return previous[-1]


def olmocr_cell_matches(value: str, expected: str, test: dict[str, Any]) -> bool:
    return olmocr_contains(str(value), expected, test)


def normalize_olmocr_text(text: str, test: dict[str, Any] | None = None) -> str:
    normalized = " ".join(str(text).split())
    case_sensitive = True if test is None else bool(test.get("case_sensitive", True))
    return normalized if case_sensitive else normalized.lower()


def normalize_math_text(text: str) -> str:
    text = str(text)
    text = text.replace("\\left", "").replace("\\right", "")
    text = re.sub(r"\s+", "", text)
    text = re.sub(r"[$`{}]", "", text)
    return text.lower()


def full_image_geometry_accuracy(json_out: Path) -> float | None:
    doc = json.loads(json_out.read_text())
    scores = []
    for page in doc.get("pages", []):
        width = numeric_value(page.get("width"))
        height = numeric_value(page.get("height"))
        if not width or not height:
            continue
        expected = {"x": 0.0, "y": 0.0, "width": width, "height": height}
        bboxes = []
        for block in page.get("blocks", []):
            if isinstance(block, dict) and isinstance(block.get("bbox"), dict):
                bboxes.append(block["bbox"])
        for image in page.get("images", []):
            if isinstance(image, dict) and isinstance(image.get("bbox"), dict):
                bboxes.append(image["bbox"])
        if not bboxes:
            continue
        scores.append(max(bbox_iou(expected, bbox) for bbox in bboxes))
    if not scores:
        return None
    return sum(scores) / len(scores)


def bbox_iou(left: dict[str, Any], right: dict[str, Any]) -> float:
    left_x = numeric_value(left.get("x")) or 0.0
    left_y = numeric_value(left.get("y")) or 0.0
    left_w = numeric_value(left.get("width")) or 0.0
    left_h = numeric_value(left.get("height")) or 0.0
    right_x = numeric_value(right.get("x")) or 0.0
    right_y = numeric_value(right.get("y")) or 0.0
    right_w = numeric_value(right.get("width")) or 0.0
    right_h = numeric_value(right.get("height")) or 0.0
    if left_w <= 0 or left_h <= 0 or right_w <= 0 or right_h <= 0:
        return 0.0
    inter_x0 = max(left_x, right_x)
    inter_y0 = max(left_y, right_y)
    inter_x1 = min(left_x + left_w, right_x + right_w)
    inter_y1 = min(left_y + left_h, right_y + right_h)
    inter_w = max(inter_x1 - inter_x0, 0.0)
    inter_h = max(inter_y1 - inter_y0, 0.0)
    intersection = inter_w * inter_h
    union = left_w * left_h + right_w * right_h - intersection
    return intersection / union if union > 0 else 0.0


def numeric_value(value: Any) -> float | None:
    if isinstance(value, (int, float)):
        return float(value)
    return None


def replace_readme_benchmark_section(readme: str, table: str) -> str:
    replacement = f"{START_MARKER}\n{table.rstrip()}\n{END_MARKER}"
    pattern = re.compile(f"{re.escape(START_MARKER)}.*?{re.escape(END_MARKER)}", re.S)
    if pattern.search(readme):
        return pattern.sub(replacement, readme)
    return readme.rstrip() + "\n\n## Benchmarks\n\n" + replacement + "\n"


def load_manifest(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def directory_size(path: Path) -> int:
    if not path.exists():
        return 0
    if path.is_file():
        return path.stat().st_size
    total = 0
    for item in path.rglob("*"):
        if item.is_file() and not item.is_symlink():
            total += item.stat().st_size
    return total


def discover_pdfs(path: Path) -> list[Path]:
    if not path.exists():
        return []
    return sorted(path.rglob("*.pdf"))


def discover_images(path: Path) -> list[Path]:
    if not path.exists():
        return []
    return sorted(
        item
        for item in path.rglob("*")
        if item.is_file()
        and item.suffix.lower() in IMAGE_SUFFIXES
        and not is_macos_resource_fork_sidecar(item)
    )


def is_macos_resource_fork_sidecar(path: Path) -> bool:
    return path.name.startswith("._") or "__MACOSX" in path.parts


def discover_supported_documents(path: Path) -> list[Path]:
    if not path.exists():
        return []
    return sorted(
        item
        for item in path.rglob("*")
        if item.is_file()
        and document_logical_suffix(item) in DOCUMENT_SUFFIXES
        and not is_macos_resource_fork_sidecar(item)
    )


def select_benchmark_documents(documents: list[Path], local_dir: Path, limit: int) -> list[Path]:
    if len(documents) <= limit:
        return documents
    groups: dict[str, list[Path]] = {}
    for document in documents:
        key = benchmark_group_key(document, local_dir)
        groups.setdefault(key, []).append(document)
    for group in groups.values():
        group.sort()
    selected = []
    group_names = sorted(groups)
    index = 0
    while len(selected) < limit:
        added = False
        for group_name in group_names:
            group = groups[group_name]
            if index < len(group):
                selected.append(group[index])
                added = True
                if len(selected) == limit:
                    break
        if not added:
            break
        index += 1
    return selected


def benchmark_group_key(document: Path, local_dir: Path) -> str:
    try:
        relative = document.relative_to(local_dir)
    except ValueError:
        return "."
    parts = relative.parts
    if len(parts) >= 4 and parts[0] == "bench_data" and parts[1] == "pdfs":
        return parts[2]
    if len(parts) > 1:
        return parts[0]
    return "."


def document_logical_suffix(path: Path) -> str:
    suffix = path.suffix.lower()
    if suffix != ".gz":
        return suffix
    if path.name.lower().endswith(".tar.gz"):
        return ".tar"
    inner_suffix = Path(path.stem).suffix.lower()
    if inner_suffix in DOCUMENT_SUFFIXES - {".gz"}:
        return inner_suffix
    return ".gz"


def output_name(dataset_slug: str, pdf: Path) -> str:
    digest = hashlib.sha256(str(pdf).encode()).hexdigest()[:12]
    return f"{dataset_slug}-{pdf.stem}-{digest}.json"


def build_cli(repo_root: Path) -> Path:
    subprocess.run(["cargo", "build", "-q", "-p", "dongler"], cwd=repo_root, check=True)
    executable = "dongler.exe" if os.name == "nt" else "dongler"
    return repo_root / "target" / "debug" / executable


def run_document(cli: Path, document: Path, json_out: Path) -> tuple[bool, float, str | None]:
    started = time.perf_counter()
    result = subprocess.run(
        [str(cli), "extract", str(document), "--format", "json"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    elapsed = max(time.perf_counter() - started, 0.000001)
    if result.returncode != 0:
        return False, elapsed, result.stderr.strip() or result.stdout.strip()
    json_out.write_text(result.stdout)
    return True, elapsed, None


def render_visual_comparison(
    repo_root: Path,
    cli: Path,
    document: Path,
    visual_out_dir: Path,
) -> tuple[dict[str, str] | None, str | None]:
    script = repo_root / "scripts" / "render-extraction-comparison.py"
    result = subprocess.run(
        [
            "python3",
            str(script),
            str(document),
            "--cli",
            str(cli),
            "--out-dir",
            str(visual_out_dir),
        ],
        cwd=repo_root,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if result.returncode != 0:
        return None, result.stderr.strip() or result.stdout.strip()
    try:
        return json.loads(result.stdout), None
    except json.JSONDecodeError as error:
        return None, f"visual comparison produced invalid JSON: {error}"


def format_percent(value: float | None) -> str:
    if value is None:
        return "n/a"
    return f"{value * 100:.1f}%"


def format_float(value: float | None) -> str:
    if value is None:
        return "n/a"
    return f"{value:.2f}"


def native_coverage_score(
    parse_success_rate: float | None,
    bbox_block_rate: float | None,
    anchored_block_rate: float | None,
) -> float | None:
    if parse_success_rate is None or bbox_block_rate is None or anchored_block_rate is None:
        return None
    return parse_success_rate * min(bbox_block_rate, anchored_block_rate)


def benchmark_dataset(
    dataset: dict[str, Any],
    data_root: Path,
    out_dir: Path,
    cli: Path,
    max_pdfs: int,
    repo_root: Path | None = None,
    visual_samples: int = 0,
    visual_out_dir: Path | None = None,
) -> dict[str, Any]:
    slug = dataset["slug"]
    local_dir = data_root / dataset.get("local_dir", slug)
    pdfs = discover_pdfs(local_dir)
    images = discover_images(local_dir)
    documents = discover_supported_documents(local_dir)
    structured_documents = [
        document
        for document in documents
        if document_logical_suffix(document) in STRUCTURED_DOCUMENT_SUFFIXES
    ]
    other_documents = [
        document
        for document in documents
        if document_logical_suffix(document) not in STRUCTURED_DOCUMENT_SUFFIXES
    ]
    if pdfs:
        selected = select_benchmark_documents(pdfs, local_dir, max_pdfs)
    elif structured_documents:
        selected = (structured_documents + other_documents)[:max_pdfs]
    else:
        selected = (images or documents)[:max_pdfs]
    result = {
        "dataset": dataset["name"],
        "slug": slug,
        "task": dataset["task"],
        "status": "missing" if not local_dir.exists() else "no_extractable_documents",
        "local_dir": str(local_dir),
        "local_bytes": directory_size(local_dir),
        "pdfs_found": len(pdfs),
        "documents_found": len(documents),
        "documents_evaluated": 0,
        "pdfs_evaluated": 0,
        "images_found": len(images),
        "parse_successes": 0,
        "parse_failures": 0,
        "parse_success_rate": None,
        "pages": 0,
        "blocks": 0,
        "bbox_block_rate": None,
        "anchored_block_rate": None,
        "native_coverage_score": None,
        "pages_per_second": None,
        "warnings": 0,
        "ground_truth_accuracy": None,
        "ground_truth_documents": 0,
        "ground_truth_checks": 0,
        "ground_truth_accuracy_by_document": [],
        "ground_truth_metric": None,
        "visual_comparisons": [],
        "notes": dataset.get("notes", ""),
        "errors": [],
    }

    if not local_dir.exists():
        return result
    if not selected:
        result["status"] = "no_extractable_documents"
        return result

    dataset_out = out_dir / slug
    dataset_out.mkdir(parents=True, exist_ok=True)
    olmocr_tests_by_pdf = index_olmocr_tests(local_dir) if slug == "olmocr-bench" else {}
    total_elapsed = 0.0
    total_bbox_blocks = 0
    total_anchored = 0
    total_accuracy = 0.0
    total_accuracy_weight = 0

    for document in selected:
        json_out = dataset_out / output_name(slug, document)
        ok, elapsed, error = run_document(cli, document, json_out)
        total_elapsed += elapsed
        result["documents_evaluated"] += 1
        if document.suffix.lower() == ".pdf":
            result["pdfs_evaluated"] += 1
        if not ok:
            result["parse_failures"] += 1
            result["errors"].append({"document": str(document), "error": error})
            continue

        result["parse_successes"] += 1
        summary = summarize_document_json(json_out)
        result["pages"] += summary.pages
        result["blocks"] += summary.blocks
        result["warnings"] += summary.warnings
        total_bbox_blocks += summary.bbox_blocks
        total_anchored += summary.anchored_blocks
        olmocr_tests = olmocr_tests_by_pdf.get(olmocr_document_key(document, local_dir), [])
        if not olmocr_tests:
            olmocr_tests = olmocr_tests_by_pdf.get(document.name, [])
        if olmocr_tests:
            accuracy = olmocr_unit_pass_rate(olmocr_tests, json_out)
            if accuracy is not None:
                weight = len(olmocr_tests)
                total_accuracy += accuracy * weight
                total_accuracy_weight += weight
                result["ground_truth_documents"] += 1
                result["ground_truth_checks"] += weight
                result["ground_truth_metric"] = result["ground_truth_metric"] or "olmocr_unit_pass_rate"
                result["ground_truth_accuracy_by_document"].append(
                    {
                        "document": str(document),
                        "accuracy": accuracy,
                        "metric": "olmocr_unit_pass_rate",
                        "checks": len(olmocr_tests),
                    }
                )
        else:
            reference = reference_target_for_document(document, local_dir)
            if reference:
                accuracy = best_extraction_accuracy(reference, json_out)
                if accuracy is not None:
                    total_accuracy += accuracy
                    total_accuracy_weight += 1
                    result["ground_truth_documents"] += 1
                    result["ground_truth_checks"] += 1
                    result["ground_truth_metric"] = result["ground_truth_metric"] or "token_f1"
                    result["ground_truth_accuracy_by_document"].append(
                        {
                            "document": str(document),
                            "accuracy": accuracy,
                            "metric": "token_f1",
                        }
                    )
            elif document.suffix.lower() in IMAGE_SUFFIXES:
                accuracy = full_image_geometry_accuracy(json_out)
                if accuracy is not None:
                    total_accuracy += accuracy
                    total_accuracy_weight += 1
                    result["ground_truth_documents"] += 1
                    result["ground_truth_checks"] += 1
                    result["ground_truth_metric"] = result["ground_truth_metric"] or "full_image_iou"
                    result["ground_truth_accuracy_by_document"].append(
                        {
                            "document": str(document),
                            "accuracy": accuracy,
                            "metric": "full_image_iou",
                        }
                    )
        if (
            repo_root is not None
            and visual_out_dir is not None
            and len(result["visual_comparisons"]) < visual_samples
        ):
            artifacts, visual_error = render_visual_comparison(
                repo_root,
                cli,
                document,
                visual_out_dir / slug,
            )
            if artifacts:
                result["visual_comparisons"].append(
                    {
                        "document": str(document),
                        "artifacts": artifacts,
                    }
                )
            elif visual_error:
                result["errors"].append(
                    {
                        "document": str(document),
                        "visual_comparison_error": visual_error,
                    }
                )

    result["status"] = "ok" if result["parse_successes"] else "failed"
    result["parse_success_rate"] = result["parse_successes"] / result["documents_evaluated"]
    if result["pdfs_evaluated"] > 0 or (total_bbox_blocks > 0 and total_anchored > 0):
        result["bbox_block_rate"] = total_bbox_blocks / result["blocks"] if result["blocks"] else None
        result["anchored_block_rate"] = total_anchored / result["blocks"] if result["blocks"] else None
        result["native_coverage_score"] = native_coverage_score(
            result["parse_success_rate"],
            result["bbox_block_rate"],
            result["anchored_block_rate"],
        )
    result["pages_per_second"] = result["pages"] / total_elapsed if total_elapsed else None
    if result["ground_truth_documents"]:
        result["ground_truth_accuracy"] = total_accuracy / total_accuracy_weight
    return result


def markdown_table(results: list[dict[str, Any]], generated_at: str, max_pdfs: int) -> str:
    total_mb = sum(row["local_bytes"] for row in results) / (1024 * 1024)
    lines = [
        f"_Generated by `scripts/run-benchmarks.py` on {generated_at}. "
        f"Local cache: {total_mb:.1f} MB. Cap: {max_pdfs} files per dataset._",
        "",
        "Coverage is `parse / bbox / anchors`. Ground-truth accuracy is token-F1, "
        "olmOCR unit-check pass rate, or full-image IoU; `n/a` means no local target signal. "
        "Detailed task names, discovery counts, native scores, and notes are recorded in "
        "`eval/out/benchmarks/latest.json`.",
        "",
        "| Dataset | Status | Local data | Docs eval | Coverage | Pages/sec | GT accuracy |",
        "| --- | --- | ---: | ---: | --- | ---: | ---: |",
    ]
    for row in results:
        local_mb = row["local_bytes"] / (1024 * 1024)
        lines.append(
            "| {dataset} | {status} | {local_mb:.1f} MB | {documents_evaluated} | {coverage} | {pps} | {accuracy} |".format(
                dataset=row["dataset"],
                status=row["status"],
                local_mb=local_mb,
                documents_evaluated=row.get("documents_evaluated", row["pdfs_evaluated"]),
                coverage=" / ".join(
                    [
                        format_percent(row["parse_success_rate"]),
                        format_percent(row["bbox_block_rate"]),
                        format_percent(row["anchored_block_rate"]),
                    ]
                ),
                pps=format_float(row["pages_per_second"]),
                accuracy=format_percent(row["ground_truth_accuracy"]),
            )
        )
    return "\n".join(lines)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run Dongler local document benchmarks.")
    parser.add_argument("--manifest", default="eval/datasets/document-benchmarks-v1.json")
    parser.add_argument("--data-root", default=os.environ.get("DONGLER_EVAL_DATA_DIR", "eval/data"))
    parser.add_argument("--out-dir", default=os.environ.get("DONGLER_EVAL_OUT_DIR", "eval/out/benchmarks"))
    parser.add_argument("--max-pdfs-per-dataset", type=int)
    parser.add_argument("--dataset", action="append", help="Dataset slug to include; may repeat.")
    parser.add_argument("--update-readme", action="store_true")
    parser.add_argument("--readme", default="README.md")
    parser.add_argument(
        "--visual-samples-per-dataset",
        type=int,
        default=0,
        help="Render this many original/Markdown/LaTeX visual comparisons per dataset.",
    )
    parser.add_argument(
        "--visual-out-dir",
        default=os.environ.get("DONGLER_VISUAL_OUT_DIR", "eval/out/visual-comparisons"),
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo_root = Path.cwd()
    manifest = load_manifest(repo_root / args.manifest)
    data_root = repo_root / args.data_root
    out_dir = repo_root / args.out_dir
    visual_out_dir = repo_root / args.visual_out_dir
    out_dir.mkdir(parents=True, exist_ok=True)
    if args.visual_samples_per_dataset > 0:
        visual_out_dir.mkdir(parents=True, exist_ok=True)
    max_pdfs = args.max_pdfs_per_dataset or manifest.get("default_max_pdfs_per_dataset", 50)
    selected = set(args.dataset or [])
    datasets = [
        dataset
        for dataset in manifest["datasets"]
        if not selected or dataset["slug"] in selected
    ]

    cli = build_cli(repo_root)
    generated_at = time.strftime("%Y-%m-%d %H:%M:%S %Z")
    results = [
        benchmark_dataset(
            dataset,
            data_root,
            out_dir,
            cli,
            max_pdfs,
            repo_root=repo_root,
            visual_samples=max(args.visual_samples_per_dataset, 0),
            visual_out_dir=visual_out_dir,
        )
        for dataset in datasets
    ]
    payload = {"generated_at": generated_at, "manifest": manifest["name"], "results": results}
    (out_dir / "latest.json").write_text(json.dumps(payload, indent=2, sort_keys=True))
    table = markdown_table(results, generated_at, max_pdfs)
    (out_dir / "latest.md").write_text(table + "\n")
    print(table)

    if args.update_readme:
        readme_path = repo_root / args.readme
        readme_path.write_text(replace_readme_benchmark_section(readme_path.read_text(), table))

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
