//! Automation: what a lane controls (`AutomationTarget`), the value a lane
//! has at a given tick, and applying every lane to a copy of the
//! arrangement at the playhead. Also the single source of truth for each
//! automatable parameter's range (normalized 0..1 <-> real value) and its
//! display format, shared by the device panels' knobs and the lanes so the
//! two can never disagree.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use super::model::{Arrangement, AutomationLane, Effect, EffectNodeId, EffectParam};
use super::time::Ticks;

/// What an automation lane controls, on the lane's own track.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutomationTarget {
    /// The track's fader.
    TrackGain,
    /// One knob on one node of the track's effect chain.
    Effect { node: EffectNodeId, param: EffectParam },
}

pub fn lin_norm(min: f32, max: f32, value: f32) -> f32 {
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

pub fn lin_value(min: f32, max: f32, norm: f32) -> f32 {
    min + (max - min) * norm.clamp(0.0, 1.0)
}

pub fn log_norm(min: f32, max: f32, value: f32) -> f32 {
    ((value.max(min).ln() - min.ln()) / (max.ln() - min.ln())).clamp(0.0, 1.0)
}

pub fn log_value(min: f32, max: f32, norm: f32) -> f32 {
    (min.ln() + norm.clamp(0.0, 1.0) * (max.ln() - min.ln())).exp()
}

/// Fader position (0..1) to gain in dB: unity at 0.75, +6 dB at the top,
/// -60..0 dB below that - typical DAW fader taper.
pub fn fader_pos_to_gain_db(position: f32) -> f32 {
    let position = position.clamp(0.0, 1.0);
    if position <= 0.0 {
        -100.0
    } else if position >= 0.75 {
        (position - 0.75) / 0.25 * 6.0
    } else {
        (position / 0.75 - 1.0) * 60.0
    }
}

pub fn gain_db_to_fader_pos(db: f32) -> f32 {
    if db >= 0.0 {
        (0.75 + db / 24.0).clamp(0.75, 1.0)
    } else {
        (0.75 * (db / 60.0 + 1.0)).clamp(0.0, 0.75)
    }
}

impl EffectParam {
    /// The effect type this param belongs to ("Compressor", "EQ").
    pub fn effect_name(self) -> &'static str {
        match self {
            EffectParam::CompressorThreshold
            | EffectParam::CompressorRatio
            | EffectParam::CompressorAttack
            | EffectParam::CompressorRelease
            | EffectParam::CompressorMakeup => "Compressor",
            EffectParam::EqFreq | EffectParam::EqGain | EffectParam::EqQ => "EQ",
        }
    }

    /// This param's current value on `effect`, normalized 0..1 - `None` if
    /// `effect` is a different effect type.
    pub fn norm(self, effect: &Effect) -> Option<f32> {
        Some(match (self, effect) {
            (EffectParam::CompressorThreshold, Effect::Compressor(c)) => lin_norm(-60.0, 0.0, c.threshold_db),
            (EffectParam::CompressorRatio, Effect::Compressor(c)) => lin_norm(1.0, 20.0, c.ratio),
            (EffectParam::CompressorAttack, Effect::Compressor(c)) => lin_norm(0.1, 100.0, c.attack_ms),
            (EffectParam::CompressorRelease, Effect::Compressor(c)) => lin_norm(10.0, 1000.0, c.release_ms),
            (EffectParam::CompressorMakeup, Effect::Compressor(c)) => lin_norm(0.0, 24.0, c.makeup_db),
            (EffectParam::EqFreq, Effect::Eq(e)) => log_norm(20.0, 20_000.0, e.freq_hz),
            (EffectParam::EqGain, Effect::Eq(e)) => lin_norm(-18.0, 18.0, e.gain_db),
            (EffectParam::EqQ, Effect::Eq(e)) => lin_norm(0.1, 10.0, e.q),
            _ => return None,
        })
    }

    /// Sets this param on `effect` from a normalized 0..1 value. A no-op if
    /// `effect` is a different effect type.
    pub fn apply_norm(self, effect: &mut Effect, norm: f32) {
        match (self, effect) {
            (EffectParam::CompressorThreshold, Effect::Compressor(c)) => c.threshold_db = lin_value(-60.0, 0.0, norm),
            (EffectParam::CompressorRatio, Effect::Compressor(c)) => c.ratio = lin_value(1.0, 20.0, norm),
            (EffectParam::CompressorAttack, Effect::Compressor(c)) => c.attack_ms = lin_value(0.1, 100.0, norm),
            (EffectParam::CompressorRelease, Effect::Compressor(c)) => c.release_ms = lin_value(10.0, 1000.0, norm),
            (EffectParam::CompressorMakeup, Effect::Compressor(c)) => c.makeup_db = lin_value(0.0, 24.0, norm),
            (EffectParam::EqFreq, Effect::Eq(e)) => e.freq_hz = log_value(20.0, 20_000.0, norm),
            (EffectParam::EqGain, Effect::Eq(e)) => e.gain_db = lin_value(-18.0, 18.0, norm),
            (EffectParam::EqQ, Effect::Eq(e)) => e.q = lin_value(0.1, 10.0, norm),
            _ => {}
        }
    }

    /// This param's value on `effect`, formatted the way its knob shows it.
    pub fn format(self, effect: &Effect) -> String {
        match (self, effect) {
            (EffectParam::CompressorThreshold, Effect::Compressor(c)) => format!("{:+.1} dB", c.threshold_db),
            (EffectParam::CompressorRatio, Effect::Compressor(c)) => format!("{:.1}:1", c.ratio),
            (EffectParam::CompressorAttack, Effect::Compressor(c)) => format!("{:.1} ms", c.attack_ms),
            (EffectParam::CompressorRelease, Effect::Compressor(c)) => format!("{:.0} ms", c.release_ms),
            (EffectParam::CompressorMakeup, Effect::Compressor(c)) => format!("{:+.1} dB", c.makeup_db),
            (EffectParam::EqFreq, Effect::Eq(e)) => format!("{:.0} Hz", e.freq_hz),
            (EffectParam::EqGain, Effect::Eq(e)) => format!("{:+.1} dB", e.gain_db),
            (EffectParam::EqQ, Effect::Eq(e)) => format!("{:.2}", e.q),
            _ => String::new(),
        }
    }
}

