//! Voice leading: build a progression from the key's chords and see how to
//! move between them with as little motion as possible - as whole guitar
//! chords chosen so fingers stay down where they can, as close shapes on
//! the top four strings drawn like tab (each string one voice), or as
//! voices drawn as lines through the chords the way a pianist would. Hold a chord to hear
//! it on the selected track; Play hears them all in turn. A page of its
//! own over the timeline, opened from the rail.

use std::sync::Arc;

use vizia::prelude::*;
use vizia::vg;

use shared::theory::guitar::{self, Shape};
use shared::theory::voicing::{self, Chord};
use shared::theory::note_name_for_key;

use crate::hidpi::Logical;
use crate::synth::state::SynthEvent;
use crate::tokens::{self, ThemeId};

/// Whole guitar chords, as chord charts.
pub const FULL_CHORDS: u8 = 0;
/// Close shapes on the top four strings, drawn like tab: each string one
/// voice.
pub const FOUR_STRINGS: u8 = 1;
/// Voices as lines through the chords, the way a pianist would think.
pub const PIANO_LINES: u8 = 2;

/// How long each chord sounds in Play, in 16ths (half a bar).
const CHORD_16THS: i64 = 8;

pub enum VoicingEvent {
    ToggleOpen,
    Add(usize),
    RemoveLast,
    Clear,
    SetSevenths(bool),
    SetVoices(usize),
    /// FULL_CHORDS, FOUR_STRINGS or PIANO_LINES.
    SetView(u8),
    /// Hear the whole progression (again: stop).
    Play,
    Rendered { generation: u64, audio: Arc<[f32]> },
}

pub struct VoicingModel {
    pub open: Signal<bool>,
    /// Scale degrees (0 = the first note of the key's scale), in order.
    pub progression: Signal<Vec<usize>>,
    pub sevenths: Signal<bool>,
    pub voices: Signal<usize>,
    pub view: Signal<u8>,
    pub playing: Signal<bool>,
    key: Signal<u8>,
    scale_mask: Signal<u16>,
    player: crate::preview_player::SharedPlayer,
    sample_rate: u32,
    generation: u64,
    token: Option<u64>,
}

impl VoicingModel {
    pub fn new(key: Signal<u8>, scale_mask: Signal<u16>, player: crate::preview_player::SharedPlayer, sample_rate: u32) -> Self {
        let open = Signal::new(false);
        Self {
            open,
            progression: Signal::new(Vec::new()),
            sevenths: Signal::new(false),
            voices: Signal::new(4),
            view: Signal::new(FOUR_STRINGS),
            playing: Signal::new(false),
            key,
            scale_mask,
            player,
            sample_rate,
            generation: 0,
            token: None,
        }
    }

    fn stop(&mut self) {
        if let Some(token) = self.token.take() {
            self.player.borrow_mut().stop_if(token);
        }
        self.playing.set(false);
    }
}

