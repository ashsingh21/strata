//! Interval Input's own (small) state: which key and scale the three
//! panels are exploring. Note on/off isn't tracked here at all - tapping a
//! pad in any panel emits the same `SynthEvent::ToggleKey` the on-screen
//! keyboard does, and all three panels read Carve's own `held_notes` back
//! (via `SynthState`) to know what to highlight. That keeps this tool,
//! the keyboard, and step-entry recording all looking at one shared
//! "what's currently held" truth.

use vizia::prelude::*;

use shared::theory::SCALE_PRESETS;

pub struct IntervalInputModel {
    /// Root pitch class, 0..12 (0 = C).
    pub key: Signal<u8>,
    /// Bit `n` set means semitone `n` above the root is in scale.
    pub scale_mask: Signal<u16>,
}

pub enum IntervalInputEvent {
    SetKey(u8),
    ToggleDegree(u8),
    CyclePreset,
}

impl IntervalInputModel {
    pub fn new() -> Self {
        let minor_pentatonic = SCALE_PRESETS.iter().find(|p| p.name == "Minor pentatonic").unwrap();
        Self {
            key: Signal::new(9), // A
            scale_mask: Signal::new(minor_pentatonic.mask),
        }
    }
}

impl Default for IntervalInputModel {
    fn default() -> Self {
        Self::new()
    }
}

impl Model for IntervalInputModel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            IntervalInputEvent::SetKey(key) => self.key.set(key % 12),
            IntervalInputEvent::ToggleDegree(degree) => {
                if *degree != 0 {
                    self.scale_mask.update(|m| *m ^= 1 << (degree % 12));
                }
            }
            IntervalInputEvent::CyclePreset => {
                let current = self.scale_mask.get();
                let next_index = SCALE_PRESETS
                    .iter()
                    .position(|p| p.mask == current)
                    .map(|i| (i + 1) % SCALE_PRESETS.len())
                    .unwrap_or(0);
                self.scale_mask.set(SCALE_PRESETS[next_index].mask);
            }
        });
    }
}

/// The current scale's name, or "Custom" if the mask doesn't exactly match
/// a preset (e.g. after toggling an individual degree).
pub fn scale_name(mask: u16) -> &'static str {
    SCALE_PRESETS.iter().find(|p| p.mask == mask).map(|p| p.name).unwrap_or("Custom")
}
