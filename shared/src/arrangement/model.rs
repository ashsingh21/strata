//! The arrangement document: tracks, clips, automation lanes, loop range
//! and markers. All positions are [`Ticks`]. Mutated only through
//! [`super::commands::Command`] so every edit is undoable.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::peaks::PeakPyramid;
use super::time::{TempoMap, Ticks, PPQ};

pub type TrackId = u32;
pub type ClipId = u32;
/// Groups linked MIDI clips (see `ClipContent::Midi::link`).
pub type LinkId = u32;
pub type AutomationLaneId = u32;
pub type MarkerId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    Audio,
    Midi,
}

/// One of the six `clip-*` design tokens. Also used as a track's colour
/// swatch, since a track's clips default to its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipColor {
    Coral,
    Amber,
    Teal,
    Blue,
    Violet,
    Pink,
}

/// A track lane's height in px: the timeline default (matches
/// `ui::timeline::LANE_HEIGHT`), and a resize handle's range either side.
pub const DEFAULT_TRACK_HEIGHT: f32 = 96.0;
pub const MIN_TRACK_HEIGHT: f32 = 40.0;
pub const MAX_TRACK_HEIGHT: f32 = 240.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub name: String,
    pub color: ClipColor,
    pub kind: TrackKind,
    pub mute: bool,
    pub solo: bool,
    pub arm: bool,
    pub gain_db: f32,
    /// Lane height in pixels; defaults to `tokens::SIZE_LANE`.
    pub height: f32,
    /// What a MIDI track plays through (its patch lives in the project's
    /// per-track instrument list). Always `None` on audio tracks.
    /// `default` so projects saved before instruments existed still load.
    #[serde(default)]
    pub instrument: Option<Instrument>,
    /// The track's insert effect chain, as a graph (still walked in a
    /// strict straight line until the parallel-branches phase - node
    /// *order* comes from `EffectGraph`'s edges, never from a node's
    /// on-canvas `position`, so dragging a node around and reordering
    /// the signal chain stay decoupled). Separate from `instrument` (an
    /// audio track has effects but no instrument; a MIDI track can have
    /// both). `default` so projects saved before effects existed still
    /// load; `Project::migrate` fills this in from `effect_slots`/
    /// `effects` (below) for a project saved before the graph shape
    /// existed.
    #[serde(default)]
    pub fx: EffectGraph,
    /// A Drum Kit track's per-pad mute, level and tuning, in `DRUM_KIT`
    /// order. `default` so older projects load with every pad as sampled.
    #[serde(default)]
    pub drum_pads: crate::drums::Pads,
    /// Old shape (a flat `Vec<EffectSlot>`, no positions/graph) - read-
    /// only, kept only so `Project::migrate` can convert it into `fx`
    /// once. Never written to a new save (`skip_serializing`).
    #[serde(default, skip_serializing)]
    pub effect_slots: Vec<EffectSlot>,
    /// Older still (a bare `Effect`, no enable bit either) - same
    /// read-only, migrate-once, never-written-again treatment.
    #[serde(default, skip_serializing)]
    pub effects: Vec<Effect>,
}

/// One effect in a track's chain, plus whether it's actually running -
/// added so the `TrackHeaderFx` pip control (filled = on, hollow = off)
/// has a real bit to read instead of existence-only state. Superseded by
/// `EffectNode` (which adds a graph position) but kept as the legacy
/// shape `Project::migrate` reads from.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectSlot {
    pub effect: Effect,
    #[serde(default = "default_effect_enabled")]
    pub enabled: bool,
}

fn default_effect_enabled() -> bool {
    true
}

impl EffectSlot {
    pub fn new(effect: Effect) -> Self {
        Self { effect, enabled: true }
    }
}

impl From<Effect> for EffectSlot {
    fn from(effect: Effect) -> Self {
        Self::new(effect)
    }
}

pub type EffectNodeId = u32;

/// One real effect on the board, at a position the canvas can render it
/// at (cosmetic only - see `EffectGraph`'s own doc comment on why
/// position never drives audio order).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectNode {
    pub id: EffectNodeId,
    pub effect: Effect,
    pub enabled: bool,
    pub position: (f32, f32),
}

/// A directed connection between two nodes - `from`/`to` are either a
/// real `EffectNode.id` or `EffectGraph::SOURCE`/`EffectGraph::OUTPUT`,
/// the two fixed marker ids every graph has that never appear in `nodes`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EffectEdge {
    pub from: EffectNodeId,
    pub to: EffectNodeId,
}

/// A track's (or the master bus's) effect chain. Until the
/// parallel-branches phase this is enforced-by-construction to stay a
/// strict linear chain - every node has exactly one inbound and one
/// outbound edge - so every mutating method here keeps that invariant
/// rather than the caller having to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectGraph {
    pub nodes: Vec<EffectNode>,
    pub edges: Vec<EffectEdge>,
    next_id: EffectNodeId,
}

impl EffectGraph {
    /// Fixed marker ids for the chain's two ends - never real nodes, so
    /// they never collide with a `next_id`-allocated `EffectNode.id`
    /// (which starts at `SOURCE + 1` and only grows).
    pub const SOURCE: EffectNodeId = 0;
    pub const OUTPUT: EffectNodeId = 1;

    pub fn new() -> Self {
        Self { nodes: vec![], edges: vec![EffectEdge { from: Self::SOURCE, to: Self::OUTPUT }], next_id: 2 }
    }

