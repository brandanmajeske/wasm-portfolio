//! Solar Siege — an Asteroids-style arena shooter, rewritten in Rust.
//!
//! Originally built in ActionScript 3, later ported to Phaser.js; this is the
//! third life of the game. The whole engine is Rust: simulation, collision,
//! particles, and a software rasterizer that draws every pixel into an RGBA
//! buffer blitted to the canvas once per frame. Only text and the CRT overlay
//! use the browser (fillText / CSS). Sound is procedural WebAudio via web-sys.
//!
//! New over the Phaser version: splitting asteroid fields, a Dreadnought boss
//! every 5th wave, and a much heavier particle load — the kind of per-pixel
//! work that stays cheap when it compiles to WASM.

use wasm_bindgen::prelude::*;
use wasm_bindgen::{Clamped, JsCast};

mod audio;
mod fb;

use audio::Audio;
use fb::{place, Color, Fb};

const W: f32 = 800.0;
const H: f32 = 600.0;

const ROT_SPEED: f32 = std::f32::consts::PI; // 180 deg/s
const THRUST: f32 = 220.0;
const MAX_SPEED: f32 = 380.0;
const FIRE_RATE_MS: f64 = 220.0;
const BULLET_SPEED: f32 = 520.0;
const EBULLET_SPEED: f32 = 190.0;
const INVINCIBLE_MS: f64 = 2200.0;

const WAVE_BREAK_MS: f64 = 2500.0;
const SPAWN_FLOOR_MS: f64 = 700.0;
const SPAWN_START_MS: f64 = 2200.0;

const COMBO_TIMEOUT_MS: f64 = 2500.0;

const DROP_CHANCE: f32 = 0.14;
const PICKUP_LIFE_MS: f64 = 9000.0;
const PICKUP_BLINK_MS: f64 = 2000.0;
const POWERUP_DURATION_MS: f64 = 10000.0;

const DEMO_TIMEOUT_MS: f64 = 38000.0;
const ATTRACT_PAGE_MS: f64 = 5200.0;

const SLOWMO_SCALE: f64 = 0.35;
const SLOWMO_REAL_MS: f64 = 600.0;

const LS_KEY: &str = "solar-siege-highscores";
const HS_MAX: usize = 5;

const MAX_PARTICLES: usize = 1600;

const FONT: &str = "'DM Mono',monospace";

// ── Entities ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum EKind {
    Grunt,
    Darter,
    Gunner,
    Boss,
}

impl EKind {
    fn radius(self) -> f32 {
        match self {
            EKind::Grunt => 18.0,
            EKind::Darter => 12.0,
            EKind::Gunner => 20.0,
            EKind::Boss => 44.0,
        }
    }
    fn score(self) -> u32 {
        match self {
            EKind::Grunt => 100,
            EKind::Darter => 150,
            EKind::Gunner => 200,
            EKind::Boss => 2000,
        }
    }
}

#[derive(Clone, Copy)]
struct Enemy {
    kind: EKind,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    ang: f32,
    speed: f32,
    fire_every: f64,
    next_fire: f64,
    aux_next: f64, // boss radial pattern
    hp: i32,
    max_hp: i32,
    flash_until: f64,
}

#[derive(Clone, Copy)]
struct Bullet {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    ang: f32,
}

#[derive(Clone, Copy)]
struct EBullet {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum PuKind {
    Spread,
    Rapid,
    Shield,
    Life,
}

#[derive(Clone, Copy)]
struct Powerup {
    kind: PuKind,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    expire_at: f64,
}

#[derive(Clone, Copy)]
struct Asteroid {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    size: u8, // 2 large, 1 medium, 0 small
    ang: f32,
    spin: f32,
    verts: [f32; 11], // radius multiplier per vertex
    tint: f32,
}

impl Asteroid {
    fn radius(&self) -> f32 {
        match self.size {
            2 => 36.0,
            1 => 21.0,
            _ => 11.0,
        }
    }
    fn score(&self) -> u32 {
        match self.size {
            2 => 50,
            1 => 75,
            _ => 100,
        }
    }
}

#[derive(Clone, Copy)]
enum PKind {
    Fire,
    Spark,
    Debris,
    Flame,
    Ring(f32), // target radius
}

#[derive(Clone, Copy)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    born: f64,
    life: f64,
    size: f32,
    color: Color,
    kind: PKind,
    drag: f32,
}

struct Popup {
    x: f32,
    y: f32,
    text: String,
    gold: bool,
    born: f64,
}

struct Banner {
    text: String,
    color: &'static str,
    px: f64,
    born: f64,
}

#[derive(Clone, Copy)]
struct Star {
    x: f32,
    y: f32,
    size: f32,
    alpha: f32,
    speed: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Attract,
    Playing,
    WaveBreak,
    GameOver,
}

struct Input {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    fire: bool,
}

// ── Game ─────────────────────────────────────────────────────────────────

#[wasm_bindgen]
pub struct Siege {
    ctx: web_sys::CanvasRenderingContext2d,
    fb: Fb,
    rng: u32,
    audio: Audio,

    last_now: f64,
    t: f64, // game clock, ms (slow-mo scaled)
    slowmo_left: f64,

    state: State,
    demo: bool,
    paused: bool,
    /// Enhanced mode adds asteroids and boss waves on top of the faithful
    /// port. Off by default — classic is the original game, exactly.
    enhanced: bool,

    in_left: bool,
    in_right: bool,
    in_up: bool,
    in_down: bool,
    in_fire: bool,
    thrusting: bool,
    reversing: bool,

    px: f32,
    py: f32,
    pvx: f32,
    pvy: f32,
    pang: f32,
    player_visible: bool,
    last_fired: f64,
    last_thrust_sfx: f64,
    last_rev_sfx: f64,
    last_trail: f64,
    invincible_until: f64,
    spread_until: f64,
    rapid_until: f64,
    shield: bool,

    score: u32,
    lives: i32,
    combo: u32,
    last_kill_at: f64,

    wave: u32,
    budget: u32,
    spawned: u32,
    next_spawn: f64,
    wave_break_at: f64,

    enemies: Vec<Enemy>,
    bullets: Vec<Bullet>,
    ebullets: Vec<EBullet>,
    powerups: Vec<Powerup>,
    asteroids: Vec<Asteroid>,
    particles: Vec<Particle>,
    popups: Vec<Popup>,
    banners: Vec<Banner>,
    stars: Vec<Star>,

    shake_until: f64,
    shake_mag: f32,
    flash_until: f64,

    hs: Vec<(String, u32)>,

    attract_idx: usize,
    attract_next: f64,
    demo_until: f64,

    go_at: f64,
    go_theme_played: bool,
    go_flow_started: bool,
    entering_name: bool,
    table_shown: bool,
    space_armed: bool,
    name_buf: String,
}

#[wasm_bindgen]
impl Siege {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: &str) -> Result<Siege, JsValue> {
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
        ctx.set_text_baseline("middle");

