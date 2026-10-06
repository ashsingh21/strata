//! Riyaz: sing against the tanpura and see your voice against Sa and the
//! raag's swars - the swar you're on and how far off, a trace of the last
//! few seconds, and the notes you held and how steadily. Docked under the
//! devices like Theory, opened from the rail.

use std::sync::Arc;
use std::time::Instant;

use vizia::prelude::*;
use vizia::vg;

use shared::recorder::RecordParams;
use shared::riyaz::{self, Held, Point, Tracker};
use shared::theory::sargam_name;

use crate::hidpi::Logical;
use crate::tokens::{self, ThemeId};

/// Analyses per second at most.
const HOP_SECS: f32 = 1.0 / 40.0;
/// The trace's range against Sa, in cents: Pa below to Re above.
const LOW_CENTS: f32 = -500.0;
const HIGH_CENTS: f32 = 1400.0;

pub enum RiyazEvent {
    ToggleOpen,
    ToggleListen,
    ToggleTanpura,
    SetOctave(i32),
    Clear,
    /// Once a frame, from the render timer: read the mic, follow the voice.
    Tick,
    TanpuraRendered { generation: u64, audio: Arc<[f32]> },
}

pub struct RiyazModel {
    pub open: Signal<bool>,
    pub listening: Signal<bool>,
    pub tanpura: Signal<bool>,
    /// Sa's octave (3: Sa near C3, a low voice; 4 for a high one).
    pub octave: Signal<i32>,
    pub trace: Signal<Vec<Point>>,
    pub held: Signal<Vec<Held>>,
    /// The swar being sung: (swar, cents off, held for).
    pub now: Signal<Option<(i32, f32, f32)>>,
    /// The mic isn't there (or not open).
    pub no_input: Signal<bool>,
    key: Signal<u8>,
    scale_mask: Signal<u16>,
    voice_rx: rtrb::Consumer<f32>,
    record_params: Arc<RecordParams>,
    tracker: Tracker,
    listener: riyaz::Listener,
    started: Instant,
    last_analysis: f32,
    player: crate::preview_player::SharedPlayer,
    sample_rate: u32,
    tanpura_generation: u64,
    tanpura_token: Option<u64>,
}

impl RiyazModel {
    pub fn new(
        key: Signal<u8>,
        scale_mask: Signal<u16>,
        voice_rx: rtrb::Consumer<f32>,
        record_params: Arc<RecordParams>,
        player: crate::preview_player::SharedPlayer,
        sample_rate: u32,
    ) -> Self {
        let open = Signal::new(false);
        Self {
            open,
            listening: Signal::new(false),
            tanpura: Signal::new(false),
            octave: Signal::new(3),
            trace: Signal::new(Vec::new()),
            held: Signal::new(Vec::new()),
            now: Signal::new(None),
            no_input: Signal::new(false),
            key,
            scale_mask,
            voice_rx,
            record_params,
            tracker: Tracker::new(),
            listener: riyaz::Listener::new(48_000),
            started: Instant::now(),
            last_analysis: 0.0,
            player,
            sample_rate,
            tanpura_generation: 0,
            tanpura_token: None,
        }
    }

    fn set_listening(&mut self, on: bool) {
        let on = on && self.record_params.input_rate() > 0;
        self.no_input.set(self.record_params.input_rate() == 0);
        self.record_params.set_listening(on);
        while self.voice_rx.pop().is_ok() {}
        self.listener = riyaz::Listener::new(self.record_params.input_rate().max(1));
        if self.listening.get() != on {
            self.listening.set(on);
        }
    }

    fn stop_tanpura(&mut self) {
        self.tanpura_generation += 1;
        if let Some(token) = self.tanpura_token.take() {
            self.player.borrow_mut().stop_if(token);
        }
    }

    /// Renders a few bars of the tanpura in the song's key and loops it.
    fn start_tanpura(&mut self, cx: &mut EventContext) {
        self.stop_tanpura();
        let key = self.key.get();
        let mut project = shared::lessons::starting_project(shared::lessons::THEORY_RAAG);
        project.migrate();
        // The lesson's drone is on C: move it to this Sa, staying near it.
        let shift = if key <= 6 { key as i32 } else { key as i32 - 12 };
        for clip in &mut project.arrangement.clips {
            if let shared::arrangement::ClipContent::Midi { notes, .. } = &mut clip.content {
                for note in notes.iter_mut() {
                    note.pitch = (note.pitch as i32 + shift + 12 * (self.octave.get() - 3)).clamp(0, 127) as u8;
                }
            }
        }
        let bar = shared::lessons::BAR;
        self.tanpura_generation += 1;
        let generation = self.tanpura_generation;
        let sample_rate = self.sample_rate;
        cx.spawn(move |proxy| {
            let patches = project.instruments.into_iter().collect();
            let job = engine::render::RenderJob { arrangement: project.arrangement, patches, sources: Default::default(), sample_rate };
            let audio = engine::render::render_between(&job, 0, 4 * bar, 0.0);
            let _ = proxy.emit(RiyazEvent::TanpuraRendered { generation, audio: Arc::from(audio) });
        });
    }

