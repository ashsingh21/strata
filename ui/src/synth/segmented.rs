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
                .height(Stretch(1.0))
                .on_press(move |cx| on_select(cx, i));
        }
    })
    .class("synth-seg")
    // The buttons sit inside the group's border: filling it edge to edge,
    // the selected one's tint painted over the border and looked cut out
    // of the group.
    .padding(Pixels(1.0))
    // Inline, not the stylesheet's height: the buttons stretch to fill it,
    // and a stretch child of an auto-sized parent comes out at nothing.
    .height(Pixels(crate::tokens::SIZE_CONTROL))
    .width(Auto)
}
