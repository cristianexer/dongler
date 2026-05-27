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
extraction_status: supported
```

## Extract

Text and PDF files work today:

```bash
dongler extract notes.txt --format markdown
dongler extract notes.txt --format latex
dongler extract invoice.pdf --format json
```
