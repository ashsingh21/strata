//! `EffectUnit`/`EffectChain`: the engine-side generalization from "one
//! hardcoded Compressor per bus/slot" to "an ordered chain of N effect
//! units" - the plumbing shape needed before a second effect type
//! (`EffectUnit::Eq`, coming with the EQ) can exist at all, done as a
//! pure refactor first so it's verifiable with zero audible change.

use shared::synth::{EffectUnitState, MAX_EFFECTS_PER_CHAIN};

use crate::compressor::Compressor;
use crate::eq::Eq;

/// One DSP object in a chain - mirrors `EffectUnitState`.
pub enum EffectUnit {
    Compressor(Compressor),
    Eq(Eq),
}

impl EffectUnit {
    fn new_for(state: EffectUnitState, sample_rate: f32) -> Self {
        match state {
            EffectUnitState::Compressor(_) => EffectUnit::Compressor(Compressor::new(sample_rate)),
            EffectUnitState::Eq(_) => EffectUnit::Eq(Eq::new(sample_rate)),
        }
    }

    /// Replaces this slot's config in place when the unit type already
    /// matches; the caller (`EffectChain::set_state`) swaps in a fresh
    /// unit first when it doesn't - which loses that slot's running
    /// state (an envelope, a filter's history), but that only ever
    /// happens when the chain's actual effect *types* change (adding,
    /// removing or reordering effects), not on every routine params
    /// update, so continuity is preserved for the common case.
    fn set_state(&mut self, state: EffectUnitState) {
        match (self, state) {
            (EffectUnit::Compressor(c), EffectUnitState::Compressor(s)) => c.set_state(s),
            (EffectUnit::Eq(e), EffectUnitState::Eq(s)) => e.set_state(s),
            _ => {}
        }
    }

    fn matches(&self, state: &EffectUnitState) -> bool {
        matches!(
            (self, state),
            (EffectUnit::Compressor(_), EffectUnitState::Compressor(_)) | (EffectUnit::Eq(_), EffectUnitState::Eq(_))
        )
    }

    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        match self {
            EffectUnit::Compressor(c) => c.process(l, r),
            EffectUnit::Eq(e) => e.process(l, r),
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

    /// Replaces every unit's state from an ordered snapshot - units
    /// beyond `count` just keep whatever state they last had (harmless:
    /// `active` means they're not run at all until reused). A slot whose
    /// effect *type* changed since the last snapshot gets a fresh unit
    /// of the new type first - see `EffectUnit::set_state`'s own doc
    /// comment for why that's the right trade-off.
    pub fn set_state(&mut self, count: u8, states: &[EffectUnitState; MAX_EFFECTS_PER_CHAIN]) {
        self.active = (count as usize).min(self.units.len());
        for (unit, state) in self.units.iter_mut().zip(states.iter()).take(self.active) {
            if !unit.matches(state) {
                *unit = EffectUnit::new_for(*state, self.sample_rate);
            }
            unit.set_state(*state);
        }
    }

    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let mut out = (l, r);
        for unit in self.units.iter_mut().take(self.active) {
            out = unit.process(out.0, out.1);
        }
        out
    }
}
