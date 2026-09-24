//! Carve, the subtractive synth console: parameter model and the pure
//! curve generators its displays draw from.

pub mod curves;
pub mod model;

pub use curves::{envelope_points, filter_response_points, waveform_points};
pub use model::{
    seed_synth, Envelope, Filter, FilterType, Lfo, Mix, Oscillator, Output, SynthState, VoiceMode,
    Waveform,
};
