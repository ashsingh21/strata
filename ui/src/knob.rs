//! The Strata rotary knob: a 270 degree arc control, custom-drawn on canvas.
//!
//! Geometry mirrors the Strata design spec exactly (see
//! `strata/components/Knob/preview.html` in the design handoff): a 270deg
//! sweep starting at 135deg (bottom-left) and ending at 45deg (bottom-right),
//! all proportions expressed relative to the knob's diameter so the same
//! code draws both the 32px standard size and the 24px mixer-strip size.
//!
use vizia::prelude::*;
use vizia::vg;

use crate::tokens::ThemeId;

const START_ANGLE_DEG: f32 = 135.0;
const SWEEP_DEG: f32 = 270.0;

const TRACK_RADIUS_FRAC: f32 = 12.0 / 32.0;
const TRACK_STROKE_FRAC: f32 = 3.0 / 32.0;
const CAP_RADIUS_FRAC: f32 = 8.5 / 32.0;
const MOD_RING_RADIUS_FRAC: f32 = 15.5 / 32.0;
const MOD_RING_STROKE_FRAC: f32 = 1.5 / 32.0;
const POINTER_INNER_FRAC: f32 = 3.0 / 32.0;
const POINTER_OUTER_FRAC: f32 = 8.0 / 32.0;
const POINTER_STROKE_FRAC: f32 = 2.0 / 32.0;

const DRAG_SCALAR: f32 = 1.0 / 200.0;
const FINE_SCALAR: f32 = 0.2;
const WHEEL_SCALAR: f32 = 0.02;

type ChangeCallback = Box<dyn Fn(&mut EventContext, f32)>;

/// Generic over the value source (a plain `Signal<f32>` or a derived
/// `Memo<f32>`, e.g. read out of a larger model like `SynthState`) and,
/// separately, the modulation centre/depth source.
pub struct Knob<V: SignalGet<f32> + Copy + 'static, M: SignalGet<f32> + Copy + 'static = V> {
    value: V,
    default_value: f32,
    /// (centre, depth), both 0..1. The mod ring spans
    /// `[centre - depth, centre + depth]`, clamped to 0..1.
    modulation: Option<(M, M)>,
    theme: Signal<ThemeId>,
    is_dragging: bool,
    prev_drag_y: f32,
    continuous: f32,
    on_changing: Option<ChangeCallback>,
}

impl<V: SignalGet<f32> + Copy + 'static, M: SignalGet<f32> + Copy + 'static> Knob<V, M> {
    /// `modulation` is `Some((centre, depth))` to draw a modulation ring, or
    /// `None` for a plain knob. `on_changing` fires as the knob is dragged,
    /// scrolled or reset.
    pub fn new(
        cx: &mut Context,
        value: V,
        default_value: f32,
        theme: Signal<ThemeId>,
        modulation: Option<(M, M)>,
        on_changing: impl 'static + Fn(&mut EventContext, f32),
    ) -> Handle<'_, Self> {
        let initial = value.get();
        let handle = Self {
            value,
            default_value,
            modulation,
            theme,
            is_dragging: false,
            prev_drag_y: 0.0,
            continuous: initial,
            on_changing: Some(Box::new(on_changing)),
        }
        .build(cx, |_| {})
        .bind(value, |mut handle| handle.needs_redraw())
        .bind(theme, |mut handle| handle.needs_redraw())
        .cursor(CursorIcon::Default);

        match modulation {
            Some((centre, depth)) => {
                handle.bind(centre, |mut h| h.needs_redraw()).bind(depth, |mut h| h.needs_redraw())
            }
            None => handle,
        }
    }
}

impl<V: SignalGet<f32> + Copy + 'static> Knob<V, Signal<f32>> {
    /// A knob with no modulation ring - fixes the otherwise-unconstrained
    /// modulation type parameter so callers don't need a turbofish.
    pub fn plain(
        cx: &mut Context,
        value: V,
        default_value: f32,
        theme: Signal<ThemeId>,
        on_changing: impl 'static + Fn(&mut EventContext, f32),
    ) -> Handle<'_, Self> {
        Self::new(cx, value, default_value, theme, None, on_changing)
    }
}

impl<V: SignalGet<f32> + Copy + 'static, M: SignalGet<f32> + Copy + 'static> View for Knob<V, M> {
    fn element(&self) -> Option<&'static str> {
        Some("strata-knob")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        let move_value = |knob: &mut Self, cx: &mut EventContext, new_value: f32| {
            knob.continuous = new_value.clamp(0.0, 1.0);
            if let Some(callback) = &knob.on_changing {
                (callback)(cx, knob.continuous);
            }
        };

