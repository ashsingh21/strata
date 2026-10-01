//! Carve's `Model`. Numeric knobs share one generic `Update` event (a
//! closure mutating `SynthState`) built at the call site once the knob's
//! own 0..1 position has been mapped to physical units - avoids a
//! multi-dozen-variant event enum for ~25 knobs. Enum/bool controls
//! (waveform, filter type, sync, voice mode, held keys) get their own
//! variants since they aren't naturally a knob position.
//!
//! This model also owns the real-time bridge to the engine's voice
//! renderer: every tick it pushes a fresh [`SynthParams`] snapshot and
//! drains the engine's own peak meter; note on/off (from a mouse click on
//! the on-screen keyboard, or a mapped computer-keyboard key) go straight
//! through as they happen.

use std::collections::{HashMap, HashSet};

use vizia::prelude::*;

use std::collections::BTreeMap;

use shared::arrangement::{Arrangement, Instrument, TrackId, TrackKind};
use shared::synth::{
    seed_synth, ALL_NOTES_OFF, MAX_INSTRUMENTS, FilterType, LfoTarget, NoteEvent, SynthParams, SynthState, SynthTelemetry, VoiceMode, Waveform,
};

use crate::timeline::state::TimelineEvent;

pub enum SynthEvent {
    Update(Box<dyn Fn(&mut SynthState) + Send>),
    SetOsc1Waveform(Waveform),
    SetOsc2Waveform(Waveform),
    ToggleOsc2Sync,
    /// Right-click on a Carve knob: offer "Automate Carve · <param>" for the
    /// selected track (which is whose patch the panel shows).
    OpenAutomateMenu { param: shared::synth::SynthParam, x: f32, y: f32 },
    /// A Carve knob was turned: remembered for the selected track's
    /// Automate button.
    Touched(shared::synth::SynthParam),
    SetFilterType(FilterType),
    SetLfo1Target(LfoTarget),
    SetLfo2Target(LfoTarget),
    SetVoiceMode(VoiceMode),
    /// Mouse press/release on the on-screen keyboard: press-and-hold, like
    /// a real key, not a toggle - a note keeps sounding only as long as
    /// the mouse button stays down on it.
    KeyPress(u8),
    KeyRelease(u8),
    /// Click on an Interval Input pad: toggles the note into/out of the
    /// held chord. Deliberately different from the keyboard's
    /// press-and-hold - a mouse can only press one thing at a time, so
    /// building a multi-note chord one pad at a time needs each click to
    /// latch, not release the moment the button comes up.
    ToggleKey(u8),
    /// Triggered by the timeline's playback scheduler, not by the player -
    /// sounds a note and updates the on-screen keyboard, but doesn't feed
    /// step-entry recording.
    /// From the playback scheduler: (track, pitch, velocity) - played by
    /// that track's own Carve.
    NoteOn(TrackId, u8, u8),
    NoteOff(TrackId, u8),
    /// A track was selected (header click, clip click, clip opened): the
    /// panel now shows and edits that track's instrument.
    SelectTrack(TrackId),
    /// Sidebar's "Carve" / "Drum Kit": gives the selected MIDI track that
    /// instrument (replacing a different one).
    AddInstrumentToSelected(Instrument),
    ToggleHelp,
    /// Replaces the whole patch with a preset (keeping held keys held).
    LoadPreset(fn() -> SynthState),
    /// A preset you saved (by name) - see `user_presets`.
    LoadUserPreset(&'static str),
    /// Saves the patch on screen as a preset of this name.
    SaveUserPreset(String),
    DeleteUserPreset(&'static str),
    /// An LFO pill was pressed: the start of dragging it onto a knob.
    BeginLfoDrag(usize),
    /// An LFO pill was released over a routable knob: patch LFO `.0` there.
    RouteLfo(usize, LfoTarget),
    /// Pushes the latest params snapshot to the engine and drains its peak
    /// meter and LFO phases; `dt` in seconds.
    Tick(f32),
    /// A whole different project's instrument patches just got loaded (or
    /// a fresh, empty set, for a new project): replaces every track's
    /// patch outright. Deselects rather than trying to guess which (if
    /// any) track in the new project corresponds to whatever was selected
    /// a moment ago in the old one.
    LoadPatches(BTreeMap<TrackId, SynthState>),
}

/// The computer-keyboard "typing piano": one row of white keys (Z through
/// Slash) plus their black keys, `+`/`-` shift the whole row by an octave.
const KEYBOARD_BASE_NOTE: i32 = 60; // C4, centred in the on-screen keyboard's 3-octave span.
const MAX_OCTAVE_SHIFT: i8 = 4;

fn key_semitone_offset(code: Code) -> Option<i32> {
    use Code::*;
    Some(match code {
        KeyZ => 0,
        KeyS => 1,
        KeyX => 2,
        KeyD => 3,
        KeyC => 4,
        KeyV => 5,
        KeyG => 6,
        KeyB => 7,
        KeyH => 8,
        KeyN => 9,
        KeyJ => 10,
        KeyM => 11,
        Comma => 12,
        KeyL => 13,
        Period => 14,
        Semicolon => 15,
        Slash => 16,
        _ => return None,
    })
}

const METER_FLOOR_DB: f32 = -60.0;
const METER_DECAY_DB_PER_SEC: f32 = 20.0;

fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0001 {
        -100.0
    } else {
        20.0 * gain.log10()
    }
}

fn db_to_meter_fraction(db: f32) -> f32 {
    ((db - METER_FLOOR_DB) / -METER_FLOOR_DB).clamp(0.0, 1.0)
}

pub struct SynthModel {
    pub state: Signal<SynthState>,
    /// Each LFO's live phase (0..1), as reported by the engine.
    pub lfo1_phase: Signal<f32>,
    pub lfo2_phase: Signal<f32>,
    pub octave_shift: Signal<i8>,
    pub meter_l: Signal<f32>,
    pub meter_r: Signal<f32>,
    /// Every track's meter (L, R), 0..1 like the other meters, with their
    /// fall-off - the track header faders show them. Instrument tracks'
    /// come from `SynthTelemetry`, audio tracks' from `bus_peaks`.
    pub track_levels: Signal<HashMap<TrackId, (f32, f32)>>,
    track_db: HashMap<TrackId, (f32, f32)>,
    bus_peaks: Signal<[(f32, f32); shared::playback::MAX_BUS_TRACKS]>,
    pub help_open: Signal<bool>,
    /// The presets you've saved, alphabetical.
    pub user_presets: Signal<Vec<&'static str>>,
    /// The LFO pill being dragged, if any (drop targets light up).
    pub lfo_drag: Signal<Option<usize>>,
    /// The selected track. `state` is its Carve patch, if it has one.
    pub selected_track: Signal<Option<TrackId>>,
    /// Every instrument track's patch - the selected one kept in step with
    /// `state` - for saving, and for playing the tracks not on screen.
    pub patches: Signal<BTreeMap<TrackId, SynthState>>,
    arrangement: Signal<Arrangement>,
    /// Where automation is evaluated each tick (see `Arrangement::with_automation_at`).
    playhead: Signal<shared::arrangement::Ticks>,
    /// Which engine slot plays which track's Carve.
    slots: [Option<TrackId>; MAX_INSTRUMENTS],
    /// Whose patch `state` currently holds - only ever stored back there,
    /// so selecting a track without an instrument can't leak the previous
    /// track's patch onto it when it later gets one.
    state_track: Option<TrackId>,

