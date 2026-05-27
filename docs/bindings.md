---
sidebar_position: 6
---

# Python and TypeScript

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
import { load } from "dongler";

const doc = load("notes.txt");

doc.metadata;
doc.pages;
doc.toMarkdown();
doc.toLatex();
doc.toJson();
doc.toObject();
```

`toJson()` returns a JSON string. `toObject()` returns the typed document data.

## Planned PDF Usage

The same object API is intended for PDFs:

```ts
const doc = load("invoice.pdf");
const markdown = doc.toMarkdown();
const latex = doc.toLatex();
```

Today, PDF paths return a planned-format error. That error comes from the Rust
core and is surfaced consistently through both bindings.
