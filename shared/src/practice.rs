//! Practice: hear a one-bar phrase, then play it back. Rhythm exercises
//! are a rhythm to tap on any key; melody exercises are a few notes of the
//! key's scale. Each level makes a new exercise every time (seeded), and
//! what you played is scored here - which hits were on time, which notes
//! were right - so the UI only plays, listens and draws.

use crate::theory::degrees_in_mask;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Rhythm,
    Melody,
}

pub const LEVELS: usize = 5;

impl Kind {
    /// Each level's name, easiest first.
    pub fn levels(self) -> [&'static str; LEVELS] {
        match self {
            Kind::Rhythm => ["Quarter notes", "Eighth notes", "Off-beats", "Sixteenths", "Syncopation"],
            Kind::Melody => ["Three notes, by step", "Four notes, by step", "With a leap", "Five notes", "With a rhythm"],
        }
    }
}

/// One exercise: its notes, each (16th of the bar, pitch, length in 16ths).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exercise {
    pub kind: Kind,
    pub level: usize,
    pub notes: Vec<(i64, u8, i64)>,
}

/// The pitch a rhythm is played on (the clap, on a Drum Kit).
pub const RHYTHM_PITCH: u8 = crate::drums::CLAP;

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        let mut x = self.0.max(1);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u32) as usize
    }
    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }
}

/// A new rhythm exercise at `level` (0 = easiest).
pub fn rhythm(level: usize, seed: u32) -> Exercise {
    let mut rng = Rng(seed.wrapping_mul(2_654_435_761) | 1);
    // Per beat, a pattern of 16ths (1 = a hit) - more kinds as levels rise.
    let beat_patterns: &[&[u8]] = match level {
        0 => &[&[1, 0, 0, 0], &[0, 0, 0, 0]],
        1 => &[&[1, 0, 0, 0], &[1, 0, 1, 0], &[1, 0, 1, 0], &[0, 0, 0, 0]],
        2 => &[&[0, 0, 1, 0], &[1, 0, 1, 0], &[0, 0, 1, 0], &[1, 0, 0, 0]],
        3 => &[&[1, 1, 1, 1], &[1, 0, 1, 1], &[1, 1, 1, 0], &[1, 0, 1, 0], &[1, 0, 0, 0]],
        _ => &[],
    };
    let onsets: Vec<i64> = if level >= 4 {
        // Syncopation: hits that land between beats and carry across them.
        let templates: [&[i64]; 6] = [&[0, 3, 6, 10, 12], &[0, 3, 8, 11, 14], &[0, 6, 8, 14], &[0, 3, 6, 8, 12], &[0, 2, 6, 10, 14], &[0, 3, 7, 10, 12]];
        rng.pick(&templates).to_vec()
    } else {
        loop {
            let mut onsets = Vec::new();
            for beat in 0..4i64 {
                let pattern = rng.pick(beat_patterns);
                // The bar always starts on its first beat, except off-beat
                // practice, whose point is the "and".
                let pattern: &[u8] = if beat == 0 && level != 2 { if pattern[0] == 1 { pattern } else { &[1, 0, 0, 0] } } else { pattern };
                onsets.extend(pattern.iter().enumerate().filter(|(_, h)| **h == 1).map(|(i, _)| beat * 4 + i as i64));
            }
            let enough = onsets.len() >= 3 && (level != 2 || onsets.iter().filter(|o| *o % 4 == 2).count() >= 2);
            if enough {
                break onsets;
            }
        }
    };
    let notes = onsets.iter().map(|&o| (o, RHYTHM_PITCH, 1)).collect();
    Exercise { kind: Kind::Rhythm, level, notes }
}

/// A new melody exercise at `level` in the key (`key` 0 = C, `mask` its
/// scale), around middle C.
pub fn melody(level: usize, seed: u32, key: u8, mask: u16) -> Exercise {
    let mut rng = Rng(seed.wrapping_mul(2_246_822_519) | 1);
    let degrees = degrees_in_mask(mask);
    let degrees = if degrees.is_empty() { vec![0, 2, 4, 5, 7, 9, 11] } else { degrees };
    let root = 60 + key as i32 - if key > 6 { 12 } else { 0 };
    let pitch = |step: i32| -> u8 {
        let n = degrees.len() as i32;
        (root + 12 * step.div_euclid(n) + degrees[step.rem_euclid(n) as usize] as i32) as u8
    };
    let (count, span, max_leap) = match level {
        0 => (3, 2, 1),
        1 => (4, 4, 1),
        2 => (4, 4, 2),
        3 => (5, 7, 4),
        _ => (5, 5, 2),
    };
    let steps = loop {
        let mut steps = vec![0i32];
        let mut leapt = false;
        while steps.len() < count {
            let last = *steps.last().unwrap();
            let size = 1 + rng.below(max_leap as usize) as i32;
            let next = if rng.below(2) == 0 { last + size } else { last - size };
            if (0..=span).contains(&next) {
                leapt |= size > 1;
                steps.push(next);
            }
        }
        // A leap level must leap; the longer phrases come home at the end.
        if (level != 2 || leapt) && (level < 3 || steps.last() == Some(&0) || rng.below(3) == 0) {
            break steps;
        }
    };
    let starts: Vec<(i64, i64)> = match count {
        3 => vec![(0, 4), (4, 4), (8, 8)],
        4 if level < 4 => vec![(0, 4), (4, 4), (8, 4), (12, 4)],
        5 if level < 4 => vec![(0, 4), (4, 4), (8, 2), (10, 2), (12, 4)],
        _ => {
            let rhythms: [&[(i64, i64)]; 3] =
                [&[(0, 2), (2, 2), (4, 4), (8, 4), (12, 4)], &[(0, 3), (3, 3), (6, 2), (8, 4), (12, 4)], &[(0, 4), (4, 2), (6, 2), (8, 6), (14, 2)]];
            rng.pick(&rhythms).to_vec()
        }
    };
    let notes = steps.iter().zip(starts).map(|(&s, (at, len))| (at, pitch(s), len)).collect();
    Exercise { kind: Kind::Melody, level, notes }
}

