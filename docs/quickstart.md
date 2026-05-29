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

doc = dongler.load("report.pdf")

print(doc.metadata["block_count"])
print(doc.to_markdown())
print(doc.to_latex())
```

`to_dict()` gives you the full document object for custom pipelines:

```python
data = doc.to_dict()
for page in data["pages"]:
    print(page["number"], len(page["blocks"]))
```

## TypeScript

```ts
import { load } from "@cristianexer/dongler";

const doc = load("report.pdf");

console.log(doc.metadata.block_count);
console.log(doc.toMarkdown());
console.log(doc.toLatex());
```

## Rust

```rust
use dongler_core::load_path;

fn main() -> dongler_core::Result<()> {
    let doc = load_path("report.pdf")?;

    println!("blocks: {}", doc.metadata.block_count);
    println!("{}", doc.to_markdown()?);
    Ok(())
}
```

## CLI

```bash
dongler inspect report.pdf
dongler extract report.pdf --format markdown
dongler extract report.pdf --format latex
dongler extract report.pdf --format json
```

The CLI uses the same native extraction engine as the language bindings.

## Next Steps

- Read the [developer guide](./developer-guide.md) for pipeline patterns.
- Use the [API reference](./api.md) when you need document object fields.
- Use the [PDF workflow](./pdf-workflow.md) for PDF-specific behavior and scope.
