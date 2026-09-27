//! Carve: the subtractive synth device panel. Every control here reads/
//! writes `SynthState`, which is mirrored to the engine's real-time voice
//! renderer (see `engine::synth` and `shared::synth::bridge`) - so turning
//! a knob or playing a note here makes real sound.

pub mod display;
pub mod help;
pub mod keyboard;
pub mod segmented;
pub mod state;

use vizia::prelude::*;

use std::cell::Cell;

use shared::synth::{lfo_mod_depth, FilterType, LfoTarget, SynthState, VoiceMode, Waveform, MAX_UNISON};

use crate::glyph::{ink_when_on, Glyph, GlyphKind};
use crate::knob::{Knob, KnobAccentExt};
use crate::status::StatusEvent;
use crate::pill::modulator_pill;
use crate::tokens::{self, ThemeId};
use shared::arrangement::ClipColor;
use display::{EnvelopeDisplay, FilterDisplay, LfoScope, WaveDisplay};
use keyboard::Keyboard;
use segmented::segmented;
use state::{lin, lin_inv, log, log_inv, SynthEvent};

fn update(f: impl Fn(&mut SynthState) + Send + 'static) -> SynthEvent {
    SynthEvent::Update(Box::new(f))
}

/// Carve's track colour: the violet swatch in its header, and (as its
/// `-line` variant) every knob's value arc - one hue per device.
fn carve_accent(p: &tokens::Palette) -> Color {
    match TRACK_COLOR.get() {
        ClipColor::Coral => p.clip_coral_line,
        ClipColor::Amber => p.clip_amber_line,
        ClipColor::Teal => p.clip_teal_line,
        ClipColor::Blue => p.clip_blue_line,
        ClipColor::Violet => p.clip_violet_line,
        ClipColor::Pink => p.clip_pink_line,
    }
}

thread_local! {
    /// The colour of the track whose Carve is on screen: its value arcs
    /// and header swatch take it (one hue per device), so it's obvious
    /// whose instrument you're editing. Set by `synth_view`.
    static TRACK_COLOR: Cell<ClipColor> = const { Cell::new(ClipColor::Violet) };
}

/// Strata's voice uses a true minus sign in values ("−6.2 dB").
fn true_minus(text: String) -> String {
    text.replace('-', "\u{2212}")
}

/// The three knob sizes: `Lg` for the control the device is played with
/// (Cutoff), `Md` for main controls, `Sm` for trims.
#[derive(Clone, Copy, PartialEq)]
enum KnobSize {
    Sm,
    Md,
    Lg,
}

impl KnobSize {
    fn px(self) -> f32 {
        match self {
            KnobSize::Sm => tokens::SIZE_KNOB_SM,
            KnobSize::Md => tokens::SIZE_KNOB,
            KnobSize::Lg => tokens::SIZE_KNOB_LG,
        }
    }
}

thread_local! {
    /// Which LFO pill (0 or 1) is being dragged, if any - set by
    /// `synth_view` from `SynthModel::lfo_drag` so every routable knob can
    /// light up as a drop target and accept the drop, without threading one
    /// more argument through every section builder.
    static LFO_DRAG: Cell<Option<Signal<Option<usize>>>> = const { Cell::new(None) };
}

