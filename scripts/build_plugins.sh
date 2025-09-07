#!/usr/bin/env bash
# Build all plugin crates and copy their artifacts into plugin-host/plugins_out
# Optional flag: --skip-build  (only copy existing target artifacts; do not run `cargo build`)
set -euo pipefail
ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
PLUGINS_DIR="$ROOT_DIR/plugins"
OUT_DIR="$ROOT_DIR/plugin-host/plugins_out"
# Clean plugins_out to avoid stale artifacts from previous runs
if [ -d "$OUT_DIR" ]; then
  echo "Cleaning existing plugins_out: $OUT_DIR"
  rm -f "$OUT_DIR"/* || true
else
  mkdir -p "$OUT_DIR"
fi

SKIP_BUILD=false
if [[ "${1:-}" == "--skip-build" || "${1:-}" == "--no-build" ]]; then
  SKIP_BUILD=true
fi

for p in "$PLUGINS_DIR"/*/; do
  if [ -f "$p/Cargo.toml" ]; then
    NAME=$(basename "$p")
    echo "Processing plugin: $NAME"
    if [ "$SKIP_BUILD" = false ]; then
      echo "Building plugin: $NAME (target-dir: $p/target)"
      # Use --manifest-path and --target-dir so artifacts go into the plugin's target directory
      cargo build --manifest-path "$p/Cargo.toml" --target-dir "$p/target"
    else
      echo "Skipping build for plugin: $NAME (copy-only mode)"
    fi

    # On macOS, cargo produces .dylib; some tests expect .so — create a .so shim if needed
    uname_s=$(uname -s || true)
    if [ "$uname_s" = "Darwin" ]; then
      # canonical names
      dylib_candidate="$p/target/debug/lib${NAME}.dylib"
      dylib_candidate2="$p/target/debug/lib${NAME//-/_}.dylib"
      so_candidate="$p/target/debug/lib${NAME}.so"
      so_candidate2="$p/target/debug/lib${NAME//-/_}.so"
      if [ -f "$dylib_candidate" ] && [ ! -f "$so_candidate" ]; then
        echo "Creating .so shim for $NAME from .dylib"
        cp "$dylib_candidate" "$so_candidate"
      elif [ -f "$dylib_candidate2" ] && [ ! -f "$so_candidate2" ]; then
        echo "Creating .so shim for ${NAME//-/_} from .dylib"
        cp "$dylib_candidate2" "$so_candidate2"
      fi
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