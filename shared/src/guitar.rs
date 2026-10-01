//! The guitar effects: Gate, Amp, Cabinet, Chorus, Tape Echo and Spring
//! Reverb. Each is one `GuitarFx` - a kind plus a few knob values - and
//! `specs` is the single table of what each knob is called, how far it
//! turns and how it reads out, so the device panel, automation lanes and
//! the engine all agree. The DSP lives in `engine::guitar`.

use serde::{Deserialize, Serialize};

/// The most knobs any guitar effect has.
pub const MAX_PARAMS: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GuitarKind {
    Gate,
    Amp,
    Cabinet,
    Chorus,
    Echo,
    Spring,
}

/// How a knob's value is written on its label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Db,
    /// Stored 0..1, shown 0..100%.
    Percent,
    Hz,
    Ms,
    /// 0..10, like the numbers on an amp.
    Dial,
}

#[derive(Clone, Copy, Debug)]
pub struct Param {
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    /// A log taper: the knob's middle is the geometric mean, not the average.
    pub log: bool,
    pub default: f32,
    pub unit: Unit,
}

const fn p(name: &'static str, min: f32, max: f32, log: bool, default: f32, unit: Unit) -> Param {
    Param { name, min, max, log, default, unit }
}

pub const GATE_THRESHOLD: usize = 0;
pub const GATE_RELEASE: usize = 1;

pub const AMP_GAIN: usize = 0;
pub const AMP_BASS: usize = 1;
pub const AMP_MID: usize = 2;
pub const AMP_TREBLE: usize = 3;
pub const AMP_VOLUME: usize = 4;
pub const AMP_REVERB: usize = 5;

pub const CAB_TONE: usize = 0;
pub const CAB_ROOM: usize = 1;

pub const CHORUS_MIX: usize = 0;
pub const CHORUS_RATE: usize = 1;
pub const CHORUS_DEPTH: usize = 2;
pub const CHORUS_SPREAD: usize = 3;

pub const ECHO_TIME: usize = 0;
pub const ECHO_FEEDBACK: usize = 1;
pub const ECHO_MIX: usize = 2;
pub const ECHO_TONE: usize = 3;
pub const ECHO_TAPE: usize = 4;

pub const SPRING_LENGTH: usize = 0;
pub const SPRING_TONE: usize = 1;
pub const SPRING_DRIP: usize = 2;
pub const SPRING_MIX: usize = 3;

const GATE: [Param; 2] = [
    p("Threshold", -80.0, -20.0, false, -62.0, Unit::Db),
    p("Release", 10.0, 500.0, true, 90.0, Unit::Ms),
];
const AMP: [Param; 6] = [
    p("Gain", 0.0, 10.0, false, 3.5, Unit::Dial),
    p("Bass", 0.0, 10.0, false, 5.0, Unit::Dial),
    p("Mid", 0.0, 10.0, false, 5.0, Unit::Dial),
    p("Treble", 0.0, 10.0, false, 5.0, Unit::Dial),
    p("Volume", -24.0, 12.0, false, 0.0, Unit::Db),
    p("Reverb", 0.0, 1.0, false, 0.12, Unit::Percent),
];
const CABINET: [Param; 2] = [p("Tone", 2000.0, 12_000.0, true, 6000.0, Unit::Hz), p("Room", 0.0, 1.0, false, 0.15, Unit::Percent)];
const CHORUS: [Param; 4] = [
    p("Blend", 0.0, 1.0, false, 0.5, Unit::Percent),
    p("Rate", 0.1, 8.0, true, 0.8, Unit::Hz),
    p("Depth", 0.0, 1.0, false, 0.5, Unit::Percent),
    p("Spread", 0.0, 1.0, false, 0.7, Unit::Percent),
];
const ECHO: [Param; 5] = [
    p("Time", 40.0, 1200.0, true, 380.0, Unit::Ms),
    p("Feedback", 0.0, 0.95, false, 0.35, Unit::Percent),
    p("Mix", 0.0, 1.0, false, 0.3, Unit::Percent),
    p("Tone", 800.0, 10_000.0, true, 3500.0, Unit::Hz),
    p("Tape", 0.0, 1.0, false, 0.3, Unit::Percent),
];
const SPRING: [Param; 4] = [
    p("Length", 0.0, 1.0, false, 0.5, Unit::Percent),
    p("Tone", 1000.0, 8000.0, true, 4000.0, Unit::Hz),
    p("Drip", 0.0, 1.0, false, 0.5, Unit::Percent),
    p("Mix", 0.0, 1.0, false, 0.3, Unit::Percent),
];

impl GuitarKind {
    pub const ALL: [GuitarKind; 6] =
        [GuitarKind::Gate, GuitarKind::Amp, GuitarKind::Cabinet, GuitarKind::Chorus, GuitarKind::Echo, GuitarKind::Spring];

