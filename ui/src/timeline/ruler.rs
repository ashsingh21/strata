//! The timeline ruler: bar/beat ticks and numbers, the loop range bar,
//! markers and the playhead's triangle head. One custom canvas view; draws
//! only the visible tick range.

use vizia::prelude::*;
use vizia::vg;

use shared::arrangement::{snap, Arrangement, LoopRange, Ticks, TimeSignature, ViewTransform};

use crate::timeline::state::{ContextMenu, ContextMenuTarget, TimelineEvent};
use crate::tokens::ThemeId;

const LOOP_BAR_HEIGHT: f32 = 5.0;
const BAR_TICK_HEIGHT: f32 = 10.0;
const BEAT_TICK_HEIGHT: f32 = 4.0;
const MIN_BEAT_PX: f64 = 6.0;
const EDGE_HIT_PX: f64 = 5.0;

#[derive(Clone, Copy)]
enum Drag {
    Scrub,
    LoopStart,
    LoopEnd,
    LoopMiddle { grab_offset: Ticks },
    /// Dragging on the ruler with no loop range set yet - there was
    /// previously no way to create the *first* one at all (every other
    /// drag variant only adjusts an existing range). A plain click still
    /// just scrubs, same as always: this only becomes a real loop if the
    /// drag actually covers a nonzero span by mouse-up.
    CreateLoop { anchor: Ticks },
}

pub struct Ruler {
    arrangement: Signal<Arrangement>,
    transform: Signal<ViewTransform>,
    playhead: Signal<Ticks>,
    theme: Signal<ThemeId>,
    loop_on: Signal<bool>,
    drag: Option<Drag>,
    /// Live loop-range preview while dragging; committed as one command on
    /// mouse-up so a drag is a single undo step.
    loop_preview: Option<LoopRange>,
}

impl Ruler {
    pub fn new(
        cx: &mut Context,
        arrangement: Signal<Arrangement>,
        transform: Signal<ViewTransform>,
        playhead: Signal<Ticks>,
        theme: Signal<ThemeId>,
        loop_on: Signal<bool>,
    ) -> Handle<'_, Self> {
        Self { arrangement, transform, playhead, theme, loop_on, drag: None, loop_preview: None }
            .build(cx, |_| {})
            .bind(arrangement, |mut h| h.needs_redraw())
            .bind(transform, |mut h| h.needs_redraw())
            .bind(playhead, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
            .bind(loop_on, |mut h| h.needs_redraw())
    }

    fn loop_range(&self) -> Option<LoopRange> {
        self.loop_preview.or(self.arrangement.get().loop_range)
    }
}

