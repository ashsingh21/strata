//! Ear: learn to play on the guitar what you hear in your head, in five
//! steps (`shared::ear`): which way a tune moves, how far, where that is
//! on a string, short tunes, then tunes you know. Every sound is named on
//! screen as it plays (1st note, 2nd note...), the question is one plain
//! sentence, and you answer with a button or on a guitar neck - tapped,
//! or played on your real guitar into the mic (or a MIDI keyboard). A
//! Practice tab, like Riyaz.

use std::sync::Arc;
use std::time::{Duration, Instant};

use vizia::prelude::*;
use vizia::vg;

use shared::ear::{self, Choice, NoteFollower, Question, Step};

use crate::hidpi::Logical;
use crate::synth::state::SynthEvent;
use crate::tokens::{self, ThemeId};

/// Mic readings per second at most.
const HOP_SECS: f32 = 1.0 / 40.0;
/// After a right answer, the next question comes by itself.
const NEXT_AFTER: Duration = Duration::from_millis(1300);
/// Clarity a mic reading needs to count as a note.
const CLEAR: f32 = 0.75;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    /// The audio is being made.
    Loading,
    /// The question is playing: no answers yet (the mic would hear it).
    Playing,
    Answering,
    Done,
}

pub enum EarEvent {
    ToggleOpen,
    SetStep(Step),
    Next,
    /// The question again (Tunes you know: its first note, until answered).
    Again,
    Choose(Choice),
    /// The neck, tapped.
    Tap { string: usize, fret: u8 },
    ToggleMic,
    Rendered { generation: u64, audio: Arc<[f32]>, notes: usize, question: bool },
    Tick,
}

/// A note you played, and where it shows on the neck.
pub type Played = (u8, Option<(usize, u8)>);

pub struct EarModel {
    pub open: Signal<bool>,
    pub step: Signal<Step>,
    pub phase: Signal<Phase>,
    pub question: Signal<Option<Question>>,
    pub played: Signal<Vec<Played>>,
    pub chose: Signal<Option<Choice>>,
    /// Each answer right or not, once answered.
    pub result: Signal<Option<Vec<bool>>>,
    pub score: Signal<u32>,
    pub streak: Signal<u32>,
    pub banner: Signal<String>,
    /// Which of the question's notes is sounding now.
    pub sounding: Signal<Option<usize>>,
    pub mic_on: Signal<bool>,
    pub no_input: Signal<bool>,
    /// The note the mic hears now.
    pub hearing: Signal<Option<u8>>,
    player: crate::preview_player::SharedPlayer,
    mic: crate::mic::SharedMic,
    sample_rate: u32,
    generation: u64,
    token: Option<u64>,
    /// What's playing: since when, how many notes, and whether it's the
    /// question (answering opens after).
    playing: Option<(Instant, usize, bool)>,
    /// The mic ignores the speakers until then.
    quiet_until: Option<Instant>,
    next_at: Option<Instant>,
    follower: NoteFollower,
    started: Instant,
    last_reading: f32,
    seed: u32,
}

impl EarModel {
    pub fn new(player: crate::preview_player::SharedPlayer, mic: crate::mic::SharedMic, sample_rate: u32) -> Self {
        Self {
            open: Signal::new(false),
            step: Signal::new(Step::Direction),
            phase: Signal::new(Phase::Idle),
            question: Signal::new(None),
            played: Signal::new(Vec::new()),
            chose: Signal::new(None),
            result: Signal::new(None),
            score: Signal::new(0),
            streak: Signal::new(0),
            banner: Signal::new(String::new()),
            sounding: Signal::new(None),
            mic_on: Signal::new(false),
            no_input: Signal::new(false),
            hearing: Signal::new(None),
            player,
            mic,
            sample_rate,
            generation: 0,
            token: None,
            playing: None,
            quiet_until: None,
            next_at: None,
            follower: NoteFollower::default(),
            started: Instant::now(),
            last_reading: 0.0,
            seed: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(1) | 1,
        }
    }

