//! "Fits key": which notes a pitched sample plays, and what key it's in -
//! heard, not read from its name (no sample here carries a key). A
//! chroma profile - the energy at each of the 12 note names, across the
//! whole file - then the Krumhansl-Kessler key profiles for its key.
//! Drum hits and noise have no clear notes, so they get no key and are
//! never filtered out: they fit any key.

use std::f32::consts::PI;

pub const NOTE_NAMES: [&str; 12] = ["C", "C\u{266f}", "D", "E\u{266d}", "E", "F", "F\u{266f}", "G", "A\u{266d}", "A", "B\u{266d}", "B"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SampleKey {
    /// The key's root (0 = C).
    pub root: u8,
    pub minor: bool,
    /// The note names it plays (bit 0 = C).
    pub notes: u16,
}

impl SampleKey {
    /// "A min", "F♯ maj".
    pub fn label(&self) -> String {
        format!("{} {}", NOTE_NAMES[self.root as usize % 12], if self.minor { "min" } else { "maj" })
    }

    /// Every note it plays is in the project's scale (`mask` relative to
    /// `root`).
    pub fn fits(&self, root: u8, mask: u16) -> bool {
        (0..12u8).filter(|pc| self.notes & (1 << pc) != 0).all(|pc| mask & (1 << ((pc + 12 - root % 12) % 12)) != 0)
    }
}

/// Krumhansl-Kessler major and minor key profiles (C).
const MAJOR: [f32; 12] = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
const MINOR: [f32; 12] = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];

const FFT: usize = 4096;
/// The range notes are read from: below it is rumble, above it mostly
/// overtones.
const LOW_HZ: f32 = 55.0;
const HIGH_HZ: f32 = 2000.0;
/// At most this much of a file is analysed.
const MAX_SECONDS: f32 = 20.0;

/// The energy at each note name across `mono`.
pub fn chroma(mono: &[f32], sample_rate: u32) -> [f32; 12] {
    let sr = sample_rate as f32;
    let end = mono.len().min((MAX_SECONDS * sr) as usize);
    let window: Vec<f32> = (0..FFT).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / FFT as f32).cos()).collect();
    let bin_hz = sr / FFT as f32;
    let mut chroma = [0.0f32; 12];
    let mut at = 0;
    while at + FFT <= end {
        let mut re: Vec<f32> = (0..FFT).map(|i| mono[at + i] * window[i]).collect();
        let mut im = vec![0.0; FFT];
        shared::analysis::fft(&mut re, &mut im);
        let mag: Vec<f32> = (0..FFT / 2).map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt()).collect();
        // Only spectral peaks count: a note's energy leaks into the bins
        // either side, which at low pitches are a semitone away.
        for k in ((LOW_HZ / bin_hz) as usize).max(1)..((HIGH_HZ / bin_hz) as usize).min(FFT / 2 - 1) {
            if mag[k] < mag[k - 1] || mag[k] < mag[k + 1] {
                continue;
            }
            // The peak's true frequency, between bins (parabolic fit).
            let (a, b, c) = (mag[k - 1], mag[k], mag[k + 1]);
            let offset = 0.5 * (a - c) / (a - 2.0 * b + c).min(-1e-9);
            let hz = (k as f32 + offset) * bin_hz;
            let midi = 69.0 + 12.0 * (hz / 440.0).log2();
            let pc = (midi.round() as i32).rem_euclid(12) as usize;
            chroma[pc] += b;
        }
        at += FFT / 2;
    }
    chroma
}

fn correlation(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let (ma, mb) = (a.iter().sum::<f32>() / 12.0, b.iter().sum::<f32>() / 12.0);
    let cov: f32 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let va: f32 = a.iter().map(|x| (x - ma).powi(2)).sum();
    let vb: f32 = b.iter().map(|y| (y - mb).powi(2)).sum();
    cov / (va * vb).sqrt().max(1e-9)
}

/// The key a chroma profile is in, if it has clear notes at all.
pub fn key_of(chroma: &[f32; 12]) -> Option<SampleKey> {
    let max = chroma.iter().copied().fold(0.0, f32::max);
    if max <= 0.0 {
        return None;
    }
    let mean = chroma.iter().sum::<f32>() / 12.0;
    // Flat profiles - drums, noise - have no notes to speak of.
    if max / mean < 1.9 {
        return None;
    }
    let notes = (0..12).filter(|&pc| chroma[pc] >= 0.4 * max).fold(0u16, |m, pc| m | 1 << pc);
    // One or two notes is a pitched thump (a kick, a tom), not a key.
    if notes.count_ones() < 3 {
        return None;
    }
    let mut best = (f32::MIN, 0u8, false);
    for root in 0..12u8 {
        let rotated: [f32; 12] = std::array::from_fn(|i| chroma[(i + root as usize) % 12]);
        for (minor, profile) in [(false, &MAJOR), (true, &MINOR)] {
            let r = correlation(&rotated, profile);
            if r > best.0 {
                best = (r, root, minor);
            }
        }
    }
    Some(SampleKey { root: best.1, minor: best.2, notes })
}

/// Drums by name: their tones (a kick's thump, a snare's ring) add up to
/// something chord-like that isn't a key, so they fit any key.
fn is_drums(path: &std::path::Path) -> bool {
    let name = path.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    ["kick", "snare", "clap", "hat", "drum", "perc", "tom", "cymbal", "shaker", "rim"].iter().any(|d| name.contains(d))
}

/// Analyses a `.wav` (a path under the assets folder).
pub fn analyse_file(path: &std::path::Path) -> Option<SampleKey> {
    if is_drums(path) {
        return None;
    }
    let (samples, spec) = crate::timeline::peaks_loader::decode_wav(path)?;
    let channels = spec.channels.max(1) as usize;
    let mono: Vec<f32> = samples.chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect();
    key_of(&chroma(&mono, spec.sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tones(pitches: &[u8], seconds: f32) -> Vec<f32> {
        let sr = 48_000.0;
        (0..(sr * seconds) as usize)
            .map(|i| {
                let t = i as f32 / sr;
                pitches.iter().map(|&p| 0.2 * (2.0 * PI * 440.0 * 2f32.powf((p as f32 - 69.0) / 12.0) * t).sin()).sum::<f32>()
            })
            .collect()
    }

    #[test]
    fn an_a_minor_chord_is_in_a_minor() {
        let key = key_of(&chroma(&tones(&[57, 60, 64], 1.0), 48_000)).unwrap();
        assert_eq!((key.root, key.minor), (9, true), "{}", key.label());
        assert_eq!(key.notes, 1 << 9 | 1 << 0 | 1 << 4);
    }

    #[test]
    fn noise_has_no_key() {
        let mut x = 0x1234_5678u32;
        let noise: Vec<f32> = (0..48_000)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x as f32 / u32::MAX as f32) - 0.5
            })
            .collect();
        assert_eq!(key_of(&chroma(&noise, 48_000)), None);
    }

    #[test]
    fn fitting_is_every_note_in_the_scale() {
        let a_minor = SampleKey { root: 9, minor: true, notes: 1 << 9 | 1 << 0 | 1 << 4 };
        let minor_pent = shared::theory::SCALE_PRESETS.iter().find(|p| p.name == "Minor pentatonic").unwrap().mask;
        assert!(a_minor.fits(9, minor_pent));
        // A C E isn't all in C minor pentatonic (C Eb F G Bb).
        assert!(!a_minor.fits(0, minor_pent));
    }
}
