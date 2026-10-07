//! Starting projects for the built-in lessons (the course itself - steps,
//! checks, text - lives in the UI crate, which owns the state it checks).
//! Each lesson opens a known project so its steps can check exact
//! things: lesson 1 starts blank, lesson 2 with lesson 1's finished beat,
//! lesson 3 with the beat and lesson 2's bassline.

use crate::arrangement::{
    empty_arrangement, Arrangement, AutomationLane, AutomationTarget, Breakpoint, Clip, ClipColor, ClipContent,
    EffectGraph, Instrument, MidiNote, TempoMap, Ticks, TimeSignature, Track, TrackId, TrackKind, DEFAULT_TRACK_HEIGHT,
    PPQ,
};
use crate::drums::{CLAP, CLOSED_HAT, KICK, OPEN_HAT, SNARE};
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
pub const CARVE_SYNC_FM: &str = "carve-sync-fm";
pub const RECIPE_BASS: &str = "recipe-bass";
pub const RECIPE_FLUTE: &str = "recipe-flute";
pub const RECIPE_HARP: &str = "recipe-harp";
pub const RECIPE_LEAD: &str = "recipe-lead";
/// A wobble bass: an LFO synced to the beat sweeping a resonant filter.
pub const RECIPE_WOBBLE: &str = "recipe-wobble";
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
/// The lo-fi recipe, and "Lo-fi beat" in four parts, then "Bollywood
/// lo-fi" in two more on top of it - one chain, each part starting from
/// the one before.
pub const RECIPE_KEYS: &str = "recipe-keys";
/// "Sound match": hear a hidden target, read its spectrum, rebuild it.
pub const MATCH_WAVE: &str = "match-wave";
pub const MATCH_CUTOFF: &str = "match-cutoff";
pub const MATCH_RESONANCE: &str = "match-resonance";
pub const MATCH_SUB: &str = "match-sub";
pub const MATCH_PLUCK: &str = "match-pluck";
pub const MATCH_SWELL: &str = "match-swell";
pub const MATCH_MYSTERY: &str = "match-mystery";
/// The note the Sound match challenges play and measure: A3, held.
pub const MATCH_NOTE: u8 = 57;
/// How long it's held, in 16ths.
pub const MATCH_NOTE_16THS: i64 = 12;
/// "Melody": shaping a line - steps and leaps, call and response, motifs.
/// All three write on a Melody track over the I-V-vi-IV chords, in C major.
pub const MELODY_STEPS: &str = "melody-steps";
pub const MELODY_CALL: &str = "melody-call";
pub const MELODY_MOTIF: &str = "melody-motif";
/// More melody: rhythm, shape, chord notes, hooks, then one of your own.
pub const MELODY_RHYTHM: &str = "melody-rhythm";
pub const MELODY_SHAPE: &str = "melody-shape";
pub const MELODY_CHORD_TONES: &str = "melody-chord-tones";
pub const MELODY_HOOK: &str = "melody-hook";
pub const MELODY_OWN: &str = "melody-own";
/// "Piano roll": the notes you write, played with feeling.
pub const ROLL_DYNAMICS: &str = "roll-dynamics";
pub const ROLL_LENGTH: &str = "roll-length";
/// Sound basics: what dB, Hz and harmonics are, by ear, before Carve.
pub const SOUND_LOUDNESS: &str = "sound-loudness";
pub const SOUND_PITCH: &str = "sound-pitch";
pub const SOUND_HARMONICS: &str = "sound-harmonics";
/// The pitch lesson's held note: A3, 220 Hz.
pub const SOUND_PITCH_NOTE: u8 = 57;
pub const ROLL_PAINT: &str = "roll-paint";
pub const ROLL_SWING: &str = "roll-swing";
pub const ROLL_ROLLS: &str = "roll-rolls";
/// The step lessons' tempos: a house-ish paint job, a laid-back shuffle,
/// and trap's double-time hats.
pub const PAINT_BPM: f64 = 120.0;
pub const SWING_BPM: f64 = 90.0;
pub const TRAP_BPM: f64 = 140.0;

pub const TRAP_DRUMS: &str = "trap-drums";
pub const TRAP_808: &str = "trap-808";
pub const TRAP_MELODY: &str = "trap-melody";
pub const TRAP_ARRANGE: &str = "trap-arrange";
const TRAP_PARTS: [&str; 4] = [TRAP_DRUMS, TRAP_808, TRAP_MELODY, TRAP_ARRANGE];
/// Half-time kicks: beat 1 and the "and" of 2 (16ths).
pub const TRAP_KICKS: [i64; 2] = [0, 6];
/// One snare a bar, on beat 3: half time.
pub const TRAP_SNARE: i64 = 8;
/// The 808 (16th, pitch, 16ths): with the kicks, then a long A with a C
/// starting inside it - so Mono glides up into the C.
pub const TRAP_808_NOTES: [(i64, u8, i64); 4] = [(0, 57, 6), (6, 57, 2), (8, 57, 4), (10, 60, 6)];
/// The flute's two bars in A minor: down from E to B, then an answer that
/// dips to G and comes home to A.
pub const TRAP_PHRASE: [(i64, u8, i64); 8] =
    [(0, 76, 4), (4, 74, 2), (6, 72, 2), (8, 71, 8), (16, 72, 4), (20, 71, 2), (22, 67, 2), (24, 69, 8)];
