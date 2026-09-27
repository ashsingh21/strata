//! The Effects Board: a full-width lower-panel view of a track's effect
//! graph - palette (left), canvas (middle, nodes at their own positions
//! wired by cables), inspector (right). Phase 5 of the effects-board
//! plan: renders the real graph, but read-only - no drag, no wiring, no
//! palette-drop yet (Phases 6-9).

use vizia::prelude::*;
use vizia::vg;

use shared::arrangement::{Arrangement, ClipColor, Effect, EffectGraph, EffectNodeId, TrackId};

use crate::timeline::state::TimelineEvent;
use crate::tokens::{self, ThemeId};

/// A node box's fixed size (per the FxBoard spec: effect nodes are
/// 150x86; source/output are smaller, 88x40).
const NODE_W: f32 = 150.0;
const NODE_H: f32 = 86.0;
const IO_W: f32 = 88.0;
const IO_H: f32 = 40.0;
/// Horizontal spacing between auto-laid-out nodes - matches
/// `EffectGraph::push_at_end`'s own spacing so a freshly added node
/// lines up with this board's layout instead of drifting from it.
const NODE_SPACING: f32 = 166.0;
const ROW_Y: f32 = 60.0;

#[derive(Clone, Copy)]
pub struct FxBoardProps {
    pub theme: Signal<ThemeId>,
    pub arrangement: Signal<Arrangement>,
    pub track: TrackId,
    /// Cleared (`None`) to close the board - the lower panel falls back
    /// to the device area.
    pub board_open_track: Signal<Option<TrackId>>,
}

fn io_position(graph: &EffectGraph, id: EffectNodeId) -> (f32, f32) {
    if id == EffectGraph::SOURCE {
        return (-NODE_SPACING, ROW_Y + (NODE_H - IO_H) * 0.5);
    }
    if id == EffectGraph::OUTPUT {
        let max_x = graph.nodes.iter().map(|n| n.position.0).fold(0.0f32, f32::max);
        let x = if graph.nodes.is_empty() { 0.0 } else { max_x + NODE_SPACING };
        return (x, ROW_Y + (NODE_H - IO_H) * 0.5);
    }
    graph.node(id).map(|n| n.position).unwrap_or((0.0, ROW_Y))
}

fn port_out(graph: &EffectGraph, id: EffectNodeId) -> (f32, f32) {
    let (x, y) = io_position(graph, id);
    let (w, h) = node_size(id);
    (x + w, y + h * 0.5)
}

fn port_in(graph: &EffectGraph, id: EffectNodeId) -> (f32, f32) {
    let (x, y) = io_position(graph, id);
    let (_, h) = node_size(id);
    (x, y + h * 0.5)
}

fn node_size(id: EffectNodeId) -> (f32, f32) {
    if id == EffectGraph::SOURCE || id == EffectGraph::OUTPUT {
        (IO_W, IO_H)
    } else {
        (NODE_W, NODE_H)
    }
}

/// Draws every cable in the graph as a cubic-bezier curve between its
/// two ports - the selected node's own cables draw heavier/brighter,
/// matching the FxBoard spec.
struct FxCables {
    arrangement: Signal<Arrangement>,
    track: TrackId,
    theme: Signal<ThemeId>,
    selected: Signal<Option<EffectNodeId>>,
}

impl FxCables {
    fn new(
        cx: &mut Context,
        arrangement: Signal<Arrangement>,
        track: TrackId,
        theme: Signal<ThemeId>,
        selected: Signal<Option<EffectNodeId>>,
    ) -> Handle<'_, Self> {
        Self { arrangement, track, theme, selected }
            .build(cx, |_| {})
            .bind(arrangement, |mut h| h.needs_redraw())
            .bind(theme, |mut h| h.needs_redraw())
            .bind(selected, |mut h| h.needs_redraw())
    }
}

