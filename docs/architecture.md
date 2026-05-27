---
sidebar_position: 6
---

# Architecture

Dongler is organized around a Rust core, path-based loaders, extraction engines,
and thin ecosystem bindings. The public API returns a document object so users
can render Markdown, LaTeX, or JSON without re-running extraction.

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

The public APIs in Rust, Python, and TypeScript expose the same operations:

- load one path
- load many paths
- render from a document object
- `detect_format`

The original text helpers remain available for compatibility.

## Data Flow

```text
input path or text
  -> InputFormat detection
  -> SourceLoader
  -> ExtractionEngine
  -> Document IR
  -> Document object
  -> Markdown, LaTeX, or JSON renderers
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
- Keep path/object APIs stable so the PDF engine can land without changing user
  code.

## Current IR Contract

The IR currently models ordered pages containing text and table blocks. This is
enough for the text vertical slice and gives PDF work a clear target.

The PDF roadmap should extend the IR only when the extraction engine has a real
need for new data. Likely additions are coordinates, spans, reading-order hints,
font/style metadata, and richer table structure.
