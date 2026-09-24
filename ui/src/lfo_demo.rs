//! The LFO demo: a standalone "Cutoff" knob whose modulation ring is
//! animated by a UI-side 0.5 Hz sine LFO, plus an "LFO 1" pill.
//!
//! Not mounted in the main view: it was milestone 1's proof that the
//! Knob's modulation ring renders and animates correctly, not a real
//! device. Its eventual home is the subtractive-synth device panel.
#![allow(dead_code)]

use vizia::prelude::*;

use crate::app::AppEvent;
use crate::knob::Knob;
use crate::pill::modulator_pill;
use crate::tokens::{ThemeId, SIZE_KNOB, SPACE_2};

pub fn lfo_demo(
    cx: &mut Context,
    theme: Signal<ThemeId>,
    cutoff: Signal<f32>,
    cutoff_mod_center: Signal<f32>,
    cutoff_mod_depth: Signal<f32>,
) {
    VStack::new(cx, move |cx| {
        modulator_pill(cx, theme, "LFO 1", 1);

        Knob::new(
            cx,
            cutoff,
            0.45,
            theme,
            Some((cutoff_mod_center, cutoff_mod_depth)),
            |cx, value| cx.emit(AppEvent::SetCutoff(value)),
        )
        .size(Pixels(SIZE_KNOB));

        Label::new(cx, "Cutoff").class("label");

        let value_text = cutoff.map(|v| format!("{:.2} kHz", 0.1 + v * 19.9));
        Label::new(cx, value_text).class("mono");
    })
    .gap(Pixels(SPACE_2))
    .alignment(Alignment::Center)
    .width(Auto)
    .height(Auto);
}
