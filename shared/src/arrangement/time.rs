//! Musical time: integer ticks at 960 PPQ, and a tempo map that converts
//! between ticks, seconds and samples. Positions and lengths in the
//! arrangement are always ticks - never floats or seconds - so edits stay
//! exact regardless of tempo or sample rate.

use serde::{Deserialize, Serialize};

/// Pulses (ticks) per quarter note.
pub const PPQ: i64 = 960;

pub type Ticks = i64;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

impl TimeSignature {
    pub const FOUR_FOUR: Self = Self { numerator: 4, denominator: 4 };

    /// Ticks in one bar under this signature.
    pub fn ticks_per_bar(&self) -> Ticks {
        // A quarter note is PPQ ticks; a `denominator`-th note is
        // `PPQ * 4 / denominator` ticks; a bar holds `numerator` of them.
        (PPQ * 4 / self.denominator as i64) * self.numerator as i64
    }

    pub fn ticks_per_beat(&self) -> Ticks {
        PPQ * 4 / self.denominator as i64
    }
}

/// A tempo (and time signature) change starting at `tick`. The map always
/// has at least one event, at tick 0.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TempoEvent {
    pub tick: Ticks,
    pub bpm: f64,
    pub time_signature: TimeSignature,
}

/// Converts between ticks, seconds and samples. Built from a sorted list of
/// [`TempoEvent`]s so it already supports future tempo/signature changes,
/// even though callers today only ever install one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TempoMap {
    events: Vec<TempoEvent>,
}

impl TempoMap {
    /// A constant tempo and time signature for the whole timeline.
    pub fn constant(bpm: f64, time_signature: TimeSignature) -> Self {
        Self { events: vec![TempoEvent { tick: 0, bpm, time_signature }] }
    }

    pub fn events(&self) -> &[TempoEvent] {
        &self.events
    }

    /// The tempo event in effect at `tick`.
    fn event_at(&self, tick: Ticks) -> TempoEvent {
        self.events.iter().rev().find(|e| e.tick <= tick).copied().unwrap_or(self.events[0])
    }

    pub fn bpm_at(&self, tick: Ticks) -> f64 {
        self.event_at(tick).bpm
    }

    pub fn time_signature_at(&self, tick: Ticks) -> TimeSignature {
        self.event_at(tick).time_signature
    }

    /// Walks the segments between tempo events from 0 up to `tick`,
    /// accumulating seconds. Degenerates to one multiplication with a
    /// single-event map.
    pub fn ticks_to_seconds(&self, tick: Ticks) -> f64 {
        let mut seconds = 0.0;
        let mut prev_tick = 0;
        let mut prev_bpm = self.events[0].bpm;
        for event in self.events.iter().skip(1) {
            if event.tick >= tick {
                break;
            }
            seconds += ticks_duration_seconds(event.tick - prev_tick, prev_bpm);
            prev_tick = event.tick;
            prev_bpm = event.bpm;
        }
        seconds += ticks_duration_seconds(tick - prev_tick, prev_bpm);
        seconds
    }

    pub fn seconds_to_ticks(&self, seconds: f64) -> Ticks {
        let mut remaining = seconds;
        let mut prev_tick = 0;
        let mut prev_bpm = self.events[0].bpm;
        for event in self.events.iter().skip(1) {
            let segment_seconds = ticks_duration_seconds(event.tick - prev_tick, prev_bpm);
            if segment_seconds >= remaining {
                break;
            }
            remaining -= segment_seconds;
            prev_tick = event.tick;
            prev_bpm = event.bpm;
        }
        prev_tick + seconds_duration_ticks(remaining, prev_bpm)
    }

    pub fn ticks_to_samples(&self, tick: Ticks, sample_rate: u32) -> i64 {
        (self.ticks_to_seconds(tick) * sample_rate as f64).round() as i64
    }

    pub fn samples_to_ticks(&self, samples: i64, sample_rate: u32) -> Ticks {
        self.seconds_to_ticks(samples as f64 / sample_rate as f64)
    }

