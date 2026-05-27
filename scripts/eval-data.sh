#!/usr/bin/env bash
set -euo pipefail

dataset="${1:-all}"
root="${DONGLER_EVAL_DATA_DIR:-eval/data}"
mkdir -p "$root"

download_doclaynet() {
  if ! command -v hf >/dev/null 2>&1; then
    echo "hf CLI is required for DocLayNet: pip install -U huggingface_hub" >&2
    return 1
  fi
  hf download ds4sd/DocLayNet --repo-type dataset --local-dir "$root/doclaynet"
}

download_pubtables() {
  if [ -z "${PUBTABLES1M_AZURE_URL:-}" ]; then
    echo "Set PUBTABLES1M_AZURE_URL to the generated Microsoft Research Open Data URL for PubTables-1M." >&2
    return 1
  fi
  if command -v azcopy >/dev/null 2>&1; then
    azcopy copy "$PUBTABLES1M_AZURE_URL" "$root/pubtables-1m" --recursive=true
  else
    echo "azcopy is required for PubTables-1M recursive Azure downloads." >&2
    return 1
  fi
}

download_olmocr() {
  if [ -d "$root/olmocr/.git" ]; then
    git -C "$root/olmocr" pull --ff-only
  else
    git clone --depth 1 https://github.com/allenai/olmocr "$root/olmocr"
  fi
}

download_ckorzen() {
  if [ -d "$root/ckorzen-pdf-text-extraction/.git" ]; then
    git -C "$root/ckorzen-pdf-text-extraction" pull --ff-only
  else
    git clone --depth 1 https://github.com/ckorzen/pdf-text-extraction-benchmark "$root/ckorzen-pdf-text-extraction"
  fi
}

case "$dataset" in
  all)
    download_doclaynet
    download_olmocr
    download_ckorzen
    if [ -n "${PUBTABLES1M_AZURE_URL:-}" ]; then
      download_pubtables
    else
      echo "Skipping PubTables-1M because PUBTABLES1M_AZURE_URL is not set." >&2
    fi
    ;;
  doclaynet) download_doclaynet ;;
  pubtables-1m) download_pubtables ;;
  olmocr-bench) download_olmocr ;;
  ckorzen) download_ckorzen ;;
  *)
    echo "unknown dataset: $dataset" >&2
    exit 2
    ;;
esac
