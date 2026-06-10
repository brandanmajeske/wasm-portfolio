// Line-for-line JavaScript port of demos/raytracer/src/lib.rs, used by
// bench.html for the JS-vs-WASM comparison. Keep the two in sync.

const v3 = (x, y, z) => ({ x, y, z });
const add = (a, b) => v3(a.x + b.x, a.y + b.y, a.z + b.z);
const sub = (a, b) => v3(a.x - b.x, a.y - b.y, a.z - b.z);
const muls = (a, s) => v3(a.x * s, a.y * s, a.z * s);
const mulv = (a, b) => v3(a.x * b.x, a.y * b.y, a.z * b.z);
const neg = (a) => v3(-a.x, -a.y, -a.z);
const dot = (a, b) => a.x * b.x + a.y * b.y + a.z * b.z;
const cross = (a, b) =>
  v3(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x);
const norm = (a) => muls(a, 1 / Math.sqrt(dot(a, a)));

const reflect = (v, n) => sub(v, muls(n, 2 * dot(v, n)));
const refract = (uv, n, ratio) => {
  const cos = Math.min(dot(neg(uv), n), 1);
  const rPerp = muls(add(uv, muls(n, cos)), ratio);
  const rPar = muls(n, -Math.sqrt(Math.abs(1 - dot(rPerp, rPerp))));
  return add(rPerp, rPar);
};
const schlick = (cos, ratio) => {
  const r0 = ((1 - ratio) / (1 + ratio)) ** 2;
  return r0 + (1 - r0) * (1 - cos) ** 5;
};

const LAMBERT = 0, METAL = 1, GLASS = 2, CHECKER = 3;
const MAX_DEPTH = 8;

export class JsTracer {
  constructor(canvas) {
    this.ctx = canvas.getContext('2d');
    this.w = canvas.width;
    this.h = canvas.height;
    this.accum = new Float32Array(this.w * this.h * 3);
    this.img = this.ctx.createImageData(this.w, this.h);
    this._passes = 0;
    this.rng = ((Math.random() * 0xffffffff) >>> 0) | 1;

    // Camera — identical to the Rust version.
    const lookfrom = v3(0.0, 1.4, 3.4);
    const lookat = v3(0.0, 0.55, 0.0);
    const vup = v3(0, 1, 0);
    const vfov = 48.0;
    const aspect = this.w / this.h;
    const vh = 2 * Math.tan((vfov * Math.PI) / 180 / 2);
    const vw = aspect * vh;
    const cw = norm(sub(lookfrom, lookat));
    const cu = norm(cross(vup, cw));
    const cv = cross(cw, cu);
    this.horizontal = muls(cu, vw);
    this.vertical = muls(cv, vh);
    this.lowerLeft = sub(
      sub(sub(lookfrom, muls(this.horizontal, 0.5)), muls(this.vertical, 0.5)),
      cw
    );
    this.origin = lookfrom;

    // Scene — identical to the Rust version.
    this.scene = [
      { c: v3(0, -1000, 0), r: 1000, mat: CHECKER },
      { c: v3(0, 0.6, -0.2), r: 0.6, mat: LAMBERT, albedo: v3(0.3, 0.4, 0.85) },
      { c: v3(-1.35, 0.6, 0.1), r: 0.6, mat: GLASS, ior: 1.5 },
      { c: v3(1.35, 0.6, 0.1), r: 0.6, mat: METAL, albedo: v3(0.85, 0.85, 0.9), fuzz: 0.04 },
      { c: v3(-0.7, 0.22, 1.1), r: 0.22, mat: METAL, albedo: v3(0.9, 0.7, 0.5), fuzz: 0.15 },
      { c: v3(0.7, 0.22, 1.1), r: 0.22, mat: LAMBERT, albedo: v3(0.85, 0.3, 0.3) },
      { c: v3(-2.1, 0.22, 0.9), r: 0.22, mat: LAMBERT, albedo: v3(0.3, 0.8, 0.4) },
      { c: v3(2.1, 0.22, 0.9), r: 0.22, mat: GLASS, ior: 1.5 },
      { c: v3(-0.2, 0.22, 1.6), r: 0.22, mat: LAMBERT, albedo: v3(0.9, 0.75, 0.25) },
      { c: v3(1.4, 0.22, 1.5), r: 0.22, mat: METAL, albedo: v3(0.6, 0.7, 0.9), fuzz: 0.0 },
    ];
  }