    // Engine bridge (not reactive).
    params_tx: rtrb::Producer<SynthParams>,
    note_tx: rtrb::Producer<NoteEvent>,
    telemetry_rx: rtrb::Consumer<SynthTelemetry>,
    meter_db_l: f32,
    meter_db_r: f32,

    /// Physical keys currently held, mapped to the note they triggered -
    /// so a mid-hold octave shift doesn't change an already-sounding note,
    /// and OS key-repeat doesn't retrigger it.
    held_computer_keys: HashMap<Code, u8>,
    /// Pitches played since the last time every key came back up - the
    /// chord that gets committed to a step-entry recording, if one's
    /// running, on full release.
    step_record_pitches: HashSet<u8>,
    rest_held: bool,
}

impl SynthModel {
    pub fn new(
        params_tx: rtrb::Producer<SynthParams>,
        note_tx: rtrb::Producer<NoteEvent>,
        telemetry_rx: rtrb::Consumer<SynthTelemetry>,
        arrangement: Signal<Arrangement>,
        patches: Vec<(TrackId, SynthState)>,
        selected_track: Signal<Option<TrackId>>,
        playhead: Signal<shared::arrangement::Ticks>,
        bus_peaks: Signal<[(f32, f32); shared::playback::MAX_BUS_TRACKS]>,
    ) -> Self {
        // Start on the first track with an instrument, showing its patch.
        let patches: BTreeMap<TrackId, SynthState> = patches.into_iter().collect();
        let first = arrangement.get().tracks.iter().find(|t| t.instrument.is_some()).map(|t| t.id);
        let state = first.and_then(|id| patches.get(&id).cloned()).unwrap_or_else(seed_synth);
        Self {
            selected_track: {
                selected_track.set(first);
                selected_track
            },
            patches: Signal::new(patches),
            arrangement,
            playhead,
            slots: [None; MAX_INSTRUMENTS],
            state_track: first,
            state: Signal::new(state),
            lfo1_phase: Signal::new(0.0),
            lfo2_phase: Signal::new(0.0),
            octave_shift: Signal::new(0),
            meter_l: Signal::new(0.0),
            meter_r: Signal::new(0.0),
            track_levels: Signal::new(HashMap::new()),
            track_db: HashMap::new(),
            bus_peaks,
            help_open: Signal::new(false),
            user_presets: Signal::new(crate::user_presets::list()),
            lfo_drag: Signal::new(None),
            params_tx,
            note_tx,
            telemetry_rx,
            meter_db_l: METER_FLOOR_DB,
            meter_db_r: METER_FLOOR_DB,
            held_computer_keys: HashMap::new(),
            step_record_pitches: HashSet::new(),
            rest_held: false,
        }
    }

