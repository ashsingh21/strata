//! The Drum Kit's device panel: the kit (Standard or Tabla), then one pad
//! per sound. Clicking a pad plays it on the selected track (and, while
//! step entry is armed, records it), the same way a key on Carve's
//! keyboard does. Under each pad: mute, level and tuning for that pad on
//! this track. Drop a sample from the browser on a pad and it plays that
//! instead; x puts the kit's own back.

use vizia::prelude::*;

use shared::arrangement::{Arrangement, ClipColor, TrackId};
use shared::drums::{Kit, PadSettings};

use crate::knob::Knob;
use crate::timeline::state::TimelineEvent;
use crate::tokens::ThemeId;

/// A pad's level knob: -24 to +6 dB. Its tuning knob: an octave either way.
const GAIN_RANGE: (f32, f32) = (-24.0, 6.0);
const PITCH_RANGE: f32 = 12.0;

use crate::synth::state::SynthEvent;
use crate::tokens;

pub fn drum_kit_panel(cx: &mut Context, color: ClipColor, track: TrackId, arrangement: Signal<Arrangement>, theme: Signal<ThemeId>) {
    let kit = Memo::new(move |_| arrangement.get().track(track).map(|t| t.drum_pads.kit).unwrap_or_default());
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Label::new(cx, "Kit").class("label");
            crate::synth::segmented::segmented(
                cx,
                Kit::ALL.len(),
                |cx, i| Label::new(cx, Kit::ALL[i].name()),
                move |i| kit.map(move |k| *k == Kit::ALL[i]),
                move |cx, i| cx.emit(TimelineEvent::SetDrumKit { track, kit: Kit::ALL[i] }),
            )
            .height(Pixels(tokens::SIZE_CONTROL));
            Label::new(
                cx,
                kit.map(|k| match k {
                    Kit::Chop => "Drop a loop here to cut it across the pads, or a sample on one pad",
                    _ => "Drag a sample from the browser onto a pad to play it instead",
                }),
            )
            .class("value")
            .class("empty-note");
            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
            // Swing and Humanize for the whole track (the clip editor has
            // them per clip): the first clip's swing shows.
            let swing = Memo::new(move |_| {
                let arr = arrangement.get();
                arr.clips.iter().find(|c| c.track == track && matches!(c.content, shared::arrangement::ClipContent::Midi { .. })).map(|c| c.swing).unwrap_or(0.0)
            });
            Label::new(cx, "Swing").class("label");
            crate::knob::Knob::plain(cx, swing, 0.0, theme, move |cx, value| cx.emit(TimelineEvent::SetTrackSwing { track, swing: value }))
                .tooltip(|cx| Tooltip::new(cx, |cx| { Label::new(cx, "Pushes the off-beat 16ths late in every clip on this track, for a shuffle"); }).arrow(false))
                .size(Pixels(20.0));
            Label::new(cx, swing.map(|s| format!("{:.0}%", s * 100.0))).class("value").width(Pixels(30.0));
            Button::new(cx, |cx| Label::new(cx, "Humanize"))
                .class("btn")
                .class("sm")
                .tooltip(|cx| Tooltip::new(cx, |cx| { Label::new(cx, "Nudges every hit on this track a little, like a drummer. Undo takes it back."); }).arrow(false))
                .on_press(move |cx| cx.emit(TimelineEvent::HumanizeTrack(track)));
        })
        .gap(Pixels(tokens::SPACE_2))
        .alignment(Alignment::Left)
        .width(Stretch(1.0))
        .height(Auto);
        // The pads, rebuilt when the kit changes.
        Binding::new(cx, kit, move |cx| pads(cx, color, track, kit.get(), arrangement, theme));
    })
    .class("device")
    // Chop: a loop dropped on the panel (not on one pad) fills them all.
    .on_drop(move |cx, _| {
        if kit.get() != Kit::Chop {
            return;
        }
        let Some(source) = crate::browser::view::dragged().and_then(|item| item.source().cloned()) else { return };
        if let Some(pads) = chop(&source, arrangement.get().track(track).map(|t| t.drum_pads).unwrap_or_default()) {
            cx.emit(TimelineEvent::SetDrumPads { track, pads });
        }
    })
    .gap(Pixels(tokens::SPACE_3))
    .padding(Pixels(tokens::SPACE_3))
    .width(Stretch(1.0))
    .height(Auto);
}

