//! Background WAV decoding and peak-pyramid building. Runs on a spawned
//! thread via `Context::spawn` (never the UI or audio thread) and reports
//! back through a `ContextProxy`, which is the `Send`-safe handle Vizia
//! gives a background thread for emitting events into the app.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use vizia::prelude::*;

use shared::arrangement::{Arrangement, ClipContent, PeakPyramid};
use shared::playback::DecodedSource;

use crate::timeline::state::TimelineEvent;

/// Every unique audio source referenced by `arrangement`'s clips.
pub(crate) fn audio_sources(arrangement: &Arrangement) -> HashSet<Arc<str>> {
    arrangement
        .clips
        .iter()
        .filter_map(|clip| match &clip.content {
            ClipContent::Audio { source, .. } => Some(source.clone()),
            ClipContent::Midi { .. } => None,
        })
        .collect()
}

/// Spawns one background loader per unique audio source referenced by
/// `arrangement`'s clips.
pub fn spawn_peak_loaders(cx: &Context, assets_dir: &Path, arrangement: &Arrangement) {
    for source in audio_sources(arrangement) {
        let path = assets_dir.join(&*source);
        cx.spawn(move |proxy| {
            if let Some(pyramid) = load_and_build(&path) {
                let _ = proxy.emit(TimelineEvent::PeaksLoaded { source, peaks: Arc::new(pyramid) });
            } else {
                eprintln!("timeline: failed to load {}", path.display());
                let _ = proxy.emit(TimelineEvent::SourceMissing(source));
            }
        });
    }
}

/// Spawns a loader for exactly one source - used when a single new clip
/// (a freshly recorded take, or an imported drum sample) is added after
/// startup, rather than the whole-arrangement scan `spawn_peak_loaders`
/// does. Takes `&mut EventContext`, which has its own `.spawn` (same
/// shape as `Context::spawn`), since that's what's available from inside
/// a `Model`'s event handler.
pub fn spawn_peak_loader_for_source(cx: &mut EventContext, assets_dir: &Path, source: Arc<str>) {
    let path = assets_dir.join(&*source);
    cx.spawn(move |proxy| {
        if let Some(pyramid) = load_and_build(&path) {
            let _ = proxy.emit(TimelineEvent::PeaksLoaded { source, peaks: Arc::new(pyramid) });
        } else {
            eprintln!("timeline: failed to load {}", path.display());
            let _ = proxy.emit(TimelineEvent::SourceMissing(source));
        }
    });
}

/// Just a file's duration - for sizing a freshly imported sample's clip
/// to its real length without decoding every sample (peak-building and
/// full decode both happen separately, in the background).
pub fn wav_duration_seconds(path: &Path) -> Option<f64> {
    let reader = hound::WavReader::open(path).ok()?;
    let spec = reader.spec();
    Some(reader.duration() as f64 / spec.sample_rate as f64)
}

fn load_and_build(path: &Path) -> Option<PeakPyramid> {
    let (samples, spec) = decode_wav(path)?;
    Some(PeakPyramid::build_from_interleaved(&samples, spec.channels, spec.sample_rate))
}

/// Reads and fully decodes a WAV to interleaved `f32` samples in -1..1,
/// regardless of the file's own sample format/bit depth.
///
/// A source requested right after a take stops (`spawn_peak_loader_for_source`,
/// the decode worker) races the recorder's writer thread: `Stop` is only a
/// message on a ring buffer, and until that thread actually calls
/// `WavWriter::finalize()` the header on disk still declares a 0-sample
/// data chunk, so `hound` opens the file fine but yields no samples at
/// all - not an error, just silently empty, and (since nothing re-requests
/// it) permanently so until the next full reload. So: retry briefly
/// whenever a read comes back with zero samples, rather than trusting the
/// first attempt. A file that's genuinely empty (or missing) just burns
/// this budget and still returns empty, same as before.
pub fn decode_wav(path: &Path) -> Option<(Vec<f32>, hound::WavSpec)> {
    for attempt in 0..25 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let Ok(mut reader) = hound::WavReader::open(path) else { continue };
        let spec = reader.spec();
        if reader.duration() == 0 {
            continue;
        }

        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => {
                reader.samples::<f32>().filter_map(Result::ok).collect()
            }
            hound::SampleFormat::Int => {
                let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
                reader.samples::<i32>().filter_map(Result::ok).map(|s| s as f32 / max).collect()
            }
        };
        return Some((samples, spec));
    }
    // Ran out of retries: fall back to whatever a last, un-retried open
    // reports (a genuinely empty/missing file), so callers still get their
    // usual None on a real failure instead of this function looping forever.
    let mut reader = hound::WavReader::open(path).ok()?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().filter_map(Result::ok).map(|s| s as f32 / max).collect()
        }
    };
    Some((samples, spec))
}

/// Spawns one persistent background thread (never the UI or audio
/// thread) that fully decodes whatever audio source names are sent to
/// it and pushes each result to the engine via `decode_tx` - see
/// `shared::playback`. One long-lived thread, not one per source or per
/// call, because `rtrb::Producer` is single-producer: only one thread
/// can ever hold `decode_tx`. Returns the sending half; call it once at
/// startup with the initial arrangement's sources, then keep the sender
/// around to request a freshly recorded clip's source later (see
/// `crate::recorder::RecordingCoordinator`).
pub fn spawn_audio_decoder_worker(
    assets_dir: &Path,
    arrangement: &Arrangement,
    mut decode_tx: rtrb::Producer<DecodedSource>,
) -> std::sync::mpsc::Sender<Arc<str>> {
    let (request_tx, request_rx) = std::sync::mpsc::channel::<Arc<str>>();
    for source in audio_sources(arrangement) {
        let _ = request_tx.send(source);
    }

    let assets_dir = assets_dir.to_path_buf();
    std::thread::spawn(move || {
        for source in request_rx {
            let path = assets_dir.join(&*source);
            match decode_wav(&path) {
                Some((samples, spec)) => {
                    let decoded = DecodedSource {
                        source,
                        sample_rate: spec.sample_rate,
                        channels: spec.channels,
                        samples: Arc::from(samples),
                    };
                    let _ = decode_tx.push(decoded);
                }
                None => eprintln!("playback: failed to decode {}", path.display()),
            }
        }
    });

    request_tx
}
