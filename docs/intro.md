---
sidebar_position: 1
---

# Introduction

Dongler is a Rust-native document extraction engine with Python and TypeScript
bindings. The public workflow is path-first:

```python
import dongler

doc = dongler.load("document.pdf")
markdown = doc.to_markdown()
latex = doc.to_latex()
```

PDF extraction is the product focus. Dongler `0.1.0` does not extract PDFs yet;
it ships the package structure, object API, batch API, renderers, and a working
`.txt` extraction path that the PDF engine will build on.

## What Works Today

- Load `.txt` and `.text` files by path.
- Parse text into Dongler's document IR.
- Render Markdown, LaTeX, and JSON from a document object.
- Batch process paths with per-file success and error results.
- Detect PDFs and other common formats, with clear planned-format errors.

## What Dongler Is Optimized For

- PDF text extraction.
- Table extraction into structured table blocks.
- Clean Markdown and LaTeX output.
- A Rust core with consistent Python and TypeScript bindings.
- Honest errors when a file type is detected but not implemented yet.
