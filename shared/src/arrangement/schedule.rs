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

    for clip in &arr.clips {
        if !matches!(clip.content, ClipContent::Midi { .. }) {
            continue;
        }
        let Some(track) = arr.track(clip.track) else { continue };
        if !arr.is_audible(track) {
            continue;
        }
        // Skip clips entirely outside the range before expanding loops.
        if clip.end() < from || clip.start > to {
            continue;
        }
        for note in &clip.played_notes() {
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

/// One note on or off at an exact tick (see `timed_notes_in_range`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimedNote {
    pub tick: Ticks,
    pub track: TrackId,
    pub pitch: u8,
    pub velocity: u8,
    pub on: bool,
}

/// `notes_in_range`, with each event's own tick, sorted by time (a note-off
/// before a note-on at the same tick, so a repeated note retriggers). For
/// offline rendering, where every note can land on its exact sample.
pub fn timed_notes_in_range(arr: &Arrangement, from: Ticks, to: Ticks) -> Vec<TimedNote> {
    let mut events = Vec::new();
    if to <= from {
        return events;
    }
    for clip in &arr.clips {
        if !matches!(clip.content, ClipContent::Midi { .. }) || clip.end() < from || clip.start > to {
            continue;
        }
        let Some(track) = arr.track(clip.track) else { continue };
        if !arr.is_audible(track) {
            continue;
        }
        for note in &clip.played_notes() {
            let start = clip.start + note.start;
            let end = start + note.length;
            if start > from && start <= to {
                events.push(TimedNote { tick: start, track: clip.track, pitch: note.pitch, velocity: note.velocity, on: true });
            }
            if end > from && end <= to {
                events.push(TimedNote { tick: end, track: clip.track, pitch: note.pitch, velocity: 0, on: false });
            }
        }
    }
    events.sort_by_key(|e| (e.tick, e.on));
    events
}

/// Which wheel a control change is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wheel {
    Bend,
    Mod,
}

/// Where in `clip`'s own time song tick `tick` falls (a looping clip's
/// pattern repeats), or `None` outside it.
fn clip_time(clip: &super::model::Clip, tick: Ticks) -> Option<Ticks> {
    if tick < clip.start || tick >= clip.end() {
        return None;
    }
    let ClipContent::Midi { loop_len, .. } = &clip.content else { return None };
    let local = tick - clip.start;
    Some(match loop_len {
        Some(len) if *len > 0 => local % len,
        _ => local,
    })
}

/// Wheel moves the playhead crossed in `(from, to]`: (track, wheel,
/// value), in time order, across every audible MIDI clip.
pub fn controls_in_range(arr: &Arrangement, from: Ticks, to: Ticks) -> Vec<(TrackId, Wheel, f32)> {
    let mut out: Vec<(Ticks, TrackId, Wheel, f32)> = Vec::new();
    if to <= from {
        return Vec::new();
    }
    for clip in &arr.clips {
        let ClipContent::Midi { controls, loop_len, .. } = &clip.content else { continue };
        if controls.is_empty() || clip.end() < from || clip.start > to {
            continue;
        }
        let Some(track) = arr.track(clip.track) else { continue };
        if !arr.is_audible(track) {
            continue;
        }
        let period = loop_len.filter(|l| *l > 0).unwrap_or(clip.length.max(1));
        for (wheel, points) in [(Wheel::Bend, &controls.bend), (Wheel::Mod, &controls.modwheel)] {
            for p in points.iter().filter(|p| p.tick < period) {
                // Every repeat of the point that lands in the range.
                let mut at = clip.start + p.tick;
                while at <= to && at < clip.end() {
                    if at > from {
                        out.push((at, clip.track, wheel, p.value));
                    }
                    at += period;
                }
            }
        }
    }
    out.sort_by_key(|e| e.0);
    out.into_iter().map(|(_, t, w, v)| (t, w, v)).collect()
}

/// Each audible MIDI track's wheels at `tick` (where playback starts or
/// jumps to): from the clip under it, or at rest.
pub fn controls_at(arr: &Arrangement, tick: Ticks) -> Vec<(TrackId, f32, f32)> {
    arr.tracks
        .iter()
        .filter(|t| t.kind == super::model::TrackKind::Midi && arr.is_audible(t))
        .map(|t| {
            let here = arr.clips.iter().filter(|c| c.track == t.id).find_map(|c| {
                let ClipContent::Midi { controls, .. } = &c.content else { return None };
                clip_time(c, tick).map(|local| controls.at(local))
            });
            let (bend, modwheel) = here.unwrap_or((0.0, 0.0));
            (t.id, bend, modwheel)
        })
        .collect()
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
        effect_slots: vec![],
        fx: crate::arrangement::EffectGraph::new(),
        drum_pads: Default::default(),
        });
        arr.clips.push(Clip {
            id: 1,
            track: 1,
            start: 0,
            length: PPQ * 4,
            name: "Clip".into(),
            content: ClipContent::Midi { notes: vec![MidiNote { start: PPQ, length: PPQ, pitch: 60, velocity: DEFAULT_VELOCITY }], loop_len: None, link: None, controls: Default::default() },
            recording: false,
            gain_db: 0.0,
            swing: 0.0,
        });
        arr
    }

    #[test]
    fn wheel_moves_play_back_and_repeat_with_a_loop() {
        use crate::arrangement::model::ControlPoint;
        let mut arr = arrangement_with_one_note(false, false);
        if let ClipContent::Midi { controls, loop_len, .. } = &mut arr.clips[0].content {
            controls.bend = vec![ControlPoint { tick: PPQ, value: 0.5 }];
            *loop_len = Some(PPQ * 2);
        }
        // The point at beat 1 of a 2-beat loop: heard at beats 1 and 3.
        let moves = controls_in_range(&arr, 0, PPQ * 4);
        assert_eq!(moves, vec![(1, Wheel::Bend, 0.5), (1, Wheel::Bend, 0.5)]);
        assert_eq!(controls_at(&arr, PPQ / 2), vec![(1, 0.0, 0.0)]);
        assert_eq!(controls_at(&arr, PPQ * 3 + 1), vec![(1, 0.5, 0.0)]);
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
        effect_slots: vec![],
        fx: crate::arrangement::EffectGraph::new(),
        drum_pads: Default::default(),
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

    #[test]
    fn a_looping_clip_schedules_its_repeats() {
        let mut arr = arrangement_with_one_note(false, false);
        // The one note (at beat 1) in a one-bar loop, clip two bars long.
        let clip = &mut arr.clips[0];
        clip.length = PPQ * 8;
        if let ClipContent::Midi { loop_len, .. } = &mut clip.content {
            *loop_len = Some(PPQ * 4);
        }
        let start = clip.start;
        let ons = notes_in_range(&arr, start, start + PPQ * 8).note_on.len();
        assert_eq!(ons, 2);
        let second_bar = notes_in_range(&arr, start + PPQ * 4, start + PPQ * 6).note_on.len();
        assert_eq!(second_bar, 1);
    }
}