        let mut g = Siege {
            ctx,
            fb: Fb::new(W as i32, H as i32),
            rng: (js_sys::Math::random() * u32::MAX as f64) as u32 | 1,
            audio: Audio::new(),
            last_now: -1.0,
            t: 0.0,
            slowmo_left: 0.0,
            state: State::Attract,
            demo: false,
            paused: false,
            enhanced: false,
            in_left: false,
            in_right: false,
            in_up: false,
            in_down: false,
            in_fire: false,
            thrusting: false,
            reversing: false,
            px: W / 2.0,
            py: H / 2.0,
            pvx: 0.0,
            pvy: 0.0,
            pang: -std::f32::consts::FRAC_PI_2,
            player_visible: true,
            last_fired: 0.0,
            last_thrust_sfx: 0.0,
            last_rev_sfx: 0.0,
            last_trail: 0.0,
            invincible_until: 0.0,
            spread_until: 0.0,
            rapid_until: 0.0,
            shield: false,
            score: 0,
            lives: 3,
            combo: 0,
            last_kill_at: 0.0,
            wave: 0,
            budget: 0,
            spawned: 0,
            next_spawn: 0.0,
            wave_break_at: 0.0,
            enemies: Vec::new(),
            bullets: Vec::new(),
            ebullets: Vec::new(),
            powerups: Vec::new(),
            asteroids: Vec::new(),
            particles: Vec::new(),
            popups: Vec::new(),
            banners: Vec::new(),
            stars: Vec::new(),
            shake_until: 0.0,
            shake_mag: 0.0,
            flash_until: 0.0,
            hs: load_high_scores(),
            attract_idx: 0,
            attract_next: 0.0,
            demo_until: 0.0,
            go_at: 0.0,
            go_theme_played: false,
            go_flow_started: false,
            entering_name: false,
            table_shown: false,
            space_armed: false,
            name_buf: String::new(),
        };
        g.reset(false);
        Ok(g)
    }

    /// Advance + render one animation frame. `now` is performance.now().
    pub fn frame(&mut self, now: f64) -> Result<(), JsValue> {
        let dt = if self.last_now < 0.0 { 16.7 } else { now - self.last_now };
        self.last_now = now;
        self.step(dt.clamp(0.0, 50.0));
        self.draw()
    }

    /// Headless test hook: advance the simulation without rendering.
    pub fn step_ms(&mut self, dt: f64) {
        self.step(dt.clamp(0.0, 50.0));
    }

    /// Headless test hook: render the current state once.
    pub fn render_once(&mut self) -> Result<(), JsValue> {
        self.draw()
    }

    /// Headless test hook: jump straight into the AI demo game.
    pub fn boot_demo(&mut self) {
        self.reset(false);
        self.launch_demo();
    }

    /// Headless test hook: skip ahead so late-game content (boss waves)
    /// can be exercised without playing through.
    pub fn jump_to_wave(&mut self, wave: u32) {
        if matches!(self.state, State::Playing | State::WaveBreak) && wave >= 1 {
            self.enemies.clear();
            self.ebullets.clear();
            self.wave = wave - 1;
            self.start_next_wave();
        }
    }

    pub fn current_score(&self) -> u32 {
        self.score
    }

    /// Switch between the faithful classic port and enhanced mode
    /// (asteroid fields + Dreadnought boss waves). Resets to the title.
    pub fn set_enhanced(&mut self, on: bool) {
        if self.enhanced != on {
            self.enhanced = on;
            self.reset(false);
        }
    }

    pub fn is_enhanced(&self) -> bool {
        self.enhanced
    }

    pub fn key(&mut self, k: &str, down: bool) {
        match k {
            "ArrowLeft" => self.in_left = down,
            "ArrowRight" => self.in_right = down,
            "ArrowUp" => self.in_up = down,
            "ArrowDown" => self.in_down = down,
            " " => self.in_fire = down,
            _ => {}
        }
        if !down {
            return;
        }
        self.audio.unlock();

        if self.entering_name {
            match k {
                "Backspace" => {
                    self.name_buf.pop();
                }
                "Enter" => self.finish_name_entry(),
                _ => {
                    if self.name_buf.len() < 3 && k.len() == 1 {
                        let c = k.chars().next().unwrap();
                        if c.is_ascii_alphabetic() {
                            self.name_buf.push(c.to_ascii_uppercase());
                        }
                    }
                }
            }
            return;
        }

        match k {
            " " => {
                if self.demo {
                    self.reset(true);
                } else {
                    match self.state {
                        State::Attract => self.start_game(),
                        State::GameOver if self.space_armed => self.reset(true),
                        _ => {}
                    }
                }
            }
            "p" | "P" | "Escape" => {
                if self.paused
                    || (!self.demo
                        && matches!(self.state, State::Playing | State::WaveBreak))
                {
                    self.paused = !self.paused;
                }
            }
            // QoL: instant restart and mute toggle.
            "r" | "R" => {
                if !self.demo
                    && matches!(
                        self.state,
                        State::Playing | State::WaveBreak | State::GameOver
                    )
                {
                    self.reset(true);
                }
            }
            "m" | "M" => self.audio.toggle_mute(),
            _ => {}
        }
    }
}

// ── Simulation ───────────────────────────────────────────────────────────

impl Siege {
    fn rand(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    fn rf(&mut self) -> f32 {
        (self.rand() >> 8) as f32 / 16_777_216.0
    }

    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.rf()
    }

    fn chance(&mut self, p: f32) -> bool {
        self.rf() < p
    }

    fn sfx_on(&self) -> bool {
        !self.demo && self.state != State::Attract
    }

    fn reset(&mut self, replay: bool) {
        self.t = 0.0;
        self.slowmo_left = 0.0;
        self.demo = false;
        self.paused = false;
        self.px = W / 2.0;
        self.py = H / 2.0;
        self.pvx = 0.0;
        self.pvy = 0.0;
        self.pang = -std::f32::consts::FRAC_PI_2;
        self.player_visible = true;
        self.last_fired = 0.0;
        self.last_thrust_sfx = 0.0;
        self.last_rev_sfx = 0.0;
        self.last_trail = 0.0;
        self.invincible_until = 0.0;
        self.spread_until = 0.0;
        self.rapid_until = 0.0;
        self.shield = false;
        self.score = 0;
        self.lives = 3;
        self.combo = 0;
        self.last_kill_at = 0.0;
        self.wave = 0;
        self.budget = 0;
        self.spawned = 0;
        self.next_spawn = f64::MAX;
        self.wave_break_at = 0.0;
        self.enemies.clear();
        self.bullets.clear();
        self.ebullets.clear();
        self.powerups.clear();
        self.asteroids.clear();
        self.particles.clear();
        self.popups.clear();
        self.banners.clear();
        self.shake_until = 0.0;
        self.flash_until = 0.0;
        self.go_theme_played = false;
        self.go_flow_started = false;
        self.entering_name = false;
        self.table_shown = false;
        self.space_armed = false;
        self.name_buf.clear();
        self.hs = load_high_scores();

        self.stars.clear();
        for _ in 0..140 {
            let size = if self.chance(0.15) { 2.0 } else { 1.0 };
            let star = Star {
                x: self.range(0.0, W),
                y: self.range(0.0, H),
                size,
                alpha: 0.35 + self.rf() * 0.65,
                speed: if size > 1.5 {
                    self.range(35.0, 65.0)
                } else {
                    self.range(12.0, 32.0)
                },
            };
            self.stars.push(star);
        }

        if replay {
            self.state = State::Playing;
            self.start_game();
        } else {
            self.state = State::Attract;
            self.attract_idx = 0;
            self.attract_next = self.t + ATTRACT_PAGE_MS;
            if self.enhanced {
                // A few rocks drifting behind the title screen; they stay
                // in play if the player starts from here.
                for _ in 0..3 {
                    self.spawn_asteroid(2);
                }
            }
        }
    }

    fn start_game(&mut self) {
        self.demo = false;
        self.state = State::Playing;
        self.wave = 0;
        self.audio.opening_theme();
        self.start_next_wave();
    }

    fn launch_demo(&mut self) {
        self.demo = true;
        self.state = State::Playing;
        self.demo_until = self.t + DEMO_TIMEOUT_MS;
        self.wave = 0;
        self.start_next_wave();
    }

    fn step(&mut self, dt_real: f64) {
        if self.paused {
            return;
        }
        let mut dtm = dt_real;
        if self.slowmo_left > 0.0 {
            self.slowmo_left -= dt_real;
            dtm *= SLOWMO_SCALE;
        }
        self.t += dtm;
        let dts = (dtm / 1000.0) as f32;

        // Stars scroll in every state, including game over.
        for s in self.stars.iter_mut() {
            s.y += s.speed * dts;
            if s.y > H + 2.0 {
                s.y = -2.0;
            }
        }

        self.update_particles(dts);
        let t = self.t;
        self.popups.retain(|p| t - p.born < 700.0);
        self.banners.retain(|b| t - b.born < 2160.0);
        self.update_powerups(dts);

        match self.state {
            State::Attract => {
                if self.t >= self.attract_next {
                    self.attract_next = self.t + ATTRACT_PAGE_MS;
                    if self.attract_idx >= 2 {
                        self.launch_demo();
                    } else {
                        self.attract_idx += 1;
                    }
                }
                // Title-screen asteroids keep drifting.
                self.update_asteroids(dts);
            }
            State::Playing | State::WaveBreak => self.gameplay(dts),
            State::GameOver => {
                // The world coasts: bullets fly out, debris drifts.
                self.update_bullets(dts);
                for e in self.enemies.iter_mut() {
                    e.x += e.vx * dts;
                    e.y += e.vy * dts;
                }
                self.update_asteroids(dts);
                self.cull_bullets();
                self.gameover_sequence();
            }
        }
    }