/// The arrangement's one-bar drop-out before the hook comes round again
/// (0-based bar).
pub const TRAP_GAP_BAR: i64 = 11;
/// The dynamics lesson's hats, all at full strength to start: every 8th
/// (16ths 0, 2 ... 14), plus two extra 16ths (7 and 15) to become ghost
/// notes.
pub const ROLL_HATS_8THS: [i64; 8] = [0, 2, 4, 6, 8, 10, 12, 14];
pub const ROLL_GHOSTS: [i64; 2] = [7, 15];
/// "Mixing": the finished house track, made to sound finished.
pub const MIX_LEVELS: &str = "mix-levels";
pub const MIX_EQ: &str = "mix-eq";
pub const MIX_COMPRESS: &str = "mix-compress";
pub const MIX_FINISH: &str = "mix-finish";
/// Where the levels lesson starts the drums (buried) and chords (too loud).
pub const MIX_BURIED_DRUMS_DB: f32 = -22.0;
pub const MIX_LOUD_CHORDS_DB: f32 = 0.0;
/// "Theory": the ideas behind the notes, heard first, then written.
pub const THEORY_OCTAVES: &str = "theory-octaves";
pub const THEORY_SCALES: &str = "theory-scales";
pub const THEORY_KEYS: &str = "theory-keys";
pub const THEORY_MAJOR_MINOR: &str = "theory-major-minor";
pub const THEORY_INTERVALS: &str = "theory-intervals";
pub const THEORY_TRIADS: &str = "theory-triads";
pub const THEORY_PROGRESSIONS: &str = "theory-progressions";
pub const THEORY_MELODY: &str = "theory-melody";
pub const THEORY_SEVENTHS: &str = "theory-sevenths";
pub const THEORY_RAAG: &str = "theory-raag";
/// Raag Bhairav's phrases: its pakad, where they rest, and building your own.
pub const THEORY_BHAIRAV: &str = "theory-bhairav";
/// I-V-vi-IV in C, one chord a bar, voiced close together: C (C4), G
/// (G3), Am (A3), F (F3) - what the progressions lesson writes, and what
/// the melody lesson starts with.
pub const PROGRESSION: [[u8; 3]; 4] = [[60, 64, 67], [55, 59, 62], [57, 60, 64], [53, 57, 60]];
/// The raag lesson's tanpura with Sa on C: Pa, Sa, Sa, low Sa.
pub const RAAG_TANPURA: [(i64, u8, i64); 4] = [(0, 55, 4), (4, 60, 4), (8, 60, 4), (12, 48, 4)];
/// The theory lessons' tempo: unhurried, so each note can be heard.
pub const THEORY_BPM: f64 = 100.0;
/// Twinkle Twinkle Little Star's opening, C C G G A A G - a tune nearly
/// everyone can hum, so "the same tune, higher" is plain by ear.
/// (16th, pitch, 16ths)
pub const OCTAVE_TUNE: [(i64, u8, i64); 7] = [(0, 60, 3), (4, 60, 3), (8, 67, 3), (12, 67, 3), (16, 69, 3), (20, 69, 3), (24, 67, 7)];
/// A two-bar tune in C major that walks home to C on its last note
/// (16th 28) - the lesson swaps that note for A.
pub const HOME_TUNE: [(i64, u8, i64); 11] = [
    (0, 64, 2), (2, 62, 2), (4, 60, 4), (8, 62, 2), (10, 64, 2), (12, 65, 4),
    (16, 67, 4), (20, 65, 2), (22, 64, 2), (24, 62, 4), (28, 60, 4),
];
/// A C major arpeggio: C E G E, C E G. Its E's (16ths 2, 6, 10) are what
/// the major/minor lesson lowers.
pub const MAJOR_TUNE: [(i64, u8, i64); 7] = [(0, 60, 2), (2, 64, 2), (4, 67, 2), (6, 64, 2), (8, 60, 2), (10, 64, 2), (12, 67, 4)];
/// The intervals lesson's reference clip: an octave, a fifth, a major
/// third and a minor third, each from C (the minor third from A).
pub const INTERVAL_EXAMPLES: [(i64, u8, i64); 8] =
    [(0, 60, 3), (4, 72, 3), (8, 60, 3), (12, 67, 3), (16, 60, 3), (20, 64, 3), (24, 57, 3), (28, 60, 3)];
pub const LOFI_BEAT: &str = "lofi-beat";
pub const LOFI_KEYS: &str = "lofi-keys";
pub const LOFI_BASS: &str = "lofi-bass";
pub const LOFI_FINISH: &str = "lofi-finish";
pub const BOLLY_MELODY: &str = "bolly-melody";
pub const BOLLY_DRONE: &str = "bolly-drone";

const HOUSE_PARTS: [&str; 5] = [PROJECT_GROOVE, PROJECT_BASS, PROJECT_CHORDS, PROJECT_ARRANGE, PROJECT_FINISH];
const LOFI_PARTS: [&str; 6] = [LOFI_BEAT, LOFI_KEYS, LOFI_BASS, LOFI_FINISH, BOLLY_MELODY, BOLLY_DRONE];

