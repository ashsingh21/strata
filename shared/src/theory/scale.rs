//! Chromatic pitch-class naming and scale definitions: which of the 12
//! semitones above a root are "in scale", as a bitmask (bit `n` set means
//! semitone `n` above the root is included; bit 0 - the root itself - is
//! always set for every preset here).

/// Standard interval name for each semitone offset from a root (0..12).
pub const DEGREE_NAMES: [&str; 12] =
    ["1", "b2", "2", "b3", "3", "4", "#4", "5", "b6", "6", "b7", "7"];

/// Note name for each pitch class (0..12), always spelled with sharps -
/// simplest consistent choice; this tool doesn't attempt correct
/// enharmonic spelling per key.
pub const NOTE_NAMES: [&str; 12] =
    ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Same pitch classes, spelled with flats instead.
pub const NOTE_NAMES_FLAT: [&str; 12] =
    ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"];

pub fn note_name(pitch_class: u8) -> &'static str {
    NOTE_NAMES[(pitch_class % 12) as usize]
}

/// Whether `root` (a pitch class, 0..12) conventionally spells its own
/// scale with flats rather than sharps - the standard circle-of-fifths
/// choice (F, Bb, Eb, Ab, Db; everything else, including the
/// enharmonically ambiguous F#/Gb, goes with sharps).
fn prefers_flats(root: u8) -> bool {
    matches!(root % 12, 1 | 3 | 5 | 8 | 10)
}

/// Note name for `pitch_class`, spelled with sharps or flats depending on
/// which convention `root` itself uses - unlike `note_name`, which always
/// picks sharps regardless of key. Meant for displaying a whole scale's
/// notes together (e.g. Interval Input's note-name labels), where showing
/// "Db" for an Eb-major-ish scale reads far more naturally than "C#".
pub fn note_name_for_key(pitch_class: u8, root: u8) -> &'static str {
    if prefers_flats(root) {
        NOTE_NAMES_FLAT[(pitch_class % 12) as usize]
    } else {
        NOTE_NAMES[(pitch_class % 12) as usize]
    }
}

pub fn degree_name(semitones_from_root: u8) -> &'static str {
    DEGREE_NAMES[(semitones_from_root % 12) as usize]
}

/// The note `semitones_from_root` above Sa, in sargam: shuddh (natural)
/// notes capitalised, komal (flat) ones lower case - re, ga, dha, ni - and
/// tivra (sharp) Ma capitalised against shuddh ma.
pub fn sargam_name(semitones_from_root: u8) -> &'static str {
    const NAMES: [&str; 12] = ["Sa", "re", "Re", "ga", "Ga", "ma", "Ma", "Pa", "dha", "Dha", "ni", "Ni"];
    NAMES[(semitones_from_root % 12) as usize]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScalePreset {
    pub name: &'static str,
    /// Bit `n` set means semitone `n` above the root is in the scale.
    pub mask: u16,
}

const fn mask(degrees: &[u8]) -> u16 {
    let mut m = 0u16;
    let mut i = 0;
    while i < degrees.len() {
        m |= 1 << degrees[i];
        i += 1;
    }
    m
}

pub const SCALE_PRESETS: &[ScalePreset] = &[
    ScalePreset { name: "Major", mask: mask(&[0, 2, 4, 5, 7, 9, 11]) },
    ScalePreset { name: "Natural minor", mask: mask(&[0, 2, 3, 5, 7, 8, 10]) },
    ScalePreset { name: "Major pentatonic", mask: mask(&[0, 2, 4, 7, 9]) },
    ScalePreset { name: "Minor pentatonic", mask: mask(&[0, 3, 5, 7, 10]) },
    ScalePreset { name: "Blues", mask: mask(&[0, 3, 5, 6, 7, 10]) },
    ScalePreset { name: "Dorian", mask: mask(&[0, 2, 3, 5, 7, 9, 10]) },
    ScalePreset { name: "Phrygian", mask: mask(&[0, 1, 3, 5, 7, 8, 10]) },
    ScalePreset { name: "Lydian", mask: mask(&[0, 2, 4, 6, 7, 9, 11]) },
    ScalePreset { name: "Mixolydian", mask: mask(&[0, 2, 4, 5, 7, 9, 10]) },
    ScalePreset { name: "Locrian", mask: mask(&[0, 1, 3, 5, 6, 8, 10]) },
    ScalePreset { name: "Harmonic minor", mask: mask(&[0, 2, 3, 5, 7, 8, 11]) },
    ScalePreset { name: "Chromatic", mask: mask(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]) },
    // Hindustani ragas - their note SET only (the "that"/parent scale a
    // raga draws from), as a bitmask like everything else here. Real raga
    // grammar (arohana/avarohana asymmetry, vadi/samvadi, vakra movement,
    // gamak ornamentation) is far beyond what a note-set mask can express;
    // this just gets the right notes highlighted to play/explore with.
    // Several ragas (Bilawal, Khamaj, Kafi, Asavari, Bhairavi, Kalyan,
    // Bhoopali) share an exact note set with a Western mode already above
    // and so aren't duplicated here.
    ScalePreset { name: "Raga Bhairav", mask: mask(&[0, 1, 4, 5, 7, 8, 11]) },
    ScalePreset { name: "Raga Todi", mask: mask(&[0, 1, 3, 6, 7, 8, 11]) },
    ScalePreset { name: "Raga Marwa", mask: mask(&[0, 1, 4, 6, 7, 9, 11]) },
    ScalePreset { name: "Raga Purvi", mask: mask(&[0, 1, 4, 6, 7, 8, 11]) },
    ScalePreset { name: "Raga Malkauns", mask: mask(&[0, 3, 5, 8, 10]) },
    ScalePreset { name: "Raga Hindol", mask: mask(&[0, 4, 6, 9, 11]) },
];