/// A knob bound to a physical value via a `to_pos`/`apply` mapping pair
/// (see `state::{lin, log}` and their inverses).
///
/// `route` makes it an LFO destination: it draws a `mod` ring for however
/// far the LFOs patched to that target swing it, lights up while an LFO
/// pill is dragged, and routes the pill's LFO to it when dropped on.
///
/// The knob sits at the bottom of a `slot`-tall box so every knob in a
/// row - whatever its size - lines up on its label.
#[allow(clippy::too_many_arguments)]
fn knob(
    cx: &mut Context,
    state: Signal<SynthState>,
    theme: Signal<ThemeId>,
    size: KnobSize,
    slot: f32,
    label: &'static str,
    default_pos: f32,
    to_pos: impl Fn(&SynthState) -> f32 + Copy + 'static,
    format: impl Fn(&SynthState) -> String + Copy + 'static,
    apply: impl Fn(&mut SynthState, f32) + Copy + Send + 'static,
    route: Option<LfoTarget>,
) {
    let pos = state.map(move |s| to_pos(s));
    let text = state.map(move |s| true_minus(format(s)));
    let column = VStack::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            let on_change = move |cx: &mut EventContext, p: f32| {
                cx.emit(update(move |s| apply(s, p)));
                cx.emit(StatusEvent::Touched { name: format!("Carve \u{b7} {label}"), value: text });
            };
            match route {
                Some(target) => {
                    let centre = state.map(move |s| to_pos(s));
                    let depth = state.map(move |s| lfo_mod_depth(s, target));
                    Knob::new(cx, pos, default_pos, theme, Some((centre, depth)), on_change)
                        .accent(carve_accent)
                        .size(Pixels(size.px()));
                }
                None => {
                    Knob::plain(cx, pos, default_pos, theme, on_change).accent(carve_accent).size(Pixels(size.px()));
                }
            }
        })
        .alignment(Alignment::BottomCenter)
        .width(Auto)
        .height(Pixels(slot));
        Label::new(cx, label).class(if size == KnobSize::Lg { "label-lg" } else { "label" });
        Label::new(cx, text).class("value");
    })
    .class("knob-col")
    .alignment(Alignment::Center)
    .gap(Pixels(2.0))
    .padding(Pixels(2.0))
    .min_width(Pixels(44.0))
    .width(Auto)
    .height(Auto);

    if let (Some(target), Some(drag)) = (route, LFO_DRAG.get()) {
        column
            .toggle_class("drop-target", drag.map(|d| d.is_some()))
            .on_mouse_up(move |cx, button| {
                if button == MouseButton::Left {
                    if let Some(lfo) = drag.get() {
                        cx.emit(SynthEvent::RouteLfo(lfo, target));
                    }
                }
            });
    }
}

/// The `line` hairline that separates device sections (vertical) and rows
/// (horizontal) - grouping by lines, never by boxes. Explicit elements
/// rather than one-sided CSS borders, which Vizia doesn't draw.
fn vrule(cx: &mut Context) {
    Element::new(cx).class("hairline").width(Pixels(1.0)).height(Stretch(1.0));
}

fn hrule(cx: &mut Context) {
    Element::new(cx).class("hairline").width(Stretch(1.0)).height(Pixels(1.0));
}

/// A device section: its `title` on the left and its mode controls
/// (waveform, Sync, filter type, modulators) right-aligned in the same
/// header, then its content.
fn section<'a>(
    cx: &'a mut Context,
    title: &'static str,
    header_right: impl FnOnce(&mut Context),
    content: impl FnOnce(&mut Context),
) -> Handle<'a, VStack> {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Label::new(cx, title).class("title");
            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
            header_right(cx);
        })
        .gap(Pixels(tokens::SPACE_2))
        .alignment(Alignment::Left)
        .width(Stretch(1.0))
        .height(Pixels(tokens::SIZE_CONTROL));

        content(cx);
    })
    .class("synth-sec")
    .gap(Pixels(tokens::SPACE_3))
    .height(Stretch(1.0))
}

/// A row of knobs spread evenly across the section's width.
fn knob_row(cx: &mut Context, content: impl FnOnce(&mut Context)) {
    HStack::new(cx, content).gap(Stretch(1.0)).width(Stretch(1.0)).height(Auto);
}

