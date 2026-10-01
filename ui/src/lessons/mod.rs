//! Interactive lessons: a short course that runs inside the real app. A
//! lesson opens a known starting project (through the unsaved-changes
//! guard, see `ProjectEvent::StartLesson`), then walks through steps that
//! check themselves against app state each frame - nothing to click
//! "Next" on for an action - while the control a step needs glows.
//!
//! The course itself is data (`course::LESSONS`); checks are plain
//! functions of a [`Snapshot`], so each lesson is unit-tested end to end.

pub mod bar;
pub mod panel;
pub mod map;
pub mod match_view;
pub mod course;
pub mod preview;
pub mod show;
pub mod sound_match;

use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use vizia::prelude::*;

use shared::arrangement::{Arrangement, Instrument, TrackId, TrackKind};
use shared::synth::SynthState;

use crate::app::AppEvent;
use crate::piano_roll::state::PianoRollEvent;
use crate::timeline::state::{TimelineEvent, TimelineTool};

/// How long an action step waits before offering its hint.
const HINT_AFTER: Duration = Duration::from_secs(20);

/// Everything a step's check can look at, gathered once per frame while a
/// lesson runs (never when idle).
#[derive(Clone)]
pub struct Snapshot {
    pub arrangement: Arrangement,
    pub selected_track: Option<TrackId>,
    pub playing: bool,
    /// The on-screen Carve patch (its name, and the keys held right now).
    pub synth: SynthState,
    /// The clip open in the editor, if any.
    pub open_clip: Option<shared::arrangement::ClipId>,
    /// Where the playhead is.
    pub playhead: shared::arrangement::Ticks,
    /// In a Sound match challenge: how close the patch is to the target
    /// (0 to 1; 0 elsewhere).
    pub match_score: f32,
    /// The song's key: its home note (0 = C) and scale (bit n = n
    /// semitones above it).
    pub key: u8,
    pub scale_mask: u16,
    /// An export has finished (since the app started).
    pub exported: bool,
    /// The spectrum analyzer is showing.
    pub analyzer_open: bool,
    /// The Snap grid (the timeline's and the clip editor's).
    pub snap: shared::arrangement::SnapGrid,
}

/// A control a step can make glow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    AddMidiTrack,
    AddDrumTrack,
    SidebarInstrument(Instrument),
    /// A Carve preset (its entry in Carve's preset list; the list's
    /// button glows too).
    Preset(&'static str),
    Play,
    PatternPlus,
    /// A track's lane in the timeline (a canvas wash).
    Lane(TrackId),
    /// A row of the open piano roll (a canvas wash).
    PianoRollRow(u8),
    /// A Carve knob.
    Knob(shared::synth::SynthParam),
    /// Oscillator 1 or 2's wave switch.
    OscWave(u8),
    /// Carve's Mono/Poly switch.
    VoiceMode,
    /// Oscillator 2's Sync switch.
    Sync,
    /// The velocity stem at this tick of the open clip (a canvas wash).
    Velocity(shared::arrangement::Ticks),
    /// The piano roll's Snap button.
    Snap,
    /// The drum editor's Swing knob.
    Swing,
    /// The drum editor's Humanize button.
    Humanize,
    /// Carve's filter type switch (LP 24 / LP 12 / BP / HP).
    FilterType,
    /// LFO 1 or 2's pill (drag it onto a knob).
    LfoPill(u8),
    /// A track's Mute / Solo button.
    Mute(TrackId),
    Solo(TrackId),
    /// A bar on the ruler (0-based) - where to click.
    RulerBar(i64),
    /// The Key button (header and clip editor): opens the key menu.
    KeyMenu,
    /// A track's volume: its fader and dB readout.
    Fader(TrackId),
    /// "+ Compressor" / "+ EQ" in the device chain.
    AddCompressor,
    AddEq,
    /// A knob in an effect's panel.
    EffectKnob(shared::arrangement::EffectParam),
    /// An EQ band's on/off button (index into `EqState::bands`).
    EqBand(usize),
    /// The Spectrum button in the header.
    Spectrum,
    /// The project name's menu (File).
    FileMenu,
    /// A track's automation lanes (a canvas wash).
    Automation(TrackId),
    /// A track header's Automate (A) button.
    AutomateButton(TrackId),
}

