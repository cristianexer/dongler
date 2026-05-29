# dongler

TypeScript bindings for Dongler, a fast Rust-native document extraction engine.

Dongler is designed around a simple workflow: load a document path, receive a
document object, then render Markdown, LaTeX, or JSON from that object.

## Status

The npm package calls the Rust core through a NAPI native addon. It supports the
same native PDF and text extraction paths as the Rust and Python packages.

## Install

```bash
npm install @cristianexer/dongler
```

## PDF Workflow

Parse a PDF into Markdown, LaTeX, or JSON:

```ts
import { load } from "@cristianexer/dongler";

const doc = load("report.pdf");
const markdown = doc.toMarkdown();
const latex = doc.toLatex();
const data = doc.toObject();
```

PDF documents include page geometry, block source anchors, warnings, and image
positions in `doc.toObject()` / `doc.toJson()`.

## Works Today

```ts
import { load } from "@cristianexer/dongler";

const doc = load("notes.txt");

console.log(doc.metadata.block_count);
console.log(doc.toMarkdown());
console.log(doc.toLatex());
console.log(doc.toJson());
```

## Batch Processing

```ts
import { loadMany } from "@cristianexer/dongler";

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
import { parseText, toLatex, toMarkdown } from "@cristianexer/dongler";

const doc = parseText("Hello from Dongler");
const markdown = toMarkdown("Hello from Dongler");
const latex = toLatex("Revenue is 100%");
```

## License

Dongler is MIT licensed. Copyright (c) 2026 Daniel Fat. See `LICENSE` and
`NOTICE` for the full notice text.
