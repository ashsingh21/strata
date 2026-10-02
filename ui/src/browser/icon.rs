//! The browser's icons: the rail's (a 16px box) and the result rows' type
//! marks (12px), stroked like the design's SVGs, drawn on canvas - the app
//! font has no such glyphs.

use vizia::prelude::*;
use vizia::vg;

use crate::hidpi::Logical;
use crate::tokens::{Palette, ThemeId};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IconKind {
    // The rail (16px box).
    Browse,
    Samples,
    Presets,
    Files,
    History,
    Learn,
    Theory,
    Chords,
    Panel,
    Settings,
    /// Strata's mark: layers, like rock strata.
    Logo,
    // Result types (12px box).
    Instrument,
    Preset,
    Sample,
    Effect,
    Pattern,
    Song,
    Track,
    Lesson,
    // Small marks (12px box).
    Search,
    Star,
    StarOutline,
    Play,
    Stop,
    Close,
    Check,
}

impl IconKind {
    /// The design box the coordinates below are in.
    fn view_box(self) -> f32 {
        use IconKind::*;
        match self {
            Browse | Samples | Presets | Files | History | Learn | Theory | Chords | Panel | Settings | Logo => 16.0,
            _ => 12.0,
        }
    }

    /// Filled shapes (stars, play, stop) rather than strokes.
    fn filled(self) -> bool {
        matches!(self, IconKind::Star | IconKind::Play | IconKind::Stop)
    }
}

/// How an icon picks its colour: from the palette and its on state.
pub type IconColor = fn(&Palette, bool) -> Color;

pub fn muted_or_ink(p: &Palette, on: bool) -> Color {
    // `active` is the mod accent (see design/tokens.json).
    if on {
        p.md
    } else {
        p.ink_muted
    }
}

pub struct Icon<M: SignalGet<bool> + Copy + 'static> {
    kind: IconKind,
    on: M,
    theme: Signal<ThemeId>,
    color: IconColor,
}

impl<M: SignalGet<bool> + Copy + 'static> Icon<M> {
    pub fn new(cx: &mut Context, kind: IconKind, size: f32, on: M, theme: Signal<ThemeId>, color: IconColor) -> Handle<'_, Self> {
        Self { kind, on, theme, color }
            .build(cx, |_| {})
            .bind(on, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
            .width(Pixels(size))
            .height(Pixels(size))
            .hoverable(false)
    }
}

impl<M: SignalGet<bool> + Copy + 'static> View for Icon<M> {
    fn element(&self) -> Option<&'static str> {
        Some("browser-icon")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let b = cx.lbounds();
        let color = (self.color)(&self.theme.get().palette(), self.on.get());
        draw_icon(canvas, self.kind, b.x, b.y, b.w.min(b.h), color);
    }
}