impl View for FxCables {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();
        let arr = self.arrangement.get();
        let Some(track) = arr.track(self.track) else { return };
        let graph = &track.fx;
        let selected = self.selected.get();

        for edge in &graph.edges {
            let (x0, y0) = port_out(graph, edge.from);
            let (x1, y1) = port_in(graph, edge.to);
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
        draw_dot(port_out(graph, EffectGraph::SOURCE).0, port_out(graph, EffectGraph::SOURCE).1);
        draw_dot(port_in(graph, EffectGraph::OUTPUT).0, port_in(graph, EffectGraph::OUTPUT).1);
        for node in &graph.nodes {
            let (ox, oy) = port_out(graph, node.id);
            let (ix, iy) = port_in(graph, node.id);
            draw_dot(ox, oy);
            draw_dot(ix, iy);
        }
    }
}

pub fn fx_board(cx: &mut Context, p: FxBoardProps) {
    let selected: Signal<Option<EffectNodeId>> = Signal::new(None);
    let track_name = p.arrangement.map(move |arr| arr.track(p.track).map(|t| t.name.clone()).unwrap_or_default());
    let track_color = p.arrangement.map(move |arr| arr.track(p.track).map(|t| t.color).unwrap_or(ClipColor::Violet));
    let all_bypassed = p.arrangement.map(move |arr| {
        arr.track(p.track)
            .map(|t| {
                let nodes = t.fx.ordered();
                !nodes.is_empty() && nodes.iter().all(|n| !n.enabled)
            })
            .unwrap_or(false)
    });

    VStack::new(cx, move |cx| {
        // Header.
        HStack::new(cx, move |cx| {
            Element::new(cx)
                .class("swatch")
                .background_color(track_color.map(|c| crate::timeline::header::clip_color_to_rgb(*c)));
            Label::new(cx, track_name).class("title");
            Label::new(cx, "Effects").class("meta");
            Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(16.0));
            Button::new(cx, |cx| Label::new(cx, "Bypass all")).class("btn").class("sm").on_press(move |cx| {
                cx.emit(TimelineEvent::SetChainBypassed(p.track, !all_bypassed.get()));
            });
            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
            Label::new(cx, "Latency 0.0 ms").class("meta");
            Label::new(cx, "CPU 0%").class("meta");
            Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(16.0));
            Label::new(cx, "100%").class("readout").class("sm");
            Button::new(cx, |cx| Label::new(cx, "\u{2715}")).class("btn").class("sm").class("quiet").on_press(move |_cx| {
                p.board_open_track.set(None);
            });
        })
        .class("transport")
        .gap(Pixels(tokens::SPACE_3))
        .padding(Pixels(tokens::SPACE_2))
        .alignment(Alignment::Left)
        .width(Stretch(1.0))
        .height(Pixels(tokens::SIZE_TOOLBAR));

        // Body: palette | canvas | inspector.
        HStack::new(cx, move |cx| {
            // Palette (static list - dragging is Phase 8).
            VStack::new(cx, move |cx| {
                Textbox::new(cx, Signal::new(String::new())).class("search").width(Stretch(1.0));
                Label::new(cx, "Dynamics").class("side-head");
                Label::new(cx, "Compressor").class("side-row");
                Label::new(cx, "EQ and filter").class("side-head");
                Label::new(cx, "EQ").class("side-row");
            })
            .class("panel")
            .gap(Pixels(tokens::SPACE_1))
            .padding(Pixels(tokens::SPACE_2))
            .width(Pixels(176.0))
            .height(Stretch(1.0));

            // Canvas.
            ZStack::new(cx, move |cx| {
                FxCables::new(cx, p.arrangement, p.track, p.theme, selected).width(Stretch(1.0)).height(Stretch(1.0));

                Binding::new(cx, p.arrangement, move |cx| {
                    let arr = p.arrangement.get();
                    let Some(track) = arr.track(p.track) else { return };
                    let graph = &track.fx;

                    for id in [EffectGraph::SOURCE, EffectGraph::OUTPUT] {
                        let (x, y) = io_position(graph, id);
                        let label = if id == EffectGraph::SOURCE { "Source" } else { "Out" };
                        let meta = if id == EffectGraph::SOURCE { track.instrument.map(|i| i.name()).unwrap_or("\u{2014}") } else { "to mixer" };
                        VStack::new(cx, move |cx| {
                            Label::new(cx, label).class("meta");
                            Label::new(cx, meta).class("meta");
                        })
                        .class("device")
                        .position_type(PositionType::Absolute)
                        .left(Pixels(x))
                        .top(Pixels(y))
                        .width(Pixels(IO_W))
                        .height(Pixels(IO_H))
                        .alignment(Alignment::Center);
                    }

                    for node in graph.nodes.clone() {
                        let (x, y) = node.position;
                        let is_selected = selected.map(move |s| *s == Some(node.id));
                        VStack::new(cx, move |cx| {
                            HStack::new(cx, move |cx| {
                                Element::new(cx)
                                    .class("fx-pip")
                                    .toggle_class("is-on", node.enabled)
                                    .on_press(move |cx| cx.emit(TimelineEvent::ToggleEffectEnabled(p.track, node.id)));
                                Label::new(cx, node.effect.name()).class("title");
                            })
                            .class("hd")
                            .alignment(Alignment::Left)
                            .gap(Pixels(4.0))
                            .height(Pixels(22.0))
                            .width(Stretch(1.0));

                            match node.effect {
                                Effect::Eq(state) => {
                                    let state_signal = Memo::new(move |_| state);
                                    crate::eq_curve::eq_curve(cx, state_signal, p.theme);
                                    Label::new(cx, format!("{:.0} Hz \u{b7} {:+.1} dB", state.freq_hz, state.gain_db)).class("meta");
                                }
                                Effect::Compressor(state) => {
                                    Element::new(cx).class("device").width(Stretch(1.0)).height(Pixels(36.0));
                                    Label::new(cx, format!("{:.0}:1 \u{b7} {:+.1} dB", state.ratio, state.threshold_db)).class("meta");
                                }
                            }
                        })
                        .class("panel")
                        .toggle_class("is-sel", is_selected)
                        .position_type(PositionType::Absolute)
                        .left(Pixels(x))
                        .top(Pixels(y))
                        .width(Pixels(NODE_W))
                        .height(Pixels(NODE_H))
                        .padding(Pixels(4.0))
                        .gap(Pixels(4.0))
                        .cursor(CursorIcon::Hand)
                        .on_press(move |cx| {
                            selected.set(Some(node.id));
                            let _ = cx;
                        });
                    }
                });
            })
            .class("device")
            .width(Stretch(1.0))
            .height(Stretch(1.0));

            // Inspector: the selected node's real panel, reusing
            // compressor_panel/eq_panel exactly as device_area does.
            VStack::new(cx, move |cx| {
                Binding::new(cx, selected, move |cx| {
                    let Some(node_id) = selected.get() else {
                        Label::new(cx, "No device selected").class("meta");
                        return;
                    };
                    let arr = p.arrangement.get();
                    let Some(track) = arr.track(p.track) else { return };
                    let Some(node) = track.fx.node(node_id) else { return };
                    match node.effect {
                        Effect::Compressor(_) => {
                            crate::compressor_panel::compressor_panel(cx, p.theme, p.arrangement, p.track, track.color)
                        }
                        Effect::Eq(_) => crate::eq_panel::eq_panel(cx, p.theme, p.arrangement, p.track, track.color),
                    }
                });
            })
            .class("panel")
            .padding(Pixels(tokens::SPACE_2))
            .width(Pixels(248.0))
            .height(Stretch(1.0));
        })
        .width(Stretch(1.0))
        .height(Stretch(1.0));
    })
    .class("panel")
    .width(Stretch(1.0))
    .height(Pixels(320.0));
}
