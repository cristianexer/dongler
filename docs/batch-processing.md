---
sidebar_position: 4
---

# Batch Processing

Use batch processing when converting folders, queues, or mixed document sets.
Dongler returns one result per path so one failed file does not stop the batch.

## Python

```python
import dongler

for result in dongler.load_many(["notes.txt", "invoice.pdf"]):
    if result["ok"]:
        print(result["document"].to_markdown())
    else:
        print(f"{result['path']}: {result['error']}")
```

## TypeScript

```ts
import { loadMany } from "@cristianexer/dongler";

for (const result of loadMany(["notes.txt", "invoice.pdf"])) {
  if (result.ok) {
    console.log(result.document!.toMarkdown());
  } else {
    console.error(`${result.path}: ${result.error}`);
  }
}
```

## Rust

```rust
use dongler_core::load_many;

for result in load_many(["notes.txt", "invoice.pdf"]) {
    if result.ok {
        println!("{}", result.document.unwrap().to_markdown().unwrap());
    } else {
        eprintln!("{}: {}", result.path, result.error.unwrap());
    }
}
```

Batch results include `path`, `ok`, `document`, and `error`.
