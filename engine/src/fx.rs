//! Carve's post-synth effects: a stereo chorus, a Freeverb-style reverb
//! and an output limiter. Every buffer is allocated in `new` (before the
//! audio stream starts) and sized for the stream's sample rate, so
//! `process` never allocates.

use crate::dsp::Smoother;

/// A delay line read at a fractional position (linear interpolation).
struct DelayLine {
    buf: Vec<f32>,
    write: usize,
}

impl DelayLine {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(2)], write: 0 }
    }

    #[inline]
    fn push(&mut self, x: f32) {
        self.buf[self.write] = x;
        self.write = (self.write + 1) % self.buf.len();
    }

    /// The sample written `delay` samples ago (fractional, >= 1).
    #[inline]
    fn read(&self, delay: f32) -> f32 {
        let len = self.buf.len();
        let delay = delay.clamp(1.0, (len - 2) as f32);
        let pos = self.write as f32 - delay + len as f32;
        let i0 = pos.floor() as usize % len;
        let i1 = (i0 + 1) % len;
        let frac = pos.fract();
        self.buf[i0] + (self.buf[i1] - self.buf[i0]) * frac
    }
}

/// Stereo chorus: one modulated delay per side, their LFOs a quarter cycle
/// apart so the two sides move differently - which is what makes it wide.
pub struct Chorus {
    left: DelayLine,
    right: DelayLine,
    phase: f32,
    sample_rate: f32,
    mix: Smoother,
    depth: Smoother,
}

const CHORUS_BASE_MS: f32 = 12.0;
const CHORUS_MAX_SWING_MS: f32 = 7.0;
const CHORUS_RATE_HZ: f32 = 0.45;

impl Chorus {
    pub fn new(sample_rate: f32) -> Self {
        let len = ((CHORUS_BASE_MS + CHORUS_MAX_SWING_MS + 2.0) * 0.001 * sample_rate) as usize + 4;
        Self {
            left: DelayLine::new(len),
            right: DelayLine::new(len),
            phase: 0.0,
            sample_rate,
            mix: Smoother::new(0.0, 20.0, sample_rate),
            depth: Smoother::new(0.0, 20.0, sample_rate),
        }
    }

    pub fn process(&mut self, l: f32, r: f32, depth: f32, mix: f32) -> (f32, f32) {
        let mix = self.mix.next(mix);
        let depth = self.depth.next(depth);
        self.left.push(l);
        self.right.push(r);
        self.phase = (self.phase + CHORUS_RATE_HZ / self.sample_rate).fract();
        if mix < 1.0e-4 {
            return (l, r);
        }
        let ms_to_samples = 0.001 * self.sample_rate;
        let swing = CHORUS_MAX_SWING_MS * depth;
        let lfo_l = (self.phase * std::f32::consts::TAU).sin();
        let lfo_r = ((self.phase + 0.25) * std::f32::consts::TAU).sin();
        let wet_l = self.left.read((CHORUS_BASE_MS + swing * lfo_l) * ms_to_samples);
        let wet_r = self.right.read((CHORUS_BASE_MS + swing * lfo_r) * ms_to_samples);
        (l * (1.0 - 0.5 * mix) + wet_l * mix * 0.7, r * (1.0 - 0.5 * mix) + wet_r * mix * 0.7)
    }
}

/// A lowpass-damped feedback comb, Freeverb's building block.
struct Comb {
    buf: Vec<f32>,
    pos: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(1)], pos: 0, store: 0.0 }
    }

    #[inline]
    fn process(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let out = self.buf[self.pos];
        self.store = out * (1.0 - damp) + self.store * damp;
        self.buf[self.pos] = x + self.store * feedback;
        self.pos = (self.pos + 1) % self.buf.len();
        out
    }
}

struct Allpass {
    buf: Vec<f32>,
    pos: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(1)], pos: 0 }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let delayed = self.buf[self.pos];
        let out = delayed - x;
        self.buf[self.pos] = x + delayed * 0.5;
        self.pos = (self.pos + 1) % self.buf.len();
        out
    }
}

/// Freeverb (Jezar's public-domain design): eight parallel damped combs
/// into four series allpasses per side, the right side's delays offset a
/// little for stereo decorrelation. Tunings are for 44.1 kHz, scaled to
/// the stream's rate.
pub struct Reverb {
    combs_l: Vec<Comb>,
    combs_r: Vec<Comb>,
    allpass_l: Vec<Allpass>,
    allpass_r: Vec<Allpass>,
    mix: Smoother,
    size: Smoother,
}

const COMB_TUNINGS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNINGS: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;
const REVERB_INPUT_GAIN: f32 = 0.015;
const REVERB_DAMP: f32 = 0.35;
const REVERB_WET_GAIN: f32 = 3.0;

impl Reverb {
    pub fn new(sample_rate: f32) -> Self {
        let scale = |n: usize| ((n as f32) * sample_rate / 44_100.0) as usize;
        Self {
            combs_l: COMB_TUNINGS.iter().map(|&n| Comb::new(scale(n))).collect(),
            combs_r: COMB_TUNINGS.iter().map(|&n| Comb::new(scale(n + STEREO_SPREAD))).collect(),
            allpass_l: ALLPASS_TUNINGS.iter().map(|&n| Allpass::new(scale(n))).collect(),
            allpass_r: ALLPASS_TUNINGS.iter().map(|&n| Allpass::new(scale(n + STEREO_SPREAD))).collect(),
            mix: Smoother::new(0.0, 20.0, sample_rate),
            size: Smoother::new(0.5, 50.0, sample_rate),
        }
    }

