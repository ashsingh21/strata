//! The lane area: grid, clips (waveform/MIDI), automation, selection wash
//! and playhead. One custom canvas view; draws and hit-tests only the
//! visible tick range and visible rows.

use std::sync::Arc;
use std::time::{Duration, Instant};

use vizia::prelude::*;
use vizia::vg;

use shared::arrangement::{
    snap, Arrangement, AutomationLaneId, Breakpoint, Clip, ClipContent, ClipId, SnapGrid,
    Ticks, TrackId, TrackKind, ViewTransform,
};

use crate::recorder::RecordingPreview;
use crate::synth::state::SynthEvent;
use crate::timeline::header::clip_color_to_rgb;
use crate::timeline::state::{ContextMenu, ContextMenuTarget, Selection, TimelineEvent, TimelineTool};
use crate::tokens::{self, ThemeId};

/// One bar at 4/4 - the default length for a clip created with a plain
/// click (rather than a drag) in Draw mode.
const DEFAULT_DRAWN_CLIP_LENGTH: Ticks = shared::arrangement::PPQ * 4;

const CLIP_HEADER_H: f32 = 14.0;
const CLIP_INSET: f32 = 2.0;
/// Waveforms drawn bigger than their true amplitude (clamped back to the
/// row's height) - most real playing doesn't reach 0dBFS, and a waveform
/// scaled to actual peak reads as flatter/quieter than it sounds.
const WAVEFORM_BOOST: f32 = 1.6;
const EDGE_GRAB_PX: f32 = 6.0;
const BREAKPOINT_GRAB_PX: f32 = 6.0;
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

#[derive(Clone, Copy, PartialEq)]
enum RowKind {
    Track(TrackId),
    Automation(AutomationLaneId),
}

#[derive(Clone, Copy)]
struct Row {
    kind: RowKind,
    top: f32,
    height: f32,
}

fn row_at_y(rows: &[Row], y_scrolled: f32) -> Option<usize> {
    rows.iter().position(|r| y_scrolled >= r.top && y_scrolled < r.top + r.height)
}

fn build_rows(arr: &Arrangement) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut y = 0.0f32;
    for track in &arr.tracks {
        let height = track.height.clamp(shared::arrangement::MIN_TRACK_HEIGHT, shared::arrangement::MAX_TRACK_HEIGHT);
        rows.push(Row { kind: RowKind::Track(track.id), top: y, height });
        y += height;
        for lane in arr.automation.iter().filter(|a| a.track == track.id) {
            rows.push(Row { kind: RowKind::Automation(lane.id), top: y, height: crate::timeline::LANE_AUTO_HEIGHT });
            y += crate::timeline::LANE_AUTO_HEIGHT;
        }
    }
    rows
}

#[derive(Clone, Copy)]
enum Edge {
    Start,
    End,
}

enum Drag {
    MoveClips {
        /// (clip, original track, original start).
        clips: Vec<(ClipId, TrackId, Ticks)>,
        grab_tick: Ticks,
        /// Only applied when exactly one clip is being dragged.
        original_row: usize,
        delta_ticks: Ticks,
        row_delta: i32,
    },
    TrimClip {
        clip: ClipId,
        edge: Edge,
        original_start: Ticks,
        original_length: Ticks,
        delta_ticks: Ticks,
    },
    RubberBand {
        anchor: (f32, f32),
        current: (f32, f32),
    },
    MoveBreakpoint {
        lane: AutomationLaneId,
        original: Breakpoint,
        current: Breakpoint,
    },
    /// Draw tool: dragging out a new MIDI clip on empty track space.
    DrawClip {
        track: TrackId,
        row_top: f32,
        row_height: f32,
        anchor_tick: Ticks,
        current_tick: Ticks,
    },
    /// Dragging a scrollbar thumb: pointer travel scales to content travel.
    ScrollThumb { vertical: bool, last: f32 },
}

/// Scrollbar thumb thickness and its grab zone from the lane edge.
const THUMB_PX: f32 = 5.0;
const THUMB_GRAB_PX: f32 = 12.0;

/// A scrollbar thumb along an edge `track_len` long: (start, length) in
/// px, or `None` when everything already fits.
fn thumb(track_len: f32, content: f32, scroll: f32) -> Option<(f32, f32)> {
    if content <= track_len + 1.0 {
        return None;
    }
    let len = (track_len * track_len / content).max(24.0);
    let start = (scroll / (content - track_len)).clamp(0.0, 1.0) * (track_len - len);
    Some((start, len))
}

pub struct LaneArea {
    arrangement: Signal<Arrangement>,
    transform: Signal<ViewTransform>,
    selection: Signal<Selection>,
    playhead: Signal<Ticks>,
    theme: Signal<ThemeId>,
    recording_preview: Signal<Option<RecordingPreview>>,
    live_peaks: Signal<Arc<[f32]>>,
    tool: Signal<TimelineTool>,
    /// Set from `on_mouse_move` whenever the cursor is over a clip's
    /// trim edge (and back to `Default` when it isn't) - purely a visual
    /// hint before any drag starts; the actual edge hit-test at drag time
    /// (`on_mouse_down`) is separate and authoritative.
    hover_cursor: Signal<CursorIcon>,
    drag: Option<Drag>,
    last_click: Option<(Instant, f32, f32)>,
}

impl LaneArea {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cx: &mut Context,
        arrangement: Signal<Arrangement>,
        transform: Signal<ViewTransform>,
        selection: Signal<Selection>,
        playhead: Signal<Ticks>,
        theme: Signal<ThemeId>,
        recording_preview: Signal<Option<RecordingPreview>>,
        live_peaks: Signal<Arc<[f32]>>,
        tool: Signal<TimelineTool>,
    ) -> Handle<'_, Self> {
        // Deliberately not bound to `playhead`: it changes every frame
        // during playback, and redrawing every clip/waveform/grid line
        // just to move a 1px line was the actual cause of the jittery
        // playhead - see `PlayheadOverlay`, which now owns that line and
        // is the only thing that redraws at playback rate. `self.playhead`
        // stays a field only for the dead `clip.recording` chase-length
        // branch above, which nothing currently triggers.
        //
        // `live_peaks` IS bound despite updating every frame, unlike
        // playhead - it only does that while a take is actively
        // recording, a rare, deliberate state (not the common playback
        // path this file otherwise guards so carefully), and it's what
        // makes the in-progress clip draw a live waveform instead of
        // sitting flat until the take is decoded.
        let hover_cursor: Signal<CursorIcon> = Signal::new(CursorIcon::Default);
        Self {
            arrangement,
            transform,
            selection,
            playhead,
            theme,
            recording_preview,
            live_peaks,
            tool,
            hover_cursor,
            drag: None,
            last_click: None,
        }
        .build(cx, |_| {})
        .bind(tool, |mut h| h.needs_redraw())
        .bind(arrangement, |mut h| h.needs_redraw())
        .bind(transform, |mut h| h.needs_redraw())
        .bind(selection, |mut h| h.needs_redraw())
        .bind(theme, |mut h| h.needs_redraw())
        .bind(recording_preview, |mut h| h.needs_redraw())
        .bind(live_peaks, |mut h| h.needs_redraw())
        .cursor(hover_cursor)
    }
}

