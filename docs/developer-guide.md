---
sidebar_position: 3
---

# Developer Guide

Dongler is built for a simple integration path: install one package, pass it a
document path, and choose the output your application needs.

## Pick a Runtime

Use the runtime that matches the rest of your pipeline:

- Python: best for notebooks, ingestion jobs, data pipelines, and quick PDF to
  Markdown conversion.
- TypeScript: best for Node.js services, queues, and application backends.
- Rust: best when you want direct control over the core document IR.
- CLI: best for smoke tests, shell scripts, and inspecting what Dongler sees.

All of these call the same Rust extraction engine.

## Parse a PDF

```python
import dongler

doc = dongler.load("report.pdf")

markdown = doc.to_markdown()
json_payload = doc.to_json()
data = doc.to_dict()
```

The Markdown renderer is the fastest path when you need readable text for
review, indexing, or downstream text processing. The JSON output is better when
you need metadata, page numbers, block types, bounding boxes, warnings, images,
or table cells.

## Handle Failures

Treat document extraction as I/O. Files can be encrypted, scanned, malformed,
or unsupported.

```python
import dongler

try:
    doc = dongler.load("report.pdf")
except Exception as exc:
    print(f"Could not extract report.pdf: {exc}")
else:
    print(doc.to_markdown())
```

For ingestion jobs, prefer `load_many()` so one bad file does not stop the
batch:

```python
for result in dongler.load_many(["report.pdf", "invoice.pdf", "notes.txt"]):
    if result["ok"]:
        process(result["document"].to_markdown())
    else:
        log_error(result["path"], result["error"])
```

## Inspect Output Quality

Start with the document metadata:

```python
doc = dongler.load("report.pdf")
data = doc.to_dict()

print(data["metadata"]["character_count"])
print(data["metadata"]["word_count"])
print(data["metadata"]["block_count"])
print(data.get("warnings", []))
```

For a normal digitally born PDF, zero words and zero blocks usually means the
file is scanned, image-only, encrypted in an unusual way, or using PDF features
Dongler does not yet model. Keep a minimal fixture for documents like that so
you can verify future upgrades.

## Choose an Output Format

Use Markdown when the next step is search, display, review, RAG ingestion, or
plain text processing.

Use LaTeX when the document contains technical text and you want a renderer that
keeps more document-like structure.

Use JSON when your application needs the document IR: pages, blocks, tables,
images, warnings, metadata, and anchors.

## Keep It Local

Dongler's default PDF path is native and local. It does not send documents to a
hosted API and does not require an OCR or LLM dependency for digitally born
PDFs. If your workload includes scanned PDFs, add OCR as a separate step and
feed the OCR output into your own pipeline until Dongler's optional OCR path is
ready.
