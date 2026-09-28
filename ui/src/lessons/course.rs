//! The course: every lesson's steps, their text, what each one checks and
//! which control glows. Text lives only here, so translating the course
//! is a change to this table.

use shared::arrangement::{Clip, ClipContent, EffectParam, Instrument, Ticks, EQ_LOW_CUT, PPQ};
use shared::drums::{CLAP, CLOSED_HAT, KICK, OPEN_HAT, SNARE};
use shared::lessons::{
    BAR, BASSLINE, BASS_NOTE, CARVE_ENVELOPES, CARVE_FILTER, CARVE_MIX, CARVE_MOVEMENT, CARVE_SYNC_FM, CARVE_WAVES, CHORDS, FIRST_BEAT,
    RECIPE_BASS, RECIPE_FLUTE, RECIPE_HARP, RECIPE_LEAD, RECIPE_PAD, RECIPE_TANPURA, RECIPE_REED, ARRANGE_HOUSE, ARRANGE_BHAIRAV,
    PROJECT_ARRANGE, PROJECT_BASS, PROJECT_BASS_ROOTS, PROJECT_CHORDS, PROJECT_CHORDS_NOTES, PROJECT_FINISH, PROJECT_GROOVE,
    RECIPE_KEYS, LOFI_BEAT, LOFI_KEYS, LOFI_BASS, LOFI_FINISH, BOLLY_MELODY, BOLLY_DRONE, LOFI_CHORDS, LOFI_BASS_ROOTS,
    SIXTEENTH, MATCH_WAVE, MATCH_CUTOFF, MATCH_RESONANCE, MATCH_SUB, MATCH_PLUCK, MATCH_SWELL, MATCH_MYSTERY,
    THEORY_OCTAVES, THEORY_SCALES, THEORY_KEYS, THEORY_MAJOR_MINOR, THEORY_INTERVALS, THEORY_TRIADS, OCTAVE_TUNE,
    THEORY_PROGRESSIONS, THEORY_MELODY, THEORY_SEVENTHS, THEORY_RAAG, PROGRESSION, MIX_LEVELS, MIX_EQ, MIX_COMPRESS,
    MIX_FINISH, ROLL_DYNAMICS, ROLL_LENGTH, ROLL_HATS_8THS, ROLL_GHOSTS, MELODY_STEPS, MELODY_CALL, MELODY_MOTIF,
};
use super::sound_match::WIN;
use shared::synth::{lfo_rate_hz, FilterType, LfoTarget, SynthParam, SynthState, VoiceMode, Waveform};

use super::{selected, tracks_with, Snapshot, Target};

