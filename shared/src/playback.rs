//! The audio-clip playback bridge: UI -> engine, mirroring the shape of
//! `synth::bridge`. Two data flows:
//! - A `PlaybackPlan` snapshot (latest-wins, like `SynthParams`) describing
//!   every audio clip currently in the arrangement in sample terms, so the
//!   audio thread never has to touch `Arrangement`/`Ticks`/tempo directly.
//! - Decoded source audio (ordered, like `NoteEvent`), so a freshly
//!   referenced WAV's full samples reach the audio thread without it ever
//!   touching the filesystem.

use std::sync::Arc;

use crate::arrangement::{Arrangement, ClipContent, CompressorState, Effect, TrackId};

/// How many distinct audio tracks can have their own persistent
/// per-track Compressor state at once - generous relative to how many
/// audio tracks a project is likely to use, matching `MAX_INSTRUMENTS`'s
/// own "generous, not exact" sizing for synth slots.
pub const MAX_BUS_TRACKS: usize = 32;

/// One audio clip's position and source, already converted from ticks to
/// samples at the engine's real output sample rate.
#[derive(Clone, Debug)]
pub struct PlaybackClip {
    pub track: TrackId,
    pub source: Arc<str>,
    pub start_sample: i64,
    pub length_samples: i64,
    /// Offset into the source's own samples (per-channel-frame, not raw
    /// float index) that `start_sample` corresponds to - mirrors
    /// `ClipContent::Audio::source_offset_samples`.
    pub source_offset_samples: u64,
    /// The owning track's mixer gain (`Track.gain_db`) at the moment this
    /// plan was built - applied per-sample when mixing this clip in.
    pub gain_db: f32,
    /// This clip's own gain (`Clip.gain_db`) - applied to its own samples
    /// before they're summed into the track bus, separately from the
    /// track-wide `gain_db` above.
    pub clip_gain_db: f32,
    /// The owning track's Compressor insert effect, if any -
    /// `CompressorState::bypass()` when it has none. See `SynthParams`'s
    /// own `compressor` field for why this is always concrete, never
    /// `Option`.
    pub compressor: CompressorState,
    /// A stable small index for the owning track (its position in
    /// `Arrangement::tracks` at the moment this plan was built, clamped
    /// to `MAX_BUS_TRACKS`), so the engine can keep one persistent
    /// Compressor per track across blocks and clips - the same
    /// "producer assigns a small stable slot, consumer trusts it"
    /// pattern `SynthParams::slot` already uses for Carve instances.
    pub bus_slot: u8,
}

/// A full snapshot of what should be audible, replacing whatever the
/// engine had before. Rebuilt and pushed whenever the arrangement's clip
/// layout changes.
///
/// Note: unlike the rest of this bridge, swapping this in on the audio
/// thread does drop the previous `Vec` - a small, infrequent allocation
/// (only when the clip layout actually changes, not per block). A fixed-
/// capacity array would avoid it, but isn't worth the complexity at this
/// project's scale; this is a deliberate, bounded exception to the audio
/// callback's usual no-alloc rule, not an oversight.
#[derive(Clone, Debug, Default)]
pub struct PlaybackPlan {
    pub clips: Vec<PlaybackClip>,
}

impl PlaybackPlan {
    /// Builds a plan from the arrangement's current audio clips, converting
    /// each one's tick position to samples at `sample_rate` and skipping
    /// clips on a muted track, or on any track when some other track is
    /// soloed and this one isn't.
    pub fn from_arrangement(arrangement: &Arrangement, sample_rate: u32) -> Self {
        let any_solo = arrangement.tracks.iter().any(|t| t.solo);
        let clips = arrangement
            .clips
            .iter()
            .filter_map(|clip| {
                let track = arrangement.track(clip.track)?;
                if track.mute || (any_solo && !track.solo) {
                    return None;
                }
                let ClipContent::Audio { source, source_offset_samples, .. } = &clip.content else {
                    return None;
                };
                let start_sample = arrangement.tempo_map.ticks_to_samples(clip.start, sample_rate);
                let end_sample = arrangement.tempo_map.ticks_to_samples(clip.end(), sample_rate);
                let bus_slot = arrangement
                    .tracks
                    .iter()
                    .position(|t| t.id == clip.track)
                    .unwrap_or(0)
                    .min(MAX_BUS_TRACKS - 1) as u8;
                let compressor = track
                    .fx
                    .ordered()
                    .into_iter()
                    .find_map(|n| match (n.enabled, n.effect) {
                        (true, Effect::Compressor(c)) => Some(c),
                        (false, Effect::Compressor(_)) => None,
                    })
                    .unwrap_or_else(CompressorState::bypass);
                Some(PlaybackClip {
                    track: clip.track,
                    source: source.clone(),
                    start_sample,
                    length_samples: end_sample - start_sample,
                    source_offset_samples: *source_offset_samples,
                    gain_db: track.gain_db,
                    clip_gain_db: clip.gain_db,
                    compressor,
                    bus_slot,
                })
            })
            .collect();
        Self { clips }
    }
}

