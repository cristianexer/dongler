---
sidebar_position: 3
---

# PDF Workflow

Dongler includes a Rust-native PDF extraction path for digitally born PDFs:
text, page geometry, source anchors, table structure, image positions, and
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
- Convert positioned and ruled tables into table blocks.
- Carry useful metadata.
- Preserve block/page bounding boxes for citations.
- Record image object positions and source anchors.
- Render clean Markdown and LaTeX from the same document object.

## Practical Checks

When you integrate Dongler into a PDF pipeline, start with these checks:

```python
doc = dongler.load("report.pdf")
data = doc.to_dict()

print(data["metadata"]["word_count"])
print(data["metadata"]["block_count"])
for warning in data.get("warnings", []):
    print(warning)
```

If `word_count` and `block_count` are both zero for a visually simple PDF, the
file may be scanned, image-only, encrypted in an unusual way, or using a PDF
encoding path Dongler does not yet model. Keep a small fixture for that document
and file an issue with the PDF if it can be shared.

## Native-First Scope

The v1 engine is deterministic and native-first. OCR and VLM/LLM repair are not
default dependencies; low-confidence or unsupported PDF structures are surfaced
as warnings and can be evaluated separately.
