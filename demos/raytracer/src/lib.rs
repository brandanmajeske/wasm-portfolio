use std::ops::{Add, Mul, Neg, Sub};
use wasm_bindgen::prelude::*;
use wasm_bindgen::{Clamped, JsCast};

#[derive(Clone, Copy)]
struct V3 {
    x: f32,
    y: f32,
    z: f32,
}

fn v3(x: f32, y: f32, z: f32) -> V3 {
    V3 { x, y, z }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f32> for V3 {
    type Output = V3;
    fn mul(self, s: f32) -> V3 {
        v3(self.x * s, self.y * s, self.z * s)
    }
}
impl Mul for V3 {
    type Output = V3;
    fn mul(self, o: V3) -> V3 {
        v3(self.x * o.x, self.y * o.y, self.z * o.z)
    }
}
impl Neg for V3 {
    type Output = V3;
    fn neg(self) -> V3 {
        v3(-self.x, -self.y, -self.z)
    }
}

impl V3 {
    fn dot(self, o: V3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    fn cross(self, o: V3) -> V3 {
        v3(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    fn len2(self) -> f32 {
        self.dot(self)
    }
    fn norm(self) -> V3 {
        self * (1.0 / self.len2().sqrt())
    }
}

fn reflect(v: V3, n: V3) -> V3 {
    v - n * (2.0 * v.dot(n))
}

fn refract(uv: V3, n: V3, ratio: f32) -> V3 {
    let cos_theta = (-uv).dot(n).min(1.0);
    let r_perp = (uv + n * cos_theta) * ratio;
    let r_par = n * -(1.0 - r_perp.len2()).abs().sqrt();
    r_perp + r_par
}

fn schlick(cos: f32, ratio: f32) -> f32 {
    let r0 = ((1.0 - ratio) / (1.0 + ratio)).powi(2);
    r0 + (1.0 - r0) * (1.0 - cos).powi(5)
}

#[derive(Clone, Copy)]
enum Mat {
    Lambert(V3),
    Metal(V3, f32),
    Glass(f32),
    Checker,
}

struct Sphere {
    c: V3,
    r: f32,
    mat: Mat,
}

const MAX_DEPTH: u32 = 8;

#[wasm_bindgen]
pub struct Raytracer {
    w: usize,
    h: usize,
    accum: Vec<f32>,
    pixels: Vec<u8>,
    passes: u32,
    scene: Vec<Sphere>,
    ctx: web_sys::CanvasRenderingContext2d,
    rng: u32,
    // camera basis
    origin: V3,
    lower_left: V3,
    horizontal: V3,
    vertical: V3,
}

#[wasm_bindgen]
impl Raytracer {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: &str) -> Result<Raytracer, JsValue> {
        let document = web_sys::window()
            .ok_or("no window")?
            .document()
            .ok_or("no document")?;
        let canvas = document
            .get_element_by_id(canvas_id)
            .ok_or("canvas not found")?
            .dyn_into::<web_sys::HtmlCanvasElement>()?;
        let ctx = canvas
            .get_context("2d")?
            .ok_or("no 2d context")?
            .dyn_into::<web_sys::CanvasRenderingContext2d>()?;
        let (w, h) = (canvas.width() as usize, canvas.height() as usize);

        // Camera
        let lookfrom = v3(0.0, 1.4, 3.4);
        let lookat = v3(0.0, 0.55, 0.0);
        let vup = v3(0.0, 1.0, 0.0);
        let vfov: f32 = 48.0;
        let aspect = w as f32 / h as f32;
        let vh = 2.0 * (vfov.to_radians() / 2.0).tan();
        let vw = aspect * vh;
        let cw = (lookfrom - lookat).norm();
        let cu = vup.cross(cw).norm();
        let cv = cw.cross(cu);
        let horizontal = cu * vw;
        let vertical = cv * vh;
        let lower_left = lookfrom - horizontal * 0.5 - vertical * 0.5 - cw;

        // Scene: checkered ground, three hero spheres, a ring of small ones.
        let mut scene = vec![
            Sphere { c: v3(0.0, -1000.0, 0.0), r: 1000.0, mat: Mat::Checker },
            Sphere { c: v3(0.0, 0.6, -0.2), r: 0.6, mat: Mat::Lambert(v3(0.30, 0.40, 0.85)) },
            Sphere { c: v3(-1.35, 0.6, 0.1), r: 0.6, mat: Mat::Glass(1.5) },
            Sphere { c: v3(1.35, 0.6, 0.1), r: 0.6, mat: Mat::Metal(v3(0.85, 0.85, 0.9), 0.04) },
        ];
        let small = [
            (-0.7_f32, 1.1_f32, Mat::Metal(v3(0.9, 0.7, 0.5), 0.15)),
            (0.7, 1.1, Mat::Lambert(v3(0.85, 0.3, 0.3))),
            (-2.1, 0.9, Mat::Lambert(v3(0.3, 0.8, 0.4))),
            (2.1, 0.9, Mat::Glass(1.5)),
            (-0.2, 1.6, Mat::Lambert(v3(0.9, 0.75, 0.25))),
            (1.4, 1.5, Mat::Metal(v3(0.6, 0.7, 0.9), 0.0)),
        ];
        for (x, z, mat) in small {
            scene.push(Sphere { c: v3(x, 0.22, z), r: 0.22, mat });
        }

        Ok(Raytracer {
            w,
            h,
            accum: vec![0.0; w * h * 3],
            pixels: vec![0; w * h * 4],
            passes: 0,
            scene,
            ctx,
            rng: (js_sys::Math::random() * u32::MAX as f64) as u32 | 1,
            origin: lookfrom,
            lower_left,
            horizontal,
            vertical,
        })
    }

    /// Trace one sample per pixel and add it to the accumulation buffer.
    pub fn pass(&mut self) {
        for y in 0..self.h {
            for x in 0..self.w {
                let u = (x as f32 + self.randf()) / self.w as f32;
                let v = ((self.h - 1 - y) as f32 + self.randf()) / self.h as f32;
                let rd = (self.lower_left + self.horizontal * u + self.vertical * v
                    - self.origin)
                    .norm();
                let c = self.ray_color(self.origin, rd);
                let i = (y * self.w + x) * 3;
                self.accum[i] += c.x;
                self.accum[i + 1] += c.y;
                self.accum[i + 2] += c.z;
            }
        }
        self.passes += 1;
    }

    pub fn passes(&self) -> u32 {
        self.passes
    }

    /// Blit the averaged, gamma-corrected accumulation buffer to the canvas.
    pub fn present(&mut self) -> Result<(), JsValue> {
        let inv = 1.0 / self.passes.max(1) as f32;
        for i in 0..self.w * self.h {
            for ch in 0..3 {
                let c = (self.accum[i * 3 + ch] * inv).clamp(0.0, 1.0).sqrt();
                self.pixels[i * 4 + ch] = (c * 255.0) as u8;
            }
            self.pixels[i * 4 + 3] = 255;
        }
        let img = web_sys::ImageData::new_with_u8_clamped_array_and_sh(
            Clamped(&self.pixels),
            self.w as u32,
            self.h as u32,
        )?;
        self.ctx.put_image_data(&img, 0.0, 0.0)
    }

    pub fn reset(&mut self) {
        self.accum.fill(0.0);
        self.passes = 0;
    }
}

impl Raytracer {
    fn rand(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    fn randf(&mut self) -> f32 {
        (self.rand() >> 8) as f32 / 16_777_216.0
    }

    fn rand_unit(&mut self) -> V3 {
        loop {
            let p = v3(self.randf(), self.randf(), self.randf()) * 2.0 - v3(1.0, 1.0, 1.0);
            let l2 = p.len2();
            if l2 > 1e-7 && l2 < 1.0 {
                return p * (1.0 / l2.sqrt());
            }
        }
    }

    fn hit(&self, ro: V3, rd: V3) -> Option<(f32, usize)> {
        let mut best: Option<(f32, usize)> = None;
        let mut closest = f32::MAX;
        for (i, s) in self.scene.iter().enumerate() {
            let oc = ro - s.c;
            let half_b = oc.dot(rd);
            let c = oc.len2() - s.r * s.r;
            let disc = half_b * half_b - c;
            if disc < 0.0 {
                continue;
            }
            let sq = disc.sqrt();
            for t in [-half_b - sq, -half_b + sq] {
                if t > 0.001 && t < closest {
                    closest = t;
                    best = Some((t, i));
                    break;
                }
            }
        }
        best
    }

    fn ray_color(&mut self, mut ro: V3, mut rd: V3) -> V3 {
        let mut atten = v3(1.0, 1.0, 1.0);
        for _ in 0..MAX_DEPTH {
            let Some((t, idx)) = self.hit(ro, rd) else {
                // Sky gradient
                let s = 0.5 * (rd.y + 1.0);
                let sky = v3(1.0, 1.0, 1.0) * (1.0 - s) + v3(0.45, 0.65, 1.0) * s;
                return atten * sky;
            };
            let sphere = &self.scene[idx];
            let p = ro + rd * t;
            let n = (p - sphere.c) * (1.0 / sphere.r);
            let mat = sphere.mat;
            ro = p;
            match mat {
                Mat::Lambert(albedo) => {
                    atten = atten * albedo;
                    rd = (n + self.rand_unit()).norm();
                }
                Mat::Checker => {
                    let dark = ((p.x.floor() + p.z.floor()) as i32) & 1 == 0;
                    let albedo = if dark { v3(0.25, 0.3, 0.35) } else { v3(0.85, 0.85, 0.8) };
                    atten = atten * albedo;
                    rd = (n + self.rand_unit()).norm();
                }
                Mat::Metal(albedo, fuzz) => {
                    atten = atten * albedo;
                    rd = (reflect(rd, n) + self.rand_unit() * fuzz).norm();
                    if rd.dot(n) <= 0.0 {
                        return v3(0.0, 0.0, 0.0);
                    }
                }
                Mat::Glass(ior) => {
                    let front = rd.dot(n) < 0.0;
                    let nf = if front { n } else { -n };
                    let ratio = if front { 1.0 / ior } else { ior };
                    let cos = (-rd).dot(nf).min(1.0);
                    let sin = (1.0 - cos * cos).sqrt();
                    rd = if ratio * sin > 1.0 || schlick(cos, ratio) > self.randf() {
                        reflect(rd, nf).norm()
                    } else {
                        refract(rd, nf, ratio).norm()
                    };
                }
            }
        }
        v3(0.0, 0.0, 0.0)
    }
}