impl Model for VoicingModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            VoicingEvent::ToggleOpen => self.open.set(!self.open.get()),
            VoicingEvent::Add(degree) => self.progression.update(|p| p.push(*degree)),
            VoicingEvent::RemoveLast => self.progression.update(|p| {
                p.pop();
            }),
            VoicingEvent::Clear => self.progression.set(Vec::new()),
            VoicingEvent::SetSevenths(on) => self.sevenths.set(*on),
            VoicingEvent::SetVoices(n) => self.voices.set(*n),
            VoicingEvent::SetView(v) => self.view.set(*v),
            VoicingEvent::Play => {
                if self.playing.get() {
                    self.stop();
                    return;
                }
                let chords: Vec<Vec<u8>> = if self.view.get() != PIANO_LINES {
                    on_guitar(&self.progression.get(), self.key.get(), self.scale_mask.get(), self.sevenths.get(), self.view.get() == FOUR_STRINGS)
                        .into_iter()
                        .map(|(_, _, shape)| shape.map(|s| s.notes()).unwrap_or_default())
                        .collect()
                } else {
                    voiced(&self.progression.get(), self.key.get(), self.scale_mask.get(), self.sevenths.get(), self.voices.get())
                        .into_iter()
                        .map(|(_, notes)| notes)
                        .collect()
                };
                if chords.is_empty() {
                    return;
                }
                let notes: Vec<(i64, u8, i64)> = chords
                    .iter()
                    .enumerate()
                    .flat_map(|(i, notes)| notes.iter().map(move |&n| (i as i64 * CHORD_16THS, n, CHORD_16THS - 1)))
                    .collect();
                let take = crate::lessons::preview::quiz_take(&notes);
                self.generation += 1;
                let generation = self.generation;
                let sample_rate = self.sample_rate;
                self.playing.set(true);
                cx.spawn(move |proxy| {
                    let job = engine::render::RenderJob {
                        arrangement: take.arrangement,
                        patches: take.patches,
                        sources: Default::default(),
                        sample_rate,
                    };
                    let audio = engine::render::render_between(&job, take.from, take.to, crate::lessons::preview::TAIL_SECONDS);
                    let _ = proxy.emit(VoicingEvent::Rendered { generation, audio: Arc::from(audio) });
                });
            }
            VoicingEvent::Rendered { generation, audio } => {
                if *generation != self.generation || !self.playing.get() {
                    return;
                }
                self.token = self.player.borrow_mut().play(audio.to_vec(), false);
                if self.token.is_none() {
                    self.playing.set(false);
                }
            }
        });
        // Finished on its own, or replaced by another preview.
        if self.playing.get() && self.token.is_some() && self.player.borrow().current() != self.token {
            self.token = None;
            self.playing.set(false);
        }
    }
}

/// Each chord of the progression with its voicing.
fn voiced(progression: &[usize], key: u8, mask: u16, sevenths: bool, voices: usize) -> Vec<(Chord, Vec<u8>)> {
    let chords: Vec<Chord> = progression.iter().filter_map(|&d| voicing::diatonic(d, key, voicing::seven_notes(mask), sevenths)).collect();
    let voicings = voicing::smooth(&chords, voices);
    chords.into_iter().zip(voicings).collect()
}

/// Each chord of the progression - its scale degree, the chord, and a
/// guitar shape for it - a whole chord, or (`small`) one on the top four
/// strings - the shapes chosen together to keep the hand still.
pub fn on_guitar(progression: &[usize], key: u8, mask: u16, sevenths: bool, small: bool) -> Vec<(usize, Chord, Option<Shape>)> {
    let chords: Vec<(usize, Chord)> = progression.iter().filter_map(|&d| voicing::diatonic(d, key, voicing::seven_notes(mask), sevenths).map(|c| (d, c))).collect();
    let plain: Vec<Chord> = chords.iter().map(|(_, c)| c.clone()).collect();
    let shapes = if small { guitar::lead_small(&plain) } else { guitar::lead(&plain) };
    chords.into_iter().zip(shapes).map(|((d, c), s)| (d, c, s)).collect()
}

#[derive(Clone, Copy)]
pub struct VoicingProps {
    pub open: Signal<bool>,
    pub progression: Signal<Vec<usize>>,
    pub sevenths: Signal<bool>,
    pub voices: Signal<usize>,
    pub view: Signal<u8>,
    pub playing: Signal<bool>,
    pub key: Signal<u8>,
    pub scale_mask: Signal<u16>,
    pub theme: Signal<ThemeId>,
}

impl VoicingProps {
    pub fn of(m: &VoicingModel, theme: Signal<ThemeId>) -> Self {
        Self {
            open: m.open,
            progression: m.progression,
            sevenths: m.sevenths,
            voices: m.voices,
            view: m.view,
            playing: m.playing,
            key: m.key,
            scale_mask: m.scale_mask,
            theme,
        }
    }
}

