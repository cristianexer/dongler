# dongler

TypeScript bindings for Dongler, a Rust-native document extraction engine.

Dongler is designed around a simple workflow: load a document path, receive a
document object, then render Markdown, LaTeX, or JSON from that object.

Created by Daniel Fat.

## Status

The npm package calls the Rust core through a NAPI native addon. It supports the
same `.txt` extraction path as the Rust and Python packages today. PDF is the
primary product target, but PDF extraction currently returns a planned-format
error.

## Install

```bash
npm install dongler
```

## Planned PDF Workflow

The API is already shaped for PDF extraction:

```ts
import { load } from "dongler";

const doc = load("invoice.pdf");
const markdown = doc.toMarkdown();
const latex = doc.toLatex();
```

Until the PDF engine lands, loading a PDF throws an error such as
`pdf extraction is planned but not implemented yet`.

## Works Today

```ts
import { load } from "dongler";

const doc = load("notes.txt");

console.log(doc.metadata.block_count);
console.log(doc.toMarkdown());
console.log(doc.toLatex());
console.log(doc.toJson());
```

## Batch Processing

```ts
import { loadMany } from "dongler";

const results = loadMany(["notes.txt", "invoice.pdf"]);

for (const result of results) {
  if (result.ok) {
    console.log(result.document!.toMarkdown());
  } else {
    console.error(`${result.path}: ${result.error}`);
  }
}
```

## Compatibility API

The original text helpers remain available:

```ts
import { parseText, toLatex, toMarkdown } from "dongler";

const doc = parseText("Hello from Dongler");
const markdown = toMarkdown("Hello from Dongler");
const latex = toLatex("Revenue is 100%");
```