/// Draws `kind` into the square at (`x`, `y`) of side `size`.
pub fn draw_icon(canvas: &Canvas, kind: IconKind, x: f32, y: f32, size: f32, color: Color) {
    let s = size / kind.view_box();
    let p = |px: f32, py: f32| vg::Point::new(x + px * s, y + py * s);
    let mut paint = vg::Paint::default();
    paint.set_color(color);
    paint.set_anti_alias(true);
    if kind.filled() {
        paint.set_style(vg::PaintStyle::Fill);
    } else {
        paint.set_style(vg::PaintStyle::Stroke);
        paint.set_stroke_width((if kind.view_box() > 12.0 { 1.4 } else { 1.3 }) * s);
        paint.set_stroke_cap(vg::PaintCap::Round);
        paint.set_stroke_join(vg::PaintJoin::Round);
    }
    let lines = |canvas: &Canvas, segments: &[((f32, f32), (f32, f32))], paint: &vg::Paint| {
        let mut path = vg::PathBuilder::new();
        for &((x0, y0), (x1, y1)) in segments {
            path.move_to(p(x0, y0));
            path.line_to(p(x1, y1));
        }
        canvas.draw_path(&path.detach(), paint);
    };
    let poly = |canvas: &Canvas, points: &[(f32, f32)], close: bool, paint: &vg::Paint| {
        let mut path = vg::PathBuilder::new();
        path.move_to(p(points[0].0, points[0].1));
        for &(px, py) in &points[1..] {
            path.line_to(p(px, py));
        }
        if close {
            path.close();
        }
        canvas.draw_path(&path.detach(), paint);
    };
    let rect = |canvas: &Canvas, rx: f32, ry: f32, w: f32, h: f32, r: f32, paint: &vg::Paint| {
        let rr = vg::RRect::new_rect_xy(vg::Rect::new(x + rx * s, y + ry * s, x + (rx + w) * s, y + (ry + h) * s), r * s, r * s);
        canvas.draw_rrect(rr, paint);
    };
    let circle = |canvas: &Canvas, cx: f32, cy: f32, r: f32, paint: &vg::Paint| {
        canvas.draw_circle(p(cx, cy), r * s, paint);
    };
    use IconKind::*;
    match kind {
        Browse => {
            for (rx, ry) in [(2.0, 2.0), (9.0, 2.0), (2.0, 9.0), (9.0, 9.0)] {
                rect(canvas, rx, ry, 5.0, 5.0, 1.0, &paint);
            }
        }
        Samples => lines(
            canvas,
            &[((2.0, 8.0), (3.5, 8.0)), ((4.5, 5.0), (4.5, 11.0)), ((7.0, 3.0), (7.0, 13.0)), ((9.5, 6.0), (9.5, 10.0)), ((12.0, 4.5), (12.0, 11.5)), ((14.0, 8.0), (14.5, 8.0))],
            &paint,
        ),
        Presets => {
            lines(canvas, &[((4.0, 2.0), (4.0, 14.0)), ((8.0, 2.0), (8.0, 14.0)), ((12.0, 2.0), (12.0, 14.0))], &paint);
            for (rx, ry) in [(2.5, 9.0), (6.5, 4.0), (10.5, 7.0)] {
                rect(canvas, rx, ry, 3.0, 2.0, 0.5, &paint);
            }
        }
        Files => poly(
            canvas,
            &[(2.0, 4.5), (2.0, 12.0), (3.0, 13.0), (13.0, 13.0), (14.0, 12.0), (14.0, 6.0), (13.0, 5.0), (8.0, 5.0), (6.5, 3.5), (3.0, 3.5)],
            true,
            &paint,
        ),
        History => {
            circle(canvas, 8.0, 8.0, 5.5, &paint);
            poly(canvas, &[(8.0, 5.0), (8.0, 8.0), (10.0, 9.5)], false, &paint);
        }
        Learn => {
            poly(canvas, &[(8.0, 5.0), (6.5, 3.5), (2.0, 3.5), (2.0, 12.0), (7.0, 12.0), (8.0, 13.0)], false, &paint);
            poly(canvas, &[(8.0, 5.0), (9.5, 3.5), (14.0, 3.5), (14.0, 12.0), (9.0, 12.0), (8.0, 13.0)], false, &paint);
            lines(canvas, &[((8.0, 5.0), (8.0, 13.0))], &paint);
        }
        // The chord ring: a triangle of notes on a circle.
        Theory => {
            circle(canvas, 8.0, 8.0, 6.0, &paint);
            poly(canvas, &[(8.0, 2.0), (13.2, 11.0), (2.8, 11.0)], true, &paint);
        }
        // Three voices, each stepping to the next chord.
        Chords => {
            poly(canvas, &[(2.0, 4.0), (6.0, 4.0), (10.0, 2.5), (14.0, 2.5)], false, &paint);
            poly(canvas, &[(2.0, 8.0), (14.0, 8.0)], false, &paint);
            poly(canvas, &[(2.0, 13.0), (6.0, 13.0), (10.0, 11.5), (14.0, 11.5)], false, &paint);
        }
        Panel => {
            rect(canvas, 2.0, 2.5, 12.0, 11.0, 1.5, &paint);
            lines(canvas, &[((6.0, 2.5), (6.0, 13.5))], &paint);
        }
        Settings => {
            circle(canvas, 8.0, 8.0, 2.0, &paint);
            lines(
                canvas,
                &[
                    ((8.0, 1.8), (8.0, 3.8)),
                    ((8.0, 12.2), (8.0, 14.2)),
                    ((1.8, 8.0), (3.8, 8.0)),
                    ((12.2, 8.0), (14.2, 8.0)),
                    ((3.6, 3.6), (5.0, 5.0)),
                    ((11.0, 11.0), (12.4, 12.4)),
                    ((3.6, 12.4), (5.0, 11.0)),
                    ((11.0, 5.0), (12.4, 3.6)),
                ],
                &paint,
            );
        }
        Logo => {
            let mut thick = paint.clone();
            thick.set_stroke_width(1.8 * s);
            lines(canvas, &[((3.0, 4.5), (13.0, 4.5)), ((2.0, 8.0), (11.0, 8.0)), ((5.0, 11.5), (14.0, 11.5))], &thick);
        }
        Instrument => {
            circle(canvas, 6.0, 6.0, 4.2, &paint);
            lines(canvas, &[((6.0, 6.0), (8.3, 3.7))], &paint);
        }
        Preset => {
            lines(canvas, &[((3.0, 1.5), (3.0, 10.5)), ((6.0, 1.5), (6.0, 10.5)), ((9.0, 1.5), (9.0, 10.5))], &paint);
            let mut thick = paint.clone();
            thick.set_stroke_width(2.0 * s);
            lines(canvas, &[((2.0, 7.0), (4.0, 7.0)), ((5.0, 3.5), (7.0, 3.5)), ((8.0, 5.5), (10.0, 5.5))], &thick);
        }
        Sample => lines(
            canvas,
            &[((1.0, 6.0), (2.0, 6.0)), ((3.0, 4.0), (3.0, 8.0)), ((5.0, 2.0), (5.0, 10.0)), ((7.0, 4.5), (7.0, 7.5)), ((9.0, 3.0), (9.0, 9.0)), ((11.0, 6.0), (11.2, 6.0))],
            &paint,
        ),
        Effect => {
            rect(canvas, 1.5, 3.0, 4.0, 6.0, 1.0, &paint);
            rect(canvas, 6.5, 3.0, 4.0, 6.0, 1.0, &paint);
        }
        Pattern => {
            for (rx, ry) in [(1.5, 1.5), (6.5, 1.5), (1.5, 6.5), (6.5, 6.5)] {
                rect(canvas, rx, ry, 4.0, 4.0, 0.8, &paint);
            }
        }
        Song => {
            poly(canvas, &[(4.5, 9.5), (4.5, 2.5), (10.0, 1.5), (10.0, 8.5)], false, &paint);
            circle(canvas, 3.3, 9.5, 1.3, &paint);
            circle(canvas, 8.8, 8.5, 1.3, &paint);
        }
        Track => poly(canvas, &[(3.0, 1.5), (7.5, 1.5), (9.5, 3.5), (9.5, 10.5), (3.0, 10.5)], true, &paint),
        Lesson => {
            poly(canvas, &[(6.0, 3.5), (5.0, 2.5), (1.5, 2.5), (1.5, 9.0), (5.2, 9.0), (6.0, 10.0)], false, &paint);
            poly(canvas, &[(6.0, 3.5), (7.0, 2.5), (10.5, 2.5), (10.5, 9.0), (6.8, 9.0), (6.0, 10.0)], false, &paint);
        }
        Search => {
            circle(canvas, 5.2, 5.2, 3.4, &paint);
            lines(canvas, &[((7.8, 7.8), (10.5, 10.5))], &paint);
        }
        Star | StarOutline => poly(
            canvas,
            &[(6.0, 1.2), (7.4, 4.3), (10.7, 4.6), (8.2, 6.8), (9.0, 10.0), (6.0, 8.3), (3.0, 10.0), (3.8, 6.8), (1.3, 4.6), (4.6, 4.3)],
            true,
            &paint,
        ),
        Play => poly(canvas, &[(3.0, 1.8), (3.0, 10.2), (10.0, 6.0)], true, &paint),
        Stop => rect(canvas, 2.5, 2.5, 7.0, 7.0, 1.0, &paint),
        Close => lines(canvas, &[((3.0, 3.0), (9.0, 9.0)), ((9.0, 3.0), (3.0, 9.0))], &paint),
        Check => poly(canvas, &[(2.5, 6.5), (5.0, 9.0), (9.5, 3.5)], false, &paint),
    }
}