    /// Only into a Carve that's actually on screen - never into some other
    /// track's patch.
    fn carve_on_screen(&self) -> bool {
        let selected = self.selected_track.get();
        selected.is_some() && selected == self.state_track && selected.is_some_and(|t| self.has_instrument(t))
    }

    /// Replaces the whole patch (keeping held keys held).
    fn load_preset(&mut self, preset: SynthState) {
        if !self.carve_on_screen() {
            return;
        }
        self.state.update(|s| {
            let held = std::mem::take(&mut s.held_notes);
            *s = preset;
            s.held_notes = held;
        });
    }

    /// The engine slot playing `track`'s Carve, if it has one.
    /// Every track's meter from this tick's peaks: an instrument track's
    /// slot, an audio track's bus (its index in the track list), with the
    /// same rise and fall-off as the master meter.
    fn update_track_levels(&mut self, slot_peaks: &[(f32, f32); MAX_INSTRUMENTS], decay: f32) {
        let bus = self.bus_peaks.get();
        let raw: Vec<(TrackId, (f32, f32))> = self.arrangement.with(|arr| {
            arr.tracks
                .iter()
                .enumerate()
                .map(|(index, t)| {
                    let peak = if t.instrument.is_some() {
                        self.slot_of(t.id).map(|s| slot_peaks[s as usize])
                    } else if t.kind == TrackKind::Audio {
                        bus.get(index).copied()
                    } else {
                        None
                    };
                    (t.id, peak.unwrap_or((0.0, 0.0)))
                })
                .collect()
        });
        let fall = |db: f32, peak: f32| {
            let target = gain_to_db(peak).max(METER_FLOOR_DB);
            if target > db { target } else { (db - decay).max(target) }
        };
        let mut levels = HashMap::with_capacity(raw.len());
        for (id, (l, r)) in raw {
            let db = self.track_db.entry(id).or_insert((METER_FLOOR_DB, METER_FLOOR_DB));
            *db = (fall(db.0, l), fall(db.1, r));
            levels.insert(id, (db_to_meter_fraction(db.0), db_to_meter_fraction(db.1)));
        }
        self.track_db.retain(|id, _| levels.contains_key(id));
        if levels != self.track_levels.get() {
            self.track_levels.set(levels);
        }
    }

