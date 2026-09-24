//! Carve: the subtractive synth device panel. Visual + interactive only
//! (see README) - every control here reads/writes `SynthState` but nothing
//! is wired to the audio engine yet.

pub mod display;
pub mod keyboard;
pub mod segmented;
pub mod state;

use vizia::prelude::*;

use shared::synth::{FilterType, SynthState, VoiceMode, Waveform};

use crate::knob::Knob;
use crate::pill::modulator_pill;
use crate::tokens::{self, ThemeId, CLIP_VIOLET};
use display::{EnvelopeDisplay, FilterDisplay, LfoScope, WaveDisplay};
use keyboard::Keyboard;
use segmented::segmented;
use state::{lin, lin_inv, log, log_inv, SynthEvent};

fn update(f: impl Fn(&mut SynthState) + Send + 'static) -> SynthEvent {
    SynthEvent::Update(Box::new(f))
}

/// A knob bound to a physical value via a `to_pos`/`apply` mapping pair
/// (see `state::{lin, log}` and their inverses), with no modulation ring.
#[allow(clippy::too_many_arguments)]
fn knob(
    cx: &mut Context,
    state: Signal<SynthState>,
    theme: Signal<ThemeId>,
    label: &'static str,
    default_pos: f32,
    to_pos: impl Fn(&SynthState) -> f32 + Copy + 'static,
    format: impl Fn(&SynthState) -> String + Copy + 'static,
    apply: impl Fn(&mut SynthState, f32) + Copy + Send + 'static,
) {
    let pos = state.map(move |s| to_pos(s));
    let text = state.map(move |s| format(s));
    VStack::new(cx, move |cx| {
        Knob::plain(cx, pos, default_pos, theme, move |cx, p| {
            cx.emit(update(move |s| apply(s, p)));
        })
        .size(Pixels(32.0));
        Label::new(cx, label).class("label");
        Label::new(cx, text).class("mono");
    })
    .alignment(Alignment::Center)
    .gap(Pixels(2.0))
    .width(Auto)
    .height(Auto);
}

/// Same as [`knob`], plus a modulation ring driven by `mod_center`/`mod_depth`.
#[allow(clippy::too_many_arguments)]
fn knob_with_mod(
    cx: &mut Context,
    state: Signal<SynthState>,
    theme: Signal<ThemeId>,
    label: &'static str,
    default_pos: f32,
    to_pos: impl Fn(&SynthState) -> f32 + Copy + 'static,
    format: impl Fn(&SynthState) -> String + Copy + 'static,
    apply: impl Fn(&mut SynthState, f32) + Copy + Send + 'static,
    mod_center: impl Fn(&SynthState) -> f32 + Copy + 'static,
    mod_depth: impl Fn(&SynthState) -> f32 + Copy + 'static,
) {
    let pos = state.map(move |s| to_pos(s));
    let text = state.map(move |s| format(s));
    let center = state.map(move |s| mod_center(s));
    let depth = state.map(move |s| mod_depth(s));
    VStack::new(cx, move |cx| {
        Knob::new(cx, pos, default_pos, theme, Some((center, depth)), move |cx, p| {
            cx.emit(update(move |s| apply(s, p)));
        })
        .size(Pixels(32.0));
        Label::new(cx, label).class("label");
        Label::new(cx, text).class("mono");
    })
    .alignment(Alignment::Center)
    .gap(Pixels(2.0))
    .width(Auto)
    .height(Auto);
}

fn section(cx: &mut Context, label: &'static str, next: &'static str, content: impl FnOnce(&mut Context)) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Label::new(cx, label).class("label");
            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
            if !next.is_empty() {
                Label::new(cx, next).class("meta");
            }
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