impl AutomationLane {
    /// The lane's normalized value at `tick`: linear between breakpoints,
    /// holding the first/last value outside them. `None` with no
    /// breakpoints. Breakpoints are kept sorted by tick.
    pub fn value_at(&self, tick: Ticks) -> Option<f32> {
        let points = &self.breakpoints;
        let first = points.first()?;
        let last = points.last()?;
        if tick <= first.tick {
            return Some(first.value);
        }
        if tick >= last.tick {
            return Some(last.value);
        }
        let i = points.partition_point(|p| p.tick <= tick);
        let (a, b) = (&points[i - 1], &points[i]);
        let span = (b.tick - a.tick) as f32;
        let t = if span > 0.0 { (tick - a.tick) as f32 / span } else { 1.0 };
        Some(a.value + (b.value - a.value) * t)
    }
}

impl Arrangement {
    /// The arrangement as it should sound at `tick`: every lane with a
    /// target applied to a copy (track gain, effect-node params). Borrowed
    /// untouched when no lane has a target - the common case costs nothing.
    /// Lanes whose target no longer exists (node deleted) are skipped.
    pub fn with_automation_at(&self, tick: Ticks) -> Cow<'_, Arrangement> {
        if !self.automation.iter().any(|lane| lane.target.is_some() && !lane.breakpoints.is_empty()) {
            return Cow::Borrowed(self);
        }
        let mut arr = self.clone();
        for lane in &self.automation {
            let (Some(target), Some(norm)) = (lane.target, lane.value_at(tick)) else { continue };
            let Some(track) = arr.track_mut(lane.track) else { continue };
            match target {
                AutomationTarget::TrackGain => track.gain_db = fader_pos_to_gain_db(norm),
                AutomationTarget::Effect { node, param } => {
                    if let Some(n) = track.fx.nodes.iter_mut().find(|n| n.id == node) {
                        param.apply_norm(&mut n.effect, norm);
                    }
                }
            }
        }
        Cow::Owned(arr)
    }

    /// The current (un-automated) normalized value of `target` on `track` -
    /// what a newly created lane's first breakpoint holds, so creating a
    /// lane never changes the sound. `None` if the target doesn't exist.
    pub fn target_norm(&self, track: super::model::TrackId, target: AutomationTarget) -> Option<f32> {
        let t = self.track(track)?;
        match target {
            AutomationTarget::TrackGain => Some(gain_db_to_fader_pos(t.gain_db)),
            AutomationTarget::Effect { node, param } => param.norm(&t.fx.node(node)?.effect),
        }
    }

    /// "Compressor · Threshold", "Track · Gain" - `None` if the target no
    /// longer exists (its effect node was removed).
    pub fn target_label(&self, track: super::model::TrackId, target: AutomationTarget) -> Option<String> {
        match target {
            AutomationTarget::TrackGain => Some("Track \u{b7} Gain".to_string()),
            AutomationTarget::Effect { node, param } => {
                self.track(track)?.fx.node(node)?;
                Some(format!("{} \u{b7} {}", param.effect_name(), param.name()))
            }
        }
    }

    /// `target`'s value on `track` in this arrangement, formatted like its
    /// knob/fader readout. Call on `with_automation_at(tick)` for the
    /// automated value.
    pub fn target_display(&self, track: super::model::TrackId, target: AutomationTarget) -> Option<String> {
        let t = self.track(track)?;
        match target {
            AutomationTarget::TrackGain => Some(format!("{:+.1} dB", t.gain_db)),
            AutomationTarget::Effect { node, param } => Some(param.format(&t.fx.node(node)?.effect)),
        }
    }
}

