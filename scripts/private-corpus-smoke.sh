#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <private-corpus-directory>" >&2
  exit 2
fi

CORPUS_DIR="$1"
if [ ! -d "$CORPUS_DIR" ]; then
  echo "private corpus directory does not exist: $CORPUS_DIR" >&2
  exit 2
fi

BIN="${DONGLER_BIN:-target/release/dongler}"
if [ ! -x "$BIN" ]; then
  echo "dongler binary is not executable: $BIN" >&2
  exit 2
fi

COUNT=0
while IFS= read -r -d '' file; do
  COUNT=$((COUNT + 1))
  "$BIN" inspect "$file" >/dev/null
  "$BIN" extract "$file" --format markdown >/dev/null
  "$BIN" extract "$file" --format json >/dev/null
  "$BIN" extract "$file" --format latex >/dev/null
done < <(find "$CORPUS_DIR" -type f \( \
  -name '*.txt' -o -name '*.text' -o -name '*.md' -o -name '*.markdown' -o \
  -name '*.tex' -o -name '*.latex' -o -name '*.ltx' -o \
  -name '*.pdf' -o -name '*.docx' -o -name '*.xlsx' -o -name '*.pptx' -o \
  -name '*.odt' -o -name '*.ods' -o -name '*.odp' -o \
  -name '*.tar' -o -name '*.tar.gz' -o -name '*.tgz' -o -name '*.zip' -o \
  -name '*.json' -o -name '*.jsonl' -o -name '*.ndjson' -o \
  -name '*.csv' -o -name '*.tsv' -o \
  -name '*.xml' -o -name '*.nxml' -o -name '*.tei' -o \
  -name '*.txt.gz' -o -name '*.text.gz' -o -name '*.md.gz' -o \
  -name '*.tex.gz' -o -name '*.json.gz' -o -name '*.jsonl.gz' -o \
  -name '*.ndjson.gz' -o -name '*.csv.gz' -o -name '*.tsv.gz' -o \
  -name '*.xml.gz' -o -name '*.nxml.gz' -o -name '*.tei.gz' -o \
  -name '*.gz' -o \
  -name '*.html' -o -name '*.htm' -o -name '*.eml' -o \
  -name '*.png' -o -name '*.jpg' -o -name '*.jpeg' -o \
  -name '*.gif' -o -name '*.bmp' -o -name '*.tif' -o \
  -name '*.tiff' -o -name '*.webp' \
\) -print0)

if [ "$COUNT" -eq 0 ]; then
  echo "private corpus contains no supported document files" >&2
  exit 1
fi

echo "private corpus smoke-tested $COUNT supported documents"
