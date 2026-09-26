//! The keyboard strip: 3 octaves (21 white keys, 15 black), held notes lit
//! `signal`. There's no MIDI input wired up, so this is a stand-in for a
//! real controller: press-and-hold, same as the computer-keyboard input -
//! a note sounds only as long as the mouse button stays down on it, not
//! a click-to-toggle latch.

use vizia::prelude::*;
use vizia::vg;

use shared::synth::SynthState;

use crate::tokens::ThemeId;
use crate::synth::state::SynthEvent;

const WHITE_KEYS: usize = 21;
const BASE_NOTE: u8 = 48; // C3
const WHITE_SEMITONES: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
const BLACK_AFTER: [bool; 7] = [true, true, false, true, true, true, false];
const BLACK_SEMITONE: [u8; 7] = [1, 3, 0, 6, 8, 10, 0];
const BLACK_HEIGHT_FRAC: f32 = 0.6;
const BLACK_WIDTH_FRAC: f32 = 0.032;

fn white_note(white_index: usize) -> u8 {
    BASE_NOTE + (white_index / 7 * 12) as u8 + WHITE_SEMITONES[white_index % 7]
}

fn black_note_after(white_index: usize) -> Option<u8> {
    let i = white_index % 7;
    BLACK_AFTER[i].then(|| BASE_NOTE + (white_index / 7 * 12) as u8 + BLACK_SEMITONE[i])
}

pub struct Keyboard {
    state: Signal<SynthState>,
    theme: Signal<ThemeId>,
    /// The note the mouse button is currently down on, if any - released
    /// on `MouseUp` regardless of where the cursor ends up, via
    /// `cx.capture()`, same as the knob's drag handling.
    pressed: Option<u8>,
}

impl Keyboard {
    pub fn new(cx: &mut Context, state: Signal<SynthState>, theme: Signal<ThemeId>) -> Handle<'_, Self> {
        Self { state, theme, pressed: None }
            .build(cx, |_| {})
            .bind(state, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
    }

    /// Hit-tests a local (view-relative) point against the keys, black
    /// keys first since they sit on top.
    fn note_at(bounds: BoundingBox, lx: f32, ly: f32) -> Option<u8> {
        let white_w = bounds.w / WHITE_KEYS as f32;

        if ly < bounds.h * BLACK_HEIGHT_FRAC {
            for white_index in 0..WHITE_KEYS {
                if let Some(note) = black_note_after(white_index) {
                    let center = (white_index + 1) as f32 / WHITE_KEYS as f32 * bounds.w;
                    let half = BLACK_WIDTH_FRAC * bounds.w * 0.5;
                    if lx >= center - half && lx <= center + half {
                        return Some(note);
                    }
                }
            }
        }

        let white_index = ((lx / white_w) as usize).min(WHITE_KEYS - 1);
        Some(white_note(white_index))
    }
}

impl View for Keyboard {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, _| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                let bounds = cx.bounds();
                let lx = cx.mouse().cursor_x - bounds.x;
                let ly = cx.mouse().cursor_y - bounds.y;
                if let Some(note) = Self::note_at(bounds, lx, ly) {
                    self.pressed = Some(note);
                    cx.capture();
                    cx.emit(SynthEvent::KeyPress(note));
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if let Some(note) = self.pressed.take() {
                    cx.release();
                    cx.emit(SynthEvent::KeyRelease(note));
                }
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();
        let held = &self.state.get().held_notes;
        let white_w = bounds.w / WHITE_KEYS as f32;

        for white_index in 0..WHITE_KEYS {
            let x0 = bounds.x + white_index as f32 * white_w;
            let mut paint = vg::Paint::default();
            let is_held = held.contains(&white_note(white_index));
            paint.set_color(if is_held { palette.signal } else { palette.key_white });
            paint.set_anti_alias(true);
            let rect = vg::Rect::new(x0 + 0.5, bounds.y, x0 + white_w - 0.5, bounds.y + bounds.h);
            canvas.draw_path(&vg::Path::rect(rect, None), &paint);
        }

        for white_index in 0..WHITE_KEYS {
            let Some(note) = black_note_after(white_index) else { continue };
            let center = bounds.x + (white_index + 1) as f32 / WHITE_KEYS as f32 * bounds.w;
            let half = BLACK_WIDTH_FRAC * bounds.w * 0.5;
            let mut paint = vg::Paint::default();
            let is_held = held.contains(&note);
            paint.set_color(if is_held { palette.signal } else { palette.key_black });
            paint.set_anti_alias(true);
            let rect = vg::Rect::new(center - half, bounds.y, center + half, bounds.y + bounds.h * BLACK_HEIGHT_FRAC);
            canvas.draw_path(&vg::Path::rect(rect, None), &paint);
        }
    }
}
