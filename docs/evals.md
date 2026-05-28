---
sidebar_position: 6
---

# Evals

Dongler keeps public eval configuration in
`eval/datasets/document-benchmarks-v1.json`. The default benchmark set covers
layout, table structure, reading order, OCR-oriented image datasets, and
end-to-end Markdown quality:

- DocLayNet for layout classes and bounding boxes.
- PubTables-1M for table, row, column, and cell geometry.
- olmOCR-Bench for end-to-end document conversion checks.
- Korzen PDF text extraction benchmark for scientific PDF text order.

Large public datasets are not downloaded in CI. For local runs:

```bash
make bench-data
make bench-run
make eval-smoke PDF=paper.pdf
```

The downloader keeps data in `eval/data/` and generated reports in `eval/out/`;
both paths are git-ignored so local corpora remain inspectable but are not
committed. The default data budget is 100GB and can be reduced:

```bash
DONGLER_DATA_BUDGET_GB=25 make bench-data
```

PubTables-1M requires a generated Microsoft Research Open Data Azure URL and is
opt-in:

```bash
PUBTABLES1M_AZURE_URL="https://..." python3 scripts/download-benchmark-data.py pubtables-1m
```

Benchmarks write per-document JSON plus Markdown/JSON summaries under
`eval/out/`. The default table reports parse success, block bounding-box
coverage, source-anchor coverage, pages per second, native coverage score, and
ground-truth accuracy where the evaluated file has an aligned local target.
Ground-truth accuracy is token-F1 for aligned text targets, check-weighted
olmOCR unit pass rate for downloaded olmOCR JSONL checks, or full-image IoU for
image-only crop datasets. PDFs are preferred when a dataset has them. If no PDFs are present,
structured JSON/CSV/XML annotations are preferred over raw images, with image
files and supported text/Office/OpenDocument/HTML/XML/email/source files used as
fallbacks. Dataset ground-truth accuracy stays `n/a` only when no aligned local
target signal is available for the evaluated files.

Native structured fallbacks currently include COCO JSON layout annotations,
DocBank token-label text annotations, PubTabNet JSONL table structure
annotations with cell boxes, FUNSD JSON forms, SROIE CSV OCR boxes, Tesseract
TSV OCR lines, grid-cell JSON table annotations, word-box JSON annotations,
ckorzen TSV feature boxes, PASCAL VOC XML boxes, ALTO XML OCR lines, PAGE XML
OCR lines, hOCR HTML OCR lines, and anchored image-page blocks from common image
headers including TIFF. Gzip-compressed text, JSON/JSONL, CSV/TSV, and XML
corpus files are detected by their compound extension, and `.zip`/
`.tar`/`.tar.gz` plus bare `.gz` source packages are scanned for supported text
resources.

Visual comparison artifacts can be generated for a document after building the
CLI:

```bash
python3 scripts/render-extraction-comparison.py paper.pdf
```

The script writes the original first-page PNG when the source is a PDF, extracted
Markdown, extracted LaTeX, rendered Markdown PNG, rendered LaTeX PNG, and a JSON
manifest under `eval/out/visual-comparisons/`.

Benchmark runs can also attach visual samples:

```bash
python3 scripts/run-benchmarks.py --dataset docbank --visual-samples-per-dataset 1
```

Those per-dataset artifacts are recorded in `eval/out/benchmarks/latest.json`
under each row's `visual_comparisons` field.