/// `source` cut into a slice per Chop pad, keeping each pad's mute, level
/// and tuning. `None` if it can't be read.
fn chop(source: &str, mut pads: shared::drums::Pads) -> Option<shared::drums::Pads> {
    let path = crate::paths::audio_file(&crate::timeline::assets_dir(), source);
    let (samples, spec) = crate::timeline::peaks_loader::decode_wav(&path)?;
    let channels = spec.channels.max(1) as usize;
    let mono: Vec<f32> = samples.chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect();
    let slices = shared::drums::chop_points(&mono, spec.sample_rate, Kit::Chop.pads().len());
    let name = shared::drums::intern(source);
    pads.kit = Kit::Chop;
    for (pad, slice) in pads.pads.iter_mut().zip(slices) {
        pad.sample = Some(name);
        pad.slice = Some(slice);
    }
    Some(pads)
}

fn pads(cx: &mut Context, color: ClipColor, track: TrackId, kit: Kit, arrangement: Signal<Arrangement>, theme: Signal<ThemeId>) {
    let accent = crate::timeline::header::clip_color_to_rgb(color);
    let all = kit.pads();
    // Eight pads: two rows of four, as on the MPK Mini - 5-8 above, 1-4 below.
    let rows: Vec<std::ops::Range<usize>> = if all.len() > 5 { vec![4..all.len(), 0..4] } else { vec![0..all.len()] };
    VStack::new(cx, move |cx| {
    for range in rows {
    HStack::new(cx, move |cx| {
        for index in range.clone() {
            let pad = &all[index];
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
            // Your own sample's name under the pad's, when it has one.
            let detail = settings.map(move |s| match (s.sample, s.slice) {
                (Some(sample), slice) => {
                    let stem = std::path::Path::new(sample).file_stem().and_then(|f| f.to_str()).unwrap_or(sample);
                    match slice {
                        // Where in the loop the slice starts (the loop's
                        // name is the same on every pad).
                        Some((start, _)) => format!("from {:.0}%", start * 100.0),
                        None => stem.to_string(),
                    }
                }
                (None, _) if kit == Kit::Chop => "empty".to_string(),
                (None, _) => note_label.clone(),
            });
            Button::new(cx, move |cx| {
                VStack::new(cx, move |cx| {
                    Element::new(cx).background_color(accent).width(Stretch(1.0)).height(Pixels(3.0)).hoverable(false);
                    Label::new(cx, pad.name).class("label").hoverable(false);
                    Label::new(cx, detail)
                        .class("value")
                        .text_wrap(false)
                        .text_overflow(TextOverflow::Ellipsis)
                        .width(Stretch(1.0))
                        .hoverable(false);
                })
                .gap(Pixels(4.0))
                .alignment(Alignment::Center)
                .padding_left(Pixels(6.0))
                .padding_right(Pixels(6.0))
                .width(Stretch(1.0))
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
            .toggle_class("is-muted", settings.map(|s| s.mute))
            // A sample from the browser dropped here: this pad plays it.
            .on_drop(move |cx, _| {
                if let Some(source) = crate::browser::view::dragged().and_then(|item| item.source().cloned()) {
                    let name = shared::drums::intern(&source);
                    set(cx, &move |s| {
                        s.sample = Some(name);
                        s.slice = None;
                    });
                }
            });
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
                // Back to the kit's own sound.
                Button::new(cx, |cx| Label::new(cx, "\u{2715}"))
                    .class("btn")
                    .class("sm")
                    .class("quiet")
                    .tooltip(|cx| Tooltip::new(cx, |cx| { Label::new(cx, "Back to the kit's own sound"); }).arrow(false))
                    .toggle_class("hidden", settings.map(|s| s.sample.is_none()))
                    .on_press(move |cx| {
                        set(cx, &|s| {
                            s.sample = None;
                            s.slice = None;
                        })
                    });
            })
            .gap(Pixels(4.0))
            .alignment(Alignment::Left)
            .size(Auto);
            })
            .gap(Pixels(6.0))
            .size(Auto);
        }
    })
    .gap(Pixels(tokens::SPACE_2))
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Pixels(96.0));
    }
    })
    .gap(Pixels(tokens::SPACE_2))
    .width(Stretch(1.0))
    .height(Auto);
}
