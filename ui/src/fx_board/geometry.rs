//! Pure layout math for the Effects Board: where a node/port sits, which
//! port a wire-drag release landed nearest, and an effect's processing
//! latency. No Vizia dependency at all - this is "what goes where and
//! why," kept separate from `mod.rs`'s "how it's actually drawn/wired
//! up" and `cables.rs`'s "how the cables/grid/ports get painted."

use shared::arrangement::{Effect, EffectGraph, EffectNodeId};

/// A node box's fixed size (per the FxBoard spec: effect nodes are
/// 150x86; source/output are smaller, 88x40).
pub const NODE_W: f32 = 150.0;
pub const NODE_H: f32 = 86.0;
pub const IO_W: f32 = 88.0;
pub const IO_H: f32 = 40.0;
/// Horizontal spacing between auto-laid-out nodes - matches
/// `EffectGraph::push_at_end`'s own spacing so a freshly added node
/// lines up with this board's layout instead of drifting from it.
const NODE_SPACING: f32 = 166.0;
const ROW_Y: f32 = 60.0;
/// Reserves room left of the first real node for the Source pill and its
/// cable, so the graph never renders at a negative canvas-local x (which
/// used to spill the Source node visually into the palette column).
/// Purely a display-space offset - stored `EffectNode::position` values
/// stay 0-based; this is added/subtracted at the render boundary only.
pub const CANVAS_MARGIN_X: f32 = NODE_SPACING;
/// Gap between the last real node (or Source, if the chain is empty) and
/// the Out node - deliberately roomier than the tight 16px gap
/// `EffectGraph::push_at_end` leaves between consecutive effect nodes, so
/// Out reads as the board's fixed "end of chain" anchor rather than just
/// another node crowded into the row.
const OUTPUT_GAP: f32 = 56.0;

/// A node's position, honoring a live drag preview for whichever node
/// (if any) is currently being dragged - so cables follow the drag
/// without needing the model itself (and thus the whole node list) to
/// change until the drag actually commits on release.
pub fn io_position(graph: &EffectGraph, id: EffectNodeId, drag: Option<(EffectNodeId, f32, f32)>) -> (f32, f32) {
    if let Some((drag_id, x, y)) = drag {
        if drag_id == id {
            return (x, y);
        }
    }
    if id == EffectGraph::SOURCE {
        return (0.0, ROW_Y + (NODE_H - IO_H) * 0.5);
    }
    if id == EffectGraph::OUTPUT {
        // The gap is measured from the *actual last node in signal-chain
        // order* (`ordered()`, walked from the real edge list), not just
        // whichever node happens to have the largest x - position is
        // cosmetic and dragging doesn't touch chain order (Phase 7), so
        // those two can disagree once a node's been dragged out of its
        // auto-layout position.
        let last_x = graph.ordered().last().map(|n| n.position.0);
        let x = match last_x {
            Some(last_x) => last_x + CANVAS_MARGIN_X + NODE_W + OUTPUT_GAP,
            None => IO_W + OUTPUT_GAP,
        };
        return (x, ROW_Y + (NODE_H - IO_H) * 0.5);
    }
    graph.node(id).map(|n| (n.position.0 + CANVAS_MARGIN_X, n.position.1)).unwrap_or((CANVAS_MARGIN_X, ROW_Y))
}

pub fn port_out(graph: &EffectGraph, id: EffectNodeId, drag: Option<(EffectNodeId, f32, f32)>) -> (f32, f32) {
    let (x, y) = io_position(graph, id, drag);
    let (w, h) = node_size(id);
    (x + w, y + h * 0.5)
}

pub fn port_in(graph: &EffectGraph, id: EffectNodeId, drag: Option<(EffectNodeId, f32, f32)>) -> (f32, f32) {
    let (x, y) = io_position(graph, id, drag);
    let (_, h) = node_size(id);
    (x, y + h * 0.5)
}

/// The input port nearest `(x, y)` (canvas-local), within a generous
/// grab radius - used on wire-drag release to figure out what the
/// cursor actually landed on, since mouse capture routes the release
/// event to the port that started the drag, not whatever's visually
/// under the cursor.
pub fn nearest_input_port(graph: &EffectGraph, x: f32, y: f32) -> Option<EffectNodeId> {
    const RADIUS: f32 = 20.0;
    let mut targets: Vec<EffectNodeId> = graph.nodes.iter().map(|n| n.id).collect();
    targets.push(EffectGraph::OUTPUT);
    targets
        .into_iter()
        .map(|id| {
            let (px, py) = port_in(graph, id, None);
            (id, ((px - x).powi(2) + (py - y).powi(2)).sqrt())
        })
        .filter(|(_, d)| *d <= RADIUS)
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(id, _)| id)
}

/// An effect's processing latency in milliseconds, at the engine's fixed
/// 48kHz sample rate (matching `eq_curve.rs`'s own display-side constant).
/// Both effect types today are zero-latency feedforward DSP - update this
/// alongside `engine::EffectUnit` when a lookahead- or FIR-based effect
/// type is added.
pub fn effect_latency_ms(effect: &Effect) -> f32 {
    match effect {
        Effect::Compressor(_) | Effect::Eq(_) => 0.0,
    }
}

pub fn node_size(id: EffectNodeId) -> (f32, f32) {
    if id == EffectGraph::SOURCE || id == EffectGraph::OUTPUT {
        (IO_W, IO_H)
    } else {
        (NODE_W, NODE_H)
    }
}
