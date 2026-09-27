//! The timeline's central `Model`: owns the arrangement, the undo/redo
//! command stack, the view transform, selection and playhead. Every edit
//! from the ruler/lane canvas views and the track headers arrives here as a
//! `TimelineEvent` and is applied through [`shared::arrangement::Command`].

use std::collections::HashSet;
use std::sync::Arc;

use vizia::prelude::*;

use shared::arrangement::{
    CompressorState, Effect, EffectNodeId, EqState, Instrument,
    empty_arrangement, snap, step_entry_commit, Arrangement, AutomationLaneId, Breakpoint, Clip,
    ClipColor, ClipContent, ClipId, Command, CommandStack, LoopRange, Marker, MarkerId, MidiNote,
    PeakPyramid, SnapGrid, Ticks, Track, TrackId, TrackKind, ViewTransform, PPQ,
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
    /// The open right-click menu, if any.
    pub context_menu: Signal<Option<ContextMenu>>,
    /// The marker currently showing an inline rename textbox, if any.
    pub renaming_marker: Signal<Option<MarkerId>>,
    /// The track currently showing an inline rename textbox, if any.
    pub renaming_track: Signal<Option<TrackId>>,
    /// Whether Copy/Cut has put anything aside - so a context menu on
    /// empty space knows whether to offer Paste.
    pub clipboard_nonempty: Signal<bool>,
    /// The lane area's on-screen size in px, reported by `LaneArea`: what
    /// scrolling is clamped against, and what Follow keeps the playhead in.
    viewport: (f64, f64),
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
    /// Where pasted clips land when they all came from one track.
    selected_track: Signal<Option<TrackId>>,
    /// The last Copy/Cut: clips, or (with the piano roll open) notes.
    clipboard: Option<Clipboard>,

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

    /// Drum-pad keys currently physically held down - guards against OS
    /// key-repeat re-triggering `TapDrumPad` dozens of times a second for
    /// one held key (each retrigger used to mean a fresh clip *and* a
    /// fresh full decode-and-rebuild-peaks job with no caching at all).
    held_drum_pads: HashSet<Code>,
}

/// What Copy/Cut put aside. Starts are relative to the earliest item, so
/// a paste keeps the items' spacing wherever it lands.
#[derive(Clone)]
enum Clipboard {
    Clips(Vec<Clip>),
    Notes(Vec<MidiNote>),
}

/// What a right-click context menu is showing actions for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextMenuTarget {
    Clip(ClipId),
    Track(TrackId),
    /// Empty track space: `tick` is where the click landed, for "Paste".
    Lane { track: TrackId, tick: Ticks },
    /// Empty ruler space: `tick` is where the click landed, for "Add
    /// marker here".
    Ruler { tick: Ticks },
    /// An existing marker's own tab.
    Marker { marker: MarkerId },
}

/// A right-click context menu: what it's for, and where to draw it
/// (window-absolute px, from the click).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextMenu {
    pub target: ContextMenuTarget,
    pub x: f32,
    pub y: f32,
}

/// Room below the last row for the "+ Audio track / + MIDI track" actions
/// in the header column, so scrolling can always reach them.
const ADD_ROW_ROOM: f64 = 48.0;

/// The stacked rows' total height, in px (as `LaneArea` lays them out).
pub fn content_height(arr: &Arrangement) -> f64 {
    let tracks: f64 = arr
        .tracks
        .iter()
        .map(|t| t.height.clamp(shared::arrangement::MIN_TRACK_HEIGHT, shared::arrangement::MAX_TRACK_HEIGHT) as f64)
        .sum();
    let lanes = arr
        .automation
        .iter()
        .filter(|l| arr.tracks.iter().any(|t| t.id == l.track))
        .count() as f64
        * crate::timeline::LANE_AUTO_HEIGHT as f64;
    tracks + lanes + ADD_ROW_ROOM
}

/// How far right there's anything to scroll to, in unscrolled px: the end
/// of the last clip plus eight bars of empty room to write into (at least
/// 32 bars).
pub fn content_width(arr: &Arrangement, t: &ViewTransform) -> f64 {
    let bar = shared::arrangement::PPQ * 4;
    let end = arr.clips.iter().map(|c| c.start + c.length).max().unwrap_or(0);
    t.ticks_to_px((end + bar * 8).max(bar * 32))
}