    pub fn name(self) -> &'static str {
        match self {
            GuitarKind::Gate => "Gate",
            GuitarKind::Amp => "Amp",
            GuitarKind::Cabinet => "Cabinet",
            GuitarKind::Chorus => "Chorus",
            GuitarKind::Echo => "Tape Echo",
            GuitarKind::Spring => "Spring Reverb",
        }
    }

    /// One line for the browser: what it's for.
    pub fn blurb(self) -> &'static str {
        match self {
            GuitarKind::Gate => "Silences hum between notes",
            GuitarKind::Amp => "Drive and tone",
            GuitarKind::Cabinet => "The speaker and the room",
            GuitarKind::Chorus => "Thickens, or wobbles with Blend up",
            GuitarKind::Echo => "Repeats that age like tape",
            GuitarKind::Spring => "A dripping, splashy room",
        }
    }

    pub fn specs(self) -> &'static [Param] {
        match self {
            GuitarKind::Gate => &GATE,
            GuitarKind::Amp => &AMP,
            GuitarKind::Cabinet => &CABINET,
            GuitarKind::Chorus => &CHORUS,
            GuitarKind::Echo => &ECHO,
            GuitarKind::Spring => &SPRING,
        }
    }

    /// Where it sits in a guitar chain: a new effect goes before the first
    /// one with a higher rank (the amp belongs before the echo, not after).
    pub fn rank(self) -> u8 {
        self as u8
    }
}

/// One guitar effect's settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GuitarFx {
    pub kind: GuitarKind,
    pub values: [f32; MAX_PARAMS],
    /// Set only on the copy sent to the engine for a node that's switched
    /// off - never stored in the arrangement.
    #[serde(default, skip_serializing)]
    pub bypassed: bool,
}

impl GuitarFx {
    pub fn new(kind: GuitarKind) -> Self {
        let mut values = [0.0; MAX_PARAMS];
        for (v, spec) in values.iter_mut().zip(kind.specs()) {
            *v = spec.default;
        }
        Self { kind, values, bypassed: false }
    }

    pub fn bypass(self) -> Self {
        Self { bypassed: true, ..self }
    }

    pub fn param_count(&self) -> usize {
        self.kind.specs().len()
    }

    pub fn value(&self, i: usize) -> f32 {
        self.values.get(i).copied().unwrap_or(0.0)
    }

    pub fn norm(&self, i: usize) -> Option<f32> {
        let spec = self.kind.specs().get(i)?;
        Some(if spec.log {
            ((self.values[i].max(spec.min).ln() - spec.min.ln()) / (spec.max.ln() - spec.min.ln())).clamp(0.0, 1.0)
        } else {
            ((self.values[i] - spec.min) / (spec.max - spec.min)).clamp(0.0, 1.0)
        })
    }

    pub fn set_norm(&mut self, i: usize, norm: f32) {
        let Some(spec) = self.kind.specs().get(i) else { return };
        let n = norm.clamp(0.0, 1.0);
        self.values[i] =
            if spec.log { (spec.min.ln() + n * (spec.max.ln() - spec.min.ln())).exp() } else { spec.min + (spec.max - spec.min) * n };
    }

    pub fn format(&self, i: usize) -> String {
        let Some(spec) = self.kind.specs().get(i) else { return String::new() };
        let v = self.values[i];
        match spec.unit {
            Unit::Db => format!("{v:+.1} dB"),
            Unit::Percent => format!("{:.0}%", v * 100.0),
            Unit::Hz if v >= 1000.0 => format!("{:.1} kHz", v / 1000.0),
            Unit::Hz => format!("{v:.1} Hz"),
            Unit::Ms => format!("{v:.0} ms"),
            Unit::Dial => format!("{v:.1}"),
        }
    }
}

/// What "+ Guitar" puts on a new track: quiet the noise, the amp, the speaker.
pub fn starter_chain() -> [GuitarFx; 3] {
    [GuitarFx::new(GuitarKind::Gate), GuitarFx::new(GuitarKind::Amp), GuitarFx::new(GuitarKind::Cabinet)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_sits_inside_its_range_and_round_trips_through_the_knob() {
        for kind in GuitarKind::ALL {
            assert!(kind.specs().len() <= MAX_PARAMS);
            let fx = GuitarFx::new(kind);
            for (i, spec) in kind.specs().iter().enumerate() {
                assert!(spec.default >= spec.min && spec.default <= spec.max, "{} {}", kind.name(), spec.name);
                let mut copy = fx;
                copy.set_norm(i, fx.norm(i).unwrap());
                assert!((copy.value(i) - fx.value(i)).abs() <= 1.0e-3 * spec.max.abs().max(1.0), "{} {}", kind.name(), spec.name);
            }
        }
    }

    #[test]
    fn knob_ends_reach_the_range_ends() {
        let mut echo = GuitarFx::new(GuitarKind::Echo);
        echo.set_norm(ECHO_TIME, 0.0);
        assert!((echo.value(ECHO_TIME) - 40.0).abs() < 1.0e-3);
        echo.set_norm(ECHO_TIME, 1.0);
        assert!((echo.value(ECHO_TIME) - 1200.0).abs() < 0.1);
        assert_eq!(echo.format(ECHO_TIME), "1200 ms");
    }

    #[test]
    fn a_saved_effect_does_not_remember_being_bypassed() {
        let text = serde_json::to_string(&GuitarFx::new(GuitarKind::Amp).bypass()).unwrap();
        let back: GuitarFx = serde_json::from_str(&text).unwrap();
        assert!(!back.bypassed);
    }

    #[test]
    fn chain_order_puts_the_amp_before_the_echo() {
        assert!(GuitarKind::Amp.rank() < GuitarKind::Echo.rank());
        assert!(GuitarKind::Gate.rank() < GuitarKind::Amp.rank());
    }
}
