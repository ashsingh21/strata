//! Carve's DSP: a small polyphonic subtractive-synth voice engine.
//!
//! One [`SvfStage`] (Andrew Simper's "TPT" topology) per filter pole pair,
//! cascaded twice for the 24 dB/oct mode; one linear-in-time ADSR per voice
//! for both the amp and filter envelopes; a single free-running LFO pair
//! shared by all voices (LFO1 -> filter cutoff, LFO2 -> pitch vibrato).
//! Everything here runs on the audio thread: no allocation once `SynthEngine`
//! is built, no locks, no syscalls.

use shared::synth::{FilterType, LfoTarget, NoteEvent, SynthParams, VoiceMode, Waveform};

const MAX_VOICES: usize = 16;
/// Reference note for octave/key-tracking math (C4).
const REF_NOTE: f32 = 60.0;

fn midi_to_hz(note: f32) -> f32 {
    440.0 * 2f32.powf((note - 69.0) / 12.0)
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
    fn tick(&mut self, params: &shared::synth::Envelope, sample_rate: f32) -> f32 {
        let coeff = |ms: f32| 1.0 - (-1.0 / (ms.max(0.5) * 0.001 * sample_rate)).exp();
        match self.stage {
            EnvStage::Idle => self.level = 0.0,
            EnvStage::Attack => {
                self.level += (1.0 - self.level) * coeff(params.attack_ms);
                if self.level >= 0.999 {
                    self.level = 1.0;
                    self.stage = EnvStage::Decay;
                }
            }
            EnvStage::Decay => {
                self.level += (params.sustain - self.level) * coeff(params.decay_ms);
                if (self.level - params.sustain).abs() < 0.001 {
                    self.level = params.sustain;
                    self.stage = EnvStage::Sustain;
                }
            }
            EnvStage::Sustain => self.level = params.sustain,
            EnvStage::Release => {
                self.level += (0.0 - self.level) * coeff(params.release_ms);
                if self.level < 0.0005 {
                    self.level = 0.0;
                    self.stage = EnvStage::Idle;
                }
            }
        }
        self.level
    }
}

/// One 2-pole (12 dB/oct) state-variable filter stage producing
/// simultaneous low/band/high-pass outputs; `Lp24` cascades two of these.
#[derive(Clone, Copy, Default)]
struct SvfStage {
    ic1eq: f32,
    ic2eq: f32,
}

impl SvfStage {
    /// Returns (lowpass, bandpass, highpass) for one input sample.
    fn process(&mut self, input: f32, cutoff_hz: f32, q: f32, sample_rate: f32) -> (f32, f32, f32) {
        let g = (std::f32::consts::PI * cutoff_hz / sample_rate).tan();
        let k = 1.0 / q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;

        let v3 = input - self.ic2eq;
        let v1 = a1 * self.ic1eq + a2 * v3;
        let v2 = self.ic2eq + a2 * self.ic1eq + a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;

        let lowpass = v2;
        let bandpass = v1;
        let highpass = input - k * v1 - v2;
        (lowpass, bandpass, highpass)
    }
}

fn xorshift32(state: &mut u32) -> f32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// A ramp that rises linearly from -1 to 1 over `[0, duty)` then falls back
/// to -1 over `[duty, 1)`: a symmetric triangle at `duty = 0.5`, a rising
/// sawtooth as `duty -> 1`, a falling one as `duty -> 0`.
fn variable_ramp(phase01: f32, duty: f32) -> f32 {
    let duty = duty.clamp(0.02, 0.98);
    if phase01 < duty {
        -1.0 + 2.0 * (phase01 / duty)
    } else {
        1.0 - 2.0 * ((phase01 - duty) / (1.0 - duty))
    }
}

/// Raw, unfiltered waveform sample for a phase in `0..1` cycles. `shape` is
/// 0..1 and, for every waveform but Square, is a "stay pure at 0, morph
/// away with more character as it rises" control: Sine gains a folded
/// harmonic, Triangle skews toward a sawtooth, Saw rounds toward a
/// triangle - so turning it up always adds movement rather than requiring
/// a specific waveform to hear anything.
fn osc_sample(waveform: Waveform, phase01: f32, shape: f32) -> f32 {
    let shape = shape.clamp(0.0, 1.0);
    match waveform {
        Waveform::Sine => {
            let fold = shape * 0.6;
            let raw = (phase01 * std::f32::consts::TAU).sin() + fold * (phase01 * std::f32::consts::TAU * 2.0).sin();
            raw / (1.0 + fold)
        }
        Waveform::Triangle => variable_ramp(phase01, 0.5 + shape * 0.48),
        Waveform::Saw => variable_ramp(phase01, 1.0 - shape * 0.5),
        Waveform::Square => {
            let duty = shape.clamp(0.05, 0.95);
            if phase01 < duty {
                1.0
            } else {
                -1.0
            }
        }
    }
}

