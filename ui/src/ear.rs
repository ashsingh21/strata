//! Ear: hear a melody inside, then find it on your instrument. Scale
//! degrees in the song's key - find one note, echo a short melody, or
//! imagine one from its numbers and play it - answered on the pads, a
//! MIDI or computer keyboard, or a guitar (or voice) into the mic. The
//! rules and questions are `shared::ear`; this plays them, listens and
//! keeps score. A Practice tab, like Riyaz.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use vizia::prelude::*;
use vizia::vg;

use shared::ear::{self, Mode, NoteFollower, Question};
use shared::theory::note_name_for_key;

use crate::hidpi::Logical;
use crate::synth::state::SynthEvent;
use crate::tokens::{self, ThemeId};

/// Mic readings per second at most.
const HOP_SECS: f32 = 1.0 / 40.0;
/// After a right answer, the next question comes by itself.
const NEXT_AFTER: Duration = Duration::from_millis(1100);
/// The question's tempo.
const BPM: f64 = 100.0;
/// Clarity a mic reading needs to count as a note.
const CLEAR: f32 = 0.75;
/// The last answers shown as dots.
const HISTORY: usize = 30;

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
    pub history: Signal<Vec<bool>>,
    /// What just happened, big: "Right! +20", "Level up!".
    pub banner: Signal<String>,
    /// The mix-up heard most, if any yet.
    pub mixed: Signal<Option<String>>,
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
    confusions: HashMap<(u8, u8), u32>,
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
            history: Signal::new(Vec::new()),
            banner: Signal::new(String::new()),
            mixed: Signal::new(None),
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
            confusions: HashMap::new(),
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
        self.history.update(|h| {
            h.push(right);
            if h.len() > HISTORY {
                h.remove(0);
            }
        });
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
            for (i, ok) in result.iter().enumerate() {
                if !ok {
                    let got = ear::degree_of(played[i], q.key);
                    *self.confusions.entry((q.degrees()[i], got)).or_default() += 1;
                }
            }
            self.mixed.set(self.most_mixed());
            match q.mode {
                // Yours, then the right one: hear the difference.
                Mode::Find => self.hear(cx, vec![played[0], q.notes[0]]),
                // What it should have been.
                Mode::Imagine => self.hear(cx, q.notes.clone()),
                Mode::Echo => {}
            }
        }
    }

    fn most_mixed(&self) -> Option<String> {
        let sargam = self.sargam.get();
        self.confusions
            .iter()
            .filter(|(_, n)| **n >= 2)
            .max_by_key(|(_, n)| **n)
            .map(|((want, got), n)| format!("Mixed up most: {} heard as {} ({n}\u{d7})", ear::label(*want, sargam), ear::label(*got, sargam)))
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
                if !played.is_empty() {
                    self.hear(cx, played);
                }
            }
            EarEvent::Pad(degree) => {
                let Some(q) = self.question.get() else { return };
                self.input(cx, ear::home(q.key) + degree, true);
            }
            EarEvent::ToggleMic => self.set_mic(!self.mic_on.get()),
            EarEvent::ToggleSargam => {
                self.sargam.set(!self.sargam.get());
                self.mixed.set(self.most_mixed());
            }
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
    pub history: Signal<Vec<bool>>,
    pub banner: Signal<String>,
    pub mixed: Signal<Option<String>>,
    pub mic_on: Signal<bool>,
    pub no_input: Signal<bool>,
    pub hearing: Signal<Option<u8>>,
    pub sargam: Signal<bool>,
    pub key: Signal<u8>,
    pub scale_mask: Signal<u16>,
    pub theme: Signal<ThemeId>,
}

impl EarProps {
    pub fn of(m: &EarModel, theme: Signal<ThemeId>) -> Self {
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
            history: m.history,
            banner: m.banner,
            mixed: m.mixed,
            mic_on: m.mic_on,
            no_input: m.no_input,
            hearing: m.hearing,
            sargam: m.sargam,
            key: m.key,
            scale_mask: m.scale_mask,
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
                status(cx, p);
                Stage::new(cx, p).width(Stretch(1.0)).height(Stretch(1.0));
                pads(cx, p);
            })
            .gap(Pixels(tokens::SPACE_4))
            .width(Stretch(1.0))
            .height(Pixels(176.0));
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
        Button::new(cx, move |cx| Label::new(cx, p.sargam.map(|s| if *s { "Sa Re Ga" } else { "1 2 3" })))
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
            Memo::new(move |_| format!("Score {}   Streak {}   Best {}", p.score.get(), p.streak.get(), p.best.get())),
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

