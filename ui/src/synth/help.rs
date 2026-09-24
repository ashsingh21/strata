//! Carve's interactive help manual: a toggleable overlay (the "?" button
//! in the device header) explaining what every knob and control does to
//! the sound, since the panel itself has no room for that much text.

use vizia::prelude::*;

use crate::tokens;

/// One section of the manual: a heading plus `(name, description)` lines.
const SECTIONS: &[(&str, &[(&str, &str)])] = &[
    (
        "Voice",
        &[
            ("Mono / Poly", "Mono plays one note at a time with glide between them; Poly lets multiple notes sound together."),
            ("Voices", "The maximum number of notes that can sound at once in Poly mode."),
        ],
    ),
    (
        "Osc 1 / Osc 2",
        &[
            ("Waveform", "sin/tri/saw/sq - the oscillator's basic tone colour, from pure and soft (sine) to bright and buzzy (square)."),
            ("Octave", "Transposes the oscillator up or down by whole octaves."),
            ("Tune / Detune", "Fine pitch offset in cents. Detuning Osc 2 slightly from Osc 1 thickens the sound with beating."),
            ("Shape / PW", "Morphs the waveform's character - adds a harmonic to a sine, skews a triangle toward a saw, rounds a saw toward a triangle, or changes a square's pulse width."),
            ("Drift", "Osc 1 only. A slow, random pitch wander that adds analog-style warmth and instability."),
            ("FM", "Osc 2 only. Frequency-modulates Osc 2 by Osc 1's waveform, for metallic or bell-like tones as it increases."),
            ("Sync", "Osc 2 only. Hard-resets Osc 2's cycle every time Osc 1 completes one, for an aggressive, tearing sync tone."),
        ],
    ),
    (
        "Mix",
        &[
            ("Osc 1 / Osc 2", "How loud each oscillator is in the blend."),
            ("Sub", "Level of a sine an octave below Osc 1 - adds low-end weight without extra harmonics."),
            ("Noise", "Level of white noise mixed in - adds breath, air or edge."),
        ],
    ),
    (
        "Filter",
        &[
            ("Type", "LP24/LP12 pass low frequencies (24 or 12 dB/oct steeper or gentler); BP passes a narrow band; HP passes highs only."),
            ("Cutoff", "The frequency the filter acts around - the main control for how bright or dark the sound is."),
            ("Reso", "Emphasises frequencies right at the cutoff; high settings make the filter ring or whistle there."),
            ("Drive", "Saturates the signal before it hits the filter, adding grit and harmonics."),
            ("Env", "How far the Filter Env pushes the cutoff up (positive) or down (negative) over the note."),
            ("Key trk", "How much the cutoff follows the pitch you play, so higher notes stay proportionally as bright as lower ones."),
        ],
    ),
    (
        "Filter Env / Amp Env",
        &[
            ("A - Attack", "Time to rise to full level when a note starts."),
            ("D - Decay", "Time to fall from full level down to the Sustain level."),
            ("S - Sustain", "The level held for as long as the note stays down."),
            ("R - Release", "Time to fade to silence after the note is released."),
            ("Filter Env", "Shapes the filter's Cutoff over time (via the Filter section's Env knob)."),
            ("Amp Env", "Shapes the note's volume over time - this is what gives a note its basic shape."),
        ],
    ),
    (
        "Mod",
        &[
            ("Rate", "How fast the LFO cycles."),
            ("Depth", "How strongly the LFO affects its target."),
            ("Sync", "Locks the LFO's rate to the song tempo instead of running free (display only for now)."),
            ("Target button", "Click to choose what the LFO modulates: filter Cutoff (a wah-like sweep) or Pitch (vibrato)."),
        ],
    ),
    (
        "Out",
        &[
            ("Glide", "In Mono mode, the time it takes to slide from one note's pitch to the next."),
            ("Volume", "The synth's overall output level."),
        ],
    ),
    (
        "Keyboard",
        &[
            ("Mouse", "Click a key to play it, click again to release."),
            ("Computer keyboard", "Z X C V B N M , . / play white keys; S D G H J L ; play the black keys between them."),
            ("+ / -", "Shift the whole computer keyboard up or down by an octave; shown as the \"Oct\" readout."),
        ],
    ),
];

pub fn help_overlay(cx: &mut Context, open: Signal<bool>) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Label::new(cx, "Carve manual").class("control");
            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
            Button::new(cx, |cx| Label::new(cx, "Close"))
                .class("btn")
                .class("sm")
                .on_press(|cx| cx.emit(super::state::SynthEvent::ToggleHelp));
        })
        .alignment(Alignment::Center)
        .width(Stretch(1.0))
        .height(Auto);

        ScrollView::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                for (heading, lines) in SECTIONS {
                    Label::new(cx, *heading).class("label");
                    for (name, description) in *lines {
                        Label::new(cx, format!("{name} - {description}"))
                            .class("meta")
                            .text_wrap(true)
                            .width(Stretch(1.0));
                    }
                }
            })
            .gap(Pixels(tokens::SPACE_2))
            .width(Stretch(1.0))
            .height(Auto);
        })
        .width(Stretch(1.0))
        .height(Stretch(1.0));
    })
    .class("panel")
    .class("synth-help-panel")
    .toggle_class("hidden", open.map(|b| !*b))
    .gap(Pixels(tokens::SPACE_2))
    .padding(Pixels(tokens::SPACE_3))
    .position_type(PositionType::Absolute)
    .top(Pixels(0.0))
    .left(Pixels(0.0))
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}
