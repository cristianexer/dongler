# Dongler Improvement Plan

_Authored 2026-06-10. Research verified against primary sources (pypdf/pdfplumber/PyMuPDF source, ISO 32000-1, HuggingFace dataset pages) and against dongler's own source with `file:line` references._

---

## Context

**Dongler** (v0.3.4, MIT) is a fast, Rust-native document-extraction engine with a **custom PDF parser** (no pdfium/poppler/lopdf — only `flate2` + `rayon` + `serde`), exposed through CLI + Python (PyO3) + Node (NAPI). It lowers documents into a `dongler.ir.v1` IR (`Document → Page → Block{Text|Table|Figure}` with `bbox` / `lines` / `spans` / `source_anchors` / `confidence`) and renders Markdown / LaTeX / JSON.

The IR is already shaped for rich output — `Span` even carries `font` and `size` ([ir.rs:156](crates/dongler-core/src/ir.rs)) — but the **detectors and renderers throw most of that signal away**. This plan addresses six asks: (1) understand scope, (2) more eval datasets, (3) speed/quality, (4) accurate text+markdown+bbox and how to test them, (5) more tests, (6) competitor-inspired (license-safe) techniques.

### Three findings that reframe the work

1. **🔴 Heading detection is dead end-to-end (a real bug, not just a weak heuristic).** The PDF path emits `kind = "heading"` ([pdf.rs:4137](crates/dongler-core/src/pdf.rs)), but the renderers only recognize `"heading_N"` via `strip_prefix("heading_")` ([render.rs:231](crates/dongler-core/src/render.rs)). So `heading_level("heading") == None` and **every PDF-detected heading renders as a plain paragraph** in both Markdown and LaTeX. The `"list"` kind is likewise never produced by the PDF path. Markdown structure from PDFs is currently almost entirely flat.
2. **🟠 The eval harness measures far less than the manifests claim.** Only three metric families are implemented in [run-benchmarks.py](scripts/run-benchmarks.py): `token_f1` (multiset token overlap, ~L435), `olmocr_unit_pass_rate` (binary present/absent/order/table/math checks, ~L925), and `full_image_iou` — and that last one is **degenerate**: it scores each block's bbox against the *whole page rectangle* (~L1158), so it is a "does anything cover the page" check, not per-element IoU. **TEDS, GriTS, CER/WER, real bbox IoU, and reading-order metrics are named in the dataset manifests but have zero implementation.** We cannot currently prove markdown/bbox/table accuracy.
3. **🟠 The custom parser is permissively positioned to learn from the best.** The user noted "pypdf 6.13.1 … we can't use the license." That's worth correcting: **pypdf 6.13.1 is the current release (2026-06-08) and is BSD-3-Clause — permissive.** Code reuse/porting is allowed with attribution. The genuinely restrictive engine is **PyMuPDF (AGPL-3.0 / paid Artifex)**. Either way, since dongler is its own Rust engine, we take *algorithmic* inspiration; the verified heuristics below are all from permissive or spec sources.

---

## 1. Project scope & goal (confirmed understanding)

| Aspect | Detail |
|---|---|
| Goal | Fast, **local**, no-API/no-LLM/no-OCR (for born-digital PDFs) extraction to Markdown / LaTeX / JSON |
| Core | `crates/dongler-core` — `pdf.rs` (~4350 LOC custom parser), `engine.rs`, `render.rs`, `ir.rs`, plus OpenXML/HTML/CSV/JSON/XML/archive/image engines |
| Bindings | CLI (`clap`), Python (PyO3/maturin), Node (NAPI) — all over one core |
| Parallelism | `rayon` page-level `par_iter` ([pdf.rs:275](crates/dongler-core/src/pdf.rs)) |
| Differentiators | Source anchors (block→PDF object), batch API that doesn't abort on one bad file, minimal dependency surface |

The architecture is sound; the gaps are in **detection quality, output richness, eval rigor, and a few perf redundancies** — not in the foundations.

---

## 2. Plan — eval datasets to download

We already reference a good set in [`eval/datasets/document-benchmarks-v1.json`](eval/datasets/document-benchmarks-v1.json), but most are `"download": "disabled"` and the modern end-to-end ones are under-exploited. Below is a **verified** procurement list (IDs/licenses confirmed on the pages this week). Add a `document-benchmarks-v2.json` manifest and extend [`download-benchmark-data.py`](scripts/download-benchmark-data.py).

