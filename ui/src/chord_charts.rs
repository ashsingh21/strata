//! The Guitar side of Voice leading: each chord of the progression as a
//! chord chart - six strings, the frets, a dot per finger with which
//! finger, open and unplayed strings above the nut, the notes it sounds
//! below. The shapes are chosen together so the hand moves least; a
//! finger that stays where it was is drawn in the signal colour. Press a
//! chart to hear it.

use vizia::prelude::*;
use vizia::vg;

use shared::theory::guitar::{Shape, STRING_NAMES};
use shared::theory::note_name_for_key;
use shared::theory::voicing;

use crate::hidpi::Logical;
use crate::synth::state::SynthEvent;
use crate::voicing::{on_guitar, VoicingProps};

/// Frets drawn on each chart.
const FRETS: u8 = 5;
const CARD_MIN_W: f32 = 168.0;
const CARD_MAX_W: f32 = 236.0;
const GAP: f32 = 34.0;
const CARD_H: f32 = 330.0;

pub struct ChordCharts {
    p: VoicingProps,
    held: Vec<u8>,
    pressed: Option<usize>,
}

impl ChordCharts {
    pub fn new(cx: &mut Context, p: VoicingProps) -> Handle<'_, Self> {
        Self { p, held: Vec::new(), pressed: None }
            .build(cx, |_| {})
            .bind(p.progression, |mut h| h.needs_redraw())
            .bind(p.sevenths, |mut h| h.needs_redraw())
            .bind(p.key, |mut h| h.needs_redraw())
            .bind(p.scale_mask, |mut h| h.needs_redraw())
            .bind(p.theme, |mut h| h.needs_redraw())
    }

    fn chords(&self) -> Vec<(usize, voicing::Chord, Option<Shape>)> {
        on_guitar(&self.p.progression.get(), self.p.key.get(), self.p.scale_mask.get(), self.p.sevenths.get())
    }

    fn release(&mut self, cx: &mut EventContext) {
        for note in std::mem::take(&mut self.held) {
            cx.emit(SynthEvent::KeyRelease(note));
        }
        self.pressed = None;
    }
}

/// Where each chart goes in `b`: in rows, as wide as fits, centred.
fn layout(n: usize, b: BoundingBox) -> Vec<vg::Rect> {
    if n == 0 {
        return Vec::new();
    }
    let usable = b.w - 2.0 * GAP;
    let per_row = (((usable + GAP) / (CARD_MIN_W + GAP)).floor() as usize).clamp(1, n);
    let w = ((usable - GAP * (per_row - 1) as f32) / per_row as f32).min(CARD_MAX_W);
    let h = CARD_H.min(w * 1.6);
    let rows = n.div_ceil(per_row);
    let total_h = rows as f32 * h + (rows - 1) as f32 * GAP;
    let top = b.y + ((b.h - total_h) / 2.0).max(GAP / 2.0);
    (0..n)
        .map(|i| {
            let (row, col) = (i / per_row, i % per_row);
            let in_row = (n - row * per_row).min(per_row);
            let row_w = in_row as f32 * w + (in_row - 1) as f32 * GAP;
            let x = b.x + (b.w - row_w) / 2.0 + col as f32 * (w + GAP);
            let y = top + row as f32 * (h + GAP);
            vg::Rect::new(x, y, x + w, y + h)
        })
        .collect()
}

impl View for ChordCharts {
    fn element(&self) -> Option<&'static str> {
        Some("chord-charts")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, _| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                let chords = self.chords();
                let (mx, my) = cx.lmouse();
                let hit = layout(chords.len(), cx.lbounds())
                    .iter()
                    .position(|r| mx >= r.left && mx <= r.right && my >= r.top && my <= r.bottom);
                if let Some(i) = hit {
                    self.release(cx);
                    if let Some(shape) = chords[i].2 {
                        self.held = shape.notes();
                        for &note in &self.held {
                            cx.emit(SynthEvent::KeyPress(note));
                        }
                    }
                    self.pressed = Some(i);
                    cx.capture();
                    cx.needs_redraw();
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                self.release(cx);
                cx.release();
                cx.needs_redraw();
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let b = cx.lbounds();
        crate::hidpi::clip(canvas, b);
        let pal = self.p.theme.get().palette();
        let key = self.p.key.get();
        let chords = self.chords();
        if chords.is_empty() {
            let line = "Add chords above. Each gets a guitar shape picked so your hand moves as little as it can from the one before.";
            let w = crate::canvas_text::canvas_font(14.0).measure_str(line, None).0;
            text(canvas, line, b.x + (b.w - w) / 2.0, b.y + b.h / 2.0, 14.0, pal.ink_muted);
            return;
        }
        let rects = layout(chords.len(), b);
        for (i, ((degree, chord, shape), rect)) in chords.iter().zip(&rects).enumerate() {
            let prev = if i > 0 { chords[i - 1].2 } else { None };
            let card = Card { rect: *rect, pressed: self.pressed == Some(i), pal: &pal, key };
            card.draw(canvas, *degree, chord, shape.as_ref(), prev.as_ref(), i);
            // A small arrow to the next chart, when it's beside this one.
            if let Some(next) = rects.get(i + 1) {
                if (next.top - rect.top).abs() < 1.0 {
                    let y = rect.top + rect.height() / 2.0;
                    arrow(canvas, rect.right + 8.0, next.left - 8.0, y, pal.ink_faint);
                }
            }
        }
    }
}

