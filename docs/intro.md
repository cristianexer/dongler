---
sidebar_position: 1
---

# Introduction

Dongler is a fast, local PDF extraction package for developers who want one
installable tool that turns documents into Markdown, LaTeX, or structured JSON.
The public workflow is deliberately path-first:

```python
import dongler

doc = dongler.load("document.pdf")
markdown = doc.to_markdown()
latex = doc.to_latex()
```

PDF extraction is the product focus. The same Rust core also handles
`.txt`/Markdown/TeX, DOCX, XLSX, PPTX, ODT/ODS/ODP, HTML/XML, EML,
JSON/JSONL, CSV/TSV, gzip-compressed text corpus files, bare gzip source files,
image metadata extraction, object APIs, batch APIs, and renderers.

The default path is local and deterministic. There is no hosted service, API
key, OCR dependency, or LLM dependency required for the common PDF-to-Markdown
workflow.

## Why Use Dongler

- You have PDFs and need useful Markdown quickly.
- You want Python, Node.js, Rust, and CLI entrypoints over the same engine.
- You need structured JSON with pages, blocks, tables, images, warnings, and
  source anchors when Markdown is not enough.
- You want local extraction without sending documents to a hosted API.
- You want batch APIs that keep going when one file is malformed or unsupported.

## What Works Today

- Load `.txt`, `.text`, `.md`, `.tex`, digitally born `.pdf`, `.docx`, `.xlsx`,
  `.pptx`, `.odt`, `.ods`, `.odp`, `.html`, `.xml`, `.eml`, `.json`, `.jsonl`,
  `.csv`, `.tsv`, and common image files by path.
- Extract PDF text, page geometry, source anchors, image positions, and simple
  table blocks.
- Extract Office, OpenDocument, HTML/XML, EML, JSON/JSONL, CSV/TSV, source
  archive, gzip, and image metadata inputs into the same document IR.
- Render Markdown, LaTeX, and JSON from a document object.
- Batch process paths with per-file success and error results.
- Detect legacy binary Office/Outlook formats with clear planned-format errors.

## When to Use Dongler

Use Dongler when you want a package you can install into a developer workflow:

- Convert user-supplied PDFs to Markdown before indexing or review.
- Extract tables and page-aware structure for downstream processing.
- Batch through mixed folders without one bad file stopping the run.
- Keep extraction local instead of sending documents to a service.

Dongler is still evolving. Scanned PDFs and image-only pages need OCR outside
the default native path today, and complex layout recovery is being improved
incrementally.
