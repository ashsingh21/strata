//! The EQ device panel: three plain knobs (freq, gain, Q) over the
//! selected track's `EqState`. Same shape as `compressor_panel` - see
//! that module's own doc comment for the knob-column idiom this mirrors.

use std::cell::Cell;

use vizia::prelude::*;

use shared::arrangement::{Arrangement, ClipColor, Effect, EqState, TrackId};

use crate::knob::{Knob, KnobAccentExt};
use crate::synth::state::{log, log_inv};
use crate::timeline::state::TimelineEvent;
use crate::tokens::{self, Palette, ThemeId};

thread_local! {
    static TRACK_COLOR: Cell<ClipColor> = const { Cell::new(ClipColor::Violet) };
}

fn eq_accent(p: &Palette) -> Color {
    match TRACK_COLOR.get() {
        ClipColor::Coral => p.clip_coral_line,
        ClipColor::Amber => p.clip_amber_line,
        ClipColor::Teal => p.clip_teal_line,
        ClipColor::Blue => p.clip_blue_line,
        ClipColor::Violet => p.clip_violet_line,
        ClipColor::Pink => p.clip_pink_line,
    }
}

fn lin(min: f32, max: f32, value: f32) -> f32 {
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

fn inv_lin(min: f32, max: f32, pos: f32) -> f32 {
    min + (max - min) * pos.clamp(0.0, 1.0)
}

pub fn eq_panel(cx: &mut Context, theme: Signal<ThemeId>, arrangement: Signal<Arrangement>, track_id: TrackId, track_color: ClipColor) {
    TRACK_COLOR.set(track_color);

    let state = arrangement.map(move |arr| {
        arr.track(track_id)
            .and_then(|t| {
                t.fx.ordered().iter().find_map(|n| match n.effect {
                    Effect::Eq(e) => Some(e),
                    _ => None,
                })
            })
            .unwrap_or_default()
    });
    let node_id = arrangement.map(move |arr| {
        arr.track(track_id).and_then(|t| t.fx.ordered().iter().find(|n| matches!(n.effect, Effect::Eq(_))).map(|n| n.id))
    });
    let enabled = arrangement.map(move |arr| {
        arr.track(track_id)
            .and_then(|t| t.fx.ordered().iter().find(|n| matches!(n.effect, Effect::Eq(_))).map(|n| n.enabled))
            .unwrap_or(true)
    });

    Button::new(cx, |cx| Label::new(cx, "Enabled"))
        .class("btn")
        .class("sm")
        .toggle_class("is-on", enabled)
        .on_press(move |cx| {
            if let Some(id) = node_id.get() {
                cx.emit(TimelineEvent::ToggleEffectEnabled(track_id, id));
            }
        });

    HStack::new(cx, move |cx| {
        crate::eq_curve::eq_curve(cx, state, theme);

        knob_col(cx, theme, state, track_id, "Freq", log_inv(1000.0, 20.0, 20_000.0), |s| log_inv(s.freq_hz, 20.0, 20_000.0), |s, p| {
            s.freq_hz = log(p, 20.0, 20_000.0);
        }, |s| format!("{:.0} Hz", s.freq_hz));

        knob_col(cx, theme, state, track_id, "Gain", lin(-18.0, 18.0, 0.0), |s| lin(-18.0, 18.0, s.gain_db), |s, p| {
            s.gain_db = inv_lin(-18.0, 18.0, p);
        }, |s| format!("{:+.1} dB", s.gain_db));

        knob_col(cx, theme, state, track_id, "Q", lin(0.1, 10.0, 1.0), |s| lin(0.1, 10.0, s.q), |s, p| {
            s.q = inv_lin(0.1, 10.0, p);
        }, |s| format!("{:.2}", s.q));
    })
    .class("device")
    .gap(Pixels(tokens::SPACE_4))
    .alignment(Alignment::Center)
    .padding(Pixels(tokens::SPACE_3))
    .width(Stretch(1.0))
    .height(Pixels(96.0));
}

#[allow(clippy::too_many_arguments)]
fn knob_col(
    cx: &mut Context,
    theme: Signal<ThemeId>,
    state: Memo<EqState>,
    track_id: TrackId,
    label: &'static str,
    default_pos: f32,
    to_pos: impl Fn(EqState) -> f32 + Copy + 'static,
    apply: impl Fn(&mut EqState, f32) + Copy + 'static,
    format: impl Fn(EqState) -> String + Copy + 'static,
) {
    let pos = state.map(move |s| to_pos(*s));
    let text = state.map(move |s| format(*s));
    VStack::new(cx, move |cx| {
        Knob::plain(cx, pos, default_pos, theme, move |cx, p| {
            let mut updated = state.get();
            apply(&mut updated, p);
            cx.emit(TimelineEvent::SetEqState(track_id, updated));
        })
        .accent(eq_accent)
        .size(Pixels(tokens::SIZE_KNOB));
        Label::new(cx, label).class("label");
        Label::new(cx, text).class("value");
    })
    .class("knob-col")
    .alignment(Alignment::Center)
    .gap(Pixels(2.0))
    .width(Auto)
    .height(Auto);
}