    fn slot_of(&self, track: TrackId) -> Option<u8> {
        self.slots.iter().position(|s| *s == Some(track)).map(|i| i as u8)
    }

    /// Sounds a note on `track`'s Carve, lighting the on-screen keyboard
    /// if that's the track on screen - shared by player-triggered notes
    /// and the playback scheduler.
    fn sound_on(&mut self, track: Option<TrackId>, note: u8, velocity: u8) {
        let Some(track) = track else { return };
        if self.selected_track.get() == Some(track) {
            self.state.update(|s| {
                if !s.held_notes.contains(&note) {
                    s.held_notes.push(note);
                }
            });
        }
        if let Some(slot) = self.slot_of(track) {
            let _ = self.note_tx.push(NoteEvent { slot, note, on: true, velocity });
        }
    }

    fn sound_off(&mut self, track: Option<TrackId>, note: u8) {
        let Some(track) = track else { return };
        if self.selected_track.get() == Some(track) {
            self.state.update(|s| s.held_notes.retain(|n| *n != note));
        }
        if let Some(slot) = self.slot_of(track) {
            let _ = self.note_tx.push(NoteEvent { slot, note, on: false, velocity: 0 });
        }
    }

    /// Puts the selected track's patch on screen once it has one (just
    /// selected, or just given a Carve).
    fn show_selected_patch(&mut self) {
        let selected = self.selected_track.get();
        if selected != self.state_track {
            if let Some(patch) = selected.and_then(|t| self.patches.get().get(&t).cloned()) {
                self.state.set(patch);
                self.state_track = selected;
            }
        }
    }

    /// Stores the on-screen patch back into `patches` for its track.
    fn store_state(&mut self) {
        if let Some(track) = self.state_track {
            if self.has_instrument(track) {
                let mut patch = self.state.get();
                patch.held_notes.clear();
                if self.patches.get().get(&track) != Some(&patch) {
                    self.patches.update(|p| {
                        p.insert(track, patch);
                    });
                }
            }
        }
    }

    /// Whether `track` plays through Carve (the only instrument with a patch).
    fn has_instrument(&self, track: TrackId) -> bool {
        self.arrangement.get().track(track).is_some_and(|t| t.instrument == Some(Instrument::Carve))
    }

    /// Keeps engine slots matched to the tracks that have instruments:
    /// frees slots whose track lost its instrument (silencing it), gives
    /// every instrument track a slot and a patch.
    fn sync_slots(&mut self) {
        let arr = self.arrangement.get();
        let wanted: Vec<TrackId> = arr.tracks.iter().filter(|t| t.instrument.is_some()).map(|t| t.id).collect();
        for i in 0..MAX_INSTRUMENTS {
            if let Some(track) = self.slots[i] {
                if !wanted.contains(&track) {
                    self.slots[i] = None;
                    let _ = self.note_tx.push(NoteEvent { slot: i as u8, note: ALL_NOTES_OFF, on: false, velocity: 0 });
                }
            }
        }
        for track in wanted {
            if self.slot_of(track).is_none() {
                if let Some(free) = self.slots.iter().position(Option::is_none) {
                    self.slots[free] = Some(track);
                }
            }
            // Only Carve has a patch; a Drum Kit's sounds are fixed.
            let is_carve = arr.track(track).and_then(|t| t.instrument) == Some(Instrument::Carve);
            if is_carve && !self.patches.get().contains_key(&track) {
                self.patches.update(|p| {
                    p.insert(track, seed_synth());
                });
            }
        }
    }

