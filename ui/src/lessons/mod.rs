//! Interactive lessons: a short course that runs inside the real app. A
//! lesson opens a known starting project (through the unsaved-changes
//! guard, see `ProjectEvent::StartLesson`), then walks through steps that
//! check themselves against app state each frame - nothing to click
//! "Next" on for an action - while the control a step needs glows.
//!
//! The course itself is data (`course::LESSONS`); checks are plain
//! functions of a [`Snapshot`], so each lesson is unit-tested end to end.

pub mod bar;
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
    /// Carve's filter type switch (LP 24 / LP 12 / BP / HP).
    FilterType,
    /// LFO 1 or 2's pill (drag it onto a knob).
    LfoPill(u8),
    /// A track's Mute / Solo button.
    Mute(TrackId),
    Solo(TrackId),
    /// A bar on the ruler (0-based) - where to click.
    RulerBar(i64),
    /// A track's automation lanes (a canvas wash).
    Automation(TrackId),
}

thread_local! {
    /// The glowing target, readable by any view without threading a prop
    /// through every builder (same approach as `effect_panel`'s
    /// `TRACK_COLOR`). Set once in `main` before the views are built.
    static HIGHLIGHT: Cell<Option<Signal<Option<Target>>>> = const { Cell::new(None) };
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
    sidebar_open: Signal<bool>,
    open_clip: Signal<Option<shared::arrangement::ClipId>>,
    playhead: Signal<shared::arrangement::Ticks>,
    patches: Signal<BTreeMap<TrackId, SynthState>>,
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
    /// When the lesson's project was last auto-saved to My tracks.
    last_save: Instant,
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

/// How often a running lesson saves the learner's track.
const AUTO_SAVE_EVERY: Duration = Duration::from_secs(5);

impl LessonModel {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        arrangement: Signal<Arrangement>,
        selected_track: Signal<Option<TrackId>>,
        playing: Signal<bool>,
        synth: Signal<SynthState>,
        sidebar_open: Signal<bool>,
        open_clip: Signal<Option<shared::arrangement::ClipId>>,
        playhead: Signal<shared::arrangement::Ticks>,
        patches: Signal<BTreeMap<TrackId, SynthState>>,
        player: crate::preview_player::SharedPlayer,
        sample_rate: u32,
    ) -> Self {
        let highlight = Signal::new(None);
        HIGHLIGHT.set(Some(highlight));
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
            sidebar_open,
            open_clip,
            playhead,
            patches,
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
            last_save: Instant::now(),
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
        }
    }

    fn go_to(&mut self, lesson: usize, step: usize) {
        let steps = course::LESSONS[lesson].steps;
        if step >= steps.len() {
            self.exit();
            return;
        }
        self.active.set(Some((lesson, step)));
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
        self.set_highlight(None);
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
        self.reset_match();
        self.stop_preview();
        self.step_before = None;
        self.last_change = None;
        self.change_step.set(None);
        self.hint_visible.set(false);
        self.set_highlight(None);
    }

    /// Renders `which` on a worker thread; `PreviewReady` plays it.
    fn hear(&mut self, cx: &mut EventContext, which: preview::Which) {
        if self.previewing.get() == Some(which) {
            self.stop_preview();
            return;
        }
        let Some((lesson, _)) = self.active.get() else { return };
        let take = match which {
            preview::Which::Goal => preview::goal(course::LESSONS[lesson].id, &self.snapshot(), &self.patches.get()),
            preview::Which::Before => self.last_change.as_ref().map(|(_, before, _)| before.clone()),
            preview::Which::After => self.last_change.as_ref().map(|(_, _, after)| after.clone()),
            preview::Which::Yours => Some(preview::take_of(&self.snapshot(), &self.patches.get())),
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
            let sources = crate::project::decode_sources(&take.arrangement);
            let job = engine::render::RenderJob { arrangement: take.arrangement, patches: take.patches, sources, sample_rate };
            let audio = engine::render::render_between(&job, take.from, take.to, preview::TAIL_SECONDS);
            let _ = proxy.emit(LessonEvent::PreviewReady { generation, which, audio: Arc::from(audio) });
        });
    }

    /// Makes the app match the current step done: the arrangement as one
    /// undoable edit, then the selection, sound, editor and transport.
    fn show_me(&mut self, cx: &mut EventContext) {
        let Some((lesson, step)) = self.active.get() else { return };
        let steps = course::LESSONS[lesson].steps;
        let index = steps[..step].iter().filter(|s| matches!(s.kind, course::Kind::Action { .. })).count();
        let shows = show::steps(course::LESSONS[lesson].id);
        let Some(show) = shows.get(index) else { return };
        let before = self.snapshot();
        let mut after = before.clone();
        if !show::run(&**show, &mut after) {
            return;
        }

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
                cx.emit(AppEvent::Stop);
                cx.emit(PianoRollEvent::Close);
                cx.emit(TimelineEvent::SetTool(TimelineTool::Select));
                if !self.sidebar_open.get() {
                    cx.emit(AppEvent::ToggleSidebar);
                }
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
                if self.last_save.elapsed() >= AUTO_SAVE_EVERY {
                    self.last_save = Instant::now();
                    cx.emit(crate::project::ProjectEvent::AutoSave);
                }
                let course::Kind::Action { check, target } = course::LESSONS[lesson].steps[step].kind else { return };
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
                } else {
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

/// The track a snapshot's selection points at.
pub fn selected<'a>(s: &'a Snapshot) -> Option<&'a shared::arrangement::Track> {
    s.selected_track.and_then(|id| s.arrangement.track(id))
}

/// Tracks playing through `instrument`.
pub fn tracks_with(s: &Snapshot, instrument: Instrument) -> impl Iterator<Item = &shared::arrangement::Track> {
    s.arrangement.tracks.iter().filter(move |t| t.kind == TrackKind::Midi && t.instrument == Some(instrument))
}