/// The page, over the timeline, built only while open.
pub fn voicing_view(cx: &mut Context, p: VoicingProps) {
    Binding::new(cx, p.open, move |cx| {
        if !p.open.get() {
            return;
        }
        VStack::new(cx, move |cx| {
            header(cx, p);
            chord_buttons(cx, p);
            VStack::new(cx, move |cx| {
                Binding::new(cx, p.view, move |cx| match p.view.get() {
                    FULL_CHORDS => {
                        crate::chord_charts::ChordCharts::new(cx, p).width(Stretch(1.0)).height(Stretch(1.0));
                    }
                    FOUR_STRINGS => {
                        crate::chord_charts::StringsTab::new(cx, p).width(Stretch(1.0)).height(Stretch(1.0));
                    }
                    _ => {
                        Chart::new(cx, p).width(Stretch(1.0)).height(Stretch(1.0));
                    }
                });
            })
            .width(Stretch(1.0))
            .height(Stretch(1.0));
        })
        .class("map-view")
        .position_type(PositionType::Absolute)
        .z_index(40)
        .gap(Pixels(tokens::SPACE_3))
        .padding(Pixels(tokens::SPACE_4))
        .width(Stretch(1.0))
        .height(Stretch(1.0));
    });
}

fn header(cx: &mut Context, p: VoicingProps) {
    HStack::new(cx, move |cx| {
        Label::new(cx, "Voice leading").class("heading");
        let key_text = Memo::new(move |_| {
            format!(
                "{} {}  \u{2304}",
                shared::theory::note_name(p.key.get()),
                crate::interval_input::state::scale_name(p.scale_mask.get()).to_lowercase()
            )
        });
        Button::new(cx, move |cx| Label::new(cx, key_text)).class("btn").class("sm").on_press(crate::key_menu::open_from);
        crate::synth::segmented::segmented(
            cx,
            2,
            |cx, i| Label::new(cx, if i == 0 { "Triads" } else { "7ths" }),
            move |i| p.sevenths.map(move |s| *s == (i == 1)),
            |cx, i| cx.emit(VoicingEvent::SetSevenths(i == 1)),
        )
        .height(Pixels(tokens::SIZE_CONTROL));
        crate::synth::segmented::segmented(
            cx,
            3,
            |cx, i| Label::new(cx, ["Full chords", "4 strings", "Piano lines"][i]),
            move |i| p.view.map(move |v| *v as usize == i),
            |cx, i| cx.emit(VoicingEvent::SetView(i as u8)),
        )
        .height(Pixels(tokens::SIZE_CONTROL));
        // How many voices only matters when they're drawn as lines.
        HStack::new(cx, move |cx| {
            Binding::new(cx, p.view, move |cx| {
                if p.view.get() != PIANO_LINES {
                    return;
                }
                crate::synth::segmented::segmented(
                    cx,
                    2,
                    |cx, i| Label::new(cx, if i == 0 { "3 voices" } else { "4 voices" }),
                    move |i| p.voices.map(move |v| *v == i + 3),
                    |cx, i| cx.emit(VoicingEvent::SetVoices(i + 3)),
                )
                .height(Pixels(tokens::SIZE_CONTROL));
            });
        })
        .width(Auto)
        .height(Pixels(tokens::SIZE_CONTROL));
        Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
        Button::new(cx, move |cx| Label::new(cx, p.playing.map(|on| if *on { "\u{25a0} Stop" } else { "\u{25b8} Play" })))
            .class("btn")
            .class("is-on")
            .on_press(|cx| cx.emit(VoicingEvent::Play));
        Button::new(cx, |cx| Label::new(cx, "Remove last")).class("btn").class("quiet").on_press(|cx| cx.emit(VoicingEvent::RemoveLast));
        Button::new(cx, |cx| Label::new(cx, "Clear")).class("btn").class("quiet").on_press(|cx| cx.emit(VoicingEvent::Clear));
        Button::new(cx, |cx| Label::new(cx, "Back to the timeline")).class("btn").on_press(|cx| cx.emit(VoicingEvent::ToggleOpen));
    })
    .gap(Pixels(tokens::SPACE_2))
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Pixels(tokens::SIZE_CONTROL));
}

