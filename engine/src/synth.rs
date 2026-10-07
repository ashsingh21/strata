//! Carve's DSP: a polyphonic subtractive-synth voice engine.
//!
//! Per voice: up to `MAX_UNISON` detuned copies of both oscillators, each
//! band-limited (PolyBLEP on edges, PolyBLAMP on corners, a BLEP on every
//! hard-sync reset) and panned across the stereo field; a sine sub and
//! white noise; a DC blocker, a 2x-oversampled anti-aliased (ADAA) `tanh`
//! drive, then per channel a 4-pole ladder for LP 24 (zero-delay feedback,
//! soft-clipped resonance) or an Andrew Simper "TPT" state-variable filter
//! for LP 12 / BP / HP; exponential ADSRs for amp and filter. Every
//! continuous parameter is smoothed per sample so knob moves don't click.
//! The voice sum runs through a chorus, a reverb and a lookahead limiter.
//!
//! Everything here runs on the audio thread: no allocation once
//! `SynthEngine` is built (it's built before the stream starts), no locks,
//! no syscalls.

use shared::synth::{
    FilterType, LfoTarget, NoteEvent, SynthParams, VoiceMode, Waveform, ALL_NOTES_OFF, LFO_CUTOFF_MAX_OCT, LFO_PITCH_MAX_CENTS,
    LFO_PULSE_WIDTH_MAX, LFO_RESONANCE_MAX, MAX_UNISON,
};

use wide::f32x4;

use crate::dsp::{floor_fast, fract_fast, poly_blamp, tan_fast, DcBlocker, DriveBank, Smoother};
use crate::fx::{Chorus, Limiter, Reverb};

const MAX_VOICES: usize = 16;
const UNISON: usize = MAX_UNISON as usize;
/// Reference note for octave/key-tracking math (C4).
const REF_NOTE: f32 = 60.0;
/// Most a drifting oscillator wanders, in cents.
const DRIFT_MAX_CENTS: f32 = 15.0;
/// Parameter smoothing time: long enough to kill zipper noise, short
/// enough that knobs still feel immediate.
const SMOOTHING_MS: f32 = 12.0;
/// Fixed gain per voice (not divided by the number playing, which made
/// every note quieter as more joined in); the limiter catches the sum.
const VOICE_GAIN: f32 = 0.35;

fn midi_to_hz(note: f32) -> f32 {
    440.0 * 2f32.powf((note - 69.0) / 12.0)
}

/// Velocity to level: full at 127, and a square-law curve below it (so 64
/// is about -12 dB) - soft notes get audibly softer without vanishing.
fn velocity_to_gain(velocity: u8) -> f32 {
    let v = velocity.clamp(1, 127) as f32 / 127.0;
    v * v
}

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[derive(Clone, Copy, PartialEq)]
enum EnvStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

struct Envelope {
    stage: EnvStage,
    level: f32,
}

impl Envelope {
    fn new() -> Self {
        Self { stage: EnvStage::Idle, level: 0.0 }
    }

    fn note_on(&mut self) {
        self.stage = EnvStage::Attack;
    }

    fn note_off(&mut self) {
        if self.stage != EnvStage::Idle {
            self.stage = EnvStage::Release;
        }
    }

    /// Advances one sample and returns the current level (0..1).
    fn tick(&mut self, params: &shared::synth::Envelope, c: &EnvCoeffs) -> f32 {
        match self.stage {
            EnvStage::Idle => self.level = 0.0,
            EnvStage::Attack => {
                self.level += (1.0 - self.level) * c.attack;
                if self.level >= 0.999 {
                    self.level = 1.0;
                    self.stage = EnvStage::Decay;
                }
            }
            EnvStage::Decay => {
                self.level += (params.sustain - self.level) * c.decay;
                if (self.level - params.sustain).abs() < 0.001 {
                    self.level = params.sustain;
                    self.stage = EnvStage::Sustain;
                }
            }
            EnvStage::Sustain => self.level = params.sustain,
            EnvStage::Release => {
                self.level += (0.0 - self.level) * c.release;
                if self.level < 0.0005 {
                    self.level = 0.0;
                    self.stage = EnvStage::Idle;
                }
            }
        }
        self.level
    }
}

/// An envelope's per-sample approach coefficients - the same for every
/// voice, so computed once per sample rather than once per voice.
#[derive(Clone, Copy)]
struct EnvCoeffs {
    attack: f32,
    decay: f32,
    release: f32,
}

impl EnvCoeffs {
    fn new(params: &shared::synth::Envelope, sample_rate: f32) -> Self {
        let coeff = |ms: f32| 1.0 - (-1.0 / (ms.max(0.5) * 0.001 * sample_rate)).exp();
        Self { attack: coeff(params.attack_ms), decay: coeff(params.decay_ms), release: coeff(params.release_ms) }
    }
}

/// One 2-pole (12 dB/oct) state-variable filter stage producing
/// simultaneous low/band/high-pass outputs; `Lp24` cascades two of these.
#[derive(Clone, Copy, Default)]
struct SvfStage {
    ic1eq: f32,
    ic2eq: f32,
}

/// Coefficients for one cutoff and resonance: the state-variable stage's
/// (LP 12, BP, HP) and the ladder's (LP 24).
#[derive(Clone, Copy, Default)]
struct SvfCoeffs {
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    /// The ladder's one-pole gain, G = g / (1 + g).
    lg: f32,
    /// How each stage's state feeds the output, for the zero-delay loop.
    beta: [f32; 4],
    /// Resonance feedback (4 is where it starts to ring on its own).
    lk: f32,
    /// 1 / (1 + k G^4): solves the loop.
    norm: f32,
    /// Gives back some of the low end resonance takes away.
    comp: f32,
}

impl SvfCoeffs {
    fn new(cutoff_hz: f32, resonance: f32, sample_rate: f32) -> Self {
        let g = tan_fast(std::f32::consts::PI * cutoff_hz / sample_rate);
        let q = 0.5 + resonance * resonance * 19.5;
        let k = 1.0 / q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let lg = g / (1.0 + g);
        let inv = 1.0 / (1.0 + g);
        let lk = LADDER_MAX_K * resonance.clamp(0.0, 1.0).powf(0.7);
        let g4 = lg * lg * lg * lg;
        Self {
            k,
            a1,
            a2,
            a3: g * a2,
            lg,
            beta: [lg * lg * lg * inv, lg * lg * inv, lg * inv, inv],
            lk,
            norm: 1.0 / (1.0 + lk * g4),
            comp: 1.0 + 0.5 * lk,
        }
    }
}

/// The ladder's feedback at full resonance: right at the edge of ringing
/// on its own, held there by the soft clip in the loop.
const LADDER_MAX_K: f32 = 4.0;

