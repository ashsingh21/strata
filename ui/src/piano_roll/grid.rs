//! The piano roll's canvas: a row-label column, a bar.beat ruler, the note
//! grid (scale-degree rows, not every semitone - only degrees in the
//! current scale, plus any pitch the clip's own notes use), and a velocity
//! lane underneath. One canvas, like the timeline's own `LaneArea`, laid
//! out as:
//!
//! ```text
//!  LABEL_W | ruler (RULER_H)
//!  labels  | rows (ROW_H each)
//!  "Vel."  | velocity lane (VEL_H)
//! ```

use std::collections::HashSet;

use vizia::prelude::*;
use crate::hidpi::Logical;
use vizia::vg;

use shared::arrangement::{Arrangement, ClipId, MidiNote, SnapGrid, Ticks, DEFAULT_VELOCITY, PPQ};
use shared::theory::{degree_name, degrees_in_mask, note_name};

use crate::piano_roll::state::{ChordShape, EditMode, LabelMode, NoteKey, PianoRollEvent};
use crate::timeline::state::TimelineEvent;
use crate::tokens::{Palette, ThemeId};

pub const ROW_H: f32 = 20.0;
/// The row-label column (`size-ed-head`).
pub const LABEL_W: f32 = 88.0;
pub const RULER_H: f32 = 24.0;
pub const VEL_H: f32 = 56.0;
/// How close (px) a click must be to a velocity stem to grab it.
const STEM_GRAB_PX: f32 = 6.0;

/// Which pitches get a row: every scale degree across the clip's own
/// range (a few semitones of headroom either side, at least an octave and
/// a half so there's room to write), or two octaves up from the key's
/// third octave for an empty clip - plus any pitch the notes use even if
/// it's off-scale (e.g. from before a key change). High to low,
/// piano-style. `octave` moves the two-octave window (the Octave −/+ in
/// the header); notes outside it still get their rows.
pub fn row_pitches(notes: &[MidiNote], key: u8, mask: u16, drums: bool, octave: i32) -> Vec<u8> {
    // A Drum Kit clip is a step grid: one row per pad (plus any other
    // pitch the notes use), kick at the bottom, scale ignored.
    if drums {
        let mut rows: Vec<u8> = shared::drums::DRUM_KIT.iter().map(|p| p.note).collect();
        rows.extend(notes.iter().map(|n| n.pitch));
        rows.sort_unstable_by(|a, b| b.cmp(a));
        rows.dedup();
        return rows;
    }
    // Two octaves of the key - moved down by whole octaves if a note sits
    // below them, and grown upward to reach the highest note. Clicking a
    // visible row never moves the rows (a range fitted tightly round the
    // notes re-centred with every new note, so the grid slid under the
    // pointer); an empty clip shows the octave from the key's third.
    let base = 48 + key as i32;
    // Only ever moved down (so the top row's note can't push it up).
    let window = base + 12 * octave;
    let lowest = notes.iter().map(|n| n.pitch as i32).min().map(|min| (min - window).div_euclid(12).min(0)).unwrap_or(0);
    let mut lo = window + 12 * lowest;
    // Left alone, the window follows low notes down (a bass clip gets a
    // compact grid); moved by hand, it stays where it was put.
    let top = if octave == 0 { lo + 24 } else { (lo + 24).max(window + 24) };
    let mut hi = top.max(notes.iter().map(|n| n.pitch as i32).max().unwrap_or(0));
    lo = lo.max(0);
    hi = hi.min(127);

    let used: HashSet<u8> = notes.iter().map(|n| n.pitch).collect();
    let degrees = degrees_in_mask(mask);
    let mut rows: Vec<u8> = (lo..=hi)
        .map(|p| p as u8)
        .filter(|&p| {
            let rel = ((p as i32 - key as i32).rem_euclid(12)) as u8;
            degrees.contains(&rel) || used.contains(&p)
        })
        .collect();
    rows.sort_unstable_by(|a, b| b.cmp(a));
    rows
}

/// Whether `clip` sits on a Drum Kit track (its editor is a step grid).
pub fn is_drum_clip(arr: &Arrangement, clip: ClipId) -> bool {
    arr.clip(clip)
        .and_then(|c| arr.track(c.track))
        .is_some_and(|t| t.instrument == Some(shared::arrangement::Instrument::Drums))
}

