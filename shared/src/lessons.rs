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
use crate::synth::{
    deep_rave_bass, Envelope, Filter, FilterType, Fx, Lfo, LfoTarget, Mix, Oscillator, SynthState, Unison,
    VoiceMode, Waveform,
};

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
pub const CARVE_WAVES: &str = "carve-waves";
pub const CARVE_MIX: &str = "carve-mix";
pub const CARVE_FILTER: &str = "carve-filter";
pub const CARVE_ENVELOPES: &str = "carve-envelopes";
pub const CARVE_MOVEMENT: &str = "carve-movement";
pub const RECIPE_BASS: &str = "recipe-bass";
pub const RECIPE_FLUTE: &str = "recipe-flute";
pub const RECIPE_HARP: &str = "recipe-harp";
pub const RECIPE_LEAD: &str = "recipe-lead";
pub const RECIPE_PAD: &str = "recipe-pad";
pub const RECIPE_TANPURA: &str = "recipe-tanpura";
pub const RECIPE_REED: &str = "recipe-reed";
/// The arrangement lessons open a finished demo song.
pub const ARRANGE_HOUSE: &str = "arrange-house";
pub const ARRANGE_BHAIRAV: &str = "arrange-bhairav";
/// "Your first house track", in five parts; each starts from the
/// previous part's finished result.
pub const PROJECT_GROOVE: &str = "project-groove";
pub const PROJECT_BASS: &str = "project-bass";
pub const PROJECT_CHORDS: &str = "project-chords";
pub const PROJECT_ARRANGE: &str = "project-arrange";
pub const PROJECT_FINISH: &str = "project-finish";

/// The project's bassline: off-beat roots, one bar each of A, C, D, C.
pub const PROJECT_BASS_ROOTS: [u8; 4] = [57, 60, 62, 60];
/// The project's chords, one stab per bar: Am, C, Dsus4, C - all in A
/// minor pentatonic, so every note is on a row the piano roll shows.
pub const PROJECT_CHORDS_NOTES: [[u8; 3]; 4] = [[57, 60, 64], [60, 64, 67], [62, 67, 69], [60, 64, 67]];

/// Carve lessons loop their riff this long, so there's time to turn knobs.
const CARVE_BARS: i64 = 64;

/// A blank-slate Carve patch for the synth lessons: one plain saw, the
/// filter wide open, no envelope movement, no LFOs, no effects - so every
/// change a lesson asks for is clearly audible on its own.
pub fn init_patch() -> SynthState {
    SynthState {
        name: "Init",
        voice_mode: VoiceMode::Poly,
        voices: 8,
        osc1: Oscillator { waveform: Waveform::Saw, octave: 0, knob_a_cents: 0.0, knob_b: 0.0, knob_c: 0.0, sync: false },
        osc2: Oscillator { waveform: Waveform::Saw, octave: 0, knob_a_cents: 0.0, knob_b: 0.5, knob_c: 0.0, sync: false },
        mix: Mix { osc1_db: -6.0, osc2_db: -60.0, sub_db: -60.0, noise_db: -60.0 },
        filter: Filter {
            filter_type: FilterType::Lp24,
            cutoff_hz: 18_000.0,
            resonance: 0.1,
            drive_db: 0.0,
            env_amount_oct: 0.0,
            key_track: 0.0,
        },
        filter_env: Envelope { attack_ms: 1.0, decay_ms: 400.0, sustain: 0.0, release_ms: 200.0 },
        amp_env: Envelope { attack_ms: 8.0, decay_ms: 200.0, sustain: 1.0, release_ms: 150.0 },
        lfo1: Lfo { rate_label: "", rate_norm: 0.6, depth: 0.0, sync: false, target: LfoTarget::PulseWidth, target_count: 1 },
        lfo2: Lfo { rate_label: "", rate_norm: 0.3, depth: 0.0, sync: false, target: LfoTarget::Pitch, target_count: 1 },
        output: crate::synth::Output { glide_ms: 1.0, volume_db: -6.0, meter_l: 0.0, meter_r: 0.0 },
        unison: Unison { voices: 1, detune_cents: 12.0, width: 0.6 },
        fx: Fx { chorus_depth: 0.4, chorus_mix: 0.0, reverb_size: 0.5, reverb_mix: 0.0 },
        held_notes: vec![],
    }
}

