//! The Drum Kit's device panel: one pad per kit sound. Clicking a pad plays
//! it on the selected track (and, while step entry is armed, records it),
//! the same way a key on Carve's keyboard does.

use vizia::prelude::*;

use shared::arrangement::ClipColor;
use shared::drums::DRUM_KIT;

use crate::synth::state::SynthEvent;
use crate::tokens;

pub fn drum_kit_panel(cx: &mut Context, color: ClipColor) {
    let accent = crate::timeline::header::clip_color_to_rgb(color);
    HStack::new(cx, move |cx| {
        for pad in &DRUM_KIT {
            let note = pad.note;
            let octave = note as i32 / 12 - 1;
            let note_label = format!("{}{} \u{b7} {}", shared::theory::scale::note_name(note % 12), octave, note);
            Button::new(cx, move |cx| {
                VStack::new(cx, move |cx| {
                    Element::new(cx).background_color(accent).width(Stretch(1.0)).height(Pixels(3.0)).hoverable(false);
                    Label::new(cx, pad.name).class("label").hoverable(false);
                    Label::new(cx, note_label.clone()).class("value").hoverable(false);
                })
                .gap(Pixels(4.0))
                .hoverable(false)
            })
            .class("btn")
            .class("drum-pad")
            .width(Pixels(92.0))
            .height(Pixels(64.0))
            // Drums are one-shots: a click is a hit, played on mouse-down
            // for timing. The release that follows ends the step-entry chord.
            .on_press_down(move |cx| {
                cx.emit(SynthEvent::KeyPress(note));
                cx.emit(SynthEvent::KeyRelease(note));
            });
        }
    })
    .class("device")
    .gap(Pixels(tokens::SPACE_2))
    .alignment(Alignment::Left)
    .padding(Pixels(tokens::SPACE_3))
    .width(Stretch(1.0))
    .height(Pixels(96.0));
}