impl View for Ruler {
    fn element(&self) -> Option<&'static str> {
        Some("strata-ruler")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, _| match window_event {
            WindowEvent::MouseDown(button) if *button == MouseButton::Left => {
                let bounds = cx.bounds();
                let x = cx.mouse().cursor_x as f64 - bounds.x as f64;
                let transform = self.transform.get();
                let tick = transform.x_to_tick(x);

                self.drag = Some(match self.arrangement.get().loop_range {
                    Some(range) if (transform.tick_to_x(range.start) - x).abs() <= EDGE_HIT_PX => {
                        Drag::LoopStart
                    }
                    Some(range) if (transform.tick_to_x(range.end) - x).abs() <= EDGE_HIT_PX => {
                        Drag::LoopEnd
                    }
                    Some(range) if tick > range.start && tick < range.end => {
                        Drag::LoopMiddle { grab_offset: tick - range.start }
                    }
                    Some(_) => Drag::Scrub,
                    None => Drag::CreateLoop { anchor: tick.max(0) },
                });
                if matches!(self.drag, Some(Drag::Scrub) | Some(Drag::CreateLoop { .. })) {
                    cx.emit(TimelineEvent::ScrubPlayhead(tick.max(0)));
                }
                cx.capture();
            }

            WindowEvent::MouseDown(button) if *button == MouseButton::Right => {
                let bounds = cx.bounds();
                let x = cx.mouse().cursor_x as f64 - bounds.x as f64;
                let transform = self.transform.get();
                let arr = self.arrangement.get();

                let hit = arr.markers.iter().find(|m| {
                    let mx = transform.tick_to_x(m.position);
                    let width = (8.0 + m.name.len() as f32 * 6.0) as f64;
                    x >= mx && x <= mx + width
                });

                let target = match hit {
                    Some(marker) => ContextMenuTarget::Marker { marker: marker.id },
                    None => ContextMenuTarget::Ruler { tick: transform.x_to_tick(x).max(0) },
                };
                cx.emit(TimelineEvent::OpenContextMenu(ContextMenu {
                    target,
                    x: cx.mouse().cursor_x,
                    y: cx.mouse().cursor_y,
                }));
            }

            WindowEvent::MouseMove(x, _) => {
                if let Some(drag) = self.drag {
                    let bounds = cx.bounds();
                    let local_x = *x as f64 - bounds.x as f64;
                    let transform = self.transform.get();
                    let bypass = cx.modifiers().alt();
                    let tick = snap(transform.x_to_tick(local_x), shared::arrangement::SnapGrid::Sixteenth, bypass);

                    match drag {
                        Drag::Scrub => cx.emit(TimelineEvent::ScrubPlayhead(tick.max(0))),
                        Drag::LoopStart => {
                            let end = self.loop_range().map(|r| r.end).unwrap_or(tick + 1);
                            self.loop_preview = Some(LoopRange { start: tick.min(end - 1).max(0), end });
                            cx.needs_redraw();
                        }
                        Drag::LoopEnd => {
                            let start = self.loop_range().map(|r| r.start).unwrap_or(0);
                            self.loop_preview = Some(LoopRange { start, end: tick.max(start + 1) });
                            cx.needs_redraw();
                        }
                        Drag::LoopMiddle { grab_offset } => {
                            if let Some(range) = self.loop_range() {
                                let length = range.end - range.start;
                                let start = (tick - grab_offset).max(0);
                                self.loop_preview = Some(LoopRange { start, end: start + length });
                                cx.needs_redraw();
                            }
                        }
                        Drag::CreateLoop { anchor } => {
                            let lo = anchor.min(tick).max(0);
                            let hi = anchor.max(tick).max(0);
                            self.loop_preview = Some(LoopRange { start: lo, end: hi });
                            cx.needs_redraw();
                        }
                    }
                }
            }

            WindowEvent::MouseUp(button) if *button == MouseButton::Left => {
                match self.drag {
                    Some(Drag::LoopStart) | Some(Drag::LoopEnd) | Some(Drag::LoopMiddle { .. }) => {
                        if let Some(range) = self.loop_preview.take() {
                            cx.emit(TimelineEvent::SetLoopRange(Some(range)));
                        }
                    }
                    // Only a real drag (nonzero span) becomes a loop - a
                    // plain click (start == end, or never moved far enough
                    // to snap to a different grid line) just scrubbed the
                    // playhead already, on mouse-down.
                    Some(Drag::CreateLoop { .. }) => {
                        if let Some(range) = self.loop_preview.take() {
                            if range.end > range.start {
                                cx.emit(TimelineEvent::SetLoopRange(Some(range)));
                            }
                        }
                    }
                    _ => {}
                }
                self.drag = None;
                self.loop_preview = None;
                cx.release();
            }

            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();
        let arr = self.arrangement.get();
        let transform = self.transform.get();
        let sig = TimeSignature::FOUR_FOUR;
        let ticks_per_bar = sig.ticks_per_bar();
        let ticks_per_beat = sig.ticks_per_beat();

        let mut bg = vg::Paint::default();
        bg.set_color(palette.bg_000);
        bg.set_anti_alias(true);
        canvas.draw_path(&vg::Path::rect(vg::Rect::new(bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h), None), &bg);

        // The loop/section range: a full-height wash first (so it sits
        // behind the bar numbers), brighter once Loop is actually on -
        // a range can be drawn and kept without looping, so the ruler
        // shouldn't look identical to "this is now playing on repeat".
        let loop_range = self.loop_range();
        if let Some(range) = loop_range {
            let x0 = (bounds.x as f64 + transform.tick_to_x(range.start)) as f32;
            let x1 = (bounds.x as f64 + transform.tick_to_x(range.end)) as f32;
            let mut wash = vg::Paint::default();
            wash.set_color(if self.loop_on.get() { palette.mod_soft } else { palette.bg_200 });
            wash.set_anti_alias(true);
            canvas.draw_path(&vg::Path::rect(vg::Rect::new(x0, bounds.y, x1, bounds.y + bounds.h), None), &wash);
        }

        let start_tick = transform.x_to_tick(0.0).max(0);
        let end_tick = transform.x_to_tick(bounds.w as f64) + ticks_per_bar;

        let bar_px = transform.ticks_to_px(ticks_per_bar);
        let beat_px = transform.ticks_to_px(ticks_per_beat);
        let bar_stride: i64 = if bar_px >= 30.0 {
            1
        } else if bar_px >= 15.0 {
            2
        } else if bar_px >= 7.0 {
            4
        } else {
            8
        };

        let first_bar = start_tick / ticks_per_bar;
        let last_bar = end_tick / ticks_per_bar + 1;

        let mut tick_paint = vg::Paint::default();
        tick_paint.set_anti_alias(false);
        let font = crate::canvas_text::canvas_font(10.0);
        let mut text_paint = vg::Paint::default();
        text_paint.set_anti_alias(true);
        text_paint.set_color(palette.ink_muted);

        for bar in first_bar..last_bar {
            let tick = bar * ticks_per_bar;
            let x = (bounds.x as f64 + transform.tick_to_x(tick)) as f32;
            if x < bounds.x - 20.0 || x > bounds.x + bounds.w + 20.0 {
                continue;
            }

            tick_paint.set_color(palette.ink_muted);
            let rect = vg::Rect::new(x, bounds.y + bounds.h - BAR_TICK_HEIGHT, x + 1.0, bounds.y + bounds.h);
            canvas.draw_path(&vg::Path::rect(rect, None), &tick_paint);

            if beat_px >= MIN_BEAT_PX {
                tick_paint.set_color(palette.ink_faint);
                for beat in 1..sig.numerator as i64 {
                    let bx = (bounds.x as f64 + transform.tick_to_x(tick + beat * ticks_per_beat)) as f32;
                    let brect = vg::Rect::new(bx, bounds.y + bounds.h - BEAT_TICK_HEIGHT, bx + 1.0, bounds.y + bounds.h);
                    canvas.draw_path(&vg::Path::rect(brect, None), &tick_paint);
                }
            }

            if bar % bar_stride == 0 {
                canvas.draw_str((bar + 1).to_string(), vg::Point::new(x + 4.0, bounds.y + 15.0), &font, &text_paint);
            }
        }

        // The crisp handle bar on top of everything else, so it stays the
        // obvious thing to grab even where it crosses bar numbers.
        if let Some(range) = loop_range {
            let x0 = (bounds.x as f64 + transform.tick_to_x(range.start)) as f32;
            let x1 = (bounds.x as f64 + transform.tick_to_x(range.end)) as f32;
            let mut loop_paint = vg::Paint::default();
            loop_paint.set_color(if self.loop_on.get() { palette.md } else { palette.ink_faint });
            loop_paint.set_anti_alias(true);
            canvas.draw_path(
                &vg::Path::rect(vg::Rect::new(x0, bounds.y, x1, bounds.y + LOOP_BAR_HEIGHT), None),
                &loop_paint,
            );
        }

        for marker in &arr.markers {
            let x = (bounds.x as f64 + transform.tick_to_x(marker.position)) as f32;
            let width = 8.0 + marker.name.len() as f32 * 6.0;
            let mut tab_paint = vg::Paint::default();
            tab_paint.set_color(palette.bg_300);
            tab_paint.set_anti_alias(true);
            let rect = vg::Rect::new(x, bounds.y + bounds.h - 12.0, x + width, bounds.y + bounds.h);
            canvas.draw_path(&vg::Path::rect(rect, None), &tab_paint);

            let mut marker_text = vg::Paint::default();
            marker_text.set_anti_alias(true);
            marker_text.set_color(palette.ink);
            canvas.draw_str(&marker.name, vg::Point::new(x + 4.0, bounds.y + bounds.h - 2.0), &font, &marker_text);
        }

        let playhead_x = (bounds.x as f64 + transform.tick_to_x(self.playhead.get())) as f32;
        if playhead_x >= bounds.x && playhead_x <= bounds.x + bounds.w {
            let mut ph_paint = vg::Paint::default();
            ph_paint.set_color(palette.playhead);
            ph_paint.set_anti_alias(true);
            canvas.draw_path(
                &vg::Path::rect(vg::Rect::new(playhead_x, bounds.y, playhead_x + 1.0, bounds.y + bounds.h), None),
                &ph_paint,
            );
            let mut tri = vg::PathBuilder::new();
            tri.move_to(vg::Point::new(playhead_x - 5.0, bounds.y));
            tri.line_to(vg::Point::new(playhead_x + 6.0, bounds.y));
            tri.line_to(vg::Point::new(playhead_x + 0.5, bounds.y + 7.0));
            tri.close();
            canvas.draw_path(&tri.detach(), &ph_paint);
        }
    }
}
