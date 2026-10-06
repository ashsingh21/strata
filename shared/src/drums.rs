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

/// The tabla's bols, one per pad. The bayan (left hand, the bass drum)
/// first, then the dayan (right hand, tuned to Sa), then the two together.
/// Ke, the bayan pressed flat, stops its ringing Ge.
pub const TABLA: [DrumPad; 8] = [
    DrumPad { note: 36, name: "Ge", sample: "drums/tabla/ge.wav", chokes: &[] },
    DrumPad { note: 37, name: "Ke", sample: "drums/tabla/ke.wav", chokes: &[36, 42, 43] },
    DrumPad { note: 38, name: "Na", sample: "drums/tabla/na.wav", chokes: &[] },
    DrumPad { note: 39, name: "Tin", sample: "drums/tabla/tin.wav", chokes: &[] },
    DrumPad { note: 40, name: "Tun", sample: "drums/tabla/tun.wav", chokes: &[] },
    DrumPad { note: 41, name: "Te", sample: "drums/tabla/te.wav", chokes: &[] },
    DrumPad { note: 42, name: "Dha", sample: "drums/tabla/dha.wav", chokes: &[] },
    DrumPad { note: 43, name: "Dhin", sample: "drums/tabla/dhin.wav", chokes: &[] },
];

/// Chop: eight empty pads, filled by dropping a loop on the kit (cut into
/// eight slices) or a sample on a pad.
pub const CHOP: [DrumPad; 8] = [
    DrumPad { note: 36, name: "1", sample: "", chokes: &[] },
    DrumPad { note: 37, name: "2", sample: "", chokes: &[] },
    DrumPad { note: 38, name: "3", sample: "", chokes: &[] },
    DrumPad { note: 39, name: "4", sample: "", chokes: &[] },
    DrumPad { note: 40, name: "5", sample: "", chokes: &[] },
    DrumPad { note: 41, name: "6", sample: "", chokes: &[] },
    DrumPad { note: 42, name: "7", sample: "", chokes: &[] },
    DrumPad { note: 43, name: "8", sample: "", chokes: &[] },
];

/// The most pads a kit has (a track keeps settings for this many).
pub const MAX_PADS: usize = 8;

/// Which set of sounds a Drum Kit track plays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Kit {
    #[default]
    Standard,
    Tabla,
    Chop,
}

impl Kit {
    pub const ALL: [Kit; 3] = [Kit::Standard, Kit::Tabla, Kit::Chop];

    pub fn name(self) -> &'static str {
        match self {
            Kit::Standard => "Standard",
            Kit::Tabla => "Tabla",
            Kit::Chop => "Chop",
        }
    }

    /// Its pads, low note to high.
    pub fn pads(self) -> &'static [DrumPad] {
        match self {
            Kit::Standard => &DRUM_KIT,
            Kit::Tabla => &TABLA,
            Kit::Chop => &CHOP,
        }
    }

    pub fn pad_index(self, note: u8) -> Option<usize> {
        self.pads().iter().position(|p| p.note == note)
    }

    pub fn pad_for_note(self, note: u8) -> Option<&'static DrumPad> {
        self.pads().iter().find(|p| p.note == note)
    }
}

/// Every kit's own samples, to load up front.
pub fn all_kit_samples() -> impl Iterator<Item = &'static str> {
    Kit::ALL.into_iter().flat_map(|k| k.pads().iter().map(|p| p.sample)).filter(|s| !s.is_empty())
}

/// One pad's own settings on a Drum Kit track (indexed like its kit's pads).
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
pub struct PadSettings {
    /// Silent: new hits on this pad are ignored.
    pub mute: bool,
    /// Its level, in dB (0 = as sampled).
    pub gain_db: f32,
    /// Its tuning, in semitones (the sample plays faster or slower).
    pub pitch: f32,
    /// A sample of your own in place of the kit's (a name like an audio
    /// clip's source); `None` plays the kit's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample: Option<&'static str>,
    /// Only this part of the sample, as fractions of its length (start,
    /// end) - a slice of a chopped loop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slice: Option<(f32, f32)>,
}

