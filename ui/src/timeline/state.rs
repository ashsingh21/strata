//! The timeline's central `Model`: owns the arrangement, the undo/redo
//! command stack, the view transform, selection and playhead. Every edit
//! from the ruler/lane canvas views and the track headers arrives here as a
//! `TimelineEvent` and is applied through [`shared::arrangement::Command`].

use std::collections::HashSet;
use std::sync::Arc;

use vizia::prelude::*;

use shared::arrangement::{
    AutomationTarget, CompressorState, Effect, EffectNodeId, EqState, Instrument,
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
            // Fix-ups for words the bundled sample packs spell or case
            // badly - the files keep their names (saved projects reference
            // them by path), only what's shown changes.
            match w.to_ascii_lowercase().as_str() {
                "efect" => return "Effect".to_string(),
                "bpm" => return "BPM".to_string(),
                _ => {}
            }
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
    /// The open right-click menu, if any.
    pub context_menu: Signal<Option<ContextMenu>>,
    /// The marker currently showing an inline rename textbox, if any.
    pub renaming_marker: Signal<Option<MarkerId>>,
    /// The track currently showing an inline rename textbox, if any.
    pub renaming_track: Signal<Option<TrackId>>,
    /// The clip whose rename box is open, and where to show it (window
    /// coordinates: the click that asked for it).
    pub renaming_clip: Signal<Option<(ClipId, f32, f32)>>,
    /// Whether Copy/Cut has put anything aside - so a context menu on
    /// empty space knows whether to offer Paste.
    pub clipboard_nonempty: Signal<bool>,
    /// Audio sources that failed to load (missing or unreadable file) -
    /// their clips say so instead of waiting forever on a waveform.
    pub missing_sources: Signal<HashSet<Arc<str>>>,
    /// The Effects Board's selected node (board scope, node), if any - here
    /// rather than local to the board so Delete/Backspace (DeleteSelected)
    /// can remove it. Mutually exclusive with a clip selection (main.rs
    /// clears it when clips get selected, the board clears clips when a
    /// node does), and reset whenever the board opens/closes/switches.
    pub fx_selected: Signal<Option<(Option<TrackId>, EffectNodeId)>>,
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
#[derive(Clone, Copy, Debug, PartialEq)]
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
    /// A parameter's control (an effect knob): offers "Automate <param>".
    /// `current` is the parameter's value when the arrangement can't know
    /// it (a Carve knob lives in the synth patch).
    Param { track: TrackId, target: AutomationTarget, current: Option<f32> },
    /// An automation lane's header: offers "Remove automation lane".
    AutomationLane { lane: AutomationLaneId },
    /// A track's Automate button: every knob it can automate.
    Automate { track: TrackId },
}

/// A right-click context menu: what it's for, and where to draw it
/// (window-absolute px, from the click).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextMenu {
    pub target: ContextMenuTarget,
    pub x: f32,
    pub y: f32,
}