    fn gameplay(&mut self, dts: f32) {
        if self.demo && self.t >= self.demo_until {
            self.reset(false);
            return;
        }

        if self.combo > 0 && self.t - self.last_kill_at > COMBO_TIMEOUT_MS {
            self.combo = 0;
        }

        let inp = if self.demo {
            self.demo_inputs()
        } else {
            Input {
                left: self.in_left,
                right: self.in_right,
                up: self.in_up,
                down: self.in_down,
                fire: self.in_fire,
            }
        };

        // Rotation
        if inp.left {
            self.pang -= ROT_SPEED * dts;
        } else if inp.right {
            self.pang += ROT_SPEED * dts;
        }

        // Thrust / reverse
        let (sin, cos) = self.pang.sin_cos();
        self.thrusting = inp.up;
        self.reversing = inp.down && !inp.up;
        if self.thrusting {
            self.pvx += cos * THRUST * dts;
            self.pvy += sin * THRUST * dts;
            let spd = (self.pvx * self.pvx + self.pvy * self.pvy).sqrt();
            if spd > MAX_SPEED {
                let k = MAX_SPEED / spd;
                self.pvx *= k;
                self.pvy *= k;
            }
        } else if self.reversing {
            self.pvx -= cos * THRUST * 0.6 * dts;
            self.pvy -= sin * THRUST * 0.6 * dts;
            let cap = MAX_SPEED * 0.7;
            let spd = (self.pvx * self.pvx + self.pvy * self.pvy).sqrt();
            if spd > cap {
                let k = cap / spd;
                self.pvx *= k;
                self.pvy *= k;
            }
        }

        // Light drag — same curve as the original (0.998 per ms).
        let drag = 0.998f32.powf(dts * 1000.0);
        self.pvx *= drag;
        self.pvy *= drag;

        self.px += self.pvx * dts;
        self.py += self.pvy * dts;
        wrap_pos(&mut self.px, &mut self.py, 28.0);

        if self.thrusting {
            if self.t - self.last_thrust_sfx > 85.0 {
                self.last_thrust_sfx = self.t;
                if self.sfx_on() {
                    self.audio.thrust(false);
                }
            }
            if self.t - self.last_trail > 45.0 {
                self.last_trail = self.t;
                let (tx, ty) = (self.px - cos * 24.0, self.py - sin * 24.0);
                let size = 2.0 + self.rf() * 2.0;
                let (jx, jy) = (self.range(-8.0, 8.0), self.range(-8.0, 8.0));
                self.spawn_particle(Particle {
                    x: tx,
                    y: ty,
                    vx: -cos * 60.0 + jx,
                    vy: -sin * 60.0 + jy,
                    born: self.t,
                    life: 320.0,
                    size,
                    color: (255, 153, 0),
                    kind: PKind::Flame,
                    drag: 0.9,
                });
            }
        }
        if self.reversing && self.t - self.last_rev_sfx > 100.0 {
            self.last_rev_sfx = self.t;
            if self.sfx_on() {
                self.audio.thrust(true);
            }
        }

        // Fire — rapid halves the cooldown, spread fires a 3-bullet fan.
        let fr = if self.t < self.rapid_until {
            FIRE_RATE_MS / 2.0
        } else {
            FIRE_RATE_MS
        };
        if inp.fire && self.t - self.last_fired > fr {
            self.last_fired = self.t;
            if self.sfx_on() {
                self.audio.shoot();
            }
            if self.t < self.spread_until {
                self.spawn_bullet(self.pang);
                self.spawn_bullet(self.pang - 0.16);
                self.spawn_bullet(self.pang + 0.16);
            } else {
                self.spawn_bullet(self.pang);
            }
        }

        if self.state == State::Playing {
            self.update_enemies(dts);

            // Spawn timer
            if self.spawned < self.budget && self.t >= self.next_spawn {
                self.next_spawn = self.t + self.spawn_interval();
                if (self.enemies.len() as u32) < self.max_concurrent() {
                    self.spawn_enemy();
                    self.spawned += 1;
                }
            }
        }

        self.update_bullets(dts);
        self.update_asteroids(dts);
        self.collide();

        if self.state == State::Playing {
            self.check_wave_complete();
        } else if self.state == State::WaveBreak && self.t >= self.wave_break_at {
            self.start_next_wave();
        }

        self.cull_bullets();
    }

    // ── Waves ────────────────────────────────────────────────────────────

    fn spawn_interval(&self) -> f64 {
        (SPAWN_START_MS - (self.wave as f64 - 1.0) * 160.0).max(SPAWN_FLOOR_MS)
    }

    fn max_concurrent(&self) -> u32 {
        (6 + (self.wave - 1) / 2).min(12)
    }

    fn start_next_wave(&mut self) {
        self.wave += 1;
        self.state = State::Playing;
        self.spawned = 0;

        if self.enhanced && self.wave % 5 == 0 {
            // Boss wave: the Dreadnought plus a two-grunt escort.
            self.budget = 3;
            self.next_spawn = f64::MAX;
            self.spawn_boss();
            self.spawn_enemy_of(EKind::Grunt);
            self.spawn_enemy_of(EKind::Grunt);
            self.spawned = 3;
            self.banner(format!("WAVE {} — DREADNOUGHT", self.wave), "#ff5566", 34.0);
            if self.sfx_on() {
                self.audio.boss_siren();
            }
        } else {
            self.budget = (4 + self.wave * 2).min(22);
            self.next_spawn = self.t + self.spawn_interval();
            self.spawn_enemy();
            self.spawn_enemy();
            self.spawned = 2;
            self.banner(format!("WAVE {}", self.wave), "#44ddff", 34.0);
        }

        if self.enhanced {
            // Scatter fresh rocks at wave start, capped so the field never clogs.
            let want = (2 + self.wave / 3).min(4) as usize;
            for _ in 0..want {
                if self.asteroids.len() >= 9 {
                    break;
                }
                self.spawn_asteroid(2);
            }
        }
    }

    fn check_wave_complete(&mut self) {
        if self.spawned >= self.budget && self.enemies.is_empty() {
            self.state = State::WaveBreak;
            let bonus = 250 * self.wave;
            self.score += bonus;
            if self.sfx_on() {
                self.audio.wave_clear();
            }
            self.shake(150.0, 3.0);
            self.banner(
                format!("WAVE {} CLEAR  +{}", self.wave, bonus),
                "#44ddff",
                34.0,
            );
            self.wave_break_at = self.t + WAVE_BREAK_MS;
        }
    }

    // ── Spawning ─────────────────────────────────────────────────────────

    fn edge_point(&mut self) -> (f32, f32) {
        match self.rand() % 4 {
            0 => (self.range(0.0, W), -30.0),
            1 => (W + 30.0, self.range(0.0, H)),
            2 => (self.range(0.0, W), H + 30.0),
            _ => (-30.0, self.range(0.0, H)),
        }
    }

    fn pick_enemy_type(&mut self) -> EKind {
        let r = 1 + self.rand() % 100;
        if self.wave <= 1 {
            return EKind::Grunt;
        }
        if self.wave == 2 {
            return if r <= 65 { EKind::Grunt } else { EKind::Darter };
        }
        if r <= 45 {
            EKind::Grunt
        } else if r <= 75 {
            EKind::Darter
        } else {
            EKind::Gunner
        }
    }

    fn spawn_enemy(&mut self) {
        let kind = self.pick_enemy_type();
        self.spawn_enemy_of(kind);
    }

    fn spawn_enemy_of(&mut self, kind: EKind) {
        let (x, y) = self.edge_point();
        let speed_scale = 1.0 + (self.wave as f32 - 1.0) * 0.05;
        let (speed, fire_every, hp) = match kind {
            EKind::Grunt => (self.range(60.0, 130.0), self.range(1600.0, 3200.0), 1),
            EKind::Darter => (self.range(170.0, 230.0), self.range(4000.0, 7000.0), 1),
            EKind::Gunner => (self.range(40.0, 70.0), self.range(1400.0, 2400.0), 2),
            EKind::Boss => unreachable!(),
        };
        self.enemies.push(Enemy {
            kind,
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            ang: 0.0,
            speed: speed * speed_scale,
            fire_every: fire_every as f64,
            // The original initialized lastFired to 0, so enemies take
            // their first shot the moment they spawn. Keep that.
            next_fire: self.t,
            aux_next: 0.0,
            hp,
            max_hp: hp,
            flash_until: 0.0,
        });
    }

    fn spawn_boss(&mut self) {
        let (x, y) = self.edge_point();
        let hp = 24 + (self.wave as i32 / 5) * 10;
        self.enemies.push(Enemy {
            kind: EKind::Boss,
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            ang: 0.0,
            speed: 38.0,
            fire_every: 1300.0,
            next_fire: self.t + 1600.0,
            aux_next: self.t + 3000.0,
            hp,
            max_hp: hp,
            flash_until: 0.0,
        });
    }

    fn spawn_asteroid(&mut self, size: u8) {
        let (x, y) = self.edge_point();
        let a = self.rf() * std::f32::consts::TAU;
        let spd = self.range(20.0, 55.0);
        self.spawn_asteroid_at(x, y, a.cos() * spd, a.sin() * spd, size);
    }

    fn spawn_asteroid_at(&mut self, x: f32, y: f32, vx: f32, vy: f32, size: u8) {
        let mut verts = [1.0f32; 11];
        for v in verts.iter_mut() {
            *v = self.range(0.72, 1.18);
        }
        let asteroid = Asteroid {
            x,
            y,
            vx,
            vy,
            size,
            ang: self.rf() * std::f32::consts::TAU,
            spin: self.range(-0.8, 0.8),
            verts,
            tint: self.range(-12.0, 12.0),
        };
        self.asteroids.push(asteroid);
    }

    fn spawn_bullet(&mut self, ang: f32) {
        let (sin, cos) = ang.sin_cos();
        self.bullets.push(Bullet {
            x: self.px,
            y: self.py,
            vx: cos * BULLET_SPEED,
            vy: sin * BULLET_SPEED,
            ang,
        });
    }

