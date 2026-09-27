//! A single-band peaking EQ: the RBJ ("Audio EQ Cookbook") peaking
//! biquad, one instance per channel. Real-time safe (fixed coefficients
//! recomputed only when the config actually changes, two state values
//! per channel, no allocation) - same config-in-`shared`/state-in-
//! `engine` split as `Compressor`.

use shared::arrangement::EqState;

#[derive(Clone, Copy, Default)]
struct BiquadState {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BiquadState {
    fn process(&mut self, x0: f32, coeffs: &Coeffs) -> f32 {
        let y0 = coeffs.b0 * x0 + coeffs.b1 * self.x1 + coeffs.b2 * self.x2
            - coeffs.a1 * self.y1
            - coeffs.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x0;
        self.y2 = self.y1;
        self.y1 = y0;
        y0
    }
}

#[derive(Clone, Copy)]
struct Coeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Coeffs {
    fn peaking(state: EqState, sample_rate: f32) -> Self {
        let freq = state.freq_hz.clamp(20.0, sample_rate * 0.49);
        let q = state.q.max(0.05);
        let a = 10f32.powf(state.gain_db / 40.0);
        let w0 = std::f32::consts::TAU * freq / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
    }
}

pub struct Eq {
    state: EqState,
    sample_rate: f32,
    coeffs: Coeffs,
    left: BiquadState,
    right: BiquadState,
}

impl Eq {
    pub fn new(sample_rate: f32) -> Self {
        let state = EqState::bypass();
        Self {
            state,
            sample_rate,
            coeffs: Coeffs::peaking(state, sample_rate),
            left: BiquadState::default(),
            right: BiquadState::default(),
        }
    }

    pub fn set_state(&mut self, state: EqState) {
        if state != self.state {
            self.coeffs = Coeffs::peaking(state, self.sample_rate);
            self.state = state;
        }
    }

    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        (self.left.process(l, &self.coeffs), self.right.process(r, &self.coeffs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settle(eq: &mut Eq, n: usize) -> f32 {
        let mut out = 0.0;
        for i in 0..n {
            let x = (i as f32 * 0.37).sin();
            out = eq.process(x, x).0;
        }
        out.abs()
    }

    #[test]
    fn zero_gain_is_close_to_a_no_op() {
        let mut eq = Eq::new(48_000.0);
        eq.set_state(EqState { freq_hz: 1000.0, gain_db: 0.0, q: 1.0 });
        // A true peaking filter at 0dB gain is an identity filter up to
        // floating point noise - feed it a few different levels and
        // confirm they come back roughly unchanged.
        for input in [0.1f32, 0.5, 0.9] {
            let mut probe = Eq::new(48_000.0);
            probe.set_state(EqState { freq_hz: 1000.0, gain_db: 0.0, q: 1.0 });
            let (l, _) = probe.process(input, input);
            assert!((l - input).abs() < 1e-3, "expected ~{input}, got {l}");
        }
        let _ = settle(&mut eq, 8);
    }

    #[test]
    fn boost_increases_energy_at_the_target_frequency() {
        let sample_rate = 48_000.0;
        let freq = 1000.0;
        let mut flat = Eq::new(sample_rate);
        flat.set_state(EqState::bypass());
        let mut boosted = Eq::new(sample_rate);
        boosted.set_state(EqState { freq_hz: freq, gain_db: 12.0, q: 1.0 });

        let n = 2000;
        let mut flat_peak = 0.0f32;
        let mut boosted_peak = 0.0f32;
        for i in 0..n {
            let x = (std::f32::consts::TAU * freq * i as f32 / sample_rate).sin();
            flat_peak = flat_peak.max(flat.process(x, x).0.abs());
            boosted_peak = boosted_peak.max(boosted.process(x, x).0.abs());
        }
        assert!(boosted_peak > flat_peak * 1.5, "expected a real boost, flat={flat_peak} boosted={boosted_peak}");
    }
}
