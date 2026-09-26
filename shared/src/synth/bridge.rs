//! The Carve engine <-> UI bridge, mirroring the shape of the top-level
//! transport bridge in `lib.rs`: control data flows UI -> engine as
//! best-effort ring-buffer snapshots (audio thread keeps only the latest),
//! note on/off flows UI -> engine as an ordered ring buffer (every event
//! matters here, not just the latest), and level metering flows back
//! engine -> UI the same way the transport's peak meter does.

use super::{seed_synth, Envelope, Filter, Fx, LfoTarget, Mix, Oscillator, SynthState, Unison, VoiceMode};

/// A DSP-relevant snapshot of `SynthState`: everything the audio thread
/// needs to render a block, with the UI-only fields (name, held notes,
/// display labels, target counts) stripped out. Plain, `Copy`, allocation
/// free, so pushing one is real-time safe on the UI side and reading one
/// is real-time safe on the audio side.
#[derive(Clone, Copy, Debug)]
pub struct SynthParams {
    pub voice_mode: VoiceMode,
    pub max_voices: u8,
    pub osc1: Oscillator,
    pub osc2: Oscillator,
    pub mix: Mix,
    pub filter: Filter,
    pub filter_env: Envelope,
    pub amp_env: Envelope,
    pub lfo1_rate_hz: f32,
    pub lfo1_depth: f32,
    pub lfo1_target: LfoTarget,
    pub lfo2_rate_hz: f32,
    pub lfo2_depth: f32,
    pub lfo2_target: LfoTarget,
    pub glide_ms: f32,
    pub volume_db: f32,
    pub unison: Unison,
    pub fx: Fx,
}

impl SynthParams {
    pub fn from_state(s: &SynthState) -> Self {
        Self {
            voice_mode: s.voice_mode,
            max_voices: s.voices,
            osc1: s.osc1,
            osc2: s.osc2,
            mix: s.mix,
            filter: s.filter,
            filter_env: s.filter_env,
            amp_env: s.amp_env,
            lfo1_rate_hz: lfo_rate_hz(s.lfo1.rate_norm),
            lfo1_depth: s.lfo1.depth,
            lfo1_target: s.lfo1.target,
            lfo2_rate_hz: lfo_rate_hz(s.lfo2.rate_norm),
            lfo2_depth: s.lfo2.depth,
            lfo2_target: s.lfo2.target,
            glide_ms: s.output.glide_ms,
            volume_db: s.output.volume_db,
            unison: s.unison,
            fx: s.fx,
        }
    }
}

impl Default for SynthParams {
    fn default() -> Self {
        Self::from_state(&seed_synth())
    }
}

/// Maps the Rate knob's normalized 0..1 position to a free-running LFO
/// rate in Hz (0.05..20 Hz, log taper) - independent of the display's
/// tempo-synced label text.
pub fn lfo_rate_hz(rate_norm: f32) -> f32 {
    const MIN_HZ: f32 = 0.05;
    const MAX_HZ: f32 = 20.0;
    let n = rate_norm.clamp(0.0, 1.0);
    (MIN_HZ.ln() + n * (MAX_HZ.ln() - MIN_HZ.ln())).exp()
}

/// A single note on/off, in MIDI note-number terms. Pushed UI -> engine in
/// order; unlike `SynthParams`, every one of these matters, so the engine
/// drains and applies all pending events rather than just the latest.
#[derive(Clone, Copy, Debug)]
pub struct NoteEvent {
    pub note: u8,
    pub on: bool,
    /// 1..=127; sets the voice's level (ignored for note-offs).
    pub velocity: u8,
}

/// One block's worth of Carve's own post-mix peak level, separate from the
/// transport's master peak meter.
#[derive(Clone, Copy, Debug, Default)]
pub struct SynthTelemetry {
    pub peak_l: f32,
    pub peak_r: f32,
    /// Where each LFO is in its cycle (0..1) at the end of the block, so
    /// the scopes show the real LFOs rather than an animation of their own.
    pub lfo1_phase: f32,
    pub lfo2_phase: f32,
}

pub const SYNTH_PARAMS_CAPACITY: usize = 64;
pub const NOTE_EVENT_CAPACITY: usize = 256;
pub const SYNTH_TELEMETRY_CAPACITY: usize = 512;

pub struct SynthBridge {
    pub params_tx: rtrb::Producer<SynthParams>,
    pub params_rx: rtrb::Consumer<SynthParams>,
    pub note_tx: rtrb::Producer<NoteEvent>,
    pub note_rx: rtrb::Consumer<NoteEvent>,
    pub telemetry_tx: rtrb::Producer<SynthTelemetry>,
    pub telemetry_rx: rtrb::Consumer<SynthTelemetry>,
}

pub fn synth_bridge() -> SynthBridge {
    let (params_tx, params_rx) = rtrb::RingBuffer::new(SYNTH_PARAMS_CAPACITY);
    let (note_tx, note_rx) = rtrb::RingBuffer::new(NOTE_EVENT_CAPACITY);
    let (telemetry_tx, telemetry_rx) = rtrb::RingBuffer::new(SYNTH_TELEMETRY_CAPACITY);
    SynthBridge { params_tx, params_rx, note_tx, note_rx, telemetry_tx, telemetry_rx }
}
