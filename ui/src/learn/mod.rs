//! Learn, by goal: the Learn home (where to carry on, today's practice,
//! the goals), each goal's page (its lessons and drills in order), the
//! goal's path in the sidebar while you work through it, and the drill
//! bar over a practice tool opened from a goal (how far into the session,
//! and what next). Lessons themselves still run in `lessons`; the drills
//! are the practice tools (Ear, Exercises, Riyaz...) set up for one skill.

pub mod bar;
pub mod goals;
pub mod home;
pub mod side;

use std::time::{Duration, Instant};

use vizia::prelude::*;

use goals::{Drill, Item, GOALS};

use crate::tools::{Tool, ToolsEvent};

/// The page over the arrangement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Goal(usize),
}

/// A drill session running: which item of which goal, how it's going.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub goal: usize,
    pub item: usize,
    pub drill: Drill,
    /// Each answer so far, right or not.
    pub answers: Vec<bool>,
    pub finished: bool,
    /// Part of today's practice (its slot), so Next goes on to the next.
    pub today: Option<usize>,
}

impl Run {
    pub fn title(&self) -> &'static str {
        GOALS[self.goal].items[self.item].title()
    }
    pub fn right(&self) -> u32 {
        self.answers.iter().filter(|a| **a).count() as u32
    }
    pub fn passed(&self) -> bool {
        self.finished && (self.drill.session() == 0 || self.right() >= self.drill.pass())
    }
}

/// The tool a drill runs on.
pub fn tool_of(drill: Drill) -> Tool {
    match drill {
        Drill::Ear(_) => Tool::Ear,
        Drill::Rhythm(_) | Drill::Melody(_) | Drill::Classics => Tool::Exercises,
        Drill::Riyaz => Tool::Riyaz,
        Drill::TheoryRing => Tool::Theory,
        Drill::VoiceLeading => Tool::Voicing,
    }
}

/// How long a riyaz session is.
const RIYAZ_SESSION: Duration = Duration::from_secs(120);

pub enum LearnEvent {
    /// Show a page over the arrangement, or none.
    Open(Option<Page>),
    /// Start one of a goal's items: its lesson, or its drill.
    Start { goal: usize, item: usize },
    /// The first of today's practice not done yet.
    StartToday,
    /// A drill's answer, from its tool.
    Answered(bool),
    /// On to the next: today's next drill, or the goal's next item.
    Next,
    /// The same drill, a fresh session.
    Again,
    /// Close the drill (and its tool).
    Leave,
    Tick,
}

pub struct LearnModel {
    pub page: Signal<Option<Page>>,
    /// The goal you're working through (the sidebar shows it).
    pub goal: Signal<usize>,
    pub run: Signal<Option<Run>>,
    /// Today's three drills, done or not.
    pub today_done: Signal<[bool; 3]>,
    pub streak: Signal<usize>,
    /// Finished lessons and drills (the lessons' list, shared).
    done: Signal<Vec<String>>,
    started: Instant,
    /// The item started last, for Next after a lesson.
    last: Option<(usize, usize)>,
    day: u64,
    days: Vec<u64>,
}

/// Days since 1970 (UTC): today's practice starts over each day.
fn today_number() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 86_400).unwrap_or(0)
}

thread_local! {
    static PROPS: std::cell::Cell<Option<LearnProps>> = const { std::cell::Cell::new(None) };
}

/// The Learn signals, for views built far from `main` (the lesson pane's
/// "next" button, the browser's Learn section).
pub fn props() -> Option<LearnProps> {
    PROPS.get()
}

impl LearnModel {
    pub fn new(done: Signal<Vec<String>>) -> Self {
        let day = today_number();
        let days = crate::settings::load_practice_days();
        let today_done = match crate::settings::load_practice_today() {
            Some((d, slots)) if d == day => [0, 1, 2].map(|i| slots.contains(&i)),
            _ => [false; 3],
        };
        let goal = crate::settings::load_learn_goal().unwrap_or(0).min(GOALS.len() - 1);
        Self {
            page: Signal::new(None),
            goal: Signal::new(goal),
            run: Signal::new(None),
            today_done: Signal::new(today_done),
            streak: Signal::new(goals::streak(&days, day)),
            done,
            started: Instant::now(),
            last: None,
            day,
            days,
        }
    }