/// Lo-fi's tempo: slow enough to nod to.
pub const LOFI_BPM: f64 = 80.0;
/// Fmaj7, Em7, Dm7, Am7 - a slow slide down through A minor, voiced so
/// every note sits in the piano roll's first two octaves (A3 up).
pub const LOFI_CHORDS: [[u8; 4]; 4] = [[57, 60, 64, 65], [59, 62, 64, 67], [57, 60, 62, 65], [57, 60, 64, 67]];
/// The bass follows the chords' roots: F, E, D, A (played an octave down
/// by the patch).
pub const LOFI_BASS_ROOTS: [u8; 4] = [65, 64, 62, 57];
/// The Bollywood lo-fi phrase over those chords, as (16th, pitch, 16ths).
pub const BOLLY_PHRASE: [(i64, u8, i64); 13] = [
    (0, 76, 6), (6, 74, 2), (8, 72, 4), (12, 69, 4),
    (16, 71, 6), (22, 72, 2), (24, 71, 2), (26, 67, 6),
    (32, 69, 4), (36, 72, 4), (40, 74, 6), (46, 77, 2),
    (48, 76, 12),
];
/// The tanpura's cycle with Sa on A: Pa (E), Sa, Sa, low Sa.
pub const BOLLY_TANPURA: [(i64, u8, i64); 4] = [(0, 64, 4), (4, 69, 4), (8, 69, 4), (12, 57, 4)];

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
        osc2: Oscillator { waveform: Waveform::Saw, octave: 0, knob_a_cents: 0.0, knob_b: 0.0, knob_c: 0.0, sync: false },
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
        lfo1: Lfo { rate_label: "", rate_norm: 0.6, depth: 0.0, sync: false, target: LfoTarget::PulseWidth, target_count: 1, beat_sync: false },
        lfo2: Lfo { rate_label: "", rate_norm: 0.3, depth: 0.0, sync: false, target: LfoTarget::Pitch, target_count: 1, beat_sync: false },
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
        // A low note then a high one, held: sync and FM change the tone
        // while it rings, and key tracking is about the gap between them.
        CARVE_SYNC_FM => ("Low and high", steps(&[(0, 45, 7), (8, 69, 7)]), 1),
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
        // Stabs of Am7 then Fmaj7: short notes, so it's the envelope's
        // tail - not the note length - that makes them ring.
        RECIPE_KEYS => ("Chords", steps(&[
            (0, 57, 2), (0, 60, 2), (0, 64, 2), (0, 67, 2), (6, 57, 2), (6, 60, 2), (6, 64, 2), (6, 67, 2),
            (16, 57, 2), (16, 60, 2), (16, 64, 2), (16, 65, 2), (22, 57, 2), (22, 60, 2), (22, 64, 2), (22, 65, 2),
        ]), 2),
        // One held note, the one the challenge measures, and a gap to
        // hear its tail.
        _ if lesson.starts_with("match-") => ("Note", steps(&[(0, MATCH_NOTE, MATCH_NOTE_16THS)]), 1),
        RECIPE_BASS => ("Bassline", steps(&[(2, 45, 1), (6, 45, 1), (10, 45, 1), (14, 57, 1)]), 1),
        // Long notes: a wobble needs a held note to move.
        RECIPE_WOBBLE => ("Wobble line", steps(&[(0, 45, 7), (8, 45, 4), (12, 48, 4)]), 1),
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
    if lesson == RECIPE_BASS || lesson == RECIPE_LEAD || lesson == RECIPE_WOBBLE {
        let drums = add_track(&mut arr, "Drums", ClipColor::Coral, Instrument::Drums, -8.0);
        add_loop(&mut arr, drums, "Beat", lesson_one_beat(), 1, CARVE_BARS);
    }
    let synth = add_track(&mut arr, "Carve", ClipColor::Violet, Instrument::Carve, -4.0);
    let (name, notes, pattern_bars) = carve_riff(lesson);
    add_loop(&mut arr, synth, name, notes, pattern_bars, CARVE_BARS);
    Project { arrangement: arr, instruments: vec![(synth, init_patch())], synth: None }
}

/// A plain, clear keyboard sound for the theory lessons: a triangle with
/// a little saw, struck and fading like a piano - no wobble, no effects
/// that smear pitch, so every note is easy to place by ear.
pub fn theory_keys() -> SynthState {
    let mut p = init_patch();
    p.name = "Keys";
    p.osc1.waveform = Waveform::Triangle;
    p.osc2.waveform = Waveform::Saw;
    p.mix.osc2_db = -18.0;
    p.filter.cutoff_hz = 3500.0;
    p.filter.resonance = 0.0;
    p.amp_env = Envelope { attack_ms: 3.0, decay_ms: 700.0, sustain: 0.35, release_ms: 350.0 };
    p.fx.reverb_mix = 0.12;
    p.fx.reverb_size = 0.35;
    p.output.volume_db = -2.0;
    p
}

