//! Step-entry recording: turns one committed step (zero or more
//! simultaneous pitches played on Carve) into the `Command`s that place it
//! on the timeline. Pure and side-effect free - `next_new_id` is a
//! pre-allocated id the caller reserves up front (via
//! `Arrangement::alloc_id`) since this function can't mutate the
//! arrangement itself; it's simply unused if an existing clip is reused
//! instead.

use super::model::{Arrangement, Clip, ClipContent, ClipId, MidiNote, TrackId, DEFAULT_VELOCITY};
use super::time::Ticks;
use super::Command;

/// Builds the command for one step-entry step. Reuses `reuse_clip` by
/// extending it if it belongs to `track` and ends exactly at `playhead`
/// (i.e. the previous step landed right before this one); otherwise starts
/// a fresh one-step clip at `playhead` using `next_new_id`. Returns the
/// command to apply and the id of the clip it ends up targeting (so the
/// caller can pass it back in as `reuse_clip` next time).
pub fn step_entry_commit(
    arr: &Arrangement,
    track: TrackId,
    playhead: Ticks,
    step: Ticks,
    reuse_clip: Option<ClipId>,
    next_new_id: ClipId,
    pitches: &[u8],
) -> (Command, ClipId) {
    let reuse = reuse_clip.and_then(|id| arr.clip(id)).filter(|c| c.track == track && c.end() == playhead);

    let mut batch = Vec::new();
    let (clip_id, clip_start) = if let Some(clip) = reuse {
        let id = clip.id;
        let start = clip.start;
        batch.push(Command::TrimClip { clip: id, start, length: clip.length + step });
        (id, start)
    } else {
        let clip = Clip {
            id: next_new_id,
            track,
            start: playhead,
            length: step,
            name: "Step".into(),
            content: ClipContent::Midi { notes: Vec::new() },
            recording: false,
            gain_db: 0.0,
        };
        batch.push(Command::InsertClip { clip: Box::new(clip) });
        (next_new_id, playhead)
    };

    for &pitch in pitches {
        batch.push(Command::AddMidiNote {
            clip: clip_id,
            note: MidiNote { start: playhead - clip_start, length: step, pitch, velocity: DEFAULT_VELOCITY },
        });
    }

    (Command::Batch(batch), clip_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangement::commands::CommandStack;
    use crate::arrangement::model::{ClipColor, Track, TrackKind};
    use crate::arrangement::time::{TempoMap, TimeSignature, PPQ};

    const STEP: Ticks = PPQ / 4;

    fn test_arrangement() -> Arrangement {
        let mut arr = Arrangement::new(TempoMap::constant(120.0, TimeSignature::FOUR_FOUR));
        arr.tracks.push(Track {
            id: 1,
            name: "Bass".into(),
            color: ClipColor::Amber,
            kind: TrackKind::Midi,
            mute: false,
            solo: false,
            arm: true,
            gain_db: 0.0,
            height: 56.0,
            instrument: None,
        effects: vec![],
        effect_slots: vec![],
        });
        arr
    }

    #[test]
    fn first_step_creates_a_new_clip() {
        let mut arr = test_arrangement();
        let mut stack = CommandStack::new();
        let new_id = arr.alloc_id();

        let (command, clip_id) = step_entry_commit(&arr, 1, 0, STEP, None, new_id, &[60]);
        stack.do_command(command, &mut arr);

        assert_eq!(clip_id, new_id);
        let clip = arr.clip(clip_id).unwrap();
        assert_eq!(clip.start, 0);
        assert_eq!(clip.length, STEP);
        let ClipContent::Midi { notes } = &clip.content else { panic!("expected a MIDI clip") };
        assert_eq!(notes, &[MidiNote { start: 0, length: STEP, pitch: 60, velocity: DEFAULT_VELOCITY }]);
    }

    #[test]
    fn next_step_extends_the_clip_when_it_abuts_the_playhead() {
        let mut arr = test_arrangement();
        let mut stack = CommandStack::new();
        let id1 = arr.alloc_id();
        let (cmd1, clip1) = step_entry_commit(&arr, 1, 0, STEP, None, id1, &[60]);
        stack.do_command(cmd1, &mut arr);

        let id2 = arr.alloc_id();
        let (cmd2, clip2) = step_entry_commit(&arr, 1, STEP, STEP, Some(clip1), id2, &[64]);
        stack.do_command(cmd2, &mut arr);

        assert_eq!(clip2, clip1, "should reuse the same clip, not create a second one");
        assert_eq!(arr.clips.len(), 1);
        let clip = arr.clip(clip1).unwrap();
        assert_eq!(clip.length, STEP * 2);
        let ClipContent::Midi { notes } = &clip.content else { panic!() };
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[1], MidiNote { start: STEP, length: STEP, pitch: 64, velocity: DEFAULT_VELOCITY });
    }

    #[test]
    fn a_gap_starts_a_fresh_clip_instead_of_reusing() {
        let mut arr = test_arrangement();
        let mut stack = CommandStack::new();
        let id1 = arr.alloc_id();
        let (cmd1, clip1) = step_entry_commit(&arr, 1, 0, STEP, None, id1, &[60]);
        stack.do_command(cmd1, &mut arr);

        // Playhead jumped ahead (e.g. the user scrubbed) instead of landing
        // right after the first clip - shouldn't silently extend it.
        let id2 = arr.alloc_id();
        let (cmd2, clip2) = step_entry_commit(&arr, 1, STEP * 5, STEP, Some(clip1), id2, &[64]);
        stack.do_command(cmd2, &mut arr);

        assert_ne!(clip2, clip1);
        assert_eq!(arr.clips.len(), 2);
        assert_eq!(arr.clip(clip1).unwrap().length, STEP);
    }

    #[test]
    fn a_rest_extends_length_without_adding_a_note() {
        let mut arr = test_arrangement();
        let mut stack = CommandStack::new();
        let id1 = arr.alloc_id();
        let (cmd1, clip1) = step_entry_commit(&arr, 1, 0, STEP, None, id1, &[60]);
        stack.do_command(cmd1, &mut arr);

        let id2 = arr.alloc_id();
        let (cmd2, clip2) = step_entry_commit(&arr, 1, STEP, STEP, Some(clip1), id2, &[]);
        stack.do_command(cmd2, &mut arr);

        assert_eq!(clip2, clip1);
        let clip = arr.clip(clip1).unwrap();
        assert_eq!(clip.length, STEP * 2);
        let ClipContent::Midi { notes } = &clip.content else { panic!() };
        assert_eq!(notes.len(), 1, "the rest shouldn't have added a note");
    }

    #[test]
    fn commit_then_undo_restores_the_previous_clip_state() {
        let mut arr = test_arrangement();
        let mut stack = CommandStack::new();
        let id1 = arr.alloc_id();
        let (cmd1, clip1) = step_entry_commit(&arr, 1, 0, STEP, None, id1, &[60]);
        stack.do_command(cmd1, &mut arr);

        let id2 = arr.alloc_id();
        let (cmd2, _) = step_entry_commit(&arr, 1, STEP, STEP, Some(clip1), id2, &[64]);
        stack.do_command(cmd2, &mut arr);
        assert_eq!(arr.clip(clip1).unwrap().length, STEP * 2);

        assert!(stack.undo(&mut arr));
        assert_eq!(arr.clip(clip1).unwrap().length, STEP);
        assert!(stack.undo(&mut arr));
        assert!(arr.clip(clip1).is_none());
    }
}