    fn maybe_drop_powerup(&mut self, x: f32, y: f32, force: bool) {
        if !force && !self.chance(DROP_CHANCE) {
            return;
        }
        let roll = self.rf();
        let kind = if roll < 0.34 {
            PuKind::Spread
        } else if roll < 0.64 {
            PuKind::Rapid
        } else if roll < 0.90 {
            PuKind::Shield
        } else {
            PuKind::Life
        };
        let a = self.rf() * std::f32::consts::TAU;
        self.powerups.push(Powerup {
            kind,
            x,
            y,
            vx: a.cos() * 40.0,
            vy: a.sin() * 40.0,
            expire_at: self.t + PICKUP_LIFE_MS,
        });
    }

    // ── Per-entity updates ───────────────────────────────────────────────

    fn update_enemies(&mut self, dts: f32) {
        let damp = 0.92f32.powf(dts * 60.0);
        for i in 0..self.enemies.len() {
            let mut e = self.enemies[i];
            let ang_to = (self.py - e.y).atan2(self.px - e.x);
            let diff = wrap_angle(ang_to - e.ang);
            e.ang += diff * (5.0 * dts).min(1.0);
            let dist = ((self.px - e.x).powi(2) + (self.py - e.y).powi(2)).sqrt();
            let (sin, cos) = ang_to.sin_cos();

            match e.kind {
                EKind::Darter => {
                    e.vx = cos * e.speed;
                    e.vy = sin * e.speed;
                }
                EKind::Gunner => {
                    if dist > 260.0 {
                        e.vx = cos * e.speed;
                        e.vy = sin * e.speed;
                    } else if dist < 220.0 {
                        e.vx = -cos * e.speed;
                        e.vy = -sin * e.speed;
                    } else {
                        e.vx *= damp;
                        e.vy *= damp;
                    }
                }
                EKind::Grunt => {
                    if dist > 80.0 {
                        e.vx = cos * e.speed;
                        e.vy = sin * e.speed;
                    } else {
                        e.vx *= damp;
                        e.vy *= damp;
                    }
                }
                EKind::Boss => {
                    // Hold mid-range and strafe in a slow weave.
                    let weave = ((self.t / 1700.0).sin() * 0.65) as f32;
                    let (tsin, tcos) = (cos, -sin); // perpendicular
                    if dist > 320.0 {
                        e.vx = cos * e.speed + tcos * e.speed * weave;
                        e.vy = sin * e.speed + tsin * e.speed * weave;
                    } else if dist < 260.0 {
                        e.vx = -cos * e.speed + tcos * e.speed * weave;
                        e.vy = -sin * e.speed + tsin * e.speed * weave;
                    } else {
                        e.vx = tcos * e.speed * (0.4 + weave);
                        e.vy = tsin * e.speed * (0.4 + weave);
                    }
                }
            }

            e.x += e.vx * dts;
            e.y += e.vy * dts;
            let pad = if e.kind == EKind::Boss { 60.0 } else { 22.0 };
            wrap_pos(&mut e.x, &mut e.y, pad);

            if self.t >= e.next_fire {
                e.next_fire = self.t + e.fire_every;
                match e.kind {
                    EKind::Gunner => {
                        if self.sfx_on() {
                            self.audio.enemy_shoot();
                        }
                        for off in [-0.18f32, 0.0, 0.18] {
                            self.fire_ebullet(e.x, e.y, ang_to + off, EBULLET_SPEED);
                        }
                    }
                    EKind::Boss => {
                        if self.sfx_on() {
                            self.audio.enemy_shoot();
                        }
                        for off in [-0.3f32, -0.15, 0.0, 0.15, 0.3] {
                            self.fire_ebullet(e.x, e.y, ang_to + off, EBULLET_SPEED * 1.15);
                        }
                    }
                    _ => {
                        if self.sfx_on() {
                            self.audio.enemy_shoot();
                        }
                        self.fire_ebullet(e.x, e.y, ang_to, EBULLET_SPEED);
                    }
                }
            }

            // Boss second pattern: a full radial ring.
            if e.kind == EKind::Boss && self.t >= e.aux_next {
                e.aux_next = self.t + 3400.0;
                if self.sfx_on() {
                    self.audio.enemy_shoot();
                }
                for k in 0..14 {
                    let a = k as f32 / 14.0 * std::f32::consts::TAU + e.ang;
                    self.fire_ebullet(e.x, e.y, a, EBULLET_SPEED * 0.85);
                }
            }

            self.enemies[i] = e;
        }
    }

    fn fire_ebullet(&mut self, x: f32, y: f32, ang: f32, speed: f32) {
        let (sin, cos) = ang.sin_cos();
        self.ebullets.push(EBullet {
            x,
            y,
            vx: cos * speed,
            vy: sin * speed,
        });
    }

    /// Fly every projectile forward. Bullets don't wrap — `cull_bullets`
    /// retires them once they leave the arena.
    fn update_bullets(&mut self, dts: f32) {
        for b in self.bullets.iter_mut() {
            b.x += b.vx * dts;
            b.y += b.vy * dts;
        }
        for b in self.ebullets.iter_mut() {
            b.x += b.vx * dts;
            b.y += b.vy * dts;
        }
    }

    fn update_asteroids(&mut self, dts: f32) {
        for a in self.asteroids.iter_mut() {
            a.x += a.vx * dts;
            a.y += a.vy * dts;
            a.ang += a.spin * dts;
            wrap_pos(&mut a.x, &mut a.y, 45.0);
        }
    }

    fn update_powerups(&mut self, dts: f32) {
        let t = self.t;
        for p in self.powerups.iter_mut() {
            p.x += p.vx * dts;
            p.y += p.vy * dts;
            wrap_pos(&mut p.x, &mut p.y, 12.0);
        }
        self.powerups.retain(|p| t < p.expire_at);
    }

    fn update_particles(&mut self, dts: f32) {
        let t = self.t;
        for p in self.particles.iter_mut() {
            let drag = p.drag.powf(dts * 60.0);
            p.vx *= drag;
            p.vy *= drag;
            p.x += p.vx * dts;
            p.y += p.vy * dts;
        }
        self.particles.retain(|p| t - p.born < p.life);
    }

    fn spawn_particle(&mut self, p: Particle) {
        if self.particles.len() < MAX_PARTICLES {
            self.particles.push(p);
        }
    }

    fn cull_bullets(&mut self) {
        const PAD: f32 = 30.0;
        self.bullets
            .retain(|b| b.x > -PAD && b.x < W + PAD && b.y > -PAD && b.y < H + PAD);
        self.ebullets
            .retain(|b| b.x > -PAD && b.x < W + PAD && b.y > -PAD && b.y < H + PAD);
    }

    // ── Collision ────────────────────────────────────────────────────────

    fn collide(&mut self) {
        // Player bullets vs enemies and asteroids.
        let mut bi = 0;
        'bullets: while bi < self.bullets.len() {
            let b = self.bullets[bi];
            for ei in 0..self.enemies.len() {
                let e = self.enemies[ei];
                if circles_hit(b.x, b.y, e.x, e.y, e.kind.radius() + 4.0) {
                    self.bullets.swap_remove(bi);
                    self.impact_sparks(b.x, b.y);
                    self.damage_enemy(ei);
                    continue 'bullets;
                }
            }
            for ai in 0..self.asteroids.len() {
                let a = self.asteroids[ai];
                if circles_hit(b.x, b.y, a.x, a.y, a.radius() + 3.0) {
                    self.bullets.swap_remove(bi);
                    self.split_asteroid(ai, true);
                    continue 'bullets;
                }
            }
            bi += 1;
        }

