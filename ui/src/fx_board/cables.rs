//! Draws everything on the Effects Board's canvas that isn't a Vizia
//! widget: the dot grid, every cable in the graph as a cubic-bezier curve
//! between its two ports, the port dots themselves, and a live wire being
//! dragged from a port to the cursor. The selected node's own cables draw
//! heavier/brighter, matching the FxBoard spec.

use vizia::prelude::*;
use vizia::vg;

use shared::arrangement::{Arrangement, EffectGraph, EffectNodeId, TrackId};

use super::geometry::{port_in, port_out};
use crate::tokens::ThemeId;

pub struct FxCables {
    arrangement: Signal<Arrangement>,
    track: Option<TrackId>,
    theme: Signal<ThemeId>,
    selected: Memo<Option<EffectNodeId>>,
    drag_state: Signal<Option<(EffectNodeId, f32, f32)>>,
    wire_drag: Signal<Option<(EffectNodeId, f32, f32)>>,
}

impl FxCables {
    pub fn new(
        cx: &mut Context,
        arrangement: Signal<Arrangement>,
        track: Option<TrackId>,
        theme: Signal<ThemeId>,
        selected: Memo<Option<EffectNodeId>>,
        drag_state: Signal<Option<(EffectNodeId, f32, f32)>>,
        wire_drag: Signal<Option<(EffectNodeId, f32, f32)>>,
    ) -> Handle<'_, Self> {
        Self { arrangement, track, theme, selected, drag_state, wire_drag }
            .build(cx, |_| {})
            .bind(arrangement, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
            .bind(selected, |mut h| h.needs_redraw())
            .bind(drag_state, |mut h| h.needs_redraw())
            .bind(wire_drag, |mut h| h.needs_redraw())
    }
}

impl View for FxCables {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();

        // A dot grid at the same 16px spacing nodes snap to on drag-release
        // - a working graph-paper surface, not just decoration.
        let mut grid_dot = vg::Paint::default();
        grid_dot.set_color(palette.ink_faint);
        grid_dot.set_anti_alias(true);
        let grid_step = 16.0f32;
        let mut gy = bounds.y;
        while gy < bounds.y + bounds.h {
            let mut gx = bounds.x;
            while gx < bounds.x + bounds.w {
                canvas.draw_path(&vg::Path::circle(vg::Point::new(gx, gy), 0.75, None), &grid_dot);
                gx += grid_step;
            }
            gy += grid_step;
        }

        let arr = self.arrangement.get();
        let Some(graph) = arr.fx(self.track) else { return };
        let selected = self.selected.get();
        let drag = self.drag_state.get();

        for edge in &graph.edges {
            let (x0, y0) = port_out(graph, edge.from, drag);
            let (x1, y1) = port_in(graph, edge.to, drag);
            let (x0, y0, x1, y1) = (bounds.x + x0, bounds.y + y0, bounds.x + x1, bounds.y + y1);
            let mid = (x0 + x1) * 0.5;

            let mut path = vg::PathBuilder::new();
            path.move_to(vg::Point::new(x0, y0));
            path.cubic_to(vg::Point::new(mid, y0), vg::Point::new(mid, y1), vg::Point::new(x1, y1));
            let path = path.detach();

            let is_selected = selected.is_some_and(|s| s == edge.from || s == edge.to);
            let mut paint = vg::Paint::default();
            paint.set_color(if is_selected { palette.ink } else { palette.ink_muted });
            paint.set_style(vg::PaintStyle::Stroke);
            paint.set_stroke_width(if is_selected { 2.0 } else { 1.5 });
            paint.set_anti_alias(true);
            canvas.draw_path(&path, &paint);
        }

        // Ports: 10px ink dots at every real node's in/out (source only
        // has an out, output only an in).
        let mut dot = vg::Paint::default();
        dot.set_color(palette.ink);
        dot.set_anti_alias(true);
        let draw_dot = |x: f32, y: f32| {
            canvas.draw_path(&vg::Path::circle(vg::Point::new(bounds.x + x, bounds.y + y), 3.0, None), &dot);
        };
        let (sx, sy) = port_out(graph, EffectGraph::SOURCE, drag);
        draw_dot(sx, sy);
        let (ox, oy) = port_in(graph, EffectGraph::OUTPUT, drag);
        draw_dot(ox, oy);
        for node in &graph.nodes {
            let (ox, oy) = port_out(graph, node.id, drag);
            let (ix, iy) = port_in(graph, node.id, drag);
            draw_dot(ox, oy);
            draw_dot(ix, iy);
        }

        // A live wire being dragged from a port to the cursor.
        if let Some((from, mx, my)) = self.wire_drag.get() {
            let (x0, y0) = port_out(graph, from, drag);
            let (x0, y0) = (bounds.x + x0, bounds.y + y0);
            let (x1, y1) = (bounds.x + mx, bounds.y + my);
            let mid = (x0 + x1) * 0.5;
            let mut path = vg::PathBuilder::new();
            path.move_to(vg::Point::new(x0, y0));
            path.cubic_to(vg::Point::new(mid, y0), vg::Point::new(mid, y1), vg::Point::new(x1, y1));
            let mut paint = vg::Paint::default();
            paint.set_color(palette.ink);
            paint.set_style(vg::PaintStyle::Stroke);
            paint.set_stroke_width(2.0);
            paint.set_anti_alias(true);
            canvas.draw_path(&path.detach(), &paint);
        }
    }
}
