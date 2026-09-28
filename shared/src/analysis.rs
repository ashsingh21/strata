//! Sound analysis for the spectrum views and the "Sound match" game: a
//! sound's spectrum (how loud it is at each pitch, on a musical - octave -
//! scale) and its loudness over time, and a score for how alike two
//! sounds are. Pure maths on sample buffers; no allocation-free promises,
//! so never call it from the audio thread.

use std::f32::consts::PI;

/// Lowest and highest frequency the spectrum covers.
pub const LOW_HZ: f32 = 40.0;
pub const HIGH_HZ: f32 = 16_000.0;
/// Spectrum bands per octave - fine enough that a note's first few
/// harmonics show as separate peaks.
pub const BANDS_PER_OCTAVE: f32 = 12.0;
/// Quietest level shown, in dB (everything quieter reads as this).
pub const FLOOR_DB: f32 = -80.0;
/// The loudness curve's step, in seconds.
pub const ENVELOPE_STEP: f32 = 0.025;
const ENVELOPE_FLOOR_DB: f32 = -60.0;
const FFT_SIZE: usize = 4096;

/// What a sound looks like: its spectrum, band by band from `LOW_HZ`
/// (each band `1 / BANDS_PER_OCTAVE` octave wide), and its loudness every
/// `ENVELOPE_STEP` seconds - both in dB.
#[derive(Clone, Debug, PartialEq)]
pub struct Analysis {
    pub spectrum: Vec<f32>,
    pub envelope: Vec<f32>,
}

/// How many spectrum bands there are.
pub fn band_count() -> usize {
    ((HIGH_HZ / LOW_HZ).log2() * BANDS_PER_OCTAVE).ceil() as usize
}

/// The centre frequency of band `i`.
pub fn band_hz(i: usize) -> f32 {
    LOW_HZ * 2f32.powf((i as f32 + 0.5) / BANDS_PER_OCTAVE)
}

/// In-place radix-2 FFT of `re`/`im` (length a power of two).
pub fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    // The twiddle factors, once: every stage uses a stride through them.
    let twiddles: Vec<(f32, f32)> = (0..n / 2).map(|k| (-2.0 * PI * k as f32 / n as f32).sin_cos()).collect();
    let mut len = 2;
    while len <= n {
        let stride = n / len;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (s, c) = twiddles[k * stride];
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * c - im[b] * s;
                let ti = re[b] * s + im[b] * c;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

fn to_db(power: f32, floor: f32) -> f32 {
    if power <= 0.0 {
        floor
    } else {
        (10.0 * power.log10()).max(floor)
    }
}

/// The average spectrum of `mono` between `from` and `to` seconds, in bands.
pub fn spectrum(mono: &[f32], sample_rate: u32, from: f32, to: f32) -> Vec<f32> {
    let sr = sample_rate as f32;
    let start = ((from * sr) as usize).min(mono.len());
    let end = ((to * sr) as usize).min(mono.len());
    let window: Vec<f32> = (0..FFT_SIZE).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / FFT_SIZE as f32).cos()).collect();
    // A Hann window's gain, so a full-scale sine reads about 0 dB.
    let norm = (FFT_SIZE as f32 / 4.0).powi(2);
    let mut power = vec![0.0f32; FFT_SIZE / 2];
    let mut frames = 0;
    // Frames overlapping by half across the range (at least one, if the
    // sound is long enough for a frame at all).
    let mut at = start;
    while at + FFT_SIZE <= mono.len() && (frames == 0 || at + FFT_SIZE <= end) {
        let mut re: Vec<f32> = (0..FFT_SIZE).map(|i| mono[at + i] * window[i]).collect();
        let mut im = vec![0.0f32; FFT_SIZE];
        fft(&mut re, &mut im);
        for (k, p) in power.iter_mut().enumerate() {
            *p += (re[k] * re[k] + im[k] * im[k]) / norm;
        }
        frames += 1;
        at += FFT_SIZE / 2;
    }
    let bin_hz = sr / FFT_SIZE as f32;
    (0..band_count())
        .map(|b| {
            if frames == 0 {
                return FLOOR_DB;
            }
            // The band's loudest bin: a harmonic's peak, not smeared
            // into the gaps around it.
            let lo = LOW_HZ * 2f32.powf(b as f32 / BANDS_PER_OCTAVE);
            let hi = LOW_HZ * 2f32.powf((b + 1) as f32 / BANDS_PER_OCTAVE);
            let (k0, k1) = ((lo / bin_hz).floor() as usize, ((hi / bin_hz).ceil() as usize).max((lo / bin_hz).floor() as usize + 1));
            let peak = power[k0.min(power.len() - 1)..k1.min(power.len())].iter().copied().fold(0.0, f32::max);
            to_db(peak / frames as f32, FLOOR_DB)
        })
        .collect()
}

