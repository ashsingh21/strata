//! The timeline's right-click context menu: a small floating list of
//! actions for whatever was clicked (a clip, a track header, or empty
//! track space), opened at the click. It's one Vizia dropdown at the
//! window root, its zero-size anchor moved to the click: it opens there,
//! moves to stay inside the window, and closes on a click elsewhere or
//! Escape. `TimelineState` keeps what was clicked and opens it.
//!
//! Every clip action reuses the timeline's existing Cut/Copy/Duplicate/
//! Delete events rather than adding new ones: right-clicking a clip first
//! selects it (unless it's already part of a bigger selection), so those
//! events already do the right thing.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use vizia::prelude::*;

use shared::arrangement::{Arrangement, AutomationTarget, ClipContent, EffectParam, Instrument, TrackId, TrackKind};
use shared::synth::{SynthParam, SynthState};

use crate::piano_roll::state::PianoRollEvent;
use crate::synth::state::SynthEvent;
use crate::timeline::state::{ContextMenu, ContextMenuTarget, TimelineEvent};
use crate::tokens;

/// One row: a label, an optional right-aligned shortcut hint (muted, like
/// a native menu's), and an action - closing the menu after either way.
fn item(cx: &mut Context, label: impl Into<String>, action: impl Fn(&mut EventContext) + Send + Sync + Copy + 'static) {
    item_with_shortcut(cx, label, "", action);
}

fn item_with_shortcut(
    cx: &mut Context,
    label: impl Into<String>,
    shortcut: &'static str,
    action: impl Fn(&mut EventContext) + Send + Sync + Copy + 'static,
) {
    let label: String = label.into();
    HStack::new(cx, move |cx| {
        // Children aren't hit-testable (same as Vizia's own Button does to
        // its content): `on_press` only fires when the press targets the row
        // itself, so a hoverable label made clicks on the text do nothing.
        Label::new(cx, label).class("body").hoverable(false);
        Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0)).hoverable(false);
        let shortcut = crate::shortcut(shortcut);
        if !shortcut.is_empty() {
            Label::new(cx, shortcut).class("value").hoverable(false);
        }
    })
    .class("menu-item")
    .on_press(move |cx| {
        action(cx);
        cx.emit(TimelineEvent::CloseContextMenu);
    })
    .cursor(CursorIcon::Hand)
    .gap(Pixels(tokens::SPACE_3))
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Pixels(28.0));
}

fn separator(cx: &mut Context) {
    Element::new(cx).class("menu-sep").width(Stretch(1.0)).height(Pixels(1.0));
}

thread_local! {
    /// The menu's dropdown, for `open` and `close`.
    static HOST: Cell<Option<Entity>> = const { Cell::new(None) };
    /// The knob each track last had turned: its Automate menu offers it
    /// first, so "turn the knob, press A" works.
    static LAST_TOUCHED: RefCell<HashMap<TrackId, AutomationTarget>> = RefCell::new(HashMap::new());
}

/// Remembers `target` as the last knob turned on `track`.
pub fn touched(track: TrackId, target: AutomationTarget) {
    LAST_TOUCHED.with(|m| m.borrow_mut().insert(track, target));
}

/// The Carve knobs the Automate menu lists - the ones worth moving over a
/// song. Any other knob: turn it, and it's offered first; or right-click it.
const CARVE_AUTOMATABLE: [SynthParam; 8] = [
    SynthParam::Cutoff,
    SynthParam::Resonance,
    SynthParam::Drive,
    SynthParam::EnvAmount,
    SynthParam::ReverbMix,
    SynthParam::ChorusMix,
    SynthParam::Lfo1Depth,
    SynthParam::Volume,
];

/// One column of the Automate menu (the menu's usual width, less padding).
const COLUMN_W: f32 = 204.0;

/// Opens the menu at the click `TimelineState` just recorded.
pub fn open(cx: &mut EventContext) {
    if let Some(host) = HOST.get() {
        cx.emit_to(host, PopupEvent::Open);
    }
}

pub fn close(cx: &mut EventContext) {
    if let Some(host) = HOST.get() {
        cx.emit_to(host, PopupEvent::Close);
    }
}

/// Mounted once, at the window root, after everything it opens over.
pub fn context_menu_view(cx: &mut Context, arrangement: Signal<Arrangement>, menu: Signal<Option<ContextMenu>>, synth: Signal<SynthState>) {
    let at = menu.map(|m| m.map(|m| (m.x, m.y)).unwrap_or((0.0, 0.0)));
    let host = crate::menu::menu(
        cx,
        Placement::BottomStart,
        |cx| {
            Element::new(cx).width(Stretch(1.0)).height(Stretch(1.0));
        },
        move |cx| items(cx, arrangement, menu, synth),
    )
    .position_type(PositionType::Absolute)
    .left(at.map(|a| Pixels(a.0)))
    .top(at.map(|a| Pixels(a.1)))
    .width(Pixels(0.0))
    .height(Pixels(0.0))
    .entity();
    HOST.set(Some(host));
}

