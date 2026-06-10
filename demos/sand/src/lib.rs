use wasm_bindgen::prelude::*;
use wasm_bindgen::{Clamped, JsCast};

pub const EMPTY: u8 = 0;
pub const SAND: u8 = 1;
pub const WATER: u8 = 2;
pub const STONE: u8 = 3;

#[wasm_bindgen]
pub struct Sand {
    w: i32,
    h: i32,
    cells: Vec<u8>,
    shade: Vec<u8>,
    moved: Vec<bool>,
    pixels: Vec<u8>,
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
        Ok(Sand {
            w,
            h,
            cells: vec![EMPTY; n],
            shade: vec![0; n],
            moved: vec![false; n],
            pixels: vec![0; n * 4],
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
                    self.shade[i] = (self.rand() % 4) as u8;
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.cells.fill(EMPTY);
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
                    SAND => {
                        let side = if self.rand() & 1 == 0 { 1 } else { -1 };
                        let _ = self.step(x, y, 0, 1, true)
                            || self.step(x, y, side, 1, true)
                            || self.step(x, y, -side, 1, true);
                    }
                    WATER => {
                        let side = if self.rand() & 1 == 0 { 1 } else { -1 };
                        let _ = self.step(x, y, 0, 1, false)
                            || self.step(x, y, side, 1, false)
                            || self.step(x, y, -side, 1, false)
                            || self.step(x, y, side, 0, false)
                            || self.step(x, y, -side, 0, false);
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn render(&mut self) -> Result<(), JsValue> {
        for i in 0..self.cells.len() {
            let s = self.shade[i] as u8;
            let (r, g, b) = match self.cells[i] {
                SAND => (204 + s * 8, 174 + s * 6, 102 + s * 4),
                WATER => (40 + s * 4, 110 + s * 4, 220 + s * 6),
                STONE => (88 + s * 6, 92 + s * 6, 108 + s * 6),
                _ => (22, 22, 30),
            };
            let p = i * 4;
            self.pixels[p] = r;
            self.pixels[p + 1] = g;
            self.pixels[p + 2] = b;
            self.pixels[p + 3] = 255;
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

    /// Try to move the cell at (x, y) by (dx, dy). Returns true if it moved.
    /// `sinks` lets the mover displace water (sand falls through it).
    fn step(&mut self, x: i32, y: i32, dx: i32, dy: i32, sinks: bool) -> bool {
        let (nx, ny) = (x + dx, y + dy);
        if nx < 0 || nx >= self.w || ny < 0 || ny >= self.h {
            return false;
        }
        let from = (y * self.w + x) as usize;
        let to = (ny * self.w + nx) as usize;
        let target = self.cells[to];
        if !(target == EMPTY || (sinks && target == WATER && !self.moved[to])) {
            return false;
        }
        self.cells.swap(from, to);
        self.shade.swap(from, to);
        self.moved[to] = true;
        self.moved[from] = true;
        true
    }
}
