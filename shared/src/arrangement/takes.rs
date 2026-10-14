//! Takes: recording over an audio clip keeps the old recording as an
//! alternate of the new one, rather than stacking clips; any take can be
//! swapped in, and a split keeps every take lined up, so each part can
//! play a different one (comping).

use super::commands::Command;
use super::model::{Arrangement, Clip, ClipContent, Take};
use super::time::PPQ;

/// What putting `new` (a just-recorded audio clip) on its track does: the
/// audio clips it was recorded over - starting within a beat of it and
/// ending inside it - become its takes (most recent first) and leave the
/// track. Every other clip stays.
pub fn record_over(arr: &Arrangement, mut new: Clip) -> Vec<Command> {
    let mut commands = Vec::new();
    let mut takes = Vec::new();
    let mut covered: Vec<&Clip> = arr
        .clips
        .iter()
        .filter(|c| c.track == new.track && c.id != new.id)
        .filter(|c| (c.start - new.start).abs() <= PPQ && c.end() <= new.end() + PPQ)
        .filter(|c| matches!(c.content, ClipContent::Audio { .. }))
        .collect();
    covered.sort_by_key(|c| std::cmp::Reverse(c.id));
    for old in covered {
        let ClipContent::Audio { source, source_offset_samples, takes: old_takes, .. } = &old.content else { continue };
        // Lined up with the new clip's start (frames at 48 kHz).
        let shift = |offset: u64| -> u64 {
            let delta = arr.tempo_map.ticks_to_samples(new.start, 48_000) - arr.tempo_map.ticks_to_samples(old.start, 48_000);
            (offset as i64 + delta).max(0) as u64
        };
        takes.push(Take { source: source.clone(), offset: shift(*source_offset_samples) });
        takes.extend(old_takes.iter().map(|t| Take { source: t.source.clone(), offset: shift(t.offset) }));
        commands.push(Command::DeleteClip { clip: old.id });
    }
    if let ClipContent::Audio { takes: new_takes, .. } = &mut new.content {
        new_takes.extend(takes);
    }
    commands.push(Command::InsertClip { clip: Box::new(new) });
    commands
}

/// `clip` playing its take `index` instead; what it played becomes that
/// take. `None` if there's no such take.
pub fn use_take(clip: &Clip, index: usize) -> Option<Clip> {
    let mut next = clip.clone();
    let ClipContent::Audio { source, source_offset_samples, takes, peaks, .. } = &mut next.content else { return None };
    let take = takes.get_mut(index)?;
    std::mem::swap(source, &mut take.source);
    std::mem::swap(source_offset_samples, &mut take.offset);
    *peaks = None;
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangement::model::{ClipColor, Track, TrackKind};
    use crate::arrangement::time::{TempoMap, TimeSignature};
    use crate::arrangement::CommandStack;

    fn audio(id: u32, start: i64, length: i64, source: &str) -> Clip {
        Clip {
            id,
            track: 1,
            start,
            length,
            name: "Take".into(),
            content: ClipContent::Audio { source: source.into(), peaks: None, source_offset_samples: 0, fade_in: 0, fade_out: 0, takes: Vec::new() },
            recording: false,
            gain_db: 0.0,
            swing: 0.0,
        }
    }

    fn song() -> Arrangement {
        let mut arr = Arrangement::new(TempoMap::constant(120.0, TimeSignature::FOUR_FOUR));
        arr.tracks.push(Track {
            id: 1,
            name: "Vocal".into(),
            color: ClipColor::Teal,
            kind: TrackKind::Audio,
            mute: false,
            solo: false,
            arm: true,
            gain_db: 0.0,
            height: 56.0,
            instrument: None,
            effects: vec![],
            effect_slots: vec![],
            fx: crate::arrangement::EffectGraph::new(),
            drum_pads: Default::default(),
        });
        arr.clips.push(audio(2, PPQ * 4, PPQ * 8, "first.wav"));
        arr.clips.push(audio(3, PPQ * 20, PPQ * 4, "elsewhere.wav"));
        arr
    }

    fn source(clip: &Clip) -> &str {
        match &clip.content {
            ClipContent::Audio { source, .. } => source,
            _ => "",
        }
    }

    #[test]
    fn recording_over_a_clip_keeps_it_as_a_take() {
        let mut arr = song();
        let mut stack = CommandStack::new();
        let commands = record_over(&arr, audio(9, PPQ * 4, PPQ * 8, "second.wav"));
        stack.do_command(Command::Batch(commands), &mut arr);
        assert_eq!(arr.clips.len(), 2, "the old clip became a take; the other clip stays");
        let new = arr.clip(9).unwrap();
        let ClipContent::Audio { takes, .. } = &new.content else { panic!() };
        assert_eq!(takes, &vec![Take { source: "first.wav".into(), offset: 0 }]);

        // Swap the first recording back in, then undo all of it.
        let swapped = use_take(new, 0).unwrap();
        assert_eq!(source(&swapped), "first.wav");
        assert!(matches!(&swapped.content, ClipContent::Audio { takes, .. } if &*takes[0].source == "second.wav"));
        assert!(stack.undo(&mut arr));
        assert_eq!(arr.clips.iter().map(source).collect::<Vec<_>>(), vec!["elsewhere.wav", "first.wav"]);
    }

    #[test]
    fn a_split_keeps_every_take_lined_up() {
        let mut arr = song();
        let commands = record_over(&arr, audio(9, PPQ * 4, PPQ * 8, "second.wav"));
        Command::Batch(commands).apply(&mut arr);
        Command::SplitClip { clip: 9, at: PPQ * 8, new_id: 10 }.apply(&mut arr);
        let right = arr.clip(10).unwrap();
        let ClipContent::Audio { source_offset_samples, takes, .. } = &right.content else { panic!() };
        // Four beats in at 120 bpm = 96000 frames into both recordings.
        assert_eq!((*source_offset_samples, takes[0].offset), (96_000, 96_000));
    }
}
