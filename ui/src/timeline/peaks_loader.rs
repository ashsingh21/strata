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

/// Spawns one background loader per unique audio source referenced by
/// `arrangement`'s clips.
pub fn spawn_peak_loaders(cx: &Context, assets_dir: &Path, arrangement: &Arrangement) {
    let sources: HashSet<Arc<str>> = arrangement
        .clips
        .iter()
        .filter_map(|clip| match &clip.content {
            ClipContent::Audio { source, .. } => Some(source.clone()),
            ClipContent::Midi { .. } => None,
        })
        .collect();

    for source in sources {
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

/// Spawns one background loader per unique audio source referenced by
/// `arrangement`'s clips that fully decodes the source and pushes it to
/// the engine for playback, via `decode_tx` - see `shared::playback`.
/// Sequential (one thread, not one per source) since `rtrb::Producer` is
/// single-producer; decoding is fast enough that this isn't a concern at
/// this project's scale.
pub fn spawn_audio_decoders(
    cx: &Context,
    assets_dir: &Path,
    arrangement: &Arrangement,
    mut decode_tx: rtrb::Producer<DecodedSource>,
) {
    let sources: HashSet<Arc<str>> = arrangement
        .clips
        .iter()
        .filter_map(|clip| match &clip.content {
            ClipContent::Audio { source, .. } => Some(source.clone()),
            ClipContent::Midi { .. } => None,
        })
        .collect();
    if sources.is_empty() {
        return;
    }

    let assets_dir = assets_dir.to_path_buf();
    cx.spawn(move |_proxy| {
        for source in sources {
            let path = assets_dir.join(&*source);
            match decode_wav(&path) {
                Some((samples, spec)) => {
                    let decoded = DecodedSource {
                        source: source.clone(),
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
}
