//! A single-band peaking EQ's frequency-response curve - the same RBJ
//! peaking-biquad math `engine/src/eq.rs` runs, recomputed here purely
//! for display (no dependency on the engine crate from `ui`, same
//! "shared math, separate real-time DSP" split `synth/curves.rs` already
//! uses for Carve's own envelope/filter displays).

use vizia::prelude::*;
use vizia::vg;

use shared::arrangement::EqState;

/// 20*log10(|H(e^jw)|) for the RBJ peaking-EQ biquad at `freq_hz`, given
/// the effect's own config - pure math, no running state (unlike the
/// engine's own `Eq`, which needs continuity between blocks; a display
/// curve is recomputed fresh every time it's drawn).
fn response_db(state: EqState, freq_hz: f32, sample_rate: f32) -> f32 {
    let center = state.freq_hz.clamp(20.0, sample_rate * 0.49);
    let q = state.q.max(0.05);
    let a = 10f32.powf(state.gain_db / 40.0);
    let w0 = std::f32::consts::TAU * center / sample_rate;
    let (sin_w0, cos_w0) = w0.sin_cos();
    let alpha = sin_w0 / (2.0 * q);

    let b0 = 1.0 + alpha * a;
    let b1 = -2.0 * cos_w0;
    let b2 = 1.0 - alpha * a;
    let a0 = 1.0 + alpha / a;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha / a;
    let (b0, b1, b2, a1, a2) = (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0);

    let w = std::f32::consts::TAU * freq_hz / sample_rate;
    let (cw, sw) = w.sin_cos();
    let (c2w, s2w) = (2.0 * w).sin_cos();
    // H(e^jw) with z^-1 = cos(w) - j*sin(w), z^-2 = cos(2w) - j*sin(2w).
    let num_re = b0 + b1 * cw + b2 * c2w;
    let num_im = -b1 * sw - b2 * s2w;
    let den_re = 1.0 + a1 * cw + a2 * c2w;
    let den_im = -a1 * sw - a2 * s2w;
    let num_mag = (num_re * num_re + num_im * num_im).sqrt();
    let den_mag = (den_re * den_re + den_im * den_im).sqrt().max(1e-9);
    20.0 * (num_mag / den_mag).max(1e-6).log10()
}

pub struct EqCurve {
    state: Memo<EqState>,
    theme: Signal<crate::tokens::ThemeId>,
}

impl EqCurve {
    pub fn new(cx: &mut Context, state: Memo<EqState>, theme: Signal<crate::tokens::ThemeId>) -> Handle<'_, Self> {
        Self { state, theme }
            .build(cx, |_| {})
            .bind(state, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
    }
}

impl View for EqCurve {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();
        let state = self.state.get();
        let sample_rate = 48_000.0;

        let mut axis = vg::Paint::default();
        axis.set_color(palette.line);
        canvas.draw_path(
            &vg::Path::rect(vg::Rect::new(bounds.x, bounds.y + bounds.h * 0.5, bounds.x + bounds.w, bounds.y + bounds.h * 0.5 + 1.0), None),
            &axis,
        );

        let mut path = vg::PathBuilder::new();
        let steps = 64;
        // dB range the curve's height spans, clamped symmetrically -
        // matches the knob's own +-18dB gain range.
        let db_range = 18.0f32;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            // Log-spaced 20Hz..20kHz, same range the Freq knob covers.
            let freq = 20.0 * (20_000.0f32 / 20.0).powf(t);
            let db = response_db(state, freq, sample_rate).clamp(-db_range, db_range);
            let x = bounds.x + t * bounds.w;
            let y = bounds.y + bounds.h * 0.5 - (db / db_range) * bounds.h * 0.45;
            if i == 0 {
                path.move_to(vg::Point::new(x, y));
            } else {
                path.line_to(vg::Point::new(x, y));
            }
        }
        let path = path.detach();

        let mut paint = vg::Paint::default();
        paint.set_color(palette.ink);
        paint.set_style(vg::PaintStyle::Stroke);
        paint.set_stroke_width(1.5);
        paint.set_anti_alias(true);
        canvas.draw_path(&path, &paint);
    }
}

pub fn eq_curve(cx: &mut Context, state: Memo<EqState>, theme: Signal<crate::tokens::ThemeId>) {
    EqCurve::new(cx, state, theme).width(Pixels(96.0)).height(Pixels(48.0)).class("device");
}
