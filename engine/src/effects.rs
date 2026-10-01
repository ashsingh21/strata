//! `EffectUnit`/`EffectChain`: the engine-side generalization from "one
//! hardcoded Compressor per bus/slot" to "an ordered chain of N effect
//! units" - the plumbing shape needed before a second effect type
//! (`EffectUnit::Eq`, coming with the EQ) can exist at all, done as a
//! pure refactor first so it's verifiable with zero audible change.

use shared::synth::{EffectUnitState, MAX_EFFECTS_PER_CHAIN};

use crate::compressor::Compressor;
use crate::eq::Eq;
use crate::guitar::{GuitarPool, GuitarUnit};

/// One DSP object in a chain - mirrors `EffectUnitState`.
pub enum EffectUnit {
    Compressor(Compressor),
    Eq(Eq),
    /// Borrowed from the `GuitarPool` for as long as the chain wants it.
    Guitar(Box<GuitarUnit>),
    /// A slot with nothing in it: passes the signal through.
    Empty,
}

impl EffectUnit {
    /// A unit for `state`, or `Empty` if it's a guitar effect and the pool
    /// has none left.
    fn new_for(state: &EffectUnitState, sample_rate: f32, pool: &mut GuitarPool) -> Self {
        match state {
            EffectUnitState::Compressor(_) => EffectUnit::Compressor(Compressor::new(sample_rate)),
            EffectUnitState::Eq(_) => EffectUnit::Eq(Eq::new(sample_rate)),
            EffectUnitState::Guitar(g) => pool.take(g.kind).map_or(EffectUnit::Empty, EffectUnit::Guitar),
        }
    }

    /// Replaces this slot's config in place when the unit type already
    /// matches; the caller (`EffectChain::set_state`) swaps in a fresh
    /// unit first when it doesn't - which loses that slot's running
    /// state (an envelope, a filter's history), but that only ever
    /// happens when the chain's actual effect *types* change (adding,
    /// removing or reordering effects), not on every routine params
    /// update, so continuity is preserved for the common case.
    fn set_state(&mut self, state: &EffectUnitState) {
        match (self, state) {
            (EffectUnit::Compressor(c), EffectUnitState::Compressor(s)) => c.set_state(*s),
            (EffectUnit::Eq(e), EffectUnitState::Eq(s)) => e.set_state(*s),
            (EffectUnit::Guitar(g), EffectUnitState::Guitar(s)) => g.set(s),
            _ => {}
        }
    }

    /// Whether this unit is what `state` asks for. A slot left `Empty`
    /// because the pool ran dry counts as matching a guitar effect, so
    /// it isn't retried every block.
    fn matches(&self, state: &EffectUnitState) -> bool {
        match (self, state) {
            (EffectUnit::Compressor(_), EffectUnitState::Compressor(_)) | (EffectUnit::Eq(_), EffectUnitState::Eq(_)) => true,
            (EffectUnit::Guitar(g), EffectUnitState::Guitar(s)) => g.kind() == s.kind,
            _ => false,
        }
    }

    /// If this is a guitar unit, returns it to the pool and leaves the slot
    /// empty. (Compressors and EQs own no heap, so dropping them is free.)
    fn give_back(&mut self, pool: &mut GuitarPool) {
        if let EffectUnit::Guitar(_) = self {
            if let EffectUnit::Guitar(g) = std::mem::replace(self, EffectUnit::Empty) {
                pool.give(g);
            }
        }
    }

    #[inline]
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        match self {
            EffectUnit::Compressor(c) => c.process(l, r),
            EffectUnit::Eq(e) => e.process(l, r),
            EffectUnit::Guitar(g) => g.process(l, r),
            EffectUnit::Empty => (l, r),
        }
    }
}

/// A fixed-capacity, pre-allocated-once ordered chain of effect units,
/// one per bus/slot - persistent across blocks (not rebuilt per clip or
/// per block) so each unit's own envelope/state carries over smoothly,
/// same reasoning the old single `Compressor` per slot already had.
pub struct EffectChain {
    units: Vec<EffectUnit>,
    active: usize,
    sample_rate: f32,
}

impl EffectChain {
    pub fn new(sample_rate: f32) -> Self {
        let units = (0..MAX_EFFECTS_PER_CHAIN).map(|_| EffectUnit::Compressor(Compressor::new(sample_rate))).collect();
        Self { units, active: 0, sample_rate }
    }

    /// Replaces every unit's state from an ordered snapshot. A slot whose
    /// effect *type* changed since the last snapshot gets a fresh unit of
    /// the new type first - see `EffectUnit::set_state`'s own doc comment
    /// for why that's the right trade-off. Guitar units come from, and
    /// go back to, `pool`: a slot that's no longer used gives its unit
    /// back, so a track that loses its amp frees it for another.
    pub fn set_state(&mut self, count: u8, states: &[EffectUnitState; MAX_EFFECTS_PER_CHAIN], pool: &mut GuitarPool) {
        self.active = (count as usize).min(self.units.len());
        for (i, (unit, state)) in self.units.iter_mut().zip(states.iter()).enumerate() {
            if i >= self.active {
                unit.give_back(pool);
                continue;
            }
            if !unit.matches(state) {
                unit.give_back(pool);
                *unit = EffectUnit::new_for(state, self.sample_rate, pool);
            }
            unit.set_state(state);
        }
    }

    /// Gives every guitar unit back (the chain is going away, or its track has).
    pub fn release(&mut self, pool: &mut GuitarPool) {
        self.active = 0;
        for unit in &mut self.units {
            unit.give_back(pool);
        }
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let mut out = (l, r);
        for unit in self.units.iter_mut().take(self.active) {
            out = unit.process(out.0, out.1);
        }
        out
    }
}
