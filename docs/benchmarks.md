---
sidebar_position: 5
title: Benchmarks
description: How accurately and how fast Dongler converts real documents — measured on public, PDF-native benchmarks.
---

# Benchmarks

Dongler runs **locally** with no cloud service, no API key, no LLM, and no OCR for
digitally-born PDFs. These benchmarks measure two things that matter for a
document-extraction engine: **how faithfully** it turns real PDFs into
Markdown / LaTeX / JSON, and **how fast** it does it.

All numbers come from `scripts/run-benchmarks.py` against public datasets, using a
release build on a single host with no GPU. Nothing here is dataset-specific —
the same extractor runs on every document.

## At a glance

| | |
|---|---|
| **Throughput** | ~90 born-digital pages / second (single host, no GPU) |
| **Tables (olmOCR-Bench)** | **59.7% → 65.5%** unit-check pass rate (+9.7% relative) |
| **Reading order (olmOCR-Bench)** | 30.7% → 32.0% |
| **Footprint** | 0 network calls · 0 model downloads · runs offline |
| **Outputs** | Markdown · LaTeX · structured JSON (`dongler.ir.v1`) |

Deltas compare the **previous release to the current release** on the identical
harness, isolating the extractor's own improvement.

## What we measure

[**olmOCR-Bench**](https://huggingface.co/datasets/allenai/olmOCR-bench) is the
reliable PDF-native benchmark: **1,403 real PDFs** with **7,019 unit checks** that
assert specific facts about the extracted text — is a phrase present, is a
header/footer correctly omitted, are table cells adjacent, is multi-column text in
reading order, is a formula preserved.

:::note Reading the absolute numbers
olmOCR-Bench deliberately mixes **born-digital** and **scanned** documents.
Dongler targets born-digital PDFs and does **no OCR**, so scanned pages bound the
absolute pass rate. The meaningful signal is the **delta on the identical harness**
and the per-check-type breakdown below — not the headline percentage.
:::

## Results — olmOCR-Bench

Pass rate by check type (full 1,403-PDF run, release build):

| Check type | Previous | Current | Change |
|---|---:|---:|:--:|
| **Tables** | 59.7% | **65.5%** | 🟢 +9.7% rel. |
| **Reading order** | 30.7% | **32.0%** | 🟢 +4.0% rel. |
| Math preservation | 1.6% | 1.6% | — |
| Overall checks passed | 1,562 | **1,595** | 🟢 +33 |

The table and reading-order gains come from the structural work described below.
(Text-present / text-absent checks moved within noise; some shifts are *better*
extraction — a correctly-spaced running header now matches an "omit this header"
check that previously passed only because the header was garbled.)

## Extraction quality: before → after

The benchmark deltas are driven by concrete, visible fixes. On a rendered
SEC 10-K:

**Word segmentation** — born-digital PDFs position glyphs individually; the old
assembly dropped real spaces and inserted phantom ones:

| Before | After |
|---|---|
| `UNITEDSTATES` | `UNITED STATES` |
| `Washington, D. C. 2 0 5 4 9` | `Washington, D.C. 20549` |
| `Netincome` · `fi scal` | `Net income` · `fiscal` |
| `Share-b asedcompensationexpense` | `Share-based compensation expense` |

**Tables** — a financial statement is one table with section headers between the
rows. It used to fragment into a single block plus a detached wall of numbers; it
now extracts as one complete, aligned table:

```text
| Net income                       | 93,736 | 96,995 | 99,803 |
| Depreciation and amortization    | 11,445 | 11,519 | 11,104 |
| Share-based compensation expense | 11,688 | 10,833 |  9,038 |
| Other                            | (2,266)| (2,227)|  1,006 |
```

The same structure is emitted in **Markdown, LaTeX, and JSON**, so a downstream
LLM sees aligned columns rather than a stream of digits. Verified across 14
companies (Apple, Microsoft, J&J, Boeing, NextEra, Tesla, Verizon, …).

## Speed

Dongler's PDF engine is written from scratch in Rust (`flate2` + `rayon`, no
pdfium/poppler), parses pages in parallel, and runs no per-page model — so it
sustains **~90 born-digital pages per second** on a single host with no GPU while
producing the structured output above.

## Experimental: hybrid ML table structure

All numbers above are the **deterministic, model-free** engine — what every
release ships and what `dongler.load` / `convert` / `extract` use by default.

A separate, opt-in path (`convert --ml`, builds with the `ml` feature) recognizes
table **structure** with a local ONNX model and snaps cell text from the text
layer (hallucination-free by construction). It is **preview-grade and not yet on
the scoreboard**: the end-to-end plumbing is verified, but its accuracy is bounded
by the geometric table-region detector that feeds the model — clean table regions
from a layout-detection model are the next milestone, at which point a TEDS
result on FinTabNet.c lands here. Until then, the deterministic numbers above are
the ones to rely on.

## Reproduce

```bash
make bench-data      # download bounded public corpora into eval/data/ (git-ignored)
make bench-run       # run the full benchmark, write reports to eval/out/
```

A/B two builds on the identical harness:

```bash
python scripts/run-benchmarks.py --dataset olmocr-bench --cli ./target/release/dongler
```

Per-document scores, check-type breakdowns, and failure diagnostics are written to
`eval/out/benchmarks/latest.json`. See [Evals](./evals) for the dataset manifest
and licensing notes.