pub enum Kind {
    /// Done when `check` passes; `target` says what glows meanwhile.
    Action { check: fn(&Snapshot) -> bool, target: fn(&Snapshot) -> Option<Target> },
    /// Read, then Continue.
    Info,
    /// Listen, then pick the answer: `notes` - (16th, pitch, 16ths) - is
    /// what Play plays, `options` the buttons, `answer` the right one.
    Quiz { notes: &'static [(i64, u8, i64)], options: &'static [&'static str], answer: usize },
}

pub struct Step {
    pub text: &'static str,
    /// Why this step sounds the way it does - shown once it's done, while
    /// the change is still in your ears. Empty for most non-recipe steps.
    pub why: &'static str,
    /// Shown if the step hasn't been done after a while.
    pub hint: &'static str,
    pub kind: Kind,
}

pub struct Lesson {
    /// Matches `shared::lessons` (starting project) and the saved "done" list.
    pub id: &'static str,
    /// The sidebar heading it's listed under.
    pub group: &'static str,
    pub title: &'static str,
    pub steps: &'static [Step],
}

pub const BASICS: &str = "Basics";
pub const CARVE: &str = "Carve synth";
pub const RECIPES: &str = "Recipes";
pub const ARRANGEMENT: &str = "Arrangement";
pub const PROJECTS: &str = "Projects";
pub const SOUND_MATCH: &str = "Sound match";
pub const THEORY: &str = "Theory";
pub const MIXING: &str = "Mixing";
pub const ROLL: &str = "Piano roll";
pub const MELODY: &str = "Melody";

const fn act(text: &'static str, hint: &'static str, check: fn(&Snapshot) -> bool, target: fn(&Snapshot) -> Option<Target>) -> Step {
    Step { text, why: "", hint, kind: Kind::Action { check, target } }
}

/// A recipe step: like `act`, plus the reason it sounds that way.
const fn recipe(
    text: &'static str,
    why: &'static str,
    hint: &'static str,
    check: fn(&Snapshot) -> bool,
    target: fn(&Snapshot) -> Option<Target>,
) -> Step {
    Step { text, why, hint, kind: Kind::Action { check, target } }
}

const fn info(text: &'static str) -> Step {
    Step { text, why: "", hint: "", kind: Kind::Info }
}

/// A listening question: Play, then pick one of `options`. `why` is shown
/// once it's answered, like a recipe step's.
const fn quiz(
    text: &'static str,
    why: &'static str,
    notes: &'static [(i64, u8, i64)],
    options: &'static [&'static str],
    answer: usize,
) -> Step {
    Step { text, why, hint: "", kind: Kind::Quiz { notes, options, answer } }
}

pub const LESSONS: &[Lesson] = &[
    Lesson {
        id: FIRST_BEAT,
        group: BASICS,
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
        group: BASICS,
        title: "A bassline",
        steps: &[
            act(
                "Your beat is ready. For a bass, click \u{201c}+ MIDI track\u{201d}.",
                "It's below the track list, on the left of the timeline.",
                |s| tracks_with(s, Instrument::Carve).next().is_some(),
                |_| Some(Target::AddMidiTrack),
            ),
            act(
                "A MIDI track holds notes; its instrument turns them into sound. New ones play Carve, a synth. Pick a bass sound: click the preset name at the top of Carve (it says Warm Bass) and choose Deep Rave Bass.",
                "Or step through the presets with the \u{2039} \u{203a} arrows beside the name.",
                |s| selected(s).is_some_and(|t| t.instrument == Some(Instrument::Carve)) && s.synth.name == "Deep Rave Bass",
                |_| Some(Target::Preset("Deep Rave Bass")),
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
        group: BASICS,
        title: "Chords",
        steps: &[
            act(
                "Drums and bass are ready. Add one more MIDI track for chords.",
                "\u{201c}+ MIDI track\u{201d}, below the track list.",
                |s| tracks_with(s, Instrument::Carve).count() >= 2,
                |_| Some(Target::AddMidiTrack),
            ),
            act(
                "Pick a soft sound for chords: click the preset name at the top of Carve and choose Soft Pad.",
                "Or step through the presets with the \u{2039} \u{203a} arrows beside the name.",
                |s| selected(s).is_some_and(|t| t.instrument == Some(Instrument::Carve) && t.name != "Bass") && s.synth.name == "Soft Pad",
                |_| Some(Target::Preset("Soft Pad")),
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
    Lesson {
        id: ROLL_DYNAMICS,
        group: ROLL,
        title: "Accents and ghost notes",
        steps: &[
            act(
                "Press Space: every hi-hat hits exactly as hard as the last. It sounds like a machine.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Open the Beat clip: double-click it.",
                "Two quick clicks on the clip in the Drums lane.",
                |s| drums_open(s),
                |s| track_named(s, "Drums").map(|t| Target::Lane(t.id)),
            ),
            recipe(
                "Make two ghost notes: the hats just before beats 3 and 1 (at 1.2.4 and 1.4.4). Drag their stems in the Velocity lane, under the grid, down to less than half height.",
                "A ghost note is felt more than heard: it fills the gap before the next beat and makes the pattern roll forward.",
                "The glowing column in the Velocity lane. Drag the top of the stem down, below the halfway mark.",
                |s| ROLL_GHOSTS.iter().all(|&at| hat_velocity(s, at).is_some_and(|v| v < 60)),
                |s| velocity_target(s, &ROLL_GHOSTS, |v| v < 60),
            ),
            recipe(
                "Now soften the hats halfway between the beats (1.1.3, 1.2.3, 1.3.3 and 1.4.3): drag their stems a little way down, to about three quarters height.",
                "Strong on the beat, lighter between, barely there on the ghosts. That rise and fall is what people mean by groove.",
                "Their stems glow in turn. Anywhere between half and nearly full height counts.",
                |s| ROLL_HATS_8THS.iter().filter(|&&at| at % 4 == 2).all(|&at| hat_velocity(s, at).is_some_and(|v| (60..=105).contains(&v))),
                |s| velocity_target(s, &[2, 6, 10, 14], |v| (60..=105).contains(&v)),
            ),
            info(
                "Velocity is how hard a note is played. A drummer never hits twice the same, and neither should a programmed beat. \
                 The same goes for keys and bass: lean on the notes that matter and let the rest sit back.",
            ),
        ],
    },
    Lesson {
        id: ROLL_LENGTH,
        group: ROLL,
        title: "Note length and timing",
        steps: &[
            act(
                "Press Space: a beat, and an empty Keys part to write on.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Open the Notes clip in the Keys lane: double-click it.",
                "Two quick clicks on the empty clip under the drums.",
                |s| track_open(s, "Keys"),
                |s| track_named(s, "Keys").map(|t| Target::Lane(t.id)),
            ),
            recipe(
                "Long notes: click Snap (top right of the editor) until it says 1/4, then click A3 on beat 1 and on beat 3.",
                "With Snap at 1/4, each click writes a note a whole beat long. Long notes ring into each other and connect.",
                "A new note is as long as one step of Snap. A3 is the bottom row.",
                |s| long_note_on(s, 0) && long_note_on(s, 2),
                |s| if !track_open(s, "Keys") { None } else if long_note_on(s, 0) || long_note_on(s, 2) { Some(Target::PianoRollRow(57)) } else { Some(Target::Snap) },
            ),
            recipe(
                "A short stab: set Snap back to 1/16, then click C4 on beat 4.",
                "A 16th-long note is a stab: it punctuates. Short notes next to long ones are what make a part breathe.",
                "Click Snap until it says 1/16. C4 is the row above A3.",
                |s| short_note_on(s, 3),
                |_| Some(Target::Snap),
            ),
            recipe(
                "Off the grid: hold Alt and click E4 just after beat 2.",
                "Alt puts a note exactly where you click. A little late feels laid back; a little early feels pushed. The grid is a guide, not a rule.",
                "Hold Alt (Option on a Mac) while you click, a little right of the 1.2 line.",
                |s| keys_notes(s).iter().any(|n| n.start % SIXTEENTH != 0),
                |_| Some(Target::PianoRollRow(64)),
            ),
            info(
                "Length and timing are the other half of expression: long or short, on the grid or just off it. \
                 Most of a part's feel comes from these and velocity, not from which notes it plays.",
            ),
        ],
    },
    Lesson {
        id: THEORY_OCTAVES,
        group: THEORY,
        title: "Octaves",
        steps: &[
            act(
                "Press Space and listen: C, D, E, C - the start of Frère Jacques.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Open the Tune clip: double-click it.",
                "Two quick clicks on the clip in the Keys lane. Its notes open below.",
                |s| theory_open(s),
                |s| theory_lane(s),
            ),
            recipe(
                "Move the whole tune up an octave. Pick Select (above the grid), click the first note, Shift-click the other three, then press Shift+\u{2191}.",
                "Every note now vibrates twice as fast, yet it's plainly the same tune. That's an octave: the same note, higher.",
                "Shift+\u{2191} moves the notes you've picked up an octave; \u{2191} on its own moves them one note of the scale.",
                |s| theory_pattern_has_all(s, &OCTAVE_TUNE, 12) && !theory_pattern_has_any(s, &OCTAVE_TUNE, 0),
                |s| if theory_open(s) { None } else { theory_lane(s) },
            ),
            recipe(
                "Put the first C back where it was: pick Draw and click C4 on the very first square, under the moved C5.",
                "C4 and C5 together blend almost into one note - an octave's two notes share most of what you hear in them.",
                "C4 is the row with the dark mark, at the bottom of the grid.",
                |s| theory_clip(s).is_some_and(|c| has_notes(c, 60, &[0])),
                |s| theory_row(s, 60),
            ),
            info(
                "An octave is 12 semitones, and both notes share a name: C4 and C5 are both C. \
                 The number says which octave. Note names only go A to G, then repeat - octave after octave.",
            ),
        ],
    },
    Lesson {
        id: THEORY_SCALES,
        group: THEORY,
        title: "Steps and scales",
        steps: &[
            act(
                "Open the Notes clip: double-click it. Every row of the grid is one semitone, the smallest step between two notes.",
                "Two quick clicks on the empty clip in the Keys lane.",
                |s| theory_open(s),
                |s| theory_lane(s),
            ),
            act(
                "Start on C: click C4 on beat 1 (the first square).",
                "C4 is the row with the dark mark.",
                |s| theory_beats(s, &[(60, 0)]),
                |s| theory_row(s, 60),
            ),
            act(
                "A whole step is two semitones: skip a row. Click D4 on beat 2, then E4 on beat 3.",
                "Beats are the 1.2 and 1.3 marks on the ruler above the grid.",
                |s| theory_beats(s, &[(62, 1), (64, 2)]),
                |s| theory_first_missing(s, &[(62, 1), (64, 2)]),
            ),
            recipe(
                "A half step is one semitone: the very next row. Click F4 on beat 4.",
                "E to F has no row between them - on a piano, no black key.",
                "F4 is the row right above E4.",
                |s| theory_beats(s, &[(65, 3)]),
                |s| theory_row(s, 65),
            ),
            act(
                "Whole, whole, whole: G4, A4 and B4 on beats 1, 2 and 3 of bar 2.",
                "Bar 2 starts at the 2 mark. Skip a row between each.",
                |s| theory_beats(s, &[(67, 4), (69, 5), (71, 6)]),
                |s| theory_first_missing(s, &[(67, 4), (69, 5), (71, 6)]),
            ),
            act(
                "Half: C5 on beat 4 of bar 2 - the row right above B4.",
                "C5 is the top row, with the dark mark.",
                |s| theory_beats(s, &[(72, 7)]),
                |s| theory_row(s, 72),
            ),
            recipe(
                "Press Space: you've built a scale.",
                "Whole, whole, half, whole, whole, whole, half: that pattern is every major scale, whatever note it starts on.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            info(
                "That's C major - the piano's white keys. Start the same whole-and-half pattern on G and you get G major \
                 (it needs F#, a black key, to keep the pattern). A scale is a pattern of steps; the note it starts on names it.",
            ),
        ],
    },
    Lesson {
        id: THEORY_KEYS,
        group: THEORY,
        title: "Keys: where home is",
        steps: &[
            act(
                "Press Space: a tune in C major. Listen to its last note, C - it sounds like arriving home.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Open the Tune clip: double-click it.",
                "Two quick clicks on the clip in the Keys lane.",
                |s| theory_open(s),
                |s| theory_lane(s),
            ),
            recipe(
                "Change where it lands: click the last note (C4, beat 4 of bar 2) to remove it, then click A3 in the same spot.",
                "Now it stops on A and doesn't feel finished - or it feels sadder. Every other note is the same; only home moved.",
                "A3 is two rows below C4. With Draw on, clicking a note removes it.",
                |s| theory_beats(s, &[(57, 7)]) && !theory_beats(s, &[(60, 7)]),
                |s| if theory_beats(s, &[(60, 7)]) { theory_row(s, 60) } else { theory_row(s, 57) },
            ),
            recipe(
                "A key names the home note. Open the Key menu at the top of the window and pick A, then Natural minor.",
                "A minor uses exactly C major's notes - it's C major's relative minor. The rows didn't change; the shaded home row did.",
                "Key sits left of the tempo. Pick the note first, then the scale.",
                |s| s.key == 9 && s.scale_mask == scale_mask("Natural minor"),
                |_| Some(Target::KeyMenu),
            ),
            info(
                "A key is a home note plus a scale: C major, A minor. Songs in the same key share their notes - \
                 that's what the browser's Fits key button checks. Where a tune comes to rest tells your ear which key it's in.",
            ),
        ],
    },
    Lesson {
        id: THEORY_MAJOR_MINOR,
        group: THEORY,
        title: "Major and minor",
        steps: &[
            act(
                "Press Space: C, E and G, up and down. Bright, like a fanfare.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            recipe(
                "Switch the scale: open the Key menu at the top and pick Natural minor (keep C).",
                "The rows now follow C minor: E\u{266d} replaced E. Your E's are still there, now on shaded rows - off the scale.",
                "Key sits left of the tempo.",
                |s| s.key == 0 && s.scale_mask == scale_mask("Natural minor"),
                |_| Some(Target::KeyMenu),
            ),
            act(
                "Open the Tune clip: double-click it.",
                "Two quick clicks on the clip in the Keys lane.",
                |s| theory_open(s),
                |s| theory_lane(s),
            ),
            recipe(
                "Lower every E to E\u{266d}: pick Select, click an E, Shift-click the other two, then press \u{2193}.",
                "One semitone, and bright turned dark. The third note of the scale - E or E\u{266d} over C - decides major or minor.",
                "\u{2193} moves the notes you've picked to the next note of the scale below: from E, that's E\u{266d}.",
                |s| theory_beats_16(s, &[(63, 2), (63, 6), (63, 10)]) && !theory_pattern_has_any(s, &[(2, 64, 2), (6, 64, 2), (10, 64, 2)], 0),
                |s| if theory_open(s) { None } else { theory_lane(s) },
            ),
            info(
                "A major third is 4 semitones above the root (C to E); a minor third is 3 (C to E\u{266d}). \
                 That one semitone is the whole difference between a major chord and a minor one.",
            ),
        ],
    },
    Lesson {
        id: THEORY_INTERVALS,
        group: THEORY,
        title: "Intervals by ear",
        steps: &[
            info(
                "An interval is the distance between two notes. Four to know by ear: the octave (the same note, higher), \
                 the fifth (open and strong - a power chord), and the thirds: major (bright) and minor (dark).",
            ),
            act(
                "Press Space to hear all four, in that order: octave, fifth, major third, minor third.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            quiz(
                "Play it, then name the interval.",
                "An octave: the second note is the first, higher.",
                &[(0, 60, 4), (4, 72, 6)],
                &["Octave", "Fifth", "Third"],
                0,
            ),
            quiz(
                "And this one?",
                "A fifth: open and hollow, the power chord's two notes.",
                &[(0, 60, 4), (4, 67, 6)],
                &["Octave", "Fifth", "Third"],
                1,
            ),
            quiz(
                "Major or minor third?",
                "A major third: C to E, bright.",
                &[(0, 60, 4), (4, 64, 6)],
                &["Major third", "Minor third"],
                0,
            ),
            quiz(
                "And this third?",
                "A minor third: A to C, darker.",
                &[(0, 57, 4), (4, 60, 6)],
                &["Major third", "Minor third"],
                1,
            ),
            quiz(
                "Now from another note.",
                "A fifth again, from D this time: the distance is the sound, not the notes.",
                &[(0, 62, 4), (4, 69, 6)],
                &["Octave", "Fifth", "Minor third"],
                1,
            ),
            quiz(
                "Both at once this time.",
                "A fifth, played together: that's how guitarists' power chords sound.",
                &[(0, 60, 8), (0, 67, 8)],
                &["Octave", "Fifth", "Major third"],
                1,
            ),
            quiz(
                "Last one.",
                "A minor third: E to G, the top of a C major chord.",
                &[(0, 64, 4), (4, 67, 6)],
                &["Major third", "Minor third", "Fifth"],
                1,
            ),
            info(
                "Fifths make a chord solid, thirds give it its mood, octaves just double it. \
                 Hum along when you listen to songs: you'll start hearing these everywhere.",
            ),
        ],
    },
    Lesson {
        id: THEORY_TRIADS,
        group: THEORY,
        title: "Triads",
        steps: &[
            act(
                "Open the Notes clip: double-click it.",
                "Two quick clicks on the empty clip in the Keys lane.",
                |s| theory_open(s),
                |s| theory_lane(s),
            ),
            recipe(
                "Pick Triad (right of Select / Draw, above the grid), then click C4 on beat 1.",
                "C, E and G: every other note of the scale, stacked. That's a triad - C major.",
                "Triad writes three notes per click. C4 is the row with the dark mark.",
                |s| theory_chord(s, &[60, 64, 67], 0),
                |s| theory_row(s, 60),
            ),
            recipe(
                "Now click D4 on beat 2.",
                "The same shape from D - but D to F is only 3 semitones, a minor third. That makes it D minor.",
                "Beat 2 is the 1.2 mark.",
                |s| theory_chord(s, &[62, 65, 69], 1),
                |s| theory_row(s, 62),
            ),
            recipe(
                "E4 on beat 3, then F4 on beat 4.",
                "E minor, then F major: the shape stays, the thirds inside it change.",
                "Beats 3 and 4 are the 1.3 and 1.4 marks.",
                |s| theory_chord(s, &[64, 67, 71], 2) && theory_chord(s, &[65, 69, 72], 3),
                |s| if theory_chord(s, &[64, 67, 71], 2) { theory_row(s, 65) } else { theory_row(s, 64) },
            ),
            recipe(
                "Bar 2: G4 on beat 1, A4 on beat 2, B4 on beat 3.",
                "G major, A minor - and B diminished, the odd one: two minor thirds, tense.",
                "Bar 2 starts at the 2 mark.",
                |s| theory_chord(s, &[67, 71, 74], 4) && theory_chord(s, &[69, 72, 76], 5) && theory_chord(s, &[71, 74, 77], 6),
                |s| {
                    [(67, 4), (69, 5), (71, 6)]
                        .into_iter()
                        .find(|&(p, beat)| !theory_beats(s, &[(p, beat)]))
                        .and_then(|(p, _)| theory_row(s, p))
                },
            ),
            act(
                "Come home: C4 again, on beat 4 of bar 2.",
                "The last beat of bar 2.",
                |s| theory_chord(s, &[60, 64, 67], 7),
                |s| theory_row(s, 60),
            ),
            act("Press Space and listen to all eight.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            info(
                "C major's seven triads: C, Dm, Em, F, G, Am and B\u{b0}. Three major, three minor, one diminished. \
                 Most songs in C use only these - and the same shapes work in every key.",
            ),
        ],
    },
    Lesson {
        id: THEORY_PROGRESSIONS,
        group: THEORY,
        title: "Chord progressions",
        steps: &[
            info(
                "Chords get numbers from where they sit in the key: I is built on the 1st note (C), IV on the 4th (F), \
                 V on the 5th (G), vi on the 6th (A). Lower case means minor. Write the most used order in pop: I, V, vi, IV.",
            ),
            act(
                "Open the Chords clip: double-click it.",
                "Two quick clicks on the empty clip in the Keys lane.",
                |s| theory_open(s),
                |s| theory_lane(s),
            ),
            recipe(
                "I: pick Triad (right of Select / Draw) and click C4 on beat 1 of bar 1.",
                "Home: the I chord, C major.",
                "C4 is the row with the dark mark.",
                |s| track_chord(s, "Keys", &PROGRESSION[0], 0),
                |s| theory_row(s, 60),
            ),
            recipe(
                "V: G3 on beat 1 of bar 2.",
                "G major, a fifth above home, pulls back towards it. G3 sits below C4, so the chords stay close.",
                "Bar 2 starts at the 2 mark. G3 is three rows below C4.",
                |s| track_chord(s, "Keys", &PROGRESSION[1], 4),
                |s| theory_row(s, 55),
            ),
            recipe(
                "vi: A3 on beat 1 of bar 3.",
                "A minor: it shares two notes with C major, so it feels like home turned darker.",
                "A3 is the row just above G3.",
                |s| track_chord(s, "Keys", &PROGRESSION[2], 8),
                |s| theory_row(s, 57),
            ),
            recipe(
                "IV: F3 on beat 1 of bar 4.",
                "F major lifts, and leads round to C again when the clip loops.",
                "F3 is just below G3.",
                |s| track_chord(s, "Keys", &PROGRESSION[3], 12),
                |s| theory_row(s, 53),
            ),
            act("Press Space and let it loop.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            info(
                "I - V - vi - IV runs under hundreds of songs, in every key. Other loops to try: vi - IV - I - V (the same \
                 chords, starting sad), or I - vi - IV - V (the fifties). The numbers stay the same when the key changes.",
            ),
        ],
    },
    Lesson {
        id: THEORY_MELODY,
        group: THEORY,
        title: "Melody over chords",
        steps: &[
            act(
                "Press Space: the chords from last lesson, C, G, Am, F, one a bar. Keep it looping while you write.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Open the Melody clip: double-click it (in the Melody lane).",
                "Two quick clicks on the empty clip under the chords.",
                |s| track_open(s, "Melody"),
                |s| track_named(s, "Melody").map(|t| Target::Lane(t.id)),
            ),
            recipe(
                "On beat 1 of bars 1 and 2, click a note of that bar's chord: bar 1 is C (C, E or G), bar 2 is G (G, B or D).",
                "A chord's own note on the beat it lands sounds settled - the melody and the chord agree.",
                "Any octave counts. Bar 2 starts at the 2 mark.",
                |s| melody_on_chord(s, 0) && melody_on_chord(s, 1),
                |s| melody_target(s),
            ),
            act(
                "Bars 3 and 4 the same way: bar 3 is A minor (A, C or E), bar 4 is F (F, A or C).",
                "Any octave counts. Bars 3 and 4 start at the 3 and 4 marks.",
                |s| melody_on_chord(s, 2) && melody_on_chord(s, 3),
                |s| melody_target(s),
            ),
            recipe(
                "Now move between them: add a note on beat 3 of every bar - any note of the scale.",
                "Notes off the chord, between the beats that matter, are what make a melody move instead of just outlining chords.",
                "Beat 3 is the .3 mark of each bar. Try stepping towards the next bar's first note.",
                |s| (0..4).all(|bar| melody_has_beat(s, bar, 2)),
                |s| melody_target(s),
            ),
            info(
                "Chord notes on the strong beats (1 and 3), anything from the scale in between: that's most melodies. \
                 The Interval labels help - 1, 3 and 5 are the home chord's notes.",
            ),
        ],
    },
    Lesson {
        id: THEORY_SEVENTHS,
        group: THEORY,
        title: "7th chords",
        steps: &[
            act(
                "Open the Notes clip: double-click it.",
                "Two quick clicks on the empty clip in the Keys lane.",
                |s| theory_open(s),
                |s| theory_lane(s),
            ),
            recipe(
                "Pick 7th (right of Triad) and click D3 on beat 1.",
                "D, F, A and C: a triad with one more third on top. D minor 7 - softer and more open than D minor.",
                "D3 is the row just above the bottom C3.",
                |s| track_chord(s, "Keys", &[50, 53, 57, 60], 0),
                |s| theory_row(s, 50),
            ),
            recipe(
                "G3 on beat 3.",
                "G7: the added F leans hard towards home. It's the chord that wants to resolve most.",
                "Beat 3 is the 1.3 mark.",
                |s| track_chord(s, "Keys", &[55, 59, 62, 65], 2),
                |s| theory_row(s, 55),
            ),
            recipe(
                "And home: C3 on beat 1 of bar 2.",
                "C major 7: home, but dreamy rather than final - the sound of lo-fi and jazz.",
                "C3 is the bottom row. Bar 2 starts at the 2 mark.",
                |s| track_chord(s, "Keys", &[48, 52, 55, 59], 4),
                |s| theory_row(s, 48),
            ),
            act("Press Space and let it loop.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            info(
                "ii - V - I: the move jazz is built on. A 7th chord is a triad plus the next third up; it softens the \
                 triad and makes it want to move. The lo-fi lessons' keys use them all the way through.",
            ),
        ],
    },
    Lesson {
        id: THEORY_RAAG,
        group: THEORY,
        title: "Raag basics",
        steps: &[
            act(
                "Press Space: the tanpura plays Sa, the home note (here C), with Pa, a fifth above. Everything is heard against it.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            info(
                "Indian music names notes by their place above Sa: Sa Re Ga ma Pa Dha Ni - like 1 to 7. Click the grid's \
                 top-left heading (Interval \u{203a}) until it says Sargam, and the rows are named that way.",
            ),
            act(
                "Open the Aroha clip: double-click it (in the Keys lane).",
                "Two quick clicks on the empty clip under the drone.",
                |s| theory_open(s),
                |s| theory_lane(s),
            ),
            recipe(
                "Aroha means going up. One note a beat: Sa Re Ga ma in bar 1 (C4 D4 E4 F4), Pa Dha Ni Sa in bar 2 (G4 A4 B4 C5).",
                "Every note is heard against the drone's Sa - that's how a raag's notes get their colour.",
                "Start on the Sa row with the dark mark; one row up each beat.",
                |s| theory_beats(s, &[(60, 0), (62, 1), (64, 2), (65, 3), (67, 4), (69, 5), (71, 6), (72, 7)]),
                |s| theory_first_missing(s, &[(60, 0), (62, 1), (64, 2), (65, 3), (67, 4), (69, 5), (71, 6), (72, 7)]),
            ),
            recipe(
                "Now Raag Bhairav: open the Key menu at the top and pick Raga Bhairav (keep C).",
                "Bhairav's Re and Dha are komal - lowered. Yours are now on shaded rows: outside the raag.",
                "Key sits left of the tempo. The ragas are in the right-hand column.",
                |s| s.key == 0 && s.scale_mask == scale_mask("Raga Bhairav"),
                |_| Some(Target::KeyMenu),
            ),
            recipe(
                "Make them komal: pick Select, click D4, Shift-click A4, then press \u{2193}.",
                "Sa re Ga ma Pa dha Ni Sa: two lowered notes turn a bright scale into a solemn dawn raag.",
                "\u{2193} moves the notes you've picked to the next note of the raag below.",
                |s| theory_beats(s, &[(61, 1), (68, 5)]) && !theory_beats(s, &[(62, 1)]) && !theory_beats(s, &[(69, 5)]),
                |s| if theory_open(s) { None } else { theory_lane(s) },
            ),
            info(
                "A raag is more than its notes: it has phrases it returns to (its pakad), notes it dwells on, a way up \
                 (aroha) and down (avaroha), and a time of day - Bhairav belongs to dawn. Hear it at work in the Bhairav \
                 rave demo and the Reed recipe.",
            ),
        ],
    },
    Lesson {
        id: MELODY_STEPS,
        group: MELODY,
        title: "Steps and leaps",
        steps: &[
            act(
                "Press Space: C, G, A minor and F, one chord a bar. Keep it looping while you write.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Open the Melody clip: double-click it (in the Melody lane).",
                "Two quick clicks on the empty clip under the chords.",
                |s| track_open(s, "Melody"),
                |s| track_named(s, "Melody").map(|t| Target::Lane(t.id)),
            ),
            recipe(
                "Bar 1, only steps: four notes, one on each beat, each the next note of the scale up or down. Try E, D, C, D.",
                "Smooth and easy to sing - but it doesn't stick. A melody made only of steps drifts by.",
                "A step is the next row up or down. E4, D4, C4, D4 on beats 1 to 4.",
                |s| {
                    let bar = melody_bar(s, 0);
                    bar.len() >= 4 && bar.windows(2).all(|w| (1..=2).contains(&w[0].pitch.abs_diff(w[1].pitch)))
                },
                |s| melody_row(s, 64),
            ),
            recipe(
                "Bar 2: open with a leap - up a 4th or more, to a note of the G chord (G, B or D) - then step back down.",
                "The leap is the moment you remember. Stepping back the other way fills the gap it opened, so the line still feels natural.",
                "From D4, G4 is a leap of a 4th. Then F4 on the next beat is a step down.",
                |s| {
                    let (a, b) = (melody_bar(s, 0), melody_bar(s, 1));
                    let (Some(last), [first, next, ..]) = (a.last(), b.as_slice()) else { return false };
                    first.pitch >= last.pitch + 5
                        && [7, 11, 2].contains(&(first.pitch % 12))
                        && next.pitch < first.pitch
                        && first.pitch - next.pitch <= 2
                },
                |s| melody_row(s, 67),
            ),
            info(
                "Most good melodies are mostly steps - roughly half to two thirds - with a few leaps saved for the moments \
                 that matter. After a leap, step back the other way. Hum any chorus you like and listen for it.",
            ),
        ],
    },
    Lesson {
        id: MELODY_CALL,
        group: MELODY,
        title: "Call and response",
        steps: &[
            act(
                "Press Space: the chords again. A melody can talk to itself - a question, then an answer.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            quiz(
                "Play it: does this phrase sound finished?",
                "Unfinished. It stops on G, the 5th: it hangs there, waiting for something to answer it.",
                &[(0, 60, 3), (4, 62, 3), (8, 64, 3), (12, 67, 8)],
                &["Unfinished, like a question", "Finished"],
                0,
            ),
            quiz(
                "And this one?",
                "Finished. It comes to rest on C, home: nothing more needs to happen.",
                &[(0, 64, 3), (4, 62, 3), (8, 62, 3), (12, 60, 8)],
                &["Unfinished, like a question", "Finished"],
                1,
            ),
            act(
                "Open the Melody clip: double-click it (in the Melody lane).",
                "Two quick clicks on the empty clip under the chords.",
                |s| track_open(s, "Melody"),
                |s| track_named(s, "Melody").map(|t| Target::Lane(t.id)),
            ),
            recipe(
                "The call: in bar 1, a short phrase on beats 1 to 3 that ends on D or G. Leave beat 4 empty.",
                "Ending on the 2nd or 5th leaves it hanging - a question. The empty beat is the pause where the answer can come in.",
                "Try C4, E4, G4 on beats 1, 2 and 3. Nothing on 1.4.",
                |s| {
                    let bar = melody_bar(s, 0);
                    bar.len() >= 2
                        && bar.last().is_some_and(|n| [2, 7].contains(&(n.pitch % 12)))
                        && !bar.iter().any(|n| n.start >= 3 * PPQ)
                },
                |s| melody_row(s, 67),
            ),
            recipe(
                "The answer: in bar 2, the same rhythm as the call, ending on C or E.",
                "Same rhythm tells the ear it's a reply; landing on home closes it. Question, pause, answer.",
                "Put notes on the same beats as bar 1 (1, 2 and 3), and make the last one C or E.",
                |s| {
                    let (a, b) = (melody_bar(s, 0), melody_bar(s, 1));
                    !b.is_empty()
                        && rhythm(&a, 0) == rhythm(&b, 1)
                        && b.last().is_some_and(|n| [0, 4].contains(&(n.pitch % 12)))
                },
                |s| melody_row(s, 60),
            ),
            info(
                "Leave space, keep the call and the answer about the same length, and end the call away from home and \
                 the answer on it. Guitarists build whole solos this way - so do vocal hooks. Try answering an octave \
                 higher, or on a different sound.",
            ),
        ],
    },
    Lesson {
        id: MELODY_MOTIF,
        group: MELODY,
        title: "Motifs: repeat and vary",
        steps: &[
            act(
                "Press Space: the chords again. This time you'll build a whole melody out of one small idea.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Open the Melody clip: double-click it (in the Melody lane).",
                "Two quick clicks on the empty clip under the chords.",
                |s| track_open(s, "Melody"),
                |s| track_named(s, "Melody").map(|t| Target::Lane(t.id)),
            ),
            recipe(
                "The motif: in bar 1, three or four notes - a small idea. Try C, D, E on beats 1, 2 and 3.",
                "A motif is a musical word: short enough to remember, with a shape you'll recognise when it comes back.",
                "Three or four notes, all in bar 1.",
                |s| (3..=4).contains(&melody_bar(s, 0).len()),
                |s| melody_row(s, 60),
            ),
            recipe(
                "Bar 2: the same rhythm and shape, starting one scale step higher (D, E, F for C, D, E).",
                "A sequence: the same idea moved up the scale. The ear recognises it and hears the line going somewhere.",
                "Same beats as bar 1, every note one row up.",
                |s| sequenced(s, 0, 1),
                |s| melody_row(s, 62),
            ),
            recipe(
                "Bar 3: once more, a step higher again.",
                "Three times up builds tension - the listener expects it to keep going.",
                "Same beats again, one more row up (E, F, G).",
                |s| sequenced(s, 1, 2),
                |s| melody_row(s, 64),
            ),
            recipe(
                "Bar 4: start it like bar 1, but change the ending so it lands on C.",
                "Three times the same, then a twist that comes home. Most hooks are built exactly like this.",
                "Begin with bar 1's first two notes, then end on a C - try C, D, C.",
                |s| {
                    let (a, d) = (melody_bar(s, 0), melody_bar(s, 3));
                    a.len() >= 2
                        && d.len() >= 2
                        && d[0].pitch == a[0].pitch
                        && d[1].pitch == a[1].pitch
                        && d.last().is_some_and(|n| n.pitch % 12 == 0)
                        && d.iter().map(|n| n.pitch).collect::<Vec<_>>() != a.iter().map(|n| n.pitch).collect::<Vec<_>>()
                },
                |s| melody_row(s, 60),
            ),
            info(
                "One idea, repeated, moved and changed: that's a melody that hangs together. When you're stuck, don't \
                 look for new notes - take the idea you have and move it, flip it or change its ending.",
            ),
        ],
    },
    Lesson {
        id: CARVE_WAVES,
        group: CARVE,
        title: "Waves",
        steps: &[
            act(
                "Press Space. This is Oscillator 1 playing a saw wave: bright and buzzy, because it holds every harmonic.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Click the square wave, the last shape above Oscillator 1. Hollow and woody: only the odd harmonics.",
                "The four little wave pictures next to \u{201c}Oscillator 1\u{201d}.",
                |s| carve(s).is_some_and(|p| p.osc1.waveform == Waveform::Square),
                |_| Some(Target::OscWave(1)),
            ),
            act(
                "Now the triangle, the second shape: softer - its harmonics are faint.",
                "The shape that looks like a mountain range.",
                |s| carve(s).is_some_and(|p| p.osc1.waveform == Waveform::Triangle),
                |_| Some(Target::OscWave(1)),
            ),
            act(
                "Now the sine, the first shape: pure, with no harmonics at all. Flutes and sub-basses live here.",
                "The smooth round wave.",
                |s| carve(s).is_some_and(|p| p.osc1.waveform == Waveform::Sine),
                |_| Some(Target::OscWave(1)),
            ),
            act(
                "Back to the saw. The more harmonics a wave has, the more a filter can shape - which is why most synth sounds start from a saw or square.",
                "The saw is the third shape: a ramp that drops.",
                |s| carve(s).is_some_and(|p| p.osc1.waveform == Waveform::Saw),
                |_| Some(Target::OscWave(1)),
            ),
            info("Sine is pure, triangle soft, square hollow, saw bright. Every sound in this course starts by picking one."),
        ],
    },
    Lesson {
        id: CARVE_MIX,
        group: CARVE,
        title: "Mixing oscillators",
        steps: &[
            act("Press Space: one saw, held.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            act(
                "Oscillator 2 is silent. In the Mixer, turn Osc 2 up past -12 dB.",
                "Drag the Osc 2 knob in the Mixer upward.",
                |s| carve(s).is_some_and(|p| p.mix.osc2_db > -12.0),
                |_| Some(Target::Knob(SynthParam::Osc2Level)),
            ),
            act(
                "Detune it: set Oscillator 2's Detune to about +10 cents. The two waves drift in and out of step - that beating makes it thick.",
                "Anywhere from +5 to +25 cents works.",
                |s| carve(s).is_some_and(|p| (5.0..=25.0).contains(&p.osc2.knob_a_cents)),
                |_| Some(Target::Knob(SynthParam::Osc2Detune)),
            ),
            act(
                "Set Oscillator 2's Octave to +1: it now plays an octave up, adding brightness on top.",
                "One step up on the Octave knob under Oscillator 2.",
                |s| carve(s).is_some_and(|p| p.osc2.octave == 1),
                |_| Some(Target::Knob(SynthParam::Osc2Octave)),
            ),
            act(
                "Add weight: turn Sub up past -12 dB - a sine one octave below the note.",
                "The Sub knob in the Mixer.",
                |s| carve(s).is_some_and(|p| p.mix.sub_db > -12.0),
                |_| Some(Target::Knob(SynthParam::SubLevel)),
            ),
            act(
                "A little air: Noise to about -30 dB. Not much - noise gets harsh fast.",
                "Between -40 and -15 dB.",
                |s| carve(s).is_some_and(|p| (-40.0..=-15.0).contains(&p.mix.noise_db)),
                |_| Some(Target::Knob(SynthParam::NoiseLevel)),
            ),
            info(
                "The Mixer blends four sources: two oscillators, a sub and noise. Most sounds use two or three, \
                 and their levels matter as much as their waves.",
            ),
        ],
    },
    Lesson {
        id: CARVE_FILTER,
        group: CARVE,
        title: "The filter",
        steps: &[
            act("Press Space: a bright saw riff.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            act(
                "Turn Cutoff down below 400 Hz. The low-pass filter removes the highs: darker, muffled.",
                "Cutoff is the big knob in the Filter section.",
                |s| carve(s).is_some_and(|p| p.filter.cutoff_hz < 400.0),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            act(
                "Turn Resonance up past 70%. It boosts right at the cutoff - that whistling edge.",
                "Resonance is next to Cutoff.",
                |s| carve(s).is_some_and(|p| p.filter.resonance > 0.7),
                |_| Some(Target::Knob(SynthParam::Resonance)),
            ),
            act(
                "Now sweep Cutoff slowly back up past 3 kHz while it plays: the classic filter sweep of acid house.",
                "Drag it up gradually and listen.",
                |s| carve(s).is_some_and(|p| p.filter.cutoff_hz > 3000.0),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            act(
                "Try another filter type: click HP (high-pass). It keeps only the highs - thin and airy.",
                "The LP 24 / LP 12 / BP / HP switch at the top of the Filter.",
                |s| carve(s).is_some_and(|p| p.filter.filter_type == FilterType::Hp),
                |_| Some(Target::FilterType),
            ),
            act(
                "Back to LP 24, the warm low-pass most sounds use.",
                "The first option on the same switch.",
                |s| carve(s).is_some_and(|p| p.filter.filter_type == FilterType::Lp24),
                |_| Some(Target::FilterType),
            ),
            info(
                "Low-pass cuts highs (warm), high-pass cuts lows (thin), band-pass keeps a middle band. \
                 Cutoff sets where the filter cuts; resonance sets how sharp the edge is.",
            ),
        ],
    },
    Lesson {
        id: CARVE_ENVELOPES,
        group: CARVE,
        title: "Envelopes",
        steps: &[
            act("Press Space.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            act(
                "The Amp envelope shapes each note's loudness. Drag Amp Sustain to 0: notes now fade while they're held.",
                "Sustain is the third knob under Amp envelope.",
                |s| carve(s).is_some_and(|p| p.amp_env.sustain < 0.05),
                |_| Some(Target::Knob(SynthParam::AmpSustain)),
            ),
            act(
                "Shorten Amp Decay below 150 ms: a short, plucked note.",
                "Decay is the second knob under Amp envelope.",
                |s| carve(s).is_some_and(|p| p.amp_env.decay_ms < 150.0),
                |_| Some(Target::Knob(SynthParam::AmpDecay)),
            ),
            act(
                "Close the filter a little: Cutoff to about 1 kHz.",
                "Between 500 Hz and 2 kHz.",
                |s| carve(s).is_some_and(|p| (500.0..=2000.0).contains(&p.filter.cutoff_hz)),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            act(
                "Now the Filter envelope: turn Env amount above +3 oct. Each note opens the filter, then it closes - a \u{201c}blip\u{201d}.",
                "Env amount is in the Filter section.",
                |s| carve(s).is_some_and(|p| p.filter.env_amount_oct > 3.0),
                |_| Some(Target::Knob(SynthParam::EnvAmount)),
            ),
            act(
                "Filter Decay below 150 ms: a snappier blip.",
                "Decay under Filter envelope.",
                |s| carve(s).is_some_and(|p| p.filter_env.decay_ms < 150.0),
                |_| Some(Target::Knob(SynthParam::FilterDecay)),
            ),
            act(
                "Last: raise Amp Attack above 300 ms. Notes swell in instead of striking - that's how pads begin.",
                "Attack is the first knob under Amp envelope.",
                |s| carve(s).is_some_and(|p| p.amp_env.attack_ms > 300.0),
                |_| Some(Target::Knob(SynthParam::AmpAttack)),
            ),
            info(
                "Attack is how fast a note starts, Decay how fast it falls, Sustain the level while held, Release the tail \
                 after you let go. The Amp envelope moves loudness, the Filter envelope moves brightness.",
            ),
        ],
    },
    Lesson {
        id: CARVE_MOVEMENT,
        group: CARVE,
        title: "Movement",
        steps: &[
            act("Press Space: two held chords.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            act(
                "First darken it: Cutoff to about 700 Hz.",
                "Between 400 Hz and 1.2 kHz.",
                |s| carve(s).is_some_and(|p| (400.0..=1200.0).contains(&p.filter.cutoff_hz)),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            act(
                "LFOs move knobs for you. Drag the LFO 1 pill (top of Modulation) and drop it on the Cutoff knob.",
                "Press on \u{201c}LFO 1\u{201d}, hold, move onto Cutoff, let go.",
                |s| carve(s).is_some_and(|p| p.lfo1.target == LfoTarget::Cutoff),
                |_| Some(Target::LfoPill(1)),
            ),
            act(
                "Turn LFO 1 Depth past 50%: the filter now opens and closes on its own.",
                "Depth is under LFO 1 in Modulation.",
                |s| carve(s).is_some_and(|p| p.lfo1.target == LfoTarget::Cutoff && p.lfo1.depth > 0.5),
                |_| Some(Target::Knob(SynthParam::Lfo1Depth)),
            ),
            act(
                "Slow it down: LFO 1 Rate under 1 Hz, for a long sweep.",
                "Rate is next to Depth.",
                |s| carve(s).is_some_and(|p| lfo_rate_hz(p.lfo1.rate_norm) < 1.0),
                |_| Some(Target::Knob(SynthParam::Lfo1Rate)),
            ),
            act(
                "Now speed it up past 5 Hz: a wobble.",
                "Rate up.",
                |s| carve(s).is_some_and(|p| lfo_rate_hz(p.lfo1.rate_norm) > 5.0),
                |_| Some(Target::Knob(SynthParam::Lfo1Rate)),
            ),
            act(
                "Make it wide: Unison Voices to 3 or more - detuned copies of every note, spread left and right.",
                "Voices, under Unison.",
                |s| carve(s).is_some_and(|p| p.unison.voices >= 3),
                |_| Some(Target::Knob(SynthParam::UnisonVoices)),
            ),
            act(
                "Add space: Reverb mix past 30%.",
                "Reverb, under Effects.",
                |s| carve(s).is_some_and(|p| p.fx.reverb_mix > 0.3),
                |_| Some(Target::Knob(SynthParam::ReverbMix)),
            ),
            info(
                "An LFO is a slow wave that moves another control: on Cutoff it's a sweep or wobble, on Pitch it's vibrato. \
                 Unison and reverb make any sound bigger.",
            ),
        ],
    },
    Lesson {
        id: CARVE_SYNC_FM,
        group: CARVE,
        title: "Harder sounds: sync and FM",
        steps: &[
            act(
                "Press Space: a low note, then a high one, on a plain saw.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Hear Oscillator 2 on its own: in the Mixer, turn Osc 2 up past \u{2212}12 dB and Osc 1 all the way down.",
                "The Osc 1 and Osc 2 knobs in the Mixer.",
                |s| carve(s).is_some_and(|p| p.mix.osc2_db > -12.0 && p.mix.osc1_db < -40.0),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.mix.osc2_db > -12.0, Target::Knob(SynthParam::Osc2Level)),
                        (p.mix.osc1_db < -40.0, Target::Knob(SynthParam::Osc1Level)),
                    ])
                },
            ),
            recipe(
                "Switch on Sync, above Oscillator 2.",
                "Sync restarts Oscillator 2 every time Oscillator 1 starts a cycle. Nothing changes yet: they're at the same pitch.",
                "The Sync button, left of Oscillator 2's wave buttons.",
                |s| carve(s).is_some_and(|p| p.osc2.sync),
                |_| Some(Target::Sync),
            ),
            recipe(
                "Now raise Oscillator 2's Octave to +2.",
                "The note stays the same, but the tone turns hard and nasal. Oscillator 2 races ahead and gets cut off mid-cycle, over and over. That's the classic sync lead.",
                "Two steps up on the Octave knob under Oscillator 2.",
                |s| carve(s).is_some_and(|p| p.osc2.sync && p.osc2.octave >= 2),
                |_| Some(Target::Knob(SynthParam::Osc2Octave)),
            ),
            act(
                "Switch Sync off again, and put the Octave back to 0.",
                "Click Sync, then turn the Octave knob back to the middle.",
                |s| carve(s).is_some_and(|p| !p.osc2.sync && p.osc2.octave == 0),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[(!p.osc2.sync, Target::Sync), (p.osc2.octave == 0, Target::Knob(SynthParam::Osc2Octave))])
                },
            ),
            recipe(
                "Now FM: turn Oscillator 2's FM up to about 30%.",
                "Oscillator 1 bends Oscillator 2's pitch hundreds of times a second. That makes new overtones that aren't in either wave: metallic, clangy.",
                "The FM knob under Oscillator 2. Anywhere from 20% to 50% is fine.",
                |s| carve(s).is_some_and(|p| (0.2..=0.5).contains(&p.osc2.knob_c)),
                |_| Some(Target::Knob(SynthParam::Osc2Fm)),
            ),
            recipe(
                "Switch Oscillator 2 to the sine wave.",
                "FM on a sine is the sound of electric pianos and bells: the overtones come from the FM, not from the wave.",
                "The first of Oscillator 2's wave buttons.",
                |s| carve(s).is_some_and(|p| p.osc2.waveform == Waveform::Sine && p.osc2.knob_c >= 0.2),
                |_| Some(Target::OscWave(2)),
            ),
            recipe(
                "One more control: turn Cutoff down to about 500 Hz. Listen to the high note.",
                "The low note still sounds full, but the high one is dull: the filter cuts the same frequencies from both, and the high note has less below the cutoff.",
                "The big Cutoff knob in the Filter. Anywhere below 700 Hz is fine.",
                |s| carve(s).is_some_and(|p| p.filter.cutoff_hz <= 700.0),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            recipe(
                "Turn Key track up to 100%.",
                "Now the cutoff follows the notes, so it opens up for high notes. Both notes sound alike again. Basses and leads that play across a range need it.",
                "The Key track knob in the Filter. 80% or more is fine.",
                |s| carve(s).is_some_and(|p| p.filter.key_track >= 0.8),
                |_| Some(Target::Knob(SynthParam::KeyTrack)),
            ),
            info(
                "Sync makes tearing leads, FM makes bells and metal, and key tracking keeps a sound the same across the keyboard. \
                 Try sync with the Octave at +1 and a little FM together - most hard leads use both.",
            ),
        ],
    },
    Lesson {
        id: RECIPE_BASS,
        group: RECIPES,
        title: "Deep bass",
        steps: &[
            act("Press Space: a beat and a plain saw bass.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "Weight first: Oscillator 1 Octave to -1.",
                "An octave down halves the pitch, moving the note into the range you feel in your chest more than hear.",
                "The Octave knob under Oscillator 1, one step down.",
                |s| carve(s).is_some_and(|p| p.osc1.octave == -1),
                |_| Some(Target::Knob(SynthParam::Osc1Octave)),
            ),
            recipe(
                "Sub up to about -6 dB: the sine underneath is what you feel on big speakers.",
                "The sub is a pure sine an octave below. It adds weight without buzz, because a sine has no harmonics to clutter the mix.",
                "Above -9 dB.",
                |s| carve(s).is_some_and(|p| p.mix.sub_db > -9.0),
                |_| Some(Target::Knob(SynthParam::SubLevel)),
            ),
            recipe(
                "Close the filter: Cutoff to about 250 Hz. Bass wants weight, not fizz.",
                "A saw's upper harmonics are the \u{201c}fizz\u{201d}. Closing the low-pass filter removes them and leaves the round low end.",
                "Between 150 and 450 Hz.",
                |s| carve(s).is_some_and(|p| (150.0..=450.0).contains(&p.filter.cutoff_hz)),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            recipe(
                "Make each note punch: Env amount about +2.5 oct and Filter Decay about 200 ms.",
                "The filter envelope throws the filter open at each note and closes it within 200 ms: a burst of brightness your ear reads as a punch.",
                "Env amount +1.5 to +3.5 oct; Filter Decay 100 to 350 ms.",
                |s| carve(s).is_some_and(|p| (1.5..=3.5).contains(&p.filter.env_amount_oct) && (100.0..=350.0).contains(&p.filter_env.decay_ms)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((1.5..=3.5).contains(&p.filter.env_amount_oct), Target::Knob(SynthParam::EnvAmount)),
                        ((100.0..=350.0).contains(&p.filter_env.decay_ms), Target::Knob(SynthParam::FilterDecay)),
                    ])
                },
            ),
            recipe(
                "Grit: Drive past 6 dB.",
                "Drive pushes the sound into the filter harder, adding harmonics back as grit, so the bass still cuts through on phone and laptop speakers.",
                "Drive is in the Filter section.",
                |s| carve(s).is_some_and(|p| p.filter.drive_db > 6.0),
                |_| Some(Target::Knob(SynthParam::Drive)),
            ),
            recipe(
                "Switch to Mono and set Glide to about 50 ms: notes slide into each other instead of stacking up.",
                "Mono plays one note at a time, so overlaps can't pile into mud; glide slides the pitch between notes, like a bassist's finger.",
                "Mono is at the top right of Carve; Glide is under Output (20 to 120 ms).",
                |s| carve(s).is_some_and(|p| p.voice_mode == VoiceMode::Mono && (20.0..=120.0).contains(&p.output.glide_ms)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.voice_mode == VoiceMode::Mono, Target::VoiceMode),
                        ((20.0..=120.0).contains(&p.output.glide_ms), Target::Knob(SynthParam::Glide)),
                    ])
                },
            ),
            info(
                "Deep bass: a low octave, a sub underneath, a closed filter that punches open, and mono. \
                 Compare with Carve's Deep Rave Bass preset, which adds a detuned second saw.",
            ),
        ],
    },
    Lesson {
        id: RECIPE_PAD,
        group: RECIPES,
        title: "Soft pad",
        steps: &[
            act("Press Space: two held chords on a plain saw.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "Two saws, slightly apart: Osc 2 up to about -6 dB, and Osc 2 Detune about +9 cents.",
                "Two saws a few cents apart drift in and out of step with each other - a slow, breathing movement that makes a pad feel alive rather than static.",
                "Mixer Osc 2 above -9 dB; Detune +5 to +15 cents.",
                |s| carve(s).is_some_and(|p| p.mix.osc2_db > -9.0 && (5.0..=15.0).contains(&p.osc2.knob_a_cents)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.mix.osc2_db > -9.0, Target::Knob(SynthParam::Osc2Level)),
                        ((5.0..=15.0).contains(&p.osc2.knob_a_cents), Target::Knob(SynthParam::Osc2Detune)),
                    ])
                },
            ),
            recipe(
                "Take the edge off: Cutoff about 900 Hz, Resonance low (under 20%).",
                "A pad sits behind everything else, so it shouldn't fight the lead for the highs. Low-pass it, and keep resonance down so no single frequency pokes out.",
                "Cutoff 600 Hz to 1.3 kHz; Resonance under 20%.",
                |s| carve(s).is_some_and(|p| (600.0..=1300.0).contains(&p.filter.cutoff_hz) && p.filter.resonance < 0.2),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((600.0..=1300.0).contains(&p.filter.cutoff_hz), Target::Knob(SynthParam::Cutoff)),
                        (p.filter.resonance < 0.2, Target::Knob(SynthParam::Resonance)),
                    ])
                },
            ),
            recipe(
                "Swell in, fade out: Amp Attack about 500 ms and Release about 1.2 s.",
                "A slow attack means no hard start - each chord fades in like a string section. The long release lets it hang over into the next chord, so the changes blur together.",
                "Attack 300 to 900 ms; Release 0.8 to 2 s.",
                |s| carve(s).is_some_and(|p| (300.0..=900.0).contains(&p.amp_env.attack_ms) && (800.0..=2000.0).contains(&p.amp_env.release_ms)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((300.0..=900.0).contains(&p.amp_env.attack_ms), Target::Knob(SynthParam::AmpAttack)),
                        ((800.0..=2000.0).contains(&p.amp_env.release_ms), Target::Knob(SynthParam::AmpRelease)),
                    ])
                },
            ),
            recipe(
                "Let it bloom: Env amount about +1.5 oct, and Filter Attack about 800 ms.",
                "With a slow filter attack, the filter opens gradually while each chord holds, so the pad gets brighter over time - movement without touching a knob.",
                "Env amount +1 to +2.5 oct; Filter Attack 400 ms to 1.5 s.",
                |s| carve(s).is_some_and(|p| (1.0..=2.5).contains(&p.filter.env_amount_oct) && (400.0..=1500.0).contains(&p.filter_env.attack_ms)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((1.0..=2.5).contains(&p.filter.env_amount_oct), Target::Knob(SynthParam::EnvAmount)),
                        ((400.0..=1500.0).contains(&p.filter_env.attack_ms), Target::Knob(SynthParam::FilterAttack)),
                    ])
                },
            ),
            recipe(
                "Make it wide: Unison Voices to 3 or more, Unison Detune about 18 cents.",
                "Unison stacks several detuned copies of every note and spreads them left and right - the width that makes a pad surround you instead of sitting in the middle.",
                "Voices 3+; Detune 15 to 30 cents.",
                |s| carve(s).is_some_and(|p| p.unison.voices >= 3 && (15.0..=30.0).contains(&p.unison.detune_cents)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.unison.voices >= 3, Target::Knob(SynthParam::UnisonVoices)),
                        ((15.0..=30.0).contains(&p.unison.detune_cents), Target::Knob(SynthParam::UnisonDetune)),
                    ])
                },
            ),
            recipe(
                "Shimmer and space: Chorus mix about 35%, Reverb mix about 35%, Reverb Size above 70%.",
                "Chorus adds a gentle shimmer and a big reverb puts the pad in a large room. Both blur its edges - exactly what a background sound should do.",
                "Chorus 25 to 50%; Reverb 25 to 50%; Size over 70%.",
                |s| {
                    carve(s).is_some_and(|p| {
                        (0.25..=0.5).contains(&p.fx.chorus_mix) && (0.25..=0.5).contains(&p.fx.reverb_mix) && p.fx.reverb_size > 0.7
                    })
                },
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((0.25..=0.5).contains(&p.fx.chorus_mix), Target::Knob(SynthParam::ChorusMix)),
                        ((0.25..=0.5).contains(&p.fx.reverb_mix), Target::Knob(SynthParam::ReverbMix)),
                        (p.fx.reverb_size > 0.7, Target::Knob(SynthParam::ReverbSize)),
                    ])
                },
            ),
            info(
                "Soft pad: two detuned saws, a darker filter that blooms, a slow swell, width and space. \
                 Compare it with Carve's Soft Pad preset - same ideas, a few different settings.",
            ),
        ],
    },
    Lesson {
        id: RECIPE_FLUTE,
        group: RECIPES,
        title: "Flute",
        steps: &[
            act("Press Space: a slow melody, on a buzzy saw for now.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "A flute is almost pure: set Oscillator 1 to the triangle.",
                "A triangle's harmonics fade quickly, so it's close to a pure tone - which is what a flute's column of air produces.",
                "The second wave shape above Oscillator 1.",
                |s| carve(s).is_some_and(|p| p.osc1.waveform == Waveform::Triangle),
                |_| Some(Target::OscWave(1)),
            ),
            recipe(
                "Breath: Noise to about -26 dB.",
                "A real flute is air rushing across an edge. A little noise under the tone is that breath.",
                "Between -34 and -18 dB, in the Mixer.",
                |s| carve(s).is_some_and(|p| (-34.0..=-18.0).contains(&p.mix.noise_db)),
                |_| Some(Target::Knob(SynthParam::NoiseLevel)),
            ),
            recipe(
                "Soften it: Cutoff about 2.5 kHz, so the breath isn't hissy.",
                "Noise has energy at every frequency; cutting above ~2.5 kHz keeps the soft part that reads as breath and drops the hiss.",
                "Between 1.5 and 3.5 kHz.",
                |s| carve(s).is_some_and(|p| (1500.0..=3500.0).contains(&p.filter.cutoff_hz)),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            recipe(
                "Blow into it: Amp Attack about 80 ms, so each note breathes in.",
                "It takes a moment of breath before a flute speaks. A short fade-in copies that; an instant attack gives it away as a synth.",
                "Between 50 and 150 ms.",
                |s| carve(s).is_some_and(|p| (50.0..=150.0).contains(&p.amp_env.attack_ms)),
                |_| Some(Target::Knob(SynthParam::AmpAttack)),
            ),
            recipe(
                "Vibrato: LFO 2 already points at Pitch. Set its Rate to about 5 Hz and Depth to about 20%.",
                "Flautists add vibrato with their breath, about five wobbles a second. LFO 2 moving the pitch that fast, and only slightly, is exactly that.",
                "Rate 4 to 7 Hz, Depth 10 to 35%, under LFO 2.",
                |s| carve(s).is_some_and(vibrato),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((4.0..=7.0).contains(&lfo_rate_hz(p.lfo2.rate_norm)), Target::Knob(SynthParam::Lfo2Rate)),
                        ((0.1..=0.35).contains(&p.lfo2.depth), Target::Knob(SynthParam::Lfo2Depth)),
                    ])
                },
            ),
            recipe(
                "A room to play in: Reverb mix about 30%.",
                "Wind instruments are almost always heard in a room, and the room's echoes are part of their sound.",
                "Between 20 and 50%.",
                |s| carve(s).is_some_and(|p| (0.2..=0.5).contains(&p.fx.reverb_mix)),
                |_| Some(Target::Knob(SynthParam::ReverbMix)),
            ),
            info(
                "Flute: a soft wave, a breath of noise, a gentle attack and vibrato. The same idea - soft wave, \
                 slow attack, vibrato - gives you recorders, ocarinas and the bansuri.",
            ),
        ],
    },
    Lesson {
        id: RECIPE_HARP,
        group: RECIPES,
        title: "Indian harp",
        steps: &[
            act(
                "Press Space. This cascade uses the notes of raga Malkauns (A, C, D, F, G), a late-night raga.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            recipe(
                "Strings are bright: keep the saw on Oscillator 1, and bring in Oscillator 2 as a square, one octave up, at about -10 dB.",
                "A struck string is richest in harmonics at the moment it's hit. A square an octave up adds bright upper harmonics on top of the saw.",
                "Osc 2 wave: square. Octave: +1. Mixer Osc 2: above -14 dB.",
                |s| carve(s).is_some_and(|p| p.osc2.waveform == Waveform::Square && p.osc2.octave == 1 && p.mix.osc2_db > -14.0),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.osc2.waveform == Waveform::Square, Target::OscWave(2)),
                        (p.osc2.octave == 1, Target::Knob(SynthParam::Osc2Octave)),
                        (p.mix.osc2_db > -14.0, Target::Knob(SynthParam::Osc2Level)),
                    ])
                },
            ),
            recipe(
                "The pluck: Amp Attack all the way down (under 5 ms), Sustain to 0, Decay about 1 second.",
                "A pluck is all start and fade: it speaks instantly and nothing is held, so attack near zero and sustain at zero.",
                "Decay between 0.6 and 1.6 s.",
                |s| carve(s).is_some_and(|p| p.amp_env.attack_ms < 5.0 && p.amp_env.sustain < 0.05 && (600.0..=1600.0).contains(&p.amp_env.decay_ms)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.amp_env.attack_ms < 5.0, Target::Knob(SynthParam::AmpAttack)),
                        (p.amp_env.sustain < 0.05, Target::Knob(SynthParam::AmpSustain)),
                        ((600.0..=1600.0).contains(&p.amp_env.decay_ms), Target::Knob(SynthParam::AmpDecay)),
                    ])
                },
            ),
            recipe(
                "Let the strings ring after each note: Amp Release about 1 second.",
                "Strings keep ringing after the finger leaves them. A long release does the same, so the cascade blurs into a shimmer.",
                "Between 0.6 and 1.6 s.",
                |s| carve(s).is_some_and(|p| (600.0..=1600.0).contains(&p.amp_env.release_ms)),
                |_| Some(Target::Knob(SynthParam::AmpRelease)),
            ),
            recipe(
                "A string is brightest when struck, then mellows: Cutoff about 1 kHz, Env amount about +3 oct, Filter Decay about 300 ms.",
                "Real strings lose their high harmonics first as they ring. A filter that starts open and closes over 300 ms copies that.",
                "Cutoff 0.6 to 1.8 kHz, Env amount +2 to +4 oct, Filter Decay 150 to 500 ms.",
                |s| {
                    carve(s).is_some_and(|p| {
                        (600.0..=1800.0).contains(&p.filter.cutoff_hz)
                            && (2.0..=4.0).contains(&p.filter.env_amount_oct)
                            && (150.0..=500.0).contains(&p.filter_env.decay_ms)
                    })
                },
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((600.0..=1800.0).contains(&p.filter.cutoff_hz), Target::Knob(SynthParam::Cutoff)),
                        ((2.0..=4.0).contains(&p.filter.env_amount_oct), Target::Knob(SynthParam::EnvAmount)),
                        ((150.0..=500.0).contains(&p.filter_env.decay_ms), Target::Knob(SynthParam::FilterDecay)),
                    ])
                },
            ),
            recipe(
                "Shimmer: Chorus mix about 30%.",
                "Swarmandal and santoor strings come in courses of nearly-matching strings; chorus imitates that slightly-out-of-tune doubling.",
                "Between 20 and 50%, under Effects.",
                |s| carve(s).is_some_and(|p| (0.2..=0.5).contains(&p.fx.chorus_mix)),
                |_| Some(Target::Knob(SynthParam::ChorusMix)),
            ),
            recipe(
                "Space around it: Reverb mix about 40%, and Size above 70%.",
                "A big reverb lets each note hang into the next - the sustaining wash these instruments are known for.",
                "Reverb mix 30 to 60%; Size over 70%.",
                |s| carve(s).is_some_and(|p| (0.3..=0.6).contains(&p.fx.reverb_mix) && p.fx.reverb_size > 0.7),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((0.3..=0.6).contains(&p.fx.reverb_mix), Target::Knob(SynthParam::ReverbMix)),
                        (p.fx.reverb_size > 0.7, Target::Knob(SynthParam::ReverbSize)),
                    ])
                },
            ),
            info(
                "A plucked-string sound in the spirit of the swarmandal and santoor: bright attack, a filter that \
                 mellows, a long ring and a big room. Write your own cascade: Malkauns has no E and no B.",
            ),
        ],
    },
    Lesson {
        id: RECIPE_TANPURA,
        group: RECIPES,
        title: "Tanpura",
        steps: &[
            act(
                "Press Space. The tanpura plucks its four strings in turn - Pa, Sa, Sa, low Sa - over and over, under the whole performance.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            recipe(
                "Pluck and ring: Amp Attack under 10 ms, Decay near the top (over 1.5 s), Sustain about 40%, Release over 1.5 s.",
                "A tanpura string is plucked and then rings for seconds. With long decay and release, each string is still sounding when the next is plucked - so the four blur into one continuous drone instead of four notes.",
                "Attack is already short. Decay and Release: drag them nearly all the way up.",
                |s| {
                    carve(s).is_some_and(|p| {
                        p.amp_env.attack_ms < 10.0
                            && p.amp_env.decay_ms >= 1500.0
                            && (0.3..=0.6).contains(&p.amp_env.sustain)
                            && p.amp_env.release_ms >= 1500.0
                    })
                },
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.amp_env.attack_ms < 10.0, Target::Knob(SynthParam::AmpAttack)),
                        (p.amp_env.decay_ms >= 1500.0, Target::Knob(SynthParam::AmpDecay)),
                        ((0.3..=0.6).contains(&p.amp_env.sustain), Target::Knob(SynthParam::AmpSustain)),
                        (p.amp_env.release_ms >= 1500.0, Target::Knob(SynthParam::AmpRelease)),
                    ])
                },
            ),
            recipe(
                "Deep and round: Cutoff about 700 Hz, Resonance about 40%.",
                "A tanpura is dark and warm, not bright. The resonance picks out a narrow band of harmonics at the cutoff - the band the next step will set in motion.",
                "Cutoff 450 Hz to 1.1 kHz; Resonance 30 to 55%.",
                |s| carve(s).is_some_and(|p| (450.0..=1100.0).contains(&p.filter.cutoff_hz) && (0.3..=0.55).contains(&p.filter.resonance)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((450.0..=1100.0).contains(&p.filter.cutoff_hz), Target::Knob(SynthParam::Cutoff)),
                        ((0.3..=0.55).contains(&p.filter.resonance), Target::Knob(SynthParam::Resonance)),
                    ])
                },
            ),
            recipe(
                "The jawari bloom: Env amount about +2 oct, Filter Attack about 500 ms, Filter Decay over 1.2 s.",
                "A tanpura's bridge (the jawari) is curved, so the string buzzes against it and its upper harmonics swell a moment after the pluck. A slow filter attack does the same thing: every note starts dark and blooms bright.",
                "Env amount +1.5 to +3 oct; Filter Attack 300 to 900 ms; Filter Decay over 1.2 s.",
                |s| {
                    carve(s).is_some_and(|p| {
                        (1.5..=3.0).contains(&p.filter.env_amount_oct)
                            && (300.0..=900.0).contains(&p.filter_env.attack_ms)
                            && p.filter_env.decay_ms >= 1200.0
                    })
                },
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((1.5..=3.0).contains(&p.filter.env_amount_oct), Target::Knob(SynthParam::EnvAmount)),
                        ((300.0..=900.0).contains(&p.filter_env.attack_ms), Target::Knob(SynthParam::FilterAttack)),
                        (p.filter_env.decay_ms >= 1200.0, Target::Knob(SynthParam::FilterDecay)),
                    ])
                },
            ),
            recipe(
                "The buzz: Drive past 6 dB.",
                "That jawari buzz is the string rattling against wood - a gentle distortion. Drive adds the same rasp of extra harmonics.",
                "Drive is in the Filter section.",
                |s| carve(s).is_some_and(|p| p.filter.drive_db > 6.0),
                |_| Some(Target::Knob(SynthParam::Drive)),
            ),
            recipe(
                "The swirl: drag LFO 1 onto Cutoff, then set its Rate under 0.5 Hz and Depth about 30%.",
                "Listen to a real tanpura and its overtones seem to rotate slowly, even between plucks. A slow LFO sweeping the filter keeps the harmonics moving the same way.",
                "Press on \u{201c}LFO 1\u{201d}, drop it on Cutoff, then Rate and Depth under LFO 1.",
                |s| {
                    carve(s).is_some_and(|p| {
                        p.lfo1.target == LfoTarget::Cutoff && lfo_rate_hz(p.lfo1.rate_norm) < 0.5 && (0.15..=0.5).contains(&p.lfo1.depth)
                    })
                },
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.lfo1.target == LfoTarget::Cutoff, Target::LfoPill(1)),
                        (lfo_rate_hz(p.lfo1.rate_norm) < 0.5, Target::Knob(SynthParam::Lfo1Rate)),
                        ((0.15..=0.5).contains(&p.lfo1.depth), Target::Knob(SynthParam::Lfo1Depth)),
                    ])
                },
            ),
            recipe(
                "Strings and a room: Chorus mix about 30%, Reverb mix about 45%, Reverb Size above 80%.",
                "The four strings are never perfectly in tune with one another; chorus adds that gentle beating. A large reverb gives the drone the resonant space it's usually heard in.",
                "Chorus 20 to 45%; Reverb 30 to 60%; Size over 80%.",
                |s| {
                    carve(s).is_some_and(|p| {
                        (0.2..=0.45).contains(&p.fx.chorus_mix) && (0.3..=0.6).contains(&p.fx.reverb_mix) && p.fx.reverb_size > 0.8
                    })
                },
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((0.2..=0.45).contains(&p.fx.chorus_mix), Target::Knob(SynthParam::ChorusMix)),
                        ((0.3..=0.6).contains(&p.fx.reverb_mix), Target::Knob(SynthParam::ReverbMix)),
                        (p.fx.reverb_size > 0.8, Target::Knob(SynthParam::ReverbSize)),
                    ])
                },
            ),
            info(
                "A tanpura: long-ringing plucks that overlap into a drone, a bloom and buzz from the jawari, and a slow \
                 swirl of overtones. It's tuned to Sa and Pa, so it sits under any raag - try it under the Bhairav rave demo.",
            ),
        ],
    },
    Lesson {
        id: RECIPE_REED,
        group: RECIPES,
        title: "Reed (shehnai)",
        steps: &[
            act(
                "Press Space: Raag Bhairav's signature phrase, Ga ma dha Pa, Ga ma re Sa - on a plain saw for now.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            recipe(
                "The reed: Oscillator 1 to the square, and Osc 2 (a saw) up to about -9 dB.",
                "A reed is a thin tongue snapping open and shut, and that on-off motion makes strong odd harmonics - a square wave's recipe. The saw underneath adds the even ones, for a richer, more complex reed.",
                "Osc 1 wave: square. Mixer Osc 2: above -12 dB.",
                |s| carve(s).is_some_and(|p| p.osc1.waveform == Waveform::Square && p.mix.osc2_db > -12.0),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.osc1.waveform == Waveform::Square, Target::OscWave(1)),
                        (p.mix.osc2_db > -12.0, Target::Knob(SynthParam::Osc2Level)),
                    ])
                },
            ),
            recipe(
                "The nasal honk: filter type BP (band-pass), Cutoff about 1.7 kHz, Resonance about 40%.",
                "A shehnai's narrow bore amplifies one band of frequencies, around 1-2 kHz, and that band is the honk. A band-pass filter keeps just that band - cutting both the lows and the highs, as if the sound came through a narrow pipe.",
                "BP on the filter's type switch; Cutoff 1.2 to 2.5 kHz; Resonance 25 to 55%.",
                |s| {
                    carve(s).is_some_and(|p| {
                        p.filter.filter_type == FilterType::Bp
                            && (1200.0..=2500.0).contains(&p.filter.cutoff_hz)
                            && (0.25..=0.55).contains(&p.filter.resonance)
                    })
                },
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.filter.filter_type == FilterType::Bp, Target::FilterType),
                        ((1200.0..=2500.0).contains(&p.filter.cutoff_hz), Target::Knob(SynthParam::Cutoff)),
                        ((0.25..=0.55).contains(&p.filter.resonance), Target::Knob(SynthParam::Resonance)),
                    ])
                },
            ),
            recipe(
                "Breath: Noise to about -32 dB.",
                "Air forced through a reed always hisses a little; a touch of noise under the tone is that breath.",
                "Between -40 and -24 dB, in the Mixer.",
                |s| carve(s).is_some_and(|p| (-40.0..=-24.0).contains(&p.mix.noise_db)),
                |_| Some(Target::Knob(SynthParam::NoiseLevel)),
            ),
            recipe(
                "Let it speak: Amp Attack about 35 ms.",
                "A reed needs a moment of breath pressure before it starts to vibrate, so notes begin with a soft push rather than a click.",
                "Between 20 and 80 ms.",
                |s| carve(s).is_some_and(|p| (20.0..=80.0).contains(&p.amp_env.attack_ms)),
                |_| Some(Target::Knob(SynthParam::AmpAttack)),
            ),
            recipe(
                "Meend: switch to Mono and set Glide to about 70 ms.",
                "Shehnai players slide between notes - meend - instead of jumping. Mono with glide does exactly that. Listen to the fall from re to Sa at the end of the phrase: in Bhairav that slide is the signature.",
                "Mono at the top right of Carve; Glide 40 to 150 ms, under Output.",
                |s| carve(s).is_some_and(|p| p.voice_mode == VoiceMode::Mono && (40.0..=150.0).contains(&p.output.glide_ms)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.voice_mode == VoiceMode::Mono, Target::VoiceMode),
                        ((40.0..=150.0).contains(&p.output.glide_ms), Target::Knob(SynthParam::Glide)),
                    ])
                },
            ),
            recipe(
                "Vibrato: LFO 2 (on Pitch) at about 5.5 Hz, Depth about 15%.",
                "Reed players shape held notes with breath vibrato, a little over five wobbles a second - it's what makes a line sound sung rather than typed.",
                "Rate 4 to 7 Hz, Depth 10 to 35%, under LFO 2.",
                |s| carve(s).is_some_and(vibrato),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((4.0..=7.0).contains(&lfo_rate_hz(p.lfo2.rate_norm)), Target::Knob(SynthParam::Lfo2Rate)),
                        ((0.1..=0.35).contains(&p.lfo2.depth), Target::Knob(SynthParam::Lfo2Depth)),
                    ])
                },
            ),
            recipe(
                "A hall to play in: Reverb mix about 30%.",
                "The shehnai is an outdoor, ceremonial instrument that carries far; reverb gives it the space it's meant to fill.",
                "Between 20 and 45%.",
                |s| carve(s).is_some_and(|p| (0.2..=0.45).contains(&p.fx.reverb_mix)),
                |_| Some(Target::Knob(SynthParam::ReverbMix)),
            ),
            info(
                "A reed: square and saw for the vibrating tongue, a band-pass filter for the nasal bore, breath, a soft \
                 attack, meend and vibrato. Widen the filter (LP 24, higher cutoff) and it becomes a clarinet or a sax.",
            ),
        ],
    },
    Lesson {
        id: RECIPE_LEAD,
        group: RECIPES,
        title: "Lead melody",
        steps: &[
            act("Press Space: a beat and a hook on a plain saw.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "A lead has to cut through: Oscillator 1 to the square, and Osc 2 up to about -8 dB, detuned about +7 cents.",
                "Square plus saw gives a hollow body with a bright edge, and detuning them a few cents thickens it so it stands apart from the chords.",
                "Osc 1 wave: square. Mixer Osc 2: above -12 dB. Osc 2 Detune: +3 to +15 cents.",
                |s| {
                    carve(s).is_some_and(|p| {
                        p.osc1.waveform == Waveform::Square && p.mix.osc2_db > -12.0 && (3.0..=15.0).contains(&p.osc2.knob_a_cents)
                    })
                },
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.osc1.waveform == Waveform::Square, Target::OscWave(1)),
                        (p.mix.osc2_db > -12.0, Target::Knob(SynthParam::Osc2Level)),
                        ((3.0..=15.0).contains(&p.osc2.knob_a_cents), Target::Knob(SynthParam::Osc2Detune)),
                    ])
                },
            ),
            recipe(
                "Bright but not harsh: Cutoff about 3 kHz, Resonance about 30%.",
                "Our ears are most sensitive around 2-4 kHz. A lead that lives there, with a little resonance, cuts through without being loud.",
                "Cutoff 1.8 to 4.5 kHz; Resonance 20 to 45%.",
                |s| carve(s).is_some_and(|p| (1800.0..=4500.0).contains(&p.filter.cutoff_hz) && (0.2..=0.45).contains(&p.filter.resonance)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((1800.0..=4500.0).contains(&p.filter.cutoff_hz), Target::Knob(SynthParam::Cutoff)),
                        ((0.2..=0.45).contains(&p.filter.resonance), Target::Knob(SynthParam::Resonance)),
                    ])
                },
            ),
            recipe(
                "Switch to Mono with Glide about 60 ms: notes slide like a voice.",
                "Mono with glide makes each note connect to the next, the way a singer or a guitar bend moves between pitches.",
                "Mono at the top right of Carve; Glide 30 to 120 ms, under Output.",
                |s| carve(s).is_some_and(|p| p.voice_mode == VoiceMode::Mono && (30.0..=120.0).contains(&p.output.glide_ms)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.voice_mode == VoiceMode::Mono, Target::VoiceMode),
                        ((30.0..=120.0).contains(&p.output.glide_ms), Target::Knob(SynthParam::Glide)),
                    ])
                },
            ),
            recipe(
                "Expression: vibrato on LFO 2 - Rate about 5 Hz, Depth about 15%.",
                "Singers and guitarists add vibrato to held notes. It's what makes a line sound performed rather than programmed.",
                "Rate 4 to 7 Hz, Depth 10 to 35%.",
                |s| carve(s).is_some_and(vibrato),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((4.0..=7.0).contains(&lfo_rate_hz(p.lfo2.rate_norm)), Target::Knob(SynthParam::Lfo2Rate)),
                        ((0.1..=0.35).contains(&p.lfo2.depth), Target::Knob(SynthParam::Lfo2Depth)),
                    ])
                },
            ),
            recipe(
                "Polish: Chorus mix about 20% and Reverb mix about 25%.",
                "Chorus widens it and reverb gives it a place in the room - both kept small, so the lead stays up front.",
                "Chorus 10 to 40%; Reverb 15 to 40%.",
                |s| carve(s).is_some_and(|p| (0.1..=0.4).contains(&p.fx.chorus_mix) && (0.15..=0.4).contains(&p.fx.reverb_mix)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((0.1..=0.4).contains(&p.fx.chorus_mix), Target::Knob(SynthParam::ChorusMix)),
                        ((0.15..=0.4).contains(&p.fx.reverb_mix), Target::Knob(SynthParam::ReverbMix)),
                    ])
                },
            ),
            info(
                "Lead: two detuned oscillators, a bright filter, mono glide and vibrato. Swap the hook for your own \
                 melody - double-click the clip to open it.",
            ),
        ],
    },
    Lesson {
        id: RECIPE_KEYS,
        group: RECIPES,
        title: "Lo-fi keys",
        steps: &[
            act("Press Space: stabs of two chords, on a buzzy saw for now.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "An electric piano is soft and round: set Oscillator 1 to the sine.",
                "A sine has no harmonics at all - the smoothest tone there is. An electric piano is close to it: a struck metal bar, ringing almost purely.",
                "The first wave shape above Oscillator 1.",
                |s| carve(s).is_some_and(|p| p.osc1.waveform == Waveform::Sine),
                |_| Some(Target::OscWave(1)),
            ),
            recipe(
                "Add the bell: Oscillator 2 to the triangle, one octave up, its level about -14 dB.",
                "The ping at the start of an electric-piano note is a quieter, higher tone above the main one. A soft triangle an octave up, kept low, is that bell.",
                "Oscillator 2's wave switch and Octave knob, then Osc 2 in the Mixer (-20 to -8 dB).",
                |s| carve(s).is_some_and(|p| p.osc2.waveform == Waveform::Triangle && p.osc2.octave == 1 && (-20.0..=-8.0).contains(&p.mix.osc2_db)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.osc2.waveform == Waveform::Triangle, Target::OscWave(2)),
                        (p.osc2.octave == 1, Target::Knob(SynthParam::Osc2Octave)),
                        ((-20.0..=-8.0).contains(&p.mix.osc2_db), Target::Knob(SynthParam::Osc2Level)),
                    ])
                },
            ),
            recipe(
                "Strike and fade: Amp Decay about 1.5 s, Sustain down near 0, Release about 1.5 s.",
                "A piano note is struck, then fades whether you hold the key or not. A long decay to almost nothing, and a long release, let every short click ring out - like holding the sustain pedal.",
                "Decay and Release 1 to 2.5 s, Sustain under 30%.",
                |s| carve(s).is_some_and(struck),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((1000.0..=2500.0).contains(&p.amp_env.decay_ms), Target::Knob(SynthParam::AmpDecay)),
                        (p.amp_env.sustain <= 0.3, Target::Knob(SynthParam::AmpSustain)),
                        ((1000.0..=2500.0).contains(&p.amp_env.release_ms), Target::Knob(SynthParam::AmpRelease)),
                    ])
                },
            ),
            recipe(
                "Dust: Cutoff about 1.5 kHz.",
                "Old recordings lose their top end. Cutting the highs is most of what makes a sound feel like it's playing off a worn record.",
                "Between 1 and 2.2 kHz.",
                |s| carve(s).is_some_and(|p| (1000.0..=2200.0).contains(&p.filter.cutoff_hz)),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            recipe(
                "Tape wobble: LFO 2 already points at Pitch. Rate about 0.6 Hz, Depth about 8%.",
                "Old tape never ran at quite the same speed, so the pitch drifts slowly up and down. A slow, shallow pitch wobble is that warble - lo-fi's signature.",
                "Rate 0.3 to 1 Hz, Depth 4 to 15%, under LFO 2.",
                |s| carve(s).is_some_and(tape_wobble),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((0.3..=1.0).contains(&lfo_rate_hz(p.lfo2.rate_norm)), Target::Knob(SynthParam::Lfo2Rate)),
                        ((0.04..=0.15).contains(&p.lfo2.depth), Target::Knob(SynthParam::Lfo2Depth)),
                    ])
                },
            ),
            recipe(
                "Space: Chorus mix and Reverb mix about 30% each.",
                "Chorus's slightly detuned copies add to the warble, and reverb puts the keys in a room, softening every edge.",
                "Each between 15 and 50%.",
                |s| carve(s).is_some_and(|p| (0.15..=0.5).contains(&p.fx.chorus_mix) && (0.15..=0.5).contains(&p.fx.reverb_mix)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((0.15..=0.5).contains(&p.fx.chorus_mix), Target::Knob(SynthParam::ChorusMix)),
                        ((0.15..=0.5).contains(&p.fx.reverb_mix), Target::Knob(SynthParam::ReverbMix)),
                    ])
                },
            ),
            info(
                "Lo-fi keys: a soft sine with a bell on top, struck and fading, dulled and warbling like old tape. It's \
                 the sound of the Lo-fi project - and more Depth makes it seasick on purpose.",
            ),
        ],
    },
    Lesson {
        id: ARRANGE_HOUSE,
        group: ARRANGEMENT,
        title: "House: how a track is built",
        steps: &[
            info(
                "This is the House demo, laid out the way most house and techno is: in blocks of 8 bars. The markers on the ruler \
                 name each section - Intro, Build, Groove, Breakdown, Drop, Outro.",
            ),
            act(
                "Press Space and listen to the Intro: a kick and a quiet pad, little else. Intros are sparse on purpose - DJs mix a new track in over the old one here.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Keep listening until bar 9, the Build: the clap and the bass arrive. Every 8 bars something comes in or drops out - that's what keeps a loop feeling like a journey.",
                "The glowing bar on the ruler is bar 9. Or click it to jump there.",
                |s| s.playhead >= bars(8),
                |_| Some(Target::RulerBar(8)),
            ),
            act(
                "Jump to the Breakdown: click the ruler at bar 33. The kick stops and the chords take over - the tension comes from waiting for the kick to return.",
                "Click the glowing bar near the Breakdown marker.",
                |s| in_bars(s, 32, 40),
                |_| Some(Target::RulerBar(32)),
            ),
            act(
                "Feel how much the kick does: mute the Drums track (its M button) for a moment.",
                "M is on the Drums track's header, bottom left.",
                |s| track_named(s, "Drums").is_some_and(|t| t.mute),
                |s| track_named(s, "Drums").map(|t| Target::Mute(t.id)),
            ),
            act(
                "Bring it back: click M again.",
                "The same M button.",
                |s| track_named(s, "Drums").is_some_and(|t| !t.mute),
                |s| track_named(s, "Drums").map(|t| Target::Mute(t.id)),
            ),
            act(
                "Now jump to the Drop at bar 41: kick, bass, stabs and the lead hook all at once - the payoff the breakdown made you wait for.",
                "Click the glowing bar near the Drop marker.",
                |s| in_bars(s, 40, 56),
                |_| Some(Target::RulerBar(40)),
            ),
            act(
                "Mark a spot of your own: right-click the ruler at bar 25 and choose Add marker. Markers are how you plan a song before it's written.",
                "Right-click the glowing bar, then \u{201c}Add marker here\u{201d}.",
                |s| s.arrangement.markers.len() > 6,
                |_| Some(Target::RulerBar(24)),
            ),
            info(
                "The shape: Intro 8, Build 8, Groove 16, Breakdown 8, Drop 16, Outro 8 bars - 64 bars, two minutes at 128 BPM. \
                 Club versions double every section (5 to 7 minutes) so DJs have room to mix. Rave, techno and hardcore use the \
                 same bones, faster (130 to 175 BPM), with snare rolls and risers for the build and a harder drop.",
            ),
        ],
    },
    Lesson {
        id: ARRANGE_BHAIRAV,
        group: ARRANGEMENT,
        title: "Bhairav: a raag as a rave",
        steps: &[
            info(
                "A raag performance starts slowly: the alap, just the drone and a voice finding Sa, before any rhythm. \
                 The Bhairav rave borrows that shape and dresses it as a dance track.",
            ),
            act(
                "Press Space. First the tanpura alone, then hats: Sa is established before anything else, the way a raag begins.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Solo the Tanpura (its S button): Sa and Pa, held for the whole song. Every other part is heard against this.",
                "S is on the Tanpura track's header.",
                |s| track_named(s, "Tanpura").is_some_and(|t| t.solo),
                |s| track_named(s, "Tanpura").map(|t| Target::Solo(t.id)),
            ),
            act(
                "Un-solo it: click S again.",
                "The same S button.",
                |s| track_named(s, "Tanpura").is_some_and(|t| !t.solo),
                |s| track_named(s, "Tanpura").map(|t| Target::Solo(t.id)),
            ),
            act(
                "Jump to the Build at bar 9: the bass arrives, pulling from komal re back to Sa on every beat - Bhairav's gravity, as a groove.",
                "Click the glowing bar near the Build marker.",
                |s| in_bars(s, 8, 16),
                |_| Some(Target::RulerBar(8)),
            ),
            act(
                "Jump to the Break at bar 33: the beat stops and the reed sings Bhairav's phrase over the drone - the melodic heart, like a vocal breakdown in house.",
                "Click the glowing bar near the Break marker.",
                |s| in_bars(s, 32, 40),
                |_| Some(Target::RulerBar(32)),
            ),
            act(
                "Mute the Tanpura and listen to the reed: without Sa underneath, the melody loses its home.",
                "M on the Tanpura track's header.",
                |s| track_named(s, "Tanpura").is_some_and(|t| t.mute),
                |s| track_named(s, "Tanpura").map(|t| Target::Mute(t.id)),
            ),
            act(
                "Bring the drone back: click M again.",
                "The same M button.",
                |s| track_named(s, "Tanpura").is_some_and(|t| !t.mute),
                |s| track_named(s, "Tanpura").map(|t| Target::Mute(t.id)),
            ),
            act(
                "Jump to the Drop at bar 41: beat, bass, drone and reed together.",
                "Click the glowing bar near the Drop marker.",
                |s| in_bars(s, 40, 56),
                |_| Some(Target::RulerBar(40)),
            ),
            info(
                "Drone first, then rhythm, the melody alone in the break, everything in the drop - and back to the drone at the \
                 end, finishing on Sa where it began, as a raag does. Try the shape with another raag: change the Key to its \
                 scale and rewrite the bass and reed in its notes.",
            ),
        ],
    },
    Lesson {
        id: PROJECT_GROOVE,
        group: PROJECTS,
        title: "House track 1: the groove",
        steps: &[
            info(
                "Over five parts you'll build a whole house track yourself - beat, bass, chords, arrangement and mix. \
                 Each part starts where the last one ended. Part 1: the groove every house track stands on.",
            ),
            act(
                "Add a drum track: \u{201c}+ Drums\u{201d} under the tracks.",
                "Below the track list, on the left of the timeline.",
                |s| tracks_with(s, Instrument::Drums).next().is_some(),
                |_| Some(Target::AddDrumTrack),
            ),
            act(
                "Double-click bar 1 of the Drums track to make a clip.",
                "Two quick clicks on the empty lane. The clip opens below.",
                |s| clips_on(s, Instrument::Drums).next().is_some(),
                |s| tracks_with(s, Instrument::Drums).next().map(|t| Target::Lane(t.id)),
            ),
            act(
                "Kick on every beat: the Kick row under 1, 2, 3 and 4.",
                "Grid not showing? Double-click the clip.",
                |s| drum_pattern_has(s, KICK, &[0, PPQ, 2 * PPQ, 3 * PPQ]),
                |s| row_or_clip(s, Instrument::Drums, KICK),
            ),
            act(
                "Clap on beats 2 and 4.",
                "The Clap row, under 2 and 4.",
                |s| drum_pattern_has(s, CLAP, &[PPQ, 3 * PPQ]),
                |s| row_or_clip(s, Instrument::Drums, CLAP),
            ),
            act(
                "Closed hats drive it - your pick: the classic is the 2nd and 4th square of every beat (the \u{201c}e\u{201d} \
                 and \u{201c}a\u{201d}), but every square, or a pattern of your own, works too. At least four hits.",
                "Each beat has four squares: 1 e and a. The e-and-a pattern leaves the 3rd square free for the open hat.",
                |s| clips_on(s, Instrument::Drums).any(|c| pitch_count(c, CLOSED_HAT) >= 4),
                |s| row_or_clip(s, Instrument::Drums, CLOSED_HAT),
            ),
            act(
                "Open hat on the \u{201c}and\u{201d} of every beat - the 3rd square.",
                "The Open Hat row, halfway between the beats.",
                |s| drum_pattern_has(s, OPEN_HAT, &OFFBEATS),
                |s| row_or_clip(s, Instrument::Drums, OPEN_HAT),
            ),
            act("Press Space: that's a house groove.", "Or the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "Your turn: make it yours. Add one hit the groove doesn't have yet - a Snare on the very last square, a \
                 Kick just before the bar ends, a Clap somewhere new. Listen while it plays; keep what you like.",
                "One hit away from the obvious places is what makes a groove sound played rather than programmed.",
                "Anything outside the pattern counts. Don't like it? Click the note again to remove it and try another.",
                groove_has_extra,
                |s| row_or_clip(s, Instrument::Drums, SNARE),
            ),
            act(
                "Stretch the clip out to bar 17: drag its right edge. Sixteen bars of groove to build on.",
                "Grab the very end of the clip in the timeline.",
                |s| clips_on(s, Instrument::Drums).any(|c| loops(c, 16)),
                |s| tracks_with(s, Instrument::Drums).next().map(|t| Target::Lane(t.id)),
            ),
            info(
                "Kick for the pulse, clap for the backbeat, closed hats for the drive and the open hat for the bounce. \
                 Next: a bassline that locks to it.",
            ),
        ],
    },
    Lesson {
        id: PROJECT_BASS,
        group: PROJECTS,
        title: "House track 2: the bassline",
        steps: &[
            act(
                "Add a MIDI track for the bass: \u{201c}+ MIDI track\u{201d}.",
                "Below the track list.",
                |s| tracks_with(s, Instrument::Carve).next().is_some(),
                |_| Some(Target::AddMidiTrack),
            ),
            act(
                "Pick its sound: click the preset name at the top of Carve and choose Deep Bass - the one you built in the recipe.",
                "Or step through with the \u{2039} \u{203a} arrows.",
                |s| selected(s).is_some_and(|t| t.instrument == Some(Instrument::Carve)) && s.synth.name == "Deep Bass",
                |_| Some(Target::Preset("Deep Bass")),
            ),
            act(
                "The bass comes in after the drums: double-click bar 5 of the bass track.",
                "Bar 5, not bar 1 - four bars of drums alone first.",
                |s| project_bass(s).is_some_and(|b| clips_of(s, b.id).any(|c| c.start == bars(4))),
                |s| s.selected_track.map(Target::Lane),
            ),
            act(
                "The line is four bars long, one bar per chord: click + next to Pattern until it says 4 bars.",
                "Above the grid.",
                |s| project_bass(s).is_some_and(|b| clips_of(s, b.id).any(|c| c.content_len() == bars(4))),
                |_| Some(Target::PatternPlus),
            ),
            act(
                "Bar 1: A, the bottom row. House bass usually sits on the four off-beats, between the kicks - but the \
                 rhythm is yours: at least two A's in bar 1.",
                "Off-beats: two squares after each beat number, where the open hat plays.",
                |s| bass_bar(s, 0),
                |s| row_or_clip(s, Instrument::Carve, PROJECT_BASS_ROOTS[0]),
            ),
            act(
                "Bar 2: C, in the same rhythm (or a new one).",
                "Bar 2 starts at the 2 mark along the top.",
                |s| bass_bar(s, 1),
                |s| row_or_clip(s, Instrument::Carve, PROJECT_BASS_ROOTS[1]),
            ),
            act("Bar 3: on D.", "Bar 3 starts at the 3 mark.", |s| bass_bar(s, 2), |s| row_or_clip(s, Instrument::Carve, PROJECT_BASS_ROOTS[2])),
            act("Bar 4: back to C.", "Bar 4 starts at the 4 mark.", |s| bass_bar(s, 3), |s| row_or_clip(s, Instrument::Carve, PROJECT_BASS_ROOTS[3])),
            act(
                "Stretch the bass clip to bar 17, level with the drums.",
                "Drag its right edge.",
                |s| project_bass(s).is_some_and(|b| clips_of(s, b.id).any(|c| c.end() >= bars(16) && looping(c))),
                |s| project_bass(s).map(|b| Target::Lane(b.id)),
            ),
            act("Press Space.", "Or the play button.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "Your turn: swap one bass note for another note of its chord. The chords you'll add are A minor (A C E), \
                 C major (C E G), D sus (D G A) and C again - so in bar 1, try an E instead of one A.",
                "Roots make the bass solid; other notes of the chord make it move. Both belong to the chord, so neither clashes.",
                "Bar 1: E, a few rows above A. Bar 2: E or G. Bar 3: G or A. Click a note to remove it, click the new row to add one.",
                bass_moves,
                |s| row_or_clip(s, Instrument::Carve, PROJECT_CHORDS_NOTES[0][2]),
            ),
            info(
                "The bass plays the root of each chord you'll add next - A, C, D, C - in the gaps between the kicks. \
                 Next: the chords themselves.",
            ),
        ],
    },
    Lesson {
        id: PROJECT_CHORDS,
        group: PROJECTS,
        title: "House track 3: chords",
        steps: &[
            act(
                "One more MIDI track, for chords.",
                "\u{201c}+ MIDI track\u{201d}.",
                |s| tracks_with(s, Instrument::Carve).count() >= 2,
                |_| Some(Target::AddMidiTrack),
            ),
            act(
                "Preset: Soft Pad - its slow swell and long tail turn short hits into lush chords.",
                "The preset name at the top of Carve.",
                |s| project_chord_track(s).is_some_and(|t| Some(t.id) == s.selected_track) && s.synth.name == "Soft Pad",
                |_| Some(Target::Preset("Soft Pad")),
            ),
            act(
                "The chords arrive at bar 9: double-click bar 9 of the new track.",
                "Eight bars of drums and bass first.",
                |s| project_chord_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| c.start == bars(8))),
                |s| s.selected_track.map(Target::Lane),
            ),
            act(
                "Four chords, four bars: Pattern + until it says 4 bars.",
                "Above the grid.",
                |s| project_chord_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| c.content_len() == bars(4))),
                |_| Some(Target::PatternPlus),
            ),
            act(
                "Bar 1, A minor: A, C and E, stacked on the very first square. (A chord is its three notes: anywhere in \
                 the bar, in any octave, still counts.)",
                "Three clicks in the same column: A (bottom row), C, E.",
                |s| chord_bar(s, 0),
                |s| chord_target(s, 0),
            ),
            act(
                "Bar 2, C major: C, E and G on its first square.",
                "Bar 2 starts at the 2 mark.",
                |s| chord_bar(s, 1),
                |s| chord_target(s, 1),
            ),
            act(
                "Bar 3, D suspended: D, G and A.",
                "Bar 3 starts at the 3 mark. \u{201c}Suspended\u{201d}: no third, so it floats.",
                |s| chord_bar(s, 2),
                |s| chord_target(s, 2),
            ),
            act("Bar 4, C major again: C, E, G.", "Bar 4 starts at the 4 mark.", |s| chord_bar(s, 3), |s| chord_target(s, 3)),
            act(
                "Stretch the chords to bar 17.",
                "Drag the clip's right edge.",
                |s| project_chord_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| c.end() >= bars(16) && looping(c))),
                |s| project_chord_track(s).map(|t| Target::Lane(t.id)),
            ),
            act("Press Space: drums, bass and chords.", "Or the play button.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "Your turn: give the chords a rhythm. Add a second hit of a chord later in its bar - try A, C and E again \
                 on the \u{201c}and\u{201d} of beat 2 in bar 1.",
                "Chords that only land on the one sit still; a second hit answers the beat, and the gaps between hits become the groove.",
                "The \u{201c}and\u{201d} of 2: two squares after the 2 mark. Or any square you like.",
                chords_have_rhythm,
                |s| chord_target(s, 0),
            ),
            info(
                "Am, C, Dsus, C - and the bass under them plays each chord's root. Notice the chords share notes \
                 (C and E in the first two, G in the next), so they flow. Next: turning 16 bars into a song.",
            ),
        ],
    },
    Lesson {
        id: PROJECT_ARRANGE,
        group: PROJECTS,
        title: "House track 4: arrangement",
        steps: &[
            info(
                "Right now the track only builds. A song needs tension and release: you'll extend it to 32 bars and \
                 cut a breakdown in the middle, where the drums and bass drop out.",
            ),
            act(
                "Plan it with markers: right-click the ruler at bars 1, 9, 17 and 25 and Add marker at each (Intro, Groove, Breakdown, Drop).",
                "Right-click the glowing bar, then \u{201c}Add marker here\u{201d}. Double-click a marker to rename it.",
                |s| MARKER_BARS.iter().all(|&b| has_marker_at(s, b)),
                |s| MARKER_BARS.iter().find(|&&b| !has_marker_at(s, b)).map(|&b| Target::RulerBar(b)),
            ),
            act(
                "Extend everything to bar 33: stretch the drums, bass and chords clips.",
                "Drag each clip's right edge to bar 33.",
                |s| ["Drums", "Bass", "Chords"].iter().all(|n| reaches(s, n, 32)),
                |s| ["Drums", "Bass", "Chords"].iter().find(|n| !reaches(s, n, 32)).and_then(|n| track_named(s, n)).map(|t| Target::Lane(t.id)),
            ),
            act(
                "Cut at the breakdown: click the ruler at bar 17, then press Ctrl+E (\u{2318}E on a Mac). Every clip there splits in two.",
                "Click the glowing bar first - the split happens at the playhead.",
                |s| starts_at(s, "Drums", 16),
                |_| Some(Target::RulerBar(16)),
            ),
            act(
                "And where the drop comes back: click the ruler at bar 25 and Ctrl+E again.",
                "The glowing bar.",
                |s| starts_at(s, "Drums", 24),
                |_| Some(Target::RulerBar(24)),
            ),
            act(
                "Empty the breakdown: click the Drums piece between bars 17 and 25 and press Delete.",
                "Just that middle piece.",
                |s| silent_in(s, "Drums", 16, 24) && !silent_in(s, "Drums", 24, 32),
                |s| track_named(s, "Drums").map(|t| Target::Lane(t.id)),
            ),
            act(
                "Same for the Bass piece in bars 17 to 25 - only the chords remain there.",
                "Click it, then Delete.",
                |s| silent_in(s, "Bass", 16, 24) && !silent_in(s, "Bass", 24, 32),
                |s| track_named(s, "Bass").map(|t| Target::Lane(t.id)),
            ),
            act(
                "Hear it: click the ruler at bar 13 and press Space. The groove falls away into the breakdown, then everything slams back at bar 25.",
                "Click the glowing bar, then Space.",
                |s| s.playing && s.playhead >= bars(12),
                |_| Some(Target::RulerBar(12)),
            ),
            info(
                "That drop only hits because the breakdown took the kick and bass away first. Taking things out is \
                 as important as putting them in. Last part: movement and the final mix.",
            ),
        ],
    },
    Lesson {
        id: PROJECT_FINISH,
        group: PROJECTS,
        title: "House track 5: movement and mix",
        steps: &[
            act(
                "The Chords track is selected, so its Carve is below. Automate its filter: right-click the Cutoff knob and choose Automate.",
                "A lane appears under the Chords track. (If Carve isn't showing, click the Chords track's name first.)",
                |s| chords_cutoff_lane(s).is_some(),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            act(
                "Build the breakdown: in the new lane, click a point low at bar 17 and another high at bar 25 - the filter opens as the drop approaches.",
                "Click in the lane to add a point; drag a point to move it.",
                |s| chords_cutoff_lane(s).is_some_and(|l| rises(l, 16, 24)),
                |s| track_named(s, "Chords").map(|t| Target::Automation(t.id)),
            ),
            act(
                "Balance: pads sit behind the beat. Drag the Chords track's fader down to about -10 dB.",
                "The fader is on the right of the track header; its level shows beside it.",
                |s| track_named(s, "Chords").is_some_and(|t| (-13.0..=-7.0).contains(&t.gain_db)),
                |_| None,
            ),
            act(
                "Listen from the top: press Home, then Space - the whole track.",
                "Home jumps to the start.",
                |s| s.playing && s.playhead < bars(4),
                |_| Some(Target::Play),
            ),
            info(
                "You built a house track: groove, bass, chords, an arrangement with a breakdown and drop, and \
                 automation for movement. Save it (Ctrl+S, \u{2318}S on a Mac) and export it (File \u{2192} Export \
                 Audio) to play it anywhere.",
            ),
        ],
    },
    Lesson {
        id: MIX_LEVELS,
        group: MIXING,
        title: "Levels",
        steps: &[
            act(
                "Press Space and listen: your house track, badly mixed. The chords drown everything and the drums are nearly gone.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            recipe(
                "Bring the drums up: drag the Drums fader until its readout says about \u{2212}6 dB.",
                "The kick and clap are the song's pulse, and everything else sits around them. Mixes usually start from the drums.",
                "The fader is the slider at the right of the Drums track. Anywhere from \u{2212}9 to \u{2212}3 dB is fine.",
                |s| gain_between(s, "Drums", -9.0, -3.0),
                |s| track_named(s, "Drums").map(|t| Target::Fader(t.id)),
            ),
            recipe(
                "Now pull the Chords down to about \u{2212}12 dB.",
                "Pads fill space, and a little goes a long way. Turning things down rather than up keeps the mix from getting too loud.",
                "The Chords fader. Anywhere from \u{2212}18 to \u{2212}9 dB is fine.",
                |s| gain_between(s, "Chords", -18.0, -9.0),
                |s| track_named(s, "Chords").map(|t| Target::Fader(t.id)),
            ),
            info(
                "Getting the levels right is most of a mix. Keep an eye on Out at the top right: the loudest moments should stay \
                 below 0 dB. Set levels at a quiet volume, then turn your speakers up, not the faders.",
            ),
        ],
    },
    Lesson {
        id: MIX_EQ,
        group: MIXING,
        title: "EQ: making room",
        steps: &[
            act(
                "Press Space: the bass and the chords blur together down low. That's what people call muddy.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            recipe(
                "Open the Spectrum (top right) to see the sound, from low pitches on the left to high on the right.",
                "Everything playing, as a picture. The tall shapes on the left are the kick and the bass.",
                "The Spectrum button is left of CPU.",
                |s| s.analyzer_open,
                |_| Some(Target::Spectrum),
            ),
            recipe(
                "Solo the Chords: click S on the Chords track.",
                "Now you hear, and see, only the chords. The hump at the far left is low end that the chords don't need.",
                "S is under the track's name.",
                |s| track_named(s, "Chords").is_some_and(|t| t.solo),
                |s| track_named(s, "Chords").map(|t| Target::Solo(t.id)),
            ),
            act(
                "Add an EQ to the Chords: click the Chords track's name, then + EQ below.",
                "+ EQ is in the row above the instrument, once the Chords track is selected.",
                |s| eq_on(s, "Chords").is_some(),
                |s| if is_selected(s, track_named(s, "Chords")) { Some(Target::AddEq) } else { track_named(s, "Chords").map(|t| Target::Lane(t.id)) },
            ),
            recipe(
                "Switch on Low cut and turn its knob to about 250 Hz.",
                "Below about 250 Hz is the bass's space. The chords lose nothing you'd miss, and the hump in the spectrum is gone.",
                "If the EQ isn't showing, click EQ in the row above. Anywhere from 180 to 400 Hz is fine.",
                |s| eq_on(s, "Chords").is_some_and(|e| e.bands[EQ_LOW_CUT].on && (180.0..=400.0).contains(&e.bands[EQ_LOW_CUT].freq_hz)),
                |s| match eq_on(s, "Chords") {
                    Some(e) if !e.bands[EQ_LOW_CUT].on => Some(Target::EqBand(EQ_LOW_CUT)),
                    _ => Some(Target::EffectKnob(EffectParam::EqLowCut)),
                },
            ),
            recipe(
                "Unsolo the Chords, and listen to the bass come through.",
                "That's what EQ does in a mix: take out what one part doesn't need, so another can be heard.",
                "Click S on the Chords track again.",
                |s| track_named(s, "Chords").is_some_and(|t| !t.solo),
                |s| track_named(s, "Chords").map(|t| Target::Solo(t.id)),
            ),
            info(
                "Cut before you boost: taking away what's in the way sounds cleaner than turning up what you want. \
                 A low cut on everything that isn't bass or kick is one of the most common moves in mixing.",
            ),
        ],
    },
    Lesson {
        id: MIX_COMPRESS,
        group: MIXING,
        title: "Compression",
        steps: &[
            act(
                "Press Space and listen to the drums: a bit thin, each hit a different size.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Add a compressor to the drums: click the Drums track's name, then + Compressor below.",
                "+ Compressor is in the row above the drum pads, once the Drums track is selected.",
                |s| compressor_on(s, "Drums").is_some(),
                |s| if is_selected(s, track_named(s, "Drums")) { Some(Target::AddCompressor) } else { track_named(s, "Drums").map(|t| Target::Lane(t.id)) },
            ),
            recipe(
                "Turn Threshold down to about \u{2212}25 dB.",
                "Anything louder than the threshold gets turned down. The lower it is, the more of the drums it catches.",
                "Anywhere below \u{2212}23 dB is fine.",
                |s| compressor_on(s, "Drums").is_some_and(|c| c.threshold_db <= -23.0),
                |_| Some(Target::EffectKnob(EffectParam::CompressorThreshold)),
            ),
            recipe(
                "Ratio to about 6:1.",
                "How hard it turns down: at 6:1, 6 dB over the threshold comes out as 1. Drums take a firm hand.",
                "Anywhere from 5:1 to 10:1 is fine.",
                |s| compressor_on(s, "Drums").is_some_and(|c| (5.0..=10.0).contains(&c.ratio)),
                |_| Some(Target::EffectKnob(EffectParam::CompressorRatio)),
            ),
            recipe(
                "Attack to about 20 ms.",
                "A slower attack lets the first click of each hit through before the compressor reacts. That's what gives drums punch.",
                "Anywhere from 15 to 40 ms is fine.",
                |s| compressor_on(s, "Drums").is_some_and(|c| (15.0..=40.0).contains(&c.attack_ms)),
                |_| Some(Target::EffectKnob(EffectParam::CompressorAttack)),
            ),
            recipe(
                "Makeup to about +4 dB.",
                "Compressing made the drums quieter. Makeup brings them back up, and now they're fuller and more even.",
                "Anywhere from +3 dB up is fine.",
                |s| compressor_on(s, "Drums").is_some_and(|c| c.makeup_db >= 3.0),
                |_| Some(Target::EffectKnob(EffectParam::CompressorMakeup)),
            ),
            recipe(
                "Now overdo it: Ratio right up (15:1 or more) and Threshold down to \u{2212}40 dB. Listen.",
                "Flat and lifeless, with no punch left: that's too much. When you compress, back off until you can only just hear it working.",
                "Both knobs, turned all the way.",
                |s| compressor_on(s, "Drums").is_some_and(|c| c.ratio >= 15.0 && c.threshold_db <= -38.0),
                |s| match compressor_on(s, "Drums") {
                    Some(c) if c.ratio < 15.0 => Some(Target::EffectKnob(EffectParam::CompressorRatio)),
                    _ => Some(Target::EffectKnob(EffectParam::CompressorThreshold)),
                },
            ),
            info(
                "Compression evens out loud and quiet moments. Gentle settings (2:1 to 4:1, a few dB turned down) glue a part \
                 together. Put Ratio and Threshold back to where they sounded good before you move on.",
            ),
        ],
    },
    Lesson {
        id: MIX_FINISH,
        group: MIXING,
        title: "Finishing a mix",
        steps: &[
            act(
                "Press Space and play the whole song. Watch Out at the top right as it goes.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            info(
                "Out should peak somewhere around \u{2212}6 to \u{2212}1 dB. If it hits the top, turn down the loudest tracks, \
                 not the whole song. Shor's master bus has a limiter that catches the odd peak, but it can't fix a mix that's too loud.",
            ),
            recipe(
                "Export it: open the menu under the project name (top left) and choose Export Audio...",
                "Your mix, as a WAV file you can play anywhere.",
                "Pick where to save it; the status bar says Exported when it's done.",
                |s| s.exported,
                |_| Some(Target::FileMenu),
            ),
            info(
                "Now listen to it on your phone, in earbuds, on a laptop. A mix that works on small speakers works anywhere, \
                 and each place shows you something different to fix.",
            ),
        ],
    },
    Lesson {
        id: LOFI_BEAT,
        group: PROJECTS,
        title: "Lo-fi 1: the beat",
        steps: &[
            info(
                "Lo-fi: slow, soft and a little dusty - music to study to. In four parts you'll build a lo-fi beat at \
                 80 BPM, then add a Bollywood melody on top. Part 1: the boom-bap drums.",
            ),
            act(
                "Add a drum track: \u{201c}+ Drums\u{201d} under the tracks.",
                "Below the track list, on the left of the timeline.",
                |s| tracks_with(s, Instrument::Drums).next().is_some(),
                |_| Some(Target::AddDrumTrack),
            ),
            act(
                "Double-click bar 1 of the Drums track to make a clip.",
                "Two quick clicks on the empty lane. The clip opens below.",
                |s| clips_on(s, Instrument::Drums).next().is_some(),
                |s| tracks_with(s, Instrument::Drums).next().map(|t| Target::Lane(t.id)),
            ),
            act(
                "The \u{201c}boom\u{201d}: Kick on beat 1, and again on the \u{201c}and\u{201d} of beat 3 (the 11th square).",
                "Each beat has four squares; the \u{201c}and\u{201d} of 3 is two squares after the 3 mark.",
                |s| drum_pattern_has(s, KICK, &[0, 2 * PPQ + PPQ / 2]),
                |s| row_or_clip(s, Instrument::Drums, KICK),
            ),
            act(
                "The \u{201c}bap\u{201d}: Snare on beats 2 and 4.",
                "The Snare row, under 2 and 4.",
                |s| drum_pattern_has(s, SNARE, &[PPQ, 3 * PPQ]),
                |s| row_or_clip(s, Instrument::Drums, SNARE),
            ),
            act(
                "Closed hats keep time - your pick: every other square (the eighths) is the classic. At least four hits.",
                "The Closed Hat row. Eighths: squares 1, 3, 5, 7...",
                |s| clips_on(s, Instrument::Drums).any(|c| pitch_count(c, CLOSED_HAT) >= 4),
                |s| row_or_clip(s, Instrument::Drums, CLOSED_HAT),
            ),
            act("Press Space: hear how much slower than house it is.", "Or the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "Your turn: a ghost note. Add a Snare on the very last square, or a Kick on a square of your own - the \
                 little extras that make a beat swing.",
                "Hip-hop drummers fill the gaps with quiet extra hits. One off the main beats makes the loop feel played by a person.",
                "Anything that isn't one of the kicks or snares above counts. Click it again to take it away.",
                lofi_has_extra,
                |s| row_or_clip(s, Instrument::Drums, SNARE),
            ),
            act(
                "Stretch the clip to bar 17: drag its right edge.",
                "Grab the very end of the clip in the timeline.",
                |s| clips_on(s, Instrument::Drums).any(|c| loops(c, 16)),
                |s| tracks_with(s, Instrument::Drums).next().map(|t| Target::Lane(t.id)),
            ),
            info("Boom on the kick, bap on the snare, hats ticking between - slow and laid back. Next: the keys."),
        ],
    },
    Lesson {
        id: LOFI_KEYS,
        group: PROJECTS,
        title: "Lo-fi 2: jazzy keys",
        steps: &[
            act(
                "Add a MIDI track for the keys: \u{201c}+ MIDI track\u{201d}.",
                "Below the track list.",
                |s| tracks_with(s, Instrument::Carve).next().is_some(),
                |_| Some(Target::AddMidiTrack),
            ),
            act(
                "Its sound: open the presets at the top of Carve and pick Lo-fi Keys (the recipe of that name builds it).",
                "Or step through with the \u{2039} \u{203a} arrows.",
                |s| is_selected(s, keys_track(s)) && s.synth.name == "Lo-fi Keys",
                |_| Some(Target::Preset("Lo-fi Keys")),
            ),
            act(
                "Double-click bar 1 of the keys track.",
                "On its empty lane.",
                |s| keys_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| c.start == 0)),
                |s| keys_track(s).map(|t| Target::Lane(t.id)),
            ),
            act(
                "Four chords, four bars: Pattern + until it says 4 bars.",
                "Above the grid.",
                |s| keys_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| c.content_len() == bars(4))),
                |_| Some(Target::PatternPlus),
            ),
            act(
                "Bar 1, F major 7: A, C, E and F, stacked on its first square.",
                "Four clicks in one column. The rows go A, B, C, D, E, F, G from the bottom.",
                |s| keys_chord(s, 0),
                |s| keys_chord_target(s, 0),
            ),
            act("Bar 2, E minor 7: B, D, E and G.", "Bar 2 starts at the 2 mark.", |s| keys_chord(s, 1), |s| keys_chord_target(s, 1)),
            act("Bar 3, D minor 7: A, C, D and F.", "Bar 3 starts at the 3 mark.", |s| keys_chord(s, 2), |s| keys_chord_target(s, 2)),
            act("Bar 4, A minor 7: A, C, E and G.", "Bar 4 starts at the 4 mark.", |s| keys_chord(s, 3), |s| keys_chord_target(s, 3)),
            act(
                "Stretch the keys clip to bar 17, level with the drums.",
                "Drag its right edge.",
                |s| keys_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| c.end() >= bars(16) && looping(c))),
                |s| keys_track(s).map(|t| Target::Lane(t.id)),
            ),
            act("Press Space.", "Or the play button.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "Your turn: a colour note. Lo-fi loves 9ths - a G on the F chord, an E on the D minor, a B on the A \
                 minor. Stack one on top and listen.",
                "A 9th is a scale note just past the chord. It adds a soft, unresolved glow - the jazz colour lo-fi borrows.",
                "Any note of the scale that isn't already in its bar's chord counts.",
                keys_have_colour,
                |s| keys_track(s).map(|t| row_or_lane(s, t, 71)).flatten(),
            ),
            info(
                "Four seventh chords sliding down the scale - F, E, D, A - each ringing into the next. \
                 Next: the bass under them.",
            ),
        ],
    },
    Lesson {
        id: LOFI_BASS,
        group: PROJECTS,
        title: "Lo-fi 3: the bass",
        steps: &[
            act(
                "One more MIDI track, for the bass.",
                "\u{201c}+ MIDI track\u{201d}.",
                |s| tracks_with(s, Instrument::Carve).count() >= 2,
                |_| Some(Target::AddMidiTrack),
            ),
            act(
                "Preset: Deep Bass.",
                "The preset name at the top of Carve.",
                |s| is_selected(s, lofi_bass_track(s)) && s.synth.name == "Deep Bass",
                |_| Some(Target::Preset("Deep Bass")),
            ),
            recipe(
                "Make it round: Drive down to 0 dB, and Amp Release up to about 700 ms.",
                "Lo-fi bass is warm, not gritty. Drive adds the edge, so it goes; a longer release lets each note bloom and fade like a plucked upright bass.",
                "Drive is in the Filter section (under 2 dB). Release is in the Amp envelope (400 to 1200 ms).",
                |s| is_selected(s, lofi_bass_track(s)) && carve(s).is_some_and(|p| p.filter.drive_db <= 2.0 && (400.0..=1200.0).contains(&p.amp_env.release_ms)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.filter.drive_db <= 2.0, Target::Knob(SynthParam::Drive)),
                        ((400.0..=1200.0).contains(&p.amp_env.release_ms), Target::Knob(SynthParam::AmpRelease)),
                    ])
                },
            ),
            act(
                "Double-click bar 1 of the bass track.",
                "On its empty lane.",
                |s| lofi_bass_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| c.start == 0)),
                |s| lofi_bass_track(s).map(|t| Target::Lane(t.id)),
            ),
            act(
                "Pattern + until it says 4 bars - one bar per chord.",
                "Above the grid.",
                |s| lofi_bass_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| c.content_len() == bars(4))),
                |_| Some(Target::PatternPlus),
            ),
            act(
                "Bar 1: F, on beat 1 and again on the \u{201c}and\u{201d} of 3 - right with the kicks.",
                "F is the sixth row up. The patch plays it an octave lower.",
                |s| bass_root(s, 0),
                |s| lofi_bass_track(s).and_then(|t| row_or_lane(s, t, LOFI_BASS_ROOTS[0])),
            ),
            act("Bar 2: E, in the same places.", "Bar 2 starts at the 2 mark.", |s| bass_root(s, 1), |s| lofi_bass_track(s).and_then(|t| row_or_lane(s, t, LOFI_BASS_ROOTS[1]))),
            act("Bar 3: D.", "Bar 3 starts at the 3 mark.", |s| bass_root(s, 2), |s| lofi_bass_track(s).and_then(|t| row_or_lane(s, t, LOFI_BASS_ROOTS[2]))),
            act("Bar 4: A, the bottom row.", "Bar 4 starts at the 4 mark.", |s| bass_root(s, 3), |s| lofi_bass_track(s).and_then(|t| row_or_lane(s, t, LOFI_BASS_ROOTS[3]))),
            act(
                "Stretch the bass clip to bar 17.",
                "Drag its right edge.",
                |s| lofi_bass_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| c.end() >= bars(16) && looping(c))),
                |s| lofi_bass_track(s).map(|t| Target::Lane(t.id)),
            ),
            act("Press Space: drums, keys and bass.", "Or the play button.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "Your turn: a passing note. On beat 4 of a bar, add a note that steps toward the next bar's root - in \
                 bar 3, a C walks down to the A.",
                "A note between two roots walks the bass from one chord to the next instead of jumping - the way a jazz bassist moves.",
                "Any note that isn't its bar's root counts.",
                bass_walks,
                |s| lofi_bass_track(s).and_then(|t| row_or_lane(s, t, 60)),
            ),
            info("The bass plays each chord's root with the kick, and walks between them. Last part: an intro, and the mix."),
        ],
    },
    Lesson {
        id: LOFI_FINISH,
        group: PROJECTS,
        title: "Lo-fi 4: intro and mix",
        steps: &[
            act(
                "An intro - the keys alone, then the beat. Click the ruler at bar 5 and press Ctrl+E (\u{2318}E on a Mac): every clip splits there.",
                "Click the glowing bar first - the split happens at the playhead.",
                |s| starts_at(s, "Drums", 4),
                |_| Some(Target::RulerBar(4)),
            ),
            act(
                "Click the Drums piece in bars 1 to 4 and press Delete.",
                "Just that first piece.",
                |s| silent_in(s, "Drums", 0, 4) && !silent_in(s, "Drums", 4, 16),
                |s| track_named(s, "Drums").map(|t| Target::Lane(t.id)),
            ),
            act(
                "Same for the Bass piece in bars 1 to 4.",
                "Click it, then Delete.",
                |s| silent_in(s, "Bass", 0, 4) && !silent_in(s, "Bass", 4, 16),
                |s| track_named(s, "Bass").map(|t| Target::Lane(t.id)),
            ),
            act(
                "Let the keys open up as the beat arrives. Click the Keys track's name, then right-click Carve's Cutoff knob and choose Automate.",
                "A lane appears under the Keys track.",
                |s| cutoff_lane(s, "Keys").is_some(),
                |s| match track_named(s, "Keys") {
                    Some(t) if s.selected_track == Some(t.id) => Some(Target::Knob(SynthParam::Cutoff)),
                    Some(t) => Some(Target::Lane(t.id)),
                    None => None,
                },
            ),
            act(
                "In the new lane, a point low at bar 1 and another high at bar 5 - like the music coming in from another room.",
                "Click in the lane to add a point; drag a point to move it.",
                |s| cutoff_lane(s, "Keys").is_some_and(|l| rises(l, 0, 4)),
                |s| track_named(s, "Keys").map(|t| Target::Automation(t.id)),
            ),
            act(
                "Lo-fi drums sit soft: bring the Drums fader down to about -8 dB.",
                "The fader is on the right of the track header.",
                |s| track_named(s, "Drums").is_some_and(|t| (-11.0..=-5.0).contains(&t.gain_db)),
                |_| None,
            ),
            act(
                "Listen from the top: press Home, then Space.",
                "Home jumps to the start.",
                |s| s.playing && s.playhead < bars(2),
                |_| Some(Target::Play),
            ),
            info(
                "Keys drifting in through a closed filter, then the beat: a lo-fi track. Save it (Ctrl+S, \u{2318}S on a \
                 Mac) and export it - or carry on to Bollywood lo-fi, which adds a melody from Indian film music.",
            ),
        ],
    },
    Lesson {
        id: BOLLY_MELODY,
        group: PROJECTS,
        title: "Bollywood lo-fi 1: the melody",
        steps: &[
            info(
                "Bollywood lo-fi: a slow, dusty beat under a melody from Indian film music. Your lo-fi track is the start. \
                 Its scale, A minor, matches Asavari - one of the ten parent scales (thaats) of Hindustani music.",
            ),
            act(
                "Add a MIDI track for the melody.",
                "\u{201c}+ MIDI track\u{201d}.",
                |s| tracks_with(s, Instrument::Carve).count() >= 3,
                |_| Some(Target::AddMidiTrack),
            ),
            act(
                "Preset: Indian Harp - its bright, ringing pluck is close to a santoor's.",
                "The preset name at the top of Carve.",
                |s| is_selected(s, melody_track(s)) && s.synth.name == "Indian Harp",
                |_| Some(Target::Preset("Indian Harp")),
            ),
            act(
                "The melody comes in with the beat: double-click bar 5 of the new track.",
                "Bar 5, where the drums start.",
                |s| melody_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| c.start == bars(4))),
                |s| melody_track(s).map(|t| Target::Lane(t.id)),
            ),
            act(
                "Pattern + until it says 4 bars.",
                "Above the grid.",
                |s| melody_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| c.content_len() == bars(4))),
                |_| Some(Target::PatternPlus),
            ),
            act(
                "Bars 1 and 2: E, D, C, A - then B, C, B, G. Space the notes out; the harp rings on by itself.",
                "Use the upper octave: E is the 12th row up. Bar 2 starts at the 2 mark.",
                |s| phrase_has(s, 0, &[76, 74, 72, 69]) && phrase_has(s, 1, &[71, 72, 67]),
                |s| melody_track(s).and_then(|t| row_or_lane(s, t, 76)),
            ),
            act(
                "Bars 3 and 4: A, C, D, F - then land on E, and let it ring.",
                "Bar 3 starts at the 3 mark; the E is on bar 4's first square.",
                |s| phrase_has(s, 2, &[69, 72, 74, 77]) && phrase_has(s, 3, &[76]),
                |s| melody_track(s).and_then(|t| row_or_lane(s, t, 69)),
            ),
            act(
                "Stretch the melody to bar 17.",
                "Drag its right edge.",
                |s| melody_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| c.end() >= bars(16) && looping(c))),
                |s| melody_track(s).map(|t| Target::Lane(t.id)),
            ),
            act("Press Space.", "Or the play button.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "Your turn: decorate a note the Indian way. One square before one of your notes, add the note just above it - a quick touch from above.",
                "Indian melodies rarely land on a note plainly. A grace note from above - a kan - gives the line its sung, curling feel.",
                "Click the row one above a note, one square to its left.",
                has_kan,
                |s| melody_track(s).and_then(|t| row_or_lane(s, t, 71)),
            ),
            info("A santoor-like line in Asavari over your lo-fi chords. Last part: the drone that makes it sound Indian."),
        ],
    },
    Lesson {
        id: BOLLY_DRONE,
        group: PROJECTS,
        title: "Bollywood lo-fi 2: the drone",
        steps: &[
            act(
                "Add a MIDI track for a tanpura.",
                "\u{201c}+ MIDI track\u{201d}.",
                |s| tracks_with(s, Instrument::Carve).count() >= 4,
                |_| Some(Target::AddMidiTrack),
            ),
            act(
                "Preset: Tanpura.",
                "The preset name at the top of Carve. (The Tanpura recipe explains how it's built.)",
                |s| is_selected(s, drone_track(s)) && s.synth.name == "Tanpura",
                |_| Some(Target::Preset("Tanpura")),
            ),
            act(
                "Double-click bar 1 of the new track - the drone starts before everything.",
                "On its empty lane.",
                |s| drone_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| c.start == 0)),
                |s| drone_track(s).map(|t| Target::Lane(t.id)),
            ),
            act(
                "The tanpura's cycle, one string per beat: E (Pa) on beat 1, A (Sa) on beats 2 and 3, and the low A - the bottom row - on beat 4.",
                "Sa is the home note, A here; Pa is the fifth above it, E.",
                tanpura_cycle,
                |s| drone_track(s).and_then(|t| row_or_lane(s, t, 64)),
            ),
            act(
                "Stretch the drone to bar 17.",
                "Drag its right edge.",
                |s| drone_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| c.end() >= bars(16) && looping(c))),
                |s| drone_track(s).map(|t| Target::Lane(t.id)),
            ),
            act(
                "Tuck it in: the drone's fader down to about -10 dB - felt more than heard.",
                "The fader is on the right of the track header.",
                |s| drone_track(s).is_some_and(|t| (-14.0..=-7.0).contains(&t.gain_db)),
                |_| None,
            ),
            act(
                "Listen from the top: press Home, then Space.",
                "Home jumps to the start.",
                |s| s.playing && s.playhead < bars(2),
                |_| Some(Target::Play),
            ),
            info(
                "Beat, keys, bass, a santoor-like melody with its kan, and a tanpura holding Sa underneath: Bollywood \
                 lo-fi, made by you. Save it and export it to play anywhere.",
            ),
        ],
    },
    Lesson {
        id: MATCH_WAVE,
        group: SOUND_MATCH,
        title: "Which wave?",
        steps: &[
            info(
                "Sound match: a hidden sound - the Target - to rebuild in Carve. The graphs show both: the filled \
                 shape is the Target, the line is yours. Hear each with \u{25b8} Target and \u{25b8} Yours.",
            ),
            info(
                "Reading the spectrum: a note isn't one frequency but a stack of them, its harmonics - each one a \
                 peak, from low pitch on the left to high on the right. Yours is a saw: every harmonic, each a \
                 little quieter than the last. Now look at the Target's peaks.",
            ),
            recipe(
                "Match it: pick the wave whose peaks look like the Target's. 92% wins.",
                "A square wave has only the odd harmonics (1st, 3rd, 5th...), so every other peak is missing. Those gaps are its hollow, woody sound.",
                "Try each wave shape above Oscillator 1 and watch the peaks.",
                |s| s.match_score >= WIN,
                |_| Some(Target::OscWave(1)),
            ),
            info(
                "Gaps between the peaks: a square (or a hollow sound, like a clarinet). Every peak: a saw, bright \
                 and buzzy. A single peak: a sine, pure.",
            ),
        ],
    },
    Lesson {
        id: MATCH_CUTOFF,
        group: SOUND_MATCH,
        title: "Where's the cutoff?",
        steps: &[
            info(
                "This Target is a saw too - but look where its peaks drop away. Above a point the harmonics fall \
                 fast: that point is a filter's cutoff.",
            ),
            recipe(
                "Match it: turn Cutoff until your peaks fall away where the Target's do.",
                "A low-pass filter removes what's above its cutoff, so the high harmonics - the brightness - go. The lower the cutoff, the darker and rounder the sound.",
                "Hear both: the Target is darker. Watch where the two shapes part.",
                |s| s.match_score >= WIN,
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            info("Reading a sound's brightness off its spectrum is how producers match a sound they've heard."),
        ],
    },
    Lesson {
        id: MATCH_RESONANCE,
        group: SOUND_MATCH,
        title: "A bump at the edge",
        steps: &[
            info(
                "This Target has a bump: a few peaks louder than the rest, just before the drop. That's resonance - \
                 the filter ringing at its cutoff.",
            ),
            recipe(
                "Match it: set Cutoff where the bump is, then raise Resonance until your bump matches.",
                "Resonance feeds the filter back into itself, boosting the harmonics right at the cutoff - the squelchy, vocal sound of acid basslines.",
                "The bump is a little above 1 kHz.",
                |s| s.match_score >= WIN,
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((900.0..=1600.0).contains(&p.filter.cutoff_hz), Target::Knob(SynthParam::Cutoff)),
                        (true, Target::Knob(SynthParam::Resonance)),
                    ])
                },
            ),
            info("A bump at the edge of the drop: resonance. The taller the bump, the more the filter sings."),
        ],
    },
    Lesson {
        id: MATCH_SUB,
        group: SOUND_MATCH,
        title: "Something underneath",
        steps: &[
            info(
                "Look at the far left: the Target has a peak below your lowest one. Something is playing an octave \
                 under the note.",
            ),
            recipe(
                "Match it: bring in the Sub oscillator until your low peak is as tall as the Target's.",
                "The sub oscillator plays a pure tone an octave down. It adds weight you feel more than hear - why bass sounds use it.",
                "Sub, in the Mixer.",
                |s| s.match_score >= WIN,
                |_| Some(Target::Knob(SynthParam::SubLevel)),
            ),
            info("A peak below the note: a sub. Bass sounds live there."),
        ],
    },
    Lesson {
        id: MATCH_PLUCK,
        group: SOUND_MATCH,
        title: "Pluck",
        steps: &[
            info(
                "Now the right-hand graph: loudness over time. Yours holds steady while the note is held. The \
                 Target starts loud and dies away, like a plucked string.",
            ),
            recipe(
                "Match it: Amp Sustain down to zero, then set Decay so your fade matches the Target's.",
                "With no sustain a note fades to silence however long it's held; decay sets how fast. Short decays pluck, long ones ring.",
                "Sustain and Decay, in the Amp envelope. The Target fades in about a quarter of a second.",
                |s| s.match_score >= WIN,
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[(p.amp_env.sustain <= 0.05, Target::Knob(SynthParam::AmpSustain)), (false, Target::Knob(SynthParam::AmpDecay))])
                },
            ),
            info("A loud start and a quick fade: a pluck. The spectrum can't show that - the loudness curve can."),
        ],
    },
    Lesson {
        id: MATCH_SWELL,
        group: SOUND_MATCH,
        title: "Swell",
        steps: &[
            info("This Target's loudness curve starts at nothing and rises: it fades in."),
            recipe(
                "Match it: raise Amp Attack until your curve rises as slowly as the Target's.",
                "Attack is how long a note takes to reach full volume. A long attack makes pads and strings swell in instead of starting with a hit.",
                "About half a second.",
                |s| s.match_score >= WIN,
                |_| Some(Target::Knob(SynthParam::AmpAttack)),
            ),
            info("A slow rise at the start: a long attack. Pads, strings and bowed sounds are built on it."),
        ],
    },
    Lesson {
        id: MATCH_MYSTERY,
        group: SOUND_MATCH,
        title: "Mystery sound",
        steps: &[
            info(
                "No clues this time: the Target differs in its wave, its brightness and its shape over time. Use \
                 both graphs - and your ears.",
            ),
            recipe(
                "Match it. 92% wins.",
                "A hollow wave (gaps between the peaks), a filter taking the top off (where they fall away), and a fade to a lower level (the loudness curve's drop) - all three read off the graphs.",
                "Check the gaps between peaks, where the peaks fall away, and the drop after the start of the loudness curve.",
                |s| s.match_score >= WIN,
                |_| None,
            ),
            info(
                "You rebuilt a sound by reading it. That's what producers do with a reference: listen, look, then \
                 turn the knobs.",
            ),
        ],
    },
];