        // Enemy bullets: blocked by asteroids, otherwise hit the player.
        let mut bi = 0;
        'ebullets: while bi < self.ebullets.len() {
            let b = self.ebullets[bi];
            for a in self.asteroids.iter() {
                if circles_hit(b.x, b.y, a.x, a.y, a.radius() + 3.0) {
                    self.ebullets.swap_remove(bi);
                    self.impact_sparks(b.x, b.y);
                    continue 'ebullets;
                }
            }
            if self.t >= self.invincible_until
                && circles_hit(b.x, b.y, self.px, self.py, 16.0 + 5.0)
            {
                self.ebullets.swap_remove(bi);
                self.damage_player();
                continue 'ebullets;
            }
            bi += 1;
        }

        // Enemy ships ram the player — the enemy dies on impact.
        if self.t >= self.invincible_until {
            let mut ei = 0;
            while ei < self.enemies.len() {
                let e = self.enemies[ei];
                if circles_hit(e.x, e.y, self.px, self.py, e.kind.radius() + 14.0) {
                    let score = e.kind.score();
                    self.explode(e.x, e.y, e.kind == EKind::Boss);
                    self.enemies.swap_remove(ei);
                    self.register_kill(e.x, e.y, score);
                    self.damage_player();
                    if self.state != State::Playing {
                        return;
                    }
                    continue;
                }
                ei += 1;
            }

            // Asteroids ram the player — the rock shatters, no score.
            let mut ai = 0;
            while ai < self.asteroids.len() {
                let a = self.asteroids[ai];
                if circles_hit(a.x, a.y, self.px, self.py, a.radius() + 12.0) {
                    self.split_asteroid(ai, false);
                    self.damage_player();
                    if self.state != State::Playing {
                        return;
                    }
                    continue;
                }
                ai += 1;
            }
        }

        // Power-up pickups.
        let mut pi = 0;
        while pi < self.powerups.len() {
            let p = self.powerups[pi];
            if circles_hit(p.x, p.y, self.px, self.py, 16.0 + 14.0) {
                self.powerups.swap_remove(pi);
                if self.sfx_on() {
                    self.audio.powerup();
                }
                self.apply_powerup(p.kind);
                continue;
            }
            pi += 1;
        }
    }

    fn damage_enemy(&mut self, ei: usize) {
        let e = &mut self.enemies[ei];
        e.hp -= 1;
        if e.hp > 0 {
            e.flash_until = self.t + 60.0;
            return;
        }
        let e = self.enemies.swap_remove(ei);
        let big = e.kind == EKind::Boss;
        self.explode(e.x, e.y, big);
        self.maybe_drop_powerup(e.x, e.y, false);
        if big {
            // The Dreadnought always pays out.
            self.maybe_drop_powerup(e.x - 20.0, e.y, true);
            self.maybe_drop_powerup(e.x + 20.0, e.y, true);
        }
        self.register_kill(e.x, e.y, e.kind.score());
        if self.state == State::Playing {
            self.check_wave_complete();
        }
    }

    fn split_asteroid(&mut self, ai: usize, scored: bool) {
        let a = self.asteroids.swap_remove(ai);
        if self.sfx_on() {
            self.audio.explosion(false);
        }
        self.asteroid_debris(a.x, a.y, a.size);
        if a.size > 0 {
            for side in [-1.0f32, 1.0] {
                let perp = (a.vy).atan2(a.vx) + side * std::f32::consts::FRAC_PI_2;
                let kick = self.range(30.0, 70.0);
                let (sin, cos) = perp.sin_cos();
                self.spawn_asteroid_at(
                    a.x + cos * 8.0,
                    a.y + sin * 8.0,
                    a.vx * 1.1 + cos * kick,
                    a.vy * 1.1 + sin * kick,
                    a.size - 1,
                );
            }
        }
        if scored {
            self.register_kill(a.x, a.y, a.score());
        }
    }

    // ── Scoring / damage ─────────────────────────────────────────────────

    fn multiplier(&self) -> u32 {
        (1 + self.combo / 3).min(5)
    }

    fn register_kill(&mut self, x: f32, y: f32, base: u32) {
        let prev = self.multiplier();
        self.combo += 1;
        self.last_kill_at = self.t;
        let mult = self.multiplier();
        let award = base * mult;
        self.score += award;
        if mult > prev && mult > 1 && self.sfx_on() {
            self.audio.combo();
        }
        let text = if mult > 1 {
            format!("+{} x{}", award, mult)
        } else {
            format!("+{}", award)
        };
        self.popups.push(Popup {
            x,
            y,
            text,
            gold: mult > 1,
            born: self.t,
        });
    }

    fn apply_powerup(&mut self, kind: PuKind) {
        match kind {
            PuKind::Spread => self.spread_until = self.t + POWERUP_DURATION_MS,
            PuKind::Rapid => self.rapid_until = self.t + POWERUP_DURATION_MS,
            PuKind::Shield => self.shield = true,
            PuKind::Life => self.lives = (self.lives + 1).min(5),
        }
    }

    fn damage_player(&mut self) {
        if self.shield {
            self.shield = false;
            if self.sfx_on() {
                self.audio.shield_break();
            }
            self.invincible_until = self.t + INVINCIBLE_MS;
            return;
        }

        self.lives -= 1;
        self.invincible_until = self.t + INVINCIBLE_MS;
        self.combo = 0;
        self.shake(240.0, 6.0);
        self.flash_until = self.t + 220.0;

        if self.lives > 0 {
            if self.sfx_on() {
                self.audio.hit();
            }
            return;
        }

        if self.demo {
            self.reset(false);
            return;
        }
        self.show_game_over();
    }

    fn show_game_over(&mut self) {
        self.state = State::GameOver;
        self.slowmo_left = SLOWMO_REAL_MS;
        self.go_at = self.t;
        self.explode(self.px, self.py, true);
        self.player_visible = false;
        self.next_spawn = f64::MAX;
    }

    fn gameover_sequence(&mut self) {
        let e = self.t - self.go_at;
        if e >= 700.0 && !self.go_theme_played {
            self.go_theme_played = true;
            self.audio.gameover_theme();
        }
        if e >= 1500.0 && !self.go_flow_started {
            self.go_flow_started = true;
            if self.score_qualifies() {
                self.entering_name = true;
            } else {
                self.table_shown = true;
                self.space_armed = true;
            }
        }
    }

    fn score_qualifies(&self) -> bool {
        if self.score == 0 {
            return false;
        }
        if self.hs.len() < HS_MAX {
            return true;
        }
        self.score > self.hs.last().map(|e| e.1).unwrap_or(0)
    }

    fn finish_name_entry(&mut self) {
        let name = if self.name_buf.is_empty() {
            "YOU".to_string()
        } else {
            self.name_buf.clone()
        };
        self.hs.push((name, self.score));
        self.hs.sort_by(|a, b| b.1.cmp(&a.1));
        self.hs.truncate(HS_MAX);
        save_high_scores(&self.hs);
        self.entering_name = false;
        self.name_buf.clear();
        self.table_shown = true;
        self.space_armed = true;
    }

    // ── Effects ──────────────────────────────────────────────────────────

    fn shake(&mut self, ms: f64, mag: f32) {
        self.shake_until = self.t + ms;
        self.shake_mag = self.shake_mag.max(mag);
    }

    fn banner(&mut self, text: String, color: &'static str, px: f64) {
        self.banners.push(Banner {
            text,
            color,
            px,
            born: self.t,
        });
    }

    fn impact_sparks(&mut self, x: f32, y: f32) {
        for _ in 0..6 {
            let a = self.rf() * std::f32::consts::TAU;
            let spd = self.range(40.0, 110.0);
            let (sin, cos) = a.sin_cos();
            self.spawn_particle(Particle {
                x,
                y,
                vx: cos * spd,
                vy: sin * spd,
                born: self.t,
                life: 240.0,
                size: 1.5,
                color: (170, 255, 255),
                kind: PKind::Spark,
                drag: 0.92,
            });
        }
    }

    fn explode(&mut self, x: f32, y: f32, big: bool) {
        if self.sfx_on() {
            self.audio.explosion(big);
        }
        self.shake(if big { 400.0 } else { 120.0 }, if big { 10.0 } else { 3.0 });

        let n_fire = if big { 46 } else { 18 };
        for _ in 0..n_fire {
            let a = self.rf() * std::f32::consts::TAU;
            let spd = self.range(30.0, if big { 260.0 } else { 180.0 });
            let life = self.range(300.0, if big { 900.0 } else { 650.0 }) as f64;
            let size = self.range(1.5, if big { 6.0 } else { 4.0 });
            let (sin, cos) = a.sin_cos();
            self.spawn_particle(Particle {
                x,
                y,
                vx: cos * spd,
                vy: sin * spd,
                born: self.t,
                life,
                size,
                color: (255, 150, 40),
                kind: PKind::Fire,
                drag: 0.9,
            });
        }
        let n_spark = if big { 14 } else { 5 };
        for _ in 0..n_spark {
            let a = self.rf() * std::f32::consts::TAU;
            let spd = self.range(200.0, 420.0);
            let life = self.range(150.0, 350.0) as f64;
            let (sin, cos) = a.sin_cos();
            self.spawn_particle(Particle {
                x,
                y,
                vx: cos * spd,
                vy: sin * spd,
                born: self.t,
                life,
                size: 1.5,
                color: (255, 240, 200),
                kind: PKind::Spark,
                drag: 0.96,
            });
        }
        // Expanding shock ring(s).
        self.spawn_particle(Particle {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            born: self.t,
            life: if big { 380.0 } else { 220.0 },
            size: 2.0,
            color: if big { (255, 120, 30) } else { (255, 80, 20) },
            kind: PKind::Ring(if big { 95.0 } else { 42.0 }),
            drag: 1.0,
        });
        if big {
            self.spawn_particle(Particle {
                x,
                y,
                vx: 0.0,
                vy: 0.0,
                born: self.t,
                life: 550.0,
                size: 3.0,
                color: (255, 200, 90),
                kind: PKind::Ring(150.0),
                drag: 1.0,
            });
        }
    }

    fn asteroid_debris(&mut self, x: f32, y: f32, size: u8) {
        let n = 10 + size as i32 * 6;
        for _ in 0..n {
            let a = self.rf() * std::f32::consts::TAU;
            let spd = self.range(20.0, 120.0);
            let shade = self.range(-20.0, 25.0);
            let g = (104.0 + shade) as u8;
            let life = self.range(350.0, 900.0) as f64;
            let size = self.range(1.0, 2.8);
            let (sin, cos) = a.sin_cos();
            self.spawn_particle(Particle {
                x,
                y,
                vx: cos * spd,
                vy: sin * spd,
                born: self.t,
                life,
                size,
                color: (g.saturating_add(8), g, g.saturating_sub(6)),
                kind: PKind::Debris,
                drag: 0.97,
            });
        }
    }

    // ── Demo AI ──────────────────────────────────────────────────────────

    fn demo_inputs(&mut self) -> Input {
        let mut out = Input {
            left: false,
            right: false,
            up: false,
            down: false,
            fire: false,
        };

        // Nearest enemy, falling back to the nearest asteroid.
        let mut best = f32::MAX;
        let mut target: Option<(f32, f32)> = None;
        for e in self.enemies.iter() {
            let d = (e.x - self.px).hypot(e.y - self.py);
            if d < best {
                best = d;
                target = Some((e.x, e.y));
            }
        }
        if target.is_none() {
            for a in self.asteroids.iter() {
                let d = (a.x - self.px).hypot(a.y - self.py);
                if d < best {
                    best = d;
                    target = Some((a.x, a.y));
                }
            }
        }
        let Some((tx, ty)) = target else {
            out.up = self.chance(0.5);
            return out;
        };

        let desired = (ty - self.py).atan2(tx - self.px);
        let diff = wrap_angle(desired - self.pang).to_degrees();
        if diff < -6.0 {
            out.left = true;
        } else if diff > 6.0 {
            out.right = true;
        }

        let aligned = diff.abs() < 18.0;
        out.fire = aligned;
        out.up = aligned && best > 150.0;

        for b in self.ebullets.iter() {
            if (b.x - self.px).hypot(b.y - self.py) < 70.0 {
                out.down = true;
                out.up = false;
                break;
            }
        }
        out
    }
}

