//! Procedural sound — a direct port of the original game's WebAudio synth.
//! Every effect is an oscillator/noise patch built on the fly; no samples.

use web_sys::{AudioContext, AudioContextState, BiquadFilterType, OscillatorType};

pub struct Audio {
    ctx: Option<AudioContext>,
    /// No AudioContext until the user has pressed a key — browsers block it.
    unlocked: bool,
    muted: bool,
}

impl Audio {
    pub fn new() -> Audio {
        Audio { ctx: None, unlocked: false, muted: false }
    }

    pub fn unlock(&mut self) {
        self.unlocked = true;
    }

    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    fn ac(&mut self) -> Option<&AudioContext> {
        if !self.unlocked || self.muted {
            return None;
        }
        if self.ctx.is_none() {
            self.ctx = AudioContext::new().ok();
        }
        let ctx = self.ctx.as_ref()?;
        if ctx.state() == AudioContextState::Suspended {
            let _ = ctx.resume();
        }
        Some(ctx)
    }

    /// Square-wave blip with an exponential pitch slide and decay.
    fn sweep(&mut self, kind: OscillatorType, f0: f32, f1: f32, dur: f64, vol: f32) {
        let Some(ctx) = self.ac() else { return };
        let now = ctx.current_time();
        let _ = (|| -> Result<(), wasm_bindgen::JsValue> {
            let osc = ctx.create_oscillator()?;
            let g = ctx.create_gain()?;
            osc.set_type(kind);
            osc.frequency().set_value_at_time(f0, now)?;
            osc.frequency().exponential_ramp_to_value_at_time(f1, now + dur)?;
            g.gain().set_value_at_time(vol, now)?;
            g.gain().exponential_ramp_to_value_at_time(0.001, now + dur)?;
            osc.connect_with_audio_node(&g)?;
            g.connect_with_audio_node(&ctx.destination())?;
            osc.start()?;
            osc.stop_with_when(now + dur)?;
            Ok(())
        })();
    }

    /// White-noise burst through a filter — explosions and thruster hiss.
    fn noise(&mut self, dur: f64, filter: BiquadFilterType, freq: f32, q: f32, vol: f32) {
        let Some(ctx) = self.ac() else { return };
        let now = ctx.current_time();
        let _ = (|| -> Result<(), wasm_bindgen::JsValue> {
            let sr = ctx.sample_rate();
            let len = (sr as f64 * dur) as u32;
            let buf = ctx.create_buffer(1, len, sr)?;
            let mut data = vec![0f32; len as usize];
            let mut seed: u32 = 0x9e3779b9 ^ len;
            for v in data.iter_mut() {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                *v = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
            }
            buf.copy_to_channel(&data, 0)?;
            let src = ctx.create_buffer_source()?;
            src.set_buffer(Some(&buf));
            let filt = ctx.create_biquad_filter()?;
            filt.set_type(filter);
            filt.frequency().set_value(freq);
            filt.q().set_value(q);
            let g = ctx.create_gain()?;
            g.gain().set_value_at_time(vol, now)?;
            g.gain().exponential_ramp_to_value_at_time(0.001, now + dur)?;
            src.connect_with_audio_node(&filt)?;
            filt.connect_with_audio_node(&g)?;
            g.connect_with_audio_node(&ctx.destination())?;
            src.start()?;
            Ok(())
        })();
    }

    fn note(ctx: &AudioContext, freq: f32, start: f64, dur: f64, vol: f32) {
        let _ = (|| -> Result<(), wasm_bindgen::JsValue> {
            let osc = ctx.create_oscillator()?;
            let g = ctx.create_gain()?;
            osc.set_type(OscillatorType::Square);
            osc.frequency().set_value(freq);
            g.gain().set_value_at_time(vol, start)?;
            g.gain().set_value_at_time(vol * 0.55, start + dur * 0.7)?;
            g.gain().linear_ramp_to_value_at_time(0.001, start + dur)?;
            osc.connect_with_audio_node(&g)?;
            g.connect_with_audio_node(&ctx.destination())?;
            osc.start_with_when(start)?;
            osc.stop_with_when(start + dur + 0.01)?;
            Ok(())
        })();
    }

