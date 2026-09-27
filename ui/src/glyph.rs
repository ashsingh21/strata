//! Strata's icons: filled geometry on a 12px box (play triangle, stop
//! square, record dot, ...) plus the four waveform shapes, drawn on canvas
//! rather than taken from a font - IBM Plex has no such glyphs, and the
//! fallback font drew ▶ as a colour emoji.

use vizia::prelude::*;
use vizia::vg;

use crate::tokens::{Palette, ThemeId};

#[derive(Clone, Copy, PartialEq)]
pub enum GlyphKind {
    Sine,
    Triangle,
    Saw,
    Square,
    Play,
    Stop,
    Record,
    Rewind,
    Loop,
    Metronome,
    /// A microphone: the recording input.
    Mic,
}

/// Picks the glyph's colour from the current palette and its on/off state.
pub type GlyphColor = fn(&Palette, bool) -> Color;

/// Ink when on, muted ink when off - segmented-control choices.
pub fn ink_when_on(p: &Palette, on: bool) -> Color {
    if on {
        p.ink
    } else {
        p.ink_muted
    }
}

pub struct Glyph<M: SignalGet<bool> + Copy + 'static> {
    kind: GlyphKind,
    on: M,
    theme: Signal<ThemeId>,
    color: GlyphColor,
}

impl<M: SignalGet<bool> + Copy + 'static> Glyph<M> {
    pub fn new(cx: &mut Context, kind: GlyphKind, on: M, theme: Signal<ThemeId>, color: GlyphColor) -> Handle<'_, Self> {
        Self { kind, on, theme, color }
            .build(cx, |_| {})
            .bind(on, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
            .width(Pixels(12.0))
            .height(Pixels(12.0))
            .hoverable(false)
    }
}

impl<M: SignalGet<bool> + Copy + 'static> View for Glyph<M> {
    fn element(&self) -> Option<&'static str> {
        Some("glyph")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let b = cx.bounds();
        let color = (self.color)(&self.theme.get().palette(), self.on.get());
        // All geometry is drawn in a 12-unit box, scaled to fill whatever
        // size the caller gave it (the default is a 12px box, so `unit`
        // is 1px there; a caller asking for a bigger glyph - the header's
        // transport icons - gets everything, strokes included, scaled up
        // with it instead of a bigger box around a still-tiny icon).
        let size = b.w.min(b.h);
        let unit = size / 12.0;
        let ox = b.x + (b.w - size) * 0.5;
        let oy = b.y + (b.h - size) * 0.5;
        let pt = |x: f32, y: f32| vg::Point::new(ox + x * unit, oy + y * unit);

        let mut fill = vg::Paint::default();
        fill.set_color(color);
        fill.set_anti_alias(true);
        let mut stroke = fill.clone();
        stroke.set_style(vg::PaintStyle::Stroke);
        stroke.set_stroke_width(1.25 * unit);
        stroke.set_stroke_join(vg::PaintJoin::Round);
        stroke.set_stroke_cap(vg::PaintCap::Round);

        let poly = |points: &[(f32, f32)]| {
            let mut p = vg::PathBuilder::new();
            for (i, &(x, y)) in points.iter().enumerate() {
                if i == 0 {
                    p.move_to(pt(x, y));
                } else {
                    p.line_to(pt(x, y));
                }
            }
            p
        };

        match self.kind {
            GlyphKind::Play => {
                let mut p = poly(&[(3.0, 2.0), (10.5, 6.0), (3.0, 10.0)]);
                p.close();
                canvas.draw_path(&p.detach(), &fill);
            }
            GlyphKind::Stop => {
                canvas.draw_path(&vg::Path::rect(vg::Rect::new(ox + 2.5 * unit, oy + 2.5 * unit, ox + 9.5 * unit, oy + 9.5 * unit), None), &fill);
            }
            GlyphKind::Record => {
                canvas.draw_path(&vg::Path::circle(pt(6.0, 6.0), 3.75 * unit, None), &fill);
            }
            GlyphKind::Rewind => {
                canvas.draw_path(&vg::Path::rect(vg::Rect::new(ox + 2.0 * unit, oy + 2.5 * unit, ox + 3.5 * unit, oy + 9.5 * unit), None), &fill);
                let mut p = poly(&[(10.0, 2.5), (4.0, 6.0), (10.0, 9.5)]);
                p.close();
                canvas.draw_path(&p.detach(), &fill);
            }
            GlyphKind::Loop => {
                // Two arrows chasing round a rounded rectangle.
                let mut top = poly(&[(2.0, 7.0), (2.0, 3.5), (9.0, 3.5)]);
                canvas.draw_path(&top.detach(), &stroke);
                let mut head = poly(&[(8.0, 1.5), (10.5, 3.5), (8.0, 5.5)]);
                head.close();
                canvas.draw_path(&head.detach(), &fill);
                let mut bottom = poly(&[(10.0, 5.0), (10.0, 8.5), (3.0, 8.5)]);
                canvas.draw_path(&bottom.detach(), &stroke);
                let mut head = poly(&[(4.0, 6.5), (1.5, 8.5), (4.0, 10.5)]);
                head.close();
                canvas.draw_path(&head.detach(), &fill);
            }
            GlyphKind::Mic => {
                // Capsule, the stand's cradle, then its stem and foot.
                let capsule = vg::RRect::new_rect_xy(
                    vg::Rect::new(ox + 4.25 * unit, oy + 1.25 * unit, ox + 7.75 * unit, oy + 7.25 * unit),
                    1.75 * unit,
                    1.75 * unit,
                );
                canvas.draw_rrect(capsule, &stroke);
                let mut cradle =
                    poly(&[(2.75, 5.5), (3.1, 7.2), (4.3, 8.5), (6.0, 9.0), (7.7, 8.5), (8.9, 7.2), (9.25, 5.5)]);
                canvas.draw_path(&cradle.detach(), &stroke);
                let mut stand = poly(&[(6.0, 9.0), (6.0, 10.75)]);
                canvas.draw_path(&stand.detach(), &stroke);
                let mut foot = poly(&[(4.25, 10.75), (7.75, 10.75)]);
                canvas.draw_path(&foot.detach(), &stroke);
            }
            GlyphKind::Metronome => {
                let mut body = poly(&[(4.5, 1.5), (7.5, 1.5), (10.0, 10.5), (2.0, 10.5)]);
                body.close();
                canvas.draw_path(&body.detach(), &stroke);
                let mut arm = poly(&[(6.0, 8.0), (9.5, 3.0)]);
                canvas.draw_path(&arm.detach(), &stroke);
            }
            GlyphKind::Sine | GlyphKind::Triangle | GlyphKind::Saw | GlyphKind::Square => {
                let points: Vec<(f32, f32)> = match self.kind {
                    GlyphKind::Sine => (0..=24)
                        .map(|i| {
                            let t = i as f32 / 24.0;
                            (1.0 + t * 10.0, 6.0 - (t * std::f32::consts::TAU).sin() * 3.5)
                        })
                        .collect(),
                    GlyphKind::Triangle => vec![(1.0, 6.0), (3.5, 2.5), (8.5, 9.5), (11.0, 6.0)],
                    GlyphKind::Saw => vec![(1.0, 9.5), (6.0, 2.5), (6.0, 9.5), (11.0, 2.5), (11.0, 9.5)],
                    _ => vec![(1.0, 9.5), (1.0, 2.5), (6.0, 2.5), (6.0, 9.5), (11.0, 9.5), (11.0, 2.5)],
                };
                let mut wave = poly(&points);
                canvas.draw_path(&wave.detach(), &stroke);
            }
        }
    }
}