    fn stop_sound(&mut self) {
        self.generation += 1;
        if let Some(token) = self.token.take() {
            self.player.borrow_mut().stop_if(token);
        }
        self.playing = None;
        self.sounding.set(None);
    }

    fn reset(&mut self) {
        self.stop_sound();
        self.next_at = None;
        self.question.set(None);
        self.played.set(Vec::new());
        self.chose.set(None);
        self.result.set(None);
        self.banner.set(String::new());
        self.phase.set(Phase::Idle);
        self.streak.set(0);
    }

    /// Plays the question's first `count` notes.
    fn play(&mut self, cx: &mut EventContext, count: usize, question: bool) {
        let Some(q) = self.question.get() else { return };
        self.stop_sound();
        let notes: Vec<u8> = q.notes.iter().take(count).copied().collect();
        let generation = self.generation;
        let sample_rate = self.sample_rate;
        cx.spawn(move |proxy| {
            let mut project = ear::project(&notes);
            project.migrate();
            let patches = project.instruments.into_iter().collect();
            let job = engine::render::RenderJob { arrangement: project.arrangement, patches, sources: Default::default(), sample_rate };
            let audio = engine::render::render_between(&job, 0, ear::project_end(&notes), 0.6);
            let _ = proxy.emit(EarEvent::Rendered { generation, audio: Arc::from(audio), notes: notes.len(), question });
        });
    }

    fn next(&mut self, cx: &mut EventContext) {
        self.next_at = None;
        self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let avoid = self.question.get().map(|q| q.notes).unwrap_or_default();
        let q = ear::question(self.step.get(), self.seed, &avoid);
        let count = if q.plays_all() { q.notes.len() } else { 1 };
        self.question.set(Some(q));
        self.played.set(Vec::new());
        self.chose.set(None);
        self.result.set(None);
        self.banner.set(String::new());
        self.phase.set(Phase::Loading);
        self.play(cx, count, true);
    }

    fn choose(&mut self, cx: &mut EventContext, choice: Choice) {
        let Some(q) = self.question.get() else { return };
        if self.phase.get() != Phase::Answering || q.choice().is_none() {
            return;
        }
        self.chose.set(Some(choice));
        self.finish(cx, vec![q.choice() == Some(choice)]);
    }

    /// A note from the neck, a keyboard or the mic.
    fn input(&mut self, cx: &mut EventContext, pitch: u8, at: Option<(usize, u8)>) {
        let Some(q) = self.question.get() else { return };
        if self.phase.get() != Phase::Answering || q.choice().is_some() {
            return;
        }
        let at = at.or_else(|| ear::position_of(pitch, q.given_at));
        let mut played = self.played.get();
        played.push((pitch, at));
        self.played.set(played.clone());
        if played.len() >= q.to_find().len() {
            let pitches: Vec<u8> = played.iter().map(|p| p.0).collect();
            self.finish(cx, ear::check(&q, &pitches));
        }
    }

    fn finish(&mut self, cx: &mut EventContext, result: Vec<bool>) {
        let Some(q) = self.question.get() else { return };
        let right = result.iter().all(|r| *r);
        self.result.set(Some(result));
        self.phase.set(Phase::Done);
        // A drill from Learn counts it.
        cx.emit(crate::learn::LearnEvent::Answered(right));
        if right {
            let streak = self.streak.get() + 1;
            let points = ear::points(streak - 1);
            self.streak.set(streak);
            self.score.set(self.score.get() + points);
            let mut banner = format!("Yes! +{points}");
            if streak == ear::READY_STREAK && q.step != Step::Known {
                banner = format!("{banner}  \u{b7}  {streak} in a row: ready for step {}", q.step.index() + 2);
            }
            self.banner.set(banner);
            // A tune you know: hear it whole, then on.
            let wait = if q.step == Step::Known {
                self.play(cx, q.notes.len(), false);
                Duration::from_secs_f32(ear::note_secs() * q.notes.len() as f32)
            } else {
                Duration::ZERO
            };
            self.next_at = Some(Instant::now() + NEXT_AFTER + wait);
        } else {
            self.streak.set(0);
            self.banner.set("Not quite".to_string());
            // On the neck: hear how it really goes, the dots lit as it plays.
            if q.choice().is_none() {
                self.play(cx, q.notes.len(), false);
            }
        }
    }

