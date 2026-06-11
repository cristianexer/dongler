# Dongler v2 — Hybrid PDF-to-Markdown Extraction: Master Plan (PRD)

| | |
|---|---|
| **Status** | ACCEPTED — implementation plan of record |
| **Date** | 2026-06-11 |
| **Supersedes** | `IMPROVEMENT_PLAN.md` (kept for its license research §2/§6 and metric work §4.9, which carry forward) |
| **Branch** | starts from `claude/pdf-extraction-plan-edh271` |

**Thesis.** Dongler's quality plateaued for two reasons: (a) the heuristic-only architecture has a hard ceiling on layout, tables, and scanned documents, and (b) the eval harness measured the wrong things, so effort went into rabbit holes that "improved" numbers without improving output. This plan fixes **measurement first**, then rebuilds extraction as a **hybrid pipeline** — deterministic born-digital text extraction fused with pretrained local models for layout, tables, and OCR. This is the architecture proven by MinerU, Marker, and Docling. We salvage the IR schema, the metric library, and the Rust parser; everything else is rebuilt.

**Locked decisions** (made 2026-06-11, do not re-litigate):

1. **Approach: hybrid pipeline.** Deterministic text layer + pretrained models. No model training from scratch. No pure-heuristics path.
2. **Scope: born-digital AND scanned PDFs.**
3. **Constraints: local-first.** ML dependencies (ONNX weights) are fine. CPU must work; GPU is an accelerator, not a requirement. No required API calls.
4. **Clean slate with salvage.** Fresh package layout; salvage `crates/dongler-core/src/ir.rs` (evolved to v2), `scripts/eval_metrics/` (moved to `eval/metrics/`), and the Rust parser as a text provider that must *earn* its place via E0.
5. **Output: Markdown with embedded HTML tables** (HTML preserves rowspan/colspan; pipe tables are a lossy opt-in).

Non-goals are in §10 — read them before adding scope.

---

## 1. Problem statement and post-mortem

Why did months of work not move the needle? Three repo-verifiable causes:

1. **Measurement was broken.** `scripts/run-benchmarks.py` only ever wired three metric families: token-F1 (bag-of-words overlap — insensitive to structure, order, and tables), olmOCR unit pass rate, and a degenerate `full_image_iou` that scores each block against the *whole page rectangle*. The real metrics — TEDS, GriTS, CER/WER, reading-order ARD/Kendall-τ — were implemented in `scripts/eval_metrics/` (dependency-free, unit-tested) but **never connected to dataset ground truth**. The "100.0% GT accuracy" rows in the README (TableBank, SROIE) measured parse success, not fidelity. The only number that resembles reality is olmOCR-Bench overall: **22.7%** (1,595/7,019 checks).
2. **The heuristics hit their ceiling.** Reading order is hard-coded for ≤2 columns. Table detection requires a literal "Table" caption or a ≥4×2 ruled grid. Math passes 1.6% of olmOCR checks. Scanned pages are out of scope entirely, which bounds every mixed benchmark. Each additional heuristic (financial tables, columnar rescue, leader-gap rows — see `git log` v0.3.10–v0.3.12) bought a point or two on one dataset while adding brittle special cases.
3. **The field already solved this shape of problem.** Every credible open-source converter (MinerU, Marker, Docling, olmOCR) converged on the same architecture: a vision model for page-level layout understanding, the PDF text layer for born-digital text fidelity, OCR only where no text layer exists, and a fusion step that snaps model-detected regions to deterministic text. We adopt that architecture instead of re-deriving it heuristic by heuristic.

**The rule going forward: no extraction change merges without an eval diff on the frozen suite.** If we can't see a change in the numbers, we don't ship it.

---

## 2. Eval-first methodology (the #1 fix)

This section is the contract that prevents future rabbit holes. The harness gets rebuilt **before** any extraction work (M0).

### 2.1 The frozen benchmark suite

