//! Pure, analytic stand-ins for the synth's displays: an oscillator
//! waveform, a filter's magnitude response, and an ADSR envelope shape.
//! None of this is audio-accurate DSP - it only has to look like the
//! thing it represents, cheaply enough to redraw at 60fps.

use super::model::{Envelope, FilterType, Waveform};

/// `n` samples of one or more cycles of `waveform` with its Shape knob at
/// `shape` (0..1), in -1..1 - the same shapes the engine plays, so the
/// picture shows which way the knob bends the wave.
pub fn waveform_points(waveform: Waveform, shape: f32, cycles: f32, n: usize) -> Vec<f32> {
    let shape = shape.clamp(0.0, 1.0);
    (0..n)
        .map(|i| {
            let t = i as f32 / (n - 1).max(1) as f32;
            let phase = (t * cycles).fract();
            match waveform {
                // A second harmonic folded in.
                Waveform::Sine => {
                    let fold = shape * 0.6;
                    let x = phase * std::f32::consts::TAU;
                    (x.sin() + fold * (2.0 * x).sin()) / (1.0 + fold)
                }
                // Up over `duty`, down over the rest: a triangle leans into
                // a saw, a saw rounds into a triangle.
                Waveform::Triangle | Waveform::Saw => {
                    let duty = if waveform == Waveform::Triangle { 0.5 + shape * 0.48 } else { 1.0 - shape * 0.5 }.clamp(0.01, 0.99);
                    if phase < duty {
                        -1.0 + 2.0 * phase / duty
                    } else {
                        1.0 - 2.0 * (phase - duty) / (1.0 - duty)
                    }
                }
                // A square at 0, narrowing to a 5% pulse.
                Waveform::Square => {
                    if phase < 0.5 - shape * 0.45 {
                        1.0
                    } else {
                        -1.0
                    }
                }
            }
        })
        .collect()
}

/// What the Shape knob does to `waveform`, as its label: the direction it
/// bends the wave.
pub fn shape_label(waveform: Waveform) -> &'static str {
    match waveform {
        Waveform::Sine => "Harmonic",
        Waveform::Triangle => "To saw",
        Waveform::Saw => "To triangle",
        Waveform::Square => "Narrow",
    }
}

/// `n` samples of the filter's magnitude response (0..1, normalized to its
/// own peak) across a log frequency axis from 20 Hz to 20 kHz.
pub fn filter_response_points(
    filter_type: FilterType,
    cutoff_hz: f32,
    resonance: f32,
    n: usize,
) -> Vec<f32> {
    let q = 0.5 + resonance.clamp(0.0, 1.0) * 8.0;
    let log_min = 20f32.ln();
    let log_max = 20_000f32.ln();

    let raw: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / (n - 1).max(1) as f32;
            let freq = (log_min + t * (log_max - log_min)).exp();
            let ratio = freq / cutoff_hz.max(1.0);
            resonant_magnitude(filter_type, ratio, q)
        })
        .collect();

    raw.into_iter().map(|v| db_height(gain_db(v))).collect()
}

/// The filter display's vertical scale, in dB: fixed, like an EQ's, so
/// what passes untouched always sits on the 0 dB line and resonance rises
/// above it. (Scaled to its own peak, a resonant curve squashed the
/// passband to the floor - it looked as if the filter removed everything.)
pub const FILTER_DB_TOP: f32 = 24.0;
pub const FILTER_DB_FLOOR: f32 = -48.0;

/// A level in dB as a height on the filter display: 0 at the floor, 1 at
/// the top, `db_height(0.0)` the "passes untouched" line.
pub fn db_height(db: f32) -> f32 {
    ((db - FILTER_DB_FLOOR) / (FILTER_DB_TOP - FILTER_DB_FLOOR)).clamp(0.0, 1.0)
}

fn gain_db(gain: f32) -> f32 {
    20.0 * gain.max(1e-9).log10()
}