fn status(cx: &mut Context, p: EarProps) {
    VStack::new(cx, move |cx| {
        let big = Memo::new(move |_| {
            let mode = p.mode.get();
            match p.phase.get() {
                Phase::Idle => "Ready".to_string(),
                Phase::Loading | Phase::Playing => "Listen\u{2026}".to_string(),
                Phase::Answering => match mode {
                    Mode::Find => "Which degree?".to_string(),
                    Mode::Echo => "Play it back".to_string(),
                    Mode::Imagine => "Hear it inside".to_string(),
                },
                Phase::Done => p.banner.get(),
            }
        });
        Label::new(cx, big).class("practice-status").text_wrap(true).width(Stretch(1.0));
        let line = Memo::new(move |_| {
            let sargam = p.sargam.get();
            let q = p.question.get();
            let played = p.played.get();
            let hearing = p.hearing.get().filter(|_| p.mic_on.get());
            match (p.phase.get(), q) {
                (Phase::Idle, _) => p.mode.get().how().to_string(),
                (Phase::Loading | Phase::Playing, Some(q)) if q.mode == Mode::Imagine => "First, home.".to_string(),
                (Phase::Loading | Phase::Playing, _) => "Home's chords, then the question.".to_string(),
                (Phase::Answering, Some(q)) => {
                    let heard = hearing.map(|h| format!("  \u{b7}  hearing {}", name_of(h, q.key, sargam))).unwrap_or_default();
                    match q.mode {
                        Mode::Find => format!("Tap it, or play it.{heard}"),
                        Mode::Echo => format!("Note {} of {}.{heard}", played.len() + 1, q.notes.len()),
                        Mode::Imagine => format!("Then play it: note {} of {}.{heard}", played.len() + 1, q.notes.len()),
                    }
                }
                (Phase::Done, Some(q)) => {
                    let right = p.result.get().is_some_and(|r| r.iter().all(|x| *x));
                    let answer = q.notes.iter().map(|&n| name_of(n, q.key, sargam)).collect::<Vec<_>>().join(" ");
                    match (q.mode, right) {
                        (Mode::Find, true) => format!("It was {answer}."),
                        (Mode::Find, false) => format!(
                            "It was {answer}, not {}. Hear the two.",
                            played.first().map(|&n| name_of(n, q.key, sargam)).unwrap_or_default()
                        ),
                        (Mode::Imagine, true) => "That's what you imagined.".to_string(),
                        (Mode::Imagine, false) => format!("It goes {answer}. Listen."),
                        (Mode::Echo, true) => format!("{answer}."),
                        (Mode::Echo, false) => format!("It went {answer}. Again to hear it."),
                    }
                }
                _ => String::new(),
            }
        });
        Label::new(cx, line).class("value").text_wrap(true).width(Stretch(1.0));
        Element::new(cx).height(Stretch(1.0)).width(Pixels(1.0));
        HStack::new(cx, move |cx| {
            Button::new(cx, move |cx| Label::new(cx, p.phase.map(|ph| if *ph == Phase::Idle { "\u{25b8} Start" } else { "\u{25b8} Next" })))
                .class("btn")
                .class("lg")
                .class("is-on")
                .on_press(|cx| cx.emit(EarEvent::Next));
            Button::new(cx, |cx| Label::new(cx, "Again"))
                .class("btn")
                .class("lg")
                .toggle_class("hidden", p.question.map(|q| q.is_none()))
                .on_press(|cx| cx.emit(EarEvent::Again))
                .tooltip(|cx| Tooltip::new(cx, |cx| { Label::new(cx, "Hear it again"); }).arrow(false));
            Button::new(cx, |cx| Label::new(cx, "Home"))
                .class("btn")
                .class("lg")
                .on_press(|cx| cx.emit(EarEvent::Key))
                .tooltip(|cx| Tooltip::new(cx, |cx| { Label::new(cx, "Hear the key again: home's chords"); }).arrow(false));
            Button::new(cx, |cx| Label::new(cx, "Mine"))
                .class("btn")
                .class("lg")
                .toggle_class("hidden", Memo::new(move |_| p.phase.get() != Phase::Done || p.played.get().is_empty()))
                .on_press(|cx| cx.emit(EarEvent::Mine))
                .tooltip(|cx| Tooltip::new(cx, |cx| { Label::new(cx, "Hear what you played"); }).arrow(false));
        })
        .gap(Pixels(tokens::SPACE_2))
        .height(Auto);
    })
    .gap(Pixels(6.0))
    .width(Pixels(250.0))
    .height(Stretch(1.0));
}

/// A pad for each degree the level uses: the degree big, its note under
/// it (where it is on the neck is the next thing to learn).
fn pads(cx: &mut Context, p: EarProps) {
    let shape = Memo::new(move |_| (p.level.get(), p.scale_mask.get(), p.key.get(), p.sargam.get()));
    VStack::new(cx, move |cx| {
        Binding::new(cx, shape, move |cx| {
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
            .wrap(LayoutWrap::Wrap)
            .gap(Pixels(6.0))
            .width(Stretch(1.0))
            .height(Auto);
        });
    })
    .width(Pixels(4.0 * 58.0 + 3.0 * 6.0))
    .height(Stretch(1.0));
}

/// The question's notes as slots (Imagine shows their numbers; the
/// others reveal them once answered), what you played under each, and
/// your last answers as dots.
struct Stage {
    p: EarProps,
}

