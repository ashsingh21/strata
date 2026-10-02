//! The lower panel's contents: the selected track's device chain, then
//! whatever that track shows - its open clip's editor, its Carve, or (for
//! a MIDI track with no instrument, an audio track, or no selection) a
//! short empty state - with Theory (the Interval Input views) docked underneath.

use std::collections::HashSet;

use vizia::prelude::*;
use crate::lessons::LessonTargetExt;

use shared::arrangement::{Arrangement, ClipId, Effect, EffectNodeId, Instrument, SnapGrid, Ticks, TrackId, TrackKind};
use shared::synth::SynthState;

use crate::interval_input;
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
    Drums(TrackId),
    Effect(TrackId, EffectNodeId),
    NoInstrument(TrackId),
    Audio,
    Nothing,
}

#[derive(Clone, Copy)]
pub struct DeviceAreaProps {
    pub theme: Signal<ThemeId>,
    pub arrangement: Signal<Arrangement>,
    pub selected_track: Signal<Option<TrackId>>,
    /// Which effect node the panel shows instead of the instrument/empty
    /// state, if any - toggled by an effect's own chip (or, generically,
    /// the `TrackHeaderFx` control, which just picks the first one).
    pub viewing_effect: Signal<Option<EffectNodeId>>,
    // Carve.
    pub synth_state: Signal<SynthState>,
    pub lfo_phases: (Signal<f32>, Signal<f32>),
    pub octave_shift: Signal<i8>,
    pub meter_l: Signal<f32>,
    pub meter_r: Signal<f32>,
    pub help_open: Signal<bool>,
    pub lfo_drag: Signal<Option<usize>>,
    pub user_presets: Signal<Vec<&'static str>>,
    // Piano roll.
    pub open_clip: Signal<Option<ClipId>>,
    pub edit_mode: Signal<EditMode>,
    pub label_mode: Signal<LabelMode>,
    pub selected_notes: Signal<HashSet<NoteKey>>,
    pub rows_octave: Signal<i32>,
    pub draw_chord: Signal<crate::piano_roll::state::ChordShape>,
    pub snap: Signal<SnapGrid>,
    pub playhead: Signal<Ticks>,
    // Interval Input.
    pub key: Signal<u8>,
    pub scale_mask: Signal<u16>,
    pub interval_open: Signal<bool>,
    pub show_note_names: Signal<bool>,
    /// Voice leading, docked under Theory.
    pub voicing: crate::voicing::VoicingProps,
}