/// (16th, pitch, length in 16ths) -> notes.
fn steps(pattern: &[(i64, u8, i64)]) -> Vec<MidiNote> {
    pattern.iter().map(|&(at, pitch, len)| MidiNote { start: at * SIXTEENTH, length: len * SIXTEENTH, pitch, velocity: 100 }).collect()
}

/// Each Carve lesson's riff and pattern length in bars.
fn carve_riff(lesson: &str) -> (&'static str, Vec<MidiNote>, i64) {
    match lesson {
        // Long notes: waves and detuning are easiest to hear held.
        CARVE_WAVES => ("Held note", steps(&[(0, 57, 14)]), 1),
        CARVE_MIX => ("Held note", steps(&[(0, 45, 14)]), 1),
        // Repeated eighths: filter and envelope changes show on every hit.
        CARVE_FILTER | CARVE_ENVELOPES => (
            "Riff",
            steps(&[(0, 45, 2), (2, 45, 2), (4, 57, 2), (6, 45, 2), (8, 48, 2), (10, 45, 2), (12, 55, 2), (14, 45, 2)]),
            1,
        ),
        // Held chords, A minor then F: slow movement (and a pad's slow
        // swell) needs time to show.
        CARVE_MOVEMENT | RECIPE_PAD => {
            let mut notes = steps(&[(0, 57, 15), (0, 60, 15), (0, 64, 15)]);
            notes.extend(steps(&[(16, 53, 15), (16, 57, 15), (16, 60, 15)]));
            ("Chords", notes, 2)
        }
        RECIPE_BASS => ("Bassline", steps(&[(2, 45, 1), (6, 45, 1), (10, 45, 1), (14, 57, 1)]), 1),
        // Slow and singing, in A minor pentatonic.
        RECIPE_FLUTE => ("Melody", steps(&[(0, 69, 3), (4, 72, 3), (8, 74, 7), (16, 76, 11), (28, 74, 3)]), 2),
        // Raga Malkauns in A (A C D F G): a descending cascade, then a phrase.
        RECIPE_HARP => (
            "Cascade",
            steps(&[
                (0, 81, 1), (1, 79, 1), (2, 77, 1), (3, 74, 1), (4, 72, 1), (5, 69, 1), (6, 67, 1), (7, 65, 1),
                (8, 62, 1), (9, 60, 1), (10, 57, 6),
                (16, 57, 3), (20, 60, 2), (22, 62, 2), (24, 65, 7),
            ]),
            2,
        ),
        // The tanpura's cycle: Pa, Sa, Sa, then low Sa - one string per
        // beat, each left to ring into the next (Sa on C).
        RECIPE_TANPURA => ("Tanpura cycle", steps(&[(0, 55, 4), (4, 60, 4), (8, 60, 4), (12, 48, 4)]), 1),
        // Raag Bhairav's pakad with Sa on C5: Ga ma dha Pa, Ga ma re Sa.
        RECIPE_REED => (
            "Pakad",
            steps(&[(0, 76, 3), (4, 77, 3), (8, 80, 6), (14, 79, 2), (16, 76, 3), (20, 77, 3), (24, 73, 5), (29, 72, 3)]),
            2,
        ),
        RECIPE_LEAD => (
            "Hook",
            steps(&[
                (0, 81, 2), (3, 79, 1), (4, 76, 2), (6, 74, 2), (8, 76, 3), (11, 72, 1), (12, 74, 2), (14, 76, 2),
                (16, 72, 3), (19, 74, 1), (20, 76, 2), (22, 72, 2), (24, 69, 6),
            ]),
            2,
        ),
        _ => ("Clip", Vec::new(), 1),
    }
}

/// The Carve lessons: a synth track (after a drum track, for the recipes
/// that sit in a beat) with the Init patch, looping a riff.
fn carve_lesson(lesson: &str) -> Project {
    let mut arr = empty_arrangement();
    if lesson == RECIPE_BASS || lesson == RECIPE_LEAD {
        let drums = add_track(&mut arr, "Drums", ClipColor::Coral, Instrument::Drums, -8.0);
        add_loop(&mut arr, drums, "Beat", lesson_one_beat(), 1, CARVE_BARS);
    }
    let synth = add_track(&mut arr, "Carve", ClipColor::Violet, Instrument::Carve, -4.0);
    let (name, notes, pattern_bars) = carve_riff(lesson);
    add_loop(&mut arr, synth, name, notes, pattern_bars, CARVE_BARS);
    Project { arrangement: arr, instruments: vec![(synth, init_patch())], synth: None }
}

