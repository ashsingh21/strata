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
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// UI -> engine input gain, applied in `input`'s capture callback before
/// samples hit the ring buffer - plain atomics shared via `Arc`, same
/// style as `Params` in `lib.rs`, so gain can be staged in real time
/// without going through a ring buffer.
pub struct RecordParams {
    input_gain_db: AtomicU32,
    monitoring: AtomicBool,
    /// Riyaz is listening: the input's samples go to the voice ring too.
    listening: AtomicBool,
    /// The open input's sample rate (0: none open).
    input_rate: AtomicU32,
}

impl RecordParams {
    pub fn new() -> Self {
        Self {
            input_gain_db: AtomicU32::new(0.0f32.to_bits()),
            monitoring: AtomicBool::new(false),
            listening: AtomicBool::new(false),
            input_rate: AtomicU32::new(0),
        }
    }

    pub fn set_listening(&self, on: bool) {
        self.listening.store(on, Ordering::Relaxed);
    }

    pub fn listening(&self) -> bool {
        self.listening.load(Ordering::Relaxed)
    }

    pub fn set_input_rate(&self, rate: u32) {
        self.input_rate.store(rate, Ordering::Relaxed);
    }

    pub fn input_rate(&self) -> u32 {
        self.input_rate.load(Ordering::Relaxed)
    }

    /// Whether the input is heard live, through the armed track's effects.
    pub fn set_monitoring(&self, on: bool) {
        self.monitoring.store(on, Ordering::Relaxed);
    }

    pub fn monitoring(&self) -> bool {
        self.monitoring.load(Ordering::Relaxed)
    }

    pub fn set_input_gain_db(&self, db: f32) {
        self.input_gain_db.store(db.to_bits(), Ordering::Relaxed);
    }

    pub fn input_gain_db(&self) -> f32 {
        f32::from_bits(self.input_gain_db.load(Ordering::Relaxed))
    }
}

impl Default for RecordParams {
    fn default() -> Self {
        Self::new()
    }
}

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
/// Two seconds at 48 kHz: the UI drains it every frame.
pub const VOICE_CAPACITY: usize = 96_000;

pub struct RecorderBridge {
    pub command_tx: rtrb::Producer<RecordCommand>,
    pub command_rx: rtrb::Consumer<RecordCommand>,
    pub telemetry_tx: rtrb::Producer<InputTelemetry>,
    pub telemetry_rx: rtrb::Consumer<InputTelemetry>,
    /// The input's samples while Riyaz listens, for its pitch tracker.
    pub voice_tx: rtrb::Producer<f32>,
    pub voice_rx: rtrb::Consumer<f32>,
}

pub fn recorder_bridge() -> RecorderBridge {
    let (command_tx, command_rx) = rtrb::RingBuffer::new(RECORD_COMMAND_CAPACITY);
    let (telemetry_tx, telemetry_rx) = rtrb::RingBuffer::new(INPUT_TELEMETRY_CAPACITY);
    let (voice_tx, voice_rx) = rtrb::RingBuffer::new(VOICE_CAPACITY);
    RecorderBridge { command_tx, command_rx, telemetry_tx, telemetry_rx, voice_tx, voice_rx }
}
