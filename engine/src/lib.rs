//! The audio engine: Carve's synth voices, the arrangement's audio clips
//! and a metronome click, mixed, with peak metering and transport position reported back through a
//! [`shared::Telemetry`] ring buffer.
//!
//! Everything in the audio callback ([`write_block`]) is allocation-, lock-
//! and syscall-free, with one deliberate, bounded exception: swapping in a
//! new [`PlaybackPlan`] (only on the rare block where the arrangement's
//! clip layout actually changed) drops the previous one's `Vec`. See
//! `shared::playback` for why that's an acceptable trade here.

pub mod input;
mod compressor;
mod drums;
pub mod render;
mod dsp;
mod effects;
mod eq;
mod fx;
mod synth;

use std::fmt;
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Error as CpalError, FromSample, OutputCallbackInfo, Sample, SampleFormat, SizedSample, StreamConfig};
use shared::playback::{DecodedSource, PlaybackPlan, DECODED_SOURCE_CAPACITY, MAX_BUS_TRACKS};
use shared::recorder::RecordCommand;
use shared::synth::{NoteEvent, SynthParams, SynthTelemetry, MAX_INSTRUMENTS};
use shared::{Params, Position, Telemetry};
use drums::DrumEngine;
use effects::EffectChain;
use synth::SynthEngine;

/// A gain that glides to its target instead of jumping. Track gain now
/// changes continuously (automation, fader drags), but only reaches the
/// audio thread once per UI frame / audio block; applied as a step, each
/// change would click ("zipper"). A one-pole ramp with a ~5 ms time
/// constant removes that without audible lag.
#[derive(Clone, Copy)]
struct SmoothedGain {
    current: f32,
    coeff: f32,
}

impl SmoothedGain {
    fn new(sample_rate: f32) -> Self {
        Self { current: 1.0, coeff: 1.0 - (-1.0 / (0.005 * sample_rate)).exp() }
    }

    /// Jumps straight to `value` (no glide) - for a start with no previous
    /// level to glide from.
    fn reset(&mut self, value: f32) {
        self.current = value;
    }

    fn next(&mut self, target: f32) -> f32 {
        self.current += (target - self.current) * self.coeff;
        self.current
    }
}

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// How many captured input samples can sit unwritten between the input
/// callback and the writer thread - generous relative to a typical block
/// size at common sample rates, so a brief writer-thread stall (e.g. a
/// slow disk) doesn't drop audio.
const CAPTURE_CAPACITY: usize = 1 << 16;

const BEATS_PER_BAR: u64 = 4;
const SIXTEENTHS_PER_BEAT: u64 = 4;
/// Metronome click: a short decaying sine blip, higher-pitched on the
/// downbeat so bar starts are audible over the mix.
const CLICK_HZ_DOWNBEAT: f32 = 1600.0;
const CLICK_HZ_BEAT: f32 = 1000.0;
const CLICK_DECAY_MS: f32 = 15.0;
const CLICK_AMPLITUDE: f32 = 0.3;

/// Owns the live cpal stream(s). Dropping it stops audio. `input_stream`
/// is `None` when no usable input device was found - recording is then
/// simply unavailable, not a startup failure (see `input::start`).
pub struct EngineHandle {
    _stream: cpal::Stream,
    input_stream: Option<cpal::Stream>,
    input: InputPath,
    pub sample_rate: u32,
}

/// What any input stream feeds: the buffers to the WAV writer and the
/// input meter, and the rate the writer should stamp a take with. Kept
/// here so a new input device can be plugged into the same path.
struct InputPath {
    capture: input::SharedProducer<f32>,
    telemetry: input::SharedProducer<shared::recorder::InputTelemetry>,
    record_params: Arc<shared::recorder::RecordParams>,
    rate: Arc<std::sync::atomic::AtomicU32>,
}