fn waveform_seg(cx: &mut Context, state: Signal<SynthState>, is_osc1: bool) {
    let waves = [Waveform::Sine, Waveform::Triangle, Waveform::Saw, Waveform::Square];
    let labels = ["sin", "tri", "saw", "sq"];
    segmented(
        cx,
        4,
        move |cx, i| Label::new(cx, labels[i]),
        move |i| {
            let current = if is_osc1 { state.get().osc1.waveform } else { state.get().osc2.waveform };
            current == waves[i]
        },
        move |cx, i| {
            cx.emit(if is_osc1 {
                SynthEvent::SetOsc1Waveform(waves[i])
            } else {
                SynthEvent::SetOsc2Waveform(waves[i])
            });
        },
    );
}

fn osc1_section(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Osc 1", "\u{2192} mix", move |cx| {
        WaveDisplay::new(cx, state, theme, |s| s.osc1).width(Pixels(176.0)).height(Pixels(40.0)).class("synth-disp");

        HStack::new(cx, move |cx| {
            waveform_seg(cx, state, true);
            let octave_text = state.map(|s| format!("{:+} oct", s.osc1.octave));
            Label::new(cx, octave_text).class("mono");
        })
        .gap(Pixels(tokens::SPACE_2))
        .alignment(Alignment::Left)
        .width(Auto)
        .height(Auto);

        HStack::new(cx, move |cx| {
            knob(cx, state, theme, "Tune", lin_inv(0.0, -100.0, 100.0),
                |s| lin_inv(s.osc1.knob_a_cents, -100.0, 100.0),
                |s| format!("{:.0} ct", s.osc1.knob_a_cents),
                |s, p| s.osc1.knob_a_cents = lin(p, -100.0, 100.0));
            knob(cx, state, theme, "Shape", 0.35,
                |s| s.osc1.knob_b,
                |s| format!("{:.0}%", s.osc1.knob_b * 100.0),
                |s, p| s.osc1.knob_b = p);
            knob(cx, state, theme, "Drift", 0.12,
                |s| s.osc1.knob_c,
                |s| format!("{:.0}%", s.osc1.knob_c * 100.0),
                |s, p| s.osc1.knob_c = p);
        })
        .gap(Pixels(tokens::SPACE_3))
        .width(Auto)
        .height(Auto);
    });
}

fn osc2_section(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Osc 2", "\u{2192} mix", move |cx| {
        WaveDisplay::new(cx, state, theme, |s| s.osc2).width(Pixels(176.0)).height(Pixels(40.0)).class("synth-disp");

        HStack::new(cx, move |cx| {
            waveform_seg(cx, state, false);
            let sync = state.map(|s| s.osc2.sync);
            Button::new(cx, |cx| Label::new(cx, "SYNC"))
                .class("btn")
                .class("sm")
                .toggle_class("is-mute", sync)
                .on_press(|cx| cx.emit(SynthEvent::ToggleOsc2Sync));
        })
        .gap(Pixels(tokens::SPACE_2))
        .alignment(Alignment::Left)
        .width(Auto)
        .height(Auto);

        HStack::new(cx, move |cx| {
            knob(cx, state, theme, "Detune", lin_inv(0.0, -50.0, 50.0),
                |s| lin_inv(s.osc2.knob_a_cents, -50.0, 50.0),
                |s| format!("{:+.0} ct", s.osc2.knob_a_cents),
                |s, p| s.osc2.knob_a_cents = lin(p, -50.0, 50.0));
            knob_with_mod(cx, state, theme, "PW", 0.38,
                |s| s.osc2.knob_b,
                |s| format!("{:.0}%", s.osc2.knob_b * 100.0),
                |s, p| s.osc2.knob_b = p,
                |s| s.osc2.knob_b,
                |_| 0.15);
            knob(cx, state, theme, "FM", 0.0,
                |s| s.osc2.knob_c,
                |s| format!("{:.0}%", s.osc2.knob_c * 100.0),
                |s, p| s.osc2.knob_c = p);
        })
        .gap(Pixels(tokens::SPACE_3))
        .width(Auto)
        .height(Auto);
    });
}

