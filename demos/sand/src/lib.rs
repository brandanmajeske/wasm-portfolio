use wasm_bindgen::prelude::*;
use wasm_bindgen::{Clamped, JsCast};

pub const EMPTY: u8 = 0;
pub const SAND: u8 = 1;
pub const WATER: u8 = 2;
pub const STONE: u8 = 3;

/// Terminal velocity in cells per tick.
const MAX_FALL: u8 = 5;
/// How far water can slide sideways in one tick when it can't fall.
const DISPERSION: i32 = 5;

#[wasm_bindgen]
pub struct Sand {
    w: i32,
    h: i32,
    cells: Vec<u8>,
    shade: Vec<u8>,
    /// Current fall speed per cell; accelerates under gravity, resets on landing.
    vel: Vec<u8>,
    /// Water's preferred flow direction (0 = left, 1 = right) — gives streams
    /// momentum instead of dithering in place.
    dir: Vec<u8>,
    moved: Vec<bool>,
    /// Water depth below the surface, rebuilt each render for color grading.
    depth: Vec<u8>,
    pixels: Vec<u8>,
    /// Precomputed background gradient, one RGB per row.
    sky: Vec<(u8, u8, u8)>,
    ctx: web_sys::CanvasRenderingContext2d,
    rng: u32,
    flip: bool,
}

#[wasm_bindgen]
impl Sand {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: &str) -> Result<Sand, JsValue> {
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

        let (w, h) = (canvas.width() as i32, canvas.height() as i32);
        let n = (w * h) as usize;
        let sky = (0..h)
            .map(|y| {
                let t = y as f32 / h as f32;
                (
                    (26.0 - 10.0 * t) as u8,
                    (28.0 - 11.0 * t) as u8,
                    (42.0 - 16.0 * t) as u8,
                )
            })
            .collect();
        Ok(Sand {
            w,
            h,
            cells: vec![EMPTY; n],
            shade: vec![0; n],
            vel: vec![0; n],
            dir: vec![0; n],
            moved: vec![false; n],
            depth: vec![0; n],
            pixels: vec![0; n * 4],
            sky,
            ctx,
            rng: (js_sys::Math::random() * u32::MAX as f64) as u32 | 1,
            flip: false,
        })
    }

    pub fn paint(&mut self, x: i32, y: i32, material: u8, radius: i32) {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let (px, py) = (x + dx, y + dy);
                if px < 0 || px >= self.w || py < 0 || py >= self.h {
                    continue;
                }
                // Sprinkle granular materials loosely; stone and eraser are solid.
                if (material == SAND || material == WATER) && self.rand() % 4 != 0 {
                    continue;
                }
                let i = (py * self.w + px) as usize;
                if material == EMPTY || self.cells[i] == EMPTY {
                    self.cells[i] = material;
                    self.shade[i] = (self.rand() % 8) as u8;
                    self.vel[i] = 0;
                    self.dir[i] = (self.rand() & 1) as u8;
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.cells.fill(EMPTY);
        self.vel.fill(0);
    }

    pub fn tick(&mut self) {
        self.moved.fill(false);
        self.flip = !self.flip;
        for y in (0..self.h).rev() {
            for sx in 0..self.w {
                let x = if self.flip { self.w - 1 - sx } else { sx };
                let i = (y * self.w + x) as usize;
                if self.moved[i] {
                    continue;
                }
                match self.cells[i] {
                    SAND => self.update_sand(x, y),
                    WATER => self.update_water(x, y),
                    _ => {}
                }
            }
        }
    }

    pub fn render(&mut self) -> Result<(), JsValue> {
        // Column scan: depth of each water cell below the surface.
        for x in 0..self.w {
            let mut d: u8 = 0;
            for y in 0..self.h {
                let i = (y * self.w + x) as usize;
                d = if self.cells[i] == WATER { d.saturating_add(1) } else { 0 };
                self.depth[i] = d;
            }
        }

        for y in 0..self.h {
            let (skr, skg, skb) = self.sky[y as usize];
            for x in 0..self.w {
                let i = (y * self.w + x) as usize;
                let s = self.shade[i] as i32;
                let (r, g, b) = match self.cells[i] {
                    SAND => {
                        let (mut r, mut g, mut b) =
                            (188 + s * 6, 152 + s * 5, 92 + s * 4);
                        if self.touches_water(x, y) {
                            // Wet sand is darker and slightly redder.
                            r = r * 168 / 256;
                            g = g * 152 / 256;
                            b = b * 140 / 256;
                        }
                        (r, g, b)
                    }
                    WATER => {
                        // Lerp from shallow to deep blue by depth, with a
                        // lighter, sparkling surface line.
                        let d = self.depth[i].min(24) as i32;
                        let mut r = 64 - d * 2 + s;
                        let mut g = 134 - d * 3 + s * 2;
                        let mut b = 238 - d * 4 + s * 2;
                        if self.depth[i] == 1 {
                            r += 26;
                            g += 30;
                            b += 17;
                            if self.rand() % 24 == 0 {
                                // Occasional glint where light hits the surface.
                                r += 70;
                                g += 60;
                                b += 17;
                            }
                        }
                        (r, g, b)
                    }
                    STONE => {
                        // Speckled granite: a stable per-cell hash dims some cells.
                        let speck = (i as u32).wrapping_mul(2654435761) >> 29;
                        let k = if speck == 0 { -14 } else { 0 };
                        (82 + s * 5 + k, 86 + s * 5 + k, 100 + s * 5 + k)
                    }
                    _ => (skr as i32, skg as i32, skb as i32),
                };
                let p = i * 4;
                self.pixels[p] = r.clamp(0, 255) as u8;
                self.pixels[p + 1] = g.clamp(0, 255) as u8;
                self.pixels[p + 2] = b.clamp(0, 255) as u8;
                self.pixels[p + 3] = 255;
            }
        }
        let img = web_sys::ImageData::new_with_u8_clamped_array_and_sh(
            Clamped(&self.pixels),
            self.w as u32,
            self.h as u32,
        )?;
        self.ctx.put_image_data(&img, 0.0, 0.0)
    }
}

