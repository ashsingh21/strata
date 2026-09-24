//! The guitar/mic recording bridge: UI -> engine control (arm/stop a
//! take) and engine -> UI input-level telemetry, mirroring the shape of
//! the other bridges in this crate (see `lib.rs`'s `Params`/`Telemetry`,
//! `synth::bridge`, `playback`).
//!
//! Deliberately minimal: there's no "recording finished, here's the exact
//! sample count" message back from the engine. The UI already knows
//! exactly when it sent `Stop` and drives everything else (the playhead,
//! the live preview clip's length) from elapsed ticks against the same
//! tempo map, so it can compute the finished clip's length the same way
//! rather than needing the engine to report it.

use std::path::PathBuf;

/// Sent UI -> engine's input writer thread. Ordered - every command
/// matters, unlike the "latest wins" control snapshots elsewhere.
#[derive(Clone, Debug)]
pub enum RecordCommand {
    /// Start streaming captured input to a new WAV file at `path`,
    /// finalizing whatever was previously being written first.
    Start { path: PathBuf },
    /// Finalize whatever WAV is currently being written, if any.
    Stop,
}

/// One block's worth of the input device's peak level, for the UI's
/// input meter - reported continuously, independent of whether anything
/// is currently being recorded, so gain can be staged before arming.
#[derive(Clone, Copy, Debug, Default)]
pub struct InputTelemetry {
    pub peak: f32,
}

pub const RECORD_COMMAND_CAPACITY: usize = 16;
/// Generous relative to a typical block size, so the producer never
/// blocks waiting for the UI to drain it.
pub const INPUT_TELEMETRY_CAPACITY: usize = 512;

pub struct RecorderBridge {
    pub command_tx: rtrb::Producer<RecordCommand>,
    pub command_rx: rtrb::Consumer<RecordCommand>,
    pub telemetry_tx: rtrb::Producer<InputTelemetry>,
    pub telemetry_rx: rtrb::Consumer<InputTelemetry>,
}

pub fn recorder_bridge() -> RecorderBridge {
    let (command_tx, command_rx) = rtrb::RingBuffer::new(RECORD_COMMAND_CAPACITY);
    let (telemetry_tx, telemetry_rx) = rtrb::RingBuffer::new(INPUT_TELEMETRY_CAPACITY);
    RecorderBridge { command_tx, command_rx, telemetry_tx, telemetry_rx }
}