struct Card<'a> {
    rect: vg::Rect,
    pressed: bool,
    pal: &'a crate::tokens::Palette,
    key: u8,
}

impl Card<'_> {
    fn draw(&self, canvas: &Canvas, degree: usize, chord: &voicing::Chord, shape: Option<&Shape>, prev: Option<&Shape>, index: usize) {
        let pal = self.pal;
        let r = self.rect;
        let mut paint = vg::Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(if self.pressed { pal.bg_200 } else { pal.bg_100 });
        canvas.draw_rrect(vg::RRect::new_rect_xy(r, 10.0, 10.0), &paint);
        paint.set_style(vg::PaintStyle::Stroke);
        paint.set_stroke_width(if self.pressed { 2.0 } else { 1.0 });
        paint.set_color(if self.pressed { pal.signal } else { pal.line });
        canvas.draw_rrect(vg::RRect::new_rect_xy(r, 10.0, 10.0), &paint);

        // The name, large, and its number in the key.
        let name = voicing::name(chord, self.key).replace('\u{266d}', "b");
        text(canvas, &name, r.left + 16.0, r.top + 34.0, 24.0, pal.ink);
        let numeral = voicing::numeral(degree, chord).replace('\u{b0}', "o");
        let font = crate::canvas_text::canvas_font(12.0);
        let w = font.measure_str(&numeral, None).0;
        text(canvas, &numeral, r.right - 16.0 - w, r.top + 30.0, 12.0, pal.ink_faint);
        let order = format!("{}", index + 1);
        let w = font.measure_str(&order, None).0;
        text(canvas, &order, r.right - 16.0 - w, r.top + 16.0, 10.0, pal.ink_faint);

        let Some(shape) = shape else {
            text(canvas, "No easy shape", r.left + 16.0, r.top + 80.0, 12.0, pal.ink_muted);
            return;
        };

        // The grid: strings across, frets down.
        let margin = (r.width() * 0.16).max(26.0);
        let left = r.left + margin;
        let right = r.right - margin * 0.7;
        let sx = (right - left) / 5.0;
        let top = r.top + 74.0;
        let bottom = r.bottom - 74.0;
        let fh = (bottom - top) / FRETS as f32;
        let string_x = |s: usize| left + sx * s as f32;
        // Open position starts at the nut; anything higher starts at its
        // lowest fret, numbered.
        let first = if shape.high_fret() <= FRETS - 1 { 1 } else { shape.low_fret() };
        let fret_y = |f: u8| top + fh * (f as f32 - first as f32 + 0.5);

        // Frets.
        paint.set_style(vg::PaintStyle::Stroke);
        paint.set_color(pal.line_control);
        paint.set_stroke_width(1.5);
        for i in 0..=FRETS {
            let y = top + fh * i as f32;
            line(canvas, string_x(0), y, string_x(5), y, &paint);
        }
        if first == 1 {
            paint.set_color(pal.ink);
            paint.set_stroke_width(5.0);
            paint.set_stroke_cap(vg::paint::Cap::Round);
            line(canvas, string_x(0), top - 1.0, string_x(5), top - 1.0, &paint);
            paint.set_stroke_cap(vg::paint::Cap::Butt);
        } else {
            let label = format!("{first}fr");
            let w = crate::canvas_text::canvas_font(12.0).measure_str(&label, None).0;
            text(canvas, &label, string_x(0) - 10.0 - w, fret_y(first) + 4.0, 12.0, pal.ink_muted);
        }
        // Strings, thicker at the bass.
        paint.set_color(pal.ink_muted);
        for s in 0..6 {
            paint.set_stroke_width(2.4 - 0.28 * s as f32);
            line(canvas, string_x(s), top, string_x(s), bottom, &paint);
        }

        // Above the nut: o for open, x for not played.
        let mark_y = top - 16.0;
        for s in 0..6 {
            let x = string_x(s);
            let mut p = vg::Paint::default();
            p.set_anti_alias(true);
            p.set_style(vg::PaintStyle::Stroke);
            p.set_stroke_width(1.6);
            match shape.frets[s] {
                None => {
                    p.set_color(pal.ink_faint);
                    line(canvas, x - 4.5, mark_y - 4.5, x + 4.5, mark_y + 4.5, &p);
                    line(canvas, x - 4.5, mark_y + 4.5, x + 4.5, mark_y - 4.5, &p);
                }
                Some(0) => {
                    p.set_color(pal.ink);
                    canvas.draw_circle(vg::Point::new(x, mark_y), 5.5, &p);
                }
                _ => {}
            }
        }

        // Fingers. One that stays where it was in the chord before is in
        // the signal colour.
        let stays = |s: usize| prev.is_some_and(|p| shape.frets[s].is_some_and(|f| f > 0) && p.frets[s] == shape.frets[s]);
        let radius = (sx.min(fh) * 0.36).min(13.0);
        let fingers = shape.fingers();
        let mut dot = vg::Paint::default();
        dot.set_anti_alias(true);
        if let Some(barre) = shape.barre() {
            let y = fret_y(barre.fret);
            let all_stay = (barre.from..=barre.to).filter(|&s| shape.frets[s] == Some(barre.fret)).all(stays);
            dot.set_color(if all_stay { pal.signal } else { pal.ink });
            let rect = vg::Rect::new(string_x(barre.from) - radius, y - radius, string_x(barre.to) + radius, y + radius);
            canvas.draw_rrect(vg::RRect::new_rect_xy(rect, radius, radius), &dot);
        }
        let finger_font = crate::canvas_text::canvas_font(radius * 1.05);
        for s in 0..6 {
            let Some(f) = shape.frets[s].filter(|f| *f > 0) else { continue };
            let (x, y) = (string_x(s), fret_y(f));
            let on_barre = shape.barre().is_some_and(|b| b.fret == f && (b.from..=b.to).contains(&s));
            let fill = if stays(s) { pal.signal } else { pal.ink };
            if !on_barre {
                dot.set_color(fill);
                canvas.draw_circle(vg::Point::new(x, y), radius, &dot);
            }
            if let Some(n) = fingers[s].filter(|_| !on_barre || s == shape.barre().map(|b| b.from).unwrap_or(s)) {
                let label = n.to_string();
                let w = finger_font.measure_str(&label, None).0;
                text(canvas, &label, x - w / 2.0, y + radius * 0.38, radius * 1.05, pal.bg_000);
            }
        }

        // Below: what each string sounds, the bass (the root) brighter,
        // and the string's own name under that.
        let note_y = bottom + 22.0;
        let font = crate::canvas_text::canvas_font(12.0);
        let small = crate::canvas_text::canvas_font(10.0);
        let prev_notes = prev.map(|p| p.notes()).unwrap_or_default();
        let bass = shape.frets.iter().position(|f| f.is_some());
        for s in 0..6 {
            let x = string_x(s);
            if let Some(pitch) = shape.pitch(s) {
                let label = note_name_for_key(pitch % 12, self.key).replace('\u{266d}', "b");
                let w = font.measure_str(&label, None).0;
                let kept = prev_notes.contains(&pitch);
                let color = if kept { pal.signal } else if Some(s) == bass { pal.ink } else { pal.ink_muted };
                text(canvas, &label, x - w / 2.0, note_y, 12.0, color);
            }
            let w = small.measure_str(STRING_NAMES[s], None).0;
            text(canvas, STRING_NAMES[s], x - w / 2.0, note_y + 16.0, 10.0, pal.ink_faint);
        }

        // What changes from the chord before.
        let footer = match prev {
            None => "Start here".to_string(),
            Some(p) => {
                let kept = (0..6).filter(|&s| shape.frets[s].is_some_and(|f| f > 0) && p.frets[s] == shape.frets[s]).count();
                let moved = (shape.low_fret() as i32 - p.low_fret() as i32).abs();
                match (kept, moved) {
                    (0, m) if m >= 3 => format!("Slide up or down {m} frets"),
                    (0, _) => "New shape, same place".to_string(),
                    (1, _) => "Keep 1 finger down".to_string(),
                    (k, _) => format!("Keep {k} fingers down"),
                }
            }
        };
        let w = font.measure_str(&footer, None).0;
        text(canvas, &footer, r.left + (r.width() - w) / 2.0, r.bottom - 14.0, 12.0, pal.ink_muted);
    }
}