fn waveform_seg(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>, is_osc1: bool) {
    let waves = [Waveform::Sine, Waveform::Triangle, Waveform::Saw, Waveform::Square];
    let icons = [GlyphKind::Sine, GlyphKind::Triangle, GlyphKind::Saw, GlyphKind::Square];
    let selected = move |i: usize| {
        state.map(move |s| {
            let current = if is_osc1 { s.osc1.waveform } else { s.osc2.waveform };
            current == waves[i]
        })
    };
    segmented(
        cx,
        4,
        move |cx, i| Glyph::new(cx, icons[i], selected(i), theme, ink_when_on),
        selected,
        move |cx, i| {
            cx.emit(if is_osc1 {
                SynthEvent::SetOsc1Waveform(waves[i])
            } else {
                SynthEvent::SetOsc2Waveform(waves[i])
            });
        },
    );
}

const OCTAVE_RANGE: f32 = 3.0;

fn octave_pos(octave: i8) -> f32 {
    lin_inv(octave as f32, -OCTAVE_RANGE, OCTAVE_RANGE)
}

fn octave_from_pos(p: f32) -> i8 {
    lin(p, -OCTAVE_RANGE, OCTAVE_RANGE).round() as i8
}

const OSC_WIDTH: f32 = 256.0;

// Fixed row heights (not `Auto`): sections stretch to fill their row so
// the hairline between them runs its full height, and a stretch child
// inside an auto-sized parent resolves to nothing. Each is the tallest
// section's content - header, display, knob slot, label and value - plus
// padding and a little slack.
const ROW1_HEIGHT: f32 = 252.0;
const ROW2_HEIGHT: f32 = 218.0;
const ROW_SLOT: f32 = tokens::SIZE_KNOB;

fn osc1_section(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Oscillator 1", move |cx| waveform_seg(cx, state, theme, true), move |cx| {
        WaveDisplay::new(cx, state, theme, |s| s.osc1).width(Stretch(1.0)).height(Pixels(44.0)).class("synth-disp");
        knob_row(cx, move |cx| {
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, "Octave", octave_pos(-1),
                |s| octave_pos(s.osc1.octave),
                |s| format!("{:+}", s.osc1.octave),
                |s, p| s.osc1.octave = octave_from_pos(p), None);
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, "Tune", lin_inv(0.0, -100.0, 100.0),
                |s| lin_inv(s.osc1.knob_a_cents, -100.0, 100.0),
                |s| format!("{:.0} ct", s.osc1.knob_a_cents),
                |s, p| s.osc1.knob_a_cents = lin(p, -100.0, 100.0), Some(LfoTarget::Pitch));
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, "Shape", 0.35,
                |s| s.osc1.knob_b,
                |s| format!("{:.0}%", s.osc1.knob_b * 100.0),
                |s, p| s.osc1.knob_b = p, None);
            knob(cx, state, theme, KnobSize::Sm, ROW_SLOT, "Drift", 0.12,
                |s| s.osc1.knob_c,
                |s| format!("{:.0}%", s.osc1.knob_c * 100.0),
                |s, p| s.osc1.knob_c = p, None);
        });
    })
    .width(Pixels(OSC_WIDTH));
}

fn osc2_section(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>) {
    section(
        cx,
        "Oscillator 2",
        move |cx| {
            let sync = state.map(|s| s.osc2.sync);
            Button::new(cx, |cx| Label::new(cx, "Sync"))
                .class("btn")
                .class("sm")
                .toggle_class("is-on", sync)
                .on_press(|cx| cx.emit(SynthEvent::ToggleOsc2Sync));
            waveform_seg(cx, state, theme, false);
        },
        move |cx| {
            WaveDisplay::new(cx, state, theme, |s| s.osc2).width(Stretch(1.0)).height(Pixels(44.0)).class("synth-disp");
            knob_row(cx, move |cx| {
                knob(cx, state, theme, KnobSize::Sm, ROW_SLOT, "Octave", octave_pos(0),
                    |s| octave_pos(s.osc2.octave),
                    |s| format!("{:+}", s.osc2.octave),
                    |s, p| s.osc2.octave = octave_from_pos(p), None);
                knob(cx, state, theme, KnobSize::Md, ROW_SLOT, "Detune", lin_inv(0.0, -50.0, 50.0),
                    |s| lin_inv(s.osc2.knob_a_cents, -50.0, 50.0),
                    |s| format!("{:+.0} ct", s.osc2.knob_a_cents),
                    |s, p| s.osc2.knob_a_cents = lin(p, -50.0, 50.0), None);
                knob(cx, state, theme, KnobSize::Md, ROW_SLOT, "Pulse width", 0.38,
                    |s| s.osc2.knob_b,
                    |s| format!("{:.0}%", s.osc2.knob_b * 100.0),
                    |s, p| s.osc2.knob_b = p, Some(LfoTarget::PulseWidth));
                knob(cx, state, theme, KnobSize::Md, ROW_SLOT, "FM", 0.0,
                    |s| s.osc2.knob_c,
                    |s| format!("{:.0}%", s.osc2.knob_c * 100.0),
                    |s, p| s.osc2.knob_c = p, None);
            });
        },
    )
    .width(Pixels(OSC_WIDTH + 44.0));
}