/// Semitone offsets (0..12, ascending, always including 0) that are set in
/// `mask`.
pub fn degrees_in_mask(mask: u16) -> Vec<u8> {
    (0..12u8).filter(|&d| mask & (1 << d) != 0).collect()
}

/// Semitone gaps between consecutive in-scale degrees, wrapping the last
/// gap back to the root an octave up (so a 5-note scale yields 5 gaps that
/// sum to 12).
pub fn gaps_in_mask(mask: u16) -> Vec<u8> {
    let degrees = degrees_in_mask(mask);
    if degrees.is_empty() {
        return Vec::new();
    }
    degrees
        .iter()
        .zip(degrees.iter().skip(1).chain(std::iter::once(&12)))
        .map(|(&a, &b)| b - a)
        .collect()
}

/// `pitch` moved `steps` notes along the scale (`key` + `mask`) - up if
/// positive. An off-scale pitch lands on the nearest scale note that way
/// first. `None` if that leaves MIDI's 0..=127 (or the mask is empty).
pub fn scale_step(pitch: u8, key: u8, mask: u16, steps: i32) -> Option<u8> {
    let degrees = degrees_in_mask(mask);
    if degrees.is_empty() {
        return None;
    }
    let in_scale = |p: i32| degrees.contains(&(((p - key as i32).rem_euclid(12)) as u8));
    let dir = steps.signum();
    let mut p = pitch as i32;
    for _ in 0..steps.abs() {
        p += dir;
        while (0..=127).contains(&p) && !in_scale(p) {
            p += dir;
        }
    }
    u8::try_from(p).ok().filter(|p| *p <= 127)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minor_pentatonic_has_five_degrees() {
        let preset = SCALE_PRESETS.iter().find(|p| p.name == "Minor pentatonic").unwrap();
        assert_eq!(degrees_in_mask(preset.mask), vec![0, 3, 5, 7, 10]);
    }

    #[test]
    fn gaps_sum_to_an_octave() {
        for preset in SCALE_PRESETS {
            let gaps = gaps_in_mask(preset.mask);
            assert_eq!(gaps.iter().map(|&g| g as u32).sum::<u32>(), 12, "{}", preset.name);
        }
    }

    #[test]
    fn every_preset_includes_the_root() {
        for preset in SCALE_PRESETS {
            assert_eq!(preset.mask & 1, 1, "{} is missing its root", preset.name);
        }
    }

    #[test]
    fn note_name_wraps_at_the_octave() {
        assert_eq!(note_name(9), "A");
        assert_eq!(note_name(21), "A");
    }

    #[test]
    fn raga_bhairav_has_the_expected_degrees() {
        let preset = SCALE_PRESETS.iter().find(|p| p.name == "Raga Bhairav").unwrap();
        assert_eq!(degrees_in_mask(preset.mask), vec![0, 1, 4, 5, 7, 8, 11]);
    }

    #[test]
    fn every_preset_has_a_unique_name() {
        let mut names: Vec<&str> = SCALE_PRESETS.iter().map(|p| p.name).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before, "duplicate preset name");
    }

    #[test]
    fn note_name_for_key_follows_the_root_convention() {
        // F, Bb, Eb, Ab, Db roots spell their own scale with flats.
        assert_eq!(note_name_for_key(1, 5), "Db"); // in F, C# reads as Db
        assert_eq!(note_name_for_key(8, 10), "Ab"); // in Bb, G# reads as Ab
        // Everything else, including the ambiguous F#/Gb root itself,
        // keeps sharps.
        assert_eq!(note_name_for_key(1, 0), "C#"); // in C
        assert_eq!(note_name_for_key(6, 6), "F#"); // root F# spells itself F#, not Gb
    }

    #[test]
    fn scale_steps_stay_in_key() {
        let a_minor_pent = mask(&[0, 3, 5, 7, 10]);
        // A3 up one step is C4, down one is G3.
        assert_eq!(scale_step(57, 9, a_minor_pent, 1), Some(60));
        assert_eq!(scale_step(57, 9, a_minor_pent, -1), Some(55));
        // Five steps is an octave.
        assert_eq!(scale_step(57, 9, a_minor_pent, 5), Some(69));
        // Off-scale B3 goes to the next scale note in that direction.
        assert_eq!(scale_step(59, 9, a_minor_pent, 1), Some(60));
        assert_eq!(scale_step(59, 9, a_minor_pent, -1), Some(57));
        assert_eq!(scale_step(127, 0, mask(&[0, 2, 4, 5, 7, 9, 11]), 1), None);
    }
}
