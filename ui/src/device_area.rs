//! The lower panel's contents: the selected track's device chain, then
//! whatever that track shows - its open clip's editor, its Carve, or (for
//! a MIDI track with no instrument, an audio track, or no selection) a
//! short empty state - with the Interval Input docked underneath.

use std::collections::HashSet;

use vizia::prelude::*;

use shared::arrangement::{Arrangement, ClipId, Effect, Instrument, SnapGrid, Ticks, TrackId, TrackKind};
use shared::synth::SynthState;

use crate::compressor_panel;
use crate::interval_input;
use crate::interval_input::state::IntervalInputEvent;
use crate::piano_roll;
use crate::piano_roll::state::{EditMode, LabelMode, NoteKey, PianoRollEvent};
use crate::synth;
use crate::synth::state::SynthEvent;
use crate::timeline::state::TimelineEvent;
use crate::tokens::{self, ThemeId};

/// What the selected track puts in the panel.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Panel {
    Carve,
    Compressor(TrackId),
    NoInstrument(TrackId),
    Audio,
    Nothing,
}

#[derive(Clone, Copy)]
pub struct DeviceAreaProps {
    pub theme: Signal<ThemeId>,
    pub arrangement: Signal<Arrangement>,
    pub selected_track: Signal<Option<TrackId>>,
    /// Whether the panel shows the Compressor instead of the instrument/
    /// empty state - toggled by the Compressor chip.
    pub viewing_effect: Signal<bool>,
    // Carve.
    pub synth_state: Signal<SynthState>,
    pub lfo_phases: (Signal<f32>, Signal<f32>),
    pub octave_shift: Signal<i8>,
    pub meter_l: Signal<f32>,
    pub meter_r: Signal<f32>,
    pub help_open: Signal<bool>,
    pub lfo_drag: Signal<Option<usize>>,
    // Piano roll.
    pub open_clip: Signal<Option<ClipId>>,
    pub edit_mode: Signal<EditMode>,
    pub label_mode: Signal<LabelMode>,
    pub selected_notes: Signal<HashSet<NoteKey>>,
    pub snap: Signal<SnapGrid>,
    pub playhead: Signal<Ticks>,
    // Interval Input.
    pub key: Signal<u8>,
    pub scale_mask: Signal<u16>,
    pub interval_open: Signal<bool>,
    pub show_note_names: Signal<bool>,
}

pub fn device_area(cx: &mut Context, p: DeviceAreaProps) {
    let panel = Memo::new(move |_| {
        let arr = p.arrangement.get();
        match p.selected_track.get().and_then(|id| arr.track(id).cloned()) {
            Some(t) if p.viewing_effect.get() && t.fx.ordered().iter().any(|n| matches!(n.effect, Effect::Compressor(_))) => {
                Panel::Compressor(t.id)
            }
            Some(t) if t.kind == TrackKind::Audio => Panel::Audio,
            Some(t) if t.instrument.is_some() => Panel::Carve,
            Some(t) => Panel::NoInstrument(t.id),
            None => Panel::Nothing,
        }
    });

    device_chain(cx, p, panel);

    // The editor replaces the device while a clip is open.
    Binding::new(cx, p.open_clip, move |cx| {
        if p.open_clip.get().is_some() {
            piano_roll::piano_roll_view(
                cx,
                p.theme,
                p.arrangement,
                p.open_clip,
                p.edit_mode,
                p.label_mode,
                p.selected_notes,
                p.snap,
                p.key,
                p.scale_mask,
                p.playhead,
            );
            return;
        }
        Binding::new(cx, panel, move |cx| match panel.get() {
            // Rebuilt per track, so the knobs take that track's colour.
            Panel::Carve => Binding::new(cx, p.selected_track, move |cx| {
                let color = p
                    .selected_track
                    .get()
                    .and_then(|id| p.arrangement.get().track(id).map(|t| t.color))
                    .unwrap_or(shared::arrangement::ClipColor::Violet);
                synth::synth_view(
                    cx,
                    p.theme,
                    p.synth_state,
                    p.lfo_phases,
                    p.octave_shift,
                    p.meter_l,
                    p.meter_r,
                    p.help_open,
                    p.lfo_drag,
                    color,
                );
            }),
            Panel::Compressor(track) => {
                let color =
                    p.arrangement.get().track(track).map(|t| t.color).unwrap_or(shared::arrangement::ClipColor::Violet);
                compressor_panel::compressor_panel(cx, p.theme, p.arrangement, track, color);
            }
            Panel::NoInstrument(track) => empty_state(cx, "No instrument on this track", move |cx| {
                Button::new(cx, |cx| Label::new(cx, "Add Carve"))
                    .class("btn")
                    .on_press(move |cx| {
                        cx.emit(TimelineEvent::SetInstrument { track, instrument: Some(Instrument::Carve) })
                    });
            }),
            Panel::Audio => empty_state(cx, "Audio track \u{b7} no instrument", |_| {}),
            Panel::Nothing => empty_state(cx, "No track selected", |_| {}),
        });
    });

    interval_input::interval_input_view(
        cx,
        p.theme,
        p.synth_state,
        p.key,
        p.scale_mask,
        p.interval_open,
        p.show_note_names,
    );
}

