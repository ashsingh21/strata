//! Panel B - "Lattice": a Tonnetz. Moving one step right is a perfect 5th
//! (+7 semitones), one step up-right is a major 3rd (+4); the third edge
//! of any small triangle is then automatically a minor 3rd (+3) - so
//! every small triangle in the grid is a triad, alternating major
//! (pointing up) and minor (pointing down) as you'd expect from a
//! Tonnetz.

use vizia::prelude::*;
use vizia::vg;

use shared::synth::SynthState;
use shared::theory::{degree_name, note_name};

use crate::tokens::ThemeId;

const BASE_NOTE: u8 = 48;
const COL_W: f32 = 62.0;
const ROW_H: f32 = 54.0;
const NODE_R: f32 = 15.0;
const I_RANGE: std::ops::RangeInclusive<i32> = -1..=6;
const J_RANGE: std::ops::RangeInclusive<i32> = -1..=2;

fn rel_pc(i: i32, j: i32) -> u8 {
    (7 * i + 4 * j).rem_euclid(12) as u8
}

pub struct Lattice {
    synth_state: Signal<SynthState>,
    theme: Signal<ThemeId>,
    key: Signal<u8>,
    scale_mask: Signal<u16>,
}

impl Lattice {
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

    fn node_pos(&self, bounds: BoundingBox, i: i32, j: i32) -> vg::Point {
        let origin_x = bounds.x + 40.0;
        let origin_y = bounds.y + bounds.h * 0.55;
        vg::Point::new(origin_x + i as f32 * COL_W + j as f32 * (COL_W * 0.5), origin_y - j as f32 * ROW_H)
    }
}

impl View for Lattice {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, _| {
            if let WindowEvent::MouseDown(MouseButton::Left) = window_event {
                let bounds = cx.bounds();
                let mx = cx.mouse().cursor_x;
                let my = cx.mouse().cursor_y;
                let key = self.key.get();
                for i in I_RANGE {
                    for j in J_RANGE {
                        let p = self.node_pos(bounds, i, j);
                        if (mx - p.x).powi(2) + (my - p.y).powi(2) <= NODE_R * NODE_R {
                            let note = BASE_NOTE + key + rel_pc(i, j);
                            cx.emit(crate::synth::state::SynthEvent::ToggleKey(note));
                            return;
                        }
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
        let in_scale = |rel: u8| mask & (1 << rel) != 0;

        // Triangles first, so node markers draw on top of the fill.
        let mut tri_paint = vg::Paint::default();
        tri_paint.set_color(palette.volt_soft);
        tri_paint.set_anti_alias(true);
        for i in *I_RANGE.start()..*I_RANGE.end() {
            for j in *J_RANGE.start()..*J_RANGE.end() {
                let up = [(i, j), (i + 1, j), (i, j + 1)];
                let down = [(i + 1, j), (i, j + 1), (i + 1, j + 1)];
                for tri in [up, down] {
                    if tri.iter().all(|&(ti, tj)| is_held(rel_pc(ti, tj))) {
                        let pts: Vec<vg::Point> = tri.iter().map(|&(ti, tj)| self.node_pos(bounds, ti, tj)).collect();
                        let mut path = vg::PathBuilder::new();
                        path.move_to(pts[0]);
                        path.line_to(pts[1]);
                        path.line_to(pts[2]);
                        path.close();
                        canvas.draw_path(&path.detach(), &tri_paint);
                    }
                }
            }
        }

        let label_font = crate::canvas_text::canvas_font(11.0);
        let sub_font = crate::canvas_text::canvas_font(8.0);

        for i in I_RANGE {
            for j in J_RANGE {
                let p = self.node_pos(bounds, i, j);
                if p.x < bounds.x - NODE_R || p.x > bounds.x + bounds.w + NODE_R {
                    continue;
                }
                let rel = rel_pc(i, j);
                let held_here = is_held(rel);
                let scale_member = in_scale(rel);
                let is_root = rel == 0;

                if !scale_member {
                    let mut dot = vg::Paint::default();
                    dot.set_color(palette.ink_faint);
                    dot.set_anti_alias(true);
                    canvas.draw_circle(p, 2.5, &dot);
                    continue;
                }

                let mut fill = vg::Paint::default();
                fill.set_color(if held_here { palette.volt } else { palette.bg_200 });
                fill.set_anti_alias(true);
                canvas.draw_circle(p, NODE_R, &fill);

                let mut border = vg::Paint::default();
                border.set_color(if is_root { palette.ink } else { palette.line_control });
                border.set_style(vg::PaintStyle::Stroke);
                border.set_stroke_width(if is_root { 2.0 } else { 1.0 });
                border.set_anti_alias(true);
                canvas.draw_circle(p, NODE_R, &border);

                let text_color = if held_here { palette.on_volt } else { palette.ink };
                let mut text_paint = vg::Paint::default();
                text_paint.set_color(text_color);
                text_paint.set_anti_alias(true);
                let degree_text = degree_name(rel);
                canvas.draw_str(
                    degree_text,
                    vg::Point::new(p.x - degree_text.len() as f32 * 3.0, p.y - 1.0),
                    &label_font,
                    &text_paint,
                );

                let mut sub_paint = vg::Paint::default();
                sub_paint.set_color(if held_here { palette.on_volt } else { palette.ink_muted });
                sub_paint.set_anti_alias(true);
                let note_text = note_name((key + rel) % 12);
                canvas.draw_str(
                    note_text,
                    vg::Point::new(p.x - note_text.len() as f32 * 2.4, p.y + 10.0),
                    &sub_font,
                    &sub_paint,
                );
            }
        }
    }
}
