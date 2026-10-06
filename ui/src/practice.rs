//! Practice: hear a one-bar phrase, then play it back on your keyboard or
//! pads - rhythm (tap any key) or melody (the right notes). A count-in
//! before the phrase and before your turn; then each hit's timing, or each
//! note, and a streak to beat. Docked under the devices, from the rail.

use std::cell::Cell;
use std::sync::Arc;
use std::time::Instant;

use vizia::prelude::*;
use vizia::vg;

use shared::practice::{self, Exercise, Hit, Kind, NoteResult};
use shared::theory::note_name_for_key;

use crate::hidpi::Logical;
use crate::synth::state::SynthEvent;
use crate::tokens::{self, ThemeId};

thread_local! {
    static OPEN: Cell<Option<Signal<bool>>> = const { Cell::new(None) };
}

pub fn open_signal() -> Option<Signal<bool>> {
    OPEN.get()
}

/// After the last bar, how long late notes still count (ms).
const GRACE_MS: f32 = 400.0;
/// In a row, to suggest the next level.
const STREAK_TO_LEVEL_UP: u32 = 3;

pub enum PracticeEvent {
    ToggleOpen,
    SetKind(Kind),
    /// Famous melodies, phrase by phrase, instead of new exercises.
    SetClassics(bool),
    SetPiece(usize),
    SetLevel(usize),
    Tempo(i32),
    /// A new exercise (or, after one, the next).
    Start,
    /// The same exercise again.
    Again,
    Rendered { generation: u64, audio: Arc<[f32]> },
    Tick,
}

/// What happened, for the chart.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub hits: Vec<Hit>,
    pub extra: usize,
    pub notes: Vec<NoteResult>,
    /// What you played: (ms from your bar's start, pitch).
    pub played: Vec<(f32, u8)>,
}

impl Outcome {
    pub fn perfect(&self, kind: Kind) -> bool {
        match kind {
            Kind::Rhythm => self.extra == 0 && self.hits.iter().all(|h| matches!(h, Hit::OnTime(_))),
            Kind::Melody => self.notes.iter().all(|n| matches!(n, NoteResult::Right | NoteResult::Octave)),
        }
    }

    pub fn summary(&self, kind: Kind) -> String {
        match kind {
            Kind::Rhythm => {
                let on = self.hits.iter().filter(|h| matches!(h, Hit::OnTime(_))).count();
                let extra = if self.extra > 0 { format!(" \u{b7} {} extra", self.extra) } else { String::new() };
                format!("{on} of {} on time{extra}", self.hits.len())
            }
            Kind::Melody => {
                let right = self.notes.iter().filter(|n| matches!(n, NoteResult::Right | NoteResult::Octave)).count();
                format!("{right} of {} notes right", self.notes.len())
            }
        }
    }
}

pub struct PracticeModel {
    pub open: Signal<bool>,
    pub kind: Signal<Kind>,
    /// The level for each kind.
    pub levels: Signal<[usize; 2]>,
    pub bpm: Signal<i32>,
    pub exercise: Signal<Option<Exercise>>,
    /// Where the four bars are: 0..4 while playing, `None` otherwise.
    pub position: Signal<Option<f32>>,
    pub outcome: Signal<Option<Outcome>>,
    pub streak: Signal<u32>,
    pub loading: Signal<bool>,
    pub classics: Signal<bool>,
    pub piece: Signal<usize>,
    pub phrase: Signal<usize>,
    key: Signal<u8>,
    scale_mask: Signal<u16>,
    player: crate::preview_player::SharedPlayer,
    sample_rate: u32,
    generation: u64,
    token: Option<u64>,
    started: Option<Instant>,
    played: Vec<(f32, u8)>,
    seed: u32,
}

