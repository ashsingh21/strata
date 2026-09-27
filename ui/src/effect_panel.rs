//! An effect node's device panel: an Enabled toggle and one knob per
//! parameter, for exactly the node it's given (not "the track's first
//! compressor"). The knobs come from `EffectParam`'s shared table - name,
//! normalized range and readout format - so the panel, the Effects Board
//! and automation lanes all agree on what a knob position means. The EQ
//! also shows its response curve.

use std::cell::Cell;

use vizia::prelude::*;

use shared::arrangement::{Arrangement, AutomationTarget, ClipColor, Effect, EffectNodeId, EffectParam, Ticks, TrackId};

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
    playhead: Signal<Ticks>,
) {
    TRACK_COLOR.set(track_color);

    let Some(initial) = arrangement.get().fx(track).and_then(|fx| fx.node(node)).map(|n| n.effect) else { return };
    // The node's live effect state; falls back to its last-seen value for
    // the frame in which the node is being removed.
    let stored = arrangement.map(move |arr| arr.fx(track).and_then(|fx| fx.node(node)).map(|n| n.effect).unwrap_or(initial));
    // What's heard at the playhead: `stored` with this node's lanes applied.
    // The knobs show this; edits still go to `stored`.
    let shown = Memo::new(move |_| {
        let mut effect = stored.get();
        if let Some(track) = track {
            let tick = playhead.get();
            for lane in arrangement.get().automation.iter().filter(|l| l.track == track) {
                if let (Some(AutomationTarget::Effect { node: n, param }), Some(v)) = (lane.target, lane.value_at(tick)) {
                    if n == node {
                        param.apply_norm(&mut effect, v);
                    }
                }
            }
        }
        effect
    });
    let enabled = arrangement.map(move |arr| arr.fx(track).and_then(|fx| fx.node(node)).map(|n| n.enabled).unwrap_or(true));

    Button::new(cx, |cx| Label::new(cx, "Enabled"))
        .class("btn")
        .class("sm")
        .toggle_class("is-on", enabled)
        .on_press(move |cx| cx.emit(TimelineEvent::ToggleEffectEnabled(track, node)));

    HStack::new(cx, move |cx| {
        if matches!(initial, Effect::Eq(_)) {
            let eq_state = shown.map(|e| match e {
                Effect::Eq(s) => *s,
                _ => shared::arrangement::EqState::default(),
            });
            crate::eq_curve::eq_curve(cx, eq_state, theme);
        }
        for &param in EffectParam::for_effect(initial) {
            param_knob(cx, theme, arrangement, stored, shown, track, node, param, initial);
        }
    })
    .class("device")
    .gap(Pixels(tokens::SPACE_4))
    .alignment(Alignment::Center)
    .padding(Pixels(tokens::SPACE_3))
    .width(Stretch(1.0))
    .height(Pixels(96.0));
}

#[allow(clippy::too_many_arguments)]
fn param_knob(
    cx: &mut Context,
    theme: Signal<ThemeId>,
    arrangement: Signal<Arrangement>,
    stored: Memo<Effect>,
    shown: Memo<Effect>,
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
    let pos = shown.map(move |e| param.norm(e).unwrap_or(0.0));
    let text = shown.map(move |e| param.format(e));
    // Automated: follows its lane and is read-only (a drag would only be
    // overridden by the lane).
    let target = AutomationTarget::Effect { node, param };
    let automated = arrangement.map(move |arr| {
        track.is_some_and(|t| arr.automation.iter().any(|l| l.track == t && l.target == Some(target)))
    });
    VStack::new(cx, move |cx| {
        Knob::plain(cx, pos, default_pos, theme, move |cx, p| {
            let mut updated = stored.get();
            param.apply_norm(&mut updated, p);
            cx.emit(TimelineEvent::SetEffectState(track, node, updated));
        })
        .accent(accent)
        .pointer_events(automated.map(|a| if *a { PointerEvents::None } else { PointerEvents::Auto }))
        .size(Pixels(tokens::SIZE_KNOB));
        Label::new(cx, param.name()).class("label");
        Label::new(cx, text).class("value");
    })
    .class("knob-col")
    .toggle_class("is-automated", automated)
    // Right-click: "Automate <param>". Track effects only - master-bus
    // automation isn't supported (lanes belong to tracks).
    .on_mouse_down(move |cx, button| {
        if let (MouseButton::Right, Some(track)) = (button, track) {
            let (x, y) = (cx.mouse().cursor_x, cx.mouse().cursor_y);
            cx.emit(TimelineEvent::OpenContextMenu(ContextMenu {
                target: ContextMenuTarget::Param { track, target: AutomationTarget::Effect { node, param }, current: None },
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
