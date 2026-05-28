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

Supported text, PDF, Office, web/email, annotation, and image formats work
through the same command:

```bash
dongler extract notes.txt --format markdown
dongler extract notes.txt --format latex
dongler extract invoice.pdf --format json
dongler extract deck.pptx --format markdown
dongler extract notes.odt --format markdown
dongler extract annotations.json --format markdown
dongler extract boxes.csv --format json
```