    fn mark_done(&self, item: Item) {
        let id = item.done_id();
        if !self.done.get().contains(&id) {
            self.done.update(|d| d.push(id));
            crate::settings::save_lessons_done(&self.done.get());
        }
    }

    fn set_goal(&self, goal: usize) {
        if self.goal.get() != goal {
            self.goal.set(goal);
            crate::settings::save_learn_goal(goal);
        }
    }

    fn start(&mut self, cx: &mut EventContext, goal: usize, item: usize, today: Option<usize>) {
        let Some(&it) = GOALS.get(goal).and_then(|g| g.items.get(item)) else { return };
        self.set_goal(goal);
        self.last = Some((goal, item));
        self.page.set(None);
        match it {
            Item::Lesson(id) => {
                self.leave(cx);
                if let Some(i) = goals::lesson(id) {
                    cx.emit(crate::project::ProjectEvent::StartLesson(i));
                }
            }
            Item::Drill { drill, .. } => {
                cx.emit(crate::browser::BrowserEvent::ShowLearn);
                self.open_tool(cx, drill);
                self.started = Instant::now();
                let finished = matches!(drill, Drill::TheoryRing | Drill::VoiceLeading);
                if finished {
                    // A tool to explore: opening it is the step.
                    self.mark_done(it);
                }
                self.run.set(Some(Run { goal, item, drill, answers: Vec::new(), finished, today }));
            }
        }
    }

    fn open_tool(&self, cx: &mut EventContext, drill: Drill) {
        use crate::practice::PracticeEvent;
        use shared::practice::Kind;
        match drill {
            Drill::Ear(step) => {
                cx.emit(ToolsEvent::Show(Tool::Ear));
                cx.emit(crate::ear::EarEvent::SetStep(step));
                cx.emit(crate::ear::EarEvent::Next);
            }
            Drill::Rhythm(level) | Drill::Melody(level) => {
                cx.emit(ToolsEvent::Show(Tool::Exercises));
                cx.emit(PracticeEvent::SetKind(if matches!(drill, Drill::Rhythm(_)) { Kind::Rhythm } else { Kind::Melody }));
                cx.emit(PracticeEvent::SetLevel(level));
                cx.emit(PracticeEvent::Start);
            }
            Drill::Classics => {
                cx.emit(ToolsEvent::Show(Tool::Exercises));
                cx.emit(PracticeEvent::SetClassics(true));
                cx.emit(PracticeEvent::Start);
            }
            Drill::Riyaz => cx.emit(ToolsEvent::Show(Tool::Riyaz)),
            Drill::TheoryRing => cx.emit(ToolsEvent::Show(Tool::Theory)),
            Drill::VoiceLeading => cx.emit(ToolsEvent::Show(Tool::Voicing)),
        }
    }

    fn leave(&mut self, cx: &mut EventContext) {
        if self.run.get().is_some() {
            self.run.set(None);
            cx.emit(ToolsEvent::CloseAll);
        }
    }

    /// A session ended: done if it went well, and today's slot ticked.
    fn finish(&mut self, mut run: Run) {
        run.finished = true;
        let item = GOALS[run.goal].items[run.item];
        if run.passed() {
            self.mark_done(item);
        }
        if let Some(slot) = run.today {
            let mut t = self.today_done.get();
            t[slot] = true;
            self.today_done.set(t);
            let slots: Vec<usize> = (0..3).filter(|&i| t[i]).collect();
            crate::settings::save_practice_today(self.day, &slots);
            if t.iter().all(|x| *x) && !self.days.contains(&self.day) {
                self.days.push(self.day);
                crate::settings::save_practice_days(&self.days);
                self.streak.set(goals::streak(&self.days, self.day));
            }
        }
        self.run.set(Some(run));
    }