    /// Reads what the mic sent since last frame and follows the voice.
    fn listen(&mut self) {
        let rx = &mut self.voice_rx;
        self.listener.feed(std::iter::from_fn(|| rx.pop().ok()));
        let t = self.started.elapsed().as_secs_f32();
        if t - self.last_analysis < HOP_SECS {
            return;
        }
        let Some(pitch) = self.listener.read() else { return };
        self.last_analysis = t;
        let sa = riyaz::sa_hz(self.key.get(), self.octave.get());
        self.tracker.push(t, pitch.map(|p| riyaz::cents(p.hz, sa)), self.scale_mask.get());
        self.trace.set(self.tracker.trace.iter().copied().collect());
        if self.held.get() != self.tracker.held {
            self.held.set(self.tracker.held.clone());
        }
        let now = self.tracker.now(t);
        if self.now.get() != now {
            self.now.set(now);
        }
    }
}

impl Model for RiyazModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            RiyazEvent::ToggleOpen => {
                let open = !self.open.get();
                self.open.set(open);
                // Open: listening straight away. Closed: nothing left running.
                self.set_listening(open);
                if !open {
                    self.stop_tanpura();
                    self.tanpura.set(false);
                }
            }
            RiyazEvent::ToggleListen => self.set_listening(!self.listening.get()),
            RiyazEvent::ToggleTanpura => {
                let on = !self.tanpura.get();
                self.tanpura.set(on);
                if on {
                    self.start_tanpura(cx);
                } else {
                    self.stop_tanpura();
                }
            }
            RiyazEvent::SetOctave(octave) => {
                self.octave.set(*octave);
                if self.tanpura.get() {
                    self.start_tanpura(cx);
                }
            }
            RiyazEvent::Clear => {
                self.tracker.clear();
                self.trace.set(Vec::new());
                self.held.set(Vec::new());
                self.now.set(None);
            }
            RiyazEvent::Tick => {
                if self.listening.get() {
                    self.listen();
                }
                // Something else took the player (a preview): the switch follows.
                if self.tanpura.get() && self.tanpura_token.is_some() && self.player.borrow().current() != self.tanpura_token {
                    self.tanpura_token = None;
                    self.tanpura.set(false);
                }
            }
            RiyazEvent::TanpuraRendered { generation, audio } => {
                if *generation == self.tanpura_generation && self.tanpura.get() {
                    self.tanpura_token = self.player.borrow_mut().play(audio.to_vec(), true);
                }
            }
        });
    }
}

#[derive(Clone, Copy)]
pub struct RiyazProps {
    pub open: Signal<bool>,
    pub listening: Signal<bool>,
    pub tanpura: Signal<bool>,
    pub octave: Signal<i32>,
    pub trace: Signal<Vec<Point>>,
    pub held: Signal<Vec<Held>>,
    pub now: Signal<Option<(i32, f32, f32)>>,
    pub no_input: Signal<bool>,
    pub key: Signal<u8>,
    pub scale_mask: Signal<u16>,
    pub theme: Signal<ThemeId>,
}

impl RiyazProps {
    pub fn of(m: &RiyazModel, theme: Signal<ThemeId>) -> Self {
        Self {
            open: m.open,
            listening: m.listening,
            tanpura: m.tanpura,
            octave: m.octave,
            trace: m.trace,
            held: m.held,
            now: m.now,
            no_input: m.no_input,
            key: m.key,
            scale_mask: m.scale_mask,
            theme,
        }
    }
}

/// A swar's name with its octave: "Sa", "ni" below (with a dot under in
/// notation; here a low mark), "Sa'" above.
pub fn swar_name(swar: i32) -> String {
    let name = sargam_name(swar.rem_euclid(12) as u8);
    match swar.div_euclid(12) {
        0 => name.to_string(),
        n if n < 0 => format!("{name}\u{2080}"),
        _ => format!("{name}\u{2019}"),
    }
}

fn signed_cents(c: f32) -> String {
    let rounded = c.round() as i32;
    if rounded > 0 {
        format!("+{rounded}\u{a2}")
    } else if rounded < 0 {
        format!("\u{2212}{}\u{a2}", -rounded)
    } else {
        "0\u{a2}".to_string()
    }
}

