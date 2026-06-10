# WebAssembly Developer Portfolio — Plan

*Drafted 2026-06-09*

## Concept

A developer portfolio where the site itself demonstrates the skill: a fast static
shell for content, with WebAssembly-powered interactive demos embedded as islands.
The medium is the message — every demo is both a portfolio piece and proof of
shipping WASM to production.

## Architecture: hybrid shell + WASM islands

- **Static HTML/CSS shell** for content pages (about, resume, projects index,
  contact). Fast, crawlable, accessible — avoids the SEO/a11y failure mode of
  full-WASM SPAs.
- **WASM demo islands** embedded in project pages. Candidates:
  - Ray tracer / path tracer rendering to canvas
  - Game of Life or falling-sand simulation
  - Audio visualizer (Web Audio + DSP in WASM)
  - Image filter playground
  - Small interpreter / code playground
- **"How this site works" page** explaining the build pipeline, bundle sizes,
  and interop — interviewers love this.

### Repo structure

```
portfolio/
├── site/            # static shell (plain HTML/CSS or Astro)
├── demos/
│   ├── raytracer/   # one crate/module per demo
│   ├── game-of-life/
│   └── audio-viz/
└── infra/           # IaC for AWS (CDK or Terraform)
```

### Build pipeline

1. Per demo: `wasm-pack build --release` (or `trunk build`)
2. Shrink: `wasm-opt -Oz`
3. Copy `.wasm` + JS glue into the static site's output directory

## Language choice

### Primary comparison

| | Rust | Go (TinyGo) | Zig |
|---|---|---|---|
| Hello-world size | ~100 KB | ~2 MB (~20 KB TinyGo) | <10 KB |
| JS/DOM interop | Generated, typed (`wasm-bindgen`/`web-sys`) | Clunky, stringly (`syscall/js`) | Fully manual |
| SPA frameworks | Leptos, Yew, Dioxus | None real | None |
| Library ecosystem for demos | Rich | Medium (TinyGo: limited) | Thin |
| Learning curve | Steep | Gentle | Moderate, but DIY everything |
| Compile speed | Slow | Fast | Fast |
| Stability | Stable | Stable (TinyGo lags) | Pre-1.0, breaking changes |
| Hiring signal for WASM work | Strongest | Weakest | Niche-strong |

**Decision: Rust as the primary language.** Best tooling (`wasm-pack`,
`wasm-bindgen`, `trunk`), richest demo ecosystem (`nalgebra`, `image`,
`rapier`), strongest resume pairing.

Optional polyglot play (after the Rust pipeline ships): add **one** Zig module
with a "this entire demo is 8 KB" callout, and/or an AssemblyScript module for
a JS-vs-WASM benchmark page. Two+ languages targeting one runtime is itself a
WASM talking point.

### Other languages considered

- **C/C++ (Emscripten)** — most battle-tested; best for porting a recognizable
  C codebase (emulator, Doom, SQLite) into the browser.
- **C#/Blazor** — productive for .NET devs, but multi-MB bundles hurt first load.
- **AssemblyScript** — easy for TS devs, tiny output, but weak "breadth" signal.
- **Kotlin/Wasm** — modern WasmGC target; best if telling a multiplatform story.
- **Swift, Dart, OCaml, Haskell, Nim, D** — viable, situational/niche.
- **Grain, MoonBit** — WASM-native curiosities; great conversation pieces,
  pre-mainstream ecosystems.

## Pros / cons of the WASM approach

**Pros**

- Differentiation — almost no portfolios do this; built-in interview talking point
- Near-native performance for compute-heavy demos
- Demonstrates systems skills: toolchains, memory models, build pipelines
- Deploys as static files — no servers, pennies a month

**Cons / risks**

- SEO + accessibility require the hybrid shell (full-WASM SPA renders nothing
  without executing code; canvas UIs are invisible to screen readers)
- Bundle size discipline needed — first-load on mobile is the first impression
- Slower iteration than JS (compile cycles, rougher debugging)
- Gimmick risk: demos must *justify* WASM (computation, not content)

## AWS deployment

### Static hosting: S3 + CloudFront

1. **S3 bucket** — private, holds build output (no website-hosting mode)
2. **CloudFront** in front with Origin Access Control (only CloudFront reads
   the bucket)
3. **`Content-Type: application/wasm`** on `.wasm` objects — verify after sync;
   wrong MIME breaks `WebAssembly.instantiateStreaming()`
4. **Compression** — enable Brotli/gzip in CloudFront (WASM compresses 30–50%)
5. **Cache policy** — long-lived for hashed `.wasm`/`.js` assets, short for
   `index.html`
6. **Route 53 + ACM** for domain + TLS (cert must be in `us-east-1` for
   CloudFront)
7. **Threads/SharedArrayBuffer** (if any demo needs them): response-headers
   policy adding `Cross-Origin-Opener-Policy: same-origin` and
   `Cross-Origin-Embedder-Policy: require-corp`

### CI/CD: GitHub Actions

On push to `main`:

1. Install Rust toolchain + `wasm-pack` + `wasm-opt`
2. Build all demos, build site
3. `aws s3 sync` to the bucket
4. `aws cloudfront create-invalidation`

Use **OIDC federation** for AWS credentials (no long-lived keys) — also a good
interview talking point.

### Optional backend

Contact form / analytics: API Gateway + Lambda. Bonus flex: run WASM
server-side too (Rust binary on a custom runtime, or Wasmtime) for a
full-stack WASM story.

### Cost estimate

~$1–3/month: Route 53 hosted zone (~$0.50) + pennies of S3/CloudFront at
portfolio-level traffic.

## Milestones

1. **Walking skeleton** — static shell + one trivial Rust WASM module
   ("hello from Rust" on a canvas) deployed end-to-end through
   S3/CloudFront/CI. *Ship this before building any real demo.*
2. **First real demo** — pick the most visually impressive feasible one
   (Game of Life or falling-sand is the classic fast win).
3. **Content pages** — about, resume, projects index, "how this site works."
4. **Second demo** — compute-heavy (ray tracer or audio DSP) to justify WASM.
5. **Polish** — Lighthouse pass (perf/a11y/SEO), bundle-size badges per demo,
   OG tags, custom domain.
6. **Stretch** — polyglot module (Zig/AssemblyScript), server-side WASM
   backend, JS-vs-WASM benchmark page.
