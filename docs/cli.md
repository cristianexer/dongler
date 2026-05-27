---
sidebar_position: 7
---

# CLI

The `dongler` command is the quickest way to inspect files and extract supported
formats.

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
extraction_status: planned
```

## Extract

Text files work today:

```bash
dongler extract notes.txt --format markdown
dongler extract notes.txt --format latex
```

PDF extraction will be added through the same command surface once the PDF
engine lands.
