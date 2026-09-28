//! Pure music-theory helpers for the interval-input tool: scale/degree
//! naming and chord recognition. No UI, no audio - just data, so it's
//! cheap to unit test.

pub mod chord;
pub mod scale;

pub use chord::{recognize, ChordMatch};
pub use scale::{
    degree_name, degrees_in_mask, gaps_in_mask, note_name, note_name_for_key, sargam_name, scale_step, ScalePreset,
    SCALE_PRESETS,
};