/// The project `lesson` starts from (a blank one for an unknown id).
pub fn starting_project(lesson: &str) -> Project {
    match lesson {
        ARRANGE_HOUSE => return crate::demo::house_demo(),
        ARRANGE_BHAIRAV => return crate::demo::bhairav_demo(),
        PROJECT_GROOVE => return project_after(0),
        PROJECT_BASS => return project_after(1),
        PROJECT_CHORDS => return project_after(2),
        PROJECT_ARRANGE => return project_after(3),
        PROJECT_FINISH => return project_after(4),
        _ => {}
    }
    if lesson.starts_with("carve-") || lesson.starts_with("recipe-") {
        return carve_lesson(lesson);
    }
    let mut arr = empty_arrangement();
    let mut instruments = Vec::new();
    if lesson == BASSLINE || lesson == CHORDS {
        let drums = add_track(&mut arr, "Drums", ClipColor::Coral, Instrument::Drums, -6.0);
        add_loop(&mut arr, drums, "Beat", lesson_one_beat(), 1, PART_BARS);
    }
    if lesson == CHORDS {
        let bass = add_track(&mut arr, "Bass", ClipColor::Blue, Instrument::Carve, -7.0);
        add_loop(&mut arr, bass, "Bassline", lesson_two_bassline(), 1, PART_BARS);
        instruments.push((bass, deep_rave_bass()));
    }
    Project { arrangement: arr, instruments, synth: None }
}

/// The house-track project as it stands after `parts` parts - what the
/// next part starts from, so any part can be taken on its own.
pub fn project_after(parts: usize) -> Project {
    let mut arr = empty_arrangement();
    let mut instruments = Vec::new();
    if parts >= 1 {
        let drums = add_track(&mut arr, "Drums", ClipColor::Coral, Instrument::Drums, -6.0);
        if parts >= 4 {
            add_clip(&mut arr, drums, "Beat", 0, 16, 1, project_groove());
            add_clip(&mut arr, drums, "Beat", 24, 32, 1, project_groove());
        } else {
            add_clip(&mut arr, drums, "Beat", 0, 16, 1, project_groove());
        }
    }
    if parts >= 2 {
        let bass = add_track(&mut arr, "Bass", ClipColor::Blue, Instrument::Carve, -7.0);
        if parts >= 4 {
            add_clip(&mut arr, bass, "Bassline", 4, 16, 4, project_bassline());
            add_clip(&mut arr, bass, "Bassline", 24, 32, 4, project_bassline());
        } else {
            add_clip(&mut arr, bass, "Bassline", 4, 16, 4, project_bassline());
        }
        instruments.push((bass, crate::synth::recipes::deep_bass()));
    }
    if parts >= 3 {
        let chords = add_track(&mut arr, "Chords", ClipColor::Violet, Instrument::Carve, if parts >= 5 { -10.0 } else { 0.0 });
        let end = if parts >= 4 { 32 } else { 16 };
        add_clip(&mut arr, chords, "Chords", 8, end, 4, project_chords());
        instruments.push((chords, crate::synth::soft_pad()));
    }
    if parts >= 4 {
        for (name, bar) in [("Intro", 0), ("Groove", 8), ("Breakdown", 16), ("Drop", 24)] {
            let id = arr.alloc_id();
            arr.markers.push(crate::arrangement::Marker { id, position: bar * BAR, name: name.into() });
        }
    }
    Project { arrangement: arr, instruments, synth: None }
}

/// The project's beat: kick on every beat, clap on 2 and 4, closed hats
/// on the "e" and "a", open hat on the "and".
pub fn project_groove() -> Vec<MidiNote> {
    let hit = |start, pitch, velocity| MidiNote { start, length: SIXTEENTH, pitch, velocity };
    let mut notes = Vec::new();
    for beat in 0..4 {
        let at = beat * PPQ;
        notes.push(hit(at, KICK, 127));
        notes.push(hit(at + SIXTEENTH, crate::drums::CLOSED_HAT, 70));
        notes.push(hit(at + PPQ / 2, OPEN_HAT, 70));
        notes.push(hit(at + 3 * SIXTEENTH, crate::drums::CLOSED_HAT, 55));
        if beat % 2 == 1 {
            notes.push(hit(at, CLAP, 100));
        }
    }
    notes
}

