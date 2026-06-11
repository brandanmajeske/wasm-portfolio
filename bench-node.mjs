// Headless JS-vs-WASM tracer benchmark with a real clock — verification
// harness for bench.html, run as: node bench-node.mjs [passes]
// Stubs just enough DOM for the wasm-bindgen glue and JsTracer.
import { readFileSync, readdirSync } from 'node:fs';

// build.sh content-hashes asset filenames; resolve them by pattern.
const find = (dir, re) => {
  const f = readdirSync(dir).find(f => re.test(f));
  if (!f) throw new Error(`no file matching ${re} in ${dir}`);
  return `${dir}/${f}`;
};

const W = 320, H = 240;

class CanvasRenderingContext2D {
  createImageData(w, h) { return { data: new Uint8ClampedArray(w * h * 4) }; }
  putImageData() {}
}
class HTMLCanvasElement {
  constructor() { this.width = W; this.height = H; this._ctx = new CanvasRenderingContext2D(); }
  getContext() { return this._ctx; }
}
class ImageData {
  constructor(data, w, h) { this.data = data; this.width = w; this.height = h; }
}

const canvases = { 'wasm-canvas': new HTMLCanvasElement(), 'js-canvas': new HTMLCanvasElement() };
class Document {
  getElementById(id) { return canvases[id] ?? null; }
}
class Window {
  constructor() { this.document = new Document(); }
}
globalThis.HTMLCanvasElement = HTMLCanvasElement;
globalThis.CanvasRenderingContext2D = CanvasRenderingContext2D;
globalThis.ImageData = ImageData;
globalThis.Document = Document;
globalThis.Window = Window;
globalThis.window = new Window();
globalThis.document = globalThis.window.document;

const { default: init, Raytracer } = await import(find('./dist/demos/raytracer', /^raytracer\.[0-9a-f]{8}\.js$/));
const { JsTracer } = await import(find('./dist', /^bench-tracer\.[0-9a-f]{8}\.js$/));
await init({ module_or_path: readFileSync(find('./dist/demos/raytracer', /^raytracer_bg\.[0-9a-f]{8}\.wasm$/)) });

const passes = Number(process.argv[2] || 20);
const wasmRt = new Raytracer('wasm-canvas');
const jsRt = new JsTracer(canvases['js-canvas']);

const time = (fn) => {
  const t0 = process.hrtime.bigint();
  fn();
  return Number(process.hrtime.bigint() - t0) / 1e6;
};

// Warm up both engines (JIT for JS, instantiation costs for WASM).
wasmRt.pass(); jsRt.pass();

const wasmMs = time(() => { for (let i = 0; i < passes; i++) wasmRt.pass(); });
const jsMs = time(() => { for (let i = 0; i < passes; i++) jsRt.pass(); });

const msps = (ms) => ((W * H * passes) / (ms / 1000) / 1e6).toFixed(1);
console.log(`passes: ${passes} @ ${W}x${H}`);
console.log(`wasm: ${(wasmMs / passes).toFixed(1)} ms/pass  ${msps(wasmMs)} Msamples/s`);
console.log(`js:   ${(jsMs / passes).toFixed(1)} ms/pass  ${msps(jsMs)} Msamples/s`);
console.log(`speedup: ${(jsMs / wasmMs).toFixed(2)}x`);