/// A pad as saved: its sample name read as text (and then interned).
#[derive(serde::Deserialize)]
struct PadSaved {
    #[serde(default)]
    mute: bool,
    #[serde(default)]
    gain_db: f32,
    #[serde(default)]
    pitch: f32,
    #[serde(default)]
    sample: Option<String>,
    #[serde(default)]
    slice: Option<(f32, f32)>,
}

// By hand: a derived one ties the `&'static str` to the input's lifetime.
impl<'de> serde::Deserialize<'de> for PadSettings {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let p = PadSaved::deserialize(d)?;
        Ok(PadSettings { mute: p.mute, gain_db: p.gain_db, pitch: p.pitch, sample: p.sample.as_deref().map(intern), slice: p.slice })
    }
}

impl PadSettings {
    /// What this pad plays in `kit` at `index`.
    pub fn sample_in(&self, kit: Kit, index: usize) -> Option<&'static str> {
        self.sample.or_else(|| kit.pads().get(index).map(|p| p.sample)).filter(|s| !s.is_empty())
    }
}

/// A Drum Kit track's kit and its pads' settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(from = "PadsSaved")]
pub struct Pads {
    #[serde(default)]
    pub kit: Kit,
    pub pads: [PadSettings; MAX_PADS],
}

impl Pads {
    pub fn get(&self, index: usize) -> Option<&PadSettings> {
        self.pads.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut PadSettings> {
        self.pads.get_mut(index)
    }

    /// Every sample these pads play (their kit's and any of your own).
    pub fn samples(&self) -> impl Iterator<Item = &'static str> + '_ {
        (0..self.kit.pads().len()).filter_map(move |i| self.pads[i].sample_in(self.kit, i))
    }
}

/// How pads were saved: now kit and settings, before that just the five
/// standard pads' settings as a list.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum PadsSaved {
    Now { #[serde(default)] kit: Kit, pads: Vec<PadSettings> },
    Before(Vec<PadSettings>),
}

impl From<PadsSaved> for Pads {
    fn from(saved: PadsSaved) -> Self {
        let (kit, list) = match saved {
            PadsSaved::Now { kit, pads } => (kit, pads),
            PadsSaved::Before(pads) => (Kit::Standard, pads),
        };
        let mut pads = [PadSettings::default(); MAX_PADS];
        for (slot, p) in pads.iter_mut().zip(list) {
            *slot = p;
        }
        Pads { kit, pads }
    }
}

/// A pad's own sample name, kept as a `&'static str` (interned once per
/// distinct name) so pad settings stay `Copy` - they cross to the audio
/// thread.
mod interned {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};

    pub fn intern(name: &str) -> &'static str {
        static NAMES: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
        let mut names = NAMES.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
        if let Some(&known) = names.get(name) {
            return known;
        }
        let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
        names.insert(leaked);
        leaked
    }
}

pub use interned::intern;

/// Where to cut a loop into `count` slices, as fractions of its length:
/// even divisions, each moved to the nearest hit (a jump in loudness)
/// within 60 ms, so a slice starts on its drum rather than just before.
/// `mono` is the loop's samples at `rate`.
pub fn chop_points(mono: &[f32], rate: u32, count: usize) -> Vec<(f32, f32)> {
    let len = mono.len().max(1);
    // Loudness in 5 ms frames, and how much each frame rose over the last.
    let frame = (rate as usize / 200).max(1);
    let energy: Vec<f32> = mono.chunks(frame).map(|c| c.iter().map(|x| x * x).sum::<f32>() / c.len() as f32).collect();
    let rise = |i: usize| if i == 0 { energy[0] } else { (energy[i] - energy[i - 1]).max(0.0) };
    let reach = (rate as f32 * 0.06) as usize / frame;
    let mut starts: Vec<usize> = (0..count)
        .map(|k| {
            let even = k * len / count;
            if k == 0 || energy.is_empty() {
                return even;
            }
            let centre = even / frame;
            let lo = centre.saturating_sub(reach);
            let hi = (centre + reach).min(energy.len() - 1);
            let best = (lo..=hi).max_by(|&a, &b| rise(a).total_cmp(&rise(b))).unwrap_or(centre);
            // Only move to a real hit; on a smooth sound stay on the grid.
            if rise(best) > 1.0e-6 { best * frame } else { even }
        })
        .collect();
    starts.dedup();
    starts.sort_unstable();
    (0..starts.len())
        .map(|i| {
            let end = starts.get(i + 1).copied().unwrap_or(len);
            (starts[i] as f32 / len as f32, end as f32 / len as f32)
        })
        .collect()
}

