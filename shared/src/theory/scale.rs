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

pub fn note_name(pitch_class: u8) -> &'static str {
    NOTE_NAMES[(pitch_class % 12) as usize]
}

pub fn degree_name(semitones_from_root: u8) -> &'static str {
    DEGREE_NAMES[(semitones_from_root % 12) as usize]
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
}