/// The "and" of every beat.
const OFFBEATS: [Ticks; 4] = [PPQ / 2, PPQ + PPQ / 2, 2 * PPQ + PPQ / 2, 3 * PPQ + PPQ / 2];
/// Where the arrangement's markers go (0-based bars).
pub(super) const MARKER_BARS: [i64; 4] = [0, 8, 16, 24];


/// The suggested order for someone new: a beat first, then the notes on
/// top of it, a first look at sound, then a whole track; the rest after.
pub const PATH: &[&str] = &[
    FIRST_BEAT, BASSLINE, CHORDS, ROLL_DYNAMICS, ROLL_LENGTH, THEORY_OCTAVES, THEORY_SCALES, THEORY_KEYS, THEORY_MAJOR_MINOR, THEORY_INTERVALS,
    THEORY_TRIADS, THEORY_PROGRESSIONS, THEORY_MELODY, THEORY_SEVENTHS, THEORY_RAAG, MELODY_STEPS, MELODY_CALL,
    MELODY_MOTIF, CARVE_WAVES, CARVE_FILTER, CARVE_ENVELOPES, RECIPE_BASS, RECIPE_PAD, PROJECT_GROOVE,
    PROJECT_BASS, PROJECT_CHORDS, PROJECT_ARRANGE, PROJECT_FINISH, MIX_LEVELS, MIX_EQ, MIX_COMPRESS, MIX_FINISH,
    ARRANGE_HOUSE, RECIPE_KEYS, LOFI_BEAT, LOFI_KEYS,
    LOFI_BASS, LOFI_FINISH, BOLLY_MELODY, BOLLY_DRONE,
];

