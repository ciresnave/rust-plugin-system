#!/usr/bin/env bash
# Build all plugin crates and copy their artifacts into plugin-host/plugins_out
# Optional flag: --skip-build  (only copy existing target artifacts; do not run `cargo build`)
set -euo pipefail
ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
PLUGINS_DIR="$ROOT_DIR/plugins"
OUT_DIR="$ROOT_DIR/plugin-host/plugins_out"
mkdir -p "$OUT_DIR"

SKIP_BUILD=false
if [[ "${1:-}" == "--skip-build" || "${1:-}" == "--no-build" ]]; then
  SKIP_BUILD=true
fi

for p in "$PLUGINS_DIR"/*/; do
  if [ -f "$p/Cargo.toml" ]; then
    NAME=$(basename "$p")
    echo "Processing plugin: $NAME"
    if [ "$SKIP_BUILD" = false ]; then
      echo "Building plugin: $NAME"
      (cd "$p" && cargo build)
    else
      echo "Skipping build for plugin: $NAME (copy-only mode)"
    fi

    candidates=("lib${NAME}.so" "lib${NAME//-/_}.so" "lib${NAME}.dylib" "lib${NAME//-/_}.dylib" "${NAME}.dll" "${NAME//-/_}.dll")
    found=false
    for cand in "${candidates[@]}"; do
      src="$p/target/debug/$cand"
      if [ -f "$src" ]; then
        cp "$src" "$OUT_DIR/"
        echo "Copied $cand to plugins_out"
        found=true
        break
      fi
    done
    if [ "$found" = false ]; then
      echo "Warning: could not find built artifact for plugin $NAME"
    fi
  fi
done

echo "Plugins copied to: $OUT_DIR"