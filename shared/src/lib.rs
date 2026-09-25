//! The engine <-> UI bridge.
//!
//! Two directions, two mechanisms:
//! - UI -> engine control parameters live in [`Params`], plain atomics read
//!   every audio callback and smoothed there to avoid zipper noise.
//! - engine -> UI telemetry ([`Telemetry`]) is pushed from the audio
//!   callback into a lock-free SPSC ring buffer ([`rtrb`]) and drained by a
//!   UI-side timer.
//!
//! Nothing here allocates, locks, or blocks once the bridge is built, so the
//! audio callback can use it directly.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub mod arrangement;
pub mod playback;
pub mod project;
pub mod recorder;
pub mod synth;
pub mod theory;

/// UI -> engine control parameters.
pub struct Params {
    /// Fader gain, linear 0.0..=1.0, stored as f32 bits.
    gain: AtomicU32,
    /// Pan, -1.0 (left) ..= 1.0 (right), stored as f32 bits.
    pan: AtomicU32,
    playing: AtomicBool,
    /// Edge-triggered: the UI sets this on Stop; the engine clears it after
    /// resetting its sample counter.
    stop_requested: AtomicBool,
    /// Whether the metronome click should sound while playing.
    click_enabled: AtomicBool,
}

impl Params {
    pub fn new() -> Self {
        Self {
            gain: AtomicU32::new(1.0f32.to_bits()),
            pan: AtomicU32::new(0.0f32.to_bits()),
            playing: AtomicBool::new(false),
            stop_requested: AtomicBool::new(false),
            click_enabled: AtomicBool::new(false),
        }
    }

    pub fn set_gain(&self, value: f32) {
        self.gain.store(value.to_bits(), Ordering::Relaxed);
    }

    pub fn gain(&self) -> f32 {
        f32::from_bits(self.gain.load(Ordering::Relaxed))
    }

    pub fn set_pan(&self, value: f32) {
        self.pan.store(value.to_bits(), Ordering::Relaxed);
    }

    pub fn pan(&self) -> f32 {
        f32::from_bits(self.pan.load(Ordering::Relaxed))
    }

    pub fn set_playing(&self, value: bool) {
        self.playing.store(value, Ordering::Relaxed);
    }

    pub fn playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub fn request_stop(&self) {
        self.playing.store(false, Ordering::Relaxed);
        self.stop_requested.store(true, Ordering::Relaxed);
    }

    /// Engine-side: consume the stop request, if any.
    pub fn take_stop_request(&self) -> bool {
        self.stop_requested.swap(false, Ordering::Relaxed)
    }

    pub fn set_click_enabled(&self, value: bool) {
        self.click_enabled.store(value, Ordering::Relaxed);
    }

    pub fn click_enabled(&self) -> bool {
        self.click_enabled.load(Ordering::Relaxed)
    }
}

impl Default for Params {
    fn default() -> Self {
        Self::new()
    }
}

/// A musical position: 1-indexed bar, beat and sixteenth, matching the
/// transport's "bar.beat.sixteenth" readout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Position {
    pub bar: u32,
    pub beat: u8,
    pub sixteenth: u8,
}

/// One telemetry snapshot, pushed from the audio callback into the ring
/// buffer roughly once per audio block.
#[derive(Clone, Copy, Debug, Default)]
pub struct Telemetry {
    pub peak_l: f32,
    pub peak_r: f32,
    pub position: Position,
}

/// Generous relative to a typical block size, so the producer never blocks
/// waiting for the UI to drain it.
pub const TELEMETRY_CAPACITY: usize = 512;

/// Builds the full bridge: shared control-parameter atomics plus the two
/// ends of the telemetry ring buffer.
pub fn bridge() -> (Arc<Params>, rtrb::Producer<Telemetry>, rtrb::Consumer<Telemetry>) {
    let params = Arc::new(Params::new());
    let (producer, consumer) = rtrb::RingBuffer::new(TELEMETRY_CAPACITY);
    (params, producer, consumer)
}
