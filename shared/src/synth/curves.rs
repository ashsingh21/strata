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

    let peak = raw.iter().cloned().fold(0.0f32, f32::max).max(1e-6);
    raw.into_iter().map(|v| (v / peak).clamp(0.0, 1.0)).collect()
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

    #[test]
    fn filter_response_is_normalized() {
        let points = filter_response_points(FilterType::Lp24, 1200.0, 0.6, 170);
        assert_eq!(points.len(), 170);
        let peak = points.iter().cloned().fold(0.0f32, f32::max);
        assert!((peak - 1.0).abs() < 1e-4);
        assert!(points.iter().all(|&v| (0.0..=1.0).contains(&v)));
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