  rand() {
    let x = this.rng;
    x ^= (x << 13) >>> 0; x >>>= 0;
    x ^= x >>> 17;
    x ^= (x << 5) >>> 0; x >>>= 0;
    this.rng = x;
    return x;
  }

  randf() {
    return (this.rand() >>> 8) / 16777216;
  }

  randUnit() {
    for (;;) {
      const p = v3(this.randf() * 2 - 1, this.randf() * 2 - 1, this.randf() * 2 - 1);
      const l2 = dot(p, p);
      if (l2 > 1e-7 && l2 < 1) return muls(p, 1 / Math.sqrt(l2));
    }
  }

  hit(ro, rd) {
    let bestT = Infinity;
    let bestI = -1;
    for (let i = 0; i < this.scene.length; i++) {
      const s = this.scene[i];
      const oc = sub(ro, s.c);
      const halfB = dot(oc, rd);
      const c = dot(oc, oc) - s.r * s.r;
      const disc = halfB * halfB - c;
      if (disc < 0) continue;
      const sq = Math.sqrt(disc);
      for (const t of [-halfB - sq, -halfB + sq]) {
        if (t > 0.001 && t < bestT) {
          bestT = t;
          bestI = i;
          break;
        }
      }
    }
    return bestI < 0 ? null : { t: bestT, i: bestI };
  }

  rayColor(ro, rd) {
    let atten = v3(1, 1, 1);
    for (let depth = 0; depth < MAX_DEPTH; depth++) {
      const h = this.hit(ro, rd);
      if (!h) {
        const s = 0.5 * (rd.y + 1);
        const sky = add(muls(v3(1, 1, 1), 1 - s), muls(v3(0.45, 0.65, 1.0), s));
        return mulv(atten, sky);
      }
      const sphere = this.scene[h.i];
      const p = add(ro, muls(rd, h.t));
      const n = muls(sub(p, sphere.c), 1 / sphere.r);
      ro = p;
      switch (sphere.mat) {
        case LAMBERT:
          atten = mulv(atten, sphere.albedo);
          rd = norm(add(n, this.randUnit()));
          break;
        case CHECKER: {
          const dark = ((Math.floor(p.x) + Math.floor(p.z)) & 1) === 0;
          atten = mulv(atten, dark ? v3(0.25, 0.3, 0.35) : v3(0.85, 0.85, 0.8));
          rd = norm(add(n, this.randUnit()));
          break;
        }
        case METAL:
          atten = mulv(atten, sphere.albedo);
          rd = norm(add(reflect(rd, n), muls(this.randUnit(), sphere.fuzz)));
          if (dot(rd, n) <= 0) return v3(0, 0, 0);
          break;
        case GLASS: {
          const front = dot(rd, n) < 0;
          const nf = front ? n : neg(n);
          const ratio = front ? 1 / sphere.ior : sphere.ior;
          const cos = Math.min(dot(neg(rd), nf), 1);
          const sin = Math.sqrt(1 - cos * cos);
          rd =
            ratio * sin > 1 || schlick(cos, ratio) > this.randf()
              ? norm(reflect(rd, nf))
              : norm(refract(rd, nf, ratio));
          break;
        }
      }
    }
    return v3(0, 0, 0);
  }

  pass() {
    const { w, h } = this;
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        const u = (x + this.randf()) / w;
        const v = (h - 1 - y + this.randf()) / h;
        const rd = norm(
          sub(
            add(this.lowerLeft, add(muls(this.horizontal, u), muls(this.vertical, v))),
            this.origin
          )
        );
        const c = this.rayColor(this.origin, rd);
        const i = (y * w + x) * 3;
        this.accum[i] += c.x;
        this.accum[i + 1] += c.y;
        this.accum[i + 2] += c.z;
      }
    }
    this._passes++;
  }

  passes() {
    return this._passes;
  }

  present() {
    const inv = 1 / Math.max(this._passes, 1);
    const data = this.img.data;
    for (let i = 0; i < this.w * this.h; i++) {
      for (let ch = 0; ch < 3; ch++) {
        const c = Math.sqrt(Math.min(Math.max(this.accum[i * 3 + ch] * inv, 0), 1));
        data[i * 4 + ch] = (c * 255) | 0;
      }
      data[i * 4 + 3] = 255;
    }
    this.ctx.putImageData(this.img, 0, 0);
  }
}