impl PracticeModel {
    pub fn new(key: Signal<u8>, scale_mask: Signal<u16>, player: crate::preview_player::SharedPlayer, sample_rate: u32) -> Self {
        let open = Signal::new(false);
        OPEN.set(Some(open));
        Self {
            open,
            kind: Signal::new(Kind::Rhythm),
            levels: Signal::new([0, 0]),
            bpm: Signal::new(80),
            exercise: Signal::new(None),
            position: Signal::new(None),
            outcome: Signal::new(None),
            streak: Signal::new(0),
            loading: Signal::new(false),
            classics: Signal::new(false),
            piece: Signal::new(0),
            phrase: Signal::new(0),
            key,
            scale_mask,
            player,
            sample_rate,
            generation: 0,
            token: None,
            started: None,
            played: Vec::new(),
            seed: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(1),
        }
    }

    fn level(&self) -> usize {
        self.levels.get()[kind_index(self.kind.get())]
    }

    fn stop(&mut self) {
        self.generation += 1;
        if let Some(token) = self.token.take() {
            self.player.borrow_mut().stop_if(token);
        }
        self.started = None;
        self.position.set(None);
        self.loading.set(false);
    }

    fn new_exercise(&mut self) -> Exercise {
        self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        match self.kind.get() {
            Kind::Rhythm => practice::rhythm(self.level(), self.seed),
            Kind::Melody => practice::melody(self.level(), self.seed, self.key.get(), self.scale_mask.get()),
        }
    }

    fn play(&mut self, cx: &mut EventContext, ex: Exercise) {
        self.stop();
        self.exercise.set(Some(ex.clone()));
        self.outcome.set(None);
        self.played.clear();
        self.loading.set(true);
        let generation = self.generation;
        let sample_rate = self.sample_rate;
        let bpm = self.bpm.get() as f64;
        let ex_bars = ex.total_bars();
        cx.spawn(move |proxy| {
            let mut project = practice::session(&ex, bpm);
            project.migrate();
            let bar = 4 * shared::arrangement::PPQ;
            let patches = project.instruments.into_iter().collect();
            let job = engine::render::RenderJob { arrangement: project.arrangement, patches, sources: Default::default(), sample_rate };
            let audio = engine::render::render_between(&job, 0, ex_bars * bar, 0.5);
            let _ = proxy.emit(PracticeEvent::Rendered { generation, audio: Arc::from(audio) });
        });
    }

    fn bar_ms(&self) -> f32 {
        4.0 * 60_000.0 / self.bpm.get() as f32
    }

    fn finish(&mut self) {
        let Some(ex) = self.exercise.get() else { return };
        let bpm = self.bpm.get() as f64;
        let your_bar = ex.your_bar() as f32 * self.bar_ms();
        let your_ms = ex.bars as f32 * self.bar_ms();
        let played: Vec<(f32, u8)> = self
            .played
            .iter()
            .map(|&(ms, pitch)| (ms - your_bar, pitch))
            .filter(|(ms, _)| *ms > -200.0 && *ms < your_ms + GRACE_MS)
            .collect();
        let (hits, extra) = practice::score_rhythm(&practice::targets_ms(&ex, bpm), &played.iter().map(|p| p.0).collect::<Vec<_>>());
        let targets: Vec<u8> = ex.notes.iter().map(|n| n.1).collect();
        let notes = practice::score_melody(&targets, &played.iter().map(|p| p.1).collect::<Vec<_>>());
        let outcome = Outcome { hits, extra, notes, played };
        let perfect = outcome.perfect(ex.kind);
        self.streak.set(if perfect { self.streak.get() + 1 } else { 0 });
        self.outcome.set(Some(outcome));
        self.started = None;
        self.position.set(None);
    }
}

fn kind_index(kind: Kind) -> usize {
    match kind {
        Kind::Rhythm => 0,
        Kind::Melody => 1,
    }
}

