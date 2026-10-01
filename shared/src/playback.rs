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
use crate::synth::{EffectUnitState, MAX_EFFECTS_PER_CHAIN};

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
    /// The owning track's effect chain, in order - only
    /// `effects[..effect_count]` is meaningful. See `SynthParams`'s own
    /// `effects`/`effect_count` fields for the same "fixed array, not a
    /// Vec" reasoning.
    pub effect_count: u8,
    pub effects: [EffectUnitState; MAX_EFFECTS_PER_CHAIN],
    /// A stable small index for the owning track (its position in
    /// `Arrangement::tracks` at the moment this plan was built, clamped
    /// to `MAX_BUS_TRACKS`), so the engine can keep one persistent
    /// Compressor per track across blocks and clips - the same
    /// "producer assigns a small stable slot, consumer trusts it"
    /// pattern `SynthParams::slot` already uses for Carve instances.
    pub bus_slot: u8,
}

/// What you hear of the track you're playing into: its input, live,
/// through the track's own effects and fader. Only the armed audio track
/// has one.
#[derive(Clone, Copy, Debug)]
pub struct MonitorPlan {
    /// The track's bus slot, so its meter shows what you hear.
    pub bus_slot: u8,
    pub gain_db: f32,
    pub effect_count: u8,
    pub effects: [EffectUnitState; MAX_EFFECTS_PER_CHAIN],
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
#[derive(Clone, Debug)]
pub struct PlaybackPlan {
    pub clips: Vec<PlaybackClip>,
    /// The master bus's own effect chain - applied once, after every
    /// track's own chain and fader have summed into the final mix,
    /// before the output meter. Same fixed-array shape as a track's.
    pub master_effect_count: u8,
    pub master_effects: [EffectUnitState; MAX_EFFECTS_PER_CHAIN],
    /// The armed audio track's live input, if there is one (and it isn't
    /// muted). Whether it's actually fed is `RecordParams::monitoring`.
    pub monitor: Option<MonitorPlan>,
}

impl Default for PlaybackPlan {
    fn default() -> Self {
        Self {
            clips: Vec::new(),
            master_effect_count: 0,
            master_effects: [EffectUnitState::Compressor(CompressorState::bypass()); MAX_EFFECTS_PER_CHAIN],
            monitor: None,
        }
    }
}

/// Converts a track's `EffectGraph` (ordered, source-to-output) into the
/// fixed-array wire shape both `PlaybackClip` and `SynthParams` carry
/// across the UI -> engine bridge. A disabled node still occupies a slot
/// (as its own effect's bypass state) rather than being skipped, so
/// toggling it on/off later doesn't shift every other slot's index -
/// same "always run the same unit, never branch on enabled at the DSP
/// level" reasoning the old always-concrete `CompressorState` had.
pub fn build_effect_units(fx: &crate::arrangement::EffectGraph) -> (u8, [EffectUnitState; MAX_EFFECTS_PER_CHAIN]) {
    let mut effects = [EffectUnitState::Compressor(CompressorState::bypass()); MAX_EFFECTS_PER_CHAIN];
    let mut count = 0usize;
    for node in fx.ordered() {
        if count >= MAX_EFFECTS_PER_CHAIN {
            break;
        }
        effects[count] = match node.effect {
            Effect::Compressor(c) => {
                EffectUnitState::Compressor(if node.enabled { c } else { CompressorState::bypass() })
            }
            Effect::Eq(e) => EffectUnitState::Eq(if node.enabled { e } else { crate::arrangement::EqState::bypass() }),
            Effect::Guitar(g) => EffectUnitState::Guitar(if node.enabled { g } else { g.bypass() }),
        };
        count += 1;
    }
    (count as u8, effects)
}

/// What the engine needs to play `track`'s instrument at `tick`: its patch
/// (for Carve) with synth-parameter automation applied, plus the track's
/// gain and effect chain as automated (`automated` is
/// `arr.with_automation_at(tick)`). `None` if the track has no instrument,
/// or a Carve track has no patch. Shared by live playback and export, so
/// an exported song sounds like the one you hear.
pub fn instrument_params(
    arr: &crate::arrangement::Arrangement,
    automated: &crate::arrangement::Arrangement,
    track: crate::arrangement::TrackId,
    slot: u8,
    patch: Option<&crate::synth::SynthState>,
    tick: crate::arrangement::Ticks,
) -> Option<crate::synth::SynthParams> {
    use crate::arrangement::Instrument;
    let t = automated.track(track)?;
    let mut params = match t.instrument? {
        Instrument::Drums => crate::synth::SynthParams { drums: true, drum_pads: t.drum_pads, ..Default::default() },
        Instrument::Carve => {
            let mut patch = patch?.clone();
            arr.apply_synth_automation(track, tick, &mut patch);
            crate::synth::SynthParams::from_state(&patch)
        }
    };
    params.slot = slot;
    params.gain_db = t.gain_db;
    let (count, effects) = build_effect_units(&t.fx);
    params.effect_count = count;
    params.effects = effects;
    Some(params)
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
                let (effect_count, effects) = build_effect_units(&track.fx);
                Some(PlaybackClip {
                    track: clip.track,
                    source: source.clone(),
                    start_sample,
                    length_samples: end_sample - start_sample,
                    source_offset_samples: *source_offset_samples,
                    gain_db: track.gain_db,
                    clip_gain_db: clip.gain_db,
                    effect_count,
                    effects,
                    bus_slot,
                })
            })
            .collect();
        let (master_effect_count, master_effects) = build_effect_units(&arrangement.master_effects);
        let monitor = arrangement
            .tracks
            .iter()
            .enumerate()
            .find(|(_, t)| t.arm && t.kind == crate::arrangement::TrackKind::Audio)
            .filter(|(_, t)| !t.mute && !(any_solo && !t.solo))
            .map(|(i, t)| {
                let (effect_count, effects) = build_effect_units(&t.fx);
                MonitorPlan { bus_slot: i.min(MAX_BUS_TRACKS - 1) as u8, gain_db: t.gain_db, effect_count, effects }
            });
        Self { clips, master_effect_count, master_effects, monitor }
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

