//! The arrangement document: tracks, clips, automation lanes, loop range
//! and markers. All positions are [`Ticks`]. Mutated only through
//! [`super::commands::Command`] so every edit is undoable.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::peaks::PeakPyramid;
use super::time::{TempoMap, Ticks};

pub type TrackId = u32;
pub type ClipId = u32;
pub type AutomationLaneId = u32;
pub type MarkerId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    Audio,
    Midi,
}

/// One of the six `clip-*` design tokens. Also used as a track's colour
/// swatch, since a track's clips default to its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipColor {
    Coral,
    Amber,
    Teal,
    Blue,
    Violet,
    Pink,
}

/// A track lane's height in px: the timeline default (matches
/// `ui::timeline::LANE_HEIGHT`), and a resize handle's range either side.
pub const DEFAULT_TRACK_HEIGHT: f32 = 96.0;
pub const MIN_TRACK_HEIGHT: f32 = 40.0;
pub const MAX_TRACK_HEIGHT: f32 = 240.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub name: String,
    pub color: ClipColor,
    pub kind: TrackKind,
    pub mute: bool,
    pub solo: bool,
    pub arm: bool,
    pub gain_db: f32,
    /// Lane height in pixels; defaults to `tokens::SIZE_LANE`.
    pub height: f32,
    /// What a MIDI track plays through (its patch lives in the project's
    /// per-track instrument list). Always `None` on audio tracks.
    /// `default` so projects saved before instruments existed still load.
    #[serde(default)]
    pub instrument: Option<Instrument>,
    /// The track's insert effect chain - separate from `instrument`
    /// (an audio track has effects but no instrument; a MIDI track can
    /// have both). `default` so projects saved before effects existed
    /// still load.
    #[serde(default)]
    pub effects: Vec<Effect>,
}

/// A track's instrument. Only Carve so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Instrument {
    Carve,
}

impl Instrument {
    pub fn name(self) -> &'static str {
        match self {
            Instrument::Carve => "Carve",
        }
    }

    /// What a new track of `kind` starts with: MIDI tracks get Carve (a
    /// MIDI track with no instrument makes no sound), audio tracks nothing.
    pub fn default_for(kind: TrackKind) -> Option<Instrument> {
        match kind {
            TrackKind::Midi => Some(Instrument::Carve),
            TrackKind::Audio => None,
        }
    }
}

/// A track's insert effect. Only Compressor so far - a single-variant
/// enum, same convention as `Instrument`, so a second effect type is a
/// clean addition later rather than a reshape.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Effect {
    Compressor(CompressorState),
}

impl Effect {
    pub fn name(self) -> &'static str {
        match self {
            Effect::Compressor(_) => "Compressor",
        }
    }
}

/// A feedforward compressor's knobs. The DSP itself (the envelope
/// follower and its running state) lives in `engine` - this is just the
/// config, mirroring the `SynthState`/`SynthParams` split.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompressorState {
    pub threshold_db: f32,
    /// 1.0 = no compression, higher = harder. Not clamped at the top;
    /// a very high ratio is how a user gets limiter-like behavior.
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    /// Makeup gain, applied after gain reduction.
    pub makeup_db: f32,
}

impl Default for CompressorState {
    fn default() -> Self {
        Self { threshold_db: -18.0, ratio: 4.0, attack_ms: 10.0, release_ms: 150.0, makeup_db: 0.0 }
    }
}

impl CompressorState {
    /// Ratio 1.0 is mathematically a no-op (zero gain reduction
    /// regardless of threshold) - what every track without a Compressor
    /// in its `effects` list is treated as, so the engine can always run
    /// the same DSP unit rather than branching on `Option`.
    pub fn bypass() -> Self {
        Self { ratio: 1.0, ..Self::default() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MidiNote {
    pub start: Ticks,
    pub length: Ticks,
    pub pitch: u8,
    /// 1..=127, how hard the note is played (Carve maps it to level).
    /// `default` so projects saved before velocity existed still load.
    #[serde(default = "default_velocity")]
    pub velocity: u8,
}

/// The velocity every new note gets (drawn, step-entered or seeded).
pub const DEFAULT_VELOCITY: u8 = 100;

fn default_velocity() -> u8 {
    DEFAULT_VELOCITY
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClipContent {
    Audio {
        /// File name under the assets directory, e.g. `"drums.wav"`. Several
        /// clips (and tracks) commonly share one source; the peak pyramid is
        /// built once per unique source and shared via `Arc`.
        source: Arc<str>,
        /// Populated asynchronously once the background loader finishes
        /// decoding the source WAV and building the peak pyramid - never
        /// saved (rebuilt from the WAV file on load, same as on first
        /// reference today).
        #[serde(skip)]
        peaks: Option<Arc<PeakPyramid>>,
        /// Offset into the source audio, in samples, that this clip's
        /// `start` corresponds to. Trimming or splitting the left edge
        /// advances this so playback (and the displayed waveform slice)
        /// keeps referencing the right part of the source.
        source_offset_samples: u64,
    },
    Midi {
        notes: Vec<MidiNote>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clip {
    pub id: ClipId,
    pub track: TrackId,
    pub start: Ticks,
    pub length: Ticks,
    pub name: String,
    pub content: ClipContent,
    pub recording: bool,
}

impl Clip {
    pub fn end(&self) -> Ticks {
        self.start + self.length
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Breakpoint {
    pub tick: Ticks,
    /// Normalized 0..1.
    pub value: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomationLane {
    pub id: AutomationLaneId,
    pub track: TrackId,
    pub parameter_name: String,
    /// Formatted current value shown in the lane's header, e.g. "2.4 kHz".
    pub display_value: String,
    /// Sorted by tick.
    pub breakpoints: Vec<Breakpoint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopRange {
    pub start: Ticks,
    pub end: Ticks,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Marker {
    pub id: MarkerId,
    pub position: Ticks,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Arrangement {
    pub tempo_map: TempoMap,
    pub tracks: Vec<Track>,
    pub clips: Vec<Clip>,
    pub automation: Vec<AutomationLane>,
    pub loop_range: Option<LoopRange>,
    pub markers: Vec<Marker>,
    next_id: u32,
}

impl Arrangement {
    pub fn new(tempo_map: TempoMap) -> Self {
        Self {
            tempo_map,
            tracks: Vec::new(),
            clips: Vec::new(),
            automation: Vec::new(),
            loop_range: None,
            markers: Vec::new(),
            next_id: 1,
        }
    }

    /// Allocates a fresh id, unique within this arrangement (ids are never
    /// reused, even across undo/redo, so stale references can't collide).
    pub fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    pub fn clip(&self, id: ClipId) -> Option<&Clip> {
        self.clips.iter().find(|c| c.id == id)
    }

    pub fn clip_mut(&mut self, id: ClipId) -> Option<&mut Clip> {
        self.clips.iter_mut().find(|c| c.id == id)
    }

    pub fn clips_on_track(&self, track: TrackId) -> impl Iterator<Item = &Clip> {
        self.clips.iter().filter(move |c| c.track == track)
    }

    pub fn automation_lane(&self, id: AutomationLaneId) -> Option<&AutomationLane> {
        self.automation.iter().find(|a| a.id == id)
    }

    pub fn automation_lane_mut(&mut self, id: AutomationLaneId) -> Option<&mut AutomationLane> {
        self.automation.iter_mut().find(|a| a.id == id)
    }
}