impl Model for PracticeModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        // What you play, timed from the start of the four bars.
        event.map(|event, _| {
            let note = match event {
                SynthEvent::PlayNote(note, _) | SynthEvent::KeyPress(note) => Some(*note),
                _ => None,
            };
            if let (Some(note), Some(t0)) = (note, self.started) {
                self.played.push((t0.elapsed().as_secs_f32() * 1000.0, note));
            }
        });
        event.map(|event, _| match event {
            PracticeEvent::ToggleOpen => {
                self.open.set(!self.open.get());
                if !self.open.get() {
                    self.stop();
                }
            }
            PracticeEvent::SetClassics(on) => {
                if self.classics.get() != *on {
                    self.stop();
                    self.classics.set(*on);
                    self.exercise.set(None);
                    self.outcome.set(None);
                    self.streak.set(0);
                    if *on {
                        self.bpm.set(shared::classics::PIECES[self.piece.get()].bpm);
                    }
                }
            }
            PracticeEvent::SetPiece(i) => {
                self.stop();
                self.piece.set(*i);
                self.phrase.set(0);
                self.bpm.set(shared::classics::PIECES[*i].bpm);
                self.exercise.set(None);
                self.outcome.set(None);
                self.streak.set(0);
            }
            PracticeEvent::SetKind(kind) => {
                self.classics.set(false);
                if self.kind.get() != *kind {
                    self.stop();
                    self.kind.set(*kind);
                    self.exercise.set(None);
                    self.outcome.set(None);
                    self.streak.set(0);
                }
            }
            PracticeEvent::SetLevel(level) => {
                self.stop();
                let i = kind_index(self.kind.get());
                self.levels.update(|l| l[i] = *level);
                self.exercise.set(None);
                self.outcome.set(None);
                self.streak.set(0);
            }
            PracticeEvent::Tempo(delta) => self.bpm.set((self.bpm.get() + delta).clamp(50, 140)),
            PracticeEvent::Start => {
                let ex = if self.classics.get() {
                    // Next goes on to the tune's next phrase (round to the start).
                    let piece = &shared::classics::PIECES[self.piece.get()];
                    if self.exercise.get().is_some() {
                        self.phrase.set((self.phrase.get() + 1) % piece.phrases.len());
                    }
                    piece.exercise(self.phrase.get())
                } else {
                    self.new_exercise()
                };
                self.play(cx, ex);
            }
            PracticeEvent::Again => {
                if let Some(ex) = self.exercise.get() {
                    self.play(cx, ex);
                }
            }
            PracticeEvent::Rendered { generation, audio } => {
                if *generation != self.generation {
                    return;
                }
                self.loading.set(false);
                // The song stops for an exercise.
                cx.emit(crate::app::AppEvent::Stop);
                self.token = self.player.borrow_mut().play(audio.to_vec(), false);
                if self.token.is_some() {
                    self.started = Some(Instant::now());
                    self.played.clear();
                }
            }
            PracticeEvent::Tick => {
                let Some(t0) = self.started else { return };
                let total = self.exercise.get().map(|e| e.total_bars()).unwrap_or(4) as f32;
                let ms = t0.elapsed().as_secs_f32() * 1000.0;
                let bars = ms / self.bar_ms();
                if ms > total * self.bar_ms() + GRACE_MS {
                    self.finish();
                } else if self.position.get().is_none_or(|p| (p - bars).abs() > 0.02) {
                    self.position.set(Some(bars.min(total)));
                }
            }
        });
    }
}

#[derive(Clone, Copy)]
pub struct PracticeProps {
    pub open: Signal<bool>,
    pub kind: Signal<Kind>,
    pub levels: Signal<[usize; 2]>,
    pub bpm: Signal<i32>,
    pub exercise: Signal<Option<Exercise>>,
    pub position: Signal<Option<f32>>,
    pub outcome: Signal<Option<Outcome>>,
    pub streak: Signal<u32>,
    pub loading: Signal<bool>,
    pub classics: Signal<bool>,
    pub piece: Signal<usize>,
    pub phrase: Signal<usize>,
    pub key: Signal<u8>,
    pub scale_mask: Signal<u16>,
    pub theme: Signal<ThemeId>,
}

