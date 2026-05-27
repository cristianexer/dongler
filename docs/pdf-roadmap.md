---
sidebar_position: 7
---

# PDF Roadmap

Dongler should become useful by doing one hard workflow well: extracting text,
tables, layout, and metadata from PDFs into Markdown and LaTeX.

The path/object API already exists. The missing piece is the PDF extraction
engine behind `load("file.pdf")`.

## Phase 1: Text Extraction

Goal: produce readable Markdown and LaTeX from ordinary digital PDFs.

Expected behavior:

- Load PDF files through a dedicated Rust loader.
- Extract text per page.
- Preserve stable reading order for common single-column and two-column pages.
- Populate page count, source path, format, engine, character count, word count,
  and block count.
- Return useful errors for encrypted, scanned, malformed, or unsupported PDFs.

## Phase 2: Table Extraction

Goal: convert common PDF tables into `TableBlock` values that render cleanly.

Expected behavior:

- Detect simple ruled tables.
- Detect common whitespace-aligned tables.
- Preserve headers and row cells where confidence is high.
- Fall back to text blocks when table confidence is low.
- Render tables as Markdown tables and LaTeX `tabular` blocks.

## Phase 3: Layout Metadata

Goal: retain enough layout information for downstream tools to reason about the
document.

Likely IR additions:

- Page dimensions.
- Block bounding boxes.
- Reading-order indexes.
- Optional text spans for font/style data.
- Confidence values for layout and table detection.

## Phase 4: Broader Documents

After PDFs are useful, add loaders and engines for:

- Word documents.
- Excel spreadsheets.
- HTML pages.
- Images through OCR.
- Email messages and attachments.

Each new format should still produce the same `Document` IR and use the same
Markdown and LaTeX renderers where possible.