/// Room below the last row for the Add (Audio / MIDI / Drums) row in the
/// header column, so scrolling can always reach it. The headers' viewport
/// is the lanes' height less the Master row pinned under it, so that's
/// counted too - without it the Add row stayed hidden behind Master.
const ADD_ROW_ROOM: f64 =
    (crate::tokens::SIZE_CONTROL + crate::tokens::SPACE_3 + crate::tokens::SIZE_TOOLBAR + crate::tokens::SPACE_2) as f64;

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
            context_menu: Signal::new(None),
            renaming_marker: Signal::new(None),
            renaming_track: Signal::new(None),
            renaming_clip: Signal::new(None),
            clipboard_nonempty: Signal::new(false),
            missing_sources: Signal::new(HashSet::new()),
            fx_selected: Signal::new(None),
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
            let Some(ClipContent::Midi { notes, .. }) = arr.clip(clip).map(|c| &c.content) else { return false };
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
                let ClipContent::Midi { notes: existing, .. } = &clip.content else { return };
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
                            // Pasted copies are independent, as in other DAWs.
                            let mut clip = c.clone().unlinked();
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
    /// The piano roll's pattern control: makes a MIDI clip loop a pattern
    /// `bars` long (at least 1), growing the clip if it's shorter.
    SetPatternBars { clip: ClipId, bars: i64 },
    /// A MIDI clip to exactly `bars` bars long (typed in the clip editor):
    /// longer repeats its pattern, shorter trims it.
    SetClipBars { clip: ClipId, bars: i64 },
    SplitAtPlayhead,
    DeleteSelected,
    DuplicateSelected,
    /// Duplicates the selected MIDI clips as linked copies: editing the
    /// notes of any of them edits them all.
    DuplicateLinked,
    /// Makes the selected clips independent of their link groups.
    UnlinkSelected,
    RepeatToFillLoop,
    TapDrumPad(usize),
    AddBreakpoint { lane: AutomationLaneId, point: Breakpoint },
    MoveBreakpoint { lane: AutomationLaneId, tick: Ticks, new_tick: Ticks, new_value: f32 },
    RemoveBreakpoint { lane: AutomationLaneId, tick: Ticks },
    SetLoopRange(Option<LoopRange>),
    SetTempo(f64),
    SetTimeSignature { numerator: u8, denominator: u8 },
    /// A whole new arrangement as one undoable edit (a lesson's "Show me").
    ReplaceArrangement(Box<Arrangement>),
    Undo,
    Redo,
    SetSnap(SnapGrid),
    /// Small, repeatable random changes to every note's strength and
    /// timing, so a programmed part sounds played. One undo step.
    HumanizeClip(ClipId),
    /// A Drum Kit pad's mute, level or tuning (like a knob, not an undo
    /// step).
    SetDrumPad { track: TrackId, pad: usize, settings: shared::drums::PadSettings },
    /// Which kit a Drum Kit track plays (its pads keep their settings).
    SetDrumKit { track: TrackId, kit: shared::drums::Kit },
    /// Mute or unmute one pad (a click on its row's name in the step grid).
    ToggleDrumPadMute { track: TrackId, pad: usize },
    /// A MIDI clip's swing, 0 (straight) to 1 (see `Clip::swing`). Like
    /// a knob, not an undo step.
    SetClipSwing { clip: ClipId, swing: f32 },
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
    /// A source's file couldn't be read (see `peaks_loader`).
    SourceMissing(Arc<str>),

    /// Emitted by Carve whenever every held note comes back up (or a rest
    /// is played via Space): commits `pitches` (possibly empty, for a
    /// rest) as one step of a step-entry recording.
    CommitStepChord(HashSet<u8>),

    /// The piano roll's own note add/remove. A Draw click adds one note
    /// or a whole chord - every note that isn't already there, as one undo
    /// step; a multi-note delete of the selection goes through
    /// `DeleteSelected` instead.
    AddMidiNotesAt { clip: ClipId, notes: Vec<MidiNote> },
    RemoveMidiNoteAt { clip: ClipId, start: Ticks, pitch: u8 },
    /// A paint stroke that erased steps: one undo step.
    RemoveMidiNotes { clip: ClipId, notes: Vec<(Ticks, u8)> },
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
    /// Same shape as the Compressor trio above, for the EQ.
    AddEqEffect(TrackId),
    RemoveEqEffect(TrackId),
    /// Replaces one effect node's whole config (a panel knob moved) - not
    /// undoable, like `SetTrackHeight`/gain: a knob-drag, not an edit worth
    /// a history entry. `None` track is the master bus.
    SetEffectState(Option<TrackId>, EffectNodeId, Effect),
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
    /// Creates a lane for `target` on `track` (one breakpoint holding the
    /// parameter's current value, so the sound doesn't change) - or does
    /// nothing if that lane already exists.
    AutomateParam { track: TrackId, target: AutomationTarget, current: Option<f32> },
    RemoveAutomationLane(AutomationLaneId),
    CloseContextMenu,
    /// A structural marker, added at `tick` with a default name.
    AddMarker(Ticks),
    DeleteMarker(MarkerId),
    /// Opens the marker's inline rename textbox.
    BeginRenameMarker(MarkerId),
    /// Commits the rename textbox's current text.
    CommitRenameMarker(MarkerId, String),
    CancelRenameMarker,
    /// A velocity-lane drag ended: its notes (a chord's, usually) to one
    /// velocity, as one undo step.
    SetNoteVelocities { clip: ClipId, notes: Vec<(Ticks, u8)>, velocity: u8 },
    /// Notes (start, pitch) given new lengths, as one undo step.
    SetNoteLengths { clip: ClipId, notes: Vec<(Ticks, u8, Ticks)> },
    /// The piano roll's selected notes moved `octaves` octaves, or
    /// `steps` notes along the scale (`key` + `mask`), as one undo step.
    /// Nothing moves if any would leave MIDI's range or land on a note.
    MoveSelectedNotes { octaves: i32, steps: i32, key: u8, mask: u16 },

    /// A finished guitar/mic take: insert it as a real clip on `track`,
    /// one undo step, same as any other clip insertion.
    InsertRecordedClip { track: TrackId, start: Ticks, length: Ticks, source: Arc<str> },

    /// Opens a track's inline rename textbox.
    BeginRenameTrack(TrackId),
    /// Commits the track rename textbox's current text.
    CommitRenameTrack(TrackId, String),
    CancelRenameTrack,
    /// Opens the clip rename box at window position (`x`, `y`).
    BeginRenameClip { clip: ClipId, x: f32, y: f32 },
    CommitRenameClip(ClipId, String),
    CancelRenameClip,
    AddTrack(TrackKind),
    /// "+ Drums": a MIDI track with a Drum Kit, named "Drums".
    AddDrumTrack,
    /// "+ Guitar": an armed audio track with the starter amp chain, ready
    /// to play through.
    AddGuitarTrack,
    /// One guitar effect on an audio track, in its place in the chain.
    AddGuitarEffect(TrackId, shared::guitar::GuitarKind),
    /// Imports a drum sample (a `.wav` under `assets/drums/`, named
    /// relative to the assets dir, e.g. `"drums/kick.wav"`) as a new
    /// track - one clip, sized to the sample's own length, at tick 0.
    AddDrumSample(Arc<str>),
    /// A sample dropped from the browser: onto `track` at `start` if it's
    /// an audio track, else on a new track, starting at `start`.
    AddSampleAt { source: Arc<str>, track: Option<TrackId>, start: Ticks },
    /// A new MIDI track playing `instrument`, selected - a browser drop on
    /// empty timeline space.
    AddTrackWith(Option<Instrument>),
    /// A built-in multi-bar pattern (index into
    /// `beat_templates::TEMPLATES`) - one or more new tracks, each with
    /// every bar's worth of hits already placed, one undo step.
    AddDrumPattern(usize),
    RemoveTrack(TrackId),

    /// A whole different project just got loaded (Open) or a fresh one
    /// started (New): replaces the arrangement outright and drops undo
    /// history, rather than going through `Command` - the old history
    /// belongs to a now-gone arrangement, and reapplying it against this
    /// one would corrupt it.
    LoadArrangement(Arrangement),
}