fn mix_section(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Mix", "\u{2192} filter", move |cx| {
        let db_knob = |cx: &mut Context, label: &'static str, default_db: f32, get: fn(&SynthState) -> f32, set: fn(&mut SynthState, f32)| {
            knob(cx, state, theme, label, lin_inv(default_db, -60.0, 0.0),
                move |s| lin_inv(get(s), -60.0, 0.0),
                move |s| format!("{:.1} dB", get(s)),
                move |s, p| set(s, lin(p, -60.0, 0.0)));
        };
        HStack::new(cx, move |cx| {
            db_knob(cx, "Osc 1", -1.9, |s| s.mix.osc1_db, |s, v| s.mix.osc1_db = v);
            db_knob(cx, "Osc 2", -5.2, |s| s.mix.osc2_db, |s, v| s.mix.osc2_db = v);
        })
        .gap(Pixels(tokens::SPACE_3))
        .width(Auto)
        .height(Auto);
        HStack::new(cx, move |cx| {
            db_knob(cx, "Sub", -10.0, |s| s.mix.sub_db, |s, v| s.mix.sub_db = v);
            db_knob(cx, "Noise", -28.0, |s| s.mix.noise_db, |s, v| s.mix.noise_db = v);
        })
        .gap(Pixels(tokens::SPACE_3))
        .width(Auto)
        .height(Auto);
    });
}

fn filter_section(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Filter", "\u{2192} amp", move |cx| {
        FilterDisplay::new(cx, state, theme).width(Pixels(340.0)).height(Pixels(76.0)).class("synth-disp");

        HStack::new(cx, move |cx| {
            let types = [FilterType::Lp24, FilterType::Lp12, FilterType::Bp, FilterType::Hp];
            let labels = ["LP 24", "LP 12", "BP", "HP"];
            segmented(
                cx,
                4,
                move |cx, i| Label::new(cx, labels[i]),
                move |i| state.get().filter.filter_type == types[i],
                move |cx, i| cx.emit(SynthEvent::SetFilterType(types[i])),
            );
            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
            Label::new(cx, "ladder").class("meta");
        })
        .alignment(Alignment::Left)
        .width(Stretch(1.0))
        .height(Auto);

        HStack::new(cx, move |cx| {
            knob_with_mod(cx, state, theme, "Cutoff", log_inv(1200.0, 20.0, 20_000.0),
                |s| log_inv(s.filter.cutoff_hz, 20.0, 20_000.0),
                |s| format!("{:.2} kHz", s.filter.cutoff_hz / 1000.0),
                |s, p| s.filter.cutoff_hz = log(p, 20.0, 20_000.0),
                |s| log_inv(s.filter.cutoff_hz, 20.0, 20_000.0),
                |s| s.filter.cutoff_mod_depth);
            knob(cx, state, theme, "Reso", 0.62,
                |s| s.filter.resonance,
                |s| format!("{:.0}%", s.filter.resonance * 100.0),
                |s, p| s.filter.resonance = p);
            knob(cx, state, theme, "Drive", lin_inv(4.5, 0.0, 24.0),
                |s| lin_inv(s.filter.drive_db, 0.0, 24.0),
                |s| format!("{:+.1} dB", s.filter.drive_db),
                |s, p| s.filter.drive_db = lin(p, 0.0, 24.0));
            knob(cx, state, theme, "Env", lin_inv(2.4, -4.0, 4.0),
                |s| lin_inv(s.filter.env_amount_oct, -4.0, 4.0),
                |s| format!("{:+.1} oct", s.filter.env_amount_oct),
                |s, p| s.filter.env_amount_oct = lin(p, -4.0, 4.0));
            knob(cx, state, theme, "Key trk", 0.5,
                |s| s.filter.key_track,
                |s| format!("{:.0}%", s.filter.key_track * 100.0),
                |s, p| s.filter.key_track = p);
        })
        .gap(Pixels(tokens::SPACE_3))
        .width(Auto)
        .height(Auto);
    });
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
    HStack::new(cx, move |cx| {
        knob(cx, state, theme, "A", log_inv(get(&state.get()).attack_ms, 1.0, 2000.0),
            move |s| log_inv(get(s).attack_ms, 1.0, 2000.0),
            move |s| format!("{:.0} ms", get(s).attack_ms),
            move |s, p| set_a(s, log(p, 1.0, 2000.0)));
        knob(cx, state, theme, "D", log_inv(get(&state.get()).decay_ms, 1.0, 2000.0),
            move |s| log_inv(get(s).decay_ms, 1.0, 2000.0),
            move |s| format!("{:.0} ms", get(s).decay_ms),
            move |s, p| set_d(s, log(p, 1.0, 2000.0)));
        knob(cx, state, theme, "S", get(&state.get()).sustain,
            move |s| get(s).sustain,
            move |s| format!("{:.0}%", get(s).sustain * 100.0),
            set_s);
        knob(cx, state, theme, "R", log_inv(get(&state.get()).release_ms, 1.0, 2000.0),
            move |s| log_inv(get(s).release_ms, 1.0, 2000.0),
            move |s| format!("{:.0} ms", get(s).release_ms),
            move |s, p| set_r(s, log(p, 1.0, 2000.0)));
    })
    .gap(Pixels(tokens::SPACE_2))
    .width(Auto)
    .height(Auto);
}

