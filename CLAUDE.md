# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & run

```bash
# Build all demos and assemble dist/
./build.sh

# Serve locally
python -m http.server -d dist 8080
# open http://localhost:8080
```

Build a single demo (outputs to `demos/<name>/pkg/`):
```bash
wasm-pack build demos/<name> --release --target web --out-dir pkg --no-typescript
```

The full build also runs `wasm-opt -Oz` on each `.wasm` file if `binaryen` is installed. `wasm-opt` is optional but recommended — it shrinks binaries noticeably.

Headless verification (no browser needed):
```bash
node bench-node.mjs [passes]   # JS-vs-WASM tracer benchmark with real clock
```

Each demo page also accepts query params for synchronous headless testing:
- `sand.html?ticks=N` — runs N simulation ticks then renders once
- `bench.html?passes=N` — runs both tracers for N passes then shows results

## Architecture

The site is a **static HTML/CSS shell with WASM islands**. There is no JS framework or bundler — every page is a plain HTML file in `site/` that loads a WASM module via `<script type="module">` and `import()`.

```
site/            Static shell (HTML, CSS, bench-tracer.js)
demos/           One Rust crate per WASM demo
  hello/         Canvas "hello" — walking skeleton
  sand/          Falling-sand cellular automaton
  raytracer/     Monte Carlo path tracer
dist/            Build output (generated, not committed)
infra/           AWS resource reference (manual setup notes)
.github/         CI/CD (build → S3 sync → CloudFront invalidation)
```

### Per-demo build pattern

Each demo under `demos/<name>/` is an independent Rust crate (`crate-type = ["cdylib"]`). `build.sh` builds each with `wasm-pack` and copies two files into `dist/demos/<name>/`:
- `<name>.js` — wasm-bindgen JS glue (ES module)
- `<name>_bg.wasm` — the binary

HTML pages import the JS glue directly: `import('./demos/<name>/<name>.js')`. There is no bundling step.

### wasm-opt flag: disabled in Cargo.toml

All Cargo.toml files set `wasm-opt = false` under `[package.metadata.wasm-pack.profile.release]`. This is intentional: `build.sh` invokes `wasm-opt` manually with `--enable-bulk-memory --enable-sign-ext --enable-nontrapping-float-to-int`. The flags are required because recent rustc emits bulk-memory instructions that wasm-pack's built-in wasm-opt pass (an older binaryen) rejects.

### WASM→JS interop

Each demo exposes one `#[wasm_bindgen]` struct with methods:

| Demo | Struct | Key API |
|---|---|---|
| hello | (bare `#[wasm_bindgen]` fns) | draws once on load |
| sand | `Sand` | `new(canvas_id)`, `paint(x,y,material,radius)`, `tick()`, `render()`, `clear()` |
| raytracer | `Raytracer` | `new(canvas_id)`, `pass()`, `passes()`, `present()`, `reset()` |

All demos receive a canvas ID string and grab the `CanvasRenderingContext2d` inside Rust via `web-sys`. Rendering is done by blitting an `ImageData` buffer.

### JS vs WASM benchmark

`site/bench-tracer.js` is a **line-for-line JavaScript port** of `demos/raytracer/src/lib.rs`. The two must stay in sync — same scene, same camera, same algorithm. The bench page races them side by side to demonstrate the WASM speedup. `bench-node.mjs` runs the same race headlessly in Node.

### Deploy

Pushing to `main` triggers `.github/workflows/deploy.yml`:
1. Builds with `./build.sh`
2. Syncs `dist/` to S3 (`labs.brandanmajeske.com` bucket, us-west-2)
3. Invalidates CloudFront (`E36BRP2R77ZEOB`)

AWS credentials use OIDC federation (no stored keys). The deploy role is `labs-wasm-portfolio-deploy`, trusted only for pushes to `main`.

`.wasm` files must be uploaded with `Content-Type: application/wasm`. The deploy script does this explicitly — wrong MIME breaks `WebAssembly.instantiateStreaming()`.