- A frozen test set defined in `eval/datasets/frozen-suite-v1.json`: pinned dataset revisions (HF commit hashes), pinned document lists (explicit file IDs, not globs), pinned metric-library version. Any change to the suite bumps the version and invalidates cross-version comparisons.
- **Dev/test split discipline.** Every dataset gets a *dev slice* (tune freely, run constantly) and a *frozen test slice* (scored only at milestone gates; results appended to an immutable `eval/results/ledger.jsonl`). Budget: ≤3 frozen-suite runs per milestone.
- **Baseline-before-change rule.** Every PR touching extraction includes `eval diff` output (per-document deltas) in its description.

### 2.2 Metric–dataset matrix

All metrics already exist in `scripts/eval_metrics/` (`text.py`, `table.py`, `grits.py`, `layout.py`, `order.py`); M0 wires them to ground truth.

| Dataset | License tier | Role | Metrics |
|---|---|---|---|
| **OmniDocBench** (~1,651 pages) | eval-only | **Headline end-to-end**: per-element markdown GT, table HTML, reading order, layout boxes | edit-similarity/CER per element type, TEDS, ARD + Kendall-τ, layout mAP |
| **olmOCR-Bench** (1,403 PDFs, 7,019 checks) | ODC-BY (permissive) | **Headline unit checks** incl. scanned, math, multi-column; per-category breakdown is mandatory in every report | unit pass rate by category |
| **READoc** | MIT | Markdown structure (headings/lists) on arXiv/GitHub docs | edit-sim, BLEU, heading-structure score |
| **FinTabNet.c** | CDLA-Permissive-2.0 | Table model dev + frozen test | TEDS, TEDS-struct, GriTS top/con/loc |
| **PubTabNet** (subset) | CDLA-Permissive-1.0 | Table dev (large, cheap slices) | TEDS |
| **DocLayNet** (capped slice) | CDLA-Permissive-1.0 | Layout model dev + frozen test | COCO mAP, mean-best-IoU |
| **Internal-50** (§5) | owned | **The reality check.** Fully held out from all tuning | edit-sim vs hand-checked markdown + manual rubric |

License gating carries over from `eval/datasets/document-benchmarks-v2.json` (`license_class`: `permissive` / `eval_only` / `unverified`; downloader `--allow` flag). Eval-only data never ships in the repo or in any tuning artifact; published headline numbers always note the tier.

### 2.3 Anti-Goodhart rules

1. **No dataset-conditional code paths.** No dataset name appears outside `eval/` (grep-able review rule).
2. **Per-document regression counts** are reported alongside aggregates. A change that raises the mean but regresses >5% of documents needs written justification.
3. **Two uncorrelated metric families minimum** for a merge: e.g. text edit-sim AND olmOCR units AND TEDS must all be non-regressing.
4. **Internal-50 is never tuned against.** OmniDocBench frozen slice is scored only at milestone gates.
5. **Throughput is part of the suite** (pages/sec on a fixed fixture), so quality wins can't silently cost 10× latency.

### 2.4 CI smoke slice

~25 documents in `eval/smoke/`, license-clean only (olmOCR ODC-BY + READoc MIT + PubTabNet CDLA + owned docs), <5 min on CPU. CI fails on any per-doc metric drop beyond a noise epsilon; artifacts stored per run.

### 2.5 Artifacts and dashboards

- Every run writes `eval/out/runs/<run_id>/`: per-doc JSON scores, aggregate JSON, markdown report.
- `eval diff <baseline> <candidate>`: improved/regressed/unchanged doc lists, top-10 worst regressions linked to visual diffs.
- **Visual side-by-side** (evolve `scripts/render-extraction-comparison.py` into the harness): one HTML page per doc showing page raster | layout-box overlay | rendered markdown | GT markdown. Looking at output is mandatory at every gate; aggregate numbers alone are how we got here.

---

## 3. Architecture: the hybrid pipeline

The pipeline lives in **Python** (every credible ML component ships Python/ONNX). The Rust core remains the born-digital text engine via the existing PyO3 binding (`crates/dongler-python` → `python/dongler/`). Stages:

```
PDF ─► A. Triage (per page: born_digital | scanned | hybrid)
        ├─► B. Rasterize (pypdfium2, ~150–200 DPI; higher-DPI crops for OCR/tables)
        ├─► C. Text layer (dongler-core, pdfium-text fallback)        [born_digital]
        ├─► D. Layout detection (ONNX model, every page) ──► typed regions
        ├─► E. OCR (per region, scanned/hybrid only)
        ├─► F. Table structure model (table regions) ──► grid topology only
        ├─► G. Fusion: snap region text from text layer or OCR; no text dropped
        ├─► H. Reading order: recursive XY-cut over typed blocks
        └─► I. Render IR v2 ──► Markdown (+ embedded HTML tables) / JSON / LaTeX
```

### 3.A Triage / router (per page)

Classify each page using the text layer (Stage C, cheap) and page imagery:

- `born_digital`: text-layer characters cover ≥ ~85% of inked area (threshold tuned in E3).
- `scanned`: no/negligible text layer, or a single image covers >90% of the page.
- `hybrid`: image-heavy with a partial text layer — including scans with embedded invisible OCR (text render mode 3 / text-over-image overlap). Probe the embedded OCR's quality and either trust it or discard and re-OCR.

The route is recorded per page in IR provenance. **Invariant: a born-digital page never routes to OCR** (regression-tested).

### 3.B Rasterization — `pypdfium2`

pdfium (Apache-2.0/BSD-style, wheels everywhere, battle-hardened). The Rust parser does not render and writing a rasterizer is a non-goal. This is the one new native dependency.

### 3.C Born-digital text source — `dongler-core`, but it must earn it

- **For keeping it:** MIT and ours; emits spans with font/size/bold/italic + bboxes + source anchors (`ir.rs::Span`), richer than pdfium's text API; the word-segmentation/ligature/CIDFont work of Phases 0–1 is real measured progress (olmOCR tables 59.7→65.5%); zero extra dependency on the fast path.
- **Against:** 4,350 LOC of custom parser; pdfium is more robust on malformed files; sunk cost is not an argument.
- **Resolution:** the fusion layer defines a `TextProvider` interface with two implementations: `dongler-core` (default) and `pdfium-text` (fallback + comparison). **E0 runs both head-to-head on per-page CER (OmniDocBench/ckorzen text GT). If dongler-core loses by >1 CER point on >10% of pages, the default flips.** Either way, pdfium-text is the automatic fallback when dongler-core errors on a file. The parser earns its place with data.
- **Demoted from the salvaged core** (out of the hot path, superseded by stages D–F/H): the heuristic table detectors, 2-column reading-order detectors, and heading classifier. **Kept:** parsing, font decoding, spans, geometry, rotation, renderers.

### 3.D Layout detection (ML, every page)

A pretrained detector on the page raster → typed regions: `title, text, list, table, figure, caption, formula, page_header, page_footer, footnote`.

- **Default (pending E2 bake-off): Docling's layout model** (docling-ibm-models lineage, MIT code, RT-DETR family), run through **ONNX Runtime** (CPU EP default, CUDA EP optional).
- **Contender:** PP-DocLayout (Apache-2.0) via paddle2onnx.
- **Reference-only:** DocLayout-YOLO (YOLOv10-derived, AGPL — incompatible with an MIT-distributed default) and Surya layout (modified-GPL/revenue-capped license — optional extra, never the default). *Exact weight licenses verified during E2.*

### 3.E OCR (scanned/hybrid pages, per detected region)

- **Default (pending E1): RapidOCR** — Apache-2.0 code, ONNX Runtime, PaddleOCR-lineage det+rec models, CPU-friendly, multilingual. *Model-weight provenance verified in E1.*
- **Contenders:** PaddleOCR PP-OCRv4/v5 (Apache-2.0 but heavy runtime), Surya OCR (top quality, restricted license — opt-in extra), Tesseract (baseline floor only).
- OCR runs **per layout region**, not per page (accuracy + speed), with an orientation/deskew probe first.