/// The lesson to take next: the first unfinished one on the path, then
/// any other unfinished one. `None` once everything's done.
pub fn next_lesson(done: &[String]) -> Option<usize> {
    let open = |id: &str| !done.iter().any(|d| d == id);
    let id = PATH.iter().copied().find(|id| open(id)).or_else(|| LESSONS.iter().map(|l| l.id).find(|id| open(id)))?;
    LESSONS.iter().position(|l| l.id == id)
}

/// Music words the steps use, in plain language - shown under a step the
/// first time its lesson uses one. Matched as whole words (a trailing "s"
/// too), ignoring case.
pub const GLOSSARY: &[(&str, &str)] = &[
    ("off-beat", "halfway between two beats: the \u{201c}and\u{201d} when you count 1-and-2-and"),
    ("downbeat", "the first beat of a bar"),
    ("clip", "a block on the timeline holding notes (or recorded sound)"),
    ("pattern", "the notes inside a clip that repeat as it loops"),
    ("kick", "the deep drum, the heartbeat"),
    ("clap", "the sharp hit that answers the kick"),
    ("snare", "a sharp drum, like the clap but with a rattle"),
    ("hat", "hi-hat, the ticking cymbal: closed is short, open rings"),
    ("tempo", "speed, in beats per minute (BPM)"),
    ("chord", "three or more notes played together"),
    ("root", "the note a chord is named after: A in A minor"),
    ("minor", "a darker, sadder-sounding chord or scale"),
    ("major", "a brighter, happier-sounding chord or scale"),
    ("suspended", "a chord without its middle note, so it floats"),
    ("octave", "the same note, higher or lower (12 semitones apart)"),
    ("semitone", "the smallest step between two notes: one row of the grid, one piano key to the next"),
    ("interval", "the distance between two notes"),
    ("scale", "the set of notes a tune uses, from a pattern of whole and half steps"),
    ("triad", "a three-note chord: a note, the third above it and the fifth"),
    ("third", "3 or 4 semitones up: the interval that makes a chord major or minor"),
    ("fifth", "7 semitones up: open and strong"),
    ("diminished", "a tense chord built from two minor thirds"),
    ("progression", "a series of chords, usually looping"),
    ("velocity", "how hard a note is played: the height of its stem under the grid"),
    ("motif", "a short musical idea that a melody repeats and changes"),
    ("sequence", "the same idea played again, starting on a different note"),
    ("phrase", "a short musical sentence, with a start and an end"),
    ("groove", "the feel of a rhythm: which hits lean in and which sit back"),
    ("snap", "the grid clicks land on - it also sets how long a new note is"),
    ("mix", "balancing the parts of a song so each one can be heard"),
    ("eq", "turns parts of a sound's range - its lows, middle or highs - up or down"),
    ("spectrum", "a picture of a sound, from low pitches to high"),
    ("compressor", "turns the loud moments of a part down, so it sounds more even"),
    ("threshold", "the level above which a compressor starts turning down"),
    ("ratio", "how hard a compressor turns down: at 4:1, 4 dB too loud comes out as 1"),
    ("makeup", "volume added after compressing, to bring the part back up"),
    ("export", "save the song as an audio file you can play anywhere"),
    ("resolve", "move from a tense chord or note to a restful one"),
    ("sargam", "the Indian note names: Sa Re Ga ma Pa Dha Ni"),
    ("komal", "lowered, in Indian music: re, ga, dha and ni are the komal notes"),
    ("aroha", "a raag's way up; avaroha is its way down"),
    ("oscillator", "the part of a synth that makes the raw tone"),
    ("sync", "oscillator 2 restarts whenever oscillator 1 does, for a hard, tearing tone"),
    ("fm", "one oscillator bending another's pitch very fast, which makes new, metallic overtones"),
    ("wave", "the shape of a tone: sine is pure, saw is buzzy"),
    ("harmonic", "the quieter, higher tones inside every note: more of them sounds brighter"),
    ("filter", "takes some of a sound's brightness away"),
    ("cutoff", "where the filter starts cutting: lower is darker"),
    ("resonance", "a ring at the cutoff that makes the filter sing"),
    ("envelope", "how a sound changes during one note: its start, fade and end"),
    ("attack", "how long a note takes to reach full volume"),
    ("decay", "how quickly a note falls after it starts"),
    ("sustain", "the level a note holds while the key is down"),
    ("release", "how long a note rings after the key lets go"),
    ("drive", "pushes the sound harder until it turns gritty"),
    ("sub", "an extra, deeper copy of each note, an octave down"),
    ("noise", "a hiss, like breath or wind"),
    ("lfo", "a slow, automatic wobble that turns a knob for you"),
    ("vibrato", "a quick, gentle wobble in pitch, like a singer's"),
    ("unison", "several slightly out-of-tune copies of each note, for a wide sound"),
    ("detune", "putting copies slightly out of tune with each other, for width"),
    ("glide", "sliding from one note to the next instead of jumping"),
    ("mono", "one note at a time, like a voice"),
    ("chorus", "a shimmer from slightly delayed copies of the sound"),
    ("reverb", "the echo of a room or a hall"),
    ("drone", "one note held under everything"),
    ("raag", "a set of notes and rules for a melody, from Indian classical music"),
    ("fader", "the slider that sets a track's volume"),
    ("db", "decibels, how loud: -6 dB is about half as loud"),
    ("mute", "silences a track"),
    ("solo", "plays only this track"),
    ("marker", "a named flag on the ruler, marking a section of the song"),
    ("breakdown", "a quieter section where the drums and bass drop out"),
    ("automate", "let a knob move by itself as the song plays"),
];

