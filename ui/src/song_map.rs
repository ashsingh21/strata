//! The Map: the song as places instead of a line. Each section (from the
//! markers, or found where the parts playing change) is a card, higher up
//! the more is going on in it, listing the parts in it; the arrows between
//! them say what comes in and what drops out. Click a section to go
//! there; while the song plays, the click is queued and the move happens
//! at the next four-bar line, so it lands on the phrase (a way to perform
//! the song in a different order). The sections themselves are
//! `shared::sections`.

use vizia::prelude::*;
use vizia::vg;

use shared::arrangement::{Arrangement, Ticks, PPQ};
use shared::sections::{self, Section};

use crate::hidpi::Logical;
use crate::tokens::{self, ThemeId};

const BAR: Ticks = 4 * PPQ;
/// Queued moves happen on this grid: a phrase.
const PHRASE: Ticks = 4 * BAR;

pub enum MapEvent {
    Toggle,
    Close,
    /// A section clicked: go there now, or (playing) at the next phrase.
    Pick(usize),
    Tick,
}

thread_local! {
    static OPEN: std::cell::Cell<Option<Signal<bool>>> = const { std::cell::Cell::new(None) };
}

/// Whether the Map is showing, for the header's button.
pub fn open_signal() -> Option<Signal<bool>> {
    OPEN.get()
}

pub struct MapModel {
    pub open: Signal<bool>,
    /// The section to move to at the next phrase, while playing.
    pub queued: Signal<Option<usize>>,
    arrangement: Signal<Arrangement>,
    playhead: Signal<Ticks>,
    playing: Signal<bool>,
    last_tick: Ticks,
}

impl MapModel {
    pub fn new(arrangement: Signal<Arrangement>, playhead: Signal<Ticks>, playing: Signal<bool>) -> Self {
        let open = Signal::new(false);
        OPEN.set(Some(open));
        Self { open, queued: Signal::new(None), arrangement, playhead, playing, last_tick: 0 }
    }
}

impl Model for MapModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            MapEvent::Toggle => self.open.set(!self.open.get()),
            MapEvent::Close => self.open.set(false),
            MapEvent::Pick(i) => {
                let all = sections::sections(&self.arrangement.get());
                let Some(s) = all.get(*i) else { return };
                if self.playing.get() {
                    // Pick it again to cancel.
                    self.queued.set(if self.queued.get() == Some(*i) { None } else { Some(*i) });
                } else {
                    cx.emit(crate::app::AppEvent::Seek(s.start));
                }
            }
            MapEvent::Tick => {
                let now = self.playhead.get();
                let before = self.last_tick;
                self.last_tick = now;
                let Some(i) = self.queued.get() else { return };
                if !self.playing.get() {
                    self.queued.set(None);
                    return;
                }
                // Crossed a phrase line since the last frame: go.
                let crossed = now > before && now / PHRASE != before / PHRASE;
                if crossed {
                    if let Some(s) = sections::sections(&self.arrangement.get()).get(i) {
                        cx.emit(crate::app::AppEvent::Seek(s.start));
                    }
                    self.queued.set(None);
                }
            }
        });
    }
}

#[derive(Clone, Copy)]
pub struct MapProps {
    pub open: Signal<bool>,
    pub queued: Signal<Option<usize>>,
    pub arrangement: Signal<Arrangement>,
    pub playhead: Signal<Ticks>,
    pub playing: Signal<bool>,
    pub theme: Signal<ThemeId>,
}

impl MapProps {
    pub fn of(m: &MapModel, theme: Signal<ThemeId>) -> Self {
        Self { open: m.open, queued: m.queued, arrangement: m.arrangement, playhead: m.playhead, playing: m.playing, theme }
    }
}