impl PracticeProps {
    pub fn of(m: &PracticeModel, theme: Signal<ThemeId>) -> Self {
        Self {
            open: m.open,
            kind: m.kind,
            levels: m.levels,
            bpm: m.bpm,
            exercise: m.exercise,
            position: m.position,
            outcome: m.outcome,
            streak: m.streak,
            loading: m.loading,
            classics: m.classics,
            piece: m.piece,
            phrase: m.phrase,
            key: m.key,
            scale_mask: m.scale_mask,
            theme,
        }
    }
}

pub fn practice_view(cx: &mut Context, p: PracticeProps) {
    Binding::new(cx, p.open, move |cx| {
        if !p.open.get() {
            return;
        }
        VStack::new(cx, move |cx| {
            // Rebuilt when the mode changes: in a column, unlike a row,
            // views built again lay out where they belong.
            Binding::new(cx, p.classics, move |cx| header(cx, p));
            HStack::new(cx, move |cx| {
                status(cx, p);
                Chart::new(cx, p).width(Stretch(1.0)).height(Stretch(1.0));
            })
            .gap(Pixels(tokens::SPACE_4))
            .width(Stretch(1.0))
            .height(Pixels(200.0));
        })
        .class("device")
        .gap(Pixels(tokens::SPACE_3))
        .padding(Pixels(tokens::SPACE_3))
        .width(Stretch(1.0))
        .height(Auto);
    });
}

fn header(cx: &mut Context, p: PracticeProps) {
    HStack::new(cx, move |cx| {
        Label::new(cx, "Practice").class("heading");
        crate::synth::segmented::segmented(
            cx,
            3,
            |cx, i| Label::new(cx, ["Rhythm", "Melody", "Classics"][i]),
            move |i| Memo::new(move |_| if p.classics.get() { i == 2 } else { kind_index(p.kind.get()) == i }),
            |cx, i| match i {
                0 => cx.emit(PracticeEvent::SetKind(Kind::Rhythm)),
                1 => cx.emit(PracticeEvent::SetKind(Kind::Melody)),
                _ => cx.emit(PracticeEvent::SetClassics(true)),
            },
        )
        .height(Pixels(tokens::SIZE_CONTROL));
        // A tune and its phrase for Classics, levels for new exercises (the
        // whole row is rebuilt on a switch - see `practice_view`).
        if p.classics.get() {
            HStack::new(cx, move |cx| {
                crate::menu::menu(
                    cx,
                    Placement::TopStart,
                    move |cx| {
                        Button::new(cx, move |cx| {
                            Label::new(cx, p.piece.map(|i| {
                                let piece = &shared::classics::PIECES[*i];
                                format!("{} \u{b7} {}  \u{2304}", piece.title, piece.composer)
                            }))
                        })
                        .class("btn")
                        .class("sm")
                        .width(Stretch(1.0))
                        .height(Pixels(tokens::SIZE_CONTROL))
                        .on_press(crate::menu::toggle);
                    },
                    move |cx| {
                        crate::menu::panel(cx, 300.0, move |cx| {
                            for (i, piece) in shared::classics::PIECES.iter().enumerate() {
                                Button::new(cx, move |cx| Label::new(cx, format!("{} \u{b7} {}", piece.title, piece.composer)).class("body"))
                                    .class("menu-item")
                                    .toggle_class("is-on", p.piece.map(move |c| *c == i))
                                    .width(Stretch(1.0))
                                    .on_press(move |cx| {
                                        cx.emit(PracticeEvent::SetPiece(i));
                                        crate::menu::close(cx);
                                    });
                            }
                        });
                    },
                )
                // Its own width: an Auto dropdown in a row drew its button
                // over its neighbours.
                .width(Pixels(280.0))
                .height(Pixels(tokens::SIZE_CONTROL));
                Label::new(
                    cx,
                    Memo::new(move |_| format!("Phrase {} of {}", p.phrase.get() + 1, shared::classics::PIECES[p.piece.get()].phrases.len())),
                )
                .class("value");
            })
            .gap(Pixels(tokens::SPACE_2))
            .alignment(Alignment::Left)
            .size(Auto);
        } else {
            HStack::new(cx, move |cx| {
                Label::new(cx, "Level").class("label");
                crate::synth::segmented::segmented(
                    cx,
                    practice::LEVELS,
                    |cx, i| Label::new(cx, format!("{}", i + 1)),
                    move |i| Memo::new(move |_| p.levels.get()[kind_index(p.kind.get())] == i),
                    |cx, i| cx.emit(PracticeEvent::SetLevel(i)),
                )
                .height(Pixels(tokens::SIZE_CONTROL));
                Label::new(cx, Memo::new(move |_| p.kind.get().levels()[p.levels.get()[kind_index(p.kind.get())]])).class("value");
            })
            .gap(Pixels(tokens::SPACE_2))
            .alignment(Alignment::Left)
            .size(Auto);
        }
        Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(16.0));
        Button::new(cx, |cx| Label::new(cx, "\u{2212}")).class("btn").class("sm").class("quiet").on_press(|cx| cx.emit(PracticeEvent::Tempo(-5)));
        Label::new(cx, p.bpm.map(|b| format!("{b} BPM"))).class("value");
        Button::new(cx, |cx| Label::new(cx, "+")).class("btn").class("sm").class("quiet").on_press(|cx| cx.emit(PracticeEvent::Tempo(5)));
        // Melodies are in the song's key.
        let key_text = Memo::new(move |_| {
            format!(
                "{} {}  \u{2304}",
                shared::theory::note_name(p.key.get()),
                crate::interval_input::state::scale_name(p.scale_mask.get()).to_lowercase()
            )
        });
        Button::new(cx, move |cx| Label::new(cx, key_text))
            .class("btn")
            .class("sm")
            .toggle_class("hidden", Memo::new(move |_| p.kind.get() != Kind::Melody || p.classics.get()))
            .on_press(crate::key_menu::open_from);
        Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
        Button::new(cx, |cx| Label::new(cx, "Close")).class("btn").class("quiet").on_press(|cx| cx.emit(PracticeEvent::ToggleOpen));
    })
    .gap(Pixels(tokens::SPACE_2))
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Pixels(tokens::SIZE_CONTROL));
}

