//! Panel C - "Ring": the 12 chromatic pitch classes arranged in a circle,
//! root at the top. Tapping a pad stacks or unstacks it; the currently
//! held pads are connected into the polygon that is their chord shape,
//! with the recognized chord name in the centre.

use vizia::prelude::*;
use vizia::vg;

use shared::synth::SynthState;
use shared::theory::{note_name, recognize};

use crate::tokens::ThemeId;

const BASE_NOTE: u8 = 48;
const NODE_R: f32 = 16.0;

fn angle_of(rel: u8) -> f32 {
    // Root at the top (12 o'clock), clockwise.
    (rel as f32) * (std::f32::consts::TAU / 12.0) - std::f32::consts::FRAC_PI_2
}

pub struct Ring {
    synth_state: Signal<SynthState>,
    theme: Signal<ThemeId>,
    key: Signal<u8>,
    scale_mask: Signal<u16>,
}

impl Ring {
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

    fn center_radius(&self, bounds: BoundingBox) -> (vg::Point, f32) {
        let cx = bounds.x + bounds.w * 0.5;
        let cy = bounds.y + bounds.h * 0.5;
        let radius = (bounds.w.min(bounds.h) * 0.5 - NODE_R - 4.0).max(10.0);
        (vg::Point::new(cx, cy), radius)
    }

    fn node_pos(&self, bounds: BoundingBox, rel: u8) -> vg::Point {
        let (center, radius) = self.center_radius(bounds);
        let a = angle_of(rel);
        vg::Point::new(center.x + radius * a.cos(), center.y + radius * a.sin())
    }
}

impl View for Ring {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, _| {
            if let WindowEvent::MouseDown(MouseButton::Left) = window_event {
                let bounds = cx.bounds();
                let mx = cx.mouse().cursor_x;
                let my = cx.mouse().cursor_y;
                let key = self.key.get();
                for rel in 0u8..12 {
                    let p = self.node_pos(bounds, rel);
                    if (mx - p.x).powi(2) + (my - p.y).powi(2) <= NODE_R * NODE_R {
                        let note = BASE_NOTE + key + rel;
                        cx.emit(crate::synth::state::SynthEvent::ToggleKey(note));
                        return;
                    }
                }
            }
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();
        let key = self.key.get();
        let mask = self.scale_mask.get();
        let held = self.synth_state.get().held_notes.clone();
        let is_held = |rel: u8| held.contains(&(BASE_NOTE + key + rel));

        // Track ring: a faint circle through every node position.
        let (center, radius) = self.center_radius(bounds);
        let mut track = vg::Paint::default();
        track.set_color(palette.line);
        track.set_style(vg::PaintStyle::Stroke);
        track.set_stroke_width(1.0);
        track.set_anti_alias(true);
        canvas.draw_circle(center, radius, &track);

        // The chord shape: held pads connected in ring order.
        let held_rels: Vec<u8> = (0u8..12).filter(|&r| is_held(r)).collect();
        if held_rels.len() >= 2 {
            let pts: Vec<vg::Point> = held_rels.iter().map(|&r| self.node_pos(bounds, r)).collect();
            let mut path = vg::PathBuilder::new();
            path.move_to(pts[0]);
            for p in &pts[1..] {
                path.line_to(*p);
            }
            path.close();
            let mut fill = vg::Paint::default();
            fill.set_color(palette.signal_soft);
            fill.set_anti_alias(true);
            canvas.draw_path(&path.detach(), &fill);
            let mut stroke = vg::Paint::default();
            stroke.set_color(palette.signal);
            stroke.set_style(vg::PaintStyle::Stroke);
            stroke.set_stroke_width(1.5);
            stroke.set_anti_alias(true);
            canvas.draw_path(&path.detach(), &stroke);
        }

        let label_font = crate::canvas_text::canvas_font(12.0);
        for rel in 0u8..12 {
            let p = self.node_pos(bounds, rel);
            let scale_member = mask & (1 << rel) != 0;
            let held_here = is_held(rel);
            let is_root = rel == 0;

            if !scale_member {
                let mut dot = vg::Paint::default();
                dot.set_color(palette.ink_faint);
                dot.set_anti_alias(true);
                canvas.draw_circle(p, 3.0, &dot);
                continue;
            }

            let mut fill = vg::Paint::default();
            fill.set_color(if held_here { palette.signal } else { palette.bg_200 });
            fill.set_anti_alias(true);
            canvas.draw_circle(p, NODE_R, &fill);

            let mut border = vg::Paint::default();
            border.set_color(if is_root { palette.ink } else { palette.line_control });
            border.set_style(vg::PaintStyle::Stroke);
            border.set_stroke_width(if is_root { 2.0 } else { 1.0 });
            border.set_anti_alias(true);
            canvas.draw_circle(p, NODE_R, &border);

            let text_color = if held_here { palette.on_signal } else { palette.ink };
            let mut text_paint = vg::Paint::default();
            text_paint.set_color(text_color);
            text_paint.set_anti_alias(true);
            let text = shared::theory::degree_name(rel);
            canvas.draw_str(text, vg::Point::new(p.x - text.len() as f32 * 3.2, p.y + 4.0), &label_font, &text_paint);
        }

        // Centre readout: the chord this ring is currently forming.
        let held_notes: Vec<u8> = held_rels.iter().map(|&r| BASE_NOTE + key + r).collect();
        let name_font = crate::canvas_text::canvas_font(20.0);
        let formula_font = crate::canvas_text::canvas_font(10.0);
        let mut name_paint = vg::Paint::default();
        name_paint.set_color(palette.ink);
        name_paint.set_anti_alias(true);
        let mut formula_paint = vg::Paint::default();
        formula_paint.set_color(palette.ink_muted);
        formula_paint.set_anti_alias(true);

        match recognize(&held_notes) {
            Some(chord) => {
                canvas.draw_str(
                    &chord.name,
                    vg::Point::new(center.x - chord.name.len() as f32 * 5.5, center.y + 4.0),
                    &name_font,
                    &name_paint,
                );
                canvas.draw_str(
                    &chord.formula,
                    vg::Point::new(center.x - chord.formula.len() as f32 * 2.6, center.y + 20.0),
                    &formula_font,
                    &formula_paint,
                );
            }
            None if !held_notes.is_empty() => {
                let text = note_name((key + held_rels[0]) % 12);
                canvas.draw_str(
                    text,
                    vg::Point::new(center.x - text.len() as f32 * 5.5, center.y + 4.0),
                    &name_font,
                    &name_paint,
                );
            }
            None => {}
        }
    }
}