pub fn device_area(cx: &mut Context, p: DeviceAreaProps) {
    let panel = Memo::new(move |_| {
        let arr = p.arrangement.get();
        match p.selected_track.get().and_then(|id| arr.track(id).cloned()) {
            Some(t) if p.viewing_effect.get().is_some_and(|id| t.fx.node(id).is_some()) => {
                Panel::Effect(t.id, p.viewing_effect.get().unwrap())
            }
            Some(t) if t.kind == TrackKind::Audio => Panel::Audio,
            Some(t) if t.instrument == Some(Instrument::Drums) => Panel::Drums(t.id),
            Some(t) if t.instrument.is_some() => Panel::Carve,
            Some(t) => Panel::NoInstrument(t.id),
            None => Panel::Nothing,
        }
    });

    device_chain(cx, p, panel);

    // An effect added to the selected track opens its panel, however it
    // got there (+ EQ, the Effects Board, a lesson's Show me, redo) -
    // otherwise its controls stayed hidden behind the instrument until its
    // chip was clicked.
    let effect_ids = Memo::new(move |_| {
        let arr = p.arrangement.get();
        p.selected_track.get().and_then(|t| arr.track(t).map(|tr| (t, tr.fx.nodes.iter().map(|n| n.id).collect::<Vec<_>>())))
    });
    let seen: Signal<Option<(TrackId, Vec<EffectNodeId>)>> = Signal::new(None);
    Binding::new(cx, effect_ids, move |_| {
        let now = effect_ids.get();
        if let (Some((track, ids)), Some((seen_track, seen_ids))) = (&now, &seen.get()) {
            if track == seen_track {
                if let Some(new) = ids.iter().find(|id| !seen_ids.contains(id)) {
                    p.viewing_effect.set(Some(*new));
                }
            }
        }
        seen.set(now);
    });

    // The editor replaces the device while a clip is open - and only while
    // that clip still exists (after an undo removed it, a blank editor
    // stayed up whose controls did nothing).
    // Selecting another track shows that track's device instead (the clip
    // stays open, and comes back with its track).
    let open_existing = Memo::new(move |_| {
        let arr = p.arrangement.get();
        p.open_clip.get().filter(|id| arr.clip(*id).is_some_and(|c| p.selected_track.get().is_none_or(|t| t == c.track)))
    });
    Binding::new(cx, open_existing, move |cx| {
        if open_existing.get().is_some() {
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
                p.rows_octave,
                p.draw_chord,
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
                    p.arrangement,
                    p.selected_track,
                    p.playhead,
                    p.lfo_phases,
                    p.octave_shift,
                    p.meter_l,
                    p.meter_r,
                    p.help_open,
                    p.lfo_drag,
                    p.user_presets,
                    color,
                );
            }),
            Panel::Effect(track, node) => {
                let arr = p.arrangement.get();
                let color = arr.track(track).map(|t| t.color).unwrap_or(shared::arrangement::ClipColor::Violet);
                let kind = arr.track(track).and_then(|t| t.fx.node(node)).map(|n| n.effect);
                if kind.is_some() {
                    crate::effect_panel::effect_panel(cx, p.theme, p.arrangement, Some(track), node, color, p.playhead);
                }
            }
            Panel::Drums(track) => {
                let color = p.arrangement.get().track(track).map(|t| t.color).unwrap_or(shared::arrangement::ClipColor::Coral);
                crate::drum_kit_panel::drum_kit_panel(cx, color, track, p.arrangement, p.theme);
            }
            Panel::NoInstrument(track) => empty_state(cx, "No instrument on this track", move |cx| {
                Button::new(cx, |cx| Label::new(cx, "Add Carve"))
                    .class("btn")
                    .on_press(move |cx| {
                        cx.emit(TimelineEvent::SetInstrument { track, instrument: Some(Instrument::Carve) })
                    });
                Button::new(cx, |cx| Label::new(cx, "Add Drum Kit"))
                    .class("btn")
                    .on_press(move |cx| {
                        cx.emit(TimelineEvent::SetInstrument { track, instrument: Some(Instrument::Drums) })
                    });
            }),
            Panel::Audio => empty_state(cx, "An audio track plays its clips; add effects with the buttons above", |_| {}),
            Panel::Nothing => empty_state(cx, "Select a track to see its instrument and effects", |_| {}),
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
    crate::voicing::voicing_view(cx, p.voicing);
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
        // Hidden with the swatch/name when no track is selected - on its
        // own it was a stray line at the strip's left edge.
        Element::new(cx)
            .class("hairline")
            .toggle_class("hidden", track.map(|t| t.is_none()))
            .width(Pixels(1.0))
            .height(Pixels(16.0));

        let editing = Memo::new(move |_| {
            let arr = p.arrangement.get();
            p.open_clip.get().is_some_and(|id| arr.clip(id).is_some_and(|c| p.selected_track.get().is_none_or(|t| t == c.track)))
        });
        let clip_name = Memo::new(move |_| {
            p.open_clip.get().and_then(|id| p.arrangement.get().clip(id).map(|c| c.name.clone())).unwrap_or_default()
        });
        Button::new(cx, move |cx| Label::new(cx, clip_name))
            .on_press(move |cx| {
                use crate::hidpi::Logical;
                let b = cx.lbounds();
                if let Some(clip) = p.open_clip.get() {
                    cx.emit(crate::timeline::state::TimelineEvent::BeginRenameClip { clip, x: b.x, y: b.y });
                }
            })
            .tooltip(|cx| Tooltip::new(cx, |cx| { Label::new(cx, "Rename this clip"); }).arrow(false))
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
        let instrument_name = Memo::new(move |_| {
            p.selected_track
                .get()
                .and_then(|id| p.arrangement.get().track(id).and_then(|t| t.instrument))
                .map(|i| i.name())
                .unwrap_or("Carve")
        });
        Button::new(cx, move |cx| Label::new(cx, instrument_name))
            .class("btn")
            .toggle_class(
                "is-on",
                Memo::new(move |_| matches!(panel.get(), Panel::Carve | Panel::Drums(_)) && !editing.get()),
            )
            .toggle_class("hidden", has_instrument.map(|c| !*c))
            .on_press(move |cx| {
                p.viewing_effect.set(None);
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
            .on_press(|cx| cx.emit(SynthEvent::AddInstrumentToSelected(Instrument::Carve)));

        let has_compressor = Memo::new(move |_| {
            p.selected_track
                .get()
                .and_then(|id| p.arrangement.get().track(id).map(|t| t.fx.ordered().iter().any(|n| matches!(n.effect, Effect::Compressor(_)))))
                .unwrap_or(false)
        });
        Button::new(cx, |cx| Label::new(cx, "Compressor"))
            .class("btn")
            .toggle_class(
                "is-on",
                Memo::new(move |_| {
                    matches!(panel.get(), Panel::Effect(t, n) if p.arrangement.get().track(t).and_then(|t| t.fx.node(n)).is_some_and(|node| matches!(node.effect, Effect::Compressor(_))))
                }),
            )
            .toggle_class("hidden", has_compressor.map(|c| !*c))
            .on_press(move |cx| {
                let node = p.selected_track.get().and_then(|id| {
                    p.arrangement.get().track(id).and_then(|t| {
                        t.fx.ordered().iter().find(|n| matches!(n.effect, Effect::Compressor(_))).map(|n| n.id)
                    })
                });
                p.viewing_effect.set(node);
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
            .lesson_target(crate::lessons::Target::AddCompressor)
            .class("btn")
            .class("quiet")
            .toggle_class("hidden", can_add_compressor.map(|n| !*n))
            .on_press(move |cx| {
                if let Some(track) = p.selected_track.get() {
                    cx.emit(TimelineEvent::AddCompressorEffect(track));
                }
            });

        let has_eq = Memo::new(move |_| {
            p.selected_track
                .get()
                .and_then(|id| p.arrangement.get().track(id).map(|t| t.fx.ordered().iter().any(|n| matches!(n.effect, Effect::Eq(_)))))
                .unwrap_or(false)
        });
        Button::new(cx, |cx| Label::new(cx, "EQ"))
            .class("btn")
            .toggle_class(
                "is-on",
                Memo::new(move |_| {
                    matches!(panel.get(), Panel::Effect(t, n) if p.arrangement.get().track(t).and_then(|t| t.fx.node(n)).is_some_and(|node| matches!(node.effect, Effect::Eq(_))))
                }),
            )
            .toggle_class("hidden", has_eq.map(|c| !*c))
            .on_press(move |cx| {
                let node = p.selected_track.get().and_then(|id| {
                    p.arrangement.get().track(id).and_then(|t| t.fx.ordered().iter().find(|n| matches!(n.effect, Effect::Eq(_))).map(|n| n.id))
                });
                p.viewing_effect.set(node);
                cx.emit(PianoRollEvent::Close);
            });
        Button::new(cx, |cx| Label::new(cx, "\u{2715}"))
            .class("btn")
            .class("quiet")
            .toggle_class("hidden", has_eq.map(|c| !*c))
            .on_press(move |cx| {
                if let Some(track) = p.selected_track.get() {
                    cx.emit(TimelineEvent::RemoveEqEffect(track));
                }
            });
        let can_add_eq = Memo::new(move |_| {
            p.selected_track.get().is_some_and(|id| !has_eq.get() && p.arrangement.get().track(id).is_some())
        });
        Button::new(cx, |cx| Label::new(cx, "+ EQ"))
            .lesson_target(crate::lessons::Target::AddEq)
            .class("btn")
            .class("quiet")
            .toggle_class("hidden", can_add_eq.map(|n| !*n))
            .on_press(move |cx| {
                if let Some(track) = p.selected_track.get() {
                    cx.emit(TimelineEvent::AddEqEffect(track));
                }
            });

        // One chip per guitar effect, in chain order; rebuilt when the
        // set changes. They're added from the browser or the Effects Board.
        let guitar_nodes = Memo::new(move |_| {
            p.selected_track
                .get()
                .and_then(|id| p.arrangement.get().track(id).cloned())
                .map(|t| {
                    t.fx.ordered()
                        .iter()
                        .filter_map(|n| match n.effect {
                            Effect::Guitar(g) => Some((n.id, g.kind.name())),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        });
        Binding::new(cx, guitar_nodes, move |cx| {
            for (node, name) in guitar_nodes.get() {
                let showing = Memo::new(move |_| matches!(panel.get(), Panel::Effect(_, n) if n == node));
                Button::new(cx, move |cx| Label::new(cx, name))
                    .class("btn")
                    .toggle_class("is-on", showing)
                    .on_press(move |cx| {
                        p.viewing_effect.set(Some(node));
                        cx.emit(PianoRollEvent::Close);
                    });
                Button::new(cx, |cx| Label::new(cx, "\u{2715}")).class("btn").class("quiet").on_press(move |cx| {
                    if let Some(track) = p.selected_track.get() {
                        if p.viewing_effect.get() == Some(node) {
                            p.viewing_effect.set(None);
                        }
                        cx.emit(TimelineEvent::RemoveEffectNodeFromBoard(Some(track), node));
                    }
                });
            }
        });

        Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
    })
    .gap(Pixels(tokens::SPACE_2))
    .padding_left(Pixels(tokens::SPACE_3))
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Pixels(tokens::SIZE_CONTROL + 6.0));
}

/// A quiet, short panel standing in for a device the track doesn't have.
fn empty_state(cx: &mut Context, message: &'static str, action: impl FnOnce(&mut Context)) {
    HStack::new(cx, move |cx| {
        Label::new(cx, message).class("body").class("empty-note");
        action(cx);
    })
    // No frame: an empty box read as a panel that had failed to load.
    .gap(Pixels(tokens::SPACE_3))
    .alignment(Alignment::Center)
    .width(Stretch(1.0))
    .height(Pixels(120.0));
}