/// Mounted over the timeline; built fresh each time it opens (a view
/// shown again after being hidden can come back blank).
pub fn map_view(cx: &mut Context, p: MapProps) {
    Binding::new(cx, p.open, move |cx| {
        if !p.open.get() {
            return;
        }
        VStack::new(cx, move |cx| {
            HStack::new(cx, move |cx| {
                Label::new(cx, "Map").class("heading");
                Label::new(
                    cx,
                    p.playing.map(|on| {
                        if *on {
                            "Click a section to go there at the next 4-bar line. Higher up is busier."
                        } else {
                            "Each card is a section, higher up the busier it is; the arrows say what changes. Click one to go there."
                        }
                    }),
                )
                .class("value");
                Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
                Button::new(cx, |cx| Label::new(cx, "Back to the timeline")).class("btn").on_press(|cx| cx.emit(MapEvent::Close));
            })
            .gap(Pixels(tokens::SPACE_3))
            .alignment(Alignment::Left)
            .padding_left(Pixels(tokens::SPACE_4))
            .padding_right(Pixels(tokens::SPACE_4))
            .width(Stretch(1.0))
            .height(Pixels(44.0));
            MapCanvas::new(cx, p).width(Stretch(1.0)).height(Stretch(1.0));
        })
        .class("map-view")
        .position_type(PositionType::Absolute)
        .z_index(40)
        .width(Stretch(1.0))
        .height(Stretch(1.0));
    });
}

/// Where each section's card goes in `b`: left to right in song order,
/// higher the busier.
fn layout(sections: &[Section], b: BoundingBox) -> Vec<vg::Rect> {
    let n = sections.len().max(1) as f32;
    let pad = 40.0;
    // Room between the cards for the arrows; less when there are many.
    let gap = if n > 7.0 { 22.0 } else { 40.0 };
    let w = ((b.w - 2.0 * pad - gap * (n - 1.0)) / n).clamp(78.0, 200.0);
    // Tall enough for every card's parts, one chip a row at worst.
    let rows = sections.iter().map(|s| chip_rows(s, w)).max().unwrap_or(1) as f32;
    let h = 58.0 + 20.0 * rows + 6.0;
    let total = n * w + (n - 1.0) * gap;
    let x0 = b.x + ((b.w - total) / 2.0).max(pad);
    let (top, bottom) = (b.y + 40.0, b.y + b.h - h - 56.0);
    sections
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let x = x0 + i as f32 * (w + gap);
            let y = bottom - s.energy * (bottom - top).max(0.0);
            vg::Rect::from_xywh(x, y, w, h)
        })
        .collect()
}

/// How many rows a section's part chips take in a card `w` wide.
fn chip_rows(s: &Section, w: f32) -> usize {
    let font = crate::canvas_text::canvas_font(11.0);
    let (mut x, mut rows) = (0.0, 1);
    for l in &s.layers {
        let cw = font.measure_str(&l.name, None).0 + 14.0;
        if x + cw > w - 16.0 && x > 0.0 {
            rows += 1;
            x = 0.0;
        }
        x += cw;
    }
    rows
}

struct MapCanvas {
    p: MapProps,
}

impl MapCanvas {
    fn new(cx: &mut Context, p: MapProps) -> Handle<'_, Self> {
        Self { p }
            .build(cx, |_| {})
            .bind(p.arrangement, |mut h| h.needs_redraw())
            .bind(p.playhead, |mut h| h.needs_redraw())
            .bind(p.queued, |mut h| h.needs_redraw())
            .bind(p.playing, |mut h| h.needs_redraw())
            .bind(p.theme, |mut h| h.needs_redraw())
    }
}