// ── Rendering ────────────────────────────────────────────────────────────

const SHIP: [[f32; 2]; 4] = [[22.0, 0.0], [-22.0, -20.0], [-12.0, 0.0], [-22.0, 20.0]];
const GRUNT: [[f32; 2]; 4] = [[18.0, 0.0], [-18.0, 20.0], [-6.0, 0.0], [-18.0, -20.0]];
const DARTER: [[f32; 2]; 4] = [[18.0, 0.0], [-16.0, 10.0], [-6.0, 0.0], [-16.0, -10.0]];
const GUNNER_BODY: [[f32; 2]; 4] =
    [[14.0, -18.0], [14.0, 18.0], [-14.0, 18.0], [-14.0, -18.0]];
const GUNNER_NOSE: [[f32; 2]; 3] = [[20.0, 0.0], [6.0, -18.0], [6.0, 18.0]];

impl Siege {
    fn draw(&mut self) -> Result<(), JsValue> {
        // Screen shake offset, applied to world-space drawing only.
        let (ox, oy) = if self.t < self.shake_until {
            let m = self.shake_mag;
            (self.range(-m, m), self.range(-m, m))
        } else {
            self.shake_mag = 0.0;
            (0.0, 0.0)
        };

        self.fb.clear();

        for s in self.stars.iter() {
            let (x, y) = (s.x + ox * 0.4, s.y + oy * 0.4);
            self.fb.fill_rect(x, y, s.size, s.size, (255, 255, 255), s.alpha);
        }

        self.draw_powerups(ox, oy);
        self.draw_asteroids(ox, oy);

        // Engine flames sit under the ships; everything else on top.
        for i in 0..self.particles.len() {
            let p = self.particles[i];
            if matches!(p.kind, PKind::Flame) {
                self.draw_particle(&p, ox, oy);
            }
        }

        self.draw_enemies(ox, oy);
        self.draw_ebullets(ox, oy);
        self.draw_bullets(ox, oy);
        self.draw_player(ox, oy);

        for i in 0..self.particles.len() {
            let p = self.particles[i];
            if !matches!(p.kind, PKind::Flame) {
                self.draw_particle(&p, ox, oy);
            }
        }

        // Damage flash.
        if self.t < self.flash_until {
            let a = 0.55 * ((self.flash_until - self.t) / 220.0) as f32;
            self.fb.fill_rect(0.0, 0.0, W, H, (255, 255, 255), a);
        }

        let img = web_sys::ImageData::new_with_u8_clamped_array_and_sh(
            Clamped(&self.fb.px),
            W as u32,
            H as u32,
        )?;
        self.ctx.put_image_data(&img, 0.0, 0.0)?;

        // Text and HUD go straight to the canvas on top of the blit.
        self.draw_overlay();
        Ok(())
    }

    fn draw_particle(&mut self, p: &Particle, ox: f32, oy: f32) {
        let t = ((self.t - p.born) / p.life).clamp(0.0, 1.0) as f32;
        let (x, y) = (p.x + ox, p.y + oy);
        match p.kind {
            PKind::Fire => {
                // White-hot core cooling to deep red.
                let c = if t < 0.25 {
                    (255, 235, 170)
                } else if t < 0.55 {
                    (255, 140, 30)
                } else {
                    (150, 45, 15)
                };
                let size = p.size * (1.0 - t * 0.55);
                self.fb.glow(x, y, size * 2.5, c, (1.0 - t) * 0.7);
                self.fb.fill_circle(x, y, size, c, 1.0 - t * 0.6);
            }
            PKind::Spark => {
                self.fb.add(x as i32, y as i32, p.color, 1.0 - t);
                self.fb.glow(x, y, 3.0, p.color, (1.0 - t) * 0.6);
            }
            PKind::Debris => {
                self.fb.fill_circle(x, y, p.size, p.color, 1.0 - t);
            }
            PKind::Flame => {
                let size = p.size * (1.0 - t * 0.8);
                self.fb.glow(x, y, size * 3.0, p.color, (1.0 - t) * 0.8);
            }
            PKind::Ring(r1) => {
                let r = 5.0 + (r1 - 5.0) * ease_out(t);
                self.fb
                    .stroke_circle(x, y, r, p.size, p.color, (1.0 - t) * 0.9);
            }
        }
    }

    fn draw_player(&mut self, ox: f32, oy: f32) {
        if !self.player_visible {
            return;
        }
        let (x, y) = (self.px + ox, self.py + oy);

        // Invincibility blink.
        let alpha = if self.t < self.invincible_until && (self.t / 80.0) as i64 % 2 == 0 {
            0.2
        } else {
            1.0
        };

        let (sin, cos) = self.pang.sin_cos();
        if self.thrusting {
            let flick = 10.0 + self.rf() * 8.0;
            self.fb
                .glow(x - cos * 26.0, y - sin * 26.0, flick, (255, 153, 0), 0.9 * alpha);
            self.fb
                .glow(x - cos * 30.0, y - sin * 30.0, 6.0, (255, 221, 102), 0.7 * alpha);
        }
        if self.reversing {
            let flick = 5.0 + self.rf() * 6.0;
            let (nx, ny) = (x + cos * 22.0, y + sin * 22.0);
            self.fb.glow(nx, ny, 7.0, (34, 102, 255), 0.9 * alpha);
            self.fb
                .glow(nx + cos * flick, ny + sin * flick, 4.0, (170, 221, 255), 0.6 * alpha);
        }

        let pts = place(&SHIP, self.pang, x, y);
        self.fb.fill_poly(&pts, (184, 196, 204), alpha);
        self.fb
            .fill_circle(x + cos * 2.0, y + sin * 2.0, 5.0, (228, 234, 238), 0.9 * alpha);

        if self.shield {
            self.fb.stroke_circle(x, y, 30.0, 2.0, (68, 255, 136), 0.7);
            self.fb.glow(x, y, 34.0, (68, 255, 136), 0.12);
        }
    }

