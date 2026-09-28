//! The EQ's filter maths, shared by the engine (which runs it) and the UI
//! (which draws its curve), so the two can't disagree. Every band is a
//! biquad from Robert Bristow-Johnson's "Audio EQ Cookbook".

use crate::arrangement::{EqBand, EqBandKind, EqState};

/// A biquad's coefficients, normalized so a0 = 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Biquad {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl Biquad {
    pub const IDENTITY: Biquad = Biquad { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    /// This filter's gain at `freq_hz`, in dB.
    pub fn response_db(&self, freq_hz: f32, sample_rate: f32) -> f32 {
        let w = std::f32::consts::TAU * freq_hz / sample_rate;
        let (sw, cw) = w.sin_cos();
        let (s2w, c2w) = (2.0 * w).sin_cos();
        // H(e^jw) with z^-1 = cos(w) - j sin(w), z^-2 = cos(2w) - j sin(2w).
        let num_re = self.b0 + self.b1 * cw + self.b2 * c2w;
        let num_im = -self.b1 * sw - self.b2 * s2w;
        let den_re = 1.0 + self.a1 * cw + self.a2 * c2w;
        let den_im = -self.a1 * sw - self.a2 * s2w;
        let num = (num_re * num_re + num_im * num_im).sqrt();
        let den = (den_re * den_re + den_im * den_im).sqrt().max(1e-9);
        20.0 * (num / den).max(1e-6).log10()
    }
}

/// Shelves use the cookbook's shelf slope S = 1 (the steepest without a
/// bump), which is Q = 1/sqrt(2); the low cut is a Butterworth high-pass.
const BUTTERWORTH_Q: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// Whether `band` changes the sound at all (off, or a shelf/bell at 0 dB,
/// doesn't - the engine skips those).
pub fn is_active(band: &EqBand) -> bool {
    band.on && (band.kind == EqBandKind::LowCut || band.gain_db.abs() > 1.0e-3)
}

/// `band`'s coefficients at `sample_rate` (identity when it's inactive).
pub fn coefficients(band: &EqBand, sample_rate: f32) -> Biquad {
    if !is_active(band) {
        return Biquad::IDENTITY;
    }
    let freq = band.freq_hz.clamp(10.0, sample_rate * 0.49);
    let w0 = std::f32::consts::TAU * freq / sample_rate;
    let (sin_w0, cos_w0) = w0.sin_cos();
    let a = 10f32.powf(band.gain_db / 40.0);
    let q = match band.kind {
        EqBandKind::Bell => band.q.max(0.05),
        _ => BUTTERWORTH_Q,
    };
    let alpha = sin_w0 / (2.0 * q);
    let (b0, b1, b2, a0, a1, a2) = match band.kind {
        EqBandKind::LowCut => {
            let b = (1.0 + cos_w0) / 2.0;
            (b, -(1.0 + cos_w0), b, 1.0 + alpha, -2.0 * cos_w0, 1.0 - alpha)
        }
        EqBandKind::Bell => {
            (1.0 + alpha * a, -2.0 * cos_w0, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos_w0, 1.0 - alpha / a)
        }
        EqBandKind::LowShelf => {
            let k = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) - (a - 1.0) * cos_w0 + k),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
                a * ((a + 1.0) - (a - 1.0) * cos_w0 - k),
                (a + 1.0) + (a - 1.0) * cos_w0 + k,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
                (a + 1.0) + (a - 1.0) * cos_w0 - k,
            )
        }
        EqBandKind::HighShelf => {
            let k = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) + (a - 1.0) * cos_w0 + k),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
                a * ((a + 1.0) + (a - 1.0) * cos_w0 - k),
                (a + 1.0) - (a - 1.0) * cos_w0 + k,
                2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
                (a + 1.0) - (a - 1.0) * cos_w0 - k,
            )
        }
    };
    Biquad { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
}

/// The whole EQ's gain at `freq_hz`, in dB: its bands, one after another.
pub fn response_db(state: &EqState, freq_hz: f32, sample_rate: f32) -> f32 {
    state.bands.iter().map(|b| coefficients(b, sample_rate).response_db(freq_hz, sample_rate)).sum()
}

/// A band's short name, as the EQ panel labels it.
pub fn band_name(kind: EqBandKind) -> &'static str {
    match kind {
        EqBandKind::LowCut => "Low cut",
        EqBandKind::LowShelf => "Low",
        EqBandKind::Bell => "Bell",
        EqBandKind::HighShelf => "High",
    }
}

/// What the EQ is doing, in a few words: "Flat", the one band that's
/// doing something ("Bell 400 Hz +8.0 dB"), or how many are.
pub fn summary(state: &EqState) -> String {
    let active: Vec<&EqBand> = state.bands.iter().filter(|b| is_active(b)).collect();
    match active.as_slice() {
        [] => "Flat".to_string(),
        [b] if b.kind == EqBandKind::LowCut => format!("Low cut {:.0} Hz", b.freq_hz),
        [b] => format!("{} {:.0} Hz {:+.1} dB", band_name(b.kind), b.freq_hz, b.gain_db),
        many => format!("{} bands", many.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangement::{EQ_BELL, EQ_HIGH_SHELF, EQ_LOW_CUT, EQ_LOW_SHELF};

    const SR: f32 = 48_000.0;

    #[test]
    fn the_default_eq_is_flat() {
        let eq = EqState::default();
        for f in [20.0, 100.0, 1000.0, 10_000.0, 20_000.0] {
            assert!(response_db(&eq, f, SR).abs() < 0.01, "{f} Hz");
        }
    }

    #[test]
    fn each_band_does_its_job() {
        let mut eq = EqState::default();
        eq.bands[EQ_LOW_CUT].on = true;
        eq.bands[EQ_LOW_CUT].freq_hz = 100.0;
        assert!(response_db(&eq, 100.0, SR) < -2.5 && response_db(&eq, 100.0, SR) > -3.5, "-3 dB at the cutoff");
        assert!(response_db(&eq, 25.0, SR) < -20.0, "12 dB/oct below it");
        assert!(response_db(&eq, 1000.0, SR).abs() < 0.1, "flat above it");

        let mut eq = EqState::default();
        eq.bands[EQ_LOW_SHELF].gain_db = 6.0;
        eq.bands[EQ_HIGH_SHELF].gain_db = -6.0;
        assert!((response_db(&eq, 20.0, SR) - 6.0).abs() < 0.2);
        assert!((response_db(&eq, 18_000.0, SR) + 6.0).abs() < 0.5);
        assert!(response_db(&eq, 1000.0, SR).abs() < 0.5, "both shelves leave the middle alone");

        let eq = EqState::bell(2000.0, 9.0, 2.0);
        assert!((response_db(&eq, 2000.0, SR) - 9.0).abs() < 0.05);
        assert!(response_db(&eq, 200.0, SR).abs() < 0.3);
        assert_eq!(eq.bands[EQ_BELL].kind, EqBandKind::Bell);
    }

    #[test]
    fn a_one_band_eq_saved_before_bands_loads_as_the_bell() {
        let old: EqState = serde_json::from_str(r#"{"freq_hz":400.0,"gain_db":8.0,"q":2.0}"#).unwrap();
        assert_eq!(old, EqState::bell(400.0, 8.0, 2.0));
        let json = serde_json::to_string(&old).unwrap();
        assert_eq!(serde_json::from_str::<EqState>(&json).unwrap(), old);
    }
}
