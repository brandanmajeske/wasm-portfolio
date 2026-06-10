#!/usr/bin/env bash
# Builds all WASM demos and assembles the deployable site into dist/.
set -euo pipefail
cd "$(dirname "$0")"

DIST=dist
rm -rf "$DIST"
mkdir -p "$DIST"

cp -r site/. "$DIST/"

for demo in demos/*/; do
  name=$(basename "$demo")
  echo "==> building $name"
  wasm-pack build "$demo" --release --target web --out-dir pkg --no-typescript

  out="$DIST/demos/$name"
  mkdir -p "$out"
  cp "$demo/pkg/${name}.js" "$demo/pkg/${name}_bg.wasm" "$out/"

  if command -v wasm-opt >/dev/null; then
    wasm-opt -Oz --enable-bulk-memory --enable-sign-ext --enable-nontrapping-float-to-int \
      "$out/${name}_bg.wasm" -o "$out/${name}_bg.wasm"
  fi

  printf '    %s: %s\n' "$name" "$(du -h "$out/${name}_bg.wasm" | cut -f1)"
done

echo "==> dist/ ready"