### Tier A — add first (modern, end-to-end, rich ground truth)

| Dataset | Verified source | License | Size | Ground truth | Metric |
|---|---|---|---|---|---|
| **OmniDocBench** | HF `opendatalab/OmniDocBench` | research-only (data); Apache-2.0 (code) | 1,651 pages / 1.55 GB | Markdown + 28 block & 4 span bboxes + reading order + table HTML/LaTeX + formula LaTeX | Norm. edit-dist, TEDS, CDM, BLEU/METEOR, COCO mAP |
| **olmOCR-Bench** | HF `allenai/olmOCR-bench` | ODC-BY | 1,403 PDFs / 7,010 unit tests | JSONL pass/fail assertions (math, tables, multi-col order, headers/footers, old scans) | Unit-test pass-rate % (already partly wired) |
| **READoc** | HF `lazyc/READoc` | MIT | 3,576 rows / 3.28 GB | Structured Markdown (arXiv+GitHub) | Norm. edit-dist + segmentation/structure score |
| **ReadingBank** | HF `zilongwang/ReadingBank` (full gated; ~100 sample open) | unverified | 500k pages | Word sequence + coords in **reading order** | Page-BLEU + Avg. Relative Distance (ARD) |

### Tier B — tables (we have table detection but no table accuracy metric)

| Dataset | Source | License | Notes |
|---|---|---|---|
| **PubTables-1M** | HF `bsmock/pubtables-1m` | CDLA-Permissive-2.0 | 117 GB; PASCAL-VOC bbox + structure + word boxes; **GriTS** |
| **PubTabNet** | GH `ibm-aur-nlp/PubTabNet` (HF mirror `apoidea/pubtabnet-html`) | CDLA-Permissive-1.0 | 568k tables; HTML structure + cell bbox; **TEDS** |
| **FinTabNet.c** | HF `bsmock/FinTabNet.c` | CDLA-Permissive-2.0 | 3.43 GB; TSR-aligned; GriTS/TEDS |
| **SciTSR** | GH `Academic-Hammer/SciTSR` | MIT (verify) | 15k scientific tables, merged cells; cell-adjacency F1 / TEDS |

### Tier C — layout / multilingual / forms (already partly present; broaden)

- **DocLayNet** `docling-project/DocLayNet` (CDLA-Permissive-1.0, 80,863 pages, COCO, mAP) and **ICDAR2023-DocLayNet** competition split — fix the stale `ds4sd/DocLayNet` reference (now redirects).
- **Nougat-style arXiv math/markdown GT**: no packaged dataset — use Nougat's LaTeXML pipeline (GH `facebookresearch/nougat`, code MIT) to build math/markdown GT; metrics edit-dist/BLEU/METEOR/F1.
- **Multilingual/Chinese layout**: `HCIILAB/M6Doc` (CC BY-NC-ND, ZH+EN), `buptlihang/CDLA` (Chinese), `D4LA` (via ModelScope). License-gate these for non-commercial eval-only.
- **Forms/receipts (already have FUNSD/SROIE)**: add **CORD-v2** `naver-clova-ix/cord-v2` (CC BY-4.0) and **XFUND** (multilingual). Note FUNSD is non-commercial-research-only.

### Licensing guardrails (must encode in the manifest)

Tag each dataset `commercial_ok` vs `eval_only`. **Eval-only / non-commercial**: OmniDocBench data, FUNSD, M6Doc, TableBank (GitHub copy is CC BY-NC-ND), XFUND. **Permissive**: READoc (MIT), DocLayNet & PubTables-1M & FinTabNet.c (CDLA-Permissive), olmOCR-Bench (ODC-BY). Flag the **UNVERIFIED** licenses before bulk download: SciTSR, WTW, D4LA, CDLA, ReadingBank, SROIE/XFUND mirrors.

### Deliverables for §2
- `eval/datasets/document-benchmarks-v2.json` with Tier A/B/C, `license_class`, and per-dataset GT format.
- Extend `download-benchmark-data.py` with the new HF ids + a license-class gate (`--allow eval_only`).
- Wire a **small, license-clean CI slice** (a handful of OmniDocBench + olmOCR + READoc + PubTabNet pages, committed-or-cached) so accuracy regressions are caught without a 100 GB pull.

