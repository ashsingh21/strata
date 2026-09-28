//! The four-band EQ (low cut, low shelf, bell, high shelf): one biquad
//! per band per channel, coefficients from `shared::eq` (the same maths
//! the UI draws its curve with). Real-time safe: coefficients are
//! recomputed only when the config changes, flat bands are skipped, and
//! nothing allocates - same config-in-`shared`/state-in-`engine` split as
//! `Compressor`.

use shared::arrangement::EqState;
use shared::eq::{coefficients, is_active, Biquad};

#[derive(Clone, Copy, Default)]
struct BiquadState {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BiquadState {
    #[inline]
    fn process(&mut self, x0: f32, c: &Biquad) -> f32 {
        let y0 = c.b0 * x0 + c.b1 * self.x1 + c.b2 * self.x2 - c.a1 * self.y1 - c.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x0;
        self.y2 = self.y1;
        self.y1 = y0;
        y0
    }
}

pub struct Eq {
    state: EqState,
    sample_rate: f32,
    coeffs: [Biquad; 4],
    active: [bool; 4],
    left: [BiquadState; 4],
    right: [BiquadState; 4],
}

impl Eq {
    pub fn new(sample_rate: f32) -> Self {
        let mut eq = Self {
            state: EqState::bypass(),
            sample_rate,
            coeffs: [Biquad::IDENTITY; 4],
            active: [false; 4],
            left: [BiquadState::default(); 4],
            right: [BiquadState::default(); 4],
        };
        eq.recompute();
        eq
    }

    pub fn set_state(&mut self, state: EqState) {
        if state != self.state {
            self.state = state;
            self.recompute();
        }
    }

    fn recompute(&mut self) {
        for (i, band) in self.state.bands.iter().enumerate() {
            let was = self.active[i];
            self.active[i] = is_active(band);
            self.coeffs[i] = coefficients(band, self.sample_rate);
            // A band switching on starts from silence, not stale history.
            if self.active[i] && !was {
                self.left[i] = BiquadState::default();
                self.right[i] = BiquadState::default();
            }
        }
    }

    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let (mut l, mut r) = (l, r);
        for i in 0..4 {
            if self.active[i] {
                l = self.left[i].process(l, &self.coeffs[i]);
                r = self.right[i].process(r, &self.coeffs[i]);
            }
        }
        (l, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::arrangement::{EQ_LOW_CUT, EQ_LOW_SHELF};

    /// Steady-state peak of a sine at `freq` through `eq`.
    fn level(state: EqState, freq: f32) -> f32 {
        let sr = 48_000.0;
        let mut eq = Eq::new(sr);
        eq.set_state(state);
        let mut peak = 0.0f32;
        for i in 0..9600 {
            let x = (std::f32::consts::TAU * freq * i as f32 / sr).sin();
            let y = eq.process(x, x).0;
            if i > 4800 {
                peak = peak.max(y.abs());
            }
        }
        peak
    }

    #[test]
    fn flat_passes_everything_unchanged() {
        let mut eq = Eq::new(48_000.0);
        eq.set_state(EqState::default());
        for x in [0.1f32, -0.5, 0.9] {
            assert_eq!(eq.process(x, x), (x, x));
        }
    }

    #[test]
    fn the_bell_boosts_its_frequency() {
        let boosted = level(EqState::bell(1000.0, 12.0, 1.0), 1000.0);
        assert!((20.0 * boosted.log10() - 12.0).abs() < 0.3, "{boosted}");
    }

    #[test]
    fn the_low_cut_and_shelf_shape_the_bass() {
        let mut state = EqState::default();
        state.bands[EQ_LOW_CUT].on = true;
        state.bands[EQ_LOW_CUT].freq_hz = 200.0;
        assert!(level(state, 50.0) < 0.1, "50 Hz cut");
        assert!(level(state, 2000.0) > 0.98, "2 kHz untouched");

        let mut state = EqState::default();
        state.bands[EQ_LOW_SHELF].gain_db = -12.0;
        assert!((20.0 * level(state, 40.0).log10() + 12.0).abs() < 0.5);
    }
}
