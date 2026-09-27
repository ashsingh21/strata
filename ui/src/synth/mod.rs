//! Carve: the subtractive synth device panel. Every control here reads/
//! writes `SynthState`, which is mirrored to the engine's real-time voice
//! renderer (see `engine::synth` and `shared::synth::bridge`) - so turning
//! a knob or playing a note here makes real sound.

pub mod display;
pub mod help;
pub mod keyboard;
pub mod segmented;
pub mod state;

use crate::lessons::LessonTargetExt;
use vizia::prelude::*;

use std::cell::Cell;

use shared::synth::{lfo_mod_depth, FilterType, LfoTarget, SynthParam, SynthState, VoiceMode, Waveform};

use crate::glyph::{ink_when_on, Glyph, GlyphKind};
use crate::knob::{Knob, KnobAccentExt};
use crate::status::StatusEvent;
use crate::pill::modulator_pill;
use crate::tokens::{self, ThemeId};
use shared::arrangement::{Arrangement, AutomationTarget, ClipColor, Ticks, TrackId};
use display::{EnvelopeDisplay, FilterDisplay, LfoScope, WaveDisplay};
use keyboard::Keyboard;
use segmented::segmented;
use state::{lin_inv, log_inv, SynthEvent};

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
    /// The selected track's automated Carve params - set by `synth_view`,
    /// read by `knob` (same pattern as LFO_DRAG, since knob's callers don't
    /// carry the arrangement).
    static AUTOMATED: Cell<Option<Memo<Vec<SynthParam>>>> = const { Cell::new(None) };
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
    state: Memo<SynthState>,
    theme: Signal<ThemeId>,
    size: KnobSize,
    slot: f32,
    param: SynthParam,
    default_pos: f32,
    route: Option<LfoTarget>,
) {
    // Name, position, readout and how a position applies all come from the
    // shared SynthParam table - the same one automation lanes use.
    let label = param.name();
    let to_pos = move |s: &SynthState| param.norm(s);
    let apply = move |s: &mut SynthState, p: f32| param.apply_norm(s, p);
    let pos = state.map(move |s| to_pos(s));
    let text = state.map(move |s| true_minus(param.format(s)));
    // An automated knob follows its lane (`state` already has it applied)
    // and is read-only - a drag would only be overridden by the lane.
    let automated = Memo::new(move |_| AUTOMATED.get().is_some_and(|a| a.get().contains(&param)));
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
        .pointer_events(automated.map(|a| if *a { PointerEvents::None } else { PointerEvents::Auto }))
        .width(Auto)
        .height(Pixels(slot));
        Label::new(cx, label).class(if size == KnobSize::Lg { "label-lg" } else { "label" });
        Label::new(cx, text).class("value");
    })
    .class("knob-col")
    .lesson_target(crate::lessons::Target::Knob(param))
    .toggle_class("is-automated", automated)
    // Right-click: "Automate Carve · <param>" on the selected track.
    .on_mouse_down(move |cx, button| {
        if button == MouseButton::Right {
            let (x, y) = (cx.mouse().cursor_x, cx.mouse().cursor_y);
            cx.emit(SynthEvent::OpenAutomateMenu { param, x, y });
        }
    })
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

