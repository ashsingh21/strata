//! The Sound match graphs: a target sound (filled) against yours (a line)
//! - either its spectrum (left to right, low to high pitch; up, louder)
//! or its loudness over time.

use std::sync::Arc;

use vizia::prelude::*;
use vizia::vg;

use shared::analysis::{band_count, band_hz, Analysis, ENVELOPE_STEP, FLOOR_DB, HIGH_HZ, LOW_HZ};

use crate::hidpi::Logical;
use crate::tokens::ThemeId;

#[derive(Clone, Copy, PartialEq)]
pub enum Graph {
    Spectrum,
    Loudness,
}

pub struct MatchGraph {
    graph: Graph,
    target: Signal<Option<Arc<Analysis>>>,
    yours: Signal<Option<Arc<Analysis>>>,
    theme: Signal<ThemeId>,
}

/// The loudness graph's floor, and how many seconds it shows.
const LOUDNESS_FLOOR_DB: f32 = -60.0;
const LOUDNESS_SECONDS: f32 = 3.0;

impl MatchGraph {
    pub fn new(
        cx: &mut Context,
        graph: Graph,
        target: Signal<Option<Arc<Analysis>>>,
        yours: Signal<Option<Arc<Analysis>>>,
        theme: Signal<ThemeId>,
    ) -> Handle<'_, Self> {
        Self { graph, target, yours, theme }
            .build(cx, |_| {})
            .bind(target, |mut h| h.needs_redraw())
            .bind(yours, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
    }

    /// The graph's points for `a`, as (0..1 across, 0..1 up).
    fn points(&self, a: &Analysis) -> Vec<(f32, f32)> {
        match self.graph {
            Graph::Spectrum => {
                let octaves = (HIGH_HZ / LOW_HZ).log2();
                (0..band_count().min(a.spectrum.len()))
                    .map(|i| ((band_hz(i) / LOW_HZ).log2() / octaves, (a.spectrum[i] - FLOOR_DB) / -FLOOR_DB))
                    .collect()
            }
            Graph::Loudness => a
                .envelope
                .iter()
                .enumerate()
                .map(|(i, db)| {
                    (i as f32 * ENVELOPE_STEP / LOUDNESS_SECONDS, (db - LOUDNESS_FLOOR_DB) / -LOUDNESS_FLOOR_DB)
                })
                .take_while(|(x, _)| *x <= 1.0)
                .collect(),
        }
    }
}

impl View for MatchGraph {
    fn element(&self) -> Option<&'static str> {
        Some("match-graph")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let b = cx.lbounds();
        crate::hidpi::clip(canvas, b);
        let p = self.theme.get().palette();
        let paint = |color: Color| {
            let mut paint = vg::Paint::default();
            paint.set_color(color);
            paint.set_anti_alias(true);
            paint
        };
        canvas.draw_path(&vg::Path::rect(vg::Rect::new(b.x, b.y, b.x + b.w, b.y + b.h), None), &paint(p.bg_000));

        // Room at the bottom for the scale's labels.
        let (gx, gy, gw, gh) = (b.x, b.y + 16.0, b.w, b.h - 30.0);
        let at = |(x, y): (f32, f32)| vg::Point::new(gx + x * gw, gy + (1.0 - y.clamp(0.0, 1.0)) * gh);
        let font = crate::canvas_text::canvas_font(10.0);
        let label = paint(p.ink_faint);

        // Gridlines and their labels.
        let mut grid = paint(p.line);
        grid.set_style(vg::PaintStyle::Stroke);
        grid.set_stroke_width(1.0);
        let ticks: Vec<(f32, String)> = match self.graph {
            Graph::Spectrum => [(100.0, "100 Hz"), (1000.0, "1 kHz"), (10_000.0, "10 kHz")]
                .into_iter()
                .map(|(hz, s)| ((hz / LOW_HZ).log2() / (HIGH_HZ / LOW_HZ).log2(), s.to_string()))
                .collect(),
            Graph::Loudness => (1..3).map(|s| (s as f32 / LOUDNESS_SECONDS, format!("{s} s"))).collect(),
        };
        for (x, text) in &ticks {
            let px = gx + x * gw;
            let mut line = vg::PathBuilder::new();
            line.move_to((px, gy));
            line.line_to((px, gy + gh));
            canvas.draw_path(&line.detach(), &grid);
            canvas.draw_str(text, vg::Point::new(px + 3.0, b.y + b.h - 4.0), &font, &label);
        }
        let title = match self.graph {
            Graph::Spectrum => "Spectrum: low \u{2192} high pitch",
            Graph::Loudness => "Loudness over time",
        };
        canvas.draw_str(title, vg::Point::new(b.x + 4.0, b.y + 11.0), &font, &label);
        // The key, top right.
        let key_x = b.x + b.w - 100.0;
        canvas.draw_path(&vg::Path::rect(vg::Rect::new(key_x, b.y + 4.0, key_x + 10.0, b.y + 11.0), None), &paint(p.signal));
        canvas.draw_str("Target", vg::Point::new(key_x + 14.0, b.y + 11.0), &font, &label);
        canvas.draw_path(&vg::Path::rect(vg::Rect::new(key_x + 54.0, b.y + 7.0, key_x + 64.0, b.y + 8.5), None), &paint(p.ink));
        canvas.draw_str("Yours", vg::Point::new(key_x + 68.0, b.y + 11.0), &font, &label);

        // The target: a filled shape.
        if let Some(target) = self.target.get() {
            let pts = self.points(&target);
            if let (Some(first), Some(last)) = (pts.first(), pts.last()) {
                let mut area = vg::PathBuilder::new();
                area.move_to(at((first.0, 0.0)));
                for &pt in &pts {
                    area.line_to(at(pt));
                }
                area.line_to(at((last.0, 0.0)));
                area.close();
                canvas.draw_path(&area.detach(), &paint(p.signal_soft));
                let mut edge = vg::PathBuilder::new();
                edge.move_to(at(pts[0]));
                for &pt in &pts[1..] {
                    edge.line_to(at(pt));
                }
                let mut stroke = paint(p.signal);
                stroke.set_style(vg::PaintStyle::Stroke);
                stroke.set_stroke_width(2.0);
                canvas.draw_path(&edge.detach(), &stroke);
            }
        }
        // Yours: a line on top.
        if let Some(yours) = self.yours.get() {
            let pts = self.points(&yours);
            if pts.len() > 1 {
                let mut line = vg::PathBuilder::new();
                line.move_to(at(pts[0]));
                for &pt in &pts[1..] {
                    line.line_to(at(pt));
                }
                let mut stroke = paint(p.ink);
                stroke.set_style(vg::PaintStyle::Stroke);
                stroke.set_stroke_width(1.5);
                canvas.draw_path(&line.detach(), &stroke);
            }
        }
    }
}
