//! The Effects Board: a full-width lower-panel view of a track's effect
//! graph - palette (left), canvas (middle, nodes at their own positions
//! wired by cables), inspector (right).
//!
//! Split into three pieces, same "one directory module per growing UI
//! area" convention `timeline/`, `synth/` and `piano_roll/` already use:
//! - `geometry`: pure layout math (node/port positions, latency), no
//!   Vizia dependency at all.
//! - `cables`: the canvas's custom-drawn layer (dot grid, cable curves,
//!   port dots, live wire-drag preview).
//! - this file: the actual widget tree and its interaction wiring
//!   (drag-to-move, drag-to-rewire, palette-to-add, select/delete).

mod cables;
mod geometry;

use vizia::prelude::*;

use shared::arrangement::{Arrangement, ClipColor, Effect, EffectGraph, EffectNodeId, TrackId};

use cables::FxCables;
use geometry::{effect_latency_ms, io_position, nearest_input_port, port_out, CANVAS_MARGIN_X, IO_H, IO_W, NODE_H, NODE_W, ROW_Y};

use crate::timeline::state::TimelineEvent;
use crate::tokens::{self, ThemeId};

#[derive(Clone, Copy)]
pub struct FxBoardProps {
    pub theme: Signal<ThemeId>,
    pub arrangement: Signal<Arrangement>,
    /// `Some(id)` for a track's board, `None` for the master bus's -
    /// the same convention `Arrangement::fx`/`fx_mut` use.
    pub track: Option<TrackId>,
    /// Cleared to close the board - the lower panel falls back to the
    /// device area. `Some(None)` is the master board open; `None` is
    /// no board open at all.
    pub board_open_track: Signal<Option<Option<TrackId>>>,
    /// The board's selected node - owned by `TimelineState` so Delete can
    /// remove it (see `TimelineState::fx_selected`).
    pub selected: Signal<Option<(Option<TrackId>, EffectNodeId)>>,
    /// Where automated knobs are shown at (see `effect_panel`).
    pub playhead: Signal<shared::arrangement::Ticks>,
}

/// One palette row: `bg-400` while its own effect is the one currently
/// being dragged (per the FxBoard spec); press-drag-release adds it.
/// Hidden while the search text doesn't match its name - same
/// `toggle_class("hidden", ...)` idiom `sidebar.rs`'s own search uses.
fn palette_row(cx: &mut Context, label: &'static str, effect: Effect, p: FxBoardProps, palette_drag: Signal<Option<Effect>>, query: Signal<String>) {
    let needle = label.to_lowercase();
    Label::new(cx, label)
        .class("side-row")
        .toggle_class("is-on", palette_drag.map(move |d| *d == Some(effect)))
        .toggle_class("hidden", query.map(move |q| !q.is_empty() && !needle.contains(&q.to_lowercase())))
        .cursor(CursorIcon::Hand)
        // Without an explicit width the Label only hit-tests its own text
        // glyphs - fine for "Compressor", nearly unclickable for "EQ".
        // `sidebar.rs`'s equivalent row helper stretches for the same
        // reason.
        .width(Stretch(1.0))
        .on_mouse_down(move |cx, button| {
            if button == MouseButton::Left {
                palette_drag.set(Some(effect));
                cx.capture();
            }
        })
        .on_mouse_up(move |cx, button| {
            if button == MouseButton::Left && palette_drag.get() == Some(effect) {
                cx.release();
                palette_drag.set(None);
                cx.emit(TimelineEvent::AddEffectNodeToBoard(p.track, effect, None));
            }
        });
}