fn mix_section(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Mixer", |_| {}, move |cx| {
        let db_knob = |cx: &mut Context, label: &'static str, default_db: f32, get: fn(&SynthState) -> f32, set: fn(&mut SynthState, f32)| {
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, label, lin_inv(default_db, -60.0, 0.0),
                move |s| lin_inv(get(s), -60.0, 0.0),
                move |s| format!("{:.1} dB", get(s)),
                move |s, p| set(s, lin(p, -60.0, 0.0)), None);
        };
        HStack::new(cx, move |cx| {
            db_knob(cx, "Osc 1", -1.9, |s| s.mix.osc1_db, |s, v| s.mix.osc1_db = v);
            db_knob(cx, "Osc 2", -5.2, |s| s.mix.osc2_db, |s, v| s.mix.osc2_db = v);
        })
        .gap(Pixels(tokens::SPACE_3))
        .size(Auto);
        HStack::new(cx, move |cx| {
            db_knob(cx, "Sub", -10.0, |s| s.mix.sub_db, |s, v| s.mix.sub_db = v);
            db_knob(cx, "Noise", -28.0, |s| s.mix.noise_db, |s, v| s.mix.noise_db = v);
        })
        .gap(Pixels(tokens::SPACE_3))
        .size(Auto);
    })
    .width(Auto);
}

fn filter_section(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>) {
    section(
        cx,
        "Filter",
        move |cx| {
            let types = [FilterType::Lp24, FilterType::Lp12, FilterType::Bp, FilterType::Hp];
            let labels = ["LP 24", "LP 12", "BP", "HP"];
            segmented(
                cx,
                4,
                move |cx, i| Label::new(cx, labels[i]),
                move |i| state.map(move |s| s.filter.filter_type == types[i]),
                move |cx, i| cx.emit(SynthEvent::SetFilterType(types[i])),
            );
        },
        move |cx| {
            FilterDisplay::new(cx, state, theme).width(Stretch(1.0)).height(Pixels(88.0)).class("synth-disp");
            let slot = tokens::SIZE_KNOB_LG;
            knob_row(cx, move |cx| {
                knob(cx, state, theme, KnobSize::Lg, slot, "Cutoff", log_inv(1200.0, 20.0, 20_000.0),
                    |s| log_inv(s.filter.cutoff_hz, 20.0, 20_000.0),
                    |s| format_hz(s.filter.cutoff_hz),
                    |s, p| s.filter.cutoff_hz = log(p, 20.0, 20_000.0),
                    Some(LfoTarget::Cutoff));
                knob(cx, state, theme, KnobSize::Md, slot, "Resonance", 0.62,
                    |s| s.filter.resonance,
                    |s| format!("{:.0}%", s.filter.resonance * 100.0),
                    |s, p| s.filter.resonance = p, Some(LfoTarget::Resonance));
                knob(cx, state, theme, KnobSize::Md, slot, "Drive", lin_inv(4.5, 0.0, 24.0),
                    |s| lin_inv(s.filter.drive_db, 0.0, 24.0),
                    |s| format!("{:+.1} dB", s.filter.drive_db),
                    |s, p| s.filter.drive_db = lin(p, 0.0, 24.0), None);
                knob(cx, state, theme, KnobSize::Md, slot, "Env amount", lin_inv(2.4, -4.0, 4.0),
                    |s| lin_inv(s.filter.env_amount_oct, -4.0, 4.0),
                    |s| format!("{:+.1} oct", s.filter.env_amount_oct),
                    |s, p| s.filter.env_amount_oct = lin(p, -4.0, 4.0), None);
                knob(cx, state, theme, KnobSize::Sm, slot, "Key track", 0.5,
                    |s| s.filter.key_track,
                    |s| format!("{:.0}%", s.filter.key_track * 100.0),
                    |s, p| s.filter.key_track = p, None);
            });
        },
    )
    .width(Stretch(1.0));
}

