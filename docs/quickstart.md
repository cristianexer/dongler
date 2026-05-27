---
sidebar_position: 2
---

# Quick Start

Install the package for the ecosystem you use:

```bash
cargo install dongler
pip install dongler
npm install @cristianexer/dongler
```

For Rust library usage, depend on `dongler-core`. The `dongler` crate is the CLI
package.

## Python

```python
import dongler

doc = dongler.load("notes.txt")

print(doc.metadata["block_count"])
print(doc.to_markdown())
print(doc.to_latex())
```

## TypeScript

```ts
import { load } from "@cristianexer/dongler";

const doc = load("notes.txt");

console.log(doc.metadata.block_count);
console.log(doc.toMarkdown());
console.log(doc.toLatex());
```

## Rust

```rust
use dongler_core::load_path;

fn main() -> dongler_core::Result<()> {
    let doc = load_path("notes.txt")?;

    println!("blocks: {}", doc.metadata.block_count);
    println!("{}", doc.to_markdown()?);
    Ok(())
}
```

## CLI

```bash
dongler inspect notes.txt
dongler extract notes.txt --format markdown
dongler extract notes.txt --format latex
dongler extract notes.txt --format json
```

PDF paths are detected today, but extraction is planned:

```bash
dongler inspect invoice.pdf
```