/// The canvas height for `rows` rows: ruler + rows + velocity lane.
pub fn grid_height(rows: usize) -> f32 {
    RULER_H + rows as f32 * ROW_H + VEL_H
}

/// "C4"-style name: pitch class plus octave (MIDI 60 = C4).
pub fn note_with_octave(pitch: u8) -> String {
    format!("{}{}", note_name(pitch % 12), pitch as i32 / 12 - 1)
}

/// A tick offset inside a clip as bar.beat.sixteenth, 1-based.
pub fn ticks_to_bbs(t: Ticks) -> String {
    let bar = t / (PPQ * 4);
    let beat = (t % (PPQ * 4)) / PPQ;
    let sixteenth = (t % PPQ) / (PPQ / 4);
    format!("{}.{}.{}", bar + 1, beat + 1, sixteenth + 1)
}

pub struct Grid {
    arrangement: Signal<Arrangement>,
    open_clip: Signal<Option<ClipId>>,
    mode: Signal<EditMode>,
    label_mode: Signal<LabelMode>,
    selected: Signal<HashSet<NoteKey>>,
    snap: Signal<SnapGrid>,
    key: Signal<u8>,
    scale_mask: Signal<u16>,
    playhead: Signal<Ticks>,
    theme: Signal<ThemeId>,
    /// Where the rows' two-octave window sits (see `row_pitches`).
    octave: Signal<i32>,
    /// What a Draw click writes: one note or a chord.
    chord: Signal<ChordShape>,
    /// A velocity stem being dragged: the note and its live (uncommitted)
    /// velocity. Committed as one undoable edit on release.
    vel_drag: Option<(NoteKey, u8)>,
    /// Wheel travel over the row labels in the current gesture, and when
    /// the last scroll arrived: one gesture moves one octave, so a
    /// trackpad flick (dozens of small steps, then momentum) doesn't fly
    /// through all of them.
    wheel: f32,
    wheel_at: Option<std::time::Instant>,
    wheel_moved: bool,
}

