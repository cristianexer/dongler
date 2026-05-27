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
done < <(find "$CORPUS_DIR" -type f \( -name '*.txt' -o -name '*.text' \) -print0)

if [ "$COUNT" -eq 0 ]; then
  echo "private corpus contains no .txt or .text files" >&2
  exit 1
fi

echo "private corpus smoke-tested $COUNT text documents"
