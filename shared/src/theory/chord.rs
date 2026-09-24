//! Names the chord formed by a set of currently-held notes: the lowest
//! held note is taken as the chord's root (no inversion detection - a
//! reasonable simplification for notes actually played by hand or tapped
//! on a pad), and the other notes' intervals above it are matched against
//! a table of common shapes.

use super::scale::{degree_name, note_name};

/// `(intervals above the root, suffix appended to the root's note name)`.
/// Checked in order; the first exact match (as a set) wins, so put more
/// specific/longer shapes first where they could otherwise be shadowed by
/// a shorter one that isn't actually a subset here (there are none, since
/// matching is exact-set, not subset).
const SHAPES: &[(&[u8], &str)] = &[
    (&[0, 4, 7, 11], "maj7"),
    (&[0, 4, 7, 10], "7"),
    (&[0, 3, 7, 10], "m7"),
    (&[0, 3, 6, 10], "m7b5"),
    (&[0, 3, 6, 9], "dim7"),
    (&[0, 4, 7, 9], "6"),
    (&[0, 3, 7, 9], "m6"),
    (&[0, 2, 7], "sus2"),
    (&[0, 5, 7], "sus4"),
    (&[0, 4, 7], ""),
    (&[0, 3, 7], "m"),
    (&[0, 3, 6], "dim"),
    (&[0, 4, 8], "aug"),
    (&[0, 7], "5"),
];

#[derive(Clone, Debug, PartialEq)]
pub struct ChordMatch {
    pub root_pitch_class: u8,
    /// e.g. "Am7".
    pub name: String,
    /// e.g. "1 b3 5 b7", intervals relative to the chord's own root.
    pub formula: String,
}

/// `held_notes` are absolute MIDI note numbers (as in `SynthState::held_notes`).
pub fn recognize(held_notes: &[u8]) -> Option<ChordMatch> {
    let root_note = *held_notes.iter().min()?;
    let root_pc = root_note % 12;

    // Safe: `root_note` is the minimum of `held_notes`, so every `n` here
    // is `>= root_note`.
    let mut intervals: Vec<u8> = held_notes.iter().map(|&n| (n - root_note) % 12).collect();
    intervals.sort_unstable();
    intervals.dedup();

    let shape = SHAPES.iter().find(|(shape, _)| shape_matches(shape, &intervals))?;
    let formula = intervals.iter().map(|&i| degree_name(i)).collect::<Vec<_>>().join(" ");
    Some(ChordMatch {
        root_pitch_class: root_pc,
        name: format!("{}{}", note_name(root_pc), shape.1),
        formula,
    })
}

fn shape_matches(shape: &[u8], intervals: &[u8]) -> bool {
    shape.len() == intervals.len() && shape.iter().all(|s| intervals.contains(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minor_seven_from_the_reference_example() {
        // A2, C3, E3, G3 - A minor pentatonic's 1, b3, 5, b7.
        let held = [45u8, 48, 52, 55];
        let m = recognize(&held).unwrap();
        assert_eq!(m.name, "Am7");
        assert_eq!(m.formula, "1 b3 5 b7");
    }

    #[test]
    fn major_triad_regardless_of_octave() {
        let held = [60u8, 76, 67]; // C4, E5, G4 - order/octave shouldn't matter.
        let m = recognize(&held).unwrap();
        assert_eq!(m.name, "C");
    }

    #[test]
    fn single_note_has_no_chord() {
        assert!(recognize(&[60]).is_none());
    }

    #[test]
    fn empty_has_no_chord() {
        assert!(recognize(&[]).is_none());
    }

    #[test]
    fn unmatched_interval_set_returns_none() {
        // A minor 2nd apart - not in the shape table.
        assert!(recognize(&[60, 61]).is_none());
    }

    #[test]
    fn root_is_always_the_lowest_held_note() {
        let held = [67u8, 60, 64]; // G4, C4, E4 -> root should be C.
        let m = recognize(&held).unwrap();
        assert_eq!(m.name, "C");
    }
}
