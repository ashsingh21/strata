//! The song at a glance, under the lanes: every clip as a thin bar in its
//! track's colour across the whole song, with a box around the part on
//! screen. Click or drag on it to look somewhere else.

use vizia::prelude::*;
use vizia::vg;

use shared::arrangement::{Arrangement, Ticks, ViewTransform, PPQ};

use super::state::TimelineEvent;
use crate::hidpi::Logical;
use crate::tokens::ThemeId;

pub struct Overview {
    arrangement: Signal<Arrangement>,
    transform: Signal<ViewTransform>,
    theme: Signal<ThemeId>,
    dragging: bool,
}

/// How much of the song the strip covers: past the last clip (or marker,
/// or loop) by a bar, and never under 16 bars.
fn song_end(arr: &Arrangement) -> Ticks {
    let bar = PPQ * 4;
    let last = arr
        .clips
        .iter()
        .map(|c| c.end())
        .chain(arr.markers.iter().map(|m| m.position))
        .chain(arr.loop_range.map(|r| r.end))
        .max()
        .unwrap_or(0);
    (last + bar).max(bar * 16)
}

impl Overview {
    pub fn new(cx: &mut Context, arrangement: Signal<Arrangement>, transform: Signal<ViewTransform>, theme: Signal<ThemeId>) -> Handle<'_, Self> {
        Self { arrangement, transform, theme, dragging: false }
            .build(cx, |_| {})
            .bind(arrangement, |mut h| h.needs_redraw())
            .bind(transform, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
    }

    /// Centres the lanes on the song position under the pointer.
    fn look_at(&self, cx: &mut EventContext) {
        let bounds = cx.lbounds();
        let t = self.transform.get();
        let end = song_end(&self.arrangement.get());
        let frac = ((cx.lmouse().0 - bounds.x) / bounds.w.max(1.0)).clamp(0.0, 1.0) as f64;
        let tick = (frac * end as f64) as Ticks;
        let x = t.ticks_to_px(tick) - bounds.w as f64 / 2.0;
        cx.emit(TimelineEvent::ScrollBy { dx: x - t.scroll_x, dy: 0.0 });
    }
}

impl View for Overview {
    fn element(&self) -> Option<&'static str> {
        Some("strata-overview")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, _| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                self.dragging = true;
                cx.capture();
                self.look_at(cx);
            }
            WindowEvent::MouseMove(..) if self.dragging => self.look_at(cx),
            WindowEvent::MouseUp(MouseButton::Left) => {
                self.dragging = false;
                cx.release();
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let bounds = cx.lbounds();
        crate::hidpi::clip(canvas, bounds);
        let palette = self.theme.get().palette();
        let arr = self.arrangement.get();
        let end = song_end(&arr) as f32;
        let x_of = |tick: Ticks| bounds.x + tick as f32 / end * bounds.w;

        let mut paint = vg::Paint::default();
        paint.set_color(palette.bg_000);
        canvas.draw_path(&vg::Path::rect(vg::Rect::new(bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h), None), &paint);

        // One thin row per track, squeezed into the strip's height.
        let rows = arr.tracks.len().max(1) as f32;
        let row_h = ((bounds.h - 4.0) / rows).clamp(1.0, 4.0);
        for (i, track) in arr.tracks.iter().enumerate() {
            let y = bounds.y + 2.0 + i as f32 * row_h;
            paint.set_color(super::header::clip_color_to_rgb(track.color));
            for clip in arr.clips.iter().filter(|c| c.track == track.id) {
                let (x0, x1) = (x_of(clip.start), x_of(clip.end()).max(x_of(clip.start) + 1.0));
                canvas.draw_path(&vg::Path::rect(vg::Rect::new(x0, y, x1, y + (row_h - 1.0).max(1.0)), None), &paint);
            }
        }

        // What's on screen.
        let t = self.transform.get();
        let view_start = t.scroll_x / t.pixels_per_beat * PPQ as f64;
        let view_ticks = bounds.w as f64 / t.pixels_per_beat * PPQ as f64;
        let x0 = x_of(view_start as Ticks);
        let x1 = x_of((view_start + view_ticks) as Ticks).min(bounds.x + bounds.w);
        let mut frame = vg::Paint::default();
        frame.set_color(palette.ink);
        frame.set_style(vg::PaintStyle::Stroke);
        frame.set_stroke_width(1.0);
        canvas.draw_path(&vg::Path::rect(vg::Rect::new(x0 + 0.5, bounds.y + 0.5, (x1 - 0.5).max(x0 + 2.0), bounds.y + bounds.h - 0.5), None), &frame);
    }
}