/// The bar you play in (0-based): a count-in, the phrase, a count-in, you.
pub const YOUR_BAR: i64 = 3;
pub const BARS: i64 = 4;

/// The exercise as a song to play: four clicks, the phrase, four clicks,
/// then quieter clicks under your turn. A rhythm is clapped on the Drum
/// Kit; a melody plays on the Piano.
pub fn session(ex: &Exercise, bpm: f64) -> crate::project::Project {
    use crate::arrangement::{empty_arrangement, ClipColor, Instrument, MidiNote, TempoMap, TimeSignature, PPQ};
    use crate::lessons::{add_clip, add_track, SIXTEENTH};
    let mut arr = empty_arrangement();
    arr.tempo_map = TempoMap::constant(bpm, TimeSignature::FOUR_FOUR);
    let bar = 4 * PPQ;
    let click = |bar_index: i64, beat: i64, loud: bool| MidiNote {
        start: bar_index * bar + beat * PPQ,
        length: SIXTEENTH,
        pitch: crate::drums::CLOSED_HAT,
        velocity: match (loud, beat) {
            (true, 0) => 120,
            (true, _) => 95,
            (false, _) => 55,
        },
    };
    let mut drums: Vec<MidiNote> = Vec::new();
    for beat in 0..4 {
        drums.push(click(0, beat, true));
        drums.push(click(2, beat, true));
        drums.push(click(YOUR_BAR, beat, false));
    }
    let phrase = ex.notes.iter().map(|&(at, pitch, len)| MidiNote { start: bar + at * SIXTEENTH, length: len * SIXTEENTH, pitch, velocity: 110 });
    let mut instruments = Vec::new();
    match ex.kind {
        Kind::Rhythm => drums.extend(phrase),
        Kind::Melody => {
            let keys = add_track(&mut arr, "Piano", ClipColor::Violet, Instrument::Carve, 0.0);
            add_clip(&mut arr, keys, "Phrase", 0, BARS, BARS, phrase.collect());
            instruments.push((keys, crate::synth::recipes::piano()));
        }
    }
    let kit = add_track(&mut arr, "Click", ClipColor::Amber, Instrument::Drums, -4.0);
    add_clip(&mut arr, kit, "Click", 0, BARS, BARS, drums);
    crate::project::Project { arrangement: arr, instruments, synth: None }
}

/// Where each of the exercise's notes falls in your bar, in milliseconds.
pub fn targets_ms(ex: &Exercise, bpm: f64) -> Vec<f32> {
    let sixteenth_ms = 60_000.0 / bpm / 4.0;
    ex.notes.iter().map(|n| (n.0 as f64 * sixteenth_ms) as f32).collect()
}

/// How a hit landed, in milliseconds (negative: early).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    OnTime(f32),
    Early(f32),
    Late(f32),
    Missed,
}

/// Close enough to call on time, and the furthest still counted as the hit.
pub const ON_TIME_MS: f32 = 60.0;
const COUNTED_MS: f32 = 180.0;

/// Matches what was played (milliseconds from the bar's start) to the
/// rhythm's hits (likewise): each hit takes the nearest unused tap within
/// reach. Returns each hit's result and how many taps matched nothing.
pub fn score_rhythm(targets_ms: &[f32], played_ms: &[f32]) -> (Vec<Hit>, usize) {
    let mut used = vec![false; played_ms.len()];
    let hits = targets_ms
        .iter()
        .map(|&t| {
            let best = played_ms
                .iter()
                .enumerate()
                .filter(|(i, p)| !used[*i] && (*p - t).abs() <= COUNTED_MS)
                .min_by(|a, b| (a.1 - t).abs().total_cmp(&(b.1 - t).abs()));
            match best {
                Some((i, &p)) => {
                    used[i] = true;
                    let off = p - t;
                    if off.abs() <= ON_TIME_MS {
                        Hit::OnTime(off)
                    } else if off < 0.0 {
                        Hit::Early(off)
                    } else {
                        Hit::Late(off)
                    }
                }
                None => Hit::Missed,
            }
        })
        .collect();
    (hits, used.iter().filter(|u| !**u).count())
}

