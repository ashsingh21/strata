//! Small real-time DSP building blocks shared by Carve's voices and its
//! effects: band-limiting residuals for the oscillators, parameter
//! smoothing, a DC blocker and an anti-aliased saturator. Nothing here
//! allocates.

/// PolyBLAMP residual (the integrated PolyBLEP) for a corner - a jump in
/// *slope* - at phase 0 of an oscillator with phase `t` in 0..1 and
/// per-sample increment `dt`. Scaled by the slope change per sample, it
/// band-limits triangles and every other piecewise-linear shape.
#[inline]
pub fn poly_blamp(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt - 1.0;
        -x * x * x / 3.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt + 1.0;
        x * x * x / 3.0
    } else {
        0.0
    }
}

/// One-pole smoothing towards a target, per sample: turns block-rate
/// parameter jumps (a knob turned while a note plays) into short ramps,
/// so they don't click or "zipper".
#[derive(Clone, Copy)]
pub struct Smoother {
    pub value: f32,
    coeff: f32,
}

impl Smoother {
    pub fn new(value: f32, time_ms: f32, sample_rate: f32) -> Self {
        Self { value, coeff: 1.0 - (-1.0 / (time_ms * 0.001 * sample_rate)).exp() }
    }

    #[inline]
    pub fn next(&mut self, target: f32) -> f32 {
        self.value += (target - self.value) * self.coeff;
        self.value
    }
}

/// First-order DC blocker (a ~10 Hz high-pass): removes the offset
/// asymmetric waveforms (narrow pulses, skewed ramps) carry, so it neither
/// biases the saturator nor eats headroom.
#[derive(Clone, Copy, Default)]
pub struct DcBlocker {
    x1: f32,
    y1: f32,
    r: f32,
}

impl DcBlocker {
    pub fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        Self { x1: 0.0, y1: 0.0, r: 1.0 - std::f32::consts::TAU * cutoff_hz / sample_rate }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + self.r * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// `ln(cosh(x))`, the antiderivative of `tanh`, written so it can't
/// overflow for large `|x|`.
#[inline]
fn ln_cosh(x: f32) -> f32 {
    let a = x.abs();
    a + (-2.0 * a).exp().ln_1p() - std::f32::consts::LN_2
}

/// `tanh` saturation with first-order antiderivative anti-aliasing (ADAA):
/// the output is the average of `tanh` over the segment between this input
/// and the last, which suppresses the aliasing plain `tanh` produces when
/// driven hard - without oversampling.
#[derive(Clone, Copy, Default)]
pub struct AdaaTanh {
    x1: f32,
    f1: f32,
}

impl AdaaTanh {
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let f = ln_cosh(x);
        let dx = x - self.x1;
        let y = if dx.abs() > 1.0e-4 { (f - self.f1) / dx } else { (0.5 * (x + self.x1)).tanh() };
        self.x1 = x;
        self.f1 = f;
        y
    }
}

// A 31-tap Blackman-windowed halfband lowpass (cutoff at a quarter of the
// oversampled rate, unity DC gain), used for both 2x up- and down-sampling.
/// The halfband's non-zero taps away from the centre: (tap index, weight).
/// Every odd tap but the centre is zero, which is what makes the
/// polyphase forms below cheap.
const HALFBAND_EVEN_TAPS: [(usize, f32); 14] = [
    (2, 0.000410323), (4, -0.002230286), (6, 0.007100857), (8, -0.017917030), (10, 0.040107418),
    (12, -0.090106922), (14, 0.312633322), (16, 0.312633322), (18, -0.090106922), (20, 0.040107418),
    (22, -0.017917030), (24, 0.007100857), (26, -0.002230286), (28, 0.000410323),
];
const HALFBAND_CENTRE: f32 = 0.500004637;

/// A short history ring, written twice so any 16-long window is contiguous.
#[derive(Clone, Copy)]
struct History {
    buf: [f32; 32],
    pos: usize,
}

impl Default for History {
    fn default() -> Self {
        Self { buf: [0.0; 32], pos: 0 }
    }
}

impl History {
    #[inline]
    fn push(&mut self, x: f32) {
        self.pos = (self.pos + 15) % 16;
        self.buf[self.pos] = x;
        self.buf[self.pos + 16] = x;
    }

    /// The sample pushed `age` pushes ago (0 = latest), `age` < 16.
    #[inline]
    fn get(&self, age: usize) -> f32 {
        self.buf[self.pos + age]
    }
}

/// The drive stage: [`AdaaTanh`] run at twice the sample rate, with
/// polyphase halfband filters for the 2x up- and down-sampling. The two
/// techniques stack - oversampling pushes the harmonics saturation creates
/// further from the fold-over point, ADAA attenuates what's left.
#[derive(Clone, Copy, Default)]
pub struct OversampledDrive {
    /// Base-rate input history (the up-sampler's zero-stuffing is implicit).
    input: History,
    /// Saturated 2x-rate samples, split by phase.
    even: History,
    odd: History,
    sat: AdaaTanh,
}

impl OversampledDrive {
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.input.push(x);
        // Up: the even output uses the even taps over the inputs, the odd
        // output only the centre tap (x2 restores the zero-stuffed level).
        let mut up_even = 0.0;
        for &(k, h) in &HALFBAND_EVEN_TAPS {
            up_even += h * self.input.get(k / 2);
        }
        let up_odd = HALFBAND_CENTRE * self.input.get(7);
        let a = self.sat.process(2.0 * up_even);
        let b = self.sat.process(2.0 * up_odd);
        self.even.push(a);
        self.odd.push(b);
        // Down: one output per pair, taken at the odd (latest) sample - so
        // the even taps land on the odd phase, the centre tap on the even.
        let mut down = HALFBAND_CENTRE * self.even.get(7);
        for &(k, h) in &HALFBAND_EVEN_TAPS {
            down += h * self.odd.get(k / 2);
        }
        down
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaa_tanh_tracks_tanh_for_slow_signals() {
        let mut sat = AdaaTanh::default();
        let mut last = 0.0;
        for i in 0..2000 {
            let x = 3.0 * (i as f32 * 0.001).sin();
            last = sat.process(x);
        }
        let x = 3.0 * (1999.0f32 * 0.001).sin();
        assert!((last - x.tanh()).abs() < 0.01, "{last} vs {}", x.tanh());
    }

    #[test]
    fn dc_blocker_removes_offset() {
        let mut dc = DcBlocker::new(10.0, 48_000.0);
        let mut y = 0.0;
        for _ in 0..48_000 {
            y = dc.process(0.5);
        }
        assert!(y.abs() < 1.0e-3, "{y}");
    }
}