/// The key's seven chords, numeral over name; a click adds one.
fn chord_buttons(cx: &mut Context, p: VoicingProps) {
    let chords = Memo::new(move |_| {
        (0..7)
            .filter_map(|d| voicing::diatonic(d, p.key.get(), voicing::seven_notes(p.scale_mask.get()), p.sevenths.get()).map(|c| (d, c)))
            .map(|(d, c)| (d, voicing::numeral(d, &c), voicing::name(&c, p.key.get())))
            .collect::<Vec<_>>()
    });
    Binding::new(cx, chords, move |cx| {
        let chords = chords.get();
        HStack::new(cx, move |cx| {
            if chords.is_empty() {
                Label::new(cx, "Voice leading builds chords from a 7-note scale: pick major, minor or a 7-note raag in the key menu.")
                    .class("body")
                    .class("empty-note");
                return;
            }
            Label::new(cx, "Add").class("label");
            // A five-note scale borrows the chords of the key it comes from.
            let mask = p.scale_mask.get();
            if voicing::seven_notes(mask) != mask {
                let from = if voicing::seven_notes(mask) == voicing::MAJOR { "major" } else { "minor" };
                Label::new(cx, format!("(chords of {} {from})", shared::theory::note_name(p.key.get())))
                    .class("value");
            }
            for (degree, numeral, name) in chords {
                Button::new(cx, move |cx| {
                    VStack::new(cx, move |cx| {
                        Label::new(cx, numeral.clone()).class("value").hoverable(false);
                        Label::new(cx, name.clone()).class("title").hoverable(false);
                    })
                    .alignment(Alignment::Center)
                    .size(Auto)
                    .hoverable(false)
                })
                .class("btn")
                .width(Pixels(72.0))
                .height(Pixels(48.0))
                .on_press(move |cx| cx.emit(VoicingEvent::Add(degree)));
            }
        })
        .gap(Pixels(tokens::SPACE_2))
        .alignment(Alignment::Left)
        .width(Stretch(1.0))
        .height(Pixels(48.0));
    });
}

/// The progression voiced: a column per chord, a line per voice. Press a
/// column to hear that chord.
struct Chart {
    p: VoicingProps,
    held: Vec<u8>,
}

impl Chart {
    fn new(cx: &mut Context, p: VoicingProps) -> Handle<'_, Self> {
        Self { p, held: Vec::new() }
            .build(cx, |_| {})
            .bind(p.progression, |mut h| h.needs_redraw())
            .bind(p.sevenths, |mut h| h.needs_redraw())
            .bind(p.voices, |mut h| h.needs_redraw())
            .bind(p.key, |mut h| h.needs_redraw())
            .bind(p.scale_mask, |mut h| h.needs_redraw())
            .bind(p.theme, |mut h| h.needs_redraw())
    }

    fn voiced(&self) -> Vec<(Chord, Vec<u8>)> {
        voiced(&self.p.progression.get(), self.p.key.get(), self.p.scale_mask.get(), self.p.sevenths.get(), self.p.voices.get())
    }

    /// Column width and where the columns start, for `n` chords in `w`.
    fn columns(n: usize, x: f32, w: f32) -> (f32, f32) {
        let col = (w / n.max(1) as f32).min(170.0);
        (col, x + (w - col * n as f32) / 2.0)
    }

    fn release(&mut self, cx: &mut EventContext) {
        for note in std::mem::take(&mut self.held) {
            cx.emit(SynthEvent::KeyRelease(note));
        }
    }
}

const TOP: f32 = 34.0;
const BOTTOM: f32 = 22.0;
const NOTE_W: f32 = 34.0;
const NOTE_H: f32 = 16.0;

