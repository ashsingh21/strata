//! The timeline's central `Model`: owns the arrangement, the undo/redo
//! command stack, the view transform, selection and playhead. Every edit
//! from the ruler/lane canvas views and the track headers arrives here as a
//! `TimelineEvent` and is applied through [`shared::arrangement::Command`].

use std::collections::HashSet;
use std::sync::Arc;

use vizia::prelude::*;

use shared::arrangement::{
    seed_arrangement, snap, Arrangement, AutomationLaneId, Breakpoint, ClipContent, ClipId,
    Command, CommandStack, LoopRange, PeakPyramid, SnapGrid, Ticks, TrackId, ViewTransform,
};

/// The lane area's viewport width isn't known to the model (Vizia only
/// reports layout to views), so Follow mode pages against this rough
/// estimate rather than the live viewport - close enough at the window size
/// this app opens with, but not exact if the window is resized.
const ASSUMED_LANE_WIDTH: f64 = 900.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Selection {
    pub clips: HashSet<ClipId>,
    /// A time range (start, end), shown as a wash under every visible lane.
    pub time_range: Option<(Ticks, Ticks)>,
    pub breakpoint: Option<(AutomationLaneId, Ticks)>,
}

pub struct TimelineState {
    pub arrangement: Signal<Arrangement>,
    pub transform: Signal<ViewTransform>,
    pub snap: Signal<SnapGrid>,
    pub selection: Signal<Selection>,
    pub follow: Signal<bool>,
    pub playhead_ticks: Signal<Ticks>,
    command_stack: CommandStack,
}

impl TimelineState {
    pub fn new() -> Self {
        Self {
            arrangement: Signal::new(seed_arrangement()),
            transform: Signal::new(ViewTransform::default()),
            snap: Signal::new(SnapGrid::Sixteenth),
            selection: Signal::new(Selection::default()),
            follow: Signal::new(true),
            playhead_ticks: Signal::new(0),
            command_stack: CommandStack::new(),
        }
    }

    fn with_arrangement(&mut self, f: impl FnOnce(&mut Arrangement, &mut CommandStack)) {
        let mut arr = self.arrangement.get();
        f(&mut arr, &mut self.command_stack);
        self.arrangement.set(arr);
    }

    fn do_command(&mut self, command: Command) {
        self.with_arrangement(|arr, stack| stack.do_command(command, arr));
    }
}

impl Default for TimelineState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum TimelineEvent {
    MoveClip { clip: ClipId, track: TrackId, start: Ticks },
    TrimClip { clip: ClipId, start: Ticks, length: Ticks },
    SplitAtPlayhead,
    DeleteSelected,
    DuplicateSelected,
    AddBreakpoint { lane: AutomationLaneId, point: Breakpoint },
    MoveBreakpoint { lane: AutomationLaneId, tick: Ticks, new_tick: Ticks, new_value: f32 },
    RemoveBreakpoint { lane: AutomationLaneId, tick: Ticks },
    SetLoopRange(Option<LoopRange>),
    Undo,
    Redo,
    CycleSnap,
    ToggleFollow,
    ToggleMute(TrackId),
    ToggleSolo(TrackId),
    ToggleArm(TrackId),
    SetTrackGain { track: TrackId, gain_db: f32 },

    SelectClip { clip: ClipId, extend: bool },
    SelectClips(HashSet<ClipId>),
    SetTimeSelection(Option<(Ticks, Ticks)>),
    SelectBreakpoint(Option<(AutomationLaneId, Ticks)>),
    ClearSelection,

    Zoom { cursor_x: f64, factor: f64 },
    ScrollBy { dx: f64, dy: f64 },

    ScrubPlayhead(Ticks),
    /// Sent every frame from the app's central timer: while playing, the
    /// engine's real position drives the playhead; while stopped, the last
    /// scrubbed position (if any) is kept.
    SyncPlayhead { ticks: Ticks, playing: bool },

    PeaksLoaded { source: Arc<str>, peaks: Arc<PeakPyramid> },
}

