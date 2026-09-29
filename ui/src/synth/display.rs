//! The synth's canvas displays: oscillator waveform, filter response,
//! envelope shape and the LFO scope. One shared colour rule (Strata 2):
//! every shape is a 1.5px `ink` line over a faint `selection` fill - the
//! only colour on a display is modulation (`mod`); grid/axis use
//! grid-beat/line.

use vizia::prelude::*;
use crate::hidpi::Logical;
use vizia::vg;

use shared::synth::{db_height, envelope_points, filter_gain, filter_response_points, harmonics, waveform_points, SynthState};

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
    fill_paint.set_color(palette.selection);
    fill_paint.set_anti_alias(true);
    canvas.save();
    canvas.translate((bounds.x, bounds.y));
    canvas.draw_path(&fill, &fill_paint);

    let mut line_paint = vg::Paint::default();
    line_paint.set_color(palette.ink);
    line_paint.set_style(vg::PaintStyle::Stroke);
    line_paint.set_stroke_width(1.5);
    line_paint.set_anti_alias(true);
    canvas.draw_path(&line, &line_paint);
    canvas.restore();
}

/// An oscillator's waveform, redrawn from its current settings each time
/// they change (no need to animate: the shape only depends on waveform +
/// pulse-width/shape, not on a running phase).
pub struct WaveDisplay {
    state: Memo<SynthState>,
    theme: Signal<ThemeId>,
    extract: fn(&SynthState) -> shared::synth::Oscillator,
}

impl WaveDisplay {
    pub fn new(
        cx: &mut Context,
        state: Memo<SynthState>,
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
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let bounds = cx.lbounds();
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
        let samples = waveform_points(osc.waveform, osc.knob_b, 2.0, n);
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
    state: Memo<SynthState>,
    theme: Signal<ThemeId>,
}

impl FilterDisplay {
    pub fn new(cx: &mut Context, state: Memo<SynthState>, theme: Signal<ThemeId>) -> Handle<'_, Self> {
        Self { state, theme }
            .build(cx, |_| {})
            .bind(state, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
    }
}

impl View for FilterDisplay {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let bounds = cx.lbounds();
        let palette = self.theme.get().palette();
        let state = self.state.get();
        let filter = state.filter;

        // Grid lines at 1/4, 1/2, 3/4 width (~octave markers), matching the
        // preview's three vertical `grid-beat` lines.
        let mut grid = vg::Paint::default();
        grid.set_color(palette.grid_beat);
        grid.set_anti_alias(false);
        for frac in [0.25, 0.5, 0.75] {
            let x = bounds.x + bounds.w * frac;
            canvas.draw_path(&vg::Path::rect(vg::Rect::new(x, bounds.y, x + 1.0, bounds.y + bounds.h), None), &grid);
        }

        // "Passes untouched": the 0 dB line the curve sits on below the
        // cutoff, and resonance rises above.
        let unity_y = bounds.y + bounds.h * (1.0 - db_height(0.0));
        let mut unity = vg::Paint::default();
        unity.set_color(palette.line);
        canvas.draw_path(&vg::Path::rect(vg::Rect::new(bounds.x, unity_y, bounds.x + bounds.w, unity_y + 1.0), None), &unity);

        harmonic_bars(canvas, bounds, &palette, &state);

        let n = 170usize;
        let response = filter_response_points(filter.filter_type, filter.cutoff_hz, filter.resonance, n);
        let points: Vec<(f32, f32)> =
            response.iter().enumerate().map(|(i, &v)| (i as f32 / (n - 1) as f32, 1.0 - v)).collect();

        // Modulation band: a dashed mod-coloured ring around the cutoff's
        // swept range, filled with mod-soft, matching the Cutoff knob's ring.
        let log_min = 20f32.ln();
        let log_max = 20_000f32.ln();
        let cutoff_frac = ((filter.cutoff_hz.max(1.0).ln() - log_min) / (log_max - log_min)).clamp(0.0, 1.0);
        let depth = shared::synth::cutoff_mod_depth(&state);
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
        band_line.set_path_effect(vg::PathEffect::dash(&[3.0, 3.0], 0.0));
        if band_hi > band_lo {
            for frac in [band_lo, band_hi] {
                let x = (bounds.x + bounds.w * frac).round() + 0.5;
                let mut edge = vg::PathBuilder::new();
                edge.move_to(vg::Point::new(x, bounds.y));
                edge.line_to(vg::Point::new(x, bounds.y + bounds.h));
                canvas.draw_path(&edge.detach(), &band_line);
            }
        }

