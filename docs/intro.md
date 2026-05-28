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

PDF extraction is the product focus. Dongler `0.3.0` ships a native Rust PDF
path alongside `.txt`/Markdown/TeX, DOCX, XLSX, PPTX, ODT/ODS/ODP, HTML/XML,
EML, JSON/JSONL, CSV/TSV, gzip-compressed text corpus files, bare gzip source
files, and image metadata extraction, object APIs, batch APIs, and renderers.

## What Works Today

- Load `.txt`, `.text`, `.md`, `.tex`, digitally born `.pdf`, `.docx`, `.xlsx`,
  `.pptx`, `.odt`, `.ods`, `.odp`, `.html`, `.xml`, `.eml`, `.json`, `.jsonl`,
  `.csv`, `.tsv`, and common image files by path.
- Parse text into Dongler's document IR.
- Extract PDF text, page geometry, source anchors, image positions, and simple
  table blocks.
- Extract DOCX paragraphs, XLSX rows, PPTX slide text, OpenDocument text,
  spreadsheet rows, and presentation text, HTML/XML text, EML subject/body
  text, JSON/JSONL text and annotation blocks, and CSV/TSV rows or OCR box
  records.
- Extract supported text/XML/TeX resources from `.zip`, `.tar`, `.tar.gz`,
  `.tgz`, and bare `.gz` source packages.
- Extract image page dimensions and image assets, including TIFF metadata,
  without OCR dependencies.
- Render Markdown, LaTeX, and JSON from a document object.
- Batch process paths with per-file success and error results.
- Detect legacy binary Office/Outlook formats with clear planned-format errors.

## What Dongler Is Optimized For

- PDF text extraction.
- Table extraction into structured table blocks.
- Clean Markdown and LaTeX output.
- A Rust core with consistent Python and TypeScript bindings.
- Honest warnings when a PDF structure is detected but only partially modeled.
