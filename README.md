<p align="center">
  <a href="https://cristianexer.github.io/dongler/"><img src="https://cristianexer.github.io/dongler/img/dongler-banner.svg" alt="Dongler — fast, Rust-native document extraction to Markdown, LaTeX, and JSON" width="100%"></a>
</p>

<h1 align="center">Dongler</h1>

<p align="center">
  <b>Turn PDFs and documents into clean Markdown, LaTeX, or structured JSON.</b><br>
  Rust-native, runs locally, no hosted service · API key · LLM · or OCR for digitally born PDFs.
</p>

<p align="center">
  <a href="https://pypi.org/project/dongler/"><img alt="PyPI" src="https://img.shields.io/pypi/v/dongler?style=for-the-badge&logo=python&logoColor=white&label=PyPI&color=3776AB"></a>
  <a href="https://crates.io/crates/dongler"><img alt="crates.io" src="https://img.shields.io/crates/v/dongler?style=for-the-badge&logo=rust&logoColor=white&label=crates.io&color=8B4513"></a>
  <a href="https://www.npmjs.com/package/@cristianexer/dongler"><img alt="npm" src="https://img.shields.io/npm/v/@cristianexer/dongler?style=for-the-badge&logo=npm&logoColor=white&label=npm&color=CB3837"></a>
</p>

<p align="center">
  <a href="https://github.com/cristianexer/dongler/releases"><img alt="Release" src="https://img.shields.io/github/v/release/cristianexer/dongler?style=flat-square&logo=github&label=release&color=17b9c8"></a>
  <a href="https://github.com/cristianexer/dongler/actions/workflows/workflow.yml"><img alt="Build" src="https://img.shields.io/github/actions/workflow/status/cristianexer/dongler/workflow.yml?branch=main&style=flat-square&logo=githubactions&logoColor=white&label=build"></a>
  <a href="https://github.com/cristianexer/dongler/blob/main/LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-0aa?style=flat-square"></a>
  <a href="https://cristianexer.github.io/dongler/"><img alt="Docs" src="https://img.shields.io/badge/docs-online-5ef0e6?style=flat-square&logo=readthedocs&logoColor=white"></a>
</p>

<p align="center">
  <a href="https://cristianexer.github.io/dongler/docs/intro"><b>Documentation</b></a>&nbsp; ·&nbsp;
  <a href="https://cristianexer.github.io/dongler/docs/quickstart">Quick start</a>&nbsp; ·&nbsp;
  <a href="https://cristianexer.github.io/dongler/docs/api">API reference</a>&nbsp; ·&nbsp;
  <a href="https://cristianexer.github.io/dongler/llms.txt">LLM context</a>
</p>

---

Dongler is built around a **path-first workflow**: load a file, inspect the document object when
you need to, then render the output format your pipeline wants. One Rust core powers the **CLI,
Python, TypeScript, and Rust** APIs, so the extraction model is identical everywhere.

## Install

```bash
cargo install dongler                  # CLI + Rust
pip install dongler                    # Python
npm install @cristianexer/dongler      # Node / TypeScript
```

For the Rust library, depend on `dongler-core`. The public `dongler` crate is the CLI package.

To run extraction in the browser or another WebAssembly host, build the
`dongler-wasm` crate (`make build-wasm`). It exposes the same engine over an
in-memory byte API, so files can be parsed client-side with no server. See
[`crates/dongler-wasm/README.md`](crates/dongler-wasm/README.md).

## Parse a PDF

<table>
<tr>
<td valign="top">

**Python**

```python
import dongler

doc = dongler.load("report.pdf")
markdown = doc.to_markdown()
latex = doc.to_latex()
data = doc.to_dict()
```

</td>
<td valign="top">

**TypeScript**

```ts
import { load } from "@cristianexer/dongler";

const doc = load("report.pdf");
const markdown = doc.toMarkdown();
const latex = doc.toLatex();
const data = doc.toObject();
```

</td>
<td valign="top">

**Rust**

```rust
use dongler_core::load_path;

let doc = load_path("report.pdf")?;
println!("{}", doc.to_markdown()?);
```

</td>
</tr>
</table>

## What you get

<table>
<tr>
<td width="50%" valign="top">

**📄 Markdown · LaTeX · JSON**
<br>Three renderers from one document object — headings, tables, lists, figures, and emphasis.

**⚡ Native speed, local runtime**
<br>A custom Rust PDF parser with `rayon` page-parallelism. No hosted service, API key, LLM, or OCR for born-digital PDFs.

**🧱 Structured document model**
<br>Page, block, table, image, span, warning, and metadata fields — with source anchors back to PDF objects.

</td>
<td width="50%" valign="top">

**🧩 One API across stacks**
<br>The same extraction model in Python, Node.js, Rust, and the CLI.

**📦 Pipeline-friendly batches**
<br>Batch APIs return one result per file — a single bad document never stops the job.

**🔌 Beyond PDF**
<br>Native extraction for DOCX/XLSX/PPTX, ODT/ODS/ODP, HTML/XML, EML, JSON/JSONL, CSV/TSV, images, and archives.

</td>
</tr>
</table>

## Why Dongler

Use Dongler when the job starts with a document path and the next step needs useful text quickly:

- Convert PDFs to Markdown for indexing, review, or RAG ingestion.
- Keep page/block/table/image metadata available through JSON.
- Run locally in scripts, services, queues, notebooks, and shell workflows.
- Use the same extraction model across Python, Node.js, Rust, and the CLI.

## Supported inputs