/// The glossary words step `step` of `lesson` uses that no earlier step
/// of that lesson did.
pub fn new_words(lesson: &Lesson, step: usize) -> Vec<(&'static str, &'static str)> {
    let words = |text: &str| -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !(c.is_alphanumeric() || c == '-'))
            .map(|w| w.trim_matches('-').to_string())
            .filter(|w| !w.is_empty())
            .collect()
    };
    let uses = |text: &str, term: &str| {
        let plural = format!("{term}s");
        words(text).iter().any(|w| w == term || *w == plural || (term == "automate" && w == "automation"))
    };
    let text_of = |s: &Step| format!("{} {}", s.text, s.why);
    GLOSSARY
        .iter()
        .filter(|(term, _)| uses(&text_of(&lesson.steps[step]), term))
        .filter(|(term, _)| !lesson.steps[..step].iter().any(|s| uses(&text_of(s), term)))
        .copied()
        .take(3)
        .collect()
}


/// The piano-roll row for `pitch` while `track`'s clip is open in the
/// editor, else `track`'s lane (double-clicking the clip opens it).
fn row_or_lane(s: &Snapshot, track: &shared::arrangement::Track, pitch: u8) -> Option<Target> {
    let open_here = s.open_clip.and_then(|id| s.arrangement.clip(id)).is_some_and(|c| c.track == track.id);
    Some(if open_here { Target::PianoRollRow(pitch) } else { Target::Lane(track.id) })
}