/// The theory lessons: one Keys track at 100 BPM, holding the lesson's
/// tune (or an empty clip to write into).
fn theory_lesson(lesson: &str) -> Project {
    let mut arr = empty_arrangement();
    arr.tempo_map = TempoMap::constant(THEORY_BPM, TimeSignature::FOUR_FOUR);
    let mut instruments = Vec::new();
    // Raag: the tanpura's drone comes first, the Keys to write on last.
    if lesson == THEORY_RAAG || lesson == THEORY_BHAIRAV {
        let drone = add_track(&mut arr, "Tanpura", ClipColor::Amber, Instrument::Carve, -10.0);
        add_loop(&mut arr, drone, "Drone", steps(&RAAG_TANPURA), 1, 16);
        instruments.push((drone, crate::synth::recipes::tanpura()));
    }
    let keys = add_track(&mut arr, "Keys", ClipColor::Teal, Instrument::Carve, -4.0);
    instruments.push((keys, theory_keys()));
    let (name, notes, pattern_bars, total_bars) = match lesson {
        THEORY_OCTAVES => ("Tune", steps(&OCTAVE_TUNE), 2, 4),
        THEORY_PROGRESSIONS => ("Chords", Vec::new(), 4, 4),
        THEORY_MELODY => {
            let chords = PROGRESSION
                .iter()
                .enumerate()
                .flat_map(|(bar, chord)| chord.iter().map(move |&p| (bar as i64 * 16, p, 15)))
                .collect::<Vec<_>>();
            ("Chords", steps(&chords), 4, 8)
        }
        THEORY_RAAG => ("Aroha", Vec::new(), 2, 2),
        THEORY_BHAIRAV => ("Phrases", Vec::new(), 4, 4),
        THEORY_KEYS => ("Tune", steps(&HOME_TUNE), 2, 4),
        THEORY_MAJOR_MINOR => ("Tune", steps(&MAJOR_TUNE), 1, 4),
        THEORY_INTERVALS => ("Examples", steps(&INTERVAL_EXAMPLES), 2, 2),
        // Written by the learner.
        _ => ("Notes", Vec::new(), 2, 2),
    };
    add_loop(&mut arr, keys, name, notes, pattern_bars, total_bars);
    // The melody lesson writes on a track of its own, over the chords.
    if lesson == THEORY_MELODY {
        let melody = add_track(&mut arr, "Melody", ClipColor::Violet, Instrument::Carve, -4.0);
        add_loop(&mut arr, melody, "Melody", Vec::new(), 4, 8);
        instruments.push((melody, theory_keys()));
    }
    Project { arrangement: arr, instruments, synth: None }
}

/// The step lessons: a beat to finish in the drum editor. Painting starts
/// from a kick and clap with no hats; swing from stiff, even 16th hats;
/// rolls from a half-time trap beat with plain eighth hats.
fn step_lesson(lesson: &str) -> Project {
    let mut arr = empty_arrangement();
    let hit = |at: i64, pitch: u8, velocity: u8| MidiNote { start: at * SIXTEENTH, length: SIXTEENTH, pitch, velocity };
    let (bpm, beat) = match lesson {
        ROLL_PAINT => {
            let mut beat: Vec<MidiNote> = (0..4).map(|b| hit(b * 4, KICK, 115)).collect();
            beat.extend([hit(4, CLAP, 105), hit(12, CLAP, 105)]);
            (PAINT_BPM, beat)
        }
        ROLL_SWING => {
            let mut beat = vec![hit(0, KICK, 120), hit(10, KICK, 105), hit(4, SNARE, 110), hit(12, SNARE, 110)];
            beat.extend((0..16).map(|at| hit(at, CLOSED_HAT, if at % 2 == 0 { 90 } else { 65 })));
            (SWING_BPM, beat)
        }
        _ => {
            let mut beat: Vec<MidiNote> = TRAP_KICKS.iter().map(|&at| hit(at, KICK, 120)).collect();
            beat.push(hit(TRAP_SNARE, SNARE, 115));
            beat.extend((0..8).map(|i| hit(i * 2, CLOSED_HAT, 85)));
            (TRAP_BPM, beat)
        }
    };
    arr.tempo_map = TempoMap::constant(bpm, TimeSignature::FOUR_FOUR);
    let drums = add_track(&mut arr, "Drums", ClipColor::Coral, Instrument::Drums, -6.0);
    add_loop(&mut arr, drums, "Beat", beat, 1, 8);
    Project { arrangement: arr, instruments: Vec::new(), synth: None }
}

