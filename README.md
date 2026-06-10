# wasm-portfolio

Developer portfolio with a static shell and WebAssembly demo islands.
See [PLAN.md](PLAN.md) for the full architecture and roadmap.

## Layout

```
site/            static shell (HTML/CSS)
demos/hello/     milestone-1 Rust → WASM module (canvas hello)
infra/           AWS setup notes (S3 + CloudFront)
build.sh         builds demos + assembles dist/
```

## Prerequisites

```bash
sudo pacman -S rustup
rustup default stable
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
sudo pacman -S binaryen   # provides wasm-opt (optional, shrinks binaries)
```

## Build & run locally

```bash
./build.sh
python -m http.server -d dist 8080
# open http://localhost:8080
```

## Deploy

Pushed to `main` → GitHub Actions builds and syncs `dist/` to S3, then
invalidates CloudFront. See `.github/workflows/deploy.yml` (fill in the
repository variables listed there) and `infra/README.md` for the one-time
AWS setup.
