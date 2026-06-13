//! Software framebuffer: every game pixel is drawn in Rust and blitted to the
//! canvas as one ImageData per frame, same pattern as the sand demo.

pub type Color = (u8, u8, u8);

pub struct Fb {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u8>,
    /// Precomputed background gradient, one RGB per row.
    bg: Vec<Color>,
}

impl Fb {
    pub fn new(w: i32, h: i32) -> Fb {
        let bg = (0..h)
            .map(|y| {
                let t = y as f32 / h as f32;
                (
                    (12.0 + 8.0 * t) as u8,
                    (13.0 + 8.0 * t) as u8,
                    (26.0 + 12.0 * t) as u8,
                )
            })
            .collect();
        Fb { w, h, px: vec![0; (w * h * 4) as usize], bg }
    }

    pub fn clear(&mut self) {
        for y in 0..self.h {
            let (r, g, b) = self.bg[y as usize];
            let row = (y * self.w * 4) as usize;
            for x in 0..self.w as usize {
                let p = row + x * 4;
                self.px[p] = r;
                self.px[p + 1] = g;
                self.px[p + 2] = b;
                self.px[p + 3] = 255;
            }
        }
    }

    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, c: Color, a: f32) {
        if x < 0 || x >= self.w || y < 0 || y >= self.h || a <= 0.0 {
            return;
        }
        let a = a.min(1.0);
        let p = ((y * self.w + x) * 4) as usize;
        self.px[p] = (self.px[p] as f32 + (c.0 as f32 - self.px[p] as f32) * a) as u8;
        self.px[p + 1] = (self.px[p + 1] as f32 + (c.1 as f32 - self.px[p + 1] as f32) * a) as u8;
        self.px[p + 2] = (self.px[p + 2] as f32 + (c.2 as f32 - self.px[p + 2] as f32) * a) as u8;
    }

    #[inline]
    pub fn add(&mut self, x: i32, y: i32, c: Color, a: f32) {
        if x < 0 || x >= self.w || y < 0 || y >= self.h || a <= 0.0 {
            return;
        }
        let a = a.min(1.0);
        let p = ((y * self.w + x) * 4) as usize;
        self.px[p] = (self.px[p] as f32 + c.0 as f32 * a).min(255.0) as u8;
        self.px[p + 1] = (self.px[p + 1] as f32 + c.1 as f32 * a).min(255.0) as u8;
        self.px[p + 2] = (self.px[p + 2] as f32 + c.2 as f32 * a).min(255.0) as u8;
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color, a: f32) {
        let x0 = x.floor() as i32;
        let y0 = y.floor() as i32;
        let x1 = (x + w).ceil() as i32;
        let y1 = (y + h).ceil() as i32;
        for yy in y0..y1 {
            for xx in x0..x1 {
                self.blend(xx, yy, c, a);
            }
        }
    }

    pub fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, c: Color, a: f32) {
        let x0 = (cx - r).floor() as i32;
        let x1 = (cx + r).ceil() as i32;
        let y0 = (cy - r).floor() as i32;
        let y1 = (cy + r).ceil() as i32;
        let r2 = r * r;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let d2 = dx * dx + dy * dy;
                if d2 <= r2 {
                    // Soft 1px edge so small circles don't look like squares.
                    let d = d2.sqrt();
                    let edge = (r - d).min(1.0);
                    self.blend(x, y, c, a * edge);
                }
            }
        }
    }

    /// Additive radial glow — the workhorse for engine flames, bullets,
    /// explosions. Cheap enough in WASM to splash around freely.
    pub fn glow(&mut self, cx: f32, cy: f32, r: f32, c: Color, k: f32) {
        let x0 = (cx - r).floor() as i32;
        let x1 = (cx + r).ceil() as i32;
        let y0 = (cy - r).floor() as i32;
        let y1 = (cy + r).ceil() as i32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt();
                let t = 1.0 - d / r;
                if t > 0.0 {
                    self.add(x, y, c, t * t * k);
                }
            }
        }
    }

    pub fn stroke_circle(&mut self, cx: f32, cy: f32, r: f32, thick: f32, c: Color, a: f32) {
        let ro = r + thick * 0.5;
        let ri = (r - thick * 0.5).max(0.0);
        let x0 = (cx - ro).floor() as i32;
        let x1 = (cx + ro).ceil() as i32;
        let y0 = (cy - ro).floor() as i32;
        let y1 = (cy + ro).ceil() as i32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt();
                if d <= ro && d >= ri {
                    let edge = (ro - d).min(d - ri + 1.0).min(1.0);
                    self.blend(x, y, c, a * edge);
                }
            }
        }
    }

    /// Anti-aliased thick line segment — distance-to-segment with a 1px edge.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, thick: f32, c: Color, a: f32) {
        let hw = thick * 0.5;
        let minx = (x0.min(x1) - hw - 1.0).floor().max(0.0) as i32;
        let maxx = (x0.max(x1) + hw + 1.0).ceil().min(self.w as f32 - 1.0) as i32;
        let miny = (y0.min(y1) - hw - 1.0).floor().max(0.0) as i32;
        let maxy = (y0.max(y1) + hw + 1.0).ceil().min(self.h as f32 - 1.0) as i32;
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len2 = dx * dx + dy * dy;
        for y in miny..=maxy {
            for x in minx..=maxx {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let t = if len2 > 0.0 {
                    (((px - x0) * dx + (py - y0) * dy) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let d = ((px - (x0 + t * dx)).powi(2) + (py - (y0 + t * dy)).powi(2)).sqrt();
                let edge = (hw - d + 0.5).clamp(0.0, 1.0);
                if edge > 0.0 {
                    self.blend(x, y, c, a * edge);
                }
            }
        }
    }

    /// Stroke a closed polygon outline — the luminous edges on the hero hull.
    pub fn stroke_poly(&mut self, pts: &[[f32; 2]], thick: f32, c: Color, a: f32) {
        let n = pts.len();
        if n < 2 {
            return;
        }
        for i in 0..n {
            let p = pts[i];
            let q = pts[(i + 1) % n];
            self.line(p[0], p[1], q[0], q[1], thick, c, a);
        }
    }

    /// Even-odd scanline fill — handles every ship/asteroid silhouette here.
    pub fn fill_poly(&mut self, pts: &[[f32; 2]], c: Color, a: f32) {
        let n = pts.len();
        if n < 3 {
            return;
        }
        let mut ymin = f32::MAX;
        let mut ymax = f32::MIN;
        for p in pts {
            ymin = ymin.min(p[1]);
            ymax = ymax.max(p[1]);
        }
        let ys = (ymin.floor().max(0.0)) as i32;
        let ye = (ymax.ceil().min(self.h as f32 - 1.0)) as i32;
        for y in ys..=ye {
            let fy = y as f32 + 0.5;
            let mut ix = [0f32; 16];
            let mut cnt = 0usize;
            for i in 0..n {
                let p = pts[i];
                let q = pts[(i + 1) % n];
                if (p[1] <= fy && q[1] > fy) || (q[1] <= fy && p[1] > fy) {
                    if cnt < 16 {
                        let t = (fy - p[1]) / (q[1] - p[1]);
                        ix[cnt] = p[0] + t * (q[0] - p[0]);
                        cnt += 1;
                    }
                }
            }
            ix[..cnt].sort_by(|x, y| x.partial_cmp(y).unwrap());
            let mut i = 0;
            while i + 1 < cnt {
                let xa = ix[i].round().max(0.0) as i32;
                let xb = ix[i + 1].round().min(self.w as f32 - 1.0) as i32;
                for x in xa..=xb {
                    self.blend(x, y, c, a);
                }
                i += 2;
            }
        }
    }
}

/// Rotate model-space points (+x = forward) by `ang` and translate to (cx, cy).
pub fn place(model: &[[f32; 2]], ang: f32, cx: f32, cy: f32) -> Vec<[f32; 2]> {
    let (s, c) = ang.sin_cos();
    model
        .iter()
        .map(|p| [cx + p[0] * c - p[1] * s, cy + p[0] * s + p[1] * c])
        .collect()
}