/// The Sound basics lessons, at 100 BPM. Loudness: the Keys playing a
/// tune, to turn down. Pitch: a held A3 to write octaves and fifths over.
/// Harmonics: Carve's Init patch as a pure sine, holding one note, to
/// step through the waves.
fn sound_lesson(lesson: &str) -> Project {
    let mut arr = empty_arrangement();
    arr.tempo_map = TempoMap::constant(THEORY_BPM, TimeSignature::FOUR_FOUR);
    if lesson == SOUND_HARMONICS {
        let synth = add_track(&mut arr, "Carve", ClipColor::Violet, Instrument::Carve, -4.0);
        add_loop(&mut arr, synth, "Held note", steps(&[(0, SOUND_PITCH_NOTE, 14)]), 1, CARVE_BARS);
        let mut patch = init_patch();
        patch.osc1.waveform = Waveform::Sine;
        return Project { arrangement: arr, instruments: vec![(synth, patch)], synth: None };
    }
    let keys = add_track(&mut arr, "Keys", ClipColor::Teal, Instrument::Carve, 0.0);
    let (name, notes, pattern_bars) = if lesson == SOUND_LOUDNESS {
        ("Tune", steps(&HOME_TUNE), 2)
    } else {
        ("Notes", steps(&[(0, SOUND_PITCH_NOTE, 15)]), 1)
    };
    add_loop(&mut arr, keys, name, notes, pattern_bars, 8);
    Project { arrangement: arr, instruments: vec![(keys, theory_keys())], synth: None }
}

/// The piano roll lessons, at an easy 100 BPM: a beat whose hats all hit
/// equally hard (dynamics), or that beat under an empty Keys clip
/// (length and timing).
fn roll_lesson(lesson: &str) -> Project {
    if [ROLL_PAINT, ROLL_SWING, ROLL_ROLLS].contains(&lesson) {
        return step_lesson(lesson);
    }
    let mut arr = empty_arrangement();
    arr.tempo_map = TempoMap::constant(THEORY_BPM, TimeSignature::FOUR_FOUR);
    let hit = |at: i64, pitch: u8, velocity: u8| MidiNote { start: at * SIXTEENTH, length: SIXTEENTH, pitch, velocity };
    let mut beat: Vec<MidiNote> = Vec::new();
    for b in 0..4 {
        beat.push(hit(b * 4, KICK, 115));
        if b % 2 == 1 {
            beat.push(hit(b * 4, CLAP, 105));
        }
    }
    let drums = add_track(&mut arr, "Drums", ClipColor::Coral, Instrument::Drums, -6.0);
    let mut instruments = Vec::new();
    if lesson == ROLL_DYNAMICS {
        beat.extend(ROLL_HATS_8THS.iter().chain(&ROLL_GHOSTS).map(|&at| hit(at, CLOSED_HAT, 110)));
        add_loop(&mut arr, drums, "Beat", beat, 1, 8);
    } else {
        beat.extend(ROLL_HATS_8THS.iter().map(|&at| hit(at, CLOSED_HAT, if at % 4 == 0 { 100 } else { 70 })));
        add_loop(&mut arr, drums, "Beat", beat, 1, 8);
        let keys = add_track(&mut arr, "Keys", ClipColor::Teal, Instrument::Carve, -4.0);
        add_loop(&mut arr, keys, "Notes", Vec::new(), 1, 8);
        instruments.push((keys, theory_keys()));
    }
    Project { arrangement: arr, instruments, synth: None }
}

/// The mixing lessons: the finished house track (the Projects chain's
/// end), with the problem each lesson fixes put in.
fn mix_lesson(lesson: &str) -> Project {
    let mut project = project_after(5);
    let id = |p: &Project, name: &str| p.arrangement.tracks.iter().find(|t| t.name == name).map(|t| t.id);
    match lesson {
        MIX_LEVELS => {
            for (name, db) in [("Drums", MIX_BURIED_DRUMS_DB), ("Chords", MIX_LOUD_CHORDS_DB)] {
                if let Some(t) = id(&project, name).and_then(|t| project.arrangement.track_mut(t)) {
                    t.gain_db = db;
                }
            }
        }
        // The chords' sound has a sub under it: low end that crowds the bass.
        MIX_EQ => {
            if let Some(chords) = id(&project, "Chords") {
                if let Some((_, patch)) = project.instruments.iter_mut().find(|(t, _)| *t == chords) {
                    patch.mix.sub_db = -3.0;
                }
            }
        }
        _ => {}
    }
    project
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
        LOFI_BEAT => return lofi_after(0),
        LOFI_KEYS => return lofi_after(1),
        LOFI_BASS => return lofi_after(2),
        LOFI_FINISH => return lofi_after(3),
        BOLLY_MELODY => return lofi_after(4),
        BOLLY_DRONE => return lofi_after(5),
        TRAP_DRUMS => return trap_after(0),
        TRAP_808 => return trap_after(1),
        TRAP_MELODY => return trap_after(2),
        TRAP_ARRANGE => return trap_after(3),
        _ => {}
    }
    if lesson.starts_with("carve-") || lesson.starts_with("recipe-") || lesson.starts_with("match-") {
        return carve_lesson(lesson);
    }
    if lesson.starts_with("theory-") {
        return theory_lesson(lesson);
    }
    if lesson.starts_with("sound-") {
        return sound_lesson(lesson);
    }
    if lesson.starts_with("mix-") {
        return mix_lesson(lesson);
    }
    if lesson.starts_with("roll-") {
        return roll_lesson(lesson);
    }
    // The melody lessons start where "Melody over chords" does.
    if lesson.starts_with("melody-") {
        return theory_lesson(THEORY_MELODY);
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

/// The project part before `lesson`, whose result it builds on.
pub fn previous_part(lesson: &str) -> Option<&'static str> {
    [&HOUSE_PARTS[..], &LOFI_PARTS[..], &TRAP_PARTS[..]].into_iter().find_map(|parts| {
        let i = parts.iter().position(|p| *p == lesson)?;
        i.checked_sub(1).map(|j| parts[j])
    })
}

