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
fn audio_sources(arrangement: &Arrangement) -> HashSet<Arc<str>> {
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
            }
        });
    }
}

fn load_and_build(path: &Path) -> Option<PeakPyramid> {
    let (samples, spec) = decode_wav(path)?;
    Some(PeakPyramid::build_from_interleaved(&samples, spec.channels, spec.sample_rate))
}

/// Reads and fully decodes a WAV to interleaved `f32` samples in -1..1,
/// regardless of the file's own sample format/bit depth.
fn decode_wav(path: &Path) -> Option<(Vec<f32>, hound::WavSpec)> {
    let mut reader = hound::WavReader::open(path).ok()?;
    let spec = reader.spec();

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            reader.samples::<f32>().filter_map(Result::ok).collect()
        }
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