thread_local! {
    /// The glowing target, readable by any view without threading a prop
    /// through every builder (same approach as `effect_panel`'s
    /// `TRACK_COLOR`). Set once in `main` before the views are built.
    static HIGHLIGHT: Cell<Option<Signal<Option<Target>>>> = const { Cell::new(None) };
}

/// A note the current step will add - drawn as an outline in the piano
/// roll until it's there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ghost {
    pub clip: shared::arrangement::ClipId,
    pub note: shared::arrangement::MidiNote,
}

thread_local! {
    static GHOSTS: Cell<Option<Signal<Vec<Ghost>>>> = const { Cell::new(None) };
}

/// The current step's notes still to place (in clip order), if a lesson
/// is running.
pub fn ghosts_signal() -> Option<Signal<Vec<Ghost>>> {
    GHOSTS.get()
}

/// The current lesson target, if a lesson is running.
pub fn highlighted() -> Option<Target> {
    HIGHLIGHT.get().and_then(|h| h.get())
}

/// The highlight signal itself, for canvases that redraw when it changes.
pub fn highlight_signal() -> Option<Signal<Option<Target>>> {
    HIGHLIGHT.get()
}

pub trait LessonTargetExt {
    /// Glows (`is-lesson-target`) while a lesson step points at `target`.
    fn lesson_target(self, target: Target) -> Self;
    /// Glows while the current target matches `pred` (for a control that
    /// leads to several targets, like a menu's button).
    fn lesson_target_if(self, pred: fn(&Option<Target>) -> bool) -> Self;
}

impl<V: View> LessonTargetExt for Handle<'_, V> {
    fn lesson_target(self, target: Target) -> Self {
        match HIGHLIGHT.get() {
            Some(h) => self.toggle_class("is-lesson-target", h.map(move |t| *t == Some(target))),
            None => self,
        }
    }

    fn lesson_target_if(self, pred: fn(&Option<Target>) -> bool) -> Self {
        match HIGHLIGHT.get() {
            Some(h) => self.toggle_class("is-lesson-target", h.map(move |t| pred(t))),
            None => self,
        }
    }
}

pub enum LessonEvent {
    /// The lesson's starting project has just been loaded: start at step 1.
    Begin(usize),
    Tick,
    /// Finish an info step (the last one ends the lesson).
    Continue,
    /// Skip an action step without doing it.
    Skip,
    Exit,
    /// Play the lesson's goal, or the last step's before / after (again:
    /// stop it).
    Hear(preview::Which),
    /// Do the current step for the learner (undo gives it back).
    ShowMe,
    /// Take back what "Show me" just did and return to that step.
    TryYourself,
    /// A quiz step's answer (an index into its options).
    Answer(usize),
    /// Reread an earlier step (read-only; the current one keeps checking
    /// meanwhile), or `None` to go back to the current step.
    Review(Option<usize>),
    /// Done reading why the step just finished sounds as it does: on to
    /// the next one.
    NextStep,
    /// Show or hide the course map (over the arrangement, while no lesson
    /// runs).
    ShowMap(bool),
    /// Open one of `course::EXPLAINERS` under the bar (again: close it).
    Explain(Option<usize>),
    /// A preview finished rendering; `generation` drops a stale one.
    PreviewReady { generation: u64, which: preview::Which, audio: Arc<[f32]> },
    /// A Sound match measurement finished: the target's, or (with the
    /// patch it measured) yours.
    MatchMeasured { lesson: usize, patch: Option<SynthState>, analysis: Arc<shared::analysis::Analysis> },
}