    fn draw_enemies(&mut self, ox: f32, oy: f32) {
        for i in 0..self.enemies.len() {
            let e = self.enemies[i];
            let (x, y) = (e.x + ox, e.y + oy);
            let flash = self.t < e.flash_until;

            match e.kind {
                EKind::Grunt => {
                    let body = if flash { (255, 255, 255) } else { (204, 34, 0) };
                    let pts = place(&GRUNT, e.ang, x, y);
                    self.fb.fill_poly(&pts, body, 1.0);
                    let (sin, cos) = e.ang.sin_cos();
                    self.fb
                        .fill_circle(x + cos * 4.0, y + sin * 4.0, 5.0, (255, 102, 68), 0.85);
                }
                EKind::Darter => {
                    let body = if flash { (255, 255, 255) } else { (255, 204, 34) };
                    let pts = place(&DARTER, e.ang, x, y);
                    self.fb.fill_poly(&pts, body, 1.0);
                    self.fb.fill_circle(x, y, 3.0, (255, 240, 153), 0.9);
                }
                EKind::Gunner => {
                    let body = if flash { (255, 255, 255) } else { (153, 68, 204) };
                    let nose = if flash { (255, 255, 255) } else { (187, 119, 221) };
                    self.fb.fill_poly(&place(&GUNNER_BODY, e.ang, x, y), body, 1.0);
                    self.fb.fill_poly(&place(&GUNNER_NOSE, e.ang, x, y), nose, 1.0);
                    self.fb.fill_circle(x, y, 6.0, (255, 102, 255), 0.85);
                }
                EKind::Boss => {
                    let body = if flash { (255, 255, 255) } else { (85, 34, 102) };
                    let plate = if flash { (255, 255, 255) } else { (119, 51, 170) };
                    let hex: Vec<[f32; 2]> = (0..6)
                        .map(|k| {
                            let a = k as f32 / 6.0 * std::f32::consts::TAU;
                            [a.cos() * 46.0, a.sin() * 38.0]
                        })
                        .collect();
                    self.fb.fill_poly(&place(&hex, e.ang, x, y), body, 1.0);
                    let inner: Vec<[f32; 2]> =
                        hex.iter().map(|p| [p[0] * 0.62, p[1] * 0.62]).collect();
                    self.fb.fill_poly(&place(&inner, -e.ang * 1.4, x, y), plate, 1.0);
                    let pulse = 0.55 + 0.45 * ((self.t / 260.0).sin() as f32).abs();
                    self.fb.glow(x, y, 18.0, (255, 102, 255), pulse);
                    self.fb.fill_circle(x, y, 8.0, (255, 102, 255), 0.9);
                    for k in 0..3 {
                        let a = e.ang * 2.0 + k as f32 / 3.0 * std::f32::consts::TAU;
                        self.fb.fill_circle(
                            x + a.cos() * 28.0,
                            y + a.sin() * 24.0,
                            4.0,
                            (255, 80, 120),
                            0.9,
                        );
                    }
                }
            }
        }
    }

    fn draw_bullets(&mut self, ox: f32, oy: f32) {
        for i in 0..self.bullets.len() {
            let b = self.bullets[i];
            let (x, y) = (b.x + ox, b.y + oy);
            let bar = [[7.0f32, -2.0], [7.0, 2.0], [-7.0, 2.0], [-7.0, -2.0]];
            self.fb.fill_poly(&place(&bar, b.ang, x, y), (68, 221, 255), 1.0);
            self.fb.glow(x, y, 7.0, (68, 221, 255), 0.5);
        }
    }

    fn draw_ebullets(&mut self, ox: f32, oy: f32) {
        for i in 0..self.ebullets.len() {
            let b = self.ebullets[i];
            let (x, y) = (b.x + ox, b.y + oy);
            self.fb.fill_circle(x, y, 5.0, (255, 85, 0), 1.0);
            self.fb.fill_circle(x - 1.0, y - 1.0, 2.5, (255, 170, 68), 0.8);
            self.fb.glow(x, y, 9.0, (255, 85, 0), 0.5);
        }
    }

    fn draw_asteroids(&mut self, ox: f32, oy: f32) {
        for i in 0..self.asteroids.len() {
            let a = self.asteroids[i];
            let (x, y) = (a.x + ox, a.y + oy);
            let r = a.radius();
            let base = (110.0 + a.tint) as u8;
            let pts: Vec<[f32; 2]> = (0..11)
                .map(|k| {
                    let ang = a.ang + k as f32 / 11.0 * std::f32::consts::TAU;
                    let rr = r * a.verts[k];
                    [x + ang.cos() * rr, y + ang.sin() * rr]
                })
                .collect();
            self.fb
                .fill_poly(&pts, (base.saturating_add(6), base, base.saturating_sub(8)), 1.0);
            // Two stable craters keyed off the vertex jitter.
            let c1 = a.verts[2] - 0.95;
            let c2 = a.verts[7] - 0.95;
            self.fb.fill_circle(
                x + c1 * r * 2.0,
                y + c2 * r * 1.6,
                r * 0.18,
                (base.saturating_sub(26), base.saturating_sub(28), base.saturating_sub(30)),
                0.9,
            );
            self.fb.fill_circle(
                x - c2 * r * 1.8,
                y + c1 * r * 2.2,
                r * 0.12,
                (base.saturating_sub(20), base.saturating_sub(22), base.saturating_sub(24)),
                0.9,
            );
        }
    }

    fn draw_powerups(&mut self, ox: f32, oy: f32) {
        for i in 0..self.powerups.len() {
            let p = self.powerups[i];
            // Blink during the final 2s of life.
            if p.expire_at - self.t < PICKUP_BLINK_MS && (self.t / 120.0) as i64 % 2 == 1 {
                continue;
            }
            let (x, y) = (p.x + ox, p.y + oy);
            let (body, dark): (Color, Color) = match p.kind {
                PuKind::Spread => ((68, 221, 255), (0, 34, 51)),
                PuKind::Rapid => ((255, 204, 34), (51, 34, 0)),
                PuKind::Shield => ((68, 255, 136), (0, 51, 17)),
                PuKind::Life => ((255, 68, 136), (51, 0, 17)),
            };
            self.fb.fill_rect(x - 11.0, y - 11.0, 22.0, 22.0, body, 1.0);
            self.fb.glow(x, y, 16.0, body, 0.25);
            match p.kind {
                PuKind::Spread => {
                    self.fb.fill_rect(x - 6.0, y - 1.0, 12.0, 2.0, dark, 1.0);
                    self.fb.fill_rect(x - 8.0, y - 5.0, 4.0, 2.0, dark, 1.0);
                    self.fb.fill_rect(x + 4.0, y - 5.0, 4.0, 2.0, dark, 1.0);
                    self.fb.fill_rect(x - 8.0, y + 3.0, 4.0, 2.0, dark, 1.0);
                    self.fb.fill_rect(x + 4.0, y + 3.0, 4.0, 2.0, dark, 1.0);
                }
                PuKind::Rapid => {
                    self.fb.fill_rect(x - 4.0, y - 7.0, 3.0, 14.0, dark, 1.0);
                    self.fb.fill_rect(x + 1.0, y - 7.0, 3.0, 14.0, dark, 1.0);
                }
                PuKind::Shield => {
                    self.fb.stroke_circle(x, y, 6.0, 2.0, dark, 1.0);
                }
                PuKind::Life => {
                    self.fb.fill_circle(x - 3.0, y - 3.0, 3.0, dark, 1.0);
                    self.fb.fill_circle(x + 3.0, y - 3.0, 3.0, dark, 1.0);
                    let heart = [[-7.0f32, -2.0], [7.0, -2.0], [0.0, 6.0]];
                    let pts: Vec<[f32; 2]> =
                        heart.iter().map(|p| [x + p[0], y + p[1]]).collect();
                    self.fb.fill_poly(&pts, dark, 1.0);
                }
            }
        }
    }

    // ── Canvas-text overlay (HUD, banners, screens) ──────────────────────

    fn text(&self, s: &str, x: f64, y: f64, px: f64, color: &str, bold: bool, align: &str) {
        self.text_a(s, x, y, px, color, bold, align, 1.0);
    }

    #[allow(clippy::too_many_arguments)]
    fn text_a(
        &self,
        s: &str,
        x: f64,
        y: f64,
        px: f64,
        color: &str,
        bold: bool,
        align: &str,
        alpha: f64,
    ) {
        let weight = if bold { "700 " } else { "" };
        self.ctx.set_font(&format!("{}{}px {}", weight, px, FONT));
        self.ctx.set_text_align(align);
        self.ctx.set_global_alpha(alpha.clamp(0.0, 1.0));
        self.ctx.set_fill_style_str(color);
        let _ = self.ctx.fill_text(s, x, y);
        self.ctx.set_global_alpha(1.0);
    }

