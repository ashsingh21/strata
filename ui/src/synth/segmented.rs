//! The segmented control: mutually-exclusive choices (waveform, filter
//! type, mono/poly). The selected option inverts to ink-on-bg-100, reusing
//! the same "on" look as a Sync toggle (`.btn.is-mute`) - colour stays
//! reserved for state, per the Strata voice.

use vizia::prelude::*;

/// `count` items; `content` builds each button's label content, `is_on`
/// gives a reactive "is index `i` selected" (a `Memo`, not a plain bool -
/// a bool is read once at build time, so the highlight never followed the
/// selection), `on_select` fires with the clicked index.
pub fn segmented<V: View>(
    cx: &mut Context,
    count: usize,
    mut content: impl FnMut(&mut Context, usize) -> Handle<'_, V>,
    mut is_on: impl FnMut(usize) -> Memo<bool>,
    on_select: impl Fn(&mut EventContext, usize) + Copy + Send + Sync + 'static,
) -> Handle<'_, HStack> {
    HStack::new(cx, move |cx| {
        for i in 0..count {
            Button::new(cx, |cx| content(cx, i))
                .class("synth-seg-btn")
                .toggle_class("is-on", is_on(i))
                .on_press(move |cx| on_select(cx, i));
        }
    })
    .class("synth-seg")
    .size(Auto)
}
