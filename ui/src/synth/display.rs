//! The synth's canvas displays: oscillator waveform, filter response,
//! envelope shape and the LFO scope. One shared colour rule (README):
//! the current shape is a 2px volt line over a volt-soft fill; modulation
//! is mod; grid/axis use grid-beat/line.

use vizia::prelude::*;
use vizia::vg;

use shared::synth::{envelope_points, filter_response_points, waveform_points, SynthState};

use crate::canvas_text::canvas_font;
use crate::tokens::ThemeId;

fn line_and_fill_path(points: &[(f32, f32)], w: f32, h: f32, baseline_frac: f32) -> (vg::Path, vg::Path) {
    let mut line = vg::PathBuilder::new();
    for (i, &(tx, ty)) in points.iter().enumerate() {
        let p = vg::Point::new(tx * w, ty * h);
        if i == 0 {
            line.move_to(p);
        } else {
            line.line_to(p);
        }
    }
    let line = line.detach();

    let mut fill = vg::PathBuilder::new();
    fill.move_to(vg::Point::new(0.0, baseline_frac * h));
    for &(tx, ty) in points {
        fill.line_to(vg::Point::new(tx * w, ty * h));
    }
    fill.line_to(vg::Point::new(w, baseline_frac * h));
    fill.close();
    let fill = fill.detach();

    (line, fill)
}

fn draw_signal(canvas: &Canvas, bounds: BoundingBox, palette: &crate::tokens::Palette, points: &[(f32, f32)], baseline_frac: f32) {
    let (line, fill) = line_and_fill_path(points, bounds.w, bounds.h, baseline_frac);

    let mut fill_paint = vg::Paint::default();
    fill_paint.set_color(palette.volt_soft);
    fill_paint.set_anti_alias(true);
    canvas.save();
    canvas.translate((bounds.x, bounds.y));
    canvas.draw_path(&fill, &fill_paint);

    let mut line_paint = vg::Paint::default();
    line_paint.set_color(palette.volt);
    line_paint.set_style(vg::PaintStyle::Stroke);
    line_paint.set_stroke_width(2.0);
    line_paint.set_anti_alias(true);
    canvas.draw_path(&line, &line_paint);
    canvas.restore();
}

/// An oscillator's waveform, redrawn from its current settings each time
/// they change (no need to animate: the shape only depends on waveform +
/// pulse-width/shape, not on a running phase).
pub struct WaveDisplay {
    state: Signal<SynthState>,
    theme: Signal<ThemeId>,
    extract: fn(&SynthState) -> shared::synth::Oscillator,
}

impl WaveDisplay {
    pub fn new(
        cx: &mut Context,
        state: Signal<SynthState>,
        theme: Signal<ThemeId>,
        extract: fn(&SynthState) -> shared::synth::Oscillator,
    ) -> Handle<'_, Self> {
        Self { state, theme, extract }
            .build(cx, |_| {})
            .bind(state, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
    }
}

impl View for WaveDisplay {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();
        let osc = (self.extract)(&self.state.get());

        let mut axis = vg::Paint::default();
        axis.set_color(palette.line);
        axis.set_anti_alias(true);
        canvas.draw_path(
            &vg::Path::rect(vg::Rect::new(bounds.x, bounds.y + bounds.h * 0.5, bounds.x + bounds.w, bounds.y + bounds.h * 0.5 + 1.0), None),
            &axis,
        );

        let n = 176usize;
        let samples = waveform_points(osc.waveform, 2.0, n);
        let points: Vec<(f32, f32)> = samples
            .iter()
            .enumerate()
            .map(|(i, &y)| (i as f32 / (n - 1) as f32, 0.5 - y * 0.42))
            .collect();
        draw_signal(canvas, bounds, &palette, &points, 0.5);
    }
}

/// The filter's magnitude response, with a dashed mod band showing the
/// cutoff's modulation range and a cutoff marker + readout.
pub struct FilterDisplay {
    state: Signal<SynthState>,
    theme: Signal<ThemeId>,
}

impl FilterDisplay {
    pub fn new(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>) -> Handle<'_, Self> {
        Self { state, theme }
            .build(cx, |_| {})
            .bind(state, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
    }
}