/// Off-beat roots following the chords, four bars.
pub fn project_bassline() -> Vec<MidiNote> {
    PROJECT_BASS_ROOTS
        .iter()
        .enumerate()
        .flat_map(|(bar, &pitch)| {
            (0..4).map(move |beat| MidiNote {
                start: bar as i64 * BAR + beat * PPQ + PPQ / 2,
                length: SIXTEENTH,
                pitch,
                velocity: 100,
            })
        })
        .collect()
}

/// One chord stab per bar, on the downbeat.
pub fn project_chords() -> Vec<MidiNote> {
    PROJECT_CHORDS_NOTES
        .iter()
        .enumerate()
        .flat_map(|(bar, chord)| {
            chord.iter().map(move |&pitch| MidiNote { start: bar as i64 * BAR, length: SIXTEENTH, pitch, velocity: 100 })
        })
        .collect()
}

/// A clip from `start_bar` to `end_bar` looping a `pattern_bars` pattern.
fn add_clip(arr: &mut Arrangement, track: TrackId, name: &str, start_bar: i64, end_bar: i64, pattern_bars: i64, notes: Vec<MidiNote>) {
    let id = arr.alloc_id();
    arr.clips.push(Clip {
        id,
        track,
        start: start_bar * BAR,
        length: (end_bar - start_bar) * BAR,
        name: name.into(),
        content: ClipContent::Midi { notes, loop_len: Some(pattern_bars * BAR), link: None },
        recording: false,
        gain_db: 0.0,
    });
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

/// A `pattern_bars` pattern looping for `bars` bars from the start.
fn add_loop(arr: &mut Arrangement, track: TrackId, name: &str, notes: Vec<MidiNote>, pattern_bars: i64, bars: i64) {
    let id = arr.alloc_id();
    arr.clips.push(Clip {
        id,
        track,
        start: 0,
        length: bars * BAR,
        name: name.into(),
        content: ClipContent::Midi { notes, loop_len: Some(pattern_bars * BAR), link: None },
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
    fn carve_lessons_start_on_the_init_patch_with_a_riff() {
        for id in [CARVE_WAVES, CARVE_MIX, CARVE_FILTER, CARVE_ENVELOPES, CARVE_MOVEMENT, RECIPE_BASS, RECIPE_FLUTE, RECIPE_HARP, RECIPE_LEAD, RECIPE_PAD, RECIPE_TANPURA, RECIPE_REED] {
            let p = starting_project(id);
            let synth = p.arrangement.tracks.last().unwrap();
            assert_eq!(synth.instrument, Some(Instrument::Carve), "{id}");
            assert_eq!(p.instruments, vec![(synth.id, init_patch())], "{id}");
            let clip = p.arrangement.clips.iter().find(|c| c.track == synth.id).unwrap();
            assert!(!clip.played_notes().is_empty(), "{id} has no riff");
        }
        // The harp cascade stays in raga Malkauns (A C D F G).
        let harp = starting_project(RECIPE_HARP);
        let ClipContent::Midi { notes, .. } = &harp.arrangement.clips[0].content else { panic!() };
        assert!(notes.iter().all(|n| [9, 0, 2, 5, 7].contains(&(n.pitch % 12))));
    }

    #[test]
    fn starting_projects_round_trip_through_the_save_format() {
        for id in [FIRST_BEAT, BASSLINE, CHORDS, CARVE_WAVES, RECIPE_HARP, RECIPE_LEAD] {
            let p = starting_project(id);
            let back: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
            assert_eq!(back.arrangement.clips.len(), p.arrangement.clips.len(), "{id}");
        }
    }

    #[test]
    fn each_project_part_starts_where_the_last_ended() {
        let tracks = |n| project_after(n).arrangement.tracks.iter().map(|t| t.name.clone()).collect::<Vec<_>>();
        assert!(tracks(0).is_empty());
        assert_eq!(tracks(1), ["Drums"]);
        assert_eq!(tracks(3), ["Drums", "Bass", "Chords"]);
        let after_arrange = project_after(4);
        assert_eq!(after_arrange.arrangement.markers.len(), 4);
        // The breakdown (bars 17-24) has no drums or bass.
        for clip in &after_arrange.arrangement.clips {
            let name = &after_arrange.arrangement.track(clip.track).unwrap().name;
            if name == "Drums" || name == "Bass" {
                assert!(clip.end() <= 16 * BAR || clip.start >= 24 * BAR, "{name} plays in the breakdown");
            }
        }
    }
}
