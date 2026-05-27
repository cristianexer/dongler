---
sidebar_position: 3
---

# PDF Workflow

Dongler includes a Rust-native PDF extraction path for digitally born PDFs:
text, page geometry, source anchors, basic table structure, image positions, and
metadata rendered to Markdown, JSON, and LaTeX.

The intended workflow is:

```python
import dongler

doc = dongler.load("invoice.pdf")
markdown = doc.to_markdown()
latex = doc.to_latex()
```

## PDF Output Goals

- Preserve readable page order.
- Extract text into paragraphs and sections.
- Convert simple positioned tables into table blocks.
- Carry useful metadata.
- Preserve block/page bounding boxes for citations.
- Record image object positions and source anchors.
- Render clean Markdown and LaTeX from the same document object.

## Native-First Scope

The v1 engine is deterministic and native-first. OCR and VLM/LLM repair are not
default dependencies; low-confidence or unsupported PDF structures are surfaced
as warnings and can be evaluated separately.