fn format_hz(hz: f32) -> String {
    if hz >= 1000.0 {
        format!("{:.2} kHz", hz / 1000.0)
    } else {
        format!("{hz:.0} Hz")
    }
}

#[allow(clippy::too_many_arguments)]
fn adsr_knobs(
    cx: &mut Context,
    state: Signal<SynthState>,
    theme: Signal<ThemeId>,
    get: fn(&SynthState) -> shared::synth::Envelope,
    set_a: fn(&mut SynthState, f32),
    set_d: fn(&mut SynthState, f32),
    set_s: fn(&mut SynthState, f32),
    set_r: fn(&mut SynthState, f32),
) {
    knob_row(cx, move |cx| {
        knob(cx, state, theme, KnobSize::Md, ROW_SLOT, "Attack", log_inv(get(&state.get()).attack_ms, 1.0, 2000.0),
            move |s| log_inv(get(s).attack_ms, 1.0, 2000.0),
            move |s| format!("{:.0} ms", get(s).attack_ms),
            move |s, p| set_a(s, log(p, 1.0, 2000.0)), None);
        knob(cx, state, theme, KnobSize::Md, ROW_SLOT, "Decay", log_inv(get(&state.get()).decay_ms, 1.0, 2000.0),
            move |s| log_inv(get(s).decay_ms, 1.0, 2000.0),
            move |s| format!("{:.0} ms", get(s).decay_ms),
            move |s, p| set_d(s, log(p, 1.0, 2000.0)), None);
        knob(cx, state, theme, KnobSize::Md, ROW_SLOT, "Sustain", get(&state.get()).sustain,
            move |s| get(s).sustain,
            move |s| format!("{:.0}%", get(s).sustain * 100.0),
            set_s, None);
        knob(cx, state, theme, KnobSize::Md, ROW_SLOT, "Release", log_inv(get(&state.get()).release_ms, 1.0, 2000.0),
            move |s| log_inv(get(s).release_ms, 1.0, 2000.0),
            move |s| format!("{:.0} ms", get(s).release_ms),
            move |s, p| set_r(s, log(p, 1.0, 2000.0)), None);
    });
}

fn filter_env_section(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Filter envelope", |_| {}, move |cx| {
        EnvelopeDisplay::new(cx, state, theme, |s| s.filter_env)
            .width(Stretch(1.0))
            .height(Pixels(44.0))
            .class("synth-disp");
        adsr_knobs(
            cx, state, theme,
            |s| s.filter_env,
            |s, v| s.filter_env.attack_ms = v,
            |s, v| s.filter_env.decay_ms = v,
            |s, v| s.filter_env.sustain = v,
            |s, v| s.filter_env.release_ms = v,
        );
    })
    .width(Stretch(1.0));
}

