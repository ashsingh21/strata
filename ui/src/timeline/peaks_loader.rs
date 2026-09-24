//! Background WAV decoding and peak-pyramid building. Runs on a spawned
//! thread via `Context::spawn` (never the UI or audio thread) and reports
//! back through a `ContextProxy`, which is the `Send`-safe handle Vizia
//! gives a background thread for emitting events into the app.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vizia::prelude::*;

use shared::arrangement::{Arrangement, ClipContent, PeakPyramid};

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

fn load_and_build(path: &PathBuf) -> Option<PeakPyramid> {
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

    Some(PeakPyramid::build_from_interleaved(&samples, spec.channels, spec.sample_rate))
}