pub fn riyaz_view(cx: &mut Context, p: RiyazProps) {
    Binding::new(cx, p.open, move |cx| {
        if !p.open.get() {
            return;
        }
        VStack::new(cx, move |cx| {
            header(cx, p);
            HStack::new(cx, move |cx| {
                readout(cx, p);
                Trace::new(cx, p).width(Stretch(1.0)).height(Stretch(1.0));
                held_list(cx, p);
            })
            .gap(Pixels(tokens::SPACE_3))
            .width(Stretch(1.0))
            .height(Pixels(220.0));
        })
        .class("device")
        .gap(Pixels(tokens::SPACE_3))
        .padding(Pixels(tokens::SPACE_3))
        .width(Stretch(1.0))
        .height(Auto);
    });
}

fn header(cx: &mut Context, p: RiyazProps) {
    HStack::new(cx, move |cx| {
        Label::new(cx, "Riyaz").class("heading");
        let key_text = Memo::new(move |_| {
            format!(
                "Sa = {} \u{b7} {}  \u{2304}",
                shared::theory::note_name(p.key.get()),
                crate::interval_input::state::scale_name(p.scale_mask.get()).to_lowercase()
            )
        });
        Button::new(cx, move |cx| Label::new(cx, key_text)).class("btn").class("sm").on_press(crate::key_menu::open_from);
        Label::new(cx, "Sa's octave").class("label");
        crate::synth::segmented::segmented(
            cx,
            3,
            move |cx, i| Label::new(cx, p.key.map(move |k| format!("{}{}", shared::theory::note_name(*k), i + 2))),
            move |i| p.octave.map(move |o| *o == i as i32 + 2),
            |cx, i| cx.emit(RiyazEvent::SetOctave(i as i32 + 2)),
        )
        .height(Pixels(tokens::SIZE_CONTROL));
        Button::new(cx, |cx| Label::new(cx, "Tanpura"))
            .class("btn")
            .toggle_class("is-on", p.tanpura)
            .tooltip(|cx| Tooltip::new(cx, |cx| { Label::new(cx, "The drone on Sa and Pa, in the song's key"); }).arrow(false))
            .on_press(|cx| cx.emit(RiyazEvent::ToggleTanpura));
        Button::new(cx, move |cx| Label::new(cx, p.listening.map(|l| if *l { "\u{25cf} Listening" } else { "Listen" })))
            .class("btn")
            .toggle_class("is-rec", p.listening)
            .on_press(|cx| cx.emit(RiyazEvent::ToggleListen));
        Label::new(cx, "No microphone found: pick an input in the header")
            .class("value")
            .class("empty-note")
            .toggle_class("hidden", p.no_input.map(|n| !*n));
        Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
        Button::new(cx, |cx| Label::new(cx, "Clear")).class("btn").class("quiet").on_press(|cx| cx.emit(RiyazEvent::Clear));
        Button::new(cx, |cx| Label::new(cx, "Close")).class("btn").class("quiet").on_press(|cx| cx.emit(RiyazEvent::ToggleOpen));
    })
    .gap(Pixels(tokens::SPACE_2))
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Pixels(tokens::SIZE_CONTROL));
}

/// The swar being sung, big, and how far off.
fn readout(cx: &mut Context, p: RiyazProps) {
    VStack::new(cx, move |cx| {
        Label::new(cx, "Now").class("label");
        Label::new(cx, p.now.map(|n| n.map(|(s, _, _)| swar_name(s)).unwrap_or_else(|| "\u{2014}".to_string()))).class("riyaz-swar");
        Label::new(cx, p.now.map(|n| n.map(|(_, off, _)| signed_cents(off)).unwrap_or_default()))
            .class("title")
            .toggle_class("lesson-done", p.now.map(|n| n.is_some_and(|(_, off, _)| off.abs() < Held::STEADY)));
        Label::new(cx, p.now.map(|n| match n {
            Some((_, off, _)) if off.abs() < Held::STEADY => "in tune".to_string(),
            Some((_, off, _)) if *off > 0.0 => "a little sharp".to_string(),
            Some(_) => "a little flat".to_string(),
            None => "sing a note".to_string(),
        }))
        .class("value");
    })
    .gap(Pixels(4.0))
    .width(Pixels(140.0))
    .height(Stretch(1.0));
}

