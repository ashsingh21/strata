//! The arrangement timeline: a track-header column, a ruler and a lane
//! area, backed by [`state::TimelineState`].

pub mod beat_templates;
pub mod drum_pads;
pub mod header;
pub mod lanes;
pub mod peaks_loader;
pub mod ruler;
pub mod scheduler;
pub mod state;

use crate::lessons::LessonTargetExt;
use std::path::PathBuf;

use vizia::prelude::*;

use header::{automation_header, fx_pip_button, track_header};
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

/// Wider than the `size-lane-auto` token (40px): taller so automation
/// curves have more room. Track lanes themselves are `Track::height`
/// (default `shared::arrangement::DEFAULT_TRACK_HEIGHT`, also wider than
/// `size-lane`'s 56px) - resizable per track, unlike this one.
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
    live_peaks: Signal<std::sync::Arc<[f32]>>,
    missing_sources: Signal<std::collections::HashSet<std::sync::Arc<str>>>,
    tool: Signal<TimelineTool>,
    selected_track: Signal<Option<shared::arrangement::TrackId>>,
    loop_on: Signal<bool>,
    renaming_marker: Signal<Option<shared::arrangement::MarkerId>>,
    renaming_track: Signal<Option<shared::arrangement::TrackId>>,
    board_open_track: Signal<Option<Option<shared::arrangement::TrackId>>>,
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
            .padding_left(Pixels(tokens::SPACE_3))
            .alignment(Alignment::Left)
            .gap(Pixels(tokens::SPACE_2))
            .width(Pixels(HEAD_WIDTH))
            .height(Pixels(tokens::SIZE_RULER));

            // The headers scroll with the lanes: a clipping viewport the
            // lanes' height, with the header stack inside it offset by the
            // lanes' vertical scroll. The wheel over the headers scrolls too.
            WheelScroll {}.build(cx, move |cx| {
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
                        track_header(cx, arrangement, theme, selected_track, renaming_track, track.id, board_open_track, playhead);
                        for lane in arr.automation.iter().filter(|a| a.track == track.id) {
                            automation_header(cx, arrangement, playhead, lane.id);
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
                        .lesson_target(crate::lessons::Target::AddMidiTrack)
                        .on_press(|cx| {
                            cx.emit(TimelineEvent::AddTrack(shared::arrangement::TrackKind::Midi))
                        });
                    // A MIDI track playing the Drum Kit (one-shot samples
                    // are in the sidebar's Drum samples).
                    Button::new(cx, |cx| Label::new(cx, "+ Drums"))
                        .class("btn")
                        .class("quiet")
                        .lesson_target(crate::lessons::Target::AddDrumTrack)
                        .on_press(|cx| cx.emit(TimelineEvent::AddDrumTrack));
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
            // Vizia's own scroll offset (what ScrollView uses): shifts the
            // headers and their hit-testing together.
            .vertical_scroll(transform.map(|t| -(t.scroll_y as f32)))
            .overflow(Overflow::Hidden)
            .width(Pixels(HEAD_WIDTH))
            .height(Stretch(1.0));

            // The master row, pinned under the track list (outside the
            // scroll viewport above, so it's always visible) - same
            // "board open" mechanism as a track's own FX pip, scoped to
            // the master bus (`board_open_track`'s outer `None` means
            // closed, `Some(None)` means master's board is open).
            HStack::new(cx, move |cx| {
                Label::new(cx, "Master").class("title");
                Label::new(cx, "Main out").class("meta");
                Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
                // Same TrackHeaderFx control a track's own header gets
                // (pips per effect, Alt-click bypass-all) - was a plain
                // toggle with no at-a-glance sense of what's on the
                // master chain.
                fx_pip_button(cx, arrangement, theme, None, board_open_track);
            })
            .class("transport")
            .gap(Pixels(tokens::SPACE_2))
            .padding(Pixels(tokens::SPACE_2))
            .alignment(Alignment::Left)
            .width(Pixels(HEAD_WIDTH))
            .height(Pixels(tokens::SIZE_TOOLBAR));
        })
        .width(Pixels(HEAD_WIDTH))
        .height(Stretch(1.0));

        ZStack::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                Ruler::new(cx, arrangement, transform, playhead, theme, loop_on)
                    .height(Pixels(tokens::SIZE_RULER))
                    .width(Stretch(1.0))
                    .tooltip(|cx| {
                        // One action per line: a wrapped paragraph outgrew
                        // the box (Vizia sizes it before wrapping).
                        Tooltip::new(cx, |cx| {
                            VStack::new(cx, |cx| {
                                for line in [
                                    "Click: move the playhead",
                                    "Drag: set a loop region",
                                    "Click the loop bar: loop on/off",
                                    "Right-click: add a marker or remove the loop",
                                ] {
                                    Label::new(cx, line);
                                }
                            })
                            .gap(Pixels(2.0))
                            .size(Auto);
                        })
                        .placement(Placement::Bottom)
                        .arrow(false)
                    });

                // PlayheadOverlay stacked on LaneArea rather than drawn as
                // part of it, so moving the playhead during playback doesn't
                // force a full redraw of every clip/waveform/grid line every
                // frame - see PlayheadOverlay's own doc comment.
                ZStack::new(cx, move |cx| {
                    LaneArea::new(cx, arrangement, transform, selection, playhead, theme, recording_preview, live_peaks, tool, missing_sources)
                        // A browser result dropped on the timeline: the
                        // lanes work out which track and when.
                        .on_drop(|cx, _| {
                            if let Some(item) = crate::browser::view::dragged() {
                                let lanes = cx.current();
                                cx.emit_to(lanes, lanes::LaneDrop(item.id));
                            }
                        })
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

            // The marker rename textbox: a real widget positioned over the
            // marker's own tab (drawn on the ruler's canvas, which can't
            // host a widget itself), rather than a second way to edit its
            // name. A sibling of the whole ruler+lanes stack (not nested
            // with the ruler itself) so the ruler's own layout, and its
            // mouse hit-testing, is untouched by this being here at all.
            Binding::new(cx, renaming_marker, move |cx| {
                let Some(marker_id) = renaming_marker.get() else { return };
                let arr = arrangement.get();
                let Some(marker) = arr.markers.iter().find(|m| m.id == marker_id) else { return };
                let x = transform.get().tick_to_x(marker.position) as f32;
                let draft: Signal<String> = Signal::new(marker.name.clone());
                Textbox::new(cx, draft)
                    .class("search")
                    .font_size(11.0)
                    .on_edit(move |_cx, text| draft.set(text))
                    .on_submit(move |cx, text, _from_key| cx.emit(TimelineEvent::CommitRenameMarker(marker_id, text)))
                    .on_cancel(move |cx| cx.emit(TimelineEvent::CancelRenameMarker))
                    .position_type(PositionType::Absolute)
                    .left(Pixels(x))
                    .top(Pixels(2.0))
                    .width(Pixels(100.0))
                    .height(Pixels(18.0));
            });
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
/// Passes wheel/trackpad scrolling over the track headers on to the
/// timeline, so the whole arrangement scrolls wherever the pointer is.
struct WheelScroll {}

impl View for WheelScroll {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| {
            if let WindowEvent::MouseScroll(x, y) = window_event {
                if !cx.modifiers().ctrl() && !cx.modifiers().logo() {
                    cx.emit(TimelineEvent::ScrollBy { dx: (-*x as f64) * 32.0, dy: (-*y as f64) * 32.0 });
                    meta.consume();
                }
            }
        });
    }
}

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

/// A group of `drum_samples()` filenames sharing a naming prefix - so a
/// folder that's grown well past a handful of one-shots (a dropped-in
/// pack of loops, say) still browses as a short list of short lists,
/// not one long undifferentiated one.
pub struct SampleCategory {
    pub label: &'static str,
    pub prefix: &'static str,
    pub files: Vec<String>,
}

/// Buckets `drum_samples()` by filename prefix, dropping the prefix from
/// each file's own display name (redundant once it's already the
/// section header) and omitting any category with nothing in it. What
/// doesn't match a known prefix - the original hand-picked one-shots -
/// stays under the plain "Drum samples" heading.
pub fn drum_sample_categories() -> Vec<SampleCategory> {
    const CATEGORIES: &[(&str, &str)] = &[("Piano loops", "piano_"), ("Ambient sketches", "micro_")];

    let mut samples = drum_samples();
    let mut categories: Vec<SampleCategory> = CATEGORIES
        .iter()
        .map(|(label, prefix)| SampleCategory { label, prefix, files: Vec::new() })
        .collect();

    samples.retain(|filename| {
        for category in categories.iter_mut() {
            if filename.starts_with(category.prefix) {
                category.files.push(filename.clone());
                return false;
            }
        }
        true
    });

    let mut all = vec![SampleCategory { label: "Drum samples", prefix: "", files: samples }];
    all.extend(categories);
    all.retain(|c| !c.files.is_empty());
    all
}

/// A category's file's display name, with its category prefix (if any)
/// stripped first - the section header already says "Piano loops", so
/// every row under it repeating "Piano" would be noise, not information.
pub fn sample_display_name(category: &SampleCategory, filename: &str) -> String {
    let stem = filename.strip_suffix(".wav").unwrap_or(filename);
    let stem = stem.strip_prefix(category.prefix).unwrap_or(stem);
    state::display_name_from_stem(stem)
}