impl Model for TimelineState {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            TimelineEvent::MoveClip { clip, track, start } => {
                let snap_grid = self.snap.get();
                let bypass = cx.modifiers().alt();
                let start = snap(*start, snap_grid, bypass);
                self.do_command(Command::MoveClip { clip: *clip, track: *track, start });
            }
            TimelineEvent::TrimClip { clip, start, length } => {
                let snap_grid = self.snap.get();
                let bypass = cx.modifiers().alt();
                let start = snap(*start, snap_grid, bypass);
                let end = snap(start + *length, snap_grid, bypass);
                self.do_command(Command::TrimClip { clip: *clip, start, length: (end - start).max(1) });
            }
            TimelineEvent::SplitAtPlayhead => {
                let playhead = self.playhead_ticks.get();
                let arr = self.arrangement.get();
                let mut new_ids = Vec::new();
                let targets: Vec<ClipId> = arr
                    .clips
                    .iter()
                    .filter(|c| c.start < playhead && c.end() > playhead)
                    .map(|c| c.id)
                    .collect();
                if !targets.is_empty() {
                    self.with_arrangement(|arr, stack| {
                        let commands = targets
                            .into_iter()
                            .map(|clip| {
                                let new_id = arr.alloc_id();
                                new_ids.push(new_id);
                                Command::SplitClip { clip, at: playhead, new_id }
                            })
                            .collect();
                        stack.do_command(Command::Batch(commands), arr);
                    });
                }
            }
            TimelineEvent::DeleteSelected => {
                let selection = self.selection.get();
                if !selection.clips.is_empty() {
                    let commands =
                        selection.clips.iter().map(|&clip| Command::DeleteClip { clip }).collect();
                    self.do_command(Command::Batch(commands));
                    self.selection.set(Selection::default());
                }
            }
            TimelineEvent::DuplicateSelected => {
                let selection = self.selection.get();
                if !selection.clips.is_empty() {
                    let mut new_selection = HashSet::new();
                    self.with_arrangement(|arr, stack| {
                        let commands = selection
                            .clips
                            .iter()
                            .filter_map(|&clip| {
                                let length = arr.clip(clip)?.length;
                                let new_id = arr.alloc_id();
                                new_selection.insert(new_id);
                                Some(Command::DuplicateClip { clip, new_id, offset: length })
                            })
                            .collect();
                        stack.do_command(Command::Batch(commands), arr);
                    });
                    self.selection.set(Selection { clips: new_selection, ..Default::default() });
                }
            }
            TimelineEvent::AddBreakpoint { lane, point } => {
                self.do_command(Command::AddBreakpoint { lane: *lane, point: *point });
            }
            TimelineEvent::MoveBreakpoint { lane, tick, new_tick, new_value } => {
                self.do_command(Command::MoveBreakpoint {
                    lane: *lane,
                    tick: *tick,
                    new_tick: *new_tick,
                    new_value: *new_value,
                });
                self.selection.set(Selection {
                    breakpoint: Some((*lane, *new_tick)),
                    ..Default::default()
                });
            }
            TimelineEvent::RemoveBreakpoint { lane, tick } => {
                self.do_command(Command::RemoveBreakpoint { lane: *lane, tick: *tick });
            }
            TimelineEvent::SetLoopRange(range) => {
                self.do_command(Command::SetLoopRange { range: *range });
            }
            TimelineEvent::Undo => {
                self.with_arrangement(|arr, stack| {
                    stack.undo(arr);
                });
            }
            TimelineEvent::Redo => {
                self.with_arrangement(|arr, stack| {
                    stack.redo(arr);
                });
            }
            TimelineEvent::CycleSnap => {
                self.snap.update(|s| *s = s.cycled());
            }
            TimelineEvent::ToggleFollow => {
                self.follow.update(|f| *f = !*f);
            }
            TimelineEvent::ToggleMute(track) => {
                self.with_arrangement(|arr, _| {
                    if let Some(t) = arr.track_mut(*track) {
                        t.mute = !t.mute;
                    }
                });
            }
            TimelineEvent::ToggleSolo(track) => {
                self.with_arrangement(|arr, _| {
                    if let Some(t) = arr.track_mut(*track) {
                        t.solo = !t.solo;
                    }
                });
            }
            TimelineEvent::ToggleArm(track) => {
                self.with_arrangement(|arr, _| {
                    if let Some(t) = arr.track_mut(*track) {
                        t.arm = !t.arm;
                    }
                });
            }
            TimelineEvent::SetTrackGain { track, gain_db } => {
                self.with_arrangement(|arr, _| {
                    if let Some(t) = arr.track_mut(*track) {
                        t.gain_db = *gain_db;
                    }
                });
            }
            TimelineEvent::SelectClip { clip, extend } => {
                self.selection.update(|sel| {
                    if *extend {
                        if !sel.clips.remove(clip) {
                            sel.clips.insert(*clip);
                        }
                    } else {
                        sel.clips = std::iter::once(*clip).collect();
                    }
                    sel.time_range = None;
                    sel.breakpoint = None;
                });
            }
            TimelineEvent::SelectClips(clips) => {
                self.selection.set(Selection { clips: clips.clone(), ..Default::default() });
            }
            TimelineEvent::SetTimeSelection(range) => {
                self.selection.update(|sel| sel.time_range = *range);
            }
            TimelineEvent::SelectBreakpoint(bp) => {
                self.selection.set(Selection { breakpoint: *bp, ..Default::default() });
            }
            TimelineEvent::ClearSelection => {
                self.selection.set(Selection::default());
            }
            TimelineEvent::Zoom { cursor_x, factor } => {
                self.transform.update(|t| t.zoom_at(*cursor_x, *factor));
            }
            TimelineEvent::ScrollBy { dx, dy } => {
                self.transform.update(|t| {
                    t.scroll_x = (t.scroll_x + dx).max(0.0);
                    t.scroll_y = (t.scroll_y + dy).max(0.0);
                });
            }
            TimelineEvent::ScrubPlayhead(ticks) => {
                self.playhead_ticks.set((*ticks).max(0));
            }
            TimelineEvent::SyncPlayhead { ticks, playing } => {
                if *playing {
                    self.playhead_ticks.set(*ticks);
                    if self.follow.get() {
                        self.transform.update(|t| {
                            let x = t.tick_to_x(*ticks);
                            let in_view = (0.0..=ASSUMED_LANE_WIDTH * 0.9).contains(&x);
                            if !in_view {
                                t.scroll_x = (t.scroll_x + x - ASSUMED_LANE_WIDTH * 0.1).max(0.0);
                            }
                        });
                    }
                }
            }
            TimelineEvent::PeaksLoaded { source, peaks } => {
                self.with_arrangement(|arr, _| {
                    for clip in &mut arr.clips {
                        if let ClipContent::Audio { source: clip_source, peaks: slot, .. } =
                            &mut clip.content
                        {
                            if clip_source == source {
                                *slot = Some(peaks.clone());
                            }
                        }
                    }
                });
            }
        });
    }
}