---

## 3. Plan — make it faster and better (performance)

All `file:line` in `crates/dongler-core/src/pdf.rs`. Ordered by effort/payoff.

| # | Where | Problem | Fix | Impact |
|---|---|---|---|---|
| **P1** | `load_font_decoders` 3391 / `font_decoder` 3423 / `decode_stream_object` 3446, called per page from 323 | ToUnicode CMap is **flate-inflated + parsed once per page**; fonts are document-shared → `P·F` work instead of `F`. A 500-page/5-font doc does ~2,500 redundant inflations. | Build a **doc-level `HashMap<u32, Arc<FontDecoder>>`** keyed by font **object number** once before `par_iter` (optionally `unique_font_objs.par_iter().map(font_decoder)`); pass `&cache` into `extract_page`. | **High** — likely the dominant avoidable cost |
| **P2** | `group_text_runs` 2749 (esp. 2760 find, 2765 re-sort, 2767 `union_boxes`) | Per-insert: linear `find` over lines (`O(n·L)`), full re-sort of the line (`O(k² log k)`), and `union_boxes` recomputed over all runs (`O(k²)`). Pathological on dense/table/math pages. | **Bucket runs into `HashMap<i32, Vec<TextRun>>`** by quantized y in one `O(n)` pass; sort each band by x **once**; compute each `union_boxes` **once**. Drops to `O(n log n)`. | **High** on dense pages |
| **P3** | `parse_indirect_objects` 3040 + map clone 241 | Object bodies copied 2–3× (`to_vec` + map clone + original kept for `extract_info_string`). 100 MB PDF → 200 MB+ duplicate `Vec<u8>`. | `HashMap<u32, Arc<PdfObject>>`, **move** objects in (drain, not clone), share `Arc<ObjectMap>` into `par_iter`; iterate map for info string. | **High** memory |
| **P4** | `expand_object_streams` 3055 / `page_seed` 3118 | Two `String` allocations **per object** (`lossy` + whitespace-compacted) just to substring-test `/Type/ObjStm`. Scales with object count (10k–100k). | Byte-level whitespace-skipping `contains` helper; avoid the double alloc. | **Med** |
| **P5** | `detect_paired_text_columns` 1046–1062 | Pairwise `O(L²)` left/right comparison per page. | Sort-and-sweep: reuse P2's y-buckets, only compare within a band. | **Med** |
| **P6** | 829, 866, 1289 | Runs already x-sorted out of `group_text_runs` but **clone+re-sorted 3×** (deep `String` clones). | Establish "`TextLine.runs` is x-sorted" invariant; operate on `&line.runs`; delete the redundant clone+sort. | **Med** alloc |
| **P7** | `contains_name` 258/266/303, `find_subslice` 4332 | Three full-file scans for `/Encrypt`/`/ObjStm`; naive `windows(m).position`. | Compute `contains_name` once into locals; use `memchr`/`memmem` for substring search. | **Low** |
| **P8** | `FontDecoder::decode_byte` 115; `source_object_ids.to_vec()` 660 | `String`/`Vec` alloc per glyph & per run. | `Cow<str>` / reused buffer; `Arc<[String]>` for shared ids. | **Low–Med** |

**Add a perf gate.** Wire `eval-smoke.sh` (or a Criterion bench) into CI with a `pages_per_second` floor on a fixed large fixture, so regressions surface.

---

## 4. Plan — accurate text, accurate Markdown, accurate bounding boxes (+ how to test)

This is the heart of the request. Each item lists the **verified heuristic** (with constants/sources) and the **dongler change**.

### 4.1 Reading order — replace 2-column-only heuristics with column-aware XY-cut

**Current:** two detectors, both **hard-limited to exactly two columns** ([pdf.rs:1039](crates/dongler-core/src/pdf.rs), [pdf.rs:1178](crates/dongler-core/src/pdf.rs)). 3+ columns collapse/interleave; single columns with a wide caption or right-aligned block get **falsely split** at the ≥90 pt gap (pdf.rs:1190).