impl DecodedSource {
    /// This source at `rate`. The engine reads sources one frame per
    /// output sample, so every source must be converted to the engine's
    /// rate before it gets there - a 44.1 kHz file read at 48 kHz played
    /// 9% fast and 1.5 semitones sharp. Done once, off the audio thread.
    pub fn at_rate(self, rate: u32) -> Self {
        if self.sample_rate == rate || rate == 0 || self.sample_rate == 0 {
            return self;
        }
        let samples = resample(&self.samples, self.channels.max(1) as usize, self.sample_rate, rate);
        Self { samples: Arc::from(samples), sample_rate: rate, ..self }
    }
}

/// Zero crossings of the sinc kernel either side of its centre: enough
/// for a transition band under 2 kHz, so content just above the new
/// Nyquist is removed rather than folded back.
const SINC_HALF_WIDTH: f32 = 64.0;
/// Kernel table entries per input sample (linearly interpolated between).
const SINC_TABLE_STEPS: f32 = 512.0;

/// Converts interleaved `samples` from `from` Hz to `to` Hz with a
/// Blackman-windowed sinc (128 taps at the lower of the two rates, band-
/// limited to just under the lower Nyquist so downsampling doesn't alias).
pub fn resample(samples: &[f32], channels: usize, from: u32, to: u32) -> Vec<f32> {
    let channels = channels.max(1);
    let frames = samples.len() / channels;
    if from == to || frames == 0 {
        return samples.to_vec();
    }
    let ratio = to as f64 / from as f64;
    // Cutoff as a fraction of the input's Nyquist.
    let fc = (ratio.min(1.0) * 0.95) as f32;
    // The kernel's half-width in input samples.
    let half = SINC_HALF_WIDTH / fc;
    let table_len = (half * SINC_TABLE_STEPS) as usize + 2;
    let table: Vec<f32> = (0..table_len)
        .map(|i| {
            let x = i as f32 / SINC_TABLE_STEPS;
            if x >= half {
                return 0.0;
            }
            let arg = std::f32::consts::PI * fc * x;
            let sinc = if arg.abs() < 1.0e-6 { 1.0 } else { arg.sin() / arg };
            let u = x / half;
            let window = 0.42 + 0.5 * (std::f32::consts::PI * u).cos() + 0.08 * (std::f32::consts::TAU * u).cos();
            fc * sinc * window
        })
        .collect();
    let kernel = |d: f32| {
        let pos = d.abs() * SINC_TABLE_STEPS;
        let i = pos as usize;
        if i + 1 >= table.len() {
            return 0.0;
        }
        let frac = pos - i as f32;
        table[i] + (table[i + 1] - table[i]) * frac
    };
    let out_frames = (frames as f64 * ratio).round() as usize;
    let reach = half.ceil() as i64;
    let mut out = Vec::with_capacity(out_frames * channels);
    for n in 0..out_frames {
        let t = n as f64 / ratio;
        let centre = t.floor() as i64;
        let frac = (t - centre as f64) as f32;
        let lo = (centre - reach + 1).max(0);
        let hi = (centre + reach).min(frames as i64 - 1);
        for ch in 0..channels {
            let mut acc = 0.0f32;
            for k in lo..=hi {
                acc += samples[k as usize * channels + ch] * kernel((k - centre) as f32 - frac);
            }
            out.push(acc);
        }
    }
    out
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
        drum_pads: Default::default(),
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
            swing: 0.0,
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
            content: ClipContent::Midi { notes: vec![], loop_len: None, link: None },
            recording: false,
            gain_db: 0.0,
            swing: 0.0,
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

/// A short, already-rendered stereo clip (interleaved, at the engine's
/// rate) played straight to the output - the lessons' "Hear it", the
/// browser's previews. `looping` plays it round until replaced. An empty
/// one stops whatever is previewing. Buffers the engine has finished
/// with come back on `retired` so they're freed on the UI thread, never
/// the audio thread.
pub struct PreviewSound {
    pub audio: Vec<f32>,
    pub looping: bool,
}

pub type PreviewBuffer = Arc<PreviewSound>;

/// The preview's volume (an `f32`'s bits, linear gain), changed live.
pub type PreviewGain = Arc<std::sync::atomic::AtomicU32>;

pub fn preview_gain(gain: f32) -> PreviewGain {
    Arc::new(std::sync::atomic::AtomicU32::new(gain.to_bits()))
}

pub const PREVIEW_CAPACITY: usize = 8;

/// How many output samples the live analyzer's tap holds (a third of a
/// second): the UI drains it every frame, and needs only the latest few
/// thousand.
pub const ANALYZER_CAPACITY: usize = 16_384;

pub struct PreviewBridge {
    pub play_tx: rtrb::Producer<PreviewBuffer>,
    pub play_rx: rtrb::Consumer<PreviewBuffer>,
    pub retired_tx: rtrb::Producer<PreviewBuffer>,
    pub retired_rx: rtrb::Consumer<PreviewBuffer>,
    /// The output as heard (mono), for the live spectrum analyzer.
    pub analyzer_tx: rtrb::Producer<f32>,
    pub analyzer_rx: rtrb::Consumer<f32>,
    pub gain: PreviewGain,
}

pub fn preview_bridge() -> PreviewBridge {
    let (play_tx, play_rx) = rtrb::RingBuffer::new(PREVIEW_CAPACITY);
    let (retired_tx, retired_rx) = rtrb::RingBuffer::new(PREVIEW_CAPACITY * 2);
    let (analyzer_tx, analyzer_rx) = rtrb::RingBuffer::new(ANALYZER_CAPACITY);
    PreviewBridge { play_tx, play_rx, retired_tx, retired_rx, analyzer_tx, analyzer_rx, gain: preview_gain(1.0) }
}

/// The UI's ends of a `PreviewBridge`: send buffers to play, and take
/// played ones back to free.
pub struct PreviewSender {
    pub play_tx: rtrb::Producer<PreviewBuffer>,
    pub retired_rx: rtrb::Consumer<PreviewBuffer>,
    pub gain: PreviewGain,
}

/// The engine's ends of a `PreviewBridge`: previews in, and the output
/// out to the analyzer.
pub struct PreviewEnds {
    pub play_rx: rtrb::Consumer<PreviewBuffer>,
    pub retired_tx: rtrb::Producer<PreviewBuffer>,
    pub analyzer_tx: rtrb::Producer<f32>,
    pub gain: PreviewGain,
}

#[cfg(test)]
mod resample_tests {
    use super::*;

    fn tone(hz: f32, rate: u32, seconds: f32) -> Vec<f32> {
        (0..(rate as f32 * seconds) as usize).map(|i| 0.5 * (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin()).collect()
    }

    /// Rising zero crossings per second, away from the edges.
    fn frequency(x: &[f32], rate: u32) -> f32 {
        let (a, b) = (x.len() / 10, x.len() * 9 / 10);
        let crossings = (a..b).filter(|&i| x[i - 1] < 0.0 && x[i] >= 0.0).count();
        crossings as f32 / ((b - a) as f32 / rate as f32)
    }

    #[test]
    fn a_tone_keeps_its_pitch_length_and_level() {
        let src = tone(1000.0, 44_100, 1.0);
        let out = resample(&src, 1, 44_100, 48_000);
        assert_eq!(out.len(), 48_000);
        assert!((frequency(&out, 48_000) - 1000.0).abs() < 2.0, "{}", frequency(&out, 48_000));
        let peak = out[4_800..43_200].iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!((peak - 0.5).abs() < 0.01, "{peak}");
    }

    #[test]
    fn downsampling_filters_what_would_alias() {
        // 23 kHz is above 22.05 kHz, the new Nyquist: it must go, not fold.
        let src = tone(23_000.0, 48_000, 0.5);
        let out = resample(&src, 1, 48_000, 44_100);
        let peak = out[4_410..17_640].iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak < 0.01, "{peak}");
    }

    #[test]
    fn stereo_channels_stay_apart_and_same_rate_is_untouched() {
        let left = tone(500.0, 44_100, 0.2);
        let stereo: Vec<f32> = left.iter().flat_map(|&l| [l, 0.0]).collect();
        let out = resample(&stereo, 2, 44_100, 48_000);
        assert!(out.iter().skip(1).step_by(2).all(|r| r.abs() < 1.0e-6));
        let same = DecodedSource { source: Arc::from("x"), sample_rate: 48_000, channels: 2, samples: Arc::from(stereo.clone()) }.at_rate(48_000);
        assert_eq!(same.samples.len(), stereo.len());
    }
}
