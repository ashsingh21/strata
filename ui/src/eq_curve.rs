//! The EQ's frequency-response curve - the same `shared::eq` maths the
//! engine runs. In the EQ's own panel it's also the main control: each
//! band is a dot to drag (left-right is frequency, up-down is gain; the
//! low cut only moves sideways), the wheel over the bell's dot narrows or
//! widens it, and a double-click puts a band back to 0 dB. On the Effects
//! Board it's display-only.

use vizia::prelude::*;
use crate::hidpi::Logical;
use vizia::vg;

use shared::arrangement::{automation, EqBandKind, EqState};

/// The curve's gain range, top to bottom - the gain knobs' own +-18 dB.
const DB_RANGE: f32 = 18.0;
const MIN_HZ: f32 = 20.0;
const MAX_HZ: f32 = 20_000.0;
/// How close (px) a press must be to grab a dot.
const GRAB_PX: f32 = 12.0;

pub struct EqCurve {
    state: Memo<EqState>,
    theme: Signal<crate::tokens::ThemeId>,
    /// `Some` in the EQ's panel: the dots can be dragged.
    on_edit: Option<Box<dyn Fn(&mut EventContext, EqState)>>,
    drag: Option<usize>,
}

impl EqCurve {
    pub fn new(cx: &mut Context, state: Memo<EqState>, theme: Signal<crate::tokens::ThemeId>) -> Handle<'_, Self> {
        Self { state, theme, on_edit: None, drag: None }
            .build(cx, |_| {})
            .bind(state, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
    }

    /// The panel's curve: draggable, `on_edit` getting each change.
    pub fn editable(
        cx: &mut Context,
        state: Memo<EqState>,
        theme: Signal<crate::tokens::ThemeId>,
        on_edit: impl Fn(&mut EventContext, EqState) + 'static,
    ) -> Handle<'_, Self> {
        Self { state, theme, on_edit: Some(Box::new(on_edit)), drag: None }
            .build(cx, |_| {})
            .bind(state, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
            .cursor(CursorIcon::Hand)
    }
}

/// x (0..1 across the curve) for `hz`, log-spaced like the Freq knobs.
fn x_of(hz: f32) -> f32 {
    (hz.max(MIN_HZ) / MIN_HZ).ln() / (MAX_HZ / MIN_HZ).ln()
}

fn hz_of(x: f32) -> f32 {
    MIN_HZ * (MAX_HZ / MIN_HZ).powf(x.clamp(0.0, 1.0))
}

/// Where each band's dot sits, in view pixels: at its frequency, at its
/// gain (the low cut on the curve's 0 dB line).
fn dot_positions(state: &EqState, b: BoundingBox) -> [(f32, f32); 4] {
    let y_of = |db: f32| b.y + b.h * 0.5 - (db / DB_RANGE).clamp(-1.0, 1.0) * b.h * 0.45;
    let mut out = [(0.0, 0.0); 4];
    for (i, band) in state.bands.iter().enumerate() {
        let db = if band.kind == EqBandKind::LowCut { 0.0 } else { band.gain_db };
        out[i] = (b.x + x_of(band.freq_hz) * b.w, y_of(db));
    }
    out
}

/// Each band's frequency range - the same as its knob's.
fn freq_range(kind: EqBandKind) -> (f32, f32) {
    match kind {
        EqBandKind::LowCut => automation::LOW_CUT,
        EqBandKind::LowShelf => automation::LOW_SHELF,
        EqBandKind::Bell => (MIN_HZ, MAX_HZ),
        EqBandKind::HighShelf => automation::HIGH_SHELF,
    }
}