/// Where `note`'s pad sits in the standard kit.
pub fn pad_index(note: u8) -> Option<usize> {
    Kit::Standard.pad_index(note)
}

pub fn pad_for_note(note: u8) -> Option<&'static DrumPad> {
    Kit::Standard.pad_for_note(note)
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
        for sample in all_kit_samples() {
            assert!(assets.join(sample).exists(), "{sample} is missing");
        }
    }

    #[test]
    fn each_kit_fits_and_its_chokes_are_its_own() {
        for kit in Kit::ALL {
            assert!(kit.pads().len() <= MAX_PADS);
            for pad in kit.pads() {
                for c in pad.chokes {
                    assert!(kit.pad_for_note(*c).is_some(), "{}: {} chokes a missing note {c}", kit.name(), pad.name);
                }
            }
        }
    }

    #[test]
    fn a_loop_is_chopped_on_its_hits() {
        // Four hits, the third 20 ms late: the third slice starts on it.
        let rate = 1000;
        let mut mono = vec![0.0f32; 4000];
        for &at in &[0usize, 1000, 2020, 3000] {
            for x in &mut mono[at..at + 200] {
                *x = 0.8;
            }
        }
        let slices = chop_points(&mono, rate, 4);
        assert_eq!(slices.len(), 4);
        assert_eq!(slices[0].0, 0.0);
        assert_eq!(slices[3].1, 1.0);
        assert!((slices[2].0 - 0.505).abs() < 0.003, "{slices:?}");
        assert!(slices.windows(2).all(|w| w[0].1 == w[1].0));
        // Silence: even slices.
        let even = chop_points(&vec![0.0; 800], 1000, 8);
        assert!((even[1].0 - 0.125).abs() < 1e-6);
    }

    #[test]
    fn old_projects_pads_still_load_and_new_ones_round_trip() {
        // Before kits: a list of the five standard pads.
        let old = r#"[{"mute":true,"gain_db":-3.0,"pitch":0.0},{"mute":false,"gain_db":0.0,"pitch":2.0},{"mute":false,"gain_db":0.0,"pitch":0.0},{"mute":false,"gain_db":0.0,"pitch":0.0},{"mute":false,"gain_db":0.0,"pitch":0.0}]"#;
        let pads: Pads = serde_json::from_str(old).unwrap();
        assert_eq!(pads.kit, Kit::Standard);
        assert!(pads.pads[0].mute && pads.pads[1].pitch == 2.0 && pads.pads[7] == PadSettings::default());
        let mut tabla = Pads { kit: Kit::Tabla, ..Pads::default() };
        tabla.pads[2].sample = Some(intern("drums/clap.wav"));
        tabla.pads[2].slice = Some((0.25, 0.5));
        let back: Pads = serde_json::from_str(&serde_json::to_string(&tabla).unwrap()).unwrap();
        assert_eq!(back, tabla);
        assert_eq!(back.pads[2].sample_in(Kit::Tabla, 2), Some("drums/clap.wav"));
        assert_eq!(back.pads[3].sample_in(Kit::Tabla, 3), Some("drums/tabla/tin.wav"));
        assert!(back.samples().any(|s| s == "drums/clap.wav"));
    }
}
