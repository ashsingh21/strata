//! The Drum Kit instrument's pad map: which MIDI note plays which sample.
//! Shared so the engine (which plays the samples), the UI (pad grid,
//! piano-roll row names) and the decoder (which loads the samples up
//! front) all agree. Notes follow the General MIDI drum map, so a beat
//! written here means the same thing in any other DAW.

pub struct DrumPad {
    pub note: u8,
    pub name: &'static str,
    /// Under the assets dir, same naming as audio clip sources.
    pub sample: &'static str,
    /// Playing this pad silences any still-ringing hit of these notes, the
    /// way a closed hi-hat cuts off an open one.
    pub chokes: &'static [u8],
}

pub const KICK: u8 = 36;
pub const SNARE: u8 = 38;
pub const CLAP: u8 = 39;
pub const CLOSED_HAT: u8 = 42;
pub const OPEN_HAT: u8 = 46;

/// In note order, low to high - the pad panel's left-to-right order and
/// the step grid's bottom-to-top. Only samples that are Strata's own (the
/// repo's tracked, generated one-shots) - never the downloaded packs,
/// which aren't ours to redistribute (see `.gitignore`).
pub const DRUM_KIT: [DrumPad; 5] = [
    DrumPad { note: KICK, name: "Kick", sample: "drums/kick.wav", chokes: &[] },
    DrumPad { note: SNARE, name: "Snare", sample: "drums/snare.wav", chokes: &[] },
    DrumPad { note: CLAP, name: "Clap", sample: "drums/clap.wav", chokes: &[] },
    DrumPad { note: CLOSED_HAT, name: "Closed Hat", sample: "drums/hihat_closed.wav", chokes: &[OPEN_HAT] },
    DrumPad { note: OPEN_HAT, name: "Open Hat", sample: "drums/hihat_open.wav", chokes: &[] },
];

/// One pad's own settings on a Drum Kit track (indexed like `DRUM_KIT`).
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PadSettings {
    /// Silent: new hits on this pad are ignored.
    pub mute: bool,
    /// Its level, in dB (0 = as sampled).
    pub gain_db: f32,
    /// Its tuning, in semitones (the sample plays faster or slower).
    pub pitch: f32,
}

/// Where `note`'s pad sits in `DRUM_KIT` (and so in a track's settings).
pub fn pad_index(note: u8) -> Option<usize> {
    DRUM_KIT.iter().position(|p| p.note == note)
}

pub fn pad_for_note(note: u8) -> Option<&'static DrumPad> {
    DRUM_KIT.iter().find(|p| p.note == note)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_are_unique_and_chokes_point_at_real_pads() {
        let mut notes: Vec<u8> = DRUM_KIT.iter().map(|p| p.note).collect();
        notes.sort();
        notes.dedup();
        assert_eq!(notes.len(), DRUM_KIT.len());
        for pad in &DRUM_KIT {
            for c in pad.chokes {
                assert!(pad_for_note(*c).is_some(), "{} chokes a missing note {c}", pad.name);
            }
        }
    }

    #[test]
    fn every_sample_is_tracked_in_the_repo() {
        // A kit sample that's gitignored works here but is missing from
        // every fresh clone (and may not be ours to ship).
        let ignore = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.gitignore")).unwrap();
        for pad in &DRUM_KIT {
            let file = format!("assets/{}", pad.sample);
            assert!(!ignore.lines().any(|l| l.trim() == file), "{file} is gitignored");
        }
    }

    #[test]
    fn every_sample_exists() {
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
        for pad in &DRUM_KIT {
            assert!(assets.join(pad.sample).exists(), "{} is missing", pad.sample);
        }
    }
}
