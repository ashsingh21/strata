//! The Strata vertical fader: a track hairline plus a draggable cap with a
//! centre tick, matching `.st-fader` / `.st-fader .cap` in the design spec.
//! Used by the timeline's track headers and (dead for now) the mixer strip.

use vizia::prelude::*;
use vizia::vg;

use crate::tokens::ThemeId;

const CAP_HEIGHT: f32 = 22.0;
const CAP_BORDER: f32 = 1.0;
const CENTER_LINE_HEIGHT: f32 = 2.0;
const TRACK_WIDTH: f32 = 2.0;

const DRAG_SCALAR_DIVISOR: f32 = 1.0; // delta is divided by bounds.h directly.
const FINE_SCALAR: f32 = 0.2;
const WHEEL_SCALAR: f32 = 0.02;

type ChangeCallback = Box<dyn Fn(&mut EventContext, f32)>;

/// Generic over the value source so both a plain `Signal<f32>` (the mixer
/// strip) and a derived `Memo<f32>` (the timeline's per-track fader, read
/// out of the arrangement) work without an extra indirection layer.
pub struct Fader<V: SignalGet<f32> + Copy + 'static> {
    value: V,
    default_value: f32,
    theme: Signal<ThemeId>,
    is_dragging: bool,
    prev_drag_y: f32,
    continuous: f32,
    on_changing: Option<ChangeCallback>,
    on_release: Option<ChangeCallback>,
}

impl<V: SignalGet<f32> + Copy + 'static> Fader<V> {
    pub fn new(
        cx: &mut Context,
        value: V,
        default_value: f32,
        theme: Signal<ThemeId>,
        on_changing: impl 'static + Fn(&mut EventContext, f32),
    ) -> Handle<'_, Self> {
        let initial = value.get();
        Self {
            value,
            default_value,
            theme,
            is_dragging: false,
            prev_drag_y: 0.0,
            continuous: initial,
            on_changing: Some(Box::new(on_changing)),
            on_release: None,
        }
        .build(cx, |_| {})
        .bind(value, |mut handle| handle.needs_redraw())
        .bind(theme, |mut handle| handle.needs_redraw())
    }
}

pub trait FaderModifiers {
    /// Fired once, with the final value, on mouse-up (and on the
    /// double-click-to-reset gesture) - separate from `on_changing`, which
    /// fires on every intermediate move. A caller whose `on_changing` write
    /// would itself invalidate the fader mid-drag (e.g. writing into a
    /// `Binding`-watched model that rebuilds this very view) should commit
    /// there instead, passing a no-op to `on_changing`; the cap still
    /// tracks the live drag position via the view's own local state.
    fn on_release(self, on_release: impl 'static + Fn(&mut EventContext, f32)) -> Self;
}

impl<V: SignalGet<f32> + Copy + 'static> FaderModifiers for Handle<'_, Fader<V>> {
    fn on_release(self, on_release: impl 'static + Fn(&mut EventContext, f32)) -> Self {
        self.modify(|fader| fader.on_release = Some(Box::new(on_release)))
    }
}

impl<V: SignalGet<f32> + Copy + 'static> View for Fader<V> {
    fn element(&self) -> Option<&'static str> {
        Some("strata-fader")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        let height = cx.bounds().h.max(1.0);

        let move_value = |fader: &mut Self, cx: &mut EventContext, new_value: f32| {
            fader.continuous = new_value.clamp(0.0, 1.0);
            if let Some(callback) = &fader.on_changing {
                (callback)(cx, fader.continuous);
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
                if let Some(callback) = &self.on_release {
                    (callback)(cx, self.continuous);
                }
            }

            WindowEvent::MouseMove(_, y) => {
                if self.is_dragging {
                    let mut delta = (*y - self.prev_drag_y) / (height * DRAG_SCALAR_DIVISOR);
                    self.prev_drag_y = *y;
                    if cx.modifiers().shift() {
                        delta *= FINE_SCALAR;
                    }
                    // Screen y grows downward; dragging up should increase value.
                    let new_value = self.continuous - delta;
                    move_value(self, cx, new_value);
                    cx.needs_redraw();
                }
            }

            WindowEvent::MouseScroll(_, y) => {
                if *y != 0.0 {
                    let new_value = self.continuous + *y * WHEEL_SCALAR;
                    move_value(self, cx, new_value);
                    if let Some(callback) = &self.on_release {
                        (callback)(cx, self.continuous);
                    }
                }
            }

            WindowEvent::MouseDoubleClick(button) if *button == MouseButton::Left => {
                self.is_dragging = false;
                move_value(self, cx, self.default_value);
                if let Some(callback) = &self.on_release {
                    (callback)(cx, self.continuous);
                }
            }

            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();
        // While dragging, follow the live local value rather than the
        // external signal - the external commit may be deferred to
        // mouse-up (see `on_release`), so the signal itself might not
        // move until the drag ends.
        let value = if self.is_dragging { self.continuous } else { self.value.get() }.clamp(0.0, 1.0);

        let center_x = bounds.x + bounds.w * 0.5;

        // Cap: vertically centred at (1 - value) from the top.
        let cap_center_y = bounds.y + bounds.h * (1.0 - value);

        // Track hairline: empty (bg-300) above the cap, filled (ink) below
        // it - a setting, not a sound, so it stays neutral.
        let mut empty_paint = vg::Paint::default();
        empty_paint.set_color(palette.bg_300);
        empty_paint.set_anti_alias(true);
        let empty_rect =
            vg::Rect::new(center_x - TRACK_WIDTH * 0.5, bounds.y, center_x + TRACK_WIDTH * 0.5, cap_center_y);
        canvas.draw_path(&vg::Path::rect(empty_rect, None), &empty_paint);

        let mut fill_paint = vg::Paint::default();
        fill_paint.set_color(palette.ink_muted);
        fill_paint.set_anti_alias(true);
        let fill_rect = vg::Rect::new(
            center_x - TRACK_WIDTH * 0.5,
            cap_center_y,
            center_x + TRACK_WIDTH * 0.5,
            bounds.y + bounds.h,
        );
        canvas.draw_path(&vg::Path::rect(fill_rect, None), &fill_paint);
        let cap_rect = vg::Rect::new(
            bounds.x,
            cap_center_y - CAP_HEIGHT * 0.5,
            bounds.x + bounds.w,
            cap_center_y + CAP_HEIGHT * 0.5,
        );
        let mut cap_fill = vg::Paint::default();
        cap_fill.set_color(palette.bg_300);
        cap_fill.set_anti_alias(true);
        canvas.draw_path(&vg::Path::rect(cap_rect, None), &cap_fill);

        let mut cap_border = vg::Paint::default();
        cap_border.set_color(palette.line_control);
        cap_border.set_style(vg::PaintStyle::Stroke);
        cap_border.set_stroke_width(CAP_BORDER);
        cap_border.set_anti_alias(true);
        canvas.draw_path(&vg::Path::rect(cap_rect, None), &cap_border);

        let center_line_rect = vg::Rect::new(
            bounds.x + 2.0,
            cap_center_y - CENTER_LINE_HEIGHT * 0.5,
            bounds.x + bounds.w - 2.0,
            cap_center_y + CENTER_LINE_HEIGHT * 0.5,
        );
        let mut center_line_paint = vg::Paint::default();
        center_line_paint.set_color(palette.ink);
        center_line_paint.set_anti_alias(true);
        canvas.draw_path(&vg::Path::rect(center_line_rect, None), &center_line_paint);
    }
}