/// A cheap, smooth tanh-like curve for the ladder's loop: linear for small
/// signals, easing into +-1.
#[inline]
fn soft_clip(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

/// A 4-pole transistor-ladder low-pass (Zavalishin's zero-delay-feedback
/// form): four one-pole stages in a loop with the resonance feedback,
/// soft-clipped where the feedback meets the input. Resonance rings and
/// growls instead of whistling, and stays bounded however far it's turned.
#[derive(Clone, Copy, Default)]
struct Ladder([f32; 4]);

impl Ladder {
    #[inline]
    fn process(&mut self, x: f32, c: &SvfCoeffs) -> f32 {
        let s = &mut self.0;
        let sum = c.beta[0] * s[0] + c.beta[1] * s[1] + c.beta[2] * s[2] + c.beta[3] * s[3];
        let mut y = soft_clip((x - c.lk * sum) * c.norm);
        for st in s.iter_mut() {
            let v = (y - *st) * c.lg;
            let out = v + *st;
            *st = out + v;
            y = out;
        }
        y * c.comp
    }
}

impl SvfStage {
    /// Returns (lowpass, bandpass, highpass) for one input sample.
    #[inline]
    fn process(&mut self, input: f32, c: &SvfCoeffs) -> (f32, f32, f32) {
        let v3 = input - self.ic2eq;
        let v1 = c.a1 * self.ic1eq + c.a2 * v3;
        let v2 = self.ic2eq + c.a2 * self.ic1eq + c.a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;
        (v2, v1, input - c.k * v1 - v2)
    }
}

/// One channel's filter: the ladder for LP 24, a state-variable stage for
/// the rest.
#[derive(Clone, Copy, Default)]
struct Filter {
    svf: SvfStage,
    ladder: Ladder,
}

impl Filter {
    #[inline]
    fn process(&mut self, x: f32, filter_type: FilterType, c: &SvfCoeffs) -> f32 {
        if filter_type == FilterType::Lp24 {
            return self.ladder.process(x, c);
        }
        let (lp, bp, hp) = self.svf.process(x, c);
        match filter_type {
            FilterType::Lp12 | FilterType::Lp24 => lp,
            FilterType::Bp => bp,
            FilterType::Hp => hp,
        }
    }
}

/// Wraps a phase into 0..1. Same result as `rem_euclid(1.0)`, without the
/// `fmod` - this runs several times per oscillator per sample.
#[inline]
fn wrap01(x: f32) -> f32 {
    x - floor_fast(x)
}

fn xorshift32(state: &mut u32) -> f32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// The naive (unfiltered) value of each waveform at `t` in 0..1 - used for
/// the size of a hard-sync jump.
fn naive(waveform: Waveform, t: f32, shape: f32, dt: f32) -> f32 {
    match waveform {
        Waveform::Sine => sine(t, shape),
        Waveform::Square => {
            let (duty, rise) = pulse_shape(shape, dt);
            trapezoid(t, duty, rise)
        }
        Waveform::Triangle | Waveform::Saw => ramp(t, ramp_duty(waveform, shape, dt)),
    }
}

/// A pulse's duty and edge time (both in phase units): edges last two
/// samples, and stay clear of each other at extreme widths and pitches.
fn pulse_shape(shape: f32, dt: f32) -> (f32, f32) {
    let rise = (2.0 * dt).min(0.2);
    let duty = pulse_duty(shape).clamp(rise * 2.0, 1.0 - rise * 2.0);
    (duty, rise)
}

/// -1 -> 1 over `[0, rise)`, high until `duty`, 1 -> -1 over
/// `[duty, duty + rise)`, then low.
fn trapezoid(t: f32, duty: f32, rise: f32) -> f32 {
    if t < rise {
        -1.0 + 2.0 * t / rise
    } else if t < duty {
        1.0
    } else if t < duty + rise {
        1.0 - 2.0 * (t - duty) / rise
    } else {
        -1.0
    }
}

/// Sine, gaining a folded 2nd harmonic as `shape` rises.
fn sine(t: f32, shape: f32) -> f32 {
    let fold = shape.clamp(0.0, 1.0) * 0.6;
    let x = t * std::f32::consts::TAU;
    (x.sin() + fold * (2.0 * x).sin()) / (1.0 + fold)
}

/// The square's Shape knob narrows it: 0 is a true square (high half the
/// cycle), 1 the thinnest pulse (5%). A pulse and its mirror (30% / 70%)
/// sound the same, so the knob only runs one way from the square.
fn pulse_duty(shape: f32) -> f32 {
    0.5 - shape.clamp(0.0, 1.0) * 0.45
}

/// Triangle and saw are one shape: a ramp rising from -1 to 1 over
/// `[0, duty)` and falling back over `[duty, 1)`. Triangle skews toward a
/// saw as `shape` rises; saw rounds toward a triangle. The duty stays at
/// least two samples from either end, so even a "pure" saw has a short,
/// band-limitable fall instead of an infinitely steep one.
fn ramp_duty(waveform: Waveform, shape: f32, dt: f32) -> f32 {
    let shape = shape.clamp(0.0, 1.0);
    let duty = match waveform {
        Waveform::Triangle => 0.5 + shape * 0.48,
        _ => 1.0 - shape * 0.5,
    };
    let margin = (2.0 * dt).min(0.5);
    duty.clamp(margin, 1.0 - margin)
}

fn ramp(t: f32, duty: f32) -> f32 {
    if t < duty {
        -1.0 + 2.0 * (t / duty)
    } else {
        1.0 - 2.0 * ((t - duty) / (1.0 - duty))
    }
}

/// Band-limited oscillator output at phase `t` (0..1) with per-sample
/// phase increment `dt`. `after_reset` means a hard-sync reset (not the
/// waveform's own wrap) put the phase back to 0 this sample: the jump it
/// caused was already corrected by the caller, so the waveform's own
/// start-of-cycle correction must not be applied on top.
fn osc_sample(waveform: Waveform, t: f32, shape: f32, dt: f32, after_reset: bool) -> f32 {
    let start = |r: f32| if after_reset && t < dt { 0.0 } else { r };
    match waveform {
        Waveform::Sine => sine(t, shape),
        Waveform::Square => {
            // A trapezoid whose edges take two samples, with each of its
            // four corners BLAMP-smoothed - measurably cleaner than
            // PolyBLEP on an instantaneous edge.
            let (duty, rise) = pulse_shape(shape, dt);
            let slope = 2.0 / rise * 0.5 * dt;
            let corner = |at: f32| poly_blamp(wrap01(t - at), dt);
            trapezoid(t, duty, rise) + start(slope * corner(0.0)) - slope * corner(rise) - slope * corner(duty)
                + slope * corner(duty + rise)
        }
        Waveform::Triangle | Waveform::Saw => {
            let duty = ramp_duty(waveform, shape, dt);
            let rise = 2.0 / duty;
            let fall = -2.0 / (1.0 - duty);
            // Slope changes (per unit phase) at the two corners, each
            // smoothed by a BLAMP scaled to the change per sample.
            let at_start = (rise - fall) * 0.5 * dt;
            let at_duty = (fall - rise) * 0.5 * dt;
            ramp(t, duty) + start(at_start * poly_blamp(t, dt)) + at_duty * poly_blamp(wrap01(t - duty), dt)
        }
    }
}

// --- The oscillators again, four unison copies at a time. Every operation
// mirrors the scalar function above it, in the same order, so a lane's
// result is bit-for-bit the scalar's; `lane_oscillators_match_the_scalar_ones`
// holds them to that. (No `after_reset`: hard sync keeps the scalar path.)

#[inline(always)]
fn splat(x: f32) -> f32x4 {
    f32x4::splat(x)
}

#[inline(always)]
fn floor_lanes(x: f32x4) -> f32x4 {
    // SSE2 has no `roundps`, so `floor` is emulated slowly; phases are
    // tiny, so truncate through an integer and step down where that rounded up.
    let t = f32x4::from_i32x4(x.trunc_int());
    t - t.simd_gt(x).select(splat(1.0), f32x4::ZERO)
}

#[inline(always)]
fn wrap01_lanes(x: f32x4) -> f32x4 {
    x - floor_lanes(x)
}

#[inline(always)]
fn poly_blamp_lanes(t: f32x4, dt: f32x4) -> f32x4 {
    let x = t / dt - splat(1.0);
    let rising = (-x * x * x) / splat(3.0);
    let y = (t - splat(1.0)) / dt + splat(1.0);
    let falling = y * y * y / splat(3.0);
    t.simd_lt(dt).select(rising, t.simd_gt(splat(1.0) - dt).select(falling, f32x4::ZERO))
}

fn osc_lanes(waveform: Waveform, t: f32x4, shape: f32, dt: f32x4) -> f32x4 {
    match waveform {
        Waveform::Sine => {
            let (t, dt) = (t.to_array(), dt.to_array());
            f32x4::from(std::array::from_fn(|i| osc_sample(waveform, t[i], shape, dt[i], false)))
        }
        Waveform::Square => {
            let rise = (splat(2.0) * dt).min(splat(0.2));
            let duty = splat(pulse_duty(shape)).max(rise * splat(2.0)).min(splat(1.0) - rise * splat(2.0));
            let slope = splat(2.0) / rise * splat(0.5) * dt;
            let corner = |at: f32x4| poly_blamp_lanes(wrap01_lanes(t - at), dt);
            let mut trap = splat(-1.0);
            trap = t.simd_lt(duty + rise).select(splat(1.0) - splat(2.0) * (t - duty) / rise, trap);
            trap = t.simd_lt(duty).select(splat(1.0), trap);
            trap = t.simd_lt(rise).select(splat(-1.0) + splat(2.0) * t / rise, trap);
            trap + slope * corner(f32x4::ZERO) - slope * corner(rise) - slope * corner(duty) + slope * corner(duty + rise)
        }
        Waveform::Triangle | Waveform::Saw => {
            let shape = shape.clamp(0.0, 1.0);
            let duty = splat(match waveform {
                Waveform::Triangle => 0.5 + shape * 0.48,
                _ => 1.0 - shape * 0.5,
            });
            let margin = (splat(2.0) * dt).min(splat(0.5));
            let duty = duty.max(margin).min(splat(1.0) - margin);
            let rise = splat(2.0) / duty;
            let fall = splat(-2.0) / (splat(1.0) - duty);
            let at_start = (rise - fall) * splat(0.5) * dt;
            let at_duty = (fall - rise) * splat(0.5) * dt;
            let up = splat(-1.0) + splat(2.0) * (t / duty);
            let down = splat(1.0) - splat(2.0) * ((t - duty) / (splat(1.0) - duty));
            let ramp = t.simd_lt(duty).select(up, down);
            ramp + at_start * poly_blamp_lanes(t, dt) + at_duty * poly_blamp_lanes(wrap01_lanes(t - duty), dt)
        }
    }
}

/// One unison copy's oscillator pair and its own analog-style drift.
#[derive(Clone, Copy)]
struct UnisonOsc {
    phase1: f32,
    phase2: f32,
    drift: f32,
    drift_rng: u32,
    /// Hard-sync BLEP correction owed to the next sample.
    sync_residual: f32,
    /// Whether osc 2's phase was reset by sync going into this sample.
    synced: bool,
}

impl UnisonOsc {
    fn new(seed: u32) -> Self {
        // Spread the starting phases so unison copies don't start in
        // phase (which would sound like one loud oscillator, then flange).
        let mut rng = seed | 1;
        let phase1 = (xorshift32(&mut rng) * 0.5 + 0.5).fract();
        let phase2 = (xorshift32(&mut rng) * 0.5 + 0.5).fract();
        Self { phase1, phase2, drift: 0.0, drift_rng: rng, sync_residual: 0.0, synced: false }
    }
}

struct Voice {
    active: bool,
    note: u8,
    current_note: f32,
    target_note: f32,
    /// The note's velocity as a level multiplier.
    velocity_gain: f32,
    /// `midi_to_hz(current_note)`, recomputed only while gliding.
    current_hz: f32,
    unison: [UnisonOsc; UNISON],
    sub_phase: f32,
    noise_state: u32,
    amp_env: Envelope,
    filter_env: Envelope,
    dc: [DcBlocker; 2],
    filter: [Filter; 2],
    /// The filter's coefficients for the last (cutoff exponent, Q): they
    /// only move while the filter envelope, an LFO or the knobs do.
    coeff_memo: Memo<(f32, f32), SvfCoeffs>,
}

impl Voice {
    fn new(seed: u32, sample_rate: f32) -> Self {
        Self {
            active: false,
            note: 0,
            current_note: REF_NOTE,
            target_note: REF_NOTE,
            current_hz: midi_to_hz(REF_NOTE),
            velocity_gain: 1.0,
            unison: std::array::from_fn(|i| {
                UnisonOsc::new(seed.wrapping_mul(2_654_435_761).wrapping_add(i as u32 * 97))
            }),
            sub_phase: 0.0,
            noise_state: seed | 1,
            amp_env: Envelope::new(),
            filter_env: Envelope::new(),
            dc: [DcBlocker::new(10.0, sample_rate); 2],
            filter: [Filter::default(); 2],
            coeff_memo: Memo::EMPTY,
        }
    }
}

/// Every continuous parameter, smoothed per sample. Shared by all voices
/// (they all read the same knobs).
struct Smoothed {
    osc1_tune: Smoother,
    osc1_shape: Smoother,
    osc1_drift: Smoother,
    osc2_detune: Smoother,
    osc2_shape: Smoother,
    osc2_fm: Smoother,
    osc1_gain: Smoother,
    osc2_gain: Smoother,
    sub_gain: Smoother,
    noise_gain: Smoother,
    cutoff_log2: Smoother,
    resonance: Smoother,
    drive_gain: Smoother,
    env_amount: Smoother,
    key_track: Smoother,
    volume_gain: Smoother,
    unison_detune: Smoother,
    unison_width: Smoother,
}

impl Smoothed {
    fn new(p: &SynthParams, sr: f32) -> Self {
        let s = |v: f32| Smoother::new(v, SMOOTHING_MS, sr);
        Self {
            osc1_tune: s(p.osc1.knob_a_cents),
            osc1_shape: s(p.osc1.knob_b),
            osc1_drift: s(p.osc1.knob_c),
            osc2_detune: s(p.osc2.knob_a_cents),
            osc2_shape: s(p.osc2.knob_b),
            osc2_fm: s(p.osc2.knob_c),
            osc1_gain: s(db_to_gain(p.mix.osc1_db)),
            osc2_gain: s(db_to_gain(p.mix.osc2_db)),
            sub_gain: s(db_to_gain(p.mix.sub_db)),
            noise_gain: s(db_to_gain(p.mix.noise_db)),
            cutoff_log2: s(p.filter.cutoff_hz.max(1.0).log2()),
            resonance: s(p.filter.resonance),
            drive_gain: s(db_to_gain(p.filter.drive_db)),
            env_amount: s(p.filter.env_amount_oct),
            key_track: s(p.filter.key_track),
            volume_gain: s(db_to_gain(p.volume_db)),
            unison_detune: s(p.unison.detune_cents),
            unison_width: s(p.unison.width),
        }
    }
}

/// The last result of a pure function of its inputs. Most of what a sample
/// derives from the patch (dB to gain, envelope coefficients, pitch
/// ratios) only changes when a knob does, so it's computed then, not
/// 48 000 times a second.
#[derive(Clone, Copy)]
struct Memo<K: Copy + PartialEq, V: Copy>(Option<(K, V)>);

impl<K: Copy + PartialEq, V: Copy> Memo<K, V> {
    const EMPTY: Self = Self(None);

    #[inline(always)]
    fn get(&mut self, key: K, compute: impl FnOnce(K) -> V) -> V {
        match self.0 {
            Some((k, v)) if k == key => v,
            _ => {
                let v = compute(key);
                self.0 = Some((key, v));
                v
            }
        }
    }
}

/// Unison copies' detune ratios, pans and level, for one (count, detune,
/// width).
#[derive(Clone, Copy)]
struct UnisonLayout {
    ratio: [f32; UNISON],
    pan: [(f32, f32); UNISON],
    norm: f32,
}

impl UnisonLayout {
    fn new((copies, detune, width): (usize, f32, f32)) -> Self {
        let mut ratio = [1.0f32; UNISON];
        let mut pan = [(1.0f32, 1.0f32); UNISON];
        for i in 0..copies {
            let spread = if copies == 1 { 0.0 } else { -1.0 + 2.0 * i as f32 / (copies - 1) as f32 };
            ratio[i] = 2f32.powf(spread * detune * 0.5 / 1200.0);
            let angle = (spread * width + 1.0) * std::f32::consts::FRAC_PI_4;
            pan[i] = (angle.cos() * std::f32::consts::SQRT_2, angle.sin() * std::f32::consts::SQRT_2);
        }
        Self { ratio, pan, norm: 1.0 / (copies as f32).sqrt() }
    }
}

#[derive(Clone, Copy)]
struct Derived {
    g_osc1: Memo<f32, f32>,
    g_osc2: Memo<f32, f32>,
    g_sub: Memo<f32, f32>,
    g_noise: Memo<f32, f32>,
    drive: Memo<f32, f32>,
    volume: Memo<f32, f32>,
    cutoff_log2: Memo<f32, f32>,
    glide: Memo<f32, f32>,
    amp_env: Memo<[f32; 3], EnvCoeffs>,
    filter_env: Memo<[f32; 3], EnvCoeffs>,
    vibrato: Memo<f32, f32>,
    osc1_ratio: Memo<(i8, f32), f32>,
    osc2_ratio: Memo<(i8, f32), f32>,
    sub_oct: Memo<i8, f32>,
    unison: Memo<(usize, f32, f32), UnisonLayout>,
}

impl Derived {
    const EMPTY: Self = Self {
        g_osc1: Memo::EMPTY,
        g_osc2: Memo::EMPTY,
        g_sub: Memo::EMPTY,
        g_noise: Memo::EMPTY,
        drive: Memo::EMPTY,
        volume: Memo::EMPTY,
        cutoff_log2: Memo::EMPTY,
        glide: Memo::EMPTY,
        amp_env: Memo::EMPTY,
        filter_env: Memo::EMPTY,
        vibrato: Memo::EMPTY,
        osc1_ratio: Memo::EMPTY,
        osc2_ratio: Memo::EMPTY,
        sub_oct: Memo::EMPTY,
        unison: Memo::EMPTY,
    };
}

pub struct SynthEngine {
    sample_rate: f32,
    voices: [Voice; MAX_VOICES],
    /// The drive stage of every voice, two voices (four channels) to a bank.
    drive: [DriveBank; MAX_VOICES / 2],
    next_voice: usize,
    mono_stack: Vec<u8>,
    lfo1_phase: f32,
    lfo2_phase: f32,
    /// Where the song is, in beats, while it plays (synced LFOs lock to
    /// it); `None` while stopped, when they run on at the tempo.
    beat: Option<f64>,
    beats_per_sample: f64,
    params: SynthParams,
    smoothed: Smoothed,
    derived: Derived,
    drift_coeff: f32,
    chorus: Chorus,
    reverb: Reverb,
    limiter: Limiter,
    /// Samples since the last voice finished; once past `IDLE_AFTER_S`
    /// (every effect tail long gone) the instance skips all its work.
    silent_samples: u32,
}

/// Longer than any reverb/chorus tail, so skipping never cuts one off.
const IDLE_AFTER_S: f32 = 10.0;

impl SynthEngine {
    /// Allocates everything the engine will ever need (voices, effect
    /// buffers sized for `sample_rate`) - call before the stream starts.
    pub fn new(sample_rate: f32) -> Self {
        let params = SynthParams::default();
        Self {
            sample_rate,
            voices: std::array::from_fn(|i| Voice::new(0x9e37_79b9u32.wrapping_mul(i as u32 + 1), sample_rate)),
            drive: [DriveBank::default(); MAX_VOICES / 2],
            next_voice: 0,
            mono_stack: Vec::with_capacity(MAX_VOICES),
            lfo1_phase: 0.0,
            lfo2_phase: 0.0,
            beat: None,
            beats_per_sample: 2.0 / sample_rate as f64,
            smoothed: Smoothed::new(&params, sample_rate),
            derived: Derived::EMPTY,
            drift_coeff: 1.0 - (-1.0 / (2.0 * sample_rate)).exp(),
            params,
            chorus: Chorus::new(sample_rate),
            reverb: Reverb::new(sample_rate),
            limiter: Limiter::new(sample_rate),
            silent_samples: u32::MAX / 2,
        }
    }

    pub fn set_params(&mut self, params: SynthParams) {
        self.params = params;
    }

    /// The song's clock, once a block: where it is in beats (`None` while
    /// stopped) and how far one sample moves it.
    pub fn set_clock(&mut self, beat: Option<f64>, beats_per_sample: f64) {
        self.beat = beat;
        self.beats_per_sample = beats_per_sample;
    }

    /// Each LFO's position in its cycle, 0..1.
    pub fn lfo_phases(&self) -> (f32, f32) {
        (self.lfo1_phase, self.lfo2_phase)
    }

    pub fn handle_note_event(&mut self, event: NoteEvent) {
        if event.note == ALL_NOTES_OFF {
            self.mono_stack.clear();
            for voice in &mut self.voices {
                voice.amp_env.note_off();
                voice.filter_env.note_off();
            }
            return;
        }
        if event.on {
            self.note_on(event.note, event.velocity);
        } else {
            self.note_off(event.note);
        }
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        // A phrase starting (no key held): free LFOs start their cycle
        // again, so every first note gets the same sweep instead of
        // wherever the wave happened to be. Chords and legato lines keep
        // one sweep going; synced ones follow the beat instead.
        let held = self.voices.iter().any(|v| v.active && v.amp_env.stage != EnvStage::Release);
        if !held {
            if self.params.lfo1_per_beat == 0.0 {
                self.lfo1_phase = 0.0;
            }
            if self.params.lfo2_per_beat == 0.0 {
                self.lfo2_phase = 0.0;
            }
        }
        let velocity_gain = velocity_to_gain(velocity);
        if self.params.voice_mode == VoiceMode::Mono {
            self.mono_stack.retain(|n| *n != note);
            // Legato (glide, no retrigger) only while another key is still
            // held. A released note keeps the voice active through its
            // release tail; treating that as legato let a note played in
            // the tail inherit the dying envelope and come out ~30 dB down.
            let legato = !self.mono_stack.is_empty() && self.voices[0].active;
            self.mono_stack.push(note);
            self.voices[0].note = note;
            self.voices[0].velocity_gain = velocity_gain;
            self.voices[0].target_note = note as f32;
            if !legato {
                self.voices[0].current_note = note as f32;
                self.voices[0].current_hz = midi_to_hz(note as f32);
                self.voices[0].active = true;
                self.clear_drive(0);
                self.voices[0].amp_env.note_on();
                self.voices[0].filter_env.note_on();
            }
            return;
        }

        let max_voices = (self.params.max_voices as usize).clamp(1, MAX_VOICES);
        let slot = (0..max_voices).find(|i| !self.voices[*i].active).unwrap_or_else(|| {
            let slot = self.next_voice % max_voices;
            self.next_voice = self.next_voice.wrapping_add(1);
            slot
        });

        self.clear_drive(slot);
        let voice = &mut self.voices[slot];
        voice.active = true;
        voice.note = note;
        voice.velocity_gain = velocity_gain;
        voice.current_note = note as f32;
        voice.current_hz = midi_to_hz(note as f32);
        voice.target_note = note as f32;
        voice.amp_env.note_on();
        voice.filter_env.note_on();
    }

    /// A voice starting fresh must not inherit the drive stage's memory of
    /// the note that last played in its slot.
    fn clear_drive(&mut self, voice: usize) {
        for ch in 0..2 {
            self.drive[voice / 2].clear_lane(voice % 2 * 2 + ch);
        }
    }

    fn note_off(&mut self, note: u8) {
        if self.params.voice_mode == VoiceMode::Mono {
            self.mono_stack.retain(|n| *n != note);
            match self.mono_stack.last() {
                Some(fallback) => self.voices[0].target_note = *fallback as f32,
                None => {
                    self.voices[0].amp_env.note_off();
                    self.voices[0].filter_env.note_off();
                }
            }
            return;
        }

        for voice in &mut self.voices {
            if voice.active && voice.note == note {
                voice.amp_env.note_off();
                voice.filter_env.note_off();
            }
        }
    }

    /// Renders one stereo sample.
    pub fn process(&mut self) -> (f32, f32) {
        // An instance nobody has played for a while costs next to nothing:
        // with 16 of them allocated, most are idle most of the time.
        if self.voices.iter().any(|v| v.active) {
            self.silent_samples = 0;
        } else {
            self.silent_samples = self.silent_samples.saturating_add(1);
            if self.silent_samples as f32 > IDLE_AFTER_S * self.sample_rate {
                return (0.0, 0.0);
            }
        }
        let sr = self.sample_rate;
        let p = self.params;
        let sm = &mut self.smoothed;
        let d = &mut self.derived;

        // --- Smoothed parameters (advanced once per sample, shared). ---
        let osc1_tune = sm.osc1_tune.next(p.osc1.knob_a_cents);
        let osc1_shape = sm.osc1_shape.next(p.osc1.knob_b);
        let osc1_drift = sm.osc1_drift.next(p.osc1.knob_c);
        let osc2_detune = sm.osc2_detune.next(p.osc2.knob_a_cents);
        let mut osc2_shape = sm.osc2_shape.next(p.osc2.knob_b);
        let osc2_fm = sm.osc2_fm.next(p.osc2.knob_c);
        let g_osc1 = sm.osc1_gain.next(d.g_osc1.get(p.mix.osc1_db, db_to_gain));
        let g_osc2 = sm.osc2_gain.next(d.g_osc2.get(p.mix.osc2_db, db_to_gain));
        let g_sub = sm.sub_gain.next(d.g_sub.get(p.mix.sub_db, db_to_gain));
        let g_noise = sm.noise_gain.next(d.g_noise.get(p.mix.noise_db, db_to_gain));
        let cutoff_log2 = sm.cutoff_log2.next(d.cutoff_log2.get(p.filter.cutoff_hz, |hz| hz.max(1.0).log2()));
        let mut resonance = sm.resonance.next(p.filter.resonance);
        let drive_gain = sm.drive_gain.next(d.drive.get(p.filter.drive_db, db_to_gain));
        let env_amount = sm.env_amount.next(p.filter.env_amount_oct);
        let key_track = sm.key_track.next(p.filter.key_track);
        let volume = sm.volume_gain.next(d.volume.get(p.volume_db, db_to_gain));
        let uni_detune = sm.unison_detune.next(p.unison.detune_cents);
        let uni_width = sm.unison_width.next(p.unison.width);

        // --- LFOs: each adds into whichever target it's patched to. ---
        // A synced one follows the song's beat (or, stopped, runs on at
        // the tempo) and starts each cycle at its lowest, so a wobble on
        // Cutoff opens up from closed on every beat it's locked to.
        let (beat, per_sample) = (self.beat, self.beats_per_sample);
        let advance = |phase: f32, hz: f32, per_beat: f32| -> f32 {
            match (per_beat > 0.0, beat) {
                (false, _) => fract_fast(phase + hz / sr),
                (true, Some(b)) => (b * per_beat as f64).rem_euclid(1.0) as f32,
                (true, None) => fract_fast(phase + (per_beat as f64 * per_sample) as f32),
            }
        };
        self.lfo1_phase = advance(self.lfo1_phase, p.lfo1_rate_hz, p.lfo1_per_beat);
        self.lfo2_phase = advance(self.lfo2_phase, p.lfo2_rate_hz, p.lfo2_per_beat);
        if let Some(b) = &mut self.beat {
            *b += per_sample;
        }
        let wave = |phase: f32, synced: bool| {
            let t = phase * std::f32::consts::TAU;
            if synced { -t.cos() } else { t.sin() }
        };
        let lfo1 = if p.lfo1_depth == 0.0 { 0.0 } else { wave(self.lfo1_phase, p.lfo1_per_beat > 0.0) };
        let lfo2 = if p.lfo2_depth == 0.0 { 0.0 } else { wave(self.lfo2_phase, p.lfo2_per_beat > 0.0) };
        let mut cutoff_lfo_oct = 0.0f32;
        let mut pitch_lfo_cents = 0.0f32;
        for (lfo, depth, target) in [(lfo1, p.lfo1_depth, p.lfo1_target), (lfo2, p.lfo2_depth, p.lfo2_target)] {
            let amount = lfo * depth;
            match target {
                LfoTarget::Cutoff => cutoff_lfo_oct += amount * LFO_CUTOFF_MAX_OCT,
                LfoTarget::Pitch => pitch_lfo_cents += amount * LFO_PITCH_MAX_CENTS,
                LfoTarget::PulseWidth => osc2_shape += amount * LFO_PULSE_WIDTH_MAX,
                LfoTarget::Resonance => resonance += amount * LFO_RESONANCE_MAX,
            }
        }
        let osc2_shape = osc2_shape.clamp(0.0, 1.0);
        let resonance = resonance.clamp(0.0, 1.0);
        let vibrato_ratio = d.vibrato.get(pitch_lfo_cents, |c| 2f32.powf(c / 1200.0));

        let glide_coeff = d.glide.get(p.glide_ms, |ms| 1.0 - (-1.0 / (ms.max(0.5) * 0.001 * sr)).exp());
        let amp_coeffs = d.amp_env.get([p.amp_env.attack_ms, p.amp_env.decay_ms, p.amp_env.release_ms], |_| EnvCoeffs::new(&p.amp_env, sr));
        let filter_coeffs =
            d.filter_env.get([p.filter_env.attack_ms, p.filter_env.decay_ms, p.filter_env.release_ms], |_| EnvCoeffs::new(&p.filter_env, sr));
        // ~2 second time constant: slow enough to feel like wander, not vibrato.
        let drift_coeff = self.drift_coeff;

        // Unison layout: detune ratios and equal-power pan gains.
        let copies = (p.unison.voices as usize).clamp(1, UNISON);
        let UnisonLayout { ratio: uni_ratio, pan: uni_pan, norm: uni_norm } =
            d.unison.get((copies, uni_detune, uni_width), UnisonLayout::new);
        let inv_sr = 1.0 / sr;

        // Pitch factors shared by every voice.
        let osc1_ratio = d.osc1_ratio.get((p.osc1.octave, osc1_tune), |(o, t)| 2f32.powf(o as f32 + t / 1200.0));
        let osc2_ratio = d.osc2_ratio.get((p.osc2.octave, osc2_detune), |(o, t)| 2f32.powf(o as f32 + t / 1200.0));
        let drift_depth = osc1_drift * DRIFT_MAX_CENTS / 1200.0 * std::f32::consts::LN_2;
        let sub_oct = d.sub_oct.get(p.osc1.octave, |o| 2f32.powf((o - 1) as f32));

        let mut sum_l = 0.0f32;
        let mut sum_r = 0.0f32;

        // Pass 1, per voice: envelopes, oscillators and DC blocker, up to
        // the drive stage's input.
        let mut live = [false; MAX_VOICES];
        let mut pre = [[0.0f32; 2]; MAX_VOICES];
        let mut coeffs = [SvfCoeffs::default(); MAX_VOICES];
        let mut level = [0.0f32; MAX_VOICES];

        for (vi, voice) in self.voices.iter_mut().enumerate() {
            if !voice.active {
                continue;
            }

            let gap = voice.target_note - voice.current_note;
            if gap != 0.0 {
                voice.current_note = if gap.abs() < 1.0e-4 { voice.target_note } else { voice.current_note + gap * glide_coeff };
                voice.current_hz = midi_to_hz(voice.current_note);
            }

            let amp_level = voice.amp_env.tick(&p.amp_env, &amp_coeffs);
            let filter_level = voice.filter_env.tick(&p.filter_env, &filter_coeffs);
            if voice.amp_env.stage == EnvStage::Idle {
                voice.active = false;
                continue;
            }

            let base_hz = voice.current_hz * vibrato_ratio;
            let mut dry_l = 0.0f32;
            let mut dry_r = 0.0f32;

            let lanes_ok = copies >= 2
                && !p.osc2.sync
                && voice.unison.iter().take(copies).all(|u| !u.synced && u.sync_residual == 0.0);
            if lanes_ok {
                let mut phase1 = [0.0f32; UNISON];
                let mut phase2 = [0.0f32; UNISON];
                let mut dt1 = [0.01f32; UNISON];
                let mut dt2_base = [0.01f32; UNISON];
                let mut drift = [0.0f32; UNISON];
                for (i, u) in voice.unison.iter_mut().take(copies).enumerate() {
                    u.drift += (xorshift32(&mut u.drift_rng) - u.drift) * drift_coeff;
                    drift[i] = u.drift;
                    phase1[i] = u.phase1;
                    phase2[i] = u.phase2;
                }
                let d = f32x4::from(drift) * splat(drift_depth);
                let drift_ratio = splat(1.0) + d + splat(0.5) * d * d;
                for i in 0..copies {
                    let base = base_hz * uni_ratio[i] * inv_sr;
                    dt1[i] = base * osc1_ratio;
                    dt2_base[i] = base * osc2_ratio;
                }
                let dt1 = f32x4::from(dt1) * drift_ratio;
                let (phase1, phase2) = (f32x4::from(phase1), f32x4::from(phase2));

                let v1 = osc_lanes(p.osc1.waveform, phase1, osc1_shape, dt1);
                let dt2 = f32x4::from(dt2_base) * (splat(1.0) + splat(osc2_fm * 4.0) * v1);
                let dt2_abs = dt2.abs().max(splat(1.0e-7));
                let v2 = osc_lanes(p.osc2.waveform, phase2, osc2_shape, dt2_abs);

                let next1 = phase1 + dt1;
                let next1 = next1.simd_ge(splat(1.0)).select(next1 - splat(1.0), next1).to_array();
                let next2 = wrap01_lanes(phase2 + dt2).to_array();
                let s = ((v1 * splat(g_osc1) + v2 * splat(g_osc2)) * splat(uni_norm)).to_array();
                for (i, u) in voice.unison.iter_mut().take(copies).enumerate() {
                    u.phase1 = next1[i];
                    u.phase2 = next2[i];
                    dry_l += s[i] * uni_pan[i].0;
                    dry_r += s[i] * uni_pan[i].1;
                }
            } else {
                for (i, u) in voice.unison.iter_mut().take(copies).enumerate() {
                    u.drift += (xorshift32(&mut u.drift_rng) - u.drift) * drift_coeff;

                    // Drift is at most 15 cents: 2^x's series to x^2 is exact to
                    // well under a tenth of a cent there, and far cheaper.
                    let d = u.drift * drift_depth;
                    let drift_ratio = 1.0 + d + 0.5 * d * d;
                    let base = base_hz * uni_ratio[i] * inv_sr;
                    let dt1 = base * osc1_ratio * drift_ratio;
                    let dt2_base = base * osc2_ratio;

                    let v1 = osc_sample(p.osc1.waveform, u.phase1, osc1_shape, dt1, false);

                    // FM: osc 1 pushes osc 2's frequency around. Through-zero
                    // is allowed; band-limiting uses the step's magnitude.
                    let dt2 = dt2_base * (1.0 + osc2_fm * 4.0 * v1);
                    let dt2_abs = dt2.abs().max(1.0e-7);
                    let mut v2 = osc_sample(p.osc2.waveform, u.phase2, osc2_shape, dt2_abs, u.synced) + u.sync_residual;
                    u.sync_residual = 0.0;
                    u.synced = false;

                    // Advance osc 1; if it wraps before the next sample and
                    // sync is on, osc 2 restarts at that sub-sample instant.
                    u.phase1 += dt1;
                    let mut next_phase2 = wrap01(u.phase2 + dt2);
                    if u.phase1 >= 1.0 {
                        u.phase1 -= 1.0;
                        if p.osc2.sync {
                            // Fraction of a sample since the wrap, at the next sample.
                            let x = (u.phase1 / dt1).clamp(0.0, 1.0);
                            let phase_at_reset = wrap01(u.phase2 + dt2 * (1.0 - x));
                            let before = naive(p.osc2.waveform, phase_at_reset, osc2_shape, dt2_abs);
                            let after = naive(p.osc2.waveform, 0.0, osc2_shape, dt2_abs);
                            let jump = after - before;
                            // Two-sample BLEP: this sample gets the lead-in,
                            // the next the lead-out.
                            v2 += jump * 0.5 * x * x;
                            u.sync_residual = -jump * 0.5 * (1.0 - x) * (1.0 - x);
                            next_phase2 = wrap01(dt2 * x);
                            u.synced = true;
                        }
                    }
                    u.phase2 = next_phase2;

                    let s = (v1 * g_osc1 + v2 * g_osc2) * uni_norm;
                    dry_l += s * uni_pan[i].0;
                    dry_r += s * uni_pan[i].1;
                }
            }

            // Sub and noise sit in the centre, once per voice.
            let sub = if g_sub == 0.0 { 0.0 } else { (voice.sub_phase * std::f32::consts::TAU).sin() * g_sub };
            voice.sub_phase = fract_fast(voice.sub_phase + base_hz * sub_oct / sr);
            let noise = xorshift32(&mut voice.noise_state) * g_noise;
            dry_l += sub + noise;
            dry_r += sub + noise;

            let key_oct = key_track * ((voice.note as f32 - REF_NOTE) / 12.0);
            let env_oct = env_amount * filter_level;
            let cutoff_arg = cutoff_log2 + key_oct + env_oct + cutoff_lfo_oct;
            coeffs[vi] = voice.coeff_memo.get((cutoff_arg, resonance), |(arg, resonance)| {
                let cutoff = 2f32.powf(arg).clamp(20.0, 20_000.0).min(sr * 0.45);
                SvfCoeffs::new(cutoff, resonance, sr)
            });
            level[vi] = amp_level * voice.velocity_gain;
            for (ch, dry) in [dry_l, dry_r].into_iter().enumerate() {
                pre[vi][ch] = voice.dc[ch].process(dry) * drive_gain;
            }
            live[vi] = true;
        }

        // Pass 2: the drive stage, two voices at a time as one vector. A
        // pair with no live voice is skipped; a dead one in a live pair
        // gets silence.
        for (b, bank) in self.drive.iter_mut().enumerate() {
            let (v0, v1) = (2 * b, 2 * b + 1);
            if !(live[v0] || live[v1]) {
                continue;
            }
            let out = bank.process([pre[v0][0], pre[v0][1], pre[v1][0], pre[v1][1]]);
            pre[v0] = [out[0], out[1]];
            pre[v1] = [out[2], out[3]];
        }

        // Pass 3, per voice: filter, level, sum.
        for (vi, voice) in self.voices.iter_mut().enumerate() {
            if !live[vi] {
                continue;
            }
            let l = voice.filter[0].process(pre[vi][0], p.filter.filter_type, &coeffs[vi]) * level[vi];
            let r = voice.filter[1].process(pre[vi][1], p.filter.filter_type, &coeffs[vi]) * level[vi];
            sum_l += l;
            sum_r += r;
        }

        let gain = volume * VOICE_GAIN;
        let (l, r) = (sum_l * gain, sum_r * gain);
        let (l, r) = self.chorus.process(l, r, p.fx.chorus_depth, p.fx.chorus_mix);
        let (l, r) = self.reverb.process(l, r, p.fx.reverb_size, p.fx.reverb_mix);
        self.limiter.process(l, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::synth::{seed_synth, Envelope as EnvParams, SynthState};

    const SR: f32 = 48_000.0;

    /// A free LFO starts its cycle with each phrase, so the same note
    /// sounds the same every time - but a chord's notes, played together,
    /// share one sweep.
    #[test]
    fn a_free_lfo_restarts_with_each_phrase() {
        let mut s = seed_synth();
        s.lfo1.depth = 1.0;
        let mut e = SynthEngine::new(SR);
        e.set_params(SynthParams::from_state(&s));
        let on = |e: &mut SynthEngine, note| e.handle_note_event(NoteEvent { slot: 0, note, on: true, velocity: 100 });
        on(&mut e, 45);
        for _ in 0..10_000 {
            e.process();
        }
        let moved = e.lfo_phases().0;
        assert!(moved > 0.01);
        // Another note while one's held: the sweep carries on.
        on(&mut e, 52);
        assert_eq!(e.lfo_phases().0, moved);
        // Let go of everything, then a new phrase: back to the start.
        e.handle_note_event(NoteEvent { slot: 0, note: ALL_NOTES_OFF, on: false, velocity: 0 });
        e.process();
        on(&mut e, 45);
        assert_eq!(e.lfo_phases().0, 0.0);
    }

    /// A synced LFO starts every cycle on its beat, wherever the song was
    /// started from, and runs on at the tempo while stopped.
    #[test]
    fn a_synced_lfo_locks_to_the_beat() {
        let mut s = seed_synth();
        s.lfo1.beat_sync = true;
        s.lfo1.rate_norm = shared::synth::division_norm("1/8");
        s.lfo1.depth = 1.0;
        let mut e = SynthEngine::new(SR);
        e.set_params(SynthParams::from_state(&s));
        e.handle_note_event(NoteEvent { slot: 0, note: 45, on: true, velocity: 100 });
        // 120 BPM: a beat is 24 000 samples, an 8th 12 000.
        let per_sample = 1.0 / 24_000.0;
        // Started from beat 3.25: half an 8th past beat 3, so half a cycle.
        e.set_clock(Some(3.25), per_sample);
        e.process();
        assert!((e.lfo_phases().0 - 0.5).abs() < 1e-3, "{}", e.lfo_phases().0);
        // Another 8th on: the same place in the next cycle.
        for _ in 0..12_000 {
            e.process();
        }
        assert!((e.lfo_phases().0 - 0.5).abs() < 1e-3, "{}", e.lfo_phases().0);
        // Seek to a beat: the cycle starts there.
        e.set_clock(Some(8.0), per_sample);
        e.process();
        assert!(e.lfo_phases().0 < 1e-3);
        // Stopped: it keeps going at the tempo, an 8th per cycle.
        e.set_clock(None, per_sample);
        let before = e.lfo_phases().0;
        for _ in 0..6_000 {
            e.process();
        }
        assert!((e.lfo_phases().0 - (before + 0.5)).abs() < 1e-3);
        // Off: the Rate knob's Hz again.
        s.lfo1.beat_sync = false;
        assert_eq!(SynthParams::from_state(&s).lfo1_per_beat, 0.0);
        // The old, never-used flag changes nothing: Warm Bass has it on
        // and must sound as it always did.
        let warm = seed_synth();
        assert!(warm.lfo1.sync && !warm.lfo1.beat_sync);
        assert_eq!(SynthParams::from_state(&warm).lfo1_per_beat, 0.0);
    }

    /// A single raw oscillator straight to the output: everything else
    /// (osc 2, sub, noise, drive, resonance, envelopes, LFOs) neutralised,
    /// the filter wide open.
    fn raw_osc(waveform: Waveform, shape: f32) -> SynthState {
        let mut s = seed_synth();
        s.voice_mode = VoiceMode::Mono;
        s.osc1.waveform = waveform;
        s.osc1.octave = 0;
        s.osc1.knob_a_cents = 0.0;
        s.osc1.knob_b = shape;
        s.osc1.knob_c = 0.0;
        s.osc2.sync = false;
        s.mix.osc1_db = 0.0;
        s.mix.osc2_db = -200.0;
        s.mix.sub_db = -200.0;
        s.mix.noise_db = -200.0;
        s.filter.filter_type = FilterType::Lp12;
        s.filter.cutoff_hz = 20_000.0;
        s.filter.resonance = 0.0;
        s.filter.drive_db = 0.0;
        s.filter.env_amount_oct = 0.0;
        s.filter.key_track = 0.0;
        let flat = EnvParams { attack_ms: 0.5, decay_ms: 1.0, sustain: 1.0, release_ms: 5.0 };
        s.amp_env = flat;
        s.filter_env = flat;
        s.lfo1.depth = 0.0;
        s.lfo2.depth = 0.0;
        s.output.volume_db = 0.0;
        s
    }

    fn peak(engine: &mut SynthEngine, seconds: f32) -> f32 {
        (0..(SR * seconds) as usize).map(|_| engine.process().0.abs()).fold(0.0, f32::max)
    }

    #[test]
    fn lane_oscillators_match_the_scalar_ones() {
        let mut rng = 12_345u32;
        for waveform in [Waveform::Sine, Waveform::Square, Waveform::Triangle, Waveform::Saw] {
            for shape in [0.0, 0.2, 0.5, 0.97, 1.0] {
                for _ in 0..2_000 {
                    let t: [f32; 4] = std::array::from_fn(|_| xorshift32(&mut rng) * 0.5 + 0.5);
                    let dt: [f32; 4] = std::array::from_fn(|_| (xorshift32(&mut rng) * 0.5 + 0.5) * 0.3 + 1.0e-5);
                    let got = osc_lanes(waveform, f32x4::from(t), shape, f32x4::from(dt)).to_array();
                    for i in 0..4 {
                        let want = osc_sample(waveform, t[i], shape, dt[i], false);
                        assert_eq!(got[i].to_bits(), want.to_bits(), "{waveform:?} shape {shape} t {} dt {}", t[i], dt[i]);
                    }
                }
            }
        }
    }

    #[test]
    fn a_mono_note_in_the_last_ones_release_tail_retriggers() {
        // Deep Rave Bass style: Mono, a release long enough that the voice
        // is still fading when the next note starts. Both notes must hit
        // equally hard.
        let mut s = raw_osc(Waveform::Saw, 0.0);
        s.amp_env = EnvParams { attack_ms: 2.0, decay_ms: 400.0, sustain: 0.85, release_ms: 300.0 };
        let mut engine = SynthEngine::new(SR);
        engine.set_params(SynthParams::from_state(&s));
        let hit = |e: &mut SynthEngine, on: bool| e.handle_note_event(NoteEvent { slot: 0, note: 45, on, velocity: 110 });
        hit(&mut engine, true);
        let first = peak(&mut engine, 0.1);
        hit(&mut engine, false);
        peak(&mut engine, 0.25); // into the release tail, voice still active
        hit(&mut engine, true);
        let second = peak(&mut engine, 0.1);
        assert!(second > first * 0.8, "second note {second} vs first {first}");
    }

    #[test]
    fn mono_legato_still_glides_while_a_key_is_held() {
        let mut s = raw_osc(Waveform::Saw, 0.0);
        s.output.glide_ms = 100.0;
        let mut engine = SynthEngine::new(SR);
        engine.set_params(SynthParams::from_state(&s));
        engine.handle_note_event(NoteEvent { slot: 0, note: 45, on: true, velocity: 110 });
        peak(&mut engine, 0.05);
        engine.handle_note_event(NoteEvent { slot: 0, note: 57, on: true, velocity: 110 });
        // Held-over: gliding from 45 toward 57, not jumped there.
        engine.process();
        assert!(engine.voices[0].current_note < 50.0, "{}", engine.voices[0].current_note);
    }

    fn render(state: &SynthState, note: u8, seconds: f32) -> Vec<f32> {
        let mut engine = SynthEngine::new(SR);
        engine.set_params(SynthParams::from_state(state));
        engine.handle_note_event(NoteEvent { slot: 0, note, on: true, velocity: 127 });
        (0..(SR * seconds) as usize).map(|_| engine.process().0).collect()
    }

    /// Energy off the harmonic series of `f0`, relative to the energy on
    /// it, in dB: how loud the aliasing is next to the real tone. Naive
    /// DFT over a Hann-windowed slice - slow, but this is a test.
    fn alias_to_signal_db(samples: &[f32], f0: f32) -> f32 {
        const N: usize = 4096;
        let start = samples.len() - N;
        let windowed: Vec<f32> = (0..N)
            .map(|i| {
                let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / N as f32).cos();
                samples[start + i] * w
            })
            .collect();
        let bin_hz = SR / N as f32;
        let (mut harmonic, mut alias) = (0.0f64, 0.0f64);
        for k in 2..N / 2 {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            let step = std::f64::consts::TAU * k as f64 / N as f64;
            for (i, &x) in windowed.iter().enumerate() {
                let a = step * i as f64;
                re += x as f64 * a.cos();
                im -= x as f64 * a.sin();
            }
            let power = re * re + im * im;
            let freq = k as f32 * bin_hz;
            let nearest = (freq / f0).round().max(1.0) * f0;
            if (freq - nearest).abs() <= 3.0 * bin_hz {
                harmonic += power;
            } else {
                alias += power;
            }
        }
        10.0 * (alias / harmonic).log10() as f32
    }

    fn report(name: &str, state: &SynthState, note: u8) -> f32 {
        let samples = render(state, note, 0.3);
        let f0 = midi_to_hz(note as f32);
        let db = alias_to_signal_db(&samples, f0);
        println!("{name:<28} note {note} ({f0:.0} Hz): aliasing {db:.1} dB");
        db
    }

    /// Prints the aliasing figures (`cargo test -p engine alias -- --nocapture`).
    #[test]
    fn alias_measurements() {
        report("saw", &raw_osc(Waveform::Saw, 0.0), 100);
        report("square 50%", &raw_osc(Waveform::Square, 0.0), 100);
        report("pulse 20%", &raw_osc(Waveform::Square, 2.0 / 3.0), 100);
        report("triangle", &raw_osc(Waveform::Triangle, 0.0), 100);
        report("saw->tri shape 50%", &raw_osc(Waveform::Saw, 0.5), 100);
        let mut synced = raw_osc(Waveform::Saw, 0.0);
        synced.mix.osc1_db = -200.0;
        synced.mix.osc2_db = 0.0;
        synced.osc2.waveform = Waveform::Saw;
        synced.osc2.knob_b = 0.0;
        synced.osc2.knob_a_cents = 0.0;
        synced.osc2.octave = 1;
        synced.osc2.sync = true;
        report("osc2 saw, synced +1 oct", &synced, 88);
        let mut driven = raw_osc(Waveform::Saw, 0.0);
        driven.filter.drive_db = 18.0;
        report("saw, drive +18 dB", &driven, 88);
        // Read these against the plain saw at the same note: around -40 dB
        // is this measurement's own floor there (window leakage from the
        // saw's many harmonics, plus pitch drift), and sync and drive land
        // within about a dB of it - they add no aliasing it can see.
        for note in [48, 60, 72, 88] {
            report("plain saw (baseline)", &raw_osc(Waveform::Saw, 0.0), note);
            report("osc2 saw, synced +1 oct", &synced, note);
            report("saw, drive +18 dB", &driven, note);
        }
    }

    /// Guards the headline numbers: the raw oscillators' aliasing must stay
    /// far below the tone (it was -13 to -17 dB before band-limiting).
    #[test]
    fn oscillators_are_band_limited() {
        for (name, state) in [
            ("saw", raw_osc(Waveform::Saw, 0.0)),
            ("square", raw_osc(Waveform::Square, 0.0)),
            ("pulse", raw_osc(Waveform::Square, 2.0 / 3.0)),
            ("triangle", raw_osc(Waveform::Triangle, 0.0)),
        ] {
            let db = report(name, &state, 100);
            assert!(db < -40.0, "{name}: aliasing {db:.1} dB");
        }
    }

    /// Worst case CPU: 16 voices x 4 unison copies with every effect on.
    /// Prints the real-time factor (`-- --nocapture`); fails only if a
    /// release build can't render faster than real time.
    #[test]
    fn renders_faster_than_real_time() {
        let mut s = raw_osc(Waveform::Saw, 0.0);
        s.voice_mode = VoiceMode::Poly;
        s.voices = 16;
        s.unison.voices = 4;
        s.osc2.sync = true;
        s.mix.osc2_db = -6.0;
        s.fx.chorus_mix = 0.5;
        s.fx.reverb_mix = 0.3;
        let mut engine = SynthEngine::new(SR);
        engine.set_params(SynthParams::from_state(&s));
        for note in 40..56 {
            engine.handle_note_event(NoteEvent { slot: 0, note, on: true, velocity: 127 });
        }
        let started = std::time::Instant::now();
        let mut acc = 0.0f32;
        for _ in 0..SR as usize {
            acc += engine.process().0;
        }
        let elapsed = started.elapsed().as_secs_f32();
        println!("1 s of 16 voices x 4 unison rendered in {:.1} ms ({:.0}x real time) [{acc}]", elapsed * 1000.0, 1.0 / elapsed);
        if !cfg!(debug_assertions) {
            assert!(elapsed < 1.0, "{elapsed}");
        }
    }

    /// A sine through the filter: its steady peak level.
    fn filter_gain(hz: f32, cutoff: f32, resonance: f32, ft: FilterType, level: f32) -> f32 {
        let c = SvfCoeffs::new(cutoff, resonance, SR);
        let mut f = Filter::default();
        let mut peak = 0.0f32;
        for i in 0..SR as usize {
            let y = f.process(level * (i as f32 * hz / SR * std::f32::consts::TAU).sin(), ft, &c);
            if i > SR as usize / 2 {
                peak = peak.max(y.abs());
            }
        }
        peak / level
    }

    /// The LP 24 ladder's resonance is a musical bump, not a whistle: at
    /// 50% a few dB over the low end (it was +29 dB with two resonant
    /// stages in a row), and bounded at full - while no resonance is the
    /// same gentle slope as before.
    #[test]
    fn ladder_resonance_is_musical() {
        let db = |g: f32| 20.0 * g.max(1e-9).log10();
        let bump = |res: f32| db(filter_gain(1000.0, 1000.0, res, FilterType::Lp24, 0.1)) - db(filter_gain(100.0, 1000.0, res, FilterType::Lp24, 0.1));
        for res in [0.0, 0.25, 0.5, 0.75, 1.0] {
            println!("resonance {res:.2}: {:+.1} dB at the cutoff over the low end; full-level peak {:.2}", bump(res), filter_gain(1000.0, 1000.0, res, FilterType::Lp24, 1.0));
        }
        assert!((bump(0.0) + 12.0).abs() < 0.6, "{}", bump(0.0));
        assert!((3.0..=12.0).contains(&bump(0.5)), "{}", bump(0.5));
        assert!(bump(0.75) > bump(0.5) + 3.0);
        // Even ringing at full resonance, a full-level note stays near it.
        assert!(filter_gain(1000.0, 1000.0, 1.0, FilterType::Lp24, 1.0) < 3.0);
        // Resonance doesn't hollow out the bass: within a few dB of none.
        let low = |res: f32| db(filter_gain(100.0, 1000.0, res, FilterType::Lp24, 0.1));
        assert!((low(0.75) - low(0.0)).abs() < 6.0, "{} vs {}", low(0.75), low(0.0));
    }

    #[test]
    fn output_never_exceeds_full_scale() {
        let mut s = raw_osc(Waveform::Saw, 0.0);
        s.voice_mode = VoiceMode::Poly;
        s.filter.resonance = 1.0;
        s.filter.drive_db = 24.0;
        s.output.volume_db = 6.0;
        s.unison.voices = 4;
        let mut engine = SynthEngine::new(SR);
        engine.set_params(SynthParams::from_state(&s));
        for note in [36, 43, 48, 55, 60, 64, 67, 72] {
            engine.handle_note_event(NoteEvent { slot: 0, note, on: true, velocity: 127 });
        }
        let peak = (0..SR as usize).map(|_| { let (l, r) = engine.process(); l.abs().max(r.abs()) }).fold(0.0, f32::max);
        assert!(peak <= 1.0, "{peak}");
    }
}

