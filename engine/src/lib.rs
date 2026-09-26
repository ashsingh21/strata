//! The audio engine: Carve's synth voices, the arrangement's audio clips
//! and a metronome click, mixed, with peak metering and transport position reported back through a
//! [`shared::Telemetry`] ring buffer.
//!
//! Everything in the audio callback ([`write_block`]) is allocation-, lock-
//! and syscall-free, with one deliberate, bounded exception: swapping in a
//! new [`PlaybackPlan`] (only on the rare block where the arrangement's
//! clip layout actually changed) drops the previous one's `Vec`. See
//! `shared::playback` for why that's an acceptable trade here.

mod input;
mod dsp;
mod fx;
mod synth;

use std::fmt;
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Error as CpalError, FromSample, OutputCallbackInfo, Sample, SampleFormat, SizedSample, StreamConfig};
use shared::playback::{DecodedSource, PlaybackPlan, DECODED_SOURCE_CAPACITY};
use shared::recorder::RecordCommand;
use shared::synth::{NoteEvent, SynthParams, SynthTelemetry, MAX_INSTRUMENTS};
use shared::{Params, Position, Telemetry};
use synth::SynthEngine;

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

/// Owns the live cpal stream(s). Dropping it stops audio. `_input_stream`
/// is `None` when no usable input device was found - recording is then
/// simply unavailable, not a startup failure (see `input::start`).
pub struct EngineHandle {
    _stream: cpal::Stream,
    _input_stream: Option<cpal::Stream>,
    pub sample_rate: u32,
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
/// mic recording - see `shared::recorder` and `input`.
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
        )?,
        other => return Err(EngineError::UnsupportedSampleFormat(other)),
    };

    stream.play()?;

    let (capture_tx, capture_rx) = rtrb::RingBuffer::<f32>::new(CAPTURE_CAPACITY);
    let input_stream = match input::start(sample_rate, capture_tx, input_telemetry, record_params) {
        Some((stream, input_sample_rate)) => {
            spawn_writer_thread(capture_rx, record_commands, input_sample_rate);
            Some(stream)
        }
        None => None,
    };

    Ok(EngineHandle { _stream: stream, _input_stream: input_stream, sample_rate })
}

/// Runs for the lifetime of the process, off the audio thread: streams
/// captured input samples to a WAV file between `RecordCommand::Start`
/// and `Stop`, discarding them (but still draining the ring buffer, so it
/// never backs up) whenever nothing is armed.
fn spawn_writer_thread(
    mut capture_rx: rtrb::Consumer<f32>,
    mut command_rx: rtrb::Consumer<RecordCommand>,
    sample_rate: u32,
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
                        let spec = hound::WavSpec {
                            channels: 1,
                            sample_rate,
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
    let mut click_phase = 0.0f32;
    let mut click_env = 0.0f32;
    let mut click_hz = CLICK_HZ_BEAT;
    let click_decay_coeff = (-1.0 / (CLICK_DECAY_MS * 0.001 * sample_rate)).exp();

    let mut current_plan = PlaybackPlan::default();
    let mut current_sources: Vec<DecodedSource> = Vec::with_capacity(DECODED_SOURCE_CAPACITY);

    let err_fn = |err: CpalError| eprintln!("audio stream error: {err}");

    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [T], _info: &OutputCallbackInfo| {
                // Latest-wins: only the most recent params snapshot matters.
                while let Ok(next) = synth_params.pop() {
                    if let Some(engine) = synth_engines.get_mut(next.slot as usize) {
                        engine.set_params(next);
                    }
                }
                // Ordered: every note on/off matters.
                while let Ok(event) = note_events.pop() {
                    if let Some(engine) = synth_engines.get_mut(event.slot as usize) {
                        engine.handle_note_event(event);
                    }
                }
                // Latest-wins: the clip layout only, not any one sample.
                while let Ok(next) = playback_plan.pop() {
                    current_plan = next;
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
                    &mut synth_telemetry,
                    &mut click_phase,
                    &mut click_env,
                    &mut click_hz,
                    click_decay_coeff,
                    &current_plan,
                    &current_sources,
                );
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
    synth_telemetry: &mut rtrb::Producer<SynthTelemetry>,
    click_phase: &mut f32,
    click_env: &mut f32,
    click_hz: &mut f32,
    click_decay_coeff: f32,
    plan: &PlaybackPlan,
    sources: &[DecodedSource],
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
        for (engine, peak) in synth_engines.iter_mut().zip(synth_peaks.iter_mut()) {
            let (l, r) = engine.process();
            synth_l += l;
            synth_r += r;
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
            mix_audio_clips(plan, sources, *sample_counter as i64)
        } else {
            (0.0, 0.0)
        };

        let out_l = synth_l + click + clip_l;
        let out_r = synth_r + click + clip_r;

        peak_l = peak_l.max(out_l.abs());
        peak_r = peak_r.max(out_r.abs());

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
    let _ = telemetry.push(Telemetry { peak_l, peak_r, position, cpu_load, block_frames: frames as u32 });
    let mut lfo_phases = [(0.0f32, 0.0f32); MAX_INSTRUMENTS];
    for (engine, phases) in synth_engines.iter().zip(lfo_phases.iter_mut()) {
        *phases = engine.lfo_phases();
    }
    let _ = synth_telemetry.push(SynthTelemetry { peaks: synth_peaks, lfo_phases });
}

/// Sums every clip in `plan` that's currently sounding at `pos` (samples
/// since playback started) against its decoded source in `sources`. A
/// clip whose source hasn't finished decoding yet, or whose source's
/// sample rate doesn't match the engine's output, is silently skipped -
/// no resampling in this pass (see `shared::playback`).
fn mix_audio_clips(plan: &PlaybackPlan, sources: &[DecodedSource], pos: i64) -> (f32, f32) {
    let mut out_l = 0.0f32;
    let mut out_r = 0.0f32;
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
        if source.channels <= 1 {
            let s = source.samples[base];
            out_l += s;
            out_r += s;
        } else {
            let l = source.samples[base];
            let r = source.samples.get(base + 1).copied().unwrap_or(l);
            out_l += l;
            out_r += r;
        }
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