fn is_selected(s: &Snapshot, track: Option<&shared::arrangement::Track>) -> bool {
    track.is_some_and(|t| s.selected_track == Some(t.id))
}

/// A project part's own Carve track: `name` once it has it (a finished
/// earlier part), else the first Carve track that isn't one of `taken`.
fn part_track<'a>(s: &'a Snapshot, name: &str, taken: &[&str]) -> Option<&'a shared::arrangement::Track> {
    track_named(s, name).or_else(|| tracks_with(s, Instrument::Carve).find(|t| !taken.contains(&t.name.as_str())))
}

fn keys_track(s: &Snapshot) -> Option<&shared::arrangement::Track> {
    part_track(s, "Keys", &[])
}

fn lofi_bass_track(s: &Snapshot) -> Option<&shared::arrangement::Track> {
    part_track(s, "Bass", &["Keys"])
}

fn melody_track(s: &Snapshot) -> Option<&shared::arrangement::Track> {
    part_track(s, "Melody", &["Keys", "Bass"])
}

fn drone_track(s: &Snapshot) -> Option<&shared::arrangement::Track> {
    part_track(s, "Tanpura", &["Keys", "Bass", "Melody"])
}

/// A minor (natural): A B C D E F G.
const A_MINOR: [u8; 7] = [9, 11, 0, 2, 4, 5, 7];

