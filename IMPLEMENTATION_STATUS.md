# Implementation Status — Dongler v2 Rust-Native Hybrid Pipeline

Tracks progress against `PRD.md`. Honest about what is built-and-tested vs. what
still requires model downloads, large datasets, or GPU-class resources.

## Summary

| PRD milestone | Status | Notes |
|---|---|---|
| **M0 — Eval harness** | ✅ Done | Real metrics wired to extraction output + GT; smoke slice; regression diff |
| **IR v2** | ✅ Done | `route`, `provenance`, `TableBlock.html`, `BlockKind` vocab; v1 stays deserializable |
| **M1 — Deterministic pipeline** | ✅ Done | triage · XY-Cut++ reading order · fusion · `convert()` · HTML tables |
| **M2 — Layout + reading order** | 🟡 Scaffolded | ML stages compile (ort+pdfium); preprocessing/decode tested; model wiring + bake-off (E2) pending real weights |
| **M3 — OCR** | ⬜ Todo | Per-region OCR (PP-OCRv5) — registry entry present |
| **M4 — Tables** | ⬜ Todo | SLANet-plus structure + text-snap fusion — fusion + HTML rendering ready |
| **M5 — Ship / VLM** | ⬜ Todo | Packaging, formula stretch, VLM escalation hook |

## What's built and tested

### M0 — Eval harness (`eval/`)
The PRD's #1 fix. Wires the existing metric library to real dongler output + GT.
- `eval/harness/run.py` — runs the CLI (`--cmd extract|convert`) over a suite,
  scores per-doc (edit-sim, CER, WER, BLEU, token coverage, spurious-token rate,
  TEDS when applicable), writes `per_doc.json` + `aggregate.json` + `report.md`.
- `eval/harness/diff.py` — regression gate (non-zero exit on regression).
- `eval/smoke/` — 6 committed, license-clean PDF+GT fixtures.
- `eval/metrics/` — canonical copy of the metric library.
- **E0 baseline (smoke slice, current dongler):** edit_similarity 0.958,
  spurious_token_rate 0.0.
- Run: `uv run --no-project --with pytest pytest eval/tests/ -q` → 12 passed.

### IR v2 (`crates/dongler-core/src/ir.rs`)
Additive, non-breaking (v1 still deserializes). Adds `Page.route`,
`*.provenance`, `TableBlock.html`, and a tolerant `BlockKind` vocabulary helper.
Renderer now emits HTML tables for spans and drops page furniture.

### M1 — Deterministic pipeline (`crates/dongler-pipeline/`)
Model-free, default build pulls in **no** ONNX/pdfium.
- `geometry` · `order` (XY-Cut++) · `fusion` (R*-tree, no-drop invariant) ·
  `triage` · `textprovider` (TextProvider trait) · `registry` (license-gated) ·
  `Pipeline::convert_*`.
- CLI: `dongler convert <pdf> --format markdown|json|latex` (fast path stays
  `dongler extract`).
- **M1 no-regression gate:** on the smoke slice `convert` == `extract`
  (0 regressions via `eval diff`).

### M2 — ML scaffold (`crates/dongler-pipeline/src/ml/`, `--features ml`)
Proves the Rust ONNX path compiles end-to-end (ort 2.0.0-rc.12 + pdfium-render).
- `preprocess` (RGB→NCHW tensor, RT-DETR decode) and `layout` (label→region
  mapping) are pure and unit-tested.
- `LayoutEngine` loads/runs an ONNX model (`run_raw`); `raster` renders pages.
- Model-specific output decode/normalization are pinned in the E2 bake-off.

## How to build & test

```bash
cargo test                              # all crates, default (model-free)
cargo test -p dongler-pipeline --features ml   # + ML scaffold (downloads ORT)
uv run --no-project --with pytest pytest eval/tests/ -q   # harness
# Measure current engines on the smoke slice:
cargo build -p dongler
python3 -m eval.harness.run --suite smoke --bin ./target/debug/dongler --cmd convert
```

## What remains (requires resources beyond this environment)

- **E0/E2/E4 bake-offs** — download model weights (HF) + benchmark datasets
  (OmniDocBench/olmOCR/DocLayNet/FinTabNet, multi-GB) and pin output decode.
- **M3 OCR / M4 tables** — wire PP-OCRv5 + SLANet-plus through `ort`, snap text
  via the existing fusion stage.
- **Dataset-scale evals + GPU throughput targets** — per PRD §9.
- **VLM escalation (M5/E7)** — opt-in Granite-Docling route.
