//! The timeline's right-click context menu: a small floating list of
//! actions for whatever was clicked (a clip, a track header, or empty
//! track space), positioned at the click. A transparent backdrop behind
//! it closes it on any click elsewhere, the same convention as every
//! other overlay in the app (`drums_menu_view`, Interval Input, the piano
//! roll's own backdrop).
//!
//! Every clip action reuses the timeline's existing Cut/Copy/Duplicate/
//! Delete events rather than adding new ones: right-clicking a clip first
//! selects it (unless it's already part of a bigger selection), so those
//! events already do the right thing.

use vizia::prelude::*;

use shared::arrangement::{Arrangement, AutomationTarget, ClipContent, Instrument, TrackKind};

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

pub fn context_menu_view(
    cx: &mut Context,
    arrangement: Signal<Arrangement>,
    menu: Signal<Option<ContextMenu>>,
    clipboard_nonempty: Signal<bool>,
) {
    Element::new(cx)
        .class("context-menu-backdrop")
        .toggle_class("hidden", menu.map(|m| m.is_none()))
        .on_mouse_down(|cx, _| cx.emit(TimelineEvent::CloseContextMenu))
        .position_type(PositionType::Absolute)
        .top(Pixels(0.0))
        .left(Pixels(0.0))
        .width(Stretch(1.0))
        .height(Stretch(1.0));

    Binding::new(cx, menu, move |cx| {
        let Some(m) = menu.get() else { return };
        let arr = arrangement.get();

        VStack::new(cx, move |cx| match m.target {
            ContextMenuTarget::Clip(clip_id) => {
                let is_midi = arr.clip(clip_id).map(|c| matches!(c.content, ClipContent::Midi { .. })).unwrap_or(false);
                item_with_shortcut(cx, "Cut", "Ctrl+X", |cx| cx.emit(TimelineEvent::Cut));
                item_with_shortcut(cx, "Copy", "Ctrl+C", |cx| cx.emit(TimelineEvent::Copy));
                item_with_shortcut(cx, "Duplicate", "Ctrl+D", |cx| cx.emit(TimelineEvent::DuplicateSelected));
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
                        cx.emit(TimelineEvent::AutomateParam { track: track_id, target: AutomationTarget::TrackGain })
                    });
                }
                if is_midi {
                    separator(cx);
                    if has_instrument {
                        item(cx, "Remove instrument", move |cx| {
                            cx.emit(TimelineEvent::SetInstrument { track: track_id, instrument: None })
                        });
                    } else {
                        item(cx, "Add Carve", move |cx| {
                            cx.emit(TimelineEvent::SetInstrument { track: track_id, instrument: Some(Instrument::Carve) });
                            cx.emit(SynthEvent::SelectTrack(track_id));
                        });
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
            ContextMenuTarget::Param { track, target } => {
                let already = arr.automation.iter().any(|l| l.track == track && l.target == Some(target));
                let name = arr.target_label(track, target).unwrap_or_default();
                let label = if already { format!("{name} is automated") } else { format!("Automate {name}") };
                item(cx, label, move |cx| cx.emit(TimelineEvent::AutomateParam { track, target }));
            }
            ContextMenuTarget::AutomationLane { lane } => {
                item(cx, "Remove automation lane", move |cx| cx.emit(TimelineEvent::RemoveAutomationLane(lane)));
            }
        })
        .class("panel")
        .class("context-menu")
        // A Lane menu with nothing to paste has no rows at all; hide the
        // empty panel rather than show an empty floating box.
        .toggle_class(
            "hidden",
            Memo::new(move |_| {
                matches!(m.target, ContextMenuTarget::Lane { .. }) && !clipboard_nonempty.get()
            }),
        )
        .position_type(PositionType::Absolute)
        .left(Pixels(m.x))
        .top(Pixels(m.y))
        .gap(Pixels(2.0))
        .padding_top(Pixels(tokens::SPACE_2))
        .padding_bottom(Pixels(tokens::SPACE_2))
        // Left/right margin so an item's hover highlight sits inset from
        // the panel's own border rather than running flush into it.
        .padding_left(Pixels(tokens::SPACE_1))
        .padding_right(Pixels(tokens::SPACE_1))
        .width(Pixels(212.0))
        .height(Auto);
    });
}
