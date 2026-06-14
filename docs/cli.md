---
sidebar_position: 7
---

# CLI

The `dongler` command is the quickest way to inspect files and extract Markdown,
LaTeX, or JSON from supported formats.

```bash
dongler --version
dongler inspect <path>
dongler extract <path> --format markdown
dongler extract <path> --format latex
dongler extract <path> --format json
```

## Inspect

```bash
dongler inspect invoice.pdf
```

Example output:

```text
path: invoice.pdf
format: pdf
extraction_status: supported
```

## Extract

PDFs are the primary workflow:

```bash
dongler extract invoice.pdf --format json
dongler extract invoice.pdf --format markdown
dongler extract invoice.pdf --format latex
```

Supported text, Office, web/email, annotation, and image formats work through
the same command:

```bash
dongler extract notes.txt --format markdown
dongler extract notes.txt --format latex
dongler extract deck.pptx --format markdown
dongler extract notes.odt --format markdown
dongler extract annotations.json --format markdown
dongler extract boxes.csv --format json
```

For shell pipelines, prefer JSON when you need metadata or warnings:

```bash
dongler extract report.pdf --format json
```

## Convert (hybrid pipeline)

`convert` runs the staged pipeline — page triage, reading order, and IR v2
provenance — over the same deterministic, model-free engine as `extract`:

```bash
dongler convert report.pdf --format markdown
dongler convert report.pdf --format json
```

### `--ml` table-structure recognition (experimental)

Builds compiled with the `ml` feature gain an opt-in stage that recognizes table
**structure** with a local ONNX model ([SLANet](https://huggingface.co/bdatdo0601/slanet-1m-onnx),
MIT) and snaps each cell's text from the PDF's own text layer — the model decides
the grid, the text layer decides the content, so values are never hallucinated:

```bash
# build once with the ml feature (pulls ONNX Runtime + a pdfium binary)
cargo install dongler --features ml
dongler convert report.pdf --ml --format markdown
```

The model weights download once to `~/.cache/dongler/models` (override with
`DONGLER_CACHE_DIR`; `DONGLER_OFFLINE=1` requires a pre-fetched cache). If the
stage fails for any region it falls back to the deterministic table and records a
warning — output is never blocked.

:::note Preview
`--ml` table structure is **experimental**. It is plumbing-complete and
hallucination-free by construction, but its accuracy is currently bounded by the
geometric table-region detector that feeds it; a layout-detection model for clean
table regions is the next milestone. The default `convert`/`extract` path is
unchanged and remains the recommended path today.
:::