impl View for LaneArea {
    fn element(&self) -> Option<&'static str> {
        Some("strata-lanes")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, _| match window_event {
            WindowEvent::MouseDown(button) if *button == MouseButton::Left => {
                self.on_mouse_down(cx);
            }
            WindowEvent::MouseDown(button) if *button == MouseButton::Right => {
                self.on_right_click(cx);
            }
            WindowEvent::MouseMove(x, y) => {
                self.on_mouse_move(cx, *x, *y);
            }
            WindowEvent::MouseUp(button) if *button == MouseButton::Left => {
                self.on_mouse_up(cx);
            }
            WindowEvent::MouseScroll(x, y) => {
                self.on_scroll(cx, *x, *y);
            }
            WindowEvent::GeometryChanged(_) => {
                let b = cx.bounds();
                cx.emit(TimelineEvent::SetViewport { width: b.w as f64, height: b.h as f64 });
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        self.draw_impl(cx, canvas);
        self.draw_scrollbars(cx, canvas);
    }
}

impl LaneArea {
    fn local_pos(&self, cx: &EventContext) -> (f32, f32) {
        let bounds = cx.bounds();
        (cx.mouse().cursor_x - bounds.x, cx.mouse().cursor_y - bounds.y)
    }

    fn on_mouse_down(&mut self, cx: &mut EventContext) {
        let (lx, ly) = self.local_pos(cx);
        // Scrollbar thumbs sit on top of everything along the edges.
        {
            let b = cx.bounds();
            let (cw, ch) = self.content_size();
            let t = self.transform.get();
            let on_right = lx >= b.w - THUMB_GRAB_PX && thumb(b.h, ch, t.scroll_y as f32).is_some();
            let on_bottom = ly >= b.h - THUMB_GRAB_PX && thumb(b.w, cw, t.scroll_x as f32).is_some();
            if on_right || on_bottom {
                self.drag = Some(Drag::ScrollThumb { vertical: on_right, last: if on_right { ly } else { lx } });
                cx.capture();
                return;
            }
        }
        let transform = self.transform.get();
        let arr = self.arrangement.get();
        let rows = build_rows(&arr);
        let y_scrolled = ly + transform.scroll_y as f32;
        let tick = transform.x_to_tick(lx as f64);

        cx.capture();
        cx.focus_with_visibility(false);

        let Some(row_index) = row_at_y(&rows, y_scrolled) else {
            self.drag = Some(Drag::RubberBand { anchor: (lx, ly), current: (lx, ly) });
            return;
        };
        let row = rows[row_index];

        match row.kind {
            RowKind::Track(track_id) => {
                // Any click in this row selects its track - clicking a
                // clip already did this as a side effect of `SelectClip`,
                // but only when the clip wasn't already selected, and
                // empty lane space (drawing, rubber-band) never did at
                // all. Selecting here first covers every case uniformly,
                // matching a click anywhere in the track's own header row.
                cx.emit(SynthEvent::SelectTrack(track_id));

                // Hit-test clips on this track, topmost/last first.
                let hit = arr
                    .clips
                    .iter()
                    .filter(|c| c.track == track_id)
                    .rev()
                    .find(|c| tick >= c.start && tick < c.end());

                if let Some(clip) = hit {
                    let is_double = self
                        .last_click
                        .map(|(t, px, py)| t.elapsed() < DOUBLE_CLICK && (px - lx).abs() < 8.0 && (py - ly).abs() < 8.0)
                        .unwrap_or(false);
                    self.last_click = Some((Instant::now(), lx, ly));
                    if is_double && matches!(clip.content, ClipContent::Midi { .. }) {
                        cx.emit(crate::piano_roll::state::PianoRollEvent::Open(clip.id));
                        return;
                    }

                    let start_x = transform.tick_to_x(clip.start) as f32;
                    let end_x = transform.tick_to_x(clip.end()) as f32;
                    // A short clip (a single drum hit, easily narrower than
                    // twice the grab zone) needs a proportionally smaller
                    // grab zone, or every click anywhere on it - including
                    // its middle - reads as an edge, and it can never be
                    // moved by dragging, only ever shrunk toward nothing.
                    let grab_px = EDGE_GRAB_PX.min((end_x - start_x) / 4.0).max(1.0);
                    let edge = if (lx - start_x).abs() <= grab_px {
                        Some(Edge::Start)
                    } else if (lx - end_x).abs() <= grab_px {
                        Some(Edge::End)
                    } else {
                        None
                    };

                    if let Some(edge) = edge {
                        self.drag = Some(Drag::TrimClip {
                            clip: clip.id,
                            edge,
                            original_start: clip.start,
                            original_length: clip.length,
                            delta_ticks: 0,
                        });
                        return;
                    }

                    let extend = cx.modifiers().shift();
                    let already_selected = self.selection.get().clips.contains(&clip.id);
                    if !already_selected {
                        cx.emit(TimelineEvent::SelectClip { clip: clip.id, extend });
                    }

                    let selection_after = if extend {
                        let mut sel = self.selection.get().clips;
                        if already_selected {
                            sel.remove(&clip.id);
                        } else {
                            sel.insert(clip.id);
                        }
                        sel
                    } else if already_selected {
                        self.selection.get().clips
                    } else {
                        std::iter::once(clip.id).collect()
                    };

                    let clips: Vec<(ClipId, TrackId, Ticks)> = arr
                        .clips
                        .iter()
                        .filter(|c| selection_after.contains(&c.id))
                        .map(|c| (c.id, c.track, c.start))
                        .collect();

                    self.drag = Some(Drag::MoveClips {
                        clips,
                        grab_tick: tick,
                        original_row: row_index,
                        delta_ticks: 0,
                        row_delta: 0,
                    });
                } else {
                    let is_midi = arr.track(track_id).map(|t| t.kind) == Some(TrackKind::Midi);
                    if self.tool.get() == TimelineTool::Draw && is_midi {
                        let anchor = tick.max(0);
                        self.drag = Some(Drag::DrawClip {
                            track: track_id,
                            row_top: row.top,
                            row_height: row.height,
                            anchor_tick: anchor,
                            current_tick: anchor,
                        });
                    } else {
                        if !cx.modifiers().shift() {
                            cx.emit(TimelineEvent::ClearSelection);
                        }
                        self.drag = Some(Drag::RubberBand { anchor: (lx, ly), current: (lx, ly) });
                    }
                }
            }

            RowKind::Automation(lane_id) => {
                let is_double = self
                    .last_click
                    .map(|(t, px, py)| t.elapsed() < DOUBLE_CLICK && (px - lx).abs() < 8.0 && (py - ly).abs() < 8.0)
                    .unwrap_or(false);
                self.last_click = Some((Instant::now(), lx, ly));

                let hit = arr.automation_lane(lane_id).and_then(|lane| {
                    lane.breakpoints.iter().find(|bp| {
                        let bx = transform.tick_to_x(bp.tick) as f32;
                        let by = row.top - y_scrolled + ly + (1.0 - bp.value) * row.height;
                        (bx - lx).abs() <= BREAKPOINT_GRAB_PX && (by - ly).abs() <= BREAKPOINT_GRAB_PX
                    })
                });

                if let Some(bp) = hit {
                    if is_double {
                        cx.emit(TimelineEvent::RemoveBreakpoint { lane: lane_id, tick: bp.tick });
                    } else {
                        cx.emit(TimelineEvent::SelectBreakpoint(Some((lane_id, bp.tick))));
                        self.drag =
                            Some(Drag::MoveBreakpoint { lane: lane_id, original: *bp, current: *bp });
                    }
                } else {
                    let local_y = ly - (row.top - y_scrolled);
                    let value = (1.0 - (local_y / row.height)).clamp(0.0, 1.0);
                    let point = Breakpoint { tick: tick.max(0), value };
                    cx.emit(TimelineEvent::AddBreakpoint { lane: lane_id, point });
                    cx.emit(TimelineEvent::SelectBreakpoint(Some((lane_id, point.tick))));
                }
            }
        }
    }

