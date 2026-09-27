//! The Compressor's transfer curve (input level -> output level) - the
//! same hard-knee gain-reduction math `engine/src/compressor.rs::process`
//! runs, recomputed here purely for display (no dependency on the engine
//! crate from `ui`, same "shared math, separate real-time DSP" split
//! `eq_curve.rs` already uses for the EQ's frequency-response curve).

use vizia::prelude::*;
use vizia::vg;

use shared::arrangement::CompressorState;

/// Output level in dB for a given input level in dB - the exact formula
/// `Compressor::process` applies to its (already-settled) envelope, minus
/// the envelope smoothing itself (a display curve shows the steady-state
/// transfer function, not attack/release behavior).
fn output_db(state: CompressorState, input_db: f32) -> f32 {
    let over_db = (input_db - state.threshold_db).max(0.0);
    let gain_reduction_db = over_db * (1.0 - 1.0 / state.ratio.max(1.0));
    input_db + state.makeup_db - gain_reduction_db
}

pub struct CompressorCurve {
    state: Memo<CompressorState>,
    theme: Signal<crate::tokens::ThemeId>,
}

impl CompressorCurve {
    pub fn new(cx: &mut Context, state: Memo<CompressorState>, theme: Signal<crate::tokens::ThemeId>) -> Handle<'_, Self> {
        Self { state, theme }
            .build(cx, |_| {})
            .bind(state, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
    }
}

impl View for CompressorCurve {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();
        let state = self.state.get();

        // Both axes span the same -60..0dB range the Threshold knob
        // covers, plus headroom above 0 for makeup gain to push into.
        let db_min = -60.0f32;
        let db_max = 12.0f32;
        let db_range = db_max - db_min;
        let px = |db: f32| bounds.x + ((db - db_min) / db_range).clamp(0.0, 1.0) * bounds.w;
        let py = |db: f32| bounds.y + bounds.h - ((db - db_min) / db_range).clamp(0.0, 1.0) * bounds.h;

        // Faint 1:1 reference diagonal (unity gain, no compression).
        let mut ref_paint = vg::Paint::default();
        ref_paint.set_color(palette.line);
        ref_paint.set_style(vg::PaintStyle::Stroke);
        ref_paint.set_stroke_width(1.0);
        ref_paint.set_anti_alias(true);
        let mut ref_path = vg::PathBuilder::new();
        ref_path.move_to(vg::Point::new(px(db_min), py(db_min)));
        ref_path.line_to(vg::Point::new(px(db_max), py(db_max)));
        canvas.draw_path(&ref_path.detach(), &ref_paint);

        // The actual transfer curve.
        let mut path = vg::PathBuilder::new();
        let steps = 48;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let input_db = db_min + t * db_range;
            let out = output_db(state, input_db);
            let (x, y) = (px(input_db), py(out));
            if i == 0 {
                path.move_to(vg::Point::new(x, y));
            } else {
                path.line_to(vg::Point::new(x, y));
            }
        }
        let mut paint = vg::Paint::default();
        paint.set_color(palette.ink);
        paint.set_style(vg::PaintStyle::Stroke);
        paint.set_stroke_width(1.5);
        paint.set_anti_alias(true);
        canvas.draw_path(&path.detach(), &paint);

        // A small dot marking the knee (threshold on both axes, before
        // makeup shifts the output).
        let (kx, ky) = (px(state.threshold_db), py(output_db(state, state.threshold_db)));
        let mut knee = vg::Paint::default();
        knee.set_color(palette.ink);
        knee.set_anti_alias(true);
        canvas.draw_path(&vg::Path::circle(vg::Point::new(kx, ky), 2.0, None), &knee);
    }
}
