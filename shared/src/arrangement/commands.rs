//! Every arrangement edit goes through a [`Command`]. Applying a command
//! returns the command that undoes it, built from the arrangement's actual
//! prior state (not guessed), so [`CommandStack`] can implement undo/redo as
//! a plain stack of inverses.

use super::model::{
    Arrangement, AutomationLane, AutomationLaneId, Breakpoint, Clip, ClipContent, ClipId, Effect,
    EffectEdge, EffectNode, EffectNodeId, Instrument, LoopRange, Marker, MarkerId, MidiNote, Track, TrackId,
};
#[cfg(test)]
use super::model::DEFAULT_VELOCITY;
use super::time::{TempoMap, Ticks, TimeSignature};

#[derive(Clone, Debug)]
pub enum Command {
    /// Applies several commands in order; its inverse is their inverses in
    /// reverse order. Used for edits that touch more than one thing (split,
    /// duplicate-with-select, ...).
    Batch(Vec<Command>),
    MoveClip { clip: ClipId, track: TrackId, start: Ticks },
    TrimClip { clip: ClipId, start: Ticks, length: Ticks },
    /// Splits `clip` at tick `at`; the new (right-hand) clip gets `new_id`.
    SplitClip { clip: ClipId, at: Ticks, new_id: ClipId },
    InsertClip { clip: Box<Clip> },
    DeleteClip { clip: ClipId },
    DuplicateClip { clip: ClipId, new_id: ClipId, offset: Ticks },
    /// Swaps in a whole new version of an existing clip (same id, same
    /// place in the list); the inverse swaps the old one back. For edits
    /// that change several fields at once, like turning a clip into a loop.
    ReplaceClip { clip: Box<Clip> },
    AddMidiNote { clip: ClipId, note: MidiNote },
    RemoveMidiNote { clip: ClipId, start: Ticks, pitch: u8 },
    /// Sets (or, with `None`, removes) a track's instrument.
    SetInstrument { track: TrackId, instrument: Option<Instrument> },
    /// Appends a new effect node at the end of a track's chain.
    /// `position`, when given, overrides the auto-layout position
    /// `EffectGraph::push_at_end` would otherwise pick - used when the
    /// node's coming from an explicit drop point (dragged from the
    /// palette), not a generic "+Effect" add.
    AddEffectNode { track: Option<TrackId>, effect: Effect, position: Option<(f32, f32)> },
    /// Removes an effect node, reconnecting its neighbours - inverse
    /// carries the exact removed node and its two edges so undo restores
    /// precisely where it was, not just "a node with this effect".
    RemoveEffectNode { track: Option<TrackId>, node: EffectNodeId },
    /// The literal inverse of `RemoveEffectNode` - never emitted directly
    /// by UI code, only produced as another command's undo.
    ReinsertEffectNode { track: Option<TrackId>, node: EffectNode, inbound: EffectEdge, outbound: EffectEdge },
    SetEffectEnabled { track: Option<TrackId>, node: EffectNodeId, enabled: bool },
    /// Cosmetic (canvas position only) but still undoable, same as any
    /// other edit here.
    SetEffectNodePosition { track: Option<TrackId>, node: EffectNodeId, position: (f32, f32) },
    /// The board's port-drag rewire: moves `node` to just before
    /// `before` in the chain (see `EffectGraph::move_before`'s own doc
    /// comment for why that's the right primitive while the graph stays
    /// linear). Inverse moves it back to just before its own
    /// pre-rewire successor, which restores the exact prior adjacency.
    RewireEffect { track: Option<TrackId>, node: EffectNodeId, before: EffectNodeId },
    /// Adds a parallel connection without removing any existing one
    /// (fan-out/fan-in) - a no-op (see `EffectGraph::connect`'s own doc
    /// comment) if it would create a cycle or the edge already exists.
    /// Not wired to any UI gesture yet - see the effects-board plan's
    /// own Phase 11 notes on why engine execution has to land first.
    ConnectEffect { track: Option<TrackId>, from: EffectNodeId, to: EffectNodeId },
    DisconnectEffect { track: Option<TrackId>, from: EffectNodeId, to: EffectNodeId },
    /// Sets the velocity of the note at (`start`, `pitch`).
    SetNoteVelocity { clip: ClipId, start: Ticks, pitch: u8, velocity: u8 },
    /// Inserts `lane` at `index` in the lane list (clamped) - a new lane,
    /// or `RemoveAutomationLane`'s inverse restoring it in place.
    InsertAutomationLane { lane: Box<AutomationLane>, index: usize },
    RemoveAutomationLane { lane: AutomationLaneId },
    AddBreakpoint { lane: AutomationLaneId, point: Breakpoint },
    RemoveBreakpoint { lane: AutomationLaneId, tick: Ticks },
    MoveBreakpoint { lane: AutomationLaneId, tick: Ticks, new_tick: Ticks, new_value: f32 },
    SetLoopRange { range: Option<LoopRange> },
    InsertMarker { marker: Marker },
    RemoveMarker { marker: MarkerId },
    RenameMarker { marker: MarkerId, name: String },
    RenameTrack { track: TrackId, name: String },
    /// Inserts `track` at `index` in the track list, along with any
    /// `clips`/`automation` it should already own - used both for a
    /// fresh "add track" (both empty) and as `DeleteTrack`'s inverse
    /// (restoring everything that was on it).
    InsertTrack { track: Box<Track>, index: usize, clips: Vec<Clip>, automation: Vec<AutomationLane> },
    /// Removes `track` and cascades to every clip and automation lane on
    /// it, so nothing is left pointing at a track that no longer exists.
    DeleteTrack { track: TrackId },
    /// Replaces the whole tempo map with a single constant tempo at
    /// `bpm`, keeping the current time signature. Collapses any future
    /// mid-song tempo changes back to one value - there's no UI for
    /// tempo automation yet, so that's exactly what every caller today
    /// already has anyway.
    SetTempo { bpm: f64 },
    /// Same "replace the whole tempo map, keeping the other half"
    /// shape as `SetTempo` (and the same reasoning: no UI for
    /// per-position time signature changes yet, so every caller already
    /// treats it as one global value).
    SetTimeSignature { numerator: u8, denominator: u8 },
    /// Swaps in a whole arrangement - for an edit made elsewhere as a
    /// finished result (a lesson's "Show me"), undone in one step.
    Replace(Box<Arrangement>),
}