impl TimelineState {
    pub fn new(
        record_armed: Signal<bool>,
        playing: Signal<bool>,
        piano_roll_open_clip: Signal<Option<ClipId>>,
        piano_roll_selected: Signal<HashSet<(Ticks, u8)>>,
        selected_track: Signal<Option<TrackId>>,
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
            context_menu: Signal::new(None),
            renaming_marker: Signal::new(None),
            renaming_track: Signal::new(None),
            clipboard_nonempty: Signal::new(false),
            viewport: (ASSUMED_LANE_WIDTH, 400.0),
            command_stack: CommandStack::new(),
            record_armed,
            playing,
            step_entry_clip: None,
            piano_roll_open_clip,
            piano_roll_selected,
            selected_track,
            clipboard: None,
            decode_request_tx: None,
            held_drum_pads: HashSet::new(),
        }
    }

    /// Copies the piano roll's selected notes if it's open with a
    /// selection, otherwise the selected clips. Returns whether anything
    /// was copied (Cut only deletes when it was).
    fn copy(&mut self) -> bool {
        let arr = self.arrangement.get();
        let notes_selected = self.piano_roll_selected.get();
        if let (Some(clip), false) = (self.piano_roll_open_clip.get(), notes_selected.is_empty()) {
            let Some(ClipContent::Midi { notes }) = arr.clip(clip).map(|c| &c.content) else { return false };
            let chosen: Vec<MidiNote> =
                notes.iter().filter(|n| notes_selected.contains(&(n.start, n.pitch))).copied().collect();
            let first = chosen.iter().map(|n| n.start).min().unwrap_or(0);
            self.clipboard = Some(Clipboard::Notes(
                chosen.into_iter().map(|n| MidiNote { start: n.start - first, ..n }).collect(),
            ));
            self.clipboard_nonempty.set(true);
            return true;
        }
        let selection = self.selection.get();
        let mut clips: Vec<Clip> = selection.clips.iter().filter_map(|id| arr.clip(*id).cloned()).collect();
        if clips.is_empty() {
            return false;
        }
        let first = clips.iter().map(|c| c.start).min().unwrap_or(0);
        for clip in &mut clips {
            clip.start -= first;
            clip.recording = false;
        }
        self.clipboard = Some(Clipboard::Clips(clips));
        self.clipboard_nonempty.set(true);
        true
    }

    /// Pastes at the playhead: notes into the open clip, or clips - onto
    /// the selected track if they all came from one track of the same kind
    /// (so a bassline can be copied to another track), otherwise back onto
    /// their own tracks. One undoable edit; the pasted items are selected.
    fn paste(&mut self, cx: &mut EventContext) {
        let Some(clipboard) = self.clipboard.clone() else { return };
        let playhead = self.playhead_ticks.get().max(0);
        match clipboard {
            Clipboard::Notes(notes) => {
                let Some(clip_id) = self.piano_roll_open_clip.get() else { return };
                let arr = self.arrangement.get();
                let Some(clip) = arr.clip(clip_id) else { return };
                let ClipContent::Midi { notes: existing } = &clip.content else { return };
                let at = (playhead - clip.start).clamp(0, clip.length.max(1) - 1);
                let pasted: Vec<MidiNote> = notes
                    .iter()
                    .map(|n| MidiNote { start: n.start + at, ..*n })
                    .filter(|n| n.start < clip.length)
                    .filter(|n| !existing.iter().any(|e| e.start == n.start && e.pitch == n.pitch))
                    .collect();
                if pasted.is_empty() {
                    return;
                }
                let commands = pasted.iter().map(|&note| Command::AddMidiNote { clip: clip_id, note }).collect();
                self.do_command(Command::Batch(commands));
                cx.emit(crate::piano_roll::state::PianoRollEvent::ClearSelection);
                for n in &pasted {
                    cx.emit(crate::piano_roll::state::PianoRollEvent::SelectNote { key: (n.start, n.pitch), extend: true });
                }
            }
            Clipboard::Clips(clips) => {
                let target = self.selected_track.get();
                let mut new_selection = HashSet::new();
                self.with_arrangement(|arr, stack| {
                    let one_source = clips.iter().all(|c| c.track == clips[0].track);
                    let kind_of = |arr: &Arrangement, id: TrackId| arr.track(id).map(|t| t.kind);
                    let retarget = target.filter(|t| {
                        one_source && kind_of(arr, *t).is_some() && kind_of(arr, *t) == kind_of(arr, clips[0].track)
                    });
                    let commands = clips
                        .iter()
                        .filter_map(|c| {
                            let track = retarget.unwrap_or(c.track);
                            arr.track(track)?;
                            let mut clip = c.clone();
                            clip.id = arr.alloc_id();
                            clip.track = track;
                            clip.start += playhead;
                            new_selection.insert(clip.id);
                            Some(Command::InsertClip { clip: Box::new(clip) })
                        })
                        .collect();
                    stack.do_command(Command::Batch(commands), arr);
                });
                self.selection.set(Selection { clips: new_selection, ..Default::default() });
            }
        }
    }

    /// Keeps the view inside the arrangement: no scrolling past the last
    /// row (plus room for the add-track actions) or far past the last clip.
    fn clamp_scroll(&mut self) {
        let arr = self.arrangement.get();
        let (w, h) = self.viewport;
        self.transform.update(|t| {
            let max_x = content_width(&arr, t) - w;
            let max_y = content_height(&arr) - h;
            t.clamp_scroll(max_x, max_y);
        });
    }

    pub fn set_decode_sender(&mut self, tx: std::sync::mpsc::Sender<Arc<str>>) {
        self.decode_request_tx = Some(tx);
    }

    fn with_arrangement(&mut self, f: impl FnOnce(&mut Arrangement, &mut CommandStack)) {
        let mut arr = self.arrangement.get();
        f(&mut arr, &mut self.command_stack);
        self.arrangement.set(arr);
        // Removing tracks or clips can leave the view scrolled past the end.
        self.clamp_scroll();
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
    RepeatToFillLoop,
    TapDrumPad(usize),
    AddBreakpoint { lane: AutomationLaneId, point: Breakpoint },
    MoveBreakpoint { lane: AutomationLaneId, tick: Ticks, new_tick: Ticks, new_value: f32 },
    RemoveBreakpoint { lane: AutomationLaneId, tick: Ticks },
    SetLoopRange(Option<LoopRange>),
    SetTempo(f64),
    SetTimeSignature { numerator: u8, denominator: u8 },
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
    /// The lane area's size changed (window resize, panel opening).
    SetViewport { width: f64, height: f64 },

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
    /// Ctrl/Cmd+C, X, V: clips on the timeline, or notes when the piano
    /// roll is open with a note selection.
    Copy,
    Cut,
    Paste,
    /// Gives a track an instrument, or removes it (`None`).
    SetInstrument { track: TrackId, instrument: Option<Instrument> },
    /// Adds a Compressor to the track's effect chain - a no-op if it
    /// already has one (only one instance of a given effect makes sense
    /// until there's a real multi-slot chain UI).
    AddCompressorEffect(TrackId),
    RemoveCompressorEffect(TrackId),
    /// Replaces the track's Compressor's whole config - not undoable
    /// (like `SetTrackHeight`/gain, a knob-drag preference, not an edit
    /// worth a history entry), and a no-op if the track has none.
    SetCompressorState(Option<TrackId>, CompressorState),
    /// Same shape as the Compressor trio above, for the EQ.
    AddEqEffect(TrackId),
    RemoveEqEffect(TrackId),
    SetEqState(Option<TrackId>, EqState),
    /// Flips one effect slot's own enabled bit (the `TrackHeaderFx` pip).
    /// `None` targets the master bus's own chain (the pinned row's pips).
    ToggleEffectEnabled(Option<TrackId>, EffectNodeId),
    /// Bypasses (`true`) or restores (`false`) every effect at once -
    /// the header's Alt-click "bypass all". `None` is master.
    SetChainBypassed(Option<TrackId>, bool),
    /// Removes a specific effect node by id - the FxBoard's own delete
    /// (unlike `RemoveCompressorEffect`/`RemoveEqEffect`, which look a
    /// node up by *type*, the board always knows exactly which node).
    /// `None` is master.
    RemoveEffectNodeFromBoard(Option<TrackId>, EffectNodeId),
    /// Appends a new effect node of the given kind - the FxBoard's own
    /// add (from the palette or the empty-canvas search popover).
    /// `None` is master.
    AddEffectNodeToBoard(Option<TrackId>, Effect, Option<(f32, f32)>),
    SetEffectNodePosition(Option<TrackId>, EffectNodeId, (f32, f32)),
    /// The board's port-drag rewire: node, before. `None` is master.
    RewireEffect(Option<TrackId>, EffectNodeId, EffectNodeId),
    /// Drag on a track header's resize handle: absolute new height in px
    /// (clamped by the handler), not undoable - a view preference, like
    /// mute or gain.
    SetTrackHeight { track: TrackId, height: f32 },
    /// A right-click: opens the menu for that target at that position.
    OpenContextMenu(ContextMenu),
    CloseContextMenu,
    /// A structural marker, added at `tick` with a default name.
    AddMarker(Ticks),
    DeleteMarker(MarkerId),
    /// Opens the marker's inline rename textbox.
    BeginRenameMarker(MarkerId),
    /// Commits the rename textbox's current text.
    CommitRenameMarker(MarkerId, String),
    CancelRenameMarker,
    /// A velocity-lane drag ended: one undoable edit, not one per pixel.
    SetNoteVelocity { clip: ClipId, start: Ticks, pitch: u8, velocity: u8 },

    /// A finished guitar/mic take: insert it as a real clip on `track`,
    /// one undo step, same as any other clip insertion.
    InsertRecordedClip { track: TrackId, start: Ticks, length: Ticks, source: Arc<str> },

    /// Opens a track's inline rename textbox.
    BeginRenameTrack(TrackId),
    /// Commits the track rename textbox's current text.
    CommitRenameTrack(TrackId, String),
    CancelRenameTrack,
    AddTrack(TrackKind),
    /// Imports a drum sample (a `.wav` under `assets/drums/`, named
    /// relative to the assets dir, e.g. `"drums/kick.wav"`) as a new
    /// track - one clip, sized to the sample's own length, at tick 0.
    AddDrumSample(Arc<str>),
    /// A built-in multi-bar pattern (index into
    /// `beat_templates::TEMPLATES`) - one or more new tracks, each with
    /// every bar's worth of hits already placed, one undo step.
    AddDrumPattern(usize),
    ToggleDrumsMenu,
    RemoveTrack(TrackId),

    /// A whole different project just got loaded (Open) or a fresh one
    /// started (New): replaces the arrangement outright and drops undo
    /// history, rather than going through `Command` - the old history
    /// belongs to a now-gone arrangement, and reapplying it against this
    /// one would corrupt it.
    LoadArrangement(Arrangement),
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
            TimelineEvent::Copy => {
                self.copy();
            }
            TimelineEvent::Cut => {
                if self.copy() {
                    cx.emit(TimelineEvent::DeleteSelected);
                }
            }
            TimelineEvent::Paste => self.paste(cx),
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
            TimelineEvent::RepeatToFillLoop => {
                let selection = self.selection.get();
                let arr = self.arrangement.get();
                let Some(loop_range) = arr.loop_range else { return };
                let clips: Vec<Clip> = selection.clips.iter().filter_map(|&id| arr.clip(id).cloned()).collect();
                if clips.is_empty() {
                    return;
                }
                // The whole selection's own span, not any one clip's
                // length - a boom-chuck pattern is a kick clip and a
                // snare clip together, and both need to repeat as one
                // unit, staying lined up with each other.
                let pattern_start = clips.iter().map(|c| c.start).min().unwrap();
                let pattern_end = clips.iter().map(|c| c.start + c.length).max().unwrap();
                let raw_length = pattern_end - pattern_start;
                if raw_length <= 0 {
                    return;
                }
                // Round up to a whole number of bars: a repeated group is
                // a per-bar unit, not a beat-filling one - a burst of hits
                // clustered near the start of a bar should repeat once
                // per bar, not once per beat right on the burst's own
                // (much shorter) raw span.
                let ticks_per_bar = arr.tempo_map.time_signature_at(pattern_start).ticks_per_bar();
                let bars = (raw_length + ticks_per_bar - 1) / ticks_per_bar;
                let pattern_length = bars * ticks_per_bar;
                let repeats = (loop_range.end - pattern_end) / pattern_length;
                if repeats < 1 {
                    return;
                }
                let mut new_selection = HashSet::new();
                self.with_arrangement(|arr, stack| {
                    let mut commands = Vec::new();
                    for i in 1..=repeats {
                        for clip in &clips {
                            let new_id = arr.alloc_id();
                            new_selection.insert(new_id);
                            // Not `DuplicateClip`: that clones the source
                            // verbatim, gain_db included, so every repeat
                            // of a drum-pad-tapped pattern would replay
                            // the exact same handful of gain values over
                            // and over - reads as mechanical/looped even
                            // though each hit *within* one repeat still
                            // varies. Each copy gets its own fresh nudge.
                            let mut new_clip = clip.clone();
                            new_clip.id = new_id;
                            new_clip.start = clip.start + pattern_length * i;
                            new_clip.gain_db = random_gain_variation_db();
                            commands.push(Command::InsertClip { clip: Box::new(new_clip) });
                        }
                    }
                    stack.do_command(Command::Batch(commands), arr);
                });
                self.selection.set(Selection { clips: new_selection, ..Default::default() });
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
            TimelineEvent::SetTimeSignature { numerator, denominator } => {
                self.do_command(Command::SetTimeSignature { numerator: *numerator, denominator: *denominator });
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
                        gain_db: 0.0,
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
                // Clicking a clip also selects its track (and so its instrument).
                if let Some(track) = self.arrangement.get().clip(*clip).map(|c| c.track) {
                    cx.emit(crate::synth::state::SynthEvent::SelectTrack(track));
                }
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
                self.clamp_scroll();
            }
            TimelineEvent::ScrollBy { dx, dy } => {
                self.transform.update(|t| {
                    t.scroll_x += dx;
                    t.scroll_y += dy;
                });
                self.clamp_scroll();
            }
            TimelineEvent::SetViewport { width, height } => {
                self.viewport = (*width, *height);
                self.clamp_scroll();
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
                            let width = self.viewport.0;
                            let in_view = (0.0..=width * 0.9).contains(&x);
                            if !in_view {
                                t.scroll_x = (t.scroll_x + x - width * 0.1).max(0.0);
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
            TimelineEvent::SetInstrument { track, instrument } => {
                self.do_command(Command::SetInstrument { track: *track, instrument: *instrument });
            }
            TimelineEvent::AddCompressorEffect(track) => {
                let arr = self.arrangement.get();
                if let Some(t) = arr.track(*track) {
                    if !t.fx.ordered().iter().any(|n| matches!(n.effect, Effect::Compressor(_))) {
                        self.do_command(Command::AddEffectNode {
                            track: Some(*track),
                            effect: Effect::Compressor(CompressorState::default()),
                            position: None,
                        });
                    }
                }
            }
            TimelineEvent::RemoveCompressorEffect(track) => {
                let arr = self.arrangement.get();
                if let Some(t) = arr.track(*track) {
                    if let Some(node) = t.fx.ordered().iter().find(|n| matches!(n.effect, Effect::Compressor(_))) {
                        self.do_command(Command::RemoveEffectNode { track: Some(*track), node: node.id });
                    }
                }
            }
            TimelineEvent::SetCompressorState(track, state) => {
                self.with_arrangement(|arr, _| {
                    if let Some(fx) = arr.fx_mut(*track) {
                        if let Some(node) = fx.nodes.iter_mut().find(|n| matches!(n.effect, Effect::Compressor(_))) {
                            node.effect = Effect::Compressor(*state);
                        }
                    }
                });
            }
            TimelineEvent::AddEqEffect(track) => {
                let arr = self.arrangement.get();
                if let Some(t) = arr.track(*track) {
                    if !t.fx.ordered().iter().any(|n| matches!(n.effect, Effect::Eq(_))) {
                        self.do_command(Command::AddEffectNode {
                            track: Some(*track),
                            effect: Effect::Eq(EqState::default()),
                            position: None,
                        });
                    }
                }
            }
            TimelineEvent::RemoveEqEffect(track) => {
                let arr = self.arrangement.get();
                if let Some(t) = arr.track(*track) {
                    if let Some(node) = t.fx.ordered().iter().find(|n| matches!(n.effect, Effect::Eq(_))) {
                        self.do_command(Command::RemoveEffectNode { track: Some(*track), node: node.id });
                    }
                }
            }
            TimelineEvent::SetEqState(track, state) => {
                self.with_arrangement(|arr, _| {
                    if let Some(fx) = arr.fx_mut(*track) {
                        if let Some(node) = fx.nodes.iter_mut().find(|n| matches!(n.effect, Effect::Eq(_))) {
                            node.effect = Effect::Eq(*state);
                        }
                    }
                });
            }
            TimelineEvent::ToggleEffectEnabled(track, node) => {
                let arr = self.arrangement.get();
                if let Some(fx) = arr.fx(*track) {
                    if let Some(n) = fx.node(*node) {
                        self.do_command(Command::SetEffectEnabled { track: *track, node: *node, enabled: !n.enabled });
                    }
                }
            }
            TimelineEvent::SetChainBypassed(track, bypassed) => {
                let arr = self.arrangement.get();
                if let Some(fx) = arr.fx(*track) {
                    let commands = fx
                        .ordered()
                        .iter()
                        .map(|n| Command::SetEffectEnabled { track: *track, node: n.id, enabled: !bypassed })
                        .collect();
                    self.do_command(Command::Batch(commands));
                }
            }
            TimelineEvent::RemoveEffectNodeFromBoard(track, node) => {
                self.do_command(Command::RemoveEffectNode { track: *track, node: *node });
            }
            TimelineEvent::AddEffectNodeToBoard(track, effect, position) => {
                self.do_command(Command::AddEffectNode { track: *track, effect: *effect, position: *position });
            }
            TimelineEvent::SetEffectNodePosition(track, node, position) => {
                self.do_command(Command::SetEffectNodePosition { track: *track, node: *node, position: *position });
            }
            TimelineEvent::RewireEffect(track, node, before) => {
                self.do_command(Command::RewireEffect { track: *track, node: *node, before: *before });
            }
            TimelineEvent::SetTrackHeight { track, height } => {
                let height = height.clamp(shared::arrangement::MIN_TRACK_HEIGHT, shared::arrangement::MAX_TRACK_HEIGHT);
                self.with_arrangement(|arr, _| {
                    if let Some(t) = arr.track_mut(*track) {
                        t.height = height;
                    }
                });
            }
            TimelineEvent::OpenContextMenu(menu) => {
                self.context_menu.set(Some(*menu));
            }
            TimelineEvent::CloseContextMenu => {
                self.context_menu.set(None);
            }
            TimelineEvent::AddMarker(tick) => {
                let name = format!("Marker {}", self.arrangement.get().markers.len() + 1);
                self.with_arrangement(|arr, stack| {
                    let id = arr.alloc_id();
                    stack.do_command(Command::InsertMarker { marker: Marker { id, position: *tick, name } }, arr);
                });
            }
            TimelineEvent::DeleteMarker(marker) => {
                self.do_command(Command::RemoveMarker { marker: *marker });
                if self.renaming_marker.get() == Some(*marker) {
                    self.renaming_marker.set(None);
                }
            }
            TimelineEvent::BeginRenameMarker(marker) => {
                self.renaming_marker.set(Some(*marker));
            }
            TimelineEvent::CommitRenameMarker(marker, name) => {
                let name = name.trim();
                if !name.is_empty() {
                    self.do_command(Command::RenameMarker { marker: *marker, name: name.to_string() });
                }
                self.renaming_marker.set(None);
            }
            TimelineEvent::CancelRenameMarker => {
                self.renaming_marker.set(None);
            }
            TimelineEvent::SetNoteVelocity { clip, start, pitch, velocity } => {
                self.do_command(Command::SetNoteVelocity { clip: *clip, start: *start, pitch: *pitch, velocity: *velocity });
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
                        gain_db: 0.0,
                    };
                    stack.do_command(Command::InsertClip { clip: Box::new(clip) }, arr);
                });
                // Without this the clip sits at `peaks: None` - and so
                // waveform-less - until the project is next reloaded, which
                // is what the peaks_loader worker normally runs on. A fresh
                // take needs its waveform right away, same as a dropped-in
                // drum sample gets via `AddDrumSample`.
                crate::timeline::peaks_loader::spawn_peak_loader_for_source(cx, &crate::timeline::assets_dir(), source.clone());
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
                let mut new_track = None;
                self.with_arrangement(|arr, stack| {
                    let id = arr.alloc_id();
                    new_track = Some(id);
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
                        height: shared::arrangement::DEFAULT_TRACK_HEIGHT,
                        instrument: Instrument::default_for(*kind),
                        effects: vec![],
                        effect_slots: vec![],
                        fx: shared::arrangement::EffectGraph::new(),
                    };
                    stack.do_command(
                        Command::InsertTrack { track: Box::new(track), index, clips: vec![], automation: vec![] },
                        arr,
                    );
                });
                // A new track is where you're about to work: select it, so
                // a new MIDI track's Carve is right there in the panel.
                if let Some(id) = new_track {
                    cx.emit(crate::synth::state::SynthEvent::SelectTrack(id));
                }
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
                        height: shared::arrangement::DEFAULT_TRACK_HEIGHT,
                        instrument: None,
                        effects: vec![],
                        effect_slots: vec![],
                        fx: shared::arrangement::EffectGraph::new(),
                    };
                    let clip = Clip {
                        id: clip_id,
                        track: track_id,
                        start: 0,
                        length,
                        name: name.clone(),
                        content: ClipContent::Audio { source: source.clone(), peaks: None, source_offset_samples: 0 },
                        recording: false,
                        gain_db: 0.0,
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
            TimelineEvent::AddDrumPattern(template_index) => {
                let Some(template) = crate::timeline::beat_templates::TEMPLATES.get(*template_index) else { return };
                let assets_dir = crate::timeline::assets_dir();

                // Each unique sample's length in ticks, read once - a
                // one-shot's own duration, same as `AddDrumSample`, not a
                // fixed length that would drift if the tempo differs from
                // whatever the sample happens to sound right at.
                let mut lengths: std::collections::HashMap<&str, Ticks> = std::collections::HashMap::new();
                for hit in template.hits {
                    if lengths.contains_key(hit.sample) {
                        continue;
                    }
                    let path = assets_dir.join(hit.sample);
                    let Some(seconds) = crate::timeline::peaks_loader::wav_duration_seconds(&path) else {
                        eprintln!("timeline: failed to read {}", path.display());
                        continue;
                    };
                    lengths.insert(hit.sample, self.arrangement.get().tempo_map.seconds_to_ticks(seconds).max(1));
                }

                self.with_arrangement(|arr, stack| {
                    let mut by_track: Vec<(&str, ClipColor, Vec<&crate::timeline::beat_templates::DrumHit>)> = Vec::new();
                    for hit in template.hits {
                        match by_track.iter_mut().find(|(name, _, _)| *name == hit.track_name) {
                            Some(entry) => entry.2.push(hit),
                            None => by_track.push((hit.track_name, hit.color, vec![hit])),
                        }
                    }
                    let mut commands = Vec::new();
                    for (track_name, color, hits) in by_track {
                        let track_id = arr.alloc_id();
                        let index = arr.tracks.len();
                        let mut clips = Vec::new();
                        for bar in 0..template.bars {
                            for hit in &hits {
                                let Some(&length) = lengths.get(hit.sample) else { continue };
                                let clip_id = arr.alloc_id();
                                let start = bar * (PPQ * 4) + hit.beat * PPQ;
                                clips.push(Clip {
                                    id: clip_id,
                                    track: track_id,
                                    start,
                                    length,
                                    name: track_name.to_string(),
                                    content: ClipContent::Audio {
                                        source: hit.sample.into(),
                                        peaks: None,
                                        source_offset_samples: 0,
                                    },
                                    recording: false,
                                    gain_db: 0.0,
                                });
                            }
                        }
                        let track = Track {
                            id: track_id,
                            name: track_name.to_string(),
                            color,
                            kind: TrackKind::Audio,
                            mute: false,
                            solo: false,
                            arm: false,
                            gain_db: 0.0,
                            height: shared::arrangement::DEFAULT_TRACK_HEIGHT,
                            instrument: None,
                            effects: vec![],
                            effect_slots: vec![],
                            fx: shared::arrangement::EffectGraph::new(),
                        };
                        commands.push(Command::InsertTrack {
                            track: Box::new(track),
                            index,
                            clips,
                            automation: vec![],
                        });
                    }
                    stack.do_command(Command::Batch(commands), arr);
                });

                let mut spawned: HashSet<&str> = HashSet::new();
                for hit in template.hits {
                    if !spawned.insert(hit.sample) {
                        continue;
                    }
                    let source: Arc<str> = hit.sample.into();
                    crate::timeline::peaks_loader::spawn_peak_loader_for_source(cx, &assets_dir, source.clone());
                    if let Some(tx) = &self.decode_request_tx {
                        let _ = tx.send(source);
                    }
                }
                self.drums_menu_open.set(false);
            }
            TimelineEvent::ToggleDrumsMenu => {
                self.drums_menu_open.update(|v| *v = !*v);
            }
            TimelineEvent::RemoveTrack(track) => {
                self.do_command(Command::DeleteTrack { track: *track });
                self.selection.set(Selection::default());
                if self.renaming_track.get() == Some(*track) {
                    self.renaming_track.set(None);
                }
            }
            TimelineEvent::BeginRenameTrack(track) => {
                self.renaming_track.set(Some(*track));
            }
            TimelineEvent::CommitRenameTrack(track, name) => {
                let name = name.trim();
                if !name.is_empty() {
                    self.do_command(Command::RenameTrack { track: *track, name: name.to_string() });
                }
                self.renaming_track.set(None);
            }
            TimelineEvent::CancelRenameTrack => {
                self.renaming_track.set(None);
            }
            TimelineEvent::LoadArrangement(arrangement) => {
                self.arrangement.set(arrangement.clone());
                self.command_stack = CommandStack::new();
                self.selection.set(Selection::default());
                self.playhead_ticks.set(0);
                self.clipboard = None;
                self.clipboard_nonempty.set(false);
                self.context_menu.set(None);
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
            TimelineEvent::TapDrumPad(index) => {
                let Some(pad) = crate::timeline::drum_pads::DRUM_PADS.get(*index).copied() else { return };
                let assets_dir = crate::timeline::assets_dir();
                let path = assets_dir.join(pad.sample);
                let Some(duration_seconds) = crate::timeline::peaks_loader::wav_duration_seconds(&path) else {
                    eprintln!("timeline: failed to read {}", path.display());
                    return;
                };
                let playhead = self.playhead_ticks.get();
                let source: Arc<str> = Arc::from(pad.sample);
                // A drum pad hits the same handful of samples over and
                // over - reuse another clip's already-loaded peaks
                // instead of re-decoding this source from disk every
                // single tap (that unconditional reload, combined with no
                // guard against OS key-repeat, is what turned a couple of
                // seconds of a held key into a pile of clips and a
                // backlog of redundant decode jobs).
                let cached_peaks = self.arrangement.get().clips.iter().find_map(|c| match &c.content {
                    ClipContent::Audio { source: s, peaks: Some(p), .. } if *s == source => Some(p.clone()),
                    _ => None,
                });
                self.with_arrangement(|arr, stack| {
                    let length = arr.tempo_map.seconds_to_ticks(duration_seconds).max(1);
                    let clip_id = arr.alloc_id();
                    let new_clip = |track_id: TrackId| Clip {
                        id: clip_id,
                        track: track_id,
                        start: playhead,
                        length,
                        name: pad.track_name.to_string(),
                        content: ClipContent::Audio {
                            source: source.clone(),
                            peaks: cached_peaks.clone(),
                            source_offset_samples: 0,
                        },
                        recording: false,
                        // A touch of per-hit level variation so the same
                        // sample struck over and over doesn't sound like
                        // the exact same recording played back-to-back -
                        // real hits are never that identical.
                        gain_db: random_gain_variation_db(),
                    };
                    if let Some(track_id) = arr.tracks.iter().find(|t| t.name == pad.track_name).map(|t| t.id) {
                        stack.do_command(Command::InsertClip { clip: Box::new(new_clip(track_id)) }, arr);
                    } else {
                        let track_id = arr.alloc_id();
                        let index = arr.tracks.len();
                        let track = Track {
                            id: track_id,
                            name: pad.track_name.to_string(),
                            color: pad.color,
                            kind: TrackKind::Audio,
                            mute: false,
                            solo: false,
                            arm: false,
                            gain_db: 0.0,
                            height: shared::arrangement::DEFAULT_TRACK_HEIGHT,
                            instrument: None,
                            effects: vec![],
                            effect_slots: vec![],
                            fx: shared::arrangement::EffectGraph::new(),
                        };
                        stack.do_command(
                            Command::InsertTrack {
                                track: Box::new(track),
                                index,
                                clips: vec![new_clip(track_id)],
                                automation: vec![],
                            },
                            arr,
                        );
                    }
                });
                if cached_peaks.is_none() {
                    crate::timeline::peaks_loader::spawn_peak_loader_for_source(cx, &assets_dir, source.clone());
                }
                if let Some(tx) = &self.decode_request_tx {
                    let _ = tx.send(source.clone());
                }
            }
        });

        event.map(|window_event, _| match window_event {
            WindowEvent::KeyDown(code, _) => {
                if cx.modifiers().is_empty() && self.held_drum_pads.insert(*code) {
                    if let Some(index) = crate::timeline::drum_pads::DRUM_PADS.iter().position(|p| p.key == *code) {
                        cx.emit(TimelineEvent::TapDrumPad(index));
                    }
                }
            }
            WindowEvent::KeyUp(code, _) => {
                self.held_drum_pads.remove(code);
            }
            _ => {}
        });
    }
}

/// A small +-2 dB nudge for one drum-pad hit - not cryptographic, just
/// enough that back-to-back taps of the same sample don't sound like the
/// exact same recording playing twice. Seeded from the clock rather than
/// a stored RNG state since this fires from user input, not the audio
/// callback - no real-time-safety constraint to satisfy here.
fn random_gain_variation_db() -> f32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let unit = (nanos % 1000) as f32 / 1000.0; // 0..1
    (unit - 0.5) * 4.0 // -2..2
}