/// Bar `bar` of the keys pattern has all four notes of its chord (any octave).
fn keys_chord(s: &Snapshot, bar: usize) -> bool {
    keys_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| LOFI_CHORDS[bar].iter().all(|&p| notes_in_bar(c, bar).any(|n| same_class(n.pitch, p)))))
}

/// The row of the chord's first missing note (or the keys lane).
fn keys_chord_target(s: &Snapshot, bar: usize) -> Option<Target> {
    let t = keys_track(s)?;
    let clip = clips_of(s, t.id).next();
    let missing = LOFI_CHORDS[bar]
        .into_iter()
        .find(|&p| !clip.is_some_and(|c| notes_in_bar(c, bar).any(|n| same_class(n.pitch, p))))
        .unwrap_or(LOFI_CHORDS[bar][0]);
    row_or_lane(s, t, missing)
}

/// Somewhere in the keys, a scale note that isn't in its bar's chord.
fn keys_have_colour(s: &Snapshot) -> bool {
    keys_track(s).is_some_and(|t| {
        clips_of(s, t.id).any(|c| {
            (0..4).any(|bar| {
                notes_in_bar(c, bar).any(|n| {
                    A_MINOR.contains(&(n.pitch % 12)) && !LOFI_CHORDS[bar].iter().any(|&p| same_class(n.pitch, p))
                })
            })
        })
    })
}

/// Bar `bar` of the bass has its root at least once.
fn bass_root(s: &Snapshot, bar: usize) -> bool {
    lofi_bass_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| notes_in_bar(c, bar).any(|n| same_class(n.pitch, LOFI_BASS_ROOTS[bar]))))
}

/// A bass note that isn't its bar's root.
fn bass_walks(s: &Snapshot) -> bool {
    lofi_bass_track(s).is_some_and(|t| {
        clips_of(s, t.id).any(|c| (0..4).any(|bar| notes_in_bar(c, bar).any(|n| !same_class(n.pitch, LOFI_BASS_ROOTS[bar]))))
    })
}

/// A drum hit the lo-fi beat didn't ask for: a kick or snare off its
/// squares, or a clap or open hat anywhere.
fn lofi_has_extra(s: &Snapshot) -> bool {
    clips_on(s, Instrument::Drums).any(|c| {
        let ClipContent::Midi { notes, .. } = &c.content else { return false };
        notes.iter().any(|n| {
            let at = n.start % BAR;
            match n.pitch {
                KICK => at != 0 && at != 2 * PPQ + PPQ / 2,
                SNARE => at != PPQ && at != 3 * PPQ,
                CLOSED_HAT => false,
                _ => true,
            }
        })
    })
}

/// Bar `bar` of the melody has every one of `pitches`' note names.
fn phrase_has(s: &Snapshot, bar: usize, pitches: &[u8]) -> bool {
    melody_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| pitches.iter().all(|&p| notes_in_bar(c, bar).any(|n| same_class(n.pitch, p)))))
}

/// A grace note from above: a note one 16th before a lower one.
fn has_kan(s: &Snapshot) -> bool {
    melody_track(s).is_some_and(|t| {
        clips_of(s, t.id).any(|c| {
            let ClipContent::Midi { notes, .. } = &c.content else { return false };
            notes.iter().any(|a| notes.iter().any(|b| b.start == a.start + SIXTEENTH && a.pitch > b.pitch))
        })
    })
}

/// The drone's first bar has Pa (E) and Sa (A) at least twice.
fn tanpura_cycle(s: &Snapshot) -> bool {
    drone_track(s).is_some_and(|t| {
        clips_of(s, t.id).any(|c| {
            notes_in_bar(c, 0).any(|n| same_class(n.pitch, 64)) && notes_in_bar(c, 0).filter(|n| same_class(n.pitch, 57)).count() >= 2
        })
    })
}

/// The named track's Cutoff automation lane.
fn cutoff_lane<'a>(s: &'a Snapshot, name: &str) -> Option<&'a shared::arrangement::AutomationLane> {
    let t = track_named(s, name)?;
    s.arrangement
        .automation
        .iter()
        .find(|l| l.track == t.id && l.target == Some(shared::arrangement::AutomationTarget::Synth(SynthParam::Cutoff)))
}

/// An electric piano's envelope: a long fade to almost nothing.
fn struck(p: &SynthState) -> bool {
    (1000.0..=2500.0).contains(&p.amp_env.decay_ms) && p.amp_env.sustain <= 0.3 && (1000.0..=2500.0).contains(&p.amp_env.release_ms)
}

/// LFO 2 slowly and slightly on pitch: tape wow.
fn tape_wobble(p: &SynthState) -> bool {
    p.lfo2.target == LfoTarget::Pitch && (0.3..=1.0).contains(&lfo_rate_hz(p.lfo2.rate_norm)) && (0.04..=0.15).contains(&p.lfo2.depth)
}

fn clips_of(s: &Snapshot, track: shared::arrangement::TrackId) -> impl Iterator<Item = &Clip> {
    s.arrangement.clips.iter().filter(move |c| c.track == track)
}

fn looping(c: &Clip) -> bool {
    matches!(c.content, ClipContent::Midi { loop_len: Some(_), .. })
}

/// The project's bass track: "Bass" once it exists, else the (only) Carve track.
fn project_bass(s: &Snapshot) -> Option<&shared::arrangement::Track> {
    track_named(s, "Bass").or_else(|| tracks_with(s, Instrument::Carve).next())
}

/// The project's chords track: a Carve track that isn't the bass.
fn project_chord_track(s: &Snapshot) -> Option<&shared::arrangement::Track> {
    track_named(s, "Chords").or_else(|| tracks_with(s, Instrument::Carve).find(|t| t.name != "Bass"))
}

/// The pattern notes starting in bar `bar` (0-based) of `clip`.
fn notes_in_bar(clip: &Clip, bar: usize) -> impl Iterator<Item = &shared::arrangement::MidiNote> {
    let range = bars(bar as i64)..bars(bar as i64 + 1);
    let notes = match &clip.content {
        ClipContent::Midi { notes, .. } => notes.as_slice(),
        _ => &[],
    };
    notes.iter().filter(move |n| range.contains(&n.start))
}

/// Same note name, any octave.
fn same_class(a: u8, b: u8) -> bool {
    a % 12 == b % 12
}

/// Bar `bar` of the bass pattern has its root at least twice - any
/// rhythm, any octave.
fn bass_bar(s: &Snapshot, bar: usize) -> bool {
    let root = PROJECT_BASS_ROOTS[bar];
    project_bass(s).is_some_and(|b| clips_of(s, b.id).any(|c| notes_in_bar(c, bar).filter(|n| same_class(n.pitch, root)).count() >= 2))
}

/// Somewhere in the bass pattern, a note of its bar's chord other than
/// the root.
fn bass_moves(s: &Snapshot) -> bool {
    project_bass(s).is_some_and(|b| {
        clips_of(s, b.id).any(|c| {
            (0..4).any(|bar| {
                notes_in_bar(c, bar).any(|n| {
                    !same_class(n.pitch, PROJECT_BASS_ROOTS[bar])
                        && PROJECT_CHORDS_NOTES[bar].iter().any(|&p| same_class(n.pitch, p))
                })
            })
        })
    })
}

/// The chord's notes still missing from bar `bar`, in the order to click.
fn chord_missing(clip: &Clip, bar: usize) -> impl Iterator<Item = u8> + '_ {
    PROJECT_CHORDS_NOTES[bar].into_iter().filter(move |&p| !notes_in_bar(clip, bar).any(|n| same_class(n.pitch, p)))
}

/// Bar `bar` of the chords pattern has all three of its chord's notes -
/// stacked or spread, in any octave.
fn chord_bar(s: &Snapshot, bar: usize) -> bool {
    project_chord_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| chord_missing(c, bar).next().is_none()))
}

/// Some bar of the chords plays its chord's notes at two different times.
fn chords_have_rhythm(s: &Snapshot) -> bool {
    project_chord_track(s).is_some_and(|t| {
        clips_of(s, t.id).any(|c| {
            (0..4).any(|bar| {
                let mut starts =
                    notes_in_bar(c, bar).filter(|n| PROJECT_CHORDS_NOTES[bar].iter().any(|&p| same_class(n.pitch, p))).map(|n| n.start);
                starts.next().is_some_and(|first| starts.any(|t| t != first))
            })
        })
    })
}

/// The row of the first chord note still missing (or the clip's lane).
fn chord_target(s: &Snapshot, bar: usize) -> Option<Target> {
    let t = project_chord_track(s)?;
    if s.open_clip.is_none() {
        return Some(Target::Lane(t.id));
    }
    let clip = clips_of(s, t.id).next()?;
    Some(Target::PianoRollRow(chord_missing(clip, bar).next().unwrap_or(PROJECT_CHORDS_NOTES[bar][0])))
}

/// How many times `pitch` plays in `clip`'s pattern.
fn pitch_count(clip: &Clip, pitch: u8) -> usize {
    match &clip.content {
        ClipContent::Midi { notes, .. } => notes.iter().filter(|n| n.pitch == pitch).count(),
        _ => 0,
    }
}

/// A drum hit the groove lesson didn't ask for: a snare anywhere, or a
/// kick, clap or open hat off its taught squares (closed hats were a free
/// choice already).
fn groove_has_extra(s: &Snapshot) -> bool {
    let beats = [0, PPQ, 2 * PPQ, 3 * PPQ];
    clips_on(s, Instrument::Drums).any(|c| {
        let ClipContent::Midi { notes, .. } = &c.content else { return false };
        notes.iter().any(|n| {
            let at = n.start % BAR;
            match n.pitch {
                KICK => !beats.contains(&at),
                CLAP => at != PPQ && at != 3 * PPQ,
                OPEN_HAT => !OFFBEATS.contains(&at),
                CLOSED_HAT => false,
                _ => true,
            }
        })
    })
}

fn has_marker_at(s: &Snapshot, bar: i64) -> bool {
    s.arrangement.markers.iter().any(|m| m.position >= bars(bar) && m.position < bars(bar + 1))
}

/// The named track has a clip reaching bar `bar` (0-based end).
fn reaches(s: &Snapshot, name: &str, bar: i64) -> bool {
    track_named(s, name).is_some_and(|t| clips_of(s, t.id).any(|c| c.end() >= bars(bar)))
}

/// The named track has a clip starting exactly at bar `bar`.
fn starts_at(s: &Snapshot, name: &str, bar: i64) -> bool {
    track_named(s, name).is_some_and(|t| clips_of(s, t.id).any(|c| c.start == bars(bar)))
}

/// No note of the named track sounds anywhere in bars `from..to`.
fn silent_in(s: &Snapshot, name: &str, from: i64, to: i64) -> bool {
    let Some(t) = track_named(s, name) else { return false };
    !clips_of(s, t.id).any(|c| c.played_notes().iter().any(|n| (bars(from)..bars(to)).contains(&(c.start + n.start))))
}

fn chords_cutoff_lane(s: &Snapshot) -> Option<&shared::arrangement::AutomationLane> {
    let t = track_named(s, "Chords")?;
    s.arrangement
        .automation
        .iter()
        .find(|l| l.track == t.id && l.target == Some(shared::arrangement::AutomationTarget::Synth(SynthParam::Cutoff)))
}