/// Carve's presets, where they belong - on the instrument: arrows to
/// step through them, the name to open the full list.
fn preset_selector(cx: &mut Context, state: Memo<SynthState>) {
    use shared::synth::PRESETS;
    let open = Signal::new(false);
    // Where the current patch sits in the list (a patch not from the list
    // steps from the start).
    let index = move || PRESETS.iter().position(|(n, _)| *n == state.get().name);
    let step = move |cx: &mut EventContext, delta: isize| {
        let len = PRESETS.len() as isize;
        let next = match index() {
            Some(i) => (i as isize + delta).rem_euclid(len),
            None => if delta > 0 { 0 } else { len - 1 },
        };
        cx.emit(SynthEvent::LoadPreset(PRESETS[next as usize].1));
    };
    HStack::new(cx, move |cx| {
        Button::new(cx, |cx| Label::new(cx, "\u{2039}"))
            .class("btn")
            .class("quiet")
            .class("sm")
            .on_press(move |cx| step(cx, -1));
        let name = state.map(|s| if s.name.is_empty() { "Untitled".to_string() } else { s.name.to_string() });
        Button::new(cx, move |cx| Label::new(cx, name).hoverable(false))
            .class("btn")
            .class("sm")
            .min_width(Pixels(120.0))
            .lesson_target_if(|t| matches!(t, Some(crate::lessons::Target::Preset(_))))
            .on_press(move |_| open.update(|o| *o = !*o));
        Button::new(cx, |cx| Label::new(cx, "\u{203a}"))
            .class("btn")
            .class("quiet")
            .class("sm")
            .on_press(move |cx| step(cx, 1));

        // The list, dropping down under the name.
        VStack::new(cx, move |cx| {
            for (preset, build) in PRESETS {
                Button::new(cx, move |cx| Label::new(cx, preset).class("body").hoverable(false))
                    .class("menu-item")
                    .toggle_class("is-on", state.map(move |s| s.name == preset))
                    .lesson_target(crate::lessons::Target::Preset(preset))
                    .width(Stretch(1.0))
                    .on_press(move |cx| {
                        cx.emit(SynthEvent::LoadPreset(build));
                        open.set(false);
                    });
            }
        })
        .class("panel")
        .class("context-menu")
        .toggle_class("hidden", open.map(|o| !*o))
        .position_type(PositionType::Absolute)
        .top(Pixels(tokens::SIZE_CONTROL + 4.0))
        .left(Pixels(0.0))
        .width(Pixels(200.0))
        .height(Auto);
    })
    .gap(Pixels(2.0))
    .alignment(Alignment::Left)
    .size(Auto);
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

fn waveform_seg(cx: &mut Context, state: Memo<SynthState>, theme: Signal<ThemeId>, is_osc1: bool) {
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
    )
    .lesson_target(crate::lessons::Target::OscWave(if is_osc1 { 1 } else { 2 }));
}

const OCTAVE_RANGE: f32 = 3.0;

fn octave_pos(octave: i8) -> f32 {
    lin_inv(octave as f32, -OCTAVE_RANGE, OCTAVE_RANGE)
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

fn osc1_section(cx: &mut Context, state: Memo<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Oscillator 1", move |cx| waveform_seg(cx, state, theme, true), move |cx| {
        WaveDisplay::new(cx, state, theme, |s| s.osc1).width(Stretch(1.0)).height(Pixels(44.0)).class("synth-disp");
        knob_row(cx, move |cx| {
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, SynthParam::Osc1Octave, octave_pos(-1), None);
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, SynthParam::Osc1Tune, lin_inv(0.0, -100.0, 100.0), Some(LfoTarget::Pitch));
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, SynthParam::Osc1Shape, 0.35, None);
            knob(cx, state, theme, KnobSize::Sm, ROW_SLOT, SynthParam::Osc1Drift, 0.12, None);
        });
    })
    .width(Pixels(OSC_WIDTH));
}

fn osc2_section(cx: &mut Context, state: Memo<SynthState>, theme: Signal<ThemeId>) {
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
                knob(cx, state, theme, KnobSize::Sm, ROW_SLOT, SynthParam::Osc2Octave, octave_pos(0), None);
                knob(cx, state, theme, KnobSize::Md, ROW_SLOT, SynthParam::Osc2Detune, lin_inv(0.0, -50.0, 50.0), None);
                knob(cx, state, theme, KnobSize::Md, ROW_SLOT, SynthParam::Osc2PulseWidth, 0.38, Some(LfoTarget::PulseWidth));
                knob(cx, state, theme, KnobSize::Md, ROW_SLOT, SynthParam::Osc2Fm, 0.0, None);
            });
        },
    )
    .width(Pixels(OSC_WIDTH + 44.0));
}