/// What a project's Carve tracks are called, in the order its parts add
/// them - so a learner's own previous part can be renamed to match.
pub fn part_track_names(lesson: &str) -> &'static [&'static str] {
    if LOFI_PARTS.contains(&lesson) {
        &["Keys", "Bass", "Melody", "Tanpura"]
    } else if TRAP_PARTS.contains(&lesson) {
        &["808", "Melody"]
    } else if HOUSE_PARTS.contains(&lesson) {
        &["Bass", "Chords"]
    } else {
        &[]
    }
}

/// The key and scale (a `theory::SCALE_PRESETS` name) the piano roll
/// should show for `lesson`, so its rows are the notes the steps name.
pub fn lesson_key(lesson: &str) -> Option<(u8, &'static str)> {
    Some(match lesson {
        TRAP_DRUMS | TRAP_808 | TRAP_MELODY | TRAP_ARRANGE => (9, "Natural minor"),
        RECIPE_KEYS | LOFI_BEAT | LOFI_KEYS | LOFI_BASS | LOFI_FINISH | BOLLY_MELODY | BOLLY_DRONE => (9, "Natural minor"),
        ARRANGE_HOUSE => crate::demo::DemoSong::House.key(),
        ARRANGE_BHAIRAV => crate::demo::DemoSong::Bhairav.key(),
        RECIPE_HARP => (9, "Raga Malkauns"),
        RECIPE_REED | THEORY_BHAIRAV => (0, "Raga Bhairav"),
        RECIPE_TANPURA => return None,
        // Every semitone a row: this lesson counts them.
        THEORY_SCALES => (0, "Chromatic"),
        _ if lesson.starts_with("theory-") || lesson.starts_with("melody-") => (0, "Major"),
        _ => (9, "Minor pentatonic"),
    })
}

/// The lo-fi chain as it stands after `parts` parts (0 = a blank 80 BPM
/// project, 4 = the finished lo-fi beat, 6 = the Bollywood lo-fi).
pub fn lofi_after(parts: usize) -> Project {
    let mut arr = empty_arrangement();
    arr.tempo_map = TempoMap::constant(LOFI_BPM, TimeSignature::FOUR_FOUR);
    let mut instruments = Vec::new();
    // Part 4 cuts an intro: the keys alone for four bars.
    let enter = if parts >= 4 { 4 } else { 0 };
    if parts >= 1 {
        let drums = add_track(&mut arr, "Drums", ClipColor::Coral, Instrument::Drums, if parts >= 4 { -8.0 } else { 0.0 });
        add_clip(&mut arr, drums, "Beat", enter, 16, 1, lofi_beat());
    }
    if parts >= 2 {
        let keys = add_track(&mut arr, "Keys", ClipColor::Violet, Instrument::Carve, 0.0);
        add_clip(&mut arr, keys, "Chords", 0, 16, 4, lofi_chords());
        instruments.push((keys, crate::synth::recipes::lofi_keys()));
        if parts >= 4 {
            // The intro's filter, opening as the beat arrives.
            let id = arr.alloc_id();
            arr.automation.push(AutomationLane {
                id,
                track: keys,
                parameter_name: "Carve \u{b7} Cutoff".into(),
                display_value: String::new(),
                breakpoints: vec![Breakpoint { tick: 0, value: 0.3 }, Breakpoint { tick: 4 * BAR, value: lofi_keys_open() }],
                target: Some(AutomationTarget::Synth(crate::synth::SynthParam::Cutoff)),
            });
        }
    }
    if parts >= 3 {
        let bass = add_track(&mut arr, "Bass", ClipColor::Blue, Instrument::Carve, 0.0);
        add_clip(&mut arr, bass, "Bassline", enter, 16, 4, lofi_bassline());
        instruments.push((bass, lofi_bass()));
    }
    if parts >= 5 {
        let melody = add_track(&mut arr, "Melody", ClipColor::Amber, Instrument::Carve, -2.0);
        add_clip(&mut arr, melody, "Phrase", 4, 16, 4, steps(&BOLLY_PHRASE));
        instruments.push((melody, crate::synth::recipes::indian_harp()));
    }
    if parts >= 6 {
        let drone = add_track(&mut arr, "Tanpura", ClipColor::Teal, Instrument::Carve, -10.0);
        add_clip(&mut arr, drone, "Drone", 0, 16, 1, steps(&BOLLY_TANPURA));
        instruments.push((drone, crate::synth::recipes::tanpura()));
    }
    Project { arrangement: arr, instruments, synth: None }
}

