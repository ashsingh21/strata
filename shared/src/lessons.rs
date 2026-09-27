//! Starting projects for the built-in lessons (the course itself - steps,
//! checks, text - lives in the UI crate, which owns the state it checks).
//! Each lesson opens a known project so its steps can check exact
//! things: lesson 1 starts blank, lesson 2 with lesson 1's finished beat,
//! lesson 3 with the beat and lesson 2's bassline.

use crate::arrangement::{
    empty_arrangement, Arrangement, Clip, ClipColor, ClipContent, EffectGraph, Instrument, MidiNote, Ticks, Track,
    TrackId, TrackKind, DEFAULT_TRACK_HEIGHT, PPQ,
};
use crate::drums::{CLAP, KICK, OPEN_HAT};
use crate::project::Project;
use crate::synth::deep_rave_bass;

pub const BAR: Ticks = PPQ * 4;
pub const SIXTEENTH: Ticks = PPQ / 4;
/// The bass note lesson 2 writes (and lesson 3 starts with): A3, the
/// piano roll's bottom row for an empty clip in A.
pub const BASS_NOTE: u8 = 57;
/// How long the pre-built parts loop for.
const PART_BARS: i64 = 8;

/// Lesson ids, in course order - the UI's lesson table uses the same ids.
pub const FIRST_BEAT: &str = "first-beat";
pub const BASSLINE: &str = "bassline";
pub const CHORDS: &str = "chords";

/// The project `lesson` starts from (a blank one for an unknown id).
pub fn starting_project(lesson: &str) -> Project {
    let mut arr = empty_arrangement();
    let mut instruments = Vec::new();
    if lesson == BASSLINE || lesson == CHORDS {
        let drums = add_track(&mut arr, "Drums", ClipColor::Coral, Instrument::Drums, -6.0);
        add_loop(&mut arr, drums, "Beat", lesson_one_beat());
    }
    if lesson == CHORDS {
        let bass = add_track(&mut arr, "Bass", ClipColor::Blue, Instrument::Carve, -7.0);
        add_loop(&mut arr, bass, "Bassline", lesson_two_bassline());
        instruments.push((bass, deep_rave_bass()));
    }
    Project { arrangement: arr, instruments, synth: None }
}

/// Lesson 1's beat: kick on every beat, clap on 2 and 4, open hat on
/// every "and".
pub fn lesson_one_beat() -> Vec<MidiNote> {
    let hit = |start, pitch, velocity| MidiNote { start, length: SIXTEENTH, pitch, velocity };
    let mut notes = Vec::new();
    for beat in 0..4 {
        notes.push(hit(beat * PPQ, KICK, 127));
        notes.push(hit(beat * PPQ + PPQ / 2, OPEN_HAT, 70));
        if beat % 2 == 1 {
            notes.push(hit(beat * PPQ, CLAP, 100));
        }
    }
    notes
}

/// Lesson 2's bassline: one note on every off-beat.
pub fn lesson_two_bassline() -> Vec<MidiNote> {
    (0..4).map(|beat| MidiNote { start: beat * PPQ + PPQ / 2, length: SIXTEENTH, pitch: BASS_NOTE, velocity: 100 }).collect()
}

fn add_track(arr: &mut Arrangement, name: &str, color: ClipColor, instrument: Instrument, gain_db: f32) -> TrackId {
    let id = arr.alloc_id();
    arr.tracks.push(Track {
        id,
        name: name.into(),
        color,
        kind: TrackKind::Midi,
        mute: false,
        solo: false,
        arm: false,
        gain_db,
        height: DEFAULT_TRACK_HEIGHT,
        instrument: Some(instrument),
        effects: vec![],
        effect_slots: vec![],
        fx: EffectGraph::new(),
    });
    id
}

/// A one-bar pattern looping for `PART_BARS` bars from the start.
fn add_loop(arr: &mut Arrangement, track: TrackId, name: &str, notes: Vec<MidiNote>) {
    let id = arr.alloc_id();
    arr.clips.push(Clip {
        id,
        track,
        start: 0,
        length: PART_BARS * BAR,
        name: name.into(),
        content: ClipContent::Midi { notes, loop_len: Some(BAR), link: None },
        recording: false,
        gain_db: 0.0,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_lesson_starts_where_the_last_left_off() {
        let one = starting_project(FIRST_BEAT);
        assert!(one.arrangement.tracks.is_empty());

        let two = starting_project(BASSLINE);
        assert_eq!(two.arrangement.tracks.len(), 1);
        assert_eq!(two.arrangement.tracks[0].instrument, Some(Instrument::Drums));
        assert_eq!(two.arrangement.clips[0].played_notes().len(), 8 * 10);

        let three = starting_project(CHORDS);
        assert_eq!(three.arrangement.tracks.len(), 2);
        let bass = &three.arrangement.tracks[1];
        assert_eq!(bass.instrument, Some(Instrument::Carve));
        assert_eq!(three.instruments.len(), 1);
        assert_eq!(three.instruments[0].0, bass.id);
    }

    #[test]
    fn starting_projects_round_trip_through_the_save_format() {
        for id in [FIRST_BEAT, BASSLINE, CHORDS] {
            let p = starting_project(id);
            let back: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
            assert_eq!(back.arrangement.clips.len(), p.arrangement.clips.len(), "{id}");
        }
    }
}