    /// 1-indexed (bar, beat, tick-within-beat) for a transport readout.
    pub fn bar_beat_tick(&self, tick: Ticks) -> (i64, i64, i64) {
        let sig = self.time_signature_at(tick);
        let ticks_per_bar = sig.ticks_per_bar();
        let ticks_per_beat = sig.ticks_per_beat();
        let bar = tick.div_euclid(ticks_per_bar);
        let into_bar = tick.rem_euclid(ticks_per_bar);
        let beat = into_bar.div_euclid(ticks_per_beat);
        let into_beat = into_bar.rem_euclid(ticks_per_beat);
        (bar + 1, beat + 1, into_beat)
    }

    pub fn bars_to_ticks(&self, bars: i64) -> Ticks {
        self.time_signature_at(0).ticks_per_bar() * bars
    }
}

/// Converts the transport's bar/beat/sixteenth readout (see [`crate::Position`],
/// always 4/4) into ticks. Kept as a free function since `Position` lives at
/// the crate root, not in the tempo map itself.
pub fn position_to_ticks(position: crate::Position) -> Ticks {
    let sig = TimeSignature::FOUR_FOUR;
    let ticks_per_sixteenth = sig.ticks_per_beat() / 4;
    (position.bar as Ticks - 1) * sig.ticks_per_bar()
        + (position.beat as Ticks - 1) * sig.ticks_per_beat()
        + (position.sixteenth as Ticks - 1) * ticks_per_sixteenth
}

fn ticks_duration_seconds(ticks: Ticks, bpm: f64) -> f64 {
    (ticks as f64 / PPQ as f64) * (60.0 / bpm)
}

fn seconds_duration_ticks(seconds: f64, bpm: f64) -> Ticks {
    ((seconds * bpm / 60.0) * PPQ as f64).round() as Ticks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarter_note_is_one_ppq() {
        let map = TempoMap::constant(120.0, TimeSignature::FOUR_FOUR);
        // At 120 BPM a quarter note (PPQ ticks) is 0.5s.
        assert!((map.ticks_to_seconds(PPQ) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn bar_length_four_four() {
        let sig = TimeSignature::FOUR_FOUR;
        assert_eq!(sig.ticks_per_bar(), PPQ * 4);
        assert_eq!(sig.ticks_per_beat(), PPQ);
    }

    #[test]
    fn round_trip_seconds() {
        let map = TempoMap::constant(128.0, TimeSignature::FOUR_FOUR);
        for tick in [0, 100, PPQ, PPQ * 16, PPQ * 4 * 16] {
            let seconds = map.ticks_to_seconds(tick);
            let back = map.seconds_to_ticks(seconds);
            assert!((back - tick).abs() <= 1, "tick={tick} back={back}");
        }
    }

    #[test]
    fn bar_beat_tick_at_origin_is_one_one_zero() {
        let map = TempoMap::constant(128.0, TimeSignature::FOUR_FOUR);
        assert_eq!(map.bar_beat_tick(0), (1, 1, 0));
        assert_eq!(map.bar_beat_tick(PPQ), (1, 2, 0));
        assert_eq!(map.bar_beat_tick(PPQ * 4), (2, 1, 0));
    }

    #[test]
    fn samples_round_trip() {
        let map = TempoMap::constant(128.0, TimeSignature::FOUR_FOUR);
        let sr = 48_000;
        for tick in [0, PPQ * 4 * 5, PPQ * 4 * 13] {
            let samples = map.ticks_to_samples(tick, sr);
            let back = map.samples_to_ticks(samples, sr);
            assert!((back - tick).abs() <= 2);
        }
    }

    #[test]
    fn bars_to_ticks_matches_bar_length() {
        let map = TempoMap::constant(128.0, TimeSignature::FOUR_FOUR);
        assert_eq!(map.bars_to_ticks(16), PPQ * 4 * 16);
    }

    #[test]
    fn position_to_ticks_matches_bar_beat_tick() {
        let map = TempoMap::constant(128.0, TimeSignature::FOUR_FOUR);
        for tick in [0, PPQ, PPQ / 4, PPQ * 4 * 8 + PPQ * 2 + PPQ / 4 * 3] {
            let (bar, beat, sixteenth_ticks) = map.bar_beat_tick(tick);
            let position = crate::Position {
                bar: bar as u32,
                beat: beat as u8,
                sixteenth: (sixteenth_ticks / (PPQ / 4)) as u8 + 1,
            };
            assert_eq!(position_to_ticks(position), tick);
        }
    }
}
