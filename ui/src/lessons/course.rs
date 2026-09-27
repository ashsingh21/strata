//! The course: every lesson's steps, their text, what each one checks and
//! which control glows. Text lives only here, so translating the course
//! is a change to this table.

use shared::arrangement::{Clip, ClipContent, Instrument, Ticks, PPQ};
use shared::drums::{CLAP, KICK, OPEN_HAT};
use shared::lessons::{BAR, BASSLINE, BASS_NOTE, CHORDS, FIRST_BEAT};

use super::{selected, tracks_with, Snapshot, Target};

pub enum Kind {
    /// Done when `check` passes; `target` says what glows meanwhile.
    Action { check: fn(&Snapshot) -> bool, target: fn(&Snapshot) -> Option<Target> },
    /// Read, then Continue.
    Info,
}

pub struct Step {
    pub text: &'static str,
    /// Shown if the step hasn't been done after a while.
    pub hint: &'static str,
    pub kind: Kind,
}

pub struct Lesson {
    /// Matches `shared::lessons` (starting project) and the saved "done" list.
    pub id: &'static str,
    pub title: &'static str,
    pub steps: &'static [Step],
}

const fn act(text: &'static str, hint: &'static str, check: fn(&Snapshot) -> bool, target: fn(&Snapshot) -> Option<Target>) -> Step {
    Step { text, hint, kind: Kind::Action { check, target } }
}

const fn info(text: &'static str) -> Step {
    Step { text, hint: "", kind: Kind::Info }
}