        event.map(|window_event, _| match window_event {
            WindowEvent::MouseDown(button) if *button == MouseButton::Left => {
                self.is_dragging = true;
                self.prev_drag_y = cx.mouse().left.pos_down.1;
                self.continuous = self.value.get();
                cx.capture();
                cx.focus_with_visibility(false);
            }

            WindowEvent::MouseUp(button) if *button == MouseButton::Left => {
                self.is_dragging = false;
                cx.release();
            }

            WindowEvent::MouseMove(_, y) => {
                if self.is_dragging {
                    let mut delta = (*y - self.prev_drag_y) * DRAG_SCALAR;
                    self.prev_drag_y = *y;
                    if cx.modifiers().shift() {
                        delta *= FINE_SCALAR;
                    }
                    let new_value = self.continuous - delta;
                    move_value(self, cx, new_value);
                }
            }

            WindowEvent::MouseScroll(_, y) => {
                if *y != 0.0 {
                    let new_value = self.continuous + *y * WHEEL_SCALAR;
                    move_value(self, cx, new_value);
                }
            }

            WindowEvent::MouseDoubleClick(button) if *button == MouseButton::Left => {
                self.is_dragging = false;
                move_value(self, cx, self.default_value);
            }

            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();
        let value = self.value.get().clamp(0.0, 1.0);

        let d = bounds.w.min(bounds.h);
        let center = (bounds.x + bounds.w * 0.5, bounds.y + bounds.h * 0.5);

        let oval = |radius_frac: f32| {
            let r = d * radius_frac;
            vg::Rect::new(center.0 - r, center.1 - r, center.0 + r, center.1 + r)
        };
        let arc_paint = |color: Color, stroke: f32| {
            let mut paint = vg::Paint::default();
            paint.set_color(color);
            paint.set_style(vg::PaintStyle::Stroke);
            paint.set_stroke_width(stroke);
            paint.set_stroke_cap(vg::PaintCap::Round);
            paint.set_anti_alias(true);
            paint
        };

        // Track.
        let track_oval = oval(TRACK_RADIUS_FRAC);
        let track_stroke = d * TRACK_STROKE_FRAC;
        canvas.draw_arc(
            track_oval,
            START_ANGLE_DEG,
            SWEEP_DEG,
            false,
            &arc_paint(palette.bg_300, track_stroke),
        );

        // Value arc.
        canvas.draw_arc(
            track_oval,
            START_ANGLE_DEG,
            SWEEP_DEG * value,
            false,
            &arc_paint(palette.volt, track_stroke),
        );

        // Modulation ring.
        if let Some((centre_sig, depth_sig)) = &self.modulation {
            let centre = centre_sig.get().clamp(0.0, 1.0);
            let depth = depth_sig.get().max(0.0);
            let lo = (centre - depth).clamp(0.0, 1.0);
            let hi = (centre + depth).clamp(0.0, 1.0);
            if hi > lo {
                let ring_oval = oval(MOD_RING_RADIUS_FRAC);
                let ring_stroke = d * MOD_RING_STROKE_FRAC;
                canvas.draw_arc(
                    ring_oval,
                    START_ANGLE_DEG + lo * SWEEP_DEG,
                    (hi - lo) * SWEEP_DEG,
                    false,
                    &arc_paint(palette.md, ring_stroke),
                );
            }
        }

        // Cap.
        let cap_radius = d * CAP_RADIUS_FRAC;
        let cap_path = vg::Path::circle(vg::Point::new(center.0, center.1), cap_radius, None);
        let mut cap_fill = vg::Paint::default();
        cap_fill.set_color(palette.bg_200);
        cap_fill.set_anti_alias(true);
        canvas.draw_path(&cap_path, &cap_fill);

        let mut cap_border = vg::Paint::default();
        cap_border.set_color(palette.line_control);
        cap_border.set_style(vg::PaintStyle::Stroke);
        cap_border.set_stroke_width(1.0);
        cap_border.set_anti_alias(true);
        canvas.draw_path(&cap_path, &cap_border);

        // Pointer.
        let angle_rad = (START_ANGLE_DEG + value * SWEEP_DEG).to_radians();
        let (sin_a, cos_a) = angle_rad.sin_cos();
        let inner = d * POINTER_INNER_FRAC;
        let outer = d * POINTER_OUTER_FRAC;
        let p1 = vg::Point::new(center.0 + cos_a * inner, center.1 + sin_a * inner);
        let p2 = vg::Point::new(center.0 + cos_a * outer, center.1 + sin_a * outer);
        let mut pointer_path = vg::PathBuilder::new();
        pointer_path.move_to(p1);
        pointer_path.line_to(p2);
        let pointer_path = pointer_path.detach();
        canvas.draw_path(&pointer_path, &arc_paint(palette.ink, d * POINTER_STROKE_FRAC));
    }
}
