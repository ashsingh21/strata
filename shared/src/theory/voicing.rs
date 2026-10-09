//! Voice leading: chords built from the key's own notes, and a voicing
//! for each that moves its voices as little as possible from the chord
//! before - the way a choir or a pianist's hand would, rather than every
//! chord jumping to root position.

use super::scale::{degrees_in_mask, note_name_for_key};

/// A chord: its root and its tones, as pitch classes (0 = C), root first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chord {
    pub root: u8,
    pub tones: Vec<u8>,
}

/// The lowest and highest notes a voicing may use (E2 to C6).
const LOW: i32 = 40;
const HIGH: i32 = 84;
/// How far one voice may move to the next chord, in semitones.
const REACH: i32 = 7;
/// What a parallel fifth or octave between two voices costs, in semitones
/// of movement: enough to prefer a slightly longer way round.
const PARALLEL_COST: i32 = 3;

/// The chord on scale degree `degree` (0 = the first note of the scale):
/// every other scale note stacked up from it - three of them, or four for
/// a seventh. `None` unless the scale has seven notes (stacking every
/// other note of a pentatonic scale doesn't make thirds), or for no such
/// degree.
pub fn diatonic(degree: usize, key: u8, mask: u16, seventh: bool) -> Option<Chord> {
    let degrees = degrees_in_mask(mask);
    if degrees.len() != 7 || degree >= degrees.len() {
        return None;
    }
    let count = if seventh { 4 } else { 3 };
    let tones = (0..count).map(|i| (key + degrees[(degree + 2 * i) % degrees.len()]) % 12).collect::<Vec<_>>();
    Some(Chord { root: tones[0], tones })
}

pub const MAJOR: u16 = 0b1010_1011_0101;
pub const MINOR: u16 = 0b0101_1010_1101;

/// The scale to build chords from: a seven-note scale as it is; any other
/// (a pentatonic) as the major or minor scale it sits in - minor when it
/// has a minor third and no major one.
pub fn seven_notes(mask: u16) -> u16 {
    if degrees_in_mask(mask).len() == 7 {
        mask
    } else if mask & (1 << 3) != 0 && mask & (1 << 4) == 0 {
        MINOR
    } else {
        MAJOR
    }
}

/// "Am", "G7", "Bdim", "Cmaj7" - or the root and its notes when the shape
/// has no common name. `key` picks sharps or flats.
pub fn name(chord: &Chord, key: u8) -> String {
    let mut steps: Vec<u8> = chord.tones.iter().skip(1).map(|&t| (t + 12 - chord.root) % 12).collect();
    steps.sort_unstable();
    let quality = match steps.as_slice() {
        [4, 7] => "",
        [3, 7] => "m",
        [3, 6] => "dim",
        [4, 8] => "aug",
        [4, 7, 11] => "maj7",
        [4, 7, 10] => "7",
        [3, 7, 10] => "m7",
        [3, 6, 10] => "m7\u{266d}5",
        [3, 6, 9] => "dim7",
        [3, 7, 11] => "m(maj7)",
        _ => "?",
    };
    format!("{}{quality}", note_name_for_key(chord.root, key))
}

/// The roman numeral of `chord` in `key`: upper case for a major third,
/// lower for minor, ° for diminished.
pub fn numeral(degree: usize, chord: &Chord) -> String {
    const NUMERALS: [&str; 7] = ["I", "II", "III", "IV", "V", "VI", "VII"];
    let base = NUMERALS.get(degree).copied().unwrap_or("?");
    let third = chord.tones.get(1).map(|&t| (t + 12 - chord.root) % 12);
    let fifth = chord.tones.get(2).map(|&t| (t + 12 - chord.root) % 12);
    match (third, fifth) {
        (Some(4), _) => base.to_string(),
        (Some(3), Some(6)) => format!("{}\u{b0}", base.to_lowercase()),
        _ => base.to_lowercase(),
    }
}

/// A voicing for each chord, `voices` notes each (3 or 4), low to high.
/// The first is close position from the root around middle C; each after
/// it is the one whose voices move least from the chord before (no voice
/// crossing, every chord tone present, parallel fifths and octaves
/// discouraged).
pub fn smooth(chords: &[Chord], voices: usize) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    for chord in chords {
        let next = match out.last() {
            None => close_position(chord, voices),
            Some(prev) => nearest(prev, chord, voices).unwrap_or_else(|| close_position(chord, voices)),
        };
        out.push(next);
    }
    out
}

/// Each voice's move from `a` to `b`, in semitones (positive is up).
pub fn movement(a: &[u8], b: &[u8]) -> Vec<i32> {
    a.iter().zip(b).map(|(&x, &y)| y as i32 - x as i32).collect()
}

/// Root position, stacked close from the root at or above G3; a fourth
/// voice on a triad doubles the root an octave up, and three voices on a
/// seventh leave out the fifth.
fn close_position(chord: &Chord, voices: usize) -> Vec<u8> {
    let mut pitch = 48 + chord.root as i32;
    if pitch < 55 {
        pitch += 12;
    }
    let tones = required(chord, voices);
    let mut notes = vec![pitch];
    for i in 1..voices {
        let pc = tones[i % tones.len()] as i32;
        let mut next = pitch - pitch.rem_euclid(12) + pc;
        while next <= pitch {
            next += 12;
        }
        notes.push(next);
        pitch = next;
    }
    notes.into_iter().map(|p| p as u8).collect()
}