impl View for MapCanvas {
    fn element(&self) -> Option<&'static str> {
        Some("song-map")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, _| {
            if let WindowEvent::MouseDown(MouseButton::Left) = window_event {
                let all = sections::sections(&self.p.arrangement.get());
                let (mx, my) = cx.lmouse();
                if let Some(i) = layout(&all, cx.lbounds()).iter().position(|r| mx >= r.left && mx <= r.right && my >= r.top && my <= r.bottom) {
                    cx.emit(MapEvent::Pick(i));
                }
            }
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let b = cx.lbounds();
        crate::hidpi::clip(canvas, b);
        let pal = self.p.theme.get().palette();
        fill(canvas, vg::Rect::from_xywh(b.x, b.y, b.w, b.h), pal.bg_000, 0.0);
        let all = sections::sections(&self.p.arrangement.get());
        if all.is_empty() {
            text(canvas, "Nothing here yet: add some clips, and their sections show up as places.", b.x + 40.0, b.y + 60.0, 13.0, pal.ink_muted);
            return;
        }
        let rects = layout(&all, b);
        let playing_now = sections::at(&all, self.p.playhead.get());
        let queued = self.p.queued.get();
        text(canvas, "busier", b.x + 8.0, b.y + 52.0, 11.0, pal.ink_faint);
        text(canvas, "calmer", b.x + 8.0, b.y + b.h - 70.0, 11.0, pal.ink_faint);

        // The way through: an elbow from each card to the next, labelled
        // with what changes.
        for i in 1..rects.len() {
            let (a, c) = (rects[i - 1], rects[i]);
            let (y1, y2) = (a.center_y(), c.center_y());
            let xm = (a.right + c.left) / 2.0;
            let color = pal.line_control;
            line(canvas, a.right, y1, xm, y1, color);
            line(canvas, xm, y1, xm, y2, color);
            line(canvas, xm, y2, c.left - 2.0, y2, color);
            head(canvas, c.left - 1.0, y2, color);
        }

        // The cards.
        for (i, (s, r)) in all.iter().zip(&rects).enumerate() {
            let now = playing_now == Some(i) && self.p.playing.get();
            let is_queued = queued == Some(i);
            let bg = if now { pal.signal_soft } else { pal.bg_100 };
            fill(canvas, *r, bg, 4.0);
            let edge = if now {
                Some((pal.signal, 2.0))
            } else if is_queued {
                Some((pal.md, 2.0))
            } else if playing_now == Some(i) {
                Some((pal.ink_muted, 1.0))
            } else {
                Some((pal.line, 1.0))
            };
            if let Some((c, w)) = edge {
                stroke(canvas, *r, c, w);
            }
            let x = r.left + 10.0;
            let mut y = r.top + 20.0;
            text(canvas, &fit(&format!("{}  {}", i + 1, s.name), r.width() - 16.0, 13.0), x, y, 13.0, pal.ink);
            y += 16.0;
            let status = if now {
                "playing now".to_string()
            } else if is_queued {
                "next, at the phrase".to_string()
            } else {
                format!("bar {} \u{b7} {} bars", s.start / BAR + 1, s.bars())
            };
            text(canvas, &fit(&status, r.width() - 16.0, 11.0), x, y, 11.0, if now { pal.signal } else if is_queued { pal.md } else { pal.ink_muted });
            // What changed coming in: parts that joined, then parts that left.
            y += 15.0;
            if i > 0 {
                let (inn, out) = sections::changes(&all[i - 1], s);
                let mut tx = x;
                for (words, color) in [(inn.iter().map(|n| format!("+{n}")).collect::<Vec<_>>(), pal.signal), (out.iter().map(|n| format!("\u{2212}{n}")).collect(), pal.ink_faint)] {
                    for w in words {
                        let width = crate::canvas_text::canvas_font(11.0).measure_str(&w, None).0;
                        if tx + width > r.right - 6.0 {
                            break;
                        }
                        text(canvas, &w, tx, y, 11.0, color);
                        tx += width + 6.0;
                    }
                }
                if tx == x {
                    text(canvas, "same parts", x, y, 11.0, pal.ink_faint);
                }
            } else {
                text(canvas, "start", x, y, 11.0, pal.ink_faint);
            }
            // The parts, as coloured chips wrapping inside the card.
            let mut cx_ = x;
            let mut cy_ = y + 8.0;
            for l in &s.layers {
                let w = crate::canvas_text::canvas_font(11.0).measure_str(&l.name, None).0 + 10.0;
                if cx_ + w > r.right - 6.0 && cx_ > x {
                    cx_ = x;
                    cy_ += 20.0;
                }
                if cy_ + 16.0 > r.bottom - 4.0 {
                    break;
                }
                let color = crate::timeline::header::clip_color_to_rgb(l.color);
                let chip = vg::Rect::from_xywh(cx_, cy_, w, 16.0);
                let mut paint = vg::Paint::default();
                paint.set_anti_alias(true);
                paint.set_color(color);
                paint.set_alpha(if l.full { 255 } else { 120 });
                canvas.draw_rrect(vg::RRect::new_rect_xy(chip, 2.0, 2.0), &paint);
                text(canvas, &l.name, cx_ + 5.0, cy_ + 12.0, 11.0, Color::rgb(20, 20, 20));
                cx_ += w + 4.0;
            }
        }

        // The path along the bottom: each section's share of the song.
        let end = shared::sections::song_end(&self.p.arrangement.get()).max(1) as f32;
        let (sx, sy, sw) = (b.x + 40.0, b.y + b.h - 36.0, b.w - 80.0);
        for (i, s) in all.iter().enumerate() {
            let x = sx + s.start as f32 / end * sw;
            let w = (s.end - s.start) as f32 / end * sw - 3.0;
            let now = playing_now == Some(i) && self.p.playing.get();
            fill(canvas, vg::Rect::from_xywh(x, sy, w.max(2.0), 22.0), if now { pal.signal_soft } else { pal.bg_200 }, 3.0);
            if w > 40.0 {
                text(canvas, &format!("{} {}", i + 1, s.name), x + 6.0, sy + 15.0, 11.0, pal.ink_muted);
            }
        }
        let ph = sx + self.p.playhead.get() as f32 / end * sw;
        fill(canvas, vg::Rect::from_xywh(ph, sy - 3.0, 2.0, 28.0), pal.ink, 0.0);
    }
}