    /// Right-click: hit-test a clip (selecting it first, unless it's
    /// already part of a multi-selection) or fall back to empty track
    /// space, and open the context menu there. No drag, no capture.
    fn on_right_click(&mut self, cx: &mut EventContext) {
        let (lx, ly) = self.local_pos(cx);
        let transform = self.transform.get();
        let arr = self.arrangement.get();
        let rows = build_rows(&arr);
        let y_scrolled = ly + transform.scroll_y as f32;
        let tick = transform.x_to_tick(lx as f64);
        let (window_x, window_y) = (cx.mouse().cursor_x, cx.mouse().cursor_y);

        let Some(row_index) = row_at_y(&rows, y_scrolled) else { return };
        let RowKind::Track(track_id) = rows[row_index].kind else { return };

        let hit = arr.clips.iter().filter(|c| c.track == track_id).rev().find(|c| tick >= c.start && tick < c.end());

        let target = if let Some(clip) = hit {
            if !self.selection.get().clips.contains(&clip.id) {
                cx.emit(TimelineEvent::SelectClip { clip: clip.id, extend: false });
            }
            ContextMenuTarget::Clip(clip.id)
        } else {
            ContextMenuTarget::Lane { track: track_id, tick: tick.max(0) }
        };
        cx.emit(TimelineEvent::OpenContextMenu(ContextMenu { target, x: window_x, y: window_y }));
    }

    /// Whether (lx, ly) (local, unscrolled) sits over a clip's trim edge -
    /// a read-only query shared by the hover cursor and (via its own
    /// independent copy of this hit-test) `on_mouse_down`'s decision to
    /// start a `Drag::TrimClip`.
    fn edge_hover_at(&self, lx: f32, ly: f32) -> bool {
        let transform = self.transform.get();
        let arr = self.arrangement.get();
        let rows = build_rows(&arr);
        let y_scrolled = ly + transform.scroll_y as f32;
        let tick = transform.x_to_tick(lx as f64);
        let Some(row_index) = row_at_y(&rows, y_scrolled) else { return false };
        let RowKind::Track(track_id) = rows[row_index].kind else { return false };
        let Some(clip) =
            arr.clips.iter().filter(|c| c.track == track_id).rev().find(|c| tick >= c.start && tick < c.end())
        else {
            return false;
        };
        let start_x = transform.tick_to_x(clip.start) as f32;
        let end_x = transform.tick_to_x(clip.end()) as f32;
        let grab_px = EDGE_GRAB_PX.min((end_x - start_x) / 4.0).max(1.0);
        (lx - start_x).abs() <= grab_px || (lx - end_x).abs() <= grab_px
    }

    fn on_mouse_move(&mut self, cx: &mut EventContext, x: f32, y: f32) {
        if self.drag.is_none() {
            let bounds = cx.bounds();
            let (lx, ly) = (x - bounds.x, y - bounds.y);
            let icon = if self.edge_hover_at(lx, ly) { CursorIcon::EwResize } else { CursorIcon::Default };
            if self.hover_cursor.get() != icon {
                self.hover_cursor.set(icon);
                // The reactive `.cursor(hover_cursor)` binding alone
                // doesn't repaint the OS cursor here: Vizia only re-reads
                // an entity's `cursor` style (`hover.rs::hover_system`)
                // when the *hovered entity itself* changes, never on a
                // style value changing while the same (one large canvas)
                // entity stays hovered - unlike `TrackResizeHandle`,
                // which is its own small entity, so simply entering it
                // triggers that same check. Applying it directly here is
                // the actual fix, not just belt-and-suspenders.
                cx.emit(WindowEvent::SetCursor(icon));
            }
        }
        if let Some(Drag::ScrollThumb { .. }) = self.drag {
            let b = cx.bounds();
            let (cw, ch) = self.content_size();
            let Some(Drag::ScrollThumb { vertical, last }) = &mut self.drag else { return };
            let (pos, track, content) = if *vertical { (y - b.y, b.h, ch) } else { (x - b.x, b.w, cw) };
            let travel = pos - *last;
            *last = pos;
            // Thumb travel maps to content travel in proportion.
            let scale = (content / track.max(1.0)) as f64;
            let delta = travel as f64 * scale;
            if *vertical {
                cx.emit(TimelineEvent::ScrollBy { dx: 0.0, dy: delta });
            } else {
                cx.emit(TimelineEvent::ScrollBy { dx: delta, dy: 0.0 });
            }
            return;
        }
        let Some(drag) = &mut self.drag else { return };
        let bounds = cx.bounds();
        let (lx, ly) = (x - bounds.x, y - bounds.y);
        let transform = self.transform.get();
        let arr = self.arrangement.get();
        let bypass = cx.modifiers().alt();
        let snap_grid = SnapGrid::Sixteenth;
        let tick = transform.x_to_tick(lx as f64);

        match drag {
            Drag::ScrollThumb { .. } => {}
            Drag::MoveClips { clips, grab_tick, original_row, delta_ticks, row_delta } => {
                *delta_ticks = snap(tick - *grab_tick, snap_grid, bypass);
                if clips.len() == 1 {
                    let rows = build_rows(&arr);
                    let y_scrolled = ly + transform.scroll_y as f32;
                    if let Some(target_row) = row_at_y(&rows, y_scrolled) {
                        let track_rows: Vec<usize> = rows
                            .iter()
                            .enumerate()
                            .filter(|(_, r)| matches!(r.kind, RowKind::Track(_)))
                            .map(|(i, _)| i)
                            .collect();
                        if let (Some(orig_pos), Some(target_pos)) = (
                            track_rows.iter().position(|&i| i == *original_row),
                            track_rows.iter().position(|&i| i == target_row),
                        ) {
                            *row_delta = target_pos as i32 - orig_pos as i32;
                        }
                    }
                }
                cx.needs_redraw();
            }
            Drag::TrimClip { edge, original_start, original_length, delta_ticks, .. } => {
                *delta_ticks = match edge {
                    Edge::Start => snap(tick, snap_grid, bypass) - *original_start,
                    Edge::End => snap(tick, snap_grid, bypass) - (*original_start + *original_length),
                };
                cx.needs_redraw();
            }
            Drag::RubberBand { current, .. } => {
                *current = (lx, ly);
                cx.needs_redraw();
            }
            Drag::DrawClip { current_tick, .. } => {
                *current_tick = snap(tick.max(0), snap_grid, bypass);
                cx.needs_redraw();
            }
            Drag::MoveBreakpoint { lane, current, .. } => {
                let rows = build_rows(&arr);
                let lane_id = *lane;
                if let Some(row) = rows.iter().find(|r| matches!(r.kind, RowKind::Automation(id) if id == lane_id)) {
                    let y_scrolled = ly + transform.scroll_y as f32;
                    let local_y = y_scrolled - row.top;
                    let value = (1.0 - (local_y / row.height)).clamp(0.0, 1.0);
                    current.tick = snap(tick.max(0), snap_grid, bypass);
                    current.value = value;
                    cx.needs_redraw();
                }
            }
        }
    }