impl Command {
    /// Applies this command to `arr` and returns its inverse.
    pub fn apply(self, arr: &mut Arrangement) -> Command {
        // A note edit to a linked clip is made in every clip it's linked
        // to. Syncing after the edit (and after its undo, which is itself
        // one of these commands) keeps the whole group identical.
        let edited_notes = match &self {
            Command::AddMidiNote { clip, .. } | Command::RemoveMidiNote { clip, .. } | Command::SetNoteVelocity { clip, .. } => {
                Some(*clip)
            }
            Command::ReplaceClip { clip } => Some(clip.id),
            _ => None,
        };
        let inverse = self.apply_one(arr);
        if let Some(clip) = edited_notes {
            arr.sync_links(clip);
        }
        inverse
    }

    fn apply_one(self, arr: &mut Arrangement) -> Command {
        match self {
            Command::Replace(mut next) => {
                std::mem::swap(arr, &mut next);
                Command::Replace(next)
            }

            Command::Batch(cmds) => {
                // Applied in the given order (later commands may depend on
                // earlier ones, e.g. referencing a clip an earlier InsertClip
                // just created) - only the returned inverse list is
                // reversed, so undo replays them back-to-front.
                let mut inverses: Vec<Command> = cmds.into_iter().map(|c| c.apply(arr)).collect();
                inverses.reverse();
                Command::Batch(inverses)
            }

            Command::MoveClip { clip: clip_id, track, start } => {
                let clip = arr.clip_mut(clip_id).expect("MoveClip: unknown clip");
                let inverse =
                    Command::MoveClip { clip: clip_id, track: clip.track, start: clip.start };
                clip.track = track;
                clip.start = start;
                inverse
            }

            Command::TrimClip { clip: clip_id, start, length } => {
                let sample_rate = 48_000u32; // peak-pyramid indexing rate; see note below.
                let old_start;
                let old_length;
                let delta_ticks;
                {
                    let clip = arr.clip_mut(clip_id).expect("TrimClip: unknown clip");
                    old_start = clip.start;
                    old_length = clip.length;
                    delta_ticks = start - old_start;
                    clip.start = start;
                    clip.length = length;
                }
                let bpm = arr.tempo_map.bpm_at(start);
                if let Some(clip) = arr.clip_mut(clip_id) {
                    if let ClipContent::Audio { source_offset_samples, .. } = &mut clip.content {
                        let delta_samples = ticks_to_samples_at(delta_ticks, bpm, sample_rate);
                        *source_offset_samples =
                            (*source_offset_samples as i64 + delta_samples).max(0) as u64;
                    }
                }
                Command::TrimClip { clip: clip_id, start: old_start, length: old_length }
            }

            Command::SplitClip { clip: clip_id, at, new_id } => {
                let original = arr.clip(clip_id).expect("SplitClip: unknown clip").clone();
                assert!(at > original.start && at < original.end(), "split point outside clip");

                let right_start = at;
                let right_length = original.end() - at;
                let left_length = at - original.start;

                let right_content = match &original.content {
                    ClipContent::Audio { source, peaks, source_offset_samples } => {
                        let bpm = arr.tempo_map.bpm_at(original.start);
                        let sample_rate = 48_000u32;
                        let delta_samples =
                            ticks_to_samples_at(left_length, bpm, sample_rate) as u64;
                        ClipContent::Audio {
                            source: source.clone(),
                            peaks: peaks.clone(),
                            source_offset_samples: source_offset_samples + delta_samples,
                        }
                    }
                    // Split what's heard (a looping clip is unrolled), so
                    // both halves play exactly what the original did.
                    ClipContent::Midi { .. } => ClipContent::Midi {
                        notes: original
                            .played_notes()
                            .into_iter()
                            .filter(|n| n.start >= left_length)
                            .map(|n| MidiNote { start: n.start - left_length, ..n })
                            .collect(),
                        loop_len: None,
                        link: None,
                    },
                };
                let left_notes: Option<Vec<MidiNote>> = matches!(original.content, ClipContent::Midi { .. })
                    .then(|| original.played_notes().into_iter().filter(|n| n.start < left_length).collect());

                let right_clip = Clip {
                    id: new_id,
                    track: original.track,
                    start: right_start,
                    length: right_length,
                    name: original.name.clone(),
                    content: right_content,
                    recording: false,
                    gain_db: original.gain_db,
                };

                let clip = arr.clip_mut(clip_id).unwrap();
                clip.length = left_length;
                if let (Some(notes), ClipContent::Midi { notes: dst, loop_len, link }) =
                    (left_notes, &mut clip.content)
                {
                    *dst = notes;
                    *loop_len = None;
                    // Its notes no longer match the rest of its group.
                    *link = None;
                }
                arr.clips.push(right_clip);

                // The whole original back, notes and all (a length-only
                // undo lost the notes that went to the right half).
                Command::Batch(vec![Command::DeleteClip { clip: new_id }, Command::ReplaceClip { clip: Box::new(original) }])
            }

            Command::ReplaceClip { clip } => {
                let slot = arr.clip_mut(clip.id).expect("ReplaceClip: unknown clip");
                let old = std::mem::replace(slot, *clip);
                Command::ReplaceClip { clip: Box::new(old) }
            }

            Command::InsertClip { clip } => {
                let id = clip.id;
                arr.clips.push(*clip);
                Command::DeleteClip { clip: id }
            }

            Command::DeleteClip { clip: clip_id } => {
                let index = arr.clips.iter().position(|c| c.id == clip_id).expect("DeleteClip: unknown clip");
                let removed = arr.clips.remove(index);
                Command::InsertClip { clip: Box::new(removed) }
            }

            Command::InsertTrack { track, index, clips, automation } => {
                let id = track.id;
                let index = index.min(arr.tracks.len());
                arr.tracks.insert(index, *track);
                arr.clips.extend(clips);
                arr.automation.extend(automation);
                Command::DeleteTrack { track: id }
            }

            Command::DeleteTrack { track: track_id } => {
                let index = arr.tracks.iter().position(|t| t.id == track_id).expect("DeleteTrack: unknown track");
                let track = arr.tracks.remove(index);
                let mut clips = Vec::new();
                arr.clips.retain(|c| {
                    let keep = c.track != track_id;
                    if !keep {
                        clips.push(c.clone());
                    }
                    keep
                });
                let mut automation = Vec::new();
                arr.automation.retain(|a| {
                    let keep = a.track != track_id;
                    if !keep {
                        automation.push(a.clone());
                    }
                    keep
                });
                Command::InsertTrack { track: Box::new(track), index, clips, automation }
            }

            Command::DuplicateClip { clip: clip_id, new_id, offset } => {
                let mut copy = arr.clip(clip_id).expect("DuplicateClip: unknown clip").clone();
                copy.id = new_id;
                copy.start += offset;
                copy.recording = false;
                // A plain duplicate is independent (see "Duplicate linked").
                if let ClipContent::Midi { link, .. } = &mut copy.content {
                    *link = None;
                }
                arr.clips.push(copy);
                Command::DeleteClip { clip: new_id }
            }

            Command::AddMidiNote { clip: clip_id, note } => {
                let clip = arr.clip_mut(clip_id).expect("AddMidiNote: unknown clip");
                match &mut clip.content {
                    ClipContent::Midi { notes, .. } => notes.push(note),
                    ClipContent::Audio { .. } => panic!("AddMidiNote: clip is not a MIDI clip"),
                }
                Command::RemoveMidiNote { clip: clip_id, start: note.start, pitch: note.pitch }
            }

            Command::RemoveMidiNote { clip: clip_id, start, pitch } => {
                let clip = arr.clip_mut(clip_id).expect("RemoveMidiNote: unknown clip");
                let notes = match &mut clip.content {
                    ClipContent::Midi { notes, .. } => notes,
                    ClipContent::Audio { .. } => panic!("RemoveMidiNote: clip is not a MIDI clip"),
                };
                let index = notes
                    .iter()
                    .position(|n| n.start == start && n.pitch == pitch)
                    .expect("RemoveMidiNote: no matching note");
                let removed = notes.remove(index);
                Command::AddMidiNote { clip: clip_id, note: removed }
            }

            Command::SetInstrument { track, instrument } => {
                let t = arr.track_mut(track).expect("SetInstrument: unknown track");
                let previous = std::mem::replace(&mut t.instrument, instrument);
                Command::SetInstrument { track, instrument: previous }
            }

            Command::AddEffectNode { track, effect, position } => {
                let fx = arr.fx_mut(track).expect("AddEffectNode: unknown track");
                let node = fx.push_at_end(effect);
                if let Some(pos) = position {
                    fx.set_position(node, pos);
                }
                Command::RemoveEffectNode { track, node }
            }

            Command::RemoveEffectNode { track, node } => {
                let fx = arr.fx_mut(track).expect("RemoveEffectNode: unknown track");
                let (node, inbound, outbound) = fx.remove(node).expect("RemoveEffectNode: unknown node");
                Command::ReinsertEffectNode { track, node, inbound, outbound }
            }

            Command::ReinsertEffectNode { track, node, inbound, outbound } => {
                let fx = arr.fx_mut(track).expect("ReinsertEffectNode: unknown track");
                let id = node.id;
                fx.reinsert(node, inbound, outbound);
                Command::RemoveEffectNode { track, node: id }
            }

            Command::SetEffectEnabled { track, node, enabled } => {
                let fx = arr.fx_mut(track).expect("SetEffectEnabled: unknown track");
                let previous = fx.set_enabled(node, enabled);
                Command::SetEffectEnabled { track, node, enabled: previous }
            }

            Command::SetEffectNodePosition { track, node, position } => {
                let fx = arr.fx_mut(track).expect("SetEffectNodePosition: unknown track");
                let previous = fx.set_position(node, position);
                Command::SetEffectNodePosition { track, node, position: previous }
            }

            Command::RewireEffect { track, node, before } => {
                let fx = arr.fx_mut(track).expect("RewireEffect: unknown track");
                let old_before = fx.successor_of(node).expect("RewireEffect: node has no successor");
                fx.move_before(node, before);
                Command::RewireEffect { track, node, before: old_before }
            }

            Command::ConnectEffect { track, from, to } => {
                let fx = arr.fx_mut(track).expect("ConnectEffect: unknown track");
                fx.connect(from, to);
                Command::DisconnectEffect { track, from, to }
            }

            Command::DisconnectEffect { track, from, to } => {
                let fx = arr.fx_mut(track).expect("DisconnectEffect: unknown track");
                fx.disconnect(from, to);
                Command::ConnectEffect { track, from, to }
            }

            Command::SetNoteVelocity { clip: clip_id, start, pitch, velocity } => {
                let clip = arr.clip_mut(clip_id).expect("SetNoteVelocity: unknown clip");
                let ClipContent::Midi { notes, .. } = &mut clip.content else {
                    panic!("SetNoteVelocity: clip is not a MIDI clip")
                };
                let note = notes
                    .iter_mut()
                    .find(|n| n.start == start && n.pitch == pitch)
                    .expect("SetNoteVelocity: no matching note");
                let previous = note.velocity;
                note.velocity = velocity.clamp(1, 127);
                Command::SetNoteVelocity { clip: clip_id, start, pitch, velocity: previous }
            }

            Command::InsertAutomationLane { lane, index } => {
                let id = lane.id;
                let index = index.min(arr.automation.len());
                arr.automation.insert(index, *lane);
                Command::RemoveAutomationLane { lane: id }
            }

            Command::RemoveAutomationLane { lane } => {
                let index = arr
                    .automation
                    .iter()
                    .position(|l| l.id == lane)
                    .expect("RemoveAutomationLane: unknown lane");
                let removed = arr.automation.remove(index);
                Command::InsertAutomationLane { lane: Box::new(removed), index }
            }

            Command::AddBreakpoint { lane, point } => {
                let lane = arr.automation_lane_mut(lane).expect("AddBreakpoint: unknown lane");
                insert_breakpoint_sorted(&mut lane.breakpoints, point);
                Command::RemoveBreakpoint { lane: lane.id, tick: point.tick }
            }

            Command::RemoveBreakpoint { lane, tick } => {
                let lane_mut = arr.automation_lane_mut(lane).expect("RemoveBreakpoint: unknown lane");
                let index = lane_mut
                    .breakpoints
                    .iter()
                    .position(|b| b.tick == tick)
                    .expect("RemoveBreakpoint: no breakpoint at tick");
                let removed = lane_mut.breakpoints.remove(index);
                Command::AddBreakpoint { lane, point: removed }
            }

            Command::MoveBreakpoint { lane, tick, new_tick, new_value } => {
                let lane_mut = arr.automation_lane_mut(lane).expect("MoveBreakpoint: unknown lane");
                let index = lane_mut
                    .breakpoints
                    .iter()
                    .position(|b| b.tick == tick)
                    .expect("MoveBreakpoint: no breakpoint at tick");
                let old = lane_mut.breakpoints.remove(index);
                insert_breakpoint_sorted(
                    &mut lane_mut.breakpoints,
                    Breakpoint { tick: new_tick, value: new_value },
                );
                Command::MoveBreakpoint { lane, tick: new_tick, new_tick: old.tick, new_value: old.value }
            }

            Command::SetLoopRange { range } => {
                let old = arr.loop_range;
                arr.loop_range = range;
                Command::SetLoopRange { range: old }
            }

            Command::InsertMarker { marker } => {
                let id = marker.id;
                arr.markers.push(marker);
                Command::RemoveMarker { marker: id }
            }

            Command::RemoveMarker { marker: marker_id } => {
                let index = arr.markers.iter().position(|m| m.id == marker_id).expect("RemoveMarker: unknown marker");
                let removed = arr.markers.remove(index);
                Command::InsertMarker { marker: removed }
            }

            Command::RenameMarker { marker: marker_id, name } => {
                let marker = arr.markers.iter_mut().find(|m| m.id == marker_id).expect("RenameMarker: unknown marker");
                let old_name = std::mem::replace(&mut marker.name, name);
                Command::RenameMarker { marker: marker_id, name: old_name }
            }

            Command::RenameTrack { track: track_id, name } => {
                let track = arr.track_mut(track_id).expect("RenameTrack: unknown track");
                let old_name = std::mem::replace(&mut track.name, name);
                Command::RenameTrack { track: track_id, name: old_name }
            }

            Command::SetTempo { bpm } => {
                let old_bpm = arr.tempo_map.bpm_at(0);
                let time_signature = arr.tempo_map.time_signature_at(0);
                arr.tempo_map = TempoMap::constant(bpm, time_signature);
                Command::SetTempo { bpm: old_bpm }
            }

            Command::SetTimeSignature { numerator, denominator } => {
                let bpm = arr.tempo_map.bpm_at(0);
                let old_sig = arr.tempo_map.time_signature_at(0);
                arr.tempo_map = TempoMap::constant(bpm, TimeSignature { numerator, denominator });
                Command::SetTimeSignature { numerator: old_sig.numerator, denominator: old_sig.denominator }
            }
        }
    }
}