impl Stage {
    fn new(cx: &mut Context, p: EarProps) -> Handle<'_, Self> {
        Self { p }
            .build(cx, |_| {})
            .bind(p.question, |mut h| h.needs_redraw())
            .bind(p.played, |mut h| h.needs_redraw())
            .bind(p.result, |mut h| h.needs_redraw())
            .bind(p.phase, |mut h| h.needs_redraw())
            .bind(p.history, |mut h| h.needs_redraw())
            .bind(p.mixed, |mut h| h.needs_redraw())
            .bind(p.sargam, |mut h| h.needs_redraw())
            .bind(p.hearing, |mut h| h.needs_redraw())
            .bind(p.theme, |mut h| h.needs_redraw())
    }
}

const SLOT_W: f32 = 64.0;
const SLOT_H: f32 = 56.0;

impl View for Stage {
    fn element(&self) -> Option<&'static str> {
        Some("ear-stage")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let b = cx.lbounds();
        crate::hidpi::clip(canvas, b);
        let pal = self.p.theme.get().palette();
        rrect(canvas, vg::Rect::new(b.x, b.y, b.x + b.w, b.y + b.h), 6.0, pal.bg_000);
        let sargam = self.p.sargam.get();
        let phase = self.p.phase.get();

        // Your last answers, and the mix-up heard most.
        let dots_y = b.y + b.h - 14.0;
        for (i, right) in self.p.history.get().iter().enumerate() {
            dot(canvas, b.x + 16.0 + i as f32 * 12.0, dots_y, 4.0, if *right { pal.signal } else { pal.record });
        }
        if let Some(mixed) = self.p.mixed.get() {
            text(canvas, &mixed, b.x + 12.0, dots_y - 14.0, 11.0, pal.ink_muted);
        }

        let Some(q) = self.p.question.get() else {
            text(canvas, "Start, then answer on the pads, a keyboard or your guitar.", b.x + 16.0, b.y + 40.0, 12.0, pal.ink_muted);
            return;
        };
        let played = self.p.played.get();
        let result = self.p.result.get();
        let n = q.notes.len();
        let gap = 10.0;
        let total = n as f32 * SLOT_W + (n.saturating_sub(1)) as f32 * gap;
        let x0 = b.x + ((b.w - total) / 2.0).max(12.0);
        let top = b.y + 18.0;
        for (i, &note) in q.notes.iter().enumerate() {
            let x = x0 + i as f32 * (SLOT_W + gap);
            let rect = vg::Rect::new(x, top, x + SLOT_W, top + SLOT_H);
            let ok = result.as_ref().and_then(|r| r.get(i).copied());
            let bg = match ok {
                Some(true) => pal.signal_soft,
                Some(false) => pal.bg_300,
                None => pal.bg_200,
            };
            rrect(canvas, rect, 6.0, bg);
            // The note you're on.
            if phase == Phase::Answering && i == played.len() {
                stroke(canvas, rect, pal.ink);
            }
            let shown = q.mode == Mode::Imagine || result.is_some();
            let label = if shown { name_of(note, q.key, sargam) } else { "?".to_string() };
            let color = if shown { pal.ink } else { pal.ink_faint };
            centered(canvas, &label, rect.center_x(), rect.center_y() + 8.0, 24.0, color);
            // What you played for it.
            if let Some(&got) = played.get(i) {
                let color = match ok {
                    Some(false) => pal.record,
                    _ => pal.ink_muted,
                };
                centered(canvas, &name_of(got, q.key, sargam), rect.center_x(), rect.bottom + 18.0, 13.0, color);
            }
        }
    }
}

fn rrect(canvas: &Canvas, rect: vg::Rect, r: f32, color: Color) {
    let mut paint = vg::Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(color);
    canvas.draw_rrect(vg::RRect::new_rect_xy(rect, r, r), &paint);
}

fn stroke(canvas: &Canvas, rect: vg::Rect, color: Color) {
    let mut paint = vg::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(vg::PaintStyle::Stroke);
    paint.set_stroke_width(2.0);
    paint.set_color(color);
    canvas.draw_rrect(vg::RRect::new_rect_xy(rect, 6.0, 6.0), &paint);
}

fn dot(canvas: &Canvas, x: f32, y: f32, r: f32, color: Color) {
    let mut paint = vg::Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(color);
    canvas.draw_circle(vg::Point::new(x, y), r, &paint);
}

fn text(canvas: &Canvas, s: &str, x: f32, y: f32, size: f32, color: Color) {
    // The canvas font has no flat or sharp sign.
    let s = &s.replace('\u{266d}', "b").replace('\u{266f}', "#");
    let font = crate::canvas_text::canvas_font(size);
    let mut paint = vg::Paint::default();
    paint.set_color(color);
    paint.set_anti_alias(true);
    canvas.draw_str(s, vg::Point::new(x, y), &font, &paint);
}

fn centered(canvas: &Canvas, s: &str, cx: f32, y: f32, size: f32, color: Color) {
    let font = crate::canvas_text::canvas_font(size);
    let (w, _) = font.measure_str(s.replace('\u{266d}', "b").replace('\u{266f}', "#"), None);
    text(canvas, s, cx - w / 2.0, y, size, color);
}