fn fill(canvas: &Canvas, r: vg::Rect, color: Color, radius: f32) {
    let mut paint = vg::Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(color);
    canvas.draw_rrect(vg::RRect::new_rect_xy(r, radius, radius), &paint);
}

fn stroke(canvas: &Canvas, r: vg::Rect, color: Color, width: f32) {
    let mut paint = vg::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(vg::PaintStyle::Stroke);
    paint.set_stroke_width(width);
    paint.set_color(color);
    canvas.draw_rrect(vg::RRect::new_rect_xy(r, 4.0, 4.0), &paint);
}

fn line(canvas: &Canvas, x1: f32, y1: f32, x2: f32, y2: f32, color: Color) {
    let mut paint = vg::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(vg::PaintStyle::Stroke);
    paint.set_stroke_width(1.5);
    paint.set_color(color);
    canvas.draw_line(vg::Point::new(x1, y1), vg::Point::new(x2, y2), &paint);
}

/// An arrow head pointing right, its tip at (x, y).
fn head(canvas: &Canvas, x: f32, y: f32, color: Color) {
    let mut path = vg::PathBuilder::new();
    path.move_to((x, y));
    path.line_to((x - 7.0, y - 4.0));
    path.line_to((x - 7.0, y + 4.0));
    path.close();
    let mut paint = vg::Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(color);
    canvas.draw_path(&path.detach(), &paint);
}

/// `s`, cut with an ellipsis to fit `width`.
fn fit(s: &str, width: f32, size: f32) -> String {
    let font = crate::canvas_text::canvas_font(size);
    if font.measure_str(s, None).0 <= width {
        return s.to_string();
    }
    let mut out: String = s.to_string();
    while !out.is_empty() && font.measure_str(format!("{out}\u{2026}"), None).0 > width {
        out.pop();
    }
    format!("{}\u{2026}", out.trim_end())
}

fn text(canvas: &Canvas, s: &str, x: f32, y: f32, size: f32, color: Color) {
    let font = crate::canvas_text::canvas_font(size);
    let mut paint = vg::Paint::default();
    paint.set_color(color);
    paint.set_anti_alias(true);
    canvas.draw_str(s, vg::Point::new(x, y), &font, &paint);
}