/// Which section is playing at `position` (in bars) for a phrase of
/// `bars`: the count-in (0), the phrase (1), the second count-in (2) or
/// your turn (3).
fn phase(position: f32, bars: i64) -> (&'static str, usize) {
    let bar = position as i64;
    if bar == 0 {
        ("Count in", 0)
    } else if bar <= bars {
        ("Listen", 1)
    } else if bar == bars + 1 {
        ("Get ready", 2)
    } else {
        ("Your turn", 3)
    }
}

fn status(cx: &mut Context, p: PracticeProps) {
    VStack::new(cx, move |cx| {
        let big = Memo::new(move |_| {
            if p.loading.get() {
                return "\u{2026}".to_string();
            }
            match (p.position.get(), p.outcome.get(), p.exercise.get()) {
                (Some(pos), _, ex) => {
                    let (name, bar) = phase(pos, ex.map(|e| e.bars).unwrap_or(1));
                    if bar == 1 {
                        name.to_string()
                    } else {
                        format!("{name} {}", (pos.fract() * 4.0) as i32 + 1)
                    }
                }
                (None, Some(o), Some(ex)) => o.summary(ex.kind),
                _ => "Ready".to_string(),
            }
        });
        Label::new(cx, big).class("practice-status").text_wrap(true).width(Stretch(1.0));
        let line = Memo::new(move |_| match (p.position.get(), p.outcome.get(), p.exercise.get()) {
            (Some(pos), _, ex) if phase(pos, ex.as_ref().map(|e| e.bars).unwrap_or(1)).1 < 3 => "Listen, then play it back.".to_string(),
            (Some(_), _, Some(ex)) if ex.kind == Kind::Rhythm => "Tap it on any key or pad.".to_string(),
            (Some(_), _, _) => "Play the notes you heard.".to_string(),
            (None, Some(o), Some(ex)) if o.perfect(ex.kind) && p.streak.get() >= STREAK_TO_LEVEL_UP => {
                format!("{} in a row - try the next level.", p.streak.get())
            }
            (None, Some(o), Some(ex)) if o.perfect(ex.kind) => format!("Nice. Streak {}.", p.streak.get()),
            (None, Some(_), _) if p.classics.get() => "Again to retry it, Next for the next phrase.".to_string(),
            (None, Some(_), _) => "Again to retry it, Next for a new one.".to_string(),
            _ => "A count-in, the phrase, a count-in, then you play it back.".to_string(),
        });
        Label::new(cx, line).class("value").text_wrap(true).width(Stretch(1.0));
        Element::new(cx).height(Stretch(1.0)).width(Pixels(1.0));
        HStack::new(cx, move |cx| {
            Button::new(cx, move |cx| Label::new(cx, p.exercise.map(|e| if e.is_some() { "\u{25b8} Next" } else { "\u{25b8} Start" })))
                .class("btn")
                .class("lg")
                .class("is-on")
                .on_press(|cx| cx.emit(PracticeEvent::Start));
            Button::new(cx, |cx| Label::new(cx, "Again"))
                .class("btn")
                .class("lg")
                .toggle_class("hidden", p.exercise.map(|e| e.is_none()))
                .on_press(|cx| cx.emit(PracticeEvent::Again));
        })
        .gap(Pixels(tokens::SPACE_2))
        .height(Auto);
    })
    .gap(Pixels(6.0))
    .width(Pixels(240.0))
    .height(Stretch(1.0));
}