impl Arrangement {
    /// `target` on `track` formatted as if set to normalized `norm` - a lane
    /// header's live readout, without cloning the arrangement.
    pub fn target_display_at(&self, track: super::model::TrackId, target: AutomationTarget, norm: f32) -> Option<String> {
        let t = self.track(track)?;
        Some(match target {
            AutomationTarget::TrackGain => {
                let db = fader_pos_to_gain_db(norm);
                if db <= -99.0 { "-inf dB".to_string() } else { format!("{db:+.1} dB") }
            }
            AutomationTarget::Effect { node, param } => {
                let mut effect = t.fx.node(node)?.effect;
                param.apply_norm(&mut effect, norm);
                param.format(&effect)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangement::model::{Breakpoint, ClipColor, CompressorState, EffectGraph, Track, TrackKind};

    fn lane(points: &[(Ticks, f32)], target: Option<AutomationTarget>) -> AutomationLane {
        AutomationLane {
            id: 99,
            track: 1,
            parameter_name: String::new(),
            display_value: String::new(),
            breakpoints: points.iter().map(|&(tick, value)| Breakpoint { tick, value }).collect(),
            target,
        }
    }

    fn arrangement_with_compressor() -> (Arrangement, EffectNodeId) {
        let mut arr = crate::arrangement::empty_arrangement();
        let mut fx = EffectGraph::new();
        let node = fx.push_at_end(Effect::Compressor(CompressorState::default()));
        let id = arr.alloc_id();
        assert_eq!(id, 1, "tests assume the first track gets id 1");
        arr.tracks.push(Track {
            id,
            name: "Audio 1".into(),
            color: ClipColor::Coral,
            kind: TrackKind::Audio,
            mute: false,
            solo: false,
            arm: false,
            gain_db: 0.0,
            height: crate::arrangement::DEFAULT_TRACK_HEIGHT,
            instrument: None,
            effects: vec![],
            effect_slots: vec![],
            fx,
        });
        (arr, node)
    }

    #[test]
    fn value_at_interpolates_and_holds_the_ends() {
        let l = lane(&[(100, 0.2), (200, 0.6)], None);
        assert_eq!(l.value_at(0), Some(0.2));
        assert_eq!(l.value_at(100), Some(0.2));
        assert!((l.value_at(150).unwrap() - 0.4).abs() < 1e-6);
        assert_eq!(l.value_at(200), Some(0.6));
        assert_eq!(l.value_at(10_000), Some(0.6));
        assert_eq!(lane(&[], None).value_at(0), None);
    }

    #[test]
    fn value_at_handles_a_vertical_step() {
        let l = lane(&[(0, 0.0), (100, 0.0), (100, 1.0), (200, 1.0)], None);
        assert_eq!(l.value_at(50), Some(0.0));
        assert_eq!(l.value_at(150), Some(1.0));
    }

    #[test]
    fn with_automation_at_borrows_when_nothing_is_targeted() {
        let (mut arr, _) = arrangement_with_compressor();
        arr.automation.push(lane(&[(0, 0.5)], None));
        assert!(matches!(arr.with_automation_at(0), Cow::Borrowed(_)));
    }

    #[test]
    fn with_automation_at_applies_gain_and_effect_params() {
        let (mut arr, node) = arrangement_with_compressor();
        arr.automation.push(lane(&[(0, 0.75)], Some(AutomationTarget::TrackGain)));
        arr.automation
            .push(lane(&[(0, 0.0), (100, 1.0)], Some(AutomationTarget::Effect { node, param: EffectParam::CompressorThreshold })));
        let at = arr.with_automation_at(50);
        let t = at.track(1).unwrap();
        assert!((t.gain_db - 0.0).abs() < 1e-4, "0.75 is unity on the fader taper");
        let Effect::Compressor(c) = t.fx.node(node).unwrap().effect else { panic!() };
        assert!((c.threshold_db - -30.0).abs() < 1e-3, "halfway through -60..0");
        // The source arrangement itself is untouched.
        let Effect::Compressor(orig) = arr.track(1).unwrap().fx.node(node).unwrap().effect else { panic!() };
        assert_eq!(orig.threshold_db, CompressorState::default().threshold_db);
    }

    #[test]
    fn with_automation_at_skips_a_removed_node() {
        let (mut arr, node) = arrangement_with_compressor();
        arr.automation.push(lane(&[(0, 0.0)], Some(AutomationTarget::Effect { node: node + 50, param: EffectParam::CompressorRatio })));
        let at = arr.with_automation_at(0);
        assert_eq!(at.track(1).unwrap().fx.nodes.len(), 1);
        assert!(arr.target_label(1, AutomationTarget::Effect { node: node + 50, param: EffectParam::CompressorRatio }).is_none());
        let _ = node;
    }

    #[test]
    fn every_param_round_trips_through_its_norm() {
        let effects = [Effect::Compressor(CompressorState::default()), Effect::Eq(crate::arrangement::EqState::default())];
        for effect in effects {
            for &param in EffectParam::for_effect(effect) {
                for norm in [0.0, 0.3, 1.0] {
                    let mut e = effect;
                    param.apply_norm(&mut e, norm);
                    let back = param.norm(&e).unwrap();
                    assert!((back - norm).abs() < 1e-4, "{param:?} {norm} -> {back}");
                }
            }
        }
    }

    #[test]
    fn fader_taper_round_trips() {
        for pos in [0.1, 0.5, 0.75, 0.9, 1.0] {
            assert!((gain_db_to_fader_pos(fader_pos_to_gain_db(pos)) - pos).abs() < 1e-4, "{pos}");
        }
    }

    #[test]
    fn target_norm_matches_the_current_value() {
        let (arr, node) = arrangement_with_compressor();
        let t = AutomationTarget::Effect { node, param: EffectParam::CompressorThreshold };
        assert!((arr.target_norm(1, t).unwrap() - lin_norm(-60.0, 0.0, -18.0)).abs() < 1e-6);
        assert_eq!(arr.target_label(1, t).as_deref(), Some("Compressor \u{b7} Threshold"));
        assert_eq!(arr.target_display(1, t).as_deref(), Some("-18.0 dB"));
    }
}
