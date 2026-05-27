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

PDF extraction is the product focus. Dongler `0.2.0` ships a native Rust PDF
path alongside `.txt` extraction, object APIs, batch APIs, and renderers.

## What Works Today

- Load `.txt`, `.text`, and digitally born `.pdf` files by path.
- Parse text into Dongler's document IR.
- Extract PDF text, page geometry, source anchors, image positions, and simple
  table blocks.
- Render Markdown, LaTeX, and JSON from a document object.
- Batch process paths with per-file success and error results.
- Detect other common formats with clear planned-format errors.

## What Dongler Is Optimized For

- PDF text extraction.
- Table extraction into structured table blocks.
- Clean Markdown and LaTeX output.
- A Rust core with consistent Python and TypeScript bindings.
- Honest warnings when a PDF structure is detected but only partially modeled.