fn amp_env_section(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Amp envelope", |_| {}, move |cx| {
        EnvelopeDisplay::new(cx, state, theme, |s| s.amp_env)
            .width(Stretch(1.0))
            .height(Pixels(44.0))
            .class("synth-disp");
        adsr_knobs(
            cx, state, theme,
            |s| s.amp_env,
            |s, v| s.amp_env.attack_ms = v,
            |s, v| s.amp_env.decay_ms = v,
            |s, v| s.amp_env.sustain = v,
            |s, v| s.amp_env.release_ms = v,
        );
    })
    .width(Stretch(1.0));
}

/// One LFO's column: Rate and Depth, then its Sync toggle and target.
/// `lfo`/`lfo_mut` pick LFO 1 or 2 out of the state.
fn lfo_column(
    cx: &mut Context,
    state: Signal<SynthState>,
    theme: Signal<ThemeId>,
    lfo: fn(&SynthState) -> &shared::synth::Lfo,
    lfo_mut: fn(&mut SynthState) -> &mut shared::synth::Lfo,
    set_target: fn(LfoTarget) -> SynthEvent,
    phase: Signal<f32>,
) {
    VStack::new(cx, move |cx| {
        LfoScope::new(cx, state, theme, phase, lfo).width(Stretch(1.0)).height(Pixels(40.0)).class("synth-disp");
        HStack::new(cx, move |cx| {
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, "Rate", lfo(&state.get()).rate_norm,
                move |s| lfo(s).rate_norm,
                move |s| format!("{:.2} Hz", shared::synth::lfo_rate_hz(lfo(s).rate_norm)),
                move |s, p| lfo_mut(s).rate_norm = p, None);
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, "Depth", lfo(&state.get()).depth,
                move |s| lfo(s).depth,
                move |s| format!("{:.0}%", lfo(s).depth * 100.0),
                move |s, p| lfo_mut(s).depth = p, None);
        })
        .gap(Pixels(tokens::SPACE_1))
        .size(Auto);
        HStack::new(cx, move |cx| {
            // No tempo-sync button: the engine only ever runs the LFO at
            // the Rate shown in Hz, so a Sync toggle here did nothing.
            let target_text = state.map(move |s| lfo(s).target.name());
            Button::new(cx, |cx| Label::new(cx, target_text))
                .class("btn")
                .class("sm")
                .on_press(move |cx| cx.emit(set_target(lfo(&state.get()).target.next())));
        })
        .gap(Pixels(tokens::SPACE_1))
        .alignment(Alignment::Center)
        .size(Auto);
    })
    .gap(Pixels(tokens::SPACE_2))
    .alignment(Alignment::TopCenter)
    .size(Auto);
}

fn mod_section(
    cx: &mut Context,
    state: Signal<SynthState>,
    theme: Signal<ThemeId>,
    lfo_phases: (Signal<f32>, Signal<f32>),
) {
    section(
        cx,
        "Modulation",
        move |cx| {
            // Press a pill and release over any knob with a ring (Cutoff,
            // Resonance, Tune, Pulse width) to route that LFO there.
            for (i, name) in ["LFO 1", "LFO 2"].into_iter().enumerate() {
                HStack::new(cx, move |cx| modulator_pill(cx, theme, name, 1))
                    .size(Auto)
                    .cursor(CursorIcon::Grab)
                    .on_mouse_down(move |cx, button| {
                        if button == MouseButton::Left {
                            cx.emit(SynthEvent::BeginLfoDrag(i));
                        }
                    });
            }
        },
        move |cx| {
            // Each LFO is its own column - scope, knobs, then its Sync and
            // target - lined up under its own pill in the header.
            HStack::new(cx, move |cx| {
                lfo_column(cx, state, theme, |s| &s.lfo1, |s| &mut s.lfo1,
                    SynthEvent::SetLfo1Target, lfo_phases.0);
                Element::new(cx).class("hairline").width(Pixels(1.0)).height(Stretch(1.0));
                lfo_column(cx, state, theme, |s| &s.lfo2, |s| &mut s.lfo2,
                    SynthEvent::SetLfo2Target, lfo_phases.1);
            })
            .gap(Stretch(1.0))
            .width(Stretch(1.0))
            .height(Auto);
        },
    )
    .width(Pixels(300.0));
}

