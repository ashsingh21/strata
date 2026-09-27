//! An effect node's device panel: an Enabled toggle and one knob per
//! parameter, for exactly the node it's given (not "the track's first
//! compressor"). The knobs come from `EffectParam`'s shared table - name,
//! normalized range and readout format - so the panel, the Effects Board
//! and automation lanes all agree on what a knob position means. The EQ
//! also shows its response curve.

use std::cell::Cell;

use vizia::prelude::*;

use shared::arrangement::{Arrangement, AutomationTarget, ClipColor, Effect, EffectNodeId, EffectParam, TrackId};

use crate::knob::{Knob, KnobAccentExt};
use crate::timeline::state::{ContextMenu, ContextMenuTarget, TimelineEvent};
use crate::tokens::{self, Palette, ThemeId};

thread_local! {
    /// Set at the top of `effect_panel`, read by `accent` - `Knob`'s accent
    /// is a plain `fn(&Palette) -> Color`, so it can't capture the colour.
    static TRACK_COLOR: Cell<ClipColor> = const { Cell::new(ClipColor::Violet) };
}

fn accent(p: &Palette) -> Color {
    match TRACK_COLOR.get() {
        ClipColor::Coral => p.clip_coral_line,
        ClipColor::Amber => p.clip_amber_line,
        ClipColor::Teal => p.clip_teal_line,
        ClipColor::Blue => p.clip_blue_line,
        ClipColor::Violet => p.clip_violet_line,
        ClipColor::Pink => p.clip_pink_line,
    }
}

/// `track` is `None` for the master bus (same convention as `Arrangement::fx`).
pub fn effect_panel(
    cx: &mut Context,
    theme: Signal<ThemeId>,
    arrangement: Signal<Arrangement>,
    track: Option<TrackId>,
    node: EffectNodeId,
    track_color: ClipColor,
) {
    TRACK_COLOR.set(track_color);

    let Some(initial) = arrangement.get().fx(track).and_then(|fx| fx.node(node)).map(|n| n.effect) else { return };
    // The node's live effect state; falls back to its last-seen value for
    // the frame in which the node is being removed.
    let effect = arrangement.map(move |arr| arr.fx(track).and_then(|fx| fx.node(node)).map(|n| n.effect).unwrap_or(initial));
    let enabled = arrangement.map(move |arr| arr.fx(track).and_then(|fx| fx.node(node)).map(|n| n.enabled).unwrap_or(true));

    Button::new(cx, |cx| Label::new(cx, "Enabled"))
        .class("btn")
        .class("sm")
        .toggle_class("is-on", enabled)
        .on_press(move |cx| cx.emit(TimelineEvent::ToggleEffectEnabled(track, node)));

    HStack::new(cx, move |cx| {
        if matches!(initial, Effect::Eq(_)) {
            let eq_state = effect.map(|e| match e {
                Effect::Eq(s) => *s,
                _ => shared::arrangement::EqState::default(),
            });
            crate::eq_curve::eq_curve(cx, eq_state, theme);
        }
        for &param in EffectParam::for_effect(initial) {
            param_knob(cx, theme, effect, track, node, param, initial);
        }
    })
    .class("device")
    .gap(Pixels(tokens::SPACE_4))
    .alignment(Alignment::Center)
    .padding(Pixels(tokens::SPACE_3))
    .width(Stretch(1.0))
    .height(Pixels(96.0));
}

fn param_knob(
    cx: &mut Context,
    theme: Signal<ThemeId>,
    effect: Memo<Effect>,
    track: Option<TrackId>,
    node: EffectNodeId,
    param: EffectParam,
    kind: Effect,
) {
    // Double-click resets to the effect type's default value.
    let default_effect = match kind {
        Effect::Compressor(_) => Effect::Compressor(Default::default()),
        Effect::Eq(_) => Effect::Eq(Default::default()),
    };
    let default_pos = param.norm(&default_effect).unwrap_or(0.0);
    let pos = effect.map(move |e| param.norm(e).unwrap_or(0.0));
    let text = effect.map(move |e| param.format(e));
    VStack::new(cx, move |cx| {
        Knob::plain(cx, pos, default_pos, theme, move |cx, p| {
            let mut updated = effect.get();
            param.apply_norm(&mut updated, p);
            cx.emit(TimelineEvent::SetEffectState(track, node, updated));
        })
        .accent(accent)
        .size(Pixels(tokens::SIZE_KNOB));
        Label::new(cx, param.name()).class("label");
        Label::new(cx, text).class("value");
    })
    .class("knob-col")
    // Right-click: "Automate <param>". Track effects only - master-bus
    // automation isn't supported (lanes belong to tracks).
    .on_mouse_down(move |cx, button| {
        if let (MouseButton::Right, Some(track)) = (button, track) {
            let (x, y) = (cx.mouse().cursor_x, cx.mouse().cursor_y);
            cx.emit(TimelineEvent::OpenContextMenu(ContextMenu {
                target: ContextMenuTarget::Param { track, target: AutomationTarget::Effect { node, param } },
                x,
                y,
            }));
        }
    })
    .alignment(Alignment::Center)
    .gap(Pixels(2.0))
    .width(Auto)
    .height(Auto);
}
