//! The piano roll's grid: pitch rows (scale-degree rows, not every
//! semitone - only degrees actually in the current scale, plus whatever
//! pitches the clip's own notes use even if they've drifted off-scale)
//! across the clip's own tick range. One canvas, like the timeline's own
//! `LaneArea`.

use std::collections::HashSet;

use vizia::prelude::*;
use vizia::vg;

use shared::arrangement::{snap, Arrangement, ClipId, MidiNote, SnapGrid, Ticks, PPQ};
use shared::theory::{degree_name, degrees_in_mask, note_name};

use crate::piano_roll::state::{EditMode, LabelMode, NoteKey, PianoRollEvent};
use crate::timeline::state::TimelineEvent;
use crate::tokens::ThemeId;

const ROW_H: f32 = 20.0;

/// Which pitches get a row: every scale degree within two octaves of a
/// sensible centre, plus any pitch the clip's own notes actually use (so a
/// note that's off-scale, e.g. from before a key change, still has
/// somewhere to live) - sorted high to low, piano-style.
fn row_pitches(notes: &[MidiNote], key: u8, mask: u16) -> Vec<u8> {
    let mut lo = 48i32 + key as i32;
    let mut hi = lo + 24;
    for n in notes {
        lo = lo.min(n.pitch as i32 - 2);
        hi = hi.max(n.pitch as i32 + 2);
    }
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
    ) -> Handle<'_, Self> {
        Self { arrangement, open_clip, mode, label_mode, selected, snap, key, scale_mask, playhead, theme }
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
    }

    /// The open clip's id, start and length, plus its notes - or `None` if
    /// nothing's open (or the id is stale).
    fn clip_info(&self) -> Option<(ClipId, Ticks, Ticks, Vec<MidiNote>)> {
        let id = self.open_clip.get()?;
        let arr = self.arrangement.get();
        let clip = arr.clip(id)?;
        let shared::arrangement::ClipContent::Midi { notes } = &clip.content else { return None };
        Some((id, clip.start, clip.length, notes.clone()))
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
}