    fn on_mouse_up(&mut self, cx: &mut EventContext) {
        let Some(drag) = self.drag.take() else { return };
        cx.release();

        match drag {
            Drag::ScrollThumb { .. } => {}
            Drag::MoveClips { clips, delta_ticks, row_delta, .. } => {
                if delta_ticks == 0 && row_delta == 0 {
                    return;
                }
                let arr = self.arrangement.get();
                let track_ids: Vec<TrackId> = arr.tracks.iter().map(|t| t.id).collect();
                for (clip_id, original_track, original_start) in clips {
                    let new_start = (original_start + delta_ticks).max(0);
                    let new_track = if row_delta != 0 {
                        let idx = track_ids.iter().position(|&t| t == original_track);
                        idx.and_then(|i| {
                            let new_i = i as i32 + row_delta;
                            (new_i >= 0 && (new_i as usize) < track_ids.len())
                                .then(|| track_ids[new_i as usize])
                        })
                        .unwrap_or(original_track)
                    } else {
                        original_track
                    };
                    cx.emit(TimelineEvent::MoveClip { clip: clip_id, track: new_track, start: new_start });
                }
            }
            Drag::TrimClip { clip, edge, original_start, original_length, delta_ticks } => {
                if delta_ticks == 0 {
                    return;
                }
                let (start, length) = match edge {
                    Edge::Start => {
                        let new_start = (original_start + delta_ticks).min(original_start + original_length - 1);
                        (new_start, original_start + original_length - new_start)
                    }
                    Edge::End => {
                        let new_length = (original_length + delta_ticks).max(1);
                        (original_start, new_length)
                    }
                };
                cx.emit(TimelineEvent::TrimClip { clip, start, length });
            }
            Drag::RubberBand { anchor, current } => {
                let transform = self.transform.get();
                let arr = self.arrangement.get();
                let rows = build_rows(&arr);
                let (x0, x1) = (anchor.0.min(current.0), anchor.0.max(current.0));
                let (y0, y1) = (anchor.1.min(current.1), anchor.1.max(current.1));
                if (x1 - x0).abs() < 2.0 && (y1 - y0).abs() < 2.0 {
                    return;
                }
                let tick0 = transform.x_to_tick(x0 as f64);
                let tick1 = transform.x_to_tick(x1 as f64);
                let y_scroll = transform.scroll_y as f32;

                let mut selected = std::collections::HashSet::new();
                for row in &rows {
                    if let RowKind::Track(track_id) = row.kind {
                        let row_top = row.top - y_scroll;
                        let row_bottom = row_top + row.height;
                        if row_bottom < y0 || row_top > y1 {
                            continue;
                        }
                        for clip in arr.clips_on_track(track_id) {
                            if clip.end() > tick0 && clip.start < tick1 {
                                selected.insert(clip.id);
                            }
                        }
                    }
                }
                cx.emit(TimelineEvent::SelectClips(selected));
                cx.emit(TimelineEvent::SetTimeSelection(Some((tick0.max(0), tick1.max(0)))));
            }
            Drag::MoveBreakpoint { lane, original, current } => {
                if original.tick != current.tick || original.value != current.value {
                    cx.emit(TimelineEvent::MoveBreakpoint {
                        lane,
                        tick: original.tick,
                        new_tick: current.tick,
                        new_value: current.value,
                    });
                }
            }
            Drag::DrawClip { track, anchor_tick, current_tick, .. } => {
                let (start, end) = if current_tick == anchor_tick {
                    // A plain click, not a drag: a default one-bar clip
                    // rather than nothing, so Draw mode always produces
                    // something to open into the piano roll.
                    (anchor_tick, anchor_tick + DEFAULT_DRAWN_CLIP_LENGTH)
                } else {
                    (anchor_tick.min(current_tick), anchor_tick.max(current_tick))
                };
                cx.emit(TimelineEvent::InsertMidiClip { track, start, length: (end - start).max(1) });
            }
        }
    }

