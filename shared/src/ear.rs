//! Ear training for a guitarist who wants to play what they hear in
//! their head: five steps, each one skill on the way there, in notes and
//! frets - no theory words.
//!
//! 1. Higher or lower: two notes - which way did it go?
//! 2. Step or jump: next door (a fret or two), or further?
//! 3. One string: the first note is lit on the A string - find the second
//!    on the same string (how far it went becomes how many frets).
//! 4. Short tunes: four notes from the lit one, in the box at the 5th fret
//!    most guitarists learn first.
//! 5. Tunes you know: the start of a tune everyone can hum, from memory,
//!    starting on the lit note - then hear it.
//!
//! Notes count in any octave and anywhere on the neck.

use crate::arrangement::{empty_arrangement, ClipColor, Instrument, MidiNote, TempoMap, TimeSignature, PPQ};
use crate::lessons::{add_clip, add_track};
use crate::theory::scale::NOTE_NAMES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Direction,
    Distance,
    OneString,
    Tunes,
    Known,
}

impl Step {
    pub const ALL: [Step; 5] = [Step::Direction, Step::Distance, Step::OneString, Step::Tunes, Step::Known];

    pub fn title(self) -> &'static str {
        match self {
            Step::Direction => "Higher or lower",
            Step::Distance => "Step or jump",
            Step::OneString => "One string",
            Step::Tunes => "Short tunes",
            Step::Known => "Tunes you know",
        }
    }

    /// Why this step, in a line.
    pub fn why(self) -> &'static str {
        match self {
            Step::Direction => "Hearing which way a tune moves is the first thing to get right.",
            Step::Distance => "Then how far: the next fret or two, or a leap.",
            Step::OneString => "On one string, how far it went is how many frets.",
            Step::Tunes => "Short tunes in the shape at the 5th fret most players learn first.",
            Step::Known => "The real thing: a tune already in your head, onto the guitar.",
        }
    }

    pub fn index(self) -> usize {
        Step::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// An answer for the first two steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Lower,
    Same,
    Higher,
    Step,
    Jump,
}

impl Choice {
    pub fn name(self) -> &'static str {
        match self {
            Choice::Lower => "Lower",
            Choice::Same => "Same",
            Choice::Higher => "Higher",
            Choice::Step => "A step",
            Choice::Jump => "A jump",
        }
    }
}

/// Standard tuning, low E to high E.
pub const TUNING: [u8; 6] = [40, 45, 50, 55, 59, 64];
pub const STRING_NAMES: [&str; 6] = ["E", "A", "D", "G", "B", "e"];
/// The neck shown: open strings to the 12th fret.
pub const FRETS: u8 = 12;

pub fn pitch_at(string: usize, fret: u8) -> u8 {
    TUNING[string] + fret
}

/// The A minor pentatonic box at the 5th fret: two notes a string.
pub const BOX: [(usize, u8); 12] = [(0, 5), (0, 8), (1, 5), (1, 7), (2, 5), (2, 7), (3, 5), (3, 7), (4, 5), (4, 8), (5, 5), (5, 8)];

/// Where to show `pitch` on the neck: the place nearest `near` (or an
/// octave or two either way if it's off the neck).
pub fn position_of(pitch: u8, near: (usize, u8)) -> Option<(usize, u8)> {
    for p in [0, -12, 12, -24, 24].map(|o| pitch as i32 + o) {
        let best = (0..6)
            .filter_map(|s| {
                let fret = p - TUNING[s] as i32;
                (0..=FRETS as i32).contains(&fret).then_some((s, fret as u8))
            })
            .min_by_key(|&(s, f)| (f as i32 - near.1 as i32).abs() * 2 + (s as i32 - near.0 as i32).abs() * 3);
        if best.is_some() {
            return best;
        }
    }
    None
}

/// "A", "C#".
pub fn name(pitch: u8) -> &'static str {
    NOTE_NAMES[(pitch % 12) as usize]
}