/// The trap chain as it stands after `parts` parts (0 = a blank 140 BPM
/// project, 4 = the arranged track).
pub fn trap_after(parts: usize) -> Project {
    let mut arr = empty_arrangement();
    arr.tempo_map = TempoMap::constant(TRAP_BPM, TimeSignature::FOUR_FOUR);
    let mut instruments = Vec::new();
    // Part 4 cuts an intro (the flute alone for four bars) and drops the
    // drums out for a bar before the hook comes round.
    let enter = if parts >= 4 { 4 } else { 0 };
    if parts >= 1 {
        let drums = add_track(&mut arr, "Drums", ClipColor::Coral, Instrument::Drums, -4.0);
        if parts >= 4 {
            add_clip(&mut arr, drums, "Beat", enter, TRAP_GAP_BAR, 1, trap_beat());
            add_clip(&mut arr, drums, "Beat", TRAP_GAP_BAR + 1, 16, 1, trap_beat());
        } else {
            add_clip(&mut arr, drums, "Beat", 0, 16, 1, trap_beat());
        }
    }
    if parts >= 2 {
        let bass = add_track(&mut arr, "808", ClipColor::Blue, Instrument::Carve, -4.0);
        add_clip(&mut arr, bass, "808", enter, 16, 1, steps(&TRAP_808_NOTES));
        instruments.push((bass, crate::synth::recipes::eight_oh_eight()));
    }
    if parts >= 3 {
        let melody = add_track(&mut arr, "Melody", ClipColor::Amber, Instrument::Carve, -6.0);
        add_clip(&mut arr, melody, "Flute", 0, 16, 2, steps(&TRAP_PHRASE));
        instruments.push((melody, crate::synth::recipes::flute()));
    }
    Project { arrangement: arr, instruments, synth: None }
}

/// Trap drums: half-time kick and snare, eighth-note hats, and a 32nd
/// roll through beat 4 that starts soft and climbs.
pub fn trap_beat() -> Vec<MidiNote> {
    let hit = |start, pitch, velocity| MidiNote { start, length: SIXTEENTH / 2, pitch, velocity };
    let mut notes: Vec<MidiNote> = TRAP_KICKS.iter().map(|&at| hit(at * SIXTEENTH, KICK, 120)).collect();
    notes.push(hit(TRAP_SNARE * SIXTEENTH, SNARE, 115));
    notes.extend((0..6).map(|i| hit(i * PPQ / 2, CLOSED_HAT, 85)));
    notes.extend((0..8).map(|i| hit(3 * PPQ + i * PPQ / 8, CLOSED_HAT, 50 + i as u8 * 8)));
    notes
}

/// Where the lo-fi intro's filter ends up: the Lo-fi Keys patch's own
/// cutoff, so the keys sound as designed once the beat is in.
pub fn lofi_keys_open() -> f32 {
    crate::synth::SynthParam::Cutoff.norm(&crate::synth::recipes::lofi_keys())
}

/// Boom-bap: kick on 1 and the "and" of 3, snare on 2 and 4, soft
/// closed hats on the eighths.
pub fn lofi_beat() -> Vec<MidiNote> {
    let hit = |start, pitch, velocity| MidiNote { start, length: SIXTEENTH, pitch, velocity };
    let mut notes = vec![hit(0, KICK, 120), hit(2 * PPQ + PPQ / 2, KICK, 105), hit(PPQ, SNARE, 110), hit(3 * PPQ, SNARE, 110)];
    notes.extend((0..8).map(|i| hit(i * PPQ / 2, CLOSED_HAT, if i % 2 == 0 { 60 } else { 45 })));
    notes
}

/// One held chord per bar.
pub fn lofi_chords() -> Vec<MidiNote> {
    LOFI_CHORDS
        .iter()
        .enumerate()
        .flat_map(|(bar, chord)| {
            chord.iter().map(move |&pitch| MidiNote { start: bar as i64 * BAR, length: BAR - SIXTEENTH, pitch, velocity: 85 })
        })
        .collect()
}

/// Each bar's root on beat 1 and on the "and" of 3, with the kicks.
pub fn lofi_bassline() -> Vec<MidiNote> {
    LOFI_BASS_ROOTS
        .iter()
        .enumerate()
        .flat_map(|(bar, &pitch)| {
            let at = bar as i64 * BAR;
            [(at, 6 * SIXTEENTH), (at + 2 * PPQ + PPQ / 2, 4 * SIXTEENTH)]
                .map(|(start, length)| MidiNote { start, length, pitch, velocity: 100 })
        })
        .collect()
}

