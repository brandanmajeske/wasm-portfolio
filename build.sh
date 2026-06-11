#!/usr/bin/env bash
# Builds all WASM demos and assembles the deployable site into dist/.
# Mutable assets (CSS, JS, WASM) get a content hash in their filename and
# the HTML is rewritten to match, so browsers can cache them long-term and
# still pick up new versions as soon as the HTML revalidates.
set -euo pipefail
cd "$(dirname "$0")"

DIST=dist
rm -rf "$DIST"
mkdir -p "$DIST"

cp -r site/. "$DIST/"

hash8() { sha256sum "$1" | cut -c1-8; }

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

  # Hash the wasm first (the JS glue fetches it by name), then the glue.
  wh=$(hash8 "$out/${name}_bg.wasm")
  mv "$out/${name}_bg.wasm" "$out/${name}_bg.$wh.wasm"
  sed -i "s/${name}_bg\.wasm/${name}_bg.$wh.wasm/g" "$out/${name}.js"

  jh=$(hash8 "$out/${name}.js")
  mv "$out/${name}.js" "$out/${name}.$jh.js"
  sed -i "s|demos/$name/$name\.js|demos/$name/$name.$jh.js|g" "$DIST"/*.html

  printf '    %s: %s\n' "$name" "$(du -h "$out/${name}_bg.$wh.wasm" | cut -f1)"
done

th=$(hash8 "$DIST/bench-tracer.js")
mv "$DIST/bench-tracer.js" "$DIST/bench-tracer.$th.js"
sed -i "s|\./bench-tracer\.js|./bench-tracer.$th.js|g" "$DIST"/*.html

ch=$(hash8 "$DIST/styles.css")
mv "$DIST/styles.css" "$DIST/styles.$ch.css"
sed -i "s|styles\.css|styles.$ch.css|g" "$DIST"/*.html

echo "==> dist/ ready"
