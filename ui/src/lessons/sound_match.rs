//! "Sound match": a challenge plays a hidden target sound; the learner
//! reads its spectrum and loudness curve against their own and turns
//! Carve's knobs until they match. Both sounds are measured the same way:
//! one held note rendered offline, then analysed (`shared::analysis`).

use std::collections::BTreeMap;

use shared::analysis::{analyse, Analysis};
use shared::arrangement::{empty_arrangement, Clip, ClipColor, ClipContent, EffectGraph, Instrument, MidiNote, TempoMap, TimeSignature, Track, TrackKind, DEFAULT_TRACK_HEIGHT, PPQ};
use shared::lessons::*;
use shared::synth::{SynthState, Waveform};

const SAMPLE_RATE: u32 = 48_000;
/// The measured note at 120 BPM: held 1.5 s, then 1.5 s for its tail.
const BPM: f64 = 120.0;
/// Where the spectrum is read: while the note is held, past its attack.
const HELD: (f32, f32) = (0.2, 1.45);
/// The score that wins a challenge.
pub const WIN: f32 = 0.92;

/// Renders `patch` playing the challenge note, and analyses it.
pub fn measure(patch: &SynthState) -> Analysis {
    let mut arr = empty_arrangement();
    arr.tempo_map = TempoMap::constant(BPM, TimeSignature::FOUR_FOUR);
    let track = arr.alloc_id();
    arr.tracks.push(Track {
        id: track,
        name: "Match".into(),
        color: ClipColor::Violet,
        kind: TrackKind::Midi,
        mute: false,
        solo: false,
        arm: false,
        gain_db: 0.0,
        height: DEFAULT_TRACK_HEIGHT,
        instrument: Some(Instrument::Carve),
        effects: vec![],
        effect_slots: vec![],
        fx: EffectGraph::new(),
    });
    let id = arr.alloc_id();
    let note = MidiNote { start: 0, length: MATCH_NOTE_16THS * PPQ / 4, pitch: MATCH_NOTE, velocity: 100 };
    arr.clips.push(Clip {
        id,
        track,
        start: 0,
        length: 4 * PPQ,
        name: "Note".into(),
        content: ClipContent::Midi { notes: vec![note], loop_len: None, link: None },
        recording: false,
        gain_db: 0.0,
    });
    let mut patch = patch.clone();
    patch.held_notes.clear();
    let job = engine::render::RenderJob {
        arrangement: arr,
        patches: BTreeMap::from([(track, patch)]),
        sources: vec![],
        sample_rate: SAMPLE_RATE,
    };
    let audio = engine::render::render_between(&job, 0, 3 * PPQ, 1.5);
    analyse(&audio, SAMPLE_RATE, HELD)
}

/// The hidden sound a challenge asks for (`None` for other lessons).
pub fn target(lesson: &str) -> Option<SynthState> {
    let mut p = init_patch();
    p.name = "Target";
    match lesson {
        MATCH_WAVE => p.osc1.waveform = Waveform::Square,
        MATCH_CUTOFF => p.filter.cutoff_hz = 700.0,
        MATCH_RESONANCE => {
            p.filter.cutoff_hz = 1200.0;
            p.filter.resonance = 0.75;
        }
        MATCH_SUB => p.mix.sub_db = -6.0,
        MATCH_PLUCK => {
            p.amp_env.decay_ms = 250.0;
            p.amp_env.sustain = 0.0;
        }
        MATCH_SWELL => p.amp_env.attack_ms = 500.0,
        MATCH_MYSTERY => {
            p.osc1.waveform = Waveform::Square;
            p.filter.cutoff_hz = 1500.0;
            p.amp_env.decay_ms = 400.0;
            p.amp_env.sustain = 0.3;
        }
        _ => return None,
    }
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::analysis::likeness;

    /// The score for `patch` against `lesson`'s target.
    fn score(lesson: &str, patch: impl FnOnce(&mut SynthState)) -> f32 {
        let mut p = init_patch();
        patch(&mut p);
        likeness(&measure(&target(lesson).unwrap()), &measure(&p))
    }

    #[test]
    fn close_is_close_enough_and_wrong_is_not() {
        // Near the target's 700 Hz wins; well off it doesn't.
        assert!(score(MATCH_CUTOFF, |p| p.filter.cutoff_hz = 800.0) >= WIN);
        assert!(score(MATCH_CUTOFF, |p| p.filter.cutoff_hz = 1300.0) < WIN);
        // A triangle isn't a square.
        assert!(score(MATCH_WAVE, |p| p.osc1.waveform = Waveform::Triangle) < WIN);
        // A 300 ms pluck is near enough the target's 250 ms.
        assert!(score(MATCH_PLUCK, |p| {
            p.amp_env.sustain = 0.0;
            p.amp_env.decay_ms = 300.0;
        }) >= WIN);
        // Without its sustain at zero, the right decay isn't a pluck.
        assert!(score(MATCH_PLUCK, |p| p.amp_env.decay_ms = 250.0) < WIN);
    }

}
