//! Ear training for hearing a melody in your head and finding it on an
//! instrument. Everything is a scale degree in the song's key, the way a
//! tune sits in the mind ("it starts on the 5th and falls to the 3rd"),
//! never an absolute note:
//!
//! - Find: the key, then one note - which degree is it?
//! - Echo: the key, then a short melody - play it back.
//! - Imagine: degrees on screen - hear them inside, play them, then hear
//!   whether that's what you imagined.
//!
//! Degrees come from the scale mask, so it works for major, a pentatonic
//! or a raag alike. Answers compare pitch classes: any octave counts (a
//! guitar sounds an octave under the piano's written note anyway).

use crate::arrangement::{empty_arrangement, ClipColor, Instrument, MidiNote, TempoMap, TimeSignature, PPQ};
use crate::lessons::{add_clip, add_track};
use crate::theory::scale::{degree_name, degrees_in_mask, sargam_name};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Find,
    Echo,
    Imagine,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Find, Mode::Echo, Mode::Imagine];

    pub fn name(self) -> &'static str {
        match self {
            Mode::Find => "Find the note",
            Mode::Echo => "Echo",
            Mode::Imagine => "Imagine",
        }
    }

    pub fn how(self) -> &'static str {
        match self {
            Mode::Find => "Hear the key, then a note. Which degree? Tap it, or play it.",
            Mode::Echo => "Hear the key, then a melody. Play it back.",
            Mode::Imagine => "Hear the numbers in your head, play them, then check.",
        }
    }
}

pub const LEVELS: usize = 5;

/// Right answers in a row that move you up a level.
pub const LEVEL_UP_STREAK: u32 = 6;

/// What each level adds, for the label next to it.
pub fn level_name(level: usize) -> &'static str {
    ["Home notes: 1 3 5", "Five notes", "The whole scale", "Wider: up to the 8", "Two octaves"][level.min(LEVELS - 1)]
}

/// The scale's degrees in the order they're easiest to hear: home (1),
/// the 5th, the 3rd, then the rest from the steadiest out.
fn by_ease(mask: u16) -> Vec<u8> {
    const ORDER: [u8; 12] = [0, 7, 4, 3, 5, 2, 9, 8, 11, 10, 6, 1];
    let degrees = degrees_in_mask(mask | 1);
    ORDER.iter().copied().filter(|d| degrees.contains(d)).collect()
}

/// The degrees (semitones above home, 0..12) a level uses, in scale order.
pub fn pool(level: usize, mask: u16) -> Vec<u8> {
    let easy = by_ease(mask);
    let n = match level {
        0 => 3,
        1 => 5,
        _ => easy.len(),
    };
    let mut out: Vec<u8> = easy.into_iter().take(n).collect();
    out.sort_unstable();
    out
}

/// A degree's name: "1", "♭3", "5" (or Sa, ga, Pa).
pub fn label(degree: u8, sargam: bool) -> String {
    if sargam {
        return sargam_name(degree).to_string();
    }
    degree_name(degree).replace('b', "\u{266d}").replace('#', "\u{266f}")
}

/// Home (degree 1) for `key`: between F3 and E4, under a guitar's
/// middle and a comfortable singing range.
pub fn home(key: u8) -> u8 {
    let key = key % 12;
    if key <= 4 {
        60 + key
    } else {
        48 + key
    }
}

