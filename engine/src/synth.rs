//! Carve's DSP: a polyphonic subtractive-synth voice engine.
//!
//! Per voice: up to `MAX_UNISON` detuned copies of both oscillators, each
//! band-limited (PolyBLEP on edges, PolyBLAMP on corners, a BLEP on every
//! hard-sync reset) and panned across the stereo field; a sine sub and
//! white noise; a DC blocker, a 2x-oversampled anti-aliased (ADAA) `tanh`
//! drive and a stereo pair of Andrew Simper "TPT" state-variable filters
//! (cascaded twice for 24 dB/oct); exponential ADSRs for amp and filter. Every
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

use crate::dsp::{poly_blamp, DcBlocker, OversampledDrive, Smoother};
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

/// Coefficients shared by every stage running at one cutoff/Q.
#[derive(Clone, Copy)]
struct SvfCoeffs {
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
}

impl SvfCoeffs {
    fn new(cutoff_hz: f32, q: f32, sample_rate: f32) -> Self {
        let g = (std::f32::consts::PI * cutoff_hz / sample_rate).tan();
        let k = 1.0 / q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        Self { k, a1, a2, a3: g * a2 }
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

/// One channel's filter: two stages, used as one (12 dB) or two (24 dB).
#[derive(Clone, Copy, Default)]
struct Filter([SvfStage; 2]);

impl Filter {
    #[inline]
    fn process(&mut self, x: f32, filter_type: FilterType, c: &SvfCoeffs) -> f32 {
        let (lp, bp, hp) = self.0[0].process(x, c);
        match filter_type {
            FilterType::Lp24 => self.0[1].process(lp, c).0,
            FilterType::Lp12 => lp,
            FilterType::Bp => bp,
            FilterType::Hp => hp,
        }
    }
}

/// Wraps a phase into 0..1. Same result as `rem_euclid(1.0)`, without the
/// `fmod` - this runs several times per oscillator per sample.
#[inline]
fn wrap01(x: f32) -> f32 {
    x - x.floor()
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
    drive: [OversampledDrive; 2],
    filter: [Filter; 2],
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
            drive: [OversampledDrive::default(); 2],
            filter: [Filter::default(); 2],
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

pub struct SynthEngine {
    sample_rate: f32,
    voices: [Voice; MAX_VOICES],
    next_voice: usize,
    mono_stack: Vec<u8>,
    lfo1_phase: f32,
    lfo2_phase: f32,
    params: SynthParams,
    smoothed: Smoothed,
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
            next_voice: 0,
            mono_stack: Vec::with_capacity(MAX_VOICES),
            lfo1_phase: 0.0,
            lfo2_phase: 0.0,
            smoothed: Smoothed::new(&params, sample_rate),
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

        // --- Smoothed parameters (advanced once per sample, shared). ---
        let osc1_tune = sm.osc1_tune.next(p.osc1.knob_a_cents);
        let osc1_shape = sm.osc1_shape.next(p.osc1.knob_b);
        let osc1_drift = sm.osc1_drift.next(p.osc1.knob_c);
        let osc2_detune = sm.osc2_detune.next(p.osc2.knob_a_cents);
        let mut osc2_shape = sm.osc2_shape.next(p.osc2.knob_b);
        let osc2_fm = sm.osc2_fm.next(p.osc2.knob_c);
        let g_osc1 = sm.osc1_gain.next(db_to_gain(p.mix.osc1_db));
        let g_osc2 = sm.osc2_gain.next(db_to_gain(p.mix.osc2_db));
        let g_sub = sm.sub_gain.next(db_to_gain(p.mix.sub_db));
        let g_noise = sm.noise_gain.next(db_to_gain(p.mix.noise_db));
        let cutoff_log2 = sm.cutoff_log2.next(p.filter.cutoff_hz.max(1.0).log2());
        let mut resonance = sm.resonance.next(p.filter.resonance);
        let drive_gain = sm.drive_gain.next(db_to_gain(p.filter.drive_db));
        let env_amount = sm.env_amount.next(p.filter.env_amount_oct);
        let key_track = sm.key_track.next(p.filter.key_track);
        let volume = sm.volume_gain.next(db_to_gain(p.volume_db));
        let uni_detune = sm.unison_detune.next(p.unison.detune_cents);
        let uni_width = sm.unison_width.next(p.unison.width);

        // --- LFOs: each adds into whichever target it's patched to. ---
        self.lfo1_phase = (self.lfo1_phase + p.lfo1_rate_hz / sr).fract();
        self.lfo2_phase = (self.lfo2_phase + p.lfo2_rate_hz / sr).fract();
        let lfo1 = (self.lfo1_phase * std::f32::consts::TAU).sin();
        let lfo2 = (self.lfo2_phase * std::f32::consts::TAU).sin();
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
        let vibrato_ratio = 2f32.powf(pitch_lfo_cents / 1200.0);
        let q = 0.5 + resonance * resonance * 19.5;

        let glide_coeff = 1.0 - (-1.0 / (p.glide_ms.max(0.5) * 0.001 * sr)).exp();
        let amp_coeffs = EnvCoeffs::new(&p.amp_env, sr);
        let filter_coeffs = EnvCoeffs::new(&p.filter_env, sr);
        // ~2 second time constant: slow enough to feel like wander, not vibrato.
        let drift_coeff = 1.0 - (-1.0 / (2.0 * sr)).exp();

        // --- Unison layout: detune offsets and equal-power pan gains. ---
        let copies = (p.unison.voices as usize).clamp(1, UNISON);
        let mut uni_cents = [0.0f32; UNISON];
        let mut uni_pan = [(1.0f32, 1.0f32); UNISON];
        for i in 0..copies {
            let spread = if copies == 1 { 0.0 } else { -1.0 + 2.0 * i as f32 / (copies - 1) as f32 };
            uni_cents[i] = spread * uni_detune * 0.5;
            let angle = (spread * uni_width + 1.0) * std::f32::consts::FRAC_PI_4;
            uni_pan[i] = (angle.cos() * std::f32::consts::SQRT_2, angle.sin() * std::f32::consts::SQRT_2);
        }
        let uni_norm = 1.0 / (copies as f32).sqrt();
        let inv_sr = 1.0 / sr;

        // Pitch factors shared by every voice, computed once per sample.
        let osc1_ratio = 2f32.powf(p.osc1.octave as f32 + osc1_tune / 1200.0);
        let osc2_ratio = 2f32.powf(p.osc2.octave as f32 + osc2_detune / 1200.0);
        let mut uni_ratio = [1.0f32; UNISON];
        for i in 0..copies {
            uni_ratio[i] = 2f32.powf(uni_cents[i] / 1200.0);
        }
        let drift_depth = osc1_drift * DRIFT_MAX_CENTS / 1200.0 * std::f32::consts::LN_2;
        let sub_oct = 2f32.powf((p.osc1.octave - 1) as f32);

        let mut sum_l = 0.0f32;
        let mut sum_r = 0.0f32;

        for voice in &mut self.voices {
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

            // Sub and noise sit in the centre, once per voice.
            let sub = (voice.sub_phase * std::f32::consts::TAU).sin() * g_sub;
            voice.sub_phase = (voice.sub_phase + base_hz * sub_oct / sr).fract();
            let noise = xorshift32(&mut voice.noise_state) * g_noise;
            dry_l += sub + noise;
            dry_r += sub + noise;

            let key_oct = key_track * ((voice.note as f32 - REF_NOTE) / 12.0);
            let env_oct = env_amount * filter_level;
            let cutoff =
                2f32.powf(cutoff_log2 + key_oct + env_oct + cutoff_lfo_oct).clamp(20.0, 20_000.0).min(sr * 0.45);
            let coeffs = SvfCoeffs::new(cutoff, q, sr);

            for (ch, dry) in [dry_l, dry_r].into_iter().enumerate() {
                let x = voice.dc[ch].process(dry);
                let x = voice.drive[ch].process(x * drive_gain);
                let y = voice.filter[ch].process(x, p.filter.filter_type, &coeffs) * amp_level * voice.velocity_gain;
                if ch == 0 {
                    sum_l += y;
                } else {
                    sum_r += y;
                }
            }
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
