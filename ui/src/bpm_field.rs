//! A draggable BPM readout: click and drag vertically (or scroll) to
//! change tempo, hold Shift to drag finely, double-click to reset - the
//! same interaction language as `Knob`/`Fader`, just rendered as a plain
//! number instead of a dial, since a tempo value reads far more
//! naturally that way than as a knob position.

use vizia::prelude::*;
use vizia::vg;

use crate::tokens::ThemeId;

const DRAG_SCALAR: f64 = 0.5; // BPM per pixel dragged.
const FINE_SCALAR: f64 = 0.2;
const WHEEL_SCALAR: f64 = 1.0; // BPM per scroll notch.
const MIN_BPM: f64 = 20.0;
const MAX_BPM: f64 = 300.0;
pub const DEFAULT_BPM: f64 = shared::DEFAULT_BPM;

type ChangeCallback = Box<dyn Fn(&mut EventContext, f64)>;

/// Generic over the value source, same reasoning as `Knob<V>`: the
/// transport bar reads this out of the arrangement's tempo map via a
/// derived `Memo`, not a plain `Signal` directly.
pub struct BpmField<V: SignalGet<f64> + Copy + 'static> {
    value: V,
    theme: Signal<ThemeId>,
    is_dragging: bool,
    prev_drag_y: f32,
    continuous: f64,
    on_changing: Option<ChangeCallback>,
}

impl<V: SignalGet<f64> + Copy + 'static> BpmField<V> {
    pub fn new(
        cx: &mut Context,
        value: V,
        theme: Signal<ThemeId>,
        on_changing: impl 'static + Fn(&mut EventContext, f64),
    ) -> Handle<'_, Self> {
        let initial = value.get();
        Self { value, theme, is_dragging: false, prev_drag_y: 0.0, continuous: initial, on_changing: Some(Box::new(on_changing)) }
            .build(cx, |_| {})
            .bind(value, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
            .cursor(CursorIcon::Default)
    }
}

impl<V: SignalGet<f64> + Copy + 'static> View for BpmField<V> {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        // Unlike Knob/Fader, this drives an undo-tracked Command
        // (TimelineEvent::SetTempo), so on_changing can't fire on every
        // mouse-move tick the way theirs do - that would push one undo
        // step per pixel dragged instead of one for the whole gesture.
        // Same live-preview-then-commit-once shape as the timeline's own
        // clip drags (MoveClip/TrimClip/DrawClip in lanes.rs): update
        // `continuous` (and redraw from it) during the drag, only call
        // on_changing on release.
        let commit = |field: &mut Self, cx: &mut EventContext| {
            if let Some(callback) = &field.on_changing {
                (callback)(cx, field.continuous);
            }
        };

        event.map(|window_event, _| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                self.is_dragging = true;
                self.prev_drag_y = cx.mouse().left.pos_down.1;
                self.continuous = self.value.get();
                cx.capture();
                cx.focus_with_visibility(false);
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.is_dragging {
                    self.is_dragging = false;
                    cx.release();
                    commit(self, cx);
                }
            }
            WindowEvent::MouseMove(_, y) => {
                if self.is_dragging {
                    let mut delta = (*y - self.prev_drag_y) as f64 * DRAG_SCALAR;
                    self.prev_drag_y = *y;
                    if cx.modifiers().shift() {
                        delta *= FINE_SCALAR;
                    }
                    self.continuous = (self.continuous - delta).clamp(MIN_BPM, MAX_BPM);
                    cx.needs_redraw();
                }
            }
            WindowEvent::MouseScroll(_, y) => {
                if *y != 0.0 {
                    self.continuous = (self.continuous + *y as f64 * WHEEL_SCALAR).clamp(MIN_BPM, MAX_BPM);
                    commit(self, cx);
                }
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                self.is_dragging = false;
                self.continuous = DEFAULT_BPM;
                commit(self, cx);
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();
        // While dragging, show the live local value (continuous) rather
        // than `value`, which only updates once the drag commits.
        let display = if self.is_dragging { self.continuous } else { self.value.get() };
        let text = format!("{display:.2}");
        let font = crate::canvas_text::canvas_font(12.0);
        let mut paint = vg::Paint::default();
        paint.set_color(palette.ink);
        paint.set_anti_alias(true);
        canvas.draw_str(&text, vg::Point::new(bounds.x, bounds.y + bounds.h * 0.5 + 4.0), &font, &paint);
    }
}