    fn next(&mut self, cx: &mut EventContext) {
        let done = self.done.get();
        if let Some(run) = self.run.get() {
            if let Some(slot) = run.today {
                let t = self.today_done.get();
                let plan = goals::today(&done);
                if let Some(next) = (slot + 1..3).chain(0..slot).find(|&i| !t[i]) {
                    let (g, i) = plan[next];
                    self.start(cx, g, i, Some(next));
                } else {
                    self.leave(cx);
                    self.page.set(Some(Page::Home));
                }
                return;
            }
        }
        let Some((goal, item)) = self.last else { return };
        let items = GOALS[goal].items;
        // The next one not done yet after it, else simply the next.
        let next = (item + 1..items.len()).find(|&i| !items[i].is_done(&done)).or((item + 1 < items.len()).then_some(item + 1));
        match next {
            Some(i) => self.start(cx, goal, i, None),
            None => {
                self.leave(cx);
                self.page.set(Some(Page::Goal(goal)));
            }
        }
    }
}

impl Model for LearnModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            LearnEvent::Open(page) => {
                if let Some(Page::Goal(g)) = page {
                    self.set_goal(*g);
                }
                self.page.set(*page);
            }
            LearnEvent::Start { goal, item } => self.start(cx, *goal, *item, None),
            LearnEvent::StartToday => {
                let t = self.today_done.get();
                let plan = goals::today(&self.done.get());
                let slot = (0..3).find(|&i| !t[i]).unwrap_or(0);
                self.start(cx, plan[slot].0, plan[slot].1, Some(slot));
            }
            LearnEvent::Answered(right) => {
                let Some(mut run) = self.run.get() else { return };
                if run.finished || run.drill.session() == 0 {
                    return;
                }
                run.answers.push(*right);
                if run.answers.len() as u32 >= run.drill.session() {
                    self.finish(run);
                } else {
                    self.run.set(Some(run));
                }
            }
            LearnEvent::Next => self.next(cx),
            LearnEvent::Again => {
                if let Some(run) = self.run.get() {
                    self.start(cx, run.goal, run.item, run.today);
                }
            }
            LearnEvent::Leave => self.leave(cx),
            LearnEvent::Tick => {
                // Its tool closed some other way (its own Close): the drill
                // is over. (A moment's grace while it opens.)
                if let Some(run) = self.run.get() {
                    if self.started.elapsed() > Duration::from_millis(500) && !crate::tools::is_open(tool_of(run.drill)) {
                        self.run.set(None);
                    }
                }
                // A riyaz session is two minutes of it.
                if let Some(run) = self.run.get() {
                    if run.drill == Drill::Riyaz && !run.finished && self.started.elapsed() >= RIYAZ_SESSION {
                        self.finish(run);
                    }
                }
                // A new day: today's practice starts over.
                let day = today_number();
                if day != self.day {
                    self.day = day;
                    self.today_done.set([false; 3]);
                    self.streak.set(goals::streak(&self.days, day));
                }
            }
        });
    }
}

#[derive(Clone, Copy)]
pub struct LearnProps {
    pub page: Signal<Option<Page>>,
    pub goal: Signal<usize>,
    pub run: Signal<Option<Run>>,
    pub today_done: Signal<[bool; 3]>,
    pub streak: Signal<usize>,
    pub done: Signal<Vec<String>>,
    pub lessons_active: Signal<Option<(usize, usize)>>,
    pub theme: Signal<crate::tokens::ThemeId>,
}

impl LearnProps {
    pub fn of(m: &LearnModel, lessons_active: Signal<Option<(usize, usize)>>, theme: Signal<crate::tokens::ThemeId>) -> Self {
        let p = Self {
            page: m.page,
            goal: m.goal,
            run: m.run,
            today_done: m.today_done,
            streak: m.streak,
            done: m.done,
            lessons_active,
            theme,
        };
        PROPS.set(Some(p));
        p
    }
}

/// What a lesson's "next" button offers once it's done: the next item of
/// the goal it was started from (the current goal, if it's in it).
pub fn next_after_lesson(lesson_id: &str) -> Option<(usize, usize)> {
    let p = props()?;
    let done = p.done.get();
    let current = p.goal.get();
    let goal = if GOALS.get(current).is_some_and(|g| g.items.iter().any(|i| i.is_lesson(lesson_id))) {
        current
    } else {
        goals::goals_with(lesson_id).next()?
    };
    let items = GOALS[goal].items;
    let at = items.iter().position(|i| i.is_lesson(lesson_id))?;
    (at + 1..items.len()).find(|&i| !items[i].is_done(&done)).or((at + 1 < items.len()).then_some(at + 1)).map(|i| (goal, i))
}