impl View for Grid {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, _| {
            if let WindowEvent::MouseDown(MouseButton::Left) = window_event {
                let Some((clip_id, _clip_start, clip_length, notes)) = self.clip_info() else { return };
                let bounds = cx.bounds();
                let rows = row_pitches(&notes, self.key.get(), self.scale_mask.get());
                if rows.is_empty() || clip_length <= 0 {
                    return;
                }
                cx.focus_with_visibility(false);

                let lx = cx.mouse().cursor_x - bounds.x;
                let ly = cx.mouse().cursor_y - bounds.y;
                let row_index = (ly / ROW_H) as usize;
                if row_index >= rows.len() {
                    return;
                }
                let pitch = rows[row_index];
                let px_per_tick = bounds.w as f64 / clip_length as f64;
                let raw_tick = (lx as f64 / px_per_tick) as Ticks;

                match self.mode.get() {
                    EditMode::Draw => {
                        let bypass = cx.modifiers().alt();
                        let snapped = snap(raw_tick, self.snap.get(), bypass).clamp(0, clip_length - 1);
                        if let Some(hit) = Self::note_at(&notes, snapped, pitch) {
                            cx.emit(TimelineEvent::RemoveMidiNoteAt { clip: clip_id, start: hit.start, pitch: hit.pitch });
                        } else {
                            let step = self.snap.get().ticks().unwrap_or(PPQ / 4);
                            let length = step.min(clip_length - snapped).max(1);
                            cx.emit(TimelineEvent::AddMidiNoteAt {
                                clip: clip_id,
                                note: MidiNote { start: snapped, length, pitch },
                            });
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
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();
        let Some((_clip_id, clip_start, clip_length, notes)) = self.clip_info() else { return };
        if clip_length <= 0 {
            return;
        }
        let key = self.key.get();
        let rows = row_pitches(&notes, key, self.scale_mask.get());
        if rows.is_empty() {
            return;
        }
        let label_mode = self.label_mode.get();
        let selected = self.selected.get();
        let px_per_tick = bounds.w as f64 / clip_length as f64;
        let tick_to_x = |t: Ticks| bounds.x + (t as f64 * px_per_tick) as f32;

        let label_font = crate::canvas_text::canvas_font(11.0);
        let clip_color = self.clip_color();

        // Rows: background band + separator + degree/note label.
        for (i, &pitch) in rows.iter().enumerate() {
            let y0 = bounds.y + i as f32 * ROW_H;
            if y0 > bounds.y + bounds.h {
                break;
            }
            let y1 = (y0 + ROW_H).min(bounds.y + bounds.h);
            let is_root = pitch % 12 == key % 12;

            let mut band = vg::Paint::default();
            band.set_color(if is_root { palette.bg_200 } else { palette.bg_000 });
            band.set_anti_alias(true);
            canvas.draw_path(&vg::Path::rect(vg::Rect::new(bounds.x, y0, bounds.x + bounds.w, y1), None), &band);

            let mut sep = vg::Paint::default();
            sep.set_color(palette.line);
            sep.set_anti_alias(true);
            canvas.draw_path(&vg::Path::rect(vg::Rect::new(bounds.x, y1 - 1.0, bounds.x + bounds.w, y1), None), &sep);

            let rel = ((pitch as i32 - key as i32).rem_euclid(12)) as u8;
            let text = match label_mode {
                LabelMode::Notes => note_name(pitch % 12).to_string(),
                LabelMode::Intervals => degree_name(rel).to_string(),
            };
            let mut text_paint = vg::Paint::default();
            text_paint.set_color(palette.ink_muted);
            text_paint.set_anti_alias(true);
            canvas.draw_str(&text, vg::Point::new(bounds.x + 4.0, y0 + ROW_H * 0.5 + 4.0), &label_font, &text_paint);
        }

        // Vertical grid: a line every beat, heavier every bar.
        let ticks_per_beat = PPQ;
        let ticks_per_bar = PPQ * 4;
        let mut t = 0;
        while t <= clip_length {
            let x = tick_to_x(t);
            let mut grid_paint = vg::Paint::default();
            grid_paint.set_color(if t % ticks_per_bar == 0 { palette.grid_bar } else { palette.grid_beat });
            grid_paint.set_anti_alias(false);
            canvas.draw_path(&vg::Path::rect(vg::Rect::new(x, bounds.y, x + 1.0, bounds.y + bounds.h), None), &grid_paint);
            t += ticks_per_beat;
        }

        // Notes.
        for note in &notes {
            let Some(row) = rows.iter().position(|&p| p == note.pitch) else { continue };
            let y0 = (bounds.y + row as f32 * ROW_H + 1.0).max(bounds.y);
            let y1 = (y0 + ROW_H - 2.0).min(bounds.y + bounds.h);
            if y1 <= y0 {
                continue;
            }
            let x0 = tick_to_x(note.start).max(bounds.x);
            let x1 = tick_to_x(note.start + note.length).min(bounds.x + bounds.w);
            if x1 <= x0 {
                continue;
            }

            // Clip colour at rest, `signal` while sounding (under the
            // playhead, matching held pads), a 2px ink outline when selected.
            let is_selected = selected.contains(&(note.start, note.pitch));
            let playhead = self.playhead.get() - clip_start;
            let is_sounding = playhead >= note.start && playhead < note.start + note.length;
            let note_rect = vg::Rect::new(x0, y0, x1, y1);
            let mut fill = vg::Paint::default();
            fill.set_color(if is_sounding { palette.signal } else { clip_color });
            fill.set_anti_alias(true);
            canvas.draw_path(&vg::Path::rect(note_rect, None), &fill);

            let mut edge = vg::Paint::default();
            edge.set_style(vg::PaintStyle::Stroke);
            edge.set_anti_alias(true);
            if is_selected {
                edge.set_color(palette.ink);
                edge.set_stroke_width(2.0);
                canvas.draw_path(&vg::Path::rect(note_rect.with_outset((1.0, 1.0)), None), &edge);
            } else {
                edge.set_color(palette.ink_faint);
                edge.set_stroke_width(1.0);
                canvas.draw_path(&vg::Path::rect(note_rect.with_inset((0.5, 0.5)), None), &edge);
            }

            if x1 - x0 >= 16.0 {
                let rel = ((note.pitch as i32 - key as i32).rem_euclid(12)) as u8;
                let text = match label_mode {
                    LabelMode::Notes => note_name(note.pitch % 12).to_string(),
                    LabelMode::Intervals => degree_name(rel).to_string(),
                };
                let mut text_paint = vg::Paint::default();
                text_paint.set_color(crate::tokens::ON_CLIP);
                text_paint.set_anti_alias(true);
                canvas.draw_str(&text, vg::Point::new(x0 + 3.0, y0 + ROW_H * 0.5 + 3.0), &label_font, &text_paint);
            }
        }

        // Playhead, if the global transport is currently inside this clip.
        let ph = self.playhead.get();
        if ph >= clip_start && ph < clip_start + clip_length {
            let x = tick_to_x(ph - clip_start);
            let mut ph_paint = vg::Paint::default();
            ph_paint.set_color(palette.playhead);
            ph_paint.set_anti_alias(true);
            canvas.draw_path(&vg::Path::rect(vg::Rect::new(x, bounds.y, x + 1.0, bounds.y + bounds.h), None), &ph_paint);
        }
    }
}
