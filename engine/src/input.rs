//! The guitar/mic input path: opens the default input device, if any, and,
//! real-time safe just like the output stream in `lib.rs`, downmixes each
//! captured frame to mono, pushes it into a ring buffer for the writer
//! thread, and reports a peak level for the UI's input meter.
//!
//! A machine with no input device (or an unusable one) should still run
//! the rest of the app fine, so failures here are logged and degrade to
//! "recording disabled", never a hard error.

use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Error as CpalError, FromSample, InputCallbackInfo, Sample, SampleFormat, SizedSample, StreamConfig};
use shared::recorder::{InputTelemetry, RecordParams};

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Opens the default input device, preferring `desired_sample_rate` (the
/// engine's own output rate) when the device supports it so a freshly
/// recorded take doesn't need resampling to play back (see
/// `shared::playback`'s no-resampling limitation). Returns the live
/// stream plus the rate it actually opened at, or `None` if there's no
/// usable input device.
pub fn start(
    desired_sample_rate: u32,
    capture_tx: rtrb::Producer<f32>,
    telemetry: rtrb::Producer<InputTelemetry>,
    record_params: Arc<RecordParams>,
) -> Option<(cpal::Stream, u32)> {
    let host = cpal::default_host();
    let device = host.default_input_device().or_else(|| {
        eprintln!("input: no default input device; recording disabled");
        None
    })?;
    let config = match build_input_config(&device, desired_sample_rate) {
        Some(config) => config,
        None => {
            eprintln!("input: no usable input config; recording disabled");
            return None;
        }
    };
    let sample_rate = config.sample_rate();
    let channels = config.channels() as usize;
    let sample_format = config.sample_format();
    let stream_config: StreamConfig = config.into();

    let stream = match sample_format {
        SampleFormat::F32 => {
            build_input_stream::<f32>(&device, stream_config, channels, capture_tx, telemetry, record_params)
        }
        SampleFormat::I16 => {
            build_input_stream::<i16>(&device, stream_config, channels, capture_tx, telemetry, record_params)
        }
        SampleFormat::U16 => {
            build_input_stream::<u16>(&device, stream_config, channels, capture_tx, telemetry, record_params)
        }
        SampleFormat::U8 => {
            build_input_stream::<u8>(&device, stream_config, channels, capture_tx, telemetry, record_params)
        }
        other => {
            eprintln!("input: unsupported sample format {other}; recording disabled");
            return None;
        }
    };

    let stream = match stream {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("input: failed to build stream: {e}");
            return None;
        }
    };

    if let Err(e) = stream.play() {
        eprintln!("input: failed to start stream: {e}");
        return None;
    }

    Some((stream, sample_rate))
}

/// Picks the first supported config range covering `desired_sample_rate`,
/// falling back to the device's own default if none does.
fn build_input_config(device: &cpal::Device, desired_sample_rate: u32) -> Option<cpal::SupportedStreamConfig> {
    if let Ok(configs) = device.supported_input_configs() {
        for range in configs {
            if range.min_sample_rate() <= desired_sample_rate && desired_sample_rate <= range.max_sample_rate() {
                return Some(range.with_sample_rate(desired_sample_rate));
            }
        }
    }
    device.default_input_config().ok()
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    channels: usize,
    mut capture_tx: rtrb::Producer<f32>,
    mut telemetry: rtrb::Producer<InputTelemetry>,
    record_params: Arc<RecordParams>,
) -> Result<cpal::Stream, CpalError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = channels.max(1);
    let err_fn = |err: CpalError| eprintln!("input stream error: {err}");

    device.build_input_stream(
        config,
        move |data: &[T], _info: &InputCallbackInfo| {
            let gain = db_to_gain(record_params.input_gain_db());
            let mut peak = 0.0f32;
            for frame in data.chunks(channels) {
                let sum: f32 = frame.iter().map(|&s| f32::from_sample(s)).sum();
                let mono = (sum / frame.len() as f32) * gain;
                peak = peak.max(mono.abs());
                let _ = capture_tx.push(mono);
            }
            let _ = telemetry.push(InputTelemetry { peak });
        },
        err_fn,
        None,
    )
}