pub const LESSONS: &[Lesson] = &[
    Lesson {
        id: FIRST_BEAT,
        title: "Your first beat",
        steps: &[
            act(
                "Add a drum track: click \u{201c}+ Drums\u{201d} under the tracks.",
                "It's below the track list, on the left of the timeline.",
                |s| tracks_with(s, Instrument::Drums).next().is_some(),
                |_| Some(Target::AddDrumTrack),
            ),
            act(
                "Make a clip: double-click bar 1 of the Drums track.",
                "Two quick clicks on the empty lane, right of the track's name. A one-bar clip appears and opens below.",
                |s| clips_on(s, Instrument::Drums).next().is_some(),
                |s| tracks_with(s, Instrument::Drums).next().map(|t| Target::Lane(t.id)),
            ),
            act(
                "Kick on every beat: in the grid below, click the Kick row at 1, 2, 3 and 4.",
                "The numbers along the top are beats. Grid not showing? Double-click the clip to open it.",
                |s| drum_pattern_has(s, KICK, &[0, PPQ, 2 * PPQ, 3 * PPQ]),
                |s| row_or_clip(s, Instrument::Drums, KICK),
            ),
            act(
                "Press Space to hear it. (Space again stops.)",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Clap on beats 2 and 4: click the Clap row under 2 and 4.",
                "Beats 2 and 4 are the claps in almost every house and pop beat.",
                |s| drum_pattern_has(s, CLAP, &[PPQ, 3 * PPQ]),
                |s| row_or_clip(s, Instrument::Drums, CLAP),
            ),
            act(
                "Open hat between the beats: click the Open Hat row halfway between each beat.",
                "Halfway is two squares after each beat number (the \u{201c}and\u{201d}: 1-and, 2-and...).",
                |s| drum_pattern_has(s, OPEN_HAT, &[PPQ / 2, PPQ + PPQ / 2, 2 * PPQ + PPQ / 2, 3 * PPQ + PPQ / 2]),
                |s| row_or_clip(s, Instrument::Drums, OPEN_HAT),
            ),
            act(
                "Make it loop: in the timeline, drag the clip's right edge out to bar 9.",
                "Grab the very end of the clip. Stretching it repeats the bar you wrote.",
                |s| clips_on(s, Instrument::Drums).any(|c| loops(c, 4)),
                |s| tracks_with(s, Instrument::Drums).next().map(|t| Target::Lane(t.id)),
            ),
            info(
                "That's a house beat: kick on every beat, clap on 2 and 4, open hat in between. \
                 Everything else in a house track sits on top of this.",
            ),
        ],
    },
    Lesson {
        id: BASSLINE,
        title: "A bassline",
        steps: &[
            act(
                "Your beat is ready. For a bass, click \u{201c}+ MIDI track\u{201d}.",
                "It's below the track list, on the left of the timeline.",
                |s| tracks_with(s, Instrument::Carve).next().is_some(),
                |_| Some(Target::AddMidiTrack),
            ),
            act(
                "A MIDI track holds notes; its instrument turns them into sound. New ones play Carve, a synth. Pick a bass sound: Presets \u{2192} Deep Rave Bass.",
                "Presets are in the sidebar, under Instruments.",
                |s| selected(s).is_some_and(|t| t.instrument == Some(Instrument::Carve)) && s.synth.name == "Deep Rave Bass",
                |_| Some(Target::SidebarPreset("Deep Rave Bass")),
            ),
            act(
                "Try it: press Z, X or C on your computer keyboard.",
                "The bottom row of letters plays notes, like piano keys.",
                |s| !s.synth.held_notes.is_empty(),
                |_| None,
            ),
            act(
                "Make a clip: double-click bar 1 of the bass track.",
                "Two quick clicks on the empty lane. A one-bar clip appears and opens below.",
                |s| clips_on(s, Instrument::Carve).next().is_some(),
                |s| s.selected_track.map(Target::Lane),
            ),
            act(
                "Bass between the kicks: on the bottom row (A), click halfway between each beat.",
                "Halfway is two squares after each beat number - the same places as the open hat.",
                |s| carve_pattern_has(s, BASS_NOTE, &[PPQ / 2, PPQ + PPQ / 2, 2 * PPQ + PPQ / 2, 3 * PPQ + PPQ / 2]),
                |s| row_or_clip(s, Instrument::Carve, BASS_NOTE),
            ),
            act("Press Space to hear it with the drums.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            act(
                "Loop it: drag the bass clip's right edge out to bar 9.",
                "Grab the very end of the clip, like you did with the beat.",
                |s| clips_on(s, Instrument::Carve).any(|c| loops(c, 4)),
                |s| s.selected_track.map(Target::Lane),
            ),
            info(
                "Bass on the off-beats, between the kicks, is what makes house bounce. \
                 Try moving one: click it away, then click the C or E row in its place.",
            ),
        ],
    },
    Lesson {
        id: CHORDS,
        title: "Chords",
        steps: &[
            act(
                "Drums and bass are ready. Add one more MIDI track for chords.",
                "\u{201c}+ MIDI track\u{201d}, below the track list.",
                |s| tracks_with(s, Instrument::Carve).count() >= 2,
                |_| Some(Target::AddMidiTrack),
            ),
            act(
                "Pick a soft sound for chords: Presets \u{2192} Soft Pad.",
                "Presets are in the sidebar, under Instruments.",
                |s| selected(s).is_some_and(|t| t.instrument == Some(Instrument::Carve) && t.name != "Bass") && s.synth.name == "Soft Pad",
                |_| Some(Target::SidebarPreset("Soft Pad")),
            ),
            act(
                "Make a clip: double-click bar 1 of the new track.",
                "Two quick clicks on the empty lane. A one-bar clip appears and opens below.",
                |s| chord_clips(s).next().is_some(),
                |s| s.selected_track.map(Target::Lane),
            ),
            act(
                "Two chords need two bars: click + next to Pattern, above the grid.",
                "Pattern sets how long the part is before it repeats.",
                |s| chord_clips(s).any(|c| c.content_len() == 2 * BAR),
                |_| Some(Target::PatternPlus),
            ),
            act(
                "A minor: on beats 1 and 3 of bar 1, click A, C and E (stacked).",
                "Beats 1 and 3 are the 1.1 and 1.3 marks. A is the bottom row.",
                |s| chord_at(s, &[57, 60, 64], &[0, 2 * PPQ]),
                |s| if s.open_clip.is_none() { s.selected_track.map(Target::Lane) } else { first_missing(s, &[57, 60, 64], &[0, 2 * PPQ]).map(Target::PianoRollRow) },
            ),
            act(
                "C major: on beats 1 and 3 of bar 2, click C, E and G.",
                "Bar 2 starts at the 2 mark.",
                |s| chord_at(s, &[60, 64, 67], &[4 * PPQ, 6 * PPQ]),
                |s| if s.open_clip.is_none() { s.selected_track.map(Target::Lane) } else { first_missing(s, &[60, 64, 67], &[4 * PPQ, 6 * PPQ]).map(Target::PianoRollRow) },
            ),
            act("Press Space to hear all three parts.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            info(
                "Two chords, four hits. A minor and C major share two notes (C and E), \
                 which is why one flows so smoothly into the other.",
            ),
        ],
    },
];

/// Clips on tracks playing `instrument`.
fn clips_on(s: &Snapshot, instrument: Instrument) -> impl Iterator<Item = &Clip> {
    let tracks: Vec<_> = tracks_with(s, instrument).map(|t| t.id).collect();
    s.arrangement.clips.iter().filter(move |c| tracks.contains(&c.track) && matches!(c.content, ClipContent::Midi { .. }))
}