impl Sand {
    fn rand(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    fn idx(&self, x: i32, y: i32) -> usize {
        (y * self.w + x) as usize
    }

    fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && x < self.w && y >= 0 && y < self.h
    }

    fn cell_at(&self, x: i32, y: i32) -> u8 {
        if self.in_bounds(x, y) {
            self.cells[self.idx(x, y)]
        } else {
            STONE // walls behave like stone
        }
    }

    fn is_open(&self, x: i32, y: i32) -> bool {
        self.in_bounds(x, y) && self.cells[self.idx(x, y)] == EMPTY
    }

    /// Swap two cells, carrying all per-particle state along.
    fn swap(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) {
        let (a, b) = (self.idx(x1, y1), self.idx(x2, y2));
        self.cells.swap(a, b);
        self.shade.swap(a, b);
        self.vel.swap(a, b);
        self.dir.swap(a, b);
    }

    fn touches_water(&self, x: i32, y: i32) -> bool {
        self.cell_at(x, y - 1) == WATER
            || self.cell_at(x, y + 1) == WATER
            || self.cell_at(x - 1, y) == WATER
            || self.cell_at(x + 1, y) == WATER
    }

    fn update_sand(&mut self, x: i32, y: i32) {
        let i = self.idx(x, y);
        let v = (self.vel[i] + 1).min(MAX_FALL);
        self.vel[i] = v;

        // Accelerating free fall: cover up to `v` cells this tick.
        let mut cy = y;
        for _ in 0..v {
            if self.is_open(x, cy + 1) {
                self.swap(x, cy, x, cy + 1);
                cy += 1;
            } else {
                break;
            }
        }
        if cy > y {
            let to = self.idx(x, cy);
            self.moved[to] = true;
            return;
        }

        // Blocked: sink through water (slowly — drag), else slide diagonally.
        if self.cell_at(x, y + 1) == WATER && self.rand() % 4 != 0 {
            self.swap(x, y, x, y + 1);
            let below = self.idx(x, y + 1);
            let from = self.idx(x, y);
            self.vel[below] = 1; // water drag kills momentum
            self.moved[below] = true;
            self.moved[from] = true; // displaced water already moved
            return;
        }
        let side = if self.rand() & 1 == 0 { 1 } else { -1 };
        for s in [side, -side] {
            if self.is_open(x + s, y + 1) {
                self.swap(x, y, x + s, y + 1);
                let to = self.idx(x + s, y + 1);
                self.moved[to] = true;
                return;
            }
        }
        self.vel[i] = 0; // settled
    }

    fn update_water(&mut self, x: i32, y: i32) {
        let i = self.idx(x, y);
        let v = (self.vel[i] + 1).min(MAX_FALL);
        self.vel[i] = v;

        let mut cy = y;
        for _ in 0..v {
            if self.is_open(x, cy + 1) {
                self.swap(x, cy, x, cy + 1);
                cy += 1;
            } else {
                break;
            }
        }
        if cy > y {
            let to = self.idx(x, cy);
            self.moved[to] = true;
            return;
        }

        let side = if self.dir[i] == 1 { 1 } else { -1 };
        for s in [side, -side] {
            if self.is_open(x + s, y + 1) {
                self.swap(x, y, x + s, y + 1);
                let to = self.idx(x + s, y + 1);
                self.dir[to] = if s > 0 { 1 } else { 0 };
                self.moved[to] = true;
                return;
            }
        }

        // Can't fall: flow sideways, keeping momentum in the preferred
        // direction and sliding up to DISPERSION cells at once.
        self.vel[i] = 0;
        for s in [side, -side] {
            let mut tx = x;
            while (tx - x).abs() < DISPERSION && self.is_open(tx + s, y) {
                tx += s;
            }
            if tx != x {
                self.swap(x, y, tx, y);
                let to = self.idx(tx, y);
                let from = self.idx(x, y);
                self.dir[to] = if s > 0 { 1 } else { 0 };
                self.moved[to] = true;
                self.moved[from] = true;
                return;
            }
        }
        // Boxed in: flip preference so pooled water keeps probing both ways.
        self.dir[i] ^= 1;
    }
}