/// Deep Bass made round for lo-fi: no drive, and a release that lets
/// each note bloom.
pub fn lofi_bass() -> SynthState {
    let mut s = crate::synth::recipes::deep_bass();
    s.filter.drive_db = 0.0;
    s.amp_env.release_ms = 700.0;
    s
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
pub(crate) fn add_clip(arr: &mut Arrangement, track: TrackId, name: &str, start_bar: i64, end_bar: i64, pattern_bars: i64, notes: Vec<MidiNote>) {
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
        swing: 0.0,
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

pub(crate) fn add_track(arr: &mut Arrangement, name: &str, color: ClipColor, instrument: Instrument, gain_db: f32) -> TrackId {
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
        drum_pads: Default::default(),
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
        swing: 0.0,
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn each_house_part_follows_the_one_before() {
        assert_eq!(super::previous_part(super::PROJECT_GROOVE), None);
        assert_eq!(super::previous_part(super::PROJECT_BASS), Some(super::PROJECT_GROOVE));
        assert_eq!(super::previous_part(super::PROJECT_FINISH), Some(super::PROJECT_ARRANGE));
        assert_eq!(super::previous_part(super::RECIPE_BASS), None);
    }

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
        for id in [CARVE_WAVES, CARVE_MIX, CARVE_FILTER, CARVE_ENVELOPES, CARVE_MOVEMENT, RECIPE_BASS, RECIPE_WOBBLE, RECIPE_FLUTE, RECIPE_HARP, RECIPE_LEAD, RECIPE_PAD, RECIPE_TANPURA, RECIPE_REED, RECIPE_KEYS] {
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

    #[test]
    fn the_lofi_chain_builds_part_by_part_in_a_minor() {
        let names = |n| lofi_after(n).arrangement.tracks.iter().map(|t| t.name.clone()).collect::<Vec<_>>();
        assert!(names(0).is_empty());
        assert_eq!(names(3), ["Drums", "Keys", "Bass"]);
        assert_eq!(names(6), ["Drums", "Keys", "Bass", "Melody", "Tanpura"]);
        let p = lofi_after(6);
        assert_eq!(p.arrangement.tempo_map.bpm_at(0), LOFI_BPM);
        // Every pitched note is in A natural minor (A B C D E F G), so on a
        // row the piano roll shows.
        let minor = [9, 11, 0, 2, 4, 5, 7];
        for clip in &p.arrangement.clips {
            let track = p.arrangement.track(clip.track).unwrap();
            if track.instrument == Some(Instrument::Carve) {
                assert!(clip.played_notes().iter().all(|n| minor.contains(&(n.pitch % 12))), "{}", track.name);
                assert!(clip.played_notes().iter().all(|n| n.pitch >= 57), "{} dips below the first row", track.name);
            }
        }
        // The finished beat has its intro: no drums or bass before bar 5.
        let four = lofi_after(4);
        for clip in &four.arrangement.clips {
            let name = &four.arrangement.track(clip.track).unwrap().name;
            if name == "Drums" || name == "Bass" {
                assert_eq!(clip.start, 4 * BAR, "{name}");
            }
        }
        assert_eq!(previous_part(BOLLY_MELODY), Some(LOFI_FINISH));
        assert_eq!(lesson_key(LOFI_KEYS), Some((9, "Natural minor")));
    }

    #[test]
    fn the_trap_chain_builds_part_by_part() {
        let names = |n| trap_after(n).arrangement.tracks.iter().map(|t| t.name.clone()).collect::<Vec<_>>();
        assert!(names(0).is_empty());
        assert_eq!(names(3), ["Drums", "808", "Melody"]);
        assert_eq!(part_track_names(TRAP_MELODY), ["808", "Melody"]);
        assert_eq!(previous_part(TRAP_ARRANGE), Some(TRAP_MELODY));
        let p = trap_after(4);
        assert_eq!(p.arrangement.tempo_map.bpm_at(0), TRAP_BPM);
        let minor = [9, 11, 0, 2, 4, 5, 7];
        for clip in &p.arrangement.clips {
            let track = p.arrangement.track(clip.track).unwrap();
            let notes = clip.played_notes();
            if track.instrument == Some(Instrument::Carve) {
                assert!(notes.iter().all(|n| minor.contains(&(n.pitch % 12)) && n.pitch >= 57), "{}", track.name);
            }
            // The arranged drums: nothing in the intro or the drop-out bar.
            if track.name == "Drums" {
                assert!(notes.iter().all(|n| {
                    let at = clip.start + n.start;
                    at >= 4 * BAR && !(TRAP_GAP_BAR * BAR..(TRAP_GAP_BAR + 1) * BAR).contains(&at)
                }));
            }
        }
        // The 808 slides: its C starts while the long A still sounds.
        let bass = steps(&TRAP_808_NOTES);
        assert!(bass.iter().any(|a| bass.iter().any(|b| b.pitch != a.pitch && b.start > a.start && b.start < a.start + a.length)));
    }

    #[test]
    fn step_lessons_start_with_the_beat_they_finish() {
        let hats = |id| {
            let p = starting_project(id);
            let ClipContent::Midi { notes, .. } = &p.arrangement.clips[0].content else { panic!() };
            notes.iter().filter(|n| n.pitch == CLOSED_HAT).count()
        };
        assert_eq!(hats(ROLL_PAINT), 0);
        assert_eq!(hats(ROLL_SWING), 16);
        assert_eq!(hats(ROLL_ROLLS), 8);
        assert_eq!(starting_project(ROLL_ROLLS).arrangement.tempo_map.bpm_at(0), TRAP_BPM);
    }

    #[test]
    fn every_lesson_key_is_a_real_scale() {
        for id in LOFI_PARTS.iter().chain(&HOUSE_PARTS).chain(&[RECIPE_HARP, RECIPE_REED, RECIPE_KEYS, FIRST_BEAT, ARRANGE_HOUSE, ARRANGE_BHAIRAV, THEORY_BHAIRAV]) {
            if let Some((root, scale)) = lesson_key(id) {
                assert!(root < 12, "{id}");
                assert!(crate::theory::SCALE_PRESETS.iter().any(|p| p.name == scale), "{id}: no scale {scale}");
            }
        }
    }
}