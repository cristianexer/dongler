#!/usr/bin/env bash
#
# Build the Dongler WebAssembly package.
#
# Produces wasm-bindgen JS glue for one or more JS targets next to the compiled
# module under crates/dongler-wasm/pkg/<target>/.
#
# Usage:
#   scripts/build-wasm.sh [target ...]
#
# Targets are wasm-bindgen target names (web, bundler, nodejs, deno, no-modules).
# Defaults to "web bundler nodejs" when none are given.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE_DIR="$ROOT/crates/dongler-wasm"
WASM="$ROOT/target/wasm32-unknown-unknown/release/dongler_wasm.wasm"

TARGETS=("$@")
if [ "${#TARGETS[@]}" -eq 0 ]; then
  TARGETS=(web bundler nodejs)
fi

if ! rustup target list --installed 2>/dev/null | grep -q '^wasm32-unknown-unknown$'; then
  echo "Installing wasm32-unknown-unknown target..." >&2
  rustup target add wasm32-unknown-unknown
fi

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "error: wasm-bindgen CLI not found. Install with:" >&2
  echo "  cargo install wasm-bindgen-cli" >&2
  exit 1
fi

echo "Compiling dongler-wasm for wasm32-unknown-unknown (release)..." >&2
cargo build -p dongler-wasm --release --target wasm32-unknown-unknown

for target in "${TARGETS[@]}"; do
  out_dir="$CRATE_DIR/pkg/$target"
  echo "Generating bindings for target '$target' -> $out_dir" >&2
  rm -rf "$out_dir"
  wasm-bindgen "$WASM" \
    --out-dir "$out_dir" \
    --out-name dongler \
    --target "$target"
done

echo "Done. Bindings written under $CRATE_DIR/pkg/." >&2
