# Dongler v2 — Rust-Native Hybrid PDF-to-Markdown Extraction: Master Plan (PRD)

| | |
|---|---|
| **Status** | ACCEPTED — implementation plan of record (rev 2, Rust-native) |
| **Date** | 2026-06-12 |
| **Supersedes** | `IMPROVEMENT_PLAN.md` (its §2 license research and §4.9 metric work carry forward) and rev 1 of this document |
| **Research basis** | All model/crate/license/leaderboard claims in this document were verified against live sources on 2026-06-12; sources in §13.4. Items still needing verification are marked **[verify in E-phase]**. |

**Thesis.** Dongler's quality plateaued for two reasons: (a) the heuristic-only architecture has a hard ceiling on layout, tables, and scanned documents, and (b) the eval harness measured the wrong things, so effort went into rabbit holes that "improved" numbers without improving output. This plan fixes **measurement first**, then rebuilds extraction as a **Rust-native hybrid pipeline**: deterministic born-digital text extraction fused with pretrained local ONNX models for layout, tables, and OCR — all orchestrated in Rust via `ort`, with no Python in the product. A small-VLM escalation hook (the architecture the 2026 leaders converged on) is designed in from day one as an optional, gated route.

**Locked decisions** (do not re-litigate):

1. **Approach: hybrid pipeline.** Deterministic text layer + pretrained models. No model training. No pure-heuristics path.
2. **Scope: born-digital AND scanned PDFs.**
3. **Runtime: Rust end-to-end.** The pipeline, including ML inference, runs in Rust (`ort`/ONNX Runtime). Python and Node are thin bindings over the same Rust core — which means, unlike every Python competitor, the *full* pipeline ships to all binding targets. CPU must work; GPU accelerates. No required API calls. Model weights are downloaded once and cached; the born-digital fast path stays zero-download.
4. **Clean slate with salvage.** Fresh workspace layout; salvage `crates/dongler-core/src/ir.rs` (evolved to v2), `scripts/eval_metrics/` (the eval harness remains Python — it is tooling, not product), and the Rust parser as a text provider that must *earn* its place via E0.
5. **Output: Markdown with embedded HTML tables** (preserves rowspan/colspan; what MinerU/Marker emit). GFM pipe tables as a lossy opt-in. JSON IR always available.
6. **License floor: MIT/Apache-2.0 end-to-end** — code *and* model weights. Restricted models (AGPL, OpenRAIL revenue-capped, CC-BY-SA) are reference ceilings in evals or opt-in plugins, never defaults. This is the mistake MinerU had to walk back (it dropped DocLayout-YOLO over AGPL); we avoid it from the start.

A note on "Rust for speed", stated honestly so we don't fool ourselves: neural-net inference speed is determined by ONNX Runtime and the model, not the host language — a Python pipeline calling the same ONNX graphs runs them at the same speed. What Rust buys is real but different: the deterministic 90-pages/s fast path stays Rust-fast; pre/post-processing (rasterize, resize, NMS, fusion geometry) is native instead of NumPy-bound; the product is a single binary with no Python runtime, venv, or wheel matrix; and one core serves CLI, Python, Node, and (fast path) WASM. Those are the wins we claim.

---

## 1. Problem statement and post-mortem

Why did months of work not move the needle? Three repo-verifiable causes:

1. **Measurement was broken.** `scripts/run-benchmarks.py` only ever wired three metric families: token-F1 (bag-of-words overlap — insensitive to structure, order, and tables), olmOCR unit pass rate, and a degenerate `full_image_iou` that scores each block against the *whole page rectangle*. The real metrics — TEDS, GriTS, CER/WER, reading-order ARD/Kendall-τ — were implemented in `scripts/eval_metrics/` (dependency-free, unit-tested) but **never connected to dataset ground truth**. The "100.0% GT accuracy" rows in the README (TableBank, SROIE) measured parse success, not fidelity. The only number that resembles reality is olmOCR-Bench overall: **22.7%** (1,595/7,019).
2. **The heuristics hit their ceiling.** Reading order is hard-coded for ≤2 columns. Table detection requires a literal "Table" caption or a ≥4×2 ruled grid. Math passes 1.6% of olmOCR checks. Scanned pages are out of scope entirely. Each additional heuristic (financial tables, columnar rescue, leader-gap rows — see `git log` v0.3.10–v0.3.12) bought a point or two on one dataset while adding brittle special cases.
3. **The field's architecture question is settled and we weren't using the answer.** Layout understanding requires a vision model; text fidelity for born-digital pages requires the text layer, not OCR; the fusion of the two is the secret sauce. §2 documents where the field actually is in mid-2026.

**The rule going forward: no extraction change merges without an eval diff on the frozen suite.** If we can't see a change in the numbers, we don't ship it.

---

## 2. State of the field, June 2026 (research summary)

What we verified, and what it means for this plan.

### 2.1 Leaderboards

**OmniDocBench v1.6** (1,947 pages; overall = ((1−text edit)×100 + table TEDS + formula CDM)/3; scores not comparable to v1.5):

| System | Type | Overall ↑ | Text edit ↓ | Table TEDS ↑ |
|---|---|---|---|---|
| MinerU2.5-Pro (1.2B) | specialized VLM | 95.75 | 0.036 | 93.42 |
| GLM-OCR (0.9B) | specialized VLM | 95.22 | 0.044 | 92.83 |
| PaddleOCR-VL-1.5 (0.9B) | specialized VLM | 94.93 | 0.038 | 91.67 |
| Gemini 3 Pro | general VLM | 92.91 | 0.064 | 89.15 |
| **MinerU-Pipeline** | **modular pipeline** | **85.75** | 0.063 | 80.43 |
| olmOCR (7B) | VLM | 85.74 | 0.139 | 83.00 |
| **Marker** | **modular pipeline** | **78.44** | 0.157 | 65.77 |

**olmOCR-Bench** (unit checks; † = vendor-reported, not reproduced by Ai2): Chandra 2 claims 85.9†, Chandra 0.1.0 83.1†, olmOCR-2 82.4 (reproduced), PaddleOCR-VL 80.0†, **Marker 1.10.1 76.1**, DeepSeek-OCR 75.7, MinerU 2.5.4 75.2†, Mistral OCR API 72.0. Note: Marker — a pipeline — beats MinerU's own VLM here. "Old scans" is everyone's worst category (33–50).

### 2.2 The five lessons that shape this plan