/// The notes held for a moment or more: swar, where it sat, how steady.
fn held_list(cx: &mut Context, p: RiyazProps) {
    VStack::new(cx, move |cx| {
        Label::new(cx, "Held notes").class("label");
        Binding::new(cx, p.held, move |cx| {
            let held = p.held.get();
            if held.is_empty() {
                Label::new(cx, "Hold a note for a moment and it shows here.").class("value").text_wrap(true).width(Stretch(1.0));
            }
            for h in held.iter().rev() {
                let steady = h.spread < Held::STEADY;
                HStack::new(cx, move |cx| {
                    Label::new(cx, swar_name(h.swar)).class("title").width(Pixels(44.0));
                    Label::new(cx, signed_cents(h.mean)).class("value").toggle_class("lesson-done", h.mean.abs() < Held::STEADY).width(Pixels(44.0));
                    Label::new(cx, if steady { "steady" } else { "wavering" }).class("value").width(Stretch(1.0));
                    Label::new(cx, format!("{:.1} s", h.secs)).class("value");
                })
                .gap(Pixels(4.0))
                .alignment(Alignment::Left)
                .width(Stretch(1.0))
                .height(Pixels(20.0));
            }
        });
    })
    .gap(Pixels(4.0))
    .width(Pixels(230.0))
    .height(Stretch(1.0));
}

/// The last few seconds of the voice against the raag's swars.
struct Trace {
    p: RiyazProps,
}

impl Trace {
    fn new(cx: &mut Context, p: RiyazProps) -> Handle<'_, Self> {
        Self { p }
            .build(cx, |_| {})
            .bind(p.trace, |mut h| h.needs_redraw())
            .bind(p.scale_mask, |mut h| h.needs_redraw())
            .bind(p.theme, |mut h| h.needs_redraw())
    }
}

impl View for Trace {
    fn element(&self) -> Option<&'static str> {
        Some("riyaz-trace")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let b = cx.lbounds();
        crate::hidpi::clip(canvas, b);
        let pal = self.p.theme.get().palette();
        let mask = self.p.scale_mask.get();
        fill(canvas, vg::Rect::new(b.x, b.y, b.x + b.w, b.y + b.h), pal.bg_000);
        let label_w = 40.0;
        // A little room above the top line and below the bottom one, for their labels.
        let (top, height) = (b.y + 10.0, b.h - 20.0);
        let y_of = |cents: f32| top + (HIGH_CENTS - cents) / (HIGH_CENTS - LOW_CENTS) * height;

        // The raag's swars across the range; Sa and Pa a little stronger.
        let first = (LOW_CENTS / 100.0).ceil() as i32;
        let last = (HIGH_CENTS / 100.0).floor() as i32;
        for swar in first..=last {
            if mask & (1 << swar.rem_euclid(12)) == 0 {
                continue;
            }
            let y = y_of(swar as f32 * 100.0).round();
            let strong = matches!(swar.rem_euclid(12), 0 | 7);
            fill(canvas, vg::Rect::new(b.x + label_w, y, b.x + b.w, y + 1.0), if strong { pal.line_control } else { pal.line });
            text(canvas, &swar_name(swar), b.x + 6.0, y + 4.0, 11.0, if strong { pal.ink } else { pal.ink_muted });
        }

        // The voice: a line where there was a note, gaps for silence.
        let trace = self.p.trace.get();
        let Some(end) = trace.last().map(|p| p.t).filter(|_| trace.iter().any(|p| p.cents.is_some())) else {
            text(canvas, "Sing - your voice draws here against the swars.", b.x + label_w + 12.0, b.y + b.h / 2.0, 12.0, pal.ink_muted);
            return;
        };
        let x_of = |t: f32| b.x + label_w + (1.0 - (end - t) / riyaz::TRACE_SECS) * (b.w - label_w);
        let mut paint = vg::Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(vg::PaintStyle::Stroke);
        paint.set_stroke_width(2.5);
        paint.set_stroke_cap(vg::PaintCap::Round);
        paint.set_color(pal.signal);
        let mut path = vg::PathBuilder::new();
        let mut drawing = false;
        for point in &trace {
            match point.cents.filter(|c| (LOW_CENTS..=HIGH_CENTS).contains(c)) {
                Some(c) => {
                    let pt = vg::Point::new(x_of(point.t), y_of(c));
                    if drawing {
                        path.line_to(pt);
                    } else {
                        path.move_to(pt);
                        drawing = true;
                    }
                }
                None => drawing = false,
            }
        }
        canvas.draw_path(&path.detach(), &paint);
    }
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
    fn swars_are_named_with_their_octave() {
        assert_eq!(swar_name(0), "Sa");
        assert_eq!(swar_name(7), "Pa");
        assert_eq!(swar_name(-1), "Ni\u{2080}");
        assert_eq!(swar_name(12), "Sa\u{2019}");
        assert_eq!(signed_cents(-12.4), "\u{2212}12\u{a2}");
    }
}