/// How much the filter passes at `freq` (linear gain; 1 = untouched).
pub fn filter_gain(filter_type: FilterType, cutoff_hz: f32, resonance: f32, freq: f32) -> f32 {
    let ratio = freq / cutoff_hz.max(1.0);
    if filter_type == FilterType::Lp24 {
        return ladder_magnitude(ratio, ladder_k(resonance));
    }
    resonant_magnitude(filter_type, ratio, svf_q(resonance))
}

/// The state-variable filter's Q (LP 12, BP, HP) for the Resonance knob:
/// a gentle start, about +7 dB at the cutoff by half way, +18 dB at full.
pub fn svf_q(resonance: f32) -> f32 {
    let r = resonance.clamp(0.0, 1.0);
    0.5 + r * r * 7.5
}

/// The LP 24 ladder's feedback for the Resonance knob: 4 at full, the edge
/// of ringing on its own.
pub fn ladder_k(resonance: f32) -> f32 {
    4.0 * resonance.clamp(0.0, 1.0).powf(0.7)
}

/// The ladder's response (four one-pole stages in a feedback loop, with
/// its bass make-up gain): |1 / ((1 + jr)^4 + k)| x (1 + k/2).
fn ladder_magnitude(ratio: f32, k: f32) -> f32 {
    // (1 + jr)^2 = (1 - r^2) + j2r, squared again.
    let (a, b) = (1.0 - ratio * ratio, 2.0 * ratio);
    let (re, im) = (a * a - b * b + k, 2.0 * a * b);
    (1.0 + 0.5 * k) / (re * re + im * im).sqrt().max(1e-6)
}

/// The harmonics a wave holds, as (multiple of the note, level): a sine
/// only its fundamental, a triangle the odd ones falling fast (1/n²), a
/// square the odd ones (1/n), a saw all of them (1/n). The Shape knob's
/// bending isn't included - this is what each wave is, to see the filter
/// against.
pub fn harmonics(waveform: Waveform, max_harmonic: u32) -> Vec<(u32, f32)> {
    (1..=max_harmonic)
        .filter_map(|n| {
            let level = match waveform {
                Waveform::Sine => (n == 1).then_some(1.0)?,
                Waveform::Triangle => (n % 2 == 1).then(|| 1.0 / (n * n) as f32)?,
                Waveform::Square => (n % 2 == 1).then(|| 1.0 / n as f32)?,
                Waveform::Saw => 1.0 / n as f32,
            };
            Some((n, level))
        })
        .collect()
}

fn resonant_magnitude(filter_type: FilterType, ratio: f32, q: f32) -> f32 {
    let lowpass_2pole = |r: f32| -> f32 {
        let denom = ((1.0 - r * r).powi(2) + (r / q).powi(2)).sqrt();
        1.0 / denom.max(1e-6)
    };
    match filter_type {
        FilterType::Lp12 => lowpass_2pole(ratio),
        FilterType::Lp24 => lowpass_2pole(ratio).powi(2),
        FilterType::Hp => ratio * ratio * lowpass_2pole(ratio),
        FilterType::Bp => (ratio / q) * lowpass_2pole(ratio),
    }
}

