//! Carve, the subtractive synth console: parameter model and the pure
//! curve generators its displays draw from.

pub mod bridge;
pub mod curves;
pub mod model;

pub use bridge::{
    lfo_rate_hz, synth_bridge, NoteEvent, SynthBridge, SynthParams, SynthTelemetry,
    NOTE_EVENT_CAPACITY, SYNTH_PARAMS_CAPACITY, SYNTH_TELEMETRY_CAPACITY,
};
pub use curves::{envelope_points, filter_response_points, waveform_points};
pub use model::{
    cutoff_mod_depth, seed_synth, Envelope, Filter, FilterType, Lfo, LfoTarget, Mix, Oscillator,
    Output, SynthState, VoiceMode, Waveform,
};
