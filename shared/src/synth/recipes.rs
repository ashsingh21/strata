//! Carve presets for the sounds the recipe lessons build. Each one is the
//! lessons' Init patch with every recipe step applied at the middle of the
//! range its lesson accepts - loading the preset gives the sound you get
//! by finishing the lesson (the UI's course tests check they agree).

use super::model::{FilterType, LfoTarget, SynthState, VoiceMode, Waveform};

/// The LFO Rate knob position for `hz` (the inverse of `lfo_rate_hz`).
fn rate_for_hz(hz: f32) -> f32 {
    (hz / 0.05).ln() / (20.0f32 / 0.05).ln()
}

fn init(name: &'static str) -> SynthState {
    let mut s = crate::lessons::init_patch();
    s.name = name;
    s
}

pub fn deep_bass() -> SynthState {
    let mut s = init("Deep Bass");
    s.osc1.octave = -1;
    s.mix.sub_db = -6.0;
    s.filter.cutoff_hz = 250.0;
    s.filter.env_amount_oct = 2.5;
    s.filter_env.decay_ms = 200.0;
    s.filter.drive_db = 9.0;
    s.voice_mode = VoiceMode::Mono;
    s.output.glide_ms = 50.0;
    s
}

pub fn flute() -> SynthState {
    let mut s = init("Flute");
    s.osc1.waveform = Waveform::Triangle;
    s.mix.noise_db = -26.0;
    s.filter.cutoff_hz = 2500.0;
    s.amp_env.attack_ms = 80.0;
    s.lfo2.target = LfoTarget::Pitch;
    s.lfo2.rate_norm = rate_for_hz(5.0);
    s.lfo2.depth = 0.2;
    s.fx.reverb_mix = 0.3;
    s
}

pub fn indian_harp() -> SynthState {
    let mut s = init("Indian Harp");
    s.osc2.waveform = Waveform::Square;
    s.osc2.octave = 1;
    s.mix.osc2_db = -10.0;
    s.amp_env.attack_ms = 1.0;
    s.amp_env.sustain = 0.0;
    s.amp_env.decay_ms = 1000.0;
    s.amp_env.release_ms = 1000.0;
    s.filter.cutoff_hz = 1000.0;
    s.filter.env_amount_oct = 3.0;
    s.filter_env.decay_ms = 300.0;
    s.fx.chorus_mix = 0.3;
    s.fx.reverb_mix = 0.4;
    s.fx.reverb_size = 0.8;
    s
}

pub fn tanpura() -> SynthState {
    let mut s = init("Tanpura");
    s.amp_env.attack_ms = 2.0;
    s.amp_env.decay_ms = 1900.0;
    s.amp_env.sustain = 0.4;
    s.amp_env.release_ms = 1800.0;
    s.filter.cutoff_hz = 700.0;
    s.filter.resonance = 0.4;
    s.filter.env_amount_oct = 2.0;
    s.filter_env.attack_ms = 500.0;
    s.filter_env.decay_ms = 1500.0;
    s.filter.drive_db = 8.0;
    s.lfo1.target = LfoTarget::Cutoff;
    s.lfo1.rate_norm = rate_for_hz(0.3);
    s.lfo1.depth = 0.3;
    s.fx.chorus_mix = 0.3;
    s.fx.reverb_mix = 0.45;
    s.fx.reverb_size = 0.85;
    s
}

pub fn reed() -> SynthState {
    let mut s = init("Reed");
    s.osc1.waveform = Waveform::Square;
    s.mix.osc2_db = -9.0;
    s.filter.filter_type = FilterType::Bp;
    s.filter.cutoff_hz = 1700.0;
    s.filter.resonance = 0.4;
    s.mix.noise_db = -32.0;
    s.amp_env.attack_ms = 35.0;
    s.voice_mode = VoiceMode::Mono;
    s.output.glide_ms = 70.0;
    s.lfo2.target = LfoTarget::Pitch;
    s.lfo2.rate_norm = rate_for_hz(5.5);
    s.lfo2.depth = 0.15;
    s.fx.reverb_mix = 0.3;
    s
}

pub fn lead() -> SynthState {
    let mut s = init("Lead");
    s.osc1.waveform = Waveform::Square;
    s.mix.osc2_db = -8.0;
    s.osc2.knob_a_cents = 7.0;
    s.filter.cutoff_hz = 3000.0;
    s.filter.resonance = 0.3;
    s.voice_mode = VoiceMode::Mono;
    s.output.glide_ms = 60.0;
    s.lfo2.target = LfoTarget::Pitch;
    s.lfo2.rate_norm = rate_for_hz(5.0);
    s.lfo2.depth = 0.15;
    s.fx.chorus_mix = 0.2;
    s.fx.reverb_mix = 0.25;
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_for_hz_inverts_the_rate_knob() {
        for hz in [0.3, 1.0, 5.0, 5.5] {
            let back = crate::synth::lfo_rate_hz(rate_for_hz(hz));
            assert!((back - hz).abs() < 0.01, "{hz} -> {back}");
        }
    }
}
