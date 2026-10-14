//! Edits along the song's time, across every track at once: cutting all
//! the clips at one point, and taking a stretch of bars out of the song so
//! what comes after it moves back to close the gap.

use super::commands::Command;
use super::model::{Arrangement, LoopRange};
use super::time::Ticks;

/// One `SplitClip` for every clip that crosses `at`, on every track.
pub fn split_all_at(arr: &mut Arrangement, at: Ticks) -> Vec<Command> {
    let targets: Vec<_> = arr.clips.iter().filter(|c| c.start < at && c.end() > at).map(|c| c.id).collect();
    targets.into_iter().map(|clip| Command::SplitClip { clip, at, new_id: arr.alloc_id() }).collect()
}

/// `arr` with `start..end` taken out of every track: clips are cut at both
/// ends and what's inside is dropped, and everything after moves back by
/// the gap. Markers, automation points and the loop range move with it.
pub fn remove_time(arr: &Arrangement, start: Ticks, end: Ticks) -> Arrangement {
    let mut next = arr.clone();
    if end <= start {
        return next;
    }
    let gap = end - start;
    for at in [start, end] {
        for command in split_all_at(&mut next, at) {
            command.apply(&mut next);
        }
    }
    next.clips.retain(|c| c.end() <= start || c.start >= end);
    for clip in next.clips.iter_mut().filter(|c| c.start >= end) {
        clip.start -= gap;
    }

    let moved = |t: Ticks| if t <= start { t } else if t < end { start } else { t - gap };
    for lane in &mut next.automation {
        lane.breakpoints.retain(|b| b.tick < start || b.tick >= end);
        for point in &mut lane.breakpoints {
            point.tick = moved(point.tick);
        }
    }
    next.markers.retain(|m| m.position < start || m.position >= end);
    for marker in &mut next.markers {
        marker.position = moved(marker.position);
    }
    next.loop_range = next.loop_range.and_then(|r| {
        let (s, e) = (moved(r.start), moved(r.end));
        (e > s).then_some(LoopRange { start: s, end: e })
    });
    next
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangement::model::{Clip, ClipColor, ClipContent, Marker, MidiNote, Track, TrackKind};
    use crate::arrangement::time::{TempoMap, TimeSignature, PPQ};

    const BAR: Ticks = PPQ * 4;

    fn track(id: u32) -> Track {
        Track {
            id,
            name: format!("T{id}"),
            color: ClipColor::Teal,
            kind: TrackKind::Midi,
            mute: false,
            solo: false,
            arm: false,
            gain_db: 0.0,
            height: 56.0,
            instrument: None,
            effects: vec![],
            effect_slots: vec![],
            fx: crate::arrangement::EffectGraph::new(),
            drum_pads: Default::default(),
        }
    }

    /// A one-bar pattern (a note on every beat) looped from `start` for `bars`.
    fn clip(id: u32, track: u32, start: Ticks, bars: Ticks) -> Clip {
        let notes = (0..4).map(|b| MidiNote { start: b * PPQ, length: PPQ / 2, pitch: 60 + b as u8, velocity: 100 }).collect();
        Clip {
            id,
            track,
            start,
            length: bars * BAR,
            name: "Beat".into(),
            content: ClipContent::Midi { notes, loop_len: Some(BAR), link: None },
            recording: false,
            gain_db: 0.0,
            swing: 0.0,
        }
    }

    fn song() -> Arrangement {
        let mut arr = Arrangement::new(TempoMap::constant(120.0, TimeSignature::FOUR_FOUR));
        arr.tracks = vec![track(1), track(2)];
        arr.clips = vec![clip(10, 1, 0, 8), clip(11, 2, 0, 2), clip(12, 2, 10 * BAR, 2)];
        arr.markers = vec![
            Marker { id: 20, position: 3 * BAR, name: "gone".into() },
            Marker { id: 21, position: 6 * BAR, name: "Drop".into() },
        ];
        arr.loop_range = Some(LoopRange { start: 5 * BAR, end: 9 * BAR });
        // Ids handed out from here on don't collide with the ones above.
        while arr.alloc_id() < 100 {}
        arr
    }

    fn spans(arr: &Arrangement, track: u32) -> Vec<(Ticks, Ticks)> {
        let mut out: Vec<_> = arr.clips.iter().filter(|c| c.track == track).map(|c| (c.start / BAR, c.end() / BAR)).collect();
        out.sort();
        out
    }

    #[test]
    fn split_all_cuts_every_track_at_the_point() {
        let mut arr = song();
        for command in split_all_at(&mut arr, BAR) {
            command.apply(&mut arr);
        }
        assert_eq!(spans(&arr, 1), vec![(0, 1), (1, 8)]);
        assert_eq!(spans(&arr, 2), vec![(0, 1), (1, 2), (10, 12)]);
    }

    #[test]
    fn removing_bars_closes_the_gap_on_every_track() {
        let next = remove_time(&song(), 2 * BAR, 4 * BAR);
        // The long clip loses two bars from its middle; the short one ends
        // where the cut starts, so it stays; the later one moves back.
        assert_eq!(spans(&next, 1), vec![(0, 2), (2, 6)]);
        assert_eq!(spans(&next, 2), vec![(0, 2), (8, 10)]);
        // What was after the cut still plays the same notes.
        let right = next.clips.iter().find(|c| c.track == 1 && c.start == 2 * BAR).unwrap();
        assert_eq!(right.played_notes().first().map(|n| (n.start, n.pitch)), Some((0, 60)));
        assert_eq!(next.markers.iter().map(|m| (m.name.as_str(), m.position / BAR)).collect::<Vec<_>>(), vec![("Drop", 4)]);
        assert_eq!(next.loop_range, Some(LoopRange { start: 3 * BAR, end: 7 * BAR }));
    }

    #[test]
    fn a_loop_inside_the_removed_bars_goes_with_them() {
        let mut arr = song();
        arr.loop_range = Some(LoopRange { start: 2 * BAR, end: 3 * BAR });
        assert_eq!(remove_time(&arr, 2 * BAR, 4 * BAR).loop_range, None);
    }
}