/// The lane is clearly higher at bar `to` than at bar `from`.
fn rises(lane: &shared::arrangement::AutomationLane, from: i64, to: i64) -> bool {
    match (lane.value_at(bars(from)), lane.value_at(bars(to))) {
        (Some(a), Some(b)) => b - a > 0.25,
        _ => false,
    }
}

/// The on-screen Carve patch - only while a Carve track is selected (the
/// panel shows the selected track's patch).
fn carve(s: &Snapshot) -> Option<&SynthState> {
    selected(s).filter(|t| t.instrument == Some(Instrument::Carve)).map(|_| &s.synth)
}

/// The first control in a multi-knob step that isn't set yet.
fn first_unmet(controls: &[(bool, Target)]) -> Option<Target> {
    controls.iter().find(|(done, _)| !done).map(|&(_, t)| t)
}

/// LFO 2 on pitch at a vibrato rate and depth.
fn vibrato(p: &SynthState) -> bool {
    p.lfo2.target == LfoTarget::Pitch && (4.0..=7.0).contains(&lfo_rate_hz(p.lfo2.rate_norm)) && (0.1..=0.35).contains(&p.lfo2.depth)
}

/// `n` bars in ticks (4/4 throughout the course).
const fn bars(n: i64) -> Ticks {
    n * BAR
}

/// The playhead is somewhere in bars `from..to` (0-based).
fn in_bars(s: &Snapshot, from: i64, to: i64) -> bool {
    (bars(from)..bars(to)).contains(&s.playhead)
}

fn track_named<'a>(s: &'a Snapshot, name: &str) -> Option<&'a shared::arrangement::Track> {
    s.arrangement.tracks.iter().find(|t| t.name == name)
}

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

/// The theory lessons' Keys track's clip.
fn theory_clip(s: &Snapshot) -> Option<&Clip> {
    let track = track_named(s, "Keys")?;
    s.arrangement.clips.iter().find(|c| c.track == track.id)
}

fn theory_open(s: &Snapshot) -> bool {
    theory_clip(s).is_some_and(|c| s.open_clip == Some(c.id))
}

fn theory_lane(s: &Snapshot) -> Option<Target> {
    track_named(s, "Keys").map(|t| Target::Lane(t.id))
}

/// `pitch`'s row once the clip is open, else the lane to open it from.
fn theory_row(s: &Snapshot, pitch: u8) -> Option<Target> {
    if theory_open(s) { Some(Target::PianoRollRow(pitch)) } else { theory_lane(s) }
}

/// The clip has each (pitch, beat) - a note starting anywhere in that
/// beat (0-based, across bars), so a click in any of its squares counts.
fn theory_beats(s: &Snapshot, want: &[(u8, i64)]) -> bool {
    let Some(ClipContent::Midi { notes, .. }) = theory_clip(s).map(|c| &c.content) else { return false };
    want.iter().all(|&(pitch, beat)| notes.iter().any(|n| n.pitch == pitch && n.start / PPQ == beat))
}

/// Like `theory_beats`, at exact 16ths.
fn theory_beats_16(s: &Snapshot, want: &[(u8, i64)]) -> bool {
    theory_clip(s).is_some_and(|c| want.iter().all(|&(pitch, at)| has_notes(c, pitch, &[at * SIXTEENTH])))
}

fn theory_first_missing(s: &Snapshot, want: &[(u8, i64)]) -> Option<Target> {
    want.iter().find(|&&w| !theory_beats(s, &[w])).and_then(|&(p, _)| theory_row(s, p))
}

/// A tune's notes, each moved by `shift` semitones: all there / any there.
fn theory_pattern_has_all(s: &Snapshot, tune: &[(i64, u8, i64)], shift: i32) -> bool {
    theory_clip(s).is_some_and(|c| tune.iter().all(|&(at, p, _)| has_notes(c, (p as i32 + shift) as u8, &[at * SIXTEENTH])))
}

fn theory_pattern_has_any(s: &Snapshot, tune: &[(i64, u8, i64)], shift: i32) -> bool {
    theory_clip(s).is_some_and(|c| tune.iter().any(|&(at, p, _)| has_notes(c, (p as i32 + shift) as u8, &[at * SIXTEENTH])))
}

/// Every one of `pitches` on `beat`.
fn theory_chord(s: &Snapshot, pitches: &[u8], beat: i64) -> bool {
    pitches.iter().all(|&p| theory_beats(s, &[(p, beat)]))
}

/// A clip on the track called `name`.
fn named_clip<'a>(s: &'a Snapshot, name: &str) -> Option<&'a Clip> {
    let track = track_named(s, name)?;
    s.arrangement.clips.iter().find(|c| c.track == track.id)
}

fn track_open(s: &Snapshot, name: &str) -> bool {
    named_clip(s, name).is_some_and(|c| s.open_clip == Some(c.id))
}

/// Every one of `pitches` on `beat` (0-based, across bars) of `name`'s clip.
fn track_chord(s: &Snapshot, name: &str, pitches: &[u8], beat: i64) -> bool {
    let Some(ClipContent::Midi { notes, .. }) = named_clip(s, name).map(|c| &c.content) else { return false };
    pitches.iter().all(|&p| notes.iter().any(|n| n.pitch == p && n.start / PPQ == beat))
}

/// The melody lesson: a note of bar `bar`'s chord (any octave) on its beat 1.
fn melody_on_chord(s: &Snapshot, bar: usize) -> bool {
    let Some(ClipContent::Midi { notes, .. }) = named_clip(s, "Melody").map(|c| &c.content) else { return false };
    let beat = bar as i64 * 4;
    notes.iter().any(|n| n.start / PPQ == beat && PROGRESSION[bar].iter().any(|&p| same_class(p, n.pitch)))
}

/// Some note starting on beat `beat` (0-3) of bar `bar` of the melody.
fn melody_has_beat(s: &Snapshot, bar: i64, beat: i64) -> bool {
    let Some(ClipContent::Midi { notes, .. }) = named_clip(s, "Melody").map(|c| &c.content) else { return false };
    notes.iter().any(|n| n.start / PPQ == bar * 4 + beat)
}

/// The melody lane until its clip is open; then nothing (any row will do).
fn melody_target(s: &Snapshot) -> Option<Target> {
    if track_open(s, "Melody") { None } else { track_named(s, "Melody").map(|t| Target::Lane(t.id)) }
}

/// The Melody clip's notes in bar `bar` (0-based), in order.
fn melody_bar(s: &Snapshot, bar: i64) -> Vec<shared::arrangement::MidiNote> {
    let Some(ClipContent::Midi { notes, .. }) = named_clip(s, "Melody").map(|c| &c.content) else { return Vec::new() };
    let mut notes: Vec<_> = notes.iter().filter(|n| n.start / BAR == bar).copied().collect();
    notes.sort_by_key(|n| (n.start, n.pitch));
    notes
}

/// Where in its bar each note starts - a bar's rhythm.
fn rhythm(notes: &[shared::arrangement::MidiNote], bar: i64) -> Vec<Ticks> {
    notes.iter().map(|n| n.start - bar * BAR).collect()
}

/// A note's step in C major (C4 = 28), or `None` off the scale.
fn c_major_step(pitch: u8) -> Option<i32> {
    let index = [0, 2, 4, 5, 7, 9, 11].iter().position(|&d| d == pitch % 12)?;
    Some((pitch / 12) as i32 * 7 + index as i32)
}

/// Bar `to` is bar `from`, same rhythm, every note one scale step higher.
fn sequenced(s: &Snapshot, from: i64, to: i64) -> bool {
    let (a, b) = (melody_bar(s, from), melody_bar(s, to));
    !a.is_empty()
        && a.len() == b.len()
        && rhythm(&a, from) == rhythm(&b, to)
        && a.iter().zip(&b).all(|(x, y)| match (c_major_step(x.pitch), c_major_step(y.pitch)) {
            (Some(x), Some(y)) => y == x + 1,
            _ => false,
        })
}

/// `pitch`'s row once the Melody clip is open, else its lane.
fn melody_row(s: &Snapshot, pitch: u8) -> Option<Target> {
    if track_open(s, "Melody") { Some(Target::PianoRollRow(pitch)) } else { track_named(s, "Melody").map(|t| Target::Lane(t.id)) }
}

fn drums_open(s: &Snapshot) -> bool {
    track_open(s, "Drums")
}

/// The dynamics lesson's closed hat at 16th `at`, and its velocity.
fn hat_velocity(s: &Snapshot, at: i64) -> Option<u8> {
    let ClipContent::Midi { notes, .. } = &named_clip(s, "Drums")?.content else { return None };
    notes.iter().find(|n| n.pitch == shared::drums::CLOSED_HAT && n.start == at * SIXTEENTH).map(|n| n.velocity)
}

/// The first of `at` (16ths) whose hat isn't `done` yet: its stem, or the
/// lane while the clip is closed.
fn velocity_target(s: &Snapshot, at: &[i64], done: fn(u8) -> bool) -> Option<Target> {
    if !drums_open(s) {
        return track_named(s, "Drums").map(|t| Target::Lane(t.id));
    }
    at.iter().find(|&&a| !hat_velocity(s, a).is_some_and(done)).map(|&a| Target::Velocity(a * SIXTEENTH))
}

fn keys_notes(s: &Snapshot) -> Vec<shared::arrangement::MidiNote> {
    match named_clip(s, "Keys").map(|c| &c.content) {
        Some(ClipContent::Midi { notes, .. }) => notes.clone(),
        _ => Vec::new(),
    }
}

/// A Keys note starting in beat `beat` that lasts at least a beat.
fn long_note_on(s: &Snapshot, beat: i64) -> bool {
    keys_notes(s).iter().any(|n| n.start / PPQ == beat && n.length >= PPQ)
}

/// A Keys note starting in beat `beat` no longer than a 16th.
fn short_note_on(s: &Snapshot, beat: i64) -> bool {
    keys_notes(s).iter().any(|n| n.start / PPQ == beat && n.length <= SIXTEENTH)
}

/// `name`'s volume is within `lo..=hi` dB.
fn gain_between(s: &Snapshot, name: &str, lo: f32, hi: f32) -> bool {
    track_named(s, name).is_some_and(|t| (lo..=hi).contains(&t.gain_db))
}

/// The first effect on `name`'s chain that `pick` matches.
fn effect_on<T>(s: &Snapshot, name: &str, pick: impl Fn(&shared::arrangement::Effect) -> Option<T>) -> Option<T> {
    let track = track_named(s, name)?;
    s.arrangement.fx(Some(track.id))?.nodes.iter().find_map(|n| pick(&n.effect))
}

fn eq_on(s: &Snapshot, name: &str) -> Option<shared::arrangement::EqState> {
    effect_on(s, name, |e| match e {
        shared::arrangement::Effect::Eq(eq) => Some(*eq),
        _ => None,
    })
}

fn compressor_on(s: &Snapshot, name: &str) -> Option<shared::arrangement::CompressorState> {
    effect_on(s, name, |e| match e {
        shared::arrangement::Effect::Compressor(c) => Some(*c),
        _ => None,
    })
}

/// A scale preset's mask, by name.
fn scale_mask(name: &str) -> u16 {
    shared::theory::SCALE_PRESETS.iter().find(|p| p.name == name).map(|p| p.mask).unwrap_or(0)
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
    use crate::lessons::show::{add_midi_track, add_notes, add_track, draw_clip_at, pattern_bars};
    use shared::lessons::starting_project;

    /// The app state a lesson starts in.
    fn start(id: &str) -> Snapshot {
        // As `LessonEvent::Begin` leaves it: the last track selected, its
        // patch on screen, the lesson's key set.
        crate::lessons::preview::starting_snapshot(id).0
    }

    fn lesson(id: &str) -> &'static Lesson {
        LESSONS.iter().find(|l| l.id == id).unwrap()
    }


    #[test]
    fn every_recipe_step_explains_itself() {
        for l in LESSONS.iter().filter(|l| l.group == RECIPES) {
            for step in l.steps {
                if let Kind::Action { .. } = step.kind {
                    let plays = step.text.starts_with("Press Space");
                    assert!(plays || !step.why.is_empty(), "{}: no why for \"{}\"", l.id, step.text);
                }
            }
        }
    }

    #[test]
    fn carve_steps_need_the_carve_track_selected() {
        // With nothing selected the panel isn't Carve's, so no knob step
        // can pass, whatever the patch holds.
        let mut s = start(CARVE_FILTER);
        s.synth.filter.cutoff_hz = 100.0;
        s.selected_track = None;
        let Kind::Action { check, .. } = lesson(CARVE_FILTER).steps[1].kind else { panic!() };
        assert!(!check(&s));
    }

    /// Every lesson done start to finish by "Show me": before each action
    /// step's entry runs, the step must not pass yet; after it, it must.
    #[test]
    fn show_me_does_every_step() {
        for l in LESSONS {
            let id = l.id;
            let mut s = start(id);
            let shows = crate::lessons::show::steps(id);
            // A Sound match's score, as the model measures it.
            let target = crate::lessons::sound_match::target(id).map(|t| crate::lessons::sound_match::measure(&t));
            let score = |s: &mut Snapshot| {
                if let Some(t) = &target {
                    s.match_score = shared::analysis::likeness(t, &crate::lessons::sound_match::measure(&s.synth));
                }
            };
            score(&mut s);
            let actions: Vec<&Step> = l.steps.iter().filter(|st| matches!(st.kind, Kind::Action { .. })).collect();
            assert_eq!(shows.len(), actions.len(), "{id}: one Show me per action step");
            for (i, (step, show)) in actions.iter().zip(&shows).enumerate() {
                let Kind::Action { check, target } = step.kind else { unreachable!() };
                assert!(!check(&s), "{id} action {} already passes: {}", i + 1, step.text);
                let _ = target(&s);
                show(&mut s);
                score(&mut s);
                assert!(check(&s), "{id} action {} doesn't pass after Show me: {}", i + 1, step.text);
            }
        }
    }

    #[test]
    fn each_recipe_preset_is_what_its_lesson_builds() {
        // Loading the preset should satisfy every knob step of its recipe.
        for (id, preset) in [
            (RECIPE_BASS, "Deep Bass"),
            (RECIPE_FLUTE, "Flute"),
            (RECIPE_HARP, "Indian Harp"),
            (RECIPE_TANPURA, "Tanpura"),
            (RECIPE_REED, "Reed"),
            (RECIPE_LEAD, "Lead"),
            (RECIPE_KEYS, "Lo-fi Keys"),
        ] {
            let build = shared::synth::PRESETS.iter().find(|p| p.0 == preset).unwrap_or_else(|| panic!("no preset {preset}")).1;
            let mut s = start(id);
            s.synth = build();
            assert_eq!(s.synth.name, preset);
            s.playing = true;
            for step in lesson(id).steps {
                if let Kind::Action { check, .. } = step.kind {
                    assert!(check(&s), "{preset} doesn't satisfy {id}: {}", step.text);
                }
            }
        }
    }


    #[test]
    fn project_parts_take_other_choices_too() {
        // Bass: a rhythm of its own, an octave up.
        let mut s = start(PROJECT_BASS);
        add_midi_track(&mut s, "MIDI 1");
        draw_clip_at(&mut s, 4);
        pattern_bars(&mut s, 4);
        add_notes(&mut s, PROJECT_BASS_ROOTS[0] + 12, &[0, 3 * PPQ]);
        assert!(bass_bar(&s, 0));
        assert!(!bass_bar(&s, 1));
        // A note outside the chord isn't the "your turn" swap.
        add_notes(&mut s, 62, &[PPQ]);
        assert!(!bass_moves(&s));
        // Chords: A minor spread across bar 1 as an arpeggio.
        let mut s = start(PROJECT_CHORDS);
        add_midi_track(&mut s, "MIDI 1");
        draw_clip_at(&mut s, 8);
        pattern_bars(&mut s, 4);
        for (i, p) in [69u8, 60, 64].into_iter().enumerate() {
            add_notes(&mut s, p, &[i as i64 * PPQ]);
        }
        assert!(chord_bar(&s, 0));
        assert!(!chord_bar(&s, 1));
        // Groove: hats on every square are a fine choice, and aren't an
        // "extra" hit.
        let mut s = start(PROJECT_GROOVE);
        add_track(&mut s, "Drums", Some(Instrument::Drums));
        draw_clip_at(&mut s, 0);
        add_notes(&mut s, CLOSED_HAT, &(0..16).map(|i| i * PPQ / 4).collect::<Vec<_>>());
        assert!(clips_on(&s, Instrument::Drums).any(|c| pitch_count(c, CLOSED_HAT) >= 4));
        assert!(!groove_has_extra(&s));
        add_notes(&mut s, KICK, &[3 * PPQ + 3 * PPQ / 4]);
        assert!(groove_has_extra(&s));
    }

    /// The theory lessons expect what the editor really writes: the
    /// Triad tool's chords in C major, and the down arrow taking E to E flat
    /// in C minor.
    #[test]
    fn theory_steps_match_the_editor() {
        use crate::piano_roll::state::ChordShape;
        let major = scale_mask("Major");
        for chord in [[60, 64, 67], [62, 65, 69], [64, 67, 71], [65, 69, 72], [67, 71, 74], [69, 72, 76], [71, 74, 77]] {
            assert_eq!(ChordShape::Triad.pitches(chord[0], 0, major), chord.to_vec());
        }
        for chord in PROGRESSION {
            assert_eq!(ChordShape::Triad.pitches(chord[0], 0, major), chord.to_vec());
        }
        for chord in [[50, 53, 57, 60], [55, 59, 62, 65], [48, 52, 55, 59]] {
            assert_eq!(ChordShape::Seventh.pitches(chord[0], 0, major), chord.to_vec());
        }
        assert_eq!(shared::theory::scale_step(64, 0, scale_mask("Natural minor"), -1), Some(63));
        // Bhairav: Re and Dha step down to their komal forms.
        let bhairav = scale_mask("Raga Bhairav");
        assert_eq!(shared::theory::scale_step(62, 0, bhairav, -1), Some(61));
        assert_eq!(shared::theory::scale_step(69, 0, bhairav, -1), Some(68));
        assert_eq!(scale_mask("Chromatic"), 0xfff);
        // Every quiz's answer is one of its buttons, and it has something to play.
        for l in LESSONS {
            for step in l.steps {
                if let Kind::Quiz { notes, options, answer } = step.kind {
                    assert!(answer < options.len() && !notes.is_empty(), "{}: {}", l.id, step.text);
                }
            }
        }
    }

    /// The melody checks turn down the near-misses a learner would make.
    #[test]
    fn melody_checks_reject_near_misses() {
        use crate::lessons::show::add_notes_on_track as add;
        let check = |id: &str, action: usize, s: &Snapshot| {
            let Kind::Action { check, .. } = lesson(id).steps.iter().filter(|st| matches!(st.kind, Kind::Action { .. })).nth(action).unwrap().kind
            else {
                unreachable!()
            };
            check(s)
        };
        // Steps: a bar with a skip (C to E is a 3rd) isn't all steps.
        let mut s = start(MELODY_STEPS);
        for (beat, p) in [60, 64, 62, 60].into_iter().enumerate() {
            add(&mut s, "Melody", p, beat as i64 * PPQ);
        }
        assert!(!check(MELODY_STEPS, 2, &s));
        // A leap to a note outside the G chord (A) doesn't count.
        let mut s = start(MELODY_STEPS);
        for (beat, p) in [64, 62, 60, 62].into_iter().enumerate() {
            add(&mut s, "Melody", p, beat as i64 * PPQ);
        }
        add(&mut s, "Melody", 69, BAR);
        add(&mut s, "Melody", 67, BAR + PPQ);
        assert!(!check(MELODY_STEPS, 3, &s));
        // Call: ending on C is an answer, not a question; filling beat 4 leaves no gap.
        let mut s = start(MELODY_CALL);
        add(&mut s, "Melody", 64, 0);
        add(&mut s, "Melody", 60, PPQ);
        assert!(!check(MELODY_CALL, 2, &s));
        let mut s = start(MELODY_CALL);
        for (beat, p) in [60, 64, 65, 67].into_iter().enumerate() {
            add(&mut s, "Melody", p, beat as i64 * PPQ);
        }
        assert!(!check(MELODY_CALL, 2, &s));
        // Answer: home note but a different rhythm isn't a reply.
        let mut s = start(MELODY_CALL);
        for (beat, p) in [60, 64, 67].into_iter().enumerate() {
            add(&mut s, "Melody", p, beat as i64 * PPQ);
        }
        add(&mut s, "Melody", 62, BAR);
        add(&mut s, "Melody", 60, BAR + 2 * PPQ);
        assert!(!check(MELODY_CALL, 3, &s));
        // Motif: moved up a 3rd (two steps) isn't the one-step sequence.
        let mut s = start(MELODY_MOTIF);
        for (beat, p) in [60, 62, 64].into_iter().enumerate() {
            add(&mut s, "Melody", p, beat as i64 * PPQ);
            add(&mut s, "Melody", p + 4, BAR + beat as i64 * PPQ);
        }
        assert!(!check(MELODY_MOTIF, 3, &s));
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

    #[test]
    fn the_path_starts_at_the_first_beat_and_moves_on() {
        assert_eq!(LESSONS[next_lesson(&[]).unwrap()].id, FIRST_BEAT);
        let done = vec![FIRST_BEAT.to_string()];
        assert_eq!(LESSONS[next_lesson(&done).unwrap()].id, BASSLINE);
        for id in PATH {
            assert!(LESSONS.iter().any(|l| l.id == *id), "{id} isn't a lesson");
        }
        let all: Vec<String> = LESSONS.iter().map(|l| l.id.to_string()).collect();
        assert_eq!(next_lesson(&all), None);
        // Off the path, the rest still come up.
        let path_done: Vec<String> = PATH.iter().map(|s| s.to_string()).collect();
        assert!(next_lesson(&path_done).is_some());
    }

    #[test]
    fn words_are_explained_once_per_lesson() {
        let bass = LESSONS.iter().find(|l| l.id == PROJECT_BASS).unwrap();
        let first = bass.steps.iter().position(|s| s.text.contains("off-beats")).unwrap();
        assert!(new_words(bass, first).iter().any(|(t, _)| *t == "off-beat"));
        let later = (first + 1..bass.steps.len()).find(|&i| bass.steps[i].text.contains("off-beat"));
        if let Some(later) = later {
            assert!(!new_words(bass, later).iter().any(|(t, _)| *t == "off-beat"));
        }
        // No false matches inside other words ("sub" in "subtle").
        let l = &LESSONS[0];
        for i in 0..l.steps.len() {
            for (term, _) in new_words(l, i) {
                assert!(format!("{} {}", l.steps[i].text, l.steps[i].why).to_lowercase().contains(term));
            }
        }
    }
}