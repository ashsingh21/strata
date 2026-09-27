//! Carve, the subtractive synth console: parameter model and the pure
//! curve generators its displays draw from.

pub mod bridge;
pub mod curves;
pub mod model;
pub mod params;

pub use bridge::{
    lfo_rate_hz, synth_bridge, EffectUnitState, NoteEvent, ALL_NOTES_OFF, MAX_EFFECTS_PER_CHAIN, MAX_INSTRUMENTS,
    SynthBridge, SynthParams, SynthTelemetry, NOTE_EVENT_CAPACITY, SYNTH_PARAMS_CAPACITY, SYNTH_TELEMETRY_CAPACITY,
};
pub use params::SynthParam;
pub use curves::{envelope_points, filter_response_points, waveform_points};
pub use model::{
    cutoff_mod_depth, deep_rave_bass, lfo_mod_depth, seed_synth, Envelope, Filter, FilterType, Fx, Lfo, LfoTarget, Mix,
    Oscillator, Output, SynthState, Unison, VoiceMode, Waveform, LFO_CUTOFF_MAX_OCT, LFO_PITCH_MAX_CENTS,
    LFO_PULSE_WIDTH_MAX, LFO_RESONANCE_MAX, MAX_UNISON, PRESETS,
};