    /// Rebuilds a graph from the old flat-list shape, in order - used
    /// only by `Project::migrate`.
    pub fn from_flat(slots: Vec<EffectSlot>) -> Self {
        let mut graph = Self::new();
        for (i, slot) in slots.into_iter().enumerate() {
            let id = graph.push_at_end(slot.effect);
            graph.set_enabled(id, slot.enabled);
            if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == id) {
                node.position = (i as f32 * 166.0, 0.0);
            }
        }
        graph
    }

    /// The real effect nodes, source-to-output. Every read site that used
    /// to iterate the old flat `Vec<EffectSlot>` in order reads this
    /// instead - it's the one thing that has to walk edges rather than
    /// just `nodes` directly, since `nodes`' own storage order isn't
    /// meaningful.
    pub fn ordered(&self) -> Vec<&EffectNode> {
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut current = Self::SOURCE;
        while let Some(edge) = self.edges.iter().find(|e| e.from == current) {
            if edge.to == Self::OUTPUT {
                break;
            }
            match self.nodes.iter().find(|n| n.id == edge.to) {
                Some(node) => out.push(node),
                None => break,
            }
            current = edge.to;
        }
        out
    }

    /// Where a new guitar effect of `kind` goes: just before the first
    /// guitar effect that ranks after it (the amp ahead of the echo), or
    /// the end of the chain when none does.
    pub fn guitar_slot(&self, kind: crate::guitar::GuitarKind) -> EffectNodeId {
        self.ordered()
            .into_iter()
            .find(|n| matches!(n.effect, Effect::Guitar(g) if g.kind.rank() > kind.rank()))
            .map_or(Self::OUTPUT, |n| n.id)
    }

    pub fn node(&self, id: EffectNodeId) -> Option<&EffectNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Appends a new node right before the output, at the end of the
    /// chain - the only way a node gets created pre-Phase-11, since the
    /// graph stays a strict line until then.
    pub fn push_at_end(&mut self, effect: Effect) -> EffectNodeId {
        let id = self.next_id;
        self.next_id += 1;
        let last_index = self.edges.iter().position(|e| e.to == Self::OUTPUT).expect("EffectGraph: no edge into OUTPUT");
        let last_edge = self.edges.remove(last_index);
        let x = self.nodes.len() as f32 * 166.0;
        self.nodes.push(EffectNode { id, effect, enabled: true, position: (x, 0.0) });
        self.edges.push(EffectEdge { from: last_edge.from, to: id });
        self.edges.push(EffectEdge { from: id, to: Self::OUTPUT });
        id
    }

    /// Removes a node and reconnects its former neighbours directly -
    /// only valid while the graph is a strict line (every node has
    /// exactly one inbound/outbound edge to splice around). Returns the
    /// removed node and its two edges so the caller's undo command can
    /// restore the exact prior structure.
    pub fn remove(&mut self, id: EffectNodeId) -> Option<(EffectNode, EffectEdge, EffectEdge)> {
        let node_index = self.nodes.iter().position(|n| n.id == id)?;
        let inbound_index = self.edges.iter().position(|e| e.to == id)?;
        let outbound_index = self.edges.iter().position(|e| e.from == id)?;
        let node = self.nodes.remove(node_index);
        // Remove the higher index first so the second removal's index
        // still points at the right element.
        let (first, second) = if inbound_index > outbound_index { (inbound_index, outbound_index) } else { (outbound_index, inbound_index) };
        let edge_a = self.edges.remove(first);
        let edge_b = self.edges.remove(second);
        let (inbound, outbound) = if edge_a.to == id { (edge_a, edge_b) } else { (edge_b, edge_a) };
        self.edges.push(EffectEdge { from: inbound.from, to: outbound.to });
        Some((node, inbound, outbound))
    }

    /// Re-inserts a node exactly where `remove` took it from - the
    /// inverse half of `RemoveEffectNode`'s undo.
    pub fn reinsert(&mut self, node: EffectNode, inbound: EffectEdge, outbound: EffectEdge) {
        let splice_index = self.edges.iter().position(|e| e.from == inbound.from && e.to == outbound.to);
        if let Some(i) = splice_index {
            self.edges.remove(i);
        }
        self.nodes.push(node);
        self.edges.push(inbound);
        self.edges.push(outbound);
    }

    /// The id a node currently feeds - `None` only if `id` isn't in the
    /// graph at all (every real node always has exactly one outbound
    /// edge while the chain stays linear).
    pub fn successor_of(&self, id: EffectNodeId) -> Option<EffectNodeId> {
        self.edges.iter().find(|e| e.from == id).map(|e| e.to)
    }

    /// Moves `node` to a new position in the chain, right before
    /// `before` - the port-drag rewire operation. Since the graph stays
    /// a strict line until parallel branches exist, "wire node's output
    /// into before's input" and "move node to just before before" are
    /// the same edit: extract `node` from wherever it currently sits
    /// (splicing its old neighbours together, same reconnect logic
    /// `remove` uses), then splice it back in immediately ahead of
    /// `before`. A no-op if `node == before` or either id isn't a real
    /// edge endpoint.
    pub fn move_before(&mut self, node: EffectNodeId, before: EffectNodeId) {
        if node == before {
            return;
        }
        let Some(in_idx) = self.edges.iter().position(|e| e.to == node) else { return };
        let Some(out_idx) = self.edges.iter().position(|e| e.from == node) else { return };
        let (first, second) = if in_idx > out_idx { (in_idx, out_idx) } else { (out_idx, in_idx) };
        let edge_a = self.edges.remove(first);
        let edge_b = self.edges.remove(second);
        let (inbound, outbound) = if edge_a.to == node { (edge_a, edge_b) } else { (edge_b, edge_a) };
        self.edges.push(EffectEdge { from: inbound.from, to: outbound.to });

        let Some(before_in_idx) = self.edges.iter().position(|e| e.to == before) else {
            // `before` isn't reachable (shouldn't happen for a valid id) -
            // put `node` back exactly where it was rather than lose it.
            self.edges.retain(|e| !(e.from == inbound.from && e.to == outbound.to));
            self.edges.push(inbound);
            self.edges.push(outbound);
            return;
        };
        let before_in = self.edges.remove(before_in_idx);
        self.edges.push(EffectEdge { from: before_in.from, to: node });
        self.edges.push(EffectEdge { from: node, to: before });
    }

    /// Returns the previous value, for the caller's undo inverse.
    pub fn set_enabled(&mut self, id: EffectNodeId, enabled: bool) -> bool {
        match self.nodes.iter_mut().find(|n| n.id == id) {
            Some(node) => std::mem::replace(&mut node.enabled, enabled),
            None => enabled,
        }
    }

    /// Returns the previous value, for the caller's undo inverse.
    pub fn set_position(&mut self, id: EffectNodeId, position: (f32, f32)) -> (f32, f32) {
        match self.nodes.iter_mut().find(|n| n.id == id) {
            Some(node) => std::mem::replace(&mut node.position, position),
            None => position,
        }
    }

    // --- Parallel branches (Phase 11) -----------------------------------
    //
    // Everything above keeps the graph a strict line by construction.
    // These add real fan-out/fan-in - one output feeding several inputs,
    // one input summing several outputs - without touching any of the
    // above, so every earlier phase's behavior on an still-linear graph
    // (the overwhelmingly common case) is unchanged. Engine-side
    // execution of a genuinely branching graph, and a wiring gesture
    // that creates one (as opposed to reordering via `move_before`), are
    // deliberately not part of this pass - see the effects-board plan's
    // own Phase 11 notes.

    /// Whether adding an edge `from -> to` would create a cycle - a DFS
    /// from `to` looking for a path back to `from`. Every edge-adding
    /// method here checks this first and refuses (returns `false`
    /// instead of mutating) rather than ever leaving the graph cyclic.
    pub fn would_cycle(&self, from: EffectNodeId, to: EffectNodeId) -> bool {
        if from == to {
            return true;
        }
        let mut stack = vec![to];
        let mut seen = std::collections::HashSet::new();
        while let Some(current) = stack.pop() {
            if current == from {
                return true;
            }
            if !seen.insert(current) {
                continue;
            }
            stack.extend(self.edges.iter().filter(|e| e.from == current).map(|e| e.to));
        }
        false
    }

    /// Adds a new edge without removing any existing one - the fan-out/
    /// fan-in primitive. Returns `false` (no-op) if it would create a
    /// cycle or the edge already exists.
    pub fn connect(&mut self, from: EffectNodeId, to: EffectNodeId) -> bool {
        if self.edges.contains(&EffectEdge { from, to }) || self.would_cycle(from, to) {
            return false;
        }
        self.edges.push(EffectEdge { from, to });
        true
    }

    /// Removes one specific edge (not the whole node) - the inverse of
    /// `connect`. Returns `false` if that exact edge wasn't there.
    pub fn disconnect(&mut self, from: EffectNodeId, to: EffectNodeId) -> bool {
        let target = EffectEdge { from, to };
        match self.edges.iter().position(|e| *e == target) {
            Some(i) => {
                self.edges.remove(i);
                true
            }
            None => false,
        }
    }

    /// A topological order of the real nodes (Kahn's algorithm) - well-
    /// defined for any acyclic graph, branching or not, unlike
    /// `ordered()` (which assumes a strict line and silently follows
    /// only the first outbound edge at each step). The engine's own
    /// per-block DAG execution (summing at fan-in points) isn't wired to
    /// this yet - it's the primitive that work would run on top of.
    pub fn topo_order(&self) -> Vec<EffectNodeId> {
        let mut in_degree: std::collections::HashMap<EffectNodeId, usize> =
            self.nodes.iter().map(|n| (n.id, 0)).collect();
        for edge in &self.edges {
            // Only count an inbound edge from another *real* node -
            // SOURCE isn't one, so a node fed straight from it (the
            // overwhelmingly common case, "the first effect in the
            // chain") still starts at in-degree 0, ready immediately.
            if in_degree.contains_key(&edge.from) {
                if let Some(d) = in_degree.get_mut(&edge.to) {
                    *d += 1;
                }
            }
        }
        let mut ready: Vec<EffectNodeId> =
            in_degree.iter().filter(|(_, d)| **d == 0).map(|(id, _)| *id).collect();
        ready.sort();
        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some(id) = ready.pop() {
            order.push(id);
            let mut newly_ready = Vec::new();
            for edge in self.edges.iter().filter(|e| e.from == id) {
                if let Some(d) = in_degree.get_mut(&edge.to) {
                    *d -= 1;
                    if *d == 0 {
                        newly_ready.push(edge.to);
                    }
                }
            }
            newly_ready.sort();
            ready.extend(newly_ready);
        }
        order
    }
}