/// A node's output port: drag from here to rewire - "wire this node's
/// output into another node's input" and "move this node to just
/// before that other node" are the same edit while the chain stays
/// linear (see `EffectGraph::move_before`'s doc comment).
fn output_port(cx: &mut Context, p: FxBoardProps, node: EffectNodeId, x: f32, y: f32, wire_drag: Signal<Option<(EffectNodeId, f32, f32)>>) {
    Element::new(cx)
        .class("fx-pip")
        .class("fx-port")
        .class("is-on")
        .position_type(PositionType::Absolute)
        .left(Pixels(x - 5.0))
        .top(Pixels(y - 5.0))
        .width(Pixels(10.0))
        .height(Pixels(10.0))
        .cursor(CursorIcon::Crosshair)
        .on_mouse_down(move |cx, button| {
            if button == MouseButton::Left {
                cx.capture();
                wire_drag.set(Some((node, x, y)));
            }
        })
        .on_mouse_move(move |cx, mx, my| {
            if wire_drag.get().is_some_and(|(id, ..)| id == node) {
                let bounds = cx.bounds();
                let origin_x = bounds.x - (x - 5.0);
                let origin_y = bounds.y - (y - 5.0);
                wire_drag.set(Some((node, mx - origin_x, my - origin_y)));
            }
        })
        .on_mouse_up(move |cx, button| {
            if button != MouseButton::Left {
                return;
            }
            let Some((id, local_x, local_y)) = wire_drag.get() else { return };
            if id != node {
                return;
            }
            cx.release();
            wire_drag.set(None);
            let arr = p.arrangement.get();
            let Some(fx) = arr.fx(p.track) else { return };
            if let Some(target) = nearest_input_port(fx, local_x, local_y) {
                if target != node {
                    cx.emit(TimelineEvent::RewireEffect(p.track, node, target));
                }
            }
        });
}