pub struct LessonModel {
    /// (lesson, step) while a lesson runs.
    pub active: Signal<Option<(usize, usize)>>,
    /// Ids of finished lessons (persisted in settings).
    pub done: Signal<Vec<String>>,
    pub highlight: Signal<Option<Target>>,
    pub hint_visible: Signal<bool>,
    step_started: Instant,
    arrangement: Signal<Arrangement>,
    selected_track: Signal<Option<TrackId>>,
    playing: Signal<bool>,
    synth: Signal<SynthState>,
    open_clip: Signal<Option<shared::arrangement::ClipId>>,
    playhead: Signal<shared::arrangement::Ticks>,
    patches: Signal<BTreeMap<TrackId, SynthState>>,
    key: Signal<u8>,
    scale_mask: Signal<u16>,
    export_status: Signal<String>,
    analyzer_open: Signal<bool>,
    snap: Signal<shared::arrangement::SnapGrid>,
    /// The course map is asked for (it shows while no lesson runs).
    pub map_open: Signal<bool>,
    /// The notes the current step adds (see `Ghost`).
    pub ghosts: Signal<Vec<Ghost>>,
    /// Of those, how many are in place now: (placed, all). `None` for a
    /// step that adds no notes.
    pub ghost_progress: Signal<Option<(usize, usize)>>,
    /// The project with the current step done (its "Show me"), for Hear it.
    example: Option<Snapshot>,
    /// Whether that example sounds different from now (new notes or a
    /// changed sound) - so worth a Hear it button.
    pub has_example: Signal<bool>,
    /// A quiz step's answer that was wrong (cleared on the next step).
    pub quiz_wrong: Signal<Option<usize>>,
    /// The preview playing (for the buttons' labels), and when it ends.
    pub previewing: Signal<Option<preview::Which>>,
    /// Bumped per request, so a slow render can't start after a newer one.
    preview_generation: u64,
    player: crate::preview_player::SharedPlayer,
    /// The player's token for the lesson preview playing.
    preview_token: Option<u64>,
    sample_rate: u32,
    /// The app as it stood when the current step began.
    step_before: Option<preview::Take>,
    /// The step just done (index), before and after it.
    last_change: Option<(usize, preview::Take, preview::Take)>,
    /// Whether the current lesson has a goal to hear.
    pub has_goal: Signal<bool>,
    /// The step whose before / after can be heard (the one just done).
    pub change_step: Signal<Option<usize>>,
    /// Keys "Show me" pressed on the on-screen keyboard, released a
    /// moment later.
    release_keys: Option<(Instant, Vec<u8>)>,
    /// The step "Show me" just did, the patch before it (if it changed
    /// the sound), and whether it made an (undoable) arrangement edit.
    shown: Option<(usize, Option<SynthState>, bool)>,
    /// That step, for the bar's "Try it yourself".
    pub shown_step: Signal<Option<usize>>,
    /// An earlier step being reread, if any (see `LessonEvent::Review`).
    pub reviewing: Signal<Option<usize>>,
    /// The step before the current one was just done and explains itself:
    /// the bar stays on it (what you did, why it sounds so, Before/After)
    /// until Next step - jumping straight to the next instruction lost it.
    pub completed: Signal<bool>,
    /// The explainer open under the bar, if any (an index into
    /// `course::EXPLAINERS`).
    pub explaining: Signal<Option<usize>>,
    /// A lesson just reached its end (reported to the project on the
    /// next tick).
    finished: Option<&'static str>,
    /// When the current step was last checked.
    last_check: Instant,
    /// Sound match: the target and your sound, measured, and how alike.
    pub match_target: Signal<Option<Arc<shared::analysis::Analysis>>>,
    pub match_yours: Signal<Option<Arc<shared::analysis::Analysis>>>,
    pub match_score: Signal<Option<f32>>,
    /// A measurement of yours is running.
    match_measuring: bool,
    /// The patch `match_yours` measured.
    match_measured: Option<SynthState>,
}

/// How often a step checks itself: 15 times a second is instant to a
/// person, and a quarter of the work of checking every frame (each check
/// copies the whole project).
const CHECK_EVERY: Duration = Duration::from_millis(66);


