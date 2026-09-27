//! Carve, Strata's subtractive synth console: pure parameter data. This
//! milestone is visual + interactive only (see the crate README) - nothing
//! here is wired to the audio engine yet, so every value is just state a
//! panel of knobs reads and writes.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Waveform {
    Sine,
    Triangle,
    Saw,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterType {
    Lp24,
    Lp12,
    Bp,
    Hp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceMode {
    Mono,
    Poly,
}

/// What an LFO's output is patched to - set by dragging its pill onto a
/// knob, or by cycling its target button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LfoTarget {
    /// Filter cutoff, in octaves either side of the Cutoff knob's setting.
    Cutoff,
    /// Pitch (vibrato), in cents either side of the played note.
    Pitch,
    /// Osc 2's pulse width / shape (pulse-width modulation).
    PulseWidth,
    /// Filter resonance.
    Resonance,
}

impl LfoTarget {
    pub const ALL: [LfoTarget; 4] = [LfoTarget::Cutoff, LfoTarget::Pitch, LfoTarget::PulseWidth, LfoTarget::Resonance];

    pub fn name(self) -> &'static str {
        match self {
            LfoTarget::Cutoff => "Cutoff",
            LfoTarget::Pitch => "Pitch",
            LfoTarget::PulseWidth => "Pulse width",
            LfoTarget::Resonance => "Resonance",
        }
    }

    pub fn next(self) -> LfoTarget {
        let i = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

/// Unison: several detuned copies of both oscillators per note, spread
/// across the stereo field - the standard way a synth gets width.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Unison {
    /// Copies per note, 1 (off) to `MAX_UNISON`.
    pub voices: u8,
    /// Total spread between the outermost copies, in cents.
    pub detune_cents: f32,
    /// Stereo spread of the copies, 0 (mono) to 1 (hard left/right).
    pub width: f32,
}

pub const MAX_UNISON: u8 = 4;

impl Default for Unison {
    fn default() -> Self {
        Self { voices: 1, detune_cents: 14.0, width: 0.7 }
    }
}

/// The post-synth effects: a stereo chorus and a reverb. (The output
/// limiter is always on and has no controls.)
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fx {
    /// Chorus modulation depth, 0..1.
    pub chorus_depth: f32,
    /// Chorus wet/dry, 0..1 (0 = off).
    pub chorus_mix: f32,
    /// Reverb room size, 0..1.
    pub reverb_size: f32,
    /// Reverb wet/dry, 0..1 (0 = off).
    pub reverb_mix: f32,
}

impl Default for Fx {
    fn default() -> Self {
        Self { chorus_depth: 0.4, chorus_mix: 0.0, reverb_size: 0.5, reverb_mix: 0.0 }
    }
}