/// The rows for whatever was clicked, built as the menu opens.
fn items(cx: &mut Context, arrangement: Signal<Arrangement>, menu: Signal<Option<ContextMenu>>, synth: Signal<SynthState>) {
    {
        let Some(m) = menu.get() else { return };
        let arr = arrangement.get();
        let two_columns = match m.target {
            ContextMenuTarget::Automate { track } => arr.track(track).is_some_and(|t| !t.fx.ordered().is_empty()),
            _ => false,
        };

        VStack::new(cx, move |cx| match m.target {
            ContextMenuTarget::Clip(clip_id) => {
                let is_midi = arr.clip(clip_id).map(|c| matches!(c.content, ClipContent::Midi { .. })).unwrap_or(false);
                let is_linked = arr.clip(clip_id).is_some_and(|c| c.link().is_some());
                item(cx, "Rename...", move |cx| cx.emit(TimelineEvent::BeginRenameClip { clip: clip_id, x: m.x, y: m.y }));
                separator(cx);
                item_with_shortcut(cx, "Cut", "Ctrl+X", |cx| cx.emit(TimelineEvent::Cut));
                item_with_shortcut(cx, "Copy", "Ctrl+C", |cx| cx.emit(TimelineEvent::Copy));
                item_with_shortcut(cx, "Duplicate", "Ctrl+D", |cx| cx.emit(TimelineEvent::DuplicateSelected));
                if is_midi {
                    item_with_shortcut(cx, "Duplicate linked", "Ctrl+Shift+D", |cx| cx.emit(TimelineEvent::DuplicateLinked));
                }
                if is_linked {
                    item(cx, "Unlink", |cx| cx.emit(TimelineEvent::UnlinkSelected));
                }
                if arr.loop_range.is_some() {
                    item(cx, "Repeat to fill loop", |cx| cx.emit(TimelineEvent::RepeatToFillLoop));
                }
                if is_midi {
                    separator(cx);
                    item(cx, "Open in piano roll", move |cx| cx.emit(PianoRollEvent::Open(clip_id)));
                }
                separator(cx);
                item_with_shortcut(cx, "Delete", "Del", |cx| cx.emit(TimelineEvent::DeleteSelected));
            }
            ContextMenuTarget::Track(track_id) => {
                let track = arr.track(track_id).cloned();
                let muted = track.as_ref().map(|t| t.mute).unwrap_or(false);
                let soloed = track.as_ref().map(|t| t.solo).unwrap_or(false);
                let is_midi = track.as_ref().map(|t| t.kind == TrackKind::Midi).unwrap_or(false);
                let has_instrument = track.as_ref().is_some_and(|t| t.instrument.is_some());

                item(cx, "Rename...", move |cx| cx.emit(TimelineEvent::BeginRenameTrack(track_id)));
                separator(cx);
                item(cx, if muted { "Unmute" } else { "Mute" }, move |cx| cx.emit(TimelineEvent::ToggleMute(track_id)));
                item(cx, if soloed { "Unsolo" } else { "Solo" }, move |cx| cx.emit(TimelineEvent::ToggleSolo(track_id)));
                let gain_automated = arr
                    .automation
                    .iter()
                    .any(|l| l.track == track_id && l.target == Some(AutomationTarget::TrackGain));
                if !gain_automated {
                    item(cx, "Automate gain", move |cx| {
                        cx.emit(TimelineEvent::AutomateParam { track: track_id, target: AutomationTarget::TrackGain, current: None })
                    });
                }
                if is_midi {
                    separator(cx);
                    if has_instrument {
                        item(cx, "Remove instrument", move |cx| {
                            cx.emit(TimelineEvent::SetInstrument { track: track_id, instrument: None })
                        });
                    } else {
                        for instrument in [Instrument::Carve, Instrument::Drums] {
                            let label = if instrument == Instrument::Carve { "Add Carve" } else { "Add Drum Kit" };
                            item(cx, label, move |cx| {
                                cx.emit(TimelineEvent::SetInstrument { track: track_id, instrument: Some(instrument) });
                                cx.emit(SynthEvent::SelectTrack(track_id));
                            });
                        }
                    }
                }
                separator(cx);
                item(cx, "Remove track", move |cx| cx.emit(TimelineEvent::RemoveTrack(track_id)));
            }
            ContextMenuTarget::Lane { tick, .. } => {
                item_with_shortcut(cx, "Paste", "Ctrl+V", move |cx| {
                    cx.emit(TimelineEvent::ScrubPlayhead(tick));
                    cx.emit(TimelineEvent::Paste);
                });
            }
            ContextMenuTarget::Ruler { tick } => {
                item(cx, "Add marker here", move |cx| cx.emit(TimelineEvent::AddMarker(tick)));
                if arr.loop_range.is_some() {
                    separator(cx);
                    item(cx, "Remove loop", |cx| cx.emit(TimelineEvent::SetLoopRange(None)));
                }
            }
            ContextMenuTarget::Marker { marker } => {
                item(cx, "Rename...", move |cx| cx.emit(TimelineEvent::BeginRenameMarker(marker)));
                separator(cx);
                item(cx, "Delete", move |cx| cx.emit(TimelineEvent::DeleteMarker(marker)));
            }
            ContextMenuTarget::Param { track, target, current } => {
                let already = arr.automation.iter().any(|l| l.track == track && l.target == Some(target));
                let name = arr.target_label(track, target).unwrap_or_default();
                let label = if already { format!("{name} is automated") } else { format!("Automate {name}") };
                item(cx, label, move |cx| cx.emit(TimelineEvent::AutomateParam { track, target, current }));
            }
            ContextMenuTarget::AutomationLane { lane } => {
                item(cx, "Remove automation lane", move |cx| cx.emit(TimelineEvent::RemoveAutomationLane(lane)));
            }
            ContextMenuTarget::Automate { track } => automate_items(cx, &arr, track, &synth.get()),
        })
        .class("panel")
        .class("context-menu")
        .gap(Pixels(2.0))
        .padding_top(Pixels(tokens::SPACE_2))
        .padding_bottom(Pixels(tokens::SPACE_2))
        // Left/right margin so an item's hover highlight sits inset from
        // the panel's own border rather than running flush into it.
        .padding_left(Pixels(tokens::SPACE_1))
        .padding_right(Pixels(tokens::SPACE_1))
        // The Automate menu is two columns wide when the track has effects.
        .width(Pixels(if two_columns { 2.0 * COLUMN_W + 10.0 } else { 212.0 }))
        .height(Auto);
    }
}

