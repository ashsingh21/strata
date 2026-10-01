//! Small real-time DSP building blocks shared by Carve's voices and its
//! effects: band-limiting residuals for the oscillators, parameter
//! smoothing, a DC blocker and an anti-aliased saturator. Nothing here
//! allocates.

use wide::{f32x4, i32x4};

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

/// Makes this thread treat denormal floats (the tiny values a decaying
/// reverb tail, filter or envelope passes through on its way to silence)
/// as zero. x86 CPUs process denormals up to ~100x slower, so without this
/// the CPU spiked - and could crackle - just after sounds stopped. Call at
/// the top of every audio callback and offline render (it's per thread,
/// and cheap).
#[inline]
pub fn flush_denormals() {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: only sets MXCSR's flush-to-zero and denormals-are-zero bits.
    #[allow(deprecated)]
    unsafe {
        use std::arch::x86_64::{_mm_getcsr, _mm_setcsr};
        _mm_setcsr(_mm_getcsr() | 0x8040);
    }
    #[cfg(target_arch = "aarch64")]
    // SAFETY: only sets FPCR's flush-to-zero bit.
    unsafe {
        let mut fpcr: u64;
        std::arch::asm!("mrs {}, fpcr", out(reg) fpcr);
        std::arch::asm!("msr fpcr, {}", in(reg) fpcr | (1 << 24));
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

/// How many drive channels one [`DriveBank`] runs side by side: two
/// stereo voices, one 128-bit register's worth.
pub const DRIVE_LANES: usize = 4;

fn splat(x: f32) -> f32x4 {
    f32x4::splat(x)
}

/// `exp(x)` for `x <= 0` (clamped at -30), to about 1e-8 absolute: a Taylor
/// series on the fractional power of two, scaled by building the exponent
/// bits.
#[inline(always)]
fn exp_neg(x: f32x4) -> f32x4 {
    let y = x.max(splat(-30.0)) * splat(std::f32::consts::LOG2_E);
    let whole = y.trunc_int();
    let u = (y - f32x4::from_i32x4(whole)) * splat(std::f32::consts::LN_2);
    let mut p = splat(1.0 / 362_880.0);
    for c in [1.0 / 40_320.0, 1.0 / 5040.0, 1.0 / 720.0, 1.0 / 120.0, 1.0 / 24.0, 1.0 / 6.0, 0.5, 1.0, 1.0] {
        p = p * u + splat(c);
    }
    p * f32x4::from_bits(((whole + i32x4::splat(127)) << 23i32).cast_unsigned())
}

/// `ln(1 + e)` for `0 <= e <= 1`, through the series for `atanh`.
#[inline(always)]
fn ln_1p_unit(e: f32x4) -> f32x4 {
    let s = e / (splat(2.0) + e);
    let s2 = s * s;
    let mut p = splat(1.0 / 15.0);
    for c in [1.0 / 13.0, 1.0 / 11.0, 1.0 / 9.0, 1.0 / 7.0, 1.0 / 5.0, 1.0 / 3.0, 1.0] {
        p = p * s2 + splat(c);
    }
    splat(2.0) * s * p
}

/// [`ln_cosh`] across lanes.
#[inline(always)]
fn ln_cosh_lanes(x: f32x4) -> f32x4 {
    let a = x.abs();
    a + ln_1p_unit(exp_neg(splat(-2.0) * a)) - splat(std::f32::consts::LN_2)
}

#[inline(always)]
fn tanh_lanes(x: f32x4) -> f32x4 {
    let e = exp_neg(splat(-2.0) * x.abs());
    ((splat(1.0) - e) / (splat(1.0) + e)).copysign(x)
}

/// A short history ring for four channels at once, written twice so any
/// 16-long window is contiguous. All lanes share one write position.
#[derive(Clone, Copy)]
struct LaneHistory {
    buf: [f32x4; 32],
    pos: usize,
}

impl LaneHistory {
    const ZERO: Self = Self { buf: [f32x4::ZERO; 32], pos: 0 };

    #[inline(always)]
    fn push(&mut self, x: f32x4) {
        self.pos = (self.pos + 15) % 16;
        self.buf[self.pos] = x;
        self.buf[self.pos + 16] = x;
    }

    #[inline(always)]
    fn get(&self, age: usize) -> f32x4 {
        self.buf[self.pos + age]
    }

    fn clear_lane(&mut self, lane: usize) {
        for row in &mut self.buf {
            *row = clear(*row, lane);
        }
    }
}

fn clear(v: f32x4, lane: usize) -> f32x4 {
    let mut a = v.to_array();
    a[lane] = 0.0;
    f32x4::from(a)
}

/// [`AdaaTanh`] across lanes.
#[derive(Clone, Copy)]
struct LaneAdaa {
    x1: f32x4,
    f1: f32x4,
}

impl LaneAdaa {
    #[inline(always)]
    fn process(&mut self, x: f32x4) -> f32x4 {
        let f = ln_cosh_lanes(x);
        let dx = x - self.x1;
        let wide = (f - self.f1) / dx;
        let mid = splat(0.5) * (x + self.x1);
        let small = dx.abs().simd_le(splat(1.0e-4));
        // Only a still, non-zero signal takes the slow path (silence is
        // zero either way).
        let narrow = if (small & mid.abs().simd_gt(splat(0.0))).any() { tanh_lanes(mid) } else { f32x4::ZERO };
        let y = small.select(narrow, wide);
        self.x1 = x;
        self.f1 = f;
        y
    }
}

/// [`OversampledDrive`] for `DRIVE_LANES` channels in lockstep: the same
/// filters and saturator, as SIMD. A lane whose input stays zero stays
/// silent, so unused lanes only cost their share of the vector.
#[derive(Clone, Copy)]
pub struct DriveBank {
    input: LaneHistory,
    even: LaneHistory,
    odd: LaneHistory,
    sat: LaneAdaa,
}

impl Default for DriveBank {
    fn default() -> Self {
        Self { input: LaneHistory::ZERO, even: LaneHistory::ZERO, odd: LaneHistory::ZERO, sat: LaneAdaa { x1: f32x4::ZERO, f1: f32x4::ZERO } }
    }
}

impl DriveBank {
    /// Forgets lane `lane`'s past, so a new note doesn't start on the
    /// ringing of whatever played there before.
    pub fn clear_lane(&mut self, lane: usize) {
        self.input.clear_lane(lane);
        self.even.clear_lane(lane);
        self.odd.clear_lane(lane);
        self.sat.x1 = clear(self.sat.x1, lane);
        self.sat.f1 = clear(self.sat.f1, lane);
    }

    #[inline]
    pub fn process(&mut self, x: [f32; DRIVE_LANES]) -> [f32; DRIVE_LANES] {
        self.input.push(f32x4::from(x));
        let mut up_even = f32x4::ZERO;
        for &(k, h) in &HALFBAND_EVEN_TAPS {
            up_even += splat(h) * self.input.get(k / 2);
        }
        let up_odd = splat(HALFBAND_CENTRE) * self.input.get(7);
        let a = self.sat.process(splat(2.0) * up_even);
        let b = self.sat.process(splat(2.0) * up_odd);
        self.even.push(a);
        self.odd.push(b);
        let mut down = splat(HALFBAND_CENTRE) * self.even.get(7);
        for &(k, h) in &HALFBAND_EVEN_TAPS {
            down += splat(h) * self.odd.get(k / 2);
        }
        down.to_array()
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
    fn denormals_flush_to_zero() {
        flush_denormals();
        let tiny = std::hint::black_box(f32::MIN_POSITIVE);
        // Halving the smallest normal float would make a denormal.
        assert_eq!(std::hint::black_box(tiny * 0.5), 0.0);
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

    #[test]
    fn the_drive_bank_matches_the_scalar_drive() {
        let mut scalar = [OversampledDrive::default(); DRIVE_LANES];
        let mut bank = DriveBank::default();
        let gains = [0.3f32, 1.0, 4.0, 12.0];
        let mut worst = 0.0f32;
        let mut rng = 12345u32;
        for i in 0..20_000 {
            let mut x = [0.0; DRIVE_LANES];
            for l in 0..DRIVE_LANES {
                rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (rng >> 8) as f32 / (1 << 23) as f32 - 1.0;
                let tone = (i as f32 * 0.01 * (l + 1) as f32).sin();
                // Lane 3 is silent half the time, like an idle voice.
                let on = if l == 3 { (i / 4000) % 2 == 0 } else { true };
                x[l] = if on { gains[l] * (0.8 * tone + 0.2 * noise) } else { 0.0 };
            }
            let out = bank.process(x);
            for l in 0..DRIVE_LANES {
                let want = scalar[l].process(x[l]);
                worst = worst.max((out[l] - want).abs());
            }
        }
        assert!(worst < 2.0e-3, "worst difference {worst}");
    }

    #[test]
    fn the_fast_math_agrees_with_libm() {
        let mut worst_exp = 0.0f32;
        let mut worst_ln = 0.0f32;
        for i in 0..=3000 {
            let x = -i as f32 * 0.01;
            worst_exp = worst_exp.max((exp_neg(splat(x)).to_array()[0] - x.exp()).abs());
            let e = i as f32 / 3000.0;
            worst_ln = worst_ln.max((ln_1p_unit(splat(e)).to_array()[0] - e.ln_1p()).abs());
        }
        assert!(worst_exp < 2.0e-7, "exp {worst_exp}");
        assert!(worst_ln < 2.0e-7, "ln {worst_ln}");
        assert!(ln_cosh_lanes(splat(0.0)).to_array()[0].abs() < 1.0e-7);
    }

    #[test]
    fn clearing_a_lane_silences_its_memory() {
        let mut bank = DriveBank::default();
        for i in 0..100 {
            bank.process([(i as f32 * 0.2).sin(); DRIVE_LANES]);
        }
        bank.clear_lane(1);
        let out = bank.process([0.0; DRIVE_LANES]);
        assert_eq!(out[1], 0.0);
        assert_ne!(out[0], 0.0);
    }
}
