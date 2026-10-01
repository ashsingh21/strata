//! The Drum Kit's device panel: one pad per kit sound. Clicking a pad plays
//! it on the selected track (and, while step entry is armed, records it),
//! the same way a key on Carve's keyboard does. Under each pad: mute, level
//! and tuning for that pad on this track.

use vizia::prelude::*;

use shared::arrangement::{Arrangement, ClipColor, TrackId};
use shared::drums::{PadSettings, DRUM_KIT};

use crate::knob::Knob;
use crate::timeline::state::TimelineEvent;
use crate::tokens::ThemeId;

/// A pad's level knob: -24 to +6 dB. Its tuning knob: an octave either way.
const GAIN_RANGE: (f32, f32) = (-24.0, 6.0);
const PITCH_RANGE: f32 = 12.0;

use crate::synth::state::SynthEvent;
use crate::tokens;

pub fn drum_kit_panel(cx: &mut Context, color: ClipColor, track: TrackId, arrangement: Signal<Arrangement>, theme: Signal<ThemeId>) {
    let accent = crate::timeline::header::clip_color_to_rgb(color);
    HStack::new(cx, move |cx| {
        for (index, pad) in DRUM_KIT.iter().enumerate() {
            let settings = Memo::new(move |_| {
                arrangement.get().track(track).and_then(|t| t.drum_pads.get(index).copied()).unwrap_or_default()
            });
            let set = move |cx: &mut EventContext, change: &dyn Fn(&mut PadSettings)| {
                let mut s = settings.get();
                change(&mut s);
                cx.emit(TimelineEvent::SetDrumPad { track, pad: index, settings: s });
            };
            VStack::new(cx, move |cx| {
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
            })
            .toggle_class("is-muted", settings.map(|s| s.mute));
            // Mute, level and tuning for this pad.
            HStack::new(cx, move |cx| {
                Button::new(cx, |cx| Label::new(cx, "M"))
                    .class("btn")
                    .class("sm")
                    .toggle_class("is-mute", settings.map(|s| s.mute))
                    .on_press(move |cx| set(cx, &|s| s.mute = !s.mute));
                let gain = settings.map(|s| ((s.gain_db - GAIN_RANGE.0) / (GAIN_RANGE.1 - GAIN_RANGE.0)).clamp(0.0, 1.0));
                let gain_zero = -GAIN_RANGE.0 / (GAIN_RANGE.1 - GAIN_RANGE.0);
                Knob::plain(cx, gain, gain_zero, theme, move |cx, v| {
                    set(cx, &|s| s.gain_db = GAIN_RANGE.0 + v * (GAIN_RANGE.1 - GAIN_RANGE.0))
                })
                .tooltip(|cx| Tooltip::new(cx, |cx| { Label::new(cx, "Level"); }).arrow(false))
                .size(Pixels(22.0));
                let pitch = settings.map(|s| (s.pitch / PITCH_RANGE * 0.5 + 0.5).clamp(0.0, 1.0));
                Knob::plain(cx, pitch, 0.5, theme, move |cx, v| {
                    // Whole semitones: a drum tuned in steps, like a pad on a sampler.
                    set(cx, &|s| s.pitch = ((v - 0.5) * 2.0 * PITCH_RANGE).round())
                })
                .tooltip(|cx| Tooltip::new(cx, |cx| { Label::new(cx, "Tune (semitones)"); }).arrow(false))
                .size(Pixels(22.0));
                Label::new(
                    cx,
                    settings.map(|s| {
                        let signed = |v: f32| format!("{v:+.0}").replace('-', "\u{2212}");
                        let pitch = if s.pitch == 0.0 { String::new() } else { format!(" \u{b7} {} st", signed(s.pitch)) };
                        format!("{} dB{pitch}", signed(s.gain_db))
                    }),
                )
                .class("value");
            })
            .gap(Pixels(4.0))
            .alignment(Alignment::Left)
            .size(Auto);
            })
            .gap(Pixels(6.0))
            .size(Auto);
        }
    })
    .class("device")
    .gap(Pixels(tokens::SPACE_2))
    .alignment(Alignment::Left)
    .padding(Pixels(tokens::SPACE_3))
    .width(Stretch(1.0))
    .height(Pixels(124.0));
}