fn mix_section(cx: &mut Context, state: Memo<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Mixer", |_| {}, move |cx| {
        let db_knob = |cx: &mut Context, param: SynthParam, default_db: f32| {
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, param, lin_inv(default_db, -60.0, 0.0), None);
        };
        HStack::new(cx, move |cx| {
            db_knob(cx, SynthParam::Osc1Level, -1.9);
            db_knob(cx, SynthParam::Osc2Level, -5.2);
        })
        .gap(Pixels(tokens::SPACE_3))
        .size(Auto);
        HStack::new(cx, move |cx| {
            db_knob(cx, SynthParam::SubLevel, -10.0);
            db_knob(cx, SynthParam::NoiseLevel, -28.0);
        })
        .gap(Pixels(tokens::SPACE_3))
        .size(Auto);
    })
    .width(Auto);
}

fn filter_section(cx: &mut Context, state: Memo<SynthState>, theme: Signal<ThemeId>) {
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
            )
            .lesson_target(crate::lessons::Target::FilterType);
        },
        move |cx| {
            FilterDisplay::new(cx, state, theme).width(Stretch(1.0)).height(Pixels(88.0)).class("synth-disp");
            let slot = tokens::SIZE_KNOB_LG;
            knob_row(cx, move |cx| {
                knob(cx, state, theme, KnobSize::Lg, slot, SynthParam::Cutoff, log_inv(1200.0, 20.0, 20_000.0), Some(LfoTarget::Cutoff));
                knob(cx, state, theme, KnobSize::Md, slot, SynthParam::Resonance, 0.62, Some(LfoTarget::Resonance));
                knob(cx, state, theme, KnobSize::Md, slot, SynthParam::Drive, lin_inv(4.5, 0.0, 24.0), None);
                knob(cx, state, theme, KnobSize::Md, slot, SynthParam::EnvAmount, lin_inv(2.4, -4.0, 4.0), None);
                knob(cx, state, theme, KnobSize::Sm, slot, SynthParam::KeyTrack, 0.5, None);
            });
        },
    )
    .width(Stretch(1.0));
}
fn adsr_knobs(cx: &mut Context, state: Memo<SynthState>, theme: Signal<ThemeId>, params: [SynthParam; 4]) {
    knob_row(cx, move |cx| {
        for param in params {
            // Default (double-click) = the value the section opened with.
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, param, param.norm(&state.get()), None);
        }
    });
}

fn filter_env_section(cx: &mut Context, state: Memo<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Filter envelope", |_| {}, move |cx| {
        EnvelopeDisplay::new(cx, state, theme, |s| s.filter_env)
            .width(Stretch(1.0))
            .height(Pixels(44.0))
            .class("synth-disp");
        adsr_knobs(
            cx, state, theme,
            [SynthParam::FilterAttack, SynthParam::FilterDecay, SynthParam::FilterSustain, SynthParam::FilterRelease],
        );
    })
    .width(Stretch(1.0));
}

fn amp_env_section(cx: &mut Context, state: Memo<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Amp envelope", |_| {}, move |cx| {
        EnvelopeDisplay::new(cx, state, theme, |s| s.amp_env)
            .width(Stretch(1.0))
            .height(Pixels(44.0))
            .class("synth-disp");
        adsr_knobs(
            cx, state, theme,
            [SynthParam::AmpAttack, SynthParam::AmpDecay, SynthParam::AmpSustain, SynthParam::AmpRelease],
        );
    })
    .width(Stretch(1.0));
}

/// One LFO's column: Rate and Depth, then its Sync toggle and target.
/// `lfo`/`lfo_mut` pick LFO 1 or 2 out of the state.
fn lfo_column(
    cx: &mut Context,
    state: Memo<SynthState>,
    theme: Signal<ThemeId>,
    lfo: fn(&SynthState) -> &shared::synth::Lfo,
    params: (SynthParam, SynthParam),
    set_target: fn(LfoTarget) -> SynthEvent,
    phase: Signal<f32>,
) {
    VStack::new(cx, move |cx| {
        LfoScope::new(cx, state, theme, phase, lfo).width(Stretch(1.0)).height(Pixels(40.0)).class("synth-disp");
        HStack::new(cx, move |cx| {
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, params.0, lfo(&state.get()).rate_norm, None);
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, params.1, lfo(&state.get()).depth, None);
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
    state: Memo<SynthState>,
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
                    .lesson_target(crate::lessons::Target::LfoPill(i as u8 + 1))
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
                lfo_column(cx, state, theme, |s| &s.lfo1, (SynthParam::Lfo1Rate, SynthParam::Lfo1Depth),
                    SynthEvent::SetLfo1Target, lfo_phases.0);
                Element::new(cx).class("hairline").width(Pixels(1.0)).height(Stretch(1.0));
                lfo_column(cx, state, theme, |s| &s.lfo2, (SynthParam::Lfo2Rate, SynthParam::Lfo2Depth),
                    SynthEvent::SetLfo2Target, lfo_phases.1);
            })
            .gap(Stretch(1.0))
            .width(Stretch(1.0))
            .height(Auto);
        },
    )
    .width(Pixels(300.0));
}

