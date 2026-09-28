//! Every continuously controllable Carve parameter, with its normalized
//! 0..1 range, how to apply it, and how to display it - one table shared
//! by the synth panel's knobs and automation lanes, so a knob and the lane
//! automating it can never disagree about what a position means.

use serde::{Deserialize, Serialize};

use super::bridge::lfo_rate_hz;
use super::model::{SynthState, MAX_UNISON};

/// Osc octave knobs span -3..+3.
const OCTAVE_RANGE: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SynthParam {
    Osc1Octave,
    Osc1Tune,
    Osc1Shape,
    Osc1Drift,
    Osc2Octave,
    Osc2Detune,
    Osc2PulseWidth,
    Osc2Fm,
    Osc1Level,
    Osc2Level,
    SubLevel,
    NoiseLevel,
    Cutoff,
    Resonance,
    Drive,
    EnvAmount,
    KeyTrack,
    FilterAttack,
    FilterDecay,
    FilterSustain,
    FilterRelease,
    AmpAttack,
    AmpDecay,
    AmpSustain,
    AmpRelease,
    Lfo1Rate,
    Lfo1Depth,
    Lfo2Rate,
    Lfo2Depth,
    UnisonVoices,
    UnisonDetune,
    UnisonWidth,
    ChorusMix,
    ChorusDepth,
    ReverbMix,
    ReverbSize,
    Volume,
    Glide,
}

fn lin_norm(min: f32, max: f32, v: f32) -> f32 {
    ((v - min) / (max - min)).clamp(0.0, 1.0)
}
fn lin_value(min: f32, max: f32, n: f32) -> f32 {
    min + (max - min) * n.clamp(0.0, 1.0)
}
fn log_norm(min: f32, max: f32, v: f32) -> f32 {
    ((v.max(min).ln() - min.ln()) / (max.ln() - min.ln())).clamp(0.0, 1.0)
}
fn log_value(min: f32, max: f32, n: f32) -> f32 {
    (min.ln() + n.clamp(0.0, 1.0) * (max.ln() - min.ln())).exp()
}
fn pct(v: f32) -> String {
    format!("{:.0}%", v * 100.0)
}

pub fn format_hz(hz: f32) -> String {
    if hz >= 1000.0 {
        format!("{:.2} kHz", hz / 1000.0)
    } else {
        format!("{hz:.0} Hz")
    }
}

