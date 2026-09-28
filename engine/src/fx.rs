//! Carve's post-synth effects: a stereo chorus, a plate reverb
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

/// A delay line read at whole-sample taps (the plate's output taps read
/// several points inside it).
struct Taps {
    buf: Vec<f32>,
    write: usize,
}

impl Taps {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(2)], write: 0 }
    }

    #[inline]
    fn push(&mut self, x: f32) {
        self.buf[self.write] = x;
        self.write = (self.write + 1) % self.buf.len();
    }

    /// The sample written `n` samples ago (1 = the latest).
    #[inline]
    fn tap(&self, n: usize) -> f32 {
        let len = self.buf.len();
        self.buf[(self.write + len - n.clamp(1, len)) % len]
    }

    #[inline]
    fn tap_frac(&self, delay: f32) -> f32 {
        let len = self.buf.len();
        let delay = delay.clamp(1.0, (len - 2) as f32);
        let pos = self.write as f32 - delay + len as f32;
        let i0 = pos.floor() as usize % len;
        let i1 = (i0 + 1) % len;
        self.buf[i0] + (self.buf[i1] - self.buf[i0]) * pos.fract()
    }
}

/// A Schroeder allpass of `len` samples (fractional, so the tank's can
/// be modulated), gain `g`.
struct Diffuser {
    line: Taps,
    len: f32,
}

impl Diffuser {
    fn new(len: f32, extra: usize) -> Self {
        Self { line: Taps::new(len as usize + extra + 4), len }
    }

    #[inline]
    fn process(&mut self, x: f32, g: f32, delay: f32) -> f32 {
        let delayed = self.line.tap_frac(delay);
        let v = x - g * delayed;
        self.line.push(v);
        delayed + g * v
    }
}

/// Jon Dattorro's plate ("Effect Design Part 1", JAES 1997): four input
/// diffusers into a figure-eight tank of two modulated allpass + delay
/// halves, stereo taken from taps spread through the tank. Replaces a
/// Freeverb (parallel combs), whose tails rang metallic. Lengths are the
/// paper's, for 29.761 kHz, scaled to the stream's rate.
pub struct Reverb {
    input_lp: f32,
    input: [Diffuser; 4],
    // Tank, left half then right half.
    mod_a: Diffuser,
    delay_a: Taps,
    damp_a: f32,
    ap_a: Diffuser,
    delay_a2: Taps,
    mod_b: Diffuser,
    delay_b: Taps,
    damp_b: f32,
    ap_b: Diffuser,
    delay_b2: Taps,
    lens: [f32; 8],
    scale: f32,
    lfo: f32,
    lfo_step: f32,
    mix: Smoother,
    size: Smoother,
}

const PLATE_RATE: f32 = 29_761.0;
const INPUT_DIFFUSERS: [(f32, f32); 4] = [(142.0, 0.75), (107.0, 0.75), (379.0, 0.625), (277.0, 0.625)];
/// Tank lengths: mod allpass, delay, allpass, delay - left, then right.
const TANK: [f32; 8] = [672.0, 4453.0, 1800.0, 3720.0, 908.0, 4217.0, 2656.0, 3163.0];
const MOD_EXCURSION: f32 = 16.0;
const MOD_RATE_HZ: f32 = 0.9;
/// How much of the input's top end survives into the tank.
const REVERB_BANDWIDTH: f32 = 0.9995;
const REVERB_DAMP: f32 = 0.35;
/// Wet level, set so a sound's reverb is as loud as the old Freeverb's.
const REVERB_WET_GAIN: f32 = 1.53;

impl Reverb {
    pub fn new(sample_rate: f32) -> Self {
        let scale = sample_rate / PLATE_RATE;
        let lens = TANK.map(|n| n * scale);
        let excursion = (MOD_EXCURSION * scale).ceil() as usize;
        Self {
            input_lp: 0.0,
            input: INPUT_DIFFUSERS.map(|(n, _)| Diffuser::new(n * scale, 0)),
            mod_a: Diffuser::new(lens[0], excursion),
            delay_a: Taps::new(lens[1] as usize + 1),
            damp_a: 0.0,
            ap_a: Diffuser::new(lens[2], 0),
            delay_a2: Taps::new(lens[3] as usize + 1),
            mod_b: Diffuser::new(lens[4], excursion),
            delay_b: Taps::new(lens[5] as usize + 1),
            damp_b: 0.0,
            ap_b: Diffuser::new(lens[6], 0),
            delay_b2: Taps::new(lens[7] as usize + 1),
            lens,
            scale,
            lfo: 0.0,
            lfo_step: MOD_RATE_HZ / sample_rate,
            mix: Smoother::new(0.0, 20.0, sample_rate),
            size: Smoother::new(0.5, 50.0, sample_rate),
        }
    }