/// Loudness (RMS, dB) every `ENVELOPE_STEP` seconds.
pub fn envelope(mono: &[f32], sample_rate: u32) -> Vec<f32> {
    let step = ((ENVELOPE_STEP * sample_rate as f32) as usize).max(1);
    mono.chunks(step)
        .map(|c| {
            let ms = c.iter().map(|x| x * x).sum::<f32>() / c.len() as f32;
            to_db(ms, ENVELOPE_FLOOR_DB)
        })
        .collect()
}

/// Mixes interleaved stereo down to mono.
pub fn mono(stereo: &[f32]) -> Vec<f32> {
    stereo.chunks(2).map(|f| 0.5 * (f[0] + f.get(1).copied().unwrap_or(f[0]))).collect()
}

/// Analyses a rendered note: the spectrum while it's held (`held` seconds
/// in, skipping the attack's first moment), and the whole loudness curve.
pub fn analyse(stereo: &[f32], sample_rate: u32, held: (f32, f32)) -> Analysis {
    let m = mono(stereo);
    Analysis { spectrum: spectrum(&m, sample_rate, held.0, held.1), envelope: envelope(&m, sample_rate) }
}

/// How alike two sounds are, 0 (nothing alike) to 1 (the same): the
/// average gap between their spectra and between their loudness curves,
/// in dB, where either is audible.
pub fn likeness(a: &Analysis, b: &Analysis) -> f32 {
    fn gap(x: &[f32], y: &[f32], audible: f32) -> f32 {
        let pairs: Vec<(f32, f32)> = x.iter().zip(y).map(|(&p, &q)| (p, q)).filter(|(p, q)| p.max(*q) > audible).collect();
        if pairs.is_empty() {
            return 0.0;
        }
        pairs.iter().map(|(p, q)| (p - q).abs()).sum::<f32>() / pairs.len() as f32
    }
    // dB of average difference that counts as "nothing alike".
    const SPECTRUM_SPAN: f32 = 24.0;
    const ENVELOPE_SPAN: f32 = 24.0;
    let spectrum = 1.0 - (gap(&a.spectrum, &b.spectrum, FLOOR_DB + 20.0) / SPECTRUM_SPAN).min(1.0);
    let envelope = 1.0 - (gap(&a.envelope, &b.envelope, ENVELOPE_FLOOR_DB + 10.0) / ENVELOPE_SPAN).min(1.0);
    0.6 * spectrum + 0.4 * envelope
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, amp: f32, seconds: f32) -> Vec<f32> {
        (0..(48_000.0 * seconds) as usize).map(|i| amp * (2.0 * PI * hz * i as f32 / 48_000.0).sin()).collect()
    }

    #[test]
    fn a_sine_shows_one_peak_where_it_is() {
        let s = spectrum(&sine(440.0, 0.5, 1.0), 48_000, 0.0, 1.0);
        let loudest = (0..s.len()).max_by(|&a, &b| s[a].total_cmp(&s[b])).unwrap();
        let hz = band_hz(loudest);
        assert!((hz / 440.0).log2().abs() < 1.0 / BANDS_PER_OCTAVE, "peak at {hz} Hz");
        // Half-scale: about -6 dB.
        assert!((s[loudest] + 6.0).abs() < 2.0, "{} dB", s[loudest]);
        // Two octaves away it's quiet.
        let far = (0..s.len()).find(|&i| band_hz(i) > 1760.0).unwrap();
        assert!(s[far] < s[loudest] - 40.0);
    }

    #[test]
    fn the_envelope_follows_loudness() {
        let mut x = sine(440.0, 0.5, 0.5);
        x.extend(vec![0.0; 24_000]);
        let e = envelope(&x, 48_000);
        assert!(e[5] > -12.0);
        assert!(e[e.len() - 2] <= -60.0);
    }

    #[test]
    fn likeness_is_one_for_the_same_sound_and_lower_for_another() {
        let stereo = |m: Vec<f32>| m.iter().flat_map(|&x| [x, x]).collect::<Vec<_>>();
        let a = analyse(&stereo(sine(220.0, 0.5, 1.0)), 48_000, (0.1, 0.9));
        let b = analyse(&stereo(sine(880.0, 0.5, 1.0)), 48_000, (0.1, 0.9));
        assert!((likeness(&a, &a) - 1.0).abs() < 1e-6);
        assert!(likeness(&a, &b) < 0.9);
    }
}