        draw_signal(canvas, bounds, &palette, &points, 1.0);


        // Cutoff marker.
        let mut marker = vg::Paint::default();
        marker.set_color(palette.ink_faint);
        marker.set_anti_alias(true);
        let cutoff_x = bounds.x + cutoff_frac * bounds.w;
        canvas.draw_path(&vg::Path::rect(vg::Rect::new(cutoff_x, bounds.y, cutoff_x + 1.0, bounds.y + bounds.h), None), &marker);

        let font = canvas_font(11.0);
        let mut text_paint = vg::Paint::default();
        text_paint.set_color(palette.ink_muted);
        text_paint.set_anti_alias(true);
        // The cutoff's readout beside its line, on whichever side has room,
        // on a plate so the curve (which peaks right there) never runs
        // through the text.
        let label = format_hz(filter.cutoff_hz);
        let text_w = font.measure_str(&label, None).0;
        let right = cutoff_x + 6.0;
        let label_x = if right + text_w <= bounds.x + bounds.w - 2.0 { right } else { cutoff_x - 6.0 - text_w };
        let mut plate = vg::Paint::default();
        plate.set_color(palette.bg_100);
        plate.set_anti_alias(true);
        let plate_rect = vg::Rect::new(label_x - 3.0, bounds.y + 2.0, label_x + text_w + 3.0, bounds.y + 16.0);
        canvas.draw_path(&vg::Path::rect(plate_rect, None), &plate);
        canvas.draw_str(&label, vg::Point::new(label_x, bounds.y + 13.0), &font, &text_paint);
    }
}

/// The patch's own harmonics behind the filter curve, at the note being
/// held (A2 otherwise): each a green bar as loud as it is after the
/// filter, with a faint tick where it was before - so turning Cutoff shows
/// the high harmonics pressed down, and Resonance lifts the ones near it.
/// Worked out from the knobs (oscillators, their octaves and levels, the
/// sub), not measured, so it always matches this patch alone.
fn harmonic_bars(canvas: &Canvas, bounds: BoundingBox, palette: &crate::tokens::Palette, state: &SynthState) {
    let note = state.held_notes.last().copied().unwrap_or(45);
    let base = 440.0 * 2f32.powf((note as f32 - 69.0) / 12.0);
    let db = |gain: f32| 20.0 * gain.max(1e-9).log10();
    let gain = |level_db: f32| 10f32.powf(level_db / 20.0);
    let mut partials: Vec<(f32, f32)> = Vec::new();
    for (osc, level_db) in [(state.osc1, state.mix.osc1_db), (state.osc2, state.mix.osc2_db)] {
        if level_db <= -59.0 {
            continue;
        }
        let root = base * 2f32.powi(osc.octave as i32);
        partials.extend(harmonics(osc.waveform, 64).into_iter().map(|(n, level)| (root * n as f32, level * gain(level_db))));
    }
    if state.mix.sub_db > -59.0 {
        partials.push((base * 2f32.powi(state.osc1.octave as i32 - 1), gain(state.mix.sub_db)));
    }

    let (log_min, log_max) = (20f32.ln(), 20_000f32.ln());
    let mut after = vg::Paint::default();
    after.set_color(Color::rgba(palette.signal.r(), palette.signal.g(), palette.signal.b(), 110));
    let mut before = vg::Paint::default();
    before.set_color(Color::rgba(palette.ink_faint.r(), palette.ink_faint.g(), palette.ink_faint.b(), 160));
    let floor = bounds.y + bounds.h;
    for (freq, level) in partials {
        if !(20.0..20_000.0).contains(&freq) {
            continue;
        }
        let x = bounds.x + bounds.w * (freq.ln() - log_min) / (log_max - log_min);
        let filtered = level * filter_gain(state.filter.filter_type, state.filter.cutoff_hz, state.filter.resonance, freq);
        let y_after = floor - bounds.h * db_height(db(filtered));
        let y_before = floor - bounds.h * db_height(db(level));
        canvas.draw_path(&vg::Path::rect(vg::Rect::new(x - 1.0, y_after, x + 1.0, floor), None), &after);
        if y_after - y_before > 2.0 {
            canvas.draw_path(&vg::Path::rect(vg::Rect::new(x - 2.0, y_before, x + 2.0, y_before + 1.0), None), &before);
        }
    }
}

fn format_hz(hz: f32) -> String {
    if hz >= 1000.0 {
        format!("{:.2} kHz", hz / 1000.0)
    } else {
        format!("{hz:.0} Hz")
    }
}

