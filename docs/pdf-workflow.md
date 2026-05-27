---
sidebar_position: 3
---

# PDF Workflow

Dongler is being built for PDF extraction first: text, tables, layout, and
metadata rendered to Markdown and LaTeX.

The intended workflow is:

```python
import dongler

doc = dongler.load("invoice.pdf")
markdown = doc.to_markdown()
latex = doc.to_latex()
```

In `0.1.0`, this API shape exists, but PDF extraction returns:

```text
pdf extraction is planned but not implemented yet
```

## Why the API Exists Before the PDF Engine

The stable user workflow should not change when the PDF engine lands. The text
engine proves the same path:

```python
doc = dongler.load("notes.txt")
print(doc.to_markdown())
```

The PDF engine will plug into the existing format detection, source loading,
document IR, and renderer boundaries.

## PDF Output Goals

- Preserve readable page order.
- Extract text into paragraphs and sections.
- Convert tables into table blocks.
- Carry useful metadata.
- Render clean Markdown and LaTeX from the same document object.