/// The four bars as a strip (which one is playing), and your bar: the
/// phrase's notes once you've played, with where your hits landed.
struct Chart {
    p: PracticeProps,
}

impl Chart {
    fn new(cx: &mut Context, p: PracticeProps) -> Handle<'_, Self> {
        Self { p }
            .build(cx, |_| {})
            .bind(p.position, |mut h| h.needs_redraw())
            .bind(p.outcome, |mut h| h.needs_redraw())
            .bind(p.exercise, |mut h| h.needs_redraw())
            .bind(p.theme, |mut h| h.needs_redraw())
    }
}

const STRIP_H: f32 = 22.0;

impl View for Chart {
    fn element(&self) -> Option<&'static str> {
        Some("practice-chart")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let b = cx.lbounds();
        crate::hidpi::clip(canvas, b);
        let pal = self.p.theme.get().palette();
        fill(canvas, vg::Rect::new(b.x, b.y, b.x + b.w, b.y + b.h), pal.bg_000);

        // The strip: count-in, the phrase, count-in, you - each as long as
        // it plays - with the one playing filled.
        let position = self.p.position.get();
        let bars = self.p.exercise.get().map(|e| e.bars).unwrap_or(1);
        let total = (2 + 2 * bars) as f32;
        let sections = [("Count in", 0, 1), ("Listen", 1, bars), ("Get ready", 1 + bars, 1), ("You", 2 + bars, bars)];
        for (i, &(name, start, len)) in sections.iter().enumerate() {
            let x0 = b.x + start as f32 / total * b.w;
            let w = len as f32 / total * b.w;
            let now = position.is_some_and(|p| phase(p, bars).1 == i);
            fill(canvas, vg::Rect::new(x0 + 1.0, b.y + 1.0, x0 + w - 1.0, b.y + STRIP_H), if now { pal.signal_soft } else { pal.bg_200 });
            if let Some(p) = position.filter(|_| now) {
                let x = x0 + (p - start as f32) / len as f32 * w;
                fill(canvas, vg::Rect::new(x, b.y + 1.0, x + 2.0, b.y + STRIP_H), pal.ink);
            }
            text(canvas, name, x0 + 8.0, b.y + 15.0, 11.0, if now { pal.ink } else { pal.ink_muted });
        }

