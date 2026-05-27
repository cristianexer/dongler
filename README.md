<p align="center">
  <img src="assets/logo.png" alt="Dongler logo" width="132">
</p>

# Dongler

Dongler is a Rust-native document extraction engine with Python and TypeScript
bindings. It is built for the workflow developers actually need: load a
document path, extract structure once, then render clean Markdown or LaTeX from
the same document object.

Created by Daniel Fat.

## Status

Dongler ships `.txt` extraction and a native Rust PDF extraction path with page
geometry, text source anchors, basic table reconstruction, and image object
positions.

| Format | Detection | Extraction |
| --- | --- | --- |
| `.txt`, `.text` | yes | supported |
| `.pdf` | yes | supported |
| Word, Excel, HTML, images, email | yes | planned |

Current outputs:

- Markdown
- LaTeX
- JSON
- Dongler's typed document IR

## Install

```bash
cargo install dongler
pip install dongler
npm install @cristianexer/dongler
```

For Rust library usage, depend on `dongler-core`. The public `dongler` crate is
the CLI package.

## API

Document extraction is a two-step process: load a document path, then render the
extracted structure in the format you need. The same document object can be
rendered to Markdown, LaTeX, or JSON without re-extracting the document.

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

## API Surface

The high-level object API:

- Rust: `load_path(path)`, `load_many(paths)`, `doc.to_markdown()`,
  `doc.to_latex()`, `doc.to_json()`
- Python: `dongler.load(path)`, `dongler.load_many(paths)`,
  `doc.to_markdown()`, `doc.to_latex()`, `doc.to_json()`
- TypeScript: `load(path)`, `loadMany(paths)`, `doc.toMarkdown()`,
  `doc.toLatex()`, `doc.toJson()`

Compatibility functions remain available:

- `parse_text`
- `to_markdown`
- `to_latex`
- `to_json`
- `detect_format`

## Documentation

The Docusaurus documentation site lives in `website/` and builds from `docs/`.

```bash
cd website
npm install
npm run start
npm run build
```

## Development

```bash
make test
make build
```

Focused commands:

```bash
make test-rust
make test-python
make test-js
make build-docs
```

## License

Dongler is licensed under the MIT License. See `LICENSE` and `NOTICE`.
