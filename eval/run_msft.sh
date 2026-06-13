#!/usr/bin/env bash
# Build + install the local dongler, fetch the Microsoft 10-K, run extraction,
# and produce eval/out/msft.md for visual inspection.
#
#   bash eval/run_msft.sh
#
# The PDF host (Akamai) blocks non-browser clients, so if the download is
# refused the script tells you to drop the file at eval/data/repro/msft.pdf
# (your browser download works) and re-run.
set -euo pipefail
cd "$(dirname "$0")/.."

PDF_URL="https://microsoft.gcs-web.com/static-files/e2931fdb-9823-4130-b2a8-f6b8db0b15a9"
PDF_DST="eval/data/repro/msft.pdf"
MD_OUT="eval/out/msft.md"
HTML_OUT="eval/out/msft.html"
mkdir -p eval/data/repro eval/out

echo "==> Building + installing latest local dongler (CLI + Python package)"
cargo build -p dongler --release
if command -v uv >/dev/null 2>&1; then
  # Make `import dongler` in your notebook resolve to this checkout.
  uv run maturin develop --release >/dev/null 2>&1 \
    || uv run maturin develop >/dev/null 2>&1 \
    || echo "   (maturin develop skipped — CLI still built)"
fi

echo "==> Fetching MSFT 10-K -> $PDF_DST"
if [ ! -s "$PDF_DST" ] || ! head -c4 "$PDF_DST" 2>/dev/null | grep -q '%PDF'; then
  curl -fsSL \
    -A 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124 Safari/537.36' \
    -H 'Referer: https://microsoft.gcs-web.com/' \
    -o "$PDF_DST" "$PDF_URL" || true
fi
if ! head -c4 "$PDF_DST" 2>/dev/null | grep -q '%PDF'; then
  rm -f "$PDF_DST"
  cat >&2 <<EOF
!! Could not download the PDF (the host blocks non-browser clients).
   Download it in your browser from:
     $PDF_URL
   save it to:
     $PDF_DST
   then re-run: bash eval/run_msft.sh
EOF
  exit 1
fi
echo "   got $(wc -c < "$PDF_DST") bytes, $(pdfinfo "$PDF_DST" 2>/dev/null | sed -n 's/^Pages: *//p') pages"

echo "==> Extracting -> $MD_OUT (dongler convert = the hybrid pipeline)"
./target/release/dongler convert "$PDF_DST" --format markdown > "$MD_OUT"
echo "   wrote $MD_OUT ($(wc -l < "$MD_OUT") lines)"

# Also emit a standalone HTML so you can eyeball tables in a browser.
if command -v uv >/dev/null 2>&1; then
  uv run --with markdown python - "$MD_OUT" "$HTML_OUT" <<'PY' 2>/dev/null || true
import sys, markdown, pathlib
md = pathlib.Path(sys.argv[1]).read_text()
body = markdown.markdown(md, extensions=["tables", "md_in_html"])
css = "body{font:14px -apple-system,Helvetica,Arial,sans-serif;max-width:1000px;margin:30px auto;color:#111}" \
      "table{border-collapse:collapse;margin:10px 0}th,td{border:1px solid #bbb;padding:4px 8px}th{background:#f3f4f6}"
pathlib.Path(sys.argv[2]).write_text(f"<!doctype html><meta charset=utf-8><style>{css}</style><body>{body}")
print(f"   wrote {sys.argv[2]}")
PY
fi

echo
echo "Visualise:"
echo "  - Markdown: open $MD_OUT   (VS Code preview, or any markdown viewer)"
echo "  - HTML:     open $HTML_OUT  (renders the tables in a browser)"