        // Your turn: its 16ths, beats marked, bars numbered.
        let steps = 16 * bars;
        let top = b.y + STRIP_H + 14.0;
        let grid = vg::Rect::new(b.x + 40.0, top, b.x + b.w - 16.0, b.y + b.h - 8.0);
        let step_w = grid.width() / steps as f32;
        let x_of_16th = |s: f32| grid.left + s * step_w;
        for s in 0..=steps {
            let x = x_of_16th(s as f32).round();
            let color = if s % 16 == 0 { pal.ink_faint } else if s % 4 == 0 { pal.line_control } else { pal.line };
            fill(canvas, vg::Rect::new(x, grid.top, x + 1.0, grid.bottom), color);
            if s % 4 == 0 && s < steps {
                let label = if bars > 1 { format!("{}.{}", s / 16 + 1, s % 16 / 4 + 1) } else { format!("{}", s / 4 + 1) };
                text(canvas, &label, x + 3.0, grid.top + 11.0, 11.0, pal.ink_muted);
            }
        }
        let Some(ex) = self.p.exercise.get() else {
            text(canvas, "Press Start.", grid.left + 12.0, grid.center_y(), 12.0, pal.ink_muted);
            return;
        };
        let outcome = self.p.outcome.get();
        let row_target = grid.top + grid.height() * 0.38;
        let row_played = grid.top + grid.height() * 0.75;
        text(canvas, "Heard", b.x + 4.0, row_target + 4.0, 11.0, pal.ink_muted);
        text(canvas, "You", b.x + 4.0, row_played + 4.0, 11.0, pal.ink_muted);
        let key = self.p.key.get();
        let name = |pitch: u8| format!("{}{}", note_name_for_key(pitch % 12, key), pitch as i32 / 12 - 1);
        let sixteenth_ms = 60_000.0 / self.p.bpm.get() as f32 / 4.0;

        // The phrase: hidden until you've played (it's by ear), then shown.
        for (i, &(at, pitch, len)) in ex.notes.iter().enumerate() {
            let x = x_of_16th(at as f32);
            match (&outcome, ex.kind) {
                (None, _) => text(canvas, "?", x + 4.0, row_target + 4.0, 13.0, pal.ink_faint),
                (Some(_), Kind::Rhythm) => dot(canvas, x, row_target, 6.0, pal.ink, false),
                (Some(o), Kind::Melody) => {
                    let ok = matches!(o.notes.get(i), Some(NoteResult::Right | NoteResult::Octave));
                    note_box(canvas, x, x_of_16th((at + len) as f32), row_target, &name(pitch), if ok { pal.signal_soft } else { pal.bg_200 }, pal.ink);
                }
            }
        }
        let Some(o) = outcome else { return };
        match ex.kind {
            Kind::Rhythm => {
                for (i, hit) in o.hits.iter().enumerate() {
                    let x = x_of_16th(ex.notes[i].0 as f32);
                    let (off, color, label) = match *hit {
                        Hit::OnTime(ms) => (ms, pal.signal, "on time".to_string()),
                        Hit::Early(ms) => (ms, pal.warn, format!("{:.0} ms early", -ms)),
                        Hit::Late(ms) => (ms, pal.warn, format!("{ms:.0} ms late")),
                        Hit::Missed => {
                            text(canvas, "missed", x - 4.0, row_played + 4.0, 11.0, pal.record);
                            continue;
                        }
                    };
                    let px = x + off / sixteenth_ms * step_w;
                    dot(canvas, px, row_played, 6.0, color, true);
                    text(canvas, &label, px - 14.0, row_played + 20.0, 10.0, pal.ink_muted);
                }
                // Taps that matched nothing.
                let matched: usize = o.hits.iter().filter(|h| !matches!(h, Hit::Missed)).count();
                if o.extra > 0 && o.played.len() > matched {
                    for &(ms, _) in &o.played {
                        let near = o.hits.iter().enumerate().any(|(i, h)| {
                            let t = ex.notes[i].0 as f32 * sixteenth_ms;
                            !matches!(h, Hit::Missed) && (ms - t).abs() < 1.0 + 180.0
                        });
                        if !near {
                            let px = x_of_16th(0.0) + ms / sixteenth_ms * step_w;
                            text(canvas, "\u{d7}", px - 3.0, row_played + 4.0, 13.0, pal.record);
                        }
                    }
                }
            }
            Kind::Melody => {
                for (i, result) in o.notes.iter().enumerate() {
                    let Some(&(ms, pitch)) = o.played.get(i) else {
                        let x = x_of_16th(ex.notes[i].0 as f32);
                        text(canvas, "missed", x, row_played + 4.0, 11.0, pal.record);
                        continue;
                    };
                    let x = x_of_16th((ms / sixteenth_ms).clamp(0.0, steps as f32 - 1.0));
                    let fill_color = match result {
                        NoteResult::Right => pal.signal_soft,
                        NoteResult::Octave => pal.signal_soft,
                        _ => pal.bg_300,
                    };
                    note_box(canvas, x, x + step_w * 2.0, row_played, &name(pitch), fill_color, if matches!(result, NoteResult::Wrong(_)) { pal.record } else { pal.ink });
                }
            }
        }
    }
}