    fn listen(&mut self, cx: &mut EventContext) {
        self.mic.borrow_mut().feed();
        let t = self.started.elapsed().as_secs_f32();
        if t - self.last_reading < HOP_SECS {
            return;
        }
        self.last_reading = t;
        let Some(pitch) = self.mic.borrow().read() else { return };
        // While the speakers play, what the mic hears is us.
        let quiet = self.quiet_until.is_some_and(|q| Instant::now() < q);
        let midi = pitch.filter(|p| p.clarity >= CLEAR && !quiet).map(|p| ear::midi_of(p.hz));
        let hearing = midi.map(|m| m.round() as u8);
        if self.hearing.get() != hearing {
            self.hearing.set(hearing);
        }
        if let Some(note) = self.follower.push(midi) {
            self.input(cx, note, None);
        }
    }

    fn set_mic(&mut self, on: bool) {
        let mut mic = self.mic.borrow_mut();
        self.no_input.set(on && !mic.available());
        let on = mic.set_listening(on);
        drop(mic);
        self.follower.clear();
        self.hearing.set(None);
        if self.mic_on.get() != on {
            self.mic_on.set(on);
        }
    }
}

impl Model for EarModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        // Notes from a MIDI or computer keyboard answer on the neck too.
        event.map(|event, _| match event {
            SynthEvent::PlayNote(note, _) | SynthEvent::KeyPress(note) if self.open.get() => self.input(cx, *note, None),
            _ => {}
        });
        event.map(|event, _| match event {
            EarEvent::ToggleOpen => {
                let open = !self.open.get();
                self.open.set(open);
                if open {
                    if self.mic_on.get() {
                        self.set_mic(true);
                    }
                } else {
                    self.reset();
                    let was = self.mic_on.get();
                    self.set_mic(false);
                    // Remembered for next time.
                    self.mic_on.set(was);
                }
            }
            EarEvent::SetStep(step) => {
                if self.step.get() != *step {
                    self.step.set(*step);
                    self.reset();
                }
            }
            EarEvent::Next => self.next(cx),
            EarEvent::Again => {
                self.next_at = None;
                if let Some(q) = self.question.get() {
                    let phase = self.phase.get();
                    let all = q.plays_all() || phase == Phase::Done;
                    // Before answering, the question again (no answers while it plays).
                    self.play(cx, if all { q.notes.len() } else { 1 }, phase != Phase::Done);
                }
            }
            EarEvent::Choose(choice) => self.choose(cx, *choice),
            EarEvent::Tap { string, fret } => self.input(cx, ear::pitch_at(*string, *fret), Some((*string, *fret))),
            EarEvent::ToggleMic => self.set_mic(!self.mic_on.get()),
            EarEvent::Rendered { generation, audio, notes, question } => {
                if *generation != self.generation || !self.open.get() {
                    return;
                }
                cx.emit(crate::app::AppEvent::Stop);
                let secs = audio.len() as f64 / 2.0 / self.sample_rate.max(1) as f64;
                self.token = self.player.borrow_mut().play(audio.to_vec(), false);
                let now = Instant::now();
                self.quiet_until = Some(now + Duration::from_secs_f64(secs));
                self.follower.clear();
                self.playing = Some((now, *notes, *question));
                if *question {
                    self.phase.set(Phase::Playing);
                }
            }
            EarEvent::Tick => {
                if !self.open.get() {
                    return;
                }
                let now = Instant::now();
                if let Some((since, count, question)) = self.playing {
                    let at = (now - since).as_secs_f32() / ear::note_secs();
                    let sounding = (at < count as f32).then_some(at as usize);
                    if self.sounding.get() != sounding {
                        self.sounding.set(sounding);
                    }
                    if sounding.is_none() {
                        self.playing = None;
                        if question && self.phase.get() == Phase::Playing {
                            self.phase.set(Phase::Answering);
                        }
                    }
                }
                if self.next_at.is_some_and(|t| now >= t) {
                    self.next(cx);
                }
                if self.mic_on.get() {
                    self.listen(cx);
                }
            }
        });
    }
}