/// An ADSR envelope shape, with static 6px ring handles at the attack peak,
/// decay-end and release-start points (display only - dragging them is an
/// alternative input method the spec allows but this milestone doesn't
/// wire; the knobs below each display are the editing path).
pub struct EnvelopeDisplay {
    state: Memo<SynthState>,
    theme: Signal<ThemeId>,
    extract: fn(&SynthState) -> shared::synth::Envelope,
}

impl EnvelopeDisplay {
    pub fn new(
        cx: &mut Context,
        state: Memo<SynthState>,
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
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let bounds = cx.lbounds();
        let palette = self.theme.get().palette();
        let env = (self.extract)(&self.state.get());
        let keypoints = envelope_points(env);
        let points: Vec<(f32, f32)> = keypoints.iter().map(|&(t, level)| (t, 1.0 - level * 0.92)).collect();

        draw_signal(canvas, bounds, &palette, &points, 1.0);

        // 6px rings: a bg-000 centre (the display well) inside an ink edge.
        let mut ring_fill = vg::Paint::default();
        ring_fill.set_color(palette.bg_000);
        ring_fill.set_anti_alias(true);
        let mut ring_edge = vg::Paint::default();
        ring_edge.set_color(palette.ink);
        ring_edge.set_style(vg::PaintStyle::Stroke);
        ring_edge.set_stroke_width(1.25);
        ring_edge.set_anti_alias(true);
        for &(t, y) in &points[1..4] {
            let centre = vg::Point::new(bounds.x + t * bounds.w, bounds.y + y * bounds.h);
            let ring = vg::Path::circle(centre, 3.0, None);
            canvas.draw_path(&ring, &ring_fill);
            canvas.draw_path(&ring, &ring_edge);
        }
    }
}

/// One LFO's scope: a single cycle of its sine, as tall as its Depth, with
/// a marker riding it at the LFO's real current phase (reported by the
/// engine) - so the marker's speed is the Rate and its swing the Depth.
pub struct LfoScope {
    state: Memo<SynthState>,
    theme: Signal<ThemeId>,
    phase: Signal<f32>,
    lfo: fn(&SynthState) -> &shared::synth::Lfo,
}

impl LfoScope {
    pub fn new(
        cx: &mut Context,
        state: Memo<SynthState>,
        theme: Signal<ThemeId>,
        phase: Signal<f32>,
        lfo: fn(&SynthState) -> &shared::synth::Lfo,
    ) -> Handle<'_, Self> {
        Self { state, theme, phase, lfo }
            .build(cx, |_| {})
            .bind(state, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
            .bind(phase, |mut h| h.needs_redraw())
    }
}

impl View for LfoScope {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let bounds = cx.lbounds();
        let palette = self.theme.get().palette();
        let state = self.state.get();
        let depth = (self.lfo)(&state).depth.clamp(0.0, 1.0);
        let amplitude = depth * 0.42;
        let y_at = |t: f32| bounds.y + bounds.h * (0.5 - (t * std::f32::consts::TAU).sin() * amplitude);

        let mut axis = vg::Paint::default();
        axis.set_color(palette.line);
        axis.set_anti_alias(true);
        let mid = bounds.y + bounds.h * 0.5;
        canvas.draw_path(&vg::Path::rect(vg::Rect::new(bounds.x, mid, bounds.x + bounds.w, mid + 1.0), None), &axis);

        let pad = 4.0;
        let x_at = |t: f32| bounds.x + pad + t * (bounds.w - 2.0 * pad);
        let n = 96usize;
        let mut path = vg::PathBuilder::new();
        for i in 0..n {
            let t = i as f32 / (n - 1) as f32;
            let p = vg::Point::new(x_at(t), y_at(t));
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

        // Where the LFO is right now: a hairline plus a dot on the wave.
        let phase = self.phase.get().fract();
        let x = x_at(phase);
        let mut marker = vg::Paint::default();
        marker.set_color(palette.ink_faint);
        marker.set_anti_alias(true);
        canvas.draw_path(&vg::Path::rect(vg::Rect::new(x, bounds.y, x + 1.0, bounds.y + bounds.h), None), &marker);
        let mut dot = vg::Paint::default();
        dot.set_color(palette.ink);
        dot.set_anti_alias(true);
        canvas.draw_path(&vg::Path::circle(vg::Point::new(x + 0.5, y_at(phase)), 3.0, None), &dot);
    }
}
