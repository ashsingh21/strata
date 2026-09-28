//! Offline rendering ("export"): the whole song mixed to a buffer, faster
//! than real time, by the same voices, effects and clip mixer the live
//! audio callback uses - so an exported file sounds like playback. What
//! the UI thread does live (scheduling notes, applying automation) is
//! done here per block instead, from the arrangement itself; notes land on
//! their exact sample rather than the UI's ~16 ms frame.

use std::collections::{BTreeMap, HashMap};

use shared::arrangement::{timed_notes_in_range, Arrangement, Ticks, TrackId};
use shared::playback::{instrument_params, DecodedSource, PlaybackPlan, MAX_BUS_TRACKS};
use shared::synth::{NoteEvent, SynthState, MAX_INSTRUMENTS};

use crate::drums::DrumEngine;
use crate::effects::EffectChain;
use crate::synth::SynthEngine;
use crate::{clip_fade_samples, db_to_gain, mix_audio_clips, SmoothedGain};

/// Samples per render block: how often automation and instrument settings
/// are re-read (~2.7 ms at 48 kHz - finer than live playback's frame).
const BLOCK: u64 = 128;
/// Blocks between rebuilding the audio-clip plan (track gain/effects).
const PLAN_EVERY: u64 = 8;
/// Silence rendered after the last clip ends, so releases and reverb
/// tails aren't cut off.
pub const TAIL_SECONDS: f64 = 2.0;

pub struct RenderJob {
    pub arrangement: Arrangement,
    /// Each Carve track's patch.
    pub patches: BTreeMap<TrackId, SynthState>,
    /// Every audio source the song uses (clips and drum-kit samples),
    /// decoded.
    pub sources: Vec<DecodedSource>,
    pub sample_rate: u32,
}

/// Where the song ends: the end of its last clip, or 0 if it has none.
pub fn song_end(arr: &Arrangement) -> Ticks {
    arr.clips.iter().map(|c| c.end()).max().unwrap_or(0)
}

/// Renders `job` from the start to the song's end plus `TAIL_SECONDS`, as
/// interleaved stereo. `progress` is called now and then with 0..1 and
/// returns `false` to cancel (then `None` is returned).
pub fn render(job: &RenderJob, progress: impl FnMut(f32) -> bool) -> Option<Vec<f32>> {
    let arr = &job.arrangement;
    let end = arr.tempo_map.ticks_to_samples(song_end(arr), job.sample_rate) + (TAIL_SECONDS * job.sample_rate as f64) as i64;
    render_samples(job, 0, end.max(0) as u64, progress)
}

/// Renders just ticks `from..to` (plus a short tail so the last notes
/// ring out) - the lessons' "Hear it" previews. Notes that started
/// before `from` aren't heard.
pub fn render_between(job: &RenderJob, from: Ticks, to: Ticks, tail_seconds: f64) -> Vec<f32> {
    let arr = &job.arrangement;
    let sr = job.sample_rate;
    let start = arr.tempo_map.ticks_to_samples(from, sr).max(0) as u64;
    let end = arr.tempo_map.ticks_to_samples(to, sr).max(0) as u64 + (tail_seconds * sr as f64) as u64;
    render_samples(job, start, end, |_| true).unwrap_or_default()
}