    pub fn process(&mut self, l: f32, r: f32, size: f32, mix: f32) -> (f32, f32) {
        let mix = self.mix.next(mix);
        let size = self.size.next(size);
        // Keep the tank running even when dry, so turning the mix up
        // later doesn't start from silence mid-phrase - but skip the work
        // while it's fully off and already empty.
        if mix < 1.0e-4 {
            return (l, r);
        }
        let feedback = 0.7 + size.clamp(0.0, 1.0) * 0.28;
        let input = (l + r) * REVERB_INPUT_GAIN;
        let mut wet_l = 0.0;
        let mut wet_r = 0.0;
        for comb in &mut self.combs_l {
            wet_l += comb.process(input, feedback, REVERB_DAMP);
        }
        for comb in &mut self.combs_r {
            wet_r += comb.process(input, feedback, REVERB_DAMP);
        }
        for ap in &mut self.allpass_l {
            wet_l = ap.process(wet_l);
        }
        for ap in &mut self.allpass_r {
            wet_r = ap.process(wet_r);
        }
        (l + wet_l * REVERB_WET_GAIN * mix, r + wet_r * REVERB_WET_GAIN * mix)
    }
}

/// A stereo-linked lookahead peak limiter: the signal is delayed by a
/// couple of milliseconds, so the gain is already down by the time a peak
/// arrives; the release is slow enough not to pump. Replaces the old
/// `tanh` soft clip, which distorted anything loud instead of controlling
/// its level.
pub struct Limiter {
    left: DelayLine,
    right: DelayLine,
    lookahead: f32,
    gain: f32,
    attack: f32,
    release: f32,
    /// The loudest recent peak, held for a full lookahead window so the
    /// gain has fully settled before that peak leaves the delay line.
    held_peak: f32,
    hold_left: u32,
}

const LIMITER_CEILING: f32 = 0.944; // -0.5 dBFS
const LIMITER_LOOKAHEAD_MS: f32 = 2.0;
const LIMITER_RELEASE_MS: f32 = 120.0;

impl Limiter {
    pub fn new(sample_rate: f32) -> Self {
        let lookahead = (LIMITER_LOOKAHEAD_MS * 0.001 * sample_rate).max(1.0);
        let coeff = |ms: f32| 1.0 - (-1.0 / (ms * 0.001 * sample_rate)).exp();
        Self {
            left: DelayLine::new(lookahead as usize + 4),
            right: DelayLine::new(lookahead as usize + 4),
            lookahead,
            gain: 1.0,
            // Settle (to within 0.1%) well inside the lookahead window.
            attack: coeff(LIMITER_LOOKAHEAD_MS / 8.0),
            release: coeff(LIMITER_RELEASE_MS),
            held_peak: 0.0,
            hold_left: 0,
        }
    }

    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let peak = l.abs().max(r.abs());
        if peak >= self.held_peak || self.hold_left == 0 {
            self.held_peak = peak;
            self.hold_left = self.lookahead as u32 + 1;
        } else {
            self.hold_left -= 1;
        }
        let target = if self.held_peak > LIMITER_CEILING { LIMITER_CEILING / self.held_peak } else { 1.0 };
        let coeff = if target < self.gain { self.attack } else { self.release };
        self.gain += (target - self.gain) * coeff;
        self.left.push(l);
        self.right.push(r);
        let out_l = self.left.read(self.lookahead) * self.gain;
        let out_r = self.right.read(self.lookahead) * self.gain;
        // Last-resort safety for the rare transient faster than the attack.
        (out_l.clamp(-1.0, 1.0), out_r.clamp(-1.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_holds_the_ceiling() {
        let mut lim = Limiter::new(48_000.0);
        let mut max_out = 0.0f32;
        for i in 0..48_000 {
            let x = 4.0 * (i as f32 * 0.05).sin();
            let (l, _) = lim.process(x, x);
            if i > 1000 {
                max_out = max_out.max(l.abs());
            }
        }
        assert!(max_out <= LIMITER_CEILING * 1.005, "{max_out}");
    }

    #[test]
    fn limiter_passes_quiet_signals_untouched() {
        let mut lim = Limiter::new(48_000.0);
        let mut max_out = 0.0f32;
        for i in 0..48_000 {
            let x = 0.5 * (i as f32 * 0.05).sin();
            let (l, _) = lim.process(x, x);
            max_out = max_out.max(l.abs());
        }
        assert!((max_out - 0.5).abs() < 0.01, "{max_out}");
    }

    #[test]
    fn reverb_tail_decays() {
        let mut rev = Reverb::new(48_000.0);
        rev.process(1.0, 1.0, 0.5, 1.0);
        let mut late = 0.0f32;
        for i in 0..48_000 * 6 {
            let (l, _) = rev.process(0.0, 0.0, 0.5, 1.0);
            if i > 48_000 * 5 {
                late = late.max(l.abs());
            }
        }
        assert!(late < 1.0e-3, "{late}");
    }
}