#[derive(Clone, Copy)]
pub struct EarProps {
    pub open: Signal<bool>,
    pub step: Signal<Step>,
    pub phase: Signal<Phase>,
    pub question: Signal<Option<Question>>,
    pub played: Signal<Vec<Played>>,
    pub chose: Signal<Option<Choice>>,
    pub result: Signal<Option<Vec<bool>>>,
    pub score: Signal<u32>,
    pub streak: Signal<u32>,
    pub banner: Signal<String>,
    pub sounding: Signal<Option<usize>>,
    pub mic_on: Signal<bool>,
    pub no_input: Signal<bool>,
    pub hearing: Signal<Option<u8>>,
    pub theme: Signal<ThemeId>,
}

impl EarProps {
    pub fn of(m: &EarModel, theme: Signal<ThemeId>) -> Self {
        Self {
            open: m.open,
            step: m.step,
            phase: m.phase,
            question: m.question,
            played: m.played,
            chose: m.chose,
            result: m.result,
            score: m.score,
            streak: m.streak,
            banner: m.banner,
            sounding: m.sounding,
            mic_on: m.mic_on,
            no_input: m.no_input,
            hearing: m.hearing,
            theme,
        }
    }
}

pub fn ear_view(cx: &mut Context, p: EarProps) {
    Binding::new(cx, p.open, move |cx| {
        if !p.open.get() {
            return;
        }
        VStack::new(cx, move |cx| {
            header(cx, p);
            HStack::new(cx, move |cx| {
                // What to do, what's playing, how it went, what next.
                VStack::new(cx, move |cx| {
                    Label::new(cx, Memo::new(move |_| question_text(p))).class("label-lg").class("ear-question").text_wrap(true).width(Stretch(1.0));
                    VStack::new(cx, move |cx| chips(cx, p)).height(Pixels(30.0)).width(Stretch(1.0));
                    Label::new(cx, Memo::new(move |_| status_text(p))).class("value").text_wrap(true).width(Stretch(1.0)).height(Pixels(18.0));
                    HStack::new(cx, move |cx| {
                        Button::new(cx, move |cx| Label::new(cx, p.question.map(|q| if q.is_none() { "\u{25b8} Start" } else { "Next \u{203a}" })))
                            .class("btn")
                            .class("lg")
                            .class("is-on")
                            .on_press(|cx| cx.emit(EarEvent::Next));
                        Button::new(cx, |cx| Label::new(cx, "\u{21bb} Play again"))
                            .class("btn")
                            .class("lg")
                            .toggle_class("hidden", p.question.map(|q| q.is_none()))
                            .on_press(|cx| cx.emit(EarEvent::Again));
                    })
                    .gap(Pixels(tokens::SPACE_2))
                    .height(Auto);
                })
                .gap(Pixels(6.0))
                .width(Stretch(1.0))
                .height(Auto);
                // Where you answer.
                VStack::new(cx, move |cx| answer_area(cx, p)).width(Pixels(560.0)).height(Pixels(124.0));
            })
            .gap(Pixels(tokens::SPACE_4))
            .width(Stretch(1.0))
            .height(Auto);
        })
        .class("device")
        .gap(Pixels(tokens::SPACE_3))
        .padding(Pixels(tokens::SPACE_3))
        .width(Stretch(1.0))
        .height(Auto);
    });
}