### 3.F Table structure recognition

- **Default (pending E4): TATR** (microsoft/table-transformer, MIT code; *weight license verified in E4*). **Speed contender:** SLANet/RapidTable (Apache-2.0, ONNX).
- **Critical design rule (the Docling trick):** the model predicts **grid topology only** — rows, columns, spanning cells. Cell *content* is snapped from the deterministic text layer (born-digital) or region OCR (scanned) by bbox intersection. The table model never transcribes text.

### 3.G Fusion (the secret sauce)

This is an algorithm, not a vibe:

1. Transform layout regions from raster coordinates into PDF user space (inverse render matrix).
2. For each region on a `born_digital` page: collect text-layer lines/spans whose bbox centers fall inside the region (with tolerance); region text = those spans in line order. **Text always comes from the text layer when it exists.** OCR text is used only where the layer is absent — on `hybrid` pages this is a per-region decision (region text-layer coverage < threshold → OCR that region's crop).
3. **Conflict policy:** overlapping regions resolve by class priority (table > figure > text); each span is claimed by exactly one region; orphan spans attach to the nearest region or emit as a fallback text block. **Hard invariant, tested: no text is ever silently dropped.**
4. Page headers/footers/page numbers: layout-model class + repeated-across-pages check; kept in IR with their `kind`, excluded from default markdown (matches olmOCR-Bench expectations).
5. Every region becomes an IR block with provenance: `text_source: text_layer|ocr`, `detector: <model>@<version>`, `confidence`.

### 3.H Reading order — recursive XY-cut over typed blocks

Rule-based recursive XY-cut with XY-Cut++ refinements (extract full-width elements first; density-adaptive cut direction), applied to the ~10–30 detected layout blocks — **not** to raw lines (ordering typed blocks is far more robust). Measured with ARD/Kendall-τ on OmniDocBench. A learned reading-order model (LayoutReader-style) is explicitly deferred unless XY-cut misses the E5 gate.

### 3.I Rendering — IR v2 → Markdown

- **Markdown with embedded HTML tables** by default (preserves spans; what Marker/MinerU emit). `--tables=pipe` for lossy GFM tables. Math as `$…$`/`$$…$$`; formula *recognition* is a stretch goal (initially: LaTeX from text layer where available, else image + placeholder).
- **IR evolves to `dongler.ir.v2`** (extend `crates/dongler-core/src/ir.rs`; v1 stays deserializable): closed enum of block kinds (`heading_1..6, paragraph, list_item, code, formula, caption, page_header, page_footer, footnote, table, figure`), per-block provenance struct, per-page `route`, `TableBlock.html: Option<String>` alongside existing `cells` (which already carry `col_span`/`row_span`).
- Rendering stays in Rust (`render.rs`) so CLI/fast path keep working; the Python pipeline constructs IR JSON and calls the existing binding.

### 3.J Language / process architecture

- **The Python orchestrator package is the product** for the hybrid pipeline; the Rust core is a library inside it. Node/WASM bindings keep the born-digital fast path only — stated expectation: the full pipeline is Python-only in v1.
- Two public modes:
  - `dongler.load(path)` — fast path, Rust only, today's behavior (~90 pages/s), zero model downloads.
  - `dongler.convert(path, pipeline="hybrid")` — full pipeline.
- Model weights lazy-download to `~/.cache/dongler/models` (HF Hub) with `dongler models download` for offline prefetch. Never required for the fast path.

---

## 4. New repo layout

```
dongler/
  crates/dongler-core/            # SALVAGED: parser, ir.rs (v2), render.rs; heuristic detectors demoted
  crates/dongler-{python,cli,node,wasm}/   # bindings; node/wasm = fast path only
  python/dongler/
    __init__.py                   # load() fast path (existing) + convert() hybrid entry
    pipeline/
      triage.py rasterize.py textlayer.py    # Stages A–C
      layout.py ocr.py tables.py             # Stages D–F
      fusion.py order.py render.py           # Stages G–I
      models/registry.py          # model name → version → sha256 → license → URL
      ir.py                       # typed IR v2 dataclasses <-> JSON
    cli.py                        # `dongler convert`, `dongler models`
  eval/
    metrics/                      # MOVED from scripts/eval_metrics/ (unchanged + tests)
    harness/                      # rewrite of run-benchmarks.py: run.py, diff.py, visual.py, adapters/<dataset>.py
    datasets/                     # frozen-suite-v1.json, document-benchmarks-v2.json
    internal/                     # Internal-50 corpus + GT (committed where rights allow)
    smoke/                        # CI slice
    results/ledger.jsonl          # immutable milestone-gate scores
  experiments/                    # E0..E6: one dir each with README.md + results.md (committed)
  PRD.md
```

---

## 5. Datasets plan

Builds on the license-classed manifest already in `eval/datasets/document-benchmarks-v2.json` and the research in `IMPROVEMENT_PLAN.md` §2.

| Dataset | Tier | Use | Cap |
|---|---|---|---|
| olmOCR-Bench | permissive (ODC-BY) | frozen headline + smoke slice | full (~340 MB) |
| OmniDocBench | eval-only | frozen headline end-to-end | full (~40 MB) |
| READoc | permissive (MIT) | dev + smoke | full (~40 MB) |
| FinTabNet.c | permissive (CDLA-P-2.0) | table dev + frozen test | 2 GB slice |
| PubTabNet | permissive (CDLA-P-1.0) | table dev | 1 GB slice |
| DocLayNet | permissive (CDLA-P-1.0) | layout dev + frozen test | 2 GB slice |
| ckorzen | research | text CER head-to-head (E0) | existing 67 MB |
| DocBank | research | secondary text check | existing slice |

**Dropped from v1** (manifest entries kept, marked deprecated): TableBank (CC BY-NC-ND, weak GT), RVL-CDIP (classification, irrelevant), bulk arXiv/PMC/S2ORC (low value per GB).

**Internal-50 — the dataset that catches what academic benchmarks miss.** 30–50 real-world PDFs we actually care about: SEC filings (`scripts/download-sec-10k.py` exists), invoices, manuals, multi-column papers, scanned letters, rotated scans, CJK/RTL samples, forms. Hand-checked markdown ground truth, written once, reviewed, frozen, stored in `eval/internal/` with a per-file rights note. **Never tuned against.** Building this is an M0 task (GT writing can run in parallel with harness work).

---

## 6. Experiments

Each experiment gets a directory under `experiments/` with a README (hypothesis, method) and a committed `results.md`. Time-boxed; the gate decides, not vibes.

| ID | Question | Method | Gate / decision criterion |
|---|---|---|---|
| **E0 — Baseline reality check** (in M0) | Where is the bar? Are we actually bad, and where exactly? | Run *current dongler* AND **Marker, MinerU, Docling** (+ pymupdf4llm reference-only, AGPL) through the rebuilt harness on the frozen suite. Also: dongler-core vs pdfium-text per-page CER. | Harness reproduces competitors' published numbers within tolerance (validates the harness itself). Text-provider default decided (>1 CER pt worse on >10% pages → flip). All numbers → ledger. |
| **E1 — OCR bake-off** | Which OCR engine, locally, CPU-first? | RapidOCR vs PaddleOCR vs Surya vs Tesseract on olmOCR scanned slices + OmniDocBench scanned pages: CER/WER, s/page CPU, license verification. | Pick default. Must beat Tesseract on CER and meet CPU latency budget. |
| **E2 — Layout bake-off** | Which layout detector? | Docling-layout vs PP-DocLayout (vs AGPL/restricted models as reference ceiling) on DocLayNet slice + OmniDocBench layout GT: mAP, latency, ONNX-CPU viability, license check. | Pick default. |
| **E3 — Triage + fusion tuning** | What text-coverage thresholds? | Sweep thresholds; end-to-end edit-sim on mixed corpora. | Thresholds frozen; invariant verified: no born-digital page routes to OCR. |
| **E4 — Table bake-off** | TATR or SLANet/RapidTable? | Both, with text-snap fusion, on FinTabNet.c/PubTabNet: TEDS/GriTS vs latency. | Pick default; TEDS target set from E0 competitor numbers. |
| **E5 — Reading order** | Is XY-cut over blocks enough? | XY-cut++ vs current heuristics vs competitor orders: ARD/Kendall-τ on OmniDocBench. | If within ε of Marker/MinerU order quality → no learned model. Else open a LayoutReader spike. |
| **E6 — End-to-end vs the field** | Did the rebuild work? | Full pipeline vs E0 baselines on OmniDocBench + olmOCR-Bench + Internal-50. | Evidence for the M4/M5 gates and §9 success criteria. |

---

## 7. Milestones

| Milestone | Content | Exit criteria (frozen suite) |
|---|---|---|
| **M0 — Measure** | Rebuild `eval/harness/`; wire all five metric families to real GT; frozen suite + smoke slice + `eval diff` + visual diffs; **E0**: baseline current dongler AND Marker/MinerU/Docling; start Internal-50 GT | Harness reproduces competitor published numbers within tolerance; CI smoke green; "are we actually bad, and where" answered per-category in the ledger |
| **M1 — Skeleton** | New package layout; triage + rasterize + TextProvider; IR v2; fast path preserved | Born-digital end-to-end ≥ current dongler on every metric (no regression from re-plumbing); triage ≥99% on a labeled page sample |
| **M2 — Layout + order** | Layout model + fusion + XY-cut (born-digital) | OmniDocBench ARD and olmOCR multi-column pass rate beat M0-dongler by the E5 margin; no-text-loss invariant holds suite-wide |
| **M3 — OCR** | Scanned + hybrid routes | olmOCR scanned categories jump; Internal-50 scanned docs produce usable markdown (rubric) |
| **M4 — Tables** | Table model + HTML table rendering | E4 TEDS/GriTS gate; olmOCR table pass rate at target vs competitors |
| **M5 — Ship** | Model download UX, CPU latency tuning, docs, PyPI release; formula stretch-goal decision | §9 criteria met; install-to-first-convert <5 min on a clean machine |

---

## 8. Risks

| Risk | Mitigation |
|---|---|
| **Model licenses** (Surya modified-GPL/revenue-cap; DocLayout-YOLO AGPL; PaddleOCR-lineage weight provenance) | Permissive defaults only; restricted models as opt-in extras, never bundled; license recorded per model in `models/registry.py`; verification is an explicit step inside E1/E2/E4 |
| **CPU latency** (full pipeline) | Provisional target ≥1–2 pages/s CPU (re-based in E-phase); fast path keeps ~90 pages/s; levers: raster DPI, per-region batching, ONNX quantized weights |
| **Weight-download UX** vs the "zero downloads" brand | Fast path stays zero-download; clear first-run messaging; `dongler models download` offline bundle |
| **Wheel matrix weight** (onnxruntime + pypdfium2) | Both ship manylinux/mac/win wheels; pin minimal versions; ML deps in an optional extra `dongler[hybrid]` |
| **Eval-only data leakage** | License gate in downloader; eval-only data never committed; headline numbers note tier |
| **Scope creep** (formulas, KIE, VLMs) | §10 non-goals; formula recognition is a single explicitly-gated stretch decision at M5 |

---

## 9. Success criteria

Numeric targets are **provisional until E0 re-bases them against measured competitor numbers**; the re-based versions get committed to the ledger and become the contract.

- olmOCR-Bench overall **≥60%** (from 22.7%) and **within 10 points of the best measured open competitor** on our harness.
- OmniDocBench overall edit-similarity **within 5 points of Marker/MinerU** as measured by *our* harness.
- TEDS **≥0.85** on the FinTabNet.c frozen slice.
- **Zero per-doc regressions** vs current dongler on born-digital Internal-50 documents.
- Hybrid pipeline **≥1 page/s on CPU**; fast path unchanged (~90 pages/s).
- Every claim above reproducible by `eval run --suite frozen-v1` from a clean checkout.

---

## 10. Non-goals (v2)

- No model training or fine-tuning.
- No LLM/VLM calls (a local-VLM fallback for hard pages is a possible v3 — out of scope now).
- No handwriting recognition.
- No key-information extraction / DocVQA.
- No Node/WASM support for the ML pipeline (fast path only).
- No PDF editing/generation; no hosted service.
- LaTeX renderer maintained, not advanced.

---

## 11. Appendices

### A. Salvaged assets (by path)

| Asset | Path | Disposition |
|---|---|---|
| IR schema v1 | `crates/dongler-core/src/ir.rs` | Extend to v2 (§3.I); v1 stays deserializable |
| Metric library | `scripts/eval_metrics/` | Move to `eval/metrics/` unchanged; wire in M0 |
| Rust PDF parser | `crates/dongler-core/src/pdf.rs` | Default `TextProvider`, subject to E0; heuristic detectors demoted |
| Renderers | `crates/dongler-core/src/render.rs` | Keep; add HTML-table emission for `TableBlock.html` |
| License research | `IMPROVEMENT_PLAN.md` §2, §6 | Carried into §2.2/§5 here |
| Dataset manifest | `eval/datasets/document-benchmarks-v2.json` | Basis for frozen-suite-v1 |
| Visual compare script | `scripts/render-extraction-comparison.py` | Evolves into `eval/harness/visual.py` |
| SEC downloader | `scripts/download-sec-10k.py` | Feeds Internal-50 |

### B. Model registry (initial; licenses to verify in E-phase)

| Task | Default | License | Runtime | Status |
|---|---|---|---|---|
| Layout | Docling layout (RT-DETR lineage) | MIT code; **verify weights (E2)** | ONNX | candidate |
| Layout alt | PP-DocLayout | Apache-2.0 | paddle2onnx → ONNX | candidate |
| OCR | RapidOCR (PP-OCR lineage) | Apache-2.0 code; **verify weights (E1)** | ONNX | candidate |
| OCR floor | Tesseract | Apache-2.0 | native | baseline only |
| Tables | TATR (table-transformer) | MIT code; **verify weights (E4)** | PyTorch/ONNX | candidate |
| Tables alt | SLANet / RapidTable | Apache-2.0 | ONNX | candidate |
| Restricted (opt-in only, never default) | Surya layout/OCR, DocLayout-YOLO | modified-GPL / AGPL | — | reference ceiling |

### C. Metric glossary

- **CER/WER** — character/word error rate vs GT text (`text.py`).
- **Edit similarity** — normalized Levenshtein on markdown (`text.py`).
- **TEDS / TEDS-struct** — tree edit distance similarity on table HTML, with/without cell content (`table.py`).
- **GriTS top/con/loc** — table grid similarity: topology, content, location (`grits.py`).
- **Layout mAP / mean-best-IoU** — COCO-style detection quality on block boxes (`layout.py`).
- **ARD / Kendall-τ** — reading-order: absolute rank distance and rank correlation (`order.py`).
- **olmOCR unit pass rate** — binary per-check assertions (presence, order, table cells, math), by category.

### D. Operating rules (how we avoid rabbit holes, summarized)

1. M0 before any extraction work. No exceptions.
2. Every extraction PR carries an `eval diff`. Aggregates AND per-doc regressions.
3. Bake-offs decide model choices; gates decide phase transitions; the ledger records both.
4. Look at visual diffs at every gate — numbers alone got us here.
5. Time-box experiments. If a gate isn't met in its box, escalate the decision rather than grinding.
