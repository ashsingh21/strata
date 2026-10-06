//! Ear: hear a melody inside, then find it on your instrument. Scale
//! degrees in the song's key - find one note, echo a short melody, or
//! imagine one from its numbers and play it - answered on the pads, a
//! MIDI or computer keyboard, or a guitar (or voice) into the mic. The
//! rules and questions are `shared::ear`; this plays them, listens and
//! keeps score. A Practice tab, like Riyaz.

use std::sync::Arc;
use std::time::{Duration, Instant};

use vizia::prelude::*;

use shared::ear::{self, Mode, NoteFollower, Question};
use shared::theory::note_name_for_key;

use crate::synth::state::SynthEvent;
use crate::tokens;

/// Mic readings per second at most.
const HOP_SECS: f32 = 1.0 / 40.0;
/// After a right answer, the next question comes by itself.
const NEXT_AFTER: Duration = Duration::from_millis(1100);
/// The question's tempo.
const BPM: f64 = 100.0;
/// Clarity a mic reading needs to count as a note.
const CLEAR: f32 = 0.75;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    /// The audio is being made.
    Loading,
    /// The key and question are playing: no answers yet (the mic would
    /// hear the speakers).
    Playing,
    Answering,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// The question (and the key before it): answering opens after.
    Question,
    /// Anything else heard: a pad, your notes, the answer.
    Listen,
}

pub enum EarEvent {
    ToggleOpen,
    SetMode(Mode),
    SetLevel(usize),
    /// A new question.
    Next,
    /// The question again (Imagine: the answer).
    Again,
    /// The key alone: home's chords.
    Key,
    /// What you played, to hear it against the question.
    Mine,
    /// A pad: this degree.
    Pad(u8),
    ToggleMic,
    ToggleSargam,
    Rendered { generation: u64, audio: Arc<[f32]>, purpose: Purpose },
    Tick,
}

pub struct EarModel {
    pub open: Signal<bool>,
    pub mode: Signal<Mode>,
    pub level: Signal<usize>,
    pub phase: Signal<Phase>,
    pub question: Signal<Option<Question>>,
    /// What you've played for it, as pitches.
    pub played: Signal<Vec<u8>>,
    /// Each note right or not, once answered.
    pub result: Signal<Option<Vec<bool>>>,
    pub score: Signal<u32>,
    pub streak: Signal<u32>,
    pub best: Signal<u32>,
    /// What just happened, big: "Right! +20", "Level up!".
    pub banner: Signal<String>,
    pub mic_on: Signal<bool>,
    pub no_input: Signal<bool>,
    /// The note the mic hears now.
    pub hearing: Signal<Option<u8>>,
    pub sargam: Signal<bool>,
    key: Signal<u8>,
    scale_mask: Signal<u16>,
    player: crate::preview_player::SharedPlayer,
    mic: crate::mic::SharedMic,
    sample_rate: u32,
    generation: u64,
    token: Option<u64>,
    /// When the question finishes playing.
    playing_until: Option<Instant>,
    /// The mic ignores the speakers until then.
    quiet_until: Option<Instant>,
    next_at: Option<Instant>,
    /// The key needs playing before the next question (first one, or the
    /// song's key changed).
    need_key: bool,
    heard_key: (u8, u16),
    follower: NoteFollower,
    started: Instant,
    last_reading: f32,
    seed: u32,
}

