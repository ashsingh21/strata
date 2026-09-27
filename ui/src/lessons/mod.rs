//! Interactive lessons: a short course that runs inside the real app. A
//! lesson opens a known starting project (through the unsaved-changes
//! guard, see `ProjectEvent::StartLesson`), then walks through steps that
//! check themselves against app state each frame - nothing to click
//! "Next" on for an action - while the control a step needs glows.
//!
//! The course itself is data (`course::LESSONS`); checks are plain
//! functions of a [`Snapshot`], so each lesson is unit-tested end to end.

pub mod bar;
pub mod course;

use std::cell::Cell;
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
}

/// A control a step can make glow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    AddMidiTrack,
    AddDrumTrack,
    SidebarInstrument(Instrument),
    SidebarPreset(&'static str),
    Play,
    PatternPlus,
    /// A track's lane in the timeline (a canvas wash).
    Lane(TrackId),
    /// A row of the open piano roll (a canvas wash).
    PianoRollRow(u8),
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
}

impl<V: View> LessonTargetExt for Handle<'_, V> {
    fn lesson_target(self, target: Target) -> Self {
        match HIGHLIGHT.get() {
            Some(h) => self.toggle_class("is-lesson-target", h.map(move |t| *t == Some(target))),
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
}

impl LessonModel {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        arrangement: Signal<Arrangement>,
        selected_track: Signal<Option<TrackId>>,
        playing: Signal<bool>,
        synth: Signal<SynthState>,
        sidebar_open: Signal<bool>,
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
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            arrangement: self.arrangement.get(),
            selected_track: self.selected_track.get(),
            playing: self.playing.get(),
            synth: self.synth.get(),
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
        self.hint_visible.set(false);
        self.set_highlight(None);
        // Reaching the closing step is finishing the lesson.
        if step == steps.len() - 1 {
            let id = course::LESSONS[lesson].id.to_string();
            if !self.done.get().contains(&id) {
                self.done.update(|d| d.push(id));
                crate::settings::save_lessons_done(&self.done.get());
            }
        }
    }

    fn exit(&mut self) {
        self.active.set(None);
        self.hint_visible.set(false);
        self.set_highlight(None);
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
                self.go_to(*lesson, 0);
            }
            LessonEvent::Tick => {
                let Some((lesson, step)) = self.active.get() else { return };
                let course::Kind::Action { check, target } = course::LESSONS[lesson].steps[step].kind else { return };
                let snap = self.snapshot();
                if check(&snap) {
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
            LessonEvent::Exit => self.exit(),
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
