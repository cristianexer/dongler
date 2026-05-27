#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -eq 0 ]; then
  echo "usage: scripts/eval-smoke.sh file.pdf [file2.pdf ...]" >&2
  exit 2
fi

out_dir="${DONGLER_EVAL_OUT_DIR:-eval/out/smoke}"
mkdir -p "$out_dir"

for pdf in "$@"; do
  name="$(basename "$pdf")"
  json_out="$out_dir/${name%.pdf}.dongler.json"
  md_out="$out_dir/${name%.pdf}.md"

  start_ns="$(date +%s%N)"
  cargo run -q -p dongler -- extract "$pdf" --format json > "$json_out"
  cargo run -q -p dongler -- extract "$pdf" --format markdown > "$md_out"
  end_ns="$(date +%s%N)"

  python3 - "$json_out" "$pdf" "$start_ns" "$end_ns" <<'PY'
import hashlib
import json
import os
import sys

json_path, pdf_path, start_ns, end_ns = sys.argv[1:]
with open(json_path, "rb") as handle:
    payload = handle.read()
doc = json.loads(payload)
pages = len(doc.get("pages", []))
blocks = sum(len(page.get("blocks", [])) for page in doc.get("pages", []))
anchored = 0
for page in doc.get("pages", []):
    for block in page.get("blocks", []):
        if block.get("source_anchors"):
            anchored += 1

elapsed = max((int(end_ns) - int(start_ns)) / 1_000_000_000, 0.000001)
summary = {
    "pdf": pdf_path,
    "json": json_path,
    "bytes": os.path.getsize(pdf_path),
    "pages": pages,
    "blocks": blocks,
    "anchored_block_rate": anchored / blocks if blocks else 0,
    "pages_per_second": pages / elapsed,
    "sha256": hashlib.sha256(payload).hexdigest(),
    "warnings": len(doc.get("warnings", [])) + sum(len(page.get("warnings", [])) for page in doc.get("pages", [])),
}
print(json.dumps(summary, sort_keys=True))
PY
done
