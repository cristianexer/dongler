#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -eq 0 ] || [ "${1:-}" = "all" ]; then
  shift || true
  exec python3 scripts/download-benchmark-data.py "$@"
fi

exec python3 scripts/download-benchmark-data.py "$@"