/// A track's Automate menu: the knob turned last, then gain, Carve's main
/// knobs and every effect's. Picking one adds its lane (already automated
/// ones are ticked and do nothing).
fn automate_items(cx: &mut Context, arr: &Arrangement, track: TrackId, patch: &SynthState) {
    let Some(t) = arr.track(track) else { return };
    let carve = t.instrument == Some(Instrument::Carve);
    // The patch on screen is the selected track's - pressing A selects it.
    let current = move |target: AutomationTarget| match target {
        AutomationTarget::Synth(param) => Some(param.norm(patch)),
        _ => None,
    };
    // Gain and Carve on the left, the effects' knobs beside them - one
    // tall list ran off the window, and rows inside a ScrollView never got
    // their clicks.
    let mut own = vec![AutomationTarget::TrackGain];
    if carve {
        own.extend(CARVE_AUTOMATABLE.map(AutomationTarget::Synth));
    }
    let mut effects = Vec::new();
    for node in t.fx.ordered() {
        effects.extend(EffectParam::for_effect(node.effect).iter().map(|&param| AutomationTarget::Effect { node: node.id, param }));
    }
    let last = LAST_TOUCHED.with(|m| m.borrow().get(&track).copied()).filter(|&l| arr.target_label(track, l).is_some());

    // (label, target, value) per row.
    let row = |target: AutomationTarget, prefix: &str| {
        let name = arr.target_label(track, target)?;
        let automated = arr.automation.iter().any(|l| l.track == track && l.target == Some(target));
        let label = if automated { format!("\u{2713} {name}") } else { format!("{prefix}{name}") };
        Some((label, target, current(target)))
    };
    let add = move |cx: &mut Context, (label, target, value): (String, AutomationTarget, Option<f32>)| {
        item(cx, label, move |cx| cx.emit(TimelineEvent::AutomateParam { track, target, current: value }));
    };

    Label::new(cx, "Automate").class("label").padding_left(Pixels(tokens::SPACE_2));
    if let Some(last) = last.and_then(|l| row(l, "Last turned: ")) {
        add(cx, last);
        separator(cx);
    }
    let own: Vec<_> = own.into_iter().filter_map(|t| row(t, "")).collect();
    let effects: Vec<_> = effects.into_iter().filter_map(|t| row(t, "")).collect();
    HStack::new(cx, move |cx| {
        for column in [own.clone(), effects.clone()] {
            if column.is_empty() {
                continue;
            }
            VStack::new(cx, move |cx| {
                for r in column.clone() {
                    add(cx, r);
                }
            })
            .gap(Pixels(2.0))
            .width(Pixels(COLUMN_W))
            .height(Auto);
        }
    })
    .gap(Pixels(2.0))
    .width(Auto)
    .height(Auto);
    Label::new(cx, "Or right-click any knob.").class("meta").padding_left(Pixels(tokens::SPACE_2));
}