fn unison_section(cx: &mut Context, state: Memo<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Unison", |_| {}, move |cx| {
        HStack::new(cx, move |cx| {
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, SynthParam::UnisonVoices, 0.0, None);
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, SynthParam::UnisonDetune, lin_inv(14.0, 0.0, 50.0), None);
        })
        .gap(Pixels(tokens::SPACE_1))
        .size(Auto);
        knob(cx, state, theme, KnobSize::Sm, tokens::SIZE_KNOB_SM, SynthParam::UnisonWidth, 0.7, None);
    })
    .alignment(Alignment::TopCenter)
    .width(Auto);
}

fn fx_section(cx: &mut Context, state: Memo<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Effects", |_| {}, move |cx| {
        HStack::new(cx, move |cx| {
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, SynthParam::ChorusMix, 0.0, None);
            knob(cx, state, theme, KnobSize::Sm, ROW_SLOT, SynthParam::ChorusDepth, 0.4, None);
        })
        .gap(Pixels(tokens::SPACE_1))
        .size(Auto);
        HStack::new(cx, move |cx| {
            knob(cx, state, theme, KnobSize::Md, ROW_SLOT, SynthParam::ReverbMix, 0.0, None);
            knob(cx, state, theme, KnobSize::Sm, ROW_SLOT, SynthParam::ReverbSize, 0.5, None);
        })
        .gap(Pixels(tokens::SPACE_1))
        .size(Auto);
    })
    .width(Auto);
}

fn out_section(
    cx: &mut Context,
    state: Memo<SynthState>,
    theme: Signal<ThemeId>,
    meter_l: Signal<f32>,
    meter_r: Signal<f32>,
) {
    section(cx, "Output", |_| {}, move |cx| {
        HStack::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                knob(cx, state, theme, KnobSize::Md, ROW_SLOT, SynthParam::Volume, lin_inv(-3.0, -60.0, 6.0), None);
                knob(cx, state, theme, KnobSize::Sm, tokens::SIZE_KNOB_SM, SynthParam::Glide, log_inv(40.0, 1.0, 500.0), None);
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
    patch: Signal<SynthState>,
    arrangement: Signal<Arrangement>,
    track: Signal<Option<TrackId>>,
    playhead: Signal<Ticks>,
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
    // What the panel shows: the patch as it sounds at the playhead, i.e.
    // with this track's Carve automation applied. Knobs, displays and the
    // keyboard all read this; edits still go to the patch via SynthEvent.
    let state = Memo::new(move |_| {
        let mut shown = patch.get();
        if let Some(track) = track.get() {
            arrangement.get().apply_synth_automation(track, playhead.get(), &mut shown);
        }
        shown
    });
    let automated = Memo::new(move |_| {
        let arr = arrangement.get();
        arr.automation
            .iter()
            .filter(|l| Some(l.track) == track.get())
            .filter_map(|l| match l.target {
                Some(AutomationTarget::Synth(param)) => Some(param),
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    AUTOMATED.set(Some(automated));
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Element::new(cx).class("swatch").background_color(crate::timeline::header::clip_color_to_rgb(track_color));
            Label::new(cx, "Carve").class("heading");
            preset_selector(cx, state);
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
            )
            .lesson_target(crate::lessons::Target::VoiceMode);
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