    /// A note the player actually pressed (mouse or computer keyboard):
    /// sounds it and marks it as part of the in-progress step-entry chord.
    fn note_on(&mut self, note: u8) {
        self.sound_on(self.selected_track.get(), note, shared::arrangement::DEFAULT_VELOCITY);
        self.step_record_pitches.insert(note);
    }

    /// The player-pressed counterpart to `note_on`: once every held note
    /// is back up, commits whatever chord was played to the timeline's
    /// step-entry recorder (a no-op there unless it's actually armed).
    fn note_off(&mut self, cx: &mut EventContext, note: u8) {
        self.sound_off(self.selected_track.get(), note);
        if self.state.get().held_notes.is_empty() && !self.step_record_pitches.is_empty() {
            let pitches = std::mem::take(&mut self.step_record_pitches);
            cx.emit(TimelineEvent::CommitStepChord(pitches));
        }
    }
}

impl Model for SynthModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        // A clip opened from anywhere (a menu, a lesson) brings its track
        // with it: the editor shows only under its own track.
        event.map(|event, _| {
            if let crate::piano_roll::state::PianoRollEvent::Open(clip) = event {
                if let Some(track) = self.arrangement.get().clip(*clip).map(|c| c.track) {
                    cx.emit(SynthEvent::SelectTrack(track));
                }
            }
        });
        event.map(|event, _| match event {
            SynthEvent::Update(f) => self.state.update(|s| f(s)),
            SynthEvent::SetOsc1Waveform(w) => self.state.update(|s| s.osc1.waveform = *w),
            SynthEvent::SetOsc2Waveform(w) => self.state.update(|s| s.osc2.waveform = *w),
            SynthEvent::ToggleOsc2Sync => self.state.update(|s| s.osc2.sync = !s.osc2.sync),
            SynthEvent::Touched(param) => {
                if let Some(skipped) = shared::diag::throttle("carve-knob", std::time::Duration::from_millis(300)) {
                    tracing::debug!(target: "action", skipped, "carve knob: {param:?}");
                }
                if let Some(track) = self.selected_track.get() {
                    crate::context_menu::touched(track, shared::arrangement::AutomationTarget::Synth(*param));
                }
            }
            SynthEvent::OpenAutomateMenu { param, x, y } => {
                let track = self.selected_track.get().filter(|id| {
                    self.arrangement.get().track(*id).is_some_and(|t| t.instrument.is_some())
                });
                if let Some(track) = track {
                    let current = param.norm(&self.state.get());
                    cx.emit(TimelineEvent::OpenContextMenu(crate::timeline::state::ContextMenu {
                        target: crate::timeline::state::ContextMenuTarget::Param {
                            track,
                            target: shared::arrangement::AutomationTarget::Synth(*param),
                            current: Some(current),
                        },
                        x: *x,
                        y: *y,
                    }));
                }
            }
            SynthEvent::SetFilterType(t) => self.state.update(|s| s.filter.filter_type = *t),
            SynthEvent::SetLfo1Target(t) => self.state.update(|s| s.lfo1.target = *t),
            SynthEvent::SetLfo2Target(t) => self.state.update(|s| s.lfo2.target = *t),
            SynthEvent::SetVoiceMode(m) => self.state.update(|s| s.voice_mode = *m),
            SynthEvent::KeyPress(note) => {
                if !self.state.get().held_notes.contains(note) {
                    self.note_on(*note);
                }
            }
            SynthEvent::KeyRelease(note) => {
                if self.state.get().held_notes.contains(note) {
                    self.note_off(cx, *note);
                }
            }
            SynthEvent::ToggleKey(note) => {
                if self.state.get().held_notes.contains(note) {
                    self.note_off(cx, *note);
                } else {
                    self.note_on(*note);
                }
            }
            SynthEvent::NoteOn(track, note, velocity) => self.sound_on(Some(*track), *note, *velocity),
            SynthEvent::NoteOff(track, note) => self.sound_off(Some(*track), *note),
            SynthEvent::SelectTrack(track) => {
                if self.selected_track.get() != Some(*track) {
                    self.store_state();
                    // Release what the old track's keys were holding.
                    for note in self.state.get().held_notes {
                        self.sound_off(self.selected_track.get(), note);
                    }
                    self.selected_track.set(Some(*track));
                    self.show_selected_patch();
                }
            }
            SynthEvent::AddInstrumentToSelected(instrument) => {
                if let Some(track) = self.selected_track.get() {
                    let arr = self.arrangement.get();
                    if let Some(t) = arr.track(track) {
                        if t.kind == TrackKind::Midi && t.instrument != Some(*instrument) {
                            cx.emit(TimelineEvent::SetInstrument { track, instrument: Some(*instrument) });
                        }
                    }
                }
            }
            SynthEvent::ToggleHelp => self.help_open.update(|v| *v = !*v),
            SynthEvent::LoadPreset(build) => self.load_preset(build()),
            SynthEvent::LoadUserPreset(name) => match crate::user_presets::load(name) {
                Some(preset) => self.load_preset(preset),
                None => {
                    tracing::warn!("presets: couldn't read the saved preset \u{201c}{name}\u{201d}");
                    self.user_presets.set(crate::user_presets::list());
                }
            },
            SynthEvent::SaveUserPreset(name) => {
                if self.carve_on_screen() {
                    match crate::user_presets::save(name, &self.state.get()) {
                        Ok(saved) => {
                            self.state.update(|s| s.name = saved);
                            self.user_presets.set(crate::user_presets::list());
                        }
                        Err(e) => tracing::warn!("presets: couldn't save \u{201c}{name}\u{201d}: {e}"),
                    }
                }
            }
            SynthEvent::DeleteUserPreset(name) => {
                crate::user_presets::delete(name);
                self.user_presets.set(crate::user_presets::list());
            }
            SynthEvent::BeginLfoDrag(lfo) => self.lfo_drag.set(Some(*lfo)),
            SynthEvent::RouteLfo(lfo, target) => {
                let target = *target;
                self.state.update(|s| if *lfo == 0 { s.lfo1.target = target } else { s.lfo2.target = target });
            }
            SynthEvent::Tick(dt) => {
                self.sync_slots();
                self.store_state();
                self.show_selected_patch();

                // Meter and LFO scopes follow the track on screen.
                let shown = self.selected_track.get().and_then(|t| self.slot_of(t)).map(|s| s as usize);
                let mut peak_l = 0.0f32;
                let mut peak_r = 0.0f32;
                let mut phases = None;
                let mut slot_peaks = [(0.0f32, 0.0f32); MAX_INSTRUMENTS];
                while let Ok(SynthTelemetry { peaks, lfo_phases }) = self.telemetry_rx.pop() {
                    for (peak, p) in slot_peaks.iter_mut().zip(peaks) {
                        *peak = (peak.0.max(p.0), peak.1.max(p.1));
                    }
                    if let Some(slot) = shown {
                        peak_l = peak_l.max(peaks[slot].0);
                        peak_r = peak_r.max(peaks[slot].1);
                        phases = Some(lfo_phases[slot]);
                    }
                }
                if let Some((p1, p2)) = phases {
                    self.lfo1_phase.set(p1);
                    self.lfo2_phase.set(p2);
                }
                let decay = METER_DECAY_DB_PER_SEC * dt;
                let target_l = gain_to_db(peak_l).max(METER_FLOOR_DB);
                let target_r = gain_to_db(peak_r).max(METER_FLOOR_DB);
                self.meter_db_l = if target_l > self.meter_db_l { target_l } else { (self.meter_db_l - decay).max(target_l) };
                self.meter_db_r = if target_r > self.meter_db_r { target_r } else { (self.meter_db_r - decay).max(target_r) };
                self.meter_l.set(db_to_meter_fraction(self.meter_db_l));
                self.meter_r.set(db_to_meter_fraction(self.meter_db_r));
                self.update_track_levels(&slot_peaks, decay);

                // Every instrument's latest patch to its slot (the engine
                // keeps the latest per slot).
                let patches = self.patches.get();
                // Automated track gain / effect params, as they are at the
                // playhead - never written back to the arrangement.
                let arrangement = self.arrangement.get();
                let arr = arrangement.with_automation_at(self.playhead.get());
                let tick = self.playhead.get();
                for (slot, track) in self.slots.iter().enumerate() {
                    let Some(track) = *track else { continue };
                    if let Some(snapshot) =
                        shared::playback::instrument_params(&arrangement, &arr, track, slot as u8, patches.get(&track), tick)
                    {
                        let _ = self.params_tx.push(snapshot);
                    }
                }
            }
            SynthEvent::LoadPatches(patches) => {
                for note in self.state.get().held_notes {
                    self.sound_off(self.selected_track.get(), note);
                }
                self.patches.set(patches.clone());
                self.selected_track.set(None);
            }
        });