impl View for Chart {
    fn element(&self) -> Option<&'static str> {
        Some("voicing-chart")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, _| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                let b = cx.lbounds();
                let voiced = self.voiced();
                let (col, x0) = Self::columns(voiced.len(), b.x, b.w);
                let i = ((cx.lmouse().0 - x0) / col).floor();
                if i >= 0.0 && (i as usize) < voiced.len() {
                    self.release(cx);
                    self.held = voiced[i as usize].1.clone();
                    for &note in &self.held {
                        cx.emit(SynthEvent::KeyPress(note));
                    }
                    cx.capture();
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                self.release(cx);
                cx.release();
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let b = cx.lbounds();
        crate::hidpi::clip(canvas, b);
        let pal = self.p.theme.get().palette();
        let key = self.p.key.get();
        fill(canvas, vg::Rect::new(b.x, b.y, b.x + b.w, b.y + b.h), pal.bg_000);
        let voiced = self.voiced();
        if voiced.is_empty() {
            text(canvas, "Add chords above: each is voiced to move as little as it can from the one before.", b.x + 16.0, b.y + b.h / 2.0 + 4.0, 12.0, pal.ink_muted);
            return;
        }
        let low = voiced.iter().flat_map(|(_, n)| n.iter()).min().copied().unwrap_or(60) as f32 - 1.0;
        let high = voiced.iter().flat_map(|(_, n)| n.iter()).max().copied().unwrap_or(72) as f32 + 1.0;
        let span = (b.h - TOP - BOTTOM).max(20.0);
        let y_of = |pitch: u8| b.y + TOP + (high - pitch as f32) / (high - low).max(1.0) * span;
        let (col, x0) = Self::columns(voiced.len(), b.x, b.w);
        let cx_of = |i: usize| x0 + col * (i as f32 + 0.5);

        // Voices: a line from each chord's note to the next; held notes
        // (no movement) in signal.
        for (i, pair) in voiced.windows(2).enumerate() {
            for (&a, &bn) in pair[0].1.iter().zip(&pair[1].1) {
                let mut paint = vg::Paint::default();
                paint.set_anti_alias(true);
                paint.set_style(vg::PaintStyle::Stroke);
                let held = a == bn;
                paint.set_color(if held { pal.signal } else { pal.ink_muted });
                paint.set_stroke_width(if held { 3.0 } else { 1.5 });
                let mut path = vg::PathBuilder::new();
                path.move_to(vg::Point::new(cx_of(i) + NOTE_W / 2.0, y_of(a)));
                path.line_to(vg::Point::new(cx_of(i + 1) - NOTE_W / 2.0, y_of(bn)));
                canvas.draw_path(&path.detach(), &paint);
            }
        }

        let label_font = crate::canvas_text::canvas_font(11.0);
        for (i, (chord, notes)) in voiced.iter().enumerate() {
            let x = cx_of(i);
            // The chord's name, and below, how far its voices moved.
            let degree = self.p.progression.get().get(i).copied().unwrap_or(0);
            let title = format!("{}  {}", voicing::numeral(degree, chord), voicing::name(chord, key));
            let w = label_font.measure_str(&title, None).0;
            text(canvas, &title, x - w / 2.0, b.y + 18.0, 12.0, pal.ink);
            if i > 0 {
                let moved: i32 = voicing::movement(&voiced[i - 1].1, notes).iter().map(|m| m.abs()).sum();
                let held = voicing::movement(&voiced[i - 1].1, notes).iter().filter(|m| **m == 0).count();
                let line = if held > 0 { format!("{moved} st \u{b7} {held} held") } else { format!("{moved} st") };
                let w = label_font.measure_str(&line, None).0;
                text(canvas, &line, x - w / 2.0, b.y + b.h - 6.0, 11.0, pal.ink_muted);
            }
            for (v, &n) in notes.iter().enumerate() {
                let y = y_of(n);
                let rect = vg::Rect::new(x - NOTE_W / 2.0, y - NOTE_H / 2.0, x + NOTE_W / 2.0, y + NOTE_H / 2.0);
                let held = i > 0 && voiced[i - 1].1.get(v) == Some(&n);
                let mut paint = vg::Paint::default();
                paint.set_anti_alias(true);
                paint.set_color(if held { pal.signal_soft } else { pal.bg_200 });
                canvas.draw_rrect(vg::RRect::new_rect_xy(rect, 3.0, 3.0), &paint);
                paint.set_style(vg::PaintStyle::Stroke);
                paint.set_stroke_width(1.0);
                paint.set_color(if held { pal.signal } else { pal.line_control });
                canvas.draw_rrect(vg::RRect::new_rect_xy(rect, 3.0, 3.0), &paint);
                let name = format!("{}{}", note_name_for_key(n % 12, key), n as i32 / 12 - 1);
                let w = label_font.measure_str(&name, None).0;
                text(canvas, &name, x - w / 2.0, y + 4.0, 11.0, pal.ink);
            }
        }
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