    fn draw_impl(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();
        let arr = self.arrangement.get();
        let transform = self.transform.get();
        let selection = self.selection.get();
        let rows = build_rows(&arr);
        let scroll_y = transform.scroll_y as f32;
        let sig = arr.tempo_map.time_signature_at(0);
        let ticks_per_bar = sig.ticks_per_bar();
        let ticks_per_beat = sig.ticks_per_beat();

        let (drag_move, drag_trim, rubber_band, draw_clip) = match &self.drag {
            Some(Drag::MoveClips { clips, delta_ticks, row_delta, .. }) => {
                (Some((clips.as_slice(), *delta_ticks, *row_delta)), None, None, None)
            }
            Some(Drag::TrimClip { clip, edge, original_start, original_length, delta_ticks }) => {
                (None, Some((*clip, *edge, *original_start, *original_length, *delta_ticks)), None, None)
            }
            Some(Drag::RubberBand { anchor, current }) => (None, None, Some((*anchor, *current)), None),
            Some(Drag::DrawClip { row_top, row_height, anchor_tick, current_tick, .. }) => {
                (None, None, None, Some((*row_top, *row_height, *anchor_tick, *current_tick)))
            }
            _ => (None, None, None, None),
        };
        let track_ids: Vec<TrackId> = arr.tracks.iter().map(|t| t.id).collect();

        for row in &rows {
            let top = row.top - scroll_y;
            if top + row.height < 0.0 || top > bounds.h {
                continue;
            }
            let y0 = bounds.y + top;
            let row_rect = vg::Rect::new(bounds.x, y0, bounds.x + bounds.w, y0 + row.height);

            match row.kind {
                RowKind::Track(_) => {
                    self.draw_grid(canvas, &palette, row_rect, &transform, ticks_per_bar, ticks_per_beat);
                }
                RowKind::Automation(_) => {
                    let mut bg = vg::Paint::default();
                    bg.set_color(palette.bg_000);
                    bg.set_anti_alias(true);
                    canvas.draw_path(&vg::Path::rect(row_rect, None), &bg);
                }
            }

            let mut line_paint = vg::Paint::default();
            line_paint.set_color(palette.line);
            line_paint.set_anti_alias(true);
            canvas.draw_path(
                &vg::Path::rect(vg::Rect::new(row_rect.left, row_rect.bottom - 1.0, row_rect.right, row_rect.bottom), None),
                &line_paint,
            );
        }

        // Time selection wash, under every visible track row.
        if let Some((sel_start, sel_end)) = selection.time_range {
            let x0 = bounds.x + transform.tick_to_x(sel_start) as f32;
            let x1 = bounds.x + transform.tick_to_x(sel_end) as f32;
            let mut wash = vg::Paint::default();
            wash.set_color(palette.selection);
            wash.set_anti_alias(true);
            for row in &rows {
                if !matches!(row.kind, RowKind::Track(_)) {
                    continue;
                }
                let top = row.top - scroll_y;
                if top + row.height < 0.0 || top > bounds.h {
                    continue;
                }
                let y0 = bounds.y + top;
                canvas.draw_path(&vg::Path::rect(vg::Rect::new(x0, y0, x1, y0 + row.height), None), &wash);
            }
        }

        // A clip being dragged to a different track row is repositioned
        // (below) to draw at its target row's y - but it's still drawn
        // *during that row's own turn* in this loop, wherever that falls
        // in track order. Dragging it to a row earlier in that order (so
        // its turn comes first) means every later row's real clips then
        // paint over it: it visibly "goes behind" them mid-drag. Skipped
        // here and drawn once more, after every row, so it's always the
        // last (topmost) thing painted regardless of drag direction.
        let dragged_across_rows: Option<ClipId> = match drag_move {
            Some((clips, _, row_delta)) if clips.len() == 1 && row_delta != 0 => Some(clips[0].0),
            _ => None,
        };

        // Clips.
        for row in &rows {
            let RowKind::Track(track_id) = row.kind else { continue };
            let top = row.top - scroll_y;
            if top + row.height < 0.0 || top > bounds.h {
                continue;
            }
            let track_color = arr.track(track_id).map(|t| t.color).unwrap_or(shared::arrangement::ClipColor::Coral);
            for clip in arr.clips_on_track(track_id) {
                if Some(clip.id) == dragged_across_rows {
                    continue;
                }
                let mut start = clip.start;
                let mut track_row_top = top;

                if let Some((clips, delta_ticks, row_delta)) = drag_move {
                    if let Some(&(_, orig_track, orig_start)) =
                        clips.iter().find(|(id, _, _)| *id == clip.id)
                    {
                        start = (orig_start + delta_ticks).max(0);
                        if clips.len() == 1 && row_delta != 0 {
                            if let Some(idx) = track_ids.iter().position(|&t| t == orig_track) {
                                let new_i = (idx as i32 + row_delta)
                                    .clamp(0, track_ids.len() as i32 - 1);
                                if let Some(target_row) =
                                    rows.iter().find(|r| r.kind == RowKind::Track(track_ids[new_i as usize]))
                                {
                                    track_row_top = target_row.top - scroll_y;
                                }
                            }
                        }
                    }
                }

                let mut length = clip.length;
                if clip.recording {
                    let playhead = self.playhead.get();
                    if playhead > clip.start {
                        length = playhead - clip.start;
                    }
                }
                // While a clip is being trimmed, only `start`/`length` (and
                // so the box's own on-screen position) live-preview here -
                // `clip` itself stays the original, uncommitted-drag data
                // until mouse-up actually applies `Command::TrimClip`. If
                // the waveform below queried `clip` directly it'd keep
                // asking for the *original* sample range and squeeze it
                // into the shrinking/growing box - looking squished
                // rather than trimmed until release. So build a real
                // preview `Clip` (mirroring `Command::TrimClip`'s own
                // `source_offset_samples` adjustment) and draw *that*
                // instead, whenever this is the clip being trimmed.
                let mut trimmed_preview = None;
                if let Some((trim_clip, edge, orig_start, orig_length, delta_ticks)) = drag_trim {
                    if trim_clip == clip.id {
                        match edge {
                            Edge::Start => {
                                start = (orig_start + delta_ticks).min(orig_start + orig_length - 1);
                                length = orig_start + orig_length - start;
                            }
                            Edge::End => {
                                length = (orig_length + delta_ticks).max(1);
                            }
                        }
                        let mut preview = clip.clone();
                        preview.start = start;
                        preview.length = length;
                        if let (Edge::Start, ClipContent::Audio { source_offset_samples, .. }) =
                            (edge, &mut preview.content)
                        {
                            let bpm = arr.tempo_map.bpm_at(start);
                            let seconds = ((start - orig_start) as f64 / shared::arrangement::PPQ as f64) * (60.0 / bpm);
                            let delta_samples = (seconds * 48_000.0).round() as i64;
                            *source_offset_samples = (*source_offset_samples as i64 + delta_samples).max(0) as u64;
                        }
                        trimmed_preview = Some(preview);
                    }
                }
                let clip = trimmed_preview.as_ref().unwrap_or(clip);

                let x0_raw = bounds.x + transform.tick_to_x(start) as f32;
                let x1_raw = bounds.x + transform.tick_to_x(start + length) as f32;
                if x1_raw < bounds.x || x0_raw > bounds.x + bounds.w {
                    continue;
                }
                // Skia doesn't clip to this view's own layout bounds, so a
                // clip scrolled off-screen to the left (or one stretched far
                // past it, like the still-"recording" demo clip) would
                // otherwise paint straight across the track header sidebar.
                let x0 = x0_raw.max(bounds.x);
                let x1 = x1_raw.min(bounds.x + bounds.w);
                let y0 = bounds.y + track_row_top + CLIP_INSET;
                let y1 = bounds.y + track_row_top + row.height - CLIP_INSET;

                let selected = selection.clips.contains(&clip.id);
                self.draw_clip(canvas, &palette, clip, track_color, x0, y0, x1, y1, selected);
            }

            // The in-progress take, if this is its track - a transient
            // preview, never a real `Arrangement` clip (see
            // `crate::recorder`), reusing `draw_clip` for a consistent
            // look via a throwaway `Clip` that's never inserted anywhere.
            if let Some(preview) = self.recording_preview.get() {
                if preview.track == track_id {
                    let x0_raw = bounds.x + transform.tick_to_x(preview.start) as f32;
                    let x1_raw = bounds.x + transform.tick_to_x(preview.start + preview.length) as f32;
                    if x1_raw >= bounds.x && x0_raw <= bounds.x + bounds.w {
                        let x0 = x0_raw.max(bounds.x);
                        let x1 = x1_raw.min(bounds.x + bounds.w);
                        let y0 = bounds.y + top + CLIP_INSET;
                        let y1 = bounds.y + top + row.height - CLIP_INSET;
                        let preview_clip = Clip {
                            id: 0,
                            track: preview.track,
                            start: preview.start,
                            length: preview.length,
                            name: "Recording".to_string(),
                            content: ClipContent::Audio { source: "".into(), peaks: None, source_offset_samples: 0 },
                            recording: true,
                            gain_db: 0.0,
                        };
                        self.draw_clip(canvas, &palette, &preview_clip, track_color, x0, y0, x1, y1, false);
                        let peaks = self.live_peaks.get();
                        if !peaks.is_empty() {
                            let header_bottom = (y0 + CLIP_HEADER_H).min(y1);
                            if y1 > header_bottom {
                                self.draw_live_waveform(canvas, &peaks, x0, header_bottom, x1, y1);
                            }
                        }
                    }
                }
            }
        }

        // The clip skipped above, drawn last so it's always on top - see
        // the comment where `dragged_across_rows` is computed.
        if let (Some(dragged_id), Some((clips, delta_ticks, row_delta))) = (dragged_across_rows, drag_move) {
            let (_, orig_track, orig_start) = clips[0];
            if let (Some(clip), Some(idx)) = (arr.clip(dragged_id), track_ids.iter().position(|&t| t == orig_track)) {
                let new_i = (idx as i32 + row_delta).clamp(0, track_ids.len() as i32 - 1);
                let target_track = track_ids[new_i as usize];
                if let Some(target_row) = rows.iter().find(|r| r.kind == RowKind::Track(target_track)) {
                    let top = target_row.top - scroll_y;
                    if top + target_row.height >= 0.0 && top <= bounds.h {
                        let start = (orig_start + delta_ticks).max(0);
                        let x0_raw = bounds.x + transform.tick_to_x(start) as f32;
                        let x1_raw = bounds.x + transform.tick_to_x(start + clip.length) as f32;
                        if x1_raw >= bounds.x && x0_raw <= bounds.x + bounds.w {
                            let x0 = x0_raw.max(bounds.x);
                            let x1 = x1_raw.min(bounds.x + bounds.w);
                            let y0 = bounds.y + top + CLIP_INSET;
                            let y1 = bounds.y + top + target_row.height - CLIP_INSET;
                            let track_color =
                                arr.track(target_track).map(|t| t.color).unwrap_or(shared::arrangement::ClipColor::Coral);
                            let selected = selection.clips.contains(&clip.id);
                            self.draw_clip(canvas, &palette, clip, track_color, x0, y0, x1, y1, selected);
                        }
                    }
                }
            }
        }

        // Automation lines + breakpoints.
        for row in &rows {
            let RowKind::Automation(lane_id) = row.kind else { continue };
            let top = row.top - scroll_y;
            if top + row.height < 0.0 || top > bounds.h {
                continue;
            }
            let Some(lane) = arr.automation_lane(lane_id) else { continue };
            if lane.breakpoints.is_empty() {
                continue;
            }
            let y0 = bounds.y + top;

            let value_at = |tick: Ticks, value: f32| -> vg::Point {
                let x = bounds.x + transform.tick_to_x(tick) as f32;
                let y = y0 + (1.0 - value) * row.height;
                vg::Point::new(x, y)
            };

            let mut path = vg::PathBuilder::new();
            for (i, bp) in lane.breakpoints.iter().enumerate() {
                let (tick, value) = live_breakpoint(&self.drag, lane_id, bp);
                let p = value_at(tick, value);
                if i == 0 {
                    path.move_to(p);
                } else {
                    path.line_to(p);
                }
            }
            let mut line_paint = vg::Paint::default();
            line_paint.set_color(palette.ink_muted);
            line_paint.set_style(vg::PaintStyle::Stroke);
            line_paint.set_stroke_width(1.5);
            line_paint.set_anti_alias(true);
            canvas.draw_path(&path.detach(), &line_paint);

            let mut bp_paint = vg::Paint::default();
            bp_paint.set_color(palette.ink);
            bp_paint.set_anti_alias(true);
            for bp in lane.breakpoints.iter() {
                let (tick, value) = live_breakpoint(&self.drag, lane_id, bp);
                let p = value_at(tick, value);
                let rect = vg::Rect::new(p.x - 2.5, p.y - 2.5, p.x + 2.5, p.y + 2.5);
                canvas.draw_path(&vg::Path::rect(rect, None), &bp_paint);
            }
        }

        // The playhead itself is drawn by `PlayheadOverlay`, a separate
        // view stacked on top - see its doc comment for why.

        if let Some((anchor, current)) = rubber_band {
            let x0 = bounds.x + anchor.0.min(current.0);
            let x1 = bounds.x + anchor.0.max(current.0);
            let y0 = bounds.y + anchor.1.min(current.1);
            let y1 = bounds.y + anchor.1.max(current.1);
            let mut band = vg::Paint::default();
            band.set_color(palette.selection);
            band.set_anti_alias(true);
            canvas.draw_path(&vg::Path::rect(vg::Rect::new(x0, y0, x1, y1), None), &band);
            let mut border = vg::Paint::default();
            border.set_color(palette.ink_muted);
            border.set_style(vg::PaintStyle::Stroke);
            border.set_stroke_width(1.0);
            border.set_anti_alias(true);
            canvas.draw_path(&vg::Path::rect(vg::Rect::new(x0, y0, x1, y1), None), &border);
        }

        if let Some((row_top, row_height, anchor_tick, current_tick)) = draw_clip {
            let (start, end) = if current_tick == anchor_tick {
                (anchor_tick, anchor_tick + DEFAULT_DRAWN_CLIP_LENGTH)
            } else {
                (anchor_tick.min(current_tick), anchor_tick.max(current_tick))
            };
            let x0 = (bounds.x + transform.tick_to_x(start) as f32).max(bounds.x);
            let x1 = (bounds.x + transform.tick_to_x(end) as f32).min(bounds.x + bounds.w);
            if x1 > x0 {
                let y0 = bounds.y + row_top - scroll_y + CLIP_INSET;
                let y1 = bounds.y + row_top - scroll_y + row_height - CLIP_INSET;
                let mut fill = vg::Paint::default();
                // A clip being drawn isn't sounding: a neutral selection wash
                // with an ink outline, not `signal`.
                fill.set_color(palette.selection);
                fill.set_anti_alias(true);
                canvas.draw_path(&vg::Path::rect(vg::Rect::new(x0, y0, x1, y1), None), &fill);
                let mut border = vg::Paint::default();
                border.set_color(palette.ink);
                border.set_style(vg::PaintStyle::Stroke);
                border.set_stroke_width(1.5);
                border.set_anti_alias(true);
                canvas.draw_path(&vg::Path::rect(vg::Rect::new(x0, y0, x1, y1), None), &border);
            }
        }
    }

