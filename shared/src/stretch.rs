//! Time stretch: audio made longer or shorter without changing its pitch
//! (WSOLA - overlapping short windows of the original, each taken from
//! where it best continues the one before, so the waveform never jumps).
//! Offline: a clip is stretched once into a new file, not while playing.

/// `samples` (interleaved, `channels` per frame, at `rate`) stretched to
/// `factor` times as long: 2.0 is half speed, 0.5 double, same pitch.
pub fn stretch(samples: &[f32], channels: usize, rate: u32, factor: f64) -> Vec<f32> {
    let channels = channels.max(1);
    let frames = samples.len() / channels;
    let factor = factor.clamp(0.25, 4.0);
    let out_frames = (frames as f64 * factor).round() as usize;
    if frames < 2 || (factor - 1.0).abs() < 1e-6 {
        return samples.to_vec();
    }

    // ~40 ms windows, half-overlapped; a quarter window either way to find
    // the best fit.
    let window = ((rate as f64 * 0.04) as usize).max(64) & !1;
    let hop_out = window / 2;
    let hop_in = hop_out as f64 / factor;
    let tolerance = window / 4;
    let hann: Vec<f32> = (0..window).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / window as f32).cos()).collect();
    // One channel to line windows up by (the mix of all of them).
    let mono: Vec<f32> = samples.chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect();
    let mono_at = |i: isize| if i >= 0 && (i as usize) < frames { mono[i as usize] } else { 0.0 };

    let mut out = vec![0.0f32; (out_frames + window) * channels];
    let mut weight = vec![0.0f32; out_frames + window];
    let mut prev: isize = 0;
    let mut k = 0usize;
    while k * hop_out < out_frames {
        let nominal = (k as f64 * hop_in) as isize;
        let pos = if k == 0 {
            0
        } else {
            // Where the last window would naturally have carried on.
            let natural = prev + hop_out as isize;
            let mut best = (nominal, f32::MIN);
            let mut offset = -(tolerance as isize);
            while offset <= tolerance as isize {
                let candidate = nominal + offset;
                let score: f32 = (0..window).step_by(4).map(|i| mono_at(natural + i as isize) * mono_at(candidate + i as isize)).sum();
                if score > best.1 {
                    best = (candidate, score);
                }
                offset += 2;
            }
            best.0
        };
        let at = k * hop_out;
        for i in 0..window {
            let src = pos + i as isize;
            if src < 0 || src as usize >= frames {
                continue;
            }
            let w = hann[i];
            weight[at + i] += w;
            for c in 0..channels {
                out[(at + i) * channels + c] += w * samples[src as usize * channels + c];
            }
        }
        prev = pos;
        k += 1;
    }
    out.truncate(out_frames * channels);
    for (i, frame) in out.chunks_mut(channels).enumerate() {
        let w = weight[i];
        if w > 1e-3 {
            for s in frame {
                *s /= w;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, rate: u32, seconds: f32) -> Vec<f32> {
        (0..(rate as f32 * seconds) as usize).map(|i| (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin() * 0.5).collect()
    }

    /// Upward zero crossings per second: the pitch.
    fn pitch(samples: &[f32], rate: u32) -> f32 {
        let ups = samples.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        ups as f32 / (samples.len() as f32 / rate as f32)
    }

    #[test]
    fn longer_and_shorter_at_the_same_pitch() {
        let rate = 48_000;
        let tone = sine(440.0, rate, 1.0);
        for factor in [0.5, 1.5, 2.0] {
            let out = stretch(&tone, 1, rate, factor);
            assert_eq!(out.len(), (tone.len() as f64 * factor).round() as usize);
            let p = pitch(&out[4800..out.len() - 4800], rate);
            assert!((p - 440.0).abs() < 8.0, "factor {factor}: {p} Hz");
        }
    }

    #[test]
    fn stays_at_the_same_level() {
        let rate = 48_000;
        let out = stretch(&sine(220.0, rate, 1.0), 1, rate, 1.5);
        let peak = out[4800..out.len() - 4800].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((0.4..0.6).contains(&peak), "{peak}");
    }

    #[test]
    fn stereo_keeps_its_channels() {
        let rate = 48_000;
        let left = sine(330.0, rate, 0.5);
        let stereo: Vec<f32> = left.iter().flat_map(|s| [*s, 0.0]).collect();
        let out = stretch(&stereo, 2, rate, 2.0);
        assert!(out.chunks(2).all(|f| f[1] == 0.0));
        assert!(out.chunks(2).any(|f| f[0].abs() > 0.4));
    }
}
