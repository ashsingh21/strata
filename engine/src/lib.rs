//! The audio engine: a 220 Hz test tone through a gain/pan stage, driven by
//! [`shared::Params`], with post-fader peak metering and transport position
//! reported back through a [`shared::Telemetry`] ring buffer.
//!
//! Everything in the audio callback ([`write_block`]) is allocation-, lock-
//! and syscall-free: only atomic loads and a lock-free ring-buffer push.

use std::fmt;
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Error as CpalError, FromSample, OutputCallbackInfo, Sample, SampleFormat, SizedSample, StreamConfig};
use shared::{Params, Position, Telemetry};

const TEST_TONE_HZ: f32 = 220.0;
/// -18 dBFS.
const TEST_TONE_AMPLITUDE: f32 = 0.125_892_5;
const BPM: f64 = 128.0;
const BEATS_PER_BAR: u64 = 4;
const SIXTEENTHS_PER_BEAT: u64 = 4;
/// One-pole smoothing time constant for gain/pan, in milliseconds. Short
/// enough to feel immediate, long enough to kill zipper noise.
const SMOOTHING_MS: f32 = 5.0;

/// Owns the live cpal stream. Dropping it stops audio.
pub struct EngineHandle {
    _stream: cpal::Stream,
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
/// the producing end of the engine -> UI ring buffer.
pub fn start(
    params: Arc<Params>,
    telemetry: rtrb::Producer<Telemetry>,
) -> Result<EngineHandle, EngineError> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or(EngineError::NoOutputDevice)?;
    let config = device.default_output_config()?;
    let sample_rate = config.sample_rate();
    let sample_format = config.sample_format();
    let stream_config: StreamConfig = config.into();

    let stream = match sample_format {
        SampleFormat::F32 => build_stream::<f32>(&device, stream_config, params, telemetry)?,
        SampleFormat::I16 => build_stream::<i16>(&device, stream_config, params, telemetry)?,
        SampleFormat::U16 => build_stream::<u16>(&device, stream_config, params, telemetry)?,
        other => return Err(EngineError::UnsupportedSampleFormat(other)),
    };

    stream.play()?;

    Ok(EngineHandle { _stream: stream, sample_rate })
}

fn build_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    params: Arc<Params>,
    mut telemetry: rtrb::Producer<Telemetry>,
) -> Result<cpal::Stream, EngineError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate as f32;

    let mut phase = 0.0f32;
    let phase_step = TEST_TONE_HZ * std::f32::consts::TAU / sample_rate;

    let mut smoothed_gain = params.gain();
    let mut smoothed_pan = params.pan();
    let mut smoothed_play = 0.0f32;
    // One-pole coefficient: how far smoothed value moves toward target per
    // sample. Derived from the desired smoothing time constant.
    let smoothing_coeff = 1.0 - (-1.0 / (SMOOTHING_MS * 0.001 * sample_rate)).exp();

    let mut sample_counter: u64 = 0;

    let err_fn = |err: CpalError| eprintln!("audio stream error: {err}");

    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [T], _info: &OutputCallbackInfo| {
                write_block(
                    data,
                    channels,
                    &params,
                    &mut telemetry,
                    &mut phase,
                    phase_step,
                    &mut smoothed_gain,
                    &mut smoothed_pan,
                    &mut smoothed_play,
                    smoothing_coeff,
                    &mut sample_counter,
                    sample_rate,
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
    phase: &mut f32,
    phase_step: f32,
    smoothed_gain: &mut f32,
    smoothed_pan: &mut f32,
    smoothed_play: &mut f32,
    smoothing_coeff: f32,
    sample_counter: &mut u64,
    sample_rate: f32,
) where
    T: Sample + FromSample<f32>,
{
    if params.take_stop_request() {
        *sample_counter = 0;
    }
    let playing = params.playing();
    let target_gain = params.gain();
    let target_pan = params.pan();

    let mut peak_l = 0.0f32;
    let mut peak_r = 0.0f32;
    let mut frames = 0u64;

    for frame in output.chunks_mut(channels) {
        *smoothed_gain += (target_gain - *smoothed_gain) * smoothing_coeff;
        *smoothed_pan += (target_pan - *smoothed_pan) * smoothing_coeff;
        let play_target = if playing { 1.0 } else { 0.0 };
        *smoothed_play += (play_target - *smoothed_play) * smoothing_coeff;

        let tone = (*phase).sin() * TEST_TONE_AMPLITUDE * *smoothed_play;
        *phase += phase_step;
        if *phase >= std::f32::consts::TAU {
            *phase -= std::f32::consts::TAU;
        }

        // Equal-power pan law: pan in [-1, 1] maps to a quarter turn.
        let angle = (*smoothed_pan + 1.0) * std::f32::consts::FRAC_PI_4;
        let left_gain = angle.cos() * *smoothed_gain;
        let right_gain = angle.sin() * *smoothed_gain;

        let out_l = tone * left_gain;
        let out_r = tone * right_gain;

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
        }
        frames += 1;
    }
    let _ = frames;

    let position = position_from_samples(*sample_counter, sample_rate);
    // Best-effort: if the UI hasn't drained recently the ring buffer may be
    // full. Dropping a telemetry frame is harmless; never block.
    let _ = telemetry.push(Telemetry { peak_l, peak_r, position });
}

fn position_from_samples(sample_counter: u64, sample_rate: f32) -> Position {
    let samples_per_sixteenth = (sample_rate as f64 * 60.0) / (BPM * SIXTEENTHS_PER_BEAT as f64);
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
