---
sidebar_position: 5
---

# API Reference

Dongler exposes an object API for path-based extraction and keeps the original
text helper functions for compatibility.

## Object API

Python:

```python
doc = dongler.load("notes.txt")
doc.to_markdown()
doc.to_latex()
doc.to_json()
```

TypeScript:

```ts
const doc = load("notes.txt");
doc.toMarkdown();
doc.toLatex();
doc.toJson();
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
  metadata
  pages[]

Page
  number
  blocks[]

Block
  text | table
```

`TableBlock` already renders to Markdown and LaTeX. PDF table extraction will
produce these table blocks once implemented.