impl LessonModel {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        arrangement: Signal<Arrangement>,
        selected_track: Signal<Option<TrackId>>,
        playing: Signal<bool>,
        synth: Signal<SynthState>,
        open_clip: Signal<Option<shared::arrangement::ClipId>>,
        playhead: Signal<shared::arrangement::Ticks>,
        patches: Signal<BTreeMap<TrackId, SynthState>>,
        key: Signal<u8>,
        scale_mask: Signal<u16>,
        export_status: Signal<String>,
        analyzer_open: Signal<bool>,
        snap: Signal<shared::arrangement::SnapGrid>,
        player: crate::preview_player::SharedPlayer,
        sample_rate: u32,
    ) -> Self {
        let highlight = Signal::new(None);
        HIGHLIGHT.set(Some(highlight));
        let ghosts = Signal::new(Vec::new());
        GHOSTS.set(Some(ghosts));
        Self {
            active: Signal::new(None),
            done: Signal::new(crate::settings::load_lessons_done()),
            highlight,
            hint_visible: Signal::new(false),
            step_started: Instant::now(),
            arrangement,
            selected_track,
            playing,
            synth,
            open_clip,
            playhead,
            patches,
            key,
            scale_mask,
            export_status,
            analyzer_open,
            snap,
            map_open: Signal::new(false),
            ghosts,
            ghost_progress: Signal::new(None),
            example: None,
            has_example: Signal::new(false),
            quiz_wrong: Signal::new(None),
            previewing: Signal::new(None),
            preview_generation: 0,
            player,
            preview_token: None,
            sample_rate,
            step_before: None,
            last_change: None,
            has_goal: Signal::new(false),
            change_step: Signal::new(None),
            release_keys: None,
            shown: None,
            shown_step: Signal::new(None),
            reviewing: Signal::new(None),
            completed: Signal::new(false),
            explaining: Signal::new(None),
            finished: None,
            last_check: Instant::now(),
            match_target: Signal::new(None),
            match_yours: Signal::new(None),
            match_score: Signal::new(None),
            match_measuring: false,
            match_measured: None,
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            arrangement: self.arrangement.get(),
            selected_track: self.selected_track.get(),
            playing: self.playing.get(),
            synth: self.synth.get(),
            // Only a clip that still exists counts as open (the editor
            // closes itself otherwise).
            open_clip: self.open_clip.get().filter(|id| self.arrangement.get().clip(*id).is_some()),
            playhead: self.playhead.get(),
            match_score: self.match_score.get().unwrap_or(0.0),
            key: self.key.get(),
            scale_mask: self.scale_mask.get(),
            exported: self.export_status.get().starts_with("Exported"),
            analyzer_open: self.analyzer_open.get(),
            snap: self.snap.get(),
        }
    }

    fn go_to(&mut self, lesson: usize, step: usize) {
        let steps = course::LESSONS[lesson].steps;
        if step >= steps.len() {
            self.exit();
            return;
        }
        self.active.set(Some((lesson, step)));
        self.reviewing.set(None);
        self.completed.set(false);
        self.explaining.set(None);
        self.step_started = Instant::now();
        self.step_before = Some(preview::take_of(&self.snapshot(), &self.patches.get()));
        if self.last_change.as_ref().is_some_and(|(i, ..)| i + 1 != step) {
            self.last_change = None;
        }
        self.change_step.set(self.last_change.as_ref().map(|(i, ..)| *i));
        // "Try it yourself" is offered only right after the shown step.
        if self.shown.as_ref().is_some_and(|(i, ..)| i + 1 != step) {
            self.shown = None;
        }
        self.shown_step.set(self.shown.as_ref().map(|(i, ..)| *i));
        self.hint_visible.set(false);
        self.quiz_wrong.set(None);
        self.set_highlight(None);
        self.prepare_example(lesson, step);
        // Reaching the closing step is finishing the lesson.
        if step == steps.len() - 1 {
            let id = course::LESSONS[lesson].id.to_string();
            self.finished = Some(course::LESSONS[lesson].id);
            if !self.done.get().contains(&id) {
                self.done.update(|d| d.push(id));
                crate::settings::save_lessons_done(&self.done.get());
            }
        }
    }

    /// Works out the current step done (its "Show me" on a copy of the
    /// project): the notes it adds become ghosts, and if it sounds any
    /// different, Hear it can play it.
    fn prepare_example(&mut self, lesson: usize, step: usize) {
        let now = self.snapshot();
        let example = show::example(&course::LESSONS[lesson], step, &now);
        let ghosts = example.as_ref().map(|done| added_notes(&now.arrangement, &done.arrangement)).unwrap_or_default();
        let sound_changed = example.as_ref().is_some_and(|done| {
            let (mut a, mut b) = (now.synth.clone(), done.synth.clone());
            a.held_notes.clear();
            b.held_notes.clear();
            a != b
        });
        self.has_example.set(!ghosts.is_empty() || sound_changed);
        self.ghost_progress.set((!ghosts.is_empty()).then(|| (placed(&now.arrangement, &ghosts), ghosts.len())));
        if self.ghosts.get() != ghosts {
            self.ghosts.set(ghosts);
        }
        self.example = example;
    }

    fn clear_example(&mut self) {
        self.example = None;
        self.has_example.set(false);
        self.ghost_progress.set(None);
        if !self.ghosts.get().is_empty() {
            self.ghosts.set(Vec::new());
        }
    }

    fn reset_match(&mut self) {
        self.match_target.set(None);
        self.match_yours.set(None);
        self.match_score.set(None);
        self.match_measured = None;
    }

    /// Measures the current patch if it's changed since the last
    /// measurement (one at a time, on a worker thread).
    fn measure_yours(&mut self, cx: &mut EventContext, lesson: usize) {
        if self.match_measuring {
            return;
        }
        let mut patch = self.synth.get();
        patch.held_notes.clear();
        if self.match_measured.as_ref() == Some(&patch) {
            return;
        }
        self.match_measuring = true;
        cx.spawn(move |proxy| {
            let analysis = Arc::new(sound_match::measure(&patch));
            let _ = proxy.emit(LessonEvent::MatchMeasured { lesson, patch: Some(patch), analysis });
        });
    }

    fn exit(&mut self) {
        self.active.set(None);
        self.reviewing.set(None);
        self.completed.set(false);
        self.explaining.set(None);
        self.reset_match();
        self.stop_preview();
        self.step_before = None;
        self.last_change = None;
        self.change_step.set(None);
        self.hint_visible.set(false);
        self.set_highlight(None);
        self.clear_example();
    }

    /// Renders `which` on a worker thread; `PreviewReady` plays it.
    fn hear(&mut self, cx: &mut EventContext, which: preview::Which) {
        if self.previewing.get() == Some(which) {
            self.stop_preview();
            return;
        }
        let Some((lesson, step)) = self.active.get() else { return };
        let take = match which {
            preview::Which::Goal => preview::goal(course::LESSONS[lesson].id, &self.snapshot(), &self.patches.get()),
            preview::Which::Before => self.last_change.as_ref().map(|(_, before, _)| before.clone()),
            preview::Which::After => self.last_change.as_ref().map(|(_, _, after)| after.clone()),
            preview::Which::Yours => Some(preview::take_of(&self.snapshot(), &self.patches.get())),
            preview::Which::Example => self.example.as_ref().map(|done| preview::take_of(done, &self.patches.get())),
            preview::Which::Quiz => match course::LESSONS[lesson].steps[step].kind {
                course::Kind::Quiz { notes, .. } => Some(preview::quiz_take(notes)),
                _ => None,
            },
        };
        let Some(take) = take else { return };
        // One thing at a time: the song stops for a preview.
        if self.playing.get() {
            cx.emit(AppEvent::Stop);
        }
        self.stop_preview();
        self.preview_generation += 1;
        let generation = self.preview_generation;
        let sample_rate = self.sample_rate;
        cx.spawn(move |proxy| {
            let sources = crate::project::decode_sources(&take.arrangement, sample_rate);
            let job = engine::render::RenderJob { arrangement: take.arrangement, patches: take.patches, sources, sample_rate };
            let audio = engine::render::render_between(&job, take.from, take.to, preview::TAIL_SECONDS);
            let _ = proxy.emit(LessonEvent::PreviewReady { generation, which, audio: Arc::from(audio) });
        });
    }

    /// Makes the app match the current step done: the arrangement as one
    /// undoable edit, then the selection, sound, editor and transport.
    fn show_me(&mut self, cx: &mut EventContext) {
        let Some((lesson, step)) = self.active.get() else { return };
        let before = self.snapshot();
        let Some(after) = show::example(&course::LESSONS[lesson], step, &before) else { return };

        let json = |a: &Arrangement| serde_json::to_string(a).unwrap_or_default();
        let edited = json(&after.arrangement) != json(&before.arrangement);
        if edited {
            cx.emit(TimelineEvent::ReplaceArrangement(Box::new(after.arrangement.clone())));
        }
        if let Some(track) = after.selected_track.filter(|t| Some(*t) != before.selected_track) {
            cx.emit(crate::synth::state::SynthEvent::SelectTrack(track));
        }
        let mut patch = after.synth.clone();
        patch.held_notes = before.synth.held_notes.clone();
        let sound_changed = patch != before.synth;
        if sound_changed {
            cx.emit(crate::synth::state::SynthEvent::Update(Box::new(move |p| {
                let held = std::mem::take(&mut p.held_notes);
                *p = patch.clone();
                p.held_notes = held;
            })));
        }
        let pressed: Vec<u8> = after.synth.held_notes.iter().copied().filter(|n| !before.synth.held_notes.contains(n)).collect();
        for &note in &pressed {
            cx.emit(crate::synth::state::SynthEvent::KeyPress(note));
        }
        if !pressed.is_empty() {
            self.release_keys = Some((Instant::now() + Duration::from_millis(600), pressed));
        }
        if let Some(clip) = after.open_clip.filter(|c| Some(*c) != before.open_clip) {
            cx.emit(PianoRollEvent::Open(clip));
        }
        if after.analyzer_open && !before.analyzer_open {
            cx.emit(crate::analyzer::AnalyzerEvent::Toggle);
        }
        // An export step: the dialog, for the learner to pick where.
        if after.exported && !before.exported {
            cx.emit(crate::project::ProjectEvent::ExportDialog);
        }
        // A key step: what picking it in the Key menu does.
        if after.key != before.key {
            cx.emit(crate::interval_input::state::IntervalInputEvent::SetKey(after.key));
        }
        if after.scale_mask != before.scale_mask {
            cx.emit(crate::interval_input::state::IntervalInputEvent::SetScaleMask(after.scale_mask));
        }
        if after.snap != before.snap {
            cx.emit(TimelineEvent::SetSnap(after.snap));
        }
        if after.playhead != before.playhead {
            cx.emit(TimelineEvent::ScrubPlayhead(after.playhead));
        }
        if after.playing && !before.playing {
            cx.emit(AppEvent::TogglePlay);
        }
        // Remembered once the step passes (next tick), for "Try it yourself".
        self.shown = Some((step, sound_changed.then_some(before.synth), edited));
    }

    fn try_yourself(&mut self, cx: &mut EventContext) {
        let Some((lesson, step)) = self.active.get() else { return };
        let Some((shown, patch, edited)) = self.shown.take() else { return };
        if shown + 1 != step {
            return;
        }
        if edited {
            cx.emit(TimelineEvent::Undo);
        }
        if let Some(patch) = patch {
            cx.emit(crate::synth::state::SynthEvent::Update(Box::new(move |p| {
                let held = std::mem::take(&mut p.held_notes);
                *p = patch.clone();
                p.held_notes = held;
            })));
        }
        self.last_change = None;
        self.go_to(lesson, shown);
    }

    fn stop_preview(&mut self) {
        if let Some(token) = self.preview_token.take() {
            self.player.borrow_mut().stop_if(token);
        }
        if self.previewing.get().is_some() {
            self.previewing.set(None);
        }
    }

    fn set_highlight(&mut self, target: Option<Target>) {
        if self.highlight.get() != target {
            self.highlight.set(target);
        }
    }
}

