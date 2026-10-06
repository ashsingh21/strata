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

pub fn lofi_keys() -> SynthState {
    let mut s = init("Lo-fi Keys");
    s.osc1.waveform = Waveform::Sine;
    s.osc2.waveform = Waveform::Triangle;
    s.osc2.octave = 1;
    s.mix.osc2_db = -14.0;
    s.amp_env.attack_ms = 2.0;
    s.amp_env.decay_ms = 1500.0;
    s.amp_env.sustain = 0.15;
    s.amp_env.release_ms = 1500.0;
    s.filter.cutoff_hz = 1500.0;
    s.lfo2.target = LfoTarget::Pitch;
    s.lfo2.rate_norm = rate_for_hz(0.6);
    s.lfo2.depth = 0.08;
    s.fx.chorus_mix = 0.3;
    s.fx.reverb_mix = 0.3;
    s
}

/// A soft, warm piano (more Rhodes than concert grand - a subtractive
/// synth can't do a hammered string): a rounded triangle with a quiet sine
/// an octave up for the bell, a filter that opens on the attack and with
/// the pitch (high notes brighter, as on a real one), and a long fall to
/// silence while the key's held. Louder playing is louder.
pub fn piano() -> SynthState {
    let mut s = init("Piano");
    s.osc1.waveform = Waveform::Triangle;
    s.osc1.knob_b = 0.25;
    s.osc1.knob_c = 0.04;
    s.osc2.waveform = Waveform::Sine;
    s.osc2.octave = 1;
    s.osc2.knob_a_cents = 3.0;
    s.osc2.knob_c = 0.06;
    s.mix.osc1_db = -6.0;
    s.mix.osc2_db = -15.0;
    s.filter.filter_type = FilterType::Lp12;
    s.filter.cutoff_hz = 900.0;
    s.filter.resonance = 0.0;
    s.filter.env_amount_oct = 2.6;
    s.filter.key_track = 0.7;
    s.filter_env.attack_ms = 1.0;
    s.filter_env.decay_ms = 700.0;
    s.filter_env.sustain = 0.1;
    s.filter_env.release_ms = 400.0;
    s.amp_env.attack_ms = 1.0;
    s.amp_env.decay_ms = 2800.0;
    s.amp_env.sustain = 0.0;
    s.amp_env.release_ms = 380.0;
    s.fx.chorus_mix = 0.12;
    s.fx.chorus_depth = 0.3;
    s.fx.reverb_mix = 0.22;
    s.fx.reverb_size = 0.55;
    s.output.volume_db = -4.0;
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

/// A trap 808: a sine two octaves down (so it's written on the piano
/// roll's usual rows and sounds an octave under the bass), a little drive
/// so it still shows on phone speakers, a long boom that fades on its own,
/// and Mono with glide so an overlapping note slides into the next.
pub fn eight_oh_eight() -> SynthState {
    let mut s = init("808");
    s.osc1.waveform = Waveform::Sine;
    s.osc1.octave = -2;
    s.mix.osc1_db = -3.0;
    s.filter.cutoff_hz = 1200.0;
    s.filter.resonance = 0.0;
    s.filter.drive_db = 8.0;
    s.amp_env.attack_ms = 1.0;
    s.amp_env.decay_ms = 1600.0;
    s.amp_env.sustain = 0.3;
    s.amp_env.release_ms = 300.0;
    s.voice_mode = VoiceMode::Mono;
    s.output.glide_ms = 90.0;
    s.output.volume_db = -4.0;
    s
}
