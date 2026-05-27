# dongler

TypeScript bindings for Dongler, a Rust-native document extraction engine.

Created by Daniel Fat.

## Status

The current npm package calls the Rust core through a NAPI native addon. It
supports the same initial text extraction pipeline as the Rust crate and Python
package:

- parse text into Dongler's document IR
- render Markdown
- render LaTeX
- render JSON
- detect common document formats

PDF extraction is the primary next target. The package detects PDFs today, but
PDF extraction returns a planned-format error until the Rust PDF engine lands.

## Usage

```ts
import { parseText, toLatex, toMarkdown } from "dongler";

const doc = parseText("Hello from Dongler\n\nSecond paragraph");
console.log(doc.metadata.block_count);
console.log(toMarkdown("Hello from Dongler"));
console.log(toLatex("Revenue is 100%"));
```