/// Key points (time fraction 0..1, level 0..1) tracing an ADSR shape: a
/// fixed visual sustain plateau stands in for sustain's indefinite hold.
pub fn envelope_points(env: Envelope) -> [(f32, f32); 5] {
    const SUSTAIN_PLATEAU_MS: f32 = 220.0;
    let total = (env.attack_ms + env.decay_ms + SUSTAIN_PLATEAU_MS + env.release_ms).max(1.0);

    let t_attack = env.attack_ms / total;
    let t_decay_end = t_attack + env.decay_ms / total;
    let t_sustain_end = t_decay_end + SUSTAIN_PLATEAU_MS / total;

    [
        (0.0, 0.0),
        (t_attack, 1.0),
        (t_decay_end, env.sustain),
        (t_sustain_end, env.sustain),
        (1.0, 0.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::model::Waveform;

    #[test]
    fn sine_wave_hits_extremes() {
        let points = waveform_points(Waveform::Sine, 0.0, 1.0, 100);
        assert!(points.iter().cloned().fold(0.0f32, f32::max) > 0.99);
        assert!(points.iter().cloned().fold(0.0f32, f32::min) < -0.99);
    }

    #[test]
    fn triangle_wave_hits_extremes_and_stays_in_range() {
        let points = waveform_points(Waveform::Triangle, 0.0, 2.0, 200);
        assert!(points.iter().cloned().fold(0.0f32, f32::max) > 0.95);
        assert!(points.iter().cloned().fold(0.0f32, f32::min) < -0.95);
        assert!(points.iter().all(|&v| (-1.0..=1.0).contains(&v)));
    }

    #[test]
    fn square_wave_is_bilevel() {
        let points = waveform_points(Waveform::Square, 0.0, 2.0, 50);
        assert!(points.iter().all(|&v| v == 1.0 || v == -1.0));
    }

    /// The knob bends each wave toward its neighbour: a triangle turned up
    /// is nearly a saw, a saw turned up is nearly a triangle, a narrow
    /// square is high for less of its cycle.
    #[test]
    fn shape_bends_each_wave_toward_its_neighbour() {
        let n = 400;
        let close = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f32>() / a.len() as f32;
        let saw = waveform_points(Waveform::Saw, 0.0, 1.0, n);
        let triangle = waveform_points(Waveform::Triangle, 0.0, 1.0, n);
        assert!(close(&waveform_points(Waveform::Triangle, 1.0, 1.0, n), &saw) < 0.05);
        assert!(close(&waveform_points(Waveform::Saw, 1.0, 1.0, n), &triangle) < 0.01);
        let high = |shape| waveform_points(Waveform::Square, shape, 1.0, n).iter().filter(|&&v| v > 0.0).count();
        assert!(high(0.5) < high(0.0));
        assert!(high(0.0).abs_diff(n / 2) <= 1, "0 is a true square: high half the cycle");
    }

    /// Resonance adds a peak at the cutoff; the lows below it still pass
    /// untouched, on the 0 dB line, however high the resonance goes.
    #[test]
    fn resonance_rises_above_an_unchanged_passband() {
        let unity = db_height(0.0);
        for resonance in [0.0, 0.7, 1.0] {
            let points = filter_response_points(FilterType::Lp24, 1200.0, resonance, 170);
            assert_eq!(points.len(), 170);
            assert!((points[0] - unity).abs() < 0.01, "passband moved at resonance {resonance}");
            assert!(points.iter().all(|&v| (0.0..=1.0).contains(&v)));
        }
        let peak = |r| filter_response_points(FilterType::Lp24, 1200.0, r, 170).into_iter().fold(0.0f32, f32::max);
        assert!(peak(0.7) > unity + 0.1, "resonance should rise above 0 dB");
    }

    #[test]
    fn harmonics_follow_each_wave() {
        assert_eq!(harmonics(Waveform::Sine, 8), vec![(1, 1.0)]);
        assert!(harmonics(Waveform::Square, 8).iter().all(|(n, _)| n % 2 == 1));
        assert_eq!(harmonics(Waveform::Saw, 8).len(), 8);
        let tri = harmonics(Waveform::Triangle, 8);
        assert!((tri[1].1 - 1.0 / 9.0).abs() < 1e-6);
    }

    #[test]
    fn lowpass_rolls_off_at_high_frequency() {
        let points = filter_response_points(FilterType::Lp24, 500.0, 0.2, 170);
        // Near 20 Hz (low end) should pass more than near 20 kHz (high end).
        assert!(points[0] > points[points.len() - 1]);
    }

    #[test]
    fn highpass_rolls_off_at_low_frequency() {
        let points = filter_response_points(FilterType::Hp, 500.0, 0.2, 170);
        assert!(points[points.len() - 1] > points[0]);
    }

    #[test]
    fn envelope_points_are_monotonic_in_time() {
        let env = Envelope { attack_ms: 4.0, decay_ms: 320.0, sustain: 0.35, release_ms: 280.0 };
        let points = envelope_points(env);
        for pair in points.windows(2) {
            assert!(pair[0].0 <= pair[1].0);
        }
        assert_eq!(points[0].1, 0.0);
        assert_eq!(points[1].1, 1.0);
        assert_eq!(points.last().unwrap().1, 0.0);
    }
}
