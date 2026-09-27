//! The Compressor device panel: five plain knobs (threshold, ratio,
//! attack, release, makeup) over the selected track's `CompressorState`.
//! Same knob-column visual idiom as Carve's own panels
//! (`ui/src/synth/mod.rs`'s `knob` helper), without the LFO-routing/
//! `StatusEvent` machinery those need - this has no modulation.

use std::cell::Cell;

use vizia::prelude::*;

use shared::arrangement::{Arrangement, ClipColor, CompressorState, Effect, TrackId};

use crate::knob::{Knob, KnobAccentExt};
use crate::timeline::state::TimelineEvent;
use crate::tokens::{self, Palette, ThemeId};

thread_local! {
    /// Set once at the top of `compressor_panel`, read by `compressor_accent` -
    /// same pattern as `synth::mod::TRACK_COLOR`, needed because `Knob`'s
    /// accent is a plain `fn(&Palette) -> Color`, not a closure, so it
    /// can't capture the track's colour directly.
    static TRACK_COLOR: Cell<ClipColor> = const { Cell::new(ClipColor::Violet) };
}

fn compressor_accent(p: &Palette) -> Color {
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

pub fn compressor_panel(
    cx: &mut Context,
    theme: Signal<ThemeId>,
    arrangement: Signal<Arrangement>,
    track_id: TrackId,
    track_color: ClipColor,
) {
    TRACK_COLOR.set(track_color);

    let state = arrangement.map(move |arr| {
        arr.track(track_id)
            .and_then(|t| {
                t.fx.ordered().iter().find_map(|n| match n.effect {
                    Effect::Compressor(c) => Some(c),
                })
            })
            .unwrap_or_default()
    });
    let node_id = arrangement.map(move |arr| {
        arr.track(track_id).and_then(|t| t.fx.ordered().iter().find(|n| matches!(n.effect, Effect::Compressor(_))).map(|n| n.id))
    });
    let enabled = arrangement.map(move |arr| {
        arr.track(track_id)
            .and_then(|t| t.fx.ordered().iter().find(|n| matches!(n.effect, Effect::Compressor(_))).map(|n| n.enabled))
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
        knob_col(cx, theme, state, track_id, "Threshold", lin(-60.0, 0.0, -18.0), |s| lin(-60.0, 0.0, s.threshold_db), |s, p| {
            s.threshold_db = inv_lin(-60.0, 0.0, p);
        }, |s| format!("{:+.1} dB", s.threshold_db));

        knob_col(cx, theme, state, track_id, "Ratio", lin(1.0, 20.0, 4.0), |s| lin(1.0, 20.0, s.ratio), |s, p| {
            s.ratio = inv_lin(1.0, 20.0, p);
        }, |s| format!("{:.1}:1", s.ratio));

        knob_col(cx, theme, state, track_id, "Attack", lin(0.1, 100.0, 10.0), |s| lin(0.1, 100.0, s.attack_ms), |s, p| {
            s.attack_ms = inv_lin(0.1, 100.0, p);
        }, |s| format!("{:.1} ms", s.attack_ms));

        knob_col(cx, theme, state, track_id, "Release", lin(10.0, 1000.0, 150.0), |s| lin(10.0, 1000.0, s.release_ms), |s, p| {
            s.release_ms = inv_lin(10.0, 1000.0, p);
        }, |s| format!("{:.0} ms", s.release_ms));

        knob_col(cx, theme, state, track_id, "Makeup", lin(0.0, 24.0, 0.0), |s| lin(0.0, 24.0, s.makeup_db), |s, p| {
            s.makeup_db = inv_lin(0.0, 24.0, p);
        }, |s| format!("{:+.1} dB", s.makeup_db));
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
    state: Memo<CompressorState>,
    track_id: TrackId,
    label: &'static str,
    default_pos: f32,
    to_pos: impl Fn(CompressorState) -> f32 + Copy + 'static,
    apply: impl Fn(&mut CompressorState, f32) + Copy + 'static,
    format: impl Fn(CompressorState) -> String + Copy + 'static,
) {
    let pos = state.map(move |s| to_pos(*s));
    let text = state.map(move |s| format(*s));
    VStack::new(cx, move |cx| {
        Knob::plain(cx, pos, default_pos, theme, move |cx, p| {
            let mut updated = state.get();
            apply(&mut updated, p);
            cx.emit(TimelineEvent::SetCompressorState(track_id, updated));
        })
        .accent(compressor_accent)
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