/// "1st", "2nd", ...
pub fn ordinal(i: usize) -> String {
    let n = i + 1;
    let suffix = match n {
        1 => "st",
        2 => "nd",
        3 => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    pub step: Step,
    /// Every note, the first one given.
    pub notes: Vec<u8>,
    /// Where the first note is lit on the neck.
    pub given_at: (usize, u8),
    /// Tunes you know: which.
    pub title: Option<&'static str>,
}

impl Question {
    /// The answer, for the first two steps.
    pub fn choice(&self) -> Option<Choice> {
        let d = self.notes[1] as i32 - self.notes[0] as i32;
        match self.step {
            Step::Direction => Some(match d {
                0 => Choice::Same,
                d if d > 0 => Choice::Higher,
                _ => Choice::Lower,
            }),
            Step::Distance => Some(if d.abs() <= 2 { Choice::Step } else { Choice::Jump }),
            _ => None,
        }
    }

    /// The notes to find (all but the given first).
    pub fn to_find(&self) -> &[u8] {
        &self.notes[1..]
    }

    /// Whether the whole question plays before you answer (Tunes you know
    /// plays only the first note: the rest is in your head).
    pub fn plays_all(&self) -> bool {
        self.step != Step::Known
    }
}

/// A small, fast random step: xorshift.
fn next(seed: &mut u32) -> u32 {
    let mut x = (*seed).max(1);
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *seed = x;
    x
}

/// A new question; `avoid` is the last one's notes, so it doesn't repeat.
pub fn question(step: Step, seed: u32, avoid: &[u8]) -> Question {
    let mut seed = seed;
    let mut q = make(step, &mut seed);
    for _ in 0..8 {
        if q.notes.as_slice() != avoid {
            break;
        }
        q = make(step, &mut seed);
    }
    q
}

fn make(step: Step, seed: &mut u32) -> Question {
    let mut pick = |n: u32| next(seed) % n;
    match step {
        Step::Direction | Step::Distance => {
            let first = 45 + pick(13) as u8;
            let size = match step {
                // Same now and then; otherwise up to a 5th either way.
                Step::Direction if pick(7) == 0 => 0,
                Step::Direction => 1 + pick(7) as i32,
                // A step (1-2) or a clear jump (4-7): nothing in between.
                _ if pick(2) == 0 => 1 + pick(2) as i32,
                _ => 4 + pick(4) as i32,
            };
            let second = if pick(2) == 0 { first as i32 + size } else { first as i32 - size };
            let given_at = position_of(first, (1, 5)).unwrap_or((1, 0));
            Question { step, notes: vec![first, second as u8], given_at, title: None }
        }
        Step::OneString => {
            let from = pick(FRETS as u32 + 1) as i32;
            let mut to = from;
            while to == from || !(0..=FRETS as i32).contains(&to) {
                to = from + pick(11) as i32 - 5;
            }
            Question { step, notes: vec![pitch_at(1, from as u8), pitch_at(1, to as u8)], given_at: (1, from as u8), title: None }
        }
        Step::Tunes => {
            let pitches: Vec<u8> = BOX.iter().map(|&(s, f)| pitch_at(s, f)).collect();
            // From an A: the low one or the middle one.
            let mut at = if pick(2) == 0 { 0 } else { 5 };
            let given_at = BOX[at];
            let mut notes = vec![pitches[at]];
            while notes.len() < 4 {
                let step = 1 + pick(2) as usize;
                let up = pick(2) == 0;
                let to = if up { at + step } else { at.wrapping_sub(step) };
                if to < pitches.len() {
                    at = to;
                    notes.push(pitches[at]);
                }
            }
            Question { step, notes, given_at, title: None }
        }
        Step::Known => {
            let (title, tune) = KNOWN[pick(KNOWN.len() as u32) as usize];
            // Starting on A (57), kept on the neck.
            let mut shift = 57 - tune[0] as i32;
            if tune.iter().any(|&n| n as i32 + shift < 45) {
                shift += 12;
            }
            if tune.iter().any(|&n| n as i32 + shift > 76) {
                shift -= 12;
            }
            let notes: Vec<u8> = tune.iter().map(|&n| (n as i32 + shift) as u8).collect();
            let given_at = position_of(notes[0], (2, 7)).unwrap_or((2, 7));
            Question { step, notes, given_at, title: Some(title) }
        }
    }
}

/// Tunes nearly everyone can hum, their openings (all public domain).
pub const KNOWN: &[(&str, &[u8])] = &[
    ("Twinkle, Twinkle, Little Star", &[60, 60, 67, 67, 69, 69, 67]),
    ("Happy Birthday", &[67, 67, 69, 67, 72, 71]),
    ("Mary Had a Little Lamb", &[64, 62, 60, 62, 64, 64, 64]),
    ("Ode to Joy", &[64, 64, 65, 67, 67, 65, 64]),
    ("Fr\u{e8}re Jacques", &[60, 62, 64, 60, 60, 62, 64]),
    ("When the Saints Go Marching In", &[60, 64, 65, 67, 60, 64, 65]),
    ("F\u{fc}r Elise", &[76, 75, 76, 75, 76, 71, 74]),
    ("Beethoven's 5th", &[67, 67, 67, 63, 65, 65, 65]),
];

/// Each note to find, right or not (any octave).
pub fn check(q: &Question, played: &[u8]) -> Vec<bool> {
    q.to_find().iter().enumerate().map(|(i, &n)| played.get(i).is_some_and(|&p| p % 12 == n % 12)).collect()
}

/// Points for a right answer: more for a longer streak (x1 to x4).
pub fn points(streak: u32) -> u32 {
    10 * (1 + (streak / 3).min(3))
}

/// Right answers in a row before suggesting the next step.
pub const READY_STREAK: u32 = 5;

pub const BPM: f64 = 90.0;

/// How long each note takes, in seconds.
pub fn note_secs() -> f32 {
    60.0 / BPM as f32
}

/// The notes, one a beat, as a project to render.
pub fn project(notes: &[u8]) -> crate::project::Project {
    let mut arr = empty_arrangement();
    arr.tempo_map = TempoMap::constant(BPM, TimeSignature::FOUR_FOUR);
    let out: Vec<MidiNote> =
        notes.iter().enumerate().map(|(i, &pitch)| MidiNote { start: i as i64 * PPQ, length: PPQ - PPQ / 8, pitch, velocity: 105 }).collect();
    let bars = ((notes.len() as i64 + 3) / 4).max(1);
    let keys = add_track(&mut arr, "Piano", ClipColor::Violet, Instrument::Carve, 0.0);
    add_clip(&mut arr, keys, "Ear", 0, bars, bars, out);
    crate::project::Project { arrangement: arr, instruments: vec![(keys, crate::synth::recipes::piano())], synth: None }
}

/// The project's end in ticks.
pub fn project_end(notes: &[u8]) -> i64 {
    notes.len() as i64 * PPQ
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

    fn many(step: Step) -> Vec<Question> {
        (1..400u32).map(|s| question(step, s.wrapping_mul(2_654_435_761), &[])).collect()
    }

    #[test]
    fn the_first_two_steps_have_clear_answers() {
        let qs = many(Step::Direction);
        for q in &qs {
            assert_eq!(q.notes.len(), 2);
            assert!((q.notes[1] as i32 - q.notes[0] as i32).abs() <= 7);
        }
        for c in [Choice::Lower, Choice::Same, Choice::Higher] {
            assert!(qs.iter().any(|q| q.choice() == Some(c)), "{c:?} never comes");
        }
        let qs = many(Step::Distance);
        for q in &qs {
            let d = (q.notes[1] as i32 - q.notes[0] as i32).abs();
            assert!((1..=2).contains(&d) || (4..=7).contains(&d), "{d}: neither a step nor a clear jump");
        }
        assert!(qs.iter().any(|q| q.choice() == Some(Choice::Step)));
        assert!(qs.iter().any(|q| q.choice() == Some(Choice::Jump)));
    }

    #[test]
    fn one_string_stays_on_the_a_string() {
        for q in many(Step::OneString) {
            assert_eq!(q.given_at.0, 1);
            assert_eq!(pitch_at(q.given_at.0, q.given_at.1), q.notes[0]);
            let to = q.notes[1] as i32 - 45;
            assert!((0..=12).contains(&to) && q.notes[1] != q.notes[0]);
            assert!((q.notes[1] as i32 - q.notes[0] as i32).abs() <= 5);
        }
    }

    #[test]
    fn short_tunes_stay_in_the_box_from_an_a() {
        let box_pitches: Vec<u8> = BOX.iter().map(|&(s, f)| pitch_at(s, f)).collect();
        for q in many(Step::Tunes) {
            assert_eq!(q.notes.len(), 4);
            assert_eq!(q.notes[0] % 12, 9, "starts on an A");
            assert!(q.notes.iter().all(|n| box_pitches.contains(n)));
            assert!(q.notes.windows(2).all(|w| w[0] != w[1]));
            assert_eq!(pitch_at(q.given_at.0, q.given_at.1), q.notes[0]);
        }
    }

    #[test]
    fn known_tunes_start_on_the_lit_a_and_keep_their_shape() {
        for q in many(Step::Known) {
            assert_eq!(q.notes[0] % 12, 9);
            assert!(q.notes.iter().all(|&n| (40..=76).contains(&n)), "{:?}", q.notes);
            assert!(q.title.is_some());
            assert!(!q.plays_all());
        }
        // Ode to Joy, from A: A A Bb C C Bb A.
        let ode = (1..400u32).map(|s| question(Step::Known, s * 7919, &[])).find(|q| q.title == Some("Ode to Joy")).unwrap();
        assert_eq!(ode.notes.iter().map(|&n| name(n)).collect::<Vec<_>>(), ["A", "A", "A#", "C", "C", "A#", "A"]);
        // Every tune comes up.
        let titles: std::collections::HashSet<_> = many(Step::Known).iter().filter_map(|q| q.title).collect();
        assert_eq!(titles.len(), KNOWN.len());
    }

    #[test]
    fn answers_count_in_any_octave() {
        let q = Question { step: Step::Tunes, notes: vec![57, 60, 62, 64], given_at: (2, 7), title: None };
        assert_eq!(check(&q, &[48, 74, 63]), [true, true, false]);
    }

    #[test]
    fn notes_are_shown_near_the_lit_one() {
        assert_eq!(position_of(45, (1, 5)), Some((0, 5)), "A on the low E's 5th fret, not the open A");
        assert_eq!(position_of(45, (1, 0)), Some((1, 0)));
        assert_eq!(position_of(57, (2, 7)), Some((2, 7)));
        assert_eq!(position_of(90, (2, 7)).map(|(s, f)| pitch_at(s, f) % 12), Some(90 % 12), "off the neck: an octave down");
        assert_eq!(ordinal(0), "1st");
        assert_eq!(ordinal(3), "4th");
    }

    #[test]
    fn a_mic_note_counts_once_held_and_again_after_a_gap() {
        let mut f = NoteFollower::default();
        let mut got = Vec::new();
        let readings = [
            None, Some(59.6), Some(60.1), Some(59.9), Some(60.2), Some(60.0),
            Some(64.0), Some(64.1), Some(63.9),
            Some(64.0), None, None, None, None, Some(64.0), Some(64.0), Some(64.1),
            Some(70.0), Some(65.0),
        ];
        for r in readings {
            got.extend(f.push(r));
        }
        assert_eq!(got, [60, 64, 64]);
        assert!((midi_of(82.41) - 40.0).abs() < 0.01, "a guitar's low E");
    }
}
