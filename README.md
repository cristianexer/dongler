<p align="center">
  <img src="assets/logo.png" alt="Dongler logo" width="132">
</p>

# Dongler

Dongler is a Rust-native document extraction engine with Python and TypeScript
bindings. It is built for the workflow developers actually need: load a
document path, extract structure once, then render clean Markdown or LaTeX from
the same document object.

## Install

```bash
cargo install dongler
pip install dongler
npm install @cristianexer/dongler
```

For Rust library usage, depend on `dongler-core`. The public `dongler` crate is
the CLI package.

## How to Use

Dongler supports a wide variety of document formats, including PDFs, Microsoft Word documents, HTML files, and plain text. The same API works across all
formats, so you can use the same code to extract Markdown from a PDF invoice or a plain text note.


Python:

```python
import dongler

doc = dongler.load("invoice.pdf")
markdown = doc.to_markdown()
latex = doc.to_latex()
```

TypeScript:

```ts
import { load } from "@cristianexer/dongler";

const doc = load("invoice.pdf");
const markdown = doc.toMarkdown();
const latex = doc.toLatex();
```

Rust:

```rust
use dongler_core::load_path;

fn main() -> dongler_core::Result<()> {
    let doc = load_path("invoice.pdf")?;
    println!("{}", doc.to_markdown()?);
    Ok(())
}
```

## Batch Processing

Batch processing returns one result per file. One bad or unsupported document
does not stop the batch.

Python:

```python
import dongler

for result in dongler.load_many(["notes.txt", "invoice.pdf"]):
    if result["ok"]:
        print(result["document"].to_markdown())
    else:
        print(f"{result['path']}: {result['error']}")
```

TypeScript:

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

Rust:

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

## CLI

```bash
dongler --version
dongler inspect notes.txt
dongler inspect invoice.pdf
dongler extract notes.txt --format markdown
dongler extract notes.txt --format latex
dongler extract notes.txt --format json
```

PDF extraction through the CLI uses the same Rust-native engine as the Rust,
Python, and TypeScript packages.


## License

Dongler is licensed under the MIT License. See `LICENSE` and `NOTICE`.