impl TimelineState {
    /// A sample (a `.wav` under the assets folder) as a clip: on `onto` at
    /// `start` if that's an audio track, else on a new track of its own.
    fn add_sample(&mut self, cx: &mut EventContext, source: Arc<str>, onto: Option<TrackId>, start: Ticks) {
                let assets_dir = crate::timeline::assets_dir();
                let path = assets_dir.join(&*source);
                let Some(duration_seconds) = crate::timeline::peaks_loader::wav_duration_seconds(&path) else {
                    tracing::warn!("timeline: failed to read {}", path.display());
                    return;
                };
                let name = std::path::Path::new(&*source)
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
                    // Onto an existing audio track, at the drop point.
                    if let Some(track_id) = onto.filter(|t| arr.track(*t).is_some_and(|t| t.kind == TrackKind::Audio)) {
                        let clip_id = arr.alloc_id();
                        let length = arr.tempo_map.seconds_to_ticks(duration_seconds).max(1);
                        let clip = Clip {
                            id: clip_id,
                            track: track_id,
                            start: start.max(0),
                            length,
                            name: name.clone(),
                            content: ClipContent::Audio { source: source.clone(), peaks: None, source_offset_samples: 0 },
                            recording: false,
                            gain_db: 0.0,
                            swing: 0.0,
                        };
                        stack.do_command(Command::InsertClip { clip: Box::new(clip) }, arr);
                        return;
                    }
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
                        drum_pads: Default::default(),
                    };
                    let clip = Clip {
                        id: clip_id,
                        track: track_id,
                        start: start.max(0),
                        length,
                        name: name.clone(),
                        content: ClipContent::Audio { source: source.clone(), peaks: None, source_offset_samples: 0 },
                        recording: false,
                        gain_db: 0.0,
                        swing: 0.0,
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
                }