impl SynthParam {
    /// The knob's own label (unique within its panel section).
    pub fn name(self) -> &'static str {
        use SynthParam::*;
        match self {
            Osc1Octave | Osc2Octave => "Octave",
            Osc1Tune => "Tune",
            Osc1Shape => "Shape",
            Osc1Drift => "Drift",
            Osc2Detune | UnisonDetune => "Detune",
            Osc2PulseWidth => "Shape",
            Osc2Fm => "FM",
            Osc1Level => "Osc 1",
            Osc2Level => "Osc 2",
            SubLevel => "Sub",
            NoiseLevel => "Noise",
            Cutoff => "Cutoff",
            Resonance => "Resonance",
            Drive => "Drive",
            EnvAmount => "Env amount",
            KeyTrack => "Key track",
            FilterAttack | AmpAttack => "Attack",
            FilterDecay | AmpDecay => "Decay",
            FilterSustain | AmpSustain => "Sustain",
            FilterRelease | AmpRelease => "Release",
            Lfo1Rate | Lfo2Rate => "Rate",
            Lfo1Depth | Lfo2Depth | ChorusDepth => "Depth",
            UnisonVoices => "Voices",
            UnisonWidth => "Width",
            ChorusMix => "Chorus",
            ReverbMix => "Reverb",
            ReverbSize => "Size",
            Volume => "Volume",
            Glide => "Glide",
        }
    }

    /// Unambiguous across the whole synth - an automation lane's label.
    pub fn long_name(self) -> &'static str {
        use SynthParam::*;
        match self {
            Osc1Octave => "Osc 1 octave",
            Osc1Tune => "Osc 1 tune",
            Osc1Shape => "Osc 1 shape",
            Osc1Drift => "Osc 1 drift",
            Osc2Octave => "Osc 2 octave",
            Osc2Detune => "Osc 2 detune",
            Osc2PulseWidth => "Osc 2 shape",
            Osc2Fm => "Osc 2 FM",
            Osc1Level => "Osc 1 level",
            Osc2Level => "Osc 2 level",
            SubLevel => "Sub level",
            NoiseLevel => "Noise level",
            Cutoff => "Cutoff",
            Resonance => "Resonance",
            Drive => "Drive",
            EnvAmount => "Filter env amount",
            KeyTrack => "Key track",
            FilterAttack => "Filter env attack",
            FilterDecay => "Filter env decay",
            FilterSustain => "Filter env sustain",
            FilterRelease => "Filter env release",
            AmpAttack => "Amp attack",
            AmpDecay => "Amp decay",
            AmpSustain => "Amp sustain",
            AmpRelease => "Amp release",
            Lfo1Rate => "LFO 1 rate",
            Lfo1Depth => "LFO 1 depth",
            Lfo2Rate => "LFO 2 rate",
            Lfo2Depth => "LFO 2 depth",
            UnisonVoices => "Unison voices",
            UnisonDetune => "Unison detune",
            UnisonWidth => "Unison width",
            ChorusMix => "Chorus",
            ChorusDepth => "Chorus depth",
            ReverbMix => "Reverb",
            ReverbSize => "Reverb size",
            Volume => "Volume",
            Glide => "Glide",
        }
    }

    pub fn norm(self, s: &SynthState) -> f32 {
        use SynthParam::*;
        let unison_max = MAX_UNISON as f32;
        match self {
            Osc1Octave => lin_norm(-OCTAVE_RANGE, OCTAVE_RANGE, s.osc1.octave as f32),
            Osc1Tune => lin_norm(-100.0, 100.0, s.osc1.knob_a_cents),
            Osc1Shape => s.osc1.knob_b,
            Osc1Drift => s.osc1.knob_c,
            Osc2Octave => lin_norm(-OCTAVE_RANGE, OCTAVE_RANGE, s.osc2.octave as f32),
            Osc2Detune => lin_norm(-50.0, 50.0, s.osc2.knob_a_cents),
            Osc2PulseWidth => s.osc2.knob_b,
            Osc2Fm => s.osc2.knob_c,
            Osc1Level => lin_norm(-60.0, 0.0, s.mix.osc1_db),
            Osc2Level => lin_norm(-60.0, 0.0, s.mix.osc2_db),
            SubLevel => lin_norm(-60.0, 0.0, s.mix.sub_db),
            NoiseLevel => lin_norm(-60.0, 0.0, s.mix.noise_db),
            Cutoff => log_norm(20.0, 20_000.0, s.filter.cutoff_hz),
            Resonance => s.filter.resonance,
            Drive => lin_norm(0.0, 24.0, s.filter.drive_db),
            EnvAmount => lin_norm(-4.0, 4.0, s.filter.env_amount_oct),
            KeyTrack => s.filter.key_track,
            FilterAttack => log_norm(1.0, 2000.0, s.filter_env.attack_ms),
            FilterDecay => log_norm(1.0, 2000.0, s.filter_env.decay_ms),
            FilterSustain => s.filter_env.sustain,
            FilterRelease => log_norm(1.0, 2000.0, s.filter_env.release_ms),
            AmpAttack => log_norm(1.0, 2000.0, s.amp_env.attack_ms),
            AmpDecay => log_norm(1.0, 2000.0, s.amp_env.decay_ms),
            AmpSustain => s.amp_env.sustain,
            AmpRelease => log_norm(1.0, 2000.0, s.amp_env.release_ms),
            Lfo1Rate => s.lfo1.rate_norm,
            Lfo1Depth => s.lfo1.depth,
            Lfo2Rate => s.lfo2.rate_norm,
            Lfo2Depth => s.lfo2.depth,
            UnisonVoices => lin_norm(1.0, unison_max, s.unison.voices as f32),
            UnisonDetune => lin_norm(0.0, 50.0, s.unison.detune_cents),
            UnisonWidth => s.unison.width,
            ChorusMix => s.fx.chorus_mix,
            ChorusDepth => s.fx.chorus_depth,
            ReverbMix => s.fx.reverb_mix,
            ReverbSize => s.fx.reverb_size,
            Volume => lin_norm(-60.0, 6.0, s.output.volume_db),
            Glide => log_norm(1.0, 500.0, s.output.glide_ms),
        }
    }

    pub fn apply_norm(self, s: &mut SynthState, n: f32) {
        use SynthParam::*;
        let n = n.clamp(0.0, 1.0);
        let unison_max = MAX_UNISON as f32;
        match self {
            Osc1Octave => s.osc1.octave = lin_value(-OCTAVE_RANGE, OCTAVE_RANGE, n).round() as i8,
            Osc1Tune => s.osc1.knob_a_cents = lin_value(-100.0, 100.0, n),
            Osc1Shape => s.osc1.knob_b = n,
            Osc1Drift => s.osc1.knob_c = n,
            Osc2Octave => s.osc2.octave = lin_value(-OCTAVE_RANGE, OCTAVE_RANGE, n).round() as i8,
            Osc2Detune => s.osc2.knob_a_cents = lin_value(-50.0, 50.0, n),
            Osc2PulseWidth => s.osc2.knob_b = n,
            Osc2Fm => s.osc2.knob_c = n,
            Osc1Level => s.mix.osc1_db = lin_value(-60.0, 0.0, n),
            Osc2Level => s.mix.osc2_db = lin_value(-60.0, 0.0, n),
            SubLevel => s.mix.sub_db = lin_value(-60.0, 0.0, n),
            NoiseLevel => s.mix.noise_db = lin_value(-60.0, 0.0, n),
            Cutoff => s.filter.cutoff_hz = log_value(20.0, 20_000.0, n),
            Resonance => s.filter.resonance = n,
            Drive => s.filter.drive_db = lin_value(0.0, 24.0, n),
            EnvAmount => s.filter.env_amount_oct = lin_value(-4.0, 4.0, n),
            KeyTrack => s.filter.key_track = n,
            FilterAttack => s.filter_env.attack_ms = log_value(1.0, 2000.0, n),
            FilterDecay => s.filter_env.decay_ms = log_value(1.0, 2000.0, n),
            FilterSustain => s.filter_env.sustain = n,
            FilterRelease => s.filter_env.release_ms = log_value(1.0, 2000.0, n),
            AmpAttack => s.amp_env.attack_ms = log_value(1.0, 2000.0, n),
            AmpDecay => s.amp_env.decay_ms = log_value(1.0, 2000.0, n),
            AmpSustain => s.amp_env.sustain = n,
            AmpRelease => s.amp_env.release_ms = log_value(1.0, 2000.0, n),
            Lfo1Rate => s.lfo1.rate_norm = n,
            Lfo1Depth => s.lfo1.depth = n,
            Lfo2Rate => s.lfo2.rate_norm = n,
            Lfo2Depth => s.lfo2.depth = n,
            UnisonVoices => s.unison.voices = lin_value(1.0, unison_max, n).round() as u8,
            UnisonDetune => s.unison.detune_cents = lin_value(0.0, 50.0, n),
            UnisonWidth => s.unison.width = n,
            ChorusMix => s.fx.chorus_mix = n,
            ChorusDepth => s.fx.chorus_depth = n,
            ReverbMix => s.fx.reverb_mix = n,
            ReverbSize => s.fx.reverb_size = n,
            Volume => s.output.volume_db = lin_value(-60.0, 6.0, n),
            Glide => s.output.glide_ms = log_value(1.0, 500.0, n),
        }
    }

    /// The value on `s`, formatted as its knob shows it.
    pub fn format(self, s: &SynthState) -> String {
        use SynthParam::*;
        match self {
            Osc1Octave => format!("{:+}", s.osc1.octave),
            Osc2Octave => format!("{:+}", s.osc2.octave),
            Osc1Tune => format!("{:.0} ct", s.osc1.knob_a_cents),
            Osc1Shape => pct(s.osc1.knob_b),
            Osc1Drift => pct(s.osc1.knob_c),
            Osc2Detune => format!("{:+.0} ct", s.osc2.knob_a_cents),
            Osc2PulseWidth => pct(s.osc2.knob_b),
            Osc2Fm => pct(s.osc2.knob_c),
            Osc1Level => format!("{:.1} dB", s.mix.osc1_db),
            Osc2Level => format!("{:.1} dB", s.mix.osc2_db),
            SubLevel => format!("{:.1} dB", s.mix.sub_db),
            NoiseLevel => format!("{:.1} dB", s.mix.noise_db),
            Cutoff => format_hz(s.filter.cutoff_hz),
            Resonance => pct(s.filter.resonance),
            Drive => format!("{:+.1} dB", s.filter.drive_db),
            EnvAmount => format!("{:+.1} oct", s.filter.env_amount_oct),
            KeyTrack => pct(s.filter.key_track),
            FilterAttack => format!("{:.0} ms", s.filter_env.attack_ms),
            FilterDecay => format!("{:.0} ms", s.filter_env.decay_ms),
            FilterSustain => pct(s.filter_env.sustain),
            FilterRelease => format!("{:.0} ms", s.filter_env.release_ms),
            AmpAttack => format!("{:.0} ms", s.amp_env.attack_ms),
            AmpDecay => format!("{:.0} ms", s.amp_env.decay_ms),
            AmpSustain => pct(s.amp_env.sustain),
            AmpRelease => format!("{:.0} ms", s.amp_env.release_ms),
            Lfo1Rate => format!("{:.2} Hz", lfo_rate_hz(s.lfo1.rate_norm)),
            Lfo2Rate => format!("{:.2} Hz", lfo_rate_hz(s.lfo2.rate_norm)),
            Lfo1Depth => pct(s.lfo1.depth),
            Lfo2Depth => pct(s.lfo2.depth),
            UnisonVoices => {
                if s.unison.voices <= 1 {
                    "Off".to_string()
                } else {
                    format!("{}", s.unison.voices)
                }
            }
            UnisonDetune => format!("{:.0} ct", s.unison.detune_cents),
            UnisonWidth => pct(s.unison.width),
            ChorusMix => pct(s.fx.chorus_mix),
            ChorusDepth => pct(s.fx.chorus_depth),
            ReverbMix => pct(s.fx.reverb_mix),
            ReverbSize => pct(s.fx.reverb_size),
            Volume => format!("{:.1} dB", s.output.volume_db),
            Glide => format!("{:.0} ms", s.output.glide_ms),
        }
    }

    /// `norm` formatted as this param's readout, independent of any patch
    /// (every format depends only on the param's own value).
    pub fn format_norm(self, norm: f32) -> String {
        let mut s = super::model::seed_synth();
        self.apply_norm(&mut s, norm);
        self.format(&s)
    }

    pub const ALL: [SynthParam; 38] = {
        use SynthParam::*;
        [
            Osc1Octave, Osc1Tune, Osc1Shape, Osc1Drift, Osc2Octave, Osc2Detune, Osc2PulseWidth, Osc2Fm, Osc1Level,
            Osc2Level, SubLevel, NoiseLevel, Cutoff, Resonance, Drive, EnvAmount, KeyTrack, FilterAttack, FilterDecay,
            FilterSustain, FilterRelease, AmpAttack, AmpDecay, AmpSustain, AmpRelease, Lfo1Rate, Lfo1Depth, Lfo2Rate,
            Lfo2Depth, UnisonVoices, UnisonDetune, UnisonWidth, ChorusMix, ChorusDepth, ReverbMix, ReverbSize, Volume,
            Glide,
        ]
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuous_params_round_trip_through_norm() {
        let discrete = [SynthParam::Osc1Octave, SynthParam::Osc2Octave, SynthParam::UnisonVoices];
        for p in SynthParam::ALL {
            let mut s = crate::synth::seed_synth();
            if discrete.contains(&p) {
                // Stepped: the value is whole steps, and a step's own
                // position maps back to itself.
                p.apply_norm(&mut s, 0.5);
                let n = p.norm(&s);
                let mut again = s.clone();
                p.apply_norm(&mut again, n);
                assert_eq!(p.format(&again), p.format(&s), "{p:?}");
                continue;
            }
            for n in [0.0, 0.37, 1.0] {
                p.apply_norm(&mut s, n);
                assert!((p.norm(&s) - n).abs() < 1e-4, "{p:?} {n} -> {}", p.norm(&s));
            }
        }
    }

    #[test]
    fn long_names_are_unique() {
        let mut names: Vec<_> = SynthParam::ALL.iter().map(|p| p.long_name()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), SynthParam::ALL.len());
    }
}