impl Default for EffectGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// A track's instrument.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Instrument {
    Carve,
    /// Plays one-shot samples by note (see `crate::drums::DRUM_KIT`).
    Drums,
}

impl Instrument {
    pub fn name(self) -> &'static str {
        match self {
            Instrument::Carve => "Carve",
            Instrument::Drums => "Drum Kit",
        }
    }

    /// What a new track of `kind` starts with: MIDI tracks get Carve (a
    /// MIDI track with no instrument makes no sound), audio tracks nothing.
    pub fn default_for(kind: TrackKind) -> Option<Instrument> {
        match kind {
            TrackKind::Midi => Some(Instrument::Carve),
            TrackKind::Audio => None,
        }
    }
}

/// A track's insert effect.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Effect {
    Compressor(CompressorState),
    Eq(EqState),
    Guitar(crate::guitar::GuitarFx),
}

impl Effect {
    pub fn name(self) -> &'static str {
        match self {
            Effect::Compressor(_) => "Compressor",
            Effect::Eq(_) => "EQ",
            Effect::Guitar(g) => g.kind.name(),
        }
    }
}

/// A feedforward compressor's knobs. The DSP itself (the envelope
/// follower and its running state) lives in `engine` - this is just the
/// config, mirroring the `SynthState`/`SynthParams` split.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompressorState {
    pub threshold_db: f32,
    /// 1.0 = no compression, higher = harder. Not clamped at the top;
    /// a very high ratio is how a user gets limiter-like behavior.
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    /// Makeup gain, applied after gain reduction.
    pub makeup_db: f32,
    /// How much of the untouched signal is mixed back in (0..1) - parallel
    /// compression, which keeps a guitar's attack while evening out its tail.
    #[serde(default)]
    pub dry: f32,
    /// A high-pass on what the compressor listens to (not on what you
    /// hear), so low notes don't make it clamp down. 0 = off.
    #[serde(default)]
    pub detector_hp_hz: f32,
}

impl Default for CompressorState {
    fn default() -> Self {
        Self { threshold_db: -18.0, ratio: 4.0, attack_ms: 10.0, release_ms: 150.0, makeup_db: 0.0, dry: 0.0, detector_hp_hz: 0.0 }
    }
}

impl CompressorState {
    /// Ratio 1.0 is mathematically a no-op (zero gain reduction
    /// regardless of threshold) - what every track without a Compressor
    /// in its `effects` list is treated as, so the engine can always run
    /// the same DSP unit rather than branching on `Option`.
    pub fn bypass() -> Self {
        Self { ratio: 1.0, ..Self::default() }
    }
}

/// What one EQ band does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EqBandKind {
    /// Removes everything below `freq_hz` (12 dB/oct high-pass).
    LowCut,
    /// Raises or lowers everything below `freq_hz` by `gain_db`.
    LowShelf,
    /// Raises or lowers a band around `freq_hz`; `q` sets how narrow.
    Bell,
    /// Raises or lowers everything above `freq_hz` by `gain_db`.
    HighShelf,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EqBand {
    pub kind: EqBandKind,
    pub on: bool,
    pub freq_hz: f32,
    /// Boost (positive) or cut (negative); unused by a low cut.
    pub gain_db: f32,
    /// Bandwidth - higher is narrower, same convention as most EQs. Only
    /// the bell uses it.
    pub q: f32,
}

/// Which band is which in `EqState::bands`.
pub const EQ_LOW_CUT: usize = 0;
pub const EQ_LOW_SHELF: usize = 1;
pub const EQ_BELL: usize = 2;
pub const EQ_HIGH_SHELF: usize = 3;

/// A four-band EQ: low cut, low shelf, bell, high shelf, in that order.
/// Same config/DSP split as `CompressorState` - the running filter state
/// lives in `engine`, the filter maths in `crate::eq`.
///
/// Projects saved when the EQ was one bell band (`{freq_hz, gain_db, q}`)
/// load as that bell, the other bands flat.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "EqRepr")]
pub struct EqState {
    pub bands: [EqBand; 4],
}

#[derive(Deserialize)]
#[serde(untagged)]
enum EqRepr {
    Bands { bands: [EqBand; 4] },
    Bell { freq_hz: f32, gain_db: f32, q: f32 },
}