fn unison_section(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Unison", |_| {}, move |cx| {
        let max = MAX_UNISON as f32;
        HStack::new(cx, move |cx| {
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, "Voices", 0.0,
                move |s| lin_inv(s.unison.voices as f32, 1.0, max),
                |s| if s.unison.voices <= 1 { "Off".to_string() } else { format!("{}", s.unison.voices) },
                move |s, p| s.unison.voices = lin(p, 1.0, max).round() as u8, None);
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, "Detune", lin_inv(14.0, 0.0, 50.0),
                |s| lin_inv(s.unison.detune_cents, 0.0, 50.0),
                |s| format!("{:.0} ct", s.unison.detune_cents),
                |s, p| s.unison.detune_cents = lin(p, 0.0, 50.0), None);
        })
        .gap(Pixels(tokens::SPACE_1))
        .size(Auto);
        knob(cx, state, theme, KnobSize::Sm, tokens::SIZE_KNOB_SM, "Width", 0.7,
            |s| s.unison.width,
            |s| format!("{:.0}%", s.unison.width * 100.0),
            |s, p| s.unison.width = p, None);
    })
    .alignment(Alignment::TopCenter)
    .width(Auto);
}

fn fx_section(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Effects", |_| {}, move |cx| {
        HStack::new(cx, move |cx| {
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, "Chorus", 0.0,
                |s| s.fx.chorus_mix,
                |s| format!("{:.0}%", s.fx.chorus_mix * 100.0),
                |s, p| s.fx.chorus_mix = p, None);
            knob(cx, state, theme, KnobSize::Sm, ROW_SLOT, "Depth", 0.4,
                |s| s.fx.chorus_depth,
                |s| format!("{:.0}%", s.fx.chorus_depth * 100.0),
                |s, p| s.fx.chorus_depth = p, None);
        })
        .gap(Pixels(tokens::SPACE_1))
        .size(Auto);
        HStack::new(cx, move |cx| {
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, "Reverb", 0.0,
                |s| s.fx.reverb_mix,
                |s| format!("{:.0}%", s.fx.reverb_mix * 100.0),
                |s, p| s.fx.reverb_mix = p, None);
            knob(cx, state, theme, KnobSize::Sm, ROW_SLOT, "Size", 0.5,
                |s| s.fx.reverb_size,
                |s| format!("{:.0}%", s.fx.reverb_size * 100.0),
                |s, p| s.fx.reverb_size = p, None);
        })
        .gap(Pixels(tokens::SPACE_1))
        .size(Auto);
    })
    .width(Auto);
}

fn out_section(
    cx: &mut Context,
    state: Signal<SynthState>,
    theme: Signal<ThemeId>,
    meter_l: Signal<f32>,
    meter_r: Signal<f32>,
) {
    section(cx, "Output", |_| {}, move |cx| {
        HStack::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                knob(cx, state, theme, KnobSize::Md, ROW_SLOT, "Volume", lin_inv(-3.0, -60.0, 6.0),
                    |s| lin_inv(s.output.volume_db, -60.0, 6.0),
                    |s| format!("{:.1} dB", s.output.volume_db),
                    |s, p| s.output.volume_db = lin(p, -60.0, 6.0), None);
                knob(cx, state, theme, KnobSize::Sm, tokens::SIZE_KNOB_SM, "Glide", log_inv(40.0, 1.0, 500.0),
                    |s| log_inv(s.output.glide_ms, 1.0, 500.0),
                    |s| format!("{:.0} ms", s.output.glide_ms),
                    |s, p| s.output.glide_ms = log(p, 1.0, 500.0), None);
            })
            .gap(Pixels(tokens::SPACE_2))
            .size(Auto);

            crate::meter::Meter::new(cx, meter_l, meter_r, Signal::new(false), Signal::new(false), theme, |_cx| {})
                .width(Pixels(10.0))
                .height(Pixels(96.0));
        })
        .gap(Pixels(tokens::SPACE_3))
        .size(Auto);
    })
    .width(Auto);
}