fn header(cx: &mut Context, p: EarProps) {
    HStack::new(cx, move |cx| {
        crate::synth::segmented::segmented(
            cx,
            Step::ALL.len(),
            |cx, i| Label::new(cx, format!("{}  {}", i + 1, Step::ALL[i].title())),
            move |i| p.step.map(move |s| *s == Step::ALL[i]),
            |cx, i| cx.emit(EarEvent::SetStep(Step::ALL[i])),
        )
        // A drill from Learn has its own bar naming the step.
        .toggle_class("hidden", Memo::new(|_| crate::learn::props().is_some_and(|l| l.run.get().is_some_and(|r| matches!(r.drill, crate::learn::goals::Drill::Ear(_))))))
        .height(Pixels(tokens::SIZE_CONTROL));
        Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
        Button::new(cx, move |cx| {
            Label::new(
                cx,
                Memo::new(move |_| {
                    if p.no_input.get() {
                        "No mic".to_string()
                    } else if p.mic_on.get() {
                        "\u{25cf} Hearing your guitar".to_string()
                    } else {
                        "Use my guitar (mic)".to_string()
                    }
                }),
            )
        })
        .class("btn")
        .class("sm")
        .toggle_class("is-rec", p.mic_on)
        .on_press(|cx| cx.emit(EarEvent::ToggleMic))
        .tooltip(|cx| Tooltip::new(cx, |cx| { Label::new(cx, "Answer the neck steps by playing your guitar into the mic"); }).arrow(false));
        Label::new(
            cx,
            Memo::new(move |_| match p.streak.get() {
                0 => format!("{} points", p.score.get()),
                n => format!("{} points  \u{b7}  {n} in a row", p.score.get()),
            }),
        )
        .class("value");
    })
    .gap(Pixels(tokens::SPACE_3))
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Pixels(tokens::SIZE_CONTROL));
}

/// The one thing to do, in a sentence.
fn question_text(p: EarProps) -> String {
    let step = p.step.get();
    let Some(q) = p.question.get() else {
        return format!("Step {}: {}. {}", step.index() + 1, step.title(), step.why());
    };
    match q.step {
        Step::Direction => "Two notes play. Is the 2nd one higher or lower than the 1st?".to_string(),
        Step::Distance => "Two notes play. Is the 2nd one a step away (1 or 2 frets) or a jump?".to_string(),
        Step::OneString => format!("The 1st note is {}, lit on the A string. Find the 2nd on the same string.", ear::name(q.notes[0])),
        Step::Tunes => format!("A {}-note tune starts on the lit {}. Play the other {}.", q.notes.len(), ear::name(q.notes[0]), q.to_find().len()),
        Step::Known => format!(
            "Play the start of {} from memory: {} notes, starting on the lit {}.",
            q.title.unwrap_or("the tune"),
            q.notes.len(),
            ear::name(q.notes[0])
        ),
    }
}

/// What's happening, or how it went.
fn status_text(p: EarProps) -> String {
    let Some(q) = p.question.get() else { return "Press Start, then listen.".to_string() };
    let on_neck = q.choice().is_none();
    let hearing = p.hearing.get().filter(|_| p.mic_on.get() && on_neck).map(|h| format!("  (hearing {})", ear::name(h))).unwrap_or_default();
    match p.phase.get() {
        Phase::Idle | Phase::Loading => "Listen\u{2026}".to_string(),
        Phase::Playing if q.step == Step::Known => format!("This is your 1st note: {}. The rest is in your head.", ear::name(q.notes[0])),
        Phase::Playing => "Listen\u{2026} (the lit box shows which note is playing)".to_string(),
        Phase::Answering if !on_neck => "Pick one on the right.".to_string(),
        Phase::Answering => {
            let n = p.played.get().len();
            format!("Tap the {} note on the neck, or play it on your guitar.{hearing}", ear::ordinal(n + 1))
        }
        Phase::Done => {
            let right = p.result.get().is_some_and(|r| r.iter().all(|x| *x));
            let d = q.notes[1] as i32 - q.notes[0] as i32;
            let frets = |d: i32| if d.abs() == 1 { "1 fret".to_string() } else { format!("{} frets", d.abs()) };
            let how = match q.choice() {
                Some(Choice::Same) => "It was the same note.".to_string(),
                Some(Choice::Higher) => format!("It went higher, by {}.", frets(d)),
                Some(Choice::Lower) => format!("It went lower, by {}.", frets(d)),
                Some(Choice::Step) => format!("A step: {}.", frets(d)),
                Some(Choice::Jump) => format!("A jump: {}.", frets(d)),
                None => format!("It goes {}.", q.notes.iter().map(|&n| ear::name(n)).collect::<Vec<_>>().join(" ")),
            };
            if right {
                format!("{}  {how}", p.banner.get())
            } else if on_neck {
                format!("Not quite. {how} Green shows where.")
            } else {
                format!("Not quite. {how} Play again to hear it.")
            }
        }
    }
}

