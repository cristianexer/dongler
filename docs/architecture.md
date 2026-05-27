# Architecture

Dongler is organized around a small Rust core and thin ecosystem bindings. The
core crate owns extraction, the document IR, and rendering. The Python and Node
packages call Rust through native bindings and should not reimplement document
parsing or rendering logic.

## Crate Boundaries

```text
dongler-core
  format.rs   file format detection and support status
  source.rs   source loaders for files or in-memory content
  engine.rs   extraction engines that produce Document IR
  ir.rs       serializable document model
  render.rs   Markdown, LaTeX, and JSON renderers
  error.rs    shared error type

dongler-cli
  main.rs     command parsing and user-facing CLI behavior

dongler-python
  lib.rs      PyO3 functions over dongler-core

dongler-node
  lib.rs      NAPI-RS functions over dongler-core
```

The public APIs in Rust, Python, and TypeScript expose the same basic operations:

- `parse_text`
- `to_markdown`
- `to_latex`
- `to_json`
- `detect_format`

The names differ only where ecosystem conventions require it, such as
`parseText` in TypeScript.

## Data Flow

```text
input path or text
  -> InputFormat detection
  -> SourceLoader
  -> ExtractionEngine
  -> Document IR
  -> Renderer
  -> Markdown, LaTeX, or JSON
```

This separation keeps future PDF work contained. A PDF implementation should add
a `PdfSourceLoader` and one or more PDF extraction engines, then produce the
same `Document` IR used by the existing renderers.

## Extension Rules

- Add extraction logic in Rust.
- Keep Python and TypeScript wrappers thin.
- Add new file format support through `InputFormat`, a loader, and an engine.
- Add output formats through new `Renderer` implementations.
- Keep unsupported formats detectable when useful, but return explicit planned
  errors until extraction is implemented.

## Current IR Contract

The IR currently models ordered pages containing text and table blocks. This is
enough for the text vertical slice and gives PDF work a clear target.

The PDF roadmap should extend the IR only when the extraction engine has a real
need for new data. Likely additions are coordinates, spans, reading-order hints,
font/style metadata, and richer table structure.