    /// A tap `n` paper-samples into a line, at this rate.
    #[inline]
    fn t(&self, n: f32) -> usize {
        (n * self.scale) as usize
    }

    pub fn process(&mut self, l: f32, r: f32, size: f32, mix: f32) -> (f32, f32) {
        let mix = self.mix.next(mix);
        let size = self.size.next(size);
        if mix < 1.0e-4 {
            return (l, r);
        }
        // Fitted so each size decays as long as the old Freeverb's did
        // (RT60 about 0.8 s at 0.2, 1.2 s at 0.5, 2.3 s at 0.8) - presets
        // keep their space.
        let decay = (-1.9 + 1.69 * size.clamp(0.0, 1.0)).exp();

        // Input: mono, band-limited, diffused.
        self.input_lp += ((l + r) * 0.5 - self.input_lp) * REVERB_BANDWIDTH;
        let mut x = self.input_lp;
        for (d, (_, g)) in self.input.iter_mut().zip(INPUT_DIFFUSERS) {
            let len = d.len;
            x = d.process(x, g, len);
        }

        // The tank: each half is fed by the other's end.
        self.lfo = (self.lfo + self.lfo_step).fract();
        let wobble = (self.lfo * std::f32::consts::TAU).sin() * MOD_EXCURSION * self.scale;
        let end_a = self.delay_a2.tap(self.lens[3] as usize);
        let end_b = self.delay_b2.tap(self.lens[7] as usize);

        let a = self.mod_a.process(x + end_b * decay, -0.7, self.lens[0] + wobble);
        self.delay_a.push(a);
        let a = self.delay_a.tap(self.lens[1] as usize);
        self.damp_a += (a - self.damp_a) * (1.0 - REVERB_DAMP);
        let a = self.ap_a.process(self.damp_a * decay, 0.5, self.lens[2]);
        self.delay_a2.push(a);

        let b = self.mod_b.process(x + end_a * decay, -0.7, self.lens[4] - wobble);
        self.delay_b.push(b);
        let b = self.delay_b.tap(self.lens[5] as usize);
        self.damp_b += (b - self.damp_b) * (1.0 - REVERB_DAMP);
        let b = self.ap_b.process(self.damp_b * decay, 0.5, self.lens[6]);
        self.delay_b2.push(b);

        // Output taps (the paper's table).
        let wet_l = self.delay_b.tap(self.t(266.0)) + self.delay_b.tap(self.t(2974.0)) - self.ap_b.line.tap(self.t(1913.0))
            + self.delay_b2.tap(self.t(1996.0))
            - self.delay_a.tap(self.t(1990.0))
            - self.ap_a.line.tap(self.t(187.0))
            - self.delay_a2.tap(self.t(1066.0));
        let wet_r = self.delay_a.tap(self.t(353.0)) + self.delay_a.tap(self.t(3627.0)) - self.ap_a.line.tap(self.t(1228.0))
            + self.delay_a2.tap(self.t(2673.0))
            - self.delay_b.tap(self.t(2111.0))
            - self.ap_b.line.tap(self.t(335.0))
            - self.delay_b2.tap(self.t(121.0));
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
        // A whole number of samples, so its latency can be compensated
        // exactly (see `latency`).
        let lookahead = (LIMITER_LOOKAHEAD_MS * 0.001 * sample_rate).round().max(2.0);
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

    /// How many samples late the output is (the lookahead).
    pub fn latency(&self) -> usize {
        self.lookahead as usize - 1
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

    /// The tail is smooth, not metallic: no band of it rings far above the
    /// rest. Measured as the 20 loudest bins of its spectrum (300 ms - 1 s,
    /// 200 Hz - 8 kHz) over the median; plain noise scores about 11 dB, the
    /// old Freeverb 20-23 dB.
    #[test]
    fn reverb_tail_does_not_ring() {
        for size in [0.5f32, 0.8] {
            let mut rev = Reverb::new(48_000.0);
            let tail: Vec<f32> = (0..48_000)
                .map(|i| {
                    let x = if i == 0 { 1.0 } else { 0.0 };
                    rev.process(x, x, size, 1.0).0 - x
                })
                .collect();
            let mut re = tail[14_400..14_400 + 32_768].to_vec();
            let mut im = vec![0.0; re.len()];
            shared::analysis::fft(&mut re, &mut im);
            let bin = |hz: f32| (hz / 48_000.0 * 32_768.0) as usize;
            let mut band: Vec<f32> =
                (bin(200.0)..bin(8000.0)).map(|i| (re[i] * re[i] + im[i] * im[i]).sqrt()).collect();
            band.sort_by(|a, b| a.total_cmp(b));
            let median = band[band.len() / 2];
            let top = band[band.len() - 20..].iter().sum::<f32>() / 20.0;
            let db = 20.0 * (top / median).log10();
            assert!(db < 16.0, "size {size}: peaks {db:.1} dB over the median");
        }
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