impl EngineHandle {
    /// Switches recording to `device` (a name from
    /// `input::available_input_devices`, `None` for the OS default) while
    /// running: closes the current input, opens the new one on the same
    /// buffers. Returns whether an input is open afterwards. Don't call
    /// mid-take - the take would change device (and maybe rate) halfway.
    pub fn switch_input(&mut self, device: Option<&str>) -> bool {
        // The old stream (and its callback's hold on the buffers) goes
        // first, so the new one never contends with it.
        self.input_stream = None;
        let opened = input::start(
            self.sample_rate,
            device,
            self.input.capture.clone(),
            self.input.telemetry.clone(),
            self.input.record_params.clone(),
        );
        match opened {
            Some((stream, rate)) => {
                self.input.rate.store(rate, std::sync::atomic::Ordering::Relaxed);
                self.input_stream = Some(stream);
                true
            }
            None => false,
        }
    }
}

#[derive(Debug)]
pub enum EngineError {
    NoOutputDevice,
    Cpal(CpalError),
    UnsupportedSampleFormat(SampleFormat),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOutputDevice => write!(f, "no default output device"),
            Self::Cpal(e) => write!(f, "cpal error: {e}"),
            Self::UnsupportedSampleFormat(fmt) => write!(f, "unsupported sample format: {fmt}"),
        }
    }
}

impl From<CpalError> for EngineError {
    fn from(e: CpalError) -> Self {
        Self::Cpal(e)
    }
}

impl std::error::Error for EngineError {}

/// Starts the audio engine. `params` is shared with the UI; `telemetry` is
/// the producing end of the engine -> UI ring buffer. `synth_params`/
/// `note_events` feed Carve's voice engine; `synth_telemetry` reports its
/// post-mix peak level back to the UI. `playback_plan`/`decoded_sources`
/// feed the arrangement's audio clips into the mix - see
/// `shared::playback`. `record_commands`/`input_telemetry` drive guitar/
/// mic recording - see `shared::recorder` and `input`. `preferred_input_device`
/// names a specific input device (see `input::available_input_devices`) to
/// open instead of the OS default; `None` (or a name that no longer
/// matches anything) falls back to it.
#[allow(clippy::too_many_arguments)]
pub fn start(
    params: Arc<Params>,
    telemetry: rtrb::Producer<Telemetry>,
    synth_params: rtrb::Consumer<SynthParams>,
    note_events: rtrb::Consumer<NoteEvent>,
    synth_telemetry: rtrb::Producer<SynthTelemetry>,
    playback_plan: rtrb::Consumer<PlaybackPlan>,
    decoded_sources: rtrb::Consumer<DecodedSource>,
    record_commands: rtrb::Consumer<RecordCommand>,
    input_telemetry: rtrb::Producer<shared::recorder::InputTelemetry>,
    record_params: Arc<shared::recorder::RecordParams>,
    preferred_input_device: Option<&str>,
    preview: shared::playback::PreviewEnds,
) -> Result<EngineHandle, EngineError> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or(EngineError::NoOutputDevice)?;
    let config = device.default_output_config()?;
    let sample_rate = config.sample_rate();
    let sample_format = config.sample_format();
    let stream_config: StreamConfig = config.into();

    let stream = match sample_format {
        SampleFormat::F32 => build_stream::<f32>(
            &device,
            stream_config,
            params,
            telemetry,
            synth_params,
            note_events,
            synth_telemetry,
            playback_plan,
            decoded_sources,
            preview,
        )?,
        SampleFormat::I16 => build_stream::<i16>(
            &device,
            stream_config,
            params,
            telemetry,
            synth_params,
            note_events,
            synth_telemetry,
            playback_plan,
            decoded_sources,
            preview,
        )?,
        SampleFormat::U16 => build_stream::<u16>(
            &device,
            stream_config,
            params,
            telemetry,
            synth_params,
            note_events,
            synth_telemetry,
            playback_plan,
            decoded_sources,
            preview,
        )?,
        other => return Err(EngineError::UnsupportedSampleFormat(other)),
    };

    stream.play()?;

    let (capture_tx, capture_rx) = rtrb::RingBuffer::<f32>::new(CAPTURE_CAPACITY);
    let input = InputPath {
        capture: Arc::new(std::sync::Mutex::new(capture_tx)),
        telemetry: Arc::new(std::sync::Mutex::new(input_telemetry)),
        record_params,
        rate: Arc::new(std::sync::atomic::AtomicU32::new(sample_rate)),
    };
    // Always running, even with no input yet: a device can be picked later.
    spawn_writer_thread(capture_rx, record_commands, input.rate.clone());
    let mut handle = EngineHandle { _stream: stream, input_stream: None, input, sample_rate };
    handle.switch_input(preferred_input_device);
    Ok(handle)
}

