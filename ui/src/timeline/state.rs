//! The timeline's central `Model`: owns the arrangement, the undo/redo
//! command stack, the view transform, selection and playhead. Every edit
//! from the ruler/lane canvas views and the track headers arrives here as a
//! `TimelineEvent` and is applied through [`shared::arrangement::Command`].

use std::collections::HashSet;
use std::sync::Arc;

use vizia::prelude::*;

use shared::arrangement::{
    empty_arrangement, snap, step_entry_commit, Arrangement, AutomationLaneId, Breakpoint, Clip,
    ClipColor, ClipContent, ClipId, Command, CommandStack, LoopRange, MidiNote, PeakPyramid,
    SnapGrid, Ticks, Track, TrackId, TrackKind, ViewTransform, PPQ,
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

/// Which thing a click/drag on empty track space does - mirrors the piano
/// roll's own Select/Draw toggle. `Select` is every existing behavior
/// (rubber-band selection); `Draw` is new: click or drag empty space on a
/// MIDI track to create a clip there directly, instead of the only route
/// being step-entry recording.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimelineTool {
    #[default]
    Select,
    Draw,
}

/// One 16th note at 4/4 - the step-entry recorder's fixed grid.
const STEP_TICKS: Ticks = PPQ / 4;

/// A file stem like "hihat_closed" -> "Hihat Closed", for a new track's
/// default name when a drum sample is imported.
pub(crate) fn display_name_from_stem(stem: &str) -> String {
    stem.split(['_', '-'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub struct TimelineState {
    pub arrangement: Signal<Arrangement>,
    pub transform: Signal<ViewTransform>,
    pub snap: Signal<SnapGrid>,
    pub selection: Signal<Selection>,
    pub follow: Signal<bool>,
    pub tool: Signal<TimelineTool>,
    pub playhead_ticks: Signal<Ticks>,
    pub drums_menu_open: Signal<bool>,
    command_stack: CommandStack,

    // Step-entry recording (not reactive): armed via the transport's
    // record toggle, targets whichever MIDI track is armed, active only
    // while the transport is stopped.
    record_armed: Signal<bool>,
    playing: Signal<bool>,
    /// The clip currently being extended, if the next committed step
    /// lands right at its end.
    step_entry_clip: Option<ClipId>,

    // The piano roll's own state (not reactive here): when it's open with
    // a note selection, Delete/Backspace removes those notes instead of
    // the timeline's selected clips - one global shortcut, routed to
    // whichever thing you're actually looking at.
    piano_roll_open_clip: Signal<Option<ClipId>>,
    piano_roll_selected: Signal<HashSet<(Ticks, u8)>>,

    /// Requests a background decode of a newly added audio source (an
    /// imported drum sample, here) so it's audible without restarting -
    /// same persistent worker `RecordingCoordinator` already uses for a
    /// freshly recorded take; `Sender` is `Clone`, so both just hold
    /// their own copy of it. `None` until `set_decode_sender` is called:
    /// the worker itself is only spawned once `TimelineState::new` has
    /// already returned (its initial decode batch needs the starting
    /// arrangement, which needs `Self::arrangement` to exist first), so
    /// this can't be a constructor argument without a circular
    /// dependency.
    decode_request_tx: Option<std::sync::mpsc::Sender<Arc<str>>>,
}

impl TimelineState {
    pub fn new(
        record_armed: Signal<bool>,
        playing: Signal<bool>,
        piano_roll_open_clip: Signal<Option<ClipId>>,
        piano_roll_selected: Signal<HashSet<(Ticks, u8)>>,
    ) -> Self {
        Self {
            arrangement: Signal::new(empty_arrangement()),
            transform: Signal::new(ViewTransform::default()),
            snap: Signal::new(SnapGrid::Sixteenth),
            selection: Signal::new(Selection::default()),
            follow: Signal::new(true),
            tool: Signal::new(TimelineTool::default()),
            playhead_ticks: Signal::new(0),
            drums_menu_open: Signal::new(false),
            command_stack: CommandStack::new(),
            record_armed,
            playing,
            step_entry_clip: None,
            piano_roll_open_clip,
            piano_roll_selected,
            decode_request_tx: None,
        }
    }

    pub fn set_decode_sender(&mut self, tx: std::sync::mpsc::Sender<Arc<str>>) {
        self.decode_request_tx = Some(tx);
    }

    fn with_arrangement(&mut self, f: impl FnOnce(&mut Arrangement, &mut CommandStack)) {
        let mut arr = self.arrangement.get();
        f(&mut arr, &mut self.command_stack);
        self.arrangement.set(arr);
    }

    fn do_command(&mut self, command: Command) {
        self.with_arrangement(|arr, stack| stack.do_command(command, arr));
    }

    /// Commits one step-entry step: zero or more simultaneous `pitches` at
    /// the playhead on the armed MIDI track, each one step (a 16th note)
    /// long, then advances the playhead by one step. A no-op unless
    /// record is armed, the transport is stopped, and some MIDI track is
    /// armed.
    fn commit_step(&mut self, pitches: &HashSet<u8>) {
        if !self.record_armed.get() || self.playing.get() {
            return;
        }
        let track_id = {
            let arr = self.arrangement.get();
            arr.tracks.iter().find(|t| t.arm && t.kind == TrackKind::Midi).map(|t| t.id)
        };
        let Some(track_id) = track_id else { return };

        let playhead = self.playhead_ticks.get();
        let reuse_id = self.step_entry_clip;
        let pitches: Vec<u8> = pitches.iter().copied().collect();
        let mut committed_clip = None;

        self.with_arrangement(|arr, stack| {
            let next_new_id = arr.alloc_id();
            let (command, clip_id) =
                step_entry_commit(arr, track_id, playhead, STEP_TICKS, reuse_id, next_new_id, &pitches);
            stack.do_command(command, arr);
            committed_clip = Some(clip_id);
        });

        self.step_entry_clip = committed_clip;
        self.playhead_ticks.set(playhead + STEP_TICKS);
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
    SetTempo(f64),
    Undo,
    Redo,
    CycleSnap,
    ToggleFollow,
    SetTool(TimelineTool),
    /// A clip drawn directly on empty MIDI-track space (Draw tool), rather
    /// than built up via step-entry recording.
    InsertMidiClip { track: TrackId, start: Ticks, length: Ticks },
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

    /// Emitted by Carve whenever every held note comes back up (or a rest
    /// is played via Space): commits `pitches` (possibly empty, for a
    /// rest) as one step of a step-entry recording.
    CommitStepChord(HashSet<u8>),

    /// The piano roll's own note add/remove (one at a time; a multi-note
    /// delete of the piano roll's selection goes through
    /// `DeleteSelected` instead, batched into one undo step).
    AddMidiNoteAt { clip: ClipId, note: MidiNote },
    RemoveMidiNoteAt { clip: ClipId, start: Ticks, pitch: u8 },

    /// A finished guitar/mic take: insert it as a real clip on `track`,
    /// one undo step, same as any other clip insertion.
    InsertRecordedClip { track: TrackId, start: Ticks, length: Ticks, source: Arc<str> },

    AddTrack(TrackKind),
    /// Imports a drum sample (a `.wav` under `assets/drums/`, named
    /// relative to the assets dir, e.g. `"drums/kick.wav"`) as a new
    /// track - one clip, sized to the sample's own length, at tick 0.
    AddDrumSample(Arc<str>),
    ToggleDrumsMenu,
    RemoveTrack(TrackId),
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
                let piano_roll_selected = self.piano_roll_selected.get();
                if let (Some(clip), false) =
                    (self.piano_roll_open_clip.get(), piano_roll_selected.is_empty())
                {
                    let commands = piano_roll_selected
                        .iter()
                        .map(|&(start, pitch)| Command::RemoveMidiNote { clip, start, pitch })
                        .collect();
                    self.do_command(Command::Batch(commands));
                    cx.emit(crate::piano_roll::state::PianoRollEvent::ClearSelection);
                } else {
                    let selection = self.selection.get();
                    if !selection.clips.is_empty() {
                        let commands =
                            selection.clips.iter().map(|&clip| Command::DeleteClip { clip }).collect();
                        self.do_command(Command::Batch(commands));
                        self.selection.set(Selection::default());
                    }
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
            TimelineEvent::SetTempo(bpm) => {
                self.do_command(Command::SetTempo { bpm: *bpm });
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
            TimelineEvent::SetTool(t) => {
                self.tool.set(*t);
            }
            TimelineEvent::InsertMidiClip { track, start, length } => {
                let bypass = cx.modifiers().alt();
                let snap_grid = self.snap.get();
                let raw_start = *start;
                let raw_end = *start + *length;
                let snapped_start = snap(raw_start, snap_grid, bypass).max(0);
                let snapped_end = snap(raw_end, snap_grid, bypass).max(snapped_start + 1);

                let mut new_id = 0;
                self.with_arrangement(|arr, stack| {
                    new_id = arr.alloc_id();
                    let clip = Clip {
                        id: new_id,
                        track: *track,
                        start: snapped_start,
                        length: snapped_end - snapped_start,
                        name: "Clip".to_string(),
                        content: ClipContent::Midi { notes: vec![] },
                        recording: false,
                    };
                    stack.do_command(Command::InsertClip { clip: Box::new(clip) }, arr);
                });
                self.selection.set(Selection { clips: std::iter::once(new_id).collect(), ..Default::default() });
                // Straight into note entry - drawing an empty clip is
                // only useful as a step toward putting notes in it.
                cx.emit(crate::piano_roll::state::PianoRollEvent::Open(new_id));
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
            TimelineEvent::CommitStepChord(pitches) => {
                self.commit_step(pitches);
            }
            TimelineEvent::AddMidiNoteAt { clip, note } => {
                self.do_command(Command::AddMidiNote { clip: *clip, note: *note });
            }
            TimelineEvent::RemoveMidiNoteAt { clip, start, pitch } => {
                self.do_command(Command::RemoveMidiNote { clip: *clip, start: *start, pitch: *pitch });
            }
            TimelineEvent::InsertRecordedClip { track, start, length, source } => {
                self.with_arrangement(|arr, stack| {
                    let id = arr.alloc_id();
                    let clip = Clip {
                        id,
                        track: *track,
                        start: *start,
                        length: *length,
                        name: "Take".to_string(),
                        content: ClipContent::Audio { source: source.clone(), peaks: None, source_offset_samples: 0 },
                        recording: false,
                    };
                    stack.do_command(Command::InsertClip { clip: Box::new(clip) }, arr);
                });
            }
            TimelineEvent::AddTrack(kind) => {
                const COLORS: [ClipColor; 6] = [
                    ClipColor::Coral,
                    ClipColor::Amber,
                    ClipColor::Teal,
                    ClipColor::Blue,
                    ClipColor::Violet,
                    ClipColor::Pink,
                ];
                self.with_arrangement(|arr, stack| {
                    let id = arr.alloc_id();
                    let index = arr.tracks.len();
                    let color = COLORS[index % COLORS.len()];
                    let name = match kind {
                        TrackKind::Audio => format!("Audio {id}"),
                        TrackKind::Midi => format!("MIDI {id}"),
                    };
                    let track = Track {
                        id,
                        name,
                        color,
                        kind: *kind,
                        mute: false,
                        solo: false,
                        arm: false,
                        gain_db: 0.0,
                        height: 56.0,
                    };
                    stack.do_command(
                        Command::InsertTrack { track: Box::new(track), index, clips: vec![], automation: vec![] },
                        arr,
                    );
                });
            }
            TimelineEvent::AddDrumSample(source) => {
                let assets_dir = crate::timeline::assets_dir();
                let path = assets_dir.join(&**source);
                let Some(duration_seconds) = crate::timeline::peaks_loader::wav_duration_seconds(&path) else {
                    eprintln!("timeline: failed to read {}", path.display());
                    return;
                };
                let name = std::path::Path::new(&**source)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(display_name_from_stem)
                    .unwrap_or_else(|| "Sample".to_string());

                const COLORS: [ClipColor; 6] = [
                    ClipColor::Coral,
                    ClipColor::Amber,
                    ClipColor::Teal,
                    ClipColor::Blue,
                    ClipColor::Violet,
                    ClipColor::Pink,
                ];
                self.with_arrangement(|arr, stack| {
                    let track_id = arr.alloc_id();
                    let clip_id = arr.alloc_id();
                    let index = arr.tracks.len();
                    let color = COLORS[index % COLORS.len()];
                    let length = arr.tempo_map.seconds_to_ticks(duration_seconds).max(1);
                    let track = Track {
                        id: track_id,
                        name: name.clone(),
                        color,
                        kind: TrackKind::Audio,
                        mute: false,
                        solo: false,
                        arm: false,
                        gain_db: 0.0,
                        height: 56.0,
                    };
                    let clip = Clip {
                        id: clip_id,
                        track: track_id,
                        start: 0,
                        length,
                        name: name.clone(),
                        content: ClipContent::Audio { source: source.clone(), peaks: None, source_offset_samples: 0 },
                        recording: false,
                    };
                    stack.do_command(
                        Command::InsertTrack { track: Box::new(track), index, clips: vec![clip], automation: vec![] },
                        arr,
                    );
                });
                crate::timeline::peaks_loader::spawn_peak_loader_for_source(cx, &assets_dir, source.clone());
                if let Some(tx) = &self.decode_request_tx {
                    let _ = tx.send(source.clone());
                }
                self.drums_menu_open.set(false);
            }
            TimelineEvent::ToggleDrumsMenu => {
                self.drums_menu_open.update(|v| *v = !*v);
            }
            TimelineEvent::RemoveTrack(track) => {
                self.do_command(Command::DeleteTrack { track: *track });
                self.selection.set(Selection::default());
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
