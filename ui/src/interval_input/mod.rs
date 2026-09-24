//! Interval Input: three linked views of one scale (true-spacing ruler,
//! tonnetz lattice, chord ring) for exploring intervals and chords by
//! ear. Tapping a pad in any of the three plays the note for real through
//! Carve (same `SynthEvent::ToggleKey` the on-screen keyboard uses), and
//! all three highlight from Carve's own `held_notes` - so they, the
//! keyboard, and step-entry recording all agree on what's currently held.

pub mod lattice;
pub mod ring;
pub mod spacing;
pub mod state;

use vizia::prelude::*;

use shared::synth::SynthState;
use shared::theory::{degree_name, note_name};

use crate::tokens::{self, ThemeId};
use lattice::Lattice;
use ring::Ring;
use spacing::Spacing;
use state::{scale_name, IntervalInputEvent};

#[allow(clippy::too_many_arguments)]
pub fn interval_input_view(
    cx: &mut Context,
    theme: Signal<ThemeId>,
    synth_state: Signal<SynthState>,
    key: Signal<u8>,
    scale_mask: Signal<u16>,
    open: Signal<bool>,
) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Label::new(cx, "Interval Input").class("control");
            Label::new(cx, "explorations").class("meta");
            Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(16.0));

            let key_text = key.map(|k| format!("key {}", note_name(*k)));
            Button::new(cx, move |cx| Label::new(cx, key_text)).class("btn").class("sm").on_press(
                move |cx| cx.emit(IntervalInputEvent::SetKey((key.get() + 1) % 12)),
            );

            let scale_text = scale_mask.map(|m| scale_name(*m).to_string());
            Button::new(cx, move |cx| Label::new(cx, scale_text))
                .class("btn")
                .class("sm")
                .on_press(|cx| cx.emit(IntervalInputEvent::CyclePreset));

            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));

            HStack::new(cx, move |cx| {
                for degree in 0u8..12 {
                    let name = degree_name(degree);
                    let on = scale_mask.map(move |m| m & (1 << degree) != 0);
                    Button::new(cx, move |cx| Label::new(cx, name).class("mono"))
                        .class("synth-seg-btn")
                        .toggle_class("is-on", on)
                        .on_press(move |cx| cx.emit(IntervalInputEvent::ToggleDegree(degree)));
                }
            })
            .class("synth-seg")
            .size(Auto);

            Button::new(cx, |cx| Label::new(cx, "Close"))
                .class("btn")
                .class("sm")
                .on_press(|cx| cx.emit(IntervalInputEvent::ToggleOpen));
        })
        .class("synth-devhead")
        .gap(Pixels(tokens::SPACE_2))
        .alignment(Alignment::Left)
        .width(Stretch(1.0));

        section(cx, "A \u{b7} True spacing", "pads sit on a semitone ruler; gaps show interval size", move |cx| {
            Spacing::new(cx, synth_state, theme, key, scale_mask)
                .class("synth-disp")
                .width(Pixels(748.0))
                .height(Pixels(110.0));
        });

        HStack::new(cx, move |cx| {
            section(cx, "B \u{b7} Lattice", "\u{2192} 5ths  \u{2197} maj 3rds  \u{2198} min 3rds \u{b7} a triangle is a triad", move |cx| {
                Lattice::new(cx, synth_state, theme, key, scale_mask)
                    .class("synth-disp")
                    .width(Pixels(480.0))
                    .height(Pixels(260.0));
            });
            section(cx, "C \u{b7} Ring", "tap to stack; the shape is the chord", move |cx| {
                Ring::new(cx, synth_state, theme, key, scale_mask)
                    .class("synth-disp")
                    .width(Pixels(260.0))
                    .height(Pixels(260.0));
            });
        })
        .gap(Pixels(tokens::SPACE_2))
        .width(Auto)
        .height(Auto);
    })
    .class("panel")
    .class("interval-overlay")
    .toggle_class("hidden", open.map(|o| !*o))
    .gap(Pixels(tokens::SPACE_2))
    .padding(Pixels(tokens::SPACE_2))
    .position_type(PositionType::Absolute)
    .top(Pixels(48.0))
    .left(Pixels(260.0))
    .width(Auto)
    .height(Auto);
}

fn section(cx: &mut Context, label: &'static str, next: &'static str, content: impl FnOnce(&mut Context)) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Label::new(cx, label).class("label");
            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
            Label::new(cx, next).class("meta");
        })
        .width(Stretch(1.0))
        .height(Auto);

        content(cx);
    })
    .class("synth-sec")
    .gap(Pixels(tokens::SPACE_2))
    .width(Auto)
    .height(Auto);
}
