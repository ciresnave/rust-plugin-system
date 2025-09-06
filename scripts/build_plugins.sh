#!/usr/bin/env bash
# Build all plugin crates and copy their artifacts into plugin-host/plugins_out
set -euo pipefail
ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
PLUGINS_DIR="$ROOT_DIR/plugins"
OUT_DIR="$ROOT_DIR/plugin-host/plugins_out"
mkdir -p "$OUT_DIR"

for p in "$PLUGINS_DIR"/*/; do
  if [ -f "$p/Cargo.toml" ]; then
    echo "Building plugin: $(basename "$p")"
    (cd "$p" && cargo build)

    NAME=$(basename "$p")
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