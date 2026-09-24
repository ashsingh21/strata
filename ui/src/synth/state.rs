//! Carve's `Model`. Numeric knobs share one generic `Update` event (a
//! closure mutating `SynthState`) built at the call site once the knob's
//! own 0..1 position has been mapped to physical units - avoids a
//! multi-dozen-variant event enum for ~25 knobs. Enum/bool controls
//! (waveform, filter type, sync, voice mode, held keys) get their own
//! variants since they aren't naturally a knob position.

use vizia::prelude::*;

use shared::synth::{seed_synth, FilterType, SynthState, VoiceMode, Waveform};

pub enum SynthEvent {
    Update(Box<dyn Fn(&mut SynthState) + Send>),
    SetOsc1Waveform(Waveform),
    SetOsc2Waveform(Waveform),
    ToggleOsc2Sync,
    SetFilterType(FilterType),
    ToggleLfo1Sync,
    ToggleLfo2Sync,
    SetVoiceMode(VoiceMode),
    ToggleKey(u8),
    /// Advances the LFO scope's animated phase; `dt` in seconds.
    Tick(f32),
}

pub struct SynthModel {
    pub state: Signal<SynthState>,
    pub lfo_scope_phase: Signal<f32>,
}

impl SynthModel {
    pub fn new() -> Self {
        Self { state: Signal::new(seed_synth()), lfo_scope_phase: Signal::new(0.0) }
    }
}

impl Default for SynthModel {
    fn default() -> Self {
        Self::new()
    }
}

const LFO_SCOPE_HZ: f32 = 0.5;

impl Model for SynthModel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            SynthEvent::Update(f) => self.state.update(|s| f(s)),
            SynthEvent::SetOsc1Waveform(w) => self.state.update(|s| s.osc1.waveform = *w),
            SynthEvent::SetOsc2Waveform(w) => self.state.update(|s| s.osc2.waveform = *w),
            SynthEvent::ToggleOsc2Sync => self.state.update(|s| s.osc2.sync = !s.osc2.sync),
            SynthEvent::SetFilterType(t) => self.state.update(|s| s.filter.filter_type = *t),
            SynthEvent::ToggleLfo1Sync => self.state.update(|s| s.lfo1.sync = !s.lfo1.sync),
            SynthEvent::ToggleLfo2Sync => self.state.update(|s| s.lfo2.sync = !s.lfo2.sync),
            SynthEvent::SetVoiceMode(m) => self.state.update(|s| s.voice_mode = *m),
            SynthEvent::ToggleKey(note) => self.state.update(|s| {
                if let Some(pos) = s.held_notes.iter().position(|n| n == note) {
                    s.held_notes.remove(pos);
                } else {
                    s.held_notes.push(*note);
                }
            }),
            SynthEvent::Tick(dt) => {
                self.lfo_scope_phase.update(|p| {
                    *p = (*p + LFO_SCOPE_HZ * std::f32::consts::TAU * dt) % std::f32::consts::TAU;
                });
            }
        });
    }
}

// --- Knob position <-> physical unit mappings, shared by every knob. -----

pub fn lin(pos: f32, min: f32, max: f32) -> f32 {
    min + pos.clamp(0.0, 1.0) * (max - min)
}

pub fn lin_inv(value: f32, min: f32, max: f32) -> f32 {
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

/// Exponential mapping for things that feel right on a log scale (Hz, ms).
pub fn log(pos: f32, min: f32, max: f32) -> f32 {
    (min.ln() + pos.clamp(0.0, 1.0) * (max.ln() - min.ln())).exp()
}

pub fn log_inv(value: f32, min: f32, max: f32) -> f32 {
    ((value.max(min).ln() - min.ln()) / (max.ln() - min.ln())).clamp(0.0, 1.0)
}