**Verified approach — Recursive XY-cut / XY-Cut++** (Ha/Haralick/Phillips ICDAR'95; OpenDataLoader XY-Cut++): project content onto X and Y, **cut along the axis with the larger whitespace valley, recurse until each region is a single column**, emit leaves top→bottom/left→right. XY-Cut++ refinements worth copying: pull out **full-width cross-layout elements (headers/footers/titles) first** so they don't break column detection, then reinsert by Y; **density-adaptive direction** (density > 0.9 → prefer horizontal cuts first); **min gap 5.0 pt**. MinerU frames the same idea as "divide page into regions each containing at most one column, then read top→bottom, left→right."

**Change:** new `reading_order.rs` implementing N-column recursive cut on the line/run boxes, replacing the paired/2-col detectors. Keep the existing column tests as regression anchors; add 3-column + cross-layout fixtures.

### 4.2 Word segmentation — principled gap thresholds tied to font metrics

**Verified constants:** Poppler `minWordBreakSpace = 0.1·fontSize`, `maxCharSpacing = 0.03`, wide-tracking guard `×1.3`; pdf.js `TRACKING_SPACE_FACTOR = 0.102`, `SPACE_IN_FLOW_MAX = 0.6`, `VERTICAL_SHIFT_RATIO = 0.25`; pdfplumber `x_tolerance = 3 pt` (or `x_tolerance_ratio·size`). pypdf inserts one space when a TJ kerning number `|n| ≥ 0.95·(space_width/2)`.

**Change:** when joining runs/glyphs, insert a space when `gap > k·font_size` with `k ≈ 0.2`, *and* cross-check the font's real space-glyph advance (`gap > 0.3·space_width`) when available — use the per-glyph `/Widths` advance already parsed (`font_widths` 3461), not a constant. Add a wide-tracking guard so letter-spaced display text isn't shattered into single chars.

### 4.3 Heading detection — font-size statistics (and fix the dead `"heading"` kind)

**Verified approach (pymupdf4llm `IdentifyHeaders`, academic body-text extraction):** body size = **most frequent (mode) size weighted by char count**; everything ≤ body = body text; distinct **larger** sizes sorted descending → `#`…`######` (cap at H6, collapse deeper levels into H6). Combine with **bold/ALLCAPS/space-before** because many docs use bold-at-body-size for headings (supervised approach hits ~97% with size+bold+caps+indent features).

**Change:**
1. **Fix the bug first:** emit `"heading_1".."heading_6"` from the PDF path (not `"heading"`), or teach `heading_level` to accept `"heading"`. (Lowest-effort, highest-visible-impact change in the whole plan.)
2. Replace `classify_text_line` ([pdf.rs:4135](crates/dongler-core/src/pdf.rs)) with a doc-level pass: compute the body-size histogram from `Span.size` (already populated, [pdf.rs:1267](crates/dongler-core/src/pdf.rs)), map larger sizes → `heading_N`, fold in bold/caps.

### 4.4 Bold / italic — detect from font descriptor flags + BaseFont name

**Verified (ISO 32000-1 §9.8.2 Table 123 + PyMuPDF flags):** descriptor `/Flags` bit 7 (`64`) = Italic, bit 19 (`262144`) = ForceBold; also `/ItalicAngle != 0` ⇒ italic. **Most reliable in practice:** case-insensitive BaseFont-name substring (after stripping the `ABCDEF+` subset prefix): bold ∈ {`Bold`,`-Bd`,`Semibold`,`Black`,`Heavy`,`Demi`}, italic ∈ {`Italic`,`Oblique`,`-It`}.

**Change:** parse `/FontDescriptor` `/Flags` + `/ItalicAngle` in `font_decoder` and add `bold`/`italic` to `TextRun`/`Span`; the font name is already on the span. Then enrich the renderer (4.7).

### 4.5 Ligatures — expand FB00–FB04 (NFKC, not NFC)

**Verified:** glyph *names* `fi`/`fl`/… are expanded ([pdf.rs:4026](crates/dongler-core/src/pdf.rs)), but a ToUnicode/`uniFB01` mapping to precomposed **U+FB00–U+FB06 is passed through unchanged** ([pdf.rs:4085](crates/dongler-core/src/pdf.rs)); there's no NFC/NFKC anywhere. Note: **NFC/NFD do *not* decompose ligatures — only NFKC/NFKD do.** Leaving raw `ﬁ` degrades search/LLM matching.

**Change:** add an explicit `FB00→ff, FB01→fi, FB02→fl, FB03→ffi, FB04→ffl, FB05/FB06→st` lookup in `normalize_pdf_token` ([pdf.rs:1636](crates/dongler-core/src/pdf.rs)) (explicit table avoids NFKC's over-aggressive other rewrites). Mirror the benchmark's existing `→fi` normalization (run-benchmarks.py:457) into the *extractor* so eval and output agree.

### 4.6 Page rotation — actually apply `/Rotate`

**Verified gap:** `/Rotate` is parsed ([pdf.rs:318](crates/dongler-core/src/pdf.rs)) and stored ([pdf.rs:387](crates/dongler-core/src/pdf.rs)) but **never applied** to coordinates or reading order; `text_run_bbox` uses only text-matrix + CTM (pdf.rs:701). A 90/270 page is grouped/ordered along the wrong axis.

**Verified transform (ISO 32000-1 §7.7.3.3):** clockwise display rotation — 90: `(x,y)→(y, W−x)`, swap W/H; 180: `(W−x, H−y)`; 270: `(H−y, x)`, swap W/H. Apply to every glyph point **before** line grouping/column detection so left-to-right matches what a human sees.

**Change:** apply the rotation transform into the coordinate frame before `group_text_runs` ([pdf.rs:371](crates/dongler-core/src/pdf.rs)); swap page width/height for 90/270.

### 4.7 Markdown renderer — emit the structure the IR already holds

**Current:** `MarkdownRenderer` ([render.rs:71](crates/dongler-core/src/render.rs)) emits only ATX headings (for kinds it never receives), flat bullet lists (likewise), GFM pipe tables, and images. **No bold/italic/inline-code/blockquote/nested lists.** It reads only `kind`, never `lines`/`spans`/`font`.

**Change:** make the renderer span-aware — wrap bold spans in `**…**`, italic in `*…*`, monospaced (font name/flags) in `` `…` `` (pymupdf4llm does exactly this from span flags). Emit headings from the new `heading_N` kinds (4.3), lists from a real list detector, and keep tables. This is where 4.3–4.5 become visible.

### 4.8 Bounding-box accuracy — use real font metrics, not a fixed multiple

**Current:** min bbox enforced as `max(size·0.25)` ([pdf.rs:718](crates/dongler-core/src/pdf.rs)) — a fixed fraction of font size.

**Verified (ISO 32000-1 §9.4.4 + FontDescriptor Table 122):** horizontal extent from real per-glyph `/Widths` (`tx = (w0 − Tj/1000)·Tfs·Th + …`); vertical extent from `/Ascent`, `/Descent` (height `= (Ascent−Descent)·Tfs/1000`) or `/FontBBox` for true ink bounds — passed through `Trm = font-size-scale × Tm × CTM` so boxes stay correct under rotation/scaling.

**Change:** parse `/Ascent`/`/Descent`/`/FontBBox` from the descriptor; compute glyph/line boxes from those instead of `size·0.25`. This also gives correct boxes for the bbox-IoU eval (4.9).

### 4.9 How we test these — implement the missing eval metrics ✅ DONE (2026-06-10)

We can't claim accuracy we don't measure. The metric toolkit now lives in
[scripts/eval_metrics/](scripts/eval_metrics/) (pure, dependency-free, 139 unit
tests) and is wired into [run-benchmarks.py](scripts/run-benchmarks.py):

| Metric | For | Status |
|---|---|---|
| **CER / WER / edit similarity / BLEU** | text & markdown vs OmniDocBench/READoc/ckorzen/DocBank text GT | ✅ implemented (`eval_metrics.text`) + **wired** on the text-GT path (reported per-document + dataset `text_metrics`, plus an "Edit sim" report column) |
| **TEDS / TEDS-Struct** | table structure vs HTML GT | ✅ implemented (`eval_metrics.table`, Zhang-Shasha) + **wired** via sibling `<doc>.tables.html` / `.tables.json` |
| **GriTS (top/con/loc)** | table grid topology+content vs GT | ✅ implemented (`eval_metrics.grits`, factored 2D-MSS) + **wired** alongside TEDS |
| **Real per-element bbox IoU + COCO mAP** | block boxes vs layout GT | ✅ implemented (`eval_metrics.layout`) + **wired** via sibling `<doc>.boxes.json` (label-agnostic localization mAP + mean-best-IoU) |
| **Reading-order: ARD + sequence BLEU + Kendall-τ** | reading order vs GT | ✅ implemented (`eval_metrics.order`) — exported & unit-tested; auto-wiring deferred until a shared-id GT format is defined (e.g. OmniDocBench/ReadingBank) |

Remaining: **visual diff + per-metric dashboards** (build on
`render-extraction-comparison.py`) and a small committed CI slice (from §2) that
fails on regression. The metrics above now make every §4 change measurable
before/after.

---

## 5. Plan — expand the test suite (cover all angles)

Today: 100 in-memory Rust tests in [tests/core.rs](crates/dongler-core/tests/core.rs) (strong on text/geometry/columns/tables), but **one** inline `pdf.rs` unit test, **zero on-disk/golden fixtures**, **no property/fuzz tests**, and **no cross-binding consistency test**. Backlog (each → a ticket):

1. **Rotation** — `/Rotate 90/180/270` fixtures asserting transformed page W/H, bbox, and reading order (pairs with 4.6).
2. **Encrypted PDFs** — RC4/AES `/Encrypt` fixture: assert today's `pdf.encrypted` warning ([pdf.rs:258](crates/dongler-core/src/pdf.rs)); later add the password-decrypt path + test (currently `password` option exists in `ExtractOptions` but no decryption).
3. **Malformed/truncated corpus** — truncated stream, bad/missing xref, missing `endobj`, cyclic `/Pages`, wrong `/Length`: assert graceful `Result`/warning, **no panic**.
4. **Property-based + fuzz** — `proptest` on `parse_indirect_objects` and the CMap parser (codespace/bfrange/bfchar round-trips); optional `cargo-fuzz` on raw `load_path` bytes (panic-free, bounded memory, no infinite loop — note pypdf shipped infinite-loop guards in 6.12.2/6.13.0).
5. **Real Unicode ligature** + **bold/italic** font fixtures (assert markdown emphasis once 4.4/4.5/4.7 land).
6. **RTL (Arabic/Hebrew) + CJK** fixtures with asserted order/text (none today).
7. **3+ column** non-table reading-order fixture (pairs with 4.1).
8. **Cross-binding golden test** — one shared PDF → assert byte-identical JSON across Rust/Python/Node (today each binding has its own duplicated `minimal_pdf` and hand-mirrored asserts).
9. **Native-PDF absolute glyph-bbox** assertions (today only relative/`is_some()`).
10. **TEDS/GriTS unit tests** for the new table metrics; wire `eval-smoke` thresholds into CI (today non-gating).

Convert the per-binding duplicated generators into one shared fixture set to keep 8 honest.

---

## 6. Plan — competitor-inspired, license-safe techniques

**License reality (verified this week):**
- **pypdf 6.13.1** — current release, **BSD-3-Clause = permissive**. Porting with attribution is allowed. (Correcting the original assumption.)
- **pdfplumber** — MIT (permissive). **Docling** — MIT. **MinerU** — now "Apache-2.0 + conditions" (was AGPL). **Nougat** — code MIT (no packaged dataset).
- **PyMuPDF / pymupdf4llm** — **AGPL-3.0 / paid commercial** = the restrictive one. Learn the *algorithm* (documented), don't lift code.
- **Marker** — GPL-3.0 code + RAIL-licensed weights. Heuristics are describable; avoid the code.

Since dongler is its own Rust engine, we adopt **techniques**, already folded into §3–§5:

| Source | Borrowable technique | Lands in |
|---|---|---|
| **pypdf** (plain mode) | Space when TJ kerning `|n| ≥ 0.95·space_width/2`; newline when `Δy > 0.8·fontHeight`; space when `moved_width ≥ ½·space_width + Σglyph_widths` | 4.2 |
| **pypdf** (layout mode) | Rigorous advance `Tfs·(w−Td)/1000 + Tc + n·Tw`, Tz via `Tfs·Tz/100` — accurate spacing without heuristics | 4.2, 4.8 |
| **pdfplumber** | Tolerance-based word/line clustering (`x_tol=y_tol=3`); table strategies `lines`/`lines_strict`/`text` with `snap`/`join`/`intersection` tolerances and `min_words_vertical=3` | 4.2, table recall |
| **pymupdf4llm** | Font-size-histogram heading levels; per-span `**`/`_`/`` ` `` from flags; tables via finder | 4.3, 4.7 |
| **MinerU / XY-Cut++** | Column-aware recursive XY-cut; pull full-width headers/footers first; header/footer/footnote stripping by position | 4.1 |
| **Docling / TableFormer** | Predict grid topology, then **snap real PDF text cells back into the grid** in post-processing (avoids re-transcription) — applicable to our ruled-grid path | table accuracy |
| **Nougat** | Benchmark target & metrics (edit-dist/BLEU/METEOR/F1) for math/markdown GT | 4.9 |

**Table recall fix (from pdfplumber):** today implied/alignment tables require a literal "Table" label nearby ([pdf.rs:2343](crates/dongler-core/src/pdf.rs)) and a ruled grid needs ≥4 rows×≥2 cols — a recall gap. Adopt pdfplumber's `text` strategy (infer rulings from `min_words_vertical=3` aligned words) to promote unlabeled columnar blocks; add `rowspan`/`colspan` to `TableCell` ([ir.rs:167](crates/dongler-core/src/ir.rs)) for spanning cells (none today).

---

## Prioritized roadmap

**Phase 0 — quick wins (days):**
- ✅ **DONE** Fix the dead heading kind — `classify_text_line` now emits `heading_1..3` from font size relative to a page body-size baseline ([pdf.rs](crates/dongler-core/src/pdf.rs) `classify_text_line` / `page_body_size` / `line_dominant_size`), so PDF headings render as `#`/`\section`. Tested (`load_path_classifies_larger_pdf_line_as_heading`).
- ✅ **DONE** Ligature FB00–FB06 expansion at the `normalize_pdf_token` chokepoint (`expand_latin_ligatures`). Tested (`load_path_expands_pdf_unicode_ligatures`).
- ✅ **DONE** P7: compute the `/Encrypt` scan once (was scanned twice). P6: `runs_sorted_by_x` returns a `Cow` that borrows when the line is already x-sorted (the common case out of `group_text_runs`), eliminating the deep `Vec<TextRun>` clone+sort at the three column/word sites (`split_text_line_at_wide_gap`, `has_repeated_tight_column_band_evidence`, `text_from_line_runs`). Output is identical (verified by the 102 core tests).

**Phase 1 — accuracy core (1–2 weeks):**
- Doc-level font cache (P1) + `Arc<PdfObject>` (P3) + `group_text_runs` bucketing (P2).
- Font-size heading detection + bold/italic flags (4.3/4.4) and span-aware Markdown renderer (4.7).
- Apply `/Rotate` (4.6); font-metric bboxes (4.8).

**Phase 2 — reading order & tables (2–3 weeks):**
- XY-cut reading order (4.1); pdfplumber-style table recall + spanning cells (§6).

**Phase 3 — eval rigor (parallelizable):**
- Implement TEDS/GriTS/CER-WER/real-IoU/reading-order metrics (4.9); add v2 dataset manifest + downloader (§2); CI slice + perf gate.

**Phase 4 — test hardening:** the §5 backlog (rotation, encryption, malformed, proptest/fuzz, RTL/CJK, cross-binding golden).

---

## Verification

- **Unit/integration:** `cargo test --workspace`; new fixtures per §5.
- **Bindings:** `make test-python`, `make test-js`; new cross-binding golden test.
- **Accuracy:** `make bench-data && make bench-run` against the v2 datasets; assert improvements in norm-edit-dist (text/markdown), TEDS/GriTS (tables), bbox IoU/mAP (layout), ARD (reading order) vs the current baseline.
- **Speed:** `make eval-smoke PDF=<large>.pdf` and a Criterion bench wired into CI with a `pages_per_second` floor.
- **Visual:** `render-extraction-comparison.py` before/after on a fixed sample set.
- **No regressions:** every Phase keeps the existing 100 core tests green.