impl Grid {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cx: &mut Context,
        arrangement: Signal<Arrangement>,
        open_clip: Signal<Option<ClipId>>,
        mode: Signal<EditMode>,
        label_mode: Signal<LabelMode>,
        selected: Signal<HashSet<NoteKey>>,
        snap: Signal<SnapGrid>,
        key: Signal<u8>,
        scale_mask: Signal<u16>,
        playhead: Signal<Ticks>,
        theme: Signal<ThemeId>,
        octave: Signal<i32>,
        chord: Signal<ChordShape>,
    ) -> Handle<'_, Self> {
        Self { arrangement, open_clip, mode, label_mode, selected, snap, key, scale_mask, playhead, theme, octave, chord, vel_drag: None, wheel: 0.0, wheel_at: None, wheel_moved: false }
            .build(cx, |_| {})
            .bind(arrangement, |mut h| h.needs_redraw())
            .bind(open_clip, |mut h| h.needs_redraw())
            .bind(mode, |mut h| h.needs_redraw())
            .bind(label_mode, |mut h| h.needs_redraw())
            .bind(selected, |mut h| h.needs_redraw())
            .bind(snap, |mut h| h.needs_redraw())
            .bind(key, |mut h| h.needs_redraw())
            .bind(scale_mask, |mut h| h.needs_redraw())
            .bind(playhead, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
            .bind(octave, |mut h| h.needs_redraw())
            .bind(crate::lessons::highlight_signal().unwrap_or_else(|| Signal::new(None)), |mut h| h.needs_redraw())
    }

    /// The open clip's id, start and length, plus its notes - or `None` if
    /// nothing's open (or the id is stale).
    /// The open clip's id, start, editable length and notes. The editable
    /// length is the pattern (`content_len`): a looping clip is edited as
    /// its one pattern, not as every repeat.
    fn clip_info(&self) -> Option<(ClipId, Ticks, Ticks, Vec<MidiNote>)> {
        let id = self.open_clip.get()?;
        let arr = self.arrangement.get();
        let clip = arr.clip(id)?;
        let shared::arrangement::ClipContent::Midi { notes, .. } = &clip.content else { return None };
        Some((id, clip.start, clip.content_len(), notes.clone()))
    }

    fn drums(&self) -> bool {
        self.open_clip.get().is_some_and(|id| is_drum_clip(&self.arrangement.get(), id))
    }

    /// The open clip's track colour - notes are filled with it.
    fn clip_color(&self) -> Color {
        let arr = self.arrangement.get();
        self.open_clip
            .get()
            .and_then(|id| arr.clip(id))
            .and_then(|clip| arr.track(clip.track))
            .map(|track| crate::timeline::header::clip_color_to_rgb(track.color))
            .unwrap_or(crate::tokens::CLIP_VIOLET)
    }

    /// Hit-tests `(tick, pitch)` against `notes`, returning the note under
    /// it if any.
    fn note_at(notes: &[MidiNote], tick: Ticks, pitch: u8) -> Option<MidiNote> {
        notes.iter().rev().find(|n| n.pitch == pitch && tick >= n.start && tick < n.start + n.length).copied()
    }

    /// Velocity (1..=127) for a y position inside the velocity lane.
    /// The notes a velocity drag on `key`'s stem changes: every note that
    /// starts with it - a chord's stems stand on top of each other, and
    /// changing just one of them barely changed the sound - or, if some of
    /// those are selected, only the selected ones (to balance a chord).
    fn velocity_group(&self, notes: &[MidiNote], key: NoteKey) -> Vec<NoteKey> {
        let column: Vec<NoteKey> = notes.iter().filter(|n| n.start == key.0).map(|n| (n.start, n.pitch)).collect();
        let selected = self.selected.get();
        let chosen: Vec<NoteKey> = column.iter().copied().filter(|k| selected.contains(k)).collect();
        if chosen.is_empty() {
            column
        } else {
            chosen
        }
    }

    fn velocity_at(lane_top: f32, y: f32) -> u8 {
        let t = 1.0 - ((y - lane_top - 6.0) / (VEL_H - 10.0)).clamp(0.0, 1.0);
        (1.0 + t * 126.0).round() as u8
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

impl View for Grid {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            // The wheel over the row labels moves the rows an octave.
            WindowEvent::MouseScroll(_, y) => {
                let bounds = cx.lbounds();
                if self.drums() || cx.lmouse().0 - bounds.x >= LABEL_W {
                    return;
                }
                meta.consume();
                let now = std::time::Instant::now();
                let new_gesture = self.wheel_at.is_none_or(|at| now.duration_since(at).as_millis() > 150);
                self.wheel_at = Some(now);
                if new_gesture || self.wheel.signum() != y.signum() {
                    self.wheel = 0.0;
                    self.wheel_moved = false;
                }
                self.wheel += *y;
                if !self.wheel_moved && self.wheel.abs() >= 1.0 {
                    cx.emit(PianoRollEvent::ShiftOctave(self.wheel.signum() as i32));
                    self.wheel_moved = true;
                }
            }
            // A quick second click on the same pixel arrives as a
            // Double/TripleClick instead of a MouseDown - still a click.
            WindowEvent::MouseDown(MouseButton::Left)
            | WindowEvent::MouseDoubleClick(MouseButton::Left)
            | WindowEvent::MouseTripleClick(MouseButton::Left) => {
                let Some((clip_id, _clip_start, clip_length, notes)) = self.clip_info() else { return };
                let bounds = cx.lbounds();
                let rows = row_pitches(&notes, self.key.get(), self.scale_mask.get(), self.drums(), self.octave.get());
                if rows.is_empty() || clip_length <= 0 {
                    return;
                }
                cx.focus_with_visibility(false);

                let lx = cx.lmouse().0 - bounds.x - LABEL_W;
                let ly = cx.lmouse().1 - bounds.y - RULER_H;
                if lx < 0.0 || ly < 0.0 {
                    return;
                }
                let grid_w = bounds.w - LABEL_W;
                let px_per_tick = grid_w as f64 / clip_length as f64;
                let rows_h = rows.len() as f32 * ROW_H;

                // Velocity lane: grab the nearest stem.
                if ly >= rows_h {
                    let lane_top = bounds.y + RULER_H + rows_h;
                    let nearest = notes
                        .iter()
                        .map(|n| (n, ((n.start as f64 * px_per_tick) as f32 - lx).abs()))
                        .filter(|(_, d)| *d <= STEM_GRAB_PX)
                        .min_by(|a, b| a.1.total_cmp(&b.1));
                    if let Some((note, _)) = nearest {
                        let velocity = Self::velocity_at(lane_top, cx.lmouse().1);
                        self.vel_drag = Some(((note.start, note.pitch), velocity));
                        cx.capture();
                        cx.needs_redraw();
                    }
                    return;
                }

                let pitch = rows[(ly / ROW_H) as usize];
                let raw_tick = (lx as f64 / px_per_tick) as Ticks;

                match self.mode.get() {
                    EditMode::Draw => {
                        // The grid square under the pointer, not the nearest
                        // line: clicking inside a square puts the note in it
                        // (rounding sent anything past the middle one square
                        // late). Alt places it freely.
                        let bypass = cx.modifiers().alt();
                        let snapped = match self.snap.get().ticks() {
                            Some(step) if !bypass => raw_tick.div_euclid(step) * step,
                            _ => raw_tick,
                        }
                        .clamp(0, clip_length - 1);
                        if let Some(hit) = Self::note_at(&notes, snapped, pitch) {
                            cx.emit(TimelineEvent::RemoveMidiNoteAt { clip: clip_id, start: hit.start, pitch: hit.pitch });
                        } else {
                            let step = self.snap.get().ticks().unwrap_or(PPQ / 4);
                            let length = step.min(clip_length - snapped).max(1);
                            let shape = if self.drums() { ChordShape::Note } else { self.chord.get() };
                            let notes = shape
                                .pitches(pitch, self.key.get(), self.scale_mask.get())
                                .into_iter()
                                .map(|pitch| MidiNote { start: snapped, length, pitch, velocity: DEFAULT_VELOCITY })
                                .collect();
                            cx.emit(TimelineEvent::AddMidiNotesAt { clip: clip_id, notes });
                        }
                    }
                    EditMode::Select => {
                        let extend = cx.modifiers().shift();
                        if let Some(hit) = Self::note_at(&notes, raw_tick.clamp(0, clip_length - 1), pitch) {
                            cx.emit(PianoRollEvent::SelectNote { key: (hit.start, hit.pitch), extend });
                        } else if !extend {
                            cx.emit(PianoRollEvent::ClearSelection);
                        }
                    }
                }
            }
            // Selected notes: up/down a scale step, or with Shift an
            // octave. (The grid has focus once you've clicked in it.)
            WindowEvent::KeyDown(code @ (Code::ArrowUp | Code::ArrowDown), _) => {
                if self.selected.get().is_empty() {
                    return;
                }
                meta.consume();
                let dir = if *code == Code::ArrowUp { 1 } else { -1 };
                let (octaves, steps) = if cx.modifiers().shift() { (dir, 0) } else { (0, dir) };
                cx.emit(TimelineEvent::MoveSelectedNotes { octaves, steps, key: self.key.get(), mask: self.scale_mask.get() });
            }
            WindowEvent::MouseMove(_, y) => {
                if let Some((key, _)) = self.vel_drag {
                    let Some((_, _, _, notes)) = self.clip_info() else { return };
                    let rows = row_pitches(&notes, self.key.get(), self.scale_mask.get(), self.drums(), self.octave.get());
                    let lane_top = cx.lbounds().y + RULER_H + rows.len() as f32 * ROW_H;
                    self.vel_drag = Some((key, Self::velocity_at(lane_top, crate::hidpi::l(cx, *y))));
                    cx.needs_redraw();
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if let Some((key, velocity)) = self.vel_drag.take() {
                    cx.release();
                    if let (Some(clip), Some((_, _, _, notes))) = (self.open_clip.get(), self.clip_info()) {
                        let group = self.velocity_group(&notes, key);
                        cx.emit(TimelineEvent::SetNoteVelocities { clip, notes: group, velocity });
                    }
                }
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let bounds = cx.lbounds();
        crate::hidpi::clip(canvas, bounds);
        let p: Palette = self.theme.get().palette();
        let Some((_clip_id, clip_start, clip_length, notes)) = self.clip_info() else { return };
        if clip_length <= 0 {
            return;
        }
        let key = self.key.get();
        let drums = self.drums();
        let rows = row_pitches(&notes, key, self.scale_mask.get(), drums, self.octave.get());
        if rows.is_empty() {
            return;
        }
        let label_mode = self.label_mode.get();
        let selected = self.selected.get();
        let clip_color = self.clip_color();
        let degrees = degrees_in_mask(self.scale_mask.get());

        let gx = bounds.x + LABEL_W;
        let gw = bounds.w - LABEL_W;
        let top = bounds.y + RULER_H;
        let rows_h = rows.len() as f32 * ROW_H;
        let lane_top = top + rows_h;
        let bottom = bounds.y + bounds.h;
        let px_per_tick = gw as f64 / clip_length as f64;
        let tick_to_x = |t: Ticks| gx + (t as f64 * px_per_tick) as f32;

        // Grounds: ruler and label column on bg-000, grid on bg-100.
        fill(canvas, vg::Rect::new(bounds.x, bounds.y, bounds.x + bounds.w, bottom), p.bg_100);
        fill(canvas, vg::Rect::new(bounds.x, bounds.y, bounds.x + bounds.w, top), p.bg_000);
        fill(canvas, vg::Rect::new(bounds.x, top, gx, bottom), p.bg_000);

        // Rows: root rows get a selection wash across the grid and a 3px
        // ink tick on their label; rows off the scale get a bg-000 stripe.
        for (i, &pitch) in rows.iter().enumerate() {
            let y0 = top + i as f32 * ROW_H;
            let y1 = y0 + ROW_H;
            let rel = ((pitch as i32 - key as i32).rem_euclid(12)) as u8;
            if drums {
                // Alternate rows shaded so a hit reads across to its pad.
                if i % 2 == 1 {
                    fill(canvas, vg::Rect::new(gx, y0, gx + gw, y1), p.bg_000);
                }
            } else if rel == 0 {
                fill(canvas, vg::Rect::new(gx, y0, gx + gw, y1), p.selection);
                fill(canvas, vg::Rect::new(bounds.x, y0 + 2.0, bounds.x + 3.0, y1 - 2.0), p.ink);
            } else if !degrees.contains(&rel) {
                fill(canvas, vg::Rect::new(gx, y0, gx + gw, y1), p.bg_000);
            }
            // A lesson pointing at this row: it glows across the grid.
            if crate::lessons::highlighted() == Some(crate::lessons::Target::PianoRollRow(pitch)) {
                fill(canvas, vg::Rect::new(bounds.x, y0, bounds.x + bounds.w, y1), p.signal_soft);
            }
            fill(canvas, vg::Rect::new(bounds.x, y1 - 1.0, bounds.x + bounds.w, y1), p.grid_beat);

            let (big, small) = match (drums, label_mode) {
                (true, _) => (
                    shared::drums::pad_for_note(pitch).map(|p| p.name.to_string()).unwrap_or_else(|| note_with_octave(pitch)),
                    String::new(),
                ),
                (false, LabelMode::Intervals) => (degree_name(rel).to_string(), note_with_octave(pitch)),
                (false, LabelMode::Notes) => (note_with_octave(pitch), degree_name(rel).to_string()),
            };
            let baseline = y0 + ROW_H * 0.5 + 4.0;
            text(canvas, &big, bounds.x + 10.0, baseline, 12.0, p.ink);
            text(canvas, &small, bounds.x + 44.0, baseline, 11.0, p.ink_muted);
        }

        // Column heads.
        let head = match (drums, label_mode) {
            (true, _) => "Pad",
            (false, LabelMode::Intervals) => "Interval",
            (false, LabelMode::Notes) => "Note",
        };
        text(canvas, head, bounds.x + 10.0, bounds.y + 16.0, 11.0, p.ink_muted);
        text(canvas, "Velocity", bounds.x + 10.0, lane_top + 18.0, 11.0, p.ink_muted);

        // Vertical grid through rows and lane: bars, beats, sixteenths
        // (sixteenths dropped when they'd sit closer than 6px).
        let sixteenth = PPQ / 4;
        let sixteenths_fit = (sixteenth as f64 * px_per_tick) >= 6.0;
        let mut t = 0;
        while t <= clip_length {
            let x = tick_to_x(t).round();
            let color = if t % (PPQ * 4) == 0 {
                p.ink_faint
            } else if t % PPQ == 0 {
                p.line
            } else {
                p.grid_beat
            };
            if t % PPQ == 0 || sixteenths_fit {
                fill(canvas, vg::Rect::new(x, top, x + 1.0, bottom), color);
            }
            t += sixteenth;
        }

        // Ruler: bar numbers in ink, beats as bar.beat, sixteenth ticks.
        let mut t = 0;
        while t < clip_length {
            let x = tick_to_x(t);
            let is_bar = t % (PPQ * 4) == 0;
            let is_beat = t % PPQ == 0;
            let tick_h = if is_bar { 10.0 } else if is_beat { 6.0 } else { 3.0 };
            if is_beat || sixteenths_fit {
                fill(canvas, vg::Rect::new(x, top - tick_h, x + 1.0, top), if is_beat { p.ink_muted } else { p.ink_faint });
            }
            if is_beat {
                let bar = t / (PPQ * 4) + 1;
                let beat = (t % (PPQ * 4)) / PPQ + 1;
                if is_bar {
                    text(canvas, &bar.to_string(), x + 3.0, bounds.y + 13.0, 11.0, p.ink);
                } else if (PPQ as f64 * px_per_tick) >= 28.0 {
                    text(canvas, &format!("{bar}.{beat}"), x + 3.0, bounds.y + 13.0, 11.0, p.ink_muted);
                }
            }
            t += sixteenth;
        }
        fill(canvas, vg::Rect::new(bounds.x, top - 1.0, bounds.x + bounds.w, top), p.line);
        fill(canvas, vg::Rect::new(gx - 1.0, bounds.y, gx, bottom), p.line);
        fill(canvas, vg::Rect::new(bounds.x, lane_top, bounds.x + bounds.w, lane_top + 1.0), p.line);

        let mut playhead = self.playhead.get() - clip_start;
        // Inside a looping clip, the playhead wraps round the pattern.
        let full_length = self.open_clip.get().and_then(|id| self.arrangement.get().clip(id).map(|c| c.length)).unwrap_or(0);
        if playhead >= 0 && playhead < full_length {
            playhead %= clip_length;
        }
        let label_font = crate::canvas_text::canvas_font(11.0);

        // Notes: clip colour with a faint edge; `signal` while sounding;
        // a 2px ink outline when selected.
        // The notes a velocity drag in progress moves together.
        let drag_group = self.vel_drag.map(|(k, _)| self.velocity_group(&notes, k)).unwrap_or_default();
        for note in &notes {
            let Some(row) = rows.iter().position(|&r| r == note.pitch) else { continue };
            let y0 = top + row as f32 * ROW_H + 1.0;
            let y1 = y0 + ROW_H - 3.0;
            let x0 = tick_to_x(note.start) + 1.0;
            let x1 = tick_to_x(note.start + note.length).min(gx + gw);
            if x1 <= x0 {
                continue;
            }
            let key_of_note = (note.start, note.pitch);
            let is_selected = selected.contains(&key_of_note);
            let is_sounding = playhead >= note.start && playhead < note.start + note.length;
            let rect = vg::Rect::new(x0, y0, x1, y1);
            fill(canvas, rect, if is_sounding { p.signal } else { clip_color });

            let mut edge = vg::Paint::default();
            edge.set_style(vg::PaintStyle::Stroke);
            edge.set_anti_alias(true);
            if is_selected {
                edge.set_color(p.ink);
                edge.set_stroke_width(2.0);
                canvas.draw_path(&vg::Path::rect(rect.with_outset((1.0, 1.0)), None), &edge);
            } else {
                edge.set_color(p.ink_faint);
                edge.set_stroke_width(1.0);
                canvas.draw_path(&vg::Path::rect(rect.with_inset((0.5, 0.5)), None), &edge);
            }

            if x1 - x0 >= 18.0 && !drums {
                let rel = ((note.pitch as i32 - key as i32).rem_euclid(12)) as u8;
                let name = match label_mode {
                    LabelMode::Notes => note_name(note.pitch % 12).to_string(),
                    LabelMode::Intervals => degree_name(rel).to_string(),
                };
                let mut paint = vg::Paint::default();
                paint.set_color(crate::tokens::ON_CLIP);
                paint.set_anti_alias(true);
                canvas.draw_str(&name, vg::Point::new(x0 + 4.0, y0 + ROW_H * 0.5 + 2.0), &label_font, &paint);
            }

            // Velocity stem: ink-muted at rest, signal sounding, ink when
            // selected or being dragged.
            let dragging = self.vel_drag.is_some_and(|(k, _)| drag_group.contains(&key_of_note) && k.0 == note.start);
            let velocity = match self.vel_drag {
                Some((_, v)) if dragging => v,
                _ => note.velocity,
            };
            let stem_color = if is_sounding {
                p.signal
            } else if is_selected || dragging {
                p.ink
            } else {
                p.ink_muted
            };
            let stem_top = lane_top + 6.0 + (1.0 - velocity as f32 / 127.0) * (VEL_H - 10.0);
            let sx = tick_to_x(note.start).round() + 1.0;
            fill(canvas, vg::Rect::new(sx, stem_top, sx + 2.0, bottom - 4.0), stem_color);
            fill(canvas, vg::Rect::new(sx - 2.0, stem_top - 1.0, sx + 4.0, stem_top + 2.0), stem_color);
        }

        // Playhead through ruler, rows and lane, with a triangle head.
        if playhead >= 0 && playhead < clip_length {
            let x = tick_to_x(playhead).round();
            fill(canvas, vg::Rect::new(x, bounds.y, x + 1.0, bottom), p.playhead);
            let mut head = vg::PathBuilder::new();
            head.move_to(vg::Point::new(x - 4.5, bounds.y));
            head.line_to(vg::Point::new(x + 5.5, bounds.y));
            head.line_to(vg::Point::new(x + 0.5, bounds.y + 6.0));
            head.close();
            let mut paint = vg::Paint::default();
            paint.set_color(p.playhead);
            paint.set_anti_alias(true);
            canvas.draw_path(&head.detach(), &paint);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: u8 = 9;
    const MINOR_PENTATONIC: u16 = 0b0100_1010_1001;

    fn note(pitch: u8) -> MidiNote {
        MidiNote { start: 0, length: PPQ / 4, pitch, velocity: 100 }
    }

    #[test]
    fn an_empty_clip_starts_on_the_keys_third_octave() {
        let rows = row_pitches(&[], A, MINOR_PENTATONIC, false, 0);
        assert_eq!(*rows.last().unwrap(), 57, "A3 at the bottom");
    }

    #[test]
    fn adding_a_note_on_a_visible_row_never_moves_the_rows() {
        let empty = row_pitches(&[], A, MINOR_PENTATONIC, false, 0);
        for &pitch in &empty {
            assert_eq!(row_pitches(&[note(pitch)], A, MINOR_PENTATONIC, false, 0), empty, "after adding {pitch}");
        }
        // The same from a clip that already has low notes.
        let low = [note(33), note(45)];
        let rows = row_pitches(&low, A, MINOR_PENTATONIC, false, 0);
        for &pitch in &rows {
            let mut more = low.to_vec();
            more.push(note(pitch));
            assert_eq!(row_pitches(&more, A, MINOR_PENTATONIC, false, 0), rows, "after adding {pitch}");
        }
    }

    #[test]
    fn low_notes_get_a_compact_grid_round_them() {
        let rows = row_pitches(&[note(29), note(43)], A, MINOR_PENTATONIC, false, 0);
        assert!(rows.contains(&29) && rows.contains(&43));
        assert!(rows.len() <= 12, "{} rows", rows.len());
    }

    #[test]
    fn the_octave_moves_the_window_but_keeps_every_note() {
        let up = row_pitches(&[], A, MINOR_PENTATONIC, false, 1);
        assert_eq!(*up.last().unwrap(), 69, "A4 at the bottom");
        let down = row_pitches(&[], A, MINOR_PENTATONIC, false, -2);
        assert_eq!(*down.last().unwrap(), 33, "A1 at the bottom");
        let with_low_note = row_pitches(&[note(45)], A, MINOR_PENTATONIC, false, 1);
        assert!(with_low_note.contains(&45) && with_low_note.contains(&93), "A2 note kept, A6 top kept");
    }
}