fn insert_breakpoint_sorted(breakpoints: &mut Vec<Breakpoint>, point: Breakpoint) {
    let index = breakpoints.partition_point(|b| b.tick < point.tick);
    breakpoints.insert(index, point);
}

fn ticks_to_samples_at(ticks: Ticks, bpm: f64, sample_rate: u32) -> i64 {
    let seconds = (ticks as f64 / super::time::PPQ as f64) * (60.0 / bpm);
    (seconds * sample_rate as f64).round() as i64
}

/// A plain undo/redo stack of inverse commands. `do_command` clears the
/// redo stack, matching standard editor behaviour.
#[derive(Default)]
pub struct CommandStack {
    undo: Vec<Command>,
    redo: Vec<Command>,
}

impl CommandStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn do_command(&mut self, command: Command, arr: &mut Arrangement) {
        let inverse = command.apply(arr);
        self.undo.push(inverse);
        self.redo.clear();
    }

    pub fn undo(&mut self, arr: &mut Arrangement) -> bool {
        match self.undo.pop() {
            Some(command) => {
                let inverse = command.apply(arr);
                self.redo.push(inverse);
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self, arr: &mut Arrangement) -> bool {
        match self.redo.pop() {
            Some(command) => {
                let inverse = command.apply(arr);
                self.undo.push(inverse);
                true
            }
            None => false,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangement::model::{Arrangement, AutomationLane, ClipColor, Track, TrackKind};
    use crate::arrangement::time::{TempoMap, TimeSignature, PPQ};

    fn test_arrangement() -> Arrangement {
        let mut arr = Arrangement::new(TempoMap::constant(128.0, TimeSignature::FOUR_FOUR));
        arr.tracks.push(Track {
            id: 1,
            name: "Drums".into(),
            color: ClipColor::Coral,
            kind: TrackKind::Audio,
            mute: false,
            solo: false,
            arm: false,
            gain_db: 0.0,
            height: 56.0,
            instrument: None,
            effects: vec![],
            effect_slots: vec![],
            fx: crate::arrangement::EffectGraph::new(),
        });
        arr
    }

    fn audio_clip(id: ClipId, track: TrackId, start: Ticks, length: Ticks) -> Clip {
        Clip {
            id,
            track,
            start,
            length,
            name: "Clip".into(),
            content: ClipContent::Audio {
                source: "test.wav".into(),
                peaks: None,
                source_offset_samples: 0,
            },
            recording: false,
            gain_db: 0.0,
        }
    }

    #[test]
    fn move_clip_undoes_to_original_position() {
        let mut arr = test_arrangement();
        arr.clips.push(audio_clip(1, 1, 0, PPQ * 4));
        let mut stack = CommandStack::new();

        stack.do_command(Command::MoveClip { clip: 1, track: 1, start: PPQ * 8 }, &mut arr);
        assert_eq!(arr.clip(1).unwrap().start, PPQ * 8);

        assert!(stack.undo(&mut arr));
        assert_eq!(arr.clip(1).unwrap().start, 0);

        assert!(stack.redo(&mut arr));
        assert_eq!(arr.clip(1).unwrap().start, PPQ * 8);
    }

    #[test]
    fn split_then_undo_restores_single_clip() {
        let mut arr = test_arrangement();
        arr.clips.push(audio_clip(1, 1, 0, PPQ * 4 * 4));
        let mut stack = CommandStack::new();

        stack.do_command(Command::SplitClip { clip: 1, at: PPQ * 4 * 2, new_id: 2 }, &mut arr);
        assert_eq!(arr.clips.len(), 2);
        assert_eq!(arr.clip(1).unwrap().length, PPQ * 4 * 2);
        assert_eq!(arr.clip(2).unwrap().start, PPQ * 4 * 2);
        assert_eq!(arr.clip(2).unwrap().length, PPQ * 4 * 2);

        assert!(stack.undo(&mut arr));
        assert_eq!(arr.clips.len(), 1);
        assert_eq!(arr.clip(1).unwrap().length, PPQ * 4 * 4);

        assert!(stack.redo(&mut arr));
        assert_eq!(arr.clips.len(), 2);
    }

    #[test]
    fn delete_then_undo_restores_clip() {
        let mut arr = test_arrangement();
        arr.clips.push(audio_clip(1, 1, PPQ * 4, PPQ * 4));
        let mut stack = CommandStack::new();

        stack.do_command(Command::DeleteClip { clip: 1 }, &mut arr);
        assert!(arr.clip(1).is_none());

        assert!(stack.undo(&mut arr));
        let clip = arr.clip(1).expect("clip restored");
        assert_eq!(clip.start, PPQ * 4);
        assert_eq!(clip.length, PPQ * 4);
    }

    #[test]
    fn duplicate_creates_offset_copy_and_undoes() {
        let mut arr = test_arrangement();
        arr.clips.push(audio_clip(1, 1, 0, PPQ * 4));
        let mut stack = CommandStack::new();

        stack.do_command(
            Command::DuplicateClip { clip: 1, new_id: 2, offset: PPQ * 4 },
            &mut arr,
        );
        assert_eq!(arr.clip(2).unwrap().start, PPQ * 4);
        assert_eq!(arr.clips.len(), 2);

        assert!(stack.undo(&mut arr));
        assert_eq!(arr.clips.len(), 1);
    }

    #[test]
    fn add_then_remove_automation_lane_round_trips_through_undo() {
        let mut arr = test_arrangement();
        let lane = |id| AutomationLane {
            id,
            track: 1,
            parameter_name: String::new(),
            display_value: String::new(),
            breakpoints: vec![Breakpoint { tick: 0, value: 0.3 }],
            target: Some(crate::arrangement::AutomationTarget::TrackGain),
        };
        let mut stack = CommandStack::new();
        stack.do_command(Command::InsertAutomationLane { lane: Box::new(lane(1)), index: 0 }, &mut arr);
        stack.do_command(Command::InsertAutomationLane { lane: Box::new(lane(2)), index: 1 }, &mut arr);
        stack.do_command(Command::RemoveAutomationLane { lane: 1 }, &mut arr);
        assert_eq!(arr.automation.iter().map(|l| l.id).collect::<Vec<_>>(), vec![2]);
        assert!(stack.undo(&mut arr));
        assert_eq!(arr.automation.iter().map(|l| l.id).collect::<Vec<_>>(), vec![1, 2], "restored in place");
        assert!(stack.undo(&mut arr));
        assert!(stack.undo(&mut arr));
        assert!(arr.automation.is_empty());
    }

    #[test]
    fn breakpoints_stay_sorted_and_undo_removes() {
        let mut arr = test_arrangement();
        arr.automation.push(AutomationLane {
            id: 1,
            track: 1,
            parameter_name: "Cutoff".into(),
            display_value: "2.4 kHz".into(),
            breakpoints: vec![],
            target: None,
        });
        let mut stack = CommandStack::new();

        stack.do_command(
            Command::AddBreakpoint { lane: 1, point: Breakpoint { tick: PPQ * 8, value: 0.8 } },
            &mut arr,
        );
        stack.do_command(
            Command::AddBreakpoint { lane: 1, point: Breakpoint { tick: PPQ * 2, value: 0.2 } },
            &mut arr,
        );
        let lane = arr.automation_lane(1).unwrap();
        assert_eq!(lane.breakpoints.iter().map(|b| b.tick).collect::<Vec<_>>(), vec![PPQ * 2, PPQ * 8]);

        assert!(stack.undo(&mut arr));
        assert_eq!(arr.automation_lane(1).unwrap().breakpoints.len(), 1);
        assert!(stack.undo(&mut arr));
        assert_eq!(arr.automation_lane(1).unwrap().breakpoints.len(), 0);
    }

    fn looping_clip(arr: &mut Arrangement) -> ClipId {
        // A one-bar pattern (kick on every beat) looped over four bars.
        let notes = (0..4).map(|b| MidiNote { start: b * PPQ, length: PPQ / 4, pitch: 36, velocity: 100 }).collect();
        arr.clips.push(Clip {
            id: 1,
            track: 1,
            start: 0,
            length: PPQ * 16,
            name: "Beat".into(),
            content: ClipContent::Midi { notes, loop_len: Some(PPQ * 4), link: None },
            recording: false,
            gain_db: 0.0,
        });
        1
    }

    #[test]
    fn a_looping_clip_plays_its_pattern_for_its_whole_length() {
        let mut arr = test_arrangement();
        let id = looping_clip(&mut arr);
        let played = arr.clip(id).unwrap().played_notes();
        assert_eq!(played.len(), 16);
        assert_eq!(played.last().unwrap().start, PPQ * 15);
        // Shorter than the pattern: only what fits, the last note cut.
        let clip = arr.clip_mut(id).unwrap();
        clip.length = PPQ * 2 + PPQ / 8;
        let played = arr.clip(id).unwrap().played_notes();
        assert_eq!(played.len(), 3);
        assert_eq!(played[2].length, PPQ / 8);
    }

    #[test]
    fn notes_past_a_non_looping_clips_end_are_not_played() {
        let mut arr = test_arrangement();
        let id = looping_clip(&mut arr);
        if let ClipContent::Midi { loop_len, .. } = &mut arr.clip_mut(id).unwrap().content {
            *loop_len = None;
        }
        arr.clip_mut(id).unwrap().length = PPQ * 2;
        assert_eq!(arr.clip(id).unwrap().played_notes().len(), 2);
    }

    #[test]
    fn splitting_a_looping_clip_keeps_every_hit_and_undo_restores_it() {
        let mut arr = test_arrangement();
        let id = looping_clip(&mut arr);
        let before = arr.clip(id).unwrap().clone();
        let mut stack = CommandStack::new();
        stack.do_command(Command::SplitClip { clip: id, at: PPQ * 6, new_id: 2 }, &mut arr);
        let left = arr.clip(id).unwrap().played_notes().len();
        let right = arr.clip(2).unwrap().played_notes().len();
        assert_eq!((left, right), (6, 10));
        assert_eq!(arr.clip(2).unwrap().played_notes()[0].start, 0);

        assert!(stack.undo(&mut arr));
        assert!(arr.clip(2).is_none());
        let after = arr.clip(id).unwrap();
        assert_eq!(after.length, before.length);
        assert_eq!(after.played_notes(), before.played_notes());
        assert!(matches!(after.content, ClipContent::Midi { loop_len: Some(_), .. }));
    }

    /// Clip 1 (the looping beat) and a linked copy, clip 2, four bars later.
    fn linked_pair(arr: &mut Arrangement) {
        let id = looping_clip(arr);
        if let ClipContent::Midi { link, .. } = &mut arr.clip_mut(id).unwrap().content {
            *link = Some(99);
        }
        let mut copy = arr.clip(id).unwrap().clone();
        copy.id = 2;
        copy.start = PPQ * 16;
        arr.clips.push(copy);
    }

    fn note_count(arr: &Arrangement, clip: ClipId) -> usize {
        match &arr.clip(clip).unwrap().content {
            ClipContent::Midi { notes, .. } => notes.len(),
            _ => 0,
        }
    }

    #[test]
    fn editing_a_linked_clip_edits_every_copy_and_undo_too() {
        let mut arr = test_arrangement();
        linked_pair(&mut arr);
        let mut stack = CommandStack::new();
        let snare = MidiNote { start: PPQ, length: PPQ / 4, pitch: 38, velocity: 100 };
        stack.do_command(Command::AddMidiNote { clip: 2, note: snare }, &mut arr);
        assert_eq!((note_count(&arr, 1), note_count(&arr, 2)), (5, 5));
        stack.do_command(Command::SetNoteVelocity { clip: 1, start: PPQ, pitch: 38, velocity: 40 }, &mut arr);
        let velocity = |c| match &arr.clip(c).unwrap().content {
            ClipContent::Midi { notes, .. } => notes.iter().find(|n| n.pitch == 38).unwrap().velocity,
            _ => 0,
        };
        assert_eq!((velocity(1), velocity(2)), (40, 40));
        assert!(stack.undo(&mut arr));
        assert!(stack.undo(&mut arr));
        assert_eq!((note_count(&arr, 1), note_count(&arr, 2)), (4, 4));
        // Positions stay each clip's own.
        assert_eq!(arr.clip(2).unwrap().start, PPQ * 16);
    }

    #[test]
    fn plain_duplicates_and_split_halves_are_independent() {
        let mut arr = test_arrangement();
        linked_pair(&mut arr);
        let mut stack = CommandStack::new();
        stack.do_command(Command::DuplicateClip { clip: 1, new_id: 3, offset: PPQ * 64 }, &mut arr);
        stack.do_command(Command::AddMidiNote { clip: 3, note: MidiNote { start: 0, length: 1, pitch: 50, velocity: 1 } }, &mut arr);
        assert_eq!((note_count(&arr, 1), note_count(&arr, 3)), (4, 5));
        stack.do_command(Command::SplitClip { clip: 2, at: PPQ * 20, new_id: 4 }, &mut arr);
        assert!(matches!(arr.clip(2).unwrap().content, ClipContent::Midi { link: None, .. }));
        stack.do_command(Command::AddMidiNote { clip: 1, note: MidiNote { start: 0, length: 1, pitch: 51, velocity: 1 } }, &mut arr);
        assert_eq!(note_count(&arr, 2), 4, "split half no longer follows its old group");
        assert_eq!(arr.link_count(99), 1);
    }

    #[test]
    fn replace_clip_undoes_to_the_old_clip() {
        let mut arr = test_arrangement();
        let id = looping_clip(&mut arr);
        let mut stack = CommandStack::new();
        let mut longer = arr.clip(id).unwrap().clone();
        longer.length = PPQ * 32;
        stack.do_command(Command::ReplaceClip { clip: Box::new(longer) }, &mut arr);
        assert_eq!(arr.clip(id).unwrap().played_notes().len(), 32);
        assert!(stack.undo(&mut arr));
        assert_eq!(arr.clip(id).unwrap().length, PPQ * 16);
    }

    #[test]
    fn add_midi_note_then_undo_removes_it() {
        let mut arr = test_arrangement();
        let clip_id = 1;
        arr.clips.push(Clip {
            id: clip_id,
            track: 1,
            start: 0,
            length: PPQ * 4,
            name: "Step".into(),
            content: ClipContent::Midi { notes: vec![], loop_len: None, link: None },
            recording: false,
            gain_db: 0.0,
        });
        let mut stack = CommandStack::new();

        stack.do_command(
            Command::AddMidiNote { clip: clip_id, note: MidiNote { start: 0, length: PPQ / 4, pitch: 60, velocity: DEFAULT_VELOCITY } },
            &mut arr,
        );
        let ClipContent::Midi { notes, .. } = &arr.clip(clip_id).unwrap().content else { panic!() };
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].pitch, 60);

        assert!(stack.undo(&mut arr));
        let ClipContent::Midi { notes, .. } = &arr.clip(clip_id).unwrap().content else { panic!() };
        assert!(notes.is_empty());

        assert!(stack.redo(&mut arr));
        let ClipContent::Midi { notes, .. } = &arr.clip(clip_id).unwrap().content else { panic!() };
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn redo_stack_clears_on_new_command() {
        let mut arr = test_arrangement();
        arr.clips.push(audio_clip(1, 1, 0, PPQ * 4));
        let mut stack = CommandStack::new();

        stack.do_command(Command::MoveClip { clip: 1, track: 1, start: PPQ * 4 }, &mut arr);
        stack.undo(&mut arr);
        assert!(stack.can_redo());

        stack.do_command(Command::MoveClip { clip: 1, track: 1, start: PPQ * 8 }, &mut arr);
        assert!(!stack.can_redo());
    }

    #[test]
    fn set_tempo_keeps_time_signature_and_undoes() {
        let mut arr = test_arrangement();
        let mut stack = CommandStack::new();
        assert_eq!(arr.tempo_map.bpm_at(0), 128.0);

        stack.do_command(Command::SetTempo { bpm: 90.0 }, &mut arr);
        assert_eq!(arr.tempo_map.bpm_at(0), 90.0);
        assert_eq!(arr.tempo_map.time_signature_at(0), TimeSignature::FOUR_FOUR);

        assert!(stack.undo(&mut arr));
        assert_eq!(arr.tempo_map.bpm_at(0), 128.0);
    }

    fn new_track(id: TrackId) -> Track {
        Track {
            id,
            name: "Mic".into(),
            color: ClipColor::Teal,
            kind: TrackKind::Audio,
            mute: false,
            solo: false,
            arm: false,
            gain_db: 0.0,
            height: 56.0,
            instrument: None,
            effects: vec![],
            effect_slots: vec![],
            fx: crate::arrangement::EffectGraph::new(),
        }
    }

    #[test]
    fn insert_track_appends_and_undo_removes() {
        let mut arr = test_arrangement();
        let mut stack = CommandStack::new();

        stack.do_command(
            Command::InsertTrack { track: Box::new(new_track(2)), index: 1, clips: vec![], automation: vec![] },
            &mut arr,
        );
        assert_eq!(arr.tracks.len(), 2);
        assert_eq!(arr.tracks[1].id, 2);

        assert!(stack.undo(&mut arr));
        assert_eq!(arr.tracks.len(), 1);
        assert!(arr.track(2).is_none());
    }

    #[test]
    fn delete_track_cascades_to_clips_and_automation_then_undo_restores_everything() {
        let mut arr = test_arrangement();
        arr.tracks.push(new_track(2));
        arr.clips.push(audio_clip(1, 2, 0, PPQ * 4));
        arr.automation.push(AutomationLane {
            id: 1,
            track: 2,
            parameter_name: "Gain".into(),
            display_value: "0 dB".into(),
            breakpoints: vec![Breakpoint { tick: 0, value: 0.5 }],
            target: None,
        });
        // An untouched clip on the other track shouldn't be affected.
        arr.clips.push(audio_clip(2, 1, 0, PPQ * 4));
        let mut stack = CommandStack::new();

        stack.do_command(Command::DeleteTrack { track: 2 }, &mut arr);
        assert!(arr.track(2).is_none());
        assert!(arr.clip(1).is_none());
        assert!(arr.automation_lane(1).is_none());
        assert!(arr.clip(2).is_some());

        assert!(stack.undo(&mut arr));
        assert!(arr.track(2).is_some());
        assert_eq!(arr.tracks[1].id, 2);
        assert_eq!(arr.clip(1).unwrap().track, 2);
        assert_eq!(arr.automation_lane(1).unwrap().track, 2);
        assert_eq!(arr.clip(2).unwrap().track, 1);
    }

    #[test]
    fn set_note_velocity_undoes_to_the_previous_value() {
        let mut arr = test_arrangement();
        let clip_id = 1;
        arr.clips.push(Clip {
            id: clip_id,
            track: 1,
            start: 0,
            length: PPQ * 4,
            name: "Velocity".into(),
            content: ClipContent::Midi { notes: vec![], loop_len: None, link: None },
            recording: false,
            gain_db: 0.0,
        });
        let add = Command::AddMidiNote {
            clip: clip_id,
            note: MidiNote { start: 0, length: PPQ / 4, pitch: 60, velocity: DEFAULT_VELOCITY },
        };
        add.apply(&mut arr);
        let undo = Command::SetNoteVelocity { clip: clip_id, start: 0, pitch: 60, velocity: 40 }.apply(&mut arr);
        let velocity = |arr: &Arrangement| match &arr.clip(clip_id).unwrap().content {
            ClipContent::Midi { notes, .. } => notes[0].velocity,
            _ => unreachable!(),
        };
        assert_eq!(velocity(&arr), 40);
        undo.apply(&mut arr);
        assert_eq!(velocity(&arr), DEFAULT_VELOCITY);
    }
}
