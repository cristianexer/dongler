# dongler-wasm

WebAssembly bindings for [Dongler](https://github.com/cristianexer/dongler),
the fast Rust-native PDF and document extraction engine.

These bindings expose Dongler's **filesystem-free** extraction API, so documents
can be parsed entirely in the browser (or any other wasm host) from an in-memory
byte buffer — for example a `File`/`Blob` from a file input — with no server
round-trip.

## What works in wasm

The full extraction and rendering pipeline runs in `wasm32-unknown-unknown`:

- PDF, Office (`docx`/`xlsx`/`pptx`), OpenDocument, HTML, XML, email, JSON, CSV,
  Markdown, LaTeX, plain text, and archive extraction.
- Markdown, JSON, and LaTeX rendering of the document IR.

Two pieces of the native build are intentionally **not** part of the wasm build,
because the platform has no equivalent:

- **Threads** — the core's `parallel` feature (rayon) is disabled, so PDF page
  and font decoding run sequentially. Output is identical, just single-threaded.
- **OCR fallback** — the optional scanned-PDF OCR path shells out to external
  `pdftoppm`/`tesseract` binaries and is not available without a process host.

## Building

You need the wasm target and the `wasm-bindgen` CLI:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli
```

Then build the bindings for the JS targets you need (defaults to
`web bundler nodejs`):

```bash
# from the repository root
scripts/build-wasm.sh            # web + bundler + nodejs
scripts/build-wasm.sh web        # just the `web` target
make build-wasm                  # same as scripts/build-wasm.sh
```

Generated bindings are written to `crates/dongler-wasm/pkg/<target>/`.

For a polished npm package you can also use
[`wasm-pack`](https://rustwasm.github.io/wasm-pack/):

```bash
wasm-pack build crates/dongler-wasm --target web
```

## Usage

### Browser (`web` target)

```js
import init, { extractBytesMarkdown, version } from "./pkg/web/dongler.js";

await init(); // load and instantiate the wasm module

const file = document.querySelector("input[type=file]").files[0];
const bytes = new Uint8Array(await file.arrayBuffer());

// `file.name` is used only to detect the format from its extension.
const markdown = extractBytesMarkdown(bytes, file.name);
console.log(`dongler ${version()}`, markdown);
```

### Node.js (`nodejs` target)

```js
const fs = require("node:fs");
const dongler = require("./pkg/nodejs/dongler.js");

const bytes = new Uint8Array(fs.readFileSync("report.pdf"));
const json = dongler.extractBytesJson(bytes, "report.pdf");
console.log(JSON.parse(json).metadata);
```

## API

| Function | Description |
| --- | --- |
| `version()` | Package version string. |
| `detectFormat(filename)` | Detect the format name (e.g. `"pdf"`) from a file name. |
| `parseTextJson(text)` | Parse plain text into the document IR as JSON. |
| `toMarkdown(text)` / `toJson(text)` / `toLatex(text)` | Parse plain text and render. |
| `extractBytesJson(bytes, filename)` | Extract any supported format from bytes to IR JSON. |
| `extractBytesJsonWithOptions(bytes, filename, optionsJson)` | As above with an `ExtractOptions` JSON string. |
| `extractBytesMarkdown(bytes, filename)` | Extract from bytes and render Markdown. |
| `extractBytesLatex(bytes, filename)` | Extract from bytes and render LaTeX. |
| `documentToMarkdown(json)` / `documentToJson(json)` / `documentToLatex(json)` | Re-render a previously produced IR document. |

All functions throw a JavaScript `Error` (carrying the Dongler error message) on
failure, e.g. for unknown or unsupported formats.

## License

MIT