/// Runs for the lifetime of the process, off the audio thread: streams
/// captured input samples to a WAV file between `RecordCommand::Start`
/// and `Stop`, discarding them (but still draining the ring buffer, so it
/// never backs up) whenever nothing is armed.
fn spawn_writer_thread(
    mut capture_rx: rtrb::Consumer<f32>,
    mut command_rx: rtrb::Consumer<RecordCommand>,
    input_rate: Arc<std::sync::atomic::AtomicU32>,
) {
    std::thread::spawn(move || {
        let mut writer: Option<hound::WavWriter<std::io::BufWriter<std::fs::File>>> = None;
        loop {
            while let Ok(command) = command_rx.pop() {
                match command {
                    RecordCommand::Start { path } => {
                        if let Some(w) = writer.take() {
                            let _ = w.finalize();
                        }
                        // The rate of whichever input is open now.
                        let spec = hound::WavSpec {
                            channels: 1,
                            sample_rate: input_rate.load(std::sync::atomic::Ordering::Relaxed),
                            bits_per_sample: 32,
                            sample_format: hound::SampleFormat::Float,
                        };
                        match hound::WavWriter::create(&path, spec) {
                            Ok(w) => writer = Some(w),
                            Err(e) => eprintln!("recorder: failed to create {}: {e}", path.display()),
                        }
                    }
                    RecordCommand::Stop => {
                        if let Some(w) = writer.take() {
                            if let Err(e) = w.finalize() {
                                eprintln!("recorder: failed to finalize WAV: {e}");
                            }
                        }
                    }
                }
            }

            let mut drained_any = false;
            while let Ok(sample) = capture_rx.pop() {
                drained_any = true;
                if let Some(w) = writer.as_mut() {
                    let _ = w.write_sample(sample);
                }
            }
            if !drained_any {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn build_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    params: Arc<Params>,
    mut telemetry: rtrb::Producer<Telemetry>,
    mut synth_params: rtrb::Consumer<SynthParams>,
    mut note_events: rtrb::Consumer<NoteEvent>,
    mut synth_telemetry: rtrb::Producer<SynthTelemetry>,
    mut playback_plan: rtrb::Consumer<PlaybackPlan>,
    mut decoded_sources: rtrb::Consumer<DecodedSource>,
    mut preview: shared::playback::PreviewEnds,
) -> Result<cpal::Stream, EngineError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate as f32;


    let mut sample_counter: u64 = 0;
    // One Carve per instrument track, all allocated here - before the
    // stream starts - so the audio thread never allocates.
    let mut synth_engines: Vec<SynthEngine> = (0..MAX_INSTRUMENTS).map(|_| SynthEngine::new(sample_rate)).collect();
    // Same slots, for tracks whose instrument is a Drum Kit; `slot_is_drums`
    // says which of the two plays each slot.
    let mut drum_engines: Vec<DrumEngine> = (0..MAX_INSTRUMENTS).map(|_| DrumEngine::new(sample_rate)).collect();
    let mut slot_is_drums = [false; MAX_INSTRUMENTS];
    // Each slot's owning track's mixer gain, as a linear multiplier - unity
    // until the first `SynthParams` snapshot for that slot arrives, so a
    // freshly added track isn't silent before the UI's first tick.
    let mut slot_gain = [1.0f32; MAX_INSTRUMENTS];
    // What each slot's / bus's gain is actually at, gliding towards the
    // latest target (see `SmoothedGain`).
    let mut slot_gain_smooth = [SmoothedGain::new(sample_rate); MAX_INSTRUMENTS];
    let mut bus_gain_smooth = [SmoothedGain::new(sample_rate); MAX_BUS_TRACKS];
    // One persistent effect chain per Carve slot - persistent (not
    // rebuilt per block) because e.g. a compressor's envelope follower
    // needs continuity across blocks to sound like one rather than
    // clicking per block.
    let mut slot_effects: Vec<EffectChain> = (0..MAX_INSTRUMENTS).map(|_| EffectChain::new(sample_rate)).collect();
    // Same, but per audio track (`PlaybackClip::bus_slot`) rather than
    // per Carve slot - audio clips have no engine "slot" of their own
    // otherwise.
    let mut bus_effects: Vec<EffectChain> = (0..MAX_BUS_TRACKS).map(|_| EffectChain::new(sample_rate)).collect();
    // The master bus's own chain - one instance, applied once to the
    // final mix, after every track's own chain/fader, before metering.
    let mut master_effects = EffectChain::new(sample_rate);
    let mut click_phase = 0.0f32;
    let mut click_env = 0.0f32;
    let mut click_hz = CLICK_HZ_BEAT;
    let click_decay_coeff = (-1.0 / (CLICK_DECAY_MS * 0.001 * sample_rate)).exp();

    // The lesson preview playing, if any, and how far into it we are.
    let mut preview_now: Option<(shared::playback::PreviewBuffer, usize)> = None;
    let mut current_plan = PlaybackPlan::default();
    let mut current_sources: Vec<DecodedSource> = Vec::with_capacity(DECODED_SOURCE_CAPACITY);

    let err_fn = |err: CpalError| eprintln!("audio stream error: {err}");

    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [T], _info: &OutputCallbackInfo| {
                // Latest-wins: only the most recent params snapshot matters.
                while let Ok(next) = synth_params.pop() {
                    if let Some(gain) = slot_gain.get_mut(next.slot as usize) {
                        *gain = db_to_gain(next.gain_db);
                    }
                    if let Some(chain) = slot_effects.get_mut(next.slot as usize) {
                        chain.set_state(next.effect_count, &next.effects);
                    }
                    let slot = next.slot as usize;
                    if slot < MAX_INSTRUMENTS && slot_is_drums[slot] != next.drums {
                        // The track switched instruments: silence the old one.
                        let off = NoteEvent { slot: next.slot, note: shared::synth::ALL_NOTES_OFF, on: false, velocity: 0 };
                        synth_engines[slot].handle_note_event(off);
                        drum_engines[slot].handle_note_event(off, &current_sources);
                        slot_is_drums[slot] = next.drums;
                    }
                    if let Some(engine) = synth_engines.get_mut(slot) {
                        engine.set_params(next);
                    }
                }
                // Ordered: every note on/off matters.
                while let Ok(event) = note_events.pop() {
                    let slot = event.slot as usize;
                    if slot >= MAX_INSTRUMENTS {
                        continue;
                    }
                    if slot_is_drums[slot] {
                        drum_engines[slot].handle_note_event(event, &current_sources);
                    } else {
                        synth_engines[slot].handle_note_event(event);
                    }
                }
                // Latest-wins: the clip layout only, not any one sample.
                while let Ok(next) = playback_plan.pop() {
                    current_plan = next;
                    // Config only - not per-sample - since it only needs
                    // to catch up whenever the plan itself changes.
                    for clip in &current_plan.clips {
                        if let Some(chain) = bus_effects.get_mut(clip.bus_slot as usize) {
                            chain.set_state(clip.effect_count, &clip.effects);
                        }
                    }
                    master_effects.set_state(current_plan.master_effect_count, &current_plan.master_effects);
                }
                // Latest-wins: a new preview replaces the one playing (an
                // empty one just stops it). Finished buffers go back to be
                // freed off the audio thread.
                while let Ok(next) = preview.play_rx.pop() {
                    if let Some((old, _)) = preview_now.take() {
                        let _ = preview.retired_tx.push(old);
                    }
                    if next.is_empty() {
                        let _ = preview.retired_tx.push(next);
                    } else {
                        preview_now = Some((next, 0));
                    }
                }
                // Ordered: each newly decoded source matters.
                while let Ok(decoded) = decoded_sources.pop() {
                    if let Some(existing) = current_sources.iter_mut().find(|d| d.source == decoded.source) {
                        *existing = decoded;
                    } else if current_sources.len() < current_sources.capacity() {
                        current_sources.push(decoded);
                    }
                }

                write_block(
                    data,
                    channels,
                    &params,
                    &mut telemetry,
                    &mut sample_counter,
                    sample_rate,
                    &mut synth_engines,
                    &mut drum_engines,
                    &slot_is_drums,
                    &slot_gain,
                    &mut slot_gain_smooth,
                    &mut bus_gain_smooth,
                    &mut slot_effects,
                    &mut bus_effects,
                    &mut master_effects,
                    &mut synth_telemetry,
                    &mut click_phase,
                    &mut click_env,
                    &mut click_hz,
                    click_decay_coeff,
                    &current_plan,
                    &current_sources,
                    &mut preview_now,
                    &mut preview.analyzer_tx,
                );
                if preview_now.as_ref().is_some_and(|(buf, pos)| *pos >= buf.len()) {
                    if let Some((done, _)) = preview_now.take() {
                        let _ = preview.retired_tx.push(done);
                    }
                }
            },
            err_fn,
            None,
        )?;

    Ok(stream)
}

#[allow(clippy::too_many_arguments)]
fn write_block<T>(
    output: &mut [T],
    channels: usize,
    params: &Params,
    telemetry: &mut rtrb::Producer<Telemetry>,
    sample_counter: &mut u64,
    sample_rate: f32,
    synth_engines: &mut [SynthEngine],
    drum_engines: &mut [DrumEngine],
    slot_is_drums: &[bool],
    slot_gain: &[f32],
    slot_gain_smooth: &mut [SmoothedGain],
    bus_gain_smooth: &mut [SmoothedGain],
    slot_effects: &mut [EffectChain],
    bus_effects: &mut [EffectChain],
    master_effects: &mut EffectChain,
    synth_telemetry: &mut rtrb::Producer<SynthTelemetry>,
    click_phase: &mut f32,
    click_env: &mut f32,
    click_hz: &mut f32,
    click_decay_coeff: f32,
    plan: &PlaybackPlan,
    sources: &[DecodedSource],
    preview: &mut Option<(shared::playback::PreviewBuffer, usize)>,
    analyzer: &mut rtrb::Producer<f32>,
) where
    T: Sample + FromSample<f32>,
{
    // CPU load: how much of the block's real-time budget rendering it
    // took. `Instant::now` reads the vDSO clock - no syscall, no lock.
    let started = std::time::Instant::now();
    if params.take_stop_request() {
        *sample_counter = 0;
    }
    let playing = params.playing();
    let click_enabled = params.click_enabled();
    let bpm = params.bpm();
    let samples_per_beat = (sample_rate as f64 * 60.0) / bpm;
    let (loop_enabled, loop_start, loop_end) = params.loop_range();
    let loop_active = loop_enabled && loop_end > loop_start;

    let mut peak_l = 0.0f32;
    let mut peak_r = 0.0f32;
    let mut synth_peaks = [(0.0f32, 0.0f32); MAX_INSTRUMENTS];
    let mut frames = 0u64;

    for frame in output.chunks_mut(channels) {

        let mut synth_l = 0.0f32;
        let mut synth_r = 0.0f32;
        for (i, engine) in synth_engines.iter_mut().enumerate() {
            let (raw_l, raw_r) = if slot_is_drums[i] { drum_engines[i].process(sources) } else { engine.process() };
            // Chain order: instrument -> Compressor insert -> track fader,
            // same as a real device chain (the fader is the last thing
            // before the master sum, not part of the chain itself).
            let (fx_l, fx_r) = slot_effects[i].process(raw_l, raw_r);
            let gain = slot_gain_smooth[i].next(slot_gain[i]);
            let (l, r) = (fx_l * gain, fx_r * gain);
            synth_l += l;
            synth_r += r;
            let peak = &mut synth_peaks[i];
            peak.0 = peak.0.max(l.abs());
            peak.1 = peak.1.max(r.abs());
        }

        if playing && click_enabled {
            let next = *sample_counter + 1;
            let beat_before = (*sample_counter as f64 / samples_per_beat) as u64;
            let beat_after = (next as f64 / samples_per_beat) as u64;
            if beat_after != beat_before {
                *click_env = 1.0;
                *click_phase = 0.0;
                *click_hz = if beat_after % BEATS_PER_BAR == 0 { CLICK_HZ_DOWNBEAT } else { CLICK_HZ_BEAT };
            }
        }
        let click = (*click_phase).sin() * *click_env * CLICK_AMPLITUDE;
        *click_phase += *click_hz * std::f32::consts::TAU / sample_rate;
        if *click_phase >= std::f32::consts::TAU {
            *click_phase -= std::f32::consts::TAU;
        }
        *click_env *= click_decay_coeff;

        let (clip_l, clip_r) = if playing {
            mix_audio_clips(plan, sources, *sample_counter as i64, bus_effects, bus_gain_smooth)
        } else {
            (0.0, 0.0)
        };
        let (mut out_l, mut out_r) = master_effects.process(synth_l + click + clip_l, synth_r + click + clip_r);
        // A lesson preview: already mixed and mastered, added last.
        if let Some((buf, pos)) = preview.as_mut() {
            if *pos + 1 < buf.len() {
                out_l += buf[*pos];
                out_r += buf[*pos + 1];
            }
            *pos += 2;
        }

        peak_l = peak_l.max(out_l.abs());
        peak_r = peak_r.max(out_r.abs());
        // To the live analyzer. Full when it's closed (nobody drains it):
        // the sample is dropped, never waited on.
        let _ = analyzer.push(0.5 * (out_l + out_r));

        if channels == 1 {
            frame[0] = T::from_sample(out_l);
        } else {
            frame[0] = T::from_sample(out_l);
            frame[1] = T::from_sample(out_r);
            for sample in &mut frame[2..] {
                *sample = T::from_sample(0.0f32);
            }
        }

        if playing {
            *sample_counter += 1;
            // Wrap back to the loop start the instant playback reaches its
            // end, so the section repeats seamlessly. Every position-
            // derived thing downstream (the transport readout, the MIDI
            // scheduler, clip playback) reads straight off `sample_counter`
            // with no other persistent state, so jumping it back here is
            // enough - nothing needs telling separately.
            if loop_active && *sample_counter >= loop_end as u64 {
                *sample_counter = loop_start as u64;
            }
        }
        frames += 1;
    }
    let position = position_from_samples(*sample_counter, sample_rate, bpm);
    let budget = frames as f32 / sample_rate;
    let cpu_load = if budget > 0.0 { started.elapsed().as_secs_f32() / budget } else { 0.0 };
    // Best-effort: if the UI hasn't drained recently the ring buffer may be
    // full. Dropping a telemetry frame is harmless; never block.
    let _ = telemetry.push(Telemetry {
        peak_l,
        peak_r,
        position,
        sample_counter: *sample_counter,
        cpu_load,
        block_frames: frames as u32,
    });
    let mut lfo_phases = [(0.0f32, 0.0f32); MAX_INSTRUMENTS];
    for (engine, phases) in synth_engines.iter().zip(lfo_phases.iter_mut()) {
        *phases = engine.lfo_phases();
    }
    let _ = synth_telemetry.push(SynthTelemetry { peaks: synth_peaks, lfo_phases });
}

/// Sums every clip in `plan` that's currently sounding at `pos` (samples
/// since playback started) against its decoded source in `sources`,
/// grouped by owning track (`bus_slot`) - so a track's Compressor sees
/// that track's whole signal, not one clip in isolation - then applies
/// each track's Compressor and gain before adding it into the master
/// sum. A clip whose source hasn't finished decoding yet, or whose
/// source's sample rate doesn't match the engine's output, is silently
/// skipped - no resampling in this pass (see `shared::playback`).
fn mix_audio_clips(
    plan: &PlaybackPlan,
    sources: &[DecodedSource],
    pos: i64,
    bus_effects: &mut [EffectChain],
    bus_gain_smooth: &mut [SmoothedGain],
) -> (f32, f32) {
    let mut bus_raw = [(0.0f32, 0.0f32); MAX_BUS_TRACKS];
    let mut bus_gain_db = [0.0f32; MAX_BUS_TRACKS];
    let mut bus_active = [false; MAX_BUS_TRACKS];

    for clip in &plan.clips {
        if pos < clip.start_sample || pos >= clip.start_sample + clip.length_samples {
            continue;
        }
        let Some(source) = sources.iter().find(|s| s.source == clip.source) else { continue };
        let channels = source.channels.max(1) as i64;
        let frame_index = clip.source_offset_samples as i64 + (pos - clip.start_sample);
        let frame_count = source.samples.len() as i64 / channels;
        if frame_index < 0 || frame_index >= frame_count {
            continue;
        }
        let base = (frame_index * channels) as usize;
        let (l, r) = if source.channels <= 1 {
            let s = source.samples[base];
            (s, s)
        } else {
            let l = source.samples[base];
            let r = source.samples.get(base + 1).copied().unwrap_or(l);
            (l, r)
        };
        let slot = (clip.bus_slot as usize).min(MAX_BUS_TRACKS - 1);
        let clip_gain = db_to_gain(clip.clip_gain_db);
        bus_raw[slot].0 += l * clip_gain;
        bus_raw[slot].1 += r * clip_gain;
        bus_gain_db[slot] = clip.gain_db;
        bus_active[slot] = true;
    }

    let mut out_l = 0.0f32;
    let mut out_r = 0.0f32;
    for slot in 0..MAX_BUS_TRACKS {
        if !bus_active[slot] {
            continue;
        }
        let (raw_l, raw_r) = bus_raw[slot];
        let (fx_l, fx_r) = bus_effects[slot].process(raw_l, raw_r);
        let gain = bus_gain_smooth[slot].next(db_to_gain(bus_gain_db[slot]));
        out_l += fx_l * gain;
        out_r += fx_r * gain;
    }
    (out_l, out_r)
}

fn position_from_samples(sample_counter: u64, sample_rate: f32, bpm: f64) -> Position {
    let samples_per_sixteenth = (sample_rate as f64 * 60.0) / (bpm * SIXTEENTHS_PER_BEAT as f64);
    let sixteenth_index = (sample_counter as f64 / samples_per_sixteenth) as u64;
    let sixteenth_in_beat = sixteenth_index % SIXTEENTHS_PER_BEAT;
    let beat_index = (sixteenth_index / SIXTEENTHS_PER_BEAT) % BEATS_PER_BAR;
    let bar_index = sixteenth_index / (SIXTEENTHS_PER_BEAT * BEATS_PER_BAR);

    Position {
        bar: bar_index as u32 + 1,
        beat: beat_index as u8 + 1,
        sixteenth: sixteenth_in_beat as u8 + 1,
    }
}

#[cfg(test)]
mod gain_smoothing_tests {
    use super::SmoothedGain;

    #[test]
    fn glides_instead_of_jumping_and_settles_quickly() {
        let sr = 48_000.0;
        let mut g = SmoothedGain::new(sr);
        let first = g.next(0.0);
        assert!(first > 0.9, "one sample after a 1.0 -> 0.0 change it must not have jumped (got {first})");
        for _ in 0..(0.03 * sr) as usize {
            g.next(0.0);
        }
        assert!(g.current < 0.01, "should have settled within ~30 ms (got {})", g.current);
    }
}