impl Model for LessonModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            LessonEvent::Begin(lesson) => {
                if *lesson >= course::LESSONS.len() {
                    return;
                }
                // A tidy workspace to start from: stopped, nothing open,
                // the sidebar (which steps point at) showing.
                self.map_open.set(false);
                cx.emit(AppEvent::Stop);
                cx.emit(PianoRollEvent::Close);
                cx.emit(TimelineEvent::SetTool(TimelineTool::Select));
                // Snap as the steps expect it: 16ths.
                cx.emit(TimelineEvent::SetSnap(shared::arrangement::SnapGrid::Sixteenth));
                // The lesson itself shows in the sidebar's Learn panel.
                cx.emit(crate::browser::BrowserEvent::ShowLearn);
                // The part the lesson is about is the last track: select
                // it, so its instrument is what the panel shows.
                if let Some(last) = self.arrangement.get().tracks.last() {
                    cx.emit(crate::synth::state::SynthEvent::SelectTrack(last.id));
                }
                self.last_change = None;
                self.reset_match();
                if let Some(target) = sound_match::target(course::LESSONS[*lesson].id) {
                    let lesson = *lesson;
                    cx.spawn(move |proxy| {
                        let analysis = Arc::new(sound_match::measure(&target));
                        let _ = proxy.emit(LessonEvent::MatchMeasured { lesson, patch: None, analysis });
                    });
                }
                self.go_to(*lesson, 0);
                let has_goal = preview::goal(course::LESSONS[*lesson].id, &self.snapshot(), &self.patches.get()).is_some();
                self.has_goal.set(has_goal);
            }
            LessonEvent::Tick => {
                if self.release_keys.as_ref().is_some_and(|(at, _)| Instant::now() >= *at) {
                    for note in self.release_keys.take().map(|(_, n)| n).unwrap_or_default() {
                        cx.emit(crate::synth::state::SynthEvent::KeyRelease(note));
                    }
                }
                // Ended, replaced (by a browser preview), or the song started.
                let ours = self.preview_token.is_some() && self.player.borrow().current() == self.preview_token;
                if self.previewing.get().is_some() && (self.playing.get() || !ours) {
                    self.stop_preview();
                }
                if let Some(id) = self.finished.take() {
                    cx.emit(crate::project::ProjectEvent::LessonFinished(id));
                }
                let Some((lesson, step)) = self.active.get() else { return };
                if self.match_target.get().is_some() {
                    self.measure_yours(cx, lesson);
                }
                let course::Kind::Action { check, target } = course::LESSONS[lesson].steps[step].kind else { return };
                // Still reading about the step just done: this one waits.
                if self.completed.get() {
                    return;
                }
                if self.last_check.elapsed() < CHECK_EVERY {
                    return;
                }
                self.last_check = Instant::now();
                let snap = self.snapshot();
                if check(&snap) {
                    if let Some(before) = self.step_before.take() {
                        self.last_change = Some((step, before, preview::take_of(&snap, &self.patches.get())));
                    }
                    self.go_to(lesson, step + 1);
                    if !course::LESSONS[lesson].steps[step].why.is_empty() {
                        self.completed.set(true);
                    }
                } else {
                    let ghosts = self.ghosts.get();
                    if !ghosts.is_empty() {
                        let progress = Some((placed(&snap.arrangement, &ghosts), ghosts.len()));
                        if self.ghost_progress.get() != progress {
                            self.ghost_progress.set(progress);
                        }
                    }
                    self.set_highlight(target(&snap));
                    if !self.hint_visible.get() && self.step_started.elapsed() >= HINT_AFTER {
                        self.hint_visible.set(true);
                    }
                }
            }
            LessonEvent::Continue | LessonEvent::Skip => {
                if let Some((lesson, step)) = self.active.get() {
                    self.go_to(lesson, step + 1);
                }
            }
            LessonEvent::Exit => {
                cx.emit(crate::project::ProjectEvent::AutoSave);
                self.exit();
            }
            LessonEvent::Hear(which) => self.hear(cx, *which),
            LessonEvent::ShowMe => self.show_me(cx),
            LessonEvent::MatchMeasured { lesson, patch, analysis } => {
                if patch.is_some() {
                    self.match_measuring = false;
                }
                if self.active.get().map(|(l, _)| l) != Some(*lesson) {
                    return;
                }
                match patch {
                    None => self.match_target.set(Some(analysis.clone())),
                    Some(patch) => {
                        self.match_measured = Some(patch.clone());
                        self.match_yours.set(Some(analysis.clone()));
                    }
                }
                if let (Some(t), Some(y)) = (self.match_target.get(), self.match_yours.get()) {
                    self.match_score.set(Some(shared::analysis::likeness(&t, &y)));
                }
            }
            LessonEvent::TryYourself => self.try_yourself(cx),
            LessonEvent::ShowMap(open) => {
                if self.map_open.get() != *open {
                    self.map_open.set(*open);
                }
            }
            LessonEvent::Explain(which) => {
                let open = self.explaining.get();
                self.explaining.set(if *which == open { None } else { *which });
            }
            LessonEvent::NextStep => {
                self.completed.set(false);
                self.explaining.set(None);
                self.step_started = Instant::now();
            }
            LessonEvent::Review(step) => {
                let current = self.active.get().map(|(_, s)| s);
                self.reviewing.set(step.filter(|s| current.is_some_and(|c| *s < c)));
                self.explaining.set(None);
            }
            LessonEvent::Answer(choice) => {
                let Some((lesson, step)) = self.active.get() else { return };
                let course::Kind::Quiz { answer, .. } = course::LESSONS[lesson].steps[step].kind else { return };
                if *choice == answer {
                    self.stop_preview();
                    self.go_to(lesson, step + 1);
                } else {
                    self.quiz_wrong.set(Some(*choice));
                }
            }
            LessonEvent::PreviewReady { generation, which, audio } => {
                if *generation != self.preview_generation || self.active.get().is_none() {
                    return;
                }
                if let Some(token) = self.player.borrow_mut().play(audio.to_vec(), false) {
                    self.preview_token = Some(token);
                    self.previewing.set(Some(*which));
                }
            }
        });
    }
}

