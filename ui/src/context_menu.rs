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

use shared::arrangement::{Arrangement, ClipContent, Instrument, TrackKind};

use crate::piano_roll::state::PianoRollEvent;
use crate::synth::state::SynthEvent;
use crate::timeline::state::{ContextMenu, ContextMenuTarget, TimelineEvent};
use crate::tokens;

/// One row: a label and an action, closing the menu after either way.
fn item(cx: &mut Context, label: &'static str, action: impl Fn(&mut EventContext) + Send + Sync + Copy + 'static) {
    HStack::new(cx, move |cx| {
        Label::new(cx, label).class("body");
    })
    .class("menu-item")
    .on_press(move |cx| {
        action(cx);
        cx.emit(TimelineEvent::CloseContextMenu);
    })
    .cursor(CursorIcon::Hand)
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Pixels(26.0));
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
                item(cx, "Cut", |cx| cx.emit(TimelineEvent::Cut));
                item(cx, "Copy", |cx| cx.emit(TimelineEvent::Copy));
                item(cx, "Duplicate", |cx| cx.emit(TimelineEvent::DuplicateSelected));
                if is_midi {
                    separator(cx);
                    item(cx, "Open in piano roll", move |cx| cx.emit(PianoRollEvent::Open(clip_id)));
                }
                separator(cx);
                item(cx, "Delete", |cx| cx.emit(TimelineEvent::DeleteSelected));
            }
            ContextMenuTarget::Track(track_id) => {
                let track = arr.track(track_id).cloned();
                let muted = track.as_ref().map(|t| t.mute).unwrap_or(false);
                let soloed = track.as_ref().map(|t| t.solo).unwrap_or(false);
                let is_midi = track.as_ref().map(|t| t.kind == TrackKind::Midi).unwrap_or(false);
                let has_instrument = track.as_ref().is_some_and(|t| t.instrument.is_some());

                item(cx, if muted { "Unmute" } else { "Mute" }, move |cx| cx.emit(TimelineEvent::ToggleMute(track_id)));
                item(cx, if soloed { "Unsolo" } else { "Solo" }, move |cx| cx.emit(TimelineEvent::ToggleSolo(track_id)));
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
                item(cx, "Paste", move |cx| {
                    cx.emit(TimelineEvent::ScrubPlayhead(tick));
                    cx.emit(TimelineEvent::Paste);
                });
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
        .padding_top(Pixels(tokens::SPACE_1))
        .padding_bottom(Pixels(tokens::SPACE_1))
        .width(Pixels(180.0))
        .height(Auto);
    });
}
