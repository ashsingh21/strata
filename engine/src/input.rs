//! The guitar/mic input path: opens the default input device, if any, and,
//! real-time safe just like the output stream in `lib.rs`, downmixes each
//! captured frame to mono, pushes it into a ring buffer for the writer
//! thread, and reports a peak level for the UI's input meter.
//!
//! A machine with no input device (or an unusable one) should still run
//! the rest of the app fine, so failures here are logged and degrade to
//! "recording disabled", never a hard error.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Error as CpalError, FromSample, InputCallbackInfo, Sample, SampleFormat, SizedSample, StreamConfig};
use shared::recorder::{InputTelemetry, RecordParams};

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Substrings (lowercased) of ALSA's own virtual/software PCM hints -
/// resamplers, DSP plugins, and the JACK/PulseAudio/PipeWire/OSS bridges
/// - that show up in `input_devices()` alongside every real piece of
/// hardware and would otherwise vastly outnumber it in a picker. Real
/// devices are also enumerated several times over (once per ALSA PCM
/// hint that resolves to the same underlying card), which plain
/// deduplication in `available_input_devices` handles.
const VIRTUAL_DEVICE_KEYWORDS: &[&str] = &[
    "rate converter",
    "jack audio",
    "open sound system",
    "pipewire sound server",
    "pulseaudio sound server",
    "plugin",
    "discard all samples",
    "default alsa output",
    "speex",
];

/// Every real input device's display name the host can currently see,
/// deduplicated and with ALSA's own virtual/software PCMs filtered out -
/// a settings picker's only sane alternative to the OS's raw idea of
/// "default" (see `start`'s `preferred_device`), which silently records
/// from the wrong interface if that's not the one actually wired up (a
/// real case: a webcam mic outranking an audio interface).
pub fn available_input_devices() -> Vec<String> {
    let Ok(devices) = cpal::default_host().input_devices() else { return Vec::new() };
    let mut seen = std::collections::HashSet::new();
    devices
        .filter_map(|d| d.description().ok().map(|desc| desc.name().to_string()))
        .filter(|name| {
            let lower = name.to_lowercase();
            !VIRTUAL_DEVICE_KEYWORDS.iter().any(|k| lower.contains(k))
        })
        .filter(|name| seen.insert(name.clone()))
        .collect()
}

/// Opens an input device, preferring `desired_sample_rate` (the engine's
/// own output rate) when the device supports it so a freshly recorded
/// take doesn't need resampling to play back (see `shared::playback`'s
/// no-resampling limitation). `preferred_device` names a specific device
/// (as returned by `available_input_devices`) to open instead of
/// whatever the OS calls default. ALSA often exposes the *same* physical
/// device under several different hints that all report the identical
/// description name, at noticeably different quality - e.g. one hint
/// for a real interface here negotiated 16-bit samples, another 8-bit
/// (audibly noisy) - so a name match tries every device sharing it and
/// keeps the best-sounding config among them (see
/// `sample_format_quality`), not just the first one that happens not to
/// crash. Falls back to the OS default if no candidate works, or if
/// `preferred_device` is `None`, or no longer matches anything.
/// Returns the live stream plus the rate it actually opened at, or
/// `None` if there's no usable input device.
/// A ring buffer's producing end, shared so it outlives any one input
/// stream: switching devices hands the same buffers to the new stream,
/// and the writer thread / meter on the other ends never notice.
pub type SharedProducer<T> = Arc<Mutex<rtrb::Producer<T>>>;

