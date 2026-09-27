//! A BPM readout: scroll to nudge tempo by 1 BPM at a time, double-click
//! to reset to the default - no click-and-drag gesture (removed per
//! feedback: drag-to-change was error-prone and hard to discover, and the
//! transport bar now has dedicated +/- buttons and a right-click-to-type
//! field for that).

use vizia::prelude::*;
use crate::hidpi::Logical;
use vizia::vg;

use crate::tokens::ThemeId;

const WHEEL_SCALAR: f64 = 1.0; // BPM per scroll notch.
pub const MIN_BPM: f64 = 20.0;
pub const MAX_BPM: f64 = 300.0;
pub const DEFAULT_BPM: f64 = shared::DEFAULT_BPM;

type ChangeCallback = Box<dyn Fn(&mut EventContext, f64)>;

/// Generic over the value source, same reasoning as `Knob<V>`: the
/// transport bar reads this out of the arrangement's tempo map via a
/// derived `Memo`, not a plain `Signal` directly.
pub struct BpmField<V: SignalGet<f64> + Copy + 'static> {
    value: V,
    theme: Signal<ThemeId>,
    on_changing: Option<ChangeCallback>,
}

impl<V: SignalGet<f64> + Copy + 'static> BpmField<V> {
    pub fn new(
        cx: &mut Context,
        value: V,
        theme: Signal<ThemeId>,
        on_changing: impl 'static + Fn(&mut EventContext, f64),
    ) -> Handle<'_, Self> {
        Self { value, theme, on_changing: Some(Box::new(on_changing)) }
            .build(cx, |_| {})
            .bind(value, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
            .cursor(CursorIcon::Default)
    }
}

impl<V: SignalGet<f64> + Copy + 'static> View for BpmField<V> {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        // Every gesture here reads `self.value.get()` fresh and commits
        // once - no persistent local state to drift out of sync with
        // whatever else (the +/- buttons, the right-click textbox) may
        // have changed the real value in between gestures.
        let commit = |field: &Self, cx: &mut EventContext, v: f64| {
            if let Some(callback) = &field.on_changing {
                (callback)(cx, v);
            }
        };

        event.map(|window_event, _| match window_event {
            WindowEvent::MouseScroll(_, y) => {
                if *y != 0.0 {
                    let v = (self.value.get() + *y as f64 * WHEEL_SCALAR).clamp(MIN_BPM, MAX_BPM);
                    commit(self, cx, v);
                }
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                commit(self, cx, DEFAULT_BPM);
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let bounds = cx.lbounds();
        let palette = self.theme.get().palette();
        let text = format!("{:.2}", self.value.get());
        let font = crate::canvas_text::canvas_font(13.0);
        let mut paint = vg::Paint::default();
        paint.set_color(palette.ink);
        paint.set_anti_alias(true);
        canvas.draw_str(&text, vg::Point::new(bounds.x, bounds.y + bounds.h * 0.5 + 4.0), &font, &paint);
    }
}
