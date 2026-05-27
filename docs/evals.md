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

Benchmarks write per-PDF JSON plus Markdown/JSON summaries under `eval/out/`.
The default table reports parse success, block bounding-box coverage,
source-anchor coverage, pages per second, and a native coverage score. Dataset
ground-truth accuracy stays `n/a` until an aligned target harness exists for
that dataset and modality.