/// Lesson 3's clips: on a Carve track that isn't the pre-built bass.
fn chord_clips(s: &Snapshot) -> impl Iterator<Item = &Clip> {
    let tracks: Vec<_> = tracks_with(s, Instrument::Carve).filter(|t| t.name != "Bass").map(|t| t.id).collect();
    s.arrangement.clips.iter().filter(move |c| tracks.contains(&c.track))
}

/// The row to click - or, with no clip open in the editor, the lane
/// holding the clip (double-clicking it opens the grid).
fn row_or_clip(s: &Snapshot, instrument: Instrument, pitch: u8) -> Option<Target> {
    if s.open_clip.is_some() {
        return Some(Target::PianoRollRow(pitch));
    }
    clips_on(s, instrument).next().map(|c| Target::Lane(c.track))
}

/// Whether `clip`'s pattern has `pitch` at every one of `starts`.
fn has_notes(clip: &Clip, pitch: u8, starts: &[Ticks]) -> bool {
    let ClipContent::Midi { notes, .. } = &clip.content else { return false };
    starts.iter().all(|&at| notes.iter().any(|n| n.pitch == pitch && n.start == at))
}

fn drum_pattern_has(s: &Snapshot, pitch: u8, starts: &[Ticks]) -> bool {
    clips_on(s, Instrument::Drums).any(|c| has_notes(c, pitch, starts))
}

fn carve_pattern_has(s: &Snapshot, pitch: u8, starts: &[Ticks]) -> bool {
    clips_on(s, Instrument::Carve).any(|c| has_notes(c, pitch, starts))
}

fn chord_at(s: &Snapshot, pitches: &[u8], starts: &[Ticks]) -> bool {
    chord_clips(s).any(|c| pitches.iter().all(|&p| has_notes(c, p, starts)))
}

/// The first chord note still to write (for the glow), if any.
fn first_missing(s: &Snapshot, pitches: &[u8], starts: &[Ticks]) -> Option<u8> {
    let clip = chord_clips(s).next()?;
    pitches.iter().copied().find(|&p| !has_notes(clip, p, starts))
}

