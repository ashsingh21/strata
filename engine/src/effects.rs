//! `EffectUnit`/`EffectChain`: the engine-side generalization from "one
//! hardcoded Compressor per bus/slot" to "an ordered chain of N effect
//! units" - the plumbing shape needed before a second effect type
//! (`EffectUnit::Eq`, coming with the EQ) can exist at all, done as a
//! pure refactor first so it's verifiable with zero audible change.

use shared::synth::{EffectUnitState, MAX_EFFECTS_PER_CHAIN};

use crate::compressor::Compressor;

/// One DSP object in a chain - single-variant today (mirrors
/// `EffectUnitState`), so a second effect type is additive here too.
pub enum EffectUnit {
    Compressor(Compressor),
}

impl EffectUnit {
    fn set_state(&mut self, state: EffectUnitState) {
        match (self, state) {
            (EffectUnit::Compressor(c), EffectUnitState::Compressor(s)) => c.set_state(s),
        }
    }

    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        match self {
            EffectUnit::Compressor(c) => c.process(l, r),
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
}

impl EffectChain {
    pub fn new(sample_rate: f32) -> Self {
        let units = (0..MAX_EFFECTS_PER_CHAIN).map(|_| EffectUnit::Compressor(Compressor::new(sample_rate))).collect();
        Self { units, active: 0 }
    }

    /// Replaces every unit's state from an ordered snapshot - units
    /// beyond `count` just keep whatever state they last had (harmless:
    /// `active` means they're not run at all until reused).
    pub fn set_state(&mut self, count: u8, states: &[EffectUnitState; MAX_EFFECTS_PER_CHAIN]) {
        self.active = (count as usize).min(self.units.len());
        for (unit, state) in self.units.iter_mut().zip(states.iter()).take(self.active) {
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