impl View for FilterDisplay {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();
        let filter = self.state.get().filter;

        // Grid lines at 1/4, 1/2, 3/4 width (~octave markers), matching the
        // preview's three vertical `grid-beat` lines.
        let mut grid = vg::Paint::default();
        grid.set_color(palette.grid_beat);
        grid.set_anti_alias(false);
        for frac in [0.25, 0.5, 0.75] {
            let x = bounds.x + bounds.w * frac;
            canvas.draw_path(&vg::Path::rect(vg::Rect::new(x, bounds.y, x + 1.0, bounds.y + bounds.h), None), &grid);
        }

        let n = 170usize;
        let response = filter_response_points(filter.filter_type, filter.cutoff_hz, filter.resonance, n);
        let points: Vec<(f32, f32)> =
            response.iter().enumerate().map(|(i, &v)| (i as f32 / (n - 1) as f32, 1.0 - v)).collect();

        // Modulation band: a dashed mod-coloured ring around the cutoff's
        // swept range, filled with mod-soft, matching the Cutoff knob's ring.
        let log_min = 20f32.ln();
        let log_max = 20_000f32.ln();
        let cutoff_frac = ((filter.cutoff_hz.max(1.0).ln() - log_min) / (log_max - log_min)).clamp(0.0, 1.0);
        let depth = filter.cutoff_mod_depth;
        let band_lo = (cutoff_frac - depth).clamp(0.0, 1.0);
        let band_hi = (cutoff_frac + depth).clamp(0.0, 1.0);
        let mut band_paint = vg::Paint::default();
        band_paint.set_color(palette.mod_soft);
        band_paint.set_anti_alias(true);
        canvas.draw_path(
            &vg::Path::rect(
                vg::Rect::new(bounds.x + band_lo * bounds.w, bounds.y, bounds.x + band_hi * bounds.w, bounds.y + bounds.h),
                None,
            ),
            &band_paint,
        );
        let mut band_line = vg::Paint::default();
        band_line.set_color(palette.md);
        band_line.set_style(vg::PaintStyle::Stroke);
        band_line.set_stroke_width(1.0);
        band_line.set_anti_alias(true);
        for frac in [band_lo, band_hi] {
            let x = bounds.x + bounds.w * frac;
            canvas.draw_path(&vg::Path::rect(vg::Rect::new(x, bounds.y, x + 1.0, bounds.y + bounds.h), None), &band_line);
        }

        draw_signal(canvas, bounds, &palette, &points, 1.0);

        // Axis (drawn after the fill so it stays visible).
        let mut axis = vg::Paint::default();
        axis.set_color(palette.line);
        axis.set_anti_alias(true);
        let axis_y = bounds.y + bounds.h * (1.0 - response[0]).max(0.0);
        canvas.draw_path(&vg::Path::rect(vg::Rect::new(bounds.x, axis_y, bounds.x + bounds.w, axis_y + 1.0), None), &axis);

        // Cutoff marker.
        let mut marker = vg::Paint::default();
        marker.set_color(palette.ink_faint);
        marker.set_anti_alias(true);
        let cutoff_x = bounds.x + cutoff_frac * bounds.w;
        canvas.draw_path(&vg::Path::rect(vg::Rect::new(cutoff_x, bounds.y, cutoff_x + 1.0, bounds.y + bounds.h), None), &marker);

        let font = canvas_font(10.0);
        let mut text_paint = vg::Paint::default();
        text_paint.set_color(palette.ink_muted);
        text_paint.set_anti_alias(true);
        let label = format_hz(filter.cutoff_hz);
        let label_x = (cutoff_x + 4.0).min(bounds.x + bounds.w - 50.0);
        canvas.draw_str(&label, vg::Point::new(label_x, bounds.y + 12.0), &font, &text_paint);
    }
}

fn format_hz(hz: f32) -> String {
    if hz >= 1000.0 {
        format!("{:.2} kHz", hz / 1000.0)
    } else {
        format!("{hz:.0} Hz")
    }
}