/// An oscillator's three knobs mean different things per-oscillator (Tune
/// vs Detune, Shape vs PW, Drift vs FM); the UI labels them, this just
/// stores the three normalized/physical values uniformly.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mix {
    pub osc1_db: f32,
    pub osc2_db: f32,
    pub sub_db: f32,
    pub noise_db: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Filter {
    pub filter_type: FilterType,
    pub cutoff_hz: f32,
    pub resonance: f32,
    pub drive_db: f32,
    pub env_amount_oct: f32,
    pub key_track: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lfo {
    /// Cosmetic only ("1/8", "3.2 Hz") - never read by the engine (the
    /// actual rate is `rate_norm`) and never updated after the synth is
    /// first built, so it isn't worth a save-able `String`. Not saved;
    /// a reloaded project just shows it blank.
    #[serde(skip)]
    pub rate_label: &'static str,
    pub rate_norm: f32,
    pub depth: f32,
    /// Tempo sync - not implemented by the engine (it always runs at
    /// `rate_norm`'s Hz) and not exposed in the UI; kept so existing saved
    /// projects/presets still deserialize.
    pub sync: bool,
    pub target: LfoTarget,
    pub target_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Output {
    pub glide_ms: f32,
    pub volume_db: f32,
    pub meter_l: f32,
    pub meter_r: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SynthState {
    /// The patch name - cosmetic only, not saved (blank after a reload).
    #[serde(skip)]
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
    /// `default` so projects saved before these existed still load.
    #[serde(default)]
    pub unison: Unison,
    #[serde(default)]
    pub fx: Fx,
    /// MIDI-style note numbers currently held, for the keyboard strip.
    pub held_notes: Vec<u8>,
}

/// How far an LFO at 100% depth swings each target. Shared by the engine
/// (which applies them) and the UI (which draws the swing as a ring), so
/// the two can't disagree.
pub const LFO_CUTOFF_MAX_OCT: f32 = 2.0;
pub const LFO_PITCH_MAX_CENTS: f32 = 50.0;
pub const LFO_PULSE_WIDTH_MAX: f32 = 0.4;
pub const LFO_RESONANCE_MAX: f32 = 0.5;

/// The Cutoff knob's range in octaves (20 Hz to 20 kHz, log taper).
const CUTOFF_KNOB_OCTAVES: f32 = 9.965_784; // log2(20_000 / 20)
/// The Tune knob's range in cents (-100..100).
const TUNE_KNOB_CENTS: f32 = 200.0;

/// How far LFO modulation currently swings `target`, in that target's own
/// knob's 0..1 space - the sum of every LFO patched to it. Drives the
/// knob's modulation ring (and, for Cutoff, the filter display's dashed
/// band), so what's drawn always matches what's applied.
pub fn lfo_mod_depth(s: &SynthState, target: LfoTarget) -> f32 {
    let mut depth = 0.0;
    for lfo in [&s.lfo1, &s.lfo2] {
        if lfo.target == target {
            depth += lfo.depth;
        }
    }
    let per_unit = match target {
        LfoTarget::Cutoff => LFO_CUTOFF_MAX_OCT / CUTOFF_KNOB_OCTAVES,
        LfoTarget::Pitch => LFO_PITCH_MAX_CENTS / TUNE_KNOB_CENTS,
        LfoTarget::PulseWidth => LFO_PULSE_WIDTH_MAX,
        LfoTarget::Resonance => LFO_RESONANCE_MAX,
    };
    (depth * per_unit).clamp(0.0, 1.0)
}

pub fn cutoff_mod_depth(s: &SynthState) -> f32 {
    lfo_mod_depth(s, LfoTarget::Cutoff)
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
        },
        filter_env: Envelope { attack_ms: 4.0, decay_ms: 320.0, sustain: 0.35, release_ms: 280.0 },
        amp_env: Envelope { attack_ms: 2.0, decay_ms: 140.0, sustain: 0.80, release_ms: 220.0 },
        lfo1: Lfo { rate_label: "1/8", rate_norm: 0.45, depth: 0.6, sync: true, target: LfoTarget::Cutoff, target_count: 1 },
        lfo2: Lfo { rate_label: "3.2 Hz", rate_norm: 0.3, depth: 0.4, sync: false, target: LfoTarget::Pitch, target_count: 1 },
        output: Output { glide_ms: 40.0, volume_db: -3.0, meter_l: 0.62, meter_r: 0.58 },
        unison: Unison::default(),
        fx: Fx::default(),
        // Not a demo chord: a fresh session starting with notes already
        // held meant they rang immediately with zero interaction, and
        // (since `held_notes.is_empty()` never went true) silently
        // blocked the very first step-entry recording until those 3
        // keys were clicked to release them by hand.
        held_notes: vec![],
    }
}

/// Carve's factory presets, as listed in the browser: (name, builder).
/// The last six are the recipe lessons' sounds (see `super::recipes`).
pub const PRESETS: [(&str, fn() -> SynthState); 10] = [
    ("Warm Bass", seed_synth),
    ("Deep Rave Bass", deep_rave_bass),
    ("Soft Pad", soft_pad),
    ("Deep Bass", super::recipes::deep_bass),
    ("Flute", super::recipes::flute),
    ("Indian Harp", super::recipes::indian_harp),
    ("Tanpura", super::recipes::tanpura),
    ("Reed", super::recipes::reed),
    ("Lead", super::recipes::lead),
    ("Lo-fi Keys", super::recipes::lofi_keys),
];

/// A wide, slow pad: detuned saws in 3-voice unison, a slow amp swell and
/// filter bloom, chorus and a big reverb - the opposite of a bass, so two
/// tracks' Carves obviously sound different.
pub fn soft_pad() -> SynthState {
    let mut s = seed_synth();
    s.name = "Soft Pad";
    s.voice_mode = VoiceMode::Poly;
    s.osc1 = Oscillator { waveform: Waveform::Saw, octave: 0, knob_a_cents: 0.0, knob_b: 0.15, knob_c: 0.25, sync: false };
    s.osc2 = Oscillator { waveform: Waveform::Saw, octave: 0, knob_a_cents: 9.0, knob_b: 0.15, knob_c: 0.0, sync: false };
    s.mix = Mix { osc1_db: -6.0, osc2_db: -6.0, sub_db: -60.0, noise_db: -38.0 };
    s.filter = Filter {
        filter_type: FilterType::Lp12,
        cutoff_hz: 900.0,
        resonance: 0.15,
        drive_db: 0.0,
        env_amount_oct: 1.2,
        key_track: 0.6,
    };
    s.filter_env = Envelope { attack_ms: 900.0, decay_ms: 1500.0, sustain: 0.5, release_ms: 1200.0 };
    s.amp_env = Envelope { attack_ms: 450.0, decay_ms: 800.0, sustain: 0.9, release_ms: 1400.0 };
    s.lfo1 = Lfo { rate_label: "", rate_norm: 0.25, depth: 0.12, sync: false, target: LfoTarget::Cutoff, target_count: 1 };
    s.lfo2 = Lfo { rate_label: "", rate_norm: 0.35, depth: 0.25, sync: false, target: LfoTarget::PulseWidth, target_count: 1 };
    s.unison = Unison { voices: 3, detune_cents: 18.0, width: 0.9 };
    s.fx = Fx { chorus_depth: 0.5, chorus_mix: 0.35, reverb_size: 0.8, reverb_mix: 0.35 };
    s.output.glide_ms = 1.0;
    s.output.volume_db = -6.0;
    s
}

/// The "Deep Rave Bass" patch the help guide's recipe tab loads and then
/// walks through knob by knob: two saws an octave down, detuned against
/// each other for a slow reese-style beat (Sync off, or Osc 2 would lock
/// to Osc 1 and the beat would vanish), a loud sine sub, a dark driven
/// LP24 that a fast filter envelope pops open on each note, and Mono with
/// glide so overlapping notes slide instead of stacking up into mud.
pub fn deep_rave_bass() -> SynthState {
    SynthState {
        name: "Deep Rave Bass",
        voice_mode: VoiceMode::Mono,
        voices: 8,
        osc1: Oscillator {
            waveform: Waveform::Saw,
            octave: -1,
            knob_a_cents: 0.0,
            knob_b: 0.0,
            knob_c: 0.2,
            sync: false,
        },
        osc2: Oscillator {
            waveform: Waveform::Saw,
            octave: -1,
            knob_a_cents: 12.0,
            knob_b: 0.0,
            knob_c: 0.0,
            sync: false,
        },
        mix: Mix { osc1_db: -4.0, osc2_db: -5.0, sub_db: -3.0, noise_db: -60.0 },
        filter: Filter {
            filter_type: FilterType::Lp24,
            cutoff_hz: 220.0,
            resonance: 0.35,
            drive_db: 9.0,
            env_amount_oct: 3.0,
            key_track: 0.5,
        },
        filter_env: Envelope { attack_ms: 1.0, decay_ms: 220.0, sustain: 0.15, release_ms: 150.0 },
        amp_env: Envelope { attack_ms: 2.0, decay_ms: 400.0, sustain: 0.85, release_ms: 90.0 },
        lfo1: Lfo { rate_label: "", rate_norm: 0.2, depth: 0.05, sync: false, target: LfoTarget::Cutoff, target_count: 1 },
        lfo2: Lfo { rate_label: "", rate_norm: 0.3, depth: 0.0, sync: false, target: LfoTarget::Pitch, target_count: 1 },
        output: Output { glide_ms: 70.0, volume_db: -3.0, meter_l: 0.0, meter_r: 0.0 },
        // Bass stays mono and dry: width and reverb in the low end smear it.
        unison: Unison { voices: 1, detune_cents: 14.0, width: 0.0 },
        fx: Fx { chorus_depth: 0.4, chorus_mix: 0.0, reverb_size: 0.4, reverb_mix: 0.0 },
        held_notes: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutoff_mod_depth_matches_the_engines_sweep() {
        let mut s = seed_synth();
        s.lfo1.target = LfoTarget::Cutoff;
        s.lfo1.depth = 1.0;
        s.lfo2.target = LfoTarget::Pitch;
        // 100% depth sweeps +-2 octaves; the knob spans ~10 octaves, so the
        // ring should cover about a fifth of the knob either side.
        let depth = cutoff_mod_depth(&s);
        assert!((depth - 2.0 / (20_000f32 / 20.0).log2()).abs() < 1e-4, "{depth}");
    }
}