#[allow(clippy::too_many_arguments)]
pub fn synth_view(
    cx: &mut Context,
    theme: Signal<ThemeId>,
    state: Signal<SynthState>,
    lfo_phases: (Signal<f32>, Signal<f32>),
    octave_shift: Signal<i8>,
    meter_l: Signal<f32>,
    meter_r: Signal<f32>,
    help_open: Signal<bool>,
    lfo_drag: Signal<Option<usize>>,
    track_color: ClipColor,
) {
    LFO_DRAG.set(Some(lfo_drag));
    TRACK_COLOR.set(track_color);
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Element::new(cx).class("swatch").background_color(crate::timeline::header::clip_color_to_rgb(track_color));
            Label::new(cx, "Carve").class("heading");
            let preset = state.map(|s| if s.name.is_empty() { "Untitled".to_string() } else { s.name.to_string() });
            Label::new(cx, preset).class("readout").size(Auto);
            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
            Button::new(cx, |cx| Label::new(cx, "Guide"))
                .class("btn")
                .class("quiet")
                .on_press(|cx| cx.emit(SynthEvent::ToggleHelp));
            segmented(
                cx,
                2,
                |cx, i| Label::new(cx, if i == 0 { "Mono" } else { "Poly" }),
                move |i| state.map(move |s| (i == 1) == (s.voice_mode == VoiceMode::Poly)),
                move |cx, i| {
                    cx.emit(SynthEvent::SetVoiceMode(if i == 0 { VoiceMode::Mono } else { VoiceMode::Poly }));
                },
            );
            let voices = state.map(|s| format!("{} voices", s.voices));
            Label::new(cx, voices).class("readout").size(Auto);
        })
        .class("synth-devhead")
        .gap(Pixels(tokens::SPACE_2))
        .alignment(Alignment::Left)
        .width(Stretch(1.0))
        .height(Pixels(tokens::SIZE_TOOLBAR));
        hrule(cx);

        // Row 1, the sound: left to right is the signal path.
        HStack::new(cx, move |cx| {
            osc1_section(cx, state, theme);
            vrule(cx);
            osc2_section(cx, state, theme);
            vrule(cx);
            mix_section(cx, state, theme);
            vrule(cx);
            filter_section(cx, state, theme);
        })
        .class("synth-row")
        .width(Stretch(1.0))
        .height(Pixels(ROW1_HEIGHT));
        hrule(cx);

        // Row 2, what moves over time.
        HStack::new(cx, move |cx| {
            filter_env_section(cx, state, theme);
            vrule(cx);
            amp_env_section(cx, state, theme);
            vrule(cx);
            mod_section(cx, state, theme, lfo_phases);
            vrule(cx);
            unison_section(cx, state, theme);
            vrule(cx);
            fx_section(cx, state, theme);
            vrule(cx);
            out_section(cx, state, theme, meter_l, meter_r);
        })
        .class("synth-row")
        .width(Stretch(1.0))
        .height(Pixels(ROW2_HEIGHT));
        hrule(cx);

        HStack::new(cx, move |cx| {
            Keyboard::new(cx, state, theme).class("synth-keys").width(Stretch(1.0)).height(Pixels(48.0));
            let octave_text = octave_shift.map(|o| true_minus(format!("Oct {o:+}")));
            Label::new(cx, octave_text).class("value").width(Auto);
        })
        .gap(Pixels(tokens::SPACE_2))
        .padding(Pixels(tokens::SPACE_3))
        .alignment(Alignment::Center)
        .width(Stretch(1.0))
        .height(Auto);

        help::help_overlay(cx, help_open);
    })
    .class("device")
    .width(Stretch(1.0))
    .height(Auto);
}
