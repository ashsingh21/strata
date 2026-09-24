//! The arrangement timeline: a track-header column, a ruler and a lane
//! area, backed by [`state::TimelineState`].

pub mod header;
pub mod lanes;
pub mod peaks_loader;
pub mod ruler;
pub mod scheduler;
pub mod state;

use std::path::PathBuf;

use vizia::prelude::*;

use header::{automation_header, track_header};
use lanes::LaneArea;
use ruler::Ruler;
use shared::arrangement::{Arrangement, SnapGrid, Ticks};
use state::TimelineEvent;

use crate::timeline::state::Selection;
use crate::tokens::{self, ThemeId};

pub fn assets_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets"))
}

/// Wider than the `size-track-head` token (176px): more breathing room for
/// track names, kind labels and the automation parameter name than the
/// spec's compact value gives at this window size.
pub const HEAD_WIDTH: f32 = 240.0;

/// Wider than the `size-lane`/`size-lane-auto` tokens (56px/40px): taller
/// rows so waveforms, MIDI notes and automation curves have more room.
pub const LANE_HEIGHT: f32 = 96.0;
pub const LANE_AUTO_HEIGHT: f32 = 64.0;

#[allow(clippy::too_many_arguments)]
pub fn timeline_view(
    cx: &mut Context,
    theme: Signal<ThemeId>,
    arrangement: Signal<Arrangement>,
    transform: Signal<shared::arrangement::ViewTransform>,
    snap: Signal<SnapGrid>,
    selection: Signal<Selection>,
    playhead: Signal<Ticks>,
    recording_preview: Signal<Option<crate::recorder::RecordingPreview>>,
) {
    HStack::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            HStack::new(cx, move |cx| {
                Label::new(cx, "Snap").class("label");
                let snap_label = snap.map(|s| s.label().to_string());
                Button::new(cx, move |cx| Label::new(cx, snap_label))
                    .class("readout")
                    .on_press(|cx| cx.emit(TimelineEvent::CycleSnap));
                Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
                Label::new(cx, "16 bars").class("meta");
            })
            .class("tl-corner")
            .gap(Pixels(tokens::SPACE_2))
            .width(Pixels(HEAD_WIDTH))
            .height(Pixels(tokens::SIZE_RULER));

            // Plain top-to-bottom relative flow, explicit fixed heights
            // throughout, no Stretch/Absolute anywhere in this column: two
            // earlier attempts using Stretch(1.0) containers and Absolute
            // positioning both produced large, unexplained vertical offsets
            // (Vizia stack layout quirk this session couldn't pin down).
            // This trades "fills the window" for "is definitely correct".
            VStack::new(cx, move |cx| {
                // A plain loop only builds this once, at startup: it
                // won't pick up a track being added or removed later.
                // `Binding` rebuilds its contents whenever `arrangement`
                // changes, which - unlike a live clip drag, which stays
                // canvas-local until it commits - is exactly as often as
                // the track list itself can actually change.
                Binding::new(cx, arrangement, move |cx| {
                    let arr = arrangement.get();
                    for track in arr.tracks.clone() {
                        track_header(cx, arrangement, theme, track.id);
                        for lane in arr.automation.iter().filter(|a| a.track == track.id) {
                            automation_header(cx, arrangement, lane.id);
                        }
                    }
                });

                HStack::new(cx, move |cx| {
                    Button::new(cx, |cx| Label::new(cx, "+ Audio"))
                        .class("btn")
                        .class("sm")
                        .on_press(|cx| {
                            cx.emit(TimelineEvent::AddTrack(shared::arrangement::TrackKind::Audio))
                        });
                    Button::new(cx, |cx| Label::new(cx, "+ MIDI"))
                        .class("btn")
                        .class("sm")
                        .on_press(|cx| {
                            cx.emit(TimelineEvent::AddTrack(shared::arrangement::TrackKind::Midi))
                        });
                })
                .gap(Pixels(tokens::SPACE_2))
                .padding(Pixels(tokens::SPACE_2))
                .height(Auto)
                .width(Pixels(HEAD_WIDTH));
            })
            .class("tl-heads")
            .width(Pixels(HEAD_WIDTH))
            .height(Auto);
        })
        .width(Pixels(HEAD_WIDTH))
        .height(Auto);

        VStack::new(cx, move |cx| {
            Ruler::new(cx, arrangement, transform, playhead, theme)
                .height(Pixels(tokens::SIZE_RULER))
                .width(Stretch(1.0));

            LaneArea::new(cx, arrangement, transform, selection, playhead, theme, recording_preview)
                .height(Stretch(1.0))
                .width(Stretch(1.0));
        })
        .width(Stretch(1.0))
        .height(Stretch(1.0));
    })
    .height(Stretch(1.0))
    .width(Stretch(1.0));
}