/// The render loop over samples `start..end`.
fn render_samples(job: &RenderJob, start: u64, end: u64, mut progress: impl FnMut(f32) -> bool) -> Option<Vec<f32>> {
    let arr = &job.arrangement;
    let sr = job.sample_rate;
    let srf = sr as f32;

    // One engine slot per instrument track, in track order.
    let slots: Vec<TrackId> =
        arr.tracks.iter().filter(|t| t.instrument.is_some()).map(|t| t.id).take(MAX_INSTRUMENTS).collect();
    let slot_of: HashMap<TrackId, usize> = slots.iter().enumerate().map(|(i, t)| (*t, i)).collect();
    let mut synths: Vec<SynthEngine> = slots.iter().map(|_| SynthEngine::new(srf)).collect();
    let mut drums: Vec<DrumEngine> = slots.iter().map(|_| DrumEngine::new(srf)).collect();
    let mut is_drums = vec![false; slots.len()];
    let mut slot_fx: Vec<EffectChain> = slots.iter().map(|_| EffectChain::new(srf)).collect();
    let mut slot_gain = vec![1.0f32; slots.len()];
    let mut slot_smooth = vec![SmoothedGain::new(srf); slots.len()];
    let mut bus_fx: Vec<EffectChain> = (0..MAX_BUS_TRACKS).map(|_| EffectChain::new(srf)).collect();
    let mut bus_smooth = [SmoothedGain::new(srf); MAX_BUS_TRACKS];
    let mut master_fx = EffectChain::new(srf);
    let mut master_limiter = crate::fx::Limiter::new(srf);
    // The limiter delays everything by its lookahead: render that much
    // further and drop that much from the start, so a hit on beat 1 of the
    // export is on beat 1 of the file, to the sample.
    let latency = master_limiter.latency() as u64;
    let total = end + latency;
    crate::dsp::flush_denormals();
    let mut plan = PlaybackPlan::default();

    // (track, pitch) -> overlapping notes holding it, as live playback does.
    let mut held: HashMap<(TrackId, u8), u32> = HashMap::new();
    let mut out = Vec::with_capacity(end.saturating_sub(start) as usize * 2);
    let mut last_tick: Ticks = arr.tempo_map.samples_to_ticks(start as i64, sr) - 1;
    // Events found but not yet reached: one exactly on a block boundary is
    // in this block's tick range but plays on the next block's first sample.
    let mut pending: Vec<shared::arrangement::TimedNote> = Vec::new();
    let mut block = 0u64;
    let mut pos = start;
    while pos < total {
        let len = BLOCK.min(total - pos);
        let tick = arr.tempo_map.samples_to_ticks(pos as i64, sr);
        let block_end_tick = arr.tempo_map.samples_to_ticks((pos + len) as i64, sr);
        let automated = arr.with_automation_at(tick);

        for (i, &track) in slots.iter().enumerate() {
            if let Some(p) = instrument_params(arr, &automated, track, i as u8, job.patches.get(&track), tick) {
                slot_gain[i] = db_to_gain(p.gain_db);
                slot_fx[i].set_state(p.effect_count, &p.effects);
                is_drums[i] = p.drums;
                drums[i].set_pads(p.drum_pads);
                synths[i].set_params(p);
            }
        }
        if block % PLAN_EVERY == 0 {
            plan = PlaybackPlan::from_arrangement(&automated, sr);
            for clip in &plan.clips {
                if let Some(chain) = bus_fx.get_mut(clip.bus_slot as usize) {
                    chain.set_state(clip.effect_count, &clip.effects);
                }
            }
            master_fx.set_state(plan.master_effect_count, &plan.master_effects);
        }
        // Start every gain at its real level: gliding up from unity made
        // the song's first hits louder than the rest.
        if block == 0 {
            for (smooth, &gain) in slot_smooth.iter_mut().zip(&slot_gain) {
                smooth.reset(gain);
            }
            for clip in &plan.clips {
                if let Some(smooth) = bus_smooth.get_mut(clip.bus_slot as usize) {
                    smooth.reset(db_to_gain(clip.gain_db));
                }
            }
        }

        // This block's notes, each at its own sample.
        pending.extend(timed_notes_in_range(arr, last_tick, block_end_tick));
        last_tick = block_end_tick;
        let events = std::mem::take(&mut pending);
        let mut next_event = 0;

        for frame in 0..len {
            let sample = pos + frame;
            while next_event < events.len() {
                let e = events[next_event];
                let at = arr.tempo_map.ticks_to_samples(e.tick, sr).max(0) as u64;
                if at > sample {
                    break;
                }
                next_event += 1;
                let Some(&slot) = slot_of.get(&e.track) else { continue };
                let key = (e.track, e.pitch);
                let fire = if e.on {
                    let count = held.entry(key).or_insert(0);
                    *count += 1;
                    *count == 1
                } else {
                    match held.get_mut(&key) {
                        Some(count) => {
                            *count = count.saturating_sub(1);
                            let released = *count == 0;
                            if released {
                                held.remove(&key);
                            }
                            released
                        }
                        None => false,
                    }
                };
                if fire {
                    let event = NoteEvent { slot: slot as u8, note: e.pitch, on: e.on, velocity: e.velocity };
                    if is_drums[slot] {
                        drums[slot].handle_note_event(event, &job.sources);
                    } else {
                        synths[slot].handle_note_event(event);
                    }
                }
            }

            let (mut l, mut r) = (0.0f32, 0.0f32);
            for i in 0..slots.len() {
                let raw = if is_drums[i] { drums[i].process(&job.sources) } else { synths[i].process() };
                let (fl, fr) = slot_fx[i].process(raw.0, raw.1);
                let g = slot_smooth[i].next(slot_gain[i]);
                l += fl * g;
                r += fr * g;
            }
            let (cl, cr) = mix_audio_clips(&plan, &job.sources, sample as i64, &mut bus_fx, &mut bus_smooth, clip_fade_samples(srf));
            let (ol, or) = master_fx.process(l + cl, r + cr);
            let (ol, or) = master_limiter.process(ol, or);
            if sample >= start + latency {
                out.push(ol);
                out.push(or);
            }
        }

        pending.extend_from_slice(&events[next_event..]);
        pos += len;
        block += 1;
        if block % 256 == 0 && !progress((pos - start) as f32 / (total - start).max(1) as f32) {
            return None;
        }
    }
    progress(1.0);
    Some(out)
}