/// The selected track's swatch and name, then its devices as chips: the
/// open clip (raised while editing), its instrument (click to go back to
/// it; x to remove it) or "+ Carve", and Show input on the right.
fn device_chain(cx: &mut Context, p: DeviceAreaProps, panel: Memo<Panel>) {
    HStack::new(cx, move |cx| {
        let track = Memo::new(move |_| {
            p.selected_track.get().and_then(|id| p.arrangement.get().track(id).map(|t| (t.name.clone(), t.color)))
        });
        let color = track.map(|t| {
            t.as_ref().map(|(_, c)| crate::timeline::header::clip_color_to_rgb(*c)).unwrap_or(tokens::CLIP_VIOLET)
        });
        Element::new(cx).class("swatch").background_color(color).toggle_class("hidden", track.map(|t| t.is_none()));
        Label::new(cx, track.map(|t| t.as_ref().map(|(name, _)| name.clone()).unwrap_or_default())).class("title");
        Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(16.0));

        let editing = p.open_clip.map(|c| c.is_some());
        let clip_name = Memo::new(move |_| {
            p.open_clip.get().and_then(|id| p.arrangement.get().clip(id).map(|c| c.name.clone())).unwrap_or_default()
        });
        Button::new(cx, move |cx| Label::new(cx, clip_name))
            .class("btn")
            .class("is-on")
            .toggle_class("hidden", editing.map(|e| !*e));

        // "Carve"/"NoInstrument" reflect what's actually showing (`panel`,
        // which already accounts for `viewing_effect`); the chips
        // themselves only care whether the track *has* an instrument, so
        // they still show even while the Compressor is the one on screen.
        let has_instrument = Memo::new(move |_| {
            p.selected_track.get().and_then(|id| p.arrangement.get().track(id).map(|t| t.instrument.is_some())).unwrap_or(false)
        });
        Button::new(cx, |cx| Label::new(cx, "Carve"))
            .class("btn")
            .toggle_class("is-on", Memo::new(move |_| panel.get() == Panel::Carve && !editing.get()))
            .toggle_class("hidden", has_instrument.map(|c| !*c))
            .on_press(move |cx| {
                p.viewing_effect.set(false);
                cx.emit(PianoRollEvent::Close);
            });
        Button::new(cx, |cx| Label::new(cx, "\u{2715}"))
            .class("btn")
            .class("quiet")
            .toggle_class("hidden", has_instrument.map(|c| !*c))
            .on_press(move |cx| {
                if let Some(track) = p.selected_track.get() {
                    cx.emit(TimelineEvent::SetInstrument { track, instrument: None });
                }
            });

        let no_instrument = Memo::new(move |_| {
            !has_instrument.get()
                && p.selected_track
                    .get()
                    .and_then(|id| p.arrangement.get().track(id).map(|t| t.kind == TrackKind::Midi))
                    .unwrap_or(false)
        });
        Button::new(cx, |cx| Label::new(cx, "+ Carve"))
            .class("btn")
            .class("quiet")
            .toggle_class("hidden", no_instrument.map(|n| !*n))
            .on_press(|cx| cx.emit(SynthEvent::AddCarveToSelected));

        let has_compressor = Memo::new(move |_| {
            p.selected_track
                .get()
                .and_then(|id| p.arrangement.get().track(id).map(|t| t.fx.ordered().iter().any(|n| matches!(n.effect, Effect::Compressor(_)))))
                .unwrap_or(false)
        });
        Button::new(cx, |cx| Label::new(cx, "Compressor"))
            .class("btn")
            .toggle_class("is-on", Memo::new(move |_| matches!(panel.get(), Panel::Compressor(_))))
            .toggle_class("hidden", has_compressor.map(|c| !*c))
            .on_press(move |cx| {
                p.viewing_effect.set(true);
                cx.emit(PianoRollEvent::Close);
            });
        Button::new(cx, |cx| Label::new(cx, "\u{2715}"))
            .class("btn")
            .class("quiet")
            .toggle_class("hidden", has_compressor.map(|c| !*c))
            .on_press(move |cx| {
                if let Some(track) = p.selected_track.get() {
                    cx.emit(TimelineEvent::RemoveCompressorEffect(track));
                }
            });
        let can_add_compressor = Memo::new(move |_| {
            p.selected_track.get().is_some_and(|id| !has_compressor.get() && p.arrangement.get().track(id).is_some())
        });
        Button::new(cx, |cx| Label::new(cx, "+ Compressor"))
            .class("btn")
            .class("quiet")
            .toggle_class("hidden", can_add_compressor.map(|n| !*n))
            .on_press(move |cx| {
                if let Some(track) = p.selected_track.get() {
                    cx.emit(TimelineEvent::AddCompressorEffect(track));
                    p.viewing_effect.set(true);
                }
            });

        Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
        Button::new(cx, |cx| Label::new(cx, "Show input"))
            .class("btn")
            .class("quiet")
            .toggle_class("is-on", p.interval_open)
            .on_press(|cx| cx.emit(IntervalInputEvent::ToggleOpen));
    })
    .gap(Pixels(tokens::SPACE_2))
    .padding_left(Pixels(tokens::SPACE_1))
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Pixels(tokens::SIZE_CONTROL + 6.0));
}

/// A quiet, short panel standing in for a device the track doesn't have.
fn empty_state(cx: &mut Context, message: &'static str, action: impl FnOnce(&mut Context)) {
    HStack::new(cx, move |cx| {
        Label::new(cx, message).class("body");
        action(cx);
    })
    .class("device")
    .gap(Pixels(tokens::SPACE_3))
    .alignment(Alignment::Center)
    .width(Stretch(1.0))
    .height(Pixels(96.0));
}
