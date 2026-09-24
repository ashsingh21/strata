//! The arrangement/timeline document: musical time, tracks/clips/automation,
//! undo-able commands, waveform peaks and the view-space transform. Pure
//! logic only - no Vizia, no audio I/O - so `cargo test -p shared` covers
//! all of it without pulling in the GUI or engine stack.

pub mod commands;
pub mod model;
pub mod peaks;
pub mod seed;
pub mod time;
pub mod transform;

pub use commands::{Command, CommandStack};
pub use model::{
    Arrangement, AutomationLane, AutomationLaneId, Breakpoint, Clip, ClipColor, ClipContent,
    ClipId, LoopRange, Marker, MarkerId, MidiNote, Track, TrackId, TrackKind,
};
pub use peaks::{PeakLevel, PeakPyramid};
pub use seed::seed_arrangement;
pub use time::{position_to_ticks, TempoEvent, TempoMap, TimeSignature, Ticks, PPQ};
pub use transform::{snap, SnapGrid, ViewTransform, MAX_PIXELS_PER_BEAT, MIN_PIXELS_PER_BEAT};