/// A box for each note of the question, lit while it plays: "1st", "2nd"
/// (the first's name always, the rest once answered).
fn chips(cx: &mut Context, p: EarProps) {
    let shape = Memo::new(move |_| (p.question.get(), p.result.get(), p.sounding.get()));
    Binding::new(cx, shape, move |cx| {
        let (Some(q), result, sounding) = shape.get() else { return };
        let on_neck = q.choice().is_none();
        HStack::new(cx, move |cx| {
            for i in 0..q.notes.len() {
                let text = match (i, on_neck, result.is_some()) {
                    (_, false, _) => format!("{} note", ear::ordinal(i)),
                    (0, true, _) | (_, true, true) => format!("{} \u{b7} {}", ear::ordinal(i), ear::name(q.notes[i])),
                    _ => format!("{} \u{b7} ?", ear::ordinal(i)),
                };
                let ok = if on_neck && i > 0 { result.as_ref().and_then(|r| r.get(i - 1).copied()) } else { None };
                Label::new(cx, text)
                    .class("ear-chip")
                    .height(Pixels(28.0))
                    .padding_left(Pixels(12.0))
                    .padding_right(Pixels(12.0))
                    .alignment(Alignment::Center)
                    .toggle_class("is-sounding", sounding == Some(i))
                    .toggle_class("is-given", i == 0 && on_neck)
                    .toggle_class("is-right", ok == Some(true))
                    .toggle_class("is-wrong", ok == Some(false));
            }
        })
        .gap(Pixels(6.0))
        .size(Auto);
    });
}

/// Buttons for the first two steps, the neck for the rest.
fn answer_area(cx: &mut Context, p: EarProps) {
    let on_neck = Memo::new(move |_| matches!(p.step.get(), Step::OneString | Step::Tunes | Step::Known));
    Binding::new(cx, on_neck, move |cx| {
        if on_neck.get() {
            Fretboard::new(cx, p).width(Stretch(1.0)).height(Stretch(1.0));
            return;
        }
        let choices = Memo::new(move |_| match p.step.get() {
            Step::Direction => vec![Choice::Lower, Choice::Same, Choice::Higher],
            _ => vec![Choice::Step, Choice::Jump],
        });
        Binding::new(cx, choices, move |cx| {
            HStack::new(cx, move |cx| {
                for choice in choices.get() {
                    let marks = Memo::new(move |_| {
                        let (Some(q), Some(_)) = (p.question.get(), p.result.get()) else { return (false, false) };
                        let picked = p.chose.get() == Some(choice);
                        (q.choice() == Some(choice), picked && q.choice() != Some(choice))
                    });
                    let arrow = match choice {
                        Choice::Lower => "\u{2193}  ",
                        Choice::Higher => "\u{2191}  ",
                        Choice::Same => "=  ",
                        _ => "",
                    };
                    Button::new(cx, move |cx| Label::new(cx, format!("{arrow}{}", choice.name())).hoverable(false))
                        .class("btn")
                        .class("ear-choice")
                        .toggle_class("is-right", marks.map(|m| m.0))
                        .toggle_class("is-wrong", marks.map(|m| m.1))
                        .on_press(move |cx| cx.emit(EarEvent::Choose(choice)));
                }
            })
            .gap(Pixels(tokens::SPACE_2))
            .alignment(Alignment::Center)
            .width(Stretch(1.0))
            .height(Stretch(1.0));
        });
    });
}

