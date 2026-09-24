//! The transport bar: stop/play/record/loop, tempo/meter/position readouts.

use vizia::prelude::*;

use shared::Position;

use crate::app::AppEvent;
use crate::interval_input::state::IntervalInputEvent;
use crate::tokens::{SPACE_2, SPACE_3};

#[allow(clippy::too_many_arguments)]
pub fn transport_bar(
    cx: &mut Context,
    playing: Signal<bool>,
    loop_on: Signal<bool>,
    record_armed: Signal<bool>,
    click_on: Signal<bool>,
    position: Signal<Position>,
    interval_open: Signal<bool>,
) {
    HStack::new(cx, move |cx| {
        Button::new(cx, |cx| Label::new(cx, "\u{25A0}")).class("btn").on_press(|cx| cx.emit(AppEvent::Stop));

        Button::new(cx, |cx| Label::new(cx, "\u{25B6}"))
            .class("btn")
            .toggle_class("is-play", playing)
            .on_press(|cx| cx.emit(AppEvent::TogglePlay));

        Button::new(cx, |cx| Label::new(cx, "\u{25CF}"))
            .class("btn")
            .toggle_class("is-rec", record_armed)
            .on_press(|cx| cx.emit(AppEvent::ToggleArm));

        Button::new(cx, |cx| Label::new(cx, "\u{21BB}"))
            .class("btn")
            .toggle_class("is-mod", loop_on)
            .on_press(|cx| cx.emit(AppEvent::ToggleLoop));

        Button::new(cx, |cx| Label::new(cx, "Click"))
            .class("btn")
            .class("sm")
            .toggle_class("is-mod", click_on)
            .on_press(|cx| cx.emit(AppEvent::ToggleClick));

        Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(16.0));

        HStack::new(cx, |cx| {
            Label::new(cx, "128.00").class("mono");
            Label::new(cx, "BPM").class("unit");
        })
        .class("readout")
        .gap(Pixels(4.0))
        .size(Auto);

        Label::new(cx, "4/4").class("readout").size(Auto);

        let position_text = position.map(|p| format!("{}.{}.{}", p.bar, p.beat, p.sixteenth));
        Label::new(cx, position_text).class("readout").size(Auto);

        Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(16.0));

        Label::new(cx, "CPU 0%").class("meta");

        Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));

        Button::new(cx, |cx| Label::new(cx, "Intervals"))
            .class("btn")
            .class("sm")
            .toggle_class("is-mod", interval_open)
            .on_press(|cx| cx.emit(IntervalInputEvent::ToggleOpen));
    })
    .class("transport")
    .gap(Pixels(SPACE_2))
    .padding_left(Pixels(SPACE_3))
    .padding_right(Pixels(SPACE_3))
    .alignment(Alignment::Left)
    .height(Auto)
    .width(Stretch(1.0));
}