impl EarModel {
    pub fn new(
        key: Signal<u8>,
        scale_mask: Signal<u16>,
        player: crate::preview_player::SharedPlayer,
        mic: crate::mic::SharedMic,
        sample_rate: u32,
    ) -> Self {
        Self {
            open: Signal::new(false),
            mode: Signal::new(Mode::Find),
            level: Signal::new(0),
            phase: Signal::new(Phase::Idle),
            question: Signal::new(None),
            played: Signal::new(Vec::new()),
            result: Signal::new(None),
            score: Signal::new(0),
            streak: Signal::new(0),
            best: Signal::new(0),
            banner: Signal::new(String::new()),
            mic_on: Signal::new(false),
            no_input: Signal::new(false),
            hearing: Signal::new(None),
            sargam: Signal::new(false),
            key,
            scale_mask,
            player,
            mic,
            sample_rate,
            generation: 0,
            token: None,
            playing_until: None,
            quiet_until: None,
            next_at: None,
            need_key: true,
            heard_key: (255, 0),
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
        self.playing_until = None;
    }

    /// Back to no question (a new mode or level, or closed).
    fn reset(&mut self) {
        self.stop_sound();
        self.next_at = None;
        self.question.set(None);
        self.played.set(Vec::new());
        self.result.set(None);
        self.banner.set(String::new());
        self.phase.set(Phase::Idle);
        self.streak.set(0);
    }

    fn render(&mut self, cx: &mut EventContext, project: shared::project::Project, purpose: Purpose) {
        self.stop_sound();
        let generation = self.generation;
        let sample_rate = self.sample_rate;
        cx.spawn(move |proxy| {
            let mut project = project;
            project.migrate();
            let end = ear::project_end(&project);
            let patches = project.instruments.into_iter().collect();
            let job = engine::render::RenderJob { arrangement: project.arrangement, patches, sources: Default::default(), sample_rate };
            let audio = engine::render::render_between(&job, 0, end, 0.6);
            let _ = proxy.emit(EarEvent::Rendered { generation, audio: Arc::from(audio), purpose });
        });
    }

    fn key_changed(&self) -> bool {
        self.heard_key != (self.key.get(), self.scale_mask.get())
    }

    fn next(&mut self, cx: &mut EventContext) {
        self.next_at = None;
        self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let avoid = self.question.get().map(|q| q.notes).unwrap_or_default();
        let q = ear::question(self.mode.get(), self.level.get(), self.key.get(), self.scale_mask.get(), self.seed, &avoid);
        let cadence = self.need_key || self.key_changed();
        self.need_key = false;
        self.heard_key = (self.key.get(), self.scale_mask.get());
        // Imagine shows the numbers and plays only the key: the melody is
        // yours to hear inside.
        let notes = q.mode != Mode::Imagine;
        self.question.set(Some(q.clone()));
        self.played.set(Vec::new());
        self.result.set(None);
        self.banner.set(String::new());
        if !cadence && !notes {
            self.phase.set(Phase::Answering);
            return;
        }
        self.phase.set(Phase::Loading);
        let (project, _) = ear::project(&q, self.scale_mask.get(), BPM, cadence, notes);
        self.render(cx, project, Purpose::Question);
    }

    /// Plays these notes, nothing else (a pad, your answer, the melody).
    fn hear(&mut self, cx: &mut EventContext, notes: Vec<u8>) {
        let Some(q) = self.question.get() else { return };
        let (project, _) = ear::project(&Question { notes, ..q }, self.scale_mask.get(), BPM, false, true);
        self.render(cx, project, Purpose::Listen);
    }

    /// A note from the pads, a keyboard or the mic.
    fn input(&mut self, cx: &mut EventContext, pitch: u8, from_pad: bool) {
        if self.phase.get() != Phase::Answering {
            return;
        }
        let Some(q) = self.question.get() else { return };
        let mut played = self.played.get();
        played.push(pitch);
        self.played.set(played.clone());
        if played.len() < q.notes.len() {
            // A pad sounds its note (a key or the guitar already did).
            if from_pad {
                self.hear(cx, vec![pitch]);
            }
            return;
        }
        let result = ear::check(&q, &played);
        let right = result.iter().all(|r| *r);
        self.result.set(Some(result.clone()));
        self.phase.set(Phase::Done);
        if right {
            let streak = self.streak.get() + 1;
            let points = ear::points(streak - 1, self.level.get());
            self.streak.set(streak);
            self.best.set(self.best.get().max(streak));
            self.score.set(self.score.get() + points);
            let combo = 1 + ((streak - 1) / 3).min(3);
            let mut banner = if combo > 1 { format!("Right! +{points}  \u{d7}{combo}") } else { format!("Right! +{points}") };
            if streak % ear::LEVEL_UP_STREAK == 0 && self.level.get() + 1 < ear::LEVELS {
                self.level.set(self.level.get() + 1);
                banner = format!("Level up! {}", ear::level_name(self.level.get()));
                // New notes: hear the key again with them.
                self.need_key = true;
            }
            self.banner.set(banner);
            match q.mode {
                // Hear what you imagined, then on.
                Mode::Imagine => {
                    self.hear(cx, q.notes.clone());
                    self.next_at = Some(Instant::now() + NEXT_AFTER + Duration::from_millis(600 * q.notes.len() as u64));
                }
                _ => {
                    if from_pad && q.mode == Mode::Echo {
                        self.hear(cx, vec![pitch]);
                    }
                    self.next_at = Some(Instant::now() + NEXT_AFTER);
                }
            }
        } else {
            self.streak.set(0);
            self.banner.set(if q.mode == Mode::Find { "Not quite".into() } else { format!("{} of {}", result.iter().filter(|r| **r).count(), result.len()) });
            match q.mode {
                // Yours, then the right one: hear the difference.
                Mode::Find => self.hear(cx, vec![played[0], q.notes[0]]),
                // What it should have been.
                Mode::Imagine => self.hear(cx, q.notes.clone()),
                Mode::Echo => {}
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
            self.input(cx, note, false);
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
        // Notes from a MIDI or computer keyboard answer too.
        event.map(|event, _| match event {
            SynthEvent::PlayNote(note, _) | SynthEvent::KeyPress(note) if self.open.get() => self.input(cx, *note, false),
            _ => {}
        });
        event.map(|event, _| match event {
            EarEvent::ToggleOpen => {
                let open = !self.open.get();
                self.open.set(open);
                if open {
                    // Listening again if it was before.
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
            EarEvent::SetMode(mode) => {
                if self.mode.get() != *mode {
                    self.mode.set(*mode);
                    self.reset();
                }
            }
            EarEvent::SetLevel(level) => {
                self.level.set(*level);
                self.need_key = true;
                self.reset();
            }
            EarEvent::Next => self.next(cx),
            EarEvent::Again => {
                self.next_at = None;
                if let Some(q) = self.question.get() {
                    if q.mode == Mode::Imagine && self.phase.get() != Phase::Done {
                        // Imagine: the key again (the melody's for after).
                        let (project, _) = ear::project(&q, self.scale_mask.get(), BPM, true, false);
                        self.render(cx, project, Purpose::Listen);
                    } else {
                        self.hear(cx, q.notes);
                    }
                }
            }
            EarEvent::Key => {
                self.next_at = None;
                let q = self.question.get().unwrap_or(Question { mode: self.mode.get(), key: self.key.get(), notes: Vec::new() });
                let q = Question { key: self.key.get(), ..q };
                let (project, _) = ear::project(&q, self.scale_mask.get(), BPM, true, false);
                self.heard_key = (self.key.get(), self.scale_mask.get());
                self.render(cx, project, Purpose::Listen);
            }
            EarEvent::Mine => {
                self.next_at = None;
                let played = self.played.get();
                let Some(q) = self.question.get() else { return };
                // Find: yours, then the right one.
                match (q.mode, played.first()) {
                    (Mode::Find, Some(&mine)) => self.hear(cx, vec![mine, q.notes[0]]),
                    _ if !played.is_empty() => self.hear(cx, played),
                    _ => {}
                }
            }
            EarEvent::Pad(degree) => {
                let Some(q) = self.question.get() else { return };
                self.input(cx, ear::home(q.key) + degree, true);
            }
            EarEvent::ToggleMic => self.set_mic(!self.mic_on.get()),
            EarEvent::ToggleSargam => self.sargam.set(!self.sargam.get()),
            EarEvent::Rendered { generation, audio, purpose } => {
                if *generation != self.generation || !self.open.get() {
                    return;
                }
                // The song stops for the ear.
                cx.emit(crate::app::AppEvent::Stop);
                let secs = audio.len() as f64 / 2.0 / self.sample_rate.max(1) as f64;
                self.token = self.player.borrow_mut().play(audio.to_vec(), false);
                let now = Instant::now();
                self.quiet_until = Some(now + Duration::from_secs_f64(secs));
                self.follower.clear();
                if *purpose == Purpose::Question {
                    self.phase.set(Phase::Playing);
                    // The tail rings on; answering opens as the last note ends.
                    self.playing_until = Some(now + Duration::from_secs_f64((secs - 0.6).max(0.0)));
                }
            }
            EarEvent::Tick => {
                if !self.open.get() {
                    return;
                }
                let now = Instant::now();
                if self.playing_until.is_some_and(|t| now >= t) {
                    self.playing_until = None;
                    if self.phase.get() == Phase::Playing {
                        self.phase.set(Phase::Answering);
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
    pub mode: Signal<Mode>,
    pub level: Signal<usize>,
    pub phase: Signal<Phase>,
    pub question: Signal<Option<Question>>,
    pub played: Signal<Vec<u8>>,
    pub result: Signal<Option<Vec<bool>>>,
    pub score: Signal<u32>,
    pub streak: Signal<u32>,
    pub best: Signal<u32>,
    pub banner: Signal<String>,
    pub mic_on: Signal<bool>,
    pub no_input: Signal<bool>,
    pub hearing: Signal<Option<u8>>,
    pub sargam: Signal<bool>,
    pub key: Signal<u8>,
    pub scale_mask: Signal<u16>,
}

impl EarProps {
    pub fn of(m: &EarModel) -> Self {
        Self {
            open: m.open,
            mode: m.mode,
            level: m.level,
            phase: m.phase,
            question: m.question,
            played: m.played,
            result: m.result,
            score: m.score,
            streak: m.streak,
            best: m.best,
            banner: m.banner,
            mic_on: m.mic_on,
            no_input: m.no_input,
            hearing: m.hearing,
            sargam: m.sargam,
            key: m.key,
            scale_mask: m.scale_mask,
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
            // One column, read top to bottom: what to do, the notes, the
            // pads to answer on, what next.
            VStack::new(cx, move |cx| {
                // A melody's notes take the heading's place (the line under
                // them says what to do), so the pads always fit.
                let melody = Memo::new(move |_| p.question.get().is_some_and(|q| q.mode != Mode::Find));
                Binding::new(cx, melody, move |cx| {
                    if melody.get() {
                        slots(cx, p);
                    } else {
                        Label::new(cx, Memo::new(move |_| prompt(p).0)).class("practice-status").width(Auto).height(Pixels(44.0)).alignment(Alignment::Center);
                    }
                });
                Label::new(cx, Memo::new(move |_| prompt(p).1)).class("value").width(Auto).height(Pixels(16.0));
                // The pads in the middle - staying put whatever the buttons
                // beside them say - and what to do next to their right.
                HStack::new(cx, move |cx| {
                    Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
                    pads(cx, p);
                    HStack::new(cx, move |cx| {
                        Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(36.0));
                        actions(cx, p);
                    })
                    .alignment(Alignment::Left)
                    .gap(Pixels(tokens::SPACE_4))
                    .width(Stretch(1.0))
                    .height(Auto);
                })
                .alignment(Alignment::Center)
                .gap(Pixels(tokens::SPACE_4))
                .width(Stretch(1.0))
                .height(Auto);
            })
            .alignment(Alignment::TopCenter)
            .gap(Pixels(tokens::SPACE_2))
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
            Mode::ALL.len(),
            |cx, i| Label::new(cx, Mode::ALL[i].name()),
            move |i| p.mode.map(move |m| *m == Mode::ALL[i]),
            |cx, i| cx.emit(EarEvent::SetMode(Mode::ALL[i])),
        )
        .height(Pixels(tokens::SIZE_CONTROL));
        Label::new(cx, "Level").class("label");
        crate::synth::segmented::segmented(
            cx,
            ear::LEVELS,
            |cx, i| Label::new(cx, format!("{}", i + 1)),
            move |i| p.level.map(move |l| *l == i),
            |cx, i| cx.emit(EarEvent::SetLevel(i)),
        )
        .height(Pixels(tokens::SIZE_CONTROL));
        Label::new(cx, p.level.map(|l| ear::level_name(*l))).class("value");
        Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(16.0));
        // In the song's key and scale.
        let key_text = Memo::new(move |_| {
            format!(
                "{} {}  \u{2304}",
                shared::theory::note_name(p.key.get()),
                crate::interval_input::state::scale_name(p.scale_mask.get()).to_lowercase()
            )
        });
        Button::new(cx, move |cx| Label::new(cx, key_text)).class("btn").class("sm").on_press(crate::key_menu::open_from);
        Button::new(cx, move |cx| Label::new(cx, p.sargam.map(|s| if *s { "Sargam" } else { "Numbers" })))
            .class("btn")
            .class("sm")
            .on_press(|cx| cx.emit(EarEvent::ToggleSargam))
            .tooltip(|cx| Tooltip::new(cx, |cx| { Label::new(cx, "Name the degrees as numbers or in sargam"); }).arrow(false));
        Button::new(cx, move |cx| {
            Label::new(
                cx,
                Memo::new(move |_| {
                    if p.no_input.get() {
                        "No mic".to_string()
                    } else if p.mic_on.get() {
                        "\u{25cf} Listening".to_string()
                    } else {
                        "Guitar / voice".to_string()
                    }
                }),
            )
        })
        .class("btn")
        .class("sm")
        .toggle_class("is-rec", p.mic_on)
        .on_press(|cx| cx.emit(EarEvent::ToggleMic))
        .tooltip(|cx| Tooltip::new(cx, |cx| { Label::new(cx, "Answer by playing your guitar (or singing) into the mic. Keys and MIDI always work."); }).arrow(false));
        Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
        Label::new(
            cx,
            Memo::new(move |_| match p.streak.get() {
                0 if p.best.get() == 0 => format!("{} points", p.score.get()),
                0 => format!("{} points  \u{b7}  best {} in a row", p.score.get(), p.best.get()),
                n => format!("{} points  \u{b7}  {n} in a row", p.score.get()),
            }),
        )
        .class("value");
    })
    .gap(Pixels(tokens::SPACE_2))
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Pixels(tokens::SIZE_CONTROL));
}

fn name_of(pitch: u8, key: u8, sargam: bool) -> String {
    ear::label(ear::degree_of(pitch, key), sargam)
}

/// What to do now, big, and a line under it.
fn prompt(p: EarProps) -> (String, String) {
    let sargam = p.sargam.get();
    let played = p.played.get();
    let hearing = p.hearing.get().filter(|_| p.mic_on.get());
    let Some(q) = p.question.get() else {
        return ("Ear training".to_string(), p.mode.get().how().to_string());
    };
    let heard = hearing.map(|h| format!("  \u{b7}  hearing {}", name_of(h, q.key, sargam))).unwrap_or_default();
    let answer = q.notes.iter().map(|&n| name_of(n, q.key, sargam)).collect::<Vec<_>>().join(" ");
    match p.phase.get() {
        Phase::Idle => ("Ear training".to_string(), q.mode.how().to_string()),
        Phase::Loading | Phase::Playing => match q.mode {
            Mode::Find => ("Listen\u{2026}".to_string(), String::new()),
            Mode::Echo => (String::new(), "Listen\u{2026}".to_string()),
            Mode::Imagine => (String::new(), "First, the key\u{2026}".to_string()),
        },
        Phase::Answering => match q.mode {
            Mode::Find => ("Which note was it?".to_string(), format!("Tap it below, or play it.{heard}")),
            Mode::Echo => (String::new(), format!("Play it back  \u{b7}  note {} of {}{heard}", played.len() + 1, q.notes.len())),
            Mode::Imagine => (String::new(), format!("Hear it in your head, then play it  \u{b7}  note {} of {}{heard}", played.len() + 1, q.notes.len())),
        },
        Phase::Done => {
            let result = p.result.get().unwrap_or_default();
            let right = result.iter().all(|r| *r);
            if right {
                match q.mode {
                    Mode::Find => (p.banner.get(), format!("It was {answer}.")),
                    _ => (String::new(), p.banner.get()),
                }
            } else {
                match q.mode {
                    Mode::Find => (
                        format!("It was {answer}"),
                        format!("You picked {}. Compare to hear both.", played.first().map(|&n| name_of(n, q.key, sargam)).unwrap_or_default()),
                    ),
                    _ => {
                        let mine = played.iter().map(|&n| name_of(n, q.key, sargam)).collect::<Vec<_>>().join(" ");
                        let after = if q.mode == Mode::Echo { "Play again to hear it." } else { "Listen to how it goes." };
                        (
                            String::new(),
                            format!("{} of {} right  \u{b7}  you played {mine}. {after}", result.iter().filter(|r| **r).count(), result.len()),
                        )
                    }
                }
            }
        }
    }
}

/// A box per note of a melody (not for one note: the pads are the
/// answer). Echo: "?" until you play each, then what you played.
/// Imagine: the numbers to imagine. Once answered: the right notes, red
/// where yours were off (the line under says what you played).
fn slots(cx: &mut Context, p: EarProps) {
    let shape = Memo::new(move |_| (p.question.get(), p.played.get(), p.result.get(), p.phase.get(), p.sargam.get()));
    Binding::new(cx, shape, move |cx| {
        let (Some(q), played, result, phase, sargam) = shape.get() else { return };
        HStack::new(cx, move |cx| {
            for (i, &note) in q.notes.iter().enumerate() {
                let ok = result.as_ref().and_then(|r| r.get(i).copied());
                let mine = played.get(i).copied();
                let current = phase == Phase::Answering && i == played.len();
                let text = match (ok, q.mode, mine) {
                    (Some(_), _, _) | (None, Mode::Imagine, _) => name_of(note, q.key, sargam),
                    (None, _, Some(m)) => name_of(m, q.key, sargam),
                    (None, _, None) => "?".to_string(),
                };
                Label::new(cx, text)
                    .class("ear-slot")
                    .alignment(Alignment::Center)
                    .toggle_class("is-right", ok == Some(true))
                    .toggle_class("is-wrong", ok == Some(false))
                    .toggle_class("is-current", current)
                    .toggle_class("is-filled", ok.is_none() && mine.is_some());
            }
        })
        .gap(Pixels(8.0))
        .size(Auto);
    });
}

/// A pad for each degree the level uses: the degree big, its note under
/// it (where it is on the neck is the next thing to learn).
fn pads(cx: &mut Context, p: EarProps) {
    let shape = Memo::new(move |_| (p.level.get(), p.scale_mask.get(), p.key.get(), p.sargam.get()));
    // In a column of its own: a Binding rebuilt straight in a row lays out
    // in the wrong place.
    VStack::new(cx, move |cx| Binding::new(cx, shape, move |cx| {
        let (level, mask, key, sargam) = shape.get();
        HStack::new(cx, move |cx| {
            for degree in ear::pool(level, mask) {
                let marks = Memo::new(move |_| {
                    // Find marks the answer, and a wrong pick.
                    let (Some(q), Some(_)) = (p.question.get(), p.result.get()) else { return (false, false) };
                    if q.mode != Mode::Find {
                        return (false, false);
                    }
                    let target = q.degrees()[0];
                    let picked = p.played.get().first().map(|&n| ear::degree_of(n, q.key));
                    (target == degree, picked == Some(degree) && target != degree)
                });
                Button::new(cx, move |cx| {
                    VStack::new(cx, move |cx| {
                        Label::new(cx, ear::label(degree, sargam)).class("ear-pad-degree").hoverable(false);
                        Label::new(cx, note_name_for_key((key + degree) % 12, key)).class("ear-pad-note").hoverable(false);
                    })
                    .alignment(Alignment::Center)
                    .gap(Pixels(2.0))
                    .hoverable(false)
                })
                .class("btn")
                .class("ear-pad")
                .toggle_class("is-right", marks.map(|m| m.0))
                .toggle_class("is-wrong", marks.map(|m| m.1))
                .on_press(move |cx| cx.emit(EarEvent::Pad(degree)));
            }
        })
        .gap(Pixels(6.0))
        .size(Auto);
    }))
    .size(Auto);
}

/// One main button (Start, then Next), and the hearing aids beside it.
fn actions(cx: &mut Context, p: EarProps) {
    HStack::new(cx, move |cx| {
        Button::new(cx, move |cx| Label::new(cx, p.question.map(|q| if q.is_none() { "\u{25b8} Start" } else { "Next \u{203a}" })))
            .class("btn")
            .class("lg")
            .class("is-on")
            .on_press(|cx| cx.emit(EarEvent::Next));
        Button::new(cx, |cx| Label::new(cx, "Play again"))
            .class("btn")
            .class("quiet")
            .toggle_class("hidden", p.question.map(|q| q.is_none()))
            .on_press(|cx| cx.emit(EarEvent::Again));
        Button::new(cx, |cx| Label::new(cx, "Hear the key"))
            .class("btn")
            .class("quiet")
            .on_press(|cx| cx.emit(EarEvent::Key));
        let wrong = Memo::new(move |_| p.phase.get() == Phase::Done && p.result.get().is_some_and(|r| r.iter().any(|x| !*x)));
        Button::new(cx, move |cx| Label::new(cx, p.mode.map(|m| if *m == Mode::Find { "Compare" } else { "Hear mine" })))
            .class("btn")
            .class("quiet")
            .toggle_class("hidden", wrong.map(|w| !*w))
            .on_press(|cx| cx.emit(EarEvent::Mine));
    })
    .gap(Pixels(tokens::SPACE_2))
    .alignment(Alignment::Center)
    .size(Auto);
}
