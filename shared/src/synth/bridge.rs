//! The Carve engine <-> UI bridge, mirroring the shape of the top-level
//! transport bridge in `lib.rs`: control data flows UI -> engine as
//! best-effort ring-buffer snapshots (audio thread keeps only the latest),
//! note on/off flows UI -> engine as an ordered ring buffer (every event
//! matters here, not just the latest), and level metering flows back
//! engine -> UI the same way the transport's peak meter does.

use crate::arrangement::{CompressorState, EqState};

use super::{seed_synth, Envelope, Filter, Fx, LfoTarget, Mix, Oscillator, SynthState, Unison, VoiceMode};

/// How many effect units a single chain (one track's, or master's) can
/// carry across the UI -> engine bridge - generous relative to how many
/// effects a chain is likely to have, matching `MAX_INSTRUMENTS`'s own
/// "generous, not exact" sizing, and the `TrackHeaderFx` pip control's
/// own "up to 8, then +n" convention.
pub const MAX_EFFECTS_PER_CHAIN: usize = 8;

/// One effect unit's config, in the shape the engine actually runs -
/// same convention as `Effect`/`Instrument`.
#[derive(Clone, Copy, Debug)]
pub enum EffectUnitState {
    Compressor(CompressorState),
    Eq(EqState),
    Guitar(crate::guitar::GuitarFx),
}

/// A DSP-relevant snapshot of `SynthState`: everything the audio thread
/// needs to render a block, with the UI-only fields (name, held notes,
/// display labels, target counts) stripped out. Plain, `Copy`, allocation
/// free, so pushing one is real-time safe on the UI side and reading one
/// is real-time safe on the audio side.
/// How many Carve instances the engine keeps (one per instrument track),
/// all allocated before the audio stream starts.
pub const MAX_INSTRUMENTS: usize = 16;

#[derive(Clone, Copy, Debug)]
pub struct SynthParams {
    /// Which Carve instance (0..MAX_INSTRUMENTS) this snapshot is for.
    pub slot: u8,
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
    /// The owning track's mixer gain (`Track.gain_db`) - distinct from
    /// `volume_db` (Carve's own Output knob, part of the patch itself).
    /// Set by the caller after `from_state`, same as `slot`: this is
    /// arrangement state, not something a saved patch carries.
    pub gain_db: f32,
    /// The owning track's effect chain, in order - set by the caller
    /// same as `gain_db`. Only `effects[..effect_count]` is meaningful;
    /// the rest of the fixed array is unused padding (same "producer
    /// assigns a slot, consumer trusts it" convention `bus_slot` already
    /// uses elsewhere) - a plain fixed array, not a `Vec`, so this stays
    /// `Copy` and allocation-free across the ring buffer.
    pub effect_count: u8,
    pub effects: [EffectUnitState; MAX_EFFECTS_PER_CHAIN],
    /// This slot is a Drum Kit, not a Carve: notes trigger kit samples and
    /// the synth voice fields above are ignored. Set by the caller.
    pub drums: bool,
    /// A Drum Kit slot's per-pad settings (`Track::drum_pads`).
    pub drum_pads: [crate::drums::PadSettings; crate::drums::DRUM_KIT.len()],
}

impl SynthParams {
    pub fn from_state(s: &SynthState) -> Self {
        Self {
            slot: 0,
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
            gain_db: 0.0,
            effect_count: 0,
            effects: [EffectUnitState::Compressor(CompressorState::bypass()); MAX_EFFECTS_PER_CHAIN],
            drums: false,
            drum_pads: Default::default(),
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
    /// Which instrument slot plays it.
    pub slot: u8,
    pub note: u8,
    pub on: bool,
    /// 1..=127; sets the voice's level (ignored for note-offs).
    pub velocity: u8,
}

/// A `NoteEvent::note` meaning "release every voice in this slot" - sent
/// when a track loses its instrument, so nothing is left hanging.
pub const ALL_NOTES_OFF: u8 = 255;

/// One block's worth of Carve's own post-mix peak level, separate from the
/// transport's master peak meter.
#[derive(Clone, Copy, Debug, Default)]
pub struct SynthTelemetry {
    /// Each instance's post-mix peak (L, R) over the block.
    pub peaks: [(f32, f32); MAX_INSTRUMENTS],
    /// Where each instance's two LFOs are in their cycles (0..1) at the end
    /// of the block, so the scopes show the real LFOs.
    pub lfo_phases: [(f32, f32); MAX_INSTRUMENTS],
}

pub const SYNTH_PARAMS_CAPACITY: usize = 1024;
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