pub fn fx_board(cx: &mut Context, p: FxBoardProps) {
    let selected = p.selected.map(|s| s.map(|(_, node)| node));
    // Live drag preview: (node, x, y) while a node's being dragged, so
    // the node box and its cables can redraw without committing to the
    // model (and thus without an undo step) until the drag releases.
    let drag_state: Signal<Option<(EffectNodeId, f32, f32)>> = Signal::new(None);
    // (node, grab-x, grab-y, original-node-x, original-node-y) captured
    // on mouse-down, read on every subsequent move to compute the delta.
    let drag_anchor: Signal<Option<(EffectNodeId, f32, f32, f32, f32)>> = Signal::new(None);
    // (source node, live cursor x, live cursor y - both canvas-local)
    // while dragging a wire from that node's output port.
    let wire_drag: Signal<Option<(EffectNodeId, f32, f32)>> = Signal::new(None);
    let track_name = p.arrangement.map(move |arr| match p.track {
        Some(id) => arr.track(id).map(|t| t.name.clone()).unwrap_or_default(),
        None => "Master".to_string(),
    });
    let track_color = p.arrangement.map(move |arr| {
        p.track.and_then(|id| arr.track(id)).map(|t| t.color).unwrap_or(ClipColor::Violet)
    });
    let all_bypassed = p.arrangement.map(move |arr| {
        arr.fx(p.track)
            .map(|fx| {
                let nodes = fx.ordered();
                !nodes.is_empty() && nodes.iter().all(|n| !n.enabled)
            })
            .unwrap_or(false)
    });
    // Real, summed from each enabled node's own latency - not a placeholder.
    // Both effect types today (Compressor, EQ) are zero-latency (no
    // lookahead, no FIR delay), so this is currently always 0.0ms; it'll
    // start reporting something real the day a delay-based effect type
    // adds `EffectUnit::latency_samples()` (see the backlog's "Parallel
    // branches, engine side" note) without this readout needing to change.
    let latency_ms = p.arrangement.map(move |arr| {
        // `.max(0.0)` isn't just defensive - an empty `Iterator::sum`
        // over f32 can produce -0.0 (confirmed on this toolchain), which
        // `{:.1}` then prints as the nonsensical "-0.0 ms".
        arr.fx(p.track)
            .map(|fx| fx.ordered().iter().filter(|n| n.enabled).map(|n| effect_latency_ms(&n.effect)).sum::<f32>().max(0.0))
            .unwrap_or(0.0)
    });

    VStack::new(cx, move |cx| {
        // Header.
        HStack::new(cx, move |cx| {
            Element::new(cx)
                .class("swatch")
                .background_color(track_color.map(|c| crate::timeline::header::clip_color_to_rgb(*c)));
            Label::new(cx, track_name).class("title");
            Label::new(cx, if p.track.is_some() { "Effects" } else { "Main out" }).class("meta");
            Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(16.0));
            // Lit while every effect is bypassed, so it reads as the state
            // toggle it is rather than a one-shot action.
            Button::new(cx, |cx| Label::new(cx, "Bypass all"))
                .class("btn")
                .class("sm")
                .toggle_class("is-on", all_bypassed)
                .on_press(move |cx| cx.emit(TimelineEvent::SetChainBypassed(p.track, !all_bypassed.get())));
            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
            Label::new(cx, latency_ms.map(|ms| format!("Latency {ms:.1} ms"))).class("meta");
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
        // Which effect (if any) is being dragged from the palette right
        // now - drives the dragged row's bg-400 highlight, and on
        // release (wherever the cursor ends up) adds it to the chain.
        // Drop-point placement isn't tracked yet (it lands via the same
        // auto-layout push_at_end position "+Effect" already uses) -
        // the gesture itself is what this phase proves.
        let palette_drag: Signal<Option<Effect>> = Signal::new(None);
        let palette_query: Signal<String> = Signal::new(String::new());
        HStack::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                Textbox::new(cx, palette_query)
                    .placeholder("Search effects")
                    .on_edit(move |_cx, text| palette_query.set(text))
                    .class("search")
                    .width(Stretch(1.0));
                Label::new(cx, "Dynamics").class("side-head");
                palette_row(cx, "Compressor", Effect::Compressor(shared::arrangement::CompressorState::default()), p, palette_drag, palette_query);
                Label::new(cx, "EQ and filter").class("side-head");
                palette_row(cx, "EQ", Effect::Eq(shared::arrangement::EqState::default()), p, palette_drag, palette_query);
            })
            .class("panel")
            .gap(Pixels(tokens::SPACE_1))
            .padding(Pixels(tokens::SPACE_2))
            .width(Pixels(176.0))
            .height(Stretch(1.0));

            // Canvas.
            let search_popover: Signal<Option<(f32, f32)>> = Signal::new(None);
            ZStack::new(cx, move |cx| {
                FxCables::new(cx, p.arrangement, p.track, p.theme, selected, drag_state, wire_drag)
                    .width(Stretch(1.0))
                    .height(Stretch(1.0));

                Element::new(cx)
                    .width(Stretch(1.0))
                    .height(Stretch(1.0))
                    .on_double_click(move |cx, _| {
                        let bounds = cx.bounds();
                        let local = (cx.mouse().cursor_x - bounds.x, cx.mouse().cursor_y - bounds.y);
                        search_popover.set(Some(local));
                    });

                Binding::new(cx, p.arrangement, move |cx| {
                    let arr = p.arrangement.get();
                    let Some(graph) = arr.fx(p.track) else { return };
                    let source_meta = match p.track {
                        // An audio track has no instrument - its source is its clips.
                        Some(id) => arr.track(id).and_then(|t| t.instrument).map(|i| i.name()).unwrap_or("Audio clips"),
                        None => "Mix",
                    };

                    for id in [EffectGraph::SOURCE, EffectGraph::OUTPUT] {
                        let (x, y) = io_position(graph, id, None);
                        let label = if id == EffectGraph::SOURCE { "Source" } else { "Out" };
                        let meta = if id == EffectGraph::SOURCE {
                            source_meta
                        } else if p.track.is_some() {
                            "to master"
                        } else {
                            "Main out"
                        };
                        VStack::new(cx, move |cx| {
                            Label::new(cx, label).class("meta");
                            Label::new(cx, meta).class("meta");
                        })
                        .class("fx-node")
                        .position_type(PositionType::Absolute)
                        .left(Pixels(x))
                        .top(Pixels(y))
                        .width(Pixels(IO_W))
                        .height(Pixels(IO_H))
                        .alignment(Alignment::Center);
                    }

                    for node in graph.nodes.clone() {
                        // Display-space position (model position plus the
                        // canvas's left margin) - drag math below stays in
                        // this space throughout, then converts back to
                        // model-space only when committing/emitting.
                        let (orig_x, orig_y) = (node.position.0 + CANVAS_MARGIN_X, node.position.1 + ROW_Y);
                        let x = Memo::new(move |_| {
                            match drag_state.get() {
                                Some((id, x, _)) if id == node.id => x,
                                _ => orig_x,
                            }
                        });
                        let y = Memo::new(move |_| {
                            match drag_state.get() {
                                Some((id, _, y)) if id == node.id => y,
                                _ => orig_y,
                            }
                        });
                        let is_selected = selected.map(move |s| *s == Some(node.id));
                        VStack::new(cx, move |cx| {
                            HStack::new(cx, move |cx| {
                                Element::new(cx)
                                    .class("fx-pip")
                                    .toggle_class("is-on", node.enabled)
                                    .on_press(move |cx| cx.emit(TimelineEvent::ToggleEffectEnabled(p.track, node.id)));
                                Label::new(cx, node.effect.name()).class("title");
                                Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
                                Button::new(cx, |cx| Label::new(cx, "\u{2715}"))
                                    .class("btn")
                                    .class("quiet")
                                    .on_press(move |cx| {
                                        if selected.get() == Some(node.id) {
                                            p.selected.set(None);
                                        }
                                        cx.emit(TimelineEvent::RemoveEffectNodeFromBoard(p.track, node.id));
                                    });
                            })
                            .class("hd")
                            .alignment(Alignment::Left)
                            .gap(Pixels(4.0))
                            .height(Pixels(22.0))
                            .width(Stretch(1.0));

                            Element::new(cx).class("hairline").width(Stretch(1.0)).height(Pixels(1.0));

                            match node.effect {
                                Effect::Eq(state) => {
                                    let state_signal = Memo::new(move |_| state);
                                    crate::eq_curve::EqCurve::new(cx, state_signal, p.theme).class("device").width(Stretch(1.0)).height(Stretch(1.0));
                                    Label::new(cx, format!("{:.0} Hz \u{b7} {:+.1} dB", state.freq_hz, state.gain_db)).class("meta");
                                }
                                Effect::Compressor(state) => {
                                    let state_signal = Memo::new(move |_| state);
                                    crate::compressor_curve::CompressorCurve::new(cx, state_signal, p.theme).class("device").width(Stretch(1.0)).height(Stretch(1.0));
                                    Label::new(cx, format!("{:.0}:1 \u{b7} {:+.1} dB", state.ratio, state.threshold_db)).class("meta");
                                }
                            }
                        })
                        .class("fx-node")
                        .class("fx-effect")
                        .toggle_class("is-sel", is_selected)
                        // Bypassed nodes dim, so the chain's state reads at
                        // a glance, not just from the 6px pip.
                        .toggle_class("is-off", !node.enabled)
                        .position_type(PositionType::Absolute)
                        .left(x.map(|v| Pixels(*v)))
                        .top(y.map(|v| Pixels(*v)))
                        .width(Pixels(NODE_W))
                        .height(Pixels(NODE_H))
                        .padding(Pixels(6.0))
                        // The mini-display stretches into whatever height
                        // the title row and meta line leave, so the node's
                        // content can't overflow its box.
                        .gap(Pixels(4.0))
                        .cursor(CursorIcon::Hand)
                        .on_mouse_down(move |cx, button| {
                            if button == MouseButton::Left {
                                p.selected.set(Some((p.track, node.id)));
                                // One selection at a time, so Delete acts
                                // on the node, not clips picked earlier.
                                cx.emit(TimelineEvent::ClearSelection);
                                cx.capture();
                                drag_anchor.set(Some((
                                    node.id,
                                    cx.mouse().cursor_x,
                                    cx.mouse().cursor_y,
                                    orig_x,
                                    orig_y,
                                )));
                                drag_state.set(Some((node.id, orig_x, orig_y)));
                            }
                        })
                        .on_mouse_move(move |_cx, mx, my| {
                            if let Some((id, anchor_mx, anchor_my, base_x, base_y)) = drag_anchor.get() {
                                if id == node.id {
                                    let new_x = base_x + (mx - anchor_mx);
                                    let new_y = base_y + (my - anchor_my);
                                    drag_state.set(Some((id, new_x, new_y)));
                                }
                            }
                        })
                        .on_mouse_up(move |cx, button| {
                            if button != MouseButton::Left {
                                return;
                            }
                            if let Some((id, _, _, _, _)) = drag_anchor.get() {
                                if id == node.id {
                                    cx.release();
                                    drag_anchor.set(None);
                                    if let Some((_, x, y)) = drag_state.get() {
                                        // Back to model-space (strip the
                                        // canvas margins) before snapping to
                                        // the 16px dot grid.
                                        let snapped = (((x - CANVAS_MARGIN_X) / 16.0).round() * 16.0, ((y - ROW_Y) / 16.0).round() * 16.0);
                                        cx.emit(TimelineEvent::SetEffectNodePosition(p.track, node.id, snapped));
                                    }
                                    drag_state.set(None);
                                }
                            }
                        });

                        let (out_x, out_y) = port_out(graph, node.id, drag_state.get());
                        output_port(cx, p, node.id, out_x, out_y, wire_drag);
                    }
                });

                // Double-click empty canvas: a small search popover to
                // append an effect, same absolute-positioned/backdrop
                // convention as the timeline's own context menu.
                Element::new(cx)
                    .class("context-menu-backdrop")
                    .toggle_class("hidden", search_popover.map(|p| p.is_none()))
                    .on_mouse_down(move |_cx, _| search_popover.set(None))
                    .position_type(PositionType::Absolute)
                    .top(Pixels(0.0))
                    .left(Pixels(0.0))
                    .width(Stretch(1.0))
                    .height(Stretch(1.0));
                Binding::new(cx, search_popover, move |cx| {
                    let Some((x, y)) = search_popover.get() else { return };
                    VStack::new(cx, move |cx| {
                        for (label, effect) in [
                            ("Compressor", Effect::Compressor(shared::arrangement::CompressorState::default())),
                            ("EQ", Effect::Eq(shared::arrangement::EqState::default())),
                        ] {
                            Label::new(cx, label).class("menu-item").class("body").on_press(move |cx| {
                                // `(x, y)` is the double-click's canvas-local
                                // (display-space) point - strip the margin
                                // before storing it as the node's model-space
                                // position.
                                cx.emit(TimelineEvent::AddEffectNodeToBoard(p.track, effect, Some((x - CANVAS_MARGIN_X, y - ROW_Y))));
                                search_popover.set(None);
                            });
                        }
                    })
                    .class("panel")
                    .class("context-menu")
                    .position_type(PositionType::Absolute)
                    .left(Pixels(x))
                    .top(Pixels(y))
                    .width(Pixels(120.0));
                });
            })
            .class("fx-canvas")
            .width(Stretch(1.0))
            .height(Stretch(1.0));

            // Inspector: the selected node's real panel, reusing
            // the same effect_panel device_area uses, for exactly this node.
            VStack::new(cx, move |cx| {
                Binding::new(cx, selected, move |cx| {
                    let Some(node_id) = selected.get() else {
                        Label::new(cx, "No device selected").class("meta");
                        return;
                    };
                    let arr = p.arrangement.get();
                    let Some(fx) = arr.fx(p.track) else { return };
                    let Some(node) = fx.node(node_id) else { return };
                    let color = p.track.and_then(|id| arr.track(id)).map(|t| t.color).unwrap_or(ClipColor::Violet);
                    crate::effect_panel::effect_panel(cx, p.theme, p.arrangement, p.track, node.id, color, p.playhead);
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