/// A looping clip at least `bars` long.
fn loops(clip: &Clip, bars: i64) -> bool {
    matches!(clip.content, ClipContent::Midi { loop_len: Some(_), .. }) && clip.length >= bars * BAR
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::arrangement::{Command, MidiNote, Track, TrackKind, DEFAULT_TRACK_HEIGHT};
    use shared::lessons::starting_project;

    /// The app state a lesson starts in.
    fn start(id: &str) -> Snapshot {
        let p = starting_project(id);
        Snapshot {
            arrangement: p.arrangement,
            selected_track: None,
            playing: false,
            synth: shared::synth::seed_synth(),
            open_clip: None,
        }
    }

    fn lesson(id: &str) -> &'static Lesson {
        LESSONS.iter().find(|l| l.id == id).unwrap()
    }

    /// Runs `lesson` step by step: before `do_step[i]`, step i must not
    /// pass yet; after it, it must. The last step is the closing info.
    fn walk(id: &str, do_step: &[&dyn Fn(&mut Snapshot)]) {
        let l = lesson(id);
        let mut s = start(id);
        assert_eq!(do_step.len(), l.steps.len() - 1, "{id}: one action per step");
        for (i, (step, act)) in l.steps.iter().zip(do_step).enumerate() {
            let Kind::Action { check, target } = step.kind else { panic!("{id} step {i} should be an action") };
            assert!(!check(&s), "{id} step {} already passes before it's done: {}", i + 1, step.text);
            let _ = target(&s);
            act(&mut s);
            assert!(check(&s), "{id} step {} doesn't pass after doing it: {}", i + 1, step.text);
        }
        assert!(matches!(l.steps.last().unwrap().kind, Kind::Info));
    }

    fn add_midi_track(s: &mut Snapshot, name: &str) {
        add_track(s, name, Instrument::default_for(TrackKind::Midi));
    }

    fn add_track(s: &mut Snapshot, name: &str, instrument: Option<Instrument>) {
        let id = s.arrangement.alloc_id();
        let track = Track {
            id,
            name: name.into(),
            color: shared::arrangement::ClipColor::Violet,
            kind: TrackKind::Midi,
            mute: false,
            solo: false,
            arm: false,
            gain_db: 0.0,
            height: DEFAULT_TRACK_HEIGHT,
            instrument,
            effects: vec![],
            effect_slots: vec![],
            fx: Default::default(),
        };
        let index = s.arrangement.tracks.len();
        Command::InsertTrack { track: Box::new(track), index, clips: vec![], automation: vec![] }.apply(&mut s.arrangement);
        s.selected_track = Some(id);
    }

    /// What a double-click on an empty lane does: a one-bar empty clip on
    /// the selected track.
    fn draw_clip(s: &mut Snapshot) {
        let id = s.arrangement.alloc_id();
        let clip = Clip {
            id,
            track: s.selected_track.unwrap(),
            start: 0,
            length: BAR,
            name: "Clip".into(),
            content: ClipContent::Midi { notes: vec![], loop_len: None, link: None },
            recording: false,
            gain_db: 0.0,
        };
        Command::InsertClip { clip: Box::new(clip) }.apply(&mut s.arrangement);
    }

    fn add_notes(s: &mut Snapshot, pitch: u8, starts: &[Ticks]) {
        let clip = last_clip(s);
        for &start in starts {
            let note = MidiNote { start, length: PPQ / 4, pitch, velocity: 100 };
            Command::AddMidiNote { clip, note }.apply(&mut s.arrangement);
        }
    }

    /// Dragging the open clip's right edge out to bar 9.
    fn stretch(s: &mut Snapshot) {
        let clip = s.arrangement.clip(last_clip(s)).unwrap().extended_as_loop(8 * BAR).unwrap();
        Command::ReplaceClip { clip: Box::new(clip) }.apply(&mut s.arrangement);
    }

    /// The clip the learner just drew (the newest one).
    fn last_clip(s: &Snapshot) -> shared::arrangement::ClipId {
        s.arrangement.clips.last().unwrap().id
    }

    const OFFBEATS: [Ticks; 4] = [PPQ / 2, PPQ + PPQ / 2, 2 * PPQ + PPQ / 2, 3 * PPQ + PPQ / 2];

    #[test]
    fn lesson_one_can_be_done_step_by_step() {
        walk(
            FIRST_BEAT,
            &[
                &|s| add_track(s, "Drums", Some(Instrument::Drums)),
                &draw_clip,
                &|s| add_notes(s, KICK, &[0, PPQ, 2 * PPQ, 3 * PPQ]),
                &|s| s.playing = true,
                &|s| add_notes(s, CLAP, &[PPQ, 3 * PPQ]),
                &|s| add_notes(s, OPEN_HAT, &OFFBEATS),
                &stretch,
            ],
        );
    }

    #[test]
    fn lesson_two_can_be_done_step_by_step() {
        walk(
            BASSLINE,
            &[
                &|s| add_midi_track(s, "MIDI 1"),
                &|s| s.synth = shared::synth::deep_rave_bass(),
                &|s| s.synth.held_notes.push(48),
                &draw_clip,
                &|s| add_notes(s, BASS_NOTE, &OFFBEATS),
                &|s| s.playing = true,
                &stretch,
            ],
        );
    }

    #[test]
    fn lesson_three_can_be_done_step_by_step() {
        walk(
            CHORDS,
            &[
                &|s| add_midi_track(s, "MIDI 1"),
                &|s| s.synth = shared::synth::PRESETS.iter().find(|p| p.0 == "Soft Pad").unwrap().1(),
                &draw_clip,
                &|s| {
                    let mut clip = s.arrangement.clip(last_clip(s)).unwrap().clone();
                    clip.length = 2 * BAR;
                    clip.content = ClipContent::Midi { notes: vec![], loop_len: Some(2 * BAR), link: None };
                    Command::ReplaceClip { clip: Box::new(clip) }.apply(&mut s.arrangement);
                },
                &|s| {
                    for p in [57, 60, 64] {
                        add_notes(s, p, &[0, 2 * PPQ]);
                    }
                },
                &|s| {
                    for p in [60, 64, 67] {
                        add_notes(s, p, &[4 * PPQ, 6 * PPQ]);
                    }
                },
                &|s| s.playing = true,
            ],
        );
    }

    #[test]
    fn preset_names_the_checks_use_exist() {
        for name in ["Deep Rave Bass", "Soft Pad"] {
            assert!(shared::synth::PRESETS.iter().any(|p| p.0 == name && p.1().name == name), "{name}");
        }
    }

    #[test]
    fn every_lesson_ends_with_info_and_has_a_starting_project() {
        for l in LESSONS {
            assert!(matches!(l.steps.last().unwrap().kind, Kind::Info), "{}", l.id);
            let _ = starting_project(l.id);
        }
    }
}