1. **The frontier is small (0.9–1.2B) document-specialized VLMs, but the winning ones are internally two-stage**: layout analysis on a downsampled page, then content recognition on native-resolution crops (MinerU2.5, PaddleOCR-VL, GLM-OCR all work this way). "Layout first, then recognize crops" survived the VLM transition — it is the architecture, independent of implementation. Our pipeline keeps that skeleton with swappable recognizers.
2. **Single-pass full-page VLM decoding is the riskiest design**: repetition loops and silent omissions are its documented production failure modes. Crop-level decoding bounds the blast radius. If we ever escalate to a VLM, it is per-region, never per-page.
3. **A well-engineered modular pipeline lands in the 76–86 band** (Marker, MinerU-Pipeline) — ~10–17 points behind frontier VLMs on OmniDocBench, concentrated in formulas, complex tables, and degraded scans. On clean born-digital PDFs the gap nearly vanishes because text comes from the text layer. For a CPU-first engine this band is production-credible and the honest target.
4. **Pipelines cannot hallucinate — and evals now reward that.** VLM parsers fabricate plausible text on degraded scans where classical OCR fails legibly (documented in academic stress tests and the new SCORE-Bench's spurious-token metrics). Determinism is a marketable property; we preserve it, and any VLM output must pass machine validators (balanced tables, no n-gram loops) before acceptance, with deterministic fallback on failure.
5. **Both headline benchmarks are near-saturated and have documented flaws** — olmOCR-Bench has label errors (~96–97% estimated ceiling) and skews academic (~56% arXiv/textbook, English-only, no forms/invoices); OmniDocBench's edit distance penalizes semantically equivalent formatting, and an independent audit (PureDocBench, May 2026) found **2,580 annotation errors (12.08%)** in OmniDocBench's scored blocks; OmniDocBench is maintained by the same group that builds the top-ranked MinerU. Consequences for us: report per-category numbers (never just overall), use multiple benchmarks plus our own Internal-50, and treat top-of-leaderboard deltas <2 points as noise. Newer robustness/source-traceable benchmarks (PureDocBench, Real5-OmniDocBench, CC-OCR v2, OHR-Bench) exist specifically because the headline two are saturated — we adopt several as secondary signals (§3.2).

### 2.3 What this means for dongler

Dongler's modular pipeline will not reach 95 on OmniDocBench — nothing without a VLM does. The credible play, validated by where MinerU 3.x itself landed (a "hybrid backend" with escalation): **a deterministic, hallucination-free, CPU-first Rust pipeline targeting the Marker/MinerU-Pipeline band (76–86), with a per-region small-VLM escalation hook for the hard 10%** (low-confidence tables, formulas, degraded scans), local and optional. Granite-Docling-258M (Apache-2.0) exists precisely for that slot, and its official 2-stage variant is prompted with layout regions from the *same* docling-layout-heron model we adopt in Stage D — the integration path is paved.

---

## 3. Eval-first methodology (the #1 fix)

This section is the contract that prevents future rabbit holes. The harness gets rebuilt **before** any extraction work (M0). The harness and metrics remain Python (tooling, not product); the product under test is the Rust CLI's JSON/markdown output.

### 3.1 The frozen benchmark suite

- A frozen test set defined in `eval/datasets/frozen-suite-v1.json`: pinned dataset revisions (HF commit hashes), pinned document lists (explicit file IDs, not globs), pinned metric-library version. Any change bumps the version and invalidates cross-version comparisons.
- **Dev/test split discipline.** Every dataset gets a *dev slice* (tune freely, run constantly) and a *frozen test slice* (scored only at milestone gates; results appended to an immutable `eval/results/ledger.jsonl`). Budget: ≤3 frozen-suite runs per milestone.
- **Baseline-before-change rule.** Every PR touching extraction includes `eval diff` output (per-document deltas) in its description.

### 3.2 Metric–dataset matrix

All metrics already exist in `scripts/eval_metrics/` (`text.py`, `table.py`, `grits.py`, `layout.py`, `order.py`); M0 wires them to ground truth.

| Dataset | License tier | Role | Metrics |
|---|---|---|---|
| **OmniDocBench v1.6** (1,947 pages) | eval-only | **Headline end-to-end**: per-element markdown GT, table HTML, reading order, layout boxes | edit-similarity/CER per element type, TEDS, ARD + Kendall-τ, layout mAP |
| **olmOCR-Bench** (1,403 PDFs, 7,019 checks) | ODC-BY (permissive) | **Headline unit checks** incl. scanned, math, multi-column; per-category breakdown mandatory; known label errors → treat 96–97 as ceiling, deltas <1.5 pts as noise | unit pass rate by category |
| **READoc** | MIT | Markdown structure (headings/lists) on arXiv/GitHub docs | edit-sim, BLEU, heading-structure score |
| **FinTabNet.c** | CDLA-Permissive-2.0 | Table dev + frozen test | TEDS, TEDS-struct, GriTS top/con/loc |
| **PubTabNet** (subset) | CDLA-Permissive-1.0 | Table dev (large, cheap slices) | TEDS |
| **DocLayNet** (capped slice) | CDLA-Permissive-1.0 | Layout model dev + frozen test | COCO mAP, mean-best-IoU |
| **OHR-Bench** (ICCV 2025) | **[verify license in M0]** | Secondary: downstream RAG damage from parsing errors — catches "looks fine, retrieves badly" | its own QA-degradation protocol |
| **Real5-OmniDocBench** (PaddlePaddle, 2026) | **[verify]** | Secondary: robustness to capture degradation (scan/warp/screen-photo/illumination/skew) — directly relevant to the scanned route | OmniDocBench metrics, identical GT |
| **PureDocBench** (2026) | **[verify]** | Secondary: source-traceable GT (generated from HTML/CSS), three degradation conditions; cleaner annotations than OmniDocBench | edit-sim, TEDS, CDM |
| **Internal-50** (§6) | owned | **The reality check.** Fully held out from all tuning | edit-sim vs hand-checked markdown + manual rubric |

Plus one metric family the legacy harness never had, now table stakes (SCORE-Bench's argument): **hallucination/spurious-token rate and token coverage** — fraction of output tokens not attributable to the source, and fraction of source text present in output. Cheap to compute for a pipeline (we have provenance for every span), and it operationalizes the "no text invented, no text dropped" invariants.

The 2026 consensus is that no single string metric is trustworthy for representation-ambiguous content: edit distance punishes equally-correct formatting, TEDS is HTML-conversion-sensitive, and CDM correlates only r≈0.34 with human formula judgment vs r≈0.78 for an LLM judge. We therefore treat string metrics as the fast regression signal and reserve an **optional LLM/VLM-as-judge pass** (DOCR-Inspector-style, gated, run only at milestone gates on dev slices) for adjudicating formula/table semantic equivalence when a string metric flags a regression that visual inspection suggests is a false positive. This is a checker, never a tuning target.

License gating carries over from `eval/datasets/document-benchmarks-v2.json` (`license_class`: `permissive` / `eval_only` / `unverified`; downloader `--allow` flag). Eval-only data never ships in the repo; published headline numbers always note the tier.

### 3.3 Anti-Goodhart rules

1. **No dataset-conditional code paths.** No dataset name appears outside `eval/` (grep-able review rule).
2. **Per-document regression counts** reported alongside aggregates. A change that raises the mean but regresses >5% of documents needs written justification.
3. **Two uncorrelated metric families minimum** for a merge: e.g. text edit-sim AND olmOCR units AND TEDS all non-regressing.
4. **Internal-50 is never tuned against.** OmniDocBench frozen slice scored only at milestone gates.
5. **Throughput is part of the suite** (pages/sec on a fixed fixture per route), so quality wins can't silently cost 10× latency.
6. **Saturation awareness**: improvements claimed near a benchmark's known ceiling (olmOCR ≥90 in any category) require visual-diff evidence, not just the number.

### 3.4 CI smoke slice

~25 documents in `eval/smoke/`, license-clean only (olmOCR ODC-BY + READoc MIT + PubTabNet CDLA + owned docs), <5 min on CPU including model inference. CI fails on any per-doc metric drop beyond a noise epsilon; artifacts stored per run.

### 3.5 Artifacts and dashboards

- Every run writes `eval/out/runs/<run_id>/`: per-doc JSON scores, aggregate JSON, markdown report.
- `eval diff <baseline> <candidate>`: improved/regressed/unchanged doc lists, top-10 worst regressions linked to visual diffs.
- **Visual side-by-side** (evolve `scripts/render-extraction-comparison.py`): one HTML page per doc showing page raster | layout-box overlay | rendered markdown | GT markdown. Looking at output is mandatory at every gate; aggregate numbers alone are how we got here.

---

## 4. Architecture: the Rust-native hybrid pipeline

One Rust workspace. The pipeline crate (`dongler-pipeline`) orchestrates all stages; `dongler-core` (salvaged) provides parsing/IR/rendering; `ort` provides inference. Python/Node bindings wrap the pipeline crate, so every binding gets the full hybrid pipeline — a structural advantage no Python competitor has.

```
PDF ─► A. Triage (per page: born_digital | scanned | hybrid)
        ├─► B. Rasterize           pdfium-render @ 150–200 DPI (hi-DPI crops for OCR/tables)
        ├─► C. Text layer          dongler-core (default) / pdfium text (fallback)   [born_digital]
        ├─► D. Layout detection    docling-layout-heron ONNX via ort (every page) ─► typed regions
        ├─► E. OCR                 PP-OCRv5 ONNX via ort, per region                 [scanned/hybrid]
        ├─► F. Table structure     SLANet-plus ONNX via ort ─► grid topology only
        ├─► G. Fusion              snap region text from text layer or OCR; nothing dropped
        ├─► H. Reading order       recursive XY-Cut++ over typed blocks
        ├─► (X. VLM escalation)    optional, per-region, gated by validators — see 4.X
        └─► I. Render              IR v2 ─► Markdown (+HTML tables) / JSON / LaTeX
```

### 4.A Triage / router (per page)

Classify each page using the text layer (cheap) and page imagery:

- `born_digital`: text-layer characters cover ≥ ~85% of inked area (threshold tuned in E3).
- `scanned`: no/negligible text layer, or a single image covers >90% of the page.
- `hybrid`: image-heavy with a partial text layer — including scans with embedded invisible OCR (text render mode 3 / text-over-image overlap). Probe the embedded OCR's quality (dictionary-word rate, glyph-coverage agreement) and either trust it or discard and re-OCR.

Route recorded per page in IR provenance. **Invariant: a born-digital page never routes to OCR** (regression-tested in E3). Prior art to study (MIT, reusable): `firecrawl/pdf-inspector` (Feb 2026) does exactly this classification — TextBased/Scanned/ImageBased/Mixed with confidence and per-page OCR-routing recommendations, in pure Rust on `lopdf` — and validates that a cheap, no-ML triage stage is both feasible and highly valued.

### 4.B Rasterization — `pdfium-render`

`pdfium-render` 0.9.x (MIT OR Apache-2.0) over Google's pdfium (BSD-3/Apache-2.0): battle-hardened via Chrome, prebuilt binaries continuously published (bblanchon/pdfium-binaries, latest June 2026), dynamic *or* static linking, and the de facto choice of existing Rust document pipelines (ferrules, seekstorm, multiple converters). Render at 150–200 DPI for layout detection; re-crop at higher DPI for OCR and table regions. MuPDF is excluded (AGPL). Pure-Rust `hayro` (MIT/Apache, by the Typst ecosystem) is tracked as a future replacement once its structured-text extraction lands — not load-bearing now.

### 4.C Born-digital text source — `dongler-core`, but it must earn it

- **For keeping it:** MIT and ours; emits spans with font/size/bold/italic + bboxes + source anchors (`ir.rs::Span`), richer than pdfium's char-level text API; the word-segmentation/ligature/CIDFont work of Phases 0–1 is real measured progress (olmOCR tables 59.7→65.5%); zero extra dependency on the fast path.
- **Against:** 4,350 LOC of custom parser; pdfium is more robust on malformed files; sunk cost is not an argument.
- **Resolution:** a `TextProvider` trait with two implementations: `dongler-core` (default) and `pdfium-text` (via the same pdfium-render dependency we already carry for rasterization — zero extra cost). **E0 runs both head-to-head on per-page CER (OmniDocBench/ckorzen text GT). If dongler-core loses by >1 CER point on >10% of pages, the default flips.** Either way pdfium-text is the automatic fallback when dongler-core errors. The parser earns its place with data.
- **Demoted from the salvaged core** (superseded by stages D–F/H, kept only for the zero-model fast path): heuristic table detectors, 2-column reading-order detectors, heading classifier. **Kept:** parsing, font decoding, spans, geometry, rotation, renderers.

### 4.D Layout detection — `docling-layout-heron` ONNX via `ort`

Detect typed regions on every page raster: `title, section_header, text, list_item, table, picture, caption, formula, page_header, page_footer, footnote` (heron's label set maps 1:1 onto IR v2 block kinds).

- **Default: docling-layout-heron** — MIT code (docling-ibm-models), **Apache-2.0 weights, official ONNX export published by the Docling project** (`docling-project/docling-layout-heron-onnx`), RT-DETRv2 architecture → **NMS-free** (set prediction; no postprocessing minefield), ~78% mAP class (heron-101), 28 ms/img on A100. The cleanest license + packaging combination that exists, and it doubles as the layout prompt source for the Granite-Docling 2-stage VLM if E7 lands.
- **Bake-off contender (E2):** PP-DocLayout_plus-L (Apache-2.0 weights; ONNX via paddle2onnx; also shipped as ONNX by RapidAI/RapidLayout **[verify exact artifact provenance in E2]**). PP-DocLayoutV3 exists with community ONNX **[verify]**.
- **Excluded as default:** DocLayout-YOLO — repo is AGPL-3.0, HF weights tagged Apache-2.0, contradiction raised in its issue #110 and unanswered; legally ambiguous. Surya layout — OpenRAIL with $5M revenue cap. Both usable as reference ceilings in E2 evals only.
- Inference pattern (standard across usls/oar-ocr/ferrules): `image` decode → `fast_image_resize` (SIMD) → f32 normalize → NCHW `ndarray` → `ort::Value`. RT-DETR needs only /255 normalization and no NMS.

### 4.E OCR — PP-OCRv5 ONNX via `ort` (scanned/hybrid pages, per region)

- **Default: PP-OCRv5** detection + recognition, **Apache-2.0 weights confirmed on the official PaddlePaddle HF cards**, mobile variants for CPU, server variants for GPU; top NED among modular OCR in OmniDocBench module evals. **PP-OCRv6 shipped 2026-06-11** (+4.6% det / +5.1% rec over v5_server, 5.2× CPU speedup via OpenVINO, one model covers ZH+EN+JA+46 Latin langs) — adopt it the moment a vetted ONNX export lands in RapidOCR/oar-ocr; until then v5 is the proven-in-Rust default. E1 re-checks v6 ONNX availability.
- **Implementation route:** the **`oar-ocr` crate (Apache-2.0, v0.7.0 released 2026-06-11, built on `ort`)** already implements PP-OCRv5 det/rec, document orientation, rectification — plus PP-DocLayout and SLANet-plus. Decision for E1: depend on it vs. vendor the relevant pipelines into `dongler-pipeline` (it's image-input focused, no PDF awareness — our fusion/IR layer is the differentiator either way).
- **Contenders (E1):** `ocrs` (pure-Rust via RTen, MIT/Apache code — but Latin-only and **CC-BY-SA-4.0 weights**: share-alike, kept opt-in only); Tesseract via `rusty-tesseract` (baseline floor); Surya OCR (reference ceiling only, revenue-capped license).
- OCR runs **per layout region**, not per page (accuracy + speed), with an orientation/deskew probe first. Recognition output keeps per-line confidence for the escalation gate (4.X).

### 4.F Table structure — SLANet-plus ONNX via `ort`

- **Default: SLANet-plus** (Apache-2.0; ONNX artifacts via RapidAI/RapidTable, already proven in Rust by `oar-ocr`; RapidTable led OmniDocBench's module-level table eval). **Contender (E4):** TATR / microsoft/table-transformer (MIT code **and** MIT weights confirmed), DETR-family (NMS-free) but with documented ONNX-export friction and staler results.
- **Critical design rule (the Docling trick):** the model predicts **grid topology only** — rows, columns, spanning cells. Cell *content* is snapped from the deterministic text layer (born-digital) or region OCR (scanned) by bbox intersection. The table model never transcribes text. This single rule is why pipeline tables don't hallucinate values — non-negotiable.

### 4.G Fusion (the secret sauce)

Algorithm, not vibe. All in Rust, operating on IR-space geometry:

```
inputs: regions[] from Stage D (raster px), spans[] from Stage C (PDF user space),
        page route from Stage A, render matrix M
1. regions ← M⁻¹(regions)                       # into PDF user space
2. for each span: owner ← the highest-priority region whose bbox contains
   span.center (priority: table > figure > formula > text classes;
   ties → smallest region). Each span has exactly one owner.
3. for each region on born_digital page:
     text ← owned spans, grouped into lines by baseline, ordered x-then-y
   for each region on scanned page:
     text ← OCR(region crop @ high DPI)
   for each region on hybrid page:
     coverage ← owned-span char coverage of region area
     text ← coverage ≥ τ ? spans : OCR(crop)     # τ tuned in E3
4. orphan spans (no owner): attach to nearest region within ε, else emit
   as fallback text block.       # HARD INVARIANT: no span silently dropped
5. page_header/page_footer/footnote: model class + repeated-across-pages
   check; kept in IR, excluded from default markdown (olmOCR convention).
6. every region → IR block with provenance:
   { text_source: text_layer|ocr|vlm, detector: model@version, confidence }
```

The no-drop invariant is tested suite-wide: `sum(chars in IR) ≥ sum(chars in text layer) − ε` per page, plus the token-coverage metric from §3.2 at eval time. Span-to-region assignment uses an R*-tree (`rstar` crate) over region bboxes so the per-span owner lookup is O(log n), not O(spans × regions) — pages with thousands of spans stay sub-millisecond.

### 4.H Reading order — XY-Cut++ in Rust

Recursive XY-cut with the XY-Cut++ refinements (pre-mask cross-layout elements like full-width headers/tables, then density-adaptive cut direction), applied to the ~10–30 typed blocks — not raw lines. Validated by the field: XY-Cut++ reports 0.953 BLEU-4 vs MinerU's 0.926 on its benchmark, and MinerU itself *retreated* from the learned LayoutReader model (CC-BY-NC weights) to algorithmic ordering. The only existing Rust implementation is GPL-3.0 and 639 LOC, so we write our own (MIT, ~ a few hundred LOC, property-tested). If E5 shows XY-Cut++ materially behind competitor ordering on OmniDocBench ARD/Kendall-τ — unlikely given the field's retreat — the permissive learned fallback is **PP-DocLayoutV2's pointer-network head (Apache-2.0)**, which predicts reading order jointly with detection and is what PaddleOCR-VL uses; no non-commercial LayoutReader weights enter the codebase.

### 4.X VLM escalation hook (optional route, designed in from day one)

The deterministic pipeline is the product; this is its quality ceiling raiser for the hard 10%.

- **Trigger:** per-*region* only (never full pages — lesson 2 in §2.2): table regions whose structure confidence < threshold, formula regions, OCR regions with low recognition confidence on `scanned` routes.
- **Candidate model: Granite-Docling-258M** (Apache-2.0, SigLIP2 + Granite-165M, emits DocTags — typed, bbox-grounded markup that maps directly onto IR v2; TEDS-content 0.96 on tables). Its official **2-stage variant is prompted with RT-DETR layout regions from docling-layout-heron — the exact model we already run in Stage D**, making dongler's integration the natural one. Inference options: llama.cpp GGUF (~400–500 tok/s on consumer GPUs, ~3–6 s/page; CPU realistically 0.5–3 min/page — batch-viable only), ONNX (community export, ~0.8 s/page desktop **[verify]**), candle. Rust binding route (llama-cpp FFI vs ort-on-ONNX vs candle) is decided in E7. Bigger options if a GPU exists (all permissive): PaddleOCR-VL-1.5 0.9B (Apache-2.0), GLM-OCR 0.9B (MIT), LightOnOCR-2-1B (Apache-2.0, official GGUF).
- **Acceptance gate (non-negotiable):** VLM output is validated mechanically — balanced/parseable table HTML, no n-gram repetition loops, token-coverage sanity vs the region's OCR/text-layer reading. Fail → keep the deterministic result. The pipeline's "cannot hallucinate" property survives because unvalidated VLM text never enters the IR. Provenance marks `text_source: vlm` so downstream users can filter.
- **Decision points:** E7 is time-boxed and runs only after M4. If validated escalation doesn't move frozen-suite table/formula/scan metrics by its pre-registered margin, the hook stays dormant (a feature flag, zero deps pulled in by default — `dongler` installs nothing VLM-related unless the `vlm` feature/extra is chosen).

### 4.I Rendering — IR v2 → Markdown

- Markdown with embedded HTML tables by default; `--tables=pipe` lossy option. Math as `$…$`/`$$…$$` where a LaTeX source exists (text layer or validated VLM); otherwise formula regions emit a placeholder image reference. Formula *recognition* (PP-FormulaNet, Apache-2.0) is an explicitly gated M5 stretch goal.
- **IR evolves to `dongler.ir.v2`** (extend `crates/dongler-core/src/ir.rs`; v1 stays deserializable): closed block-kind enum (`heading_1..6, paragraph, list_item, code, formula, caption, page_header, page_footer, footnote, table, figure`), per-block provenance struct (`text_source`, `detector`, `confidence`), per-page `route`, `TableBlock.html: Option<String>` alongside existing `cells` (already carrying `col_span`/`row_span`). Schema sketch in §13.1.
- Rendering stays in `render.rs` (Rust), shared by every consumer.

### 4.J Process/distribution architecture

- **Crates:** `dongler-core` (parse/IR/render, zero ML deps — the WASM/fast-path crate), `dongler-pipeline` (stages A–I, depends on core + ort + pdfium-render), `dongler-cli`, `dongler-python` (PyO3), `dongler-node` (NAPI), `dongler-wasm` (fast path only).
- **Two public modes everywhere:** `load()` — fast path, zero downloads, today's ~90 pages/s behavior; `convert()` — full pipeline. Python: `pip install dongler` (fast path) / `dongler[pipeline]` extra pulls nothing extra at all — the pipeline is compiled in; only *model weights* download on first `convert()`, to `~/.cache/dongler/models`, sha256-pinned via the model registry, with `dongler models download` for offline prefetch and a documented fully-offline workflow.
- **ort linking:** `download-binaries` default (GitHub-attested since rc.12); `load-dynamic` escape hatch for exotic platforms. Pin the exact rc (`=2.0.0-rc.12`) — rc-to-rc API churn is real. Execution providers: CPU default; `cuda`/`coreml`/`directml` behind cargo features.
- **WASM:** fast path only in v2. (A future pure-Rust inference path exists via `rten`/ort alternative backends, but it is out of scope — noted in §11 non-goals.)

---

## 5. Technology stack (verified 2026-06-12)

### 5.1 Crates

| Crate | Version | License | Role | Notes |
|---|---|---|---|---|
| `ort` | =2.0.0-rc.12 | MIT/Apache-2.0 | ONNX inference | binds ONNX Runtime 1.17–1.24; 11M downloads; used by HF TEI, Magika, oar-ocr. Pin exact rc. |
| `pdfium-render` | 0.9.x | MIT/Apache-2.0 | raster + fallback text | pdfium itself BSD-3/Apache-2.0; binaries from bblanchon (June 2026 current); static or dynamic link |
| `oar-ocr` | 0.7.x | Apache-2.0 | PP-OCRv5 / layout / SLANet+ pipelines on ort | depend-vs-vendor decided in E1; very active (released 2026-06-11) |
| `image` | 0.25.x | MIT/Apache-2.0 | decode/encode | |
| `fast_image_resize` | 6.x | MIT/Apache-2.0 | SIMD resize | the standard pairing with ort |
| `ndarray` | 0.17.x | MIT/Apache-2.0 | tensor staging | |
| `imageproc` | 0.27.x | MIT | deskew/morphology | orientation probe |
| `rstar` | 0.13.x | MIT/Apache-2.0 | R*-tree for fusion span↔region queries | GeoRust, active |
| `hf-hub` | 0.5.x | Apache-2.0 | model-weight download/cache | `HF_HOME`/`~/.cache`; sha256 alongside registry |
| `rayon`, `serde`, `thiserror` | current | — | carried over from v1 | |

Rejected: `mupdf` (AGPL), `poppler-rs` (GPL link), `wonnx` (archived 2025), `tract`/`candle` as primary runtime (tract = embedded niche, ~85% opset, viable WASM fallback only; candle-onnx immature for arbitrary graphs; `burn-onnx` lacks NonMaxSuppression — moot since our defaults are NMS-free RT-DETR), `usls` (MIT, kept as a reference for RT-DETR pre/postprocessing code patterns), `kreuzberg` v4 (Elastic-2.0, architecture reference only), `ferrules` (GPL-3.0, architecture reference only).

### 5.2 Model registry (`dongler-pipeline/src/models/registry.rs`)

Every model: name, version, sha256, source URL, code license, **weight license**, input spec. Initial registry:

| Slot | Default | Weights | ONNX status | Contenders (E-phase) |
|---|---|---|---|---|
| Layout | docling-layout-heron | **Apache-2.0** (verified on HF card) | **official ONNX** (docling-project org) | PP-DocLayout_plus-L (Apache-2.0, paddle2onnx); heron-101 |
| OCR det+rec | PP-OCRv5 mobile (CPU) / server (GPU); → PP-OCRv6 when ONNX lands | **Apache-2.0** (verified on official HF cards) | official Paddle → ONNX, shipped in oar-ocr | ocrs (CC-BY-SA weights, opt-in), Tesseract (floor) |
| Table structure | SLANet-plus | **Apache-2.0** | ONNX via RapidTable assets **[verify provenance in E4]** | TATR (MIT weights, export friction) |
| Reading order | XY-Cut++ (our Rust impl) | n/a — no model | n/a | learned model only if E5 fails |
| Formula (M5 stretch) | PP-FormulaNet | Apache-2.0 **[verify card in E-phase]** | via paddle2onnx | UniMERNet (verify weights license) |
| VLM escalation (opt-in) | Granite-Docling-258M (+2stage) | **Apache-2.0** | GGUF (llama.cpp), community ONNX, MLX | PaddleOCR-VL-1.5 / GLM-OCR / LightOnOCR-2 (GPU class) |

**Reference ceilings, never shipped** (license-restricted): Surya (OpenRAIL $5M cap), DocLayout-YOLO (contested AGPL), Chandra (OpenRAIL $2M cap), LayoutReader weights (CC-BY-NC).

---

## 6. Datasets plan

Builds on the license-classed manifest in `eval/datasets/document-benchmarks-v2.json` and `IMPROVEMENT_PLAN.md` §2.

| Dataset | Tier | Use | Cap |
|---|---|---|---|
| olmOCR-Bench | permissive (ODC-BY) | frozen headline + smoke slice | full (~340 MB) |
| OmniDocBench v1.6 | eval-only | frozen headline end-to-end | full |
| READoc | permissive (MIT) | dev + smoke | full (~40 MB) |
| FinTabNet.c | permissive (CDLA-P-2.0) | table dev + frozen test | 2 GB slice |
| PubTabNet | permissive (CDLA-P-1.0) | table dev | 1 GB slice |
| DocLayNet | permissive (CDLA-P-1.0) | layout dev + frozen test | 2 GB slice |
| OHR-Bench | [verify] | secondary, RAG-impact | slice |
| ckorzen | research | text CER head-to-head (E0) | existing 67 MB |
| DocBank | research | secondary text check | existing slice |

**Dropped from v1** (manifest entries kept, marked deprecated): TableBank (CC BY-NC-ND, weak GT), RVL-CDIP (irrelevant), bulk arXiv/PMC/S2ORC (low value per GB).

**Internal-50 — the dataset that catches what benchmarks miss** (and §2.2 lesson 5 says the public benchmarks miss plenty: invoices, forms, non-academic layouts are exactly olmOCR-Bench's documented blind spots). 30–50 real-world PDFs we actually care about: SEC filings (`scripts/download-sec-10k.py` exists), invoices, manuals, multi-column papers, scanned letters, rotated scans, CJK/RTL samples, forms. Hand-checked markdown ground truth, written once, reviewed, frozen, stored in `eval/internal/` with per-file rights notes. **Never tuned against.** Building the GT is an M0 task, parallel to harness work.

---

## 7. New repo layout

```
dongler/
  crates/
    dongler-core/            # SALVAGED: parser, ir.rs (v2), render.rs; zero ML deps; WASM-safe
    dongler-pipeline/        # NEW: stages A–I + X
      src/
        triage.rs raster.rs textlayer.rs       # A–C (TextProvider trait here)
        layout.rs ocr.rs tables.rs             # D–F (ort sessions)
        fusion.rs order.rs                     # G–H (XY-Cut++ in order.rs)
        vlm.rs                                 # X (feature-gated)
        models/registry.rs                     # name→version→sha256→license→URL
        preprocess.rs                          # resize/normalize/NCHW (shared)
    dongler-cli/             # convert, models download, eval-dump subcommands
    dongler-python/          # PyO3: load() + convert()
    dongler-node/            # NAPI: load() + convert()
    dongler-wasm/            # fast path only
  eval/
    metrics/                 # MOVED from scripts/eval_metrics/ (Python, unchanged + tests)
    harness/                 # Python rewrite of run-benchmarks.py: run.py, diff.py, visual.py, adapters/<dataset>.py
    datasets/                # frozen-suite-v1.json, document-benchmarks-v2.json
    internal/                # Internal-50 corpus + GT
    smoke/                   # CI slice
    results/ledger.jsonl     # immutable milestone-gate scores
  experiments/               # E0..E7: one dir each, README.md + results.md (committed)
  PRD.md
```

---

## 8. Experiments

Each experiment: a directory under `experiments/` with README (hypothesis, method, pre-registered decision criterion) and committed `results.md`. Time-boxed; the gate decides, not vibes.

| ID | Question | Method | Gate / decision criterion |
|---|---|---|---|
| **E0 — Baseline reality check** (in M0) | Where is the bar, and are we actually bad — where exactly? | Run *current dongler* AND **Marker, MinerU (pipeline + hybrid backends), Docling** through the rebuilt harness on the frozen suite (their published numbers: §2.1 — our harness must roughly reproduce them, which validates the harness). Run competitors for *measurement only* — these are copyleft/restricted (Marker GPL, MinerU AGPL/custom "MinerU Open Source License", Docling MIT) so they never enter the shipped product; only Docling's architecture is freely borrowable. Also: dongler-core vs pdfium-text per-page CER. | Harness within tolerance of published competitor numbers. Text-provider default decided (>1 CER pt worse on >10% pages → flip). All numbers → ledger. |
| **E1 — OCR integration** | oar-ocr dependency or vendored PP-OCRv5-on-ort? And does PP-OCRv5 hold up on CPU? | Both routes prototyped; PP-OCRv5(mobile/server) vs Tesseract vs ocrs on olmOCR scanned slices + OmniDocBench scanned pages: CER/WER, s/region CPU, license re-verification of shipped ONNX artifacts. | Pick route + default. Must beat Tesseract CER at <0.5 s/region CPU (mobile variant). |
| **E2 — Layout bake-off** | heron vs PP-DocLayout_plus-L (vs restricted reference ceilings) | DocLayNet test slice + OmniDocBench layout GT: mAP, per-class recall (tables/formulas matter most downstream), latency CPU/GPU, artifact provenance check. | Pick default. Pre-registered: heron wins ties (official ONNX, license clarity, VLM synergy). |
| **E3 — Triage + fusion tuning** | Coverage thresholds; hybrid-page τ; orphan-span ε | Sweep on dev slices; end-to-end edit-sim on mixed corpora; verify invariants (no born-digital→OCR; no-drop). | Thresholds frozen into defaults; invariant tests green suite-wide. |
| **E4 — Table bake-off** | SLANet-plus vs TATR (with text-snap fusion for both) | FinTabNet.c/PubTabNet dev: TEDS/TEDS-struct/GriTS vs latency; ONNX artifact provenance. | Pick default; TEDS target set from E0's Marker/MinerU-pipeline numbers. |
| **E5 — Reading order** | Is our XY-Cut++ at parity with the field's algorithmic ordering? | Our impl vs current dongler heuristics vs competitor block orders: ARD/Kendall-τ on OmniDocBench; BLEU protocol from the XY-Cut++ paper on comparable data. | Within ε of MinerU-pipeline ordering → done. Else (unlikely, see §4.H): scoped learned-model spike with permissive weights or none. |
| **E6 — End-to-end vs the field** | Did the rebuild work? | Full pipeline vs E0 baselines on OmniDocBench + olmOCR-Bench (per-category) + Internal-50. | Evidence for M4/M5 gates and §10 success criteria. |
| **E7 — VLM escalation** (post-M4, time-boxed) | Does gated per-region Granite-Docling escalation buy real points? | Rust inference route bake-off (llama.cpp FFI vs ONNX vs candle); escalate low-confidence tables/formulas/scan regions on dev slices; measure validated-acceptance rate, per-region latency, frozen-suite deltas. | Pre-registered margin on table TEDS + formula + old-scans categories at acceptable latency → ship behind `vlm` feature. Else hook stays dormant. |

---

## 9. Milestones

| Milestone | Content | Exit criteria (frozen suite) |
|---|---|---|
| **M0 — Measure** | Rebuild `eval/harness/`; wire all five metric families + coverage/spurious-token metrics to real GT; frozen suite + smoke slice + `eval diff` + visual diffs; **E0** baselines (dongler AND Marker/MinerU/Docling); Internal-50 GT started | Harness reproduces competitor published numbers within tolerance; CI smoke green; "are we actually bad, and where" answered per-category in the ledger |
| **M1 — Skeleton** | Workspace restructure (`dongler-pipeline` crate); triage + raster + TextProvider; IR v2; fast path preserved byte-identical | Born-digital end-to-end ≥ current dongler on every metric (no regression from re-plumbing); triage ≥99% on a labeled page sample; `cargo build` with no ML features still yields the zero-dep fast path |
| **M2 — Layout + order** | heron via ort + fusion + XY-Cut++ (born-digital) | OmniDocBench ARD and olmOCR multi-column pass rate beat M0-dongler by the E5-set margin; no-drop invariant green suite-wide; ≥2 pages/s CPU on the born-digital+layout route |
| **M3 — OCR** | PP-OCRv5 integration; scanned + hybrid routes | olmOCR scanned categories jump from ~0 baseline to E1-projected level; Internal-50 scanned docs usable (rubric); ≥0.5 pages/s CPU scanned route |
| **M4 — Tables** | SLANet-plus + text-snap + HTML table rendering | E4 TEDS/GriTS gate; olmOCR table pass rate ≥ Marker's 72.9 within margin |
| **M5 — Ship** | Weights download UX, latency tuning, docs, releases (crates.io/PyPI/npm); E7 VLM decision; PP-FormulaNet stretch decision | §10 criteria met; clean-machine install-to-first-convert <5 min including weight download |

---

## 10. Success criteria

Calibrated against the measured field (§2.1), re-based by E0 on *our* harness; the re-based versions go in the ledger and become the contract.

- **olmOCR-Bench overall ≥ 70**, stretch 76 (Marker's level — the best measured pipeline; current dongler: 22.7). Per-category: tables ≥ 72, multi-column ≥ 75, headers/footers ≥ 85.
- **OmniDocBench v1.6 overall ≥ 80**, stretch 85.75 (MinerU-Pipeline's level), measured on our harness. With validated VLM escalation (if E7 ships): +3 or it stays dormant.
- **TEDS ≥ 0.85** on the FinTabNet.c frozen slice.
- **Hallucination: zero invented tokens** on the deterministic route by construction (provenance-audited); spurious-token rate < competitors' on SCORE-style measurement.
- **Zero per-doc regressions** vs current dongler on born-digital Internal-50.
- **Throughput:** fast path unchanged (~90 pages/s); born-digital pipeline route ≥2 pages/s CPU; scanned ≥0.5 pages/s CPU; GPU ≥5× those.
- Every claim reproducible by `eval run --suite frozen-v1` from a clean checkout.

---

## 11. Risks

| Risk | Mitigation |
|---|---|
| **`ort` is an RC** (2.0 final not out; rc-to-rc churn) | Pin `=2.0.0-rc.12`; it is the de facto production line (11M downloads, HF/Magika in prod); upgrade deliberately at milestones |
| **Model-weight licenses** | §5.2 registry records weight license per artifact; only verified Apache/MIT ship; contested ones (DocLayout-YOLO) excluded; ONNX artifact *provenance* (who converted it) checked in E1/E2/E4 — prefer official exports (heron) and first-party weights (Paddle HF org) |
| **oar-ocr is a young dependency** (single-maintainer, v0.7) | E1 decides depend-vs-vendor; either way our surface is `ort` + ONNX artifacts, so vendoring is a bounded rewrite, not a redesign |
| **pdfium binary distribution** (native lib per platform) | bblanchon prebuilts are continuously released; static-link option; document `PDFIUM_*` overrides; WASM/fast path unaffected |
| **CPU latency of the full pipeline** | Mobile-class model variants by default on CPU; per-region (not per-page) OCR; raster DPI and batch levers; throughput floor in the suite (§3.3 rule 5) |
| **VLM escalation drags in heavy deps / hallucination** | Feature-gated, off by default; per-region only; mechanical validators with deterministic fallback; E7 pre-registered margin or it stays dormant |
| **Weight-download UX vs "zero downloads" brand** | Fast path stays zero-download; sha256-pinned cache; `dongler models download`; offline workflow documented |
| **Benchmark saturation / Goodharting the saturated** | §3.3 rule 6; per-category reporting; Internal-50 held out; visual diffs mandatory at gates |
| **Scope creep** (formulas, KIE, handwriting, WASM-ML) | §12 non-goals; single gated stretch decision (formula) at M5 |

---

## 12. Non-goals (v2)

- No model training or fine-tuning.
- No hosted-API calls; no cloud service.
- No full-page VLM parsing (per-region escalation only, and only if E7 earns it).
- No handwriting recognition; no KIE/DocVQA.
- No ML pipeline in WASM/Node-fast-path (fast path only there); Python/Node *do* get the full pipeline via bindings.
- No PDF editing/generation.
- LaTeX renderer maintained, not advanced.

---

## 13. Appendices

### 13.1 IR v2 schema sketch (diff vs `ir.rs` v1)

```rust
// version: "dongler.ir.v2"  (v1 remains deserializable)
struct Page { route: Route, ..v1 }                    // + Route
enum Route { BornDigital, Scanned, Hybrid }
enum BlockKind {                                       // closed enum replaces free string
  Heading(u8 /*1..=6*/), Paragraph, ListItem, Code, Formula,
  Caption, PageHeader, PageFooter, Footnote, Table, Figure,
}
struct Provenance {                                    // NEW, on every block
  text_source: TextSource,        // TextLayer | Ocr | Vlm
  detector: Option<String>,       // "docling-layout-heron@v2.1"
  confidence: Option<f32>,
}
struct TableBlock { html: Option<String>, ..v1 }       // + html; cells keep col_span/row_span
// Span (font/size/bold/italic/bbox/anchor) unchanged from v1
```

### 13.2 Salvaged assets (by path)

| Asset | Path | Disposition |
|---|---|---|
| IR schema v1 | `crates/dongler-core/src/ir.rs` | Extend to v2 (§13.1) |
| Metric library | `scripts/eval_metrics/` | Move to `eval/metrics/`; wire in M0 |
| Rust PDF parser | `crates/dongler-core/src/pdf.rs` | Default `TextProvider`, subject to E0; heuristic detectors demoted to fast path |
| Renderers | `crates/dongler-core/src/render.rs` | Keep; add HTML-table emission |
| License research | `IMPROVEMENT_PLAN.md` §2, §6 | Carried into §3.2/§6 here |
| Dataset manifest | `eval/datasets/document-benchmarks-v2.json` | Basis for frozen-suite-v1 |
| Visual compare script | `scripts/render-extraction-comparison.py` | Evolves into `eval/harness/visual.py` |
| SEC downloader | `scripts/download-sec-10k.py` | Feeds Internal-50 |

### 13.3 Metric glossary

- **CER/WER** — character/word error rate vs GT text (`text.py`).
- **Edit similarity** — normalized Levenshtein on markdown (`text.py`).
- **TEDS / TEDS-struct** — tree edit distance similarity on table HTML, with/without content (`table.py`).
- **GriTS top/con/loc** — table grid similarity: topology, content, location (`grits.py`).
- **Layout mAP / mean-best-IoU** — COCO-style detection quality (`layout.py`).
- **ARD / Kendall-τ** — reading-order rank distance and correlation (`order.py`).
- **olmOCR unit pass rate** — binary per-check assertions, by category.
- **Token coverage / spurious-token rate** — NEW (M0): fraction of source text present in output / fraction of output not attributable to source (SCORE-Bench-style).
- **CDM** — formula recognition metric used by OmniDocBench (only relevant if M5 stretch ships).

### 13.4 Key sources (verified 2026-06-12)

Leaderboards: OmniDocBench repo (v1.6 table, 2026-04-30 update); allenai/olmocr bench README + olmOCR-2 paper (arXiv:2510.19817); saturation/quality critiques: Datalab "Saturating the olmOCR Benchmark", LlamaIndex benchmark reviews (2025-12/2026-02), PureDocBench (arXiv:2605.07492 — the 12.08% OmniDocBench annotation-error audit), Real5-OmniDocBench (HF, PaddlePaddle), CC-OCR v2 (arXiv:2605.03903); SCORE-Bench (Unstructured, 2025-12); OHR-Bench (ICCV 2025, arXiv:2412.02592); CDM vs LLM-judge correlation (pdf-parse-bench, arXiv:2512.09874; DOCR-Inspector, arXiv:2512.10619). Architecture: MinerU2.5 paper (arXiv:2509.22186); PaddleOCR-VL (arXiv:2510.14528); XY-Cut++ (arXiv:2504.10258); Granite-Docling announcement + HF cards (ibm-granite/granite-docling-258M, docling-project/granite-docling-2stage-258m). Models/licenses: HF cards for docling-layout-heron(-onnx), PaddlePaddle/PP-OCRv5_server_det + PP-DocLayoutV2 + SLANet, PP-OCRv6 (PaddleOCR v3.7.0 release, 2026-06-11), microsoft/table-transformer-* (MIT); DocLayout-YOLO issue #110 (license contradiction); surya MODEL_LICENSE; MinerU LICENSE.md history (AGPL→Apache→AGPL→"MinerU Open Source License", finalized 2026-04-17; MinerU2.5-2509 weights AGPL vs MinerU2.5-Pro-2604 weights Apache). Rust ecosystem: crates.io/GitHub for ort (2.0.0-rc.12), pdfium-render (0.9.1), oar-ocr (0.7.0), ocrs/rten, hayro (0.7.1), firecrawl/pdf-inspector (MIT triage prior art), rstar, hf-hub (0.5.0), ferrules (GPL ref), kreuzberg v4 (Elastic-2.0 ref), usls; bblanchon/pdfium-binaries releases.

### 13.5 Operating rules (how we avoid rabbit holes, summarized)

1. M0 before any extraction work. No exceptions.
2. Every extraction PR carries an `eval diff`. Aggregates AND per-doc regressions.
3. Bake-offs decide model choices; pre-registered gates decide phase transitions; the ledger records both.
4. Look at visual diffs at every gate — numbers alone got us here.
5. Time-box experiments. If a gate isn't met in its box, escalate the decision rather than grinding.
6. License check is part of every model decision: weight license, code license, and ONNX-artifact provenance, recorded in the registry.