fn filter_env_section(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Filter env", "\u{2192} cutoff", move |cx| {
        EnvelopeDisplay::new(cx, state, theme, |s| s.filter_env)
            .width(Pixels(236.0))
            .height(Pixels(56.0))
            .class("synth-disp");
        adsr_knobs(
            cx, state, theme,
            |s| s.filter_env,
            |s, v| s.filter_env.attack_ms = v,
            |s, v| s.filter_env.decay_ms = v,
            |s, v| s.filter_env.sustain = v,
            |s, v| s.filter_env.release_ms = v,
        );
    });
}

fn amp_env_section(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Amp env", "\u{2192} out", move |cx| {
        EnvelopeDisplay::new(cx, state, theme, |s| s.amp_env)
            .width(Pixels(236.0))
            .height(Pixels(56.0))
            .class("synth-disp");
        adsr_knobs(
            cx, state, theme,
            |s| s.amp_env,
            |s, v| s.amp_env.attack_ms = v,
            |s, v| s.amp_env.decay_ms = v,
            |s, v| s.amp_env.sustain = v,
            |s, v| s.amp_env.release_ms = v,
        );
    });
}

fn mod_section(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>, lfo_phase: Signal<f32>) {
    section(cx, "Mod", "drag to a knob", move |cx| {
        HStack::new(cx, move |cx| {
            let count1 = state.map(|s| s.lfo1.target_count);
            let count2 = state.map(|s| s.lfo2.target_count);
            modulator_pill(cx, theme, "LFO 1", count1.get());
            modulator_pill(cx, theme, "LFO 2", count2.get());
        })
        .gap(Pixels(tokens::SPACE_2))
        .width(Auto)
        .height(Auto);

        LfoScope::new(cx, state, theme, lfo_phase).width(Pixels(160.0)).height(Pixels(40.0)).class("synth-disp");

        HStack::new(cx, move |cx| {
            knob(cx, state, theme, "Rate", 0.45, |s| s.lfo1.rate_norm, |s| s.lfo1.rate_label.to_string(), |s, p| s.lfo1.rate_norm = p);
            knob(cx, state, theme, "Depth", 0.6, |s| s.lfo1.depth, |s| format!("{:.0}%", s.lfo1.depth * 100.0), |s, p| s.lfo1.depth = p);
            let sync = state.map(|s| s.lfo1.sync);
            Button::new(cx, |cx| Label::new(cx, "SYNC"))
                .class("btn")
                .class("sm")
                .toggle_class("is-mute", sync)
                .on_press(|cx| cx.emit(SynthEvent::ToggleLfo1Sync));
        })
        .gap(Pixels(tokens::SPACE_2))
        .width(Auto)
        .height(Auto);

        HStack::new(cx, move |cx| {
            knob(cx, state, theme, "Rate", 0.3, |s| s.lfo2.rate_norm, |s| s.lfo2.rate_label.to_string(), |s, p| s.lfo2.rate_norm = p);
            knob(cx, state, theme, "Depth", 0.4, |s| s.lfo2.depth, |s| format!("{:.0}%", s.lfo2.depth * 100.0), |s, p| s.lfo2.depth = p);
            let sync = state.map(|s| s.lfo2.sync);
            Button::new(cx, |cx| Label::new(cx, "SYNC"))
                .class("btn")
                .class("sm")
                .toggle_class("is-mute", sync)
                .on_press(|cx| cx.emit(SynthEvent::ToggleLfo2Sync));
        })
        .gap(Pixels(tokens::SPACE_2))
        .width(Auto)
        .height(Auto);
    });
}