    fn draw_grid(
        &self,
        canvas: &Canvas,
        palette: &crate::tokens::Palette,
        rect: vg::Rect,
        transform: &ViewTransform,
        ticks_per_bar: Ticks,
        ticks_per_beat: Ticks,
    ) {
        let mut bg = vg::Paint::default();
        bg.set_color(palette.bg_100);
        bg.set_anti_alias(true);
        canvas.draw_path(&vg::Path::rect(rect, None), &bg);

        let visible_start = transform.x_to_tick(0.0).max(0);
        let visible_end = transform.x_to_tick((rect.right - rect.left) as f64) + ticks_per_bar;

        let beat_px = transform.ticks_to_px(ticks_per_beat);
        let mut paint = vg::Paint::default();
        paint.set_anti_alias(false);

        let first_bar = visible_start / ticks_per_bar;
        let last_bar = visible_end / ticks_per_bar + 1;
        for bar in first_bar..last_bar {
            let bar_tick = bar * ticks_per_bar;
            if beat_px >= 6.0 {
                paint.set_color(palette.grid_beat);
                for beat in 0..(ticks_per_bar / ticks_per_beat) {
                    let tick = bar_tick + beat * ticks_per_beat;
                    let x = rect.left + transform.tick_to_x(tick) as f32;
                    if x < rect.left || x > rect.right {
                        continue;
                    }
                    canvas.draw_path(&vg::Path::rect(vg::Rect::new(x, rect.top, x + 1.0, rect.bottom), None), &paint);
                }
            }
            paint.set_color(palette.grid_bar);
            let x = rect.left + transform.tick_to_x(bar_tick) as f32;
            if x >= rect.left && x <= rect.right {
                canvas.draw_path(&vg::Path::rect(vg::Rect::new(x, rect.top, x + 1.0, rect.bottom), None), &paint);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_clip(
        &self,
        canvas: &Canvas,
        palette: &crate::tokens::Palette,
        clip: &shared::arrangement::Clip,
        track_color: shared::arrangement::ClipColor,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        selected: bool,
    ) {
        if x1 <= x0 {
            return;
        }
        let fill_color = if clip.recording { palette.record } else { clip_color_to_rgb(track_color) };

        let mut fill = vg::Paint::default();
        fill.set_color(fill_color);
        fill.set_anti_alias(true);
        canvas.draw_path(&vg::Path::rect(vg::Rect::new(x0, y0, x1, y1), None), &fill);

        // `clip-edge`: a 1px inner edge (transparent in Studio) so the muted
        // clip colours hold their shape on Daylight's light ground.
        let mut edge = vg::Paint::default();
        edge.set_color(palette.clip_edge);
        edge.set_style(vg::PaintStyle::Stroke);
        edge.set_stroke_width(1.0);
        edge.set_anti_alias(true);
        canvas.draw_path(&vg::Path::rect(vg::Rect::new(x0 + 0.5, y0 + 0.5, x1 - 0.5, y1 - 0.5), None), &edge);

        let header_bottom = (y0 + CLIP_HEADER_H).min(y1);
        let on_clip = tokens::ON_CLIP;
        let divider_color = Color::rgba(on_clip.r(), on_clip.g(), on_clip.b(), 77);
        let mut divider = vg::Paint::default();
        divider.set_color(divider_color);
        divider.set_anti_alias(true);
        canvas.draw_path(
            &vg::Path::rect(vg::Rect::new(x0, header_bottom - 1.0, x1, header_bottom), None),
            &divider,
        );

        let mut text_paint = vg::Paint::default();
        text_paint.set_color(tokens::ON_CLIP);
        text_paint.set_anti_alias(true);
        let font = crate::canvas_text::canvas_font(11.0);
        // Ellipsized to the clip's own width - short one-shot clips used to
        // spill their names across their neighbours ("Kic Kic Kic Kick").
        if let Some(label) = crate::canvas_text::fit_text(&clip.name, &font, x1 - x0 - 8.0) {
            canvas.draw_str(&label, vg::Point::new(x0 + 4.0, y0 + 10.0), &font, &text_paint);
        }

        if y1 > header_bottom {
            self.draw_clip_body(canvas, clip, x0, header_bottom, x1, y1);
        }

        if selected {
            let mut outline = vg::Paint::default();
            outline.set_color(palette.ink);
            outline.set_style(vg::PaintStyle::Stroke);
            outline.set_stroke_width(2.0);
            outline.set_anti_alias(true);
            canvas.draw_path(
                &vg::Path::rect(vg::Rect::new(x0 - 1.0, y0 - 1.0, x1 + 1.0, y1 + 1.0), None),
                &outline,
            );
        }
    }

    /// The in-progress take's waveform: `peaks` is one abs-peak per input
    /// block, in capture order (see `RecorderModel::live_peaks`), not the
    /// real min/max pairs a decoded `PeakPyramid` has, so the envelope is
    /// drawn symmetric around the centre line rather than true min/max -
    /// close enough for a live view that gets thrown away and replaced by
    /// the real waveform the moment the take is decoded.
    ///
    /// Each peak gets a fixed pixel width (`PX_PER_PEAK`) rather than
    /// stretching the whole history to fill the clip's current width every
    /// frame - the clip's pixel width and `peaks.len()` both grow every
    /// frame but not in lockstep (one's tick-based, the other's however
    /// many audio blocks happened to arrive since the last UI tick), so
    /// re-stretching re-bins already-drawn bars slightly differently each
    /// time and the whole envelope visibly swims. Fixed spacing means a
    /// bar's x position is only ever a function of its own index, so
    /// already-drawn bars never move - new ones just append past them.
    fn draw_live_waveform(&self, canvas: &Canvas, peaks: &[f32], x0: f32, y0: f32, x1: f32, y1: f32) {
        let n = peaks.len();
        if n < 2 || x1 <= x0 {
            return;
        }
        let mid = (y0 + y1) * 0.5;
        let half_h = (y1 - y0) * 0.5 - 1.0;
        let width = x1 - x0;

        // Only an abs-peak per block is available here (no true signed
        // min/max, unlike the decoded waveform below), so the sign
        // alternates per entry to fake the same single-line zigzag rather
        // than a flat one-sided trace.
        //
        // Spread every peak across the box's *actual* width rather than
        // a fixed pixel stride per peak: one peak is one audio callback
        // block (a few ms), and at any real zoom that's far less than a
        // pixel's worth of time, so a fixed stride only ever drew the
        // first sliver of the recording - the growing box quickly
        // outran it, leaving the rest empty no matter how long the take
        // ran.
        let mut path = vg::PathBuilder::new();
        for (i, &p) in peaks.iter().enumerate() {
            let x = x0 + (i as f32 / (n - 1) as f32) * width;
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            let y = mid - (p * WAVEFORM_BOOST).min(1.0) * half_h * sign;
            if i == 0 {
                path.move_to(vg::Point::new(x, y));
            } else {
                path.line_to(vg::Point::new(x, y));
            }
        }

        let mut paint = vg::Paint::default();
        paint.set_style(vg::PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        paint.set_stroke_join(vg::PaintJoin::Round);
        paint.set_color(tokens::ON_CLIP);
        paint.set_anti_alias(true);
        canvas.draw_path(&path.detach(), &paint);
    }

    fn draw_clip_body(
        &self,
        canvas: &Canvas,
        clip: &shared::arrangement::Clip,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
    ) {
        let width_px = (x1 - x0).round().max(1.0) as usize;
        let mid = (y0 + y1) * 0.5;
        let half_h = (y1 - y0) * 0.5 - 2.0;

        match &clip.content {
            ClipContent::Audio { peaks: Some(pyramid), source_offset_samples, .. } => {
                let tempo = &self.arrangement.get().tempo_map;
                let bpm = tempo.bpm_at(clip.start);
                let duration_samples =
                    ((clip.length as f64 / shared::arrangement::PPQ as f64) * (60.0 / bpm)
                        * pyramid.sample_rate as f64) as u64;
                let start_sample = *source_offset_samples;
                let end_sample = start_sample + duration_samples.max(1);
                let peaks = pyramid.peaks_for_range(start_sample, end_sample, width_px);

                // A single zigzag line through each column's real min/max
                // (rather than a filled min/max envelope) - a plainer, more
                // minimal look, matching the in-progress take's own single-
                // line waveform above.
                let mut path = vg::PathBuilder::new();
                for (i, (mn, mx)) in peaks.iter().enumerate() {
                    let x = x0 + i as f32;
                    let top = (mx * WAVEFORM_BOOST).min(1.0);
                    let bottom = (mn * WAVEFORM_BOOST).max(-1.0);
                    if i == 0 {
                        path.move_to(vg::Point::new(x, mid - top * half_h));
                    } else {
                        path.line_to(vg::Point::new(x, mid - top * half_h));
                    }
                    path.line_to(vg::Point::new(x, mid - bottom * half_h));
                }

                let mut paint = vg::Paint::default();
                paint.set_style(vg::PaintStyle::Stroke);
                paint.set_stroke_width(1.0);
                paint.set_stroke_join(vg::PaintJoin::Round);
                paint.set_color(tokens::ON_CLIP);
                paint.set_anti_alias(true);
                canvas.draw_path(&path.detach(), &paint);
            }
            ClipContent::Audio { peaks: None, .. } => {
                // Not loaded yet: a thin centre line as a placeholder.
                let mut paint = vg::Paint::default();
                paint.set_color(tokens::ON_CLIP);
                paint.set_anti_alias(true);
                canvas.draw_path(
                    &vg::Path::rect(vg::Rect::new(x0, mid - 0.5, x1, mid + 0.5), None),
                    &paint,
                );
            }
            ClipContent::Midi { notes } => {
                if notes.is_empty() {
                    return;
                }
                let min_pitch = notes.iter().map(|n| n.pitch).min().unwrap() as f32;
                let max_pitch = notes.iter().map(|n| n.pitch).max().unwrap() as f32;
                let span = (max_pitch - min_pitch).max(1.0);
                let mut paint = vg::Paint::default();
                paint.set_color(tokens::ON_CLIP);
                paint.set_anti_alias(true);
                let transform = self.transform.get();
                for note in notes {
                    let nx0 = x0 + transform.ticks_to_px(note.start) as f32;
                    let nx1 = x0 + transform.ticks_to_px(note.start + note.length) as f32;
                    if nx1 <= x0 || nx0 >= x1 {
                        continue;
                    }
                    let frac = 1.0 - (note.pitch as f32 - min_pitch) / span;
                    let ny = y0 + frac * (y1 - y0 - 3.0);
                    canvas.draw_path(
                        &vg::Path::rect(vg::Rect::new(nx0.max(x0), ny, nx1.min(x1), ny + 3.0), None),
                        &paint,
                    );
                }
            }
        }
    }

    /// Wheel: vertical scroll; Shift+wheel or a sideways trackpad swipe:
    /// horizontal; Ctrl/Cmd+wheel: zoom around the pointer.
    fn on_scroll(&mut self, cx: &mut EventContext, x: f32, y: f32) {
        if cx.modifiers().ctrl() || cx.modifiers().logo() {
            let (lx, _) = self.local_pos(cx);
            let factor = 1.0 + (y as f64) * 0.1;
            cx.emit(TimelineEvent::Zoom { cursor_x: lx as f64, factor });
        } else if cx.modifiers().shift() {
            cx.emit(TimelineEvent::ScrollBy { dx: (-y as f64) * 32.0, dy: 0.0 });
        } else {
            cx.emit(TimelineEvent::ScrollBy { dx: (-x as f64) * 32.0, dy: (-y as f64) * 32.0 });
        }
    }

    /// Thin thumbs along the right and bottom edges, only when there's
    /// more to see in that direction.
    fn draw_scrollbars(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let b = cx.bounds();
        let palette = self.theme.get().palette();
        let t = self.transform.get();
        let (cw, ch) = self.content_size();
        let mut paint = vg::Paint::default();
        paint.set_color(palette.bg_400);
        paint.set_anti_alias(true);
        let inset = 2.0;
        if let Some((start, len)) = thumb(b.h, ch, t.scroll_y as f32) {
            let x = b.x + b.w - THUMB_PX - inset;
            let rect = vg::Rect::new(x, b.y + start, x + THUMB_PX, b.y + start + len);
            canvas.draw_path(&vg::Path::rrect(vg::RRect::new_rect_xy(rect, 2.5, 2.5), None), &paint);
        }
        if let Some((start, len)) = thumb(b.w, cw, t.scroll_x as f32) {
            let y = b.y + b.h - THUMB_PX - inset;
            let rect = vg::Rect::new(b.x + start, y, b.x + start + len, y + THUMB_PX);
            canvas.draw_path(&vg::Path::rrect(vg::RRect::new_rect_xy(rect, 2.5, 2.5), None), &paint);
        }
    }

    /// Content size in px (width unscrolled), for the scrollbar thumbs.
    fn content_size(&self) -> (f32, f32) {
        let arr = self.arrangement.get();
        let t = self.transform.get();
        (
            crate::timeline::state::content_width(&arr, &t) as f32,
            crate::timeline::state::content_height(&arr) as f32,
        )
    }
}

/// The breakpoint being live-dragged renders at its in-progress position
/// instead of its (still unmodified) stored one.
fn live_breakpoint(drag: &Option<Drag>, lane_id: AutomationLaneId, bp: &Breakpoint) -> (Ticks, f32) {
    if let Some(Drag::MoveBreakpoint { lane, original, current }) = drag {
        if *lane == lane_id && original.tick == bp.tick && original.value == bp.value {
            return (current.tick, current.value);
        }
    }
    (bp.tick, bp.value)
}

/// Just the playhead line, stacked on top of `LaneArea` instead of drawn
/// as part of it. During playback the playhead moves every frame; when
/// it lived inside `LaneArea`'s own draw, that meant redrawing every
/// clip, every waveform and the whole grid 60 times a second just to
/// move a 1px line - the actual cause of the jittery playhead. This view
/// is bound to nothing but `transform`/`playhead`/`theme`, so it's the
/// only thing that pays the playback-rate redraw cost. `pointer_events`
/// is off so clicks pass straight through to `LaneArea` underneath.
pub struct PlayheadOverlay {
    transform: Signal<ViewTransform>,
    playhead: Signal<Ticks>,
    theme: Signal<ThemeId>,
}

impl PlayheadOverlay {
    pub fn new(
        cx: &mut Context,
        transform: Signal<ViewTransform>,
        playhead: Signal<Ticks>,
        theme: Signal<ThemeId>,
    ) -> Handle<'_, Self> {
        Self { transform, playhead, theme }
            .build(cx, |_| {})
            .bind(transform, |mut h| h.needs_redraw())
            .bind(playhead, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
            .pointer_events(PointerEvents::None)
    }
}

impl View for PlayheadOverlay {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();
        let transform = self.transform.get();
        let playhead_x = bounds.x + transform.tick_to_x(self.playhead.get()) as f32;
        if playhead_x >= bounds.x && playhead_x <= bounds.x + bounds.w {
            let mut paint = vg::Paint::default();
            paint.set_color(palette.playhead);
            paint.set_anti_alias(true);
            canvas.draw_path(
                &vg::Path::rect(vg::Rect::new(playhead_x, bounds.y, playhead_x + 1.0, bounds.y + bounds.h), None),
                &paint,
            );
        }
    }
}
