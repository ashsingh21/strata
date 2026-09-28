//! The song's key and scale, picked from one menu: the 12 notes, then the
//! scales. Opened by the header's Key button and the clip editor's key
//! button - it opens under whichever was pressed - and links to the Theory
//! view (the scale explorer) at the bottom.

use std::cell::Cell;

use vizia::prelude::*;

use shared::theory::{note_name, SCALE_PRESETS};

use crate::hidpi::Logical;
use crate::interval_input::state::IntervalInputEvent;
use crate::tokens::{self, SPACE_1, SPACE_2};
use crate::transport::HeaderMenus;

const MENU_WIDTH: f32 = 300.0;
/// About how tall the menu is: it opens above its button when there's no
/// room below (the clip editor's button sits low in the window).
const MENU_HEIGHT: f32 = 380.0;

thread_local! {
    static MENU: Cell<Option<(HeaderMenus, Signal<(f32, f32)>)>> = const { Cell::new(None) };
}

/// Opens (or closes) the menu under the button handling this press.
pub fn toggle_under(cx: &mut EventContext) {
    let Some((menus, at)) = MENU.get() else { return };
    if !menus.key.get() {
        let button = cx.lbounds();
        let (window_w, window_h) = cx.with_current(Entity::root(), |cx| {
            let b = cx.lbounds();
            (b.w, b.h)
        });
        let x = button.x.min(window_w - MENU_WIDTH - SPACE_2).max(SPACE_2);
        let below = button.y + button.h + SPACE_1;
        let y = if below + MENU_HEIGHT > window_h { (button.y - MENU_HEIGHT - SPACE_1).max(SPACE_2) } else { below };
        at.set((x, y));
    }
    menus.toggle(menus.key);
}

/// The menu itself, mounted at the window root (above everything, with
/// the header menus' click-outside backdrop under it).
pub fn key_menu(cx: &mut Context, menus: HeaderMenus, key: Signal<u8>, scale_mask: Signal<u16>, theory_open: Signal<bool>) {
    let at = Signal::new((0.0f32, 0.0f32));
    MENU.set(Some((menus, at)));
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
                            .width(Stretch(1.0))
                            .on_press(move |cx| {
                                cx.emit(IntervalInputEvent::SetScaleMask(mask));
                                menus.close_all();
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
            .width(Stretch(1.0))
            .on_press(move |cx| {
                if !theory_open.get() {
                    cx.emit(IntervalInputEvent::ToggleOpen);
                }
                menus.close_all();
            });
    })
    .class("panel")
    .class("context-menu")
    .toggle_class("hidden", menus.key.map(|o| !*o))
    .position_type(PositionType::Absolute)
    .left(at.map(|(x, _)| Pixels(*x)))
    .top(at.map(|(_, y)| Pixels(*y)))
    .gap(Pixels(SPACE_1))
    .padding_top(Pixels(SPACE_2))
    .padding_bottom(Pixels(SPACE_2))
    .padding_left(Pixels(SPACE_2))
    .padding_right(Pixels(SPACE_2))
    .width(Pixels(MENU_WIDTH))
    .height(Auto);
}