fn dot(canvas: &Canvas, x: f32, y: f32, r: f32, color: Color, filled: bool) {
    let mut paint = vg::Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(color);
    if !filled {
        paint.set_style(vg::PaintStyle::Stroke);
        paint.set_stroke_width(2.0);
    }
    canvas.draw_circle(vg::Point::new(x, y), r, &paint);
}

fn note_box(canvas: &Canvas, x0: f32, x1: f32, y: f32, label: &str, bg: Color, ink: Color) {
    let rect = vg::Rect::new(x0 + 2.0, y - 9.0, (x1 - 2.0).max(x0 + 34.0), y + 9.0);
    let mut paint = vg::Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(bg);
    canvas.draw_rrect(vg::RRect::new_rect_xy(rect, 3.0, 3.0), &paint);
    text(canvas, label, rect.left + 5.0, y + 4.0, 11.0, ink);
}

fn fill(canvas: &Canvas, rect: vg::Rect, color: Color) {
    let mut paint = vg::Paint::default();
    paint.set_color(color);
    paint.set_anti_alias(true);
    canvas.draw_path(&vg::Path::rect(rect, None), &paint);
}

fn text(canvas: &Canvas, s: &str, x: f32, y: f32, size: f32, color: Color) {
    let font = crate::canvas_text::canvas_font(size);
    let mut paint = vg::Paint::default();
    paint.set_color(color);
    paint.set_anti_alias(true);
    canvas.draw_str(s, vg::Point::new(x, y), &font, &paint);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_bars_have_names() {
        assert_eq!(phase(0.5, 1).0, "Count in");
        assert_eq!(phase(1.2, 1).0, "Listen");
        assert_eq!(phase(3.9, 1).0, "Your turn");
        // A two-bar phrase: listen for two bars, play for two.
        assert_eq!(phase(2.5, 2).0, "Listen");
        assert_eq!(phase(3.5, 2).0, "Get ready");
        assert_eq!(phase(5.5, 2).0, "Your turn");
        let o = Outcome { hits: vec![Hit::OnTime(5.0), Hit::Late(90.0)], extra: 1, notes: vec![], played: vec![] };
        assert_eq!(o.summary(Kind::Rhythm), "1 of 2 on time \u{b7} 1 extra");
        assert!(!o.perfect(Kind::Rhythm));
    }
}
