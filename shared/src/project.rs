//! Whole-project save/load: one JSON file holding the arrangement and
//! Carve's patch. Nothing else - theme, window size, which tool/mode is
//! selected, Interval Input's key/scale, the recorder's input gain - is
//! considered project content; those reset to their defaults on restart
//! the same way they always have.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::arrangement::Arrangement;
use crate::synth::SynthState;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub arrangement: Arrangement,
    pub synth: SynthState,
}

#[derive(Debug)]
pub enum ProjectError {
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Json(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ProjectError {}

impl From<std::io::Error> for ProjectError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for ProjectError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

pub fn save(project: &Project, path: &Path) -> Result<(), ProjectError> {
    let json = serde_json::to_string_pretty(project)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn load(path: &Path) -> Result<Project, ProjectError> {
    let json = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&json)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangement::{seed_arrangement, ClipContent};
    use crate::synth::seed_synth;

    #[test]
    fn round_trips_a_project_through_json() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("strata-project-test-{}.json", std::process::id()));

        let original = Project { arrangement: seed_arrangement(), synth: seed_synth() };
        save(&original, &path).unwrap();
        let loaded = load(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(loaded.arrangement.tracks.len(), original.arrangement.tracks.len());
        assert_eq!(loaded.arrangement.clips.len(), original.arrangement.clips.len());
        assert_eq!(loaded.synth.osc1, original.synth.osc1);
        assert_eq!(loaded.synth.held_notes, original.synth.held_notes);

        // Peaks are never saved - they're rebuilt from the WAV file after
        // load, same as the first time a source is ever referenced.
        for clip in &loaded.arrangement.clips {
            if let ClipContent::Audio { peaks, .. } = &clip.content {
                assert!(peaks.is_none());
            }
        }
    }
}
