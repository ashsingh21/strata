//! Pure MIDI playback scheduling: given the arrangement and the tick range
//! the playhead just advanced through, which notes should turn on or off.
//! Stateless and side-effect free on purpose, so it's cheap to unit test;
//! the UI owns the actual note-on/off bookkeeping (see
//! `ui::timeline::scheduler`).

use super::model::{Arrangement, ClipContent, TrackId};
use super::time::Ticks;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScheduledNotes {
    /// (track, pitch, velocity) of every note starting in the range.
    pub note_on: Vec<(TrackId, u8, u8)>,
    /// (track, pitch) of every note ending in it.
    pub note_off: Vec<(TrackId, u8)>,
}

/// Notes whose start or end falls in `(from, to]`, across every audible
/// MIDI clip (respecting track mute/solo the way a mixer would: a muted
/// track is silent, and if any track is soloed only soloed tracks sound).
/// Assumes `to >= from`; returns nothing for an empty or reversed range.
pub fn notes_in_range(arr: &Arrangement, from: Ticks, to: Ticks) -> ScheduledNotes {
    let mut result = ScheduledNotes::default();
    if to <= from {
        return result;
    }

    let any_solo = arr.tracks.iter().any(|t| t.solo);
    for clip in &arr.clips {
        let ClipContent::Midi { notes } = &clip.content else { continue };
        let Some(track) = arr.track(clip.track) else { continue };
        if track.mute || (any_solo && !track.solo) {
            continue;
        }
        for note in notes {
            let abs_start = clip.start + note.start;
            let abs_end = abs_start + note.length;
            if abs_start > from && abs_start <= to {
                result.note_on.push((clip.track, note.pitch, note.velocity));
            }
            if abs_end > from && abs_end <= to {
                result.note_off.push((clip.track, note.pitch));
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangement::model::{Clip, ClipColor, MidiNote, Track, TrackKind, DEFAULT_VELOCITY};
    use crate::arrangement::time::{TempoMap, TimeSignature, PPQ};
    use crate::arrangement::Arrangement;

    fn arrangement_with_one_note(mute: bool, solo: bool) -> Arrangement {
        let mut arr = Arrangement::new(TempoMap::constant(120.0, TimeSignature::FOUR_FOUR));
        arr.tracks.push(Track {
            id: 1,
            name: "Lead".into(),
            color: ClipColor::Teal,
            kind: TrackKind::Midi,
            mute,
            solo,
            arm: false,
            gain_db: 0.0,
            height: 56.0,
            instrument: None,
        effects: vec![],
        });
        arr.clips.push(Clip {
            id: 1,
            track: 1,
            start: 0,
            length: PPQ * 4,
            name: "Clip".into(),
            content: ClipContent::Midi { notes: vec![MidiNote { start: PPQ, length: PPQ, pitch: 60, velocity: DEFAULT_VELOCITY }] },
            recording: false,
            gain_db: 0.0,
        });
        arr
    }

    #[test]
    fn note_start_triggers_on_within_range() {
        let arr = arrangement_with_one_note(false, false);
        let scheduled = notes_in_range(&arr, PPQ - 10, PPQ + 10);
        assert_eq!(scheduled.note_on, vec![(1, 60, DEFAULT_VELOCITY)]);
        assert!(scheduled.note_off.is_empty());
    }

    #[test]
    fn note_end_triggers_off_within_range() {
        let arr = arrangement_with_one_note(false, false);
        let scheduled = notes_in_range(&arr, PPQ * 2 - 10, PPQ * 2 + 10);
        assert!(scheduled.note_on.is_empty());
        assert_eq!(scheduled.note_off, vec![(1, 60)]);
    }

    #[test]
    fn outside_range_triggers_nothing() {
        let arr = arrangement_with_one_note(false, false);
        let scheduled = notes_in_range(&arr, 0, PPQ / 2);
        assert!(scheduled.note_on.is_empty() && scheduled.note_off.is_empty());
    }

    #[test]
    fn muted_track_is_silent() {
        let arr = arrangement_with_one_note(true, false);
        let scheduled = notes_in_range(&arr, PPQ - 10, PPQ + 10);
        assert!(scheduled.note_on.is_empty());
    }

    #[test]
    fn unsoloed_track_is_silent_when_another_is_soloed() {
        let mut arr = arrangement_with_one_note(false, false);
        arr.tracks.push(Track {
            id: 2,
            name: "Other".into(),
            color: ClipColor::Amber,
            kind: TrackKind::Midi,
            mute: false,
            solo: true,
            arm: false,
            gain_db: 0.0,
            height: 56.0,
            instrument: None,
        effects: vec![],
        });
        let scheduled = notes_in_range(&arr, PPQ - 10, PPQ + 10);
        assert!(scheduled.note_on.is_empty());
    }

    #[test]
    fn reversed_range_triggers_nothing() {
        let arr = arrangement_with_one_note(false, false);
        let scheduled = notes_in_range(&arr, PPQ + 10, PPQ - 10);
        assert!(scheduled.note_on.is_empty() && scheduled.note_off.is_empty());
    }
}