struct Voice {
    active: bool,
    note: u8,
    current_note: f32,
    target_note: f32,
    osc1_phase: f32,
    osc2_phase: f32,
    sub_phase: f32,
    noise_state: u32,
    /// Slow-wandering analog-style pitch drift for osc1, in -1..1.
    drift: f32,
    drift_rng: u32,
    amp_env: Envelope,
    filter_env: Envelope,
    filter: [SvfStage; 2],
}

impl Voice {
    fn new(seed: u32) -> Self {
        Self {
            active: false,
            note: 0,
            current_note: REF_NOTE,
            target_note: REF_NOTE,
            osc1_phase: 0.0,
            osc2_phase: 0.0,
            sub_phase: 0.0,
            noise_state: seed | 1,
            drift: 0.0,
            drift_rng: (seed ^ 0x5bd1_e995) | 1,
            amp_env: Envelope::new(),
            filter_env: Envelope::new(),
            filter: [SvfStage::default(); 2],
        }
    }
}

pub struct SynthEngine {
    voices: [Voice; MAX_VOICES],
    next_voice: usize,
    mono_stack: Vec<u8>,
    lfo1_phase: f32,
    lfo2_phase: f32,
    params: SynthParams,
}

impl SynthEngine {
    pub fn new() -> Self {
        let voices = std::array::from_fn(|i| Voice::new(0x9e37_79b9u32.wrapping_mul(i as u32 + 1)));
        Self {
            voices,
            next_voice: 0,
            mono_stack: Vec::with_capacity(MAX_VOICES),
            lfo1_phase: 0.0,
            lfo2_phase: 0.0,
            params: SynthParams::default(),
        }
    }

    pub fn set_params(&mut self, params: SynthParams) {
        self.params = params;
    }

    pub fn handle_note_event(&mut self, event: NoteEvent) {
        if event.on {
            self.note_on(event.note);
        } else {
            self.note_off(event.note);
        }
    }