    fn melody(&mut self, lead_in: f64, vol: f32, notes: &[(f32, f64, f64)]) {
        let Some(ctx) = self.ac() else { return };
        let t = ctx.current_time() + lead_in;
        // Clone to drop the borrow of self before the loop (ctx is Rc-like).
        let ctx = ctx.clone();
        for &(f, s, d) in notes {
            Self::note(&ctx, f, t + s, d, vol);
        }
    }

    // ── Effects ──────────────────────────────────────────────────────────

    pub fn shoot(&mut self) {
        self.sweep(OscillatorType::Square, 880.0, 180.0, 0.09, 0.14);
    }

    pub fn enemy_shoot(&mut self) {
        self.sweep(OscillatorType::Sawtooth, 280.0, 70.0, 0.16, 0.08);
    }

    pub fn explosion(&mut self, big: bool) {
        let dur = if big { 0.65 } else { 0.3 };
        let freq = if big { 500.0 } else { 1100.0 };
        let vol = if big { 0.35 } else { 0.22 };
        self.noise(dur, BiquadFilterType::Lowpass, freq, 1.0, vol);
        if big {
            self.sweep(OscillatorType::Sine, 55.0, 18.0, 0.55, 0.28);
        }
    }

    pub fn hit(&mut self) {
        self.sweep(OscillatorType::Sine, 140.0, 35.0, 0.22, 0.28);
    }

    pub fn thrust(&mut self, reverse: bool) {
        let f = if reverse { 280.0 } else { 750.0 };
        self.noise(0.06, BiquadFilterType::Bandpass, f, 1.5, 0.07);
    }

    pub fn shield_break(&mut self) {
        self.sweep(OscillatorType::Triangle, 520.0, 120.0, 0.2, 0.2);
    }

    pub fn combo(&mut self) {
        self.sweep(OscillatorType::Square, 1200.0, 1800.0, 0.07, 0.1);
    }

    pub fn powerup(&mut self) {
        self.melody(0.01, 0.13, &[
            (660.0, 0.00, 0.08),
            (880.0, 0.06, 0.08),
            (1320.0, 0.12, 0.08),
        ]);
    }

    pub fn wave_clear(&mut self) {
        self.melody(0.02, 0.15, &[
            (523.25, 0.00, 0.12),
            (659.25, 0.10, 0.12),
            (783.99, 0.20, 0.12),
            (1046.5, 0.30, 0.12),
        ]);
    }

    /// Two-tone klaxon for the boss wave intro.
    pub fn boss_siren(&mut self) {
        self.melody(0.02, 0.14, &[
            (220.0, 0.00, 0.28),
            (311.13, 0.30, 0.28),
            (220.0, 0.60, 0.28),
            (311.13, 0.90, 0.38),
        ]);
    }

    pub fn opening_theme(&mut self) {
        self.melody(0.05, 0.16, &[
            // Phrase 1 — C major ascending fanfare
            (523.25, 0.00, 0.09),
            (659.25, 0.11, 0.09),
            (783.99, 0.22, 0.09),
            (1046.5, 0.33, 0.22),
            (783.99, 0.57, 0.07),
            (659.25, 0.66, 0.07),
            (783.99, 0.75, 0.07),
            (1046.5, 0.84, 0.09),
            // Phrase 2 — scale run to final note
            (523.25, 0.98, 0.07),
            (587.33, 1.07, 0.07),
            (659.25, 1.16, 0.07),
            (698.46, 1.25, 0.07),
            (783.99, 1.34, 0.07),
            (880.00, 1.43, 0.07),
            (987.77, 1.52, 0.07),
            (1046.5, 1.61, 0.45),
        ]);
    }

    pub fn gameover_theme(&mut self) {
        self.melody(0.08, 0.19, &[
            // 8-note chromatic descent — classic arcade "you lose"
            (392.00, 0.00, 0.14),
            (369.99, 0.15, 0.14),
            (349.23, 0.30, 0.14),
            (329.63, 0.45, 0.14),
            (311.13, 0.60, 0.14),
            (293.66, 0.75, 0.14),
            (277.18, 0.90, 0.14),
            (261.63, 1.05, 0.45),
        ]);
    }
}