/// How each played note compares with the melody's, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteResult {
    Right,
    /// The right note in another octave.
    Octave,
    Wrong(u8),
    Missed,
}

pub fn score_melody(targets: &[u8], played: &[u8]) -> Vec<NoteResult> {
    targets
        .iter()
        .enumerate()
        .map(|(i, &t)| match played.get(i) {
            Some(&p) if p == t => NoteResult::Right,
            Some(&p) if p % 12 == t % 12 => NoteResult::Octave,
            Some(&p) => NoteResult::Wrong(p),
            None => NoteResult::Missed,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_MINOR_PENT: u16 = (1 << 0) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 10);

    #[test]
    fn rhythms_fit_their_level() {
        for seed in 1..200 {
            let quarters = rhythm(0, seed);
            assert!(quarters.notes.iter().all(|n| n.0 % 4 == 0) && quarters.notes[0].0 == 0, "{quarters:?}");
            let eighths = rhythm(1, seed);
            assert!(eighths.notes.iter().all(|n| n.0 % 2 == 0));
            let offbeats = rhythm(2, seed);
            assert!(offbeats.notes.iter().filter(|n| n.0 % 4 == 2).count() >= 2);
            for level in 0..LEVELS {
                let r = rhythm(level, seed);
                assert!(r.notes.len() >= 3 && r.notes.iter().all(|n| (0..16).contains(&n.0)), "{level}: {r:?}");
                assert!(r.notes.windows(2).all(|w| w[0].0 < w[1].0));
            }
        }
        assert_ne!(rhythm(3, 1), rhythm(3, 2));
    }

    #[test]
    fn melodies_stay_in_the_key_and_fit_their_level() {
        for seed in 1..200 {
            for level in 0..LEVELS {
                let m = melody(level, seed, 9, A_MINOR_PENT);
                assert!(m.notes.iter().all(|n| A_MINOR_PENT & (1 << ((n.1 as i32 - 9).rem_euclid(12))) != 0), "{m:?}");
                assert_eq!(m.notes[0].1 % 12, 9, "starts on the key's home note");
                assert!(m.notes.iter().map(|n| n.0 + n.2).max().unwrap() <= 16);
            }
            let by_step = melody(1, seed, 0, 0b1010_1011_0101);
            let pitches: Vec<i32> = by_step.notes.iter().map(|n| n.1 as i32).collect();
            assert!(pitches.windows(2).all(|w| (w[1] - w[0]).abs() <= 2), "{pitches:?}");
            assert_eq!(melody(0, seed, 0, 0).notes.len(), 3);
        }
    }

    #[test]
    fn taps_are_matched_to_the_nearest_hit() {
        let targets = [0.0, 500.0, 1000.0, 1500.0];
        let (hits, extra) = score_rhythm(&targets, &[10.0, 590.0, 880.0, 1200.0]);
        assert_eq!(hits[0], Hit::OnTime(10.0));
        assert_eq!(hits[1], Hit::Late(90.0));
        assert_eq!(hits[2], Hit::Early(-120.0));
        assert_eq!(hits[3], Hit::Missed);
        assert_eq!(extra, 1);
    }

    #[test]
    fn a_session_is_count_in_phrase_count_in_your_turn() {
        let ex = melody(1, 7, 0, 0b1010_1011_0101);
        let p = session(&ex, 80.0);
        assert_eq!(p.arrangement.tracks.len(), 2);
        assert_eq!(p.instruments.len(), 1);
        let bar = 4 * crate::arrangement::PPQ;
        let clicks = p.arrangement.clips.iter().find(|c| c.name == "Click").unwrap();
        let crate::arrangement::ClipContent::Midi { notes, .. } = &clicks.content else { panic!() };
        assert_eq!(notes.len(), 12);
        assert!(notes.iter().all(|n| n.start / bar != 1), "no clicks under the phrase");
        let r = session(&rhythm(0, 3), 80.0);
        assert_eq!(r.arrangement.tracks.len(), 1);
        assert_eq!(targets_ms(&rhythm(0, 3), 60.0)[0], 0.0);
        assert_eq!(targets_ms(&Exercise { kind: Kind::Rhythm, level: 0, notes: vec![(4, 39, 1)] }, 60.0), [1000.0]);
    }

    #[test]
    fn notes_are_right_wrong_or_an_octave_off() {
        let r = score_melody(&[60, 62, 64], &[60, 74, 65]);
        assert_eq!(r, [NoteResult::Right, NoteResult::Octave, NoteResult::Wrong(65)]);
        assert_eq!(score_melody(&[60, 62], &[60]), [NoteResult::Right, NoteResult::Missed]);
    }
}