fn line(canvas: &Canvas, x0: f32, y0: f32, x1: f32, y1: f32, paint: &vg::Paint) {
    let mut path = vg::PathBuilder::new();
    path.move_to(vg::Point::new(x0, y0));
    path.line_to(vg::Point::new(x1, y1));
    canvas.draw_path(&path.detach(), paint);
}

fn arrow(canvas: &Canvas, x0: f32, x1: f32, y: f32, color: Color) {
    if x1 - x0 < 8.0 {
        return;
    }
    let mut paint = vg::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(vg::PaintStyle::Stroke);
    paint.set_stroke_width(1.5);
    paint.set_color(color);
    paint.set_stroke_cap(vg::paint::Cap::Round);
    line(canvas, x0, y, x1, y, &paint);
    line(canvas, x1 - 6.0, y - 5.0, x1, y, &paint);
    line(canvas, x1 - 6.0, y + 5.0, x1, y, &paint);
}

fn text(canvas: &Canvas, s: &str, x: f32, y: f32, size: f32, color: Color) {
    let font = crate::canvas_text::canvas_font(size);
    let mut paint = vg::Paint::default();
    paint.set_color(color);
    paint.set_anti_alias(true);
    canvas.draw_str(s, vg::Point::new(x, y), &font, &paint);
}
