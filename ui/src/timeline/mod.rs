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
use lanes::{LaneArea, PlayheadOverlay};
use ruler::Ruler;
use shared::arrangement::{Arrangement, SnapGrid, Ticks};
use state::{TimelineEvent, TimelineTool};

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
    tool: Signal<TimelineTool>,
) {
    HStack::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            HStack::new(cx, move |cx| {
                Label::new(cx, "Snap").class("label");
                let snap_label = snap.map(|s| s.label().to_string());
                Button::new(cx, move |cx| Label::new(cx, snap_label))
                    .class("readout")
                    .class("snap")
                    .on_press(|cx| cx.emit(TimelineEvent::CycleSnap));

                Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));

                // Select (rubber-band selection, drag clips) vs Draw
                // (click/drag empty space on a MIDI track to create a clip).
                // The same segmented control as every other mode switch.
                let tools = [TimelineTool::Select, TimelineTool::Draw];
                crate::synth::segmented::segmented(
                    cx,
                    2,
                    |cx, i| Label::new(cx, if i == 0 { "Select" } else { "Draw" }),
                    move |i| tool.map(move |t| *t == tools[i]),
                    move |cx, i| cx.emit(TimelineEvent::SetTool(tools[i])),
                );
            })
            .class("tl-corner")
            .alignment(Alignment::Left)
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

                // Quiet actions under the last track, not a toolbar.
                HStack::new(cx, move |cx| {
                    Button::new(cx, |cx| Label::new(cx, "+ Audio track"))
                        .class("btn")
                        .class("quiet")
                        .on_press(|cx| {
                            cx.emit(TimelineEvent::AddTrack(shared::arrangement::TrackKind::Audio))
                        });
                    Button::new(cx, |cx| Label::new(cx, "+ MIDI track"))
                        .class("btn")
                        .class("quiet")
                        .on_press(|cx| {
                            cx.emit(TimelineEvent::AddTrack(shared::arrangement::TrackKind::Midi))
                        });
                    Button::new(cx, |cx| Label::new(cx, "+ Drums"))
                        .class("btn")
                        .class("quiet")
                        .on_press(|cx| cx.emit(TimelineEvent::ToggleDrumsMenu));
                })
                .gap(Pixels(tokens::SPACE_1))
                .padding_left(Pixels(tokens::SPACE_1))
                .padding_top(Pixels(tokens::SPACE_1))
                .height(Pixels(tokens::SIZE_CONTROL + tokens::SPACE_2))
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

            // PlayheadOverlay stacked on LaneArea rather than drawn as
            // part of it, so moving the playhead during playback doesn't
            // force a full redraw of every clip/waveform/grid line every
            // frame - see PlayheadOverlay's own doc comment.
            ZStack::new(cx, move |cx| {
                LaneArea::new(cx, arrangement, transform, selection, playhead, theme, recording_preview, tool)
                    .position_type(PositionType::Absolute)
                    .height(Stretch(1.0))
                    .width(Stretch(1.0));

                PlayheadOverlay::new(cx, transform, playhead, theme)
                    .position_type(PositionType::Absolute)
                    .height(Stretch(1.0))
                    .width(Stretch(1.0));
            })
            .height(Stretch(1.0))
            .width(Stretch(1.0));
        })
        .width(Stretch(1.0))
        .height(Stretch(1.0));
    })
    .height(Stretch(1.0))
    .width(Stretch(1.0));
}

/// The "Drums" button's dropdown: every `.wav` under `assets/drums/`,
/// clicking one imports it as a new track. See
/// `TimelineEvent::AddDrumSample`'s own doc comment for what that does.
///
/// This is a separate top-level overlay rather than nested inside the
/// cramped 240px header column, for the same reason Interval Input's own
/// overlay is: Absolute positioning nested inside a narrow fixed-width
/// parent produced unexplained offsets earlier in this project (see
/// `timeline_view`'s layout comment), so floating panels here stay
/// top-level instead.
/// The drum sample file names in `assets/drums/`, sorted.
pub fn drum_samples() -> Vec<String> {
    let mut samples: Vec<String> = std::fs::read_dir(assets_dir().join("drums"))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|e| {
                    let path = e.path();
                    (path.extension().and_then(|s| s.to_str()) == Some("wav"))
                        .then(|| path.file_name().and_then(|s| s.to_str()).map(str::to_string))
                        .flatten()
                })
                .collect()
        })
        .unwrap_or_default();
    samples.sort();
    samples
}

pub fn drums_menu_view(cx: &mut Context, open: Signal<bool>) {
    let samples = drum_samples();

    VStack::new(cx, move |cx| {
        Label::new(cx, "Drum samples").class("label");
        if samples.is_empty() {
            Label::new(cx, "none in assets/drums/").class("meta");
        }
        for filename in &samples {
            let source: std::sync::Arc<str> = format!("drums/{filename}").into();
            let display = state::display_name_from_stem(filename.strip_suffix(".wav").unwrap_or(filename));
            Button::new(cx, move |cx| Label::new(cx, display.clone()))
                .class("btn")
                .class("sm")
                .width(Stretch(1.0))
                .on_press(move |cx| cx.emit(TimelineEvent::AddDrumSample(source.clone())));
        }
    })
    .class("panel")
    .class("drums-menu")
    .toggle_class("hidden", open.map(|o| !*o))
    .gap(Pixels(tokens::SPACE_1))
    .padding(Pixels(tokens::SPACE_2))
    .position_type(PositionType::Absolute)
    // Just right of the sidebar, under the header and ruler.
    .top(Pixels(tokens::SIZE_TOOLBAR + tokens::SIZE_RULER + 8.0))
    .left(Pixels(216.0))
    .width(Pixels(160.0))
    .height(Auto);
}
