//! The lane area: grid, clips (waveform/MIDI), automation, selection wash
//! and playhead. One custom canvas view; draws and hit-tests only the
//! visible tick range and visible rows.

use std::time::{Duration, Instant};

use vizia::prelude::*;
use vizia::vg;

use shared::arrangement::{
    snap, Arrangement, AutomationLaneId, Breakpoint, Clip, ClipContent, ClipId, SnapGrid,
    TimeSignature, Ticks, TrackId, ViewTransform,
};

use crate::recorder::RecordingPreview;
use crate::timeline::header::clip_color_to_rgb;
use crate::timeline::state::{Selection, TimelineEvent};
use crate::tokens::{self, ThemeId};

const CLIP_HEADER_H: f32 = 14.0;
const CLIP_INSET: f32 = 2.0;
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
        rows.push(Row { kind: RowKind::Track(track.id), top: y, height: crate::timeline::LANE_HEIGHT });
        y += crate::timeline::LANE_HEIGHT;
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
}

pub struct LaneArea {
    arrangement: Signal<Arrangement>,
    transform: Signal<ViewTransform>,
    selection: Signal<Selection>,
    playhead: Signal<Ticks>,
    theme: Signal<ThemeId>,
    recording_preview: Signal<Option<RecordingPreview>>,
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
    ) -> Handle<'_, Self> {
        Self { arrangement, transform, selection, playhead, theme, recording_preview, drag: None, last_click: None }
            .build(cx, |_| {})
            .bind(arrangement, |mut h| h.needs_redraw())
            .bind(transform, |mut h| h.needs_redraw())
            .bind(selection, |mut h| h.needs_redraw())
            .bind(playhead, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
            .bind(recording_preview, |mut h| h.needs_redraw())
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
            WindowEvent::MouseMove(x, y) => {
                self.on_mouse_move(cx, *x, *y);
            }
            WindowEvent::MouseUp(button) if *button == MouseButton::Left => {
                self.on_mouse_up(cx);
            }
            WindowEvent::MouseScroll(_x, y) => {
                self.on_scroll(cx, *y);
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        self.draw_impl(cx, canvas);
    }
}

impl LaneArea {
    fn local_pos(&self, cx: &EventContext) -> (f32, f32) {
        let bounds = cx.bounds();
        (cx.mouse().cursor_x - bounds.x, cx.mouse().cursor_y - bounds.y)
    }

    fn on_mouse_down(&mut self, cx: &mut EventContext) {
        let (lx, ly) = self.local_pos(cx);
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
                    let edge = if (lx - start_x).abs() <= EDGE_GRAB_PX {
                        Some(Edge::Start)
                    } else if (lx - end_x).abs() <= EDGE_GRAB_PX {
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
                    if !cx.modifiers().shift() {
                        cx.emit(TimelineEvent::ClearSelection);
                    }
                    self.drag = Some(Drag::RubberBand { anchor: (lx, ly), current: (lx, ly) });
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

    fn on_mouse_move(&mut self, cx: &mut EventContext, x: f32, y: f32) {
        let Some(drag) = &mut self.drag else { return };
        let bounds = cx.bounds();
        let (lx, ly) = (x - bounds.x, y - bounds.y);
        let transform = self.transform.get();
        let arr = self.arrangement.get();
        let bypass = cx.modifiers().alt();
        let snap_grid = SnapGrid::Sixteenth;
        let tick = transform.x_to_tick(lx as f64);

        match drag {
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
        let sig = TimeSignature::FOUR_FOUR;
        let ticks_per_bar = sig.ticks_per_bar();
        let ticks_per_beat = sig.ticks_per_beat();

        let (drag_move, drag_trim, rubber_band) = match &self.drag {
            Some(Drag::MoveClips { clips, delta_ticks, row_delta, .. }) => {
                (Some((clips.as_slice(), *delta_ticks, *row_delta)), None, None)
            }
            Some(Drag::TrimClip { clip, edge, original_start, original_length, delta_ticks }) => {
                (None, Some((*clip, *edge, *original_start, *original_length, *delta_ticks)), None)
            }
            Some(Drag::RubberBand { anchor, current }) => (None, None, Some((*anchor, *current))),
            _ => (None, None, None),
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

        // Clips.
        for row in &rows {
            let RowKind::Track(track_id) = row.kind else { continue };
            let top = row.top - scroll_y;
            if top + row.height < 0.0 || top > bounds.h {
                continue;
            }
            let track_color = arr.track(track_id).map(|t| t.color).unwrap_or(shared::arrangement::ClipColor::Coral);
            for clip in arr.clips_on_track(track_id) {
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
                    }
                }

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
                        };
                        self.draw_clip(canvas, &palette, &preview_clip, track_color, x0, y0, x1, y1, false);
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

        // Playhead, spanning the full lane area.
        let playhead_x = bounds.x + transform.tick_to_x(self.playhead.get()) as f32;
        if playhead_x >= bounds.x && playhead_x <= bounds.x + bounds.w {
            let mut ph_paint = vg::Paint::default();
            ph_paint.set_color(palette.playhead);
            ph_paint.set_anti_alias(true);
            canvas.draw_path(
                &vg::Path::rect(vg::Rect::new(playhead_x, bounds.y, playhead_x + 1.0, bounds.y + bounds.h), None),
                &ph_paint,
            );
        }

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
        let font = crate::canvas_text::canvas_font(10.0);
        canvas.draw_str(&clip.name, vg::Point::new(x0 + 4.0, y0 + 10.0), &font, &text_paint);

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

                let mut path = vg::PathBuilder::new();
                for (i, (_, mx)) in peaks.iter().enumerate() {
                    let x = x0 + i as f32;
                    let y = mid - mx * half_h;
                    if i == 0 {
                        path.move_to(vg::Point::new(x, y));
                    } else {
                        path.line_to(vg::Point::new(x, y));
                    }
                }
                for (i, (mn, _)) in peaks.iter().enumerate().rev() {
                    let x = x0 + i as f32;
                    let y = mid - mn * half_h;
                    path.line_to(vg::Point::new(x, y));
                }
                path.close();

                let mut paint = vg::Paint::default();
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

    fn on_scroll(&mut self, cx: &mut EventContext, y: f32) {
        if cx.modifiers().ctrl() || cx.modifiers().logo() {
            let (lx, _) = self.local_pos(cx);
            let factor = 1.0 + (y as f64) * 0.1;
            cx.emit(TimelineEvent::Zoom { cursor_x: lx as f64, factor });
        } else if cx.modifiers().shift() {
            cx.emit(TimelineEvent::ScrollBy { dx: (-y as f64) * 32.0, dy: 0.0 });
        } else {
            cx.emit(TimelineEvent::ScrollBy { dx: 0.0, dy: (-y as f64) * 32.0 });
        }
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
