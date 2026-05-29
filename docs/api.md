---
sidebar_position: 5
---

# API Reference

Dongler exposes an object API for path-based extraction and keeps the original
text helper functions for compatibility.

## Object API

Python:

```python
import dongler

doc = dongler.load("notes.txt")
doc.to_markdown()
doc.to_latex()
doc.to_json()
doc.to_dict()
```

TypeScript:

```ts
import { load } from "@cristianexer/dongler";

const doc = load("notes.txt");
doc.toMarkdown();
doc.toLatex();
doc.toJson();
doc.toObject();
```

Rust:

```rust
let doc = dongler_core::load_path("notes.txt")?;
doc.to_markdown()?;
doc.to_latex()?;
doc.to_json()?;
```

## Batch API

Batch processing returns one result per path. A failed or unsupported file does
not stop the batch.

Python:

```python
results = dongler.load_many(["notes.txt", "invoice.pdf"])
```

TypeScript:

```ts
const results = loadMany(["notes.txt", "invoice.pdf"]);
```

Rust:

```rust
let results = dongler_core::load_many(["notes.txt", "invoice.pdf"]);
```

Each result has:

- `path`
- `ok`
- `document`
- `error`

## Choosing an Output

- Use Markdown when indexing, displaying, reviewing, or passing document text to
  downstream text systems.
- Use LaTeX when preserving technical text, formulas, or document-oriented
  rendering is more important.
- Use JSON when you need page numbers, block types, bounding boxes, warnings,
  image references, or table cells.

## Compatibility Helpers

These functions still operate on in-memory text:

- `parse_text`
- `to_markdown`
- `to_latex`
- `to_json`
- `detect_format`

## Document IR

The document object wraps Dongler's serializable IR:

```text
Document
  schema_version
  metadata
  pages[]
  assets[]
  warnings[]

Page
  number
  width
  height
  rotation
  bbox
  blocks[]
  images[]
  assets[]
  warnings[]

Block
  text | table | figure
```

PDF blocks include source anchors and optional bounding boxes so rendered
content can point back to page regions. `TableBlock` renders to Markdown and
LaTeX when its extracted grid is rectangular.
