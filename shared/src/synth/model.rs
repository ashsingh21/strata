//! Carve, Strata's subtractive synth console: pure parameter data. This
//! milestone is visual + interactive only (see the crate README) - nothing
//! here is wired to the audio engine yet, so every value is just state a
//! panel of knobs reads and writes.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waveform {
    Sine,
    Triangle,
    Saw,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterType {
    Lp24,
    Lp12,
    Bp,
    Hp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceMode {
    Mono,
    Poly,
}

/// An oscillator's three knobs mean different things per-oscillator (Tune
/// vs Detune, Shape vs PW, Drift vs FM); the UI labels them, this just
/// stores the three normalized/physical values uniformly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oscillator {
    pub waveform: Waveform,
    pub octave: i8,
    /// Osc 1: tune in cents. Osc 2: detune in cents.
    pub knob_a_cents: f32,
    /// Osc 1: pulse/wave shape 0..1. Osc 2: pulse width 0..1.
    pub knob_b: f32,
    /// Osc 1: drift 0..1. Osc 2: FM amount 0..1.
    pub knob_c: f32,
    /// Osc 2 only.
    pub sync: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mix {
    pub osc1_db: f32,
    pub osc2_db: f32,
    pub sub_db: f32,
    pub noise_db: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Filter {
    pub filter_type: FilterType,
    pub cutoff_hz: f32,
    pub resonance: f32,
    pub drive_db: f32,
    pub env_amount_oct: f32,
    pub key_track: f32,
    /// Modulation ring on the Cutoff knob: depth in the knob's own 0..1
    /// space (not Hz), matching `Knob::modulation`'s convention.
    pub cutoff_mod_depth: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Envelope {
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lfo {
    pub rate_label: &'static str,
    pub rate_norm: f32,
    pub depth: f32,
    pub sync: bool,
    pub target_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Output {
    pub glide_ms: f32,
    pub volume_db: f32,
    pub meter_l: f32,
    pub meter_r: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SynthState {
    pub name: &'static str,
    pub voice_mode: VoiceMode,
    pub voices: u8,
    pub osc1: Oscillator,
    pub osc2: Oscillator,
    pub mix: Mix,
    pub filter: Filter,
    pub filter_env: Envelope,
    pub amp_env: Envelope,
    pub lfo1: Lfo,
    pub lfo2: Lfo,
    pub output: Output,
    /// MIDI-style note numbers currently held, for the keyboard strip.
    pub held_notes: Vec<u8>,
}

pub fn seed_synth() -> SynthState {
    SynthState {
        name: "Warm Bass",
        voice_mode: VoiceMode::Poly,
        voices: 8,
        osc1: Oscillator {
            waveform: Waveform::Saw,
            octave: -1,
            knob_a_cents: 0.0,
            knob_b: 0.35,
            knob_c: 0.12,
            sync: false,
        },
        osc2: Oscillator {
            waveform: Waveform::Square,
            octave: 0,
            knob_a_cents: 7.0,
            knob_b: 0.38,
            knob_c: 0.0,
            sync: true,
        },
        mix: Mix { osc1_db: -1.9, osc2_db: -5.2, sub_db: -10.0, noise_db: -28.0 },
        filter: Filter {
            filter_type: FilterType::Lp24,
            cutoff_hz: 1200.0,
            resonance: 0.62,
            drive_db: 4.5,
            env_amount_oct: 2.4,
            key_track: 0.5,
            cutoff_mod_depth: 0.15,
        },
        filter_env: Envelope { attack_ms: 4.0, decay_ms: 320.0, sustain: 0.35, release_ms: 280.0 },
        amp_env: Envelope { attack_ms: 2.0, decay_ms: 140.0, sustain: 0.80, release_ms: 220.0 },
        lfo1: Lfo { rate_label: "1/8", rate_norm: 0.45, depth: 0.6, sync: true, target_count: 1 },
        lfo2: Lfo { rate_label: "3.2 Hz", rate_norm: 0.3, depth: 0.4, sync: false, target_count: 1 },
        output: Output { glide_ms: 40.0, volume_db: -3.0, meter_l: 0.62, meter_r: 0.58 },
        held_notes: vec![60, 64, 67],
    }
}
