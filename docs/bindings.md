---
sidebar_position: 6
---

# Python and TypeScript Bindings

The Python and TypeScript packages are thin wrappers over the Rust core. They do
not duplicate extraction logic.

## Python Objects

```python
import dongler

doc = dongler.load("notes.txt")

doc.metadata
doc.pages
doc.to_markdown()
doc.to_latex()
doc.to_json()
doc.to_dict()
```

`to_json()` returns a JSON string. `to_dict()` returns a native Python dict.

## TypeScript Objects

```ts
import { load } from "@cristianexer/dongler";

const doc = load("notes.txt");

doc.metadata;
doc.pages;
doc.toMarkdown();
doc.toLatex();
doc.toJson();
doc.toObject();
```

`toJson()` returns a JSON string. `toObject()` returns the typed document data.

## PDF Usage

The same object API works for PDFs:

```ts
const doc = load("invoice.pdf");
const markdown = doc.toMarkdown();
const latex = doc.toLatex();
```

PDF documents expose the same render methods plus rich page/block fields such as
`bbox`, `source_anchors`, `images`, and `warnings`.

## Package Names

- Python: `dongler`
- TypeScript/Node.js: `@cristianexer/dongler`
- Rust library: `dongler-core`
- Rust CLI: `dongler`