        event.map(|window_event, _| match window_event {
            WindowEvent::KeyDown(code, _) if cx.modifiers().is_empty() && !crate::text_input_focused(cx) => match code {
                Code::Equal => {
                    self.octave_shift.update(|o| *o = (*o + 1).min(MAX_OCTAVE_SHIFT));
                }
                Code::Minus => {
                    self.octave_shift.update(|o| *o = (*o - 1).max(-MAX_OCTAVE_SHIFT));
                }
                // Not Space: Vizia's buttons treat Space (like Enter) as
                // "activate the focused button", so using it here meant a
                // rest could silently re-trigger whatever button last had
                // focus (e.g. Close) instead of - or as well as - recording
                // a rest. Backquote isn't a default activation key for
                // anything.
                Code::Backquote => {
                    if !self.rest_held {
                        self.rest_held = true;
                        // A rest: advances a running step-entry recording by
                        // one step with no note. No-op if nothing's armed.
                        cx.emit(TimelineEvent::CommitStepChord(HashSet::new()));
                    }
                }
                _ => {
                    if let Some(offset) = key_semitone_offset(*code) {
                        if !self.held_computer_keys.contains_key(code) {
                            let note = (KEYBOARD_BASE_NOTE + self.octave_shift.get() as i32 * 12 + offset)
                                .clamp(0, 127) as u8;
                            self.held_computer_keys.insert(*code, note);
                            self.note_on(note);
                        }
                    }
                }
            },
            // A release anywhere ends an LFO drag; a knob under the pointer
            // has already routed it (it sees the release first).
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.lfo_drag.get().is_some() {
                    self.lfo_drag.set(None);
                }
            }
            WindowEvent::KeyUp(code, _) => {
                if *code == Code::Backquote {
                    self.rest_held = false;
                }
                if let Some(note) = self.held_computer_keys.remove(code) {
                    self.note_off(cx, note);
                }
            }
            _ => {}
        });
    }
}

// --- Knob position <-> physical unit mappings, shared by every knob. -----
pub fn lin_inv(value: f32, min: f32, max: f32) -> f32 {
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

pub fn log_inv(value: f32, min: f32, max: f32) -> f32 {
    ((value.max(min).ln() - min.ln()) / (max.ln() - min.ln())).clamp(0.0, 1.0)
}
