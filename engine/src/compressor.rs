//! A per-track insert compressor: feedforward and stereo-linked, with a
//! soft knee. The detector holds each peak for `HOLD_MS` before letting it
//! fall, then attack/release smooth the gain reduction. Without the hold
//! (as before), a fast release followed every cycle of a low note down to
//! its zero crossings, so the gain rippled with the waveform and distorted
//! bass.
//!
//! No lookahead: per-track effects aren't latency-compensated, and a
//! musical compressor doesn't need one (the master limiter has its own).

use shared::arrangement::CompressorState;

/// The knee's width in dB: gain reduction eases in over the 6 dB around
/// the threshold instead of switching on at it.
const KNEE_DB: f32 = 6.0;
/// Longer than half a cycle of anything down to 25 Hz, so a steady note's
/// next peak arrives before the last one is let go.
const HOLD_MS: f32 = 20.0;

pub struct Compressor {
    state: CompressorState,
    /// Smoothed gain reduction, in dB (>= 0).
    reduction_db: f32,
    attack: f32,
    release: f32,
    makeup: f32,
    held_peak: f32,
    hold_left: u32,
    hold_samples: u32,
    sample_rate: f32,
}

impl Compressor {
    pub fn new(sample_rate: f32) -> Self {
        let mut c = Self { state: CompressorState::bypass(), reduction_db: 0.0,
            attack: 1.0,
            release: 1.0,
            makeup: 1.0,
            held_peak: 0.0,
            hold_left: 0,
            hold_samples: (HOLD_MS * 0.001 * sample_rate) as u32,
            sample_rate,
        };
        c.set_state(CompressorState::bypass());
        c
    }

    /// Takes new settings; the time constants and makeup are worked out
    /// here, once, not per sample.
    pub fn set_state(&mut self, state: CompressorState) {
        self.state = state;
        self.attack = time_coeff(state.attack_ms, self.sample_rate);
        self.release = time_coeff(state.release_ms, self.sample_rate);
        self.makeup = db_to_gain(state.makeup_db);
    }

    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let ratio = self.state.ratio.max(1.0);
        let target = if ratio <= 1.0 {
            0.0
        } else {
            let now = l.abs().max(r.abs());
            if now >= self.held_peak || self.hold_left == 0 {
                self.held_peak = now;
                self.hold_left = self.hold_samples;
            } else {
                self.hold_left -= 1;
            }
            let peak = self.held_peak;
            // Well under the knee there's nothing to compute (and no log).
            let knee_floor = db_to_gain(self.state.threshold_db - KNEE_DB * 0.5);
            if peak <= knee_floor {
                0.0
            } else {
                gain_reduction_db(20.0 * peak.log10(), self.state.threshold_db, ratio)
            }
        };
        let coeff = if target > self.reduction_db { self.attack } else { self.release };
        self.reduction_db += (target - self.reduction_db) * coeff;
        let gain = if self.reduction_db < 1.0e-4 { self.makeup } else { self.makeup * db_to_gain(-self.reduction_db) };
        (l * gain, r * gain)
    }
}

/// How many dB to turn a `level_db` signal down: nothing below the knee,
/// the full ratio above it, and a quadratic blend across it.
fn gain_reduction_db(level_db: f32, threshold_db: f32, ratio: f32) -> f32 {
    let over = level_db - threshold_db;
    let slope = 1.0 - 1.0 / ratio;
    if 2.0 * over <= -KNEE_DB {
        0.0
    } else if 2.0 * over >= KNEE_DB {
        over * slope
    } else {
        slope * (over + KNEE_DB * 0.5).powi(2) / (2.0 * KNEE_DB)
    }
}

fn time_coeff(ms: f32, sample_rate: f32) -> f32 {
    1.0 - (-1.0 / (ms.max(0.1) * 0.001 * sample_rate)).exp()
}

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_state_does_not_change_level() {
        let mut c = Compressor::new(48_000.0);
        // Feed it well past any reasonable threshold - bypass (ratio 1.0)
        // must still pass the signal through unchanged.
        for _ in 0..1000 {
            let (l, r) = c.process(0.9, 0.9);
            assert!((l - 0.9).abs() < 1e-4);
            assert!((r - 0.9).abs() < 1e-4);
        }
    }

    #[test]
    fn reduces_gain_above_threshold() {
        let mut c = Compressor::new(48_000.0);
        c.set_state(CompressorState { threshold_db: -12.0, ratio: 4.0, attack_ms: 1.0, release_ms: 50.0, makeup_db: 0.0 });
        let mut out = (0.0, 0.0);
        for _ in 0..48_000 {
            out = c.process(0.9, 0.9);
        }
        assert!(out.0 < 0.9, "expected gain reduction once the envelope settles, got {}", out.0);
    }

    #[test]
    fn the_knee_is_continuous_and_reaches_the_ratio() {
        let t = -20.0;
        // Joins at both edges of the knee.
        assert!(gain_reduction_db(t - KNEE_DB / 2.0, t, 4.0).abs() < 1e-5);
        let top = gain_reduction_db(t + KNEE_DB / 2.0, t, 4.0);
        assert!((top - (KNEE_DB / 2.0) * 0.75).abs() < 1e-4, "{top}");
        // 12 dB over at 4:1 comes out 3 dB over: 9 dB of reduction.
        assert!((gain_reduction_db(t + 12.0, t, 4.0) - 9.0).abs() < 1e-4);
    }

    /// A steady low note, compressed hard with a fast release, should
    /// come out as a clean sine: its 3rd harmonic (what a gain rippling
    /// with the waveform adds) at least 40 dB down.
    #[test]
    fn a_steady_bass_note_is_not_distorted() {
        let sr = 48_000.0;
        let mut c = Compressor::new(sr);
        c.set_state(CompressorState { threshold_db: -24.0, ratio: 8.0, attack_ms: 1.0, release_ms: 30.0, makeup_db: 0.0 });
        let freq = 50.0;
        let n = sr as usize;
        let out: Vec<f32> =
            (0..n).map(|i| c.process(0.8 * (std::f32::consts::TAU * freq * i as f32 / sr).sin(), 0.0).0).collect();
        // Settled second half: an exact number of cycles.
        let tail = &out[n / 2..];
        let level = |h: f32| {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (i, y) in tail.iter().enumerate() {
                let ph = std::f64::consts::TAU * (h * freq) as f64 * i as f64 / sr as f64;
                re += *y as f64 * ph.cos();
                im += *y as f64 * ph.sin();
            }
            (re * re + im * im).sqrt()
        };
        let db = 20.0 * (level(3.0) / level(1.0)).log10();
        assert!(db < -40.0, "3rd harmonic at {db:.1} dB");
    }
}