/// An ADSR envelope shape, with static 6px handles at the attack peak,
/// decay-end and release-start points (display only - dragging them is an
/// alternative input method the spec allows but this milestone doesn't
/// wire; the knobs below each display are the editing path).
pub struct EnvelopeDisplay {
    state: Signal<SynthState>,
    theme: Signal<ThemeId>,
    extract: fn(&SynthState) -> shared::synth::Envelope,
}

impl EnvelopeDisplay {
    pub fn new(
        cx: &mut Context,
        state: Signal<SynthState>,
        theme: Signal<ThemeId>,
        extract: fn(&SynthState) -> shared::synth::Envelope,
    ) -> Handle<'_, Self> {
        Self { state, theme, extract }
            .build(cx, |_| {})
            .bind(state, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
    }
}

impl View for EnvelopeDisplay {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();
        let env = (self.extract)(&self.state.get());
        let keypoints = envelope_points(env);
        let points: Vec<(f32, f32)> = keypoints.iter().map(|&(t, level)| (t, 1.0 - level * 0.92)).collect();

        draw_signal(canvas, bounds, &palette, &points, 1.0);

        let mut handle_paint = vg::Paint::default();
        handle_paint.set_color(palette.ink);
        handle_paint.set_anti_alias(true);
        for &(t, y) in &points[1..4] {
            let cx_px = bounds.x + t * bounds.w;
            let cy_px = bounds.y + y * bounds.h;
            let rect = vg::Rect::new(cx_px - 3.0, cy_px - 3.0, cx_px + 3.0, cy_px + 3.0);
            canvas.draw_path(&vg::Path::rect(rect, None), &handle_paint);
        }
    }
}

/// An animated LFO scope: a sine wave with a moving playhead. Phase is
/// driven externally (the app's shared 60fps timer) via `SynthEvent::Tick`.
pub struct LfoScope {
    state: Signal<SynthState>,
    theme: Signal<ThemeId>,
    phase: Signal<f32>,
}

impl LfoScope {
    pub fn new(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>, phase: Signal<f32>) -> Handle<'_, Self> {
        Self { state, theme, phase }
            .build(cx, |_| {})
            .bind(state, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
            .bind(phase, |mut h| h.needs_redraw())
    }
}

impl View for LfoScope {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();
        let phase = self.phase.get();
        let lfo1 = self.state.get().lfo1;
        let cycles = 1.0 + lfo1.rate_norm * 3.0;
        let amplitude = 0.1 + lfo1.depth * 0.32;

        let mut axis = vg::Paint::default();
        axis.set_color(palette.line);
        axis.set_anti_alias(true);
        canvas.draw_path(
            &vg::Path::rect(vg::Rect::new(bounds.x, bounds.y + bounds.h * 0.5, bounds.x + bounds.w, bounds.y + bounds.h * 0.5 + 1.0), None),
            &axis,
        );

        let n = 160usize;
        let mut path = vg::PathBuilder::new();
        for i in 0..n {
            let t = i as f32 / (n - 1) as f32;
            let y = 0.5 - (t * std::f32::consts::TAU * cycles).sin() * amplitude;
            let p = vg::Point::new(bounds.x + t * bounds.w, bounds.y + y * bounds.h);
            if i == 0 {
                path.move_to(p);
            } else {
                path.line_to(p);
            }
        }
        let mut wave_paint = vg::Paint::default();
        wave_paint.set_color(palette.md);
        wave_paint.set_style(vg::PaintStyle::Stroke);
        wave_paint.set_stroke_width(1.5);
        wave_paint.set_anti_alias(true);
        canvas.draw_path(&path.detach(), &wave_paint);

        let playhead_t = (phase / std::f32::consts::TAU).fract();
        let ph_x = bounds.x + playhead_t * bounds.w;
        let mut ph_paint = vg::Paint::default();
        ph_paint.set_color(palette.ink);
        ph_paint.set_anti_alias(true);
        canvas.draw_path(&vg::Path::rect(vg::Rect::new(ph_x, bounds.y, ph_x + 1.0, bounds.y + bounds.h), None), &ph_paint);
    }
}