impl From<EqRepr> for EqState {
    fn from(repr: EqRepr) -> Self {
        match repr {
            EqRepr::Bands { bands } => Self { bands },
            EqRepr::Bell { freq_hz, gain_db, q } => Self::bell(freq_hz, gain_db, q),
        }
    }
}

impl Default for EqState {
    fn default() -> Self {
        let band = |kind, on, freq_hz| EqBand { kind, on, freq_hz, gain_db: 0.0, q: 1.0 };
        Self {
            bands: [
                band(EqBandKind::LowCut, false, 40.0),
                band(EqBandKind::LowShelf, true, 120.0),
                band(EqBandKind::Bell, true, 1000.0),
                band(EqBandKind::HighShelf, true, 8000.0),
            ],
        }
    }
}

impl EqState {
    /// Flat, with the bell set - what the one-band EQ was.
    pub fn bell(freq_hz: f32, gain_db: f32, q: f32) -> Self {
        let mut eq = Self::default();
        eq.bands[EQ_BELL] = EqBand { freq_hz, gain_db, q, ..eq.bands[EQ_BELL] };
        eq
    }

    /// Flat: every band at 0 dB and the low cut off - a no-op, what a
    /// bypassed EQ is treated as, same reasoning as `CompressorState::bypass`.
    pub fn bypass() -> Self {
        Self::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MidiNote {
    pub start: Ticks,
    pub length: Ticks,
    pub pitch: u8,
    /// 1..=127, how hard the note is played (Carve maps it to level).
    /// `default` so projects saved before velocity existed still load.
    #[serde(default = "default_velocity")]
    pub velocity: u8,
}

/// The velocity every new note gets (drawn, step-entered or seeded).
pub const DEFAULT_VELOCITY: u8 = 100;

fn default_velocity() -> u8 {
    DEFAULT_VELOCITY
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClipContent {
    Audio {
        /// File name under the assets directory, e.g. `"drums.wav"`. Several
        /// clips (and tracks) commonly share one source; the peak pyramid is
        /// built once per unique source and shared via `Arc`.
        source: Arc<str>,
        /// Populated asynchronously once the background loader finishes
        /// decoding the source WAV and building the peak pyramid - never
        /// saved (rebuilt from the WAV file on load, same as on first
        /// reference today).
        #[serde(skip)]
        peaks: Option<Arc<PeakPyramid>>,
        /// Offset into the source audio, in samples, that this clip's
        /// `start` corresponds to. Trimming or splitting the left edge
        /// advances this so playback (and the displayed waveform slice)
        /// keeps referencing the right part of the source.
        source_offset_samples: u64,
    },
    Midi {
        /// Starts are relative to the clip's start.
        notes: Vec<MidiNote>,
        /// `Some(len)`: the clip is a looping pattern - the notes in
        /// `0..len` repeat every `len` ticks for the clip's whole length
        /// (notes past `len` are kept but not heard). `None`: the notes
        /// play once. `default` so older projects load.
        #[serde(default)]
        loop_len: Option<Ticks>,
        /// Clips sharing a link id are copies of one pattern: a note edit
        /// in any of them (or a change of pattern length) is made in all.
        /// Each keeps its own position and length. `None`: independent.
        #[serde(default)]
        link: Option<LinkId>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clip {
    pub id: ClipId,
    pub track: TrackId,
    pub start: Ticks,
    pub length: Ticks,
    pub name: String,
    pub content: ClipContent,
    pub recording: bool,
    /// This clip's own level, on top of its track's mixer gain - 0 for
    /// every normal clip; a drum pad tap sets a small random value so
    /// repeated hits of the same sample aren't byte-for-byte identical.
    #[serde(default)]
    pub gain_db: f32,
    /// MIDI clips: how much the off-beat 16ths (the "e" and "a" of each
    /// beat) are pushed late when played, 0 = straight to 1 = half a 16th
    /// late (75% swing). The notes stay on the grid; see `played_notes`.
    #[serde(default)]
    pub swing: f32,
}

impl Clip {
    pub fn end(&self) -> Ticks {
        self.start + self.length
    }

    /// This clip's link id, if it's a linked MIDI clip.
    pub fn link(&self) -> Option<LinkId> {
        match self.content {
            ClipContent::Midi { link, .. } => link,
            ClipContent::Audio { .. } => None,
        }
    }

    /// The same clip, no longer linked to anything.
    pub fn unlinked(mut self) -> Clip {
        if let ClipContent::Midi { link, .. } = &mut self.content {
            *link = None;
        }
        self
    }

    /// The same clip, in link group `id` (MIDI only).
    pub fn linked_to(mut self, id: LinkId) -> Clip {
        if let ClipContent::Midi { link, .. } = &mut self.content {
            *link = Some(id);
        }
        self
    }

    /// A MIDI clip's pattern length: its loop length, or the whole clip.
    pub fn content_len(&self) -> Ticks {
        match &self.content {
            ClipContent::Midi { loop_len: Some(len), .. } => (*len).max(1),
            _ => self.length,
        }
    }

    /// This clip with its end moved so it's `length` long. A MIDI clip
    /// stretched past its current length becomes a loop of what it held
    /// (the way clips extend in Ableton or Bitwig), so dragging a one-bar
    /// beat out to 16 bars repeats it. `None` for anything else (audio,
    /// or not getting longer): a plain trim.
    pub fn extended_as_loop(&self, length: Ticks) -> Option<Clip> {
        let ClipContent::Midi { notes, loop_len, link } = &self.content else { return None };
        if length <= self.length {
            return None;
        }
        let mut clip = self.clone();
        clip.length = length;
        clip.content = ClipContent::Midi { notes: notes.clone(), loop_len: Some(loop_len.unwrap_or(self.length)), link: *link };
        Some(clip)
    }

    /// Every note as heard, with starts relative to the clip's start: the
    /// pattern repeated across the clip if it loops, and nothing past the
    /// clip's end (a note running over it is shortened). Empty for audio.
    /// With `swing`, a note on an off-beat 16th is pushed late by up to
    /// half a 16th.
    pub fn played_notes(&self) -> Vec<MidiNote> {
        let ClipContent::Midi { notes, loop_len, .. } = &self.content else { return Vec::new() };
        let sixteenth = PPQ / 4;
        let swing_by = (self.swing.clamp(0.0, 1.0) * (sixteenth / 2) as f32).round() as Ticks;
        let period = loop_len.map(|l| l.max(1)).unwrap_or(self.length.max(1));
        let mut out = Vec::new();
        let mut offset = 0;
        while offset < self.length {
            for n in notes.iter().filter(|n| n.start < period) {
                let swung = swing_by > 0 && n.start % (2 * sixteenth) == sixteenth;
                let start = offset + n.start + if swung { swing_by } else { 0 };
                if start >= self.length {
                    continue;
                }
                out.push(MidiNote { start, length: n.length.min(self.length - start), ..*n });
            }
            if loop_len.is_none() {
                break;
            }
            offset += period;
        }
        out.sort_by_key(|n| (n.start, n.pitch));
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Breakpoint {
    pub tick: Ticks,
    /// Normalized 0..1.
    pub value: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomationLane {
    pub id: AutomationLaneId,
    pub track: TrackId,
    pub parameter_name: String,
    /// Formatted current value shown in the lane's header, e.g. "2.4 kHz".
    pub display_value: String,
    /// Sorted by tick.
    pub breakpoints: Vec<Breakpoint>,
    /// What this lane controls. `None` for lanes from before automation was
    /// audible (and the demo seed lane) - those only draw.
    #[serde(default)]
    pub target: Option<super::automation::AutomationTarget>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopRange {
    pub start: Ticks,
    pub end: Ticks,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Marker {
    pub id: MarkerId,
    pub position: Ticks,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Arrangement {
    pub tempo_map: TempoMap,
    pub tracks: Vec<Track>,
    pub clips: Vec<Clip>,
    pub automation: Vec<AutomationLane>,
    pub loop_range: Option<LoopRange>,
    pub markers: Vec<Marker>,
    /// The master bus's own effect chain - runs after every track's own
    /// chain and fader, on the final mixed signal, before the main
    /// output meter. `default` so projects saved before master effects
    /// existed still load.
    #[serde(default)]
    pub master_effects: EffectGraph,
    next_id: u32,
}

impl Arrangement {
    pub fn new(tempo_map: TempoMap) -> Self {
        Self {
            tempo_map,
            tracks: Vec::new(),
            clips: Vec::new(),
            automation: Vec::new(),
            loop_range: None,
            markers: Vec::new(),
            master_effects: EffectGraph::new(),
            next_id: 1,
        }
    }

    /// The effect chain a board or command targets - `Some(id)` for a
    /// track, `None` for the master bus. A thin indirection so the same
    /// `Command`/`TimelineEvent` shapes work for both without a second,
    /// duplicated set of "Master*" variants.
    pub fn fx(&self, track: Option<TrackId>) -> Option<&EffectGraph> {
        match track {
            Some(id) => self.track(id).map(|t| &t.fx),
            None => Some(&self.master_effects),
        }
    }

    pub fn fx_mut(&mut self, track: Option<TrackId>) -> Option<&mut EffectGraph> {
        match track {
            Some(id) => self.track_mut(id).map(|t| &mut t.fx),
            None => Some(&mut self.master_effects),
        }
    }

    /// Allocates a fresh id, unique within this arrangement (ids are never
    /// reused, even across undo/redo, so stale references can't collide).
    pub fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// "Audio 2", "MIDI 1", ... - the lowest number not already used by a
    /// track name with that prefix. Numbering by the arrangement-wide id
    /// instead gave names like "Audio 15" for the fourth track.
    pub fn next_track_name(&self, kind: TrackKind) -> String {
        let prefix = match kind {
            TrackKind::Audio => "Audio",
            TrackKind::Midi => "MIDI",
        };
        let taken: std::collections::HashSet<u32> = self
            .tracks
            .iter()
            .filter_map(|t| t.name.strip_prefix(prefix)?.strip_prefix(' ')?.parse().ok())
            .collect();
        let n = (1..).find(|n| !taken.contains(n)).unwrap_or(1);
        format!("{prefix} {n}")
    }

    /// `base` if no track is called that yet, else "`base` 2", "`base` 3"...
    pub fn next_named(&self, base: &str) -> String {
        let taken = |name: &str| self.tracks.iter().any(|t| t.name == name);
        if !taken(base) {
            return base.to_string();
        }
        (2..).map(|n| format!("{base} {n}")).find(|n| !taken(n)).unwrap_or_else(|| base.to_string())
    }

    /// The automatic name for a MIDI track playing `instrument`: "Drums"
    /// for a Drum Kit, "MIDI N" otherwise.
    pub fn name_for_instrument(&self, instrument: Option<Instrument>) -> String {
        match instrument {
            Some(Instrument::Drums) => self.next_named("Drums"),
            _ => self.next_track_name(TrackKind::Midi),
        }
    }

    /// Whether `name` is one Strata gave a MIDI track itself ("MIDI 3",
    /// "Drums", "Drums 2") rather than one the user typed - only those
    /// follow the track's instrument when it changes.
    pub fn is_automatic_midi_name(name: &str) -> bool {
        let numbered = |prefix: &str| {
            name.strip_prefix(prefix).is_some_and(|rest| rest.is_empty() || rest.strip_prefix(' ').is_some_and(|n| n.parse::<u32>().is_ok()))
        };
        (numbered("MIDI") && name != "MIDI") || numbered("Drums")
    }

    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    pub fn clip(&self, id: ClipId) -> Option<&Clip> {
        self.clips.iter().find(|c| c.id == id)
    }

    pub fn clip_mut(&mut self, id: ClipId) -> Option<&mut Clip> {
        self.clips.iter_mut().find(|c| c.id == id)
    }

    /// Copies `from`'s pattern (notes and loop length) to every other clip
    /// linked to it. Positions and lengths stay each clip's own.
    pub fn sync_links(&mut self, from: ClipId) {
        let Some(ClipContent::Midi { notes, loop_len, link: Some(link) }) = self.clip(from).map(|c| c.content.clone()) else {
            return;
        };
        for clip in self.clips.iter_mut().filter(|c| c.id != from) {
            if let ClipContent::Midi { notes: n, loop_len: l, link: Some(k) } = &mut clip.content {
                if *k == link {
                    *n = notes.clone();
                    *l = loop_len;
                }
            }
        }
    }

    /// How many clips share `link`.
    pub fn link_count(&self, link: LinkId) -> usize {
        self.clips.iter().filter(|c| matches!(c.content, ClipContent::Midi { link: Some(k), .. } if k == link)).count()
    }

    pub fn clips_on_track(&self, track: TrackId) -> impl Iterator<Item = &Clip> {
        self.clips.iter().filter(move |c| c.track == track)
    }

    pub fn automation_lane(&self, id: AutomationLaneId) -> Option<&AutomationLane> {
        self.automation.iter().find(|a| a.id == id)
    }

    pub fn automation_lane_mut(&mut self, id: AutomationLaneId) -> Option<&mut AutomationLane> {
        self.automation.iter_mut().find(|a| a.id == id)
    }
}

// --- Modulation targets (Phase 12 design spike) -------------------------
//
// Neither existing "point at a parameter" system reaches "any parameter on
// any effect node": `synth::model::LfoTarget` is a closed 4-variant enum
// entirely internal to one Carve instance (no track/node concept at all),
// and `AutomationLane.parameter_name` is a freeform display `String`, not
// a structured address, and is track-only (no master, per `TrackId` not
// `Option<TrackId>`). `ModTarget` is the generalized address the
// effects-board plan's own Phase 12 called for - added here as a data-only
// foundation (not yet wired into the engine's per-sample modulation math
// or a board UI for dragging a modulator onto a knob), matching the same
// "model layer now, engine/UI execution later" split Phase 11 used for
// parallel branches.

/// One effect type's own knobs, addressable individually - the
/// parameter half of a `ModTarget`. A new effect type needs its own
/// variants added here, same as `Effect`/`EffectUnitState` themselves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectParam {
    CompressorThreshold,
    CompressorRatio,
    CompressorAttack,
    CompressorRelease,
    CompressorMakeup,
    CompressorDry,
    CompressorDetectorHp,
    /// One knob of a guitar effect: its kind, and its index in `GuitarKind::specs`.
    Guitar(crate::guitar::GuitarKind, u8),
    /// The bell band (the EQ's only band once, hence the plain names -
    /// saved automation lanes still point at them).
    EqFreq,
    EqGain,
    EqQ,
    EqLowCut,
    EqLowFreq,
    EqLowGain,
    EqHighFreq,
    EqHighGain,
}

/// A guitar effect's params as `EffectParam`s - a static table, since
/// `for_effect` hands out a slice.
fn guitar_params(kind: crate::guitar::GuitarKind) -> &'static [EffectParam] {
    use crate::guitar::GuitarKind as K;
    macro_rules! knobs {
        ($kind:expr; $($i:expr),*) => { &[$(EffectParam::Guitar($kind, $i)),*] };
    }
    match kind {
        K::Gate => knobs!(K::Gate; 0, 1),
        K::Amp => knobs!(K::Amp; 0, 1, 2, 3, 4, 5),
        K::Cabinet => knobs!(K::Cabinet; 0, 1),
        K::Chorus => knobs!(K::Chorus; 0, 1, 2, 3),
        K::Echo => knobs!(K::Echo; 0, 1, 2, 3, 4),
        K::Spring => knobs!(K::Spring; 0, 1, 2, 3),
    }
}

impl EffectParam {
    pub fn name(self) -> &'static str {
        match self {
            EffectParam::CompressorThreshold => "Threshold",
            EffectParam::CompressorRatio => "Ratio",
            EffectParam::CompressorAttack => "Attack",
            EffectParam::CompressorRelease => "Release",
            EffectParam::CompressorMakeup => "Makeup",
            EffectParam::CompressorDry => "Dry",
            EffectParam::CompressorDetectorHp => "Detector HP",
            EffectParam::Guitar(kind, i) => kind.specs().get(i as usize).map(|s| s.name).unwrap_or(""),
            EffectParam::EqFreq => "Freq",
            EffectParam::EqGain => "Gain",
            EffectParam::EqQ => "Q",
            EffectParam::EqLowCut => "Low cut",
            EffectParam::EqLowFreq => "Low freq",
            EffectParam::EqLowGain => "Low gain",
            EffectParam::EqHighFreq => "High freq",
            EffectParam::EqHighGain => "High gain",
        }
    }

    /// Every param a modulator could target on `effect` - what a future
    /// "drop a modulator on this node" UI would list.
    pub fn for_effect(effect: Effect) -> &'static [EffectParam] {
        match effect {
            Effect::Compressor(_) => &[
                EffectParam::CompressorThreshold,
                EffectParam::CompressorRatio,
                EffectParam::CompressorAttack,
                EffectParam::CompressorRelease,
                EffectParam::CompressorMakeup,
                EffectParam::CompressorDry,
                EffectParam::CompressorDetectorHp,
            ],
            Effect::Guitar(g) => guitar_params(g.kind),
            Effect::Eq(_) => &[
                EffectParam::EqLowCut,
                EffectParam::EqLowFreq,
                EffectParam::EqLowGain,
                EffectParam::EqFreq,
                EffectParam::EqGain,
                EffectParam::EqQ,
                EffectParam::EqHighFreq,
                EffectParam::EqHighGain,
            ],
        }
    }
}

/// A specific knob on a specific effect node on a specific chain -
/// `owner: None` is the master bus, same convention as `Arrangement::fx`.
/// What a modulator (an LFO, or anything else later) would point at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModTarget {
    pub owner: Option<TrackId>,
    pub node: EffectNodeId,
    pub param: EffectParam,
}

#[cfg(test)]
mod effect_graph_tests {
    use super::*;

    fn compressor() -> Effect {
        Effect::Compressor(CompressorState::default())
    }

    /// `EffectGraph`'s derived `PartialEq` compares `edges` as an
    /// ordered `Vec`, but `move_before`'s remove-then-push bookkeeping
    /// doesn't preserve original edge order even when the resulting
    /// *set* of edges (and so the real, observable chain) is identical -
    /// compare edge sets instead for "these two graphs wire up the same
    /// way" assertions.
    fn same_wiring(a: &EffectGraph, b: &EffectGraph) -> bool {
        let sa: std::collections::HashSet<_> = a.edges.iter().collect();
        let sb: std::collections::HashSet<_> = b.edges.iter().collect();
        sa == sb
    }

    /// Every node has exactly one inbound and one outbound edge - the
    /// "still a strict line" invariant every phase before parallel
    /// branches relies on.
    fn assert_still_linear(graph: &EffectGraph) {
        for node in &graph.nodes {
            let inbound = graph.edges.iter().filter(|e| e.to == node.id).count();
            let outbound = graph.edges.iter().filter(|e| e.from == node.id).count();
            assert_eq!(inbound, 1, "node {} has {inbound} inbound edges", node.id);
            assert_eq!(outbound, 1, "node {} has {outbound} outbound edges", node.id);
        }
        // Exactly one edge into OUTPUT and one out of SOURCE.
        assert_eq!(graph.edges.iter().filter(|e| e.to == EffectGraph::OUTPUT).count(), 1);
        assert_eq!(graph.edges.iter().filter(|e| e.from == EffectGraph::SOURCE).count(), 1);
    }

    #[test]
    fn new_graph_is_just_source_to_output() {
        let graph = EffectGraph::new();
        assert!(graph.nodes.is_empty());
        assert!(graph.ordered().is_empty());
        assert_still_linear(&graph);
    }

    #[test]
    fn push_at_end_stays_linear_and_ordered() {
        let mut graph = EffectGraph::new();
        let a = graph.push_at_end(compressor());
        let b = graph.push_at_end(compressor());
        let c = graph.push_at_end(compressor());
        assert_still_linear(&graph);
        let ids: Vec<_> = graph.ordered().iter().map(|n| n.id).collect();
        assert_eq!(ids, vec![a, b, c]);
    }

    #[test]
    fn remove_middle_node_reconnects_neighbours() {
        let mut graph = EffectGraph::new();
        let a = graph.push_at_end(compressor());
        let b = graph.push_at_end(compressor());
        let c = graph.push_at_end(compressor());
        graph.remove(b);
        assert_still_linear(&graph);
        let ids: Vec<_> = graph.ordered().iter().map(|n| n.id).collect();
        assert_eq!(ids, vec![a, c]);
    }

    #[test]
    fn remove_then_reinsert_restores_exact_structure() {
        let mut graph = EffectGraph::new();
        let a = graph.push_at_end(compressor());
        let b = graph.push_at_end(compressor());
        let before = graph.clone();
        let (node, inbound, outbound) = graph.remove(b).unwrap();
        assert_ne!(graph, before);
        graph.reinsert(node, inbound, outbound);
        let ids: Vec<_> = graph.ordered().iter().map(|n| n.id).collect();
        assert_eq!(ids, vec![a, b]);
        assert_still_linear(&graph);
    }

    #[test]
    fn move_before_reorders_two_nodes() {
        // Compressor -> EQ, drag EQ's output onto Compressor's input:
        // matches the effects-board plan's own Phase 9 example exactly.
        let mut graph = EffectGraph::new();
        let comp = graph.push_at_end(compressor());
        let eq = graph.push_at_end(compressor());
        graph.move_before(eq, comp);
        assert_still_linear(&graph);
        let ids: Vec<_> = graph.ordered().iter().map(|n| n.id).collect();
        assert_eq!(ids, vec![eq, comp]);
    }

    #[test]
    fn move_before_to_own_successor_is_a_no_op() {
        let mut graph = EffectGraph::new();
        let a = graph.push_at_end(compressor());
        graph.push_at_end(compressor());
        let before = graph.clone();
        let successor = graph.successor_of(a).unwrap();
        graph.move_before(a, successor);
        assert!(same_wiring(&graph, &before));
    }

    #[test]
    fn move_before_then_undo_by_moving_back_restores_exact_structure() {
        let mut graph = EffectGraph::new();
        let a = graph.push_at_end(compressor());
        let b = graph.push_at_end(compressor());
        let c = graph.push_at_end(compressor());
        let before = graph.clone();
        // Move `a` (the first node) to just before `c` (the last) -
        // b, a, c - then move it back to just before its own old
        // successor (`b`) and confirm that's a full round-trip, the
        // same "move back to just before the old successor" inverse
        // `Command::RewireEffect`'s apply() relies on.
        let old_successor = graph.successor_of(a).unwrap();
        graph.move_before(a, c);
        assert!(!same_wiring(&graph, &before));
        graph.move_before(a, old_successor);
        assert!(same_wiring(&graph, &before));
        let _ = b;
    }

    #[test]
    fn from_flat_preserves_order_and_enabled_bits() {
        let slots = vec![
            EffectSlot { effect: compressor(), enabled: true },
            EffectSlot { effect: compressor(), enabled: false },
        ];
        let graph = EffectGraph::from_flat(slots);
        assert_still_linear(&graph);
        let ordered = graph.ordered();
        assert_eq!(ordered.len(), 2);
        assert!(ordered[0].enabled);
        assert!(!ordered[1].enabled);
    }

    #[test]
    fn connect_adds_a_parallel_branch_without_removing_the_existing_one() {
        // source -> a -> output, then connect source -> b too (fan-out)
        // and b -> output (fan-in) - a now runs in parallel with b.
        let mut graph = EffectGraph::new();
        let a = graph.push_at_end(compressor());
        let b = graph.push_at_end(compressor());
        // `b` currently sits after `a` in the line; disconnect it from
        // `a` and wire it in parallel instead: source -> b, b -> output.
        assert!(graph.disconnect(a, b));
        assert!(graph.connect(a, EffectGraph::OUTPUT));
        assert!(graph.connect(EffectGraph::SOURCE, b));
        // `b -> output` already exists from `push_at_end` and was never
        // touched above - no need (and no room) to add it again.

        // `a` still has its own source -> a -> output path.
        assert!(graph.edges.contains(&EffectEdge { from: EffectGraph::SOURCE, to: a }));
        assert!(graph.edges.contains(&EffectEdge { from: a, to: EffectGraph::OUTPUT }));
        // `b` now has its own, independent path too.
        assert!(graph.edges.contains(&EffectEdge { from: EffectGraph::SOURCE, to: b }));
        assert!(graph.edges.contains(&EffectEdge { from: b, to: EffectGraph::OUTPUT }));

        let order = graph.topo_order();
        assert_eq!(order.len(), 2);
        assert!(order.contains(&a));
        assert!(order.contains(&b));
    }

    #[test]
    fn connect_refuses_to_create_a_cycle() {
        let mut graph = EffectGraph::new();
        let a = graph.push_at_end(compressor());
        let b = graph.push_at_end(compressor());
        // a -> b already exists (the linear chain); wiring b -> a too
        // would be a cycle.
        assert!(graph.would_cycle(b, a));
        assert!(!graph.connect(b, a));
    }

    #[test]
    fn connect_refuses_a_duplicate_edge() {
        let mut graph = EffectGraph::new();
        let a = graph.push_at_end(compressor());
        assert!(!graph.connect(EffectGraph::SOURCE, a), "source -> a already exists from push_at_end");
    }

    #[test]
    fn disconnect_then_connect_round_trips() {
        let mut graph = EffectGraph::new();
        let a = graph.push_at_end(compressor());
        let before = graph.clone();
        assert!(graph.disconnect(EffectGraph::SOURCE, a));
        assert!(!same_wiring(&graph, &before));
        assert!(graph.connect(EffectGraph::SOURCE, a));
        assert!(same_wiring(&graph, &before));
    }

    #[test]
    fn topo_order_respects_dependencies_even_when_branching() {
        // Starts as source -> a -> c -> b -> output; rewire so both a
        // and b feed into c instead (fan-in), and c feeds output
        // directly: source -> a -> c -> output, source -> b -> c.
        let mut graph = EffectGraph::new();
        let a = graph.push_at_end(compressor());
        let c = graph.push_at_end(compressor());
        let b = graph.push_at_end(compressor());
        assert!(graph.disconnect(c, b));
        assert!(graph.disconnect(b, EffectGraph::OUTPUT));
        assert!(graph.connect(c, EffectGraph::OUTPUT));
        assert!(graph.connect(EffectGraph::SOURCE, b));
        assert!(graph.connect(b, c));

        let order = graph.topo_order();
        let pos = |id| order.iter().position(|n| *n == id).unwrap();
        assert!(pos(a) < pos(c));
        assert!(pos(b) < pos(c));
    }

    #[test]
    fn arrangement_fx_targets_track_or_master_by_none() {
        let mut arr = Arrangement::new(crate::arrangement::TempoMap::constant(
            120.0,
            crate::arrangement::TimeSignature::FOUR_FOUR,
        ));
        let track_id = arr.alloc_id();
        arr.tracks.push(Track {
            id: track_id,
            name: "Track".into(),
            color: ClipColor::Amber,
            kind: TrackKind::Audio,
            mute: false,
            solo: false,
            arm: false,
            gain_db: 0.0,
            height: 56.0,
            instrument: None,
            fx: EffectGraph::new(),
            drum_pads: Default::default(),
            effect_slots: vec![],
            effects: vec![],
        });

        arr.fx_mut(Some(track_id)).unwrap().push_at_end(compressor());
        arr.fx_mut(None).unwrap().push_at_end(compressor());

        assert_eq!(arr.fx(Some(track_id)).unwrap().ordered().len(), 1);
        assert_eq!(arr.fx(None).unwrap().ordered().len(), 1);
        assert_eq!(arr.master_effects.ordered().len(), 1, "None should target the real master_effects field");
        assert!(arr.fx(Some(track_id + 100)).is_none(), "an unknown track id should be None, not master");
    }

    #[test]
    fn mod_target_addresses_track_and_master_nodes_distinctly() {
        let track_target = ModTarget { owner: Some(1), node: 2, param: EffectParam::EqFreq };
        let master_target = ModTarget { owner: None, node: 2, param: EffectParam::EqFreq };
        assert_ne!(track_target, master_target, "same node/param on a track vs master must be distinct targets");

        let same_again = ModTarget { owner: Some(1), node: 2, param: EffectParam::EqFreq };
        assert_eq!(track_target, same_again);
    }

    #[test]
    fn effect_param_for_effect_matches_each_effects_own_knobs() {
        let compressor_params = EffectParam::for_effect(compressor());
        assert_eq!(compressor_params.len(), 7);
        assert!(compressor_params.contains(&EffectParam::CompressorThreshold));
        assert!(compressor_params.contains(&EffectParam::CompressorDry));

        let eq_params = EffectParam::for_effect(Effect::Eq(EqState::default()));
        assert_eq!(eq_params.len(), 8, "four bands: cut, shelf freq+gain, bell freq+gain+Q, shelf freq+gain");
        assert!(eq_params.contains(&EffectParam::EqFreq));
        assert!(!eq_params.contains(&EffectParam::CompressorThreshold), "an EQ node shouldn't offer Compressor knobs");

        for kind in crate::guitar::GuitarKind::ALL {
            let fx = Effect::Guitar(crate::guitar::GuitarFx::new(kind));
            let params = EffectParam::for_effect(fx);
            assert_eq!(params.len(), kind.specs().len(), "{}", kind.name());
            for &param in params {
                assert!(param.norm(&fx).is_some(), "{} {}", kind.name(), param.name());
                assert!(!param.format(&fx).is_empty());
            }
        }
    }
}

#[cfg(test)]
mod track_name_tests {
    use super::*;

    fn arrangement_with(names: &[&str]) -> Arrangement {
        let mut arr = crate::arrangement::empty_arrangement();
        for name in names {
            let id = arr.alloc_id();
            arr.tracks.push(Track {
                id,
                name: name.to_string(),
                color: ClipColor::Coral,
                kind: TrackKind::Audio,
                mute: false,
                solo: false,
                arm: false,
                gain_db: 0.0,
                height: DEFAULT_TRACK_HEIGHT,
                instrument: None,
                effects: vec![],
                effect_slots: vec![],
                fx: EffectGraph::new(),
                drum_pads: Default::default(),
            });
        }
        arr
    }

    #[test]
    fn numbers_per_kind_not_by_global_id() {
        let arr = arrangement_with(&["Audio 1", "Kick", "Snare"]);
        assert_eq!(arr.next_track_name(TrackKind::Audio), "Audio 2");
        assert_eq!(arr.next_track_name(TrackKind::Midi), "MIDI 1");
    }

    #[test]
    fn fills_the_lowest_gap_and_ignores_lookalikes() {
        let arr = arrangement_with(&["Audio 1", "Audio 3", "Audio 2b", "Audiophile 2"]);
        assert_eq!(arr.next_track_name(TrackKind::Audio), "Audio 2");
    }

    #[test]
    fn drum_tracks_are_named_drums_then_numbered() {
        let arr = arrangement_with(&["Drums", "Drums 3"]);
        assert_eq!(arr.name_for_instrument(Some(Instrument::Drums)), "Drums 2");
        assert_eq!(arrangement_with(&[]).name_for_instrument(Some(Instrument::Drums)), "Drums");
        assert_eq!(arr.name_for_instrument(Some(Instrument::Carve)), "MIDI 1");
    }

    #[test]
    fn only_names_strata_gave_follow_the_instrument() {
        for auto in ["MIDI 1", "MIDI 12", "Drums", "Drums 2"] {
            assert!(Arrangement::is_automatic_midi_name(auto), "{auto}");
        }
        for typed in ["MIDI", "Bass", "Drums loop", "MIDI one", "Drumsy"] {
            assert!(!Arrangement::is_automatic_midi_name(typed), "{typed}");
        }
    }
}

#[cfg(test)]
mod swing_tests {
    use super::*;

    fn clip(swing: f32) -> Clip {
        let s = PPQ / 4;
        let notes = (0..4).map(|i| MidiNote { start: i * s, length: s, pitch: 42, velocity: 100 }).collect();
        Clip {
            id: 1,
            track: 1,
            start: 0,
            length: 2 * PPQ,
            name: "Hats".into(),
            content: ClipContent::Midi { notes, loop_len: Some(PPQ), link: None },
            recording: false,
            gain_db: 0.0,
            swing,
        }
    }

    #[test]
    fn swing_pushes_only_the_off_beat_16ths_late() {
        let s = PPQ / 4;
        let starts = |c: &Clip| c.played_notes().iter().map(|n| n.start).collect::<Vec<_>>();
        assert_eq!(starts(&clip(0.0)), vec![0, s, 2 * s, 3 * s, PPQ, PPQ + s, PPQ + 2 * s, PPQ + 3 * s]);
        // Full swing: half a 16th late on the "e" and "a", every repeat.
        let late = s / 2;
        assert_eq!(starts(&clip(1.0)), vec![0, s + late, 2 * s, 3 * s + late, PPQ, PPQ + s + late, PPQ + 2 * s, PPQ + 3 * s + late]);
    }

    #[test]
    fn a_clip_saved_without_swing_loads_straight() {
        let mut json = serde_json::to_value(clip(0.5)).unwrap();
        json.as_object_mut().unwrap().remove("swing");
        let loaded: Clip = serde_json::from_value(json).unwrap();
        assert_eq!(loaded.swing, 0.0);
    }
}