Dongler focuses on digitally born PDFs and also supports native extraction for DOCX, XLSX, PPTX,
ODT/ODS/ODP, HTML/XML, EML, JSON/JSONL, CSV/TSV, image metadata including TIFF, and plain
text/Markdown/TeX. It also reads gzip-compressed text/JSON/XML/CSV corpus files, bare gzip source
files, and zip/tar/tar.gz source packages. Legacy binary Office and Outlook containers are detected
and return explicit planned-format errors until their engines land.

## Batch processing

One result per file — a bad or unsupported document does not stop the batch.

```python
import dongler

for result in dongler.load_many(["notes.txt", "invoice.pdf"]):
    if result["ok"]:
        print(result["document"].to_markdown())
    else:
        print(f"{result['path']}: {result['error']}")
```

## CLI

```bash
dongler --version
dongler inspect invoice.pdf
dongler extract report.docx --format markdown
dongler extract book.xlsx   --format json
dongler extract notes.txt   --format latex
```

PDF extraction through the CLI uses the same Rust-native engine as the Rust, Python, and TypeScript
packages.

## Documentation

- [Documentation](https://cristianexer.github.io/dongler/docs/intro)
- [Quick start](https://cristianexer.github.io/dongler/docs/quickstart)
- [Developer guide](https://cristianexer.github.io/dongler/docs/developer-guide)
- [API reference](https://cristianexer.github.io/dongler/docs/api)
- [LLM context](https://cristianexer.github.io/dongler/llms.txt)

## Benchmarks

Dongler sustains **~90 born-digital pages/second** on a single host with no GPU, and on
**olmOCR-Bench** (1,403 real PDFs, 7,019 unit checks) its table-structure pass rate improved
**+9.7% relative** over the previous release on the identical harness. Full results, methodology,
and before/after examples are on the [**benchmarks page**](https://cristianexer.github.io/dongler/docs/benchmarks).

<!-- BENCHMARKS:START -->
_Generated by `scripts/run-benchmarks.py` on 2026-05-28 19:56:50 BST. Local cache: 1894.9 MB. All discovered files per dataset. olmOCR-Bench re-measured 2026-06-11 after the table-extraction work; the full table regenerates on the next release run._

Coverage is `parse / bbox / anchors`. Ground-truth accuracy is token-F1, olmOCR unit-check pass rate, or full-image IoU; `n/a` means no local target signal. Detailed task names, discovery counts, native scores, and notes are recorded in `eval/out/benchmarks/latest.json`.

| Dataset | Status | Local data | Docs eval | Coverage | Pages/sec | GT accuracy |
| --- | --- | ---: | ---: | --- | ---: | ---: |
| DocLayNet | missing | 0.0 MB | 0 | n/a / n/a / n/a | n/a | n/a |
| PubLayNet | missing | 0.0 MB | 0 | n/a / n/a / n/a | n/a | n/a |
| DocBank | ok | 735.6 MB | 200 | 100.0% / 100.0% / 100.0% | 81.94 | 89.5% |
| PubTabNet | missing | 0.0 MB | 0 | n/a / n/a / n/a | n/a | n/a |
| PubTables-1M | missing | 0.0 MB | 0 | n/a / n/a / n/a | n/a | n/a |
| TableBank | ok | 1.6 MB | 10 | 100.0% / 100.0% / 100.0% | 193.45 | 100.0% |
| FUNSD | ok | 42.6 MB | 200 | 100.0% / 48.9% / 100.0% | 96.09 | 100.0% |
| SROIE | ok | 627.3 MB | 1264 | 100.0% / 92.7% / 100.0% | 231.85 | 100.0% |
| RVL-CDIP | missing | 0.0 MB | 0 | n/a / n/a / n/a | n/a | n/a |
| READoc | ok | 39.9 MB | 959 | 100.0% / n/a / n/a | 96.86 | 100.0% |
| OmniDocBench | ok | 40.3 MB | 1 | 100.0% / 100.0% / 100.0% | 1030.96 | 88.5% |
| olmOCR-Bench | ok | 340.5 MB | 1403 | 100.0% / 100.0% / 100.0% | 21.15 | 22.7% |
| ckorzen benchmark | ok | 67.1 MB | 192 | 100.0% / 15.4% / 100.0% | 100.37 | 88.4% |
| S2ORC | missing | 0.0 MB | 0 | n/a / n/a / n/a | n/a | n/a |
| PMC OA | missing | 0.0 MB | 0 | n/a / n/a / n/a | n/a | n/a |
| arXiv source/PDF | missing | 0.0 MB | 0 | n/a / n/a / n/a | n/a | n/a |
<!-- BENCHMARKS:END -->

### Extraction-quality improvements

A controlled A/B of the current parser against the previous release baseline — run on the full
olmOCR-Bench corpus (1403 real PDFs, 7019 unit checks, identical harness and release build) —
isolates the gains from the recent text-spacing and table-structure work:

| Signal | Before | After |
| --- | ---: | ---: |
| olmOCR table-structure checks passed | 59.7% | **65.5%** (+9.7% relative) |
| olmOCR reading-order checks passed | 30.7% | **32.0%** |
| Overall olmOCR checks passed | 1562 / 7019 | **1595 / 7019** |
| Throughput (born-digital) | ~90 pages/sec | ~90 pages/sec |

Born-digital word segmentation is fixed end to end (`UNITEDSTATES` → `UNITED STATES`,
`Netincome` → `Net income`, `fi scal` → `fiscal`), and multi-section financial statements now
extract as a single aligned table — in Markdown, LaTeX, **and** JSON — instead of a label column
followed by a detached block of numbers. See the
[benchmarks page](https://cristianexer.github.io/dongler/docs/benchmarks) for the full breakdown.

## License

Dongler is MIT licensed. Copyright (c) 2026 Daniel Fat. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE)
for the full notice text.