/// The voicing of `chord` closest to `prev`, if one is in reach.
fn nearest(prev: &[u8], chord: &Chord, voices: usize) -> Option<Vec<u8>> {
    let options: Vec<Vec<i32>> = prev
        .iter()
        .map(|&p| {
            let p = p as i32;
            ((p - REACH).max(LOW)..=(p + REACH).min(HIGH)).filter(|n| chord.tones.contains(&(n.rem_euclid(12) as u8))).collect()
        })
        .collect();
    let mut best: Option<(i32, Vec<i32>)> = None;
    let mut pick = vec![0i32; voices];
    search(prev, chord, &options, 0, &mut pick, &mut best);
    best.map(|(_, notes)| notes.into_iter().map(|n| n as u8).collect())
}

fn search(prev: &[u8], chord: &Chord, options: &[Vec<i32>], voice: usize, pick: &mut Vec<i32>, best: &mut Option<(i32, Vec<i32>)>) {
    if voice == options.len() {
        if !required(chord, pick.len()).iter().all(|t| pick.iter().any(|n| n.rem_euclid(12) as u8 == *t)) {
            return;
        }
        let cost = cost(prev, pick);
        if best.as_ref().is_none_or(|(c, _)| cost < *c) {
            *best = Some((cost, pick.clone()));
        }
        return;
    }
    for &n in &options[voice] {
        // Low to high, no two voices on one note.
        if voice > 0 && n <= pick[voice - 1] {
            continue;
        }
        pick[voice] = n;
        search(prev, chord, options, voice + 1, pick, best);
    }
}

/// The tones a voicing of `voices` notes must have: all of them, or -
/// three voices on a seventh chord - all but the fifth, which adds least.
fn required(chord: &Chord, voices: usize) -> Vec<u8> {
    if voices >= chord.tones.len() {
        chord.tones.clone()
    } else {
        chord.tones.iter().enumerate().filter(|(i, _)| *i != 2).map(|(_, t)| *t).take(voices).collect()
    }
}

/// Total movement, plus a penalty for each pair of voices moving in
/// parallel fifths or octaves.
fn cost(prev: &[u8], next: &[i32]) -> i32 {
    let moved: i32 = prev.iter().zip(next).map(|(&a, &b)| (b - a as i32).abs()).sum();
    let mut parallels = 0;
    for i in 0..next.len() {
        for j in i + 1..next.len() {
            let before = (prev[j] as i32 - prev[i] as i32).rem_euclid(12);
            let after = (next[j] - next[i]).rem_euclid(12);
            let moved = next[i] != prev[i] as i32;
            if moved && before == after && (after == 7 || after == 0) {
                parallels += 1;
            }
        }
    }
    moved + parallels * PARALLEL_COST
}

#[cfg(test)]
mod tests {
    use super::*;

    const C_MAJOR: u16 = 0b1010_1011_0101;

    fn chords(degrees: &[usize], seventh: bool) -> Vec<Chord> {
        degrees.iter().map(|&d| diatonic(d, 0, C_MAJOR, seventh).unwrap()).collect()
    }

    #[test]
    fn the_key_gives_its_own_chords() {
        let names: Vec<String> = (0..7).map(|d| name(&diatonic(d, 0, C_MAJOR, false).unwrap(), 0)).collect();
        assert_eq!(names, ["C", "Dm", "Em", "F", "G", "Am", "Bdim"]);
        let sevenths: Vec<String> = (0..7).map(|d| name(&diatonic(d, 0, C_MAJOR, true).unwrap(), 0)).collect();
        assert_eq!(sevenths, ["Cmaj7", "Dm7", "Em7", "Fmaj7", "G7", "Am7", "Bm7\u{266d}5"]);
        assert_eq!(numeral(6, &diatonic(6, 0, C_MAJOR, false).unwrap()), "vii\u{b0}");
        assert_eq!(numeral(1, &diatonic(1, 0, C_MAJOR, false).unwrap()), "ii");
        // A pentatonic scale has no stacked thirds to build from.
        assert!(diatonic(0, 9, 0b0100_1010_1001, false).is_none());
        // ... so it borrows the minor scale's.
        assert_eq!(seven_notes(0b0100_1010_1001), MINOR);
        assert_eq!(seven_notes(0b0010_1001_0101), MAJOR);
        assert_eq!(seven_notes(C_MAJOR), C_MAJOR);
    }

    #[test]
    fn common_tones_stay_and_the_rest_move_by_step() {
        // C Am F G: C and E stay into Am, C and A into F.
        let v = smooth(&chords(&[0, 5, 3, 4], false), 3);
        assert_eq!(v[0], [60, 64, 67]);
        assert_eq!(v[1], [60, 64, 69]);
        assert_eq!(v[2], [60, 65, 69]);
        for pair in v.windows(2) {
            let moved: i32 = movement(&pair[0], &pair[1]).iter().map(|m| m.abs()).sum();
            assert!(moved <= 6, "{pair:?}");
        }
    }

    #[test]
    fn every_voicing_holds_its_chord_low_to_high() {
        let progression = chords(&[1, 4, 0, 5, 3, 6, 2, 0], true);
        for voices in [3, 4] {
            let v = smooth(&progression, voices);
            for (notes, chord) in v.iter().zip(&progression) {
                assert_eq!(notes.len(), voices);
                assert!(notes.windows(2).all(|w| w[0] < w[1]), "{notes:?}");
                assert!(notes.iter().all(|&n| (LOW..=HIGH).contains(&(n as i32))));
                let needed = if voices == 3 { 3 } else { 4 };
                let present = chord.tones.iter().filter(|t| notes.iter().any(|n| n % 12 == **t)).count();
                assert!(present >= needed.min(chord.tones.len()).min(voices), "{notes:?} for {chord:?}");
            }
        }
    }

    #[test]
    fn four_voices_on_a_triad_double_a_tone() {
        let v = smooth(&chords(&[0, 4], false), 4);
        assert_eq!(v[0], [60, 64, 67, 72]);
        assert_eq!(v[1].len(), 4);
    }
}
