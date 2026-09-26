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
use shared::theory::{degree_name, note_name, note_name_for_key};

use crate::tokens::{self, ThemeId};
use lattice::Lattice;
use ring::Ring;
use spacing::Spacing;
use state::{scale_name, IntervalInputEvent};

/// Row 3 of the device area: docked under Carve (not floating over the
/// arrangement), toggled by "Show input" in the device chain or the Key
/// button in the header. Built only while open - a `Binding` rather than a
/// `display: none` toggle, since text shown from hidden never laid out.
#[allow(clippy::too_many_arguments)]
pub fn interval_input_view(
    cx: &mut Context,
    theme: Signal<ThemeId>,
    synth_state: Signal<SynthState>,
    key: Signal<u8>,
    scale_mask: Signal<u16>,
    open: Signal<bool>,
    show_note_names: Signal<bool>,
) {
    Binding::new(cx, open, move |cx| {
        if !open.get() {
            return;
        }
        VStack::new(cx, move |cx| {
            HStack::new(cx, move |cx| {
                Label::new(cx, "Interval input").class("heading");

                let key_text = key.map(|k| format!("Key {}", note_name(*k)));
                Button::new(cx, move |cx| Label::new(cx, key_text))
                    .class("btn")
                    .class("sm")
                    .on_press(move |cx| cx.emit(IntervalInputEvent::SetKey((key.get() + 1) % 12)));

                let scale_text = scale_mask.map(|m| scale_name(*m).to_string());
                Button::new(cx, move |cx| Label::new(cx, scale_text))
                    .class("btn")
                    .class("sm")
                    .on_press(|cx| cx.emit(IntervalInputEvent::CyclePreset));

                crate::synth::segmented::segmented(
                    cx,
                    2,
                    |cx, i| Label::new(cx, if i == 0 { "Notes" } else { "Intervals" }),
                    move |i| show_note_names.map(move |notes| *notes == (i == 0)),
                    move |cx, i| {
                        if show_note_names.get() != (i == 0) {
                            cx.emit(IntervalInputEvent::ToggleLabelMode);
                        }
                    },
                );

                Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));

                HStack::new(cx, move |cx| {
                    for degree in 0u8..12 {
                        let name = key.map(move |k| {
                            if show_note_names.get() {
                                note_name_for_key((*k + degree) % 12, *k).to_string()
                            } else {
                                degree_name(degree).to_string()
                            }
                        });
                        let on = scale_mask.map(move |m| m & (1 << degree) != 0);
                        Button::new(cx, move |cx| Label::new(cx, name))
                            .class("synth-seg-btn")
                            .toggle_class("is-on", on)
                            .on_press(move |cx| cx.emit(IntervalInputEvent::ToggleDegree(degree)));
                    }
                })
                .class("synth-seg")
                .size(Auto);
            })
            .class("synth-devhead")
            .gap(Pixels(tokens::SPACE_2))
            .alignment(Alignment::Left)
            .width(Stretch(1.0))
            .height(Pixels(tokens::SIZE_TOOLBAR));
            Element::new(cx).class("hairline").width(Stretch(1.0)).height(Pixels(1.0));

            HStack::new(cx, move |cx| {
                section(cx, "True spacing", move |cx| {
                    Spacing::new(cx, synth_state, theme, key, scale_mask)
                        .class("synth-disp")
                        .width(Stretch(1.0))
                        .height(Stretch(1.0));
                })
                .width(Stretch(1.0));
                Element::new(cx).class("hairline").width(Pixels(1.0)).height(Stretch(1.0));
                section(cx, "Lattice", move |cx| {
                    Lattice::new(cx, synth_state, theme, key, scale_mask)
                        .class("synth-disp")
                        .width(Stretch(1.0))
                        .height(Stretch(1.0));
                })
                .width(Pixels(420.0));
                Element::new(cx).class("hairline").width(Pixels(1.0)).height(Stretch(1.0));
                section(cx, "Ring", move |cx| {
                    Ring::new(cx, synth_state, theme, key, scale_mask)
                        .class("synth-disp")
                        .width(Stretch(1.0))
                        .height(Stretch(1.0));
                })
                .width(Pixels(240.0));
            })
            .width(Stretch(1.0))
            .height(Pixels(DOCK_BODY_HEIGHT));
        })
        .class("device")
        .width(Stretch(1.0))
        .height(Auto);
    });
}

/// The docked views' height: tall enough for the lattice's four rows and
/// the ring, short enough to leave the arrangement most of the window.
const DOCK_BODY_HEIGHT: f32 = 236.0;

fn section<'a>(cx: &'a mut Context, title: &'static str, content: impl FnOnce(&mut Context)) -> Handle<'a, VStack> {
    VStack::new(cx, move |cx| {
        Label::new(cx, title).class("title").height(Pixels(tokens::SIZE_CONTROL));
        content(cx);
    })
    .class("synth-sec")
    .gap(Pixels(tokens::SPACE_2))
    .height(Stretch(1.0))
}
