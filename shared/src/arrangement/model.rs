//! The arrangement document: tracks, clips, automation lanes, loop range
//! and markers. All positions are [`Ticks`]. Mutated only through
//! [`super::commands::Command`] so every edit is undoable.

use std::sync::Arc;

use super::peaks::PeakPyramid;
use super::time::{TempoMap, Ticks};

pub type TrackId = u32;
pub type ClipId = u32;
pub type AutomationLaneId = u32;
pub type MarkerId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackKind {
    Audio,
    Midi,
}

/// One of the six `clip-*` design tokens. Also used as a track's colour
/// swatch, since a track's clips default to its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipColor {
    Coral,
    Amber,
    Teal,
    Blue,
    Violet,
    Pink,
}

#[derive(Clone, Debug)]
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
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MidiNote {
    pub start: Ticks,
    pub length: Ticks,
    pub pitch: u8,
}

#[derive(Clone, Debug)]
pub enum ClipContent {
    Audio {
        /// File name under the assets directory, e.g. `"drums.wav"`. Several
        /// clips (and tracks) commonly share one source; the peak pyramid is
        /// built once per unique source and shared via `Arc`.
        source: Arc<str>,
        /// Populated asynchronously once the background loader finishes
        /// decoding the source WAV and building the peak pyramid.
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

#[derive(Clone, Debug)]
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Breakpoint {
    pub tick: Ticks,
    /// Normalized 0..1.
    pub value: f32,
}

#[derive(Clone, Debug)]
pub struct AutomationLane {
    pub id: AutomationLaneId,
    pub track: TrackId,
    pub parameter_name: String,
    /// Formatted current value shown in the lane's header, e.g. "2.4 kHz".
    pub display_value: String,
    /// Sorted by tick.
    pub breakpoints: Vec<Breakpoint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoopRange {
    pub start: Ticks,
    pub end: Ticks,
}

#[derive(Clone, Debug)]
pub struct Marker {
    pub id: MarkerId,
    pub position: Ticks,
    pub name: String,
}

#[derive(Clone, Debug)]
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
