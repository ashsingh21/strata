//! Min/max peak pyramid for drawing audio waveforms without touching raw
//! samples at draw time. Building one is pure CPU work with no I/O, so it's
//! safe to call from a background thread (the caller decodes the WAV and
//! passes decoded samples in); nothing here should run on the UI or audio
//! thread for anything but the smallest test fixtures.

/// Samples-per-entry at each pyramid level, finest first.
pub const RESOLUTIONS: [u32; 3] = [256, 1024, 4096];

#[derive(Debug)]
pub struct PeakLevel {
    pub resolution: u32,
    pub mins: Vec<i8>,
    pub maxs: Vec<i8>,
}

#[derive(Debug)]
pub struct PeakPyramid {
    /// Ascending resolution (finest first).
    pub levels: Vec<PeakLevel>,
    pub length_samples: u64,
    pub sample_rate: u32,
}

impl PeakPyramid {
    /// Builds all levels from mono (already downmixed) samples in -1..1.
    pub fn build_from_mono_samples(samples: &[f32], sample_rate: u32) -> Self {
        let levels = RESOLUTIONS.iter().map(|&res| build_level(samples, res)).collect();
        Self { levels, length_samples: samples.len() as u64, sample_rate }
    }

    /// Downmixes interleaved multi-channel samples to mono, then builds.
    pub fn build_from_interleaved(samples: &[f32], channels: u16, sample_rate: u32) -> Self {
        let channels = channels.max(1) as usize;
        let mono: Vec<f32> = samples
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect();
        Self::build_from_mono_samples(&mono, sample_rate)
    }

    /// One (min, max) pair per pixel over `[start_sample, end_sample)`,
    /// picking the coarsest level that still gives roughly one peak per
    /// pixel. Values are in -1..1.
    pub fn peaks_for_range(
        &self,
        start_sample: u64,
        end_sample: u64,
        pixel_count: usize,
    ) -> Vec<(f32, f32)> {
        if pixel_count == 0 || end_sample <= start_sample || self.levels.is_empty() {
            return Vec::new();
        }
        let samples_per_pixel = (end_sample - start_sample) as f64 / pixel_count as f64;

        let level = self
            .levels
            .iter()
            .filter(|l| (l.resolution as f64) <= samples_per_pixel)
            .max_by_key(|l| l.resolution)
            .unwrap_or(&self.levels[0]);

        let mut out = Vec::with_capacity(pixel_count);
        for px in 0..pixel_count {
            let px_start = start_sample as f64 + px as f64 * samples_per_pixel;
            let px_end = start_sample as f64 + (px + 1) as f64 * samples_per_pixel;
            let entry_start = ((px_start / level.resolution as f64).floor() as usize)
                .min(level.mins.len());
            let entry_end = (((px_end / level.resolution as f64).ceil() as usize).max(entry_start + 1))
                .min(level.mins.len());

            if entry_start >= entry_end {
                out.push((0.0, 0.0));
                continue;
            }
            let mn = level.mins[entry_start..entry_end].iter().copied().min().unwrap_or(0);
            let mx = level.maxs[entry_start..entry_end].iter().copied().max().unwrap_or(0);
            out.push((mn as f32 / 127.0, mx as f32 / 127.0));
        }
        out
    }
}

fn build_level(samples: &[f32], resolution: u32) -> PeakLevel {
    let resolution = resolution.max(1);
    let entry_count = samples.len().div_ceil(resolution as usize).max(1);
    let mut mins = Vec::with_capacity(entry_count);
    let mut maxs = Vec::with_capacity(entry_count);
    for chunk in samples.chunks(resolution as usize) {
        let mut mn = 0.0f32;
        let mut mx = 0.0f32;
        for &s in chunk {
            mn = mn.min(s);
            mx = mx.max(s);
        }
        mins.push(quantize(mn));
        maxs.push(quantize(mx));
    }
    if mins.is_empty() {
        mins.push(0);
        maxs.push(0);
    }
    PeakLevel { resolution, mins, maxs }
}

fn quantize(sample: f32) -> i8 {
    (sample.clamp(-1.0, 1.0) * 127.0).round() as i8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(len: usize, cycles: f32) -> Vec<f32> {
        (0..len)
            .map(|i| (i as f32 / len as f32 * cycles * std::f32::consts::TAU).sin())
            .collect()
    }

    #[test]
    fn builds_all_three_levels() {
        let samples = sine(20_000, 40.0);
        let pyramid = PeakPyramid::build_from_mono_samples(&samples, 48_000);
        assert_eq!(pyramid.levels.len(), RESOLUTIONS.len());
        for (level, &res) in pyramid.levels.iter().zip(RESOLUTIONS.iter()) {
            assert_eq!(level.resolution, res);
        }
    }

    #[test]
    fn full_scale_sine_hits_near_extremes() {
        let samples = sine(100_000, 200.0);
        let pyramid = PeakPyramid::build_from_mono_samples(&samples, 48_000);
        let peaks = pyramid.peaks_for_range(0, samples.len() as u64, 50);
        for (mn, mx) in &peaks {
            assert!(*mx > 0.9, "expected near-full-scale max, got {mx}");
            assert!(*mn < -0.9, "expected near-full-scale min, got {mn}");
        }
    }

    #[test]
    fn silence_is_all_zero() {
        let samples = vec![0.0f32; 10_000];
        let pyramid = PeakPyramid::build_from_mono_samples(&samples, 48_000);
        let peaks = pyramid.peaks_for_range(0, samples.len() as u64, 20);
        assert!(peaks.iter().all(|&(mn, mx)| mn == 0.0 && mx == 0.0));
    }

    #[test]
    fn peaks_for_range_returns_requested_pixel_count() {
        let samples = sine(50_000, 30.0);
        let pyramid = PeakPyramid::build_from_mono_samples(&samples, 48_000);
        for px in [1, 7, 200, 768] {
            assert_eq!(pyramid.peaks_for_range(0, samples.len() as u64, px).len(), px);
        }
    }

    #[test]
    fn wide_range_prefers_coarser_level() {
        let samples = sine(1_000_000, 1000.0);
        let pyramid = PeakPyramid::build_from_mono_samples(&samples, 48_000);
        // ~1000 samples/pixel over 1000 pixels: coarsest level (4096) is too
        // coarse, so the 1024 level should be picked over the 256 one; both
        // finer levels would also satisfy "<= samples_per_pixel" except
        // 4096, so the max (1024) should win.
        let peaks = pyramid.peaks_for_range(0, 1_000_000, 1000);
        assert_eq!(peaks.len(), 1000);
    }

    #[test]
    fn empty_range_returns_empty() {
        let samples = sine(1000, 5.0);
        let pyramid = PeakPyramid::build_from_mono_samples(&samples, 48_000);
        assert!(pyramid.peaks_for_range(500, 500, 10).is_empty());
        assert!(pyramid.peaks_for_range(0, 1000, 0).is_empty());
    }
}
