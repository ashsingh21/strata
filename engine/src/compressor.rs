//! A per-track insert compressor: a feedforward peak detector (attack/
//! release smoothed, same idiom as `fx::Limiter`) driving a linear gain
//! reduction. Unlike `Limiter` (always-on, fixed, no lookahead delay so it
//! stays cheap), this is user-configurable and has no lookahead - it's a
//! musical effect the user dials in, not a safety net.

use shared::arrangement::CompressorState;

pub struct Compressor {
    state: CompressorState,
    /// The detector's smoothed level, in dB.
    envelope_db: f32,
    sample_rate: f32,
}

impl Compressor {
    pub fn new(sample_rate: f32) -> Self {
        Self { state: CompressorState::bypass(), envelope_db: -100.0, sample_rate }
    }

    pub fn set_state(&mut self, state: CompressorState) {
        self.state = state;
    }

    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let peak = l.abs().max(r.abs()).max(1e-6);
        let level_db = 20.0 * peak.log10();
        let coeff = if level_db > self.envelope_db {
            time_coeff(self.state.attack_ms, self.sample_rate)
        } else {
            time_coeff(self.state.release_ms, self.sample_rate)
        };
        self.envelope_db += (level_db - self.envelope_db) * coeff;

        let over_db = (self.envelope_db - self.state.threshold_db).max(0.0);
        let gain_reduction_db = over_db * (1.0 - 1.0 / self.state.ratio.max(1.0));
        let gain = db_to_gain(self.state.makeup_db - gain_reduction_db);
        (l * gain, r * gain)
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
}
