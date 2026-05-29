---
sidebar_position: 6
---

# Language Bindings

The Python and TypeScript packages are thin wrappers over the Rust core. The
Rust library exposes the same document model directly, and the CLI uses the same
engine for inspection and extraction.

## Python

```python
import dongler

doc = dongler.load("report.pdf")

doc.metadata
doc.pages
doc.to_markdown()
doc.to_latex()
doc.to_json()
doc.to_dict()
```

`to_json()` returns a JSON string. `to_dict()` returns a native Python dict.

## TypeScript

```ts
import { load } from "@cristianexer/dongler";

const doc = load("report.pdf");

doc.metadata;
doc.pages;
doc.toMarkdown();
doc.toLatex();
doc.toJson();
doc.toObject();
```

`toJson()` returns a JSON string. `toObject()` returns the typed document data.

## Rust

```rust
let doc = dongler_core::load_path("report.pdf")?;
let markdown = doc.to_markdown()?;
let json = doc.to_json()?;
```

PDF documents expose the same render methods plus rich page/block fields such as
`bbox`, `source_anchors`, `images`, and `warnings`.

## Package Names

- Python: `dongler`
- TypeScript/Node.js: `@cristianexer/dongler`
- Rust library: `dongler-core`
- Rust CLI: `dongler`
