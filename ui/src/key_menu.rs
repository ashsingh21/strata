//! The song's key and scale, picked from one menu: the 12 notes, then the
//! scales. Opened by the header's Key button, the clip editor's and
//! Theory's, and it links to the Theory view (the scale explorer) at the
//! bottom.
//!
//! One Vizia dropdown at the window root serves all three: pressing a Key
//! button moves the dropdown's anchor onto that button's left edge (zero
//! wide, so it never catches a click) and opens it, so it opens beside the button, moves to stay in
//! the window, and closes on a click elsewhere or Escape. A dropdown of
//! the button's own would be cut off - two of the buttons are inside the
//! lower panel, which scrolls and so clips what's drawn outside it.

use std::cell::Cell;

use vizia::prelude::*;

use shared::theory::{note_name, SCALE_PRESETS};

use crate::hidpi::Logical;
use crate::interval_input::state::IntervalInputEvent;
use crate::tokens::{self, SPACE_1, SPACE_2};

const MENU_WIDTH: f32 = 300.0;
/// Scale rows: compact, so 12 of them don't make a tower.
const ROW_HEIGHT: f32 = 24.0;

thread_local! {
    /// The key, the scale and whether Theory is open - set once in `main`,
    /// so every Key button's menu can read them without threading them
    /// through each view's props.
    static SIGNALS: Cell<Option<(Signal<u8>, Signal<u16>, Signal<bool>)>> = const { Cell::new(None) };
    /// The root dropdown, and where its anchor sits (x, y, height).
    static HOST: Cell<Option<(Entity, Signal<(f32, f32, f32)>)>> = const { Cell::new(None) };
}

/// The menu's dropdown, mounted once, last, at the window root.
pub fn host(cx: &mut Context, key: Signal<u8>, scale_mask: Signal<u16>, theory_open: Signal<bool>) {
    SIGNALS.set(Some((key, scale_mask, theory_open)));
    let anchor = Signal::new((0.0f32, 0.0f32, 0.0f32));
    let dropdown = crate::menu::menu(
        cx,
        Placement::BottomStart,
        |cx| {
            Element::new(cx).hoverable(false).width(Stretch(1.0)).height(Stretch(1.0));
        },
        content,
    )
    .position_type(PositionType::Absolute)
    .left(anchor.map(|a| Pixels(a.0)))
    .top(anchor.map(|a| Pixels(a.1)))
    .width(Pixels(0.0))
    .height(anchor.map(|a| Pixels(a.2)))
    .entity();
    HOST.set(Some((dropdown, anchor)));
}

/// A Key button's press: the menu, opened on that button.
pub fn open_from(cx: &mut EventContext) {
    let Some((dropdown, anchor)) = HOST.get() else { return };
    let b = cx.lbounds();
    anchor.set((b.x, b.y, b.h));
    cx.emit_to(dropdown, PopupEvent::Open);
}

fn content(cx: &mut Context) {
    let Some((key, scale_mask, theory_open)) = SIGNALS.get() else { return };
    VStack::new(cx, move |cx| {
        Label::new(cx, "Key").class("label");
        for row in 0..2u8 {
            HStack::new(cx, move |cx| {
                for n in row * 6..row * 6 + 6 {
                    Button::new(cx, move |cx| Label::new(cx, note_name(n)).hoverable(false))
                        .class("synth-seg-btn")
                        .toggle_class("is-on", key.map(move |k| *k == n))
                        .alignment(Alignment::Center)
                        .width(Stretch(1.0))
                        .height(Pixels(tokens::SIZE_CONTROL))
                        .on_press(move |cx| cx.emit(IntervalInputEvent::SetKey(n)));
                }
            })
            .class("synth-seg")
            .width(Stretch(1.0))
            .height(Auto);
        }

        Label::new(cx, "Scale").class("label").top(Pixels(SPACE_1));
        // Two columns: the Western scales, then the ragas.
        let split = SCALE_PRESETS.iter().position(|p| p.name.starts_with("Raga")).unwrap_or(SCALE_PRESETS.len());
        HStack::new(cx, move |cx| {
            for column in [&SCALE_PRESETS[..split], &SCALE_PRESETS[split..]] {
                VStack::new(cx, move |cx| {
                    for preset in column {
                        let mask = preset.mask;
                        Button::new(cx, move |cx| Label::new(cx, preset.name).class("body").hoverable(false))
                            .class("menu-item")
                            .toggle_class("is-on", scale_mask.map(move |m| *m == mask))
                            .alignment(Alignment::Left)
                            .padding_left(Pixels(SPACE_2))
                            .width(Stretch(1.0))
                            .height(Pixels(ROW_HEIGHT))
                            .on_press(move |cx| {
                                cx.emit(IntervalInputEvent::SetScaleMask(mask));
                                crate::menu::close(cx);
                            });
                    }
                })
                .gap(Pixels(2.0))
                .width(Stretch(1.0))
                .height(Auto);
            }
        })
        .gap(Pixels(SPACE_1))
        .width(Stretch(1.0))
        .height(Auto);

        Element::new(cx).class("hairline").width(Stretch(1.0)).height(Pixels(1.0));
        Button::new(cx, |cx| Label::new(cx, "Explore this scale in Theory").class("body").hoverable(false))
            .class("menu-item")
            .alignment(Alignment::Left)
            .padding_left(Pixels(SPACE_2))
            .width(Stretch(1.0))
            .height(Pixels(ROW_HEIGHT))
            .on_press(move |cx| {
                if !theory_open.get() {
                    cx.emit(IntervalInputEvent::ToggleOpen);
                }
                crate::menu::close(cx);
            });
    })
    .class("panel")
    .class("context-menu")
    .gap(Pixels(SPACE_1))
    .padding_top(Pixels(SPACE_2))
    .padding_bottom(Pixels(SPACE_2))
    .padding_left(Pixels(SPACE_2))
    .padding_right(Pixels(SPACE_2))
    .width(Pixels(MENU_WIDTH))
    .height(Auto);
}