/// Writes interleaved stereo `samples` as a 24-bit WAV with TPDF dither:
/// a triangular-distributed noise of +/-1 step added before rounding, so
/// quiet fades and reverb tails keep fading smoothly instead of breaking
/// into the grainy distortion plain truncation leaves at low levels.
pub fn write_wav(path: &std::path::Path, samples: &[f32], sample_rate: u32) -> Result<(), hound::Error> {
    let spec = hound::WavSpec { channels: 2, sample_rate, bits_per_sample: 24, sample_format: hound::SampleFormat::Int };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for s in to_24_bit(samples) {
        writer.write_sample(s)?;
    }
    writer.finalize()
}

/// Full scale for 24-bit samples.
const FULL_SCALE_24: f32 = 8_388_607.0;

/// Float samples to dithered 24-bit integers.
fn to_24_bit(samples: &[f32]) -> impl Iterator<Item = i32> + '_ {
    let mut rng = 0x9E37_79B9u32;
    let mut uniform = move || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        rng as f32 / u32::MAX as f32
    };
    samples.iter().map(move |&s| {
        // Two uniform draws make a triangle from -1 to +1 steps.
        let dither = uniform() - uniform();
        (s.clamp(-1.0, 1.0) * FULL_SCALE_24 + dither).round().clamp(-FULL_SCALE_24 - 1.0, FULL_SCALE_24) as i32
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::arrangement::{Clip, ClipColor, ClipContent, Instrument, MidiNote, TempoMap, TimeSignature, Track, TrackKind, PPQ};
    use std::sync::Arc;

    fn one_track_song(instrument: Instrument, notes: Vec<MidiNote>, length: Ticks) -> Arrangement {
        let mut arr = Arrangement::new(TempoMap::constant(120.0, TimeSignature::FOUR_FOUR));
        let id = arr.alloc_id();
        arr.tracks.push(Track {
            id,
            name: "T".into(),
            color: ClipColor::Blue,
            kind: TrackKind::Midi,
            mute: false,
            solo: false,
            arm: false,
            gain_db: 0.0,
            height: 60.0,
            instrument: Some(instrument),
            effects: vec![],
            effect_slots: vec![],
            fx: Default::default(),
            drum_pads: Default::default(),
        });
        let clip = arr.alloc_id();
        arr.clips.push(Clip {
            id: clip,
            track: id,
            start: 0,
            length,
            name: "C".into(),
            content: ClipContent::Midi { notes, loop_len: None, link: None },
            recording: false,
            gain_db: 0.0,
            swing: 0.0,
        });
        arr
    }

    #[test]
    fn renders_the_song_plus_a_tail_and_a_carve_note_makes_sound() {
        let note = MidiNote { start: 0, length: PPQ, pitch: 48, velocity: 110 };
        let arr = one_track_song(Instrument::Carve, vec![note], PPQ * 4);
        let track = arr.tracks[0].id;
        let job = RenderJob {
            arrangement: arr,
            patches: [(track, shared::synth::seed_synth())].into_iter().collect(),
            sources: vec![],
            sample_rate: 48_000,
        };
        let out = render(&job, |_| true).unwrap();
        // 4 beats at 120 BPM = 2 s, plus the 2 s tail, stereo.
        assert_eq!(out.len(), 4 * 48_000 * 2);
        let peak = |from: usize, to: usize| out[from * 2..to * 2].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak(0, 24_000) > 0.01, "the note should sound");
        assert!(peak(3 * 48_000, 4 * 48_000) < 0.001, "and be gone by the end of the tail");
    }

    #[test]
    fn drum_hits_land_on_their_exact_sample() {
        // A kick on beat 2 (0.5 s at 120 BPM): silent before, sounding at.
        let kick = MidiNote { start: PPQ, length: PPQ / 4, pitch: shared::drums::KICK, velocity: 127 };
        let arr = one_track_song(Instrument::Drums, vec![kick], PPQ * 2);
        let source = DecodedSource {
            source: Arc::from(shared::drums::pad_for_note(shared::drums::KICK).unwrap().sample),
            sample_rate: 48_000,
            channels: 1,
            samples: Arc::from(vec![0.5f32; 4_800]),
        };
        let job = RenderJob { arrangement: arr, patches: BTreeMap::new(), sources: vec![source], sample_rate: 48_000 };
        let out = render(&job, |_| true).unwrap();
        assert_eq!(out[(24_000 - 1) * 2], 0.0);
        assert!(out[24_000 * 2] > 0.1);
    }

    #[test]
    fn the_first_hit_is_as_loud_as_the_rest() {
        // Two identical kicks on a -12 dB track: the first used to glide
        // down from unity gain and come out louder.
        let kick = |start| MidiNote { start, length: PPQ / 4, pitch: shared::drums::KICK, velocity: 127 };
        let mut arr = one_track_song(Instrument::Drums, vec![kick(0), kick(PPQ * 2)], PPQ * 4);
        arr.tracks[0].gain_db = -12.0;
        let source = DecodedSource {
            source: Arc::from(shared::drums::pad_for_note(shared::drums::KICK).unwrap().sample),
            sample_rate: 48_000,
            channels: 1,
            samples: Arc::from(vec![0.5f32; 2_400]),
        };
        let job = RenderJob { arrangement: arr, patches: BTreeMap::new(), sources: vec![source], sample_rate: 48_000 };
        let out = render(&job, |_| true).unwrap();
        let peak = |from: usize| out[from * 2..(from + 2_400) * 2].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let (first, second) = (peak(0), peak(48_000));
        assert!((first - second).abs() < 0.001, "first {first} vs second {second}");
    }

    #[test]
    fn cancelling_stops_the_render() {
        let arr = one_track_song(Instrument::Carve, vec![], PPQ * 400);
        let track = arr.tracks[0].id;
        let job = RenderJob {
            arrangement: arr,
            patches: [(track, shared::synth::seed_synth())].into_iter().collect(),
            sources: vec![],
            sample_rate: 48_000,
        };
        assert!(render(&job, |_| false).is_none());
    }

    #[test]
    fn a_range_renders_only_that_range() {
        // A kick on beat 3 (1 s at 120 BPM); render beats 2 to 4.
        let kick = MidiNote { start: PPQ * 2, length: PPQ / 4, pitch: shared::drums::KICK, velocity: 127 };
        let arr = one_track_song(Instrument::Drums, vec![kick], PPQ * 4);
        let source = DecodedSource {
            source: Arc::from(shared::drums::pad_for_note(shared::drums::KICK).unwrap().sample),
            sample_rate: 48_000,
            channels: 1,
            samples: Arc::from(vec![0.5f32; 2_400]),
        };
        let job = RenderJob { arrangement: arr, patches: BTreeMap::new(), sources: vec![source], sample_rate: 48_000 };
        let out = render_between(&job, PPQ, PPQ * 3, 0.0);
        // Two beats = 1 s, stereo.
        assert_eq!(out.len(), 48_000 * 2);
        // The kick lands half a second in.
        assert_eq!(out[(24_000 - 1) * 2], 0.0);
        assert!(out[24_000 * 2] > 0.01);
    }

    #[test]
    fn export_is_24_bit_and_dithered() {
        // A signal quieter than one 24-bit step: undithered it would be all
        // zeros; dithered it's noise whose average still carries it.
        let quiet = vec![0.3 / FULL_SCALE_24; 20_000];
        let out: Vec<i32> = to_24_bit(&quiet).collect();
        assert!(out.iter().any(|&x| x != 0), "dither adds noise");
        let mean = out.iter().map(|&x| x as f64).sum::<f64>() / out.len() as f64;
        assert!((mean - 0.3).abs() < 0.05, "the average keeps the signal: {mean}");
        assert!(out.iter().all(|&x| x.abs() <= 2), "only a step or so of noise");
        // Full scale stays in range.
        let loud: Vec<i32> = to_24_bit(&[1.0, -1.0, 1.5]).collect();
        assert!(loud.iter().all(|&x| (-8_388_608..=8_388_607).contains(&x)));
    }

    #[test]
    fn clips_fade_in_and_out_and_the_master_never_clips() {
        // A full-scale square audio clip: its first sample is faded (no
        // click), and the output never goes past the limiter's ceiling.
        let mut arr = Arrangement::new(TempoMap::constant(120.0, TimeSignature::FOUR_FOUR));
        let track = arr.alloc_id();
        arr.tracks.push(Track {
            id: track,
            name: "A".into(),
            color: ClipColor::Blue,
            kind: TrackKind::Audio,
            mute: false,
            solo: false,
            arm: false,
            gain_db: 6.0,
            height: shared::arrangement::DEFAULT_TRACK_HEIGHT,
            instrument: None,
            effects: vec![],
            effect_slots: vec![],
            fx: Default::default(),
            drum_pads: Default::default(),
        });
        let id = arr.alloc_id();
        arr.clips.push(Clip {
            id,
            track,
            start: 0,
            length: PPQ * 2,
            name: "Square".into(),
            content: ClipContent::Audio { source: Arc::from("sq.wav"), peaks: None, source_offset_samples: 0 },
            recording: false,
            gain_db: 0.0,
            swing: 0.0,
        });
        let square: Vec<f32> = (0..48_000).map(|i| if (i / 50) % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let source = DecodedSource { source: Arc::from("sq.wav"), sample_rate: 48_000, channels: 1, samples: Arc::from(square) };
        let job = RenderJob { arrangement: arr, patches: BTreeMap::new(), sources: vec![source], sample_rate: 48_000 };
        let out = render_between(&job, 0, PPQ * 2, 0.0);
        let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak <= 0.95, "the master limiter holds -0.5 dBFS: {peak}");
        // The limiter's 2 ms lookahead delays everything; the clip's first
        // sound is a ramp, not a jump to full level.
        let first = out.iter().step_by(2).position(|x| x.abs() > 1.0e-6).unwrap();
        assert!(out[first * 2].abs() < 0.05, "starts faded: {}", out[first * 2]);
    }
}