    fn note_on(&mut self, note: u8) {
        if self.params.voice_mode == VoiceMode::Mono {
            self.mono_stack.retain(|n| *n != note);
            self.mono_stack.push(note);
            let legato = self.voices[0].active;
            self.voices[0].note = note;
            self.voices[0].target_note = note as f32;
            if !legato {
                self.voices[0].current_note = note as f32;
                self.voices[0].active = true;
                self.voices[0].amp_env.note_on();
                self.voices[0].filter_env.note_on();
            }
            return;
        }

        let max_voices = (self.params.max_voices as usize).clamp(1, MAX_VOICES);
        let slot = (0..max_voices)
            .find(|i| !self.voices[*i].active)
            .unwrap_or_else(|| {
                let slot = self.next_voice % max_voices;
                self.next_voice = self.next_voice.wrapping_add(1);
                slot
            });

        let voice = &mut self.voices[slot];
        voice.active = true;
        voice.note = note;
        voice.current_note = note as f32;
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
    pub fn process(&mut self, sample_rate: f32) -> (f32, f32) {
        let p = &self.params;

        self.lfo1_phase = (self.lfo1_phase + p.lfo1_rate_hz / sample_rate).fract();
        self.lfo2_phase = (self.lfo2_phase + p.lfo2_rate_hz / sample_rate).fract();
        let lfo1 = (self.lfo1_phase * std::f32::consts::TAU).sin();
        let lfo2 = (self.lfo2_phase * std::f32::consts::TAU).sin();

        // Each LFO adds into whichever accumulator its target picks -
        // either one (or both) can drive cutoff and/or pitch.
        let mut cutoff_lfo_oct = 0.0f32;
        let mut pitch_lfo_cents = 0.0f32;
        for (lfo_sin, depth, target) in [(lfo1, p.lfo1_depth, p.lfo1_target), (lfo2, p.lfo2_depth, p.lfo2_target)] {
            match target {
                LfoTarget::Cutoff => cutoff_lfo_oct += lfo_sin * depth * 2.0,
                LfoTarget::Pitch => pitch_lfo_cents += lfo_sin * depth * 50.0,
            }
        }
        let vibrato_ratio = 2f32.powf(pitch_lfo_cents / 1200.0);

        let glide_coeff = 1.0 - (-1.0 / (p.glide_ms.max(0.5) * 0.001 * sample_rate)).exp();
        // ~2 second time constant: slow enough to feel like wander, not vibrato.
        let drift_coeff = 1.0 - (-1.0 / (2.0 * sample_rate)).exp();
        const DRIFT_MAX_CENTS: f32 = 15.0;

        let mix_osc1 = db_to_gain(p.mix.osc1_db);
        let mix_osc2 = db_to_gain(p.mix.osc2_db);
        let mix_sub = db_to_gain(p.mix.sub_db);
        let mix_noise = db_to_gain(p.mix.noise_db);
        let drive_gain = db_to_gain(p.filter.drive_db);
        let q = 0.5 + p.filter.resonance.clamp(0.0, 1.0).powf(2.0) * 19.5;

        let active_count = self.voices.iter().filter(|v| v.active).count().max(1);
        let mut sum = 0.0f32;

        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }

            voice.current_note += (voice.target_note - voice.current_note) * glide_coeff;

            let amp_level = voice.amp_env.tick(&p.amp_env, sample_rate);
            let filter_level = voice.filter_env.tick(&p.filter_env, sample_rate);

            if voice.amp_env.stage == EnvStage::Idle {
                voice.active = false;
                continue;
            }

            voice.drift += (xorshift32(&mut voice.drift_rng) - voice.drift) * drift_coeff;

            let base_hz = midi_to_hz(voice.current_note) * vibrato_ratio;

            let osc1_hz = base_hz
                * 2f32.powf(p.osc1.octave as f32)
                * 2f32.powf((p.osc1.knob_a_cents + voice.drift * p.osc1.knob_c * DRIFT_MAX_CENTS) / 1200.0);
            let osc2_hz = base_hz * 2f32.powf(p.osc2.octave as f32) * 2f32.powf(p.osc2.knob_a_cents / 1200.0);
            let sub_hz = base_hz * 2f32.powf((p.osc1.octave - 1) as f32);

            let osc1_sample = osc_sample(p.osc1.waveform, voice.osc1_phase, p.osc1.knob_b);
            voice.osc1_phase += osc1_hz / sample_rate;
            let wrapped = voice.osc1_phase >= 1.0;
            if wrapped {
                voice.osc1_phase -= 1.0;
                if p.osc2.sync {
                    voice.osc2_phase = 0.0;
                }
            }

            let fm_step = (osc2_hz / sample_rate) * (1.0 + p.osc2.knob_c * 4.0 * osc1_sample);
            let osc2_sample = osc_sample(p.osc2.waveform, voice.osc2_phase, p.osc2.knob_b);
            voice.osc2_phase = (voice.osc2_phase + fm_step).rem_euclid(1.0);

            let sub_sample = (voice.sub_phase * std::f32::consts::TAU).sin();
            voice.sub_phase = (voice.sub_phase + sub_hz / sample_rate).fract();

            let noise_sample = xorshift32(&mut voice.noise_state);

            let mut dry = osc1_sample * mix_osc1
                + osc2_sample * mix_osc2
                + sub_sample * mix_sub
                + noise_sample * mix_noise;
            dry = (dry * drive_gain).tanh();

            let key_oct = p.filter.key_track * ((voice.note as f32 - REF_NOTE) / 12.0);
            let env_oct = p.filter.env_amount_oct * filter_level;
            let cutoff =
                (p.filter.cutoff_hz * 2f32.powf(key_oct + env_oct + cutoff_lfo_oct)).clamp(20.0, 20_000.0);

            let (lp1, bp1, hp1) = voice.filter[0].process(dry, cutoff, q, sample_rate);
            let filtered = match p.filter.filter_type {
                FilterType::Lp24 => voice.filter[1].process(lp1, cutoff, q, sample_rate).0,
                FilterType::Lp12 => lp1,
                FilterType::Bp => bp1,
                FilterType::Hp => hp1,
            };

            sum += filtered * amp_level;
        }

        // A resonant filter can push a single voice close to the final soft
        // limiter on its own; this leaves comfortable headroom before it.
        const OUTPUT_HEADROOM: f32 = 0.3;
        let master = db_to_gain(p.volume_db) * OUTPUT_HEADROOM;
        let out = (sum / (active_count as f32).sqrt() * master).tanh();
        (out, out)
    }
}

impl Default for SynthEngine {
    fn default() -> Self {
        Self::new()
    }
}
