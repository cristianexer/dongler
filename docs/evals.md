---
sidebar_position: 6
---

# Evals

Dongler keeps public eval configuration in `eval/datasets/dongler-public-v1.json`.
The default benchmark set covers layout, table structure, reading order, and
end-to-end Markdown quality:

- DocLayNet for layout classes and bounding boxes.
- PubTables-1M for table, row, column, and cell geometry.
- olmOCR-Bench for end-to-end document conversion checks.
- Korzen PDF text extraction benchmark for scientific PDF text order.

Large public datasets are not downloaded in CI. For local runs:

```bash
make eval-data
make eval-smoke PDF=paper.pdf
```

PubTables-1M requires a generated Microsoft Research Open Data Azure URL:

```bash
PUBTABLES1M_AZURE_URL="https://..." make eval-data
```

Smoke evals write JSON, Markdown, runtime, deterministic output hashes, warning
counts, and anchored-block rates under `eval/out/smoke/`.