/// The notes a level may use, as pitches, low to high.
fn range(level: usize, key: u8, mask: u16) -> Vec<u8> {
    let tonic = home(key);
    let degrees = pool(level, mask);
    let mut out: Vec<u8> = degrees.iter().map(|d| tonic + d).collect();
    // Melodies dip under home from level 2 (the 5th below is where so
    // many begin).
    if level >= 1 {
        out.extend(degrees.iter().filter(|&&d| d >= 7).map(|d| tonic + d - 12));
    }
    if level >= 3 {
        out.push(tonic + 12);
    }
    if level >= 4 {
        out.extend(degrees.iter().filter(|&&d| d >= 5).map(|d| tonic + d - 12));
        out.extend(degrees.iter().filter(|&&d| d > 0 && d <= 7).map(|d| tonic + 12 + d));
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    pub mode: Mode,
    pub key: u8,
    /// The notes to hear (Find: one), as pitches.
    pub notes: Vec<u8>,
}

impl Question {
    /// Each note's degree.
    pub fn degrees(&self) -> Vec<u8> {
        self.notes.iter().map(|&n| degree_of(n, self.key)).collect()
    }
}

pub fn degree_of(pitch: u8, key: u8) -> u8 {
    (pitch + 12 - key % 12) % 12
}

/// A small, fast random step (no dependency): xorshift.
fn next(seed: &mut u32) -> u32 {
    let mut x = (*seed).max(1);
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *seed = x;
    x
}

/// How many notes a melody has at a level.
fn length(level: usize) -> usize {
    [3, 3, 4, 4, 5][level.min(LEVELS - 1)]
}

/// A new question. `avoid` is the last one's notes, so the same thing
/// doesn't come twice running.
pub fn question(mode: Mode, level: usize, key: u8, mask: u16, seed: u32, avoid: &[u8]) -> Question {
    let notes = range(level, key, mask);
    let mut seed = seed;
    let pick = |seed: &mut u32| -> Vec<u8> {
        match mode {
            Mode::Find => vec![notes[next(seed) as usize % notes.len()]],
            Mode::Echo | Mode::Imagine => melody(&notes, home(key), length(level), level, seed),
        }
    };
    let mut out = pick(&mut seed);
    for _ in 0..8 {
        if out.as_slice() != avoid {
            break;
        }
        out = pick(&mut seed);
    }
    Question { mode, key: key % 12, notes: out }
}

/// A walk over `notes`: steps mostly, the odd leap from level 2 up, never
/// the same note twice running (a guitar can't tell a second pluck from
/// the first ringing on). The first level starts at home, the first two
/// end there.
fn melody(notes: &[u8], home: u8, len: usize, level: usize, seed: &mut u32) -> Vec<u8> {
    let home_at = notes.iter().position(|&n| n == home).unwrap_or(0);
    let mut at = if level == 0 { home_at } else { next(seed) as usize % notes.len() };
    let mut out = vec![notes[at]];
    let reach = if level < 4 { 2 } else { 3 };
    while out.len() < len {
        let step = 1 + next(seed) as usize % reach;
        let up = next(seed) % 2 == 0;
        let to = if up { at + step } else { at.wrapping_sub(step) };
        let to = if to >= notes.len() { if up { at.saturating_sub(step) } else { (at + step).min(notes.len() - 1) } } else { to };
        if to == at {
            continue;
        }
        at = to;
        out.push(notes[at]);
    }
    // Early melodies end at home: they resolve, which is easier to hold.
    if level < 2 && len > 2 {
        out.truncate(len - 1);
        let i = out.len() - 1;
        if out[i] == home {
            // The note before home can't be home: a neighbour instead.
            let before = if i > 0 { Some(out[i - 1]) } else { None };
            let near = [home_at + 1, home_at.wrapping_sub(1), home_at + 2]
                .into_iter()
                .filter_map(|j| notes.get(j).copied())
                .find(|&n| n != home && Some(n) != before);
            if let Some(n) = near {
                out[i] = n;
            }
        }
        out.push(home);
    }
    out
}

/// What the notes played say: each right or not (any octave).
pub fn check(question: &Question, played: &[u8]) -> Vec<bool> {
    question.notes.iter().enumerate().map(|(i, &n)| played.get(i).is_some_and(|&p| p % 12 == n % 12)).collect()
}

/// Points for a right answer: more for a longer streak (x1 to x4) and a
/// higher level.
pub fn points(streak: u32, level: usize) -> u32 {
    let combo = 1 + (streak / 3).min(3);
    10 * combo * (1 + level as u32 / 2)
}

/// The key, then (optionally) the question, as a project to render: the
/// home chord moving I-IV-V-I when the scale has seven notes (just home's
/// chord otherwise), a beat's rest, then the notes. Returns the project
/// and the tick where the question starts.
pub fn project(q: &Question, mask: u16, bpm: f64, cadence: bool, notes: bool) -> (crate::project::Project, i64) {
    let mut arr = empty_arrangement();
    arr.tempo_map = TempoMap::constant(bpm, TimeSignature::FOUR_FOUR);
    let tonic = home(q.key);
    let mut out: Vec<MidiNote> = Vec::new();
    let mut t = 0;
    let note = |start: i64, length: i64, pitch: u8, velocity: u8| MidiNote { start, length, pitch, velocity };
    if cadence {
        let degrees = degrees_in_mask(mask | 1);
        let chord = |root_index: usize| -> Vec<u8> {
            if degrees.len() == 7 {
                (0..3).map(|i| degrees[(root_index + 2 * i) % 7] + if root_index + 2 * i >= 7 { 12 } else { 0 }).collect()
            } else {
                let third = [4u8, 3].into_iter().find(|d| degrees.contains(d));
                let fifth = [7u8].into_iter().find(|d| degrees.contains(d));
                std::iter::once(0).chain(third).chain(fifth).collect()
            }
        };
        let steps: Vec<(usize, i32)> = if degrees.len() == 7 { vec![(0, 0), (3, 0), (4, -12), (0, 0)] } else { vec![(0, 0), (0, 0)] };
        for (root_index, shift) in steps {
            for d in chord(root_index) {
                let pitch = (tonic as i32 + d as i32 + shift).clamp(0, 127) as u8;
                out.push(note(t, PPQ - PPQ / 8, pitch, 80));
            }
            // A low home note under each, so the key is unmistakable.
            out.push(note(t, PPQ - PPQ / 8, tonic.saturating_sub(12), 70));
            t += PPQ;
        }
        t += PPQ;
    }
    let start = t;
    if notes {
        let len = if q.notes.len() == 1 { 2 * PPQ } else { PPQ };
        for &pitch in &q.notes {
            out.push(note(t, len - PPQ / 8, pitch, 105));
            t += len;
        }
    }
    let bar = 4 * PPQ;
    let bars = ((t + bar - 1) / bar).max(1);
    let keys = add_track(&mut arr, "Piano", ClipColor::Violet, Instrument::Carve, 0.0);
    add_clip(&mut arr, keys, "Ear", 0, bars, bars, out);
    (crate::project::Project { arrangement: arr, instruments: vec![(keys, crate::synth::recipes::piano())], synth: None }, start)
}

/// The project's length in ticks (the last note's end).
pub fn project_end(project: &crate::project::Project) -> i64 {
    project
        .arrangement
        .clips
        .iter()
        .filter_map(|c| match &c.content {
            crate::arrangement::ClipContent::Midi { notes, .. } => notes.iter().map(|n| n.start + n.length).max(),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

/// Notes out of a stream of pitch readings from a mic (a guitar, a voice):
/// a note counts once it has held the same semitone for a few readings,
/// and the next one when the semitone changes or after a gap.
#[derive(Default)]
pub struct NoteFollower {
    candidate: Option<u8>,
    count: u32,
    last: Option<u8>,
    silent: u32,
}

impl NoteFollower {
    /// Readings (at 40 a second) a semitone must hold to count: 75 ms.
    const HOLD: u32 = 3;
    /// Readings of quiet after which the same note again is a new one.
    const GAP: u32 = 4;

    /// A reading: a MIDI pitch (fractional), or `None` for silence.
    /// Returns a note when one starts.
    pub fn push(&mut self, pitch: Option<f32>) -> Option<u8> {
        let Some(pitch) = pitch.filter(|p| (20.0..120.0).contains(p)) else {
            self.silent += 1;
            self.candidate = None;
            self.count = 0;
            if self.silent >= Self::GAP {
                self.last = None;
            }
            return None;
        };
        self.silent = 0;
        let semitone = pitch.round() as u8;
        if self.candidate == Some(semitone) {
            self.count += 1;
        } else {
            self.candidate = Some(semitone);
            self.count = 1;
        }
        if self.count == Self::HOLD && self.last != Some(semitone) {
            self.last = Some(semitone);
            return Some(semitone);
        }
        None
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// A frequency as a (fractional) MIDI pitch.
pub fn midi_of(hz: f32) -> f32 {
    69.0 + 12.0 * (hz / 440.0).log2()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAJOR: u16 = 0b1010_1011_0101;
    const PENTA_MINOR: u16 = 1 | 1 << 3 | 1 << 5 | 1 << 7 | 1 << 10;

    #[test]
    fn levels_start_from_home_and_grow() {
        assert_eq!(pool(0, MAJOR), [0, 4, 7]);
        assert_eq!(pool(1, MAJOR), [0, 2, 4, 5, 7]);
        assert_eq!(pool(2, MAJOR), [0, 2, 4, 5, 7, 9, 11]);
        // Minor pentatonic: 1 b3 5 first.
        assert_eq!(pool(0, PENTA_MINOR), [0, 3, 7]);
        assert_eq!(pool(1, PENTA_MINOR), [0, 3, 5, 7, 10]);
        assert_eq!(label(3, false), "\u{266d}3");
        assert_eq!(label(7, true), "Pa");
    }

    #[test]
    fn questions_stay_in_the_key_and_the_level() {
        for level in 0..LEVELS {
            for seed in 1..200u32 {
                for mode in Mode::ALL {
                    let q = question(mode, level, 9, MAJOR, seed.wrapping_mul(2_654_435_761), &[]);
                    let allowed = pool(level, MAJOR);
                    assert!(q.degrees().iter().all(|d| allowed.contains(d)), "{mode:?} {level} {:?}", q.notes);
                    match mode {
                        Mode::Find => assert_eq!(q.notes.len(), 1),
                        _ => {
                            assert_eq!(q.notes.len(), length(level));
                            assert!(q.notes.windows(2).all(|w| w[0] != w[1]), "repeats: {:?}", q.notes);
                        }
                    }
                    assert!(q.notes.iter().all(|&n| (36..=84).contains(&n)));
                }
            }
        }
    }

    #[test]
    fn the_same_question_doesnt_come_twice_running() {
        let a = question(Mode::Find, 0, 0, MAJOR, 7, &[]);
        let mut seed = 7u32;
        for _ in 0..50 {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            assert_ne!(question(Mode::Find, 0, 0, MAJOR, seed, &a.notes).notes, a.notes);
        }
    }

    #[test]
    fn early_melodies_go_home() {
        for seed in 1..100u32 {
            let q = question(Mode::Echo, 0, 0, MAJOR, seed * 977, &[]);
            assert_eq!(q.notes[0], home(0));
            assert_eq!(*q.notes.last().unwrap(), home(0));
            // The second level dips under home, and still ends there.
            let q = question(Mode::Echo, 1, 0, MAJOR, seed * 977, &[]);
            assert_eq!(*q.notes.last().unwrap(), home(0), "{:?}", q.notes);
        }
        // ...and its melodies aren't all the same shape.
        let shapes: std::collections::HashSet<Vec<u8>> = (1..60u32).map(|s| question(Mode::Echo, 1, 0, MAJOR, s * 7919, &[]).notes).collect();
        assert!(shapes.len() >= 6, "{shapes:?}");
    }

    #[test]
    fn answers_count_in_any_octave() {
        let q = Question { mode: Mode::Echo, key: 0, notes: vec![60, 64, 67] };
        assert_eq!(check(&q, &[48, 76, 66]), [true, true, false]);
        assert_eq!(check(&q, &[60]), [true, false, false]);
    }

    #[test]
    fn the_key_comes_first_then_the_notes() {
        let q = Question { mode: Mode::Echo, key: 0, notes: vec![60, 62, 64] };
        let (with, start) = project(&q, MAJOR, 100.0, true, true);
        assert_eq!(start, 5 * PPQ, "four chords and a beat's rest");
        assert_eq!(project_end(&with), start + 3 * PPQ - PPQ / 8);
        let (without, start) = project(&q, MAJOR, 100.0, false, true);
        assert_eq!(start, 0);
        assert!(project_end(&without) < project_end(&with));
        // A pentatonic gets just home's chord, twice.
        let (penta, start) = project(&q, PENTA_MINOR, 100.0, true, false);
        assert_eq!(start, 3 * PPQ);
        assert!(project_end(&penta) > 0);
    }

    #[test]
    fn points_grow_with_the_streak() {
        assert_eq!(points(0, 0), 10);
        assert_eq!(points(3, 0), 20);
        assert_eq!(points(30, 0), 40);
        assert_eq!(points(0, 2), 20);
    }

    #[test]
    fn a_mic_note_counts_once_held_and_again_after_a_gap() {
        let mut f = NoteFollower::default();
        let mut got = Vec::new();
        let readings = [
            None, Some(59.6), Some(60.1), Some(59.9), Some(60.2), Some(60.0), // C held
            Some(64.0), Some(64.1), Some(63.9),                              // E
            Some(64.0), None, None, None, None, Some(64.0), Some(64.0), Some(64.1), // E again after a gap
            Some(70.0), Some(65.0), // a blip: not held
        ];
        for r in readings {
            got.extend(f.push(r));
        }
        assert_eq!(got, [60, 64, 64]);
        assert!((midi_of(440.0) - 69.0).abs() < 1e-4);
        assert!((midi_of(82.41) - 40.0).abs() < 0.01, "a guitar's low E");
    }
}