    /// A new track at the bottom, named for what it plays, and selected
    /// (it's where the user is about to work).
    fn add_track(&mut self, cx: &mut EventContext, kind: TrackKind, instrument: Option<Instrument>) {
        self.add_track_with(cx, kind, instrument, |_, _| {});
    }

    /// `setup` shapes the new track (name, arm, effects) before it's
    /// inserted, so the whole thing is one undo step.
    fn add_track_with(&mut self, cx: &mut EventContext, kind: TrackKind, instrument: Option<Instrument>, setup: impl FnOnce(&Arrangement, &mut Track)) {
        const COLORS: [ClipColor; 6] =
            [ClipColor::Coral, ClipColor::Amber, ClipColor::Teal, ClipColor::Blue, ClipColor::Violet, ClipColor::Pink];
        let mut new_track = None;
        self.with_arrangement(|arr, stack| {
            let id = arr.alloc_id();
            new_track = Some(id);
            let index = arr.tracks.len();
            let color = COLORS[index % COLORS.len()];
            let name = match kind {
                TrackKind::Midi => arr.name_for_instrument(instrument),
                TrackKind::Audio => arr.next_track_name(kind),
            };
            let track = Track {
                id,
                name,
                color,
                kind,
                mute: false,
                solo: false,
                arm: false,
                gain_db: 0.0,
                height: shared::arrangement::DEFAULT_TRACK_HEIGHT,
                instrument,
                effects: vec![],
                effect_slots: vec![],
                fx: shared::arrangement::EffectGraph::new(),
                drum_pads: Default::default(),
            };
            let mut track = track;
            setup(arr, &mut track);
            stack.do_command(Command::InsertTrack { track: Box::new(track), index, clips: vec![], automation: vec![] }, arr);
        });
        if let Some(id) = new_track {
            cx.emit(crate::synth::state::SynthEvent::SelectTrack(id));
        }
    }
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
                let length = (end - start).max(1);
                let arr = self.arrangement.get();
                let looped = arr.clip(*clip).filter(|c| c.start == start).and_then(|c| c.extended_as_loop(length));
                match looped {
                    Some(looped) => self.do_command(Command::ReplaceClip { clip: Box::new(looped) }),
                    None => self.do_command(Command::TrimClip { clip: *clip, start, length }),
                }
            }
            TimelineEvent::SetClipBars { clip, bars } => {
                let arr = self.arrangement.get();
                let Some(old) = arr.clip(*clip) else { return };
                let bar = arr.tempo_map.time_signature_at(old.start).ticks_per_bar();
                let length = (*bars).max(1) * bar;
                if length == old.length {
                    return;
                }
                match old.extended_as_loop(length) {
                    Some(longer) => self.do_command(Command::ReplaceClip { clip: Box::new(longer) }),
                    None => self.do_command(Command::TrimClip { clip: *clip, start: old.start, length }),
                }
            }
            TimelineEvent::SetPatternBars { clip, bars } => {
                let arr = self.arrangement.get();
                let Some(old) = arr.clip(*clip) else { return };
                let ClipContent::Midi { notes, link, .. } = &old.content else { return };
                let bar = arr.tempo_map.time_signature_at(old.start).ticks_per_bar();
                let len = (*bars).max(1) * bar;
                if old.content_len() == len {
                    return;
                }
                let mut new = old.clone();
                new.length = new.length.max(len);
                new.content = ClipContent::Midi { notes: notes.clone(), loop_len: Some(len), link: *link };
                self.do_command(Command::ReplaceClip { clip: Box::new(new) });
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
                } else if let Some((track, node)) = self.fx_selected.get() {
                    self.do_command(Command::RemoveEffectNode { track, node });
                    self.fx_selected.set(None);
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
            TimelineEvent::DuplicateLinked => {
                let selection = self.selection.get();
                let mut new_selection = HashSet::new();
                self.with_arrangement(|arr, stack| {
                    let mut commands = Vec::new();
                    for &id in &selection.clips {
                        let Some(source) = arr.clip(id).cloned() else { continue };
                        if !matches!(source.content, ClipContent::Midi { .. }) {
                            continue;
                        }
                        // Start a link group if the source isn't in one yet.
                        let link = match source.link() {
                            Some(link) => link,
                            None => {
                                let link = arr.alloc_id();
                                commands.push(Command::ReplaceClip { clip: Box::new(source.clone().linked_to(link)) });
                                link
                            }
                        };
                        let mut copy = source.clone().linked_to(link);
                        copy.id = arr.alloc_id();
                        copy.start = source.end();
                        new_selection.insert(copy.id);
                        commands.push(Command::InsertClip { clip: Box::new(copy) });
                    }
                    if !commands.is_empty() {
                        stack.do_command(Command::Batch(commands), arr);
                    }
                });
                if !new_selection.is_empty() {
                    self.selection.set(Selection { clips: new_selection, ..Default::default() });
                }
            }
            TimelineEvent::UnlinkSelected => {
                let arr = self.arrangement.get();
                let commands: Vec<Command> = self
                    .selection
                    .get()
                    .clips
                    .iter()
                    .filter_map(|&id| arr.clip(id).filter(|c| c.link().is_some()).cloned())
                    .map(|c| Command::ReplaceClip { clip: Box::new(c.unlinked()) })
                    .collect();
                if !commands.is_empty() {
                    self.do_command(Command::Batch(commands));
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
                            let mut new_clip = clip.clone().unlinked();
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
            TimelineEvent::ReplaceArrangement(next) => {
                self.do_command(Command::Replace(next.clone()));
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
            TimelineEvent::SetSnap(grid) => self.snap.set(*grid),
            TimelineEvent::SetDrumPad { track, pad, settings } => {
                // A sample of your own: loaded so the pad can play it.
                if let (Some(sample), Some(tx)) = (settings.sample, &self.decode_request_tx) {
                    let _ = tx.send(sample.into());
                }
                self.with_arrangement(|arr, _| {
                    if let Some(p) = arr.track_mut(*track).and_then(|t| t.drum_pads.get_mut(*pad)) {
                        *p = *settings;
                    }
                });
            }
            TimelineEvent::SetDrumKit { track, kit } => {
                self.with_arrangement(|arr, _| {
                    if let Some(t) = arr.track_mut(*track) {
                        t.drum_pads.kit = *kit;
                    }
                });
            }
            TimelineEvent::ToggleDrumPadMute { track, pad } => {
                self.with_arrangement(|arr, _| {
                    if let Some(p) = arr.track_mut(*track).and_then(|t| t.drum_pads.get_mut(*pad)) {
                        p.mute = !p.mute;
                    }
                });
            }
            TimelineEvent::HumanizeClip(clip) => {
                let arr = self.arrangement.get();
                let Some(c) = arr.clip(*clip) else { return };
                let ClipContent::Midi { notes, .. } = &c.content else { return };
                let len = c.content_len();
                let moved: Vec<(MidiNote, MidiNote)> = notes.iter().map(|n| (*n, humanized(n, len))).collect();
                let removes = moved.iter().map(|(from, _)| Command::RemoveMidiNote { clip: *clip, start: from.start, pitch: from.pitch });
                let adds = moved.iter().map(|(_, to)| Command::AddMidiNote { clip: *clip, note: *to });
                self.do_command(Command::Batch(removes.chain(adds).collect()));
            }
            TimelineEvent::SetClipSwing { clip, swing } => {
                let swing = swing.clamp(0.0, 1.0);
                self.with_arrangement(|arr, _| {
                    if let Some(c) = arr.clips.iter_mut().find(|c| c.id == *clip) {
                        c.swing = swing;
                    }
                });
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
                        content: ClipContent::Midi { notes: vec![], loop_len: None, link: None },
                        recording: false,
                        gain_db: 0.0,
                        swing: 0.0,
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
            TimelineEvent::AddMidiNotesAt { clip, notes } => {
                let arr = self.arrangement.get();
                let Some(ClipContent::Midi { notes: existing, .. }) = arr.clip(*clip).map(|c| &c.content) else { return };
                let commands: Vec<Command> = notes
                    .iter()
                    .filter(|n| !existing.iter().any(|e| e.start == n.start && e.pitch == n.pitch))
                    .map(|n| Command::AddMidiNote { clip: *clip, note: *n })
                    .collect();
                if !commands.is_empty() {
                    self.do_command(Command::Batch(commands));
                }
            }
            TimelineEvent::SetInstrument { track, instrument } => {
                let arr = self.arrangement.get();
                let Some(t) = arr.track(*track) else { return };
                if t.instrument == *instrument {
                    return;
                }
                // A track still called what Strata named it follows its
                // instrument ("MIDI 1" with a Drum Kit becomes "Drums");
                // a name the user typed is left alone.
                let mut commands = vec![Command::SetInstrument { track: *track, instrument: *instrument }];
                if t.kind == TrackKind::Midi && Arrangement::is_automatic_midi_name(&t.name) {
                    let mut others = arr.clone();
                    others.tracks.retain(|o| o.id != *track);
                    let name = others.name_for_instrument(*instrument);
                    if name != t.name {
                        commands.push(Command::RenameTrack { track: *track, name });
                    }
                }
                self.do_command(Command::Batch(commands));
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
            TimelineEvent::SetEffectState(track, node, effect) => {
                self.with_arrangement(|arr, _| {
                    if let Some(n) = arr.fx_mut(*track).and_then(|fx| fx.nodes.iter_mut().find(|n| n.id == *node)) {
                        n.effect = *effect;
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
                // Guitar effects run on an audio track's own chain (and the live monitor); elsewhere they'd have nothing to run on.
                let audio_track = track.is_some_and(|t| self.arrangement.get().track(t).is_some_and(|t| t.kind == TrackKind::Audio));
                if matches!(effect, Effect::Guitar(_)) && !audio_track {
                    return;
                }
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
                // Empty space with nothing to paste has no actions: no menu.
                let nothing = matches!(menu.target, ContextMenuTarget::Lane { .. }) && !self.clipboard_nonempty.get();
                if nothing {
                    crate::context_menu::close(cx);
                } else {
                    crate::context_menu::open(cx);
                }
            }
            TimelineEvent::CloseContextMenu => {
                self.context_menu.set(None);
                crate::context_menu::close(cx);
            }
            TimelineEvent::AddMarker(tick) => {
                let name = format!("Marker {}", self.arrangement.get().markers.len() + 1);
                self.with_arrangement(|arr, stack| {
                    let id = arr.alloc_id();
                    stack.do_command(Command::InsertMarker { marker: Marker { id, position: *tick, name } }, arr);
                });
            }
            TimelineEvent::AutomateParam { track, target, current } => {
                let arr = self.arrangement.get();
                let exists = arr.automation.iter().any(|l| l.track == *track && l.target == Some(*target));
                if let (false, Some(norm), Some(label)) =
                    (exists, current.or_else(|| arr.target_norm(*track, *target)), arr.target_label(*track, *target))
                {
                    let mut id = 0;
                    self.with_arrangement(|arr, _| id = arr.alloc_id());
                    let lane = shared::arrangement::AutomationLane {
                        id,
                        track: *track,
                        parameter_name: label,
                        display_value: String::new(),
                        breakpoints: vec![Breakpoint { tick: 0, value: norm }],
                        target: Some(*target),
                    };
                    let index = arr.automation.len();
                    self.do_command(Command::InsertAutomationLane { lane: Box::new(lane), index });
                }
            }
            TimelineEvent::RemoveAutomationLane(lane) => {
                if self.arrangement.get().automation_lane(*lane).is_some() {
                    self.do_command(Command::RemoveAutomationLane { lane: *lane });
                }
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
            TimelineEvent::SetNoteVelocities { clip, notes, velocity } => {
                let commands = notes
                    .iter()
                    .map(|&(start, pitch)| Command::SetNoteVelocity { clip: *clip, start, pitch, velocity: *velocity })
                    .collect();
                self.do_command(Command::Batch(commands));
            }
            TimelineEvent::SetNoteLengths { clip, notes } => {
                let arr = self.arrangement.get();
                let Some(ClipContent::Midi { notes: existing, .. }) = arr.clip(*clip).map(|c| &c.content) else { return };
                let mut commands = Vec::new();
                for &(start, pitch, length) in notes {
                    let Some(old) = existing.iter().find(|n| n.start == start && n.pitch == pitch) else { continue };
                    commands.push(Command::RemoveMidiNote { clip: *clip, start, pitch });
                    commands.push(Command::AddMidiNote { clip: *clip, note: MidiNote { length: length.max(1), ..*old } });
                }
                if !commands.is_empty() {
                    self.do_command(Command::Batch(commands));
                }
            }
            TimelineEvent::MoveSelectedNotes { octaves, steps, key, mask } => {
                let (Some(clip), selected) = (self.piano_roll_open_clip.get(), self.piano_roll_selected.get()) else { return };
                let arr = self.arrangement.get();
                if selected.is_empty() || crate::piano_roll::grid::is_drum_clip(&arr, clip) {
                    return;
                }
                let Some(ClipContent::Midi { notes, .. }) = arr.clip(clip).map(|c| &c.content) else { return };
                let moved: Option<Vec<(MidiNote, MidiNote)>> = notes
                    .iter()
                    .filter(|n| selected.contains(&(n.start, n.pitch)))
                    .map(|n| {
                        let pitch = if *octaves != 0 {
                            u8::try_from(n.pitch as i32 + 12 * octaves).ok().filter(|p| *p <= 127)
                        } else {
                            shared::theory::scale_step(n.pitch, *key, *mask, *steps)
                        }?;
                        Some((*n, MidiNote { pitch, ..*n }))
                    })
                    .collect();
                let Some(moved) = moved else { return };
                let lands_on_a_note = moved.iter().any(|(_, to)| {
                    !selected.contains(&(to.start, to.pitch)) && notes.iter().any(|n| n.start == to.start && n.pitch == to.pitch)
                });
                if moved.is_empty() || lands_on_a_note {
                    return;
                }
                let removes = moved.iter().map(|(from, _)| Command::RemoveMidiNote { clip, start: from.start, pitch: from.pitch });
                let adds = moved.iter().map(|(_, to)| Command::AddMidiNote { clip, note: *to });
                self.do_command(Command::Batch(removes.chain(adds).collect()));
                cx.emit(crate::piano_roll::state::PianoRollEvent::SetSelection(
                    moved.iter().map(|(_, to)| (to.start, to.pitch)).collect(),
                ));
            }
            TimelineEvent::RemoveMidiNotes { clip, notes } => {
                let commands =
                    notes.iter().map(|&(start, pitch)| Command::RemoveMidiNote { clip: *clip, start, pitch }).collect();
                self.do_command(Command::Batch(commands));
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
                        swing: 0.0,
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
            TimelineEvent::AddTrack(kind) => self.add_track(cx, *kind, Instrument::default_for(*kind)),
            TimelineEvent::AddDrumTrack => self.add_track(cx, TrackKind::Midi, Some(Instrument::Drums)),
            TimelineEvent::AddGuitarTrack => self.add_track_with(cx, TrackKind::Audio, None, |arr, track| {
                track.name = arr.next_named("Guitar");
                track.arm = true;
                for fx in shared::guitar::starter_chain() {
                    track.fx.push_at_end(Effect::Guitar(fx));
                }
            }),
            TimelineEvent::AddGuitarEffect(track, kind) => {
                let arr = self.arrangement.get();
                if let Some(t) = arr.track(*track).filter(|t| t.kind == TrackKind::Audio) {
                    let before = t.fx.guitar_slot(*kind);
                    self.do_command(Command::InsertEffectNode {
                        track: Some(*track),
                        effect: Effect::Guitar(shared::guitar::GuitarFx::new(*kind)),
                        before,
                    });
                }
            }
            TimelineEvent::AddDrumSample(source) => self.add_sample(cx, source.clone(), None, 0),
            TimelineEvent::AddSampleAt { source, track, start } => self.add_sample(cx, source.clone(), *track, *start),
            TimelineEvent::AddTrackWith(instrument) => self.add_track(cx, TrackKind::Midi, *instrument),
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
                        tracing::warn!("timeline: failed to read {}", path.display());
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
                                    swing: 0.0,
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
                            drum_pads: Default::default(),
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
            TimelineEvent::BeginRenameClip { clip, x, y } => {
                self.renaming_clip.set(Some((*clip, *x, *y)));
            }
            TimelineEvent::CommitRenameClip(clip, name) => {
                let name = name.trim();
                let renamed = self
                    .arrangement
                    .get()
                    .clip(*clip)
                    .filter(|c| !name.is_empty() && c.name != name)
                    .map(|c| Clip { name: name.to_string(), ..c.clone() });
                if let Some(renamed) = renamed {
                    self.do_command(Command::ReplaceClip { clip: Box::new(renamed) });
                }
                self.renaming_clip.set(None);
            }
            TimelineEvent::CancelRenameClip => {
                self.renaming_clip.set(None);
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
            TimelineEvent::SourceMissing(source) => {
                self.missing_sources.update(|m| {
                    m.insert(source.clone());
                });
            }
            TimelineEvent::PeaksLoaded { source, peaks } => {
                if self.missing_sources.get().contains(source) {
                    self.missing_sources.update(|m| {
                        m.remove(source);
                    });
                }
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
                    tracing::warn!("timeline: failed to read {}", path.display());
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
                        swing: 0.0,
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
                            drum_pads: Default::default(),
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
                // Not while typing in a text box (search, a track or marker
                // name): the digits there added drum hits to the project.
                if cx.modifiers().is_empty() && !crate::text_input_focused(cx) && self.held_drum_pads.insert(*code) {
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

/// `note`, a little off: strength up to 12 either way, timing up to 12
/// ticks (about a hundredth of a beat) either way, kept inside the
/// pattern; a note on the downbeat stays put. Seeded by the note itself,
/// so the result is repeatable rather than different on every press.
pub(crate) fn humanized(note: &MidiNote, pattern_len: Ticks) -> MidiNote {
    let mut h = (note.start as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (note.pitch as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 31;
    let velocity = (note.velocity as i32 + (h % 25) as i32 - 12).clamp(1, 127) as u8;
    let nudge = ((h >> 16) % 25) as Ticks - 12;
    let start = if note.start == 0 { 0 } else { (note.start + nudge).clamp(0, pattern_len - 1) };
    MidiNote { start, velocity, ..*note }
}