/// One audio source's fully decoded samples, interleaved by channel.
#[derive(Clone)]
pub struct DecodedSource {
    pub source: Arc<str>,
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Arc<[f32]>,
}

pub const PLAYBACK_PLAN_CAPACITY: usize = 4;
/// Generous relative to how many distinct audio sources a project is
/// likely to reference at once.
pub const DECODED_SOURCE_CAPACITY: usize = 32;

pub struct PlaybackBridge {
    pub plan_tx: rtrb::Producer<PlaybackPlan>,
    pub plan_rx: rtrb::Consumer<PlaybackPlan>,
    pub decode_tx: rtrb::Producer<DecodedSource>,
    pub decode_rx: rtrb::Consumer<DecodedSource>,
}

pub fn playback_bridge() -> PlaybackBridge {
    let (plan_tx, plan_rx) = rtrb::RingBuffer::new(PLAYBACK_PLAN_CAPACITY);
    let (decode_tx, decode_rx) = rtrb::RingBuffer::new(DECODED_SOURCE_CAPACITY);
    PlaybackBridge { plan_tx, plan_rx, decode_tx, decode_rx }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangement::{Clip, ClipColor, Track, TrackKind};
    use crate::arrangement::time::{TempoMap, TimeSignature, PPQ};

    fn track(id: TrackId, mute: bool, solo: bool) -> Track {
        Track {
            id,
            name: "Track".into(),
            color: ClipColor::Amber,
            kind: TrackKind::Audio,
            mute,
            solo,
            arm: false,
            gain_db: 0.0,
            height: 56.0,
            instrument: None,
        effects: vec![],
        effect_slots: vec![],
        fx: crate::arrangement::EffectGraph::new(),
        }
    }

    fn audio_clip(id: u32, track: TrackId, start: i64) -> Clip {
        Clip {
            id,
            track,
            start,
            length: PPQ,
            name: "Take".into(),
            content: ClipContent::Audio { source: "take.wav".into(), peaks: None, source_offset_samples: 0 },
            recording: false,
            gain_db: 0.0,
        }
    }

    fn test_arrangement() -> Arrangement {
        let mut arr = Arrangement::new(TempoMap::constant(120.0, TimeSignature::FOUR_FOUR));
        arr.tracks.push(track(1, false, false));
        arr.clips.push(audio_clip(1, 1, 0));
        arr
    }

    #[test]
    fn converts_tick_position_to_samples() {
        let arr = test_arrangement();
        let plan = PlaybackPlan::from_arrangement(&arr, 48_000);
        assert_eq!(plan.clips.len(), 1);
        let clip = &plan.clips[0];
        assert_eq!(clip.start_sample, arr.tempo_map.ticks_to_samples(0, 48_000));
        assert_eq!(clip.length_samples, arr.tempo_map.ticks_to_samples(PPQ, 48_000));
    }

    #[test]
    fn skips_midi_clips() {
        let mut arr = Arrangement::new(TempoMap::constant(120.0, TimeSignature::FOUR_FOUR));
        arr.tracks.push(track(1, false, false));
        arr.clips.push(Clip {
            id: 1,
            track: 1,
            start: 0,
            length: PPQ,
            name: "Notes".into(),
            content: ClipContent::Midi { notes: vec![] },
            recording: false,
            gain_db: 0.0,
        });
        let plan = PlaybackPlan::from_arrangement(&arr, 48_000);
        assert!(plan.clips.is_empty());
    }

    #[test]
    fn skips_muted_tracks() {
        let mut arr = test_arrangement();
        arr.tracks[0].mute = true;
        let plan = PlaybackPlan::from_arrangement(&arr, 48_000);
        assert!(plan.clips.is_empty());
    }

    #[test]
    fn solo_excludes_non_soloed_tracks() {
        let mut arr = test_arrangement();
        arr.tracks.push(track(2, false, true));
        arr.clips.push(audio_clip(2, 2, 0));
        let plan = PlaybackPlan::from_arrangement(&arr, 48_000);
        assert_eq!(plan.clips.len(), 1);
        assert_eq!(plan.clips[0].track, 2);
    }
}