fn out_section(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>) {
    section(cx, "Out", "", move |cx| {
        HStack::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                knob(cx, state, theme, "Glide", log_inv(40.0, 1.0, 500.0),
                    |s| log_inv(s.output.glide_ms, 1.0, 500.0),
                    |s| format!("{:.0} ms", s.output.glide_ms),
                    |s, p| s.output.glide_ms = log(p, 1.0, 500.0));
                knob(cx, state, theme, "Volume", lin_inv(-3.0, -60.0, 6.0),
                    |s| lin_inv(s.output.volume_db, -60.0, 6.0),
                    |s| format!("{:.1} dB", s.output.volume_db),
                    |s, p| s.output.volume_db = lin(p, -60.0, 6.0));
            })
            .gap(Pixels(tokens::SPACE_2))
            .width(Auto)
            .height(Auto);

            crate::meter::Meter::new(
                cx,
                state.map(|s| s.output.meter_l),
                state.map(|s| s.output.meter_r),
                Signal::new(false),
                Signal::new(false),
                theme,
                |_cx| {},
            )
            .width(Pixels(10.0))
            .height(Pixels(88.0));
        })
        .gap(Pixels(tokens::SPACE_3))
        .width(Auto)
        .height(Auto);
    });
}

pub fn synth_view(cx: &mut Context, theme: Signal<ThemeId>, state: Signal<SynthState>, lfo_phase: Signal<f32>) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Element::new(cx).class("swatch").background_color(CLIP_VIOLET);
            Label::new(cx, "Carve").class("control");
            Label::new(cx, "subtractive").class("meta");
            Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(16.0));
            Label::new(cx, "Osc \u{2192} Mix \u{2192} Filter \u{2192} Amp \u{2192} Out").class("meta");
            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
            let preset = state.map(|s| s.name.to_string());
            Label::new(cx, preset).class("readout").size(Auto);
            segmented(
                cx,
                2,
                |cx, i| Label::new(cx, if i == 0 { "Mono" } else { "Poly" }),
                move |i| {
                    let on = state.get().voice_mode == VoiceMode::Poly;
                    (i == 1) == on
                },
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
        .width(Stretch(1.0));

        HStack::new(cx, move |cx| {
            osc1_section(cx, state, theme);
            osc2_section(cx, state, theme);
            mix_section(cx, state, theme);
            filter_section(cx, state, theme);
        })
        .gap(Pixels(tokens::SPACE_2))
        .width(Auto)
        .height(Auto);

        HStack::new(cx, move |cx| {
            filter_env_section(cx, state, theme);
            amp_env_section(cx, state, theme);
            mod_section(cx, state, theme, lfo_phase);
            out_section(cx, state, theme);
        })
        .gap(Pixels(tokens::SPACE_2))
        .width(Auto)
        .height(Auto);

        Keyboard::new(cx, state, theme).class("synth-keys").width(Stretch(1.0)).height(Pixels(48.0));
    })
    .class("panel")
    .gap(Pixels(tokens::SPACE_2))
    .padding(Pixels(tokens::SPACE_2))
    .width(Auto)
    .height(Auto);
}
