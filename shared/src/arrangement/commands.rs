//! Every arrangement edit goes through a [`Command`]. Applying a command
//! returns the command that undoes it, built from the arrangement's actual
//! prior state (not guessed), so [`CommandStack`] can implement undo/redo as
//! a plain stack of inverses.

use super::model::{
    Arrangement, AutomationLaneId, Breakpoint, Clip, ClipContent, ClipId, LoopRange, MidiNote,
    TrackId,
};
use super::time::Ticks;

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
    AddBreakpoint { lane: AutomationLaneId, point: Breakpoint },
    RemoveBreakpoint { lane: AutomationLaneId, tick: Ticks },
    MoveBreakpoint { lane: AutomationLaneId, tick: Ticks, new_tick: Ticks, new_value: f32 },
    SetLoopRange { range: Option<LoopRange> },
}

impl Command {
    /// Applies this command to `arr` and returns its inverse.
    pub fn apply(self, arr: &mut Arrangement) -> Command {
        match self {
            Command::Batch(cmds) => {
                let inverses =
                    cmds.into_iter().map(|c| c.apply(arr)).rev().collect::<Vec<_>>();
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
                    ClipContent::Midi { notes } => {
                        let (_left, right): (Vec<&MidiNote>, Vec<&MidiNote>) =
                            notes.iter().partition(|n| n.start < left_length);
                        ClipContent::Midi {
                            notes: right
                                .into_iter()
                                .map(|n| MidiNote { start: n.start - left_length, ..*n })
                                .collect(),
                        }
                    }
                };
                let left_notes = if let ClipContent::Midi { notes } = &original.content {
                    Some(notes.iter().filter(|n| n.start < left_length).copied().collect())
                } else {
                    None
                };

                let right_clip = Clip {
                    id: new_id,
                    track: original.track,
                    start: right_start,
                    length: right_length,
                    name: original.name.clone(),
                    content: right_content,
                    recording: false,
                };

                let clip = arr.clip_mut(clip_id).unwrap();
                clip.length = left_length;
                if let (Some(notes), ClipContent::Midi { notes: dst }) =
                    (left_notes, &mut clip.content)
                {
                    *dst = notes;
                }
                arr.clips.push(right_clip);

                Command::Batch(vec![
                    Command::DeleteClip { clip: new_id },
                    Command::TrimClip { clip: clip_id, start: original.start, length: original.length },
                ])
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

            Command::DuplicateClip { clip: clip_id, new_id, offset } => {
                let mut copy = arr.clip(clip_id).expect("DuplicateClip: unknown clip").clone();
                copy.id = new_id;
                copy.start += offset;
                copy.recording = false;
                arr.clips.push(copy);
                Command::DeleteClip { clip: new_id }
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
    fn breakpoints_stay_sorted_and_undo_removes() {
        let mut arr = test_arrangement();
        arr.automation.push(AutomationLane {
            id: 1,
            track: 1,
            parameter_name: "Cutoff".into(),
            display_value: "2.4 kHz".into(),
            breakpoints: vec![],
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
}