pub fn start(
    desired_sample_rate: u32,
    preferred_device: Option<&str>,
    capture_tx: SharedProducer<f32>,
    monitor_tx: SharedProducer<f32>,
    telemetry: SharedProducer<InputTelemetry>,
    record_params: Arc<RecordParams>,
) -> Option<(cpal::Stream, u32)> {
    let host = cpal::default_host();
    let named_candidates: Vec<cpal::Device> = preferred_device
        .map(|wanted| {
            host.input_devices()
                .into_iter()
                .flatten()
                .filter(|d| d.description().is_ok_and(|desc| desc.name() == wanted))
                .collect()
        })
        .unwrap_or_default();
    // Keep every candidate that at least builds a usable config, then
    // take the best-sounding one - "first that works" previously landed
    // on a technically-valid but 8-bit-quantized (audibly noisy) hint
    // while a full-resolution one for the very same device sat right
    // next to it in the list.
    let named_pick = named_candidates
        .into_iter()
        .filter_map(|d| {
            let config = build_input_config(&d, desired_sample_rate)?;
            is_usable_sample_format(config.sample_format()).then_some((d, config))
        })
        .max_by_key(|(_, config)| sample_format_quality(config.sample_format()));
    let (device, config) = match named_pick {
        Some(pair) => pair,
        None => {
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
            (device, config)
        }
    };
    let sample_rate = config.sample_rate();
    let channels = config.channels() as usize;
    let sample_format = config.sample_format();
    let stream_config: StreamConfig = config.into();
    // The monitor ring is read at the output's rate: an input that couldn't
    // be opened at it would play back at the wrong pitch, so it isn't fed.
    let can_monitor = sample_rate == desired_sample_rate;
    if !can_monitor {
        eprintln!("input: opened at {sample_rate} Hz, output runs at {desired_sample_rate} Hz; live monitoring unavailable");
    }
    let tx = InputTx { capture: capture_tx, monitor: monitor_tx, telemetry, can_monitor };

    let stream = match sample_format {
        SampleFormat::F32 => {
            build_input_stream::<f32>(&device, stream_config, channels, tx, record_params)
        }
        SampleFormat::I16 => {
            build_input_stream::<i16>(&device, stream_config, channels, tx, record_params)
        }
        SampleFormat::U16 => {
            build_input_stream::<u16>(&device, stream_config, channels, tx, record_params)
        }
        SampleFormat::U8 => {
            build_input_stream::<u8>(&device, stream_config, channels, tx, record_params)
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
/// Whether `build_input_stream` actually has a branch for this format -
/// same check the final `match sample_format` below makes, but usable
/// earlier, while still trying alternate candidates for the same named
/// device (see `start`'s doc comment).
fn is_usable_sample_format(format: SampleFormat) -> bool {
    matches!(format, SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16 | SampleFormat::U8)
}

/// Higher is better-sounding: bit depth (and so quantization noise) is
/// the whole story here, not just "does `build_input_stream` support
/// it" - see `start`'s named-candidate selection.
fn sample_format_quality(format: SampleFormat) -> u8 {
    match format {
        SampleFormat::F32 => 3,
        SampleFormat::I16 => 2,
        SampleFormat::U16 => 1,
        _ => 0,
    }
}

/// Picks the best-*sounding* supported config range covering
/// `desired_sample_rate` - a device offering both, say, 8-bit and
/// 16-bit capture at the same rate lists both ranges, and the first one
/// `supported_input_configs` happens to yield isn't necessarily the
/// good one (confirmed the hard way: recordings this project has always
/// made turned out to be quantized to exactly 40 levels, 8-bit's worth,
/// even ones nobody suspected). Falls back to the device's own default
/// only if nothing covers the desired rate at all.
fn build_input_config(device: &cpal::Device, desired_sample_rate: u32) -> Option<cpal::SupportedStreamConfig> {
    if let Ok(configs) = device.supported_input_configs() {
        let best = configs
            .filter(|range| range.min_sample_rate() <= desired_sample_rate && desired_sample_rate <= range.max_sample_rate())
            .max_by_key(|range| sample_format_quality(range.sample_format()));
        if let Some(range) = best {
            return Some(range.with_sample_rate(desired_sample_rate));
        }
    }
    device.default_input_config().ok()
}

/// Where an input stream's samples go.
struct InputTx {
    capture: SharedProducer<f32>,
    monitor: SharedProducer<f32>,
    telemetry: SharedProducer<InputTelemetry>,
    can_monitor: bool,
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    channels: usize,
    tx: InputTx,
    record_params: Arc<RecordParams>,
) -> Result<cpal::Stream, CpalError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = channels.max(1);
    let err_fn = |err: CpalError| eprintln!("input stream error: {err}");
    let InputTx { capture: capture_tx, monitor: monitor_tx, telemetry, can_monitor } = tx;

    device.build_input_stream(
        config,
        move |data: &[T], _info: &InputCallbackInfo| {
            // Never blocks: the locks are only ever held by this stream's
            // own callback (the old stream is gone before a new one starts),
            // so try_lock only fails in a switch's brief overlap - and then
            // dropping one block is harmless.
            let (Ok(mut capture_tx), Ok(mut telemetry)) = (capture_tx.try_lock(), telemetry.try_lock()) else { return };
            let gain = db_to_gain(record_params.input_gain_db());
            // Fed only while it's being listened to, so the ring never holds stale audio.
            let mut monitor = if can_monitor && record_params.monitoring() { monitor_tx.try_lock().ok() } else { None };
            let mut peak = 0.0f32;
            for frame in data.chunks(channels) {
                // The loudest channel this frame, not the average of all
                // of them - averaging a real signal on one channel with
                // near-silence on an unused one (a common state for a
                // 2-in interface with only one input actually connected)
                // roughly halves the effective level for no reason.
                let loudest = frame
                    .iter()
                    .map(|&s| f32::from_sample(s))
                    .max_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap_or(std::cmp::Ordering::Equal))
                    .unwrap_or(0.0);
                let mono = loudest * gain;
                peak = peak.max(mono.abs());
                let _ = capture_tx.push(mono);
                if let Some(monitor) = monitor.as_mut() {
                    let _ = monitor.push(mono);
                }
            }
            let _ = telemetry.push(InputTelemetry { peak });
        },
        err_fn,
        None,
    )
}