/// Notes in `after` that `before` doesn't have (same clip, start and
/// pitch), in clip then time order.
fn added_notes(before: &Arrangement, after: &Arrangement) -> Vec<Ghost> {
    use shared::arrangement::ClipContent;
    let mut ghosts = Vec::new();
    for clip in &after.clips {
        let ClipContent::Midi { notes, .. } = &clip.content else { continue };
        let old: &[shared::arrangement::MidiNote] = match before.clip(clip.id).map(|c| &c.content) {
            Some(ClipContent::Midi { notes, .. }) => notes,
            _ => &[],
        };
        for note in notes {
            if !old.iter().any(|o| o.start == note.start && o.pitch == note.pitch) {
                ghosts.push(Ghost { clip: clip.id, note: *note });
            }
        }
    }
    ghosts.sort_by_key(|g| (g.clip, g.note.start, g.note.pitch));
    ghosts
}

/// How many of `ghosts` are in place in `arr`.
fn placed(arr: &Arrangement, ghosts: &[Ghost]) -> usize {
    ghosts.iter().filter(|g| is_placed(arr, g)).count()
}

pub fn is_placed(arr: &Arrangement, g: &Ghost) -> bool {
    matches!(arr.clip(g.clip).map(|c| &c.content), Some(shared::arrangement::ClipContent::Midi { notes, .. })
        if notes.iter().any(|n| n.start == g.note.start && n.pitch == g.note.pitch))
}

/// The track a snapshot's selection points at.
pub fn selected<'a>(s: &'a Snapshot) -> Option<&'a shared::arrangement::Track> {
    s.selected_track.and_then(|id| s.arrangement.track(id))
}

/// Tracks playing through `instrument`.
pub fn tracks_with(s: &Snapshot, instrument: Instrument) -> impl Iterator<Item = &shared::arrangement::Track> {
    s.arrangement.tracks.iter().filter(move |t| t.kind == TrackKind::Midi && t.instrument == Some(instrument))
}