/// A guitar neck, open strings to the 12th fret, high e on top as in tab:
/// the given note lit, your notes, and - once answered - where the right
/// ones are. Tap a string at a fret to answer.
struct Fretboard {
    p: EarProps,
}

impl Fretboard {
    fn new(cx: &mut Context, p: EarProps) -> Handle<'_, Self> {
        Self { p }
            .build(cx, |_| {})
            .bind(p.question, |mut h| h.needs_redraw())
            .bind(p.played, |mut h| h.needs_redraw())
            .bind(p.result, |mut h| h.needs_redraw())
            .bind(p.phase, |mut h| h.needs_redraw())
            .bind(p.sounding, |mut h| h.needs_redraw())
            .bind(p.theme, |mut h| h.needs_redraw())
    }
}

/// The neck's layout in `b`: (left edge, a fret's width, top string's y,
/// the gap between strings).
fn neck(b: BoundingBox) -> (f32, f32, f32, f32) {
    let left = b.x + 24.0;
    let col = (b.w - 24.0 - 6.0) / (ear::FRETS as f32 + 1.0);
    let top = b.y + 22.0;
    let row = (b.h - 22.0 - 9.0) / 5.0;
    (left, col, top, row)
}

fn spot(b: BoundingBox, (string, fret): (usize, u8)) -> (f32, f32) {
    let (left, col, top, row) = neck(b);
    (left + (fret as f32 + 0.5) * col, top + (5 - string) as f32 * row)
}

impl View for Fretboard {
    fn element(&self) -> Option<&'static str> {
        Some("fretboard")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, _| {
            if let WindowEvent::MouseDown(MouseButton::Left) = window_event {
                let b = cx.lbounds();
                let (mx, my) = cx.lmouse();
                let (left, col, top, row) = neck(b);
                let fret = ((mx - left) / col).floor();
                let from_top = ((my - top) / row).round();
                if (0.0..=ear::FRETS as f32).contains(&fret) && (0.0..=5.0).contains(&from_top) && (my - (top + from_top * row)).abs() < row * 0.6 {
                    cx.emit(EarEvent::Tap { string: 5 - from_top as usize, fret: fret as u8 });
                }
            }
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let b = cx.lbounds();
        crate::hidpi::clip(canvas, b);
        let pal = self.p.theme.get().palette();
        let (left, col, top, row) = neck(b);
        let bottom = top + 5.0 * row;
        rrect(canvas, vg::Rect::new(b.x, b.y, b.x + b.w, b.y + b.h), 6.0, pal.bg_000);
        let q = self.p.question.get();
        let step = self.p.step.get();

        // Fret numbers, inlays, frets and the nut.
        for fret in 0..=ear::FRETS {
            let cxm = left + (fret as f32 + 0.5) * col;
            centered(canvas, &fret.to_string(), cxm, b.y + 13.0, 10.0, if [3, 5, 7, 9, 12].contains(&fret) { pal.ink_muted } else { pal.ink_faint });
            if [3, 5, 7, 9].contains(&fret) {
                dot(canvas, cxm, top + 2.5 * row, 3.0, pal.bg_300);
            } else if fret == 12 {
                dot(canvas, cxm, top + 1.5 * row, 3.0, pal.bg_300);
                dot(canvas, cxm, top + 3.5 * row, 3.0, pal.bg_300);
            }
            let x = left + (fret as f32 + 1.0) * col;
            fill(canvas, vg::Rect::new(x, top, x + if fret == 0 { 3.0 } else { 1.0 }, bottom), if fret == 0 { pal.ink_muted } else { pal.line_control });
        }
        // Strings, high e on top; the one to stay on in One string drawn bold.
        for s in 0..6 {
            let y = top + (5 - s) as f32 * row;
            let active = step == Step::OneString && s == 1;
            let w = if active { 2.5 } else { 1.0 + (5 - s) as f32 * 0.25 };
            fill(canvas, vg::Rect::new(left, y - w / 2.0, b.x + b.w - 6.0, y + w / 2.0), if active { pal.ink } else { pal.ink_faint });
            text(canvas, ear::STRING_NAMES[s], b.x + 8.0, y + 4.0, 11.0, if active { pal.ink } else { pal.ink_muted });
        }
        // The box at the 5th fret, faintly, for short tunes.
        if step == Step::Tunes {
            for &at in &ear::BOX {
                let (x, y) = spot(b, at);
                dot(canvas, x, y, row * 0.36, pal.bg_200);
            }
        }
        let Some(q) = q else { return };
        let r = (row * 0.42).min(col * 0.42);
        let result = self.p.result.get();
        let sounding = self.p.sounding.get();
        // Once answered: where the ones you missed are (lit as each plays) -
        // on the A string for One string.
        if let Some(result) = &result {
            for (i, &note) in q.notes.iter().enumerate().skip(1) {
                if result.get(i - 1) == Some(&true) {
                    continue;
                }
                let at = if q.step == Step::OneString { Some((1, note - ear::TUNING[1])) } else { ear::position_of(note, q.given_at) };
                if let Some(at) = at {
                    let (x, y) = spot(b, at);
                    if sounding == Some(i) {
                        dot(canvas, x, y, r + 3.0, pal.signal_soft);
                    }
                    ring(canvas, x, y, r, pal.signal);
                    centered(canvas, ear::name(note), x, y + 4.0, 10.0, pal.signal);
                }
            }
        }
        // Yours.
        for (i, &(pitch, at)) in self.p.played.get().iter().enumerate() {
            let Some(at) = at else { continue };
            let (x, y) = spot(b, at);
            let color = match result.as_ref().and_then(|r| r.get(i).copied()) {
                Some(true) => pal.signal,
                Some(false) => pal.record,
                None => pal.ink_muted,
            };
            dot(canvas, x, y, r, color);
            centered(canvas, ear::name(pitch), x, y + 4.0, 10.0, pal.bg_000);
        }
        // The given note, on top.
        let (x, y) = spot(b, q.given_at);
        if sounding == Some(0) {
            dot(canvas, x, y, r + 3.0, pal.signal_soft);
        }
        dot(canvas, x, y, r, pal.ink);
        centered(canvas, ear::name(q.notes[0]), x, y + 4.0, 10.0, pal.bg_000);
    }
}

