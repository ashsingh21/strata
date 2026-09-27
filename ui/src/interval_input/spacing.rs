//! Panel A - "True spacing": one pad per in-scale semitone across two
//! octaves, positioned proportionally to its semitone distance from the
//! root (so a wide gap in the scale shows up as visible empty space
//! rather than every degree being evenly spaced regardless of interval
//! size). Out-of-scale semitones get a thin tick instead of a pad.

use vizia::prelude::*;
use crate::hidpi::Logical;
use vizia::vg;

use shared::synth::SynthState;
use shared::theory::{degree_name, note_name};

use crate::tokens::ThemeId;

const SPAN_SEMITONES: f32 = 24.0;
const PAD_W: f32 = 46.0;
const PAD_H: f32 = 54.0;
/// C3: the same low-mid register the on-screen keyboard's leftmost key
/// sits at, so this tool and the keyboard agree on what "the root" sounds
/// like.
const BASE_NOTE: u8 = 48;

pub struct Spacing {
    synth_state: Signal<SynthState>,
    theme: Signal<ThemeId>,
    key: Signal<u8>,
    scale_mask: Signal<u16>,
}

impl Spacing {
    pub fn new(
        cx: &mut Context,
        synth_state: Signal<SynthState>,
        theme: Signal<ThemeId>,
        key: Signal<u8>,
        scale_mask: Signal<u16>,
    ) -> Handle<'_, Self> {
        Self { synth_state, theme, key, scale_mask }
            .build(cx, |_| {})
            .bind(synth_state, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
            .bind(key, |mut h| h.needs_redraw())
            .bind(scale_mask, |mut h| h.needs_redraw())
    }

    fn scale_offsets(&self) -> Vec<u8> {
        let degrees = shared::theory::degrees_in_mask(self.scale_mask.get());
        let mut out = Vec::new();
        for base in [0u8, 12] {
            for &d in &degrees {
                out.push(base + d);
            }
        }
        out.push(24);
        out
    }

    fn pad_x(&self, bounds: BoundingBox, offset: u8) -> f32 {
        bounds.x + (offset as f32 / SPAN_SEMITONES) * bounds.w
    }
}

impl View for Spacing {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, _| {
            if let WindowEvent::MouseDown(MouseButton::Left) = window_event {
                let bounds = cx.lbounds();
                let lx = cx.lmouse().0;
                let key = self.key.get();
                for offset in self.scale_offsets() {
                    let x = self.pad_x(bounds, offset);
                    if (lx - x).abs() <= PAD_W / 2.0 {
                        let note = BASE_NOTE + key + offset;
                        cx.emit(crate::synth::state::SynthEvent::ToggleKey(note));
                        break;
                    }
                }
            }
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let bounds = cx.lbounds();
        let palette = self.theme.get().palette();
        let key = self.key.get();
        let held = self.synth_state.get().held_notes.clone();
        let offsets = self.scale_offsets();

        let mid_y = bounds.y + bounds.h * 0.42;

        // Baseline + ticks for every semitone, on top of which pads sit.
        let mut axis = vg::Paint::default();
        axis.set_color(palette.line);
        axis.set_anti_alias(true);
        canvas.draw_path(
            &vg::Path::rect(vg::Rect::new(bounds.x, mid_y + PAD_H / 2.0 + 10.0, bounds.x + bounds.w, mid_y + PAD_H / 2.0 + 11.0), None),
            &axis,
        );
        for s in 0..=(SPAN_SEMITONES as u8) {
            if offsets.contains(&s) {
                continue;
            }
            let x = self.pad_x(bounds, s);
            let mut tick = vg::Paint::default();
            tick.set_color(palette.ink_faint);
            tick.set_anti_alias(true);
            canvas.draw_path(
                &vg::Path::rect(vg::Rect::new(x - 0.5, mid_y + PAD_H / 2.0 + 6.0, x + 0.5, mid_y + PAD_H / 2.0 + 15.0), None),
                &tick,
            );
        }

        let label_font = crate::canvas_text::canvas_font(13.0);
        let sub_font = crate::canvas_text::canvas_font(9.0);
        let gap_font = crate::canvas_text::canvas_font(9.0);

        for (i, &offset) in offsets.iter().enumerate() {
            let x = self.pad_x(bounds, offset);
            let note = BASE_NOTE + key + offset;
            let is_root = offset % 12 == 0;
            let is_held = held.contains(&note);

            let rect = vg::Rect::new(x - PAD_W / 2.0, mid_y - PAD_H / 2.0, x + PAD_W / 2.0, mid_y + PAD_H / 2.0);

            let mut fill = vg::Paint::default();
            fill.set_color(if is_held { palette.signal } else { palette.bg_200 });
            fill.set_anti_alias(true);
            canvas.draw_path(&vg::Path::rect(rect, None), &fill);

            let mut border = vg::Paint::default();
            border.set_color(if is_root { palette.ink } else { palette.line_control });
            border.set_style(vg::PaintStyle::Stroke);
            border.set_stroke_width(if is_root { 2.0 } else { 1.0 });
            border.set_anti_alias(true);
            canvas.draw_path(&vg::Path::rect(rect, None), &border);

            let text_color = if is_held { palette.on_signal } else { palette.ink };
            let mut text_paint = vg::Paint::default();
            text_paint.set_color(text_color);
            text_paint.set_anti_alias(true);
            let degree_text = degree_name(offset % 12);
            let dx = x - (degree_text.len() as f32 * 3.2);
            canvas.draw_str(degree_text, vg::Point::new(dx, mid_y - 4.0), &label_font, &text_paint);

            let mut sub_paint = vg::Paint::default();
            sub_paint.set_color(if is_held { palette.on_signal } else { palette.ink_muted });
            sub_paint.set_anti_alias(true);
            let note_text = note_name((key + offset) % 12);
            let nx = x - (note_text.len() as f32 * 2.6);
            canvas.draw_str(note_text, vg::Point::new(nx, mid_y + 12.0), &sub_font, &sub_paint);

            if i + 1 < offsets.len() {
                let next_x = self.pad_x(bounds, offsets[i + 1]);
                let gap = offsets[i + 1] - offset;
                let mut gap_paint = vg::Paint::default();
                gap_paint.set_color(palette.ink_faint);
                gap_paint.set_anti_alias(true);
                let gap_text = gap.to_string();
                let gx = (x + next_x) / 2.0 - 3.0;
                canvas.draw_str(&gap_text, vg::Point::new(gx, mid_y + PAD_H / 2.0 + 28.0), &gap_font, &gap_paint);
            }
        }
    }
}