    fn draw_overlay(&mut self) {
        let cx = (W / 2.0) as f64;
        let cy = (H / 2.0) as f64;

        // Score popups (world space).
        for p in self.popups.iter() {
            let t = ((self.t - p.born) / 700.0).clamp(0.0, 1.0);
            let color = if p.gold { "#ffcc22" } else { "#ffffff" };
            self.text_a(
                &p.text,
                p.x as f64,
                p.y as f64 - 34.0 * t,
                14.0,
                color,
                false,
                "center",
                1.0 - t,
            );
        }

        // Banners (scale-in, hold, fade).
        for b in self.banners.iter() {
            let age = self.t - b.born;
            let (scale, alpha) = if age < 260.0 {
                let t = age / 260.0;
                (0.6 + 0.4 * back_out(t), t)
            } else if age < 1760.0 {
                (1.0, 1.0)
            } else {
                (1.0, 1.0 - (age - 1760.0) / 400.0)
            };
            self.text_a(
                &b.text,
                cx,
                cy - 40.0,
                b.px * scale,
                b.color,
                true,
                "center",
                alpha,
            );
        }

        match self.state {
            State::Attract if !self.demo => self.draw_attract(cx),
            _ => {
                self.draw_hud();
                if self.demo {
                    self.text(
                        "ATTRACT MODE — PRESS SPACE",
                        cx,
                        30.0,
                        13.0,
                        "#888888",
                        false,
                        "center",
                    );
                }
                if self.state == State::GameOver {
                    self.draw_gameover(cx, cy);
                }
            }
        }

        if self.paused {
            self.ctx.set_global_alpha(0.6);
            self.ctx.set_fill_style_str("#000000");
            self.ctx.fill_rect(0.0, 0.0, W as f64, H as f64);
            self.ctx.set_global_alpha(1.0);
            self.text("PAUSED", cx, cy - 20.0, 40.0, "#44ddff", true, "center");
            self.text(
                "← → ROTATE   ↑ THRUST   ↓ REVERSE   SPACE FIRE   P/ESC RESUME   R RESTART   M MUTE",
                cx,
                cy + 28.0,
                12.0,
                "#888888",
                false,
                "center",
            );
        }
    }

    fn draw_hud(&mut self) {
        self.text(
            &format!("SCORE  {}", self.score),
            14.0,
            24.0,
            16.0,
            "#ffffff",
            false,
            "left",
        );

        // Lives as solid squares, right-aligned.
        self.ctx.set_fill_style_str("#44aaff");
        for i in 0..self.lives.max(0) {
            self.ctx
                .fill_rect(W as f64 - 24.0 - i as f64 * 18.0, 16.0, 11.0, 11.0);
        }

        // Timed power-up status.
        let mut parts: Vec<String> = Vec::new();
        let spread = ((self.spread_until - self.t) / 1000.0).ceil();
        let rapid = ((self.rapid_until - self.t) / 1000.0).ceil();
        if spread > 0.0 {
            parts.push(format!("SPREAD {}", spread));
        }
        if rapid > 0.0 {
            parts.push(format!("RAPID {}", rapid));
        }
        if self.audio.is_muted() {
            parts.push("MUTED".to_string());
        }
        if !parts.is_empty() {
            self.text(
                &parts.join("  "),
                W as f64 - 14.0,
                46.0,
                12.0,
                "#ffcc44",
                false,
                "right",
            );
        }

        if self.combo >= 3 {
            self.text(
                &format!("x{}  COMBO {}", self.multiplier(), self.combo),
                (W / 2.0) as f64,
                24.0,
                18.0,
                "#ffcc22",
                true,
                "center",
            );
        }

        // Boss health bar.
        if let Some(boss) = self.enemies.iter().find(|e| e.kind == EKind::Boss) {
            let frac = (boss.hp.max(0) as f64) / (boss.max_hp as f64);
            let bw = 300.0;
            let x = (W as f64 - bw) / 2.0;
            self.ctx.set_fill_style_str("#220011");
            self.ctx.fill_rect(x, 44.0, bw, 10.0);
            self.ctx.set_fill_style_str("#ff3355");
            self.ctx.fill_rect(x + 1.0, 45.0, (bw - 2.0) * frac, 8.0);
            self.text(
                "DREADNOUGHT",
                (W / 2.0) as f64,
                64.0,
                10.0,
                "#ff5566",
                false,
                "center",
            );
        }
    }

    fn draw_attract(&mut self, cx: f64) {
        match self.attract_idx {
            0 => {
                self.text("SOLAR SIEGE", cx, 200.0, 52.0, "#44ddff", true, "center");
                let blink = (self.t / 600.0) as i64 % 2 == 0;
                if blink {
                    self.text("PRESS SPACE TO PLAY", cx, 270.0, 18.0, "#ffffff", false, "center");
                }
                self.text(
                    "RUST → WASM REWRITE · EVERY PIXEL SOFTWARE-RENDERED",
                    cx,
                    330.0,
                    11.0,
                    "#667788",
                    false,
                    "center",
                );
            }
            1 => {
                self.text("HOW TO PLAY", cx, 180.0, 26.0, "#ffcc22", true, "center");
                let mut lines = vec![
                    "← →  ROTATE",
                    "↑  THRUST    ↓  REVERSE",
                    "SPACE  FIRE    P  PAUSE    R  RESTART    M  MUTE",
                    "",
                    "CLEAR EACH WAVE. CHAIN KILLS FOR COMBO.",
                ];
                if self.enhanced {
                    lines.push("ASTEROIDS SPLIT WHEN SHOT — AND BLOCK ENEMY FIRE.");
                    lines.push("EVERY 5TH WAVE: THE DREADNOUGHT.");
                }
                for (i, l) in lines.iter().enumerate() {
                    self.text(l, cx, 230.0 + i as f64 * 22.0, 14.0, "#ffffff", false, "center");
                }
            }
            _ => {
                self.text("HIGH SCORES", cx, 180.0, 26.0, "#ffcc22", true, "center");
                if self.hs.is_empty() {
                    self.text("NO SCORES YET", cx, 280.0, 16.0, "#ffffff", false, "center");
                } else {
                    for (i, (name, score)) in self.hs.iter().enumerate() {
                        self.text(
                            &format!("{}. {:<3}  {}", i + 1, name, score),
                            cx,
                            240.0 + i as f64 * 24.0,
                            16.0,
                            "#ffffff",
                            false,
                            "center",
                        );
                    }
                }
            }
        }
    }

    fn draw_gameover(&mut self, cx: f64, cy: f64) {
        let e = self.t - self.go_at;
        if e < 400.0 {
            return;
        }
        let scale = (0.5 + 0.5 * back_out(((e - 400.0) / 300.0).min(1.0))).min(1.0);
        self.text("GAME OVER", cx, cy - 168.0, 52.0 * scale, "#ff3300", true, "center");
        self.text(
            &format!("SCORE  {}", self.score),
            cx,
            cy - 118.0,
            26.0,
            "#ffffff",
            false,
            "center",
        );

        if self.entering_name {
            self.text(
                "NEW HIGH SCORE — TYPE NAME, ENTER",
                cx,
                cy + 56.0,
                13.0,
                "#ffcc22",
                false,
                "center",
            );
            let mut chars: Vec<String> =
                self.name_buf.chars().map(|c| c.to_string()).collect();
            while chars.len() < 3 {
                chars.push("_".to_string());
            }
            self.text(&chars.join(" "), cx, cy + 84.0, 24.0, "#ffffff", false, "center");
        }

        if self.table_shown {
            self.text("HIGH SCORES", cx, cy - 70.0, 16.0, "#ffcc22", true, "center");
            if self.hs.is_empty() {
                self.text("NO SCORES YET", cx, cy - 44.0, 15.0, "#666666", false, "center");
            } else {
                for (i, (name, score)) in self.hs.iter().enumerate() {
                    self.text(
                        &format!("{}. {:<3}  {:>7}", i + 1, name, score),
                        cx,
                        cy - 44.0 + i as f64 * 20.0,
                        15.0,
                        "#cccccc",
                        false,
                        "center",
                    );
                }
            }
            self.text(
                "SPACE TO PLAY AGAIN",
                cx,
                cy + 152.0,
                15.0,
                "#888888",
                false,
                "center",
            );
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn wrap_pos(x: &mut f32, y: &mut f32, pad: f32) {
    if *x < -pad {
        *x = W + pad;
    } else if *x > W + pad {
        *x = -pad;
    }
    if *y < -pad {
        *y = H + pad;
    } else if *y > H + pad {
        *y = -pad;
    }
}

fn wrap_angle(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (a + PI).rem_euclid(TAU) - PI
}

fn circles_hit(ax: f32, ay: f32, bx: f32, by: f32, r: f32) -> bool {
    let dx = ax - bx;
    let dy = ay - by;
    dx * dx + dy * dy < r * r
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t)
}

fn back_out(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0) - 1.0;
    const C1: f64 = 1.70158;
    1.0 + (C1 + 1.0) * t * t * t + C1 * t * t
}

fn load_high_scores() -> Vec<(String, u32)> {
    let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        return Vec::new();
    };
    let Ok(Some(raw)) = storage.get_item(LS_KEY) else {
        return Vec::new();
    };
    let mut out: Vec<(String, u32)> = raw
        .lines()
        .filter_map(|l| {
            let (name, score) = l.rsplit_once(' ')?;
            Some((name.trim().to_string(), score.parse().ok()?))
        })
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out.truncate(HS_MAX);
    out
}

fn save_high_scores(hs: &[(String, u32)]) {
    let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        return;
    };
    let raw: Vec<String> = hs.iter().map(|(n, s)| format!("{} {}", n, s)).collect();
    let _ = storage.set_item(LS_KEY, &raw.join("\n"));
}