fn rrect(canvas: &Canvas, rect: vg::Rect, r: f32, color: Color) {
    let mut paint = vg::Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(color);
    canvas.draw_rrect(vg::RRect::new_rect_xy(rect, r, r), &paint);
}

fn fill(canvas: &Canvas, rect: vg::Rect, color: Color) {
    let mut paint = vg::Paint::default();
    paint.set_color(color);
    paint.set_anti_alias(true);
    canvas.draw_rect(rect, &paint);
}

fn dot(canvas: &Canvas, x: f32, y: f32, r: f32, color: Color) {
    let mut paint = vg::Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(color);
    canvas.draw_circle(vg::Point::new(x, y), r, &paint);
}

fn ring(canvas: &Canvas, x: f32, y: f32, r: f32, color: Color) {
    let mut paint = vg::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(vg::PaintStyle::Stroke);
    paint.set_stroke_width(2.0);
    paint.set_color(color);
    canvas.draw_circle(vg::Point::new(x, y), r, &paint);
}

fn text(canvas: &Canvas, s: &str, x: f32, y: f32, size: f32, color: Color) {
    let font = crate::canvas_text::canvas_font(size);
    let mut paint = vg::Paint::default();
    paint.set_color(color);
    paint.set_anti_alias(true);
    canvas.draw_str(s, vg::Point::new(x, y), &font, &paint);
}

fn centered(canvas: &Canvas, s: &str, cx: f32, y: f32, size: f32, color: Color) {
    let font = crate::canvas_text::canvas_font(size);
    let (w, _) = font.measure_str(s, None);
    text(canvas, s, cx - w / 2.0, y, size, color);
}
