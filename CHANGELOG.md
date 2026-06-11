# Changelog

## Unreleased

- Added a `dongler-wasm` crate with WebAssembly (`wasm32-unknown-unknown`)
  bindings, so the extraction engine runs in the browser or any wasm host
  directly from an in-memory byte buffer with no filesystem.
- Added a filesystem-free `extract_bytes` API to `dongler-core` and made the
  rayon-based parallel decoding optional behind a default `parallel` feature so
  the core builds for threadless targets.

## 0.3.0

- Expanded the Rust-native extraction engine to DOCX, XLSX, PPTX, OpenDocument,
  HTML/XML, EML, JSON/JSONL, CSV/TSV, gzip text/corpus files, source archives,
  image metadata, TIFF dimensions, and richer dataset annotation formats.
- Improved PDF extraction ordering, multi-column handling, front matter,
  source anchors, tables, image positions, math glyph repair, and Markdown,
  LaTeX, and JSON rendering.
- Added local benchmark coverage across DocBank, TableBank, FUNSD, SROIE,
  READoc, OmniDocBench, olmOCR-Bench, and the ckorzen benchmark, with uncapped
  README benchmark recalculation across all discovered local documents.
- Added visual comparison artifact generation for original, Markdown, and
  LaTeX extraction outputs.

## 0.1.0

- Initial Rust-first document extraction workspace.
- Added working plain-text extraction, Markdown, JSON, and LaTeX rendering.
- Added Python and TypeScript bindings backed by the Rust core.