impl EqCurve {
    fn nearest_dot(&self, cx: &EventContext) -> Option<usize> {
        let (mx, my) = cx.lmouse();
        dot_positions(&self.state.get(), cx.lbounds())
            .iter()
            .enumerate()
            .map(|(i, (x, y))| (i, ((x - mx).powi(2) + (y - my).powi(2)).sqrt()))
            .filter(|(_, d)| *d <= GRAB_PX)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    fn edit(&self, cx: &mut EventContext, state: EqState) {
        if let Some(on_edit) = &self.on_edit {
            on_edit(cx, state);
        }
    }
}

impl View for EqCurve {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        if self.on_edit.is_none() {
            return;
        }
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                if let Some(i) = self.nearest_dot(cx) {
                    self.drag = Some(i);
                    cx.capture();
                    meta.consume();
                }
            }
            WindowEvent::MouseMove(..) => {
                let Some(i) = self.drag else { return };
                let b = cx.lbounds();
                let (mx, my) = cx.lmouse();
                let mut state = self.state.get();
                let band = &mut state.bands[i];
                let (lo, hi) = freq_range(band.kind);
                band.freq_hz = hz_of((mx - b.x) / b.w).clamp(lo, hi);
                if band.kind != EqBandKind::LowCut {
                    let db = (b.y + b.h * 0.5 - my) / (b.h * 0.45) * DB_RANGE;
                    band.gain_db = (db * 2.0).round() / 2.0;
                    band.gain_db = band.gain_db.clamp(-DB_RANGE, DB_RANGE);
                }
                // Dragging a band that's off switches it on.
                band.on = true;
                self.edit(cx, state);
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.drag.take().is_some() {
                    cx.release();
                }
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                if let Some(i) = self.nearest_dot(cx) {
                    let mut state = self.state.get();
                    let band = &mut state.bands[i];
                    if band.kind == EqBandKind::LowCut {
                        band.on = false;
                    } else {
                        band.gain_db = 0.0;
                    }
                    self.edit(cx, state);
                    meta.consume();
                }
            }
            // The wheel over the bell: narrower (up) or wider (down).
            WindowEvent::MouseScroll(_, y) => {
                let mut state = self.state.get();
                let Some(i) = self.nearest_dot(cx).filter(|&i| state.bands[i].kind == EqBandKind::Bell) else { return };
                let band = &mut state.bands[i];
                band.q = (band.q * 1.15f32.powf(y.clamp(-3.0, 3.0))).clamp(0.1, 10.0);
                self.edit(cx, state);
                meta.consume();
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let bounds = cx.lbounds();
        let palette = self.theme.get().palette();
        let state = self.state.get();
        let sample_rate = 48_000.0;
        let fill = |rect: vg::Rect, color: Color| {
            let mut paint = vg::Paint::default();
            paint.set_color(color);
            canvas.draw_path(&vg::Path::rect(rect, None), &paint);
        };

        // 0 dB, and faint lines at 100 Hz, 1 kHz and 10 kHz.
        let mid = bounds.y + bounds.h * 0.5;
        fill(vg::Rect::new(bounds.x, mid, bounds.x + bounds.w, mid + 1.0), palette.line);
        if self.on_edit.is_some() {
            for hz in [100.0, 1000.0, 10_000.0] {
                let x = (bounds.x + x_of(hz) * bounds.w).round();
                fill(vg::Rect::new(x, bounds.y, x + 1.0, bounds.y + bounds.h), palette.grid_beat);
            }
        }

        let mut path = vg::PathBuilder::new();
        let steps = if self.on_edit.is_some() { 160 } else { 64 };
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let db = shared::eq::response_db(&state, hz_of(t), sample_rate).clamp(-DB_RANGE, DB_RANGE);
            let x = bounds.x + t * bounds.w;
            let y = mid - (db / DB_RANGE) * bounds.h * 0.45;
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

        // The dots: filled while the band is on, a ring while it's off.
        if self.on_edit.is_some() {
            for (i, (x, y)) in dot_positions(&state, bounds).into_iter().enumerate() {
                let mut dot = vg::Paint::default();
                dot.set_anti_alias(true);
                let on = state.bands[i].on;
                dot.set_color(if self.drag == Some(i) { palette.signal } else if on { palette.ink } else { palette.ink_muted });
                if !on {
                    dot.set_style(vg::PaintStyle::Stroke);
                    dot.set_stroke_width(1.5);
                }
                canvas.draw_circle(vg::Point::new(x, y), 4.5, &dot);
            }
        }
    }
}
